use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::runtime::combat::CardLineageId;
use crate::state::selection::SelectionTargetRef;

/// Resumable enumeration of unordered run-level selections.
///
/// The exact engine still receives one complete atomic selection, but the
/// oracle frontier never materializes the whole `n choose k` action family.
/// A small policy-provided prefix may be emitted first; the remaining exact
/// combinations are then visited once in deterministic lexicographic order.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LazyUnorderedSelectionCursorV1 {
    targets: Vec<SelectionTargetRef>,
    min_count: usize,
    max_count: usize,
    preferred: VecDeque<Vec<SelectionTargetRef>>,
    preferred_keys: BTreeSet<Vec<CardLineageId>>,
    next_count: usize,
    next_indices: Option<Vec<usize>>,
    total_count: u64,
    emitted_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LazyUnorderedSelectionMemberV1 {
    pub selected: Vec<SelectionTargetRef>,
    pub ordinal: u64,
}

impl LazyUnorderedSelectionCursorV1 {
    pub(super) fn new(
        targets: Vec<SelectionTargetRef>,
        min_count: usize,
        max_count: usize,
        preferred: impl IntoIterator<Item = Vec<SelectionTargetRef>>,
    ) -> Result<Self, String> {
        let max_count = max_count.min(targets.len());
        if min_count > max_count {
            return Err(format!(
                "selection requires at least {min_count} targets but only {} are available",
                targets.len()
            ));
        }
        let target_ordinals = targets
            .iter()
            .enumerate()
            .map(|(index, target)| (target.card_lineage(), index))
            .collect::<std::collections::BTreeMap<_, _>>();
        if target_ordinals.len() != targets.len() {
            return Err("run-level selection domain contains duplicate card lineages".to_string());
        }

        let mut preferred_keys = BTreeSet::new();
        let mut preferred_members = VecDeque::new();
        for selected in preferred {
            if selected.len() < min_count || selected.len() > max_count {
                continue;
            }
            let mut indexed = selected
                .into_iter()
                .map(|target| {
                    target_ordinals
                        .get(&target.card_lineage())
                        .copied()
                        .map(|index| (index, target))
                })
                .collect::<Option<Vec<_>>>();
            let Some(ref mut indexed) = indexed else {
                continue;
            };
            indexed.sort_by_key(|(index, _)| *index);
            if indexed.windows(2).any(|window| window[0].0 == window[1].0) {
                continue;
            }
            let canonical = indexed
                .iter()
                .map(|(_, target)| *target)
                .collect::<Vec<_>>();
            let key = selection_key(&canonical);
            if preferred_keys.insert(key) {
                preferred_members.push_back(canonical);
            }
        }

        let total_count = (min_count..=max_count).fold(0u64, |total, count| {
            total.saturating_add(binomial_saturating(targets.len(), count))
        });
        let next_count = min_count;
        let next_indices = first_indices(targets.len(), next_count);

        Ok(Self {
            targets,
            min_count,
            max_count,
            preferred: preferred_members,
            preferred_keys,
            next_count,
            next_indices,
            total_count,
            emitted_count: 0,
        })
    }

    pub(super) fn total_count(&self) -> u64 {
        self.total_count
    }

    pub(super) fn emitted_count(&self) -> u64 {
        self.emitted_count
    }

    #[cfg(test)]
    pub(super) fn remaining_count(&self) -> u64 {
        self.total_count.saturating_sub(self.emitted_count)
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.preferred.is_empty() && self.next_indices.is_none()
    }

    pub(super) fn next_member(&mut self) -> Option<LazyUnorderedSelectionMemberV1> {
        if let Some(selected) = self.preferred.pop_front() {
            return Some(self.emit(selected));
        }

        while let Some(indices) = self.next_indices.clone() {
            self.advance_lexicographic(&indices);
            let selected = indices
                .iter()
                .map(|index| self.targets[*index])
                .collect::<Vec<_>>();
            if self.preferred_keys.contains(&selection_key(&selected)) {
                continue;
            }
            return Some(self.emit(selected));
        }
        None
    }

    fn emit(&mut self, selected: Vec<SelectionTargetRef>) -> LazyUnorderedSelectionMemberV1 {
        let ordinal = self.emitted_count;
        self.emitted_count = self.emitted_count.saturating_add(1);
        LazyUnorderedSelectionMemberV1 { selected, ordinal }
    }

    fn advance_lexicographic(&mut self, current: &[usize]) {
        if let Some(next) = next_indices(self.targets.len(), current) {
            self.next_indices = Some(next);
            return;
        }
        self.next_count = self.next_count.saturating_add(1);
        self.next_indices = if self.next_count <= self.max_count {
            first_indices(self.targets.len(), self.next_count)
        } else {
            None
        };
    }
}

fn selection_key(selected: &[SelectionTargetRef]) -> Vec<CardLineageId> {
    selected
        .iter()
        .map(|target| target.card_lineage())
        .collect()
}

fn first_indices(domain_len: usize, count: usize) -> Option<Vec<usize>> {
    (count <= domain_len).then(|| (0..count).collect())
}

fn next_indices(domain_len: usize, current: &[usize]) -> Option<Vec<usize>> {
    if current.is_empty() {
        return None;
    }
    let mut next = current.to_vec();
    for pivot in (0..next.len()).rev() {
        let max_at_pivot = domain_len.saturating_sub(next.len() - pivot);
        if next[pivot] < max_at_pivot {
            next[pivot] += 1;
            for index in pivot + 1..next.len() {
                next[index] = next[index - 1] + 1;
            }
            return Some(next);
        }
    }
    None
}

fn binomial_saturating(n: usize, k: usize) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut value = 1u128;
    for index in 0..k {
        value = value
            .saturating_mul((n - index) as u128)
            .checked_div((index + 1) as u128)
            .unwrap_or(u128::MAX);
        if value >= u128::from(u64::MAX) {
            return u64::MAX;
        }
    }
    value as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(count: u32) -> Vec<SelectionTargetRef> {
        (0..count)
            .map(crate::test_support::scenario_lineage)
            .map(SelectionTargetRef::CardLineage)
            .collect()
    }

    #[test]
    fn large_choose_three_domain_is_constant_frontier_memory_and_resumable() {
        const DOMAIN_COUNT: u32 = 50;
        let preferred = vec![vec![
            SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(47),
            ),
            SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(48),
            ),
            SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(49),
            ),
        ]];
        let mut cursor =
            LazyUnorderedSelectionCursorV1::new(targets(DOMAIN_COUNT), 3, 3, preferred.clone())
                .unwrap();

        assert_eq!(cursor.total_count(), 19_600);
        assert_eq!(cursor.next_member().unwrap().selected, preferred[0]);
        assert_eq!(cursor.emitted_count(), 1);
        assert_eq!(cursor.remaining_count(), 19_599);

        let encoded = serde_json::to_vec(&cursor).unwrap();
        assert!(
            encoded.len() < DOMAIN_COUNT as usize * 64,
            "cursor checkpoint must stay proportional to the domain, not 19,600 combinations; encoded_bytes={}",
            encoded.len()
        );
        let mut restored: LazyUnorderedSelectionCursorV1 =
            serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            restored.next_member().unwrap().selected,
            vec![
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(0)
                ),
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(1)
                ),
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(2)
                )
            ]
        );
    }

    #[test]
    fn preferred_member_is_not_repeated_by_lexicographic_fallback() {
        let preferred = vec![vec![
            SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(0),
            ),
            SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(2),
            ),
        ]];
        let mut cursor =
            LazyUnorderedSelectionCursorV1::new(targets(4), 2, 2, preferred.clone()).unwrap();
        let members = std::iter::from_fn(|| cursor.next_member())
            .map(|member| member.selected)
            .collect::<Vec<_>>();

        assert_eq!(members.len(), 6);
        assert_eq!(members[0], preferred[0]);
        assert_eq!(
            members
                .iter()
                .filter(|selected| **selected == preferred[0])
                .count(),
            1
        );
        assert!(cursor.is_exhausted());
    }

    #[test]
    fn optional_empty_selection_is_emitted_once() {
        let mut cursor = LazyUnorderedSelectionCursorV1::new(targets(3), 0, 2, Vec::new()).unwrap();
        assert_eq!(cursor.total_count(), 7);
        assert_eq!(cursor.next_member().unwrap().selected, Vec::new());
        assert_eq!(std::iter::from_fn(|| cursor.next_member()).count(), 6);
    }
}
