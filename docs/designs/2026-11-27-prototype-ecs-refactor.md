# Prototype refactor: behaviour components and RON recipes

Status: approved for implementation (autopilot); review the decisions marked "my call" in the PR.
Issue: [#16](https://github.com/alekece/bevy-tdfd/issues/16). Builds on the first playable prototype (#11).

## Why

The prototype works but is shaped around its content. `Ant` and `KnightBunny` each own their whole
logic, the economy has `tomato_price` and `knight_bunny_cost`, and defenders query `With<Ant>`. A second
enemy or defender would mean copying a plugin. The [ECS design rules](../../AGENTS.md#ecs-design) ask for
the opposite: an entity is its behaviour components, generic plugins never name content, and variants are
RON recipes.

## Goals

- Same gameplay as the prototype: farmer, two rabbits, tomato, Growth Boost, eating, selling, green ants,
  Knight Bunny, waves, timeline, win and loss.
- Generic plugins (movement, combat, targeting, guarding, growth, trade) name no content.
- A variant that combines existing behaviours (a "big bunny", a "red ant", a "carrot") is RON only.
- Who attacks whom is data (`Targets`), never self.
- Explicit simulation order each tick; gameplay stops the tick the level ends.
- Hot reload changes future spawns and restarted levels only.
- Bad data fails at load with an error naming the field.

## Non-goals

- No new content or mechanics: no Blue Moon, Biter ants, locusts, wolves, cooking or peddler trips.
- No change to visuals beyond what recipes need (colour and simple shape).
- No separate content file per category yet; recipes live in the level file (my call: one asset to load
  and hot reload; split when a second level exists).

## Anti-goals

- A behaviour plugin that names `Ant`, `KnightBunny` or `tomato`.
- A "god" recipe system or reflection-driven component lists: recipes are typed serde structs.
- Abstractions for content no step uses (no ability system, no generic effect engine).

## Approaches considered

1. **Typed serde recipes (chosen).** Each category has a recipe struct with required stats and optional
   capabilities (`melee: Some(..)`); spawning maps a recipe to components. Typed, validated, easy to read.
   Costs a Rust change when a new capability appears, which is the intended split.
2. **Reflection-based RON (`DynamicScene`-like component lists).** Any component from RON with no Rust,
   but weak validation, verbose files, and no `.bsn` loader in Bevy 0.20. Rejected.
3. **Keep content plugins, add traits for variants.** Smallest diff, but stays object-oriented. Rejected.

## Architecture

### Schedule (`game.rs`)

`FixedUpdate` sets, chained: `Clock → Spawn → Decide → Move → Act → Resolve → Outcome`.
`Update` sets: `Input → Presentation`. All run only in `AppState::InLevel` **and** while
`LevelStatus::Running`; `LevelStatus` is a resource in `game.rs` set by level flow the tick the level ends,
so catch-up ticks in the same frame do nothing (my call over a sub-state, which only switches next frame).

Ordering inside a set is explicit (`.chain()`) wherever one system reads what another writes:
growth runs before harvesting, so a crop that ripens this tick can be harvested this tick.

### Generic plugins

| Plugin | Owns | Set |
|---|---|---|
| `movement` | `Speed` (world units/s), `Intent` (`Idle`, `MoveTo(Vec2)`, `Approach { target, within }`), `FollowPath` (walks the level path), `move_agents` | Decide (`follow_path`), Move |
| `targeting` | `Tags` (strings), `Targets` (`Anyone`, `Only([tags])`), `Sight { radius, anchor }`, `acquire_targets` | Decide |
| `guarding` | `Guard { home, radius }`: return home when idle, stay inside the area, area ring tied to the guard's life, `Guard::overlaps` for placement | Decide, Move |
| `combat` | `Health`, `Damage` message, `Melee { damage, range, timer }`, `strike`, `apply_damage` (despawn at zero) | Act, Resolve |
| `growth` | `Growth` (timer, ripe, harvest resets), `Boosted`, `Yield { item, amount }`, grow and ripe visuals | Act, Presentation |
| `economy` | `Gold`, `Inventory` (item → count, ordered), `Trade { buys, sells }` (item → gold), `SellAll` | Input/observer |

Rules:

- **Targeting.** Each tick an agent with `Targets` and `Sight` keeps its current `Approach` target while
  it still exists, still matches and is still in sight; otherwise it picks a new one, or goes `Idle`.
  The order of `Only([..])` is a priority: the agent picks the nearest match of the first tag that has one
  in sight, so ants choose a crop over the crystal. Sight is measured from `anchor` when set (a guard's home), else from the agent. An entity never
  targets itself. `Targets::Anyone` matches any tagged entity. My call: Knight Bunnies now keep a target
  until it leaves their area instead of switching to a nearer ant each tick; the difference is hard to see.
- **Fallback movement.** After targeting, an `Idle` path follower moves to its next waypoint, and an `Idle`
  guard moves home. Ants therefore eat a crop until it is gone, resume the path, and attack the crystal when
  it comes into sight at the end of the path.
- **Melee.** An agent with `Melee` and an `Approach` target within `range` ticks its attack timer and
  sends `Damage` when it fires. The timer only advances while in reach.
- **Placement** checks that the new guard area does not overlap another (`Guard::overlaps`) and that the
  point is on the field. Plants may sit inside an area.

My call: the old "crop must be near the path" filter is dropped; ant sight from its own position gives the
same result on the prototype map. The crystal is now picked whenever it is in sight rather than only after
the last waypoint; on the prototype map it is only in sight near the end of the path.

### Content (`content.rs`)

Category markers bring their defaults with `#[require]`: `Enemy` and `Defender` require `Intent`;
`Crop` requires nothing extra yet. `Recipes` holds three maps keyed by id: `enemies`, `defenders`, `crops`.
Spawning copies recipe values into components and adds `Tags([category, id])` (`"enemy"`, `"green_ant"`).
Meshes and materials are built once per recipe into a `RecipeVisuals` resource and rebuilt on reload.

```ron
enemies: {
    "green_ant": (
        health: 3.0,
        speed: 2.0,
        sight: 3.0,
        targets: Only(["crop", "crystal"]),
        movement: FollowPath,
        melee: Some((damage: 1.0, range: 0.65, interval: "1500ms")),
        visual: (shape: Sphere(radius: 0.35), color: (0.1, 0.48, 0.16)),
    ),
},
defenders: {
    "knight_bunny": (
        health: 12.0,
        speed: 3.0,
        sight: 3.0, // also the guard area radius
        targets: Only(["enemy"]),
        movement: Guard,
        melee: Some((damage: 1.0, range: 0.7, interval: "1s")),
        visual: (shape: Capsule(radius: 0.3, length: 0.6), color: (0.86, 0.68, 0.28)),
    ),
},
crops: {
    "tomato": (
        health: 3.0,
        growth_time: "25s",
        yield: 1,
        eat: Some((regeneration_multiplier: 3.0, duration: "1m")),
        visual: (shape: Sphere(radius: 0.35), color: (0.3, 0.75, 0.25), ripe_color: (0.9, 0.15, 0.1)),
    ),
},
```

Enemies and defenders share one `AgentRecipe`; the section they sit in decides the category marker.
`movement` is an enum (`FollowPath`, `Guard`), not flags.

### Game plugins

They may name game content where the rule needs it, and otherwise reuse the generic components.

- `map`: loads the level asset, validates it, spawns path, plots, spawner, crystal (`Tags(["crystal"])`,
  `Health`) and peddler.
- `waves`: the wave script `[(at, enemy, count)]`, sorted at load, spawns enemy recipes at the spawner.
- `farming`: farmer, rabbits, plots, seeds, mana, Growth Boost, planting, harvesting into `Inventory`,
  eating. The farmer and rabbits use `Speed` and `Intent::MoveTo` from movement; their decisions run in
  Decide and their work in Act. Rabbits flee anything matching `flee_from: Only(["enemy"])`.
  The level names the crop the farmer plants (`seed: "tomato"`); `E` eats the first edible item.
- `recruiting`: `K` starts placing the peddler's first defender offer, left click places it.
  `Placement` is an enum (`Idle`, `Placing { defender }`), replacing the boolean flag.
- `level_flow`, `hud`, `stage`: as today, reading the new public resources.

### Data and hot reload

Level file sections: `difficulties`, `timeline`, `map`, `farming`, `trade` (`buys: {"tomato": 5}`,
`sells: {"knight_bunny": 10}`), `waves`, `enemies`, `defenders`, `crops`.

- `Recipes` and `RecipeVisuals` update on every reload: future spawns use new values, live entities keep
  the values copied at spawn. If a reload during a level removes a recipe the running level still uses
  (its waves, seed or shop), the new recipes wait for the next level start and a warning says so.
- Every other section is copied into resources when a level starts, so a reload mid-level changes the next
  level, not the running one.

Load-time validation, each error naming its field (`enemies.green_ant.health must be greater than zero`):
every number finite; positive health, speed, sight, ranges, intervals, growth times and multipliers
(the Growth Boost check that exists today stays); waves and trade
referencing existing recipes; the farming seed and `trade.buys` naming crops; at least one defender for
sale. Waves are sorted by time instead of rejected.

## Tests

Headless `App` tests for the rules: targeting (`Only`, `Anyone`, never self, keeps a locked target,
anchor), path following then crystal attack, crop diversion, melee timer only in reach, guard leash and
ring despawn, crop priority over the crystal, placement (overlap, gold, cancel), growth then same-tick harvest, rabbits flee and harvest,
selling and spending, waves at their time, level freeze on the catch-up tick, recipe validation errors
(including negative, NaN and infinite multipliers), reload keeps live stats, a mid-level reload that renames
a used recipe is deferred. The prototype level parses and is internally consistent.

## Left to the builder

File split inside each plugin, exact names of private systems, mesh choices for shapes, and how tests share
setup.

## References

- [Bevy relationships](https://docs.rs/bevy/0.20.1/bevy/ecs/relationship/index.html) (`linked_spawn` for
  the guard ring)
- [Bevy `#[require]`](https://docs.rs/bevy/0.20.1/bevy/ecs/component/trait.Component.html#required-components)
- [Unity DOTS: authoring and baking](https://docs.unity3d.com/Packages/com.unity.entities@1.0/manual/baking.html),
  the same split between authored data and runtime components
