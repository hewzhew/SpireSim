use std::collections::BTreeSet;

use super::super::super::discard_order_shadow_audit::{
    is_static_discard_order_candidate, summarize_discard_order_shadow_audit,
    DiscardOrderShadowAuditKey, DiscardOrderShadowAuditObservation,
};
use super::super::super::*;
use super::super::TurnSequenceDiagnosticsCollector;
use super::samples::preview;

impl TurnSequenceDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn run_discard_order_exact_shadow_audit(
        &mut self,
        stepper: &impl CombatStepper,
        config: &CombatSearchV2Config,
        plugins: &CombatSearchPluginStack,
    ) {
        let candidate_keys = self.discard_order_shadow_audit_candidate_keys();
        self.discard_order_shadow_audit
            .run_one_step_exact_shadow_audit(stepper, config, plugins, &candidate_keys);
    }

    pub(super) fn discard_order_shadow_audit(
        &self,
    ) -> CombatSearchV2DiagnosticsDiscardOrderShadowAudit {
        let observations = self
            .groups
            .iter()
            .filter_map(|(key, aggregate)| {
                if aggregate.ordered_variants.len() <= 1 || aggregate.effect_variants.len() <= 1 {
                    return None;
                }

                let divergence = aggregate.effect_components.classify();
                if !is_static_discard_order_candidate(
                    divergence.kind,
                    divergence.first_divergence_path,
                    divergence.guessed_reveal_gate,
                ) {
                    return None;
                }

                Some(DiscardOrderShadowAuditObservation {
                    origin_key: key.origin_key.clone(),
                    unordered_key_preview: preview(&key.unordered_key),
                    states: aggregate.states,
                    max_prefix_length: aggregate.max_prefix_length,
                    ordered_variants: aggregate.ordered_variants.len(),
                    effect_variants: aggregate.effect_variants.len(),
                    max_legal_actions: aggregate.max_legal_actions,
                    first_divergence_path: divergence.first_divergence_path,
                    reveal_gate: divergence.guessed_reveal_gate,
                })
            })
            .collect();
        summarize_discard_order_shadow_audit(observations, &self.discard_order_shadow_audit)
    }

    fn discard_order_shadow_audit_candidate_keys(&self) -> BTreeSet<DiscardOrderShadowAuditKey> {
        self.groups
            .iter()
            .filter_map(|(key, aggregate)| {
                if aggregate.ordered_variants.len() <= 1 || aggregate.effect_variants.len() <= 1 {
                    return None;
                }

                let divergence = aggregate.effect_components.classify();
                if !is_static_discard_order_candidate(
                    divergence.kind,
                    divergence.first_divergence_path,
                    divergence.guessed_reveal_gate,
                ) {
                    return None;
                }

                Some(DiscardOrderShadowAuditKey {
                    origin_key: key.origin_key.clone(),
                    unordered_key: key.unordered_key.clone(),
                })
            })
            .collect()
    }
}
