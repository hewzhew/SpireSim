use crate::runtime::action::Action;
use crate::runtime::combat::{CardLineageId, CombatCard, CombatState};
use crate::EntityId;
use serde::{Deserialize, Serialize};

pub const MIN_RANDOMIZED_CARD_COST: u8 = 0;
pub const MAX_RANDOMIZED_CARD_COST: u8 = 3;

pub const MECHANICS: crate::content::powers::mechanics::PowerMechanics =
    crate::content::powers::mechanics::PowerMechanics {
        amount_unit: "presence marker (sentinel -1)",
        effect: "Whenever a card with a nonnegative combat cost is drawn, randomize that instance's combat cost to 0, 1, 2 or 3. X-cost and unplayable cards are excluded. This can make cheap cards more expensive or expensive cards cheaper. Each new draw can randomize the cost again; use the displayed hand cost.",
        timing: "Persists for the rest of combat; the amount is not a duration or a multiplier.",
        round_duration: false,
    };

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawnCardCostEligibilityV1 {
    NonnegativeCombatCost,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawnCardTurnCostUpdateV1 {
    OnlyWhenCombatCostChanges,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DrawnCardCostRandomizationFactsV1 {
    pub minimum_cost: u8,
    pub maximum_cost: u8,
    pub eligible_cost: DrawnCardCostEligibilityV1,
    pub turn_cost_update: DrawnCardTurnCostUpdateV1,
    pub clears_free_to_play_once: bool,
}

pub fn cost_randomization_rule_v1() -> DrawnCardCostRandomizationFactsV1 {
    DrawnCardCostRandomizationFactsV1 {
        minimum_cost: MIN_RANDOMIZED_CARD_COST,
        maximum_cost: MAX_RANDOMIZED_CARD_COST,
        eligible_cost: DrawnCardCostEligibilityV1::NonnegativeCombatCost,
        turn_cost_update: DrawnCardTurnCostUpdateV1::OnlyWhenCombatCostChanges,
        clears_free_to_play_once: true,
    }
}

// Confusion randomizes card costs when drawn.
pub fn on_card_draw(state: &mut CombatState, card: &mut CombatCard) {
    let rule = cost_randomization_rule_v1();
    let combat_cost = card.combat_cost_without_turn_override_java();
    let eligible = match rule.eligible_cost {
        DrawnCardCostEligibilityV1::NonnegativeCombatCost => combat_cost >= 0,
    };
    if eligible {
        let new_cost = state
            .rng
            .card_random_rng
            .random(i32::from(rule.maximum_cost));
        match rule.turn_cost_update {
            DrawnCardTurnCostUpdateV1::OnlyWhenCombatCostChanges => {
                if new_cost != combat_cost {
                    card.set_combat_and_turn_cost_java(new_cost);
                }
            }
        }
        if rule.clears_free_to_play_once {
            card.free_to_play_once = false;
        }
    }
}

pub fn on_card_drawn(
    _state: &mut CombatState,
    _owner: EntityId,
    _card_lineage: CardLineageId,
    _power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;

    #[test]
    fn nonnegative_status_and_curse_costs_consume_the_same_roll_as_other_cards() {
        for id in [CardId::Strike, CardId::Slimed, CardId::Pride] {
            let mut state = crate::test_support::blank_test_combat();
            let mut expected_rng = state.rng.card_random_rng.clone();
            let expected_cost = expected_rng.random(3);
            let mut card = CombatCard::for_scenario(id, 101);
            card.free_to_play_once = true;
            on_card_draw(&mut state, &mut card);
            assert_eq!(
                state.rng.card_random_rng.counter, expected_rng.counter,
                "{id:?}"
            );
            assert_eq!(
                card.combat_cost_without_turn_override_java(),
                expected_cost,
                "{id:?}"
            );
            assert_eq!(card.cost_for_turn_java(), expected_cost, "{id:?}");
            assert!(!card.free_to_play_once);
        }
    }

    #[test]
    fn equal_roll_preserves_turn_override_but_clears_free_play() {
        let mut state = crate::test_support::blank_test_combat();
        let mut expected_rng = state.rng.card_random_rng.clone();
        let rolled_cost = expected_rng.random(3);
        let mut card = CombatCard::for_scenario(CardId::Strike, 102);
        card.set_combat_and_turn_cost_java(rolled_cost);
        let temporary_cost = (rolled_cost + 1) % 4;
        card.set_cost_for_turn_java(temporary_cost);
        card.free_to_play_once = true;
        on_card_draw(&mut state, &mut card);
        assert_eq!(card.combat_cost_without_turn_override_java(), rolled_cost);
        assert_eq!(card.cost_for_turn_java(), temporary_cost);
        assert_eq!(state.rng.card_random_rng.counter, expected_rng.counter);
        assert!(!card.free_to_play_once);
    }

    #[test]
    fn changed_roll_replaces_both_costs_of_the_combat_instance() {
        let mut state = crate::test_support::blank_test_combat();
        let mut expected_rng = state.rng.card_random_rng.clone();
        let rolled_cost = expected_rng.random(3);
        let mut card = CombatCard::for_scenario(CardId::Strike, 105);
        card.set_combat_and_turn_cost_java((rolled_cost + 1) % 4);
        card.set_cost_for_turn_java((rolled_cost + 2) % 4);
        on_card_draw(&mut state, &mut card);
        assert_eq!(card.combat_cost_without_turn_override_java(), rolled_cost);
        assert_eq!(card.cost_for_turn_java(), rolled_cost);
        assert_eq!(state.rng.card_random_rng.counter, expected_rng.counter);
    }

    #[test]
    fn negative_combat_costs_skip_roll_and_free_play_reset_even_with_turn_override() {
        for id in [CardId::Whirlwind, CardId::Wound] {
            let mut state = crate::test_support::blank_test_combat();
            let before = state.rng.card_random_rng.counter;
            let mut card = CombatCard::for_scenario(id, 103);
            card.cost_for_turn = Some(0);
            card.free_to_play_once = true;
            let original = card.clone();
            on_card_draw(&mut state, &mut card);
            assert_eq!(card, original);
            assert_eq!(state.rng.card_random_rng.counter, before);
        }
    }

    #[test]
    fn ordinary_draw_with_confusion_randomizes_slimed_and_preserves_draw_identity() {
        let mut state = crate::test_support::blank_test_combat();
        crate::engine::action_handlers::powers::handle_apply_power(
            0,
            0,
            crate::content::powers::PowerId::Confusion,
            -1,
            &mut state,
        );
        let slimed = CombatCard::for_scenario(CardId::Slimed, 104);
        let lineage = slimed.lineage;
        state.zones.draw_pile = vec![slimed, CombatCard::for_scenario(CardId::Strike, 106)].into();
        let mut expected_rng = state.rng.card_random_rng.clone();
        let expected_cost = expected_rng.random(3);
        let following_cost = expected_rng.random(3);
        crate::engine::action_handlers::execute_action(Action::DrawCards(2), &mut state);
        assert_eq!(state.zones.hand.len(), 2);
        assert_eq!(state.zones.hand[0].lineage, lineage);
        assert_eq!(
            state.zones.hand[0].combat_cost_without_turn_override_java(),
            expected_cost
        );
        assert_eq!(
            state.zones.hand[1].combat_cost_without_turn_override_java(),
            following_cost
        );
        assert_eq!(state.rng.card_random_rng.counter, expected_rng.counter);
        assert!(state.zones.draw_pile.is_empty());
    }
}
