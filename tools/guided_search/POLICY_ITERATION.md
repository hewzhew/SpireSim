# Finite-search policy/value iteration

`policy_iteration.py` initializes an entity action network from fresh whole-combat
solver demonstrations and deploys it with bounded PUCT. Each generation plays a
student cohort, requests solver continuations at sampled student decisions, and
adds the better observed winning continuations to this run's policy examples.
Subsequent policy updates rehearse those accepted examples. It then freezes the updated
policy while newly played PUCT fights and sampled search-query continuations teach its separate critic. The solver is an
external teacher; student actions do not automatically become improvement labels.
It reads no previous corpus or weights. Results and checkpoints belong in the
chosen artifact directory; this document defines the executable contract.

```powershell
.\learning\dev.ps1 refresh-bridge
.\scripts\py.ps1 tools/guided_search/policy_iteration.py artifacts/new-policy-run --cases 64 --simulations 16 --updates 320 --iterations 3
```

The declared population is constructed A20 Ironclad against Jaw Worm, Gremlin
Nob, Lagavulin and Guardian. Recipes include unupgraded Armaments and upgraded
True Grit in the boss deck. Initialization and the repeated evaluation start at
60/75 HP. Later policy teaching and value collection cross HP 45/60/75 with an
unchanged deck, one Strike replaced by Defend, or the reverse. The nine contexts
occur within every encounter over each 36-case cycle; a partial cycle need not
be balanced. These are constructed loadout interventions, not natural floors,
whole runs, A20H rates or NoSL play. The input and simulator searches are Oracle.
No potions are supplied or allowed in this
initial population. The bridge supports explicit other recipes within the
[entity envelope](ENTITY_LEARNING.md); that does not establish their training coverage.

## Decision and learning contract

- `PolicySearchBatch.evaluation_batch()` advances independent trees to pending
  neural leaves. `evaluate(revision, probabilities, values)` supplies their
  normalized legal-action priors and values in [0,1]. All rows validate before
  any prediction is installed.
- After the declared simulations, `decision_batch()` returns root entities,
  edge priors, visits, mean backed-up Q-values, root predictions and terminal
  statistics. An unvisited Q is absent, not a zero reward. `terminal_visits` counts
  backups from terminal leaves (including repeated visits); `terminal_values`
  supplies exact outcomes only for immediate terminal successors. The exported
  `regularization` is the current PUCT bonus multiplier in smoothed-visit form,
  `1.5 * sqrt(N+1)/(N+A)`, for diagnostics, not an additional search parameter.
  `advance(revision, choices)` adopts visited source-bound branches.
  It does not wait for a complete winning path. Selection assembly uses the same
  factored grammar, and only completed selections become simulator inputs.
- The root neural evaluation is initialization, not a visit. Each simulation
  backs up one terminal result or leaf estimate, with no sign flip in this PvE
  task. Repeated terminal visits count normally. Each actual decision starts a
  fresh tree. This implementation has no subtree reuse or transposition merging.
- The local utility is 0 for defeat/escape and `(1 + terminal HP/max HP)/2` for
  victory. HP includes settled victory hooks. This is an explicit initial
  tradeoff between winning and retaining HP, not an A20H value function. Potion
  use is separately constrained; no implicit HP price for potions is learned.
- Fresh solver demonstrations provide selected-action imitation targets and
  their own actual returns. The teacher uses `--teacher-work` per construction
  during initialization and per sampled source in later generations, selecting
  the highest-HP discovered win.
  These are executable plans, not optimal-action labels or fabricated visit
  counts. A solver budget without a win contributes no demonstration or loss label.
- Each later generation first plays a deterministic student cohort. It samples
  one ordinary multi-action decision uniformly per fight and, when present, one
  attached selection root. Selection reasons are balanced across the cohort by
  querying a least-represented available engine reason, with random ties and a
  uniform decision within that reason. This prevents rare exhaust/upgrade tasks
  from disappearing among ordinary plays. There are at most two teacher queries
  per fight; a fight with only one group uses that group, and one with neither
  uses its first recorded decision. `--teacher-work` is per query, so the added
  coverage also adds explicitly recorded teacher work.
  `policy_demonstration(spec, work, prefix_choices=...)` replays the student's
  source-local branch ordinals under the same construction and current grammar.
  PendingChoice roots (including Armaments and True Grit) are supported; unfinished
  virtual selection prefixes are excluded from sampling and rejected by the bridge.
  Already spent potion uses reduce the suffix solver's remaining allowance.
  Prefix replay adds work outside the declared solver budget; its branch count is
  recorded. Solver outputs use query-local slots; `corrections.json` maps each
  query to its original fight, decision and group. Accepted policy examples retain
  the original fight slot. The returned first position must match the collected
  student position.
- At each sampled source, a solver suffix replaces the actual student suffix only
  when its observed terminal utility is higher (or the student was truncated).
  An equal or better student win is retained, also when the solver finds no win.
  When neither continuation wins, no policy target is invented. The selected
  suffix supplies ordinary adopted-action imitation rows, without oversampling
  its root. This compares two executed continuations, not every action or optimal
  play. It does not guarantee that fitting the network improves other states.
- Deployment evaluation and main-loop value-data collection both use unchanged
  model priors and maximum visits throughout the fight. Equal-visit leaders use
  the highest backed-up Q; an exact Q tie retains source-local order. The recorded
  actor is `argmax_visits_then_q`. This uses the existing tree evidence, without
  an additional search budget or a hardcoded action preference. Each collected row retains a
  one-hot record of its adopted action. These student decisions, including
  unsuccessful fights, remain available for diagnostics. The deterministic
  student's winning suffixes may be retained by the comparison above; value-cohort
  decisions do not supervise the policy. Their actual returns teach the critic.
  `collect(training=True)` remains an explicit diagnostic actor: it adds 25%
  Dirichlet root noise with alpha 0.3 and samples normalized visits, using the
  declared NumPy stream across cases. Its outcomes estimate that exploratory
  actor and are not used as main-loop deployment-value targets. Search visits
  remain available separately from adopted-action records.
- Initial policy cross-entropy fits fresh demonstrations; initial value fits
  their actual bounded returns. Separate policy and critic copies then have
  persistent optimizers. Each 32-row policy minibatch draws 16 rows from the
  accumulated accepted student-state continuations and 16 rows from this run's
  initial fresh demonstrations, with replacement. The continuation pool starts
  empty and appends the accepted rows of each generation before its update. A
  generation with no new winning continuation still rehearses previous accepted
  continuations; if the pool is empty, all 32 rows come from initialization.
  The pool assigns disjoint fight slots to each cohort; repeated local slot numbers
  in different generations never merge their decision weights. Saved per-generation
  rows retain their original local slots.
  Initialization itself requires at least one found plan.
  Within each pool, the objective gives fights equal mass and decisions within a
  fight uniform mass. Policy minibatches sample only decisions with more than one
  legal action, weighting each sampled loss by that fight's choice fraction.
  If forced-only fights or pools are omitted, their remaining mass is corrected
  as well. This conditions sampling on useful gradients while preserving the
  original expected objective. Forced phases remain in raw trajectories and value
  data; when both pools contain only forced phases, the action update changes no
  weights. Longer plans or an extra attached-choice query do
  not increase that fight's weight relative to other encounters. Accepted
  rows retain their construction slot, whose encounter/context schedule is fixed
  within this invocation.
  This continues the imitation objective with new seeds and construction contexts.
  Only observed winning plans supply action targets; unsuccessful student play
  still contributes actual failure labels to value learning. This policy pool
  lives within one invocation under the unchanged simulator and numerical schema.
  Earlier executed plans remain feasible demonstrations, not claims about the
  current actor's return or optimal play. Their returns never enter critic replay.
  No historical corpus or previous invocation's examples are read. That update
  changes neither the critic nor the unused policy
  value head. The updated policy is frozen for value collection and fitting.
  Neither network is trained during a
  combat search.
- Value cross-entropy fits the actual terminal utility of the current cohort's
  deterministic deployment actor. Its decisions use bounded PUCT throughout the fight;
  a bare greedy policy's failure is not substituted for this actor's outcome.
  Each visited root gets that fight's actual return. Sampled pending queries
  independently continue with the same simulation budget, frozen policy/critic,
  maximum visits with the same Q tie-break and no root noise. Query rows and played-decision/continuation
  rows receive equal learning mass when both exist. This estimates the generating actor with its
  collection-time critic, not optimal play or the future actor. Teacher searches
  still discover alternative plans; there is no requirement to execute random
  actions throughout a fight to teach this value target.
- Previous-generation value labels are not replayed: they describe a different
  actor. Policy loss fits action distributions; observed returns select the
  demonstrated continuation but are not policy loss weights. Critic weights and
  optimizer persist across generations. Truncated fights supply no terminal
  value label. No exhaustive branch labels are required,
  and failed play does not prove a state unsolvable.

## Outputs and review

Solver stages write construction recipes, work budgets, per-case found-plan
results, replay prefixes and ragged numerical samples with selected branches
and returns. Later teaching stages keep these raw solver results in `solver/`;
their own `samples.npz` holds adopted continuations. `corrections.json` records
the sampled source, student/solver returns and whether the solver, student or
neither supplied the teaching suffix. The student collection retains its full
executed trajectory separately.
Student fights additionally retain executed simulator inputs in `evidence.json.gz`.
Their `search.json.gz` contains one root-statistics record per `samples.npz` row,
in the same order, retaining the original prior, visits, Q and terminal evidence
independently of its imitation target and eventual played-fight return. These
statistics are diagnostic/training data, not new inputs to the network.
Summaries include the generating actor's simulation budget, action-selection rule
and root-noise settings, plus HP, potion inventory/use, decisions, transitions and
timing. The diagnostic
`end_with_playable` is not an error label: legal early EndTurn can be correct.
The eight repeated evaluation seeds are development checks; once inspected,
they are not an independent final test. To compare frozen models, collect a
separate untouched population with the same recipes and search budget.
Each generation collects separate policy-teacher and value-data cohorts, then
runs `--updates` optimizer steps for each network. Policy improvement therefore
adds training work, including the extra deterministic student cohort and prefix
replay. Solver work per query and student simulations per decision
are different budgets; equal deployment budgets do not imply equal total
training cost. Teacher summaries report taught cases, newly accepted samples,
accumulated policy samples and elapsed time. The per-generation `samples.npz`
files retain only that generation's accepted rows; the in-memory policy pool
uses their concatenation in generation order. Value rows remain current-cohort only.
Frozen-policy ablations must state which continuation policy their value labels measure.

`LeafReturns` samples pending neural queries with probability 1/64, bounded by
`--leaf-queries` (default 16) per generation. `leaf_rollouts` returns an independent
`PolicySearchBatch` with the exact queried state, including selection prefixes
and cumulative potion uses. It replaces the previous bare-policy continuation
owner; training and deployment use the same PUCT execution path. Continuation
search does not advance or install any result in the parent tree. Its separate
transition, inference and elapsed costs are recorded in `leaves-N/summary.json`;
parent collection time also includes synchronous leaf observation, so these two
times must not be added. Saved sources include the recipe, parent inputs, root
selection and search branches. `sample_index` addresses each query's exact
numerical row; `query` is true only at that continuation's first adopted decision.
Leaf and suffix action targets are zero: they train value, not policy imitation.
Only completed outcomes supply value targets. A single-role cohort is sampled
uniformly. New labels always come from this generation's frozen search actor.

`collect(..., max_seconds=N)` supports a shared cooperative wall allowance for
the declared cohort, including construction, simulation and neural inference.
It checks between search/evaluation batches, so one batch may overrun slightly.
An unfinished fight keeps its active/truncated kind and no terminal return;
time expiration never creates a loss label. Export time is outside this allowance.
Iteration checkpoints contain separate `policy` and `critic` state dictionaries
and their completed policy `generation`;
`warm.pt` contains the initial joint `model`. This loop does not load old checkpoints.
The two-network actor costs more inference than the joint initial
network; compare both search transitions and measured wall time. Constant-value
controls retain the same frozen action network and simulator terminal values.

Owners: [policy_search.rs](../../crates/sts_guided_search/src/policy_search.rs),
[native adapter](../../bindings/python_learning/src/policy.rs),
[runner](policy_iteration.py), and [entity network](entity_learning.py).
This path replaces neither the old whole-combat solver nor the real-run owner;
it makes the model/search learning loop independently executable.
