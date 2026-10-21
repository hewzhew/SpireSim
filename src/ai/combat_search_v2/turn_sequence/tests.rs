use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::runtime::combat::CombatCard;
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn collector_detects_same_effect_order_variants() {
    let mut collector = TurnSequenceDiagnosticsCollector::default();

    collector.observe(&summary(
        "origin",
        "card:Strike#1>card:Defend#2",
        "card:Defend#2>card:Strike#1",
        "effect",
    ));
    collector.observe(&summary(
        "origin",
        "card:Defend#2>card:Strike#1",
        "card:Defend#2>card:Strike#1",
        "effect",
    ));

    let report = collector.finish();

    assert_eq!(report.states_observed, 2);
    assert_eq!(report.groups_with_order_variants, 1);
    assert_eq!(report.same_effect_order_variant_groups, 1);
    assert_eq!(report.order_sensitive_groups, 0);
    assert_eq!(
        report.largest_groups[0].group_class,
        "same_effect_order_variants"
    );
}

#[test]
fn collector_detects_order_sensitive_groups() {
    let mut collector = TurnSequenceDiagnosticsCollector::default();

    collector.observe(&summary("origin", "A>B", "A>B", "effect_1"));
    collector.observe(&summary("origin", "B>A", "A>B", "effect_2"));

    let report = collector.finish();

    assert_eq!(report.groups_with_order_variants, 1);
    assert_eq!(report.same_effect_order_variant_groups, 0);
    assert_eq!(report.order_sensitive_groups, 1);
    assert_eq!(report.max_effect_variants_per_group, 2);
}

#[test]
fn summarize_turn_sequence_uses_non_empty_combat_prefix() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let mut node = test_node(combat.clone());
    let transition = TurnBranchTransition::test_same_turn_play_card();
    node.note_turn_prefix(
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        transition,
    );

    let summary = summarize_turn_sequence(&node, 3);

    assert_eq!(summary.prefix_length, 1);
    assert_eq!(summary.legal_actions, 3);
    assert!(summary.origin_key.is_some());
    assert!(summary
        .ordered_key
        .as_deref()
        .is_some_and(|key| key.contains("card:Strike")));
    assert!(summary.unordered_key.is_some());
    assert!(summary.effect_key.is_some());
}

fn summary(
    origin_key: &str,
    ordered_key: &str,
    unordered_key: &str,
    effect_key: &str,
) -> TurnSequenceSummary {
    TurnSequenceSummary {
        prefix_length: 2,
        legal_actions: 5,
        origin_key: Some(origin_key.to_string()),
        ordered_key: Some(ordered_key.to_string()),
        unordered_key: Some(unordered_key.to_string()),
        effect_key: Some(effect_key.to_string()),
        effect_fingerprint: None,
    }
}

fn test_node(combat: CombatState) -> SearchNode {
    SearchNode {
        engine: EngineState::CombatPlayerTurn,
        combat,
        actions: Vec::new(),
        turn_prefix: TurnPrefixState::default(),
        initial_hp: 80,
        potions_used: 0,
        potions_discarded: 0,
        cards_played: 0,
        potion_tactical_priority: 0,
        last_turn_branch_priority: 0,
        action_prior_score: None,
        action_ordering_frontier_hint: 0,
        rollout_estimate: RolloutNodeEstimate::unevaluated(),
        root_lineage: Default::default(),
    }
}
