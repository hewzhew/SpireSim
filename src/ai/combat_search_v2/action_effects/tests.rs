use super::*;
use crate::ai::combat_search_v2::visible_incoming_damage;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::runtime::combat::{CombatCard, Power, PowerPayload};
use crate::test_support::{blank_test_combat, planned_monster, test_monster};

#[test]
fn disarm_reports_persistent_enemy_strength_down_without_card_id_special_case() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 1;
    guardian.set_planned_move_id(6);
    combat.entities.monsters = vec![guardian];
    let disarm = CombatCard::for_scenario(CardId::Disarm, 10);

    let facts = card_play_effect_facts(&combat, &disarm, Some(1));

    assert!(facts.direct.persistent_enemy_strength_down > 0);
    assert_eq!(facts.direct.temporary_enemy_strength_down, 0);
}

#[test]
fn artifact_blocks_disarm_in_action_effect_facts() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 1;
    guardian.set_planned_move_id(6);
    combat.entities.monsters = vec![guardian];
    insert_power(&mut combat, 1, PowerId::Artifact, 1);

    let facts = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::Disarm, 10),
        Some(1),
    );

    assert_eq!(facts.direct.persistent_enemy_strength_down, 0);
    assert_eq!(facts.direct.temporary_enemy_strength_down, 0);
    assert_eq!(facts.direct.visible_attack_mitigation_hint, 0);
}

#[test]
fn direct_block_facts_follow_card_actions_not_dexterity_mutation_fields() {
    let mut combat = blank_test_combat();
    insert_power(&mut combat, 0, PowerId::Dexterity, 6);

    let disarm = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::Disarm, 10),
        Some(1),
    );
    let defend =
        card_play_effect_facts(&combat, &CombatCard::for_scenario(CardId::Defend, 11), None);

    assert_eq!(disarm.direct.player_block, 0);
    assert_eq!(defend.direct.player_block, 11);
}

#[test]
fn intangible_gain_reports_exact_visible_attack_mitigation() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![planned_monster(EnemyId::TimeEater, 2)];

    let apparition = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::Apparition, 12),
        None,
    );

    assert_eq!(visible_incoming_damage(&combat), 21);
    assert_eq!(
        apparition.direct.visible_attack_mitigation_hint, 18,
        "three visible hits are reduced from 21 total damage to 3"
    );
}

#[test]
fn intangible_gain_without_visible_attack_has_no_immediate_mitigation_claim() {
    let mut combat = blank_test_combat();
    let mut cultist = test_monster(EnemyId::Cultist);
    cultist.id = 1;
    combat.entities.monsters = vec![cultist];

    let apparition = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::Apparition, 12),
        None,
    );

    assert_eq!(apparition.direct.visible_attack_mitigation_hint, 0);
}

#[test]
fn additional_intangible_does_not_claim_the_same_visible_mitigation_twice() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![planned_monster(EnemyId::TimeEater, 2)];
    insert_power(&mut combat, 0, PowerId::IntangiblePlayer, 1);

    let apparition = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::Apparition, 12),
        None,
    );

    assert_eq!(visible_incoming_damage(&combat), 3);
    assert_eq!(apparition.direct.visible_attack_mitigation_hint, 0);
}

#[test]
fn artifact_is_consumed_in_card_action_order_before_later_debuffs() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 1;
    combat.entities.monsters = vec![guardian];
    insert_power(&mut combat, 1, PowerId::Artifact, 1);

    let facts = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::Shockwave, 10),
        None,
    );

    assert_eq!(facts.direct.enemy_weak, 0, "the first debuff is absorbed");
    assert_eq!(
        facts.direct.enemy_vulnerable, 3,
        "the later debuff applies after Artifact is consumed"
    );
}

#[test]
fn enough_artifact_blocks_every_direct_debuff_from_one_card() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 1;
    combat.entities.monsters = vec![guardian];
    insert_power(&mut combat, 1, PowerId::Artifact, 2);

    let facts = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::Shockwave, 10),
        None,
    );

    assert_eq!(facts.direct.enemy_weak, 0);
    assert_eq!(facts.direct.enemy_vulnerable, 0);
}

#[test]
fn player_artifact_makes_flex_strength_persistent_in_action_effect_facts() {
    let mut combat = blank_test_combat();
    insert_power(&mut combat, 0, PowerId::Artifact, 1);

    let facts = card_play_effect_facts(&combat, &CombatCard::for_scenario(CardId::Flex, 10), None);

    assert_eq!(facts.direct.player_strength_gain, 2);
    assert_eq!(facts.direct.player_temporary_strength_gain, 0);
}

#[test]
fn anger_power_reports_enemy_strength_gain_for_skill_without_monster_special_case() {
    let mut combat = blank_test_combat();
    let mut nob = test_monster(EnemyId::GremlinNob);
    nob.id = 1;
    combat.entities.monsters = vec![nob];
    insert_power(&mut combat, 1, PowerId::Anger, 2);

    let defend = CombatCard::for_scenario(CardId::Defend, 10);
    let strike = CombatCard::for_scenario(CardId::Strike, 11);

    let defend_facts = card_play_effect_facts(&combat, &defend, None);
    let strike_facts = card_play_effect_facts(&combat, &strike, Some(1));

    assert!(defend_facts.reactive.enemy_strength_gain > 0);
    assert!(defend_facts.enemy_scaling_risk_score() > 0);
    assert_eq!(strike_facts.reactive.enemy_strength_gain, 0);
}

#[test]
fn flex_reports_player_strength_gain_without_enemy_scaling_risk() {
    let combat = blank_test_combat();
    let flex = CombatCard::for_scenario(CardId::Flex, 10);

    let facts = card_play_effect_facts(&combat, &flex, None);

    assert_eq!(facts.direct.player_strength_gain, 2);
    assert_eq!(facts.direct.player_temporary_strength_gain, 2);
    assert_eq!(facts.direct.enemy_strength_gain, 0);
    assert_eq!(facts.enemy_scaling_risk_score(), 0);
}

#[test]
fn spot_weakness_strength_facts_follow_runtime_intent_and_upgrade() {
    for (move_id, damage, applies) in [(3, 7, true), (3, 0, true), (4, 7, false)] {
        for upgrades in [0, 1] {
            let mut combat = blank_test_combat();
            let mut louse = test_monster(EnemyId::LouseNormal);
            louse.id = 7;
            louse.louse.bite_damage = Some(damage);
            louse.set_planned_move_id(move_id);
            combat.entities.monsters = vec![louse];
            let mut card = CombatCard::for_scenario(CardId::SpotWeakness, 10);
            card.upgrades = upgrades;
            let facts = card_play_effect_facts(&combat, &card, Some(7));
            let mut resolved = combat.clone();
            for action in crate::content::cards::resolve_card_play(card.id, &combat, &card, Some(7))
            {
                crate::engine::action_handlers::execute_action(action.action, &mut resolved);
            }
            while let Some(action) = resolved.pop_next_action() {
                crate::engine::action_handlers::execute_action(action, &mut resolved);
            }
            let expected = if applies { 3 + i32::from(upgrades) } else { 0 };
            assert_eq!(resolved.get_power(0, PowerId::Strength), expected);
            assert_eq!(
                facts.direct.player_strength_gain, expected,
                "move={move_id}, damage={damage}, upgrades={upgrades}"
            );
            assert_eq!(facts.direct.player_temporary_strength_gain, 0);
        }
    }
    let combat = blank_test_combat();
    let demon = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::DemonForm, 11),
        None,
    );
    assert_eq!(
        demon.direct.player_strength_gain, 0,
        "future turn growth is not immediate Strength"
    );
}

#[test]
fn sharp_hide_reports_reactive_player_hp_loss_for_attack() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 1;
    combat.entities.monsters = vec![guardian];
    insert_power(&mut combat, 1, PowerId::SharpHide, 3);

    let strike = CombatCard::for_scenario(CardId::Strike, 10);
    let defend = CombatCard::for_scenario(CardId::Defend, 11);

    let strike_facts = card_play_effect_facts(&combat, &strike, Some(1));
    let defend_facts = card_play_effect_facts(&combat, &defend, None);

    assert_eq!(strike_facts.reactive.player_hp_loss, 3);
    assert_eq!(
        strike_facts.reactive.attack_retaliation_trigger_count_hint,
        0
    );
    assert_eq!(
        strike_facts.reactive.attack_retaliation_player_hp_loss_hint,
        0
    );
    assert_eq!(defend_facts.reactive.player_hp_loss, 0);
}

#[test]
fn attack_retaliation_counts_explicit_damage_events_without_affecting_non_attacks() {
    let mut combat = blank_test_combat();
    let mut spiker = test_monster(EnemyId::Spiker);
    spiker.id = 1;
    combat.entities.monsters = vec![spiker];
    insert_power(&mut combat, 1, PowerId::Thorns, 3);

    let strike = CombatCard::for_scenario(CardId::Strike, 10);
    let twin_strike = CombatCard::for_scenario(CardId::TwinStrike, 11);
    let defend = CombatCard::for_scenario(CardId::Defend, 12);

    let strike_facts = card_play_effect_facts(&combat, &strike, Some(1));
    let twin_strike_facts = card_play_effect_facts(&combat, &twin_strike, Some(1));
    let defend_facts = card_play_effect_facts(&combat, &defend, None);

    assert_eq!(
        strike_facts.reactive.attack_retaliation_trigger_count_hint,
        1
    );
    assert_eq!(
        strike_facts.reactive.attack_retaliation_player_hp_loss_hint,
        3
    );
    assert_eq!(strike_facts.reactive.player_hp_loss, 3);
    assert_eq!(
        twin_strike_facts
            .reactive
            .attack_retaliation_trigger_count_hint,
        2
    );
    assert_eq!(
        twin_strike_facts
            .reactive
            .attack_retaliation_player_hp_loss_hint,
        6
    );
    assert_eq!(twin_strike_facts.reactive.player_hp_loss, 6);
    assert_eq!(
        defend_facts.reactive.attack_retaliation_trigger_count_hint,
        0
    );
    assert_eq!(
        defend_facts.reactive.attack_retaliation_player_hp_loss_hint,
        0
    );
    assert_eq!(defend_facts.reactive.player_hp_loss, 0);
}

#[test]
fn attack_retaliation_consumes_current_block_sequentially_before_hp() {
    let mut combat = blank_test_combat();
    combat.entities.player.block = 5;
    let mut spiker = test_monster(EnemyId::Spiker);
    spiker.id = 1;
    combat.entities.monsters = vec![spiker];
    insert_power(&mut combat, 1, PowerId::Thorns, 3);

    let strike = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::Strike, 9),
        Some(1),
    );
    let twin = card_play_effect_facts(
        &combat,
        &CombatCard::for_scenario(CardId::TwinStrike, 10),
        Some(1),
    );

    assert_eq!(strike.reactive.attack_retaliation_trigger_count_hint, 1);
    assert_eq!(strike.reactive.attack_retaliation_raw_player_damage_hint, 3);
    assert_eq!(strike.reactive.attack_retaliation_player_block_loss_hint, 3);
    assert_eq!(strike.reactive.attack_retaliation_player_hp_loss_hint, 0);
    assert_eq!(strike.reactive.player_hp_loss, 0);
    assert_eq!(strike.reactive_risk_score(), 3);

    assert_eq!(twin.reactive.attack_retaliation_trigger_count_hint, 2);
    assert_eq!(twin.reactive.attack_retaliation_raw_player_damage_hint, 6);
    assert_eq!(twin.reactive.attack_retaliation_player_block_loss_hint, 5);
    assert_eq!(twin.reactive.attack_retaliation_player_hp_loss_hint, 1);
    assert_eq!(twin.reactive.player_hp_loss, 1);
    assert_eq!(twin.reactive_risk_score(), 6);
    assert_eq!(combat.entities.player.block, 5);
}

#[test]
fn after_image_reports_reactive_player_block() {
    let mut combat = blank_test_combat();
    insert_power(&mut combat, 0, PowerId::AfterImage, 1);
    let strike = CombatCard::for_scenario(CardId::Strike, 10);

    let facts = card_play_effect_facts(&combat, &strike, Some(1));

    assert_eq!(facts.reactive.player_block, 1);
}

#[test]
fn hex_reports_bad_draw_cards_for_non_attack() {
    let mut combat = blank_test_combat();
    let mut chosen = test_monster(EnemyId::Chosen);
    chosen.id = 1;
    combat.entities.monsters = vec![chosen];
    insert_power(&mut combat, 1, PowerId::Hex, 1);

    let defend = CombatCard::for_scenario(CardId::Defend, 10);
    let strike = CombatCard::for_scenario(CardId::Strike, 11);

    let defend_facts = card_play_effect_facts(&combat, &defend, None);
    let strike_facts = card_play_effect_facts(&combat, &strike, Some(1));

    assert_eq!(defend_facts.reactive.bad_draw_cards, 1);
    assert_eq!(strike_facts.reactive.bad_draw_cards, 0);
}

#[test]
fn time_warp_reports_forced_turn_end() {
    let mut combat = blank_test_combat();
    let mut eater = test_monster(EnemyId::TimeEater);
    eater.id = 1;
    combat.entities.monsters = vec![eater];
    insert_power(&mut combat, 1, PowerId::TimeWarp, 11);
    let strike = CombatCard::for_scenario(CardId::Strike, 10);

    let facts = card_play_effect_facts(&combat, &strike, Some(1));

    assert!(facts.reactive.forced_turn_end);
}

fn insert_power(combat: &mut CombatState, owner: usize, power_type: PowerId, amount: i32) {
    combat.entities.power_db.insert(
        owner,
        vec![Power {
            power_type,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
}
