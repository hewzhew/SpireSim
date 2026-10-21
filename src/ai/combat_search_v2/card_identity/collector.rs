use super::summary::{CardIdentityGroupSummary, CardIdentitySummary};

const SAMPLE_LIMIT: usize = 8;

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct CardIdentityDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2::card_identity) states_observed: u64,
    pub(in crate::ai::combat_search_v2::card_identity) active_cards_observed: u64,
    pub(in crate::ai::combat_search_v2::card_identity) action_payload_cards_observed: u64,
    pub(in crate::ai::combat_search_v2::card_identity) action_payload_constructed_cards: u64,
    pub(in crate::ai::combat_search_v2::card_identity) states_with_duplicate_active_lineage: u64,
    pub(in crate::ai::combat_search_v2::card_identity) duplicate_active_lineage_observations: u64,
    pub(in crate::ai::combat_search_v2::card_identity) states_with_lineage_card_id_conflict: u64,
    pub(in crate::ai::combat_search_v2::card_identity) lineage_card_id_conflict_observations: u64,
    pub(in crate::ai::combat_search_v2::card_identity) max_duplicate_group_size: usize,
    pub(in crate::ai::combat_search_v2::card_identity) largest_duplicate_groups:
        Vec<CardIdentityObservedGroup>,
    pub(in crate::ai::combat_search_v2::card_identity) largest_conflict_groups:
        Vec<CardIdentityObservedGroup>,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2::card_identity) struct CardIdentityObservedGroup {
    pub(in crate::ai::combat_search_v2::card_identity) observed_at_state_query: u64,
    pub(in crate::ai::combat_search_v2::card_identity) group: CardIdentityGroupSummary,
}

impl CardIdentityDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe(&mut self, summary: &CardIdentitySummary) {
        self.states_observed = self.states_observed.saturating_add(1);
        self.active_cards_observed = self
            .active_cards_observed
            .saturating_add(summary.active_cards as u64);
        self.action_payload_cards_observed = self
            .action_payload_cards_observed
            .saturating_add(summary.action_payload_cards as u64);
        self.action_payload_constructed_cards = self
            .action_payload_constructed_cards
            .saturating_add(summary.action_payload_constructed_cards as u64);

        if !summary.duplicate_groups.is_empty() {
            self.states_with_duplicate_active_lineage =
                self.states_with_duplicate_active_lineage.saturating_add(1);
            self.duplicate_active_lineage_observations = self
                .duplicate_active_lineage_observations
                .saturating_add(summary.duplicate_groups.len() as u64);
        }
        if !summary.conflict_groups.is_empty() {
            self.states_with_lineage_card_id_conflict =
                self.states_with_lineage_card_id_conflict.saturating_add(1);
            self.lineage_card_id_conflict_observations = self
                .lineage_card_id_conflict_observations
                .saturating_add(summary.conflict_groups.len() as u64);
        }
        for group in &summary.duplicate_groups {
            self.max_duplicate_group_size =
                self.max_duplicate_group_size.max(group.occurrence_count);
            remember_group(
                &mut self.largest_duplicate_groups,
                self.states_observed,
                group,
            );
        }
        for group in &summary.conflict_groups {
            remember_group(
                &mut self.largest_conflict_groups,
                self.states_observed,
                group,
            );
        }
    }
}

fn remember_group(
    groups: &mut Vec<CardIdentityObservedGroup>,
    observed_at_state_query: u64,
    group: &CardIdentityGroupSummary,
) {
    groups.push(CardIdentityObservedGroup {
        observed_at_state_query,
        group: group.clone(),
    });
    groups.sort_by(|left, right| {
        right
            .group
            .occurrence_count
            .cmp(&left.group.occurrence_count)
            .then_with(|| {
                right
                    .group
                    .distinct_card_labels
                    .len()
                    .cmp(&left.group.distinct_card_labels.len())
            })
            .then_with(|| {
                left.observed_at_state_query
                    .cmp(&right.observed_at_state_query)
            })
    });
    groups.truncate(SAMPLE_LIMIT);
}
