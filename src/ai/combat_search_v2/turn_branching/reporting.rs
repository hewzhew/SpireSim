use super::super::*;
use super::{TurnBranchingDiagnosticsCollector, TurnBranchingStateObservation};

const LARGEST_TURN_FANOUT_SAMPLE_LIMIT: usize = 8;

impl TurnBranchingDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsTurnBranching {
        CombatSearchV2DiagnosticsTurnBranching {
            organization_policy: "turn_transition_classification_with_late_frontier_tie_break",
            behavioral_effect: "diagnostic_summary_plus_priority_hint_no_prune_no_merge",
            states_observed: self.states_observed,
            total_legal_actions: self.total_legal_actions,
            total_generated_children: self.total_generated_children,
            generated_children_per_state: rounded_ratio(
                self.total_generated_children,
                self.states_observed,
            ),
            same_turn_children: self.same_turn_children,
            next_turn_children: self.next_turn_children,
            pending_choice_children: self.pending_choice_children,
            terminal_children: self.terminal_children,
            other_children: self.other_children,
            end_turn_children: self.end_turn_children,
            same_turn_child_ratio: rounded_ratio(
                self.same_turn_children,
                self.total_generated_children,
            ),
            next_turn_child_ratio: rounded_ratio(
                self.next_turn_children,
                self.total_generated_children,
            ),
            transition_counts: self.transition_count_reports(),
            largest_turn_fanouts: self.largest_turn_fanout_reports(),
            notes: vec![
                "turn branching classifies generated children after simulator execution",
                "same_turn and next_turn are transition labels, not pruning rules",
                "priority hints are late frontier tie-breaks and do not remove legal actions",
                "future turn-prefix dominance must prove safety before pruning",
            ],
        }
    }

    pub(super) fn remember_largest_turn_fanout(
        &mut self,
        observation: &TurnBranchingStateObservation,
    ) {
        if observation.generated_children <= 1 {
            return;
        }
        self.largest_turn_fanouts.push(observation.clone());
        self.largest_turn_fanouts.sort_by(|left, right| {
            right
                .generated_children
                .cmp(&left.generated_children)
                .then_with(|| right.same_turn_children.cmp(&left.same_turn_children))
                .then_with(|| left.parent_turn_count.cmp(&right.parent_turn_count))
        });
        self.largest_turn_fanouts
            .truncate(LARGEST_TURN_FANOUT_SAMPLE_LIMIT);
    }

    fn transition_count_reports(&self) -> Vec<CombatSearchV2DiagnosticsTurnTransitionCount> {
        self.transition_counts
            .iter()
            .map(
                |(key, children)| CombatSearchV2DiagnosticsTurnTransitionCount {
                    action_kind: key.action_kind.label().to_string(),
                    transition_kind: key.transition_kind.label().to_string(),
                    children: *children,
                },
            )
            .collect()
    }

    fn largest_turn_fanout_reports(&self) -> Vec<CombatSearchV2DiagnosticsTurnFanoutSample> {
        self.largest_turn_fanouts
            .iter()
            .map(|sample| CombatSearchV2DiagnosticsTurnFanoutSample {
                parent_turn_count: sample.parent_turn_count,
                parent_energy: sample.parent_energy,
                legal_actions: sample.legal_actions,
                generated_children: sample.generated_children,
                same_turn_children: sample.same_turn_children,
                next_turn_children: sample.next_turn_children,
                pending_choice_children: sample.pending_choice_children,
                terminal_children: sample.terminal_children,
                end_turn_children: sample.end_turn_children,
            })
            .collect()
    }
}

fn rounded_ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    let value = numerator as f64 / denominator as f64;
    (value * 100.0).round() / 100.0
}
