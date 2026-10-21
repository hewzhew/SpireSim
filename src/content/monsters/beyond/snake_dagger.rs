use crate::content::cards::CardId;
use crate::content::monsters::exordium::{add_card_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity, SnakeDaggerRuntimeState};
use crate::semantics::combat::{
    AddCardStep, AttackSpec, CardDestination, DamageKind, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep,
};
use smallvec::smallvec;

pub struct SnakeDagger;

const WOUND: u8 = 1;
const EXPLODE: u8 = 2;
const EXPLODE_DAMAGE: i32 = 25;

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.snake_dagger = SnakeDaggerRuntimeState {
        protocol_seeded: true,
        first_move: true,
    };
}

fn first_move(entity: &MonsterEntity) -> bool {
    assert!(
        entity.snake_dagger.protocol_seeded,
        "snake dagger runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.snake_dagger.first_move
}

fn snake_dagger_runtime_update(entity: &MonsterEntity, first_move: bool) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::SnakeDagger {
            first_move: Some(first_move),
            protocol_seeded: Some(true),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn explode_uses_lose_hp_action_not_suicide_like_java() {
        let mut dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        dagger.current_hp = 7;
        let mut state = crate::test_support::combat_with_monsters(vec![dagger.clone()]);

        let actions = SnakeDagger::take_turn_plan(&mut state, &dagger, &explode_plan());

        assert!(
            actions.iter().any(|action| matches!(
                action,
                Action::LoseHp {
                    target: 1,
                    amount: 7,
                    ..
                }
            )),
            "Java SnakeDagger explode queues LoseHPAction(this, this, currentHealth)"
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Suicide { target: 1, .. })),
            "SuicideAction bypasses the Java monster damage/death pipeline"
        );
    }

    #[test]
    fn first_move_uses_runtime_flag_not_empty_history() {
        let mut dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        dagger.snake_dagger.first_move = false;
        dagger.move_history_mut().clear();

        let plan = SnakeDagger::roll_move_plan(&mut StsRng::new(0), &dagger, 0, 0);

        assert_eq!(
            plan.move_id, EXPLODE,
            "Java SnakeDagger.firstMove is the opening gate, not move history"
        );
    }

    #[test]
    fn opening_roll_clears_first_move_runtime_flag() {
        let dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        let plan = SnakeDagger::roll_move_plan(&mut StsRng::new(0), &dagger, 0, 0);

        let actions = SnakeDagger::on_roll_move(0, &dagger, 0, &plan);

        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::SnakeDagger {
                    first_move: Some(false),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }
}

fn wound_plan() -> MonsterTurnPlan {
    let wound = AddCardStep {
        card_id: CardId::Wound,
        amount: 1,
        upgraded: false,
        destination: CardDestination::Discard,
        visible_strength: EffectStrength::Normal,
    };
    MonsterTurnPlan::with_visible_spec(
        WOUND,
        smallvec![
            MoveStep::Attack(crate::semantics::combat::AttackStep {
                target: crate::semantics::combat::MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: 9,
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::AddCard(wound.clone()),
        ],
        MonsterMoveSpec::AttackAddCard(
            AttackSpec {
                base_damage: 9,
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            wound,
        ),
    )
}

fn explode_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        EXPLODE,
        smallvec![
            MoveStep::Attack(crate::semantics::combat::AttackStep {
                target: crate::semantics::combat::MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: EXPLODE_DAMAGE,
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::Suicide,
        ],
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: EXPLODE_DAMAGE,
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn plan_for(move_id: u8) -> MonsterTurnPlan {
    match move_id {
        WOUND => wound_plan(),
        EXPLODE => explode_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics() -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![("Wound", wound_plan())],
        special_moves: vec![("Explode", format!("Attack player for {EXPLODE_DAMAGE} base damage/hit (hits: 1), then lose own current HP through the normal damage/death pipeline."))],
        notes: "On its first enemy turn, attacks and adds a Wound to the player's discard pile. On its next turn, attacks for 25 base damage and then loses its own current HP. A newly summoned dagger starts with the same sequence. Minions die when Reptomancer dies.",
    }
}

impl MonsterBehavior for SnakeDagger {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        _ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if first_move(entity) {
            wound_plan()
        } else {
            explode_plan()
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if first_move(entity) {
            vec![snake_dagger_runtime_update(entity, false)]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(_state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id())
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (WOUND, [MoveStep::Attack(attack), MoveStep::AddCard(wound)]) => {
                let mut actions = attack_actions(entity.id, PLAYER, &attack.attack);
                actions.push(add_card_action(wound));
                actions
            }
            (EXPLODE, [MoveStep::Attack(attack), MoveStep::Suicide]) => {
                let mut actions = attack_actions(entity.id, PLAYER, &attack.attack);
                actions.push(Action::LoseHp {
                    target: entity.id,
                    amount: entity.current_hp,
                    triggers_rupture: false,
                });
                actions
            }
            (move_id, steps) => panic!("snake dagger plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
