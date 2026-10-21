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

const STAB: u8 = 1;
const RAKE: u8 = 4;

pub struct SlaverBlue;

enum SlaverBlueTurn<'a> {
    Stab(&'a AttackSpec),
    Rake(&'a AttackSpec, &'a ApplyPowerStep),
}

fn stab_damage(asc: u8) -> i32 {
    if asc >= 2 {
        13
    } else {
        12
    }
}

fn rake_damage(asc: u8) -> i32 {
    if asc >= 2 {
        8
    } else {
        7
    }
}

fn weak_amount(asc: u8) -> i32 {
    if asc >= 17 {
        2
    } else {
        1
    }
}

fn stab_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        STAB,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: stab_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn rake_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        RAKE,
        smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: rake_damage(asc),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                amount: weak_amount(asc),
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: rake_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Weak,
                amount: weak_amount(asc),
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        STAB => stab_plan(asc),
        RAKE => rake_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![("Stab", stab_plan(asc)), ("Rake", rake_plan(asc))],
        special_moves: vec![],
        notes: if asc >= 17 {
            "Random moves; Stab cannot occur three times consecutively and Rake cannot repeat immediately. Rake deals damage before applying Weak."
        } else {
            "Random moves; neither move can occur three times consecutively. Rake deals damage before applying Weak."
        },
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let mut history = entity.move_history().iter().rev();
    matches!(
        (history.next().copied(), history.next().copied()),
        (Some(a), Some(b)) if a == move_id && b == move_id
    )
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> SlaverBlueTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            STAB,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => SlaverBlueTurn::Stab(attack),
        (
            RAKE,
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
            SlaverBlueTurn::Rake(attack, weak)
        }
        (_, []) => panic!("slaver blue plan missing locked truth"),
        (move_id, steps) => panic!("slaver blue plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for SlaverBlue {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        asc: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if num >= 40 && !last_two_moves(entity, STAB) {
            return stab_plan(asc);
        }
        if asc >= 17 {
            if !last_move(entity, RAKE) {
                return rake_plan(asc);
            }
            return stab_plan(asc);
        }
        if !last_two_moves(entity, RAKE) {
            return rake_plan(asc);
        }
        stab_plan(asc)
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
            SlaverBlueTurn::Stab(attack) => attack_actions(entity.id, PLAYER, attack),
            SlaverBlueTurn::Rake(attack, weak) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(apply_power_action(entity, weak));
                actions
            }
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::{SlaverBlue, RAKE, STAB};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;

    fn blue_slaver_with_history(history: &[u8]) -> crate::runtime::combat::MonsterEntity {
        let mut monster = crate::testing::support::test_monster(EnemyId::SlaverBlue);
        monster.move_history_mut().extend(history.iter().copied());
        monster
    }

    #[test]
    fn blue_slaver_roll_logic_matches_java_history_branches() {
        let mut rng = crate::runtime::rng::StsRng::new(1);

        let no_history = blue_slaver_with_history(&[]);
        assert_eq!(
            SlaverBlue::roll_move_plan(&mut rng, &no_history, 0, 40).move_id,
            STAB,
            "Java picks Stab on num >= 40 unless lastTwoMoves(Stab) blocks it"
        );

        let two_stabs = blue_slaver_with_history(&[STAB, STAB]);
        assert_eq!(
            SlaverBlue::roll_move_plan(&mut rng, &two_stabs, 0, 99).move_id,
            RAKE,
            "Java falls through to Rake when high-roll Stab is blocked by lastTwoMoves(Stab)"
        );

        let two_rakes = blue_slaver_with_history(&[RAKE, RAKE]);
        assert_eq!(
            SlaverBlue::roll_move_plan(&mut rng, &two_rakes, 16, 0).move_id,
            STAB,
            "Below A17 Java blocks Rake only after lastTwoMoves(Rake)"
        );

        let one_rake_a17 = blue_slaver_with_history(&[RAKE]);
        assert_eq!(
            SlaverBlue::roll_move_plan(&mut rng, &one_rake_a17, 17, 0).move_id,
            STAB,
            "At A17+ Java blocks Rake after lastMove(Rake), not lastTwoMoves(Rake)"
        );
    }

    #[test]
    fn blue_slaver_take_turn_actions_preserve_java_order_and_weak_amounts() {
        let mut state = crate::testing::support::blank_test_combat();
        let monster = crate::testing::support::test_monster(EnemyId::SlaverBlue);

        let rake_a16 = super::rake_plan(16);
        let actions = SlaverBlue::take_turn_plan(&mut state, &monster, &rake_a16);
        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 8,
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

        let rake_a17 = super::rake_plan(17);
        let actions = SlaverBlue::take_turn_plan(&mut state, &monster, &rake_a17);
        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 8,
                    ..
                },
                Action::ApplyPower {
                    source: 1,
                    target: 0,
                    power_id: PowerId::Weak,
                    amount: 2,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }
}
