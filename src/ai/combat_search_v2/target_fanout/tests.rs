use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::potions::{Potion, PotionId};
use crate::runtime::combat::CombatCard;
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn groups_same_card_across_multiple_targets() {
    let mut combat = blank_test_combat();
    let mut low_hp = test_monster(EnemyId::LouseNormal);
    low_hp.current_hp = 6;
    low_hp.max_hp = 6;
    low_hp.id = 1;
    let mut high_hp = test_monster(EnemyId::JawWorm);
    high_hp.current_hp = 30;
    high_hp.max_hp = 30;
    high_hp.id = 2;
    combat.entities.monsters = vec![low_hp, high_hp];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(2),
            },
        ),
    ];

    let summary = summarize_target_fanout(&combat, &choices);

    assert_eq!(summary.targeted_actions, 2);
    assert_eq!(summary.groups.len(), 1);
    assert_eq!(summary.groups[0].target_count, 2);
    assert_eq!(summary.groups[0].lethal_targets, 1);
    assert_eq!(summary.groups[0].target_hp_span(), 24);
}

#[test]
fn collector_reports_largest_fanouts_without_pruning_claim() {
    let mut combat = blank_test_combat();
    let mut first = test_monster(EnemyId::LouseNormal);
    first.current_hp = 6;
    first.max_hp = 6;
    first.id = 1;
    let mut second = test_monster(EnemyId::JawWorm);
    second.current_hp = 30;
    second.max_hp = 30;
    second.id = 2;
    combat.entities.monsters = vec![first, second];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let summary = summarize_target_fanout(
        &combat,
        &[
            CombatActionChoice::from_input(
                &combat,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(1),
                },
            ),
            CombatActionChoice::from_input(
                &combat,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(2),
                },
            ),
        ],
    );
    let mut collector = TargetFanoutDiagnosticsCollector::default();

    collector.observe(&summary);
    let report = collector.finish();

    assert_eq!(
        report.behavioral_effect,
        "diagnostic_only_no_target_prune_no_merge"
    );
    assert_eq!(report.targeted_actions_total, 2);
    assert_eq!(report.multi_target_fanout_groups, 1);
    assert_eq!(report.lethal_target_groups, 1);
    assert_eq!(report.unique_lethal_target_groups, 1);
    assert_eq!(report.largest_target_fanouts.len(), 1);
}

#[test]
fn fire_potion_target_fanout_reports_damage_hint() {
    let mut combat = blank_test_combat();
    let mut first = test_monster(EnemyId::LouseNormal);
    first.current_hp = 20;
    first.max_hp = 20;
    first.id = 1;
    let mut second = test_monster(EnemyId::JawWorm);
    second.current_hp = 30;
    second.max_hp = 30;
    second.id = 2;
    combat.entities.monsters = vec![first, second];
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 77))];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::UsePotion {
                potion_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::UsePotion {
                potion_index: 0,
                target: Some(2),
            },
        ),
    ];

    let summary = summarize_target_fanout(&combat, &choices);

    assert_eq!(summary.groups.len(), 1);
    assert_eq!(summary.groups[0].min_damage_hint, 20);
    assert_eq!(summary.groups[0].max_damage_hint, 20);
    assert_eq!(summary.groups[0].lethal_targets, 1);
}
