use super::*;

#[test]
fn semantic_policy_keeps_attack_potion_when_ordinary_damage_is_uncovered() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![attacking_monster()];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Defend, 2),
    ];
    combat.entities.potions = vec![Some(Potion::new(PotionId::AttackPotion, 3))];
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
        CombatSearchV2PotionPolicy::SemanticBudgeted,
        None,
        None,
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
        .all(|choice| !matches!(choice.input, ClientInput::DiscardPotion(0))));
    assert_eq!(
        proposals::semantic_potion_gate_decision(
            &combat,
            &ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        )
        .reason,
        decision::PotionGateReason::VisibleIncomingUncoveredByHandBlock
    );
    assert_eq!(
        proposals::semantic_potion_gate_decision(
            &combat,
            &ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        )
        .role,
        Some(decision::PotionTacticalRole::PreventUncoveredDamage)
    );
}

#[test]
fn semantic_policy_rejects_attack_potion_when_only_pressure_is_no_lethal() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.current_hp = 65;
    monster.max_hp = 65;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::AttackPotion, 3))];

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
fn semantic_policy_keeps_attack_potion_in_high_stakes_no_lethal_state() {
    let mut combat = blank_test_combat();
    combat.meta.is_elite_fight = true;
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.current_hp = 65;
    monster.max_hp = 65;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::AttackPotion, 3))];

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
        decision::PotionGateReason::HighStakesNoVisibleHandLethal
    );
    assert_eq!(
        decision.role,
        Some(decision::PotionTacticalRole::HighStakesResourceConversion)
    );
}

#[test]
fn semantic_policy_does_not_admit_passive_fairy_use() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 3))];
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
        decision::PotionGateReason::PassiveOnly
    );
}

#[test]
fn semantic_policy_keeps_lethal_fire_potion_without_incoming_damage() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.current_hp = 20;
    monster.max_hp = 20;
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 3))];
    let legal = vec![CombatActionChoice::from_input(
        &combat,
        ClientInput::UsePotion {
            potion_index: 0,
            target: Some(1),
        },
    )];

    let filtered = filtered_legal_actions(
        legal,
        CombatSearchV2PotionPolicy::SemanticBudgeted,
        None,
        None,
        &combat,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(
        proposals::semantic_potion_gate_decision(
            &combat,
            &ClientInput::UsePotion {
                potion_index: 0,
                target: Some(1),
            },
        )
        .reason,
        decision::PotionGateReason::DirectDamageCanKill
    );
    assert_eq!(
        proposals::semantic_potion_tactical_role(
            &combat,
            &ClientInput::UsePotion {
                potion_index: 0,
                target: Some(1),
            },
        ),
        Some(decision::PotionTacticalRole::LethalDamage)
    );
}

#[test]
fn semantic_policy_keeps_explosive_potion_when_it_kills_any_enemy() {
    let mut combat = blank_test_combat();
    let mut low_hp = test_monster(EnemyId::LouseNormal);
    low_hp.current_hp = 10;
    low_hp.max_hp = 10;
    let mut high_hp = test_monster(EnemyId::JawWorm);
    high_hp.id = 2;
    high_hp.current_hp = 40;
    high_hp.max_hp = 40;
    combat.entities.monsters = vec![low_hp, high_hp];
    combat.entities.potions = vec![Some(Potion::new(PotionId::ExplosivePotion, 3))];

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
        decision::PotionGateReason::DirectDamageCanKill
    );
}

#[test]
fn semantic_policy_rejects_smoke_bomb_as_non_win_condition() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 3))];

    let decision = proposals::semantic_potion_gate_decision(
        &combat,
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert!(!decision.allowed);
    assert_eq!(decision.reason, decision::PotionGateReason::EscapeNotWin);
}
