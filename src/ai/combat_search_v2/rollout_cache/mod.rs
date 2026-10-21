use std::collections::{BTreeMap, HashMap};

use crate::ai::combat_search_v2::rollout_profile::RolloutPerformanceCounters;
use crate::ai::combat_search_v2::rollout_scheduler::turn_beam_extension_budget;
use crate::ai::combat_state_key::CombatExactStateKey;

use super::*;

mod estimate;
mod policy;
mod report;

#[derive(Clone, Debug)]
pub(super) struct ReplayableTerminalWinWitness {
    pub(super) estimate: RolloutNodeEstimate,
    pub(super) nodes_generated_at_discovery: u64,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RolloutCache {
    pub(super) policy: CombatSearchRolloutPluginId,
    pub(super) max_evaluations: usize,
    pub(super) max_actions: usize,
    pub(super) beam_width: usize,
    pub(super) turn_beam_extension_budget: usize,
    pub(super) turn_beam_calls: u64,
    pub(super) turn_beam_conservative_anchor_present: u64,
    pub(super) turn_beam_conservative_anchor_selected: u64,
    pub(super) turn_beam_conservative_anchor_terminal_wins: u64,
    pub(super) turn_beam_extensions: u64,
    pub(super) turn_beam_extension_budget_skips: u64,
    pub(super) turn_beam_turn_plan_calls: u64,
    pub(super) turn_beam_turn_plan_inner_nodes_expanded: u64,
    pub(super) turn_beam_turn_plan_inner_nodes_generated: u64,
    pub(super) turn_beam_turn_plans_kept: u64,
    pub(super) turn_beam_turn_plans_kept_by_bucket: BTreeMap<&'static str, u64>,
    pub(super) turn_beam_terminal_candidates_kept: u64,
    pub(super) turn_beam_best_pv_len: usize,
    pub(super) turn_beam_best_pv_terminal: Option<SearchTerminalLabel>,
    pub(super) evaluations: u64,
    pub(super) cache_queries: u64,
    pub(super) cache_hits: u64,
    pub(super) cache_misses: u64,
    pub(super) cache_inserts: u64,
    pub(super) budget_skips: u64,
    pub(super) max_evaluation_budget_skips: u64,
    pub(super) deadline_budget_skips: u64,
    pub(super) truncated: u64,
    pub(super) terminal_wins: u64,
    pub(super) terminal_losses: u64,
    pub(super) rollouts_with_pending_choice: u64,
    pub(super) rollouts_stopped_on_high_fanout_pending_choice: u64,
    pub(super) pending_choice_actions_simulated: u64,
    pub(super) max_pending_choice_estimated_action_fanout: usize,
    pub(super) performance: RolloutPerformanceCounters,
    pub(super) cache: HashMap<CombatExactStateKey, RolloutNodeEstimate>,
    pub(super) initial_external_burden_count: i32,
    pub(super) best_replayable_terminal_win: Option<ReplayableTerminalWinWitness>,
    pub(super) best_replayable_terminal_win_without_new_external_burden:
        Option<ReplayableTerminalWinWitness>,
    pub(super) best_replayable_terminal_potion_free_win_without_new_external_burden:
        Option<ReplayableTerminalWinWitness>,
}

impl RolloutCache {
    pub(super) fn new(
        policy: impl Into<CombatSearchRolloutPluginId>,
        max_evaluations: usize,
        max_actions: usize,
        beam_width: usize,
        initial_external_burden_count: i32,
    ) -> Self {
        Self {
            policy: policy.into(),
            max_evaluations,
            max_actions,
            beam_width,
            initial_external_burden_count,
            turn_beam_extension_budget: turn_beam_extension_budget(max_evaluations, beam_width),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::policy::adaptive_no_potion_rollout_plugin;
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::PowerId;
    use crate::runtime::combat::{CombatState, Power, PowerPayload};
    use crate::state::core::EngineState;
    use crate::test_support::{blank_test_combat, test_monster};

    fn combat_with_owner_powers(powers: &[(PowerId, i32)]) -> CombatState {
        let mut combat = blank_test_combat();
        let mut owner = test_monster(EnemyId::Cultist);
        owner.id = 7;
        combat.entities.monsters = vec![owner];
        combat.entities.power_db.insert(
            7,
            powers
                .iter()
                .map(|(power_type, amount)| Power {
                    power_type: *power_type,
                    instance_id: None,
                    amount: *amount,
                    extra_data: 0,
                    payload: PowerPayload::None,
                    just_applied: false,
                })
                .collect(),
        );
        combat
    }

    #[test]
    fn adaptive_no_potion_rollout_uses_phase_aware_for_finite_survival_damage_mitigation() {
        let combined = combat_with_owner_powers(&[(PowerId::Fading, 5), (PowerId::Shifting, -1)]);
        let fading_only = combat_with_owner_powers(&[(PowerId::Fading, 5)]);
        let shifting_only = combat_with_owner_powers(&[(PowerId::Shifting, -1)]);

        assert_eq!(
            adaptive_no_potion_rollout_plugin(&test_search_node(combined)),
            CombatSearchRolloutPluginId::PhaseAwareNoPotion
        );
        assert_eq!(
            adaptive_no_potion_rollout_plugin(&test_search_node(fading_only)),
            CombatSearchRolloutPluginId::ConservativeNoPotion
        );
        assert_eq!(
            adaptive_no_potion_rollout_plugin(&test_search_node(shifting_only)),
            CombatSearchRolloutPluginId::ConservativeNoPotion
        );
    }

    #[test]
    fn adaptive_no_potion_rollout_uses_phase_aware_for_guardian_and_keeps_nob_conservative() {
        let mut guardian_combat = blank_test_combat();
        guardian_combat.entities.monsters = vec![test_monster(EnemyId::TheGuardian)];

        assert_eq!(
            adaptive_no_potion_rollout_plugin(&test_search_node(guardian_combat)),
            CombatSearchRolloutPluginId::PhaseAwareNoPotion
        );

        let mut nob_combat = blank_test_combat();
        nob_combat.entities.monsters = vec![test_monster(EnemyId::GremlinNob)];

        assert_eq!(
            adaptive_no_potion_rollout_plugin(&test_search_node(nob_combat)),
            CombatSearchRolloutPluginId::ConservativeNoPotion
        );
    }

    #[test]
    fn adaptive_no_potion_rollout_uses_phase_aware_for_bronze_automaton_mechanics() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::BronzeAutomaton)];

        assert_eq!(
            adaptive_no_potion_rollout_plugin(&test_search_node(combat)),
            CombatSearchRolloutPluginId::PhaseAwareNoPotion
        );
    }

    #[test]
    fn rollout_report_exposes_cache_audit_and_internal_performance_buckets() {
        let mut cache = RolloutCache::new(
            CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            384,
            80,
            3,
            0,
        );
        cache.cache_queries = 4;
        cache.cache_misses = 3;
        cache.cache_inserts = 2;
        cache.max_evaluation_budget_skips = 1;
        cache.deadline_budget_skips = 2;
        cache.performance.cache_lookup_elapsed_us = 11;
        cache.performance.policy_dispatch_elapsed_us = 17;
        cache.performance.no_potion_iterations = 23;
        cache.performance.no_potion_phase_profile_elapsed_us = 29;
        cache.performance.no_potion_legal_actions_elapsed_us = 31;
        cache.performance.no_potion_choose_action_elapsed_us = 37;
        cache.performance.no_potion_choose_ordering_elapsed_us = 39;
        cache.performance.no_potion_probe_elapsed_us = 40;
        cache.performance.no_potion_probe_score_calls = 5;
        cache.performance.no_potion_probe_actions_evaluated = 4;
        cache.performance.no_potion_probe_step_reuses = 3;
        cache.performance.no_potion_probe_engine_step_elapsed_us = 6;
        cache.performance.no_potion_probe_phase_profile_elapsed_us = 7;
        cache.performance.no_potion_probe_action_facts_elapsed_us = 8;
        cache.performance.no_potion_engine_step_elapsed_us = 41;
        cache.performance.no_potion_child_build_elapsed_us = 43;

        let report = cache.finish(None);

        assert_eq!(report.cache_queries, 4);
        assert_eq!(report.cache_misses, 3);
        assert_eq!(report.cache_inserts, 2);
        assert_eq!(report.max_evaluation_budget_skips, 1);
        assert_eq!(report.deadline_budget_skips, 2);
        assert_eq!(report.performance.cache_lookup_us, 11);
        assert_eq!(report.performance.policy_dispatch_us, 17);
        assert_eq!(report.performance.no_potion_iterations, 23);
        assert_eq!(report.performance.no_potion_phase_profile_us, 29);
        assert_eq!(report.performance.no_potion_legal_actions_us, 31);
        assert_eq!(report.performance.no_potion_choose_action_us, 37);
        assert_eq!(report.performance.no_potion_choose_ordering_us, 39);
        assert_eq!(report.performance.no_potion_probe_us, 40);
        assert_eq!(report.performance.no_potion_probe_score_calls, 5);
        assert_eq!(report.performance.no_potion_probe_actions_evaluated, 4);
        assert_eq!(report.performance.no_potion_probe_step_reuses, 3);
        assert_eq!(report.performance.no_potion_probe_engine_step_us, 6);
        assert_eq!(report.performance.no_potion_probe_phase_profile_us, 7);
        assert_eq!(report.performance.no_potion_probe_action_facts_us, 8);
        assert_eq!(report.performance.no_potion_engine_step_us, 41);
        assert_eq!(report.performance.no_potion_child_build_us, 43);
    }

    fn test_search_node(combat: CombatState) -> SearchNode {
        SearchNode {
            engine: EngineState::CombatPlayerTurn,
            combat,
            actions: Vec::new(),
            turn_prefix: TurnPrefixState::default(),
            initial_hp: 80,
            potions_used: 0,
            potions_discarded: 0,
            cards_played: 0,
            potion_tactical_priority: 0,
            last_turn_branch_priority: 0,
            action_prior_score: None,
            action_ordering_frontier_hint: 0,
            rollout_estimate: RolloutNodeEstimate::unevaluated(),
            root_lineage: Default::default(),
        }
    }
}
