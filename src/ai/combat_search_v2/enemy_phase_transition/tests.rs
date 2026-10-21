use super::super::enemy_mechanics_profile::enemy_mechanics_profile;
use super::*;
use crate::content::cards::CardId;
use crate::runtime::combat::{CombatCard, Power, PowerPayload};
use crate::test_support::{blank_test_combat, test_monster};

fn test_power(power_type: PowerId, amount: i32) -> Power {
    Power {
        power_type,
        instance_id: None,
        amount,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    }
}

fn awakened_transition_test_combat(hp: i32, strength: i32) -> crate::runtime::combat::CombatState {
    let mut combat = blank_test_combat();
    let mut awakened = test_monster(EnemyId::AwakenedOne);
    awakened.id = 7;
    awakened.current_hp = hp;
    awakened.max_hp = 300;
    awakened.awakened_one.form1 = true;
    combat.entities.monsters = vec![awakened];
    combat
        .entities
        .power_db
        .insert(7, vec![test_power(PowerId::Strength, strength)]);
    combat
}

#[test]
fn temporary_strength_down_reports_reachable_form_one_lethal_window() {
    let mut combat = awakened_transition_test_combat(12, 4);
    combat.turn.energy = 2;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::DarkShackles, 10),
        CombatCard::for_scenario(CardId::Carnage, 11),
    ];
    let effects = super::super::action_effects::card_play_effect_facts(
        &combat,
        &combat.zones.hand[0],
        Some(7),
    );

    let opportunity = awakened_one_strength_transition_opportunity(
        &combat,
        0,
        Some(7),
        enemy_mechanics_profile(&combat),
        effects,
    )
    .expect("reachable transition window");

    assert_eq!(opportunity.temporary_strength_down, 9);
    assert_eq!(opportunity.convertible_positive_strength, 4);
    assert!(opportunity.remaining_damage_upper_bound >= 12);
    assert_eq!(opportunity.phase_one_hp_with_block, 12);
}

#[test]
fn persistent_strength_loss_reduces_conversion_without_closing_window() {
    let mut combat = awakened_transition_test_combat(12, 2);
    combat.turn.energy = 2;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::DarkShackles, 10),
        CombatCard::for_scenario(CardId::Carnage, 11),
    ];
    let effects = super::super::action_effects::card_play_effect_facts(
        &combat,
        &combat.zones.hand[0],
        Some(7),
    );

    let opportunity = awakened_one_strength_transition_opportunity(
        &combat,
        0,
        Some(7),
        enemy_mechanics_profile(&combat),
        effects,
    )
    .expect("Disarm-reduced positive Strength still leaves a transition window");

    assert_eq!(opportunity.convertible_positive_strength, 2);
}

#[test]
fn transition_opportunity_requires_form_one_strength_temporary_loss_and_damage() {
    let mut form_two = awakened_transition_test_combat(6, 4);
    form_two.entities.monsters[0].awakened_one.form1 = false;
    form_two.turn.energy = 2;
    form_two.zones.hand = vec![
        CombatCard::for_scenario(CardId::DarkShackles, 10),
        CombatCard::for_scenario(CardId::Carnage, 11),
    ];

    let mut zero_strength = awakened_transition_test_combat(6, 0);
    zero_strength.turn.energy = 2;
    zero_strength.zones.hand = form_two.zones.hand.clone();

    let mut insufficient = awakened_transition_test_combat(40, 4);
    insufficient.turn.energy = 2;
    insufficient.zones.hand = form_two.zones.hand.clone();

    let mut persistent_only = awakened_transition_test_combat(6, 4);
    persistent_only.turn.energy = 2;
    persistent_only.zones.hand = vec![
        CombatCard::for_scenario(CardId::Disarm, 10),
        CombatCard::for_scenario(CardId::Carnage, 11),
    ];

    for (label, combat) in [
        ("form-two", form_two),
        ("zero-strength", zero_strength),
        ("insufficient-damage", insufficient),
        ("persistent-only", persistent_only),
    ] {
        let effects = super::super::action_effects::card_play_effect_facts(
            &combat,
            &combat.zones.hand[0],
            Some(7),
        );
        assert!(
            awakened_one_strength_transition_opportunity(
                &combat,
                0,
                Some(7),
                enemy_mechanics_profile(&combat),
                effects,
            )
            .is_none(),
            "unexpected opportunity for {label}"
        );
    }
}

#[test]
fn detects_large_slime_split_trigger_from_card_damage() {
    let mut combat = blank_test_combat();
    let mut slime = test_monster(EnemyId::AcidSlimeL);
    slime.id = 11;
    slime.current_hp = 40;
    slime.max_hp = 65;
    combat.entities.monsters = vec![slime];
    combat
        .entities
        .power_db
        .insert(11, vec![test_power(PowerId::Split, -1)]);
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Carnage, 20)];

    let hint = enemy_phase_transition_hint_for_input(
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(11),
        },
        CombatSearchPhaseGuardPluginId::Default,
    );

    assert_eq!(hint.split_trigger_count, 1);
    assert!(hint.split_debt_hp > 0);
    assert!(hint.ordering_risk_score() > 0);
}

#[test]
fn lethal_split_monster_damage_is_not_split_debt() {
    let mut combat = blank_test_combat();
    let mut slime = test_monster(EnemyId::AcidSlimeL);
    slime.id = 11;
    slime.current_hp = 20;
    slime.max_hp = 65;
    combat.entities.monsters = vec![slime];
    combat
        .entities
        .power_db
        .insert(11, vec![test_power(PowerId::Split, -1)]);
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Carnage, 20)];

    let hint = enemy_phase_transition_hint_for_input(
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(11),
        },
        CombatSearchPhaseGuardPluginId::Default,
    );

    assert_eq!(hint.split_trigger_count, 0);
    assert_eq!(hint.split_debt_hp, 0);
}

#[test]
fn detects_slime_boss_split_trigger_from_card_damage() {
    let mut combat = blank_test_combat();
    let mut slime = test_monster(EnemyId::SlimeBoss);
    slime.id = 14;
    slime.current_hp = 80;
    slime.max_hp = 140;
    combat.entities.monsters = vec![slime];
    combat
        .entities
        .power_db
        .insert(14, vec![test_power(PowerId::Split, -1)]);
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Carnage, 20)];

    let hint = enemy_phase_transition_hint_for_input(
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(14),
        },
        CombatSearchPhaseGuardPluginId::Default,
    );

    assert_eq!(hint.split_trigger_count, 1);
    assert!(hint.split_debt_hp > 0);
}

#[test]
fn detects_guardian_mode_shift_trigger_from_card_damage() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 12;
    guardian.current_hp = 100;
    guardian.max_hp = 240;
    guardian.guardian.is_open = true;
    combat.entities.monsters = vec![guardian];
    combat
        .entities
        .power_db
        .insert(12, vec![test_power(PowerId::ModeShift, 5)]);
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 21)];

    let hint = enemy_phase_transition_hint_for_input(
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(12),
        },
        CombatSearchPhaseGuardPluginId::Default,
    );

    assert_eq!(hint.guardian_mode_shift_trigger_count, 1);
    assert_eq!(hint.guardian_min_threshold_remaining_before_hit, Some(5));
}

#[test]
fn detects_lagavulin_wake_risk_from_card_damage() {
    let mut combat = blank_test_combat();
    let mut lagavulin = test_monster(EnemyId::Lagavulin);
    lagavulin.id = 13;
    lagavulin.lagavulin.is_out = false;
    combat.entities.monsters = vec![lagavulin];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 22)];

    let hint = enemy_phase_transition_hint_for_input(
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(13),
        },
        CombatSearchPhaseGuardPluginId::Default,
    );

    assert_eq!(hint.lagavulin_wake_risk_count, 1);
}
