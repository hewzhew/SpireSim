//! Visible incoming-damage and immediate survival facts for one exact state.
//!
//! These facts use the simulator's monster-move projection. They do not set a
//! search threshold, rank a state, admit a potion, or claim survival.

use sts_core::content::powers::PowerId;
use sts_core::runtime::combat::CombatState;
use sts_core::sim::combat_projection::project_monster_move_preview_in_combat;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatTacticalPressureV1 {
    pub visible_incoming_damage: i32,
    pub survival_margin: i32,
}

pub fn combat_tactical_pressure_v1(combat: &CombatState) -> CombatTacticalPressureV1 {
    let incoming = combat_tactical_visible_incoming_damage_v1(combat);
    CombatTacticalPressureV1 {
        visible_incoming_damage: incoming,
        survival_margin: combat.entities.player.current_hp + combat.entities.player.block
            - incoming,
    }
}

pub fn combat_tactical_visible_incoming_damage_v1(combat: &CombatState) -> i32 {
    let player_intangible =
        combat.get_power(combat.entities.player.id, PowerId::IntangiblePlayer) > 0;
    combat_tactical_visible_incoming_damage_with_player_intangible_v1(combat, player_intangible)
}

pub fn combat_tactical_visible_incoming_damage_with_player_intangible_v1(
    combat: &CombatState,
    player_intangible: bool,
) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| {
            let preview = project_monster_move_preview_in_combat(combat, monster);
            if player_intangible && preview.total_damage.is_some() {
                i32::from(preview.hits)
            } else {
                preview.total_damage.unwrap_or(0)
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::monsters::EnemyId;
    use sts_core::content::powers::{store, PowerId};
    use sts_core::runtime::combat::{Power, PowerPayload};
    use sts_core::test_support::{blank_test_combat, planned_monster};

    #[test]
    fn pressure_value_uses_hp_plus_block_when_no_visible_incoming_damage() {
        let mut combat = blank_test_combat();
        combat.entities.player.current_hp = 13;
        combat.entities.player.block = 7;
        combat.entities.monsters.clear();

        let value = combat_tactical_pressure_v1(&combat);

        assert_eq!(value.visible_incoming_damage, 0);
        assert_eq!(value.survival_margin, 20);
    }

    #[test]
    fn pressure_value_caps_each_visible_attack_hit_with_player_intangible() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::TimeEater, 2)];
        let raw = combat_tactical_pressure_v1(&combat);
        assert_eq!(raw.visible_incoming_damage, 21);

        let player_id = combat.entities.player.id;
        store::set_powers_for(
            &mut combat,
            player_id,
            vec![Power {
                power_type: PowerId::IntangiblePlayer,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        let intangible = combat_tactical_pressure_v1(&combat);
        assert_eq!(intangible.visible_incoming_damage, 3);
        assert_eq!(intangible.survival_margin, 80 - 3);
    }
}
