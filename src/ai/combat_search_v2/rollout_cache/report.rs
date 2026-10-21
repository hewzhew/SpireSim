use super::super::*;
use super::RolloutCache;

impl RolloutCache {
    pub(in crate::ai::combat_search_v2) fn finish(
        &self,
        best_frontier: Option<&SearchNode>,
    ) -> CombatSearchV2RolloutReport {
        CombatSearchV2RolloutReport {
            policy: CombatSearchV2RolloutPolicy::from(self.policy).label(),
            behavioral_effect:
                "estimated_frontier_priority_with_single_exact_terminal_replay_no_estimated_terminal_claim",
            max_evaluations: self.max_evaluations,
            max_actions_per_rollout: self.max_actions,
            beam_width: self.beam_width,
            turn_beam_extension_budget: self.turn_beam_extension_budget,
            turn_beam_extensions: self.turn_beam_extensions,
            turn_beam_extension_budget_skips: self.turn_beam_extension_budget_skips,
            evaluations: self.evaluations,
            cache_queries: self.cache_queries,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            cache_inserts: self.cache_inserts,
            budget_skips: self.budget_skips,
            max_evaluation_budget_skips: self.max_evaluation_budget_skips,
            deadline_budget_skips: self.deadline_budget_skips,
            truncated_rollouts: self.truncated,
            terminal_wins: self.terminal_wins,
            terminal_losses: self.terminal_losses,
            rollouts_with_pending_choice: self.rollouts_with_pending_choice,
            rollouts_stopped_on_high_fanout_pending_choice: self
                .rollouts_stopped_on_high_fanout_pending_choice,
            pending_choice_actions_simulated: self.pending_choice_actions_simulated,
            max_pending_choice_estimated_action_fanout: self
                .max_pending_choice_estimated_action_fanout,
            performance: self.performance.to_report(),
            turn_beam_attribution: CombatSearchV2TurnBeamAttributionReport {
                enabled: self.policy == CombatSearchRolloutPluginId::TurnBeamNoPotion,
                calls: self.turn_beam_calls,
                conservative_anchor_present: self.turn_beam_conservative_anchor_present,
                conservative_anchor_selected: self.turn_beam_conservative_anchor_selected,
                conservative_anchor_terminal_wins: self
                    .turn_beam_conservative_anchor_terminal_wins,
                extension_calls: self.turn_beam_extensions,
                turn_plan_calls: self.turn_beam_turn_plan_calls,
                turn_plan_inner_nodes_expanded: self.turn_beam_turn_plan_inner_nodes_expanded,
                turn_plan_inner_nodes_generated: self.turn_beam_turn_plan_inner_nodes_generated,
                turn_plans_kept: self.turn_beam_turn_plans_kept,
                turn_plans_kept_by_bucket: self
                    .turn_beam_turn_plans_kept_by_bucket
                    .iter()
                    .map(|(bucket, count)| CombatSearchV2TurnBeamBucketCountReport {
                        bucket,
                        count: *count,
                    })
                    .collect(),
                terminal_candidates_kept: self.turn_beam_terminal_candidates_kept,
                best_pv_len: self.turn_beam_best_pv_len,
                best_pv_terminal: self.turn_beam_best_pv_terminal,
            },
            best_frontier_estimate: best_frontier
                .and_then(|node| node.rollout_estimate.to_report()),
            notes: vec![
                "rollout estimates are not terminal outcome records; one complete terminal-win witness may be promoted only after exact replay",
                "conservative_no_potion uses only legal simulator actions and disables potion actions",
                "rollout cache is keyed by exact combat runtime state",
                "unresolved rollout priority uses phase-adjusted enemy effort from phase_profile",
                "high-fanout pending choices stop rollout estimates instead of selecting an arbitrary branch",
                "small pending choices may be followed by rollout, but their actions become a terminal outcome record only after exact replay",
                "enemy_mechanics_adaptive_no_potion uses phase-aware rollout for typed Guardian/Bronze Automaton and finite-survival damage-mitigation mechanics, otherwise falling back to conservative_no_potion",
                "turn_beam_no_potion uses turn-plan end states as an estimate-only beam; a complete terminal-win witness still requires exact replay",
            ],
        }
    }
}
