use super::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

const STAB: u8 = 1;
const ENTANGLE: u8 = 2;
const SCRAPE: u8 = 3;

pub struct SlaverRed;

enum SlaverRedTurn<'a> {
    Stab(&'a AttackSpec),
    Entangle(&'a ApplyPowerStep),
    Scrape(&'a AttackSpec, &'a ApplyPowerStep),
}

fn stab_damage(asc: u8) -> i32 {
    if asc >= 2 {
        14
    } else {
        13
    }
}

fn scrape_damage(asc: u8) -> i32 {
    if asc >= 2 {
        9
    } else {
        8
    }
}

fn vulnerable_amount(asc: u8) -> i32 {
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

fn entangle_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        ENTANGLE,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Entangle,
            amount: 1,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Strong,
        }),
    )
}

fn scrape_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        SCRAPE,
        smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: scrape_damage(asc),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Vulnerable,
                amount: vulnerable_amount(asc),
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: scrape_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Vulnerable,
                amount: vulnerable_amount(asc),
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        STAB => stab_plan(asc),
        ENTANGLE => entangle_plan(),
        SCRAPE => scrape_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("slaver_red.stab"), stab_plan(asc)),
            (Text::new("slaver_red.entangle"), entangle_plan()),
            (Text::new("slaver_red.scrape"), scrape_plan(asc)),
        ],
        special_moves: vec![],
        notes: if asc >= 17 {
            Text::new("slaver_red.notesHigh")
        } else {
            Text::new("slaver_red.notesLow")
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

fn runtime(entity: &MonsterEntity) -> (bool, bool) {
    assert!(
        entity.slaver_red.protocol_seeded,
        "red slaver runtime truth must be protocol-seeded or factory-seeded"
    );
    (
        entity.slaver_red.first_turn,
        entity.slaver_red.used_entangle,
    )
}

fn red_slaver_runtime_update(
    entity: &MonsterEntity,
    first_turn: Option<bool>,
    used_entangle: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::SlaverRed {
            first_turn,
            used_entangle,
            protocol_seeded: Some(true),
        },
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> SlaverRedTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            STAB,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => SlaverRedTurn::Stab(attack),
        (
            ENTANGLE,
            [MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Entangle,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Strong,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(entangle) = &plan.steps[0] else {
                unreachable!()
            };
            SlaverRedTurn::Entangle(entangle)
        }
        (
            SCRAPE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Vulnerable,
                effect: PowerEffectKind::Debuff,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(vulnerable) = &plan.steps[1] else {
                unreachable!()
            };
            SlaverRedTurn::Scrape(attack, vulnerable)
        }
        (_, []) => panic!("slaver red plan missing locked truth"),
        (move_id, steps) => panic!("slaver red plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for SlaverRed {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        asc: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        let (first_turn, used_entangle) = runtime(entity);
        if first_turn {
            return stab_plan(asc);
        }
        if num >= 75 && !used_entangle {
            return entangle_plan();
        }
        if num >= 55 && used_entangle && !last_two_moves(entity, STAB) {
            return stab_plan(asc);
        }
        if asc >= 17 {
            if !last_move(entity, SCRAPE) {
                return scrape_plan(asc);
            }
            return stab_plan(asc);
        }
        if !last_two_moves(entity, SCRAPE) {
            return scrape_plan(asc);
        }
        stab_plan(asc)
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let (first_turn, _) = runtime(entity);
        if first_turn {
            vec![red_slaver_runtime_update(entity, Some(false), None)]
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
            SlaverRedTurn::Stab(attack) => attack_actions(entity.id, PLAYER, attack),
            SlaverRedTurn::Entangle(entangle) => vec![
                red_slaver_runtime_update(entity, None, Some(true)),
                apply_power_action(entity, entangle),
            ],
            SlaverRedTurn::Scrape(attack, vulnerable) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
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

#[cfg(test)]
mod tests {
    use super::{SlaverRed, ENTANGLE, SCRAPE, STAB};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;
    use crate::semantics::combat::{MonsterMoveSpec, MoveStep};

    fn red_slaver_with_runtime(
        history: &[u8],
        first_turn: bool,
        used_entangle: bool,
    ) -> crate::runtime::combat::MonsterEntity {
        let mut monster = crate::testing::support::test_monster(EnemyId::SlaverRed);
        monster.move_history_mut().extend(history.iter().copied());
        monster.slaver_red.first_turn = first_turn;
        monster.slaver_red.used_entangle = used_entangle;
        monster
    }

    #[test]
    fn red_slaver_roll_logic_uses_explicit_java_private_flags() {
        let mut rng = crate::runtime::rng::StsRng::new(1);

        let first = red_slaver_with_runtime(&[], true, false);
        assert_eq!(
            SlaverRed::roll_move_plan(&mut rng, &first, 0, 99).move_id,
            STAB,
            "Java firstTurn forces Stab before considering the random roll"
        );

        let after_first_stab = red_slaver_with_runtime(&[STAB], false, false);
        assert_eq!(
            SlaverRed::roll_move_plan(&mut rng, &after_first_stab, 0, 75).move_id,
            ENTANGLE,
            "Java uses Entangle once when num >= 75 and usedEntangle is false"
        );

        let after_entangle = red_slaver_with_runtime(&[STAB, ENTANGLE], false, true);
        assert_eq!(
            SlaverRed::roll_move_plan(&mut rng, &after_entangle, 0, 55).move_id,
            STAB,
            "Java post-Entangle high roll prefers Stab unless the last two moves were Stab"
        );

        let after_entangle_and_two_stabs =
            red_slaver_with_runtime(&[STAB, ENTANGLE, STAB, STAB], false, true);
        assert_eq!(
            SlaverRed::roll_move_plan(&mut rng, &after_entangle_and_two_stabs, 0, 99).move_id,
            SCRAPE,
            "Java falls through to Scrape when post-Entangle Stab is blocked by lastTwoMoves"
        );
    }

    #[test]
    fn red_slaver_used_entangle_is_private_runtime_not_truncated_history() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let monster = red_slaver_with_runtime(&[STAB], false, true);

        assert_eq!(
            SlaverRed::roll_move_plan(&mut rng, &monster, 0, 99).move_id,
            STAB,
            "Java uses private usedEntangle; live imports with truncated moveHistory must not allow Entangle again"
        );
    }

    #[test]
    fn red_slaver_first_turn_is_private_runtime_not_empty_history() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let monster = red_slaver_with_runtime(&[], false, false);

        assert_eq!(
            SlaverRed::roll_move_plan(&mut rng, &monster, 17, 0).move_id,
            SCRAPE,
            "Java uses private firstTurn; empty imported history alone must not force first-turn Stab"
        );
    }

    #[test]
    fn red_slaver_a17_scrape_cannot_repeat_immediately_like_java() {
        let mut rng = crate::runtime::rng::StsRng::new(1);

        let after_scrape = red_slaver_with_runtime(&[STAB, SCRAPE], false, false);
        assert_eq!(
            SlaverRed::roll_move_plan(&mut rng, &after_scrape, 17, 0).move_id,
            STAB,
            "Java A17+ checks lastMove(Scrape), not lastTwoMoves(Scrape)"
        );

        let after_non_scrape = red_slaver_with_runtime(&[STAB, STAB], false, false);
        assert_eq!(
            SlaverRed::roll_move_plan(&mut rng, &after_non_scrape, 17, 0).move_id,
            SCRAPE
        );
    }

    #[test]
    fn red_slaver_take_turn_actions_preserve_java_order_and_amounts() {
        let mut state = crate::testing::support::blank_test_combat();
        state.meta.ascension_level = 17;
        let monster = crate::testing::support::test_monster(EnemyId::SlaverRed);

        let entangle_actions =
            SlaverRed::take_turn_plan(&mut state, &monster, &super::entangle_plan());
        assert!(matches!(
            entangle_actions.as_slice(),
            [
                Action::UpdateMonsterRuntime {
                    monster_id: 1,
                    patch: crate::runtime::action::MonsterRuntimePatch::SlaverRed {
                        first_turn: None,
                        used_entangle: Some(true),
                        protocol_seeded: Some(true),
                    },
                },
                Action::ApplyPower {
                    source: 1,
                    target: 0,
                    power_id: PowerId::Entangle,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));

        let scrape_actions =
            SlaverRed::take_turn_plan(&mut state, &monster, &super::scrape_plan(17));
        assert!(matches!(
            scrape_actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 9,
                    ..
                },
                Action::ApplyPower {
                    source: 1,
                    target: 0,
                    power_id: PowerId::Vulnerable,
                    amount: 2,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));

        assert!(matches!(
            super::entangle_plan().summary_spec(),
            MonsterMoveSpec::StrongDebuff(_)
        ));
        assert!(matches!(
            super::scrape_plan(17).steps.as_slice(),
            [MoveStep::Attack(_), MoveStep::ApplyPower(_)]
        ));
    }
}
