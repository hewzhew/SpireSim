# Python learning bridge

The installed package exposes batched natural game episodes. Rust owns game
state and legal transitions; Python chooses public candidate ordinals and
receives observations, rewards and terminal results.

## Install

For release downloads and a complete verification run, start with
[Getting started](../../docs/getting-started.md).

Install a built wheel with the Python interpreter that will run the trainer:

```text
python -m pip install path/to/sts_learning_bridge-<version>-<python-abi>-<platform>.whl
```

The wheel's Python ABI and operating system must match that interpreter. Pip
also installs the declared NumPy dependency. The installed Rust environment
runs without Java, Steam or an original-game installation. Those are used when
recording new original-game comparison evidence, not during training.

### Build a Linux wheel

Run from the repository root with Python 3.12 or newer, a Rust toolchain and a
C compiler/linker installed. Python must include virtual-environment support;
on Debian/Ubuntu this comes from `python3-venv` or the matching versioned
package, such as `python3.12-venv`.

```bash
python3 -m venv .oracle-lab/python-learning-build
.oracle-lab/python-learning-build/bin/python -m pip install 'maturin>=1.12,<2'
.oracle-lab/python-learning-build/bin/python -m maturin build --locked --release \
  --manifest-path bindings/python_learning/Cargo.toml \
  --interpreter .oracle-lab/python-learning-build/bin/python \
  --out target/python-wheels
```

Install the exact wheel path printed by Maturin with the target runtime's
interpreter using the install command above, then run `python -m pip check`
with that same interpreter. The compiler and Maturin are build dependencies;
training uses the installed wheel and its declared NumPy dependency. On Linux,
the wheel's `manylinux` tag also declares its minimum glibc version.

## Natural episodes and trajectory acceptance

Create an A20 Heart environment with
`LearningBatchEnv(seeds, 20, final_act=True, candidate_surface="legal")`.
`player_class` accepts `ironclad` (default), `silent`, `defect` and `watcher`;
`neow_eligibility` accepts `rich` (default) and `mini`. Slot resets retain these
run settings. Role-specific mechanics and original-game coverage are tracked
by the acceptance artifacts.

`decision_batch()` returns current candidate counts and decision tokens.
`choose(tokens, ordinals)` advances a root or symbolic selection; it leaves game
state unchanged. Once every active slot is ready, `step()` commits the chosen
actions and returns the actual transition, including `reward` and `terminated`.
Read the new decision after each commit. Selecting cards can require several
`choose` calls before one `step`.

The candidate surface is explicit:

- `legal` exposes every represented legal choice, including reward order,
  potion discards and reversible map navigation.
- `policy` is the existing default for research callers. It applies the task's
  reduced candidate admission, including reward-order restrictions.

Both surfaces retain explicit potion-use restrictions. Candidate admission does
not change engine legality. Decision tokens bind the surface; checkpoints
preserve it and reject restoration into a different surface. Batch and bank
payloads use format 4 and reject earlier formats.

To verify the installed package against recorded original-game play, run from
the repository root:

```powershell
.\scripts\py.ps1 bindings/python_learning/tests/trajectory.py --evidence <java-evidence> --output <new-artifact-directory>
```

For a manual session that includes native recovery or combat retries, select
its source explicitly:

```powershell
.\scripts\py.ps1 bindings/python_learning/tests/trajectory.py --session <manual-session-id> --output <new-artifact-directory>
```

`--repository` selects the owning checkout when needed. This uses the native
recovery replay's typed parent prefixes, verifies every adopted scope, and
executes the resulting continuous trajectory from Neow. Training trajectory
format 2 records each scope's producer, exact report and global action offset.
Use `--through N` to stop after N adopted actions of that selected session;
its parent prefixes retain their recorded limits. This fixes the acceptance
scope when later play appends more actions to the same session.
Native reload preparation remains Java execution evidence; it does not become
an extra policy decision or training reward.

To verify another platform's installed wheel against the same frozen export,
pass `--trajectory <expected.json> --trajectory-sha256 <expected_sha256>` instead
of `--evidence` or `--session`. Take the digest from the exporting acceptance
report. The runner checks it before creating an environment, then uses the same
actual `choose/step`, recovery, public observation and terminal checks. Its
report retains the export digest, producer version and original source scopes;
it labels this as package verification against a frozen replay export. The
exporting run owns the fresh raw Java/Rust comparison. No original-game install
or replay executable is needed on the consuming platform.

For example, from a Linux checkout with the wheel installed in the active
Python environment:

```bash
python bindings/python_learning/tests/trajectory.py \
  --trajectory expected.json --trajectory-sha256 SHA256_FROM_EXPORT_REPORT \
  --require-build-profile release --stagger-commits 0 20 1 \
  --reject-extra-input-once --output .oracle-lab/package-acceptance/linux
```

Replace the export path and digest with the exporting report's values, and use
a new output directory for each run. This executes three slots at different
progress points, including one starting at Neow. A complete Heart trajectory
checks every slot through the formal terminal, with an atomic bad-input
rejection followed by valid continuation.

With `--evidence` or `--session`, the command invokes
`sts_lockstep replay --learning-output` to derive fresh expectations from the
raw Java evidence and current Rust implementation, then executes every actual
Python `choose/step`. It checks symbolic prefixes, public
snapshot identities and candidate order, continuation state, rewards and
termination. Original Smith opening and Astrolabe's ordered grid clicks are
grouped with their completed game action; semantic actions, Python choices and
Python steps have separate counts. The output keeps
the expected trajectory, actual commit journal, first failure or final result,
and the installed native module's SHA-256. This path does not launch Java.

The same acceptance runner supports batches with `--stagger-commits 0 20 1`.
Each number is a source commit offset for one slot; zero starts at Neow. The
runner creates each checkpoint in its own slot by executing the adopted prefix,
then restores those slot-bound checkpoints together. Different offsets require
a source ending in an actual game terminal. Repeated zero offsets can compare
several slots through the same nonterminal prefix.

Every pending row is checked against its own source commit and selection prefix.
Ready and terminal slots leave the decision rows; transition paths, rewards and
terminal columns retain their actual slot indices. The report separates prefix
preparation from checked batch calls and records each slot's start, executed
semantic actions and final resources. These slots are recovery curricula on
the same adopted trajectory.

`--reject-extra-input-once` requires multiple slots. It supplies one surplus
external input to the last active slot, checks that rejection preserves the
whole batch's exact state and prepared choices, then commits the original valid
inputs. A source with a recorded external time sample exercises that boundary.

`step(execution_inputs_json=...)` supplies recorded external inputs as one
`{"secret_portal_playtime_bits": [...]}` record per active slot. Values are
the original float32 bit patterns. Exact consumption is required; rejection
leaves the whole batch and pending choices unchanged. Ordinary `step()` uses
the environment's own inputs. `continuation_fingerprints()` is a private
diagnostic for acceptance and must stay outside model features.

## Research executors

`DistanceCombatSearch` and `distance_route_data` provide the optional
[successor-distance experiment](../../tools/guided_search/README.md#后继局面的距离启发试验).
Numerical estimates are source-bound batches; Rust owns real transitions,
resource facts, partial coverage and complete replay. They do not execute a
model-selected gameplay action or change the existing batch controllers.
The optional `lookahead_inputs` constructor argument defaults to zero; positive
values enable short checks inside that same resident graph and caller budget.
The separate `advance_forced=False` option lets single admitted choices advance
without a source estimate. Real inputs, resource ledgers, interrupted work and
all budgets remain in the same Rust core; it does not choose between alternatives.
The opt-in `probe_immediate=False` mode checks one real input before source
estimation, keeping resulting boundaries for later opening without repeating
parent transitions. It supports forced advancement but rejects simultaneous
positive `lookahead_inputs`. Missing boundary features remain absent in data.
`ordering="total_inputs"` preserves g+h ordering; opt-in `"remaining_inputs"`
orders by remaining distance, then used real inputs, then creation order.
Ordering is fixed before work and reported separately from the frozen model.
It changes exploration only, without changing replay or resource requirements.

The appended `probe_forced_boundaries=False` option requires both immediate
probing and forced advancement. After the first input it classifies each exact
boundary, continues single admitted choices, and stops at the next multi-choice
source. This extends the strict one-input stopping rule, with every intermediate
input/state and expenditure preserved under the same limits. Prepared boundaries
are reused when opened. `probe_forced_transitions` counts these extra transition
attempts as a subset of immediate/total attempts and is omitted when zero.

`estimate_probe_boundaries=False` can be enabled with forced probe boundaries.
Completed probes then queue the prepared multi-choice successors for their own
estimates, without a parent estimate gating admission. A selected successor
starts its next probe; admission never probes recursively. Only the initial
source is checked before its first estimate: queued successors have not yet
checked their outgoing edges. Direct qualifying wins found during each selected
probe are replayed before further inference. All limits, factual bounds and
resource checks remain in Rust; prepared states are not charged twice.

The independent search crate also exposes the bounded experimental
`GuidedCombatBatch` through this wheel. Its numerical schema, source binding and
bounded task scope are described in
[the local experiment](../../tools/guided_search/LOCAL_LEARNING.md). It does not use
the public semantic schema or fixed combat executor described below. Optional
`starting_hp` declares a constructed HP intervention; `search_services` switches
this ephemeral population to fixed-root search with numerical `guide` replies.
Both modes use the same independent core and exact win replay.
In search mode, `policy_rollout_steps` optionally assigns consecutive raw-policy
expansions to new leaves before returning to PUCT; the shared total budget still
accounts for every transition.
`task="draw_select"` selects a separately versioned entity/action observation and
an explicit constructed deck; the default `starter` task retains schema 1.
Optional `deck=[(canonical_card_id, upgrade_count), ...]` supplies a full ordered
recipe within the schema-2 admitted content and capacity. It is recorded as a fresh
constructed intervention. No existing root/checkpoint is loaded by this API.

`GuidedCombatBatch.from_specifications(...)` uses the separate
[entity contract](../../tools/guided_search/ENTITY_LEARNING.md) for per-case
constructed loadouts and variable-size entity/action rows. The same core owns
execution and search; setup dictionaries do not enter high-frequency step IO.

This standalone Maturin crate is intentionally excluded from the root Cargo
workspace. It batches `LearningEnvPoolV3` control calls into NumPy arrays
without per-step JSON or one Python call per environment slot for ordinary control.

The opt-in `step_with_fixed_combat(options_json)` applies one already-chosen
strategic action per active slot and settles any combat it enters using the
selected exact-search backend (`portfolio` by default, or `whole_combat` for
`sts_guided_search::solver::CombatSolver`). `fixed_combat_options_json()` supplies the
current default options. This is an oracle experiment even when the strategic
model receives only public features. The fixed policy first excludes a mugged
exit when a rewarded exit strictly improves materialized resources without a
sacrifice. The exact outcome frontier remains available for explicit branches.
It then selects highest HP, fewest potions, shortest line, and producer order;
its receipt records the executor revision. It does not claim best run value. Wall limits can affect which
witnesses are discovered, so reproducibility requires saved plans and producer
identity as well as seeds and options.

`LearningBatchEnv(seeds, ascension_level, final_act=True)` explicitly enables
the Heart task. The same keyword is available for `without_combat_potions`;
omitting it preserves the existing Act 3 default. Exact checkpoints and slot
resets retain that goal. Strategic audits expose its typed `run_goal` alongside
the current decision site, without revealing future encounters or RNG.
For the public bootstrap, `decision_batch(legality_bootstrap=True,
reward_exit_slots=[...])` supplies caller memory of committed Reward/Proceed
choices. Those slots use the task actor's shared route ranking at a map overlay;
the hint is read-only and has no effect at other sites.

An `advanced` result contains the ordinary transition columns plus separate
`fixed_combat_evidence_json`. Only the chosen strategic ordinals are model
actions; decision-step counters include the executed combat inputs. A result
with `status=unresolved` has an unresolved slot and evidence, **no transition
or reward columns**, and leaves the whole batch and pending choices unchanged.
Exceptions also leave this macro's batch unchanged. Earlier successful searches
in an unresolved batch are private attempts, not committed trajectories. Combat
exit stops at the real strategic/terminal boundary without claiming rewards or
making subsequent choices. Combat sources are rejected. Restricted-potion
environments require a never-potion executor.

`fallback_action_limit=None` stops on a search miss. An explicit positive limit
instead plays the public legality bootstrap from the unchanged combat entry,
recording its revision, source snapshots and executed ordinal paths separately
from witnesses. A real defeat produces ordinary terminal loss rows; exhausting
this action limit still returns unresolved and rolls back. Search errors and
invalid witnesses never trigger this fallback. It is a fixed, weak rollout
policy, not a proven strong combat teacher.

Executor policy is explicit per call and is not stored in state checkpoints.
`sts_learning.fixed_combat` provides a Python controller with typed limits and
separate transition/evidence results. It is an experimental component, not a
drop-in online training environment: callers must explicitly choose fallback
and treat any remaining unresolved result as an interruption. Successful-only episodes
must not be presented as unbiased win-rate or policy-improvement estimates.

At an undecoded combat root, `fixed_combat_branches(slot_index, options_json)`
searches with the same backend and privately exact-replays every sampled
nondominated witness. It returns source identity, options and attempt evidence,
`candidate_ordinals`, and independent single-slot `LearningBatchEnv` objects
aligned with those ordinals. All source slots and pending choices remain
unchanged. Branches start at the actual exit, before claiming rewards, with
their real deck, relic counters, gold, HP and remaining potions; ordinary
strategic control and fixed combat can continue them. No branch is preferred or
adopted. This is Oracle
counterfactual research, not an unbiased distribution or a run-value estimate.
An empty witness frontier returns an empty environment list, not a defeat.
Fallback rollouts are rejected, and the source potion restrictions are preserved.

`env.start_fixed_combat_research(slot_index, options_json)` retains the same
whole-combat solver for incremental resource studies. Its process-local
`FixedCombatResearch.advance_to(generation_work)` accepts cumulative work targets
within the creation contract. Search work and remaining search wall time carry
across calls; idle time, publication and continuation experiments do not reset
them. Each call returns the same branch/choice surface as `fixed_combat_branches`,
with cumulative work in `attempt_json` and per-call elapsed time. Repeating the
consumed-work target publishes without more search. Targets below consumed work,
zero and over-contract targets are rejected. Only `whole_combat` without fallback supports this method.
Earlier environments and immutable choices remain independent after refinement;
adoption still checks the unchanged exact source. This adds no automatic budget
allocation, stopping criterion, resource score or strategic selection policy.

The returned `choices` list contains immutable `FixedCombatBranchChoice`
objects aligned with those environments. After researching continuations,
`env.adopt_fixed_combat_branch(choice)` exact-replays only the chosen source
combat and returns its receipt and next decision token. It requires the same
exact source at an undecoded root; errors leave the batch unchanged. Other
slots' pending choices survive. Rewards and researched future steps are never
adopted. Choices are process-local witnesses, not checkpoint payloads, model
actions or assertions that the caller's selection is strategically best.
Adoption receipt `elapsed_ms` measures replay, excluding earlier research.

The maintained caller adapter and fresh-study entry are documented in
[the learning caller](../../learning/README.md#fixed-combat-strategic-studies).
Their execution-binding envelope lives outside this native checkpoint format;
it requires the caller's matching runtime contract before state decode.

The control surface exposes slot identity, decision phase, ragged candidate row
splits, candidate counts, V3 decision token id, public snapshot id, decision
step, and an optional dense legal-action mask. Run-level `choose` accepts those
same token ids plus candidate ordinals and rejects stale or cross-decision
inference before decoding any row. Calling
`decision_batch(semantic=True)` additionally returns the current semantic schema
as five sparse, columnar NumPy table families:

- `token`: token kinds plus per-decision row splits;
- `categorical`: token index, typed field id, and categorical value;
- `scalar`: token index, typed field id, and `float32` value;
- `relation`: source token, typed relation, and target token;
- `candidate_token_indices`: direct alignment with the flattened candidate
  rows already described by `candidate_row_splits`.

The `policy` strategic candidates omit potion discards with an empty slot,
with Sozu, or at a map decision, where discarding cannot make room for an
immediate acquisition. Full-slot replacement remains available at reward and
shop boundaries. The simulator's legal actions and human run-play controls are
unchanged.

`complete_turn_demonstration_semantic_batch(payload)` accepts one validated
winning complete-turn demonstration JSON and returns the same semantic tables,
plus candidate row splits, one demonstrated ordinal per decision, and aligned
candidate evidence. Complete-turn
rows encode the shared public root, ordered public actions, observed public
successor, potion spend count, boundary, and typed completed milestones. They
do not encode combat turn count, exact hashes, source paths, opaque guide ranks,
or recall-support claims. The demonstrated ordinal identifies one
replay-verified positive; unchosen candidates remain unknown and are exported
as such rather than as negative labels. Ordinary softmax cross-entropy would
silently convert those unknowns into negatives, so it is only a same-row
carrier smoke unless a separate evidence owner has resolved the alternatives.
This is an exact-branch research input contract, not a no-reload observation.

`replay_verified_winning_complete_turn_line(case_bytes, actions_bytes)` is the
filesystem-free corpus boundary for complete-run captures. Rust decodes and
exact-replays every `ClientInput`, rejects an unfinished/loss/trailing line,
and groups inputs only at real next-player-turn or terminal-win boundaries.
Each returned turn includes opaque serialized full-turn input JSON and every
strict prefix JSON needed by the existing exact-successor alignment methods;
Python never parses actions or reconstructs turn boundaries.

`CompleteTurnExactEnv.from_case_bytes(payload, allow_potions=...)` owns one
in-memory exact combat position over that same surface implementation.
`decision_batch()` returns a one-row batch in the ordinary scorer shape;
`choose(ordinal)` consumes the observed surface and commits its exact successor.
Terminal kind, HP, and turn count are control evidence, not model features.
The environment performs no filesystem IO and exposes no checkpoint, RNG, or
exact hash to Python. It exists for fixed-case exact-branch learning loops, not
as a deployable public-information combat environment.

`decision_batch(legality_bootstrap=True)` obtains the deterministic public
legality bootstrap in the same atomic frame. It is coverage behavior, not a
capability baseline, incumbent, combat continuation, or teacher. The aligned
`legality_bootstrap_available` and `legality_bootstrap_ordinals` columns cover
represented strategic roots, combat roots, and bridge-local symbolic
selection rows. The bootstrap consumes the same sanitized observation and
candidate semantics as the model; it cannot inspect exact RNG state, hidden
draw order, runtime UUIDs, or exact successor states. It is an initialization
aid only after its behavior has been evaluated across seeds, including cycles
at reversible boundaries. Availability means a legal choice exists; it does
not establish useful progress or teacher quality.
`public_legality_bootstrap_manifest()` is the one-call inventory
of its revision, experimental maintenance policy, covered decision domains,
fallbacks, and known limitations; callers should not reconstruct that map by
searching Rust owners.

`decision_batch(privileged_node_sl_oracle=True)` is a retained, separately
privileged exact strategic-owner projection. It returns aligned availability plus complete
ordinal paths selected by the exact strategic-owner crate. Multi-card run
choices are represented as `family -> append... -> submit`; no UUID enters the
Python payload, and every ordinal is checked again against the current
candidate row before execution. This surface is neither the public bootstrap
nor a deployable Node-Reload/NoSL actor. The function name is compatibility
debt to remove when the real task environment replaces the current prototype.

`semantic_schema()` returns the numeric enum dictionaries and categorical
vocabulary sizes from the same Rust definitions that produce the arrays. A
trainer therefore does not need a duplicate Python feature dictionary or a
source-code lookup to interpret field ids.

Public map rooms distinguish question-mark nodes from resolved events. Shop
offer tokens include prices, affordability and purchase status even when an
offer cannot currently be bought. Healing intents and potion exhaust choices
retain their public amount or pending-effect evidence. Reversible card-reward
reveals stay alongside navigation in the policy surface so that skipping a
card reward does not force it open again.

Schema v12 makes strategic occurrence identity explicitly public: deck cards
and card-target actions use local deck ordinals, potion targets use public
slots, and no runtime card/potion UUID is available to the encoder. It retains
the mechanical factorization into shared mechanics plus an identity residual.
Card tokens carry definition type, rarity, upgrade-sensitive
effective target/exhaust/ethereal, base and upgrade numerics, multi-damage,
innate, tags, explicit mechanic-role coverage, and reviewed positive roles.
Missing roles remain unknown for definition-only and partial profiles. Potion
tokens carry definition rarity, class, potency, thrown status, and composable
effect roles such as direct damage, block, draw, or colorless discovery. The
schema declares the CardId and PotionId categorical fields whose embeddings
must start at zero; an identity absent from training therefore falls back to
its mechanical facts instead of injecting a random vector. It retains the v7
card role and upgraded-behavior projections, and additionally removes physical
hand indices from card and play-action features. Exact-state-equivalent starter
Strike/Defend actions with the same target now share one canonical model
candidate while the simulator retains every legal input. Older checkpoints are
therefore rejected instead of consuming the changed categorical action surface.

Semantic schema v20 gives each map-route candidate an exact
ordinary-connection suffix summary: minimum and maximum elite, combat
(Monster plus Elite), question-mark, rest and shop counts across visible
continuations, plus earliest reachable distance to elite, rest and shop.
Boss rooms do not count as combat rooms. The earliest shop before any elite
is a joint path fact: it exists only when one continuation actually reaches
that shop without first visiting an elite. The other extrema can come from
different continuations and do not promise one combined path or estimate
combat strength. Question-mark outcomes remain unknown and future Wing Boots
flights are not assumed. The candidate-local summary gives a one-layer scorer
access to route differences beyond the immediately selected node. v19 model
weights are incompatible with this schema.

Semantic schema v21 adds partial public power-mechanism profiles from the core
power owner: trigger, card-type condition, effect and target, stack meaning and
lifetime. Hand cards link to present on-play powers through `CardTriggersPower`;
this is a conditional hook relationship, not an action recommendation or a
guarantee that a card survives earlier hooks. `PowerAppliesPower` points to an
effect-description token (for example Anger → Strength), never a fabricated
active power instance. Real instances retain `EntityHasPower` ownership and
amount. Profiles currently cover Strength, Dexterity, Weak, Vulnerable, Frail,
Anger, Rage and SharpHide; missing coverage is explicit, and profiles do not
replace relic interactions or execution order. PowerId joins zero-initialized
identity residuals. v20 weights require an explicit experimental transfer or
retraining; the model loader does not silently reinterpret them.

Combat-root artifacts contain game state, not the collector's learning action
restrictions. Pass `allow_combat_potions=False` to
`LearningBatchEnv.from_combat_root_artifact_bytes` for a potion-restricted
experiment. The default is the full action surface. The selected restriction
also applies after in-process slot checkpoint restores during Node-SL search.

Strategic rows encode run facts, context/history (including the public summary
of the most recently completed combat), cards, relics, potion slots, and
structured public event-option consequences such as HP/gold deltas, resource
effects, random outcomes, repeatability, and combat entry,
the public map graph, every `PlannerAction` variant, and typed candidate target
edges. Combat rows encode the complete public learning observation: encounter
and turn state, counters and ordered histories, player, monsters and intents,
cards and damage projections, powers and payload cards, relics, potions, orbs,
stances, and public encounter counters. Atomic candidates link directly to
their hand card, potion slot, and monster target. Indexed choices also carry
their reason and destination, not just the offered card or stance.

Semantic schema 22 adds `CandidateRemovesCard` for publicly named event removals,
linking each option to the exact current deck occurrence with its existing card
attributes and mechanics. Resolution requires unique lineage evidence and
matching public attributes; missing, stale or ambiguous targets fail encoding.
Lineage values never become model features. This covers named removals such as
Falling, not the complete structured event-effect vocabulary.

Semantic schema 23 exposes the actual clipped Rest HP gain through the same
engine helper as execution. It is the deterministic healing prefix; on-rest
relic rewards remain separate. Each Smith candidate retains its current-card
target and adds `CandidateUpgradesTo` to an `UpgradedCard` preview produced by
the real upgrade operation. Current and upgraded owned-card tokens expose
persistent cost and damage/block/magic parameters, qualified unconditional
base outputs, upgrade-sensitive exhaust/selection roles, and combat upgrade
scope. Instance growth and overrides are retained. Definition parameters remain
separately named; previews do not predict complete combat damage or strategy.
Existing numerical datasets and weights keep their original schemas.

Semantic schema 25 adds `semantic.token.scope` (`uint32`, aligned with `kind`).
Zero denotes the shared current observation. Positive row-local IDs each belong
to one candidate and all its payload, including upgrade previews and complete
turn successor observations. Candidate IDs are unique in a row and are not
model features. Row selection and concatenation retain these local scopes.
Schema 26 omits card-reward and boss-relic `option_index` from numerical
candidate features. Those indices still appear in the execution/audit contract;
reordering the same offers changes addresses, not card or relic semantics.
Mechanically meaningful order such as visible combat pile positions is retained.
Owned cards, current inventory and pending reward descriptors remain shared;
candidate payloads cannot update shared tokens or another candidate's payload.

Schema 27 adds per-card upgrade eligibility and owned-deck upgradeable counts,
including a separate Basic-rarity count. Repeat-upgradeable cards use the core
eligibility rule; Status and Curse cards are not treated as unupgraded targets.
Reviewed direct self-HP-loss, on-play draw and gross energy requests retain
their upgrade-adjusted definition amounts. Energy includes its delivery timing
or condition, and HP loss includes its block/Intangible/Rupture attributes.
These are card-owned requests, not resolved combat results: death, prevention,
draw limits, hand capacity and evaluated combat modifiers still apply. Absent
or variable/conditional draw profiles stay absent rather than becoming zero.

The semantic graph retains exact owned-deck size, card-type counts and persistent
energy-cost buckets (0, 1, 2, 3+, X, unplayable). Counts include each owned
occurrence once, retaining upgrades and permanent cost modifiers; candidates,
offers and upgrade previews are excluded. They describe composition, not a
damage bound, draw probability, playable hand or sufficiency judgment.
Schema 29 additionally counts owned source cards using reviewed core rules:
on-play, intent-conditional, temporary and per-turn Strength are separate,
as are powers that install draw or Block per later Exhaust event. Candidate
cards and upgrade previews do not increase these inventory counts. Each
Dark Embrace or Feel No Pain deployment also exposes its per-event request,
distinct from immediate draw or Block and from active power amounts. These
counts cover the reviewed card rules, not every possible relic or unreviewed
mechanism. Older numerical datasets and models retain their original schema.
Reviewed damage deliveries now retain amount rule, fixed/variable hit rule,
target scope and play condition. Fixed counts remain requests, including their
conditions; random-target hits do not promise focused damage. Per-hit signed
Strength multipliers use the production Strength hook's shared query.
Reviewed Strength gains carry delivery timing/condition and an upgraded
magnitude parameter; Demon Form is delayed, Flex schedules a separate loss,
and Spot Weakness checks the target at resolution. Strength conversion is
separate from a Strength source. Missing profiles remain unknown.

Schema 30 adds four partial relic rule fields through the same encoder for
owned relics, shop offers and reward/action payloads. Ginger and Turnip name the
player debuff application prevented before Artifact; existing debuffs remain.
White Beast Statue describes the 100-percent potion-drop chance override and
the subsequent gate at four existing reward entries. Potion collection still
uses slot/replacement and Sozu rules. Burning Blood and Black Blood expose the
healing request on surviving combat victory, subject to healing restrictions
and the HP cap. These fields describe rules rather than current Powers, held
potions or delivered HP. Other relic effects remain unreviewed in this partial
projection; missing fields are not zero-effect claims. Existing token scopes
retain ownership and candidate boundaries. Earlier weights and datasets keep
their original semantic schema and are not silently migrated.

Schema 31 adds seven compact map preparation fields. The shared public-map
owner stops each ordinary connected prefix at its first Elite, published
Boss/Victory or unresolved endpoint. It retains earliest Elite distance
separately for neither Rest/Shop, Rest only, Shop only and both on the same
prefix; mutually exclusive branches cannot create the last class. Known
Elite-free ends and unresolved leaves remain separate. At most four states
per node are explored, without enumerating all complete routes. The current
room's next targets supply shared `Map` facts with distance 1 for an adjacent
node; route candidates include their selected node with distance 0. The current
room's Rest/Shop is excluded from shared future preparation. Unknown cursors
omit the projection, and zero known starts differs from missing information.
Question-mark outcomes, future flights, fight damage, useful shop purchases and
upgrade decisions are outside these facts. Saved schema-30 weights and samples
keep their original contracts. Run-play renders examples from this same owner.

`PendingReward` tokens and `ContextHasPendingReward` retain the public reward
collection at both overview and open-card-choice boundaries, with current
collection-local indices. `RewardHasOfferedCard` distinguishes its card offers
from owned deck cards. Planner observation schema 7 preserves that collection
when a card reward is open. These are the existing public descriptors, not
settled acquisition effects or newly inspected random outcomes; reward prices
and amounts alone do not predict every relic interaction. This supplies facts
for decisions across rewards without implementing inspection memory or a
multi-step policy. v23 numerical datasets and weights are not silently consumed
as v24; saved model/schema compatibility remains enforced. `PendingActEntry`
also retains the core's publicly completed-Boss preparation stage; current
Act and upcoming Act remain separate facts.

`LearningBatchEnv.constructed_campfires(recipes_json)` accepts 1..128 strict
recipes for explicitly constructed Ironclad first-act boss campfires. Each
recipe names seed, ascension, current/max HP, first-act boss, cards and relics;
optional `gold` retains a declared public balance (0..1,000,000, default 0).
Optional `potions` names slot-ordered potion IDs or `null` for empty slots;
omitting it keeps empty inventory. Capacity comes from the core's ascension
setup and Potion Belt hook; excess entries are rejected. These are declared
inventory conditions, not attributed potion drops or pickup histories.
cards allow upgrades and persistent instance parameters. The native owner
creates a single campfire before the named boss and regenerates the complete
legal candidate surface and all facts. This is a mechanism/condition diagnostic
entry, not a natural-run source. Research artifacts must retain recipes,
producer version and the declared constructed role. Rest or Smith still uses
the normal engine when chosen; this does not certify a full-run policy.

`LearningBatchEnv.constructed_card_rewards(recipes_json)` supplies the same
bounded diagnostic entry for Ironclad card choices at first-act floors 1..14.
Each strict recipe has `loadout` (the campfire loadout fields), `floor`, and
1..8 `offers` with card id and upgrade count. Offers come from the core's red
or colorless reward pools; upgrades are validated by the real operation.
The default synthetic corridor contains ordinary fights followed by the F15
campfire and named Boss. Optional `corridor_rooms` explicitly names all 15
map-y0..14 rooms, ends in `RestRoom` and excludes Boss/Victory room types.
It permits controlled first-Elite preparation contrasts without changing cards
or generating natural history; omission preserves the original corridor.
The native owner retains every offer, Skip and applicable relic options;
choosing still uses normal reward settlement. Samples must declare this
constructed source and retain the recipes and producer version.

Symbolic-selection rows retain the unchanged matching combat or strategic
observation, the complete selection family/domain, the ordered chosen prefix,
and the current append-or-submit candidates. Run-level multi-card domains link
back to their eligible master-deck card tokens without enumerating complete
combinations. Public entity collections use their declared ordered/unordered
evidence. Candidate order remains fixed by its observation-local action
ordinal. Internal entity ids and card UUIDs are used only to resolve
relationships and never cross the bridge as feature values. An unexpected
action outside the maintained surface fails encoding instead of producing a
partial row.

`checkpoint_slot(slot_index)` returns an opaque, in-process exact checkpoint;
`restore_slot(slot_index, checkpoint)` restores both the simulator and any
unfinished symbolic-selection prefix without rerolling RNG. Checkpoints are
created only on an explicit caller request and do not form an automatic
history. A recovery curriculum therefore owns checkpoint retention, retry
counts, and replacement policy without becoming part of environment
mechanics.

Cross-process callers may explicitly snapshot the complete fixed batch with
`checkpoint_bytes(max_bytes=...)` and construct a fresh owner with
`LearningBatchEnv.from_checkpoint_bytes(payload, expected_slots=...,
max_bytes=...)`. The versioned opaque payload contains each exact run-control
session plus only the candidate-ordinal prefix needed to rebuild bridge-local
symbolic selection or a prepared action. Restore requires the exact slot count,
checks the caller byte limit before decoding, reconstructs every environment,
and replays every ordinal through the current typed decoder before exposing the
first slot. A corrupt, stale, or no-longer-legal prefix therefore fails closed.
The bridge does not inspect the payload in Python, use pickle, write files, or
publish an automatic checkpoint stream.

For vectorized curricula, `checkpoint_slots(slot_indices)` and
`restore_slots(slot_indices, checkpoints)` perform one cross-language call for
the whole selected subset. Restore validates every target, reconstructs every
environment, and observes every replacement before changing the first slot;
an invalid batch therefore cannot leave a partially restored pool.
An opaque checkpoint batch can select an ordered slot subset and return an
updated copy with replacement checkpoints for existing slots. This lets a
continuous caller retain one bounded episode-root bank across asynchronous
slot resets without inspecting sessions or falling back to per-slot bridge
calls. Selection rejects duplicate or missing slots, and update never adds a
new slot implicitly.
An explicit process-resume owner may serialize that bank with
`checkpoint_bytes(max_bytes=...)` and recover it through
`LearningCheckpointBatch.from_checkpoint_bytes(payload,
expected_slot_indices=..., max_bytes=...)`. Restore validates the exact ordered
slot identities and reconstructs every private bridge state before returning
the bank. The bank format has its own magic and version, so a current-environment
snapshot cannot be mistaken for an episode-root bank.
`reset_slots(slot_indices, seeds)` applies the same all-or-nothing rule when a
caller starts new episodes in terminal slots. Continuous callers use
`reset_slots_checkpointed(slot_indices, seeds)` to receive the exact new root
checkpoints from that same atomic bridge operation; there is no interval where
new episode seeds are active but only old roots are available.
Every recovery checkpoint is bound to the slot that created it. The ordinary
restore API rejects cross-slot cloning so recovery counts cannot be attached to
the wrong environment lineage.

Run-level `step()` returns the native V3 transition columns: source decision
token ids, ragged action ordinal paths, rewards, next-token availability/ids,
next public snapshot ids and decision steps, plus the existing typed terminal
facts. These are produced by the Rust mutation owner in the same operation;
replay and experience code do not infer transitions by joining adjacent Python
batches.

The bridge still contains no policy, optimizer, automatic reset, or PyTorch
dependency. Its semantic arrays are an input contract, not evidence that a
particular model or learning objective is correct.

At an undecoded combat root, `LearningBatchEnv.combat_group(slot_index,
replicate_count)` creates a `CombatLearningBatchEnv` without exposing or
reconstructing the run-control checkpoint in Python. Every replicate shares
the exact root id and combat-state hash, keeps its own numbered episode
lineage, and uses the same `decision_batch`, `choose`, and atomic `step` rhythm.
At any later undecoded active decision,
`group.capture_recovery_root(replicate_index)` may explicitly retain that one
replicate as an opaque in-process `CombatLearningRecoveryRoot`. The object
binds its fresh current-state root identity to the source group identity and
replicate index, and `spawn_group(replicate_count)` derives a new same-root
group without exposing the session. Partially decoded actions and terminal
replicates are rejected. The group records no automatic history; bounded
retention and replay verification remain caller-owned curriculum work.
`recovery.combat_root_artifact_bytes(max_bytes=...)` persists that exact
caller-selected boundary as one canonical opaque root. The returned bytes can
be loaded by `LearningBatchEnv.from_combat_root_artifact_bytes` or passed to
Rust search tooling; Python never maps observation-local monster indices back
to private runtime entity ids.
`group.combat_decision_audit_json(replicate_index)` is the read-only diagnostic
companion to one current model row. It returns the complete ordered typed
candidate surface, including card/potion identity and monster target semantics,
or `None` after full action decoding or terminal completion. Symbolic selection
rows retain their exact prefix and append/submit domain. This JSON never enters
the semantic model batch, chooses an action, or exposes a runtime handle; the
Python learning caller may use it only to align interpretable policy scores
with the unchanged candidate ordinals.
`group.root_context` is a frozen native `CombatLearningRootContextV1` generated
once by Rust. It exposes only compact public root facts for corpus selection and
stratification; Python does not reconstruct them from semantic feature ids, and
the context is not copied into every decision row.
`LearningBatchEnv.combat_root_contexts()` discovers every current undecoded
combat root as `(slot_index, context)` pairs in one call without constructing a
replicate group. Callers can therefore select a small corpus before paying the
session-clone cost of `combat_group`.
`LearningBatchEnv.public_run_contexts()` returns one compact read-only public
context per slot: seed, typed boundary kind, act/floor, HP/max HP, gold, and
concrete potion-slot identities. It copies no checkpoint and retains no
history. This is an opt-in measurement boundary for evaluation and auxiliary
targets; the bridge does not turn resource facts into a score or reward.
`LearningBatchEnv.without_combat_potions(seeds)` creates the same whole-run
environment while removing combat potion use and discard only from the model
candidate surface. The lane changes neither simulator legality nor non-combat
potion choices. Because the batch checkpoint schema does not yet bind that
policy, this lane rejects cross-process checkpoint serialization instead of
silently restoring as the ordinary `all` surface.
A fresh batch may instead start from exact later-run combat boundaries via
`LearningBatchEnv.from_combat_root_artifact_bytes(payload,
expected_roots=..., max_bytes=...)`. The payload is the versioned opaque batch
written by `.\scripts\ol.ps1 learning-root export`; Rust checks its byte bound,
exact root count, canonical encoding, unique root identities, recomputed root
identity/context, and active combat boundary before exposing any slot. Python
never parses a production continuation or simulator checkpoint. Private branch
cutpoint files are intentionally not accepted.
Every combat step repeats the exact root id and combat-state hash. Its newly
terminal rows have no run reward fields: they return aligned terminal kind,
bridge-owned win, player HP, root/terminal actionable living-enemy HP, turn,
potion, and card-play columns. Escaped, dying, and half-dead entities are
excluded. The source run slot is unchanged. The Python caller can
accumulate exactly one row per replicate and derive separate same-root
leave-one-out win, player-terminal-HP-ratio, enemy-HP-progress, and
potion-retention axes. Enemy progress remains diagnostic and does not change
the maintained all-loss no-signal objective. The caller does not combine these
facts into a default scalar score. This remains an execution primitive for
caller-owned grouped objectives, not a trainer or a teacher.
Smoke Bomb escape is terminal kind `Unresolved` with `terminal_won=false`.
Post-combat screen state alone never turns an escape into a learning victory.

Terminal step batches retain aligned `terminal_slot_indices`,
`terminal_reward`, public `terminal_act`, `terminal_floor`, `terminal_hp`,
`terminal_max_hp`, and `terminal_gold` columns. These raw outcome facts support
progress and lower-tail auxiliary targets without turning HP or gold into a
handcrafted reward.

Run the maintained end-to-end verification with:

```powershell
.\bindings\python_learning\verify.ps1 -Python <python-3.12-executable>
```

When validating an installed release wheel with `tests/trajectory.py`, pass
`--require-build-profile release`; the runner checks the imported native build
before replay and records its actual module path and SHA-256 in the report.

The script builds a wheel, installs it without dependency mutation into a
fresh isolated environment, binds the
target Python's existing dependency directory after that isolated wheel, and
runs `tests/smoke.py` plus one narrow real Node-SL caller smoke. The complete
`learning/tests` suite belongs to `learning/dev.ps1 test`/`verify` and is not
repeated inside this lower-level wheel verifier. It keeps the wheel,
environment, and complete logs below one fresh ignored
`.oracle-lab/python-learning-bridge/` directory while a run is active. A failed
run preserves that directory and reports the relevant log path; a successful
run removes the disposable wheel, environment, and logs after printing its
compact summary.

This crate is deliberately a standalone Cargo workspace, so it declares its
own release profiles instead of assuming the root workspace settings apply.
Routine release wheels keep exact environments and the downstream model/pool
and NumPy adapters in separate O2 units; `release-final` is the opt-in
all-O3/thin-LTO deployment profile. Dev wheels remain the fast functional
edit loop. The bridge `cdylib` intentionally has no Rust lib-test harness. Pure
typed contracts run in their workspace owners; the privileged complete-ordinal
projection is owned by `sts_oracle_learning_projection`. The built-wheel smoke
covers PyO3/NumPy arrays, semantic schema, checkpoints, combat-root artifacts,
and real typed ordinal execution; the caller smoke proves the installed
Node-SL adapter consumes that path. Explicit bridge `cargo test --lib` fails
quickly instead of recreating the retired monolithic optimized test binary.
Each wheel, Python environment, and installation check is still freshly
isolated, without accumulating successful run sandboxes.

Passing `-InstallTarget` explicitly installs that exact wheel into the target
Python with `--force-reinstall --no-deps`, but only after all isolated checks
pass. Routine learning development should use
`.\learning\dev.ps1 refresh-bridge`, which applies this guarded refresh to the
configured training runtime and follows it with `doctor`.
