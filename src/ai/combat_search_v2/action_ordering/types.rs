use super::super::action_effects::CardPlayEffectDiagnostics;
use super::super::action_priority::{ActionOrderingPriority, ActionOrderingRole};
use super::super::phase_action_ordering::PhaseActionOrderingHint;
use super::super::CombatActionChoice;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct IndexedActionChoice {
    pub(in crate::ai::combat_search_v2) original_action_id: usize,
    pub(in crate::ai::combat_search_v2) choice: CombatActionChoice,
}

pub(in crate::ai::combat_search_v2) type OrderedActionChoice = IndexedActionChoice;

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct ActionOrderingResult {
    pub(in crate::ai::combat_search_v2) choices: Vec<OrderedActionChoice>,
    pub(in crate::ai::combat_search_v2) summary: ActionOrderingSummary,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct ActionOrderingSummary {
    pub(in crate::ai::combat_search_v2::action_ordering) action_count: usize,
    pub(in crate::ai::combat_search_v2::action_ordering) max_position_shift: usize,
    pub(in crate::ai::combat_search_v2::action_ordering) role_counts:
        BTreeMap<ActionOrderingRole, usize>,
    pub(in crate::ai::combat_search_v2::action_ordering) first_role: Option<ActionOrderingRole>,
    pub(in crate::ai::combat_search_v2::action_ordering) first_original_action_id: Option<usize>,
    pub(in crate::ai::combat_search_v2::action_ordering) first_action_key: Option<String>,
    pub(in crate::ai::combat_search_v2::action_ordering) phase_signal_actions: usize,
    pub(in crate::ai::combat_search_v2::action_ordering) root_action_prior_scored_actions: usize,
    pub(in crate::ai::combat_search_v2::action_ordering) action_effect_samples:
        Vec<ActionOrderingActionEffectSummary>,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2::action_ordering) struct ActionOrderingEntry {
    pub(in crate::ai::combat_search_v2::action_ordering) original_action_id: usize,
    pub(in crate::ai::combat_search_v2::action_ordering) choice: CombatActionChoice,
    pub(in crate::ai::combat_search_v2::action_ordering) priority: ActionOrderingPriority,
    pub(in crate::ai::combat_search_v2::action_ordering) root_action_prior_score: Option<f64>,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2::action_ordering) struct ActionOrderingActionEffectSummary {
    pub(in crate::ai::combat_search_v2::action_ordering) original_action_id: usize,
    pub(in crate::ai::combat_search_v2::action_ordering) ordered_index: usize,
    pub(in crate::ai::combat_search_v2::action_ordering) role: ActionOrderingRole,
    pub(in crate::ai::combat_search_v2::action_ordering) action_key: String,
    pub(in crate::ai::combat_search_v2::action_ordering) effects: CardPlayEffectDiagnostics,
    pub(in crate::ai::combat_search_v2::action_ordering) phase_hint: PhaseActionOrderingHint,
}

impl ActionOrderingSummary {
    pub(in crate::ai::combat_search_v2) fn action_count(&self) -> usize {
        self.action_count
    }

    pub(in crate::ai::combat_search_v2) fn first_role(&self) -> Option<ActionOrderingRole> {
        self.first_role
    }

    pub(in crate::ai::combat_search_v2) fn role_counts(
        &self,
    ) -> impl Iterator<Item = (ActionOrderingRole, usize)> + '_ {
        self.role_counts.iter().map(|(role, count)| (*role, *count))
    }
}
