use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use sts_combat_planner::{LocalTurnGraphWitnessReport, DETAIL_TIMING_SAMPLE_INTERVAL};

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn share(elapsed_ns: u64, parent_ns: u64) -> f64 {
    if parent_ns == 0 {
        0.0
    } else {
        elapsed_ns as f64 / parent_ns as f64
    }
}

fn duration_share(elapsed_ns: u64, parent_ns: u64) -> Value {
    json!({
        "elapsed_ns": elapsed_ns,
        "percent_of_parent": share(elapsed_ns, parent_ns) * 100.0,
    })
}

fn per_second(count: usize, elapsed_ns: u64) -> Option<f64> {
    (elapsed_ns > 0).then(|| count as f64 * 1_000_000_000.0 / elapsed_ns as f64)
}

fn nanos_per_item(elapsed_ns: u64, count: usize) -> Option<f64> {
    (count > 0).then(|| elapsed_ns as f64 / count as f64)
}

pub(super) fn local_graph_performance_profile(
    search_elapsed: Duration,
    report: &LocalTurnGraphWitnessReport,
) -> Value {
    let search_ns = nanos(search_elapsed);
    let timing = report.performance_timing;
    let counters = &report.counters;

    let outer_accounted_ns = timing
        .selection_elapsed_ns
        .saturating_add(timing.generation_elapsed_ns)
        .saturating_add(timing.admission_elapsed_ns);
    let outer_unattributed_ns = search_ns.saturating_sub(outer_accounted_ns);
    let outer_admission_accounted_ns = timing
        .admission_root_option_elapsed_ns
        .saturating_add(timing.admission_witness_filter_elapsed_ns)
        .saturating_add(timing.admission_witness_replay_elapsed_ns)
        .saturating_add(timing.successor_identity_elapsed_ns)
        .saturating_add(timing.successor_lookup_elapsed_ns)
        .saturating_add(timing.successor_node_build_elapsed_ns)
        .saturating_add(timing.successor_edge_elapsed_ns)
        .saturating_add(timing.successor_backup_elapsed_ns)
        .saturating_add(timing.admission_refresh_elapsed_ns);
    let outer_admission_other_ns = timing
        .admission_elapsed_ns
        .saturating_sub(outer_admission_accounted_ns);

    // These four buckets are siblings inside generator time. The more
    // detailed seen/publish/trace counters below are nested inside transition
    // admission and must not be added to this total again.
    let generation_accounted_ns = timing
        .atomic_expand_elapsed_ns
        .saturating_add(timing.transition_simulation_elapsed_ns)
        .saturating_add(timing.transition_identity_elapsed_ns)
        .saturating_add(timing.transition_admission_elapsed_ns);
    let generation_unattributed_ns = timing
        .generation_elapsed_ns
        .saturating_sub(generation_accounted_ns);
    let transition_admission_other_ns = timing.transition_admission_elapsed_ns.saturating_sub(
        timing
            .transition_seen_elapsed_ns
            .saturating_add(timing.transition_publish_elapsed_ns),
    );
    let transition_identity_other_ns = timing.transition_identity_elapsed_ns.saturating_sub(
        timing
            .transition_key_build_elapsed_ns
            .saturating_add(timing.transition_key_index_elapsed_ns),
    );
    let transition_publish_other_ns = timing.transition_publish_elapsed_ns.saturating_sub(
        timing
            .transition_trace_elapsed_ns
            .saturating_add(timing.transition_publish_trace_node_elapsed_ns)
            .saturating_add(timing.transition_publish_boundary_elapsed_ns)
            .saturating_add(timing.transition_publish_complete_elapsed_ns)
            .saturating_add(timing.transition_publish_push_elapsed_ns),
    );

    let transitions = counters.applied_action_transitions;
    let unique_ratio =
        (transitions > 0).then(|| counters.unique_successor_states as f64 / transitions as f64);
    let duplicate_ratio =
        (transitions > 0).then(|| counters.duplicate_exact_successors as f64 / transitions as f64);

    json!({
        "schema_name": "LocalGraphPerformanceProfileV1",
        "schema_version": 1,
        "detail_timing_sample_interval": DETAIL_TIMING_SAMPLE_INTERVAL,
        "search_elapsed_ns": search_ns,
        "outer": {
            "selection": duration_share(timing.selection_elapsed_ns, search_ns),
            "generation": duration_share(timing.generation_elapsed_ns, search_ns),
            "admission": duration_share(timing.admission_elapsed_ns, search_ns),
            "unattributed": duration_share(outer_unattributed_ns, search_ns),
            "accounted_elapsed_ns": outer_accounted_ns,
            "admission_breakdown": {
                "root_option_accounting": duration_share(
                    timing.admission_root_option_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "witness_replay": duration_share(
                    timing.admission_witness_replay_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "witness_filter": duration_share(
                    timing.admission_witness_filter_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "successor_identity": duration_share(
                    timing.successor_identity_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "successor_lookup": duration_share(
                    timing.successor_lookup_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "successor_node_build": duration_share(
                    timing.successor_node_build_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "successor_edge": duration_share(
                    timing.successor_edge_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "successor_backup": duration_share(
                    timing.successor_backup_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "refresh_exhaustion": duration_share(
                    timing.admission_refresh_elapsed_ns,
                    timing.admission_elapsed_ns,
                ),
                "other": duration_share(
                    outer_admission_other_ns,
                    timing.admission_elapsed_ns,
                ),
            },
        },
        "generation": {
            "atomic_expand": duration_share(
                timing.atomic_expand_elapsed_ns,
                timing.generation_elapsed_ns,
            ),
            "transition_simulation": duration_share(
                timing.transition_simulation_elapsed_ns,
                timing.generation_elapsed_ns,
            ),
            "transition_identity": duration_share(
                timing.transition_identity_elapsed_ns,
                timing.generation_elapsed_ns,
            ),
            "transition_identity_breakdown": {
                "key_build": duration_share(
                    timing.transition_key_build_elapsed_ns,
                    timing.transition_identity_elapsed_ns,
                ),
                "key_index": duration_share(
                    timing.transition_key_index_elapsed_ns,
                    timing.transition_identity_elapsed_ns,
                ),
                "other": duration_share(
                    transition_identity_other_ns,
                    timing.transition_identity_elapsed_ns,
                ),
            },
            "transition_admission": duration_share(
                timing.transition_admission_elapsed_ns,
                timing.generation_elapsed_ns,
            ),
            "unattributed": duration_share(
                generation_unattributed_ns,
                timing.generation_elapsed_ns,
            ),
            "accounted_elapsed_ns": generation_accounted_ns,
            "transition_admission_breakdown": {
                "seen_set": duration_share(
                    timing.transition_seen_elapsed_ns,
                    timing.transition_admission_elapsed_ns,
                ),
                "publish": duration_share(
                    timing.transition_publish_elapsed_ns,
                    timing.transition_admission_elapsed_ns,
                ),
                "other": duration_share(
                    transition_admission_other_ns,
                    timing.transition_admission_elapsed_ns,
                ),
                "trace_subset_of_publish": duration_share(
                    timing.transition_trace_elapsed_ns,
                    timing.transition_publish_elapsed_ns,
                ),
                "publish_breakdown": {
                    "trace_node": duration_share(
                        timing.transition_publish_trace_node_elapsed_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                    "boundary": duration_share(
                        timing.transition_publish_boundary_elapsed_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                    "complete": duration_share(
                        timing.transition_publish_complete_elapsed_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                    "push": duration_share(
                        timing.transition_publish_push_elapsed_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                    "guide_subset_of_push": duration_share(
                        timing.transition_publish_guide_elapsed_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                    "retain_subset_of_push": duration_share(
                        timing.transition_publish_retain_elapsed_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                    "agenda_subset_of_push": duration_share(
                        timing.transition_publish_agenda_elapsed_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                    "trace": duration_share(
                        timing.transition_trace_elapsed_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                    "other": duration_share(
                        transition_publish_other_ns,
                        timing.transition_publish_elapsed_ns,
                    ),
                },
            },
        },
        "throughput": {
            "generation_work": counters.generation_work,
            "applied_action_transitions": transitions,
            "unique_successor_states": counters.unique_successor_states,
            "duplicate_exact_successors": counters.duplicate_exact_successors,
            "duplicate_successor_edges": counters.duplicate_successor_edges,
            "terminal_win_options": counters.terminal_win_options,
            "witness_replay_attempts": counters.witness_replay_attempts,
            "witness_replay_improvements": counters.witness_replay_improvements,
            "witness_frontier_changes": counters.witness_frontier_changes,
            "witness_replay_dominated_skips": counters.witness_replay_dominated_skips,
            "completed_turn_options": counters.completed_turn_options,
            "exact_nodes": counters.exact_nodes,
            "exact_edges": counters.exact_edges,
            "generation_work_per_second": per_second(counters.generation_work, search_ns),
            "transitions_per_second": per_second(transitions, search_ns),
            "completed_turn_options_per_second": per_second(
                counters.completed_turn_options,
                search_ns,
            ),
            "unique_successor_ratio": unique_ratio,
            "duplicate_successor_ratio": duplicate_ratio,
            "ns_per_completed_turn_option": {
                "outer_admission": nanos_per_item(
                    timing.admission_elapsed_ns,
                    counters.completed_turn_options,
                ),
                "successor_backup": nanos_per_item(
                    timing.successor_backup_elapsed_ns,
                    counters.completed_turn_options,
                ),
            },
            "ns_per_new_exact_node": {
                "node_build": nanos_per_item(
                    timing.successor_node_build_elapsed_ns,
                    counters.exact_nodes.saturating_sub(1),
                ),
            },
            "ns_per_applied_transition": {
                "simulation": nanos_per_item(
                    timing.transition_simulation_elapsed_ns,
                    transitions,
                ),
                "identity": nanos_per_item(
                    timing.transition_identity_elapsed_ns,
                    transitions,
                ),
                "key_build": nanos_per_item(
                    timing.transition_key_build_elapsed_ns,
                    transitions,
                ),
                "key_index": nanos_per_item(
                    timing.transition_key_index_elapsed_ns,
                    transitions,
                ),
                "seen_set": nanos_per_item(
                    timing.transition_seen_elapsed_ns,
                    transitions,
                ),
                "publish": nanos_per_item(
                    timing.transition_publish_elapsed_ns,
                    transitions,
                ),
                "publish_trace_node": nanos_per_item(
                    timing.transition_publish_trace_node_elapsed_ns,
                    transitions,
                ),
                "publish_boundary": nanos_per_item(
                    timing.transition_publish_boundary_elapsed_ns,
                    transitions,
                ),
                "publish_complete": nanos_per_item(
                    timing.transition_publish_complete_elapsed_ns,
                    transitions,
                ),
                "publish_push": nanos_per_item(
                    timing.transition_publish_push_elapsed_ns,
                    transitions,
                ),
                "publish_guide": nanos_per_item(
                    timing.transition_publish_guide_elapsed_ns,
                    transitions,
                ),
                "publish_retain": nanos_per_item(
                    timing.transition_publish_retain_elapsed_ns,
                    transitions,
                ),
                "publish_agenda": nanos_per_item(
                    timing.transition_publish_agenda_elapsed_ns,
                    transitions,
                ),
            },
        },
    })
}

pub(super) fn local_graph_performance_report(
    search_elapsed: Duration,
    case: &Path,
    report: &LocalTurnGraphWitnessReport,
) -> Value {
    let mut profile = local_graph_performance_profile(search_elapsed, report);
    let object = profile
        .as_object_mut()
        .expect("performance profile must be a JSON object");
    object.insert("case".to_owned(), json!(case));
    object.insert("status".to_owned(), json!(format!("{:?}", report.status)));
    object.insert(
        "witness".to_owned(),
        report
            .witness
            .as_ref()
            .map(|witness| {
                json!({
                    "final_hp": witness.final_position.combat.entities.player.current_hp,
                    "actions": witness.actions.len(),
                })
            })
            .unwrap_or(Value::Null),
    );
    profile
}

pub(super) fn local_graph_performance_timing(report: &LocalTurnGraphWitnessReport) -> Value {
    json!(report.performance_timing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_planner::{LocalTurnGraphWitnessCounters, LocalTurnGraphWitnessStatus};

    fn report() -> LocalTurnGraphWitnessReport {
        LocalTurnGraphWitnessReport {
            status: LocalTurnGraphWitnessStatus::FrontierExhausted,
            counters: LocalTurnGraphWitnessCounters::default(),
            performance_timing: Default::default(),
            root_visits: 0,
            root_generated_options: 0,
            root_children: 0,
            generation_gaps: Vec::new(),
            witness: None,
            witness_frontier: Vec::new(),
        }
    }

    #[test]
    fn profile_keeps_nested_timing_out_of_parent_totals() {
        let mut report = report();
        report.performance_timing.selection_elapsed_ns = 100;
        report.performance_timing.generation_elapsed_ns = 700;
        report.performance_timing.admission_elapsed_ns = 100;
        report.performance_timing.atomic_expand_elapsed_ns = 50;
        report.performance_timing.transition_simulation_elapsed_ns = 200;
        report.performance_timing.transition_identity_elapsed_ns = 100;
        report.performance_timing.transition_admission_elapsed_ns = 200;
        report.performance_timing.transition_seen_elapsed_ns = 50;
        report.performance_timing.transition_publish_elapsed_ns = 100;
        report.performance_timing.transition_trace_elapsed_ns = 20;
        report.counters.generation_work = 200;
        report.counters.applied_action_transitions = 100;
        report.counters.unique_successor_states = 80;
        report.counters.duplicate_exact_successors = 20;

        let profile = local_graph_performance_profile(Duration::from_nanos(1_000), &report);

        assert_eq!(profile["outer"]["unattributed"]["elapsed_ns"], 100);
        assert_eq!(
            profile["outer"]["admission_breakdown"]["other"]["elapsed_ns"],
            100
        );
        assert_eq!(profile["generation"]["accounted_elapsed_ns"], 550);
        assert_eq!(profile["generation"]["unattributed"]["elapsed_ns"], 150);
        assert_eq!(
            profile["generation"]["transition_identity_breakdown"]["other"]["elapsed_ns"],
            100
        );
        assert_eq!(
            profile["generation"]["transition_admission_breakdown"]["other"]["elapsed_ns"],
            50
        );
        assert_eq!(profile["throughput"]["unique_successor_ratio"], 0.8);
        assert_eq!(profile["throughput"]["duplicate_successor_ratio"], 0.2);
        assert_eq!(
            profile["throughput"]["ns_per_applied_transition"]["simulation"],
            2.0
        );
    }

    #[test]
    fn zero_work_profile_has_finite_shares_and_null_unit_costs() {
        let profile = local_graph_performance_profile(Duration::ZERO, &report());

        assert_eq!(profile["outer"]["generation"]["percent_of_parent"], 0.0);
        assert!(profile["throughput"]["transitions_per_second"].is_null());
        assert!(profile["throughput"]["ns_per_applied_transition"]["identity"].is_null());
    }

    #[test]
    fn performance_report_owns_case_status_and_witness_metadata() {
        let report = local_graph_performance_report(
            Duration::ZERO,
            Path::new("fixture.combat.json"),
            &report(),
        );

        assert_eq!(report["case"], "fixture.combat.json");
        assert_eq!(report["status"], "FrontierExhausted");
        assert!(report["witness"].is_null());
    }

    #[test]
    fn raw_performance_timing_comes_from_the_authoritative_timing_type() {
        let mut report = report();
        report.performance_timing.selection_elapsed_ns = 17;
        report
            .performance_timing
            .transition_publish_agenda_elapsed_ns = 23;

        let timing = local_graph_performance_timing(&report);

        assert_eq!(timing["selection_elapsed_ns"], 17);
        assert_eq!(timing["transition_publish_agenda_elapsed_ns"], 23);
        assert_eq!(
            timing,
            serde_json::to_value(report.performance_timing)
                .expect("performance timing must remain serializable")
        );
    }
}
