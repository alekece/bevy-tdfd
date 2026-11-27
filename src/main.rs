mod game;
mod map;
mod stage;

use bevy::prelude::*;

use crate::game::GamePlugin;
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
        ))
        .run()
}
