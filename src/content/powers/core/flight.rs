use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::action::{DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;
use crate::semantics::combat::{MonsterTurnPlan, MoveStep};
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Flight,
        round_duration: false,
    };

pub fn on_calculate_damage_from_player(mut damage: f32, amount: i32) -> f32 {
    if amount > 0 {
        // Flight reduces damage by exactly 50% (damage / 2.0f) before block
        damage /= 2.0;
    }
    damage
}

pub fn at_damage_final_receive(damage: i32, amount: i32, damage_type: DamageType) -> i32 {
    if amount > 0 && damage_type != DamageType::HpLoss && damage_type != DamageType::Thorns {
        // Java FlightPower.atDamageFinalReceive returns a float and DamageInfo later floors it.
        // For integer inputs this is equivalent to floor(damage / 2.0).
        (damage as f32 / 2.0).floor() as i32
    } else {
        damage
    }
}

pub fn on_attacked(
    state: &CombatState,
    owner: EntityId,
    damage: i32,
    source: EntityId,
    damage_type: DamageType,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    let owner_survived = state
        .entities
        .monsters
        .iter()
        .find(|m| m.id == owner)
        .is_some_and(|m| m.current_hp > 0 && !m.is_dying);

    if damage > 0
        && amount > 0
        && owner_survived
        && source != NO_SOURCE
        && damage_type != DamageType::HpLoss
        && damage_type != DamageType::Thorns
    {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: crate::content::powers::PowerId::Flight,
            amount: 1,
        });
    }

    actions
}

pub fn at_turn_start(
    state: &CombatState,
    owner: EntityId,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let stored_amount = state
        .entities
        .power_db
        .get(&owner)
        .and_then(|powers| {
            powers
                .iter()
                .find(|p| p.power_type == crate::content::powers::PowerId::Flight)
                .map(|p| p.extra_data)
        })
        .unwrap_or(amount);

    if stored_amount > amount {
        smallvec::smallvec![Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: crate::content::powers::PowerId::Flight,
            amount: stored_amount - amount,
        }]
    } else {
        smallvec::smallvec![]
    }
}

#[cfg(test)]
mod tests {
    use super::on_attacked;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::{store, PowerId};
    use crate::engine::action_handlers::execute_action;
    use crate::runtime::action::{Action, DamageType};
    use crate::runtime::combat::{Power, PowerPayload};

    fn flight_power(amount: i32) -> Power {
        Power {
            power_type: PowerId::Flight,
            instance_id: None,
            amount,
            extra_data: amount,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    #[test]
    fn flight_on_attacked_uses_reduce_power_so_zero_triggers_byrd_grounded_state() {
        let mut byrd = crate::test_support::test_monster(EnemyId::Byrd);
        byrd.id = 1;
        byrd.current_hp = 10;
        byrd.byrd.is_flying = true;
        let mut state = crate::test_support::combat_with_monsters(vec![byrd]);
        store::set_powers_for(&mut state, 1, vec![flight_power(1)]);

        let actions = on_attacked(&state, 1, 1, 0, DamageType::Normal, 1);

        assert_eq!(
            actions.as_slice(),
            &[Action::ReducePower {
                target: 1,
                power_id: PowerId::Flight,
                amount: 1,
            }]
        );

        execute_action(actions[0].clone(), &mut state);
        while let Some(action) = state.engine.action_queue.pop_front() {
            execute_action(action, &mut state);
        }

        let byrd = &state.entities.monsters[0];
        assert!(!byrd.byrd.is_flying);
        assert_eq!(byrd.planned_move_id(), 4);
        assert!(!store::has_power(&state, 1, PowerId::Flight));
    }

    #[test]
    fn flight_on_attacked_ignores_non_surviving_damage_like_java_will_live_guard() {
        let mut byrd = crate::test_support::test_monster(EnemyId::Byrd);
        byrd.id = 1;
        byrd.current_hp = 0;
        let state = crate::test_support::combat_with_monsters(vec![byrd]);

        assert!(
            on_attacked(&state, 1, 1, 0, DamageType::Normal, 1).is_empty(),
            "Rust calls this hook after damage; zero remaining HP mirrors Java willLive=false"
        );
    }

    #[test]
    fn flight_on_attacked_is_not_called_for_thorns_or_hp_loss_damage() {
        let byrd = crate::test_support::test_monster(EnemyId::Byrd);
        let state = crate::test_support::combat_with_monsters(vec![byrd]);

        assert_eq!(
            crate::content::powers::resolve_power_on_attacked(
                PowerId::Flight,
                &state,
                1,
                1,
                0,
                DamageType::HpLoss,
                1,
            )
            .len(),
            0
        );
        assert_eq!(
            crate::content::powers::resolve_power_on_attacked(
                PowerId::Flight,
                &state,
                1,
                1,
                0,
                DamageType::Thorns,
                1,
            )
            .len(),
            0
        );
    }
}

pub fn on_remove(state: &CombatState, owner: EntityId) -> smallvec::SmallVec<[Action; 2]> {
    if state
        .entities
        .monsters
        .iter()
        .any(|m| m.id == owner && m.enemy_id == crate::content::monsters::EnemyId::Byrd)
    {
        let plan = MonsterTurnPlan::single(4, MoveStep::Stun);
        smallvec::smallvec![
            Action::UpdateMonsterRuntime {
                monster_id: owner,
                patch: MonsterRuntimePatch::Byrd {
                    first_move: None,
                    is_flying: Some(false),
                    protocol_seeded: Some(true),
                },
            },
            Action::SetMonsterMove {
                monster_id: owner,
                next_move_byte: plan.move_id,
                planned_steps: plan.steps,
                planned_visible_spec: plan.visible_spec,
            }
        ]
    } else {
        smallvec::smallvec![]
    }
}
