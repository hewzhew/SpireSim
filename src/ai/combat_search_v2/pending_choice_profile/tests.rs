use super::*;
use crate::state::core::EngineState;

#[test]
fn single_card_grid_select_profile_uses_action_fanout_not_candidate_count() {
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
        source_pile: crate::state::core::PileType::Draw,
        candidate_lineages: crate::test_support::scenario_lineages(0..12),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: crate::state::core::GridSelectReason::DrawPileToHand,
    });

    let profile = summarize_pending_choice(&engine).expect("pending choice should profile");

    assert_eq!(profile.kind, "grid_select");
    assert_eq!(profile.candidate_count, 12);
    assert_eq!(profile.estimated_action_fanout, 12);
    assert_eq!(profile.fanout_class, "medium");
    assert_eq!(profile.search_risk, "exact_branching_pending_choice");
}

#[test]
fn scry_profile_marks_combinatorial_choices_as_high_fanout() {
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
        cards: vec![crate::content::cards::CardId::Strike; 7],
        card_lineages: crate::test_support::scenario_lineages(0..7),
    });

    let profile = summarize_pending_choice(&engine).expect("pending choice should profile");

    assert_eq!(profile.kind, "scry_select");
    assert_eq!(profile.candidate_count, 7);
    assert_eq!(profile.estimated_action_fanout, 128);
    assert_eq!(profile.fanout_class, "large");
    assert_eq!(profile.search_risk, "high_fanout_pending_choice");
}

#[test]
fn collector_reports_pending_choice_without_behavioral_claim() {
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::StanceChoice);
    let profile = summarize_pending_choice(&engine);
    let mut collector = PendingChoiceDiagnosticsCollector::default();

    collector.observe(profile.as_ref());
    let report = collector.finish();

    assert_eq!(
        report.behavioral_effect,
        "diagnostic_only_search_expansion_unchanged"
    );
    assert_eq!(report.pending_choice_states, 1);
    assert_eq!(report.max_candidate_count, 2);
    assert_eq!(report.kind_counts[0].kind, "stance_choice");
    assert_eq!(report.kind_counts[0].max_estimated_action_fanout, 2);
}
