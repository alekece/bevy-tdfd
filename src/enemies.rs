//! Green ants: waves, path-following and Eater behavior.
//!
//! Ants follow the configured path toward the crystal. They divert to the nearest crop within the
//! configured distance of their current position, destroy it, then resume toward their next
//! waypoint. They do not target rabbits or defenders in this prototype.

use std::time::Duration;

use bevy::prelude::*;
use serde::Deserialize;

use crate::combat::{Damage, Health};
use crate::farming::{Crop, Threat};
use crate::game::{AppState, GameSet, GroundMovement, GroundPosition, LevelClock};
use crate::map::{Crystal, MapLayout, Spawner};

const ANT_COLOR: Color = Color::srgb(0.1, 0.48, 0.16);
const REACH: f32 = 0.65;

/// Enemy values and spawn times for a level.
#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnemiesConfig {
    pub ant_health: f32,
    pub ant_damage: f32,
    /// Ant walking speed, in world units per second.
    pub ant_speed: f32,
    /// Distance from a walking ant at which a crop can draw its attention, in world units.
    pub crop_detection_radius: f32,
    #[serde(with = "humantime_serde")]
    pub attack_interval: Duration,
    pub waves: Vec<AntWave>,
}

/// A group of green ants that appears at a particular level time.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AntWave {
    #[serde(with = "humantime_serde")]
    pub at: Duration,
    pub count: u32,
}

/// A green ant that walks the level path.
#[derive(Component, Debug)]
pub struct Ant {
    next_waypoint: usize,
    target: Option<AntTarget>,
    attack: Timer,
}

impl Ant {
    fn new(attack_interval: Duration) -> Self {
        Self {
            next_waypoint: 1,
            target: None,
            attack: Timer::new(attack_interval, TimerMode::Repeating),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_ant() -> Self {
        Self::new(Duration::from_secs(1))
    }
}

#[derive(Debug, Clone, Copy)]
enum AntTarget {
    Crop(Entity),
    Crystal(Entity),
}

#[derive(Resource, Debug, Default)]
struct WaveProgress(usize);

#[derive(Resource, Debug)]
struct EnemyAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

impl FromWorld for EnemyAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Sphere::new(0.35).mesh().ico(1).expect("valid subdivisions"));

        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(ANT_COLOR);

        Self { mesh, material }
    }
}

/// Spawns and advances green ants.
pub struct EnemiesPlugin;

impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaveProgress>()
            .init_resource::<EnemyAssets>()
            .add_systems(OnEnter(AppState::InLevel), reset_waves)
            .add_systems(
                FixedUpdate,
                (spawn_due_waves, advance_ants).in_set(GameSet::Act),
            );
    }
}

fn reset_waves(mut progress: ResMut<WaveProgress>) {
    progress.0 = 0;
}

fn spawn_due_waves(
    mut commands: Commands,
    clock: Res<LevelClock>,
    config: Res<EnemiesConfig>,
    layout: Res<MapLayout>,
    spawners: Query<&Transform, (With<Spawner>, Without<Ant>)>,
    assets: Res<EnemyAssets>,
    mut progress: ResMut<WaveProgress>,
) {
    let position = spawners.single().map_or_else(
        |_| layout.path.spawner(),
        |spawner| spawner.translation.xz(),
    );

    while let Some(wave) = config.waves.get(progress.0)
        && clock.elapsed() >= wave.at
    {
        for index in 0..wave.count {
            let offset = Vec2::new(index as f32 * 0.5, 0.0);

            commands.spawn((
                Name::new("Green ant"),
                Ant::new(config.attack_interval),
                Threat,
                Health::new(config.ant_health),
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                Transform::from_translation((position + offset).on_ground() + Vec3::Y * 0.35),
                DespawnOnExit(AppState::InLevel),
            ));
        }

        progress.0 += 1;
    }
}

fn advance_ants(
    time: Res<Time>,
    config: Res<EnemiesConfig>,
    layout: Res<MapLayout>,
    mut damage: MessageWriter<Damage>,
    crops: Query<(Entity, &Transform), (With<Crop>, Without<Ant>)>,
    crystals: Query<(Entity, &Transform), (With<Crystal>, Without<Ant>)>,
    mut ants: Query<(&mut Ant, &mut Transform)>,
) {
    let delta = time.delta();
    let delta_seconds = time.delta_secs();
    let waypoints = layout.path.waypoints();

    for (mut ant, mut transform) in &mut ants {
        let position = transform.translation.xz();

        if let Some(target) = ant.target {
            let target_data = match target {
                AntTarget::Crop(entity) => {
                    crops.get(entity).ok().map(|(_, target)| (entity, target))
                }
                AntTarget::Crystal(entity) => crystals
                    .get(entity)
                    .ok()
                    .map(|(_, target)| (entity, target)),
            };

            if let Some((entity, target_transform)) = target_data {
                let target_position = target_transform.translation.xz();

                if transform.move_towards(target_position, config.ant_speed * delta_seconds, REACH)
                    && ant.attack.tick(delta).just_finished()
                {
                    damage.write(Damage {
                        target: entity,
                        amount: config.ant_damage,
                    });
                }

                continue;
            }

            ant.target = None;
        }

        if let Some((crop, target)) = crops
            .iter()
            .filter(|(_, target)| {
                target.translation.xz().distance(position) <= config.crop_detection_radius
                    && layout.path.distance_to(target.translation.xz())
                        <= config.crop_detection_radius
            })
            .min_by(|(_, a), (_, b)| {
                a.translation
                    .xz()
                    .distance(position)
                    .total_cmp(&b.translation.xz().distance(position))
            })
        {
            ant.target = Some(AntTarget::Crop(crop));
            let target_position = target.translation.xz();

            if transform.move_towards(target_position, config.ant_speed * delta_seconds, REACH)
                && ant.attack.tick(delta).just_finished()
            {
                damage.write(Damage {
                    target: crop,
                    amount: config.ant_damage,
                });
            }

            continue;
        }

        while let Some(waypoint) = waypoints.get(ant.next_waypoint)
            && transform.translation.xz().distance(*waypoint) <= REACH
        {
            ant.next_waypoint += 1;
        }

        let Some(waypoint) = waypoints.get(ant.next_waypoint) else {
            if let Ok((crystal, target)) = crystals.single() {
                ant.target = Some(AntTarget::Crystal(crystal));
                let target_position = target.translation.xz();

                if transform.move_towards(target_position, config.ant_speed * delta_seconds, REACH)
                    && ant.attack.tick(delta).just_finished()
                {
                    damage.write(Damage {
                        target: crystal,
                        amount: config.ant_damage,
                    });
                }
            }

            continue;
        };

        transform.move_towards(*waypoint, config.ant_speed * delta_seconds, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::combat::{CombatPlugin, Health};
    use crate::farming::{Crop, Threat};
    use crate::game::{AppState, testing::TestApp};
    use crate::map::{MapLayout, MonsterPath};

    fn enemies_app() -> App {
        let mut app = App::headless();

        app.add_plugins((CombatPlugin, EnemiesPlugin))
            .insert_resource(EnemiesConfig {
                ant_health: 3.0,
                ant_damage: 1.0,
                ant_speed: 5.0,
                crop_detection_radius: 3.0,
                attack_interval: Duration::from_secs(1),
                waves: vec![AntWave {
                    at: Duration::ZERO,
                    count: 1,
                }],
            })
            .insert_resource(MapLayout {
                crystal_health: 100.0,
                path: MonsterPath::try_from(vec![Vec2::ZERO, Vec2::new(10.0, 0.0)]).unwrap(),
                plots: vec![],
                peddler: Vec2::ZERO,
                farmer_start: Vec2::ZERO,
                rabbit_home: Vec2::ZERO,
            })
            .enter(AppState::InLevel);

        app.world_mut().spawn((
            crate::map::Crystal,
            Health::new(100.0),
            Transform::from_xyz(10.0, 0.0, 0.0),
        ));

        app
    }

    fn ant(app: &mut App) -> Entity {
        app.world_mut()
            .query_filtered::<Entity, With<Ant>>()
            .single(app.world())
            .unwrap()
    }

    #[test]
    fn green_ant_follows_waypoints_toward_the_crystal() {
        let mut app = enemies_app();

        app.advance_by(Duration::from_secs(1));

        let ant = ant(&mut app);
        assert!(app.world().get::<Transform>(ant).unwrap().translation.x > 4.0);
    }

    #[test]
    fn green_ant_diverts_to_nearby_crop_then_resumes_its_path() {
        let mut app = enemies_app();
        let crop = app
            .world_mut()
            .spawn((Crop, Health::new(1.0), Transform::from_xyz(2.0, 0.0, 2.0)))
            .id();

        app.advance_by(Duration::from_secs(3));

        let ant = ant(&mut app);

        assert!(app.world().get_entity(crop).is_err());
        assert!(app.world().get::<Threat>(ant).is_some());
        assert!(app.world().get::<Transform>(ant).unwrap().translation.x > 4.0);
    }

    #[test]
    fn green_ant_ignores_crops_outside_its_detection_range() {
        let mut app = enemies_app();
        let crop = app
            .world_mut()
            .spawn((Crop, Health::new(1.0), Transform::from_xyz(2.0, 0.0, 4.0)))
            .id();

        app.advance_by(Duration::from_secs(3));

        assert!(app.world().get_entity(crop).is_ok());
        assert_eq!(app.world().get::<Health>(crop).unwrap().current(), 1.0);
    }

    #[test]
    fn green_ant_damages_the_crystal_after_reaching_the_end_of_its_path() {
        let mut app = enemies_app();

        app.advance_by(Duration::from_secs(4));

        let crystal = app
            .world_mut()
            .query_filtered::<Entity, With<crate::map::Crystal>>()
            .single(app.world())
            .unwrap();
        assert!(app.world().get::<Health>(crystal).unwrap().current() < 100.0);
    }
}
