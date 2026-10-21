use super::super::{filtered_legal_actions, CombatSearchV2PotionPolicy};
use super::types::ActionExpansionKind;
use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::potions::{Potion, PotionId};
use crate::runtime::combat::CombatCard;
use crate::sim::combat_action::CombatActionChoice;
use crate::state::core::{ClientInput, EngineState};
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn targeted_card_actions_share_a_diagnostic_fanout_group() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 42)];
    let mut first = test_monster(EnemyId::JawWorm);
    first.id = 11;
    let mut second = test_monster(EnemyId::Cultist);
    second.id = 12;
    combat.entities.monsters = vec![first, second];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(11),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(12),
            },
        ),
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
    ];

    let summary = summarize_action_expansion(&EngineState::CombatPlayerTurn, &combat, &choices);

    assert_eq!(summary.action_count, 3);
    assert_eq!(summary.group_count, 2);
    assert!(summary.groups.iter().any(|group| {
        group.key.kind == ActionExpansionKind::PlayCard && group.action_count == 2
    }));
}

#[test]
fn potion_policy_filtered_actions_are_observed_after_filtering() {
    let mut combat = blank_test_combat();
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 7))];
    let legal = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        ),
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
    ];
    let filtered = filtered_legal_actions(
        legal,
        CombatSearchV2PotionPolicy::Never,
        None,
        None,
        &combat,
    );

    let summary = summarize_action_expansion(&EngineState::CombatPlayerTurn, &combat, &filtered);

    assert_eq!(summary.action_count, 1);
    assert!(summary.groups.iter().all(|group| {
        group.key.kind != ActionExpansionKind::UsePotion
            && group.key.kind != ActionExpansionKind::DiscardPotion
    }));
}

#[test]
fn exact_potion_slot_mask_filters_use_and_discard_before_expansion() {
    let mut combat = blank_test_combat();
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 7)),
        Some(Potion::new(PotionId::BlockPotion, 8)),
    ];
    let legal = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::UsePotion {
                potion_index: 1,
                target: None,
            },
        ),
        CombatActionChoice::from_input(&combat, ClientInput::DiscardPotion(0)),
        CombatActionChoice::from_input(&combat, ClientInput::DiscardPotion(1)),
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
    ];

    let filtered = filtered_legal_actions(
        legal,
        CombatSearchV2PotionPolicy::All,
        Some(1 << 1),
        None,
        &combat,
    );
    let inputs = filtered
        .into_iter()
        .map(|choice| choice.input)
        .collect::<Vec<_>>();

    assert_eq!(inputs.len(), 3);
    assert!(inputs.iter().any(|input| matches!(
        input,
        ClientInput::UsePotion {
            potion_index: 1,
            ..
        }
    )));
    assert!(inputs
        .iter()
        .any(|input| matches!(input, ClientInput::DiscardPotion(1))));
    assert!(inputs.iter().all(|input| !matches!(
        input,
        ClientInput::UsePotion {
            potion_index: 0,
            ..
        } | ClientInput::DiscardPotion(0)
    )));
}

#[test]
fn all_potion_use_can_explicitly_exclude_discard_before_expansion() {
    let mut combat = blank_test_combat();
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 7))];
    let legal = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        ),
        CombatActionChoice::from_input(&combat, ClientInput::DiscardPotion(0)),
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
    ];

    let filtered = filtered_legal_actions(
        legal,
        CombatSearchV2PotionPolicy::All,
        Some(1),
        Some(false),
        &combat,
    );

    assert!(filtered.iter().any(|choice| matches!(
        choice.input,
        ClientInput::UsePotion {
            potion_index: 0,
            ..
        }
    )));
    assert!(filtered
        .iter()
        .all(|choice| !matches!(choice.input, ClientInput::DiscardPotion(_))));
}

#[test]
fn expansion_collector_reports_largest_groups_without_changing_behavior() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 42)];
    let mut first = test_monster(EnemyId::JawWorm);
    first.id = 11;
    let mut second = test_monster(EnemyId::Cultist);
    second.id = 12;
    combat.entities.monsters = vec![first, second];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(11),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(12),
            },
        ),
    ];
    let summary = summarize_action_expansion(&EngineState::CombatPlayerTurn, &combat, &choices);
    let mut collector = ActionExpansionDiagnosticsCollector::default();

    collector.observe(&summary);
    let report = collector.finish();

    assert_eq!(
        report.behavioral_effect,
        "diagnostic_only_search_expansion_unchanged"
    );
    assert_eq!(report.total_atomic_actions, 2);
    assert_eq!(report.total_fanout_groups, 1);
    assert_eq!(report.max_group_size, 2);
    assert_eq!(report.largest_groups[0].kind, "play_card");
}
