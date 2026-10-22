use crate::content::monsters::exordium::{attack_actions, set_next_move_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, DamageKind, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget,
};

pub struct BanditPointy;

const POINTY_SPECIAL: u8 = 1;

fn attack_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        6
    } else {
        5
    }
}

fn pointy_special_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        POINTY_SPECIAL,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: attack_damage(ascension_level),
            hits: 2,
            damage_kind: DamageKind::Normal,
        }),
    )
}

pub fn mechanics(ascension_level: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![(
            Text::new("bandit_pointy.pointySpecial"),
            pointy_special_plan(ascension_level),
        )],
        special_moves: vec![],
        notes: Text::new("bandit_pointy.notes"),
    }
}

impl MonsterBehavior for BanditPointy {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        pointy_special_plan(ascension_level)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        match entity.planned_move_id() {
            POINTY_SPECIAL => pointy_special_plan(state.meta.ascension_level),
            other => MonsterTurnPlan::unknown(other),
        }
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match (plan.move_id, plan.steps.as_slice()) {
            (
                POINTY_SPECIAL,
                [MoveStep::Attack(crate::semantics::combat::AttackStep {
                    target: MoveTarget::Player,
                    attack,
                })],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(
                    entity,
                    pointy_special_plan(state.meta.ascension_level),
                ));
                actions
            }
            (_, []) => panic!("bandit pointy plan missing locked truth"),
            (move_id, steps) => {
                panic!("bandit pointy plan/steps mismatch: {} {:?}", move_id, steps)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{pointy_special_plan, BanditPointy};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::Action;

    #[test]
    fn pointy_special_queues_two_damage_actions_before_setmove_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::BanditPointy);

        let actions = BanditPointy::take_turn_plan(&mut state, &entity, &pointy_special_plan(0));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack { base_damage: 5, .. },
                Action::MonsterAttack { base_damage: 5, .. },
                Action::SetMonsterMove {
                    next_move_byte: super::POINTY_SPECIAL,
                    ..
                }
            ]
        ));
    }
}
