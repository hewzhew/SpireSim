# Atomic Exact Combat Search V2 Module Map

This directory owns the `AtomicExactV2` fixed-root engine. It is a maintained
diagnostic/challenger behind the dedicated fixed-root driver; it is not the
production resident `TurnGraphPortfolioV1`.

The resident oracle composes `LocalTurnGraphWitnessSession` and
`PolicyDiscrepancySession` from `sts_combat_planner` behind
`OracleResidentCombatWitnessJobV1`. See
[`docs/architecture/combat-search.md`](../../../docs/architecture/combat-search.md)
before comparing the engines or adding a shared option. Before adding an
atomic-v2 module, check this map and extend an existing boundary when one
already exists.

## Module Layer Contract

| Layer | Modules | Runner effect | Rule |
| --- | --- | --- | --- |
| Atomic exact kernel | `search/`, `frontier/`, `transition.rs`, `action_equivalence/`, `turn_local_dominance/` | Yes | May prune or schedule exact atomic-v2 branches only inside its documented safety boundary. |
| Deployable rescue/candidates | `turn_pool_rescue/`, replayable witness helpers | Yes, after replay/check | May propose a concrete line, but must keep its own engine, ranking, and report schema separate. |
| Estimate/value evidence | `value/`, `rollout/`, `rollout_*`, `pressure_value.rs`, `enemy_phase_value.rs` | Indirect | May order or estimate; must not claim a terminal result unless replayed by exact search. |
| Report-only diagnostics | `diagnostics/`, `decision_microscope/`, `turn_plan_probe*`, `trajectory_report.rs` | No | May explain behavior; must not become a strategy entrance. |
| Lab experiments | `line_lab/`, ad hoc binaries/tools | No | May compare lanes and probes; useful ideas must graduate into a named deployable module before runner use. |
| Legacy-risk / audit-first | large mixed files, old probes, one-off reports | No by default | Before extending, split boundaries or delete/retire unused parts. |

## Primary Entry Points

- `search.rs`: whole-combat search orchestration: root setup, main loop, action
  expansion handoff, and report finalization.
- `search/bootstrap.rs`: root search-node construction, root rollout estimate,
  initial frontier insertion, and optional root turn-plan seeding.
- `search/loop_state/`: mutable search-loop state ownership: frontier, stats,
  diagnostics, transposition/dominance tables, rollout cache, runtime plugin
  stack, and best-line candidates. Search-loop stages should read deployable
  search behavior from this plugin stack rather than reinterpreting
  `CombatSearchV2Config` policy fields.
  - `search/loop_state/frontier.rs`: frontier push/pop and pop timing.
  - `search/loop_state/counters.rs`: stop flags, node counters, and prune/cut
    counters.
  - `search/loop_state/trajectories.rs`: best frontier/complete/win/loss
    trajectory bookkeeping.
- `search/node_preflight.rs`: one frontier node to expansion, skip, or stop. It
  coordinates the node-stage gates below; do not add new gate logic directly
  here unless it is only wiring.
  - `search/node_budget.rs`: node and wall-clock budget admission.
  - `search/node_deferred_rollout.rs`: lazy child rollout completion when a
    deferred child is popped.
  - `search/node_terminal.rs`: terminal-node handling and complete-candidate
    acceptance.
  - `search/node_pruning.rs`: max-action, exact-transposition, and global
    dominance prune gates.
  - `search/turn_plan_seed_gate.rs` and `search/turn_plan_seeding.rs`:
    turn-boundary frontier seed admission and insertion.
- `search/node_expansion.rs`: one expandable node to an ordered action batch. It
  coordinates the action-surface and ordering stages below.
  - `search/node_action_surface.rs`: legal-action collection, potion-plugin
    filtering, and report-only diagnostics for the node action surface.
  - `search/node_action_ordering.rs`: local action equivalence compression,
    root action prior lookup, action ordering, and pending-choice ordering
    diagnostics.
  - `search/node_child_observers.rs`: per-node child observer initialization for
    turn branching and same-turn local dominance.
- `search/child_expansion.rs`: one ordered action to child disposition. It
  coordinates the child-stage pipeline below.
  - `search/child_preflight.rs`: per-child potion-plugin budget and deadline
    gates.
  - `search/child_step.rs`: apply one action through the combat stepper and
    record engine-step timing/limits.
  - `search/child_node.rs`: construct the child `SearchNode` and action trace.
  - `search/child_dominance.rs`: same-parent same-turn child dominance prune.
  - `search/child_rollout.rs`: terminal/deferred/immediate child rollout
    estimate admission.
  - `search/child_frontier.rs`: enqueue a child or remember a truncated leaf.
- `search/rollout_timing.rs`: shared rollout-estimate timing and attribution
  counters for root, child, deferred-child, and turn-plan seed estimates.
- `search/finalize.rs`: final report construction from loop state. It should
  assemble existing facts; avoid adding new search behavior here.
- `search/finish_diagnostics.rs`: post-loop diagnostics and timing finalization
  before report assembly.
  - `search/finish_coverage.rs`: finished-search coverage status and reason.
  - `search/finish_frontier.rs`: frontier sample extraction for reports.
  - `search/finish_policy.rs`: plugin/config-to-policy and budget report
    sections.
  - `search/finish_outcome.rs`: coverage outcome report section.
  - `search/finish_evidence.rs`: evidence reliability and warning section.
- `search/win_acceptance.rs`: stop/accept criteria for complete win candidates.
- `frontier/`: frontier queue, priority, `SearchNode`, and resource dominance
  vectors.
- `types/config/`: user-visible policy switches and prior hints. New
  experimental behavior should be named here before it affects search.
  - `types/config/options.rs`: `CombatSearchV2Config` and defaults.
  - `types/config/policies.rs`: policy enums, labels, serde aliases, and
    high-stakes potion budget helper.
  - `types/config/prior.rs`: root-action and turn-plan prior hint maps.
- `plugins.rs`: deployable search profile/plugin identity. Runner-facing
  profiles materialize config for compatibility, but runtime search stages
  should consume plugin ids for action prior, phase guard, frontier, rollout,
  turn-plan, potion, acceptance, and artifact behavior.
- `types/report/`: JSON report schema. Add fields only when a consumer uses
  them to make an implementation decision.

## Search Behavior Boundaries

- `transition.rs`: legal-action filtering and terminal classification wrappers.
- `action_facts/`, `action_effects/`, `action_priority/`: structured action
  semantics. Put reusable action knowledge here instead of embedding it in
  search, rollout, or reports.
  - `action_facts/`: action fact report assembly. Keep card facts,
    immediate/mechanics mapping, payload hints, target hints, and exact deltas
    in separate helpers.
  - `action_effects/types.rs`: card-play effect fact and diagnostic structs.
    Keep effect schema changes separate from effect observation logic.
  - `action_priority/play_card/`: play-card ordering entrypoint plus small
    setup and target helpers. Keep boss/setup/card-role hints out of the
    search loop.
- `action_ordering/`: action child-generation order only. It must not prune or
  merge legal actions.
  - `action_ordering/types.rs`: ordered action, result, and summary data shapes.
  - `action_ordering/summary.rs`: diagnostics-only summary aggregation.
  - `action_ordering/compare.rs`: priority and prior-score comparison.
- `phase_action_ordering/`: phase-specific ordering nudges. Keep input/output
  types, constants, and boss/phase rules separated; this layer must stay a
  nudge, not a second search policy.
- `action_equivalence/`: soundness-scoped local action-list deduplication only. Do
  not use it for global state merging.
- `turn_planner/`: exact same-turn enumeration and optional frontier seeding.
  Reuse this for turn-level macro candidates; do not create another turn-plan
  system.
  - `turn_planner/enumerate/mod.rs`: exact same-turn enumeration coordinator.
  - `turn_planner/enumerate/plan.rs`: turn-plan construction from terminal,
    next-turn, pending-choice, and engine-limit boundaries.
  - `turn_planner/enumerate/selection.rs`: bucket diversity selection and
    selection audit construction.
  - `turn_planner/enumerate/ranking.rs`: turn-plan candidate comparison and
    prior-score tie breaking.
  - `turn_planner/types/core.rs`: turn-plan config, plan records,
    enumeration counters, stop reasons, and bucket classification.
  - `turn_planner/types/coverage/bands.rs`: coverage-key fields, band enums,
    and stable labels.
  - `turn_planner/types/coverage/signature.rs`: coverage signature extraction
    from a candidate plan.
  - `turn_planner/types/selection.rs`: bucket/coverage selection audit schema.
  - `root_frontier_seed` seeds exact current-turn end states from the initial
    search root only.
  - `turn_boundary_frontier_seed` is opt-in and seeds exact current-turn end
    states whenever search reaches a new empty-prefix player-turn boundary,
    with exact source-key de-duplication. It does not prune atomic branches or
    create terminal outcome records by itself.
  - `tactical_enemy_turn_boundary_frontier_seed` is an opt-in experimental
    gated seed. It uses the same exact end-state seeding only at empty-prefix
    turn boundaries where typed enemy-mechanics facts show tactical multi-enemy
    pressure, such as a living Healer/support enemy with another living enemy or
    a Fungi Beast swarm. It should stay out of default hallway search until its
    budget and scheduling behavior are tuned enough not to crowd ordinary
    exact search.
- `turn_local_dominance/`: same-parent same-turn pruning only. Cross-turn or
  cross-parent dominance belongs in `frontier/` resource dominance, not here.

## Value And Rollout

- `value/`, `value.rs`, `value_facts.rs`, `pressure_value.rs`,
  `enemy_phase_value.rs`, `card_pile_value.rs`: state evaluation facts and
  ordering scores. These may guide frontier/rollout priority but do not prove
  terminal outcomes.
  - `value/combat_eval/types.rs`: estimate-ordering schema, labels, and public
    accessors.
  - `value/combat_eval/build.rs`: conversion from rollout estimates into
    estimate-ordering buckets and signals.
  - `value/combat_eval/ordering.rs`: ordering contract for wins, losses, and
    unresolved estimates.
- `rollout/`, `rollout_cache/`, `rollout_action_selector.rs`, `rollout_probe/`,
  `rollout_pending_choice.rs`, `rollout_scheduler.rs`, `rollout_value.rs`:
  estimate-only rollout behavior. Rollout output must remain labeled as
  estimate evidence unless replayed into an exact search node. The default
  `enemy_mechanics_adaptive_no_potion` rollout currently uses phase-aware
  rollout only for typed Guardian and Bronze Automaton mechanics and otherwise
  stays conservative.
  - `rollout/turn_beam/mod.rs`: turn-beam rollout public entry points and
    conservative-anchor wiring.
  - `rollout/turn_beam/extension.rs`: bounded turn-plan beam extension loop.
  - `rollout/turn_beam/selection.rs`: beam de-duplication, ranking, and
    estimate construction helpers.
  - `rollout/turn_beam/attribution.rs`: turn-plan attribution counters.
  - `rollout_cache/estimate.rs`: cache lookup, budget gates, policy dispatch,
    and rollout observation counters.
  - `rollout_cache/report.rs`: rollout report assembly only.
  - `rollout_cache/policy.rs`: adaptive policy selection and estimate
    comparison helpers.
- `rollout_probe/`: bounded one-step rollout action selection. This is
  behavior-affecting estimate code, not report-only diagnostics.
  - `rollout_probe/score.rs`: exact one-step probe transitions and score
    construction.
  - `rollout_probe/score_types.rs`: probe score ordering types.
  - `rollout_probe/upgrade.rs`: fallback-vs-candidate upgrade admission.
- `turn_pool_rescue/`: deployable no-win rescue candidate generation. It may
  produce a replay-checked line for run-control, so it is not a report-only lab
  module. Keep new rescue lanes here or in another explicitly deployable module,
  not in `line_lab.rs`.
  - `turn_pool_rescue/types.rs`: public report/win schema plus internal lane
    node types.
  - `turn_pool_rescue/engine.rs`: bounded lane expansion and combat stepping.
  - `turn_pool_rescue/ranking.rs`: lane ranking and report-line summaries.

## Game-Mechanics State Facts

- `phase_profile/`, `phase_action_ordering.rs`, `enemy_mechanics_profile/`,
  `enemy_phase_transition/`: enemy and phase facts used by value/ordering.
- `pending_choice_profile/`, `pending_choice_ordering/`,
  `pending_choice_fanout.rs`: pending-choice shape, ordering, and fanout risk.
- `potions/`: potion facts, proposal gates, and potion-specific tests. Potion
  policy should stay explicit and opt-in.

## Diagnostics And Report-Only Code

- `diagnostics/`, `diagnostics_tags.rs`, `target_fanout/`, `turn_sequence/`,
  `turn_sequence_effect/`, `discard_order_shadow_audit/`, `card_identity/`,
  `state_abstraction/`: observation, audit, and boundary classification.
  These modules must not remove exact branches unless the boundary is promoted
  to a prune-safe consumer.
- `decision_microscope/`: opt-in analysis tool. Do not route normal search
  behavior through it.
  - `decision_microscope/candidate_probe.rs`: exact one-step candidate
    diagnostics for the initial action surface.
  - `decision_microscope/report.rs`: selected-action, trajectory, and config
    report mapping only.
- `line_lab/`: opt-in combat-line review and cut repair reports. It may
  include `turn_pool_rescue` evidence in its report, but runner behavior must
  call the deployable rescue module directly rather than depending on lab code.
  - `line_lab/types.rs`: report schema and small replay/cut data structs.
  - `line_lab/cuts.rs`: cut-point selection from an existing parent line.
  - `line_lab/replay.rs`: exact action replay helpers for diagnostics.
  - `line_lab/repair.rs`: suffix repair search and repair ranking.
- `turn_plan_probe/` and `turn_plan_probe_report.rs`: opt-in exact
  same-turn probe enumeration and its JSON schema. Keep report type growth in
  the report file so the probe file stays focused on enumeration and mapping.
  - `turn_plan_probe/mod.rs`: bounded root probe orchestration.
  - `turn_plan_probe/candidate_report.rs`: selected turn-plan report rows.
  - `turn_plan_probe/action_mask.rs`: complete root action-mask report.
  - `turn_plan_probe/selection_audit.rs`: bucket/coverage selection audit
    mapping.
- `trajectory_report.rs` and `baseline.rs`: whole-combat outcome reporting and
  baseline comparison. Baselines are comparison evidence, not teacher labels.

## Rules To Avoid Duplicate Systems

1. If the work is about full-turn candidates, start in `turn_planner/`.
2. If the work is about child order, start in `action_ordering/` or
   `phase_action_ordering.rs`.
3. If the work is about pruning, identify the safe-pruning boundary first:
   `action_equivalence/`, `frontier/`, or `turn_local_dominance/`.
4. If the work is about estimates, use `value/` or `rollout/`; only the bounded
   exact-replay bridge in `search/rollout_terminal_promotion.rs` may convert a
   complete terminal witness into a terminal outcome.
5. If the work only explains behavior, it belongs in diagnostics and needs a
   concrete consumer before adding more report fields.
6. If the work changes the search-loop lifecycle, start in `search/` and place
   the logic in the narrowest stage module. `search.rs`, `node_preflight.rs`,
   and `child_expansion.rs` should stay as coordinators.
7. New top-level files in this directory should be rare. Prefer extending the
   nearest existing submodule and updating this map.
