use crate::content::cards::CardId;
use crate::content::monsters::exordium::{add_card_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity, NemesisRuntimeState};
use crate::semantics::combat::{
    AddCardStep, AttackSpec, CardDestination, DamageKind, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep,
};

pub struct Nemesis;

const TRI_ATTACK: u8 = 2;
const SCYTHE: u8 = 3;
const TRI_BURN: u8 = 4;

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("nemesis.triAttack"), tri_attack_plan(asc)),
            (Text::new("nemesis.scythe"), scythe_plan()),
            (Text::new("nemesis.triBurn"), tri_burn_plan(asc)),
        ],
        special_moves: vec![(
            Text::new("nemesis.intangibleAfterMove"),
            Text::new("nemesis.intangibleRule"),
        )],
        notes: Text::new("nemesis.notes"),
    }
}

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.nemesis = NemesisRuntimeState {
        protocol_seeded: true,
        first_move: true,
        scythe_cooldown: 0,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::store;
    use crate::runtime::combat::{Power, PowerPayload};
    use crate::runtime::rng::StsRng;

    fn power(power_type: PowerId, amount: i32) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    #[test]
    fn first_roll_clears_private_first_move_and_pre_decrements_cooldown() {
        let mut state = crate::testing::support::blank_test_combat();
        state.entities.monsters = vec![crate::testing::support::test_monster(EnemyId::Nemesis)];

        crate::engine::action_handlers::execute_action(
            Action::RollMonsterMove { monster_id: 1 },
            &mut state,
        );

        let nemesis = &state.entities.monsters[0];
        assert!(
            !nemesis.nemesis.first_move,
            "Java Nemesis.getMove clears private firstMove while rolling the opening move"
        );
        assert_eq!(
            nemesis.nemesis.scythe_cooldown, -1,
            "Java decrements scytheCooldown before even the opening branch"
        );
    }

    #[test]
    fn imported_first_move_false_with_empty_history_does_not_force_opening() {
        let mut nemesis = crate::testing::support::test_monster(EnemyId::Nemesis);
        nemesis.nemesis.first_move = false;
        nemesis.nemesis.scythe_cooldown = 0;

        let plan = Nemesis::roll_move_plan(&mut StsRng::new(0), &nemesis, 0, 0);

        assert_eq!(
            plan.move_id, SCYTHE,
            "Java gates the opening branch on private firstMove, not empty move history"
        );
    }

    #[test]
    fn scythe_roll_resets_private_cooldown_to_two() {
        let mut nemesis = crate::testing::support::test_monster(EnemyId::Nemesis);
        nemesis.nemesis.first_move = false;
        nemesis.nemesis.scythe_cooldown = 0;
        let plan = scythe_plan();

        let actions = Nemesis::on_roll_move(0, &nemesis, 0, &plan);

        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::Nemesis {
                    first_move: None,
                    scythe_cooldown: Some(2),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }

    #[test]
    fn non_scythe_roll_keeps_java_pre_decremented_cooldown() {
        let mut nemesis = crate::testing::support::test_monster(EnemyId::Nemesis);
        nemesis.nemesis.first_move = false;
        nemesis.nemesis.scythe_cooldown = 2;
        let plan = tri_attack_plan(0);

        let actions = Nemesis::on_roll_move(0, &nemesis, 50, &plan);

        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::Nemesis {
                    first_move: None,
                    scythe_cooldown: Some(1),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }

    #[test]
    fn tri_attack_queues_three_hits_intangible_then_roll_like_java() {
        let nemesis = crate::testing::support::test_monster(EnemyId::Nemesis);
        let mut state = crate::testing::support::combat_with_monsters(vec![nemesis.clone()]);

        let actions = Nemesis::take_turn_plan(&mut state, &nemesis, &tri_attack_plan(3));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 7,
                    ..
                },
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 7,
                    ..
                },
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 7,
                    ..
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Intangible,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }

    #[test]
    fn tri_burn_uses_a18_burn_count_before_intangible_and_roll() {
        let nemesis = crate::testing::support::test_monster(EnemyId::Nemesis);
        let mut state = crate::testing::support::combat_with_monsters(vec![nemesis.clone()]);

        let actions = Nemesis::take_turn_plan(&mut state, &nemesis, &tri_burn_plan(18));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MakeTempCardInDiscard {
                    card_id: CardId::Burn,
                    amount: 5,
                    upgraded: false,
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Intangible,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }

    #[test]
    fn take_turn_skips_new_intangible_when_already_present_like_java_has_power_guard() {
        let nemesis = crate::testing::support::test_monster(EnemyId::Nemesis);
        let mut state = crate::testing::support::combat_with_monsters(vec![nemesis.clone()]);
        store::set_powers_for(&mut state, nemesis.id, vec![power(PowerId::Intangible, 1)]);

        let actions = Nemesis::take_turn_plan(&mut state, &nemesis, &scythe_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 45,
                    ..
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }
}

fn scythe_damage() -> i32 {
    45
}

fn fire_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 3 {
        7
    } else {
        6
    }
}

fn burn_amount(ascension_level: u8) -> i32 {
    if ascension_level >= 18 {
        5
    } else {
        3
    }
}

fn tri_attack_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        TRI_ATTACK,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: fire_damage(ascension_level),
            hits: 3,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn scythe_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SCYTHE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: scythe_damage(),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn tri_burn_plan(ascension_level: u8) -> MonsterTurnPlan {
    let burn = AddCardStep {
        card_id: CardId::Burn,
        amount: burn_amount(ascension_level) as u8,
        upgraded: false,
        destination: CardDestination::Discard,
        visible_strength: EffectStrength::Normal,
    };
    MonsterTurnPlan::from_spec(TRI_BURN, MonsterMoveSpec::AddCard(burn))
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        TRI_ATTACK => tri_attack_plan(ascension_level),
        SCYTHE => scythe_plan(),
        TRI_BURN => tri_burn_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let history = entity.move_history();
    history.len() >= 2
        && history[history.len() - 1] == move_id
        && history[history.len() - 2] == move_id
}

fn runtime(entity: &MonsterEntity) -> &NemesisRuntimeState {
    assert!(
        entity.nemesis.protocol_seeded,
        "nemesis runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.nemesis
}

fn scythe_cooldown_after_java_pre_decrement(entity: &MonsterEntity) -> i32 {
    runtime(entity).scythe_cooldown - 1
}

fn nemesis_runtime_update(
    entity: &MonsterEntity,
    first_move: Option<bool>,
    scythe_cooldown: Option<i32>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Nemesis {
            first_move,
            scythe_cooldown,
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for Nemesis {
    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        let runtime = runtime(entity);
        let scythe_cooldown = scythe_cooldown_after_java_pre_decrement(entity);

        if runtime.first_move {
            return if num < 50 {
                tri_attack_plan(ascension_level)
            } else {
                tri_burn_plan(ascension_level)
            };
        }

        if num < 30 {
            if !last_move(entity, SCYTHE) && scythe_cooldown <= 0 {
                scythe_plan()
            } else if rng.random_boolean() {
                if !last_two_moves(entity, TRI_ATTACK) {
                    tri_attack_plan(ascension_level)
                } else {
                    tri_burn_plan(ascension_level)
                }
            } else if !last_move(entity, TRI_BURN) {
                tri_burn_plan(ascension_level)
            } else {
                tri_attack_plan(ascension_level)
            }
        } else if num < 65 {
            if !last_two_moves(entity, TRI_ATTACK) {
                tri_attack_plan(ascension_level)
            } else if rng.random_boolean() {
                if scythe_cooldown > 0 {
                    tri_burn_plan(ascension_level)
                } else {
                    scythe_plan()
                }
            } else {
                tri_burn_plan(ascension_level)
            }
        } else if !last_move(entity, TRI_BURN) {
            tri_burn_plan(ascension_level)
        } else if rng.random_boolean() && scythe_cooldown <= 0 {
            scythe_plan()
        } else {
            tri_attack_plan(ascension_level)
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let runtime = runtime(entity);
        let first_move = runtime.first_move.then_some(false);
        let scythe_cooldown = if plan.move_id == SCYTHE {
            2
        } else {
            runtime.scythe_cooldown - 1
        };
        vec![nemesis_runtime_update(
            entity,
            first_move,
            Some(scythe_cooldown),
        )]
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
            (SCYTHE | TRI_ATTACK, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (TRI_BURN, [MoveStep::AddCard(add_card)]) => vec![add_card_action(add_card)],
            (move_id, steps) => panic!("nemesis plan/steps mismatch: {} {:?}", move_id, steps),
        };

        if crate::content::powers::store::power_amount(state, entity.id, PowerId::Intangible) <= 0 {
            actions.push(Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Intangible,
                amount: 1,
            });
        }
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
