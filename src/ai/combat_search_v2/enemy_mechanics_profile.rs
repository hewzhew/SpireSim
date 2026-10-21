use super::attack_retaliation::attack_retaliation_for_target;
use super::*;
use crate::content::powers::{store, PowerId};
use sts_combat_knowledge::combat_tactical_timed_enemy_threats_v1;

const SPLIT_MOVE_ID: u8 = 3;
const SENTRY_BOLT_MOVE_ID: u8 = 3;
const HEXAGHOST_DIVIDER_MOVE_ID: u8 = 1;
const HEXAGHOST_ACTIVATE_MOVE_ID: u8 = 5;
const BRONZE_AUTOMATON_HYPER_BEAM_MOVE_ID: u8 = 2;
const BRONZE_AUTOMATON_SPAWN_ORBS_MOVE_ID: u8 = 4;
const BRONZE_ORB_STASIS_MOVE_ID: u8 = 3;
// Java TimeEater uses move byte 5 for Haste. `used_haste` is set when this
// move is selected, before the player receives the setup/heal window, so the
// planned move is the authoritative fact while that window is active.
const TIME_EATER_HASTE_MOVE_ID: u8 = 5;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct EnemyMechanicsProfileV1 {
    pub(super) tracked_monsters: usize,
    pub(super) timed_threat_count: usize,
    pub(super) timed_threat_min_owner_turns: Option<u32>,
    pub(super) timed_threat_total_raw_damage: i32,
    pub(super) finite_survival_damage_mitigation_target_count: usize,
    pub(super) finite_survival_damage_mitigation_min_owner_turns: Option<u32>,
    pub(super) attack_retaliation_target_count: usize,
    pub(super) attack_retaliation_total_per_event: i32,
    pub(super) attack_retaliation_visible_growth_target_count: usize,
    pub(super) attack_retaliation_visible_growth_total: i32,
    pub(super) split_pending_count: usize,
    pub(super) guardian_open_count: usize,
    pub(super) guardian_defensive_count: usize,
    pub(super) guardian_mode_shift_pending_count: usize,
    pub(super) guardian_min_mode_shift_remaining: Option<i32>,
    pub(super) lagavulin_sleeping_count: usize,
    pub(super) lagavulin_waking_count: usize,
    pub(super) gremlin_nob_enrage_count: usize,
    pub(super) gremlin_nob_anger_amount_total: i32,
    pub(super) sentry_dazed_pressure_count: usize,
    pub(super) fungi_beast_count: usize,
    pub(super) healer_support_count: usize,
    pub(super) hexaghost_opening_pressure_count: usize,
    pub(super) bronze_automaton_count: usize,
    pub(super) bronze_automaton_spawn_orbs_pending_count: usize,
    pub(super) bronze_automaton_hyper_beam_pending_count: usize,
    pub(super) bronze_orb_count: usize,
    pub(super) bronze_orb_stasis_pending_count: usize,
    pub(super) bronze_orb_stasis_card_count: usize,
    pub(super) awakened_one_curiosity_count: usize,
    pub(super) awakened_one_form_one_target: Option<usize>,
    pub(super) awakened_one_form_one_hp_with_block: Option<i32>,
    pub(super) awakened_one_positive_strength: Option<i32>,
    pub(super) time_eater_count: usize,
    pub(super) time_eater_time_warp_counter: Option<i32>,
    pub(super) time_eater_cards_until_warp: Option<i32>,
    pub(super) time_eater_haste_used: Option<bool>,
    pub(super) time_eater_pending_haste_count: usize,
    pub(super) time_eater_current_hp: Option<i32>,
    pub(super) time_eater_half_hp: Option<i32>,
}

pub(super) fn enemy_mechanics_profile(combat: &CombatState) -> EnemyMechanicsProfileV1 {
    let timed_threats = combat_tactical_timed_enemy_threats_v1(combat);
    let mut profile = EnemyMechanicsProfileV1 {
        timed_threat_count: timed_threats.len(),
        timed_threat_min_owner_turns: timed_threats
            .iter()
            .map(|threat| threat.owner_turns_until_trigger)
            .min(),
        timed_threat_total_raw_damage: timed_threats
            .iter()
            .map(|threat| threat.raw_player_damage)
            .sum(),
        ..EnemyMechanicsProfileV1::default()
    };
    for monster in combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
    {
        let fading_turns = store::power_amount(combat, monster.id, PowerId::Fading);
        if fading_turns > 0 && store::has_power(combat, monster.id, PowerId::Shifting) {
            profile.finite_survival_damage_mitigation_target_count += 1;
            let fading_turns = fading_turns as u32;
            profile.finite_survival_damage_mitigation_min_owner_turns = Some(
                profile
                    .finite_survival_damage_mitigation_min_owner_turns
                    .map_or(fading_turns, |old| old.min(fading_turns)),
            );
        }
        if let Some(retaliation) = attack_retaliation_for_target(combat, monster.id) {
            profile.attack_retaliation_target_count += 1;
            profile.attack_retaliation_total_per_event = profile
                .attack_retaliation_total_per_event
                .saturating_add(retaliation.raw_player_damage_per_damage_event);
            if retaliation.visible_growth_amount > 0 {
                profile.attack_retaliation_visible_growth_target_count += 1;
                profile.attack_retaliation_visible_growth_total = profile
                    .attack_retaliation_visible_growth_total
                    .saturating_add(retaliation.visible_growth_amount);
            }
        }
        let enemy_id = monster.enemy_id;

        match enemy_id {
            EnemyId::AcidSlimeL | EnemyId::SpikeSlimeL | EnemyId::SlimeBoss => {
                profile.tracked_monsters += 1;
                if split_pending_or_triggered(combat, monster) {
                    profile.split_pending_count += 1;
                }
            }
            EnemyId::TheGuardian => {
                profile.tracked_monsters += 1;
                if monster.guardian.is_open {
                    profile.guardian_open_count += 1;
                    if store::has_power(combat, monster.id, PowerId::ModeShift) {
                        let remaining = store::power_amount(combat, monster.id, PowerId::ModeShift);
                        profile.guardian_min_mode_shift_remaining = Some(
                            profile
                                .guardian_min_mode_shift_remaining
                                .map_or(remaining, |old| old.min(remaining)),
                        );
                        if remaining <= 0 || monster.guardian.close_up_triggered {
                            profile.guardian_mode_shift_pending_count += 1;
                        }
                    }
                } else {
                    profile.guardian_defensive_count += 1;
                }
            }
            EnemyId::Lagavulin => {
                profile.tracked_monsters += 1;
                if !monster.lagavulin.is_out {
                    profile.lagavulin_sleeping_count += 1;
                }
                if monster.lagavulin.is_out_triggered {
                    profile.lagavulin_waking_count += 1;
                }
            }
            EnemyId::GremlinNob => {
                profile.tracked_monsters += 1;
                if store::has_power(combat, monster.id, PowerId::Anger) {
                    let amount = store::power_amount(combat, monster.id, PowerId::Anger);
                    profile.gremlin_nob_enrage_count += 1;
                    profile.gremlin_nob_anger_amount_total += amount;
                }
            }
            EnemyId::Sentry => {
                profile.tracked_monsters += 1;
                if monster.planned_move_id() == SENTRY_BOLT_MOVE_ID {
                    profile.sentry_dazed_pressure_count += 1;
                }
            }
            EnemyId::FungiBeast => {
                profile.tracked_monsters += 1;
                profile.fungi_beast_count += 1;
            }
            EnemyId::Healer => {
                profile.tracked_monsters += 1;
                profile.healer_support_count += 1;
            }
            EnemyId::Hexaghost => {
                profile.tracked_monsters += 1;
                if matches!(
                    monster.planned_move_id(),
                    HEXAGHOST_ACTIVATE_MOVE_ID | HEXAGHOST_DIVIDER_MOVE_ID
                ) {
                    profile.hexaghost_opening_pressure_count += 1;
                }
            }
            EnemyId::BronzeAutomaton => {
                profile.tracked_monsters += 1;
                profile.bronze_automaton_count += 1;
                if monster.bronze_automaton.first_turn
                    || monster.planned_move_id() == BRONZE_AUTOMATON_SPAWN_ORBS_MOVE_ID
                {
                    profile.bronze_automaton_spawn_orbs_pending_count += 1;
                }
                if monster.bronze_automaton.num_turns >= 4
                    || monster.planned_move_id() == BRONZE_AUTOMATON_HYPER_BEAM_MOVE_ID
                {
                    profile.bronze_automaton_hyper_beam_pending_count += 1;
                }
            }
            EnemyId::BronzeOrb => {
                profile.tracked_monsters += 1;
                profile.bronze_orb_count += 1;
                if !monster.bronze_orb.used_stasis
                    && monster.planned_move_id() == BRONZE_ORB_STASIS_MOVE_ID
                {
                    profile.bronze_orb_stasis_pending_count += 1;
                }
                if store::has_power(combat, monster.id, PowerId::Stasis) {
                    profile.bronze_orb_stasis_card_count += 1;
                }
            }
            EnemyId::AwakenedOne => {
                profile.tracked_monsters += 1;
                if store::has_power(combat, monster.id, PowerId::Curiosity) {
                    profile.awakened_one_curiosity_count += 1;
                }
                if monster.awakened_one.form1 {
                    profile.awakened_one_form_one_target = Some(monster.id);
                    profile.awakened_one_form_one_hp_with_block =
                        Some(monster.current_hp.saturating_add(monster.block));
                    profile.awakened_one_positive_strength =
                        Some(store::power_amount(combat, monster.id, PowerId::Strength).max(0));
                }
            }
            EnemyId::TimeEater => {
                profile.tracked_monsters += 1;
                profile.time_eater_count += 1;
                let counter =
                    store::power_amount(combat, monster.id, PowerId::TimeWarp).clamp(0, 11);
                let haste_used = monster.time_eater.used_haste;
                let half_hp = monster.max_hp.saturating_div(2);
                profile.time_eater_time_warp_counter = Some(counter);
                profile.time_eater_cards_until_warp = Some(12 - counter);
                profile.time_eater_haste_used = Some(haste_used);
                profile.time_eater_current_hp = Some(monster.current_hp);
                profile.time_eater_half_hp = Some(half_hp);
                if monster.planned_move_id() == TIME_EATER_HASTE_MOVE_ID
                    || (monster.current_hp < half_hp && !haste_used)
                {
                    profile.time_eater_pending_haste_count += 1;
                }
            }
            _ => {}
        }
    }
    profile
}

pub(super) fn enemy_mechanics_profile_report(
    profile: EnemyMechanicsProfileV1,
) -> CombatSearchV2EnemyMechanicsReport {
    CombatSearchV2EnemyMechanicsReport {
        profiling_policy: "typed_enemy_mechanics_fact_profile_no_direct_score",
        tracked_monsters: profile.tracked_monsters,
        timed_threat_count: profile.timed_threat_count,
        timed_threat_min_owner_turns: profile.timed_threat_min_owner_turns,
        timed_threat_total_raw_damage: profile.timed_threat_total_raw_damage,
        finite_survival_damage_mitigation_target_count: profile
            .finite_survival_damage_mitigation_target_count,
        finite_survival_damage_mitigation_min_owner_turns: profile
            .finite_survival_damage_mitigation_min_owner_turns,
        attack_retaliation_target_count: profile.attack_retaliation_target_count,
        attack_retaliation_total_per_event: profile.attack_retaliation_total_per_event,
        attack_retaliation_visible_growth_target_count: profile
            .attack_retaliation_visible_growth_target_count,
        attack_retaliation_visible_growth_total: profile.attack_retaliation_visible_growth_total,
        split_pending_count: profile.split_pending_count,
        guardian_open_count: profile.guardian_open_count,
        guardian_defensive_count: profile.guardian_defensive_count,
        guardian_mode_shift_pending_count: profile.guardian_mode_shift_pending_count,
        guardian_min_mode_shift_remaining: profile.guardian_min_mode_shift_remaining,
        lagavulin_sleeping_count: profile.lagavulin_sleeping_count,
        lagavulin_waking_count: profile.lagavulin_waking_count,
        gremlin_nob_enrage_count: profile.gremlin_nob_enrage_count,
        gremlin_nob_anger_amount_total: profile.gremlin_nob_anger_amount_total,
        sentry_dazed_pressure_count: profile.sentry_dazed_pressure_count,
        fungi_beast_count: profile.fungi_beast_count,
        healer_support_count: profile.healer_support_count,
        hexaghost_opening_pressure_count: profile.hexaghost_opening_pressure_count,
        bronze_automaton_count: profile.bronze_automaton_count,
        bronze_automaton_spawn_orbs_pending_count: profile
            .bronze_automaton_spawn_orbs_pending_count,
        bronze_automaton_hyper_beam_pending_count: profile
            .bronze_automaton_hyper_beam_pending_count,
        bronze_orb_count: profile.bronze_orb_count,
        bronze_orb_stasis_pending_count: profile.bronze_orb_stasis_pending_count,
        bronze_orb_stasis_card_count: profile.bronze_orb_stasis_card_count,
        awakened_one_curiosity_count: profile.awakened_one_curiosity_count,
        awakened_one_form_one_target: profile.awakened_one_form_one_target,
        awakened_one_form_one_hp_with_block: profile.awakened_one_form_one_hp_with_block,
        awakened_one_positive_strength: profile.awakened_one_positive_strength,
        time_eater_count: profile.time_eater_count,
        time_eater_time_warp_counter: profile.time_eater_time_warp_counter,
        time_eater_cards_until_warp: profile.time_eater_cards_until_warp,
        time_eater_haste_used: profile.time_eater_haste_used,
        time_eater_pending_haste_count: profile.time_eater_pending_haste_count,
        time_eater_current_hp: profile.time_eater_current_hp,
        time_eater_half_hp: profile.time_eater_half_hp,
        notes: vec![
            "enemy mechanics profile exposes typed phase/support facts for value/rollout consumers",
            "this profile does not by itself score or prune search branches",
            "split phase debt used by frontier value remains in enemy_phase_value",
        ],
    }
}

fn split_pending_or_triggered(combat: &CombatState, monster: &MonsterEntity) -> bool {
    store::has_power(combat, monster.id, PowerId::Split)
        && (monster.planned_move_id() == SPLIT_MOVE_ID
            || monster.current_hp <= monster.max_hp.saturating_div(2))
}

#[cfg(test)]
mod tests;
