pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::PlatedArmor,
        round_duration: false,
    };

use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::semantics::combat::{MonsterTurnPlan, MoveStep};
use crate::EntityId;

pub fn on_monster_turn_ended(
    _state: &CombatState,
    owner: EntityId,
    power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::GainBlock {
        target: owner,
        amount: power_amount,
    }]
}

pub fn on_hp_lost(
    _state: &CombatState,
    owner: EntityId,
    amount: i32,
    source: Option<EntityId>,
    damage_type: crate::runtime::action::DamageType,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    if amount > 0
        && !matches!(
            damage_type,
            crate::runtime::action::DamageType::HpLoss | crate::runtime::action::DamageType::Thorns
        )
        && source.is_some()
        && source != Some(owner)
    {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: PowerId::PlatedArmor,
            amount: 1,
        });
    }
    actions
}

pub fn on_remove(_state: &CombatState, owner: EntityId) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    // Trigger `ARMOR_BREAK` for Shelled Parasite
    if let Some(monster) = _state.entities.monsters.iter().find(|m| m.id == owner) {
        if monster.enemy_id == crate::content::monsters::EnemyId::ShelledParasite {
            let plan = MonsterTurnPlan::single(4, MoveStep::Stun);
            actions.push(Action::SetMonsterMove {
                monster_id: owner,
                next_move_byte: plan.move_id,
                planned_steps: plan.steps,
                planned_visible_spec: plan.visible_spec,
            });
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::on_hp_lost;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::{store, PowerId};
    use crate::engine::action_handlers::execute_action;
    use crate::runtime::action::{Action, DamageType};
    use crate::runtime::combat::{Power, PowerPayload};

    fn plated_armor(amount: i32) -> Power {
        Power {
            power_type: PowerId::PlatedArmor,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    #[test]
    fn plated_armor_hp_loss_uses_reduce_power_so_shelled_parasite_break_triggers() {
        let mut parasite = crate::test_support::test_monster(EnemyId::ShelledParasite);
        parasite.id = 1;
        let mut state = crate::test_support::combat_with_monsters(vec![parasite]);
        store::set_powers_for(&mut state, 1, vec![plated_armor(1)]);

        let actions = on_hp_lost(&state, 1, 1, Some(0), DamageType::Normal);

        assert_eq!(
            actions.as_slice(),
            &[Action::ReducePower {
                target: 1,
                power_id: PowerId::PlatedArmor,
                amount: 1,
            }]
        );

        execute_action(actions[0].clone(), &mut state);
        while let Some(action) = state.engine.action_queue.pop_front() {
            execute_action(action, &mut state);
        }

        assert_eq!(state.entities.monsters[0].planned_move_id(), 4);
        assert!(!store::has_power(&state, 1, PowerId::PlatedArmor));
    }

    #[test]
    fn plated_armor_hp_loss_ignores_thorns_hp_loss_and_self_source_like_java() {
        let parasite = crate::test_support::test_monster(EnemyId::ShelledParasite);
        let state = crate::test_support::combat_with_monsters(vec![parasite]);

        assert!(on_hp_lost(&state, 1, 1, Some(0), DamageType::Thorns).is_empty());
        assert!(on_hp_lost(&state, 1, 1, Some(0), DamageType::HpLoss).is_empty());
        assert!(on_hp_lost(&state, 1, 1, Some(1), DamageType::Normal).is_empty());
        assert!(on_hp_lost(&state, 1, 1, None, DamageType::Normal).is_empty());
    }
}
