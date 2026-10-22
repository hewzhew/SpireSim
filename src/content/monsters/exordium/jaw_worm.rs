use super::{apply_power_action, attack_actions, gain_block_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, BlockStep, BuffSpec, DamageKind, DefendSpec,
    EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

const CHOMP: u8 = 1;
const BELLOW: u8 = 2;
const THRASH: u8 = 3;

pub struct JawWorm;

pub fn initialize_runtime_state(entity: &mut MonsterEntity, hard_mode: bool) {
    entity.jaw_worm.protocol_seeded = true;
    entity.jaw_worm.first_move = !hard_mode;
    entity.jaw_worm.hard_mode = hard_mode;
}

enum JawWormTurn<'a> {
    Chomp(&'a AttackSpec),
    Bellow(&'a ApplyPowerStep, &'a BlockStep),
    Thrash(&'a AttackSpec, &'a BlockStep),
}

fn chomp_damage(asc: u8) -> i32 {
    if asc >= 2 {
        12
    } else {
        11
    }
}

fn bellow_strength(asc: u8) -> i32 {
    if asc >= 17 {
        5
    } else if asc >= 2 {
        4
    } else {
        3
    }
}

fn bellow_block(asc: u8) -> i32 {
    if asc >= 17 {
        9
    } else {
        6
    }
}

fn chomp_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        CHOMP,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: chomp_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn bellow_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        BELLOW,
        smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Strength,
                amount: bellow_strength(asc),
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::GainBlock(BlockStep {
                target: MoveTarget::SelfTarget,
                amount: bellow_block(asc),
            }),
        ],
        MonsterMoveSpec::DefendBuff(
            DefendSpec {
                block: bellow_block(asc),
            },
            BuffSpec {
                power_id: PowerId::Strength,
                amount: bellow_strength(asc),
            },
        ),
    )
}

fn thrash_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        THRASH,
        smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: 7,
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::GainBlock(BlockStep {
                target: MoveTarget::SelfTarget,
                amount: 5,
            }),
        ],
        MonsterMoveSpec::AttackDefend(
            AttackSpec {
                base_damage: 7,
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DefendSpec { block: 5 },
        ),
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        CHOMP => chomp_plan(asc),
        BELLOW => bellow_plan(asc),
        THRASH => thrash_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        special_moves: vec![],
        moves: vec![
            (Text::new("jaw_worm.chomp"), chomp_plan(asc)),
            (Text::new("jaw_worm.bellow"), bellow_plan(asc)),
            (Text::new("jaw_worm.thrash"), thrash_plan()),
        ],
        notes: Text::new("jaw_worm.notes"),
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let mut history = entity.move_history().iter().rev();
    matches!(
        (history.next().copied(), history.next().copied()),
        (Some(a), Some(b)) if a == move_id && b == move_id
    )
}

fn runtime(entity: &MonsterEntity) -> (bool, bool) {
    assert!(
        entity.jaw_worm.protocol_seeded,
        "jaw worm runtime truth must be protocol-seeded or factory-seeded"
    );
    (entity.jaw_worm.first_move, entity.jaw_worm.hard_mode)
}

fn jaw_worm_runtime_update(
    entity: &MonsterEntity,
    first_move: Option<bool>,
    hard_mode: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::JawWorm {
            first_move,
            hard_mode,
            protocol_seeded: Some(true),
        },
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> JawWormTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            CHOMP,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => JawWormTurn::Chomp(attack),
        (
            BELLOW,
            [MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Strength,
                effect: PowerEffectKind::Buff,
                ..
            }), MoveStep::GainBlock(BlockStep {
                target: MoveTarget::SelfTarget,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(power) = &plan.steps[0] else {
                unreachable!()
            };
            let MoveStep::GainBlock(block) = &plan.steps[1] else {
                unreachable!()
            };
            JawWormTurn::Bellow(power, block)
        }
        (
            THRASH,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::GainBlock(BlockStep {
                target: MoveTarget::SelfTarget,
                ..
            })],
        ) => {
            let MoveStep::GainBlock(block) = &plan.steps[1] else {
                unreachable!()
            };
            JawWormTurn::Thrash(attack, block)
        }
        (_, []) => panic!("jaw worm plan missing locked truth"),
        (move_id, steps) => panic!("jaw worm plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for JawWorm {
    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        asc: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        let (first_move, _hard_mode) = runtime(entity);
        if first_move {
            return chomp_plan(asc);
        }

        if num < 25 {
            if last_move(entity, CHOMP) {
                if rng.random_boolean_chance(0.5625) {
                    bellow_plan(asc)
                } else {
                    thrash_plan()
                }
            } else {
                chomp_plan(asc)
            }
        } else if num < 55 {
            if last_two_moves(entity, THRASH) {
                if rng.random_boolean_chance(0.357) {
                    chomp_plan(asc)
                } else {
                    bellow_plan(asc)
                }
            } else {
                thrash_plan()
            }
        } else if last_move(entity, BELLOW) {
            if rng.random_boolean_chance(0.416) {
                chomp_plan(asc)
            } else {
                thrash_plan()
            }
        } else {
            bellow_plan(asc)
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let (first_move, _hard_mode) = runtime(entity);
        if first_move {
            vec![jaw_worm_runtime_update(entity, Some(false), None)]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, asc) = crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        let (_first_move, hard_mode) = runtime(entity);
        if hard_mode {
            vec![
                apply_power_action(
                    entity,
                    &ApplyPowerStep {
                        target: MoveTarget::SelfTarget,
                        power_id: PowerId::Strength,
                        amount: bellow_strength(asc),
                        effect: PowerEffectKind::Buff,
                        visible_strength: EffectStrength::Normal,
                    },
                ),
                gain_block_action(
                    entity,
                    &BlockStep {
                        target: MoveTarget::SelfTarget,
                        amount: bellow_block(asc),
                    },
                ),
            ]
        } else {
            Vec::new()
        }
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match decode_turn(plan) {
            JawWormTurn::Chomp(attack) => attack_actions(entity.id, PLAYER, attack),
            JawWormTurn::Bellow(power, block) => {
                vec![
                    apply_power_action(entity, power),
                    gain_block_action(entity, block),
                ]
            }
            JawWormTurn::Thrash(attack, block) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(gain_block_action(entity, block));
                actions
            }
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::{initialize_runtime_state, JawWorm, BELLOW, CHOMP};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::{Action, MonsterRuntimePatch};

    #[test]
    fn jaw_worm_first_roll_uses_private_first_move_and_marks_it() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let monster = crate::testing::support::test_monster(EnemyId::JawWorm);
        let plan = JawWorm::roll_move_plan(&mut rng, &monster, 0, 99);

        assert_eq!(plan.move_id, CHOMP);
        assert_eq!(
            JawWorm::on_roll_move(0, &monster, 99, &plan),
            vec![Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::JawWorm {
                    first_move: Some(false),
                    hard_mode: None,
                    protocol_seeded: Some(true),
                },
            }]
        );
    }

    #[test]
    fn jaw_worm_first_move_is_private_runtime_not_empty_history() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut monster = crate::testing::support::test_monster(EnemyId::JawWorm);
        monster.jaw_worm.first_move = false;
        monster.move_history_mut().clear();

        assert_eq!(
            JawWorm::roll_move_plan(&mut rng, &monster, 0, 99).move_id,
            BELLOW,
            "Java uses private firstMove; empty imported history alone must not force opening Chomp"
        );
    }

    #[test]
    fn jaw_worm_hard_mode_initialization_matches_java_constructor_side_effect() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut monster = crate::testing::support::test_monster(EnemyId::JawWorm);

        initialize_runtime_state(&mut monster, true);

        assert!(monster.jaw_worm.protocol_seeded);
        assert!(monster.jaw_worm.hard_mode);
        assert!(!monster.jaw_worm.first_move);
        assert_eq!(
            JawWorm::roll_move_plan(&mut rng, &monster, 0, 99).move_id,
            BELLOW
        );
    }
}
