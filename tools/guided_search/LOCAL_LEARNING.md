# Local combat learning experiments

These Rust-only experiments connect a local policy/value network to the
independent combat core. Initial training starts fresh; a separate improvement
experiment may load explicitly declared weights from that local training. They
use no external model, legacy search scores/teachers, run controller or Java process. This is bounded learning
experiment, not a gameplay frontend or an autonomous A20H runner.

## Current scope

The broader [entity path](ENTITY_LEARNING.md) admits explicit per-case
loadouts and variable-size numerical objects through the same execution core.
Its separate collector/network do not change the schema-1/2 tasks below or
upgrade their saved models into broader policies.

[`local.rs`](../../crates/sts_guided_search/src/local.rs) prepares fresh prescribed
Jaw Worm and Cultist encounters through `build_natural_combat_start`, with the full
A20 Ironclad starter configuration. An explicit `starting_hp` parameter can set
1..75 HP before combat initialization; its default is 68. This intervention is
recorded with every root. These roots have not traversed Neow or a map.
Every direct policy input is executed by `SearchSession::follow_guidance`; the same
conversion and transition code also serves the independent tree. Victories pass
the existing exact root replay. Actual losses and unresolved budget/coverage cuts
remain separate. A core error seals the population and aborts collection.

`GuidedCombatBatch` in the existing [Python bridge](../../bindings/python_learning/README.md)
is a thin NumPy binding. A frame carries an opaque revision, active slot numbers,
context rows, card rows, action offsets and action-to-card references. Revisions
and slots are control metadata, not model features. The complete action batch is
checked before any guidance is installed; old or foreign revisions are rejected.
The population is ephemeral and has no checkpoint/recovery API.

Schema 1 is deliberately restricted to this starter task. It records numerical
combat facts and the current oracle draw order. Every card instance remains a
separate row with its pile and position; legal duplicate-card actions are not
merged. An action refers to that frame's card row; -1 is EndTurn. Rust performs
legality and binding. Other cards, encounters, powers, potions and pending choices
outside the admitted scope fail explicitly. This encoder is not a general
replacement for either the existing public semantic schema or a general combat observation.
Its normalized feature layout is owned by `encode_starter`; the producer source
is retained with experiments. Extending its meaning requires a schema change.

The [component and evaluation boundaries](../../docs/architecture/model-guided-search.md)
separate broader combat learning from these small tasks. Passing or
improving a task does not qualify its encoder for other gameplay content.

### Draw/select task

`task="draw_select"` uses schema 2 from
[`local_tactics.rs`](../../crates/sts_guided_search/src/local_tactics.rs). It adds
unupgraded Burning Pact, Headbutt, Pommel Strike and Shrug It Off to the full starter
deck through typed deck mutation before normal combat initialization. Its encounter
panel contains Jaw Worm, Cultist, TwoLouse and ThreeLouse. All four encounters from
one seed belong to the same partition. A panel contains at most 32 seeds/128 roots.
These are explicit constructed roots, not collected Act 1 runs or a general deck
distribution. No new transition, replay or selection executor is introduced.

The frame uses numerical context and ragged card, monster and action rows with
independent offsets. Each action includes its kind and source-local card/target
references; absent references are -1. A selection member points to its card row,
whose pile, eligibility and selected prefix order remain explicit. Begin/Select/
Finish are distinct legal edges; their virtual steps consume work but only the
compiled selection becomes an actual game input. Only current-hand attacks have
target-specific damage preview, carried on the corresponding play action.
Every duplicate card and enemy target remains available; rows are not combinations.

The schema admits the eight named card types, up to three admitted monsters,
player Weak, monster Strength/Ritual/Vulnerable/CurlUp, and hand exhaust or grid
move-to-draw selections. Other content or overrides fail. Each active frame checks
that its card count still matches its constructed root; no card generation is admitted.
The Python [`TacticalNetwork`](local_tactics.py) pads to 16 card, 3 monster and
32 action rows with explicit masks; overflow is an error. Context, card and monster
tokens share attention. Each policy logit uses context, both referenced entities
and the action facts. Padding cannot enter attention or receive an action logit.
Fresh schema-2 parameters are trained using the same full-episode actor/critic
collector and update. Schema-1 weights cannot load into this different network.

For explicit transfer experiments, `GuidedCombatBatch(..., task="draw_select",
deck=[("Strike", 0), ...])` accepts an ordered complete deck recipe containing
1..16 admitted cards. Each entry is a canonical card ID and an upgrade count of
0 or 1; AscendersBane accepts only 0. It replaces the freshly initialized starter
deck through typed removal/addition before combat initialization, preserving
distinct lineage allocation for duplicate copies. Each episode records the full
recipe and a distinct constructed-source category. Omitting `deck` retains the
original fifteen-card construction and its source. A recipe is not an imported
run or a claim that those cards were naturally acquired. Custom decks are rejected
by the schema-1 starter task.

Card/monster/action features, normalization and padding capacities stay unchanged
across these schema-2 recipes; only the admitted source distribution changes.
Frozen schema-2 weights can therefore be evaluated on different admitted counts,
compositions and upgrades. This does not establish useful transfer by itself.

The task remains fixed oracle: pile order and current real enemy intent are
visible, but no seed, RNG, lineage identifier or search statistic enters the model.
It provides observations for proposals, not a sufficient-state or equivalence
proof. For example, complete hidden monster move history is not encoded.

[`local_train.py`](local_train.py) owns a small attention encoder with policy and
value outputs. It starts from fresh parameters, collects complete frozen-policy
cohorts, then applies one actor/critic update using actual sampled log probabilities
and terminal combat returns. The policy gradient sums decision contributions per
episode; the learned baseline is detached. Value uses binary terminal targets.
There is no damage, HP, potion or turn-count reward. Resource facts remain in
outcome evidence. This is direct policy learning, not PPO or search distillation.

### Scope probes

The explicit offline test
[`learning_scope_probes`](../../crates/sts_guided_search/src/tests/learning_scope.rs)
constructs declared roots through shared run mutations and normal combat
initialization. It exercises a larger deck, strength scaling, exhaust defense,
generated status cards, a relic counter, potion discovery and elite status
generation. Each boundary records the existing oracle observation and the
schema-2 encoder's actual acceptance or rejection. Each executed prefix is
replayed against the shared stepper; these are transition diagnostics, not
complete-battle, learned-policy or Java coverage results.

```powershell
cargo test --locked -p sts_guided_search --lib learning_scope_probes -- --ignored --nocapture
```

To retain raw roots, choices and successors, set `STS_LEARNING_SCOPE_OUTPUT` to a
new absolute directory with an existing parent before the test. Record producer
revision, working diff and test source beside that artifact. Its scripted actions
are diagnostic inputs, not policy demonstrations or a gameplay strategy. The
probe does not extend the numerical task, change learner weights or start Java.

## Run

Build and install the current bridge into the configured Python using the existing
[`learning/dev.ps1`](../../learning/dev.ps1) `refresh-bridge` flow. PyTorch and NumPy
must be available. In a new ignored experiment directory, write a JSON configuration
using the fields of `Config` in `local_train.py`; `{}` resolves all declared defaults.
The full resolved configuration is saved before execution.
Set `task` to `draw_select` for the broader constructed task; the default is `starter`.

```powershell
.\py.ps1 -u -m tools.guided_search.local_train --config <config.json> --output <new-study-directory>
```

The default study uses multiple model initializations and disjoint training/final
seed groups, applies the declared number of updates, and compares initial and final
networks under both sampled and greedy execution. All encounters and descendants
from one seed stay in its partition. Evaluation runs after training and selects no
checkpoint; the final checkpoint is the last declared generation. A failed or
unresolved cohort stops the study and retains its evidence instead of generating
replacement samples. The wall budget is checked between bounded calls, not enforced
by interrupting a tensor operation or evidence write. Set `evaluation_partition`
to `development` explicitly for a panel used in subsequent algorithm diagnosis;
these inspected seeds must not later be presented as an untouched final panel.

Files include the frozen contract, binary/source provenance, fresh initial and all
updated weights, numerical experience with sampled action probabilities, roots,
actual inputs, final successors, outcomes and update metrics. Evidence serialization
occurs after a cohort, not on the model's per-action path. Optimizer/environment
resume is not implemented. Timing columns separately cover observations, inference,
shared-core advance including replay, and updates. `wall.json` records each
successful cohort from construction through evidence writing, and `result.json`
records the study wall time. Individual components must not be summed with these
inclusive wall measurements.

## Frozen model and search comparison

[`local_compare.py`](local_compare.py) reads a completed development study and
loads every last-generation model unchanged. It compares one greedy trajectory,
bounded fresh sampled attempts, and the independent tree with neutral guidance,
policy guidance, or policy plus value. It never updates weights or creates teacher
labels. Neutral search is run once, independently of model initialization. The
comparison requires the same native binary as the recorded training study by
default. An explicitly named `--regression-baseline` can admit a new binary only
after the fresh direct controls pass the checks described below.

```powershell
.\py.ps1 -u -m tools.guided_search.local_compare --training <completed-development-study> --output <new-comparison-directory>
```

### New evaluation seeds

`--panel <panel.json>` declares a separate evaluation panel without rewriting the
training configuration or changing model parameters. Its fields are `seed_start`,
`seed_count` (1..64 for starter, 1..32 for draw/select), and `partition`
(`development` or `final`). All encounters declared by the task are constructed
for every seed. The declared seeds must be valid u64
values and disjoint from the study's training and development seed groups.
For `draw_select`, an optional `deck` field supplies the complete ordered recipe
described above. It applies to every newly evaluated root and every sampled
restart/tree, while the original regression controls retain the training recipe.
The saved training `config` remains unchanged; the new panel and each episode's
evidence record the intervention. Recipe variants may share a new seed panel for
paired development comparisons, but all such descendants remain in that seed's
partition and are not independent samples. Comparisons between recipes also change
combat difficulty and shuffle trajectories, so report same-recipe baselines.

The saved contract retains the original training `config`; `cases`,
`evaluation_panel` and `partition` describe the actual new evaluation. When a
regression baseline is required, the original development controls are freshly
checked first and saved under each model's `regression` directory. All such checks
must pass before any new-panel root is opened. Regression cases and receipts are
separate from the new panel's outcomes, denominators and per-cohort timing.

The tool validates separation from the named study; it cannot know every seed
ever examined elsewhere. The experiment protocol must declare genuinely unopened
final groups and freeze methods, parameters and work limits before collection.
Once examined, those outcomes cannot tune a method while retaining their role as
untouched final evidence. Keep all model initializations, enemies and descendants
of one seed together when reporting uncertainty; repeated evaluations are not
independent seed groups.

The same `GuidedCombatBatch` accepts `search_services=<bound>` to prepare a
fixed-root search population. Its numerical `decision_batch()` is unchanged;
`guide(revision, weights, values)` supplies a full probability vector and an
optional value for each row. All rows are validated before any mutation. This
mode uses `SearchSession::step`, sharing legal objects, state transitions and win
replay with direct policy execution. It does not warm-start from direct results.
One pending node per active tree is batched across independent roots. All modes
share the transition, legal domains and tree/evidence owner.

### Bounded policy continuations

`--rollout-steps 128` adds policy-only and policy/value continuation controls to
the comparison. In the Rust core, `SearchConfig::max_policy_rollout_steps` defaults
to zero, retaining single-edge PUCT. A positive limit gives the root and each new
tree leaf a consecutive allowance of raw maximum-weight policy expansions. Each
new state still waits for explicit guidance; the network observes its actual
successor. On a terminal, blocked path or local allowance expiry, work returns to
PUCT on the same tree. Exhausting this local allowance is a scheduling boundary,
not an incomplete-coverage or losing label; the leaf remains in the tree.

`GuidedCombatBatch(..., search_services=..., policy_rollout_steps=128)` exposes
this option through the same numerical interface. It is valid only in search
mode. The core currently requires one pending node per tree for this schedule;
independent roots still batch together. Total service, engine, depth and deadline
bounds continue to apply to every step. `policy_rollout_services` records the
subset of materialization services allocated by the raw policy. Direct execution
does not count as tree rollout work. No separate rollout simulator or reward is
introduced, and already evaluated terminal paths are not repeatedly sampled.

This changes allocation, not the objective: the first uninterrupted path can
recover a cheap greedy win. That alone does not establish better decisions.
Report which direct failures are repaired and the cost relative to sampled
retries; rollout length and tree parameters belong in the experiment contract.

### Comparing a frozen model after a core change

`--regression-baseline <completed-comparison>` explicitly names read-only evidence
from the model's training binary. Its frozen configuration, weights and training
contract must match. Before admitting any search or opening a new evaluation
panel, the new binary freshly constructs and directly executes every original
development model/panel pair. Roots, actual input
sequences, final successors, numerical observations and selected actions must
exactly match the referenced greedy controls. Failure preserves the new evidence
and stops before search. Both native hashes and regression receipts remain in the
new study. Saved roots or actions are never imported for execution.

This gate establishes only the declared control regression, not general simulator
equivalence, historical-run eligibility or equivalence of all searched branches.
The compared binary and all sources are still recorded independently. A caller
cannot silently accept a changed binary merely because a checkpoint loads.

Search stops a root at its first complete replayed win or its declared work bound.
A losing search leaf does not settle the root as lost. No win at the bound is
Unknown. Depth and engine coverage gaps remain visible. This is fixed-root win
discovery, not root-visit distillation, action-by-action search control or choosing
the best resources among winning lines. Search `decisions` and `hp` on an Unknown
outcome describe the last inspected path, not an adopted game trajectory.

Each completed comparison retains frozen parameters, producer and binary hashes,
all submitted numerical guidance and control revisions, root and terminal trace
evidence, search services, guidance rows, engine work and exact win-replay work.
Neutral guidance rows do not invoke a neural network; its `inference` timing
column covers numerical preparation and recording only.
The guidance log follows the deterministic per-slot tree schedule; it is not a
serialized tree or a recovery API. `guidance.npz` uses NaN only as the explicit
absence of a value estimate in neutral/policy-only controls; it is never submitted
to Rust or treated as a label. Replay has a separate bounded reserve. Component
and inclusive cohort wall times remain distinct.

Sampled restarts stop at the first victory and preserve every actual attempt.
Unsuccessful attempts do not prove the root impossible. Their total possible
decision count can match a tree service bound, but actual work, guidance rows and
wall time must also be compared. Extra trials alone can improve win discovery;
that improvement must not be attributed automatically to the tree or value head.

## Replayed demonstrations and continued learning

`GuidedCombatBatch.demonstration(slot)` reads the chronological path of a completed
replayed victory from the live population. It uses the same parent edges and
numerical encoders as execution/replay. Duplicate instances, target references,
and virtual Begin/Select/Finish decisions remain distinct. The export has the
same numeric schema as `decision_batch`, plus `choices` and the kind
`replayed_win_demonstration`; it has no executable `revision`. Each choice ordinal
belongs only to its adjacent numerical row. Reading never advances or replays the
environment again. Incomplete, losing, unresolved or poisoned episodes cannot
produce a successful demonstration.

[`local_demonstrations.py`](local_demonstrations.py) binds exported arrays to the
same live population's saved evidence and exact decision-ordinal path. Files
include payload/evidence hashes, included episodes and excluded outcomes. Loading
these numerical examples grants no simulation/root recovery authority. Unknown
does not become a negative example. A successful path demonstrates feasibility,
not optimality or inferiority of unchosen actions.

The separate `imitate` update applies supervised maximum likelihood to the chosen
actions: average within each complete episode, then across successful episodes.
It uses neither search visitation nor sampled behavior log probabilities. It does
not update the value head or assign search-existence targets to the critic.
Shared encoder updates can still change value predictions; the resulting value
output has not been recalibrated. An empty successful cohort produces no update.

[`local_improve.py`](local_improve.py) compares this candidate against continued
direct actor/critic learning. It loads all last-generation weights of an explicitly
declared completed local development study, clones identical initial parameters
into both arms, and resets Adam independently at the same declared learning rate.
The source learning task, deck and HP remain unchanged. Fresh training/evaluation
seed groups must be disjoint from each other and from the source study. Callers
must also exclude any other previously inspected panels when declaring new seeds.
Its optional original-binary regression baseline uses the same provenance and
fresh exact direct controls as `local_compare`, before collecting learning data.

Use a JSON object with `train_seed_start`, `train_seed_count`, `eval_seed_start`
and `eval_seed_count`; optional `Improvement` fields declare update count,
learning rate, search services, continuation length and wall budget. The resolved
contract is saved before collection. Search uses policy guidance only, a fresh
tree per root/generation and the first fully replayed win. Direct RL retains its
real full-episode return and behavior-probability checks. Incomplete direct
cohorts or execution faults stop the study with evidence.

```powershell
.\py.ps1 -u -m tools.guided_search.local_improve --training <completed-local-development-study> --config <improvement.json> --output <new-study-directory> --regression-baseline <original-binary-comparison>
```

All training finishes before evaluating initial and last-generation arms on the
declared development seeds, under direct sampled and greedy execution. No best
checkpoint selection or evaluation-driven updates occur. Every generation saves
weights, observed outcomes, full evidence, numerical data and update metrics.
Matching roots and update opportunities does not match compute: report simulation,
replay, inference, export and update costs separately. Demonstration improvement
over an unchanged model does not isolate search from ordinary successful-policy
imitation; that causal question needs an additional control.

### Comparing ordinary successes with search rescues

The shared direct `rollout(..., rule="greedy", demonstrations=True)` can export
its replayed victories using the same live-core path exporter as search. This
does not turn its behavior-probability records into supervised examples: the
numerical demonstrations are separate, source-bound files. Normal direct training
keeps its existing collection/update behavior when export is not requested.

An offline caller may freeze a local teacher, collect complete direct trajectories,
then search fresh copies of all roots on which that teacher actually died.
`validate_rescue_sources(direct_directory, search_directory)` checks that the search
cohort contains all and only those direct losses, in their recorded order, with
the exact same roots, recipe and source facts. It returns an evidence-bound mapping
between the two cohorts' local slots; it does not execute saved roots or remap
action ordinals. A direct Unknown is not a demonstrated loss; a search Unknown
remains unresolved and produces no positive example. The caller must separately
freeze and record the generating model's weights.

`imitate_groups(model, optimizer, [(data, weight), ...], config)` combines these
separate demonstration cohorts in one policy update. Each group averages action
loss within episodes and then across episodes; the supplied positive finite
weights determine relative group mass. Equal numeric slots in different cohorts
never merge. Weighting groups by their episode counts gives every episode equal
mass; equal group weights instead reserve equal mass for each group. Empty groups
are rejected, so the caller must explicitly record and handle absent data. The
single-cohort `imitate` uses the same implementation.

This makes a controlled comparison possible: ordinary successful trajectories,
the same trajectories plus conditional search rescues, and the same two groups
with different declared weights. A rescue proves a successful alternative to one
specific failed direct trajectory. It does not prove that every differing action
was necessary or that the root was generally difficult. Keep the teacher/corpus
fixed when testing the effect of supervision mass, report collection and update
costs, and distinguish training-root retention from new-seed generalization.

### First-divergence targets and frozen policy anchors

[`local_feedback.first_divergences`](local_feedback.py) extracts one positive
alternative per replayed rescue. It first validates the complete direct-loss/search
root binding, then checks the recorded decisions and every numerical observation
and legal action domain along their common prefix, including the first differing
decision. Returned rows retain the search demonstration's source-local choice and
slot; selection assembly is not skipped. The receipt records both evidence hashes,
the direct numerical experience hash, row/step offsets and the two choices. The
failed choice is diagnostic only; only the demonstrated alternative is supervised.
States after divergence are not represented as observations of the failed policy.
No successful rescue means no first-divergence target. This is read-only extraction,
not execution of saved roots or proof that this one change alone will win.

`freeze_policy(model, data)` records detached reference log probabilities alongside
a copy of the exact numerical cohort. The model must be in evaluation mode; the
caller records source weights and cohort provenance. An optional
`imitate_groups(..., anchors=[(group_index, reference, strength)])` adds positive
finite multiples of KL(reference || current) on the indicated groups. Before an
update, each anchor must match its group's full numerical data, action mask and
detached normalized distribution. Padding contributes no probability or gradient.
KL averages within episodes then across episodes, separately from the MLE group
weights, and reuses the group's current forward pass. Anchor coefficients and
measured KL remain explicit in update metrics. No value target is introduced.

First-divergence versus whole-path supervision and anchored versus unanchored
updates can be compared as separate factors with a fixed teacher/corpus. An anchor
limits distribution drift on its recorded states; it does not guarantee retention
of complete wins or constrain newly visited states. Report actual old-win retention,
rescue completion and new-seed results, rather than treating low KL or action
agreement as success. A changed action may still lead to another valid win.

## Interpretation and checks

Training success measures learning within the declared constructed task only.
Report both sampled and greedy baselines: a nearly perfect initial greedy policy
leaves little room to demonstrate stronger planning even if the sampled policy
improves greatly. Repeated roots and different model initializations are not new
independent seed groups. These experiments neither demonstrate cross-deck strength
nor validate a value model inside search.

```powershell
cargo test -p sts_guided_search --lib
.\py.ps1 -m pytest tools/guided_search -q
```

The checks cover source binding, distinct duplicate actions, actual enemy turns,
win replay, real defeat versus truncation, numerical transfer, complete-cohort
learning and rejection of stale behavior probabilities. Existing bridge smoke
tests should also pass after rebuilding the wheel. Search checks additionally
cover shared initial roots, invalid whole-batch guidance, genuine losing leaves
versus unresolved search, complete numerical-guided wins, exact successful-path
export with ordered selection edges, and separate policy-only supervision. Broader content and
teacher qualification remain experiments under the [candidate design](../../docs/architecture/model-guided-search.md).
