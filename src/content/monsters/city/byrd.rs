use crate::content::monsters::exordium::{
    apply_power_action, attack_actions, set_next_move_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, BuffSpec, DamageKind, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};

pub struct Byrd;

const PECK: u8 = 1;
const GO_AIRBORNE: u8 = 2;
const SWOOP: u8 = 3;
const STUNNED: u8 = 4;
const HEADBUTT: u8 = 5;
const CAW: u8 = 6;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn first_roll_marks_byrd_first_move_false_like_java_get_move() {
        let mut state = crate::test_support::blank_test_combat();
        let byrd = crate::test_support::test_monster(EnemyId::Byrd);
        state.entities.monsters = vec![byrd];

        crate::engine::action_handlers::execute_action(
            Action::RollMonsterMove { monster_id: 1 },
            &mut state,
        );

        let byrd = &state.entities.monsters[0];
        assert!(
            byrd.planned_move_id() == PECK || byrd.planned_move_id() == CAW,
            "Java Byrd.getMove rolls the opening CAW/PECK move"
        );
        assert!(
            !byrd.byrd.first_move,
            "Java Byrd.getMove clears firstMove while rolling the opening move, before takeTurn executes"
        );
    }

    #[test]
    fn headbutt_sets_airborne_before_queued_damage_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let byrd = crate::test_support::test_monster(EnemyId::Byrd);

        let actions = Byrd::take_turn_plan(&mut state, &byrd, &headbutt_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::SetMonsterMove {
                    next_move_byte: GO_AIRBORNE,
                    planned_visible_spec: Some(MonsterMoveSpec::Unknown),
                    ..
                },
                Action::MonsterAttack { base_damage: 3, .. }
            ]
        ));
    }

    #[test]
    fn airborne_restores_flight_but_does_not_reveal_a_buff_intent() {
        let plan = go_airborne_plan(20);
        assert_eq!(plan.visible_spec, Some(MonsterMoveSpec::Unknown));
        assert!(matches!(
            decode_turn(&plan),
            ByrdTurn::GoAirborne(ApplyPowerStep {
                power_id: PowerId::Flight,
                amount: 4,
                ..
            })
        ));
    }
}

enum ByrdTurn<'a> {
    Peck(&'a AttackSpec),
    GoAirborne(&'a ApplyPowerStep),
    Swoop(&'a AttackSpec),
    Stunned,
    Headbutt(&'a AttackSpec),
    Caw(&'a ApplyPowerStep),
}

fn peck_count(ascension_level: u8) -> u8 {
    if ascension_level >= 2 {
        6
    } else {
        5
    }
}

fn swoop_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        14
    } else {
        12
    }
}

fn flight_amount(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        4
    } else {
        3
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("byrd.peck"), peck_plan(asc)),
            (Text::new("byrd.swoop"), swoop_plan(asc)),
            (Text::new("byrd.caw"), caw_plan()),
            (Text::new("byrd.stunned"), stunned_plan()),
            (Text::new("byrd.headbutt"), headbutt_plan()),
            (Text::new("byrd.goAirborne"), go_airborne_plan(asc)),
        ],
        special_moves: vec![(
            Text::new("byrd.startingFlight"),
            Text::new("byrd.flightRule").with("amount", flight_amount(asc)),
        )],
        notes: Text::new("byrd.notes"),
    }
}

fn peck_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        PECK,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 1,
            hits: peck_count(ascension_level),
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn go_airborne_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        GO_AIRBORNE,
        smallvec::smallvec![MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Flight,
            amount: flight_amount(ascension_level),
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        })],
        // Java HEADBUTT sets GO_AIRBORNE with Intent.UNKNOWN. Its actual
        // Flight application remains locked execution truth behind that icon.
        MonsterMoveSpec::Unknown,
    )
}

fn swoop_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SWOOP,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: swoop_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn stunned_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(STUNNED, MoveStep::Stun)
}

fn headbutt_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        HEADBUTT,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 3,
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn caw_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        CAW,
        MonsterMoveSpec::Buff(BuffSpec {
            power_id: PowerId::Strength,
            amount: 1,
        }),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        PECK => peck_plan(ascension_level),
        GO_AIRBORNE => go_airborne_plan(ascension_level),
        SWOOP => swoop_plan(ascension_level),
        STUNNED => stunned_plan(),
        HEADBUTT => headbutt_plan(),
        CAW => caw_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> ByrdTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            PECK,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => ByrdTurn::Peck(attack),
        (
            GO_AIRBORNE,
            [MoveStep::ApplyPower(
                power @ ApplyPowerStep {
                    target: MoveTarget::SelfTarget,
                    power_id: PowerId::Flight,
                    effect: PowerEffectKind::Buff,
                    ..
                },
            )],
        ) => ByrdTurn::GoAirborne(power),
        (
            SWOOP,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => ByrdTurn::Swoop(attack),
        (STUNNED, [MoveStep::Stun]) => ByrdTurn::Stunned,
        (
            HEADBUTT,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => ByrdTurn::Headbutt(attack),
        (
            CAW,
            [MoveStep::ApplyPower(
                power @ ApplyPowerStep {
                    target: MoveTarget::SelfTarget,
                    power_id: PowerId::Strength,
                    effect: PowerEffectKind::Buff,
                    ..
                },
            )],
        ) => ByrdTurn::Caw(power),
        (_, []) => panic!("byrd plan missing locked truth"),
        (move_id, steps) => panic!("byrd plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

fn current_runtime_flags(entity: &MonsterEntity) -> (bool, bool) {
    assert!(
        entity.byrd.protocol_seeded,
        "byrd runtime truth must be protocol-seeded or factory-seeded"
    );
    (entity.byrd.first_move, entity.byrd.is_flying)
}

fn byrd_runtime_update(
    entity: &MonsterEntity,
    first_move: Option<bool>,
    is_flying: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Byrd {
            first_move,
            is_flying,
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for Byrd {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        let flight_amt = flight_amount(ascension_level);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::Flight,
            amount: flight_amt,
        }]
    }

    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        let (first_move, is_flying) = current_runtime_flags(entity);

        if first_move {
            if rng.random_boolean_chance(0.375) {
                return caw_plan();
            } else {
                return peck_plan(ascension_level);
            }
        }

        if is_flying {
            if num < 50 {
                let mut rev = entity.move_history().iter().rev();
                if rev.next() == Some(&PECK) && rev.next() == Some(&PECK) {
                    if rng.random_boolean_chance(0.4) {
                        swoop_plan(ascension_level)
                    } else {
                        caw_plan()
                    }
                } else {
                    peck_plan(ascension_level)
                }
            } else if num < 70 {
                if entity.move_history().back() == Some(&SWOOP) {
                    if rng.random_boolean_chance(0.375) {
                        caw_plan()
                    } else {
                        peck_plan(ascension_level)
                    }
                } else {
                    swoop_plan(ascension_level)
                }
            } else if entity.move_history().back() == Some(&CAW) {
                if rng.random_boolean_chance(0.2857) {
                    swoop_plan(ascension_level)
                } else {
                    peck_plan(ascension_level)
                }
            } else {
                caw_plan()
            }
        } else {
            headbutt_plan()
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if entity.byrd.first_move {
            vec![byrd_runtime_update(entity, Some(false), None)]
        } else {
            Vec::new()
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
        match decode_turn(plan) {
            ByrdTurn::Peck(attack) | ByrdTurn::Swoop(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
            ByrdTurn::GoAirborne(power) => {
                let mut actions = vec![
                    byrd_runtime_update(entity, None, Some(true)),
                    apply_power_action(entity, power),
                ];
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
            ByrdTurn::Caw(power) => {
                let mut actions = vec![apply_power_action(entity, power)];
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
            ByrdTurn::Stunned => vec![Action::RollMonsterMove {
                monster_id: entity.id,
            }],
            ByrdTurn::Headbutt(attack) => {
                let mut actions = vec![set_next_move_action(
                    entity,
                    go_airborne_plan(state.meta.ascension_level),
                )];
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions
            }
        }
    }
}
