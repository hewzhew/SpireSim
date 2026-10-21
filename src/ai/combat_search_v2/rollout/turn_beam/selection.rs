use std::collections::HashSet;

use super::super::super::rollout_pending_choice::RolloutPendingChoiceProgress;
use super::super::super::value::combat_eval_from_rollout_estimate;
use super::super::super::*;
use super::attribution::TurnBeamExtensionAttribution;
use super::types::TurnBeamState;

pub(super) fn finish_with_attribution(
    estimate: RolloutNodeEstimate,
    mut attribution: TurnBeamExtensionAttribution,
) -> (RolloutNodeEstimate, TurnBeamExtensionAttribution) {
    attribution.observe_best_estimate(&estimate);
    (estimate, attribution)
}

pub(super) fn select_beam(
    mut candidates: Vec<TurnBeamState>,
    beam_width: usize,
) -> Vec<TurnBeamState> {
    let mut seen = HashSet::new();
    candidates.retain(|state| {
        seen.insert(combat_exact_state_key(
            &state.node.engine,
            &state.node.combat,
        ))
    });
    candidates.sort_by(|left, right| {
        let left_eval = combat_eval_from_rollout_estimate(&left.node.rollout_estimate);
        let right_eval = combat_eval_from_rollout_estimate(&right.node.rollout_estimate);
        right_eval.cmp(&left_eval)
    });
    candidates.truncate(beam_width.max(1));
    candidates
}

pub(super) fn best_estimate(
    beam: &[TurnBeamState],
    root_action_count: usize,
    stop_reason: RolloutStopReason,
) -> RolloutNodeEstimate {
    let Some(best) = beam.iter().max_by_key(|state| {
        combat_eval_from_rollout_estimate(&state_estimate(state, root_action_count, stop_reason))
    }) else {
        return RolloutNodeEstimate::unevaluated();
    };
    state_estimate(best, root_action_count, stop_reason)
}

pub(super) fn state_estimate(
    state: &TurnBeamState,
    root_action_count: usize,
    stop_reason: RolloutStopReason,
) -> RolloutNodeEstimate {
    if let Some(estimate) = &state.estimate_override {
        return estimate.clone();
    }
    let stop_reason = state.stalled_stop_reason.unwrap_or(stop_reason);
    RolloutNodeEstimate::from_node(
        &state.node,
        state.node.actions.len().saturating_sub(root_action_count),
        stop_reason,
        state.last_action_reason,
        state.progress,
    )
}

pub(super) fn merge_prior_pending_choice_progress(
    estimate: &mut RolloutNodeEstimate,
    prior: RolloutPendingChoiceProgress,
) {
    estimate.pending_choices_seen = estimate
        .pending_choices_seen
        .saturating_add(prior.pending_choices_seen);
    estimate.pending_choice_actions_simulated = estimate
        .pending_choice_actions_simulated
        .saturating_add(prior.pending_choice_actions_simulated);
    estimate.max_pending_choice_candidate_count = estimate
        .max_pending_choice_candidate_count
        .max(prior.max_pending_choice_candidate_count);
    estimate.max_pending_choice_estimated_action_fanout = estimate
        .max_pending_choice_estimated_action_fanout
        .max(prior.max_pending_choice_estimated_action_fanout);
    if estimate.last_pending_choice_kind.is_none() {
        estimate.last_pending_choice_kind = prior.last_pending_choice_kind_label();
    }
    estimate.stopped_on_high_fanout_pending_choice |= prior.stopped_on_high_fanout_pending_choice;
    estimate.high_fanout_pending_choice |= prior.stopped_on_high_fanout_pending_choice;
}

#[cfg(test)]
mod tests {
    use crate::test_support::blank_test_combat;

    use super::*;

    #[test]
    fn mixed_stalled_states_keep_the_selected_states_stop_reason() {
        let combat = blank_test_combat();
        let better = SearchNode::root(EngineState::CombatPlayerTurn, combat.clone());
        let mut worse = better.clone();
        worse.combat.entities.player.current_hp = 1;

        let stalled = vec![
            TurnBeamState {
                node: better,
                progress: RolloutPendingChoiceProgress::default(),
                last_action_reason: None,
                estimate_override: None,
                stalled_stop_reason: Some(RolloutStopReason::HighFanoutPendingChoice),
            },
            TurnBeamState {
                node: worse,
                progress: RolloutPendingChoiceProgress::default(),
                last_action_reason: None,
                estimate_override: None,
                stalled_stop_reason: Some(RolloutStopReason::NoLegalActions),
            },
        ];

        let estimate = best_estimate(&stalled, 0, RolloutStopReason::MaxActions);

        assert_eq!(
            estimate.stop_reason,
            RolloutStopReason::HighFanoutPendingChoice
        );
    }

    #[test]
    fn beam_selection_keeps_the_best_rollout_value() {
        let low = beam_state_with_final_hp(10);
        let high = beam_state_with_final_hp(20);

        let selected = select_beam(vec![low, high], 1);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].node.rollout_estimate.final_hp, 20);
    }

    #[test]
    fn zero_configured_beam_width_still_keeps_one_live_state() {
        let selected = select_beam(vec![beam_state_with_final_hp(20)], 0);

        assert_eq!(selected.len(), 1);
    }

    fn beam_state_with_final_hp(final_hp: i32) -> TurnBeamState {
        let mut node = SearchNode::root(EngineState::CombatPlayerTurn, blank_test_combat());
        node.combat.entities.player.current_hp = final_hp;
        node.rollout_estimate = RolloutNodeEstimate::from_node(
            &node,
            0,
            RolloutStopReason::MaxActions,
            None,
            RolloutPendingChoiceProgress::default(),
        );
        TurnBeamState {
            node,
            progress: RolloutPendingChoiceProgress::default(),
            last_action_reason: None,
            estimate_override: None,
            stalled_stop_reason: None,
        }
    }
}
