//! Recruiting: buying defenders from the peddler and placing them.
//!
//! `K` starts placing the first defender the level's [`Trade`] sells, if the player can afford it,
//! and cancels a placement in progress. A left click on the field triggers [`PlaceDefender`]: the spot
//! must be on the [`Field`] and the new guard area must not overlap another [`Guard`]; gold is then
//! spent and the defender recipe spawned. A preview ring, sized to the recipe's area, follows the
//! cursor and turns red where placement is invalid. [`Placement`] tells the HUD what is going on.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::content::{RecipeVisuals, Recipes};
use crate::economy::{Gold, Trade};
use crate::game::{AppState, GameSet, GroundPosition};
use crate::guarding::{AreaRingAssets, Guard};
use crate::stage::{CursorGround, Field};

const INVALID_AREA_COLOR: Color = Color::srgba(0.95, 0.2, 0.2, 0.35);
/// Height of the preview ring above the ground, so it does not flicker with it.
const PREVIEW_HEIGHT: f32 = 0.02;

/// Whether the player is choosing where to place a defender.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub enum Placement {
    #[default]
    Idle,
    /// Waiting for a click to place the defender recipe `defender`.
    Placing { defender: String },
}

/// Starts placing the first defender for sale.
#[derive(Event, Debug)]
pub struct BeginPlacement;

/// Places the defender being placed at a point on the ground.
#[derive(Event, Debug)]
pub struct PlaceDefender {
    /// Position on the ground plane, in world units.
    pub position: Vec2,
}

#[derive(Component, Debug)]
struct PlacementPreview;

/// The guard areas already on the field.
#[derive(SystemParam)]
struct GuardAreas<'w, 's> {
    guards: Query<'w, 's, &'static Guard>,
}

impl GuardAreas<'_, '_> {
    /// Whether a guard with an area of `radius` fits at `position`.
    fn fits(&self, position: Vec2, radius: f32) -> bool {
        Field::contains(position)
            && !self
                .guards
                .iter()
                .any(|guard| guard.overlaps(position, radius))
    }
}

#[derive(Resource, Debug)]
struct InvalidAreaMaterial(Handle<StandardMaterial>);

impl FromWorld for InvalidAreaMaterial {
    fn from_world(world: &mut World) -> Self {
        Self(
            world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial {
                    base_color: INVALID_AREA_COLOR,
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                }),
        )
    }
}

/// Sells and places defenders.
pub struct RecruitingPlugin;

impl Plugin for RecruitingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Placement>()
            .init_resource::<AreaRingAssets>()
            .init_resource::<InvalidAreaMaterial>()
            .add_observer(begin_placement)
            .add_observer(place_defender)
            .add_systems(
                OnEnter(AppState::InLevel),
                (reset_placement, spawn_placement_preview),
            )
            .add_systems(
                Update,
                (
                    (request_placement, place_on_click).in_set(GameSet::Input),
                    update_placement_preview.in_set(GameSet::Presentation),
                ),
            );
    }
}

fn reset_placement(mut placement: ResMut<Placement>) {
    *placement = Placement::Idle;
}

fn spawn_placement_preview(mut commands: Commands, rings: Res<AreaRingAssets>) {
    commands.spawn((
        Name::new("Placement preview"),
        PlacementPreview,
        Mesh3d(rings.mesh.clone()),
        MeshMaterial3d(rings.material.clone()),
        Visibility::Hidden,
        DespawnOnExit(AppState::InLevel),
    ));
}

/// `K`: start or cancel placing a defender.
fn request_placement(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut placement: ResMut<Placement>,
) {
    if !keys.just_pressed(KeyCode::KeyK) {
        return;
    }

    if *placement == Placement::Idle {
        commands.trigger(BeginPlacement);
    } else {
        *placement = Placement::Idle;
        info!("Placement cancelled");
    }
}

fn begin_placement(
    _: On<BeginPlacement>,
    gold: Res<Gold>,
    trade: Res<Trade>,
    mut placement: ResMut<Placement>,
) {
    let Some((defender, price)) = trade.sells.iter().next() else {
        info!("The peddler has no defender for sale");

        return;
    };

    if gold.amount() < *price {
        info!("Not enough gold to recruit a {defender}");

        return;
    }

    *placement = Placement::Placing {
        defender: defender.clone(),
    };
    info!("Choose a position with left click; press K to cancel");
}

fn place_on_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorGround>,
    placement: Res<Placement>,
) {
    if *placement == Placement::Idle || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if let Some(position) = cursor.0 {
        commands.trigger(PlaceDefender { position });
    }
}

fn place_defender(
    event: On<PlaceDefender>,
    mut commands: Commands,
    mut gold: ResMut<Gold>,
    trade: Res<Trade>,
    recipes: Res<Recipes>,
    visuals: Res<RecipeVisuals>,
    mut placement: ResMut<Placement>,
    areas: GuardAreas,
) {
    let Placement::Placing { defender } = placement.clone() else {
        return;
    };

    let Some(radius) = recipes.defenders.get(&defender).map(|recipe| recipe.sight) else {
        warn!("Defender {defender:?} has no recipe");
        *placement = Placement::Idle;

        return;
    };

    let position = event.position;

    if !areas.fits(position, radius) {
        info!("Choose a valid position");

        return;
    }

    let price = trade.sells.get(&defender).copied().unwrap_or_default();

    if !gold.try_spend(price) {
        *placement = Placement::Idle;
        info!("Not enough gold to place a {defender}");

        return;
    }

    recipes.spawn_defender(&mut commands, &visuals, &defender, position);
    *placement = Placement::Idle;
}

fn update_placement_preview(
    placement: Res<Placement>,
    cursor: Res<CursorGround>,
    recipes: Res<Recipes>,
    rings: Res<AreaRingAssets>,
    invalid: Res<InvalidAreaMaterial>,
    areas: GuardAreas,
    preview: Single<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        With<PlacementPreview>,
    >,
) {
    let (mut transform, mut visibility, mut material) = preview.into_inner();
    let target = match &*placement {
        Placement::Idle => None,
        Placement::Placing { defender } => recipes
            .defenders
            .get(defender)
            .map(|recipe| recipe.sight)
            .zip(cursor.0),
    };

    let Some((radius, position)) = target else {
        *visibility = Visibility::Hidden;

        return;
    };

    *transform = Transform::from_translation(position.on_ground() + Vec3::Y * PREVIEW_HEIGHT)
        .with_scale(Vec3::splat(radius));
    *visibility = Visibility::Visible;
    material.0 = if areas.fits(position, radius) {
        rings.material.clone()
    } else {
        invalid.0.clone()
    };
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use super::*;
    use crate::combat::{CombatPlugin, Health};
    use crate::content::testing::{RecipeApp, defender};
    use crate::content::{Crop, Defender};
    use crate::game::testing::TestApp;
    use crate::guarding::GuardingPlugin;
    use crate::map::PlantPlot;
    use crate::movement::MovementPlugin;
    use crate::targeting::{Tags, TargetingPlugin};

    const AREA_COLOR: Color = Color::srgba(0.9, 0.8, 0.35, 0.2);

    fn recruiting_app(gold: u32) -> App {
        let mut app = App::headless();

        app.add_plugins((
            CombatPlugin,
            GuardingPlugin,
            MovementPlugin,
            TargetingPlugin,
            RecruitingPlugin,
        ))
        .with_recipes(Recipes {
            defenders: BTreeMap::from([("a".to_owned(), defender(4.0))]),
            ..default()
        })
        .init_resource::<CursorGround>()
        .insert_resource(Gold::with_amount(gold))
        .insert_resource(Trade {
            buys: BTreeMap::new(),
            sells: BTreeMap::from([("a".to_owned(), 5)]),
        })
        .enter(AppState::InLevel);

        app
    }

    fn placing() -> Placement {
        Placement::Placing {
            defender: "a".to_owned(),
        }
    }

    fn place(app: &mut App, position: Vec2) {
        app.world_mut().trigger(BeginPlacement);
        app.world_mut().trigger(PlaceDefender { position });
        app.update();
    }

    fn defender_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<Defender>>()
            .iter(app.world())
            .count()
    }

    fn preview_color(app: &mut App) -> (Visibility, Transform, Color) {
        let (visibility, transform, material) = app
            .world_mut()
            .query_filtered::<(&Visibility, &Transform, &MeshMaterial3d<StandardMaterial>), With<PlacementPreview>>()
            .single(app.world())
            .map(|(visibility, transform, material)| (*visibility, *transform, material.0.clone()))
            .expect("placement preview should exist");
        let color = app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .get(&material)
            .unwrap()
            .base_color;

        (visibility, transform, color)
    }

    #[test]
    fn overlapping_defender_areas_are_rejected_without_spending_gold() {
        let mut app = recruiting_app(10);

        place(&mut app, Vec2::ZERO);
        place(&mut app, Vec2::new(4.0, 0.0));

        assert_eq!(defender_count(&mut app), 1);
        assert_eq!(app.world().resource::<Gold>().amount(), 5);
        assert_eq!(*app.world().resource::<Placement>(), placing());
    }

    #[test]
    fn placement_uses_guard_home_instead_of_transform() {
        let mut app = recruiting_app(10);
        let guard = app
            .world_mut()
            .spawn((
                Guard {
                    home: Vec2::ZERO,
                    radius: 3.0,
                },
                Transform::from_xyz(2.0, 0.0, 0.0),
            ))
            .id();
        *app.world_mut().resource_mut::<Placement>() = placing();

        app.world_mut().trigger(PlaceDefender {
            position: Vec2::new(-5.0, 0.0),
        });
        app.update();

        assert_eq!(defender_count(&mut app), 0);
        assert_eq!(app.world().resource::<Gold>().amount(), 10);
        assert_eq!(*app.world().resource::<Placement>(), placing());

        app.world_mut()
            .get_mut::<Transform>(guard)
            .unwrap()
            .translation
            .x = 1.0;
        app.world_mut().trigger(PlaceDefender {
            position: Vec2::new(7.5, 0.0),
        });
        app.update();

        assert_eq!(defender_count(&mut app), 1);
        assert_eq!(app.world().resource::<Gold>().amount(), 5);
    }

    #[test]
    fn placement_outside_the_field_is_rejected() {
        let mut app = recruiting_app(10);

        place(&mut app, Vec2::new(Field::HALF_SIZE + 1.0, 0.0));

        assert_eq!(defender_count(&mut app), 0);
        assert_eq!(app.world().resource::<Gold>().amount(), 10);
    }

    #[test]
    fn placement_preview_tracks_cursor_and_marks_overlapping_areas_invalid() {
        let mut app = recruiting_app(10);
        *app.world_mut().resource_mut::<Placement>() = placing();
        app.world_mut().resource_mut::<CursorGround>().0 = Some(Vec2::new(1.0, 0.0));
        app.update();

        let (visibility, transform, color) = preview_color(&mut app);
        assert_eq!(visibility, Visibility::Visible);
        assert_eq!(transform.translation.xz(), Vec2::new(1.0, 0.0));
        assert_eq!(transform.rotation, Quat::IDENTITY);
        assert_eq!(transform.scale, Vec3::splat(4.0));
        assert_eq!(color, AREA_COLOR);

        app.world_mut().spawn((
            Guard {
                home: Vec2::ZERO,
                radius: 4.0,
            },
            Transform::default(),
        ));
        app.update();

        assert_eq!(preview_color(&mut app).2, INVALID_AREA_COLOR);

        *app.world_mut().resource_mut::<Placement>() = Placement::Idle;
        app.update();

        assert_eq!(preview_color(&mut app).0, Visibility::Hidden);
    }

    #[test]
    fn plants_can_be_inside_a_defenders_area() {
        let mut app = recruiting_app(5);

        app.world_mut()
            .spawn((PlantPlot, Crop, Transform::from_xyz(0.0, 0.0, 1.0)));
        place(&mut app, Vec2::ZERO);

        assert_eq!(defender_count(&mut app), 1);
        assert_eq!(app.world().resource::<Gold>().amount(), 0);
    }

    #[test]
    fn insufficient_gold_does_not_start_placement_or_spawn_a_defender() {
        let mut app = recruiting_app(4);

        place(&mut app, Vec2::ZERO);

        assert_eq!(defender_count(&mut app), 0);
        assert_eq!(app.world().resource::<Gold>().amount(), 4);
        assert_eq!(*app.world().resource::<Placement>(), Placement::Idle);
    }

    #[test]
    fn placement_resets_when_replaying_level() {
        let mut app = recruiting_app(10);
        app.world_mut().trigger(BeginPlacement);
        assert_eq!(*app.world().resource::<Placement>(), placing());

        app.enter(AppState::Results).enter(AppState::InLevel);

        assert_eq!(*app.world().resource::<Placement>(), Placement::Idle);
    }

    #[test]
    fn placed_defender_attacks_the_nearest_enemy_in_its_area() {
        let mut app = recruiting_app(5);
        let mut enemy = |x: f32| {
            app.world_mut()
                .spawn((
                    Tags::new(["enemy"]),
                    Health::new(10.0),
                    Transform::from_xyz(x, 0.0, 0.0),
                ))
                .id()
        };
        let nearest = enemy(1.5);
        let farther = enemy(2.5);
        place(&mut app, Vec2::ZERO);

        app.advance_by(Duration::from_secs(2));

        assert!(app.world().get::<Health>(nearest).unwrap().current() < 10.0);
        assert_eq!(app.world().get::<Health>(farther).unwrap().current(), 10.0);
    }
}
