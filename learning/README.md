# Python learning callers

This package owns local policy inference, optimization, sampling and evaluation.
Rust owns game mechanics, legal actions and numerical schemas through the
[Python bridge](../bindings/python_learning/README.md). It is an experimental
library, not a qualified autonomous A20H player.

## Environment

```powershell
.\learning\dev.ps1 doctor
.\py.ps1 -m pytest learning/tests -q
```

`py.ps1` uses the configured learning interpreter unless `-Python` or
`-Environment` is given. It preserves UTF-8 and forwards arguments and stdin.
It does not inject another application package into every Python invocation.

`learning/dev.ps1 refresh-bridge` builds and verifies a wheel before replacing
the configured native extension. Use its help and the bridge README when changing
the ABI or environment. Do not delete a shared environment based only on its age:
other environments may import its PyTorch or native extension through `.pth` files.

## Owners

| Area | Code |
| --- | --- |
| Policy and optimizer | `torch_policy.py`, `torch_training.py`, `torch_behavior.py` |
| Training sessions and publication | `torch_session.py`, `torch_generation.py`, `torch_resume.py` |
| Combat-specific callers | `torch_combat_session.py`, `torch_combat_training.py` |
| Strategic studies | `strategic_study.py`, `fixed_combat.py`, `fixed_seed_evaluation.py` |
| Run data and resource diagnostics | `public_trajectory.py`, `run_resource_trace.py` |
| Exact node-restart policy improvement | `node_sl.py`, `torch_node_sl.py` |

Use tests beside the affected owner to determine its exact contract. Library
modules, experiments and frozen weights have separate qualification; a module's
presence is not permission to resume historical checkpoints.

## Combat-local Node-SL learning

`play_node_sl_combat` executes a saved combat entry or suffix. With no search
configuration it runs the network directly; `NodeSLConfig(objective="combat_hp")`
uses exact checkpoint restarts. This is Seed-SL search. Each expanded node first
tries its alternatives in model-prior order, then uses a UCB-style work-allocation
bonus. A near-zero prior does not remove an action's exploration bonus. Priors
also break ties, and leaves use network rollout or scalar value. Finite budgets
can still leave actions untried; this is not exhaustive search or a statistical
confidence guarantee.

Rollout leaves sample actions from the network probabilities. `rollout_seed`
initializes a private RNG per improver, advanced across its searches; identical
inputs and call sequences reproduce the trials. It changes explored action
sequences, not the simulator's hidden draw order or RNG. Model-only evaluation
and adoption of a completed search plan remain deterministic. Near-uniform
untrained policies can therefore try varied continuations instead of repeating
the same argmax mistake at every leaf.

Actual task terminals close exact edges; a node is solved only when all its
actions are solved. Exact values propagate through symbolic selections and
combat actions. Solved returns are cached, and a solved root ends search early.
Combat-local allocation skips solved edges and spends further work on open
continuations. A completed rollout, action-limit value, or post-combat critic
does not prove a whole subtree solved.

When combat-local search finds a surviving complete plan, it adopts its best
tested plan and trains on its best observed action outcomes, not allocation
counts. `completed_action_values` records those observed returns; a missing
entry is not a loss, and observed returns are not claims of optimality.
Probability mass is uniform across distinct actions with the best observed
return. Symbolic choices reached only during rollout can have completion-based
targets with zero tree visits. Fully solved targets additionally retain
`solved_action_values`. Before finding a surviving plan, unsolved decisions use
visit targets conditioned on actions not proven fatal while alternatives remain.
If none of those actions have visits, their prior is renormalized (uniformly when
its remaining mass is zero). This avoids teaching a known death from the only
trial; it does not establish that an untried continuation survives.

`play_node_sl_combat` carries the selected plan's remaining actions into the next
search. Replaying this incumbent costs one of the configured traversals and
must finish alive within the same action/selection limits. Its actual return
competes with new plans. The environment is restored even if replay fails.
`simulations` counts traversals including replay; their variable lengths mean
equal simulation counts do not imply equal transition counts or wall time.

Network input remains public, whereas exact Seed-SL search can distinguish
hidden futures through trials. Identical public inputs can therefore produce
different search targets. The public network can learn search priors, but a
model-only actor cannot be expected to reproduce all such privileged choices.

`NodeSLCombatExample` joins the improved search policy to an actually executed
combat exit: death is -1 and surviving exit is exit HP / maximum HP, including
automatic post-combat healing. This local objective does not price potions or
future route resources. Potion-free tasks or separately declared resource
restrictions are needed when using it to study combat skill. Action-limit
truncations retain their state and produce no fabricated terminal value labels.

`update_node_sl_scorer` fits policy and value heads even for an all-defeat cohort;
identical negative outcomes can teach value without identifying a better action.
Combat and full-run objectives cannot be mixed in one update. Split experiments
by source run seed, retain failures, and report suffix tasks, full battles,
model-only play and search-assisted play separately.

## Fixed-combat strategic studies

`sts_learning.strategic_rollout` first exercises continuous play without training:

```powershell
.\py.ps1 -m sts_learning.strategic_rollout --seed 123 --output artifacts/fresh-strategic-run
```

`StrategicRunController` uses the public deterministic baseline and remembers
skipped card rewards within their source collection. An explicitly supplied frozen
`BatchPolicyChoice` policy takes only the typed card-reward and campfire sites
(including rest/upgrade); other sites retain the baseline. Symbolic selections
keep their root's owner. Reward memory commits only with a successful native
macro. An unresolved search preserves pending choices, records an interruption,
and emits no terminal label. The default whole-combat executor has no weak fallback.
Claiming other rewards remaps that memory from the resulting public card-reward
order; reward indices are not treated as persistent identities. A committed
Reward/Proceed also continues onto the map through the shared native route ranking.
Leaving a collection clears its skips; the navigation hint expires after the next
committed root choice, including when a new collection appears on the same floor.

The model surface and task baseline share public candidate admission. Map and
card-choice pages defer potion discard until a later acquisition opportunity;
full-inventory reward and shop pages retain replacement decisions. Manual play
keeps the simulator's raw legal actions. Complete-choice supervision and scope
checks operate on the entire admitted model surface, with no label-based filtering.
The learning map surface commits a route and omits closing the map. A task actor
can still close a browsing overlay to return to pending rewards; manual legality
is unchanged.

`StrategicStudySpec.strategic_navigation` opts into program-owned routine rows;
the `a0-rollout` preset enables it. Single-candidate rows and reward menus containing
only card reveal/navigation bypass both policy inference and actor experience.
Mixed resource menus remain policy decisions. `StrategicRewardMemory` owns the
shared committed collection memory. Its pending choices and skipped rewards are
bound to the environment envelope and slot bank, including reset and restoration.
Training and fixed-seed evaluation use the same row routing. A terminal after
routine actions closes the existing sampled attempt through a terminal-only
fragment, which the assembler rejects without a previously sampled open attempt.

Fresh artifacts retain committed/attempted choices, all public root candidates and
loadouts, combat receipts, actual resources, Act-entry checkpoints and the exact
endpoint. `from_memory` explicitly restores controller memory with the matching
executor-bound environment; the caller separately restores any model sampling
RNG. The default entry loads no learned weights or optimizer, and neither Act
completion nor an interruption is presented as a deck-quality score.
The smoke entry defaults to A20 with the Heart task enabled. `--goal act_three`
selects the shorter task explicitly; old native constructor calls keep their
previous Act 3 default.

To adopt saved complete-decision weights, supply both the model directory and
its explicit sites. For example, `--model-directory <directory> --model-sites
campfire` assigns only campfires to the frozen greedy network; card rewards,
shops and routes retain the public baseline. The loader verifies the live
semantic schema, scorer architecture, model digest and saved round-trip check
before creating a run. It retains source and ancestor seeds, exact contract and
summary digests, and the inference implementation in the behavior identity.
The run contract saves this source binding, and committed choices keep their
model/baseline owner. The optional policy uses CPU inference with two PyTorch
threads; no optimizer or expert labels enter play. Continuous adoption remains
a separate check from offline teacher agreement and full-run strategy quality.

To keep independently trained sites separate, repeat `--site-model
card_reward=<directory>` and `--site-model campfire=<directory>`. This alternative
is mutually exclusive with the shared model flags. Each directory is verified
for its named site and bound in the run contract; mixed batches are routed by
the native root site, and symbolic continuations retain that root's model.
Pending retries and exact restores keep the same per-site identities without
choosing again. Training a new reward model therefore need not change the
frozen campfire policy. Other sites continue to use the native baseline.

For the Heart task, the native baseline preserves a visible Act 3 path offering
all missing keys together, takes an offered sapphire key in Act 3, and recalls
when no complete visible continuation can defer it. Earlier acts retain ordinary
resource choices. These are goal constraints over public topology, not estimates
of combat safety; the fixed executor still has to solve the required fights.
Run-choice context exposes the selection effect, scope and choice bounds for
both concrete candidates and symbolic families. The baseline scores selected
cards by that effect; upgrade choices share the campfire upgrade evaluation.

### Numerical collection and scoped imitation smoke

`strategic_rollout --max-steps N --capture-samples` saves public numerical rows
for card rewards and campfires in `samples/`. The manifest keeps the exact
candidate audit, source tokens, executor/producer identities and commit status.
Bootstrap outputs are excluded from model inputs. NPZ rows contain only arrays
and scalars and are read without pickle. Interruptions retain attempts without
inventing terminal returns; adopted baseline actions have no expert target.
The v2 manifest keeps a `decision` envelope with a nullable chosen ordinal.
A scope stop records `owner=observation_only`, `ordinal=null`, and
`committed=false` in the same numerical collection. It supplies neither an
adopted action nor an outcome target. Existing v1 sample files stay immutable
and are normalized by the sample loader when read.

`sts_learning.strategic_behavior_cloning --samples <sample-directory> ...
--output <fresh-directory>` exercises a fresh small scorer using committed
baseline actions. Supply runs from both partitions of `SeedPartitionSpec`; the
split happens by source seed before concatenation. Mixed producers, duplicate
source decisions and changed row hashes are rejected. The smoke saves its
contract, weights and agreement measurements, and checks weight round-trip and
candidate-order equivariance on held-out inputs. Baseline agreement measures
imitation, not expert decision quality, RL improvement or A20H qualification.
These weights are not automatically adopted into run-play.

Independent reviewer assessments can be bound to these sources with
`save_scoped_strategic_preferences`. Its JSON overlay stores the reviewer,
reference provenance, annotation protocol, manifest/row hashes and source
tokens without rewriting samples or adopted actions. Each row has unordered
`preferred`, `disfavored` and `unknown` sets. A preference asserts only that
each preferred choice ranks above each disfavored choice in this observation;
it does not declare a unique optimum or make every other choice wrong.
Unreviewed rows remain Unknown. Conflicting sets, repeated sources, undeclared
mixed producers and changed hashes are rejected. Uncommitted samples require
an explicitly authored `basis=public_observation` to receive teacher labels.
Reviewer assessments informed by guides are not those guide authors' labels.

`sts_learning.strategic_behavior_cloning --preferences <overlay.json>
--output <fresh-directory>` trains a fresh scorer on resolved pairs using
`softplus(disfavored - preferred)`, taking the worst resolved pair within each
row and averaging across supervised rows. This focuses on the largest known
ordering error, so repeated easy alternatives cannot dilute one important
comparison. `--preference-reduction mean` explicitly selects the original
average-pair objective for declared comparisons. Co-preferred candidates are
not ranked against each other.
Unknown candidates receive no direct label gradient; shared model parameters
can still change their scores. Labels never filter inference candidates or
force a preferred choice. The original seed split isolates held-out
labels from all updates. The summary separately counts preferred, disfavored
and Unknown selections, reports pair ordering/loss and retains row predictions
for diagnosis. Unknown is not scored as success or failure. This is an offline
supervision smoke, not an RL or win-rate result, and is not adopted into play.

### Teacher authority and decision supervision

Codex is an explicitly designated source of strategy demonstrations in this
project. Its judgments do not wait for a separate human approval or a superior
teacher. Author, observations, rationale and revisions supply traceability;
this does not claim that the demonstrated action is globally optimal.

The current preference CLI teaches **relative comparisons**, not a complete
policy target. Even a perfect ordering of every resolved pair can leave an
uncompared candidate at the global maximum. Evaluating that global choice is
necessary; pair agreement alone cannot qualify this scorer for play.

Complete decision supervision uses an explicit set of actions
the teacher would adopt from the **whole supplied candidate surface**. Several
reasonable actions can share the set. If the teacher cannot decide the row,
the row supplies no decision target. Such a target must be separately authored;
it cannot be inferred from the current overlay's partial preferred set. A
decision loss must involve all competing candidates, and inference must keep
the legal candidate surface rather than use labels as an answer filter.
`save_scoped_strategic_decisions` saves this separate target as `acceptable`
(a nonempty ordinal set or whole-row `null`), with author, revision, rationale,
reference provenance and the same verified source bindings. Supplying partial
preference labels without an explicit decision target is rejected. The source
observations and adopted baseline actions remain unchanged.

An optional `preferred` nonempty subset records a recommendation among that
complete acceptable set. It cannot accompany an Unknown row. Overlays declaring
this field use `sts-scoped-strategic-decisions-v2`; each row explicitly stores
the subset or `null`. Without the field, v1 keeps its original structure and
interpretation. Training uses the preferred subset when declared, otherwise
the acceptable set (`choice_target=preferred-if-declared-else-acceptable-v1`).
The summary separately reports acceptable choices and `recommended`,
`secondary_acceptable`, `outside_set`, or `unknown` recommendations. A secondary
acceptable choice is not a rejected action, but does not meet the declared
recommendation target. Labels still judge the whole legal menu.

Teacher annotations default to `basis=committed_observation`. A separately
authored `basis=public_observation` can judge the complete observed candidate
surface before an action is adopted, including a scope interruption. Such rows
enter the same seed partition and supervised fit, and are separately counted
as pre-adoption supervision in its contract. The sample's nullable choice and
commit status stay unchanged. Baseline imitation still uses only committed
actions; observation labels do not supply returns, witness wins or new runtime
applicability. Unsupported label bases are rejected.

Annotation protocols can explicitly combine different collection origins with
`source_compatibility` (`sts-scoped-source-compatibility-v1`). The declaration
records a rationale, a common `native_sha256`, and an ordered `sources`
permission list. Each entry records its own `sampling_source_sha256` and
identifies `source_index`, `origin` (`constructed`, `natural_run`, or `fixture`),
`input_access` (`public_semantic_only`), `continuation_access` (`public_only`,
`fixed_state_oracle`, or `constructed_transition`), and its rationale. Loading
verifies the full semantic schema, common native identity, and declared sampling
version for every source; conflicting native identities are rejected. Collection
scripts, controller settings and origin metadata can differ and are retained in full,
with source manifest hashes and permissions, in the trained contract. Without
this explicit declaration, producer dictionaries must still match exactly.
Origin and permission descriptions are the named author's source assessment;
hash agreement does not establish natural reachability or NoSL provenance.

```powershell
.\py.ps1 -m sts_learning.strategic_behavior_cloning --decisions <overlay.json> `
  --output <fresh-directory> --max-updates 256 --learning-rate 0.003 `
  --check-every 16 --fit-margin 0.5 --evaluation-role development
```

The default `flat` complete-action objective is the mean of
`logsumexp(all candidates) - logsumexp(choice-target candidates)` over supervised
training rows. Null rows are excluded; every competing candidate in a labeled
row participates. The objective does not prescribe an ordering inside the
choice-target set. Without a preferred subset this is the original acceptable-set
objective. The scorer's inference still takes the full legal surface.
For flat scoring, `--observation-pooling candidate-attention` lets each
candidate query current observation tokens in its own row. Its weighted read
is added to the global mean observation summary before scoring. Queries may
use that candidate's payload; keys and values exclude every candidate payload.
The value head keeps the current-observation mean. Default `mean` preserves
old models, including contracts without this field. Initialization and warm
starts require identical observation pooling; this option learns relevance
from supervision rather than supplying a teacher's relevance mask.
The seed split happens before fitting. By default, fresh initialization and explicit
optimizer settings are saved; checks stop updates only when every taught
training argmax belongs to its choice-target set and its best target score
exceeds every alternative by the requested margin. This includes secondary
acceptable alternatives when a preferred subset is declared. Otherwise the maximum update count
ends the diagnostic, with that stop reason retained. Evaluation labels and
scores do not select weights or stop training. Previously inspected evaluation
rows use `development`; `held_out` is an explicit claim for a fresh panel.
Weights are checked by readback and candidate reversal, and are not adopted
into play automatically.
Training-only corpora are allowed; absent evaluation rows are recorded as
`not_provided`, and the weight checks use training inputs without claiming
generalization.

`model_sites` names only sites with non-null complete-decision training targets.
Offline fitting can omit a usage declaration, but live loading requires both a
successful full-action training fit and `protocol.applicability` in the teacher
overlay. This `sts-strategic-applicability-v1` declaration contains a `sites`
mapping. Each site specifies `acts`, inclusive `floor_range` and
`ascension_range`, and `categorical_domains`: explicit numeric values from the
exact native schema for CardId, RelicId, PotionId, CardMechanicCoverage,
CardMechanicRole, PotionMechanicRole, ActionKind, RunGoal, PlayerClass and
BossEncounterId. Additional native categorical domains may narrow use. Action
identity fields use the corresponding CardId/RelicId/PotionId domain as well;
the reviewed Status-creation identity also uses CardId even when that Status
is neither owned nor offered. Creation requests do not add cards to the owned
deck counts. Their queue timing and destination remain separate numeric facts.
The optional `observation_categorical_domains` maps native token-kind names
to direct categorical-field domains for shared scope-zero observations. For
example, `{"Card": {"CardId": [...]}}` narrows owned inventory independently
of offered-card and action identities. These direct constraints apply alongside
the global domains, without aliasing a creation request into inventory. A card
being covered as an offer does not establish coverage of decks containing it;
declarations remain teacher decisions, not automatically collected vocabularies.
Omitting this optional map preserves the original global-domain contract.
Teacher declarations are copied into the model contract and source identity;
they are not inferred from training vocabularies or inherited from initial
weights. A declaration cannot exclude its own taught training rows.

`StrategicRunController` requires matching typed applicability for every owned
site. It checks the whole public surface before calling any model or adopting
any row in the batch. Outside-scope and unobservable inputs stop the rollout
with `model_out_of_scope`; `out-of-scope.json` retains the unadopted numerical
input, audit, source tokens and reasons alongside the exact endpoint and memory.
Other unowned sites continue under the explicitly recorded baseline. No
outside-scope candidate is filtered or silently delegated to that baseline.
Eligibility permits a bounded diagnostic, not arbitrary deck combinations,
strategy competence or natural A20H qualification. DefinitionOnly/Partial/
Complete describes the engine's encoded facts, not learned mechanism mastery.

For `flat` or `family-target`, `--initial-model <directory>` initializes the entire network
from verified weights and allows the encoder and policy heads to learn. It
retains the source contract, hashes and ancestor seed partitions. The saved
source remains unchanged; training still stops on training rows alone. This
option is mutually exclusive with the two frozen refinement modes below.
Recheck previous decisions before adopting the updated model, since this mode
preserves no parameter path or previous choice.

Fresh scorers initialize scalar slopes at zero. A numerical field seen only at
zero then contributes no random direction when a nonzero value first appears;
nonzero training observations can learn its slope. Verified saved weights are
loaded exactly, including older slopes. Constructed studies should also vary
unrelated public resources to check that candidate choices follow the taught
conditions rather than incidental input values.

`--decision-architecture family-target` opts into separate action-family and
within-family target scores for complete decision supervision. Groups come
from the native `ActionKind` categorical field, with exactly one value per
candidate. Choice-target labels supply a family set and target members
inside each of those families. The loss is family set cross entropy plus the
mean target set cross entropy across choice-target families, then averaged over
supervised rows. Null rows teach neither level; co-target families and
targets have no prescribed internal ordering.

Greedy inference preserves every legal candidate and uses
`family_score + (target_score - best_target_score_in_family)`. Each family's
best target retains its family score, so the full argmax selects a family and
then a target. Target offsets and probability dilution from the number of
targets do not directly decide the family choice. Shared observation tokens
receive no messages from candidate scopes. Each candidate reads the shared
observation plus its own payload, and only shared tokens enter the row/value
pool. The original `vector-max` family head uses componentwise maximum pooling
over candidate representations. `--family-pooling candidate-score-max` instead
scores each real candidate with the family head before taking the family's
highest score; favorable coordinates from different targets cannot form a
synthetic target for that head. Both keep exact duplicates from changing the
aggregation. Distinct menu content can still affect the shared public reward
observation, and target ranking remains separately learned.
The pooling mode is saved in the scorer contract. Old contracts without this
field mean `vector-max`; loading them preserves that interpretation, and warm
or frozen refinement rejects a different pooling mode. Ownership is topology,
not an index feature.
Canonical token order fixes floating-point pooling and target-head evaluation
order, with scores restored to the caller's candidate order. Indices are not
input features. The summary reports full-action, family and conditional target
agreement separately. Native card-reward and boss-relic candidate option indices
remain execution/audit addresses and are omitted from semantic numerical
features. Meaningful order such as visible combat pile positions is retained.
A target choice is Unknown when the selected family is
outside the teacher set. Fit stopping still requires full-action agreement and
margin. Flat models remain the default; warm starts require the same family
field contract. The ordinary categorical sampler applies softmax to composed
candidate logits; it does not implement separate family and target sampling.
This architecture is an offline experiment, not qualification for natural play.

For an isolated family-learning task, combine `--decisions <overlay.json>` with
`--decision-architecture family-target --supervision-level action-family`.
This fresh fit derives native `ActionKind` targets from preferred actions when
declared, otherwise acceptable actions. Unknown rows and rows permitting every
offered family contribute neither loss nor informative accuracy. The full public
graph and candidate menu remain inputs; target scores receive no supervision.
Stopping checks only training family choices and margins. Predictions contain
native family codes, without action ordinals. The distinct
`sts-scoped-family-clone-v1` artifact is rejected by the complete-policy loader
and refinement entry points. `complete-action` remains the default. Family
agreement alone does not establish which card to take or correct conditional
valuation of the cards; this component has no gameplay adoption entry.

With `family-target`, `--frozen-family-model <verified-model-directory>` refines
only the target scoring head from a saved complete-decision model. The source
schema and scorer configuration must match, and its weights must match the
saved summary hash and round-trip record. Source contract, summary and weight
hashes identify initialization; the new model has no fresh initialization seed.
The graph, family head and optional value head remain frozen and are checked
for exact tensor preservation after fitting. This preserves the initialized
family-score function while target priorities change; it does not repair a bad
family policy or expand the frozen representation's capacity.

Every taught training family must already meet the requested margin before
target-only updates begin. An incompatible family source is rejected rather
than changing labels or silently unfreezing it. Full-action fit stopping and
the complete legal candidate surface remain unchanged. Source and ancestor
training/evaluation seeds are retained: a source-trained evaluation seed is
rejected, and previously evaluated seeds require `development`, not `held_out`.
Seed separation alone does not establish fresh cases or strategic quality.

For an incorrect family policy, `--frozen-encoder-model <directory>` keeps the
graph representation and optional value head fixed while fitting both
`family_head.*` and `scorer.*`. It uses the same verified source and ancestor
seed bindings, complete-action supervision and train-only stopping. The two
refinement sources are mutually exclusive. This mode can change family
choices, but cannot expand the frozen representation; preserving its tensors
does not guarantee that previous full-action choices remain correct. Check
earlier decisions alongside new public observations before adopting weights.

Before studying generalization, run a bounded training-only fit diagnostic with
explicit optimizer settings and a stopping condition on the taught decisions.
Insufficient fitting cannot establish an architectural or representation limit.
After fitting, inspect new choices and responses to relevant public conditions.
Fitting naturally different decks can leave conditions confounded: for example,
all taught low-HP rests might belong to one deck. Constructed condition checks
must state the changed public fields and preserve source observations. Any
contrasts used for training are separately declared; their fit is not evidence
of migration to unseen decks or natural game outcomes.
Previously inspected held-out rows become development evidence for subsequent
design changes; they are not a fresh final evaluation. Game and card mechanics
remain engine-owned facts. Strategy quality is judged against the named teacher
and actual decisions, not inferred from loss alone.

`--decision-architecture field-membership` selects a distinct LightGBM backend
at the same complete-decision fitting entry point. Install the `tabular` extra
alongside `torch` for the existing corpus/fit owner. `--field-num-leaves`,
`--field-max-depth`, `--field-min-data-in-leaf` and `--field-min-data-in-bin`
configure fresh trees; neural pooling and initialization flags are rejected.
The loss classifies each legal candidate's membership in the teacher's
choice-target set. `--field-objective lambdarank` instead groups every full legal
menu and ranks target relevance 1 above other relevance 0. With a declared
preferred subset, secondary acceptable candidates receive 0 for this choice
objective; their acceptable status remains in the separate evaluation.
Explicit `--field-relevance preferred-acceptable-v1` with `lambdarank` instead
uses relevance 2 for the choice-target set, 1 for secondary acceptable choices,
and 0 for outside-set choices, with linear label gains. Without a preferred
subset, every acceptable choice has relevance 2. The default `choice-target-v1`
and old contracts keep the original targets; relevance is bound in the contract
and behavior identity. This learning objective does not establish that all
lower-ranked acceptable choices have been learned correctly.
Reports also isolate rows with an explicit preferred subset, including their
choice agreement and margin. Counts distinguish preferred supervision from
acceptable-set fallback and Unknown; fallback agreement is not evidence that
an additional teacher recommendation has been learned.
Scores do not represent full-acceptable-set membership probabilities when
preferences are declared; ranking scores are comparative. The explicit objective is stored and checked against the saved
trees; old field contracts without it retain the original binary interpretation.
Unknown rows teach no positives or negatives. Stopping still
requires every taught whole-action argmax and the requested best-set margin.

`--frozen-field-model <verified-directory>` uses the same entry point to qualify
unchanged weights against a new explicit teacher corpus and applicability.
It inherits source readout, objective, relevance and fit margin, performs zero updates,
and copies weight and vocabulary bytes into a new output directory outside the
source. Optimizer, model-seed and field configuration flags are rejected in
this CLI mode. The Python API accepts a typed configuration to request a fit
margin; its objective, readout and relevance must match the verified source.
Every taught training decision is checked again, including old targets in the
new corpus; Unknown supplies no target. A failed check saves `frozen_unfit`
diagnostics that the deployment loader rejects. Evaluation remains after this
decision and cannot qualify training fit. Source contract, summary, behavior
identity, digests and ancestor seeds remain bound in the new artifact; previously
evaluated seeds cannot be presented as fresh held-out evaluation. Qualification
does not certify the declared range's unseen combinations or strategic quality.

Columns are named public fields from shared observation plus each candidate's
own scope: categorical/token counts and scalar count/sum/min/max by token kind.
Presence counts distinguish missing from supplied zero. The training-only
vocabulary is frozen; unseen columns are ignored, never added at inference.
Both fresh fitting and frozen qualification list `ignored_columns` separately
for training and evaluation. This reports readout omissions; it does not expand
the vocabulary or establish that the omitted facts are irrelevant to a choice.
`evaluation_support` separates new read observations, known observations with
new candidates, and decisions whose candidate numeric inputs were all seen in
supervised training. It also counts distinct observations and full-fact overlap;
ignored facts can change while the frozen numeric projection stays identical.
Here full facts means the chosen readout before vocabulary projection, not the
native observation graph.
These cohorts explain evaluation support, not learned competence. They use no
labels or scores, and do not change fitting, stopping, partitioning or adoption.
`reference_role` distinguishes the current supervised training partition from
a frozen qualification partition; the latter compares inputs without updates
and does not claim the source model was trained on them.
Addresses, scope numbers and token indices only resolve ownership.
The default `owned-field-statistics-v1` readout discards graph relations and
associations among fields on individual entities. Explicit
`--field-readout owned-card-upgrade-statistics-v1` retains these columns and adds
only owned `Card` counts, `CardCanUpgrade` counts and `CardUpgrades`
count/sum/min/max grouped by the same token's explicit `CardId`. Each owned card
requires exactly one such identity. Offered cards, candidates and upgrade
previews do not become inventory. This preserves which owned card is upgraded
without adding effect tables, entity graph joins or strategy rules.
`--field-readout owned-card-local-choice-statistics-v1` retains that owned-card
association and omits only shared scope-zero `OfferedCard` tokens. Current
inventory and each candidate's own complete payload remain available; changing
another offered card's catalog facts cannot change those candidate features.
Its vocabulary rejects shared catalog columns rather than silently accepting
an incompatible saved model.
`--field-readout owned-card-class-local-choice-statistics-v1` additionally groups
owned cards by their explicit `CardType` and whether their explicit rarity is
`Basic`, preserving each group's count and direct scalar count/sum/min/max.
The same card supplies its group and numbers; shared offers and candidate
previews cannot enter these inventory groups. Basic's value comes from the
schema. This preserves associations such as the amount of damage belonging to
basic attacks, without card-name value rules or per-turn output estimates.
This is a pointwise choice representation; general
menu interactions, joint rewards and multistep shop planning require their own
representation. These readouts do not estimate combat value, startup reliability
or deck strength. Readout
and vocabulary belong to the saved contract; old models retain the original
default interpretation.

The artifact stores `model.lgb`, `feature-names.json`, `contract.json` and
`summary.json` under the separate `sts-scoped-field-classifier-v1` contract.
The existing rollout loader verifies schema, typed sites, explicit applicability,
complete training fit, tree and vocabulary digests, feature domains and finite
weights. Fitting checks exact score round-trip and candidate-order preservation.
The same controller stops an out-of-scope cohort before scoring or adopting any
choice. No labels or teacher service enter runtime inference. These interface
checks permit a bounded diagnostic; they do not certify strategy or natural A20H.

The same rollout controller can explicitly use `--card-reward-guide
act1-first-attack-v1` instead of a card-reward model. This compiled Codex guide
reads the native public semantic numbers and paired audit identities, with no
fitting, row-label lookup or external model call. It is limited to an Act 1
Ironclad starter inventory with Burning Blood and optionally Neow's Lament.
The paired audit's card identities and relic lifecycle must match the native
semantic facts. At least half HP favors a covered efficient attack over small
block or conditional attacks and skip. Hemokinesis is preferred to Wild Strike
when its actual upgraded damage is at least as high. On floors 1..4, at least
three-quarter HP and four unupgraded Defends favor skipping unupgraded Ghostly
Armor, Clash, Iron Wave and Body Slam. At most quarter HP favors Ghostly Armor
over Hemokinesis, Reckless Charge and skip once Neow's Lament has no remaining
activations. These are teacher strategy defaults, not optimal or mechanical HP
thresholds. Each relation records unmet conditions and the preferences it supplied.
Added cards, other relics, intermediate HP, competing attacks and joint reward
preparation can leave the choice unresolved. The partial-order result retains
every legal option, including uncovered contenders; conflicts or missing facts
stop the cohort before submission. `guide_unresolved` saves the menu and exact
endpoint. A guide receipt has owner `guide`, and recorded samples use
`compiled_guide` rather than claiming model or baseline labels. Restoring a
controller requires the same guide binding. A campfire model can remain
separate; a card-reward model cannot simultaneously own this prototype's site.
No residual model comparison or ordinary-run coverage is implemented yet.

```powershell
.\py.ps1 -m sts_learning.strategic_rollout --seed <fresh-seed> --output <fresh-directory> `
  --card-reward-guide act1-first-attack-v1 --max-steps 12 --capture-samples
```

This is a bounded guide/interface diagnostic with the existing fixed combat
executor, not Flash, reinforcement learning or autonomous A20H evidence.

`sts_learning.strategic_study` is an opt-in experiment: a learned strategic
policy chooses actions and a fixed oracle combat executor resolves fights.
Results describe that combined controller, including its combat search limits.
New specs default to the Heart task and the whole-combat executor without a weak
fallback. `final_act: false` explicitly selects Act 3. Unresolved searches stop
the study without a fabricated training transition; an explicit fallback
remains available for separately declared experiments. This study controls the
general strategic candidate surface, whereas the scoped imitation smoke above
contains only card rewards and campfires.
Set `executor.backend` to `whole_combat` to use the maintained
`sts_guided_search::solver::CombatSolver`; `portfolio` retains the earlier
executor for explicit comparisons. The whole-combat backend consumes
`generation_work`, `wall_ms`, the engine step limit and potion constraints.
It does not use the portfolio's atomic challenger, quantum or lane schedule.
Both backends independently replay selected wins through full-run settlement.
The fixed macro first removes a mugged exit from its selection when a rewarded
exit strictly improves represented resources with no HP, potion, persistent,
non-gold reward or relic-state sacrifice. It uses the shared outcome comparison;
the original candidates and exact outcome frontier remain available for explicit
branch adoption. It then orders the remaining frontier by HP, potion expenditure,
plan length and ordinal. Search-request quality is recorded as evidence rather
than an execution filter. The receipt binds this policy through executor revision.

New studies default to `reward_metric: "act_completion"`: each entered next act
earns 0.25, and victory earns the last completed-act increment. A Heart victory
totals 1; dying anywhere in Act 1 earns zero. Decision-local returns exclude
already completed acts. Moving through more rooms within an act earns no reward.
The historical `floor_progress` objective remains explicitly selectable.

Whole-run REINFORCE and PPO sum policy terms within each completed attempt,
then average across attempts. A decision's actor weight is `1 / attempts`,
with zero PPO actor weight for forced choices; it is not divided by the
realized number of decisions in its run. This matches the trajectory
likelihood-ratio gradient of expected terminal return before PPO clipping and
regularization. Optional advantage normalization uses a shared batch scale.
The auxiliary critic still averages errors within each attempt and across
attempts. Actor/entropy loss magnitudes therefore grow with decision count;
gradient clipping and the declared learning rate remain part of the training
configuration. Trainer identities distinguish this objective from historical
per-attempt mean-policy losses; existing optimizer sessions cannot silently
resume under the changed objective.

Strategic studies may explicitly set `value_prediction_target: "terminal_score"`
instead of the default `"return_to_go"`. This scalar head learns the final run
score. The objective subtracts already earned public progress from its output
before return-to-go advantage, clipping and value diagnostics; rewards and the
terminal goal are unchanged. The exact conversion is floor progress capped at
the declared target, or completed-act progress for `act_completion`. Actor and
value errors are computed in final-score units to keep equal outcomes equal
despite float32 rounding. The choice changes trainer provenance and enters the
saved study/session contract. It is an experimental baseline representation,
not a guarantee that PPO preserves imitation choices or improves run quality.

`canonical_behavior_rows: true` selects the categorical behavior's explicit
single-row scoring mode. Float32 inference then uses the same kernel shape for
an observation regardless of the collection cohort size; sampling, RNG streams
and environment execution remain batched. This mode has a distinct behavior
configuration identity. PPO keeps its strict probability tolerance and its
single-row replay check; changing mode requires fresh declared trajectories.
The whole-run training CLI exposes the value target through
`--run-value-prediction-target`; publications and critic probes retain its
return profile and units. Critic initialization requires the same value target.
Act completion is a coarse learning signal, not a deck-strength or resource
evaluation; inspect the saved card choices, HP and potions alongside it.

```powershell
.\py.ps1 -m sts_learning.strategic_study --help
```

For a bounded A0 whole-run learning probe, use the maintained preset:

```powershell
.\py.ps1 -u -m sts_learning.strategic_study --preset a0-rollout --output <fresh-directory>
```

This explicitly selects Act 3, floor-progress returns, two initializations and
actual public-rule combat continuation when the fixed search finds no witness.
A fallback action limit still produces Unknown, never a defeat label. Native
public strategic rules, uniform choices, and initial/trained networks use the
same combat executor and held-out panel. The preset retains combat outcomes,
search budgets and costs in compact receipts rather than storing every searched
plan; it makes no replay-certification claim. Training updates report completed
episodes and elapsed time. `--spec` remains available for a declared custom
configuration; it is mutually exclusive with `--preset`. Neither changes the
default Heart/witness-only protocol above.

The training time budget is checked after a completed update. Reaching it saves
the final model and resumable session, evaluates the completed checkpoints, and
continues to the next initialization. Reports distinguish actual and planned
episode counts. Declared evaluation milestones also retain resumable sessions;
executor errors and incomplete generations remain failures.
Retained trajectory and tensor-concatenation byte limits scale with the declared
episode batch; the contract records both, and each update reports its payload size.
Training failures retain the last adopted frozen model for evaluation;
resumption uses a previously published session.

Set `final_evaluation_seed_start` and `final_evaluation_seeds` to reserve a
separate final test panel. Its exact held-out seeds are frozen in the initial
contract and must be disjoint from the development panel, including cloning
validation. Only after every initialization stops training does this panel run
the initial/final model pairs, uniform policy and enabled rule baseline. It uses
the same sampled-policy and combat contracts; it selects no checkpoint and
performs no further updates. Final-test outcomes and evidence live under
`final-test/`, included in the study's artifact verification. All initializations
share its game seeds, so their results are not independent game samples.
`evidence_max_bytes` declares the per-file evidence bound (default 256 MiB),
separately applied to the study and final-test logs.

An optional `rule_initialization` study spec initializes the actor by imitating
the public strategic baseline before PPO. It requires `strategic_navigation`.
`training_runs` selects run seeds from the training schedule; `validation_runs`
selects the first declared held-out panel seeds for agreement diagnostics only.
The remaining fields are `epochs`, `learning_rate`, `batch_rows`, `max_rows` and
`max_array_bytes`. Epochs and batches are fixed before collection; validation
does not select weights or stop fitting. Small batches bound training and
agreement tensor materialization.

The same `StrategicRunController` collects all strategic sites and symbolic
continuations. Program-owned navigation is excluded using the PPO environment's
routine resolver. Only actions whose enclosing native macro commits become
imitation targets; losing runs remain in the corpus. Saved v3 numerical samples
include symbolic choices without a root audit. Unresolved macros retain their
uncommitted samples and stop initialization. This is baseline imitation, not
optimal-action or expert supervision. No game state machine is duplicated.
Reward navigation reads `reward_card_item_indices` from the core's public
collection inventory. A pending potion claim may narrow the legal menu while
the skipped card reward remains in that collection; menu visibility does not
remove or rename the remembered item.

Each initialization retains its matching random network control, cloned actor
weights, training configuration, agreement counts and source trajectories. PPO
starts with a fresh optimizer and objective-local critic; demonstrations do not
enter on-policy experience. Whole-run evaluation separately reports `random`,
`initial` (the clone), and `final` on the same fixed seeds. The random control has
zero PPO updates, so that comparison alone cannot attribute differences to PPO.
On-policy checks retain the original propensity tolerance. A small float32
batching discrepancy may trigger a replay of that exact semantic row with the
same scorer; only a matching replay is accepted. Larger discrepancies and
nonmatching single-row probabilities still stop training.

New trials require a fresh output directory and a declared seed/configuration
panel. Keep executor settings, source identities, all outcomes and unresolved
cases. Held-out panels also save public strategic choices and before/after
resource contexts for each committed macro step in `strategic-evidence.jsonl`;
strategic choices include their pre-choice public run context and every legal
candidate. Frozen models also record each candidate's sampling probability;
uniform baseline choices record their uniform distribution. Map candidates carry
their visible ordinary-connection suffix summaries. Map and campfire choices
retain the public deck and relics at that decision, so route and rest/upgrade
comparisons can be read against the loadout then available. Older artifacts may contain only the
selected candidate or omit the public context and loadout. Combat search witnesses,
weak fallbacks and wall-limited combats are counted separately. Reports also count
committed fallbacks that immediately end at zero HP; this is an executed controller
outcome, not proof that the combat had no winning line. The contract also
records the native dev/release build profile. `FixedCombatConfig` can raise
the outer, per-quantum and atomic wall limits while retaining explicit work
limits. A work-budget comparison must check the receipts for zero wall-limit
exits; equal numeric settings alone do not establish equal search effort across
builds. Results are saved as JSON, model checkpoints and source hashes; this
entry does not generate prose reports or copy the source tree per experiment.
Verification of saved file hashes is distinct from replaying games.
Unresolved combat does not become a fabricated terminal training transition.

The public deterministic bootstrap has a separate coverage-only panel via
`learning/dev.ps1 evaluate-baseline`. Its v2 artifact keeps every planned seed,
including step/selection-limit censoring and the last public HP, gold, potions,
and decision site. Censored runs are never counted as defeats or included in
completed-only floor averages. The bootstrap has no run-history memory and can
cycle at reversible reward pages; its panel is not a strategy-quality baseline.

## Other local experiments

The newer combat/entity demonstrations and dedicated networks live under
[tools/guided_search](../tools/guided_search/README.md). The upstream PPO comparison
lives under [tools/rl_reference](../tools/rl_reference/README.md). They are separate
experimental callers, not stages that must all be run in sequence.
