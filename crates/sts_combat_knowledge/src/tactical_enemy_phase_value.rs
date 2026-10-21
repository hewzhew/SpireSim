//! Read-only enemy phase debt used by tactical state value.
//!
//! This accounts for visible encounter mechanics whose remaining effective HP
//! differs from current HP. It neither predicts an unknown intent nor claims a
//! combat outcome.

use sts_core::content::monsters::EnemyId;
use sts_core::content::powers::{store, PowerId};
use sts_core::runtime::combat::{CombatState, MonsterEntity};

// Java large slimes and Slime Boss use move byte 3 for the Split move.
const SPLIT_MOVE_ID: u8 = 3;
// Java TimeEater uses move byte 5 for Haste, which removes debuffs and heals
// back to exactly half HP. Damage below that threshold is not durable progress
// unless the player kills before the move executes.
const TIME_EATER_HASTE_MOVE_ID: u8 = 5;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatTacticalEnemyPhaseValueV1 {
    pub phase_adjusted_living_enemy_count: usize,
    pub raw_living_enemy_hp: i32,
    pub raw_living_enemy_block: i32,
    pub raw_living_enemy_effort: i32,
    pub phase_adjusted_living_enemy_hp: i32,
    pub phase_adjusted_living_enemy_effort: i32,
    pub split_pending_count: usize,
    pub split_debt_hp: i32,
    pub awakened_rebirth_pending_count: usize,
    pub awakened_rebirth_debt_hp: i32,
    pub time_eater_haste_pending_count: usize,
    pub time_eater_haste_debt_hp: i32,
    pub guardian_defensive_count: usize,
    pub guardian_defensive_block: i32,
}

pub fn combat_tactical_enemy_phase_value_v1(
    combat: &CombatState,
) -> CombatTacticalEnemyPhaseValueV1 {
    combat.entities.monsters.iter().fold(
        CombatTacticalEnemyPhaseValueV1::default(),
        |mut value, monster| {
            let alive_for_action = monster.is_alive_for_action();
            let awakened_rebirth_debt_hp = awakened_rebirth_debt_hp(monster);
            if !alive_for_action && awakened_rebirth_debt_hp == 0 {
                return value;
            }
            value.phase_adjusted_living_enemy_count =
                value.phase_adjusted_living_enemy_count.saturating_add(1);
            let raw_hp = if alive_for_action {
                monster.current_hp.max(0)
            } else {
                0
            };
            let raw_block = if alive_for_action {
                monster.block.max(0)
            } else {
                0
            };
            let split_debt_hp = if is_split_pending_or_triggered(combat, monster) {
                raw_hp
            } else {
                0
            };
            let time_eater_haste_debt_hp = if is_time_eater_haste_pending(monster) {
                monster
                    .max_hp
                    .saturating_div(2)
                    .saturating_sub(raw_hp)
                    .max(0)
            } else {
                0
            };
            let adjusted_hp = raw_hp
                .saturating_add(split_debt_hp)
                .saturating_add(time_eater_haste_debt_hp)
                .saturating_add(awakened_rebirth_debt_hp);
            let adjusted_effort = adjusted_hp.saturating_add(raw_block);
            value.raw_living_enemy_hp += raw_hp;
            value.raw_living_enemy_block += raw_block;
            value.raw_living_enemy_effort += raw_hp.saturating_add(raw_block);
            value.phase_adjusted_living_enemy_hp += adjusted_hp;
            value.phase_adjusted_living_enemy_effort += adjusted_effort;
            if split_debt_hp > 0 {
                value.split_pending_count += 1;
                value.split_debt_hp += split_debt_hp;
            }
            if awakened_rebirth_debt_hp > 0 {
                value.awakened_rebirth_pending_count += 1;
                value.awakened_rebirth_debt_hp += awakened_rebirth_debt_hp;
            }
            if time_eater_haste_debt_hp > 0 {
                value.time_eater_haste_pending_count += 1;
                value.time_eater_haste_debt_hp += time_eater_haste_debt_hp;
            }
            if is_guardian_defensive(monster) {
                value.guardian_defensive_count += 1;
                value.guardian_defensive_block += raw_block;
            }
            value
        },
    )
}

fn awakened_rebirth_debt_hp(monster: &MonsterEntity) -> i32 {
    if monster.enemy_id == EnemyId::AwakenedOne && monster.awakened_one.form1 {
        monster.max_hp.max(0)
    } else {
        0
    }
}

fn is_split_pending_or_triggered(combat: &CombatState, monster: &MonsterEntity) -> bool {
    has_split_power(combat, monster)
        && (monster.planned_move_id() == SPLIT_MOVE_ID
            || monster.current_hp <= monster.max_hp.saturating_div(2))
}

fn has_split_power(combat: &CombatState, monster: &MonsterEntity) -> bool {
    store::has_power(combat, monster.id, PowerId::Split)
}

fn is_guardian_defensive(monster: &MonsterEntity) -> bool {
    monster.enemy_id == EnemyId::TheGuardian && !monster.guardian.is_open
}

fn is_time_eater_haste_pending(monster: &MonsterEntity) -> bool {
    monster.enemy_id == EnemyId::TimeEater && monster.planned_move_id() == TIME_EATER_HASTE_MOVE_ID
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::runtime::combat::{Power, PowerPayload};
    use sts_core::test_support::{blank_test_combat, test_monster};

    #[test]
    fn split_pending_counts_inherited_child_hp_as_phase_debt() {
        let mut combat = blank_test_combat();
        let mut slime = test_monster(EnemyId::AcidSlimeL);
        slime.id = 7;
        slime.current_hp = 30;
        slime.max_hp = 65;
        slime.set_planned_move_id(SPLIT_MOVE_ID);
        combat.entities.monsters = vec![slime];
        combat.entities.power_db.insert(
            7,
            vec![Power {
                power_type: PowerId::Split,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        let value = combat_tactical_enemy_phase_value_v1(&combat);

        assert_eq!(value.raw_living_enemy_hp, 30);
        assert_eq!(value.raw_living_enemy_block, 0);
        assert_eq!(value.raw_living_enemy_effort, 30);
        assert_eq!(value.phase_adjusted_living_enemy_hp, 60);
        assert_eq!(value.phase_adjusted_living_enemy_effort, 60);
        assert_eq!(value.split_pending_count, 1);
        assert_eq!(value.split_debt_hp, 30);
    }

    #[test]
    fn split_power_above_threshold_keeps_raw_hp_until_split_is_pending() {
        let mut combat = blank_test_combat();
        let mut slime = test_monster(EnemyId::AcidSlimeL);
        slime.id = 8;
        slime.current_hp = 40;
        slime.max_hp = 65;
        combat.entities.monsters = vec![slime];
        combat.entities.power_db.insert(
            8,
            vec![Power {
                power_type: PowerId::Split,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        let value = combat_tactical_enemy_phase_value_v1(&combat);

        assert_eq!(value.raw_living_enemy_hp, 40);
        assert_eq!(value.raw_living_enemy_block, 0);
        assert_eq!(value.raw_living_enemy_effort, 40);
        assert_eq!(value.phase_adjusted_living_enemy_hp, 40);
        assert_eq!(value.phase_adjusted_living_enemy_effort, 40);
        assert_eq!(value.split_pending_count, 0);
        assert_eq!(value.split_debt_hp, 0);
    }

    #[test]
    fn guardian_defensive_block_counts_as_phase_effort() {
        let mut combat = blank_test_combat();
        let mut guardian = test_monster(EnemyId::TheGuardian);
        guardian.id = 9;
        guardian.current_hp = 180;
        guardian.max_hp = 240;
        guardian.block = 20;
        guardian.guardian.is_open = false;
        combat.entities.monsters = vec![guardian];

        let value = combat_tactical_enemy_phase_value_v1(&combat);

        assert_eq!(value.raw_living_enemy_hp, 180);
        assert_eq!(value.raw_living_enemy_block, 20);
        assert_eq!(value.raw_living_enemy_effort, 200);
        assert_eq!(value.phase_adjusted_living_enemy_hp, 180);
        assert_eq!(value.phase_adjusted_living_enemy_effort, 200);
        assert_eq!(value.guardian_defensive_count, 1);
        assert_eq!(value.guardian_defensive_block, 20);
    }

    #[test]
    fn awakened_one_form_one_includes_full_rebirth_phase_debt() {
        let mut combat = blank_test_combat();
        let mut awakened = test_monster(EnemyId::AwakenedOne);
        awakened.id = 10;
        awakened.current_hp = 1;
        awakened.max_hp = 300;
        awakened.awakened_one.form1 = true;
        combat.entities.monsters = vec![awakened];

        let value = combat_tactical_enemy_phase_value_v1(&combat);

        assert_eq!(value.phase_adjusted_living_enemy_count, 1);
        assert_eq!(value.raw_living_enemy_hp, 1);
        assert_eq!(value.phase_adjusted_living_enemy_hp, 301);
        assert_eq!(value.phase_adjusted_living_enemy_effort, 301);
        assert_eq!(value.split_pending_count, 0);
        assert_eq!(value.split_debt_hp, 0);
        assert_eq!(value.awakened_rebirth_pending_count, 1);
        assert_eq!(value.awakened_rebirth_debt_hp, 300);
    }

    #[test]
    fn awakened_one_rebirth_window_preserves_enemy_count_and_phase_debt() {
        let mut combat = blank_test_combat();
        let mut awakened = test_monster(EnemyId::AwakenedOne);
        awakened.id = 12;
        awakened.current_hp = 0;
        awakened.max_hp = 300;
        awakened.awakened_one.form1 = true;
        combat.entities.monsters = vec![awakened];

        let value = combat_tactical_enemy_phase_value_v1(&combat);

        assert_eq!(value.phase_adjusted_living_enemy_count, 1);
        assert_eq!(value.raw_living_enemy_hp, 0);
        assert_eq!(value.raw_living_enemy_effort, 0);
        assert_eq!(value.phase_adjusted_living_enemy_hp, 300);
        assert_eq!(value.phase_adjusted_living_enemy_effort, 300);
        assert_eq!(value.awakened_rebirth_pending_count, 1);
        assert_eq!(value.awakened_rebirth_debt_hp, 300);
    }

    #[test]
    fn awakened_one_form_two_has_no_remaining_rebirth_debt() {
        let mut combat = blank_test_combat();
        let mut awakened = test_monster(EnemyId::AwakenedOne);
        awakened.id = 11;
        awakened.current_hp = 300;
        awakened.max_hp = 300;
        awakened.awakened_one.form1 = false;
        combat.entities.monsters = vec![awakened];

        let value = combat_tactical_enemy_phase_value_v1(&combat);

        assert_eq!(value.phase_adjusted_living_enemy_count, 1);
        assert_eq!(value.phase_adjusted_living_enemy_hp, 300);
        assert_eq!(value.awakened_rebirth_pending_count, 0);
        assert_eq!(value.awakened_rebirth_debt_hp, 0);
    }

    #[test]
    fn time_eater_haste_window_counts_the_forced_heal_as_phase_debt() {
        let mut combat = blank_test_combat();
        let mut eater = test_monster(EnemyId::TimeEater);
        eater.id = 13;
        eater.current_hp = 105;
        eater.max_hp = 456;
        eater.time_eater.used_haste = true;
        eater.set_planned_move_id(TIME_EATER_HASTE_MOVE_ID);
        combat.entities.monsters = vec![eater];

        let value = combat_tactical_enemy_phase_value_v1(&combat);

        assert_eq!(value.raw_living_enemy_hp, 105);
        assert_eq!(value.phase_adjusted_living_enemy_hp, 228);
        assert_eq!(value.time_eater_haste_pending_count, 1);
        assert_eq!(value.time_eater_haste_debt_hp, 123);
    }

    #[test]
    fn time_eater_after_haste_keeps_raw_hp_without_phantom_debt() {
        let mut combat = blank_test_combat();
        let mut eater = test_monster(EnemyId::TimeEater);
        eater.id = 14;
        eater.current_hp = 105;
        eater.max_hp = 456;
        eater.time_eater.used_haste = true;
        eater.set_planned_move_id(2);
        combat.entities.monsters = vec![eater];

        let value = combat_tactical_enemy_phase_value_v1(&combat);

        assert_eq!(value.phase_adjusted_living_enemy_hp, 105);
        assert_eq!(value.time_eater_haste_pending_count, 0);
        assert_eq!(value.time_eater_haste_debt_hp, 0);
    }
}
