//! Heads-up display: the resources and controls the player needs while playing a level.
//!
//! Presentation only: reads the public resources and components of the gameplay plugins and never
//! changes them.

use bevy::prelude::*;

use crate::combat::Health;
use crate::content::Recipes;
use crate::economy::{Gold, Inventory};
use crate::farming::{Farmer, Mana, SeedStock};
use crate::game::{AppState, GameSet};
use crate::map::Crystal;
use crate::recruiting::Placement;

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
    inventory: Res<Inventory>,
    recipes: Res<Recipes>,
    placement: Res<Placement>,
    farmer: Single<&Mana, With<Farmer>>,
    crystal: Query<&Health, With<Crystal>>,
    mut text: Single<
        &mut Text,
        (
            With<ResourcesText>,
            Without<CrystalText>,
            Without<PlacementText>,
        ),
    >,
    mut crystal_text: Single<
        &mut Text,
        (
            With<CrystalText>,
            Without<ResourcesText>,
            Without<PlacementText>,
        ),
    >,
    mut placement_text: Single<
        &mut Text,
        (
            With<PlacementText>,
            Without<ResourcesText>,
            Without<CrystalText>,
        ),
    >,
) {
    let mana = *farmer;
    let crops = recipes
        .crops
        .keys()
        .map(|crop| format!("{} {}", crop.replace('_', " "), inventory.count(crop)))
        .collect::<Vec<_>>()
        .join("   ");

    text.0 = format!(
        "Gold {}   Seeds {}   {}   Mana {:.0}/{:.0}",
        gold.amount(),
        seeds.seeds(),
        crops,
        mana.current(),
        mana.max(),
    );

    if let Ok(health) = crystal.single() {
        crystal_text.0 = format!("Warp Crystal {:.0}/{:.0}", health.current(), health.max());
    }

    placement_text.0 = match &*placement {
        Placement::Idle => String::new(),
        Placement::Placing { defender } => {
            format!("Choose a {} position", defender.replace('_', " "))
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testing::TestApp;

    #[test]
    fn hud_starts_without_conflicting_text_queries() {
        let mut app = App::headless();

        app.add_plugins(HudPlugin)
            .init_resource::<Gold>()
            .init_resource::<SeedStock>()
            .init_resource::<Inventory>()
            .init_resource::<Recipes>()
            .init_resource::<Placement>()
            .enter(AppState::InLevel);
        app.update();
    }
}
