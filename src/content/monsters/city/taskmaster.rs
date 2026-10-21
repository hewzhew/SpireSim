use crate::content::cards::CardId;
use crate::content::monsters::exordium::{
    add_card_action, apply_power_action, attack_actions, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, ApplyPowerStep, AttackSpec, AttackStep, CardDestination, DamageKind,
    EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct Taskmaster;

const SCOURING_WHIP: u8 = 2;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::{EnemyId, MonsterBehavior};

    #[test]
    fn roll_move_is_always_scouring_whip_like_java_get_move() {
        let taskmaster = crate::test_support::test_monster(EnemyId::Taskmaster);

        let low_roll =
            Taskmaster::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &taskmaster, 0, 0);
        let high_roll = Taskmaster::roll_move_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &taskmaster,
            18,
            99,
        );

        assert_eq!(low_roll.move_id, SCOURING_WHIP);
        assert_eq!(high_roll.move_id, SCOURING_WHIP);
    }

    #[test]
    fn scouring_whip_wound_count_follows_java_ascension_thresholds() {
        assert_eq!(wound_count(0), 1);
        assert_eq!(wound_count(3), 2);
        assert_eq!(wound_count(17), 2);
        assert_eq!(wound_count(18), 3);
    }

    #[test]
    fn scouring_whip_queues_damage_wounds_strength_and_roll_like_java_a18() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 18;
        let taskmaster = crate::test_support::test_monster(EnemyId::Taskmaster);

        let actions = Taskmaster::take_turn_plan(&mut state, &taskmaster, &whip_plan(18));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 7,
                    ..
                },
                Action::MakeTempCardInDiscard {
                    card_id: CardId::Wound,
                    amount: 3,
                    upgraded: false
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Strength,
                    amount: 1
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn scouring_whip_below_a18_skips_strength_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 17;
        let taskmaster = crate::test_support::test_monster(EnemyId::Taskmaster);

        let actions = Taskmaster::take_turn_plan(&mut state, &taskmaster, &whip_plan(17));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 7,
                    ..
                },
                Action::MakeTempCardInDiscard {
                    card_id: CardId::Wound,
                    amount: 2,
                    upgraded: false
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }
}

fn whip_damage() -> i32 {
    7
}

fn wound_count(ascension_level: u8) -> u8 {
    if ascension_level >= 18 {
        3
    } else if ascension_level >= 3 {
        2
    } else {
        1
    }
}

fn strength_gain(ascension_level: u8) -> i32 {
    i32::from(ascension_level >= 18)
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Scouring Whip", whip_plan(asc))],
        special_moves: if strength_gain(asc) > 0 {
            vec![("Scouring Whip follow-up", format!("After attacking and adding Wounds, gain {} Strength on self. This Strength persists and increases later attacks.", strength_gain(asc)))]
        } else {
            vec![]
        },
        notes: "Uses Scouring Whip every turn. Wounds go to the discard pile even if the attack is fully blocked. At A18+, also gains Strength after each Whip; this is an execution effect beyond the visible attack-and-card intent.",
    }
}

fn whip_plan(ascension_level: u8) -> MonsterTurnPlan {
    let add_wound = AddCardStep {
        card_id: CardId::Wound,
        amount: wound_count(ascension_level),
        upgraded: false,
        destination: CardDestination::Discard,
        visible_strength: EffectStrength::Normal,
    };
    MonsterTurnPlan::with_visible_spec(
        SCOURING_WHIP,
        smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: whip_damage(),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::AddCard(add_wound.clone()),
        ],
        MonsterMoveSpec::AttackAddCard(
            AttackSpec {
                base_damage: whip_damage(),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            add_wound,
        ),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        SCOURING_WHIP => whip_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

impl MonsterBehavior for Taskmaster {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        whip_plan(ascension_level)
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
            (
                SCOURING_WHIP,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                }), MoveStep::AddCard(add_wound)],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(add_card_action(add_wound));
                actions
            }
            (_, []) => panic!("taskmaster plan missing locked truth"),
            (move_id, steps) => panic!("taskmaster plan/steps mismatch: {} {:?}", move_id, steps),
        };

        let strength = strength_gain(state.meta.ascension_level);
        if strength > 0 {
            actions.push(apply_power_action(
                entity,
                &ApplyPowerStep {
                    target: MoveTarget::SelfTarget,
                    power_id: PowerId::Strength,
                    amount: strength,
                    effect: PowerEffectKind::Buff,
                    visible_strength: EffectStrength::Normal,
                },
            ));
        }
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
