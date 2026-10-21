use super::*;

#[test]
fn turn_sequence_order_sensitive_boundary_is_report_only() {
    let spec = boundary_spec(StateAbstractionBoundaryId::TurnSequenceOrderSensitive);

    assert_eq!(spec.soundness, StateAbstractionSoundnessLevel::ReportOnly);
    assert_eq!(
        spec.allowed_consumers,
        vec![StateAbstractionConsumer::ReportOnly]
    );
}

#[test]
fn pending_choice_boundary_is_local_action_equivalent_only() {
    let spec = boundary_spec(StateAbstractionBoundaryId::PendingChoiceIdenticalRuntimeCard);

    assert_eq!(
        spec.soundness,
        StateAbstractionSoundnessLevel::LocalActionEquivalent
    );
    assert_eq!(
        spec.allowed_consumers,
        vec![StateAbstractionConsumer::LocalActionDedup]
    );
}

#[test]
fn classifier_marks_order_sensitive_turn_sequence_as_report_only() {
    let report = classify_state_abstraction_case(StateAbstractionCaseInput {
        case_id: "case",
        same_effect_turn_sequence_groups: 0,
        order_sensitive_turn_sequence_groups: 3,
        turn_sequence_divergence_histogram: Vec::new(),
    })
    .expect("order-sensitive sequence should classify");

    assert_eq!(
        report.boundary_id,
        StateAbstractionBoundaryId::TurnSequenceOrderSensitive
    );
    assert_eq!(report.divergence_kind, StateDivergenceKind::Unknown);
    assert_eq!(
        report.recommended_consumer,
        StateAbstractionConsumer::ReportOnly
    );
    assert!(!report.exact_branch_removal_allowed);
}

#[test]
fn classifier_uses_turn_sequence_divergence_histogram() {
    let report = classify_state_abstraction_case(StateAbstractionCaseInput {
        case_id: "case",
        same_effect_turn_sequence_groups: 0,
        order_sensitive_turn_sequence_groups: 3,
        turn_sequence_divergence_histogram: vec![StateAbstractionDivergenceInput {
            kind: StateDivergenceKind::DrawPileOrderDelta,
            first_divergence_path: Some("combat.zones.draw_pile"),
            guessed_reveal_gate: StateAbstractionRevealGate::NextDraw,
            groups: 2,
        }],
    })
    .expect("order-sensitive sequence should classify");

    assert_eq!(
        report.divergence_kind,
        StateDivergenceKind::DrawPileOrderDelta
    );
    assert_eq!(report.first_divergence_path, Some("combat.zones.draw_pile"));
    assert_eq!(
        report.guessed_reveal_gate,
        StateAbstractionRevealGate::NextDraw
    );
    assert_eq!(report.turn_sequence_divergence_histogram.len(), 1);
    assert_eq!(report.public_observation_changed, Some(false));
}

#[test]
fn discard_order_delta_is_horizon_limited_candidate_only() {
    let report = classify_state_abstraction_case(StateAbstractionCaseInput {
        case_id: "case",
        same_effect_turn_sequence_groups: 0,
        order_sensitive_turn_sequence_groups: 3,
        turn_sequence_divergence_histogram: vec![StateAbstractionDivergenceInput {
            kind: StateDivergenceKind::DiscardOrderDelta,
            first_divergence_path: Some("combat.zones.discard_pile"),
            guessed_reveal_gate: StateAbstractionRevealGate::NextShuffle,
            groups: 2,
        }],
    })
    .expect("order-sensitive sequence should classify");

    assert_eq!(
        report.latent_debt_kind,
        StateAbstractionLatentDebtKind::DiscardOrder
    );
    assert_eq!(
        report.candidate_level,
        StateAbstractionCandidateLevel::HorizonLimitedCandidate
    );
    assert!(!report.pruning_allowed);
    assert!(!report.exact_branch_removal_allowed);
}

#[test]
fn discard_lineage_order_delta_requires_identity_audit() {
    let report = classify_state_abstraction_case(StateAbstractionCaseInput {
        case_id: "case",
        same_effect_turn_sequence_groups: 0,
        order_sensitive_turn_sequence_groups: 3,
        turn_sequence_divergence_histogram: vec![StateAbstractionDivergenceInput {
            kind: StateDivergenceKind::CardLineageDelta,
            first_divergence_path: Some("combat.zones.discard_pile.lineage_order"),
            guessed_reveal_gate: StateAbstractionRevealGate::NextShuffle,
            groups: 2,
        }],
    })
    .expect("order-sensitive sequence should classify");

    assert_eq!(
        report.latent_debt_kind,
        StateAbstractionLatentDebtKind::CardIdentity
    );
    assert_eq!(
        report.candidate_level,
        StateAbstractionCandidateLevel::IdentityAuditCandidate
    );
    assert!(!report.pruning_allowed);
    assert!(!report.exact_branch_removal_allowed);
}

#[test]
fn played_card_history_delta_blocks_abstraction_candidate() {
    let report = classify_state_abstraction_case(StateAbstractionCaseInput {
        case_id: "case",
        same_effect_turn_sequence_groups: 0,
        order_sensitive_turn_sequence_groups: 3,
        turn_sequence_divergence_histogram: vec![StateAbstractionDivergenceInput {
            kind: StateDivergenceKind::TurnPlayedCardHistoryDelta,
            first_divergence_path: Some("combat.turn.counters.card_ids_played"),
            guessed_reveal_gate: StateAbstractionRevealGate::NextLegalActionGeneration,
            groups: 2,
        }],
    })
    .expect("order-sensitive sequence should classify");

    assert_eq!(
        report.latent_debt_kind,
        StateAbstractionLatentDebtKind::TurnPlayedCardHistory
    );
    assert_eq!(
        report.candidate_level,
        StateAbstractionCandidateLevel::ReportOnlyBlocked
    );
}

#[test]
fn gate_report_counts_candidate_groups_separately_from_cases() {
    let reports = vec![
        classify_state_abstraction_case(StateAbstractionCaseInput {
            case_id: "case_a",
            same_effect_turn_sequence_groups: 0,
            order_sensitive_turn_sequence_groups: 3,
            turn_sequence_divergence_histogram: vec![StateAbstractionDivergenceInput {
                kind: StateDivergenceKind::DiscardOrderDelta,
                first_divergence_path: Some("combat.zones.discard_pile"),
                guessed_reveal_gate: StateAbstractionRevealGate::NextShuffle,
                groups: 7,
            }],
        })
        .expect("discard candidate should classify"),
        classify_state_abstraction_case(StateAbstractionCaseInput {
            case_id: "case_b",
            same_effect_turn_sequence_groups: 0,
            order_sensitive_turn_sequence_groups: 3,
            turn_sequence_divergence_histogram: vec![StateAbstractionDivergenceInput {
                kind: StateDivergenceKind::TurnPlayedCardHistoryDelta,
                first_divergence_path: Some("combat.turn.counters.card_ids_played"),
                guessed_reveal_gate: StateAbstractionRevealGate::NextLegalActionGeneration,
                groups: 2,
            }],
        })
        .expect("blocked candidate should classify"),
    ];

    let gate = build_state_abstraction_gate_report(reports);

    assert_eq!(
        histogram_count(&gate.candidate_level_histogram, "horizon_limited_candidate"),
        1
    );
    assert_eq!(
        histogram_count(
            &gate.candidate_level_group_histogram,
            "horizon_limited_candidate"
        ),
        7
    );
    assert_eq!(
        histogram_count(&gate.candidate_level_group_histogram, "report_only_blocked"),
        2
    );
}

#[test]
fn gate_report_blocks_identity_candidates_until_reference_audit() {
    let reports = vec![classify_state_abstraction_case(StateAbstractionCaseInput {
        case_id: "case_identity",
        same_effect_turn_sequence_groups: 0,
        order_sensitive_turn_sequence_groups: 3,
        turn_sequence_divergence_histogram: vec![StateAbstractionDivergenceInput {
            kind: StateDivergenceKind::CardLineageDelta,
            first_divergence_path: Some("combat.zones.discard_pile.lineage_order"),
            guessed_reveal_gate: StateAbstractionRevealGate::NextShuffle,
            groups: 5,
        }],
    })
    .expect("identity candidate should classify")];

    let gate = build_state_abstraction_gate_report(reports);

    assert_eq!(gate.schema_version, 2);
    assert_eq!(gate.identity_audit.candidate_cases, 1);
    assert_eq!(gate.identity_audit.candidate_groups, 5);
    assert_eq!(
        gate.identity_audit.status,
        "blocked_until_card_identity_reference_audit"
    );
    assert!(!gate.identity_audit.proof_pruning_enabled);
    assert!(!gate.identity_audit.exact_branch_removal_allowed);
    assert_eq!(gate.identity_audit.samples[0].case_id, "case_identity");
}

fn histogram_count(entries: &[StateAbstractionHistogramEntry], key: &str) -> usize {
    entries
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| entry.cases)
        .unwrap_or(0)
}
