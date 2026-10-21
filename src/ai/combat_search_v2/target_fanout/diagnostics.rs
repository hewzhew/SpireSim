use std::collections::BTreeMap;

use super::super::{
    CombatSearchV2DiagnosticsTargetFanout, CombatSearchV2DiagnosticsTargetFanoutKindCount,
    CombatSearchV2DiagnosticsTargetFanoutSample,
};
use super::{TargetFanoutGroupSummary, TargetFanoutKind, TargetFanoutSummary};

const LARGEST_TARGET_FANOUT_SAMPLE_LIMIT: usize = 8;

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct TargetFanoutDiagnosticsCollector {
    states_observed: u64,
    targeted_actions_total: u64,
    target_fanout_groups_total: u64,
    multi_target_fanout_groups: u64,
    total_targets_in_groups: u64,
    max_targets_per_group: usize,
    lethal_target_groups: u64,
    unique_lethal_target_groups: u64,
    uniform_damage_groups: u64,
    max_target_hp_span: i32,
    kind_counts: BTreeMap<TargetFanoutKind, MutableTargetFanoutKindCount>,
    largest_target_fanouts: Vec<TargetFanoutGroupObservation>,
}

#[derive(Clone, Debug)]
struct TargetFanoutGroupObservation {
    observed_at_state_query: u64,
    group: TargetFanoutGroupSummary,
}

#[derive(Clone, Debug, Default)]
struct MutableTargetFanoutKindCount {
    groups: u64,
    actions: u64,
    multi_target_groups: u64,
    lethal_target_groups: u64,
}

impl TargetFanoutDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe(&mut self, summary: &TargetFanoutSummary) {
        self.states_observed = self.states_observed.saturating_add(1);
        self.targeted_actions_total = self
            .targeted_actions_total
            .saturating_add(summary.targeted_actions as u64);
        self.target_fanout_groups_total = self
            .target_fanout_groups_total
            .saturating_add(summary.groups.len() as u64);

        for group in &summary.groups {
            let target_count = group.target_count as u64;
            self.total_targets_in_groups =
                self.total_targets_in_groups.saturating_add(target_count);
            self.max_targets_per_group = self.max_targets_per_group.max(group.target_count);
            self.max_target_hp_span = self.max_target_hp_span.max(group.target_hp_span());

            let kind_count = self.kind_counts.entry(group.kind).or_default();
            kind_count.groups = kind_count.groups.saturating_add(1);
            kind_count.actions = kind_count.actions.saturating_add(target_count);

            if group.target_count > 1 {
                self.multi_target_fanout_groups = self.multi_target_fanout_groups.saturating_add(1);
                kind_count.multi_target_groups = kind_count.multi_target_groups.saturating_add(1);
            }
            if group.lethal_targets > 0 {
                self.lethal_target_groups = self.lethal_target_groups.saturating_add(1);
                kind_count.lethal_target_groups = kind_count.lethal_target_groups.saturating_add(1);
            }
            if group.lethal_targets == 1 {
                self.unique_lethal_target_groups =
                    self.unique_lethal_target_groups.saturating_add(1);
            }
            if group.min_damage_hint == group.max_damage_hint {
                self.uniform_damage_groups = self.uniform_damage_groups.saturating_add(1);
            }
            self.remember_largest_target_fanout(group);
        }
    }

    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsTargetFanout {
        CombatSearchV2DiagnosticsTargetFanout {
            grouping_policy: "targeted_card_and_potion_actions_grouped_by_source",
            behavioral_effect: "diagnostic_only_no_target_prune_no_merge",
            states_observed: self.states_observed,
            targeted_actions_total: self.targeted_actions_total,
            target_fanout_groups_total: self.target_fanout_groups_total,
            multi_target_fanout_groups: self.multi_target_fanout_groups,
            avg_targets_per_group: rounded_ratio(
                self.total_targets_in_groups,
                self.target_fanout_groups_total,
            ),
            max_targets_per_group: self.max_targets_per_group,
            lethal_target_groups: self.lethal_target_groups,
            unique_lethal_target_groups: self.unique_lethal_target_groups,
            uniform_damage_groups: self.uniform_damage_groups,
            max_target_hp_span: self.max_target_hp_span,
            group_kind_counts: self.group_kind_counts(),
            largest_target_fanouts: self.largest_target_fanout_samples(),
            notes: vec![
                "target fanout groups are not equivalence classes",
                "damage hints are visible ordering diagnostics, not exact simulator damage facts",
                "non-damage targeted potions are counted with zero damage hint",
                "future target pruning must prove monster-specific safety before removing branches",
            ],
        }
    }

    fn remember_largest_target_fanout(&mut self, group: &TargetFanoutGroupSummary) {
        if group.target_count <= 1 {
            return;
        }
        self.largest_target_fanouts
            .push(TargetFanoutGroupObservation {
                observed_at_state_query: self.states_observed,
                group: group.clone(),
            });
        self.largest_target_fanouts.sort_by(|left, right| {
            right
                .group
                .target_count
                .cmp(&left.group.target_count)
                .then_with(|| right.group.lethal_targets.cmp(&left.group.lethal_targets))
                .then_with(|| {
                    right
                        .group
                        .target_hp_span()
                        .cmp(&left.group.target_hp_span())
                })
                .then_with(|| {
                    left.observed_at_state_query
                        .cmp(&right.observed_at_state_query)
                })
        });
        self.largest_target_fanouts
            .truncate(LARGEST_TARGET_FANOUT_SAMPLE_LIMIT);
    }

    fn group_kind_counts(&self) -> Vec<CombatSearchV2DiagnosticsTargetFanoutKindCount> {
        self.kind_counts
            .iter()
            .map(
                |(kind, count)| CombatSearchV2DiagnosticsTargetFanoutKindCount {
                    kind: kind.label().to_string(),
                    groups: count.groups,
                    actions: count.actions,
                    multi_target_groups: count.multi_target_groups,
                    lethal_target_groups: count.lethal_target_groups,
                },
            )
            .collect()
    }

    fn largest_target_fanout_samples(&self) -> Vec<CombatSearchV2DiagnosticsTargetFanoutSample> {
        self.largest_target_fanouts
            .iter()
            .map(|observation| {
                let group = &observation.group;
                CombatSearchV2DiagnosticsTargetFanoutSample {
                    observed_at_state_query: observation.observed_at_state_query,
                    kind: group.kind.label().to_string(),
                    source_key: group.source_key.clone(),
                    target_count: group.target_count,
                    lethal_targets: group.lethal_targets,
                    min_target_hp_with_block: group.min_target_hp_with_block,
                    max_target_hp_with_block: group.max_target_hp_with_block,
                    target_hp_span: group.target_hp_span(),
                    min_damage_hint: group.min_damage_hint,
                    max_damage_hint: group.max_damage_hint,
                    first_action_key: group.first_action_key.clone(),
                }
            })
            .collect()
    }
}

impl TargetFanoutKind {
    fn label(self) -> &'static str {
        match self {
            TargetFanoutKind::PlayCard => "play_card",
            TargetFanoutKind::UsePotion => "use_potion",
        }
    }
}

fn rounded_ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    let value = numerator as f64 / denominator as f64;
    (value * 100.0).round() / 100.0
}
