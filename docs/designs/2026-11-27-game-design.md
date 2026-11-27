# Top Dungeon Farmer Defense: game design

Status: approved, 2026-11-27. This is a living document: update it in place when a design decision changes.

The approved design is tracked in [#10](https://github.com/alekece/bevy-tdfd/issues/10). The first
playable prototype is tracked in [#11](https://github.com/alekece/bevy-tdfd/issues/11).

This document describes the game: what the player sees, does and decides. How it is built (plugins, data
formats, libraries) belongs in the technical designs and ADRs.

## Pitch

A cozy, low-poly 3D tower defense with farming. A weak farmer grows magical crops on top of a tower.
Rabbits harvest them, the peddler cat sells them, and the gold recruits animal defenders. Waves of
monsters, getting stronger by the minute, march along the paths to eat the crops and break the warp crystal.

Inspired by the webtoon *The Top Dungeon Farmer* (*Solo Farming in the Tower*). It is an homage, not an
adaptation: names and designs are original, and webtoon names (Sejun, Cuengi) are not used.

## Design intent

Make a cozy tower defense where farming is an active part of the defense economy, not just decoration.
The farmer's time creates meaningful tradeoffs between growing the farm, preparing defenders and
responding to threats.

## Approaches considered

- **Classic tower defense with passive crop income:** rejected because farming would not be a meaningful
  player activity.
- **Full farming simulation:** rejected to keep the game focused on defending the tower.
- **Chosen: cozy tower defense with simple farming actions.** Planting, harvesting and petting take the
  farmer's time, while crops fund the defense. This keeps farming active without making it a full farming
  sim.

## Pillars

- **Cozy, even in a fight.** Relaxed mood, chunky expressive creatures and food. Monsters are pests, not
  horrors.
- **Farm to defend.** Every defense is paid for by the farm, and the farm is what the enemies want.
- **Placement is the skill.** Where you put plants and defenders matters more than reflexes.
- **Never make the player wait.** Timers are short and there is always something to do.

## Core loop

The farmer plants crops → the farmer and the rabbits harvest them → crops go to the inventory → sell them
to the peddler, eat them, cook them or offer them to the dragon → gold buys defenders, plants, spells and
equipment → defend the warp crystal from the waves.

## A level

### Map

- Viewed from above at an angle; the player clicks to select and place.
- Spawners connect to the **warp crystal** through one or more paths. One path is easy to defend; with
  several, enemies choose a route.
- **Walls are level terrain**, not purchases. They shape the paths. The map is mostly plant plots and
  paths.
- **Plant plots** line the paths.
- **Defenders** can be placed anywhere, but their action areas must not overlap (or only slightly).
  Placement shows whether the position is valid. Plants may sit inside a defender's area.

### Start

- The player picks easy, normal or hard. Difficulty sets the starting seeds and gold.
- Every level starts with the rabbit couple: two farmer rabbits. Nothing else is placed or owned.

### Win, loss and score

- The crystal breaks: the level is lost.
- The whole timeline survived: the level is won. The last stretch leaves time to sell or eat what is left
  before the results.
- A results screen shows a score. Levels can be replayed to beat it, and some are designed around a
  particular strategy.

### Waves

Each level has a wave script: which enemies, their color tier, their timing, and any Blue Moon nights.

- A **timeline bar** shows the whole level: where the player is now, the coming waves and the Blue Moons.
  The player always sees a Blue Moon coming.
- Idea to explore: a day and night cycle on the timeline, where some nights are Blue Moons.

## Enemies

### Ants

- Come from spawners, in growing numbers over time.
- **Color is strength**, rising over time: green → blue → red → purple.
- They always stop to attack; they never deal damage while walking past.
- **Behavior is separate from color:**
  - **Eater:** stops and eats a plant until it is destroyed.
  - **Biter:** stops, bites, then continues toward the crystal.
- After the plants, they attack the crystal.

### Locusts

- A flying family that ignores walls and paths. Same color tiers as ants.
- Every defender can hit them; their difference is movement, not immunity.
- Waves can mix ants and locusts.

### Bosses

- Bigger monsters from the dungeon's lower floors, attracted by the smell of the farm. They appear in
  specific waves of the wave script.
- Defeating a boss can unlock it as a defender (see Defenders).

## Plants and farming

Farming stays simple; this is not a farming sim.

- A crop's life: planted → grows in stages → harvestable. Harvesting sends the plant back to its first
  growth stage; it does not die. How much one harvest yields depends on the crop.
- For example, an onion blade grows two harvestable blades; harvesting one leaves the plant in place to
  regrow.
- The farmer can also remove a plant to free its plot.
- The **farmer** plants seeds and can harvest too. Both take his time. **Farmer rabbits** harvest on their
  own. Harvested crops go to the inventory, not straight to gold.
- A level opens with the farmer busy planting. The first crops ripen soon after and the rabbits start
  harvesting. From there players pick a style: plant and harvest a lot for a big economy, plant just
  enough for a steady loop with the rabbits, or switch between the two and the defense.
- Farmer rabbits move away from nearby monsters. A rabbit that dies can be recruited again, like a
  defender.
- Plant roles:
  - **Sturdy:** high health, low income.
  - **Profitable:** fragile, high income.
  - **Onion blade:** a paid upgrade of an onion. Ants that attack it hurt themselves. It keeps producing
    at a lower rate.
- Crops carry effects that follow the source and matter for cooking: tomatoes speed up mana regeneration,
  onions cure poison, rice gives health, potatoes give agility.
- Crops can also be eaten directly, without cooking. A normal crop gives a **timed** bonus: for example a
  cherry tomato speeds up mana regeneration for a few minutes. Each crop is a choice between gold now and
  a boost now.
- Until the farmer's other stats have a use, bonuses only affect mana, movement speed and action speed.
- A **blessed** crop (see Blue Moon night) gives a **permanent** bonus instead, for example faster mana
  regeneration for good. It is kept only if the level is won, every time the level is won.
- Replaying Blue Moon levels to grow stronger is intended. Later levels can be hard enough that less
  skilled players replay earlier ones first, while skilled players can go straight through.

## Defenders

Animals recruited with gold. Each lives inside its own action area: it stays there, chases the nearest
enemy, then returns home.

- The first level offers only the **Knight Bunny**. Recruit as many as you can afford.
- New defenders unlock in one of two ways, chosen per defender when designing levels:
  - **defeat it as a boss** in a wave, then recruit it from the next level on;
  - **finish a specific level**.

| Defender | Role |
|---|---|
| Knight Bunny | The first and cheapest defender. Fights with a hammer. |
| Wolf | A stronger fighter. |
| Red Bear | Grows bigger and stronger over time or with kills. |
| Poison bee | Slows groups of enemies with poison. |
| Dragon | Later: an expensive premium defender placed anywhere. |

The crystal is unguarded unless the player defends it.

## The farmer

- The player's character. No name. Weak, as in the source.
- Mouse-driven: the farmer walks where you click. Keyboard shortcuts trigger actions and spells.
- **Never attacks**, at least until farmer combat is designed.

### Actions

Always available. They cost no mana and use no spell slot, but they take the farmer's **time**, and his
time is the main resource of a level: time spent on one action is time not spent planting or harvesting.

- **Plant** seeds on a plot.
- **Harvest** a ripe crop.
- **Pet** a defender: the farmer walks to it and pets it for a moment. While petted, it briefly stops
  attacking. Petting before a Blue Moon calms it (see Blue Moon night).
- **Cook:** pick crops from the inventory, consume them, and give every defender a timed buff. A known
  recipe (for example potato and rice) is stronger and gets recorded once discovered.
- **Eat** a crop directly, without cooking.
- **Sell** crops to the peddler and **send him** on a trip.

### Spells

- Bought or unlocked, and they use **mana**. Casting also takes the farmer's time.
- **Growth Boost** is the first spell, known from the first level: crops in a small area around the cast
  grow faster for a while.
- Up to **4 spell slots**, from three families that support playing as an attacker, a pacifist or an
  economist:
  - **Attack:** later, behind conditions (for example a dagger, lightning or a fireball).
  - **Passive.**
  - **Harvest:** for example faster growth or bigger yields.

### Equipment

Later feature: raises stats such as strength.

## The lucky peddler cat

In the prototype, the peddler buys crops and sells Knight Bunnies. Trips, returned items, ranks and luck
are later features.

- The peddler buys crops and pays gold immediately.
- After a sale he leaves on a short trip; bigger sales mean longer trips. While he is away, nothing can be
  sold. A timer shows when he returns.
- He returns with items, some unidentified.
- On the map he follows the farmer and paw-boxes any monster in a small area around him: the farmer's
  little bodyguard. While the peddler is away, the farmer is unprotected.
- Click the peddler, or use a small menu, to sell and send him away.
- **Rank** (bronze → silver → gold) comes from the total gold he has paid across all levels. Higher
  ranks buy more crops and find more and better items.
- A **luck roll** on every trip means even a bronze peddler sometimes brings back something excellent.
- Later: ambushes on his trips, and paying for an escort.

## The dragon (administrator)

Later feature: an always-available menu. Offer harvested crops to:

- identify an unknown item
- learn a new spell
- reveal a hidden unlock condition (you still have to meet it)

## Blue Moon night

A wave-script event, zero or more per level, shown in advance on the timeline.

- **Defenders rampage:** they get stronger and their action area grows. The no-overlap rule stops
  protecting anything: a rampaging defender hits everything in its area, enemies, plants and other
  defenders. A rampage cannot be stopped once started.
- **Calm:** a defender petted before the Blue Moon stays calm. It does not rampage and gets no boost.
  Petting trades power for safety.
- **Preparing costs time, not mana.** Calming every defender is possible, but each one takes the farmer
  away from planting and harvesting before the Blue Moon.
- **Placement is the skill:** put the defenders you will let rampage where their grown area reaches no
  plants or friends.
- Some crops become **blessed**: better ingredients, a better price, and a permanent farmer bonus when
  eaten.
- Enemies get stronger.
- It is the game's visual showcase: moonlight, a blue tint, glowing crops.

## Progression

- **Within a level**, the shop sells defenders, plants, spells and equipment. Gold, crops, placements,
  purchases and the peddler's unfinished trips are lost when the level ends.
- **Kept between levels:** player level, unlocks, identified items, discovered recipes and conditions, and
  the peddler's rank.
- Progress widens the shop's choices. The only permanent power comes from eating blessed crops, kept on a
  win.
- Some spells and items are available from the start.
- **Hidden unlocks:** higher-tier content shows as "???" with a vague hint (for example "own N crops" or
  "recruit a Red Bear"). Meeting the condition unlocks it and reveals the condition in the catalogue.
- **End of level:** the results screen also shows what was unlocked: new recipes cooked, items found,
  spells and defenders. Discovering new content is how the player feels progress.

## Look and sound

- Cozy, colorful low-poly 3D with soft cartoon shading; angled top-down camera.
- One shared palette and set of materials keeps the look consistent.
- Visual effects: cartoon shading, an enemy tier tint, the Red Bear's growth glow, range rings during
  placement, the Blue Moon.
- Music: a light, ukulele-led farming theme that becomes more energetic during waves. The ukulele parts
  are played and recorded for the game.

## Stages

Core systems come first; optional ones are added once the core is fun. Cooking is the last optional
system because it reuses the inventory and unlocks.

### Prototype

Simple shapes, one path, the crystal, one crop, the farmer planting and harvesting, farmer rabbits, the
peddler buying crops and recruiting Knight Bunnies (no trips yet), the tomato, Growth Boost, green ants,
waves, the timeline, win and loss.

### Vertical slice

One polished level, final models and shading, sound and music, inventory and selling, a playable farmer
with two spells, and the results screen.

### Alpha

All plants, enemies and defenders including the dragon, four spells, multiple paths, Blue Moon, and
cooking.

### Beta

Player levels, unlocks, the peddler's ranks and luck, balancing, and polish.

### Deferred until the core loop is fun

The dragon's administrator services, unidentified items and identification, equipment, hidden unlock
conditions, peddler trips, and attack spells are not part of the prototype. Revisit them after the core
farming-defense loop is playable and enjoyable.

The peddler's ranks and luck are also later progression features. Hidden unlocks, equipment and the
dragon's services are not required for the first polished level.

## Open questions

Proposals are suggestions, not decisions.

- **Score.** Two options: a formula counting gold earned (not gold held), the crystal's remaining health
  and the defenders used; or a simple rating from the crystal's health alone (100%: full success,
  50 to 99%: partial success, below 50%: low success).
- **Gold while the peddler is away.** Proposal: gold stays; growth and harvest timing must avoid dead time.
- **The peddler's trip length.** Start short and fixed, about 10 seconds, and keep it configurable to
  find what feels right before making it depend on sale size.
- **Blue Moon warning.** Either a short warning of about 10 to 13 seconds, or only the timeline showing it
  coming. To settle by playtesting.
- **Action durations.** How long planting, harvesting and petting take sets how hard Blue Moon preparation
  is; to tune by playtesting.
- **Eating other crops.** Only the tomato's effect is decided; the others are open.
- **Bonus sizes and durations.** Timed bonuses last about 2 to 5 minutes; exact values and the size of
  permanent bonuses come from playtesting.
- **Mana pool.** Mana regenerates over time; the starting pool and base regeneration rate are open.
- **Wolf's role.** Only named so far; its stats and specialty are undecided.
- **Blue Moon stage.** Placed in the alpha here; confirm.
