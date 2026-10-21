use std::cmp::Ordering;

use super::types::{CombatEvalOutcomeClass, CombatEvalV2};

impl Ord for CombatEvalV2 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_core(other)
            .then_with(|| self.resource_conservation.cmp(&other.resource_conservation))
            .then_with(|| self.faster_turns.cmp(&other.faster_turns))
            .then_with(|| self.fewer_cards_played.cmp(&other.fewer_cards_played))
    }
}

impl PartialOrd for CombatEvalV2 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl CombatEvalV2 {
    pub(in crate::ai::combat_search_v2) fn cmp_core(&self, other: &Self) -> Ordering {
        self.outcome
            .cmp(&other.outcome)
            .then_with(|| self.evidence.cmp(&other.evidence))
            .then_with(|| match self.outcome {
                CombatEvalOutcomeClass::Win => self.compare_win(other),
                CombatEvalOutcomeClass::Loss => self.compare_loss_estimate(other),
                CombatEvalOutcomeClass::Unresolved => self.compare_unresolved(other),
            })
    }

    fn compare_win(self, other: &Self) -> Ordering {
        self.persistent_adjusted_hp
            .cmp(&other.persistent_adjusted_hp)
            .then_with(|| self.final_hp.cmp(&other.final_hp))
            .then_with(|| self.persistent_run_value.cmp(&other.persistent_run_value))
            .then_with(|| self.risk_margin.cmp(&other.risk_margin))
            .then_with(|| self.enemy_progress.cmp(&other.enemy_progress))
            .then_with(|| self.phase_stability.cmp(&other.phase_stability))
    }

    fn compare_loss_estimate(self, other: &Self) -> Ordering {
        self.progress
            .cmp(&other.progress)
            .then_with(|| self.enemy_progress.cmp(&other.enemy_progress))
            .then_with(|| self.phase_stability.cmp(&other.phase_stability))
            .then_with(|| self.risk_margin.cmp(&other.risk_margin))
            .then_with(|| self.final_hp.cmp(&other.final_hp))
    }

    fn compare_unresolved(self, other: &Self) -> Ordering {
        if self.survival.is_danger() || other.survival.is_danger() {
            // In danger states, avoid ranking a flashy race line above a line
            // that first fixes visible survival.
            self.survival
                .cmp(&other.survival)
                .then_with(|| self.risk_margin.cmp(&other.risk_margin))
                .then_with(|| self.progress.cmp(&other.progress))
                .then_with(|| self.enemy_progress.cmp(&other.enemy_progress))
                .then_with(|| self.phase_stability.cmp(&other.phase_stability))
                .then_with(|| {
                    self.persistent_adjusted_hp
                        .cmp(&other.persistent_adjusted_hp)
                })
                .then_with(|| self.final_hp.cmp(&other.final_hp))
        } else {
            // In stable states, enemy/phase progress outranks small HP
            // differences so high-HP stalled states do not dominate the queue.
            self.progress
                .cmp(&other.progress)
                .then_with(|| self.enemy_progress.cmp(&other.enemy_progress))
                .then_with(|| self.phase_stability.cmp(&other.phase_stability))
                .then_with(|| self.survival.cmp(&other.survival))
                .then_with(|| self.risk_margin.cmp(&other.risk_margin))
                .then_with(|| {
                    self.persistent_adjusted_hp
                        .cmp(&other.persistent_adjusted_hp)
                })
                .then_with(|| self.final_hp.cmp(&other.final_hp))
        }
    }
}
