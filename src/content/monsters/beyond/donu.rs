use crate::content::monsters::exordium::{attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, DonuRuntimeState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, BuffSpec, DamageKind, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct Donu;

const BEAM: u8 = 0;
const CIRCLE_OF_PROTECTION: u8 = 2;

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.donu = DonuRuntimeState {
        protocol_seeded: true,
        is_attacking: false,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn pre_battle_artifact_amount_matches_java_ascension_gate() {
        let mut state = crate::test_support::blank_test_combat();
        let donu = crate::test_support::test_monster(EnemyId::Donu);

        let normal = Donu::use_pre_battle_actions(
            &mut state,
            &donu,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );
        state.meta.ascension_level = 19;
        let asc19 = Donu::use_pre_battle_actions(
            &mut state,
            &donu,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );

        assert!(matches!(
            normal.as_slice(),
            [Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::Artifact,
                amount: 2
            }]
        ));
        assert!(matches!(
            asc19.as_slice(),
            [Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::Artifact,
                amount: 3
            }]
        ));
    }

    #[test]
    fn imported_is_attacking_true_with_empty_history_rolls_beam() {
        let mut donu = crate::test_support::test_monster(EnemyId::Donu);
        donu.donu.is_attacking = true;
        donu.move_history_mut().clear();

        let plan = Donu::roll_move_plan(&mut StsRng::new(0), &donu, 0, 0);

        assert_eq!(
            plan.move_id, BEAM,
            "Java Donu.getMove gates on private isAttacking, not move history"
        );
    }

    #[test]
    fn circle_turn_sets_private_is_attacking_true_before_roll() {
        let mut donu_entity = crate::test_support::test_monster(EnemyId::Donu);
        donu_entity.id = 1;
        let mut deca_entity = crate::test_support::test_monster(EnemyId::Deca);
        deca_entity.id = 2;
        deca_entity.current_hp = 0;
        deca_entity.is_dying = false;
        deca_entity.is_escaped = false;
        let mut state = crate::test_support::combat_with_monsters(vec![
            donu_entity.clone(),
            deca_entity.clone(),
        ]);
        let donu = state.entities.monsters[0].clone();
        let plan = circle_plan();

        let actions = Donu::take_turn_plan(&mut state, &donu, &plan);

        assert!(matches!(
            actions.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Strength,
                    amount: 3
                },
                Action::ApplyPower {
                    source: 1,
                    target: 2,
                    power_id: PowerId::Strength,
                    amount: 3
                },
                Action::UpdateMonsterRuntime {
                    patch: MonsterRuntimePatch::Donu {
                        is_attacking: Some(true),
                        protocol_seeded: Some(true),
                    },
                    ..
                },
                Action::RollMonsterMove { .. },
            ]
        ));
    }

    #[test]
    fn beam_turn_sets_private_is_attacking_false_before_roll() {
        let mut state =
            crate::test_support::combat_with_monsters(vec![crate::test_support::test_monster(
                EnemyId::Donu,
            )]);
        let mut donu = state.entities.monsters[0].clone();
        donu.donu.is_attacking = true;
        let plan = beam_plan(4);

        let actions = Donu::take_turn_plan(&mut state, &donu, &plan);

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 12,
                    damage_kind: DamageKind::Normal
                },
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 12,
                    damage_kind: DamageKind::Normal
                },
                Action::UpdateMonsterRuntime {
                    patch: MonsterRuntimePatch::Donu {
                        is_attacking: Some(false),
                        protocol_seeded: Some(true),
                    },
                    ..
                },
                Action::RollMonsterMove { .. },
            ]
        ));
    }
}

fn beam_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 4 {
        12
    } else {
        10
    }
}

fn beam_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BEAM,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: beam_damage(ascension_level),
            hits: 2,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn circle_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        CIRCLE_OF_PROTECTION,
        smallvec![MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::AllMonsters,
            power_id: PowerId::Strength,
            amount: 3,
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        })],
        MonsterMoveSpec::Buff(BuffSpec {
            power_id: PowerId::Strength,
            amount: 3,
        }),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        BEAM => beam_plan(ascension_level),
        CIRCLE_OF_PROTECTION => circle_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("donu.beam"), beam_plan(asc)),
            (Text::new("donu.circleOfPower"), circle_plan()),
        ],
        special_moves: vec![],
        notes: Text::new("donu.notes"),
    }
}

fn runtime(entity: &MonsterEntity) -> &DonuRuntimeState {
    assert!(
        entity.donu.protocol_seeded,
        "donu runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.donu
}

fn donu_runtime_update(entity: &MonsterEntity, is_attacking: bool) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Donu {
            is_attacking: Some(is_attacking),
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for Donu {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::Artifact,
            amount: if ascension_level >= 19 { 3 } else { 2 },
        }]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if runtime(entity).is_attacking {
            beam_plan(ascension_level)
        } else {
            circle_plan()
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (
                BEAM,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                })],
            ) => attack_actions(entity.id, PLAYER, attack),
            (
                CIRCLE_OF_PROTECTION,
                [MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::AllMonsters,
                    power_id: PowerId::Strength,
                    amount,
                    effect: PowerEffectKind::Buff,
                    ..
                })],
            ) => state
                .entities
                .monsters
                .iter()
                .map(|monster| Action::ApplyPower {
                    source: entity.id,
                    target: monster.id,
                    power_id: PowerId::Strength,
                    amount: *amount,
                })
                .collect(),
            (_, []) => panic!("donu plan missing locked truth"),
            (move_id, steps) => panic!("donu plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(donu_runtime_update(
            entity,
            plan.move_id == CIRCLE_OF_PROTECTION,
        ));
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
