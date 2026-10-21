use crate::content::monsters::exordium::{
    apply_power_action, attack_actions, set_next_move_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, DamageInfo, DamageType, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct ShelledParasite;

const FELL: u8 = 1;
const DOUBLE_STRIKE: u8 = 2;
const LIFE_SUCK: u8 = 3;
const STUNNED: u8 = 4;
const STARTING_PLATED_ARMOR: i32 = 14;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn life_suck_keeps_attack_buff_intent_and_heals_for_damage_dealt() {
        use crate::engine::action_handlers::execute_action;
        use crate::runtime::combat::Intent;
        for (block, damage) in [(0, 12), (5, 7), (20, 0)] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = 20;
            state.entities.player.block = block;
            let mut parasite = crate::test_support::test_monster(EnemyId::ShelledParasite);
            parasite.max_hp = 50;
            parasite.current_hp = 20;
            parasite.set_planned_move_id(LIFE_SUCK);
            let plan = ShelledParasite::turn_plan(&state, &parasite);
            assert_eq!(
                Intent::from_visible_move_spec(plan.visible_spec.as_ref().unwrap()),
                Intent::AttackBuff {
                    damage: 12,
                    hits: 1
                }
            );
            let actions = ShelledParasite::take_turn_plan(&mut state, &parasite, &plan);
            assert!(matches!(actions[0], Action::VampireDamage(_)));
            state.entities.monsters.push(parasite);
            execute_action(actions[0].clone(), &mut state);
            while let Some(action) = state.engine.action_queue.pop_front() {
                execute_action(action, &mut state);
            }
            assert_eq!(state.entities.player.current_hp, 80 - damage);
            assert_eq!(state.entities.monsters[0].current_hp, 20 + damage);
        }
    }

    #[test]
    fn first_roll_marks_shelled_parasite_first_move_false_like_java_get_move() {
        let mut state = crate::test_support::blank_test_combat();
        let parasite = crate::test_support::test_monster(EnemyId::ShelledParasite);
        state.entities.monsters = vec![parasite];

        crate::engine::action_handlers::execute_action(
            Action::RollMonsterMove { monster_id: 1 },
            &mut state,
        );

        let parasite = &state.entities.monsters[0];
        assert!(
            parasite.planned_move_id() == DOUBLE_STRIKE || parasite.planned_move_id() == LIFE_SUCK,
            "Java ShelledParasite.getMove rolls Double Strike or Life Suck on the first non-A17 move"
        );
        assert!(
            !parasite.shelled_parasite.first_move,
            "Java ShelledParasite.getMove clears firstMove while rolling the opening move"
        );
    }

    #[test]
    fn stunned_turn_sets_fell_then_still_rolls_like_java_take_turn() {
        let mut state = crate::test_support::blank_test_combat();
        let parasite = crate::test_support::test_monster(EnemyId::ShelledParasite);
        let plan = stunned_plan();

        let actions = ShelledParasite::take_turn_plan(&mut state, &parasite, &plan);

        assert!(
            matches!(
                actions.first(),
                Some(Action::SetMonsterMove {
                    next_move_byte: FELL,
                    ..
                })
            ),
            "Java ShelledParasite.takeTurn(STUNNED) calls setMove(FELL) before queuing RollMoveAction"
        );
        assert!(
            matches!(
                actions.last(),
                Some(Action::RollMonsterMove { monster_id: 1 })
            ),
            "Java ShelledParasite.takeTurn always queues RollMoveAction after the STUNNED case"
        );
    }
}

enum ShelledParasiteTurn<'a> {
    Fell(&'a AttackSpec, &'a ApplyPowerStep),
    DoubleStrike(&'a AttackSpec),
    LifeSuck(&'a AttackSpec),
    Stunned,
}

fn fell_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        21
    } else {
        18
    }
}

fn double_strike_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        7
    } else {
        6
    }
}

fn life_suck_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        12
    } else {
        10
    }
}

fn fell_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        FELL,
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: fell_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Frail,
                amount: 2,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn double_strike_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        DOUBLE_STRIKE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: double_strike_damage(ascension_level),
            hits: 2,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn life_suck_plan(ascension_level: u8) -> MonsterTurnPlan {
    let attack = AttackSpec {
        base_damage: life_suck_damage(ascension_level),
        hits: 1,
        damage_kind: DamageKind::Normal,
    };
    MonsterTurnPlan::with_visible_spec(
        LIFE_SUCK,
        smallvec![MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: attack.clone(),
        })],
        MonsterMoveSpec::AttackBuffIntent(attack),
    )
}

fn stunned_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(STUNNED, MoveStep::Stun)
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            ("Fell", fell_plan(asc)),
            ("Double Strike", double_strike_plan(asc)),
            ("Life Suck", life_suck_plan(asc)),
            ("Stunned", stunned_plan()),
        ],
        special_moves: vec![
            ("Starting protection", format!("Starts with {STARTING_PLATED_ARMOR} Plated Armor and {STARTING_PLATED_ARMOR} Block. Losing the last Plated Armor replaces the planned move with Stunned; inspect Plated Armor for its hit and turn-end triggers.")),
            ("Life Suck healing", "Heal self for the player's actual HP lost to this hit, after Block and damage prevention. The visible intent is Attack Buff.".into()),
        ],
        notes: "At A17+, the opening move is Fell; below A17 it is Double Strike or Life Suck. Fell cannot repeat consecutively; Double Strike and Life Suck can each repeat at most twice. Stunned skips its attack, then the next move is rolled normally. Possible base moves are not a prediction of a hidden intention.",
    }
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        FELL => fell_plan(ascension_level),
        DOUBLE_STRIKE => double_strike_plan(ascension_level),
        LIFE_SUCK => life_suck_plan(ascension_level),
        STUNNED => stunned_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn current_first_move(entity: &MonsterEntity) -> bool {
    assert!(
        entity.shelled_parasite.protocol_seeded,
        "shelled parasite runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.shelled_parasite.first_move
}

fn parasite_runtime_update(entity: &MonsterEntity, first_move: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::ShelledParasite {
            first_move,
            protocol_seeded: Some(true),
        },
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

fn plan_from_num(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    ascension_level: u8,
    num: i32,
) -> MonsterTurnPlan {
    if num < 20 {
        if !last_move(entity, FELL) {
            fell_plan(ascension_level)
        } else {
            let rerolled = rng.random_range(20, 99);
            plan_from_num(rng, entity, ascension_level, rerolled)
        }
    } else if num < 60 {
        if !last_two_moves(entity, DOUBLE_STRIKE) {
            double_strike_plan(ascension_level)
        } else {
            life_suck_plan(ascension_level)
        }
    } else if !last_two_moves(entity, LIFE_SUCK) {
        life_suck_plan(ascension_level)
    } else {
        double_strike_plan(ascension_level)
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> ShelledParasiteTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            FELL,
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
        ) => ShelledParasiteTurn::Fell(attack, power),
        (
            DOUBLE_STRIKE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => ShelledParasiteTurn::DoubleStrike(attack),
        (
            LIFE_SUCK,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => ShelledParasiteTurn::LifeSuck(attack),
        (STUNNED, [MoveStep::Stun]) => ShelledParasiteTurn::Stunned,
        (_, []) => panic!("shelled parasite plan missing locked truth"),
        (move_id, steps) => panic!(
            "shelled parasite plan/steps mismatch: {} {:?}",
            move_id, steps
        ),
    }
}

impl MonsterBehavior for ShelledParasite {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::PlatedArmor,
                amount: STARTING_PLATED_ARMOR,
            },
            Action::GainBlock {
                target: entity.id,
                amount: STARTING_PLATED_ARMOR,
            },
        ]
    }

    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if current_first_move(entity) {
            if ascension_level >= 17 {
                return fell_plan(ascension_level);
            }
            if rng.random_boolean() {
                return double_strike_plan(ascension_level);
            }
            return life_suck_plan(ascension_level);
        }

        plan_from_num(rng, entity, ascension_level, num)
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if current_first_move(entity) {
            vec![parasite_runtime_update(entity, Some(false))]
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
            ShelledParasiteTurn::Fell(attack, frail) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(apply_power_action(entity, frail));
                actions
            }
            ShelledParasiteTurn::DoubleStrike(attack) => attack_actions(entity.id, PLAYER, attack),
            ShelledParasiteTurn::LifeSuck(attack) => vec![Action::VampireDamage(DamageInfo {
                source: entity.id,
                target: PLAYER,
                base: attack.base_damage,
                output: attack.base_damage,
                damage_type: DamageType::Normal,
                is_modified: false,
            })],
            ShelledParasiteTurn::Stunned => vec![set_next_move_action(
                entity,
                fell_plan(_state.meta.ascension_level),
            )],
        };

        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
