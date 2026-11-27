//! Heads-up display: the resources and controls the player needs while playing a level.
//!
//! Presentation only: reads the public resources and components of the gameplay plugins and never
//! changes them.

use bevy::prelude::*;

use crate::farming::{Farmer, Mana, Pantry, SeedStock};
use crate::game::{AppState, GameSet};

const CONTROLS: &str =
    "Right click: walk, plant or harvest  |  G: Growth Boost  |  E: eat a tomato";

/// The text showing the player's resources.
#[derive(Component, Debug)]
struct ResourcesText;

/// Shows resources and controls during a level.
pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InLevel), spawn_hud)
            .add_systems(Update, show_resources.in_set(GameSet::Presentation));
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Name::new("HUD"),
        Node {
            position_type: PositionType::Absolute,
            top: px(10),
            left: px(10),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        },
        DespawnOnExit(AppState::InLevel),
        children![
            (
                ResourcesText,
                Text::default(),
                TextFont::from_font_size(20.0)
            ),
            (Text::new(CONTROLS), TextFont::from_font_size(14.0)),
        ],
    ));
}

fn show_resources(
    seeds: Res<SeedStock>,
    pantry: Res<Pantry>,
    farmer: Single<&Mana, With<Farmer>>,
    mut text: Single<&mut Text, With<ResourcesText>>,
) {
    let mana = *farmer;

    text.0 = format!(
        "Seeds {}   Tomatoes {}   Mana {:.0}/{:.0}",
        seeds.seeds(),
        pantry.tomatoes(),
        mana.current(),
        mana.max(),
    );
}
