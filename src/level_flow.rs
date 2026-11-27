//! Menu, level timeline and results.
//!
//! Owns the level's start and end transitions. It reads map, enemy and crystal state through public
//! resources and components; the other gameplay plugins reset their own resources when a level
//! starts.

use std::time::Duration;

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use crate::combat::Health;
use crate::game::{
    AppState, Difficulty, GameSet, LevelClock, LevelOutcome, LevelStatus, SelectedDifficulty,
    Timeline,
};
use crate::map::Crystal;
use crate::waves::WaveScript;

const MENU_BACKGROUND: Color = Color::srgb(0.13, 0.18, 0.2);
const BUTTON_COLOR: Color = Color::srgb(0.28, 0.44, 0.34);

/// The result of the most recently played level.
#[derive(Debug, Clone, Copy)]
pub struct LevelResult {
    outcome: LevelOutcome,
    crystal_health_percent: f32,
}

impl LevelResult {
    /// Whether the player completed the level.
    pub fn outcome(&self) -> LevelOutcome {
        self.outcome
    }

    /// The crystal's remaining health as a percentage.
    pub fn crystal_health_percent(&self) -> f32 {
        self.crystal_health_percent
    }
}

#[derive(Resource, Debug, Default)]
struct LevelResults(Option<LevelResult>);

/// Starts the level on the selected difficulty.
#[derive(Event, Debug)]
pub struct StartLevel {
    pub difficulty: Difficulty,
}

#[derive(Component, Debug)]
struct DifficultyButton(Difficulty);

#[derive(Component, Debug)]
struct TimelineText;

#[derive(Component, Debug)]
struct ResultsText;

/// Owns the menu, timeline display and level outcome transitions.
pub struct LevelFlowPlugin;

impl Plugin for LevelFlowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelResults>()
            .add_observer(start_level)
            .add_observer(select_difficulty_button)
            .add_systems(OnEnter(AppState::Menu), spawn_menu)
            .add_systems(
                Update,
                select_difficulty_key.run_if(in_state(AppState::Menu)),
            )
            .add_systems(OnEnter(AppState::InLevel), (reset_result, spawn_timeline))
            .add_systems(Update, update_timeline.in_set(GameSet::Presentation))
            .add_systems(FixedUpdate, check_level_end.in_set(GameSet::Outcome))
            .add_systems(OnEnter(AppState::Results), spawn_results)
            .add_systems(Update, return_to_menu.run_if(in_state(AppState::Results)));
        app.add_systems(Update, update_results.run_if(in_state(AppState::Results)));
    }
}

fn spawn_menu(mut commands: Commands) {
    commands.spawn((
        Name::new("Difficulty menu"),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(18),
            ..default()
        },
        BackgroundColor(MENU_BACKGROUND),
        DespawnOnExit(AppState::Menu),
        children![
            (
                Text::new("Top Dungeon Farmer Defense"),
                TextFont::from_font_size(36.0),
                TextColor(Color::WHITE)
            ),
            (
                Text::new("Choose your difficulty"),
                TextFont::from_font_size(22.0),
                TextColor(Color::WHITE)
            ),
            (
                Button,
                DifficultyButton(Difficulty::Easy),
                Node {
                    width: px(220),
                    height: px(48),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(BUTTON_COLOR),
                children![(
                    Text::new("Easy"),
                    TextFont::from_font_size(20.0),
                    TextColor(Color::WHITE)
                )]
            ),
            (
                Button,
                DifficultyButton(Difficulty::Normal),
                Node {
                    width: px(220),
                    height: px(48),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(BUTTON_COLOR),
                children![(
                    Text::new("Normal"),
                    TextFont::from_font_size(20.0),
                    TextColor(Color::WHITE)
                )]
            ),
            (
                Button,
                DifficultyButton(Difficulty::Hard),
                Node {
                    width: px(220),
                    height: px(48),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(BUTTON_COLOR),
                children![(
                    Text::new("Hard"),
                    TextFont::from_font_size(20.0),
                    TextColor(Color::WHITE)
                )]
            ),
            (
                Text::new("Or press 1, 2 or 3"),
                TextFont::from_font_size(16.0),
                TextColor(Color::WHITE)
            ),
        ],
    ));
}

fn select_difficulty_button(
    event: On<Activate>,
    buttons: Query<&DifficultyButton>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(event.entity) else {
        return;
    };

    commands.trigger(StartLevel {
        difficulty: button.0,
    });
}

fn select_difficulty_key(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    let difficulty = if keys.just_pressed(KeyCode::Digit1) {
        Some(Difficulty::Easy)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(Difficulty::Normal)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(Difficulty::Hard)
    } else {
        None
    };

    if let Some(difficulty) = difficulty {
        commands.trigger(StartLevel { difficulty });
    }
}

fn start_level(
    event: On<StartLevel>,
    state: Res<State<AppState>>,
    mut selected: ResMut<SelectedDifficulty>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if *state.get() != AppState::Menu {
        return;
    }

    selected.0 = event.difficulty;
    next_state.set(AppState::InLevel);
}

fn reset_result(mut result: ResMut<LevelResults>) {
    result.0 = None;
}

fn spawn_timeline(mut commands: Commands) {
    commands.spawn((
        Name::new("Level timeline"),
        Node {
            position_type: PositionType::Absolute,
            right: px(10),
            top: px(10),
            ..default()
        },
        DespawnOnExit(AppState::InLevel),
        children![(
            TimelineText,
            Text::default(),
            TextFont::from_font_size(18.0)
        )],
    ));
}

fn update_timeline(
    clock: Res<LevelClock>,
    timeline: Res<Timeline>,
    waves: Res<WaveScript>,
    mut text: Single<&mut Text, With<TimelineText>>,
) {
    let elapsed = clock.elapsed();
    let time_label =
        |time: Duration| format!("{:02}:{:02}", time.as_secs() / 60, time.as_secs() % 60);
    let next_waves = waves
        .waves()
        .iter()
        .filter(|wave| wave.at > elapsed)
        .take(3)
        .map(|wave| {
            format!(
                "{} {} at {}",
                wave.count,
                wave.enemy.replace('_', " "),
                time_label(wave.at)
            )
        })
        .collect::<Vec<_>>()
        .join("  |  ");
    let phase = if elapsed >= timeline.sell_window_start() {
        "Final sell window"
    } else {
        "Next waves"
    };

    text.0 = format!(
        "Time {} / {}   {}: {}",
        time_label(elapsed),
        time_label(timeline.length),
        phase,
        if next_waves.is_empty() {
            "none".to_owned()
        } else {
            next_waves
        },
    );
}

fn check_level_end(
    clock: Res<LevelClock>,
    timeline: Res<Timeline>,
    crystals: Query<&Health, With<Crystal>>,
    mut status: ResMut<LevelStatus>,
    mut result: ResMut<LevelResults>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let mut crystal_health = crystals.iter();

    let level_result = match crystal_health.next() {
        None => LevelResult {
            outcome: LevelOutcome::Defeat,
            crystal_health_percent: 0.0,
        },
        Some(_) if crystal_health.next().is_some() => {
            error!("A level must have exactly one warp crystal");

            return;
        }
        Some(_) if !timeline.is_complete(clock.elapsed()) => return,
        Some(crystal) => LevelResult {
            outcome: LevelOutcome::Victory,
            crystal_health_percent: crystal.current() / crystal.max() * 100.0,
        },
    };

    *status = LevelStatus::Ended(level_result.outcome);
    result.0 = Some(level_result);
    next_state.set(AppState::Results);
}

fn spawn_results(mut commands: Commands, result: Res<LevelResults>) {
    let Some(result) = result.0 else {
        error!("The results screen requires a completed level");

        return;
    };
    let outcome = match result.outcome() {
        LevelOutcome::Victory => "Level complete!",
        LevelOutcome::Defeat => "The warp crystal was destroyed",
    };

    commands.spawn((
        Name::new("Level results"),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(18),
            ..default()
        },
        BackgroundColor(MENU_BACKGROUND),
        DespawnOnExit(AppState::Results),
        children![
            (
                Text::new(outcome),
                TextFont::from_font_size(32.0),
                TextColor(Color::WHITE)
            ),
            (
                ResultsText,
                Text::default(),
                TextFont::from_font_size(22.0),
                TextColor(Color::WHITE)
            ),
            (
                Text::new("Press R to return to the menu"),
                TextFont::from_font_size(16.0),
                TextColor(Color::WHITE)
            ),
        ],
    ));
}

fn update_results(result: Res<LevelResults>, mut text: Single<&mut Text, With<ResultsText>>) {
    if let Some(result) = result.0 {
        text.0 = format!(
            "Warp Crystal: {:.0}% remaining",
            result.crystal_health_percent()
        );
    }
}

fn return_to_menu(keys: Res<ButtonInput<KeyCode>>, mut next_state: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::KeyR) {
        next_state.set(AppState::Menu);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::combat::{CombatPlugin, Health};
    use crate::content::ContentPlugin;
    use crate::economy::{EconomyPlugin, Gold};
    use crate::farming::{
        FarmingConfig, FarmingPlugin, GrowthBoost, ManaConfig, RabbitConfig, SeedStock,
    };
    use crate::game::{
        AppState, Difficulty, DifficultySettings, SelectedDifficulty, Timeline,
        testing::{STEP, TestApp},
    };
    use crate::map::Crystal;
    use crate::map::testing::layout;
    use crate::stage::CursorGround;
    use crate::targeting::Targets;
    use crate::waves::Wave;

    fn timeline() -> Timeline {
        Timeline {
            length: Duration::from_secs(5),
            sell_window: Duration::from_secs(2),
        }
    }

    fn waves() -> WaveScript {
        WaveScript::from(vec![Wave {
            at: Duration::from_secs(1),
            enemy: "a".to_owned(),
            count: 1,
        }])
    }

    fn flow_app() -> App {
        let mut app = App::headless();

        app.add_plugins((CombatPlugin, LevelFlowPlugin))
            .insert_resource(timeline())
            .insert_resource(waves());
        app.world_mut().spawn((
            Crystal,
            Health::new(100.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        app.enter(AppState::InLevel);

        app
    }

    fn difficulty_app() -> App {
        let mut app = App::headless();
        let difficulties = ron::from_str::<DifficultySettings>(
            "(easy: (seeds: 5, gold: 30), normal: (seeds: 3, gold: 15), hard: (seeds: 1, gold: 5))",
        )
        .unwrap();

        let mut layout = layout();
        layout.plots = vec![Vec2::ZERO];

        app.add_plugins((LevelFlowPlugin, FarmingPlugin, EconomyPlugin, ContentPlugin))
            .init_resource::<CursorGround>()
            .insert_resource(FarmingConfig {
                seed: "a".to_owned(),
                action_time: Duration::from_secs(1),
                farmer_speed: 2.0,
                rabbits: RabbitConfig {
                    count: 0,
                    speed: 2.0,
                    harvest_time: Duration::from_secs(1),
                    flee_radius: 3.0,
                    flee_from: Targets::Anyone,
                },
                mana: ManaConfig {
                    max: 10.0,
                    regeneration: 1.0,
                },
                growth_boost: GrowthBoost {
                    mana_cost: 1.0,
                    cast_time: Duration::from_secs(1),
                    radius: 3.0,
                    growth_multiplier: 2.0,
                    duration: Duration::from_secs(5),
                },
            })
            .insert_resource(difficulties)
            .insert_resource(timeline())
            .insert_resource(waves())
            .insert_resource(layout)
            .enter(AppState::Menu);

        app
    }

    #[test]
    fn selected_difficulty_applies_starting_seeds_and_gold() {
        let mut app = difficulty_app();

        app.world_mut().trigger(StartLevel {
            difficulty: Difficulty::Hard,
        });
        app.update();

        assert_eq!(
            app.world().resource::<SelectedDifficulty>().0,
            Difficulty::Hard
        );
        assert_eq!(app.world().resource::<SeedStock>().seeds(), 1);
        assert_eq!(app.world().resource::<Gold>().amount(), 5);
    }

    #[test]
    fn level_wins_only_after_timeline_and_final_sell_window() {
        let mut app = flow_app();

        app.advance_by(Duration::from_secs(4));
        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::InLevel
        );

        let crystal = app
            .world_mut()
            .query_filtered::<Entity, With<Crystal>>()
            .single(app.world())
            .unwrap();
        app.world_mut().write_message(crate::combat::Damage {
            target: crystal,
            amount: 45.0,
        });
        app.advance_by(Duration::from_secs(1));

        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::Results
        );
        assert_eq!(
            app.world().resource::<LevelResults>().0.unwrap().outcome(),
            LevelOutcome::Victory
        );
        assert_eq!(
            app.world()
                .resource::<LevelResults>()
                .0
                .unwrap()
                .crystal_health_percent(),
            55.0
        );
    }

    #[test]
    fn level_stops_on_the_tick_it_ends() {
        let mut app = flow_app();
        app.world_mut()
            .insert_resource(TimeUpdateStrategy::ManualDuration(STEP * 4));
        let crystal = app
            .world_mut()
            .query_filtered::<Entity, With<Crystal>>()
            .single(app.world())
            .unwrap();
        app.world_mut().despawn(crystal);
        let before = app.world().resource::<LevelClock>().elapsed();

        // One frame runs four fixed ticks; only the first one, which ends the level, simulates.
        app.update();

        assert_eq!(
            app.world().resource::<LevelClock>().elapsed(),
            before + STEP
        );
        assert_eq!(
            *app.world().resource::<LevelStatus>(),
            LevelStatus::Ended(LevelOutcome::Defeat)
        );
    }

    #[test]
    fn crystal_destruction_causes_loss_before_timeline_end() {
        let mut app = flow_app();
        let crystal = app
            .world_mut()
            .query_filtered::<Entity, With<Crystal>>()
            .single(app.world())
            .unwrap();
        app.world_mut().despawn(crystal);

        app.advance_by(Duration::from_secs(1));

        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::Results
        );
        assert_eq!(
            app.world().resource::<LevelResults>().0.unwrap().outcome(),
            LevelOutcome::Defeat
        );
        assert_eq!(
            app.world()
                .resource::<LevelResults>()
                .0
                .unwrap()
                .crystal_health_percent(),
            0.0
        );
    }
}
