use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use sts_presentation::Locale;

use serde::{Deserialize, Serialize};
use sts_combat_planner::{
    CombatDecisionRoot, LocalTurnGraphSearchSpecV1, LocalTurnGraphTerminalOutcomeSnapshotV1,
    LocalTurnGraphWitnessInterruption, LocalTurnGraphWitnessSession, LocalTurnGraphWitnessStatus,
    OracleCombatDeepStateSnapshot, OracleCombatWitnessProgressSnapshot,
    OracleCombatWitnessSatisfaction, TurnOptionAction,
};
use sts_combat_strategy::{combat_plan_projection_v1, CombatPlanMilestoneV1};
use sts_complete_turn_env::complete_turn_surface_with_config_v1;
use sts_complete_turn_proposal::{
    ExactCompleteTurnProposalBoundaryV1, ExactCompleteTurnProposalConfigV1,
    ExactCompleteTurnProposalV1,
};
use sts_complete_turn_view::{
    complete_turn_candidate_id_v1, project_complete_turn_candidate_v1, CompleteTurnCandidateViewV1,
};
use sts_oracle_learning::agent::information::action::PublicCombatSelectionDomainCandidateV1;
use sts_oracle_learning::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_learning::agent::information::state::{
    CombatLearningEnemyIdentityV1, PublicCombatStateV1,
};
use sts_oracle_learning::agent::task::{
    GameOutcomeV1, TaskActorBoundaryV1, TaskBudgetV1, TaskDecisionSiteV1, TaskEpisodeFailureV1,
    TaskEpisodeOutcomeV1, TaskObjectiveV1, TaskObservedBoundaryV1,
    TaskPrivilegedStrategicSupportOutcomeV1, TaskPrivilegedSupportAccessV1,
    TaskPrivilegedSupportProvenanceV1, TaskPublicCandidateV1, TaskPublicChoiceV1,
    TaskPublicDecisionV1, TaskPublicObservationV1, TaskPublicStepV1, TaskResearchPlanAttemptV1,
    TaskResearchPlanOutcomeV1, TaskRunSolutionArchiveV1, TaskStrategicActionV1,
    TaskStrategicCardV1,
};
use sts_oracle_learning::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_learning::ai::planner_core::{
    stable_planner_id, PlannerMapRoom, PlannerRewardDescriptor,
};
use sts_oracle_learning::ai::route_window_facts::RouteEntryPathSummaryV1;
use sts_oracle_learning::content::cards::CardId;
use sts_oracle_learning::content::potions::PotionId;
use sts_oracle_learning::eval::combat_case::CombatCase;
use sts_oracle_learning::eval::run_control::{
    exact_run_decision_root_artifacts_v1, natural_run_root_artifacts_v1,
    resume_run_until_next_public_decision_v1, resume_run_until_next_strategic_decision_v1,
    retained_combat_tactical_policy_v1, run_control_continuation_fingerprint_v1,
    run_control_session_fingerprint_v3, task_run_branch_decision_in_validated_session_v1,
    task_run_branch_decision_v1, task_run_paused_decision_node_id_v1,
    CombatMilestoneResearchCandidateV1, CombatResearchBoundedProgressMaterializationV1,
    CombatResearchBoundedProgressPathV1, CombatResearchBoundedProgressStateV1,
    CombatResearchPlanOptionsV1, CombatResearchProgressEventV1,
    CombatResearchReplayMaterializationV1, ExactBossRelicPolicyAuditV1,
    ExactBranchingTaskEvidenceRequestV1, ExactCampfirePolicyAuditV1, ExactCardRewardPolicyAuditV1,
    ExactRoutePolicyAuditV1, ExactRunChoicePolicyAuditV1, ExactRunDecisionSourceV1,
    ExactRunPlayAdvanceV1, ExactRunPlayHotSnapshotV1, ExactRunPlayOpenProfileV1,
    ExactRunPlayPreparedLineProfileV1, ExactRunPlayPreparedLineV1, ExactRunPlayProbeLineV1,
    ExactRunPlayProbeStepV1, ExactRunPlaySessionV1, ExactShopPolicyAuditV1,
    PotionRewardPolicyAuditV1, RunControlSessionCheckpointV1, TaskRunBranchCheckpointArtifactV1,
    TaskRunBranchCheckpointDecodeProfileV1, TaskRunBranchDecisionInventoryV1,
};
use sts_oracle_learning::sim::combat::{combat_terminal, CombatTerminal, EngineCombatStepper};
use sts_oracle_learning::sim::combat::{CombatPosition, CombatStepper};
use sts_oracle_learning::state::core::ClientInput;

use crate::model::{
    RunPlayBoundaryKindV1, RunPlayCombatMilestoneCandidateV1, RunPlayCombatParityCardV1,
    RunPlayCombatParityMonsterV1, RunPlayCombatParityPowerV1, RunPlayCombatParityProjectionV1,
    RunPlayCombatResearchCandidateV1, RunPlayDeltaV1, RunPlayEventV1, RunPlayEvidenceIdentityV1,
    RunPlayHeadV1, RunPlayPathStepV1, RunPlayPositionV1, RunPlayPublicScalarsV1,
    RunPlayScratchFloorEndV1, RunPlayStateV1, RunPlayStrategicOwnerAdviceV1,
    RUN_PLAY_DELTA_SCHEMA_V1, RUN_PLAY_HEAD_SCHEMA_V1, RUN_PLAY_STATE_SCHEMA_V1,
};
use crate::proposal_audit::{
    audit_complete_turn_target_v1, reserve_complete_turn_target_v1, RunPlayProposalAuditV1,
};
use crate::selector::{
    resolve_action_selector_v1, semantic_action_for_choice_v1, RunPlayActionSelectorV1,
};
use crate::storage::{
    read_bounded, write_new_synced, RunPlayPathsV1, RunPlayWorkspaceLoadProfileV1,
    RunPlayWorkspaceV1, DEFAULT_ARTIFACT_MAX_BYTES,
};

const RUN_PLAY_REROOT_CAPSULE_SCHEMA_V1: &str = "sts-run-play-reroot-capsule-v1";

fn reject_unrevealed_question_mark_probe_v1(
    decision: &TaskPublicDecisionV1,
    choice: &TaskPublicChoiceV1,
) -> Result<(), String> {
    let Some(TaskPublicCandidateV1::Strategic {
        action: TaskStrategicActionV1::ChooseRouteNode { x, y, .. },
    }) = choice
        .ordinal_path
        .first()
        .and_then(|ordinal| decision.candidates.get(*ordinal))
    else {
        return Ok(());
    };
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err("route probe has no public map observation".to_string());
    };
    let room = observation
        .public_map
        .nodes
        .iter()
        .find(|node| node.x == *x && node.y == *y)
        .map(|node| node.room);
    match room {
        Some(PlannerMapRoom::QuestionMark) => {
            Err("preview cannot enter an unrevealed ? room; use act to enter it".to_string())
        }
        Some(_) => Ok(()),
        None => Err("route probe target is absent from the public map".to_string()),
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayRerootCapsuleV1 {
    schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_evidence_identity: Option<RunPlayEvidenceIdentityV1>,
    source_session_id: String,
    source_head_revision: u64,
    source_state_revision: u64,
    source_archive_node_id: String,
    source_public_snapshot_id: String,
    source_seed: u64,
    source_act: u8,
    source_floor: i32,
    source_ascension_level: u8,
    source_objective: TaskObjectiveV1,
    exact_state_fingerprint: String,
    default_budget: TaskBudgetV1,
    combat_plan_options: CombatResearchPlanOptionsV1,
    checkpoint: RunControlSessionCheckpointV1,
}

impl RunPlayRerootCapsuleV1 {
    fn validate(&self) -> Result<(), String> {
        if self.schema != RUN_PLAY_REROOT_CAPSULE_SCHEMA_V1 {
            return Err("run-play reroot capsule schema is unsupported".to_string());
        }
        for (description, value) in [
            ("source session id", self.source_session_id.as_str()),
            (
                "source archive node id",
                self.source_archive_node_id.as_str(),
            ),
            (
                "source public snapshot id",
                self.source_public_snapshot_id.as_str(),
            ),
            (
                "exact state fingerprint",
                self.exact_state_fingerprint.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(format!(
                    "run-play reroot capsule has an empty {description}"
                ));
            }
        }
        self.default_budget.validate()?;
        self.combat_plan_options.validate()?;
        let session = self.checkpoint.clone().into_session()?;
        if session.run_state.seed != self.source_seed
            || session.run_state.act_num != self.source_act
            || session.run_state.floor_num != self.source_floor
            || session.run_state.ascension_level != self.source_ascension_level
        {
            return Err(
                "run-play reroot capsule source scalars disagree with its exact checkpoint"
                    .to_string(),
            );
        }
        if run_control_session_fingerprint_v3(&session) != self.exact_state_fingerprint {
            return Err("run-play reroot capsule exact state fingerprint is invalid".to_string());
        }
        Ok(())
    }
}

const MAX_COMPARISON_LINES: usize = 16;
// Complete-combat witnesses can legitimately exceed one busy turn's 64-action
// reservation bound. Keep exact comparison and atomic publication aligned
// with the existing bounded combat-prefix proof envelope.
const MAX_COMPARISON_STEPS_PER_LINE: usize = 256;
pub(crate) const MAX_ANCHOR_LINE_STEPS: usize = MAX_COMPARISON_STEPS_PER_LINE;
const MAX_BRANCH_PROMOTION_STEPS: usize = 256;
const MAX_EVENT_FRONTIER_LINES: usize = 64;
const MAX_EVENT_FRONTIER_STEPS: usize = 12;
const MAX_COMBAT_PREFIX_HORIZON_TURNS: usize = 2;
const MAX_COMBAT_PREFIX_HORIZON_PUBLIC_SUCCESSORS: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct RunPlayRetainedSemanticPrefixStepV1 {
    pub source_depth: usize,
    pub expected_site: TaskDecisionSiteV1,
    pub expected_before: RunPlayPublicScalarsV1,
    pub selector: RunPlayActionSelectorV1,
}

pub(crate) fn resolve_combat_research_candidate_index_v1(
    candidate_producers: &[Vec<&str>],
    producer_name: &str,
    requested_index: Option<usize>,
) -> Result<usize, String> {
    if let Some(index) = requested_index {
        let actual_producer = candidate_producers.get(index).ok_or_else(|| {
            format!(
                "combat research candidate {index} is out of range; candidate_count={}",
                candidate_producers.len()
            )
        })?;
        if !actual_producer.contains(&producer_name) {
            return Err(format!(
                "combat research candidate {index} is supported by producers [{}], not {producer_name}",
                actual_producer.join(", ")
            ));
        }
        return Ok(index);
    }

    let matching_indices = candidate_producers
        .iter()
        .enumerate()
        .filter_map(|(index, actual_producers)| {
            actual_producers.contains(&producer_name).then_some(index)
        })
        .collect::<Vec<_>>();
    match matching_indices.as_slice() {
        [index] => Ok(*index),
        [] => Err(format!(
            "combat research producer {producer_name} has no candidate; available=[{}]",
            candidate_producers
                .iter()
                .map(|producers| format!("[{}]", producers.join(", ")))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => Err(format!(
            "combat research producer {producer_name} produced multiple candidates at indices [{}]; pass --candidate <index>",
            matching_indices
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunPlayCombatHistoryV1 {
    pub current_archive_node_id: String,
    pub total_combat_steps: usize,
    pub steps: Vec<RunPlayCombatHistoryStepV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunPlayCombatHistoryStepV1 {
    /// Recoverable decision before the rendered action. This is the selector
    /// an operator can pass to `jump` to reopen the exact combat boundary.
    pub from_archive_node_id: String,
    pub path: RunPlayPathStepV1,
    pub encounter: Option<String>,
    pub turn: u32,
    pub phase: String,
    pub energy: u8,
    pub player_block: i32,
    pub hand: Vec<String>,
    pub potions: Vec<String>,
    pub player_powers: Vec<String>,
    pub monsters: Vec<RunPlayCombatMonsterHistoryV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunPlayCombatMonsterHistoryV1 {
    pub slot: u8,
    pub enemy: String,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub intent: String,
    pub powers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayConstructionHistoryV1 {
    pub current_archive_node_id: String,
    pub owner_ranking_retained: bool,
    pub steps: Vec<RunPlayConstructionHistoryStepV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayConstructionHistoryStepV1 {
    pub from_archive_node_id: String,
    pub to_archive_node_id: String,
    pub depth: usize,
    pub site: TaskDecisionSiteV1,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub deck: Vec<TaskStrategicCardV1>,
    pub relics: Vec<String>,
    pub potions: Vec<String>,
    pub candidates: Vec<RunPlayConstructionCandidateV1>,
    pub choice: TaskPublicChoiceV1,
    pub selected_action: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayConstructionCandidateV1 {
    pub ordinal: usize,
    pub kind: RunPlayConstructionCandidateKindV1,
    pub action: String,
    pub selected: bool,
    pub candidate: TaskPublicCandidateV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayConstructionCandidateKindV1 {
    SiteOption,
    AmbientResource,
    Navigation,
}

pub struct RunPlayEventAdviceDetailV1 {
    pub direct_card_acquisition: Option<ExactCardRewardPolicyAuditV1>,
    pub comparison: RunPlayComparisonV1,
    pub truncated: bool,
}

pub enum RunPlayAdviceDetailV1 {
    Route(ExactRoutePolicyAuditV1),
    CardReward(ExactCardRewardPolicyAuditV1),
    Campfire(ExactCampfirePolicyAuditV1),
    BossRelic(ExactBossRelicPolicyAuditV1),
    Shop(ExactShopPolicyAuditV1),
    RunChoice(ExactRunChoicePolicyAuditV1),
    PotionReward(PotionRewardPolicyAuditV1),
    Event(RunPlayEventAdviceDetailV1),
    Unavailable { site: TaskDecisionSiteV1 },
}

pub struct RunPlayAdviceInspectionV1 {
    pub advice: RunPlayStrategicOwnerAdviceV1,
    pub decision: TaskRunBranchDecisionInventoryV1,
    pub detail: Option<RunPlayAdviceDetailV1>,
}

pub fn create_natural_session_v1(
    paths: &RunPlayPathsV1,
    config: sts_oracle_learning::eval::run_control::RunControlConfig,
    default_budget: TaskBudgetV1,
    combat_plan_options: CombatResearchPlanOptionsV1,
) -> Result<RunPlayWorkspaceV1, String> {
    create_natural_session_with_mode_v1(paths, config, default_budget, combat_plan_options, false)
}

pub fn create_scratch_natural_session_v1(
    paths: &RunPlayPathsV1,
    config: sts_oracle_learning::eval::run_control::RunControlConfig,
    default_budget: TaskBudgetV1,
    combat_plan_options: CombatResearchPlanOptionsV1,
) -> Result<RunPlayWorkspaceV1, String> {
    create_natural_session_with_mode_v1(paths, config, default_budget, combat_plan_options, true)
}

fn create_natural_session_with_mode_v1(
    paths: &RunPlayPathsV1,
    config: sts_oracle_learning::eval::run_control::RunControlConfig,
    default_budget: TaskBudgetV1,
    combat_plan_options: CombatResearchPlanOptionsV1,
    scratch: bool,
) -> Result<RunPlayWorkspaceV1, String> {
    default_budget.validate()?;
    combat_plan_options.validate()?;
    let artifacts = natural_run_root_artifacts_v1(
        config,
        default_budget,
        combat_plan_options.clone(),
        ExactBranchingTaskEvidenceRequestV1::default(),
    )?;
    let checkpoint_payload = artifacts.checkpoints.encode(DEFAULT_ARTIFACT_MAX_BYTES)?;
    create_session_from_validated_parts_v1(
        paths,
        artifacts.archive,
        artifacts.checkpoints,
        checkpoint_payload,
        None,
        default_budget,
        combat_plan_options,
        None,
        (!scratch).then(RunPlayEvidenceIdentityV1::current_v1),
        scratch,
    )
}

/// Publishes a create-new run-play source guard around one exact constructed
/// decision checkpoint.
///
/// This is not a natural run and does not inherit a historical archive.  The
/// generated one-node session exists so PairedRoot preview/recovery can reuse
/// the ordinary immutable-head guard without pretending that a scenario root
/// came from Neow-to-combat play history.
pub(crate) fn create_constructed_decision_session_v1(
    paths: &RunPlayPathsV1,
    checkpoint: RunControlSessionCheckpointV1,
    scenario_id: &str,
    default_budget: TaskBudgetV1,
    combat_plan_options: CombatResearchPlanOptionsV1,
) -> Result<RunPlayWorkspaceV1, String> {
    if scenario_id.trim().is_empty() {
        return Err("constructed decision session requires a scenario identity".to_owned());
    }
    default_budget.validate()?;
    combat_plan_options.validate()?;
    let source = ExactRunDecisionSourceV1 {
        session_id: format!("constructed-scenario:{scenario_id}"),
        head_revision: 0,
        archive_node_id: format!("constructed-scenario-root:{scenario_id}"),
        public_snapshot_id: format!("constructed-scenario-snapshot:{scenario_id}"),
    };
    let artifacts = exact_run_decision_root_artifacts_v1(
        checkpoint,
        source,
        TaskObjectiveV1::HeartVictory,
        default_budget,
        combat_plan_options.clone(),
        ExactBranchingTaskEvidenceRequestV1::default(),
    )?;
    let checkpoint_payload = artifacts.checkpoints.encode(DEFAULT_ARTIFACT_MAX_BYTES)?;
    create_session_from_validated_parts_v1(
        paths,
        artifacts.archive,
        artifacts.checkpoints,
        checkpoint_payload,
        None,
        default_budget,
        combat_plan_options,
        None,
        Some(RunPlayEvidenceIdentityV1::current_v1()),
        false,
    )
}

pub fn snapshot_current_for_reroot_v1(
    paths: RunPlayPathsV1,
    output: &Path,
) -> Result<RunPlayRerootCapsuleV1, String> {
    let capsule = snapshot_current_capsule_v1(paths, None)?;
    let payload = rmp_serde::to_vec_named(&capsule)
        .map_err(|error| format!("failed to encode run-play reroot capsule: {error}"))?;
    if payload.len() > DEFAULT_ARTIFACT_MAX_BYTES {
        return Err("run-play reroot capsule exceeds the artifact size limit".to_string());
    }
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create run-play reroot capsule directory {}: {error}",
                parent.display()
            )
        })?;
    }
    write_new_synced(output, &payload, "run-play reroot capsule")?;
    Ok(capsule)
}

/// Verify a retained decision and create one fresh current-build root.
/// The source session is read-only and no intermediate capsule is written.
pub fn reroot_current_session_v1(
    source_paths: RunPlayPathsV1,
    target_paths: &RunPlayPathsV1,
    selector: Option<&str>,
) -> Result<RunPlayWorkspaceV1, String> {
    let source_head = source_paths.load_head()?;
    if RunPlayEvidenceIdentityV1::require_current_v1(source_head.evidence_identity.as_ref()).is_ok()
    {
        return Err("run-play source is already current; use fork for an ordinary branch".into());
    }
    let capsule = snapshot_current_capsule_v1(source_paths, selector)?;
    reroot_session_from_validated_capsule_v1(capsule, target_paths)
}

fn snapshot_current_capsule_v1(
    paths: RunPlayPathsV1,
    selector: Option<&str>,
) -> Result<RunPlayRerootCapsuleV1, String> {
    // Prefer ordinary archive replay. When only the old public projection has
    // changed, an exact checkpoint at the selected head can still provide a
    // state-only reroot. Neither path publishes into the source workspace.
    let (head, objective, archive_node_id, public_snapshot_id, mut checkpoint) =
        match match selector {
            Some(selector) => {
                RunPlayControllerV1::open_at_for_read_only_snapshot_v1(paths.clone(), selector)
            }
            None => RunPlayControllerV1::open_for_read_only_snapshot_v1(paths.clone()),
        } {
            Ok(controller) => {
                let archive_node_id = controller
                    .exact
                    .current_archive_node_id()
                    .ok_or_else(|| {
                        "run-play reroot snapshot requires a public decision".to_string()
                    })?
                    .to_string();
                let public_snapshot_id = controller
                    .exact
                    .current_public_decision()
                    .ok_or_else(|| {
                        "run-play reroot snapshot has no current public decision".to_string()
                    })?
                    .information
                    .snapshot_id
                    .clone();
                let checkpoint = controller.exact.current_run_decision_checkpoint_v1()?;
                (
                    controller.head.clone(),
                    controller.exact.archive().task.objective,
                    archive_node_id,
                    public_snapshot_id,
                    checkpoint,
                )
            }
            Err(replay_error) => {
                if selector.is_some() {
                    return Err(format!(
                        "run-play historical reroot could not verify the selected decision: {replay_error}"
                    ));
                }
                let workspace = paths.load()?;
                if RunPlayEvidenceIdentityV1::require_current_v1(
                    workspace.head.evidence_identity.as_ref(),
                )
                .is_ok()
                {
                    return Err(replay_error);
                }
                let RunPlayPositionV1::Decision { archive_node_id } = &workspace.head.position
                else {
                    return Err("run-play reroot snapshot requires a public decision".into());
                };
                let archive_node_id = archive_node_id.clone();
                let (checkpoint, source_decision) =
                    TaskRunBranchCheckpointArtifactV1::selected_exact_state_for_reroot_v1(
                        &workspace.state.checkpoint_payload,
                        &workspace.state.archive,
                        &archive_node_id,
                        DEFAULT_ARTIFACT_MAX_BYTES,
                    )
                    .map_err(|error| {
                        format!(
                            "run-play archive replay failed: {replay_error}; exact-state reroot fallback failed: {error}"
                        )
                    })?;
                let cached_decision = workspace
                    .head
                    .current_decision
                    .as_ref()
                    .ok_or("run-play head has no retained public decision")?
                    .public_decision_v1()?;
                if source_decision != cached_decision {
                    return Err(
                        "run-play head and selected exact checkpoint disagree on source decision"
                            .into(),
                    );
                }
                let public_snapshot_id = source_decision.information.snapshot_id;
                (
                    workspace.head,
                    workspace.state.archive.task.objective,
                    archive_node_id,
                    public_snapshot_id,
                    checkpoint,
                )
            }
        };
    checkpoint.clear_non_gameplay_diagnostics_for_external_checkpoint();
    let session = checkpoint.clone().into_session()?;
    let capsule = RunPlayRerootCapsuleV1 {
        schema: RUN_PLAY_REROOT_CAPSULE_SCHEMA_V1.to_string(),
        source_evidence_identity: head.evidence_identity.clone(),
        source_session_id: head.session_id.clone(),
        source_head_revision: head.head_revision,
        source_state_revision: head.state_revision,
        source_archive_node_id: archive_node_id,
        source_public_snapshot_id: public_snapshot_id,
        source_seed: session.run_state.seed,
        source_act: session.run_state.act_num,
        source_floor: session.run_state.floor_num,
        source_ascension_level: session.run_state.ascension_level,
        source_objective: objective,
        exact_state_fingerprint: run_control_session_fingerprint_v3(&session),
        default_budget: head.default_budget,
        combat_plan_options: head.combat_plan_options.clone(),
        checkpoint,
    };
    capsule.validate()?;
    Ok(capsule)
}

pub fn reroot_session_from_capsule_v1(
    capsule_path: &Path,
    target_paths: &RunPlayPathsV1,
) -> Result<RunPlayWorkspaceV1, String> {
    let payload = read_bounded(
        capsule_path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run-play reroot capsule",
    )?;
    let capsule: RunPlayRerootCapsuleV1 = rmp_serde::from_slice(&payload).map_err(|error| {
        format!(
            "failed to decode run-play reroot capsule {}: {error}",
            capsule_path.display()
        )
    })?;
    reroot_session_from_validated_capsule_v1(capsule, target_paths)
}

fn reroot_session_from_validated_capsule_v1(
    capsule: RunPlayRerootCapsuleV1,
    target_paths: &RunPlayPathsV1,
) -> Result<RunPlayWorkspaceV1, String> {
    reroot_session_from_validated_capsule_with_mode_v1(capsule, target_paths, false)
}

/// One-time import of a selected old decision into disposable play. The old
/// archive is read only; only its exact game state enters the new workspace.
pub fn adopt_current_session_v1(
    source_paths: RunPlayPathsV1,
    target_paths: &RunPlayPathsV1,
) -> Result<RunPlayWorkspaceV1, String> {
    let capsule = snapshot_current_capsule_v1(source_paths, None)?;
    reroot_session_from_validated_capsule_with_mode_v1(capsule, target_paths, true)
}

fn reroot_session_from_validated_capsule_with_mode_v1(
    capsule: RunPlayRerootCapsuleV1,
    target_paths: &RunPlayPathsV1,
    scratch: bool,
) -> Result<RunPlayWorkspaceV1, String> {
    capsule.validate()?;
    let expected_exact_state_fingerprint = capsule.exact_state_fingerprint.clone();
    // The capsule retains its original producer identity. This is a new
    // exact-state root produced and projected by this build, not a relabeling
    // or continuation of the source archive. The fingerprint is checked both
    // on capsule decode and after the new root has been reconstructed.
    let rerooted_evidence_identity = (!scratch).then(RunPlayEvidenceIdentityV1::current_v1);
    let source = ExactRunDecisionSourceV1 {
        session_id: capsule.source_session_id,
        head_revision: capsule.source_head_revision,
        archive_node_id: capsule.source_archive_node_id,
        public_snapshot_id: capsule.source_public_snapshot_id,
    };
    let artifacts = exact_run_decision_root_artifacts_v1(
        capsule.checkpoint,
        source,
        capsule.source_objective,
        capsule.default_budget,
        capsule.combat_plan_options.clone(),
        ExactBranchingTaskEvidenceRequestV1::default(),
    )?;
    let checkpoint_payload = artifacts.checkpoints.encode(DEFAULT_ARTIFACT_MAX_BYTES)?;
    let workspace = create_session_from_validated_parts_v1(
        target_paths,
        artifacts.archive,
        artifacts.checkpoints,
        checkpoint_payload,
        None,
        capsule.default_budget,
        capsule.combat_plan_options,
        Some(&expected_exact_state_fingerprint),
        rerooted_evidence_identity,
        scratch,
    )?;
    Ok(workspace)
}

pub fn import_session_v1(
    paths: &RunPlayPathsV1,
    archive_path: &Path,
    checkpoints_path: &Path,
    requested_node_id: Option<&str>,
    default_budget: TaskBudgetV1,
    combat_plan_options: CombatResearchPlanOptionsV1,
) -> Result<RunPlayWorkspaceV1, String> {
    default_budget.validate()?;
    combat_plan_options.validate()?;
    let archive_payload = read_bounded(
        archive_path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run solution archive",
    )?;
    let archive: TaskRunSolutionArchiveV1 =
        serde_json::from_slice(&archive_payload).map_err(|error| {
            format!(
                "failed to decode run solution archive {}: {error}",
                archive_path.display()
            )
        })?;
    archive.validate()?;
    let source_checkpoint_payload = read_bounded(
        checkpoints_path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run branch checkpoints",
    )?;
    let checkpoints = TaskRunBranchCheckpointArtifactV1::decode_for_archive(
        &source_checkpoint_payload,
        &archive,
        DEFAULT_ARTIFACT_MAX_BYTES,
    )?;
    create_session_from_validated_parts_v1(
        paths,
        archive,
        checkpoints,
        source_checkpoint_payload,
        requested_node_id,
        default_budget,
        combat_plan_options,
        None,
        None,
        false,
    )
}

/// Creates an independent writable session from one already retained public
/// decision without moving or publishing anything in the source workspace.
/// The fork reuses the validated in-memory archive/checkpoint owners instead
/// of round-tripping through temporary export files.
pub fn fork_session_v1(
    source_paths: RunPlayPathsV1,
    target_paths: &RunPlayPathsV1,
    selector: Option<&str>,
) -> Result<RunPlayWorkspaceV1, String> {
    let controller = if let Some(selector) = selector {
        RunPlayControllerV1::open_at(source_paths, selector)?
    } else {
        RunPlayControllerV1::open(source_paths)?
    };
    let target_archive_node_id = controller
        .exact
        .current_archive_node_id()
        .ok_or_else(|| "run-play fork requires a retained public decision".to_string())?
        .to_string();
    if controller.head.current_decision.is_none() {
        return Err("run-play fork requires a retained public decision".to_string());
    }
    let archive = controller
        .exact
        .archive()
        .retained_fork_archive_to_node_v1(&target_archive_node_id)?;
    let mut checkpoints = controller
        .exact
        .checkpoints()
        .retained_for_archive_v1(&archive)?;
    if !checkpoints
        .identities()
        .any(|identity| identity.archive_node_id == target_archive_node_id)
    {
        // A public archive node can be replayable without already owning a
        // durable checkpoint. A fork promises an exact independent root, so
        // materialize that root on the isolated source controller before
        // publishing the target. This also prevents every command in the new
        // fork from replaying the entire retained prefix again.
        let prepared = controller.exact.prepare_current_checkpoint_v1()?;
        checkpoints = prepared.checkpoints().retained_for_archive_v1(&archive)?;
    }
    let checkpoint_payload = checkpoints.encode(DEFAULT_ARTIFACT_MAX_BYTES)?;
    create_session_from_validated_parts_v1(
        target_paths,
        archive,
        checkpoints,
        checkpoint_payload,
        Some(&target_archive_node_id),
        controller.head.default_budget,
        controller.head.combat_plan_options.clone(),
        None,
        controller.head.evidence_identity.clone(),
        controller.head.scratch,
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunPlayBranchPromotionV1 {
    pub source_session_id: String,
    pub source_head_revision: u64,
    pub target_session_id: String,
    pub previous_target_head_revision: u64,
    pub common_archive_node_id: String,
    pub skipped_public_only_ancestors: usize,
    pub promoted_step_count: usize,
    pub source_endpoint_archive_node_id: String,
    pub head: RunPlayHeadV1,
}

/// Replays the active suffix of one retained session onto another session at
/// their nearest shared exact archive ancestor, then publishes the whole
/// suffix as one target-head revision. Source snapshots and typed choices are
/// checked at every boundary; no natural-language selector is involved.
pub fn promote_branch_v1(
    source_paths: RunPlayPathsV1,
    target_paths: RunPlayPathsV1,
) -> Result<RunPlayBranchPromotionV1, String> {
    let canonical_source_root = fs::canonicalize(source_paths.root()).map_err(|error| {
        format!(
            "failed to resolve run-play branch promotion source {}: {error}",
            source_paths.root().display()
        )
    })?;
    let canonical_target_root = fs::canonicalize(target_paths.root()).map_err(|error| {
        format!(
            "failed to resolve run-play branch promotion target {}: {error}",
            target_paths.root().display()
        )
    })?;
    if canonical_source_root == canonical_target_root {
        return Err(
            "run-play branch promotion requires distinct source and target sessions".to_string(),
        );
    }

    let source = source_paths.load()?;
    let target = target_paths.load()?;
    let source_endpoint = source.head.position.archive_node_id().to_string();
    let target_endpoint = target.head.position.archive_node_id().to_string();
    let source_path = archive_path_node_ids_v1(&source.state.archive, &source_endpoint)?;
    let target_path = archive_path_node_ids_v1(&target.state.archive, &target_endpoint)?;
    let public_common_len = shared_archive_prefix_len_v1(&source_path, &target_path);
    if public_common_len == 0 {
        return Err(
            "run-play branch promotion sessions have no shared exact archive ancestor".to_string(),
        );
    }
    let source_checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
        &source.state.checkpoint_payload,
        &source.state.archive,
        DEFAULT_ARTIFACT_MAX_BYTES,
    )?;
    let target_checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
        &target.state.checkpoint_payload,
        &target.state.archive,
        DEFAULT_ARTIFACT_MAX_BYTES,
    )?;
    let source_identities = source_checkpoints
        .identities()
        .map(|identity| (identity.archive_node_id.as_str(), identity))
        .collect::<BTreeMap<_, _>>();
    let target_identities = target_checkpoints
        .identities()
        .map(|identity| (identity.archive_node_id.as_str(), identity))
        .collect::<BTreeMap<_, _>>();
    let exact_common_index = (0..public_common_len)
        .rev()
        .find(|index| {
            let node_id = source_path[*index].as_str();
            matches!(
                (source_identities.get(node_id), target_identities.get(node_id)),
                (Some(source_identity), Some(target_identity))
                    if source_identity == target_identity
            )
        })
        .ok_or_else(|| {
            "run-play branch promotion sessions share public history but no identical exact checkpoint ancestor"
                .to_string()
        })?;
    let common_archive_node_id = source_path[exact_common_index].clone();
    let skipped_public_only_ancestors = public_common_len - exact_common_index - 1;
    let suffix = &source_path[exact_common_index + 1..];
    if suffix.is_empty() {
        return Err(
            "run-play branch promotion source has no active suffix beyond the shared ancestor"
                .to_string(),
        );
    }
    if suffix.len() > MAX_BRANCH_PROMOTION_STEPS {
        return Err(format!(
            "run-play branch promotion requires {} steps, exceeding the bounded maximum of {MAX_BRANCH_PROMOTION_STEPS}",
            suffix.len()
        ));
    }

    let steps = suffix
        .iter()
        .map(|node_id| {
            let node =
                source.state.archive.node(node_id).ok_or_else(|| {
                    format!("run-play branch promotion misses source node {node_id}")
                })?;
            let incoming = node.incoming_step.as_ref().ok_or_else(|| {
                format!("run-play branch promotion source node {node_id} has no incoming step")
            })?;
            Ok((
                incoming.step.source.information.snapshot_id.clone(),
                incoming.step.choice.clone(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let source_session_id = source.head.session_id.clone();
    let source_head_revision = source.head.head_revision;
    let target_session_id = target.head.session_id.clone();
    let previous_target_head_revision = target.head.head_revision;
    let source_has_public_endpoint = source.head.current_decision.as_ref().is_some();
    let expected_public_endpoint_decision = source
        .head
        .current_decision
        .as_ref()
        .map(|decision| decision.public_decision_v1())
        .transpose()?;
    let expected_terminal_outcome = match &source.head.position {
        RunPlayPositionV1::Decision { .. } => None,
        RunPlayPositionV1::Terminal { outcome, .. } => Some(outcome.clone()),
    };
    if source_has_public_endpoint == expected_terminal_outcome.is_some() {
        return Err(
            "run-play branch promotion source has an inconsistent decision/terminal endpoint"
                .to_string(),
        );
    }
    drop(source);
    drop(target);

    let expected_endpoint_continuation_fingerprint = source_has_public_endpoint
        .then(|| {
            let source_controller = RunPlayControllerV1::open(source_paths)?;
            let checkpoint = source_controller
                .exact
                .current_run_decision_checkpoint_v1()?;
            let session = checkpoint.into_session()?;
            Ok::<_, String>(run_control_continuation_fingerprint_v1(&session))
        })
        .transpose()?;

    let mut controller =
        RunPlayControllerV1::open_at_archive_node_id_v1(target_paths, &common_archive_node_id)?;
    let prepared = controller.exact.prepare_line_resolving_v1(
        steps.len(),
        &mut |index, decision| {
            let (expected_snapshot_id, choice) = &steps[index];
            if decision.information.snapshot_id != *expected_snapshot_id {
                return Err(format!(
                    "run-play branch promotion step {} expected source snapshot {}, observed target snapshot {}",
                    index + 1,
                    expected_snapshot_id,
                    decision.information.snapshot_id
                ));
            }
            Ok(choice.clone())
        },
    )?;
    match (
        prepared.current_decision(),
        prepared.terminal_outcome(),
        expected_endpoint_continuation_fingerprint.as_deref(),
        expected_terminal_outcome.as_ref(),
    ) {
        (Some(decision), None, Some(expected), None) => {
            let observed_checkpoint = prepared.current_run_decision_checkpoint_for_run_play_v1()?;
            let observed_session = observed_checkpoint.into_session()?;
            let observed = run_control_continuation_fingerprint_v1(&observed_session);
            if observed != expected {
                let observed_public = decision.public_decision_v1()?;
                let public_difference = expected_public_endpoint_decision
                    .as_ref()
                    .and_then(|source| source.replay_incompatibility_v2(&observed_public))
                    .unwrap_or_else(|| "none".to_string());
                return Err(format!(
                    "run-play branch promotion continuation endpoint mismatch: common={} skipped_public_only_ancestors={} expected={} observed={} public_difference={}",
                    common_archive_node_id,
                    skipped_public_only_ancestors,
                    expected,
                    observed,
                    public_difference,
                ));
            }
        }
        (None, Some(observed), None, Some(expected)) if observed == expected => {}
        (None, Some(observed), None, Some(expected)) => {
            return Err(format!(
                "run-play branch promotion terminal endpoint mismatch: expected {expected:?}, observed {observed:?}"
            ));
        }
        _ => {
            return Err(
                "run-play branch promotion endpoint kind disagrees with its source".to_string(),
            );
        }
    }
    controller.publish_prepared_line_v1(prepared, None, None)?;

    Ok(RunPlayBranchPromotionV1 {
        source_session_id,
        source_head_revision,
        target_session_id,
        previous_target_head_revision,
        common_archive_node_id,
        skipped_public_only_ancestors,
        promoted_step_count: steps.len(),
        source_endpoint_archive_node_id: source_endpoint,
        head: controller.head().clone(),
    })
}

fn archive_path_node_ids_v1(
    archive: &TaskRunSolutionArchiveV1,
    endpoint: &str,
) -> Result<Vec<String>, String> {
    let mut path = Vec::new();
    let mut cursor = Some(endpoint);
    while let Some(node_id) = cursor {
        let node = archive
            .node(node_id)
            .ok_or_else(|| format!("run-play archive path misses node {node_id}"))?;
        path.push(node.node_id.clone());
        cursor = node.parent_node_id.as_deref();
    }
    path.reverse();
    Ok(path)
}

fn shared_archive_prefix_len_v1(source: &[String], target: &[String]) -> usize {
    source
        .iter()
        .zip(target)
        .take_while(|(source_node, target_node)| source_node == target_node)
        .count()
}

fn create_session_from_validated_parts_v1(
    paths: &RunPlayPathsV1,
    archive: TaskRunSolutionArchiveV1,
    checkpoints: TaskRunBranchCheckpointArtifactV1,
    checkpoint_payload: Vec<u8>,
    requested_node_id: Option<&str>,
    default_budget: TaskBudgetV1,
    combat_plan_options: CombatResearchPlanOptionsV1,
    expected_exact_state_fingerprint: Option<&str>,
    evidence_identity: Option<RunPlayEvidenceIdentityV1>,
    scratch: bool,
) -> Result<RunPlayWorkspaceV1, String> {
    let current_node_id = select_initial_decision_node_v1(&archive, requested_node_id)?;
    // Publication is gated by the same exact continuation owner used by
    // `show` and `act`. A mismatched task must not leave behind a workspace
    // that can be created successfully but never reopened.
    let exact = ExactRunPlaySessionV1::open(
        archive,
        checkpoints,
        &current_node_id,
        default_budget,
        combat_plan_options.clone(),
        ExactBranchingTaskEvidenceRequestV1::default(),
    )?;
    if let Some(expected) = expected_exact_state_fingerprint {
        let checkpoint = exact.current_run_decision_checkpoint_v1()?;
        let session = checkpoint.into_session()?;
        if run_control_session_fingerprint_v3(&session) != expected {
            return Err("rerooted run-play session changed its exact state or RNG".to_string());
        }
    }
    let (archive, checkpoints) = exact.into_archive_and_checkpoints_v1();
    let current = task_run_branch_decision_v1(&archive, &checkpoints, &current_node_id)?;
    let state = RunPlayStateV1 {
        schema: RUN_PLAY_STATE_SCHEMA_V1.to_string(),
        state_revision: 0,
        archive,
        checkpoint_payload,
    };
    let identity = current.target_archive_node_id.as_str();
    let session_id = format!(
        "seed-{}-{}",
        checkpoints.seed(),
        &identity[identity.len().saturating_sub(12)..]
    );
    let mut head = RunPlayHeadV1 {
        schema: RUN_PLAY_HEAD_SCHEMA_V1.to_string(),
        scratch,
        scratch_parent_revision: None,
        scratch_floor_ends: Vec::new(),
        scratch_marks: Vec::new(),
        scratch_return_revision: None,
        evidence_identity: if scratch { None } else { evidence_identity },
        session_id,
        head_revision: 0,
        state_revision: 0,
        delta_revisions: Vec::new(),
        position: RunPlayPositionV1::Decision {
            archive_node_id: current.target_archive_node_id.clone(),
        },
        default_budget,
        combat_plan_options,
        current_decision: Some(current.clone()),
        current_path: run_play_path_v1(&state.archive, &current_node_id)?,
        events: Vec::new(),
    };
    if scratch {
        record_scratch_landmarks_v1(&mut head, None);
    }
    paths.create(&head, &state)?;
    paths.load()
}

/// Replace the disposable play archive with one exact decision. Nothing from
/// the previous trajectory is copied into the new root.
fn scratch_root_from_checkpoint_v1(
    previous: &RunPlayHeadV1,
    checkpoint: RunControlSessionCheckpointV1,
    objective: TaskObjectiveV1,
    state_revision: u64,
) -> Result<(RunPlayHeadV1, RunPlayStateV1, ExactRunPlaySessionV1), String> {
    let expected = run_control_continuation_fingerprint_v1(&checkpoint.clone().into_session()?);
    let source = ExactRunDecisionSourceV1 {
        session_id: "scratch".to_string(),
        head_revision: 0,
        archive_node_id: "current".to_string(),
        public_snapshot_id: "current".to_string(),
    };
    let artifacts = exact_run_decision_root_artifacts_v1(
        checkpoint,
        source,
        objective,
        previous.default_budget,
        previous.combat_plan_options.clone(),
        ExactBranchingTaskEvidenceRequestV1::default(),
    )?;
    let checkpoint_payload = artifacts.checkpoints.encode(DEFAULT_ARTIFACT_MAX_BYTES)?;
    let node_id = artifacts
        .checkpoints
        .identities()
        .next()
        .ok_or("scratch root has no checkpoint")?
        .archive_node_id
        .clone();
    let exact = ExactRunPlaySessionV1::open(
        artifacts.archive,
        artifacts.checkpoints,
        &node_id,
        previous.default_budget,
        previous.combat_plan_options.clone(),
        ExactBranchingTaskEvidenceRequestV1::default(),
    )?;
    let observed = run_control_continuation_fingerprint_v1(
        &exact.current_run_decision_checkpoint_v1()?.into_session()?,
    );
    if observed != expected {
        return Err("scratch root changed the exact game continuation".into());
    }
    let current = task_run_branch_decision_v1(exact.archive(), exact.checkpoints(), &node_id)?;
    let mut head = previous.clone();
    head.scratch = true;
    head.evidence_identity = None;
    head.scratch_parent_revision = Some(previous.head_revision);
    head.head_revision += 1;
    head.state_revision = state_revision;
    head.delta_revisions.clear();
    head.position = RunPlayPositionV1::Decision {
        archive_node_id: node_id.clone(),
    };
    head.current_decision = Some(current);
    record_scratch_landmarks_v1(&mut head, Some(previous));
    head.current_path = run_play_path_v1(exact.archive(), &node_id)?;
    head.events.clear();
    let state = RunPlayStateV1 {
        schema: RUN_PLAY_STATE_SCHEMA_V1.to_string(),
        state_revision,
        archive: exact.archive().clone(),
        checkpoint_payload,
    };
    Ok((head, state, exact))
}

fn scratch_public_floor_v1(head: &RunPlayHeadV1) -> Option<i32> {
    let decision = head.current_decision.as_ref()?;
    match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => Some(observation.run.floor),
        TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Available { floor, .. },
            ..
        } => Some(*floor),
        TaskPublicObservationV1::Combat { .. } => None,
    }
}

fn record_scratch_landmarks_v1(head: &mut RunPlayHeadV1, previous: Option<&RunPlayHeadV1>) {
    if let (Some(previous_floor), Some(current_floor)) = (
        previous.and_then(scratch_public_floor_v1),
        scratch_public_floor_v1(head),
    ) {
        if current_floor > previous_floor
            && !head
                .scratch_floor_ends
                .iter()
                .any(|end| end.floor == previous_floor)
        {
            // Boss/act transitions may have no map decision between floors.
            head.scratch_floor_ends.push(RunPlayScratchFloorEndV1 {
                floor: previous_floor,
                head_revision: head.head_revision,
            });
        }
    }
    let Some(decision) = &head.current_decision else {
        return;
    };
    if decision.site != TaskDecisionSiteV1::Map {
        return;
    }
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return;
    };
    let floor = observation.run.floor;
    if head
        .scratch_floor_ends
        .last()
        .is_some_and(|end| end.floor == floor)
    {
        head.scratch_floor_ends.pop();
    }
    head.scratch_floor_ends.push(RunPlayScratchFloorEndV1 {
        floor,
        head_revision: head.head_revision,
    });
}

fn select_initial_decision_node_v1(
    archive: &TaskRunSolutionArchiveV1,
    requested_node_id: Option<&str>,
) -> Result<String, String> {
    if let Some(node_id) = requested_node_id {
        return archive
            .node(node_id)
            .map(|_| node_id.to_string())
            .ok_or_else(|| format!("archive node {node_id} does not exist"));
    }
    let last_attempt = archive
        .attempts
        .last()
        .ok_or_else(|| "run archive has no attempts".to_string())?;
    let candidate_ids = last_attempt
        .node_path
        .last()
        .into_iter()
        .chain(&last_attempt.observed_frontier_node_ids)
        .collect::<Vec<_>>();
    let Some(max_depth) = candidate_ids
        .iter()
        .filter_map(|node_id| archive.node(node_id))
        .filter(|node| archive_node_has_public_decision_v1(archive, node))
        .map(|node| node.depth)
        .max()
    else {
        return Err("run artifacts contain no recoverable public decisions".to_string());
    };
    let deepest = candidate_ids
        .iter()
        .filter_map(|node_id| archive.node(node_id))
        .filter(|node| {
            node.depth == max_depth && archive_node_has_public_decision_v1(archive, node)
        })
        .collect::<Vec<_>>();
    match deepest.as_slice() {
        [node] => Ok(node.node_id.clone()),
        _ => {
            let choices = deepest
                .iter()
                .map(|node| short_id(&node.node_id).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "deepest recoverable frontier is ambiguous ({choices}); pass --node with the intended archive id"
            ))
        }
    }
}

const MAX_HOT_RECOVERY_SNAPSHOTS: usize = 24;

#[derive(Default)]
struct RunPlayHotRecoveryCacheV1 {
    insertion_order: VecDeque<String>,
    snapshots: BTreeMap<String, ExactRunPlayHotSnapshotV1>,
}

impl RunPlayHotRecoveryCacheV1 {
    fn contains(&self, archive_node_id: &str) -> bool {
        self.snapshots.contains_key(archive_node_id)
    }

    fn get(&self, archive_node_id: &str) -> Option<&ExactRunPlayHotSnapshotV1> {
        self.snapshots.get(archive_node_id)
    }

    fn insert(&mut self, archive_node_id: String, snapshot: ExactRunPlayHotSnapshotV1) {
        if self.contains(&archive_node_id) {
            return;
        }
        while self.snapshots.len() >= MAX_HOT_RECOVERY_SNAPSHOTS {
            let Some(evicted) = self.insertion_order.pop_front() else {
                break;
            };
            self.snapshots.remove(&evicted);
        }
        self.insertion_order.push_back(archive_node_id.clone());
        self.snapshots.insert(archive_node_id, snapshot);
    }

    fn absorb(&mut self, mut other: Self) {
        while let Some(archive_node_id) = other.insertion_order.pop_front() {
            let Some(snapshot) = other.snapshots.remove(&archive_node_id) else {
                continue;
            };
            self.insert(archive_node_id, snapshot);
        }
    }
}

pub struct RunPlayControllerV1 {
    paths: RunPlayPathsV1,
    head: RunPlayHeadV1,
    base_state_revision: u64,
    exact: ExactRunPlaySessionV1,
    hot_recovery: RunPlayHotRecoveryCacheV1,
}

pub struct RunPlayComparisonV1 {
    pub root_archive_node_id: String,
    pub lines: Vec<ExactRunPlayProbeLineV1>,
}

pub struct RunPlayCombatResearchAttemptV1 {
    pub attempt: TaskResearchPlanAttemptV1,
    pub witness_lines: Vec<ExactRunPlayProbeLineV1>,
    pub milestone_candidates: Vec<RunPlayCombatMilestoneResearchCandidateV1>,
    pub bounded_progress: Vec<RunPlayCombatResearchBoundedProgressV1>,
}

pub struct RunPlayCombatResearchBoundedProgressV1 {
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub max_player_turn: u32,
    pub recent_turn_survival_envelope: Vec<CombatResearchBoundedProgressStateV1>,
    pub paths: Vec<RunPlayCombatResearchBoundedProgressPathV1>,
}

pub struct RunPlayCombatResearchBoundedProgressPathV1 {
    pub candidate: CombatResearchBoundedProgressPathV1,
    pub action_line: Vec<String>,
    pub successor_public_snapshot_id: String,
}

fn combat_research_progress_state_from_public_v1(
    state: &PublicCombatStateV1,
    path_atomic_depth: usize,
) -> CombatResearchBoundedProgressStateV1 {
    let alive = state
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.escaped && !monster.dying && !monster.half_dead)
        .collect::<Vec<_>>();
    CombatResearchBoundedProgressStateV1 {
        player_turn: state.turn.turn_count,
        player_hp: state.player.hp,
        player_block: state.player.block,
        alive_enemy_count: alive.len(),
        enemy_total_hp: alive.into_iter().map(|monster| monster.hp.max(0)).sum(),
        hand_size: state.cards.hand.cards.len(),
        draw_pile_size: state.cards.draw.cards.len(),
        discard_pile_size: state.cards.discard.cards.len(),
        exhaust_pile_size: state.cards.exhaust.cards.len(),
        path_atomic_depth,
    }
}

pub struct RunPlayCombatMilestoneResearchCandidateV1 {
    pub candidate_id: String,
    pub candidate: CombatMilestoneResearchCandidateV1,
    pub action_line: Vec<String>,
    pub successor_public_snapshot_id: String,
    pub successor: PublicCombatStateV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatMilestoneContinuationAuditStatusV1 {
    VerifiedContinuation,
    UnknownInBoundedSurface,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatMilestoneContinuationCandidateV1 {
    pub continuation_id: String,
    pub new_completed_milestones: Vec<CombatPlanMilestoneV1>,
    pub view: CompleteTurnCandidateViewV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatMilestoneContinuationAuditV1 {
    pub candidate_id: String,
    pub successor_public_snapshot_id: String,
    pub status: RunPlayCombatMilestoneContinuationAuditStatusV1,
    pub bounded_surface_only: bool,
    pub absence_means_unknown: bool,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub engine_steps: usize,
    pub exact_state_skips: usize,
    pub truncated_children: usize,
    pub preselection_candidate_count: usize,
    pub continuations: Vec<RunPlayCombatMilestoneContinuationCandidateV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatPrefixHorizonStatusV1 {
    TerminalWinFound,
    NonLossFoundInBoundedSurface,
    NoNonLossInBoundedSurface,
    UnknownInBoundedSurface,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatPrefixHorizonSuccessorV1 {
    pub successor_id: String,
    pub view: CompleteTurnCandidateViewV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatPrefixHorizonLevelV1 {
    pub additional_turn: usize,
    pub parent_count: usize,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub engine_steps: usize,
    pub exact_state_skips: usize,
    pub truncated_children: usize,
    pub preselection_candidate_count: usize,
    pub retained_candidate_count: usize,
    pub terminal_win_count: usize,
    pub terminal_loss_count: usize,
    pub unresolved_count: usize,
    pub other_count: usize,
    pub public_unresolved_successor_count: usize,
    pub public_successor_projection_truncated: bool,
    pub public_unresolved_successors: Vec<RunPlayCombatPrefixHorizonSuccessorV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatPrefixHorizonAuditV1 {
    pub lineage_id: String,
    pub prefix_action_count: usize,
    pub successor_public_snapshot_id: String,
    pub requested_additional_turns: usize,
    pub completed_additional_turns: usize,
    pub status: RunPlayCombatPrefixHorizonStatusV1,
    pub bounded_surface_only: bool,
    pub absence_means_unknown: bool,
    pub levels: Vec<RunPlayCombatPrefixHorizonLevelV1>,
}

fn combat_prefix_horizon_status_v1(
    levels: &[RunPlayCombatPrefixHorizonLevelV1],
) -> RunPlayCombatPrefixHorizonStatusV1 {
    if levels.iter().any(|level| level.terminal_win_count > 0) {
        return RunPlayCombatPrefixHorizonStatusV1::TerminalWinFound;
    }
    if levels.iter().any(|level| level.other_count > 0) {
        return RunPlayCombatPrefixHorizonStatusV1::UnknownInBoundedSurface;
    }
    match levels.last() {
        Some(level) if level.unresolved_count > 0 => {
            RunPlayCombatPrefixHorizonStatusV1::NonLossFoundInBoundedSurface
        }
        Some(level)
            if level.retained_candidate_count > 0
                && level.terminal_loss_count == level.retained_candidate_count =>
        {
            RunPlayCombatPrefixHorizonStatusV1::NoNonLossInBoundedSurface
        }
        _ => RunPlayCombatPrefixHorizonStatusV1::UnknownInBoundedSurface,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnCandidateV1 {
    pub candidate_id: String,
    pub view: CompleteTurnCandidateViewV1,
}

pub struct RunPlayCombatTurnAttemptV1 {
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub engine_steps: usize,
    pub exact_state_skips: usize,
    pub truncated_children: usize,
    pub preselection_candidate_count: usize,
    pub candidates: Vec<RunPlayCombatTurnCandidateV1>,
    exact_candidates: Vec<ExactCompleteTurnProposalV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunPlayCombatTurnReservationV1 {
    pub candidate: RunPlayCombatTurnCandidateV1,
    pub already_present: bool,
    pub candidate_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationAuditOptionsV1 {
    pub max_generation_work: usize,
    pub max_selections: usize,
    pub wall_ms: u64,
    pub max_engine_steps_per_transition: usize,
    pub uniform_exploration_ppm: u32,
    pub generation_quantum_work: usize,
    pub max_turn_depth: usize,
    pub total_max_potions_used: Option<u32>,
}

impl RunPlayCombatTurnContinuationAuditOptionsV1 {
    pub fn validate(self) -> Result<(), String> {
        if self.max_generation_work == 0
            || self.max_selections == 0
            || self.wall_ms == 0
            || self.max_engine_steps_per_transition == 0
            || self.generation_quantum_work == 0
            || self.max_turn_depth == 0
        {
            return Err(
                "combat-turn continuation audit requires positive finite limits".to_string(),
            );
        }
        if self.uniform_exploration_ppm > 1_000_000 {
            return Err(
                "combat-turn continuation audit uniform exploration exceeds one million ppm"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn search_spec_v1(self, remaining_max_potions_used: Option<u32>) -> LocalTurnGraphSearchSpecV1 {
        LocalTurnGraphSearchSpecV1::from_controls(
            self.max_generation_work,
            self.max_selections,
            self.wall_ms,
            self.max_engine_steps_per_transition,
            self.uniform_exploration_ppm,
            self.generation_quantum_work,
            self.max_turn_depth,
            remaining_max_potions_used,
            false,
            None,
            None,
            None,
        )
    }
}

fn fixed_turn_exceeds_total_potion_contract_v1(
    fixed_turn_potion_expenditures: u32,
    total_max_potions_used: Option<u32>,
) -> bool {
    total_max_potions_used.is_some_and(|limit| fixed_turn_potion_expenditures > limit)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatTurnContinuationAuditStatusV1 {
    VerifiedWin,
    UnknownInBoundedSearch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatTurnContinuationAuditScopeV1 {
    FixedTurnTerminal,
    FixedTurnPlusSuffix,
    FixedTurnOutsidePotionContract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatTurnContinuationAuditExitV1 {
    WitnessFound,
    FixedTurnPotionContractExceeded,
    SelectionBudget,
    GenerationWorkBudget,
    EngineStepBudget,
    Deadline,
    FrontierExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationAuditWorkV1 {
    pub selections: usize,
    pub generation_work: usize,
    pub engine_steps: usize,
    pub exact_nodes: usize,
    pub completed_turn_options: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationPotionContractV1 {
    pub fixed_turn_potion_slot_mask: u64,
    pub fixed_turn_potion_expenditures: u32,
    /// `None` means the caller supplied no total expenditure ceiling.
    pub total_max_potion_expenditures: Option<u32>,
    /// Remaining ceiling for the searched suffix after the fixed turn.
    /// `None` means unbounded; a terminal fixed turn has no suffix search.
    pub suffix_max_potion_expenditures: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationWitnessV1 {
    pub final_hp: i32,
    pub forced_action_count: usize,
    pub continuation_action_count: usize,
    pub total_action_count: usize,
}

/// Stable public state summary from a bounded suffix search.
///
/// This deliberately omits planner queues, exact hashes, action prefixes, and
/// policy ranks. It is diagnostic evidence only and has no search authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationStateV1 {
    pub player_turn: u32,
    pub player_hp: i32,
    pub player_block: i32,
    pub alive_enemy_count: usize,
    pub enemy_total_hp: i32,
    pub hand_size: usize,
    pub draw_pile_size: usize,
    pub discard_pile_size: usize,
    pub exhaust_pile_size: usize,
    pub path_atomic_depth: usize,
}

impl From<&OracleCombatDeepStateSnapshot> for RunPlayCombatTurnContinuationStateV1 {
    fn from(value: &OracleCombatDeepStateSnapshot) -> Self {
        Self {
            player_turn: value.player_turn,
            player_hp: value.player_hp,
            player_block: value.player_block,
            alive_enemy_count: value.alive_enemy_count,
            enemy_total_hp: value.enemy_total_hp,
            hand_size: value.hand_size,
            draw_pile_size: value.draw_pile_size,
            discard_pile_size: value.discard_pile_size,
            exhaust_pile_size: value.exhaust_pile_size,
            path_atomic_depth: value.path_atomic_depth,
        }
    }
}

/// How far one finite continuation search actually progressed.
///
/// These observations explain a bounded miss without turning it into a loss
/// proof. They never participate in candidate generation, ranking, or search.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationProgressV1 {
    pub max_player_turn: u32,
    pub deepest_survival_state: Option<RunPlayCombatTurnContinuationStateV1>,
    pub deepest_progress_state: Option<RunPlayCombatTurnContinuationStateV1>,
    pub recent_turn_survival_envelope: Vec<RunPlayCombatTurnContinuationStateV1>,
    pub deepest_survival_path: Option<RunPlayCombatTurnContinuationPathV1>,
    pub deepest_progress_path: Option<RunPlayCombatTurnContinuationPathV1>,
}

impl From<&OracleCombatWitnessProgressSnapshot> for RunPlayCombatTurnContinuationProgressV1 {
    fn from(value: &OracleCombatWitnessProgressSnapshot) -> Self {
        Self {
            max_player_turn: value.max_player_turn,
            deepest_survival_state: value.deepest_survival_state.as_ref().map(Into::into),
            deepest_progress_state: value.deepest_progress_state.as_ref().map(Into::into),
            recent_turn_survival_envelope: value
                .recent_turn_survival_envelope
                .iter()
                .map(Into::into)
                .collect(),
            deepest_survival_path: None,
            deepest_progress_path: None,
        }
    }
}

/// Compact semantic replay of one diagnostic path selected by a bounded
/// continuation search. Exact hashes, planner ranks, and queue identities are
/// deliberately omitted; this only explains how the reported public state
/// was reached.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationPathV1 {
    pub action_count: usize,
    pub turns: Vec<RunPlayCombatTurnContinuationPathTurnV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationPathTurnV1 {
    pub player_turn: u32,
    pub actions: Vec<RunPlayCombatTurnContinuationSemanticActionV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationSelectedCardV1 {
    pub card_id: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RunPlayCombatTurnContinuationSemanticActionV1 {
    PlayCard {
        card_id: CardId,
        upgrades: u8,
        target_monster_index: Option<usize>,
    },
    UsePotion {
        potion_id: PotionId,
        potion_index: usize,
        target_monster_index: Option<usize>,
    },
    DiscardPotion {
        potion_id: PotionId,
        potion_index: usize,
    },
    EndTurn,
    Selection {
        input_kind: &'static str,
        selected_cards: Vec<RunPlayCombatTurnContinuationSelectedCardV1>,
    },
}

pub(crate) fn compact_continuation_input_v1(
    position: &CombatPosition,
    input: &ClientInput,
    action_index: usize,
) -> Result<RunPlayCombatTurnContinuationSemanticActionV1, String> {
    let target_monster_index = |target: Option<usize>| {
        target.and_then(|target| {
            position
                .combat
                .entities
                .monsters
                .iter()
                .position(|monster| monster.id == target)
        })
    };
    let semantic = match input {
        ClientInput::PlayCard { card_index, target } => {
            let card = position.combat.zones.hand.get(*card_index).ok_or_else(|| {
                format!(
                    "continuation diagnostic action {action_index} has no hand card {card_index}"
                )
            })?;
            RunPlayCombatTurnContinuationSemanticActionV1::PlayCard {
                card_id: card.id,
                upgrades: card.upgrades,
                target_monster_index: target_monster_index(*target),
            }
        }
        ClientInput::UsePotion {
            potion_index,
            target,
        } => {
            let potion = position
                .combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    format!(
                        "continuation diagnostic action {action_index} has no potion in slot {potion_index}"
                    )
                })?;
            RunPlayCombatTurnContinuationSemanticActionV1::UsePotion {
                potion_id: potion.id,
                potion_index: *potion_index,
                target_monster_index: target_monster_index(*target),
            }
        }
        ClientInput::DiscardPotion(potion_index) => {
            let potion = position
                .combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    format!(
                        "continuation diagnostic action {action_index} has no potion in slot {potion_index}"
                    )
                })?;
            RunPlayCombatTurnContinuationSemanticActionV1::DiscardPotion {
                potion_id: potion.id,
                potion_index: *potion_index,
            }
        }
        ClientInput::EndTurn => RunPlayCombatTurnContinuationSemanticActionV1::EndTurn,
        other => {
            let selected_cards = match other {
                ClientInput::SubmitSelection(resolution) => resolution
                    .selected_card_lineages()
                    .into_iter()
                    .map(|lineage| {
                        position
                            .combat
                            .zones
                            .hand
                            .iter()
                            .chain(position.combat.zones.draw_pile.iter())
                            .chain(position.combat.zones.discard_pile.iter())
                            .chain(position.combat.zones.exhaust_pile.iter())
                            .chain(position.combat.zones.limbo.iter())
                            .find(|card| card.lineage == lineage)
                            .map(|card| RunPlayCombatTurnContinuationSelectedCardV1 {
                                card_id: card.id,
                                upgrades: card.upgrades,
                            })
                            .ok_or_else(|| {
                                format!(
                                    "continuation diagnostic action {action_index} cannot resolve selected card {lineage}"
                                )
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => Vec::new(),
            };
            RunPlayCombatTurnContinuationSemanticActionV1::Selection {
                input_kind: other.kind_v1(),
                selected_cards,
            }
        }
    };
    Ok(semantic)
}

fn compact_continuation_path_v1(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    max_engine_steps_per_transition: usize,
) -> Result<RunPlayCombatTurnContinuationPathV1, String> {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut turns = Vec::<RunPlayCombatTurnContinuationPathTurnV1>::new();
    for (action_index, action) in actions.iter().enumerate() {
        let player_turn = position.combat.turn.turn_count;
        if turns
            .last()
            .is_none_or(|turn| turn.player_turn != player_turn)
        {
            turns.push(RunPlayCombatTurnContinuationPathTurnV1 {
                player_turn,
                actions: Vec::new(),
            });
        }
        let semantic = compact_continuation_input_v1(&position, &action.input, action_index)?;
        turns
            .last_mut()
            .expect("the current continuation turn was inserted")
            .actions
            .push(semantic);

        let result = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            sts_oracle_learning::sim::combat::CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return Err(format!(
                "continuation diagnostic action {action_index} exceeded its finite replay limit"
            ));
        }
        let actual_hash =
            combat_exact_state_hash_v2(&result.position.engine, &result.position.combat);
        if actual_hash != action.expected_successor_hash.as_str() {
            return Err(format!(
                "continuation diagnostic action {action_index} exact successor drifted"
            ));
        }
        position = result.position;
    }
    Ok(RunPlayCombatTurnContinuationPathV1 {
        action_count: actions.len(),
        turns,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationAuditV1 {
    pub candidate_id: String,
    pub scope: RunPlayCombatTurnContinuationAuditScopeV1,
    pub status: RunPlayCombatTurnContinuationAuditStatusV1,
    pub exit: RunPlayCombatTurnContinuationAuditExitV1,
    pub bounded_search_only: bool,
    pub absence_means_unknown: bool,
    pub potion_contract: RunPlayCombatTurnContinuationPotionContractV1,
    pub search_spec: Option<LocalTurnGraphSearchSpecV1>,
    pub work: RunPlayCombatTurnContinuationAuditWorkV1,
    /// Present only when a suffix search ran. `None` for an already-terminal
    /// fixed turn candidate does not mean that progress information was lost.
    pub bounded_progress: Option<RunPlayCombatTurnContinuationProgressV1>,
    pub witness: Option<RunPlayCombatTurnContinuationWitnessV1>,
    pub terminal_outcome_frontier: Vec<LocalTurnGraphTerminalOutcomeSnapshotV1>,
}

/// One terminal resource vector actually found by a bounded continuation
/// search, annotated with the fixed complete-turn candidate that reached it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnObservedTerminalOutcomeV1 {
    pub candidate_id: String,
    pub outcome: LocalTurnGraphTerminalOutcomeSnapshotV1,
    /// The same exact witness replayed through the full run boundary. This is
    /// the comparable owner projection for cached complete combat plans.
    pub public_outcome: TaskResearchPlanOutcomeV1,
}

/// Batch continuation evidence over a caller-selected candidate set.
///
/// The observed frontier compares only replay-verified witnesses found inside
/// the declared finite searches. Missing outcomes and candidates remain
/// Unknown; the frontier is not a proof that an omitted candidate can never
/// produce a better witness.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnContinuationAuditBatchV1 {
    pub audits: Vec<RunPlayCombatTurnContinuationAuditV1>,
    pub observed_terminal_frontier: Vec<RunPlayCombatTurnObservedTerminalOutcomeV1>,
    pub observed_witnesses_only: bool,
    pub absence_means_unknown: bool,
}

fn retain_observed_combat_turn_terminal_outcome_v1(
    frontier: &mut Vec<RunPlayCombatTurnObservedTerminalOutcomeV1>,
    candidate: RunPlayCombatTurnObservedTerminalOutcomeV1,
) {
    if frontier.iter().any(|existing| {
        existing
            .public_outcome
            .strictly_dominates_with_bounded_relic_progress_equivalence_v1(
                &candidate.public_outcome,
            )
    }) {
        return;
    }
    frontier.retain(|existing| {
        !candidate
            .public_outcome
            .strictly_dominates_with_bounded_relic_progress_equivalence_v1(&existing.public_outcome)
    });
    frontier.push(candidate);
}

pub fn combat_research_candidate_id_v1(
    candidate: &sts_oracle_learning::agent::task::TaskResearchPlanCandidateV1,
) -> Result<String, String> {
    stable_planner_id(
        "run_play_combat_execution_candidate_v1",
        &(
            &candidate.plan,
            &candidate.outcome,
            candidate.satisfies_requested_quality,
        ),
    )
}

/// The same visible resource frontier used by the agent protocol. Apply it
/// after full run settlement, before constructing expensive detailed traces.
/// Different potion reserves, rewards and protected relic states remain.
pub(crate) fn combat_research_frontier_indices_v1(
    candidates: &[sts_oracle_learning::agent::task::TaskResearchPlanCandidateV1],
) -> Vec<usize> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let dominated = candidates.iter().enumerate().any(|(other_index, other)| {
                other_index != index && other.resource_dominates_v1(candidate)
            });
            (!dominated).then_some(index)
        })
        .collect()
}

fn event_action_selectors_v1(decision: &TaskPublicDecisionV1) -> Vec<RunPlayActionSelectorV1> {
    decision
        .candidates
        .iter()
        .filter_map(|candidate| match candidate {
            TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::ChooseEventOption {
                        event,
                        option_index,
                        ..
                    },
            } => Some(RunPlayActionSelectorV1::EventOption {
                event: event.stable_id().to_string(),
                option_index: *option_index,
            }),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct RunPlayOpenProfileV1 {
    pub total: Duration,
    pub workspace_load: Duration,
    pub workspace_load_detail: RunPlayWorkspaceLoadProfileV1,
    pub checkpoint_decode: Duration,
    pub checkpoint_decode_detail: TaskRunBranchCheckpointDecodeProfileV1,
    pub exact: ExactRunPlayOpenProfileV1,
    pub head_refresh: Duration,
    pub hot_snapshot: Duration,
    pub archive_nodes: usize,
    pub archive_attempts: usize,
    pub checkpoint_payload_bytes: usize,
    pub checkpoint_count: usize,
    pub active_path_nodes: usize,
    pub off_path_archive_nodes: usize,
    pub active_path_checkpoints: usize,
    pub cached_head_replayed_public_steps: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RunPlayActionLineProfileV1 {
    pub total: Duration,
    pub exact_restore: Duration,
    pub prepared_line: ExactRunPlayPreparedLineProfileV1,
    pub endpoint_validation: Duration,
    pub publication: Duration,
}

#[derive(Clone, Copy, Debug)]
pub struct RunPlayActiveCompactionProfileV1 {
    pub before_nodes: usize,
    pub after_nodes: usize,
    pub before_attempts: usize,
    pub after_attempts: usize,
    pub before_checkpoints: usize,
    pub after_checkpoints: usize,
    pub before_checkpoint_payload_bytes: usize,
    pub after_checkpoint_payload_bytes: usize,
    pub after_state_payload_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct RunPlayStorageCensusV1 {
    pub elapsed: Duration,
    pub workspace_load: RunPlayWorkspaceLoadProfileV1,
    pub archive_payload_bytes: usize,
    pub rewritten_state_payload_bytes: usize,
    pub archive_task_payload_bytes: usize,
    pub archive_nodes_payload_bytes: usize,
    pub archive_attempts_payload_bytes: usize,
    pub checkpoint_payload_bytes: usize,
    pub node_count: usize,
    pub attempt_count: usize,
    pub incoming_step_count: usize,
    pub persisted_decision_occurrences: usize,
    pub persisted_decision_payload_bytes: usize,
    pub initial_decision_payload_bytes: usize,
    pub source_decision_payload_bytes: usize,
    pub observed_decision_payload_bytes: usize,
    pub decision_observation_payload_bytes: usize,
    pub decision_candidates_payload_bytes: usize,
    pub derived_information_payload_bytes: usize,
    pub exact_parent_source_duplicates: usize,
    pub exact_parent_source_duplicate_payload_bytes: usize,
}

impl RunPlayControllerV1 {
    pub fn open(paths: RunPlayPathsV1) -> Result<Self, String> {
        Self::open_profiled_v1(paths).map(|(controller, _profile)| controller)
    }

    /// Reconstruct an existing decision for read-only inspection or rerooting.
    /// The exact restore still checks replay compatibility with the saved
    /// public decision; this does not authorize writing to the old session.
    pub(crate) fn open_for_read_only_snapshot_v1(paths: RunPlayPathsV1) -> Result<Self, String> {
        if paths.load_head()?.scratch {
            return Self::open(paths);
        }
        Self::open_profiled_with_source_gate_v1(paths, false)
            .map(|(controller, _profile)| controller)
    }

    /// Opens an already retained public decision for read-only inspection
    /// without publishing a controller-head move or appending a navigation
    /// event to the durable workspace.
    pub(crate) fn open_at(paths: RunPlayPathsV1, selector: &str) -> Result<Self, String> {
        Self::open_at_with_source_gate_v1(paths, selector, true)
    }

    fn open_at_for_read_only_snapshot_v1(
        paths: RunPlayPathsV1,
        selector: &str,
    ) -> Result<Self, String> {
        Self::open_at_with_source_gate_v1(paths, selector, false)
    }

    fn open_at_with_source_gate_v1(
        paths: RunPlayPathsV1,
        selector: &str,
        require_current_source: bool,
    ) -> Result<Self, String> {
        let mut workspace = paths.load()?;
        let target_archive_node_id = match resolve_jump_node_from_parts_v1(
            &workspace.state.archive,
            &workspace.head,
            selector,
        ) {
            Ok(node) => node,
            Err(error) => {
                let Some(exact_node_id) = selector.strip_prefix('n').filter(|id| !id.is_empty())
                else {
                    return Err(error);
                };
                let Some(retained) =
                    paths.load_retained_scratch_node_v1(&workspace.head, exact_node_id)?
                else {
                    return Err(error);
                };
                workspace = retained;
                exact_node_id.to_string()
            }
        };
        Self::open_at_loaded_archive_node_id_v1(
            paths,
            workspace,
            target_archive_node_id,
            require_current_source,
        )
    }

    fn open_at_archive_node_id_v1(
        paths: RunPlayPathsV1,
        target_archive_node_id: &str,
    ) -> Result<Self, String> {
        let workspace = paths.load()?;
        if workspace
            .state
            .archive
            .node(target_archive_node_id)
            .is_none()
        {
            return Err(format!(
                "run-play archive does not retain exact node {target_archive_node_id}"
            ));
        }
        Self::open_at_loaded_archive_node_id_v1(
            paths,
            workspace,
            target_archive_node_id.to_string(),
            true,
        )
    }

    fn open_at_loaded_archive_node_id_v1(
        paths: RunPlayPathsV1,
        workspace: RunPlayWorkspaceV1,
        target_archive_node_id: String,
        require_current_source: bool,
    ) -> Result<Self, String> {
        if require_current_source && !workspace.head.scratch {
            RunPlayEvidenceIdentityV1::require_current_v1(
                workspace.head.evidence_identity.as_ref(),
            )?;
        }
        let checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
            &workspace.state.checkpoint_payload,
            &workspace.state.archive,
            DEFAULT_ARTIFACT_MAX_BYTES,
        )?;
        let base_state_revision = workspace.state.state_revision;
        let default_budget = workspace.head.default_budget;
        let combat_plan_options = workspace.head.combat_plan_options.clone();
        let (exact, _profile) = ExactRunPlaySessionV1::open_in_validated_session_profiled_v1(
            workspace.state.archive,
            checkpoints,
            &target_archive_node_id,
            default_budget,
            combat_plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )?;
        let mut head = workspace.head;
        head.position = RunPlayPositionV1::Decision {
            archive_node_id: target_archive_node_id.clone(),
        };
        head.current_decision = Some(task_run_branch_decision_in_validated_session_v1(
            exact.archive(),
            exact.checkpoints(),
            &target_archive_node_id,
        )?);
        head.current_path = run_play_path_v1(exact.archive(), &target_archive_node_id)?;
        refresh_head_decision_from_exact_v1(&mut head, &exact)?;
        let mut controller = Self {
            paths,
            head,
            base_state_revision,
            exact,
            hot_recovery: RunPlayHotRecoveryCacheV1::default(),
        };
        controller.remember_current_hot_snapshot_v1()?;
        Ok(controller)
    }

    pub fn open_profiled_v1(paths: RunPlayPathsV1) -> Result<(Self, RunPlayOpenProfileV1), String> {
        match Self::open_profiled_with_source_gate_v1(paths.clone(), true) {
            Ok(opened) => Ok(opened),
            Err(original_error) => {
                let workspace = match paths.load() {
                    Ok(workspace) if workspace.head.scratch => workspace,
                    _ => return Err(original_error),
                };
                let RunPlayPositionV1::Decision { archive_node_id } = &workspace.head.position
                else {
                    return Err(original_error);
                };
                let (checkpoint, _) =
                    TaskRunBranchCheckpointArtifactV1::selected_exact_state_for_reroot_v1(
                        &workspace.state.checkpoint_payload,
                        &workspace.state.archive,
                        archive_node_id,
                        DEFAULT_ARTIFACT_MAX_BYTES,
                    )
                    .map_err(|recovery_error| {
                        format!(
                            "scratch decision cannot open: {original_error}; exact state cannot be recovered: {recovery_error}"
                        )
                    })?;
                let (mut head, state, _) = scratch_root_from_checkpoint_v1(
                    &workspace.head,
                    checkpoint,
                    workspace.state.archive.task.objective,
                    workspace.state.state_revision + 1,
                )?;
                // Earlier snapshots may use the same incompatible projection.
                head.scratch_parent_revision = None;
                head.scratch_floor_ends.clear();
                head.scratch_marks.clear();
                head.scratch_return_revision = None;
                record_scratch_landmarks_v1(&mut head, None);
                paths.publish(workspace.head.head_revision, &head, Some(&state), None)?;
                Self::open_profiled_with_source_gate_v1(paths, true)
            }
        }
    }

    fn open_profiled_with_source_gate_v1(
        paths: RunPlayPathsV1,
        require_current_source: bool,
    ) -> Result<(Self, RunPlayOpenProfileV1), String> {
        let total = Instant::now();
        let phase = Instant::now();
        let (workspace, workspace_load_detail) = paths.load_profiled_v1()?;
        if require_current_source && !workspace.head.scratch {
            RunPlayEvidenceIdentityV1::require_current_v1(
                workspace.head.evidence_identity.as_ref(),
            )?;
        }
        let workspace_load = phase.elapsed();
        let archive_nodes = workspace.state.archive.nodes.len();
        let archive_attempts = workspace.state.archive.attempts.len();
        let checkpoint_payload_bytes = workspace.state.checkpoint_payload.len();
        let phase = Instant::now();
        let (checkpoints, checkpoint_decode_detail) =
            TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive_profiled_v1(
                &workspace.state.checkpoint_payload,
                &workspace.state.archive,
                DEFAULT_ARTIFACT_MAX_BYTES,
            )?;
        let checkpoint_decode = phase.elapsed();
        let checkpoint_count = checkpoints.checkpoint_count();
        let current_node_id = workspace.head.position.archive_node_id().to_string();
        let mut active_path_node_ids = BTreeSet::new();
        let mut active_cursor = Some(current_node_id.as_str());
        while let Some(node_id) = active_cursor {
            let node = workspace
                .state
                .archive
                .node(node_id)
                .ok_or_else(|| format!("run-play active path misses node {node_id}"))?;
            active_path_node_ids.insert(node.node_id.as_str());
            active_cursor = node.parent_node_id.as_deref();
        }
        let active_path_nodes = active_path_node_ids.len();
        let off_path_archive_nodes = archive_nodes.saturating_sub(active_path_nodes);
        let active_path_checkpoints = checkpoints
            .identities()
            .filter(|identity| active_path_node_ids.contains(identity.archive_node_id.as_str()))
            .count();
        let (exact, exact_profile) = ExactRunPlaySessionV1::open_in_validated_session_profiled_v1(
            workspace.state.archive,
            checkpoints,
            &current_node_id,
            workspace.head.default_budget,
            workspace.head.combat_plan_options.clone(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )?;
        let mut head = workspace.head;
        let phase = Instant::now();
        refresh_head_decision_from_exact_v1(&mut head, &exact)?;
        let head_refresh = phase.elapsed();
        let cached_head_replayed_public_steps = head
            .current_decision
            .as_ref()
            .map_or(0, |decision| decision.replayed_public_step_count);
        let mut controller = Self {
            paths,
            head,
            base_state_revision: workspace.state.state_revision,
            exact,
            hot_recovery: RunPlayHotRecoveryCacheV1::default(),
        };
        let phase = Instant::now();
        controller.remember_current_hot_snapshot_v1()?;
        let hot_snapshot = phase.elapsed();
        let profile = RunPlayOpenProfileV1 {
            total: total.elapsed(),
            workspace_load,
            workspace_load_detail,
            checkpoint_decode,
            checkpoint_decode_detail,
            exact: exact_profile,
            head_refresh,
            hot_snapshot,
            archive_nodes,
            archive_attempts,
            checkpoint_payload_bytes,
            checkpoint_count,
            active_path_nodes,
            off_path_archive_nodes,
            active_path_checkpoints,
            cached_head_replayed_public_steps,
        };
        Ok((controller, profile))
    }

    pub fn head(&self) -> &RunPlayHeadV1 {
        &self.head
    }

    pub(crate) fn paths(&self) -> &RunPlayPathsV1 {
        &self.paths
    }

    pub fn archive(&self) -> &TaskRunSolutionArchiveV1 {
        self.exact.archive()
    }

    /// Restores the controller's selected retained decision in memory and
    /// returns an exact gameplay checkpoint suitable for an external paired
    /// root. This never publishes a head move or navigation event. Controller
    /// diagnostics that cannot affect simulator mechanics are deliberately
    /// removed before the checkpoint leaves run-play.
    pub(crate) fn current_external_exact_checkpoint_v1(
        &mut self,
    ) -> Result<RunControlSessionCheckpointV1, String> {
        self.restore_exact_at_published_head_v1()?;
        let mut checkpoint = self.exact.current_run_decision_checkpoint_v1()?;
        checkpoint.clear_non_gameplay_diagnostics_for_external_checkpoint();
        // Decode once at the ownership boundary. A checkpoint that cannot
        // reconstruct a session is not valid recovery evidence.
        checkpoint.clone().into_session()?;
        Ok(checkpoint)
    }

    /// Prepares caller-selected resource choices on an isolated exact branch.
    /// Travel/progression and combat remain outside this diagnostic; a pending
    /// card selection must be resolved before its resources can be evaluated.
    pub(crate) fn prepare_resources_for_deck_lab_v1(
        &mut self,
        selectors: &[RunPlayActionSelectorV1],
    ) -> Result<(ExactRunPlayPreparedLineV1, Vec<String>), String> {
        validate_anchor_line_v1(selectors)?;
        self.restore_exact_at_published_head_v1()?;
        let mut actions = Vec::new();
        let prepared = self.exact.prepare_line_resolving_v1(
            selectors.len(),
            &mut |index, decision| {
                let choice = resolve_action_selector_v1(decision, &selectors[index])?;
                match choice.ordinal_path.first().and_then(|ordinal| decision.candidates.get(*ordinal)) {
                    Some(TaskPublicCandidateV1::Strategic { action }) if !matches!(action,
                        TaskStrategicActionV1::ChooseRouteNode { .. } | TaskStrategicActionV1::Proceed { .. }
                    ) => {}
                    _ => return Err("deck-lab after requires current preparation choices; travel, progression and combat use the normal game commands".into()),
                }
                actions.push(semantic_action_for_choice_v1(decision, &choice)?);
                Ok(choice)
            },
        )?;
        let endpoint = prepared
            .current_decision()
            .ok_or("deck-lab preparation reached a terminal outcome instead of ready resources")?;
        match endpoint.site {
            TaskDecisionSiteV1::Combat => return Err("deck-lab preparation entered combat; use the normal game command for that encounter".into()),
            TaskDecisionSiteV1::RunChoice => return Err("deck-lab preparation has a pending card selection; include its select action after the triggering choice".into()),
            _ => {}
        }
        Ok((prepared, actions))
    }

    /// Reads the exact run-level and active-combat RNG streams from the current
    /// decision for an explicitly privileged Java/Rust parity experiment.
    pub fn current_rngs_for_parity_v2(
        &mut self,
    ) -> Result<
        (
            sts_oracle_learning::runtime::rng::StsRng,
            sts_oracle_learning::runtime::rng::StsRng,
            sts_oracle_learning::runtime::rng::StsRng,
            Option<sts_oracle_learning::runtime::rng::RngPool>,
        ),
        String,
    > {
        self.restore_exact_at_published_head_v1()?;
        let checkpoint = self.exact.current_run_decision_checkpoint_v1()?;
        let card_rng = checkpoint.card_rng_for_run_play_parity_v1();
        let neow_rng = checkpoint.neow_rng_for_run_play_parity_v1()?;
        let session = checkpoint.into_session()?;
        let map_rng = session.run_state.map_rng.clone();
        let combat_rng = session
            .active_combat
            .map(|active| active.combat_state.rng.pool);
        Ok((card_rng, neow_rng, map_rng, combat_rng))
    }

    /// Reads both durable exact-state and future-continuation identities from
    /// one retained decision. The latter intentionally ignores bounded
    /// controller diagnostics that cannot alter future simulator behavior.
    pub fn current_state_fingerprints_v1(&mut self) -> Result<(String, String), String> {
        self.restore_exact_at_published_head_v1()?;
        let checkpoint = self.exact.current_run_decision_checkpoint_v1()?;
        let session = checkpoint.into_session()?;
        Ok((
            run_control_session_fingerprint_v3(&session),
            run_control_continuation_fingerprint_v1(&session),
        ))
    }

    /// Returns the current exact strategic owner's public recommendation
    /// without mutating the controller head or its live exact world.
    pub fn advise_v1(&mut self) -> Result<RunPlayStrategicOwnerAdviceV1, String> {
        self.restore_exact_at_published_head_v1()?;
        let decision = self
            .exact
            .current_public_decision()
            .ok_or_else(|| "run-play advice requires a public decision".to_string())?;
        if decision.site == sts_oracle_learning::agent::task::TaskDecisionSiteV1::Combat {
            return Err(
                "run-play advise is a strategic-owner surface, not a combat planner".to_string(),
            );
        }
        let support = self.exact.current_privileged_strategic_support_v1()?;
        let semantic_action = match &support.outcome {
            TaskPrivilegedStrategicSupportOutcomeV1::Choice { choice } => {
                Some(semantic_action_for_choice_v1(decision, choice)?)
            }
            TaskPrivilegedStrategicSupportOutcomeV1::Unavailable { .. } => None,
        };
        Ok(RunPlayStrategicOwnerAdviceV1 {
            support,
            semantic_action,
        })
    }

    pub fn advice_detail_v1(&mut self) -> Result<RunPlayAdviceDetailV1, String> {
        self.restore_exact_at_published_head_v1()?;
        let site = self
            .exact
            .current_public_decision()
            .ok_or_else(|| "run-play advice detail requires a public decision".to_string())?
            .site;
        match site {
            TaskDecisionSiteV1::Map => self
                .exact
                .current_exact_route_policy_audit_v1()
                .map(RunPlayAdviceDetailV1::Route),
            TaskDecisionSiteV1::CardReward => self
                .exact
                .current_exact_card_reward_policy_audit_v1()
                .map(RunPlayAdviceDetailV1::CardReward),
            TaskDecisionSiteV1::Campfire => self
                .exact
                .current_exact_campfire_policy_audit_v1()
                .map(RunPlayAdviceDetailV1::Campfire),
            TaskDecisionSiteV1::BossRelic => self
                .exact
                .current_exact_boss_relic_policy_audit_v1()
                .map(RunPlayAdviceDetailV1::BossRelic),
            TaskDecisionSiteV1::Shop => self
                .exact
                .current_exact_shop_policy_audit_v1()
                .map(RunPlayAdviceDetailV1::Shop),
            TaskDecisionSiteV1::RunChoice => self
                .exact
                .current_exact_run_choice_policy_audit_v1()
                .map(RunPlayAdviceDetailV1::RunChoice),
            TaskDecisionSiteV1::Reward => {
                self.exact
                    .current_potion_reward_policy_audit_v1()
                    .map(|audit| {
                        audit.map_or(
                            RunPlayAdviceDetailV1::Unavailable { site },
                            RunPlayAdviceDetailV1::PotionReward,
                        )
                    })
            }
            TaskDecisionSiteV1::Event => self
                .current_event_advice_detail_v1()
                .map(RunPlayAdviceDetailV1::Event),
            site => Ok(RunPlayAdviceDetailV1::Unavailable { site }),
        }
    }

    /// Returns value-free path-family facts for every currently legal public
    /// map entry. This is intentionally distinct from strategic route advice
    /// and is available at non-Map decisions such as Neow.
    pub fn route_entry_paths_v1(
        &mut self,
        path_budget: usize,
    ) -> Result<Vec<RouteEntryPathSummaryV1>, String> {
        self.restore_exact_at_published_head_v1()?;
        self.exact.current_public_route_entry_paths_v1(path_budget)
    }

    fn current_event_advice_detail_v1(&self) -> Result<RunPlayEventAdviceDetailV1, String> {
        let root = self
            .exact
            .current_public_decision()
            .ok_or_else(|| "run-play event detail requires a public decision".to_string())?;
        if root.site != TaskDecisionSiteV1::Event {
            return Err(format!(
                "run-play event detail requires Event, observed {:?}",
                root.site
            ));
        }
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "run-play event detail requires an archive node".to_string())?
            .to_string();
        let mut queue = event_action_selectors_v1(root)
            .into_iter()
            .map(|selector| vec![selector])
            .collect::<VecDeque<_>>();
        if queue.is_empty() {
            return Err("run-play event detail found no event options".to_string());
        }
        let mut lines = Vec::new();
        let mut truncated = false;
        while let Some(line) = queue.pop_front() {
            let probe = self
                .exact
                .probe_line_resolving_v1(line.len(), &mut |index, decision| {
                    resolve_action_selector_v1(decision, &line[index])
                })?;
            let next = probe
                .steps
                .last()
                .and_then(|step| step.next_decision.as_ref());
            let next_selectors = next
                .filter(|decision| decision.site == TaskDecisionSiteV1::Event)
                .map(event_action_selectors_v1)
                .unwrap_or_default();
            if next_selectors.is_empty() {
                lines.push(probe);
                continue;
            }
            if line.len() >= MAX_EVENT_FRONTIER_STEPS {
                truncated = true;
                lines.push(probe);
                continue;
            }
            for selector in next_selectors {
                if lines.len() + queue.len() >= MAX_EVENT_FRONTIER_LINES {
                    truncated = true;
                    break;
                }
                let mut child = line.clone();
                child.push(selector);
                queue.push_back(child);
            }
        }
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err("run-play event detail changed its exact session root".to_string());
        }
        Ok(RunPlayEventAdviceDetailV1 {
            direct_card_acquisition: self
                .exact
                .current_exact_direct_card_acquisition_policy_audit_v1()
                .ok(),
            comparison: RunPlayComparisonV1 {
                root_archive_node_id,
                lines,
            },
            truncated,
        })
    }

    pub fn combat_research_attempt_with_options_and_progress_v1(
        &mut self,
        options: Option<CombatResearchPlanOptionsV1>,
        deadline: Option<std::time::Instant>,
        progress: &mut dyn FnMut(CombatResearchProgressEventV1),
    ) -> Result<RunPlayCombatResearchAttemptV1, String> {
        self.restore_exact_at_published_head_v1()?;
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "run-play combat research requires a public root".to_string())?
            .to_string();
        let portfolio = self
            .exact
            .current_combat_research_portfolio_with_options_and_progress_v1(
                options, deadline, progress,
            )?;
        let mut attempt = portfolio.attempt;
        if !portfolio.witness_replays.is_empty()
            && portfolio.witness_replays.len() != attempt.candidates.len()
        {
            return Err(
                "combat research retained replay count differs from candidates".to_string(),
            );
        }
        let visible = combat_research_frontier_indices_v1(&attempt.candidates);
        let mut replays = portfolio.witness_replays.into_iter();
        let mut visible_replays = Vec::with_capacity(visible.len());
        let mut index = 0;
        attempt.candidates.retain(|_| {
            let keep = visible.binary_search(&index).is_ok();
            let replay = replays.next().flatten();
            if keep {
                visible_replays.push(replay);
            }
            index += 1;
            keep
        });
        let mut witness_lines = Vec::with_capacity(attempt.candidates.len());
        for (candidate, replay) in attempt.candidates.iter().zip(visible_replays) {
            if let Some(replay) = replay {
                witness_lines.push(combat_research_probe_from_replay_v1(
                    candidate,
                    replay,
                    self.exact.archive(),
                )?);
                continue;
            }
            let plan = &candidate.plan;
            let probe =
                self.exact
                    .probe_line_resolving_v1(plan.steps.len(), &mut |index, decision| {
                        let step = &plan.steps[index];
                        if step.source_public_snapshot_id != decision.information.snapshot_id {
                            return Err(format!(
                                "research plan step {} expected snapshot {}, observed {}",
                                index + 1,
                                step.source_public_snapshot_id,
                                decision.information.snapshot_id
                            ));
                        }
                        Ok(step.choice.clone())
                    })?;
            witness_lines.push(probe);
        }
        let mut milestone_candidates = Vec::with_capacity(portfolio.milestone_candidates.len());
        for materialization in portfolio.milestone_candidates {
            let candidate = materialization.candidate;
            let plan = &candidate.plan;
            if materialization.source_decisions.len() != plan.steps.len() {
                return Err("milestone public replay decisions and plan steps disagree".to_string());
            }
            let action_line = materialization
                .source_decisions
                .iter()
                .zip(&plan.steps)
                .enumerate()
                .map(|(index, (decision, step))| {
                    if step.source_public_snapshot_id != decision.information.snapshot_id {
                        return Err(format!(
                            "milestone plan step {} expected snapshot {}, materialized {}",
                            index + 1,
                            step.source_public_snapshot_id,
                            decision.information.snapshot_id,
                        ));
                    }
                    semantic_action_for_choice_v1(decision, &step.choice)
                })
                .collect::<Result<Vec<_>, String>>()?;
            let candidate_id =
                stable_planner_id("run_play_combat_milestone_candidate_v1", &candidate)?;
            let successor_decision = materialization.successor_decision;
            let TaskPublicObservationV1::Combat {
                observation: successor,
                ..
            } = successor_decision.observation
            else {
                return Err(
                    "milestone witness successor did not carry a public combat observation"
                        .to_string(),
                );
            };
            let successor_public_snapshot_id = successor_decision.information.snapshot_id.clone();
            milestone_candidates.push(RunPlayCombatMilestoneResearchCandidateV1 {
                candidate_id,
                candidate,
                action_line,
                successor_public_snapshot_id,
                successor,
            });
        }
        let mut bounded_progress = Vec::with_capacity(portfolio.bounded_progress.len());
        for progress in portfolio.bounded_progress {
            let CombatResearchBoundedProgressMaterializationV1 {
                provenance,
                max_player_turn,
                recent_turn_survival_envelope,
                paths,
            } = progress;
            let mut replayed_paths = Vec::with_capacity(paths.len());
            for materialization in paths {
                let candidate = materialization.candidate;
                let plan = &candidate.plan;
                if materialization.source_decisions.len() != plan.steps.len() {
                    return Err(
                        "bounded progress public replay decisions and plan steps disagree"
                            .to_string(),
                    );
                }
                let action_line = materialization
                    .source_decisions
                    .iter()
                    .zip(&plan.steps)
                    .enumerate()
                    .map(|(index, (decision, step))| {
                        if step.source_public_snapshot_id != decision.information.snapshot_id {
                            return Err(format!(
                                "bounded progress plan step {} expected snapshot {}, materialized {}",
                                index + 1,
                                step.source_public_snapshot_id,
                                decision.information.snapshot_id,
                            ));
                        }
                        semantic_action_for_choice_v1(decision, &step.choice)
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let successor_decision = materialization.successor_decision;
                let TaskPublicObservationV1::Combat {
                    observation: successor,
                    ..
                } = successor_decision.observation
                else {
                    return Err(
                        "bounded progress prefix did not end inside the root combat".to_string()
                    );
                };
                let replayed_endpoint =
                    combat_research_progress_state_from_public_v1(&successor, plan.steps.len());
                if replayed_endpoint != candidate.endpoint {
                    return Err(format!(
                        "bounded progress replay endpoint drifted: producer={} roles={:?} expected={:?} observed={:?}",
                        provenance.producer_name,
                        candidate.roles,
                        candidate.endpoint,
                        replayed_endpoint,
                    ));
                }
                let successor_public_snapshot_id =
                    successor_decision.information.snapshot_id.clone();
                replayed_paths.push(RunPlayCombatResearchBoundedProgressPathV1 {
                    candidate,
                    action_line,
                    successor_public_snapshot_id,
                });
            }
            bounded_progress.push(RunPlayCombatResearchBoundedProgressV1 {
                provenance,
                max_player_turn,
                recent_turn_survival_envelope,
                paths: replayed_paths,
            });
        }
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err("run-play combat research changed its exact session root".to_string());
        }
        Ok(RunPlayCombatResearchAttemptV1 {
            attempt,
            witness_lines,
            milestone_candidates,
            bounded_progress,
        })
    }

    /// Audits a bounded set of cached milestone successors on private exact
    /// forks. A returned continuation proves one newly completed typed
    /// milestone (or terminal victory) through one additional complete player
    /// turn. Absence is deliberately Unknown because the proposal layer is a
    /// bounded generated surface, not an exhaustive refutation engine.
    pub fn audit_cached_combat_milestone_continuations_v1(
        &mut self,
        root_head_revision: u64,
        candidates: &[RunPlayCombatMilestoneResearchCandidateV1],
        candidate_ids: &[String],
        config: ExactCompleteTurnProposalConfigV1,
    ) -> Result<Vec<RunPlayCombatMilestoneContinuationAuditV1>, String> {
        if self.head.head_revision != root_head_revision {
            return Err(format!(
                "cached combat milestone continuation audit belongs to head {root_head_revision}, current head is {}",
                self.head.head_revision
            ));
        }
        self.restore_exact_at_published_head_v1()?;
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| {
                "combat milestone continuation audit requires a public root".to_string()
            })?
            .to_string();
        let mut audits = Vec::with_capacity(candidate_ids.len());
        for candidate_id in candidate_ids {
            let mut matches = candidates
                .iter()
                .filter(|candidate| candidate.candidate_id == *candidate_id);
            let selected = matches.next().ok_or_else(|| {
                format!(
                    "cached combat milestone continuation candidate id is absent: {candidate_id}"
                )
            })?;
            if matches.next().is_some() {
                return Err(format!(
                    "cached combat milestone continuation candidate id is ambiguous: {candidate_id}"
                ));
            }
            let plan = &selected.candidate.plan;
            let prepared = self.exact.prepare_line_resolving_v1(
                plan.steps.len(),
                &mut |step_index, decision| {
                    let step = &plan.steps[step_index];
                    if step.source_public_snapshot_id != decision.information.snapshot_id {
                        return Err(format!(
                            "combat milestone continuation parent step {} expected snapshot {}, observed {}",
                            step_index + 1,
                            step.source_public_snapshot_id,
                            decision.information.snapshot_id
                        ));
                    }
                    Ok(step.choice.clone())
                },
            )?;
            if prepared.terminal_outcome().is_some()
                || prepared
                    .current_decision()
                    .is_none_or(|decision| decision.site != TaskDecisionSiteV1::Combat)
            {
                return Err(
                    "combat milestone continuation parent did not end at unresolved combat"
                        .to_string(),
                );
            }
            let continuation_root = prepared.current_combat_position_for_run_play_v1()?;
            let batch = complete_turn_surface_with_config_v1(&continuation_root, config);
            let nodes_expanded = batch.nodes_expanded;
            let nodes_generated = batch.nodes_generated;
            let engine_steps = batch.engine_steps;
            let exact_state_skips = batch.exact_state_skips;
            let truncated_children = batch.truncated_children;
            let preselection_candidate_count = batch.preselection_candidate_count;
            let parent_milestones = selected.candidate.transition.completed_milestones();
            let mut continuation_ids = BTreeSet::new();
            let mut continuations = Vec::new();
            for exact in batch.candidates {
                let view = project_complete_turn_candidate_v1(
                    &continuation_root,
                    &exact,
                    &[],
                    config.max_engine_steps_per_action,
                )?;
                let new_completed_milestones = view
                    .plan_transition
                    .as_ref()
                    .map(|transition| {
                        transition
                            .completed_milestones()
                            .iter()
                            .filter(|milestone| !parent_milestones.contains(milestone))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let terminal_win =
                    view.boundary == ExactCompleteTurnProposalBoundaryV1::TerminalWin;
                if !terminal_win && new_completed_milestones.is_empty() {
                    continue;
                }
                let continuation_id = stable_planner_id(
                    "run_play_combat_milestone_continuation_v1",
                    &(candidate_id, &new_completed_milestones, &view),
                )?;
                if !continuation_ids.insert(continuation_id.clone()) {
                    continue;
                }
                continuations.push(RunPlayCombatMilestoneContinuationCandidateV1 {
                    continuation_id,
                    new_completed_milestones,
                    view,
                });
            }
            let status = if continuations.is_empty() {
                RunPlayCombatMilestoneContinuationAuditStatusV1::UnknownInBoundedSurface
            } else {
                RunPlayCombatMilestoneContinuationAuditStatusV1::VerifiedContinuation
            };
            audits.push(RunPlayCombatMilestoneContinuationAuditV1 {
                candidate_id: candidate_id.clone(),
                successor_public_snapshot_id: selected.successor_public_snapshot_id.clone(),
                status,
                bounded_surface_only: true,
                absence_means_unknown: true,
                nodes_expanded,
                nodes_generated,
                engine_steps,
                exact_state_skips,
                truncated_children,
                preselection_candidate_count,
                continuations,
            });
        }
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err(
                "combat milestone continuation audit changed its exact session root".to_string(),
            );
        }
        Ok(audits)
    }

    /// Replays one durable semantic combat prefix on a private fork, binds its
    /// public endpoint, and audits up to two additional complete-turn layers.
    /// This avoids regenerating the portfolio that originally discovered the
    /// prefix. A surface containing only retained terminal losses remains a
    /// bounded negative observation, never an exhaustive Loss proof.
    pub fn audit_combat_prefix_horizon_v1(
        &mut self,
        root_head_revision: u64,
        lineage_id: &str,
        selectors: &[RunPlayActionSelectorV1],
        expected_successor_public_snapshot_id: &str,
        additional_turns: usize,
        config: ExactCompleteTurnProposalConfigV1,
    ) -> Result<RunPlayCombatPrefixHorizonAuditV1, String> {
        if self.head.head_revision != root_head_revision {
            return Err(format!(
                "combat prefix horizon audit belongs to head {root_head_revision}, current head is {}",
                self.head.head_revision
            ));
        }
        if lineage_id.is_empty() {
            return Err("combat prefix horizon audit requires a lineage id".to_string());
        }
        if selectors.is_empty() {
            return Err("combat prefix horizon audit requires a semantic prefix".to_string());
        }
        if !(1..=MAX_COMBAT_PREFIX_HORIZON_TURNS).contains(&additional_turns) {
            return Err(format!(
                "combat prefix horizon audit requires 1..={MAX_COMBAT_PREFIX_HORIZON_TURNS} additional turns"
            ));
        }
        self.restore_exact_at_published_head_v1()?;
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "combat prefix horizon audit requires a public root".to_string())?
            .to_string();
        let prepared = self
            .exact
            .prepare_line_resolving_v1(selectors.len(), &mut |index, decision| {
                resolve_action_selector_v1(decision, &selectors[index])
            })?;
        if prepared.terminal_outcome().is_some() {
            return Err(
                "combat prefix horizon audit prefix reached a terminal run outcome".to_string(),
            );
        }
        let successor_decision = prepared.current_decision().ok_or_else(|| {
            "combat prefix horizon audit prefix has no public successor".to_string()
        })?;
        if successor_decision.site != TaskDecisionSiteV1::Combat {
            return Err(format!(
                "combat prefix horizon audit requires an unresolved combat successor, observed {:?}",
                successor_decision.site
            ));
        }
        let actual_snapshot_id = successor_decision.public_snapshot_id.clone();
        if actual_snapshot_id != expected_successor_public_snapshot_id {
            return Err(format!(
                "combat prefix horizon audit expected successor snapshot {expected_successor_public_snapshot_id}, observed {actual_snapshot_id}"
            ));
        }
        let mut frontier = vec![prepared.current_combat_position_for_run_play_v1()?];
        let mut levels = Vec::with_capacity(additional_turns);
        for additional_turn in 1..=additional_turns {
            let parent_count = frontier.len();
            if parent_count == 0 {
                break;
            }
            let final_level = additional_turn == additional_turns;
            let mut next_frontier = Vec::new();
            let mut nodes_expanded = 0usize;
            let mut nodes_generated = 0usize;
            let mut engine_steps = 0usize;
            let mut exact_state_skips = 0usize;
            let mut truncated_children = 0usize;
            let mut preselection_candidate_count = 0usize;
            let mut retained_candidate_count = 0usize;
            let mut terminal_win_count = 0usize;
            let mut terminal_loss_count = 0usize;
            let mut unresolved_count = 0usize;
            let mut other_count = 0usize;
            let mut seen_public_successors = BTreeSet::new();
            let mut public_unresolved_successor_count = 0usize;
            let mut public_unresolved_successors = Vec::new();
            let mut public_successor_projection_truncated = false;
            for parent in frontier {
                let batch = complete_turn_surface_with_config_v1(&parent, config);
                nodes_expanded = nodes_expanded.saturating_add(batch.nodes_expanded);
                nodes_generated = nodes_generated.saturating_add(batch.nodes_generated);
                engine_steps = engine_steps.saturating_add(batch.engine_steps);
                exact_state_skips = exact_state_skips.saturating_add(batch.exact_state_skips);
                truncated_children = truncated_children.saturating_add(batch.truncated_children);
                preselection_candidate_count =
                    preselection_candidate_count.saturating_add(batch.preselection_candidate_count);
                retained_candidate_count =
                    retained_candidate_count.saturating_add(batch.candidates.len());
                for exact in batch.candidates {
                    match exact.boundary {
                        ExactCompleteTurnProposalBoundaryV1::TerminalWin => {
                            terminal_win_count = terminal_win_count.saturating_add(1);
                        }
                        ExactCompleteTurnProposalBoundaryV1::TerminalLoss => {
                            terminal_loss_count = terminal_loss_count.saturating_add(1);
                        }
                        ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn => {
                            unresolved_count = unresolved_count.saturating_add(1);
                            if final_level {
                                let view = project_complete_turn_candidate_v1(
                                    &parent,
                                    &exact,
                                    &[],
                                    config.max_engine_steps_per_action,
                                )?;
                                let successor_id = stable_planner_id(
                                    "run_play_combat_prefix_horizon_successor_v1",
                                    &view,
                                )?;
                                if seen_public_successors.insert(successor_id.clone()) {
                                    public_unresolved_successor_count =
                                        public_unresolved_successor_count.saturating_add(1);
                                    if public_unresolved_successors.len()
                                        < MAX_COMBAT_PREFIX_HORIZON_PUBLIC_SUCCESSORS
                                    {
                                        public_unresolved_successors.push(
                                            RunPlayCombatPrefixHorizonSuccessorV1 {
                                                successor_id,
                                                view,
                                            },
                                        );
                                    } else {
                                        public_successor_projection_truncated = true;
                                    }
                                }
                            } else {
                                next_frontier.push(exact.successor);
                            }
                        }
                        ExactCompleteTurnProposalBoundaryV1::Other => {
                            other_count = other_count.saturating_add(1);
                        }
                    }
                }
            }
            levels.push(RunPlayCombatPrefixHorizonLevelV1 {
                additional_turn,
                parent_count,
                nodes_expanded,
                nodes_generated,
                engine_steps,
                exact_state_skips,
                truncated_children,
                preselection_candidate_count,
                retained_candidate_count,
                terminal_win_count,
                terminal_loss_count,
                unresolved_count,
                other_count,
                public_unresolved_successor_count,
                public_successor_projection_truncated,
                public_unresolved_successors,
            });
            if final_level || next_frontier.is_empty() {
                break;
            }
            frontier = next_frontier;
        }
        let status = combat_prefix_horizon_status_v1(&levels);
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err("combat prefix horizon audit changed its exact session root".to_string());
        }
        Ok(RunPlayCombatPrefixHorizonAuditV1 {
            lineage_id: lineage_id.to_string(),
            prefix_action_count: selectors.len(),
            successor_public_snapshot_id: actual_snapshot_id,
            requested_additional_turns: additional_turns,
            completed_additional_turns: levels.len(),
            status,
            bounded_surface_only: true,
            absence_means_unknown: true,
            levels,
        })
    }

    /// Commits one cached finite milestone prefix at its unchanged exact root.
    /// The plan is replay-prepared transactionally and remains distinct from
    /// a terminal combat-research candidate in the durable receipt.
    pub fn anchor_cached_combat_milestone_candidate_id_v1(
        &mut self,
        root_head_revision: u64,
        candidates: &[RunPlayCombatMilestoneResearchCandidateV1],
        candidate_id: &str,
    ) -> Result<(), String> {
        if self.head.head_revision != root_head_revision {
            return Err(format!(
                "cached combat milestone belongs to head {root_head_revision}, current head is {}",
                self.head.head_revision
            ));
        }
        let mut matches = candidates
            .iter()
            .filter(|candidate| candidate.candidate_id == candidate_id);
        let selected = matches.next().ok_or_else(|| {
            format!("cached combat milestone candidate id is absent: {candidate_id}")
        })?;
        if matches.next().is_some() {
            return Err(format!(
                "cached combat milestone candidate id is ambiguous: {candidate_id}"
            ));
        }
        let plan = &selected.candidate.plan;
        let prepared =
            self.exact
                .prepare_line_resolving_v1(plan.steps.len(), &mut |index, decision| {
                    let step = &plan.steps[index];
                    if step.source_public_snapshot_id != decision.information.snapshot_id {
                        return Err(format!(
                            "combat milestone step {} expected snapshot {}, observed {}",
                            index + 1,
                            step.source_public_snapshot_id,
                            decision.information.snapshot_id
                        ));
                    }
                    Ok(step.choice.clone())
                })?;
        if prepared.terminal_outcome().is_some()
            || prepared
                .current_decision()
                .is_none_or(|decision| decision.site != TaskDecisionSiteV1::Combat)
        {
            return Err(
                "combat milestone replay did not end at an unresolved combat decision".to_string(),
            );
        }
        let final_position = prepared.current_combat_position_for_run_play_v1()?;
        if combat_terminal(&final_position.engine, &final_position.combat)
            != CombatTerminal::Unresolved
        {
            return Err("combat milestone replay reached a terminal combat state".to_string());
        }
        if combat_plan_projection_v1(&final_position).as_ref()
            != Some(&selected.candidate.successor_plan)
            || final_position.combat.entities.player.current_hp
                != selected.candidate.final_player_hp
        {
            return Err(
                "combat milestone replay no longer matches its cached typed acceptance proof"
                    .to_string(),
            );
        }
        let evidence = RunPlayCombatMilestoneCandidateV1 {
            provenance: selected.candidate.provenance.clone(),
            transition: selected.candidate.transition.clone(),
            successor_plan: selected.candidate.successor_plan.clone(),
            final_player_hp: selected.candidate.final_player_hp,
        };
        self.publish_prepared_line_v1(prepared, None, Some(evidence))
    }

    /// Enumerates one bounded same-turn surface and projects each retained
    /// endpoint to public-only evidence. Candidates with identical public
    /// views collapse to one canonical representative, so hidden RNG state
    /// cannot become a model-visible choice.
    pub fn combat_turn_attempt_v1(
        &mut self,
        config: ExactCompleteTurnProposalConfigV1,
    ) -> Result<RunPlayCombatTurnAttemptV1, String> {
        self.restore_exact_at_published_head_v1()?;
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "run-play combat turn requires a public root".to_string())?
            .to_string();
        let root = self.exact.current_combat_position_for_run_play_v1()?;
        let batch = complete_turn_surface_with_config_v1(&root, config);
        let mut seen_public_ids = BTreeSet::new();
        let mut candidates = Vec::new();
        let mut exact_candidates = Vec::new();
        for exact in batch.candidates {
            let view = project_complete_turn_candidate_v1(
                &root,
                &exact,
                &[],
                config.max_engine_steps_per_action,
            )?;
            let candidate_id = complete_turn_candidate_id_v1(&view)?;
            if !seen_public_ids.insert(candidate_id.clone()) {
                continue;
            }
            candidates.push(RunPlayCombatTurnCandidateV1 { candidate_id, view });
            exact_candidates.push(exact);
        }
        if candidates.is_empty() {
            return Err("run-play complete-turn surface contains no public candidate".to_string());
        }
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err("run-play combat turn changed its exact session root".to_string());
        }
        Ok(RunPlayCombatTurnAttemptV1 {
            nodes_expanded: batch.nodes_expanded,
            nodes_generated: batch.nodes_generated,
            engine_steps: batch.engine_steps,
            exact_state_skips: batch.exact_state_skips,
            truncated_children: batch.truncated_children,
            preselection_candidate_count: batch.preselection_candidate_count,
            candidates,
            exact_candidates,
        })
    }

    /// Reserves one public semantic complete-turn hypothesis into an existing
    /// hot candidate cache. The prefix generator must independently produce
    /// the exact line when its prefixes are retained; only that generated
    /// exact candidate is appended and becomes eligible for the ordinary
    /// continuation-audit and commit paths.
    pub fn reserve_cached_combat_turn_line_v1(
        &mut self,
        root_head_revision: u64,
        attempt: &mut RunPlayCombatTurnAttemptV1,
        selectors: &[RunPlayActionSelectorV1],
        config: ExactCompleteTurnProposalConfigV1,
    ) -> Result<RunPlayCombatTurnReservationV1, String> {
        if self.head.head_revision != root_head_revision {
            return Err(format!(
                "cached combat-turn reservation belongs to head {root_head_revision}, current head is {}",
                self.head.head_revision
            ));
        }
        validate_comparison_lines_v1(&[selectors.to_vec()])?;
        self.restore_exact_at_published_head_v1()?;
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "combat-turn reservation requires a public root".to_string())?
            .to_string();
        let probe = self
            .exact
            .probe_line_resolving_v1(selectors.len(), &mut |index, decision| {
                resolve_action_selector_v1(decision, &selectors[index])
            })?;
        let choices = probe
            .steps
            .iter()
            .map(|step| step.choice.clone())
            .collect::<Vec<_>>();
        let root = self.exact.current_combat_position_for_run_play_v1()?;
        let exact = reserve_complete_turn_target_v1(&root, &choices, config)?;
        let view = project_complete_turn_candidate_v1(
            &root,
            &exact,
            &[],
            config.max_engine_steps_per_action,
        )?;
        let candidate_id = complete_turn_candidate_id_v1(&view)?;
        let candidate = RunPlayCombatTurnCandidateV1 { candidate_id, view };
        let already_present = attempt
            .candidates
            .iter()
            .any(|existing| existing.candidate_id == candidate.candidate_id);
        if !already_present {
            attempt.candidates.push(candidate.clone());
            attempt.exact_candidates.push(exact);
        }
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err("combat-turn reservation changed its exact session root".to_string());
        }
        Ok(RunPlayCombatTurnReservationV1 {
            candidate,
            already_present,
            candidate_count: attempt.candidates.len(),
        })
    }

    /// Searches a small caller-selected set of cached complete-turn exact
    /// successors without publishing the head. Every missing suffix witness
    /// remains Unknown because each search has an explicit finite contract.
    pub fn audit_cached_combat_turn_continuations_v1(
        &mut self,
        root_head_revision: u64,
        attempt: &RunPlayCombatTurnAttemptV1,
        candidate_ids: &[String],
        options: RunPlayCombatTurnContinuationAuditOptionsV1,
    ) -> Result<RunPlayCombatTurnContinuationAuditBatchV1, String> {
        options.validate()?;
        if self.head.head_revision != root_head_revision {
            return Err(format!(
                "cached combat-turn continuation audit belongs to head {root_head_revision}, current head is {}",
                self.head.head_revision
            ));
        }
        if attempt.candidates.len() != attempt.exact_candidates.len() {
            return Err("cached combat-turn public and exact candidates disagree".to_string());
        }
        let mut distinct_ids = BTreeSet::new();
        if candidate_ids
            .iter()
            .any(|candidate_id| !distinct_ids.insert(candidate_id.as_str()))
        {
            return Err("combat-turn continuation candidate ids must be distinct".to_string());
        }
        self.restore_exact_at_published_head_v1()?;
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "combat-turn continuation audit requires a public root".to_string())?
            .to_string();
        let published_combat_root = self.exact.current_combat_position_for_run_play_v1()?;
        let mut audits = Vec::with_capacity(candidate_ids.len());
        let mut observed_terminal_candidates = Vec::new();
        for candidate_id in candidate_ids {
            let mut matches =
                attempt
                    .candidates
                    .iter()
                    .enumerate()
                    .filter_map(|(index, candidate)| {
                        (candidate.candidate_id == *candidate_id).then_some(index)
                    });
            let index = matches.next().ok_or_else(|| {
                format!("cached combat-turn continuation candidate id is absent: {candidate_id}")
            })?;
            if matches.next().is_some() {
                return Err(format!(
                    "cached combat-turn continuation candidate id is ambiguous: {candidate_id}"
                ));
            }
            let public = &attempt.candidates[index];
            let exact = &attempt.exact_candidates[index];
            if exact.boundary != public.view.boundary {
                return Err(format!(
                    "combat-turn candidate {candidate_id} public and exact boundaries disagree"
                ));
            }
            if exact.actions.len() != public.view.actions.len() {
                return Err(format!(
                    "combat-turn candidate {candidate_id} public and exact action counts disagree"
                ));
            }
            let forced_action_count = exact.actions.len();
            let forced_potions_used = exact.potion_expenditures;
            let outside_potion_contract = fixed_turn_exceeds_total_potion_contract_v1(
                forced_potions_used,
                options.total_max_potions_used,
            );
            let remaining_max_potions_used = match options.total_max_potions_used {
                Some(_) if outside_potion_contract => Some(0),
                Some(limit) => Some(limit - forced_potions_used),
                None => None,
            };
            let potion_contract = RunPlayCombatTurnContinuationPotionContractV1 {
                fixed_turn_potion_slot_mask: exact.potion_slot_mask,
                fixed_turn_potion_expenditures: forced_potions_used,
                total_max_potion_expenditures: options.total_max_potions_used,
                suffix_max_potion_expenditures: remaining_max_potions_used,
            };
            if outside_potion_contract {
                audits.push(RunPlayCombatTurnContinuationAuditV1 {
                    candidate_id: candidate_id.clone(),
                    scope:
                        RunPlayCombatTurnContinuationAuditScopeV1::FixedTurnOutsidePotionContract,
                    status: RunPlayCombatTurnContinuationAuditStatusV1::UnknownInBoundedSearch,
                    exit: RunPlayCombatTurnContinuationAuditExitV1::FixedTurnPotionContractExceeded,
                    bounded_search_only: true,
                    absence_means_unknown: true,
                    potion_contract,
                    search_spec: None,
                    work: RunPlayCombatTurnContinuationAuditWorkV1 {
                        selections: 0,
                        generation_work: 0,
                        engine_steps: 0,
                        exact_nodes: 0,
                        completed_turn_options: 0,
                    },
                    bounded_progress: None,
                    witness: None,
                    terminal_outcome_frontier: Vec::new(),
                });
                continue;
            }
            if exact.boundary == ExactCompleteTurnProposalBoundaryV1::TerminalWin {
                let replayed = project_complete_turn_candidate_v1(
                    &published_combat_root,
                    exact,
                    &[],
                    options.max_engine_steps_per_transition,
                )?;
                if replayed.actions != public.view.actions
                    || replayed.successor != public.view.successor
                {
                    return Err(format!(
                        "combat-turn terminal candidate {candidate_id} replay projection drifted"
                    ));
                }
                if combat_terminal(&exact.successor.engine, &exact.successor.combat)
                    != CombatTerminal::Win
                {
                    return Err(format!(
                        "combat-turn candidate {candidate_id} claims a non-winning terminal"
                    ));
                }
                let inputs = exact
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect::<Vec<_>>();
                let outcome =
                    LocalTurnGraphTerminalOutcomeSnapshotV1::from_replay_checked_position_v1(
                        &published_combat_root,
                        &inputs,
                        &exact.successor,
                        forced_action_count,
                        0.0,
                        true,
                    );
                let public_outcome = self.exact_combat_replay_outcome_v1(&inputs)?;
                observed_terminal_candidates.push(RunPlayCombatTurnObservedTerminalOutcomeV1 {
                    candidate_id: candidate_id.clone(),
                    outcome: outcome.clone(),
                    public_outcome,
                });
                audits.push(RunPlayCombatTurnContinuationAuditV1 {
                    candidate_id: candidate_id.clone(),
                    scope: RunPlayCombatTurnContinuationAuditScopeV1::FixedTurnTerminal,
                    status: RunPlayCombatTurnContinuationAuditStatusV1::VerifiedWin,
                    exit: RunPlayCombatTurnContinuationAuditExitV1::WitnessFound,
                    bounded_search_only: true,
                    absence_means_unknown: true,
                    potion_contract,
                    search_spec: None,
                    work: RunPlayCombatTurnContinuationAuditWorkV1 {
                        selections: 0,
                        generation_work: 0,
                        engine_steps: 0,
                        exact_nodes: 0,
                        completed_turn_options: 0,
                    },
                    bounded_progress: None,
                    witness: Some(RunPlayCombatTurnContinuationWitnessV1 {
                        final_hp: exact.successor.combat.entities.player.current_hp,
                        forced_action_count,
                        continuation_action_count: 0,
                        total_action_count: forced_action_count,
                    }),
                    terminal_outcome_frontier: vec![outcome],
                });
                continue;
            }
            if exact.boundary != ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn {
                return Err(format!(
                    "combat-turn candidate {candidate_id} is not a winning or unresolved boundary"
                ));
            }
            let search_spec = options.search_spec_v1(remaining_max_potions_used);
            let root = CombatDecisionRoot::new(exact.successor.clone()).map_err(|error| {
                format!("combat-turn candidate {candidate_id} has an invalid successor: {error:?}")
            })?;
            let config =
                search_spec.planner_config(OracleCombatWitnessSatisfaction::BudgetOrExhaustion);
            let mut session = LocalTurnGraphWitnessSession::with_policy(
                root,
                config,
                retained_combat_tactical_policy_v1(),
            );
            let report = session.advance(search_spec.quantum(), &EngineCombatStepper);
            let internal_progress = session.progress_snapshot();
            let mut bounded_progress =
                RunPlayCombatTurnContinuationProgressV1::from(&internal_progress);
            if internal_progress.deepest_survival_state.is_some() {
                bounded_progress.deepest_survival_path = Some(compact_continuation_path_v1(
                    &exact.successor,
                    &internal_progress.deepest_survival_actions,
                    options.max_engine_steps_per_transition,
                )?);
            }
            if internal_progress.deepest_progress_state.is_some() {
                bounded_progress.deepest_progress_path = Some(compact_continuation_path_v1(
                    &exact.successor,
                    &internal_progress.deepest_progress_actions,
                    options.max_engine_steps_per_transition,
                )?);
            }
            let exit = match report.status {
                LocalTurnGraphWitnessStatus::WitnessFound => {
                    RunPlayCombatTurnContinuationAuditExitV1::WitnessFound
                }
                LocalTurnGraphWitnessStatus::Partial(
                    LocalTurnGraphWitnessInterruption::SelectionBudget,
                ) => RunPlayCombatTurnContinuationAuditExitV1::SelectionBudget,
                LocalTurnGraphWitnessStatus::Partial(
                    LocalTurnGraphWitnessInterruption::GenerationWorkBudget,
                ) => RunPlayCombatTurnContinuationAuditExitV1::GenerationWorkBudget,
                LocalTurnGraphWitnessStatus::Partial(
                    LocalTurnGraphWitnessInterruption::EngineStepBudget,
                ) => RunPlayCombatTurnContinuationAuditExitV1::EngineStepBudget,
                LocalTurnGraphWitnessStatus::Partial(
                    LocalTurnGraphWitnessInterruption::Deadline,
                ) => RunPlayCombatTurnContinuationAuditExitV1::Deadline,
                LocalTurnGraphWitnessStatus::FrontierExhausted => {
                    RunPlayCombatTurnContinuationAuditExitV1::FrontierExhausted
                }
                LocalTurnGraphWitnessStatus::MechanicsGap => {
                    return Err(format!(
                        "combat-turn candidate {candidate_id} continuation search hit a mechanics gap"
                    ));
                }
                LocalTurnGraphWitnessStatus::ReplayMismatch(error) => {
                    return Err(format!(
                        "combat-turn candidate {candidate_id} continuation witness replay mismatched: {error:?}"
                    ));
                }
            };
            let witness = report.witness.as_ref().map(|witness| {
                let continuation_action_count = witness.actions.len();
                RunPlayCombatTurnContinuationWitnessV1 {
                    final_hp: witness.final_position.combat.entities.player.current_hp,
                    forced_action_count,
                    continuation_action_count,
                    total_action_count: forced_action_count
                        .saturating_add(continuation_action_count),
                }
            });
            let selected_actions = report
                .witness
                .as_ref()
                .map(|selected| selected.actions.as_slice());
            let forced_inputs = exact
                .actions
                .iter()
                .map(|action| action.input.clone())
                .collect::<Vec<_>>();
            let mut terminal_outcome_frontier = Vec::new();
            for suffix in session.witness_frontier() {
                let mut inputs = forced_inputs.clone();
                inputs.extend(suffix.actions.iter().map(|action| action.input.clone()));
                let outcome =
                    LocalTurnGraphTerminalOutcomeSnapshotV1::from_replay_checked_position_v1(
                        &published_combat_root,
                        &inputs,
                        &suffix.final_position,
                        forced_action_count.saturating_add(suffix.actions.len()),
                        suffix.negative_log_policy,
                        selected_actions.is_some_and(|selected| selected == suffix.actions),
                    );
                let public_outcome = self.exact_combat_replay_outcome_v1(&inputs)?;
                observed_terminal_candidates.push(RunPlayCombatTurnObservedTerminalOutcomeV1 {
                    candidate_id: candidate_id.clone(),
                    outcome: outcome.clone(),
                    public_outcome,
                });
                terminal_outcome_frontier.push(outcome);
            }
            audits.push(RunPlayCombatTurnContinuationAuditV1 {
                candidate_id: candidate_id.clone(),
                scope: RunPlayCombatTurnContinuationAuditScopeV1::FixedTurnPlusSuffix,
                status: if witness.is_some() {
                    RunPlayCombatTurnContinuationAuditStatusV1::VerifiedWin
                } else {
                    RunPlayCombatTurnContinuationAuditStatusV1::UnknownInBoundedSearch
                },
                exit,
                bounded_search_only: true,
                absence_means_unknown: true,
                potion_contract,
                search_spec: Some(search_spec),
                work: RunPlayCombatTurnContinuationAuditWorkV1 {
                    selections: report.counters.selections,
                    generation_work: report.counters.generation_work,
                    engine_steps: report.counters.engine_steps,
                    exact_nodes: report.counters.exact_nodes,
                    completed_turn_options: report.counters.completed_turn_options,
                },
                bounded_progress: Some(bounded_progress),
                witness,
                terminal_outcome_frontier,
            });
        }
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err(
                "combat-turn continuation audit changed its exact session root".to_string(),
            );
        }
        let mut observed_terminal_frontier = Vec::new();
        for candidate in observed_terminal_candidates {
            retain_observed_combat_turn_terminal_outcome_v1(
                &mut observed_terminal_frontier,
                candidate,
            );
        }
        Ok(RunPlayCombatTurnContinuationAuditBatchV1 {
            audits,
            observed_terminal_frontier,
            observed_witnesses_only: true,
            absence_means_unknown: true,
        })
    }

    /// Commits one process-local complete-turn candidate at the exact head
    /// where it was generated. Exact inputs are converted back to public task
    /// choices and replay-prepared before the durable head is published.
    pub fn anchor_cached_combat_turn_candidate_id_v1(
        &mut self,
        root_head_revision: u64,
        attempt: &RunPlayCombatTurnAttemptV1,
        candidate_id: &str,
    ) -> Result<(), String> {
        if self.head.head_revision != root_head_revision {
            return Err(format!(
                "cached combat turn belongs to head {root_head_revision}, current head is {}",
                self.head.head_revision
            ));
        }
        if attempt.candidates.len() != attempt.exact_candidates.len() {
            return Err("cached combat turn public and exact candidates disagree".to_string());
        }
        let mut matches = attempt
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                (candidate.candidate_id == candidate_id).then_some(index)
            });
        let index = matches
            .next()
            .ok_or_else(|| format!("cached combat turn candidate id is absent: {candidate_id}"))?;
        if matches.next().is_some() {
            return Err(format!(
                "cached combat turn candidate id is ambiguous: {candidate_id}"
            ));
        }
        let inputs = attempt.exact_candidates[index]
            .actions
            .iter()
            .map(|action| action.input.clone())
            .collect::<Vec<_>>();
        let plan = self.exact.current_combat_turn_plan_for_run_play_v1(
            &inputs,
            TaskPrivilegedSupportProvenanceV1 {
                access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
                producer_name: "bounded_complete_turn_model_selection".to_string(),
                producer_revision: 1,
            },
        )?;
        let prepared = self.exact.prepare_line_resolving_v1(
            plan.steps.len(),
            &mut |step_index, decision| {
                let step = &plan.steps[step_index];
                if step.source_public_snapshot_id != decision.information.snapshot_id {
                    return Err(format!(
                        "combat turn step {} expected snapshot {}, observed {}",
                        step_index + 1,
                        step.source_public_snapshot_id,
                        decision.information.snapshot_id
                    ));
                }
                Ok(step.choice.clone())
            },
        )?;
        self.publish_prepared_line_v1(prepared, None, None)
    }

    /// Commits one candidate from a frontier produced by this process at the
    /// unchanged durable head. The candidate is still replay-prepared against
    /// the live exact root before publication; caller-provided action lines or
    /// ordinals are never trusted.
    pub fn anchor_cached_combat_research_candidate_id_v1(
        &mut self,
        root_head_revision: u64,
        attempt: &TaskResearchPlanAttemptV1,
        candidate_id: &str,
    ) -> Result<(), String> {
        if self.head.head_revision != root_head_revision {
            return Err(format!(
                "cached combat research belongs to head {root_head_revision}, current head is {}",
                self.head.head_revision
            ));
        }
        attempt.validate()?;
        let mut matches = Vec::new();
        for candidate in &attempt.candidates {
            if combat_research_candidate_id_v1(candidate)? == candidate_id {
                matches.push(candidate.clone());
            }
        }
        let candidate = matches.pop().ok_or_else(|| {
            format!("cached combat research candidate id is absent: {candidate_id}")
        })?;
        if !matches.is_empty() {
            return Err(format!(
                "cached combat research candidate id is ambiguous: {candidate_id}"
            ));
        }
        self.publish_combat_research_candidate_v1(candidate)
    }

    /// Captures the current exact combat root without advancing the session.
    /// The caller owns serialization and fresh-path publication.
    pub fn current_combat_case_v2(&mut self) -> Result<CombatCase, String> {
        self.restore_exact_at_published_head_v1()?;
        self.exact.current_combat_case_for_run_play_v1()
    }

    /// Clones the current private combat root for an in-process, read-only
    /// laboratory operation. The durable controller head is not moved.
    pub(crate) fn current_combat_position_for_lab_v1(
        &mut self,
    ) -> Result<sts_oracle_learning::sim::combat::CombatPosition, String> {
        self.restore_exact_at_published_head_v1()?;
        self.exact.current_combat_position_for_run_play_v1()
    }

    pub fn export_current_combat_case_v2(&mut self, output: &Path) -> Result<(), String> {
        let case = self.current_combat_case_v2()?;
        let payload = serde_json::to_vec_pretty(&case)
            .map_err(|error| format!("failed to encode run-play combat case: {error}"))?;
        write_new_synced(output, &payload, "run-play combat case")
    }

    /// Diagnoses one semantic complete-turn line against the production
    /// proposal width without publishing archive evidence or moving the head.
    pub fn proposal_audit_v1(
        &mut self,
        selectors: &[RunPlayActionSelectorV1],
        config: ExactCompleteTurnProposalConfigV1,
    ) -> Result<RunPlayProposalAuditV1, String> {
        validate_comparison_lines_v1(&[selectors.to_vec()])?;
        self.restore_exact_at_published_head_v1()?;
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "run-play proposal audit requires a public root".to_string())?
            .to_string();
        let probe = self
            .exact
            .probe_line_resolving_v1(selectors.len(), &mut |index, decision| {
                resolve_action_selector_v1(decision, &selectors[index])
            })?;
        let choices = probe
            .steps
            .iter()
            .map(|step| step.choice.clone())
            .collect::<Vec<_>>();
        let root = self.exact.current_combat_position_for_run_play_v1()?;
        let audit = audit_complete_turn_target_v1(&root, &choices, config)?;
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err("run-play proposal audit changed its exact session root".to_string());
        }
        Ok(audit)
    }

    pub fn choose(
        &mut self,
        choice: TaskPublicChoiceV1,
        budget_override: Option<TaskBudgetV1>,
        manual: bool,
    ) -> Result<(), String> {
        self.choose_with_advice_v1(choice, budget_override, manual, true)
    }

    pub(crate) fn decision_reading_v1(
        &mut self,
        details: &[crate::decision::DecisionDetailV1],
        known: Option<&crate::decision::DecisionKnownContextV1>,
        locale: Locale,
    ) -> Result<crate::decision::RunPlayDecisionReadingV1, String> {
        let published = self.paths.load_head()?;
        if published.session_id != self.head.session_id
            || published.head_revision != self.head.head_revision
        {
            self.reopen_preserving_hot_recovery_v1()?;
        }
        if matches!(self.head.position, RunPlayPositionV1::Decision { .. }) {
            self.restore_exact_at_published_head_v1()?;
        }
        crate::decision::read_decision(&self.head, details, known, locale)
    }

    /// Execute exactly one bound choice, with no implicit strategy
    /// advice or combat continuation. The ordinary exact owner records the step.
    pub(crate) fn choose_bound_v1(
        &mut self,
        command: &crate::decision::RunPlayBoundChoiceV1,
    ) -> Result<(), String> {
        self.validate_bound_source_v1(command)?;
        let decision = self
            .head
            .current_decision
            .as_ref()
            .ok_or("No current decision")?;
        if decision.site == sts_oracle_learning::agent::task::TaskDecisionSiteV1::Combat {
            let public = decision.public_decision_v1()?;
            let selected = command
                .choice
                .ordinal_path
                .first()
                .and_then(|i| public.candidates.get(*i));
            if !selected.is_some_and(crate::decision::combat::is_pending_choice) {
                return Err("Bound combat choice requires a pending selection".into());
            }
        }
        validate_inventory_choice_v1(decision, &command.choice)?;
        self.choose_with_advice_v1(command.choice.clone(), None, true, false)
    }

    fn validate_bound_source_v1(
        &mut self,
        command: &crate::decision::RunPlayBoundChoiceV1,
    ) -> Result<(), String> {
        let stale = || "Decision changed; inspect the current decision before choosing".to_owned();
        if crate::decision::decision_source(&self.head).map_err(|_| stale())? != command.source {
            return Err(stale());
        }
        // Detect another writer before doing any work in this resident world.
        let published = self.paths.load_head()?;
        if published.session_id != self.head.session_id
            || published.head_revision != self.head.head_revision
        {
            return Err(stale());
        }
        self.restore_exact_at_published_head_v1()?;
        if crate::decision::decision_source(&self.head)? != command.source {
            return Err(stale());
        }
        Ok(())
    }

    pub(crate) fn choose_reward_sequence_v1(
        &mut self,
        first: &crate::decision::RunPlayBoundChoiceV1,
        following: &[TaskPublicChoiceV1],
    ) -> Result<(), String> {
        self.validate_bound_source_v1(first)?;
        let root = self
            .head
            .current_decision
            .as_ref()
            .ok_or("No current decision")?
            .public_decision_v1()?;
        let mut choices = vec![first.choice.clone()];
        choices.extend_from_slice(following);
        let sequence = crate::reward_sequence::RewardSequenceV1::compile(&root, &choices)?;
        let prepared = self
            .exact
            .prepare_line_resolving_v1(sequence.len(), &mut |step, decision| {
                sequence.resolve(step, decision)
            })?;
        sequence.check_endpoint(
            &prepared
                .current_decision()
                .ok_or("Reward sequence ended without a decision")?
                .public_decision_v1()?,
        )?;
        self.publish_prepared_line_v1(prepared, None, None)
    }

    fn choose_with_advice_v1(
        &mut self,
        choice: TaskPublicChoiceV1,
        budget_override: Option<TaskBudgetV1>,
        manual: bool,
        collect_advice: bool,
    ) -> Result<(), String> {
        if budget_override.is_some() {
            return Err(
                "hot run-play session uses the budget fixed when it was opened".to_string(),
            );
        }
        self.restore_exact_at_published_head_v1()?;
        let from_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "hot run-play session is not at a public decision".to_string())?
            .to_string();
        let before_nodes = self.exact.archive().nodes.len();
        let before_attempts = self.exact.archive().attempts.len();
        let decision = self
            .head
            .current_decision
            .as_ref()
            .ok_or_else(|| "run-play head has no current decision".to_string())?;
        validate_inventory_choice_v1(decision, &choice)?;
        let site = decision.site;
        let strategic_owner_advice = if !collect_advice
            || site == sts_oracle_learning::agent::task::TaskDecisionSiteV1::Combat
        {
            None
        } else if manual {
            // Advice is audit evidence, not authority over an explicit
            // manual action. Some legal public boundaries intentionally
            // have no strategic owner yet (for example, a chest relic
            // versus Sapphire Key). Preserve the user's control there
            // instead of turning missing advice into an execution veto.
            self.advise_v1().ok()
        } else {
            Some(self.advise_v1()?)
        };
        let mut step = self.exact.choose(
            choice.clone(),
            if manual {
                ExactRunPlayAdvanceV1::NextPublicDecision
            } else {
                ExactRunPlayAdvanceV1::NextStrategicDecision
            },
        )?;
        if should_replay_choice_to_public_handoff_v1(
            manual,
            step.current_archive_node_id.as_deref(),
            &step.episode.outcome,
        ) {
            // The selected action was valid and was observed by the exact
            // episode, but automatic continuation later reached a public
            // surface its actor deliberately declined to resolve. Discard the
            // unpublished failed attempt by reopening the durable head, then
            // replay only the same validated action and yield at its immediate
            // public successor. This preserves the user's strategic choice
            // without turning an incomparable combat frontier into a failure.
            self.reopen_preserving_hot_recovery_v1()?;
            step = self
                .exact
                .choose(choice.clone(), ExactRunPlayAdvanceV1::NextPublicDecision)?;
        }
        let needs_reopen = step.current_archive_node_id.is_none();
        let mut head = self.head.clone();
        head.head_revision = head.head_revision.saturating_add(1);
        head.position = match (&step.current_archive_node_id, &step.episode.outcome) {
            (Some(node_id), TaskEpisodeOutcomeV1::Paused { .. }) => RunPlayPositionV1::Decision {
                archive_node_id: node_id.clone(),
            },
            (Some(node_id), outcome) => RunPlayPositionV1::Terminal {
                archive_node_id: node_id.clone(),
                outcome: outcome.clone(),
            },
            (None, _) => self.head.position.clone(),
        };
        head.current_decision = match &head.position {
            RunPlayPositionV1::Decision { archive_node_id }
                if step.current_archive_node_id.as_deref() == Some(archive_node_id) =>
            {
                Some(task_run_branch_decision_in_validated_session_v1(
                    self.exact.archive(),
                    self.exact.checkpoints(),
                    archive_node_id,
                )?)
            }
            RunPlayPositionV1::Decision { .. } => self.head.current_decision.clone(),
            RunPlayPositionV1::Terminal { .. } => None,
        };
        if matches!(head.position, RunPlayPositionV1::Decision { .. }) {
            // The archive boundary is durable evidence, but the still-hot exact
            // world is the authoritative projection for the decision we just
            // reached. Refresh before publication so immediate rendering and a
            // later exact reopen cannot disagree about transient public state.
            refresh_head_decision_from_exact_v1(&mut head, &self.exact)?;
        }
        head.current_path =
            run_play_path_v1(self.exact.archive(), head.position.archive_node_id())?;
        if self.head.scratch {
            head.scratch_parent_revision = Some(self.head.head_revision);
        }
        head.events.push(RunPlayEventV1::Choice {
            head_revision: head.head_revision,
            from_archive_node_id: from_node_id.clone(),
            to_archive_node_id: step.current_archive_node_id.clone(),
            site,
            choice,
            strategic_owner_advice,
            executed_new_branch: true,
            attempt_id: Some(step.attempt_id),
            outcome: Some(step.episode.outcome),
        });
        if self.head.scratch && head.current_decision.is_some() {
            let checkpoint = self.exact.current_run_decision_checkpoint_v1()?;
            let (fresh_head, state, exact) = scratch_root_from_checkpoint_v1(
                &self.head,
                checkpoint,
                self.exact.archive().task.objective,
                self.base_state_revision + 1,
            )?;
            self.paths
                .publish(self.head.head_revision, &fresh_head, Some(&state), None)?;
            self.base_state_revision = state.state_revision;
            self.head = fresh_head;
            self.exact = exact;
            self.hot_recovery = RunPlayHotRecoveryCacheV1::default();
            self.remember_current_hot_snapshot_v1()?;
            return Ok(());
        }
        if can_publish_hot_choice_as_append_only_delta_v1(step.checkpoints_added, &head.position) {
            let delta = RunPlayDeltaV1 {
                schema: RUN_PLAY_DELTA_SCHEMA_V1.to_string(),
                delta_revision: head.head_revision,
                base_state_revision: self.base_state_revision,
                expected_archive_nodes: before_nodes,
                expected_archive_attempts: before_attempts,
                appended_nodes: self.exact.archive().nodes[before_nodes..].to_vec(),
                appended_attempts: self.exact.archive().attempts[before_attempts..].to_vec(),
            };
            head.delta_revisions.push(head.head_revision);
            self.paths
                .publish(self.head.head_revision, &head, None, Some(&delta))?;
        } else {
            // A successful terminal promotes the complete existing archive
            // path from ReachableUnknown to ProvenVictory. That is an in-place
            // proof change, not an append-only delta, so every terminal is
            // consolidated into a full state generation before publication.
            let new_state_revision = self.base_state_revision.saturating_add(1);
            head.state_revision = new_state_revision;
            head.delta_revisions.clear();
            let state = RunPlayStateV1 {
                schema: RUN_PLAY_STATE_SCHEMA_V1.to_string(),
                state_revision: new_state_revision,
                archive: self.exact.archive().clone(),
                checkpoint_payload: self
                    .exact
                    .checkpoints()
                    .encode(DEFAULT_ARTIFACT_MAX_BYTES)?,
            };
            self.paths
                .publish(self.head.head_revision, &head, Some(&state), None)?;
            self.base_state_revision = new_state_revision;
        }
        self.head = head;
        if needs_reopen {
            self.reopen_preserving_hot_recovery_v1()?;
        } else {
            self.remember_current_hot_snapshot_v1()?;
        }
        Ok(())
    }

    /// Compares bounded public action lines on private exact forks. Unlike
    /// `choose`, this never appends archive evidence or publishes a controller
    /// revision; it is an inspection operation over the current hot root.
    pub fn compare_action_lines_v1(
        &mut self,
        lines: &[Vec<RunPlayActionSelectorV1>],
    ) -> Result<RunPlayComparisonV1, String> {
        validate_comparison_lines_v1(lines)?;
        self.restore_exact_at_published_head_v1()?;
        let root_archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "run-play compare requires a public decision".to_string())?
            .to_string();
        let mut results = Vec::with_capacity(lines.len());
        for line in lines {
            results.push(self.exact.probe_line_resolving_v1(
                line.len(),
                &mut |index, decision| {
                    let choice = resolve_action_selector_v1(decision, &line[index])?;
                    reject_unrevealed_question_mark_probe_v1(decision, &choice)?;
                    Ok(choice)
                },
            )?);
        }
        if self.exact.current_archive_node_id() != Some(root_archive_node_id.as_str()) {
            return Err("run-play compare changed its exact session root".to_string());
        }
        Ok(RunPlayComparisonV1 {
            root_archive_node_id,
            lines: results,
        })
    }

    /// Returns private partial boundaries for user inspection. Strict prefix
    /// proofs and action publication still require the whole supplied line.
    pub(crate) fn inspect_action_lines_v1(
        &mut self,
        lines: &[Vec<RunPlayActionSelectorV1>],
    ) -> Result<Vec<sts_oracle_learning::eval::run_control::ExactRunPlayInspectionV1>, String> {
        validate_comparison_lines_v1(lines)?;
        self.restore_exact_at_published_head_v1()?;
        let root = self
            .exact
            .current_archive_node_id()
            .ok_or("run-play inspection requires a public decision")?
            .to_string();
        let mut results = Vec::with_capacity(lines.len());
        for line in lines {
            results.push(self.exact.inspect_line_resolving_v1(
                line.len(),
                &mut |index, decision| {
                    let choice = resolve_action_selector_v1(decision, &line[index])?;
                    reject_unrevealed_question_mark_probe_v1(decision, &choice)?;
                    Ok(choice)
                },
            )?);
        }
        if self.exact.current_archive_node_id() != Some(root.as_str()) {
            return Err("run-play inspection changed its exact session root".into());
        }
        Ok(results)
    }

    /// Replays one exact terminal combat line on the retained historical run
    /// root and returns resources at the real next public boundary. This is a
    /// read-only diagnostic projection; the durable head is never moved.
    pub fn exact_combat_replay_outcome_v1(
        &self,
        inputs: &[sts_oracle_learning::state::core::ClientInput],
    ) -> Result<TaskResearchPlanOutcomeV1, String> {
        self.exact.current_combat_replay_outcome_for_run_play_v1(
            inputs,
            TaskPrivilegedSupportProvenanceV1 {
                access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
                producer_name: "run_play_exact_combat_replay_projection".to_string(),
                producer_revision: 1,
            },
        )
    }

    /// Reprojects one exact terminal combat line through the public run-play
    /// action surface without publishing it. The returned probe retains the
    /// full source decision for every public choice, so an artifact producer
    /// can encode stable semantic actions instead of serializing private card
    /// indices as its execution interface.
    pub(crate) fn exact_combat_replay_probe_v1(
        &self,
        inputs: &[sts_oracle_learning::state::core::ClientInput],
        provenance: TaskPrivilegedSupportProvenanceV1,
    ) -> Result<(ExactRunPlayProbeLineV1, TaskResearchPlanOutcomeV1), String> {
        let (plan, outcome) = self
            .exact
            .current_combat_replay_plan_and_outcome_for_run_play_v1(inputs, provenance)?;
        if plan.steps.is_empty() {
            return Err("exact combat replay projection produced an empty public plan".to_string());
        }
        let choices = plan
            .steps
            .iter()
            .map(|step| step.choice.clone())
            .collect::<Vec<_>>();
        let probe = self.exact.probe_line_v1(&choices)?;
        if probe.steps.len() != plan.steps.len()
            || probe
                .steps
                .iter()
                .zip(&plan.steps)
                .any(|(observed, expected)| {
                    observed.source_decision.information.snapshot_id
                        != expected.source_public_snapshot_id
                        || observed.choice != expected.choice
                })
        {
            return Err(
                "exact combat replay public plan disagreed with its independent probe".to_string(),
            );
        }
        Ok((probe, outcome))
    }

    /// Transactionally promotes one replay-verified diagnostic line after its
    /// public plan and full successor resources are reprojected at the current
    /// exact root and matched against the retained artifact.
    pub fn anchor_exact_combat_replay_v1(
        &mut self,
        inputs: &[sts_oracle_learning::state::core::ClientInput],
        expected_outcome: &TaskResearchPlanOutcomeV1,
    ) -> Result<(), String> {
        self.restore_exact_at_published_head_v1()?;
        let provenance = TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: "run_play_exact_combat_witness_anchor".to_string(),
            producer_revision: 1,
        };
        let (plan, outcome) = self
            .exact
            .current_combat_replay_plan_and_outcome_for_run_play_v1(inputs, provenance.clone())?;
        if &outcome != expected_outcome {
            return Err(format!(
                "exact combat witness outcome drifted: artifact={expected_outcome:?} replay={outcome:?}"
            ));
        }
        self.publish_combat_research_candidate_v1(
            sts_oracle_learning::agent::task::TaskResearchPlanCandidateV1 {
                plan,
                outcome,
                satisfies_requested_quality: true,
                supporting_provenances: vec![provenance],
            },
        )
    }

    /// Resolves one stable semantic action against the current public root and
    /// then executes the resulting exact choice. The resolved ordinal remains
    /// in the evidence archive, but it is not supplied by the caller.
    pub fn choose_action_v1(
        &mut self,
        selector: &RunPlayActionSelectorV1,
        manual: bool,
    ) -> Result<(), String> {
        self.restore_exact_at_published_head_v1()?;
        let decision = self
            .exact
            .current_public_decision()
            .ok_or_else(|| "run-play action requires a public decision".to_string())?;
        let choice = resolve_action_selector_v1(decision, selector)?;
        self.choose(choice, None, manual)
    }

    /// Executes a bounded exact public line on an isolated session and
    /// publishes its typed archive plus final replay-free checkpoint as one
    /// controller revision.  No intermediate choice becomes visible.
    pub fn anchor_action_line_v1(
        &mut self,
        selectors: &[RunPlayActionSelectorV1],
    ) -> Result<RunPlayActionLineProfileV1, String> {
        let total = Instant::now();
        validate_anchor_line_v1(selectors)?;
        let phase = Instant::now();
        self.restore_exact_at_published_head_v1()?;
        let exact_restore = phase.elapsed();
        let prepared = self
            .exact
            .prepare_line_resolving_v1(selectors.len(), &mut |index, decision| {
                resolve_action_selector_v1(decision, &selectors[index])
            })?;
        let prepared_line = prepared.profile();
        let phase = Instant::now();
        self.publish_prepared_line_v1(prepared, None, None)?;
        Ok(RunPlayActionLineProfileV1 {
            total: total.elapsed(),
            exact_restore,
            prepared_line,
            endpoint_validation: Duration::ZERO,
            publication: phase.elapsed(),
        })
    }

    /// Replays one bounded chunk from a retained semantic-prefix artifact.
    /// Unlike a shell-supplied action line, every selector is guarded by the
    /// source decision site and compact public run scalars before it executes.
    pub(crate) fn anchor_retained_semantic_prefix_chunk_v1(
        &mut self,
        steps: &[RunPlayRetainedSemanticPrefixStepV1],
    ) -> Result<(), String> {
        validate_anchor_line_v1(steps)?;
        self.restore_exact_at_published_head_v1()?;
        let prepared =
            self.exact
                .prepare_line_resolving_v1(steps.len(), &mut |index, decision| {
                    let step = &steps[index];
                    validate_retained_semantic_prefix_source_v1(step, decision)?;
                    resolve_action_selector_v1(decision, &step.selector)
                })?;
        self.publish_prepared_line_v1(prepared, None, None)
    }

    /// Bridges one newly explicit routine boundary while replaying a
    /// historical typed prefix. The bridge is deliberately limited to the
    /// corrected Java-compatible empty Reward -> Map transition: all public
    /// scalars must already match the next retained step and `Advance` must
    /// resolve to one uniquely routine action. The resulting Proceed is
    /// published as ordinary history instead of being hidden inside replay.
    pub(crate) fn normalize_retained_semantic_prefix_boundary_v1(
        &mut self,
        next_step: &RunPlayRetainedSemanticPrefixStepV1,
    ) -> Result<bool, String> {
        self.restore_exact_at_published_head_v1()?;
        let decision = self
            .exact
            .current_public_decision()
            .ok_or_else(|| {
                "run-play retained-prefix normalization requires a decision".to_string()
            })?
            .clone();
        if decision.site == next_step.expected_site {
            return Ok(false);
        }
        if decision.site != TaskDecisionSiteV1::Reward
            || next_step.expected_site != TaskDecisionSiteV1::Map
        {
            return Err(format!(
                "retained semantic prefix depth {} expected site {:?}, observed {:?}",
                next_step.source_depth, next_step.expected_site, decision.site
            ));
        }
        let observed = public_scalars_for_retained_prefix_v1(&decision);
        if observed != next_step.expected_before {
            return Err(format!(
                "retained semantic prefix depth {} cannot normalize Reward -> Map because public scalars differ: expected {:?}, observed {:?}",
                next_step.source_depth, next_step.expected_before, observed
            ));
        }
        let choice = resolve_action_selector_v1(&decision, &RunPlayActionSelectorV1::Advance)
            .map_err(|error| {
                format!(
                    "retained semantic prefix depth {} cannot normalize Reward -> Map through one routine action: {error}",
                    next_step.source_depth
                )
            })?;
        self.choose(choice, None, false)?;
        self.restore_exact_at_published_head_v1()?;
        let normalized = self.exact.current_public_decision().ok_or_else(|| {
            "run-play retained-prefix normalization did not reach a decision".to_string()
        })?;
        validate_retained_semantic_prefix_source_v1(next_step, normalized)?;
        Ok(true)
    }

    /// Replays one durable combat-research proof without rerunning its search.
    /// Every semantic action is resolved only at its recorded public snapshot,
    /// and the final public/terminal boundary must match before publication.
    pub fn anchor_combat_research_replay_v1(
        &mut self,
        source_public_snapshot_ids: &[String],
        selectors: &[RunPlayActionSelectorV1],
        expected_public_endpoint_snapshot_id: Option<&str>,
        expected_terminal_outcome: Option<&TaskEpisodeOutcomeV1>,
        evidence: RunPlayCombatResearchCandidateV1,
    ) -> Result<RunPlayActionLineProfileV1, String> {
        let total = Instant::now();
        if selectors.is_empty() || selectors.len() != source_public_snapshot_ids.len() {
            return Err(
                "combat research replay requires non-empty, length-matched steps".to_string(),
            );
        }
        if expected_public_endpoint_snapshot_id.is_some() == expected_terminal_outcome.is_some() {
            return Err(
                "combat research replay requires exactly one expected endpoint".to_string(),
            );
        }
        let phase = Instant::now();
        self.restore_exact_at_published_head_v1()?;
        let exact_restore = phase.elapsed();
        let prepared =
            self.exact
                .prepare_line_resolving_v1(selectors.len(), &mut |index, decision| {
                    let expected_snapshot = &source_public_snapshot_ids[index];
                    if decision.information.snapshot_id != *expected_snapshot {
                        return Err(format!(
                            "combat research replay step {} expected snapshot {}, observed {}",
                            index + 1,
                            expected_snapshot,
                            decision.information.snapshot_id
                        ));
                    }
                    resolve_action_selector_v1(decision, &selectors[index])
                })?;
        let prepared_line = prepared.profile();
        let phase = Instant::now();
        match (
            prepared.current_decision(),
            prepared.terminal_outcome(),
            expected_public_endpoint_snapshot_id,
            expected_terminal_outcome,
        ) {
            (Some(decision), None, Some(expected), None) => {
                let observed = decision.public_decision_v1()?.information.snapshot_id;
                if observed != expected {
                    return Err(format!(
                        "combat research replay expected public endpoint {expected}, observed {observed}"
                    ));
                }
            }
            (None, Some(observed), None, Some(expected)) if observed == expected => {}
            (None, Some(observed), None, Some(expected)) => {
                return Err(format!(
                    "combat research replay terminal endpoint mismatch: expected {expected:?}, observed {observed:?}"
                ));
            }
            _ => {
                return Err(
                    "combat research replay endpoint kind disagrees with its artifact".to_string(),
                );
            }
        }
        let endpoint_validation = phase.elapsed();
        let phase = Instant::now();
        self.publish_prepared_line_v1(prepared, Some(evidence), None)?;
        let publication = phase.elapsed();
        Ok(RunPlayActionLineProfileV1 {
            total: total.elapsed(),
            exact_restore,
            prepared_line,
            endpoint_validation,
            publication,
        })
    }

    fn publish_combat_research_candidate_v1(
        &mut self,
        candidate: sts_oracle_learning::agent::task::TaskResearchPlanCandidateV1,
    ) -> Result<(), String> {
        let plan = &candidate.plan;
        let prepared =
            self.exact
                .prepare_line_resolving_v1(plan.steps.len(), &mut |index, decision| {
                    let step = &plan.steps[index];
                    if step.source_public_snapshot_id != decision.information.snapshot_id {
                        return Err(format!(
                            "research plan step {} expected snapshot {}, observed {}",
                            index + 1,
                            step.source_public_snapshot_id,
                            decision.information.snapshot_id
                        ));
                    }
                    Ok(step.choice.clone())
                })?;
        let evidence = RunPlayCombatResearchCandidateV1 {
            supporting_provenances: candidate.supporting_provenances.clone(),
            outcome: candidate.outcome.clone(),
            satisfies_requested_quality: candidate.satisfies_requested_quality,
        };
        self.publish_prepared_line_v1(prepared, Some(evidence), None)
    }

    fn publish_prepared_line_v1(
        &mut self,
        prepared: ExactRunPlayPreparedLineV1,
        combat_research_candidate: Option<RunPlayCombatResearchCandidateV1>,
        combat_milestone_candidate: Option<RunPlayCombatMilestoneCandidateV1>,
    ) -> Result<(), String> {
        if self.head.scratch && prepared.current_decision().is_some() {
            let checkpoint = prepared.current_run_decision_checkpoint_for_run_play_v1()?;
            let (head, state, exact) = scratch_root_from_checkpoint_v1(
                &self.head,
                checkpoint,
                prepared.archive().task.objective,
                self.base_state_revision + 1,
            )?;
            self.paths
                .publish(self.head.head_revision, &head, Some(&state), None)?;
            self.base_state_revision = state.state_revision;
            self.head = head;
            self.exact = exact;
            self.hot_recovery = RunPlayHotRecoveryCacheV1::default();
            return self.remember_current_hot_snapshot_v1();
        }
        let new_state_revision = self.base_state_revision.saturating_add(1);
        let mut head = self.head.clone();
        head.head_revision = head.head_revision.saturating_add(1);
        head.state_revision = new_state_revision;
        head.delta_revisions.clear();
        let (position, current_decision) = prepared_line_endpoint_v1(
            prepared.to_archive_node_id(),
            prepared.current_decision(),
            prepared.terminal_outcome(),
        )?;
        head.position = position;
        head.current_decision = current_decision;
        head.current_path = run_play_path_v1(prepared.archive(), prepared.to_archive_node_id())?;
        if self.head.scratch {
            head.scratch_parent_revision = Some(self.head.head_revision);
        }
        head.events.push(RunPlayEventV1::AnchorLine {
            head_revision: head.head_revision,
            from_archive_node_id: prepared.source_archive_node_id().to_string(),
            to_archive_node_id: prepared.to_archive_node_id().to_string(),
            choices: prepared.choices().to_vec(),
            state_revision: new_state_revision,
            before_nodes: prepared.archive_nodes_before(),
            after_nodes: prepared.archive().nodes.len(),
            before_attempts: prepared.archive_attempts_before(),
            after_attempts: prepared.archive().attempts.len(),
            before_checkpoints: prepared.checkpoints_before(),
            after_checkpoints: prepared.checkpoints().checkpoint_count(),
            combat_research_candidate,
            combat_milestone_candidate,
        });
        let state = RunPlayStateV1 {
            schema: RUN_PLAY_STATE_SCHEMA_V1.to_string(),
            state_revision: new_state_revision,
            archive: prepared.archive().clone(),
            checkpoint_payload: prepared.checkpoints().encode(DEFAULT_ARTIFACT_MAX_BYTES)?,
        };
        self.paths
            .publish(self.head.head_revision, &head, Some(&state), None)?;
        self.exact.install_prepared_line_v1(prepared);
        self.base_state_revision = new_state_revision;
        self.head = head;
        self.hot_recovery = RunPlayHotRecoveryCacheV1::default();
        self.remember_current_hot_snapshot_v1()
    }

    /// Publishes a durable exact recovery point at the current selected
    /// public decision without executing another game action.  Checkpoint
    /// preparation happens on a private fork; the hot session installs it
    /// only after the new state and commit head are durable.
    pub fn checkpoint(&mut self) -> Result<(), String> {
        if self.head.scratch {
            // Each committed scratch decision is already an exact checkpoint.
            return Ok(());
        }
        self.restore_exact_at_published_head_v1()?;
        let archive_node_id = self
            .exact
            .current_archive_node_id()
            .ok_or_else(|| "run-play checkpoint requires a public decision".to_string())?
            .to_string();
        let prepared = self.exact.prepare_current_checkpoint_v1()?;
        let new_state_revision = self.base_state_revision.saturating_add(1);
        let mut head = self.head.clone();
        head.head_revision = head.head_revision.saturating_add(1);
        head.state_revision = new_state_revision;
        head.delta_revisions.clear();
        head.current_decision = Some(prepared.current_decision().clone());
        head.events.push(RunPlayEventV1::Checkpoint {
            head_revision: head.head_revision,
            archive_node_id,
            state_revision: new_state_revision,
            checkpoint_count: prepared.checkpoints().checkpoint_count(),
        });
        let state = RunPlayStateV1 {
            schema: RUN_PLAY_STATE_SCHEMA_V1.to_string(),
            state_revision: new_state_revision,
            archive: self.exact.archive().clone(),
            checkpoint_payload: prepared.checkpoints().encode(DEFAULT_ARTIFACT_MAX_BYTES)?,
        };
        self.paths
            .publish(self.head.head_revision, &head, Some(&state), None)?;
        self.exact.install_prepared_checkpoint_v1(prepared);
        self.base_state_revision = new_state_revision;
        self.head = head;
        self.hot_recovery = RunPlayHotRecoveryCacheV1::default();
        self.remember_current_hot_snapshot_v1()
    }

    pub fn back(&mut self, steps: usize) -> Result<(), String> {
        if self.head.scratch {
            back_scratch_v1(&self.paths, steps)?;
            self.hot_recovery = RunPlayHotRecoveryCacheV1::default();
            return self.reopen_preserving_hot_recovery_v1();
        }
        let from = self.head.position.archive_node_id().to_string();
        let cursor = resolve_back_node_v1(self.exact.archive(), &from, steps)?;
        let mut head = self.head.clone();
        head.head_revision = head.head_revision.saturating_add(1);
        head.position = RunPlayPositionV1::Decision {
            archive_node_id: cursor.clone(),
        };
        head.current_decision = Some(task_run_branch_decision_in_validated_session_v1(
            self.exact.archive(),
            self.exact.checkpoints(),
            &cursor,
        )?);
        head.current_path = run_play_path_v1(self.exact.archive(), &cursor)?;
        head.events.push(RunPlayEventV1::Back {
            head_revision: head.head_revision,
            from_archive_node_id: from,
            to_archive_node_id: cursor,
            steps,
        });
        self.paths
            .publish(self.head.head_revision, &head, None, None)?;
        // Navigation is a public archive operation.  Keep the private exact
        // world where it is until the next choice actually needs execution;
        // repeated inspection/backtracking must not pay durable replay cost.
        self.head = head;
        Ok(())
    }

    pub fn jump(&mut self, selector: &str) -> Result<(), String> {
        let cursor = resolve_jump_node_from_parts_v1(self.exact.archive(), &self.head, selector)?;
        let from = self.head.position.archive_node_id().to_string();
        // A public archive node is not necessarily recoverable from the
        // currently retained exact checkpoints. Validate and materialize the
        // target before publishing the navigation event so a successful jump
        // can never leave the next command to discover a replay-scope error.
        let (target_exact, _) = ExactRunPlaySessionV1::open_in_validated_session_profiled_v1(
            self.exact.archive().clone(),
            self.exact.checkpoints().clone(),
            &cursor,
            self.head.default_budget,
            self.head.combat_plan_options.clone(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )?;
        let target_snapshot = target_exact.hot_snapshot_v1()?;
        let mut head = self.head.clone();
        head.head_revision = head.head_revision.saturating_add(1);
        head.position = RunPlayPositionV1::Decision {
            archive_node_id: cursor.clone(),
        };
        head.current_decision = Some(task_run_branch_decision_in_validated_session_v1(
            target_exact.archive(),
            target_exact.checkpoints(),
            &cursor,
        )?);
        head.current_path = run_play_path_v1(target_exact.archive(), &cursor)?;
        refresh_head_decision_from_exact_v1(&mut head, &target_exact)?;
        head.events.push(RunPlayEventV1::Jump {
            head_revision: head.head_revision,
            from_archive_node_id: from,
            to_archive_node_id: cursor.clone(),
            selector: selector.to_string(),
        });
        self.paths
            .publish(self.head.head_revision, &head, None, None)?;
        self.exact = target_exact;
        self.hot_recovery.insert(cursor, target_snapshot);
        self.head = head;
        Ok(())
    }

    fn remember_current_hot_snapshot_v1(&mut self) -> Result<(), String> {
        // A committed game terminal has an archive node but no next public
        // decision. It is already durably recoverable through the controller
        // event and must not be treated as a cacheable interactive root.
        if self.exact.current_public_decision().is_none() {
            return Ok(());
        }
        let Some(archive_node_id) = self.exact.current_archive_node_id().map(str::to_string) else {
            return Ok(());
        };
        if self.hot_recovery.contains(&archive_node_id) {
            return Ok(());
        }
        let snapshot = self.exact.hot_snapshot_v1()?;
        self.hot_recovery.insert(archive_node_id, snapshot);
        Ok(())
    }

    fn restore_exact_at_published_head_v1(&mut self) -> Result<(), String> {
        let RunPlayPositionV1::Decision {
            archive_node_id: target_archive_node_id,
        } = &self.head.position
        else {
            return Err("run-play is at a terminal; use back before choosing again".to_string());
        };
        if self.exact.current_archive_node_id() == Some(target_archive_node_id.as_str()) {
            return refresh_head_decision_from_exact_v1(&mut self.head, &self.exact);
        }
        let Some(snapshot) = self.hot_recovery.get(target_archive_node_id) else {
            return self.reopen_preserving_hot_recovery_v1();
        };
        // The cache is never authoritative. Any stale or incompatible fork
        // falls back to the same durable restore used after process restart.
        if self.exact.restore_hot_snapshot_v1(snapshot).is_err() {
            return self.reopen_preserving_hot_recovery_v1();
        }
        if refresh_head_decision_from_exact_v1(&mut self.head, &self.exact).is_err() {
            return self.reopen_preserving_hot_recovery_v1();
        }
        Ok(())
    }

    fn reopen_preserving_hot_recovery_v1(&mut self) -> Result<(), String> {
        let retained = std::mem::take(&mut self.hot_recovery);
        let mut reopened = Self::open(self.paths.clone())?;
        let current = std::mem::take(&mut reopened.hot_recovery);
        reopened.hot_recovery = retained;
        reopened.hot_recovery.absorb(current);
        *self = reopened;
        Ok(())
    }
}

/// Restores one retained decision and projects the production strategic owner
/// without returning a controller that could publish choices from that
/// temporary position.
pub fn inspect_advice_at_v1(
    paths: RunPlayPathsV1,
    selector: &str,
    include_detail: bool,
) -> Result<RunPlayAdviceInspectionV1, String> {
    let mut controller = RunPlayControllerV1::open_at(paths, selector)?;
    let advice = controller.advise_v1()?;
    let decision = controller
        .head()
        .current_decision
        .clone()
        .ok_or_else(|| "run-play advice inspection requires a decision head".to_string())?;
    let detail = include_detail
        .then(|| controller.advice_detail_v1())
        .transpose()?;
    Ok(RunPlayAdviceInspectionV1 {
        advice,
        decision,
        detail,
    })
}

fn should_replay_choice_to_public_handoff_v1(
    manual: bool,
    current_archive_node_id: Option<&str>,
    outcome: &TaskEpisodeOutcomeV1,
) -> bool {
    !manual
        && current_archive_node_id.is_none()
        && matches!(
            outcome,
            TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::UnsupportedDecisionSurface,
                game_outcome: None,
            }
        )
}

fn validate_comparison_lines_v1<T>(lines: &[Vec<T>]) -> Result<(), String> {
    if lines.is_empty() {
        return Err("run-play compare requires at least one line".to_string());
    }
    if lines.len() > MAX_COMPARISON_LINES {
        return Err(format!(
            "run-play compare accepts at most {MAX_COMPARISON_LINES} lines"
        ));
    }
    if let Some((index, _line)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| line.is_empty() || line.len() > MAX_COMPARISON_STEPS_PER_LINE)
    {
        return Err(format!(
            "run-play compare line {} must contain 1..={MAX_COMPARISON_STEPS_PER_LINE} public actions",
            index + 1
        ));
    }
    Ok(())
}

fn validate_anchor_line_v1<T>(choices: &[T]) -> Result<(), String> {
    if choices.is_empty() || choices.len() > MAX_ANCHOR_LINE_STEPS {
        return Err(format!(
            "run-play anchor line must contain 1..={MAX_ANCHOR_LINE_STEPS} public actions"
        ));
    }
    Ok(())
}

fn validate_retained_semantic_prefix_source_v1(
    step: &RunPlayRetainedSemanticPrefixStepV1,
    decision: &TaskPublicDecisionV1,
) -> Result<(), String> {
    if decision.site != step.expected_site {
        return Err(format!(
            "retained semantic prefix depth {} expected site {:?}, observed {:?}",
            step.source_depth, step.expected_site, decision.site
        ));
    }
    let observed = public_scalars_for_retained_prefix_v1(decision);
    if observed != step.expected_before {
        return Err(format!(
            "retained semantic prefix depth {} public before-state mismatch: expected {:?}, observed {:?}",
            step.source_depth, step.expected_before, observed
        ));
    }
    Ok(())
}

fn public_scalars_for_retained_prefix_v1(
    decision: &TaskPublicDecisionV1,
) -> RunPlayPublicScalarsV1 {
    match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => RunPlayPublicScalarsV1 {
            act: Some(observation.run.act),
            floor: Some(observation.run.floor),
            current_hp: observation.run.current_hp,
            max_hp: observation.run.max_hp,
            gold: observation.run.gold,
        },
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => {
            let (act, floor) = match run_context {
                PublicCombatRunContextV1::Available { act, floor, .. } => {
                    (Some(*act), Some(*floor))
                }
                PublicCombatRunContextV1::Unavailable { .. } => (None, None),
            };
            RunPlayPublicScalarsV1 {
                act,
                floor,
                current_hp: observation.player.hp,
                max_hp: observation.player.max_hp,
                gold: observation.player.gold,
            }
        }
    }
}

fn refresh_head_decision_from_exact_v1(
    head: &mut RunPlayHeadV1,
    exact: &ExactRunPlaySessionV1,
) -> Result<(), String> {
    if let (Some(cached), Some(restored)) = (
        head.current_decision.as_mut(),
        exact.current_public_decision(),
    ) {
        refresh_decision_inventory_from_public_v1(cached, restored)?;
    }
    Ok(())
}

fn refresh_decision_inventory_from_public_v1(
    cached: &mut sts_oracle_learning::eval::run_control::TaskRunBranchDecisionInventoryV1,
    restored: &TaskPublicDecisionV1,
) -> Result<(), String> {
    let cached_public = cached.public_decision_v1()?;
    if !cached_public.replay_compatible_v2(restored) {
        return Err(
            "run-play cached decision is not replay-compatible with exact restore".to_string(),
        );
    }
    cached.site = restored.site;
    cached.observation = restored.observation.clone();
    cached.public_snapshot_id = restored.information.snapshot_id.clone();
    for (cached_candidate, restored_candidate) in
        cached.candidates.iter_mut().zip(&restored.candidates)
    {
        cached_candidate.candidate = restored_candidate.clone();
    }
    Ok(())
}

pub fn choose_v1(
    paths: &RunPlayPathsV1,
    choice: TaskPublicChoiceV1,
    budget_override: Option<TaskBudgetV1>,
    manual: bool,
) -> Result<RunPlayWorkspaceV1, String> {
    if paths.load_head()?.scratch {
        let mut controller = RunPlayControllerV1::open(paths.clone())?;
        controller.choose(choice, budget_override, manual)?;
        return paths.load();
    }
    let mut loaded = paths.load()?;
    let mut checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
        &loaded.state.checkpoint_payload,
        &loaded.state.archive,
        DEFAULT_ARTIFACT_MAX_BYTES,
    )?;
    choose_loaded_v1(
        paths,
        &mut loaded,
        &mut checkpoints,
        choice,
        budget_override,
        manual,
    )?;
    Ok(loaded)
}

fn choose_loaded_v1(
    paths: &RunPlayPathsV1,
    loaded: &mut RunPlayWorkspaceV1,
    checkpoints: &mut TaskRunBranchCheckpointArtifactV1,
    choice: TaskPublicChoiceV1,
    budget_override: Option<TaskBudgetV1>,
    manual: bool,
) -> Result<(), String> {
    let RunPlayPositionV1::Decision {
        archive_node_id: current_node_id,
    } = &loaded.head.position
    else {
        return Err("run-play is at a terminal; use back before choosing again".to_string());
    };
    let current = task_run_branch_decision_v1(&loaded.state.archive, checkpoints, current_node_id)?;
    validate_inventory_choice_v1(&current, &choice)?;

    if let Some(position) =
        observed_direct_successor_v1(&loaded.state.archive, current_node_id, &choice)?
    {
        let mut head = loaded.head.clone();
        head.head_revision = head.head_revision.saturating_add(1);
        head.position = position;
        head.current_decision = match &head.position {
            RunPlayPositionV1::Decision { archive_node_id } => Some(task_run_branch_decision_v1(
                &loaded.state.archive,
                checkpoints,
                archive_node_id,
            )?),
            RunPlayPositionV1::Terminal { .. } => None,
        };
        head.current_path =
            run_play_path_v1(&loaded.state.archive, head.position.archive_node_id())?;
        head.events.push(RunPlayEventV1::Choice {
            head_revision: head.head_revision,
            from_archive_node_id: current_node_id.clone(),
            to_archive_node_id: Some(head.position.archive_node_id().to_string()),
            site: current.site,
            choice,
            strategic_owner_advice: None,
            executed_new_branch: false,
            attempt_id: None,
            outcome: terminal_outcome(&head.position).cloned(),
        });
        paths.publish(loaded.head.head_revision, &head, None, None)?;
        loaded.head = head;
        return Ok(());
    }

    let budget = budget_override.unwrap_or(loaded.head.default_budget);
    budget.validate()?;
    let result = if manual {
        resume_run_until_next_public_decision_v1(
            &loaded.state.archive,
            checkpoints,
            current_node_id,
            choice.clone(),
            budget,
            loaded.head.combat_plan_options.clone(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )?
    } else {
        resume_run_until_next_strategic_decision_v1(
            &loaded.state.archive,
            checkpoints,
            current_node_id,
            choice.clone(),
            budget,
            loaded.head.combat_plan_options.clone(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )?
    };
    let position = match &result.episode.outcome {
        TaskEpisodeOutcomeV1::Paused { .. } => RunPlayPositionV1::Decision {
            archive_node_id: task_run_paused_decision_node_id_v1(
                &result.archive,
                &result.attempt_id,
                &result.episode,
            )?,
        },
        TaskEpisodeOutcomeV1::Success { .. }
        | TaskEpisodeOutcomeV1::Failure {
            game_outcome: Some(_),
            ..
        } => {
            let attempt = result
                .archive
                .attempts
                .iter()
                .find(|attempt| attempt.attempt_id == result.attempt_id)
                .expect("continuation recorded its attempt");
            RunPlayPositionV1::Terminal {
                archive_node_id: attempt
                    .node_path
                    .last()
                    .expect("recorded attempt has a root")
                    .clone(),
                outcome: result.episode.outcome.clone(),
            }
        }
        TaskEpisodeOutcomeV1::Failure { .. } => loaded.head.position.clone(),
    };
    let mut head = loaded.head.clone();
    head.head_revision = head.head_revision.saturating_add(1);
    head.position = position;
    head.current_decision = match &head.position {
        RunPlayPositionV1::Decision { archive_node_id } => Some(task_run_branch_decision_v1(
            &result.archive,
            &result.checkpoints,
            archive_node_id,
        )?),
        RunPlayPositionV1::Terminal { .. } => None,
    };
    if matches!(result.episode.outcome, TaskEpisodeOutcomeV1::Paused { .. }) {
        if let (Some(cached), TaskActorBoundaryV1::Decision { decision: restored }) = (
            head.current_decision.as_mut(),
            &result.episode.final_view.boundary,
        ) {
            refresh_decision_inventory_from_public_v1(cached, restored)?;
        }
    }
    head.current_path = run_play_path_v1(&result.archive, head.position.archive_node_id())?;
    head.events.push(RunPlayEventV1::Choice {
        head_revision: head.head_revision,
        from_archive_node_id: current_node_id.clone(),
        to_archive_node_id: (head.position.archive_node_id() != current_node_id)
            .then(|| head.position.archive_node_id().to_string()),
        site: current.site,
        choice,
        strategic_owner_advice: None,
        executed_new_branch: true,
        attempt_id: Some(result.attempt_id),
        outcome: Some(result.episode.outcome),
    });
    if let Some(delta) = run_play_delta_v1(
        loaded,
        &result.archive,
        checkpoints.checkpoint_count(),
        result.checkpoints.checkpoint_count(),
        head.head_revision,
    ) {
        head.delta_revisions.push(head.head_revision);
        paths.publish(loaded.head.head_revision, &head, None, Some(&delta))?;
        loaded.state.archive = result.archive;
    } else {
        let new_state_revision = loaded.state.state_revision.saturating_add(1);
        head.state_revision = new_state_revision;
        head.delta_revisions.clear();
        let state = RunPlayStateV1 {
            schema: RUN_PLAY_STATE_SCHEMA_V1.to_string(),
            state_revision: new_state_revision,
            archive: result.archive,
            checkpoint_payload: result.checkpoints.encode(DEFAULT_ARTIFACT_MAX_BYTES)?,
        };
        paths.publish(loaded.head.head_revision, &head, Some(&state), None)?;
        loaded.state = state;
    }
    loaded.head = head;
    *checkpoints = result.checkpoints;
    Ok(())
}

pub fn back_v1(paths: &RunPlayPathsV1, steps: usize) -> Result<RunPlayWorkspaceV1, String> {
    if paths.load_head()?.scratch {
        return back_scratch_v1(paths, steps);
    }
    let mut loaded = paths.load()?;
    let checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
        &loaded.state.checkpoint_payload,
        &loaded.state.archive,
        DEFAULT_ARTIFACT_MAX_BYTES,
    )?;
    let from = loaded.head.position.archive_node_id().to_string();
    let cursor = resolve_back_node_v1(&loaded.state.archive, &from, steps)?;
    let mut head = loaded.head.clone();
    head.head_revision = head.head_revision.saturating_add(1);
    head.position = RunPlayPositionV1::Decision {
        archive_node_id: cursor.clone(),
    };
    head.current_decision = Some(task_run_branch_decision_v1(
        &loaded.state.archive,
        &checkpoints,
        &cursor,
    )?);
    head.current_path = run_play_path_v1(&loaded.state.archive, &cursor)?;
    head.events.push(RunPlayEventV1::Back {
        head_revision: head.head_revision,
        from_archive_node_id: from,
        to_archive_node_id: cursor,
        steps,
    });
    paths.publish(loaded.head.head_revision, &head, None, None)?;
    loaded.head = head;
    Ok(loaded)
}

fn back_scratch_v1(paths: &RunPlayPathsV1, steps: usize) -> Result<RunPlayWorkspaceV1, String> {
    let previous = paths.load_recent_scratch_decision_v1(steps)?;
    publish_scratch_rewind_v1(paths, previous)
}

pub fn back_scratch_to_floor_v1(
    paths: &RunPlayPathsV1,
    floor: i32,
) -> Result<RunPlayWorkspaceV1, String> {
    let previous = paths.load_scratch_floor_end_v1(floor)?;
    publish_scratch_rewind_v1(paths, previous)
}

pub fn back_scratch_to_mark_v1(
    paths: &RunPlayPathsV1,
    name: &str,
) -> Result<RunPlayWorkspaceV1, String> {
    let previous = paths.load_scratch_mark_v1(name)?;
    publish_scratch_rewind_v1(paths, previous)
}

pub fn save_scratch_mark_v1(paths: &RunPlayPathsV1, name: &str) -> Result<(), String> {
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err("mark name must contain 1..=64 non-control characters".into());
    }
    let _ = RunPlayControllerV1::open(paths.clone())?;
    let current = paths.load()?;
    if !current.head.scratch || current.head.current_decision.is_none() {
        return Err("mark requires a playable scratch decision".into());
    }
    let mut head = current.head;
    let saved_revision = head.head_revision;
    head.head_revision += 1;
    head.scratch_marks.retain(|mark| mark.name != name);
    head.scratch_marks.push(crate::model::RunPlayScratchMarkV1 {
        name: name.to_string(),
        head_revision: saved_revision,
    });
    paths.publish(saved_revision, &head, None, None)
}

pub fn drop_scratch_mark_v1(paths: &RunPlayPathsV1, name: &str) -> Result<(), String> {
    let current = paths.load()?;
    if !current.head.scratch {
        return Err("marks require a scratch session".into());
    }
    let original_count = current.head.scratch_marks.len();
    let mut head = current.head;
    let revision = head.head_revision;
    head.scratch_marks.retain(|mark| mark.name != name);
    if head.scratch_marks.len() == original_count {
        return Err(format!("no retained mark {name:?}"));
    }
    head.head_revision += 1;
    paths.publish(revision, &head, None, None)
}

fn publish_scratch_rewind_v1(
    paths: &RunPlayPathsV1,
    previous: RunPlayWorkspaceV1,
) -> Result<RunPlayWorkspaceV1, String> {
    let current = paths.load()?;
    publish_scratch_rewind_from_v1(paths, current, previous, false)
}

pub(crate) fn publish_scratch_rewind_from_v1(
    paths: &RunPlayPathsV1,
    current: RunPlayWorkspaceV1,
    previous: RunPlayWorkspaceV1,
    remember_endpoint: bool,
) -> Result<RunPlayWorkspaceV1, String> {
    if previous.head.head_revision == current.head.head_revision {
        return Ok(current);
    }
    let previous_revision = previous.head.head_revision;
    let mut head = previous.head;
    head.scratch_marks = current.head.scratch_marks.clone();
    head.scratch_return_revision = if remember_endpoint {
        Some(current.head.head_revision)
    } else {
        current.head.scratch_return_revision
    };
    let mut state = previous.state;
    head.head_revision = current.head.head_revision + 1;
    head.state_revision = current.state.state_revision + 1;
    for end in &mut head.scratch_floor_ends {
        if end.head_revision == previous_revision {
            end.head_revision = head.head_revision;
        }
    }
    state.state_revision = head.state_revision;
    paths.publish(current.head.head_revision, &head, Some(&state), None)?;
    Ok(RunPlayWorkspaceV1 { head, state })
}

fn resolve_back_node_v1(
    archive: &TaskRunSolutionArchiveV1,
    start_archive_node_id: &str,
    steps: usize,
) -> Result<String, String> {
    if steps == 0 {
        return Err("back steps must be positive".to_string());
    }
    let mut cursor = start_archive_node_id.to_string();
    for _ in 0..steps {
        loop {
            let node = archive
                .node(&cursor)
                .ok_or_else(|| format!("run archive node {cursor} is missing"))?;
            cursor = node.parent_node_id.clone().ok_or_else(|| {
                "run-play is already at its earliest recoverable decision".to_string()
            })?;
            let parent = archive.node(&cursor).expect("archive parent was validated");
            if archive_node_has_public_decision_v1(archive, parent) {
                break;
            }
        }
    }
    Ok(cursor)
}

/// Moves the durable controller head to any already retained public decision.
/// A selector is either `n<full-or-suffix-node-id>` or an explicit
/// `source:h<revision>` / `target:h<revision>` controller-event boundary.
pub fn jump_v1(paths: &RunPlayPathsV1, selector: &str) -> Result<RunPlayWorkspaceV1, String> {
    // Resolve from the durable archive before opening an exact session. The
    // current head may be a committed terminal, which intentionally has no
    // next public decision and therefore cannot be opened as an interactive
    // exact root. Only the selected target needs to be a recoverable decision.
    let workspace = paths.load()?;
    let cursor =
        resolve_jump_node_from_parts_v1(&workspace.state.archive, &workspace.head, selector)?;
    let from = workspace.head.position.archive_node_id().to_string();
    let expected_head_revision = workspace.head.head_revision;
    let controller = RunPlayControllerV1::open_at_loaded_archive_node_id_v1(
        paths.clone(),
        workspace,
        cursor.clone(),
        true,
    )?;
    let mut head = controller.head.clone();
    head.head_revision = head.head_revision.saturating_add(1);
    head.events.push(RunPlayEventV1::Jump {
        head_revision: head.head_revision,
        from_archive_node_id: from,
        to_archive_node_id: cursor,
        selector: selector.to_string(),
    });
    paths.publish(expected_head_revision, &head, None, None)?;
    paths.load()
}

pub fn profile_storage_v1(paths: &RunPlayPathsV1) -> Result<RunPlayStorageCensusV1, String> {
    let elapsed = Instant::now();
    let (workspace, workspace_load) = paths.load_profiled_v1()?;
    let archive = &workspace.state.archive;
    let archive_payload_bytes = named_msgpack_len_v1(archive, "archive")?;
    let rewritten_state_payload_bytes = named_msgpack_len_v1(&workspace.state, "run-play state")?;
    let archive_task_payload_bytes = named_msgpack_len_v1(&archive.task, "archive task")?;
    let archive_nodes_payload_bytes =
        named_msgpack_len_v1(&archive.nodes, "archive node collection")?;
    let archive_attempts_payload_bytes =
        named_msgpack_len_v1(&archive.attempts, "archive attempt collection")?;

    let nodes = archive
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut incoming_step_count = 0usize;
    let mut persisted_decision_occurrences = 0usize;
    let mut persisted_decision_payload_bytes = 0usize;
    let mut initial_decision_payload_bytes = 0usize;
    let mut source_decision_payload_bytes = 0usize;
    let mut observed_decision_payload_bytes = 0usize;
    let mut decision_observation_payload_bytes = 0usize;
    let mut decision_candidates_payload_bytes = 0usize;
    let mut derived_information_payload_bytes = 0usize;
    let mut exact_parent_source_duplicates = 0usize;
    let mut exact_parent_source_duplicate_payload_bytes = 0usize;

    let mut record_decision =
        |decision: &TaskPublicDecisionV1, label: &str| -> Result<usize, String> {
            let decision_bytes = named_msgpack_len_v1(decision, label)?;
            persisted_decision_occurrences = persisted_decision_occurrences.saturating_add(1);
            persisted_decision_payload_bytes =
                persisted_decision_payload_bytes.saturating_add(decision_bytes);
            decision_observation_payload_bytes = decision_observation_payload_bytes.saturating_add(
                named_msgpack_len_v1(&decision.observation, "decision observation")?,
            );
            decision_candidates_payload_bytes = decision_candidates_payload_bytes.saturating_add(
                named_msgpack_len_v1(&decision.candidates, "decision candidates")?,
            );
            derived_information_payload_bytes = derived_information_payload_bytes.saturating_add(
                named_msgpack_len_v1(&decision.information, "derived information")?,
            );
            Ok(decision_bytes)
        };

    for node in &archive.nodes {
        if let Some(initial) = node.initial_public_decision.as_ref() {
            initial_decision_payload_bytes = initial_decision_payload_bytes
                .saturating_add(record_decision(initial, "initial public decision")?);
        }
        let Some(incoming) = node.incoming_step.as_ref() else {
            continue;
        };
        incoming_step_count = incoming_step_count.saturating_add(1);
        let source = &incoming.step.source;
        let source_bytes = record_decision(source, "incoming source decision")?;
        source_decision_payload_bytes = source_decision_payload_bytes.saturating_add(source_bytes);
        if let Some(observed) = observed_boundary_decision_v1(&incoming.step.observed_boundary) {
            observed_decision_payload_bytes = observed_decision_payload_bytes
                .saturating_add(record_decision(observed, "observed boundary decision")?);
        }
        let parent_decision = node
            .parent_node_id
            .as_deref()
            .and_then(|parent_node_id| nodes.get(parent_node_id).copied())
            .and_then(archive_node_decision_v1);
        if parent_decision == Some(source) {
            exact_parent_source_duplicates = exact_parent_source_duplicates.saturating_add(1);
            exact_parent_source_duplicate_payload_bytes =
                exact_parent_source_duplicate_payload_bytes.saturating_add(source_bytes);
        }
    }

    Ok(RunPlayStorageCensusV1 {
        elapsed: elapsed.elapsed(),
        workspace_load,
        archive_payload_bytes,
        rewritten_state_payload_bytes,
        archive_task_payload_bytes,
        archive_nodes_payload_bytes,
        archive_attempts_payload_bytes,
        checkpoint_payload_bytes: workspace.state.checkpoint_payload.len(),
        node_count: archive.nodes.len(),
        attempt_count: archive.attempts.len(),
        incoming_step_count,
        persisted_decision_occurrences,
        persisted_decision_payload_bytes,
        initial_decision_payload_bytes,
        source_decision_payload_bytes,
        observed_decision_payload_bytes,
        decision_observation_payload_bytes,
        decision_candidates_payload_bytes,
        derived_information_payload_bytes,
        exact_parent_source_duplicates,
        exact_parent_source_duplicate_payload_bytes,
    })
}

pub fn compact_active_v1(
    paths: &RunPlayPathsV1,
    commit: bool,
) -> Result<(RunPlayWorkspaceV1, RunPlayActiveCompactionProfileV1), String> {
    let mut loaded = paths.load()?;
    let checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
        &loaded.state.checkpoint_payload,
        &loaded.state.archive,
        DEFAULT_ARTIFACT_MAX_BYTES,
    )?;
    let (compact_archive, compact_checkpoints) =
        active_recovery_projection_v1(&loaded, &checkpoints)?;
    let compact_checkpoint_payload = compact_checkpoints.encode(DEFAULT_ARTIFACT_MAX_BYTES)?;
    let state_revision = loaded.state.state_revision.saturating_add(1);
    let state = RunPlayStateV1 {
        schema: RUN_PLAY_STATE_SCHEMA_V1.to_string(),
        state_revision,
        archive: compact_archive,
        checkpoint_payload: compact_checkpoint_payload,
    };
    let state_payload = rmp_serde::to_vec_named(&state)
        .map_err(|error| format!("failed to size compact run-play state: {error}"))?;
    if state_payload.len() > DEFAULT_ARTIFACT_MAX_BYTES {
        return Err("compact run-play state exceeds the artifact size limit".to_string());
    }
    let profile = RunPlayActiveCompactionProfileV1 {
        before_nodes: loaded.state.archive.nodes.len(),
        after_nodes: state.archive.nodes.len(),
        before_attempts: loaded.state.archive.attempts.len(),
        after_attempts: state.archive.attempts.len(),
        before_checkpoints: checkpoints.checkpoint_count(),
        after_checkpoints: compact_checkpoints.checkpoint_count(),
        before_checkpoint_payload_bytes: loaded.state.checkpoint_payload.len(),
        after_checkpoint_payload_bytes: state.checkpoint_payload.len(),
        after_state_payload_bytes: state_payload.len(),
    };
    if !commit {
        return Ok((loaded, profile));
    }
    let mut head = loaded.head.clone();
    head.head_revision = head.head_revision.saturating_add(1);
    head.state_revision = state_revision;
    head.delta_revisions.clear();
    head.events.clear();
    head.events.push(RunPlayEventV1::CompactActive {
        head_revision: head.head_revision,
        archive_node_id: head.position.archive_node_id().to_string(),
        state_revision,
        before_nodes: profile.before_nodes,
        after_nodes: profile.after_nodes,
        before_checkpoints: profile.before_checkpoints,
        after_checkpoints: profile.after_checkpoints,
    });
    paths.publish(loaded.head.head_revision, &head, Some(&state), None)?;
    loaded.head = head;
    loaded.state = state;
    Ok((loaded, profile))
}

fn archive_node_decision_v1(
    node: &sts_oracle_learning::agent::task::TaskRunArchiveNodeV1,
) -> Option<&TaskPublicDecisionV1> {
    node.initial_public_decision.as_ref().or_else(|| {
        node.incoming_step
            .as_ref()
            .and_then(|incoming| observed_boundary_decision_v1(&incoming.step.observed_boundary))
    })
}

fn observed_boundary_decision_v1(
    boundary: &TaskObservedBoundaryV1,
) -> Option<&TaskPublicDecisionV1> {
    match boundary {
        TaskObservedBoundaryV1::Decision { decision }
        | TaskObservedBoundaryV1::CommittedNextScope { decision } => Some(decision),
        TaskObservedBoundaryV1::CommitReady
        | TaskObservedBoundaryV1::ExploredScopeExit { .. }
        | TaskObservedBoundaryV1::CommittedGameTerminal { .. } => None,
    }
}

fn named_msgpack_len_v1<T: Serialize + ?Sized>(value: &T, label: &str) -> Result<usize, String> {
    rmp_serde::to_vec_named(value)
        .map(|payload| payload.len())
        .map_err(|error| format!("failed to measure {label}: {error}"))
}

pub(crate) fn resolve_jump_node_from_parts_v1(
    archive: &TaskRunSolutionArchiveV1,
    head: &RunPlayHeadV1,
    selector: &str,
) -> Result<String, String> {
    if selector.trim().is_empty() {
        return Err("jump selector must not be empty".to_string());
    }
    if let Some((boundary, revision)) = parse_controller_event_selector_v1(selector)? {
        let event = head
            .events
            .iter()
            .find(|event| match event {
                RunPlayEventV1::Choice { head_revision, .. }
                | RunPlayEventV1::AnchorLine { head_revision, .. }
                | RunPlayEventV1::Back { head_revision, .. }
                | RunPlayEventV1::Jump { head_revision, .. }
                | RunPlayEventV1::Checkpoint { head_revision, .. }
                | RunPlayEventV1::CompactActive { head_revision, .. } => *head_revision == revision,
            })
            .ok_or_else(|| format!("run-play has no controller event h{revision}"))?;
        let node_id = controller_event_boundary_node_id_v1(event, boundary)?;
        let node = archive.node(node_id).ok_or_else(|| {
            format!(
                "controller event h{revision} {} boundary references missing archive node n{}",
                boundary.as_str(),
                node_id
            )
        })?;
        if !archive_node_has_public_decision_v1(archive, node) {
            return Err(format!(
                "controller event h{revision} {} boundary n{} is not a recoverable public decision; choose the other boundary or use an exact nNODE printed by history",
                boundary.as_str(),
                short_id(node_id)
            ));
        }
        return Ok(node_id.to_string());
    }

    let node_selector = selector.strip_prefix('n').ok_or_else(|| {
        format!(
            "invalid jump selector {selector}; use n<NODE> for an exact public decision, source:h<N> for the decision before a controller event, or target:h<N> for its recorded target"
        )
    })?;
    if node_selector.is_empty() {
        return Err("archive-node jump selector n<NODE> requires a node identity".to_string());
    }

    let matches = archive
        .nodes
        .iter()
        .filter(|node| archive_node_has_public_decision_v1(archive, node))
        .filter(|node| node.node_id == node_selector || node.node_id.ends_with(node_selector))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [node] => Ok(node.node_id.clone()),
        [] => Err(format!(
            "no retained public decision matches archive-node selector n{node_selector}"
        )),
        _ => Err(format!(
            "archive-node selector n{node_selector} is ambiguous across {} retained decisions",
            matches.len()
        )),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunPlayControllerEventBoundaryV1 {
    Source,
    Target,
}

impl RunPlayControllerEventBoundaryV1 {
    fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
        }
    }
}

fn parse_controller_event_selector_v1(
    selector: &str,
) -> Result<Option<(RunPlayControllerEventBoundaryV1, u64)>, String> {
    let parsed = [
        ("source:h", RunPlayControllerEventBoundaryV1::Source),
        ("target:h", RunPlayControllerEventBoundaryV1::Target),
    ]
    .into_iter()
    .find_map(|(prefix, boundary)| {
        selector
            .strip_prefix(prefix)
            .map(|revision| (boundary, revision))
    });
    if let Some((boundary, revision)) = parsed {
        let revision = revision.parse::<u64>().map_err(|error| {
            format!(
                "invalid {} controller-event selector {selector}: expected {}:h<revision>: {error}",
                boundary.as_str(),
                boundary.as_str()
            )
        })?;
        return Ok(Some((boundary, revision)));
    }
    if selector.starts_with('h') {
        return Err(format!(
            "controller-event selector {selector} is directionally ambiguous; use source:{selector} for the decision before the event, target:{selector} for its recorded target, or prefer an exact nNODE"
        ));
    }
    Ok(None)
}

fn controller_event_boundary_node_id_v1(
    event: &RunPlayEventV1,
    boundary: RunPlayControllerEventBoundaryV1,
) -> Result<&str, String> {
    match (boundary, event) {
        (
            RunPlayControllerEventBoundaryV1::Source,
            RunPlayEventV1::Choice {
                from_archive_node_id,
                ..
            }
            | RunPlayEventV1::AnchorLine {
                from_archive_node_id,
                ..
            }
            | RunPlayEventV1::Back {
                from_archive_node_id,
                ..
            }
            | RunPlayEventV1::Jump {
                from_archive_node_id,
                ..
            },
        ) => Ok(from_archive_node_id),
        (
            RunPlayControllerEventBoundaryV1::Target,
            RunPlayEventV1::Choice {
                to_archive_node_id,
                head_revision,
                ..
            },
        ) => to_archive_node_id.as_deref().ok_or_else(|| {
            format!(
                "controller event h{head_revision} has no recorded target archive node; use source:h{head_revision}"
            )
        }),
        (
            RunPlayControllerEventBoundaryV1::Target,
            RunPlayEventV1::AnchorLine {
                to_archive_node_id,
                ..
            }
            | RunPlayEventV1::Back {
                to_archive_node_id,
                ..
            }
            | RunPlayEventV1::Jump {
                to_archive_node_id,
                ..
            },
        ) => Ok(to_archive_node_id),
        (
            RunPlayControllerEventBoundaryV1::Source
            | RunPlayControllerEventBoundaryV1::Target,
            RunPlayEventV1::Checkpoint {
                archive_node_id, ..
            }
            | RunPlayEventV1::CompactActive {
                archive_node_id, ..
            },
        ) => Ok(archive_node_id),
    }
}

pub fn export_v1(
    workspace: &RunPlayWorkspaceV1,
    archive_output: &Path,
    checkpoint_output: &Path,
) -> Result<(), String> {
    if archive_output == checkpoint_output {
        return Err("archive and checkpoint export paths must differ".to_string());
    }
    if let Some(parent) = archive_output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create export directory {}: {error}",
                parent.display()
            )
        })?;
    }
    if let Some(parent) = checkpoint_output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create export directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
        &workspace.state.checkpoint_payload,
        &workspace.state.archive,
        DEFAULT_ARTIFACT_MAX_BYTES,
    )?;
    let (archive, checkpoints) = active_recovery_projection_v1(workspace, &checkpoints)?;
    let archive_payload = serde_json::to_vec(&archive)
        .map_err(|error| format!("failed to encode run archive export: {error}"))?;
    let checkpoint_payload = checkpoints.encode(DEFAULT_ARTIFACT_MAX_BYTES)?;
    write_new_synced(archive_output, &archive_payload, "run archive export")?;
    write_new_synced(
        checkpoint_output,
        &checkpoint_payload,
        "run checkpoint export",
    )
}

fn active_recovery_projection_v1(
    workspace: &RunPlayWorkspaceV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
) -> Result<(TaskRunSolutionArchiveV1, TaskRunBranchCheckpointArtifactV1), String> {
    let archive = workspace
        .state
        .archive
        .retained_recovery_path_to_node_v1(workspace.head.position.archive_node_id())?;
    let checkpoints = checkpoints.retained_for_archive_v1(&archive)?;
    Ok((archive, checkpoints))
}

fn validate_inventory_choice_v1(
    decision: &sts_oracle_learning::eval::run_control::TaskRunBranchDecisionInventoryV1,
    choice: &TaskPublicChoiceV1,
) -> Result<(), String> {
    let Some(first) = choice.ordinal_path.first().copied() else {
        return Err("choice requires at least one ordinal".to_string());
    };
    let candidate = decision
        .candidates
        .get(first)
        .ok_or_else(|| format!("candidate ordinal {first} is out of range"))?;
    if let Some(direct) = &candidate.direct_choice {
        if direct != choice {
            return Err(format!(
                "candidate {first} accepts only the direct choice {:?}",
                direct.ordinal_path
            ));
        }
    } else {
        match &candidate.candidate {
            TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::BeginRunCardSelection {
                        min_choices,
                        max_choices,
                        selectable_card_ordinals,
                        ..
                    },
            } => {
                let payload = &choice.ordinal_path[1..];
                if payload.len() < *min_choices || payload.len() > *max_choices {
                    return Err(format!(
                        "candidate {first} requires {min_choices}..={max_choices} selection ordinals; received {}",
                        payload.len()
                    ));
                }
                let mut unique = std::collections::BTreeSet::new();
                for ordinal in payload {
                    if *ordinal >= selectable_card_ordinals.len() {
                        return Err(format!(
                            "candidate {first} selection ordinal {ordinal} is out of range 0..{}",
                            selectable_card_ordinals.len()
                        ));
                    }
                    if !unique.insert(*ordinal) {
                        return Err(format!(
                            "candidate {first} repeats selection ordinal {ordinal}"
                        ));
                    }
                }
            }
            TaskPublicCandidateV1::CombatSelectionFamily { family } => {
                let payload = &choice.ordinal_path[1..];
                let payload_len = u64::try_from(payload.len())
                    .map_err(|_| "combat selection length does not fit u64".to_string())?;
                if payload_len < family.declared_min || payload_len > family.effective_max {
                    return Err(format!(
                        "candidate {first} requires {}..={} selection ordinals; received {}",
                        family.declared_min,
                        family.effective_max,
                        payload.len()
                    ));
                }
                let mut unique = BTreeSet::new();
                for ordinal in payload {
                    let selected = family.domain.get(*ordinal).ok_or_else(|| {
                        format!(
                            "candidate {first} selection ordinal {ordinal} is out of range 0..{}",
                            family.domain.len()
                        )
                    })?;
                    let available = match selected {
                        PublicCombatSelectionDomainCandidateV1::Card { eligible, .. } => *eligible,
                        PublicCombatSelectionDomainCandidateV1::Scry {
                            currently_present, ..
                        } => *currently_present,
                    };
                    if !available {
                        return Err(format!(
                            "candidate {first} selection ordinal {ordinal} is not currently available"
                        ));
                    }
                    if !unique.insert(*ordinal) {
                        return Err(format!(
                            "candidate {first} repeats selection ordinal {ordinal}"
                        ));
                    }
                }
            }
            _ => {
                return Err(format!(
                    "candidate {first} is marked parameterized but exposes no supported selection contract"
                ));
            }
        }
    }
    Ok(())
}

fn observed_direct_successor_v1(
    archive: &TaskRunSolutionArchiveV1,
    current_node_id: &str,
    choice: &TaskPublicChoiceV1,
) -> Result<Option<RunPlayPositionV1>, String> {
    let mut children = archive
        .nodes
        .iter()
        .filter(|node| node.parent_node_id.as_deref() == Some(current_node_id))
        .filter(|node| {
            node.incoming_step
                .as_ref()
                .is_some_and(|incoming| incoming.step.choice == *choice)
        })
        .collect::<Vec<_>>();
    if children.len() > 1 {
        return Err(
            "one public choice resolves to multiple immediate archive children".to_string(),
        );
    }
    let Some(child) = children.pop() else {
        return Ok(None);
    };
    let incoming = child
        .incoming_step
        .as_ref()
        .expect("non-root child has an incoming public step");
    match &incoming.step.observed_boundary {
        TaskObservedBoundaryV1::Decision { .. }
        | TaskObservedBoundaryV1::CommittedNextScope { .. } => Ok(Some(
            RunPlayPositionV1::Decision {
                archive_node_id: child.node_id.clone(),
            },
        )),
        TaskObservedBoundaryV1::CommittedGameTerminal { game_outcome } => {
            Ok(Some(RunPlayPositionV1::Terminal {
                archive_node_id: child.node_id.clone(),
                outcome: outcome_for_terminal_v1(archive, *game_outcome),
            }))
        }
        TaskObservedBoundaryV1::CommitReady | TaskObservedBoundaryV1::ExploredScopeExit { .. } => {
            Err("observed choice stops at an internal scope boundary and has no directly navigable public successor".to_string())
        }
    }
}

fn outcome_for_terminal_v1(
    archive: &TaskRunSolutionArchiveV1,
    game_outcome: GameOutcomeV1,
) -> TaskEpisodeOutcomeV1 {
    let success = matches!(
        (archive.task.objective, game_outcome),
        (
            TaskObjectiveV1::ActThreeVictory,
            GameOutcomeV1::ActThreeVictory | GameOutcomeV1::HeartVictory
        ) | (TaskObjectiveV1::HeartVictory, GameOutcomeV1::HeartVictory)
    );
    if success {
        TaskEpisodeOutcomeV1::Success { game_outcome }
    } else {
        use sts_oracle_learning::agent::task::TaskEpisodeFailureV1;
        TaskEpisodeOutcomeV1::Failure {
            reason: match game_outcome {
                GameOutcomeV1::Defeat => TaskEpisodeFailureV1::CommittedDefeat,
                GameOutcomeV1::ActThreeVictory | GameOutcomeV1::HeartVictory => {
                    TaskEpisodeFailureV1::CommittedTerminalWithoutObjective
                }
            },
            game_outcome: Some(game_outcome),
        }
    }
}

fn combat_research_probe_from_replay_v1(
    candidate: &sts_oracle_learning::agent::task::TaskResearchPlanCandidateV1,
    replay: CombatResearchReplayMaterializationV1,
    archive: &TaskRunSolutionArchiveV1,
) -> Result<ExactRunPlayProbeLineV1, String> {
    if replay.steps.len() != candidate.plan.steps.len() {
        return Err("combat research replay and plan lengths disagree".to_string());
    }
    let mut recorded_steps = replay.steps.into_iter().peekable();
    let mut steps = Vec::with_capacity(candidate.plan.steps.len());
    for plan_step in &candidate.plan.steps {
        let recorded = recorded_steps.next().expect("replay length checked");
        let source_decision = recorded.source_decision;
        if plan_step.source_public_snapshot_id != source_decision.information.snapshot_id {
            return Err("combat research retained replay snapshot differs from plan".to_string());
        }
        source_decision.validate_choice(&plan_step.choice)?;
        let selected_candidate =
            source_decision.candidates[plan_step.choice.ordinal_path[0]].clone();
        let next_decision = recorded_steps
            .peek()
            .map(|step| &step.source_decision)
            .or(replay.successor_decision.as_ref())
            .cloned();
        let outcome = match (&next_decision, replay.terminal_game_outcome) {
            (Some(decision), _) => TaskEpisodeOutcomeV1::Paused {
                reason: sts_oracle_learning::agent::task::TaskEpisodePauseV1::PublicDecisionControlYield {
                    site: decision.site,
                },
            },
            (None, Some(game_outcome)) => outcome_for_terminal_v1(archive, game_outcome),
            (None, None) => return Err("combat research replay has no public or terminal successor".to_string()),
        };
        steps.push(ExactRunPlayProbeStepV1 {
            source_decision,
            monster_entities_before: recorded.monster_entities_before,
            monster_entities_after: recorded.monster_entities_after,
            choice: plan_step.choice.clone(),
            selected_candidate,
            action_result: recorded.action_result,
            outcome,
            next_decision,
            elapsed: recorded.elapsed,
        });
    }
    Ok(ExactRunPlayProbeLineV1 {
        steps,
        fork_elapsed: replay.fork_elapsed,
        total_elapsed: replay.total_elapsed,
    })
}

fn terminal_outcome(position: &RunPlayPositionV1) -> Option<&TaskEpisodeOutcomeV1> {
    match position {
        RunPlayPositionV1::Terminal { outcome, .. } => Some(outcome),
        RunPlayPositionV1::Decision { .. } => None,
    }
}

fn prepared_line_endpoint_v1(
    archive_node_id: &str,
    current_decision: Option<&TaskRunBranchDecisionInventoryV1>,
    terminal_outcome: Option<&TaskEpisodeOutcomeV1>,
) -> Result<(RunPlayPositionV1, Option<TaskRunBranchDecisionInventoryV1>), String> {
    match (current_decision, terminal_outcome) {
        (Some(decision), None) => Ok((
            RunPlayPositionV1::Decision {
                archive_node_id: archive_node_id.to_string(),
            },
            Some(decision.clone()),
        )),
        (None, Some(outcome)) => Ok((
            RunPlayPositionV1::Terminal {
                archive_node_id: archive_node_id.to_string(),
                outcome: outcome.clone(),
            },
            None,
        )),
        _ => {
            Err("prepared run-play line has an inconsistent decision/terminal endpoint".to_string())
        }
    }
}

fn can_publish_hot_choice_as_append_only_delta_v1(
    checkpoints_added: usize,
    position: &RunPlayPositionV1,
) -> bool {
    checkpoints_added == 0 && matches!(position, RunPlayPositionV1::Decision { .. })
}

fn run_play_delta_v1(
    loaded: &RunPlayWorkspaceV1,
    archive: &TaskRunSolutionArchiveV1,
    previous_checkpoint_count: usize,
    new_checkpoint_count: usize,
    delta_revision: u64,
) -> Option<RunPlayDeltaV1> {
    if previous_checkpoint_count != new_checkpoint_count
        || !archive.nodes.starts_with(&loaded.state.archive.nodes)
        || !archive.attempts.starts_with(&loaded.state.archive.attempts)
    {
        return None;
    }
    Some(RunPlayDeltaV1 {
        schema: RUN_PLAY_DELTA_SCHEMA_V1.to_string(),
        delta_revision,
        base_state_revision: loaded.state.state_revision,
        expected_archive_nodes: loaded.state.archive.nodes.len(),
        expected_archive_attempts: loaded.state.archive.attempts.len(),
        appended_nodes: archive.nodes[loaded.state.archive.nodes.len()..].to_vec(),
        appended_attempts: archive.attempts[loaded.state.archive.attempts.len()..].to_vec(),
    })
}

fn archive_node_has_public_decision_v1(
    archive: &TaskRunSolutionArchiveV1,
    node: &sts_oracle_learning::agent::task::TaskRunArchiveNodeV1,
) -> bool {
    node.initial_public_decision.is_some()
        || node.incoming_step.as_ref().is_some_and(|incoming| {
            matches!(
                incoming.step.observed_boundary,
                TaskObservedBoundaryV1::Decision { .. }
                    | TaskObservedBoundaryV1::CommittedNextScope { .. }
            )
        })
        || archive
            .nodes
            .iter()
            .any(|child| child.parent_node_id.as_deref() == Some(node.node_id.as_str()))
}

pub fn combat_history_v1(
    archive: &TaskRunSolutionArchiveV1,
    current_node_id: &str,
    limit: usize,
    from_node_id: Option<&str>,
) -> Result<RunPlayCombatHistoryV1, String> {
    let current_path = run_play_path_v1(archive, current_node_id)?;
    let combat_steps = if let Some(from_node_id) = from_node_id {
        let start = current_path
            .iter()
            .position(|step| {
                step.site == TaskDecisionSiteV1::Combat
                    && archive
                        .node(&step.archive_node_id)
                        .and_then(|node| node.parent_node_id.as_deref())
                        == Some(from_node_id)
            })
            .ok_or_else(|| {
                format!(
                    "archive node {from_node_id} is not a combat-action source on the active path"
                )
            })?;
        current_path
            .into_iter()
            .skip(start)
            .take_while(|step| step.site == TaskDecisionSiteV1::Combat)
            .collect::<Vec<_>>()
    } else {
        current_path
            .into_iter()
            .filter(|step| step.site == TaskDecisionSiteV1::Combat)
            .collect::<Vec<_>>()
    };
    let total_combat_steps = combat_steps.len();
    let skip = if from_node_id.is_some() {
        0
    } else {
        total_combat_steps.saturating_sub(limit)
    };
    let mut steps = Vec::with_capacity(total_combat_steps.saturating_sub(skip).min(limit));

    for path in combat_steps.into_iter().skip(skip).take(limit) {
        let node = archive
            .node(&path.archive_node_id)
            .ok_or_else(|| format!("combat history node {} is missing", path.archive_node_id))?;
        let incoming = node.incoming_step.as_ref().ok_or_else(|| {
            format!(
                "combat history node {} has no incoming step",
                path.archive_node_id
            )
        })?;
        let TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } = &incoming.step.source.observation
        else {
            return Err(format!(
                "combat history node {} has a non-combat observation",
                path.archive_node_id
            ));
        };
        let encounter = match run_context {
            sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Available {
                encounter_id,
                ..
            } => encounter_id.map(|encounter| encounter.stable_id().to_string()),
            sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Unavailable {
                ..
            } => None,
        };
        let hand = observation
            .cards
            .hand
            .cards
            .iter()
            .map(|card| {
                format!(
                    "{:?}{}@{}",
                    card.card_id,
                    if card.upgrades > 0 { "+" } else { "" },
                    card.effective_cost
                )
            })
            .collect();
        let potions = observation
            .potions
            .iter()
            .enumerate()
            .filter_map(|(slot, potion)| {
                potion
                    .as_ref()
                    .map(|potion| format!("{slot}:{}", potion.potion_id.stable_id()))
            })
            .collect();
        let player_powers = observation
            .player
            .powers
            .iter()
            .map(|power| format!("{}:{}", power.power.stable_id(), power.amount))
            .collect();
        let monsters = observation
            .monsters
            .iter()
            .filter(|monster| monster.alive && !monster.escaped && !monster.dying)
            .map(|monster| {
                let enemy = match monster.enemy {
                    CombatLearningEnemyIdentityV1::Known { enemy_id } => {
                        enemy_id.stable_id().to_string()
                    }
                };
                let intent = match (
                    &monster.intent.intent,
                    monster.intent.preview_damage_per_hit,
                ) {
                    (Some(intent), Some(damage)) => format!("{intent:?}@{damage}"),
                    (Some(intent), None) => format!("{intent:?}"),
                    (None, _) => "hidden".to_string(),
                };
                RunPlayCombatMonsterHistoryV1 {
                    slot: monster.slot,
                    enemy,
                    hp: monster.hp,
                    max_hp: monster.max_hp,
                    block: monster.block,
                    intent,
                    powers: monster
                        .powers
                        .iter()
                        .map(|power| format!("{}:{}", power.power.stable_id(), power.amount))
                        .collect(),
                }
            })
            .collect();
        let from_archive_node_id = node.parent_node_id.clone().ok_or_else(|| {
            format!(
                "combat history node {} has no parent decision",
                path.archive_node_id
            )
        })?;
        steps.push(RunPlayCombatHistoryStepV1 {
            from_archive_node_id,
            path,
            encounter,
            turn: observation.turn.turn_count,
            phase: format!("{:?}", observation.turn.phase),
            energy: observation.turn.energy,
            player_block: observation.player.block,
            hand,
            potions,
            player_powers,
            monsters,
        });
    }

    Ok(RunPlayCombatHistoryV1 {
        current_archive_node_id: current_node_id.to_string(),
        total_combat_steps,
        steps,
    })
}

/// Projects the durable public construction history without restoring an
/// exact checkpoint or running policy/search code. Route and combat decisions
/// are omitted; every remaining strategic boundary keeps its full candidate
/// surface and the deck that was public before the committed choice.
pub fn construction_history_v1(
    paths: &RunPlayPathsV1,
    site_filter: Option<TaskDecisionSiteV1>,
    candidate_card_filter: Option<CardId>,
    added_card_filter: Option<CardId>,
    include_routine_boundaries: bool,
) -> Result<RunPlayConstructionHistoryV1, String> {
    if matches!(
        site_filter,
        Some(TaskDecisionSiteV1::Combat | TaskDecisionSiteV1::Map)
    ) {
        return Err("construction history does not include combat or map decisions".to_string());
    }
    let workspace = paths.load()?;
    workspace.state.archive.validate()?;
    let selected_node_id = workspace.head.position.archive_node_id();
    let current_path = run_play_path_v1(&workspace.state.archive, selected_node_id)?;
    let mut steps = Vec::new();
    for path_step in &current_path {
        if matches!(
            path_step.site,
            TaskDecisionSiteV1::Combat | TaskDecisionSiteV1::Map
        ) || site_filter.is_some_and(|site| site != path_step.site)
        {
            continue;
        }
        let node = workspace
            .state
            .archive
            .node(&path_step.archive_node_id)
            .ok_or_else(|| {
                format!(
                    "construction path node {} is missing",
                    path_step.archive_node_id
                )
            })?;
        let incoming = node.incoming_step.as_ref().ok_or_else(|| {
            format!(
                "construction path node {} has no incoming public step",
                node.node_id
            )
        })?;
        if added_card_filter
            .is_some_and(|card| !construction_step_adds_card_v1(&incoming.step, card))
        {
            continue;
        }
        if candidate_card_filter.is_some_and(|card| {
            !incoming
                .step
                .source
                .candidates
                .iter()
                .any(|candidate| construction_candidate_mentions_card_v1(candidate, card))
        }) {
            continue;
        }
        let TaskPublicObservationV1::Strategic { observation } = &incoming.step.source.observation
        else {
            return Err(format!(
                "construction path node {} has a non-strategic observation",
                node.node_id
            ));
        };
        let selected_ordinal = incoming
            .step
            .choice
            .ordinal_path
            .first()
            .copied()
            .ok_or_else(|| {
                format!(
                    "construction path node {} has an empty choice",
                    node.node_id
                )
            })?;
        if selected_ordinal >= incoming.step.source.candidates.len() {
            return Err(format!(
                "construction path node {} selects candidate {} from {} candidates",
                node.node_id,
                selected_ordinal,
                incoming.step.source.candidates.len()
            ));
        }
        if !include_routine_boundaries
            && !construction_candidate_is_ledger_relevant_v1(
                &incoming.step.source.candidates[selected_ordinal],
            )
        {
            continue;
        }
        let candidates = incoming
            .step
            .source
            .candidates
            .iter()
            .cloned()
            .enumerate()
            .map(|(ordinal, candidate)| {
                let choice = TaskPublicChoiceV1 {
                    ordinal_path: vec![ordinal],
                };
                let action = semantic_action_for_choice_v1(&incoming.step.source, &choice)
                    .unwrap_or_else(|_| format!("{candidate:?}"));
                RunPlayConstructionCandidateV1 {
                    ordinal,
                    kind: construction_candidate_kind_v1(&candidate),
                    action,
                    selected: ordinal == selected_ordinal,
                    candidate,
                }
            })
            .collect();
        let selected_action =
            semantic_action_for_choice_v1(&incoming.step.source, &incoming.step.choice)
                .unwrap_or_else(|error| format!("<semantic-error:{error}>"));
        let deck = observation.cards.clone();
        let relics = observation
            .relics
            .iter()
            .map(|relic| relic.relic.stable_id().to_string())
            .collect();
        let potions = observation
            .potions
            .iter()
            .map(|slot| {
                slot.potion.as_ref().map_or_else(
                    || format!("{}:empty", slot.slot),
                    |potion| format!("{}:{}", slot.slot, potion.potion.stable_id()),
                )
            })
            .collect();
        steps.push(RunPlayConstructionHistoryStepV1 {
            from_archive_node_id: node
                .parent_node_id
                .clone()
                .expect("non-root construction step has a parent"),
            to_archive_node_id: node.node_id.clone(),
            depth: node.depth,
            site: incoming.site,
            act: observation.run.act,
            floor: observation.run.floor,
            current_hp: observation.run.current_hp,
            max_hp: observation.run.max_hp,
            gold: observation.run.gold,
            deck,
            relics,
            potions,
            candidates,
            choice: incoming.step.choice.clone(),
            selected_action,
        });
    }
    Ok(RunPlayConstructionHistoryV1 {
        current_archive_node_id: selected_node_id.to_string(),
        // Historical archive nodes retain the public source, candidates, and
        // committed choice. They intentionally do not retain the private
        // owner's ranking or rationale that produced that choice.
        owner_ranking_retained: false,
        steps,
    })
}

fn construction_step_adds_card_v1(step: &TaskPublicStepV1, expected: CardId) -> bool {
    let TaskPublicObservationV1::Strategic {
        observation: before,
    } = &step.source.observation
    else {
        return false;
    };
    let successor = match &step.observed_boundary {
        TaskObservedBoundaryV1::Decision { decision }
        | TaskObservedBoundaryV1::CommittedNextScope { decision } => decision,
        TaskObservedBoundaryV1::CommitReady
        | TaskObservedBoundaryV1::ExploredScopeExit { .. }
        | TaskObservedBoundaryV1::CommittedGameTerminal { .. } => return false,
    };
    let TaskPublicObservationV1::Strategic { observation: after } = &successor.observation else {
        return false;
    };
    construction_deck_adds_card_v1(&before.cards, &after.cards, expected)
}

fn construction_deck_adds_card_v1(
    before: &[TaskStrategicCardV1],
    after: &[TaskStrategicCardV1],
    expected: CardId,
) -> bool {
    after.iter().filter(|card| card.card == expected).count()
        > before.iter().filter(|card| card.card == expected).count()
}

fn construction_candidate_is_ledger_relevant_v1(candidate: &TaskPublicCandidateV1) -> bool {
    let TaskPublicCandidateV1::Strategic { action } = candidate else {
        return false;
    };
    !matches!(
        action,
        TaskStrategicActionV1::OpenCardReward { .. }
            | TaskStrategicActionV1::OpenPendingRewards
            | TaskStrategicActionV1::BeginRunCardSelection { .. }
            | TaskStrategicActionV1::OpenChest
            | TaskStrategicActionV1::Proceed { .. }
            | TaskStrategicActionV1::Cancel { .. }
            | TaskStrategicActionV1::ClaimReward {
                reward: PlannerRewardDescriptor::Gold { .. }
                    | PlannerRewardDescriptor::StolenGold { .. },
                ..
            }
    )
}

fn construction_candidate_mentions_card_v1(
    candidate: &TaskPublicCandidateV1,
    expected: CardId,
) -> bool {
    let TaskPublicCandidateV1::Strategic { action } = candidate else {
        return false;
    };
    matches!(
        action,
        TaskStrategicActionV1::TakeCard { card, .. }
            | TaskStrategicActionV1::Smith { card, .. }
            | TaskStrategicActionV1::Toke { card, .. }
            | TaskStrategicActionV1::BuyCard { card, .. }
            | TaskStrategicActionV1::RemoveCard { card, .. }
            if *card == expected
    )
}

fn construction_candidate_kind_v1(
    candidate: &TaskPublicCandidateV1,
) -> RunPlayConstructionCandidateKindV1 {
    match candidate {
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::UseRunPotion { .. }
                | TaskStrategicActionV1::DiscardRunPotion { .. },
        } => RunPlayConstructionCandidateKindV1::AmbientResource,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Cancel { .. },
        } => RunPlayConstructionCandidateKindV1::Navigation,
        _ => RunPlayConstructionCandidateKindV1::SiteOption,
    }
}

fn combat_parity_projection_v1(
    observation: &PublicCombatStateV1,
) -> RunPlayCombatParityProjectionV1 {
    let power = |power: &sts_oracle_learning::agent::information::state::CombatLearningPowerV1| {
        RunPlayCombatParityPowerV1 {
            power_id: power.power.stable_id().to_owned(),
            amount: power.amount,
        }
    };
    let mut same_enemy_counts = BTreeMap::<String, usize>::new();
    let monsters = observation
        .monsters
        .iter()
        .enumerate()
        .map(|(source_index, monster)| {
            let enemy_id = match monster.enemy {
                CombatLearningEnemyIdentityV1::Known { enemy_id } => {
                    enemy_id.stable_id().to_owned()
                }
            };
            let same_enemy_ordinal = *same_enemy_counts.entry(enemy_id.clone()).or_default();
            *same_enemy_counts.get_mut(&enemy_id).unwrap() += 1;
            RunPlayCombatParityMonsterV1 {
                source_index,
                source_slot: monster.slot,
                enemy_id,
                same_enemy_ordinal,
                hp: monster.hp,
                max_hp: monster.max_hp,
                block: monster.block,
                alive: monster.alive,
                intent: monster
                    .intent
                    .intent
                    .as_ref()
                    .map(|intent| intent.kind_v1().to_owned()),
                powers: monster.powers.iter().map(power).collect(),
            }
        })
        .collect();
    RunPlayCombatParityProjectionV1 {
        turn_index: observation.turn.turn_count,
        energy: observation.turn.energy,
        player_block: observation.player.block,
        hand: observation
            .cards
            .hand
            .cards
            .iter()
            .map(|card| RunPlayCombatParityCardV1 {
                card_id: card.card_id.stable_id().to_owned(),
                upgrades: card.upgrades,
                cost_for_turn: card.effective_cost,
            })
            .collect(),
        potions: observation
            .potions
            .iter()
            .map(|potion| {
                potion
                    .as_ref()
                    .map(|potion| potion.potion_id.stable_id().to_owned())
            })
            .collect(),
        player_powers: observation.player.powers.iter().map(power).collect(),
        monsters,
    }
}

pub(crate) fn run_play_path_v1(
    archive: &TaskRunSolutionArchiveV1,
    current_node_id: &str,
) -> Result<Vec<RunPlayPathStepV1>, String> {
    let mut node_ids = Vec::new();
    let mut cursor = Some(current_node_id);
    while let Some(node_id) = cursor {
        let node = archive
            .node(node_id)
            .ok_or_else(|| format!("run archive path node {node_id} is missing"))?;
        node_ids.push(node.node_id.as_str());
        cursor = node.parent_node_id.as_deref();
    }
    node_ids.reverse();
    node_ids
        .into_iter()
        .skip(1)
        .map(|node_id| {
            let node = archive.node(node_id).expect("path node resolved");
            let incoming = node
                .incoming_step
                .as_ref()
                .expect("non-root path node has an incoming step");
            let selected_ordinal = incoming
                .step
                .choice
                .ordinal_path
                .first()
                .copied()
                .ok_or_else(|| format!("archive node {node_id} has an empty public choice"))?;
            let selected_candidate = incoming
                .step
                .source
                .candidates
                .get(selected_ordinal)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "archive node {node_id} choice ordinal {selected_ordinal} is out of range"
                    )
                })?;
            let semantic_action = semantic_action_for_choice_v1(
                &incoming.step.source,
                &incoming.step.choice,
            )
            .unwrap_or_else(|error| format!("<semantic-error:{error}>"));
            let (before, combat_before) = match &incoming.step.source.observation {
                TaskPublicObservationV1::Strategic { observation } => (
                    RunPlayPublicScalarsV1 {
                        act: Some(observation.run.act),
                        floor: Some(observation.run.floor),
                        current_hp: observation.run.current_hp,
                        max_hp: observation.run.max_hp,
                        gold: observation.run.gold,
                    },
                    None,
                ),
                TaskPublicObservationV1::Combat {
                    run_context,
                    observation,
                } => {
                    let (act, floor) = match run_context {
                        sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Available {
                            act, floor, ..
                        } => (Some(*act), Some(*floor)),
                        sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Unavailable { .. } => (None, None),
                    };
                    (
                        RunPlayPublicScalarsV1 {
                            act,
                            floor,
                            current_hp: observation.player.hp,
                            max_hp: observation.player.max_hp,
                            gold: observation.player.gold,
                        },
                        Some(combat_parity_projection_v1(observation)),
                    )
                }
            };
            let observed_boundary = match &incoming.step.observed_boundary {
                TaskObservedBoundaryV1::Decision { .. } => RunPlayBoundaryKindV1::Decision,
                TaskObservedBoundaryV1::CommitReady => RunPlayBoundaryKindV1::CommitReady,
                TaskObservedBoundaryV1::ExploredScopeExit { .. } => {
                    RunPlayBoundaryKindV1::ExploredScopeExit
                }
                TaskObservedBoundaryV1::CommittedNextScope { .. } => {
                    RunPlayBoundaryKindV1::CommittedNextScope
                }
                TaskObservedBoundaryV1::CommittedGameTerminal { .. } => {
                    RunPlayBoundaryKindV1::CommittedGameTerminal
                }
            };
            Ok(RunPlayPathStepV1 {
                archive_node_id: node.node_id.clone(),
                depth: node.depth,
                site: incoming.site,
                before,
                combat_before,
                choice: incoming.step.choice.clone(),
                selected_candidate,
                semantic_action: Some(semantic_action),
                observed_boundary,
            })
        })
        .collect()
}

fn short_id(value: &str) -> &str {
    &value[value.len().saturating_sub(12)..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_play_keeps_one_decision_and_can_undo_without_source_identity() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = RunPlayPathsV1::new(directory.0.join("scratch"));
        let initial = create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        assert!(initial.head.scratch);
        assert!(initial.head.evidence_identity.is_none());
        TaskRunBranchCheckpointArtifactV1::selected_exact_state_for_reroot_v1(
            &initial.state.checkpoint_payload,
            &initial.state.archive,
            initial.head.position.archive_node_id(),
            DEFAULT_ARTIFACT_MAX_BYTES,
        )
        .expect("new scratch run starts with an exact recoverable decision");
        let initial_decision = initial
            .head
            .current_decision
            .unwrap()
            .public_decision_v1()
            .unwrap();

        let mut controller = RunPlayControllerV1::open(paths.clone()).unwrap();
        controller
            .choose_action_v1(
                &crate::selector::parse_action_selector_v1("event:Neow:0", "act").unwrap(),
                true,
            )
            .unwrap();
        let advanced = paths.load().unwrap();
        assert_eq!(advanced.state.archive.nodes.len(), 1);
        assert!(advanced.head.current_path.len() <= 1);
        assert_eq!(advanced.head.scratch_parent_revision, Some(0));
        assert!(RunPlayControllerV1::open(paths.clone()).is_ok());

        controller.back(1).unwrap();
        let restored = paths.load().unwrap();
        assert_eq!(
            restored.state.archive.nodes.len(),
            initial.state.archive.nodes.len()
        );
        assert_eq!(
            restored
                .head
                .current_decision
                .unwrap()
                .public_decision_v1()
                .unwrap(),
            initial_decision
        );
        assert!(restored.head.scratch_parent_revision.is_none());
    }

    #[test]
    fn scratch_records_map_only_after_neow_selection_is_resolved() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = RunPlayPathsV1::new(directory.0.join("neow-map"));
        create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        let mut controller = RunPlayControllerV1::open(paths.clone()).unwrap();
        controller
            .choose_action_v1(
                &crate::selector::parse_action_selector_v1("event:Neow:0", "act").unwrap(),
                true,
            )
            .unwrap();
        assert!(paths.load_head().unwrap().scratch_floor_ends.is_empty());
        for _ in 0..10 {
            let head = paths.load_head().unwrap();
            if head.current_decision.as_ref().unwrap().site == TaskDecisionSiteV1::Map {
                break;
            }
            controller
                .choose(
                    TaskPublicChoiceV1 {
                        ordinal_path: vec![0],
                    },
                    None,
                    true,
                )
                .unwrap();
        }
        let head = paths.load_head().unwrap();
        assert_eq!(
            head.current_decision.as_ref().unwrap().site,
            TaskDecisionSiteV1::Map
        );
        assert_eq!(head.scratch_floor_ends.len(), 1);
        assert_eq!(head.scratch_floor_ends[0].head_revision, head.head_revision);
    }

    #[test]
    fn scratch_retains_a_floor_end_when_transition_has_no_map_page() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = RunPlayPathsV1::new(directory.0.join("no-map-transition"));
        let previous = create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap()
        .head;
        let mut next = previous.clone();
        next.head_revision = 1;
        let prior_floor = scratch_public_floor_v1(&previous).unwrap();
        if let Some(TaskRunBranchDecisionInventoryV1 {
            observation: TaskPublicObservationV1::Strategic { observation },
            ..
        }) = &mut next.current_decision
        {
            observation.run.floor += 1;
        }
        assert_ne!(
            next.current_decision.as_ref().unwrap().site,
            TaskDecisionSiteV1::Map
        );
        record_scratch_landmarks_v1(&mut next, Some(&previous));
        assert_eq!(
            next.scratch_floor_ends,
            vec![RunPlayScratchFloorEndV1 {
                floor: prior_floor,
                head_revision: 1,
            }]
        );
    }

    #[test]
    fn scratch_keeps_more_than_twelve_decisions_within_a_floor() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = RunPlayPathsV1::new(directory.0.join("bounded-scratch"));
        create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 19,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        for revision in 1..=16 {
            let workspace = paths.load().unwrap();
            let mut head = workspace.head;
            let mut state = workspace.state;
            head.scratch_parent_revision = Some(head.head_revision);
            head.head_revision = revision;
            head.state_revision = revision;
            state.state_revision = revision;
            paths
                .publish(revision - 1, &head, Some(&state), None)
                .unwrap();
        }
        assert_eq!(
            fs::read_dir(paths.root().join("states")).unwrap().count(),
            17
        );
        assert_eq!(
            fs::read_dir(paths.root().join("heads")).unwrap().count(),
            17
        );
        assert_eq!(
            paths
                .load_recent_scratch_decision_v1(16)
                .unwrap()
                .head
                .head_revision,
            0
        );
        assert!(paths.load_recent_scratch_decision_v1(17).is_err());
        let head = paths.load_head().unwrap();
        let recent = paths.recent_scratch_heads_v1(&head, 12).unwrap();
        assert_eq!(recent.len(), 12);
        assert_eq!(recent.first().unwrap().head_revision, 15);
        assert_eq!(recent.last().unwrap().head_revision, 4);
        let targets = crate::compact::render_back_targets_v1(&paths).unwrap();
        assert!(targets.contains("  back 12 |"));
        assert!(!targets.contains("  back 13 |"));
        assert_eq!(paths.load_head().unwrap(), head);
    }

    #[test]
    fn scratch_keeps_floor_ends_and_discards_old_floor_interiors() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = RunPlayPathsV1::new(directory.0.join("floor-scratch"));
        create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 19,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        for revision in 1..=24 {
            let workspace = paths.load().unwrap();
            let mut head = workspace.head;
            let mut state = workspace.state;
            head.scratch_parent_revision = Some(head.head_revision);
            head.head_revision = revision;
            head.state_revision = revision;
            state.state_revision = revision;
            if matches!(revision, 1 | 8 | 16 | 24) {
                head.scratch_floor_ends.push(RunPlayScratchFloorEndV1 {
                    floor: (revision / 8) as i32,
                    head_revision: revision,
                });
            }
            paths
                .publish(revision - 1, &head, Some(&state), None)
                .unwrap();
        }
        assert!(!paths
            .root()
            .join("heads/head-00000000000000000009.json")
            .exists());
        assert!(paths
            .root()
            .join("heads/head-00000000000000000008.json")
            .exists());
        assert_eq!(
            fs::read_dir(paths.root().join("heads")).unwrap().count(),
            11
        );
        assert_eq!(
            fs::read_dir(paths.root().join("states")).unwrap().count(),
            11
        );
        assert_eq!(
            paths
                .load_recent_scratch_decision_v1(9)
                .unwrap()
                .head
                .head_revision,
            8
        );
        let restored = back_scratch_to_floor_v1(&paths, 1).unwrap();
        assert_eq!(
            restored
                .head
                .scratch_floor_ends
                .last()
                .unwrap()
                .head_revision,
            25
        );
        assert_eq!(
            paths
                .load_recent_scratch_decision_v1(1)
                .unwrap()
                .head
                .head_revision,
            1
        );
    }

    #[test]
    fn scratch_mark_survives_rewind_without_a_second_session() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = RunPlayPathsV1::new(directory.0.join("marked-scratch"));
        let initial = create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        save_scratch_mark_v1(&paths, "start").unwrap();
        let mut controller = RunPlayControllerV1::open(paths.clone()).unwrap();
        controller
            .choose_action_v1(
                &crate::selector::parse_action_selector_v1("event:Neow:0", "act").unwrap(),
                true,
            )
            .unwrap();
        let restored = back_scratch_to_mark_v1(&paths, "start").unwrap();
        assert_eq!(restored.head.scratch_marks.len(), 1);
        assert_eq!(restored.head.scratch_marks[0].head_revision, 0);
        assert_eq!(
            restored.head.current_decision,
            initial.head.current_decision
        );
        assert!(RunPlayControllerV1::open(paths.clone()).is_ok());
        assert!(!paths
            .root()
            .join("heads/head-00000000000000000002.json")
            .exists());
        drop_scratch_mark_v1(&paths, "start").unwrap();
        assert!(paths.load_head().unwrap().scratch_marks.is_empty());
        assert!(!paths
            .root()
            .join("heads/head-00000000000000000000.json")
            .exists());
    }

    #[test]
    fn read_only_historical_scratch_root_uses_retained_mark_without_moving_head() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = RunPlayPathsV1::new(directory.0.join("read-mark"));
        let initial = create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        let node = initial.head.position.archive_node_id().to_string();
        let selector = format!("n{node}");
        save_scratch_mark_v1(&paths, "start").unwrap();
        let mut controller = RunPlayControllerV1::open(paths.clone()).unwrap();
        controller
            .choose_action_v1(
                &crate::selector::parse_action_selector_v1("event:Neow:0", "act").unwrap(),
                true,
            )
            .unwrap();
        let before = paths.load_head().unwrap();
        assert!(paths.load().unwrap().state.archive.node(&node).is_none());
        let inspected = RunPlayControllerV1::open_at(paths.clone(), &selector).unwrap();
        assert_eq!(
            inspected.head.current_decision,
            initial.head.current_decision
        );
        assert_eq!(paths.load_head().unwrap(), before);
        assert_eq!(inspected.head.position.archive_node_id(), node);

        // The same root remains inspectable from a durable terminal head.
        let mut workspace = paths.load().unwrap();
        let mut head = workspace.head;
        let old_revision = head.head_revision;
        head.head_revision += 1;
        head.state_revision = head.head_revision;
        workspace.state.state_revision = head.state_revision;
        head.scratch_parent_revision = None;
        head.position = RunPlayPositionV1::Terminal {
            archive_node_id: head.position.archive_node_id().to_string(),
            outcome: TaskEpisodeOutcomeV1::Success {
                game_outcome: GameOutcomeV1::HeartVictory,
            },
        };
        head.current_decision = None;
        paths
            .publish(old_revision, &head, Some(&workspace.state), None)
            .unwrap();
        let terminal = paths.load_head().unwrap();
        assert!(RunPlayControllerV1::open_at(paths.clone(), &selector).is_ok());
        assert_eq!(paths.load_head().unwrap(), terminal);

        // An exact root from another session must never satisfy the lookup.
        let marked_path = paths.root().join("heads/head-00000000000000000000.json");
        let original_bytes = fs::read(&marked_path).unwrap();
        let mut foreign = initial.head.clone();
        foreign.session_id = "other-session".into();
        fs::write(&marked_path, serde_json::to_vec(&foreign).unwrap()).unwrap();
        let error = RunPlayControllerV1::open_at(paths.clone(), &selector)
            .err()
            .unwrap();
        assert!(error.contains("identity"));
        assert_eq!(paths.load_head().unwrap(), terminal);
        fs::write(&marked_path, original_bytes).unwrap();
        drop_scratch_mark_v1(&paths, "start").unwrap();
        assert!(RunPlayControllerV1::open_at(paths.clone(), &selector).is_err());
    }

    #[test]
    fn marked_alternative_keeps_its_own_floor_ends() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = RunPlayPathsV1::new(directory.0.join("marked-floors"));
        create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 19,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        for revision in 1..=3 {
            let workspace = paths.load().unwrap();
            let mut head = workspace.head;
            let mut state = workspace.state;
            head.scratch_parent_revision = Some(head.head_revision);
            head.head_revision = revision;
            head.state_revision = revision;
            state.state_revision = revision;
            if revision <= 2 {
                head.scratch_floor_ends.push(RunPlayScratchFloorEndV1 {
                    floor: revision as i32 - 1,
                    head_revision: revision,
                });
            }
            paths
                .publish(revision - 1, &head, Some(&state), None)
                .unwrap();
        }
        save_scratch_mark_v1(&paths, "later").unwrap();
        let before = paths.load_head().unwrap();
        // Listing saved facts must not need exact-state recovery or publish a
        // new head. Restoration still needs the checkpoint after it returns.
        let checkpoint = paths.root().join(format!(
            "states/state-{:020}.msgpack",
            before.state_revision
        ));
        let checkpoint_bytes = fs::read(&checkpoint).unwrap();
        fs::write(&checkpoint, b"not a decodable checkpoint").unwrap();
        let targets = crate::compact::render_back_targets_v1(&paths);
        fs::write(&checkpoint, checkpoint_bytes).unwrap();
        let targets = targets.unwrap();
        assert!(targets.contains("Retained floors (back floor <number>): 0, 1"));
        assert!(targets.contains("later"));
        assert!(targets.contains("Recent return points (back <steps>; up to 12):"));
        for steps in 1..=2 {
            assert!(
                targets.contains(&format!("  back {steps} | Act")),
                "{targets}"
            );
        }
        assert!(!targets.contains("  back 3 |"), "{targets}");
        let recent = paths.recent_scratch_heads_v1(&before, 12).unwrap();
        assert_eq!(recent.len(), 2);
        for (index, retained) in recent.iter().enumerate() {
            assert_eq!(
                paths
                    .load_recent_scratch_decision_v1(index + 1)
                    .unwrap()
                    .head,
                *retained,
                "listed step must resolve the actual back target"
            );
        }
        assert!(targets.contains("HP ") && targets.contains("Potions: none"));
        assert!(targets.contains("discards unmarked later floors"));
        assert_eq!(paths.load_head().unwrap(), before);
        back_scratch_to_floor_v1(&paths, 0).unwrap();
        assert!(paths
            .root()
            .join("heads/head-00000000000000000002.json")
            .exists());
        let restored = back_scratch_to_mark_v1(&paths, "later").unwrap();
        assert_eq!(restored.head.scratch_floor_ends.len(), 2);
        assert_eq!(restored.head.scratch_marks[0].name, "later");
    }

    #[test]
    fn reroot_verifies_exact_state_and_records_new_producer_without_relabeling_source() {
        let directory = crate::decision_tests::TestDirectory::new();
        let root = &directory.0;
        let source = RunPlayPathsV1::new(root.join("source"));
        create_natural_session_v1(
            &source,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 1,
                ascension_level: 0,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        let source_workspace = source.load().unwrap();
        let source_head = source.load_head().unwrap();
        let mut old_head = source_head.clone();
        old_head
            .evidence_identity
            .as_mut()
            .unwrap()
            .build_source
            .as_mut()
            .unwrap()
            .source_sha256 = "older-build".into();
        let old_source = RunPlayPathsV1::new(root.join("old-source"));
        old_source
            .create(&old_head, &source_workspace.state)
            .unwrap();
        assert!(RunPlayControllerV1::open(old_source.clone()).is_err());
        let snapshot = root.join("snapshot.msgpack");
        let mut capsule = snapshot_current_for_reroot_v1(old_source.clone(), &snapshot).unwrap();

        let target = RunPlayPathsV1::new(root.join("target"));
        let rerooted = reroot_session_from_capsule_v1(&snapshot, &target).unwrap();
        assert_eq!(
            rerooted.head.evidence_identity,
            Some(RunPlayEvidenceIdentityV1::current_v1())
        );
        assert_eq!(source.load_head().unwrap(), source_head);
        assert_eq!(old_source.load_head().unwrap(), old_head);
        let retained: RunPlayRerootCapsuleV1 =
            rmp_serde::from_slice(&fs::read(&snapshot).unwrap()).unwrap();
        assert_eq!(retained, capsule);
        assert_eq!(
            retained
                .source_evidence_identity
                .unwrap()
                .build_source
                .unwrap()
                .source_sha256,
            "older-build"
        );
        assert!(RunPlayControllerV1::open(target).is_ok());

        let direct_target = RunPlayPathsV1::new(root.join("direct-target"));
        let direct = reroot_current_session_v1(old_source.clone(), &direct_target, None).unwrap();
        assert_eq!(
            direct.head.evidence_identity,
            Some(RunPlayEvidenceIdentityV1::current_v1())
        );
        assert!(RunPlayControllerV1::open(direct_target).is_ok());
        assert_eq!(old_source.load_head().unwrap(), old_head);
        let redundant_target = RunPlayPathsV1::new(root.join("redundant-target"));
        assert!(reroot_current_session_v1(source.clone(), &redundant_target, None).is_err());
        assert!(!redundant_target.root().exists());

        let invalid_snapshot = root.join("invalid.msgpack");
        capsule.exact_state_fingerprint = "wrong-state".into();
        fs::write(
            &invalid_snapshot,
            rmp_serde::to_vec_named(&capsule).unwrap(),
        )
        .unwrap();
        let invalid_target = RunPlayPathsV1::new(root.join("invalid-target"));
        assert!(reroot_session_from_capsule_v1(&invalid_snapshot, &invalid_target).is_err());
        assert!(!invalid_target.root().exists());
    }

    #[test]
    fn historical_reroot_verifies_an_old_source_node_without_moving_its_head() {
        let directory = crate::decision_tests::TestDirectory::new();
        let root = &directory.0;
        let source = RunPlayPathsV1::new(root.join("historical-source"));
        create_natural_session_v1(
            &source,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 1,
                ascension_level: 0,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        let initial = source.load_head().unwrap();
        let mut controller = RunPlayControllerV1::open(source.clone()).unwrap();
        controller
            .choose_action_v1(
                &crate::selector::parse_action_selector_v1("event:Neow:0", "act").unwrap(),
                true,
            )
            .unwrap();
        let advanced = source.load().unwrap();
        let mut old_head = advanced.head.clone();
        old_head
            .evidence_identity
            .as_mut()
            .unwrap()
            .build_source
            .as_mut()
            .unwrap()
            .source_sha256 = "older-build".into();
        fs::write(
            source.latest_head_path_for_control_v1().unwrap(),
            serde_json::to_vec_pretty(&old_head).unwrap(),
        )
        .unwrap();

        let selector = format!("n{}", short_id(initial.position.archive_node_id()));
        let target = RunPlayPathsV1::new(root.join("historical-target"));
        let rerooted = reroot_current_session_v1(source.clone(), &target, Some(&selector))
            .expect("selected historical state and RNG verify in this build");
        assert_eq!(source.load_head().unwrap(), old_head);
        assert_eq!(
            rerooted.head.evidence_identity,
            Some(RunPlayEvidenceIdentityV1::current_v1())
        );
        assert_eq!(
            rerooted
                .head
                .current_decision
                .as_ref()
                .unwrap()
                .public_decision_v1()
                .unwrap(),
            initial
                .current_decision
                .as_ref()
                .unwrap()
                .public_decision_v1()
                .unwrap()
        );
        assert!(RunPlayControllerV1::open(target).is_ok());
    }

    #[test]
    fn build_source_gate_preserves_old_evidence_and_allows_current_execution() {
        let directory = crate::decision_tests::TestDirectory::new();
        let root = &directory.0;
        let paths = RunPlayPathsV1::new(root.join("current"));
        let workspace = create_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 1,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        let mut controller = RunPlayControllerV1::open(paths).unwrap();
        controller
            .choose_action_v1(
                &crate::selector::parse_action_selector_v1("event:Neow:0", "act").unwrap(),
                true,
            )
            .unwrap();
        assert!(controller.head().head_revision > workspace.head.head_revision);
        crate::public_decision::export_head_public_decision_v1(
            controller.head(),
            &root.join("current-export.json"),
        )
        .unwrap();

        for variant in ["missing_identity", "missing_source", "different_source"] {
            let old_paths = RunPlayPathsV1::new(root.join(variant));
            let mut old_head = workspace.head.clone();
            match variant {
                "missing_identity" => old_head.evidence_identity = None,
                "missing_source" => {
                    old_head.evidence_identity.as_mut().unwrap().build_source = None
                }
                _ => {
                    old_head
                        .evidence_identity
                        .as_mut()
                        .unwrap()
                        .build_source
                        .as_mut()
                        .unwrap()
                        .source_sha256 = "different-build".into()
                }
            }
            // Deliberately constructed old-identity fixture, never a live migration.
            old_paths.create(&old_head, &workspace.state).unwrap();
            let before = old_paths.load().unwrap();
            assert!(RunPlayControllerV1::open(old_paths.clone()).is_err());
            assert!(RunPlayControllerV1::open_at_archive_node_id_v1(
                old_paths.clone(),
                old_head.position.archive_node_id(),
            )
            .is_err());
            let output = root.join(format!("{variant}.json"));
            assert!(
                crate::public_decision::export_head_public_decision_v1(&before.head, &output,)
                    .is_err()
            );
            assert!(!output.exists());
            let retained = old_paths.load().unwrap();
            assert_eq!(retained.head, before.head);
            assert_eq!(retained.state, before.state);
            assert!(!crate::render::render_show_v1(&retained.head)
                .unwrap()
                .is_empty());
        }
    }

    #[test]
    fn controller_event_selector_requires_an_explicit_boundary() {
        let error = parse_controller_event_selector_v1("h17")
            .expect_err("bare head revision must not choose a direction");
        assert!(error.contains("directionally ambiguous"));
        assert_eq!(
            parse_controller_event_selector_v1("source:h17")
                .expect("source selector")
                .expect("controller event selector"),
            (RunPlayControllerEventBoundaryV1::Source, 17)
        );
        assert_eq!(
            parse_controller_event_selector_v1("target:h17")
                .expect("target selector")
                .expect("controller event selector"),
            (RunPlayControllerEventBoundaryV1::Target, 17)
        );
    }

    #[test]
    fn controller_event_boundaries_do_not_change_direction_by_event_kind() {
        let back = RunPlayEventV1::Back {
            head_revision: 17,
            from_archive_node_id: "from-back".to_string(),
            to_archive_node_id: "to-back".to_string(),
            steps: 1,
        };
        let jump = RunPlayEventV1::Jump {
            head_revision: 18,
            from_archive_node_id: "from-jump".to_string(),
            to_archive_node_id: "to-jump".to_string(),
            selector: "nsource".to_string(),
        };

        assert_eq!(
            controller_event_boundary_node_id_v1(&back, RunPlayControllerEventBoundaryV1::Source)
                .expect("back source"),
            "from-back"
        );
        assert_eq!(
            controller_event_boundary_node_id_v1(&back, RunPlayControllerEventBoundaryV1::Target)
                .expect("back target"),
            "to-back"
        );
        assert_eq!(
            controller_event_boundary_node_id_v1(&jump, RunPlayControllerEventBoundaryV1::Source)
                .expect("jump source"),
            "from-jump"
        );
        assert_eq!(
            controller_event_boundary_node_id_v1(&jump, RunPlayControllerEventBoundaryV1::Target)
                .expect("jump target"),
            "to-jump"
        );
    }

    #[test]
    fn combat_turn_progress_projection_keeps_only_public_bounded_facts() {
        let state = OracleCombatDeepStateSnapshot {
            player_turn: 5,
            player_hp: 17,
            player_block: 9,
            alive_enemy_count: 2,
            enemy_total_hp: 41,
            hand_size: 4,
            draw_pile_size: 8,
            discard_pile_size: 11,
            exhaust_pile_size: 6,
            path_atomic_depth: 23,
        };
        let internal = OracleCombatWitnessProgressSnapshot {
            retained_states: 99,
            max_player_turn: 5,
            deepest_survival_state: Some(state.clone()),
            deepest_progress_state: Some(state.clone()),
            deepest_survival_actions: Vec::new(),
            deepest_progress_actions: Vec::new(),
            recent_turn_survival_envelope: vec![state],
            ..OracleCombatWitnessProgressSnapshot::default()
        };

        let public = RunPlayCombatTurnContinuationProgressV1::from(&internal);
        assert_eq!(public.max_player_turn, 5);
        assert_eq!(public.deepest_survival_state.unwrap().player_hp, 17);
        assert_eq!(public.recent_turn_survival_envelope.len(), 1);
        assert!(!serde_json::to_string(&public)
            .expect("serialize public progress")
            .contains("retained_states"));
    }

    fn observed_terminal_outcome(
        candidate_id: &str,
        final_hp: i32,
        potion_expenditures: u32,
    ) -> RunPlayCombatTurnObservedTerminalOutcomeV1 {
        RunPlayCombatTurnObservedTerminalOutcomeV1 {
            candidate_id: candidate_id.to_string(),
            outcome: LocalTurnGraphTerminalOutcomeSnapshotV1::from_materialized_outcome_v1(
                sts_oracle_learning::eval::combat_witness_materialization::CombatWitnessMaterializedOutcomeV1 {
                    final_hp,
                    final_gold: 0,
                    persistent: sts_oracle_learning::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1 {
                        max_hp: 80,
                        recoverable_gold_delta: 0,
                        unrecovered_stolen_gold: 0,
                        ritual_dagger_value: 0,
                        genetic_algorithm_value: 0,
                        external_burden_count: 0,
                    },
                    starting_potion_expenditure_slots: u64::from(potion_expenditures > 0),
                    total_potion_expenditures: potion_expenditures,
                    generated_potions: Vec::new(),
                    final_relics: Vec::new(),
                    meta_changes: Vec::new(),
                    pending_rewards: Vec::new(),
                    combat_mugged: false,
                },
                12,
                0.0,
                false,
            ),
            public_outcome: TaskResearchPlanOutcomeV1 {
                final_hp,
                final_gold: 0,
                persistent: sts_oracle_learning::agent::task::TaskResearchPlanPersistentOutcomeV1 {
                    max_hp: 80,
                    recoverable_gold_delta: 0,
                    unrecovered_stolen_gold: 0,
                    ritual_dagger_value: 0,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                consumed_entry_potion_slots: if potion_expenditures > 0 {
                    vec![0]
                } else {
                    Vec::new()
                },
                total_potions_consumed: potion_expenditures,
                generated_potions: Vec::new(),
                final_relics: Vec::new(),
                meta_changes: Vec::new(),
                pending_rewards: Vec::new(),
                combat_mugged: false,
            },
        }
    }

    #[test]
    fn observed_combat_turn_frontier_drops_only_strictly_dominated_witnesses() {
        let mut frontier = Vec::new();
        retain_observed_combat_turn_terminal_outcome_v1(
            &mut frontier,
            observed_terminal_outcome("low", 14, 0),
        );
        retain_observed_combat_turn_terminal_outcome_v1(
            &mut frontier,
            observed_terminal_outcome("high", 50, 0),
        );
        assert_eq!(frontier.len(), 1);
        assert_eq!(frontier[0].candidate_id, "high");

        retain_observed_combat_turn_terminal_outcome_v1(
            &mut frontier,
            observed_terminal_outcome("potion-tradeoff", 60, 1),
        );
        assert_eq!(frontier.len(), 2);
    }

    fn horizon_level(
        additional_turn: usize,
        retained_candidate_count: usize,
        terminal_win_count: usize,
        terminal_loss_count: usize,
        unresolved_count: usize,
        other_count: usize,
    ) -> RunPlayCombatPrefixHorizonLevelV1 {
        RunPlayCombatPrefixHorizonLevelV1 {
            additional_turn,
            parent_count: 1,
            nodes_expanded: 0,
            nodes_generated: 0,
            engine_steps: 0,
            exact_state_skips: 0,
            truncated_children: 0,
            preselection_candidate_count: retained_candidate_count,
            retained_candidate_count,
            terminal_win_count,
            terminal_loss_count,
            unresolved_count,
            other_count,
            public_unresolved_successor_count: 0,
            public_successor_projection_truncated: false,
            public_unresolved_successors: Vec::new(),
        }
    }

    #[test]
    fn combat_prefix_horizon_preserves_a_win_found_before_the_final_level() {
        let levels = [
            horizon_level(1, 2, 1, 0, 1, 0),
            horizon_level(2, 1, 0, 1, 0, 0),
        ];

        assert_eq!(
            combat_prefix_horizon_status_v1(&levels),
            RunPlayCombatPrefixHorizonStatusV1::TerminalWinFound
        );
    }

    #[test]
    fn combat_prefix_horizon_does_not_hide_an_earlier_other_boundary() {
        let levels = [
            horizon_level(1, 2, 0, 0, 1, 1),
            horizon_level(2, 1, 0, 1, 0, 0),
        ];

        assert_eq!(
            combat_prefix_horizon_status_v1(&levels),
            RunPlayCombatPrefixHorizonStatusV1::UnknownInBoundedSurface
        );
    }

    #[test]
    fn combat_research_candidate_selection_requires_an_index_only_for_duplicates() {
        let producers = vec![vec!["resident"], vec!["resident"], vec!["proposal"]];

        assert_eq!(
            resolve_combat_research_candidate_index_v1(&producers, "proposal", None)
                .expect("unique proposal"),
            2
        );
        assert_eq!(
            resolve_combat_research_candidate_index_v1(&producers, "resident", Some(0))
                .expect("explicit resident"),
            0
        );

        let ambiguous = resolve_combat_research_candidate_index_v1(&producers, "resident", None)
            .expect_err("ambiguous resident must fail closed");
        assert!(ambiguous.contains("indices [0, 1]"));
        assert!(ambiguous.contains("--candidate <index>"));
        assert!(
            resolve_combat_research_candidate_index_v1(&producers, "proposal", Some(0)).is_err()
        );
        assert!(
            resolve_combat_research_candidate_index_v1(&producers, "resident", Some(3)).is_err()
        );

        let shared = vec![vec!["proposal", "resident"]];
        assert_eq!(
            resolve_combat_research_candidate_index_v1(&shared, "proposal", None)
                .expect("proposal support"),
            0
        );
        assert_eq!(
            resolve_combat_research_candidate_index_v1(&shared, "resident", None)
                .expect("resident support"),
            0
        );
    }

    #[test]
    fn combat_research_candidate_id_ignores_supporting_producers() {
        use sts_oracle_learning::agent::task::{
            TaskResearchPlanCandidateV1, TaskResearchPlanOutcomeV1,
            TaskResearchPlanPersistentOutcomeV1, TaskResearchPlanStepV1, TaskResearchPlanV1,
        };

        let provenance = |producer_name: &str| TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: producer_name.to_string(),
            producer_revision: 1,
        };
        let mut candidate = TaskResearchPlanCandidateV1 {
            plan: TaskResearchPlanV1 {
                root_public_snapshot_id: "root".to_string(),
                steps: vec![TaskResearchPlanStepV1 {
                    source_public_snapshot_id: "root".to_string(),
                    choice: TaskPublicChoiceV1::one(0),
                }],
            },
            outcome: TaskResearchPlanOutcomeV1 {
                final_hp: 52,
                final_gold: 0,
                persistent: TaskResearchPlanPersistentOutcomeV1 {
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
                final_relics: Vec::new(),
                meta_changes: Vec::new(),
                pending_rewards: Vec::new(),
                combat_mugged: false,
            },
            satisfies_requested_quality: false,
            supporting_provenances: vec![provenance("resident")],
        };
        let resident_id = combat_research_candidate_id_v1(&candidate).expect("resident id");
        candidate.supporting_provenances = vec![provenance("proposal"), provenance("resident")];
        candidate.supporting_provenances.sort();

        assert_eq!(
            combat_research_candidate_id_v1(&candidate).expect("shared id"),
            resident_id
        );
    }

    fn comparison_line(steps: usize) -> Vec<TaskPublicChoiceV1> {
        (0..steps)
            .map(|_| TaskPublicChoiceV1 {
                ordinal_path: vec![0],
            })
            .collect()
    }

    #[test]
    fn construction_candidate_card_filter_uses_typed_strategic_actions() {
        let iron_wave = TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::TakeCard {
                reward_item_index: Some(0),
                option_index: 1,
                card: CardId::IronWave,
                upgrades: 0,
            },
        };
        let skip = TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::SkipCardReward {
                reward_item_index: 0,
            },
        };

        assert!(construction_candidate_mentions_card_v1(
            &iron_wave,
            CardId::IronWave
        ));
        assert!(!construction_candidate_mentions_card_v1(
            &iron_wave,
            CardId::FireBreathing
        ));
        assert!(!construction_candidate_mentions_card_v1(
            &skip,
            CardId::IronWave
        ));
    }

    #[test]
    fn construction_added_card_filter_uses_deck_count_not_candidate_mentions() {
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
        let before = vec![card(0, CardId::Strike), card(1, CardId::RecklessCharge)];
        let after_same = vec![card(0, CardId::Strike), card(1, CardId::RecklessCharge)];
        let after_added = vec![
            card(0, CardId::Strike),
            card(1, CardId::RecklessCharge),
            card(2, CardId::RecklessCharge),
        ];

        assert!(!construction_deck_adds_card_v1(
            &before,
            &after_same,
            CardId::RecklessCharge
        ));
        assert!(construction_deck_adds_card_v1(
            &before,
            &after_added,
            CardId::RecklessCharge
        ));
        assert!(!construction_deck_adds_card_v1(
            &before,
            &after_added,
            CardId::TrueGrit
        ));
    }

    #[test]
    fn construction_ledger_omits_routine_transitions_but_keeps_durable_choices() {
        let candidate = |action| TaskPublicCandidateV1::Strategic { action };
        assert!(!construction_candidate_is_ledger_relevant_v1(&candidate(
            TaskStrategicActionV1::OpenCardReward {
                reward_item_index: 0,
            }
        )));
        assert!(!construction_candidate_is_ledger_relevant_v1(&candidate(
            TaskStrategicActionV1::ClaimReward {
                reward_item_index: 0,
                reward: PlannerRewardDescriptor::Gold { amount: 25 },
            }
        )));
        assert!(construction_candidate_is_ledger_relevant_v1(&candidate(
            TaskStrategicActionV1::ClaimReward {
                reward_item_index: 0,
                reward: PlannerRewardDescriptor::Relic {
                    relic: sts_oracle_learning::content::relics::RelicId::Anchor,
                },
            }
        )));
        assert!(construction_candidate_is_ledger_relevant_v1(&candidate(
            TaskStrategicActionV1::TakeCard {
                reward_item_index: Some(0),
                option_index: 0,
                card: CardId::IronWave,
                upgrades: 0,
            }
        )));
    }

    #[test]
    fn comparison_line_accepts_256_steps_and_rejects_257() {
        assert!(validate_comparison_lines_v1(&[comparison_line(256)]).is_ok());
        assert_eq!(
            validate_comparison_lines_v1(&[comparison_line(257)]).unwrap_err(),
            "run-play compare line 1 must contain 1..=256 public actions"
        );
    }

    #[test]
    fn anchor_line_accepts_256_steps_and_rejects_257() {
        assert!(validate_anchor_line_v1(&comparison_line(256)).is_ok());
        assert_eq!(
            validate_anchor_line_v1(&comparison_line(257)).unwrap_err(),
            "run-play anchor line must contain 1..=256 public actions"
        );
    }

    #[test]
    fn branch_promotion_selects_the_nearest_shared_archive_ancestor() {
        let source = ["root", "shared-a", "shared-b", "source-a", "source-b"].map(str::to_string);
        let target = ["root", "shared-a", "shared-b", "target-a"].map(str::to_string);

        assert_eq!(shared_archive_prefix_len_v1(&source, &target), 3);
        assert_eq!(source[2], "shared-b");
        assert_eq!(&source[3..], ["source-a", "source-b"]);
    }

    #[test]
    fn branch_promotion_rejects_unrelated_or_already_identical_paths() {
        let source = ["root-a", "source"].map(str::to_string);
        let unrelated = ["root-b", "target"].map(str::to_string);
        let identical = source.clone();

        assert_eq!(shared_archive_prefix_len_v1(&source, &unrelated), 0);
        assert_eq!(shared_archive_prefix_len_v1(&source, &identical), 2);
        assert!(source[shared_archive_prefix_len_v1(&source, &identical)..].is_empty());
    }

    #[test]
    fn terminal_choice_requires_full_state_consolidation() {
        let decision = RunPlayPositionV1::Decision {
            archive_node_id: "decision".to_string(),
        };
        let terminal = RunPlayPositionV1::Terminal {
            archive_node_id: "terminal".to_string(),
            outcome: TaskEpisodeOutcomeV1::Success {
                game_outcome: GameOutcomeV1::ActThreeVictory,
            },
        };

        assert!(can_publish_hot_choice_as_append_only_delta_v1(0, &decision));
        assert!(!can_publish_hot_choice_as_append_only_delta_v1(
            1, &decision
        ));
        assert!(!can_publish_hot_choice_as_append_only_delta_v1(
            0, &terminal
        ));
    }

    #[test]
    fn prepared_terminal_line_has_no_cached_decision() {
        let outcome = TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::ActThreeVictory,
        };
        let (position, current_decision) =
            prepared_line_endpoint_v1("terminal", None, Some(&outcome))
                .expect("valid prepared terminal endpoint");

        assert_eq!(
            position,
            RunPlayPositionV1::Terminal {
                archive_node_id: "terminal".to_string(),
                outcome,
            }
        );
        assert!(current_decision.is_none());
        assert!(prepared_line_endpoint_v1("invalid", None, None).is_err());
    }

    #[test]
    fn unsupported_automatic_continuation_hands_off_at_the_next_public_boundary() {
        let unsupported = TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::UnsupportedDecisionSurface,
            game_outcome: None,
        };

        assert!(should_replay_choice_to_public_handoff_v1(
            false,
            None,
            &unsupported
        ));
        assert!(!should_replay_choice_to_public_handoff_v1(
            true,
            None,
            &unsupported
        ));
        assert!(!should_replay_choice_to_public_handoff_v1(
            false,
            Some("published-successor"),
            &unsupported
        ));
    }

    #[test]
    fn public_handoff_does_not_mask_other_episode_failures() {
        for outcome in [
            TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::CommittedDefeat,
                game_outcome: Some(GameOutcomeV1::Defeat),
            },
            TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::TransitionBudgetExhausted,
                game_outcome: None,
            },
            TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::MechanicsOrProtocolError {
                    message: "invalid selected choice".to_string(),
                },
                game_outcome: None,
            },
        ] {
            assert!(!should_replay_choice_to_public_handoff_v1(
                false, None, &outcome
            ));
        }
    }

    #[test]
    fn fixed_turn_potion_contract_detects_only_strict_total_overrun() {
        assert!(!fixed_turn_exceeds_total_potion_contract_v1(3, None));
        assert!(!fixed_turn_exceeds_total_potion_contract_v1(1, Some(1)));
        assert!(fixed_turn_exceeds_total_potion_contract_v1(2, Some(1)));
    }
}
