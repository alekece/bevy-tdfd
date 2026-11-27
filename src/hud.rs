//! Heads-up display: the resources and controls the player needs while playing a level.
//!
//! Presentation only: reads the public resources and components of the gameplay plugins and never
//! changes them.

use bevy::prelude::*;

use crate::combat::Health;
use crate::defenders::PlacementMode;
use crate::economy::Gold;
use crate::farming::{Farmer, Mana, Pantry, SeedStock};
use crate::game::{AppState, GameSet};
use crate::map::Crystal;

const CONTROLS: &str = "Right click: walk, plant or harvest  |  G: Growth Boost  |  E: eat  |  S: sell  |  K + left click: hire";

/// The text showing the player's resources.
#[derive(Component, Debug)]
struct ResourcesText;

#[derive(Component, Debug)]
struct CrystalText;

#[derive(Component, Debug)]
struct PlacementText;

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
            (CrystalText, Text::default(), TextFont::from_font_size(16.0)),
            (Text::new(CONTROLS), TextFont::from_font_size(14.0)),
            (
                PlacementText,
                Text::default(),
                TextFont::from_font_size(14.0)
            ),
        ],
    ));
}

fn show_resources(
    gold: Res<Gold>,
    seeds: Res<SeedStock>,
    pantry: Res<Pantry>,
    placement: Res<PlacementMode>,
    farmer: Single<&Mana, With<Farmer>>,
    crystal: Query<&Health, With<Crystal>>,
    mut text: Single<&mut Text, With<ResourcesText>>,
    mut crystal_text: Single<&mut Text, With<CrystalText>>,
    mut placement_text: Single<&mut Text, With<PlacementText>>,
) {
    let mana = *farmer;

    text.0 = format!(
        "Gold {}   Seeds {}   Tomatoes {}   Mana {:.0}/{:.0}",
        gold.amount(),
        seeds.seeds(),
        pantry.tomatoes(),
        mana.current(),
        mana.max(),
    );

    if let Ok(health) = crystal.single() {
        crystal_text.0 = format!("Warp Crystal {:.0}/{:.0}", health.current(), health.max());
    }

    placement_text.0 = if placement.is_active() {
        "Choose a Knight Bunny position".into()
    } else {
        String::new()
    };
}
