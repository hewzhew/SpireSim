use super::super::IndexedActionChoice;
use super::keys::ActionEquivalenceKey;

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct ActionEquivalenceResult {
    pub(in crate::ai::combat_search_v2) choices: Vec<IndexedActionChoice>,
    pub(in crate::ai::combat_search_v2) summary: ActionEquivalenceSummary,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct ActionEquivalenceSummary {
    pub(super) atomic_actions_in: usize,
    pub(super) representative_actions_out: usize,
    pub(super) groups: Vec<ActionEquivalenceGroupSummary>,
}

#[derive(Clone, Debug)]
pub(super) struct ActionEquivalenceGroupSummary {
    pub(super) key: ActionEquivalenceKey,
    pub(super) representative_original_action_id: usize,
    pub(super) removed_original_action_ids: Vec<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct PendingEquivalenceGroup {
    pub(super) representative_original_action_id: usize,
    pub(super) removed_original_action_ids: Vec<usize>,
}

impl ActionEquivalenceSummary {
    pub(super) fn actions_removed(&self) -> usize {
        self.atomic_actions_in
            .saturating_sub(self.representative_actions_out)
    }
}

impl ActionEquivalenceGroupSummary {
    pub(super) fn group_size(&self) -> usize {
        self.removed_original_action_ids.len().saturating_add(1)
    }
}
