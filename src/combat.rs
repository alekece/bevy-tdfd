//! Health, melee attacks and damage.
//!
//! Owns [`Health`], [`Melee`] and the [`Damage`] message. An agent with [`Melee`] whose movement
//! [`Intent`] approaches a target in reach strikes it in [`GameSet::Act`]; damage is applied and dead
//! entities are despawned in [`GameSet::Resolve`]. Attackers never touch their target's components
//! directly.

use std::time::Duration;

use bevy::prelude::*;

use crate::game::GameSet;
use crate::movement::Intent;

/// Slack on melee reach for float error once an attacker stopped at its range, in world units.
const REACH_TOLERANCE: f32 = 0.01;

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

/// A close-range attack that hits the approached target at a fixed interval.
#[derive(Component, Debug, Clone)]
pub struct Melee {
    /// Health removed per hit.
    pub damage: f32,
    /// Distance at which the attack hits, in world units.
    pub range: f32,
    /// Time between hits; only advances while the target is in reach.
    pub timer: Timer,
}

impl Melee {
    /// A melee attack that first hits `interval` after its target comes in reach.
    pub fn new(damage: f32, range: f32, interval: Duration) -> Self {
        Self {
            damage,
            range,
            timer: Timer::new(interval, TimerMode::Repeating),
        }
    }

    /// Whether a target `distance` world units away is in reach.
    pub fn reaches(&self, distance: f32) -> bool {
        distance <= self.range + REACH_TOLERANCE
    }
}

/// Runs melee attacks, applies damage and removes defeated entities.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Damage>().add_systems(
            FixedUpdate,
            (
                strike.in_set(GameSet::Act),
                apply_damage.in_set(GameSet::Resolve),
            ),
        );
    }
}

fn strike(
    time: Res<Time>,
    mut damage: MessageWriter<Damage>,
    mut attackers: Query<(&Transform, &Intent, &mut Melee)>,
    targets: Query<&Transform>,
) {
    for (transform, intent, mut melee) in &mut attackers {
        let Some(target) = intent.target() else {
            continue;
        };

        let Ok(target_transform) = targets.get(target) else {
            continue;
        };

        let distance = transform
            .translation
            .xz()
            .distance(target_transform.translation.xz());

        if !melee.reaches(distance) {
            continue;
        }

        if melee.timer.tick(time.delta()).just_finished() {
            damage.write(Damage {
                target,
                amount: melee.damage,
            });
        }
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
        app.advance_by(Duration::from_secs(1));

        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 6.0);

        app.world_mut().write_message(Damage {
            target,
            amount: 6.0,
        });
        app.advance_by(Duration::from_secs(1));

        assert!(app.world().get_entity(target).is_err());
    }

    #[test]
    fn melee_timer_only_advances_in_reach() {
        let mut app = App::headless();
        app.add_plugins(CombatPlugin).enter(AppState::InLevel);
        let target = app
            .world_mut()
            .spawn((Health::new(10.0), Transform::from_xyz(5.0, 0.0, 0.0)))
            .id();
        let attacker = app
            .world_mut()
            .spawn((
                Melee::new(1.0, 1.0, Duration::from_secs(1)),
                Intent::Approach {
                    target,
                    within: 1.0,
                },
                Transform::default(),
            ))
            .id();

        app.advance_by(Duration::from_secs(2));
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 10.0);
        assert_eq!(
            app.world().get::<Melee>(attacker).unwrap().timer.elapsed(),
            Duration::ZERO
        );

        app.world_mut()
            .get_mut::<Transform>(target)
            .unwrap()
            .translation
            .x = 1.0;
        app.advance_by(Duration::from_millis(900));
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 10.0);

        app.advance_by(Duration::from_millis(200));
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 9.0);
    }
}
