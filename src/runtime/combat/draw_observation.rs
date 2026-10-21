//! Output-only draw-pile changes. No hidden position or runtime identity is exposed.

use super::CombatCard;
use crate::content::cards::CardId;
use serde::{Deserialize, Serialize};

/// Knowledge of a card's kind, not of an instance or its changing combat cost.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DrawCardKind {
    pub card_id: CardId,
    pub upgrades: u8,
}

impl From<&CombatCard> for DrawCardKind {
    fn from(card: &CombatCard) -> Self {
        Self {
            card_id: card.id,
            upgrades: card.upgrades,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DrawPileChange {
    TakeTop {
        card: DrawCardKind,
    },
    PutTop {
        card: DrawCardKind,
    },
    PutBottom {
        card: DrawCardKind,
    },
    /// The inserted card is visible; its realized insertion index is not.
    InsertUnpositioned {
        card: DrawCardKind,
    },
    /// The removed kind is visible, but its old position/instance is not inferred.
    RemoveUnpositioned {
        card: DrawCardKind,
    },
    /// A shuffled discard group was prepended; the old draw group is untouched.
    PrependShuffled {
        count: usize,
    },
    /// An unclassified mutation/replacement loses order knowledge regardless of
    /// whether the hidden permutation happened to remain the same.
    OrderUnknown {
        count: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::DrawPile;

    #[test]
    fn hidden_insertions_have_identical_receipts_at_different_positions() {
        let initial: DrawPile = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ]
        .into();
        let mut left = initial.clone();
        let mut right = initial.clone();
        left.begin_observation();
        right.begin_observation();
        left.insert(0, CombatCard::for_scenario(CardId::Bash, 3));
        right.insert(2, CombatCard::for_scenario(CardId::Bash, 3));
        assert_ne!(left, right);
        assert_eq!(left.take_observation(), right.take_observation());
    }

    #[test]
    fn observation_is_absent_from_identity_serialization_and_clones() {
        let mut observed: DrawPile = Vec::new().into();
        observed.begin_observation();
        observed.push_top(CombatCard::for_scenario(CardId::Strike, 1));
        let mut cloned = observed.clone();
        assert_eq!(cloned, observed);
        assert_eq!(
            serde_json::to_value(&cloned).unwrap(),
            serde_json::to_value(&observed).unwrap()
        );
        assert_eq!(cloned.take_observation(), None);
        assert_eq!(observed.take_observation().unwrap().len(), 1);
        cloned.draw_top();
        assert_eq!(cloned.take_observation(), None);
    }

    #[test]
    fn unclassified_mutation_invalidates_even_when_order_is_unchanged() {
        let mut pile: DrawPile = vec![CombatCard::for_scenario(CardId::Strike, 1)].into();
        pile.begin_observation();
        pile.reverse();
        assert_eq!(
            pile.take_observation().unwrap(),
            [DrawPileChange::OrderUnknown { count: 1 }]
        );
    }

    #[test]
    fn tracked_attribute_mutation_preserves_kind_order_but_kind_changes_invalidate() {
        let mut pile: DrawPile = vec![CombatCard::for_scenario(CardId::Strike, 1)].into();
        pile.begin_observation();
        pile.mutate_cards_with_kind_tracking(CombatCard::reset_attributes_java);
        assert!(pile.take_observation().unwrap().is_empty());

        pile.begin_observation();
        pile.mutate_cards_with_kind_tracking(|card| card.id = CardId::Defend);
        assert_eq!(
            pile.take_observation().unwrap(),
            [DrawPileChange::OrderUnknown { count: 1 }]
        );
    }
}
