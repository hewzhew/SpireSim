use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sts_combat_planner::CombatDecisionRoot;
use sts_oracle_learning::agent::information::action::PublicCombatAtomicActionV1;
use sts_oracle_learning::agent::information::state::{
    CombatLearningEnemyIdentityV1, CombatLearningIntentV1, CombatLearningMonsterStateV1,
    CombatLearningPowerV1, PublicCombatStateV1,
};
use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskResearchPlanAttemptV1, TaskResearchPlanOutcomeV1,
};
use sts_oracle_learning::content::cards::CardId;
use sts_oracle_learning::content::potions::PotionId;
use sts_oracle_learning::content::powers::PowerId;
use sts_oracle_learning::content::relics::RelicState;

use crate::agent_protocol::{
    load_combat_research_artifact_v1, resolve_artifact_combat_research_report_candidate_index_v1,
    AgentCombatResearchReplayEndpointV1, AgentCombatResearchReplayProofV1,
    RunPlayCombatResearchArtifactV1, COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1,
};
use crate::combat_explain::visible_incoming_damage_v1;
use crate::combat_query::{
    load_combat_trajectory_query_commit_artifact_v2, RunPlayCombatTrajectoryQueryCommitArtifactV2,
    COMBAT_TRAJECTORY_QUERY_SCHEMA_V2,
};
use crate::driver::combat_research_candidate_id_v1;
use crate::driver::RunPlayControllerV1;
use crate::selector::{parse_action_selector_v1, semantic_action_for_choice_v1};
use crate::storage::{read_bounded, RunPlayPathsV1, DEFAULT_ARTIFACT_MAX_BYTES};

pub const COMBAT_TRACE_SCHEMA_V3: &str = "sts-run-play-combat-trace-v3";

#[derive(Deserialize)]
struct CombatTraceSourceSchemaV1 {
    schema: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTraceV1 {
    pub schema: String,
    pub session_id: String,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub source_artifact_effort: String,
    pub durable_head_unchanged: bool,
    pub steps_included: bool,
    pub candidates: Vec<RunPlayCombatCandidateTraceV1>,
}

impl RunPlayCombatTraceV1 {
    pub fn set_steps_included_v1(&mut self, include_steps: bool) {
        self.steps_included = include_steps;
        if !include_steps {
            for candidate in &mut self.candidates {
                candidate.steps.clear();
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatCandidateTraceV1 {
    pub candidate_index: usize,
    pub candidate_id: String,
    pub final_hp: i32,
    pub final_max_hp: i32,
    pub total_potions_consumed: u32,
    pub final_relics: Vec<RelicState>,
    pub action_count: usize,
    pub feed_attempts: Vec<RunPlayFeedAttemptV1>,
    pub potion_uses: Vec<RunPlayPotionUseV1>,
    pub turn_reviews: Vec<RunPlayCombatTurnReviewV1>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<RunPlayCombatTraceStepV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnReviewV1 {
    pub turn: u32,
    pub first_step: usize,
    pub last_step: usize,
    pub actions: Vec<String>,
    pub start: RunPlayCombatTurnStartV1,
    pub end_turn: Option<RunPlayCombatEndTurnReviewV1>,
    /// Net HP change across every observed transition in this turn. A lethal
    /// transition can combine card effects, damage and battle-end healing;
    /// boundary observations do not establish separate effect attribution.
    pub observed_player_hp_delta: i32,
    pub observed_player_max_hp_delta: i32,
    pub end_boundary: RunPlayCombatNextBoundaryV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatEndTurnReviewV1 {
    pub visible_incoming_damage: Option<i32>,
    pub player_block: i32,
    /// The End Turn transition's contribution to the enclosing turn total,
    /// including any battle-end or next-turn effects settled by that action.
    pub observed_player_hp_delta: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTurnStartV1 {
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub visible_incoming_damage: Option<i32>,
    pub player_powers: Vec<CombatLearningPowerV1>,
    pub hand: Vec<RunPlayCombatCardSummaryV1>,
    pub monsters: Vec<RunPlayCombatMonsterSummaryV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatCardSummaryV1 {
    pub card_id: CardId,
    pub upgrades: u8,
    pub effective_cost: i32,
    pub current_damage: i32,
    pub current_block: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatMonsterSummaryV1 {
    pub slot: u8,
    pub enemy: CombatLearningEnemyIdentityV1,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub half_dead: bool,
    pub intent: CombatLearningIntentV1,
    pub powers: Vec<CombatLearningPowerV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTraceStepV1 {
    pub step: usize,
    pub turn: u32,
    pub action: String,
    pub label: String,
    /// Ephemeral display facts from the selected public candidate. Raw trace
    /// records keep their canonical label and never serialize a view locale.
    #[serde(skip)]
    pub(crate) display: RunPlayCombatActionDisplayV1,
    pub player_block_before_action: i32,
    pub visible_incoming_damage_before_action: Option<i32>,
    pub card: Option<RunPlayPlayedCardV1>,
    pub potion: Option<RunPlayPotionUseV1>,
    pub feed: Option<RunPlayFeedAttemptV1>,
    pub delta: RunPlayCombatBoundaryDeltaV1,
    pub source_public_snapshot_id: String,
    pub next_public_snapshot_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RunPlayCombatActionDisplayV1 {
    PlayCard(RunPlayPlayedCardV1),
    UsePotion {
        slot: usize,
        potion_id: PotionId,
        target_slot: Option<u8>,
    },
    EndTurn,
    Proceed,
    Cancel,
    DiscardPotion {
        slot: usize,
    },
    ChooseCard {
        card_id: CardId,
        upgrades: u8,
    },
    ChooseStance(sts_oracle_learning::runtime::combat::StanceId),
    SelectCards(Vec<(Option<CardId>, u8)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayPlayedCardV1 {
    pub card_id: CardId,
    pub upgrades: u8,
    pub target_slot: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayPotionUseV1 {
    pub step: usize,
    pub turn: u32,
    pub slot: usize,
    pub potion_id: PotionId,
    pub target_slot: Option<u8>,
    pub observed_player_hp_delta: i32,
    pub observed_player_block_delta: i32,
    pub observed_energy_delta: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayFeedAttemptV1 {
    pub step: usize,
    pub turn: u32,
    pub target_slot: u8,
    pub target: CombatLearningEnemyIdentityV1,
    pub target_hp_before: i32,
    pub target_block_before: i32,
    pub target_hp_after: Option<i32>,
    pub target_alive_after: Option<bool>,
    pub fatal_payoff_observed: bool,
    pub max_hp_gain: i32,
    pub remaining_hp_after_nonfatal: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatBoundaryDeltaV1 {
    pub next_boundary: RunPlayCombatNextBoundaryV1,
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub player_block: i32,
    pub energy: Option<i32>,
    pub cards_played_this_turn: Option<i32>,
    pub potion_slots_changed: Vec<usize>,
    pub cards_added_to_exhaust: Vec<CardId>,
    pub player_power_changes: Vec<RunPlayPowerDeltaV1>,
    pub monster_changes: Vec<RunPlayMonsterDeltaV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatNextBoundaryV1 {
    Combat,
    Strategic,
    GameTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayPowerDeltaV1 {
    pub power: PowerId,
    pub before: i32,
    pub after: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayMonsterDeltaV1 {
    pub slot: u8,
    pub enemy: CombatLearningEnemyIdentityV1,
    pub hp: i32,
    pub block: i32,
    pub alive_before: bool,
    pub alive_after: Option<bool>,
}

#[derive(Clone, Copy)]
struct PublicPlayerBoundaryV1 {
    hp: i32,
    max_hp: i32,
    block: i32,
    energy: Option<i32>,
    cards_played_this_turn: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct VerifiedCombatTraceCandidateV1 {
    pub candidate_id: String,
    pub outcome: TaskResearchPlanOutcomeV1,
    pub replay_proof: AgentCombatResearchReplayProofV1,
}

#[derive(Clone, Debug)]
pub struct VerifiedCombatTraceSourceV1 {
    pub session_id: String,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub root_exact_state_hash: Option<String>,
    pub source_label: String,
    pub candidates: Vec<VerifiedCombatTraceCandidateV1>,
}

pub fn trace_combat_artifact_v1(
    paths: RunPlayPathsV1,
    artifact_path: &Path,
    candidate_indices: &[usize],
) -> Result<RunPlayCombatTraceV1, String> {
    let payload = read_bounded(
        artifact_path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run-play combat trace source artifact",
    )?;
    let envelope: CombatTraceSourceSchemaV1 =
        serde_json::from_slice(&payload).map_err(|error| {
            format!(
                "failed to identify combat trace source artifact {}: {error}",
                artifact_path.display()
            )
        })?;
    match envelope.schema.as_str() {
        COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1 => {
            let artifact = load_combat_research_artifact_v1(artifact_path)?;
            trace_combat_research_artifact_v1(paths, &artifact, candidate_indices)
        }
        COMBAT_TRAJECTORY_QUERY_SCHEMA_V2 => {
            if candidate_indices != [0] {
                return Err(
                    "combat trace accepts candidate 0 for a combat-query artifact's selected witness"
                        .to_string(),
                );
            }
            let artifact = load_combat_trajectory_query_commit_artifact_v2(artifact_path)?;
            let source = verified_combat_trace_source_from_query_v2(&artifact)?;
            trace_verified_combat_source_v1(paths, &source, &[0])
        }
        other => Err(format!(
            "unsupported combat trace source schema {other:?}; expected {COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1} or {COMBAT_TRAJECTORY_QUERY_SCHEMA_V2}"
        )),
    }
}

pub fn verified_combat_trace_source_from_query_v2(
    artifact: &RunPlayCombatTrajectoryQueryCommitArtifactV2,
) -> Result<VerifiedCombatTraceSourceV1, String> {
    let candidate_id = "combat-query-selected".to_string();
    let source = VerifiedCombatTraceSourceV1 {
        session_id: artifact.session_id.clone(),
        root_archive_node_id: artifact.root_archive_node_id.clone(),
        root_public_snapshot_id: artifact.root_public_snapshot_id.clone(),
        root_exact_state_hash: Some(artifact.root_exact_state_hash.clone()),
        source_label: COMBAT_TRAJECTORY_QUERY_SCHEMA_V2.to_string(),
        candidates: vec![VerifiedCombatTraceCandidateV1 {
            candidate_id: candidate_id.clone(),
            outcome: artifact.proof.public_outcome.clone(),
            replay_proof: AgentCombatResearchReplayProofV1 {
                candidate_id,
                steps: artifact.proof.replay_steps.clone(),
                endpoint: artifact.proof.endpoint.clone(),
            },
        }],
    };
    source.validate()?;
    Ok(source)
}

impl VerifiedCombatTraceSourceV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.session_id.trim().is_empty()
            || self.root_archive_node_id.trim().is_empty()
            || self.root_public_snapshot_id.trim().is_empty()
            || self
                .root_exact_state_hash
                .as_ref()
                .is_some_and(|hash| hash.trim().is_empty())
            || self.source_label.trim().is_empty()
        {
            return Err("verified combat trace source identity is incomplete".to_string());
        }
        let mut candidate_ids = BTreeSet::new();
        for candidate in &self.candidates {
            let proof = &candidate.replay_proof;
            if candidate.candidate_id.trim().is_empty()
                || !candidate_ids.insert(candidate.candidate_id.as_str())
                || proof.candidate_id != candidate.candidate_id
                || proof.steps.is_empty()
                || proof
                    .steps
                    .first()
                    .map(|step| step.source_public_snapshot_id.as_str())
                    != Some(self.root_public_snapshot_id.as_str())
                || proof.steps.iter().any(|step| {
                    step.source_public_snapshot_id.trim().is_empty()
                        || step.action.trim().is_empty()
                })
            {
                return Err(
                    "verified combat trace candidates require distinct ids and one root-bound semantic replay"
                        .to_string(),
                );
            }
            match &proof.endpoint {
                AgentCombatResearchReplayEndpointV1::PublicDecision { public_snapshot_id }
                    if public_snapshot_id.trim().is_empty() =>
                {
                    return Err("verified combat trace public endpoint is empty".to_string());
                }
                AgentCombatResearchReplayEndpointV1::GameTerminal {
                    outcome: sts_oracle_learning::agent::task::TaskEpisodeOutcomeV1::Paused { .. },
                } => {
                    return Err(
                        "verified combat trace terminal endpoint cannot be paused".to_string()
                    );
                }
                _ => {}
            }
            if candidate.outcome.final_hp < 1
                || candidate.outcome.persistent.max_hp < candidate.outcome.final_hp
            {
                return Err("verified combat trace candidate has invalid public HP".to_string());
            }
        }
        Ok(())
    }
}

pub fn trace_combat_research_artifact_v1(
    paths: RunPlayPathsV1,
    artifact: &RunPlayCombatResearchArtifactV1,
    report_candidate_indices: &[usize],
) -> Result<RunPlayCombatTraceV1, String> {
    let source = verified_combat_trace_source_from_research_artifact_v1(artifact)?;
    let local_indices = report_candidate_indices
        .iter()
        .map(|index| {
            resolve_artifact_combat_research_report_candidate_index_v1(&artifact.candidates, *index)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut trace = trace_verified_combat_source_v1(paths, &source, &local_indices)?;
    for candidate in &mut trace.candidates {
        let artifact_candidate = artifact
            .candidates
            .iter()
            .find(|artifact_candidate| artifact_candidate.candidate_id == candidate.candidate_id)
            .ok_or_else(|| {
                format!(
                    "combat trace candidate {} disappeared from its research artifact",
                    candidate.candidate_id
                )
            })?;
        candidate.candidate_index = artifact_candidate
            .report_candidate_index
            .unwrap_or(candidate.candidate_index);
    }
    Ok(trace)
}

pub fn verified_combat_trace_source_from_research_artifact_v1(
    artifact: &RunPlayCombatResearchArtifactV1,
) -> Result<VerifiedCombatTraceSourceV1, String> {
    artifact.validate()?;
    if artifact.candidate_replay_proofs.is_empty() {
        return Err("combat trace requires an artifact with replay proofs".to_string());
    }
    let candidates = artifact
        .candidates
        .iter()
        .map(|candidate| {
            Ok(VerifiedCombatTraceCandidateV1 {
                candidate_id: candidate.candidate_id.clone(),
                outcome: candidate.outcome.clone(),
                replay_proof: replay_for_candidate_v1(artifact, &candidate.candidate_id)?.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(VerifiedCombatTraceSourceV1 {
        session_id: artifact.session_id.clone(),
        root_archive_node_id: artifact.root_archive_node_id.clone(),
        root_public_snapshot_id: artifact.root_public_snapshot_id.clone(),
        root_exact_state_hash: None,
        source_label: format!("{:?}", artifact.effort),
        candidates,
    })
}

pub(crate) fn witness_minimum_hp(
    line: &sts_oracle_learning::eval::run_control::ExactRunPlayProbeLineV1,
    final_hp: i32,
) -> Option<i32> {
    let hp = |decision: &sts_oracle_learning::agent::task::TaskPublicDecisionV1| match &decision
        .observation
    {
        sts_oracle_learning::agent::task::TaskPublicObservationV1::Combat {
            observation, ..
        } => observation.player.hp,
        sts_oracle_learning::agent::task::TaskPublicObservationV1::Strategic {
            observation,
            ..
        } => observation.run.current_hp,
    };
    line.steps
        .iter()
        .flat_map(|step| {
            std::iter::once(hp(&step.source_decision)).chain(step.next_decision.as_ref().map(hp))
        })
        .chain(std::iter::once(final_hp))
        .min()
}

pub fn trace_cached_combat_research_v1(
    controller: &RunPlayControllerV1,
    attempt: &TaskResearchPlanAttemptV1,
    witness_lines: &[sts_oracle_learning::eval::run_control::ExactRunPlayProbeLineV1],
    candidate_ids: &[String],
    source_label: String,
) -> Result<RunPlayCombatTraceV1, String> {
    attempt.validate()?;
    if attempt.candidates.len() != witness_lines.len() {
        return Err("cached combat research candidates and witnesses disagree".to_string());
    }
    if candidate_ids.is_empty() {
        return Err("combat review requires at least one candidate id".to_string());
    }
    let current = controller
        .head()
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat review requires a public decision".to_string())?
        .public_decision_v1()?;
    let mut available = Vec::with_capacity(attempt.candidates.len());
    for candidate in &attempt.candidates {
        available.push(combat_research_candidate_id_v1(candidate)?);
    }
    let mut requested = Vec::with_capacity(candidate_ids.len());
    let mut seen = BTreeSet::new();
    for candidate_id in candidate_ids {
        let index = available
            .iter()
            .position(|available_id| available_id == candidate_id)
            .ok_or_else(|| {
                format!("combat review candidate id {candidate_id:?} is not in the cached frontier")
            })?;
        if !seen.insert(index) {
            return Err(format!(
                "combat review candidate id {candidate_id:?} was requested twice"
            ));
        }
        requested.push(index);
    }

    let mut candidates = Vec::with_capacity(requested.len());
    for index in requested {
        let candidate = &attempt.candidates[index];
        let line = &witness_lines[index];
        let actions = line
            .steps
            .iter()
            .map(|step| semantic_action_for_choice_v1(&step.source_decision, &step.choice))
            .collect::<Result<Vec<_>, String>>()?;
        candidates.push(trace_materialized_candidate_v1(
            index,
            available[index].clone(),
            &candidate.outcome,
            line,
            &actions,
        )?);
    }

    Ok(RunPlayCombatTraceV1 {
        schema: COMBAT_TRACE_SCHEMA_V3.to_string(),
        session_id: controller.head().session_id.clone(),
        root_archive_node_id: controller.head().position.archive_node_id().to_string(),
        root_public_snapshot_id: current.information.snapshot_id,
        source_artifact_effort: source_label,
        durable_head_unchanged: true,
        steps_included: true,
        candidates,
    })
}

pub fn trace_verified_combat_source_v1(
    paths: RunPlayPathsV1,
    source: &VerifiedCombatTraceSourceV1,
    candidate_indices: &[usize],
) -> Result<RunPlayCombatTraceV1, String> {
    source.validate()?;
    let mut unique = BTreeSet::new();
    for &index in candidate_indices {
        if index >= source.candidates.len() {
            return Err(format!(
                "combat trace candidate {index} is outside artifact candidate count {}",
                source.candidates.len()
            ));
        }
        if !unique.insert(index) {
            return Err(format!(
                "combat trace candidate {index} was requested twice"
            ));
        }
    }

    let selector = format!("n{}", source.root_archive_node_id);
    let mut controller = RunPlayControllerV1::open_at(paths, &selector)?;
    let root = controller
        .head()
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat trace historical root has no public decision".to_string())?
        .public_decision_v1()?;
    if controller.head().session_id != source.session_id
        || controller.head().position.archive_node_id() != source.root_archive_node_id
        || root.information.snapshot_id != source.root_public_snapshot_id
    {
        return Err(
            "combat trace historical root disagrees with its artifact identity".to_string(),
        );
    }
    if let Some(expected) = source.root_exact_state_hash.as_ref() {
        let position = controller.current_combat_position_for_lab_v1()?;
        let exact_root = CombatDecisionRoot::new(position)
            .map_err(|error| format!("combat trace exact root is unstable: {error:?}"))?;
        let observed = exact_root.exact_state_hash().to_string();
        if observed != *expected {
            return Err(format!(
                "combat trace exact root disagrees with its artifact: artifact={expected} current={observed}"
            ));
        }
    }

    let requested = unique.into_iter().collect::<Vec<_>>();
    let mut proofs = Vec::with_capacity(requested.len());
    let mut lines = Vec::with_capacity(requested.len());
    for &index in &requested {
        let candidate = &source.candidates[index];
        let proof = &candidate.replay_proof;
        let line = proof
            .steps
            .iter()
            .map(|step| {
                parse_action_selector_v1(
                    &step.action,
                    "combat-trace artifact",
                    sts_presentation::Locale::En,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        proofs.push(proof);
        lines.push(line);
    }
    let comparison = controller.compare_action_lines_v1(&lines, sts_presentation::Locale::En)?;
    if comparison.root_archive_node_id != source.root_archive_node_id {
        return Err("combat trace compare changed the artifact root".to_string());
    }

    let mut candidates = Vec::with_capacity(requested.len());
    for (((&index, proof), candidate), line) in requested
        .iter()
        .zip(proofs)
        .zip(requested.iter().map(|index| &source.candidates[*index]))
        .zip(comparison.lines)
    {
        validate_replayed_line_v1(proof, &line)?;
        let actions = proof
            .steps
            .iter()
            .map(|step| step.action.clone())
            .collect::<Vec<_>>();
        candidates.push(trace_materialized_candidate_v1(
            index,
            candidate.candidate_id.clone(),
            &candidate.outcome,
            &line,
            &actions,
        )?);
    }

    Ok(RunPlayCombatTraceV1 {
        schema: COMBAT_TRACE_SCHEMA_V3.to_string(),
        session_id: source.session_id.clone(),
        root_archive_node_id: source.root_archive_node_id.clone(),
        root_public_snapshot_id: source.root_public_snapshot_id.clone(),
        source_artifact_effort: source.source_label.clone(),
        durable_head_unchanged: true,
        steps_included: true,
        candidates,
    })
}

fn trace_materialized_candidate_v1(
    candidate_index: usize,
    candidate_id: String,
    outcome: &TaskResearchPlanOutcomeV1,
    line: &sts_oracle_learning::eval::run_control::ExactRunPlayProbeLineV1,
    actions: &[String],
) -> Result<RunPlayCombatCandidateTraceV1, String> {
    if line.steps.len() != actions.len() {
        return Err("combat review actions and replay steps disagree".to_string());
    }
    let mut steps = Vec::with_capacity(line.steps.len());
    let mut feed_attempts = Vec::new();
    let mut potion_uses = Vec::new();
    for (step_index, (observed, action)) in line.steps.iter().zip(actions).enumerate() {
        let traced = trace_step_v1(step_index + 1, action, observed)?;
        if let Some(feed) = traced.feed.clone() {
            feed_attempts.push(feed);
        }
        if let Some(potion) = traced.potion.clone() {
            potion_uses.push(potion);
        }
        steps.push(traced);
    }
    let turn_reviews = project_turn_reviews_v1(line, &steps)?;
    let final_boundary = line
        .steps
        .last()
        .and_then(|step| step.next_decision.as_ref());
    let observed_final = final_boundary.and_then(player_boundary_v1);
    if let Some(observed) = observed_final {
        if observed.hp != outcome.final_hp || observed.max_hp != outcome.persistent.max_hp {
            return Err(format!(
                "combat review candidate {candidate_index} final player state disagrees with its outcome: observed {}/{}, expected {}/{}",
                observed.hp,
                observed.max_hp,
                outcome.final_hp,
                outcome.persistent.max_hp
            ));
        }
    }
    Ok(RunPlayCombatCandidateTraceV1 {
        candidate_index,
        candidate_id,
        final_hp: outcome.final_hp,
        final_max_hp: outcome.persistent.max_hp,
        total_potions_consumed: outcome.total_potions_consumed,
        final_relics: outcome.final_relics.clone(),
        action_count: actions.len(),
        feed_attempts,
        potion_uses,
        turn_reviews,
        steps,
    })
}

pub(crate) fn combat_turn_start_v1(state: &PublicCombatStateV1) -> RunPlayCombatTurnStartV1 {
    RunPlayCombatTurnStartV1 {
        player_hp: state.player.hp,
        player_max_hp: state.player.max_hp,
        player_block: state.player.block,
        energy: state.turn.energy,
        visible_incoming_damage: visible_incoming_damage_v1(state),
        player_powers: state.player.powers.clone(),
        hand: state
            .cards
            .hand
            .cards
            .iter()
            .map(|card| RunPlayCombatCardSummaryV1 {
                card_id: card.card_id,
                upgrades: card.upgrades,
                effective_cost: card.effective_cost,
                current_damage: card.current_damage,
                current_block: card.current_block,
            })
            .collect(),
        monsters: state
            .monsters
            .iter()
            .filter(|monster| monster.alive || monster.half_dead)
            .map(|monster| RunPlayCombatMonsterSummaryV1 {
                slot: monster.slot,
                enemy: monster.enemy,
                hp: monster.hp,
                max_hp: monster.max_hp,
                block: monster.block,
                half_dead: monster.half_dead,
                intent: monster.intent.clone(),
                powers: monster.powers.clone(),
            })
            .collect(),
    }
}

fn project_turn_reviews_v1(
    line: &sts_oracle_learning::eval::run_control::ExactRunPlayProbeLineV1,
    steps: &[RunPlayCombatTraceStepV1],
) -> Result<Vec<RunPlayCombatTurnReviewV1>, String> {
    let mut reviews = Vec::<RunPlayCombatTurnReviewV1>::new();
    for (observed, traced) in line.steps.iter().zip(steps) {
        if reviews.last().map(|review| review.turn) != Some(traced.turn) {
            let state = combat_observation_v1(&observed.source_decision)?;
            reviews.push(RunPlayCombatTurnReviewV1 {
                turn: traced.turn,
                first_step: traced.step,
                last_step: traced.step,
                actions: Vec::new(),
                start: combat_turn_start_v1(state),
                end_turn: None,
                observed_player_hp_delta: 0,
                observed_player_max_hp_delta: 0,
                end_boundary: traced.delta.next_boundary,
            });
        }
        let review = reviews
            .last_mut()
            .expect("a turn review was created for the traced step");
        review.last_step = traced.step;
        review.actions.push(traced.action.clone());
        if traced.action == "end" {
            if review.end_turn.is_some() {
                return Err(format!(
                    "combat review turn {} contains multiple end-turn actions",
                    traced.turn
                ));
            }
            review.end_turn = Some(RunPlayCombatEndTurnReviewV1 {
                visible_incoming_damage: traced.visible_incoming_damage_before_action,
                player_block: traced.player_block_before_action,
                observed_player_hp_delta: traced.delta.player_hp,
            });
        }
        review.observed_player_hp_delta += traced.delta.player_hp;
        review.observed_player_max_hp_delta += traced.delta.player_max_hp;
        review.end_boundary = traced.delta.next_boundary;
    }
    Ok(reviews)
}

fn replay_for_candidate_v1<'a>(
    artifact: &'a RunPlayCombatResearchArtifactV1,
    candidate_id: &str,
) -> Result<&'a AgentCombatResearchReplayProofV1, String> {
    artifact
        .candidate_replay_proofs
        .iter()
        .find(|proof| proof.candidate_id == candidate_id)
        .ok_or_else(|| format!("combat trace artifact has no replay for {candidate_id}"))
}

fn validate_replayed_line_v1(
    proof: &AgentCombatResearchReplayProofV1,
    line: &sts_oracle_learning::eval::run_control::ExactRunPlayProbeLineV1,
) -> Result<(), String> {
    if line.steps.len() != proof.steps.len() {
        return Err("combat trace replay length disagrees with artifact proof".to_string());
    }
    for (index, (observed, recorded)) in line.steps.iter().zip(&proof.steps).enumerate() {
        if observed.source_decision.information.snapshot_id != recorded.source_public_snapshot_id {
            return Err(format!(
                "combat trace step {} source snapshot disagrees with artifact proof",
                index + 1
            ));
        }
    }
    let last = line
        .steps
        .last()
        .ok_or_else(|| "combat trace replay is empty".to_string())?;
    match (&proof.endpoint, &last.next_decision) {
        (
            AgentCombatResearchReplayEndpointV1::PublicDecision { public_snapshot_id },
            Some(decision),
        ) if decision.information.snapshot_id == *public_snapshot_id => Ok(()),
        (AgentCombatResearchReplayEndpointV1::GameTerminal { outcome }, None)
            if last.outcome == *outcome =>
        {
            Ok(())
        }
        _ => Err("combat trace endpoint disagrees with artifact proof".to_string()),
    }
}

pub(crate) fn trace_step_v1(
    step: usize,
    action_label: &str,
    observed: &sts_oracle_learning::eval::run_control::ExactRunPlayProbeStepV1,
) -> Result<RunPlayCombatTraceStepV1, String> {
    let before = combat_observation_v1(&observed.source_decision)?;
    let after_combat = observed
        .next_decision
        .as_ref()
        .and_then(combat_observation_optional_v1);
    let before_player = player_boundary_from_combat_v1(before);
    let successors = monster_successors_v1(
        before,
        after_combat,
        &observed.monster_entities_before,
        &observed.monster_entities_after,
    )?;
    let after_player = observed
        .next_decision
        .as_ref()
        .and_then(player_boundary_v1)
        .unwrap_or(before_player);
    let next_boundary = match observed
        .next_decision
        .as_ref()
        .map(|decision| &decision.observation)
    {
        Some(TaskPublicObservationV1::Combat { .. }) => RunPlayCombatNextBoundaryV1::Combat,
        Some(TaskPublicObservationV1::Strategic { .. }) => RunPlayCombatNextBoundaryV1::Strategic,
        None => RunPlayCombatNextBoundaryV1::GameTerminal,
    };
    let atomic = match &observed.selected_candidate {
        TaskPublicCandidateV1::CombatAtomic { action } => Some(action),
        TaskPublicCandidateV1::CombatIndexedChoice { action, .. } => Some(action),
        _ => None,
    };
    let card = match atomic {
        Some(PublicCombatAtomicActionV1::PlayCard {
            hand_index,
            target_monster_index,
        }) => {
            let played = before.cards.hand.cards.get(*hand_index).ok_or_else(|| {
                format!("combat trace step {step} play-card hand index is out of range")
            })?;
            Some(RunPlayPlayedCardV1 {
                card_id: played.card_id,
                upgrades: played.upgrades,
                target_slot: target_monster_index
                    .and_then(|index| before.monsters.get(index))
                    .map(|monster| monster.slot),
            })
        }
        _ => None,
    };
    let potion = match atomic {
        Some(PublicCombatAtomicActionV1::UsePotion {
            potion_index,
            target_monster_index,
        }) => {
            let potion_id = before
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| format!("combat trace step {step} potion slot is empty"))?
                .potion_id;
            Some(RunPlayPotionUseV1 {
                step,
                turn: before.turn.turn_count,
                slot: *potion_index,
                potion_id,
                target_slot: target_monster_index
                    .and_then(|index| before.monsters.get(index))
                    .map(|monster| monster.slot),
                observed_player_hp_delta: after_player.hp - before_player.hp,
                observed_player_block_delta: after_player.block - before_player.block,
                observed_energy_delta: after_player.energy.unwrap_or(0)
                    - before_player.energy.unwrap_or(0),
            })
        }
        _ => None,
    };
    let feed = match (&card, atomic) {
        (
            Some(card),
            Some(PublicCombatAtomicActionV1::PlayCard {
                target_monster_index: Some(target_index),
                ..
            }),
        ) if card.card_id == CardId::Feed => {
            let target = before
                .monsters
                .get(*target_index)
                .ok_or_else(|| format!("combat trace step {step} Feed target is out of range"))?;
            let after_target = successors[*target_index];
            let max_hp_gain = after_player.max_hp - before_player.max_hp;
            let fatal_payoff_observed = max_hp_gain > 0;
            Some(RunPlayFeedAttemptV1 {
                step,
                turn: before.turn.turn_count,
                target_slot: target.slot,
                target: target.enemy,
                target_hp_before: target.hp,
                target_block_before: target.block,
                target_hp_after: after_target.map(|monster| monster.hp),
                target_alive_after: after_target.map(|monster| monster.alive),
                fatal_payoff_observed,
                max_hp_gain,
                remaining_hp_after_nonfatal: after_target
                    .filter(|monster| monster.alive && !fatal_payoff_observed)
                    .map(|monster| monster.hp.max(0)),
            })
        }
        _ => None,
    };

    let display = selected_action_display(
        &observed.selected_candidate,
        &observed.choice.ordinal_path,
        card.as_ref(),
        potion.as_ref(),
    )?;
    Ok(RunPlayCombatTraceStepV1 {
        step,
        turn: before.turn.turn_count,
        action: action_label.to_string(),
        label: crate::combat_reading::action_label(&display, sts_presentation::Locale::En),
        display,
        player_block_before_action: before.player.block,
        visible_incoming_damage_before_action: visible_incoming_damage_v1(before),
        card,
        potion,
        feed,
        delta: boundary_delta_v1(
            before,
            after_combat,
            &successors,
            before_player,
            after_player,
            next_boundary,
        ),
        source_public_snapshot_id: observed.source_decision.information.snapshot_id.clone(),
        next_public_snapshot_id: observed
            .next_decision
            .as_ref()
            .map(|decision| decision.information.snapshot_id.clone()),
    })
}

pub(crate) fn selected_action_display(
    candidate: &TaskPublicCandidateV1,
    ordinal_path: &[usize],
    card: Option<&RunPlayPlayedCardV1>,
    potion: Option<&RunPlayPotionUseV1>,
) -> Result<RunPlayCombatActionDisplayV1, String> {
    use sts_oracle_learning::agent::information::action::{
        PublicCombatAtomicActionV1 as Atomic, PublicCombatIndexedChoiceCandidateV1 as Indexed,
        PublicCombatSelectionDomainCandidateV1 as Domain,
    };
    use RunPlayCombatActionDisplayV1 as Display;
    if let Some(card) = card {
        return Ok(Display::PlayCard(*card));
    }
    if let Some(potion) = potion {
        return Ok(Display::UsePotion {
            slot: potion.slot,
            potion_id: potion.potion_id,
            target_slot: potion.target_slot,
        });
    }
    Ok(match candidate {
        TaskPublicCandidateV1::CombatAtomic {
            action: Atomic::EndTurn,
        } => Display::EndTurn,
        TaskPublicCandidateV1::CombatAtomic {
            action: Atomic::Proceed,
        } => Display::Proceed,
        TaskPublicCandidateV1::CombatAtomic {
            action: Atomic::Cancel,
        } => Display::Cancel,
        TaskPublicCandidateV1::CombatAtomic {
            action: Atomic::DiscardPotion { potion_index },
        } => Display::DiscardPotion {
            slot: *potion_index,
        },
        TaskPublicCandidateV1::CombatIndexedChoice { candidate, .. } => match candidate {
            Indexed::Card { card_id, upgrades } => Display::ChooseCard {
                card_id: *card_id,
                upgrades: *upgrades,
            },
            Indexed::Stance { stance } => Display::ChooseStance(*stance),
        },
        TaskPublicCandidateV1::CombatSelectionFamily { family } => Display::SelectCards(
            ordinal_path
                .iter()
                .skip(1)
                .map(|index| {
                    let item = family
                        .domain
                        .get(*index)
                        .ok_or("Review selection outside its domain")?;
                    Ok(match item {
                        Domain::Card {
                            card_id, upgrades, ..
                        } => (*card_id, upgrades.unwrap_or(0)),
                        Domain::Scry { card_id, .. } => (*card_id, 0),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        ),
        _ => return Err("Review action lacks a typed display projection".into()),
    })
}

/// Matches public boundaries using correspondence from their actual execution
/// fork. Neither slot nor species identifies a monster across a summon.
fn monster_successors_v1<'a>(
    before: &PublicCombatStateV1,
    after: Option<&'a PublicCombatStateV1>,
    entities_before: &[(u8, sts_oracle_learning::EntityId)],
    entities_after: &[(u8, sts_oracle_learning::EntityId)],
) -> Result<Vec<Option<&'a CombatLearningMonsterStateV1>>, String> {
    before
        .monsters
        .iter()
        .map(|monster| {
            let entity_id = entities_before
                .iter()
                .find(|(slot, _)| *slot == monster.slot)
                .map(|(_, id)| *id)
                .ok_or_else(|| {
                    format!(
                        "combat trace lacks runtime identity for source M{}",
                        monster.slot
                    )
                })?;
            let Some(after) = after else {
                return Ok(None);
            };
            let Some((slot, _)) = entities_after.iter().find(|(_, id)| *id == entity_id) else {
                return Ok(None);
            };
            after
                .monsters
                .iter()
                .find(|successor| successor.slot == *slot)
                .map(Some)
                .ok_or_else(|| format!("combat trace lacks public successor for runtime M{slot}"))
        })
        .collect()
}

fn boundary_delta_v1(
    before: &PublicCombatStateV1,
    after: Option<&PublicCombatStateV1>,
    successors: &[Option<&CombatLearningMonsterStateV1>],
    before_player: PublicPlayerBoundaryV1,
    after_player: PublicPlayerBoundaryV1,
    next_boundary: RunPlayCombatNextBoundaryV1,
) -> RunPlayCombatBoundaryDeltaV1 {
    let after_potions = after.map(|state| &state.potions);
    let potion_slots_changed = (0..before.potions.len().max(after_potions.map_or(0, Vec::len)))
        .filter(|index| {
            before
                .potions
                .get(*index)
                .and_then(Option::as_ref)
                .map(|p| p.potion_id)
                != after_potions
                    .and_then(|potions| potions.get(*index))
                    .and_then(Option::as_ref)
                    .map(|p| p.potion_id)
        })
        .collect();
    let cards_added_to_exhaust = after
        .map(|state| {
            card_multiset_added_v1(&before.cards.exhaust.cards, &state.cards.exhaust.cards)
        })
        .unwrap_or_default();
    let player_power_changes = after
        .map(|state| power_deltas_v1(&before.player.powers, &state.player.powers))
        .unwrap_or_default();
    let monster_changes = before
        .monsters
        .iter()
        .zip(successors)
        .filter_map(|(monster, successor)| {
            let hp = successor.map_or(-monster.hp, |next| next.hp - monster.hp);
            let block = successor.map_or(-monster.block, |next| next.block - monster.block);
            let alive_after = successor.map(|next| next.alive);
            (hp != 0 || block != 0 || alive_after != Some(monster.alive)).then_some(
                RunPlayMonsterDeltaV1 {
                    slot: monster.slot,
                    enemy: monster.enemy,
                    hp,
                    block,
                    alive_before: monster.alive,
                    alive_after,
                },
            )
        })
        .collect();
    RunPlayCombatBoundaryDeltaV1 {
        next_boundary,
        player_hp: after_player.hp - before_player.hp,
        player_max_hp: after_player.max_hp - before_player.max_hp,
        player_block: after_player.block - before_player.block,
        energy: after_player
            .energy
            .zip(before_player.energy)
            .map(|(a, b)| a - b),
        cards_played_this_turn: after_player
            .cards_played_this_turn
            .zip(before_player.cards_played_this_turn)
            .map(|(a, b)| a - b),
        potion_slots_changed,
        cards_added_to_exhaust,
        player_power_changes,
        monster_changes,
    }
}

fn card_multiset_added_v1(
    before: &[sts_oracle_learning::agent::information::state::CombatLearningCardV1],
    after: &[sts_oracle_learning::agent::information::state::CombatLearningCardV1],
) -> Vec<CardId> {
    let mut counts = Vec::<((CardId, u8), usize)>::new();
    for card in before {
        let identity = (card.card_id, card.upgrades);
        match counts
            .iter_mut()
            .find(|(candidate, _)| *candidate == identity)
        {
            Some((_, count)) => *count += 1,
            None => counts.push((identity, 1)),
        }
    }
    let mut added = Vec::new();
    for card in after {
        let identity = (card.card_id, card.upgrades);
        let count = counts
            .iter_mut()
            .find(|(candidate, _)| *candidate == identity)
            .map(|(_, count)| count);
        if count.as_deref().copied().unwrap_or(0) == 0 {
            added.push(card.card_id);
        } else if let Some(count) = count {
            *count -= 1;
        }
    }
    added
}

fn power_deltas_v1(
    before: &[CombatLearningPowerV1],
    after: &[CombatLearningPowerV1],
) -> Vec<RunPlayPowerDeltaV1> {
    let mut amounts = Vec::<(PowerId, (i32, i32))>::new();
    for power in before {
        match amounts
            .iter_mut()
            .find(|(candidate, _)| *candidate == power.power)
        {
            Some((_, amount)) => amount.0 += power.amount,
            None => amounts.push((power.power, (power.amount, 0))),
        }
    }
    for power in after {
        match amounts
            .iter_mut()
            .find(|(candidate, _)| *candidate == power.power)
        {
            Some((_, amount)) => amount.1 += power.amount,
            None => amounts.push((power.power, (0, power.amount))),
        }
    }
    amounts.sort_by_key(|(power, _)| power.stable_ordinal());
    amounts
        .into_iter()
        .filter_map(|(power, (before, after))| {
            (before != after).then_some(RunPlayPowerDeltaV1 {
                power,
                before,
                after,
            })
        })
        .collect()
}

fn combat_observation_v1(decision: &TaskPublicDecisionV1) -> Result<&PublicCombatStateV1, String> {
    combat_observation_optional_v1(decision)
        .ok_or_else(|| "combat trace step source is not a combat decision".to_string())
}

fn combat_observation_optional_v1(decision: &TaskPublicDecisionV1) -> Option<&PublicCombatStateV1> {
    match &decision.observation {
        TaskPublicObservationV1::Combat { observation, .. } => Some(observation),
        TaskPublicObservationV1::Strategic { .. } => None,
    }
}

fn player_boundary_from_combat_v1(state: &PublicCombatStateV1) -> PublicPlayerBoundaryV1 {
    PublicPlayerBoundaryV1 {
        hp: state.player.hp,
        max_hp: state.player.max_hp,
        block: state.player.block,
        energy: Some(i32::from(state.turn.energy)),
        cards_played_this_turn: Some(i32::from(state.turn.counters.cards_played_this_turn)),
    }
}

fn player_boundary_v1(decision: &TaskPublicDecisionV1) -> Option<PublicPlayerBoundaryV1> {
    match &decision.observation {
        TaskPublicObservationV1::Combat { observation, .. } => {
            Some(player_boundary_from_combat_v1(observation))
        }
        TaskPublicObservationV1::Strategic { observation } => Some(PublicPlayerBoundaryV1 {
            hp: observation.run.current_hp,
            max_hp: observation.run.max_hp,
            block: 0,
            energy: None,
            cards_played_this_turn: None,
        }),
    }
}

pub fn render_combat_trace_v1(trace: &RunPlayCombatTraceV1) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "combat_trace root={} candidates={} durable_head_unchanged={}",
        trace.root_archive_node_id,
        trace.candidates.len(),
        trace.durable_head_unchanged
    )
    .expect("string write");
    for candidate in &trace.candidates {
        writeln!(
            output,
            "candidate={} final={}/{} potions={} actions={} feed_attempts={}",
            candidate.candidate_index,
            candidate.final_hp,
            candidate.final_max_hp,
            candidate.total_potions_consumed,
            candidate.action_count,
            candidate.feed_attempts.len()
        )
        .expect("string write");
        for feed in &candidate.feed_attempts {
            writeln!(
                output,
                "  Feed step={} turn={} target_slot={} before={}+b{} after={} fatal={} max_hp_gain={} remaining={}",
                feed.step,
                feed.turn,
                feed.target_slot,
                feed.target_hp_before,
                feed.target_block_before,
                feed.target_hp_after
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "terminal".to_string()),
                feed.fatal_payoff_observed,
                feed.max_hp_gain,
                feed.remaining_hp_after_nonfatal
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            )
            .expect("string write");
        }
        for review in &candidate.turn_reviews {
            writeln!(
                output,
                "  turn={} start={}/{} block={} energy={} incoming={} powers=[{}]",
                review.turn,
                review.start.player_hp,
                review.start.player_max_hp,
                review.start.player_block,
                review.start.energy,
                review
                    .start
                    .visible_incoming_damage
                    .map(|damage| damage.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                render_trace_powers_v1(&review.start.player_powers),
            )
            .expect("string write");
            writeln!(
                output,
                "    hand=[{}]",
                review
                    .start
                    .hand
                    .iter()
                    .map(render_trace_card_v1)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string write");
            for monster in &review.start.monsters {
                writeln!(
                    output,
                    "    enemy={} hp={}/{} block={} intent={:?}/{} powers=[{}]",
                    monster.slot,
                    monster.hp,
                    monster.max_hp,
                    monster.block,
                    monster.intent.intent,
                    monster
                        .intent
                        .preview_damage_per_hit
                        .map(|damage| damage.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    render_trace_powers_v1(&monster.powers),
                )
                .expect("string write");
            }
            for step in candidate
                .steps
                .iter()
                .filter(|step| step.step >= review.first_step && step.step <= review.last_step)
            {
                writeln!(
                    output,
                    "    step={} action={} effects=[{}]",
                    step.step,
                    step.action,
                    render_trace_effects_v1(step),
                )
                .expect("string write");
            }
            writeln!(
                output,
                "    result hp={:+} max_hp={:+} next={:?}",
                review.observed_player_hp_delta,
                review.observed_player_max_hp_delta,
                review.end_boundary,
            )
            .expect("string write");
        }
    }
    output
}

fn render_trace_card_v1(card: &RunPlayCombatCardSummaryV1) -> String {
    format!(
        "{:?}{}(cost={},damage={},block={})",
        card.card_id,
        if card.upgrades > 0 { "+" } else { "" },
        card.effective_cost,
        card.current_damage,
        card.current_block,
    )
}

fn render_trace_powers_v1(powers: &[CombatLearningPowerV1]) -> String {
    powers
        .iter()
        .map(|power| {
            if power.extra_data == 0 {
                format!("{:?}:{}", power.power, power.amount)
            } else {
                format!(
                    "{:?}:{}(extra={})",
                    power.power, power.amount, power.extra_data
                )
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn render_trace_effects_v1(step: &RunPlayCombatTraceStepV1) -> String {
    let delta = &step.delta;
    let mut effects = Vec::new();
    if step.action == "end" {
        effects.push(format!(
            "end_turn_incoming={},block_before={}",
            step.visible_incoming_damage_before_action
                .map(|damage| damage.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            step.player_block_before_action,
        ));
    }
    if delta.player_hp != 0 {
        if matches!(delta.next_boundary, RunPlayCombatNextBoundaryV1::Strategic)
            && delta.player_hp > 0
        {
            effects.push(format!("post_combat_hp={:+}", delta.player_hp));
        } else {
            effects.push(format!("hp={:+}", delta.player_hp));
        }
    }
    if delta.player_max_hp != 0 {
        effects.push(format!("max_hp={:+}", delta.player_max_hp));
    }
    if delta.player_block != 0 {
        effects.push(format!("block={:+}", delta.player_block));
    }
    if let Some(energy) = delta.energy.filter(|energy| *energy != 0) {
        effects.push(format!("energy={energy:+}"));
    }
    for power in &delta.player_power_changes {
        effects.push(format!(
            "{:?}={}->{}",
            power.power, power.before, power.after
        ));
    }
    for monster in &delta.monster_changes {
        if !monster.alive_before && monster.alive_after.is_none() {
            continue;
        }
        let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
        let mut change = format!(
            "enemy{}({})",
            monster.slot,
            sts_oracle_learning::content::identity::gameplay_display_label_v1(enemy_id)
        );
        if monster.hp != 0 {
            write!(change, ":hp={:+}", monster.hp).expect("string write");
        }
        if monster.block != 0 {
            write!(change, ":block={:+}", monster.block).expect("string write");
        }
        if monster.alive_after != Some(monster.alive_before) {
            if monster.alive_after.is_none()
                && matches!(delta.next_boundary, RunPlayCombatNextBoundaryV1::Strategic)
            {
                change.push_str(":defeated=true");
            } else {
                write!(change, ":alive={:?}", monster.alive_after).expect("string write");
            }
        }
        effects.push(change);
    }
    if !delta.cards_added_to_exhaust.is_empty() {
        effects.push(format!("exhaust={:?}", delta.cards_added_to_exhaust));
    }
    if effects.is_empty() {
        effects.push("no_public_resource_change".to_string());
    }
    effects.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::agent::information::state::public_combat_state_v1;
    use sts_oracle_learning::content::monsters::EnemyId;
    use sts_oracle_learning::test_support::{blank_test_combat, test_monster};

    #[test]
    fn action_display_preserves_selected_order_unknown_cards_and_upgrade_identity() {
        use sts_oracle_learning::agent::information::action::{
            PublicCombatSelectionDistinctByV1, PublicCombatSelectionDomainCandidateV1 as Domain,
            PublicCombatSelectionFamilyV1,
        };
        use sts_oracle_learning::sim::combat_action_surface::{
            CombatSelectionInputEncodingV2, CombatSelectionReasonV2,
        };
        let mut candidate = TaskPublicCandidateV1::CombatSelectionFamily {
            family: PublicCombatSelectionFamilyV1 {
                input_encoding: CombatSelectionInputEncodingV2::SubmitScryDiscardIndices,
                reason: CombatSelectionReasonV2::ScryDiscard,
                hand_copy_projection: None,
                source_pile: None,
                domain: vec![
                    Domain::Scry {
                        index: 5,
                        card_id: None,
                        currently_present: false,
                    },
                    Domain::Scry {
                        index: 7,
                        card_id: Some(CardId::SearingBlow),
                        currently_present: true,
                    },
                ],
                raw_domain_count: 2,
                eligible_domain_count: 2,
                max_distinct_selection_count: 4,
                declared_min: 0,
                declared_max: 2,
                effective_max: 2,
                distinct_by: PublicCombatSelectionDistinctByV1::ScryOccurrence,
            },
        };
        assert_eq!(
            selected_action_display(&candidate, &[9, 1, 0], None, None).unwrap(),
            RunPlayCombatActionDisplayV1::SelectCards(vec![
                (Some(CardId::SearingBlow), 0),
                (None, 0)
            ])
        );
        assert_eq!(
            selected_action_display(&candidate, &[9], None, None).unwrap(),
            RunPlayCombatActionDisplayV1::SelectCards(vec![])
        );
        assert!(selected_action_display(&candidate, &[9, 2], None, None)
            .unwrap_err()
            .contains("outside its domain"));
        let TaskPublicCandidateV1::CombatSelectionFamily { family } = &mut candidate else {
            unreachable!()
        };
        family.input_encoding = CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages;
        family.reason = CombatSelectionReasonV2::Grid(
            sts_oracle_learning::state::core::GridSelectReason::DiscardToHand,
        );
        family.source_pile = Some(sts_oracle_learning::state::core::PileType::Discard);
        family.domain = vec![Domain::Card {
            ordinal: 0,
            card_id: Some(CardId::SearingBlow),
            upgrades: Some(3),
            eligible: true,
        }];
        family.raw_domain_count = 1;
        family.eligible_domain_count = 1;
        family.max_distinct_selection_count = 1;
        family.declared_min = 1;
        family.declared_max = 1;
        family.effective_max = 1;
        family.distinct_by = PublicCombatSelectionDistinctByV1::CardOccurrence;
        assert_eq!(
            selected_action_display(&candidate, &[9, 0], None, None).unwrap(),
            RunPlayCombatActionDisplayV1::SelectCards(vec![(Some(CardId::SearingBlow), 3)])
        );
    }

    #[test]
    fn successor_uses_entity_identity_when_summoning_shifts_slots() {
        let mut before = blank_test_combat();
        let mut collector = test_monster(EnemyId::TheCollector);
        collector.id = 10;
        collector.slot = 0;
        collector.current_hp = 300;
        before.entities.monsters = vec![collector.clone()];
        let mut after = before.clone();
        let mut torch = test_monster(EnemyId::TorchHead);
        torch.id = 11;
        torch.slot = 0;
        torch.current_hp = 43;
        collector.slot = 1;
        collector.current_hp = 292;
        after.entities.monsters = vec![torch, collector];
        let before = public_combat_state_v1(&before);
        let after = public_combat_state_v1(&after);
        let successors =
            monster_successors_v1(&before, Some(&after), &[(0, 10)], &[(0, 11), (1, 10)]).unwrap();
        assert_eq!(successors[0].unwrap().slot, 1);
        assert_eq!(successors[0].unwrap().hp, 292);
        let delta = boundary_delta_v1(
            &before,
            Some(&after),
            &successors,
            player_boundary_from_combat_v1(&before),
            player_boundary_from_combat_v1(&after),
            RunPlayCombatNextBoundaryV1::Combat,
        );
        assert_eq!(delta.monster_changes.len(), 1);
        assert_eq!(delta.monster_changes[0].hp, -8);
    }

    #[test]
    fn same_species_and_slot_do_not_bind_a_replacement_monster() {
        let mut state = blank_test_combat();
        let mut torch = test_monster(EnemyId::TorchHead);
        torch.slot = 0;
        state.entities.monsters = vec![torch];
        let before = public_combat_state_v1(&state);
        let after = before.clone();
        let successors =
            monster_successors_v1(&before, Some(&after), &[(0, 10)], &[(0, 11)]).unwrap();
        assert!(successors[0].is_none());
        let error = monster_successors_v1(&before, Some(&after), &[], &[(0, 11)]).unwrap_err();
        assert!(error.contains("lacks runtime identity"));
    }
}
