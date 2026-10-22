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
const WEAKEN: u8 = 4;

pub struct LouseDefensive;

enum LouseDefensiveTurn<'a> {
    Bite(&'a AttackSpec),
    Weaken(&'a ApplyPowerStep),
}

fn curl_up_amount(hp_rng: &mut crate::runtime::rng::StsRng, asc: u8) -> i32 {
    let (min, max) = super::louse_curl_up_range(asc);
    hp_rng.random_range(min, max)
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::louse_mechanics(
        asc,
        vec![(Text::new("louse_defensive.spitWeb"), weaken_plan())],
    )
}

fn bite_damage(entity: &MonsterEntity) -> i32 {
    entity
        .louse
        .bite_damage
        .unwrap_or_else(|| panic!("louse defensive missing locked bite damage"))
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

fn weaken_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        WEAKEN,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Weak,
            amount: 2,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn plan_for(entity: &MonsterEntity, move_id: u8) -> MonsterTurnPlan {
    match move_id {
        BITE => bite_plan(entity),
        WEAKEN => weaken_plan(),
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

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> LouseDefensiveTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            BITE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => LouseDefensiveTurn::Bite(attack),
        (
            WEAKEN,
            [MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                effect: PowerEffectKind::Debuff,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(power) = &plan.steps[0] else {
                unreachable!()
            };
            LouseDefensiveTurn::Weaken(power)
        }
        (_, []) => panic!("louse defensive plan missing locked truth"),
        (move_id, steps) => {
            panic!(
                "louse defensive plan/steps mismatch: {} {:?}",
                move_id, steps
            )
        }
    }
}

impl MonsterBehavior for LouseDefensive {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        asc: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if asc >= 17 {
            if num < 25 {
                if entity.move_history().back().copied() == Some(WEAKEN) {
                    bite_plan(entity)
                } else {
                    weaken_plan()
                }
            } else if last_two_moves(entity, BITE) {
                weaken_plan()
            } else {
                bite_plan(entity)
            }
        } else if num < 25 {
            if last_two_moves(entity, WEAKEN) {
                bite_plan(entity)
            } else {
                weaken_plan()
            }
        } else if last_two_moves(entity, BITE) {
            weaken_plan()
        } else {
            bite_plan(entity)
        }
    }

    fn turn_plan(_state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity, entity.planned_move_id())
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
            LouseDefensiveTurn::Bite(attack) => attack_actions(entity.id, PLAYER, attack),
            LouseDefensiveTurn::Weaken(power) => vec![apply_power_action(entity, power)],
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
