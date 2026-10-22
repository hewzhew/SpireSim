use crate::content::monsters::exordium::{attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, ExploderRuntimeState, MonsterEntity};
use crate::semantics::combat::{AttackSpec, DamageKind, MonsterMoveSpec, MonsterTurnPlan};

pub struct Exploder;

const ATTACK: u8 = 1;
const BLOCK: u8 = 2;
const STARTING_EXPLOSIVE: i32 = 3;

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![(Text::new("exploder.attack"), attack_plan(asc))],
        special_moves: vec![(
            Text::new("exploder.explosion"),
            Text::new("exploder.explosionRule")
                .with("count", STARTING_EXPLOSIVE)
                .with(
                    "damage",
                    crate::content::powers::core::explosive::EXPLOSION_DAMAGE,
                ),
        )],
        notes: Text::new("exploder.notes"),
    }
}

fn attack_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        11
    } else {
        9
    }
}

fn attack_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        ATTACK,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: attack_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn block_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::unknown(BLOCK)
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        ATTACK => attack_plan(ascension_level),
        BLOCK => block_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.exploder = ExploderRuntimeState {
        protocol_seeded: true,
        turn_count: 0,
    };
}

fn runtime(entity: &MonsterEntity) -> &ExploderRuntimeState {
    assert!(
        entity.exploder.protocol_seeded,
        "exploder runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.exploder
}

fn exploder_runtime_update(entity: &MonsterEntity, turn_count: i32) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Exploder {
            turn_count: Some(turn_count),
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for Exploder {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::Explosive,
            amount: STARTING_EXPLOSIVE,
        }]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if runtime(entity).turn_count < 2 {
            attack_plan(ascension_level)
        } else {
            block_plan()
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
        let mut actions = vec![exploder_runtime_update(
            entity,
            runtime(entity).turn_count + 1,
        )];
        match (plan.move_id, plan.attack()) {
            (ATTACK, Some(attack)) => {
                actions.extend(attack_actions(entity.id, PLAYER, attack));
            }
            (BLOCK, None) => {}
            (move_id, _) => panic!("exploder plan/steps mismatch: {} {:?}", move_id, plan.steps),
        }
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn imported_turn_count_not_history_length_drives_explode_intent() {
        let mut exploder = crate::test_support::test_monster(EnemyId::Exploder);
        exploder.exploder.turn_count = 2;
        exploder.move_history_mut().clear();

        let plan = Exploder::roll_move_plan(&mut StsRng::new(0), &exploder, 0, 0);

        assert_eq!(plan.move_id, BLOCK);
        assert!(plan.attack().is_none());
    }

    #[test]
    fn take_turn_increments_java_turn_count_before_queued_damage() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::Exploder)];
        let mut exploder = state.entities.monsters[0].clone();
        exploder.exploder.turn_count = 1;
        let plan = attack_plan(0);

        let actions = Exploder::take_turn_plan(&mut state, &exploder, &plan);

        assert!(matches!(
            actions.as_slice(),
            [
                Action::UpdateMonsterRuntime {
                    patch: MonsterRuntimePatch::Exploder {
                        turn_count: Some(2),
                        protocol_seeded: Some(true),
                    },
                    ..
                },
                Action::MonsterAttack { .. },
                Action::RollMonsterMove { .. },
            ]
        ));
    }

    #[test]
    fn pre_battle_applies_explosive_three_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let mut exploder = crate::test_support::test_monster(EnemyId::Exploder);
        exploder.id = 55;

        let actions = Exploder::use_pre_battle_actions(
            &mut state,
            &exploder,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );

        assert_eq!(
            actions,
            vec![Action::ApplyPower {
                source: 55,
                target: 55,
                power_id: PowerId::Explosive,
                amount: 3,
            }]
        );
    }

    #[test]
    fn block_turn_still_increments_turn_count_before_roll() {
        let mut state = crate::test_support::blank_test_combat();
        let mut exploder = crate::test_support::test_monster(EnemyId::Exploder);
        exploder.id = 55;
        exploder.exploder.turn_count = 2;

        let actions = Exploder::take_turn_plan(&mut state, &exploder, &block_plan());

        assert_eq!(
            actions,
            vec![
                Action::UpdateMonsterRuntime {
                    monster_id: 55,
                    patch: MonsterRuntimePatch::Exploder {
                        turn_count: Some(3),
                        protocol_seeded: Some(true),
                    },
                },
                Action::RollMonsterMove { monster_id: 55 },
            ],
            "Java increments turnCount before the switch even when the UNKNOWN move has no queued body action"
        );
    }
}
