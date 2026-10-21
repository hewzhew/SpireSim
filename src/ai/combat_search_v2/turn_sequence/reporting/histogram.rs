use std::collections::BTreeMap;

use super::super::super::turn_sequence_effect::TurnSequenceDivergence;
use super::super::super::*;

pub(super) fn divergence_histogram(
    counts: BTreeMap<TurnSequenceDivergence, usize>,
) -> Vec<CombatSearchV2DiagnosticsTurnSequenceDivergenceCount> {
    let mut entries = counts
        .into_iter()
        .map(
            |(divergence, groups)| CombatSearchV2DiagnosticsTurnSequenceDivergenceCount {
                kind: divergence.kind,
                first_divergence_path: divergence.first_divergence_path,
                guessed_reveal_gate: divergence.guessed_reveal_gate,
                groups,
            },
        )
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .groups
            .cmp(&left.groups)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.first_divergence_path.cmp(&right.first_divergence_path))
    });
    entries
}
