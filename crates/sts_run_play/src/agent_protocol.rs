use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;
use sts_presentation::Locale;

mod saved_plans;
use crate::decision::DecisionDetailV1;
use saved_plans::SavedPlans;

#[cfg(test)]
use crate::run_selection::project_run_selection_family_from_parts_v1;
use crate::run_selection::{
    project_run_selection_family_v1, project_run_selection_target_v1, RunSelectionFamilyV1,
    RunSelectionTargetV1,
};
use serde::{Deserialize, Serialize};
use sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1;
use sts_complete_turn_proposal::ExactCompleteTurnProposalConfigV1;
use sts_complete_turn_view::CompleteTurnSelectedActionV1;
use sts_oracle_learning::agent::information::action::{
    PublicCombatAtomicActionV1, PublicCombatSelectionDomainCandidateV1,
};
use sts_oracle_learning::agent::information::combat::ObservationEvidenceKindV1;
use sts_oracle_learning::agent::information::effect::PublicCombatActionEffectEnvelopeV1;
use sts_oracle_learning::agent::information::run::{
    PublicCombatRunContextGapV1, PublicCombatRunContextV1,
};
use sts_oracle_learning::agent::information::state::{
    CombatLearningCardCollectionV1, CombatLearningCardV1, CombatLearningEnemyIdentityV1,
    CombatLearningIntentV1, CombatLearningMonsterMoveHistoryV1,
    CombatLearningMonsterPublicCounterV1, CombatLearningPotionV1, CombatLearningPowerV1,
    CombatLearningTurnV1, PublicCombatStateV1,
};
#[cfg(test)]
use sts_oracle_learning::agent::task::TaskStrategicCardV1;
use sts_oracle_learning::agent::task::{
    TaskEpisodeOutcomeV1, TaskPrivilegedSupportProvenanceV1, TaskPublicCandidateV1,
    TaskPublicObservationV1, TaskResearchPlanAttemptExitV1, TaskResearchPlanCandidateV1,
    TaskResearchPlanLaneAttemptV1, TaskResearchPlanOutcomeV1, TaskStrategicActionV1,
    TaskStrategicObservationV1, TaskStrategicPotionSlotV1,
};
use sts_oracle_learning::ai::card_semantics_v1::{
    card_acquisition_mechanics_facts_v1, card_owned_reading_v1, card_reward_facts_v1,
    potion_acquisition_mechanics_facts_v1, relic_acquisition_mechanics_facts_v1,
    CardAcquisitionMechanicsFactsV1, CardOwnedStateReadingV1, CardRewardFactsV1,
    PotionAcquisitionMechanicsFactsV1, RelicAcquisitionMechanicsFactsV1,
};
use sts_oracle_learning::ai::combat_witness_contract::{
    CombatWitnessPotionPolicyV1, CombatWitnessSatisfactionV1,
};
use sts_oracle_learning::ai::planner_core::{
    PlannerDecisionContext, PlannerEventPublicEffectV1, PlannerEventRelicPoolV1,
    PlannerEventTransitionV1, PlannerMapRoom, PlannerPublicHistory, PlannerPublicMap,
    PlannerQuestionMarkOutcome, PlannerRelicObservation, PlannerRewardDescriptor, PlannerRunGoal,
    PlannerRunScalars,
};
use sts_oracle_learning::ai::relic_route_mechanics_v1::limited_combat_route_effect_v1;
use sts_oracle_learning::ai::route_window_facts::{
    RouteFirstEliteSegment, RoutePathFamilySummary, RouteWindowCoverageKind,
};
use sts_oracle_learning::content::cards::CardId;
use sts_oracle_learning::content::monsters::factory::EncounterId;
use sts_oracle_learning::content::potions::PotionId;
use sts_oracle_learning::content::powers::PowerId;
use sts_oracle_learning::content::relics::{energy_master_delta, RelicId, RelicState};
use sts_oracle_learning::eval::run_control::{
    AtomicCombatResearchPlanChallengerOptionsV1, CombatResearchBoundedProgressRoleV1,
    CombatResearchBoundedProgressStateV1, CombatResearchPlanOptionsV1,
    CombatResearchProgressEventV1, CompleteTurnProposalBeamResearchOptionsV1,
    ExactCampfirePolicyAuditV1, ExactRunPlayInspectionStopV1, ExactRunPlayInspectionV1,
    ExactRunPlayProbeLineV1, RoutePolicyActionV1, RoutePolicyArrivalV1, RoutePolicyPathEvidenceV1,
};
use sts_oracle_learning::sim::combat_action_surface::CombatSelectionReasonV2;
use sts_oracle_learning::state::core::PileType;
use sts_oracle_learning::state::rewards::RewardCard;
#[cfg(test)]
use sts_oracle_learning::state::selection::{SelectionReason, SelectionScope};

use crate::agent_combat_draft::{
    validate_option_ref as validate_combat_draft_option_ref_v1,
    validate_quantum as validate_combat_draft_quantum_v1, AgentCombatDraftAdvancedV1,
    AgentCombatDraftChosenV1, AgentCombatDraftInspectedV1, AgentCombatDraftRuntimeV1,
    AgentCombatDraftSelectionEditV1, AgentCombatDraftSelectionV1, AgentCombatDraftStatusV1,
    AgentCombatDraftUndoneV1,
};
use crate::combat_trace::{trace_cached_combat_research_v1, RunPlayCombatTraceV1};
use crate::driver::{
    combat_research_candidate_id_v1, resolve_combat_research_candidate_index_v1,
    RunPlayActionLineProfileV1, RunPlayAdviceDetailV1, RunPlayCombatMilestoneContinuationAuditV1,
    RunPlayCombatMilestoneResearchCandidateV1, RunPlayCombatPrefixHorizonAuditV1,
    RunPlayCombatResearchAttemptV1, RunPlayCombatResearchBoundedProgressV1,
    RunPlayCombatTurnAttemptV1, RunPlayCombatTurnCandidateV1,
    RunPlayCombatTurnContinuationAuditOptionsV1, RunPlayCombatTurnContinuationAuditV1,
    RunPlayCombatTurnObservedTerminalOutcomeV1, RunPlayComparisonV1, RunPlayControllerV1,
    MAX_ANCHOR_LINE_STEPS,
};
use crate::model::{
    RunPlayCombatResearchCandidateV1, RunPlayEvidenceIdentityV1, RunPlayHeadV1, RunPlayPositionV1,
};
use crate::selector::{
    parse_action_selector_v1, semantic_action_for_choice_v1, RunPlayActionSelectorV1,
};
use crate::storage::{read_bounded, DEFAULT_ARTIFACT_MAX_BYTES};

pub const AGENT_REQUEST_SCHEMA_V1: &str = "sts-run-play-agent-request-v1";
pub const AGENT_RESPONSE_SCHEMA_V1: &str = "sts-run-play-agent-response-v1";
pub const AGENT_OBSERVATION_SCHEMA_V3: &str = "sts-run-play-agent-observation-v3";
pub const COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1: &str = "sts-run-play-combat-research-artifact-v1";
pub const EXACT_PREFIX_ARTIFACT_SCHEMA_V1: &str = "sts-run-play-exact-prefix-artifact-v1";
const MAX_AGENT_PROBE_LINES: usize = 4;
const MAX_AGENT_COMBAT_MILESTONE_SUCCESSOR_DETAILS: usize = 16;
const MAX_AGENT_COMBAT_REVIEW_CANDIDATES: usize = 8;
const MAX_AGENT_COMBAT_PREFIX_ACTIONS: usize = 256;
const MAX_AGENT_COMBAT_PREFIX_HORIZON_TURNS: u8 = 2;
const MAX_AGENT_COMBAT_TURN_CONTINUATION_AUDITS: usize = 4;
const MAX_AGENT_COMBAT_TURN_RESERVATION_ACTIONS: usize = 64;
const MAX_AGENT_GAME_FACT_ENTITIES: usize = 128;
const AGENT_ROUTE_ENTRY_PATH_BUDGET_V1: usize = 2_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentGameFactEntityV1 {
    Card { card: CardId },
    Relic { relic: RelicId },
    Potion { potion: PotionId },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentGameFactV1 {
    Card {
        definition: CardAcquisitionMechanicsFactsV1,
    },
    Relic {
        definition: RelicAcquisitionMechanicsFactsV1,
    },
    Potion {
        definition: PotionAcquisitionMechanicsFactsV1,
    },
}

pub fn agent_game_facts_v1(entities: &[AgentGameFactEntityV1]) -> Vec<AgentGameFactV1> {
    entities
        .iter()
        .map(|entity| match entity {
            AgentGameFactEntityV1::Card { card } => AgentGameFactV1::Card {
                definition: card_acquisition_mechanics_facts_v1(&RewardCard::new(*card, 0)),
            },
            AgentGameFactEntityV1::Relic { relic } => AgentGameFactV1::Relic {
                definition: relic_acquisition_mechanics_facts_v1(*relic),
            },
            AgentGameFactEntityV1::Potion { potion } => AgentGameFactV1::Potion {
                definition: potion_acquisition_mechanics_facts_v1(*potion),
            },
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentShellRequestV1 {
    Decision {
        schema: String,
        request_id: String,
        #[serde(default)]
        details: Vec<crate::decision::DecisionDetailV1>,
        #[serde(default)]
        known_context: Option<crate::decision::DecisionKnownContextV1>,
    },
    Choose {
        schema: String,
        request_id: String,
        selection: crate::decision::RunPlayBoundChoiceV1,
        #[serde(default)]
        following_rewards: Option<Vec<sts_oracle_learning::agent::task::TaskPublicChoiceV1>>,
        #[serde(default)]
        known_context: Option<crate::decision::DecisionKnownContextV1>,
    },
    Hello {
        schema: String,
        request_id: String,
    },
    Observe {
        schema: String,
        request_id: String,
    },
    GameFacts {
        schema: String,
        request_id: String,
        entities: Vec<AgentGameFactEntityV1>,
    },
    Info {
        schema: String,
        request_id: String,
        query: String,
        #[serde(default)]
        locale: Option<Locale>,
    },
    SlPiles {
        schema: String,
        request_id: String,
    },
    Act {
        schema: String,
        request_id: String,
        actions: Vec<String>,
        #[serde(default)]
        advance: AgentAdvanceModeV1,
    },
    Probe {
        schema: String,
        request_id: String,
        lines: Vec<Vec<String>>,
    },
    CombatResearch {
        schema: String,
        request_id: String,
        #[serde(default = "default_combat_search_ms")]
        max_search_ms: u64,
        #[serde(default)]
        max_generation_work: Option<usize>,
        /// Total potion expenditure ceiling, including passive entry-bottle
        /// loss and explicit discards. Cannot loosen the controller policy.
        #[serde(default)]
        max_potions_used: Option<u32>,
        #[serde(default)]
        effort: AgentCombatResearchEffortV1,
        /// Full successor previews are intentionally opt-in. The default
        /// response carries one small public-facts summary per milestone;
        /// selected lineages can then be expanded through
        /// `combat_milestone_successor_details` without flooding an
        /// interactive transport with every card zone.
        #[serde(default)]
        include_milestone_successor_previews: bool,
    },
    CombatReview {
        schema: String,
        request_id: String,
        candidate_refs: Vec<String>,
        #[serde(default)]
        report_id: Option<String>,
        /// One-based displayed turn to expand in the compact reading.
        #[serde(default)]
        turn: Option<u32>,
        #[serde(default)]
        effort: AgentCombatResearchEffortV1,
        /// Per-action deltas are available for a selected turn-level review,
        /// but stay opt-in so the default model transport remains compact.
        #[serde(default)]
        include_steps: bool,
        /// One line per turn instead of the three-line block form; the
        /// per-turn detail stays behind `turn`.
        #[serde(default)]
        compact: bool,
    },
    CombatPlans {
        schema: String,
        request_id: String,
        #[serde(default)]
        diagnostics: bool,
    },
    CombatMilestoneSuccessorDetails {
        schema: String,
        request_id: String,
        candidate_ids: Vec<String>,
        #[serde(default)]
        effort: AgentCombatResearchEffortV1,
    },
    CombatMilestoneContinuationAudit {
        schema: String,
        request_id: String,
        candidate_ids: Vec<String>,
        #[serde(default)]
        effort: AgentCombatResearchEffortV1,
    },
    CombatPrefixHorizonAudit {
        schema: String,
        request_id: String,
        root_head_revision: u64,
        lineage_id: String,
        prefix_actions: Vec<String>,
        expected_successor_public_snapshot_id: String,
        additional_turns: u8,
    },
    CombatDraftStatus {
        schema: String,
        request_id: String,
    },
    CombatDraftOpen {
        schema: String,
        request_id: String,
        candidate_ref: String,
        effort: AgentCombatResearchEffortV1,
        turn: u32,
        #[serde(default)]
        replace: bool,
    },
    CombatDraftAdvance {
        schema: String,
        request_id: String,
        generation_work: usize,
        engine_steps: usize,
    },
    CombatDraftInspect {
        schema: String,
        request_id: String,
        frame_ref: String,
        option_ref: String,
    },
    CombatDraftChoose {
        schema: String,
        request_id: String,
        frame_ref: String,
        option_ref: String,
    },
    CombatDraftUndo {
        schema: String,
        request_id: String,
    },
    CombatDraftSelection {
        schema: String,
        request_id: String,
        #[serde(default)]
        edit: Option<AgentCombatDraftSelectionEditV1>,
        #[serde(default)]
        source_ref: Option<String>,
        #[serde(default)]
        offset: Option<usize>,
    },
    CommitCombatPlan {
        schema: String,
        request_id: String,
        candidate_ref: String,
        #[serde(default)]
        report_id: Option<String>,
        #[serde(default)]
        effort: AgentCombatResearchEffortV1,
    },
    CommitCombatMilestone {
        schema: String,
        request_id: String,
        candidate_id: String,
        #[serde(default)]
        effort: AgentCombatResearchEffortV1,
    },
    CombatTurnResearch {
        schema: String,
        request_id: String,
    },
    CombatTurnReserve {
        schema: String,
        request_id: String,
        line: Vec<String>,
    },
    CombatTurnCandidateDetails {
        schema: String,
        request_id: String,
        candidate_ids: Vec<String>,
    },
    CombatTurnContinuationAudit {
        schema: String,
        request_id: String,
        candidate_ids: Vec<String>,
        #[serde(default)]
        effort: AgentCombatResearchEffortV1,
    },
    CommitCombatTurn {
        schema: String,
        request_id: String,
        candidate_id: String,
    },
    Quit {
        schema: String,
        request_id: String,
    },
}

impl AgentShellRequestV1 {
    fn public_manual_operation(&self) -> bool {
        matches!(
            self,
            Self::Hello { .. }
                | Self::Decision { .. }
                | Self::Choose { .. }
                | Self::Observe { .. }
                | Self::GameFacts { .. }
                | Self::Info { .. }
                | Self::Act { .. }
                | Self::Quit { .. }
        )
    }

    fn schema(&self) -> &str {
        match self {
            Self::Hello { schema, .. }
            | Self::Decision { schema, .. }
            | Self::Choose { schema, .. }
            | Self::Observe { schema, .. }
            | Self::GameFacts { schema, .. }
            | Self::Info { schema, .. }
            | Self::SlPiles { schema, .. }
            | Self::Act { schema, .. }
            | Self::Probe { schema, .. }
            | Self::CombatResearch { schema, .. }
            | Self::CombatReview { schema, .. }
            | Self::CombatPlans { schema, .. }
            | Self::CombatMilestoneSuccessorDetails { schema, .. }
            | Self::CombatMilestoneContinuationAudit { schema, .. }
            | Self::CombatPrefixHorizonAudit { schema, .. }
            | Self::CombatDraftStatus { schema, .. }
            | Self::CombatDraftOpen { schema, .. }
            | Self::CombatDraftAdvance { schema, .. }
            | Self::CombatDraftInspect { schema, .. }
            | Self::CombatDraftChoose { schema, .. }
            | Self::CombatDraftUndo { schema, .. }
            | Self::CombatDraftSelection { schema, .. }
            | Self::CommitCombatPlan { schema, .. }
            | Self::CommitCombatMilestone { schema, .. }
            | Self::CombatTurnResearch { schema, .. }
            | Self::CombatTurnReserve { schema, .. }
            | Self::CombatTurnCandidateDetails { schema, .. }
            | Self::CombatTurnContinuationAudit { schema, .. }
            | Self::CommitCombatTurn { schema, .. }
            | Self::Quit { schema, .. } => schema,
        }
    }

    fn request_id(&self) -> &str {
        match self {
            Self::Hello { request_id, .. }
            | Self::Decision { request_id, .. }
            | Self::Choose { request_id, .. }
            | Self::Observe { request_id, .. }
            | Self::GameFacts { request_id, .. }
            | Self::Info { request_id, .. }
            | Self::SlPiles { request_id, .. }
            | Self::Act { request_id, .. }
            | Self::Probe { request_id, .. }
            | Self::CombatResearch { request_id, .. }
            | Self::CombatReview { request_id, .. }
            | Self::CombatPlans { request_id, .. }
            | Self::CombatMilestoneSuccessorDetails { request_id, .. }
            | Self::CombatMilestoneContinuationAudit { request_id, .. }
            | Self::CombatPrefixHorizonAudit { request_id, .. }
            | Self::CombatDraftStatus { request_id, .. }
            | Self::CombatDraftOpen { request_id, .. }
            | Self::CombatDraftAdvance { request_id, .. }
            | Self::CombatDraftInspect { request_id, .. }
            | Self::CombatDraftChoose { request_id, .. }
            | Self::CombatDraftUndo { request_id, .. }
            | Self::CombatDraftSelection { request_id, .. }
            | Self::CommitCombatPlan { request_id, .. }
            | Self::CommitCombatMilestone { request_id, .. }
            | Self::CombatTurnResearch { request_id, .. }
            | Self::CombatTurnReserve { request_id, .. }
            | Self::CombatTurnCandidateDetails { request_id, .. }
            | Self::CombatTurnContinuationAudit { request_id, .. }
            | Self::CommitCombatTurn { request_id, .. }
            | Self::Quit { request_id, .. } => request_id,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema() != AGENT_REQUEST_SCHEMA_V1 {
            return Err(format!(
                "unsupported agent request schema {:?}; expected {AGENT_REQUEST_SCHEMA_V1}",
                self.schema()
            ));
        }
        if self.request_id().is_empty() {
            return Err("agent request_id must not be empty".to_string());
        }
        if let Self::CombatReview {
            report_id: Some(id),
            ..
        }
        | Self::CommitCombatPlan {
            report_id: Some(id),
            ..
        } = self
        {
            if id.trim().is_empty() || id.len() > 256 {
                return Err("combat report_id must contain 1..=256 bytes".into());
            }
        }
        if let Self::GameFacts { entities, .. } = self {
            if entities.is_empty() || entities.len() > MAX_AGENT_GAME_FACT_ENTITIES {
                return Err(format!(
                    "agent-shell game_facts requires 1..={MAX_AGENT_GAME_FACT_ENTITIES} entities"
                ));
            }
            if entities.iter().collect::<HashSet<_>>().len() != entities.len() {
                return Err("agent-shell game_facts entities must be distinct".to_string());
            }
        }
        if let Self::Info { query, .. } = self {
            if query.trim().is_empty() || query.len() > 256 {
                return Err("agent-shell info query must contain 1..=256 bytes".to_string());
            }
        }
        if let Self::Act {
            actions, advance, ..
        } = self
        {
            if actions.is_empty() || actions.len() > MAX_ANCHOR_LINE_STEPS {
                return Err(format!(
                    "agent-shell act requires 1..={MAX_ANCHOR_LINE_STEPS} actions"
                ));
            }
            if actions.len() > 1 && *advance != AgentAdvanceModeV1::OnePublicAction {
                return Err(
                    "agent-shell transactional action lines require one_public_action advance"
                        .to_string(),
                );
            }
            for action in actions {
                validate_agent_semantic_action_v1(action, "agent-shell act")?;
            }
        }
        if let Self::Probe { lines, .. } = self {
            if lines.is_empty() || lines.len() > MAX_AGENT_PROBE_LINES {
                return Err(format!(
                    "agent-shell probe requires 1..={MAX_AGENT_PROBE_LINES} lines"
                ));
            }
            for (line_index, line) in lines.iter().enumerate() {
                // A line that can be published must also fit a private preview.
                if line.is_empty() || line.len() > MAX_ANCHOR_LINE_STEPS {
                    return Err(format!(
                        "agent-shell probe line {} requires 1..={MAX_ANCHOR_LINE_STEPS} actions",
                        line_index + 1
                    ));
                }
                for action in line {
                    validate_agent_semantic_action_v1(action, "agent-shell probe")?;
                }
            }
        }
        if let Self::CombatMilestoneSuccessorDetails { candidate_ids, .. } = self {
            if candidate_ids.is_empty() {
                return Err(
                    "combat milestone successor details require at least one candidate id"
                        .to_string(),
                );
            }
            if candidate_ids.len() > MAX_AGENT_COMBAT_MILESTONE_SUCCESSOR_DETAILS {
                return Err(format!(
                    "combat milestone successor details exceed the bounded maximum: {} > {}",
                    candidate_ids.len(),
                    MAX_AGENT_COMBAT_MILESTONE_SUCCESSOR_DETAILS
                ));
            }
            if candidate_ids.iter().any(String::is_empty) {
                return Err(
                    "combat milestone successor detail candidate ids must be non-empty".to_string(),
                );
            }
            let distinct = candidate_ids.iter().collect::<HashSet<_>>();
            if distinct.len() != candidate_ids.len() {
                return Err(
                    "combat milestone successor detail candidate ids must be distinct".to_string(),
                );
            }
        }
        if let Self::CombatReview { candidate_refs, .. } = self {
            if candidate_refs.is_empty()
                || candidate_refs.len() > MAX_AGENT_COMBAT_REVIEW_CANDIDATES
            {
                return Err(format!(
                    "combat review requires 1..={MAX_AGENT_COMBAT_REVIEW_CANDIDATES} candidate refs"
                ));
            }
            if candidate_refs
                .iter()
                .any(|candidate_ref| candidate_ref.trim().is_empty() || candidate_ref.len() > 32)
                || candidate_refs.iter().collect::<HashSet<_>>().len() != candidate_refs.len()
            {
                return Err(
                    "combat review candidate refs must be distinct 1..=32 byte values".to_string(),
                );
            }
        }
        if let Self::CombatMilestoneContinuationAudit { candidate_ids, .. } = self {
            if candidate_ids.is_empty()
                || candidate_ids.len() > MAX_AGENT_COMBAT_MILESTONE_SUCCESSOR_DETAILS
            {
                return Err(format!(
                    "combat milestone continuation audit requires 1..={} candidate ids",
                    MAX_AGENT_COMBAT_MILESTONE_SUCCESSOR_DETAILS
                ));
            }
            if candidate_ids.iter().any(String::is_empty)
                || candidate_ids.iter().collect::<HashSet<_>>().len() != candidate_ids.len()
            {
                return Err(
                    "combat milestone continuation audit candidate ids must be non-empty and distinct"
                        .to_string(),
                );
            }
        }
        if let Self::CombatPrefixHorizonAudit {
            lineage_id,
            prefix_actions,
            expected_successor_public_snapshot_id,
            additional_turns,
            ..
        } = self
        {
            if lineage_id.trim().is_empty() || lineage_id.len() > 256 {
                return Err(
                    "combat prefix horizon audit lineage_id must contain 1..=256 bytes".to_string(),
                );
            }
            if prefix_actions.is_empty() || prefix_actions.len() > MAX_AGENT_COMBAT_PREFIX_ACTIONS {
                return Err(format!(
                    "combat prefix horizon audit requires 1..={MAX_AGENT_COMBAT_PREFIX_ACTIONS} prefix actions"
                ));
            }
            for action in prefix_actions {
                validate_agent_semantic_action_v1(
                    action,
                    "agent-shell combat_prefix_horizon_audit",
                )?;
            }
            if expected_successor_public_snapshot_id.trim().is_empty()
                || expected_successor_public_snapshot_id.len() > 256
            {
                return Err(
                    "combat prefix horizon audit expected snapshot must contain 1..=256 bytes"
                        .to_string(),
                );
            }
            if !(1..=MAX_AGENT_COMBAT_PREFIX_HORIZON_TURNS).contains(additional_turns) {
                return Err(format!(
                    "combat prefix horizon audit requires 1..={MAX_AGENT_COMBAT_PREFIX_HORIZON_TURNS} additional turns"
                ));
            }
        }
        if let Self::CombatResearch {
            max_search_ms,
            max_generation_work,
            effort,
            ..
        } = self
        {
            if !(1..=20_000).contains(max_search_ms) {
                return Err("combat research max_search_ms must be in 1..=20000".into());
            }
            if let Some(work) = max_generation_work {
                if *effort != AgentCombatResearchEffortV1::WholeCombat
                    || !(1..=1_000_000).contains(work)
                {
                    return Err("combat research max_generation_work must be in 1..=1000000 and requires whole_combat".into());
                }
            }
        }
        if let Self::CombatDraftOpen {
            candidate_ref,
            turn,
            ..
        } = self
        {
            if *turn == 0 || candidate_ref.is_empty() || candidate_ref.len() > 32 {
                return Err("combat draft opening requires a candidate ref and turn >= 1".into());
            }
        }
        if let Self::CombatDraftAdvance {
            generation_work,
            engine_steps,
            ..
        } = self
        {
            validate_combat_draft_quantum_v1(*generation_work, *engine_steps)?;
        }
        if let Self::CombatDraftInspect { option_ref, .. }
        | Self::CombatDraftChoose { option_ref, .. } = self
        {
            validate_combat_draft_option_ref_v1(option_ref)?;
        }
        if let Self::CombatDraftSelection {
            edit,
            source_ref,
            offset,
            ..
        } = self
        {
            crate::agent_combat_draft::validate_selection_request(
                edit.as_ref(),
                source_ref.as_deref(),
                *offset,
            )?;
        }
        if let Self::CommitCombatPlan { candidate_ref, .. } = self {
            if candidate_ref.trim().is_empty() || candidate_ref.len() > 32 {
                return Err(
                    "agent-shell commit_combat_plan requires a 1..=32 byte candidate_ref"
                        .to_string(),
                );
            }
        }
        if let Self::CommitCombatMilestone { candidate_id, .. } = self {
            if candidate_id.trim().is_empty() || candidate_id.len() > 256 {
                return Err(
                    "agent-shell commit_combat_milestone requires a 1..=256 byte candidate_id"
                        .to_string(),
                );
            }
        }
        if let Self::CombatTurnCandidateDetails { candidate_ids, .. } = self {
            validate_combat_turn_candidate_ids_v1(candidate_ids, "candidate details")?;
        }
        if let Self::CombatTurnReserve { line, .. } = self {
            if line.is_empty() || line.len() > MAX_AGENT_COMBAT_TURN_RESERVATION_ACTIONS {
                return Err(format!(
                    "combat-turn reservation requires 1..={MAX_AGENT_COMBAT_TURN_RESERVATION_ACTIONS} semantic actions"
                ));
            }
            for action in line {
                validate_agent_semantic_action_v1(action, "agent-shell combat_turn_reserve")?;
            }
        }
        if let Self::CombatTurnContinuationAudit { candidate_ids, .. } = self {
            validate_combat_turn_candidate_ids_v1(candidate_ids, "continuation audit")?;
        }
        if let Self::CommitCombatTurn { candidate_id, .. } = self {
            if candidate_id.trim().is_empty() || candidate_id.len() > 256 {
                return Err(
                    "agent-shell commit_combat_turn requires a 1..=256 byte candidate_id"
                        .to_string(),
                );
            }
        }
        Ok(())
    }
}

fn validate_combat_turn_candidate_ids_v1(
    candidate_ids: &[String],
    operation: &str,
) -> Result<(), String> {
    if candidate_ids.is_empty() || candidate_ids.len() > MAX_AGENT_COMBAT_TURN_CONTINUATION_AUDITS {
        return Err(format!(
            "combat-turn {operation} requires 1..={MAX_AGENT_COMBAT_TURN_CONTINUATION_AUDITS} candidate ids"
                ));
    }
    if candidate_ids
        .iter()
        .any(|candidate_id| candidate_id.trim().is_empty() || candidate_id.len() > 256)
        || candidate_ids.iter().collect::<HashSet<_>>().len() != candidate_ids.len()
    {
        return Err(format!(
            "combat-turn {operation} candidate ids must be distinct 1..=256 byte values"
        ));
    }
    Ok(())
}

fn validate_agent_semantic_action_v1(action: &str, context: &str) -> Result<(), String> {
    if action.starts_with("ordinal:") {
        return Err(format!(
            "{context} rejects ordinal addressing; copy a semantic action from current options verbatim"
        ));
    }
    parse_action_selector_v1(action, context)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAdvanceModeV1 {
    #[default]
    OnePublicAction,
    NextStrategicDecision,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum AgentCombatResearchEffortV1 {
    #[default]
    Standard,
    FastWitness,
    Expanded,
    WholeCombat,
}

pub(crate) fn combat_research_options_for_effort_v1(
    base: &CombatResearchPlanOptionsV1,
    effort: AgentCombatResearchEffortV1,
    requested_wall_ms: Option<u64>,
) -> CombatResearchPlanOptionsV1 {
    let mut options = base.clone();
    options.satisfaction = match base.satisfaction {
        sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::ZeroLossOrBudget => {
            sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(0)
        }
        sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::FirstCompleteWin => {
            sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::FirstCompleteWinWithoutNewExternalBurden
        }
        sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::HpLossAtMost(limit) => {
            sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(limit)
        }
        satisfaction => satisfaction,
    };
    match effort {
        AgentCombatResearchEffortV1::WholeCombat => {
            options.lane_schedule =
                sts_oracle_learning::eval::run_control::CombatResearchLaneScheduleV1::WholeCombat;
            options.max_generation_work = options.max_generation_work.max(30_000);
            options.max_engine_steps_per_transition =
                options.max_engine_steps_per_transition.max(512);
            // Explicit time replaces the default inner cap; otherwise longer
            // research requests only extend an unused outer deadline.
            options.wall_ms = requested_wall_ms
                .unwrap_or(options.wall_ms.max(2_000))
                .max(1);
            options.milestone_closure = None;
            options.complete_turn_proposal_beam = None;
            options.atomic_challenger = None;
            return options;
        }
        AgentCombatResearchEffortV1::Standard => {
            options.max_potions_used = combat_research_potion_limit_for_effort_v1(base, effort);
            return options;
        }
        AgentCombatResearchEffortV1::FastWitness => {
            options.lane_schedule =
                sts_oracle_learning::eval::run_control::CombatResearchLaneScheduleV1::FirstWitness;
            options.satisfaction =
                sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::FirstCompleteWinWithoutNewExternalBurden;
            options.potion_policy = CombatWitnessPotionPolicyV1::Never;
            options.max_potions_used = combat_research_potion_limit_for_effort_v1(base, effort);
            if options.complete_turn_proposal_beam.is_none() {
                options.complete_turn_proposal_beam =
                    Some(CompleteTurnProposalBeamResearchOptionsV1::default());
            }
            if options.atomic_challenger.is_none() {
                options.atomic_challenger =
                    Some(AtomicCombatResearchPlanChallengerOptionsV1::default());
            }
            return options;
        }
        AgentCombatResearchEffortV1::Expanded => {}
    }
    if options.milestone_closure.is_none() {
        options.milestone_closure = Some(
            sts_oracle_learning::eval::run_control::CombatMilestoneClosureResearchOptionsV1::default(),
        );
    }
    options.max_generation_work = options.max_generation_work.saturating_mul(8).max(32_768);
    options.max_engine_steps_per_transition = options.max_engine_steps_per_transition.max(1_000);
    options.wall_ms = options.wall_ms.saturating_mul(10).max(5_000);
    options.quantum_generation_work = options.quantum_generation_work.max(512);
    options.quantum_wall_ms = options.quantum_wall_ms.max(100);
    options.complete_turn_proposal_beam =
        options
            .complete_turn_proposal_beam
            .map(|beam| CompleteTurnProposalBeamResearchOptionsV1 {
                beam_width: beam.beam_width.saturating_mul(4).max(32),
                max_expanded_boundaries: beam.max_expanded_boundaries.saturating_mul(8).max(4_096),
                wall_ms: beam.wall_ms.saturating_mul(2).max(30_000),
            });
    options.atomic_challenger =
        options
            .atomic_challenger
            .map(|atomic| AtomicCombatResearchPlanChallengerOptionsV1 {
                max_nodes: atomic.max_nodes.saturating_mul(16).max(65_536),
                wall_ms: atomic.wall_ms.saturating_mul(30).max(30_000),
            });
    options.max_potions_used = combat_research_potion_limit_for_effort_v1(base, effort);
    options
}

fn combat_research_potion_limit_for_effort_v1(
    base: &CombatResearchPlanOptionsV1,
    effort: AgentCombatResearchEffortV1,
) -> Option<u32> {
    if effort == AgentCombatResearchEffortV1::WholeCombat {
        return if base.potion_policy == CombatWitnessPotionPolicyV1::Never {
            Some(0)
        } else {
            base.max_potions_used
        };
    }
    if effort == AgentCombatResearchEffortV1::FastWitness
        || base.potion_policy == CombatWitnessPotionPolicyV1::Never
    {
        return Some(0);
    }
    base.max_potions_used.or(Some(match effort {
        AgentCombatResearchEffortV1::FastWitness | AgentCombatResearchEffortV1::WholeCombat => {
            unreachable!("handled above")
        }
        AgentCombatResearchEffortV1::Standard => 1,
        AgentCombatResearchEffortV1::Expanded => 2,
    }))
}

fn restrict_combat_research_potions_v1(
    options: &mut CombatResearchPlanOptionsV1,
    requested: Option<u32>,
) {
    if let Some(limit) = requested {
        options.max_potions_used =
            Some(options.max_potions_used.map_or(limit, |old| old.min(limit)));
    }
}

/// Combines independently replay-verified passes from the same public root.
///
/// Beam width is not a set-monotone operation: a wider beam can evict an
/// execution found by a narrower pass. Progressive effort therefore keeps the
/// earlier pass as evidence and adds later-pass executions instead of treating
/// the later pass as a replacement. Candidate identity is the executable
/// public plan; deterministic outcome disagreements are rejected.
fn merge_combat_research_passes_v1(
    passes: impl IntoIterator<Item = RunPlayCombatResearchAttemptV1>,
) -> Result<RunPlayCombatResearchAttemptV1, String> {
    let mut merged_candidates = Vec::<TaskResearchPlanCandidateV1>::new();
    let mut merged_witness_lines = Vec::<ExactRunPlayProbeLineV1>::new();
    let mut merged_lanes = Vec::<TaskResearchPlanLaneAttemptV1>::new();
    let mut lane_ordinals = BTreeMap::<TaskPrivilegedSupportProvenanceV1, usize>::new();
    let mut merged_milestones = Vec::<RunPlayCombatMilestoneResearchCandidateV1>::new();
    let mut merged_progress = Vec::<RunPlayCombatResearchBoundedProgressV1>::new();

    for pass in passes {
        pass.attempt.validate()?;
        if pass.attempt.candidates.len() != pass.witness_lines.len() {
            return Err(
                "combat research pass candidates and replay witnesses disagree".to_string(),
            );
        }

        for lane in pass.attempt.lanes {
            if let Some(&ordinal) = lane_ordinals.get(&lane.provenance) {
                let existing = &mut merged_lanes[ordinal];
                if existing.work_unit != lane.work_unit {
                    return Err(format!(
                        "combat research producer {:?} changed work units across effort passes",
                        lane.provenance.producer_name
                    ));
                }
                existing.work_consumed = existing
                    .work_consumed
                    .checked_add(lane.work_consumed)
                    .ok_or_else(|| "combat research cumulative lane work overflowed".to_string())?;
                let existing_has_evidence = existing.exit.has_witness()
                    || existing.exit == TaskResearchPlanAttemptExitV1::MilestoneCandidates;
                let incoming_has_stronger_evidence = lane.exit.has_witness()
                    || (lane.exit == TaskResearchPlanAttemptExitV1::MilestoneCandidates
                        && !existing.exit.has_witness());
                if incoming_has_stronger_evidence || !existing_has_evidence {
                    existing.exit = lane.exit;
                }
            } else {
                lane_ordinals.insert(lane.provenance.clone(), merged_lanes.len());
                merged_lanes.push(lane);
            }
        }

        for (candidate, witness) in pass.attempt.candidates.into_iter().zip(pass.witness_lines) {
            if let Some(ordinal) = merged_candidates
                .iter()
                .position(|existing| existing.plan == candidate.plan)
            {
                let existing = &mut merged_candidates[ordinal];
                if existing.outcome != candidate.outcome
                    || existing.satisfies_requested_quality != candidate.satisfies_requested_quality
                {
                    return Err(
                        "the same combat research plan produced conflicting verified evidence"
                            .to_string(),
                    );
                }
                existing
                    .supporting_provenances
                    .extend(candidate.supporting_provenances);
                existing.supporting_provenances.sort();
                existing.supporting_provenances.dedup();
            } else {
                merged_candidates.push(candidate);
                merged_witness_lines.push(witness);
            }
        }

        for milestone in pass.milestone_candidates {
            if let Some(existing) = merged_milestones
                .iter()
                .find(|existing| existing.candidate_id == milestone.candidate_id)
            {
                if existing.candidate != milestone.candidate
                    || existing.successor_public_snapshot_id
                        != milestone.successor_public_snapshot_id
                    || existing.successor != milestone.successor
                    || existing.action_line != milestone.action_line
                {
                    return Err(format!(
                        "combat milestone id {:?} produced conflicting verified successors",
                        milestone.candidate_id
                    ));
                }
            } else {
                merged_milestones.push(milestone);
            }
        }

        for progress in pass.bounded_progress {
            if let Some(existing) = merged_progress
                .iter_mut()
                .find(|existing| existing.provenance == progress.provenance)
            {
                existing.max_player_turn = existing.max_player_turn.max(progress.max_player_turn);
                for state in progress.recent_turn_survival_envelope {
                    if !existing.recent_turn_survival_envelope.contains(&state) {
                        existing.recent_turn_survival_envelope.push(state);
                    }
                }
                existing.recent_turn_survival_envelope.sort_by_key(|state| {
                    (
                        state.player_turn,
                        state.player_hp,
                        state.player_block,
                        state.alive_enemy_count,
                        state.enemy_total_hp,
                        state.hand_size,
                        state.draw_pile_size,
                        state.discard_pile_size,
                        state.exhaust_pile_size,
                        state.path_atomic_depth,
                    )
                });
                for mut path in progress.paths {
                    if let Some(retained) = existing
                        .paths
                        .iter_mut()
                        .find(|retained| retained.candidate.plan == path.candidate.plan)
                    {
                        if retained.candidate.endpoint != path.candidate.endpoint
                            || retained.successor_public_snapshot_id
                                != path.successor_public_snapshot_id
                            || retained.action_line != path.action_line
                        {
                            return Err(
                                "one bounded progress prefix produced conflicting replay evidence"
                                    .to_string(),
                            );
                        }
                        retained.candidate.roles.append(&mut path.candidate.roles);
                        retained.candidate.roles.sort();
                        retained.candidate.roles.dedup();
                    } else {
                        existing.paths.push(path);
                    }
                }
            } else {
                merged_progress.push(progress);
            }
        }
    }

    if !merged_candidates.is_empty() {
        merged_progress.clear();
    }
    for progress in &mut merged_progress {
        progress.paths.sort_by(|left, right| {
            left.candidate
                .roles
                .cmp(&right.candidate.roles)
                .then_with(|| {
                    left.successor_public_snapshot_id
                        .cmp(&right.successor_public_snapshot_id)
                })
        });
    }
    merged_progress.sort_by(|left, right| left.provenance.cmp(&right.provenance));

    let attempt = sts_oracle_learning::agent::task::TaskResearchPlanAttemptV1 {
        candidates: merged_candidates,
        lanes: merged_lanes,
    };
    attempt.validate()?;
    Ok(RunPlayCombatResearchAttemptV1 {
        attempt,
        witness_lines: merged_witness_lines,
        milestone_candidates: merged_milestones,
        bounded_progress: merged_progress,
    })
}

fn default_combat_search_ms() -> u64 {
    10_000
}

fn combat_research_attempt_for_effort_v1(
    controller: &mut RunPlayControllerV1,
    effort: AgentCombatResearchEffortV1,
    max_generation_work: Option<usize>,
    max_potions_used: Option<u32>,
    deadline: std::time::Instant,
) -> Result<RunPlayCombatResearchAttemptV1, String> {
    combat_research_attempt_for_effort_with_progress_v1(
        controller,
        effort,
        max_generation_work,
        max_potions_used,
        Some(deadline),
        &mut |_| {},
    )
}

fn combat_research_attempt_for_effort_with_progress_v1(
    controller: &mut RunPlayControllerV1,
    effort: AgentCombatResearchEffortV1,
    max_generation_work: Option<usize>,
    max_potions_used: Option<u32>,
    deadline: Option<std::time::Instant>,
    progress: &mut dyn FnMut(CombatResearchProgressEventV1),
) -> Result<RunPlayCombatResearchAttemptV1, String> {
    if effort != AgentCombatResearchEffortV1::Expanded {
        let mut options = combat_research_options_for_effort_v1(
            &controller.head().combat_plan_options,
            effort,
            deadline.map(|limit| {
                limit
                    .saturating_duration_since(std::time::Instant::now())
                    .as_millis() as u64
            }),
        );
        if let Some(work) = max_generation_work {
            options.max_generation_work = work;
        }
        restrict_combat_research_potions_v1(&mut options, max_potions_used);
        return controller.combat_research_attempt_with_options_and_progress_v1(
            Some(options),
            deadline,
            progress,
        );
    }

    let mut standard_options = combat_research_options_for_effort_v1(
        &controller.head().combat_plan_options,
        AgentCombatResearchEffortV1::Standard,
        None,
    );
    restrict_combat_research_potions_v1(&mut standard_options, max_potions_used);
    let standard = controller.combat_research_attempt_with_options_and_progress_v1(
        Some(standard_options),
        deadline,
        progress,
    )?;
    if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
        return Ok(standard);
    }
    let mut expanded_options = combat_research_options_for_effort_v1(
        &controller.head().combat_plan_options,
        AgentCombatResearchEffortV1::Expanded,
        None,
    );
    restrict_combat_research_potions_v1(&mut expanded_options, max_potions_used);
    let expanded = controller.combat_research_attempt_with_options_and_progress_v1(
        Some(expanded_options),
        deadline,
        progress,
    )?;
    merge_combat_research_passes_v1([standard, expanded])
}

fn combat_turn_continuation_audit_options_for_effort_v1(
    base: &CombatResearchPlanOptionsV1,
    effort: AgentCombatResearchEffortV1,
) -> RunPlayCombatTurnContinuationAuditOptionsV1 {
    let total_max_potions_used = combat_research_potion_limit_for_effort_v1(base, effort);
    let (max_generation_work, wall_ms) = match effort {
        AgentCombatResearchEffortV1::Standard
        | AgentCombatResearchEffortV1::FastWitness
        | AgentCombatResearchEffortV1::WholeCombat => (250_000, 5_000),
        AgentCombatResearchEffortV1::Expanded => (1_000_000, 15_000),
    };
    RunPlayCombatTurnContinuationAuditOptionsV1 {
        max_generation_work,
        max_selections: 1_000_000,
        wall_ms,
        max_engine_steps_per_transition: base.max_engine_steps_per_transition.max(250),
        uniform_exploration_ppm: 50_000,
        generation_quantum_work: 4,
        max_turn_depth: 32,
        total_max_potions_used,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPermissionEnvelopeV1 {
    pub profile: &'static str,
    pub information_ceiling: &'static str,
    pub exact_replay_ceiling: &'static str,
    pub combat_search: &'static str,
    pub hardcoded_expert_knowledge: &'static str,
    pub knowledge_pack_reference: Option<String>,
}

impl AgentPermissionEnvelopeV1 {
    pub fn public_manual_v1(knowledge_pack_reference: Option<String>) -> Self {
        Self {
            profile: "public_manual_v1",
            information_ceiling: "public_only",
            exact_replay_ceiling: "retained_returns_allowed",
            combat_search: "disabled",
            hardcoded_expert_knowledge: "allowed",
            knowledge_pack_reference,
        }
    }

    pub fn a20_first_unrestricted_v1(knowledge_pack_reference: Option<String>) -> Self {
        Self {
            profile: "a20_first_unrestricted_v1",
            information_ceiling: "privileged_exact_allowed",
            exact_replay_ceiling: "unlimited_allowed",
            combat_search: "allowed",
            hardcoded_expert_knowledge: "allowed",
            knowledge_pack_reference,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCapabilitiesV1 {
    /// A permission ceiling is not a claim that every privileged operation is
    /// already exposed by this protocol version.
    pub permission_envelope: AgentPermissionEnvelopeV1,
    pub observation_payload: &'static str,
    pub action_addressing: &'static str,
    pub strategic_advice_authority: &'static str,
    pub combat_satisfaction_role: &'static str,
    pub available_commands: Vec<&'static str>,
}

impl AgentCapabilitiesV1 {
    pub fn new(permission_envelope: AgentPermissionEnvelopeV1) -> Self {
        let manual = permission_envelope.information_ceiling == "public_only";
        let mut capabilities = Self {
            permission_envelope,
            observation_payload: "typed_public",
            action_addressing: "source_bound_choice_or_legacy_semantic",
            strategic_advice_authority: "shadow_advisor_only",
            combat_satisfaction_role: "bounded_search_request_not_strategic_rank",
            available_commands: vec![
                "decision",
                "choose",
                "hello",
                "observe",
                "game_facts",
                "info",
                "sl_piles",
                "probe",
                "combat_research",
                "combat_review",
                "combat_plans",
                "combat_milestone_successor_details",
                "combat_milestone_continuation_audit",
                "combat_prefix_horizon_audit",
                "combat_draft_status",
                "combat_draft_open",
                "combat_draft_advance",
                "combat_draft_inspect",
                "combat_draft_choose",
                "combat_draft_undo",
                "combat_draft_selection",
                "commit_combat_plan",
                "commit_combat_milestone",
                "combat_turn_research",
                "combat_turn_reserve",
                "combat_turn_candidate_details",
                "combat_turn_continuation_audit",
                "commit_combat_turn",
                "act",
                "quit",
            ],
        };
        if manual {
            capabilities.available_commands.retain(|command| {
                matches!(
                    *command,
                    "decision"
                        | "choose"
                        | "hello"
                        | "observe"
                        | "game_facts"
                        | "info"
                        | "act"
                        | "quit"
                )
            });
        }
        capabilities
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatResearchCandidateV1 {
    pub candidate_id: String,
    /// Zero-based index printed by the complete combat-research report before
    /// the artifact projects its non-dominated resource frontier. Legacy
    /// artifacts predate this field and retain their local-index behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_candidate_index: Option<usize>,
    pub supporting_provenances: Vec<TaskPrivilegedSupportProvenanceV1>,
    pub plan_steps: usize,
    /// Minimum HP at the original replay's public action boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_observed_hp: Option<i32>,
    pub satisfies_requested_quality: bool,
    pub outcome: TaskResearchPlanOutcomeV1,
    pub action_line: Vec<String>,
}

/// Process-local projection of one durable combat-research candidate. The
/// compact reference is valid only for the current hot cache; `candidate_id`
/// remains the complete content identity retained by artifacts and traces.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentLiveCombatResearchCandidateV1 {
    pub candidate_ref: String,
    pub plan_id: String,
    pub candidate_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_candidate_index: Option<usize>,
    pub supporting_provenances: Vec<TaskPrivilegedSupportProvenanceV1>,
    pub plan_steps: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_observed_hp: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_incense_turn: Option<u32>,
    pub meets_requested_satisfaction: bool,
    pub outcome: TaskResearchPlanOutcomeV1,
    pub action_line: Vec<String>,
}

fn project_live_combat_research_candidates_v1(
    candidates: &[AgentCombatResearchCandidateV1],
    report_id: &str,
) -> Vec<AgentLiveCombatResearchCandidateV1> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| AgentLiveCombatResearchCandidateV1 {
            candidate_ref: format!("c{index}"),
            plan_id: crate::combat_reading::plan_id(report_id, &format!("c{index}")),
            candidate_id: candidate.candidate_id.clone(),
            report_candidate_index: candidate.report_candidate_index,
            supporting_provenances: candidate.supporting_provenances.clone(),
            plan_steps: candidate.plan_steps,
            minimum_observed_hp: candidate.minimum_observed_hp,
            next_incense_turn: candidate
                .outcome
                .final_relics
                .iter()
                .find(|relic| {
                    relic.id == sts_oracle_learning::content::relics::RelicId::IncenseBurner
                })
                .and_then(|relic| {
                    sts_oracle_learning::content::relics::incense_burner::next_combat_trigger_turn(
                        relic.counter,
                    )
                }),
            meets_requested_satisfaction: candidate.satisfies_requested_quality,
            outcome: candidate.outcome.clone(),
            action_line: candidate.action_line.clone(),
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCombatObservedTerminalSourceV1 {
    CompleteTurnContinuation,
    CompleteCombatPlan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatObservedTerminalOutcomeV1 {
    pub action: String,
    pub source: AgentCombatObservedTerminalSourceV1,
    pub outcome: TaskResearchPlanOutcomeV1,
}

fn retain_agent_combat_observed_terminal_outcome_v1(
    frontier: &mut Vec<AgentCombatObservedTerminalOutcomeV1>,
    candidate: AgentCombatObservedTerminalOutcomeV1,
) {
    if frontier.iter().any(|existing| {
        existing
            .outcome
            .strictly_dominates_with_bounded_relic_progress_equivalence_v1(&candidate.outcome)
    }) {
        return;
    }
    frontier.retain(|existing| {
        !candidate
            .outcome
            .strictly_dominates_with_bounded_relic_progress_equivalence_v1(&existing.outcome)
    });
    frontier.push(candidate);
}

/// One durable, semantic replay step for a combat-research candidate.  The
/// source snapshot prevents a still-legal action from silently replaying at a
/// different public state after simulator or selector changes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatResearchReplayStepV1 {
    pub source_public_snapshot_id: String,
    pub action: String,
}

/// The exact public boundary expected after the final replay step.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentCombatResearchReplayEndpointV1 {
    PublicDecision { public_snapshot_id: String },
    GameTerminal { outcome: TaskEpisodeOutcomeV1 },
}

/// Root-bound execution proof retained beside one projected research
/// candidate.  Legacy artifacts may omit the collection containing these
/// proofs and remain readable, but are deliberately inspection-only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatResearchReplayProofV1 {
    pub candidate_id: String,
    pub steps: Vec<AgentCombatResearchReplayStepV1>,
    pub endpoint: AgentCombatResearchReplayEndpointV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatMilestoneCandidateV1 {
    pub candidate_id: String,
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub plan_steps: usize,
    pub completed_milestones: Vec<sts_combat_strategy::CombatPlanMilestoneV1>,
    pub final_player_hp: i32,
    pub combat_continues: bool,
    pub action_line: Vec<String>,
    pub successor_public_snapshot_id: String,
    /// Public successor facts which are cheap enough to retain beside every
    /// exact lineage. This is diagnostic evidence, not a value score: none of
    /// these fields proves that the visible attack can be blocked or that the
    /// continuation wins.
    #[serde(default)]
    pub successor_summary: AgentCombatMilestoneSuccessorSummaryV1,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatMilestoneSuccessorSummaryV1 {
    pub turn_count: u32,
    pub current_energy: u8,
    pub player_hp: i32,
    pub player_block: i32,
    pub visible_incoming_damage: i32,
    pub visible_damage_margin: i32,
    /// Sum of the public `current_block` fields in the current hand. This does
    /// not assume that every card is jointly playable or that future draws
    /// are known.
    pub visible_hand_block_sum: i32,
    pub remaining_hand_block_sources: u16,
    pub remaining_accessible_block_sources: u16,
    pub remaining_strength_reduction: u16,
    pub live_status_cards: u16,
    pub enemy_team_strength: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatMilestoneCardIdentityV1 {
    pub card_id: CardId,
    pub upgrades: u8,
    pub misc_value: i32,
    pub base_damage_override: Option<i32>,
    pub base_block_override: Option<i32>,
    pub cost_modifier: i8,
}

impl From<&CombatLearningCardV1> for AgentCombatMilestoneCardIdentityV1 {
    fn from(card: &CombatLearningCardV1) -> Self {
        Self {
            card_id: card.card_id,
            upgrades: card.upgrades,
            misc_value: card.misc_value,
            base_damage_override: card.base_damage_override,
            base_block_override: card.base_block_override,
            cost_modifier: card.cost_modifier,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatMilestoneCardZonePreviewV1 {
    pub evidence: ObservationEvidenceKindV1,
    pub cards: Vec<AgentCombatMilestoneCardIdentityV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatMilestoneMonsterPreviewV1 {
    pub slot: u8,
    pub enemy: CombatLearningEnemyIdentityV1,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub escaped: bool,
    pub dying: bool,
    pub half_dead: bool,
    pub intent: CombatLearningIntentV1,
    pub executed_moves: CombatLearningMonsterMoveHistoryV1,
    pub public_counters: Vec<CombatLearningMonsterPublicCounterV1>,
    pub powers: Vec<CombatLearningPowerV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatMilestoneSuccessorPreviewV1 {
    pub successor_public_snapshot_id: String,
    pub turn: CombatLearningTurnV1,
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub player_block: i32,
    pub player_relics: Vec<RelicState>,
    pub player_powers: Vec<CombatLearningPowerV1>,
    pub potions: Vec<Option<CombatLearningPotionV1>>,
    pub hand: Vec<CombatLearningCardV1>,
    pub draw: AgentCombatMilestoneCardZonePreviewV1,
    pub discard: AgentCombatMilestoneCardZonePreviewV1,
    pub exhaust: AgentCombatMilestoneCardZonePreviewV1,
    pub monsters: Vec<AgentCombatMilestoneMonsterPreviewV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatResearchBoundedProgressPathV1 {
    pub roles: Vec<CombatResearchBoundedProgressRoleV1>,
    pub plan_steps: usize,
    pub endpoint: CombatResearchBoundedProgressStateV1,
    pub action_line: Vec<String>,
    pub successor_public_snapshot_id: String,
}

/// Producer-owned observations from a finite whole-combat search. These
/// prefixes explain a bounded miss, but never enter the terminal frontier or
/// become commit candidates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatResearchBoundedProgressV1 {
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub bounded_search_only: bool,
    pub absence_means_unknown: bool,
    pub max_player_turn: u32,
    pub recent_turn_survival_envelope: Vec<CombatResearchBoundedProgressStateV1>,
    pub paths: Vec<AgentCombatResearchBoundedProgressPathV1>,
}

/// Durable, read-only output from one combat-research invocation. Candidate
/// action lines and public successor snapshots are retained together so later
/// typed inspectors can audit a bounded horizon without repeating the
/// expensive research portfolio. This artifact proves reachability of each
/// recorded prefix, never terminal victory for a milestone candidate.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatResearchArtifactV1 {
    pub schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_evidence_identity: Option<RunPlayEvidenceIdentityV1>,
    pub session_id: String,
    pub root_head_revision: u64,
    pub root_state_revision: u64,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub effort: AgentCombatResearchEffortV1,
    pub durable_head_unchanged: bool,
    pub lanes: Vec<TaskResearchPlanLaneAttemptV1>,
    #[serde(default)]
    pub bounded_progress: Vec<AgentCombatResearchBoundedProgressV1>,
    pub candidates: Vec<AgentCombatResearchCandidateV1>,
    /// Present on newly captured artifacts. An empty collection identifies a
    /// legacy, read-only artifact that cannot be transactionally anchored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_replay_proofs: Vec<AgentCombatResearchReplayProofV1>,
    pub milestone_candidates: Vec<AgentCombatMilestoneCandidateV1>,
    pub milestone_successor_previews: Vec<AgentCombatMilestoneSuccessorPreviewV1>,
}

impl RunPlayCombatResearchArtifactV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1 {
            return Err(format!(
                "unsupported combat research artifact schema {:?}; expected {COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1}",
                self.schema
            ));
        }
        if self.session_id.trim().is_empty()
            || self.root_archive_node_id.trim().is_empty()
            || self.root_public_snapshot_id.trim().is_empty()
        {
            return Err("combat research artifact root identity is incomplete".to_string());
        }
        if !self.durable_head_unchanged {
            return Err("combat research artifact must preserve the durable head".to_string());
        }
        if self.lanes.is_empty() {
            return Err("combat research artifact contains no research lanes".to_string());
        }
        if !self.candidates.is_empty() && !self.bounded_progress.is_empty() {
            return Err(
                "bounded combat progress is valid only for a terminal-candidate miss".to_string(),
            );
        }

        let lane_provenances = self
            .lanes
            .iter()
            .map(|lane| &lane.provenance)
            .collect::<BTreeSet<_>>();
        let mut progress_provenances = BTreeSet::new();
        for progress in &self.bounded_progress {
            if !progress.bounded_search_only
                || !progress.absence_means_unknown
                || !lane_provenances.contains(&progress.provenance)
                || !progress_provenances.insert(&progress.provenance)
                || progress
                    .recent_turn_survival_envelope
                    .iter()
                    .any(|state| state.player_turn > progress.max_player_turn)
            {
                return Err(
                    "bounded combat progress must be unique, lane-owned, finite Unknown evidence"
                        .to_string(),
                );
            }
            for path in &progress.paths {
                let mut sorted_roles = path.roles.clone();
                sorted_roles.sort();
                sorted_roles.dedup();
                if path.roles.is_empty()
                    || path.roles != sorted_roles
                    || path.plan_steps == 0
                    || path.plan_steps != path.action_line.len()
                    || path.endpoint.path_atomic_depth != path.action_line.len()
                    || path.endpoint.player_turn > progress.max_player_turn
                    || path.successor_public_snapshot_id.trim().is_empty()
                    || path
                        .action_line
                        .iter()
                        .any(|action| action.trim().is_empty())
                {
                    return Err(
                        "bounded combat progress paths must be replayable, role-unique prefixes"
                            .to_string(),
                    );
                }
            }
        }

        let mut candidate_ids = HashSet::new();
        let mut report_candidate_indices = HashSet::new();
        let candidates_with_report_index = self
            .candidates
            .iter()
            .filter(|candidate| candidate.report_candidate_index.is_some())
            .count();
        if candidates_with_report_index != 0
            && candidates_with_report_index != self.candidates.len()
        {
            return Err(
                "combat research artifact candidates must either all retain report indices or all use legacy local indices"
                    .to_string(),
            );
        }
        for candidate in &self.candidates {
            if candidate.candidate_id.trim().is_empty()
                || candidate.action_line.is_empty()
                || !candidate_ids.insert(candidate.candidate_id.as_str())
                || candidate
                    .report_candidate_index
                    .is_some_and(|index| !report_candidate_indices.insert(index))
            {
                return Err(
                    "combat research artifact candidates require distinct ids, distinct report indices, and non-empty action lines"
                        .to_string(),
                );
            }
        }
        let candidates_by_id = self
            .candidates
            .iter()
            .map(|candidate| (candidate.candidate_id.as_str(), candidate))
            .collect::<BTreeMap<_, _>>();
        let mut replay_candidate_ids = HashSet::new();
        for replay in &self.candidate_replay_proofs {
            let candidate = candidates_by_id
                .get(replay.candidate_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "combat research replay names absent candidate {:?}",
                        replay.candidate_id
                    )
                })?;
            if !replay_candidate_ids.insert(replay.candidate_id.as_str())
                || replay.steps.is_empty()
                || replay.steps.len() != candidate.plan_steps
                || replay.steps.len() != candidate.action_line.len()
            {
                return Err(
                    "combat research replay requires one non-empty, length-matched proof per candidate"
                        .to_string(),
                );
            }
            if replay
                .steps
                .first()
                .map(|step| step.source_public_snapshot_id.as_str())
                != Some(self.root_public_snapshot_id.as_str())
                || replay.steps.iter().enumerate().any(|(index, step)| {
                    step.source_public_snapshot_id.trim().is_empty()
                        || step.action.trim().is_empty()
                        || step.action != candidate.action_line[index]
                })
            {
                return Err(
                    "combat research replay steps disagree with the root or projected action line"
                        .to_string(),
                );
            }
            match &replay.endpoint {
                AgentCombatResearchReplayEndpointV1::PublicDecision { public_snapshot_id }
                    if public_snapshot_id.trim().is_empty() =>
                {
                    return Err("combat research replay public endpoint is empty".to_string());
                }
                AgentCombatResearchReplayEndpointV1::GameTerminal {
                    outcome: TaskEpisodeOutcomeV1::Paused { .. },
                } => {
                    return Err(
                        "combat research replay terminal endpoint cannot be paused".to_string()
                    );
                }
                _ => {}
            }
        }
        if !self.candidate_replay_proofs.is_empty()
            && replay_candidate_ids.len() != self.candidates.len()
        {
            return Err(
                "combat research replay proof set must cover every candidate or be absent"
                    .to_string(),
            );
        }
        for candidate in &self.milestone_candidates {
            if candidate.candidate_id.trim().is_empty()
                || candidate.action_line.is_empty()
                || !candidate_ids.insert(candidate.candidate_id.as_str())
            {
                return Err(
                    "combat research artifact milestones require a distinct id and non-empty action line"
                        .to_string(),
                );
            }
            if !candidate.combat_continues
                || candidate.successor_public_snapshot_id.trim().is_empty()
            {
                return Err(
                    "combat research milestone must name a continuing public successor".to_string(),
                );
            }
        }

        let mut preview_snapshots = HashSet::new();
        for preview in &self.milestone_successor_previews {
            if preview.successor_public_snapshot_id.trim().is_empty()
                || !preview_snapshots.insert(preview.successor_public_snapshot_id.as_str())
            {
                return Err(
                    "combat research successor snapshot ids must be non-empty and distinct"
                        .to_string(),
                );
            }
        }
        let candidate_snapshots = self
            .milestone_candidates
            .iter()
            .map(|candidate| candidate.successor_public_snapshot_id.as_str())
            .collect::<HashSet<_>>();
        if candidate_snapshots != preview_snapshots {
            return Err(
                "combat research milestone and public successor preview sets disagree".to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatMilestoneSuccessorDetailV1 {
    pub candidate_id: String,
    pub successor_public_snapshot_id: String,
    pub transition: sts_combat_strategy::CombatPlanTransitionAnnotationV1,
    pub successor_plan: sts_combat_strategy::CombatPlanProjectionV1,
    pub successor: PublicCombatStateV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatTurnCardSummaryV1 {
    pub card_id: CardId,
    pub upgrades: u8,
    pub effective_cost: i32,
    pub energy_on_use: i32,
}

impl From<&CombatLearningCardV1> for AgentCombatTurnCardSummaryV1 {
    fn from(card: &CombatLearningCardV1) -> Self {
        Self {
            card_id: card.card_id,
            upgrades: card.upgrades,
            effective_cost: card.effective_cost,
            energy_on_use: card.energy_on_use,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentCombatTurnActionSummaryV1 {
    PlayCard {
        hand_index: usize,
        card: AgentCombatTurnCardSummaryV1,
        target_monster_index: Option<usize>,
    },
    UsePotion {
        potion_index: usize,
        potion_id: PotionId,
        target_monster_index: Option<usize>,
    },
    DiscardPotion {
        potion_index: usize,
        potion_id: PotionId,
    },
    EndTurn,
    SubmitIndexedChoice {
        choice_index: usize,
    },
    Proceed,
    Cancel,
    Selection {
        reason: CombatSelectionReasonV2,
        source_pile: Option<PileType>,
        selected_domain_ordinals: Vec<usize>,
        selected_candidates: Vec<PublicCombatSelectionDomainCandidateV1>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatTurnPowerSummaryV1 {
    pub power: PowerId,
    pub amount: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatTurnMonsterSummaryV1 {
    pub slot: u8,
    pub enemy: CombatLearningEnemyIdentityV1,
    pub hp: i32,
    pub block: i32,
    pub alive: bool,
    pub powers: Vec<AgentCombatTurnPowerSummaryV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatTurnEndpointSummaryV1 {
    pub turn_count: u32,
    pub player_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub player_powers: Vec<AgentCombatTurnPowerSummaryV1>,
    pub potion_ids: Vec<Option<PotionId>>,
    pub hand: Vec<AgentCombatTurnCardSummaryV1>,
    pub draw_count: usize,
    pub discard_count: usize,
    pub exhaust_count: usize,
    pub monsters: Vec<AgentCombatTurnMonsterSummaryV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCombatTurnCandidateSummaryV1 {
    pub candidate_id: String,
    pub actions: Vec<AgentCombatTurnActionSummaryV1>,
    pub boundary: ExactCompleteTurnProposalBoundaryV1,
    pub potion_slot_mask: u64,
    pub potion_expenditures: u32,
    pub endpoint: AgentCombatTurnEndpointSummaryV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentObservationV1 {
    pub schema: &'static str,
    pub session_id: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub capabilities: AgentCapabilitiesV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_evidence: Option<AgentDecisionEvidenceV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck_evidence: Option<AgentDeckEvidenceV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation_reading: Option<crate::decision::preparation::PreparationReadingV1>,
    pub position: AgentPositionV1,
}

/// Public permanent inventory and shared mechanics, without combat RNG or piles.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDeckEvidenceV1 {
    pub schema: &'static str,
    pub public_snapshot_id: String,
    pub cards: Vec<AgentOwnedCardEvidenceV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentOwnedCardEvidenceV1 {
    pub card: AgentStrategicCardGroupV1,
    pub mechanics: CardRewardFactsV1,
    pub persistent_cost: i32,
    pub persistent_damage: i32,
    pub persistent_block: i32,
    /// Detached one-step upgrade of this persistent instance, not a legal action
    /// or a claim that an upgrade has happened in the current run or Hand.
    pub next_upgrade: Option<CardOwnedStateReadingV1>,
}

fn project_agent_deck_evidence_v1(
    observation: &AgentStrategicObservationV1,
    public_snapshot_id: &str,
) -> AgentDeckEvidenceV1 {
    use sts_oracle_learning::runtime::combat::CardBlueprint;
    AgentDeckEvidenceV1 {
        schema: "sts-agent-deck-evidence-v1",
        public_snapshot_id: public_snapshot_id.to_string(),
        cards: observation
            .cards
            .iter()
            .map(|card| {
                let blueprint = CardBlueprint {
                    upgrades: card.upgrades,
                    misc_value: card.misc_value,
                    base_damage_override: card.base_damage_override,
                    base_block_override: card.base_block_override,
                    cost_modifier: card.cost_modifier,
                    ..CardBlueprint::fresh(card.card)
                };
                let reading = card_owned_reading_v1(&blueprint);
                AgentOwnedCardEvidenceV1 {
                    card: card.clone(),
                    mechanics: card_reward_facts_v1(&RewardCard::new(card.card, card.upgrades)),
                    persistent_cost: reading.current.persistent_cost,
                    persistent_damage: reading.current.persistent_damage,
                    persistent_block: reading.current.persistent_block,
                    next_upgrade: reading.next_upgrade,
                }
            })
            .collect(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentDecisionEvidenceV1 {
    RoutePaths {
        route_semantics_manifest: AgentRouteSemanticsManifestV1,
        candidates: Vec<AgentRoutePathEvidenceV1>,
    },
    RouteEntryPaths {
        route_semantics_manifest: AgentRouteSemanticsManifestV1,
        candidates: Vec<AgentRouteEntryPathEvidenceV1>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        limited_combat_coverages: Vec<AgentLimitedCombatCoverageEvidenceV1>,
    },
    CampfireOptions {
        candidates: Vec<AgentCampfireOptionEvidenceV1>,
    },
    BossRelicOptions {
        candidates: Vec<AgentRelicAcquisitionEvidenceV1>,
        owned_hp_loss_sources: Vec<AgentHpLossSourceEvidenceV1>,
    },
    CardAcquisitionOptions {
        candidates: Vec<AgentCardAcquisitionEvidenceV1>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        resource_options: Vec<AgentCardRewardResourceEvidenceV1>,
        route_semantics_manifest: AgentRouteSemanticsManifestV1,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        route_entries: Vec<AgentRouteEntryPathEvidenceV1>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCardAcquisitionEvidenceV1 {
    pub semantic_action: String,
    pub cards: Vec<CardAcquisitionMechanicsFactsV1>,
}

/// Immediate public reward alternative. Deltas come from a one-step native
/// preview, including healing restrictions; no successor inventory is exposed.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCardRewardResourceEvidenceV1 {
    pub semantic_action: String,
    pub hp_gain: i32,
    pub max_hp_gain: i32,
    pub native_rule: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRelicAcquisitionEvidenceV1 {
    pub semantic_action: String,
    pub definition: RelicAcquisitionMechanicsFactsV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentHpLossSourceEvidenceV1 {
    pub card: AgentStrategicCardGroupV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_play: Option<sts_oracle_learning::content::cards::mechanics::CardSelfHpLossOnPlayV1>,
    pub hp_loss_each_turn_after_deployment: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_of_turn_after_deployment:
        Option<sts_oracle_learning::content::cards::mechanics::CardEndOfTurnHpLossAndDamageV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRoutePathEvidenceV1 {
    pub semantic_action: String,
    pub x: i32,
    pub y: i32,
    pub arrival: RoutePolicyArrivalV1,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub question_mark_outcomes: Vec<PlannerQuestionMarkOutcome>,
    pub route_semantics: AgentRouteSemanticsV1,
    pub path: RoutePolicyPathEvidenceV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRouteEntryPathEvidenceV1 {
    pub x: i32,
    pub y: i32,
    pub room: Option<PlannerMapRoom>,
    pub has_emerald_key: bool,
    pub uses_wing_boots: bool,
    pub coverage: RouteWindowCoverageKind,
    pub route_semantics: AgentRouteSemanticsV1,
    pub path: RoutePathFamilySummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentObservedPathPresenceV1 {
    NoObservedPaths,
    NoneOfObservedPaths,
    SomeObservedPaths,
    AllObservedPaths,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentObservedCountRangeV1 {
    pub min: usize,
    pub max: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentObservedCountPresenceV1 {
    pub count_per_observed_path: AgentObservedCountRangeV1,
    pub presence: AgentObservedPathPresenceV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentObservedRoomAccessV1 {
    #[serde(flatten)]
    pub observed: AgentObservedCountPresenceV1,
    pub earliest_possible_floor: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFirstEliteRouteSemanticsV1 {
    pub presence: AgentObservedPathPresenceV1,
    pub observed_paths_with_first_elite: usize,
    pub before_counts_scope: &'static str,
    pub shops_before: AgentObservedCountPresenceV1,
    pub campfires_before: AgentObservedCountPresenceV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRouteSemanticsV1 {
    pub observed_path_count: usize,
    pub path_counts_are_not_probabilities: bool,
    pub early_combat_rooms_on_map_rows_0_through_3: AgentObservedCountRangeV1,
    pub early_combat_room_count_has_no_inherent_preference_direction: bool,
    pub shops: AgentObservedRoomAccessV1,
    pub campfires: AgentObservedRoomAccessV1,
    pub first_elite: AgentFirstEliteRouteSemanticsV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRouteSemanticsManifestV1 {
    pub raw_path_fields_represented: Vec<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFirstEliteCombatCoverageV1 {
    CannotCover,
    MayCover,
    CoversWithinVisibleRoomUncertainty,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRouteEntryCombatCoverageV1 {
    pub x: i32,
    pub y: i32,
    pub earliest_first_elite_combat_number: Option<usize>,
    pub latest_first_elite_combat_number_if_all_prior_unknowns_are_combats: Option<usize>,
    pub first_elite_coverage: AgentFirstEliteCombatCoverageV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentLimitedCombatCoverageEvidenceV1 {
    pub semantic_action: String,
    pub source_relic: RelicId,
    pub affected_combat_count: usize,
    pub route_entries: Vec<AgentRouteEntryCombatCoverageV1>,
}

fn observed_count_presence_v1(
    observed_path_count: usize,
    min: usize,
    max: usize,
) -> AgentObservedCountPresenceV1 {
    let presence = if observed_path_count == 0 {
        AgentObservedPathPresenceV1::NoObservedPaths
    } else if max == 0 {
        AgentObservedPathPresenceV1::NoneOfObservedPaths
    } else if min > 0 {
        AgentObservedPathPresenceV1::AllObservedPaths
    } else {
        AgentObservedPathPresenceV1::SomeObservedPaths
    };
    AgentObservedCountPresenceV1 {
        count_per_observed_path: AgentObservedCountRangeV1 { min, max },
        presence,
    }
}

fn matching_path_presence_v1(
    observed_path_count: usize,
    matching_path_count: usize,
) -> AgentObservedPathPresenceV1 {
    if observed_path_count == 0 {
        AgentObservedPathPresenceV1::NoObservedPaths
    } else if matching_path_count == 0 {
        AgentObservedPathPresenceV1::NoneOfObservedPaths
    } else if matching_path_count == observed_path_count {
        AgentObservedPathPresenceV1::AllObservedPaths
    } else {
        AgentObservedPathPresenceV1::SomeObservedPaths
    }
}

struct RouteSemanticsInputV1<'a> {
    observed_path_count: usize,
    min_early_pressure: usize,
    max_early_pressure: usize,
    min_shops: usize,
    max_shops: usize,
    first_shop_floor: Option<i32>,
    min_campfires: usize,
    max_campfires: usize,
    first_campfire_floor: Option<i32>,
    first_elite: &'a RouteFirstEliteSegment,
}

fn route_semantics_v1(input: RouteSemanticsInputV1<'_>) -> AgentRouteSemanticsV1 {
    let RouteSemanticsInputV1 {
        observed_path_count,
        min_early_pressure,
        max_early_pressure,
        min_shops,
        max_shops,
        first_shop_floor,
        min_campfires,
        max_campfires,
        first_campfire_floor,
        first_elite,
    } = input;
    AgentRouteSemanticsV1 {
        observed_path_count,
        path_counts_are_not_probabilities: true,
        early_combat_rooms_on_map_rows_0_through_3: AgentObservedCountRangeV1 {
            min: min_early_pressure,
            max: max_early_pressure,
        },
        early_combat_room_count_has_no_inherent_preference_direction: true,
        shops: AgentObservedRoomAccessV1 {
            observed: observed_count_presence_v1(observed_path_count, min_shops, max_shops),
            earliest_possible_floor: first_shop_floor,
        },
        campfires: AgentObservedRoomAccessV1 {
            observed: observed_count_presence_v1(observed_path_count, min_campfires, max_campfires),
            earliest_possible_floor: first_campfire_floor,
        },
        first_elite: AgentFirstEliteRouteSemanticsV1 {
            presence: matching_path_presence_v1(
                observed_path_count,
                first_elite.paths_with_first_elite,
            ),
            observed_paths_with_first_elite: first_elite.paths_with_first_elite,
            before_counts_scope: "observed_paths_that_reach_a_first_elite",
            shops_before: observed_count_presence_v1(
                first_elite.paths_with_first_elite,
                first_elite.min_shops_before,
                first_elite.max_shops_before,
            ),
            campfires_before: observed_count_presence_v1(
                first_elite.paths_with_first_elite,
                first_elite.min_campfires_before,
                first_elite.max_campfires_before,
            ),
        },
    }
}

fn route_semantics_manifest_v1(
    raw_path_count_field: &'static str,
) -> AgentRouteSemanticsManifestV1 {
    AgentRouteSemanticsManifestV1 {
        raw_path_fields_represented: vec![
            raw_path_count_field,
            "min_early_pressure",
            "max_early_pressure",
            "first_shop_floor",
            "first_campfire_floor",
            "min_shops",
            "max_shops",
            "min_campfires",
            "max_campfires",
            "first_elite.paths_with_first_elite",
            "first_elite.forced",
            "first_elite.optional",
            "first_elite.min_shops_before",
            "first_elite.max_shops_before",
            "first_elite.min_campfires_before",
            "first_elite.max_campfires_before",
            "first_elite.can_bail_to_rest_before",
            "first_elite.can_bail_to_shop_before",
        ],
    }
}

fn route_policy_semantics_v1(path: &RoutePolicyPathEvidenceV1) -> AgentRouteSemanticsV1 {
    route_semantics_v1(RouteSemanticsInputV1 {
        observed_path_count: path.observed_path_count,
        min_early_pressure: path.min_early_pressure,
        max_early_pressure: path.max_early_pressure,
        min_shops: path.min_shops,
        max_shops: path.max_shops,
        first_shop_floor: path.first_shop_floor,
        min_campfires: path.min_campfires,
        max_campfires: path.max_campfires,
        first_campfire_floor: path.first_campfire_floor,
        first_elite: &path.first_elite,
    })
}

fn route_entry_semantics_v1(path: &RoutePathFamilySummary) -> AgentRouteSemanticsV1 {
    route_semantics_v1(RouteSemanticsInputV1 {
        observed_path_count: path.path_count,
        min_early_pressure: path.min_early_pressure,
        max_early_pressure: path.max_early_pressure,
        min_shops: path.min_shops,
        max_shops: path.max_shops,
        first_shop_floor: path.first_shop_floor,
        min_campfires: path.min_campfires,
        max_campfires: path.max_campfires,
        first_campfire_floor: path.first_campfire_floor,
        first_elite: &path.first_elite,
    })
}

fn route_policy_semantics_manifest_v1() -> AgentRouteSemanticsManifestV1 {
    route_semantics_manifest_v1("observed_path_count")
}

fn route_entry_semantics_manifest_v1() -> AgentRouteSemanticsManifestV1 {
    route_semantics_manifest_v1("path_count")
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCampfireOptionEvidenceV1 {
    pub semantic_actions: Vec<String>,
    #[serde(flatten)]
    pub mechanics: AgentCampfireMechanicsV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "action_kind", rename_all = "snake_case")]
pub enum AgentCampfireMechanicsV1 {
    Rest {
        hp_gain: i32,
    },
    Smith {
        before: CardRewardFactsV1,
        after: CardRewardFactsV1,
    },
    Dig,
    Lift,
    Toke {
        card: CardId,
        upgrades: u8,
    },
    Recall,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentPositionV1 {
    Decision {
        site: sts_oracle_learning::agent::task::TaskDecisionSiteV1,
        public_snapshot_id: String,
        observation: AgentPublicObservationV1,
        actions: Vec<AgentActionV1>,
    },
    Terminal {
        outcome: TaskEpisodeOutcomeV1,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProbeStepV1 {
    pub action: String,
    pub outcome: TaskEpisodeOutcomeV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentProbeEndpointV1 {
    Decision {
        site: sts_oracle_learning::agent::task::TaskDecisionSiteV1,
        public_snapshot_id: String,
        observation: AgentPublicObservationV1,
        actions: Vec<AgentActionV1>,
    },
    Outcome {
        outcome: TaskEpisodeOutcomeV1,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProbeLineV1 {
    pub view: String,
    pub steps: Vec<AgentProbeStepV1>,
    pub endpoint: AgentProbeEndpointV1,
    pub stop: AgentProbeStopV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentProbeStopV1 {
    Complete,
    Terminal {
        unapplied_actions: usize,
    },
    Unresolved {
        requested_step: usize,
        error: String,
        unapplied_actions: usize,
    },
}

/// Root-bound replay evidence for one or more explicitly supplied semantic
/// lines. Unlike combat research, this makes no quality or milestone claim;
/// it proves only each exact prefix and its public endpoint.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayExactPrefixArtifactV1 {
    pub schema: String,
    pub session_id: String,
    pub root_head_revision: u64,
    pub root_state_revision: u64,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub durable_head_unchanged: bool,
    pub lines: Vec<AgentProbeLineV1>,
    /// Private diagnostic payload for wholly combat-local lines. The public
    /// observation/response stays unchanged; offline consumers can replay the
    /// same resolved inputs without implementing another semantic parser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_replay: Option<RunPlayExactCombatReplayV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayExactCombatReplayV1 {
    pub root: sts_oracle_learning::sim::combat::CombatPosition,
    pub plans: Vec<RunPlayExactCombatLineV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayExactCombatLineV1 {
    pub inputs: Vec<sts_oracle_learning::state::core::ClientInput>,
    pub successor: sts_oracle_learning::sim::combat::CombatPosition,
}

impl RunPlayExactPrefixArtifactV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != EXACT_PREFIX_ARTIFACT_SCHEMA_V1 {
            return Err(format!(
                "unsupported exact prefix artifact schema {:?}; expected {EXACT_PREFIX_ARTIFACT_SCHEMA_V1}",
                self.schema
            ));
        }
        if self.session_id.trim().is_empty()
            || self.root_archive_node_id.trim().is_empty()
            || self.root_public_snapshot_id.trim().is_empty()
        {
            return Err("exact prefix artifact root identity is incomplete".to_string());
        }
        if !self.durable_head_unchanged || self.lines.is_empty() {
            return Err(
                "exact prefix artifact requires an unchanged head and at least one line"
                    .to_string(),
            );
        }
        for line in &self.lines {
            if line.steps.is_empty() || line.steps.iter().any(|step| step.action.trim().is_empty())
            {
                return Err("exact prefix artifact contains an empty action line".to_string());
            }
            if matches!(
                &line.endpoint,
                AgentProbeEndpointV1::Decision {
                    public_snapshot_id,
                    ..
                } if public_snapshot_id.trim().is_empty()
            ) {
                return Err("exact prefix artifact has an empty endpoint snapshot".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentActionV1 {
    /// Some typed candidate families require a later indexed/selection choice
    /// and therefore do not yet have one directly executable semantic action.
    pub semantic_action: Option<String>,
    pub role: AgentActionRoleV1,
    pub candidate: TaskPublicCandidateV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_selection_family: Option<RunSelectionFamilyV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_selection_target: Option<RunSelectionTargetV1>,
    /// Present for every combat candidate and derived from the same public
    /// decision/candidate ordinal. Strategic candidates have no combat effect.
    pub combat_effect: Option<PublicCombatActionEffectEnvelopeV1>,
}

fn project_agent_combat_effect_v1(
    decision: &sts_oracle_learning::agent::task::TaskPublicDecisionV1,
    candidate_ordinal: usize,
) -> Result<Option<PublicCombatActionEffectEnvelopeV1>, String> {
    if !matches!(decision.observation, TaskPublicObservationV1::Combat { .. }) {
        return Ok(None);
    }

    decision
        .combat_candidate_effect_v1(candidate_ordinal)
        .map(|effect| Some(effect.effect))
        .map_err(|error| {
            format!("cannot project combat candidate {candidate_ordinal} effect: {error}")
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentActionRoleV1 {
    Decision,
    FreeResource,
    OpensDecision,
}

fn agent_action_role_v1(candidate: &TaskPublicCandidateV1) -> AgentActionRoleV1 {
    match candidate {
        TaskPublicCandidateV1::Strategic {
            action:
                sts_oracle_learning::agent::task::TaskStrategicActionV1::ClaimReward {
                    reward:
                        PlannerRewardDescriptor::Gold { .. }
                        | PlannerRewardDescriptor::StolenGold { .. },
                    ..
                },
        } => AgentActionRoleV1::FreeResource,
        TaskPublicCandidateV1::Strategic {
            action:
                sts_oracle_learning::agent::task::TaskStrategicActionV1::OpenCardReward { .. }
                | sts_oracle_learning::agent::task::TaskStrategicActionV1::OpenPendingRewards
                | sts_oracle_learning::agent::task::TaskStrategicActionV1::OpenChest
                | sts_oracle_learning::agent::task::TaskStrategicActionV1::BeginRunCardSelection {
                    ..
                }
                | sts_oracle_learning::agent::task::TaskStrategicActionV1::ClaimReward {
                    reward:
                        PlannerRewardDescriptor::Relic {
                            relic:
                                RelicId::BottledFlame
                                | RelicId::BottledLightning
                                | RelicId::BottledTornado,
                        },
                    ..
                }
                | sts_oracle_learning::agent::task::TaskStrategicActionV1::BuyRelic {
                    relic:
                        RelicId::BottledFlame
                        | RelicId::BottledLightning
                        | RelicId::BottledTornado
                        | RelicId::Cauldron
                        | RelicId::DollysMirror
                        | RelicId::Orrery,
                    ..
                }
                | sts_oracle_learning::agent::task::TaskStrategicActionV1::TakeBossRelic {
                    relic: RelicId::Astrolabe | RelicId::CallingBell | RelicId::EmptyCage,
                    ..
                },
        } => AgentActionRoleV1::OpensDecision,
        TaskPublicCandidateV1::Strategic {
            action:
                sts_oracle_learning::agent::task::TaskStrategicActionV1::ChooseEventOption {
                    public_semantics,
                    ..
                },
        } if public_semantics
            .structured_semantics
            .as_ref()
            .is_some_and(|semantics| {
                matches!(
                    semantics.transition,
                    PlannerEventTransitionV1::OpenSelection { .. }
                        | PlannerEventTransitionV1::OpenReward
                )
            }) =>
        {
            AgentActionRoleV1::OpensDecision
        }
        _ => AgentActionRoleV1::Decision,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum AgentPublicObservationV1 {
    Strategic {
        observation: AgentStrategicObservationV1,
    },
    Combat {
        run_context: AgentCombatRunContextV1,
        observation: PublicCombatStateV1,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentStrategicObservationV1 {
    pub run_goal: PlannerRunGoal,
    pub run: PlannerRunScalars,
    /// Permanent EnergyManager.energyMaster implied by the public relic set.
    /// Encounter-conditional gains such as Slaver's Collar are not included.
    pub base_energy_per_turn: u8,
    pub cards: Vec<AgentStrategicCardGroupV1>,
    pub relics: Vec<PlannerRelicObservation>,
    pub potions: Vec<TaskStrategicPotionSlotV1>,
    pub public_map: AgentPublicMapV1,
    pub context: PlannerDecisionContext,
    pub public_history: PlannerPublicHistory,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentStrategicCardGroupV1 {
    pub card: CardId,
    pub upgrades: u8,
    pub misc_value: i32,
    pub base_damage_override: Option<i32>,
    pub base_block_override: Option<i32>,
    pub cost_modifier: i8,
    pub count: usize,
}

pub(crate) fn public_base_energy_per_turn_v1(relics: &[PlannerRelicObservation]) -> u8 {
    relics.iter().fold(3_u8, |energy, relic| {
        energy.saturating_add(energy_master_delta(relic.relic))
    })
}

impl AgentStrategicObservationV1 {
    fn from_public_v1(observation: &TaskStrategicObservationV1) -> Self {
        let mut cards = Vec::<AgentStrategicCardGroupV1>::new();
        for card in &observation.cards {
            if let Some(group) = cards.iter_mut().find(|group| {
                group.card == card.card
                    && group.upgrades == card.upgrades
                    && group.misc_value == card.misc_value
                    && group.base_damage_override == card.base_damage_override
                    && group.base_block_override == card.base_block_override
                    && group.cost_modifier == card.cost_modifier
            }) {
                group.count += 1;
            } else {
                cards.push(AgentStrategicCardGroupV1 {
                    card: card.card,
                    upgrades: card.upgrades,
                    misc_value: card.misc_value,
                    base_damage_override: card.base_damage_override,
                    base_block_override: card.base_block_override,
                    cost_modifier: card.cost_modifier,
                    count: 1,
                });
            }
        }
        Self {
            run_goal: observation.run_goal,
            run: observation.run.clone(),
            base_energy_per_turn: public_base_energy_per_turn_v1(&observation.relics),
            cards,
            relics: observation.relics.clone(),
            potions: observation.potions.clone(),
            public_map: AgentPublicMapV1::from_public_v1(&observation.public_map),
            context: observation.context.clone(),
            public_history: observation.public_history.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPublicMapV1 {
    pub current: [i32; 2],
    pub boss: Option<EncounterId>,
    /// Tuple layout for each entry in `nodes`. It is stated once rather than
    /// repeating five long JSON field names for every public map node.
    pub node_tuple_fields: [&'static str; 5],
    pub nodes: Vec<AgentPublicMapNodeV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentPublicMapNodeV1(
    pub i32,
    pub i32,
    pub PlannerMapRoom,
    pub bool,
    pub Vec<[i32; 2]>,
);

impl AgentPublicMapV1 {
    fn from_public_v1(map: &PlannerPublicMap) -> Self {
        Self {
            current: [map.current_x, map.current_y],
            boss: map.boss,
            node_tuple_fields: ["x", "y", "room", "emerald_key", "destinations"],
            nodes: map
                .nodes
                .iter()
                .map(|node| {
                    AgentPublicMapNodeV1(
                        node.x,
                        node.y,
                        node.room,
                        node.has_emerald_key,
                        node.edges
                            .iter()
                            .map(|edge| [edge.destination_x, edge.destination_y])
                            .collect(),
                    )
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentCombatRunContextV1 {
    Available {
        run_goal: PlannerRunGoal,
        ascension_level: u8,
        combat_victory_guarantees_full_heal: bool,
        act: u8,
        floor: i32,
        keys: [bool; 3],
        public_map: AgentPublicMapV1,
        encounter_id: Option<EncounterId>,
    },
    Unavailable {
        reason: PublicCombatRunContextGapV1,
    },
}

impl AgentCombatRunContextV1 {
    fn from_public_v1(context: &PublicCombatRunContextV1) -> Self {
        match context {
            PublicCombatRunContextV1::Available {
                run_goal,
                ascension_level,
                combat_victory_guarantees_full_heal,
                act,
                floor,
                keys,
                public_map,
                encounter_id,
            } => Self::Available {
                run_goal: *run_goal,
                ascension_level: *ascension_level,
                combat_victory_guarantees_full_heal: *combat_victory_guarantees_full_heal,
                act: *act,
                floor: *floor,
                keys: *keys,
                public_map: AgentPublicMapV1::from_public_v1(public_map),
                encounter_id: *encounter_id,
            },
            PublicCombatRunContextV1::Unavailable { reason } => {
                Self::Unavailable { reason: *reason }
            }
        }
    }
}

fn project_public_observation_v1(
    observation: &TaskPublicObservationV1,
) -> AgentPublicObservationV1 {
    match observation {
        TaskPublicObservationV1::Strategic { observation } => AgentPublicObservationV1::Strategic {
            observation: AgentStrategicObservationV1::from_public_v1(observation),
        },
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => AgentPublicObservationV1::Combat {
            run_context: AgentCombatRunContextV1::from_public_v1(run_context),
            observation: observation.clone(),
        },
    }
}

pub(crate) fn project_agent_decision_v1(
    decision: &sts_oracle_learning::agent::task::TaskPublicDecisionV1,
) -> Result<AgentProbeEndpointV1, String> {
    let actions = decision
        .candidates
        .iter()
        .enumerate()
        .map(|(ordinal, candidate)| {
            let run_selection_family = project_run_selection_family_v1(decision, candidate)?;
            let semantic_action = if run_selection_family.is_some()
                || matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatSelectionFamily { .. }
                ) {
                None
            } else {
                Some(semantic_action_for_choice_v1(
                    decision,
                    &sts_oracle_learning::agent::task::TaskPublicChoiceV1::one(ordinal),
                )?)
            };
            Ok(AgentActionV1 {
                semantic_action,
                role: agent_action_role_v1(candidate),
                candidate: candidate.clone(),
                run_selection_family,
                run_selection_target: project_run_selection_target_v1(decision, candidate)?,
                combat_effect: project_agent_combat_effect_v1(decision, ordinal)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(AgentProbeEndpointV1::Decision {
        site: decision.site,
        public_snapshot_id: decision.information.snapshot_id.clone(),
        observation: project_public_observation_v1(&decision.observation),
        actions,
    })
}

fn render_private_automatic_turn_changes_v1(
    line: &ExactRunPlayProbeLineV1,
    steps: &[AgentProbeStepV1],
) -> String {
    let changes = line
        .steps
        .iter()
        .zip(steps)
        .enumerate()
        .filter_map(|(index, (step, projected))| {
            if projected.action == "end" {
                return None;
            }
            let TaskPublicObservationV1::Combat {
                observation: before,
                ..
            } = &step.source_decision.observation
            else {
                return None;
            };
            let TaskPublicObservationV1::Combat {
                observation: after, ..
            } = &step.next_decision.as_ref()?.observation
            else {
                return None;
            };
            (after.turn.turn_count > before.turn.turn_count).then(|| {
                format!(
                    "action {} {}: T{} -> T{}",
                    index + 1,
                    projected.action,
                    before.turn.turn_count.saturating_add(1),
                    after.turn.turn_count.saturating_add(1),
                )
            })
        })
        .collect::<Vec<_>>();
    if changes.is_empty() {
        String::new()
    } else {
        format!("Automatic turn changes: {}\n", changes.join("; "))
    }
}

fn project_agent_probe_line_v1(
    line: &ExactRunPlayProbeLineV1,
    locale: Locale,
) -> Result<AgentProbeLineV1, String> {
    let steps = line
        .steps
        .iter()
        .map(|step| {
            Ok(AgentProbeStepV1 {
                action: semantic_action_for_choice_v1(&step.source_decision, &step.choice)?,
                outcome: step.outcome.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let last = line
        .steps
        .last()
        .ok_or_else(|| "agent probe produced an empty line".to_string())?;
    let endpoint = match &last.next_decision {
        Some(decision) => project_agent_decision_v1(decision)?,
        None => AgentProbeEndpointV1::Outcome {
            outcome: last.outcome.clone(),
        },
    };
    let view = match &last.next_decision {
        Some(decision) => {
            let reading = crate::decision::read_preview_decision_v1(decision, None, locale)?;
            match reading.control {
                crate::decision::DecisionControlV1::Strategic => {
                    crate::compact::render_reading_status_v1(decision, &reading, true, None)?
                }
                _ => reading.view,
            }
        }
        None => format!(
            "Terminal | {}\n",
            crate::compact::render_terminal_result_v1(last)
        ),
    };
    let view = format!(
        "{}{view}",
        render_private_automatic_turn_changes_v1(line, &steps)
    );
    Ok(AgentProbeLineV1 {
        view,
        steps,
        endpoint,
        stop: if last.next_decision.is_none() {
            AgentProbeStopV1::Terminal {
                unapplied_actions: 0,
            }
        } else {
            AgentProbeStopV1::Complete
        },
    })
}

fn project_agent_inspection_v1(
    inspection: &ExactRunPlayInspectionV1,
    locale: Locale,
) -> Result<AgentProbeLineV1, String> {
    match &inspection.stop {
        ExactRunPlayInspectionStopV1::Complete => {
            project_agent_probe_line_v1(&inspection.line, locale)
        }
        ExactRunPlayInspectionStopV1::Terminal { unapplied_steps } => {
            let mut result = project_agent_probe_line_v1(&inspection.line, locale)?;
            result.stop = AgentProbeStopV1::Terminal {
                unapplied_actions: *unapplied_steps,
            };
            if *unapplied_steps > 0 {
                result.view = format!("Preview reached a terminal after {} actions; {} following actions were not simulated.\n{}",
                    result.steps.len(), unapplied_steps, result.view);
            }
            Ok(result)
        }
        ExactRunPlayInspectionStopV1::Unresolved {
            requested_step,
            error,
            decision,
        } => {
            let reading = crate::decision::read_preview_decision_v1(decision, None, locale)?;
            let view = match reading.control {
                crate::decision::DecisionControlV1::Strategic => {
                    crate::compact::render_reading_status_v1(decision, &reading, true, None)?
                }
                _ => reading.view,
            };
            let steps = inspection
                .line
                .steps
                .iter()
                .map(|step| {
                    Ok(AgentProbeStepV1 {
                        action: semantic_action_for_choice_v1(&step.source_decision, &step.choice)?,
                        outcome: step.outcome.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let count = steps.len();
            let turn_changes = render_private_automatic_turn_changes_v1(&inspection.line, &steps);
            Ok(AgentProbeLineV1 {
                view: format!("Preview stopped before action {requested_step}: {error}\n{turn_changes}Private position after {count} simulated actions:\n{view}"),
                steps,
                endpoint: project_agent_decision_v1(decision)?,
                stop: AgentProbeStopV1::Unresolved { requested_step: *requested_step, error: error.clone(),
                    unapplied_actions: inspection.requested_steps - count },
            })
        }
    }
}

#[cfg(test)]
mod private_preview_command_tests {
    use super::*;
    use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
    use sts_oracle_learning::agent::task::TaskPublicChoiceV1;
    use sts_oracle_learning::eval::run_control::{RunControlConfig, RunControlSession};
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventState},
    };

    #[test]
    fn event_preview_displays_the_successors_exact_command_without_advancing() {
        let dir = TestDirectory::new();
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.current_hp = 42;
        session.run_state.event_state = Some(EventState::new(EventId::CursedTome));
        let mut game = controller_with_session(&dir.0, "private-event-commands", &session);
        let before = game.head().clone();
        let actions =
            crate::selector::parse_action_line_v1("event:CursedTome:0", "preview test").unwrap();
        let comparison = game.compare_action_lines_v1(&[actions.clone()]).unwrap();
        let projected = project_agent_probe_line_v1(&comparison.lines[0], Locale::En).unwrap();
        assert!(
            projected
                .view
                .contains("event:CursedTome:0  |  [Continue] Take 1 damage."),
            "{}",
            projected.view
        );
        assert!(!projected.view.contains("0: [Continue]"));
        assert_eq!(*game.head(), before);
        game.anchor_action_line_v1(&actions).unwrap();
        let actual = game
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let AgentProbeEndpointV1::Decision { observation, .. } = projected.endpoint else {
            panic!("decision endpoint");
        };
        assert_eq!(
            observation,
            project_public_observation_v1(&actual.observation)
        );
        assert_eq!(
            semantic_action_for_choice_v1(&actual, &TaskPublicChoiceV1::one(0)).unwrap(),
            "event:CursedTome:0"
        );
    }

    #[test]
    fn unresolved_preview_keeps_the_private_boundary_but_cannot_certify_or_publish() {
        let dir = TestDirectory::new();
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.current_hp = 42;
        session.run_state.event_state = Some(EventState::new(EventId::CursedTome));
        let mut game = controller_with_session(&dir.0, "partial-event", &session);
        let before = game.head().clone();
        let prefix = crate::selector::parse_action_line_v1("event:CursedTome:0", "test").unwrap();
        let expected = game.compare_action_lines_v1(&[prefix]).unwrap();
        let expected = project_agent_probe_line_v1(&expected.lines[0], Locale::En).unwrap();
        let line =
            crate::selector::parse_action_line_v1("event:CursedTome:0/play:Strike@0", "test")
                .unwrap();
        let inspections = game.inspect_action_lines_v1(&[line.clone()]).unwrap();
        let projected = project_agent_inspection_v1(&inspections[0], Locale::En).unwrap();
        assert_eq!(projected.endpoint, expected.endpoint);
        assert_eq!(projected.steps.len(), 1);
        assert!(matches!(
            projected.stop,
            AgentProbeStopV1::Unresolved {
                requested_step: 2,
                unapplied_actions: 1,
                ..
            }
        ));
        assert!(projected
            .view
            .contains("Private position after 1 simulated actions"));
        assert!(projected
            .view
            .contains("event:CursedTome:0  |  [Continue] Take 1 damage."));
        assert!(capture_exact_prefix_artifact_v1(&mut game, &[line.clone()]).is_err());
        assert!(game.anchor_action_line_v1(&line).is_err());
        assert_eq!(*game.head(), before);
    }

    #[test]
    fn unresolved_first_action_still_reports_the_current_boundary() {
        let dir = TestDirectory::new();
        let mut game =
            controller_with_session(&dir.0, "partial-zero", &fixture("combat_multiturn"));
        let before = game.head().clone();
        let actual = before
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let line = crate::selector::parse_action_line_v1("play:Corruption@0", "test").unwrap();
        let inspections = game.inspect_action_lines_v1(&[line]).unwrap();
        let projected = project_agent_inspection_v1(&inspections[0], Locale::En).unwrap();
        assert!(projected.steps.is_empty());
        assert!(projected
            .view
            .contains("Private position after 0 simulated actions"));
        assert!(projected.view.contains("Hand:"));
        assert_eq!(
            projected.endpoint,
            project_agent_decision_v1(&actual).unwrap()
        );
        assert_eq!(*game.head(), before);
    }

    #[test]
    fn preview_terminal_keeps_defeat_and_hp_even_with_unused_tail() {
        let dir = TestDirectory::new();
        let mut game =
            controller_with_session(&dir.0, "partial-terminal", &fixture("combat_defeat"));
        let before = game.head().clone();
        let line = crate::selector::parse_action_line_v1("end/end", "test").unwrap();
        let inspections = game.inspect_action_lines_v1(&[line.clone()]).unwrap();
        let projected = project_agent_inspection_v1(&inspections[0], Locale::En).unwrap();
        assert!(matches!(
            projected.stop,
            AgentProbeStopV1::Terminal {
                unapplied_actions: 1
            }
        ));
        assert!(matches!(
            projected.endpoint,
            AgentProbeEndpointV1::Outcome { .. }
        ));
        assert!(
            projected.view.contains("Defeat | HP 1/75 → 0/75"),
            "{}",
            projected.view
        );
        assert_eq!(projected.steps.len(), 1);
        assert!(capture_exact_prefix_artifact_v1(&mut game, &[line.clone()]).is_err());
        assert!(game.anchor_action_line_v1(&line).is_err());
        assert_eq!(*game.head(), before);
    }

    #[test]
    fn preview_combat_victory_reports_rewards_before_unresolved_combat_tail() {
        let dir = TestDirectory::new();
        let mut game = controller_with_session(&dir.0, "partial-win", &fixture("combat_finish"));
        let before = game.head().clone();
        let line = crate::selector::parse_action_line_v1("play:Strike@0/end", "test").unwrap();
        let inspections = game.inspect_action_lines_v1(&[line]).unwrap();
        let projected = project_agent_inspection_v1(&inspections[0], Locale::En).unwrap();
        assert!(matches!(
            projected.stop,
            AgentProbeStopV1::Unresolved {
                requested_step: 2,
                ..
            }
        ));
        assert!(projected.view.contains("Rewards"), "{}", projected.view);
        assert_eq!(projected.steps.len(), 1);
        assert_eq!(*game.head(), before);
    }

    #[test]
    fn unresolved_later_turn_reports_its_drawn_hand_instead_of_the_live_root() {
        let dir = TestDirectory::new();
        let mut game =
            controller_with_session(&dir.0, "partial-next-turn", &fixture("combat_multiturn"));
        let before = game.head().clone();
        let prefix = crate::selector::parse_action_line_v1("end", "test").unwrap();
        let expected = game.compare_action_lines_v1(&[prefix]).unwrap();
        let expected = project_agent_probe_line_v1(&expected.lines[0], Locale::En).unwrap();
        let line = crate::selector::parse_action_line_v1("end/play:Corruption", "test").unwrap();
        let inspections = game.inspect_action_lines_v1(&[line]).unwrap();
        let projected = project_agent_inspection_v1(&inspections[0], Locale::En).unwrap();
        assert_eq!(projected.endpoint, expected.endpoint);
        assert!(projected.view.contains("Turn 2 |"), "{}", projected.view);
        assert!(projected.view.contains("Hand:"));
        assert!(!projected.view.contains("Automatic turn changes:"));
        assert_eq!(*game.head(), before);
    }

    #[test]
    fn preview_identifies_the_card_that_forced_a_new_turn_before_the_failed_suffix() {
        use sts_oracle_learning::runtime::combat::{Power, PowerPayload};
        let dir = TestDirectory::new();
        let mut session = fixture("combat_multiturn");
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        let monster = combat.entities.monsters[0].id;
        combat.entities.power_db.insert(
            monster,
            vec![Power {
                power_type: PowerId::TimeWarp,
                instance_id: None,
                amount: 11,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut game = controller_with_session(&dir.0, "private-auto-turn", &session);
        let before = game.head().clone();
        let line =
            crate::selector::parse_action_line_v1("play:Defend#0/play:Corruption", "test").unwrap();
        let inspections = game
            .inspect_action_lines_v1(&[vec![line[0].clone()], line])
            .unwrap();
        let complete = project_agent_inspection_v1(&inspections[0], Locale::En).unwrap();
        assert!(matches!(complete.stop, AgentProbeStopV1::Complete));
        assert!(complete
            .view
            .contains("Automatic turn changes: action 1 play:Defend+0#0: T1 -> T2"));
        let projected = project_agent_inspection_v1(&inspections[1], Locale::En).unwrap();
        assert!(
            projected
                .view
                .contains("Automatic turn changes: action 1 play:Defend+0#0: T1 -> T2"),
            "{}",
            projected.view
        );
        assert!(projected.view.contains("Turn 2 |"));
        assert!(matches!(
            projected.stop,
            AgentProbeStopV1::Unresolved {
                requested_step: 2,
                ..
            }
        ));
        assert_eq!(*game.head(), before);
    }
}

pub fn capture_exact_prefix_artifact_v1(
    controller: &mut RunPlayControllerV1,
    lines: &[Vec<RunPlayActionSelectorV1>],
) -> Result<(RunPlayComparisonV1, RunPlayExactPrefixArtifactV1), String> {
    let root_head_revision = controller.head().head_revision;
    let root_state_revision = controller.head().state_revision;
    let session_id = controller.head().session_id.clone();
    let root_archive_node_id = controller.head().position.archive_node_id().to_string();
    let root_public_snapshot_id = controller
        .head()
        .current_decision
        .as_ref()
        .ok_or_else(|| "exact prefix artifact requires a public decision".to_string())?
        .public_decision_v1()?
        .information
        .snapshot_id;
    let comparison = controller.compare_action_lines_v1(lines)?;
    let projected_lines = comparison
        .lines
        .iter()
        .map(|line| project_agent_probe_line_v1(line, Locale::En))
        .collect::<Result<Vec<_>, String>>()?;
    let combat_replay = if comparison.lines.iter().all(|line| {
        !line.steps.is_empty()
            && line.steps.iter().all(|step| {
                matches!(
                    step.source_decision.observation,
                    TaskPublicObservationV1::Combat { .. }
                )
            })
    }) {
        use sts_oracle_learning::sim::combat::{
            CombatStepLimits, CombatStepper, EngineCombatStepper,
        };
        let root = controller.current_combat_position_for_lab_v1()?;
        let plans = comparison
            .lines
            .iter()
            .map(|line| {
                let choices = line
                    .steps
                    .iter()
                    .map(|step| step.choice.clone())
                    .collect::<Vec<_>>();
                let inputs = crate::proposal_audit::resolve_target_inputs_v1(&root, &choices, 512)?;
                let mut successor = root.clone();
                for input in &inputs {
                    let step = EngineCombatStepper.apply_to_stable(
                        &successor,
                        input.clone(),
                        CombatStepLimits {
                            max_engine_steps: 512,
                            deadline: None,
                        },
                    );
                    if step.truncated || step.timed_out {
                        return Err("exact prefix combat replay did not settle".to_string());
                    }
                    successor = step.position;
                }
                Ok(RunPlayExactCombatLineV1 { inputs, successor })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Some(RunPlayExactCombatReplayV1 { root, plans })
    } else {
        None
    };
    let head = controller.head();
    let current_public_snapshot_id = head
        .current_decision
        .as_ref()
        .ok_or_else(|| "exact prefix artifact lost its public decision".to_string())?
        .public_decision_v1()?
        .information
        .snapshot_id;
    if head.head_revision != root_head_revision
        || head.state_revision != root_state_revision
        || head.session_id != session_id
        || head.position.archive_node_id() != root_archive_node_id
        || current_public_snapshot_id != root_public_snapshot_id
        || comparison.root_archive_node_id != root_archive_node_id
    {
        return Err("exact prefix artifact changed its durable controller root".to_string());
    }
    let artifact = RunPlayExactPrefixArtifactV1 {
        schema: EXACT_PREFIX_ARTIFACT_SCHEMA_V1.to_string(),
        session_id,
        root_head_revision,
        root_state_revision,
        root_archive_node_id,
        root_public_snapshot_id,
        durable_head_unchanged: true,
        lines: projected_lines,
        combat_replay,
    };
    artifact.validate()?;
    Ok((comparison, artifact))
}

pub fn project_agent_observation_v1(
    head: &RunPlayHeadV1,
    capabilities: AgentCapabilitiesV1,
    decision_evidence: Option<AgentDecisionEvidenceV1>,
    locale: Locale,
) -> Result<AgentObservationV1, String> {
    let position = match &head.position {
        RunPlayPositionV1::Terminal { outcome, .. } => AgentPositionV1::Terminal {
            outcome: outcome.clone(),
        },
        RunPlayPositionV1::Decision { .. } => {
            let inventory = head
                .current_decision
                .as_ref()
                .ok_or_else(|| "run-play agent observation has no cached decision".to_string())?;
            let decision = inventory.public_decision_v1()?;
            let actions = inventory
                .candidates
                .iter()
                .map(|candidate| {
                    let run_selection_family =
                        project_run_selection_family_v1(&decision, &candidate.candidate)?;
                    let semantic_action = candidate
                        .direct_choice
                        .as_ref()
                        .map(|choice| semantic_action_for_choice_v1(&decision, choice))
                        .transpose()?;
                    Ok(AgentActionV1 {
                        semantic_action,
                        role: agent_action_role_v1(&candidate.candidate),
                        candidate: candidate.candidate.clone(),
                        run_selection_family,
                        run_selection_target: project_run_selection_target_v1(
                            &decision,
                            &candidate.candidate,
                        )?,
                        combat_effect: project_agent_combat_effect_v1(
                            &decision,
                            candidate.candidate_ordinal,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            AgentPositionV1::Decision {
                site: inventory.site,
                public_snapshot_id: inventory.public_snapshot_id.clone(),
                observation: project_public_observation_v1(&inventory.observation),
                actions,
            }
        }
    };
    let deck_evidence = match &position {
        AgentPositionV1::Decision {
            public_snapshot_id,
            observation: AgentPublicObservationV1::Strategic { observation },
            ..
        } => Some(project_agent_deck_evidence_v1(
            observation,
            public_snapshot_id,
        )),
        _ => None,
    };
    let preparation_reading = head
        .current_decision
        .as_ref()
        .map(|inventory| {
            crate::decision::preparation::reading(&inventory.public_decision_v1()?, locale)
        })
        .transpose()?
        .flatten();
    Ok(AgentObservationV1 {
        schema: AGENT_OBSERVATION_SCHEMA_V3,
        session_id: head.session_id.clone(),
        head_revision: head.head_revision,
        state_revision: head.state_revision,
        capabilities,
        decision_evidence,
        deck_evidence,
        preparation_reading,
        position,
    })
}

fn project_agent_campfire_evidence_v1(
    head: &RunPlayHeadV1,
    audit: &ExactCampfirePolicyAuditV1,
) -> Result<AgentDecisionEvidenceV1, String> {
    let inventory = head
        .current_decision
        .as_ref()
        .ok_or_else(|| "agent campfire evidence has no cached decision".to_string())?;
    let decision = inventory.public_decision_v1()?;
    let mut candidates: Vec<AgentCampfireOptionEvidenceV1> = Vec::new();
    for (surface_index, candidate) in inventory.candidates.iter().enumerate() {
        let mechanics = match &candidate.candidate {
            TaskPublicCandidateV1::Strategic { action } => {
                project_agent_campfire_mechanics_v1(action, audit, surface_index)?
            }
            other => {
                return Err(format!(
                    "agent campfire evidence received non-strategic candidate {other:?}"
                ));
            }
        };
        let Some(mechanics) = mechanics else {
            continue;
        };
        let choice = candidate.direct_choice.as_ref().ok_or_else(|| {
            format!("agent campfire candidate {surface_index} has no direct public choice")
        })?;
        let semantic_action = semantic_action_for_choice_v1(&decision, choice)?;
        if let Some(existing) = candidates
            .iter_mut()
            .find(|existing| existing.mechanics == mechanics)
        {
            existing.semantic_actions.push(semantic_action);
        } else {
            candidates.push(AgentCampfireOptionEvidenceV1 {
                semantic_actions: vec![semantic_action],
                mechanics,
            });
        }
    }
    Ok(AgentDecisionEvidenceV1::CampfireOptions { candidates })
}

fn project_agent_campfire_mechanics_v1(
    action: &TaskStrategicActionV1,
    audit: &ExactCampfirePolicyAuditV1,
    surface_index: usize,
) -> Result<Option<AgentCampfireMechanicsV1>, String> {
    let mechanics = match action {
        TaskStrategicActionV1::Rest => {
            let hp_gain = audit
                .candidates
                .iter()
                .find(|entry| entry.surface_index == surface_index)
                .ok_or_else(|| {
                    format!("agent campfire rest candidate {surface_index} has no audit evidence")
                })?
                .hp_gain;
            AgentCampfireMechanicsV1::Rest { hp_gain }
        }
        TaskStrategicActionV1::Smith { card, upgrades, .. } => {
            let before = card_reward_facts_v1(&RewardCard::new(*card, *upgrades));
            let after = card_reward_facts_v1(&RewardCard::new(*card, upgrades.saturating_add(1)));
            AgentCampfireMechanicsV1::Smith { before, after }
        }
        TaskStrategicActionV1::Dig => AgentCampfireMechanicsV1::Dig,
        TaskStrategicActionV1::Lift => AgentCampfireMechanicsV1::Lift,
        TaskStrategicActionV1::Toke { card, upgrades, .. } => AgentCampfireMechanicsV1::Toke {
            card: *card,
            upgrades: *upgrades,
        },
        TaskStrategicActionV1::Recall => AgentCampfireMechanicsV1::Recall,
        TaskStrategicActionV1::UseRunPotion { .. }
        | TaskStrategicActionV1::DiscardRunPotion { .. } => return Ok(None),
        other => {
            return Err(format!(
                "agent campfire evidence received non-campfire action {other:?}"
            ));
        }
    };
    Ok(Some(mechanics))
}

pub(crate) fn acquired_card_mechanics_for_action_v1(
    action: &TaskStrategicActionV1,
) -> Vec<CardAcquisitionMechanicsFactsV1> {
    let cards = match action {
        TaskStrategicActionV1::TakeCard { card, upgrades, .. }
        | TaskStrategicActionV1::BuyCard { card, upgrades, .. } => {
            vec![RewardCard::new(*card, *upgrades)]
        }
        TaskStrategicActionV1::ChooseEventOption {
            public_semantics, ..
        } => public_semantics
            .specific_cards_gained
            .iter()
            .map(|card| RewardCard::new(card.card, card.upgrades))
            .collect(),
        _ => Vec::new(),
    };
    cards
        .iter()
        .map(card_acquisition_mechanics_facts_v1)
        .collect()
}

fn project_agent_route_entries_v1(
    controller: &mut RunPlayControllerV1,
) -> Result<Vec<AgentRouteEntryPathEvidenceV1>, String> {
    let entries = controller
        .route_entry_paths_v1(AGENT_ROUTE_ENTRY_PATH_BUDGET_V1)?
        .into_iter()
        .map(|entry| {
            let route_semantics = route_entry_semantics_v1(&entry.path);
            AgentRouteEntryPathEvidenceV1 {
                x: entry.target.x,
                y: entry.target.y,
                room: entry.target.room_type.map(PlannerMapRoom::from),
                has_emerald_key: entry.target.has_emerald_key,
                uses_wing_boots: entry.target.uses_wing_boots,
                coverage: entry.coverage,
                route_semantics,
                path: entry.path,
            }
        })
        .collect::<Vec<_>>();
    Ok(entries)
}

fn project_agent_card_acquisition_evidence_v1(
    head: &RunPlayHeadV1,
    route_entries: Vec<AgentRouteEntryPathEvidenceV1>,
) -> Result<Option<AgentDecisionEvidenceV1>, String> {
    let Some(inventory) = head.current_decision.as_ref() else {
        return Ok(None);
    };
    let decision = inventory.public_decision_v1()?;
    let mut candidates = Vec::new();
    for (surface_index, candidate) in inventory.candidates.iter().enumerate() {
        let TaskPublicCandidateV1::Strategic { action } = &candidate.candidate else {
            continue;
        };
        let cards = acquired_card_mechanics_for_action_v1(action);
        if cards.is_empty() {
            continue;
        }
        let choice = candidate.direct_choice.as_ref().ok_or_else(|| {
            format!("agent acquisition candidate {surface_index} has no direct public choice")
        })?;
        candidates.push(AgentCardAcquisitionEvidenceV1 {
            semantic_action: semantic_action_for_choice_v1(&decision, choice)?,
            cards,
        });
    }
    if candidates.is_empty() {
        return Ok(None);
    }
    Ok(Some(AgentDecisionEvidenceV1::CardAcquisitionOptions {
        candidates,
        resource_options: Vec::new(),
        route_semantics_manifest: route_entry_semantics_manifest_v1(),
        route_entries,
    }))
}

fn project_agent_card_reward_resources_v1(
    controller: &mut RunPlayControllerV1,
) -> Result<Vec<AgentCardRewardResourceEvidenceV1>, String> {
    let Some(inventory) = controller.head().current_decision.as_ref() else {
        return Ok(Vec::new());
    };
    let decision = inventory.public_decision_v1()?;
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Ok(Vec::new());
    };
    let before = observation.run.clone();
    let commands = inventory
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::SingingBowl { .. }
                }
            )
        })
        .map(|candidate| {
            let choice = candidate.direct_choice.as_ref().ok_or_else(|| {
                "agent Singing Bowl candidate has no direct public choice".to_string()
            })?;
            semantic_action_for_choice_v1(&decision, choice)
        })
        .collect::<Result<Vec<_>, String>>()?;
    commands
        .into_iter()
        .map(|semantic_action| {
            let selector = parse_action_selector_v1(&semantic_action, "agent reward resources")?;
            let preview = controller.compare_action_lines_v1(&[vec![selector]])?;
            let next = preview
                .lines
                .first()
                .and_then(|line| line.steps.last())
                .and_then(|step| step.next_decision.as_ref())
                .ok_or_else(|| {
                    "Singing Bowl preview needs its immediate public successor".to_string()
                })?;
            let TaskPublicObservationV1::Strategic { observation } = &next.observation else {
                return Err("Singing Bowl preview needs a strategic successor".to_string());
            };
            Ok(AgentCardRewardResourceEvidenceV1 {
                semantic_action,
                hp_gain: observation.run.current_hp - before.current_hp,
                max_hp_gain: observation.run.max_hp - before.max_hp,
                native_rule: sts_oracle_learning::content::relics::singing_bowl::MECHANICS,
            })
        })
        .collect()
}

fn project_agent_hp_loss_sources_v1(
    cards: &[AgentStrategicCardGroupV1],
) -> Vec<AgentHpLossSourceEvidenceV1> {
    cards
        .iter()
        .filter_map(|card| {
            let facts = card_reward_facts_v1(&RewardCard::new(card.card, card.upgrades));
            if card.count == 0
                || (facts.self_hp_loss_on_play.is_none()
                    && facts.hp_loss_each_turn <= 0
                    && facts.end_of_turn_hp_loss_and_damage.is_none())
            {
                return None;
            }
            Some(AgentHpLossSourceEvidenceV1 {
                card: card.clone(),
                on_play: facts.self_hp_loss_on_play,
                hp_loss_each_turn_after_deployment: facts.hp_loss_each_turn,
                end_of_turn_after_deployment: facts.end_of_turn_hp_loss_and_damage,
            })
        })
        .collect()
}

fn project_agent_boss_relic_evidence_v1(
    head: &RunPlayHeadV1,
) -> Result<Option<AgentDecisionEvidenceV1>, String> {
    let Some(inventory) = head.current_decision.as_ref() else {
        return Ok(None);
    };
    let decision = inventory.public_decision_v1()?;
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Ok(None);
    };
    let mut candidates = Vec::new();
    for candidate in &inventory.candidates {
        let TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::TakeBossRelic { relic, .. },
        } = &candidate.candidate
        else {
            continue;
        };
        let choice = candidate
            .direct_choice
            .as_ref()
            .ok_or_else(|| "agent boss relic candidate has no direct public choice".to_string())?;
        candidates.push(AgentRelicAcquisitionEvidenceV1 {
            semantic_action: semantic_action_for_choice_v1(&decision, choice)?,
            definition: relic_acquisition_mechanics_facts_v1(*relic),
        });
    }
    Ok(Some(AgentDecisionEvidenceV1::BossRelicOptions {
        candidates,
        owned_hp_loss_sources: project_agent_hp_loss_sources_v1(
            &AgentStrategicObservationV1::from_public_v1(observation).cards,
        ),
    }))
}

fn limited_combat_relics_for_action_v1(action: &TaskStrategicActionV1) -> Vec<(RelicId, usize)> {
    let TaskStrategicActionV1::ChooseEventOption {
        public_semantics, ..
    } = action
    else {
        return Vec::new();
    };
    let Some(structured) = &public_semantics.structured_semantics else {
        return Vec::new();
    };
    structured
        .effects
        .iter()
        .filter_map(|effect| match effect {
            PlannerEventPublicEffectV1::ObtainRelic {
                count,
                pool: PlannerEventRelicPoolV1::Specific { relic },
            } if *count > 0 => limited_combat_route_effect_v1(*relic)
                .map(|facts| (*relic, facts.affected_combat_count)),
            _ => None,
        })
        .collect()
}

fn first_elite_combat_coverage_v1(
    entry: &AgentRouteEntryPathEvidenceV1,
    affected_combat_count: usize,
) -> AgentRouteEntryCombatCoverageV1 {
    let has_first_elite = entry.path.first_elite.paths_with_first_elite > 0;
    let earliest = has_first_elite.then(|| {
        entry
            .path
            .first_elite
            .min_hallway_fights_before
            .saturating_add(1)
    });
    let latest = has_first_elite.then(|| {
        entry
            .path
            .first_elite
            .max_hallway_fights_before
            .saturating_add(entry.path.first_elite.max_unknowns_before)
            .saturating_add(1)
    });
    let first_elite_coverage = match (entry.coverage, earliest, latest) {
        (RouteWindowCoverageKind::CompleteWithinHorizon, Some(earliest), Some(_))
            if affected_combat_count < earliest =>
        {
            AgentFirstEliteCombatCoverageV1::CannotCover
        }
        (RouteWindowCoverageKind::CompleteWithinHorizon, Some(_), Some(latest))
            if affected_combat_count >= latest =>
        {
            AgentFirstEliteCombatCoverageV1::CoversWithinVisibleRoomUncertainty
        }
        (RouteWindowCoverageKind::CompleteWithinHorizon, Some(_), Some(_)) => {
            AgentFirstEliteCombatCoverageV1::MayCover
        }
        _ => AgentFirstEliteCombatCoverageV1::Unknown,
    };
    AgentRouteEntryCombatCoverageV1 {
        x: entry.x,
        y: entry.y,
        earliest_first_elite_combat_number: earliest,
        latest_first_elite_combat_number_if_all_prior_unknowns_are_combats: latest,
        first_elite_coverage,
    }
}

fn project_agent_route_entry_evidence_v1(
    controller: &mut RunPlayControllerV1,
) -> Result<Option<AgentDecisionEvidenceV1>, String> {
    let inventory = controller
        .head()
        .current_decision
        .clone()
        .ok_or_else(|| "agent route-entry evidence has no cached decision".to_string())?;
    let decision = inventory.public_decision_v1()?;
    let candidates = project_agent_route_entries_v1(controller)?;
    let mut limited_combat_coverages = Vec::new();
    for candidate in &inventory.candidates {
        let TaskPublicCandidateV1::Strategic { action } = &candidate.candidate else {
            continue;
        };
        for (source_relic, affected_combat_count) in limited_combat_relics_for_action_v1(action) {
            let choice = candidate.direct_choice.as_ref().ok_or_else(|| {
                "limited-combat Neow candidate has no direct public choice".to_string()
            })?;
            limited_combat_coverages.push(AgentLimitedCombatCoverageEvidenceV1 {
                semantic_action: semantic_action_for_choice_v1(&decision, choice)?,
                source_relic,
                affected_combat_count,
                route_entries: candidates
                    .iter()
                    .map(|entry| first_elite_combat_coverage_v1(entry, affected_combat_count))
                    .collect(),
            });
        }
    }
    Ok(
        (!candidates.is_empty()).then_some(AgentDecisionEvidenceV1::RouteEntryPaths {
            route_semantics_manifest: route_entry_semantics_manifest_v1(),
            candidates,
            limited_combat_coverages,
        }),
    )
}

fn current_agent_observation_v1(
    controller: &mut RunPlayControllerV1,
    capabilities: AgentCapabilitiesV1,
    locale: Locale,
) -> Result<AgentObservationV1, String> {
    let site = controller
        .head()
        .current_decision
        .as_ref()
        .map(|decision| decision.site);
    let decision_evidence = match site {
        Some(sts_oracle_learning::agent::task::TaskDecisionSiteV1::BossRelic) => {
            project_agent_boss_relic_evidence_v1(controller.head())?
        }
        Some(sts_oracle_learning::agent::task::TaskDecisionSiteV1::Map) => {
            match controller.advice_detail_v1()? {
                RunPlayAdviceDetailV1::Route(audit) => {
                    let mut candidates = audit
                        .candidates
                        .into_iter()
                        .filter_map(|candidate| match candidate.action {
                            RoutePolicyActionV1::Select {
                                x,
                                y,
                                arrival,
                                question_mark_outcomes,
                                path,
                                ..
                            } => {
                                let route_semantics = route_policy_semantics_v1(&path);
                                Some(AgentRoutePathEvidenceV1 {
                                    semantic_action: format!("route:{x}.{y}"),
                                    x,
                                    y,
                                    arrival,
                                    question_mark_outcomes,
                                    route_semantics,
                                    path,
                                })
                            }
                            RoutePolicyActionV1::CancelToPendingRewards => None,
                        })
                        .collect::<Vec<_>>();
                    candidates.sort_by_key(|candidate| (candidate.y, candidate.x));
                    Some(AgentDecisionEvidenceV1::RoutePaths {
                        route_semantics_manifest: route_policy_semantics_manifest_v1(),
                        candidates,
                    })
                }
                other => {
                    return Err(format!(
                        "agent map observation expected route detail, observed {}",
                        match other {
                            RunPlayAdviceDetailV1::CardReward(_) => "card_reward",
                            RunPlayAdviceDetailV1::Campfire(_) => "campfire",
                            RunPlayAdviceDetailV1::BossRelic(_) => "boss_relic",
                            RunPlayAdviceDetailV1::Shop(_) => "shop",
                            RunPlayAdviceDetailV1::RunChoice(_) => "run_choice",
                            RunPlayAdviceDetailV1::PotionReward(_) => "potion_reward",
                            RunPlayAdviceDetailV1::Event(_) => "event",
                            RunPlayAdviceDetailV1::Unavailable { .. } => "unavailable",
                            RunPlayAdviceDetailV1::Route(_) => unreachable!(),
                        }
                    ));
                }
            }
        }
        Some(sts_oracle_learning::agent::task::TaskDecisionSiteV1::Campfire) => {
            match controller.advice_detail_v1()? {
                RunPlayAdviceDetailV1::Campfire(audit) => Some(project_agent_campfire_evidence_v1(
                    controller.head(),
                    &audit,
                )?),
                other => {
                    return Err(format!(
                        "agent campfire observation expected campfire detail, observed {}",
                        match other {
                            RunPlayAdviceDetailV1::CardReward(_) => "card_reward",
                            RunPlayAdviceDetailV1::Route(_) => "route",
                            RunPlayAdviceDetailV1::BossRelic(_) => "boss_relic",
                            RunPlayAdviceDetailV1::Shop(_) => "shop",
                            RunPlayAdviceDetailV1::RunChoice(_) => "run_choice",
                            RunPlayAdviceDetailV1::PotionReward(_) => "potion_reward",
                            RunPlayAdviceDetailV1::Event(_) => "event",
                            RunPlayAdviceDetailV1::Unavailable { .. } => "unavailable",
                            RunPlayAdviceDetailV1::Campfire(_) => unreachable!(),
                        }
                    ));
                }
            }
        }
        Some(sts_oracle_learning::agent::task::TaskDecisionSiteV1::Neow) => {
            project_agent_route_entry_evidence_v1(controller)?
        }
        _ => {
            let evidence =
                project_agent_card_acquisition_evidence_v1(controller.head(), Vec::new())?;
            match evidence {
                Some(AgentDecisionEvidenceV1::CardAcquisitionOptions {
                    candidates,
                    route_semantics_manifest,
                    ..
                }) => Some(AgentDecisionEvidenceV1::CardAcquisitionOptions {
                    candidates,
                    resource_options: project_agent_card_reward_resources_v1(controller)?,
                    route_semantics_manifest,
                    route_entries: project_agent_route_entries_v1(controller)?,
                }),
                other => other,
            }
        }
    };
    project_agent_observation_v1(controller.head(), capabilities, decision_evidence, locale)
}

fn render_sl_piles_v1(root: &sts_oracle_learning::sim::combat::CombatPosition) -> String {
    let zones = &root.combat.zones;
    let mut view =
        "SL internal combat piles | current exact state, no actions simulated\n".to_string();
    for (label, cards) in [
        (
            "Draw (next -> last)",
            zones.draw_pile.iter().collect::<Vec<_>>(),
        ),
        (
            "Discard (bottom -> top; shuffle input order)",
            zones.discard_pile.iter().collect(),
        ),
        (
            "Exhaust (bottom -> top)",
            zones.exhaust_pile.iter().collect(),
        ),
    ] {
        let names = cards
            .iter()
            .enumerate()
            .map(|(i, card)| {
                format!(
                    "{}: {}",
                    i + 1,
                    crate::render::format_card(card.id, card.upgrades)
                )
            })
            .collect::<Vec<_>>();
        view.push_str(&format!(
            "{label} [{}]: {}\n",
            cards.len(),
            if names.is_empty() {
                "empty".into()
            } else {
                names.join(" | ")
            }
        ));
    }
    view.push_str("Positions are pile order, not playable selectors. Only Hand cards can be played.\nThis lists card identities/upgrades; it does not predict later shuffles or draw-trigger effects. Read the new hand after drawing.\n");
    view
}

fn cached_current_agent_observation_v1(
    controller: &mut RunPlayControllerV1,
    capabilities: AgentCapabilitiesV1,
    cache: &mut Option<AgentObservationV1>,
    locale: Locale,
) -> Result<AgentObservationV1, String> {
    if let Some(cached) = cache.as_ref() {
        let head = controller.head();
        if cached.session_id == head.session_id
            && cached.head_revision == head.head_revision
            && cached.state_revision == head.state_revision
        {
            return Ok(cached.clone());
        }
    }
    let observation = current_agent_observation_v1(controller, capabilities, locale)?;
    *cache = Some(observation.clone());
    Ok(observation)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentShellResponseV1 {
    pub schema: &'static str,
    pub request_id: Option<String>,
    pub elapsed_ms: f64,
    #[serde(flatten)]
    pub outcome: AgentShellResponseOutcomeV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentShellResponseOutcomeV1 {
    Ok { result: AgentShellResultV1 },
    Error { error: AgentShellErrorV1 },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentShellResultV1 {
    Decision {
        reading: crate::decision::RunPlayDecisionReadingV1,
    },
    Chosen {
        reading: crate::decision::RunPlayDecisionReadingV1,
        #[serde(skip_serializing_if = "String::is_empty")]
        revealed_changes: String,
    },
    Hello {
        capabilities: AgentCapabilitiesV1,
    },
    Observation {
        observation: AgentObservationV1,
    },
    GameFacts {
        facts: Vec<AgentGameFactV1>,
    },
    Info {
        view: String,
    },
    SlPiles {
        view: String,
    },
    Acted {
        observation: AgentObservationV1,
        reading: crate::decision::RunPlayDecisionReadingV1,
        #[serde(skip_serializing_if = "String::is_empty")]
        revealed_changes: String,
    },
    Probed {
        root_head_revision: u64,
        durable_head_unchanged: bool,
        lines: Vec<AgentProbeLineV1>,
    },
    CombatResearch {
        root_head_revision: u64,
        report_id: String,
        max_search_ms: u64,
        max_generation_work: Option<usize>,
        max_potions_used: Option<u32>,
        search_budget_exhausted: bool,
        view: String,
        durable_head_unchanged: bool,
        effort: AgentCombatResearchEffortV1,
        requested_satisfaction: CombatWitnessSatisfactionV1,
        reused_cached_frontier: bool,
        lanes: Vec<TaskResearchPlanLaneAttemptV1>,
        bounded_progress: Vec<AgentCombatResearchBoundedProgressV1>,
        candidates: Vec<AgentLiveCombatResearchCandidateV1>,
        milestone_candidates: Vec<AgentCombatMilestoneCandidateV1>,
        milestone_successor_preview_count: usize,
        milestone_successor_previews_included: bool,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        milestone_successor_previews: Vec<AgentCombatMilestoneSuccessorPreviewV1>,
    },
    CombatPlans {
        report_id: String,
        effort: AgentCombatResearchEffortV1,
        view: String,
        candidates: Vec<AgentLiveCombatResearchCandidateV1>,
    },
    CombatReview {
        root_head_revision: u64,
        view: String,
        durable_head_unchanged: bool,
        effort: AgentCombatResearchEffortV1,
        candidate_refs: Vec<String>,
        trace: RunPlayCombatTraceV1,
    },
    CombatMilestoneSuccessorDetails {
        root_head_revision: u64,
        durable_head_unchanged: bool,
        effort: AgentCombatResearchEffortV1,
        details: Vec<AgentCombatMilestoneSuccessorDetailV1>,
    },
    CombatMilestoneContinuationAudit {
        root_head_revision: u64,
        durable_head_unchanged: bool,
        effort: AgentCombatResearchEffortV1,
        audits: Vec<RunPlayCombatMilestoneContinuationAuditV1>,
    },
    CombatPrefixHorizonAudit {
        root_head_revision: u64,
        durable_head_unchanged: bool,
        audit: RunPlayCombatPrefixHorizonAuditV1,
    },
    CombatDraftStatus {
        #[serde(flatten)]
        response: AgentCombatDraftStatusV1,
    },
    CombatDraftAdvanced {
        #[serde(flatten)]
        response: AgentCombatDraftAdvancedV1,
    },
    CombatDraftInspected {
        #[serde(flatten)]
        response: AgentCombatDraftInspectedV1,
    },
    CombatDraftChosen {
        #[serde(flatten)]
        response: AgentCombatDraftChosenV1,
    },
    CombatDraftUndone {
        #[serde(flatten)]
        response: AgentCombatDraftUndoneV1,
    },
    CombatDraftSelection {
        #[serde(flatten)]
        response: AgentCombatDraftSelectionV1,
    },
    CombatPlanCommitted {
        candidate_ref: String,
        candidate_id: String,
        observation: AgentObservationV1,
        reading: crate::decision::RunPlayDecisionReadingV1,
    },
    CombatMilestoneCommitted {
        candidate_id: String,
        observation: AgentObservationV1,
    },
    CombatTurnResearch {
        root_head_revision: u64,
        durable_head_unchanged: bool,
        reused_cached_frontier: bool,
        nodes_expanded: usize,
        nodes_generated: usize,
        engine_steps: usize,
        exact_state_skips: usize,
        truncated_children: usize,
        preselection_candidate_count: usize,
        candidates: Vec<AgentCombatTurnCandidateSummaryV1>,
    },
    CombatTurnReserved {
        root_head_revision: u64,
        durable_head_unchanged: bool,
        already_present: bool,
        candidate_count: usize,
        candidate: AgentCombatTurnCandidateSummaryV1,
    },
    CombatTurnCandidateDetails {
        root_head_revision: u64,
        durable_head_unchanged: bool,
        details: Vec<RunPlayCombatTurnCandidateV1>,
    },
    CombatTurnContinuationAudit {
        root_head_revision: u64,
        durable_head_unchanged: bool,
        effort: AgentCombatResearchEffortV1,
        audits: Vec<RunPlayCombatTurnContinuationAuditV1>,
        observed_terminal_frontier: Vec<RunPlayCombatTurnObservedTerminalOutcomeV1>,
        observed_cross_surface_frontier: Vec<AgentCombatObservedTerminalOutcomeV1>,
        cross_surface_frontier_includes_cached_combat_plans: bool,
        observed_witnesses_only: bool,
        absence_means_unknown: bool,
    },
    CombatTurnCommitted {
        candidate_id: String,
        observation: AgentObservationV1,
    },
    Quitting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentCombatResearchCacheSourceV1 {
    AutomatedResearch,
    AnalystDraft,
}

struct AgentCombatResearchCacheV1 {
    report_id: Option<String>,
    source: AgentCombatResearchCacheSourceV1,
    root_head_revision: u64,
    effort: AgentCombatResearchEffortV1,
    requested_satisfaction: CombatWitnessSatisfactionV1,
    potion_policy: CombatWitnessPotionPolicyV1,
    max_potions_used: Option<u32>,
    max_search_ms: u64,
    max_generation_work: Option<usize>,
    search_budget_exhausted: bool,
    attempt: sts_oracle_learning::agent::task::TaskResearchPlanAttemptV1,
    witness_lines: Vec<ExactRunPlayProbeLineV1>,
    bounded_progress: Vec<AgentCombatResearchBoundedProgressV1>,
    candidates: Vec<AgentCombatResearchCandidateV1>,
    milestone_candidates: Vec<RunPlayCombatMilestoneResearchCandidateV1>,
    projected_milestone_candidates: Vec<AgentCombatMilestoneCandidateV1>,
    milestone_successor_previews: Vec<AgentCombatMilestoneSuccessorPreviewV1>,
}

struct AgentCombatTurnCacheV1 {
    root_head_revision: u64,
    attempt: RunPlayCombatTurnAttemptV1,
}

fn combat_research_cache_from_draft_attempt_v1(
    root_head_revision: u64,
    report: RunPlayCombatResearchAttemptV1,
) -> Result<AgentCombatResearchCacheV1, String> {
    let candidates = project_agent_combat_research_candidates_v1(&report)?;
    if candidates.len() != 1 {
        return Err("analyst combat draft did not project exactly one commit candidate".to_owned());
    }
    Ok(AgentCombatResearchCacheV1 {
        report_id: None,
        source: AgentCombatResearchCacheSourceV1::AnalystDraft,
        root_head_revision,
        effort: AgentCombatResearchEffortV1::Standard,
        requested_satisfaction: CombatWitnessSatisfactionV1::FirstCompleteWin,
        potion_policy: CombatWitnessPotionPolicyV1::All,
        max_potions_used: None,
        max_search_ms: 0,
        max_generation_work: None,
        search_budget_exhausted: false,
        attempt: report.attempt,
        witness_lines: report.witness_lines,
        bounded_progress: Vec::new(),
        candidates,
        milestone_candidates: Vec::new(),
        projected_milestone_candidates: Vec::new(),
        milestone_successor_previews: Vec::new(),
    })
}

fn resolve_cached_combat_research_ref_v1<'a>(
    cached: &'a AgentCombatResearchCacheV1,
    candidate_ref: &str,
) -> Result<&'a AgentCombatResearchCandidateV1, String> {
    let index = candidate_ref
        .strip_prefix('c')
        .and_then(|ordinal| ordinal.parse::<usize>().ok())
        .ok_or_else(|| {
            format!(
                "combat research candidate ref {candidate_ref:?} is invalid; use a cN ref from the current cached response"
            )
        })?;
    if candidate_ref != format!("c{index}") {
        return Err(format!(
            "combat research candidate ref {candidate_ref:?} is not canonical; use c{index} from the current cached response"
        ));
    }
    cached.candidates.get(index).ok_or_else(|| {
        format!(
            "combat research candidate ref {candidate_ref:?} is absent from the current cached frontier"
        )
    })
}

fn resolve_cached_combat_research_refs_v1(
    cached: &AgentCombatResearchCacheV1,
    candidate_refs: &[String],
) -> Result<Vec<String>, String> {
    candidate_refs
        .iter()
        .map(|candidate_ref| {
            resolve_cached_combat_research_ref_v1(cached, candidate_ref)
                .map(|candidate| candidate.candidate_id.clone())
        })
        .collect()
}

fn projected_combat_research_candidate_indices_v1(
    report: &RunPlayCombatResearchAttemptV1,
) -> Result<Vec<usize>, String> {
    if report.attempt.candidates.len() != report.witness_lines.len() {
        return Err("combat research candidates and witness lines disagree".to_string());
    }
    Ok(crate::driver::combat_research_frontier_indices_v1(
        &report.attempt.candidates,
    ))
}

fn project_agent_combat_research_candidates_v1(
    report: &RunPlayCombatResearchAttemptV1,
) -> Result<Vec<AgentCombatResearchCandidateV1>, String> {
    projected_combat_research_candidate_indices_v1(report)?
        .into_iter()
        .map(|index| {
            let candidate = &report.attempt.candidates[index];
            let witness = &report.witness_lines[index];
            let action_line = witness
                .steps
                .iter()
                .map(|step| semantic_action_for_choice_v1(&step.source_decision, &step.choice))
                .collect::<Result<Vec<_>, String>>()?;
            Ok(AgentCombatResearchCandidateV1 {
                candidate_id: combat_research_candidate_id_v1(candidate)?,
                report_candidate_index: Some(index),
                supporting_provenances: candidate.supporting_provenances.clone(),
                plan_steps: candidate.plan.steps.len(),
                minimum_observed_hp: crate::combat_trace::witness_minimum_hp(
                    witness,
                    candidate.outcome.final_hp,
                ),
                satisfies_requested_quality: candidate.satisfies_requested_quality,
                outcome: candidate.outcome.clone(),
                action_line,
            })
        })
        .collect()
}

fn project_agent_combat_research_bounded_progress_v1(
    progress: &[RunPlayCombatResearchBoundedProgressV1],
) -> Result<Vec<AgentCombatResearchBoundedProgressV1>, String> {
    progress
        .iter()
        .map(|progress| {
            let paths = progress
                .paths
                .iter()
                .map(|path| {
                    let action_line = path.action_line.clone();
                    if action_line.len() != path.candidate.plan.steps.len() {
                        return Err(
                            "bounded progress plan and replay action counts disagree".to_string()
                        );
                    }
                    Ok(AgentCombatResearchBoundedProgressPathV1 {
                        roles: path.candidate.roles.clone(),
                        plan_steps: path.candidate.plan.steps.len(),
                        endpoint: path.candidate.endpoint,
                        action_line,
                        successor_public_snapshot_id: path.successor_public_snapshot_id.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(AgentCombatResearchBoundedProgressV1 {
                provenance: progress.provenance.clone(),
                bounded_search_only: true,
                absence_means_unknown: true,
                max_player_turn: progress.max_player_turn,
                recent_turn_survival_envelope: progress.recent_turn_survival_envelope.clone(),
                paths,
            })
        })
        .collect()
}

fn project_agent_combat_research_replay_proofs_v1(
    report: &RunPlayCombatResearchAttemptV1,
    candidates: &[AgentCombatResearchCandidateV1],
) -> Result<Vec<AgentCombatResearchReplayProofV1>, String> {
    let projected_indices = projected_combat_research_candidate_indices_v1(report)?;
    if projected_indices.len() != candidates.len() {
        return Err(
            "combat research projected candidates and replay witnesses disagree".to_string(),
        );
    }
    projected_indices
        .into_iter()
        .zip(candidates)
        .map(|(index, candidate)| {
            let witness = &report.witness_lines[index];
            let steps = witness
                .steps
                .iter()
                .map(|step| {
                    Ok(AgentCombatResearchReplayStepV1 {
                        source_public_snapshot_id: step
                            .source_decision
                            .information
                            .snapshot_id
                            .clone(),
                        action: semantic_action_for_choice_v1(&step.source_decision, &step.choice)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let final_step = witness.steps.last().ok_or_else(|| {
                "combat research replay witness must contain at least one step".to_string()
            })?;
            let endpoint = match &final_step.next_decision {
                Some(decision) => AgentCombatResearchReplayEndpointV1::PublicDecision {
                    public_snapshot_id: decision.information.snapshot_id.clone(),
                },
                None => AgentCombatResearchReplayEndpointV1::GameTerminal {
                    outcome: final_step.outcome.clone(),
                },
            };
            Ok(AgentCombatResearchReplayProofV1 {
                candidate_id: candidate.candidate_id.clone(),
                steps,
                endpoint,
            })
        })
        .collect()
}

fn project_agent_combat_milestone_card_zone_preview_v1(
    zone: &CombatLearningCardCollectionV1,
) -> AgentCombatMilestoneCardZonePreviewV1 {
    AgentCombatMilestoneCardZonePreviewV1 {
        evidence: zone.evidence,
        cards: zone.cards.iter().map(Into::into).collect(),
    }
}

fn project_agent_combat_milestone_successor_preview_v1(
    candidate: &RunPlayCombatMilestoneResearchCandidateV1,
) -> AgentCombatMilestoneSuccessorPreviewV1 {
    let successor = &candidate.successor;
    AgentCombatMilestoneSuccessorPreviewV1 {
        successor_public_snapshot_id: candidate.successor_public_snapshot_id.clone(),
        turn: successor.turn.clone(),
        player_hp: successor.player.hp,
        player_max_hp: successor.player.max_hp,
        player_block: successor.player.block,
        player_relics: successor.player.relics.clone(),
        player_powers: successor.player.powers.clone(),
        potions: successor.potions.clone(),
        hand: successor.cards.hand.cards.clone(),
        draw: project_agent_combat_milestone_card_zone_preview_v1(&successor.cards.draw),
        discard: project_agent_combat_milestone_card_zone_preview_v1(&successor.cards.discard),
        exhaust: project_agent_combat_milestone_card_zone_preview_v1(&successor.cards.exhaust),
        monsters: successor
            .monsters
            .iter()
            .map(|monster| AgentCombatMilestoneMonsterPreviewV1 {
                slot: monster.slot,
                enemy: monster.enemy,
                hp: monster.hp,
                max_hp: monster.max_hp,
                block: monster.block,
                alive: monster.alive,
                escaped: monster.escaped,
                dying: monster.dying,
                half_dead: monster.half_dead,
                intent: monster.intent.clone(),
                executed_moves: monster.executed_moves.clone(),
                public_counters: monster.public_counters.clone(),
                powers: monster.powers.clone(),
            })
            .collect(),
    }
}

fn project_agent_combat_milestone_frontier_v1(
    candidates: &[RunPlayCombatMilestoneResearchCandidateV1],
) -> Result<
    (
        Vec<AgentCombatMilestoneCandidateV1>,
        Vec<AgentCombatMilestoneSuccessorPreviewV1>,
    ),
    String,
> {
    let projected_candidates = candidates
        .iter()
        .map(|candidate| {
            Ok(AgentCombatMilestoneCandidateV1 {
                candidate_id: candidate.candidate_id.clone(),
                provenance: candidate.candidate.provenance.clone(),
                plan_steps: candidate.candidate.plan.steps.len(),
                completed_milestones: candidate
                    .candidate
                    .transition
                    .completed_milestones()
                    .to_vec(),
                final_player_hp: candidate.candidate.final_player_hp,
                combat_continues: true,
                action_line: candidate.action_line.clone(),
                successor_public_snapshot_id: candidate.successor_public_snapshot_id.clone(),
                successor_summary: AgentCombatMilestoneSuccessorSummaryV1 {
                    turn_count: candidate.successor.turn.turn_count,
                    current_energy: candidate.successor.turn.energy,
                    player_hp: candidate.successor.player.hp,
                    player_block: candidate.successor.player.block,
                    visible_incoming_damage: candidate
                        .candidate
                        .successor_plan
                        .envelope
                        .visible_incoming_damage,
                    visible_damage_margin: candidate
                        .candidate
                        .successor_plan
                        .envelope
                        .visible_damage_margin,
                    visible_hand_block_sum: candidate
                        .successor
                        .cards
                        .hand
                        .cards
                        .iter()
                        .map(|card| card.current_block.max(0))
                        .fold(0i32, i32::saturating_add),
                    remaining_hand_block_sources: candidate
                        .candidate
                        .successor_plan
                        .resources
                        .remaining_hand_block_sources,
                    remaining_accessible_block_sources: candidate
                        .candidate
                        .successor_plan
                        .resources
                        .remaining_accessible_block_sources,
                    remaining_strength_reduction: candidate
                        .candidate
                        .successor_plan
                        .resources
                        .remaining_strength_reduction,
                    live_status_cards: candidate
                        .candidate
                        .successor_plan
                        .envelope
                        .live_status_cards,
                    enemy_team_strength: candidate
                        .candidate
                        .successor_plan
                        .envelope
                        .enemy_team_strength,
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut public_states_by_snapshot = BTreeMap::new();
    let mut previews_by_snapshot = BTreeMap::new();
    for candidate in candidates {
        match public_states_by_snapshot.get(&candidate.successor_public_snapshot_id) {
            Some(existing) if *existing != &candidate.successor => {
                return Err(format!(
                    "public combat snapshot id {} maps to unequal milestone successor states",
                    candidate.successor_public_snapshot_id
                ));
            }
            Some(_) => {}
            None => {
                public_states_by_snapshot.insert(
                    candidate.successor_public_snapshot_id.clone(),
                    &candidate.successor,
                );
            }
        }
        let preview = project_agent_combat_milestone_successor_preview_v1(candidate);
        match previews_by_snapshot.get(&candidate.successor_public_snapshot_id) {
            Some(existing) if existing != &preview => {
                return Err(format!(
                    "public combat snapshot id {} maps to unequal milestone previews",
                    candidate.successor_public_snapshot_id
                ));
            }
            Some(_) => {}
            None => {
                previews_by_snapshot
                    .insert(candidate.successor_public_snapshot_id.clone(), preview);
            }
        }
    }
    Ok((
        projected_candidates,
        previews_by_snapshot.into_values().collect(),
    ))
}

pub fn capture_combat_research_artifact_v1(
    controller: &mut RunPlayControllerV1,
    effort: AgentCombatResearchEffortV1,
) -> Result<
    (
        RunPlayCombatResearchAttemptV1,
        RunPlayCombatResearchArtifactV1,
    ),
    String,
> {
    capture_combat_research_artifact_with_progress_v1(controller, effort, &mut |_| {})
}

pub fn capture_combat_research_artifact_with_progress_v1(
    controller: &mut RunPlayControllerV1,
    effort: AgentCombatResearchEffortV1,
    progress: &mut dyn FnMut(CombatResearchProgressEventV1),
) -> Result<
    (
        RunPlayCombatResearchAttemptV1,
        RunPlayCombatResearchArtifactV1,
    ),
    String,
> {
    let root_head_revision = controller.head().head_revision;
    let root_state_revision = controller.head().state_revision;
    let session_id = controller.head().session_id.clone();
    let root_archive_node_id = controller.head().position.archive_node_id().to_string();
    let root_public_snapshot_id = controller
        .head()
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat research artifact requires a public decision".to_string())?
        .public_decision_v1()?
        .information
        .snapshot_id;
    let report = combat_research_attempt_for_effort_with_progress_v1(
        controller, effort, None, None, None, progress,
    )?;
    let head = controller.head();
    let current_public_snapshot_id = head
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat research artifact lost its public decision".to_string())?
        .public_decision_v1()?
        .information
        .snapshot_id;
    if head.head_revision != root_head_revision
        || head.state_revision != root_state_revision
        || head.session_id != session_id
        || head.position.archive_node_id() != root_archive_node_id
        || current_public_snapshot_id != root_public_snapshot_id
    {
        return Err("combat research artifact changed its durable controller root".to_string());
    }
    let artifact = artifact_from_report(controller, effort, &report)?;
    Ok((report, artifact))
}

fn artifact_from_report(
    controller: &RunPlayControllerV1,
    effort: AgentCombatResearchEffortV1,
    report: &RunPlayCombatResearchAttemptV1,
) -> Result<RunPlayCombatResearchArtifactV1, String> {
    let head = controller.head();
    let source = crate::decision::decision_source(head)?;
    let candidates = project_agent_combat_research_candidates_v1(report)?;
    let (milestone_candidates, milestone_successor_previews) =
        project_agent_combat_milestone_frontier_v1(&report.milestone_candidates)?;
    let artifact = RunPlayCombatResearchArtifactV1 {
        schema: COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1.into(),
        source_evidence_identity: head.evidence_identity.clone(),
        session_id: source.session_id,
        root_head_revision: source.head_revision,
        root_state_revision: head.state_revision,
        root_archive_node_id: source.archive_node_id,
        root_public_snapshot_id: source.public_snapshot_id,
        effort,
        durable_head_unchanged: true,
        lanes: report.attempt.lanes.clone(),
        bounded_progress: project_agent_combat_research_bounded_progress_v1(
            &report.bounded_progress,
        )?,
        candidate_replay_proofs: project_agent_combat_research_replay_proofs_v1(
            report,
            &candidates,
        )?,
        candidates,
        milestone_candidates,
        milestone_successor_previews,
    };
    artifact.validate()?;
    Ok(artifact)
}

pub fn load_combat_research_artifact_v1(
    path: &Path,
) -> Result<RunPlayCombatResearchArtifactV1, String> {
    let payload = read_bounded(
        path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run-play combat research artifact",
    )?;
    let artifact: RunPlayCombatResearchArtifactV1 =
        serde_json::from_slice(&payload).map_err(|error| {
            format!(
                "failed to decode combat research artifact {}: {error}",
                path.display()
            )
        })?;
    artifact.validate()?;
    Ok(artifact)
}

/// Selects and transactionally replays one candidate from a root-bound typed
/// artifact. No search is performed here: the live session, every step source,
/// and the final boundary are independently checked against the artifact.
pub fn anchor_combat_research_artifact_candidate_v1(
    controller: &mut RunPlayControllerV1,
    artifact: &RunPlayCombatResearchArtifactV1,
    producer_name: &str,
    requested_candidate_index: Option<usize>,
) -> Result<RunPlayActionLineProfileV1, String> {
    artifact.validate()?;
    if artifact.candidate_replay_proofs.is_empty() {
        return Err(
            "combat research artifact is legacy inspection-only evidence and has no replay proofs"
                .to_string(),
        );
    }
    let head = controller.head();
    let observed_snapshot_id = head
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat research artifact requires a current public decision".to_string())?
        .public_decision_v1()?
        .information
        .snapshot_id;
    // Head and state revisions identify the audit path, not the game root. A
    // validated jump back to the same content-addressed archive node creates
    // fresh revisions, while preserving the exact position that this replay
    // proof was captured from. Keep the root bound to the session, archive
    // node, and public snapshot; every replay step and endpoint are validated
    // independently below.
    if head.evidence_identity != artifact.source_evidence_identity {
        return Err(format!(
            "combat research artifact evidence identity mismatch: artifact={} current={}",
            RunPlayEvidenceIdentityV1::compatibility_label_v1(
                artifact.source_evidence_identity.as_ref(),
            ),
            RunPlayEvidenceIdentityV1::compatibility_label_v1(head.evidence_identity.as_ref()),
        ));
    }
    if head.session_id != artifact.session_id
        || head.position.archive_node_id() != artifact.root_archive_node_id
        || observed_snapshot_id != artifact.root_public_snapshot_id
    {
        return Err(format!(
            "combat research artifact root mismatch: artifact session={} head={} state={} node={} snapshot={}, current session={} head={} state={} node={} snapshot={}",
            artifact.session_id,
            artifact.root_head_revision,
            artifact.root_state_revision,
            artifact.root_archive_node_id,
            artifact.root_public_snapshot_id,
            head.session_id,
            head.head_revision,
            head.state_revision,
            head.position.archive_node_id(),
            observed_snapshot_id,
        ));
    }
    let candidate_index = resolve_artifact_combat_research_candidate_index_v1(
        &artifact.candidates,
        producer_name,
        requested_candidate_index,
    )?;
    let candidate = &artifact.candidates[candidate_index];
    let replay = artifact
        .candidate_replay_proofs
        .iter()
        .find(|replay| replay.candidate_id == candidate.candidate_id)
        .ok_or_else(|| {
            format!(
                "combat research artifact has no replay proof for candidate {}",
                candidate.candidate_id
            )
        })?;
    let selectors = replay
        .steps
        .iter()
        .map(|step| parse_action_selector_v1(&step.action, "anchor-combat-plan artifact"))
        .collect::<Result<Vec<_>, String>>()?;
    let source_public_snapshot_ids = replay
        .steps
        .iter()
        .map(|step| step.source_public_snapshot_id.clone())
        .collect::<Vec<_>>();
    let evidence = RunPlayCombatResearchCandidateV1 {
        supporting_provenances: candidate.supporting_provenances.clone(),
        outcome: candidate.outcome.clone(),
        satisfies_requested_quality: candidate.satisfies_requested_quality,
    };
    match &replay.endpoint {
        AgentCombatResearchReplayEndpointV1::PublicDecision { public_snapshot_id } => controller
            .anchor_combat_research_replay_v1(
                &source_public_snapshot_ids,
                &selectors,
                Some(public_snapshot_id),
                None,
                evidence,
            ),
        AgentCombatResearchReplayEndpointV1::GameTerminal { outcome } => controller
            .anchor_combat_research_replay_v1(
                &source_public_snapshot_ids,
                &selectors,
                None,
                Some(outcome),
                evidence,
            ),
    }
}

pub(crate) fn resolve_artifact_combat_research_report_candidate_index_v1(
    candidates: &[AgentCombatResearchCandidateV1],
    requested_report_index: usize,
) -> Result<usize, String> {
    if candidates
        .iter()
        .all(|candidate| candidate.report_candidate_index.is_none())
    {
        return candidates
            .get(requested_report_index)
            .map(|_| requested_report_index)
            .ok_or_else(|| {
                format!(
                    "combat research legacy artifact candidate {requested_report_index} is outside candidate count {}",
                    candidates.len()
                )
            });
    }

    candidates
        .iter()
        .position(|candidate| candidate.report_candidate_index == Some(requested_report_index))
        .ok_or_else(|| {
            format!(
                "combat research report candidate {requested_report_index} is not retained in this artifact; available_report_candidates=[{}]",
                candidates
                    .iter()
                    .filter_map(|candidate| candidate.report_candidate_index)
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
}

fn resolve_artifact_combat_research_candidate_index_v1(
    candidates: &[AgentCombatResearchCandidateV1],
    producer_name: &str,
    requested_report_index: Option<usize>,
) -> Result<usize, String> {
    let candidate_producers = candidates
        .iter()
        .map(|candidate| {
            candidate
                .supporting_provenances
                .iter()
                .map(|provenance| provenance.producer_name.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let is_legacy = candidates
        .iter()
        .all(|candidate| candidate.report_candidate_index.is_none());
    if is_legacy && requested_report_index.is_none() {
        return resolve_combat_research_candidate_index_v1(
            &candidate_producers,
            producer_name,
            requested_report_index,
        );
    }

    if let Some(report_index) = requested_report_index {
        let local_index =
            resolve_artifact_combat_research_report_candidate_index_v1(candidates, report_index)?;
        let actual_producers = &candidate_producers[local_index];
        if !actual_producers.contains(&producer_name) {
            return Err(format!(
                "combat research report candidate {report_index} is supported by producers [{}], not {producer_name}",
                actual_producers.join(", ")
            ));
        }
        return Ok(local_index);
    }

    let matching_indices = candidate_producers
        .iter()
        .enumerate()
        .filter_map(|(local_index, actual_producers)| {
            actual_producers
                .contains(&producer_name)
                .then_some(local_index)
        })
        .collect::<Vec<_>>();
    match matching_indices.as_slice() {
        [local_index] => Ok(*local_index),
        [] => Err(format!(
            "combat research producer {producer_name} has no candidate; available_report_candidates=[{}]",
            candidates
                .iter()
                .filter_map(|candidate| candidate.report_candidate_index)
                .map(|index| index.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )),
        _ => Err(format!(
            "combat research producer {producer_name} produced multiple candidates at report indices [{}]; pass --candidate <index>",
            matching_indices
                .iter()
                .filter_map(|local_index| candidates[*local_index].report_candidate_index)
                .map(|index| index.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn project_agent_combat_milestone_successor_details_v1(
    candidates: &[RunPlayCombatMilestoneResearchCandidateV1],
    candidate_ids: &[String],
) -> Result<Vec<AgentCombatMilestoneSuccessorDetailV1>, String> {
    candidate_ids
        .iter()
        .map(|candidate_id| {
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.candidate_id == *candidate_id)
                .ok_or_else(|| {
                    format!(
                        "combat milestone successor detail candidate id {candidate_id:?} is not in the cached frontier"
                    )
                })?;
            Ok(AgentCombatMilestoneSuccessorDetailV1 {
                candidate_id: candidate.candidate_id.clone(),
                successor_public_snapshot_id: candidate.successor_public_snapshot_id.clone(),
                transition: candidate.candidate.transition.clone(),
                successor_plan: candidate.candidate.successor_plan.clone(),
                successor: candidate.successor.clone(),
            })
        })
        .collect()
}

fn project_agent_combat_turn_candidate_summaries_v1(
    candidates: &[RunPlayCombatTurnCandidateV1],
) -> Result<Vec<AgentCombatTurnCandidateSummaryV1>, String> {
    candidates
        .iter()
        .map(project_agent_combat_turn_candidate_summary_v1)
        .collect()
}

fn project_agent_combat_turn_candidate_summary_v1(
    candidate: &RunPlayCombatTurnCandidateV1,
) -> Result<AgentCombatTurnCandidateSummaryV1, String> {
    let actions = candidate
        .view
        .actions
        .iter()
        .map(|selected| match selected {
            CompleteTurnSelectedActionV1::Atomic {
                action,
                card,
                potion,
                ..
            } => match action {
                PublicCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                } => Ok(AgentCombatTurnActionSummaryV1::PlayCard {
                    hand_index: *hand_index,
                    card: card
                        .as_ref()
                        .ok_or_else(|| {
                            "complete-turn play-card summary is missing its public card"
                                .to_string()
                        })?
                        .into(),
                    target_monster_index: *target_monster_index,
                }),
                PublicCombatAtomicActionV1::UsePotion {
                    potion_index,
                    target_monster_index,
                } => Ok(AgentCombatTurnActionSummaryV1::UsePotion {
                    potion_index: *potion_index,
                    potion_id: potion
                        .as_ref()
                        .ok_or_else(|| {
                            "complete-turn use-potion summary is missing its public potion"
                                .to_string()
                        })?
                        .potion_id,
                    target_monster_index: *target_monster_index,
                }),
                PublicCombatAtomicActionV1::DiscardPotion { potion_index } => {
                    Ok(AgentCombatTurnActionSummaryV1::DiscardPotion {
                        potion_index: *potion_index,
                        potion_id: potion
                            .as_ref()
                            .ok_or_else(|| {
                                "complete-turn discard-potion summary is missing its public potion"
                                    .to_string()
                            })?
                            .potion_id,
                    })
                }
                PublicCombatAtomicActionV1::EndTurn => {
                    Ok(AgentCombatTurnActionSummaryV1::EndTurn)
                }
                PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index } => {
                    Ok(AgentCombatTurnActionSummaryV1::SubmitIndexedChoice {
                        choice_index: *choice_index,
                    })
                }
                PublicCombatAtomicActionV1::Proceed => {
                    Ok(AgentCombatTurnActionSummaryV1::Proceed)
                }
                PublicCombatAtomicActionV1::Cancel => {
                    Ok(AgentCombatTurnActionSummaryV1::Cancel)
                }
            },
            CompleteTurnSelectedActionV1::Selection {
                family,
                selected_domain_ordinals,
            } => {
                let selected_candidates = selected_domain_ordinals
                    .iter()
                    .map(|ordinal| {
                        family.domain.get(*ordinal).copied().ok_or_else(|| {
                            format!(
                                "complete-turn selection summary domain ordinal {ordinal} is out of range"
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(AgentCombatTurnActionSummaryV1::Selection {
                    reason: family.reason.clone(),
                    source_pile: family.source_pile,
                    selected_domain_ordinals: selected_domain_ordinals.clone(),
                    selected_candidates,
                })
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    let successor = &candidate.view.successor;
    let power_summary = |powers: &[CombatLearningPowerV1]| {
        powers
            .iter()
            .map(|power| AgentCombatTurnPowerSummaryV1 {
                power: power.power,
                amount: power.amount,
            })
            .collect::<Vec<_>>()
    };
    Ok(AgentCombatTurnCandidateSummaryV1 {
        candidate_id: candidate.candidate_id.clone(),
        actions,
        boundary: candidate.view.boundary,
        potion_slot_mask: candidate.view.potion_slot_mask,
        potion_expenditures: candidate.view.potion_expenditures,
        endpoint: AgentCombatTurnEndpointSummaryV1 {
            turn_count: successor.turn.turn_count,
            player_hp: successor.player.hp,
            player_block: successor.player.block,
            energy: successor.turn.energy,
            player_powers: power_summary(&successor.player.powers),
            potion_ids: successor
                .potions
                .iter()
                .map(|potion| potion.as_ref().map(|potion| potion.potion_id))
                .collect(),
            hand: successor
                .cards
                .hand
                .cards
                .iter()
                .map(AgentCombatTurnCardSummaryV1::from)
                .collect(),
            draw_count: successor.cards.draw.cards.len(),
            discard_count: successor.cards.discard.cards.len(),
            exhaust_count: successor.cards.exhaust.cards.len(),
            monsters: successor
                .monsters
                .iter()
                .map(|monster| AgentCombatTurnMonsterSummaryV1 {
                    slot: monster.slot,
                    enemy: monster.enemy,
                    hp: monster.hp,
                    block: monster.block,
                    alive: monster.alive,
                    powers: power_summary(&monster.powers),
                })
                .collect(),
        },
    })
}

fn project_agent_combat_turn_candidate_details_v1(
    candidates: &[RunPlayCombatTurnCandidateV1],
    candidate_ids: &[String],
) -> Result<Vec<RunPlayCombatTurnCandidateV1>, String> {
    candidate_ids
        .iter()
        .map(|candidate_id| {
            candidates
                .iter()
                .find(|candidate| candidate.candidate_id == *candidate_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "combat-turn candidate detail id {candidate_id:?} is not in the cached frontier"
                    )
                })
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentShellErrorV1 {
    pub code: &'static str,
    pub message: String,
}

fn success_response_v1(
    request_id: String,
    started: Instant,
    result: AgentShellResultV1,
) -> AgentShellResponseV1 {
    AgentShellResponseV1 {
        schema: AGENT_RESPONSE_SCHEMA_V1,
        request_id: Some(request_id),
        elapsed_ms: started.elapsed().as_secs_f64() * 1_000.0,
        outcome: AgentShellResponseOutcomeV1::Ok { result },
    }
}

fn error_response_v1(
    request_id: Option<String>,
    started: Instant,
    code: &'static str,
    message: String,
) -> AgentShellResponseV1 {
    AgentShellResponseV1 {
        schema: AGENT_RESPONSE_SCHEMA_V1,
        request_id,
        elapsed_ms: started.elapsed().as_secs_f64() * 1_000.0,
        outcome: AgentShellResponseOutcomeV1::Error {
            error: AgentShellErrorV1 { code, message },
        },
    }
}

fn reveals_deck_result_v1(candidate: &TaskPublicCandidateV1) -> bool {
    use sts_oracle_learning::state::selection::SelectionReason;
    matches!(
        candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::BeginRunCardSelection {
                reason: SelectionReason::Transform | SelectionReason::TransformUpgraded,
                ..
            } | TaskStrategicActionV1::TakeBossRelic { .. }
                | TaskStrategicActionV1::SubmitRunSelection { .. }
                | TaskStrategicActionV1::BuyRelic { .. }
                | TaskStrategicActionV1::ClaimReward {
                    reward: PlannerRewardDescriptor::Relic { .. },
                    ..
                }
                | TaskStrategicActionV1::ChooseEventOption { .. }
                | TaskStrategicActionV1::OpenChest
                | TaskStrategicActionV1::Dig
        }
    )
}

fn render_revealed_deck_result_v1(
    controller: &RunPlayControllerV1,
    before: Option<&sts_oracle_learning::agent::task::TaskPublicDecisionV1>,
    reveals: bool,
) -> Result<String, String> {
    if !reveals {
        return Ok(String::new());
    }
    let Some(before) = before else {
        return Ok(String::new());
    };
    let Some(after) = controller.head().current_decision.as_ref() else {
        return Ok(String::new());
    };
    let change = crate::compact::preview_deck_change_v1(before, &after.public_decision_v1()?);
    Ok(match change {
        Some(change) => format!("Deck change: {change}\n"),
        None => String::new(),
    })
}

fn handle_valid_request_v1(
    controller: &mut RunPlayControllerV1,
    capabilities: &AgentCapabilitiesV1,
    observation_cache: &mut Option<AgentObservationV1>,
    combat_research_cache: &mut Option<AgentCombatResearchCacheV1>,
    combat_turn_cache: &mut Option<AgentCombatTurnCacheV1>,
    combat_draft: &mut AgentCombatDraftRuntimeV1,
    request: &AgentShellRequestV1,
    locale: Locale,
) -> Result<(AgentShellResultV1, bool), String> {
    if capabilities.permission_envelope.information_ceiling == "public_only"
        && !request.public_manual_operation()
    {
        return Err("Public manual play disables search, private previews and exact hidden piles. Execute actual actions; use retained returns to retry.".into());
    }
    match request {
        AgentShellRequestV1::Decision {
            details,
            known_context,
            ..
        } => {
            let mut reading =
                controller.decision_reading_v1(details, known_context.as_ref(), locale)?;
            if reading.control == crate::decision::DecisionControlV1::Combat
                && capabilities.permission_envelope.information_ceiling
                    == "privileged_exact_allowed"
            {
                match SavedPlans::current_if_present(controller) {
                    Ok(Some(saved)) if !saved.artifact.candidates.is_empty() => {
                        reading.view.push_str(&format!(
                            "\n{}\n",
                            crate::combat_reading::research_message(locale, "savedHeading", &[])
                        ));
                        reading
                            .view
                            .push_str(&saved.view(controller, false, locale)?);
                    }
                    Err(error) => {
                        reading.view.push_str(&format!(
                            "\n{}\n",
                            crate::combat_reading::research_message(
                                locale,
                                "cacheUnavailable",
                                &[("error", &error)]
                            )
                        ));
                    }
                    _ => {}
                }
            }
            Ok((AgentShellResultV1::Decision { reading }, false))
        }
        AgentShellRequestV1::Choose {
            selection,
            following_rewards,
            known_context,
            ..
        } => {
            let before = controller
                .head()
                .current_decision
                .as_ref()
                .filter(|decision| {
                    decision.site != sts_oracle_learning::agent::task::TaskDecisionSiteV1::Combat
                })
                .map(|decision| decision.public_decision_v1())
                .transpose()?;
            let reveals = before.as_ref().is_some_and(|decision| {
                selection
                    .choice
                    .ordinal_path
                    .first()
                    .is_some_and(|ordinal| {
                        decision
                            .candidates
                            .get(*ordinal)
                            .is_some_and(reveals_deck_result_v1)
                    })
            });
            if let Some(following) = following_rewards {
                controller.choose_reward_sequence_v1(selection, following)?;
            } else {
                controller.choose_bound_v1(selection)?;
            }
            let revealed_changes =
                render_revealed_deck_result_v1(controller, before.as_ref(), reveals)?;
            *observation_cache = None;
            *combat_research_cache = None;
            *combat_turn_cache = None;
            combat_draft.clear();
            Ok((
                AgentShellResultV1::Chosen {
                    reading: controller.decision_reading_v1(
                        &[DecisionDetailV1::Brief],
                        known_context.as_ref(),
                        locale,
                    )?,
                    revealed_changes,
                },
                false,
            ))
        }
        AgentShellRequestV1::Hello { .. } => Ok((
            AgentShellResultV1::Hello {
                capabilities: capabilities.clone(),
            },
            false,
        )),
        AgentShellRequestV1::Observe { .. } => Ok((
            AgentShellResultV1::Observation {
                observation: cached_current_agent_observation_v1(
                    controller,
                    capabilities.clone(),
                    observation_cache,
                    locale,
                )?,
            },
            false,
        )),
        AgentShellRequestV1::GameFacts { entities, .. } => Ok((
            AgentShellResultV1::GameFacts {
                facts: agent_game_facts_v1(entities),
            },
            false,
        )),
        AgentShellRequestV1::Info {
            query,
            locale: info_locale,
            ..
        } => {
            // Refresh from the published head just as ordinary reading does;
            // queries never evaluate an action or consume game RNG.
            controller.decision_reading_v1(&[], None, locale)?;
            let decision = controller
                .head()
                .current_decision
                .as_ref()
                .map(|inventory| inventory.public_decision_v1())
                .transpose()?;
            Ok((
                AgentShellResultV1::Info {
                    view: crate::mechanics_info::render(
                        decision.as_ref(),
                        &query,
                        info_locale.unwrap_or(locale),
                    )?,
                },
                false,
            ))
        }
        AgentShellRequestV1::SlPiles { .. } => {
            if capabilities.permission_envelope.information_ceiling != "privileged_exact_allowed" {
                return Err("SL piles require privileged exact information permission".into());
            }
            let root = controller.current_combat_position_for_lab_v1()?;
            Ok((
                AgentShellResultV1::SlPiles {
                    view: render_sl_piles_v1(&root),
                },
                false,
            ))
        }
        AgentShellRequestV1::Act {
            actions, advance, ..
        } => {
            let selectors = actions
                .iter()
                .map(|action| parse_action_selector_v1(action, "agent-shell act"))
                .collect::<Result<Vec<_>, String>>()?;
            let before = controller
                .head()
                .current_decision
                .as_ref()
                .filter(|decision| {
                    decision.site != sts_oracle_learning::agent::task::TaskDecisionSiteV1::Combat
                })
                .map(|decision| decision.public_decision_v1())
                .transpose()?;
            let reveals = before.as_ref().is_some_and(|decision| {
                selectors.first().is_some_and(|selector| {
                    crate::selector::resolve_action_selector_v1(decision, selector)
                        .ok()
                        .and_then(|choice| choice.ordinal_path.first().copied())
                        .and_then(|ordinal| decision.candidates.get(ordinal))
                        .is_some_and(reveals_deck_result_v1)
                })
            });
            if selectors.len() == 1 {
                controller.choose_action_v1(
                    &selectors[0],
                    matches!(advance, AgentAdvanceModeV1::OnePublicAction),
                )?;
            } else {
                controller.anchor_action_line_v1(&selectors)?;
            }
            let revealed_changes =
                render_revealed_deck_result_v1(controller, before.as_ref(), reveals)?;
            *observation_cache = None;
            *combat_research_cache = None;
            *combat_turn_cache = None;
            combat_draft.clear();
            Ok((
                AgentShellResultV1::Acted {
                    observation: cached_current_agent_observation_v1(
                        controller,
                        capabilities.clone(),
                        observation_cache,
                        locale,
                    )?,
                    reading: controller.decision_reading_v1(
                        &[DecisionDetailV1::Brief],
                        None,
                        locale,
                    )?,
                    revealed_changes,
                },
                false,
            ))
        }
        AgentShellRequestV1::Probe { lines, .. } => {
            let selectors = lines
                .iter()
                .map(|line| {
                    line.iter()
                        .map(|action| parse_action_selector_v1(action, "agent-shell probe"))
                        .collect::<Result<Vec<_>, String>>()
                })
                .collect::<Result<Vec<_>, String>>()?;
            let root_head_revision = controller.head().head_revision;
            let inspections = controller.inspect_action_lines_v1(&selectors)?;
            let lines = inspections
                .iter()
                .map(|inspection| project_agent_inspection_v1(inspection, locale))
                .collect::<Result<Vec<_>, String>>()?;
            if controller.head().head_revision != root_head_revision {
                return Err("agent-shell probe changed the durable controller head".to_string());
            }
            Ok((
                AgentShellResultV1::Probed {
                    root_head_revision,
                    durable_head_unchanged: true,
                    lines,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatResearch {
            effort,
            max_search_ms,
            max_generation_work,
            max_potions_used,
            include_milestone_successor_previews,
            ..
        } => {
            let root_head_revision = controller.head().head_revision;
            let mut requested_options = combat_research_options_for_effort_v1(
                &controller.head().combat_plan_options,
                *effort,
                Some(*max_search_ms),
            );
            restrict_combat_research_potions_v1(&mut requested_options, *max_potions_used);
            let work_limit = (*effort == AgentCombatResearchEffortV1::WholeCombat)
                .then(|| max_generation_work.unwrap_or(requested_options.max_generation_work));
            let requested_satisfaction = requested_options.satisfaction;
            let cached = combat_research_cache.as_ref().filter(|cached| {
                cached.source == AgentCombatResearchCacheSourceV1::AutomatedResearch
                    && cached.root_head_revision == root_head_revision
                    && cached.effort == *effort
                    && cached.requested_satisfaction == requested_satisfaction
                    && cached.potion_policy == requested_options.potion_policy
                    && cached.max_potions_used == requested_options.max_potions_used
                    && cached.max_search_ms == *max_search_ms
                    && cached.max_generation_work == work_limit
            });
            let reused_cached_frontier = cached.is_some();
            let (
                lanes,
                bounded_progress,
                candidates,
                milestone_candidates,
                milestone_successor_previews,
                search_budget_exhausted,
            ) = if let Some(cached) = cached {
                (
                    cached.attempt.lanes.clone(),
                    cached.bounded_progress.clone(),
                    cached.candidates.clone(),
                    cached.projected_milestone_candidates.clone(),
                    cached.milestone_successor_previews.clone(),
                    cached.search_budget_exhausted,
                )
            } else {
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(*max_search_ms);
                let report = combat_research_attempt_for_effort_v1(
                    controller,
                    *effort,
                    *max_generation_work,
                    *max_potions_used,
                    deadline,
                )?;
                let search_budget_exhausted = report.attempt.lanes.iter().any(|lane| {
                    use TaskResearchPlanAttemptExitV1::*;
                    matches!(
                        lane.exit,
                        WitnessAtWorkLimit
                            | WitnessAtWallLimit
                            | GenerationWorkExhausted
                            | ExpandedNodeBudgetExhausted
                            | WallTimeExhausted
                            | AllowanceExhausted
                            | DeadlineReached
                            | ActionPrefixBudgetExhausted
                    )
                });
                let bounded_progress =
                    project_agent_combat_research_bounded_progress_v1(&report.bounded_progress)?;
                let candidates = project_agent_combat_research_candidates_v1(&report)?;
                let (milestone_candidates, milestone_successor_previews) =
                    project_agent_combat_milestone_frontier_v1(&report.milestone_candidates)?;
                let lanes = report.attempt.lanes.clone();
                let saved = SavedPlans {
                    producer_identity: Some(RunPlayEvidenceIdentityV1::current_v1()),
                    max_search_ms: *max_search_ms,
                    max_generation_work: work_limit,
                    max_potions_used: requested_options.max_potions_used,
                    search_budget_exhausted,
                    artifact: artifact_from_report(controller, *effort, &report)?,
                };
                let report_id = Some(saved.save(controller)?);
                *combat_research_cache = Some(AgentCombatResearchCacheV1 {
                    report_id,
                    source: AgentCombatResearchCacheSourceV1::AutomatedResearch,
                    root_head_revision,
                    effort: *effort,
                    requested_satisfaction,
                    potion_policy: requested_options.potion_policy,
                    max_potions_used: requested_options.max_potions_used,
                    max_search_ms: *max_search_ms,
                    max_generation_work: work_limit,
                    search_budget_exhausted,
                    attempt: report.attempt,
                    witness_lines: report.witness_lines,
                    bounded_progress: bounded_progress.clone(),
                    candidates: candidates.clone(),
                    milestone_candidates: report.milestone_candidates,
                    projected_milestone_candidates: milestone_candidates.clone(),
                    milestone_successor_previews: milestone_successor_previews.clone(),
                });
                (
                    lanes,
                    bounded_progress,
                    candidates,
                    milestone_candidates,
                    milestone_successor_previews,
                    search_budget_exhausted,
                )
            };
            if controller.head().head_revision != root_head_revision {
                return Err(
                    "agent-shell combat research changed the durable controller head".to_string(),
                );
            }
            let milestone_successor_preview_count = milestone_successor_previews.len();
            let returned_milestone_successor_previews = if *include_milestone_successor_previews {
                milestone_successor_previews
            } else {
                Vec::new()
            };
            let decision = controller
                .head()
                .current_decision
                .as_ref()
                .ok_or("combat research requires a current decision")?
                .public_decision_v1()?;
            let TaskPublicObservationV1::Combat {
                observation: root, ..
            } = &decision.observation
            else {
                return Err("combat research requires a combat decision".into());
            };
            let report_id = combat_research_cache
                .as_ref()
                .and_then(|c| c.report_id.clone())
                .ok_or("research report was not saved")?;
            Ok((
                AgentShellResultV1::CombatResearch {
                    report_id: report_id.clone(),
                    view: {
                        let mut view = crate::combat_reading::research(
                            &report_id,
                            root,
                            &candidates,
                            &bounded_progress,
                            Some(*max_search_ms),
                            &lanes,
                            requested_satisfaction,
                            work_limit,
                            false,
                            locale,
                        );
                        if reused_cached_frontier {
                            view.push_str(&crate::combat_reading::research_message(
                                locale,
                                "cached",
                                &[],
                            ));
                            view.push('\n');
                        }
                        view
                    },
                    root_head_revision,
                    max_search_ms: *max_search_ms,
                    max_generation_work: work_limit,
                    search_budget_exhausted,
                    durable_head_unchanged: true,
                    max_potions_used: requested_options.max_potions_used,
                    effort: *effort,
                    requested_satisfaction,
                    reused_cached_frontier,
                    lanes,
                    bounded_progress,
                    candidates: project_live_combat_research_candidates_v1(&candidates, &report_id),
                    milestone_candidates,
                    milestone_successor_preview_count,
                    milestone_successor_previews_included: *include_milestone_successor_previews,
                    milestone_successor_previews: returned_milestone_successor_previews,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatPlans { diagnostics, .. } => {
            let saved = SavedPlans::load(controller, None)?;
            let report_id = saved.id()?;
            Ok((
                AgentShellResultV1::CombatPlans {
                    report_id: report_id.clone(),
                    effort: saved.artifact.effort,
                    view: saved.view(controller, *diagnostics, locale)?,
                    candidates: project_live_combat_research_candidates_v1(
                        &saved.artifact.candidates,
                        &report_id,
                    ),
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatReview {
            candidate_refs,
            report_id,
            effort,
            include_steps,
            turn,
            compact,
            ..
        } => {
            if let Some(id) = report_id {
                let saved = SavedPlans::load(controller, Some(id))?;
                if saved.artifact.effort != *effort {
                    return Err("saved report effort mismatch".into());
                }
                let indices = candidate_refs
                    .iter()
                    .map(|r| {
                        saved.candidate(r).and_then(|c| {
                            c.report_candidate_index
                                .ok_or_else(|| "saved candidate lacks report index".to_string())
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let mut trace = crate::combat_trace::trace_combat_research_artifact_v1(
                    controller.paths().clone(),
                    &saved.artifact,
                    &indices,
                )?;
                let display_refs = candidate_refs
                    .iter()
                    .map(|reference| crate::combat_reading::plan_id(id, reference))
                    .collect::<Vec<_>>();
                let view =
                    crate::combat_reading::review(&trace, &display_refs, *turn, *compact, locale)?;
                trace.set_steps_included_v1(*include_steps);
                return Ok((
                    AgentShellResultV1::CombatReview {
                        view,
                        root_head_revision: controller.head().head_revision,
                        durable_head_unchanged: true,
                        effort: *effort,
                        candidate_refs: candidate_refs.clone(),
                        trace,
                    },
                    false,
                ));
            }
            let root_head_revision = controller.head().head_revision;
            let cached = combat_research_cache.as_ref().ok_or_else(|| {
                "agent-shell combat_review requires combat_research at the current head".to_string()
            })?;
            if cached.root_head_revision != root_head_revision || cached.effort != *effort {
                return Err(
                    "agent-shell combat_review does not match the cached head and effort"
                        .to_string(),
                );
            }
            let candidate_ids = resolve_cached_combat_research_refs_v1(cached, candidate_refs)?;
            let mut trace = trace_cached_combat_research_v1(
                controller,
                &cached.attempt,
                &cached.witness_lines,
                &candidate_ids,
                format!("agent-shell:{effort:?}"),
            )?;
            let id = cached
                .report_id
                .as_deref()
                .ok_or("research report was not saved")?;
            let display_refs = candidate_refs
                .iter()
                .map(|reference| crate::combat_reading::plan_id(id, reference))
                .collect::<Vec<_>>();
            let view =
                crate::combat_reading::review(&trace, &display_refs, *turn, *compact, locale)?;
            trace.set_steps_included_v1(*include_steps);
            if controller.head().head_revision != root_head_revision {
                return Err("agent-shell combat review changed the durable head".to_string());
            }
            Ok((
                AgentShellResultV1::CombatReview {
                    view,
                    root_head_revision,
                    durable_head_unchanged: true,
                    effort: *effort,
                    candidate_refs: candidate_refs.clone(),
                    trace,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatMilestoneSuccessorDetails {
            candidate_ids,
            effort,
            ..
        } => {
            let root_head_revision = controller.head().head_revision;
            let cached = combat_research_cache.as_ref().ok_or_else(|| {
                "agent-shell combat_milestone_successor_details requires combat_research at the current head"
                    .to_string()
            })?;
            if cached.root_head_revision != root_head_revision || cached.effort != *effort {
                return Err(
                    "agent-shell combat_milestone_successor_details does not match the cached head and effort"
                        .to_string(),
                );
            }
            let details = project_agent_combat_milestone_successor_details_v1(
                &cached.milestone_candidates,
                candidate_ids,
            )?;
            if controller.head().head_revision != root_head_revision {
                return Err(
                    "agent-shell combat milestone successor details changed the durable controller head"
                        .to_string(),
                );
            }
            Ok((
                AgentShellResultV1::CombatMilestoneSuccessorDetails {
                    root_head_revision,
                    durable_head_unchanged: true,
                    effort: *effort,
                    details,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatMilestoneContinuationAudit {
            candidate_ids,
            effort,
            ..
        } => {
            let root_head_revision = controller.head().head_revision;
            let cached = combat_research_cache.as_ref().ok_or_else(|| {
                "agent-shell combat_milestone_continuation_audit requires combat_research at the current head"
                    .to_string()
            })?;
            if cached.root_head_revision != root_head_revision || cached.effort != *effort {
                return Err(
                    "agent-shell combat_milestone_continuation_audit does not match the cached head and effort"
                        .to_string(),
                );
            }
            let audits = controller.audit_cached_combat_milestone_continuations_v1(
                root_head_revision,
                &cached.milestone_candidates,
                candidate_ids,
                ExactCompleteTurnProposalConfigV1::default(),
            )?;
            if controller.head().head_revision != root_head_revision {
                return Err(
                    "agent-shell combat milestone continuation audit changed the durable controller head"
                        .to_string(),
                );
            }
            Ok((
                AgentShellResultV1::CombatMilestoneContinuationAudit {
                    root_head_revision,
                    durable_head_unchanged: true,
                    effort: *effort,
                    audits,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatPrefixHorizonAudit {
            root_head_revision,
            lineage_id,
            prefix_actions,
            expected_successor_public_snapshot_id,
            additional_turns,
            ..
        } => {
            let current_head_revision = controller.head().head_revision;
            if *root_head_revision != current_head_revision {
                return Err(format!(
                    "agent-shell combat_prefix_horizon_audit belongs to head {root_head_revision}, current head is {current_head_revision}"
                ));
            }
            let selectors = prefix_actions
                .iter()
                .map(|action| {
                    parse_action_selector_v1(action, "agent-shell combat_prefix_horizon_audit")
                })
                .collect::<Result<Vec<_>, String>>()?;
            let audit = controller.audit_combat_prefix_horizon_v1(
                *root_head_revision,
                lineage_id,
                &selectors,
                expected_successor_public_snapshot_id,
                usize::from(*additional_turns),
                ExactCompleteTurnProposalConfigV1::default(),
            )?;
            if controller.head().head_revision != current_head_revision {
                return Err(
                    "agent-shell combat prefix horizon audit changed the durable controller head"
                        .to_string(),
                );
            }
            Ok((
                AgentShellResultV1::CombatPrefixHorizonAudit {
                    root_head_revision: current_head_revision,
                    durable_head_unchanged: true,
                    audit,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatDraftOpen {
            candidate_ref,
            effort,
            turn,
            replace,
            ..
        } => {
            let cached = combat_research_cache
                .as_ref()
                .ok_or("combat draft opening requires current combat research")?;
            if cached.root_head_revision != controller.head().head_revision
                || cached.effort != *effort
            {
                return Err(
                    "combat draft opening does not match the cached head and effort".into(),
                );
            }
            let selected = resolve_cached_combat_research_ref_v1(cached, candidate_ref)?;
            let candidate = cached
                .attempt
                .candidates
                .iter()
                .find(|candidate| {
                    crate::driver::combat_research_candidate_id_v1(candidate)
                        .ok()
                        .as_ref()
                        == Some(&selected.candidate_id)
                })
                .ok_or("combat draft source is missing from the cached report")?;
            let response = combat_draft.open_candidate(controller, candidate, *turn, *replace)?;
            if cached.source == AgentCombatResearchCacheSourceV1::AnalystDraft {
                *combat_research_cache = None;
            }
            Ok((AgentShellResultV1::CombatDraftStatus { response }, false))
        }
        AgentShellRequestV1::CombatDraftStatus { .. } => {
            let commit_ready = combat_research_cache.as_ref().is_some_and(|cached| {
                cached.source == AgentCombatResearchCacheSourceV1::AnalystDraft
                    && cached.root_head_revision == controller.head().head_revision
            });
            Ok((
                AgentShellResultV1::CombatDraftStatus {
                    response: combat_draft.status(controller, commit_ready)?,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatDraftAdvance {
            generation_work,
            engine_steps,
            ..
        } => Ok((
            AgentShellResultV1::CombatDraftAdvanced {
                response: combat_draft.advance(controller, *generation_work, *engine_steps)?,
            },
            false,
        )),
        AgentShellRequestV1::CombatDraftInspect {
            frame_ref,
            option_ref,
            ..
        } => Ok((
            AgentShellResultV1::CombatDraftInspected {
                response: combat_draft.inspect(controller, frame_ref, option_ref)?,
            },
            false,
        )),
        AgentShellRequestV1::CombatDraftChoose {
            frame_ref,
            option_ref,
            ..
        } => {
            let choice = combat_draft.choose(controller, frame_ref, option_ref)?;
            if let Some(attempt) = choice.commit_attempt {
                *combat_research_cache = Some(combat_research_cache_from_draft_attempt_v1(
                    choice.response.root_head_revision,
                    attempt,
                )?);
            }
            Ok((
                AgentShellResultV1::CombatDraftChosen {
                    response: choice.response,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatDraftUndo { .. } => {
            let response = combat_draft.undo(controller)?;
            if combat_research_cache.as_ref().is_some_and(|cached| {
                cached.source == AgentCombatResearchCacheSourceV1::AnalystDraft
            }) {
                *combat_research_cache = None;
            }
            Ok((AgentShellResultV1::CombatDraftUndone { response }, false))
        }
        AgentShellRequestV1::CombatDraftSelection {
            edit,
            source_ref,
            offset,
            ..
        } => Ok((
            AgentShellResultV1::CombatDraftSelection {
                response: combat_draft.selection(
                    controller,
                    edit.as_ref(),
                    source_ref.as_deref(),
                    *offset,
                )?,
            },
            false,
        )),
        AgentShellRequestV1::CommitCombatPlan {
            candidate_ref,
            report_id,
            effort,
            ..
        } => {
            if let Some(id) = report_id {
                let saved = SavedPlans::load(controller, Some(id))?;
                if saved.artifact.effort != *effort {
                    return Err("saved report effort mismatch".into());
                }
                let candidate = saved.candidate(candidate_ref)?;
                let candidate_id = candidate.candidate_id.clone();
                let producer = &candidate
                    .supporting_provenances
                    .first()
                    .ok_or("saved candidate lacks a producer")?
                    .producer_name;
                anchor_combat_research_artifact_candidate_v1(
                    controller,
                    &saved.artifact,
                    producer,
                    candidate.report_candidate_index,
                )?;
                *observation_cache = None;
                *combat_research_cache = None;
                *combat_turn_cache = None;
                combat_draft.clear();
                return Ok((
                    AgentShellResultV1::CombatPlanCommitted {
                        candidate_ref: candidate_ref.clone(),
                        candidate_id,
                        observation: cached_current_agent_observation_v1(
                            controller,
                            capabilities.clone(),
                            observation_cache,
                            locale,
                        )?,
                        reading: controller.decision_reading_v1(
                            &[DecisionDetailV1::Brief],
                            None,
                            locale,
                        )?,
                    },
                    false,
                ));
            }
            let cached = combat_research_cache.as_ref().ok_or_else(|| {
                "agent-shell commit_combat_plan requires combat_research at the current head"
                    .to_string()
            })?;
            if cached.root_head_revision != controller.head().head_revision
                || cached.effort != *effort
            {
                return Err(
                    "agent-shell commit_combat_plan does not match the cached head and effort"
                        .to_string(),
                );
            }
            let candidate_id = resolve_cached_combat_research_ref_v1(cached, candidate_ref)?
                .candidate_id
                .clone();
            controller.anchor_cached_combat_research_candidate_id_v1(
                cached.root_head_revision,
                &cached.attempt,
                &candidate_id,
            )?;
            *observation_cache = None;
            *combat_research_cache = None;
            *combat_turn_cache = None;
            combat_draft.clear();
            Ok((
                AgentShellResultV1::CombatPlanCommitted {
                    candidate_ref: candidate_ref.clone(),
                    candidate_id,
                    observation: cached_current_agent_observation_v1(
                        controller,
                        capabilities.clone(),
                        observation_cache,
                        locale,
                    )?,
                    reading: controller.decision_reading_v1(
                        &[DecisionDetailV1::Brief],
                        None,
                        locale,
                    )?,
                },
                false,
            ))
        }
        AgentShellRequestV1::CommitCombatMilestone {
            candidate_id,
            effort,
            ..
        } => {
            let cached = combat_research_cache.as_ref().ok_or_else(|| {
                "agent-shell commit_combat_milestone requires combat_research at the current head"
                    .to_string()
            })?;
            if cached.root_head_revision != controller.head().head_revision
                || cached.effort != *effort
            {
                return Err(
                    "agent-shell commit_combat_milestone does not match the cached head and effort"
                        .to_string(),
                );
            }
            controller.anchor_cached_combat_milestone_candidate_id_v1(
                cached.root_head_revision,
                &cached.milestone_candidates,
                candidate_id,
            )?;
            *observation_cache = None;
            *combat_research_cache = None;
            *combat_turn_cache = None;
            combat_draft.clear();
            Ok((
                AgentShellResultV1::CombatMilestoneCommitted {
                    candidate_id: candidate_id.clone(),
                    observation: cached_current_agent_observation_v1(
                        controller,
                        capabilities.clone(),
                        observation_cache,
                        locale,
                    )?,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatTurnResearch { .. } => {
            let root_head_revision = controller.head().head_revision;
            let reused_cached_frontier = combat_turn_cache
                .as_ref()
                .is_some_and(|cached| cached.root_head_revision == root_head_revision);
            if !reused_cached_frontier {
                *combat_turn_cache = Some(AgentCombatTurnCacheV1 {
                    root_head_revision,
                    attempt: controller.combat_turn_attempt_v1(
                        sts_complete_turn_proposal::ExactCompleteTurnProposalConfigV1::default(),
                    )?,
                });
            }
            let cached = combat_turn_cache
                .as_ref()
                .expect("combat turn cache installed above");
            if controller.head().head_revision != root_head_revision {
                return Err(
                    "agent-shell combat turn research changed the durable controller head"
                        .to_string(),
                );
            }
            Ok((
                AgentShellResultV1::CombatTurnResearch {
                    root_head_revision,
                    durable_head_unchanged: true,
                    reused_cached_frontier,
                    nodes_expanded: cached.attempt.nodes_expanded,
                    nodes_generated: cached.attempt.nodes_generated,
                    engine_steps: cached.attempt.engine_steps,
                    exact_state_skips: cached.attempt.exact_state_skips,
                    truncated_children: cached.attempt.truncated_children,
                    preselection_candidate_count: cached.attempt.preselection_candidate_count,
                    candidates: project_agent_combat_turn_candidate_summaries_v1(
                        &cached.attempt.candidates,
                    )?,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatTurnReserve { line, .. } => {
            let root_head_revision = controller.head().head_revision;
            let cached = combat_turn_cache.as_mut().ok_or_else(|| {
                "agent-shell combat_turn_reserve requires combat_turn_research at the current head"
                    .to_string()
            })?;
            if cached.root_head_revision != root_head_revision {
                return Err(
                    "agent-shell combat-turn reservation does not match the cached head"
                        .to_string(),
                );
            }
            let selectors = line
                .iter()
                .map(|action| parse_action_selector_v1(action, "agent-shell combat_turn_reserve"))
                .collect::<Result<Vec<_>, String>>()?;
            let reservation = controller.reserve_cached_combat_turn_line_v1(
                root_head_revision,
                &mut cached.attempt,
                &selectors,
                ExactCompleteTurnProposalConfigV1::default(),
            )?;
            let candidate = project_agent_combat_turn_candidate_summaries_v1(
                std::slice::from_ref(&reservation.candidate),
            )?
            .pop()
            .expect("one reserved candidate projects to one summary");
            if controller.head().head_revision != root_head_revision {
                return Err(
                    "agent-shell combat-turn reservation changed the durable controller head"
                        .to_string(),
                );
            }
            Ok((
                AgentShellResultV1::CombatTurnReserved {
                    root_head_revision,
                    durable_head_unchanged: true,
                    already_present: reservation.already_present,
                    candidate_count: reservation.candidate_count,
                    candidate,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatTurnCandidateDetails { candidate_ids, .. } => {
            let cached = combat_turn_cache.as_ref().ok_or_else(|| {
                "agent-shell combat_turn_candidate_details requires combat_turn_research at the current head"
                    .to_string()
            })?;
            let root_head_revision = controller.head().head_revision;
            if cached.root_head_revision != root_head_revision {
                return Err(
                    "agent-shell combat-turn candidate details do not match the cached head"
                        .to_string(),
                );
            }
            let details = project_agent_combat_turn_candidate_details_v1(
                &cached.attempt.candidates,
                candidate_ids,
            )?;
            if controller.head().head_revision != root_head_revision {
                return Err(
                    "agent-shell combat-turn candidate details changed the durable controller head"
                        .to_string(),
                );
            }
            Ok((
                AgentShellResultV1::CombatTurnCandidateDetails {
                    root_head_revision,
                    durable_head_unchanged: true,
                    details,
                },
                false,
            ))
        }
        AgentShellRequestV1::CombatTurnContinuationAudit {
            candidate_ids,
            effort,
            ..
        } => {
            let cached = combat_turn_cache.as_ref().ok_or_else(|| {
                "agent-shell combat_turn_continuation_audit requires combat_turn_research at the current head"
                    .to_string()
            })?;
            let root_head_revision = controller.head().head_revision;
            if cached.root_head_revision != root_head_revision {
                return Err(
                    "agent-shell combat-turn continuation audit does not match the cached head"
                        .to_string(),
                );
            }
            let options = combat_turn_continuation_audit_options_for_effort_v1(
                &controller.head().combat_plan_options,
                *effort,
            );
            let batch = controller.audit_cached_combat_turn_continuations_v1(
                root_head_revision,
                &cached.attempt,
                candidate_ids,
                options,
            )?;
            let mut observed_cross_surface_frontier = Vec::new();
            for observed in &batch.observed_terminal_frontier {
                retain_agent_combat_observed_terminal_outcome_v1(
                    &mut observed_cross_surface_frontier,
                    AgentCombatObservedTerminalOutcomeV1 {
                        action: format!("combat-turn:{}", observed.candidate_id),
                        source: AgentCombatObservedTerminalSourceV1::CompleteTurnContinuation,
                        outcome: observed.public_outcome.clone(),
                    },
                );
            }
            let cached_plans = combat_research_cache
                .as_ref()
                .filter(|plans| plans.root_head_revision == root_head_revision);
            if let Some(plans) = cached_plans {
                for candidate in &plans.candidates {
                    retain_agent_combat_observed_terminal_outcome_v1(
                        &mut observed_cross_surface_frontier,
                        AgentCombatObservedTerminalOutcomeV1 {
                            action: format!("combat-plan:{}", candidate.candidate_id),
                            source: AgentCombatObservedTerminalSourceV1::CompleteCombatPlan,
                            outcome: candidate.outcome.clone(),
                        },
                    );
                }
            }
            if controller.head().head_revision != root_head_revision {
                return Err(
                    "agent-shell combat-turn continuation audit changed the durable controller head"
                        .to_string(),
                );
            }
            Ok((
                AgentShellResultV1::CombatTurnContinuationAudit {
                    root_head_revision,
                    durable_head_unchanged: true,
                    effort: *effort,
                    audits: batch.audits,
                    observed_terminal_frontier: batch.observed_terminal_frontier,
                    observed_cross_surface_frontier,
                    cross_surface_frontier_includes_cached_combat_plans: cached_plans
                        .is_some_and(|plans| !plans.candidates.is_empty()),
                    observed_witnesses_only: batch.observed_witnesses_only,
                    absence_means_unknown: batch.absence_means_unknown,
                },
                false,
            ))
        }
        AgentShellRequestV1::CommitCombatTurn { candidate_id, .. } => {
            let cached = combat_turn_cache.as_ref().ok_or_else(|| {
                "agent-shell commit_combat_turn requires combat_turn_research at the current head"
                    .to_string()
            })?;
            if cached.root_head_revision != controller.head().head_revision {
                return Err(
                    "agent-shell commit_combat_turn does not match the cached head".to_string(),
                );
            }
            controller.anchor_cached_combat_turn_candidate_id_v1(
                cached.root_head_revision,
                &cached.attempt,
                candidate_id,
            )?;
            *observation_cache = None;
            *combat_research_cache = None;
            *combat_turn_cache = None;
            combat_draft.clear();
            Ok((
                AgentShellResultV1::CombatTurnCommitted {
                    candidate_id: candidate_id.clone(),
                    observation: cached_current_agent_observation_v1(
                        controller,
                        capabilities.clone(),
                        observation_cache,
                        locale,
                    )?,
                },
                false,
            ))
        }
        AgentShellRequestV1::Quit { .. } => Ok((AgentShellResultV1::Quitting, true)),
    }
}

fn run_agent_jsonl_transport_v1(
    input: impl BufRead,
    mut output: impl Write,
    mut handle_valid_request: impl FnMut(
        &AgentShellRequestV1,
    ) -> Result<(AgentShellResultV1, bool), String>,
) -> Result<(), String> {
    for line in input.lines() {
        let started = Instant::now();
        let line = line.map_err(|error| format!("failed to read agent request: {error}"))?;
        let parsed = serde_json::from_str::<AgentShellRequestV1>(&line);
        let (response, should_quit) = match parsed {
            Err(error) => (
                error_response_v1(None, started, "invalid_json", error.to_string()),
                false,
            ),
            Ok(request) => match request.validate() {
                Err(error) => (
                    error_response_v1(
                        Some(request.request_id().to_string()),
                        started,
                        "invalid_request",
                        error,
                    ),
                    false,
                ),
                Ok(()) => match handle_valid_request(&request) {
                    Ok((result, should_quit)) => (
                        success_response_v1(request.request_id().to_string(), started, result),
                        should_quit,
                    ),
                    Err(error) => (
                        error_response_v1(
                            Some(request.request_id().to_string()),
                            started,
                            "command_failed",
                            error,
                        ),
                        false,
                    ),
                },
            },
        };
        serde_json::to_writer(&mut output, &response)
            .map_err(|error| format!("failed to encode agent response: {error}"))?;
        output
            .write_all(b"\n")
            .map_err(|error| format!("failed to terminate agent response: {error}"))?;
        output
            .flush()
            .map_err(|error| format!("failed to flush agent response: {error}"))?;
        if should_quit {
            break;
        }
    }
    Ok(())
}

/// Runs a machine-only JSONL transport over one already-open hot controller.
/// Every input line produces exactly one stdout JSON line. There is no prompt,
/// banner, display rendering, or timing side channel mixed into the stream.
pub fn run_agent_jsonl_v1(
    controller: &mut RunPlayControllerV1,
    capabilities: AgentCapabilitiesV1,
    input: impl BufRead,
    output: impl Write,
    locale: Locale,
) -> Result<(), String> {
    let mut observation_cache = None;
    let mut combat_research_cache = None;
    let mut combat_turn_cache = None;
    let mut combat_draft = AgentCombatDraftRuntimeV1::default();
    run_agent_jsonl_transport_v1(input, output, |request| {
        handle_valid_request_v1(
            controller,
            &capabilities,
            &mut observation_cache,
            &mut combat_research_cache,
            &mut combat_turn_cache,
            &mut combat_draft,
            request,
            locale,
        )
    })
}

/// Serves an already-terminal durable head without trying to reconstruct an
/// exact decision session. Terminal heads support capability discovery,
/// observation, and clean shutdown; mutation and research commands fail
/// explicitly because there is no public decision to act on.
pub fn run_terminal_agent_jsonl_v1(
    head: &RunPlayHeadV1,
    capabilities: AgentCapabilitiesV1,
    input: impl BufRead,
    output: impl Write,
    locale: Locale,
) -> Result<(), String> {
    if !matches!(head.position, RunPlayPositionV1::Terminal { .. })
        || head.current_decision.is_some()
    {
        return Err("terminal agent transport requires a terminal head".to_string());
    }
    let observation = project_agent_observation_v1(head, capabilities.clone(), None, locale)?;
    run_agent_jsonl_transport_v1(input, output, |request| match request {
        AgentShellRequestV1::Decision {
            details,
            known_context,
            ..
        } => Ok((
            AgentShellResultV1::Decision {
                reading: crate::decision::read_decision(
                    head,
                    details,
                    known_context.as_ref(),
                    locale,
                )?,
            },
            false,
        )),
        AgentShellRequestV1::Hello { .. } => Ok((
            AgentShellResultV1::Hello {
                capabilities: capabilities.clone(),
            },
            false,
        )),
        AgentShellRequestV1::Observe { .. } => Ok((
            AgentShellResultV1::Observation {
                observation: observation.clone(),
            },
            false,
        )),
        AgentShellRequestV1::GameFacts { entities, .. } => Ok((
            AgentShellResultV1::GameFacts {
                facts: agent_game_facts_v1(entities),
            },
            false,
        )),
        AgentShellRequestV1::Info {
            query,
            locale: info_locale,
            ..
        } => Ok((
            AgentShellResultV1::Info {
                view: crate::mechanics_info::render(None, &query, info_locale.unwrap_or(locale))?,
            },
            false,
        )),
        AgentShellRequestV1::Quit { .. } => Ok((AgentShellResultV1::Quitting, true)),
        _ => Err("agent-shell command requires a decision head".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_locale_reaches_native_queries_and_source_bound_observation_without_state_changes()
    {
        use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
        for locale in [Locale::En, Locale::ZhCn] {
            let directory = TestDirectory::new();
            let mut controller =
                controller_with_session(&directory.0, "locale-transport", &fixture("reward"));
            let before = controller.head().clone();
            let input = [
                serde_json::json!({"schema":AGENT_REQUEST_SCHEMA_V1,"request_id":"observe","command":"observe"}),
                serde_json::json!({"schema":AGENT_REQUEST_SCHEMA_V1,"request_id":"info","command":"info","query":"card Bash"}),
                serde_json::json!({"schema":AGENT_REQUEST_SCHEMA_V1,"request_id":"decision","command":"decision"}),
                serde_json::json!({"schema":AGENT_REQUEST_SCHEMA_V1,"request_id":"quit","command":"quit"}),
            ].iter().map(|request| serde_json::to_string(request).unwrap()).collect::<Vec<_>>().join("\n") + "\n";
            let mut output = Vec::new();
            run_agent_jsonl_v1(
                &mut controller,
                AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::public_manual_v1(None)),
                std::io::Cursor::new(input.as_bytes()),
                &mut output,
                locale,
            )
            .unwrap();
            let replies = String::from_utf8(output)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(replies.len(), 4);
            assert!(
                replies.iter().all(|reply| reply["status"] == "ok"),
                "{replies:?}"
            );
            let inventory = replies[0]["result"]["observation"]["preparation_reading"]["inventory"]
                ["view"]
                .as_str()
                .unwrap();
            assert!(inventory.starts_with(if locale == Locale::En {
                "Deck ("
            } else {
                "永久卡组（"
            }));
            let query = replies[1]["result"]["view"].as_str().unwrap();
            assert!(query.contains(locale.entity("card", "Bash")));
            assert_eq!(
                replies[2]["result"]["reading"]["source"]["public_snapshot_id"],
                before.current_decision.as_ref().unwrap().public_snapshot_id
            );
            assert_eq!(*controller.head(), before);
        }
    }

    #[test]
    fn opening_cursed_key_chests_reveals_added_curses_through_choose_and_act() {
        use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
        use sts_oracle_learning::content::relics::{RelicState, RelicTier};
        use sts_oracle_learning::state::core::EngineState;
        use sts_oracle_learning::state::rewards::{TreasureChestSize, TreasureChestState};

        for use_act in [false, true] {
            let dir = TestDirectory::new();
            let mut session = fixture("route");
            session
                .run_state
                .relics
                .push(RelicState::new(RelicId::CursedKey));
            session.engine_state = EngineState::TreasureRoom(TreasureChestState {
                size: TreasureChestSize::Small,
                base_relic_tier: RelicTier::Common,
                gold_reward_base_amount: Some(25),
            });
            let mut controller = controller_with_session(&dir.0, "cursed-chest", &session);
            let reading = controller
                .decision_reading_v1(&[DecisionDetailV1::Brief], None, Locale::En)
                .unwrap();
            let request = if use_act {
                AgentShellRequestV1::Act {
                    schema: AGENT_REQUEST_SCHEMA_V1.into(),
                    request_id: "open".into(),
                    actions: vec!["open-chest".into()],
                    advance: AgentAdvanceModeV1::OnePublicAction,
                }
            } else {
                let option = reading
                    .options
                    .iter()
                    .find(|option| {
                        matches!(
                            option.candidate,
                            TaskPublicCandidateV1::Strategic {
                                action: TaskStrategicActionV1::OpenChest
                            }
                        )
                    })
                    .unwrap();
                AgentShellRequestV1::Choose {
                    schema: AGENT_REQUEST_SCHEMA_V1.into(),
                    request_id: "open".into(),
                    selection: crate::decision::RunPlayBoundChoiceV1 {
                        source: reading.source.unwrap(),
                        choice: option.choice.clone(),
                    },
                    following_rewards: None,
                    known_context: None,
                }
            };
            let capabilities = AgentCapabilitiesV1::new(
                AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None),
            );
            let (result, _) = handle_valid_request_v1(
                &mut controller,
                &capabilities,
                &mut None,
                &mut None,
                &mut None,
                &mut AgentCombatDraftRuntimeV1::default(),
                &request,
                Locale::En,
            )
            .unwrap();
            let result = serde_json::to_value(result).unwrap();
            let changes = result["revealed_changes"].as_str().unwrap();
            assert!(changes.starts_with("Deck change: +"), "{changes}");
            assert!(
                !changes.contains('-'),
                "opening only adds the curse: {changes}"
            );
        }
    }

    #[test]
    fn random_deck_results_reveal_transforms_and_relic_upgrades_through_choose_and_act() {
        use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
        for (random_upgrade, use_act, event_transform) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (false, false, true),
            (false, true, true),
        ] {
            let dir = TestDirectory::new();
            let mut session = fixture(if random_upgrade { "shop" } else { "astrolabe" });
            if event_transform {
                use sts_oracle_learning::state::events::{EventId, EventState};
                session.engine_state = sts_oracle_learning::state::core::EngineState::EventRoom;
                session.run_state.event_state = Some(EventState::new(EventId::LivingWall));
                sts_oracle_learning::content::events::living_wall::handle_choice(
                    &mut session.engine_state,
                    &mut session.run_state,
                    1,
                );
            }
            if random_upgrade {
                let sts_oracle_learning::state::core::EngineState::Shop(shop) =
                    &mut session.engine_state
                else {
                    panic!("shop fixture");
                };
                shop.relics[0].relic_id = RelicId::WarPaint;
            }
            let mut controller = controller_with_session(&dir.0, "random-deck-result", &session);
            let reading = controller
                .decision_reading_v1(&[DecisionDetailV1::Brief], None, Locale::En)
                .unwrap();
            let request = if use_act {
                AgentShellRequestV1::Act {
                    schema: AGENT_REQUEST_SCHEMA_V1.into(),
                    request_id: "transform".into(),
                    actions: vec![if event_transform {
                        "select:Strike#0".into()
                    } else {
                        "select:Strike#0,Defend#0,Strike#1".into()
                    }],
                    advance: AgentAdvanceModeV1::OnePublicAction,
                }
            } else {
                let choice = if event_transform {
                    reading
                        .options
                        .iter()
                        .find(|option| {
                            matches!(
                                &option.candidate,
                                TaskPublicCandidateV1::Strategic {
                                    action: TaskStrategicActionV1::SubmitRunSelection {
                                        selected_card_ordinals, ..
                                    }
                                } if selected_card_ordinals == &[1]
                            )
                        })
                        .unwrap()
                        .choice
                        .clone()
                } else if random_upgrade {
                    reading
                        .options
                        .iter()
                        .find(|option| {
                            matches!(
                                option.candidate,
                                TaskPublicCandidateV1::Strategic {
                                    action: TaskStrategicActionV1::BuyRelic {
                                        relic: RelicId::WarPaint,
                                        ..
                                    }
                                }
                            )
                        })
                        .unwrap()
                        .choice
                        .clone()
                } else {
                    let mut choice = reading.selections[0].choice.clone();
                    choice.ordinal_path.extend([0, 1, 2]);
                    choice
                };
                AgentShellRequestV1::Choose {
                    schema: AGENT_REQUEST_SCHEMA_V1.into(),
                    request_id: "transform".into(),
                    selection: crate::decision::RunPlayBoundChoiceV1 {
                        source: reading.source.unwrap(),
                        choice,
                    },
                    following_rewards: None,
                    known_context: None,
                }
            };
            let capabilities = AgentCapabilitiesV1::new(
                AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None),
            );
            let (result, _) = handle_valid_request_v1(
                &mut controller,
                &capabilities,
                &mut None,
                &mut None,
                &mut None,
                &mut AgentCombatDraftRuntimeV1::default(),
                &request,
                Locale::En,
            )
            .unwrap();
            let result = serde_json::to_value(result).unwrap();
            let changes = result["revealed_changes"].as_str().unwrap();
            assert!(changes.starts_with("Deck change:"), "{changes}");
            if random_upgrade {
                assert!(changes.contains("-2× Defend"), "{changes}");
                assert!(changes.contains("+2× Defend+"), "{changes}");
            } else if event_transform {
                assert!(changes.contains("-Strike"), "{changes}");
            } else {
                assert!(changes.contains("-2× Strike"), "{changes}");
                assert!(changes.contains("-Defend"), "{changes}");
            }
            assert!(changes.contains('+'), "{changes}");
            assert!(!changes.contains("Relic gained"), "{changes}");
            let current = controller
                .decision_reading_v1(&[DecisionDetailV1::Brief], None, Locale::En)
                .unwrap();
            let ordinary =
                serde_json::to_value(AgentShellResultV1::Decision { reading: current }).unwrap();
            assert!(ordinary.get("revealed_changes").is_none());
        }
    }

    #[test]
    fn sl_piles_preserve_order_duplicates_and_upgrades_without_drawing() {
        use sts_oracle_learning::runtime::combat::CombatCard;
        let mut combat = sts_oracle_learning::test_support::blank_test_combat();
        let strike = CombatCard::for_scenario(CardId::Strike, 1);
        let mut shrug = CombatCard::for_scenario(CardId::ShrugItOff, 2);
        shrug.upgrades = 1;
        combat.zones.draw_pile = vec![shrug, strike.clone(), strike.clone()].into();
        combat.zones.discard_pile =
            vec![strike.clone(), CombatCard::for_scenario(CardId::Defend, 3)].into();
        combat.zones.exhaust_pile = vec![CombatCard::for_scenario(CardId::Bash, 4)].into();
        let root = sts_oracle_learning::sim::combat::CombatPosition::new(
            sts_oracle_learning::state::core::EngineState::CombatPlayerTurn,
            combat,
        );
        let before = root.clone();
        let view = render_sl_piles_v1(&root);
        assert!(
            view.contains("Draw (next -> last) [3]: 1: Shrug It Off+ | 2: Strike | 3: Strike"),
            "{view}"
        );
        assert!(view
            .contains("Discard (bottom -> top; shuffle input order) [2]: 1: Strike | 2: Defend"));
        assert!(view.contains("Exhaust (bottom -> top) [1]: 1: Bash"));
        assert_eq!(root, before);
    }

    #[test]
    fn state_interrupting_relic_actions_are_typed_as_opening_a_followup_decision() {
        for relic in [
            RelicId::BottledFlame,
            RelicId::BottledLightning,
            RelicId::BottledTornado,
        ] {
            let candidate = TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ClaimReward {
                    reward_item_index: 0,
                    reward: PlannerRewardDescriptor::Relic { relic },
                },
            };
            assert_eq!(
                agent_action_role_v1(&candidate),
                AgentActionRoleV1::OpensDecision
            );
        }
        for relic in [
            RelicId::BottledFlame,
            RelicId::BottledLightning,
            RelicId::BottledTornado,
            RelicId::Cauldron,
            RelicId::DollysMirror,
            RelicId::Orrery,
        ] {
            let candidate = TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::BuyRelic {
                    shop_slot: 0,
                    relic,
                    price: 150,
                },
            };
            assert_eq!(
                agent_action_role_v1(&candidate),
                AgentActionRoleV1::OpensDecision
            );
        }
        for relic in [RelicId::Astrolabe, RelicId::CallingBell, RelicId::EmptyCage] {
            let candidate = TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::TakeBossRelic {
                    option_index: 0,
                    relic,
                },
            };
            assert_eq!(
                agent_action_role_v1(&candidate),
                AgentActionRoleV1::OpensDecision
            );
        }
    }

    #[test]
    fn structured_event_transitions_are_typed_as_opening_followup_decisions() {
        use sts_oracle_learning::ai::planner_core::{
            PlannerEventOptionPublicSemanticsV1, PlannerEventOptionStructuredSemanticsV1,
            PlannerEventSelectionV1,
        };
        use sts_oracle_learning::state::events::{EventActionKind, EventId};

        for transition in [
            PlannerEventTransitionV1::OpenSelection {
                selection: PlannerEventSelectionV1::UpgradeCard,
            },
            PlannerEventTransitionV1::OpenReward,
        ] {
            let candidate = TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ChooseEventOption {
                    event: EventId::Neow,
                    screen: 1,
                    option_index: 0,
                    action: EventActionKind::DeckOperation,
                    public_semantics: PlannerEventOptionPublicSemanticsV1 {
                        structured_semantics: Some(PlannerEventOptionStructuredSemanticsV1 {
                            effects: Vec::new(),
                            transition,
                        }),
                        ..Default::default()
                    },
                },
            };

            assert_eq!(
                agent_action_role_v1(&candidate),
                AgentActionRoleV1::OpensDecision
            );
        }
    }

    #[test]
    fn run_selection_family_keeps_exact_card_domain_without_combination_expansion() {
        let card = |ordinal, card| TaskStrategicCardV1 {
            ordinal,
            card,
            upgrades: 0,
            misc_value: 0,
            base_damage_override: None,
            base_block_override: None,
            cost_modifier: 0,
            bottled_by: None,
        };
        let cards = vec![
            card(0, CardId::Strike),
            card(1, CardId::Strike),
            card(2, CardId::Defend),
            card(3, CardId::AscendersBane),
        ];
        let action = TaskStrategicActionV1::BeginRunCardSelection {
            scope: SelectionScope::Deck,
            reason: SelectionReason::Purge,
            min_choices: 2,
            max_choices: 2,
            selectable_card_ordinals: vec![0, 1, 2],
        };

        let family = project_run_selection_family_from_parts_v1(&cards, &action)
            .expect("typed selection family")
            .expect("selection family present");

        assert_eq!(family.scope, SelectionScope::Deck);
        assert_eq!(family.reason, SelectionReason::Purge);
        assert_eq!((family.min_choices, family.max_choices), (2, 2));
        assert_eq!(
            family.card_tokens,
            ["Strike+0#0", "Strike+0#1", "Defend+0#0"]
        );
        assert_eq!(
            family
                .selectable_cards
                .iter()
                .map(|card| (card.ordinal, card.card))
                .collect::<Vec<_>>(),
            vec![
                (0, CardId::Strike),
                (1, CardId::Strike),
                (2, CardId::Defend),
            ]
        );
    }

    #[test]
    fn strategic_agent_projection_reports_permanent_base_energy() {
        use sts_oracle_learning::agent::task::TaskPublicDecisionV1;
        let relic = |relic| PlannerRelicObservation {
            relic,
            counter: -1,
            used_up: false,
            amount: 0,
        };
        assert_eq!(
            public_base_energy_per_turn_v1(&[relic(
                sts_oracle_learning::content::relics::RelicId::FusionHammer,
            )]),
            4
        );
        assert_eq!(
            public_base_energy_per_turn_v1(&[relic(
                sts_oracle_learning::content::relics::RelicId::SlaversCollar,
            )]),
            3,
            "encounter-conditional energy is not permanent base energy"
        );
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let mut decision: TaskPublicDecisionV1 =
            serde_json::from_value(fixture["payload"]["decision"].clone()).unwrap();
        for (owned, expected) in [
            (vec![], 3),
            (
                vec![sts_oracle_learning::content::relics::RelicId::FusionHammer],
                4,
            ),
            (
                vec![sts_oracle_learning::content::relics::RelicId::SlaversCollar],
                3,
            ),
            (
                vec![sts_oracle_learning::content::relics::RelicId::Lantern],
                3,
            ),
        ] {
            let TaskPublicObservationV1::Strategic { observation } = &mut decision.observation
            else {
                panic!("expected strategic fixture")
            };
            observation.relics = owned.into_iter().map(relic).collect();
            assert_eq!(
                AgentStrategicObservationV1::from_public_v1(observation).base_energy_per_turn,
                expected
            );
        }
    }

    #[test]
    fn agent_deck_evidence_preserves_instances_and_draw_lifecycle() {
        use sts_oracle_learning::agent::task::TaskPublicDecisionV1;
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let mut decision: TaskPublicDecisionV1 =
            serde_json::from_value(fixture["payload"]["decision"].clone()).unwrap();
        let TaskPublicObservationV1::Strategic { observation } = &mut decision.observation else {
            panic!("expected strategic fixture")
        };
        observation.cards = [
            (CardId::Strike, 1, Some(20), -1),
            (CardId::Strike, 1, Some(20), -1),
            (CardId::Strike, 0, None, 0),
            (CardId::PommelStrike, 1, None, 0),
            (CardId::Warcry, 1, None, 0),
            (CardId::DarkEmbrace, 0, None, 0),
            (CardId::Brutality, 0, None, 0),
            (CardId::Pummel, 0, None, 0),
            (CardId::AscendersBane, 0, None, 0),
            (CardId::SearingBlow, 5, None, 0),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(ordinal, (card, upgrades, base_damage_override, cost_modifier))| {
                TaskStrategicCardV1 {
                    ordinal,
                    card,
                    upgrades,
                    misc_value: 0,
                    base_damage_override,
                    base_block_override: None,
                    cost_modifier,
                    bottled_by: None,
                }
            },
        )
        .collect();
        let original = observation.clone();
        let state = AgentStrategicObservationV1::from_public_v1(observation);
        let evidence = project_agent_deck_evidence_v1(&state, "deck-test-snapshot");
        assert_eq!(*observation, original);
        assert_eq!(evidence.public_snapshot_id, "deck-test-snapshot");
        assert_eq!(
            evidence
                .cards
                .iter()
                .map(|row| row.card.count)
                .sum::<usize>(),
            10
        );
        assert_eq!(evidence.cards.len(), 9);
        let modified = &evidence.cards[0];
        assert_eq!(modified.card.count, 2);
        assert_eq!(modified.persistent_damage, 20);
        assert_eq!(modified.persistent_cost, 0);
        assert_eq!(modified.mechanics.damage.damage_per_hit, 9);
        assert!(modified.next_upgrade.is_none());
        let card = |id| {
            evidence
                .cards
                .iter()
                .find(|row| row.card.card == id)
                .unwrap()
        };
        assert_eq!(card(CardId::PommelStrike).mechanics.draw_cards, 2);
        assert_eq!(
            card(CardId::Warcry).mechanics.hand_topdeck_after_draw,
            Some(1)
        );
        assert_eq!(
            card(CardId::DarkEmbrace).mechanics.exhaust_draw_power,
            Some(1)
        );
        assert_eq!(card(CardId::Brutality).mechanics.draw_cards_each_turn, 1);
        assert!(card(CardId::Pummel).mechanics.exhausts);
        assert!(card(CardId::AscendersBane).mechanics.ethereal);
        assert!(!card(CardId::AscendersBane).mechanics.exhausts);
        assert_eq!(card(CardId::SearingBlow).persistent_damage, 42);
        let strike_upgrade = evidence
            .cards
            .iter()
            .find(|row| row.card.card == CardId::Strike && row.card.upgrades == 0)
            .unwrap()
            .next_upgrade
            .as_ref()
            .unwrap();
        assert_eq!(strike_upgrade.persistent_damage, 9);
        assert_eq!(strike_upgrade.definition.upgrades, 1);
        assert_eq!(
            card(CardId::DarkEmbrace)
                .next_upgrade
                .as_ref()
                .unwrap()
                .persistent_cost,
            1
        );
        assert!(card(CardId::AscendersBane).next_upgrade.is_none());
        let wire = serde_json::to_value(evidence).unwrap();
        assert_eq!(wire["cards"][0]["card"]["base_damage_override"], 20);
        assert!(wire.get("rng").is_none());
    }

    #[test]
    fn combat_research_artifact_is_closed_and_round_trips() {
        let provenance = TaskPrivilegedSupportProvenanceV1 {
            access: sts_oracle_learning::agent::task::TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: "fixture-producer".to_string(),
            producer_revision: 1,
        };
        let artifact = RunPlayCombatResearchArtifactV1 {
            schema: COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1.to_string(),
            source_evidence_identity: None,
            session_id: "fixture-session".to_string(),
            root_head_revision: 7,
            root_state_revision: 3,
            root_archive_node_id: "nfixture".to_string(),
            root_public_snapshot_id: "snapshot-fixture".to_string(),
            effort: AgentCombatResearchEffortV1::Standard,
            durable_head_unchanged: true,
            lanes: vec![TaskResearchPlanLaneAttemptV1 {
                provenance: provenance.clone(),
                work_unit: sts_oracle_learning::agent::task::TaskResearchPlanWorkUnitV1::TurnGraphGenerationWork,
                work_consumed: 8,
                exit: sts_oracle_learning::agent::task::TaskResearchPlanAttemptExitV1::GenerationWorkExhausted,
            }],
            bounded_progress: vec![AgentCombatResearchBoundedProgressV1 {
                provenance,
                bounded_search_only: true,
                absence_means_unknown: true,
                max_player_turn: 1,
                recent_turn_survival_envelope: Vec::new(),
                paths: vec![AgentCombatResearchBoundedProgressPathV1 {
                    roles: vec![CombatResearchBoundedProgressRoleV1::DeepestSurvival],
                    plan_steps: 1,
                    endpoint: CombatResearchBoundedProgressStateV1 {
                        player_turn: 1,
                        player_hp: 70,
                        player_block: 0,
                        alive_enemy_count: 1,
                        enemy_total_hp: 40,
                        hand_size: 5,
                        draw_pile_size: 10,
                        discard_pile_size: 0,
                        exhaust_pile_size: 0,
                        path_atomic_depth: 1,
                    },
                    action_line: vec!["end".to_string()],
                    successor_public_snapshot_id: "snapshot-progress".to_string(),
                }],
            }],
            candidates: Vec::new(),
            candidate_replay_proofs: Vec::new(),
            milestone_candidates: Vec::new(),
            milestone_successor_previews: Vec::new(),
        };
        artifact.validate().expect("valid empty frontier artifact");
        let encoded = serde_json::to_vec(&artifact).expect("artifact json");
        let decoded: RunPlayCombatResearchArtifactV1 =
            serde_json::from_slice(&encoded).expect("typed artifact round trip");
        assert_eq!(decoded, artifact);

        let mut value = serde_json::to_value(&artifact).expect("artifact value");
        value
            .as_object_mut()
            .expect("artifact object")
            .remove("bounded_progress");
        let legacy: RunPlayCombatResearchArtifactV1 =
            serde_json::from_value(value.clone()).expect("legacy artifact without progress");
        assert!(legacy.bounded_progress.is_empty());
        value
            .as_object_mut()
            .expect("artifact object")
            .insert("unexpected".to_string(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<RunPlayCombatResearchArtifactV1>(value).is_err());
    }

    #[test]
    fn combat_research_artifact_replay_is_root_and_action_closed() {
        let provenance = TaskPrivilegedSupportProvenanceV1 {
            access: sts_oracle_learning::agent::task::TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: "fixture-producer".to_string(),
            producer_revision: 1,
        };
        let candidate = AgentCombatResearchCandidateV1 {
            candidate_id: "candidate-fixture".to_string(),
            report_candidate_index: None,
            supporting_provenances: vec![provenance.clone()],
            plan_steps: 1,
            minimum_observed_hp: Some(7),
            satisfies_requested_quality: false,
            outcome: TaskResearchPlanOutcomeV1 {
                final_hp: 10,
                final_gold: 0,
                persistent: sts_oracle_learning::agent::task::TaskResearchPlanPersistentOutcomeV1 {
                    max_hp: 80,
                    recoverable_gold_delta: 0,
                    unrecovered_stolen_gold: 0,
                    ritual_dagger_value: 0,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                consumed_entry_potion_slots: Vec::new(),
                total_potions_consumed: 0,
                generated_potions: Vec::new(),
                final_relics: vec![sts_oracle_learning::content::relics::RelicState {
                    counter: 4,
                    ..sts_oracle_learning::content::relics::RelicState::new(
                        sts_oracle_learning::content::relics::RelicId::IncenseBurner,
                    )
                }],
                meta_changes: Vec::new(),
                pending_rewards: Vec::new(),
                combat_mugged: false,
            },
            action_line: vec!["end".to_string()],
        };
        let live = project_live_combat_research_candidates_v1(
            std::slice::from_ref(&candidate),
            "test-report",
        );
        assert_eq!(live[0].candidate_ref, "c0");
        assert_eq!(live[0].plan_id, "c0@4bf5ca407322");
        assert_eq!(live[0].candidate_id, "candidate-fixture");
        assert!(!live[0].meets_requested_satisfaction);
        assert_eq!(live[0].minimum_observed_hp, Some(7));
        assert_eq!(live[0].next_incense_turn, Some(2));
        let mut legacy = serde_json::to_value(&candidate).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("minimum_observed_hp");
        let legacy: AgentCombatResearchCandidateV1 = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.minimum_observed_hp, None);
        let mut artifact = RunPlayCombatResearchArtifactV1 {
            schema: COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1.to_string(),
            source_evidence_identity: None,
            session_id: "fixture-session".to_string(),
            root_head_revision: 7,
            root_state_revision: 3,
            root_archive_node_id: "nfixture".to_string(),
            root_public_snapshot_id: "snapshot-fixture".to_string(),
            effort: AgentCombatResearchEffortV1::Standard,
            durable_head_unchanged: true,
            lanes: vec![TaskResearchPlanLaneAttemptV1 {
                provenance,
                work_unit: sts_oracle_learning::agent::task::TaskResearchPlanWorkUnitV1::TurnGraphGenerationWork,
                work_consumed: 8,
                exit: sts_oracle_learning::agent::task::TaskResearchPlanAttemptExitV1::GenerationWorkExhausted,
            }],
            bounded_progress: Vec::new(),
            candidates: vec![candidate],
            candidate_replay_proofs: vec![AgentCombatResearchReplayProofV1 {
                candidate_id: "candidate-fixture".to_string(),
                steps: vec![AgentCombatResearchReplayStepV1 {
                    source_public_snapshot_id: "snapshot-fixture".to_string(),
                    action: "end".to_string(),
                }],
                endpoint: AgentCombatResearchReplayEndpointV1::PublicDecision {
                    public_snapshot_id: "snapshot-successor".to_string(),
                },
            }],
            milestone_candidates: Vec::new(),
            milestone_successor_previews: Vec::new(),
        };
        artifact.validate().expect("root-bound replay artifact");

        artifact.candidate_replay_proofs[0].steps[0].action = "play:Bash".to_string();
        assert!(artifact.validate().is_err());
    }

    #[test]
    fn preview_and_act_share_action_line_limits() {
        for (count, accepted) in [
            (0, false),
            (1, true),
            (4, true),
            (5, true),
            (256, true),
            (257, false),
        ] {
            let actions = vec!["end"; count];
            for request in [
                serde_json::json!({"command":"act","actions":actions}),
                serde_json::json!({"command":"probe","lines":[actions]}),
            ] {
                let mut request = request;
                request["schema"] = AGENT_REQUEST_SCHEMA_V1.into();
                request["request_id"] = "line-limits".into();
                let typed: AgentShellRequestV1 = serde_json::from_value(request).unwrap();
                assert_eq!(
                    typed.validate().is_ok(),
                    accepted,
                    "{count} actions: {typed:?}"
                );
            }
        }
        // Raising the per-line limit does not remove the bounded batch size.
        let too_many_lines: AgentShellRequestV1 = serde_json::from_value(serde_json::json!({
            "schema":AGENT_REQUEST_SCHEMA_V1,"request_id":"batch-limit",
            "command":"probe","lines":vec![vec!["end"]; 5],
        }))
        .unwrap();
        assert!(too_many_lines
            .validate()
            .unwrap_err()
            .contains("1..=4 lines"));
    }

    #[test]
    fn requests_are_schema_closed_and_semantic() {
        let request: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r1","command":"act","actions":["take:Anger"]}"#,
        )
        .expect("valid act request");
        assert_eq!(request.validate(), Ok(()));
        assert!(matches!(
            request,
            AgentShellRequestV1::Act {
                advance: AgentAdvanceModeV1::OnePublicAction,
                ..
            }
        ));
        let transactional: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r1-line","command":"act","actions":["play:Strike+0#0@0","end"]}"#,
        )
        .expect("valid transactional act request");
        assert_eq!(transactional.validate(), Ok(()));
        let invalid_advance: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r1-advance","command":"act","actions":["play:Strike+0#0@0","end"],"advance":"next_strategic_decision"}"#,
        )
        .expect("closed transactional act request");
        assert!(invalid_advance.validate().is_err());
        let game_facts: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"facts","command":"game_facts","entities":[{"kind":"card","card":"DoubleTap"},{"kind":"relic","relic":"OrangePellets"}]}"#,
        )
        .expect("typed game facts request");
        assert_eq!(game_facts.validate(), Ok(()));
        assert!(serde_json::from_str::<AgentShellRequestV1>(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r2","command":"act","actions":["ordinal:0"],"extra":true}"#
        )
        .is_err());
        assert!(serde_json::from_str::<AgentShellRequestV1>(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r3","command":"commit_combat_plan","candidate_ref":""}"#
        )
        .expect("typed combat-plan request")
        .validate()
        .is_err());
        let ordinal: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r3","command":"act","actions":["ordinal:0"]}"#,
        )
        .expect("closed ordinal request");
        assert!(ordinal.validate().is_err());
        for action in ["ordinal:0", "ordinal:event:Neow:0"] {
            let error = validate_agent_semantic_action_v1(action, "agent-shell act")
                .expect_err("ordinal addressing is not a player action");
            assert!(error.contains("copy a semantic action from current options verbatim"));
            assert!(!error.contains("invalid digit"));
        }
        let probe: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r4","command":"probe","lines":[["event:Neow:0"],["event:Neow:2"]]}"#,
        )
        .expect("closed probe request");
        assert_eq!(probe.validate(), Ok(()));
        let ordinal_probe: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r5","command":"probe","lines":[["ordinal:0"]]}"#,
        )
        .expect("closed ordinal probe request");
        assert!(ordinal_probe.validate().is_err());
        let expanded: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r6","command":"combat_research","effort":"expanded"}"#,
        )
        .expect("typed expanded research request");
        assert!(matches!(
            expanded,
            AgentShellRequestV1::CombatResearch {
                effort: AgentCombatResearchEffortV1::Expanded,
                include_milestone_successor_previews: false,
                ..
            }
        ));
        let expanded_with_previews: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r6-preview","command":"combat_research","effort":"expanded","include_milestone_successor_previews":true}"#,
        )
        .expect("typed expanded research request with previews");
        assert!(matches!(
            expanded_with_previews,
            AgentShellRequestV1::CombatResearch {
                effort: AgentCombatResearchEffortV1::Expanded,
                include_milestone_successor_previews: true,
                ..
            }
        ));
        let draft_advance: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"draft-advance","command":"combat_draft_advance","generation_work":1024,"engine_steps":65536}"#,
        )
        .expect("typed combat draft quantum");
        assert_eq!(draft_advance.validate(), Ok(()));
        let oversized_draft_advance: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"draft-too-large","command":"combat_draft_advance","generation_work":65537,"engine_steps":65536}"#,
        )
        .expect("closed oversized draft request");
        assert!(oversized_draft_advance.validate().is_err());
        let draft_choice: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"draft-choose","command":"combat_draft_choose","frame_ref":"F2","option_ref":"T3"}"#,
        )
        .expect("frame-bound combat draft choice");
        assert_eq!(draft_choice.validate(), Ok(()));
        assert!(serde_json::from_str::<AgentShellRequestV1>(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"depth-unsafe","command":"combat_draft_choose","selected_turn":2,"option_ref":"T3"}"#,
        ).is_err());
        assert!(serde_json::from_str::<AgentShellRequestV1>(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"stale-unsafe","command":"combat_draft_choose","option_ref":"T3"}"#,
        )
        .is_err());
        assert!(serde_json::from_str::<AgentShellRequestV1>(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r7","command":"combat_research","effort":"unbounded"}"#
        )
        .is_err());
        let review: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r7-review","command":"combat_review","candidate_refs":["c0"],"effort":"expanded"}"#,
        )
        .expect("typed combat review request");
        assert_eq!(review.validate(), Ok(()));
        assert!(matches!(
            review,
            AgentShellRequestV1::CombatReview {
                include_steps: false,
                ..
            }
        ));
        let duplicate_review = AgentShellRequestV1::CombatReview {
            report_id: None,
            schema: AGENT_REQUEST_SCHEMA_V1.to_string(),
            request_id: "r7-review-duplicate".to_string(),
            candidate_refs: vec!["c0".to_string(), "c0".to_string()],
            turn: None,
            effort: AgentCombatResearchEffortV1::Expanded,
            include_steps: false,
            compact: false,
        };
        assert!(duplicate_review.validate().is_err());
        assert!(serde_json::from_str::<AgentShellRequestV1>(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r8","command":"commit_combat_turn","candidate_id":""}"#
        )
        .expect("typed combat-turn request")
        .validate()
        .is_err());
        let details: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r9","command":"combat_milestone_successor_details","candidate_ids":["candidate-a","candidate-b"],"effort":"expanded"}"#,
        )
        .expect("typed milestone successor detail request");
        assert_eq!(details.validate(), Ok(()));
        let duplicate_details = AgentShellRequestV1::CombatMilestoneSuccessorDetails {
            schema: AGENT_REQUEST_SCHEMA_V1.to_string(),
            request_id: "r10".to_string(),
            candidate_ids: vec!["candidate-a".to_string(), "candidate-a".to_string()],
            effort: AgentCombatResearchEffortV1::Expanded,
        };
        assert!(duplicate_details.validate().is_err());
        let oversized_details = AgentShellRequestV1::CombatMilestoneSuccessorDetails {
            schema: AGENT_REQUEST_SCHEMA_V1.to_string(),
            request_id: "r11".to_string(),
            candidate_ids: (0..=MAX_AGENT_COMBAT_MILESTONE_SUCCESSOR_DETAILS)
                .map(|index| format!("candidate-{index}"))
                .collect(),
            effort: AgentCombatResearchEffortV1::Expanded,
        };
        assert!(oversized_details.validate().is_err());
        let continuation_audit: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r12","command":"combat_milestone_continuation_audit","candidate_ids":["candidate-a"],"effort":"expanded"}"#,
        )
        .expect("typed milestone continuation audit request");
        assert_eq!(continuation_audit.validate(), Ok(()));
        let prefix_horizon: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r13","command":"combat_prefix_horizon_audit","root_head_revision":159,"lineage_id":"candidate-a","prefix_actions":["play:Defend+0#0","end"],"expected_successor_public_snapshot_id":"snapshot-a","additional_turns":2}"#,
        )
        .expect("typed combat prefix horizon request");
        assert_eq!(prefix_horizon.validate(), Ok(()));
        let ordinal_prefix = AgentShellRequestV1::CombatPrefixHorizonAudit {
            schema: AGENT_REQUEST_SCHEMA_V1.to_string(),
            request_id: "r14".to_string(),
            root_head_revision: 159,
            lineage_id: "candidate-a".to_string(),
            prefix_actions: vec!["ordinal:0".to_string()],
            expected_successor_public_snapshot_id: "snapshot-a".to_string(),
            additional_turns: 2,
        };
        assert!(ordinal_prefix.validate().is_err());

        let turn_continuation_audit: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r15","command":"combat_turn_continuation_audit","candidate_ids":["candidate-a","candidate-b"],"effort":"expanded"}"#,
        )
        .expect("typed combat-turn continuation audit request");
        assert_eq!(turn_continuation_audit.validate(), Ok(()));
        let turn_candidate_details: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r16","command":"combat_turn_candidate_details","candidate_ids":["candidate-a"]}"#,
        )
        .expect("typed combat-turn candidate details request");
        assert_eq!(turn_candidate_details.validate(), Ok(()));
        let turn_reservation: AgentShellRequestV1 = serde_json::from_str(
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"r17","command":"combat_turn_reserve","line":["select:Bash+#0,Defend+0#0","play:DemonForm+0#0","end"]}"#,
        )
        .expect("typed combat-turn reservation request");
        assert_eq!(turn_reservation.validate(), Ok(()));
        let duplicate_turn_candidate_details = AgentShellRequestV1::CombatTurnCandidateDetails {
            schema: AGENT_REQUEST_SCHEMA_V1.to_string(),
            request_id: "duplicate-turn-candidate-details".to_string(),
            candidate_ids: vec!["candidate-a".to_string(), "candidate-a".to_string()],
        };
        assert!(duplicate_turn_candidate_details.validate().is_err());
        for invalid_candidate_ids in [
            Vec::new(),
            vec!["candidate-a".to_string(), "candidate-a".to_string()],
            (0..=MAX_AGENT_COMBAT_TURN_CONTINUATION_AUDITS)
                .map(|index| format!("candidate-{index}"))
                .collect(),
            vec![" ".to_string()],
            vec!["x".repeat(257)],
        ] {
            let invalid = AgentShellRequestV1::CombatTurnContinuationAudit {
                schema: AGENT_REQUEST_SCHEMA_V1.to_string(),
                request_id: "invalid-turn-continuation-audit".to_string(),
                candidate_ids: invalid_candidate_ids,
                effort: AgentCombatResearchEffortV1::Standard,
            };
            assert!(invalid.validate().is_err());
        }
    }

    #[test]
    fn whole_combat_effort_honors_shorter_and_longer_explicit_time_allowances() {
        let base = CombatResearchPlanOptionsV1::default();
        for requested in [500, 4_000, 15_000] {
            let options = combat_research_options_for_effort_v1(
                &base,
                AgentCombatResearchEffortV1::WholeCombat,
                Some(requested),
            );
            assert_eq!(options.wall_ms, requested);
            assert_eq!(
                options.max_generation_work,
                base.max_generation_work.max(30_000)
            );
            assert_eq!(options.potion_policy, base.potion_policy);
            assert_eq!(options.validate(), Ok(()));
        }
        assert_eq!(
            combat_research_options_for_effort_v1(
                &base,
                AgentCombatResearchEffortV1::WholeCombat,
                None
            )
            .wall_ms,
            base.wall_ms.max(2_000)
        );
    }

    #[test]
    fn per_request_potion_ceiling_only_tightens_the_effort_contract() {
        for effort in [
            AgentCombatResearchEffortV1::WholeCombat,
            AgentCombatResearchEffortV1::Standard,
            AgentCombatResearchEffortV1::Expanded,
            AgentCombatResearchEffortV1::FastWitness,
        ] {
            let base = CombatResearchPlanOptionsV1::default();
            let mut options = combat_research_options_for_effort_v1(&base, effort, None);
            let original = options.clone();
            restrict_combat_research_potions_v1(&mut options, None);
            assert_eq!(options.max_potions_used, original.max_potions_used);
            restrict_combat_research_potions_v1(&mut options, Some(0));
            assert_eq!(options.max_potions_used, Some(0));
            restrict_combat_research_potions_v1(&mut options, Some(2));
            assert_eq!(options.max_potions_used, Some(0));
        }
        for value in [serde_json::json!(0), serde_json::json!(2)] {
            let request: AgentShellRequestV1 = serde_json::from_value(serde_json::json!({
                "schema":AGENT_REQUEST_SCHEMA_V1,"request_id":"resource-budget","command":"combat_research","max_potions_used":value,
            })).unwrap();
            assert_eq!(request.validate(), Ok(()));
        }
        for value in [
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!(4294967296_u64),
            serde_json::json!("0"),
        ] {
            assert!(serde_json::from_value::<AgentShellRequestV1>(serde_json::json!({
                "schema":AGENT_REQUEST_SCHEMA_V1,"request_id":"resource-budget","command":"combat_research","max_potions_used":value,
            })).is_err());
        }
    }

    #[test]
    fn expanded_combat_effort_is_bounded_and_preserves_resource_policy() {
        let mut base = CombatResearchPlanOptionsV1 {
            complete_turn_proposal_beam: Some(CompleteTurnProposalBeamResearchOptionsV1::default()),
            ..CombatResearchPlanOptionsV1::default()
        };
        base.max_potions_used = Some(1);
        let expanded = combat_research_options_for_effort_v1(
            &base,
            AgentCombatResearchEffortV1::Expanded,
            None,
        );
        assert_eq!(expanded.max_generation_work, 32_768);
        assert_eq!(expanded.wall_ms, 5_000);
        assert_eq!(expanded.max_potions_used, Some(1));
        assert_eq!(
            expanded.milestone_closure,
            Some(
                sts_oracle_learning::eval::run_control::CombatMilestoneClosureResearchOptionsV1::default()
            )
        );
        assert_eq!(
            expanded.satisfaction,
            sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(0)
        );
        assert_eq!(expanded.potion_policy, base.potion_policy);
        let beam = expanded.complete_turn_proposal_beam.expect("expanded beam");
        assert_eq!(beam.beam_width, 32);
        assert_eq!(beam.max_expanded_boundaries, 4_096);
        assert_eq!(beam.wall_ms, 30_000);
        let atomic = expanded.atomic_challenger.expect("expanded atomic");
        assert_eq!(atomic.max_nodes, 65_536);
        assert_eq!(atomic.wall_ms, 30_000);
    }

    #[test]
    fn expanded_combat_effort_merges_the_standard_verified_frontier() {
        use std::time::Duration;

        use sts_oracle_learning::agent::task::{
            TaskPrivilegedSupportAccessV1, TaskPublicChoiceV1, TaskResearchPlanAttemptV1,
            TaskResearchPlanPersistentOutcomeV1, TaskResearchPlanStepV1, TaskResearchPlanV1,
            TaskResearchPlanWorkUnitV1,
        };

        let provenance = TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: "fixture-beam".to_string(),
            producer_revision: 1,
        };
        let progress_provenance = TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: "fixture-resident".to_string(),
            producer_revision: 1,
        };
        let pass = |choice: usize,
                    final_hp: i32,
                    max_hp: i32,
                    consumed_entry_potion_slots: Vec<usize>,
                    work_consumed: u64| {
            let total_potions_consumed = consumed_entry_potion_slots.len() as u32;
            RunPlayCombatResearchAttemptV1 {
                attempt: TaskResearchPlanAttemptV1 {
                    candidates: vec![TaskResearchPlanCandidateV1 {
                        plan: TaskResearchPlanV1 {
                            root_public_snapshot_id: "fixture-root".to_string(),
                            steps: vec![TaskResearchPlanStepV1 {
                                source_public_snapshot_id: "fixture-root".to_string(),
                                choice: TaskPublicChoiceV1::one(choice),
                            }],
                        },
                        outcome: TaskResearchPlanOutcomeV1 {
                            final_hp,
                            final_gold: 0,
                            persistent: TaskResearchPlanPersistentOutcomeV1 {
                                max_hp,
                                recoverable_gold_delta: 0,
                                unrecovered_stolen_gold: 0,
                                ritual_dagger_value: 0,
                                genetic_algorithm_value: 0,
                                external_burden_count: 0,
                            },
                            consumed_entry_potion_slots,
                            total_potions_consumed,
                            generated_potions: Vec::new(),
                            final_relics: Vec::new(),
                            meta_changes: Vec::new(),
                            pending_rewards: Vec::new(),
                            combat_mugged: false,
                        },
                        satisfies_requested_quality: true,
                        supporting_provenances: vec![provenance.clone()],
                    }],
                    lanes: vec![TaskResearchPlanLaneAttemptV1 {
                        provenance: provenance.clone(),
                        work_unit:
                            TaskResearchPlanWorkUnitV1::CompleteTurnProposalExpandedBoundaries,
                        work_consumed,
                        exit: TaskResearchPlanAttemptExitV1::Witness,
                    }]
                    .into_iter()
                    .chain((choice == 0).then(|| TaskResearchPlanLaneAttemptV1 {
                        provenance: progress_provenance.clone(),
                        work_unit: TaskResearchPlanWorkUnitV1::TurnGraphGenerationWork,
                        work_consumed: 8,
                        exit: TaskResearchPlanAttemptExitV1::GenerationWorkExhausted,
                    }))
                    .collect(),
                },
                witness_lines: vec![ExactRunPlayProbeLineV1 {
                    steps: Vec::new(),
                    fork_elapsed: Duration::ZERO,
                    total_elapsed: Duration::ZERO,
                }],
                milestone_candidates: Vec::new(),
                bounded_progress: (choice == 0)
                    .then(|| RunPlayCombatResearchBoundedProgressV1 {
                        provenance: progress_provenance.clone(),
                        max_player_turn: 3,
                        recent_turn_survival_envelope: Vec::new(),
                        paths: Vec::new(),
                    })
                    .into_iter()
                    .collect(),
            }
        };

        let merged = merge_combat_research_passes_v1([
            pass(0, 16, 94, vec![0], 12),
            pass(1, 13, 91, Vec::new(), 30),
        ])
        .expect("cumulative effort frontier");

        assert_eq!(merged.attempt.candidates.len(), 2);
        assert_eq!(merged.witness_lines.len(), 2);
        assert_eq!(merged.attempt.candidates[0].outcome.final_hp, 16);
        assert_eq!(merged.attempt.candidates[0].outcome.persistent.max_hp, 94);
        assert_eq!(merged.attempt.candidates[1].outcome.final_hp, 13);
        assert_eq!(merged.attempt.lanes.len(), 2);
        assert!(
            merged.bounded_progress.is_empty(),
            "a terminal frontier supersedes any earlier bounded-miss receipt"
        );
        let beam_lane = merged
            .attempt
            .lanes
            .iter()
            .find(|lane| lane.provenance == provenance)
            .expect("merged beam lane");
        assert_eq!(beam_lane.work_consumed, 42);
        assert_eq!(beam_lane.exit, TaskResearchPlanAttemptExitV1::Witness);
        merged.attempt.validate().expect("merged attempt contract");

        let dominated = merge_combat_research_passes_v1([
            pass(0, 14, 95, vec![0, 1], 4),
            pass(1, 16, 95, vec![0], 8),
            pass(2, 18, 95, vec![0], 12),
            pass(3, 20, 95, Vec::new(), 30),
        ])
        .expect("merged dominated producer evidence");
        assert_eq!(
            dominated.attempt.candidates.len(),
            4,
            "producer evidence stays available inside the audited attempt"
        );
        let projected = project_agent_combat_research_candidates_v1(&dominated)
            .expect("public resource frontier");
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].report_candidate_index, Some(3));
        assert_eq!(projected[0].outcome.final_hp, 20);
        assert_eq!(projected[0].outcome.total_potions_consumed, 0);
        assert_eq!(
            resolve_artifact_combat_research_candidate_index_v1(
                &projected,
                "fixture-beam",
                Some(3),
            )
            .expect("printed report candidate index"),
            0,
            "the printed global index resolves to the retained artifact-local candidate"
        );
        assert_eq!(
            resolve_artifact_combat_research_report_candidate_index_v1(&projected, 3)
                .expect("shared artifact reader resolves the printed report index"),
            0,
            "trace and anchor readers must share the public-to-local projection"
        );
        assert!(
            resolve_artifact_combat_research_report_candidate_index_v1(&projected, 0).is_err(),
            "a current artifact must not expose its hidden local array index"
        );
        let mut legacy = projected.clone();
        for candidate in &mut legacy {
            candidate.report_candidate_index = None;
        }
        assert_eq!(
            resolve_artifact_combat_research_report_candidate_index_v1(&legacy, 0)
                .expect("legacy artifacts retain their historical local index"),
            0
        );
        assert!(
            resolve_artifact_combat_research_report_candidate_index_v1(&legacy, 3).is_err(),
            "legacy compatibility is read-only local indexing, not guessed global identity"
        );
    }

    #[test]
    fn expanded_combat_effort_merges_bounded_progress_independently_of_pass_order() {
        use crate::driver::RunPlayCombatResearchBoundedProgressPathV1;
        use sts_oracle_learning::agent::task::{
            TaskPrivilegedSupportAccessV1, TaskPublicChoiceV1, TaskResearchPlanAttemptV1,
            TaskResearchPlanStepV1, TaskResearchPlanV1, TaskResearchPlanWorkUnitV1,
        };
        use sts_oracle_learning::eval::run_control::CombatResearchBoundedProgressPathV1;

        let provenance = TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: "fixture-resident".to_string(),
            producer_revision: 1,
        };
        let pass = |player_turn: u32,
                    player_hp: i32,
                    enemy_total_hp: i32,
                    role: CombatResearchBoundedProgressRoleV1,
                    choice: usize,
                    successor_public_snapshot_id: &str,
                    work_consumed: u64| {
            let endpoint = CombatResearchBoundedProgressStateV1 {
                player_turn,
                player_hp,
                player_block: 0,
                alive_enemy_count: 1,
                enemy_total_hp,
                hand_size: 5,
                draw_pile_size: 8,
                discard_pile_size: 4,
                exhaust_pile_size: 0,
                path_atomic_depth: 1,
            };
            RunPlayCombatResearchAttemptV1 {
                attempt: TaskResearchPlanAttemptV1 {
                    candidates: Vec::new(),
                    lanes: vec![TaskResearchPlanLaneAttemptV1 {
                        provenance: provenance.clone(),
                        work_unit: TaskResearchPlanWorkUnitV1::TurnGraphGenerationWork,
                        work_consumed,
                        exit: TaskResearchPlanAttemptExitV1::GenerationWorkExhausted,
                    }],
                },
                witness_lines: Vec::new(),
                milestone_candidates: Vec::new(),
                bounded_progress: vec![RunPlayCombatResearchBoundedProgressV1 {
                    provenance: provenance.clone(),
                    max_player_turn: player_turn,
                    recent_turn_survival_envelope: vec![endpoint],
                    paths: vec![RunPlayCombatResearchBoundedProgressPathV1 {
                        candidate: CombatResearchBoundedProgressPathV1 {
                            roles: vec![role],
                            plan: TaskResearchPlanV1 {
                                root_public_snapshot_id: "fixture-root".to_string(),
                                steps: vec![TaskResearchPlanStepV1 {
                                    source_public_snapshot_id: "fixture-root".to_string(),
                                    choice: TaskPublicChoiceV1::one(choice),
                                }],
                            },
                            endpoint,
                        },
                        action_line: vec![format!("fixture:{choice}")],
                        successor_public_snapshot_id: successor_public_snapshot_id.to_string(),
                    }],
                }],
            }
        };
        let merge = |reverse: bool| {
            let standard = pass(
                3,
                12,
                40,
                CombatResearchBoundedProgressRoleV1::DeepestSurvival,
                0,
                "snapshot-survival",
                12,
            );
            let expanded = pass(
                5,
                4,
                18,
                CombatResearchBoundedProgressRoleV1::DeepestEnemyProgress,
                1,
                "snapshot-progress",
                30,
            );
            if reverse {
                merge_combat_research_passes_v1([expanded, standard])
            } else {
                merge_combat_research_passes_v1([standard, expanded])
            }
            .expect("merged bounded progress")
        };

        let forward = merge(false);
        let reverse = merge(true);
        let forward = &forward.bounded_progress[0];
        let reverse = &reverse.bounded_progress[0];
        assert_eq!(forward.max_player_turn, 5);
        assert_eq!(forward.max_player_turn, reverse.max_player_turn);
        assert_eq!(
            forward.recent_turn_survival_envelope,
            reverse.recent_turn_survival_envelope
        );
        assert_eq!(forward.paths.len(), 2);
        assert_eq!(forward.paths.len(), reverse.paths.len());
        for (forward, reverse) in forward.paths.iter().zip(&reverse.paths) {
            assert_eq!(forward.candidate, reverse.candidate);
            assert_eq!(
                forward.successor_public_snapshot_id,
                reverse.successor_public_snapshot_id
            );
        }
    }

    #[test]
    fn fast_witness_is_a_no_potion_preservation_pass() {
        let base = CombatResearchPlanOptionsV1 {
            potion_policy: CombatWitnessPotionPolicyV1::All,
            max_potions_used: None,
            complete_turn_proposal_beam: Some(CompleteTurnProposalBeamResearchOptionsV1::default()),
            ..CombatResearchPlanOptionsV1::default()
        };

        let fast = combat_research_options_for_effort_v1(
            &base,
            AgentCombatResearchEffortV1::FastWitness,
            None,
        );
        assert_eq!(
            fast.lane_schedule,
            sts_oracle_learning::eval::run_control::CombatResearchLaneScheduleV1::FirstWitness
        );
        assert_eq!(
            fast.satisfaction,
            sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::FirstCompleteWinWithoutNewExternalBurden
        );
        assert_eq!(fast.max_generation_work, base.max_generation_work);
        assert_eq!(fast.potion_policy, CombatWitnessPotionPolicyV1::Never);
        assert_eq!(fast.max_potions_used, Some(0));
        assert!(fast.complete_turn_proposal_beam.is_some());
        assert!(fast.atomic_challenger.is_some());
    }

    #[test]
    fn default_combat_efforts_expand_potion_use_one_step_at_a_time() {
        let base = CombatResearchPlanOptionsV1 {
            potion_policy: CombatWitnessPotionPolicyV1::All,
            max_potions_used: None,
            ..CombatResearchPlanOptionsV1::default()
        };

        let standard = combat_research_options_for_effort_v1(
            &base,
            AgentCombatResearchEffortV1::Standard,
            None,
        );
        let expanded = combat_research_options_for_effort_v1(
            &base,
            AgentCombatResearchEffortV1::Expanded,
            None,
        );

        assert_eq!(standard.max_potions_used, Some(1));
        assert_eq!(
            standard.satisfaction,
            sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(0)
        );
        assert_eq!(expanded.max_potions_used, Some(2));
        assert_eq!(base.max_potions_used, None);
    }

    #[test]
    fn combat_turn_continuation_effort_is_bounded_and_preserves_potion_policy() {
        let mut base = CombatResearchPlanOptionsV1 {
            max_engine_steps_per_transition: 400,
            max_potions_used: Some(2),
            ..Default::default()
        };

        let standard = combat_turn_continuation_audit_options_for_effort_v1(
            &base,
            AgentCombatResearchEffortV1::Standard,
        );
        assert_eq!(standard.max_generation_work, 250_000);
        assert_eq!(standard.max_selections, 1_000_000);
        assert_eq!(standard.wall_ms, 5_000);
        assert_eq!(standard.max_engine_steps_per_transition, 400);
        assert_eq!(standard.total_max_potions_used, Some(2));
        assert_eq!(standard.validate(), Ok(()));

        let expanded = combat_turn_continuation_audit_options_for_effort_v1(
            &base,
            AgentCombatResearchEffortV1::Expanded,
        );
        assert_eq!(expanded.max_generation_work, 1_000_000);
        assert_eq!(expanded.max_selections, 1_000_000);
        assert_eq!(expanded.wall_ms, 15_000);
        assert_eq!(expanded.total_max_potions_used, Some(2));
        assert_eq!(expanded.validate(), Ok(()));

        base.max_potions_used = None;
        assert_eq!(
            combat_turn_continuation_audit_options_for_effort_v1(
                &base,
                AgentCombatResearchEffortV1::Standard,
            )
            .total_max_potions_used,
            Some(1)
        );
        assert_eq!(
            combat_turn_continuation_audit_options_for_effort_v1(
                &base,
                AgentCombatResearchEffortV1::Expanded,
            )
            .total_max_potions_used,
            Some(2)
        );

        base.potion_policy = CombatWitnessPotionPolicyV1::Never;
        base.max_potions_used = None;
        assert_eq!(
            combat_turn_continuation_audit_options_for_effort_v1(
                &base,
                AgentCombatResearchEffortV1::Standard,
            )
            .total_max_potions_used,
            Some(0)
        );
    }

    #[test]
    fn combat_turn_summary_is_compact_and_has_no_full_successor() {
        let summary = AgentCombatTurnCandidateSummaryV1 {
            candidate_id: "candidate-a".to_string(),
            actions: vec![AgentCombatTurnActionSummaryV1::EndTurn],
            boundary: ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn,
            potion_slot_mask: 0,
            potion_expenditures: 0,
            endpoint: AgentCombatTurnEndpointSummaryV1 {
                turn_count: 2,
                player_hp: 70,
                player_block: 0,
                energy: 3,
                player_powers: Vec::new(),
                potion_ids: Vec::new(),
                hand: Vec::new(),
                draw_count: 5,
                discard_count: 5,
                exhaust_count: 0,
                monsters: Vec::new(),
            },
        };
        let encoded = serde_json::to_string(&summary).expect("compact turn summary json");
        for forbidden in [
            "successor",
            "master_deck",
            "executed_moves",
            "hidden_reasons",
        ] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn invalid_input_still_serializes_as_one_response_line() {
        let response =
            error_response_v1(None, Instant::now(), "invalid_json", "fixture".to_string());
        let mut output = Vec::new();
        serde_json::to_writer(&mut output, &response).expect("response json");
        output.push(b'\n');
        let rendered = String::from_utf8(output).expect("utf8 response");
        assert_eq!(rendered.lines().count(), 1);
        let value: serde_json::Value = serde_json::from_str(rendered.trim()).expect("response");
        assert_eq!(value["status"], "error");
        assert_eq!(value["request_id"], serde_json::Value::Null);
    }

    #[test]
    fn game_facts_preserve_simulator_owned_false_card_properties() {
        let facts = agent_game_facts_v1(&[AgentGameFactEntityV1::Card {
            card: CardId::DoubleTap,
        }]);
        let AgentGameFactV1::Card { definition, .. } = &facts[0] else {
            panic!("expected card definition")
        };
        assert_eq!(definition.current.card, CardId::DoubleTap);
        assert!(!definition.current.exhausts);
        assert!(!definition.current.ethereal);
        assert!(!definition.current.innate);
    }

    #[test]
    fn game_facts_describe_copy_choices_without_blanket_support_labels() {
        let facts = agent_game_facts_v1(&[AgentGameFactEntityV1::Card {
            card: CardId::DualWield,
        }]);
        let AgentGameFactV1::Card { definition } = &facts[0] else {
            panic!("expected card facts")
        };

        let effect = definition.current.hand_copy.unwrap();
        assert_eq!(effect.single_candidate_copies, 1);
        assert_eq!(effect.selected_candidate_replacement_copies, 2);
        let upgraded = definition.upgraded.as_ref().unwrap().hand_copy.unwrap();
        assert_eq!(upgraded.single_candidate_copies, 2);
        assert_eq!(upgraded.selected_candidate_replacement_copies, 3);
        let wire = serde_json::to_value(&facts[0]).unwrap();
        assert!(wire.get("mechanic_support").is_none());
    }

    #[test]
    fn game_facts_preserve_direct_self_hp_loss_contracts() {
        let facts = agent_game_facts_v1(&[
            AgentGameFactEntityV1::Card {
                card: CardId::Hemokinesis,
            },
            AgentGameFactEntityV1::Card {
                card: CardId::Bloodletting,
            },
        ]);

        for (fact, expected_hp_loss) in facts.iter().zip([2, 3]) {
            let AgentGameFactV1::Card { definition, .. } = fact else {
                panic!("expected card definition")
            };
            let loss = definition
                .current
                .self_hp_loss_on_play
                .expect("direct self HP loss facts");
            assert_eq!(loss.base_hp_loss, expected_hp_loss);
            assert!(loss.bypasses_block);
            assert!(!loss.bypasses_intangible);
            assert!(loss.triggers_rupture);
        }
    }

    #[test]
    fn game_facts_preserve_recurring_end_of_turn_block_contracts() {
        let facts = agent_game_facts_v1(&[AgentGameFactEntityV1::Card {
            card: CardId::Metallicize,
        }]);
        let AgentGameFactV1::Card { definition, .. } = &facts[0] else {
            panic!("expected card definition")
        };
        let current = definition
            .current
            .end_of_turn_block_each_turn
            .expect("Metallicize recurring Block");
        let upgraded = definition
            .upgraded
            .as_ref()
            .expect("Metallicize upgrade")
            .end_of_turn_block_each_turn
            .expect("upgraded Metallicize recurring Block");
        assert_eq!(current.amount, 3);
        assert_eq!(upgraded.amount, 4);
        assert!(current.persists_for_combat);
        assert!(current.stacks_additively);
        assert!(!current.affected_by_dexterity);
        assert!(!current.affected_by_frail);
    }

    #[test]
    fn game_facts_keep_card_effect_direction_and_relic_debuff_scope_typed() {
        let facts = agent_game_facts_v1(&[
            AgentGameFactEntityV1::Card {
                card: CardId::Shockwave,
            },
            AgentGameFactEntityV1::Card { card: CardId::Flex },
            AgentGameFactEntityV1::Card {
                card: CardId::Berserk,
            },
            AgentGameFactEntityV1::Card { card: CardId::Feed },
            AgentGameFactEntityV1::Relic {
                relic: RelicId::OrangePellets,
            },
        ]);

        let AgentGameFactV1::Card {
            definition: shockwave,
            ..
        } = &facts[0]
        else {
            panic!("expected Shockwave facts")
        };
        assert_eq!(
            shockwave.current.play_target,
            sts_oracle_learning::ai::card_semantics_v1::CardPlayTargetV1::AllEnemies
        );
        assert_eq!(shockwave.current.vulnerable, 3);
        assert_eq!(shockwave.current.player_vulnerable, 0);

        let AgentGameFactV1::Card {
            definition: flex, ..
        } = &facts[1]
        else {
            panic!("expected Flex facts")
        };
        assert_eq!(flex.current.temporary_strength_gain, 2);
        assert_eq!(flex.current.strength_loss_at_end_of_turn, 2);

        let AgentGameFactV1::Card {
            definition: berserk,
            ..
        } = &facts[2]
        else {
            panic!("expected Berserk facts")
        };
        assert_eq!(berserk.current.player_vulnerable, 2);
        assert_eq!(berserk.current.vulnerable, 0);
        assert_eq!(berserk.current.energy_gain_each_turn, 1);

        let AgentGameFactV1::Card {
            definition: feed, ..
        } = &facts[3]
        else {
            panic!("expected Feed facts")
        };
        assert_eq!(feed.current.max_hp_gain_on_fatal, 3);
        assert_eq!(feed.current.current_hp_gain_on_fatal, 3);

        let AgentGameFactV1::Relic {
            definition: pellets,
        } = &facts[4]
        else {
            panic!("expected Orange Pellets facts")
        };
        assert_eq!(pellets.relic, RelicId::OrangePellets);
        assert!(pellets.removes_all_player_debuffs);
        assert_eq!(
            pellets.requirements,
            vec![sts_oracle_learning::ai::card_semantics_v1::AcquisitionRequirementV1::AttackSkillPowerSameTurn]
        );
    }

    #[test]
    fn game_facts_keep_boss_relic_tradeoffs_and_owned_potion_effects_typed() {
        let facts = agent_game_facts_v1(&[
            AgentGameFactEntityV1::Relic {
                relic: RelicId::SacredBark,
            },
            AgentGameFactEntityV1::Relic {
                relic: RelicId::SneckoEye,
            },
            AgentGameFactEntityV1::Relic {
                relic: RelicId::Sozu,
            },
            AgentGameFactEntityV1::Potion {
                potion: PotionId::PowerPotion,
            },
            AgentGameFactEntityV1::Potion {
                potion: PotionId::EssenceOfSteel,
            },
        ]);

        let value = serde_json::to_value(facts).expect("game facts json");
        assert_eq!(
            value[0]["definition"]["potion_effect_rule"]["potency_multiplier"],
            2
        );
        assert_eq!(
            value[1]["definition"]["additional_cards_drawn_each_turn"],
            2
        );
        assert_eq!(
            value[1]["definition"]["drawn_card_cost_randomization"],
            serde_json::json!({
                "minimum_cost": 0,
                "maximum_cost": 3,
                "eligible_cost": "nonnegative_combat_cost",
                "turn_cost_update": "only_when_combat_cost_changes",
                "clears_free_to_play_once": true,
            })
        );
        assert_eq!(value[2]["definition"]["energy_per_turn_delta"], 1);
        assert_eq!(
            value[2]["definition"]["potion_acquisition_rule"],
            serde_json::json!({
                "prevents_new_potions": true,
                "owned_potions_remain_usable": true,
            })
        );
        assert_eq!(value[3]["definition"]["base_potency"], 1);
        assert_eq!(
            value[3]["definition"]["card_discovery"],
            serde_json::json!({
                "pool": "power",
                "offered_choices": 3,
                "base_selected_card_copies": 1,
                "selected_card_cost_this_turn": 0,
                "can_skip": true,
            })
        );
        assert_eq!(value[4]["definition"]["base_potency"], 4);
        assert_eq!(
            value[4]["definition"]["mechanic_roles"],
            serde_json::json!(["GainPlatedArmor"])
        );
    }

    #[test]
    fn game_facts_serialize_relic_healing_and_hp_loss_draw_triggers() {
        let facts = agent_game_facts_v1(&[
            AgentGameFactEntityV1::Relic {
                relic: RelicId::BurningBlood,
            },
            AgentGameFactEntityV1::Relic {
                relic: RelicId::RunicCube,
            },
            AgentGameFactEntityV1::Relic {
                relic: RelicId::Ectoplasm,
            },
        ]);
        let value = serde_json::to_value(facts).expect("game facts json");

        assert_eq!(
            value[0]["definition"]["combat_victory_healing"],
            serde_json::json!({
                "base_hp": 6,
                "requires_player_alive": true,
                "capped_at_max_hp": true,
                "affected_by_healing_modifiers": true,
            })
        );
        assert_eq!(
            value[1]["definition"]["card_draw_on_hp_loss"],
            serde_json::json!({
                "minimum_hp_lost_per_event": 1,
                "cards_drawn_per_event": 1,
            })
        );
        assert_eq!(
            value[2]["definition"]["gold_gain_rule"],
            "prevent_gold_gain"
        );
        assert_eq!(value[2]["definition"]["energy_per_turn_delta"], 1);
    }

    #[test]
    fn boss_relic_hp_loss_sources_preserve_independent_owned_card_rules() {
        let group = |card, upgrades, count| AgentStrategicCardGroupV1 {
            card,
            upgrades,
            count,
            misc_value: 0,
            base_damage_override: None,
            base_block_override: None,
            cost_modifier: 0,
        };
        let cards = vec![
            group(CardId::Hemokinesis, 1, 2),
            group(CardId::Bloodletting, 0, 1),
            group(CardId::Brutality, 0, 1),
            group(CardId::Combust, 1, 1),
            group(CardId::Strike, 0, 5),
            group(CardId::Offering, 0, 0),
        ];
        let sources = project_agent_hp_loss_sources_v1(&cards);
        assert_eq!(sources.len(), 4);
        assert_eq!(sources[0].card, cards[0]);
        assert_eq!(sources[0].on_play.unwrap().base_hp_loss, 2);
        assert_eq!(sources[1].on_play.unwrap().base_hp_loss, 3);
        assert!(sources[2].on_play.is_none());
        assert_eq!(sources[2].hp_loss_each_turn_after_deployment, 1);
        assert_eq!(
            sources[3]
                .end_of_turn_after_deployment
                .unwrap()
                .hp_loss_per_application,
            1
        );
        let only_on_play = project_agent_hp_loss_sources_v1(&cards[..2]);
        assert_eq!(only_on_play.len(), 2);
        assert!(only_on_play.iter().all(|source| source.on_play.is_some()));
        let evidence = AgentDecisionEvidenceV1::BossRelicOptions {
            candidates: vec![AgentRelicAcquisitionEvidenceV1 {
                semantic_action: "boss-relic:RunicCube#0".to_string(),
                definition: relic_acquisition_mechanics_facts_v1(RelicId::RunicCube),
            }],
            owned_hp_loss_sources: only_on_play,
        };
        let wire = serde_json::to_value(evidence).unwrap();
        assert_eq!(wire["kind"], "boss_relic_options");
        assert_eq!(wire["owned_hp_loss_sources"][0]["card"]["upgrades"], 1);
        assert!(wire["candidates"][0].get("owner_rank").is_none());
    }

    #[test]
    fn capability_output_separates_permissions_from_available_operations() {
        let capabilities =
            AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(Some(
                "ironclad-a20-v1".to_string(),
            )));
        let value = serde_json::to_value(capabilities).expect("capabilities json");
        assert_eq!(
            value["permission_envelope"]["exact_replay_ceiling"],
            "unlimited_allowed"
        );
        assert_eq!(value["strategic_advice_authority"], "shadow_advisor_only");
        assert_eq!(
            value["combat_satisfaction_role"],
            "bounded_search_request_not_strategic_rank"
        );
        assert_eq!(
            value["available_commands"],
            serde_json::json!([
                "decision",
                "choose",
                "hello",
                "observe",
                "game_facts",
                "info",
                "sl_piles",
                "probe",
                "combat_research",
                "combat_review",
                "combat_plans",
                "combat_milestone_successor_details",
                "combat_milestone_continuation_audit",
                "combat_prefix_horizon_audit",
                "combat_draft_status",
                "combat_draft_open",
                "combat_draft_advance",
                "combat_draft_inspect",
                "combat_draft_choose",
                "combat_draft_undo",
                "combat_draft_selection",
                "commit_combat_plan",
                "commit_combat_milestone",
                "combat_turn_research",
                "combat_turn_reserve",
                "combat_turn_candidate_details",
                "combat_turn_continuation_audit",
                "commit_combat_turn",
                "act",
                "quit"
            ])
        );
    }

    #[test]
    fn selection_intention_requests_distinguish_read_withdrawal_and_zero_card_preference() {
        let request = |edit: Option<serde_json::Value>| {
            let mut value = serde_json::json!({"schema": "sts-run-play-agent-request-v1", "request_id": "selection", "command": "combat_draft_selection"});
            if let Some(edit) = edit {
                value["edit"] = edit;
            }
            serde_json::from_value::<AgentShellRequestV1>(value)
        };
        assert!(matches!(
            request(None).unwrap(),
            AgentShellRequestV1::CombatDraftSelection { edit: None, .. }
        ));
        for preferences in [
            serde_json::json!([]),
            serde_json::json!([{"family":0,"choices":[]}]),
            serde_json::json!([{"family":0,"choices":[2,1]}]),
        ] {
            let parsed = request(Some(
                serde_json::json!({"source_ref":"S0","preferences":preferences}),
            ))
            .unwrap();
            parsed.validate().unwrap();
            let encoded = serde_json::to_value(parsed).unwrap();
            assert_eq!(encoded["edit"]["preferences"], preferences);
        }
        assert!(request(Some(serde_json::json!({"preferences":[]}))).is_err());
        assert!(request(Some(serde_json::json!({"source_ref":"S0","preferences":[{"family":0,"choices":[1],"lineage":22}]}))).is_err());
        for source in ["", "S01", "S-1", "S0 "] {
            assert!(request(Some(
                serde_json::json!({"source_ref":source,"preferences":[]})
            ))
            .unwrap()
            .validate()
            .is_err());
        }
        assert!(request(Some(serde_json::json!({"source_ref":"S0","preferences":vec![serde_json::json!({"family":0,"choices":[]});5]}))).unwrap().validate().is_err());
    }

    #[test]
    fn nested_selection_requests_keep_index_inspection_and_edit_distinct() {
        for (fields, valid) in [
            (serde_json::json!({"offset":0}), true),
            (serde_json::json!({"offset":8}), true),
            (serde_json::json!({"source_ref":"S9"}), true),
            (serde_json::json!({"source_ref":"S09"}), false),
            (serde_json::json!({"source_ref":"S0","offset":0}), true),
            (
                serde_json::json!({"source_ref":"S0","edit":{"source_ref":"S0","preferences":[]}}),
                false,
            ),
        ] {
            let mut request = serde_json::json!({"schema":"sts-run-play-agent-request-v1","request_id":"nested","command":"combat_draft_selection"});
            request
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let parsed: AgentShellRequestV1 = serde_json::from_value(request).unwrap();
            assert_eq!(parsed.validate().is_ok(), valid, "{fields}");
        }
    }

    #[test]
    fn observation_schema_has_no_private_recovery_fields() {
        let fields = serde_json::to_value(AgentPositionV1::Terminal {
            outcome: TaskEpisodeOutcomeV1::Success {
                game_outcome: sts_oracle_learning::agent::task::GameOutcomeV1::ActThreeVictory,
            },
        })
        .expect("terminal position json")
        .to_string();
        for forbidden in [
            "checkpoint",
            "archive_node",
            "rng",
            "exact_handle",
            "ordinal_path",
        ] {
            assert!(!fields.contains(forbidden), "leaked {forbidden}");
        }
    }

    fn terminal_head_fixture() -> RunPlayHeadV1 {
        RunPlayHeadV1 {
            schema: crate::model::RUN_PLAY_HEAD_SCHEMA_V1.to_string(),
            scratch: false,
            scratch_parent_revision: None,
            scratch_floor_ends: Vec::new(),
            scratch_marks: Vec::new(),
            scratch_return_revision: None,
            evidence_identity: None,
            session_id: "terminal-fixture".to_string(),
            head_revision: 4,
            state_revision: 3,
            delta_revisions: Vec::new(),
            position: RunPlayPositionV1::Terminal {
                archive_node_id: "terminal".to_string(),
                outcome: TaskEpisodeOutcomeV1::Success {
                    game_outcome: sts_oracle_learning::agent::task::GameOutcomeV1::ActThreeVictory,
                },
            },
            default_budget: sts_oracle_learning::agent::task::TaskBudgetV1 {
                max_world_transitions: 1,
                max_reloads: 0,
            },
            combat_plan_options: CombatResearchPlanOptionsV1::default(),
            current_decision: None,
            current_path: Vec::new(),
            events: Vec::new(),
        }
    }

    #[test]
    fn terminal_head_has_no_card_acquisition_evidence() {
        let head = terminal_head_fixture();

        assert_eq!(
            project_agent_card_acquisition_evidence_v1(&head, Vec::new()),
            Ok(None)
        );
    }

    #[test]
    fn singing_bowl_evidence_uses_native_healing_and_preserves_the_root() {
        use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
        use sts_oracle_learning::content::relics::RelicState;

        let dir = TestDirectory::new();
        for (name, blocked, injured) in [
            ("bowl-full", false, false),
            ("bowl-injured", false, true),
            ("bowl-bloom", true, true),
        ] {
            let mut session = fixture("reward");
            session
                .run_state
                .relics
                .push(RelicState::new(RelicId::SingingBowl));
            if blocked {
                session
                    .run_state
                    .relics
                    .push(RelicState::new(RelicId::MarkOfTheBloom));
            }
            if injured {
                session.run_state.current_hp -= 10;
            }
            let mut controller = controller_with_session(&dir.0, name, &session);
            let before = controller.head().clone();
            let capabilities = AgentCapabilitiesV1::new(
                AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None),
            );
            let observed =
                current_agent_observation_v1(&mut controller, capabilities, Locale::En).unwrap();
            let Some(AgentDecisionEvidenceV1::CardAcquisitionOptions {
                resource_options,
                candidates,
                ..
            }) = observed.decision_evidence
            else {
                panic!("expected reward evidence");
            };
            assert_eq!(candidates.len(), 3);
            assert_eq!(resource_options.len(), 1);
            assert_eq!(resource_options[0].semantic_action, "singing-bowl#0");
            assert_eq!(resource_options[0].max_hp_gain, 2);
            assert_eq!(resource_options[0].hp_gain, if blocked { 0 } else { 2 });
            assert_eq!(
                controller.head(),
                &before,
                "observation must retain the source"
            );
        }
    }

    #[test]
    fn terminal_agent_transport_observes_without_reconstructing_a_decision() {
        let head = terminal_head_fixture();
        let reading = crate::decision::read_decision(&head, &[], None, Locale::En).unwrap();
        assert_eq!(reading.summary, reading.view);
        assert!(reading.summary.starts_with("Terminal | "));
        let capabilities =
            AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None));
        let input = [
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"hello","command":"hello"}"#,
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"observe","command":"observe"}"#,
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"act","command":"act","actions":["skip-card#0"]}"#,
            r#"{"schema":"sts-run-play-agent-request-v1","request_id":"quit","command":"quit"}"#,
        ]
        .join("\n");
        let mut output = Vec::new();

        run_terminal_agent_jsonl_v1(
            &head,
            capabilities,
            std::io::Cursor::new(input),
            &mut output,
            Locale::En,
        )
        .expect("terminal transport");

        let rendered = String::from_utf8(output).expect("utf8 response");
        let responses = rendered
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("response"))
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 4);
        assert_eq!(responses[0]["status"], "ok");
        assert_eq!(responses[0]["result"]["kind"], "hello");
        assert_eq!(responses[1]["status"], "ok");
        assert_eq!(
            responses[1]["result"]["observation"]["position"]["kind"],
            "terminal"
        );
        assert_eq!(responses[2]["status"], "error");
        assert_eq!(responses[2]["error"]["code"], "command_failed");
        assert_eq!(responses[3]["status"], "ok");
        assert_eq!(responses[3]["result"]["kind"], "quitting");
    }

    #[test]
    fn route_evidence_exposes_public_path_facts_without_owner_ranking() {
        let path = RoutePolicyPathEvidenceV1 {
            observed_path_count: 3,
            first_shop_floor: Some(2),
            min_early_pressure: 1,
            max_early_pressure: 2,
            min_shops: 0,
            max_shops: 1,
            first_elite: RouteFirstEliteSegment {
                paths_with_first_elite: 3,
                forced: true,
                min_shops_before: 0,
                max_shops_before: 1,
                min_campfires_before: 1,
                max_campfires_before: 2,
                ..RouteFirstEliteSegment::default()
            },
            ..RoutePolicyPathEvidenceV1::default()
        };
        let evidence = AgentDecisionEvidenceV1::RoutePaths {
            route_semantics_manifest: route_policy_semantics_manifest_v1(),
            candidates: vec![AgentRoutePathEvidenceV1 {
                semantic_action: "route:6.0".to_string(),
                x: 6,
                y: 0,
                arrival: RoutePolicyArrivalV1::Combat,
                question_mark_outcomes: Vec::new(),
                route_semantics: route_policy_semantics_v1(&path),
                path,
            }],
        };
        let value = serde_json::to_value(evidence).expect("route evidence json");
        assert_eq!(value["kind"], "route_paths");
        assert_eq!(
            value["route_semantics_manifest"]["raw_path_fields_represented"][0],
            "observed_path_count"
        );
        assert_eq!(value["candidates"][0]["semantic_action"], "route:6.0");
        assert_eq!(value["candidates"][0]["path"]["first_shop_floor"], 2);
        let semantics = &value["candidates"][0]["route_semantics"];
        assert_eq!(
            semantics["early_combat_rooms_on_map_rows_0_through_3"],
            serde_json::json!({"min": 1, "max": 2})
        );
        assert_eq!(semantics["shops"]["presence"], "some_observed_paths");
        assert_eq!(semantics["shops"]["earliest_possible_floor"], 2);
        assert_eq!(semantics["first_elite"]["presence"], "all_observed_paths");
        assert_eq!(
            semantics["first_elite"]["shops_before"]["presence"],
            "some_observed_paths"
        );
        assert_eq!(
            semantics["first_elite"]["campfires_before"]["presence"],
            "all_observed_paths"
        );
        assert_eq!(
            semantics["first_elite"]["before_counts_scope"],
            "observed_paths_that_reach_a_first_elite"
        );
        assert!(semantics.get("raw_path_fields_represented").is_none());
        assert!(value["candidates"][0].get("owner_rank").is_none());
        assert!(value["candidates"][0].get("prior_probability").is_none());
        assert!(value["candidates"][0].get("band").is_none());
    }

    #[test]
    fn route_entry_evidence_has_no_invented_action_or_policy_rank() {
        let path = RoutePathFamilySummary {
            path_count: 3,
            first_shop_floor: Some(5),
            min_shops: 0,
            max_shops: 1,
            ..RoutePathFamilySummary::default()
        };
        let evidence = AgentDecisionEvidenceV1::RouteEntryPaths {
            route_semantics_manifest: route_entry_semantics_manifest_v1(),
            candidates: vec![AgentRouteEntryPathEvidenceV1 {
                x: 2,
                y: 0,
                room: Some(PlannerMapRoom::Monster),
                has_emerald_key: false,
                uses_wing_boots: false,
                coverage: RouteWindowCoverageKind::CompleteWithinHorizon,
                route_semantics: route_entry_semantics_v1(&path),
                path,
            }],
            limited_combat_coverages: Vec::new(),
        };

        let value = serde_json::to_value(evidence).expect("route entry evidence json");

        assert_eq!(value["kind"], "route_entry_paths");
        assert_eq!(
            value["route_semantics_manifest"]["raw_path_fields_represented"][0],
            "path_count"
        );
        assert_eq!(value["candidates"][0]["x"], 2);
        assert_eq!(value["candidates"][0]["path"]["path_count"], 3);
        assert!(value["candidates"][0].get("semantic_action").is_none());
        assert!(value["candidates"][0].get("owner_rank").is_none());
        assert!(value["candidates"][0].get("prior_probability").is_none());
    }

    #[test]
    fn card_acquisition_route_entries_preserve_zero_bounds_without_policy_rank() {
        let path = RoutePathFamilySummary {
            path_count: 129,
            path_budget_exhausted: false,
            min_elites: 0,
            max_elites: 2,
            min_campfires: 1,
            max_campfires: 5,
            ..RoutePathFamilySummary::default()
        };
        let evidence = AgentDecisionEvidenceV1::CardAcquisitionOptions {
            resource_options: Vec::new(),
            candidates: vec![AgentCardAcquisitionEvidenceV1 {
                semantic_action: "take:SearingBlow+0#0".to_string(),
                cards: vec![card_acquisition_mechanics_facts_v1(&RewardCard::new(
                    CardId::SearingBlow,
                    0,
                ))],
            }],
            route_semantics_manifest: route_entry_semantics_manifest_v1(),
            route_entries: vec![AgentRouteEntryPathEvidenceV1 {
                x: 5,
                y: 3,
                room: Some(PlannerMapRoom::QuestionMark),
                has_emerald_key: false,
                uses_wing_boots: false,
                coverage: RouteWindowCoverageKind::CompleteWithinHorizon,
                route_semantics: route_entry_semantics_v1(&path),
                path,
            }],
        };

        let value = serde_json::to_value(evidence).expect("card acquisition evidence json");
        let entry = &value["route_entries"][0];

        assert_eq!(value["kind"], "card_acquisition_options");
        assert_eq!(entry["path"]["path_count"], 129);
        assert_eq!(entry["path"]["path_budget_exhausted"], false);
        assert_eq!(entry["path"]["min_elites"], 0);
        assert_eq!(entry["path"]["max_elites"], 2);
        assert_eq!(entry["path"]["min_campfires"], 1);
        assert_eq!(entry["path"]["max_campfires"], 5);
        assert!(entry.get("semantic_action").is_none());
        assert!(entry.get("owner_rank").is_none());
        assert!(entry.get("prior_probability").is_none());
    }

    #[test]
    fn limited_combat_coverage_joins_charges_to_first_elite_timing() {
        let entry = |x, min_fights, max_fights, max_unknowns| {
            let path = RoutePathFamilySummary {
                path_count: 1,
                first_elite: sts_oracle_learning::ai::route_window_facts::RouteFirstEliteSegment {
                    paths_with_first_elite: 1,
                    min_hallway_fights_before: min_fights,
                    max_hallway_fights_before: max_fights,
                    max_unknowns_before: max_unknowns,
                    ..Default::default()
                },
                ..Default::default()
            };
            AgentRouteEntryPathEvidenceV1 {
                x,
                y: 0,
                room: Some(PlannerMapRoom::Monster),
                has_emerald_key: false,
                uses_wing_boots: false,
                coverage: RouteWindowCoverageKind::CompleteWithinHorizon,
                route_semantics: route_entry_semantics_v1(&path),
                path,
            }
        };

        let impossible = first_elite_combat_coverage_v1(&entry(0, 4, 4, 1), 3);
        let conditional = first_elite_combat_coverage_v1(&entry(2, 2, 3, 3), 3);

        assert_eq!(
            impossible.first_elite_coverage,
            AgentFirstEliteCombatCoverageV1::CannotCover
        );
        assert_eq!(impossible.earliest_first_elite_combat_number, Some(5));
        assert_eq!(
            conditional.first_elite_coverage,
            AgentFirstEliteCombatCoverageV1::MayCover
        );
        assert_eq!(conditional.earliest_first_elite_combat_number, Some(3));
        assert_eq!(
            conditional.latest_first_elite_combat_number_if_all_prior_unknowns_are_combats,
            Some(7)
        );
    }

    #[test]
    fn campfire_evidence_exposes_upgrade_mechanics_without_owner_ranking() {
        let evidence = AgentDecisionEvidenceV1::CampfireOptions {
            candidates: vec![AgentCampfireOptionEvidenceV1 {
                semantic_actions: vec!["smith:Whirlwind+0#0".to_string()],
                mechanics: AgentCampfireMechanicsV1::Smith {
                    before: card_reward_facts_v1(&RewardCard::new(CardId::Whirlwind, 0)),
                    after: card_reward_facts_v1(&RewardCard::new(CardId::Whirlwind, 1)),
                },
            }],
        };
        let value = serde_json::to_value(evidence).expect("campfire evidence json");
        let candidate = &value["candidates"][0];
        assert_eq!(value["kind"], "campfire_options");
        assert_eq!(
            candidate["semantic_actions"],
            serde_json::json!(["smith:Whirlwind+0#0"])
        );
        assert_eq!(candidate["before"]["cost"], -1);
        assert_eq!(candidate["after"]["cost"], -1);
        assert_eq!(candidate["before"]["damage"]["damage_per_hit"], 5);
        assert_eq!(candidate["after"]["damage"]["damage_per_hit"], 8);
        assert!(candidate.get("owner_rank").is_none());
        assert!(candidate.get("prior_probability").is_none());
        assert!(candidate.get("band").is_none());
    }

    #[test]
    fn campfire_evidence_ignores_orthogonal_potion_management_actions() {
        use sts_oracle_learning::ai::route_window_facts::RouteWindowModality;
        use sts_oracle_learning::content::potions::PotionId;
        use sts_oracle_learning::eval::run_control::CampfireRecoveryContextV1;

        let audit = ExactCampfirePolicyAuditV1 {
            current_hp: 35,
            max_hp: 78,
            recovery: CampfireRecoveryContextV1 {
                boss_nodes_remaining: None,
                rest_heal_capacity: 0,
                known_combat_before_next_campfire: RouteWindowModality::Unknown,
                known_elite_before_next_campfire: RouteWindowModality::Unknown,
            },
            candidates: Vec::new(),
        };
        for action in [
            TaskStrategicActionV1::UseRunPotion {
                slot: 0,
                potion: PotionId::FruitJuice,
            },
            TaskStrategicActionV1::DiscardRunPotion {
                slot: 0,
                potion: PotionId::FirePotion,
            },
        ] {
            assert_eq!(
                project_agent_campfire_mechanics_v1(&action, &audit, 0)
                    .expect("ambient potion action"),
                None
            );
        }
    }

    #[test]
    fn event_card_acquisition_uses_exact_upgrade_sensitive_facts() {
        let action = TaskStrategicActionV1::ChooseEventOption {
            event: sts_oracle_learning::state::events::EventId::TheLibrary,
            screen: 1,
            option_index: 5,
            action: sts_oracle_learning::state::events::EventActionKind::Gain,
            public_semantics:
                sts_oracle_learning::ai::planner_core::PlannerEventOptionPublicSemanticsV1 {
                    cards_gained: 1,
                    specific_cards_gained: vec![
                        sts_oracle_learning::ai::planner_core::PlannerOfferedCard {
                            card: CardId::Brutality,
                            upgrades: 0,
                        },
                    ],
                    ..Default::default()
                },
        };

        let cards = acquired_card_mechanics_for_action_v1(&action);

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].current.card, CardId::Brutality);
        assert_eq!(cards[0].current.draw_cards_each_turn, 1);
        let upgraded = cards[0].upgraded.as_ref().expect("upgrade facts");
        assert_eq!(upgraded.draw_cards_each_turn, 1);
        assert!(!cards[0].current.innate);
        assert!(upgraded.innate);
    }
}
