use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::runtime::combat::CombatCard;
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn prefix_accumulates_same_turn_card_actions() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let transition = TurnBranchTransition::test_same_turn_play_card();

    let prefix = advance_turn_prefix(
        &TurnPrefixState::default(),
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        transition,
    );

    assert_eq!(prefix.prefix_length, 1);
    assert_eq!(prefix.cards_played, 1);
    assert_eq!(prefix.kind(), TurnPrefixKind::CardOnly);
    assert!(prefix
        .signature_preview
        .contains("card:Strike#constructed_scenario:10"));
}

#[test]
fn prefix_resets_after_next_turn_transition() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let current = TurnPrefixState {
        prefix_length: 1,
        cards_played: 1,
        signature_preview: "card:Strike#constructed_scenario:10".to_string(),
        ..TurnPrefixState::default()
    };
    let transition = TurnBranchTransition::test_next_turn_end_turn();

    let prefix = advance_turn_prefix(&current, &combat, &ClientInput::EndTurn, transition);

    assert_eq!(prefix.prefix_length, 0);
    assert!(prefix.signature_preview.is_empty());
}

#[test]
fn collector_reports_prefix_lengths_without_action_tree() {
    let mut collector = TurnPrefixDiagnosticsCollector::default();
    let prefix = TurnPrefixState {
        prefix_length: 2,
        cards_played: 2,
        signature_preview: "card:Strike#10>card:Bash#11".to_string(),
        ..TurnPrefixState::default()
    };

    collector.observe(&summarize_turn_prefix(&prefix, 5));
    let report = collector.finish();

    assert_eq!(
        report.behavioral_effect,
        "diagnostic_only_no_turn_prefix_prune_no_merge"
    );
    assert_eq!(report.states_observed, 1);
    assert_eq!(report.non_empty_prefix_states, 1);
    assert_eq!(report.max_prefix_length, 2);
    assert_eq!(report.prefix_length_counts[0].prefix_length, 2);
    assert_eq!(report.largest_prefix_fanouts.len(), 1);
}
