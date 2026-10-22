use crate::content::monsters::exordium::{attack_actions, set_next_move_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{AttackSpec, DamageKind, MonsterMoveSpec, MonsterTurnPlan};

pub struct TorchHead;

const TACKLE: u8 = 1;
const TACKLE_DAMAGE: i32 = 7;

pub(crate) fn roll_constructor_hp(rng: &mut crate::runtime::rng::StsRng, ascension: u8) -> i32 {
    // TorchHead's super argument rolls first; its constructor then calls setHp.
    let _ = rng.random_range(38, 40);
    if ascension >= 9 {
        rng.random_range(40, 45)
    } else {
        rng.random_range(38, 40)
    }
}

fn tackle_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        TACKLE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: TACKLE_DAMAGE,
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

pub fn mechanics() -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        special_moves: vec![],
        moves: vec![(Text::new("torch_head.tackle"), tackle_plan())],
        notes: Text::new("torch_head.notes"),
    }
}

pub(crate) fn initialize_runtime_state(entity: &mut MonsterEntity) {
    // The constructor calls setMove before SpawnMonsterAction later calls init.
    let plan = tackle_plan();
    entity.set_planned_move_id(plan.move_id);
    entity.set_planned_steps(plan.steps);
    entity.set_planned_visible_spec(plan.visible_spec);
    entity.move_history_mut().push_back(TACKLE);
}

fn plan_for(move_id: u8) -> MonsterTurnPlan {
    match move_id {
        TACKLE => tackle_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

impl MonsterBehavior for TorchHead {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        _ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        tackle_plan()
    }

    fn turn_plan(_state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id())
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match plan.move_id {
            TACKLE => {
                let mut actions = attack_actions(
                    entity.id,
                    PLAYER,
                    &AttackSpec {
                        base_damage: TACKLE_DAMAGE,
                        hits: 1,
                        damage_kind: DamageKind::Normal,
                    },
                );
                actions.push(set_next_move_action(entity, tackle_plan()));
                actions
            }
            _ => panic!(
                "torch head take_turn received unsupported move {}",
                plan.move_id
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{tackle_plan, TorchHead};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::Action;

    #[test]
    fn tackle_queues_damage_before_setmove_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::TorchHead);

        let actions = TorchHead::take_turn_plan(&mut state, &entity, &tackle_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack { base_damage: 7, .. },
                Action::SetMonsterMove {
                    next_move_byte: super::TACKLE,
                    ..
                }
            ]
        ));
    }
}
