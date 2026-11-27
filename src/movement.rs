//! Movement on the ground plane.
//!
//! Owns [`Speed`], the [`Intent`] every agent decides each tick, and [`FollowPath`] for agents that
//! walk the level path stored in [`PathWaypoints`]. Deciding systems of other plugins write
//! [`Intent`] in [`GameSet::Decide`]; [`move_agents`] carries it out in [`MovementSet`], inside
//! [`GameSet::Move`]. Systems that constrain positions after movement run after [`MovementSet`].
//! An idle path follower walks to its next waypoint after targeting had its chance
//! ([`TargetingSet`]).

use bevy::prelude::*;

use crate::game::{GameSet, GroundPosition};
use crate::targeting::TargetingSet;

/// How close a path follower gets to a waypoint before heading to the next one, in world units.
const WAYPOINT_REACH: f32 = 0.65;

/// Walking speed, in world units per second.
#[derive(Component, Debug, Clone, Copy)]
pub struct Speed(pub f32);

/// What an agent wants to do this tick. Movement carries it out; other behaviours read it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub enum Intent {
    /// Stay in place.
    #[default]
    Idle,
    /// Walk to a point on the ground, then become idle.
    MoveTo(Vec2),
    /// Walk towards an entity and stop `within` world units of it.
    Approach { target: Entity, within: f32 },
}

impl Intent {
    /// The entity this intent approaches, if any.
    pub fn target(&self) -> Option<Entity> {
        match self {
            Intent::Approach { target, .. } => Some(*target),
            Intent::Idle | Intent::MoveTo(_) => None,
        }
    }
}

/// Walks the level path, waypoint by waypoint, whenever it has nothing better to do.
#[derive(Component, Debug, Clone, Copy)]
pub struct FollowPath {
    /// Index of the waypoint the agent walks to.
    pub next: usize,
}

impl Default for FollowPath {
    /// Starts towards the second waypoint, as followers appear at the first one.
    fn default() -> Self {
        Self { next: 1 }
    }
}

/// The waypoints path followers walk, in order, on the ground plane.
#[derive(Resource, Debug, Clone, Default)]
pub struct PathWaypoints(pub Vec<Vec2>);

/// The set [`move_agents`] runs in, inside [`GameSet::Move`].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MovementSet;

/// Moves a walker on the ground plane.
pub trait GroundMovement {
    /// Moves at most `max_distance` towards `target`, facing it, and stops `stop_distance` away from it.
    /// Returns whether the walker is within `stop_distance` of the target.
    fn move_towards(&mut self, target: Vec2, max_distance: f32, stop_distance: f32) -> bool;
}

impl GroundMovement for Transform {
    fn move_towards(&mut self, target: Vec2, max_distance: f32, stop_distance: f32) -> bool {
        let position = self.translation.xz();
        let remaining = position.distance(target) - stop_distance;

        if remaining <= 0.0 {
            return true;
        }

        let direction = (target - position).normalize();
        let step = direction * remaining.min(max_distance);

        self.translation += step.on_ground();
        self.look_to(direction.on_ground(), Vec3::Y);

        remaining <= max_distance
    }
}

/// Moves agents according to their [`Intent`] and walks path followers along the level path.
pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PathWaypoints>()
            .configure_sets(FixedUpdate, MovementSet.in_set(GameSet::Move))
            .add_systems(
                FixedUpdate,
                (
                    follow_path.in_set(GameSet::Decide).after(TargetingSet),
                    move_agents.in_set(MovementSet),
                ),
            );
    }
}

/// Sends idle or walking path followers to their next waypoint; approaching agents keep their target.
fn follow_path(
    waypoints: Res<PathWaypoints>,
    mut followers: Query<(&Transform, &mut FollowPath, &mut Intent)>,
) {
    for (transform, mut path, mut intent) in &mut followers {
        if matches!(*intent, Intent::Approach { .. }) {
            continue;
        }

        let position = transform.translation.xz();

        while let Some(waypoint) = waypoints.0.get(path.next)
            && position.distance(*waypoint) <= WAYPOINT_REACH
        {
            path.next += 1;
        }

        let next = waypoints
            .0
            .get(path.next)
            .map_or(Intent::Idle, |waypoint| Intent::MoveTo(*waypoint));

        intent.set_if_neq(next);
    }
}

/// Carries out every agent's [`Intent`]. An agent whose target no longer exists becomes idle.
fn move_agents(
    time: Res<Time>,
    mut agents: Query<(Entity, &Speed, &mut Intent)>,
    mut transforms: Query<&mut Transform>,
) {
    for (agent, speed, mut intent) in &mut agents {
        let step = speed.0 * time.delta_secs();

        let (destination, stop_distance) = match *intent {
            Intent::Idle => continue,
            Intent::MoveTo(point) => (point, 0.0),
            Intent::Approach { target, within } => {
                let Ok(target) = transforms.get(target) else {
                    *intent = Intent::Idle;

                    continue;
                };

                (target.translation.xz(), within)
            }
        };

        let Ok(mut transform) = transforms.get_mut(agent) else {
            continue;
        };

        let arrived = transform.move_towards(destination, step, stop_distance);

        if arrived && matches!(*intent, Intent::MoveTo(_)) {
            *intent = Intent::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::game::AppState;
    use crate::game::testing::TestApp;

    #[test]
    fn follower_walks_waypoints_then_idles() {
        let mut app = App::headless();
        app.add_plugins(MovementPlugin)
            .insert_resource(PathWaypoints(vec![
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 2.0),
            ]))
            .enter(AppState::InLevel);
        let follower = app
            .world_mut()
            .spawn((
                Speed(4.0),
                Intent::Idle,
                FollowPath::default(),
                Transform::default(),
            ))
            .id();

        app.advance_by(Duration::from_millis(500));

        let position = app
            .world()
            .get::<Transform>(follower)
            .unwrap()
            .translation
            .xz();
        assert!(position.x > 1.0 && position.y > 0.0, "{position}");

        app.advance_by(Duration::from_secs(2));

        let position = app
            .world()
            .get::<Transform>(follower)
            .unwrap()
            .translation
            .xz();
        assert!(position.distance(Vec2::new(2.0, 2.0)) <= WAYPOINT_REACH);
        assert_eq!(*app.world().get::<Intent>(follower).unwrap(), Intent::Idle);
    }
}
