//! Partial order knowledge derived from observations, never from a hidden pile.

use serde::Serialize;

use super::combat::ObservationEvidenceKindV1;
use super::state::{public_combat_state_v1, PublicCombatStateV1};
use crate::runtime::combat::{DrawCardKind, DrawPileChange};
use crate::sim::combat::CombatObservedStepResultV1;

/// Facts about card kinds. Identical kinds are not private instance identities.
/// `ordered` is a subsequence, not a claim that its cards are adjacent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DrawOrderKnowledge {
    composition: Vec<DrawCardKind>,
    ordered: Vec<DrawCardKind>,
    prefix_len: usize,
    suffix_len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrawKnowledgeError {
    UnsettledTransition,
    InconsistentObservation,
}

impl DrawOrderKnowledge {
    pub fn from_observation(observation: &PublicCombatStateV1) -> Self {
        let cards = observed_kinds(observation);
        let ordered =
            observation.cards.draw.evidence == ObservationEvidenceKindV1::PublicOrderedCollection;
        let mut knowledge = Self {
            composition: sorted(cards.clone()),
            ordered: if ordered { cards } else { Vec::new() },
            prefix_len: 0,
            suffix_len: 0,
        };
        knowledge.normalize();
        knowledge
    }

    pub fn composition(&self) -> &[DrawCardKind] {
        &self.composition
    }
    pub fn known_prefix(&self) -> &[DrawCardKind] {
        &self.ordered[..self.prefix_len]
    }
    pub fn known_suffix(&self) -> &[DrawCardKind] {
        &self.ordered[self.ordered.len() - self.suffix_len..]
    }
    pub fn ordered_subsequence(&self) -> &[DrawCardKind] {
        &self.ordered
    }

    /// For validating hypothetical futures against already-acquired knowledge.
    /// A private candidate's match result is not a new observation for the actor.
    pub fn permits_kind_order(&self, order: &[DrawCardKind]) -> bool {
        if sorted(order.to_vec()) != self.composition
            || !order.starts_with(self.known_prefix())
            || !order.ends_with(self.known_suffix())
        {
            return false;
        }
        let mut matched = 0;
        for card in order {
            if self.ordered.get(matched) == Some(card) {
                matched += 1;
            }
        }
        matched == self.ordered.len()
    }

    pub fn after_observed_step(
        &self,
        step: &CombatObservedStepResultV1,
    ) -> Result<Self, DrawKnowledgeError> {
        if step.step.truncated || step.step.timed_out {
            return Err(DrawKnowledgeError::UnsettledTransition);
        }
        self.after_changes(
            &step.draw_pile_changes,
            &public_combat_state_v1(&step.step.position.combat),
        )
    }

    /// The caller owns provenance: changes must come from the actual transition
    /// following this knowledge, not a receipt transplanted from another branch.
    pub fn after_changes(
        &self,
        changes: &[DrawPileChange],
        observation: &PublicCombatStateV1,
    ) -> Result<Self, DrawKnowledgeError> {
        use DrawPileChange::*;
        let mut next = self.clone();
        let mut count = self.composition.len();
        for change in changes {
            match *change {
                PutTop { card } => {
                    next.ordered.insert(0, card);
                    next.prefix_len += 1;
                    count += 1;
                }
                PutBottom { card } => {
                    next.ordered.push(card);
                    next.suffix_len += 1;
                    count += 1;
                }
                InsertUnpositioned { .. } => {
                    count += 1;
                    next.prefix_len = 0;
                    next.suffix_len = 0;
                }
                TakeTop { card } => {
                    count = count
                        .checked_sub(1)
                        .ok_or(DrawKnowledgeError::InconsistentObservation)?;
                    if next.prefix_len > 0 && next.ordered.first() != Some(&card) {
                        return Err(DrawKnowledgeError::InconsistentObservation);
                    }
                    // With an unknown prefix this may discard more knowledge
                    // than necessary, but never identifies a hidden occurrence.
                    if next.ordered.first() == Some(&card) {
                        next.ordered.remove(0);
                    }
                    next.prefix_len = next.prefix_len.saturating_sub(1);
                    next.suffix_len = next.suffix_len.min(next.ordered.len());
                }
                RemoveUnpositioned { card } => {
                    count = count
                        .checked_sub(1)
                        .ok_or(DrawKnowledgeError::InconsistentObservation)?;
                    // Which equal-looking occurrence left is not observable.
                    next.ordered.retain(|known| *known != card);
                    next.prefix_len = 0;
                    next.suffix_len = 0;
                }
                PrependShuffled { count: added } => {
                    count += added;
                    if added != 0 {
                        next.prefix_len = 0;
                    }
                }
                OrderUnknown { count: remaining } => {
                    count = remaining;
                    next.ordered.clear();
                    next.prefix_len = 0;
                    next.suffix_len = 0;
                }
            }
            if next.ordered.len() == count {
                next.prefix_len = count;
                next.suffix_len = count;
            }
        }

        let cards = observed_kinds(observation);
        if count != cards.len() || next.ordered.len() > count {
            return Err(DrawKnowledgeError::InconsistentObservation);
        }
        next.composition = sorted(cards.clone());
        let mut available = next.composition.clone();
        for known in &next.ordered {
            let Some(index) = available.iter().position(|card| card == known) else {
                return Err(DrawKnowledgeError::InconsistentObservation);
            };
            available.remove(index);
        }
        if observation.cards.draw.evidence == ObservationEvidenceKindV1::PublicOrderedCollection {
            if !next.permits_kind_order(&cards) {
                return Err(DrawKnowledgeError::InconsistentObservation);
            }
            next.ordered = cards;
        }
        next.normalize();
        Ok(next)
    }

    fn normalize(&mut self) {
        // A one-kind pile has a known kind order even without Frozen Eye.
        if self.composition.windows(2).all(|pair| pair[0] == pair[1]) {
            self.ordered = self.composition.clone();
        }
        if self.ordered.len() == self.composition.len() {
            self.prefix_len = self.ordered.len();
            self.suffix_len = self.ordered.len();
        }
    }
}

fn observed_kinds(observation: &PublicCombatStateV1) -> Vec<DrawCardKind> {
    observation
        .cards
        .draw
        .cards
        .iter()
        .map(|card| DrawCardKind {
            card_id: card.card_id,
            upgrades: card.upgrades,
        })
        .collect()
}

fn sorted(mut cards: Vec<DrawCardKind>) -> Vec<DrawCardKind> {
    cards.sort_by_key(|card| (card.card_id.stable_ordinal(), card.upgrades));
    cards
}

#[cfg(test)]
mod tests;
