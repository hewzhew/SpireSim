//! Pure JSON projections for completed local combat graph searches.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use sts_combat_planner::{
    LocalTurnGraphPolicyLineReport, LocalTurnGraphStorageSnapshot, LocalTurnGraphWitnessReport,
    OracleCombatWitnessProgressSnapshot, OracleCombatWitnessSatisfaction,
};
use sts_complete_turn_view::CompleteTurnCandidateViewV1;

use super::combat_graph_diagnostics::LocalGraphDiagnostics;
use super::combat_graph_execution::LocalGraphExecutionProfile;
use super::combat_graph_exports::LocalGraphExports;
use super::combat_graph_observation::LocalGraphObservation;
use super::combat_graph_search_spec::LocalGraphSearchSpec;
use super::combat_trace_view::{compact_combat_trace, compact_local_corridor_report};

#[derive(Clone, Copy)]
pub(super) struct LocalGraphCounterfactual {
    pub(super) full_health: bool,
    pub(super) original_hp: i32,
    pub(super) search_hp: i32,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalGraphRootTurnContinuationV1 {
    pub(super) candidate_id: String,
    pub(super) forced_action_count: usize,
    pub(super) candidate: CompleteTurnCandidateViewV1,
}

#[derive(Clone, Copy)]
pub(super) struct LocalGraphRunIdentity<'a> {
    pub(super) case: &'a Path,
    pub(super) elapsed: Duration,
    pub(super) satisfaction: OracleCombatWitnessSatisfaction,
    pub(super) execution_profile: LocalGraphExecutionProfile,
    pub(super) search_spec: LocalGraphSearchSpec,
    pub(super) counterfactual: LocalGraphCounterfactual,
    pub(super) root_turn_continuation: Option<&'a LocalGraphRootTurnContinuationV1>,
}

pub(super) struct LocalGraphReportData<'a> {
    pub(super) run: LocalGraphRunIdentity<'a>,
    pub(super) report: &'a LocalTurnGraphWitnessReport,
    pub(super) progress: &'a OracleCombatWitnessProgressSnapshot,
    pub(super) retained_state_work: usize,
    pub(super) storage: LocalTurnGraphStorageSnapshot,
    pub(super) policy_line: Option<&'a LocalTurnGraphPolicyLineReport>,
    pub(super) plan_transition_annotations: bool,
    pub(super) plan_transition_portfolio: &'a Value,
    pub(super) diagnostics: &'a LocalGraphDiagnostics,
    pub(super) observation: &'a LocalGraphObservation,
    pub(super) exports: &'a LocalGraphExports,
}

pub(super) struct LocalGraphFullReportOptions<'a> {
    pub(super) action_imitation_artifact: Option<&'a Path>,
    pub(super) value_prototype_artifact: Option<&'a Path>,
    pub(super) guidance_bundle: Option<&'a Path>,
    pub(super) watch_corridor_actions: &'a [PathBuf],
    pub(super) readable: bool,
    pub(super) search_elapsed: Duration,
    pub(super) performance_timing: &'a Value,
    pub(super) performance_profile: &'a Value,
}

pub(super) fn local_graph_trace_report(data: &LocalGraphReportData<'_>) -> Value {
    let forced_action_count = data
        .run
        .root_turn_continuation
        .map_or(0, |continuation| continuation.forced_action_count);
    let compact_survival_trace =
        if data.progress.deepest_survival_actions == data.progress.deepest_progress_actions {
            json!({"same_as": "deepest_progress_trace"})
        } else {
            compact_combat_trace(data.diagnostics.deepest_survival_trace.as_ref())
        };

    json!({
        "schema_name": "LocalTurnGraphCombatTraceV4",
        "schema_version": 4,
        "case": data.run.case,
        "status": format!("{:?}", data.report.status),
        "satisfaction": format!("{:?}", data.run.satisfaction),
        "execution_profile": data.run.execution_profile,
        "search_spec": data.run.search_spec,
        "elapsed_ms": data.run.elapsed.as_millis(),
        "counterfactual": {
            "full_health": data.run.counterfactual.full_health,
            "original_hp": data.run.counterfactual.original_hp,
            "search_hp": data.run.counterfactual.search_hp,
        },
        "root_turn_continuation": data.run.root_turn_continuation,
        "work": {
            "generation_work": data.report.counters.generation_work,
            "exact_nodes": data.report.counters.exact_nodes,
            "completed_turn_options": data.report.counters.completed_turn_options,
            "applied_action_transitions": data.report.counters.applied_action_transitions,
        },
        "storage": data.storage,
        "root_action_families": data.observation.root_action_families,
        "plan_compatible_policy_line": data.policy_line,
        "plan_transition_annotations": data.plan_transition_annotations,
        "plan_transition_portfolio": data.plan_transition_portfolio,
        "deepest": {
            "progress_state": data.progress.deepest_progress_state,
            "progress_node": data.diagnostics.deepest_progress_node,
            "progress_trace": compact_combat_trace(
                data.diagnostics.deepest_progress_trace.as_ref(),
            ),
            "survival_state": data.progress.deepest_survival_state,
            "survival_node": data.diagnostics.deepest_survival_node,
            "survival_trace": compact_survival_trace,
        },
        "witness": data.report.witness.as_ref().map(|witness| json!({
            "final_hp": witness.final_position.combat.entities.player.current_hp,
            "continuation_action_count": witness.actions.len(),
            "total_action_count": forced_action_count.saturating_add(witness.actions.len()),
            "trace": compact_combat_trace(data.diagnostics.witness_trace.as_ref()),
        })),
        "terminal_outcome_action_count_scope": data.run.root_turn_continuation
            .map(|_| "continuation_suffix")
            .unwrap_or("full_search_root"),
        "terminal_outcome_frontier": &data.report.witness_frontier,
        "exported_witness_actions": data.exports.witness_actions,
        "exported_witness_manifest": data.exports.witness_manifest,
        "exported_deepest_survival_case": data.exports.deepest_survival_case,
        "exported_deepest_survival_actions": data.exports.deepest_survival_actions,
        "exported_deepest_progress_case": data.exports.deepest_progress_case,
        "exported_deepest_progress_actions": data.exports.deepest_progress_actions,
    })
}

fn counterfactual_report(counterfactual: LocalGraphCounterfactual) -> Value {
    json!({
        "full_health": counterfactual.full_health,
        "original_hp": counterfactual.original_hp,
        "search_hp": counterfactual.search_hp,
    })
}

fn full_counter_report(report: &LocalTurnGraphWitnessReport) -> Value {
    json!({
        "selections": report.counters.selections,
        "node_visits": report.counters.node_visits,
        "generation_work": report.counters.generation_work,
        "engine_steps": report.counters.engine_steps,
        "exact_nodes": report.counters.exact_nodes,
        "exact_edges": report.counters.exact_edges,
        "completed_turn_options": report.counters.completed_turn_options,
        "applied_action_transitions": report.counters.applied_action_transitions,
        "unique_successor_states": report.counters.unique_successor_states,
        "duplicate_exact_successors": report.counters.duplicate_exact_successors,
        "duplicate_successor_edges": report.counters.duplicate_successor_edges,
        "terminal_losses": report.counters.terminal_losses,
        "depth_limited_successors": report.counters.depth_limited_successors,
        "exhausted_nodes": report.counters.exhausted_nodes,
        "maximum_turn_depth": report.counters.maximum_turn_depth,
        "annotated_exact_edges": report.counters.annotated_exact_edges,
        "terminal_win_options": report.counters.terminal_win_options,
        "witness_replay_attempts": report.counters.witness_replay_attempts,
        "witness_replay_improvements": report.counters.witness_replay_improvements,
        "witness_frontier_changes": report.counters.witness_frontier_changes,
        "witness_replay_dominated_skips": report.counters.witness_replay_dominated_skips,
    })
}

fn full_progress_report(data: &LocalGraphReportData<'_>, readable: bool) -> Value {
    json!({
        "retained_states": data.progress.retained_states,
        "retained_state_work": data.retained_state_work,
        "max_player_turn": data.progress.max_player_turn,
        "max_path_atomic_depth": data.progress.max_path_atomic_depth,
        "deepest_survival_state": data.progress.deepest_survival_state,
        "deepest_survival_node": data.diagnostics.deepest_survival_node,
        "deepest_survival_actions": readable
            .then_some(&data.progress.deepest_survival_actions),
        "deepest_survival_trace": data.diagnostics.deepest_survival_trace,
        "deepest_progress_state": data.progress.deepest_progress_state,
        "deepest_progress_node": data.diagnostics.deepest_progress_node,
        "deepest_progress_actions": readable
            .then_some(&data.progress.deepest_progress_actions),
        "deepest_progress_trace": data.diagnostics.deepest_progress_trace,
        "recent_turn_survival_envelope": data.progress.recent_turn_survival_envelope,
    })
}

pub(super) fn local_graph_full_report(
    data: &LocalGraphReportData<'_>,
    options: LocalGraphFullReportOptions<'_>,
) -> Value {
    let forced_action_count = data
        .run
        .root_turn_continuation
        .map_or(0, |continuation| continuation.forced_action_count);
    let witness_action_counts = data.report.witness.as_ref().map(|witness| {
        let continuation = witness.actions.len();
        json!({
            "forced_root_turn": forced_action_count,
            "continuation_suffix": continuation,
            "total": forced_action_count.saturating_add(continuation),
            "terminal_outcome_scope": data.run.root_turn_continuation
                .map(|_| "continuation_suffix")
                .unwrap_or("full_search_root"),
        })
    });
    let watched_corridor = if options.readable {
        data.observation
            .watched_corridor
            .clone()
            .unwrap_or(Value::Null)
    } else {
        compact_local_corridor_report(data.observation.watched_corridor.as_ref())
    };
    let counterfactual = counterfactual_report(data.run.counterfactual);
    let counters = full_counter_report(data.report);
    let progress = full_progress_report(data, options.readable);

    json!({
        "schema_name": "LocalTurnGraphCombatSearchReportV4",
        "schema_version": 4,
        "case": data.run.case,
        "counterfactual": counterfactual,
        "root_turn_continuation": data.run.root_turn_continuation,
        "action_imitation_artifact": options.action_imitation_artifact,
        "value_prototype_artifact": options.value_prototype_artifact,
        "guidance_bundle": options.guidance_bundle,
        "watch_corridor_actions": options.watch_corridor_actions,
        "satisfaction": format!("{:?}", data.run.satisfaction),
        "scheduler": data.run.execution_profile.scheduler_label(),
        "execution_profile": data.run.execution_profile,
        "search_spec": data.run.search_spec,
        "status": format!("{:?}", data.report.status),
        "elapsed_ms": data.run.elapsed.as_millis(),
        "initial_hp": data.run.counterfactual.search_hp,
        "final_hp": data.report.witness.as_ref().map(|witness| {
            witness.final_position.combat.entities.player.current_hp
        }),
        "witness_action_counts": witness_action_counts,
        "root": {
            "visits": data.report.root_visits,
            "generated_options": data.report.root_generated_options,
            "children": data.report.root_children,
        },
        "root_action_families": data.observation.root_action_families,
        "plan_compatible_policy_line": data.policy_line,
        "counters": counters,
        "progress": progress,
        "storage": data.storage,
        "witness_trace": data.diagnostics.witness_trace,
        "terminal_outcome_frontier": &data.report.witness_frontier,
        "generation_gap_count": data.report.generation_gaps.len(),
        "watched_states": data.observation.watched_states,
        "watched_corridor": watched_corridor,
        "exported_witness_actions": data.exports.witness_actions,
        "exported_witness_manifest": data.exports.witness_manifest,
        "exported_deepest_survival_case": data.exports.deepest_survival_case,
        "exported_deepest_survival_actions": data.exports.deepest_survival_actions,
        "exported_deepest_progress_case": data.exports.deepest_progress_case,
        "exported_deepest_progress_actions": data.exports.deepest_progress_actions,
        "plan_transition_annotations": data.plan_transition_annotations,
        "plan_transition_portfolio": data.plan_transition_portfolio,
        "search_elapsed_ms": options.search_elapsed.as_millis(),
        "performance_timing": options.performance_timing,
        "performance_profile": options.performance_profile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_planner::{
        LocalTurnGraphTerminalOutcomeSnapshotV1, LocalTurnGraphWitnessCounters,
        LocalTurnGraphWitnessStatus,
    };

    struct Fixture {
        report: LocalTurnGraphWitnessReport,
        progress: OracleCombatWitnessProgressSnapshot,
        storage: LocalTurnGraphStorageSnapshot,
        diagnostics: LocalGraphDiagnostics,
        observation: LocalGraphObservation,
        exports: LocalGraphExports,
        plan_transition_portfolio: Value,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                report: LocalTurnGraphWitnessReport {
                    status: LocalTurnGraphWitnessStatus::FrontierExhausted,
                    counters: LocalTurnGraphWitnessCounters::default(),
                    performance_timing: Default::default(),
                    root_visits: 0,
                    root_generated_options: 0,
                    root_children: 0,
                    generation_gaps: Vec::new(),
                    witness: None,
                    witness_frontier: Vec::new(),
                },
                progress: OracleCombatWitnessProgressSnapshot::default(),
                storage: LocalTurnGraphStorageSnapshot::default(),
                diagnostics: LocalGraphDiagnostics {
                    deepest_survival_trace: None,
                    deepest_progress_trace: None,
                    deepest_survival_node: None,
                    deepest_progress_node: None,
                    witness_trace: None,
                },
                observation: LocalGraphObservation {
                    root_action_families: Vec::new(),
                    watched_states: Vec::new(),
                    watched_corridor: None,
                },
                exports: LocalGraphExports {
                    witness_actions: None,
                    witness_manifest: None,
                    deepest_survival_case: None,
                    deepest_survival_actions: None,
                    deepest_progress_case: None,
                    deepest_progress_actions: None,
                },
                plan_transition_portfolio: Value::Null,
            }
        }

        fn data(&self) -> LocalGraphReportData<'_> {
            LocalGraphReportData {
                run: LocalGraphRunIdentity {
                    case: Path::new("fixture.combat.json"),
                    elapsed: Duration::from_millis(7),
                    satisfaction: OracleCombatWitnessSatisfaction::FirstWitness,
                    execution_profile: LocalGraphExecutionProfile::from_controls(
                        false, false, false, false, None,
                    )
                    .expect("fixture execution profile"),
                    search_spec: LocalGraphSearchSpec::from_controls(
                        240,
                        80,
                        15,
                        7,
                        50_000,
                        3,
                        9,
                        Some(2),
                        false,
                        None,
                        None,
                        None,
                    ),
                    counterfactual: LocalGraphCounterfactual {
                        full_health: false,
                        original_hp: 80,
                        search_hp: 80,
                    },
                    root_turn_continuation: None,
                },
                report: &self.report,
                progress: &self.progress,
                retained_state_work: 0,
                storage: self.storage,
                policy_line: None,
                plan_transition_annotations: false,
                plan_transition_portfolio: &self.plan_transition_portfolio,
                diagnostics: &self.diagnostics,
                observation: &self.observation,
                exports: &self.exports,
            }
        }
    }

    #[test]
    fn trace_schema_stays_compact_and_preserves_unknown_witness() {
        let fixture = Fixture::new();
        let report = local_graph_trace_report(&fixture.data());

        assert_eq!(report["schema_name"], "LocalTurnGraphCombatTraceV4");
        assert_eq!(report["elapsed_ms"], 7);
        assert_eq!(report["storage"]["exact_nodes"], 0);
        assert!(report["witness"].is_null());
        assert!(report.get("watched_states").is_none());
        assert!(report.get("performance_profile").is_none());
    }

    #[test]
    fn trace_terminal_frontier_contains_typed_outcomes_not_action_lines() {
        let mut fixture = Fixture::new();
        fixture.report.witness_frontier = vec![
            LocalTurnGraphTerminalOutcomeSnapshotV1::from_materialized_outcome_v1(
                sts_oracle_learning::eval::combat_witness_materialization::CombatWitnessMaterializedOutcomeV1 {
                    final_hp: 71,
                    final_gold: 0,
                    persistent: sts_oracle_learning::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1 {
                        max_hp: 85,
                        recoverable_gold_delta: 75,
                        unrecovered_stolen_gold: 0,
                        ritual_dagger_value: 0,
                        genetic_algorithm_value: 0,
                        external_burden_count: 0,
                    },
                    starting_potion_expenditure_slots: 0,
                    total_potion_expenditures: 0,
                    generated_potions: Vec::new(),
                    final_relics: Vec::new(),
                    meta_changes: Vec::new(),
                    pending_rewards: vec![
                        sts_oracle_learning::state::rewards::RewardItem::StolenGold { amount: 75 },
                    ],
                    combat_mugged: false,
                },
                19,
                4.5,
                false,
            ),
        ];

        let report = local_graph_trace_report(&fixture.data());
        let outcome = &report["terminal_outcome_frontier"][0];
        assert_eq!(outcome["final_hp"], 71);
        assert_eq!(outcome["recoverable_stolen_gold"], 75);
        assert_eq!(outcome["unrecovered_stolen_gold"], 0);
        assert!(outcome.get("actions").is_none());
        assert!(outcome.get("trace").is_none());
    }

    #[test]
    fn full_schema_owns_complete_counters_and_full_only_fields() {
        let fixture = Fixture::new();
        let timing = json!({"selection_elapsed_ns": 0});
        let profile = json!({"schema_name": "LocalGraphPerformanceProfileV1"});
        let report = local_graph_full_report(
            &fixture.data(),
            LocalGraphFullReportOptions {
                action_imitation_artifact: None,
                value_prototype_artifact: None,
                guidance_bundle: None,
                watch_corridor_actions: &[],
                readable: false,
                search_elapsed: Duration::from_millis(3),
                performance_timing: &timing,
                performance_profile: &profile,
            },
        );

        assert_eq!(report["schema_name"], "LocalTurnGraphCombatSearchReportV4");
        assert_eq!(report["scheduler"], "anchor_and_guides");
        assert_eq!(
            report["execution_profile"]["guide_service"],
            "anchor_and_guides"
        );
        assert_eq!(
            report["search_spec"]["allowance"]["max_generation_work"],
            240
        );
        assert_eq!(report["search_elapsed_ms"], 3);
        assert_eq!(report["counters"]["terminal_win_options"], 0);
        assert_eq!(report["storage"]["exact_nodes"], 0);
        assert_eq!(report["watched_states"], json!([]));
        assert_eq!(
            report["performance_profile"]["schema_name"],
            "LocalGraphPerformanceProfileV1"
        );
    }
}
