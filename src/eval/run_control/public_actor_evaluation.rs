//! Compact complete-run evidence for the public structured actor.
//!
//! The receipt deliberately retains only task/evaluator identity, typed
//! outcomes, hard-budget accounting, public decision context, and structured
//! proposal event counts. It is not a trajectory or a capability judgment.

use std::{collections::BTreeSet, time::Instant};

use serde::{Deserialize, Serialize};

use crate::agent::information::action::PublicCombatAtomicActionV1;
use crate::agent::information::run::PublicCombatRunContextV1;
use crate::agent::information::state::CombatLearningEnemyIdentityV1;
use crate::agent::task::{
    run_task_actor_episode_v1, run_task_privileged_support_actor_episode_capturing_fresh_scopes_v1,
    run_task_privileged_support_actor_episode_v1, CommittedPublicHistoryV1, GameOutcomeV1,
    PublicCombatPlanFollowerEventV1, PublicCombatPlanGenerationV1, PublicCombatProposalGroundingV1,
    TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorViewV1, TaskBudgetStateV1, TaskBudgetV1,
    TaskCommittedScopeExitV1, TaskDecisionSiteV1, TaskEpisodeFailureV1, TaskEpisodeOutcomeV1,
    TaskObjectiveV1, TaskObservedBoundaryV1, TaskPrivilegedStrategicSupportV1,
    TaskPublicCandidateV1, TaskPublicDecisionV1, TaskPublicObservationV1, TaskRunSolutionArchiveV1,
    TaskScopeOrdinalV1, TaskSpecV1, TaskStrategicActionV1,
};
use crate::ai::planner_core::{
    stable_planner_id, PlannerDecisionContext, PlannerPublicMap, PlannerPublicMapNode,
};
use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;
use crate::content::monsters::EnemyId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::eval::combat_case::CombatCase;
use crate::runtime::combat::Intent;

use super::{
    ironclad_a0_native_autosave_environment_v1, ironclad_a0_privileged_reference_environment_v1,
    ironclad_a0_production_combat_case_privileged_reference_environment_v1,
    ironclad_native_autosave_environment_v1, privileged_reference_task_actor_v1,
    public_rule_search_task_actor_v1, public_structured_deterministic_task_actor_v1,
    CombatResearchPlanAttemptReceiptV1, CombatResearchPlanOptionsV1,
    ExactBranchingTaskEvidenceRequestV1, ExactBranchingTaskExecutionEvidenceV1,
    PublicDeterministicTaskActorV1, PublicRuleSearchStatsV1, TaskRunBranchCheckpointArtifactV1,
};

pub const PUBLIC_ACTOR_EPISODE_RECEIPT_SCHEMA_V9: &str = "sts-public-actor-episode-receipt-v9";
pub const PUBLIC_ACTOR_CENSUS_SCHEMA_V9: &str = "sts-public-actor-census-v9";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrivilegedReferenceEpisodeEvidenceRequestV1 {
    pub execution: ExactBranchingTaskEvidenceRequestV1,
    pub capture_all_run_branch_points: bool,
    pub pause_at_requested_combat_entry: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorBudgetReceiptV1 {
    pub requested: TaskBudgetV1,
    pub remaining: TaskBudgetStateV1,
    pub world_transitions_used: u64,
    pub reloads_used: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorControlReceiptV1 {
    pub actor_turns: u64,
    pub act_intents: u64,
    pub reload_intents: u32,
    pub commit_intents: u64,
    pub unsupported_reason: Option<String>,
    pub committed_public_steps: usize,
    pub committed_scopes: usize,
    pub committed_history_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum PublicActorDecisionContextV1 {
    Strategic {
        act: u8,
        floor: i32,
        context: PlannerDecisionContext,
    },
    Combat {
        act: Option<u8>,
        floor: Option<i32>,
        encounter_id: Option<EncounterId>,
        turn: u32,
        is_elite: bool,
        is_boss: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorLastDecisionV1 {
    pub site: TaskDecisionSiteV1,
    pub public_snapshot_id: String,
    pub context: PublicActorDecisionContextV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorStrategicDeckCardV1 {
    pub card: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorStrategicStateV1 {
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub visible_boss: Option<EncounterId>,
    pub deck: Vec<PublicActorStrategicDeckCardV1>,
    pub relics: Vec<RelicId>,
    pub potions: Vec<Option<PotionId>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorStrategicCandidateV1 {
    /// Ordinal in the original public candidate surface. Keeping it beside the
    /// projected action prevents receipt consumers from treating a compacted
    /// vector index as an executable public choice.
    pub candidate_ordinal: usize,
    pub action: TaskStrategicActionV1,
}

/// Compact projection of an already committed strategic choice. The task
/// environment keeps the complete public history; receipts retain this narrow
/// ledger so card rewards, shops, events, and deck mutations remain auditable
/// without serializing every combat decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorStrategicDecisionV1 {
    pub scope: TaskScopeOrdinalV1,
    pub site: TaskDecisionSiteV1,
    pub public_snapshot_id: String,
    pub context: PlannerDecisionContext,
    pub before: PublicActorStrategicStateV1,
    /// Links a map choice to the exact topology visible at that choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_graph_id: Option<String>,
    pub candidates: Vec<PublicActorStrategicCandidateV1>,
    pub selected_candidate_ordinal: usize,
    pub selected: TaskStrategicActionV1,
}

/// One actor-visible topology, stored once even when several map choices share it.
/// Current position is excluded from graph identity; each decision retains its
/// candidate coordinates and source snapshot identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorMapGraphV1 {
    pub id: String,
    pub act: u8,
    pub boss: Option<EncounterId>,
    pub nodes: Vec<PlannerPublicMapNode>,
}

/// Only committed public actions are projected here. HP on an action is the
/// value visible *before* it; a terminal action has no invented after-state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorCombatActionV1 {
    pub turn: u32,
    pub hp_before: i32,
    pub block_before: i32,
    pub energy_before: u8,
    pub action: PublicActorCombatActionKindV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorCombatHandCardV1 {
    pub card: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorCombatEnemyV1 {
    pub slot: u8,
    pub enemy: EnemyId,
    pub hp: i32,
    pub block: i32,
    pub alive: bool,
    pub intent: Option<Intent>,
    pub preview_damage_per_hit: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorCombatTurnV1 {
    pub turn: u32,
    pub hp: i32,
    pub block: i32,
    pub energy: u8,
    pub hand: Vec<PublicActorCombatHandCardV1>,
    pub enemies: Vec<PublicActorCombatEnemyV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicActorCombatActionKindV1 {
    PlayCard {
        card: CardId,
        upgrades: u8,
        target_monster_index: Option<usize>,
    },
    UsePotion {
        slot: usize,
        potion: PotionId,
        target_monster_index: Option<usize>,
        hand_before: Vec<PublicActorCombatHandCardV1>,
    },
    DiscardPotion {
        slot: usize,
        potion: PotionId,
    },
    EndTurn,
    Other {
        candidate_ordinal: usize,
        ordinal_path: Vec<usize>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicActorCombatExitV1 {
    /// The next public decision may already include post-combat healing.
    PublicDecision {
        site: TaskDecisionSiteV1,
        /// HP at the next public page; it may include post-combat healing.
        hp: i32,
        potions: Vec<Option<PotionId>>,
        recent_combat_hp_loss: Option<i32>,
        recent_combat_turns: Option<u32>,
        recent_combat_potions_used: Option<u32>,
    },
    GameTerminal {
        outcome: GameOutcomeV1,
    },
    NoObservedSuccessor,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorCombatTraceV1 {
    pub scope: TaskScopeOrdinalV1,
    pub act: Option<u8>,
    pub floor: Option<i32>,
    pub encounter_id: Option<EncounterId>,
    pub is_elite: bool,
    pub is_boss: bool,
    pub entry_hp: i32,
    pub entry_potions: Vec<Option<PotionId>>,
    /// Earlier V9 combat ledgers did not retain turn-opening observations.
    #[serde(default)]
    pub turns: Vec<PublicActorCombatTurnV1>,
    pub actions: Vec<PublicActorCombatActionV1>,
    pub exit: PublicActorCombatExitV1,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatPlanEventCountsV1 {
    pub generated: u64,
    pub generation_unique_grounded: u64,
    pub generation_ambiguous: u64,
    pub generation_unknown_coverage: u64,
    pub generation_inapplicable: u64,
    pub generation_invalidated: u64,
    pub steps_issued: u64,
    pub successor_groundings: u64,
    pub successor_grounded: u64,
    pub successor_ambiguous: u64,
    pub successor_unknown_coverage: u64,
    pub successor_inapplicable: u64,
    pub successor_invalidated: u64,
    pub successor_completed: u64,
    pub advance_failures: u64,
    pub interrupted_at_commit_ready: u64,
    pub interrupted_at_scope_change: u64,
}

impl PublicCombatPlanEventCountsV1 {
    pub fn from_events(events: &[PublicCombatPlanFollowerEventV1]) -> Self {
        let mut counts = Self::default();
        for event in events {
            match event {
                PublicCombatPlanFollowerEventV1::Generated { generation } => {
                    counts.generated += 1;
                    match generation {
                        PublicCombatPlanGenerationV1::UniqueGrounded { .. } => {
                            counts.generation_unique_grounded += 1
                        }
                        PublicCombatPlanGenerationV1::Ambiguous { .. } => {
                            counts.generation_ambiguous += 1
                        }
                        PublicCombatPlanGenerationV1::UnknownCoverage { .. } => {
                            counts.generation_unknown_coverage += 1
                        }
                        PublicCombatPlanGenerationV1::Inapplicable { .. } => {
                            counts.generation_inapplicable += 1
                        }
                        PublicCombatPlanGenerationV1::Invalidated { .. } => {
                            counts.generation_invalidated += 1
                        }
                    }
                }
                PublicCombatPlanFollowerEventV1::StepIssued { .. } => counts.steps_issued += 1,
                PublicCombatPlanFollowerEventV1::SuccessorGrounded { grounding, .. } => {
                    counts.successor_groundings += 1;
                    match grounding {
                        PublicCombatProposalGroundingV1::Grounded { .. } => {
                            counts.successor_grounded += 1
                        }
                        PublicCombatProposalGroundingV1::Ambiguous { .. } => {
                            counts.successor_ambiguous += 1
                        }
                        PublicCombatProposalGroundingV1::Inapplicable { .. } => {
                            counts.successor_inapplicable += 1
                        }
                        PublicCombatProposalGroundingV1::UnknownCoverage { .. } => {
                            counts.successor_unknown_coverage += 1
                        }
                        PublicCombatProposalGroundingV1::Invalidated { .. } => {
                            counts.successor_invalidated += 1
                        }
                        PublicCombatProposalGroundingV1::Completed { .. } => {
                            counts.successor_completed += 1
                        }
                    }
                }
                PublicCombatPlanFollowerEventV1::AdvanceFailed { .. } => {
                    counts.advance_failures += 1
                }
                PublicCombatPlanFollowerEventV1::InterruptedAtCommitReady { .. } => {
                    counts.interrupted_at_commit_ready += 1
                }
                PublicCombatPlanFollowerEventV1::InterruptedAtScopeChange { .. } => {
                    counts.interrupted_at_scope_change += 1
                }
            }
        }
        counts
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorEpisodeReceiptV9 {
    pub schema: String,
    pub seed: u64,
    pub task: TaskSpecV1,
    pub actor: TaskActorIdentityV1,
    pub outcome: TaskEpisodeOutcomeV1,
    pub budget: PublicActorBudgetReceiptV1,
    pub control: PublicActorControlReceiptV1,
    pub strategic_decisions: Vec<PublicActorStrategicDecisionV1>,
    /// Absent in earlier V9 artifacts. Every new map decision refers to one
    /// of these public graphs; no unrevealed outcomes are added.
    #[serde(default)]
    pub map_graphs: Vec<PublicActorMapGraphV1>,
    /// Absent in earlier V9 artifacts; an empty ledger does not imply there
    /// were no combats when reading such an artifact.
    #[serde(default)]
    pub combat_traces: Vec<PublicActorCombatTraceV1>,
    pub last_public_decision: Option<PublicActorLastDecisionV1>,
    pub combat_plan_events: PublicCombatPlanEventCountsV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_search: Option<PublicRuleSearchStatsV1>,
    pub exact_branch_selections: usize,
    pub exact_branch_trials: usize,
    pub strategic_support: Vec<TaskPrivilegedStrategicSupportV1>,
    pub combat_research_plans_installed: usize,
    pub combat_research_plans_unavailable: usize,
    pub combat_research_plan_actions: usize,
    pub combat_research_plan_attempts: Vec<CombatResearchPlanAttemptReceiptV1>,
    pub combat_research_plan_options: Option<CombatResearchPlanOptionsV1>,
    pub wall_time_millis: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorCensusSummaryV1 {
    pub episodes: usize,
    pub successes: usize,
    pub paused: usize,
    pub committed_defeats: usize,
    pub committed_terminal_without_objective: usize,
    pub retry_budget_exhausted: usize,
    pub transition_budget_exhausted: usize,
    pub unsupported_decision_surface: usize,
    pub mechanics_or_protocol_errors: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorExecutionEnvironmentV1 {
    pub package_version: String,
    pub target_os: String,
    pub target_arch: String,
    pub cargo_profile: String,
    pub debug_assertions: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActorCensusV9 {
    pub schema: String,
    /// Added after the original V8 receipts. `None` means a legacy artifact
    /// whose execution profile was not recorded and must not be inferred.
    #[serde(default)]
    pub execution_environment: Option<PublicActorExecutionEnvironmentV1>,
    pub requested_budget: TaskBudgetV1,
    pub requested_seeds: Vec<u64>,
    pub summary: PublicActorCensusSummaryV1,
    pub episodes: Vec<PublicActorEpisodeReceiptV9>,
}

fn current_public_actor_execution_environment_v1() -> PublicActorExecutionEnvironmentV1 {
    PublicActorExecutionEnvironmentV1 {
        package_version: env!("CARGO_PKG_VERSION").to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        cargo_profile: env!("STS_CARGO_PROFILE").to_string(),
        debug_assertions: cfg!(debug_assertions),
    }
}

pub fn evaluate_public_structured_actor_episode_v9(
    seed: u64,
    budget: TaskBudgetV1,
) -> Result<PublicActorEpisodeReceiptV9, String> {
    evaluate_public_structured_actor_episode_for_task_v9(
        seed,
        0,
        TaskObjectiveV1::ActThreeVictory,
        budget,
    )
}

pub fn evaluate_public_structured_actor_episode_for_task_v9(
    seed: u64,
    ascension_level: u8,
    objective: TaskObjectiveV1,
    budget: TaskBudgetV1,
) -> Result<PublicActorEpisodeReceiptV9, String> {
    budget.validate()?;
    let started = Instant::now();
    let mut environment =
        ironclad_native_autosave_environment_v1(seed, ascension_level, objective, budget)?;
    let mut actor = public_structured_deterministic_task_actor_v1();
    let episode = run_task_actor_episode_v1(&mut environment, &mut actor)
        .map_err(|error| error.to_string())?;
    episode_receipt(
        seed,
        budget,
        episode,
        PublicCombatPlanEventCountsV1::from_events(actor.events()),
        0,
        0,
        Vec::new(),
        0,
        0,
        0,
        Vec::new(),
        None,
        started,
    )
}

pub fn evaluate_public_rule_search_actor_episode_for_task_v9(
    seed: u64,
    ascension_level: u8,
    objective: TaskObjectiveV1,
    budget: TaskBudgetV1,
) -> Result<PublicActorEpisodeReceiptV9, String> {
    budget.validate()?;
    let started = Instant::now();
    let mut environment =
        ironclad_native_autosave_environment_v1(seed, ascension_level, objective, budget)?;
    let mut actor = public_rule_search_task_actor_v1();
    let episode = run_task_actor_episode_v1(&mut environment, &mut actor)
        .map_err(|error| error.to_string())?;
    let mut receipt = episode_receipt(
        seed,
        budget,
        episode,
        PublicCombatPlanEventCountsV1::from_events(actor.events()),
        0,
        0,
        Vec::new(),
        0,
        0,
        0,
        Vec::new(),
        None,
        started,
    )?;
    receipt.rule_search = Some(actor.fallback().stats().clone());
    Ok(receipt)
}

pub fn evaluate_public_deterministic_actor_episode_v9(
    seed: u64,
    budget: TaskBudgetV1,
) -> Result<PublicActorEpisodeReceiptV9, String> {
    budget.validate()?;
    let started = Instant::now();
    let mut environment = ironclad_a0_native_autosave_environment_v1(seed, budget)?;
    let mut actor = PublicDeterministicTaskActorV1::default();
    let episode = run_task_actor_episode_v1(&mut environment, &mut actor)
        .map_err(|error| error.to_string())?;
    episode_receipt(
        seed,
        budget,
        episode,
        PublicCombatPlanEventCountsV1::default(),
        0,
        0,
        Vec::new(),
        0,
        0,
        0,
        Vec::new(),
        None,
        started,
    )
}

pub fn evaluate_privileged_reference_actor_episode_v9(
    seed: u64,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
) -> Result<PublicActorEpisodeReceiptV9, String> {
    evaluate_privileged_reference_actor_episode_with_evidence_v9(
        seed,
        budget,
        plan_options,
        PrivilegedReferenceEpisodeEvidenceRequestV1::default(),
    )
    .map(|result| result.receipt)
}

pub struct PrivilegedReferenceEpisodeEvidenceV1 {
    pub receipt: PublicActorEpisodeReceiptV9,
    pub execution: ExactBranchingTaskExecutionEvidenceV1,
    pub run_solution_archive: Option<TaskRunSolutionArchiveV1>,
    pub run_branch_checkpoints: Option<TaskRunBranchCheckpointArtifactV1>,
}

pub struct PrivilegedReferenceCensusEvidenceV1 {
    pub census: PublicActorCensusV9,
    pub execution: ExactBranchingTaskExecutionEvidenceV1,
    pub run_solution_archive: Option<TaskRunSolutionArchiveV1>,
    pub run_branch_checkpoints: Option<TaskRunBranchCheckpointArtifactV1>,
}

pub fn evaluate_privileged_reference_actor_episode_with_evidence_v9(
    seed: u64,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: PrivilegedReferenceEpisodeEvidenceRequestV1,
) -> Result<PrivilegedReferenceEpisodeEvidenceV1, String> {
    budget.validate()?;
    let started = Instant::now();
    let capture_run_branch_points = evidence_request.capture_all_run_branch_points;
    let mut environment = ironclad_a0_privileged_reference_environment_v1(
        seed,
        budget,
        plan_options.clone(),
        evidence_request.execution,
    )?;
    let mut actor = match (
        evidence_request.pause_at_requested_combat_entry,
        evidence_request.execution.combat_entry_encounter,
    ) {
        (true, Some(encounter_id)) => {
            privileged_reference_task_actor_v1().yielding_at_combat_entry_v1(encounter_id)
        }
        (true, None) => {
            return Err("combat-entry evidence pause requires a requested encounter".to_string())
        }
        (false, _) => privileged_reference_task_actor_v1(),
    };
    let (episode, run_branch_points) = if capture_run_branch_points {
        run_task_privileged_support_actor_episode_capturing_fresh_scopes_v1(
            &mut environment,
            &mut actor,
            |point| Ok::<_, String>(point.map_anchor(|anchor| anchor.durable_checkpoint_v1())),
        )
        .map_err(|error| error.to_string())?
    } else {
        (
            run_task_privileged_support_actor_episode_v1(&mut environment, &mut actor)
                .map_err(|error| error.to_string())?,
            Vec::new(),
        )
    };
    let execution_manifest_id = stable_planner_id(
        "privileged_reference_run_execution_manifest_v1",
        &(
            &episode.actor,
            &episode.final_view.task,
            budget,
            &plan_options,
        ),
    )?;
    let mut run_solution_archive = TaskRunSolutionArchiveV1::new(episode.final_view.task.clone())?;
    run_solution_archive.record_episode(seed, execution_manifest_id, budget, &episode)?;
    let run_branch_checkpoints = capture_run_branch_points
        .then(|| {
            TaskRunBranchCheckpointArtifactV1::from_branch_points(
                &run_solution_archive,
                seed,
                run_branch_points,
            )
        })
        .transpose()?;
    let execution = environment
        .take_execution_evidence()
        .materialize_committed_combats(&episode.final_view.committed_history)?;
    let receipt = episode_receipt(
        seed,
        budget,
        episode,
        PublicCombatPlanEventCountsV1::default(),
        0,
        0,
        actor.strategic_support().to_vec(),
        actor.installed_plans(),
        actor.unavailable_plans(),
        actor.planned_actions(),
        actor.attempts().to_vec(),
        Some(plan_options),
        started,
    )?;
    Ok(PrivilegedReferenceEpisodeEvidenceV1 {
        receipt,
        execution,
        run_solution_archive: Some(run_solution_archive),
        run_branch_checkpoints,
    })
}

pub fn evaluate_privileged_reference_actor_combat_case_episode_v9(
    case: &CombatCase,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
) -> Result<PublicActorEpisodeReceiptV9, String> {
    evaluate_privileged_reference_actor_combat_case_episode_with_evidence_v9(
        case,
        budget,
        plan_options,
        PrivilegedReferenceEpisodeEvidenceRequestV1::default(),
    )
    .map(|result| result.receipt)
}

pub fn evaluate_privileged_reference_actor_combat_case_episode_with_evidence_v9(
    case: &CombatCase,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: PrivilegedReferenceEpisodeEvidenceRequestV1,
) -> Result<PrivilegedReferenceEpisodeEvidenceV1, String> {
    budget.validate()?;
    if evidence_request.capture_all_run_branch_points {
        return Err(
            "run branch checkpoint capture requires a natural new-game episode".to_string(),
        );
    }
    let started = Instant::now();
    let mut environment = ironclad_a0_production_combat_case_privileged_reference_environment_v1(
        case,
        budget,
        plan_options.clone(),
        evidence_request.execution,
    )?;
    let mut actor = match (
        evidence_request.pause_at_requested_combat_entry,
        evidence_request.execution.combat_entry_encounter,
    ) {
        (true, Some(encounter_id)) => {
            privileged_reference_task_actor_v1().yielding_at_combat_entry_v1(encounter_id)
        }
        (true, None) => {
            return Err("combat-entry evidence pause requires a requested encounter".to_string())
        }
        (false, _) => privileged_reference_task_actor_v1(),
    };
    let episode = run_task_privileged_support_actor_episode_v1(&mut environment, &mut actor)
        .map_err(|error| error.to_string())?;
    let execution = environment
        .take_execution_evidence()
        .materialize_committed_combats(&episode.final_view.committed_history)?;
    let receipt = episode_receipt(
        case.core.source.seed,
        budget,
        episode,
        PublicCombatPlanEventCountsV1::default(),
        0,
        0,
        actor.strategic_support().to_vec(),
        actor.installed_plans(),
        actor.unavailable_plans(),
        actor.planned_actions(),
        actor.attempts().to_vec(),
        Some(plan_options),
        started,
    )?;
    Ok(PrivilegedReferenceEpisodeEvidenceV1 {
        receipt,
        execution,
        run_solution_archive: None,
        run_branch_checkpoints: None,
    })
}

pub fn evaluate_privileged_reference_actor_combat_case_census_v9(
    case: &CombatCase,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
) -> Result<PublicActorCensusV9, String> {
    let episode =
        evaluate_privileged_reference_actor_combat_case_episode_v9(case, budget, plan_options)?;
    let episodes = vec![episode];
    Ok(PublicActorCensusV9 {
        schema: PUBLIC_ACTOR_CENSUS_SCHEMA_V9.to_string(),
        execution_environment: Some(current_public_actor_execution_environment_v1()),
        requested_budget: budget,
        requested_seeds: vec![case.core.source.seed],
        summary: summarize(&episodes),
        episodes,
    })
}

pub fn evaluate_privileged_reference_actor_combat_case_census_with_evidence_v9(
    case: &CombatCase,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: PrivilegedReferenceEpisodeEvidenceRequestV1,
) -> Result<PrivilegedReferenceCensusEvidenceV1, String> {
    let result = evaluate_privileged_reference_actor_combat_case_episode_with_evidence_v9(
        case,
        budget,
        plan_options,
        evidence_request,
    )?;
    let episodes = vec![result.receipt];
    let census = PublicActorCensusV9 {
        schema: PUBLIC_ACTOR_CENSUS_SCHEMA_V9.to_string(),
        execution_environment: Some(current_public_actor_execution_environment_v1()),
        requested_budget: budget,
        requested_seeds: vec![case.core.source.seed],
        summary: summarize(&episodes),
        episodes,
    };
    Ok(PrivilegedReferenceCensusEvidenceV1 {
        census,
        execution: result.execution,
        run_solution_archive: result.run_solution_archive,
        run_branch_checkpoints: result.run_branch_checkpoints,
    })
}

pub fn evaluate_privileged_reference_actor_census_with_evidence_v9(
    seed: u64,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: PrivilegedReferenceEpisodeEvidenceRequestV1,
) -> Result<PrivilegedReferenceCensusEvidenceV1, String> {
    let result = evaluate_privileged_reference_actor_episode_with_evidence_v9(
        seed,
        budget,
        plan_options,
        evidence_request,
    )?;
    let episodes = vec![result.receipt];
    let census = PublicActorCensusV9 {
        schema: PUBLIC_ACTOR_CENSUS_SCHEMA_V9.to_string(),
        execution_environment: Some(current_public_actor_execution_environment_v1()),
        requested_budget: budget,
        requested_seeds: vec![seed],
        summary: summarize(&episodes),
        episodes,
    };
    Ok(PrivilegedReferenceCensusEvidenceV1 {
        census,
        execution: result.execution,
        run_solution_archive: result.run_solution_archive,
        run_branch_checkpoints: result.run_branch_checkpoints,
    })
}

pub fn evaluate_public_structured_actor_census_v9(
    seeds: &[u64],
    budget: TaskBudgetV1,
) -> Result<PublicActorCensusV9, String> {
    evaluate_census(seeds, budget, evaluate_public_structured_actor_episode_v9)
}

pub fn evaluate_public_structured_actor_census_for_task_v9(
    seeds: &[u64],
    ascension_level: u8,
    objective: TaskObjectiveV1,
    budget: TaskBudgetV1,
) -> Result<PublicActorCensusV9, String> {
    evaluate_census(seeds, budget, |seed, budget| {
        evaluate_public_structured_actor_episode_for_task_v9(
            seed,
            ascension_level,
            objective,
            budget,
        )
    })
}

pub fn evaluate_public_rule_search_actor_census_for_task_v9(
    seeds: &[u64],
    ascension_level: u8,
    objective: TaskObjectiveV1,
    budget: TaskBudgetV1,
) -> Result<PublicActorCensusV9, String> {
    evaluate_census(seeds, budget, |seed, budget| {
        evaluate_public_rule_search_actor_episode_for_task_v9(
            seed,
            ascension_level,
            objective,
            budget,
        )
    })
}

pub fn evaluate_public_deterministic_actor_census_v9(
    seeds: &[u64],
    budget: TaskBudgetV1,
) -> Result<PublicActorCensusV9, String> {
    evaluate_census(
        seeds,
        budget,
        evaluate_public_deterministic_actor_episode_v9,
    )
}

pub fn evaluate_privileged_reference_actor_census_v9(
    seeds: &[u64],
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
) -> Result<PublicActorCensusV9, String> {
    if seeds.is_empty() {
        return Err("public actor census requires at least one seed".to_string());
    }
    budget.validate()?;
    let episodes = seeds
        .iter()
        .copied()
        .map(|seed| {
            evaluate_privileged_reference_actor_episode_v9(seed, budget, plan_options.clone())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summarize(&episodes);
    Ok(PublicActorCensusV9 {
        schema: PUBLIC_ACTOR_CENSUS_SCHEMA_V9.to_string(),
        execution_environment: Some(current_public_actor_execution_environment_v1()),
        requested_budget: budget,
        requested_seeds: seeds.to_vec(),
        summary,
        episodes,
    })
}

fn evaluate_census(
    seeds: &[u64],
    budget: TaskBudgetV1,
    evaluate: impl Fn(u64, TaskBudgetV1) -> Result<PublicActorEpisodeReceiptV9, String>,
) -> Result<PublicActorCensusV9, String> {
    if seeds.is_empty() {
        return Err("public actor census requires at least one seed".to_string());
    }
    budget.validate()?;
    let episodes = seeds
        .iter()
        .copied()
        .map(|seed| evaluate(seed, budget))
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summarize(&episodes);
    Ok(PublicActorCensusV9 {
        schema: PUBLIC_ACTOR_CENSUS_SCHEMA_V9.to_string(),
        execution_environment: Some(current_public_actor_execution_environment_v1()),
        requested_budget: budget,
        requested_seeds: seeds.to_vec(),
        summary,
        episodes,
    })
}

fn episode_receipt(
    seed: u64,
    budget: TaskBudgetV1,
    episode: crate::agent::task::TaskActorEpisodeV1,
    combat_plan_events: PublicCombatPlanEventCountsV1,
    exact_branch_selections: usize,
    exact_branch_trials: usize,
    strategic_support: Vec<TaskPrivilegedStrategicSupportV1>,
    combat_research_plans_installed: usize,
    combat_research_plans_unavailable: usize,
    combat_research_plan_actions: usize,
    combat_research_plan_attempts: Vec<CombatResearchPlanAttemptReceiptV1>,
    combat_research_plan_options: Option<CombatResearchPlanOptionsV1>,
    started: Instant,
) -> Result<PublicActorEpisodeReceiptV9, String> {
    let remaining = episode.final_view.budget;
    let last_public_decision = last_public_decision(&episode.final_view).map(decision_context);
    let committed_history_id = stable_planner_id(
        "public_actor_committed_history_v1",
        &episode.final_view.committed_history,
    )?;
    let strategic_decisions = strategic_decision_ledger(&episode.final_view.committed_history)?;
    let map_graphs = public_map_graph_ledger(&episode.final_view.committed_history)?;
    let combat_traces = combat_trace_ledger(&episode.final_view.committed_history)?;
    Ok(PublicActorEpisodeReceiptV9 {
        schema: PUBLIC_ACTOR_EPISODE_RECEIPT_SCHEMA_V9.to_string(),
        seed,
        task: episode.final_view.task.clone(),
        actor: episode.actor,
        outcome: episode.outcome,
        budget: PublicActorBudgetReceiptV1 {
            requested: budget,
            remaining,
            world_transitions_used: budget
                .max_world_transitions
                .saturating_sub(remaining.remaining_world_transitions),
            reloads_used: budget
                .max_reloads
                .saturating_sub(remaining.remaining_reloads),
        },
        control: PublicActorControlReceiptV1 {
            actor_turns: episode.actor_turns,
            act_intents: episode.act_intents,
            reload_intents: episode.reload_intents,
            commit_intents: episode.commit_intents,
            unsupported_reason: episode.unsupported_reason,
            committed_public_steps: episode.final_view.committed_history.steps.len(),
            committed_scopes: episode.final_view.committed_history.completed_scopes.len(),
            committed_history_id,
        },
        strategic_decisions,
        map_graphs,
        combat_traces,
        last_public_decision,
        combat_plan_events,
        rule_search: None,
        exact_branch_selections,
        exact_branch_trials,
        strategic_support,
        combat_research_plans_installed,
        combat_research_plans_unavailable,
        combat_research_plan_actions,
        combat_research_plan_attempts,
        combat_research_plan_options,
        wall_time_millis: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
    })
}

fn public_map_graph_id(act: u8, map: &PlannerPublicMap) -> Result<String, String> {
    stable_planner_id("public_actor_map_graph_v1", &(act, map.boss, &map.nodes))
}

fn public_map_graph_ledger(
    history: &CommittedPublicHistoryV1,
) -> Result<Vec<PublicActorMapGraphV1>, String> {
    let mut seen = BTreeSet::new();
    let mut graphs = Vec::new();
    for committed in &history.steps {
        if committed.site != TaskDecisionSiteV1::Map {
            continue;
        }
        let TaskPublicObservationV1::Strategic { observation } = &committed.step.source.observation
        else {
            return Err("committed map decision has no strategic observation".to_string());
        };
        let id = public_map_graph_id(observation.run.act, &observation.public_map)?;
        if seen.insert(id.clone()) {
            graphs.push(PublicActorMapGraphV1 {
                id,
                act: observation.run.act,
                boss: observation.public_map.boss,
                nodes: observation.public_map.nodes.clone(),
            });
        }
    }
    Ok(graphs)
}

fn strategic_decision_ledger(
    history: &CommittedPublicHistoryV1,
) -> Result<Vec<PublicActorStrategicDecisionV1>, String> {
    history
        .steps
        .iter()
        .filter_map(|committed| {
            let TaskPublicObservationV1::Strategic { observation } =
                &committed.step.source.observation
            else {
                return None;
            };
            let Some(selected_candidate_ordinal) =
                committed.step.choice.ordinal_path.first().copied()
            else {
                return Some(Err(
                    "committed strategic step contains an empty public choice".to_string(),
                ));
            };
            let candidates = committed
                .step
                .source
                .candidates
                .iter()
                .enumerate()
                .map(|(candidate_ordinal, candidate)| match candidate {
                    TaskPublicCandidateV1::Strategic { action } => {
                        Ok(PublicActorStrategicCandidateV1 {
                            candidate_ordinal,
                            action: action.clone(),
                        })
                    }
                    TaskPublicCandidateV1::CombatAtomic { .. }
                    | TaskPublicCandidateV1::CombatIndexedChoice { .. }
                    | TaskPublicCandidateV1::CombatSelectionFamily { .. } => Err(format!(
                        "committed strategic decision contains non-strategic candidate {candidate_ordinal}"
                    )),
                })
                .collect::<Result<Vec<_>, _>>();
            let candidates = match candidates {
                Ok(candidates) => candidates,
                Err(error) => return Some(Err(error)),
            };
            let selected = committed
                .step
                .source
                .candidates
                .get(selected_candidate_ordinal)
                .and_then(|candidate| match candidate {
                    TaskPublicCandidateV1::Strategic { action } => Some(action.clone()),
                    TaskPublicCandidateV1::CombatAtomic { .. }
                    | TaskPublicCandidateV1::CombatIndexedChoice { .. }
                    | TaskPublicCandidateV1::CombatSelectionFamily { .. } => None,
                });
            let Some(selected) = selected else {
                return Some(Err(format!(
                    "committed strategic choice ordinal {selected_candidate_ordinal} does not select a strategic candidate"
                )));
            };
            let map_graph_id = if committed.site == TaskDecisionSiteV1::Map {
                match public_map_graph_id(observation.run.act, &observation.public_map) {
                    Ok(id) => Some(id),
                    Err(error) => return Some(Err(error)),
                }
            } else {
                None
            };
            Some(Ok(PublicActorStrategicDecisionV1 {
                scope: committed.scope,
                site: committed.site,
                public_snapshot_id: committed.step.source.information.snapshot_id.clone(),
                context: observation.context.clone(),
                before: PublicActorStrategicStateV1 {
                    act: observation.run.act,
                    floor: observation.run.floor,
                    current_hp: observation.run.current_hp,
                    max_hp: observation.run.max_hp,
                    gold: observation.run.gold,
                    visible_boss: observation.public_map.boss,
                    deck: observation
                        .cards
                        .iter()
                        .map(|card| PublicActorStrategicDeckCardV1 {
                            card: card.card,
                            upgrades: card.upgrades,
                        })
                        .collect(),
                    relics: observation.relics.iter().map(|relic| relic.relic).collect(),
                    potions: observation
                        .potions
                        .iter()
                        .map(|slot| slot.potion.as_ref().map(|potion| potion.potion))
                        .collect(),
                },
                map_graph_id,
                candidates,
                selected_candidate_ordinal,
                selected,
            }))
        })
        .collect()
}

fn combat_trace_ledger(
    history: &CommittedPublicHistoryV1,
) -> Result<Vec<PublicActorCombatTraceV1>, String> {
    let mut traces: Vec<PublicActorCombatTraceV1> = Vec::new();
    for committed in &history.steps {
        let TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } = &committed.step.source.observation
        else {
            continue;
        };
        if traces
            .last()
            .is_none_or(|trace| trace.scope != committed.scope)
        {
            let (act, floor, encounter_id) = match run_context {
                PublicCombatRunContextV1::Available {
                    act,
                    floor,
                    encounter_id,
                    ..
                } => (Some(*act), Some(*floor), *encounter_id),
                PublicCombatRunContextV1::Unavailable { .. } => (None, None, None),
            };
            traces.push(PublicActorCombatTraceV1 {
                scope: committed.scope,
                act,
                floor,
                encounter_id,
                is_elite: observation.encounter.is_elite_fight,
                is_boss: observation.encounter.is_boss_fight,
                entry_hp: observation.player.hp,
                entry_potions: observation
                    .potions
                    .iter()
                    .map(|slot| slot.as_ref().map(|potion| potion.potion_id))
                    .collect(),
                turns: Vec::new(),
                actions: Vec::new(),
                exit: PublicActorCombatExitV1::NoObservedSuccessor,
            });
        }
        let trace = traces.last_mut().expect("just inserted a combat trace");
        if trace
            .turns
            .last()
            .is_none_or(|turn| turn.turn != observation.turn.turn_count)
        {
            trace.turns.push(PublicActorCombatTurnV1 {
                turn: observation.turn.turn_count,
                hp: observation.player.hp,
                block: observation.player.block,
                energy: observation.turn.energy,
                hand: observation
                    .cards
                    .hand
                    .cards
                    .iter()
                    .map(|card| PublicActorCombatHandCardV1 {
                        card: card.card_id,
                        upgrades: card.upgrades,
                    })
                    .collect(),
                enemies: observation
                    .monsters
                    .iter()
                    .map(|monster| PublicActorCombatEnemyV1 {
                        slot: monster.slot,
                        enemy: match monster.enemy {
                            CombatLearningEnemyIdentityV1::Known { enemy_id } => enemy_id,
                        },
                        hp: monster.hp,
                        block: monster.block,
                        alive: monster.alive,
                        intent: monster.intent.intent.clone(),
                        preview_damage_per_hit: monster.intent.preview_damage_per_hit,
                    })
                    .collect(),
            });
        }
        let candidate_ordinal = committed
            .step
            .choice
            .ordinal_path
            .first()
            .copied()
            .ok_or_else(|| "committed combat step contains an empty public choice".to_string())?;
        let candidate = committed
            .step
            .source
            .candidates
            .get(candidate_ordinal)
            .ok_or_else(|| {
                format!(
                    "committed combat choice ordinal {candidate_ordinal} is outside its public candidates"
                )
            })?;
        let action = match candidate {
            TaskPublicCandidateV1::CombatAtomic {
                action:
                    PublicCombatAtomicActionV1::PlayCard {
                        hand_index,
                        target_monster_index,
                    },
            } => {
                let card = observation
                    .cards
                    .hand
                    .cards
                    .get(*hand_index)
                    .ok_or_else(|| {
                        format!("committed play card hand index {hand_index} is not public")
                    })?;
                PublicActorCombatActionKindV1::PlayCard {
                    card: card.card_id,
                    upgrades: card.upgrades,
                    target_monster_index: *target_monster_index,
                }
            }
            TaskPublicCandidateV1::CombatAtomic {
                action:
                    PublicCombatAtomicActionV1::UsePotion {
                        potion_index,
                        target_monster_index,
                    },
            } => PublicActorCombatActionKindV1::UsePotion {
                slot: *potion_index,
                potion: public_combat_potion_at(observation, *potion_index)?,
                target_monster_index: *target_monster_index,
                hand_before: observation
                    .cards
                    .hand
                    .cards
                    .iter()
                    .map(|card| PublicActorCombatHandCardV1 {
                        card: card.card_id,
                        upgrades: card.upgrades,
                    })
                    .collect(),
            },
            TaskPublicCandidateV1::CombatAtomic {
                action: PublicCombatAtomicActionV1::DiscardPotion { potion_index },
            } => PublicActorCombatActionKindV1::DiscardPotion {
                slot: *potion_index,
                potion: public_combat_potion_at(observation, *potion_index)?,
            },
            TaskPublicCandidateV1::CombatAtomic {
                action: PublicCombatAtomicActionV1::EndTurn,
            } => PublicActorCombatActionKindV1::EndTurn,
            TaskPublicCandidateV1::Strategic { .. } => {
                return Err("committed combat decision contains a strategic candidate".to_string())
            }
            TaskPublicCandidateV1::CombatAtomic { .. }
            | TaskPublicCandidateV1::CombatIndexedChoice { .. }
            | TaskPublicCandidateV1::CombatSelectionFamily { .. } => {
                PublicActorCombatActionKindV1::Other {
                    candidate_ordinal,
                    ordinal_path: committed.step.choice.ordinal_path.clone(),
                }
            }
        };
        trace.actions.push(PublicActorCombatActionV1 {
            turn: observation.turn.turn_count,
            hp_before: observation.player.hp,
            block_before: observation.player.block,
            energy_before: observation.turn.energy,
            action,
        });
        trace.exit = match &committed.step.observed_boundary {
            TaskObservedBoundaryV1::Decision { decision }
            | TaskObservedBoundaryV1::CommittedNextScope { decision } => {
                combat_successor_decision(decision)
            }
            TaskObservedBoundaryV1::CommittedGameTerminal { game_outcome } => {
                PublicActorCombatExitV1::GameTerminal {
                    outcome: *game_outcome,
                }
            }
            TaskObservedBoundaryV1::CommitReady
            | TaskObservedBoundaryV1::ExploredScopeExit { .. } => {
                PublicActorCombatExitV1::NoObservedSuccessor
            }
        };
    }
    for trace in &mut traces {
        if let Some(completed) = history
            .completed_scopes
            .iter()
            .find(|completed| completed.scope == trace.scope)
        {
            trace.exit = match &completed.exit {
                TaskCommittedScopeExitV1::NextScope { decision } => {
                    combat_successor_decision(decision)
                }
                TaskCommittedScopeExitV1::GameTerminal { game_outcome } => {
                    PublicActorCombatExitV1::GameTerminal {
                        outcome: *game_outcome,
                    }
                }
            };
        }
    }
    Ok(traces)
}

fn public_combat_potion_at(
    observation: &crate::agent::information::state::PublicCombatStateV1,
    slot: usize,
) -> Result<PotionId, String> {
    observation
        .potions
        .get(slot)
        .and_then(Option::as_ref)
        .map(|potion| potion.potion_id)
        .ok_or_else(|| format!("committed potion slot {slot} has no public potion"))
}

fn combat_successor_decision(decision: &TaskPublicDecisionV1) -> PublicActorCombatExitV1 {
    match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => {
            PublicActorCombatExitV1::PublicDecision {
                site: decision.site,
                hp: observation.run.current_hp,
                potions: observation
                    .potions
                    .iter()
                    .map(|slot| slot.potion.as_ref().map(|potion| potion.potion))
                    .collect(),
                recent_combat_hp_loss: observation.public_history.recent_combat_hp_loss,
                recent_combat_turns: observation.public_history.recent_combat_turns,
                recent_combat_potions_used: observation.public_history.recent_combat_potions_used,
            }
        }
        TaskPublicObservationV1::Combat { observation, .. } => {
            PublicActorCombatExitV1::PublicDecision {
                site: decision.site,
                hp: observation.player.hp,
                potions: observation
                    .potions
                    .iter()
                    .map(|slot| slot.as_ref().map(|potion| potion.potion_id))
                    .collect(),
                recent_combat_hp_loss: None,
                recent_combat_turns: None,
                recent_combat_potions_used: None,
            }
        }
    }
}

fn summarize(episodes: &[PublicActorEpisodeReceiptV9]) -> PublicActorCensusSummaryV1 {
    let mut summary = PublicActorCensusSummaryV1 {
        episodes: episodes.len(),
        ..PublicActorCensusSummaryV1::default()
    };
    for episode in episodes {
        match &episode.outcome {
            TaskEpisodeOutcomeV1::Success { .. } => summary.successes += 1,
            TaskEpisodeOutcomeV1::Paused { .. } => summary.paused += 1,
            TaskEpisodeOutcomeV1::Failure { reason, .. } => match reason {
                TaskEpisodeFailureV1::CommittedDefeat => summary.committed_defeats += 1,
                TaskEpisodeFailureV1::CommittedTerminalWithoutObjective => {
                    summary.committed_terminal_without_objective += 1
                }
                TaskEpisodeFailureV1::RetryBudgetExhausted => summary.retry_budget_exhausted += 1,
                TaskEpisodeFailureV1::TransitionBudgetExhausted => {
                    summary.transition_budget_exhausted += 1
                }
                TaskEpisodeFailureV1::UnsupportedDecisionSurface => {
                    summary.unsupported_decision_surface += 1
                }
                TaskEpisodeFailureV1::MechanicsOrProtocolError { .. } => {
                    summary.mechanics_or_protocol_errors += 1
                }
            },
        }
    }
    summary
}

fn last_public_decision(view: &TaskActorViewV1) -> Option<&TaskPublicDecisionV1> {
    if let TaskActorBoundaryV1::Decision { decision } = &view.boundary {
        return Some(decision);
    }
    if let Some(attempt) = view
        .node_memory
        .as_ref()
        .and_then(|memory| memory.attempts.last())
    {
        if let Some(step) = attempt.steps.last() {
            if let crate::agent::task::TaskObservedBoundaryV1::Decision { decision } =
                &step.observed_boundary
            {
                return Some(decision);
            }
            return Some(&step.source);
        }
        return Some(&attempt.root);
    }
    view.committed_history
        .steps
        .last()
        .map(|step| &step.step.source)
}

fn decision_context(decision: &TaskPublicDecisionV1) -> PublicActorLastDecisionV1 {
    let context = match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => {
            PublicActorDecisionContextV1::Strategic {
                act: observation.run.act,
                floor: observation.run.floor,
                context: observation.context.clone(),
            }
        }
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => {
            let (act, floor, encounter_id) = match run_context {
                PublicCombatRunContextV1::Available {
                    act,
                    floor,
                    encounter_id,
                    ..
                } => (Some(*act), Some(*floor), *encounter_id),
                PublicCombatRunContextV1::Unavailable { .. } => (None, None, None),
            };
            PublicActorDecisionContextV1::Combat {
                act,
                floor,
                encounter_id,
                turn: observation.turn.turn_count,
                is_elite: observation.encounter.is_elite_fight,
                is_boss: observation.encounter.is_boss_fight,
            }
        }
    };
    PublicActorLastDecisionV1 {
        site: decision.site,
        public_snapshot_id: decision.information.snapshot_id.clone(),
        context,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::{
        CommittedPublicStepV1, TaskObservedBoundaryV1, TaskPrivilegedSupportAccessV1,
        TaskPrivilegedSupportProvenanceV1, TaskPublicChoiceV1, TaskPublicStepV1,
        TaskStrategicObservationV1,
    };
    use crate::ai::planner_core::{
        PlannerDecisionSite, PlannerMapRoom, PlannerMechanicsManifest, PlannerPlayerClass,
        PlannerPublicHistory, PlannerPublicMap, PlannerPublicMapEdge, PlannerPublicMapNode,
        PlannerRunGoal, PlannerRunScalars,
    };
    use crate::eval::run_control::{
        CombatResearchPlanCandidateReceiptV1, CombatResearchPlanSelectionV1,
    };
    use serde_json::Value;

    #[test]
    fn census_records_build_profile_and_does_not_infer_it_for_legacy_artifacts() {
        let environment = current_public_actor_execution_environment_v1();
        assert_eq!(environment.cargo_profile, env!("STS_CARGO_PROFILE"));
        assert_eq!(environment.debug_assertions, cfg!(debug_assertions));

        let census = PublicActorCensusV9 {
            schema: PUBLIC_ACTOR_CENSUS_SCHEMA_V9.to_string(),
            execution_environment: Some(environment),
            requested_budget: TaskBudgetV1 {
                max_world_transitions: 1,
                max_reloads: 0,
            },
            requested_seeds: vec![17],
            summary: PublicActorCensusSummaryV1::default(),
            episodes: Vec::new(),
        };
        let mut legacy_value = serde_json::to_value(census).expect("serialize census");
        legacy_value
            .as_object_mut()
            .expect("census object")
            .remove("execution_environment");
        let legacy: PublicActorCensusV9 =
            serde_json::from_value(legacy_value).expect("read legacy census");
        assert_eq!(legacy.execution_environment, None);
    }

    #[test]
    fn proposal_event_counts_preserve_typed_generation_failures() {
        let decision_snapshot_id = "snapshot".to_string();
        let events = vec![
            PublicCombatPlanFollowerEventV1::Generated {
                generation: PublicCombatPlanGenerationV1::Ambiguous {
                    decision_snapshot_id: decision_snapshot_id.clone(),
                    attempts: Vec::new(),
                },
            },
            PublicCombatPlanFollowerEventV1::Generated {
                generation: PublicCombatPlanGenerationV1::UnknownCoverage {
                    decision_snapshot_id: decision_snapshot_id.clone(),
                    attempts: Vec::new(),
                },
            },
            PublicCombatPlanFollowerEventV1::Generated {
                generation: PublicCombatPlanGenerationV1::Inapplicable {
                    decision_snapshot_id: decision_snapshot_id.clone(),
                    attempts: Vec::new(),
                },
            },
            PublicCombatPlanFollowerEventV1::Generated {
                generation: PublicCombatPlanGenerationV1::Invalidated {
                    decision_snapshot_id,
                    detail: "non-canonical fixture".to_string(),
                    attempts: Vec::new(),
                },
            },
        ];

        let counts = PublicCombatPlanEventCountsV1::from_events(&events);

        assert_eq!(counts.generated, 4);
        assert_eq!(counts.generation_ambiguous, 1);
        assert_eq!(counts.generation_unknown_coverage, 1);
        assert_eq!(counts.generation_inapplicable, 1);
        assert_eq!(counts.generation_invalidated, 1);
        assert_eq!(counts.generation_unique_grounded, 0);
        assert_eq!(counts.steps_issued, 0);
    }

    #[test]
    fn strategic_ledger_retains_reward_surface_source_and_selected_skip() {
        let observation = TaskStrategicObservationV1 {
            mechanics: PlannerMechanicsManifest {
                mechanics_id: "ledger-fixture".to_string(),
                mechanics_version: 1,
            },
            run_goal: PlannerRunGoal::ActThreeVictory,
            decision_site: PlannerDecisionSite::CardReward,
            run: PlannerRunScalars {
                player_class: PlannerPlayerClass::Ironclad,
                ascension_level: 0,
                act: 1,
                floor: 7,
                current_hp: 61,
                max_hp: 80,
                gold: 123,
                keys: [false; 3],
                potion_capacity: 3,
                pending_act_entry: None,
            },
            cards: Vec::new(),
            relics: Vec::new(),
            potions: Vec::new(),
            public_map: PlannerPublicMap {
                current_x: 1,
                current_y: 7,
                boss: Some(EncounterId::SlimeBoss),
                nodes: Vec::new(),
            },
            context: PlannerDecisionContext::CardReward {
                reward_item_index: Some(2),
                pending_rewards: Vec::new(),
            },
            public_history: PlannerPublicHistory {
                shop_purge_count: 0,
                recent_combat_hp_loss: Some(4),
                recent_combat_turns: Some(3),
                recent_combat_potions_used: Some(0),
            },
        };
        let decision = TaskPublicDecisionV1::new(
            TaskDecisionSiteV1::CardReward,
            TaskPublicObservationV1::Strategic { observation },
            vec![
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::TakeCard {
                        reward_item_index: Some(2),
                        option_index: 0,
                        card: CardId::Entrench,
                        upgrades: 0,
                    },
                },
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::SkipCardReward {
                        reward_item_index: 2,
                    },
                },
            ],
        )
        .expect("strategic reward decision");
        let history = CommittedPublicHistoryV1 {
            steps: vec![CommittedPublicStepV1 {
                scope: TaskScopeOrdinalV1(4),
                site: TaskDecisionSiteV1::CardReward,
                step: TaskPublicStepV1 {
                    source: decision,
                    choice: TaskPublicChoiceV1::one(1),
                    observed_boundary: TaskObservedBoundaryV1::CommitReady,
                },
            }],
            completed_scopes: Vec::new(),
        };

        let ledger = strategic_decision_ledger(&history).expect("strategic ledger");

        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].before.visible_boss, Some(EncounterId::SlimeBoss));
        assert_eq!(ledger[0].candidates.len(), 2);
        assert_eq!(ledger[0].candidates[0].candidate_ordinal, 0);
        assert_eq!(ledger[0].candidates[1].candidate_ordinal, 1);
        assert_eq!(ledger[0].selected_candidate_ordinal, 1);
        assert!(matches!(
            ledger[0].selected,
            TaskStrategicActionV1::SkipCardReward {
                reward_item_index: 2
            }
        ));
        assert!(ledger[0].map_graph_id.is_none());
        assert!(public_map_graph_ledger(&history).unwrap().is_empty());

        let TaskPublicObservationV1::Strategic { mut observation } =
            history.steps[0].step.source.observation.clone()
        else {
            unreachable!();
        };
        observation.decision_site = PlannerDecisionSite::Map;
        observation.context = PlannerDecisionContext::Map { overlay: false };
        observation.run.floor = 5;
        observation.public_map = PlannerPublicMap {
            current_x: 0,
            current_y: 4,
            boss: Some(EncounterId::SlimeBoss),
            nodes: vec![
                PlannerPublicMapNode {
                    x: 0,
                    y: 5,
                    room: PlannerMapRoom::Rest,
                    has_emerald_key: false,
                    edges: vec![PlannerPublicMapEdge {
                        destination_x: 0,
                        destination_y: 6,
                    }],
                },
                PlannerPublicMapNode {
                    x: 1,
                    y: 5,
                    room: PlannerMapRoom::Shop,
                    has_emerald_key: false,
                    edges: vec![PlannerPublicMapEdge {
                        destination_x: 0,
                        destination_y: 6,
                    }],
                },
                PlannerPublicMapNode {
                    x: 0,
                    y: 6,
                    room: PlannerMapRoom::Elite,
                    has_emerald_key: false,
                    edges: Vec::new(),
                },
            ],
        };
        let mut next = observation.clone();
        next.public_map.current_x = 0;
        next.public_map.current_y = 5;
        next.run.floor = 6;
        let map_step = |observation, x, y, scope| CommittedPublicStepV1 {
            scope: TaskScopeOrdinalV1(scope),
            site: TaskDecisionSiteV1::Map,
            step: TaskPublicStepV1 {
                source: TaskPublicDecisionV1::new(
                    TaskDecisionSiteV1::Map,
                    TaskPublicObservationV1::Strategic { observation },
                    vec![TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::ChooseRouteNode {
                            x,
                            y,
                            flight: false,
                            question_mark_outcomes: Vec::new(),
                        },
                    }],
                )
                .unwrap(),
                choice: TaskPublicChoiceV1::one(0),
                observed_boundary: TaskObservedBoundaryV1::CommitReady,
            },
        };
        let mut map_history = CommittedPublicHistoryV1 {
            steps: vec![map_step(observation, 0, 5, 5), map_step(next, 0, 6, 6)],
            completed_scopes: Vec::new(),
        };
        let graph_ledger = public_map_graph_ledger(&map_history).unwrap();
        let decisions = strategic_decision_ledger(&map_history).unwrap();
        assert_eq!(
            graph_ledger.len(),
            1,
            "current position does not duplicate topology"
        );
        assert_eq!(graph_ledger[0].nodes.len(), 3);
        assert_eq!(graph_ledger[0].nodes[1].room, PlannerMapRoom::Shop);
        assert_eq!(
            decisions[0].map_graph_id.as_deref(),
            Some(graph_ledger[0].id.as_str())
        );
        assert_eq!(decisions[1].map_graph_id, decisions[0].map_graph_id);

        let TaskPublicObservationV1::Strategic { observation } =
            &mut map_history.steps[1].step.source.observation
        else {
            unreachable!();
        };
        observation.public_map.nodes[1].room = PlannerMapRoom::Monster;
        let changed_graphs = public_map_graph_ledger(&map_history).unwrap();
        let changed_decisions = strategic_decision_ledger(&map_history).unwrap();
        assert_eq!(
            changed_graphs.len(),
            2,
            "revealed maps retain separate versions"
        );
        assert_ne!(
            changed_decisions[0].map_graph_id,
            changed_decisions[1].map_graph_id
        );
    }

    #[test]
    fn v9_receipt_separates_combat_and_strategic_privileged_support() {
        let lane = crate::agent::task::TaskResearchPlanLaneAttemptV1 {
            provenance: crate::agent::task::TaskPrivilegedSupportProvenanceV1 {
                access:
                    crate::agent::task::TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
                producer_name: "atomic".to_string(),
                producer_revision: 1,
            },
            work_unit: crate::agent::task::TaskResearchPlanWorkUnitV1::AtomicExpandedNodes,
            work_consumed: 301,
            exit: crate::agent::task::TaskResearchPlanAttemptExitV1::Witness,
        };
        let provenance = lane.provenance.clone();
        let outcome = crate::agent::task::TaskResearchPlanOutcomeV1 {
            final_hp: 74,
            final_gold: 123,
            persistent: crate::agent::task::TaskResearchPlanPersistentOutcomeV1 {
                max_hp: 74,
                recoverable_gold_delta: 0,
                unrecovered_stolen_gold: 0,
                ritual_dagger_value: 0,
                genetic_algorithm_value: 0,
                external_burden_count: 0,
            },
            consumed_entry_potion_slots: vec![0],
            total_potions_consumed: 1,
            generated_potions: Vec::new(),
            final_relics: Vec::new(),
            meta_changes: Vec::new(),
            pending_rewards: Vec::new(),
            combat_mugged: false,
        };
        let value = serde_json::to_value(PublicActorEpisodeReceiptV9 {
            schema: PUBLIC_ACTOR_EPISODE_RECEIPT_SCHEMA_V9.to_string(),
            seed: 0,
            task: TaskSpecV1::ironclad_a0_act_three_fail_closed(),
            actor: TaskActorIdentityV1::privileged_support_assisted("fixture", 1),
            outcome: TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::CommittedDefeat,
                game_outcome: None,
            },
            budget: PublicActorBudgetReceiptV1 {
                requested: TaskBudgetV1 {
                    max_world_transitions: 1,
                    max_reloads: 0,
                },
                remaining: TaskBudgetStateV1 {
                    remaining_world_transitions: 0,
                    remaining_reloads: 0,
                },
                world_transitions_used: 1,
                reloads_used: 0,
            },
            control: PublicActorControlReceiptV1 {
                actor_turns: 1,
                act_intents: 1,
                reload_intents: 0,
                commit_intents: 0,
                unsupported_reason: None,
                committed_public_steps: 1,
                committed_scopes: 0,
                committed_history_id: "fixture".to_string(),
            },
            strategic_decisions: Vec::new(),
            map_graphs: Vec::new(),
            combat_traces: Vec::new(),
            last_public_decision: None,
            combat_plan_events: PublicCombatPlanEventCountsV1::default(),
            rule_search: None,
            exact_branch_selections: 0,
            exact_branch_trials: 0,
            strategic_support: Vec::new(),
            combat_research_plans_installed: 1,
            combat_research_plans_unavailable: 0,
            combat_research_plan_actions: 1,
            combat_research_plan_attempts: vec![CombatResearchPlanAttemptReceiptV1 {
                public_snapshot_id: "root".to_string(),
                run_context: Some(PublicCombatRunContextV1::Available {
                    run_goal: PlannerRunGoal::ActThreeVictory,
                    ascension_level: 0,
                    combat_victory_guarantees_full_heal: false,
                    act: 1,
                    floor: 19,
                    keys: [false; 3],
                    public_map: PlannerPublicMap {
                        current_x: 0,
                        current_y: 6,
                        boss: Some(EncounterId::SlimeBoss),
                        nodes: Vec::new(),
                    },
                    encounter_id: None,
                }),
                act: Some(1),
                floor: Some(19),
                encounter_id: None,
                hp: 74,
                max_hp: 74,
                turn: 1,
                deck: Vec::new(),
                relics: Vec::new(),
                potions: Vec::new(),
                monsters: Vec::new(),
                lanes: vec![lane],
                exit: crate::agent::task::TaskResearchPlanAttemptExitV1::Witness,
                candidates: vec![CombatResearchPlanCandidateReceiptV1 {
                    supporting_provenances: vec![provenance.clone()],
                    plan_steps: 9,
                    satisfies_requested_quality: true,
                    outcome,
                }],
                non_dominated_candidate_ordinals: vec![0],
                selection_provenance: Some(TaskPrivilegedSupportProvenanceV1 {
                    access: TaskPrivilegedSupportAccessV1::PrivilegedExactStrategicOwner,
                    producer_name: "combat-plan-owner".to_string(),
                    producer_revision: 1,
                }),
                run_commitment: None,
                selection: CombatResearchPlanSelectionV1::SingleCandidate,
                selected_candidate_ordinal: Some(0),
                selected_supporting_provenances: vec![provenance],
                plan_steps: 9,
            }],
            combat_research_plan_options: None,
            wall_time_millis: 1,
        })
        .expect("serialize V9 receipt");

        assert_eq!(
            value.get("schema"),
            Some(&Value::String(
                PUBLIC_ACTOR_EPISODE_RECEIPT_SCHEMA_V9.to_string()
            ))
        );
        let mut legacy_value = value.clone();
        legacy_value
            .as_object_mut()
            .expect("receipt object")
            .remove("combat_traces");
        legacy_value
            .as_object_mut()
            .expect("receipt object")
            .remove("map_graphs");
        let legacy: PublicActorEpisodeReceiptV9 =
            serde_json::from_value(legacy_value).expect("read V9 receipt without added ledgers");
        assert!(legacy.combat_traces.is_empty());
        assert!(legacy.map_graphs.is_empty());
        assert_eq!(
            value["combat_research_plan_attempts"][0]["candidates"][0]
                .get("satisfies_requested_quality"),
            Some(&Value::Bool(true))
        );
        assert!(value["combat_research_plan_attempts"][0]["lanes"][0]
            .get("selected_for_plan")
            .is_none());
        assert_eq!(
            value["combat_research_plan_attempts"][0]["run_context"]["public_map"]["boss"],
            Value::String("SlimeBoss".to_string())
        );
        assert_eq!(
            value["combat_research_plan_attempts"][0]["non_dominated_candidate_ordinals"],
            serde_json::json!([0])
        );
        assert_eq!(
            value["combat_research_plan_attempts"][0]["selection_provenance"]["producer_name"],
            Value::String("combat-plan-owner".to_string())
        );
        assert_eq!(
            value["combat_research_plan_attempts"][0]["selected_candidate_ordinal"],
            Value::from(0)
        );
    }
}
