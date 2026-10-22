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

pub struct Chosen;

const ZAP: u8 = 1;
const DRAIN: u8 = 2;
const DEBILITATE: u8 = 3;
const HEX: u8 = 4;
const POKE: u8 = 5;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn first_roll_marks_chosen_first_turn_false_like_java_get_move_below_a17() {
        let mut state = crate::test_support::blank_test_combat();
        let chosen = crate::test_support::test_monster(EnemyId::Chosen);
        state.entities.monsters = vec![chosen];

        crate::engine::action_handlers::execute_action(
            Action::RollMonsterMove { monster_id: 1 },
            &mut state,
        );

        let chosen = &state.entities.monsters[0];
        assert_eq!(chosen.planned_move_id(), POKE);
        assert!(
            !chosen.chosen.first_turn,
            "Java Chosen.getMove clears firstTurn while rolling the opening POKE move below A17"
        );
        assert!(
            !chosen.chosen.used_hex,
            "Java Chosen does not mark usedHex until the Hex move is rolled"
        );
    }

    #[test]
    fn a17_first_roll_marks_chosen_used_hex_like_java_get_move() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 17;
        let chosen = crate::test_support::test_monster(EnemyId::Chosen);
        state.entities.monsters = vec![chosen];

        crate::engine::action_handlers::execute_action(
            Action::RollMonsterMove { monster_id: 1 },
            &mut state,
        );

        let chosen = &state.entities.monsters[0];
        assert_eq!(chosen.planned_move_id(), HEX);
        assert!(
            chosen.chosen.used_hex,
            "Java Chosen.getMove marks usedHex while rolling the first A17 Hex move"
        );
        assert!(
            chosen.chosen.first_turn,
            "Java Chosen.getMove never touches firstTurn in the A17 branch"
        );
    }

    #[test]
    fn below_a17_second_roll_uses_hex_and_marks_used_hex_like_java() {
        let state = crate::test_support::blank_test_combat();
        let mut chosen = crate::test_support::test_monster(EnemyId::Chosen);
        chosen.chosen.first_turn = false;
        chosen.chosen.used_hex = false;
        chosen.move_history_mut().push_back(POKE);

        let plan = Chosen::roll_move_plan(
            &mut state.rng.ai_rng.clone(),
            &chosen,
            state.meta.ascension_level,
            99,
        );
        let setup = Chosen::on_roll_move(state.meta.ascension_level, &chosen, 99, &plan);

        assert_eq!(plan.move_id, HEX);
        assert!(matches!(
            setup.as_slice(),
            [Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::Chosen {
                    first_turn: None,
                    used_hex: Some(true),
                    protocol_seeded: Some(true)
                }
            }]
        ));
    }

    #[test]
    fn drain_applies_weak_then_strength_then_roll_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let chosen = crate::test_support::test_monster(EnemyId::Chosen);

        let actions = Chosen::take_turn_plan(&mut state, &chosen, &drain_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Weak,
                    amount: 3
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Strength,
                    amount: 3
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn debilitate_attacks_before_vulnerable_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let chosen = crate::test_support::test_monster(EnemyId::Chosen);

        let actions = Chosen::take_turn_plan(&mut state, &chosen, &debilitate_plan(2));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 12,
                    ..
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
    fn poke_queues_two_damage_actions_before_roll_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let chosen = crate::test_support::test_monster(EnemyId::Chosen);

        let actions = Chosen::take_turn_plan(&mut state, &chosen, &poke_plan(2));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 6,
                    ..
                },
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 6,
                    ..
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }
}

enum ChosenTurn<'a> {
    Zap(&'a AttackSpec),
    Drain(&'a ApplyPowerStep, &'a ApplyPowerStep),
    Debilitate(&'a AttackSpec, &'a ApplyPowerStep),
    Hex(&'a ApplyPowerStep),
    Poke(&'a AttackSpec),
}

fn zap_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        21
    } else {
        18
    }
}

fn debilitate_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        12
    } else {
        10
    }
}

fn poke_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        6
    } else {
        5
    }
}

fn zap_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        ZAP,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: zap_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn drain_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        DRAIN,
        smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                amount: 3,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Strength,
                amount: 3,
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            })
        ],
        MonsterMoveSpec::Debuff(DebuffSpec {
            power_id: PowerId::Weak,
            amount: 3,
            strength: EffectStrength::Normal,
        }),
    )
}

fn debilitate_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        DEBILITATE,
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: debilitate_damage(ascension_level),
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

fn hex_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        HEX,
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Hex,
            amount: 1,
            strength: EffectStrength::Strong,
        }),
    )
}

fn poke_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        POKE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: poke_damage(ascension_level),
            hits: 2,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        ZAP => zap_plan(ascension_level),
        DRAIN => drain_plan(),
        DEBILITATE => debilitate_plan(ascension_level),
        HEX => hex_plan(),
        POKE => poke_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("chosen.zap"), zap_plan(asc)),
            (Text::new("chosen.drain"), drain_plan()),
            (Text::new("chosen.debilitate"), debilitate_plan(asc)),
            (Text::new("chosen.hex"), hex_plan()),
            (Text::new("chosen.poke"), poke_plan(asc)),
        ],
        special_moves: vec![],
        notes: if asc >= 17 {
            Text::new("chosen.notesHigh")
        } else {
            Text::new("chosen.notesLow")
        },
    }
}

fn current_runtime_flags(entity: &MonsterEntity) -> (bool, bool) {
    assert!(
        entity.chosen.protocol_seeded,
        "chosen runtime truth must be protocol-seeded or factory-seeded"
    );
    (entity.chosen.first_turn, entity.chosen.used_hex)
}

fn chosen_runtime_update(
    entity: &MonsterEntity,
    first_turn: Option<bool>,
    used_hex: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Chosen {
            first_turn,
            used_hex,
            protocol_seeded: Some(true),
        },
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> ChosenTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            ZAP,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => ChosenTurn::Zap(attack),
        (
            DRAIN,
            [MoveStep::ApplyPower(
                weak @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Weak,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            ), MoveStep::ApplyPower(
                strength @ ApplyPowerStep {
                    target: MoveTarget::SelfTarget,
                    power_id: PowerId::Strength,
                    effect: PowerEffectKind::Buff,
                    ..
                },
            )],
        ) => ChosenTurn::Drain(weak, strength),
        (
            DEBILITATE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(
                power @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Vulnerable,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            )],
        ) => ChosenTurn::Debilitate(attack, power),
        (
            HEX,
            [MoveStep::ApplyPower(
                power @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Hex,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            )],
        ) => ChosenTurn::Hex(power),
        (
            POKE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => ChosenTurn::Poke(attack),
        (_, []) => panic!("chosen plan missing locked truth"),
        (move_id, steps) => panic!("chosen plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for Chosen {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        let (first_turn, used_hex) = current_runtime_flags(entity);

        if ascension_level >= 17 {
            if !used_hex {
                return hex_plan();
            }
        } else {
            if first_turn {
                return poke_plan(ascension_level);
            }
            if !used_hex {
                return hex_plan();
            }
        }

        let last_move = entity.move_history().back().copied().unwrap_or(0);
        if last_move != DEBILITATE && last_move != DRAIN {
            if num < 50 {
                return debilitate_plan(ascension_level);
            }
            return drain_plan();
        }

        if num < 40 {
            zap_plan(ascension_level)
        } else {
            poke_plan(ascension_level)
        }
    }

    fn on_roll_move(
        ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let (first_turn, used_hex) = current_runtime_flags(entity);
        let first_turn_update = if ascension_level < 17 && first_turn {
            Some(false)
        } else {
            None
        };
        let used_hex_update = if plan.move_id == HEX && !used_hex {
            Some(true)
        } else {
            None
        };

        if first_turn_update.is_some() || used_hex_update.is_some() {
            vec![chosen_runtime_update(
                entity,
                first_turn_update,
                used_hex_update,
            )]
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
            ChosenTurn::Zap(attack) | ChosenTurn::Poke(attack) => {
                attack_actions(entity.id, PLAYER, attack)
            }
            ChosenTurn::Drain(weak, strength) => {
                vec![
                    apply_power_action(entity, weak),
                    apply_power_action(entity, strength),
                ]
            }
            ChosenTurn::Debilitate(attack, power) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(apply_power_action(entity, power));
                actions
            }
            ChosenTurn::Hex(power) => vec![apply_power_action(entity, power)],
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
