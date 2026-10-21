use crate::content::cards::CardId;
use crate::content::monsters::exordium::{add_card_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, AttackSpec, CardDestination, DamageKind, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep,
};

pub struct Repulsor;

const DAZE: u8 = 1;
const ATTACK: u8 = 2;
const ATTACK_ROLL_CUTOFF: i32 = 20;

pub fn mechanics(ascension: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Attack", attack_plan(ascension)), ("Daze", daze_plan())],
        special_moves: vec![("Move selection", format!("A move roll below {ATTACK_ROLL_CUTOFF} selects Attack unless the previous move was Attack; otherwise it selects Daze. Attack cannot repeat immediately."))],
        notes: "Daze inserts cards into random positions in the draw pile, not the discard pile. Possible moves are not a prediction of the next roll.",
    }
}

fn attack_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        13
    } else {
        11
    }
}

fn attack_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        ATTACK,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: attack_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn daze_step() -> AddCardStep {
    AddCardStep {
        card_id: CardId::Dazed,
        amount: 2,
        upgraded: false,
        destination: CardDestination::DrawPileRandom,
        visible_strength: EffectStrength::Normal,
    }
}

fn daze_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(DAZE, MonsterMoveSpec::AddCard(daze_step()))
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        ATTACK => attack_plan(ascension_level),
        DAZE => daze_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

impl MonsterBehavior for Repulsor {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if num < ATTACK_ROLL_CUTOFF && entity.move_history().back().copied() != Some(ATTACK) {
            attack_plan(ascension_level)
        } else {
            daze_plan()
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
            (ATTACK, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (DAZE, [MoveStep::AddCard(add_card)]) => vec![add_card_action(add_card)],
            (move_id, steps) => panic!("repulsor plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::{Repulsor, ATTACK, DAZE};
    use crate::content::cards::CardId;
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::Action;
    use crate::runtime::rng::StsRng;

    #[test]
    fn get_move_matches_java_low_roll_and_last_attack_gate() {
        let mut repulsor = crate::test_support::test_monster(EnemyId::Repulsor);

        let plan =
            <Repulsor as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &repulsor, 20, 19);
        assert_eq!(plan.move_id, ATTACK);

        repulsor.move_history_mut().push_back(ATTACK);
        let plan =
            <Repulsor as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &repulsor, 20, 19);
        assert_eq!(
            plan.move_id, DAZE,
            "Java Repulsor blocks Attack only with lastMove(ATTACK), not lastTwoMoves"
        );

        repulsor.move_history_mut().clear();
        let plan =
            <Repulsor as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &repulsor, 20, 20);
        assert_eq!(plan.move_id, DAZE);
    }

    #[test]
    fn attack_at_a2_queues_damage_then_roll() {
        let mut state = crate::test_support::blank_test_combat();
        let mut repulsor = crate::test_support::test_monster(EnemyId::Repulsor);
        repulsor.id = 81;
        let plan =
            <Repulsor as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &repulsor, 2, 0);

        let actions = Repulsor::take_turn_plan(&mut state, &repulsor, &plan);

        assert_eq!(
            actions,
            vec![
                Action::MonsterAttack {
                    source: 81,
                    target: crate::content::monsters::exordium::PLAYER,
                    base_damage: 13,
                    damage_kind: crate::semantics::combat::DamageKind::Normal,
                },
                Action::RollMonsterMove { monster_id: 81 },
            ],
            "Java Attack queues one DamageAction before RollMoveAction"
        );
    }

    #[test]
    fn daze_queues_random_draw_pile_dazed_then_roll() {
        let mut state = crate::test_support::blank_test_combat();
        let mut repulsor = crate::test_support::test_monster(EnemyId::Repulsor);
        repulsor.id = 81;
        repulsor.move_history_mut().push_back(ATTACK);
        let plan =
            <Repulsor as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &repulsor, 20, 19);

        let actions = Repulsor::take_turn_plan(&mut state, &repulsor, &plan);

        assert_eq!(
            actions,
            vec![
                Action::MakeTempCardInDrawPile {
                    card_id: CardId::Dazed,
                    amount: 2,
                    random_spot: true,
                    to_bottom: false,
                    upgraded: false,
                },
                Action::RollMonsterMove { monster_id: 81 },
            ],
            "Java Daze queues MakeTempCardInDrawPileAction(new Dazed(), 2, true, true), then RollMoveAction"
        );
    }
}
