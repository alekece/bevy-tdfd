# Repository Guidance

Shared policy for Codex, GitHub Copilot and Claude Code. [`REVIEW.md`](REVIEW.md) holds the review rules and the risk levels used to decide how a PR is merged.

Never put secrets in logs, commits or assets metadata.

## Project Purpose

Top Dungeon Farmer Defense is a cozy low-poly 3D tower defense with farming, built with Bevy. It is a learning project: the maintainer is an experienced Rust developer learning game development (ECS architecture, plugins, assets and hot reloading, shaders, Blender, audio, game design, debugging). Code and explanations serve that goal.

## How We Work Together

- The agent writes the code. Each step is one complete, bounded plugin or feature slice that leaves the game runnable, not one tiny system at a time and not a whole stage at once.
- Each step comes with an explanation in the PR: why the plugin exists and why it is isolated, what it owns, how it talks to other plugins (components, messages, states, system sets), and the alternatives considered.
- Link references with each step: the official Bevy examples and docs, the Bevy Cheat Book, good open-source Bevy projects, and how Unity DOTS or Unreal Mass solve the same problem when the comparison helps.
- Prefer the idiomatic Bevy way over a pattern carried over from backend services, and say so when the two differ.

## Working with Existing Code

- Before modifying code, inspect the relevant implementation, callers and tests. Keep changes focused and preserve behavior outside the requested scope.

## Testing and Verification

- Test game rules and invariants (damage, economy, placement, growth, wave scripts) with headless `App` tests: `MinimalPlugins`, the plugin under test, then `app.update()`. Do not test rendering, input devices or Bevy's own behavior.
- Keep tests deterministic: drive time with `TimeUpdateStrategy::ManualDuration`, and draw randomness from a seeded RNG resource, never an unseeded one.
- Put pure rules (damage formulas, costs, placement checks) on types so they can be unit tested without an `App`.
- Before declaring a change complete, run the commands in [Commands](#commands). For gameplay or visual changes, also run the game and check the behavior; attach a screenshot or short clip to the PR when the change is visible.
- Report checks that could not run; never weaken a meaningful test to make code pass.

## Documentation and Development Workflow

- `README.md` is for humans discovering the project: what the game is, its current status, how to install, run and check it, and where the deeper docs live. Keep it short and scannable, like the READMEs of Bevy or ripgrep. It is not a changelog, a design document or a list of every system. Details go in `docs/designs/`, `docs/adr/` or module docs, and the README links to them. Update it in the same change whenever setup, controls, status or features a player would notice change.
- `docs/designs/` holds approved designs, committed. Update an evolving design in place after approved changes; Git keeps the history.
- `docs/adr/` records significant architectural decisions (for example level authoring, physics, UI stack), their alternatives and consequences. Not one per dependency.
- `docs/plans/` holds implementation plans in the worktree. Git ignores it; permanent documents never link to plans.
- Do not add duplicate harness-specific guidance. This file is the shared policy.

### Superpowers delivery workflow

1. Use Superpowers brainstorming to explore the outcome, scope and approaches.
2. Write a design and obtain explicit user approval of the written design before implementation. Approval of an approach alone does not approve the written design.
3. Before planning, identify or create a focused GitHub issue (motivation, observable acceptance criteria, constraints, references) with `gh issue create`. Describe outcomes, not task lists; check for duplicates first. Link the issue from the design.
4. Use Superpowers writing-plans to create a plan from the approved design, referencing the design and the issue.
5. Implement only the approved design, following the plan, in an isolated worktree under `.worktrees/`. Name the branch `<type>/<issue>-slug`, where `<type>` is a conventional commit type (`feat`, `fix`, `docs`, ...), for example `feat/3-farming-plugin`. Per-task review is optional and quick: one lightweight pass on that task's diff for obvious bugs.
6. After the last task, run the checks and request one independent review: a fresh code-review subagent that did not write the code, given [`REVIEW.md`](REVIEW.md). It states the risk level. Fix blocking findings, rerun the checks and have a fresh reviewer re-check the changed code; after three rounds with blocking findings left, stop and ask the user. Then do a refactor pass within the approved scope and record its result. If no fresh reviewer can be started, record "not independently reviewed" and treat the PR as at least medium risk.
7. Create a PR referencing the issue, written for humans: what changed and why, the plugin explanation and references (see [How We Work Together](#how-we-work-together)), how to try it, verification evidence and limitations. Add a **Review and risk** section with the risk level and a one-line reason, the reviewer's verdict and non-blocking findings, the blocking findings fixed, and the checks run.
8. Always squash-merge, never merge locally. Low risk: once CI is green and no blocking finding or open user thread remains, recommend merging and wait for the user's approval. Medium and high: request the user's explicit approval.
9. After a PR merges, remove its worktree, local branch and remote branch, provided no uncommitted work needs preserving.

These rules take precedence over skill defaults, including those that allow implementation without a worktree, local merges, or designs and plans under `docs/superpowers/`.

## Bevy Guidance

### ECS design

These rules shape every other Bevy rule below. When a Rust rule seems to conflict with them, they win.

- An entity is the set of its components. Build defenders, enemies, crops and items from behaviour components (`Health`, `Speed`, `Melee`, `Guard`, `Targets`, `Growth`, `Sellable`, ...), not from one component that holds a creature's whole state and logic.
- Two kinds of plugins. **Generic plugins** own a reusable behaviour (movement, health, targeting, melee, guarding, growth, trade) that could be extracted from this game; they never name game content (`Ant`, `KnightBunny`, `tomato`) and get what varies from components, recipes or their constructor. **Game plugins** hold rules that only make sense in this game (the farmer's actions, ant colony behaviour, the Blue Moon) and may name its content directly. Move a rule into a generic plugin when a second use appears.
- Content variants (defenders, enemies, crops, items) are typed RON recipes. A category marker (`Defender`, `Enemy`, `Crop`) brings its required components with `#[require(..)]`, using defaults that are not design decisions; shared tunable values come from a category section in RON. A recipe lists only what is unique: stats, targets, visual and optional capabilities such as `melee: Some(..)`. A variant that only combines existing behaviours needs no Rust. A new behaviour is written in Rust first, then attached from RON.
- Unique characters and singletons (the farmer, the helper rabbits, the peddler, the crystal) are not variants: they may have their own marker and a plugin that queries it, reuse the same behaviour components, and keep their numbers in RON.
- Relations between entities are data, not types: who an entity attacks is a component (`Targets::Anyone`, `Targets::Only([..])`) that systems read, so the game can change it at runtime (the Blue Moon).
- Each agent makes one decision per tick: a small state component picks its intent (follow the path, go to a crop, attack the crystal); generic movement and combat systems carry it out, ordered decide → move → act → resolve with system sets.
- Spawning copies recipe values into components. Hot reload changes future spawns and restarted levels, never the state of live entities.
- Content in an approved design under `docs/designs/` is not hypothetical: when a step touches a behaviour, shape it for the designed variants of that step's content (for example `Targets` with `Only` and `Anyone` even if only `Only` ships). Do not build behaviours the step does not touch.

### Plugins and boundaries

- One plugin per gameplay responsibility (map, movement, combat, farming, waves, economy, ...), in its own module. A plugin owns its components, resources, messages and systems, and registers them all in its `build`.
- Plugins talk through a small set of shared components (`Health`, ...), messages, observers, states and system sets; never by querying another plugin's private components. A concept two plugins need moves to a shared module.
- Prefer independent, reusable plugins over interdependent ones. Each plugin exposes a clear public surface others can hook into (components, messages, events, system sets). Generic plugins are configured through their constructor or data instead of hard-coding the content they serve. Make configurable what actually varies between its real uses, including the designed variants of the current step's content.
- When a plugin needs another one, it depends only on that plugin's public surface and says so in its `//!` overview.
- Keep `main.rs` to app setup: window, default plugins and the list of game plugins.
- Gate debug-only plugins (inspector, gizmos, cheats) behind the `dev` feature.

### Game flow and scheduling

- Model game flow with `States` (loading, menu, in a level, results) and sub-states for nested flow (paused, Blue Moon). Gate gameplay systems with `run_if(in_state(..))`.
- Tag entities spawned for a state with `DespawnOnExit` so leaving the state cleans them up.
- Order systems with named `SystemSet`s, configured once with `configure_sets`, not with `.after()` chains between plugins. Whenever one system reads what another writes in the same tick, order them explicitly, inside a plugin too (a set or `.chain()`); conflicting access alone does not define an order.
- Run simulation (movement, combat, growth, waves) in `FixedUpdate` so it is frame-rate independent and reproducible; keep input, camera and visuals in `Update`.
- Scale every change over time by `Time` or drive it with a `Timer`; never assume a frame rate.

### Communication

- Use a **message** (`MessageWriter`/`MessageReader`) for something many systems produce or consume in batch each frame (damage, crop harvested).
- Use an **observer** (`On<..>`) for a reaction that must happen immediately and rarely (an entity died, a defender was placed, a UI button was clicked).
- Use **change detection** (`Changed<..>`, `Added<..>`) instead of a message when the change is already a component write.
- Use a component marker plus a query instead of storing entity lists in resources.

### Entities and data

- Prefer small components with one meaning over large ones. Use category markers (`Enemy`, `Crop`) for queries, and `#[require(..)]` so spawning a marker brings its required components.
- Use `Single<..>` for an entity that must exist exactly once (the camera, the crystal); it skips the system when absent.
- Keep tunable gameplay data (stats, costs, growth times, waves, levels) in RON assets under `assets/data/` that hot reload; Rust constants only for values that are not design decisions.
- Load assets once and share their handles through a resource; never create a mesh or material per entity per frame.
- Randomness goes through one seeded RNG resource so tests and replays can reproduce it.

### Assets and content

- Layout: `assets/models/` (glTF exports), `assets/textures/`, `assets/audio/`, `assets/shaders/` (WGSL), `assets/data/` (RON). Blender sources live in `art/` and are committed; exports go to `assets/`.
- Name files in `snake_case` after the game concept (`red_bear.glb`, `blue_moon.wgsl`).
- Every third-party or AI-generated asset gets a line in `assets/CREDITS.md`: source or tool, author, license, and the date it was made. Do not commit assets whose license forbids redistribution.
- Keep the low-poly look consistent: shared palette and materials rather than per-model one-offs.

### Shaders

- Write shaders in WGSL under `assets/shaders/` so they hot reload. Start from `ExtendedMaterial<StandardMaterial, ..>` before writing a full custom `Material`.
- Keep the Rust uniform struct and the WGSL binding in the same order and document which binding each field maps to.

### Performance

- Query only the components a system needs, filtered with `With`/`Without`; avoid per-frame allocations in hot systems.
- Profile before optimizing: Bevy's `trace_tracy` feature with Tracy, or the frame-time diagnostics overlay in `dev` builds.

## Rust Guidance

### Types and functions

- Add a trait or wrapper only when it gives a concept a clear interface or enforces a contract callers would otherwise have to remember. Composing entities from behaviour components is not such an abstraction; it is the default (see [ECS design](#ecs-design)).
- Keep generic mechanisms (damage, area checks, timers) separate from content-specific rules. Do not refactor unrelated code; mention a worthwhile improvement in the PR instead.
- Encode invariants in types: enums or newtypes when they make an invalid state unrepresentable. Single-field components (`Speed(f32)`) and markers are fine without an invariant: in ECS, the type is what systems query.
- Replace boolean or stringly-typed flags with enums.
- Prefer one generic type over several near-identical ones when only a type parameter differs. Use static dispatch unless runtime polymorphism is needed.
- Time is a `Duration` or a `Timer`, never a bare number with a unit suffix (`delay_secs`). Distances and speeds are `f32` world units; document the unit on the field.
- Avoid free functions: put behavior on the type that owns the data, a standard trait (`From`, `Default`, ...), an extension trait or a small new type. **Exception:** Bevy systems, observers and run conditions are functions by design; write them as functions registered by the plugin that owns them, and keep the rules they apply on types where they can be unit tested.
- Group a system's many parameters with `#[derive(SystemParam)]` when it improves readability; the Clippy lints `too_many_arguments` and `type_complexity` are allowed for that reason, as in Bevy itself.
- Make items private by default. Derive a trait only when something uses it; derive `Reflect` on components and resources that should show in the inspector or be set from Blender.
- Prefer a maintained, widely used crate (for example `derive_more`) over hand-written boilerplate; declare dependencies with the minimum features. Record significant choices (physics, UI, audio, level authoring) in an ADR.

### Ownership

- Make data flow explicit. Do not clone, wrap in `Arc` or add interior mutability unless the borrow checker or an API requires it; Bevy resources and components already provide shared access.
- Background work (asset generation, pathfinding) runs on Bevy's task pools; every spawned task has an owner that polls or cancels it.

### Code organization

- One module per plugin. Use `module.rs` plus `module/child.rs`; never `mod.rs`.
- Order a file as constants, types, traits, implementations, then the plugin, then systems; put a type's `impl` blocks right under it.
- Put every `use` at the top of its module scope. Import types by name; import a function's parent module and call through it.
- Keep a module's tests in its own `#[cfg(test)] mod tests`.

### Naming

- Descriptive names that do not repeat their context. Use the game's domain terms: crop, plot, farmer, defender, ant, locust, wave, crystal, Blue Moon, peddler.
- Name a system after what it does (`grow_crops`, `spawn_wave`). Name a variable after the value it holds (`target`, `harvested_crops`), not the step that produced it (`result`, `done`), and a field after its role.
- Shadow a variable when you transform it without changing what it means; use distinct names when both forms are still needed.

### Readability and errors

- Always put a blank line before and after control-flow blocks, after a group of `let` declarations, and before `return` unless it is the only statement in its scope. Keep logically related lines together.
- Prefer guard clauses and `let .. else { continue }` over deep nesting. Use `Option`/`Result` and iterator combinators for value transformations, `for` loops for side effects.
- Asset loading and save data return typed errors (`thiserror`) that name the bad field; validate gameplay values (positive health, ordered waves, ...) when data loads, not where it is used. Treat a violated invariant in code as a bug, not a recoverable error.
- Game code tolerates missing entities: an entity despawned this frame is normal, not an error. Use `expect` with a message naming the invariant only when absence is a bug. `unwrap` and `expect` are fine in tests.

### Comments and docs

- Comments explain why (invariants, tradeoffs, ordering that matters), not what.
- Doc comments describe the item's own contract for a human reader; no unrelated context, cross-references or implementation details.
- A plugin module starts with a short `//!` overview of what it owns and how it connects to the rest.
- Put doc comments before all other attributes. Write documentation while implementing; it is part of "done".

## Commands

- Format: `cargo +nightly fmt --all` (nightly is needed for `group_imports` in `rustfmt.toml`)
- Lint: `cargo clippy --all-targets --locked -- -D warnings`
- Test: `cargo test --locked`
- Run: `cargo run --features dev` (dynamic linking and asset hot reloading)
- After a task is finished and no build or test is running, run `cargo clean` in its worktree; never clean another active worktree.
