mod economy;
mod farming;
mod game;
mod hud;
mod map;
mod stage;

use bevy::prelude::*;

use crate::economy::EconomyPlugin;
use crate::farming::FarmingPlugin;
use crate::game::GamePlugin;
use crate::hud::HudPlugin;
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
            HudPlugin,
        ))
        .run()
}
