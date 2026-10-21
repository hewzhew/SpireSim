mod counters;
mod frontier;
mod trajectories;

use std::collections::HashMap;

use super::super::frontier::{FrontierQueue, ResourceVector};
use super::super::*;
use super::best_trajectories::SearchTrajectoryBook;
use super::root_evidence::RootEvidenceBook;
use super::root_round_scheduler::RootRoundScheduler;
use super::turn_boundary_expansion::TurnBoundaryExpansionTracker;
use super::turn_plan_seeding::TurnPlanSeedTracker;

pub(super) struct SearchLoopState {
    pub(super) owns_engine_pending_choice_prefixes: bool,
    pub(super) plugins: CombatSearchPluginStack,
    pub(super) stats: CombatSearchV2Stats,
    pub(super) diagnostics: SearchDiagnosticsCollector,
    pub(super) exact_transpositions: HashMap<CombatExactStateKey, Vec<ResourceVector>>,
    pub(super) dominance: HashMap<CombatDominanceKey, Vec<ResourceVector>>,
    pub(super) rollout_cache: RolloutCache,
    pub(super) performance: CombatSearchV2PerformanceReport,
    pub(super) frontier: FrontierQueue,
    pub(super) turn_boundary_expansion_tracker: TurnBoundaryExpansionTracker,
    pub(super) turn_plan_seed_tracker: TurnPlanSeedTracker,
    pub(super) trajectories: SearchTrajectoryBook,
    pub(super) unresolved_leaf_count: u64,
    pub(super) max_actions_cut_count: u64,
    pub(super) engine_step_limit_count: u64,
    pub(super) potion_budget_cut_count: u64,
    pub(super) exhausted: bool,
    pub(super) accepted_complete_candidate: bool,
    pub(super) initial_external_burden_count: i32,
    pub(super) root_evidence: RootEvidenceBook,
    pub(super) root_round_scheduler: RootRoundScheduler,
    pub(super) completed_root_round_trajectories: Option<SearchTrajectoryBook>,
    pub(super) last_promoted_rollout_witness: Option<RolloutNodeEstimate>,
}

impl SearchLoopState {
    #[cfg(test)]
    pub(super) fn new(
        config: &CombatSearchV2Config,
        owns_engine_pending_choice_prefixes: bool,
        root: CombatPosition,
    ) -> Self {
        Self::new_with_priority_ablation(
            config,
            owns_engine_pending_choice_prefixes,
            root,
            CombatSearchV2PriorityAblation::Baseline,
        )
    }

    pub(super) fn new_with_priority_ablation(
        config: &CombatSearchV2Config,
        owns_engine_pending_choice_prefixes: bool,
        root: CombatPosition,
        priority_ablation: CombatSearchV2PriorityAblation,
    ) -> Self {
        let plugins = CombatSearchPluginStack::from_config(config);
        let initial_external_burden_count = outcome_score::external_burden_count(&root.combat);
        Self {
            owns_engine_pending_choice_prefixes,
            plugins,
            stats: CombatSearchV2Stats::default(),
            diagnostics: SearchDiagnosticsCollector::default(),
            exact_transpositions: HashMap::new(),
            dominance: HashMap::new(),
            rollout_cache: RolloutCache::new(
                plugins.rollout,
                config.rollout_max_evaluations,
                config.rollout_max_actions,
                config.rollout_beam_width,
                initial_external_burden_count,
            ),
            performance: CombatSearchV2PerformanceReport::default(),
            frontier: FrontierQueue::new_with_priority_ablation(priority_ablation),
            turn_boundary_expansion_tracker: TurnBoundaryExpansionTracker::default(),
            turn_plan_seed_tracker: TurnPlanSeedTracker::default(),
            trajectories: SearchTrajectoryBook::new(root),
            unresolved_leaf_count: 0,
            max_actions_cut_count: 0,
            engine_step_limit_count: 0,
            potion_budget_cut_count: 0,
            exhausted: false,
            accepted_complete_candidate: false,
            initial_external_burden_count,
            root_evidence: RootEvidenceBook::default(),
            root_round_scheduler: RootRoundScheduler::default(),
            completed_root_round_trajectories: None,
            last_promoted_rollout_witness: None,
        }
    }

    pub(super) fn begin_work_quantum(&mut self) {
        self.exhausted = false;
        self.stats.deadline_hit = false;
        self.stats.node_budget_hit = false;
        self.stats.action_prefix_budget_hit = false;
    }
}
