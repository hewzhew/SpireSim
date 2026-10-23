use std::path::Path;

use serde::{de::IgnoredAny, Deserialize, Serialize};
use sts_combat_knowledge::feed_payoff_guide_policy_v1;
use sts_combat_planner::{
    combat_plan_state_guide_policy_v1, evaluate_combat_trajectory_query_v1,
    exact_trajectory_potion_expenditures, materialize_exact_action_line,
    replay_combat_trajectory_events_v1, summarize_oracle_combat_witness_outcome,
    CombatDecisionRoot, CombatTrajectoryAtomV1, CombatTrajectoryEventV1,
    CombatTrajectoryQueryEvaluationV1, CombatTrajectoryQueryV1,
    LocalTurnGraphRootActionFamilySnapshot, LocalTurnGraphSearchSpecV1,
    LocalTurnGraphTerminalOutcomeSnapshotV1, LocalTurnGraphWitnessInterruption,
    LocalTurnGraphWitnessSession, LocalTurnGraphWitnessStatus, OracleCombatWitnessSatisfaction,
    TurnOptionAction,
};
use sts_oracle_learning::agent::task::{
    TaskEpisodeOutcomeV1, TaskPrivilegedSupportAccessV1, TaskPrivilegedSupportProvenanceV1,
    TaskResearchPlanOutcomeV1,
};
use sts_oracle_learning::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_learning::sim::combat::EngineCombatStepper;

use crate::agent_protocol::{AgentCombatResearchReplayEndpointV1, AgentCombatResearchReplayStepV1};
use crate::driver::{RunPlayActionLineProfileV1, RunPlayControllerV1};
use crate::model::RunPlayCombatResearchCandidateV1;
use crate::proposal_audit::resolve_target_inputs_v1;
use crate::selector::{
    parse_action_selector_v1, semantic_action_for_choice_v1, RunPlayActionSelectorV1,
};
use crate::storage::{read_bounded, DEFAULT_ARTIFACT_MAX_BYTES};

pub const COMBAT_TRAJECTORY_QUERY_SCHEMA_V2: &str = "sts-run-play-combat-trajectory-query-v2";
const COMBAT_TRAJECTORY_QUERY_PRODUCER_V2: &str = "run_play_combat_trajectory_query_v2";

#[derive(Clone, Debug)]
pub struct CombatTrajectoryExperimentOptionsV1 {
    pub query: CombatTrajectoryQueryV1,
    pub max_generation_work: usize,
    pub max_selections: usize,
    pub wall_ms: u64,
    pub max_turn_depth: usize,
    pub max_potions_used: Option<u32>,
    pub allow_potion_discard: bool,
    pub allowed_potion_slots: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTrajectoryQueryV2 {
    pub schema: String,
    pub session_id: String,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub root_exact_state_hash: String,
    pub query: CombatTrajectoryQueryV1,
    pub guidance: Vec<RunPlayCombatTrajectoryQueryGuidanceV1>,
    pub search_spec: LocalTurnGraphSearchSpecV1,
    pub bounded_search_only: bool,
    pub absence_means_unknown: bool,
    pub status: RunPlayCombatTrajectoryQueryStatusV2,
    pub exit: RunPlayCombatTrajectoryQueryExitV2,
    pub work: RunPlayCombatTrajectoryQueryWorkV2,
    pub exact_witness_frontier: Vec<RunPlayCombatTrajectoryQueryWitnessV2>,
    pub frontier_commit_proofs: Vec<RunPlayCombatTrajectoryQueryFrontierCommitProofV2>,
    pub selected_witness: Option<RunPlayCombatTrajectoryQueryWitnessV2>,
    pub selected_commit_proof: Option<RunPlayCombatTrajectoryQueryCommitProofV2>,
    pub durable_head_unchanged: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatTrajectoryQueryGuidanceV1 {
    GenericCombatPlanState,
    FeedPayoff,
}

fn combat_trajectory_query_guidance_v1(
    query: &CombatTrajectoryQueryV1,
) -> Vec<RunPlayCombatTrajectoryQueryGuidanceV1> {
    let mut guidance = vec![RunPlayCombatTrajectoryQueryGuidanceV1::GenericCombatPlanState];
    if query.contains_atom(CombatTrajectoryAtomV1::CardLethalPayoff {
        card_id: sts_oracle_learning::content::cards::CardId::Feed,
    }) {
        guidance.push(RunPlayCombatTrajectoryQueryGuidanceV1::FeedPayoff);
    }
    guidance
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatTrajectoryQueryStatusV2 {
    VerifiedWitness,
    UnknownInBoundedSearch,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatTrajectoryQueryExitV2 {
    WitnessFound,
    SelectionBudget,
    GenerationWorkBudget,
    EngineStepBudget,
    Deadline,
    FrontierExhausted,
    MechanicsGap,
    ReplayMismatch { detail: String },
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTrajectoryQueryWorkV2 {
    pub selections: usize,
    pub generation_work: usize,
    pub engine_steps: usize,
    pub exact_nodes: usize,
    pub exact_edges: usize,
    pub completed_turn_options: usize,
    pub terminal_win_options: usize,
    pub query_rejected_terminal_wins: usize,
    pub root_action_families: Vec<LocalTurnGraphRootActionFamilySnapshot>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTrajectoryQueryWitnessV2 {
    pub selected_by_local_hp_view: bool,
    pub query_evaluation: CombatTrajectoryQueryEvaluationV1,
    pub outcome: LocalTurnGraphTerminalOutcomeSnapshotV1,
    pub final_hp: i32,
    pub final_max_hp: i32,
    pub potion_expenditures: u32,
    pub semantic_action_line: Vec<String>,
    pub exact_actions: Vec<TurnOptionAction>,
    pub events: Vec<CombatTrajectoryEventV1>,
}

/// Root-bound, public replay authority for the selected query witness. Exact
/// actions remain report diagnostics; this proof is the only execution input
/// accepted by `anchor-combat-query`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTrajectoryQueryCommitProofV2 {
    pub semantic_action_line: Vec<String>,
    pub replay_steps: Vec<AgentCombatResearchReplayStepV1>,
    pub endpoint: AgentCombatResearchReplayEndpointV1,
    pub public_outcome: TaskResearchPlanOutcomeV1,
    pub combat_terminal_hp: i32,
    pub combat_terminal_max_hp: i32,
    pub potion_expenditures: u32,
    pub exact_action_count: usize,
}

/// Replay authority for one zero-based member of `exact_witness_frontier`.
/// Keeping the index beside the proof prevents callers from treating the
/// local-HP-selected witness as the only actionable resource frontier member.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatTrajectoryQueryFrontierCommitProofV2 {
    pub candidate_index: usize,
    pub proof: RunPlayCombatTrajectoryQueryCommitProofV2,
}

/// The strict actionable subset of a V2 query artifact. Diagnostic search
/// details are deliberately not deserialized into execution authority.
pub struct RunPlayCombatTrajectoryQueryCommitArtifactV2 {
    pub session_id: String,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub root_exact_state_hash: String,
    pub query: CombatTrajectoryQueryV1,
    pub proof: RunPlayCombatTrajectoryQueryCommitProofV2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // The loader names and discards non-authoritative diagnostics.
struct RunPlayCombatTrajectoryQueryArtifactEnvelopeV2 {
    schema: String,
    session_id: String,
    root_archive_node_id: String,
    root_public_snapshot_id: String,
    root_exact_state_hash: String,
    query: CombatTrajectoryQueryV1,
    #[serde(default)]
    guidance: Vec<RunPlayCombatTrajectoryQueryGuidanceV1>,
    search_spec: IgnoredAny,
    bounded_search_only: bool,
    absence_means_unknown: bool,
    status: RunPlayCombatTrajectoryQueryStatusV2,
    exit: IgnoredAny,
    work: IgnoredAny,
    exact_witness_frontier: IgnoredAny,
    #[serde(default)]
    frontier_commit_proofs: Vec<RunPlayCombatTrajectoryQueryFrontierCommitProofV2>,
    selected_witness: IgnoredAny,
    selected_commit_proof: Option<RunPlayCombatTrajectoryQueryCommitProofV2>,
    durable_head_unchanged: bool,
}

pub fn run_current_combat_trajectory_query_v2(
    controller: &mut RunPlayControllerV1,
    options: CombatTrajectoryExperimentOptionsV1,
) -> Result<RunPlayCombatTrajectoryQueryV2, String> {
    let head_before = controller.head();
    let session_id = head_before.session_id.clone();
    let head_revision = head_before.head_revision;
    let root_archive_node_id = head_before.position.archive_node_id().to_string();
    let root_public_snapshot_id = head_before
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat query requires a current public decision".to_string())?
        .public_snapshot_id
        .clone();
    let root_position = controller.current_combat_position_for_lab_v1()?;
    let root = CombatDecisionRoot::new(root_position.clone())
        .map_err(|error| format!("combat query requires a stable combat root: {error:?}"))?;
    let root_exact_state_hash = root.exact_state_hash().to_string();
    let search_spec = LocalTurnGraphSearchSpecV1::from_controls(
        options.max_generation_work,
        options.max_selections,
        options.wall_ms,
        250,
        50_000,
        4,
        options.max_turn_depth,
        options.max_potions_used,
        options.allow_potion_discard,
        options.allowed_potion_slots,
        None,
        None,
    );
    let base_policy = combat_plan_state_guide_policy_v1(retained_combat_tactical_policy_v1());
    let guidance = combat_trajectory_query_guidance_v1(&options.query);
    let policy = if guidance.contains(&RunPlayCombatTrajectoryQueryGuidanceV1::FeedPayoff) {
        feed_payoff_guide_policy_v1(base_policy, root_position.combat.entities.player.max_hp)
    } else {
        base_policy
    };
    let mut search = LocalTurnGraphWitnessSession::with_policy_and_trajectory_query(
        root,
        search_spec.planner_config(OracleCombatWitnessSatisfaction::BudgetOrExhaustion),
        policy,
        &options.query,
    )
    .map_err(|error| format!("invalid combat trajectory query: {error}"))?;
    let report = search.advance(search_spec.quantum(), &EngineCombatStepper);
    let root_action_families = search.root_action_families();

    let selected_materialized = report
        .witness
        .as_ref()
        .map(|witness| {
            materialize_query_witness_v2(controller, &root_position, &options.query, witness, true)
        })
        .transpose()?;
    let selected_actions = report
        .witness
        .as_ref()
        .map(|witness| witness.actions.as_slice());
    let frontier_materialized = search
        .witness_frontier()
        .iter()
        .map(|witness| {
            if selected_actions.is_some_and(|selected| selected == witness.actions) {
                return selected_materialized.as_ref().cloned().ok_or_else(|| {
                    "combat query selected witness disappeared from its projection".to_string()
                });
            }
            materialize_query_witness_v2(controller, &root_position, &options.query, witness, false)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let exact_witness_frontier = frontier_materialized
        .iter()
        .map(|(witness, _)| witness.clone())
        .collect::<Vec<_>>();
    let frontier_commit_proofs = frontier_materialized
        .iter()
        .enumerate()
        .map(
            |(candidate_index, (_, proof))| RunPlayCombatTrajectoryQueryFrontierCommitProofV2 {
                candidate_index,
                proof: proof.clone(),
            },
        )
        .collect::<Vec<_>>();
    let (selected_witness, selected_commit_proof) = selected_materialized
        .map(|(witness, proof)| (Some(witness), Some(proof)))
        .unwrap_or((None, None));
    let exit = match &report.status {
        LocalTurnGraphWitnessStatus::WitnessFound => {
            RunPlayCombatTrajectoryQueryExitV2::WitnessFound
        }
        LocalTurnGraphWitnessStatus::Partial(
            LocalTurnGraphWitnessInterruption::SelectionBudget,
        ) => RunPlayCombatTrajectoryQueryExitV2::SelectionBudget,
        LocalTurnGraphWitnessStatus::Partial(
            LocalTurnGraphWitnessInterruption::GenerationWorkBudget,
        ) => RunPlayCombatTrajectoryQueryExitV2::GenerationWorkBudget,
        LocalTurnGraphWitnessStatus::Partial(
            LocalTurnGraphWitnessInterruption::EngineStepBudget,
        ) => RunPlayCombatTrajectoryQueryExitV2::EngineStepBudget,
        LocalTurnGraphWitnessStatus::Partial(LocalTurnGraphWitnessInterruption::Deadline) => {
            RunPlayCombatTrajectoryQueryExitV2::Deadline
        }
        LocalTurnGraphWitnessStatus::FrontierExhausted => {
            RunPlayCombatTrajectoryQueryExitV2::FrontierExhausted
        }
        LocalTurnGraphWitnessStatus::MechanicsGap => {
            RunPlayCombatTrajectoryQueryExitV2::MechanicsGap
        }
        LocalTurnGraphWitnessStatus::ReplayMismatch(error) => {
            RunPlayCombatTrajectoryQueryExitV2::ReplayMismatch {
                detail: format!("{error:?}"),
            }
        }
    };
    let durable_head_unchanged = controller.head().head_revision == head_revision
        && controller.head().position.archive_node_id() == root_archive_node_id
        && controller
            .head()
            .current_decision
            .as_ref()
            .is_some_and(|decision| decision.public_snapshot_id == root_public_snapshot_id);

    Ok(RunPlayCombatTrajectoryQueryV2 {
        schema: COMBAT_TRAJECTORY_QUERY_SCHEMA_V2.to_string(),
        session_id,
        root_archive_node_id,
        root_public_snapshot_id,
        root_exact_state_hash,
        query: options.query,
        guidance,
        search_spec,
        bounded_search_only: true,
        absence_means_unknown: true,
        status: if selected_witness.is_some() {
            RunPlayCombatTrajectoryQueryStatusV2::VerifiedWitness
        } else {
            RunPlayCombatTrajectoryQueryStatusV2::UnknownInBoundedSearch
        },
        exit,
        work: RunPlayCombatTrajectoryQueryWorkV2 {
            selections: report.counters.selections,
            generation_work: report.counters.generation_work,
            engine_steps: report.counters.engine_steps,
            exact_nodes: report.counters.exact_nodes,
            exact_edges: report.counters.exact_edges,
            completed_turn_options: report.counters.completed_turn_options,
            terminal_win_options: report.counters.terminal_win_options,
            query_rejected_terminal_wins: report.counters.trajectory_query_rejected_terminal_wins,
            root_action_families,
        },
        exact_witness_frontier,
        frontier_commit_proofs,
        selected_witness,
        selected_commit_proof,
        durable_head_unchanged,
    })
}

fn materialize_query_witness_v2(
    controller: &RunPlayControllerV1,
    root: &sts_oracle_learning::sim::combat::CombatPosition,
    query: &CombatTrajectoryQueryV1,
    witness: &sts_combat_planner::OracleCombatWitness,
    selected_by_local_hp_view: bool,
) -> Result<
    (
        RunPlayCombatTrajectoryQueryWitnessV2,
        RunPlayCombatTrajectoryQueryCommitProofV2,
    ),
    String,
> {
    let events = replay_combat_trajectory_events_v1(root, &witness.actions, &EngineCombatStepper)
        .map_err(|error| format!("combat query witness event replay failed: {error:?}"))?;
    let query_evaluation = evaluate_combat_trajectory_query_v1(query, &events)
        .map_err(|error| format!("combat query witness evaluation failed: {error}"))?;
    if !query_evaluation.satisfied {
        return Err("combat query frontier retained a witness which fails its query".to_string());
    }
    let inputs = witness
        .actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let provenance = combat_trajectory_query_provenance_v2();
    let (public_probe, public_outcome) =
        controller.exact_combat_replay_probe_v1(&inputs, provenance)?;
    let semantic_action_line = public_probe
        .steps
        .iter()
        .map(|step| semantic_action_for_choice_v1(&step.source_decision, &step.choice))
        .collect::<Result<Vec<_>, _>>()?;
    let replay_steps = public_probe
        .steps
        .iter()
        .zip(&semantic_action_line)
        .map(|(step, action)| AgentCombatResearchReplayStepV1 {
            source_public_snapshot_id: step.source_decision.information.snapshot_id.clone(),
            action: action.clone(),
        })
        .collect::<Vec<_>>();
    let final_step = public_probe
        .steps
        .last()
        .ok_or_else(|| "combat query public replay produced an empty proof".to_string())?;
    let endpoint = match &final_step.next_decision {
        Some(decision) => AgentCombatResearchReplayEndpointV1::PublicDecision {
            public_snapshot_id: decision.information.snapshot_id.clone(),
        },
        None => match &final_step.outcome {
            TaskEpisodeOutcomeV1::Paused { .. } => {
                return Err(
                    "combat query public replay paused without a successor decision".to_string(),
                );
            }
            outcome => AgentCombatResearchReplayEndpointV1::GameTerminal {
                outcome: outcome.clone(),
            },
        },
    };
    let potion_expenditures =
        exact_trajectory_potion_expenditures(root, &witness.actions, &witness.final_position);
    let combat_terminal_hp = witness.final_position.combat.entities.player.current_hp;
    let combat_terminal_max_hp = witness.final_position.combat.entities.player.max_hp;
    let proof = RunPlayCombatTrajectoryQueryCommitProofV2 {
        semantic_action_line: semantic_action_line.clone(),
        replay_steps,
        endpoint,
        public_outcome,
        combat_terminal_hp,
        combat_terminal_max_hp,
        potion_expenditures,
        exact_action_count: witness.actions.len(),
    };
    validate_commit_proof_v2(
        public_probe.steps[0]
            .source_decision
            .information
            .snapshot_id
            .as_str(),
        &proof,
    )?;
    let projected = RunPlayCombatTrajectoryQueryWitnessV2 {
        selected_by_local_hp_view,
        query_evaluation,
        outcome: summarize_oracle_combat_witness_outcome(root, witness, selected_by_local_hp_view),
        final_hp: combat_terminal_hp,
        final_max_hp: combat_terminal_max_hp,
        potion_expenditures,
        semantic_action_line,
        exact_actions: witness.actions.clone(),
        events,
    };
    Ok((projected, proof))
}

pub fn render_combat_trajectory_query_v2(report: &RunPlayCombatTrajectoryQueryV2) -> String {
    let mut output = format!(
        "combat_query status={:?} exit={:?} guidance={:?} root={} exact={} durable_head_unchanged={}\n",
        report.status,
        report.exit,
        report.guidance,
        report.root_archive_node_id,
        report.root_exact_state_hash,
        report.durable_head_unchanged,
    );
    output.push_str(&format!(
        "work selections={} generation={} engine_steps={} exact_nodes={} exact_edges={} terminal_wins={} query_rejected={} root_action_families={} frontier={}\n",
        report.work.selections,
        report.work.generation_work,
        report.work.engine_steps,
        report.work.exact_nodes,
        report.work.exact_edges,
        report.work.terminal_win_options,
        report.work.query_rejected_terminal_wins,
        report.work.root_action_families.len(),
        report.exact_witness_frontier.len(),
    ));
    if let Some(witness) = report.selected_witness.as_ref() {
        output.push_str(&format!(
            "selected final={}/{} potions={} actions={} events={} query_satisfied={}\naction_line={}\n",
            witness.final_hp,
            witness.final_max_hp,
            witness.potion_expenditures,
            witness.exact_actions.len(),
            witness.events.len(),
            witness.query_evaluation.satisfied,
            witness.semantic_action_line.join("/"),
        ));
    } else {
        output.push_str("no satisfying witness found; bounded absence remains Unknown\n");
    }
    output
}

fn combat_trajectory_query_provenance_v2() -> TaskPrivilegedSupportProvenanceV1 {
    TaskPrivilegedSupportProvenanceV1 {
        access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
        producer_name: COMBAT_TRAJECTORY_QUERY_PRODUCER_V2.to_string(),
        producer_revision: 2,
    }
}

fn validate_commit_proof_v2(
    root_public_snapshot_id: &str,
    proof: &RunPlayCombatTrajectoryQueryCommitProofV2,
) -> Result<(), String> {
    if proof.semantic_action_line.is_empty()
        || proof.replay_steps.len() != proof.semantic_action_line.len()
        || proof.exact_action_count != proof.semantic_action_line.len()
    {
        return Err(
            "combat query commit proof requires one non-empty, length-matched semantic replay"
                .to_string(),
        );
    }
    if proof
        .replay_steps
        .first()
        .map(|step| step.source_public_snapshot_id.as_str())
        != Some(root_public_snapshot_id)
        || proof.replay_steps.iter().enumerate().any(|(index, step)| {
            step.source_public_snapshot_id.trim().is_empty()
                || step.action.trim().is_empty()
                || step.action != proof.semantic_action_line[index]
        })
    {
        return Err(
            "combat query commit proof steps disagree with its root or semantic action line"
                .to_string(),
        );
    }
    match &proof.endpoint {
        AgentCombatResearchReplayEndpointV1::PublicDecision { public_snapshot_id }
            if public_snapshot_id.trim().is_empty() =>
        {
            return Err("combat query commit proof has an empty public endpoint".to_string());
        }
        AgentCombatResearchReplayEndpointV1::GameTerminal {
            outcome: TaskEpisodeOutcomeV1::Paused { .. },
        } => {
            return Err("combat query commit proof endpoint cannot be paused".to_string());
        }
        _ => {}
    }
    if proof.combat_terminal_max_hp <= 0
        || proof.public_outcome.persistent.max_hp != proof.combat_terminal_max_hp
        || proof.public_outcome.total_potions_consumed != proof.potion_expenditures
    {
        return Err(
            "combat query commit proof terminal resources disagree with its public outcome"
                .to_string(),
        );
    }
    Ok(())
}

pub fn load_combat_trajectory_query_commit_artifact_v2(
    path: &Path,
) -> Result<RunPlayCombatTrajectoryQueryCommitArtifactV2, String> {
    load_combat_trajectory_query_commit_artifact_candidate_v2(path, None)
}

pub fn load_combat_trajectory_query_commit_artifact_candidate_v2(
    path: &Path,
    candidate_index: Option<usize>,
) -> Result<RunPlayCombatTrajectoryQueryCommitArtifactV2, String> {
    let payload = read_bounded(
        path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run-play combat query artifact",
    )?;
    let envelope: RunPlayCombatTrajectoryQueryArtifactEnvelopeV2 = serde_json::from_slice(&payload)
        .map_err(|error| {
            format!(
                "failed to decode combat query artifact {}: {error}",
                path.display()
            )
        })?;
    let RunPlayCombatTrajectoryQueryArtifactEnvelopeV2 {
        schema,
        session_id,
        root_archive_node_id,
        root_public_snapshot_id,
        root_exact_state_hash,
        query,
        guidance: _,
        search_spec: _,
        bounded_search_only,
        absence_means_unknown,
        status,
        exit: _,
        work: _,
        exact_witness_frontier: _,
        frontier_commit_proofs,
        selected_witness: _,
        selected_commit_proof,
        durable_head_unchanged,
    } = envelope;
    if schema != COMBAT_TRAJECTORY_QUERY_SCHEMA_V2 {
        return Err(format!(
            "unsupported combat query artifact schema {schema:?}; expected {COMBAT_TRAJECTORY_QUERY_SCHEMA_V2}"
        ));
    }
    if session_id.trim().is_empty()
        || root_archive_node_id.trim().is_empty()
        || root_public_snapshot_id.trim().is_empty()
        || root_exact_state_hash.trim().is_empty()
    {
        return Err("combat query artifact root identity is incomplete".to_string());
    }
    if !bounded_search_only || !absence_means_unknown || !durable_head_unchanged {
        return Err(
            "combat query artifact does not preserve its bounded, read-only contract".to_string(),
        );
    }
    evaluate_combat_trajectory_query_v1(&query, &[])
        .map_err(|error| format!("combat query artifact contains an invalid query: {error}"))?;
    let proof = select_commit_proof_v2(
        selected_commit_proof,
        &frontier_commit_proofs,
        candidate_index,
    )?;
    if status != RunPlayCombatTrajectoryQueryStatusV2::VerifiedWitness {
        return Err(
            "combat query artifact commit proof requires verified_witness status".to_string(),
        );
    }
    validate_commit_proof_v2(&root_public_snapshot_id, &proof)?;
    Ok(RunPlayCombatTrajectoryQueryCommitArtifactV2 {
        session_id,
        root_archive_node_id,
        root_public_snapshot_id,
        root_exact_state_hash,
        query,
        proof,
    })
}

fn select_commit_proof_v2(
    selected_commit_proof: Option<RunPlayCombatTrajectoryQueryCommitProofV2>,
    frontier_commit_proofs: &[RunPlayCombatTrajectoryQueryFrontierCommitProofV2],
    candidate_index: Option<usize>,
) -> Result<RunPlayCombatTrajectoryQueryCommitProofV2, String> {
    let Some(candidate_index) = candidate_index else {
        return selected_commit_proof.ok_or_else(|| {
            "combat query artifact has no verified selected witness to anchor".to_string()
        });
    };
    let mut matching = frontier_commit_proofs
        .iter()
        .filter(|entry| entry.candidate_index == candidate_index);
    let proof = matching
        .next()
        .ok_or_else(|| {
            if frontier_commit_proofs.is_empty() {
                "combat query artifact predates frontier commit proofs; regenerate it at the exact root before using --candidate".to_string()
            } else {
                format!(
                    "combat query frontier candidate {candidate_index} has no commit proof; available candidate indices are [{}]",
                    frontier_commit_proofs
                        .iter()
                        .map(|entry| entry.candidate_index.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        })?
        .proof
        .clone();
    if matching.next().is_some() {
        return Err(format!(
            "combat query frontier candidate {candidate_index} has duplicate commit proofs"
        ));
    }
    Ok(proof)
}

pub fn anchor_combat_trajectory_query_artifact_v2(
    controller: &mut RunPlayControllerV1,
    artifact: &RunPlayCombatTrajectoryQueryCommitArtifactV2,
) -> Result<RunPlayActionLineProfileV1, String> {
    let head = controller.head();
    let observed_snapshot_id = head
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat query anchor requires a current public decision".to_string())?
        .public_decision_v1()?
        .information
        .snapshot_id;
    if head.session_id != artifact.session_id
        || head.position.archive_node_id() != artifact.root_archive_node_id
        || observed_snapshot_id != artifact.root_public_snapshot_id
    {
        return Err(format!(
            "combat query artifact root mismatch: artifact session={} node={} snapshot={}, current session={} node={} snapshot={}",
            artifact.session_id,
            artifact.root_archive_node_id,
            artifact.root_public_snapshot_id,
            head.session_id,
            head.position.archive_node_id(),
            observed_snapshot_id,
        ));
    }
    validate_commit_proof_v2(&artifact.root_public_snapshot_id, &artifact.proof)?;
    let root_position = controller.current_combat_position_for_lab_v1()?;
    let root = CombatDecisionRoot::new(root_position.clone())
        .map_err(|error| format!("combat query anchor requires a stable combat root: {error:?}"))?;
    if root.exact_state_hash() != artifact.root_exact_state_hash {
        return Err(format!(
            "combat query artifact exact root mismatch: artifact={} current={}",
            artifact.root_exact_state_hash,
            root.exact_state_hash(),
        ));
    }
    let selectors = artifact
        .proof
        .semantic_action_line
        .iter()
        .map(|action| {
            parse_action_selector_v1(
                action,
                "anchor-combat-query artifact",
                sts_presentation::Locale::En,
            )
        })
        .collect::<Result<Vec<RunPlayActionSelectorV1>, _>>()?;
    let comparison = controller.compare_action_lines_v1(std::slice::from_ref(&selectors))?;
    let replayed = comparison
        .lines
        .into_iter()
        .next()
        .ok_or_else(|| "combat query anchor produced no private replay".to_string())?;
    if replayed.steps.len() != artifact.proof.replay_steps.len() {
        return Err("combat query anchor replay length drifted".to_string());
    }
    for (index, (observed, expected)) in replayed
        .steps
        .iter()
        .zip(&artifact.proof.replay_steps)
        .enumerate()
    {
        let observed_action =
            semantic_action_for_choice_v1(&observed.source_decision, &observed.choice)?;
        if observed.source_decision.information.snapshot_id != expected.source_public_snapshot_id
            || observed_action != expected.action
        {
            return Err(format!(
                "combat query anchor semantic replay drifted at step {}",
                index + 1
            ));
        }
    }
    validate_replay_endpoint_v2(&replayed, &artifact.proof.endpoint)?;
    let choices = replayed
        .steps
        .iter()
        .map(|step| step.choice.clone())
        .collect::<Vec<_>>();
    let inputs = resolve_target_inputs_v1(&root_position, &choices, 250)?;
    let exact = materialize_exact_action_line(&EngineCombatStepper, &root_position, &inputs, 250)
        .map_err(|error| format!("combat query anchor exact replay failed: {error:?}"))?;
    if exact.actions.len() != artifact.proof.exact_action_count {
        return Err("combat query anchor exact action count drifted".to_string());
    }
    let events =
        replay_combat_trajectory_events_v1(&root_position, &exact.actions, &EngineCombatStepper)
            .map_err(|error| format!("combat query anchor event replay failed: {error:?}"))?;
    let query_evaluation = evaluate_combat_trajectory_query_v1(&artifact.query, &events)
        .map_err(|error| format!("combat query anchor evaluation failed: {error}"))?;
    if !query_evaluation.satisfied {
        return Err("combat query anchor replay no longer satisfies its typed query".to_string());
    }
    let combat_terminal_hp = exact.final_position.combat.entities.player.current_hp;
    let combat_terminal_max_hp = exact.final_position.combat.entities.player.max_hp;
    let potion_expenditures =
        exact_trajectory_potion_expenditures(&root_position, &exact.actions, &exact.final_position);
    if combat_terminal_hp != artifact.proof.combat_terminal_hp
        || combat_terminal_max_hp != artifact.proof.combat_terminal_max_hp
        || potion_expenditures != artifact.proof.potion_expenditures
    {
        return Err(format!(
            "combat query anchor terminal resources drifted: artifact={}/{} potions={}, current={}/{} potions={}",
            artifact.proof.combat_terminal_hp,
            artifact.proof.combat_terminal_max_hp,
            artifact.proof.potion_expenditures,
            combat_terminal_hp,
            combat_terminal_max_hp,
            potion_expenditures,
        ));
    }
    let public_outcome = controller.exact_combat_replay_outcome_v1(&inputs)?;
    if public_outcome != artifact.proof.public_outcome {
        return Err(format!(
            "combat query anchor public outcome drifted: artifact={:?} current={:?}",
            artifact.proof.public_outcome, public_outcome,
        ));
    }
    let source_public_snapshot_ids = artifact
        .proof
        .replay_steps
        .iter()
        .map(|step| step.source_public_snapshot_id.clone())
        .collect::<Vec<_>>();
    let evidence = RunPlayCombatResearchCandidateV1 {
        supporting_provenances: vec![combat_trajectory_query_provenance_v2()],
        outcome: public_outcome,
        satisfies_requested_quality: true,
    };
    match &artifact.proof.endpoint {
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

fn validate_replay_endpoint_v2(
    replayed: &sts_oracle_learning::eval::run_control::ExactRunPlayProbeLineV1,
    expected: &AgentCombatResearchReplayEndpointV1,
) -> Result<(), String> {
    let last = replayed
        .steps
        .last()
        .ok_or_else(|| "combat query anchor replay is empty".to_string())?;
    match (&last.next_decision, &last.outcome, expected) {
        (
            Some(decision),
            _,
            AgentCombatResearchReplayEndpointV1::PublicDecision { public_snapshot_id },
        ) if decision.information.snapshot_id == *public_snapshot_id => Ok(()),
        (None, observed, AgentCombatResearchReplayEndpointV1::GameTerminal { outcome })
            if observed == outcome =>
        {
            Ok(())
        }
        _ => Err("combat query anchor endpoint drifted".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::agent::task::TaskResearchPlanPersistentOutcomeV1;

    fn proof() -> RunPlayCombatTrajectoryQueryCommitProofV2 {
        RunPlayCombatTrajectoryQueryCommitProofV2 {
            semantic_action_line: vec!["end".to_string()],
            replay_steps: vec![AgentCombatResearchReplayStepV1 {
                source_public_snapshot_id: "root-snapshot".to_string(),
                action: "end".to_string(),
            }],
            endpoint: AgentCombatResearchReplayEndpointV1::PublicDecision {
                public_snapshot_id: "successor-snapshot".to_string(),
            },
            public_outcome: TaskResearchPlanOutcomeV1 {
                final_hp: 16,
                final_gold: 0,
                persistent: TaskResearchPlanPersistentOutcomeV1 {
                    max_hp: 83,
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
            combat_terminal_hp: 10,
            combat_terminal_max_hp: 83,
            potion_expenditures: 0,
            exact_action_count: 1,
        }
    }

    #[test]
    fn commit_proof_accepts_one_root_bound_semantic_replay() {
        validate_commit_proof_v2("root-snapshot", &proof()).expect("valid commit proof");
    }

    #[test]
    fn commit_proof_rejects_semantic_step_drift() {
        let mut proof = proof();
        proof.replay_steps[0].action = "play:Strike@0".to_string();
        let error = validate_commit_proof_v2("root-snapshot", &proof)
            .expect_err("semantic drift must be rejected");
        assert!(error.contains("steps disagree"));
    }

    #[test]
    fn explicit_frontier_candidate_selects_its_own_commit_proof() {
        let mut second = proof();
        second.combat_terminal_hp = 11;
        second.public_outcome.final_hp = 17;
        let frontier = vec![
            RunPlayCombatTrajectoryQueryFrontierCommitProofV2 {
                candidate_index: 0,
                proof: proof(),
            },
            RunPlayCombatTrajectoryQueryFrontierCommitProofV2 {
                candidate_index: 4,
                proof: second.clone(),
            },
        ];

        let selected = select_commit_proof_v2(None, &frontier, Some(4))
            .expect("frontier candidate should be selectable");
        assert_eq!(selected, second);
    }

    #[test]
    fn legacy_query_artifact_requires_regeneration_for_frontier_candidate() {
        let error = select_commit_proof_v2(Some(proof()), &[], Some(1))
            .expect_err("old artifact must not guess a frontier proof");
        assert!(error.contains("predates frontier commit proofs"));
    }

    #[test]
    fn feed_requirement_installs_an_additive_typed_guide_component() {
        let generic = combat_trajectory_query_guidance_v1(&CombatTrajectoryQueryV1::True);
        assert_eq!(
            generic,
            vec![RunPlayCombatTrajectoryQueryGuidanceV1::GenericCombatPlanState]
        );

        let feed = combat_trajectory_query_guidance_v1(&CombatTrajectoryQueryV1::all([
            CombatTrajectoryQueryV1::eventually(CombatTrajectoryAtomV1::CardLethalPayoff {
                card_id: sts_oracle_learning::content::cards::CardId::Feed,
            }),
        ]));
        assert_eq!(
            feed,
            vec![
                RunPlayCombatTrajectoryQueryGuidanceV1::GenericCombatPlanState,
                RunPlayCombatTrajectoryQueryGuidanceV1::FeedPayoff,
            ]
        );
    }
}
