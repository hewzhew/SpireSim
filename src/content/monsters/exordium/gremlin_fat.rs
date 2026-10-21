use super::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

const BLUNT: u8 = 2;
const ESCAPE: u8 = 99;

pub struct GremlinFat;

enum GremlinFatTurn<'a> {
    Blunt(
        &'a AttackSpec,
        &'a ApplyPowerStep,
        Option<&'a ApplyPowerStep>,
    ),
    Escape,
}

fn blunt_damage(asc: u8) -> i32 {
    if asc >= 2 {
        5
    } else {
        4
    }
}

fn blunt_plan(asc: u8) -> MonsterTurnPlan {
    let mut steps = smallvec![
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: blunt_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Weak,
            amount: 1,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }),
    ];
    if asc >= 17 {
        steps.push(MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Frail,
            amount: 1,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }));
    }
    MonsterTurnPlan::with_visible_spec(
        BLUNT,
        steps,
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: blunt_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Weak,
                amount: 1,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn escape_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(ESCAPE, MoveStep::Escape)
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        BLUNT => blunt_plan(asc),
        ESCAPE => escape_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![("Smash", blunt_plan(asc)), ("Escape", escape_plan())],
        special_moves: vec![],
        notes: "Normally repeats Smash. Damage happens before Weak and, at A17+, Frail. Escape is a separately assigned move, not part of the normal attack cycle.",
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> GremlinFatTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            BLUNT,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                effect: PowerEffectKind::Debuff,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(weak) = &plan.steps[1] else {
                unreachable!()
            };
            GremlinFatTurn::Blunt(attack, weak, None)
        }
        (
            BLUNT,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                effect: PowerEffectKind::Debuff,
                ..
            }), MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Frail,
                effect: PowerEffectKind::Debuff,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(weak) = &plan.steps[1] else {
                unreachable!()
            };
            let MoveStep::ApplyPower(frail) = &plan.steps[2] else {
                unreachable!()
            };
            GremlinFatTurn::Blunt(attack, weak, Some(frail))
        }
        (ESCAPE, [MoveStep::Escape]) => GremlinFatTurn::Escape,
        (_, []) => panic!("gremlin fat plan missing locked truth"),
        (move_id, steps) => panic!("gremlin fat plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for GremlinFat {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        asc: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        blunt_plan(asc)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match decode_turn(plan) {
            GremlinFatTurn::Blunt(attack, weak, frail) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(apply_power_action(entity, weak));
                if let Some(frail) = frail {
                    actions.push(apply_power_action(entity, frail));
                }
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
            GremlinFatTurn::Escape => vec![
                Action::Escape { target: entity.id },
                super::set_next_move_action(entity, escape_plan()),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{blunt_plan, GremlinFat};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;

    #[test]
    fn blunt_rerolls_like_java_even_when_move_is_deterministic() {
        // GremlinFat.java queues RollMoveAction after BLUNT when not escaping.
        // That consumes aiRng.random(99); replacing it with SetMonsterMove shifts
        // later monster RNG even though getMove always chooses BLUNT.
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::GremlinFat);
        let actions = GremlinFat::take_turn_plan(&mut state, &entity, &blunt_plan(0));

        assert!(matches!(
            actions.last(),
            Some(Action::RollMonsterMove { monster_id: 1 })
        ));
    }

    #[test]
    fn escape_turn_queues_escape_intent_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::GremlinFat);
        let actions = GremlinFat::take_turn_plan(&mut state, &entity, &super::escape_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::Escape { .. },
                Action::SetMonsterMove {
                    next_move_byte: super::ESCAPE,
                    ..
                }
            ]
        ));
    }

    #[test]
    fn blunt_action_order_and_a17_frail_match_java() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::GremlinFat);

        let a16 = GremlinFat::take_turn_plan(&mut state, &entity, &blunt_plan(16));
        assert!(matches!(
            a16.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 5,
                    ..
                },
                Action::ApplyPower {
                    source: 1,
                    target: 0,
                    power_id: PowerId::Weak,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));

        let a17 = GremlinFat::take_turn_plan(&mut state, &entity, &blunt_plan(17));
        assert!(matches!(
            a17.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 5,
                    ..
                },
                Action::ApplyPower {
                    source: 1,
                    target: 0,
                    power_id: PowerId::Weak,
                    amount: 1,
                },
                Action::ApplyPower {
                    source: 1,
                    target: 0,
                    power_id: PowerId::Frail,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }
}
