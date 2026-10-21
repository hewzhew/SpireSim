use std::cmp::Ordering;

use crate::sim::combat::CombatStepResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RolloutActionProbeScore {
    pub(super) terminal_rank: i32,
    pub(super) recoverable_resource_urgency: i32,
    pub(super) final_hp: i32,
    pub(super) survival_margin: i32,
    pub(super) visible_hp_loss: i32,
    pub(super) living_enemy_progress: i32,
    pub(super) phase_adjusted_enemy_progress: i32,
    pub(super) split_debt_stability: i32,
    pub(super) mechanics_stability: i32,
    pub(super) action_sustained_mitigation: i32,
    pub(super) action_visible_mitigation: i32,
    pub(super) action_debuff_setup: i32,
    pub(super) action_resource_timing: i32,
    pub(super) action_progress_hint: i32,
    pub(super) action_access_gain: i32,
    pub(super) action_reactive_safety: i32,
    pub(super) pending_choice_fanout: i32,
    pub(super) ordered_preference: i32,
    pub(super) nonterminal_upgrade_eligible: bool,
}

#[derive(Clone, Debug)]
pub(super) struct RolloutActionProbeResult {
    pub(super) score: RolloutActionProbeScore,
    pub(super) step: CombatStepResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(super) struct RolloutTerminalProbeScore {
    pub(super) terminal_rank: i32,
    pub(super) final_hp: i32,
    pub(super) ordered_preference: i32,
}

#[derive(Clone, Debug)]
pub(super) struct RolloutTerminalProbeResult {
    pub(super) score: RolloutTerminalProbeScore,
    pub(super) step: CombatStepResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(super) struct RolloutPhaseProbeScore {
    split_debt_stability: i32,
    mechanics_stability: i32,
    pending_choice_fanout: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(super) struct RolloutActionFactsProbeScore {
    pub(super) sustained_mitigation: i32,
    pub(super) visible_mitigation: i32,
    pub(super) debuff_setup: i32,
    pub(super) resource_timing: i32,
    pub(super) progress_hint: i32,
    pub(super) access_gain: i32,
    pub(super) reactive_safety: i32,
}

impl Ord for RolloutActionProbeScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.terminal_rank
            .cmp(&other.terminal_rank)
            .then_with(|| {
                self.recoverable_resource_urgency
                    .cmp(&other.recoverable_resource_urgency)
            })
            .then_with(|| self.final_hp.cmp(&other.final_hp))
            .then_with(|| self.survival_margin.cmp(&other.survival_margin))
            .then_with(|| other.visible_hp_loss.cmp(&self.visible_hp_loss))
            .then_with(|| self.living_enemy_progress.cmp(&other.living_enemy_progress))
            .then_with(|| {
                self.phase_adjusted_enemy_progress
                    .cmp(&other.phase_adjusted_enemy_progress)
            })
            .then_with(|| self.phase_score().cmp(&other.phase_score()))
            .then_with(|| self.action_facts_score().cmp(&other.action_facts_score()))
            .then_with(|| self.ordered_preference.cmp(&other.ordered_preference))
    }
}

impl PartialOrd for RolloutActionProbeScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl RolloutActionProbeScore {
    pub(super) fn phase_score(self) -> RolloutPhaseProbeScore {
        RolloutPhaseProbeScore {
            split_debt_stability: self.split_debt_stability,
            mechanics_stability: self.mechanics_stability,
            pending_choice_fanout: self.pending_choice_fanout,
        }
    }

    pub(super) fn action_facts_score(self) -> RolloutActionFactsProbeScore {
        RolloutActionFactsProbeScore {
            sustained_mitigation: self.action_sustained_mitigation,
            visible_mitigation: self.action_visible_mitigation,
            debuff_setup: self.action_debuff_setup,
            resource_timing: self.action_resource_timing,
            progress_hint: self.action_progress_hint,
            access_gain: self.action_access_gain,
            reactive_safety: self.action_reactive_safety,
        }
    }
}
