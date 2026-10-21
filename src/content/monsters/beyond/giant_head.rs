use crate::content::monsters::exordium::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, GiantHeadRuntimeState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, DamageKind, DebuffSpec, EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep,
};

pub struct GiantHead;

const GLARE: u8 = 1;
const IT_IS_TIME: u8 = 2;
const COUNT: u8 = 3;

pub const STARTING_POWERS: [(PowerId, i32); 1] = [(PowerId::Slow, 0)];

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            ("Glare", glare_plan()),
            ("Count", count_plan()),
            (
                "It Is Time (first heavy attack)",
                it_is_time_plan_for_count(0, asc),
            ),
            (
                "It Is Time (maximum base attack)",
                it_is_time_plan_for_count(-6, asc),
            ),
        ],
        special_moves: vec![],
        notes: if asc >= 18 {
            "Starts with Slow at counter 0; see its per-card damage amplification and round reset. Before the heavy phase, chooses Glare or Count and cannot use either three times consecutively. At A18+, the first It Is Time is on enemy turn 4, then repeats every turn, adding 5 base damage per turn until the displayed maximum. Current Strength and Weak modify these base attacks; use the visible intent for actual damage."
        } else {
            "Starts with Slow at counter 0; see its per-card damage amplification and round reset. Before the heavy phase, chooses Glare or Count and cannot use either three times consecutively. The first It Is Time is on enemy turn 5, then repeats every turn, adding 5 base damage per turn until the displayed maximum. Current Strength and Weak modify these base attacks; use the visible intent for actual damage."
        },
    }
}

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.giant_head = GiantHeadRuntimeState {
        protocol_seeded: true,
        count: 5,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn a18_pre_battle_decrements_private_count_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 18;
        state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::GiantHead)];
        let entity = state.entities.monsters[0].clone();

        let actions = GiantHead::use_pre_battle_actions(
            &mut state,
            &entity,
            crate::content::monsters::PreBattleLegacyRng::MonsterHp,
        );

        assert_eq!(state.entities.monsters[0].giant_head.count, 4);
        assert!(matches!(
            actions.as_slice(),
            [Action::ApplyPower {
                power_id: PowerId::Slow,
                amount: 0,
                ..
            }]
        ));
    }

    #[test]
    fn roll_move_updates_private_count_before_planning_damage() {
        let mut giant = crate::test_support::test_monster(EnemyId::GiantHead);
        giant.giant_head.count = 1;

        let plan = GiantHead::roll_move_plan(&mut StsRng::new(0), &giant, 0, 99);
        let actions = GiantHead::on_roll_move(0, &giant, 99, &plan);

        assert_eq!(plan.move_id, IT_IS_TIME);
        assert_eq!(plan.attack().map(|attack| attack.base_damage), Some(30));
        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::GiantHead {
                    count: Some(0),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }

    #[test]
    fn imported_count_not_history_length_drives_it_is_time() {
        let mut giant = crate::test_support::test_monster(EnemyId::GiantHead);
        giant.giant_head.count = -1;
        giant
            .move_history_mut()
            .extend([GLARE, COUNT, GLARE, COUNT]);

        let plan = GiantHead::roll_move_plan(&mut StsRng::new(0), &giant, 0, 99);

        assert_eq!(plan.move_id, IT_IS_TIME);
        assert_eq!(
            plan.attack().map(|attack| attack.base_damage),
            Some(40),
            "Java uses private count, not reconstructed move-history length"
        );
    }

    #[test]
    fn roll_move_last_two_glare_forces_count_and_decrements_private_count() {
        let mut giant = crate::test_support::test_monster(EnemyId::GiantHead);
        giant.giant_head.count = 5;
        giant.move_history_mut().extend([GLARE, GLARE]);

        let plan = GiantHead::roll_move_plan(&mut StsRng::new(0), &giant, 0, 0);
        let actions = GiantHead::on_roll_move(0, &giant, 0, &plan);

        assert_eq!(plan.move_id, COUNT);
        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::GiantHead {
                    count: Some(4),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }

    #[test]
    fn roll_move_last_two_count_forces_glare_and_decrements_private_count() {
        let mut giant = crate::test_support::test_monster(EnemyId::GiantHead);
        giant.giant_head.count = 5;
        giant.move_history_mut().extend([COUNT, COUNT]);

        let plan = GiantHead::roll_move_plan(&mut StsRng::new(0), &giant, 0, 99);
        let actions = GiantHead::on_roll_move(0, &giant, 99, &plan);

        assert_eq!(plan.move_id, GLARE);
        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::GiantHead {
                    count: Some(4),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }

    #[test]
    fn it_is_time_count_stops_decrementing_at_java_floor() {
        let mut giant = crate::test_support::test_monster(EnemyId::GiantHead);
        giant.giant_head.count = -6;

        let plan = GiantHead::roll_move_plan(&mut StsRng::new(0), &giant, 3, 99);
        let actions = GiantHead::on_roll_move(3, &giant, 99, &plan);

        assert_eq!(plan.move_id, IT_IS_TIME);
        assert_eq!(
            plan.attack().map(|attack| attack.base_damage),
            Some(70),
            "Java stops decrementing count below -6, capping Giant Head's real damage table at starting damage + 30"
        );
        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::GiantHead {
                    count: Some(-6),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }
}

fn starting_death_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 3 {
        40
    } else {
        30
    }
}

fn runtime(entity: &MonsterEntity) -> &GiantHeadRuntimeState {
    assert!(
        entity.giant_head.protocol_seeded,
        "giant head runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.giant_head
}

fn next_count_after_java_get_move(count: i32) -> i32 {
    if count > -6 {
        count - 1
    } else {
        count
    }
}

fn it_is_time_damage_for_count(count: i32, ascension_level: u8) -> i32 {
    starting_death_damage(ascension_level) - (count * 5)
}

fn glare_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        GLARE,
        MonsterMoveSpec::Debuff(DebuffSpec {
            power_id: PowerId::Weak,
            amount: 1,
            strength: EffectStrength::Normal,
        }),
    )
}

fn count_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        COUNT,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 13,
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn it_is_time_plan_for_count(count: i32, ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        IT_IS_TIME,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: it_is_time_damage_for_count(count, ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn it_is_time_plan(entity: &MonsterEntity, ascension_level: u8) -> MonsterTurnPlan {
    it_is_time_plan_for_count(runtime(entity).count, ascension_level)
}

fn plan_for(entity: &MonsterEntity, ascension_level: u8, move_id: u8) -> MonsterTurnPlan {
    match move_id {
        GLARE => glare_plan(),
        COUNT => count_plan(),
        IT_IS_TIME => it_is_time_plan(entity, ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let history = entity.move_history();
    history.len() >= 2
        && history[history.len() - 1] == move_id
        && history[history.len() - 2] == move_id
}

impl MonsterBehavior for GiantHead {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        let count = runtime(entity).count;
        let next_count = next_count_after_java_get_move(count);
        if count <= 1 {
            return it_is_time_plan_for_count(next_count, ascension_level);
        }

        if num < 50 {
            if !last_two_moves(entity, GLARE) {
                glare_plan()
            } else {
                count_plan()
            }
        } else if !last_two_moves(entity, COUNT) {
            count_plan()
        } else {
            glare_plan()
        }
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        if ascension_level >= 18 {
            if let Some(monster) = state
                .entities
                .monsters
                .iter_mut()
                .find(|monster| monster.id == entity.id)
            {
                assert!(
                    monster.giant_head.protocol_seeded,
                    "giant head runtime truth must be protocol-seeded or factory-seeded"
                );
                monster.giant_head.count -= 1;
            }
        }
        STARTING_POWERS
            .iter()
            .map(|&(power_id, amount)| Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id,
                amount,
            })
            .collect()
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        vec![Action::UpdateMonsterRuntime {
            monster_id: entity.id,
            patch: MonsterRuntimePatch::GiantHead {
                count: Some(next_count_after_java_get_move(runtime(entity).count)),
                protocol_seeded: Some(true),
            },
        }]
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity, state.meta.ascension_level, entity.planned_move_id())
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (GLARE, [MoveStep::ApplyPower(power)]) => vec![apply_power_action(entity, power)],
            (COUNT | IT_IS_TIME, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (move_id, steps) => panic!("giant head plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
