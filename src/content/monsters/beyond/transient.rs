use crate::content::monsters::exordium::{attack_actions, set_next_move_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity, TransientRuntimeState};
use crate::semantics::combat::{
    AttackSpec, DamageKind, MonsterMoveSpec, MonsterTurnPlan, MoveStep,
};

pub struct Transient;

const ATTACK: u8 = 1;

fn starting_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        40
    } else {
        30
    }
}

fn starting_fading(asc: u8) -> i32 {
    if asc >= 17 {
        6
    } else {
        5
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Opening attack", attack_plan_for_count(asc, 0))],
        special_moves: vec![("Later attacks", format!("Each completed attack increases the next attack's unmodified base damage by 10 (starting at {}). Starts with {} Fading and Shifting. Shifting temporarily reduces Strength by HP lost, so current visible attack damage can be much lower than the base.", starting_damage(asc), starting_fading(asc)))],
        notes: "Always attacks once per turn, then the Fading countdown runs after its queued move. At 1 Fading it dies after that turn's attack; you can survive until this happens instead of reducing all 999 HP. See info power Fading and info power Shifting for their exact effects and timing.",
    }
}

fn attack_damage_for_count(ascension_level: u8, count: usize) -> i32 {
    starting_damage(ascension_level) + (count as i32 * 10)
}

fn attack_plan_for_count(ascension_level: u8, count: usize) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        ATTACK,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: attack_damage_for_count(ascension_level, count),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.transient = TransientRuntimeState {
        protocol_seeded: true,
        count: 0,
    };
}

fn runtime(entity: &MonsterEntity) -> &TransientRuntimeState {
    assert!(
        entity.transient.protocol_seeded,
        "transient runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.transient
}

fn current_attack_count(entity: &MonsterEntity) -> usize {
    runtime(entity).count.max(0) as usize
}

fn transient_runtime_update(entity: &MonsterEntity, count: i32) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Transient {
            count: Some(count),
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for Transient {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        attack_plan_for_count(ascension_level, current_attack_count(entity))
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Fading,
                amount: starting_fading(ascension_level),
            },
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Shifting,
                amount: -1,
            },
        ]
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        match entity.planned_move_id() {
            ATTACK => {
                attack_plan_for_count(state.meta.ascension_level, current_attack_count(entity))
            }
            other => MonsterTurnPlan::unknown(other),
        }
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match (plan.move_id, plan.steps.as_slice()) {
            (ATTACK, [MoveStep::Attack(attack)]) => {
                let next_count = runtime(entity).count + 1;
                let mut actions = vec![transient_runtime_update(entity, next_count)];
                actions.push(set_next_move_action(
                    entity,
                    attack_plan_for_count(state.meta.ascension_level, next_count.max(0) as usize),
                ));
                actions.extend(attack_actions(entity.id, PLAYER, &attack.attack));
                actions
            }
            (move_id, steps) => panic!("transient plan/steps mismatch: {} {:?}", move_id, steps),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn pre_battle_fading_and_shifting_match_java_amounts() {
        let mut state = crate::test_support::blank_test_combat();
        let transient = crate::test_support::test_monster(EnemyId::Transient);

        let normal = Transient::use_pre_battle_actions(
            &mut state,
            &transient,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );
        state.meta.ascension_level = 17;
        let asc17 = Transient::use_pre_battle_actions(
            &mut state,
            &transient,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );

        assert!(matches!(
            normal.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Fading,
                    amount: 5
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Shifting,
                    amount: -1
                }
            ]
        ));
        assert!(matches!(
            asc17.as_slice(),
            [
                Action::ApplyPower {
                    power_id: PowerId::Fading,
                    amount: 6,
                    ..
                },
                Action::ApplyPower {
                    power_id: PowerId::Shifting,
                    amount: -1,
                    ..
                }
            ]
        ));
    }

    #[test]
    fn imported_count_not_history_length_drives_attack_damage() {
        let mut transient = crate::test_support::test_monster(EnemyId::Transient);
        transient.transient.count = 3;
        transient.move_history_mut().clear();

        let plan = Transient::roll_move_plan(&mut StsRng::new(0), &transient, 0, 0);

        assert_eq!(plan.move_id, ATTACK);
        assert_eq!(
            plan.attack().map(|attack| attack.base_damage),
            Some(60),
            "Java uses private Transient.count, not reconstructed move-history length"
        );
    }

    #[test]
    fn turn_plan_uses_runtime_count_not_history_length() {
        let mut transient = crate::test_support::test_monster(EnemyId::Transient);
        transient.transient.count = 2;
        transient
            .move_history_mut()
            .extend([ATTACK, ATTACK, ATTACK, ATTACK]);
        transient.set_planned_move_id(ATTACK);
        let mut state = crate::test_support::combat_with_monsters(vec![transient.clone()]);
        state.meta.ascension_level = 2;

        let plan = Transient::turn_plan(&state, &transient);

        assert_eq!(plan.attack().map(|attack| attack.base_damage), Some(60));
    }

    #[test]
    fn take_turn_updates_count_and_next_move_before_queued_damage_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 0;
        state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::Transient)];
        let mut transient = state.entities.monsters[0].clone();
        transient.transient.count = 1;
        let plan = attack_plan_for_count(state.meta.ascension_level, 1);

        let actions = Transient::take_turn_plan(&mut state, &transient, &plan);

        assert!(matches!(
            actions.as_slice(),
            [
                Action::UpdateMonsterRuntime {
                    patch: MonsterRuntimePatch::Transient {
                        count: Some(2),
                        protocol_seeded: Some(true),
                    },
                    ..
                },
                Action::SetMonsterMove {
                    next_move_byte: ATTACK,
                    ..
                },
                Action::MonsterAttack {
                    base_damage: 40,
                    ..
                },
            ]
        ));
        match &actions[1] {
            Action::SetMonsterMove {
                planned_visible_spec,
                ..
            } => assert_eq!(
                planned_visible_spec
                    .as_ref()
                    .and_then(|spec| spec.attack().map(|attack| attack.base_damage)),
                Some(50)
            ),
            other => panic!("expected SetMonsterMove, got {other:?}"),
        }
    }
}
