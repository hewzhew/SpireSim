use super::super::*;
use super::types::{ActionExpansionDiagnosticsCollector, ActionExpansionGroupObservation};

const LARGEST_GROUP_SAMPLE_LIMIT: usize = 8;

impl ActionExpansionDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsExpansion {
        CombatSearchV2DiagnosticsExpansion {
            grouping_policy: "typed_fanout_groups_with_no_action_merge",
            behavioral_effect: "diagnostic_only_search_expansion_unchanged",
            states_observed: self.states_observed,
            total_atomic_actions: self.total_atomic_actions,
            total_fanout_groups: self.total_fanout_groups,
            fanout_groups_avg: rounded_ratio(self.total_fanout_groups, self.states_observed),
            fanout_groups_max: self.fanout_groups_max,
            max_group_size: self.max_group_size,
            action_kind_counts: self.action_kind_counts(),
            largest_groups: self.largest_group_samples(),
            notes: vec![
                "fanout groups explain where branching comes from; they are not equivalence classes",
                "largest_groups only includes groups with more than one atomic action",
                "targeted card and potion actions stay atomic and order-sensitive in the search",
                "future compression must prove safety before using these groups for pruning",
            ],
        }
    }

    pub(super) fn remember_largest_group(&mut self, observation: ActionExpansionGroupObservation) {
        if observation.action_count <= 1 {
            return;
        }
        self.largest_groups.push(observation);
        self.largest_groups.sort_by(|left, right| {
            right
                .action_count
                .cmp(&left.action_count)
                .then_with(|| left.key.kind.cmp(&right.key.kind))
                .then_with(|| left.key.signature.cmp(&right.key.signature))
                .then_with(|| {
                    left.observed_at_state_query
                        .cmp(&right.observed_at_state_query)
                })
        });
        self.largest_groups.truncate(LARGEST_GROUP_SAMPLE_LIMIT);
    }

    fn action_kind_counts(&self) -> Vec<CombatSearchV2DiagnosticsActionKindCount> {
        self.kind_counts
            .iter()
            .map(|(kind, count)| CombatSearchV2DiagnosticsActionKindCount {
                kind: kind.label().to_string(),
                atomic_actions: count.atomic_actions,
                fanout_groups: count.fanout_groups,
                max_group_size: count.max_group_size,
            })
            .collect()
    }

    fn largest_group_samples(&self) -> Vec<CombatSearchV2DiagnosticsActionGroupSample> {
        self.largest_groups
            .iter()
            .map(|group| CombatSearchV2DiagnosticsActionGroupSample {
                observed_at_state_query: group.observed_at_state_query,
                kind: group.key.kind.label().to_string(),
                group_key: group.key.signature.clone(),
                atomic_actions: group.action_count,
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
