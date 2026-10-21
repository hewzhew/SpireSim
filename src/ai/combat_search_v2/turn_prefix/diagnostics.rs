use std::collections::BTreeMap;

use super::{TurnPrefixKind, TurnPrefixObservation, TurnPrefixSummary};
use crate::ai::combat_search_v2::types::{
    CombatSearchV2DiagnosticsTurnPrefix, CombatSearchV2DiagnosticsTurnPrefixFanoutSample,
    CombatSearchV2DiagnosticsTurnPrefixKindCount, CombatSearchV2DiagnosticsTurnPrefixLengthCount,
};

const LARGEST_PREFIX_FANOUT_SAMPLE_LIMIT: usize = 8;

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct TurnPrefixDiagnosticsCollector {
    states_observed: u64,
    non_empty_prefix_states: u64,
    empty_prefix_states: u64,
    total_prefix_length: u64,
    max_prefix_length: usize,
    total_cards_played_in_prefix: u64,
    total_potions_used_in_prefix: u64,
    total_potions_discarded_in_prefix: u64,
    total_other_actions_in_prefix: u64,
    max_legal_actions_after_non_empty_prefix: usize,
    prefix_length_counts: BTreeMap<usize, u64>,
    prefix_kind_counts: BTreeMap<TurnPrefixKind, MutableTurnPrefixKindCount>,
    largest_prefix_fanouts: Vec<TurnPrefixObservation>,
}

#[derive(Clone, Debug, Default)]
struct MutableTurnPrefixKindCount {
    states: u64,
    legal_actions_total: u64,
    max_prefix_length: usize,
}

impl TurnPrefixDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe(&mut self, summary: &TurnPrefixSummary) {
        self.states_observed = self.states_observed.saturating_add(1);
        let prefix_length = summary.prefix.prefix_length as usize;
        self.total_prefix_length = self
            .total_prefix_length
            .saturating_add(prefix_length as u64);
        self.max_prefix_length = self.max_prefix_length.max(prefix_length);
        if prefix_length > 0 {
            self.non_empty_prefix_states = self.non_empty_prefix_states.saturating_add(1);
            self.max_legal_actions_after_non_empty_prefix = self
                .max_legal_actions_after_non_empty_prefix
                .max(summary.legal_actions);
        } else {
            self.empty_prefix_states = self.empty_prefix_states.saturating_add(1);
        }

        self.total_cards_played_in_prefix = self
            .total_cards_played_in_prefix
            .saturating_add(summary.prefix.cards_played as u64);
        self.total_potions_used_in_prefix = self
            .total_potions_used_in_prefix
            .saturating_add(summary.prefix.potions_used as u64);
        self.total_potions_discarded_in_prefix = self
            .total_potions_discarded_in_prefix
            .saturating_add(summary.prefix.potions_discarded as u64);
        self.total_other_actions_in_prefix = self
            .total_other_actions_in_prefix
            .saturating_add(summary.prefix.other_actions as u64);

        *self.prefix_length_counts.entry(prefix_length).or_insert(0) += 1;
        let kind = summary.prefix.kind();
        let kind_count = self.prefix_kind_counts.entry(kind).or_default();
        kind_count.states = kind_count.states.saturating_add(1);
        kind_count.legal_actions_total = kind_count
            .legal_actions_total
            .saturating_add(summary.legal_actions as u64);
        kind_count.max_prefix_length = kind_count.max_prefix_length.max(prefix_length);

        self.remember_largest_prefix_fanout(summary);
    }

    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsTurnPrefix {
        CombatSearchV2DiagnosticsTurnPrefix {
            tracking_policy: "current_turn_prefix_summary_from_search_node",
            behavioral_effect: "diagnostic_only_no_turn_prefix_prune_no_merge",
            states_observed: self.states_observed,
            non_empty_prefix_states: self.non_empty_prefix_states,
            empty_prefix_states: self.empty_prefix_states,
            avg_prefix_length: rounded_ratio(self.total_prefix_length, self.states_observed),
            max_prefix_length: self.max_prefix_length,
            max_legal_actions_after_non_empty_prefix: self.max_legal_actions_after_non_empty_prefix,
            total_cards_played_in_prefix: self.total_cards_played_in_prefix,
            total_potions_used_in_prefix: self.total_potions_used_in_prefix,
            total_potions_discarded_in_prefix: self.total_potions_discarded_in_prefix,
            total_other_actions_in_prefix: self.total_other_actions_in_prefix,
            prefix_length_counts: self.prefix_length_count_reports(),
            prefix_kind_counts: self.prefix_kind_count_reports(),
            largest_prefix_fanouts: self.largest_prefix_fanout_reports(),
            notes: vec![
                "turn prefix state resets after a next-turn transition",
                "prefix signature previews are capped and diagnostic only",
                "turn prefix diagnostics do not merge different action orders",
                "future turn-local dominance must prove exact state and resource coverage",
            ],
        }
    }

    fn remember_largest_prefix_fanout(&mut self, summary: &TurnPrefixSummary) {
        if summary.prefix.prefix_length == 0 || summary.legal_actions <= 1 {
            return;
        }
        self.largest_prefix_fanouts.push(TurnPrefixObservation {
            observed_at_state_query: self.states_observed,
            prefix: summary.prefix.clone(),
            legal_actions: summary.legal_actions,
        });
        self.largest_prefix_fanouts.sort_by(|left, right| {
            right
                .legal_actions
                .cmp(&left.legal_actions)
                .then_with(|| right.prefix.prefix_length.cmp(&left.prefix.prefix_length))
                .then_with(|| {
                    left.observed_at_state_query
                        .cmp(&right.observed_at_state_query)
                })
        });
        self.largest_prefix_fanouts
            .truncate(LARGEST_PREFIX_FANOUT_SAMPLE_LIMIT);
    }

    fn prefix_length_count_reports(&self) -> Vec<CombatSearchV2DiagnosticsTurnPrefixLengthCount> {
        self.prefix_length_counts
            .iter()
            .map(
                |(prefix_length, states)| CombatSearchV2DiagnosticsTurnPrefixLengthCount {
                    prefix_length: *prefix_length,
                    states: *states,
                },
            )
            .collect()
    }

    fn prefix_kind_count_reports(&self) -> Vec<CombatSearchV2DiagnosticsTurnPrefixKindCount> {
        self.prefix_kind_counts
            .iter()
            .map(
                |(kind, count)| CombatSearchV2DiagnosticsTurnPrefixKindCount {
                    kind: kind.label().to_string(),
                    states: count.states,
                    legal_actions_total: count.legal_actions_total,
                    max_prefix_length: count.max_prefix_length,
                },
            )
            .collect()
    }

    fn largest_prefix_fanout_reports(
        &self,
    ) -> Vec<CombatSearchV2DiagnosticsTurnPrefixFanoutSample> {
        self.largest_prefix_fanouts
            .iter()
            .map(|sample| CombatSearchV2DiagnosticsTurnPrefixFanoutSample {
                observed_at_state_query: sample.observed_at_state_query,
                prefix_length: sample.prefix.prefix_length as usize,
                kind: sample.prefix.kind().label().to_string(),
                cards_played: sample.prefix.cards_played as usize,
                potions_used: sample.prefix.potions_used as usize,
                potions_discarded: sample.prefix.potions_discarded as usize,
                other_actions: sample.prefix.other_actions as usize,
                legal_actions: sample.legal_actions,
                signature_preview: sample.prefix.signature_preview.clone(),
                signature_truncated: sample.prefix.signature_truncated,
            })
            .collect()
    }
}

fn rounded_ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    let value = numerator as f64 / denominator as f64;
    (value * 100.0).round() / 100.0
}
