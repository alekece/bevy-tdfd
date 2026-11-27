//! Farming: the farmer, the farmer rabbits, seeds and mana.
//!
//! Owns the farmer's orders (walk, plant, harvest, cast Growth Boost), mana, the seed stock and the
//! farmer rabbits that harvest ripe crops on their own. The farmer plants the crop recipe the level
//! names as its seed; harvests go into the economy's [`Inventory`], and eating (`E`) consumes the
//! first edible item for its recipe's mana bonus.
//!
//! The farmer and rabbits decide in [`GameSet::Decide`] by writing an [`Intent`] that movement carries
//! out, and work in [`GameSet::Act`] after [`GrowthSet`], so a crop ripening this tick can be
//! harvested this tick. Rabbits flee anything whose [`Tags`] match their configured `flee_from`.
//! Starting seeds come from the selected difficulty.

use std::time::Duration;

use bevy::prelude::*;
use serde::Deserialize;

use crate::content::{Crop, RecipeVisuals, Recipes};
use crate::economy::Inventory;
use crate::game::{AppState, DifficultySettings, GameSet, GroundPosition, SelectedDifficulty};
use crate::growth::{Boosted, Growth, GrowthSet, Yield};
use crate::map::{MapLayout, PlantPlot};
use crate::movement::{Intent, Speed};
use crate::stage::{CursorGround, Field};
use crate::targeting::{Tags, Targets};

/// How close a rabbit gets to a crop or its home to work there, in world units.
const REACH: f32 = 0.6;
/// Slack on arrival checks, so stopping exactly at a distance still counts, in world units.
const ARRIVAL_TOLERANCE: f32 = 0.01;
/// How far from a plot's center a click or a crop still counts as on the plot, in world units.
const PLOT_RADIUS: f32 = 0.8;
/// Distance between rabbits at their home, in world units.
const RABBIT_SPACING: f32 = 0.8;
const FARMER_COLOR: Color = Color::srgb(0.85, 0.55, 0.3);
const RABBIT_COLOR: Color = Color::srgb(0.95, 0.95, 0.9);

/// Farming values of a level.
#[derive(Resource, Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FarmingConfig {
    /// Id of the crop recipe the farmer plants.
    pub seed: String,
    /// Time the farmer spends planting or harvesting.
    #[serde(with = "humantime_serde")]
    pub action_time: Duration,
    /// Farmer walking speed, in world units per second.
    pub farmer_speed: f32,
    pub rabbits: RabbitConfig,
    pub mana: ManaConfig,
    pub growth_boost: GrowthBoost,
}

/// Farmer rabbit values.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RabbitConfig {
    pub count: u32,
    /// Walking speed, in world units per second.
    pub speed: f32,
    #[serde(with = "humantime_serde")]
    pub harvest_time: Duration,
    /// Rabbits run from what they flee closer than this, in world units.
    pub flee_radius: f32,
    /// What rabbits run from.
    pub flee_from: Targets,
}

/// The farmer's mana pool.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManaConfig {
    pub max: f32,
    /// Mana regained per second.
    pub regeneration: f32,
}

/// The Growth Boost spell: crops around the farmer grow faster for a while.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthBoost {
    pub mana_cost: f32,
    #[serde(with = "humantime_serde")]
    pub cast_time: Duration,
    /// Radius of the affected area around the farmer, in world units.
    pub radius: f32,
    pub growth_multiplier: f32,
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
}

/// Seeds the farmer can plant.
#[derive(Resource, Debug, Default)]
pub struct SeedStock {
    seeds: u32,
}

impl SeedStock {
    pub fn seeds(&self) -> u32 {
        self.seeds
    }

    /// Takes one seed if there is any left.
    fn take_one(&mut self) -> bool {
        let Some(seeds) = self.seeds.checked_sub(1) else {
            return false;
        };

        self.seeds = seeds;

        true
    }
}

/// The player's farmer.
#[derive(Component, Debug)]
#[require(FarmerOrder, Intent)]
pub struct Farmer;

/// The farmer's mana.
#[derive(Component, Debug)]
pub struct Mana {
    current: f32,
    max: f32,
}

impl Mana {
    fn full(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn max(&self) -> f32 {
        self.max
    }

    fn can_spend(&self, cost: f32) -> bool {
        self.current >= cost
    }

    fn try_spend(&mut self, cost: f32) -> bool {
        if !self.can_spend(cost) {
            return false;
        }

        self.current -= cost;

        true
    }

    fn regenerate(&mut self, amount: f32) {
        self.current = (self.current + amount).min(self.max);
    }
}

/// Temporarily multiplies mana regeneration, after eating.
#[derive(Component, Debug)]
struct RegenerationBonus {
    multiplier: f32,
    timer: Timer,
}

/// What the farmer is doing.
#[derive(Component, Debug, Default)]
enum FarmerOrder {
    #[default]
    Idle,
    Walk {
        to: Vec2,
        then: Option<FarmerAction>,
    },
    Work {
        action: FarmerAction,
        timer: Timer,
    },
}

impl FarmerOrder {
    /// Replaces the current order, unless the farmer is in the middle of an action. Returns whether
    /// the new order was accepted.
    fn replace(&mut self, order: FarmerOrder) -> bool {
        if matches!(self, FarmerOrder::Work { .. }) {
            return false;
        }

        *self = order;

        true
    }
}

/// An action that takes the farmer's time.
#[derive(Debug, Clone, Copy, PartialEq)]
enum FarmerAction {
    Plant { plot: Vec2 },
    Harvest { crop: Entity },
    CastGrowthBoost,
}

impl FarmerAction {
    fn duration(self, config: &FarmingConfig) -> Duration {
        match self {
            FarmerAction::Plant { .. } | FarmerAction::Harvest { .. } => config.action_time,
            FarmerAction::CastGrowthBoost => config.growth_boost.cast_time,
        }
    }

    fn start(self, config: &FarmingConfig) -> FarmerOrder {
        FarmerOrder::Work {
            action: self,
            timer: Timer::new(self.duration(config), TimerMode::Once),
        }
    }
}

/// A farmer rabbit that harvests ripe crops on its own.
#[derive(Component, Debug)]
#[require(RabbitTask, Intent)]
pub struct FarmerRabbit {
    home: Vec2,
}

#[derive(Component, Debug, Default)]
enum RabbitTask {
    #[default]
    Idle,
    Harvest {
        crop: Entity,
        work: Timer,
    },
}

impl RabbitTask {
    fn crop(&self) -> Option<Entity> {
        match self {
            RabbitTask::Harvest { crop, .. } => Some(*crop),
            RabbitTask::Idle => None,
        }
    }
}

/// The farmer eats the first edible item of the inventory to boost mana regeneration.
#[derive(Event, Debug)]
pub struct Eat;

/// Meshes and materials shared by the farmer and the rabbits.
#[derive(Resource, Debug)]
struct FarmingAssets {
    farmer_mesh: Handle<Mesh>,
    farmer_material: Handle<StandardMaterial>,
    rabbit_mesh: Handle<Mesh>,
    rabbit_material: Handle<StandardMaterial>,
}

impl FromWorld for FarmingAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let farmer_mesh = meshes.add(Capsule3d::new(0.3, 0.7));
        let rabbit_mesh = meshes.add(Capsule3d::new(0.18, 0.3));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();

        Self {
            farmer_mesh,
            farmer_material: materials.add(FARMER_COLOR),
            rabbit_mesh,
            rabbit_material: materials.add(RABBIT_COLOR),
        }
    }
}

/// Runs the farm: the farmer, farmer rabbits, seeds and mana.
pub struct FarmingPlugin;

impl Plugin for FarmingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SeedStock>()
            .init_resource::<FarmingAssets>()
            .add_observer(eat)
            .add_systems(OnEnter(AppState::InLevel), start_farm)
            .add_systems(
                Update,
                (order_farmer, cast_growth_boost, request_eating).in_set(GameSet::Input),
            )
            .add_systems(
                FixedUpdate,
                (
                    (steer_farmer, steer_rabbits).in_set(GameSet::Decide),
                    (work_farmer, harvest_with_rabbits)
                        .chain()
                        .in_set(GameSet::Act)
                        .after(GrowthSet),
                    regenerate_mana.in_set(GameSet::Act),
                ),
            );
    }
}

fn start_farm(
    mut commands: Commands,
    config: Res<FarmingConfig>,
    layout: Res<MapLayout>,
    difficulties: Res<DifficultySettings>,
    difficulty: Res<SelectedDifficulty>,
    assets: Res<FarmingAssets>,
) {
    let starting = difficulties.starting_resources(difficulty.0);

    commands.insert_resource(SeedStock {
        seeds: starting.seeds,
    });

    let first_plot = layout.plots.first().copied();

    commands.spawn((
        Name::new("Farmer"),
        Farmer,
        Speed(config.farmer_speed),
        Mana::full(config.mana.max),
        first_plot.map_or_else(FarmerOrder::default, |plot| FarmerOrder::Walk {
            to: plot,
            then: Some(FarmerAction::Plant { plot }),
        }),
        Mesh3d(assets.farmer_mesh.clone()),
        MeshMaterial3d(assets.farmer_material.clone()),
        Transform::from_translation(layout.farmer_start.on_ground()),
        DespawnOnExit(AppState::InLevel),
    ));

    for index in 0..config.rabbits.count {
        let home = layout.rabbit_home + Vec2::X * (index as f32 * RABBIT_SPACING);

        commands.spawn((
            Name::new("Farmer rabbit"),
            FarmerRabbit { home },
            Speed(config.rabbits.speed),
            Mesh3d(assets.rabbit_mesh.clone()),
            MeshMaterial3d(assets.rabbit_material.clone()),
            Transform::from_translation(home.on_ground()),
            DespawnOnExit(AppState::InLevel),
        ));
    }
}

/// Right click: walk there, and plant or harvest when clicking a plot.
fn order_farmer(
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorGround>,
    seeds: Res<SeedStock>,
    farmer: Single<&mut FarmerOrder, With<Farmer>>,
    plots: Query<&Transform, With<PlantPlot>>,
    crops: Query<(Entity, &Transform, &Growth), With<Crop>>,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    let Some(target) = cursor.0 else {
        return;
    };

    let clicked_plot = plots
        .iter()
        .map(|plot| plot.translation.xz())
        .find(|plot| plot.distance(target) < PLOT_RADIUS);

    let order = match clicked_plot {
        None => FarmerOrder::Walk {
            to: target,
            then: None,
        },
        Some(plot) => {
            let crop = crops
                .iter()
                .find(|(_, crop, _)| crop.translation.xz().distance(plot) < PLOT_RADIUS);

            let action = match crop {
                Some((crop, _, growth)) if growth.is_ripe() => Some(FarmerAction::Harvest { crop }),
                Some(_) => None,
                None if seeds.seeds() > 0 => Some(FarmerAction::Plant { plot }),
                None => {
                    info!("No seed left to plant");
                    None
                }
            };

            FarmerOrder::Walk {
                to: plot,
                then: action,
            }
        }
    };

    if !farmer.into_inner().replace(order) {
        info!("The farmer is busy");
    }
}

/// `G`: cast Growth Boost around the farmer.
fn cast_growth_boost(
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<FarmingConfig>,
    farmer: Single<(&mut FarmerOrder, &Mana), With<Farmer>>,
) {
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }

    let (mut order, mana) = farmer.into_inner();

    if !mana.can_spend(config.growth_boost.mana_cost) {
        info!("Not enough mana for Growth Boost");

        return;
    }

    if !order.replace(FarmerAction::CastGrowthBoost.start(&config)) {
        info!("The farmer is busy");
    }
}

/// `E`: eat something.
fn request_eating(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyE) {
        commands.trigger(Eat);
    }
}

fn eat(
    _: On<Eat>,
    mut commands: Commands,
    recipes: Res<Recipes>,
    mut inventory: ResMut<Inventory>,
    farmer: Query<Entity, With<Farmer>>,
) {
    let Ok(farmer) = farmer.single() else {
        return;
    };

    let edible = inventory.iter().find_map(|(item, _)| {
        let effect = recipes.crops.get(item)?.eat?;

        Some((item.to_owned(), effect))
    });

    let Some((item, effect)) = edible else {
        info!("Nothing to eat");

        return;
    };

    inventory.take(&item);
    commands.entity(farmer).insert(RegenerationBonus {
        multiplier: effect.regeneration_multiplier,
        timer: Timer::new(effect.duration, TimerMode::Once),
    });
}

/// Walks the farmer towards the destination of its order.
fn steer_farmer(mut farmer: Query<(&FarmerOrder, &mut Intent), With<Farmer>>) {
    for (order, mut intent) in &mut farmer {
        let wanted = match order {
            FarmerOrder::Walk { to, .. } => Intent::MoveTo(*to),
            FarmerOrder::Idle | FarmerOrder::Work { .. } => Intent::Idle,
        };

        intent.set_if_neq(wanted);
    }
}

/// Each rabbit flees, approaches the nearest unclaimed ripe crop, or heads home.
fn steer_rabbits(
    config: Res<FarmingConfig>,
    dangers: Query<(&Tags, &Transform)>,
    crops: Query<(Entity, &Transform, &Growth), With<Crop>>,
    mut rabbits: Query<(&FarmerRabbit, &Transform, &mut RabbitTask, &mut Intent)>,
    mut claimed_crops: Local<Vec<Entity>>,
) {
    let rabbit_config = &config.rabbits;

    claimed_crops.clear();
    claimed_crops.extend(rabbits.iter().filter_map(|(_, _, task, _)| task.crop()));

    for (rabbit, transform, mut task, mut intent) in &mut rabbits {
        let position = transform.translation.xz();
        let nearest_danger = dangers
            .iter()
            .filter(|(tags, _)| rabbit_config.flee_from.matches(tags))
            .map(|(_, danger)| danger.translation.xz())
            .filter(|danger| danger.distance(position) < rabbit_config.flee_radius)
            .min_by(|a, b| a.distance(position).total_cmp(&b.distance(position)));

        if let Some(danger) = nearest_danger {
            let away = (position - danger).normalize_or(Vec2::X);

            *task = RabbitTask::Idle;
            intent.set_if_neq(Intent::MoveTo(Field::clamp(
                position + away * rabbit_config.flee_radius,
            )));

            continue;
        }

        if let RabbitTask::Harvest { crop, .. } = *task {
            let still_ripe = crops.get(crop).is_ok_and(|(_, _, growth)| growth.is_ripe());

            if still_ripe {
                intent.set_if_neq(Intent::Approach {
                    target: crop,
                    within: REACH,
                });

                continue;
            }

            *task = RabbitTask::Idle;
        }

        let nearest_ripe_crop = crops
            .iter()
            .filter(|(crop, _, growth)| growth.is_ripe() && !claimed_crops.contains(crop))
            .min_by(|(_, a, _), (_, b, _)| {
                let a = a.translation.xz().distance(position);
                let b = b.translation.xz().distance(position);

                a.total_cmp(&b)
            });

        if let Some((crop, _, _)) = nearest_ripe_crop {
            claimed_crops.push(crop);
            *task = RabbitTask::Harvest {
                crop,
                work: Timer::new(rabbit_config.harvest_time, TimerMode::Once),
            };
            intent.set_if_neq(Intent::Approach {
                target: crop,
                within: REACH,
            });

            continue;
        }

        let wanted = if position.distance(rabbit.home) > REACH {
            Intent::MoveTo(rabbit.home)
        } else {
            Intent::Idle
        };

        intent.set_if_neq(wanted);
    }
}

fn work_farmer(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<FarmingConfig>,
    recipes: Res<Recipes>,
    visuals: Res<RecipeVisuals>,
    mut seeds: ResMut<SeedStock>,
    mut inventory: ResMut<Inventory>,
    farmer: Single<(&Transform, &mut FarmerOrder, &mut Mana), With<Farmer>>,
    plots: Query<&Transform, With<PlantPlot>>,
    mut crops: Query<(Entity, &Transform, &mut Growth, &Yield), With<Crop>>,
) {
    let (transform, mut order, mut mana) = farmer.into_inner();
    let position = transform.translation.xz();

    let finished_action = match &mut *order {
        FarmerOrder::Idle => None,
        FarmerOrder::Walk { to, then } => {
            if position.distance(*to) > ARRIVAL_TOLERANCE {
                return;
            }

            *order = then
                .take()
                .map_or(FarmerOrder::Idle, |action| action.start(&config));

            None
        }
        FarmerOrder::Work { action, timer } => {
            timer.tick(time.delta()).is_finished().then_some(*action)
        }
    };

    let Some(action) = finished_action else {
        return;
    };

    *order = FarmerOrder::Idle;

    match action {
        FarmerAction::Plant { plot } => {
            let plot_exists = plots
                .iter()
                .any(|candidate| candidate.translation.xz().distance(plot) < PLOT_RADIUS);
            let occupied = crops
                .iter()
                .any(|(_, crop, ..)| crop.translation.xz().distance(plot) < PLOT_RADIUS);

            if !plot_exists || occupied || seeds.seeds() == 0 {
                return;
            }

            if recipes
                .spawn_crop(&mut commands, &visuals, &config.seed, plot)
                .is_some()
            {
                seeds.take_one();
            }
        }
        FarmerAction::Harvest { crop } => {
            if let Ok((_, _, mut growth, harvest)) = crops.get_mut(crop)
                && growth.harvest()
            {
                inventory.add(&harvest.item, harvest.amount);
            }
        }
        FarmerAction::CastGrowthBoost => {
            if !mana.try_spend(config.growth_boost.mana_cost) {
                return;
            }

            let boost = &config.growth_boost;

            for (crop, crop_transform, ..) in &crops {
                if crop_transform.translation.xz().distance(position) <= boost.radius {
                    commands
                        .entity(crop)
                        .insert(Boosted::new(boost.growth_multiplier, boost.duration));
                }
            }
        }
    }
}

fn harvest_with_rabbits(
    time: Res<Time>,
    mut inventory: ResMut<Inventory>,
    mut rabbits: Query<(&Transform, &mut RabbitTask), With<FarmerRabbit>>,
    mut crops: Query<(&Transform, &mut Growth, &Yield), (With<Crop>, Without<FarmerRabbit>)>,
) {
    for (transform, mut task) in &mut rabbits {
        let RabbitTask::Harvest { crop, work } = &mut *task else {
            continue;
        };

        let Ok((crop_transform, mut growth, harvest)) = crops.get_mut(*crop) else {
            continue;
        };

        let distance = transform
            .translation
            .xz()
            .distance(crop_transform.translation.xz());

        if distance > REACH + ARRIVAL_TOLERANCE || !work.tick(time.delta()).is_finished() {
            continue;
        }

        if growth.harvest() {
            inventory.add(&harvest.item, harvest.amount);
        }

        *task = RabbitTask::Idle;
    }
}

fn regenerate_mana(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<FarmingConfig>,
    mut farmers: Query<(Entity, &mut Mana, Option<&mut RegenerationBonus>)>,
) {
    for (farmer, mut mana, bonus) in &mut farmers {
        let mut regeneration = config.mana.regeneration * time.delta_secs();

        if let Some(mut bonus) = bonus {
            regeneration *= bonus.multiplier;

            if bonus.timer.tick(time.delta()).is_finished() {
                commands.entity(farmer).remove::<RegenerationBonus>();
            }
        }

        mana.regenerate(regeneration);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::content::testing::{RecipeApp, crop};
    use crate::economy::EconomyPlugin;
    use crate::game::StartingResources;
    use crate::game::testing::{STEP, TestApp};
    use crate::growth::GrowthPlugin;
    use crate::map::testing::layout;
    use crate::movement::MovementPlugin;

    const PLOT_A: Vec2 = Vec2::new(0.0, 0.0);
    const PLOT_B: Vec2 = Vec2::new(6.0, 0.0);
    const SECOND: Duration = Duration::from_secs(1);

    fn config() -> FarmingConfig {
        FarmingConfig {
            seed: "a".to_owned(),
            action_time: SECOND,
            farmer_speed: 100.0,
            rabbits: RabbitConfig {
                count: 0,
                speed: 2.0,
                harvest_time: SECOND,
                flee_radius: 3.0,
                flee_from: Targets::Only(vec!["enemy".to_owned()]),
            },
            mana: ManaConfig {
                max: 10.0,
                regeneration: 1.0,
            },
            growth_boost: GrowthBoost {
                mana_cost: 4.0,
                cast_time: SECOND,
                radius: 3.0,
                growth_multiplier: 2.0,
                duration: Duration::from_secs(60),
            },
        }
    }

    fn farm_app(config: FarmingConfig, seeds: u32) -> App {
        let mut app = App::headless();
        let mut layout = layout();
        layout.plots = vec![PLOT_A, PLOT_B];
        layout.farmer_start = PLOT_A;
        layout.rabbit_home = Vec2::new(0.0, 8.0);

        app.add_plugins((FarmingPlugin, GrowthPlugin, EconomyPlugin, MovementPlugin))
            .with_recipes(Recipes {
                crops: BTreeMap::from([("a".to_owned(), crop(Duration::from_secs(10)))]),
                ..default()
            })
            .init_resource::<CursorGround>()
            .insert_resource(config)
            .insert_resource(layout)
            .insert_resource(DifficultySettings::uniform(StartingResources {
                seeds,
                gold: 0,
            }));

        for plot in [PLOT_A, PLOT_B] {
            app.world_mut()
                .spawn((PlantPlot, Transform::from_translation(plot.on_ground())));
        }

        app.enter(AppState::InLevel);

        app
    }

    fn crops(app: &mut App) -> Vec<Entity> {
        app.world_mut()
            .query_filtered::<Entity, With<Crop>>()
            .iter(app.world())
            .collect()
    }

    fn farmer(app: &mut App) -> Entity {
        app.world_mut()
            .query_filtered::<Entity, With<Farmer>>()
            .single(app.world())
            .unwrap()
    }

    fn order(app: &mut App, order: FarmerOrder) -> bool {
        let farmer = farmer(app);

        app.world_mut()
            .get_mut::<FarmerOrder>(farmer)
            .unwrap()
            .replace(order)
    }

    fn walk_and(target: Vec2, action: FarmerAction) -> FarmerOrder {
        FarmerOrder::Walk {
            to: target,
            then: Some(action),
        }
    }

    fn mana(app: &mut App) -> f32 {
        let farmer = farmer(app);

        app.world().get::<Mana>(farmer).unwrap().current()
    }

    fn harvested(app: &App) -> u32 {
        app.world().resource::<Inventory>().count("a")
    }

    #[test]
    fn planting_requires_and_consumes_a_seed() {
        let mut app = farm_app(config(), 1);

        // The farmer starts the level by planting the first plot.
        app.advance_by(2 * SECOND);
        assert_eq!(crops(&mut app).len(), 1);
        assert_eq!(app.world().resource::<SeedStock>().seeds(), 0);

        order(
            &mut app,
            walk_and(PLOT_B, FarmerAction::Plant { plot: PLOT_B }),
        );
        app.advance_by(2 * SECOND);
        assert_eq!(crops(&mut app).len(), 1);
    }

    #[test]
    fn planted_crop_comes_from_the_seed_recipe() {
        let mut app = farm_app(config(), 1);

        app.advance_by(2 * SECOND);

        let crop = crops(&mut app)[0];
        assert_eq!(
            app.world().get::<Tags>(crop),
            Some(&Tags::new(["crop", "a"]))
        );
    }

    #[test]
    fn harvesting_resets_crop_growth_and_awards_one_yield() {
        let mut app = farm_app(config(), 1);

        app.advance_by(12 * SECOND);
        let crop = crops(&mut app)[0];
        assert!(app.world().get::<Growth>(crop).unwrap().is_ripe());

        order(&mut app, walk_and(PLOT_A, FarmerAction::Harvest { crop }));
        app.advance_by(2 * SECOND);

        let growth = app.world().get::<Growth>(crop).unwrap();
        assert!(!growth.is_ripe());
        assert!(growth.progress() < 0.2);
        assert_eq!(harvested(&app), 1);

        order(&mut app, walk_and(PLOT_A, FarmerAction::Harvest { crop }));
        app.advance_by(2 * SECOND);
        assert_eq!(harvested(&app), 1);
    }

    #[test]
    fn crop_ripening_this_tick_is_harvested_this_tick() {
        let mut app = farm_app(config(), 0);
        let crop = app
            .world_mut()
            .spawn((
                Crop,
                Growth::new(STEP),
                Yield {
                    item: "a".to_owned(),
                    amount: 1,
                },
                Transform::from_translation(PLOT_B.on_ground()),
            ))
            .id();
        let farmer = farmer(&mut app);
        *app.world_mut().get_mut::<FarmerOrder>(farmer).unwrap() = FarmerOrder::Work {
            action: FarmerAction::Harvest { crop },
            timer: Timer::new(STEP, TimerMode::Once),
        };

        app.update();

        assert_eq!(harvested(&app), 1);
    }

    #[test]
    fn farmer_cannot_start_another_action_until_current_action_finishes() {
        let mut app = farm_app(config(), 2);

        // Arrived at the first plot and planting.
        app.advance_by(SECOND / 2);
        assert!(!order(
            &mut app,
            walk_and(PLOT_B, FarmerAction::Plant { plot: PLOT_B })
        ));

        app.advance_by(SECOND);
        assert!(order(
            &mut app,
            walk_and(PLOT_B, FarmerAction::Plant { plot: PLOT_B })
        ));
    }

    #[test]
    fn eating_boosts_mana_regeneration_for_the_recipe_duration() {
        let mut config = config();
        config.mana.max = 100.0;
        let mut app = farm_app(config, 0);
        let farmer = farmer(&mut app);

        app.world_mut().get_mut::<Mana>(farmer).unwrap().current = 0.0;
        app.world_mut().resource_mut::<Inventory>().add("a", 1);
        app.world_mut().trigger(Eat);
        app.advance_by(SECOND);
        assert!((mana(&mut app) - 3.0).abs() < 0.1);
        assert_eq!(harvested(&app), 0);

        // The bonus lasts five seconds, then regeneration is back to normal.
        app.advance_by(5 * SECOND);
        let before = mana(&mut app);
        app.advance_by(SECOND);
        assert!((mana(&mut app) - before - 1.0).abs() < 0.1);

        // Without anything edible there is nothing to eat.
        app.world_mut()
            .resource_mut::<Inventory>()
            .add("inedible", 1);
        app.world_mut().trigger(Eat);
        app.update();
        assert!(app.world().get::<RegenerationBonus>(farmer).is_none());
        assert_eq!(app.world().resource::<Inventory>().count("inedible"), 1);
    }

    #[test]
    fn growth_boost_only_accelerates_crops_in_its_area_and_spends_mana() {
        let mut config = config();
        config.mana.regeneration = 0.0;
        let mut app = farm_app(config, 2);

        app.advance_by(2 * SECOND);
        order(
            &mut app,
            walk_and(PLOT_B, FarmerAction::Plant { plot: PLOT_B }),
        );
        app.advance_by(2 * SECOND);
        let [near, far] = crops(&mut app)[..] else {
            panic!("two crops expected");
        };

        order(&mut app, walk_and(PLOT_A, FarmerAction::CastGrowthBoost));
        app.advance_by(2 * SECOND);
        assert!((mana(&mut app) - 6.0).abs() < 0.01);
        assert!(app.world().get::<Boosted>(near).is_some());
        assert!(app.world().get::<Boosted>(far).is_none());

        order(&mut app, walk_and(PLOT_A, FarmerAction::CastGrowthBoost));
        app.advance_by(2 * SECOND);
        assert!((mana(&mut app) - 2.0).abs() < 0.01);

        // A third cast would overspend.
        order(&mut app, walk_and(PLOT_A, FarmerAction::CastGrowthBoost));
        app.advance_by(2 * SECOND);
        assert!((mana(&mut app) - 2.0).abs() < 0.01);
    }

    #[test]
    fn rabbits_harvest_ripe_crops() {
        let mut config = config();
        config.rabbits.count = 1;
        let mut app = farm_app(config, 1);

        // Ten seconds to ripen, four to walk from home, one to harvest.
        app.advance_by(17 * SECOND);
        assert_eq!(harvested(&app), 1);
    }

    #[test]
    fn rabbits_flee_matching_tags() {
        let mut config = config();
        config.rabbits.count = 1;
        let mut app = farm_app(config, 0);
        let danger = Vec2::new(0.0, 7.0);
        let rabbit_position = |app: &mut App| {
            app.world_mut()
                .query_filtered::<&Transform, With<FarmerRabbit>>()
                .single(app.world())
                .unwrap()
                .translation
                .xz()
        };

        let bystander = app
            .world_mut()
            .spawn((
                Tags::new(["b"]),
                Transform::from_translation(danger.on_ground()),
            ))
            .id();
        app.advance_by(SECOND);
        assert!(rabbit_position(&mut app).distance(danger) < 1.5);

        app.world_mut()
            .entity_mut(bystander)
            .insert(Tags::new(["enemy"]));
        app.advance_by(SECOND);
        assert!(rabbit_position(&mut app).distance(danger) > 1.5);
    }
}
