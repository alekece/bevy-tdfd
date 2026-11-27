//! Knight Bunnies: recruiting, action areas and attacks.
//!
//! The peddler offers a Knight Bunny when the player presses `K`. Clicking a valid ground position
//! spends gold and places the defender. Knight Bunnies stay within their non-overlapping action
//! areas, chase the nearest ant in range, and send damage through the shared combat plugin.

use std::time::Duration;

use bevy::prelude::*;
use serde::Deserialize;

use crate::combat::{Damage, Health};
use crate::economy::{EconomyConfig, Gold};
use crate::enemies::Ant;
use crate::game::{AppState, GameSet, GroundMovement, GroundPosition};
use crate::stage::CursorGround;

const FIELD_HALF_SIZE: f32 = 9.5;
const ATTACK_COLOR: Color = Color::srgb(0.86, 0.68, 0.28);
const AREA_COLOR: Color = Color::srgba(0.9, 0.8, 0.35, 0.2);

/// Knight Bunny stats and action-area size for a level.
#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnightBunnyConfig {
    pub health: f32,
    pub damage: f32,
    /// Radius of its action area, in world units. Action areas cannot overlap.
    pub action_radius: f32,
    /// Distance at which a hammer can hit, in world units.
    pub attack_range: f32,
    /// Movement speed inside its action area, in world units per second.
    pub speed: f32,
    #[serde(with = "humantime_serde")]
    pub attack_interval: Duration,
}

/// Starts placing one Knight Bunny.
#[derive(Event, Debug)]
pub struct BeginKnightBunnyPlacement;

/// Places one Knight Bunny at a point on the ground.
#[derive(Event, Debug)]
pub struct PlaceKnightBunny {
    /// Position on the ground plane, in world units.
    pub position: Vec2,
}

/// Whether the player is currently choosing where to place a Knight Bunny.
#[derive(Resource, Debug, Default)]
pub struct PlacementMode {
    active: bool,
}

impl PlacementMode {
    /// Whether a placement is waiting for the player's click.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// A recruited Knight Bunny and its guard position.
#[derive(Component, Debug)]
pub struct KnightBunny {
    post: Vec2,
    attack: Timer,
}

impl KnightBunny {
    fn new(position: Vec2, interval: Duration) -> Self {
        Self {
            post: position,
            attack: Timer::new(interval, TimerMode::Repeating),
        }
    }
}

#[derive(Resource, Debug)]
struct DefenderAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    area_mesh: Handle<Mesh>,
    area_material: Handle<StandardMaterial>,
}

impl FromWorld for DefenderAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let mesh = meshes.add(Capsule3d::new(0.3, 0.6));
        let area_mesh = meshes.add(Torus::new(0.94, 1.0));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let material = materials.add(ATTACK_COLOR);
        let area_material = materials.add(StandardMaterial {
            base_color: AREA_COLOR,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });

        Self {
            mesh,
            material,
            area_mesh,
            area_material,
        }
    }
}

/// Offers, places and runs Knight Bunnies.
pub struct DefendersPlugin;

impl Plugin for DefendersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacementMode>()
            .init_resource::<DefenderAssets>()
            .add_observer(begin_placement)
            .add_observer(place_knight_bunny)
            .add_systems(
                Update,
                (request_placement, place_on_click).in_set(GameSet::Input),
            )
            .add_systems(FixedUpdate, attack_nearest_ant.in_set(GameSet::Act));
    }
}

fn request_placement(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<PlacementMode>,
) {
    if !keys.just_pressed(KeyCode::KeyK) {
        return;
    }

    if mode.active {
        mode.active = false;
        info!("Knight Bunny placement cancelled");
    } else {
        commands.trigger(BeginKnightBunnyPlacement);
    }
}

fn begin_placement(
    _: On<BeginKnightBunnyPlacement>,
    gold: Res<Gold>,
    economy: Res<EconomyConfig>,
    mut mode: ResMut<PlacementMode>,
) {
    if gold.amount() < economy.knight_bunny_cost {
        info!("Not enough gold to recruit a Knight Bunny");

        return;
    }

    mode.active = true;
    info!("Choose a Knight Bunny position with left click; press K to cancel");
}

fn place_on_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorGround>,
    mode: Res<PlacementMode>,
) {
    if !mode.active || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if let Some(position) = cursor.0 {
        commands.trigger(PlaceKnightBunny { position });
    }
}

fn place_knight_bunny(
    event: On<PlaceKnightBunny>,
    mut commands: Commands,
    mut gold: ResMut<Gold>,
    economy: Res<EconomyConfig>,
    config: Res<KnightBunnyConfig>,
    assets: Res<DefenderAssets>,
    mut mode: ResMut<PlacementMode>,
    defenders: Query<&Transform, With<KnightBunny>>,
) {
    if !mode.active {
        return;
    }

    let position = event.position;

    if position.abs().max_element() > FIELD_HALF_SIZE {
        info!("Choose a position on the map");

        return;
    }

    if defenders
        .iter()
        .any(|defender| defender.translation.xz().distance(position) < config.action_radius * 2.0)
    {
        info!("Knight Bunny action areas cannot overlap");

        return;
    }

    if !gold.try_spend(economy.knight_bunny_cost) {
        mode.active = false;
        info!("Not enough gold to place a Knight Bunny");

        return;
    }

    commands.spawn((
        Name::new("Knight Bunny"),
        KnightBunny::new(position, config.attack_interval),
        Health::new(config.health),
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material.clone()),
        Transform::from_translation(position.on_ground() + Vec3::Y * 0.6),
        DespawnOnExit(AppState::InLevel),
    ));

    commands.spawn((
        Name::new("Knight Bunny action area"),
        Mesh3d(assets.area_mesh.clone()),
        MeshMaterial3d(assets.area_material.clone()),
        Transform::from_translation(position.on_ground() + Vec3::Y * 0.02)
            .with_scale(Vec3::splat(config.action_radius))
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        DespawnOnExit(AppState::InLevel),
    ));

    mode.active = false;
}

fn attack_nearest_ant(
    time: Res<Time>,
    config: Res<KnightBunnyConfig>,
    mut damage: MessageWriter<Damage>,
    ants: Query<(Entity, &Transform), (With<Ant>, Without<KnightBunny>)>,
    mut defenders: Query<(&mut KnightBunny, &mut Transform), Without<Ant>>,
) {
    for (mut defender, mut transform) in &mut defenders {
        let position = transform.translation.xz();
        let target = ants
            .iter()
            .filter(|(_, ant)| ant.translation.xz().distance(defender.post) <= config.action_radius)
            .min_by(|(_, a), (_, b)| {
                a.translation
                    .xz()
                    .distance(position)
                    .total_cmp(&b.translation.xz().distance(position))
            });

        let Some((entity, ant)) = target else {
            transform.move_towards(defender.post, config.speed * time.delta_secs(), 0.0);

            continue;
        };

        let target_position = ant.translation.xz();
        let in_reach = transform.move_towards(
            target_position,
            config.speed * time.delta_secs(),
            config.attack_range,
        );

        if in_reach && defender.attack.tick(time.delta()).just_finished() {
            damage.write(Damage {
                target: entity,
                amount: config.damage,
            });
        }

        let relative = transform.translation.xz() - defender.post;

        if relative.length() > config.action_radius {
            let clamped = defender.post + relative.normalize() * config.action_radius;
            transform.translation.x = clamped.x;
            transform.translation.z = clamped.y;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::combat::{CombatPlugin, Health};
    use crate::economy::Gold;
    use crate::farming::Crop;
    use crate::game::{AppState, testing::TestApp};
    use crate::map::{MapLayout, PlantPlot};
    use crate::stage::CursorGround;

    fn defenders_app(gold: u32) -> App {
        let mut app = App::headless();

        app.add_plugins((CombatPlugin, DefendersPlugin))
            .init_resource::<CursorGround>()
            .insert_resource(KnightBunnyConfig {
                health: 10.0,
                damage: 1.0,
                action_radius: 4.0,
                attack_range: 0.7,
                speed: 4.0,
                attack_interval: Duration::from_millis(500),
            })
            .insert_resource(Gold::with_amount(gold))
            .insert_resource(EconomyConfig {
                tomato_price: 5,
                knight_bunny_cost: 5,
            })
            .insert_resource(MapLayout {
                crystal_health: 100.0,
                path: crate::map::MonsterPath::try_from(vec![
                    Vec2::new(-9.0, 0.0),
                    Vec2::new(9.0, 0.0),
                ])
                .unwrap(),
                plots: vec![],
                peddler: Vec2::ZERO,
                farmer_start: Vec2::ZERO,
                rabbit_home: Vec2::ZERO,
            })
            .enter(AppState::InLevel);

        app
    }

    fn place(app: &mut App, position: Vec2) {
        app.world_mut().trigger(BeginKnightBunnyPlacement);
        app.world_mut().trigger(PlaceKnightBunny { position });
        app.update();
    }

    fn knight_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<KnightBunny>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn overlapping_defender_areas_are_rejected_without_spending_gold() {
        let mut app = defenders_app(10);

        place(&mut app, Vec2::ZERO);
        place(&mut app, Vec2::new(4.0, 0.0));

        assert_eq!(knight_count(&mut app), 1);
        assert_eq!(app.world().resource::<Gold>().amount(), 5);
        assert!(app.world().resource::<PlacementMode>().is_active());
    }

    #[test]
    fn plants_can_be_inside_a_defenders_action_area() {
        let mut app = defenders_app(5);

        app.world_mut()
            .spawn((PlantPlot, Crop, Transform::from_xyz(0.0, 0.0, 1.0)));
        place(&mut app, Vec2::ZERO);

        assert_eq!(knight_count(&mut app), 1);
        assert_eq!(app.world().resource::<Gold>().amount(), 0);
    }

    #[test]
    fn insufficient_gold_does_not_start_placement_or_spawn_a_defender() {
        let mut app = defenders_app(4);

        place(&mut app, Vec2::ZERO);

        assert_eq!(knight_count(&mut app), 0);
        assert_eq!(app.world().resource::<Gold>().amount(), 4);
    }

    #[test]
    fn knight_bunny_attacks_the_nearest_ant_in_its_action_area() {
        let mut app = defenders_app(5);
        let nearest = app
            .world_mut()
            .spawn((
                crate::enemies::Ant::test_ant(),
                Health::new(10.0),
                Transform::from_xyz(1.5, 0.0, 0.0),
            ))
            .id();
        let farther = app
            .world_mut()
            .spawn((
                crate::enemies::Ant::test_ant(),
                Health::new(10.0),
                Transform::from_xyz(2.5, 0.0, 0.0),
            ))
            .id();
        place(&mut app, Vec2::ZERO);

        app.advance_by(Duration::from_secs(2));

        assert!(app.world().get::<Health>(nearest).unwrap().current() < 10.0);
        assert_eq!(app.world().get::<Health>(farther).unwrap().current(), 10.0);
    }
}
