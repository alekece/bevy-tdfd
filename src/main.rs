mod combat;
mod defenders;
mod economy;
mod enemies;
mod farming;
mod game;
mod hud;
mod level_flow;
mod map;
mod stage;

use bevy::prelude::*;

use crate::combat::CombatPlugin;
use crate::defenders::DefendersPlugin;
use crate::economy::EconomyPlugin;
use crate::enemies::EnemiesPlugin;
use crate::farming::FarmingPlugin;
use crate::game::GamePlugin;
use crate::hud::HudPlugin;
use crate::level_flow::LevelFlowPlugin;
use crate::map::MapPlugin;
use crate::stage::StagePlugin;

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
