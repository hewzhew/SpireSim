# Original-Game Save And Reload Boundaries

Status: targeted source and recovery reference. This page records original-game
facts; it does not select a current parity workflow or make an old checkpoint
executable.

This document is the maintained source of truth for the original PC game's
autosave protocol used by the A0 Node-Reload task. It answers what the game
persists and reconstructs. Simulator support and its deliberately narrower
claim are recorded under Implementation Status.

## Source Audit

Audited source root: `D:\rust\cardcrawl`, a CFR 0.152 decompilation whose
`core/CardCrawlGame.java` identifies the build as PC `V2.3.4 / 2022-12-18`.
No browser, community description, video, or simulator checkpoint was used as
original-game evidence.

The save owner is small:

- `saveAndContinue/SaveFile.java` captures run state, room identity, global RNG
  counters, pools, path, metrics, and optional post-combat rewards;
- `saveAndContinue/SaveAndContinue.java` serializes that capture to the one
  character autosave;
- `helpers/SaveHelper.java::saveIfAppropriate` constructs and writes it;
- `core/CardCrawlGame.java::loadPlayerSave` plus the saved-dungeon constructors
  reconstruct the run; and
- `dungeons/AbstractDungeon.java::nextRoomTransition` re-enters the saved room.

The compact source index below is sufficient to recheck the boundary without
rediscovering the call graph:

| Claim | Decompiled source anchor |
| --- | --- |
| audited build identity | `core/CardCrawlGame.java:112-113` |
| declared save types | `saveAndContinue/SaveFile.java:347-352` |
| complete generic autosave writer | `helpers/SaveHelper.java:228-233` |
| room-entry writes | `dungeons/Exordium.java:62-64`, `dungeons/AbstractDungeon.java:1631,1725` |
| post-Neow writes | `neow/NeowEvent.java:145`, `neow/NeowReward.java:165` |
| post-combat capture and write | `rooms/AbstractRoom.java:332-339` |
| primary-to-backup replacement | `helpers/File.java:40-49` |
| corrupt-primary fallback only | `saveAndContinue/SaveAndContinue.java:92-107` |
| normal exit without a save call | `screens/options/ConfirmPopup.java:214-225` |
| formal death deletes the autosave | `screens/DeathScreen.java:170-171,296-297` |
| deletion removes primary and backup | `saveAndContinue/SaveAndContinue.java:272-276` |
| global RNG counter restoration | `dungeons/AbstractDungeon.java:394-410` |
| post-combat load path | `core/CardCrawlGame.java:771-779` |
| floor-local RNG reconstruction | `dungeons/AbstractDungeon.java:1727-1731` |

An all-source search for `new SaveFile`, `SaveAndContinue.save`, and
`SaveHelper.saveIfAppropriate` finds only the writers listed above. A separate
search for every `SaveType` occurrence finds no caller for `AFTER_BOSS_RELIC`.

`SaveType` declares `ENTER_ROOM`, `POST_NEOW`, `POST_COMBAT`,
`AFTER_BOSS_RELIC`, and `ENDLESS_NEOW`. The complete call-site search finds no
use of `AFTER_BOSS_RELIC`; declaring an enum variant does not create a boundary.
The actual writes are:

| Save generation | Actual writer | Meaning |
| --- | --- | --- |
| `ENTER_ROOM` | `AbstractDungeon.initializeFirstRoom` and `nextRoomTransition` | selected room entrance before `onPlayerEntry` |
| `POST_NEOW` | `NeowEvent.update` and `NeowReward.update` | Neow effect and any required target selection have completed |
| `POST_COMBAT` | `AbstractRoom.update` | battle victory has produced its reward screen |
| `ENDLESS_NEOW` | `Exordium` constructor | endless-mode Neow entrance |

Normal “Save and Quit” does not capture the current UI or transient run state.
`ConfirmPopup` returns to the menu through `CardCrawlGame.startOver` without a
save call, so Continue reads the last autosave generation. Conversely,
`DeathScreen` deletes both autosave and backup. A formal game defeat is
therefore terminal; legal retry must happen while the prior autosave still
exists, not after observing the death screen.

The `.backUp` file is a write-safety and corruption fallback. `helpers/File`
copies the previous autosave before replacing it, while loading consults the
backup only after the primary file fails to parse. It is not an ordinary
player-selectable earlier generation and grants no extra reload scope.

### Neow eligibility has two state owners

Neow has an additional boundary that the generic autosave description does
not capture. In a standard run, `NeowEvent(boolean)` reads the character's
`<CLASS>_SPIRITS` preference and copies it into both the event instance and
`AbstractDungeon.bossCount` (`neow/NeowEvent.java:60-72`). Advancing the dialog
uses the event instance to choose the four-option `blessing` or two-option
`miniBlessing` (`NeowEvent.java:155-175`). Opening either panel then resets the
global `AbstractDungeon.bossCount` to zero (`NeowEvent.java:341-369`).

The initial `ENTER_ROOM` write occurs after `NeowRoom(false)` has constructed
that event (`dungeons/Exordium.java:56-65`), and `SaveFile` captures the then
current global value as `spirit_count` (`saveAndContinue/SaveFile.java:252-260`).
On reload, `AbstractDungeon.loadSave` first restores `spirit_count`, but
`populatePathTaken` later constructs another `NeowRoom`, whose `NeowEvent`
constructor rereads the profile preference (`dungeons/AbstractDungeon.java:
456-468,2655-2672`). The profile preference and the autosave field are therefore
distinct owners; neither may be replaced by a static “standard runs get four
choices” flag.

The source also narrows what grants rich eligibility. A standard-run victory
sets `<CLASS>_SPIRITS` to one (`screens/VictoryScreen.java:83-90`). Death and
main-menu abandonment use a floor-16 threshold: at or above floor 16 they set
one, below it they set zero (`screens/DeathScreen.java:103-114`,
`screens/options/ConfirmPopup.java:193-206`). This is not source-level evidence
for a Heart-victory-only rule.

Community-observed behavior reports one information-preserving Neow replay in
some prior-victory cases followed by degradation to the two-option offer. The
static call graph above proves the two owners and their write sites, but does
not by itself prove that exact replay count or every exit timing. The simulator
therefore does not claim or depend on that edge. Dynamic reproduction is
explicitly deferred until a future whole RNG/save/load parity campaign; it is
not a prerequisite for the current Neow decision slice.

## Native Scope Model

The durable unit is one autosave generation. A decision-site name is only the
current UI/action semantics; it does not identify the reload anchor. One scope
may contain several sites.

| Scope entrance | Decisions inside the same scope | Boundary replacing or deleting it |
| --- | --- | --- |
| initial `ENTER_ROOM` at Neow | Neow offer and any card/relic targeting | `POST_NEOW` |
| ordinary room `ENTER_ROOM` | event screens, shop transactions, campfire activity and target selection, treasure opening/rewards, then map choice | next room's `ENTER_ROOM` |
| combat room `ENTER_ROOM` | complete combat attempt | victory writes `POST_COMBAT`; formal defeat deletes the save |
| `POST_COMBAT` | gold/relic/potion/card claims or skips, then map choice | next room's `ENTER_ROOM` |
| boss treasure-room `ENTER_ROOM` | reveal/choose/skip boss relic, act transition, and the next act's initial map choice | first selected room's `ENTER_ROOM` |
| combat-event `POST_COMBAT` | restored event-combat reward and departure | next room's `ENTER_ROOM` |

Consequences for represented decision families:

- **Combat:** quitting before formal defeat reconstructs the room entrance.
  Winning replaces that anchor before reward choice, so a post-combat reward
  observation cannot be carried back into another combat attempt.
- **Reward and card reward:** ordinary combat rewards have their own
  `POST_COMBAT` anchor. Claims are not individually saved and are undone by a
  reload until entering the next room replaces the anchor. Card rewards are
  regenerated from the deliberately saved pre-roll card RNG position.
- **Event:** no ordinary event option writes a save. Multi-screen choices,
  effects, and target selections remain under the room-entry anchor. An event
  battle that wins creates the separate post-combat generation.
- **Shop:** inventory is regenerated at room entry from saved pools and RNG
  counters. Purchases, purge, and Courier replacements are provisional until
  the next room entrance is saved; a reload returns the whole shop scope, not
  only the most recent purchase.
- **Campfire:** activity and any subsequent card target are under the room
  entrance generation. No campfire-specific save exists.
- **Treasure:** chest type and contents are regenerated from the saved
  treasure/relic state. Opening, claiming, taking the sapphire key, or leaving
  does not write a new save by itself.
- **Boss relic / act transition:** entering `TreasureRoomBoss` creates an
  `ENTER_ROOM` save. There is no `AFTER_BOSS_RELIC` call. Choosing a boss relic
  and constructing the next act do not persist it until the first room of that
  act is selected and its entrance save is written.
- **Map:** choosing a destination mutates transient path state, then room
  transition writes `ENTER_ROOM` before the selected room's `onPlayerEntry`.
  That new generation commits the route; reload never rewinds to an older
  autosave generation.
- **Generic run/card choices:** they inherit the containing native scope.
  `CardReward`, for example, may belong to a post-combat, treasure, event, or
  Neow scope and cannot have one static reload rule of its own.

## Restore And RNG Semantics

`SaveFile` stores the run seed and counters for the global monster, event,
merchant, card, treasure, relic, and potion streams. `AbstractDungeon.loadSeeds`
reconstructs those streams from `(seed, counter)`. It also restores room/event
pools, relic pools, path, event chances, purge cost, and the destination room.

Floor-local `monsterHpRng`, `aiRng`, `shuffleRng`, `cardRandomRng`, and
`miscRng` are recreated from `seed + floor` during room transition rather than
resumed from an arbitrary mid-room cursor. Replaying the same public prefix
therefore repeats its world; changing action order may change later random
consumption and outcomes. Node memory must remain prefix-scoped.

Natural new-run construction advances `miscRng` once before the first Neow
decision, while native Continue reconstructs that floor-local stream at its
initial `seed + floor` cursor. Original `NeowReward` paths do not read
`miscRng`, and the first room transition reconstructs the stream again. The
reward-choice Neow root therefore excludes only this dead instantaneous cursor;
it still compares Neow's own RNG and every persisted global stream. Once a
reward has settled at screen 99, the only remaining button path calls
`openMap()` and never reads `NeowEvent.rng`; the completed Neow root excludes
that second dead cursor as well. These are scope-specific future-equivalence
proofs, not permission to ignore either stream at a root that can still consume
it.

An immediate Neow reward does not itself expose the Map. The original game
first applies the reward, writes the `POST_NEOW` generation, and leaves an
explicit completed dialogue with one Leave action. Simulator semantics v13
preserves the same two public transitions (introduced at the v12 boundary) and
also restores the original map generator's external Boss-anchor edge; selecting no-drawback HundredGold
therefore has a Neow child with gold 199, and only the next Leave can produce a
Map child.

For `POST_COMBAT`, the game records the card RNG position from before reward
cards were rolled and saves non-card reward identities. Loading reconstructs
the room as battle-complete, restores saved non-card rewards, and rolls card
offers again from that saved pre-roll position. Combat events additionally
adjust the saved event counter so the completed event room can be reconstructed.

The save write is asynchronous. Disk-crash timing, manual autosave copying,
corruption fallback, platform-specific ports, mods, Daily, and Endless are not
part of the first standard PC A0 task protocol. They must not silently widen
its permissions.

## Implementation Status

Original PC V2.3.4 boundaries are **source audited**.
`NativeAutosaveTaskWorldV1` is the first bounded simulator adapter for the
standard Ironclad A0 task. Production construction starts only from a natural
new-game Neow root. The adapter privately captures exact reconstruction
checkpoints and owns `ENTER_ROOM`, `POST_NEOW`, and `POST_COMBAT` generation
replacement, same-root replay, stale-generation rejection, and formal-death
deletion. Owning tests cover natural Neow closure, ordinary room entry and
reload, combat victory and post-combat reward reload, replacement, and death.

This is behavioral reconstruction at the public decision root and subsequent
simulator RNG state. It does not serialize the Java save format or claim that
every room, event, card, relic, potion, or RNG mechanism has been exhaustively
compared with the original. In particular, an `ENTER_ROOM` checkpoint may be
captured after simulator room initialization while remaining equivalent at the
first public decision; it is not presented as the Java writer's exact
pre-`onPlayerEntry` instruction state.

The Java parity workbench additionally supports a narrower evidence-only native
checkpoint for an already validated ordinary combat entrance. A continuous
full-prefix Java run copies the original `ENTER_ROOM` autosave bytes from its
isolated working directory. A later fresh UUID sandbox restores those bytes
through the original main-menu Resume action and must reproduce the captured
stable public, parity-internal, and RNG projections before any suffix is run.
No save field is edited, and this suffix evidence is explicitly not continuous
from Neow, a simulator comparison, strategy evidence, or training material.
For throughput measurement, the run-control profile has one narrower reset at
a natural combat decision with an existing native autosave. It calls the
original main-menu transition without deleting the save, clears the live run
and RNG epoch, and requires the next typed surface to contain only Continue.
`mirror batch` compiles two to sixty-four non-empty suffix plans against one
checkpoint before launch, then executes every member in one JVM. Each member
keeps its own execution report, typed resets bind adjacent members to one
session/connection, and autosave bytes must remain identical at every member
and reset boundary. The older two-run reuse sentinel is only a thin alias for
this one batch path. Newly compiled V4 combat plans also bind the retained Rust
successor scalars and typed combat projection. After the final action, Java must
match that endpoint's hand order and costs, energy, block, potion identities,
player and monster powers, and ordered monster identity, HP, block, liveness,
and visible intent. Per-member and aggregate reports expose
`successor_combat_projection_compared`; legacy V4 plans remain readable but do
not acquire this claim retroactively. This scoped endpoint comparison does not
include hidden pile order or the complete RNG fingerprint, so the broader
`complete_rust_semantic_fingerprint_compared` claim remains false. These are
runtime-reuse artifacts only; constructed scenarios and normal combat control
do not receive the reset grant.

The Neow content implementation accepts an explicit source-audited
`NeowEligibilityV1::{Mini,Rich}` run-start input, and `run_play new` exposes it
as `--neow-offer mini|rich` with `rich` as the explicit research default. The
generated two- or four-option surface is durable public evidence in the run
archive. The simulator still does not own the cross-run profile `SPIRITS`
lifecycle or the special replay edge described above. That narrower remainder
stays explicit in this save-boundary contract and parity evidence; the general
reference service does not relabel it as strategy or maintain a Neow-only gap
table.

`TaskSpecV1::ironclad_a0_act_three_fail_closed` still authorizes no reload.
Only the explicit `ironclad_a0_native_autosave_environment_v1` factory selects
`NativeAutosaveScopes`; arbitrary exact checkpoints remain research-only.

`ReloadScopeV1` carries its own `ReloadPermissionV1`. Permission is never
looked up from `TaskDecisionSiteV1`, because site and native scope are
independent identities.
