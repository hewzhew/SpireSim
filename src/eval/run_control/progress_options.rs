use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::{
    CombatSearchProfile, CombatSearchV2ChildRolloutPolicy, CombatSearchV2PhaseGuardPolicy,
    CombatSearchV2PotionPolicy, CombatSearchV2RolloutPolicy, CombatSearchV2Satisfaction,
    CombatSearchV2SetupBiasPolicy,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AtomicCombatSearchOptionsV2 {
    pub profile: Option<CombatSearchProfile>,
    pub max_nodes: Option<usize>,
    pub max_actions_per_line: Option<usize>,
    pub max_engine_steps_per_action: Option<usize>,
    pub wall_ms: Option<u64>,
    pub satisfaction: Option<CombatSearchV2Satisfaction>,
    pub max_hp_loss: Option<RunControlHpLossLimit>,
    pub potion_policy: Option<CombatSearchV2PotionPolicy>,
    pub max_potions_used: Option<u32>,
    /// Optional exact slot mask used by this atomic search. `None` keeps
    /// every legal potion slot; `Some(0)` keeps no explicit potion action.
    pub allowed_potion_slots: Option<u64>,
    pub rollout_policy: Option<CombatSearchV2RolloutPolicy>,
    pub child_rollout_policy: Option<CombatSearchV2ChildRolloutPolicy>,
    pub rollout_max_evaluations: Option<usize>,
    pub rollout_max_actions: Option<usize>,
    pub rollout_beam_width: Option<usize>,
    pub turn_plan_policy: Option<crate::ai::combat_search_v2::CombatSearchV2TurnPlanPolicy>,
    pub phase_guard_policy: Option<CombatSearchV2PhaseGuardPolicy>,
    pub setup_bias_policy: Option<CombatSearchV2SetupBiasPolicy>,
    pub segment_mode: Option<RunControlCombatSegmentMode>,
    pub enable_legacy_no_win_rescue: bool,
    pub allow_smoke_bomb_survival_fallback: bool,
    pub work_quanta: Vec<AtomicCombatSearchQuantumV2>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtomicCombatSearchQuantumV2 {
    pub label: &'static str,
    pub additional_nodes: usize,
    pub soft_wall_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicCombatSearchAdvanceV2 {
    Pending,
    ReadyToFinish,
    AllowanceExhausted,
    GlobalDeadlineReached,
}

/// One bounded service grant for the resident witness portfolio.
///
/// Generation work counts exact candidate-generation effort. It is not an
/// atomic-v2 expanded-node budget and must never be compared as if it were.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleCombatWitnessQuantumV1 {
    pub label: &'static str,
    pub additional_generation_work: usize,
    pub soft_wall_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OracleCombatWitnessAdvanceV1 {
    Pending,
    ReadyToFinish,
    AllowanceExhausted,
    GlobalDeadlineReached,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunControlCombatSegmentMode {
    TurnBoundary,
    NonBossTurnBoundary,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunControlHpLossLimit {
    Limit(u32),
    Unlimited,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunControlAutoStepOptions {
    pub search: AtomicCombatSearchOptionsV2,
    pub route: RunControlRouteAutomationMode,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RunControlRouteAutomationMode {
    #[default]
    Manual,
    Policy,
}
