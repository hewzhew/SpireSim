use super::discard_order_shadow_audit::{
    DiscardOrderShadowAuditCollector, DiscardOrderShadowAuditKey,
};
use super::turn_sequence_effect::effect_fingerprint;
use super::*;
use std::collections::BTreeMap;

mod reporting;
mod types;

pub(super) use types::TurnSequenceSummary;
use types::{TurnSequenceGroupAggregate, TurnSequenceGroupKey};

#[derive(Default)]
pub(super) struct TurnSequenceDiagnosticsCollector {
    states_observed: u64,
    non_empty_prefix_states: u64,
    grouped_prefix_states: u64,
    max_prefix_length: usize,
    max_legal_actions_after_prefix: usize,
    groups: BTreeMap<TurnSequenceGroupKey, TurnSequenceGroupAggregate>,
    discard_order_shadow_audit: DiscardOrderShadowAuditCollector,
}

pub(super) fn summarize_turn_sequence(
    node: &SearchNode,
    legal_actions: usize,
) -> TurnSequenceSummary {
    let prefix_length = node.turn_prefix.prefix_length();
    if prefix_length == 0 || !matches!(node.engine, EngineState::CombatPlayerTurn) {
        return TurnSequenceSummary {
            prefix_length,
            legal_actions,
            origin_key: None,
            ordered_key: None,
            unordered_key: None,
            effect_key: None,
            effect_fingerprint: None,
        };
    }

    let effect_fingerprint = effect_fingerprint(node, legal_actions);
    TurnSequenceSummary {
        prefix_length,
        legal_actions,
        origin_key: node.turn_prefix.origin_key().map(str::to_string),
        ordered_key: node.turn_prefix.ordered_sequence_key(),
        unordered_key: node.turn_prefix.unordered_sequence_key(),
        effect_key: Some(turn_sequence_effect::effect_key(&effect_fingerprint)),
        effect_fingerprint: Some(effect_fingerprint),
    }
}

impl TurnSequenceDiagnosticsCollector {
    #[cfg(test)]
    pub(super) fn observe(&mut self, summary: &TurnSequenceSummary) {
        self.observe_inner(summary, None);
    }

    pub(super) fn observe_with_node(&mut self, summary: &TurnSequenceSummary, node: &SearchNode) {
        self.observe_inner(summary, Some(node));
    }

    fn observe_inner(&mut self, summary: &TurnSequenceSummary, node: Option<&SearchNode>) {
        self.states_observed = self.states_observed.saturating_add(1);
        self.max_prefix_length = self.max_prefix_length.max(summary.prefix_length);
        if summary.prefix_length == 0 {
            return;
        }

        self.non_empty_prefix_states = self.non_empty_prefix_states.saturating_add(1);
        self.max_legal_actions_after_prefix = self
            .max_legal_actions_after_prefix
            .max(summary.legal_actions);

        let (Some(origin_key), Some(ordered_key), Some(unordered_key), Some(effect_key)) = (
            summary.origin_key.as_ref(),
            summary.ordered_key.as_ref(),
            summary.unordered_key.as_ref(),
            summary.effect_key.as_ref(),
        ) else {
            return;
        };

        self.grouped_prefix_states = self.grouped_prefix_states.saturating_add(1);
        let aggregate = self
            .groups
            .entry(TurnSequenceGroupKey {
                origin_key: origin_key.clone(),
                unordered_key: unordered_key.clone(),
            })
            .or_default();
        aggregate.states = aggregate.states.saturating_add(1);
        aggregate.max_prefix_length = aggregate.max_prefix_length.max(summary.prefix_length);
        aggregate.max_legal_actions = aggregate.max_legal_actions.max(summary.legal_actions);
        aggregate.ordered_variants.insert(ordered_key.clone());
        aggregate.effect_variants.insert(effect_key.clone());
        if let Some(effect_fingerprint) = summary.effect_fingerprint.as_ref() {
            aggregate.effect_components.observe(effect_fingerprint);
            if let Some(node) = node {
                self.discard_order_shadow_audit.observe_state(
                    DiscardOrderShadowAuditKey {
                        origin_key: origin_key.clone(),
                        unordered_key: unordered_key.clone(),
                    },
                    ordered_key,
                    effect_key,
                    effect_fingerprint,
                    node,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
