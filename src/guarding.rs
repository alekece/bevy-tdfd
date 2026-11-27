//! Guarding: agents that defend an area around a home point.
//!
//! Owns [`Guard`] and the area ring shown on the ground. A guard whose movement [`Intent`] has
//! nothing to do after targeting ([`TargetingSet`]) walks home, and after movement ([`MovementSet`])
//! it is held inside its area. Each guard gets a ring entity tied to it with the [`AreaRingOf`]
//! relationship, so despawning the guard despawns its ring. Placement rules use [`Guard::overlaps`].

use bevy::prelude::*;

use crate::game::{GameSet, GroundPosition};
use crate::movement::{Intent, MovementSet};
use crate::targeting::TargetingSet;

/// How far from home a guard still counts as home, in world units.
const HOME_REACH: f32 = 0.05;
/// Height of the ring above the ground, so it does not flicker with it.
const RING_HEIGHT: f32 = 0.02;
const RING_COLOR: Color = Color::srgba(0.9, 0.8, 0.35, 0.2);

/// Defends the area of `radius` world units around `home`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Guard {
    /// Center of the guarded area, on the ground plane.
    pub home: Vec2,
    /// Radius of the guarded area, in world units.
    pub radius: f32,
}

impl Guard {
    /// Whether an area of `radius` around `home` overlaps this guard's area. Touching areas do not
    /// overlap.
    pub fn overlaps(&self, home: Vec2, radius: f32) -> bool {
        self.home.distance(home) < self.radius + radius
    }

    /// The point inside the area closest to `position`.
    pub fn clamp(&self, position: Vec2) -> Vec2 {
        let offset = position - self.home;

        if offset.length() <= self.radius {
            return position;
        }

        self.home + offset.normalize() * self.radius
    }
}

/// The ring entity showing the area of the guard it points to.
#[derive(Component, Debug)]
#[relationship(relationship_target = AreaRings)]
pub struct AreaRingOf(pub Entity);

/// The area ring of a guard; despawned with it.
#[derive(Component, Debug)]
#[relationship_target(relationship = AreaRingOf, linked_spawn)]
pub struct AreaRings(Vec<Entity>);

/// Mesh and material shared by every area ring. The mesh has a radius of one world unit and is
/// scaled to the guard's radius.
#[derive(Resource, Debug)]
pub struct AreaRingAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

impl FromWorld for AreaRingAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Torus::new(0.94, 1.0));
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: RING_COLOR,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });

        Self { mesh, material }
    }
}

/// Keeps guards home and inside their area, and shows their area.
pub struct GuardingPlugin;

impl Plugin for GuardingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AreaRingAssets>()
            .add_observer(spawn_area_ring)
            .add_systems(
                FixedUpdate,
                (
                    return_home.in_set(GameSet::Decide).after(TargetingSet),
                    stay_in_area.in_set(GameSet::Move).after(MovementSet),
                ),
            );
    }
}

fn spawn_area_ring(
    add: On<Add<Guard>>,
    mut commands: Commands,
    guards: Query<&Guard>,
    assets: Res<AreaRingAssets>,
) {
    let Ok(guard) = guards.get(add.entity) else {
        return;
    };

    commands.spawn((
        Name::new("Guard area"),
        AreaRingOf(add.entity),
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material.clone()),
        Transform::from_translation(guard.home.on_ground() + Vec3::Y * RING_HEIGHT)
            .with_scale(Vec3::splat(guard.radius)),
    ));
}

fn return_home(mut guards: Query<(&Guard, &Transform, &mut Intent)>) {
    for (guard, transform, mut intent) in &mut guards {
        if *intent != Intent::Idle || transform.translation.xz().distance(guard.home) <= HOME_REACH
        {
            continue;
        }

        *intent = Intent::MoveTo(guard.home);
    }
}

fn stay_in_area(mut guards: Query<(&Guard, &mut Transform)>) {
    for (guard, mut transform) in &mut guards {
        let position = transform.translation.xz();
        let clamped = guard.clamp(position);

        if clamped != position {
            transform.translation.x = clamped.x;
            transform.translation.z = clamped.y;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::game::AppState;
    use crate::game::testing::TestApp;
    use crate::movement::{MovementPlugin, Speed};

    fn guarding_app() -> App {
        let mut app = App::headless();
        app.add_plugins((MovementPlugin, GuardingPlugin))
            .enter(AppState::InLevel);

        app
    }

    fn guard() -> Guard {
        Guard {
            home: Vec2::ZERO,
            radius: 3.0,
        }
    }

    #[test]
    fn touching_areas_do_not_overlap() {
        assert!(!guard().overlaps(Vec2::new(6.0, 0.0), 3.0));
        assert!(guard().overlaps(Vec2::new(5.9, 0.0), 3.0));
        assert!(!guard().overlaps(Vec2::new(4.0, 0.0), 1.0));
    }

    #[test]
    fn guard_returns_home_and_stays_in_area() {
        let mut app = guarding_app();
        let target = app
            .world_mut()
            .spawn(Transform::from_xyz(8.0, 0.0, 0.0))
            .id();
        let agent = app
            .world_mut()
            .spawn((
                guard(),
                Speed(4.0),
                Intent::Approach {
                    target,
                    within: 0.5,
                },
                Transform::default(),
            ))
            .id();

        app.advance_by(Duration::from_secs(2));
        let position = app
            .world()
            .get::<Transform>(agent)
            .unwrap()
            .translation
            .xz();
        assert!((position.length() - 3.0).abs() < 0.01, "{position}");

        app.world_mut().despawn(target);
        app.advance_by(Duration::from_secs(2));
        let position = app
            .world()
            .get::<Transform>(agent)
            .unwrap()
            .translation
            .xz();
        assert!(position.length() <= HOME_REACH, "{position}");
    }

    #[test]
    fn guard_ring_despawns_with_guard() {
        let mut app = guarding_app();
        let agent = app.world_mut().spawn((guard(), Transform::default())).id();
        app.update();

        let ring = app.world().get::<AreaRings>(agent).unwrap().0[0];
        assert_eq!(
            app.world().get::<Transform>(ring).unwrap().scale,
            Vec3::splat(3.0)
        );

        app.world_mut().despawn(agent);

        assert!(app.world().get_entity(ring).is_err());
    }
}
