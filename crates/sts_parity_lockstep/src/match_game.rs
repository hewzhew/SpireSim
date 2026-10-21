use crate::{DecisionSurfaceV1, SemanticActionV1, SemanticChoiceV1};
use serde::{Deserialize, Serialize};

/// Cells identify fixed positions in one dealt board, not master-deck cards.
/// Canonical capture includes the hidden layout; decision views must filter it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NaturalMatchGameV1 {
    pub attempts_remaining: u8,
    pub first: Option<u8>,
    pub last_pair: Option<[u8; 2]>,
    pub cards: Vec<NaturalMatchCellV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NaturalMatchCellV1 {
    pub position: u8,
    pub card_id: String,
    pub upgrades: u8,
    pub matched: bool,
    pub revealed: bool,
}

impl NaturalMatchGameV1 {
    pub fn validate(&self, screen: i64) -> Result<(), String> {
        let valid = self.cards.len() == 12
            && self.cards.iter().enumerate().all(|(i, c)| {
                c.position as usize == i && !c.card_id.is_empty() && (!c.matched || c.revealed)
            })
            && self.attempts_remaining <= 5
            && self.first.is_none_or(|p| {
                p < 12 && !self.cards[p as usize].matched && self.cards[p as usize].revealed
            })
            && self.last_pair.is_none_or(|[a, b]| {
                a < 12
                    && b < 12
                    && a != b
                    && self.cards[a as usize].revealed
                    && self.cards[b as usize].revealed
            })
            && match screen {
                2 => self.attempts_remaining > 0,
                3 => self.attempts_remaining == 0 && self.first.is_none(),
                _ => false,
            };
        if valid {
            Ok(())
        } else {
            Err("invalid Match and Keep board at stable boundary".into())
        }
    }

    pub fn can_flip(&self, position: u8) -> bool {
        self.attempts_remaining > 0
            && self
                .cards
                .get(position as usize)
                .is_some_and(|card| !card.matched && self.first != Some(position))
    }
}

pub fn match_cell_label(position: u8) -> String {
    format!("R{}C{}", position / 4 + 1, position % 4 + 1)
}

pub fn natural_match_flip_v1(position: u8) -> Result<SemanticActionV1, String> {
    if position >= 12 {
        return Err("Match and Keep cell is outside the board".into());
    }
    Ok(SemanticActionV1::Select {
        surface: DecisionSurfaceV1::Event,
        choice: SemanticChoiceV1::new("match_card", match_cell_label(position), 0),
    })
}
