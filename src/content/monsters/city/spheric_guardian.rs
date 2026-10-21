use crate::content::monsters::exordium::{
    apply_power_action, attack_actions, gain_block_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, BlockStep, DamageKind, DebuffSpec, DefendSpec,
    EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};

pub struct SphericGuardian;

const SLAM: u8 = 1;
const HARDEN: u8 = 2;
const BASH_AND_BLOCK: u8 = 3;
const BASH_AND_FRAIL: u8 = 4;

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.spheric_guardian.protocol_seeded = true;
    entity.spheric_guardian.first_move = true;
    entity.spheric_guardian.second_move = true;
}

enum SphericGuardianTurn<'a> {
    Slam(&'a AttackSpec),
    Harden(&'a BlockStep),
    BashAndBlock(&'a AttackSpec, &'a BlockStep),
    BashAndFrail(&'a AttackSpec, &'a ApplyPowerStep),
}

fn bash_damage(asc: u8) -> i32 {
    if asc >= 2 {
        11
    } else {
        10
    }
}

fn harden_block(asc: u8) -> i32 {
    if asc >= 17 {
        35
    } else {
        25
    }
}

fn slam_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SLAM,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: bash_damage(asc),
            hits: 2,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn harden_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        HARDEN,
        MonsterMoveSpec::Defend(DefendSpec {
            block: harden_block(asc),
        }),
    )
}

fn bash_and_block_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BASH_AND_BLOCK,
        MonsterMoveSpec::AttackDefend(
            AttackSpec {
                base_damage: bash_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DefendSpec { block: 15 },
        ),
    )
}

fn bash_and_frail_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BASH_AND_FRAIL,
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: bash_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Frail,
                amount: 5,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        SLAM => slam_plan(asc),
        HARDEN => harden_plan(asc),
        BASH_AND_BLOCK => bash_and_block_plan(asc),
        BASH_AND_FRAIL => bash_and_frail_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        special_moves: vec![],
        moves: vec![
            ("Harden", harden_plan(asc)),
            ("Bash and Frail", bash_and_frail_plan(asc)),
            ("Slam", slam_plan(asc)),
            ("Bash and Block", bash_and_block_plan(asc)),
        ],
        notes: "Starts with 20 HP, 40 Block, 3 Artifact and Barricade. Barricade preserves its Block between turns. Opens with Harden, then Bash and Frail; afterward alternates Slam and Bash and Block. Artifact blocks incoming debuffs one at a time.",
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> SphericGuardianTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            SLAM,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => SphericGuardianTurn::Slam(attack),
        (
            HARDEN,
            [MoveStep::GainBlock(
                block @ BlockStep {
                    target: MoveTarget::SelfTarget,
                    ..
                },
            )],
        ) => SphericGuardianTurn::Harden(block),
        (
            BASH_AND_BLOCK,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::GainBlock(
                block @ BlockStep {
                    target: MoveTarget::SelfTarget,
                    ..
                },
            )],
        ) => SphericGuardianTurn::BashAndBlock(attack, block),
        (
            BASH_AND_FRAIL,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(
                power @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Frail,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            )],
        ) => SphericGuardianTurn::BashAndFrail(attack, power),
        (_, []) => panic!("spheric guardian plan missing locked truth"),
        (move_id, steps) => panic!(
            "spheric guardian plan/steps mismatch: {} {:?}",
            move_id, steps
        ),
    }
}

impl MonsterBehavior for SphericGuardian {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Barricade,
                amount: -1,
            },
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Artifact,
                amount: 3,
            },
            Action::GainBlock {
                target: entity.id,
                amount: 40,
            },
        ]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        let (first_move, second_move) = runtime(entity);
        if first_move {
            return harden_plan(ascension_level);
        }
        if second_move {
            return bash_and_frail_plan(ascension_level);
        }
        if entity.move_history().back().copied() == Some(SLAM) {
            bash_and_block_plan(ascension_level)
        } else {
            slam_plan(ascension_level)
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let (first_move, second_move) = runtime(entity);
        if first_move {
            vec![spheric_guardian_runtime_update(entity, Some(false), None)]
        } else if second_move {
            vec![spheric_guardian_runtime_update(entity, None, Some(false))]
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
            SphericGuardianTurn::Slam(attack) => attack_actions(entity.id, PLAYER, attack),
            SphericGuardianTurn::Harden(block) => vec![gain_block_action(entity, block)],
            SphericGuardianTurn::BashAndBlock(attack, block) => {
                let mut actions = vec![gain_block_action(entity, block)];
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions
            }
            SphericGuardianTurn::BashAndFrail(attack, power) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(apply_power_action(entity, power));
                actions
            }
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

fn runtime(entity: &MonsterEntity) -> (bool, bool) {
    assert!(
        entity.spheric_guardian.protocol_seeded,
        "spheric guardian runtime truth must be protocol-seeded or factory-seeded"
    );
    (
        entity.spheric_guardian.first_move,
        entity.spheric_guardian.second_move,
    )
}

fn spheric_guardian_runtime_update(
    entity: &MonsterEntity,
    first_move: Option<bool>,
    second_move: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::SphericGuardian {
            first_move,
            second_move,
            protocol_seeded: Some(true),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{bash_and_block_plan, SphericGuardian, BASH_AND_FRAIL, HARDEN, SLAM};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::content::powers::PowerId;
    use crate::runtime::action::{Action, MonsterRuntimePatch};

    #[test]
    fn pre_battle_applies_barricade_artifact_and_block_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let guardian = crate::test_support::test_monster(EnemyId::SphericGuardian);

        let actions = SphericGuardian::use_pre_battle_actions(
            &mut state,
            &guardian,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );

        assert!(matches!(
            actions.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Barricade,
                    amount: -1
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Artifact,
                    amount: 3
                },
                Action::GainBlock {
                    target: 1,
                    amount: 40
                }
            ]
        ));
    }

    #[test]
    fn bash_and_block_gains_block_before_attack_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let guardian = crate::test_support::test_monster(EnemyId::SphericGuardian);
        let actions =
            SphericGuardian::take_turn_plan(&mut state, &guardian, &bash_and_block_plan(0));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::GainBlock {
                    target: 1,
                    amount: 15
                },
                Action::MonsterAttack { source: 1, .. },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn first_roll_uses_private_first_move_and_marks_it_used_like_java_get_move() {
        let state = crate::test_support::blank_test_combat();
        let mut guardian = crate::test_support::test_monster(EnemyId::SphericGuardian);
        guardian.move_history_mut().extend([SLAM, SLAM]);

        let plan = SphericGuardian::roll_move_plan(
            &mut state.rng.ai_rng.clone(),
            &guardian,
            state.meta.ascension_level,
            99,
        );
        let setup = SphericGuardian::on_roll_move(state.meta.ascension_level, &guardian, 99, &plan);

        assert_eq!(plan.move_id, HARDEN);
        assert!(matches!(
            setup.as_slice(),
            [Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::SphericGuardian {
                    first_move: Some(false),
                    second_move: None,
                    protocol_seeded: Some(true)
                }
            }]
        ));
    }

    #[test]
    fn second_roll_uses_private_second_move_even_without_history() {
        let state = crate::test_support::blank_test_combat();
        let mut guardian = crate::test_support::test_monster(EnemyId::SphericGuardian);
        guardian.spheric_guardian.first_move = false;
        guardian.spheric_guardian.second_move = true;
        guardian.move_history_mut().clear();

        let plan = SphericGuardian::roll_move_plan(
            &mut state.rng.ai_rng.clone(),
            &guardian,
            state.meta.ascension_level,
            99,
        );
        let setup = SphericGuardian::on_roll_move(state.meta.ascension_level, &guardian, 99, &plan);

        assert_eq!(plan.move_id, BASH_AND_FRAIL);
        assert!(matches!(
            setup.as_slice(),
            [Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::SphericGuardian {
                    first_move: None,
                    second_move: Some(false),
                    protocol_seeded: Some(true)
                }
            }]
        ));
    }

    #[test]
    fn post_opening_uses_java_last_move_slam_branch() {
        let state = crate::test_support::blank_test_combat();
        let mut guardian = crate::test_support::test_monster(EnemyId::SphericGuardian);
        guardian.spheric_guardian.first_move = false;
        guardian.spheric_guardian.second_move = false;
        guardian.move_history_mut().clear();
        guardian.move_history_mut().push_back(SLAM);

        let plan = SphericGuardian::roll_move_plan(
            &mut state.rng.ai_rng.clone(),
            &guardian,
            state.meta.ascension_level,
            99,
        );

        assert_eq!(plan.move_id, super::BASH_AND_BLOCK);
    }
}
