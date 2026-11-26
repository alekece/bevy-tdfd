//! Placeholder stage: a camera looking down at an angle, a light and a ground plane.
//!
//! It proves the window, renderer and plugin wiring work. Gameplay plugins replace it.

use bevy::prelude::*;

const GROUND_SIZE: f32 = 20.0;
const GROUND_COLOR: Color = Color::srgb(0.45, 0.7, 0.35);

/// Spawns the placeholder stage when the app starts.
pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_stage);
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
