# Entity combat learning

Schema 9 connects variable-size numerical objects and per-case constructed
loadouts to the same independent core as the [local tasks](LOCAL_LEARNING.md).
This is an experimental input and actor/critic path, with fresh parameters and
no default trained model, autonomous run controller, save importer or Java claim.

## Construction

`combat_entity_batch(positions_json, *, potion_uses=None, max_potion_uses=None)`
projects 1..128 exact Rust `CombatPosition` decisions through the same encoder.
The JSON array is decoded only at this boundary; the result is the usual NumPy
entity frame for `positions` / `pack` / `EntityNetwork`. It does not reconstruct
a run, advance RNG, search, or install any action. The caller retains the exact
state's source and producer version, and supplies already-spent explicit uses
for intermediate positions (default zero is appropriate for combat roots).
Stable pending choices are admitted; terminal positions have no decision frame.
Unsupported observation fields still fail explicitly. This detached Oracle
projection grants no Java or durable run authority and is not a training label.

`GuidedCombatBatch.from_specifications(specifications, max_decisions,
search_services=None, policy_rollout_steps=0)` creates a fresh population.
Each specification is a dictionary with:

| Field | Meaning |
| --- | --- |
| `seed`, `encounter`, `act`, `room` | Explicit construction source; canonical encounter ID, act 1..4, combat room |
| `hp` | Current HP intervention, 1..75 |
| `deck` | Complete ordered `(canonical_card_id, upgrades)` recipe; duplicates allocate separate instances |
| `relics` | Additional newly equipped relics; the fresh Ironclad retains Burning Blood |
| `potions` | Potions placed in available slots in order |
| `max_potion_uses` | Explicit use limit, or `None` for all legal uses |

Relics equip on a fresh starter run, then the constructor allocates distinct
master-deck lineages for the exact recipe, refreshes deck-dependent relic counters
and sets HP before normal combat initialization. Recipe placement does not call
card obtain/remove hooks: Eggs cannot change its upgrades, Omamori cannot block
its curses, and Ceramic Fish/Darkstone do not grant acquisition bonuses. War Paint
also cannot silently change the declared upgrades. On-equip RNG consequences remain
in the root. Pending on-equip choices are rejected. Bottled Flame, Lightning and
Tornado are rejected until construction has an explicit target contract. This is
a declared loadout intervention, not a naturally acquired run; carried counters
and arbitrary historical HP/max-HP states are not inferred from relic names.

The population has at most 128 cases and a declared 1..512 decision/service depth.
No constant card-count requirement applies. Setup dictionaries are decoded once;
`decision_batch`, `advance` and `guide` use arrays and the existing revision
binding. Full submitted batches are validated before actions are installed.
Unknowns, actual losses and replayed victories retain the same core semantics.
Evidence includes the recipe, root, actual inputs, current successor and outcome.
For authorized assisted collection, `readings_json()` exposes the same active slots,
revision, structured observation and choices as the numerical frame. It reuses the
shared projection and is read-only; references remain local to the returned source.
The [supplied-combat collector](CONSTRUCTED_DEMONSTRATIONS.md) owns attribution and
replay receipts separately from autonomous policy evaluation.
Outcomes retain terminal potion slots and explicit potion-use counts alongside HP.
Automatic Fairy consumption appears in inventory, not explicit-use counts. These
resource readings come from the settled combat terminal, including resolved victory
hooks such as Burning Blood healing, before subsequent run settlement and rewards.
Starting HP minus terminal HP is a net resource change, not damage received. Do not
subtract a fixed heal to reconstruct pre-victory HP: healing can be capped, blocked,
or affected by other relics.

## Numerical objects

The schema owner is [entities.rs](../../crates/sts_guided_search/src/entities.rs).
`entity_ids` has kind/category columns; `entity_values` has 32 numeric columns;
`entity_refs` has three local object references. `action_ids`, `actions` and
`action_refs` have widths 2, 4 and 3. Splits partition flat rows by active slot.
References are local to each position, with `-1` for absent. Slots, revisions,
seeds, RNG and lineage IDs never become model features.

| Kind | Category, values and relations |
| --- | --- |
| 1 player | Phase; HP/max HP, block, energy, turn, gold, facing, ascension, counters, discovery cost, potion-use limit |
| 2 card | Stable card ordinal; pile/order, upgrades, energy/X/unplayable cost, optional overrides, current numbers and five one-hot card types; described payloads use zone 5 |
| 3 monster | Stable enemy ordinal; position, HP, block, alive/half-dead/escaped and optional current intent |
| 4 power | Stable power ordinal; optional amount, extra, just-applied/order; owner, held/described card and source references |
| 5 relic | Stable ordinal; optional counter, used-up, amount and equipment order; bottled relics reference a described master-deck card, which references its live pile occurrence when present |
| 6 potion slot | Empty or stable potion ordinal plus one; explicit occupancy and slot |
| 7 selection family | Reason/parameters, min/max, active assembly and optional source pile |
| 8 selection member | Family/card references, member order and selected-prefix order |
| 9 offered card | Stable card ordinal/upgrades and indexed-choice context; distinct from existing cards |
| 10/11 history | Played card type and order this turn/combat; not additional card instances |
| 12 choice context | Discovery parameters or Choose One |
| 13 damage preview | Card/target references and shared target-specific damage |
| 14 monster fact | Explicit controller fact and scaled value; owner reference |
| 15 monster move | Monster-local move ID, current-plan flag or roll-history order; owner reference |
| 16/17 | Reserved for the path encoder's resource query/original potion rows |
| 18 planned basic effect | Attack, heal, block, random block, steal gold, remove debuffs, suicide, charge, escape, magic, sleep, stun, debug, none; source and target references |
| 19 planned power effect | Stable power ordinal, application/removal, amount, buff/debuff; source and target references |
| 20 planned card effect | Stable card ordinal, addition/upgrade, amount, upgrade level and destination |
| 21 planned spawn | Stable enemy ordinal, position, minion flag and typed HP source |
| 22 potion mechanic | Shared `PotionMechanicRole` and a reference to its held-potion row |
| 23 card mechanic | Shared card roles/coverage, card-owned exhaust/ethereal, optional draw request and reviewed enemy Strength request/restoration; card or offered-card reference |
| 24 bound power mechanic | Partial shared profile, linked to an active or planned power subject, affected creature and optional resulting-power description |
| 25 resulting-power description | Stable power ordinal and partial profile; never an active instance or an amount; may reference another resulting-power description |

Action kinds 1..10 are play, end turn, use potion, discard potion, begin
selection, select member, finish selection, choose card, cancel and proceed.
Action categories are zero. Content categories use engine stable ordinals,
with reserved category/kind capacities 4096/26; unknown indices fail explicitly.
Optional-value presence differs from zero. Actual held cards use references;
described card payloads have a separate zone. Each duplicate and discovered
option remains independently addressable. Field changes require schema review.

The envelope is fixed-oracle Ironclad without orb/stance mechanics. Discovery
and Choose One are admitted; other indexed reasons and unobserved power sources
are rejected. Monster controller observations cover all enemy types in the Act 1
weak, strong and elite encounter pools, plus Chosen, Guardian, Time Eater and
Heart, Awakened One, Collector and Torch Head. Other enemies are rejected until
their relevant fields are inspected. Collector exposes initial-spawn, ultimate-use
and turn counters. Two kind-14 `TrackedMinionSlot` rows link the owner to its
currently tracked monster rows; stale same-name torches are not guessed as targets.
Missing typed targets reject the projection. These appended fact categories use
the reserved vocabulary without changing prior categories or tensor widths.
The shared [monster observation](../../crates/sts_guided_search/src/observation/monster.rs)
provides current move, engine roll history, and explicit facts such as Haste use,
Guardian progress and Heart counters. Roll history includes unexecuted rolls;
it is not public executed history. Protocol identities and initialization flags
are excluded. Fact categories are the explicit `MonsterFactId` discriminants.
Act 1 facts include Lagavulin's awake/wake-triggered flags, sleep and attack-cycle
counters; Looter's slash count and stolen gold; Red Slaver's Entangle use;
Wizard's charge and large slimes' split trigger. Controllers using only existing
move history and combat entities need no extra facts. New fact categories append
within the reserved kind-14 vocabulary; existing category meanings, scales and
tensor widths stay unchanged. The controller-fact extension retained schema 4, but producer
source checks still apply to frozen weights, and newly admitted enemies have no
implied training coverage. Input admission is not successor-complete certification.

Representable content IDs do not certify all mechanics or successor closure.
This remains an oracle observation, not a complete Markov state. A chosen
training population must resolve remaining gaps explicitly, without filtering
inconvenient seeds or choices. Earlier records and weights are not accepted as
schema 9; new experiments initialize fresh parameters.

Schema 9 binds the shared `PowerMechanicProfile` to every active power and
planned application/removal. Column 0 marks availability of a **partial** profile;
absence means unreviewed mechanics, not absence of an effect. Columns 1..4 are
trigger (attack damage, card block, player card played, owner turn end); 5..9
are optional card type; 10..16 effect; 17/18 owner/player target; 19..21 amount
meaning; 22..25 lifetime; 26 positive-amount application guard. The enum order
comes from the shared power mechanics owner. The source power row still owns
amount, freshness and activation status. For example, Anger links to a Strength
description even before any Strength has been gained. A planned Anger does not
become active. Shackled describes its positive restoration and expiry at the
owner's turn end even after DarkShackles leaves the hand. Missing profiles and
unmodeled interactions still require real simulation.

These descriptions do not certify learned composition or better decisions. The
encoder still combines raw referenced rows before attention. Architecture trials
and their outcome comparisons stay in artifacts, not alternate production paths.

Schema 8 adds card type in columns 27..31 of card and offered-card rows. Each
has one linked kind-23 row: columns 0..20 are the shared mechanic roles, 21..23
are coverage, 24/25 are the card-owned exhaust flag (including instance override)
and ethereal flag. Absent roles are negative facts only for complete coverage.
Columns 26/27 are optional draw request/potency, 28/31 are optional signed
enemy Strength request, and 29/30 distinguish no scheduled restoration from
Shackled restoration at the owner's turn end. Request scales are 5 cards and
10 Strength. Offered identities have type/roles/schedules but no live request
numbers or instance overrides. All facts reuse the shared card mechanics owner;
there is no desirability ranking. NoDraw, Artifact, forced exhaust and later
cleanses can change actual outcomes; simulation still owns their resolution.

Schema 7 supplies held-potion request potency through the same owner used by
execution, base potency, mechanical usability, discard/target flags and thrown
status. Discovery facts distinguish three offered options from selected-copy
count, this-turn cost and skip permission. Shared role rows connect potion
identities without defining desirability. These facts do not predict actual
damage, healing after caps, random cards or the value of consuming a reserve.

[`discovery_value.py`](discovery_value.py) aggregates caller-provided contextual
card values for a uniform distinct offer without enumerating its combinations.
All values must share one objective and continuation contract; selected copies
are included in each value. Legal skipping is valued after potion consumption.
It assumes card values are independent of the other offered cards and their
RNG effects. This mathematical component does not evaluate cards, choose use
timing, price future potion opportunities or reproduce fixed-seed game RNG.

Schema 6 decodes bottled-card identity exclusively through the relic-state
owner. Its raw lineage payload no longer enters the numeric `amount` feature.
The target description is not a second live card or action candidate. A played
power may have no live pile occurrence while retaining its master-deck binding;
missing or ambiguous master-deck bindings are rejected rather than guessed by
card name. Tensor dimensions and the reserved vocabulary remain unchanged.

Schema 5 adds ordered steps from the engine's currently resolved monster plan.
The numeric mapping is owned by [effects.rs](../../crates/sts_guided_search/src/entities/effects.rs).
It distinguishes the exact planned power and amount, rather than only an intent
icon or move number. Rows are future plans, never already-active powers. Hooks,
Artifact, player intervention and later retargeting can change the actual outcome;
the projection is not a second transition engine or a complete consequence model.
For the current finite-search learning loop, see [POLICY_ITERATION.md](POLICY_ITERATION.md).

## Packing and learning

`EntityEncoder` owns the shared entity representation. `EntityNetwork` retains
its actor/critic contract; [entity_value.py](entity_value.py) uses the same encoder
without an action head to predict retained continuation consequences. Its HP,
maximum-HP and gold deltas, and each held potion's retention, come from **one**
replay-checked teacher plan. Budget misses have no resource targets. A separate
`teacher_found` head predicts finite-search coverage, not death probability or
optimal game value, and is not a retention score. `encode_return` checks the exact
sampled root and card-only/unlimited resource family before projection. Training
and evaluation must split complete source roots, not near-identical candidates
from the same fight. This module does not deploy a model into the solver.

[entity_learning.py](entity_learning.py) retains owned, unpadded records and
source-local references. `pack` computes padding, masks and offsets for each
actual inference/update batch, with a separate zero token for absent references.
Its attention-cell limit is a computation budget; exceeding it rejects the
operation rather than clipping cards.

`EntityNetwork` combines content embeddings, numerical facts and object relations
in an attention encoder, then scores all legal actions and predicts value.
`collect` samples real inputs and preserves ragged experience and Rust evidence
even after interruption. Callers retain source/binary/model manifests. Its wall
deadline is cooperative between batches, not a hard deployment deadline.
For experiments needing independent action randomness, pass a `CaseSampling`
instance with one declared distinct seed per construction slot in place of the
shared Torch generator. Active-slot reordering, another case finishing early,
and padding then do not consume that case's RNG. Sampling trims each row to its
actual action count and validates the complete active frame before drawing.
The caller owns the slot-to-seed manifest. This isolates sampling randomness;
floating-point inference differences across packing or devices remain possible.
Matching seeds also do not make different game histories identical random events.

`update` accumulates one complete-episode actor/critic update across freshly
packed minibatches. It checks behavior probabilities before any optimizer step,
uses actual terminal outcomes and confirms replay of wins. Unknowns cannot train;
validation failures clear accumulated gradients. It uses no HP/potion/turn
shaping. Passing one update is integration evidence, not model improvement.
Evaluation may use `require_complete=False` to retain Unknown alongside actual
outcomes; the update still refuses such a cohort.

`bounded_update` optionally limits the mean forward KL of the full legal-action
distribution on the collected states. It retries the same update at halved
learning rates, restoring parameters and optimizer moments between attempts.
Exhausting this declared budget restores the original model and returns a
rejection. It does not inspect evaluation outcomes, select successful episodes,
or guarantee unchanged behavior on unvisited states. Entropy and maximum action
probability describe the sampling policy's concentration; the KL limit alone
does not ensure useful exploration or prevent gradual drift across updates.

After refreshing the bridge with `learning/dev.ps1`, run:

```powershell
.\scripts\py.ps1 -m pytest tools/guided_search/test_entity_learning.py -q
cargo test --locked -p sts_guided_search --lib learning_scope_probes -- --ignored --nocapture
```

The bounded [entity trial](entity_trial.py) accepts an explicit plan and fresh
output directory via `py.ps1 -m tools.guided_search.entity_trial <plan> <output>`.
The plan declares concrete training cohorts, evaluation cases, provenance,
initializations, optimizer settings, budgets, question and decision rule. It
records initial/final development results by encounter and source group, actual
complete-episode updates, collection/update time, Unknown and failure. Seed
groups are disjoint; reordering or renaming a training loadout cannot qualify as
a new recipe. All specified initializations share the same training case list.
No deployment strength, mechanism understanding or search benefit follows from
this developmental before/after comparison alone.

The trial can explicitly warm-start from all declared endpoints of a completed
attributed-imitation experiment. `warm_start` pins its plan, producer, material,
summary and per-model final weights by SHA256. Input sources, the numerical/model
AST, native binary, runtime and network dimensions must match. Source training
and development seeds cannot enter the new trial. This loads weights only;
it does not import combat roots or automatically promote self-generated choices
to demonstrations. The `zero_logit` value initialization retains the policy and
starts the untrained outcome estimator at 0.5. The optimizer is fresh.

With `sampling_rng: per_case`, both training and evaluation use independent
declared slot sampling streams. Evaluation reuses the imitation evaluator's
complete outcome, actual-enemy and resource reporting. A declared
`max_policy_kl` enables bounded updates; rejection stops that model's training
and keeps its endpoint for assessment without declaring the update budget
complete. Other declared initializations remain in the report. Win/loss supplies
the Monte Carlo target for every visited state, with the previously recorded
value as an action-independent baseline. This is an estimate for the sampled
continuation policy, not an optimal-action label; HP and potion use remain
separate diagnostics. Warm-start strength does not by itself establish that
this high-variance feedback can improve an already concentrated policy.

## Mechanism diagnosis and practical evaluation

The [combat quality observations](COMBAT_QUALITY.md) preserve terminal resources,
pending permanent changes and exact construction groups. Explicit HP/inventory
requirements can expose differences hidden by victory counts; they are diagnostic
questions, not universal reward weights or an implemented continuation-value
learner. Practical improvement requires a declared resource-quality objective.

Keep three source families distinct: minimal constructed mechanism contrasts,
broader constructed combinations, and source-qualified real loadouts. They
respectively isolate a decision, exercise interactions, and test practical
relevance. Reconstructed historical decks are recipes, not resumed roots or
automatically qualified teacher trajectories.

Ordinary encounters, mechanism encounters, elites, bosses and Heart need not
form a strict difficulty ladder; deck/resource choices can reverse the order.
Hard encounters can remain visible as stress tests while training uses a
declared mixture of manageable complete battles.

Mechanism tests predeclare matched variations in a key condition, holding other
facts fixed as far as possible. Compare full continuation quality and resources,
not just a changed action name. Vary distractors, targets and held-out recipes
and seeds. A changed action is not proof of understanding; one feasible line is
not a uniquely correct label. If a reachable contrast or bounded outcome
comparison is unavailable, preserve Unknown.

Nob's skill punishment, Guardian's mode change, Time Eater's counter and Heart's
damage cap require different contrasts and continuation lengths. Pair relatives
stay in the same partition. Report mechanism diagnostics, cross-recipe capability
and complete-battle performance separately, following the
[training/search gates](../../docs/architecture/model-guided-search.md).

The [mechanism panel](../../crates/sts_guided_search/examples/mechanism_panel.rs)
constructs Nob/Entrench, Nob/Battle Trance and Time Warp contrasts with explicit
HP, block, card-zone and power interventions. Each condition includes fragile
and surviving HP, with/without a distractor, across declared seeds. Preparation
uses actual enemy turns; lethal damage uses the shared target preview and an
explicit enemy-block intervention. These are minimal constructed positions,
not natural loadouts. Their entire condition grid is retained, including cases
where both first actions can win or neither can win.

The bounded enumerator uses `CombatSearchSpace` for every legal atomic edge,
encodes every decision, and exactly replays every terminal path from its fresh
root. Virtual selection families are outside this panel. Node/depth budget cuts
are Unknown; the node cap permits one additional shared cutoff leaf. An
interrupted transition or failed replay stops production without publishing a
complete case. Record the producing revision, source diff, example source and
executable hash alongside the panel.

```powershell
cargo run --locked -p sts_guided_search --example mechanism_panel -- <fresh-panel-directory>
.\scripts\py.ps1 -m tools.guided_search.mechanism_eval <panel-directory> <entity-trial-directory> <fresh-evaluation-directory>
```

[mechanism_eval.py](mechanism_eval.py) checks the saved training sources against
the current observation/model code and loads both frozen weight endpoints for
every declared initialization. It performs no simulator transitions or updates.
Only existing numerical objects enter inference; recipe, seed, action receipts
and terminal results remain evidence. Full-node probabilities, source/input
hashes and per-case results are saved separately from the short console output.

Backward integration reports root preferences and complete stochastic-policy
win probability separately. Open graphs retain known win/loss mass and unresolved
mass as probability bounds; they do not receive an exact win rate or optimal
action label. On closed graphs, a necessary winning first action is identified
only when another first action has no winning continuation. Both-win/all-loss
controls have no such label. Winning HP is a secondary diagnostic, not the
win-only training objective. Familiar-content contrasts and untrained-content
transfer tests remain separate; observing this panel makes it development data.

The [early elite checkpoint](EARLY_ELITE.md) defines the candidate natural-run
milestone and distinguishes full-run results from supplied-combat diagnostics.
