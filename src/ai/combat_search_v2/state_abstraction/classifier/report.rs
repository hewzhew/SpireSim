use super::super::histogram::{group_histogram, histogram};
use super::super::registry::registered_boundary_specs;
use super::super::rules::{candidate_level, latent_debt_kind};
use super::super::types::*;
use super::identity_audit::identity_audit_report;

pub fn build_state_abstraction_gate_report(
    cases: Vec<StateAbstractionCaseReport>,
) -> StateAbstractionGateReport {
    let identity_audit = identity_audit_report(&cases);
    StateAbstractionGateReport {
        schema_name: "StateAbstractionGateReport",
        schema_version: 2,
        policy: "state abstractions are reported with explicit soundness and allowed consumers; report-only and estimate-only boundaries must not remove exact search branches",
        registered_boundaries: registered_boundary_specs(),
        case_count: cases.len(),
        divergence_histogram: histogram(cases.iter().map(|case| case.divergence_kind.label())),
        divergence_group_histogram: group_histogram(cases.iter().flat_map(|case| {
            case.turn_sequence_divergence_histogram
                .iter()
                .map(|entry| (entry.kind.label(), entry.groups))
        })),
        divergence_path_histogram: histogram(
            cases
                .iter()
                .map(|case| case.first_divergence_path.unwrap_or("none")),
        ),
        latent_debt_histogram: histogram(cases.iter().map(|case| case.latent_debt_kind.label())),
        latent_debt_group_histogram: group_histogram(cases.iter().flat_map(|case| {
            case.turn_sequence_divergence_histogram.iter().map(|entry| {
                let debt = latent_debt_kind(entry.kind);
                (debt.label(), entry.groups)
            })
        })),
        candidate_level_histogram: histogram(cases.iter().map(|case| case.candidate_level.label())),
        candidate_level_group_histogram: group_histogram(cases.iter().flat_map(|case| {
            case.turn_sequence_divergence_histogram.iter().map(|entry| {
                let debt = latent_debt_kind(entry.kind);
                let level = candidate_level(
                    entry.kind,
                    debt,
                    entry.first_divergence_path,
                    entry.guessed_reveal_gate,
                );
                (level.label(), entry.groups)
            })
        })),
        recommended_consumer_histogram: histogram(
            cases.iter().map(|case| case.recommended_consumer.label()),
        ),
        reveal_gate_histogram: histogram(cases.iter().map(|case| case.guessed_reveal_gate.label())),
        reveal_gate_group_histogram: group_histogram(cases.iter().flat_map(|case| {
            case.turn_sequence_divergence_histogram
                .iter()
                .map(|entry| (entry.guessed_reveal_gate.label(), entry.groups))
        })),
        identity_audit,
        cases,
        notes: vec![
            "exact simulator state remains authoritative",
            "turn_sequence_order_sensitive is report_only in v1",
            "candidate_level identifies future audit targets and does not enable pruning",
            "pending choice deduplication is local action-list equivalence, not global state equality",
        ],
    }
}
