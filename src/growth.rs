//! Growth: things that ripen over time and can be harvested.
//!
//! Owns [`Growth`], the temporary [`Boosted`] speed-up, the [`Yield`] a harvest gives and the
//! [`GrowthLook`] swapped when ripe. Growth advances in [`GrowthSet`] inside [`GameSet::Act`];
//! harvesting systems run after it, so whatever ripens this tick can be harvested this tick.

use std::time::Duration;

use bevy::prelude::*;

use crate::game::GameSet;

/// Smallest scale of an entity that just started growing; it reaches full scale when ripe.
const SEEDLING_SCALE: f32 = 0.4;

/// Progress towards ripeness.
#[derive(Component, Debug)]
pub struct Growth {
    timer: Timer,
}

impl Growth {
    /// Starts growing; ripe after `growth_time`.
    pub fn new(growth_time: Duration) -> Self {
        Self {
            timer: Timer::new(growth_time, TimerMode::Once),
        }
    }

    /// Advances growth by `delta`.
    pub fn advance(&mut self, delta: Duration) {
        self.timer.tick(delta);
    }

    pub fn is_ripe(&self) -> bool {
        self.timer.is_finished()
    }

    /// Progress from 0 (just planted) to 1 (ripe).
    pub fn progress(&self) -> f32 {
        self.timer.fraction()
    }

    /// Picks a ripe crop and sends it back to its first growth stage. Returns whether there was
    /// anything to pick.
    pub fn harvest(&mut self) -> bool {
        if !self.is_ripe() {
            return false;
        }

        self.timer.reset();

        true
    }
}

/// Faster growth for a while.
#[derive(Component, Debug)]
pub struct Boosted {
    multiplier: f32,
    timer: Timer,
}

impl Boosted {
    /// Grows `multiplier` times faster for `duration`.
    pub fn new(multiplier: f32, duration: Duration) -> Self {
        Self {
            multiplier,
            timer: Timer::new(duration, TimerMode::Once),
        }
    }
}

/// What one harvest gives.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Yield {
    /// Inventory item gained.
    pub item: String,
    /// Items gained per harvest.
    pub amount: u32,
}

/// Materials shown while growing and once ripe.
#[derive(Component, Debug, Clone)]
pub struct GrowthLook {
    pub growing: Handle<StandardMaterial>,
    pub ripe: Handle<StandardMaterial>,
}

/// The set growth advances in, inside [`GameSet::Act`].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GrowthSet;

/// Grows entities with [`Growth`] and shows their progress.
pub struct GrowthPlugin;

impl Plugin for GrowthPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(FixedUpdate, GrowthSet.in_set(GameSet::Act))
            .add_systems(FixedUpdate, grow.in_set(GrowthSet))
            .add_systems(Update, show_growth.in_set(GameSet::Presentation));
    }
}

fn grow(
    mut commands: Commands,
    time: Res<Time>,
    mut growing: Query<(Entity, &mut Growth, Option<&mut Boosted>)>,
) {
    for (entity, mut growth, boosted) in &mut growing {
        let Some(mut boosted) = boosted else {
            growth.advance(time.delta());

            continue;
        };

        growth.advance(time.delta().mul_f32(boosted.multiplier));

        if boosted.timer.tick(time.delta()).is_finished() {
            commands.entity(entity).remove::<Boosted>();
        }
    }
}

fn show_growth(
    mut growing: Query<(
        &Growth,
        &GrowthLook,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (growth, look, mut transform, mut material) in &mut growing {
        transform.scale = Vec3::splat(SEEDLING_SCALE + (1.0 - SEEDLING_SCALE) * growth.progress());

        let wanted = if growth.is_ripe() {
            &look.ripe
        } else {
            &look.growing
        };

        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::AppState;
    use crate::game::testing::TestApp;

    #[test]
    fn boost_speeds_up_growth_until_it_expires() {
        let mut app = App::headless();
        app.add_plugins(GrowthPlugin).enter(AppState::InLevel);
        let plain = app
            .world_mut()
            .spawn(Growth::new(Duration::from_secs(10)))
            .id();
        let boosted = app
            .world_mut()
            .spawn((
                Growth::new(Duration::from_secs(10)),
                Boosted::new(3.0, Duration::from_secs(1)),
            ))
            .id();

        app.advance_by(Duration::from_secs(2));

        let progress = |entity| app.world().get::<Growth>(entity).unwrap().progress();
        assert!((progress(plain) - 0.2).abs() < 0.01);
        assert!((progress(boosted) - 0.4).abs() < 0.01);
        assert!(app.world().get::<Boosted>(boosted).is_none());
    }

    #[test]
    fn harvest_needs_ripeness_and_restarts_growth() {
        let mut growth = Growth::new(Duration::from_secs(1));

        assert!(!growth.harvest());

        growth.advance(Duration::from_secs(1));

        assert!(growth.harvest());
        assert_eq!(growth.progress(), 0.0);
    }
}
