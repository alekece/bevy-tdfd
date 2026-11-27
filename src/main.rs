mod combat;
mod defenders;
mod economy;
mod enemies;
mod farming;
mod game;
mod guarding;
mod hud;
mod level_flow;
mod map;
mod movement;
mod stage;
mod targeting;

use bevy::prelude::*;

use crate::combat::CombatPlugin;
use crate::defenders::DefendersPlugin;
use crate::economy::EconomyPlugin;
use crate::enemies::EnemiesPlugin;
use crate::farming::FarmingPlugin;
use crate::game::GamePlugin;
use crate::guarding::GuardingPlugin;
use crate::hud::HudPlugin;
use crate::level_flow::LevelFlowPlugin;
use crate::map::MapPlugin;
use crate::movement::MovementPlugin;
use crate::stage::StagePlugin;
use crate::targeting::TargetingPlugin;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Top Dungeon Farmer Defense".into(),
                    ..default()
                }),
                ..default()
            }),
            GamePlugin,
            StagePlugin,
            MapPlugin,
            MovementPlugin,
            TargetingPlugin,
            GuardingPlugin,
            FarmingPlugin,
            EconomyPlugin,
            CombatPlugin,
            EnemiesPlugin,
            DefendersPlugin,
            HudPlugin,
            LevelFlowPlugin,
        ))
        .run()
}
