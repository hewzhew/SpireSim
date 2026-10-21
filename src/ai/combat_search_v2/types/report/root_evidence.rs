use serde::{Deserialize, Serialize};

use super::super::{CombatSearchV2OutcomeOrderKeyReport, SearchTerminalLabel};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2AdvanceStop {
    CandidateSatisfied,
    QuantumNodeBudget,
    QuantumWallTime,
    FrontierExhausted,
    AlreadyComplete,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2RootMaterializationStatus {
    NotStarted,
    Partial,
    Complete,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2RootClosureStatus {
    ProvenClosed,
    NotProven,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2RootClosureBlocker {
    RootActionSurfaceNotFullyMaterialized,
    OpenFrontierWork,
    /// Historical schema-v20 evidence. This remains distinct because its
    /// exact-state count must not be reinterpreted as frontier work items.
    #[serde(rename = "open_concrete_work")]
    LegacyOpenConcreteWork,
    OpenPendingChoiceWork,
    UnresolvedLeaf,
    PendingChoiceOrderedVariantsOmitted,
    MaxActionsPerLine,
    EngineStepLimit,
    PotionBudget,
    HierarchicalTurnBoundaryPortfolioSelection,
    ProofDispositionsUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2RootActionIdentity {
    pub action_id: usize,
    pub action_key: String,
    pub action_debug: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2RootObservedValue {
    pub terminal: SearchTerminalLabel,
    pub outcome_order_key: CombatSearchV2OutcomeOrderKeyReport,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
    pub actions_taken: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2RootWorkEvidence {
    pub generated_concrete_nodes: u64,
    pub expanded_concrete_nodes: u64,
    #[serde(default)]
    pub expanded_turn_zero_nodes: u64,
    #[serde(default)]
    pub expanded_turn_one_nodes: u64,
    #[serde(default)]
    pub expanded_turn_two_or_later_nodes: u64,
    #[serde(default)]
    pub bulk_expanded_nodes_without_depth: u64,
    #[serde(default)]
    pub max_expanded_turn: u32,
    #[serde(default)]
    pub max_expanded_action_count: usize,
    #[serde(default)]
    pub open_work_items: usize,
    /// Historical schema-v20 exact-state census, retained only when loading
    /// old trace data. New reports leave it absent.
    #[serde(
        default,
        rename = "open_concrete_states",
        skip_serializing_if = "Option::is_none"
    )]
    pub legacy_open_concrete_states: Option<usize>,
    pub open_pending_choice_work_items: usize,
    pub best_exact_complete: Option<CombatSearchV2RootObservedValue>,
    pub best_exact_win: Option<CombatSearchV2RootObservedValue>,
    /// Observation of this root's priority-queue head, not an exhaustive
    /// maximum over every open work item.
    pub best_open_observed: Option<CombatSearchV2RootObservedValue>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2RootActionEvidence {
    pub rank: Option<usize>,
    pub root_action: CombatSearchV2RootActionIdentity,
    pub work: CombatSearchV2RootWorkEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2RootEvidenceSnapshot {
    pub ranking_policy: String,
    pub work_accounting_scope: String,
    #[serde(default)]
    pub scheduling_policy: String,
    #[serde(default)]
    pub scheduling_trigger: String,
    #[serde(default)]
    pub completed_comparison_rounds: u32,
    #[serde(default)]
    pub current_comparison_round: u32,
    #[serde(default)]
    pub current_scheduling_phase: String,
    #[serde(default)]
    pub current_comparison_round_complete: bool,
    #[serde(default)]
    pub current_round_expansions_per_action: u64,
    pub materialization: CombatSearchV2RootMaterializationStatus,
    pub closure_status: CombatSearchV2RootClosureStatus,
    pub closure_blockers: Vec<CombatSearchV2RootClosureBlocker>,
    pub leader: Option<CombatSearchV2RootActionIdentity>,
    pub contenders: Vec<CombatSearchV2RootActionEvidence>,
    pub unattributed: CombatSearchV2RootWorkEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2QuantumCounters {
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub pending_choice_prefixes_expanded: u64,
    pub rollout_promotion_actions_replayed: u64,
    pub frontier_work_items: usize,
    pub exact_state_keys: usize,
    pub rollout_cache_entries: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2QuantumEvidence {
    pub quantum_index: usize,
    pub requested_additional_nodes: usize,
    pub requested_soft_wall_time_ms: Option<u128>,
    pub stop: CombatSearchV2AdvanceStop,
    pub before: CombatSearchV2QuantumCounters,
    pub after: CombatSearchV2QuantumCounters,
    pub root_evidence: CombatSearchV2RootEvidenceSnapshot,
}
