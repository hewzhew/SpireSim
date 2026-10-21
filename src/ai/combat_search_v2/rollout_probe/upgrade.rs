use super::score_types::RolloutActionProbeScore;

pub(super) fn probe_upgrade_reason(
    candidate: RolloutActionProbeScore,
    fallback: RolloutActionProbeScore,
    allow_nonterminal_upgrade: bool,
) -> Option<&'static str> {
    if candidate.terminal_rank > fallback.terminal_rank {
        return Some(
            super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_PROBE,
        );
    }
    if candidate.terminal_rank < fallback.terminal_rank {
        return None;
    }
    if candidate.recoverable_resource_urgency < fallback.recoverable_resource_urgency {
        return None;
    }
    if !allow_nonterminal_upgrade {
        return None;
    }
    if !candidate.nonterminal_upgrade_eligible {
        return None;
    }
    if candidate.final_hp < fallback.final_hp
        || candidate.survival_margin < fallback.survival_margin
    {
        return None;
    }
    if candidate.final_hp > fallback.final_hp {
        return Some(
            super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_SURVIVAL_VALUE,
        );
    }
    if candidate.visible_hp_loss < fallback.visible_hp_loss {
        return Some(
            super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_SURVIVAL_VALUE,
        );
    }
    if candidate.action_reactive_safety < fallback.action_reactive_safety {
        return None;
    }
    if candidate.phase_score() > fallback.phase_score() {
        Some(super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_PHASE_VALUE)
    } else if candidate.action_facts_score() > fallback.action_facts_score() {
        Some(
            super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_ACTION_FACTS_VALUE,
        )
    } else {
        None
    }
}
