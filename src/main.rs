mod combat;
mod content;
mod economy;
mod farming;
mod game;
mod growth;
mod guarding;
mod hud;
mod level_flow;
mod map;
mod movement;
mod recruiting;
mod stage;
mod targeting;
mod waves;

use bevy::prelude::*;

use crate::combat::CombatPlugin;
use crate::content::ContentPlugin;
use crate::economy::EconomyPlugin;
use crate::farming::FarmingPlugin;
use crate::game::GamePlugin;
use crate::growth::GrowthPlugin;
use crate::guarding::GuardingPlugin;
use crate::hud::HudPlugin;
use crate::level_flow::LevelFlowPlugin;
use crate::map::MapPlugin;
use crate::movement::MovementPlugin;
use crate::recruiting::RecruitingPlugin;
use crate::stage::StagePlugin;
use crate::targeting::TargetingPlugin;
use crate::waves::WavesPlugin;

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
            (GamePlugin, StagePlugin, MapPlugin, ContentPlugin),
            MovementPlugin,
            TargetingPlugin,
            GuardingPlugin,
            CombatPlugin,
            GrowthPlugin,
            EconomyPlugin,
            WavesPlugin,
            FarmingPlugin,
            RecruitingPlugin,
            HudPlugin,
            LevelFlowPlugin,
        ))
        .run()
}
