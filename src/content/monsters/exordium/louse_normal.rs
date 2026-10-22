use super::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, EffectStrength, MonsterTurnPlan, MoveStep,
    MoveTarget, PowerEffectKind,
};

const BITE: u8 = 3;
const STRENGTHEN: u8 = 4;

pub struct LouseNormal;

enum LouseNormalTurn<'a> {
    Bite(&'a AttackSpec),
    Strengthen(&'a ApplyPowerStep),
}

fn strengthen_amount(asc: u8) -> i32 {
    if asc >= 17 {
        4
    } else {
        3
    }
}

fn curl_up_amount(hp_rng: &mut crate::runtime::rng::StsRng, asc: u8) -> i32 {
    let (min, max) = super::louse_curl_up_range(asc);
    hp_rng.random_range(min, max)
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::louse_mechanics(
        asc,
        vec![(Text::new("louse_normal.grow"), strengthen_plan(asc))],
    )
}

fn bite_damage(entity: &MonsterEntity) -> i32 {
    entity
        .louse
        .bite_damage
        .unwrap_or_else(|| panic!("louse normal missing locked bite damage"))
}

fn bite_plan(entity: &MonsterEntity) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        BITE,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: bite_damage(entity),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn strengthen_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        STRENGTHEN,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Strength,
            amount: strengthen_amount(asc),
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn plan_for(entity: &MonsterEntity, move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        BITE => bite_plan(entity),
        STRENGTHEN => strengthen_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let mut history = entity.move_history().iter().rev();
    matches!(
        (history.next().copied(), history.next().copied()),
        (Some(a), Some(b)) if a == move_id && b == move_id
    )
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> LouseNormalTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            BITE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => LouseNormalTurn::Bite(attack),
        (
            STRENGTHEN,
            [MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Strength,
                effect: PowerEffectKind::Buff,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(power) = &plan.steps[0] else {
                unreachable!()
            };
            LouseNormalTurn::Strengthen(power)
        }
        (_, []) => panic!("louse normal plan missing locked truth"),
        (move_id, steps) => panic!("louse normal plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for LouseNormal {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        asc: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if asc >= 17 {
            if num < 25 {
                if entity.move_history().back().copied() == Some(STRENGTHEN) {
                    bite_plan(entity)
                } else {
                    strengthen_plan(asc)
                }
            } else if last_two_moves(entity, BITE) {
                strengthen_plan(asc)
            } else {
                bite_plan(entity)
            }
        } else if num < 25 {
            if last_two_moves(entity, STRENGTHEN) {
                bite_plan(entity)
            } else {
                strengthen_plan(asc)
            }
        } else if last_two_moves(entity, BITE) {
            strengthen_plan(asc)
        } else {
            bite_plan(entity)
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity, entity.planned_move_id(), state.meta.ascension_level)
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (hp_rng, asc) = crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::CurlUp,
            amount: curl_up_amount(hp_rng, asc),
        }]
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match decode_turn(plan) {
            LouseNormalTurn::Bite(attack) => attack_actions(entity.id, PLAYER, attack),
            LouseNormalTurn::Strengthen(power) => vec![apply_power_action(entity, power)],
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
