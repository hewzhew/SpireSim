use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sts_combat_planner::LocalTurnGraphRootActionFamilySnapshot;

use crate::content::identity::gameplay_display_label_v1;
use crate::content::potions::Potion;
use crate::content::relics::RelicState;
use crate::runtime::combat::CombatCard;
use crate::runtime::monster_move::MonsterMoveSpec;
use crate::sim::combat::CombatPosition;
use crate::state::core::{ClientInput, EngineState};
use crate::state::rewards::RewardState;

use crate::eval::combat_case::{
    CombatCase, CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary, CombatCaseSource,
    CombatCaseWitnessBudgetV1,
};
use crate::eval::combat_case_context::capture_oracle_analysis_combat_case_production_context_v2;

use crate::eval::run_control::{
    build_decision_surface, exact_campfire_policy_audit_v1, exact_card_reward_policy_audit_v1,
    exact_route_policy_audit_v1, exact_shop_policy_audit_v1,
    seed_oracle_run_explorer_from_checkpoint_v1, strategic_combat_survival_hp_loss_limit_v1,
    CombatAutomationMonsterStateV1, CombatAutomationTrajectoryRecordV1, DecisionCandidateKey,
    ExactCampfirePolicyAuditV1, ExactCardRewardPolicyAuditV1, ExactRoutePolicyAuditV1,
    ExactShopPolicyAuditV1, LazyOracleRunDecisionV1, OracleCombatSearchResumeKindV1,
    OracleCombatWitnessAdvanceV1, OracleCombatWitnessCandidateDispositionV1,
    OracleCombatWitnessCheckpointV1, OracleCombatWitnessQuantumV1,
    OracleResidentCombatWitnessEvidenceV1, OracleResidentCombatWitnessJobV1, OracleRunBoundaryV1,
    OracleRunBranchV1, OracleRunCombatEvidenceKindV1, OracleRunCombatWitnessBudgetsV1,
    OracleRunDecisionAnnotationFnV1, OracleRunExplorerCheckpointV1, OracleRunExplorerV1,
    OracleRunReplayStepV1, OracleRunWorkKindV1, RunControlHpLossLimit,
    RunControlSessionCheckpointV1, RunControlTraceAnnotationV1, RunDecisionAction,
    RunPolicyCandidateV1, RunPolicyPriorFnV1, RunProgressJournalV1, RunProgressStepV1,
};

#[path = "oracle_analysis_session/card_reward_path.rs"]
mod card_reward_path;
#[path = "oracle_analysis_session/combat_scratch.rs"]
mod combat_scratch;

pub use card_reward_path::{
    OracleAnalysisCardRewardApplicationUnknownV1, OracleAnalysisCardRewardApplicationV1,
    OracleAnalysisCardRewardPathAuditV1, OracleAnalysisCardRewardPathBoundaryV1,
    ORACLE_ANALYSIS_CARD_REWARD_PATH_AUDIT_SCHEMA_NAME,
    ORACLE_ANALYSIS_CARD_REWARD_PATH_AUDIT_SCHEMA_VERSION,
};
use combat_scratch::OracleAnalysisCombatLineLabV1;
pub use combat_scratch::{
    OracleAnalysisCombatLineLabActionSummaryV1, OracleAnalysisCombatLineLabActionV1,
    OracleAnalysisCombatLineLabBaselineSourceV1, OracleAnalysisCombatLineLabCardCandidateV1,
    OracleAnalysisCombatLineLabCompareV1, OracleAnalysisCombatLineLabDecisionDeltaV1,
    OracleAnalysisCombatLineLabDivergenceV1, OracleAnalysisCombatLineLabFrameV1,
    OracleAnalysisCombatLineLabLineSummaryV1, OracleAnalysisCombatLineLabLineV1,
    OracleAnalysisCombatLineLabLocationV1, OracleAnalysisCombatLineLabOpenV1,
    OracleAnalysisCombatLineLabPlayCardResultV1, OracleAnalysisCombatLineLabPotionCandidateV1,
    OracleAnalysisCombatLineLabTurnSummaryV1, OracleAnalysisCombatLineLabUsePotionResultV1,
    OracleAnalysisCombatScratchActionSelectorV1, OracleAnalysisCombatScratchActionSurfaceV1,
    OracleAnalysisCombatScratchActionV1, OracleAnalysisCombatScratchCardV1,
    OracleAnalysisCombatScratchCheckpointV1, OracleAnalysisCombatScratchContextV1,
    OracleAnalysisCombatScratchDecisionActionV1,
    OracleAnalysisCombatScratchDecisionSelectionFamilyV1,
    OracleAnalysisCombatScratchDecisionViewV1, OracleAnalysisCombatScratchMonsterV1,
    OracleAnalysisCombatScratchNodeCheckpointV1, OracleAnalysisCombatScratchPlayerV1,
    OracleAnalysisCombatScratchPositionV1, OracleAnalysisCombatScratchSearchExitV1,
    OracleAnalysisCombatScratchSearchReportV1, OracleAnalysisCombatScratchSearchRequestV1,
    OracleAnalysisCombatScratchSelectionFamilyV1, OracleAnalysisCombatScratchTreeNodeV1,
    OracleAnalysisCombatScratchTreeV1, OracleAnalysisCombatScratchViewV1,
    ORACLE_ANALYSIS_COMBAT_SCRATCH_SCHEMA_NAME, ORACLE_ANALYSIS_COMBAT_SCRATCH_SCHEMA_VERSION,
};

pub const ORACLE_ANALYSIS_SESSION_SCHEMA_NAME: &str = "OracleAnalysisSession";
pub const ORACLE_ANALYSIS_SESSION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisEdgeKindV1 {
    Decision,
    CombatWitness,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisEdgeV1 {
    pub edge_id: u64,
    pub parent_node_id: usize,
    pub child_node_id: usize,
    pub kind: OracleAnalysisEdgeKindV1,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice_ref: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisChoiceViewV1 {
    pub choice_ref: String,
    pub kind: OracleRunWorkKindV1,
    pub candidate_id: String,
    pub label: String,
    pub action: RunDecisionAction,
    pub owner_rank: u64,
    pub path_discrepancy: u64,
    pub path_negative_log_policy: f64,
    pub annotation: Option<RunControlTraceAnnotationV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisNodeSummaryV1 {
    pub node_id: usize,
    pub canonical_parent_node_id: Option<usize>,
    pub boundary: OracleRunBoundaryV1,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub replay_len: usize,
    pub is_cursor: bool,
    pub is_mainline_tip: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisTreeViewV1 {
    pub roots: Vec<usize>,
    pub cursor_node_id: usize,
    pub mainline_node_id: usize,
    pub nodes: Vec<OracleAnalysisNodeSummaryV1>,
    pub edges: Vec<OracleAnalysisEdgeV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisChildViewV1 {
    pub edge_id: u64,
    pub child_node_id: usize,
    pub kind: OracleAnalysisEdgeKindV1,
    pub label: String,
    pub is_on_mainline: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisCombatStageExitV1 {
    Active,
    ProbeWorkBudgetReached,
    ProbeWallReached,
    ProbeStageExhausted,
    ProbeNoProgress,
    PromotedForReservedQuantum,
    PromotedAfterReadyToFinish,
    PromotedAfterAllowanceExhausted,
    SearchPending,
    BudgetUnknown,
    BoundaryReached,
    ExhaustiveRefutation,
    SetupOrMechanicsError,
}

fn turn_graph_portfolio_engine_v1() -> crate::ai::combat_witness_contract::CombatWitnessEngineV1 {
    crate::ai::combat_witness_contract::CombatWitnessEngineV1::TurnGraphPortfolioV1
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatGuideServiceBiasV1 {
    pub lane: u32,
    pub extra_services_per_cycle: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatStageTraceV1 {
    #[serde(default = "turn_graph_portfolio_engine_v1")]
    pub witness_engine: crate::ai::combat_witness_contract::CombatWitnessEngineV1,
    pub stage: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_service_bias: Option<OracleAnalysisCombatGuideServiceBiasV1>,
    pub max_potions_used: Option<u32>,
    pub allowed_potion_slots: Option<u64>,
    pub potion_spend_requires_satisfaction: bool,
    pub historical_generation_work_at_entry: u64,
    pub generation_work: u64,
    pub local_generation_work: u64,
    pub discrepancy_generation_work: u64,
    pub exact_states: usize,
    pub completed_turn_options: usize,
    #[serde(default)]
    pub public_plan_turn_proposal_enabled: bool,
    #[serde(default)]
    pub public_plan_turn_proposal_attempts: usize,
    #[serde(default)]
    pub public_plan_turn_proposal_applied_transitions: usize,
    #[serde(default)]
    pub public_plan_turn_proposal_completed_options: usize,
    #[serde(default)]
    pub public_plan_turn_proposal_retained_options: usize,
    #[serde(default)]
    pub public_plan_turn_proposal_grounding_failures: usize,
    #[serde(default)]
    pub plan_prefix_proposals: usize,
    #[serde(default)]
    pub plan_prefix_proposed_turns: usize,
    #[serde(default)]
    pub plan_prefix_proposed_actions: usize,
    #[serde(default)]
    pub plan_prefix_proposal_rejections: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_candidate_final_hp: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_candidate_action_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_candidate_potions_used: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_candidate_potion_slots: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_candidate_satisfies_satisfaction: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_candidate_disposition: Option<OracleCombatWitnessCandidateDispositionV1>,
    pub incumbent_discovery_source: Option<sts_combat_planner::OracleCombatWitnessDiscoverySource>,
    pub incumbent_final_hp: Option<i32>,
    pub incumbent_action_count: Option<usize>,
    pub incumbent_potions_used: Option<u32>,
    pub incumbent_potion_slots: Option<u64>,
    pub incumbent_satisfies_satisfaction: Option<bool>,
    pub incumbent_ends_quality_refinement: Option<bool>,
    pub remaining_generation_work: usize,
    pub remaining_wall_ms: Option<u64>,
    pub last_status: Option<String>,
    pub exit: OracleAnalysisCombatStageExitV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
/// Cumulative and currently retained search evidence for one combat boundary.
///
/// Historical work remains charged across resumptions, while the local and
/// discrepancy fields describe the live frontiers retained by this process.
/// This report observes budget use; it does not grant additional search work.
pub struct OracleAnalysisCombatProgressV1 {
    pub witness_engine: crate::ai::combat_witness_contract::CombatWitnessEngineV1,
    /// Exact combat identity shared by every stage entry below.
    pub root_exact_state_hash: String,
    /// Completed stages followed by the current/final stage snapshot.
    pub stage_trace: Vec<OracleAnalysisCombatStageTraceV1>,
    /// Zero is the conserving/low-fidelity challenge; later stages use the
    /// configured full combat policy.
    pub search_stage: u8,
    /// Exact cap owned by the resident tactical search. Stage zero uses zero
    /// when the strategic conserving challenge applies.
    pub max_potions_used: Option<u32>,
    /// Exact potion slots admitted by the resident portfolio. `None` means
    /// every otherwise legal slot remains available.
    pub allowed_potion_slots: Option<u64>,
    /// A spending witness must satisfy the configured strategic target before
    /// it may replace the retained potion-free incumbent.
    pub potion_spend_requires_satisfaction: bool,
    /// Work charged by prior resident searches and preserved across resumes.
    pub historical_generation_work: u64,
    pub current_search_generation_work: u64,
    pub generation_work: u64,
    pub local_generation_work: u64,
    pub discrepancy_generation_work: u64,
    pub exact_states: usize,
    pub local_exact_states: usize,
    pub discrepancy_exact_states: usize,
    pub completed_turn_options: usize,
    pub public_plan_turn_proposal_enabled: bool,
    pub public_plan_turn_proposal_attempts: usize,
    pub public_plan_turn_proposal_applied_transitions: usize,
    pub public_plan_turn_proposal_completed_options: usize,
    pub public_plan_turn_proposal_retained_options: usize,
    pub public_plan_turn_proposal_grounding_failures: usize,
    pub retained_state_work: usize,
    pub local_retained_state_work: usize,
    pub discrepancy_retained_state_work: usize,
    pub root_state: Option<sts_combat_planner::OracleCombatWitnessStateProgressSnapshot>,
    pub max_player_turn: u32,
    pub deepest_survival_state: Option<sts_combat_planner::OracleCombatDeepStateSnapshot>,
    pub deepest_progress_state: Option<sts_combat_planner::OracleCombatDeepStateSnapshot>,
    pub deepest_survival_actions: Vec<sts_combat_planner::TurnOptionAction>,
    pub deepest_progress_actions: Vec<sts_combat_planner::TurnOptionAction>,
    pub recent_turn_survival_envelope: Vec<sts_combat_planner::OracleCombatDeepStateSnapshot>,
    pub pending_witness_replay: bool,
    /// Diagnostic counts for the bounded typed-plan prefix materialized into
    /// the ordinary exact local graph before portfolio service begins.
    pub plan_prefix_proposals: usize,
    pub plan_prefix_proposed_turns: usize,
    pub plan_prefix_proposed_actions: usize,
    pub plan_prefix_proposal_rejections: usize,
    pub local_candidate_final_hp: Option<i32>,
    pub local_candidate_action_count: Option<usize>,
    pub local_candidate_potions_used: Option<u32>,
    pub local_candidate_potion_slots: Option<u64>,
    pub local_candidate_satisfies_satisfaction: Option<bool>,
    pub local_candidate_disposition: Option<OracleCombatWitnessCandidateDispositionV1>,
    pub incumbent_discovery_source: Option<sts_combat_planner::OracleCombatWitnessDiscoverySource>,
    pub incumbent_final_hp: Option<i32>,
    pub incumbent_hp_loss: Option<i32>,
    pub incumbent_action_count: Option<usize>,
    pub incumbent_potions_used: Option<u32>,
    pub incumbent_potion_slots: Option<u64>,
    pub incumbent_satisfies_satisfaction: Option<bool>,
    pub incumbent_ends_quality_refinement: Option<bool>,
    pub quantum_count: usize,
    pub remaining_generation_work: usize,
    pub remaining_wall_ms: Option<u64>,
    pub resume_kind: OracleCombatSearchResumeKindV1,
    pub restart_count: usize,
    pub last_status: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisMonsterViewV1 {
    pub slot: u8,
    pub label: String,
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub planned_move_id: u8,
    pub intent: Option<MonsterMoveSpec>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisEncounterViewV1 {
    pub turn: u32,
    pub phase: String,
    pub energy: u8,
    pub player_block: i32,
    pub hand: Vec<CombatCard>,
    pub draw_pile_count: usize,
    pub discard_pile_count: usize,
    pub exhaust_pile_count: usize,
    pub is_elite: bool,
    pub is_boss: bool,
    pub monsters: Vec<OracleAnalysisMonsterViewV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatTurnV1 {
    pub turn: u32,
    pub start_hp: i32,
    pub end_hp: i32,
    pub hp_loss: i32,
    pub ended_turn: bool,
    pub actions: Vec<String>,
    pub player_block_after: i32,
    pub monsters_after: Vec<CombatAutomationMonsterStateV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatSummaryV1 {
    pub node_id: usize,
    pub parent_node_id: usize,
    pub encounter_start_hp: i32,
    pub encounter_start_max_hp: i32,
    pub combat_end_hp: i32,
    pub post_combat_hp: i32,
    pub post_combat_max_hp: i32,
    pub combat_hp_loss: i32,
    pub post_combat_healing: i32,
    pub action_count: usize,
    pub turns: Vec<OracleAnalysisCombatTurnV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisEventViewV1 {
    pub id: String,
    pub screen: usize,
    pub completed: bool,
    pub combat_pending: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisNodeViewV1 {
    pub node_id: usize,
    pub canonical_parent_node_id: Option<usize>,
    pub is_cursor: bool,
    pub is_on_mainline: bool,
    pub boundary: OracleRunBoundaryV1,
    pub state_fingerprint: String,
    pub neow_root_label: String,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub keys: [bool; 3],
    pub deck: Vec<CombatCard>,
    pub relics: Vec<RelicState>,
    pub potions: Vec<Option<Potion>>,
    pub reward: Option<RewardState>,
    pub replay_len: usize,
    pub recent_replay: Vec<OracleRunReplayStepV1>,
    pub choices: Vec<OracleAnalysisChoiceViewV1>,
    pub children: Vec<OracleAnalysisChildViewV1>,
    pub event: Option<OracleAnalysisEventViewV1>,
    pub encounter: Option<OracleAnalysisEncounterViewV1>,
    pub combat: Option<OracleAnalysisCombatProgressV1>,
}

fn oracle_analysis_choice_label(deck: &[CombatCard], choice: &LazyOracleRunDecisionV1) -> String {
    let RunDecisionAction::Input(ClientInput::SubmitSelection(resolution)) = &choice.action else {
        return choice.label.clone();
    };
    let selected = resolution
        .selected_card_lineages()
        .into_iter()
        .map(|lineage| {
            deck.iter()
                .find(|card| card.lineage == lineage)
                .map(|card| {
                    let upgrade = if card.upgrades == 0 {
                        String::new()
                    } else {
                        format!("+{}", card.upgrades)
                    };
                    format!(
                        "{}{} (#{lineage})",
                        gameplay_display_label_v1(card.id),
                        upgrade
                    )
                })
                .unwrap_or_else(|| format!("card #{lineage}"))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        choice.label.clone()
    } else {
        format!("Select {}", selected.join(", "))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum OracleAnalysisAdvanceStatusV1 {
    SearchPending,
    BoundaryReached { child_node_id: usize },
    BudgetUnknown,
    ExhaustiveRefutation,
    SetupOrMechanicsError,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisAdvanceReportV1 {
    pub source_node_id: usize,
    pub status: OracleAnalysisAdvanceStatusV1,
    pub quanta_served: usize,
    pub elapsed_ms: u64,
    pub combat: Option<OracleAnalysisCombatProgressV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisAdvanceRequestV1 {
    pub max_quanta: usize,
    pub quantum_generation_work: usize,
    pub quantum_ms: Option<u64>,
    pub wall_ms: Option<u64>,
    /// Continue past an insufficient verified witness until the configured
    /// strategic quality is reached.
    #[serde(default)]
    pub improve_incumbent: bool,
}

impl Default for OracleAnalysisAdvanceRequestV1 {
    fn default() -> Self {
        Self {
            max_quanta: 1,
            quantum_generation_work: 50_000,
            quantum_ms: Some(1_000),
            wall_ms: None,
            improve_incumbent: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OracleAnalysisCombatProbeRequestV1 {
    /// Maximum additional portfolio generation work charged by this call.
    pub generation_work: usize,
    /// Preemption granularity used to rotate the current stage's portfolio.
    pub quantum_generation_work: usize,
    /// Total wall deadline for this probe.
    pub wall_ms: u64,
}

impl Default for OracleAnalysisCombatProbeRequestV1 {
    fn default() -> Self {
        Self {
            generation_work: 4_096,
            quantum_generation_work: 256,
            wall_ms: 1_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisCombatProbeStopV1 {
    WorkBudgetReached,
    WallReached,
    StageExhausted,
    NoProgress,
}

#[derive(Clone, Debug, Serialize)]
pub struct OracleAnalysisCombatProbeReportV1 {
    pub source_node_id: usize,
    pub stop: OracleAnalysisCombatProbeStopV1,
    pub generation_work_requested: usize,
    pub generation_work_consumed: u64,
    pub quanta_served: usize,
    pub elapsed_ms: u64,
    pub combat: OracleAnalysisCombatProgressV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatJobCheckpointV1 {
    pub branch_id: usize,
    /// Older analysis artifacts predate staged resident jobs and always
    /// restored their work with the configured full combat policy.
    #[serde(default = "default_oracle_analysis_combat_stage")]
    pub stage: u8,
    #[serde(default)]
    pub completed_stage_trace: Vec<OracleAnalysisCombatStageTraceV1>,
    pub work: OracleCombatWitnessCheckpointV1,
}

const fn default_oracle_analysis_combat_stage() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisSessionCheckpointV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub cursor_node_id: usize,
    pub cursor_edge_path: Vec<u64>,
    pub mainline_node_id: usize,
    pub mainline_edge_path: Vec<u64>,
    pub next_edge_id: u64,
    pub edges: Vec<OracleAnalysisEdgeV1>,
    pub explorer: OracleRunExplorerCheckpointV1,
    pub combat_jobs: Vec<OracleAnalysisCombatJobCheckpointV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_scratch: Option<OracleAnalysisCombatScratchCheckpointV1>,
}

pub struct OracleAnalysisSessionV1 {
    explorer: OracleRunExplorerV1,
    cursor_node_id: usize,
    cursor_edge_path: Vec<u64>,
    mainline_node_id: usize,
    mainline_edge_path: Vec<u64>,
    next_edge_id: u64,
    edges: Vec<OracleAnalysisEdgeV1>,
    combat_jobs: BTreeMap<usize, OracleAnalysisCombatJobV1>,
    combat_scratch: Option<OracleAnalysisCombatLineLabV1>,
    combat_budgets: OracleRunCombatWitnessBudgetsV1,
    decision_prior: Option<RunPolicyPriorFnV1>,
    decision_annotation: Option<OracleRunDecisionAnnotationFnV1>,
}

struct OracleAnalysisCombatJobV1 {
    stage: u8,
    completed_stage_trace: Vec<OracleAnalysisCombatStageTraceV1>,
    work: OracleResidentCombatWitnessJobV1,
}

impl OracleAnalysisSessionV1 {
    pub fn from_explorer(
        mut explorer: OracleRunExplorerV1,
        preferred_cursor_node_id: Option<usize>,
        combat_budgets: OracleRunCombatWitnessBudgetsV1,
        decision_prior: Option<RunPolicyPriorFnV1>,
        decision_annotation: Option<OracleRunDecisionAnnotationFnV1>,
    ) -> Result<Self, String> {
        let cursor_node_id = preferred_cursor_node_id
            .filter(|branch_id| {
                explorer
                    .branches
                    .iter()
                    .any(|branch| branch.branch_id == *branch_id)
            })
            .or_else(|| {
                explorer
                    .branches
                    .iter()
                    .max_by_key(|branch| {
                        (
                            branch.session.run_state.act_num,
                            branch.session.run_state.floor_num,
                            branch.journal.len(),
                            branch.branch_id,
                        )
                    })
                    .map(|branch| branch.branch_id)
            })
            .ok_or_else(|| "oracle analysis session requires at least one branch".to_string())?;
        let combat_jobs = explorer
            .drain_pending_combats()
            .into_iter()
            .map(|(branch_id, stage, work)| {
                (
                    branch_id,
                    OracleAnalysisCombatJobV1 {
                        stage,
                        completed_stage_trace: Vec::new(),
                        work,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut session = Self {
            explorer,
            cursor_node_id,
            cursor_edge_path: Vec::new(),
            mainline_node_id: cursor_node_id,
            mainline_edge_path: Vec::new(),
            next_edge_id: 0,
            edges: Vec::new(),
            combat_jobs,
            combat_scratch: None,
            combat_budgets,
            decision_prior,
            decision_annotation,
        };
        session.seed_canonical_edges();
        session.cursor_edge_path = session.path_to_node(cursor_node_id).ok_or_else(|| {
            format!("analysis cursor node {cursor_node_id} is not reachable from any root")
        })?;
        session.mainline_edge_path = session.cursor_edge_path.clone();
        Ok(session)
    }

    pub fn restore(
        checkpoint: OracleAnalysisSessionCheckpointV1,
        combat_budgets: OracleRunCombatWitnessBudgetsV1,
        decision_prior: Option<RunPolicyPriorFnV1>,
        decision_annotation: Option<OracleRunDecisionAnnotationFnV1>,
    ) -> Result<Self, String> {
        if checkpoint.schema_name != ORACLE_ANALYSIS_SESSION_SCHEMA_NAME
            || checkpoint.schema_version != ORACLE_ANALYSIS_SESSION_SCHEMA_VERSION
        {
            return Err("unsupported oracle analysis session schema".to_string());
        }
        let explorer =
            seed_oracle_run_explorer_from_checkpoint_v1(checkpoint.explorer, &combat_budgets)?;
        let mut combat_jobs = BTreeMap::new();
        for saved in checkpoint.combat_jobs {
            let branch = explorer
                .branches
                .iter()
                .find(|branch| branch.branch_id == saved.branch_id)
                .ok_or_else(|| {
                    format!(
                        "analysis combat job references missing node {}",
                        saved.branch_id
                    )
                })?;
            let options =
                combat_budgets.for_session_stage_restore(&branch.session, saved.stage, &saved.work);
            let work = OracleResidentCombatWitnessJobV1::restore(
                &branch.session,
                options,
                saved.work,
                combat_budgets.guidance_bundle.as_deref(),
            )?;
            let job = OracleAnalysisCombatJobV1 {
                stage: saved.stage,
                completed_stage_trace: saved.completed_stage_trace,
                work,
            };
            if combat_jobs.insert(saved.branch_id, job).is_some() {
                return Err(format!(
                    "analysis checkpoint duplicated combat node {}",
                    saved.branch_id
                ));
            }
        }
        let combat_scratch = checkpoint
            .combat_scratch
            .map(|saved| {
                let branch = explorer
                    .branches
                    .iter()
                    .find(|branch| branch.branch_id == saved.run_node_id)
                    .ok_or_else(|| {
                        format!(
                            "analysis combat scratch references missing node {}",
                            saved.run_node_id
                        )
                    })?;
                let root = branch.session.current_active_combat_position()?;
                let context = OracleAnalysisCombatScratchContextV1 {
                    act: branch.session.run_state.act_num,
                    floor: branch.session.run_state.floor_num,
                    gold: branch.session.run_state.gold,
                };
                OracleAnalysisCombatLineLabV1::restore(saved, context, root)
            })
            .transpose()?;
        let session = Self {
            explorer,
            cursor_node_id: checkpoint.cursor_node_id,
            cursor_edge_path: checkpoint.cursor_edge_path,
            mainline_node_id: checkpoint.mainline_node_id,
            mainline_edge_path: checkpoint.mainline_edge_path,
            next_edge_id: checkpoint.next_edge_id,
            edges: checkpoint.edges,
            combat_jobs,
            combat_scratch,
            combat_budgets,
            decision_prior,
            decision_annotation,
        };
        session.validate_navigation_state()?;
        Ok(session)
    }

    pub fn checkpoint(&self) -> Result<OracleAnalysisSessionCheckpointV1, String> {
        self.validate_navigation_state()?;
        Ok(OracleAnalysisSessionCheckpointV1 {
            schema_name: ORACLE_ANALYSIS_SESSION_SCHEMA_NAME.to_string(),
            schema_version: ORACLE_ANALYSIS_SESSION_SCHEMA_VERSION,
            cursor_node_id: self.cursor_node_id,
            cursor_edge_path: self.cursor_edge_path.clone(),
            mainline_node_id: self.mainline_node_id,
            mainline_edge_path: self.mainline_edge_path.clone(),
            next_edge_id: self.next_edge_id,
            edges: self.edges.clone(),
            explorer: self.explorer.analysis_checkpoint()?,
            combat_jobs: self
                .combat_jobs
                .iter()
                .map(|(branch_id, job)| OracleAnalysisCombatJobCheckpointV1 {
                    branch_id: *branch_id,
                    stage: job.stage,
                    completed_stage_trace: job.completed_stage_trace.clone(),
                    work: job.work.checkpoint(),
                })
                .collect(),
            combat_scratch: self
                .combat_scratch
                .as_ref()
                .map(OracleAnalysisCombatLineLabV1::checkpoint),
        })
    }

    pub fn cursor_node_id(&self) -> usize {
        self.cursor_node_id
    }

    pub fn mainline_node_id(&self) -> usize {
        self.mainline_node_id
    }

    pub fn root_node_ids(&self) -> Vec<usize> {
        let branch_ids = self
            .explorer
            .branches
            .iter()
            .map(|branch| branch.branch_id)
            .collect::<BTreeSet<_>>();
        self.explorer
            .branches
            .iter()
            .filter(|branch| {
                branch
                    .parent_branch_id
                    .is_none_or(|parent| !branch_ids.contains(&parent))
            })
            .map(|branch| branch.branch_id)
            .collect()
    }

    pub fn focus_node(&mut self, node_id: usize) -> Result<(), String> {
        self.require_branch(node_id)?;
        self.cursor_node_id = node_id;
        self.cursor_edge_path = self
            .path_to_node(node_id)
            .ok_or_else(|| format!("analysis node {node_id} is not reachable from any root"))?;
        Ok(())
    }

    pub fn follow_edge(&mut self, edge_id: u64) -> Result<(), String> {
        let edge = self
            .edges
            .iter()
            .find(|edge| edge.edge_id == edge_id)
            .ok_or_else(|| format!("unknown oracle analysis edge {edge_id}"))?;
        if edge.parent_node_id != self.cursor_node_id {
            return Err(format!(
                "analysis edge {edge_id} starts at node {}, cursor is node {}",
                edge.parent_node_id, self.cursor_node_id
            ));
        }
        self.cursor_node_id = edge.child_node_id;
        self.cursor_edge_path.push(edge.edge_id);
        Ok(())
    }

    pub fn back(&mut self) -> Result<usize, String> {
        let edge_id = self
            .cursor_edge_path
            .pop()
            .ok_or_else(|| "oracle analysis cursor is already at a root".to_string())?;
        let edge = self
            .edges
            .iter()
            .find(|edge| edge.edge_id == edge_id)
            .ok_or_else(|| format!("analysis cursor references missing edge {edge_id}"))?;
        self.cursor_node_id = edge.parent_node_id;
        Ok(self.cursor_node_id)
    }

    pub fn promote_cursor(&mut self) {
        self.mainline_node_id = self.cursor_node_id;
        self.mainline_edge_path = self.cursor_edge_path.clone();
    }

    pub fn replay(&self, node_id: usize) -> Result<Vec<OracleRunReplayStepV1>, String> {
        Ok(self.require_branch(node_id)?.replay.clone())
    }

    pub fn journal_entries(&self, node_id: usize) -> Result<&[RunProgressStepV1], String> {
        Ok(self.require_branch(node_id)?.journal.entries())
    }

    pub fn continuation_parts(
        &self,
        node_id: usize,
    ) -> Result<(RunProgressJournalV1, RunControlSessionCheckpointV1), String> {
        let branch = self.require_branch(node_id)?;
        Ok((
            branch.journal.clone(),
            RunControlSessionCheckpointV1::from_session(&branch.session),
        ))
    }

    pub fn has_resident_combat_search(&self, node_id: usize) -> Result<bool, String> {
        self.require_branch(node_id)?;
        Ok(self.combat_jobs.contains_key(&node_id))
    }

    pub fn combat_trajectory(
        &self,
        node_id: usize,
    ) -> Result<Option<&CombatAutomationTrajectoryRecordV1>, String> {
        let branch = self.require_branch(node_id)?;
        Ok(branch
            .journal
            .entries()
            .iter()
            .rev()
            .find_map(RunProgressStepV1::as_combat_resolution)
            .map(|resolution| &resolution.trajectory)
            .or_else(|| branch.session.last_combat_automation_trajectory()))
    }

    pub fn combat_summary(&self, node_id: usize) -> Result<OracleAnalysisCombatSummaryV1, String> {
        let branch = self.require_branch(node_id)?;
        let parent_node_id = branch
            .parent_branch_id
            .ok_or_else(|| format!("oracle node {node_id} has no parent combat boundary"))?;
        let parent = self.require_branch(parent_node_id)?;
        let trajectory = self
            .combat_trajectory(node_id)?
            .ok_or_else(|| format!("oracle node {node_id} has no recorded combat trajectory"))?;
        let encounter_start_hp = parent.session.run_state.current_hp;
        let encounter_start_max_hp = parent.session.run_state.max_hp;
        let mut last_hp = encounter_start_hp;
        let mut active_turn: Option<OracleAnalysisCombatTurnV1> = None;
        let mut turns = Vec::new();

        for action in &trajectory.actions {
            let turn = action
                .opportunity_before
                .as_ref()
                .map(|opportunity| opportunity.turn)
                .unwrap_or_else(|| active_turn.as_ref().map(|turn| turn.turn).unwrap_or(0));
            if active_turn
                .as_ref()
                .is_some_and(|summary| summary.turn != turn)
            {
                turns.push(active_turn.take().expect("active turn checked above"));
            }
            let summary = active_turn.get_or_insert_with(|| OracleAnalysisCombatTurnV1 {
                turn,
                start_hp: last_hp,
                end_hp: last_hp,
                hp_loss: 0,
                ended_turn: false,
                actions: Vec::new(),
                player_block_after: 0,
                monsters_after: Vec::new(),
            });
            summary.actions.push(action.action_key.clone());
            if let Some(after) = &action.combat_after {
                last_hp = after.player_hp;
                summary.end_hp = last_hp;
                summary.hp_loss = summary.start_hp.saturating_sub(last_hp).max(0);
                summary.player_block_after = after.player_block;
                summary.monsters_after = after.monsters.clone();
            }
            if matches!(action.input, crate::state::core::ClientInput::EndTurn) {
                summary.ended_turn = true;
            }
        }
        if let Some(summary) = active_turn {
            turns.push(summary);
        }

        let post_combat_hp = branch.session.run_state.current_hp;
        Ok(OracleAnalysisCombatSummaryV1 {
            node_id,
            parent_node_id,
            encounter_start_hp,
            encounter_start_max_hp,
            combat_end_hp: last_hp,
            post_combat_hp,
            post_combat_max_hp: branch.session.run_state.max_hp,
            combat_hp_loss: encounter_start_hp.saturating_sub(last_hp).max(0),
            post_combat_healing: post_combat_hp.saturating_sub(last_hp).max(0),
            action_count: trajectory.action_count,
            turns,
        })
    }

    /// Snapshot root-action coverage from the resident combat search, if any.
    pub fn combat_root_action_families(
        &self,
        node_id: usize,
    ) -> Result<Vec<LocalTurnGraphRootActionFamilySnapshot>, String> {
        self.combat_jobs
            .get(&node_id)
            .map(|job| job.work.root_action_families())
            .ok_or_else(|| format!("oracle node {node_id} has no resident combat search"))
    }

    pub fn combat_case(
        &self,
        node_id: usize,
        seed: u64,
        ascension: u8,
        generation_work: usize,
        wall_ms: u64,
    ) -> Result<CombatCase, String> {
        let branch = self.require_branch(node_id)?;
        let position: CombatPosition = branch.session.current_active_combat_position()?;
        let generation = branch
            .journal
            .entries()
            .iter()
            .filter_map(RunProgressStepV1::as_decision)
            .count();
        let mut case = CombatCase::new(
            CombatCaseSource {
                seed,
                ascension,
                generation,
                branch_id: branch.branch_id,
                parent_id: branch.parent_branch_id,
            },
            CombatCaseGap {
                boundary: format!(
                    "Act {} Floor {} oracle analysis combat",
                    branch.session.run_state.act_num, branch.session.run_state.floor_num
                ),
                reason: "oracle_analysis_export".to_string(),
                witness_budget: CombatCaseWitnessBudgetV1::TurnGraphPortfolioV1 {
                    generation_work,
                    wall_ms,
                },
            },
            CombatCaseRunSummary {
                act: branch.session.run_state.act_num,
                floor: branch.session.run_state.floor_num,
                hp: branch.session.run_state.current_hp,
                max_hp: branch.session.run_state.max_hp,
                gold: branch.session.run_state.gold,
                deck_size: branch.session.run_state.master_deck.len(),
                relic_count: branch.session.run_state.relics.len(),
                potion_slots: branch.session.run_state.potions.len(),
            },
            Vec::new(),
            None,
            Vec::new(),
            CombatCaseRngSummary::from_pool(&branch.session.run_state.rng_pool),
            position,
        );
        case.production_context = Some(capture_oracle_analysis_combat_case_production_context_v2(
            &case.core,
            &branch.session,
            &self.combat_budgets,
        )?);
        Ok(case)
    }

    pub fn tree(&self) -> OracleAnalysisTreeViewV1 {
        OracleAnalysisTreeViewV1 {
            roots: self.root_node_ids(),
            cursor_node_id: self.cursor_node_id,
            mainline_node_id: self.mainline_node_id,
            nodes: self
                .explorer
                .branches
                .iter()
                .map(|branch| OracleAnalysisNodeSummaryV1 {
                    node_id: branch.branch_id,
                    canonical_parent_node_id: branch.parent_branch_id,
                    boundary: branch.boundary,
                    act: branch.session.run_state.act_num,
                    floor: branch.session.run_state.floor_num,
                    current_hp: branch.session.run_state.current_hp,
                    max_hp: branch.session.run_state.max_hp,
                    gold: branch.session.run_state.gold,
                    replay_len: branch.replay.len(),
                    is_cursor: branch.branch_id == self.cursor_node_id,
                    is_mainline_tip: branch.branch_id == self.mainline_node_id,
                })
                .collect(),
            edges: self.edges.clone(),
        }
    }

    pub fn view_cursor(&self) -> Result<OracleAnalysisNodeViewV1, String> {
        self.view_node(self.cursor_node_id)
    }

    pub fn view_node(&self, node_id: usize) -> Result<OracleAnalysisNodeViewV1, String> {
        let branch = self.require_branch(node_id)?;
        let mut choices = if matches!(
            branch.boundary,
            OracleRunBoundaryV1::Combat
                | OracleRunBoundaryV1::TerminalVictory
                | OracleRunBoundaryV1::TerminalDefeat
        ) {
            Vec::new()
        } else {
            self.explorer.decisions_for_branch(branch.branch_id)
        };
        choices.sort_by(|left, right| {
            left.path_discrepancy
                .cmp(&right.path_discrepancy)
                .then_with(|| {
                    left.path_negative_log_policy
                        .total_cmp(&right.path_negative_log_policy)
                })
                .then_with(|| left.stable_work_key.cmp(&right.stable_work_key))
        });
        let deck = &branch.session.run_state.master_deck;
        let choices = choices
            .into_iter()
            .map(|choice| {
                let label = oracle_analysis_choice_label(deck, &choice);
                OracleAnalysisChoiceViewV1 {
                    choice_ref: choice_ref(&choice),
                    kind: choice.kind,
                    candidate_id: choice.candidate_id.clone(),
                    label,
                    action: choice.action.clone(),
                    owner_rank: choice
                        .path_discrepancy
                        .saturating_sub(branch.path_discrepancy),
                    path_discrepancy: choice.path_discrepancy,
                    path_negative_log_policy: choice.path_negative_log_policy,
                    annotation: self
                        .decision_annotation
                        .and_then(|annotate| annotate(&branch.session, &choice.candidate_id)),
                }
            })
            .collect();
        let mainline_edges = self
            .mainline_edge_path
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let children = self
            .edges
            .iter()
            .filter(|edge| edge.parent_node_id == node_id)
            .map(|edge| OracleAnalysisChildViewV1 {
                edge_id: edge.edge_id,
                child_node_id: edge.child_node_id,
                kind: edge.kind,
                label: edge.label.clone(),
                is_on_mainline: mainline_edges.contains(&edge.edge_id),
            })
            .collect();
        let replay_len = branch.replay.len();
        let recent_replay = branch
            .replay
            .iter()
            .skip(replay_len.saturating_sub(12))
            .cloned()
            .collect();
        let run = &branch.session.run_state;
        let event = run
            .event_state
            .as_ref()
            .map(|event| OracleAnalysisEventViewV1 {
                id: event.id.stable_id().to_string(),
                screen: event.current_screen,
                completed: event.completed,
                combat_pending: event.combat_pending,
            });
        let encounter = branch.session.active_combat.as_ref().map(|active| {
            let combat = &active.combat_state;
            OracleAnalysisEncounterViewV1 {
                turn: combat.turn.turn_count,
                phase: format!("{:?}", combat.turn.current_phase),
                energy: combat.turn.energy,
                player_block: combat.entities.player.block,
                hand: combat.zones.hand.clone(),
                draw_pile_count: combat.zones.draw_pile.len(),
                discard_pile_count: combat.zones.discard_pile.len(),
                exhaust_pile_count: combat.zones.exhaust_pile.len(),
                is_elite: combat.meta.is_elite_fight,
                is_boss: combat.meta.is_boss_fight,
                monsters: combat
                    .entities
                    .monsters
                    .iter()
                    .map(|monster| OracleAnalysisMonsterViewV1 {
                        slot: monster.slot,
                        label: monster.enemy_id.get_name().to_string(),
                        current_hp: monster.current_hp,
                        max_hp: monster.max_hp,
                        block: monster.block,
                        alive: !monster.is_dead_or_escaped(),
                        planned_move_id: monster.planned_move_id(),
                        intent: exact_monster_intent(combat, monster),
                    })
                    .collect(),
            }
        });
        let reward = match &branch.session.engine_state {
            EngineState::RewardScreen(reward) => Some(reward.clone()),
            EngineState::RewardOverlay { reward_state, .. } => Some(reward_state.clone()),
            _ => None,
        };
        Ok(OracleAnalysisNodeViewV1 {
            node_id,
            canonical_parent_node_id: branch.parent_branch_id,
            is_cursor: node_id == self.cursor_node_id,
            is_on_mainline: node_id == self.mainline_node_id
                || self
                    .mainline_edge_path
                    .iter()
                    .filter_map(|edge_id| {
                        self.edges
                            .iter()
                            .find(|edge| edge.edge_id == *edge_id)
                            .map(|edge| edge.parent_node_id)
                    })
                    .any(|parent| parent == node_id),
            boundary: branch.boundary,
            state_fingerprint: branch.state_fingerprint.clone(),
            neow_root_label: branch.neow_root_label.clone(),
            act: run.act_num,
            floor: run.floor_num,
            current_hp: run.current_hp,
            max_hp: run.max_hp,
            gold: run.gold,
            keys: run.keys,
            deck: run.master_deck.clone(),
            relics: run.relics.clone(),
            potions: run.potions.clone(),
            reward,
            replay_len,
            recent_replay,
            choices,
            children,
            event,
            encounter,
            combat: self.combat_progress(node_id),
        })
    }

    /// Return the current executable owner order for one retained exact node.
    ///
    /// Direct candidates are reordered by the current policy and then joined
    /// to their unique materialized exact choice. Parameterized selection
    /// families have no directly executable abstract candidate, so their
    /// already materialized members retain exact identity and lazy-family
    /// order. Execution must consume these choice references, never the
    /// non-unique abstract candidate id.
    pub fn current_choice_order(&self, node_id: usize) -> Result<Vec<String>, String> {
        let branch = self.require_branch(node_id)?;
        let mut materialized = self.explorer.decisions_for_branch(branch.branch_id);
        materialized.sort_by(|left, right| {
            left.path_discrepancy
                .cmp(&right.path_discrepancy)
                .then_with(|| {
                    left.path_negative_log_policy
                        .total_cmp(&right.path_negative_log_policy)
                })
                .then_with(|| left.stable_work_key.cmp(&right.stable_work_key))
        });
        if materialized.is_empty() {
            return Ok(Vec::new());
        }

        let surface = build_decision_surface(&branch.session);
        let has_symbolic_selection = matches!(
            branch.session.engine_state,
            EngineState::RunPendingChoice(_)
        ) && surface.view.candidates.iter().any(|candidate| {
            matches!(
                candidate.key,
                Some(DecisionCandidateKey::SelectionSubmit { .. })
            ) && candidate.action.executable_action_ref().is_none()
        });
        if has_symbolic_selection {
            return Ok(materialized.iter().map(choice_ref).collect());
        }
        let direct_candidate_ids = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|_| candidate.id.as_str())
            })
            .collect::<BTreeSet<_>>();
        let legal = materialized
            .iter()
            .filter(|choice| direct_candidate_ids.contains(choice.candidate_id.as_str()))
            .map(|choice| RunPolicyCandidateV1 {
                candidate_id: &choice.candidate_id,
                label: &choice.label,
                action: &choice.action,
            })
            .collect::<Vec<_>>();
        if legal.is_empty() {
            return Err(format!(
                "oracle analysis node {node_id} has materialized work but no current direct choices"
            ));
        }
        let decision_prior = self.decision_prior.ok_or_else(|| {
            format!("oracle analysis node {node_id} has no current decision prior")
        })?;
        let prior = decision_prior(&branch.session, &legal)?;
        prior.validate_for(&legal)?;
        let mut ordered = Vec::with_capacity(materialized.len());
        let mut claimed_work_keys = BTreeSet::new();
        for entry in prior.entries {
            let matching = materialized
                .iter()
                .filter(|choice| choice.candidate_id == entry.candidate_id)
                .collect::<Vec<_>>();
            let [choice] = matching.as_slice() else {
                return Err(format!(
                    "oracle analysis node {node_id} has {} materialized exact choices for direct candidate '{}'; expected exactly one",
                    matching.len(),
                    entry.candidate_id,
                ));
            };
            claimed_work_keys.insert(choice.stable_work_key.as_str());
            ordered.push(choice_ref(choice));
        }
        if claimed_work_keys.len() != materialized.len() {
            let unclaimed = materialized
                .iter()
                .filter(|choice| !claimed_work_keys.contains(choice.stable_work_key.as_str()))
                .map(|choice| choice.candidate_id.as_str())
                .collect::<Vec<_>>();
            return Err(format!(
                "oracle analysis node {node_id} has materialized choices absent from the current direct surface: {}",
                unclaimed.join(", ")
            ));
        }
        Ok(ordered)
    }

    pub fn route_policy_audit(&self, node_id: usize) -> Result<ExactRoutePolicyAuditV1, String> {
        let branch = self.require_branch(node_id)?;
        let surface = build_decision_surface(&branch.session);
        let legal = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect::<Vec<_>>();
        exact_route_policy_audit_v1(&branch.session, &legal)
    }

    pub fn shop_policy_audit(&self, node_id: usize) -> Result<ExactShopPolicyAuditV1, String> {
        let branch = self.require_branch(node_id)?;
        let surface = build_decision_surface(&branch.session);
        let legal = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect::<Vec<_>>();
        exact_shop_policy_audit_v1(&branch.session, &legal)
    }

    pub fn card_reward_policy_audit(
        &self,
        node_id: usize,
    ) -> Result<ExactCardRewardPolicyAuditV1, String> {
        let branch = self.require_branch(node_id)?;
        let surface = build_decision_surface(&branch.session);
        let legal = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect::<Vec<_>>();
        exact_card_reward_policy_audit_v1(&branch.session, &legal)
    }

    pub fn campfire_policy_audit(
        &self,
        node_id: usize,
    ) -> Result<ExactCampfirePolicyAuditV1, String> {
        let branch = self.require_branch(node_id)?;
        let surface = build_decision_surface(&branch.session);
        let legal = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect::<Vec<_>>();
        exact_campfire_policy_audit_v1(&branch.session, &legal)
    }

    pub fn try_choice(&mut self, requested_ref: &str) -> Result<usize, String> {
        let (parent_node_id, stable_work_key) = parse_choice_ref(requested_ref)?;
        let parent_branch_id = self.require_branch(parent_node_id)?.branch_id;
        let committed = self
            .explorer
            .explicit_transactions()
            .commit_decision(
                parent_branch_id,
                stable_work_key,
                self.decision_annotation,
                self.decision_prior,
            )?
            .ok_or_else(|| {
                format!("choice reference is stale or is not legal at node {parent_node_id}")
            })?;
        let child_node_id = committed.child_branch_id;
        let edge_id = self.record_edge(
            parent_node_id,
            child_node_id,
            OracleAnalysisEdgeKindV1::Decision,
            committed.label,
            Some(requested_ref.to_string()),
        );
        self.move_cursor_after_edge(parent_node_id, edge_id, child_node_id);
        Ok(child_node_id)
    }

    pub fn advance_cursor(
        &mut self,
        request: OracleAnalysisAdvanceRequestV1,
    ) -> Result<OracleAnalysisAdvanceReportV1, String> {
        if request.max_quanta == 0 || request.quantum_generation_work == 0 {
            return Err("oracle analysis advance requires positive quantum budgets".to_string());
        }
        let source_node_id = self.cursor_node_id;
        let branch = self.require_branch(source_node_id)?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle analysis node {source_node_id} is at {:?}, not combat",
                branch.boundary
            ));
        }
        let requested_nodes = request
            .quantum_generation_work
            .saturating_mul(request.max_quanta);
        let requested_wall_ms = request.wall_ms.or_else(|| {
            request.quantum_ms.map(|quantum_ms| {
                quantum_ms.saturating_mul(u64::try_from(request.max_quanta).unwrap_or(u64::MAX))
            })
        });
        let preserves_identity_stage_allowance = self
            .combat_budgets
            .has_identity_partitioned_potion_allowance(&branch.session);
        let has_resident_search = self.combat_jobs.contains_key(&source_node_id);
        if !has_resident_search {
            let stage = 0;
            let work = OracleResidentCombatWitnessJobV1::new(
                &branch.session,
                self.combat_budgets
                    .for_session_stage(&branch.session, stage),
                self.combat_budgets.guidance_bundle.as_deref(),
            )?;
            self.combat_jobs.insert(
                source_node_id,
                OracleAnalysisCombatJobV1 {
                    stage,
                    completed_stage_trace: Vec::new(),
                    work,
                },
            );
        }
        let resumes_existing_search = self
            .combat_jobs
            .get(&source_node_id)
            .is_some_and(|job| job.work.quantum_count() > 0);
        let job = self
            .combat_jobs
            .get_mut(&source_node_id)
            .expect("analysis combat job exists");
        if resumes_existing_search {
            job.work.mark_search_resume_exact();
        }
        if resumes_existing_search || !preserves_identity_stage_allowance {
            job.work.ensure_requested_allowance(
                requested_nodes,
                requested_wall_ms.map(Duration::from_millis),
            );
        }
        let started = Instant::now();
        let deadline = request
            .wall_ms
            .and_then(|wall_ms| started.checked_add(Duration::from_millis(wall_ms)));
        let quantum = OracleCombatWitnessQuantumV1 {
            label: "oracle_analysis_session",
            additional_generation_work: request.quantum_generation_work,
            soft_wall_ms: request.quantum_ms,
        };
        let mut quanta_served = 0usize;
        let mut terminal_advance = None;
        while quanta_served < request.max_quanta {
            // Preserve one caller-granted quantum for the configured rescue
            // stage. Otherwise a bounded conserving challenge can consume the
            // whole request and leave autonomous callers no chance to test
            // whether a potion changes the outcome.
            if quanta_served > 0
                && quanta_served.saturating_add(1) == request.max_quanta
                && self.promote_combat_job_if_needed_with_exit(
                    source_node_id,
                    OracleAnalysisCombatStageExitV1::PromotedForReservedQuantum,
                )?
            {
                // Promotion is the work: the reserved final quantum below is
                // served against the next configured exact stage.
            }
            let job = self
                .combat_jobs
                .get_mut(&source_node_id)
                .expect("analysis combat job inserted above");
            let advance = if request.improve_incumbent {
                job.work.advance_improving_incumbent(&quantum, deadline)
            } else {
                job.work.advance(&quantum, deadline)
            };
            match advance {
                OracleCombatWitnessAdvanceV1::Pending => {
                    quanta_served = quanta_served.saturating_add(1);
                }
                OracleCombatWitnessAdvanceV1::GlobalDeadlineReached => break,
                OracleCombatWitnessAdvanceV1::ReadyToFinish
                | OracleCombatWitnessAdvanceV1::AllowanceExhausted => {
                    quanta_served = quanta_served.saturating_add(1);
                    let stage_exit = match advance {
                        OracleCombatWitnessAdvanceV1::ReadyToFinish => {
                            OracleAnalysisCombatStageExitV1::PromotedAfterReadyToFinish
                        }
                        OracleCombatWitnessAdvanceV1::AllowanceExhausted => {
                            OracleAnalysisCombatStageExitV1::PromotedAfterAllowanceExhausted
                        }
                        _ => unreachable!("matched terminal staged advance"),
                    };
                    if self.promote_combat_job_if_needed_with_exit(source_node_id, stage_exit)? {
                        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                            break;
                        }
                        continue;
                    }
                    terminal_advance = Some(advance);
                    break;
                }
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                break;
            }
        }
        if terminal_advance == Some(OracleCombatWitnessAdvanceV1::AllowanceExhausted) {
            return Ok(OracleAnalysisAdvanceReportV1 {
                source_node_id,
                status: OracleAnalysisAdvanceStatusV1::BudgetUnknown,
                quanta_served,
                elapsed_ms: elapsed_ms(started),
                combat: self.combat_progress_with_exit(
                    source_node_id,
                    OracleAnalysisCombatStageExitV1::BudgetUnknown,
                ),
            });
        }
        if terminal_advance != Some(OracleCombatWitnessAdvanceV1::ReadyToFinish) {
            return Ok(OracleAnalysisAdvanceReportV1 {
                source_node_id,
                status: OracleAnalysisAdvanceStatusV1::SearchPending,
                quanta_served,
                elapsed_ms: elapsed_ms(started),
                combat: self.combat_progress_with_exit(
                    source_node_id,
                    OracleAnalysisCombatStageExitV1::SearchPending,
                ),
            });
        }

        if !self.cursor_combat_incumbent_preserves_survival_floor()? {
            return Ok(OracleAnalysisAdvanceReportV1 {
                source_node_id,
                status: OracleAnalysisAdvanceStatusV1::BudgetUnknown,
                quanta_served,
                elapsed_ms: elapsed_ms(started),
                combat: self.combat_progress_with_exit(
                    source_node_id,
                    OracleAnalysisCombatStageExitV1::BudgetUnknown,
                ),
            });
        }

        let job = self
            .combat_jobs
            .remove(&source_node_id)
            .expect("ready analysis combat job exists");
        let child_node_id = match self.materialize_combat_work(source_node_id, &job.work) {
            Ok(child_node_id) => child_node_id,
            Err(error) => {
                self.combat_jobs.insert(source_node_id, job);
                return Err(error);
            }
        };
        let status = if let Some(child_node_id) = child_node_id {
            OracleAnalysisAdvanceStatusV1::BoundaryReached { child_node_id }
        } else {
            match self
                .explorer
                .unresolved_combats
                .iter()
                .rev()
                .find(|unresolved| unresolved.branch_id == source_node_id)
                .map(|unresolved| unresolved.evidence_kind)
            {
                Some(OracleRunCombatEvidenceKindV1::ExhaustiveRefutation) => {
                    OracleAnalysisAdvanceStatusV1::ExhaustiveRefutation
                }
                Some(OracleRunCombatEvidenceKindV1::SetupOrMechanicsError) => {
                    OracleAnalysisAdvanceStatusV1::SetupOrMechanicsError
                }
                _ => OracleAnalysisAdvanceStatusV1::BudgetUnknown,
            }
        };
        let stage_exit = match &status {
            OracleAnalysisAdvanceStatusV1::BoundaryReached { .. } => {
                OracleAnalysisCombatStageExitV1::BoundaryReached
            }
            OracleAnalysisAdvanceStatusV1::BudgetUnknown => {
                OracleAnalysisCombatStageExitV1::BudgetUnknown
            }
            OracleAnalysisAdvanceStatusV1::ExhaustiveRefutation => {
                OracleAnalysisCombatStageExitV1::ExhaustiveRefutation
            }
            OracleAnalysisAdvanceStatusV1::SetupOrMechanicsError => {
                OracleAnalysisCombatStageExitV1::SetupOrMechanicsError
            }
            OracleAnalysisAdvanceStatusV1::SearchPending => {
                OracleAnalysisCombatStageExitV1::SearchPending
            }
        };
        let final_progress = combat_progress_view_with_exit(&job, stage_exit);
        Ok(OracleAnalysisAdvanceReportV1 {
            source_node_id,
            status,
            quanta_served,
            elapsed_ms: elapsed_ms(started),
            combat: Some(final_progress),
        })
    }

    /// Spends one explicit bounded grant only in the cursor's current combat
    /// stage. Unlike strategic advance, this operation cannot promote a potion
    /// identity or materialize a child, and configured quality satisfaction
    /// does not terminate its local graph early.
    pub fn probe_cursor_combat_stage(
        &mut self,
        request: OracleAnalysisCombatProbeRequestV1,
    ) -> Result<OracleAnalysisCombatProbeReportV1, String> {
        if request.generation_work == 0
            || request.quantum_generation_work == 0
            || request.wall_ms == 0
        {
            return Err(
                "oracle analysis combat probe requires positive work, quantum, and wall budgets"
                    .to_string(),
            );
        }
        let source_node_id = self.cursor_node_id;
        let branch = self.require_branch(source_node_id)?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle analysis node {source_node_id} is at {:?}, not combat",
                branch.boundary
            ));
        }
        if !self.combat_jobs.contains_key(&source_node_id) {
            let stage = 0;
            let work = OracleResidentCombatWitnessJobV1::new(
                &branch.session,
                self.combat_budgets
                    .for_session_stage(&branch.session, stage),
                self.combat_budgets.guidance_bundle.as_deref(),
            )?;
            self.combat_jobs.insert(
                source_node_id,
                OracleAnalysisCombatJobV1 {
                    stage,
                    completed_stage_trace: Vec::new(),
                    work,
                },
            );
        }
        let job = self
            .combat_jobs
            .get_mut(&source_node_id)
            .expect("analysis combat probe job exists");
        if job.work.quantum_count() > 0 {
            job.work.mark_search_resume_exact();
        }
        job.work.ensure_requested_allowance(
            request.generation_work,
            Some(Duration::from_millis(request.wall_ms)),
        );

        let started = Instant::now();
        let deadline = started
            .checked_add(Duration::from_millis(request.wall_ms))
            .ok_or_else(|| "combat probe wall budget exceeds the platform deadline".to_string())?;
        let before_work = job.work.current_search_generation_work();
        let requested_work = u64::try_from(request.generation_work).unwrap_or(u64::MAX);
        let mut quanta_served = 0usize;
        let mut zero_progress_quanta = 0u8;
        let stop = loop {
            let consumed = job
                .work
                .current_search_generation_work()
                .saturating_sub(before_work);
            if consumed >= requested_work {
                break OracleAnalysisCombatProbeStopV1::WorkBudgetReached;
            }
            if Instant::now() >= deadline {
                break OracleAnalysisCombatProbeStopV1::WallReached;
            }
            let remaining =
                usize::try_from(requested_work.saturating_sub(consumed)).unwrap_or(usize::MAX);
            let quantum = OracleCombatWitnessQuantumV1 {
                label: "oracle_analysis_current_stage_probe",
                additional_generation_work: request.quantum_generation_work.min(remaining),
                soft_wall_ms: None,
            };
            let before_quantum = job.work.current_search_generation_work();
            let advance = job
                .work
                .advance_current_stage_probe(&quantum, Some(deadline));
            let after_quantum = job.work.current_search_generation_work();
            let quantum_consumed = after_quantum.saturating_sub(before_quantum);
            if advance != OracleCombatWitnessAdvanceV1::GlobalDeadlineReached {
                quanta_served = quanta_served.saturating_add(1);
            }
            let total_consumed = after_quantum.saturating_sub(before_work);
            if total_consumed >= requested_work {
                break OracleAnalysisCombatProbeStopV1::WorkBudgetReached;
            }
            match advance {
                OracleCombatWitnessAdvanceV1::GlobalDeadlineReached => {
                    break OracleAnalysisCombatProbeStopV1::WallReached;
                }
                OracleCombatWitnessAdvanceV1::ReadyToFinish
                | OracleCombatWitnessAdvanceV1::AllowanceExhausted => {
                    break OracleAnalysisCombatProbeStopV1::StageExhausted;
                }
                OracleCombatWitnessAdvanceV1::Pending => {
                    if quantum_consumed == 0 {
                        // One member can perform bookkeeping without charging
                        // generation work. Require a full two-member portfolio
                        // rotation before reporting genuine zero progress.
                        zero_progress_quanta = zero_progress_quanta.saturating_add(1);
                        if zero_progress_quanta >= 2 {
                            break OracleAnalysisCombatProbeStopV1::NoProgress;
                        }
                    } else {
                        zero_progress_quanta = 0;
                    }
                }
            }
        };
        let generation_work_consumed = job
            .work
            .current_search_generation_work()
            .saturating_sub(before_work);
        let stage_exit = match stop {
            OracleAnalysisCombatProbeStopV1::WorkBudgetReached => {
                OracleAnalysisCombatStageExitV1::ProbeWorkBudgetReached
            }
            OracleAnalysisCombatProbeStopV1::WallReached => {
                OracleAnalysisCombatStageExitV1::ProbeWallReached
            }
            OracleAnalysisCombatProbeStopV1::StageExhausted => {
                OracleAnalysisCombatStageExitV1::ProbeStageExhausted
            }
            OracleAnalysisCombatProbeStopV1::NoProgress => {
                OracleAnalysisCombatStageExitV1::ProbeNoProgress
            }
        };
        let combat = self
            .combat_progress_with_exit(source_node_id, stage_exit)
            .ok_or_else(|| "combat probe lost its resident current-stage job".to_string())?;
        Ok(OracleAnalysisCombatProbeReportV1 {
            source_node_id,
            stop,
            generation_work_requested: request.generation_work,
            generation_work_consumed,
            quanta_served,
            elapsed_ms: elapsed_ms(started),
            combat,
        })
    }

    /// Commits the current combat's already verified incumbent without asking
    /// the search to spend more quality-improvement budget. This is an
    /// explicit analyst action; BudgetUnknown never commits itself.
    pub fn accept_cursor_combat_incumbent(&mut self) -> Result<usize, String> {
        let source_node_id = self.cursor_node_id;
        let branch = self.require_branch(source_node_id)?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle analysis node {source_node_id} is at {:?}, not combat",
                branch.boundary
            ));
        }
        let Some(job) = self.combat_jobs.remove(&source_node_id) else {
            return Err(format!(
                "oracle analysis node {source_node_id} has no resident combat search"
            ));
        };
        if !job.work.has_verified_witness() {
            self.combat_jobs.insert(source_node_id, job);
            return Err(format!(
                "oracle analysis node {source_node_id} has no verified combat incumbent"
            ));
        }
        let child_node_id = match self.materialize_combat_work(source_node_id, &job.work) {
            Ok(child_node_id) => child_node_id,
            Err(error) => {
                self.combat_jobs.insert(source_node_id, job);
                return Err(error);
            }
        };
        child_node_id
            .ok_or_else(|| "verified combat incumbent did not materialize a child".to_string())
    }

    /// Whether the current verified incumbent preserves the run owner's broad
    /// floor-to-floor survival reserve. This is deliberately looser than the
    /// quality target: a reserve-compliant fallback may still be accepted after
    /// bounded refinement, while an incumbent below this floor remains
    /// budget-unknown unless an analyst explicitly supplies or accepts it.
    pub fn cursor_combat_incumbent_preserves_survival_floor(&self) -> Result<bool, String> {
        let source_node_id = self.cursor_node_id;
        let branch = self.require_branch(source_node_id)?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle analysis node {source_node_id} is at {:?}, not combat",
                branch.boundary
            ));
        }
        let Some(job) = self.combat_jobs.get(&source_node_id) else {
            return Ok(false);
        };
        let Some(hp_loss) = job.work.incumbent_hp_loss() else {
            return Ok(false);
        };
        Ok(
            match strategic_combat_survival_hp_loss_limit_v1(&branch.session) {
                RunControlHpLossLimit::Unlimited => true,
                RunControlHpLossLimit::Limit(limit) => hp_loss <= limit,
            },
        )
    }

    pub fn accept_cursor_combat_actions(
        &mut self,
        actions: &[ClientInput],
    ) -> Result<usize, String> {
        self.accept_combat_actions_from_node(self.cursor_node_id, actions)
    }

    fn accept_combat_actions_from_node(
        &mut self,
        source_node_id: usize,
        actions: &[ClientInput],
    ) -> Result<usize, String> {
        let branch = self.require_branch(source_node_id)?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle analysis node {source_node_id} is at {:?}, not combat",
                branch.boundary
            ));
        }
        // Analyst-supplied exact actions are prepared in an isolated work
        // object. A failed replay or downstream decision supply therefore
        // leaves any resident tactical frontier byte-for-byte untouched.
        let mut work = OracleResidentCombatWitnessJobV1::for_exact_actions(
            &branch.session,
            self.combat_budgets.for_session(&branch.session),
            self.combat_budgets.guidance_bundle.as_deref(),
        )?;
        work.verify_and_restore_action_witness(actions)?;
        let child_node_id = self.materialize_combat_work(source_node_id, &work)?;
        self.combat_jobs.remove(&source_node_id);
        child_node_id
            .ok_or_else(|| "verified combat action witness did not materialize a child".to_string())
    }

    /// Materializes the exact Smoke Bomb escape already exposed by the current
    /// combat state. This is an explicit analyst choice, not a victory claim
    /// and not a fallback hidden behind a failed search.
    pub fn accept_cursor_smoke_bomb_escape(&mut self) -> Result<usize, String> {
        let source_node_id = self.cursor_node_id;
        let branch = self.require_branch(source_node_id)?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle analysis node {source_node_id} is at {:?}, not combat",
                branch.boundary
            ));
        }
        let committed = self
            .explorer
            .explicit_transactions()
            .commit_smoke_bomb_escape(source_node_id, self.decision_prior)?;
        let child_node_id = committed
            .child_branch_id
            .expect("exact Smoke Bomb preparation must commit a child or exact survivor");
        self.combat_jobs.remove(&source_node_id);
        let child_current_hp = committed
            .child_current_hp
            .expect("committed Smoke Bomb child must retain its HP fact");
        let edge_id = self.record_edge(
            source_node_id,
            child_node_id,
            OracleAnalysisEdgeKindV1::CombatWitness,
            format!("Smoke Bomb escape -> {child_current_hp} HP"),
            None,
        );
        self.move_cursor_after_edge(source_node_id, edge_id, child_node_id);
        Ok(child_node_id)
    }

    /// Discards only the cursor combat's retained search work and starts a
    /// fresh tactical job from the same exact simulator state. Historical run
    /// state, journal entries, siblings, and navigation remain unchanged.
    pub fn restart_cursor_combat_search(&mut self) -> Result<(), String> {
        let node_id = self.cursor_node_id;
        let job = {
            let branch = self.require_branch(node_id)?;
            if branch.boundary != OracleRunBoundaryV1::Combat {
                return Err(format!(
                    "oracle analysis node {node_id} is at {:?}, not combat",
                    branch.boundary
                ));
            }
            let stage = 0;
            let work = OracleResidentCombatWitnessJobV1::restart(
                &branch.session,
                self.combat_budgets
                    .for_session_stage(&branch.session, stage),
                self.combat_budgets.guidance_bundle.as_deref(),
            )?;
            OracleAnalysisCombatJobV1 {
                stage,
                completed_stage_trace: Vec::new(),
                work,
            }
        };
        self.combat_jobs.insert(node_id, job);
        Ok(())
    }

    #[cfg(test)]
    fn promote_combat_job_if_needed(&mut self, node_id: usize) -> Result<bool, String> {
        self.promote_combat_job_if_needed_with_exit(
            node_id,
            OracleAnalysisCombatStageExitV1::PromotedForReservedQuantum,
        )
    }

    fn promote_combat_job_if_needed_with_exit(
        &mut self,
        node_id: usize,
        stage_exit: OracleAnalysisCombatStageExitV1,
    ) -> Result<bool, String> {
        let (next_stage, prior_work, completed_stage_trace) = {
            let branch = self.require_branch(node_id)?;
            let job = self
                .combat_jobs
                .get(&node_id)
                .ok_or_else(|| format!("oracle node {node_id} has no resident combat search"))?;
            if !self
                .combat_budgets
                .needs_later_stage(&branch.session, job.stage, &job.work)
            {
                return Ok(false);
            }
            let mut completed_stage_trace = job.completed_stage_trace.clone();
            completed_stage_trace.push(combat_stage_trace_view(job, stage_exit));
            (
                job.stage.saturating_add(1),
                job.work.checkpoint(),
                completed_stage_trace,
            )
        };
        let work = {
            let branch = self.require_branch(node_id)?;
            let options = self.combat_budgets.for_session_stage_with_prior(
                &branch.session,
                next_stage,
                &prior_work,
            );
            OracleResidentCombatWitnessJobV1::promote(
                &branch.session,
                options,
                prior_work,
                self.combat_budgets.guidance_bundle.as_deref(),
            )?
        };
        self.combat_jobs.insert(
            node_id,
            OracleAnalysisCombatJobV1 {
                stage: next_stage,
                completed_stage_trace,
                work,
            },
        );
        self.explorer.record_combat_search_restart();
        Ok(true)
    }

    fn materialize_combat_work(
        &mut self,
        source_node_id: usize,
        work: &OracleResidentCombatWitnessJobV1,
    ) -> Result<Option<usize>, String> {
        let committed = self
            .explorer
            .explicit_transactions()
            .commit_verified_combat(source_node_id, work, self.decision_prior)?;
        let child_node_id = committed.child_branch_id;
        if let Some(child_node_id) = child_node_id {
            let child_current_hp = committed
                .child_current_hp
                .expect("committed combat child must retain its HP fact");
            let edge_id = self.record_edge(
                source_node_id,
                child_node_id,
                OracleAnalysisEdgeKindV1::CombatWitness,
                format!("combat witness -> {child_current_hp} HP"),
                None,
            );
            self.move_cursor_after_edge(source_node_id, edge_id, child_node_id);
        }
        Ok(child_node_id)
    }

    fn require_branch(&self, node_id: usize) -> Result<&OracleRunBranchV1, String> {
        self.explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == node_id)
            .ok_or_else(|| format!("unknown oracle analysis node {node_id}"))
    }

    fn combat_progress(&self, node_id: usize) -> Option<OracleAnalysisCombatProgressV1> {
        self.combat_progress_with_exit(node_id, OracleAnalysisCombatStageExitV1::Active)
    }

    fn combat_progress_with_exit(
        &self,
        node_id: usize,
        stage_exit: OracleAnalysisCombatStageExitV1,
    ) -> Option<OracleAnalysisCombatProgressV1> {
        self.combat_jobs
            .get(&node_id)
            .map(|job| combat_progress_view_with_exit(job, stage_exit))
    }

    fn seed_canonical_edges(&mut self) {
        let parents = self
            .explorer
            .branches
            .iter()
            .filter_map(|branch| {
                branch
                    .parent_branch_id
                    .map(|parent| (parent, branch.branch_id))
            })
            .collect::<Vec<_>>();
        for (parent, child) in parents {
            if self
                .edges
                .iter()
                .any(|edge| edge.parent_node_id == parent && edge.child_node_id == child)
            {
                continue;
            }
            let label = self
                .edge_label_from_branches(parent, child)
                .unwrap_or_else(|| "continued variation".to_string());
            self.record_edge(
                parent,
                child,
                OracleAnalysisEdgeKindV1::Decision,
                label,
                None,
            );
        }
    }

    fn edge_label_from_branches(&self, parent: usize, child: usize) -> Option<String> {
        let parent = self.require_branch(parent).ok()?;
        let child = self.require_branch(child).ok()?;
        if child.replay.len() > parent.replay.len() {
            child.replay.last().map(|step| step.label.clone())
        } else if child.boundary != parent.boundary
            || child.session.run_state.current_hp != parent.session.run_state.current_hp
        {
            Some(format!(
                "combat witness -> {} HP",
                child.session.run_state.current_hp
            ))
        } else {
            None
        }
    }

    fn record_edge(
        &mut self,
        parent_node_id: usize,
        child_node_id: usize,
        kind: OracleAnalysisEdgeKindV1,
        label: String,
        choice_ref: Option<String>,
    ) -> u64 {
        if let Some(existing) = self.edges.iter().find(|edge| {
            edge.parent_node_id == parent_node_id
                && edge.child_node_id == child_node_id
                && edge.kind == kind
                && edge.choice_ref == choice_ref
        }) {
            return existing.edge_id;
        }
        let edge_id = self.next_edge_id;
        self.next_edge_id = self.next_edge_id.saturating_add(1);
        self.edges.push(OracleAnalysisEdgeV1 {
            edge_id,
            parent_node_id,
            child_node_id,
            kind,
            label,
            choice_ref,
        });
        edge_id
    }

    fn move_cursor_after_edge(
        &mut self,
        parent_node_id: usize,
        edge_id: u64,
        child_node_id: usize,
    ) {
        if self.cursor_node_id != parent_node_id {
            self.cursor_edge_path = self
                .path_to_node(parent_node_id)
                .expect("materialized analysis parent is reachable");
        }
        self.cursor_edge_path.push(edge_id);
        self.cursor_node_id = child_node_id;
    }

    fn path_to_node(&self, target: usize) -> Option<Vec<u64>> {
        if !self
            .explorer
            .branches
            .iter()
            .any(|branch| branch.branch_id == target)
        {
            return None;
        }
        let roots = self.root_node_ids();
        let mut queue = roots
            .iter()
            .map(|root| (*root, Vec::<u64>::new()))
            .collect::<VecDeque<_>>();
        let mut visited = BTreeSet::new();
        while let Some((node, path)) = queue.pop_front() {
            if !visited.insert(node) {
                continue;
            }
            if node == target {
                return Some(path);
            }
            for edge in self.edges.iter().filter(|edge| edge.parent_node_id == node) {
                let mut child_path = path.clone();
                child_path.push(edge.edge_id);
                queue.push_back((edge.child_node_id, child_path));
            }
        }
        None
    }

    fn validate_navigation_state(&self) -> Result<(), String> {
        self.require_branch(self.cursor_node_id)?;
        self.require_branch(self.mainline_node_id)?;
        let roots = self.root_node_ids().into_iter().collect::<BTreeSet<_>>();
        if self.cursor_edge_path.is_empty() && !roots.contains(&self.cursor_node_id) {
            return Err(format!(
                "analysis cursor node {} has no path from a root",
                self.cursor_node_id
            ));
        }
        if self.mainline_edge_path.is_empty() && !roots.contains(&self.mainline_node_id) {
            return Err(format!(
                "analysis mainline node {} has no path from a root",
                self.mainline_node_id
            ));
        }
        validate_edge_path(
            &self.edges,
            self.cursor_node_id,
            &self.cursor_edge_path,
            "cursor",
        )?;
        validate_edge_path(
            &self.edges,
            self.mainline_node_id,
            &self.mainline_edge_path,
            "mainline",
        )?;
        Ok(())
    }
}

fn choice_ref(work: &LazyOracleRunDecisionV1) -> String {
    format!(
        "choice-v1/{}/{}",
        work.parent_branch_id, work.stable_work_key
    )
}

fn parse_choice_ref(value: &str) -> Result<(usize, &str), String> {
    let mut parts = value.splitn(3, '/');
    if parts.next() != Some("choice-v1") {
        return Err("unsupported oracle analysis choice reference".to_string());
    }
    let node = parts
        .next()
        .ok_or_else(|| "choice reference is missing its node".to_string())?
        .parse::<usize>()
        .map_err(|_| "choice reference contains an invalid node".to_string())?;
    let key = parts
        .next()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| "choice reference is missing its fingerprint".to_string())?;
    Ok((node, key))
}

fn combat_stage_trace_view(
    job: &OracleAnalysisCombatJobV1,
    exit: OracleAnalysisCombatStageExitV1,
) -> OracleAnalysisCombatStageTraceV1 {
    let progress = job.work.evidence();
    combat_stage_trace_view_from_progress(job, &progress, exit)
}

fn combat_stage_trace_view_from_progress(
    job: &OracleAnalysisCombatJobV1,
    progress: &OracleResidentCombatWitnessEvidenceV1,
    exit: OracleAnalysisCombatStageExitV1,
) -> OracleAnalysisCombatStageTraceV1 {
    OracleAnalysisCombatStageTraceV1 {
        witness_engine: progress.witness_engine,
        stage: job.stage,
        guide_service_bias: progress.guide_service_bias.map(|bias| {
            OracleAnalysisCombatGuideServiceBiasV1 {
                lane: bias.lane.value(),
                extra_services_per_cycle: bias.extra_services_per_cycle,
            }
        }),
        max_potions_used: job.work.max_potions_used(),
        allowed_potion_slots: job.work.allowed_potion_slots(),
        potion_spend_requires_satisfaction: progress.potion_spend_requires_satisfaction,
        historical_generation_work_at_entry: progress.historical_generation_work,
        generation_work: progress.current_search_generation_work,
        local_generation_work: progress.local_generation_work,
        discrepancy_generation_work: progress.discrepancy_generation_work,
        exact_states: progress.exact_states,
        completed_turn_options: progress.completed_turn_options,
        public_plan_turn_proposal_enabled: progress.public_plan_turn_proposal_enabled,
        public_plan_turn_proposal_attempts: progress.public_plan_turn_proposal_attempts,
        public_plan_turn_proposal_applied_transitions: progress
            .public_plan_turn_proposal_applied_transitions,
        public_plan_turn_proposal_completed_options: progress
            .public_plan_turn_proposal_completed_options,
        public_plan_turn_proposal_retained_options: progress
            .public_plan_turn_proposal_retained_options,
        public_plan_turn_proposal_grounding_failures: progress
            .public_plan_turn_proposal_grounding_failures,
        plan_prefix_proposals: progress.plan_prefix_proposals,
        plan_prefix_proposed_turns: progress.plan_prefix_proposed_turns,
        plan_prefix_proposed_actions: progress.plan_prefix_proposed_actions,
        plan_prefix_proposal_rejections: progress.plan_prefix_proposal_rejections,
        local_candidate_final_hp: progress.local_candidate_final_hp,
        local_candidate_action_count: progress.local_candidate_action_count,
        local_candidate_potions_used: progress.local_candidate_potions_used,
        local_candidate_potion_slots: progress.local_candidate_potion_slots,
        local_candidate_satisfies_satisfaction: progress.local_candidate_satisfies_satisfaction,
        local_candidate_disposition: progress.local_candidate_disposition,
        incumbent_discovery_source: progress.incumbent_discovery_source,
        incumbent_final_hp: progress.incumbent_final_hp,
        incumbent_action_count: progress.incumbent_action_count,
        incumbent_potions_used: progress.incumbent_potions_used,
        incumbent_potion_slots: progress.incumbent_potion_slots,
        incumbent_satisfies_satisfaction: progress.incumbent_satisfies_satisfaction,
        incumbent_ends_quality_refinement: progress.incumbent_ends_quality_refinement,
        remaining_generation_work: job.work.remaining_generation_work(),
        remaining_wall_ms: job.work.remaining_wall_ms(),
        last_status: progress.last_status.clone(),
        exit,
    }
}

fn combat_progress_view_with_exit(
    job: &OracleAnalysisCombatJobV1,
    stage_exit: OracleAnalysisCombatStageExitV1,
) -> OracleAnalysisCombatProgressV1 {
    let work = &job.work;
    let progress: OracleResidentCombatWitnessEvidenceV1 = work.evidence();
    let mut stage_trace = job.completed_stage_trace.clone();
    stage_trace.push(combat_stage_trace_view_from_progress(
        job, &progress, stage_exit,
    ));
    OracleAnalysisCombatProgressV1 {
        witness_engine: progress.witness_engine,
        root_exact_state_hash: progress.root_exact_state_hash,
        stage_trace,
        search_stage: job.stage,
        max_potions_used: work.max_potions_used(),
        allowed_potion_slots: work.allowed_potion_slots(),
        potion_spend_requires_satisfaction: progress.potion_spend_requires_satisfaction,
        historical_generation_work: progress.historical_generation_work,
        current_search_generation_work: progress.current_search_generation_work,
        generation_work: progress.generation_work,
        local_generation_work: progress.local_generation_work,
        discrepancy_generation_work: progress.discrepancy_generation_work,
        exact_states: progress.exact_states,
        local_exact_states: progress.local_exact_states,
        discrepancy_exact_states: progress.discrepancy_exact_states,
        completed_turn_options: progress.completed_turn_options,
        public_plan_turn_proposal_enabled: progress.public_plan_turn_proposal_enabled,
        public_plan_turn_proposal_attempts: progress.public_plan_turn_proposal_attempts,
        public_plan_turn_proposal_applied_transitions: progress
            .public_plan_turn_proposal_applied_transitions,
        public_plan_turn_proposal_completed_options: progress
            .public_plan_turn_proposal_completed_options,
        public_plan_turn_proposal_retained_options: progress
            .public_plan_turn_proposal_retained_options,
        public_plan_turn_proposal_grounding_failures: progress
            .public_plan_turn_proposal_grounding_failures,
        retained_state_work: progress.retained_state_work,
        local_retained_state_work: progress.local_retained_state_work,
        discrepancy_retained_state_work: progress.discrepancy_retained_state_work,
        root_state: progress.root_state,
        max_player_turn: progress.max_player_turn,
        deepest_survival_state: progress.deepest_survival_state,
        deepest_progress_state: progress.deepest_progress_state,
        deepest_survival_actions: progress.deepest_survival_actions,
        deepest_progress_actions: progress.deepest_progress_actions,
        recent_turn_survival_envelope: progress.recent_turn_survival_envelope,
        pending_witness_replay: progress.pending_witness_replay,
        plan_prefix_proposals: progress.plan_prefix_proposals,
        plan_prefix_proposed_turns: progress.plan_prefix_proposed_turns,
        plan_prefix_proposed_actions: progress.plan_prefix_proposed_actions,
        plan_prefix_proposal_rejections: progress.plan_prefix_proposal_rejections,
        local_candidate_final_hp: progress.local_candidate_final_hp,
        local_candidate_action_count: progress.local_candidate_action_count,
        local_candidate_potions_used: progress.local_candidate_potions_used,
        local_candidate_potion_slots: progress.local_candidate_potion_slots,
        local_candidate_satisfies_satisfaction: progress.local_candidate_satisfies_satisfaction,
        local_candidate_disposition: progress.local_candidate_disposition,
        incumbent_discovery_source: progress.incumbent_discovery_source,
        incumbent_final_hp: progress.incumbent_final_hp,
        incumbent_hp_loss: progress.incumbent_hp_loss,
        incumbent_action_count: progress.incumbent_action_count,
        incumbent_potions_used: progress.incumbent_potions_used,
        incumbent_potion_slots: progress.incumbent_potion_slots,
        incumbent_satisfies_satisfaction: progress.incumbent_satisfies_satisfaction,
        incumbent_ends_quality_refinement: progress.incumbent_ends_quality_refinement,
        quantum_count: work.quantum_count(),
        remaining_generation_work: work.remaining_generation_work(),
        remaining_wall_ms: work.remaining_wall_ms(),
        resume_kind: if work.restart_count() > 0 {
            OracleCombatSearchResumeKindV1::StateReplayExactSearchRestarted
        } else if work.search_resume_exact() {
            OracleCombatSearchResumeKindV1::SearchResumeExact
        } else {
            OracleCombatSearchResumeKindV1::Fresh
        },
        restart_count: work.restart_count(),
        last_status: progress.last_status,
    }
}

fn validate_edge_path(
    edges: &[OracleAnalysisEdgeV1],
    expected_tip: usize,
    path: &[u64],
    label: &str,
) -> Result<(), String> {
    let Some(first_edge_id) = path.first() else {
        return Ok(());
    };
    let first = edges
        .iter()
        .find(|edge| edge.edge_id == *first_edge_id)
        .ok_or_else(|| format!("analysis {label} path references missing edge {first_edge_id}"))?;
    let mut node = first.parent_node_id;
    for edge_id in path {
        let edge = edges
            .iter()
            .find(|edge| edge.edge_id == *edge_id)
            .ok_or_else(|| format!("analysis {label} path references missing edge {edge_id}"))?;
        if edge.parent_node_id != node {
            return Err(format!(
                "analysis {label} path is disconnected before edge {edge_id}"
            ));
        }
        node = edge.child_node_id;
    }
    if node != expected_tip {
        return Err(format!(
            "analysis {label} path ends at node {node}, expected {expected_tip}"
        ));
    }
    Ok(())
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn exact_monster_intent(
    combat: &crate::runtime::combat::CombatState,
    monster: &crate::runtime::combat::MonsterEntity,
) -> Option<MonsterMoveSpec> {
    if monster.is_dead_or_escaped() {
        return None;
    }
    let plan = crate::content::monsters::resolve_monster_turn_plan(combat, monster);
    if plan.visible_spec.is_none() && plan.steps.is_empty() {
        return None;
    }
    Some(plan.summary_spec())
}

#[cfg(test)]
#[path = "oracle_analysis_session/tests.rs"]
mod tests;
