# Supplied continuous resource segments

This Rust-only development experiment holds a short route and initial loadout
explicitly fixed, then carries actual resources through consecutive combats.
It is a constructed intervention, not a natural Neow prefix or Java parity result.
The assisted collector and the conditional learning interface below share execution
but have separate actors, inputs and outcome contracts.

## Execution contract

[`NaturalEpisode::from_segment`](../../crates/sts_guided_search/src/natural/segment.rs)
initializes one A20 Ironclad RunState. The explicit deck replaces the starter
deck; supplied relics are additions to Burning Blood, with the same on-equip
restrictions as `ConstructedCombat`. HP is supplied, max HP and gold use the
fresh run plus actual equip effects, and potions fill initial slots in order.
Relics equip before the deck/HP intervention. No card-acquisition hooks are
invented for this initial deck.

The supplied route replaces the initial map with one column of 2–8 consecutive
Act 1 monster/elite rooms and replaces the corresponding encounter queues.
It has no Neow choices, burning elites, events, shops, campfires or bosses.
These synthetic floor numbers do not establish natural early-elite availability.
Generated RNG/pools are retained. Actual floor entry still performs the normal
floor-local RNG reseeding, room hooks and encounter-queue consumption.

After initialization, the same resident run/engine/active combat owner executes
every source-bound action through the shared run loop. Combat settlement returns
HP/max HP, gold, potion slots, full relic states, all combat RNG streams, lineage
allocation and ordered permanent changes. Normal rewards are generated. The
caller must choose rewards and map entry explicitly; the environment neither
replenishes resources nor automatically buys, rests, claims or skips rewards.

The last declared combat stops after settlement and before reward claims.
`SegmentCompleted` requires defeating all declared combats. Player smoke escape
is `SegmentEscaped`; a surviving endpoint without defeating the enemies, such as
an escaped Looter, is `SegmentNotDefeated`. Death, action-budget Unknown and
execution failure remain distinct. None can start another room.

## Bounded collection

Use the existing [assisted collection transport](NATURAL_DEMONSTRATIONS.md),
including its source-bound choices and same-producer full replay:

```powershell
cargo build --locked -p sts_guided_search --example natural_demo
target/debug/examples/natural_demo.exe <plan.json> <fresh-output-directory>
```

Keep `seeds`, `max_actions` (1–8192), `actor: "codex_assisted"` and
`information: "oracle_observation_no_search_no_sl"`. Set
`purpose: "resource_consequence_development"` and add a `segment` object:

```json
{
  "hp": 60,
  "deck": [["Bash", 0], ["Strike", 0], ["Defend", 0]],
  "relics": ["HappyFlower"],
  "potions": ["FirePotion"],
  "route": [
    {"encounter": "JawWorm", "room": "MonsterRoom"},
    {"encounter": "GremlinNob", "room": "MonsterRoomElite"}
  ]
}
```

The short deck above illustrates syntax only, not a recommended loadout. Declare
the intended loadout, seeds, information, between-room choices and comparison
arms before execution. Retain every declared attempt, including poor outcomes.
Oracle combat observations and a supplied future route cannot be called NoSL
public-information capability. No network or automatic teacher is invoked.

Collections use `supplied_segment_trial_v1` and `none_resource_experiment`, so the
natural-imitation reader rejects them. Combat decisions still expose the shared
numerical entity encoding, but this assisted collector adds no future-route feature
or training target and does not change model weights.

Every actual settlement also records the complete run/engine snapshot. Replay
reconstructs from the recipe and compares choices, numerical inputs, actual game
inputs, before/after state digests and settlement JSON. Parsed settlement numbers
are checked against the same wire representation of the fresh state; independent
state digests still cover its exact serialization. Stored state is never imported.

## Conditional learning interface

[`SegmentTask`](../../crates/sts_guided_search/src/segment_learning.rs) wraps the
same resident episode with one explicitly assigned resource requirement. The
Python bridge exposes `GuidedSegmentTask(specification, max_actions)`; its specification
contains `seed`, the `recipe` above, `requirement` and `between_rooms: "gold_only"`.
For example, `requirement: {"min_hp": 40, "reserve": [{"potion": "FirePotion", "count": 1}]}`
asks for all rooms defeated, at least 40 HP and one Fire Potion at the endpoint.
This is a candidate finite-horizon task, not a learned whole-run resource price.

The requirement does not remove legal actions or require retaining the original
potion instance. Only outgoing type counts matter. `min_hp` accepts 1..999;
impossible requirements are allowed and must be identified in an experiment plan,
not used to inflate a performance denominator. Reserve entries must have distinct
types and counts 1..5, with at most five types.

`gold_only` is an enforced experimental continuation: between rooms Rust claims
gold, declines other rewards and follows the supplied corridor. Rewards still
generate normally; the actual choices are recorded. The model controls combat,
including potion use and card selections, but has no reward or route policy here.
The final endpoint remains after combat settlement and before final reward claims.

`attained()` returns:

| Endpoint | Target |
| --- | --- |
| All declared combats defeated and all requirements met | `True` |
| All defeated but a requirement missed; actual death; player/enemy escape | `False` |
| Intermediate state, action budget or execution failure | `None` (Unknown) |

Winning an intermediate room never supplies a success label. Replay verification
does not turn an unfinished trace into a known result. Runtime failure and target
assessment errors remain explicit in evidence. Construction/projection rejection
must remain a declared admission failure in the caller's experiment record.
Evidence also exposes typed current `resources` (HP/max HP, gold, potion slots and
relic states), shared with the owner. Consumers should use these fields for resource
reports; internal serialized run checkpoints are replay evidence, not a dictionary API.

### Numerical context

`decision()` returns one source-bound frame or `None` at a stopped episode;
`advance(revision, ordinal)` selects from that frame. The outer `segment_schema: 1`
contains an unchanged combat schema-4 frame plus `context_ids` (width 2),
`context_values` (width 24), and `context_refs` (width 3). References are local
context rows, with `-1` for absent. The schema does not silently accept old combat
weights or frames. Unused numeric columns are zero.

| Context kind | Category and numeric columns from zero |
| --- | --- |
| 1 requirement | Category 1 = GoldOnly; 0: minimum HP / 75; 1–3: completed, total and remaining rooms / 8 |
| 2 encounter | Stable encounter-pool ordinal; 0: relative offset / 8; 1: current room; 2: elite; reference to requirement |
| 3 reserve | Stable potion ordinal, rows sorted by ordinal; 0: required count; reference to requirement |
| 4 master-deck card | Stable card ordinal; 0: order / 32; 1: upgrades; 2: misc / 20; 3: cost modifier / 3; reference to requirement |
| 5 pending permanent misc change | Category 0; 0: change order / 32; 1: amount / 20; reference to exact master-deck instance |
| 6 pending permanent upgrade | Category 0; 0: change order / 32; 1: amount; reference to exact master-deck instance |

For kind 4, columns 4/5, 6/7, 8/9, 10/11 and 12/13 are presence/value pairs for
damage, block, turn cost, exhaust and retain overrides, scaled by 20, 20, 3, 1
and 1. Column 14 is free-to-play-once; 15 is energy-on-use / 3. Typed lineage
binds pending permanent changes before conversion to local references; a missing
master-deck source or an unencoded pending acquisition rejects projection.

HP, potions, current relic counters and gold remain in combat entities. The network
also sees current and future supplied encounter pools, not the as-yet ungenerated
monster variants. Seeds, RNG state, lineage identifiers, revision and task keys
are metadata, excluded from features. Combat observations remain oracle-scoped;
this is neither a complete Markov representation nor a NoSL public-information
contract. Permanent lineage is not a cross-domain identity feature.

### Sampling and one update

[`segment_learning.py`](segment_learning.py) packs these variable-size inputs and
defines a fresh `SegmentNetwork`: combat and context tokens share attention, then
produce legal-action logits and a conditional-attainment value logit.
`collect` uses per-case `CaseSampling` streams for one actual continuation per
resident task, without branching or retries. Rust reconstructs and verifies every
trace from its declaration, including misses and unfinished traces. Caller budgets
bound inference batches/time; the native action budget also counts fixed
between-room choices. Time limits are cooperative, not cancellation of an active
native call. The caller must persist evidence even if collection raises.

`update` performs one undiscounted Monte Carlo actor/critic update on a complete,
fresh cohort. Its terminal return is only the assigned Boolean requirement;
policy gradients average whole-episode sums and use the frozen sampled value as
a baseline. Value loss is decision-averaged binary cross entropy. There is no
intermediate win reward, resource exchange rate, hindsight relabeling, external
model or search teacher. Unknown in any task rejects the entire cohort, rather
than silently selecting only completed or successful cases.

Every record carries its task-specification key and source revision outside the
network input. Update checks task association, source order, choices and behavior
probabilities/values before stepping the optimizer; a later minibatch error leaves
parameters unstepped. Task keys detect accidental mixing, not authenticity. Raw
specifications, transitions, numeric inputs, native/source versions and model
weights must remain available for an experiment audit.

Before sampling, declare the loadout/route/requirement distribution, independent
game and action-sampling seeds, initialization, budget and all attempts. Preserve
per-room resources, potion use, failures and full choices. Multiple targets on the
same seed are related interventions, not independent game samples. Do not use only
a target-attainment fraction to assess gameplay quality.

This interface does not establish effective exploration, temporal credit
assignment, generalization or a calibrated value estimate. Sparse terminal returns
can still fail to teach useful play. No automatic trainer, model promotion or
strategic policy is supplied; useful learning requires a separately declared
experiment with held-out game seeds and resource/decision review.

## Interpretation and checks

Compare full entry/exit resources and decisions across the segment. Different
turn counts can shift Happy Flower or Pen Nib even when HP and potions match.
See [combat quality boundaries](COMBAT_QUALITY.md). A few realized continuations
do not establish optimality, population performance or a universal resource price.
The segment endpoint also omits everything after this limited horizon.

```powershell
cargo test --locked -p sts_guided_search --lib natural::
cargo test --locked -p sts_guided_search --example natural_demo
.\py.ps1 -m pytest tools/guided_search/test_natural_demonstrations.py -q
cargo test --locked -p sts_guided_search --lib segment_learning
.\learning\dev.ps1 refresh-bridge
.\py.ps1 -m pytest tools/guided_search/test_segment_learning.py -q
```

Mechanism sentinels deliberately use small artificial decks to check Feed,
Ritual Dagger, Lesson Learned, automatic Fairy consumption and relic counters.
Those fixtures are not admitted as training or natural-play performance samples.
The numerical tests may take one disposable gradient step on artificial fixtures
to check wiring; this is not retained training material or evidence of improvement.
