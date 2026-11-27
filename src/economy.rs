//! Economy: gold and the peddler.
//!
//! Owns the player's [`Gold`], initialized from the selected difficulty, and crop prices. The peddler
//! buys every tomato in the farming [`Pantry`] when the player triggers [`SellTomatoes`]. Other
//! plugins pay with [`Gold::try_spend`].

use bevy::prelude::*;
use serde::Deserialize;

use crate::farming::Pantry;
use crate::game::{AppState, DifficultySettings, GameSet, SelectedDifficulty};

/// Economy values of a level.
#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EconomyConfig {
    /// Gold the peddler pays per tomato.
    pub tomato_price: u32,
    /// Cost of hiring one Knight Bunny.
    pub knight_bunny_cost: u32,
}

/// The player's gold.
#[derive(Resource, Debug, Default)]
pub struct Gold {
    amount: u32,
}

impl Gold {
    /// Creates a gold purse with `amount`.
    pub fn with_amount(amount: u32) -> Self {
        Self { amount }
    }

    pub fn amount(&self) -> u32 {
        self.amount
    }

    /// Spends `cost` if there is enough gold. Returns whether it was spent.
    pub fn try_spend(&mut self, cost: u32) -> bool {
        let Some(amount) = self.amount.checked_sub(cost) else {
            return false;
        };

        self.amount = amount;

        true
    }

    fn earn(&mut self, amount: u32) {
        self.amount += amount;
    }
}

/// The player sells every harvested tomato to the peddler.
#[derive(Event, Debug)]
pub struct SellTomatoes;

/// Runs gold and the peddler's trade.
pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gold>()
            .add_observer(sell_tomatoes)
            .add_systems(OnEnter(AppState::InLevel), reset_gold)
            .add_systems(Update, request_sale.in_set(GameSet::Input));
    }
}

fn reset_gold(
    mut commands: Commands,
    difficulties: Res<DifficultySettings>,
    difficulty: Res<SelectedDifficulty>,
) {
    commands.insert_resource(Gold::with_amount(
        difficulties.starting_resources(difficulty.0).gold,
    ));
}

/// `S`: sell tomatoes to the peddler.
fn request_sale(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyS) {
        commands.trigger(SellTomatoes);
    }
}

fn sell_tomatoes(
    _: On<SellTomatoes>,
    config: Res<EconomyConfig>,
    mut pantry: ResMut<Pantry>,
    mut gold: ResMut<Gold>,
) {
    let tomatoes = pantry.tomatoes();

    if pantry.take(tomatoes) {
        gold.earn(tomatoes * config.tomato_price);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::StartingResources;
    use crate::game::testing::TestApp;

    fn economy_app(gold: u32) -> App {
        let mut app = App::headless();

        app.add_plugins(EconomyPlugin)
            .init_resource::<Pantry>()
            .insert_resource(EconomyConfig {
                tomato_price: 5,
                knight_bunny_cost: 5,
            })
            .insert_resource(DifficultySettings::uniform(StartingResources {
                seeds: 0,
                gold,
            }))
            .enter(AppState::InLevel);

        app
    }

    #[test]
    fn selling_crop_consumes_inventory_and_adds_gold() {
        let mut app = economy_app(10);

        app.world_mut().resource_mut::<Pantry>().store(3);
        app.world_mut().trigger(SellTomatoes);

        assert_eq!(app.world().resource::<Pantry>().tomatoes(), 0);
        assert_eq!(app.world().resource::<Gold>().amount(), 25);
    }

    #[test]
    fn spending_more_gold_than_owned_changes_nothing() {
        let mut gold = Gold { amount: 10 };

        assert!(!gold.try_spend(11));
        assert_eq!(gold.amount(), 10);
        assert!(gold.try_spend(10));
        assert_eq!(gold.amount(), 0);
    }
}
