use std::collections::BTreeMap;

use super::types::StateAbstractionHistogramEntry;

pub(super) fn histogram(
    keys: impl Iterator<Item = &'static str>,
) -> Vec<StateAbstractionHistogramEntry> {
    let mut counts = BTreeMap::<&'static str, usize>::new();
    for key in keys {
        *counts.entry(key).or_default() += 1;
    }
    histogram_entries(counts)
}

pub(super) fn group_histogram(
    entries: impl Iterator<Item = (&'static str, usize)>,
) -> Vec<StateAbstractionHistogramEntry> {
    let mut counts = BTreeMap::<&'static str, usize>::new();
    for (key, groups) in entries {
        *counts.entry(key).or_default() += groups;
    }
    histogram_entries(counts)
}

fn histogram_entries(counts: BTreeMap<&'static str, usize>) -> Vec<StateAbstractionHistogramEntry> {
    let mut entries = counts
        .into_iter()
        .map(|(key, cases)| StateAbstractionHistogramEntry { key, cases })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .cases
            .cmp(&left.cases)
            .then_with(|| left.key.cmp(right.key))
    });
    entries
}
