# Assisted natural Act 1 demonstrations

This bounded Rust-only experiment supplies the [early elite checkpoint](EARLY_ELITE.md)
with fresh continuous runs. It has no learned run policy, automatic gameplay solver,
external provider, root importer or Java control. Codex-authored choices require
explicit user authorization and remain separate from autonomous evaluation.

The opt-in [automatic prefix diagnostic](README.md#自动自然前序诊断) reuses the resident
execution owner with explicit setup rules and Oracle combat search. Its records
are automatic diagnostic evidence, not assisted demonstrations or admitted labels;
it does not change this example's actor or interface.

The same resident owner can execute explicitly
[supplied continuous segments](CONTINUOUS_SEGMENTS.md), with a separate source
contract. Those interventions are not natural demonstrations or admitted
imitation data.

## Execution and records

[`NaturalEpisode`](../../crates/sts_guided_search/src/natural.rs) owns the run,
engine and active combat. All game inputs use the shared run loop, including real
combat settlement, rewards, shops, events and map RNG. The experiment stops after
defeating a natural map elite on F6–F8 and settling combat, before choosing rewards.
Death, leaving the early window, action budget and execution failure remain distinct.
Smoke Bomb is escape, not elite defeat. Burning status and actual encounter context
are retained. This interface does not yet implement the full checkpoint assessment
contract, including a separate map-unavailability classifier or a strategic encoder.

Strategic choices adapt the game's candidates and predicates; no ranking is applied.
Fixed-count deck choices are sequential. Variable-count strategic deck choices
currently return an explicit unsupported-boundary error. Combat selection reuses
the shared linear assembly without enumerating combinations. Choosing a stale
source or an absent action does not alter state. Runtime errors seal the episode.
Each actual input is bounded by 100,000 settling ticks; caller-selected action
budgets include virtual selection steps.

The [example](../../crates/sts_guided_search/examples/natural_demo.rs) accepts a
predeclared plan and a new output directory:

```powershell
cargo build -p sts_guided_search --example natural_demo
target/debug/examples/natural_demo.exe <plan.json> <new-output-directory>
```

Plan fields are `seeds` (distinct numeric seeds, at most 128), `max_actions`,
`actor: "codex_assisted"`, `information: "oracle_observation_no_search_no_sl"`,
and `purpose: "demonstration_development"`. Initial settings are Ironclad A20,
Heart available, Rich Neow. Declare seeds before seeing their offers. Every seed,
including incomplete or poor runs, remains in the denominator. This collection
is development material and cannot simultaneously be a held-out evaluation set.

Use a resident child process with direct stdin/stdout pipes. Requests are JSON
objects: `observe` takes `episode`; `choose` takes `episode`, the returned `source`,
an unchanged `options[i].action` object, and a short `reason`. Each response returns
a new `reading`; show its `view` by default. Never execute a sequence of stale menu
numbers. Send `finish`, or EOF, to close collection and run exact replay. Clients
must watch process exit and stderr as well as responses, so a replay failure does
not become an unexplained response timeout. Mechanism logs are diagnostics, not
protocol frames. `view` shows blocked reward/shop inventory and collapses discard
actions; all offered action objects remain in the structured frame.
The top-level HP, gold, potion slots and relic counters use the current combat
resources while a fight is active, and the settled run resources afterward.
A stopped episode shows its final state without an actionable reward inventory;
an unclaimed reward at that boundary is not evidence of full potion slots.

Combat options also expose `combat_choice`, using the shared `ModelChoice`
projection that already produces their labels. Its `hand`, `target`, `slot` and
selection references index the current `reading.combat`; strategic options omit
this field. Clients can match an exact card ID, upgrade or current hand instance
and target without parsing labels or translating engine entity IDs. For example:

```javascript
const option = reading.options.find(o =>
  o.combat_choice?.kind === "play" &&
  reading.combat.hand[o.combat_choice.hand].card === "Strike" &&
  o.combat_choice.target === 1);
```

This deliberately selects the first currently offered matching instance. Add a
hand index or card-property condition when duplicates matter. Submit the selected
option's unchanged `action` with the current `source`; rebuild the match after
every response. `combat_choice` is a reading aid, not another accepted input or
a persistent card/target handle. `Strike` must not substring-match `Pommel Strike`.

Requests, selected actions, materialized game inputs, pre-action readings, schema-4
numerical combat inputs and exact before/after state digests are saved. Reasons are
actor annotations, not simulator facts, policy features or optimality labels.
The producer records binary identity and configuration. Keep the source manifest,
build settings and executable alongside the collection when archiving an experiment.
Do not edit producer fields to make old artifacts pass a newer binary's checks.

Successful closing writes `verification.json`, covering trajectory and producer
file hashes. Verification reinitializes each seed and checks every action, numeric
input and full-state successor; it imports no stored state. Numeric comparisons
use the declared `float32` type, not JSON's incidental `float64` representation.
Explicit rechecking requires the same producer binary:

```powershell
<producer-binary> verify <output-directory>
```

An interrupted collection retains its append-only requests and completed trajectory.
An action may have executed before its result was persisted; inspect the unmatched
request before doing more work. There is no automatic retry, crash recovery or
transactional journal claim. Failed execution has a separate evidence file and
cannot receive a successful collection receipt. Verification does not certify
demonstration quality or compatibility with a subsequently changed encoder.

## Learning use and checks

[`natural_demonstrations.load`](natural_demonstrations.py) checks the native receipt,
file integrity, attribution, per-episode continuity and numerical action alignment.
Combat examples carry a chosen action for behavior cloning, with no invented
on-policy probability, value target or alternative-action ranking. Strategic
readings remain available for future encoder work. Split by run seed before any
learning or assessment; do not split adjacent decisions from one run across both.
Incomplete or failed-game episodes are retained, not relabeled as victories.

The encoder is the same one used by [entity learning](ENTITY_LEARNING.md), now also
callable directly from a resident decision without constructing a search tree.
There is no automatic training, weight migration or claim that imitation improves
fresh-run performance. A training experiment must separately declare its sampling,
quality review, seed splits and held-out evaluation.

### Bounded imitation experiment

[`natural_imitation.py`](natural_imitation.py) trains fresh `EntityNetwork` policies
on the verified combat choices, then assesses complete **supplied combats** with
new seed groups. It does not train strategic choices or run Neow-to-elite autonomously.

```powershell
.\scripts\py.ps1 -m tools.guided_search.natural_imitation <plan.json> <fresh-output-directory>
```

The plan fixes demonstration directories and file hashes, producing source manifests,
all evaluation recipes and their provenance, model initializations, epochs, optimizer
settings and wall/decision budgets. The reader checks the native replay receipt and
the current input-owner sources; a schema number alone is insufficient. Duplicate
demonstration seeds and demonstration/evaluation seed overlap are rejected. Different
recipes sharing an evaluation seed remain in that seed group.

New comparisons can declare `evaluation_rng: "per_case"` to give each evaluation
case its own action RNG, seeded by `evaluation_behavior_seed + case_index`.
This uses the shared `CaseSampling` in [entity learning](ENTITY_LEARNING.md)
while retaining batched inference. Case order is part of the declared schedule;
changing `evaluation_batch_size` does not reassign these streams. It does not
change the game's seed or reveal RNG to the model. Omitting this field preserves
the existing `shared_batch` contract, where one case's lifetime can affect others'
sampling. The evaluation summary records which contract was used. Even with
independent streams, divergent states need not represent corresponding random
events, and numerical inference can differ slightly with packing.

Each epoch covers every combat choice exactly once, accumulating one cross-entropy
update. Each run seed has equal total weight, and its decisions share that weight.
No success filter, behavior probability, value target, alternative ranking or textual
reason is introduced. The value head receives no training gradient and its predictions
are not qualified for use. Exact-instance action agreement on these same demonstrations
is a fitting diagnostic; it is not an assessment of optimal choices or fresh play.

Explicit [supplied-combat demonstrations](CONSTRUCTED_DEMONSTRATIONS.md) can be
included as a separately marked source. Such mixed plans use
`equal_seed_group_full_pass`, retaining recipe relatives within one seed group.
Natural prefixes and supplied combats keep their distinct provenance; adding a
source changes both coverage and the relative weight of existing seeds.

Initial and final policies sample at temperature 1 on the same declared evaluation
panel with the same behavior-seed schedule. Each case is one complete attempt; there
is no search, retry selection or SL. Wins require the existing native exact replay.
All declared cases remain in the denominator after a construction error, timeout or
interruption, with completed outcomes and remaining Unknowns saved separately. The
summary reports encounter/loadout groups, winning HP and potion inventory, and explicit
potion use. HP is read at the settled combat terminal, including resolved victory
healing such as Burning Blood, before subsequent run settlement and rewards.
Net HP cost includes healing and is not damage received. Automatic Fairy
consumption changes inventory without counting as an explicit `UsePotion` input.

Each evaluation row also records the actual initial enemy IDs from its saved native
evidence. Reports group these compositions separately from encounter-pool names:
`LargeSlime`, for example, can produce either large slime type. Composition groups
retain duplicate enemies and include unfinished combats as Unknown. Cases without
saved construction evidence remain in `unidentified_enemies` and in the full
denominator; their species are not inferred from the encounter name. Initial
composition does not count enemies later produced by splitting or summoning.

Copying a demonstrated deck into a fresh combat is a recipe intervention: it does not
carry its natural room prefix, RNG or relic counters. Any omitted inactive relic,
changed resource or slot compaction must be declared in that recipe's provenance.
Construction does not grant permission to import historical state. Viewed evaluation
panels become development evidence; subsequent tuning requires a new independent panel
for an unseen-performance claim. Model-generated wins are retained as evaluation
evidence and are not automatically admitted as new training targets.

```powershell
cargo test -p sts_guided_search --lib natural::tests
.\scripts\py.ps1 -m pytest tools/guided_search/test_natural_demonstrations.py -q
.\scripts\py.ps1 -m pytest tools/guided_search/test_natural_imitation.py -q
```
