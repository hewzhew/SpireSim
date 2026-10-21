use super::*;
use crate::runtime::combat::{CombatState, Power, PowerPayload};
use crate::runtime::monster_move::{BuffSpec, MonsterMoveSpec};
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

fn combat_with_owner_powers(powers: Vec<Power>) -> CombatState {
    let mut combat = blank_test_combat();
    let mut owner = test_monster(EnemyId::Cultist);
    owner.id = 7;
    combat.entities.monsters = vec![owner];
    combat.entities.power_db.insert(7, powers);
    combat
}

#[test]
fn finite_survival_damage_mitigation_profile_reads_powers_not_enemy_name() {
    let combat = combat_with_owner_powers(vec![
        test_power(PowerId::Fading, 5),
        test_power(PowerId::Shifting, -1),
    ]);

    let profile = enemy_mechanics_profile(&combat);
    let report = enemy_mechanics_profile_report(profile);

    assert_eq!(profile.finite_survival_damage_mitigation_target_count, 1);
    assert_eq!(
        profile.finite_survival_damage_mitigation_min_owner_turns,
        Some(5)
    );
    assert_eq!(report.finite_survival_damage_mitigation_target_count, 1);
    assert_eq!(
        report.finite_survival_damage_mitigation_min_owner_turns,
        Some(5)
    );
}

#[test]
fn finite_survival_damage_mitigation_profile_requires_both_powers() {
    for powers in [
        vec![test_power(PowerId::Fading, 5)],
        vec![test_power(PowerId::Shifting, -1)],
    ] {
        let combat = combat_with_owner_powers(powers);
        let profile = enemy_mechanics_profile(&combat);

        assert_eq!(profile.finite_survival_damage_mitigation_target_count, 0);
        assert_eq!(
            profile.finite_survival_damage_mitigation_min_owner_turns,
            None
        );
    }
}

#[test]
fn awakened_one_profile_reports_targetable_form_one_transition_facts() {
    let mut combat = blank_test_combat();
    let mut awakened = test_monster(EnemyId::AwakenedOne);
    awakened.id = 7;
    awakened.current_hp = 23;
    awakened.block = 4;
    awakened.awakened_one.form1 = true;
    combat.entities.monsters = vec![awakened];
    combat.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 6,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let profile = enemy_mechanics_profile(&combat);
    let report = enemy_mechanics_profile_report(profile);

    assert_eq!(profile.awakened_one_form_one_target, Some(7));
    assert_eq!(profile.awakened_one_form_one_hp_with_block, Some(27));
    assert_eq!(profile.awakened_one_positive_strength, Some(6));
    assert_eq!(report.awakened_one_form_one_target, Some(7));
}

#[test]
fn awakened_one_profile_hides_transition_facts_in_form_two() {
    let mut combat = blank_test_combat();
    let mut awakened = test_monster(EnemyId::AwakenedOne);
    awakened.id = 7;
    awakened.current_hp = 23;
    awakened.block = 4;
    awakened.awakened_one.form1 = false;
    combat.entities.monsters = vec![awakened];

    let profile = enemy_mechanics_profile(&combat);

    assert_eq!(profile.awakened_one_form_one_target, None);
    assert_eq!(profile.awakened_one_form_one_hp_with_block, None);
    assert_eq!(profile.awakened_one_positive_strength, None);
}

#[test]
fn guardian_profile_reports_mode_shift_remaining() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 7;
    guardian.guardian.is_open = true;
    combat.entities.monsters = vec![guardian];
    combat.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::ModeShift,
            instance_id: None,
            amount: 4,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let profile = enemy_mechanics_profile(&combat);

    assert_eq!(profile.guardian_open_count, 1);
    assert_eq!(profile.guardian_min_mode_shift_remaining, Some(4));
}

#[test]
fn profile_reports_timed_enemy_threat_aggregates() {
    let mut combat = blank_test_combat();
    let mut exploder = test_monster(EnemyId::Exploder);
    exploder.id = 7;
    combat.entities.monsters = vec![exploder];
    combat.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::Explosive,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let profile = enemy_mechanics_profile(&combat);
    let report = enemy_mechanics_profile_report(profile);

    assert_eq!(profile.timed_threat_count, 1);
    assert_eq!(profile.timed_threat_min_owner_turns, Some(3));
    assert_eq!(profile.timed_threat_total_raw_damage, 30);
    assert_eq!(
        report.profiling_policy,
        "typed_enemy_mechanics_fact_profile_no_direct_score"
    );
}

#[test]
fn profile_reports_attack_retaliation_aggregates() {
    let mut combat = blank_test_combat();
    let mut spiker = test_monster(EnemyId::Spiker);
    spiker.id = 7;
    spiker.set_planned_steps(
        MonsterMoveSpec::Buff(BuffSpec {
            power_id: PowerId::Thorns,
            amount: 2,
        })
        .to_steps(),
    );
    combat.entities.monsters = vec![spiker];
    combat.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::Thorns,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let profile = enemy_mechanics_profile(&combat);
    let report = enemy_mechanics_profile_report(profile);

    assert_eq!(profile.attack_retaliation_target_count, 1);
    assert_eq!(profile.attack_retaliation_total_per_event, 3);
    assert_eq!(profile.attack_retaliation_visible_growth_target_count, 1);
    assert_eq!(profile.attack_retaliation_visible_growth_total, 2);
    assert_eq!(report.attack_retaliation_target_count, 1);
    assert_eq!(report.attack_retaliation_total_per_event, 3);
    assert_eq!(report.attack_retaliation_visible_growth_target_count, 1);
    assert_eq!(report.attack_retaliation_visible_growth_total, 2);
}

#[test]
fn gremlin_nob_profile_reports_anger_amount() {
    let mut combat = blank_test_combat();
    let mut nob = test_monster(EnemyId::GremlinNob);
    nob.id = 9;
    combat.entities.monsters = vec![nob];
    combat.entities.power_db.insert(
        9,
        vec![Power {
            power_type: PowerId::Anger,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let profile = enemy_mechanics_profile(&combat);

    assert_eq!(profile.gremlin_nob_enrage_count, 1);
    assert_eq!(profile.gremlin_nob_anger_amount_total, 2);
}

#[test]
fn bronze_automaton_profile_reports_spawn_and_stasis_pressure() {
    let mut combat = blank_test_combat();
    let mut automaton = test_monster(EnemyId::BronzeAutomaton);
    automaton.id = 1;
    automaton.bronze_automaton.first_turn = true;
    let mut orb = test_monster(EnemyId::BronzeOrb);
    orb.id = 2;
    orb.bronze_orb.used_stasis = false;
    orb.set_planned_move_id(3);
    combat.entities.monsters = vec![automaton, orb];

    let profile = enemy_mechanics_profile(&combat);

    assert_eq!(profile.bronze_automaton_count, 1);
    assert_eq!(profile.bronze_automaton_spawn_orbs_pending_count, 1);
    assert_eq!(profile.bronze_orb_count, 1);
    assert_eq!(profile.bronze_orb_stasis_pending_count, 1);
}

#[test]
fn healer_profile_reports_support_enemy() {
    let mut combat = blank_test_combat();
    let mut healer = test_monster(EnemyId::Healer);
    healer.id = 2;
    combat.entities.monsters = vec![healer];

    let profile = enemy_mechanics_profile(&combat);

    assert_eq!(profile.healer_support_count, 1);
    assert_eq!(profile.tracked_monsters, 1);
}

#[test]
fn fungi_profile_reports_swarm_count() {
    let mut combat = blank_test_combat();
    let mut first = test_monster(EnemyId::FungiBeast);
    first.id = 1;
    let mut second = test_monster(EnemyId::FungiBeast);
    second.id = 2;
    let mut third = test_monster(EnemyId::FungiBeast);
    third.id = 3;
    combat.entities.monsters = vec![first, second, third];

    let profile = enemy_mechanics_profile(&combat);

    assert_eq!(profile.fungi_beast_count, 3);
    assert_eq!(profile.tracked_monsters, 3);
}

#[test]
fn slime_split_profile_includes_the_exact_half_hp_threshold() {
    let mut combat = blank_test_combat();
    let mut slime = test_monster(EnemyId::AcidSlimeL);
    slime.id = 7;
    slime.max_hp = 60;
    slime.current_hp = 30;
    combat.entities.monsters = vec![slime];
    combat
        .entities
        .power_db
        .insert(7, vec![test_power(PowerId::Split, 1)]);

    let at_half = enemy_mechanics_profile(&combat);
    combat.entities.monsters[0].current_hp = 31;
    let above_half = enemy_mechanics_profile(&combat);

    assert_eq!(at_half.split_pending_count, 1);
    assert_eq!(above_half.split_pending_count, 0);
}

#[test]
fn time_eater_haste_profile_starts_below_but_not_at_half_hp() {
    let mut combat = blank_test_combat();
    let mut time_eater = test_monster(EnemyId::TimeEater);
    time_eater.id = 7;
    time_eater.max_hp = 300;
    time_eater.current_hp = 150;
    time_eater.time_eater.used_haste = false;
    time_eater.set_planned_move_id(0);
    combat.entities.monsters = vec![time_eater];

    let at_half = enemy_mechanics_profile(&combat);
    combat.entities.monsters[0].current_hp = 149;
    let below_half = enemy_mechanics_profile(&combat);

    assert_eq!(at_half.time_eater_pending_haste_count, 0);
    assert_eq!(below_half.time_eater_pending_haste_count, 1);
}
