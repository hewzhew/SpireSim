use super::*;

#[test]
fn semantic_policy_rejects_block_potion_without_visible_incoming_loss() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 3))];
    let legal = vec![CombatActionChoice::from_input(
        &combat,
        ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    )];

    let filtered = filtered_legal_actions(
        legal,
        CombatSearchV2PotionPolicy::SemanticBudgeted,
        None,
        None,
        &combat,
    );

    assert!(filtered.is_empty());
    assert_eq!(
        proposals::semantic_potion_gate_decision(
            &combat,
            &ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        )
        .reason,
        decision::PotionGateReason::NoVisibleIncomingHpLoss
    );
}

#[test]
fn semantic_policy_rejects_block_potion_when_hand_can_cover_visible_damage() {
    let mut combat = blank_test_combat();
    combat.entities.player.block = 0;
    combat.entities.monsters = vec![attacking_monster()];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 1),
        CombatCard::for_scenario(CardId::Defend, 2),
    ];
    combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        decision::PotionGateReason::VisibleIncomingFullyBlockable
    );
}

#[test]
fn semantic_policy_rejects_block_potion_with_only_ordinary_visible_incoming_loss() {
    let mut combat = blank_test_combat();
    combat.entities.player.block = 0;
    combat.entities.monsters = vec![attacking_monster()];
    combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        decision::PotionGateReason::NoTacticalPressure
    );
}

#[test]
fn semantic_policy_keeps_block_potion_with_high_stakes_visible_incoming_loss() {
    let mut combat = blank_test_combat();
    combat.meta.is_elite_fight = true;
    combat.entities.player.block = 0;
    combat.entities.monsters = vec![attacking_monster()];
    combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(decision.allowed);
    assert_eq!(
        decision.reason,
        decision::PotionGateReason::VisibleIncomingUncoveredByHandBlock
    );
}

#[test]
fn semantic_policy_keeps_block_potion_when_visible_attack_is_lethal() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 5;
    combat.entities.player.block = 0;
    combat.entities.monsters = vec![attacking_monster()];
    combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(decision.allowed);
    assert_eq!(
        decision.reason,
        decision::PotionGateReason::VisibleIncomingLethal
    );
    assert_eq!(
        decision.role,
        Some(decision::PotionTacticalRole::PreventVisibleLethal)
    );
}

#[test]
fn semantic_policy_treats_exact_zero_hp_after_visible_attack_as_lethal() {
    let mut combat = blank_test_combat();
    combat.entities.player.block = 0;
    combat.entities.monsters = vec![attacking_monster()];
    let incoming_damage = visible_incoming_damage(&combat);
    assert!(incoming_damage > 0);
    combat.entities.player.current_hp = incoming_damage;
    combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(decision.allowed);
    assert_eq!(
        decision.reason,
        decision::PotionGateReason::VisibleIncomingLethal
    );
}

#[test]
fn semantic_policy_rejects_blood_potion_when_only_ordinary_damage_is_uncovered() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 40;
    combat.entities.monsters = vec![attacking_monster()];
    combat.entities.potions = vec![Some(Potion::new(PotionId::BloodPotion, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        decision::PotionGateReason::NoTacticalPressure
    );
}

#[test]
fn semantic_policy_keeps_blood_potion_when_high_stakes_wounded_damage_is_uncovered() {
    let mut combat = blank_test_combat();
    combat.meta.is_elite_fight = true;
    combat.entities.player.current_hp = 40;
    combat.entities.monsters = vec![attacking_monster()];
    combat.entities.potions = vec![Some(Potion::new(PotionId::BloodPotion, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(decision.allowed);
    assert_eq!(
        decision.reason,
        decision::PotionGateReason::VisibleIncomingUncoveredByHandBlock
    );
}

#[test]
fn semantic_policy_keeps_fruit_juice_when_high_stakes_player_is_wounded() {
    let mut combat = blank_test_combat();
    combat.meta.is_elite_fight = true;
    combat.entities.player.current_hp = 70;
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::FruitJuice, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(decision.allowed);
    assert_eq!(decision.reason, decision::PotionGateReason::PlayerWounded);
}
