use std::collections::BTreeMap;

use super::super::state_abstraction::{boundary_spec, StateAbstractionBoundaryId};
use super::super::{
    CombatSearchV2DiagnosticsEquivalence, CombatSearchV2DiagnosticsEquivalenceGroupSample,
    CombatSearchV2DiagnosticsEquivalenceKindCount,
};
use super::{
    ActionEquivalenceGroupSummary, ActionEquivalenceKey, ActionEquivalenceKind,
    ActionEquivalenceSummary,
};

const LARGEST_EQUIVALENCE_GROUP_SAMPLE_LIMIT: usize = 8;

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct ActionEquivalenceDiagnosticsCollector {
    states_observed: u64,
    states_compressed: u64,
    atomic_actions_in: u64,
    representative_actions_out: u64,
    actions_removed: u64,
    max_group_size: usize,
    kind_counts: BTreeMap<ActionEquivalenceKind, MutableEquivalenceKindCount>,
    largest_groups: Vec<ActionEquivalenceObservation>,
}

#[derive(Clone, Debug, Default)]
struct MutableEquivalenceKindCount {
    groups: u64,
    actions_in: u64,
    actions_removed: u64,
    max_group_size: usize,
}

#[derive(Clone, Debug)]
struct ActionEquivalenceObservation {
    observed_at_state_query: u64,
    key: ActionEquivalenceKey,
    representative_original_action_id: usize,
    removed_original_action_ids: Vec<usize>,
}

impl ActionEquivalenceDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe(&mut self, summary: &ActionEquivalenceSummary) {
        self.states_observed = self.states_observed.saturating_add(1);
        self.atomic_actions_in = self
            .atomic_actions_in
            .saturating_add(summary.atomic_actions_in as u64);
        self.representative_actions_out = self
            .representative_actions_out
            .saturating_add(summary.representative_actions_out as u64);
        let actions_removed = summary.actions_removed();
        self.actions_removed = self.actions_removed.saturating_add(actions_removed as u64);
        if actions_removed > 0 {
            self.states_compressed = self.states_compressed.saturating_add(1);
        }

        for group in &summary.groups {
            let group_size = group.group_size();
            self.max_group_size = self.max_group_size.max(group_size);
            let count = self.kind_counts.entry(group.key.kind).or_default();
            count.groups = count.groups.saturating_add(1);
            count.actions_in = count.actions_in.saturating_add(group_size as u64);
            count.actions_removed = count
                .actions_removed
                .saturating_add(group.removed_original_action_ids.len() as u64);
            count.max_group_size = count.max_group_size.max(group_size);
            self.remember_largest_group(group);
        }
    }

    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsEquivalence {
        CombatSearchV2DiagnosticsEquivalence {
            equivalence_policy:
                "conservative_duplicate_play_card_and_single_card_pending_selection_by_runtime_signature",
            behavioral_effect:
                "safe_representative_child_generation_for_proven_duplicate_actions_only",
            states_observed: self.states_observed,
            states_compressed: self.states_compressed,
            atomic_actions_in: self.atomic_actions_in,
            representative_actions_out: self.representative_actions_out,
            actions_removed: self.actions_removed,
            removed_action_ratio: rounded_ratio(self.actions_removed, self.atomic_actions_in),
            max_group_size: self.max_group_size,
            group_kind_counts: self.group_kind_counts(),
            largest_groups: self.largest_group_samples(),
            notes: vec![
                "combat player-turn duplicate play-card compression remains limited to starter basic cards in v1",
                "single-card pending grid/hand selections can merge runtime-identical source cards",
                "multi-card pending selections stay atomic because selection order can affect resolution",
                "card runtime fields and target or selection scope must match; card lineage is intentionally ignored",
                "each equivalence kind is attached to an explicit StateAbstractionBoundarySpec",
                "representative action traces keep the original legal action id",
                "non-eligible actions stay atomic and order-sensitive",
            ],
        }
    }

    fn remember_largest_group(&mut self, group: &ActionEquivalenceGroupSummary) {
        self.largest_groups.push(ActionEquivalenceObservation {
            observed_at_state_query: self.states_observed,
            key: group.key.clone(),
            representative_original_action_id: group.representative_original_action_id,
            removed_original_action_ids: group.removed_original_action_ids.clone(),
        });
        self.largest_groups.sort_by(|left, right| {
            right
                .group_size()
                .cmp(&left.group_size())
                .then_with(|| left.key.kind.cmp(&right.key.kind))
                .then_with(|| left.key.signature.cmp(&right.key.signature))
                .then_with(|| {
                    left.observed_at_state_query
                        .cmp(&right.observed_at_state_query)
                })
        });
        self.largest_groups
            .truncate(LARGEST_EQUIVALENCE_GROUP_SAMPLE_LIMIT);
    }

    fn group_kind_counts(&self) -> Vec<CombatSearchV2DiagnosticsEquivalenceKindCount> {
        self.kind_counts
            .iter()
            .map(
                |(kind, count)| CombatSearchV2DiagnosticsEquivalenceKindCount {
                    kind: kind.label().to_string(),
                    boundary_id: kind.boundary_id(),
                    soundness: boundary_spec(kind.boundary_id()).soundness,
                    allowed_consumers: boundary_spec(kind.boundary_id()).allowed_consumers,
                    groups: count.groups,
                    actions_in: count.actions_in,
                    actions_removed: count.actions_removed,
                    max_group_size: count.max_group_size,
                },
            )
            .collect()
    }

    fn largest_group_samples(&self) -> Vec<CombatSearchV2DiagnosticsEquivalenceGroupSample> {
        self.largest_groups
            .iter()
            .map(|sample| CombatSearchV2DiagnosticsEquivalenceGroupSample {
                observed_at_state_query: sample.observed_at_state_query,
                kind: sample.key.kind.label().to_string(),
                boundary_id: sample.key.kind.boundary_id(),
                soundness: boundary_spec(sample.key.kind.boundary_id()).soundness,
                allowed_consumers: boundary_spec(sample.key.kind.boundary_id()).allowed_consumers,
                equivalence_key: sample.key.signature.clone(),
                representative_original_action_id: sample.representative_original_action_id,
                removed_original_action_ids: sample.removed_original_action_ids.clone(),
                group_size: sample.group_size(),
            })
            .collect()
    }
}

impl ActionEquivalenceObservation {
    fn group_size(&self) -> usize {
        self.removed_original_action_ids.len().saturating_add(1)
    }
}

impl ActionEquivalenceKind {
    fn label(self) -> &'static str {
        match self {
            ActionEquivalenceKind::StarterBasicPlayCard => "starter_basic_play_card",
            ActionEquivalenceKind::SingleCardPendingChoiceSelection => {
                "single_card_pending_choice_selection"
            }
        }
    }

    fn boundary_id(self) -> StateAbstractionBoundaryId {
        match self {
            ActionEquivalenceKind::StarterBasicPlayCard => {
                StateAbstractionBoundaryId::StarterBasicDuplicatePlayCardByTarget
            }
            ActionEquivalenceKind::SingleCardPendingChoiceSelection => {
                StateAbstractionBoundaryId::PendingChoiceIdenticalRuntimeCard
            }
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
