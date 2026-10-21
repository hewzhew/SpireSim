use std::collections::HashSet;

use sts_core::runtime::combat::CardLineageId;
use sts_core::sim::combat_action_surface::{
    CombatSelectionActionFamilyV2, CombatSelectionDomainCandidateV2,
    CombatSelectionInputEncodingV2, CombatSelectionStatusV2,
};
use sts_core::state::core::ClientInput;
use sts_core::state::selection::{SelectionResolution, SelectionScope};

use super::types::TurnOptionGenerationGapKind;

/// Lazily enumerates the exact concrete inputs represented by one structured
/// combat selection family.
///
/// Consumers should keep their own materialization limit. The cursor reports
/// the complete remaining count without requiring every input to be stored.
#[derive(Clone, Debug)]
pub struct SelectionTransactionCursor {
    encoding: CombatSelectionInputEncodingV2,
    candidates: Vec<SelectionCandidate>,
    min_len: usize,
    max_len: usize,
    next_indices: Option<Vec<usize>>,
    remaining_input_count: usize,
    excluded: Vec<Vec<usize>>,
}

#[derive(Clone, Copy, Debug)]
struct SelectionCandidate {
    payload: SelectionCandidatePayload,
    distinct_key: CardLineageId,
}

#[derive(Clone, Copy, Debug)]
enum SelectionCandidatePayload {
    Card(CardLineageId),
    ScryIndex(usize),
}

impl SelectionTransactionCursor {
    pub fn new(
        family: &CombatSelectionActionFamilyV2,
    ) -> Result<Self, TurnOptionGenerationGapKind> {
        if !matches!(family.selection_status, CombatSelectionStatusV2::Enabled) {
            return Err(TurnOptionGenerationGapKind::DisabledStructuredChoice);
        }

        let candidates = family
            .raw_domain
            .iter()
            .filter_map(|candidate| match (family.input_encoding, candidate) {
                (
                    CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages
                    | CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages,
                    CombatSelectionDomainCandidateV2::CardLineage {
                        lineage, eligible, ..
                    },
                ) if *eligible => Some(SelectionCandidate {
                    payload: SelectionCandidatePayload::Card(*lineage),
                    distinct_key: *lineage,
                }),
                (
                    CombatSelectionInputEncodingV2::SubmitScryDiscardIndices,
                    CombatSelectionDomainCandidateV2::ScryIndex {
                        index,
                        card_lineage: Some(card_lineage),
                        currently_present: true,
                        ..
                    },
                ) => usize::try_from(*index)
                    .ok()
                    .map(|payload| SelectionCandidate {
                        payload: SelectionCandidatePayload::ScryIndex(payload),
                        distinct_key: *card_lineage,
                    }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let distinct_count = candidates
            .iter()
            .map(|candidate| candidate.distinct_key)
            .collect::<HashSet<_>>()
            .len();
        let min_len = usize::try_from(family.declared_min).unwrap_or(usize::MAX);
        let max_len = usize::try_from(family.effective_max)
            .unwrap_or(usize::MAX)
            .min(distinct_count);
        let next_indices = (min_len <= max_len)
            .then(|| first_valid_indices(&candidates, min_len))
            .flatten();
        let remaining_input_count = next_indices.as_ref().map_or(0, |_| {
            ordered_distinct_sequence_count(distinct_count, min_len, max_len)
        });

        Ok(Self {
            encoding: family.input_encoding,
            candidates,
            min_len,
            max_len,
            next_indices,
            remaining_input_count,
            excluded: Vec::new(),
        })
    }

    pub fn next_input(&mut self) -> Option<ClientInput> {
        let input = self.peek_input()?;
        self.consume_current();
        Some(input)
    }

    pub(crate) fn peek_input(&self) -> Option<ClientInput> {
        self.next_indices
            .as_ref()
            .map(|indices| self.compile(indices))
    }

    pub(crate) fn consume_current(&mut self) {
        let Some(indices) = self.next_indices.as_ref() else {
            return;
        };
        self.next_indices =
            next_valid_indices(&self.candidates, indices, self.min_len, self.max_len);
        self.remaining_input_count = self.remaining_input_count.saturating_sub(1);
        self.skip_excluded();
    }

    /// Partitions a valid, still-unseen member into another bounded cursor.
    /// Selection order is part of its identity; the reverse stays independent.
    pub(crate) fn exclude_input(&mut self, input: &ClientInput) -> bool {
        let Some(indices) = self.member_indices(input) else {
            return false;
        };
        let Some(current) = self.next_indices.as_ref() else {
            return false;
        };
        if (indices.len(), &indices) < (current.len(), current) || self.excluded.contains(&indices)
        {
            return false;
        }
        self.excluded.push(indices);
        self.remaining_input_count -= 1;
        self.skip_excluded();
        true
    }

    fn skip_excluded(&mut self) {
        while let Some(indices) = self.next_indices.as_ref() {
            if !self.excluded.contains(indices) {
                break;
            }
            self.next_indices =
                next_valid_indices(&self.candidates, indices, self.min_len, self.max_len);
        }
    }

    fn member_indices(&self, input: &ClientInput) -> Option<Vec<usize>> {
        let len = match input {
            ClientInput::SubmitSelection(resolution) => resolution.selected.len(),
            ClientInput::SubmitScryDiscard(selected) => selected.len(),
            _ => return None,
        };
        if len < self.min_len || len > self.max_len {
            return None;
        }
        let indices = match (self.encoding, input) {
            (CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages, ClientInput::SubmitSelection(resolution))
                if resolution.scope == SelectionScope::Hand => self.card_indices(&resolution.selected_card_lineages()),
            (CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages, ClientInput::SubmitSelection(resolution))
                if resolution.scope == SelectionScope::Grid => self.card_indices(&resolution.selected_card_lineages()),
            (CombatSelectionInputEncodingV2::SubmitScryDiscardIndices, ClientInput::SubmitScryDiscard(selected)) =>
                selected.iter().map(|index| self.candidates.iter().position(|candidate| {
                    matches!(candidate.payload, SelectionCandidatePayload::ScryIndex(value) if value == *index)
                })).collect(),
            _ => None,
        }?;
        (indices.iter().enumerate().all(|(offset, index)| {
            distinct_key_is_available(&self.candidates, &indices[..offset], *index)
        }))
        .then_some(indices)
    }

    fn card_indices(&self, lineages: &[CardLineageId]) -> Option<Vec<usize>> {
        lineages
            .iter()
            .map(|lineage| {
                self.candidates.iter().position(|candidate| {
            matches!(candidate.payload, SelectionCandidatePayload::Card(value) if value == *lineage)
        })
            })
            .collect()
    }

    pub fn remaining_input_count(&self) -> usize {
        self.remaining_input_count
    }

    pub fn is_exhausted(&self) -> bool {
        self.next_indices.is_none()
    }

    fn compile(&self, indices: &[usize]) -> ClientInput {
        match self.encoding {
            CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages => {
                ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                    SelectionScope::Hand,
                    indices
                        .iter()
                        .filter_map(|index| match self.candidates[*index].payload {
                            SelectionCandidatePayload::Card(lineage) => Some(lineage),
                            SelectionCandidatePayload::ScryIndex(_) => None,
                        }),
                ))
            }
            CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages => {
                ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                    SelectionScope::Grid,
                    indices
                        .iter()
                        .filter_map(|index| match self.candidates[*index].payload {
                            SelectionCandidatePayload::Card(lineage) => Some(lineage),
                            SelectionCandidatePayload::ScryIndex(_) => None,
                        }),
                ))
            }
            CombatSelectionInputEncodingV2::SubmitScryDiscardIndices => {
                ClientInput::SubmitScryDiscard(
                    indices
                        .iter()
                        .filter_map(|index| match self.candidates[*index].payload {
                            SelectionCandidatePayload::ScryIndex(index) => Some(index),
                            SelectionCandidatePayload::Card(_) => None,
                        })
                        .collect(),
                )
            }
        }
    }
}

fn ordered_distinct_sequence_count(distinct_count: usize, min_len: usize, max_len: usize) -> usize {
    (min_len..=max_len.min(distinct_count)).fold(0usize, |total, len| {
        let permutations = (0..len).fold(1usize, |count, offset| {
            count.saturating_mul(distinct_count.saturating_sub(offset))
        });
        total.saturating_add(permutations)
    })
}

fn first_valid_indices(candidates: &[SelectionCandidate], len: usize) -> Option<Vec<usize>> {
    let mut indices = Vec::with_capacity(len);
    fill_smallest_suffix(candidates, &mut indices, len).then_some(indices)
}

fn next_valid_indices(
    candidates: &[SelectionCandidate],
    current: &[usize],
    min_len: usize,
    max_len: usize,
) -> Option<Vec<usize>> {
    if !current.is_empty() {
        for pivot in (0..current.len()).rev() {
            let mut prefix = current[..pivot].to_vec();
            for candidate_index in current[pivot].saturating_add(1)..candidates.len() {
                if distinct_key_is_available(candidates, &prefix, candidate_index) {
                    prefix.push(candidate_index);
                    if fill_smallest_suffix(candidates, &mut prefix, current.len()) {
                        return Some(prefix);
                    }
                    prefix.pop();
                }
            }
        }
    }

    let next_len = current.len().max(min_len).saturating_add(1);
    (next_len <= max_len)
        .then(|| first_valid_indices(candidates, next_len))
        .flatten()
}

fn fill_smallest_suffix(
    candidates: &[SelectionCandidate],
    indices: &mut Vec<usize>,
    target_len: usize,
) -> bool {
    while indices.len() < target_len {
        let Some(next) = (0..candidates.len())
            .find(|candidate| distinct_key_is_available(candidates, indices, *candidate))
        else {
            return false;
        };
        indices.push(next);
    }
    true
}

fn distinct_key_is_available(
    candidates: &[SelectionCandidate],
    selected_indices: &[usize],
    candidate_index: usize,
) -> bool {
    selected_indices.iter().all(|selected| {
        candidates[*selected].distinct_key != candidates[candidate_index].distinct_key
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::sim::combat_action_surface::{
        CombatSelectionDistinctByV2, CombatSelectionPayloadLanguageV2, CombatSelectionReasonV2,
    };
    use sts_core::state::core::HandSelectReason;

    #[test]
    fn ordered_cursor_emits_permutations_without_eager_materialization() {
        let family = CombatSelectionActionFamilyV2 {
            input_encoding: CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages,
            reason: CombatSelectionReasonV2::Hand(HandSelectReason::Discard),
            source_pile: None,
            raw_domain: [11, 22, 33]
                .into_iter()
                .enumerate()
                .map(
                    |(ordinal, recipe_slot)| CombatSelectionDomainCandidateV2::CardLineage {
                        ordinal: ordinal as u64,
                        lineage: sts_core::test_support::scenario_lineage(recipe_slot),
                        card_id: None,
                        upgrades: None,
                        eligible: true,
                    },
                )
                .collect(),
            raw_domain_count: 3,
            eligible_domain_count: 3,
            max_distinct_selection_count: 3,
            declared_min: 2,
            declared_max: 2,
            effective_max: 2,
            selection_status: CombatSelectionStatusV2::Enabled,
            payload_language: CombatSelectionPayloadLanguageV2::OrderedDistinctSequence(
                CombatSelectionDistinctByV2::CardLineage,
            ),
        };
        let mut cursor = SelectionTransactionCursor::new(&family).unwrap();
        assert_eq!(cursor.remaining_input_count(), 6);
        let inputs = std::iter::from_fn(|| cursor.next_input()).collect::<Vec<_>>();

        assert_eq!(inputs.len(), 6);
        assert_ne!(inputs[0], inputs[1]);
        assert!(cursor.is_exhausted());
        assert_eq!(cursor.remaining_input_count(), 0);
    }

    #[test]
    fn sparse_partition_preserves_every_other_order_and_rejects_wrong_members() {
        use sts_core::test_support::scenario_lineages;
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.zones.hand = [11, 22, 33]
            .into_iter()
            .map(|id| {
                sts_core::runtime::combat::CombatCard::for_scenario(
                    sts_core::content::cards::CardId::Wound,
                    id,
                )
            })
            .collect();
        let engine = sts_core::state::core::EngineState::PendingChoice(
            sts_core::state::core::PendingChoice::HandSelect {
                candidate_lineages: scenario_lineages([11, 22, 33]),
                min_cards: 2,
                max_cards: 2,
                can_cancel: false,
                reason: HandSelectReason::Discard,
            },
        );
        let surface =
            sts_core::sim::combat_action_surface::combat_legal_action_surface_v2(&engine, &combat);
        let family = &surface.selection_families[0];
        let input = |scope, ids: &[u32]| {
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                scope,
                scenario_lineages(ids.iter().copied()),
            ))
        };
        let mut cursor = SelectionTransactionCursor::new(family).unwrap();
        let all: Vec<_> = std::iter::from_fn(|| cursor.next_input()).collect();
        let mut cursor = SelectionTransactionCursor::new(family).unwrap();
        for invalid in [
            input(SelectionScope::Grid, &[11, 22]),
            input(SelectionScope::Hand, &[11, 11]),
            input(SelectionScope::Hand, &[11, 99]),
            input(SelectionScope::Hand, &[11]),
            ClientInput::EndTurn,
        ] {
            assert!(!cursor.exclude_input(&invalid));
        }
        let preferred = input(SelectionScope::Hand, &[33, 11]);
        assert!(cursor.exclude_input(&preferred));
        assert!(!cursor.exclude_input(&preferred));
        assert_eq!(cursor.remaining_input_count(), 5);
        let remaining: Vec<_> = std::iter::from_fn(|| cursor.next_input()).collect();
        assert_eq!(
            remaining,
            all.into_iter()
                .filter(|candidate| candidate != &preferred)
                .collect::<Vec<_>>()
        );
        assert!(remaining.contains(&input(SelectionScope::Hand, &[11, 33])));
        assert_eq!(cursor.remaining_input_count(), 0);
    }
}
