use std::collections::BTreeMap;

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct ActionExpansionDiagnosticsCollector {
    pub(super) states_observed: u64,
    pub(super) total_atomic_actions: u64,
    pub(super) total_fanout_groups: u64,
    pub(super) fanout_groups_max: usize,
    pub(super) max_group_size: usize,
    pub(super) kind_counts: BTreeMap<ActionExpansionKind, MutableKindCount>,
    pub(super) largest_groups: Vec<ActionExpansionGroupObservation>,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct ActionExpansionSummary {
    pub(in crate::ai::combat_search_v2) action_count: usize,
    pub(in crate::ai::combat_search_v2) group_count: usize,
    pub(super) groups: Vec<ActionExpansionGroupSummary>,
}

#[derive(Clone, Debug)]
pub(super) struct ActionExpansionGroupSummary {
    pub(super) key: ActionExpansionGroupKey,
    pub(super) action_count: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ActionExpansionGroupObservation {
    pub(super) observed_at_state_query: u64,
    pub(super) key: ActionExpansionGroupKey,
    pub(super) action_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ActionExpansionGroupKey {
    pub(super) kind: ActionExpansionKind,
    pub(super) signature: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ActionExpansionKind {
    PlayCard,
    EndTurn,
    UsePotion,
    DiscardPotion,
    DiscoverChoice,
    HandSelect,
    GridSelect,
    ScryDiscard,
    Cancel,
    Proceed,
    Other,
}

#[derive(Clone, Debug, Default)]
pub(super) struct MutableKindCount {
    pub(super) atomic_actions: u64,
    pub(super) fanout_groups: u64,
    pub(super) max_group_size: usize,
}

impl ActionExpansionGroupKey {
    fn cmp_tuple(&self) -> (ActionExpansionKind, &str) {
        (self.kind, &self.signature)
    }
}

impl Ord for ActionExpansionGroupKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cmp_tuple().cmp(&other.cmp_tuple())
    }
}

impl PartialOrd for ActionExpansionGroupKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl ActionExpansionKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            ActionExpansionKind::PlayCard => "play_card",
            ActionExpansionKind::EndTurn => "end_turn",
            ActionExpansionKind::UsePotion => "use_potion",
            ActionExpansionKind::DiscardPotion => "discard_potion",
            ActionExpansionKind::DiscoverChoice => "discover_choice",
            ActionExpansionKind::HandSelect => "hand_select",
            ActionExpansionKind::GridSelect => "grid_select",
            ActionExpansionKind::ScryDiscard => "scry_discard",
            ActionExpansionKind::Cancel => "cancel",
            ActionExpansionKind::Proceed => "proceed",
            ActionExpansionKind::Other => "other",
        }
    }
}
