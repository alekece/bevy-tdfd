//! Level data and map layout.
//!
//! Loads the level definition from a RON asset and shares each section as a resource owned by the
//! plugin that uses it. When a level starts, spawns the path, plant plots, spawner, crystal and
//! peddler. Editing the RON file while the game runs re-applies its values (hot reload); positions
//! take effect the next time a level starts.

use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::Deserialize;

use crate::farming::FarmingConfig;
use crate::game::{AppState, DifficultySettings, GroundPosition, Timeline};

const LEVEL_PATH: &str = "data/levels/prototype.level.ron";
const PATH_WIDTH: f32 = 0.8;
const PATH_COLOR: Color = Color::srgb(0.72, 0.6, 0.42);
const PLOT_SIZE: f32 = 1.4;
const PLOT_COLOR: Color = Color::srgb(0.4, 0.26, 0.15);
const SPAWNER_COLOR: Color = Color::srgb(0.2, 0.15, 0.1);
const CRYSTAL_COLOR: Color = Color::srgb(0.35, 0.65, 1.0);
const PEDDLER_COLOR: Color = Color::srgb(0.55, 0.35, 0.7);

/// Everything a level is made of, as authored in `assets/data/levels/*.level.ron`.
#[derive(Asset, TypePath, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelDefinition {
    pub difficulties: DifficultySettings,
    pub timeline: Timeline,
    pub map: MapLayout,
    pub farming: FarmingConfig,
}

/// Where things are on the map. Positions are on the ground plane, in world units.
#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapLayout {
    pub path: MonsterPath,
    pub plots: Vec<Vec2>,
    pub peddler: Vec2,
    pub farmer_start: Vec2,
    pub rabbit_home: Vec2,
}

/// The route monsters walk, from the spawner to the crystal. Always has at least two waypoints.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "Vec<Vec2>")]
pub struct MonsterPath(Vec<Vec2>);

impl MonsterPath {
    /// Where monsters appear.
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
}

impl TryFrom<Vec<Vec2>> for MonsterPath {
    type Error = &'static str;

    fn try_from(waypoints: Vec<Vec2>) -> Result<Self, Self::Error> {
        if waypoints.len() < 2 {
            return Err("a monster path needs a spawner and a crystal waypoint");
        }

        Ok(Self(waypoints))
    }
}

/// A spot where the farmer can plant a crop.
#[derive(Component, Debug)]
pub struct PlantPlot;

/// Where monsters appear.
#[derive(Component, Debug)]
pub struct Spawner;

/// The crystal monsters try to destroy.
#[derive(Component, Debug)]
pub struct Crystal;

/// The peddler who buys crops and recruits defenders.
#[derive(Component, Debug)]
pub struct Peddler;

/// The level definition being played.
#[derive(Resource, Debug)]
struct CurrentLevel(Handle<LevelDefinition>);

/// Loads the level definition and spawns the map.
pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RonAssetPlugin::<LevelDefinition>::new(&["level.ron"]))
            .add_systems(Startup, load_level)
            .add_systems(Update, apply_level_definition)
            .add_systems(OnEnter(AppState::InLevel), spawn_map);
    }
}

fn load_level(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(CurrentLevel(asset_server.load(LEVEL_PATH)));
}

/// Shares each section of the level as a resource, on first load and on every hot reload.
fn apply_level_definition(
    mut commands: Commands,
    mut asset_events: MessageReader<AssetEvent<LevelDefinition>>,
    current_level: Res<CurrentLevel>,
    levels: Res<Assets<LevelDefinition>>,
    state: Res<State<AppState>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let reloaded = asset_events.read().any(|event| match event {
        AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => {
            *id == current_level.0.id()
        }
        _ => false,
    });

    if !reloaded {
        return;
    }

    let Some(level) = levels.get(&current_level.0) else {
        return;
    };

    commands.insert_resource(level.difficulties.clone());
    commands.insert_resource(level.timeline.clone());
    commands.insert_resource(level.map.clone());
    commands.insert_resource(level.farming.clone());
    info!("Level definition applied");

    if *state.get() == AppState::Loading {
        next_state.set(AppState::InLevel);
    }
}

fn spawn_map(
    mut commands: Commands,
    layout: Res<MapLayout>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
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
        Mesh3d(meshes.add(Cuboid::from_length(0.9))),
        MeshMaterial3d(materials.add(CRYSTAL_COLOR)),
        Transform::from_translation(layout.path.crystal().on_ground() + Vec3::Y)
            .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.6, 0.0, 0.6)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Difficulty;

    const PROTOTYPE_LEVEL: &str = include_str!("../assets/data/levels/prototype.level.ron");

    fn parse(source: &str) -> ron::error::SpannedResult<LevelDefinition> {
        ron::from_str(source)
    }

    #[test]
    fn prototype_level_has_one_path_three_plots_and_a_timeline() {
        let level = parse(PROTOTYPE_LEVEL).unwrap();

        assert!(level.map.path.waypoints().len() >= 2);
        assert_eq!(level.map.plots.len(), 3);
        assert!(level.timeline.sell_window < level.timeline.length);

        for difficulty in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
            assert!(level.difficulties.starting_resources(difficulty).seeds >= 1);
        }
    }

    #[test]
    fn malformed_level_is_a_parse_error() {
        assert!(parse("(difficulties: (easy: (seeds: 1").is_err());
    }

    #[test]
    fn path_without_a_crystal_waypoint_is_rejected() {
        assert!(ron::from_str::<MonsterPath>("[(0.0, 0.0)]").is_err());
        assert!(ron::from_str::<MonsterPath>("[(0.0, 0.0), (1.0, 0.0)]").is_ok());
    }
}
