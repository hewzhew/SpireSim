use crate::content::monsters::exordium::{
    attack_actions, gain_block_random_monster_action, PLAYER,
};
use crate::content::monsters::{MonsterBehavior, MonsterRollContext};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, AttackStep, DamageKind, DefendSpec, MonsterMoveSpec, MonsterTurnPlan, MoveStep,
    MoveTarget, RandomBlockStep,
};
use smallvec::smallvec;

pub struct Centurion;

const SLASH: u8 = 1;
const PROTECT: u8 = 2;
const FURY: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn roll_counts_zero_hp_non_dying_allies_like_java_alive_count() {
        let centurion = crate::test_support::test_monster(EnemyId::Centurion);
        let mut ally = crate::test_support::test_monster(EnemyId::Healer);
        ally.id = 2;
        ally.current_hp = 0;
        ally.is_dying = false;
        ally.is_escaped = false;
        let monsters = vec![centurion.clone(), ally];

        let plan = Centurion::roll_move_custom_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &centurion,
            0,
            65,
            &monsters,
        );

        assert_eq!(plan.move_id, PROTECT);
    }

    #[test]
    fn protect_can_block_zero_hp_non_dying_ally_like_java_action() {
        let centurion = crate::test_support::test_monster(EnemyId::Centurion);
        let mut ally = crate::test_support::test_monster(EnemyId::Healer);
        ally.id = 2;
        ally.current_hp = 0;
        ally.is_dying = false;
        ally.is_escaped = false;
        let mut state = crate::test_support::combat_with_monsters(vec![centurion, ally]);

        crate::engine::action_handlers::execute_action(
            Action::GainBlockRandomMonster {
                source: 1,
                amount: 15,
            },
            &mut state,
        );

        assert_eq!(state.entities.monsters[0].block, 0);
        assert_eq!(state.entities.monsters[1].block, 15);
    }
}

enum CenturionTurn<'a> {
    Slash(&'a AttackSpec),
    Protect(&'a RandomBlockStep),
    Fury(&'a AttackSpec),
}

fn slash_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        14
    } else {
        12
    }
}

fn fury_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        7
    } else {
        6
    }
}

fn protect_block(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        20
    } else {
        15
    }
}

fn slash_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SLASH,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: slash_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn protect_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        PROTECT,
        smallvec![MoveStep::GainBlockRandomMonster(RandomBlockStep {
            amount: protect_block(ascension_level),
        })],
        MonsterMoveSpec::Defend(DefendSpec {
            block: protect_block(ascension_level),
        }),
    )
}

fn fury_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        FURY,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: fury_damage(ascension_level),
            hits: 3,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        SLASH => slash_plan(ascension_level),
        PROTECT => protect_plan(ascension_level),
        FURY => fury_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Slash", slash_plan(asc)), ("Protect", protect_plan(asc)), ("Fury", fury_plan(asc))],
        special_moves: vec![],
        notes: "While an ally remains at move selection, chooses Slash or Protect; when alone, chooses Slash or Fury. Does not use the same move three times consecutively. Protect gives Block to a randomly chosen eligible other monster, or self if none remains when it executes. Killing an ally does not turn an already selected Protect into Fury; Fury becomes available on a subsequent move selection.",
    }
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let history = entity.move_history();
    history.len() >= 2
        && history[history.len() - 1] == move_id
        && history[history.len() - 2] == move_id
}

fn alive_monster_count(monsters: &[MonsterEntity]) -> usize {
    monsters
        .iter()
        .filter(|monster| !monster.is_dying && !monster.is_escaped)
        .count()
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> CenturionTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            SLASH,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => CenturionTurn::Slash(attack),
        (PROTECT, [MoveStep::GainBlockRandomMonster(block)]) => CenturionTurn::Protect(block),
        (
            FURY,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => CenturionTurn::Fury(attack),
        (_, []) => panic!("centurion plan missing locked truth"),
        (move_id, steps) => panic!("centurion plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl Centurion {
    fn roll_move_custom_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        monsters: &[MonsterEntity],
    ) -> MonsterTurnPlan {
        let alive_count = alive_monster_count(monsters);

        if num >= 65 && !last_two_moves(entity, PROTECT) && !last_two_moves(entity, FURY) {
            if alive_count > 1 {
                return protect_plan(ascension_level);
            }
            return fury_plan(ascension_level);
        }

        if !last_two_moves(entity, SLASH) {
            return slash_plan(ascension_level);
        }

        if alive_count > 1 {
            protect_plan(ascension_level)
        } else {
            fury_plan(ascension_level)
        }
    }
}

impl MonsterBehavior for Centurion {
    fn roll_move_plan_with_context(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        ctx: MonsterRollContext<'_>,
    ) -> MonsterTurnPlan {
        Self::roll_move_custom_plan(rng, entity, ascension_level, num, ctx.monsters)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match decode_turn(plan) {
            CenturionTurn::Slash(attack) | CenturionTurn::Fury(attack) => {
                attack_actions(entity.id, PLAYER, attack)
            }
            CenturionTurn::Protect(block) => vec![gain_block_random_monster_action(entity, block)],
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
