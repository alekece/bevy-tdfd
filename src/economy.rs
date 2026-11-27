//! Economy: gold, the inventory and trade prices.
//!
//! Owns the player's [`Gold`], initialized from the selected difficulty, the [`Inventory`] of
//! harvested items and the [`Trade`] prices of a level. Triggering [`SellAll`] (the `S` key) sells
//! every inventory item [`Trade::buys`] lists. Other plugins fill the inventory with
//! [`Inventory::add`] and pay with [`Gold::try_spend`].

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Deserialize;

use crate::game::{AppState, DifficultySettings, GameSet, SelectedDifficulty};

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

/// Items the player holds, by id, in a stable order.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct Inventory(BTreeMap<String, u32>);

impl Inventory {
    /// Adds `amount` of `item`.
    pub fn add(&mut self, item: &str, amount: u32) {
        if amount == 0 {
            return;
        }

        *self.0.entry(item.to_owned()).or_default() += amount;
    }

    /// Removes one `item` if there is any. Returns whether one was removed.
    pub fn take(&mut self, item: &str) -> bool {
        let Some(count) = self.0.get_mut(item) else {
            return false;
        };

        *count -= 1;

        if *count == 0 {
            self.0.remove(item);
        }

        true
    }

    /// How many `item` the player holds.
    pub fn count(&self, item: &str) -> u32 {
        self.0.get(item).copied().unwrap_or_default()
    }

    /// Every held item and its count, in item order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u32)> {
        self.0.iter().map(|(item, count)| (item.as_str(), *count))
    }
}

/// Prices of the level's trader, in gold per item.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trade {
    /// What the trader buys from the player.
    pub buys: BTreeMap<String, u32>,
    /// What the trader sells to the player; the first id is the default offer.
    pub sells: BTreeMap<String, u32>,
}

impl Trade {
    /// Removes every item the trader buys from `inventory`. Returns the gold earned.
    pub fn sell_all(&self, inventory: &mut Inventory) -> u32 {
        let mut earned = 0;

        inventory.0.retain(|item, count| {
            let Some(price) = self.buys.get(item) else {
                return true;
            };

            earned += price * *count;

            false
        });

        earned
    }
}

/// The player sells everything the trader buys.
#[derive(Event, Debug)]
pub struct SellAll;

/// Runs gold, the inventory and trade.
pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gold>()
            .init_resource::<Inventory>()
            .init_resource::<Trade>()
            .add_observer(sell_all)
            .add_systems(OnEnter(AppState::InLevel), reset_purse)
            .add_systems(Update, request_sale.in_set(GameSet::Input));
    }
}

fn reset_purse(
    mut commands: Commands,
    difficulties: Res<DifficultySettings>,
    difficulty: Res<SelectedDifficulty>,
) {
    commands.insert_resource(Gold::with_amount(
        difficulties.starting_resources(difficulty.0).gold,
    ));
    commands.insert_resource(Inventory::default());
}

/// `S`: sell everything the trader buys.
fn request_sale(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyS) {
        commands.trigger(SellAll);
    }
}

fn sell_all(
    _: On<SellAll>,
    trade: Res<Trade>,
    mut inventory: ResMut<Inventory>,
    mut gold: ResMut<Gold>,
) {
    gold.earn(trade.sell_all(&mut inventory));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::StartingResources;
    use crate::game::testing::TestApp;

    fn trade() -> Trade {
        Trade {
            buys: BTreeMap::from([("a".to_owned(), 5), ("b".to_owned(), 2)]),
            sells: BTreeMap::new(),
        }
    }

    #[test]
    fn selling_uses_trade_prices() {
        let mut inventory = Inventory::default();
        inventory.add("a", 3);
        inventory.add("b", 1);
        inventory.add("c", 4);

        let earned = trade().sell_all(&mut inventory);

        assert_eq!(earned, 17);
        assert_eq!(inventory.iter().collect::<Vec<_>>(), [("c", 4)]);
    }

    #[test]
    fn selling_consumes_inventory_and_adds_gold() {
        let mut app = App::headless();
        app.add_plugins(EconomyPlugin)
            .insert_resource(trade())
            .insert_resource(DifficultySettings::uniform(StartingResources {
                seeds: 0,
                gold: 10,
            }))
            .enter(AppState::InLevel);

        app.world_mut().resource_mut::<Inventory>().add("a", 3);
        app.world_mut().trigger(SellAll);

        assert_eq!(app.world().resource::<Inventory>().count("a"), 0);
        assert_eq!(app.world().resource::<Gold>().amount(), 25);
    }

    #[test]
    fn taking_the_last_item_removes_it() {
        let mut inventory = Inventory::default();
        inventory.add("a", 1);

        assert!(inventory.take("a"));
        assert!(!inventory.take("a"));
        assert_eq!(inventory.iter().count(), 0);
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
