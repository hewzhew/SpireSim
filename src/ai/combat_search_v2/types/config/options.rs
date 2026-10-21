use std::time::Duration;

use super::policies::{
    CombatSearchV2ChildRolloutPolicy, CombatSearchV2ExpansionPolicy,
    CombatSearchV2PhaseGuardPolicy, CombatSearchV2PotionPolicy, CombatSearchV2RolloutPolicy,
    CombatSearchV2SetupBiasPolicy, CombatSearchV2TurnPlanPolicy,
};
use super::prior::{CombatSearchV2RootActionPrior, CombatSearchV2TurnPlanPrior};
use super::satisfaction::CombatSearchV2Satisfaction;

#[derive(Clone, Debug)]
pub struct CombatSearchV2Config {
    pub max_nodes: usize,
    pub max_actions_per_line: usize,
    pub max_engine_steps_per_action: usize,
    pub wall_time: Option<Duration>,
    pub satisfaction: CombatSearchV2Satisfaction,
    pub input_label: Option<String>,
    pub potion_policy: CombatSearchV2PotionPolicy,
    pub max_potions_used: Option<u32>,
    pub allowed_potion_slots: Option<u64>,
    pub allow_potion_discard: Option<bool>,
    pub rollout_policy: CombatSearchV2RolloutPolicy,
    pub child_rollout_policy: CombatSearchV2ChildRolloutPolicy,
    pub rollout_max_evaluations: usize,
    pub rollout_max_actions: usize,
    pub rollout_beam_width: usize,
    pub expansion_policy: CombatSearchV2ExpansionPolicy,
    pub turn_plan_policy: CombatSearchV2TurnPlanPolicy,
    pub phase_guard_policy: CombatSearchV2PhaseGuardPolicy,
    pub setup_bias_policy: CombatSearchV2SetupBiasPolicy,
    pub turn_plan_probe_max_inner_nodes: Option<usize>,
    pub turn_plan_probe_max_partial_frontier_states: Option<usize>,
    pub turn_plan_probe_max_end_states: Option<usize>,
    pub turn_plan_probe_per_bucket_limit: Option<usize>,
    pub root_action_prior: Option<CombatSearchV2RootActionPrior>,
    pub turn_plan_prior: Option<CombatSearchV2TurnPlanPrior>,
}

impl Default for CombatSearchV2Config {
    fn default() -> Self {
        Self {
            max_nodes: 50_000,
            max_actions_per_line: 200,
            max_engine_steps_per_action: 250,
            wall_time: None,
            satisfaction: CombatSearchV2Satisfaction::default(),
            input_label: None,
            potion_policy: CombatSearchV2PotionPolicy::Never,
            max_potions_used: None,
            allowed_potion_slots: None,
            allow_potion_discard: None,
            rollout_policy: CombatSearchV2RolloutPolicy::EnemyMechanicsAdaptiveNoPotion,
            child_rollout_policy: CombatSearchV2ChildRolloutPolicy::default(),
            rollout_max_evaluations:
                crate::ai::combat_search_v2::rollout::DEFAULT_ROLLOUT_MAX_EVALUATIONS,
            rollout_max_actions: crate::ai::combat_search_v2::rollout::DEFAULT_ROLLOUT_MAX_ACTIONS,
            rollout_beam_width: crate::ai::combat_search_v2::rollout::DEFAULT_TURN_BEAM_WIDTH,
            expansion_policy: CombatSearchV2ExpansionPolicy::default(),
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::default(),
            phase_guard_policy: CombatSearchV2PhaseGuardPolicy::Default,
            setup_bias_policy: CombatSearchV2SetupBiasPolicy::Default,
            turn_plan_probe_max_inner_nodes: None,
            turn_plan_probe_max_partial_frontier_states: None,
            turn_plan_probe_max_end_states: None,
            turn_plan_probe_per_bucket_limit: None,
            root_action_prior: None,
            turn_plan_prior: None,
        }
    }
}
