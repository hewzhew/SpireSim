use super::{attack_actions, gain_block_random_monster_action, set_next_move_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, AttackStep, DamageKind, DefendSpec, MonsterMoveSpec, MonsterTurnPlan, MoveStep,
    MoveTarget, RandomBlockStep,
};

const PROTECT: u8 = 1;
const BASH: u8 = 2;
const ESCAPE: u8 = 99;

pub struct GremlinTsundere;

enum GremlinTsundereTurn<'a> {
    Protect(&'a RandomBlockStep),
    Bash(&'a AttackSpec),
    Escape,
}

fn bash_damage(asc: u8) -> i32 {
    if asc >= 2 {
        8
    } else {
        6
    }
}

fn block_amount(asc: u8) -> i32 {
    if asc >= 17 {
        11
    } else if asc >= 7 {
        8
    } else {
        7
    }
}

fn protect_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        PROTECT,
        smallvec::smallvec![MoveStep::GainBlockRandomMonster(RandomBlockStep {
            amount: block_amount(asc),
        })],
        MonsterMoveSpec::Defend(DefendSpec {
            block: block_amount(asc),
        }),
    )
}

fn bash_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        BASH,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: bash_damage(asc),
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
        PROTECT => protect_plan(asc),
        BASH => bash_plan(asc),
        ESCAPE => escape_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![("Protect", protect_plan(asc)), ("Shield Bash", bash_plan(asc)), ("Escape", escape_plan())],
        special_moves: vec![],
        notes: "Starts with Protect and keeps protecting while other non-dying, non-escaped monsters remain. When alone, Protect plans Shield Bash for the next turn, then it attacks repeatedly. Protect targets a random eligible other monster, or self if none remains. Escape is a separately assigned move.",
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> GremlinTsundereTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (PROTECT, [MoveStep::GainBlockRandomMonster(block)]) => GremlinTsundereTurn::Protect(block),
        (
            BASH,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => GremlinTsundereTurn::Bash(attack),
        (ESCAPE, [MoveStep::Escape]) => GremlinTsundereTurn::Escape,
        (_, []) => panic!("gremlin tsundere plan missing locked truth"),
        (move_id, steps) => panic!(
            "gremlin tsundere plan/steps mismatch: {} {:?}",
            move_id, steps
        ),
    }
}

fn alive_monster_count(state: &CombatState) -> usize {
    state
        .entities
        .monsters
        .iter()
        // Java counts monsters that are not isDying and not isEscaping. It
        // does not check currentHealth here, so a zero-HP monster awaiting the
        // queued death path still affects this immediate branch.
        .filter(|monster| !monster.is_dying && !monster.is_escaped)
        .count()
}

impl MonsterBehavior for GremlinTsundere {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        asc: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        protect_plan(asc)
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
            GremlinTsundereTurn::Protect(block) => {
                let next_plan = if alive_monster_count(state) > 1 {
                    protect_plan(state.meta.ascension_level)
                } else {
                    bash_plan(state.meta.ascension_level)
                };
                // Java calls setMove(...) synchronously after queueing
                // GainBlockRandomMonsterAction, so the next intent is updated
                // before that queued action can execute.
                let mut actions = vec![set_next_move_action(entity, next_plan)];
                actions.push(gain_block_random_monster_action(entity, block));
                actions
            }
            GremlinTsundereTurn::Bash(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(
                    entity,
                    bash_plan(state.meta.ascension_level),
                ));
                actions
            }
            GremlinTsundereTurn::Escape => vec![
                Action::Escape { target: entity.id },
                set_next_move_action(entity, escape_plan()),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{bash_plan, escape_plan, protect_plan, GremlinTsundere};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::Action;

    #[test]
    fn protect_followup_counts_zero_hp_not_yet_dying_monsters_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let mut tsundere = crate::test_support::test_monster(EnemyId::GremlinTsundere);
        tsundere.id = 10;
        let mut pending_death = crate::test_support::test_monster(EnemyId::GremlinWarrior);
        pending_death.id = 11;
        pending_death.current_hp = 0;
        pending_death.is_dying = false;
        state.entities.monsters = vec![tsundere.clone(), pending_death];

        let actions = GremlinTsundere::take_turn_plan(&mut state, &tsundere, &protect_plan(0));

        assert!(matches!(
            actions.first(),
            Some(Action::SetMonsterMove {
                monster_id: 10,
                next_move_byte: 1,
                ..
            })
        ));
    }

    #[test]
    fn escape_turn_queues_escape_intent_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::GremlinTsundere);

        let actions = GremlinTsundere::take_turn_plan(&mut state, &entity, &escape_plan());

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

    #[test]
    fn protect_block_amount_and_solo_followup_match_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 17;
        let mut tsundere = crate::test_support::test_monster(EnemyId::GremlinTsundere);
        tsundere.id = 10;
        state.entities.monsters = vec![tsundere.clone()];

        let actions = GremlinTsundere::take_turn_plan(&mut state, &tsundere, &protect_plan(17));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::SetMonsterMove {
                    monster_id: 10,
                    next_move_byte: super::BASH,
                    ..
                },
                Action::GainBlockRandomMonster {
                    source: 10,
                    amount: 11,
                },
            ]
        ));
    }

    #[test]
    fn bash_damage_and_followup_setmove_match_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 2;
        let entity = crate::test_support::test_monster(EnemyId::GremlinTsundere);

        let actions = GremlinTsundere::take_turn_plan(&mut state, &entity, &bash_plan(2));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 8,
                    ..
                },
                Action::SetMonsterMove {
                    monster_id: 1,
                    next_move_byte: super::BASH,
                    ..
                },
            ]
        ));
    }
}
