//! Level data and map layout.
//!
//! Loads the level definition from a RON asset and validates it: a bad value fails the load with a
//! [`LevelError`] naming its field. The content recipes go to [`RecipeStore`]; every other section is
//! copied into the resource of the plugin that uses it when a level starts, so editing the file while
//! the game runs (hot reload) changes the next level, not the running one. Recipes update right away
//! for future spawns, unless the running level still uses a recipe the new file removes: then they
//! wait for the next level. When a level starts, spawns the path, plant plots, spawner, crystal and
//! peddler, and shares the path with movement through [`PathWaypoints`].

use std::collections::BTreeMap;
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::Deserialize;
use thiserror::Error;

use crate::combat::Health;
use crate::content::{AgentRecipe, CropRecipe, MeleeRecipe, RecipeStore, Recipes, Rgb, Shape};
use crate::economy::Trade;
use crate::farming::FarmingConfig;
use crate::game::{AppState, DifficultySettings, GroundPosition, Timeline};
use crate::movement::PathWaypoints;
use crate::targeting::Tags;
use crate::waves::WaveScript;
const LEVEL_PATH: &str = "data/levels/prototype.level.ron";
const CRYSTAL_TAG: &str = "crystal";
const PATH_WIDTH: f32 = 0.8;
const PATH_COLOR: Color = Color::srgb(0.72, 0.6, 0.42);
const PLOT_SIZE: f32 = 1.4;
const PLOT_COLOR: Color = Color::srgb(0.4, 0.26, 0.15);
const SPAWNER_COLOR: Color = Color::srgb(0.2, 0.15, 0.1);
const CRYSTAL_COLOR: Color = Color::srgb(0.35, 0.65, 1.0);
const PEDDLER_COLOR: Color = Color::srgb(0.55, 0.35, 0.7);

/// Why a level definition is invalid. Each variant names the offending field.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum LevelError {
    #[error("{field} must be a finite number")]
    NotFinite { field: String },
    #[error("{field} must be greater than zero")]
    NotPositive { field: String },
    #[error("{field} must not be negative")]
    Negative { field: String },
    #[error("{field} names {id:?}, which is not in {section}")]
    UnknownRecipe {
        field: String,
        id: String,
        section: &'static str,
    },
    #[error("trade.sells must offer at least one defender")]
    NothingForSale,
}

impl LevelError {
    fn require_recipe<T>(
        recipes: &BTreeMap<String, T>,
        field: String,
        id: &str,
        section: &'static str,
    ) -> Result<(), Self> {
        if recipes.contains_key(id) {
            return Ok(());
        }

        Err(Self::UnknownRecipe {
            field,
            id: id.to_owned(),
            section,
        })
    }
}

/// Everything a level is made of, as authored in `assets/data/levels/*.level.ron`.
#[derive(Asset, TypePath, Debug, Clone, Deserialize)]
#[serde(try_from = "UnvalidatedLevelDefinition")]
pub struct LevelDefinition {
    pub difficulties: DifficultySettings,
    pub timeline: Timeline,
    pub map: MapLayout,
    pub farming: FarmingConfig,
    pub trade: Trade,
    pub waves: WaveScript,
    pub recipes: Recipes,
}

impl LevelDefinition {
    /// The first recipe the running level uses (its waves, seed or shop) that `recipes` lacks.
    fn missing_recipe(
        recipes: &Recipes,
        waves: &WaveScript,
        farming: &FarmingConfig,
        trade: &Trade,
    ) -> Option<String> {
        let enemies = waves
            .waves()
            .iter()
            .map(|wave| &wave.enemy)
            .find(|id| !recipes.enemies.contains_key(*id));
        let crops = std::iter::once(&farming.seed)
            .chain(trade.buys.keys())
            .find(|id| !recipes.crops.contains_key(*id));
        let defenders = trade
            .sells
            .keys()
            .find(|id| !recipes.defenders.contains_key(*id));

        enemies.or(crops).or(defenders).cloned()
    }

    fn validate(&self) -> Result<(), LevelError> {
        self.map.validate("map")?;
        self.farming.validate("farming")?;

        for (section, agents) in [
            ("enemies", &self.recipes.enemies),
            ("defenders", &self.recipes.defenders),
        ] {
            for (id, recipe) in agents {
                recipe.validate(&format!("{section}.{id}"))?;
            }
        }

        for (id, recipe) in &self.recipes.crops {
            recipe.validate(&format!("crops.{id}"))?;
        }

        for (index, wave) in self.waves.waves().iter().enumerate() {
            LevelError::require_recipe(
                &self.recipes.enemies,
                format!("waves[{index}].enemy"),
                &wave.enemy,
                "enemies",
            )?;
        }

        LevelError::require_recipe(
            &self.recipes.crops,
            "farming.seed".to_owned(),
            &self.farming.seed,
            "crops",
        )?;

        for id in self.trade.buys.keys() {
            LevelError::require_recipe(&self.recipes.crops, "trade.buys".to_owned(), id, "crops")?;
        }

        for id in self.trade.sells.keys() {
            LevelError::require_recipe(
                &self.recipes.defenders,
                "trade.sells".to_owned(),
                id,
                "defenders",
            )?;
        }

        if self.trade.sells.is_empty() {
            return Err(LevelError::NothingForSale);
        }

        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnvalidatedLevelDefinition {
    difficulties: DifficultySettings,
    timeline: Timeline,
    map: MapLayout,
    farming: FarmingConfig,
    trade: Trade,
    waves: WaveScript,
    enemies: BTreeMap<String, AgentRecipe>,
    defenders: BTreeMap<String, AgentRecipe>,
    crops: BTreeMap<String, CropRecipe>,
}

impl TryFrom<UnvalidatedLevelDefinition> for LevelDefinition {
    type Error = LevelError;

    fn try_from(level: UnvalidatedLevelDefinition) -> Result<Self, Self::Error> {
        let level = Self {
            difficulties: level.difficulties,
            timeline: level.timeline,
            map: level.map,
            farming: level.farming,
            trade: level.trade,
            waves: level.waves,
            recipes: Recipes {
                enemies: level.enemies,
                defenders: level.defenders,
                crops: level.crops,
            },
        };

        level.validate()?;

        Ok(level)
    }
}

/// Where things are on the map. Positions are on the ground plane, in world units.
#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapLayout {
    /// Crystal health before it breaks.
    pub crystal_health: f32,
    pub path: MonsterPath,
    pub plots: Vec<Vec2>,
    pub peddler: Vec2,
    pub farmer_start: Vec2,
    pub rabbit_home: Vec2,
}

/// The route enemies walk, from the spawner to the crystal. Always has at least two waypoints.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "Vec<Vec2>")]
pub struct MonsterPath(Vec<Vec2>);

impl MonsterPath {
    /// Where enemies appear.
    pub fn spawner(&self) -> Vec2 {
        self.0[0]
    }

    /// Where the crystal stands, at the end of the path.
    pub fn crystal(&self) -> Vec2 {
        self.0[self.0.len() - 1]
    }

    /// Every waypoint, from the spawner to the crystal.
    pub fn waypoints(&self) -> &[Vec2] {
        &self.0
    }

    /// Distance from a point to the nearest path segment.
    #[cfg(test)]
    fn distance_to(&self, point: Vec2) -> f32 {
        self.0
            .windows(2)
            .map(|segment| {
                let (start, end) = (segment[0], segment[1]);
                let direction = end - start;
                let length_squared = direction.length_squared();

                if length_squared == 0.0 {
                    return point.distance(start);
                }

                let along = ((point - start).dot(direction) / length_squared).clamp(0.0, 1.0);

                point.distance(start + direction * along)
            })
            .fold(f32::INFINITY, f32::min)
    }
}

impl TryFrom<Vec<Vec2>> for MonsterPath {
    type Error = &'static str;

    fn try_from(waypoints: Vec<Vec2>) -> Result<Self, Self::Error> {
        if waypoints.len() < 2 {
            return Err("map.path needs a spawner and a crystal waypoint");
        }

        Ok(Self(waypoints))
    }
}

/// A spot where the farmer can plant a crop.
#[derive(Component, Debug)]
pub struct PlantPlot;

/// Where enemies appear.
#[derive(Component, Debug)]
pub struct Spawner;

/// The crystal enemies try to destroy.
#[derive(Component, Debug)]
pub struct Crystal;

/// The peddler who buys crops and recruits defenders.
#[derive(Component, Debug)]
pub struct Peddler;

/// The level definition being played.
#[derive(Resource, Debug)]
struct CurrentLevel(Handle<LevelDefinition>);

/// The sections of the running level that name recipes.
#[derive(SystemParam)]
struct RunningLevel<'w> {
    waves: Option<Res<'w, WaveScript>>,
    farming: Option<Res<'w, FarmingConfig>>,
    trade: Option<Res<'w, Trade>>,
}

impl RunningLevel<'_> {
    /// The first recipe the running level uses that `recipes` lacks.
    fn missing_recipe(&self, recipes: &Recipes) -> Option<String> {
        let (Some(waves), Some(farming), Some(trade)) = (&self.waves, &self.farming, &self.trade)
        else {
            return None;
        };

        LevelDefinition::missing_recipe(recipes, waves, farming, trade)
    }
}

/// A level value that load-time validation checks.
trait LevelValue {
    /// Fails unless the value is finite.
    fn finite(&self, field: &str) -> Result<(), LevelError>;

    /// Fails unless the value is finite and greater than zero.
    fn positive(&self, field: &str) -> Result<(), LevelError>;

    /// Fails unless the value is finite and zero or more.
    fn non_negative(&self, field: &str) -> Result<(), LevelError>;
}

/// A section of the level that validates its own fields, named under `field`.
trait Validate {
    fn validate(&self, field: &str) -> Result<(), LevelError>;
}

impl LevelValue for f32 {
    fn finite(&self, field: &str) -> Result<(), LevelError> {
        if self.is_finite() {
            return Ok(());
        }

        Err(LevelError::NotFinite {
            field: field.to_owned(),
        })
    }

    fn positive(&self, field: &str) -> Result<(), LevelError> {
        self.finite(field)?;

        if *self > 0.0 {
            return Ok(());
        }

        Err(LevelError::NotPositive {
            field: field.to_owned(),
        })
    }

    fn non_negative(&self, field: &str) -> Result<(), LevelError> {
        self.finite(field)?;

        if *self >= 0.0 {
            return Ok(());
        }

        Err(LevelError::Negative {
            field: field.to_owned(),
        })
    }
}

impl LevelValue for Duration {
    fn finite(&self, _field: &str) -> Result<(), LevelError> {
        Ok(())
    }

    fn positive(&self, field: &str) -> Result<(), LevelError> {
        if !self.is_zero() {
            return Ok(());
        }

        Err(LevelError::NotPositive {
            field: field.to_owned(),
        })
    }

    fn non_negative(&self, _field: &str) -> Result<(), LevelError> {
        Ok(())
    }
}

impl LevelValue for Vec2 {
    fn finite(&self, field: &str) -> Result<(), LevelError> {
        self.x.finite(field)?;
        self.y.finite(field)
    }

    fn positive(&self, field: &str) -> Result<(), LevelError> {
        self.x.positive(field)?;
        self.y.positive(field)
    }

    fn non_negative(&self, field: &str) -> Result<(), LevelError> {
        self.x.non_negative(field)?;
        self.y.non_negative(field)
    }
}

impl LevelValue for Rgb {
    fn finite(&self, field: &str) -> Result<(), LevelError> {
        self.0.finite(field)?;
        self.1.finite(field)?;
        self.2.finite(field)
    }

    fn positive(&self, field: &str) -> Result<(), LevelError> {
        self.0.positive(field)?;
        self.1.positive(field)?;
        self.2.positive(field)
    }

    fn non_negative(&self, field: &str) -> Result<(), LevelError> {
        self.0.non_negative(field)?;
        self.1.non_negative(field)?;
        self.2.non_negative(field)
    }
}

impl Validate for MapLayout {
    fn validate(&self, field: &str) -> Result<(), LevelError> {
        self.crystal_health
            .positive(&format!("{field}.crystal_health"))?;

        for (index, waypoint) in self.path.waypoints().iter().enumerate() {
            waypoint.finite(&format!("{field}.path[{index}]"))?;
        }

        for (index, plot) in self.plots.iter().enumerate() {
            plot.finite(&format!("{field}.plots[{index}]"))?;
        }

        self.peddler.finite(&format!("{field}.peddler"))?;
        self.farmer_start.finite(&format!("{field}.farmer_start"))?;
        self.rabbit_home.finite(&format!("{field}.rabbit_home"))
    }
}

impl Validate for FarmingConfig {
    fn validate(&self, field: &str) -> Result<(), LevelError> {
        self.action_time.positive(&format!("{field}.action_time"))?;
        self.farmer_speed
            .positive(&format!("{field}.farmer_speed"))?;

        let rabbits = &self.rabbits;
        rabbits.speed.positive(&format!("{field}.rabbits.speed"))?;
        rabbits
            .harvest_time
            .positive(&format!("{field}.rabbits.harvest_time"))?;
        rabbits
            .flee_radius
            .positive(&format!("{field}.rabbits.flee_radius"))?;

        self.mana.max.positive(&format!("{field}.mana.max"))?;
        self.mana
            .regeneration
            .non_negative(&format!("{field}.mana.regeneration"))?;

        let boost = &self.growth_boost;
        boost
            .mana_cost
            .non_negative(&format!("{field}.growth_boost.mana_cost"))?;
        boost
            .radius
            .positive(&format!("{field}.growth_boost.radius"))?;
        boost
            .growth_multiplier
            .positive(&format!("{field}.growth_boost.growth_multiplier"))?;
        boost
            .duration
            .positive(&format!("{field}.growth_boost.duration"))
    }
}

impl Validate for Shape {
    fn validate(&self, field: &str) -> Result<(), LevelError> {
        match *self {
            Shape::Sphere { radius } => radius.positive(&format!("{field}.radius")),
            Shape::Capsule { radius, length } => {
                radius.positive(&format!("{field}.radius"))?;
                length.positive(&format!("{field}.length"))
            }
        }
    }
}

impl Validate for MeleeRecipe {
    fn validate(&self, field: &str) -> Result<(), LevelError> {
        self.damage.finite(&format!("{field}.damage"))?;
        self.range.positive(&format!("{field}.range"))?;
        self.interval.positive(&format!("{field}.interval"))
    }
}

impl Validate for AgentRecipe {
    fn validate(&self, field: &str) -> Result<(), LevelError> {
        self.health.positive(&format!("{field}.health"))?;
        self.speed.positive(&format!("{field}.speed"))?;
        self.sight.positive(&format!("{field}.sight"))?;

        if let Some(melee) = &self.melee {
            melee.validate(&format!("{field}.melee"))?;
        }

        self.visual
            .shape
            .validate(&format!("{field}.visual.shape"))?;
        self.visual.color.finite(&format!("{field}.visual.color"))
    }
}

impl Validate for CropRecipe {
    fn validate(&self, field: &str) -> Result<(), LevelError> {
        self.health.positive(&format!("{field}.health"))?;
        self.growth_time.positive(&format!("{field}.growth_time"))?;

        if let Some(eat) = &self.eat {
            eat.regeneration_multiplier
                .positive(&format!("{field}.eat.regeneration_multiplier"))?;
            eat.duration.positive(&format!("{field}.eat.duration"))?;
        }

        self.visual
            .shape
            .validate(&format!("{field}.visual.shape"))?;
        self.visual.color.finite(&format!("{field}.visual.color"))?;
        self.visual
            .ripe_color
            .finite(&format!("{field}.visual.ripe_color"))
    }
}

/// Loads the level definition and spawns the map.
pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RonAssetPlugin::<LevelDefinition>::new(&["level.ron"]))
            .add_systems(Startup, load_level)
            .add_systems(Update, apply_reloaded_level)
            // Copying on exit makes the sections visible to every `OnEnter(InLevel)` system.
            .add_systems(OnExit(AppState::Menu), apply_level)
            .add_systems(OnExit(AppState::Results), apply_level)
            .add_systems(OnEnter(AppState::InLevel), spawn_map);
    }
}

fn load_level(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(CurrentLevel(asset_server.load(LEVEL_PATH)));
}

/// Copies every section of the current level into resources and replaces the recipes.
fn apply_level(
    mut commands: Commands,
    current_level: Res<CurrentLevel>,
    levels: Res<Assets<LevelDefinition>>,
    mut store: RecipeStore,
) {
    let Some(level) = levels.get(&current_level.0) else {
        return;
    };

    commands.insert_resource(level.difficulties.clone());
    commands.insert_resource(level.timeline.clone());
    commands.insert_resource(level.map.clone());
    commands.insert_resource(level.farming.clone());
    commands.insert_resource(level.trade.clone());
    commands.insert_resource(level.waves.clone());
    store.replace(level.recipes.clone());
}

/// Applies the level on first load and the recipes of every valid hot reload.
fn apply_reloaded_level(
    mut commands: Commands,
    mut asset_events: MessageReader<AssetEvent<LevelDefinition>>,
    current_level: Res<CurrentLevel>,
    levels: Res<Assets<LevelDefinition>>,
    state: Res<State<AppState>>,
    mut next_state: ResMut<NextState<AppState>>,
    running: RunningLevel,
    mut store: RecipeStore,
) {
    let reloaded = asset_events.read().any(|event| match event {
        AssetEvent::Added { id }
        | AssetEvent::LoadedWithDependencies { id }
        | AssetEvent::Modified { id } => *id == current_level.0.id(),
        _ => false,
    });

    if !reloaded {
        return;
    }

    let Some(level) = levels.get(&current_level.0) else {
        return;
    };

    match state.get() {
        AppState::Loading => {
            commands.insert_resource(level.difficulties.clone());
            commands.insert_resource(level.timeline.clone());
            commands.insert_resource(level.map.clone());
            commands.insert_resource(level.farming.clone());
            commands.insert_resource(level.trade.clone());
            commands.insert_resource(level.waves.clone());
            next_state.set(AppState::Menu);
        }
        AppState::InLevel => {
            if let Some(id) = running.missing_recipe(&level.recipes) {
                warn!(
                    "The running level still uses recipe {id:?}; new recipes apply when the next \
                     level starts"
                );

                return;
            }
        }
        AppState::Menu | AppState::Results => {}
    }

    if store.recipes() != &level.recipes {
        store.replace(level.recipes.clone());
    }

    info!("Level definition applied");
}

fn spawn_map(
    mut commands: Commands,
    layout: Res<MapLayout>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(PathWaypoints(layout.path.waypoints().to_vec()));

    let path_material = materials.add(PATH_COLOR);

    for segment in layout.path.waypoints().windows(2) {
        let (start, end) = (segment[0].on_ground(), segment[1].on_ground());
        let length = start.distance(end) + PATH_WIDTH;

        commands.spawn((
            Name::new("Path"),
            Mesh3d(meshes.add(Cuboid::new(PATH_WIDTH, 0.02, length))),
            MeshMaterial3d(path_material.clone()),
            Transform::from_translation(start.midpoint(end)).looking_at(end, Vec3::Y),
            DespawnOnExit(AppState::InLevel),
        ));
    }

    let plot_mesh = meshes.add(Cuboid::new(PLOT_SIZE, 0.05, PLOT_SIZE));
    let plot_material = materials.add(PLOT_COLOR);

    for plot in &layout.plots {
        commands.spawn((
            Name::new("Plant plot"),
            PlantPlot,
            Mesh3d(plot_mesh.clone()),
            MeshMaterial3d(plot_material.clone()),
            Transform::from_translation(plot.on_ground()),
            DespawnOnExit(AppState::InLevel),
        ));
    }

    commands.spawn((
        Name::new("Spawner"),
        Spawner,
        Mesh3d(meshes.add(Cylinder::new(0.9, 0.1))),
        MeshMaterial3d(materials.add(SPAWNER_COLOR)),
        Transform::from_translation(layout.path.spawner().on_ground()),
        DespawnOnExit(AppState::InLevel),
    ));

    commands.spawn((
        Name::new("Crystal"),
        Crystal,
        Tags::new([CRYSTAL_TAG]),
        Mesh3d(meshes.add(Cuboid::from_length(0.9))),
        MeshMaterial3d(materials.add(CRYSTAL_COLOR)),
        Transform::from_translation(layout.path.crystal().on_ground() + Vec3::Y)
            .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.6, 0.0, 0.6)),
        Health::new(layout.crystal_health),
        DespawnOnExit(AppState::InLevel),
    ));

    commands.spawn((
        Name::new("Peddler"),
        Peddler,
        Mesh3d(meshes.add(Capsule3d::new(0.35, 0.8))),
        MeshMaterial3d(materials.add(PEDDLER_COLOR)),
        Transform::from_translation(layout.peddler.on_ground() + Vec3::Y * 0.75),
        DespawnOnExit(AppState::InLevel),
    ));
}

/// Map values shared by tests of plugins that need a layout.
#[cfg(test)]
pub mod testing {
    use super::*;

    /// A straight path from (-9, 0) to (9, 0) and no plots.
    pub fn layout() -> MapLayout {
        MapLayout {
            crystal_health: 100.0,
            path: MonsterPath::try_from(vec![Vec2::new(-9.0, 0.0), Vec2::new(9.0, 0.0)]).unwrap(),
            plots: vec![],
            peddler: Vec2::ZERO,
            farmer_start: Vec2::ZERO,
            rabbit_home: Vec2::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ContentPlugin;
    use crate::content::Roaming;
    use crate::economy::EconomyPlugin;
    use crate::game::Difficulty;
    use crate::game::testing::TestApp;
    use crate::waves::WavesPlugin;

    const PROTOTYPE_LEVEL: &str = include_str!("../assets/data/levels/prototype.level.ron");

    fn parse(source: &str) -> ron::error::SpannedResult<LevelDefinition> {
        ron::from_str(source)
    }

    fn error(source: &str) -> String {
        parse(source).unwrap_err().to_string()
    }

    #[test]
    fn prototype_level_parses_and_references_resolve() {
        let level = parse(PROTOTYPE_LEVEL).unwrap();

        assert!(level.map.path.waypoints().len() >= 2);
        assert_eq!(level.map.plots.len(), 3);
        assert!(level.timeline.sell_window < level.timeline.length);
        assert!(!level.waves.waves().is_empty());
        assert!(!level.trade.sells.is_empty());
        assert_eq!(
            LevelDefinition::missing_recipe(
                &level.recipes,
                &level.waves,
                &level.farming,
                &level.trade
            ),
            None
        );

        for difficulty in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
            assert!(level.difficulties.starting_resources(difficulty).seeds >= 1);
        }
    }

    #[test]
    fn hard_difficulty_can_afford_a_defender_at_level_start() {
        let level = parse(PROTOTYPE_LEVEL).unwrap();
        let hard = level.difficulties.starting_resources(Difficulty::Hard);
        let cheapest = level.trade.sells.values().min().unwrap();

        assert!(hard.seeds >= 1);
        assert!(hard.gold >= *cheapest);
    }

    #[test]
    fn prototype_plots_are_in_sight_of_path_followers() {
        let level = parse(PROTOTYPE_LEVEL).unwrap();
        let followers = level
            .recipes
            .enemies
            .values()
            .filter(|recipe| recipe.movement == Roaming::FollowPath);

        for follower in followers {
            assert!(
                level
                    .map
                    .plots
                    .iter()
                    .all(|plot| level.map.path.distance_to(*plot) <= follower.sight)
            );
        }
    }

    #[test]
    fn malformed_level_is_a_parse_error() {
        assert!(parse("(difficulties: (easy: (seeds: 1").is_err());
    }

    #[test]
    fn zero_health_names_the_field() {
        let invalid = PROTOTYPE_LEVEL.replacen("health: 3.0", "health: 0.0", 1);

        assert!(error(&invalid).contains("enemies.green_ant.health must be greater than zero"));
    }

    #[test]
    fn unknown_wave_enemy_names_the_field() {
        let invalid = PROTOTYPE_LEVEL.replacen(r#"enemy: "green_ant""#, r#"enemy: "red_ant""#, 1);

        assert!(error(&invalid).contains(r#"waves[0].enemy names "red_ant""#));
    }

    #[test]
    fn unknown_seed_names_the_field() {
        let invalid = PROTOTYPE_LEVEL.replace(r#"seed: "tomato""#, r#"seed: "carrot""#);

        assert!(error(&invalid).contains("farming.seed"));
    }

    #[test]
    fn nothing_for_sale_is_rejected() {
        let invalid = PROTOTYPE_LEVEL.replace(r#"sells: {"knight_bunny": 10}"#, "sells: {}");

        assert!(error(&invalid).contains("trade.sells must offer at least one defender"));
    }

    #[test]
    fn bad_growth_multiplier_is_rejected() {
        for value in ["-1.0", "0.0", "NaN", "inf"] {
            let invalid = PROTOTYPE_LEVEL.replace(
                "growth_multiplier: 3.0",
                &format!("growth_multiplier: {value}"),
            );

            assert!(
                error(&invalid).contains("farming.growth_boost.growth_multiplier"),
                "{value} should be rejected"
            );
        }
    }

    #[test]
    fn negative_mana_values_are_rejected_and_zero_is_allowed() {
        for (authored, field) in [
            ("regeneration: 0.25", "farming.mana.regeneration"),
            ("mana_cost: 4.0", "farming.growth_boost.mana_cost"),
        ] {
            let name = authored.split(':').next().unwrap();
            let negative = PROTOTYPE_LEVEL.replacen(authored, &format!("{name}: -1.0"), 1);
            let zero = PROTOTYPE_LEVEL.replacen(authored, &format!("{name}: 0.0"), 1);

            assert!(error(&negative).contains(&format!("{field} must not be negative")));
            assert!(parse(&zero).is_ok(), "{field} may be zero");
        }
    }

    #[test]
    fn nonpositive_crystal_health_is_a_field_specific_error() {
        let invalid = PROTOTYPE_LEVEL.replace("crystal_health: 100.0", "crystal_health: 0.0");

        assert!(error(&invalid).contains("map.crystal_health"));
    }

    #[test]
    fn path_without_a_crystal_waypoint_is_rejected() {
        assert!(ron::from_str::<MonsterPath>("[(0.0, 0.0)]").is_err());
        assert!(ron::from_str::<MonsterPath>("[(0.0, 0.0), (1.0, 0.0)]").is_ok());
    }

    fn level_app() -> (App, Handle<LevelDefinition>) {
        let mut app = App::headless();
        app.add_plugins((MapPlugin, ContentPlugin, WavesPlugin, EconomyPlugin));

        let level = app
            .world_mut()
            .resource_mut::<Assets<LevelDefinition>>()
            .add(parse(PROTOTYPE_LEVEL).unwrap());
        app.insert_resource(CurrentLevel(level.clone()));
        // Frame 1 emits the asset event, frame 2 reads it and requests Menu, frame 3 enters Menu.
        app.update();
        app.update();
        app.update();

        (app, level)
    }

    fn reload(
        app: &mut App,
        handle: &Handle<LevelDefinition>,
        edit: impl FnOnce(&mut LevelDefinition),
    ) {
        let mut assets = app.world_mut().resource_mut::<Assets<LevelDefinition>>();
        let mut level = assets.get(handle).unwrap().clone();
        edit(&mut level);
        assets.insert(handle.id(), level).unwrap();
        // The modified event is emitted at the end of one frame and read in the next.
        app.update();
        app.update();
    }

    fn rename_green_ant(level: &mut LevelDefinition) {
        let recipe = level.recipes.enemies.remove("green_ant").unwrap();
        level.recipes.enemies.insert("red_ant".to_owned(), recipe);
        level.waves = WaveScript::from(
            level
                .waves
                .waves()
                .iter()
                .cloned()
                .map(|mut wave| {
                    wave.enemy = "red_ant".to_owned();
                    wave
                })
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn first_load_applies_the_level_and_opens_the_menu() {
        let (app, _) = level_app();

        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::Menu
        );
        assert!(
            app.world()
                .resource::<Recipes>()
                .enemies
                .contains_key("green_ant")
        );
        assert!(app.world().contains_resource::<FarmingConfig>());
    }

    #[test]
    fn mid_level_rename_is_deferred() {
        let (mut app, handle) = level_app();
        app.enter(AppState::InLevel);

        reload(&mut app, &handle, rename_green_ant);

        let recipes = app.world().resource::<Recipes>();
        assert!(recipes.enemies.contains_key("green_ant"));
        assert!(!recipes.enemies.contains_key("red_ant"));

        app.enter(AppState::Results).enter(AppState::InLevel);

        let recipes = app.world().resource::<Recipes>();
        assert!(recipes.enemies.contains_key("red_ant"));
        assert_eq!(
            app.world().resource::<WaveScript>().waves()[0].enemy,
            "red_ant"
        );
    }

    #[test]
    fn mid_level_stat_change_applies_to_recipes_only() {
        let (mut app, handle) = level_app();
        app.enter(AppState::InLevel);

        reload(&mut app, &handle, |level| {
            level.recipes.enemies.get_mut("green_ant").unwrap().health = 7.0;
            level.map.crystal_health = 50.0;
        });

        assert_eq!(
            app.world().resource::<Recipes>().enemies["green_ant"].health,
            7.0
        );
        assert_eq!(app.world().resource::<MapLayout>().crystal_health, 100.0);
    }
}
