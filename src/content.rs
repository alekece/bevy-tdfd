//! Content: the enemy, defender and crop variants of the game, as typed recipes.
//!
//! Owns the category markers ([`Enemy`], [`Defender`], [`Crop`]), the [`Recipes`] read from the
//! level file and the [`RecipeVisuals`] built once per recipe. Spawning a recipe copies its values
//! into the behaviour components of movement, targeting, guarding, combat and growth and tags the
//! entity with its category and id, so later recipe changes never alter live entities.
//! [`RecipeStore`] replaces the recipes and rebuilds their visuals together.

use std::collections::BTreeMap;
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::Deserialize;

use crate::combat::{Health, Melee};
use crate::game::{AppState, GroundPosition};
use crate::growth::{Growth, GrowthLook, Yield};
use crate::guarding::Guard;
use crate::movement::{FollowPath, Intent, Speed};
use crate::targeting::{Sight, Tags, Targets};

/// Tag shared by every enemy.
pub const ENEMY_TAG: &str = "enemy";
/// Tag shared by every defender.
pub const DEFENDER_TAG: &str = "defender";
/// Tag shared by every crop.
pub const CROP_TAG: &str = "crop";

/// An enemy, spawned from a recipe of the `enemies` section.
#[derive(Component, Debug, Default)]
#[require(Intent)]
pub struct Enemy;

/// A defender, spawned from a recipe of the `defenders` section.
#[derive(Component, Debug, Default)]
#[require(Intent)]
pub struct Defender;

/// A planted crop, spawned from a recipe of the `crops` section.
#[derive(Component, Debug, Default)]
pub struct Crop;

/// An sRGB colour with components from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Rgb(pub f32, pub f32, pub f32);

impl From<Rgb> for Color {
    fn from(Rgb(red, green, blue): Rgb) -> Self {
        Color::srgb(red, green, blue)
    }
}

/// A simple low-poly shape. Sizes are in world units.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum Shape {
    Sphere { radius: f32 },
    Capsule { radius: f32, length: f32 },
}

impl Shape {
    /// Height of the shape's center when it rests on the ground.
    pub fn height(&self) -> f32 {
        match *self {
            Shape::Sphere { radius } => radius,
            Shape::Capsule { radius, length } => radius + length / 2.0,
        }
    }

    fn mesh(&self) -> Mesh {
        match *self {
            Shape::Sphere { radius } => Sphere::new(radius)
                .mesh()
                .ico(1)
                .expect("one subdivision is valid"),
            Shape::Capsule { radius, length } => Capsule3d::new(radius, length).into(),
        }
    }
}

/// How an agent looks.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visual {
    pub shape: Shape,
    pub color: Rgb,
}

/// How an agent moves when it has no target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Roaming {
    /// Walks the level path.
    FollowPath,
    /// Guards the area of radius `sight` around where it was placed.
    Guard,
}

/// A melee attack.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeRecipe {
    pub damage: f32,
    /// Distance at which the attack hits, in world units.
    pub range: f32,
    /// Time between hits.
    #[serde(with = "humantime_serde")]
    pub interval: Duration,
}

/// An enemy or defender variant.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRecipe {
    pub health: f32,
    /// Walking speed, in world units per second.
    pub speed: f32,
    /// Vision radius, in world units. Also the area radius of a guard.
    pub sight: f32,
    pub targets: Targets,
    pub movement: Roaming,
    pub melee: Option<MeleeRecipe>,
    pub visual: Visual,
}

impl AgentRecipe {
    fn spawn(
        &self,
        commands: &mut Commands,
        look: Option<&Look>,
        category: impl Bundle,
        category_tag: &str,
        id: &str,
        position: Vec2,
    ) -> Entity {
        let mut agent = commands.spawn((
            Name::new(id.to_owned()),
            category,
            Tags::new([category_tag, id]),
            Health::new(self.health),
            Speed(self.speed),
            self.targets.clone(),
            Transform::from_translation(
                position.on_ground() + Vec3::Y * self.visual.shape.height(),
            ),
            DespawnOnExit(AppState::InLevel),
        ));

        match self.movement {
            Roaming::FollowPath => agent.insert((
                FollowPath::default(),
                Sight {
                    radius: self.sight,
                    anchor: None,
                },
            )),
            Roaming::Guard => agent.insert((
                Guard {
                    home: position,
                    radius: self.sight,
                },
                Sight {
                    radius: self.sight,
                    anchor: Some(position),
                },
            )),
        };

        if let Some(melee) = &self.melee {
            agent.insert(Melee::new(melee.damage, melee.range, melee.interval));
        }

        if let Some(look) = look {
            agent.insert((
                Mesh3d(look.mesh.clone()),
                MeshMaterial3d(look.material.clone()),
            ));
        }

        agent.id()
    }
}

/// What eating a harvested crop does: it multiplies mana regeneration for a while.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EatEffect {
    pub regeneration_multiplier: f32,
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
}

/// How a crop looks while growing and once ripe.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CropVisual {
    pub shape: Shape,
    pub color: Rgb,
    pub ripe_color: Rgb,
}

/// A crop variant.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CropRecipe {
    pub health: f32,
    /// Time for a planted or harvested crop to ripen.
    #[serde(with = "humantime_serde")]
    pub growth_time: Duration,
    /// Items gained per harvest.
    #[serde(rename = "yield")]
    pub harvest_yield: u32,
    /// What eating one harvested item does, if it is edible.
    pub eat: Option<EatEffect>,
    pub visual: CropVisual,
}

/// Every content variant, by id.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Recipes {
    pub enemies: BTreeMap<String, AgentRecipe>,
    pub defenders: BTreeMap<String, AgentRecipe>,
    pub crops: BTreeMap<String, CropRecipe>,
}

impl Recipes {
    /// Spawns the enemy `id` at `position`, or nothing if there is no such recipe.
    pub fn spawn_enemy(
        &self,
        commands: &mut Commands,
        visuals: &RecipeVisuals,
        id: &str,
        position: Vec2,
    ) -> Option<Entity> {
        let recipe = self.enemies.get(id)?;

        Some(recipe.spawn(
            commands,
            visuals.enemies.get(id),
            Enemy,
            ENEMY_TAG,
            id,
            position,
        ))
    }

    /// Spawns the defender `id` guarding `position`, or nothing if there is no such recipe.
    pub fn spawn_defender(
        &self,
        commands: &mut Commands,
        visuals: &RecipeVisuals,
        id: &str,
        position: Vec2,
    ) -> Option<Entity> {
        let recipe = self.defenders.get(id)?;

        Some(recipe.spawn(
            commands,
            visuals.defenders.get(id),
            Defender,
            DEFENDER_TAG,
            id,
            position,
        ))
    }

    /// Plants the crop `id` at `position`, or nothing if there is no such recipe.
    pub fn spawn_crop(
        &self,
        commands: &mut Commands,
        visuals: &RecipeVisuals,
        id: &str,
        position: Vec2,
    ) -> Option<Entity> {
        let recipe = self.crops.get(id)?;
        let mut crop = commands.spawn((
            Name::new(id.to_owned()),
            Crop,
            Tags::new([CROP_TAG, id]),
            Health::new(recipe.health),
            Growth::new(recipe.growth_time),
            Yield {
                item: id.to_owned(),
                amount: recipe.harvest_yield,
            },
            Transform::from_translation(
                position.on_ground() + Vec3::Y * recipe.visual.shape.height(),
            ),
            DespawnOnExit(AppState::InLevel),
        ));

        if let Some(look) = visuals.crops.get(id) {
            crop.insert((
                Mesh3d(look.mesh.clone()),
                MeshMaterial3d(look.growing.clone()),
                GrowthLook {
                    growing: look.growing.clone(),
                    ripe: look.ripe.clone(),
                },
            ));
        }

        Some(crop.id())
    }
}

#[derive(Debug)]
struct Look {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Debug)]
struct CropLook {
    mesh: Handle<Mesh>,
    growing: Handle<StandardMaterial>,
    ripe: Handle<StandardMaterial>,
}

/// Meshes and materials of every recipe, shared by all entities spawned from it.
#[derive(Resource, Debug, Default)]
pub struct RecipeVisuals {
    enemies: BTreeMap<String, Look>,
    defenders: BTreeMap<String, Look>,
    crops: BTreeMap<String, CropLook>,
}

impl RecipeVisuals {
    /// Builds one mesh and material set per recipe.
    pub fn build(
        recipes: &Recipes,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let mut agent_looks = |agents: &BTreeMap<String, AgentRecipe>| {
            agents
                .iter()
                .map(|(id, recipe)| {
                    let look = Look {
                        mesh: meshes.add(recipe.visual.shape.mesh()),
                        material: materials.add(Color::from(recipe.visual.color)),
                    };

                    (id.clone(), look)
                })
                .collect::<BTreeMap<_, _>>()
        };
        let enemies = agent_looks(&recipes.enemies);
        let defenders = agent_looks(&recipes.defenders);
        let crops = recipes
            .crops
            .iter()
            .map(|(id, recipe)| {
                let look = CropLook {
                    mesh: meshes.add(recipe.visual.shape.mesh()),
                    growing: materials.add(Color::from(recipe.visual.color)),
                    ripe: materials.add(Color::from(recipe.visual.ripe_color)),
                };

                (id.clone(), look)
            })
            .collect();

        Self {
            enemies,
            defenders,
            crops,
        }
    }
}

/// Access to the recipes that keeps their visuals in step with them.
#[derive(SystemParam)]
pub struct RecipeStore<'w> {
    recipes: ResMut<'w, Recipes>,
    visuals: ResMut<'w, RecipeVisuals>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

impl RecipeStore<'_> {
    /// The current recipes.
    pub fn recipes(&self) -> &Recipes {
        &self.recipes
    }

    /// Replaces the recipes and rebuilds their visuals. Live entities keep their values.
    pub fn replace(&mut self, recipes: Recipes) {
        *self.visuals = RecipeVisuals::build(&recipes, &mut self.meshes, &mut self.materials);
        *self.recipes = recipes;
    }
}

/// Holds the recipes and their visuals.
pub struct ContentPlugin;

impl Plugin for ContentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Recipes>()
            .init_resource::<RecipeVisuals>();
    }
}

/// Recipes shared by tests of the plugins that spawn content.
#[cfg(test)]
pub mod testing {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    /// A plain sphere.
    pub fn visual() -> Visual {
        Visual {
            shape: Shape::Sphere { radius: 0.3 },
            color: Rgb(1.0, 1.0, 1.0),
        }
    }

    /// An enemy walking the path at `speed`, attacking `targets` within `sight`.
    pub fn enemy(targets: &[&str], speed: f32, sight: f32) -> AgentRecipe {
        AgentRecipe {
            health: 3.0,
            speed,
            sight,
            targets: Targets::Only(targets.iter().map(|tag| (*tag).to_owned()).collect()),
            movement: Roaming::FollowPath,
            melee: Some(MeleeRecipe {
                damage: 1.0,
                range: 0.65,
                interval: Duration::from_secs(1),
            }),
            visual: visual(),
        }
    }

    /// A guard with an area of `radius` that attacks enemies.
    pub fn defender(radius: f32) -> AgentRecipe {
        AgentRecipe {
            health: 10.0,
            speed: 4.0,
            sight: radius,
            targets: Targets::Only(vec![ENEMY_TAG.to_owned()]),
            movement: Roaming::Guard,
            melee: Some(MeleeRecipe {
                damage: 1.0,
                range: 0.7,
                interval: Duration::from_millis(500),
            }),
            visual: visual(),
        }
    }

    /// An edible crop that ripens after `growth_time`.
    pub fn crop(growth_time: Duration) -> CropRecipe {
        CropRecipe {
            health: 3.0,
            growth_time,
            harvest_yield: 1,
            eat: Some(EatEffect {
                regeneration_multiplier: 3.0,
                duration: Duration::from_secs(5),
            }),
            visual: CropVisual {
                shape: Shape::Sphere { radius: 0.3 },
                color: Rgb(0.0, 1.0, 0.0),
                ripe_color: Rgb(1.0, 0.0, 0.0),
            },
        }
    }

    /// Test helpers for recipes in an [`App`].
    pub trait RecipeApp {
        /// Adds the content plugin with these recipes and their visuals.
        fn with_recipes(&mut self, recipes: Recipes) -> &mut Self;

        /// Spawns the enemy `id` at `position` and applies the commands.
        fn spawn_enemy(&mut self, id: &'static str, position: Vec2) -> Entity;
    }

    impl RecipeApp for App {
        fn with_recipes(&mut self, recipes: Recipes) -> &mut Self {
            if !self.is_plugin_added::<ContentPlugin>() {
                self.add_plugins(ContentPlugin);
            }

            self.world_mut()
                .run_system_once(move |mut store: RecipeStore| store.replace(recipes.clone()))
                .expect("recipes can be stored");

            self
        }

        fn spawn_enemy(&mut self, id: &'static str, position: Vec2) -> Entity {
            self.world_mut()
                .run_system_once(
                    move |mut commands: Commands,
                          recipes: Res<Recipes>,
                          visuals: Res<RecipeVisuals>| {
                        recipes.spawn_enemy(&mut commands, &visuals, id, position)
                    },
                )
                .expect("the spawn system runs")
                .expect("the enemy recipe exists")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{RecipeApp, enemy};
    use super::*;
    use crate::game::testing::TestApp;

    fn recipes() -> Recipes {
        Recipes {
            enemies: BTreeMap::from([("a".to_owned(), enemy(&["b"], 2.0, 3.0))]),
            ..default()
        }
    }

    #[test]
    fn spawned_enemy_has_category_and_id_tags() {
        let mut app = App::headless();
        app.with_recipes(recipes());

        let enemy = app.spawn_enemy("a", Vec2::new(1.0, 2.0));

        let world = app.world();
        assert_eq!(world.get::<Tags>(enemy), Some(&Tags::new([ENEMY_TAG, "a"])));
        assert!(world.get::<Enemy>(enemy).is_some());
        assert_eq!(world.get::<Intent>(enemy), Some(&Intent::Idle));
        assert_eq!(world.get::<Speed>(enemy).unwrap().0, 2.0);
        assert_eq!(world.get::<Sight>(enemy).unwrap().radius, 3.0);
        assert!(world.get::<FollowPath>(enemy).is_some());
        assert!(world.get::<Mesh3d>(enemy).is_some());
        assert_eq!(
            world.get::<Transform>(enemy).unwrap().translation.xz(),
            Vec2::new(1.0, 2.0)
        );
    }

    #[test]
    fn reload_keeps_live_stats() {
        let mut app = App::headless();
        app.with_recipes(recipes());
        let veteran = app.spawn_enemy("a", Vec2::ZERO);

        let mut stronger = recipes();
        stronger.enemies.get_mut("a").unwrap().health = 10.0;
        app.with_recipes(stronger);
        let recruit = app.spawn_enemy("a", Vec2::ZERO);

        assert_eq!(app.world().get::<Health>(veteran).unwrap().max(), 3.0);
        assert_eq!(app.world().get::<Health>(recruit).unwrap().max(), 10.0);
    }

    #[test]
    fn unknown_recipe_spawns_nothing() {
        let mut commands_world = World::new();
        let mut commands = commands_world.commands();

        assert!(
            recipes()
                .spawn_enemy(
                    &mut commands,
                    &RecipeVisuals::default(),
                    "missing",
                    Vec2::ZERO
                )
                .is_none()
        );
    }
}
