# Combat Search Ownership

Status: targeted Rust-search implementation reference. This page does not own
the Java/Rust parity workflow and is not part of the default documentation
reading sequence.

This is the canonical identity and ownership map for current combat witness
engines. Cross-layer evidence rules live in
[`../ARCHITECTURE.md`](../ARCHITECTURE.md), while
operational combat recipes live in
[`../runbooks/combat-evidence.md`](../runbooks/combat-evidence.md). Do not infer
authority from a version suffix, directory size, CLI name, or the word
"planner".

The independent whole-combat solver and its offline adapter are described in
[the experiment entry point](../../tools/guided_search/README.md#无模型整场求解).
It uses shared simulation and outcome facts, without adopting these engines,
their scores, complete-turn candidate generation, or cached results. Retained
learning components are indexed in [model-guided search](model-guided-search.md).
This page documents maintained implementation, not requirements that a
replacement must preserve.

## Capability Boundary

The maintained complete-turn generator is an exact enumeration and settlement
primitive. Its `complete` coverage describes the configured legal-generation
surface; it does not mean that the returned order is strategically sufficient,
that a long encounter is solved, or that a bounded missing witness is negative
evidence. Atomic and resident witness engines likewise remain proposal/evidence
producers until a separate owner supplies cross-turn hypotheses, resource
Pareto comparison, and replay-exact adoption.

The experimental `public_actor_census --actor rule-search` is a separate
public-information task actor. Strategic choices use the existing deterministic
rules. Its combat fallback searches combinations of currently visible direct
damage/block cards, grouped by hand occurrence and constrained by current
energy. When a visible attack trigger makes order matter, it also tries card
orders within the same work limit. It adopts only a visible immediate kill or
a rescue from a visible lethal attack. Unknown card effects, draws, future
intents and hidden RNG are not simulated. The actor reobserves after every
committed action and otherwise uses the existing public rule fallback. A bounded
or inapplicable local search does not assert that no better line exists. This
actor is neither an exact combat witness engine nor a source of teacher labels.

Accordingly, an analyst must not assume that a short `Tn` list contains the
best line or choose from it merely because the list is typed and exact. Human
or model strategy may propose a small number of explicit hypotheses; search
may share a tree to compare or challenge them; legality and final replay remain
exact. No existing engine has authority to replace that strategic judgment or
to emit teacher labels.

Atomic search retains observed complete wins through the shared materialized
combat-outcome owner. Resource equality includes which starting potions were
spent, generated potions, persistent relic state, master-deck changes, and
pending rewards. Equal HP and equal potion counts do not merge distinct potion
reserves. Action count is a tie-break only after materialized resources are
equal; it cannot erase a persistent-resource choice. This frontier concerns
already observed wins, not search completeness or future run value.
The run-control atomic adapter independently replays and materializes all of
these choices, removes only resource-equivalent or dominated results, and
publishes the remaining public plans. A lane representative supports scheduling
without replacing its candidate set.

## Existing Solver Workflow

Models and routine full-run operators use one hot `sts_run_play agent-shell`
workflow:

```text
combat_research -> combat_review -> commit_combat_plan
```

This is a resumable automated-solver surface, not the default owner of analyst
judgment. Engine and lane names are internal provenance: ordinary callers do
not choose AtomicExact, turn graph, proposal beam, policy discrepancy, or a
milestone producer. A status read resumes no work, a review expands only a
stable short candidate ID, and a commit accepts only a replay-exact complete
witness. Elite and Boss combat cannot grant Java commit authority to a
next-turn segment. A budgeted missing witness is still unknown rather than
evidence that one analyst choice is worse.

## Analyst Draft Core

`CombatDraftSession` in `sts_combat_planner` is the small Rust-only owner for
turn-at-a-time analyst planning. It retains one complete-turn generator per
explored turn frame. The analyst advances only the current generator, inspects
short frame-scoped handles such as `T0`, selects one exact-replayed turn, and
may undo to the unchanged parent generator. Selecting the same parent option
again resumes its retained child, including candidates, pending work, budgets,
selection advice and deeper explored branches. Different parent options retain
separate children even at equal depth or equal-looking successor states.

Only explicitly chosen options create child frames; generation does not create
a new draft branch for every candidate. Retained frames live for the draft's
process-local lifetime. They are not durable checkpoints and are released with
the draft; switching never silently evicts another branch. Re-entering a child
still exact-replays the selected parent turn before activating it. Replay failure
preserves the current path and all retained work. Victory export follows only
the active selected path and replays from the unchanged first root.

An existing exact candidate can seed a draft through `from_exact_prefix`.
The core validates each recorded successor and requires the final transition
to reach a new player turn. The retained prefix is immutable and undo stops at
that editing boundary. It remains part of the original-root victory replay;
the adapter does not own a second trajectory or grant a suffix commit authority.
An empty prefix starts at the original decision, including its remaining turn.

The draft does not launch Java, mutate a run head, run an independent full
suffix search for every alternative, or require a whole low-level action line
up front. Loss and Smoke Bomb escape remain terminal drafts but never become
victory witnesses. Only a complete selected line that replays from the
unchanged first root to exact non-escape victory may be exported as a witness.
Generator coverage and witness existence remain separate facts. Automated
search may later challenge or repair a draft through an explicit adapter; it
does not own the draft's strategic choice.

Interactive whole-combat research carries one process-local deadline through
the run-control composer and each enabled producer. Standard and expanded
passes share that deadline. A later lane cannot reset the caller's budget;
expired admission records zero work and `DeadlineReached`. Already verified
witnesses remain available when refinement stops, while a bounded miss stays
Unknown. This deadline is separate from deterministic work limits and from
transport timeouts; witness materialization still completes before publication.
Whole-combat publication retains the public steps and transition results from
that verified replay for the settled resource frontier. The run-play adapter
builds review traces from those retained steps rather than executing the same
line again. Candidate facts and exact inputs remain available, including
dominated results; detailed replay payloads are process-local and may be dropped
when another fully settled result strictly dominates them under the same visible
resource contract. Commit still replays the adopted plan at its bound live root.

Before exact option generation, the planner's automatic action surface uses
the same conservative action-equivalence contract as `AtomicExactV2`. For card
plays it only collapses adjacent equal starter instances, including lineage;
non-starter plays and attribute-identical distinct instances remain separate.
Removing separated equal cards can change the remaining hand order, automatic
discard order and fixed-RNG shuffle. Single-card selections retain the selected
lineage too. The simulator's legal action surface is unchanged; this narrow
reduction is not an equivalence between discard orders or complete turn plans.

Multi-card selections retain an ordered, lazy cursor. One service reserves a
full engine-transition allowance and settles its current member before advancing
the cursor. A timeout retains the same member for retry. Remaining members keep
uniform conditional mass and the cursor retains their aggregate scheduling bound;
it does not allocate a separate pending transition for every permutation. Exact
selection order and complete family coverage remain unchanged. This scheduling
contract does not itself provide a preference over which cards to select.

`CombatActionPolicy::structured_selection_preferences` can name at most four
concrete members of the current multi-selection family. The generator checks
legality, encoding, cardinality, distinct source identities and order. Valid
unique suggestions are partitioned from the lazy remainder, so they receive
neither duplicate execution nor duplicate probability. Single-card families keep
their existing member-weight path. Empty or invalid suggestions are neutral.

For a family of size `N`, every member retains `epsilon / N` conditional mass.
The preferred members share the remaining `1 - epsilon` according to the existing
member-weight hook. `epsilon` uses the configured uniform exploration fraction,
with a one-ppm floor when preferences are supplied; a fully uniform configuration
ignores preferences. Both cursors share the same work and engine-step budgets,
timeout behavior and exact replay. This is an optional in-process policy contract,
not a persistent plan or new gameplay frontend. A policy retaining an external
intention must bind it to its source position; cached inputs alone cannot establish
that an intention is still appropriate. Runtime policy decorators forward this
contract, but no general card-selection heuristic is enabled by default.

The generator owns one editable `selection_intent`, which can focus on the
root or on a retained intermediate selection reached during the same turn.
An opaque `SelectionSource` binds the generator instance and its retained exact
prefix, sharing the immutable position and trace. Equal-looking tables do not
establish equal sources; a new generator cannot accept a source from an old one,
even at the same exact root. Up to four legal structured selections can be
replaced atomically or withdrawn with an empty list.

`selection_sources` reads the currently retained selection positions, including
the root when selectable. It deduplicates shared prefixes and returns a bounded
page without generation or replay. It does not maintain a historical state bank
or recover already-merged ancestry. An observed source remains inspectable for
that generator's lifetime even after its outgoing work has been consumed;
`selection_continuation_work` reports remaining retained work without implying
that a missing continuation is impossible.

An intention adds a scheduling view over the same retained work. It partitions
only unexecuted members at its exact source and follows retained descendants of
those selected inputs. Changing focus withdraws the previous focus. Existing
policy probabilities, completed options, visited states, and used/granted budgets
remain unchanged. Withdrawn pending members remain ordinary work; they are
neither discarded nor replayed from the start. Each service round retains its
ordinary frontier heads alongside the intention head. Editing grants no engine
work and cannot revoke frozen fallback service. Exact-state transpositions still
share continuations; advice does not reconstruct merged paths or promise a result
within a bounded budget.

`CombatDraftSession` and `CombatDraftEnvV1` forward this same control. A new child
starts without the parent's intention; a revisited child retains its own.
Undo restores the retained parent, intention and option handles. There is no
durable intention format.

`sts_complete_turn_view` owns the read-only presentation adapter. Its turn
index shows the current public state plus frame-local `Tn` handles, first
semantic action, action count, and boundary. Repeated index fields are
normalized without loss: one `FIRST` heading owns buckets keyed by action count
and boundary, and every exact `Tn` remains listed in exactly one bucket. This is
presentation grouping, not candidate pruning or strategic ranking. The index
does not replay and inline every successor. Inspecting one `Tn` exact-replays
only that candidate and renders nested `ACTION A0/A1/...` lines followed by its
public `NEXT` state. The turn index also identifies its process-local frame
(`F0`, `F1`, ...) and lists explored children with their existing candidate count,
consumed work and coverage. These are progress facts, not branch rankings or
promises of a winning continuation. Neither surface emits exact hashes, raw
`ClientInput`, search lanes, or a JSON envelope.

`CombatDraftEnvV1` in `sts_complete_turn_env` is the thin in-process lifecycle
adapter over that same session and view. `advance` forwards exactly one
caller-supplied quantum; `status`, `observe`, and `undo` perform no search;
`inspect` replays only its named option. Programmatic callers retain typed
frame identity, while canonical `T0`, `T1`, ... labels are current-frame
human conveniences. The adapter owns no second combat state and is not a CLI,
MCP service, Java bridge, or additional search engine.

The existing `sts_run_play agent-shell` is the sole live run adapter for this
draft. It exposes `combat_draft_status`, `combat_draft_advance`,
`combat_draft_inspect`, `combat_draft_choose`, `combat_draft_undo`, and
`combat_draft_selection` without
adding another process or durable session. `advance` creates the draft when
needed and consumes exactly the one explicit deterministic quantum in its
request. The other operations never resume generation. Inspect and choose
require `frame_ref` from the displayed frame alongside `option_ref`, for example
`{"frame_ref":"F2","option_ref":"T3"}`. Depth remains descriptive state and
cannot authorize a label: two sibling branches can be at the same turn. Frame
identities remain stable when revisited and do not collide across drafts in the
same process. They expire with the shell connection; a different active frame
rejects the request before inspection or choice. No exact hash is exposed.
Active draft replies also return `frame_ref` as a structured field, so callers
can forward it without parsing the rendered text. An absent or terminal draft
has no current frame reference.

`combat_draft_selection` shows an immediate root selection when present;
otherwise it lists a page of retained intermediate selections. An explicit
`offset: 0` requests the index even at a selectable root; pages contain up to
eight sources and an optional `next_offset`. The compact index shows a source
such as `S0`, prefix length, first public action and selection reason. It does
not enumerate combinations or print full states.

`source_ref: "S0"` expands the named source, exact-replaying only that retained
prefix through the shared action projector, then showing the public state and
selection table. The table preserves individual occurrences, order, duplicate
cards and ineligible entries. Both display and resolution use the generator's
admission surface. Exhausted retained work is reported as `Remaining work: 0`;
that is not a completeness or negative-reachability claim.

The same command accepts a structured `edit`, for example
`{"source_ref":"S0","preferences":[{"family":0,"choices":[1,2]}]}`.
A source read also lists already-generated complete turns through that selection.
The planner associates candidates using the full exact action prefix and the
following structured selection, not a matching card name or later state alone.
`Tn` handles keep their existing frame identity and generation order. Rows show
the selected occurrences, whether they match the current advice, boundary,
player HP/block, enemy HP and action count. These are observed facts; the view
keeps all matching candidates and does not rank or merge resource outcomes.

`source_ref: "S0", offset: 0` reads the first outcome page without replaying the
prefix. Outcome pages contain at most eight candidates and use `next_offset`;
without `source_ref`, `offset` continues to page the source index. Changing or
withdrawing advice updates its displayed match, never the retained membership.
An empty outcome list means none has been observed on that exact retained path.

Edits are separate from both read fields. They resolve all members before
mutation and return the updated selection table; they neither generate nor
replay. `preferences: []` withdraws the one active focus, whereas one member
with `choices: []` proposes a zero-card selection when permitted. There is no
natural-language parser or name-based card matching.

Source names bind exact core sources within one shell connection. The shell
retains at most 32 recently observed bindings; evicted names are rejected and
never recycled. Search advances do not rebind names. Choosing, undoing or
invalidating a draft expires all transport bindings. Returning to a retained
parent requires a fresh observation while preserving search results and advice.
An ordinary root with no retained selections reports `Selection: none`. A draft
can be initialized without searching using an explicit zero quantum through
`combat_draft_advance`. No new CLI process, MCP state owner or persistent source
format is introduced.

The shell retains one process-local draft for the unchanged durable head and
exact combat root. Any durable mutation drops it; an exact-root mismatch also
invalidates it before an option can be inspected or selected. A non-winning,
escaping, or losing draft has no write path; incomplete coverage and mechanics
gaps remain visible and grant no authority of their own. An exact terminal
victory is independently reprojected through the public
run-play surface and installed as `c0` in the ordinary combat-research cache;
`commit_combat_plan` remains the only whole-combat transaction that can move
the durable head. Draft responses carry the small text manifest plus bounded
work counters, never exact hashes, raw `ClientInput`, or a second full-state
JSON projection.

`combat_research` is the only routine search entry. It owns the bounded
portfolio schedule and publishes a non-dominated resource frontier with stable
content IDs. Engine names, lane schedules, artifact paths, and artifact-local
array positions are provenance or diagnostics; callers do not choose among
them to begin an ordinary fight.

`combat_review` accepts only IDs from the cached frontier at the unchanged
`(head revision, effort)` root. Its default projection is turn-level: starting
hand and effective card values, enemy HP/block/intent/powers, semantic actions,
the real incoming damage and player block immediately before End Turn, and
observed HP/max-HP change. Per-action deltas are opt-in with
`include_steps: true`; complete trace JSON remains available to offline
artifacts. Review performs no search and cannot mutate the durable head.

`commit_combat_plan` is the only routine terminal-plan write. It names the same
stable candidate ID, replay-prepares the cached typed witness against the live
exact root, and publishes transactionally. Any head mutation invalidates the
cache. A bounded research miss remains Unknown.

Milestone, query/challenge, atomic fixed-root, trace, and producer-specific
surfaces remain available for explicitly diagnosed mechanics, partial witness,
or recovery work. They are not parallel everyday workflows and must not be
selected merely because an older transcript used one of their names.

## There Is No Single "Highest" Search

The repository keeps two exact witness-engine families because they answer different
questions:

| Engine identity | Search unit | Configuration owner | Orchestration owner | Maintained role |
| --- | --- | --- | --- | --- |
| `AtomicExactV2` | One legal atomic combat action | `CombatSearchV2Config` and `AtomicCombatSearchOptionsV2` | Fixed-root driver | Fixed-root diagnostic, challenger, and benchmark search |
| `TurnGraphPortfolioV1` | Exact complete-turn boundaries plus policy-discrepancy trajectories | `OracleCombatWitnessOptionsV1` | `OracleResidentCombatWitnessJobV1`, backed privately by `OracleRunCombatWitnessWorkV1` | Current resident oracle witness producer used by run exploration and analysis |

`AtomicExactV2` is not a lower-quality alias for the resident portfolio, and
the resident portfolio is not a profile of `combat_search_v2`. Results are
comparable only when they share the same exact root, legality/potion contract,
acceptance target, work accounting, and replay verification.

## Production Resident Flow

```text
Oracle run / analysis owner
  -> OracleCombatWitnessOptionsV1
  -> OracleResidentCombatWitnessJobV1
  -> private OracleRunCombatWitnessWorkV1 portfolio scheduler
       -> LocalTurnGraphWitnessSession
       -> PolicyDiscrepancySession
  -> typed portfolio evidence + replay-exact incumbent
  -> run-control acceptance
  -> atomic combat-resolution transaction
```

`LocalTurnGraphWitnessSession` lazily generates exact complete-turn options and
shares exact boundary states among its anchor, proposal, and guide services.
`PolicyDiscrepancySession` explores atomic trajectories by accumulated
departure from the same typed action policy. `OracleRunCombatWitnessWorkV1` decides
which member receives the next bounded service grant and retains the best
verified incumbent under the run-control contract.

`combat_research_plan_v1` is the run-control composer, not another search
kernel. Resident, milestone-closure, proposal-beam, and atomic challengers
enter it through typed lane adapters. Producer order does not imply outcome
quality or resource preference. The resident adapter keeps session state,
budget accounting, potion contract, incumbent, and evidence in separate
constrained state groups; only their methods may grant or debit work, change
the admitted potion stage, or update replay-verified incumbents.

Milestone closure does not redistribute resident work. When the encounter
strategy owner says that an already-retained exact root can prove its next
typed milestone in one complete player turn, the graph exports a replayable
root path and read-only plan facts. Run control ranks a bounded number of those
unserviced roots, gives each a separately metered complete-turn generation
allowance, and replay-materializes any matching successor. Its work unit is
`MilestoneClosureGenerationWork`; its successful exit is
`MilestoneCandidates`, never `Witness`. A milestone candidate must end in an
unresolved combat, is excluded from the terminal resource frontier, and may be
published only through the separate transactional milestone commit. A head
change then requires fresh whole-combat research.

The agent transport does not inline a full public combat state into every
milestone lineage. Research returns all content-addressed lineage summaries
with a small set of public successor facts sufficient to expose immediate
damage, block-source, status, and enemy-Strength differences. Full successor
previews are opt-in for controllers that need them; selected lineages can be
expanded without search through `combat_milestone_successor_details`.
Publicly equivalent lineages remain distinct because their hidden exact
continuations can differ.
After bounded shortlisting, `combat_milestone_successor_details` expands at
most 16 selected lineage IDs from the same `(head revision, effort)` cache;
this is a read-only projection, not another search. A snapshot ID mapping to
unequal public states is a protocol error. Any head change invalidates both
the index and its detail surface.

The same selected IDs may receive a one-turn
`combat_milestone_continuation_audit`. Each parent prefix is replayed on a
private exact fork, the maintained bounded complete-turn generator runs once,
and only successors that prove a newly completed typed milestone or terminal
victory are returned. A witness is replay-exact through that next boundary;
absence is `unknown_in_bounded_surface`, never Loss. The audit cannot commit or
change the durable head.

The two member sessions live in `sts_combat_planner`. That crate owns the exact
complete-turn generator and solver kernels; it does not own campaign budgets,
potion-stage promotion, run continuation, or final application. Those belong
to run control.

## Tactical Guidance Boundary

`sts_combat_atomic` is the active compilation owner for `AtomicExactV2`.
The retained tactical adapter remains temporarily co-located because its
mature evaluation still depends on atomic-private semantic types.
`sts_combat_outcome` does not depend on this search owner; it imports only
the evidence contracts needed to materialize outcomes. The role-named package
removes the misleading retirement label, but does not by itself complete the
tactical-evaluation migration.

`sts_combat_tactics` owns the neutral `CombatActionPolicy` contract, shared
policy handle, guide rank, stable guide-lane identity, and typed guide-service
bias. It depends on simulator types but neither atomic search nor the planner.

`sts_combat_atomic::RetainedCombatTacticalPolicyV1` adapts the mature shared
tactical implementation to that contract. `sts_combat_knowledge` owns
mechanics-only tactical state facts, the typed state-value ordering, all five
guide-lane projections, encounter-selected action-ranking context, policy
decorators and typed bias, plus the neutral tactical action-rank coordinates,
their comparison contract, the engine phases in which ranking applies, and the
read-only hand/next-draw/Velvet Choker facts and visible incoming-damage pressure
used by tactical state value. Knowledge also owns visible monster-attack
relevance and the realized negative-Strength mitigation derived from it, shared
by state value and action-effect scoring, plus the phase debt for Split,
Awakened One rebirth, Time Eater Haste, and Guardian defensive block. Atomic
consumers also reuse knowledge's typed timed-enemy-threat facts; atomic keeps
the public action/report projection. Atomic still calculates action-rank
coordinates and the remaining complex enemy-mechanics profile, because those
calculations, retaliation projection, and complete potion gating still use
atomic-private semantic helpers and remain migration work. The shared formal
policy-weight path consumes only the neutral rank; atomic-private roles and
effect details remain available through its diagnostics surface.

Pending-choice fanout remains atomic for a different reason: its current
`estimated_action_fanout` calculation stops once it has proved the atomic
rollout cutoff of 64, and the same module assigns policy classes such as
`small`, `medium`, and `large`. Structural combinatorics must be separated from
that rollout cutoff before any fanout fact moves to knowledge; moving the file
as-is would falsely present a budget policy as mechanics knowledge. In particular,
`sts_combat_knowledge` has no dependency on `sts_combat_atomic` or
`sts_combat_planner`; run control explicitly composes the retained base with
optional decorators before constructing a session.

Guidance may reorder work, but it cannot remove legal actions, assert closure,
accept a terminal, or change exact replay. Stable lane IDs live with the
neutral contract so an independently compiled bias cannot silently target a
different guide after a module or crate migration.

## Atomic Exact V2 Flow

```text
fixed CombatPosition / CombatCase
  -> AtomicCombatSearchOptionsV2
  -> CombatSearchV2Config
  -> CombatSearchV2Session
  -> exact atomic-action report
  -> replay/adjudication adapter
  -> optional atomic combat-resolution transaction
```

The implementation lives under `src/ai/combat_search_v2`. Its rollout,
turn-plan seeding, frontier plugins, priority ablations, and report controls
are engine-private. The dependency-light `sts_combat_search_driver` frontend
and optimized worker expose this engine for fixed-root work. They do not invoke
the resident portfolio.

The retained tactical action adapter and transitional value-fact bridge remain
physically separated from the atomic rollout in `oracle_action_policy/shared.rs`
and `oracle_action_policy/rollout.rs`; state-value ordering and guide projection
live in `sts_combat_knowledge`. Planner consumers never import the rollout or
atomic search kernel.

The `turn_planner/` directory inside `combat_search_v2` is an atomic-v2 macro
candidate generator. It is not `LocalTurnGraphWitnessSession` and does not
make atomic-v2 the production resident witness engine.

## Configuration Boundary

Only controls an engine reads may appear in its request type.

`OracleCombatWitnessOptionsV1` owns:

- complete-turn generation work;
- maximum engine steps per generated transition;
- wall allowance and satisfaction;
- potion admission, slot mask, and discard admission.

It must not accept or inherit atomic-v2 rollout, turn-plan, frontier, plugin,
priority-ablation, or node-budget fields. In particular, resident construction
does not inherit the older `RunControlSession.search_*` atomic tuning fields.

`AtomicCombatSearchOptionsV2` owns the fixed-root atomic adapter and may project
the full `CombatSearchV2Config` surface. Production resident code must not
import that type or the `combat_search_v2` implementation namespace.

Shared vocabulary lives in `src/ai/combat_witness_contract.rs`:

- `CombatWitnessEngineV1` gives durable engine identity;
- `CombatWitnessSatisfactionV1` names cross-owner acceptance requests;
- `CombatWitnessPotionPolicyV1` names potion action admission;
- the high-stakes potion default is owner policy shared by both engines, not
  an atomic-v2 capability.

The materialized persistent-payoff compatibility score lives with
`CombatPersistentOutcomeV1`. It preserves already-realized max HP, recovered
gold, and card growth comparisons. It is explicitly not a prediction of
future run value.

## Evidence Boundary

New action trajectories must identify their producer as `AtomicExactV2` or
`TurnGraphPortfolioV1`. `SearchCombat` is read-only legacy provenance and new
code must not emit it.

Smoke Bomb escape is a distinct completed search outcome, not combat victory.
The engine may expose a post-combat `Win` terminal after leaving the room, so
all witness owners and replay boundaries require both that terminal and
`combat_smoked == false` before admitting an exact win. Escape may terminate a
search branch and appear in diagnostics, but it cannot enter a winning
frontier, materialized combat plan, or durable combat-resolution transaction.

Engine identity does not make a trajectory a teacher label. A search report,
frontier sample, rollout estimate, stage trace, or accepted action sequence is
evidence with its own root and budget contract. Learning code may consume it
only through an explicitly designed target contract.

No current engine is the learned agent or an authoritative teacher. Under the
current [lockstep direction](../ARCHITECTURE.md), search remains an isolated
proposal/evidence producer. Java follows only the selected committed
trajectory; a best witness does not become a committed successor or a learning
label without an explicit execution owner.

## Learning Evidence Audit

The current engines preserve enough information to prove winning trajectories,
but not enough to reinterpret their bounded search statistics as an AlphaZero
policy target.

| Kernel | Retained ancestry | Transposition behavior | Terminal evidence | Outcome backup | Bounded unknown | Valid current learning use |
| --- | --- | --- | --- | --- | --- | --- |
| `LocalTurnGraphWitnessSession` | Each exact node has one replaceable `diagnostic_parent`; the graph retains shared outgoing edges, not every incoming path as terminal provenance | Equal constrained exact boundaries share one node | A replay-exact non-dominated winning witness frontier plus aggregate terminal-option counters | `backup_guides_along_path` propagates cheap guide ranks only; visits are service counts, not value visits | Session-level `Partial`, `FrontierExhausted`, `MechanicsGap`, or replay failure; no closed per-root-action result | Verified solution trajectories, proposal coverage, and search-work diagnostics |
| `PolicyDiscrepancySession` | Every queued work item owns one persistent action trace | `best_state_discrepancy` keeps only the lowest-discrepancy arrival at an exact state | One replay-exact winning witness; search stops when it is found | None; losses are discarded and no descendant value is returned to earlier actions | `Partial` and `FrontierExhausted` are distinct, but there is no per-action closure | Verified solution trajectory and discrepancy/search-effort diagnostics |
| `CombatSearchV2Session` (`AtomicExactV2`) | Every node owns its full action vector and one root lineage | Exact and dominance tables prune covered unresolved arrivals, so a merged future is not retained once per incoming lineage | Non-dominated exact win candidates and best observed complete/win values per retained root lineage | None; root work counts and best observations are not edge `N/W/Q` backup | Coverage status is explicit; root closure remains `NotProven` with `ProofDispositionsUnavailable` | Fixed-root challenger, exact winning frontier, and search-work diagnostics |

The atomic terminal gate runs before transposition pruning, so a terminal node
that is actually reached is recorded. This does not repair ancestry lost when
two unresolved paths merge earlier: later descendants are credited only to the
retained lineage. Likewise, `LocalTurnGraph` descendant reachability is useful
diagnosis but its terminal options are not graph nodes and are not backed to all
ancestors.

A frozen continuation may still be used to diagnose whether one candidate is
compatible with that exact carrier. It must be named continuation-relative and
cannot supply policy-independent positive/negative labels. A budget-limited
missing solution remains unknown.

Search amortization is a separately named solver-efficiency project, not the
current complete-run strategy-learning loop. Replay-exact winning paths may
eventually train a proposal prior for an anytime exact solver, but better
work-to-win or fixed-budget combat coverage would prove only a faster
privileged solver. It would not by itself improve an A0 run actor, assign
strategic value, or make incomplete-run combat roots a curriculum. Turning
visits or root observations into a policy target still requires a later
search owner with explicit edge statistics, terminal backup along every
selected ancestry, and principled sampled-action semantics.

This boundary is consistent with the primary literature, but does not claim
that the repository already implements those algorithms:

- [AlphaGo Zero](https://www.nature.com/articles/nature24270) trains a network
  to predict the search actor's own move selections and game winner, then uses
  that network to strengthen the next search iteration.
- [Expert Iteration](https://papers.nips.cc/paper_files/paper/2017/hash/d8e1344e27a5b08cdfd5d027d9b8d6de-Abstract.html)
  explicitly separates planning from generalization and feeds the learned
  policy back into the planner.
- [MuZero](https://www.nature.com/articles/s41586-020-03051-4) learns the
  reward, policy, and value predictions needed for planning when environment
  dynamics are unknown. This repository already owns an exact simulator, so a
  learned dynamics model is not the first missing capability.
- [Gumbel AlphaZero](https://iclr.cc/virtual/2022/spotlight/6419) states that
  ordinary AlphaZero can fail to improve when root actions are not all visited;
  its small-budget repair samples root actions without replacement.
- [Sampled MuZero](https://proceedings.mlr.press/v139/hubert21a.html) treats
  improvement over sampled action subsets as a separate, principled policy
  iteration problem rather than declaring unsampled actions negative.
- [Levin Tree Search with Context Models](https://deepmind.google/research/publications/21589/)
  provides a distinct precedent for learning a policy whose declared objective
  is lower solution-search effort.

## Where To Start

- Resident oracle scheduling or potion stages: `oracle_combat_witness_work.rs`,
  `oracle_run_explorer.rs`, and `OracleCombatWitnessOptionsV1`.
- Complete-turn generation or local/discrepancy kernels:
  `crates/sts_combat_planner`.
- Fixed-root atomic search behavior: `src/ai/combat_search_v2`.
- Fixed-root CLI performance: `crates/sts_combat_search_driver` and `cs.ps1`.
- Result application and rejection: the engine-named run-control adapter, then
  the shared combat-resolution transaction.
- Learned-agent work: start with the public-belief design; do not add belief,
  replay, or teacher authority to either witness engine.

When a new control appears, first name which row in the engine table consumes
it. If neither engine reads it, it is not a supported search option.
