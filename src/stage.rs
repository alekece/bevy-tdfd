//! The stage: a camera looking down at an angle, a light and a ground plane.
//!
//! Also tracks where the mouse cursor points on the ground, so input systems of other plugins can
//! read [`CursorGround`] instead of querying the camera themselves.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::game::GameSet;

const GROUND_SIZE: f32 = 20.0;
/// World units the camera always keeps visible horizontally and vertically; the ground looks shorter
/// vertically because the camera looks down at an angle.
const VIEW_WIDTH: f32 = 21.0;
const VIEW_HEIGHT: f32 = 16.5;
const GROUND_COLOR: Color = Color::srgb(0.45, 0.7, 0.35);

/// The point on the ground under the mouse cursor, if any.
#[derive(Resource, Debug, Default)]
pub struct CursorGround(pub Option<Vec2>);

/// Spawns the stage when the app starts and tracks the cursor on the ground.
pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorGround>()
            .add_systems(Startup, spawn_stage)
            .add_systems(Update, track_cursor.before(GameSet::Input));
    }
}

// Bevy systems are plain functions by design; this is the framework exception to the free-function rule.
fn spawn_stage(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        // Orthographic keeps the whole field visible in any window shape, a common choice for tower
        // defense where the player needs to see every lane at once.
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: VIEW_WIDTH,
                min_height: VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 14.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Name::new("Sun"),
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Name::new("Ground"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(GROUND_COLOR)),
    ));
}

fn track_cursor(
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut cursor_ground: ResMut<CursorGround>,
) {
    let (camera, camera_transform) = *camera;

    cursor_ground.0 = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor).ok())
        .and_then(|ray| ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)))
        .map(|point| point.xz());
}
