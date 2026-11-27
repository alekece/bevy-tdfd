//! Shared health and damage.
//!
//! Enemies and defenders send [`Damage`] messages; this plugin owns health changes and removes
//! entities when they die. That keeps combatants from reaching into each other's components.

use bevy::prelude::*;

use crate::game::GameSet;

/// An entity's remaining health.
#[derive(Component, Debug)]
pub struct Health {
    current: f32,
    max: f32,
}

impl Health {
    /// Creates an entity with full `max` health.
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    /// Current health.
    pub fn current(&self) -> f32 {
        self.current
    }

    /// Maximum health.
    pub fn max(&self) -> f32 {
        self.max
    }

    fn apply(&mut self, amount: f32) -> bool {
        self.current = (self.current - amount.max(0.0)).max(0.0);

        self.current == 0.0
    }
}

/// A request to damage an entity. Damage is applied during `FixedUpdate`.
#[derive(Message, Debug, Clone, Copy)]
pub struct Damage {
    pub target: Entity,
    pub amount: f32,
}

/// Applies damage and removes defeated entities.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Damage>()
            .add_systems(FixedUpdate, apply_damage.in_set(GameSet::Resolve));
    }
}

fn apply_damage(
    mut commands: Commands,
    mut damage_messages: MessageReader<Damage>,
    mut health: Query<&mut Health>,
) {
    for damage in damage_messages.read() {
        let Ok(mut health) = health.get_mut(damage.target) else {
            continue;
        };

        if health.current() == 0.0 {
            continue;
        }

        if health.apply(damage.amount) {
            commands.entity(damage.target).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{AppState, testing::TestApp};

    #[test]
    fn damage_reduces_health_and_lethal_damage_despawns_entity() {
        let mut app = App::headless();
        app.add_plugins(CombatPlugin).enter(AppState::InLevel);
        let target = app.world_mut().spawn(Health::new(10.0)).id();
        assert_eq!(app.world().get::<Health>(target).unwrap().max(), 10.0);

        app.world_mut().write_message(Damage {
            target,
            amount: 4.0,
        });
        app.advance_by(std::time::Duration::from_secs(1));

        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 6.0);

        app.world_mut().write_message(Damage {
            target,
            amount: 6.0,
        });
        app.advance_by(std::time::Duration::from_secs(1));

        assert!(app.world().get_entity(target).is_err());
    }
}
