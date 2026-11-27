//! Game flow shared by every gameplay plugin.
//!
//! Owns the app states, the schedule sets gameplay systems join, the level clock and the difficulty
//! choice. Gameplay plugins put their systems in a [`GameSet`] instead of ordering themselves against
//! each other, so adding or removing a plugin never breaks another plugin's ordering.

use std::time::Duration;

use bevy::prelude::*;
use serde::Deserialize;

/// Top-level flow of the game.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    /// Waiting for the level definition to load.
    #[default]
    Loading,
    /// Choosing a difficulty.
    Menu,
    /// Playing a level.
    InLevel,
    /// Showing the outcome of the last level.
    Results,
}

/// How generous the starting resources are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

/// The difficulty the next level starts with.
#[derive(Resource, Debug, Default)]
pub struct SelectedDifficulty(pub Difficulty);

/// Seeds and gold a level starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartingResources {
    pub seeds: u32,
    pub gold: u32,
}

/// Starting resources for each difficulty.
#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifficultySettings {
    easy: StartingResources,
    normal: StartingResources,
    hard: StartingResources,
}

impl DifficultySettings {
    /// Returns the starting resources of `difficulty`.
    pub fn starting_resources(&self, difficulty: Difficulty) -> StartingResources {
        match difficulty {
            Difficulty::Easy => self.easy,
            Difficulty::Normal => self.normal,
            Difficulty::Hard => self.hard,
        }
    }

    /// The same starting resources for every difficulty.
    #[cfg(test)]
    pub fn uniform(resources: StartingResources) -> Self {
        Self {
            easy: resources,
            normal: resources,
            hard: resources,
        }
    }
}

/// The level timeline: waves happen before the final sell window, and the level is won when the
/// whole timeline has elapsed.
#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timeline {
    /// Total level length, sell window included.
    #[serde(with = "humantime_serde")]
    pub length: Duration,
    /// Quiet time at the end of the level to sell or eat the remaining crops.
    #[serde(with = "humantime_serde")]
    pub sell_window: Duration,
}

impl Timeline {
    /// When the final sell window opens.
    pub fn sell_window_start(&self) -> Duration {
        self.length.saturating_sub(self.sell_window)
    }

    /// Whether the level has run its whole timeline at `elapsed`.
    pub fn is_complete(&self, elapsed: Duration) -> bool {
        elapsed >= self.length
    }
}

/// Simulated time since the current level started.
#[derive(Resource, Debug, Default)]
pub struct LevelClock {
    elapsed: Duration,
}

impl LevelClock {
    /// Time elapsed since the level started.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

/// Whether the level was won or lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelOutcome {
    Victory,
    Defeat,
}

/// Whether the level being played still runs. Set to [`LevelStatus::Ended`] on the tick the level
/// ends, so the remaining fixed ticks of that frame simulate nothing before the state changes.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LevelStatus {
    #[default]
    Running,
    Ended(LevelOutcome),
}

/// Schedule sets every gameplay plugin joins. All of them only run while a level is played and
/// [`LevelStatus::Running`].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// `Update`: turn player input into orders.
    Input,
    /// `FixedUpdate`: advance the level clock.
    Clock,
    /// `FixedUpdate`: bring new entities into the level (waves).
    Spawn,
    /// `FixedUpdate`: each agent picks its intent for this tick.
    Decide,
    /// `FixedUpdate`: agents move according to their intent.
    Move,
    /// `FixedUpdate`: entities act (grow, attack, work) and send messages.
    Act,
    /// `FixedUpdate`: apply the consequences of the actions (damage and deaths).
    Resolve,
    /// `FixedUpdate`: check for level completion after damage and deaths.
    Outcome,
    /// `Update`: update visuals and UI from the simulation.
    Presentation,
}

/// Converts a position on the ground plane to a world position.
pub trait GroundPosition {
    /// The world position at ground height.
    fn on_ground(self) -> Vec3;
}

impl GroundPosition for Vec2 {
    fn on_ground(self) -> Vec3 {
        Vec3::new(self.x, 0.0, self.y)
    }
}

/// Owns the game states, the shared schedule sets and the level clock.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<SelectedDifficulty>()
            .init_resource::<LevelClock>()
            .init_resource::<LevelStatus>()
            .configure_sets(
                Update,
                (GameSet::Input, GameSet::Presentation)
                    .chain()
                    .run_if(in_state(AppState::InLevel))
                    .run_if(level_running),
            )
            .configure_sets(
                FixedUpdate,
                (
                    GameSet::Clock,
                    GameSet::Spawn,
                    GameSet::Decide,
                    GameSet::Move,
                    GameSet::Act,
                    GameSet::Resolve,
                    GameSet::Outcome,
                )
                    .chain()
                    .run_if(in_state(AppState::InLevel))
                    .run_if(level_running),
            )
            .add_systems(OnEnter(AppState::InLevel), start_level_clock)
            .add_systems(FixedUpdate, advance_level_clock.in_set(GameSet::Clock));
    }
}

fn level_running(status: Res<LevelStatus>) -> bool {
    *status == LevelStatus::Running
}

fn start_level_clock(mut clock: ResMut<LevelClock>, mut status: ResMut<LevelStatus>) {
    clock.elapsed = Duration::ZERO;
    *status = LevelStatus::Running;
}

fn advance_level_clock(time: Res<Time>, mut clock: ResMut<LevelClock>) {
    clock.elapsed += time.delta();
}

/// Headless app setup shared by gameplay tests.
#[cfg(test)]
pub mod testing {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    use super::*;

    /// One fixed timestep at Bevy's default 64 Hz.
    pub const STEP: Duration = Duration::from_micros(15_625);

    /// Headless test helpers for [`App`].
    pub trait TestApp {
        /// An app with the game flow and asset storage but no window or renderer. Every update advances
        /// time by exactly one fixed timestep.
        fn headless() -> Self;

        /// Switches to `state` and runs the transition.
        fn enter(&mut self, state: AppState) -> &mut Self;

        /// Runs updates until `duration` of simulated time has passed.
        fn advance_by(&mut self, duration: Duration) -> &mut Self;
    }

    impl TestApp for App {
        fn headless() -> Self {
            let mut app = App::new();

            app.add_plugins((
                MinimalPlugins,
                StatesPlugin,
                AssetPlugin::default(),
                GamePlugin,
            ))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(STEP))
            .insert_resource(Time::<Fixed>::from_duration(STEP));
            app.update();

            app
        }

        fn enter(&mut self, state: AppState) -> &mut Self {
            self.world_mut()
                .resource_mut::<NextState<AppState>>()
                .set(state);
            self.update();

            self
        }

        fn advance_by(&mut self, duration: Duration) -> &mut Self {
            let steps = duration.as_micros().div_ceil(STEP.as_micros());

            for _ in 0..steps {
                self.update();
            }

            self
        }
    }
}
