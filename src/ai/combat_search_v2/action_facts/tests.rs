use crate::content::cards::{CardId, CardType};
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::runtime::combat::{CardLineageId, CombatCard, Power, PowerPayload};
use crate::runtime::monster_move::{BuffSpec, MonsterMoveSpec};
use crate::sim::combat::EngineCombatStepper;
use crate::state::core::{GridSelectReason, PendingChoice, PileType};
use crate::state::selection::{SelectionResolution, SelectionScope};
use crate::test_support::{blank_test_combat, planned_monster, test_monster};

use super::*;

#[test]
fn facts_preserve_pending_choice_owner_ordering_without_flattening_it() {
    let mut combat = blank_test_combat();
    combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Carnage, 20)].into();
    let engine = EngineState::PendingChoice(PendingChoice::GridSelect {
        source_pile: PileType::Discard,
        candidate_lineages: vec![CardLineageId::constructed_scenario(20)],
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: GridSelectReason::MoveToDrawPile,
    });
    let input = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Grid,
        [CardLineageId::constructed_scenario(20)],
    ));

    let facts = summarize_action_facts(&engine, &combat, &input, &EngineCombatStepper, 250);
    let selection = facts
        .pending_choice_selection
        .expect("pending choice selection facts");

    assert_eq!(
        selection.role,
        CombatSearchV2ActionPendingChoiceSelectionRole::Value
    );
    assert!(selection.primary > 0);
    assert_eq!(selection.selected_count_tiebreak, -1);
    assert_eq!(
        selection.prepared_draw_card_lineage,
        Some(CardLineageId::constructed_scenario(20))
    );
}

#[test]
fn facts_report_card_definition_and_exact_delta_for_strike() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        &EngineCombatStepper,
        250,
    );

    let card = facts.card.expect("strike card facts");
    assert_eq!(card.name, "Strike");
    assert_eq!(card.card_type, CardType::Attack);
    assert!(facts.immediate.damage_hint > 0);
    assert!(facts.exact_one_step_delta.total_enemy_hp_delta < 0);
}

#[test]
fn facts_report_timed_enemy_threat_on_target() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let mut exploder = test_monster(EnemyId::Exploder);
    exploder.id = 1;
    exploder.current_hp = 30;
    exploder.max_hp = 30;
    combat.entities.monsters = vec![exploder];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Explosive,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        &EngineCombatStepper,
        250,
    );

    let threat = facts
        .target
        .and_then(|target| target.timed_enemy_threat)
        .expect("Explosive target should expose a timed threat");
    assert_eq!(threat.kind, "forced_player_damage");
    assert_eq!(threat.owner_turns_until_trigger, 3);
    assert_eq!(threat.raw_player_damage, 30);
    assert!(threat.canceled_by_owner_death);
}

#[test]
fn facts_report_attack_retaliation_on_target() {
    let mut combat = blank_test_combat();
    combat.entities.player.block = 2;
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let mut spiker = test_monster(EnemyId::Spiker);
    spiker.id = 1;
    spiker.set_planned_steps(
        MonsterMoveSpec::Buff(BuffSpec {
            power_id: PowerId::Thorns,
            amount: 2,
        })
        .to_steps(),
    );
    combat.entities.monsters = vec![spiker];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Thorns,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        &EngineCombatStepper,
        250,
    );

    let retaliation = facts
        .target
        .and_then(|target| target.attack_retaliation)
        .expect("retaliating target should expose attack retaliation");
    assert_eq!(retaliation.power_source_count, 1);
    assert_eq!(retaliation.raw_player_damage_per_damage_event, 3);
    assert_eq!(
        retaliation.projected_player_block_loss_for_next_damage_event,
        2
    );
    assert_eq!(
        retaliation.projected_player_hp_loss_for_next_damage_event,
        1
    );
    assert_eq!(retaliation.visible_growth_amount, 2);
}

#[test]
fn facts_report_action_payload_damage_for_multi_hit_card() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::TwinStrike, 10)];
    combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        &EngineCombatStepper,
        250,
    );

    assert_eq!(
        facts.card.as_ref().map(|card| card.evaluated_damage),
        Some(5)
    );
    assert_eq!(facts.immediate.action_payload_damage_hint, 10);
    assert_eq!(facts.immediate.action_payload_damage_hit_count_hint, 2);
    assert_eq!(facts.immediate.target_progress_hint, 10);
    assert_eq!(facts.exact_one_step_delta.total_enemy_hp_delta, -10);
}

#[test]
fn facts_report_nob_anger_from_reactive_power_without_card_tag() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 10)];
    let mut nob = test_monster(EnemyId::GremlinNob);
    nob.id = 1;
    combat.entities.monsters = vec![nob];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Anger,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        &EngineCombatStepper,
        250,
    );

    assert_eq!(facts.card.as_ref().map(|card| card.name), Some("Defend"));
    assert!(facts.immediate.block_hint > 0);
    assert!(facts.mechanics.reactive.enemy_strength_gain > 0);
    assert!(facts.mechanics.derived.enemy_strength_gain > 0);
}

#[test]
fn facts_report_player_strength_gain_without_enemy_scaling_risk() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Flex, 10)];

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        &EngineCombatStepper,
        250,
    );

    assert_eq!(facts.card.as_ref().map(|card| card.name), Some("Flex"));
    assert_eq!(facts.mechanics.direct.player_strength_gain, 2);
    assert_eq!(facts.mechanics.direct.player_temporary_strength_gain, 2);
    assert_eq!(facts.mechanics.derived.enemy_strength_gain, 0);
}

#[test]
fn facts_report_berserk_recurring_energy_and_vulnerable_cost() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Berserk, 10)];

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        &EngineCombatStepper,
        250,
    );

    assert_eq!(facts.card.as_ref().map(|card| card.name), Some("Berserk"));
    assert_eq!(facts.mechanics.direct.player_recurring_turn_energy_delta, 1);
    assert_eq!(facts.mechanics.derived.reactive_risk_score, 2);
}

#[test]
fn facts_report_double_tap_attack_replay_charges() {
    let mut combat = blank_test_combat();
    let mut double_tap = CombatCard::for_scenario(CardId::DoubleTap, 10);
    double_tap.upgrades = 1;
    combat.zones.hand = vec![double_tap];

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        &EngineCombatStepper,
        250,
    );

    assert_eq!(facts.mechanics.direct.player_attack_replay_charges, 2);
}

#[test]
fn facts_report_slow_as_future_attack_damage_amplification() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Evolve, 10)];
    let monster = planned_monster(EnemyId::GiantHead, 1);
    combat.entities.monsters = vec![monster];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Slow,
            instance_id: None,
            amount: 0,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        &EngineCombatStepper,
        250,
    );

    assert_eq!(
        facts
            .mechanics
            .reactive
            .future_attack_damage_percent_gain_hint,
        10
    );
}

#[test]
fn exact_delta_reports_conditional_spot_weakness_strength() {
    for (move_id, applies) in [(1, true), (2, false)] {
        for upgrades in [0, 1] {
            let mut combat = blank_test_combat();
            let mut card = CombatCard::for_scenario(CardId::SpotWeakness, 10);
            card.upgrades = upgrades;
            combat.zones.hand = vec![card];
            combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, move_id)];

            let facts = summarize_action_facts(
                &EngineState::CombatPlayerTurn,
                &combat,
                &ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(1),
                },
                &EngineCombatStepper,
                250,
            );
            let expected = if applies { 3 + i32::from(upgrades) } else { 0 };
            assert_eq!(facts.exact_one_step_delta.player_strength_delta, expected);
            assert_eq!(facts.mechanics.direct.player_strength_gain, expected);
            assert_eq!(facts.mechanics.direct.player_temporary_strength_gain, 0);
        }
    }
}

#[test]
fn facts_report_effective_ethereal_after_upgrade_sensitive_overrides() {
    let mut combat = blank_test_combat();
    let mut echo_form = CombatCard::for_scenario(CardId::EchoForm, 10);
    echo_form.upgrades = 1;
    combat.zones.hand = vec![echo_form];

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        &EngineCombatStepper,
        250,
    );

    let card = facts.card.expect("echo form card facts");
    assert_eq!(card.name, "Echo Form");
    assert!(!card.ethereal, "Echo Form+ should not be reported ethereal");
}

#[test]
fn facts_report_dropkick_contextual_draw_and_energy_delta_from_simulator() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Dropkick, 10)];
    combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 11)]).into();
    let mut monster = planned_monster(EnemyId::JawWorm, 1);
    monster.id = 1;
    combat.entities.monsters = vec![monster];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Vulnerable,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        &EngineCombatStepper,
        250,
    );

    assert_eq!(
        facts.target.as_ref().map(|target| target.vulnerable),
        Some(2)
    );
    assert_eq!(facts.exact_one_step_delta.energy_delta, 0);
    assert_eq!(facts.exact_one_step_delta.draw_delta, -1);
    assert_eq!(facts.exact_one_step_delta.hand_delta, 0);
}

#[test]
fn facts_report_fiend_fire_hand_resource_conversion_timing() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::FiendFire, 10),
        CombatCard::for_scenario(CardId::Offering, 11),
        CombatCard::for_scenario(CardId::SpotWeakness, 12),
        CombatCard::for_scenario(CardId::Strike, 13),
    ];
    let mut monster = planned_monster(EnemyId::JawWorm, 1);
    monster.id = 1;
    monster.current_hp = 80;
    monster.max_hp = 80;
    combat.entities.monsters = vec![monster];

    let facts = summarize_action_facts(
        &EngineState::CombatPlayerTurn,
        &combat,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        &EngineCombatStepper,
        250,
    );

    assert_eq!(
        facts.card.as_ref().map(|card| card.name),
        Some("Fiend Fire")
    );
    assert_eq!(facts.mechanics.resource_timing.hand_exhaust_target_count, 3);
    assert_eq!(facts.mechanics.resource_timing.hand_exhaust_fuel_count, 1);
    assert!(facts.mechanics.resource_timing.hand_exhaust_value_at_risk > 0);
    assert!(facts.mechanics.resource_timing.premature_conversion_risk > 0);
    assert_eq!(facts.mechanics.resource_timing.conversion_damage_hint, 21);
    assert_eq!(facts.immediate.damage_hint, 21);
    assert_eq!(facts.immediate.action_payload_damage_hit_count_hint, 3);
}
