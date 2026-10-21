use std::collections::BTreeMap;

use super::super::super::turn_sequence_effect::TurnSequenceDivergence;
use super::super::super::*;
use super::super::TurnSequenceDiagnosticsCollector;
use super::histogram::divergence_histogram;

impl TurnSequenceDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsTurnSequence {
        let mut groups_with_order_variants = 0usize;
        let mut same_effect_order_variant_groups = 0usize;
        let mut order_sensitive_groups = 0usize;
        let mut max_ordered_variants_per_group = 0usize;
        let mut max_effect_variants_per_group = 0usize;
        let mut divergence_counts = BTreeMap::<TurnSequenceDivergence, usize>::new();

        for aggregate in self.groups.values() {
            let ordered = aggregate.ordered_variants.len();
            let effects = aggregate.effect_variants.len();
            max_ordered_variants_per_group = max_ordered_variants_per_group.max(ordered);
            max_effect_variants_per_group = max_effect_variants_per_group.max(effects);
            if ordered > 1 {
                groups_with_order_variants += 1;
                if effects == 1 {
                    same_effect_order_variant_groups += 1;
                } else if effects > 1 {
                    order_sensitive_groups += 1;
                    *divergence_counts
                        .entry(aggregate.effect_components.classify())
                        .or_default() += 1;
                }
            }
        }

        CombatSearchV2DiagnosticsTurnSequence {
            grouping_policy: "same_turn_origin_plus_unordered_prefix_tokens",
            behavioral_effect: "diagnostic_only_no_sequence_prune_no_commutation_claim",
            states_observed: self.states_observed,
            non_empty_prefix_states: self.non_empty_prefix_states,
            grouped_prefix_states: self.grouped_prefix_states,
            unordered_sequence_groups: self.groups.len(),
            groups_with_order_variants,
            same_effect_order_variant_groups,
            order_sensitive_groups,
            max_ordered_variants_per_group,
            max_effect_variants_per_group,
            max_prefix_length: self.max_prefix_length,
            max_legal_actions_after_prefix: self.max_legal_actions_after_prefix,
            order_sensitive_divergence_histogram: divergence_histogram(divergence_counts),
            discard_order_shadow_audit: self.discard_order_shadow_audit(),
            largest_groups: self.largest_group_samples(),
            notes: vec![
                "groups are scoped by the first action's turn-origin dominance hash",
                "unordered prefix tokens intentionally ignore action order for diagnostics only",
                "effect variants use typed diagnostic components plus dominance/resource fallback hashes",
                "same-effect groups are candidates for later simulator-backed commutation probes, not safe-pruning evidence",
                "order-sensitive group divergence is classifier guidance, not prune-safe abstraction",
                "large-choice pending decisions are not handled by this diagnostic",
            ],
        }
    }
}
