//! Waves: the enemies a level sends, and when.
//!
//! Owns the [`WaveScript`] of the running level, sorted by time when it is read, and spawns each
//! wave's enemy recipe at the spawner in [`GameSet::Spawn`] once the level clock reaches it.
//! Enemies come from the content [`Recipes`]; what they do afterwards is up to their behaviours.

use std::time::Duration;

use bevy::prelude::*;
use serde::Deserialize;

use crate::content::{RecipeVisuals, Recipes};
use crate::game::{AppState, GameSet, LevelClock};
use crate::map::{MapLayout, Spawner};

/// Distance between enemies of the same wave, in world units.
const SPACING: f32 = 0.5;

/// A group of enemies that appears at a level time.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wave {
    #[serde(with = "humantime_serde")]
    pub at: Duration,
    /// Id of the enemy recipe.
    pub enemy: String,
    pub count: u32,
}

/// The waves of a level, in time order.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "Vec<Wave>")]
pub struct WaveScript(Vec<Wave>);

impl WaveScript {
    /// Every wave, in time order.
    pub fn waves(&self) -> &[Wave] {
        &self.0
    }
}

impl From<Vec<Wave>> for WaveScript {
    fn from(mut waves: Vec<Wave>) -> Self {
        waves.sort_by_key(|wave| wave.at);

        Self(waves)
    }
}

/// How many waves of the running level have spawned.
#[derive(Resource, Debug, Default)]
struct WaveProgress(usize);

/// Spawns the waves of the running level.
pub struct WavesPlugin;

impl Plugin for WavesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaveScript>()
            .init_resource::<WaveProgress>()
            .add_systems(OnEnter(AppState::InLevel), reset_waves)
            .add_systems(FixedUpdate, spawn_due_waves.in_set(GameSet::Spawn));
    }
}

fn reset_waves(mut progress: ResMut<WaveProgress>) {
    progress.0 = 0;
}

fn spawn_due_waves(
    mut commands: Commands,
    clock: Res<LevelClock>,
    script: Res<WaveScript>,
    layout: Res<MapLayout>,
    recipes: Res<Recipes>,
    visuals: Res<RecipeVisuals>,
    spawner: Query<&Transform, With<Spawner>>,
    mut progress: ResMut<WaveProgress>,
) {
    let origin = spawner.single().map_or_else(
        |_| layout.path.spawner(),
        |spawner| spawner.translation.xz(),
    );

    while let Some(wave) = script.waves().get(progress.0)
        && clock.elapsed() >= wave.at
    {
        for index in 0..wave.count {
            let position = origin + Vec2::X * (index as f32 * SPACING);

            if recipes
                .spawn_enemy(&mut commands, &visuals, &wave.enemy, position)
                .is_none()
            {
                warn!("Wave enemy {:?} has no recipe", wave.enemy);
            }
        }

        progress.0 += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::combat::{CombatPlugin, Health};
    use crate::content::Enemy;
    use crate::content::testing::{RecipeApp, enemy};
    use crate::game::testing::TestApp;
    use crate::map::testing::layout;
    use crate::map::{MapLayout, MonsterPath};
    use crate::movement::{MovementPlugin, PathWaypoints};
    use crate::targeting::{Tags, TargetingPlugin};

    fn wave(seconds: u64, count: u32) -> Wave {
        Wave {
            at: Duration::from_secs(seconds),
            enemy: "a".to_owned(),
            count,
        }
    }

    fn enemies(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<Enemy>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn waves_spawn_at_their_time() {
        let mut app = App::headless();
        app.add_plugins(WavesPlugin)
            .with_recipes(Recipes {
                enemies: BTreeMap::from([("a".to_owned(), enemy(&["b"], 1.0, 1.0))]),
                ..default()
            })
            .insert_resource(layout())
            .insert_resource(WaveScript::from(vec![wave(2, 3), wave(1, 2)]))
            .enter(AppState::InLevel);

        app.advance_by(Duration::from_millis(1500));
        assert_eq!(enemies(&mut app), 2);

        app.advance_by(Duration::from_secs(1));
        assert_eq!(enemies(&mut app), 5);
    }

    #[test]
    fn waves_are_sorted_at_load() {
        let script: WaveScript = ron::from_str(
            r#"[(at: "1m", enemy: "a", count: 1), (at: "10s", enemy: "b", count: 2)]"#,
        )
        .unwrap();

        let order: Vec<_> = script
            .waves()
            .iter()
            .map(|wave| wave.enemy.as_str())
            .collect();
        assert_eq!(order, ["b", "a"]);
    }

    /// One enemy walking (0, 0) → (10, 0) toward a crystal, attacking crops on the way.
    fn behaviour_app() -> App {
        let waypoints = vec![Vec2::ZERO, Vec2::new(10.0, 0.0)];
        let mut app = App::headless();

        app.add_plugins((WavesPlugin, MovementPlugin, TargetingPlugin, CombatPlugin))
            .with_recipes(Recipes {
                enemies: BTreeMap::from([("a".to_owned(), enemy(&["crop", "crystal"], 5.0, 3.0))]),
                ..default()
            })
            .insert_resource(MapLayout {
                path: MonsterPath::try_from(waypoints.clone()).unwrap(),
                ..layout()
            })
            .insert_resource(PathWaypoints(waypoints))
            .insert_resource(WaveScript::from(vec![wave(0, 1)]))
            .enter(AppState::InLevel);
        app.world_mut().spawn((
            crate::map::Crystal,
            Tags(vec!["crystal".to_owned()]),
            Health::new(100.0),
            Transform::from_xyz(10.0, 0.0, 0.0),
        ));

        app
    }

    fn spawned_enemy(app: &mut App) -> Entity {
        app.world_mut()
            .query_filtered::<Entity, With<Enemy>>()
            .single(app.world())
            .unwrap()
    }

    fn crop_at(app: &mut App, x: f32, z: f32) -> Entity {
        app.world_mut()
            .spawn((
                Tags(vec!["crop".to_owned()]),
                Health::new(1.0),
                Transform::from_xyz(x, 0.0, z),
            ))
            .id()
    }

    #[test]
    fn enemy_follows_the_path_toward_the_crystal() {
        let mut app = behaviour_app();

        app.advance_by(Duration::from_secs(1));

        let enemy = spawned_enemy(&mut app);
        assert!(app.world().get::<Transform>(enemy).unwrap().translation.x > 4.0);
    }

    #[test]
    fn enemy_diverts_to_a_crop_in_sight_then_resumes_its_path() {
        let mut app = behaviour_app();
        let crop = crop_at(&mut app, 2.0, 2.0);

        app.advance_by(Duration::from_secs(3));

        let enemy = spawned_enemy(&mut app);
        assert!(app.world().get_entity(crop).is_err());
        assert!(app.world().get::<Transform>(enemy).unwrap().translation.x > 4.0);
    }

    #[test]
    fn enemy_ignores_crops_out_of_sight() {
        let mut app = behaviour_app();
        let crop = crop_at(&mut app, 2.0, 4.0);

        app.advance_by(Duration::from_secs(3));

        assert_eq!(app.world().get::<Health>(crop).unwrap().current(), 1.0);
    }

    #[test]
    fn enemy_damages_the_crystal_at_the_end_of_its_path() {
        let mut app = behaviour_app();

        app.advance_by(Duration::from_secs(4));

        let crystal = app
            .world_mut()
            .query_filtered::<Entity, With<crate::map::Crystal>>()
            .single(app.world())
            .unwrap();
        assert!(app.world().get::<Health>(crystal).unwrap().current() < 100.0);
    }
}
