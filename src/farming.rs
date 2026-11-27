//! Farming: the farmer, the farmer rabbits and the crops.
//!
//! Owns crop growth, the farmer's orders (walk, plant, harvest, cast Growth Boost), mana, the seed
//! stock and the pantry of harvested crops. Farmer rabbits harvest ripe crops on their own and run
//! from any entity marked [`Threat`].
//!
//! Other plugins see crops through the [`Crop`] marker, spend harvested crops through [`Pantry`], and
//! mark dangerous entities with [`Threat`]. Starting seeds come from the selected difficulty.

use std::time::Duration;

use bevy::prelude::*;
use serde::Deserialize;

use crate::game::{
    AppState, DifficultySettings, GameSet, GroundMovement, GroundPosition, SelectedDifficulty,
};
use crate::map::{MapLayout, PlantPlot};
use crate::stage::CursorGround;

/// How close a rabbit gets to a crop or its home to work there, in world units.
const REACH: f32 = 0.6;
/// How far from a plot's center a click or a crop still counts as on the plot, in world units.
const PLOT_RADIUS: f32 = 0.8;
/// Half the size of the walkable field, in world units.
const FIELD_HALF_SIZE: f32 = 9.5;
const FARMER_COLOR: Color = Color::srgb(0.85, 0.55, 0.3);
const RABBIT_COLOR: Color = Color::srgb(0.95, 0.95, 0.9);
const GROWING_CROP_COLOR: Color = Color::srgb(0.3, 0.75, 0.25);
const RIPE_CROP_COLOR: Color = Color::srgb(0.9, 0.15, 0.1);

/// Farming values of a level.
#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FarmingConfig {
    /// Time for a planted or harvested tomato to ripen.
    #[serde(with = "humantime_serde")]
    pub growth_time: Duration,
    /// Tomatoes gained per harvest.
    pub harvest_yield: u32,
    /// Time the farmer spends planting or harvesting.
    #[serde(with = "humantime_serde")]
    pub action_time: Duration,
    /// Farmer walking speed, in world units per second.
    pub farmer_speed: f32,
    pub rabbits: RabbitConfig,
    pub mana: ManaConfig,
    pub tomato_bonus: TomatoBonus,
    pub growth_boost: GrowthBoost,
}

/// Farmer rabbit values.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RabbitConfig {
    pub count: u32,
    /// Walking speed, in world units per second.
    pub speed: f32,
    #[serde(with = "humantime_serde")]
    pub harvest_time: Duration,
    /// Rabbits run from threats closer than this, in world units.
    pub flee_radius: f32,
}

/// The farmer's mana pool.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManaConfig {
    pub max: f32,
    /// Mana regained per second.
    pub regeneration: f32,
}

/// Eating a tomato multiplies mana regeneration for a while.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomatoBonus {
    pub regeneration_multiplier: f32,
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
}

/// The Growth Boost spell: crops around the farmer grow faster for a while.
#[derive(Debug, Clone, Deserialize)]
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

/// Harvested tomatoes waiting to be sold or eaten.
#[derive(Resource, Debug, Default)]
pub struct Pantry {
    tomatoes: u32,
}

impl Pantry {
    pub fn tomatoes(&self) -> u32 {
        self.tomatoes
    }

    /// Takes `count` tomatoes, or none if there are fewer.
    pub fn take(&mut self, count: u32) -> bool {
        let Some(tomatoes) = self.tomatoes.checked_sub(count) else {
            return false;
        };

        self.tomatoes = tomatoes;

        true
    }

    fn store(&mut self, count: u32) {
        self.tomatoes += count;
    }
}

/// The player's farmer.
#[derive(Component, Debug)]
#[require(FarmerOrder)]
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

/// Temporarily multiplies mana regeneration, after eating a tomato.
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
}

/// A farmer rabbit that harvests ripe crops on its own.
#[derive(Component, Debug)]
#[require(RabbitTask)]
pub struct FarmerRabbit;

#[derive(Component, Debug, Default)]
enum RabbitTask {
    #[default]
    Idle,
    Harvest {
        crop: Entity,
        work: Timer,
    },
}

/// Something farmer rabbits run away from.
#[derive(Component, Debug, Default)]
pub struct Threat;

/// A planted crop.
#[derive(Component, Debug)]
pub struct Crop;

/// Growth of a crop towards ripeness.
#[derive(Component, Debug)]
struct Growth {
    timer: Timer,
}

impl Growth {
    fn new(growth_time: Duration) -> Self {
        Self {
            timer: Timer::new(growth_time, TimerMode::Once),
        }
    }

    fn advance(&mut self, delta: Duration) {
        self.timer.tick(delta);
    }

    fn is_ripe(&self) -> bool {
        self.timer.is_finished()
    }

    fn progress(&self) -> f32 {
        self.timer.fraction()
    }

    /// Picks a ripe crop and sends it back to its first growth stage. Returns whether there was
    /// anything to pick.
    fn harvest(&mut self) -> bool {
        if !self.is_ripe() {
            return false;
        }

        self.timer.reset();

        true
    }
}

/// Faster growth from a Growth Boost.
#[derive(Component, Debug)]
struct Boosted {
    multiplier: f32,
    timer: Timer,
}

/// The farmer eats a tomato from the pantry to boost mana regeneration.
#[derive(Event, Debug)]
pub struct EatTomato;

/// Meshes and materials shared by every farming entity.
#[derive(Resource, Debug)]
struct FarmingAssets {
    farmer_mesh: Handle<Mesh>,
    farmer_material: Handle<StandardMaterial>,
    rabbit_mesh: Handle<Mesh>,
    rabbit_material: Handle<StandardMaterial>,
    crop_mesh: Handle<Mesh>,
    growing_material: Handle<StandardMaterial>,
    ripe_material: Handle<StandardMaterial>,
}

impl FromWorld for FarmingAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let farmer_mesh = meshes.add(Capsule3d::new(0.3, 0.7));
        let rabbit_mesh = meshes.add(Capsule3d::new(0.18, 0.3));
        let crop_mesh = meshes.add(Sphere::new(0.35).mesh().ico(1).expect("valid subdivisions"));

        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();

        Self {
            farmer_mesh,
            farmer_material: materials.add(FARMER_COLOR),
            rabbit_mesh,
            rabbit_material: materials.add(RABBIT_COLOR),
            crop_mesh,
            growing_material: materials.add(GROWING_CROP_COLOR),
            ripe_material: materials.add(RIPE_CROP_COLOR),
        }
    }
}

/// Runs the farm: the farmer, farmer rabbits and crops.
pub struct FarmingPlugin;

impl Plugin for FarmingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SeedStock>()
            .init_resource::<Pantry>()
            .init_resource::<FarmingAssets>()
            .add_observer(eat_tomato)
            .add_systems(OnEnter(AppState::InLevel), start_farm)
            .add_systems(
                Update,
                (
                    (order_farmer, cast_growth_boost, request_tomato).in_set(GameSet::Input),
                    show_crop_growth.in_set(GameSet::Presentation),
                ),
            )
            .add_systems(
                FixedUpdate,
                (drive_farmer, drive_rabbits, grow_crops, regenerate_mana).in_set(GameSet::Act),
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
    commands.insert_resource(Pantry::default());

    let first_plot = layout.plots.first().copied();

    commands.spawn((
        Name::new("Farmer"),
        Farmer,
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
        let offset = Vec2::new(index as f32 * 0.8, 0.0);

        commands.spawn((
            Name::new("Farmer rabbit"),
            FarmerRabbit,
            Mesh3d(assets.rabbit_mesh.clone()),
            MeshMaterial3d(assets.rabbit_material.clone()),
            Transform::from_translation((layout.rabbit_home + offset).on_ground()),
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

    let action = FarmerAction::CastGrowthBoost;
    let cast = FarmerOrder::Work {
        action,
        timer: Timer::new(action.duration(&config), TimerMode::Once),
    };

    if !order.replace(cast) {
        info!("The farmer is busy");
    }
}

/// `E`: eat a tomato.
fn request_tomato(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyE) {
        commands.trigger(EatTomato);
    }
}

fn eat_tomato(
    _: On<EatTomato>,
    mut commands: Commands,
    config: Res<FarmingConfig>,
    mut pantry: ResMut<Pantry>,
    farmer: Query<Entity, With<Farmer>>,
) {
    let Ok(farmer) = farmer.single() else {
        return;
    };

    if !pantry.take(1) {
        info!("No tomato to eat");
        return;
    }

    commands.entity(farmer).insert(RegenerationBonus {
        multiplier: config.tomato_bonus.regeneration_multiplier,
        timer: Timer::new(config.tomato_bonus.duration, TimerMode::Once),
    });
}

fn drive_farmer(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<FarmingConfig>,
    assets: Res<FarmingAssets>,
    mut seeds: ResMut<SeedStock>,
    mut pantry: ResMut<Pantry>,
    farmer: Single<(&mut Transform, &mut FarmerOrder, &mut Mana), With<Farmer>>,
    plots: Query<&Transform, (With<PlantPlot>, Without<Farmer>, Without<Crop>)>,
    mut crops: Query<(Entity, &Transform, &mut Growth), (With<Crop>, Without<Farmer>)>,
) {
    let (mut transform, mut order, mut mana) = farmer.into_inner();

    let finished_action = match &mut *order {
        FarmerOrder::Idle => None,
        FarmerOrder::Walk { to, then } => {
            let step = config.farmer_speed * time.delta_secs();

            if !transform.move_towards(*to, step, 0.0) {
                return;
            }

            *order = match then.take() {
                Some(action) => FarmerOrder::Work {
                    action,
                    timer: Timer::new(action.duration(&config), TimerMode::Once),
                },
                None => FarmerOrder::Idle,
            };

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
                .any(|(_, crop, _)| crop.translation.xz().distance(plot) < PLOT_RADIUS);

            if plot_exists && !occupied && seeds.take_one() {
                commands.spawn((
                    Name::new("Tomato"),
                    Crop,
                    Growth::new(config.growth_time),
                    Mesh3d(assets.crop_mesh.clone()),
                    MeshMaterial3d(assets.growing_material.clone()),
                    Transform::from_translation(plot.on_ground() + Vec3::Y * 0.3),
                    DespawnOnExit(AppState::InLevel),
                ));
            }
        }
        FarmerAction::Harvest { crop } => {
            if let Ok((_, _, mut growth)) = crops.get_mut(crop)
                && growth.harvest()
            {
                pantry.store(config.harvest_yield);
            }
        }
        FarmerAction::CastGrowthBoost => {
            if !mana.try_spend(config.growth_boost.mana_cost) {
                return;
            }

            let center = transform.translation.xz();
            let boost = &config.growth_boost;

            for (crop, crop_transform, _) in &crops {
                if crop_transform.translation.xz().distance(center) <= boost.radius {
                    commands.entity(crop).insert(Boosted {
                        multiplier: boost.growth_multiplier,
                        timer: Timer::new(boost.duration, TimerMode::Once),
                    });
                }
            }
        }
    }
}

fn drive_rabbits(
    time: Res<Time>,
    config: Res<FarmingConfig>,
    layout: Res<MapLayout>,
    mut pantry: ResMut<Pantry>,
    mut claimed_crops: Local<Vec<Entity>>,
    threats: Query<&Transform, (With<Threat>, Without<FarmerRabbit>)>,
    mut rabbits: Query<(&mut Transform, &mut RabbitTask), With<FarmerRabbit>>,
    mut crops: Query<(Entity, &Transform, &mut Growth), (With<Crop>, Without<FarmerRabbit>)>,
) {
    let rabbit_config = &config.rabbits;
    let step = rabbit_config.speed * time.delta_secs();

    claimed_crops.clear();
    claimed_crops.extend(rabbits.iter().filter_map(|(_, task)| match task {
        RabbitTask::Harvest { crop, .. } => Some(*crop),
        RabbitTask::Idle => None,
    }));

    for (mut transform, mut task) in &mut rabbits {
        let position = transform.translation.xz();
        let nearest_threat = threats
            .iter()
            .map(|threat| threat.translation.xz())
            .filter(|threat| threat.distance(position) < rabbit_config.flee_radius)
            .min_by(|a, b| a.distance(position).total_cmp(&b.distance(position)));

        if let Some(threat) = nearest_threat {
            let away = (position - threat).normalize_or(Vec2::X);
            let target = (position + away * step)
                .clamp(Vec2::splat(-FIELD_HALF_SIZE), Vec2::splat(FIELD_HALF_SIZE));

            transform.move_towards(target, step, 0.0);
            *task = RabbitTask::Idle;

            continue;
        }

        match &mut *task {
            RabbitTask::Idle => {
                let nearest_ripe_crop = crops
                    .iter()
                    .filter(|(crop, _, growth)| growth.is_ripe() && !claimed_crops.contains(crop))
                    .min_by(|(_, a, _), (_, b, _)| {
                        let a = a.translation.xz().distance(position);
                        let b = b.translation.xz().distance(position);

                        a.total_cmp(&b)
                    });

                match nearest_ripe_crop {
                    Some((crop, _, _)) => {
                        claimed_crops.push(crop);
                        *task = RabbitTask::Harvest {
                            crop,
                            work: Timer::new(rabbit_config.harvest_time, TimerMode::Once),
                        };
                    }
                    None => {
                        transform.move_towards(layout.rabbit_home, step, REACH);
                    }
                }
            }
            RabbitTask::Harvest { crop, work } => {
                let Ok((_, crop_transform, mut growth)) = crops.get_mut(*crop) else {
                    *task = RabbitTask::Idle;
                    continue;
                };

                if !growth.is_ripe() {
                    *task = RabbitTask::Idle;
                    continue;
                }

                if !transform.move_towards(crop_transform.translation.xz(), step, REACH) {
                    continue;
                }

                if work.tick(time.delta()).is_finished() {
                    if growth.harvest() {
                        pantry.store(config.harvest_yield);
                    }

                    *task = RabbitTask::Idle;
                }
            }
        }
    }
}

fn grow_crops(
    mut commands: Commands,
    time: Res<Time>,
    mut crops: Query<(Entity, &mut Growth, Option<&mut Boosted>)>,
) {
    for (crop, mut growth, boosted) in &mut crops {
        let Some(mut boosted) = boosted else {
            growth.advance(time.delta());
            continue;
        };

        growth.advance(time.delta().mul_f32(boosted.multiplier));

        if boosted.timer.tick(time.delta()).is_finished() {
            commands.entity(crop).remove::<Boosted>();
        }
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

fn show_crop_growth(
    assets: Res<FarmingAssets>,
    mut crops: Query<(
        &Growth,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (growth, mut transform, mut material) in &mut crops {
        transform.scale = Vec3::splat(0.4 + 0.6 * growth.progress());

        let wanted = if growth.is_ripe() {
            &assets.ripe_material
        } else {
            &assets.growing_material
        };

        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::StartingResources;
    use crate::game::testing::TestApp;
    use crate::map::MonsterPath;

    const PLOT_A: Vec2 = Vec2::new(0.0, 0.0);
    const PLOT_B: Vec2 = Vec2::new(6.0, 0.0);
    const SECOND: Duration = Duration::from_secs(1);

    fn config() -> FarmingConfig {
        FarmingConfig {
            growth_time: Duration::from_secs(10),
            harvest_yield: 1,
            action_time: SECOND,
            farmer_speed: 100.0,
            rabbits: RabbitConfig {
                count: 0,
                speed: 2.0,
                harvest_time: SECOND,
                flee_radius: 3.0,
            },
            mana: ManaConfig {
                max: 10.0,
                regeneration: 1.0,
            },
            tomato_bonus: TomatoBonus {
                regeneration_multiplier: 3.0,
                duration: Duration::from_secs(5),
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

        app.add_plugins(FarmingPlugin)
            .init_resource::<CursorGround>()
            .insert_resource(config)
            .insert_resource(MapLayout {
                path: MonsterPath::try_from(vec![Vec2::new(-9.0, 0.0), Vec2::new(9.0, 0.0)])
                    .unwrap(),
                plots: vec![PLOT_A, PLOT_B],
                peddler: Vec2::ZERO,
                farmer_start: PLOT_A,
                rabbit_home: Vec2::new(0.0, 8.0),
            })
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
        assert_eq!(app.world().resource::<Pantry>().tomatoes(), 1);

        order(&mut app, walk_and(PLOT_A, FarmerAction::Harvest { crop }));
        app.advance_by(2 * SECOND);
        assert_eq!(app.world().resource::<Pantry>().tomatoes(), 1);
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
    fn eating_tomato_boosts_mana_regeneration_for_configured_duration() {
        let mut config = config();
        config.mana.max = 100.0;
        let mut app = farm_app(config, 0);
        let farmer = farmer(&mut app);

        app.world_mut().get_mut::<Mana>(farmer).unwrap().current = 0.0;
        app.world_mut().resource_mut::<Pantry>().store(1);
        app.world_mut().trigger(EatTomato);
        app.advance_by(SECOND);
        assert!((mana(&mut app) - 3.0).abs() < 0.1);
        assert_eq!(app.world().resource::<Pantry>().tomatoes(), 0);

        // The bonus lasts five seconds, then regeneration is back to normal.
        app.advance_by(5 * SECOND);
        let before = mana(&mut app);
        app.advance_by(SECOND);
        assert!((mana(&mut app) - before - 1.0).abs() < 0.1);

        // Without a tomato there is nothing to eat.
        app.world_mut().trigger(EatTomato);
        app.update();
        assert!(app.world().get::<RegenerationBonus>(farmer).is_none());
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
        assert_eq!(app.world().resource::<Pantry>().tomatoes(), 1);
    }

    #[test]
    fn rabbits_move_away_from_threats() {
        let mut config = config();
        config.rabbits.count = 1;
        let mut app = farm_app(config, 0);
        let threat = Vec2::new(0.0, 7.0);

        app.world_mut()
            .spawn((Threat, Transform::from_translation(threat.on_ground())));
        app.advance_by(SECOND);

        let rabbit = app
            .world_mut()
            .query_filtered::<&Transform, With<FarmerRabbit>>()
            .single(app.world())
            .unwrap()
            .translation
            .xz();
        assert!(rabbit.distance(threat) > 1.5);
    }
}
