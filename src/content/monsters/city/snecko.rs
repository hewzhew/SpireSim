use crate::content::monsters::exordium::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct Snecko;

const GLARE: u8 = 1;
const BITE: u8 = 2;
const TAIL: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn first_roll_marks_snecko_first_turn_false_like_java_get_move() {
        let mut state = crate::test_support::blank_test_combat();
        let snecko = crate::test_support::test_monster(EnemyId::Snecko);
        state.entities.monsters = vec![snecko];

        crate::engine::action_handlers::execute_action(
            Action::RollMonsterMove { monster_id: 1 },
            &mut state,
        );

        let snecko = &state.entities.monsters[0];
        assert_eq!(snecko.planned_move_id(), GLARE);
        assert!(
            !snecko.snecko.first_turn,
            "Java Snecko.getMove clears firstTurn while rolling the opening Glare move"
        );
    }

    #[test]
    fn glare_applies_confusion_sentinel_amount_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let snecko = crate::test_support::test_monster(EnemyId::Snecko);

        let actions = Snecko::take_turn_plan(&mut state, &snecko, &glare_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Confusion,
                    amount: -1
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn a17_tail_queues_weak_before_vulnerable_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let snecko = crate::test_support::test_monster(EnemyId::Snecko);

        let actions = Snecko::take_turn_plan(&mut state, &snecko, &tail_plan(17));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 10,
                    ..
                },
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Weak,
                    amount: 2
                },
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Vulnerable,
                    amount: 2
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn two_bites_force_tail_like_java_last_two_moves_rule() {
        let mut snecko = crate::test_support::test_monster(EnemyId::Snecko);
        snecko.snecko.first_turn = false;
        snecko.move_history_mut().push_back(BITE);
        snecko.move_history_mut().push_back(BITE);

        let plan = Snecko::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &snecko, 0, 90);

        assert_eq!(plan.move_id, TAIL);
    }
}

enum SneckoTurn<'a> {
    Glare(&'a ApplyPowerStep),
    Bite(&'a AttackSpec),
    Tail {
        attack: &'a AttackSpec,
        weak: Option<&'a ApplyPowerStep>,
        vulnerable: &'a ApplyPowerStep,
    },
}

fn bite_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        18
    } else {
        15
    }
}

fn tail_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        10
    } else {
        8
    }
}

fn glare_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        GLARE,
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Confusion,
            amount: -1,
            strength: EffectStrength::Strong,
        }),
    )
}

fn bite_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BITE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: bite_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn tail_plan(ascension_level: u8) -> MonsterTurnPlan {
    let attack = AttackSpec {
        base_damage: tail_damage(ascension_level),
        hits: 1,
        damage_kind: DamageKind::Normal,
    };
    let vulnerable = ApplyPowerStep {
        target: MoveTarget::Player,
        power_id: PowerId::Vulnerable,
        amount: 2,
        effect: PowerEffectKind::Debuff,
        visible_strength: EffectStrength::Normal,
    };

    if ascension_level >= 17 {
        MonsterTurnPlan::with_visible_spec(
            TAIL,
            smallvec![
                MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack: attack.clone(),
                }),
                MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Weak,
                    amount: 2,
                    effect: PowerEffectKind::Debuff,
                    visible_strength: EffectStrength::Normal,
                }),
                MoveStep::ApplyPower(vulnerable.clone()),
            ],
            MonsterMoveSpec::AttackDebuff(
                attack,
                DebuffSpec {
                    power_id: PowerId::Vulnerable,
                    amount: 2,
                    strength: EffectStrength::Normal,
                },
            ),
        )
    } else {
        MonsterTurnPlan::with_visible_spec(
            TAIL,
            smallvec![
                MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack: attack.clone(),
                }),
                MoveStep::ApplyPower(vulnerable.clone()),
            ],
            MonsterMoveSpec::AttackDebuff(
                attack,
                DebuffSpec {
                    power_id: PowerId::Vulnerable,
                    amount: 2,
                    strength: EffectStrength::Normal,
                },
            ),
        )
    }
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        GLARE => glare_plan(),
        BITE => bite_plan(ascension_level),
        TAIL => tail_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(ascension_level: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("snecko.perplexingGlare"), glare_plan()),
            (Text::new("snecko.bite"), bite_plan(ascension_level)),
            (Text::new("snecko.tailWhip"), tail_plan(ascension_level)),
        ],
        special_moves: vec![],
        notes: Text::new("snecko.notes"),
    }
}

fn current_runtime_flags(entity: &MonsterEntity) -> bool {
    assert!(
        entity.snecko.protocol_seeded,
        "snecko runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.snecko.first_turn
}

fn snecko_runtime_update(entity: &MonsterEntity, first_turn: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Snecko {
            first_turn,
            protocol_seeded: Some(true),
        },
    }
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let history = entity.move_history();
    history.len() >= 2
        && history[history.len() - 1] == move_id
        && history[history.len() - 2] == move_id
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> SneckoTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            GLARE,
            [MoveStep::ApplyPower(
                power @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Confusion,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            )],
        ) => SneckoTurn::Glare(power),
        (
            BITE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => SneckoTurn::Bite(attack),
        (
            TAIL,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(
                vulnerable @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Vulnerable,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            )],
        ) => SneckoTurn::Tail {
            attack,
            weak: None,
            vulnerable,
        },
        (
            TAIL,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(
                weak @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Weak,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            ), MoveStep::ApplyPower(
                vulnerable @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Vulnerable,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            )],
        ) => SneckoTurn::Tail {
            attack,
            weak: Some(weak),
            vulnerable,
        },
        (_, []) => panic!("snecko plan missing locked truth"),
        (move_id, steps) => panic!("snecko plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for Snecko {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if current_runtime_flags(entity) {
            return glare_plan();
        }

        if num < 40 || last_two_moves(entity, BITE) {
            tail_plan(ascension_level)
        } else {
            bite_plan(ascension_level)
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if current_runtime_flags(entity) {
            vec![snecko_runtime_update(entity, Some(false))]
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
            SneckoTurn::Glare(power) => vec![apply_power_action(entity, power)],
            SneckoTurn::Bite(attack) => attack_actions(entity.id, PLAYER, attack),
            SneckoTurn::Tail {
                attack,
                weak,
                vulnerable,
            } => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                if let Some(weak) = weak {
                    actions.push(apply_power_action(entity, weak));
                }
                actions.push(apply_power_action(entity, vulnerable));
                actions
            }
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
