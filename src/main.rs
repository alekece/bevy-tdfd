mod stage;

use bevy::prelude::*;

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
            StagePlugin,
        ))
        .run()
}
