use crate::content::cards::CardId;
use crate::content::monsters::exordium::{attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, AttackSpec, CardDestination, DamageKind, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep,
};
use smallvec::smallvec;

pub struct OrbWalker;

const LASER: u8 = 1;
const CLAW: u8 = 2;

fn laser_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        11
    } else {
        10
    }
}

fn claw_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        16
    } else {
        15
    }
}

fn strength_per_round(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        5
    } else {
        3
    }
}

pub fn mechanics(ascension_level: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Laser", laser_plan(ascension_level)), ("Claw", claw_plan(ascension_level))],
        special_moves: vec![("Startup", format!("Starts with Generic Strength Up {}: gain that much permanent Strength at the end of every round, after its attack. This does not grant Strength immediately on spawn.", strength_per_round(ascension_level)))],
        notes: "Each move roll chooses Claw below 40 (40%) or Laser otherwise (60%); if the selected move has occurred twice consecutively, use the other move. Neither move can occur three times consecutively. Laser adds one Burn to discard and one Burn at a random draw-pile position. These are possible move rules, not a prediction of the next roll.",
    }
}

fn burn_discard_step() -> AddCardStep {
    AddCardStep {
        card_id: CardId::Burn,
        amount: 1,
        upgraded: false,
        destination: CardDestination::Discard,
        visible_strength: EffectStrength::Normal,
    }
}

fn burn_draw_step() -> AddCardStep {
    AddCardStep {
        card_id: CardId::Burn,
        amount: 1,
        upgraded: false,
        destination: CardDestination::DrawPileRandom,
        visible_strength: EffectStrength::Normal,
    }
}

fn laser_plan(ascension_level: u8) -> MonsterTurnPlan {
    let visible_add = burn_discard_step();
    MonsterTurnPlan::with_visible_spec(
        LASER,
        smallvec![
            MoveStep::Attack(crate::semantics::combat::AttackStep {
                target: crate::semantics::combat::MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: laser_damage(ascension_level),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::AddCard(burn_discard_step()),
            MoveStep::AddCard(burn_draw_step()),
        ],
        MonsterMoveSpec::AttackAddCard(
            AttackSpec {
                base_damage: laser_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            visible_add,
        ),
    )
}

fn claw_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        CLAW,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: claw_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let mut history = entity.move_history().iter().rev();
    matches!(
        (history.next().copied(), history.next().copied()),
        (Some(a), Some(b)) if a == move_id && b == move_id
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        LASER => laser_plan(ascension_level),
        CLAW => claw_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

impl MonsterBehavior for OrbWalker {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::GenericStrengthUp,
            amount: strength_per_round(ascension_level),
        }]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if num < 40 {
            if !last_two_moves(entity, CLAW) {
                claw_plan(ascension_level)
            } else {
                laser_plan(ascension_level)
            }
        } else if !last_two_moves(entity, LASER) {
            laser_plan(ascension_level)
        } else {
            claw_plan(ascension_level)
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
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (CLAW, [MoveStep::Attack(attack)]) => attack_actions(entity.id, PLAYER, &attack.attack),
            (
                LASER,
                [MoveStep::Attack(attack), MoveStep::AddCard(_discard_burn), MoveStep::AddCard(_draw_burn)],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, &attack.attack);
                actions.push(Action::MakeTempCardInDiscardAndDeck {
                    card_id: CardId::Burn,
                    amount: 1,
                });
                actions
            }
            (move_id, steps) => panic!("orb walker plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::{laser_plan, OrbWalker, CLAW, LASER};
    use crate::content::cards::CardId;
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;
    use crate::runtime::rng::StsRng;

    #[test]
    fn laser_uses_java_discard_and_deck_action_not_two_expanded_add_cards() {
        let mut state = crate::test_support::blank_test_combat();
        let walker = crate::test_support::test_monster(EnemyId::OrbWalker);

        let actions = OrbWalker::take_turn_plan(&mut state, &walker, &laser_plan(0));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack { source: 1, .. },
                Action::MakeTempCardInDiscardAndDeck {
                    card_id: CardId::Burn,
                    amount: 1
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn pre_battle_generic_strength_up_uses_java_ascension_gate() {
        let mut walker = crate::test_support::test_monster(EnemyId::OrbWalker);
        walker.id = 77;

        let mut low = crate::test_support::blank_test_combat();
        low.meta.ascension_level = 16;
        let low_actions = OrbWalker::use_pre_battle_actions(
            &mut low,
            &walker,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );
        assert_eq!(
            low_actions,
            vec![Action::ApplyPower {
                source: 77,
                target: 77,
                power_id: PowerId::GenericStrengthUp,
                amount: 3,
            }]
        );

        let mut high = crate::test_support::blank_test_combat();
        high.meta.ascension_level = 17;
        let high_actions = OrbWalker::use_pre_battle_actions(
            &mut high,
            &walker,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );
        assert_eq!(
            high_actions,
            vec![Action::ApplyPower {
                source: 77,
                target: 77,
                power_id: PowerId::GenericStrengthUp,
                amount: 5,
            }],
            "Java OrbWalker applies GenericStrengthUpPower 5 only at A17+"
        );
    }

    #[test]
    fn get_move_uses_java_last_two_move_gates_without_rerolling() {
        let mut walker = crate::test_support::test_monster(EnemyId::OrbWalker);

        let plan =
            <OrbWalker as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &walker, 20, 39);
        assert_eq!(plan.move_id, CLAW);

        walker.move_history_mut().push_back(CLAW);
        walker.move_history_mut().push_back(CLAW);
        let plan =
            <OrbWalker as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &walker, 20, 39);
        assert_eq!(
            plan.move_id, LASER,
            "Java low roll falls through to Laser after lastTwoMoves(CLAW)"
        );

        walker.move_history_mut().clear();
        walker.move_history_mut().push_back(LASER);
        walker.move_history_mut().push_back(LASER);
        let plan =
            <OrbWalker as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &walker, 20, 40);
        assert_eq!(
            plan.move_id, CLAW,
            "Java high roll falls through to Claw after lastTwoMoves(LASER)"
        );
    }

    #[test]
    fn laser_at_a2_queues_damage_burn_and_roll_in_java_order() {
        let mut state = crate::test_support::blank_test_combat();
        let mut walker = crate::test_support::test_monster(EnemyId::OrbWalker);
        walker.id = 77;

        let actions = OrbWalker::take_turn_plan(&mut state, &walker, &laser_plan(2));

        assert_eq!(
            actions,
            vec![
                Action::MonsterAttack {
                    source: 77,
                    target: crate::content::monsters::exordium::PLAYER,
                    base_damage: 11,
                    damage_kind: crate::semantics::combat::DamageKind::Normal,
                },
                Action::MakeTempCardInDiscardAndDeck {
                    card_id: CardId::Burn,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 77 },
            ],
            "Java Laser queues DamageAction, MakeTempCardInDiscardAndDeckAction(Burn), then RollMoveAction"
        );
    }
}
