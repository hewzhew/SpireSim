use serde::{Deserialize, Serialize};

use crate::content::cards::{
    get_card_definition, upgraded_base_cost_override, upgraded_base_cost_override_for, CardId,
    CardType,
};
use crate::content::relics::RelicId;
use crate::runtime::combat::CombatCard;
use crate::state::run::RunState;

/// Public-state evidence that a Power can turn Mummified Hand into immediate
/// tempo after its own energy cost has been paid.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MummifiedHandPowerTempoV1 {
    pub card: CardId,
    pub paid_cost: i8,
    pub eligible_positive_cost_cards: usize,
}

pub fn mummified_hand_power_tempo_v1(
    run_state: &RunState,
    card: CardId,
    upgrades: u8,
) -> Option<MummifiedHandPowerTempoV1> {
    if !run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::MummifiedHand)
        || get_card_definition(card).card_type != CardType::Power
    {
        return None;
    }

    Some(MummifiedHandPowerTempoV1 {
        card,
        paid_cost: upgraded_base_cost_override_for(card, upgrades)
            .unwrap_or_else(|| get_card_definition(card).cost),
        eligible_positive_cost_cards: run_state
            .master_deck
            .iter()
            .filter(|owned| base_cost(owned) > 0)
            .count(),
    })
}

fn base_cost(card: &CombatCard) -> i8 {
    upgraded_base_cost_override(card).unwrap_or_else(|| get_card_definition(card.id).cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;

    #[test]
    fn mummified_hand_fact_requires_the_relic_and_a_power() {
        let mut run = RunState::new(7, 0, false, "Ironclad");
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::DarkEmbrace, 2),
        ];

        assert_eq!(
            mummified_hand_power_tempo_v1(&run, CardId::DarkEmbrace, 0),
            None
        );
        run.relics.push(RelicState::new(RelicId::MummifiedHand));
        assert_eq!(
            mummified_hand_power_tempo_v1(&run, CardId::Uppercut, 0),
            None
        );

        let fact = mummified_hand_power_tempo_v1(&run, CardId::DarkEmbrace, 1)
            .expect("upgraded Dark Embrace should expose Mummified Hand tempo");
        assert_eq!(fact.paid_cost, 1);
        assert_eq!(fact.eligible_positive_cost_cards, 2);
    }
}
