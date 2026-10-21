use super::*;
use crate::content::cards::CardId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CardLineageId, CombatCard};
use crate::test_support::blank_test_combat;

#[test]
fn duplicate_active_lineage_is_reported() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Defend, 10),
    ];

    let summary = summarize_card_identity(&combat);
    let mut collector = CardIdentityDiagnosticsCollector::default();
    collector.observe(&summary);
    let report = collector.finish();

    assert_eq!(report.states_with_duplicate_active_lineage, 1);
    assert_eq!(report.states_with_lineage_card_id_conflict, 1);
    assert_eq!(
        report.largest_conflict_groups[0].lineage,
        CardLineageId::constructed_scenario(10)
    );
    assert_eq!(report.largest_conflict_groups[0].occurrence_count, 2);
}

#[test]
fn action_payload_constructed_card_is_not_active_duplicate() {
    let mut combat = blank_test_combat();
    combat
        .engine
        .action_queue
        .push_back(Action::MakeCopyInHand {
            original: Box::new(CombatCard::for_scenario(CardId::Strike, 0)),
            amount: 1,
        });

    let summary = summarize_card_identity(&combat);
    let mut collector = CardIdentityDiagnosticsCollector::default();
    collector.observe(&summary);
    let report = collector.finish();

    assert_eq!(report.states_with_duplicate_active_lineage, 0);
    assert_eq!(report.action_payload_cards_observed, 1);
    assert_eq!(report.action_payload_constructed_cards, 1);
}
