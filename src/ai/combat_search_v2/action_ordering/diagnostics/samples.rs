use super::super::super::action_effects::CardPlayEffectDiagnostics;
use super::super::super::action_priority::ActionOrderingRole;
use super::super::super::phase_action_ordering::PhaseActionOrderingHint;

pub(super) const LARGEST_REORDER_SAMPLE_LIMIT: usize = 8;
pub(in crate::ai::combat_search_v2::action_ordering) const ACTION_EFFECT_SAMPLE_LIMIT: usize = 12;

#[derive(Clone, Debug, Default)]
pub(super) struct MutableOrderingRoleCount {
    pub(super) actions: u64,
    pub(super) first_actions: u64,
}

#[derive(Clone, Debug)]
pub(super) struct ActionOrderingObservation {
    pub(super) observed_at_state_query: u64,
    pub(super) action_count: usize,
    pub(super) max_position_shift: usize,
    pub(super) first_role: ActionOrderingRole,
    pub(super) first_original_action_id: usize,
    pub(super) first_action_key: String,
}

#[derive(Clone, Debug)]
pub(super) struct ActionOrderingActionEffectObservation {
    pub(super) observed_at_state_query: u64,
    pub(super) original_action_id: usize,
    pub(super) ordered_index: usize,
    pub(super) role: ActionOrderingRole,
    pub(super) action_key: String,
    pub(super) effects: CardPlayEffectDiagnostics,
    pub(super) phase_hint: PhaseActionOrderingHint,
}
