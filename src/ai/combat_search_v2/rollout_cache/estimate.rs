use std::time::Instant;

use crate::ai::combat_state_key::combat_exact_state_key;
use crate::sim::combat::CombatStepper;

use super::super::*;
use super::policy::{adaptive_no_potion_rollout_plugin, better_rollout_estimate};
use super::{ReplayableTerminalWinWitness, RolloutCache};

impl RolloutCache {
    pub(in crate::ai::combat_search_v2) fn estimate(
        &mut self,
        node: &SearchNode,
        stepper: &impl CombatStepper,
        config: &CombatSearchV2Config,
        deadline: Option<Instant>,
        nodes_generated_at_discovery: u64,
    ) -> RolloutNodeEstimate {
        if self.policy == CombatSearchRolloutPluginId::Disabled {
            return RolloutNodeEstimate::unevaluated();
        }

        let cache_lookup_started = Instant::now();
        let key = combat_exact_state_key(&node.engine, &node.combat);
        self.cache_queries = self.cache_queries.saturating_add(1);
        if let Some(cached) = self.cache.get(&key).cloned() {
            self.performance.cache_lookup_elapsed_us = self
                .performance
                .cache_lookup_elapsed_us
                .saturating_add(cache_lookup_started.elapsed().as_micros());
            self.cache_hits = self.cache_hits.saturating_add(1);
            return cached;
        }
        self.performance.cache_lookup_elapsed_us = self
            .performance
            .cache_lookup_elapsed_us
            .saturating_add(cache_lookup_started.elapsed().as_micros());
        self.cache_misses = self.cache_misses.saturating_add(1);
        if self.evaluations as usize >= self.max_evaluations {
            self.budget_skips = self.budget_skips.saturating_add(1);
            self.max_evaluation_budget_skips = self.max_evaluation_budget_skips.saturating_add(1);
            return RolloutNodeEstimate::unevaluated();
        }
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            self.budget_skips = self.budget_skips.saturating_add(1);
            self.deadline_budget_skips = self.deadline_budget_skips.saturating_add(1);
            return RolloutNodeEstimate::unevaluated();
        }

        self.evaluations = self.evaluations.saturating_add(1);
        let policy_dispatch_started = Instant::now();
        let estimate = match self.policy {
            CombatSearchRolloutPluginId::Disabled => RolloutNodeEstimate::unevaluated(),
            CombatSearchRolloutPluginId::EnemyMechanicsAdaptiveNoPotion => {
                match adaptive_no_potion_rollout_plugin(node) {
                    CombatSearchRolloutPluginId::PhaseAwareNoPotion => {
                        rollout::phase_aware_no_potion_rollout(
                            node,
                            stepper,
                            config,
                            self.max_actions,
                            deadline,
                            &mut self.performance,
                        )
                    }
                    _ => rollout::conservative_no_potion_rollout(
                        node,
                        stepper,
                        config,
                        self.max_actions,
                        deadline,
                        &mut self.performance,
                    ),
                }
            }
            CombatSearchRolloutPluginId::ConservativeNoPotion => {
                rollout::conservative_no_potion_rollout(
                    node,
                    stepper,
                    config,
                    self.max_actions,
                    deadline,
                    &mut self.performance,
                )
            }
            CombatSearchRolloutPluginId::PhaseAwareNoPotion => {
                rollout::phase_aware_no_potion_rollout(
                    node,
                    stepper,
                    config,
                    self.max_actions,
                    deadline,
                    &mut self.performance,
                )
            }
            CombatSearchRolloutPluginId::TurnBeamNoPotion => {
                self.turn_beam_calls = self.turn_beam_calls.saturating_add(1);
                let anchor = rollout::turn_beam_conservative_anchor_rollout(
                    node,
                    stepper,
                    config,
                    self.max_actions,
                    deadline,
                    &mut self.performance,
                );
                self.turn_beam_conservative_anchor_present =
                    self.turn_beam_conservative_anchor_present.saturating_add(1);
                if anchor.terminal == SearchTerminalLabel::Win {
                    self.turn_beam_conservative_anchor_terminal_wins = self
                        .turn_beam_conservative_anchor_terminal_wins
                        .saturating_add(1);
                    self.turn_beam_conservative_anchor_selected = self
                        .turn_beam_conservative_anchor_selected
                        .saturating_add(1);
                    self.observe_turn_beam_best_pv(&anchor);
                    anchor
                } else if self.turn_beam_extensions as usize >= self.turn_beam_extension_budget {
                    self.turn_beam_extension_budget_skips =
                        self.turn_beam_extension_budget_skips.saturating_add(1);
                    self.turn_beam_conservative_anchor_selected = self
                        .turn_beam_conservative_anchor_selected
                        .saturating_add(1);
                    self.observe_turn_beam_best_pv(&anchor);
                    anchor
                } else {
                    self.turn_beam_extensions = self.turn_beam_extensions.saturating_add(1);
                    let (beam, attribution) = rollout::turn_beam_extension_rollout_with_attribution(
                        node,
                        stepper,
                        config,
                        self.max_actions,
                        deadline,
                        &mut self.performance,
                    );
                    self.observe_turn_beam_extension_attribution(attribution);
                    let selected = better_rollout_estimate(beam, anchor.clone());
                    if selected == anchor {
                        self.turn_beam_conservative_anchor_selected = self
                            .turn_beam_conservative_anchor_selected
                            .saturating_add(1);
                    }
                    self.observe_turn_beam_best_pv(&selected);
                    selected
                }
            }
        };
        self.performance.policy_dispatch_elapsed_us = self
            .performance
            .policy_dispatch_elapsed_us
            .saturating_add(policy_dispatch_started.elapsed().as_micros());
        self.observe_estimate(&estimate, nodes_generated_at_discovery);
        self.cache.insert(key, estimate.clone());
        self.cache_inserts = self.cache_inserts.saturating_add(1);
        estimate
    }

    fn observe_estimate(
        &mut self,
        estimate: &RolloutNodeEstimate,
        nodes_generated_at_discovery: u64,
    ) {
        if estimate.is_replayable_terminal_win() {
            observe_best_replayable_terminal_win(
                &mut self.best_replayable_terminal_win,
                estimate,
                nodes_generated_at_discovery,
            );
            if estimate.is_replayable_terminal_win_without_new_external_burden(
                self.initial_external_burden_count,
            ) {
                observe_best_replayable_terminal_win(
                    &mut self.best_replayable_terminal_win_without_new_external_burden,
                    estimate,
                    nodes_generated_at_discovery,
                );
                if estimate.potions_used == 0 && estimate.potions_discarded == 0 {
                    observe_best_replayable_terminal_win(
                        &mut self
                            .best_replayable_terminal_potion_free_win_without_new_external_burden,
                        estimate,
                        nodes_generated_at_discovery,
                    );
                }
            }
        }
        if estimate.truncated {
            self.truncated = self.truncated.saturating_add(1);
        }
        if estimate.pending_choices_seen > 0 {
            self.rollouts_with_pending_choice = self.rollouts_with_pending_choice.saturating_add(1);
        }
        if estimate.stopped_on_high_fanout_pending_choice {
            self.rollouts_stopped_on_high_fanout_pending_choice = self
                .rollouts_stopped_on_high_fanout_pending_choice
                .saturating_add(1);
        }
        self.pending_choice_actions_simulated = self
            .pending_choice_actions_simulated
            .saturating_add(estimate.pending_choice_actions_simulated as u64);
        self.max_pending_choice_estimated_action_fanout = self
            .max_pending_choice_estimated_action_fanout
            .max(estimate.max_pending_choice_estimated_action_fanout);
        match estimate.terminal {
            SearchTerminalLabel::Win => self.terminal_wins = self.terminal_wins.saturating_add(1),
            SearchTerminalLabel::Loss => {
                self.terminal_losses = self.terminal_losses.saturating_add(1)
            }
            SearchTerminalLabel::Escape => {}
            SearchTerminalLabel::Unresolved => {}
        }
    }

    fn observe_turn_beam_extension_attribution(
        &mut self,
        attribution: rollout::TurnBeamExtensionAttribution,
    ) {
        self.turn_beam_turn_plan_calls = self
            .turn_beam_turn_plan_calls
            .saturating_add(attribution.turn_plan_calls);
        self.turn_beam_turn_plan_inner_nodes_expanded = self
            .turn_beam_turn_plan_inner_nodes_expanded
            .saturating_add(attribution.turn_plan_inner_nodes_expanded);
        self.turn_beam_turn_plan_inner_nodes_generated = self
            .turn_beam_turn_plan_inner_nodes_generated
            .saturating_add(attribution.turn_plan_inner_nodes_generated);
        self.turn_beam_turn_plans_kept = self
            .turn_beam_turn_plans_kept
            .saturating_add(attribution.turn_plans_kept);
        self.turn_beam_terminal_candidates_kept = self
            .turn_beam_terminal_candidates_kept
            .saturating_add(attribution.terminal_candidates_kept);
        for (bucket, count) in attribution.turn_plans_kept_by_bucket {
            *self
                .turn_beam_turn_plans_kept_by_bucket
                .entry(bucket)
                .or_default() += count;
        }
        if self.turn_beam_best_pv_terminal.is_none()
            || attribution.best_pv_len > self.turn_beam_best_pv_len
        {
            self.turn_beam_best_pv_len = attribution.best_pv_len;
            self.turn_beam_best_pv_terminal = attribution.best_pv_terminal;
        }
    }

    fn observe_turn_beam_best_pv(&mut self, estimate: &RolloutNodeEstimate) {
        if self.turn_beam_best_pv_terminal.is_none()
            || estimate.actions_simulated > self.turn_beam_best_pv_len
        {
            self.turn_beam_best_pv_len = estimate.actions_simulated;
            self.turn_beam_best_pv_terminal = Some(estimate.terminal);
        }
    }
}

fn observe_best_replayable_terminal_win(
    current: &mut Option<ReplayableTerminalWinWitness>,
    estimate: &RolloutNodeEstimate,
    nodes_generated_at_discovery: u64,
) {
    let replace = current
        .as_ref()
        .map(|current| {
            better_rollout_estimate(estimate.clone(), current.estimate.clone()) == *estimate
        })
        .unwrap_or(true);
    if replace {
        *current = Some(ReplayableTerminalWinWitness {
            estimate: estimate.clone(),
            nodes_generated_at_discovery,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::core::ClientInput;

    fn terminal_win(final_hp: i32, external_burden_count: i32) -> RolloutNodeEstimate {
        let mut estimate = RolloutNodeEstimate::unevaluated();
        estimate.evaluated = true;
        estimate.terminal = SearchTerminalLabel::Win;
        estimate.final_hp = final_hp;
        estimate.external_burden_count = external_burden_count;
        estimate.total_actions = 1;
        estimate.action_preview = vec![CombatSearchV2ActionPreview {
            action_key: "terminal".to_string(),
            input: ClientInput::EndTurn,
        }];
        estimate.stop_reason = RolloutStopReason::TerminalState;
        estimate
    }

    #[test]
    fn higher_scoring_dirty_terminal_rollout_does_not_erase_the_clean_witness() {
        let mut cache = RolloutCache {
            initial_external_burden_count: 1,
            ..RolloutCache::default()
        };
        let clean = terminal_win(10, 1);
        let dirty = terminal_win(80, 2);

        cache.observe_estimate(&clean, 3);
        cache.observe_estimate(&dirty, 7);

        assert_eq!(
            cache
                .best_replayable_terminal_win
                .as_ref()
                .map(|witness| &witness.estimate),
            Some(&dirty),
            "the unrestricted incumbent may still prefer the higher-scoring win"
        );
        assert_eq!(
            cache
                .best_replayable_terminal_win_without_new_external_burden
                .as_ref()
                .map(|witness| (&witness.estimate, witness.nodes_generated_at_discovery)),
            Some((&clean, 3)),
            "clean acceptance must retain its own replayable witness"
        );
    }

    #[test]
    fn higher_scoring_spending_rollout_does_not_erase_the_potion_free_clean_witness() {
        let mut cache = RolloutCache::default();
        let potion_free = terminal_win(10, 0);
        let mut spending = terminal_win(80, 0);
        spending.potions_used = 1;

        cache.observe_estimate(&potion_free, 3);
        cache.observe_estimate(&spending, 7);

        assert_eq!(
            cache
                .best_replayable_terminal_win_without_new_external_burden
                .as_ref()
                .map(|witness| &witness.estimate),
            Some(&spending),
            "ordinary clean acceptance may retain the higher-scoring spending witness"
        );
        assert_eq!(
            cache
                .best_replayable_terminal_potion_free_win_without_new_external_burden
                .as_ref()
                .map(|witness| (&witness.estimate, witness.nodes_generated_at_discovery)),
            Some((&potion_free, 3)),
            "potion-free quality acceptance must retain its own replayable witness"
        );
    }
}
