use super::*;

#[test]
fn static_candidate_requires_discard_order_until_next_shuffle() {
    assert!(is_static_discard_order_candidate(
        StateDivergenceKind::DiscardOrderDelta,
        Some("combat.zones.discard_pile"),
        StateAbstractionRevealGate::NextShuffle,
    ));
    assert!(!is_static_discard_order_candidate(
        StateDivergenceKind::DiscardOrderDelta,
        Some("combat.zones.draw_pile"),
        StateAbstractionRevealGate::NextShuffle,
    ));
    assert!(!is_static_discard_order_candidate(
        StateDivergenceKind::DiscardOrderDelta,
        Some("combat.zones.discard_pile"),
        StateAbstractionRevealGate::NextDraw,
    ));
    assert!(!is_static_discard_order_candidate(
        StateDivergenceKind::ImmediatePublicDelta,
        Some("combat.zones.discard_pile"),
        StateAbstractionRevealGate::NextShuffle,
    ));
}

#[test]
fn summary_reports_static_candidates_without_pruning() {
    let collector = DiscardOrderShadowAuditCollector::default();
    let report = summarize_discard_order_shadow_audit(
        vec![
            DiscardOrderShadowAuditObservation {
                origin_key: "origin_b".to_string(),
                unordered_key_preview: "B>A".to_string(),
                states: 2,
                max_prefix_length: 2,
                ordered_variants: 2,
                effect_variants: 2,
                max_legal_actions: 4,
                first_divergence_path: Some("combat.zones.discard_pile"),
                reveal_gate: StateAbstractionRevealGate::NextShuffle,
            },
            DiscardOrderShadowAuditObservation {
                origin_key: "origin_a".to_string(),
                unordered_key_preview: "A>B".to_string(),
                states: 3,
                max_prefix_length: 3,
                ordered_variants: 2,
                effect_variants: 2,
                max_legal_actions: 5,
                first_divergence_path: Some("combat.zones.discard_pile"),
                reveal_gate: StateAbstractionRevealGate::NextShuffle,
            },
        ],
        &collector,
    );

    assert_eq!(report.candidate_groups, 2);
    assert_eq!(report.candidate_states, 5);
    assert_eq!(report.static_immediate_safe_groups, 2);
    assert_eq!(report.static_immediate_safe_states, 5);
    assert_eq!(report.exact_rollout_verified_groups, 0);
    assert_eq!(report.one_step_exact_checked_groups, 0);
    assert!(!report.proof_pruning_enabled);
    assert_eq!(report.samples[0].origin_key, "origin_a");
    assert_eq!(report.samples[0].one_step_exact_status, "not_sampled");
}
