use std::path::Path;

use serde::{Deserialize, Serialize};
use sts_combat_knowledge::feed_payoff_guide_policy_v1;
use sts_combat_planner::{
    combat_plan_state_guide_policy_v1, exact_trajectory_potion_expenditures,
    materialize_exact_action_line, replay_oracle_combat_witness, CombatDecisionRoot,
    LocalTurnGraphSearchSpecV1, LocalTurnGraphTerminalOutcomeSnapshotV1,
    LocalTurnGraphWitnessInterruption, LocalTurnGraphWitnessSession, LocalTurnGraphWitnessStatus,
    OracleCombatWitness, OracleCombatWitnessDiscoverySource, OracleCombatWitnessSatisfaction,
    TurnOptionAction,
};
use sts_oracle_learning::agent::task::TaskResearchPlanOutcomeV1;
use sts_oracle_learning::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_learning::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
};
use sts_oracle_learning::state::core::ClientInput;

use crate::agent_protocol::{
    load_combat_research_artifact_v1, RunPlayCombatResearchArtifactV1,
    COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1,
};
use crate::combat_query::{
    load_combat_trajectory_query_commit_artifact_v2, COMBAT_TRAJECTORY_QUERY_SCHEMA_V2,
};
use crate::combat_trace::{
    trace_verified_combat_source_v1, verified_combat_trace_source_from_query_v2,
    verified_combat_trace_source_from_research_artifact_v1, RunPlayFeedAttemptV1,
    VerifiedCombatTraceSourceV1,
};
use crate::driver::RunPlayControllerV1;
use crate::proposal_audit::resolve_target_inputs_v1;
use crate::selector::parse_action_selector_v1;
use crate::storage::{read_bounded, RunPlayPathsV1, DEFAULT_ARTIFACT_MAX_BYTES};

pub const COMBAT_FEED_REPAIR_SCHEMA_V1: &str = "sts-run-play-combat-feed-repair-v1";
pub const COMBAT_FEED_CHALLENGE_SCHEMA_V1: &str = "sts-run-play-combat-feed-challenge-v1";
pub const COMBAT_FEED_CHALLENGE_SCHEMA_V2: &str = "sts-run-play-combat-feed-challenge-v2";
pub const COMBAT_DAMAGE_CHALLENGE_SCHEMA_V1: &str = "sts-run-play-combat-damage-challenge-v1";

#[derive(Deserialize)]
struct CombatDamageSourceSchemaV1 {
    schema: String,
}

pub fn load_verified_combat_source_v1(path: &Path) -> Result<VerifiedCombatTraceSourceV1, String> {
    let payload = read_bounded(
        path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run-play combat damage source artifact",
    )?;
    let envelope: CombatDamageSourceSchemaV1 =
        serde_json::from_slice(&payload).map_err(|error| {
            format!(
                "failed to identify combat damage source artifact {}: {error}",
                path.display()
            )
        })?;
    match envelope.schema.as_str() {
        COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1 => {
            let artifact = load_combat_research_artifact_v1(path)?;
            verified_combat_trace_source_from_research_artifact_v1(&artifact)
        }
        COMBAT_TRAJECTORY_QUERY_SCHEMA_V2 => {
            let artifact = load_combat_trajectory_query_commit_artifact_v2(path)?;
            verified_combat_trace_source_from_query_v2(&artifact)
        }
        other => Err(format!(
            "unsupported combat damage source schema {other:?}; expected {COMBAT_RESEARCH_ARTIFACT_SCHEMA_V1} or {COMBAT_TRAJECTORY_QUERY_SCHEMA_V2}"
        )),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CombatFeedRepairOptionsV1 {
    pub candidate_index: usize,
    pub fixed_prefix_actions: usize,
    pub minimum_final_hp: i32,
    pub minimum_final_max_hp: i32,
    pub max_generation_work: usize,
    pub max_selections: usize,
    pub wall_ms: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct CombatFeedChallengeOptionsV1 {
    pub max_challengers: usize,
    pub max_generation_work_per_challenger: usize,
    pub max_selections_per_challenger: usize,
    pub wall_ms_per_challenger: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct CombatDamageChallengeOptionsV1 {
    pub max_challengers: usize,
    pub max_generation_work_per_challenger: usize,
    pub max_selections_per_challenger: usize,
    pub wall_ms_per_challenger: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatDamageChallengeV1 {
    pub schema: String,
    pub session_id: String,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub source_frontier_candidate_indices: Vec<usize>,
    pub eligible_challenger_count: usize,
    pub challenger_limit: usize,
    pub attempts: Vec<RunPlayCombatDamageChallengeAttemptV1>,
    pub global_resource_frontier: Vec<RunPlayCombatDamageGlobalFrontierMemberV1>,
    pub durable_head_unchanged: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatDamageGlobalFrontierMemberV1 {
    pub attempt_index: usize,
    pub witness_index: usize,
    pub public_outcome: TaskResearchPlanOutcomeV1,
    pub combat_terminal_hp: i32,
    pub combat_terminal_max_hp: i32,
    pub total_potions_consumed: u32,
    pub total_action_count: usize,
    pub exact_actions: Vec<TurnOptionAction>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatDamageChallengeAttemptV1 {
    pub source_candidate_index: usize,
    pub source_candidate_id: String,
    pub source_outcome: TaskResearchPlanOutcomeV1,
    pub trigger: RunPlayCombatDamageCutpointV1,
    pub retained_prefix_actions: usize,
    pub report: RunPlayCombatDamageRepairV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatDamageCutpointV1 {
    pub step: usize,
    pub turn_before: u32,
    pub turn_after: u32,
    pub action: String,
    pub observed_hp_loss: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatDamageRepairV1 {
    pub source_candidate_index: usize,
    pub source_candidate_id: String,
    pub fixed_prefix_actions: Vec<String>,
    pub repair_root_turn: u32,
    pub repair_root_hp: i32,
    pub repair_root_max_hp: i32,
    pub source_total_potions_consumed: u32,
    pub fixed_prefix_potions_consumed: u32,
    pub maximum_suffix_potions: u32,
    pub bounded_search_only: bool,
    pub absence_means_unknown: bool,
    pub status: RunPlayCombatDamageRepairStatusV1,
    pub exit: RunPlayCombatRepairExitV1,
    pub work: RunPlayCombatRepairWorkV1,
    pub observed_terminal_frontier: Vec<LocalTurnGraphTerminalOutcomeSnapshotV1>,
    pub exact_witness_frontier: Vec<RunPlayCombatDamageRepairWitnessV1>,
    pub strict_dominating_witnesses: Vec<RunPlayCombatDamageRepairWitnessV1>,
    pub durable_head_unchanged: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatDamageRepairStatusV1 {
    VerifiedStrictImprovement,
    UnknownInBoundedSearch,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatDamageRepairWitnessV1 {
    pub strictly_dominates_source: bool,
    pub combat_terminal_hp: i32,
    pub combat_terminal_max_hp: i32,
    pub total_potions_consumed: u32,
    pub fixed_action_count: usize,
    pub repair_action_count: usize,
    pub total_action_count: usize,
    pub public_outcome: TaskResearchPlanOutcomeV1,
    pub exact_actions: Vec<TurnOptionAction>,
    pub trace: Vec<RunPlayCombatRepairActionTraceV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatFeedChallengeV2 {
    pub schema: String,
    pub session_id: String,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub source_frontier_candidate_indices: Vec<usize>,
    pub eligible_challenger_count: usize,
    pub challenger_limit: usize,
    pub attempts: Vec<RunPlayCombatFeedChallengeAttemptV1>,
    pub global_resource_frontier: Vec<RunPlayCombatFeedGlobalFrontierMemberV2>,
    pub durable_head_unchanged: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatFeedGlobalFrontierMemberV2 {
    pub attempt_index: usize,
    pub witness_index: usize,
    pub public_outcome: TaskResearchPlanOutcomeV1,
    pub combat_terminal_hp: i32,
    pub combat_terminal_max_hp: i32,
    pub max_hp_gain: i32,
    pub total_potions_consumed: u32,
    pub total_action_count: usize,
    pub exact_actions: Vec<TurnOptionAction>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatFeedChallengeAttemptV1 {
    pub source_candidate_index: usize,
    pub source_candidate_id: String,
    pub source_final_hp: i32,
    pub source_final_max_hp: i32,
    pub source_total_potions_consumed: u32,
    pub trigger: RunPlayFeedAttemptV1,
    pub replaced_action: String,
    pub retained_prefix_actions: usize,
    pub report: RunPlayCombatFeedRepairV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatFeedRepairV1 {
    pub schema: String,
    pub session_id: String,
    pub root_archive_node_id: String,
    pub root_public_snapshot_id: String,
    pub source_candidate_index: usize,
    pub source_candidate_id: String,
    pub fixed_prefix_actions: Vec<String>,
    pub repair_root_turn: u32,
    pub repair_root_hp: i32,
    pub repair_root_max_hp: i32,
    pub requirement: RunPlayCombatFeedRepairRequirementV1,
    pub bounded_search_only: bool,
    pub absence_means_unknown: bool,
    pub status: RunPlayCombatFeedRepairStatusV1,
    pub exit: RunPlayCombatRepairExitV1,
    pub work: RunPlayCombatRepairWorkV1,
    pub observed_terminal_frontier: Vec<LocalTurnGraphTerminalOutcomeSnapshotV1>,
    pub exact_witness_frontier: Vec<RunPlayCombatFeedRepairWitnessV1>,
    pub witness: Option<RunPlayCombatFeedRepairWitnessV1>,
    pub durable_head_unchanged: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatFeedRepairRequirementV1 {
    pub minimum_final_hp: i32,
    pub minimum_final_max_hp: i32,
    pub maximum_potions_used: u32,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatFeedRepairStatusV1 {
    VerifiedRepair,
    UnknownInBoundedSearch,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatRepairExitV1 {
    WitnessFound,
    SelectionBudget,
    GenerationWorkBudget,
    EngineStepBudget,
    Deadline,
    FrontierExhausted,
    MechanicsGap,
    ReplayMismatch,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatRepairWorkV1 {
    pub selections: usize,
    pub generation_work: usize,
    pub engine_steps: usize,
    pub exact_nodes: usize,
    pub completed_turn_options: usize,
    pub terminal_win_options: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatFeedRepairWitnessV1 {
    pub satisfies_requirement: bool,
    pub final_hp: i32,
    pub final_max_hp: i32,
    pub max_hp_gain: i32,
    pub potion_expenditures: u32,
    pub fixed_action_count: usize,
    pub repair_action_count: usize,
    pub total_action_count: usize,
    pub exact_actions: Vec<TurnOptionAction>,
    pub trace: Vec<RunPlayCombatRepairActionTraceV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatRepairActionTraceV1 {
    pub step: usize,
    pub phase: RunPlayCombatRepairActionPhaseV1,
    pub turn: u32,
    pub action: String,
    pub player_hp_before: i32,
    pub player_hp_after: i32,
    pub player_max_hp_before: i32,
    pub player_max_hp_after: i32,
    pub monster_changes: Vec<RunPlayCombatRepairMonsterDeltaV1>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayCombatRepairActionPhaseV1 {
    FixedPrefix,
    RepairedSuffix,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatRepairMonsterDeltaV1 {
    pub slot: u8,
    pub enemy: String,
    pub hp_before: i32,
    pub hp_after: Option<i32>,
    pub alive_after: Option<bool>,
}

/// Challenges non-dominated, potion-free survival witnesses only where their
/// exact replay first spends Feed without a fatal payoff. The source witness
/// remains intact; every challenger starts immediately before that semantic
/// cutpoint, must remain potion-free, and searches to its finite budget for a
/// strictly larger max-HP outcome. No HP/max-HP exchange rate is applied.
pub fn challenge_nonfatal_feed_from_verified_source_v1(
    paths: RunPlayPathsV1,
    source: &VerifiedCombatTraceSourceV1,
    options: CombatFeedChallengeOptionsV1,
) -> Result<RunPlayCombatFeedChallengeV2, String> {
    source.validate()?;
    if options.max_challengers == 0
        || options.max_generation_work_per_challenger == 0
        || options.max_selections_per_challenger == 0
        || options.wall_ms_per_challenger == 0
    {
        return Err("combat Feed challenge requires positive finite limits".to_string());
    }

    let source_frontier_candidate_indices = source
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            (!source.candidates.iter().any(|other| {
                !std::ptr::eq(candidate, other)
                    && other.outcome.strictly_dominates(&candidate.outcome)
            }))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    if source_frontier_candidate_indices.is_empty() {
        return Ok(RunPlayCombatFeedChallengeV2 {
            schema: COMBAT_FEED_CHALLENGE_SCHEMA_V2.to_string(),
            session_id: source.session_id.clone(),
            root_archive_node_id: source.root_archive_node_id.clone(),
            root_public_snapshot_id: source.root_public_snapshot_id.clone(),
            source_frontier_candidate_indices,
            eligible_challenger_count: 0,
            challenger_limit: options.max_challengers,
            attempts: Vec::new(),
            global_resource_frontier: Vec::new(),
            durable_head_unchanged: true,
        });
    }
    let trace =
        trace_verified_combat_source_v1(paths.clone(), source, &source_frontier_candidate_indices)?;
    let eligible = trace
        .candidates
        .into_iter()
        .filter_map(|candidate_trace| {
            let candidate = source.candidates.get(candidate_trace.candidate_index)?;
            if candidate.outcome.total_potions_consumed != 0 {
                return None;
            }
            let trigger = candidate_trace
                .feed_attempts
                .into_iter()
                .find(|attempt| !attempt.fatal_payoff_observed && attempt.step > 1)?;
            let replaced_action = candidate_trace
                .steps
                .get(trigger.step.saturating_sub(1))?
                .action
                .clone();
            Some((
                candidate_trace.candidate_index,
                candidate.candidate_id.clone(),
                candidate.outcome.final_hp,
                candidate.outcome.persistent.max_hp,
                candidate.outcome.total_potions_consumed,
                trigger,
                replaced_action,
            ))
        })
        .collect::<Vec<_>>();
    let eligible_challenger_count = eligible.len();
    if eligible_challenger_count > options.max_challengers {
        return Err(format!(
            "combat Feed challenge found {eligible_challenger_count} eligible frontier witnesses, exceeding explicit limit {}; raise --max-challengers rather than silently selecting a subset",
            options.max_challengers
        ));
    }

    let mut attempts = Vec::with_capacity(eligible.len());
    for (
        source_candidate_index,
        source_candidate_id,
        source_final_hp,
        source_final_max_hp,
        source_total_potions_consumed,
        trigger,
        replaced_action,
    ) in eligible
    {
        let retained_prefix_actions = trigger.step - 1;
        let report = repair_feed_from_verified_source_internal_v1(
            paths.clone(),
            source,
            CombatFeedRepairOptionsV1 {
                candidate_index: source_candidate_index,
                fixed_prefix_actions: retained_prefix_actions,
                minimum_final_hp: 1,
                minimum_final_max_hp: source_final_max_hp.saturating_add(1),
                max_generation_work: options.max_generation_work_per_challenger,
                max_selections: options.max_selections_per_challenger,
                wall_ms: options.wall_ms_per_challenger,
            },
            true,
        )?;
        attempts.push(RunPlayCombatFeedChallengeAttemptV1 {
            source_candidate_index,
            source_candidate_id,
            source_final_hp,
            source_final_max_hp,
            source_total_potions_consumed,
            trigger,
            replaced_action,
            retained_prefix_actions,
            report,
        });
    }

    let global_resource_frontier = global_feed_resource_frontier_v2(paths, source, &attempts)?;
    Ok(RunPlayCombatFeedChallengeV2 {
        schema: COMBAT_FEED_CHALLENGE_SCHEMA_V2.to_string(),
        session_id: source.session_id.clone(),
        root_archive_node_id: source.root_archive_node_id.clone(),
        root_public_snapshot_id: source.root_public_snapshot_id.clone(),
        source_frontier_candidate_indices,
        eligible_challenger_count,
        challenger_limit: options.max_challengers,
        attempts,
        global_resource_frontier,
        durable_head_unchanged: true,
    })
}

fn global_feed_resource_frontier_v2(
    paths: RunPlayPathsV1,
    source: &VerifiedCombatTraceSourceV1,
    attempts: &[RunPlayCombatFeedChallengeAttemptV1],
) -> Result<Vec<RunPlayCombatFeedGlobalFrontierMemberV2>, String> {
    let selector = format!("n{}", source.root_archive_node_id);
    let controller = RunPlayControllerV1::open_at(paths, &selector)?;
    let public_root = controller
        .head()
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat Feed challenge promotion root has no public decision".to_string())?
        .public_decision_v1()?;
    if controller.head().session_id != source.session_id
        || controller.head().position.archive_node_id() != source.root_archive_node_id
        || public_root.information.snapshot_id != source.root_public_snapshot_id
    {
        return Err(
            "combat Feed challenge promotion root identity disagrees with source".to_string(),
        );
    }

    let mut candidates = Vec::<RunPlayCombatFeedGlobalFrontierMemberV2>::new();
    for (attempt_index, attempt) in attempts.iter().enumerate() {
        for (witness_index, witness) in attempt.report.exact_witness_frontier.iter().enumerate() {
            if !witness.satisfies_requirement
                || witness.max_hp_gain <= 0
                || witness.potion_expenditures != 0
            {
                continue;
            }
            let inputs = witness
                .exact_actions
                .iter()
                .map(|action| action.input.clone())
                .collect::<Vec<_>>();
            let public_outcome = controller.exact_combat_replay_outcome_v1(&inputs)?;
            candidates.push(RunPlayCombatFeedGlobalFrontierMemberV2 {
                attempt_index,
                witness_index,
                public_outcome,
                combat_terminal_hp: witness.final_hp,
                combat_terminal_max_hp: witness.final_max_hp,
                max_hp_gain: witness.max_hp_gain,
                total_potions_consumed: witness.potion_expenditures,
                total_action_count: witness.total_action_count,
                exact_actions: witness.exact_actions.clone(),
            });
        }
    }
    let retained = retain_global_feed_frontier_v2(candidates);
    validate_global_feed_frontier_v2(&retained)?;
    Ok(retained)
}

fn retain_global_feed_frontier_v2(
    candidates: Vec<RunPlayCombatFeedGlobalFrontierMemberV2>,
) -> Vec<RunPlayCombatFeedGlobalFrontierMemberV2> {
    let mut unique = Vec::<RunPlayCombatFeedGlobalFrontierMemberV2>::new();
    for candidate in candidates {
        if unique
            .iter()
            .any(|member| member.public_outcome == candidate.public_outcome)
        {
            continue;
        }
        unique.push(candidate);
    }
    unique
        .iter()
        .enumerate()
        .filter_map(|(index, member)| {
            (!unique.iter().enumerate().any(|(other_index, other)| {
                index != other_index
                    && other
                        .public_outcome
                        .strictly_dominates(&member.public_outcome)
            }))
            .then_some(member.clone())
        })
        .collect()
}

fn validate_global_feed_frontier_v2(
    frontier: &[RunPlayCombatFeedGlobalFrontierMemberV2],
) -> Result<(), String> {
    for (index, member) in frontier.iter().enumerate() {
        if member.exact_actions.is_empty()
            || member.exact_actions.len() != member.total_action_count
        {
            return Err(format!(
                "combat Feed global frontier member {index} has inconsistent exact actions"
            ));
        }
        if member.combat_terminal_hp < 1
            || member.combat_terminal_max_hp < member.combat_terminal_hp
            || member.max_hp_gain <= 0
        {
            return Err(format!(
                "combat Feed global frontier member {index} has invalid terminal resources"
            ));
        }
        if member.total_potions_consumed != 0 || member.public_outcome.total_potions_consumed != 0 {
            return Err(format!(
                "combat Feed global frontier member {index} is not potion-free"
            ));
        }
        if member.public_outcome.final_hp < 1
            || member.public_outcome.persistent.max_hp < member.public_outcome.final_hp
            || member.public_outcome.persistent.max_hp != member.combat_terminal_max_hp
        {
            return Err(format!(
                "combat Feed global frontier member {index} has inconsistent public resources"
            ));
        }
        for (other_index, other) in frontier.iter().enumerate() {
            if index == other_index {
                continue;
            }
            if member.public_outcome == other.public_outcome {
                return Err(format!(
                    "combat Feed global frontier members {index} and {other_index} duplicate one public outcome"
                ));
            }
            if other
                .public_outcome
                .strictly_dominates(&member.public_outcome)
            {
                return Err(format!(
                    "combat Feed global frontier member {index} is dominated by member {other_index}"
                ));
            }
        }
    }
    Ok(())
}

fn require_combat_feed_challenge_schema_v2(schema: &str) -> Result<(), String> {
    if schema == COMBAT_FEED_CHALLENGE_SCHEMA_V1 {
        return Err(
            "combat Feed challenge V1 artifacts are inspection-only; regenerate V2 at the unchanged exact root before anchoring"
                .to_string(),
        );
    }
    if schema != COMBAT_FEED_CHALLENGE_SCHEMA_V2 {
        return Err(format!(
            "unsupported combat Feed challenge schema: {schema}"
        ));
    }
    Ok(())
}

pub fn load_combat_feed_challenge_artifact_v2(
    path: &Path,
) -> Result<RunPlayCombatFeedChallengeV2, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PromotionViewV2 {
        schema: String,
        session_id: String,
        root_archive_node_id: String,
        root_public_snapshot_id: String,
        source_frontier_candidate_indices: Vec<usize>,
        eligible_challenger_count: usize,
        challenger_limit: usize,
        attempts: serde_json::Value,
        global_resource_frontier: Vec<RunPlayCombatFeedGlobalFrontierMemberV2>,
        durable_head_unchanged: bool,
    }

    let payload = read_bounded(
        path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run-play combat Feed challenge artifact",
    )?;
    let schema = serde_json::from_slice::<CombatDamageSourceSchemaV1>(&payload)
        .map_err(|error| format!("failed to identify combat Feed challenge artifact: {error}"))?
        .schema;
    require_combat_feed_challenge_schema_v2(&schema)?;
    let loaded: PromotionViewV2 = serde_json::from_slice(&payload)
        .map_err(|error| format!("failed to parse combat Feed challenge artifact: {error}"))?;
    if !loaded.attempts.is_array() {
        return Err("combat Feed challenge attempts must be a JSON array".to_string());
    }
    let artifact = RunPlayCombatFeedChallengeV2 {
        schema: loaded.schema,
        session_id: loaded.session_id,
        root_archive_node_id: loaded.root_archive_node_id,
        root_public_snapshot_id: loaded.root_public_snapshot_id,
        source_frontier_candidate_indices: loaded.source_frontier_candidate_indices,
        eligible_challenger_count: loaded.eligible_challenger_count,
        challenger_limit: loaded.challenger_limit,
        attempts: Vec::new(),
        global_resource_frontier: loaded.global_resource_frontier,
        durable_head_unchanged: loaded.durable_head_unchanged,
    };
    require_combat_feed_challenge_schema_v2(&artifact.schema)?;
    if !artifact.durable_head_unchanged {
        return Err("combat Feed challenge did not preserve its durable head".to_string());
    }
    validate_global_feed_frontier_v2(&artifact.global_resource_frontier)?;
    Ok(artifact)
}

/// Reopens every turn-advancing HP-loss action in the non-dominated source
/// frontier and searches from immediately before that action. Each suffix is
/// limited to the source witness's remaining potion expenditure, while the
/// original suffix is retained as verified evidence. Improvements are judged
/// only after replaying through the full run environment to the next public
/// boundary; no HP/potion exchange rate is introduced.
pub fn challenge_turn_damage_from_verified_source_v1(
    paths: RunPlayPathsV1,
    source: &VerifiedCombatTraceSourceV1,
    options: CombatDamageChallengeOptionsV1,
) -> Result<RunPlayCombatDamageChallengeV1, String> {
    source.validate()?;
    if options.max_challengers == 0
        || options.max_generation_work_per_challenger == 0
        || options.max_selections_per_challenger == 0
        || options.wall_ms_per_challenger == 0
    {
        return Err("combat damage challenge requires positive finite limits".to_string());
    }

    let source_frontier_candidate_indices = source
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            (!source.candidates.iter().any(|other| {
                !std::ptr::eq(candidate, other)
                    && other.outcome.strictly_dominates(&candidate.outcome)
            }))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    if source_frontier_candidate_indices.is_empty() {
        return Ok(RunPlayCombatDamageChallengeV1 {
            schema: COMBAT_DAMAGE_CHALLENGE_SCHEMA_V1.to_string(),
            session_id: source.session_id.clone(),
            root_archive_node_id: source.root_archive_node_id.clone(),
            root_public_snapshot_id: source.root_public_snapshot_id.clone(),
            source_frontier_candidate_indices,
            eligible_challenger_count: 0,
            challenger_limit: options.max_challengers,
            attempts: Vec::new(),
            global_resource_frontier: Vec::new(),
            durable_head_unchanged: true,
        });
    }

    let trace =
        trace_verified_combat_source_v1(paths.clone(), source, &source_frontier_candidate_indices)?;
    let mut eligible = Vec::new();
    for candidate_trace in trace.candidates {
        for pair in candidate_trace.steps.windows(2) {
            let step = &pair[0];
            let next = &pair[1];
            if step.delta.player_hp >= 0 || next.turn <= step.turn {
                continue;
            }
            eligible.push((
                candidate_trace.candidate_index,
                RunPlayCombatDamageCutpointV1 {
                    step: step.step,
                    turn_before: step.turn,
                    turn_after: next.turn,
                    action: step.action.clone(),
                    observed_hp_loss: step.delta.player_hp.saturating_neg(),
                },
            ));
        }
    }
    let eligible_challenger_count = eligible.len();
    if eligible_challenger_count > options.max_challengers {
        return Err(format!(
            "combat damage challenge found {eligible_challenger_count} turn-damage cutpoints, exceeding explicit limit {}; raise --max-challengers rather than silently selecting a subset",
            options.max_challengers
        ));
    }

    let mut attempts = Vec::with_capacity(eligible.len());
    for (source_candidate_index, trigger) in eligible {
        let candidate = source
            .candidates
            .get(source_candidate_index)
            .ok_or_else(|| "combat damage challenge source candidate disappeared".to_string())?;
        let retained_prefix_actions = trigger.step.saturating_sub(1);
        let report = repair_turn_damage_cutpoint_from_verified_source_v1(
            paths.clone(),
            source,
            source_candidate_index,
            retained_prefix_actions,
            options,
        )?;
        attempts.push(RunPlayCombatDamageChallengeAttemptV1 {
            source_candidate_index,
            source_candidate_id: candidate.candidate_id.clone(),
            source_outcome: candidate.outcome.clone(),
            trigger,
            retained_prefix_actions,
            report,
        });
    }

    let global_resource_frontier = global_damage_resource_frontier_v1(&attempts);
    Ok(RunPlayCombatDamageChallengeV1 {
        schema: COMBAT_DAMAGE_CHALLENGE_SCHEMA_V1.to_string(),
        session_id: source.session_id.clone(),
        root_archive_node_id: source.root_archive_node_id.clone(),
        root_public_snapshot_id: source.root_public_snapshot_id.clone(),
        source_frontier_candidate_indices,
        eligible_challenger_count,
        challenger_limit: options.max_challengers,
        attempts,
        global_resource_frontier,
        durable_head_unchanged: true,
    })
}

fn global_damage_resource_frontier_v1(
    attempts: &[RunPlayCombatDamageChallengeAttemptV1],
) -> Vec<RunPlayCombatDamageGlobalFrontierMemberV1> {
    let mut unique = Vec::<RunPlayCombatDamageGlobalFrontierMemberV1>::new();
    for (attempt_index, attempt) in attempts.iter().enumerate() {
        for (witness_index, witness) in attempt.report.exact_witness_frontier.iter().enumerate() {
            if unique
                .iter()
                .any(|member| member.public_outcome == witness.public_outcome)
            {
                continue;
            }
            unique.push(RunPlayCombatDamageGlobalFrontierMemberV1 {
                attempt_index,
                witness_index,
                public_outcome: witness.public_outcome.clone(),
                combat_terminal_hp: witness.combat_terminal_hp,
                combat_terminal_max_hp: witness.combat_terminal_max_hp,
                total_potions_consumed: witness.total_potions_consumed,
                total_action_count: witness.total_action_count,
                exact_actions: witness.exact_actions.clone(),
            });
        }
    }
    unique
        .iter()
        .enumerate()
        .filter_map(|(index, member)| {
            (!unique.iter().enumerate().any(|(other_index, other)| {
                index != other_index
                    && other
                        .public_outcome
                        .strictly_dominates(&member.public_outcome)
            }))
            .then_some(member.clone())
        })
        .collect()
}

pub fn load_combat_damage_challenge_artifact_v1(
    path: &Path,
) -> Result<RunPlayCombatDamageChallengeV1, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PromotionViewV1 {
        schema: String,
        session_id: String,
        root_archive_node_id: String,
        root_public_snapshot_id: String,
        source_frontier_candidate_indices: Vec<usize>,
        eligible_challenger_count: usize,
        challenger_limit: usize,
        attempts: serde_json::Value,
        global_resource_frontier: Vec<RunPlayCombatDamageGlobalFrontierMemberV1>,
        durable_head_unchanged: bool,
    }

    let payload = read_bounded(
        path,
        DEFAULT_ARTIFACT_MAX_BYTES,
        "run-play combat damage challenge artifact",
    )?;
    let loaded: PromotionViewV1 = serde_json::from_slice(&payload)
        .map_err(|error| format!("failed to parse combat damage challenge artifact: {error}"))?;
    if !loaded.attempts.is_array() {
        return Err("combat damage challenge attempts must be a JSON array".to_string());
    }
    let artifact = RunPlayCombatDamageChallengeV1 {
        schema: loaded.schema,
        session_id: loaded.session_id,
        root_archive_node_id: loaded.root_archive_node_id,
        root_public_snapshot_id: loaded.root_public_snapshot_id,
        source_frontier_candidate_indices: loaded.source_frontier_candidate_indices,
        eligible_challenger_count: loaded.eligible_challenger_count,
        challenger_limit: loaded.challenger_limit,
        attempts: Vec::new(),
        global_resource_frontier: loaded.global_resource_frontier,
        durable_head_unchanged: loaded.durable_head_unchanged,
    };
    if artifact.schema != COMBAT_DAMAGE_CHALLENGE_SCHEMA_V1 {
        return Err(format!(
            "unsupported combat damage challenge schema: {}",
            artifact.schema
        ));
    }
    for (index, member) in artifact.global_resource_frontier.iter().enumerate() {
        if member.exact_actions.is_empty()
            || member.exact_actions.len() != member.total_action_count
        {
            return Err(format!(
                "combat damage global frontier member {index} has inconsistent exact actions"
            ));
        }
        if member.public_outcome.final_hp < 1
            || member.public_outcome.persistent.max_hp < member.public_outcome.final_hp
        {
            return Err(format!(
                "combat damage global frontier member {index} has invalid public HP"
            ));
        }
    }
    Ok(artifact)
}

fn repair_turn_damage_cutpoint_from_verified_source_v1(
    paths: RunPlayPathsV1,
    source: &VerifiedCombatTraceSourceV1,
    source_candidate_index: usize,
    fixed_prefix_actions: usize,
    options: CombatDamageChallengeOptionsV1,
) -> Result<RunPlayCombatDamageRepairV1, String> {
    let candidate = source
        .candidates
        .get(source_candidate_index)
        .ok_or_else(|| {
            format!(
                "combat damage repair candidate {source_candidate_index} is outside artifact candidate count {}",
                source.candidates.len()
            )
        })?;
    let proof = &candidate.replay_proof;
    if fixed_prefix_actions >= proof.steps.len() {
        return Err(format!(
            "combat damage repair prefix must retain 0..{} actions",
            proof.steps.len().saturating_sub(1)
        ));
    }

    let selector = format!("n{}", source.root_archive_node_id);
    let mut controller = RunPlayControllerV1::open_at(paths, &selector)?;
    let public_root = controller
        .head()
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat damage repair historical root has no public decision".to_string())?
        .public_decision_v1()?;
    if controller.head().session_id != source.session_id
        || controller.head().position.archive_node_id() != source.root_archive_node_id
        || public_root.information.snapshot_id != source.root_public_snapshot_id
    {
        return Err(
            "combat damage repair historical root identity disagrees with artifact".to_string(),
        );
    }

    let selectors = proof
        .steps
        .iter()
        .map(|step| {
            parse_action_selector_v1(
                &step.action,
                "combat-damage-challenge artifact",
                sts_presentation::Locale::En,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let compared =
        controller.compare_action_lines_v1(&[selectors], sts_presentation::Locale::En)?;
    let compared = compared
        .lines
        .into_iter()
        .next()
        .ok_or_else(|| "combat damage repair source comparison returned no line".to_string())?;
    if compared.steps.len() != proof.steps.len() {
        return Err("combat damage repair source replay length drifted".to_string());
    }
    for (index, (observed, recorded)) in compared.steps.iter().zip(&proof.steps).enumerate() {
        if observed.source_decision.information.snapshot_id != recorded.source_public_snapshot_id {
            return Err(format!(
                "combat damage repair source step {} snapshot drifted",
                index + 1
            ));
        }
    }

    let original_root = controller.current_combat_position_for_lab_v1()?;
    let source_choices = compared
        .steps
        .iter()
        .map(|step| step.choice.clone())
        .collect::<Vec<_>>();
    let source_inputs = resolve_target_inputs_v1(&original_root, &source_choices, 250)?;
    let source_line =
        materialize_exact_action_line(&EngineCombatStepper, &original_root, &source_inputs, 250)
            .map_err(|error| {
                format!("combat damage repair could not materialize source: {error:?}")
            })?;
    let replayed_source_outcome = controller.exact_combat_replay_outcome_v1(&source_inputs)?;
    if replayed_source_outcome != candidate.outcome {
        return Err(format!(
            "combat damage repair source public outcome drifted: artifact={:?} replay={:?}",
            candidate.outcome, replayed_source_outcome
        ));
    }

    let prefix_steps = &proof.steps[..fixed_prefix_actions];
    let prefix_actions = source_line.actions[..fixed_prefix_actions].to_vec();
    let repair_root = if prefix_actions.is_empty() {
        original_root.clone()
    } else {
        materialize_exact_action_line(
            &EngineCombatStepper,
            &original_root,
            &prefix_actions
                .iter()
                .map(|action| action.input.clone())
                .collect::<Vec<_>>(),
            250,
        )
        .map_err(|error| format!("combat damage repair could not materialize prefix: {error:?}"))?
        .final_position
    };
    let fixed_prefix_potions_consumed =
        exact_trajectory_potion_expenditures(&original_root, &prefix_actions, &repair_root);
    let maximum_suffix_potions = candidate
        .outcome
        .total_potions_consumed
        .checked_sub(fixed_prefix_potions_consumed)
        .ok_or_else(|| {
            "combat damage repair prefix spends more potions than the source outcome".to_string()
        })?;
    let repair_decision_root = CombatDecisionRoot::new(repair_root.clone()).map_err(|error| {
        format!("combat damage repair prefix is not a player-turn root: {error:?}")
    })?;

    let search_spec = LocalTurnGraphSearchSpecV1::from_controls(
        options.max_generation_work_per_challenger,
        options.max_selections_per_challenger,
        options.wall_ms_per_challenger,
        250,
        50_000,
        4,
        32,
        Some(maximum_suffix_potions),
        false,
        None,
        None,
        None,
    );
    let policy = combat_plan_state_guide_policy_v1(retained_combat_tactical_policy_v1());
    let mut search = LocalTurnGraphWitnessSession::with_policy(
        repair_decision_root,
        search_spec.planner_config(OracleCombatWitnessSatisfaction::BudgetOrExhaustion),
        policy,
    );
    let source_suffix = replay_oracle_combat_witness(
        &repair_root,
        &source_line.actions[fixed_prefix_actions..],
        0.0,
        OracleCombatWitnessDiscoverySource::RestoredExactActions,
        &EngineCombatStepper,
    )
    .map_err(|error| format!("combat damage repair source suffix drifted: {error:?}"))?;
    search.restore_verified_witness(source_suffix)?;

    let search_report = search.advance(search_spec.quantum(), &EngineCombatStepper);
    let selected_actions = search_report
        .witness
        .as_ref()
        .map(|selected| selected.actions.as_slice());
    let fixed_inputs = prefix_actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let observed_terminal_frontier = search
        .witness_frontier()
        .iter()
        .map(|suffix| {
            let mut inputs = fixed_inputs.clone();
            inputs.extend(suffix.actions.iter().map(|action| action.input.clone()));
            LocalTurnGraphTerminalOutcomeSnapshotV1::from_replay_checked_position_v1(
                &original_root,
                &inputs,
                &suffix.final_position,
                prefix_actions.len().saturating_add(suffix.actions.len()),
                suffix.negative_log_policy,
                selected_actions.is_some_and(|selected| selected == suffix.actions),
            )
        })
        .collect();
    let exact_witness_frontier = search
        .witness_frontier()
        .iter()
        .map(|suffix| {
            materialize_damage_repair_witness_v1(
                &controller,
                &original_root,
                &prefix_actions,
                suffix,
                fixed_prefix_actions,
                &candidate.outcome,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let strict_dominating_witnesses = exact_witness_frontier
        .iter()
        .filter(|witness| witness.strictly_dominates_source)
        .cloned()
        .collect::<Vec<_>>();

    Ok(RunPlayCombatDamageRepairV1 {
        source_candidate_index,
        source_candidate_id: candidate.candidate_id.clone(),
        fixed_prefix_actions: prefix_steps
            .iter()
            .map(|step| step.action.clone())
            .collect(),
        repair_root_turn: repair_root.combat.turn.turn_count,
        repair_root_hp: repair_root.combat.entities.player.current_hp,
        repair_root_max_hp: repair_root.combat.entities.player.max_hp,
        source_total_potions_consumed: candidate.outcome.total_potions_consumed,
        fixed_prefix_potions_consumed,
        maximum_suffix_potions,
        bounded_search_only: true,
        absence_means_unknown: true,
        status: if strict_dominating_witnesses.is_empty() {
            RunPlayCombatDamageRepairStatusV1::UnknownInBoundedSearch
        } else {
            RunPlayCombatDamageRepairStatusV1::VerifiedStrictImprovement
        },
        exit: combat_repair_exit_v1(&search_report.status),
        work: RunPlayCombatRepairWorkV1 {
            selections: search_report.counters.selections,
            generation_work: search_report.counters.generation_work,
            engine_steps: search_report.counters.engine_steps,
            exact_nodes: search_report.counters.exact_nodes,
            completed_turn_options: search_report.counters.completed_turn_options,
            terminal_win_options: search_report.counters.terminal_win_options,
        },
        observed_terminal_frontier,
        exact_witness_frontier,
        strict_dominating_witnesses,
        durable_head_unchanged: true,
    })
}

fn materialize_damage_repair_witness_v1(
    controller: &RunPlayControllerV1,
    original_root: &CombatPosition,
    prefix_actions: &[TurnOptionAction],
    suffix: &OracleCombatWitness,
    fixed_action_count: usize,
    source_outcome: &TaskResearchPlanOutcomeV1,
) -> Result<RunPlayCombatDamageRepairWitnessV1, String> {
    let mut combined_actions = prefix_actions.to_vec();
    combined_actions.extend(suffix.actions.clone());
    let combined = replay_oracle_combat_witness(
        original_root,
        &combined_actions,
        suffix.negative_log_policy,
        OracleCombatWitnessDiscoverySource::PlannerSearch,
        &EngineCombatStepper,
    )
    .map_err(|error| format!("combat damage repair combined replay failed: {error:?}"))?;
    let inputs = combined
        .actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let public_outcome = controller.exact_combat_replay_outcome_v1(&inputs)?;
    let total_potions_consumed = exact_trajectory_potion_expenditures(
        original_root,
        &combined.actions,
        &combined.final_position,
    );
    let trace = trace_exact_actions_v1(original_root, &combined.actions, fixed_action_count)?;
    Ok(RunPlayCombatDamageRepairWitnessV1 {
        strictly_dominates_source: public_outcome.strictly_dominates(source_outcome),
        combat_terminal_hp: combined.final_position.combat.entities.player.current_hp,
        combat_terminal_max_hp: combined.final_position.combat.entities.player.max_hp,
        total_potions_consumed,
        fixed_action_count: prefix_actions.len(),
        repair_action_count: suffix.actions.len(),
        total_action_count: combined.actions.len(),
        public_outcome,
        exact_actions: combined.actions,
        trace,
    })
}

pub fn repair_feed_from_combat_artifact_v1(
    paths: RunPlayPathsV1,
    artifact: &RunPlayCombatResearchArtifactV1,
    options: CombatFeedRepairOptionsV1,
) -> Result<RunPlayCombatFeedRepairV1, String> {
    let source = verified_combat_trace_source_from_research_artifact_v1(artifact)?;
    repair_feed_from_verified_source_internal_v1(paths, &source, options, false)
}

fn repair_feed_from_verified_source_internal_v1(
    paths: RunPlayPathsV1,
    source: &VerifiedCombatTraceSourceV1,
    options: CombatFeedRepairOptionsV1,
    continue_to_budget: bool,
) -> Result<RunPlayCombatFeedRepairV1, String> {
    source.validate()?;
    if options.max_generation_work == 0 || options.max_selections == 0 || options.wall_ms == 0 {
        return Err("combat Feed repair requires positive finite search limits".to_string());
    }
    let candidate = source
        .candidates
        .get(options.candidate_index)
        .ok_or_else(|| {
            format!(
                "combat Feed repair candidate {} is outside artifact candidate count {}",
                options.candidate_index,
                source.candidates.len()
            )
        })?;
    let proof = &candidate.replay_proof;
    if options.fixed_prefix_actions == 0 || options.fixed_prefix_actions >= proof.steps.len() {
        return Err(format!(
            "combat Feed repair prefix must retain 1..{} actions",
            proof.steps.len().saturating_sub(1)
        ));
    }

    let selector = format!("n{}", source.root_archive_node_id);
    let mut controller = RunPlayControllerV1::open_at(paths, &selector)?;
    let public_root = controller
        .head()
        .current_decision
        .as_ref()
        .ok_or_else(|| "combat Feed repair historical root has no public decision".to_string())?
        .public_decision_v1()?;
    if controller.head().session_id != source.session_id
        || controller.head().position.archive_node_id() != source.root_archive_node_id
        || public_root.information.snapshot_id != source.root_public_snapshot_id
    {
        return Err(
            "combat Feed repair historical root identity disagrees with artifact".to_string(),
        );
    }

    let prefix_steps = &proof.steps[..options.fixed_prefix_actions];
    let selectors = proof
        .steps
        .iter()
        .map(|step| {
            parse_action_selector_v1(
                &step.action,
                "combat-feed-repair artifact",
                sts_presentation::Locale::En,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let compared =
        controller.compare_action_lines_v1(&[selectors], sts_presentation::Locale::En)?;
    let compared = compared
        .lines
        .into_iter()
        .next()
        .ok_or_else(|| "combat Feed repair source comparison returned no line".to_string())?;
    if compared.steps.len() != proof.steps.len() {
        return Err("combat Feed repair source replay length drifted".to_string());
    }
    for (index, (observed, recorded)) in compared.steps.iter().zip(&proof.steps).enumerate() {
        if observed.source_decision.information.snapshot_id != recorded.source_public_snapshot_id {
            return Err(format!(
                "combat Feed repair source step {} snapshot drifted",
                index + 1
            ));
        }
    }

    let original_root = controller.current_combat_position_for_lab_v1()?;
    let prefix_choices = compared
        .steps
        .iter()
        .take(options.fixed_prefix_actions)
        .map(|step| step.choice.clone())
        .collect::<Vec<_>>();
    let prefix_inputs = resolve_target_inputs_v1(&original_root, &prefix_choices, 250)?;
    let prefix =
        materialize_exact_action_line(&EngineCombatStepper, &original_root, &prefix_inputs, 250)
            .map_err(|error| {
                format!("combat Feed repair could not materialize prefix: {error:?}")
            })?;
    if exact_trajectory_potion_expenditures(&original_root, &prefix.actions, &prefix.final_position)
        != 0
    {
        return Err(
            "combat Feed repair fixed prefix already spends a potion; a zero-potion repair is impossible"
                .to_string(),
        );
    }
    let repair_root = prefix.final_position.clone();
    let repair_decision_root = CombatDecisionRoot::new(repair_root.clone()).map_err(|error| {
        format!("combat Feed repair prefix is not a player-turn root: {error:?}")
    })?;

    let search_spec = LocalTurnGraphSearchSpecV1::from_controls(
        options.max_generation_work,
        options.max_selections,
        options.wall_ms,
        250,
        50_000,
        4,
        32,
        Some(0),
        false,
        None,
        None,
        None,
    );
    let satisfaction = if continue_to_budget {
        OracleCombatWitnessSatisfaction::BudgetOrExhaustion
    } else {
        OracleCombatWitnessSatisfaction::FinalHpAndMaxHpAtLeast {
            final_hp: options.minimum_final_hp,
            final_max_hp: options.minimum_final_max_hp,
        }
    };
    let base = combat_plan_state_guide_policy_v1(retained_combat_tactical_policy_v1());
    let policy = feed_payoff_guide_policy_v1(base, original_root.combat.entities.player.max_hp);
    let mut search = LocalTurnGraphWitnessSession::with_policy(
        repair_decision_root,
        search_spec.planner_config(satisfaction),
        policy,
    );
    let source_choices = compared
        .steps
        .iter()
        .map(|step| step.choice.clone())
        .collect::<Vec<_>>();
    let source_inputs = resolve_target_inputs_v1(&original_root, &source_choices, 250)?;
    let source_line =
        materialize_exact_action_line(&EngineCombatStepper, &original_root, &source_inputs, 250)
            .map_err(|error| {
                format!("combat Feed repair could not materialize source: {error:?}")
            })?;
    if exact_trajectory_potion_expenditures(
        &original_root,
        &source_line.actions,
        &source_line.final_position,
    ) == 0
    {
        let source_suffix = replay_oracle_combat_witness(
            &repair_root,
            &source_line.actions[options.fixed_prefix_actions..],
            0.0,
            OracleCombatWitnessDiscoverySource::RestoredExactActions,
            &EngineCombatStepper,
        )
        .map_err(|error| format!("combat Feed repair source suffix drifted: {error:?}"))?;
        search.restore_verified_witness(source_suffix)?;
    }
    let report = search.advance(search_spec.quantum(), &EngineCombatStepper);
    let selected_actions = report
        .witness
        .as_ref()
        .map(|selected| selected.actions.as_slice());
    let fixed_inputs = prefix
        .actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let observed_terminal_frontier = search
        .witness_frontier()
        .iter()
        .map(|suffix| {
            let mut inputs = fixed_inputs.clone();
            inputs.extend(suffix.actions.iter().map(|action| action.input.clone()));
            LocalTurnGraphTerminalOutcomeSnapshotV1::from_replay_checked_position_v1(
                &original_root,
                &inputs,
                &suffix.final_position,
                prefix.actions.len().saturating_add(suffix.actions.len()),
                suffix.negative_log_policy,
                selected_actions.is_some_and(|selected| selected == suffix.actions),
            )
        })
        .collect();
    let exact_witness_frontier = search
        .witness_frontier()
        .iter()
        .map(|suffix| {
            materialize_combined_repair_witness_v1(
                &original_root,
                &prefix.actions,
                suffix,
                options.fixed_prefix_actions,
                options.minimum_final_hp,
                options.minimum_final_max_hp,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let exit = combat_repair_exit_v1(&report.status);
    let witness = exact_witness_frontier
        .iter()
        .filter(|witness| witness.final_max_hp >= options.minimum_final_max_hp)
        .max_by_key(|witness| (witness.final_hp, witness.final_max_hp))
        .cloned();

    Ok(RunPlayCombatFeedRepairV1 {
        schema: COMBAT_FEED_REPAIR_SCHEMA_V1.to_string(),
        session_id: source.session_id.clone(),
        root_archive_node_id: source.root_archive_node_id.clone(),
        root_public_snapshot_id: source.root_public_snapshot_id.clone(),
        source_candidate_index: options.candidate_index,
        source_candidate_id: candidate.candidate_id.clone(),
        fixed_prefix_actions: prefix_steps
            .iter()
            .map(|step| step.action.clone())
            .collect(),
        repair_root_turn: repair_root.combat.turn.turn_count,
        repair_root_hp: repair_root.combat.entities.player.current_hp,
        repair_root_max_hp: repair_root.combat.entities.player.max_hp,
        requirement: RunPlayCombatFeedRepairRequirementV1 {
            minimum_final_hp: options.minimum_final_hp,
            minimum_final_max_hp: options.minimum_final_max_hp,
            maximum_potions_used: 0,
        },
        bounded_search_only: true,
        absence_means_unknown: true,
        status: if witness
            .as_ref()
            .is_some_and(|witness| witness.satisfies_requirement)
        {
            RunPlayCombatFeedRepairStatusV1::VerifiedRepair
        } else {
            RunPlayCombatFeedRepairStatusV1::UnknownInBoundedSearch
        },
        exit,
        work: RunPlayCombatRepairWorkV1 {
            selections: report.counters.selections,
            generation_work: report.counters.generation_work,
            engine_steps: report.counters.engine_steps,
            exact_nodes: report.counters.exact_nodes,
            completed_turn_options: report.counters.completed_turn_options,
            terminal_win_options: report.counters.terminal_win_options,
        },
        observed_terminal_frontier,
        exact_witness_frontier,
        witness,
        durable_head_unchanged: true,
    })
}

fn materialize_combined_repair_witness_v1(
    original_root: &CombatPosition,
    prefix_actions: &[TurnOptionAction],
    suffix: &OracleCombatWitness,
    fixed_action_count: usize,
    minimum_final_hp: i32,
    minimum_final_max_hp: i32,
) -> Result<RunPlayCombatFeedRepairWitnessV1, String> {
    let mut combined_actions = prefix_actions.to_vec();
    combined_actions.extend(suffix.actions.clone());
    let combined = replay_oracle_combat_witness(
        original_root,
        &combined_actions,
        suffix.negative_log_policy,
        OracleCombatWitnessDiscoverySource::PlannerSearch,
        &EngineCombatStepper,
    )
    .map_err(|error| format!("combat Feed repair combined replay failed: {error:?}"))?;
    let potion_expenditures = exact_trajectory_potion_expenditures(
        original_root,
        &combined.actions,
        &combined.final_position,
    );
    if potion_expenditures != 0 {
        return Err("combat Feed repair admitted a potion expenditure".to_string());
    }
    let trace = trace_exact_actions_v1(original_root, &combined.actions, fixed_action_count)?;
    Ok(RunPlayCombatFeedRepairWitnessV1 {
        satisfies_requirement: combined.final_position.combat.entities.player.current_hp
            >= minimum_final_hp
            && combined.final_position.combat.entities.player.max_hp >= minimum_final_max_hp,
        final_hp: combined.final_position.combat.entities.player.current_hp,
        final_max_hp: combined.final_position.combat.entities.player.max_hp,
        max_hp_gain: combined
            .final_position
            .combat
            .entities
            .player
            .max_hp
            .saturating_sub(original_root.combat.entities.player.max_hp),
        potion_expenditures,
        fixed_action_count: prefix_actions.len(),
        repair_action_count: suffix.actions.len(),
        total_action_count: combined.actions.len(),
        exact_actions: combined.actions,
        trace,
    })
}

fn combat_repair_exit_v1(status: &LocalTurnGraphWitnessStatus) -> RunPlayCombatRepairExitV1 {
    match status {
        LocalTurnGraphWitnessStatus::WitnessFound => RunPlayCombatRepairExitV1::WitnessFound,
        LocalTurnGraphWitnessStatus::Partial(
            LocalTurnGraphWitnessInterruption::SelectionBudget,
        ) => RunPlayCombatRepairExitV1::SelectionBudget,
        LocalTurnGraphWitnessStatus::Partial(
            LocalTurnGraphWitnessInterruption::GenerationWorkBudget,
        ) => RunPlayCombatRepairExitV1::GenerationWorkBudget,
        LocalTurnGraphWitnessStatus::Partial(
            LocalTurnGraphWitnessInterruption::EngineStepBudget,
        ) => RunPlayCombatRepairExitV1::EngineStepBudget,
        LocalTurnGraphWitnessStatus::Partial(LocalTurnGraphWitnessInterruption::Deadline) => {
            RunPlayCombatRepairExitV1::Deadline
        }
        LocalTurnGraphWitnessStatus::FrontierExhausted => {
            RunPlayCombatRepairExitV1::FrontierExhausted
        }
        LocalTurnGraphWitnessStatus::MechanicsGap => RunPlayCombatRepairExitV1::MechanicsGap,
        LocalTurnGraphWitnessStatus::ReplayMismatch(_) => RunPlayCombatRepairExitV1::ReplayMismatch,
    }
}

fn trace_exact_actions_v1(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    fixed_action_count: usize,
) -> Result<Vec<RunPlayCombatRepairActionTraceV1>, String> {
    let mut position = root.clone();
    let mut trace = Vec::with_capacity(actions.len());
    for (index, action) in actions.iter().enumerate() {
        let action_label = semantic_input_label_v1(&position, &action.input);
        let before = position.clone();
        let result = EngineCombatStepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: 250,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return Err(format!(
                "combat repair trace action {} did not stabilize",
                index + 1
            ));
        }
        let after = &result.position;
        let monster_changes = before
            .combat
            .entities
            .monsters
            .iter()
            .filter_map(|monster| {
                let successor = after
                    .combat
                    .entities
                    .monsters
                    .iter()
                    .find(|candidate| candidate.id == monster.id);
                let changed = successor.is_none_or(|successor| {
                    successor.current_hp != monster.current_hp
                        || successor.is_alive_for_action() != monster.is_alive_for_action()
                });
                changed.then(|| RunPlayCombatRepairMonsterDeltaV1 {
                    slot: monster.slot,
                    enemy: format!("{:?}", monster.enemy_id),
                    hp_before: monster.current_hp,
                    hp_after: successor.map(|successor| successor.current_hp),
                    alive_after: successor.map(|successor| successor.is_alive_for_action()),
                })
            })
            .collect();
        trace.push(RunPlayCombatRepairActionTraceV1 {
            step: index + 1,
            phase: if index < fixed_action_count {
                RunPlayCombatRepairActionPhaseV1::FixedPrefix
            } else {
                RunPlayCombatRepairActionPhaseV1::RepairedSuffix
            },
            turn: before.combat.turn.turn_count,
            action: action_label,
            player_hp_before: before.combat.entities.player.current_hp,
            player_hp_after: after.combat.entities.player.current_hp,
            player_max_hp_before: before.combat.entities.player.max_hp,
            player_max_hp_after: after.combat.entities.player.max_hp,
            monster_changes,
        });
        position = result.position;
    }
    Ok(trace)
}

fn semantic_input_label_v1(position: &CombatPosition, input: &ClientInput) -> String {
    match input {
        ClientInput::PlayCard { card_index, target } => {
            let card = position.combat.zones.hand.get(*card_index);
            let target = target.and_then(|target| {
                position
                    .combat
                    .entities
                    .monsters
                    .iter()
                    .find(|monster| monster.id == target)
                    .map(|monster| monster.slot)
            });
            match (card, target) {
                (Some(card), Some(slot)) => {
                    format!("play:{:?}+{}@{slot}", card.id, card.upgrades)
                }
                (Some(card), None) => format!("play:{:?}+{}", card.id, card.upgrades),
                (None, _) => format!("play:hand-index-{card_index}"),
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
                .map(|potion| format!("{:?}", potion.id))
                .unwrap_or_else(|| format!("slot-{potion_index}"));
            let target = target.and_then(|target| {
                position
                    .combat
                    .entities
                    .monsters
                    .iter()
                    .find(|monster| monster.id == target)
                    .map(|monster| monster.slot)
            });
            target.map_or_else(
                || format!("use-potion:{potion}"),
                |slot| format!("use-potion:{potion}@{slot}"),
            )
        }
        ClientInput::DiscardPotion(slot) => format!("discard-potion:slot-{slot}"),
        ClientInput::EndTurn => "end".to_string(),
        ClientInput::SubmitSelection(selection) => format!(
            "select:{}",
            selection
                .selected_card_lineages()
                .iter()
                .map(|lineage| combat_card_label_for_lineage_v1(position, *lineage))
                .collect::<Vec<_>>()
                .join(",")
        ),
        ClientInput::SubmitCardChoice(indices) => format!(
            "select-card-indices:{}",
            indices
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        ClientInput::SubmitDiscoverChoice(index) => format!("choose:{index}"),
        ClientInput::SubmitScryDiscard(indices) => format!(
            "scry-discard:{}",
            indices
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => other.payload_key_v1(),
    }
}

fn combat_card_label_for_lineage_v1(
    position: &CombatPosition,
    lineage: sts_oracle_learning::runtime::combat::CardLineageId,
) -> String {
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
        .map(|card| format!("{:?}+{}#{}", card.id, card.upgrades, card.lineage))
        .unwrap_or_else(|| format!("unknown-card#{lineage}"))
}

pub fn render_combat_feed_repair_v1(report: &RunPlayCombatFeedRepairV1) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "combat_feed_repair status={:?} exit={:?} candidate={} fixed={} root={}/{} turn={} requirement=hp>={} max_hp>={} potions=0\n",
        report.status,
        report.exit,
        report.source_candidate_index,
        report.fixed_prefix_actions.len(),
        report.repair_root_hp,
        report.repair_root_max_hp,
        report.repair_root_turn,
        report.requirement.minimum_final_hp,
        report.requirement.minimum_final_max_hp,
    ));
    output.push_str(&format!(
        "work selections={} generation={} engine_steps={} exact_nodes={} complete_turns={} terminal_wins={}\n",
        report.work.selections,
        report.work.generation_work,
        report.work.engine_steps,
        report.work.exact_nodes,
        report.work.completed_turn_options,
        report.work.terminal_win_options,
    ));
    output.push_str(&format!(
        "exact_witness_frontier outcomes=[{}]\n",
        report
            .exact_witness_frontier
            .iter()
            .map(|witness| format!(
                "{}/{} potions={} actions={}",
                witness.final_hp,
                witness.final_max_hp,
                witness.potion_expenditures,
                witness.total_action_count,
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    if let Some(witness) = &report.witness {
        output.push_str(&format!(
            "best_feed_witness satisfies_requirement={} final={}/{} max_hp_gain={} actions={}+{}={} potions={}\n",
            witness.satisfies_requirement,
            witness.final_hp,
            witness.final_max_hp,
            witness.max_hp_gain,
            witness.fixed_action_count,
            witness.repair_action_count,
            witness.total_action_count,
            witness.potion_expenditures,
        ));
        for step in &witness.trace {
            let monsters = step
                .monster_changes
                .iter()
                .map(|monster| {
                    format!(
                        "{}@{}:{}->{} alive={}",
                        monster.enemy,
                        monster.slot,
                        monster.hp_before,
                        monster
                            .hp_after
                            .map(|hp| hp.to_string())
                            .unwrap_or_else(|| "gone".to_string()),
                        monster
                            .alive_after
                            .map(|alive| alive.to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            output.push_str(&format!(
                "  step={} phase={:?} turn={} action={} hp={}->{} max_hp={}->{}{}\n",
                step.step,
                step.phase,
                step.turn,
                step.action,
                step.player_hp_before,
                step.player_hp_after,
                step.player_max_hp_before,
                step.player_max_hp_after,
                if monsters.is_empty() {
                    String::new()
                } else {
                    format!(" monsters=[{monsters}]")
                },
            ));
        }
    } else {
        output.push_str("no Feed-payoff witness found; bounded absence remains Unknown\n");
    }
    output
}

pub fn render_combat_feed_challenge_v2(report: &RunPlayCombatFeedChallengeV2) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "combat_feed_challenge frontier={:?} eligible={} attempts={} challenger_limit={} global_frontier={} durable_head_unchanged={}\n",
        report.source_frontier_candidate_indices,
        report.eligible_challenger_count,
        report.attempts.len(),
        report.challenger_limit,
        report.global_resource_frontier.len(),
        report.durable_head_unchanged,
    ));
    for attempt in &report.attempts {
        output.push_str(&format!(
            "source={} final={}/{} potions={} trigger_step={} turn={} replaced={} target={:?}@{} hp={} block={} remaining_after_nonfatal={:?}\n",
            attempt.source_candidate_index,
            attempt.source_final_hp,
            attempt.source_final_max_hp,
            attempt.source_total_potions_consumed,
            attempt.trigger.step,
            attempt.trigger.turn,
            attempt.replaced_action,
            attempt.trigger.target,
            attempt.trigger.target_slot,
            attempt.trigger.target_hp_before,
            attempt.trigger.target_block_before,
            attempt.trigger.remaining_hp_after_nonfatal,
        ));
        if let Some(witness) = &attempt.report.witness {
            output.push_str(&format!(
                "challenger final={}/{} max_hp_gain={} potions={} fixed={} suffix={} exit={:?} work_generation={}\n",
                witness.final_hp,
                witness.final_max_hp,
                witness.max_hp_gain,
                witness.potion_expenditures,
                witness.fixed_action_count,
                witness.repair_action_count,
                attempt.report.exit,
                attempt.report.work.generation_work,
            ));
        } else {
            output.push_str(&format!(
                "challenger none exit={:?} work_generation={}; bounded absence remains Unknown\n",
                attempt.report.exit, attempt.report.work.generation_work,
            ));
        }
        output.push_str(&format!(
            "challenger_frontier=[{}]\n",
            attempt
                .report
                .exact_witness_frontier
                .iter()
                .map(|witness| format!(
                    "{}/{} max_hp_gain={} potions={} actions={}",
                    witness.final_hp,
                    witness.final_max_hp,
                    witness.max_hp_gain,
                    witness.potion_expenditures,
                    witness.total_action_count,
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if report.attempts.is_empty() {
        output.push_str(
            "no non-dominated potion-free witness spent Feed nonfatally; no suffix challenge was opened\n",
        );
    }
    for member in &report.global_resource_frontier {
        output.push_str(&format!(
            "global_frontier attempt={} witness={} public={}/{} terminal={}/{} max_hp_gain={} potions={} actions={}\n",
            member.attempt_index,
            member.witness_index,
            member.public_outcome.final_hp,
            member.public_outcome.persistent.max_hp,
            member.combat_terminal_hp,
            member.combat_terminal_max_hp,
            member.max_hp_gain,
            member.total_potions_consumed,
            member.total_action_count,
        ));
    }
    output
}

pub fn render_combat_damage_challenge_v1(report: &RunPlayCombatDamageChallengeV1) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "combat_damage_challenge frontier={:?} eligible={} attempts={} challenger_limit={} global_frontier={} durable_head_unchanged={}\n",
        report.source_frontier_candidate_indices,
        report.eligible_challenger_count,
        report.attempts.len(),
        report.challenger_limit,
        report.global_resource_frontier.len(),
        report.durable_head_unchanged,
    ));
    for attempt in &report.attempts {
        output.push_str(&format!(
            "source={} public={}/{} potions={} cutpoint_step={} turn={}->{} action={} hp_loss={} fixed={} status={:?} exit={:?}\n",
            attempt.source_candidate_index,
            attempt.source_outcome.final_hp,
            attempt.source_outcome.persistent.max_hp,
            attempt.source_outcome.total_potions_consumed,
            attempt.trigger.step,
            attempt.trigger.turn_before,
            attempt.trigger.turn_after,
            attempt.trigger.action,
            attempt.trigger.observed_hp_loss,
            attempt.retained_prefix_actions,
            attempt.report.status,
            attempt.report.exit,
        ));
        output.push_str(&format!(
            "  root={}/{} turn={} potion_budget=fixed:{}+suffix:{} frontier={} strict_improvements={} work_generation={}\n",
            attempt.report.repair_root_hp,
            attempt.report.repair_root_max_hp,
            attempt.report.repair_root_turn,
            attempt.report.fixed_prefix_potions_consumed,
            attempt.report.maximum_suffix_potions,
            attempt.report.exact_witness_frontier.len(),
            attempt.report.strict_dominating_witnesses.len(),
            attempt.report.work.generation_work,
        ));
        for witness in &attempt.report.exact_witness_frontier {
            output.push_str(&format!(
                "  challenger public={}/{} terminal={}/{} potions={} actions={} dominates_source={}\n",
                witness.public_outcome.final_hp,
                witness.public_outcome.persistent.max_hp,
                witness.combat_terminal_hp,
                witness.combat_terminal_max_hp,
                witness.total_potions_consumed,
                witness.total_action_count,
                witness.strictly_dominates_source,
            ));
        }
        if attempt.report.strict_dominating_witnesses.is_empty() {
            output.push_str("  no strict same-or-lower-potion improvement found; bounded absence remains Unknown\n");
        }
    }
    if report.attempts.is_empty() {
        output.push_str(
            "no turn-advancing HP-loss action occurred in the non-dominated source frontier\n",
        );
    }
    for member in &report.global_resource_frontier {
        output.push_str(&format!(
            "global_frontier attempt={} witness={} public={}/{} terminal={}/{} potions={} actions={}\n",
            member.attempt_index,
            member.witness_index,
            member.public_outcome.final_hp,
            member.public_outcome.persistent.max_hp,
            member.combat_terminal_hp,
            member.combat_terminal_max_hp,
            member.total_potions_consumed,
            member.total_action_count,
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_protocol::{
        AgentCombatResearchReplayEndpointV1, AgentCombatResearchReplayStepV1,
    };
    use crate::combat_query::{
        RunPlayCombatTrajectoryQueryCommitArtifactV2, RunPlayCombatTrajectoryQueryCommitProofV2,
    };
    use sts_combat_planner::CombatTrajectoryQueryV1;
    use sts_oracle_learning::agent::task::TaskResearchPlanPersistentOutcomeV1;

    #[test]
    fn repair_trace_does_not_attribute_a_summoned_torch_to_the_collector() {
        use sts_oracle_learning::content::monsters::factory::EncounterId;
        use sts_oracle_learning::state::map::node::RoomType;
        let mut session = crate::decision_tests::fixture("combat_multiturn");
        let (engine, combat) = sts_oracle_learning::sim::combat_start::build_natural_combat_start(
            &mut session.run_state,
            EncounterId::Collector,
            RoomType::MonsterRoomBoss,
        )
        .unwrap();
        let trace = trace_exact_actions_v1(
            &CombatPosition::new(engine, combat),
            &[TurnOptionAction {
                input: ClientInput::EndTurn,
                expected_successor_hash: String::new().into(),
                engine_steps: 0,
            }],
            1,
        )
        .unwrap();
        assert_eq!(trace.len(), 1);
        assert!(
            trace[0].monster_changes.is_empty(),
            "{:?}",
            trace[0].monster_changes
        );
    }

    fn test_public_outcome(final_hp: i32, max_hp: i32) -> TaskResearchPlanOutcomeV1 {
        TaskResearchPlanOutcomeV1 {
            final_hp,
            final_gold: 137,
            persistent: TaskResearchPlanPersistentOutcomeV1 {
                max_hp,
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
        }
    }

    fn test_feed_frontier_member(
        final_hp: i32,
        max_hp: i32,
    ) -> RunPlayCombatFeedGlobalFrontierMemberV2 {
        RunPlayCombatFeedGlobalFrontierMemberV2 {
            attempt_index: 0,
            witness_index: 0,
            public_outcome: test_public_outcome(final_hp, max_hp),
            combat_terminal_hp: final_hp,
            combat_terminal_max_hp: max_hp,
            max_hp_gain: 4,
            total_potions_consumed: 0,
            total_action_count: 1,
            exact_actions: vec![TurnOptionAction {
                input: ClientInput::EndTurn,
                expected_successor_hash: "successor".into(),
                engine_steps: 1,
            }],
        }
    }

    #[test]
    fn feed_challenge_promotion_schema_keeps_v1_inspection_only() {
        let legacy = require_combat_feed_challenge_schema_v2(COMBAT_FEED_CHALLENGE_SCHEMA_V1)
            .expect_err("legacy challenge must not become executable");
        assert!(legacy.contains("inspection-only"));
        require_combat_feed_challenge_schema_v2(COMBAT_FEED_CHALLENGE_SCHEMA_V2)
            .expect("V2 challenge carries promotion evidence");
    }

    #[test]
    fn feed_global_frontier_drops_only_strictly_dominated_public_outcomes() {
        let low = test_feed_frontier_member(80, 120);
        let mut high = test_feed_frontier_member(100, 120);
        high.witness_index = 1;

        let retained = retain_global_feed_frontier_v2(vec![low, high.clone()]);

        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].public_outcome, high.public_outcome);
        validate_global_feed_frontier_v2(&retained)
            .expect("retained promotion frontier must be internally consistent");
    }

    #[test]
    fn feed_global_frontier_rejects_potion_or_action_drift() {
        let mut potion = test_feed_frontier_member(100, 120);
        potion.total_potions_consumed = 1;
        assert!(validate_global_feed_frontier_v2(&[potion]).is_err());

        let mut actions = test_feed_frontier_member(100, 120);
        actions.total_action_count = 2;
        assert!(validate_global_feed_frontier_v2(&[actions]).is_err());
    }

    #[test]
    fn query_commit_proof_projects_to_one_verified_damage_source_candidate() {
        let public_outcome = TaskResearchPlanOutcomeV1 {
            final_hp: 26,
            final_gold: 137,
            persistent: TaskResearchPlanPersistentOutcomeV1 {
                max_hp: 89,
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
        };
        let artifact = RunPlayCombatTrajectoryQueryCommitArtifactV2 {
            session_id: "session".to_string(),
            root_archive_node_id: "task_run_archive_node_v1:root".to_string(),
            root_public_snapshot_id: "public_information_snapshot_v1:root".to_string(),
            root_exact_state_hash: "exact".to_string(),
            query: CombatTrajectoryQueryV1::True,
            proof: RunPlayCombatTrajectoryQueryCommitProofV2 {
                semantic_action_line: vec!["end".to_string()],
                replay_steps: vec![AgentCombatResearchReplayStepV1 {
                    source_public_snapshot_id: "public_information_snapshot_v1:root".to_string(),
                    action: "end".to_string(),
                }],
                endpoint: AgentCombatResearchReplayEndpointV1::PublicDecision {
                    public_snapshot_id: "public_information_snapshot_v1:next".to_string(),
                },
                public_outcome: public_outcome.clone(),
                combat_terminal_hp: 26,
                combat_terminal_max_hp: 89,
                potion_expenditures: 0,
                exact_action_count: 1,
            },
        };

        let source = verified_combat_trace_source_from_query_v2(&artifact)
            .expect("V2 query proof should be a damage source");

        assert_eq!(source.source_label, COMBAT_TRAJECTORY_QUERY_SCHEMA_V2);
        assert_eq!(source.candidates.len(), 1);
        assert_eq!(source.candidates[0].outcome, public_outcome);
        assert_eq!(source.candidates[0].replay_proof.steps[0].action, "end");
        source
            .validate()
            .expect("projected source remains verified");
    }
}
