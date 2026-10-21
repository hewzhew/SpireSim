//! Reviewed temporary Status creation requests shared with card execution.
//! Counts and destinations describe queued requests, before combat-end queue
//! clearing, hand capacity and card-creation modifiers. They are not net draw
//! losses or guaranteed additions to a particular future hand.

use super::CardId;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum CardStatusCreationDestinationV1 {
    Hand = 0,
    DrawPileRandom = 1,
    DiscardPile = 2,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum CardStatusCreationTimingV1 {
    AfterAttack = 0,
    BeforeBlock = 1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardStatusCreationOnPlayV1 {
    pub card: CardId,
    pub amount: u8,
    pub destination: CardStatusCreationDestinationV1,
    pub timing: CardStatusCreationTimingV1,
    /// Constructor input, before Master Reality and other creation modifiers.
    pub upgraded: bool,
}

impl CardStatusCreationOnPlayV1 {
    pub fn action(self, state: &CombatState) -> Action {
        match self.destination {
            CardStatusCreationDestinationV1::Hand => {
                crate::content::cards::make_constructed_temp_card_in_hand_action(
                    self.card,
                    self.amount,
                    self.upgraded,
                    state,
                )
            }
            CardStatusCreationDestinationV1::DrawPileRandom => Action::MakeTempCardInDrawPile {
                card_id: self.card,
                amount: self.amount,
                random_spot: true,
                to_bottom: false,
                upgraded: self.upgraded,
            },
            CardStatusCreationDestinationV1::DiscardPile => Action::MakeTempCardInDiscard {
                card_id: self.card,
                amount: self.amount,
                upgraded: self.upgraded,
            },
        }
    }
}

/// Fixed requests currently reviewed for the Ironclad Status producers.
/// Upgrading the source card changes output, but not these creation requests.
/// Other cards remain unreviewed here rather than receiving a zero request.
pub fn card_status_creation_on_play_v1(card: CardId) -> Option<CardStatusCreationOnPlayV1> {
    use CardStatusCreationDestinationV1::{DiscardPile, DrawPileRandom, Hand};
    use CardStatusCreationTimingV1::{AfterAttack, BeforeBlock};
    let (card, amount, destination, timing) = match card {
        CardId::WildStrike => (CardId::Wound, 1, DrawPileRandom, AfterAttack),
        CardId::RecklessCharge => (CardId::Dazed, 1, DrawPileRandom, AfterAttack),
        CardId::PowerThrough => (CardId::Wound, 2, Hand, BeforeBlock),
        CardId::Immolate => (CardId::Burn, 1, DiscardPile, AfterAttack),
        _ => return None,
    };
    Some(CardStatusCreationOnPlayV1 {
        card,
        amount,
        destination,
        timing,
        upgraded: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::resolve_card_play;
    use crate::runtime::combat::CombatCard;

    #[test]
    fn status_creation_keeps_exact_request_and_order_across_upgrades() {
        let state = crate::test_support::blank_test_combat();
        let cases = [
            (
                CardId::WildStrike,
                CardId::Wound,
                1,
                CardStatusCreationDestinationV1::DrawPileRandom,
            ),
            (
                CardId::RecklessCharge,
                CardId::Dazed,
                1,
                CardStatusCreationDestinationV1::DrawPileRandom,
            ),
            (
                CardId::PowerThrough,
                CardId::Wound,
                2,
                CardStatusCreationDestinationV1::Hand,
            ),
            (
                CardId::Immolate,
                CardId::Burn,
                1,
                CardStatusCreationDestinationV1::DiscardPile,
            ),
        ];
        for (source, status, amount, destination) in cases {
            let request = card_status_creation_on_play_v1(source).unwrap();
            assert_eq!(
                (
                    request.card,
                    request.amount,
                    request.destination,
                    request.upgraded
                ),
                (status, amount, destination, false)
            );
            for upgrades in [0, 1] {
                let mut card = CombatCard::for_scenario(source, 1);
                card.upgrades = upgrades;
                let actions = resolve_card_play(source, &state, &card, Some(7));
                assert_eq!(actions.len(), 2);
                if source == CardId::PowerThrough {
                    assert_eq!(request.timing, CardStatusCreationTimingV1::BeforeBlock);
                    assert!(
                        matches!(&actions[0].action, Action::MakeConstructedCopyInHand {
                        original, amount: 2,
                    } if original.id == CardId::Wound && original.upgrades == 0)
                    );
                    assert!(matches!(actions[1].action, Action::GainBlock { amount, .. }
                        if amount == if upgrades == 0 { 15 } else { 20 }));
                } else {
                    assert_eq!(request.timing, CardStatusCreationTimingV1::AfterAttack);
                    assert!(matches!(
                        actions[0].action,
                        Action::Damage(_) | Action::DamageAllEnemies { .. }
                    ));
                    let expected = if source == CardId::Immolate {
                        Action::MakeTempCardInDiscard {
                            card_id: CardId::Burn,
                            amount: 1,
                            upgraded: false,
                        }
                    } else {
                        Action::MakeTempCardInDrawPile {
                            card_id: status,
                            amount: 1,
                            random_spot: true,
                            to_bottom: false,
                            upgraded: false,
                        }
                    };
                    assert_eq!(actions[1].action, expected);
                }
            }
        }
    }

    #[test]
    fn status_creation_does_not_claim_unreviewed_or_trigger_requests() {
        for id in [
            CardId::Turbo,
            CardId::Overclock,
            CardId::Evolve,
            CardId::FireBreathing,
            CardId::Wound,
            CardId::Pride,
        ] {
            assert_eq!(card_status_creation_on_play_v1(id), None);
        }
    }
}
