use super::types::{ActionOrderingActionEffectSummary, ActionOrderingEntry, ActionOrderingSummary};
use std::collections::BTreeMap;

pub(in crate::ai::combat_search_v2::action_ordering) fn summarize_ordering(
    entries: &[ActionOrderingEntry],
) -> ActionOrderingSummary {
    let mut role_counts = BTreeMap::new();
    let mut max_position_shift = 0usize;
    let mut phase_signal_actions = 0usize;
    let mut root_action_prior_scored_actions = 0usize;
    let mut action_effect_samples = Vec::new();
    for (ordered_index, entry) in entries.iter().enumerate() {
        *role_counts.entry(entry.priority.role).or_insert(0) += 1;
        max_position_shift =
            max_position_shift.max(entry.original_action_id.abs_diff(ordered_index));
        if entry.root_action_prior_score.is_some() {
            root_action_prior_scored_actions += 1;
        }
        if entry.priority.phase_hint.has_signal() {
            phase_signal_actions += 1;
        }
        if entry.priority.effects.has_reactive_signal() || entry.priority.phase_hint.has_signal() {
            action_effect_samples.push(ActionOrderingActionEffectSummary {
                original_action_id: entry.original_action_id,
                ordered_index,
                role: entry.priority.role,
                action_key: entry.choice.action_key.clone(),
                effects: entry.priority.effects,
                phase_hint: entry.priority.phase_hint,
            });
        }
    }

    ActionOrderingSummary {
        action_count: entries.len(),
        max_position_shift,
        role_counts,
        first_role: entries.first().map(|entry| entry.priority.role),
        first_original_action_id: entries.first().map(|entry| entry.original_action_id),
        first_action_key: entries.first().map(|entry| entry.choice.action_key.clone()),
        phase_signal_actions,
        root_action_prior_scored_actions,
        action_effect_samples,
    }
}
