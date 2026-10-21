use super::super::super::*;
use super::super::types::TurnSequenceGroupAggregate;
use super::super::TurnSequenceDiagnosticsCollector;

const LARGEST_SEQUENCE_GROUP_SAMPLE_LIMIT: usize = 8;
const PREVIEW_LIMIT: usize = 180;

impl TurnSequenceDiagnosticsCollector {
    pub(super) fn largest_group_samples(
        &self,
    ) -> Vec<CombatSearchV2DiagnosticsTurnSequenceGroupSample> {
        let mut samples = self
            .groups
            .iter()
            .filter(|(_, aggregate)| aggregate.ordered_variants.len() > 1 || aggregate.states > 1)
            .collect::<Vec<_>>();
        samples.sort_by(|(left_key, left), (right_key, right)| {
            right
                .effect_variants
                .len()
                .cmp(&left.effect_variants.len())
                .then_with(|| {
                    right
                        .ordered_variants
                        .len()
                        .cmp(&left.ordered_variants.len())
                })
                .then_with(|| right.states.cmp(&left.states))
                .then_with(|| left_key.origin_key.cmp(&right_key.origin_key))
                .then_with(|| left_key.unordered_key.cmp(&right_key.unordered_key))
        });
        samples
            .into_iter()
            .take(LARGEST_SEQUENCE_GROUP_SAMPLE_LIMIT)
            .map(|(key, aggregate)| {
                let divergence = aggregate.effect_components.classify();
                CombatSearchV2DiagnosticsTurnSequenceGroupSample {
                    group_class: group_class(aggregate).to_string(),
                    origin_key: key.origin_key.clone(),
                    unordered_key_preview: preview(&key.unordered_key),
                    states: aggregate.states,
                    max_prefix_length: aggregate.max_prefix_length,
                    ordered_variants: aggregate.ordered_variants.len(),
                    effect_variants: aggregate.effect_variants.len(),
                    max_legal_actions: aggregate.max_legal_actions,
                    divergence_kind: divergence.kind,
                    first_divergence_path: divergence.first_divergence_path,
                    guessed_reveal_gate: divergence.guessed_reveal_gate,
                    ordered_samples: aggregate
                        .ordered_variants
                        .iter()
                        .take(3)
                        .map(|ordered| preview(ordered))
                        .collect(),
                }
            })
            .collect()
    }
}

fn group_class(aggregate: &TurnSequenceGroupAggregate) -> &'static str {
    match (
        aggregate.ordered_variants.len() > 1,
        aggregate.effect_variants.len() > 1,
    ) {
        (true, true) => "order_sensitive_observed",
        (true, false) => "same_effect_order_variants",
        (false, true) => "same_order_effect_variants",
        (false, false) => "single_order_observed",
    }
}

pub(super) fn preview(value: &str) -> String {
    if value.len() <= PREVIEW_LIMIT {
        value.to_string()
    } else {
        format!("{}...", &value[..PREVIEW_LIMIT])
    }
}
