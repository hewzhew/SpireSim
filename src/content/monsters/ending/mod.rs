pub mod corrupt_heart;
pub mod spire_shield;
pub mod spire_spear;

use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub(super) fn surrounded_cleanup_actions(state: &CombatState) -> Vec<Action> {
    let mut actions = Vec::new();
    let java_alive_monsters: Vec<_> = state
        .entities
        .monsters
        .iter()
        .filter(|monster| !monster.is_dying)
        .collect();

    if java_alive_monsters.is_empty() {
        return actions;
    }

    if store::has_power(state, 0, PowerId::Surrounded) {
        actions.push(Action::RemovePower {
            target: 0,
            power_id: PowerId::Surrounded,
        });
    }

    for monster in java_alive_monsters {
        if store::has_power(state, monster.id, PowerId::BackAttack) {
            actions.push(Action::RemovePower {
                target: monster.id,
                power_id: PowerId::BackAttack,
            });
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::surrounded_cleanup_actions;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::{store, PowerId};
    use crate::runtime::action::Action;
    use crate::runtime::combat::{Power, PowerPayload};

    fn power(power_type: PowerId) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    #[test]
    fn surrounded_cleanup_does_not_filter_back_attack_by_hp_like_java_death_loop() {
        let mut monster = crate::test_support::test_monster(EnemyId::SpireSpear);
        monster.id = 7;
        monster.current_hp = 0;
        monster.is_dying = false;
        let mut state = crate::test_support::combat_with_monsters(vec![monster]);
        store::set_powers_for(&mut state, 0, vec![power(PowerId::Surrounded)]);
        store::set_powers_for(&mut state, 7, vec![power(PowerId::BackAttack)]);

        let actions = surrounded_cleanup_actions(&state);

        assert!(actions.contains(&Action::RemovePower {
            target: 0,
            power_id: PowerId::Surrounded,
        }));
        assert!(actions.contains(&Action::RemovePower {
            target: 7,
            power_id: PowerId::BackAttack,
        }));
    }

    #[test]
    fn surrounded_cleanup_waits_for_a_non_dying_monster_like_java_loop() {
        let mut monster = crate::test_support::test_monster(EnemyId::SpireShield);
        monster.id = 8;
        monster.is_dying = true;
        let mut state = crate::test_support::combat_with_monsters(vec![monster]);
        store::set_powers_for(&mut state, 0, vec![power(PowerId::Surrounded)]);
        store::set_powers_for(&mut state, 8, vec![power(PowerId::BackAttack)]);

        assert!(surrounded_cleanup_actions(&state).is_empty());
    }
}
