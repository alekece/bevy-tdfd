//! Targeting: who an agent goes after.
//!
//! Owns [`Tags`] (what an entity is), [`Targets`] (what an agent goes after) and [`Sight`] (how far
//! it looks). Each tick, in [`TargetingSet`] inside [`GameSet::Decide`], an agent keeps the target it
//! approaches while that target still exists, matches and is in sight; otherwise it picks the
//! nearest match of the highest priority in sight and approaches it, or drops its target.
//! The approach distance is the agent's [`Melee`] range from combat, so it stops where it can hit.
//! Systems that fall back to another intent when nothing is targeted run after [`TargetingSet`].

use bevy::prelude::*;
use serde::Deserialize;

use crate::combat::Melee;
use crate::game::GameSet;
use crate::movement::Intent;

/// Labels that describe what an entity is, for [`Targets`] to match.
#[derive(Component, Debug, Clone, PartialEq, Eq, Default)]
pub struct Tags(pub Vec<String>);

impl Tags {
    /// Tags from a list of labels.
    pub fn new(tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self(tags.into_iter().map(Into::into).collect())
    }

    /// Whether `tag` is one of the labels.
    pub fn contains(&self, tag: &str) -> bool {
        self.0.iter().any(|candidate| candidate == tag)
    }
}

/// Which tagged entities an agent goes after.
#[derive(Component, Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum Targets {
    /// Every tagged entity.
    Anyone,
    /// Entities carrying one of these tags. Earlier tags have priority over later ones.
    Only(Vec<String>),
}

impl Targets {
    /// The priority of an entity with `tags`, lower first, or `None` when it is not a target.
    pub fn priority(&self, tags: &Tags) -> Option<usize> {
        match self {
            Targets::Anyone => Some(0),
            Targets::Only(wanted) => wanted.iter().position(|tag| tags.contains(tag)),
        }
    }

    /// Whether an entity with `tags` is a target.
    pub fn matches(&self, tags: &Tags) -> bool {
        self.priority(tags).is_some()
    }
}

/// How far an agent sees targets.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Sight {
    /// Radius of vision, in world units.
    pub radius: f32,
    /// Fixed point vision is measured from, such as a guarded post; the agent itself when `None`.
    pub anchor: Option<Vec2>,
}

impl Sight {
    /// Whether an agent standing at `position` sees `point`.
    pub fn sees(&self, position: Vec2, point: Vec2) -> bool {
        self.anchor.unwrap_or(position).distance(point) <= self.radius
    }
}

/// The set [`acquire_targets`] runs in, inside [`GameSet::Decide`].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetingSet;

/// Picks targets for agents with [`Targets`] and [`Sight`].
pub struct TargetingPlugin;

impl Plugin for TargetingPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(FixedUpdate, TargetingSet.in_set(GameSet::Decide))
            .add_systems(FixedUpdate, acquire_targets.in_set(TargetingSet));
    }
}

fn acquire_targets(
    mut agents: Query<(
        Entity,
        &Transform,
        &Targets,
        &Sight,
        &mut Intent,
        Option<&Melee>,
    )>,
    candidates: Query<(Entity, &Transform, &Tags)>,
) {
    for (agent, transform, targets, sight, mut intent, melee) in &mut agents {
        let position = transform.translation.xz();
        let is_valid = |candidate: Entity| {
            candidates
                .get(candidate)
                .is_ok_and(|(_, candidate_transform, tags)| {
                    candidate != agent
                        && targets.matches(tags)
                        && sight.sees(position, candidate_transform.translation.xz())
                })
        };

        if intent.target().is_some_and(is_valid) {
            continue;
        }

        let target = candidates
            .iter()
            .filter(|(candidate, candidate_transform, _)| {
                *candidate != agent && sight.sees(position, candidate_transform.translation.xz())
            })
            .filter_map(|(candidate, candidate_transform, tags)| {
                let distance = candidate_transform.translation.xz().distance(position);

                targets
                    .priority(tags)
                    .map(|priority| (candidate, priority, distance))
            })
            .min_by(|(_, a_priority, a_distance), (_, b_priority, b_distance)| {
                a_priority
                    .cmp(b_priority)
                    .then(a_distance.total_cmp(b_distance))
            });

        match target {
            Some((target, _, _)) => {
                *intent = Intent::Approach {
                    target,
                    within: melee.map_or(0.0, |melee| melee.range),
                };
            }
            None if intent.target().is_some() => *intent = Intent::Idle,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::AppState;
    use crate::game::testing::TestApp;

    fn targeting_app() -> App {
        let mut app = App::headless();
        app.add_plugins(TargetingPlugin).enter(AppState::InLevel);

        app
    }

    fn spawn_tagged(app: &mut App, tags: &[&str], position: Vec2) -> Entity {
        app.world_mut()
            .spawn((
                Tags::new(tags.iter().copied()),
                Transform::from_xyz(position.x, 0.0, position.y),
            ))
            .id()
    }

    fn spawn_agent(app: &mut App, targets: Targets, sight: Sight, position: Vec2) -> Entity {
        app.world_mut()
            .spawn((
                targets,
                sight,
                Intent::Idle,
                Tags::new(["agent"]),
                Transform::from_xyz(position.x, 0.0, position.y),
            ))
            .id()
    }

    fn sight(radius: f32) -> Sight {
        Sight {
            radius,
            anchor: None,
        }
    }

    fn target_of(app: &App, agent: Entity) -> Option<Entity> {
        app.world().get::<Intent>(agent).unwrap().target()
    }

    #[test]
    fn only_targets_match_in_tag_priority_order() {
        let mut app = targeting_app();
        spawn_tagged(&mut app, &["c"], Vec2::new(0.3, 0.0));
        let near_b = spawn_tagged(&mut app, &["b"], Vec2::new(0.5, 0.0));
        let far_a = spawn_tagged(&mut app, &["a"], Vec2::new(2.5, 0.0));
        let agent = spawn_agent(
            &mut app,
            Targets::Only(vec!["a".into(), "b".into()]),
            sight(3.0),
            Vec2::ZERO,
        );

        app.update();
        assert_eq!(target_of(&app, agent), Some(far_a));

        app.world_mut().despawn(far_a);
        app.update();
        assert_eq!(target_of(&app, agent), Some(near_b));
    }

    #[test]
    fn anyone_targets_never_self() {
        let mut app = targeting_app();
        let agent = spawn_agent(&mut app, Targets::Anyone, sight(3.0), Vec2::ZERO);

        app.update();
        assert_eq!(target_of(&app, agent), None);

        let other = spawn_tagged(&mut app, &["anything"], Vec2::new(1.0, 0.0));
        app.update();
        assert_eq!(target_of(&app, agent), Some(other));
    }

    #[test]
    fn locked_target_is_kept_while_in_sight() {
        let mut app = targeting_app();
        let first = spawn_tagged(&mut app, &["a"], Vec2::new(2.0, 0.0));
        let agent = spawn_agent(
            &mut app,
            Targets::Only(vec!["a".into()]),
            sight(3.0),
            Vec2::ZERO,
        );
        app.update();
        assert_eq!(target_of(&app, agent), Some(first));

        let nearer = spawn_tagged(&mut app, &["a"], Vec2::new(0.5, 0.0));
        app.update();
        assert_eq!(target_of(&app, agent), Some(first));

        app.world_mut()
            .get_mut::<Transform>(first)
            .unwrap()
            .translation
            .x = 5.0;
        app.update();
        assert_eq!(target_of(&app, agent), Some(nearer));
    }

    #[test]
    fn despawned_target_is_dropped() {
        let mut app = targeting_app();
        let target = spawn_tagged(&mut app, &["a"], Vec2::new(1.0, 0.0));
        let agent = spawn_agent(
            &mut app,
            Targets::Only(vec!["a".into()]),
            sight(3.0),
            Vec2::ZERO,
        );
        app.update();
        assert_eq!(target_of(&app, agent), Some(target));

        app.world_mut().despawn(target);
        app.update();

        assert_eq!(*app.world().get::<Intent>(agent).unwrap(), Intent::Idle);
    }

    #[test]
    fn anchor_limits_sight_to_area() {
        let mut app = targeting_app();
        let near_agent = spawn_tagged(&mut app, &["a"], Vec2::new(1.0, 0.0));
        let near_anchor = spawn_tagged(&mut app, &["a"], Vec2::new(9.0, 0.0));
        let agent = spawn_agent(
            &mut app,
            Targets::Only(vec!["a".into()]),
            Sight {
                radius: 2.0,
                anchor: Some(Vec2::new(10.0, 0.0)),
            },
            Vec2::ZERO,
        );

        app.update();

        assert_eq!(target_of(&app, agent), Some(near_anchor));
        assert_ne!(target_of(&app, agent), Some(near_agent));
    }

    #[test]
    fn priority_follows_the_order_of_only() {
        let targets = Targets::Only(vec!["a".into(), "b".into()]);

        assert_eq!(targets.priority(&Tags::new(["b", "a"])), Some(0));
        assert_eq!(targets.priority(&Tags::new(["b"])), Some(1));
        assert_eq!(targets.priority(&Tags::new(["c"])), None);
        assert_eq!(Targets::Anyone.priority(&Tags::default()), Some(0));
    }
}
