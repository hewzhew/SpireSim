#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) enum PendingChoiceOrderingRole {
    ValueSelection,
    RemovalSelection,
    NeutralSelection,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct PendingChoiceOrderingHint {
    pub(in crate::ai::combat_search_v2) primary: i32,
    pub(in crate::ai::combat_search_v2) secondary: i32,
    pub(in crate::ai::combat_search_v2) selected_count_tiebreak: i32,
    /// Explicit semantic prior scale, separate from ordinal ordering.
    /// Every legal choice remains positive after this signed base-two bias.
    pub(in crate::ai::combat_search_v2) policy_log2_bias: i32,
    pub(in crate::ai::combat_search_v2) role: PendingChoiceOrderingRole,
}
