use super::super::registry::boundary_spec;
use super::super::rules::{candidate_level, latent_debt_kind, primary_divergence};
use super::super::types::*;

pub fn classify_state_abstraction_case(
    input: StateAbstractionCaseInput<'_>,
) -> Option<StateAbstractionCaseReport> {
    if input.same_effect_turn_sequence_groups > 0 {
        return Some(turn_sequence_case_report(
            input,
            StateDivergenceKind::IdentityOnlyCandidate,
            None,
            StateAbstractionRevealGate::Unknown,
            StateAbstractionConsumer::ReportOnly,
            vec![
                "same-effect ordered variants are candidates for later simulator-backed commutation probes",
                "v1 keeps this report-only because current diagnostics do not prove reveal gates",
            ],
        ));
    }
    if input.order_sensitive_turn_sequence_groups > 0 {
        let primary = primary_divergence(&input.turn_sequence_divergence_histogram);
        return Some(turn_sequence_case_report(
            input,
            primary.kind,
            primary.first_divergence_path,
            primary.guessed_reveal_gate,
            StateAbstractionConsumer::ReportOnly,
            vec![
                "current exact/dominance keys observe different future-relevant state for reordered prefixes",
                "turn-sequence divergence classification is diagnostic guidance, not safe-pruning evidence",
            ],
        ));
    }
    None
}

fn turn_sequence_case_report(
    input: StateAbstractionCaseInput<'_>,
    divergence_kind: StateDivergenceKind,
    first_divergence_path: Option<&'static str>,
    guessed_reveal_gate: StateAbstractionRevealGate,
    recommended_consumer: StateAbstractionConsumer,
    notes: Vec<&'static str>,
) -> StateAbstractionCaseReport {
    let spec = boundary_spec(StateAbstractionBoundaryId::TurnSequenceOrderSensitive);
    let public_observation_changed =
        matches!(divergence_kind, StateDivergenceKind::ImmediatePublicDelta);
    let legal_actions_changed = matches!(divergence_kind, StateDivergenceKind::LegalActionDelta);
    let terminal_class_changed = matches!(divergence_kind, StateDivergenceKind::TerminalDelta);
    let latent_debt_kind = latent_debt_kind(divergence_kind);
    let candidate_level = candidate_level(
        divergence_kind,
        latent_debt_kind,
        first_divergence_path,
        guessed_reveal_gate,
    );
    let turn_sequence_divergence_histogram = input
        .turn_sequence_divergence_histogram
        .into_iter()
        .map(|entry| StateAbstractionCaseDivergenceCount {
            kind: entry.kind,
            first_divergence_path: entry.first_divergence_path,
            guessed_reveal_gate: entry.guessed_reveal_gate,
            groups: entry.groups,
        })
        .collect();
    StateAbstractionCaseReport {
        case_id: input.case_id.to_string(),
        boundary_id: spec.id,
        soundness: spec.soundness,
        allowed_consumers: spec.allowed_consumers,
        divergence_kind,
        first_divergence_path,
        public_observation_changed: Some(public_observation_changed),
        legal_actions_changed: Some(legal_actions_changed),
        terminal_class_changed: Some(terminal_class_changed),
        guessed_reveal_gate,
        latent_debt_kind,
        candidate_level,
        recommended_consumer,
        pruning_allowed: false,
        exact_branch_removal_allowed: false,
        same_effect_turn_sequence_groups: input.same_effect_turn_sequence_groups,
        order_sensitive_turn_sequence_groups: input.order_sensitive_turn_sequence_groups,
        turn_sequence_divergence_histogram,
        notes,
    }
}
