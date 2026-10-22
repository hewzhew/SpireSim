use super::{attack_actions, set_next_move_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, AttackStep, DamageKind, MonsterTurnPlan, MoveStep, MoveTarget,
};

const PUNCTURE: u8 = 1;
const ESCAPE: u8 = 99;

pub struct GremlinThief;

enum GremlinThiefTurn<'a> {
    Puncture(&'a AttackSpec),
    Escape,
}

fn puncture_damage(asc: u8) -> i32 {
    if asc >= 2 {
        10
    } else {
        9
    }
}

fn puncture_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        PUNCTURE,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: puncture_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn escape_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(ESCAPE, MoveStep::Escape)
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        PUNCTURE => puncture_plan(asc),
        ESCAPE => escape_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("gremlin_thief.puncture"), puncture_plan(asc)),
            (Text::new("gremlin_thief.escape"), escape_plan()),
        ],
        special_moves: vec![],
        notes: Text::new("gremlin_thief.notes"),
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> GremlinThiefTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            PUNCTURE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => GremlinThiefTurn::Puncture(attack),
        (ESCAPE, [MoveStep::Escape]) => GremlinThiefTurn::Escape,
        (_, []) => panic!("gremlin thief plan missing locked truth"),
        (move_id, steps) => panic!("gremlin thief plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for GremlinThief {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        asc: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        puncture_plan(asc)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match decode_turn(plan) {
            GremlinThiefTurn::Puncture(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(
                    entity,
                    puncture_plan(state.meta.ascension_level),
                ));
                actions
            }
            GremlinThiefTurn::Escape => vec![
                Action::Escape { target: entity.id },
                set_next_move_action(entity, escape_plan()),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{escape_plan, puncture_plan, GremlinThief};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::Action;

    #[test]
    fn puncture_damage_and_followup_setmove_match_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 2;
        let entity = crate::test_support::test_monster(EnemyId::GremlinThief);

        let actions = GremlinThief::take_turn_plan(&mut state, &entity, &puncture_plan(2));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 10,
                    ..
                },
                Action::SetMonsterMove {
                    monster_id: 1,
                    next_move_byte: super::PUNCTURE,
                    ..
                },
            ]
        ));
    }

    #[test]
    fn escape_turn_queues_escape_intent_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::GremlinThief);

        let actions = GremlinThief::take_turn_plan(&mut state, &entity, &escape_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::Escape { .. },
                Action::SetMonsterMove {
                    next_move_byte: super::ESCAPE,
                    ..
                }
            ]
        ));
    }
}
