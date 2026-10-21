use super::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

const BULL_RUSH: u8 = 1;
const SKULL_BASH: u8 = 2;
const BELLOW: u8 = 3;

pub struct GremlinNob;

enum GremlinNobTurn<'a> {
    BullRush(&'a AttackSpec),
    SkullBash(&'a AttackSpec, &'a ApplyPowerStep),
    Bellow(&'a ApplyPowerStep),
}

fn rush_damage(asc: u8) -> i32 {
    if asc >= 3 {
        16
    } else {
        14
    }
}

fn bash_damage(asc: u8) -> i32 {
    if asc >= 3 {
        8
    } else {
        6
    }
}

fn anger_amount(asc: u8) -> i32 {
    if asc >= 18 {
        3
    } else {
        2
    }
}

fn bull_rush_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        BULL_RUSH,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: rush_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn skull_bash_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        SKULL_BASH,
        smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: bash_damage(asc),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Vulnerable,
                amount: 2,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: bash_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Vulnerable,
                amount: 2,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn bellow_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        BELLOW,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Anger,
            amount: anger_amount(asc),
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        BULL_RUSH => bull_rush_plan(asc),
        SKULL_BASH => skull_bash_plan(asc),
        BELLOW => bellow_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        special_moves: vec![],
        moves: vec![("Bellow", bellow_plan(asc)), ("Skull Bash", skull_bash_plan(asc)), ("Bull Rush", bull_rush_plan(asc))],
        notes: "Bellow activates the Anger power (Enrage): subsequent Skill plays grant it Strength. At A18+, after Bellow it repeats Skull Bash, Bull Rush, Bull Rush. Below A18 the attack choice is random with history restrictions.",
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn last_move_before(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().iter().rev().nth(1).copied() == Some(move_id)
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let mut history = entity.move_history().iter().rev();
    matches!(
        (history.next().copied(), history.next().copied()),
        (Some(a), Some(b)) if a == move_id && b == move_id
    )
}

fn runtime(entity: &MonsterEntity) -> bool {
    assert!(
        entity.gremlin_nob.protocol_seeded,
        "gremlin nob runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.gremlin_nob.used_bellow
}

fn gremlin_nob_runtime_update(entity: &MonsterEntity, used_bellow: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::GremlinNob {
            used_bellow,
            protocol_seeded: Some(true),
        },
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> GremlinNobTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            BULL_RUSH,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => GremlinNobTurn::BullRush(attack),
        (
            SKULL_BASH,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Vulnerable,
                effect: PowerEffectKind::Debuff,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(power) = &plan.steps[1] else {
                unreachable!()
            };
            GremlinNobTurn::SkullBash(attack, power)
        }
        (
            BELLOW,
            [MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Anger,
                effect: PowerEffectKind::Buff,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(power) = &plan.steps[0] else {
                unreachable!()
            };
            GremlinNobTurn::Bellow(power)
        }
        (_, []) => panic!("gremlin nob plan missing locked truth"),
        (move_id, steps) => panic!("gremlin nob plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for GremlinNob {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        asc: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if !runtime(entity) {
            return bellow_plan(asc);
        }

        if asc >= 18 {
            if (!last_move(entity, SKULL_BASH) && !last_move_before(entity, SKULL_BASH))
                || last_two_moves(entity, BULL_RUSH)
            {
                skull_bash_plan(asc)
            } else {
                bull_rush_plan(asc)
            }
        } else if num < 33 || last_two_moves(entity, BULL_RUSH) {
            skull_bash_plan(asc)
        } else {
            bull_rush_plan(asc)
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if !runtime(entity) {
            vec![gremlin_nob_runtime_update(entity, Some(true))]
        } else {
            Vec::new()
        }
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
            GremlinNobTurn::BullRush(attack) => attack_actions(entity.id, PLAYER, attack),
            GremlinNobTurn::SkullBash(attack, vulnerable) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(apply_power_action(entity, vulnerable));
                actions
            }
            GremlinNobTurn::Bellow(anger) => vec![apply_power_action(entity, anger)],
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::{GremlinNob, BELLOW, BULL_RUSH, SKULL_BASH};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::{Action, MonsterRuntimePatch};

    fn nob_with_history(history: &[u8]) -> crate::runtime::combat::MonsterEntity {
        let mut monster = crate::testing::support::test_monster(EnemyId::GremlinNob);
        monster.move_history_mut().extend(history.iter().copied());
        monster.gremlin_nob.used_bellow = history.contains(&BELLOW);
        monster
    }

    #[test]
    fn gremlin_nob_first_roll_uses_private_bellow_latch_and_marks_it() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let monster = nob_with_history(&[]);
        let plan = GremlinNob::roll_move_plan(&mut rng, &monster, 0, 99);

        assert_eq!(plan.move_id, BELLOW);
        assert_eq!(
            GremlinNob::on_roll_move(0, &monster, 99, &plan),
            vec![Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::GremlinNob {
                    used_bellow: Some(true),
                    protocol_seeded: Some(true),
                },
            }]
        );
    }

    #[test]
    fn gremlin_nob_used_bellow_is_private_runtime_not_empty_history() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut monster = nob_with_history(&[]);
        monster.gremlin_nob.used_bellow = true;

        assert_eq!(
            GremlinNob::roll_move_plan(&mut rng, &monster, 0, 99).move_id,
            BULL_RUSH,
            "Java uses private usedBellow; empty imported history alone must not force Bellow"
        );
    }

    #[test]
    fn gremlin_nob_bellow_latch_is_not_inferred_from_nonempty_history() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut monster = nob_with_history(&[BULL_RUSH]);
        monster.gremlin_nob.used_bellow = false;

        assert_eq!(
            GremlinNob::roll_move_plan(&mut rng, &monster, 0, 0).move_id,
            BELLOW,
            "Java uses private usedBellow; move history must not replace the latch"
        );
    }

    #[test]
    fn gremlin_nob_a18_keeps_java_skull_bash_sequence_after_bellow() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut monster = nob_with_history(&[BELLOW, BULL_RUSH]);
        monster.gremlin_nob.used_bellow = true;

        assert_eq!(
            GremlinNob::roll_move_plan(&mut rng, &monster, 18, 99).move_id,
            SKULL_BASH
        );
    }
}
