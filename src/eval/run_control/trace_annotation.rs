use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::{
    CombatSearchV2QuantumEvidence, CombatSearchV2RootEvidenceSnapshot, SearchTerminalLabel,
};
use crate::ai::noncombat_decision_v1::{
    render_noncombat_decision_record_validation_errors, validate_noncombat_decision_record_v1,
    NonCombatDecisionRecordV1,
};
use crate::ai::noncombat_strategy_v1::StrategyCapabilityKindV1;
use crate::ai::planner_core::{
    stable_planner_id, PlannerBehaviorEvent, PLANNER_BEHAVIOR_EVENT_SCHEMA_NAME,
    PLANNER_BEHAVIOR_EVENT_SCHEMA_VERSION,
};
use crate::ai::potion_continuation_context_v1::PotionRunContinuationContextV1;
use crate::ai::potion_continuation_pressure_v1::PotionContinuationPressureV1;
use crate::ai::strategy::pressure_assessment::PressureAxis;
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::runtime::combat::CardLineageId;
use crate::state::core::ClientInput;

use super::accepted_combat_line_evidence::AcceptedCombatLineEvidenceV1;
use super::combat_line_adjudication::CombatLineAdjudicationV1;
use super::progress_options::RunControlHpLossLimit;
use super::transition_report::CardSnapshot;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatAutomationTrajectorySource {
    /// Historical payload written before the two exact search engines had
    /// distinct evidence identities. New code must not emit this variant.
    SearchCombat,
    AtomicExactV2,
    TurnGraphPortfolioV1,
    /// An explicit action sequence supplied at an oracle-analysis boundary and
    /// accepted only after exact legal replay reaches terminal victory.
    OracleExactActions,
    CompleteLineSolver,
    TurnPlanRescue,
    #[serde(alias = "line_lab_turn_pool_rescue")]
    TurnPoolRescue,
    SearchCombatTurnSegment,
    SearchCombatSmokeBombSurvival,
}

impl CombatAutomationTrajectorySource {
    pub fn label(self) -> &'static str {
        match self {
            CombatAutomationTrajectorySource::SearchCombat => "search_combat",
            CombatAutomationTrajectorySource::AtomicExactV2 => "atomic_exact_v2",
            CombatAutomationTrajectorySource::TurnGraphPortfolioV1 => "turn_graph_portfolio_v1",
            CombatAutomationTrajectorySource::OracleExactActions => "oracle_exact_actions",
            CombatAutomationTrajectorySource::CompleteLineSolver => "complete_line_solver",
            CombatAutomationTrajectorySource::TurnPlanRescue => "turn_plan_rescue",
            CombatAutomationTrajectorySource::TurnPoolRescue => "turn_pool_rescue",
            CombatAutomationTrajectorySource::SearchCombatTurnSegment => {
                "search_combat_turn_segment"
            }
            CombatAutomationTrajectorySource::SearchCombatSmokeBombSurvival => {
                "search_combat_smoke_bomb_survival"
            }
        }
    }
}

impl std::fmt::Display for CombatAutomationTrajectorySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAutomationActionV1 {
    pub step_index: usize,
    pub action_key: String,
    pub input: ClientInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opportunity_before: Option<CombatAutomationOpportunityStateV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawn_cards: Vec<CardSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_after: Option<CombatAutomationStepStateV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAutomationOpportunityStateV1 {
    pub turn: u32,
    pub energy: u8,
    pub hand: Vec<CardSnapshot>,
    pub potions: Vec<Option<CombatAutomationPotionStateV1>>,
    pub playable_card_lineages: Vec<CardLineageId>,
    pub usable_potion_uuids: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAutomationPotionStateV1 {
    pub id: PotionId,
    pub uuid: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAutomationStepStateV1 {
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub cards_played_this_turn: u8,
    pub early_end_turn_pending: bool,
    pub monsters: Vec<CombatAutomationMonsterStateV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAutomationMonsterStateV1 {
    pub id: usize,
    pub label: String,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub time_warp: i32,
    pub strength: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAutomationTrajectoryRecordV1 {
    pub source: CombatAutomationTrajectorySource,
    pub action_count: usize,
    pub actions: Vec<CombatAutomationActionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answer_claims: Vec<CombatAutomationAnswerClaimV1>,
    pub label_role: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAutomationAnswerClaimV1 {
    pub source: CombatAutomationAnswerSourceV1,
    pub axes: Vec<PressureAxis>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatAutomationAnswerSourceV1 {
    Card {
        id: CardId,
        lineage: CardLineageId,
        upgrades: u8,
        origin: CombatAutomationCardOriginV1,
    },
    Potion {
        id: PotionId,
        uuid: u32,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatAutomationCardOriginV1 {
    MasterDeck,
    CombatGenerated,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSearchPerformanceSnapshotV1 {
    pub source: String,
    pub act: u8,
    pub floor: i32,
    pub turn: u32,
    pub combat_kind: String,
    pub enemies: Vec<String>,
    pub boss: String,
    pub external_payoff_opportunity: bool,
    pub coverage_status: String,
    pub complete_trajectory_found: bool,
    #[serde(default)]
    pub complete_win_found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_complete: Option<CombatSearchTerminalLineSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_win: Option<CombatSearchTerminalLineSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_hp_loss: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_adjudication: Option<CombatLineAdjudicationV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes_to_first_win: Option<u64>,
    #[serde(default)]
    pub deadline_hit: bool,
    #[serde(default)]
    pub node_budget_hit: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quantum_history: Vec<CombatSearchV2QuantumEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_root_evidence: Option<CombatSearchV2RootEvidenceSnapshot>,
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub terminal_wins: u64,
    pub total_us: u64,
    pub unattributed_us: u64,
    #[serde(default)]
    pub report_finalization_us: u64,
    #[serde(default)]
    pub report_frontier_scan_us: u64,
    #[serde(default)]
    pub report_search_storage_drop_us: u64,
    pub rollout_calls: u64,
    pub root_rollout_calls: u64,
    pub child_rollout_calls: u64,
    pub deferred_child_rollout_calls: u64,
    pub turn_plan_seed_rollout_calls: u64,
    pub deferred_child_rollout_nodes: u64,
    pub deferred_child_rollout_requeues: u64,
    pub rollout_cache_hits: u64,
    pub rollout_cache_queries: u64,
    pub rollout_cache_misses: u64,
    pub rollout_cache_inserts: u64,
    pub rollout_budget_skips: u64,
    pub rollout_max_evaluation_budget_skips: u64,
    pub rollout_deadline_budget_skips: u64,
    pub rollout_truncated: u64,
    pub rollout_terminal_wins: u64,
    pub rollout_cache_lookup_us: u64,
    pub rollout_policy_dispatch_us: u64,
    pub rollout_no_potion_iterations: u64,
    pub rollout_no_potion_phase_profile_us: u64,
    pub rollout_no_potion_legal_actions_us: u64,
    pub rollout_no_potion_choose_action_us: u64,
    pub rollout_no_potion_choose_ordering_us: u64,
    pub rollout_no_potion_probe_us: u64,
    pub rollout_no_potion_probe_score_calls: u64,
    pub rollout_no_potion_probe_actions_evaluated: u64,
    pub rollout_no_potion_probe_step_reuses: u64,
    pub rollout_no_potion_probe_engine_step_us: u64,
    pub rollout_no_potion_probe_phase_profile_us: u64,
    pub rollout_no_potion_probe_action_facts_us: u64,
    pub rollout_no_potion_engine_step_us: u64,
    pub rollout_no_potion_child_build_us: u64,
    pub terminal_child_rollout_skips: u64,
    pub terminal_turn_plan_seed_rollout_skips: u64,
    pub turn_local_dominance_rollout_skips: u64,
    pub rollout_us: u64,
    pub expansion_us: u64,
    pub child_bookkeeping_us: u64,
    pub engine_step_us: u64,
    pub pre_expand_us: u64,
    pub frontier_pop_us: u64,
    pub turn_plan_seed_us: u64,
    pub shadow_audit_us: u64,
    pub root_turn_plan_diag_us: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSearchTerminalLineSummary {
    pub terminal: SearchTerminalLabel,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub cards_played: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub action_count: usize,
}

pub const COMBAT_VICTORY_CONTINUATION_EVALUATOR_V1: &str =
    "strategic_combat_victory_reaches_full_heal_v1";
pub const COMBAT_SURVIVAL_HP_LIMIT_EVALUATOR_V1: &str =
    "strategic_combat_survival_hp_loss_limit_v1";
pub const COMBAT_QUALITY_HP_LIMIT_EVALUATOR_V1: &str = "strategic_combat_quality_hp_loss_limit_v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatVictoryHpCarryoverV1 {
    GuaranteedFullHealBeforeNextDamageBearingDecision,
    NotGuaranteedByRoomBossActTransition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatVictoryContinuationFactsV1 {
    pub evaluator: String,
    pub hp_carryover: CombatVictoryHpCarryoverV1,
}

impl CombatVictoryContinuationFactsV1 {
    pub fn from_guaranteed_room_boss_full_heal(guaranteed_full_heal: bool) -> Self {
        Self {
            evaluator: COMBAT_VICTORY_CONTINUATION_EVALUATOR_V1.to_owned(),
            hp_carryover: if guaranteed_full_heal {
                CombatVictoryHpCarryoverV1::GuaranteedFullHealBeforeNextDamageBearingDecision
            } else {
                CombatVictoryHpCarryoverV1::NotGuaranteedByRoomBossActTransition
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatSearchHpLossLimitV1 {
    Limited { max_hp_loss: u32 },
    Unlimited,
}

impl From<RunControlHpLossLimit> for CombatSearchHpLossLimitV1 {
    fn from(value: RunControlHpLossLimit) -> Self {
        match value {
            RunControlHpLossLimit::Limit(max_hp_loss) => Self::Limited { max_hp_loss },
            RunControlHpLossLimit::Unlimited => Self::Unlimited,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSearchStrategicHpQualityFactsV1 {
    pub survival_evaluator: String,
    pub survival_hp_loss_limit: CombatSearchHpLossLimitV1,
    pub quality_evaluator: String,
    pub quality_hp_loss_limit: CombatSearchHpLossLimitV1,
    pub entry_current_hp: i32,
    pub entry_max_hp: i32,
}

impl CombatSearchStrategicHpQualityFactsV1 {
    pub fn from_owner_limits(
        entry_current_hp: i32,
        entry_max_hp: i32,
        survival_hp_loss_limit: RunControlHpLossLimit,
        quality_hp_loss_limit: RunControlHpLossLimit,
    ) -> Self {
        Self {
            survival_evaluator: COMBAT_SURVIVAL_HP_LIMIT_EVALUATOR_V1.to_owned(),
            survival_hp_loss_limit: survival_hp_loss_limit.into(),
            quality_evaluator: COMBAT_QUALITY_HP_LIMIT_EVALUATOR_V1.to_owned(),
            quality_hp_loss_limit: quality_hp_loss_limit.into(),
            entry_current_hp,
            entry_max_hp,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtomicCombatSearchTraceSummaryV2 {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_max_nodes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_wall_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_potion_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_max_potions_used: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_allowed_potion_slots: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_internal_no_win_rescue_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atomic_stage_candidate_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atomic_witness_selected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atomic_stage_decision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potion_continuation_context: Option<PotionRunContinuationContextV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potion_continuation_pressure: Option<PotionContinuationPressureV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_victory_continuation: Option<CombatVictoryContinuationFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategic_hp_quality: Option<CombatSearchStrategicHpQualityFactsV1>,
    pub act: u8,
    pub floor: i32,
    pub turn: u32,
    pub combat_kind: String,
    pub enemies: Vec<String>,
    pub coverage_status: String,
    pub complete_trajectory_found: bool,
    #[serde(default)]
    pub complete_win_found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_complete: Option<CombatSearchTerminalLineSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_win: Option<CombatSearchTerminalLineSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_hp_loss: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_adjudication: Option<CombatLineAdjudicationV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes_to_first_win: Option<u64>,
    #[serde(default)]
    pub deadline_hit: bool,
    #[serde(default)]
    pub node_budget_hit: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quantum_history: Vec<CombatSearchV2QuantumEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_root_evidence: Option<CombatSearchV2RootEvidenceSnapshot>,
    pub nodes_expanded: u64,
    pub terminal_wins: u64,
    pub total_us: u64,
    #[serde(default)]
    pub unattributed_us: u64,
    #[serde(default)]
    pub report_finalization_us: u64,
    #[serde(default)]
    pub report_frontier_scan_us: u64,
    #[serde(default)]
    pub report_search_storage_drop_us: u64,
    #[serde(default)]
    pub rollout_us: u64,
    #[serde(default)]
    pub expansion_us: u64,
    #[serde(default)]
    pub child_bookkeeping_us: u64,
    #[serde(default)]
    pub engine_step_us: u64,
    #[serde(default)]
    pub pre_expand_us: u64,
    #[serde(default)]
    pub frontier_pop_us: u64,
    #[serde(default)]
    pub turn_plan_seed_us: u64,
    #[serde(default)]
    pub shadow_audit_us: u64,
    #[serde(default)]
    pub root_turn_plan_diag_us: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardRewardFunctionV1 {
    Answer,
    Access,
    Amplifier,
    Recovery,
    Liability,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardRewardObligationSourceV1 {
    KnownBoss,
    CommittedRoute,
    PossiblePool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardRewardObligationDeltaV1 {
    pub source: CardRewardObligationSourceV1,
    pub subject: String,
    pub deadline_nodes: Option<usize>,
    pub gaps_before: usize,
    pub gaps_after: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardRewardOwnerProvenanceV1 {
    pub functions: Vec<CardRewardFunctionV1>,
    pub obligations: Vec<CardRewardObligationDeltaV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strengthened_capabilities: Vec<StrategyCapabilityKindV1>,
    pub hard_startup_liability: bool,
    #[serde(default)]
    pub hard_duplicate_liability: bool,
    pub component_debt_count: usize,
    pub access_saturated: bool,
    pub stable_surface_index: usize,
    pub owner_rank: usize,
    pub tie_break_applied: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunControlTraceAnnotationV1 {
    CardRewardOwnerDecision {
        provenance: CardRewardOwnerProvenanceV1,
    },
    NonCombatPolicyDecision {
        record: NonCombatDecisionRecordV1,
    },
    NonCombatHumanBoundary {
        record: NonCombatDecisionRecordV1,
    },
    PlannerBehaviorDecision {
        event: PlannerBehaviorEvent,
    },
    AutoCombatCapture {
        case_id: String,
        capture_path: String,
        benchmark_manifest_path: String,
        label_role: String,
    },
    CombatAutomationTrajectory {
        source: CombatAutomationTrajectorySource,
        action_count: usize,
        actions: Vec<CombatAutomationActionV1>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        answer_claims: Vec<CombatAutomationAnswerClaimV1>,
        label_role: String,
    },
    CombatSearchPerformance {
        snapshot: CombatSearchPerformanceSnapshotV1,
    },
    AcceptedCombatLine {
        evidence: AcceptedCombatLineEvidenceV1,
    },
}

#[cfg(test)]
mod card_reward_provenance_tests {
    use super::*;

    #[test]
    fn legacy_card_reward_provenance_defaults_strengthened_capabilities() {
        let legacy = r#"{
            "functions":["access"],
            "obligations":[],
            "hard_startup_liability":false,
            "component_debt_count":0,
            "access_saturated":false,
            "stable_surface_index":0,
            "owner_rank":1,
            "tie_break_applied":false
        }"#;

        let provenance: CardRewardOwnerProvenanceV1 =
            serde_json::from_str(legacy).expect("legacy card reward provenance");
        assert!(provenance.strengthened_capabilities.is_empty());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatAutomationTrajectoryRefV1<'a> {
    pub source: CombatAutomationTrajectorySource,
    pub action_count: usize,
    pub actions: &'a [CombatAutomationActionV1],
    pub answer_claims: &'a [CombatAutomationAnswerClaimV1],
    pub label_role: &'a str,
}

impl CombatAutomationTrajectoryRecordV1 {
    pub fn new(
        source: CombatAutomationTrajectorySource,
        actions: Vec<CombatAutomationActionV1>,
    ) -> Self {
        Self {
            source,
            action_count: actions.len(),
            actions,
            answer_claims: Vec::new(),
            label_role: "simulator_generated_not_teacher_label".to_string(),
        }
    }

    pub fn with_answer_claims(mut self, answer_claims: Vec<CombatAutomationAnswerClaimV1>) -> Self {
        self.answer_claims = answer_claims;
        self
    }

    pub fn from_ref(value: CombatAutomationTrajectoryRefV1<'_>) -> Self {
        Self {
            source: value.source,
            action_count: value.action_count,
            actions: value.actions.to_vec(),
            answer_claims: value.answer_claims.to_vec(),
            label_role: value.label_role.to_string(),
        }
    }

    pub fn into_annotation(self) -> RunControlTraceAnnotationV1 {
        RunControlTraceAnnotationV1::CombatAutomationTrajectory {
            source: self.source,
            action_count: self.action_count,
            actions: self.actions,
            answer_claims: self.answer_claims,
            label_role: self.label_role,
        }
    }
}

impl RunControlTraceAnnotationV1 {
    pub fn as_combat_automation_trajectory_v1(
        &self,
    ) -> Option<CombatAutomationTrajectoryRefV1<'_>> {
        let RunControlTraceAnnotationV1::CombatAutomationTrajectory {
            source,
            action_count,
            actions,
            answer_claims,
            label_role,
        } = self
        else {
            return None;
        };
        Some(CombatAutomationTrajectoryRefV1 {
            source: *source,
            action_count: *action_count,
            actions,
            answer_claims,
            label_role,
        })
    }
}

pub fn annotations_have_combat_automation_trajectory_v1(
    annotations: &[RunControlTraceAnnotationV1],
) -> bool {
    combat_automation_trajectories_v1(annotations)
        .next()
        .is_some()
}

pub fn combat_automation_trajectories_v1(
    annotations: &[RunControlTraceAnnotationV1],
) -> impl Iterator<Item = CombatAutomationTrajectoryRefV1<'_>> {
    annotations
        .iter()
        .filter_map(RunControlTraceAnnotationV1::as_combat_automation_trajectory_v1)
}

pub fn atomic_combat_search_trace_summaries(
    annotations: &[RunControlTraceAnnotationV1],
) -> impl Iterator<Item = AtomicCombatSearchTraceSummaryV2> + '_ {
    annotations.iter().filter_map(|annotation| {
        let RunControlTraceAnnotationV1::CombatSearchPerformance { snapshot } = annotation else {
            return None;
        };
        Some(AtomicCombatSearchTraceSummaryV2 {
            source: snapshot.source.clone(),
            lane: None,
            profile_id: None,
            profile_max_nodes: None,
            profile_wall_ms: None,
            profile_potion_policy: None,
            profile_max_potions_used: None,
            profile_allowed_potion_slots: None,
            profile_internal_no_win_rescue_enabled: None,
            engine_fingerprint: None,
            atomic_stage_candidate_tier: None,
            atomic_witness_selected: None,
            atomic_stage_decision: None,
            potion_continuation_context: None,
            potion_continuation_pressure: None,
            combat_victory_continuation: None,
            strategic_hp_quality: None,
            act: snapshot.act,
            floor: snapshot.floor,
            turn: snapshot.turn,
            combat_kind: snapshot.combat_kind.clone(),
            enemies: snapshot.enemies.clone(),
            coverage_status: snapshot.coverage_status.clone(),
            complete_trajectory_found: snapshot.complete_trajectory_found,
            complete_win_found: snapshot.complete_win_found,
            best_complete: snapshot.best_complete.clone(),
            best_win: snapshot.best_win.clone(),
            best_hp_loss: snapshot.best_hp_loss,
            execution_adjudication: snapshot.execution_adjudication.clone(),
            nodes_to_first_win: snapshot.nodes_to_first_win,
            deadline_hit: snapshot.deadline_hit,
            node_budget_hit: snapshot.node_budget_hit,
            quantum_history: snapshot.quantum_history.clone(),
            final_root_evidence: snapshot.final_root_evidence.clone(),
            nodes_expanded: snapshot.nodes_expanded,
            terminal_wins: snapshot.terminal_wins,
            total_us: snapshot.total_us,
            unattributed_us: snapshot.unattributed_us,
            report_finalization_us: snapshot.report_finalization_us,
            report_frontier_scan_us: snapshot.report_frontier_scan_us,
            report_search_storage_drop_us: snapshot.report_search_storage_drop_us,
            rollout_us: snapshot.rollout_us,
            expansion_us: snapshot.expansion_us,
            child_bookkeeping_us: snapshot.child_bookkeeping_us,
            engine_step_us: snapshot.engine_step_us,
            pre_expand_us: snapshot.pre_expand_us,
            frontier_pop_us: snapshot.frontier_pop_us,
            turn_plan_seed_us: snapshot.turn_plan_seed_us,
            shadow_audit_us: snapshot.shadow_audit_us,
            root_turn_plan_diag_us: snapshot.root_turn_plan_diag_us,
        })
    })
}

pub(in crate::eval::run_control) fn validate_run_control_trace_annotations_v1(
    annotations: &[RunControlTraceAnnotationV1],
) -> Result<(), String> {
    for (idx, annotation) in annotations.iter().enumerate() {
        validate_run_control_trace_annotation_v1(idx, annotation)?;
    }
    Ok(())
}

fn validate_run_control_trace_annotation_v1(
    idx: usize,
    annotation: &RunControlTraceAnnotationV1,
) -> Result<(), String> {
    match annotation {
        RunControlTraceAnnotationV1::CardRewardOwnerDecision { provenance } => {
            if provenance.owner_rank == 0 {
                return Err(format!(
                    "trace annotation {idx} card reward owner rank must be one-based"
                ));
            }
            if provenance.obligations.is_empty() {
                return Err(format!(
                    "trace annotation {idx} card reward provenance has no strategic obligations"
                ));
            }
            Ok(())
        }
        RunControlTraceAnnotationV1::NonCombatPolicyDecision { record, .. } => {
            validate_noncombat_record_annotation(idx, "noncombat_policy_decision", record)
        }
        RunControlTraceAnnotationV1::NonCombatHumanBoundary { record } => {
            validate_noncombat_record_annotation(idx, "noncombat_human_boundary", record)
        }
        RunControlTraceAnnotationV1::PlannerBehaviorDecision { event } => {
            validate_planner_behavior_event(idx, event)
        }
        RunControlTraceAnnotationV1::AutoCombatCapture { .. }
        | RunControlTraceAnnotationV1::CombatAutomationTrajectory { .. }
        | RunControlTraceAnnotationV1::CombatSearchPerformance { .. }
        | RunControlTraceAnnotationV1::AcceptedCombatLine { .. } => Ok(()),
    }
}

fn validate_planner_behavior_event(idx: usize, event: &PlannerBehaviorEvent) -> Result<(), String> {
    if event.schema_name != PLANNER_BEHAVIOR_EVENT_SCHEMA_NAME
        || event.schema_version != PLANNER_BEHAVIOR_EVENT_SCHEMA_VERSION
    {
        return Err(format!(
            "annotation[{idx}] has unsupported planner behavior event schema"
        ));
    }
    let mut behavior_payload = event.behavior.clone();
    behavior_payload.behavior_id.clear();
    if event.behavior.behavior_id != stable_planner_id("behavior", &behavior_payload)? {
        return Err(format!(
            "annotation[{idx}] planner behavior id does not match its payload"
        ));
    }
    Ok(())
}

fn validate_noncombat_record_annotation(
    idx: usize,
    kind: &str,
    record: &NonCombatDecisionRecordV1,
) -> Result<(), String> {
    validate_noncombat_decision_record_v1(record).map_err(|errors| {
        format!(
            "annotation[{idx}] {kind} contains invalid NonCombatDecisionRecordV1: {}",
            render_noncombat_decision_record_validation_errors(&errors)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal_win(final_hp: i32, hp_loss: i32) -> CombatSearchTerminalLineSummary {
        CombatSearchTerminalLineSummary {
            terminal: SearchTerminalLabel::Win,
            final_hp,
            hp_loss,
            turns: 7,
            cards_played: 27,
            potions_used: 0,
            potions_discarded: 0,
            action_count: 34,
        }
    }

    #[test]
    fn legacy_combat_search_summary_without_potion_context_still_deserializes() {
        let payload = serde_json::to_value(AtomicCombatSearchTraceSummaryV2::default())
            .expect("serialize summary");
        assert!(payload.get("potion_continuation_context").is_none());

        let restored: AtomicCombatSearchTraceSummaryV2 =
            serde_json::from_value(payload).expect("deserialize legacy summary");

        assert!(restored.potion_continuation_context.is_none());
        assert!(restored.potion_continuation_pressure.is_none());
    }

    #[test]
    fn accepted_line_evidence_keeps_original_and_selected_losses_separate() {
        let evidence = AcceptedCombatLineEvidenceV1::new(
            terminal_win(24, 35),
            terminal_win(44, 15),
            Some("line_repair attempts=4 wins=2 improvements=1".to_string()),
        );

        assert_eq!(evidence.original.hp_loss, 35);
        assert_eq!(evidence.selected.hp_loss, 15);
        assert_eq!(evidence.hp_saved_by_selection, 20);
    }

    #[test]
    fn combat_automation_trajectory_accessor_exposes_recorded_actions() {
        let action = CombatAutomationActionV1 {
            step_index: 3,
            action_key: "combat/end_turn".to_string(),
            input: ClientInput::EndTurn,
            opportunity_before: None,
            drawn_cards: Vec::new(),
            combat_after: None,
        };
        let annotation = RunControlTraceAnnotationV1::CombatAutomationTrajectory {
            source: CombatAutomationTrajectorySource::SearchCombat,
            action_count: 1,
            actions: vec![action],
            answer_claims: Vec::new(),
            label_role: "simulator_generated_not_teacher_label".to_string(),
        };

        let trajectory = annotation
            .as_combat_automation_trajectory_v1()
            .expect("combat automation annotation should expose a trajectory view");

        assert_eq!(
            trajectory.source,
            CombatAutomationTrajectorySource::SearchCombat
        );
        assert_eq!(trajectory.action_count, 1);
        assert_eq!(trajectory.actions[0].step_index, 3);
        assert_eq!(
            trajectory.label_role,
            "simulator_generated_not_teacher_label"
        );
    }

    #[test]
    fn oracle_exact_action_source_has_distinct_serialized_provenance() {
        assert_eq!(
            CombatAutomationTrajectorySource::OracleExactActions.label(),
            "oracle_exact_actions"
        );
        assert_eq!(
            serde_json::to_string(&CombatAutomationTrajectorySource::OracleExactActions)
                .expect("trajectory source should serialize"),
            "\"oracle_exact_actions\""
        );
    }

    #[test]
    fn combat_automation_trajectory_slice_helper_detects_any_recorded_trajectory() {
        let action = CombatAutomationActionV1 {
            step_index: 0,
            action_key: "combat/end_turn".to_string(),
            input: ClientInput::EndTurn,
            opportunity_before: None,
            drawn_cards: Vec::new(),
            combat_after: None,
        };
        let annotations = vec![
            RunControlTraceAnnotationV1::AutoCombatCapture {
                case_id: "case".to_string(),
                capture_path: "case.json".to_string(),
                benchmark_manifest_path: "manifest.json".to_string(),
                label_role: "human_review_artifact".to_string(),
            },
            RunControlTraceAnnotationV1::CombatAutomationTrajectory {
                source: CombatAutomationTrajectorySource::SearchCombat,
                action_count: 1,
                actions: vec![action],
                answer_claims: Vec::new(),
                label_role: "simulator_generated_not_teacher_label".to_string(),
            },
        ];

        assert!(!annotations_have_combat_automation_trajectory_v1(&[]));
        assert!(annotations_have_combat_automation_trajectory_v1(
            &annotations
        ));
    }

    #[test]
    fn combat_automation_trajectory_iterator_returns_recorded_trajectories_only() {
        let annotations = vec![
            RunControlTraceAnnotationV1::AutoCombatCapture {
                case_id: "case".to_string(),
                capture_path: "case.json".to_string(),
                benchmark_manifest_path: "manifest.json".to_string(),
                label_role: "human_review_artifact".to_string(),
            },
            RunControlTraceAnnotationV1::CombatAutomationTrajectory {
                source: CombatAutomationTrajectorySource::SearchCombat,
                action_count: 2,
                actions: vec![CombatAutomationActionV1 {
                    step_index: 0,
                    action_key: "combat/end_turn".to_string(),
                    input: ClientInput::EndTurn,
                    opportunity_before: None,
                    drawn_cards: Vec::new(),
                    combat_after: None,
                }],
                answer_claims: Vec::new(),
                label_role: "simulator_generated_not_teacher_label".to_string(),
            },
        ];

        let trajectories = combat_automation_trajectories_v1(&annotations).collect::<Vec<_>>();

        assert_eq!(trajectories.len(), 1);
        assert_eq!(
            trajectories[0].source,
            CombatAutomationTrajectorySource::SearchCombat
        );
        assert_eq!(trajectories[0].action_count, 2);
        assert_eq!(trajectories[0].actions[0].step_index, 0);
    }

    #[test]
    fn combat_automation_trajectory_record_converts_to_annotation() {
        let record = CombatAutomationTrajectoryRecordV1::new(
            CombatAutomationTrajectorySource::SearchCombat,
            vec![CombatAutomationActionV1 {
                step_index: 1,
                action_key: "combat/end_turn".to_string(),
                input: ClientInput::EndTurn,
                opportunity_before: None,
                drawn_cards: Vec::new(),
                combat_after: None,
            }],
        );

        let annotation = record.clone().into_annotation();
        let view = annotation
            .as_combat_automation_trajectory_v1()
            .expect("record should convert into trajectory annotation");

        assert_eq!(view.source, record.source);
        assert_eq!(view.action_count, record.action_count);
        assert_eq!(view.actions, record.actions.as_slice());
        assert_eq!(view.label_role, record.label_role);
    }
}
