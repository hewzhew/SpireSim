use super::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, EffectStrength, MonsterTurnPlan, MoveStep,
    MoveTarget, PowerEffectKind,
};

const BITE: u8 = 1;
const GROW: u8 = 2;

pub struct FungiBeast;

enum FungiBeastTurn<'a> {
    Bite(&'a AttackSpec),
    Grow(&'a ApplyPowerStep),
}

fn bite_damage() -> i32 {
    6
}

fn grow_strength(asc: u8) -> i32 {
    if asc >= 17 {
        if asc >= 2 {
            5
        } else {
            4
        }
    } else if asc >= 2 {
        4
    } else {
        3
    }
}

fn bite_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        BITE,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: bite_damage(),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn grow_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        GROW,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Strength,
            amount: grow_strength(asc),
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        BITE => bite_plan(),
        GROW => grow_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        special_moves: vec![],
        moves: vec![
            (Text::new("fungi_beast.bite"), bite_plan()),
            (Text::new("fungi_beast.grow"), grow_plan(asc)),
        ],
        notes: Text::new("fungi_beast.notes"),
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

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> FungiBeastTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            BITE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => FungiBeastTurn::Bite(attack),
        (
            GROW,
            [MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Strength,
                effect: PowerEffectKind::Buff,
                ..
            })],
        ) => {
            let MoveStep::ApplyPower(power) = &plan.steps[0] else {
                unreachable!()
            };
            FungiBeastTurn::Grow(power)
        }
        (_, []) => panic!("fungi beast plan missing locked truth"),
        (move_id, steps) => panic!("fungi beast plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for FungiBeast {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        asc: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if num < 60 {
            if last_two_moves(entity, BITE) {
                grow_plan(asc)
            } else {
                bite_plan()
            }
        } else if last_move(entity, GROW) {
            bite_plan()
        } else {
            grow_plan(asc)
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::SporeCloud,
            amount: 2,
        }]
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match decode_turn(plan) {
            FungiBeastTurn::Bite(attack) => attack_actions(entity.id, PLAYER, attack),
            FungiBeastTurn::Grow(power) => vec![apply_power_action(entity, power)],
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::{FungiBeast, BITE, GROW};
    use crate::content::monsters::{EnemyId, MonsterBehavior, PreBattleLegacyRng};
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;

    fn fungi_with_history(history: &[u8]) -> crate::runtime::combat::MonsterEntity {
        let mut monster = crate::testing::support::test_monster(EnemyId::FungiBeast);
        monster.move_history_mut().extend(history.iter().copied());
        monster
    }

    #[test]
    fn fungi_beast_roll_logic_matches_java_history_branches() {
        let mut rng = crate::runtime::rng::StsRng::new(1);

        let no_history = fungi_with_history(&[]);
        assert_eq!(
            FungiBeast::roll_move_plan(&mut rng, &no_history, 0, 59).move_id,
            BITE,
            "Java low roll bites unless lastTwoMoves(BITE) blocks it"
        );

        let two_bites = fungi_with_history(&[BITE, BITE]);
        assert_eq!(
            FungiBeast::roll_move_plan(&mut rng, &two_bites, 0, 59).move_id,
            GROW
        );

        let previous_grow = fungi_with_history(&[GROW]);
        assert_eq!(
            FungiBeast::roll_move_plan(&mut rng, &previous_grow, 0, 60).move_id,
            BITE,
            "Java high roll bites if the previous move was Grow"
        );

        let previous_bite = fungi_with_history(&[BITE]);
        assert_eq!(
            FungiBeast::roll_move_plan(&mut rng, &previous_bite, 0, 60).move_id,
            GROW
        );
    }

    #[test]
    fn fungi_beast_prebattle_spore_cloud_and_grow_amounts_match_java() {
        let mut state = crate::testing::support::blank_test_combat();
        let monster = crate::testing::support::test_monster(EnemyId::FungiBeast);

        let actions =
            FungiBeast::use_pre_battle_actions(&mut state, &monster, PreBattleLegacyRng::Misc);
        assert_eq!(
            actions,
            vec![Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::SporeCloud,
                amount: 2,
            }],
            "Java FungiBeast.usePreBattleAction applies SporeCloudPower(this, 2)"
        );

        let grow_a1 = FungiBeast::take_turn_plan(&mut state, &monster, &super::grow_plan(1));
        assert!(matches!(
            grow_a1.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Strength,
                    amount: 3,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));

        let grow_a17 = FungiBeast::take_turn_plan(&mut state, &monster, &super::grow_plan(17));
        assert!(matches!(
            grow_a17.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Strength,
                    amount: 5,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }
}
