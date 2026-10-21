//! Selects exact witness states that are valuable to reanalyse next.
//!
//! The selector is deliberately read-only. It never labels a low-probability
//! action as bad and never changes the runtime policy. Its only authority is
//! computational: order verified witness states so bounded exact successor
//! reanalysis is spent where the current policy has the least support for a
//! known winning corridor.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::PathBuf;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sts_combat_planner::{CombatActionPolicy, CombatPolicyChoice};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_action_imitation::{
    concrete_combat_action_candidates_for_witness_v1, exact_witness_adjacent_accepted_indices_v1,
};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::sim::combat_action::combat_action_key;
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_trace_view::combat_action_label;
use super::exact_turn_corridor::load_corpus as load_combat_action_imitation_corpus;
use super::oracle_lab_runtime_identity;

const QUEUE_SCHEMA: &str = "ActionReanalysisQueueV1";

#[derive(Debug, Args)]
pub(crate) struct ActionReanalysisQueueArgs {
    /// Exact terminal witness manifest whose decisions are inspected.
    #[arg(long)]
    manifest: PathBuf,
    /// Current residual action policy to audit over the mature base policy.
    #[arg(long)]
    action_imitation_artifact: PathBuf,
    /// Destination for the read-only, deduplicated state queue.
    #[arg(long)]
    output: PathBuf,
    /// Global number of exact states retained for later reanalysis.
    #[arg(long, default_value_t = 24)]
    max_states: usize,
    /// Prevent one long witness from monopolizing the global queue.
    #[arg(long, default_value_t = 6)]
    max_states_per_demonstration: usize,
    /// Maximum canonical structured-selection inputs materialized.
    #[arg(long, default_value_t = 256)]
    max_structured_alternatives: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Debug, Args)]
pub(crate) struct ActionReanalysisBatchArgs {
    /// Queue produced by `build-action-reanalysis-queue`.
    #[arg(long)]
    queue: PathBuf,
    /// Directory receiving one typed successor corpus per selected state.
    #[arg(long)]
    output_dir: PathBuf,
    /// Compact batch report containing the generated corpus paths.
    #[arg(long)]
    report: PathBuf,
    /// Number of highest-priority queue items to reanalyse.
    #[arg(long, default_value_t = 4)]
    take: usize,
    /// Number of higher-priority queue items already processed.
    #[arg(long, default_value_t = 0)]
    skip: usize,
    /// Deterministic exact-search work for each non-terminal successor.
    #[arg(long, default_value_t = 5_000)]
    solve_work_per_candidate: usize,
    /// Maximum independent successors evaluated concurrently within a state.
    #[arg(long, default_value_t = 4)]
    candidate_jobs: usize,
    /// Optional legacy-teacher allowance for exact-search BudgetUnknown
    /// successors. Zero disables teacher proposals.
    #[arg(long, default_value_t = 0)]
    v2_teacher_wall_ms_per_candidate: u64,
    #[arg(long, default_value_t = 800_000)]
    v2_teacher_max_nodes_per_candidate: usize,
    /// Maximum canonical structured-selection inputs materialized.
    #[arg(long, default_value_t = 256)]
    max_structured_alternatives: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ActionReanalysisQueueItem {
    pub(crate) queue_rank: usize,
    pub(crate) demonstration_id: String,
    pub(crate) source_case: PathBuf,
    pub(crate) source_actions: Vec<PathBuf>,
    pub(crate) through: usize,
    pub(crate) source_action_count: usize,
    pub(crate) witness_progress: f64,
    pub(crate) player_turn: u32,
    pub(crate) exact_state_hash: String,
    pub(crate) candidate_count: usize,
    pub(crate) accepted_action_count: usize,
    pub(crate) accepted_policy_probability: f64,
    pub(crate) demonstrated_rank: usize,
    pub(crate) demonstrated_policy_probability: f64,
    pub(crate) demonstrated_input: ClientInput,
    pub(crate) demonstrated_action_key: String,
    pub(crate) demonstrated_label: String,
    pub(crate) best_input: ClientInput,
    pub(crate) best_action_key: String,
    pub(crate) best_label: String,
    pub(crate) best_is_witness_compatible: bool,
    pub(crate) policy_entropy: f64,
    pub(crate) selection_class: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActionReanalysisQueueCorpus {
    pub(crate) schema_name: String,
    pub(crate) schema_version: u32,
    pub(crate) action_imitation_artifact: PathBuf,
    pub(crate) queue: Vec<ActionReanalysisQueueItem>,
}

pub(crate) fn build(args: ActionReanalysisQueueArgs) -> Result<Value, String> {
    if args.max_states == 0
        || args.max_states_per_demonstration == 0
        || args.max_structured_alternatives == 0
        || args.max_engine_steps_per_transition == 0
    {
        return Err("action reanalysis queue limits must be positive".to_string());
    }

    let demonstrations = load_combat_action_imitation_corpus(&args.manifest)?;
    let policy = super::combat_policy_controls::load_action_imitation_policy(
        &args.action_imitation_artifact,
        retained_combat_tactical_policy_v1(),
    )?;
    let mut scanned_decisions = 0usize;
    let mut forced_decisions = 0usize;
    let mut per_demonstration = Vec::new();

    for demonstration in demonstrations {
        let mut position = demonstration.position;
        let mut profiles = Vec::new();
        for (action_index, demonstrated) in demonstration.actions.iter().enumerate() {
            if !EngineCombatStepper.is_legal_action(&position, demonstrated) {
                return Err(format!(
                    "demonstration {:?} action {action_index} is not legal",
                    demonstration.id
                ));
            }
            let candidates = concrete_combat_action_candidates_for_witness_v1(
                &position,
                demonstrated,
                args.max_structured_alternatives,
            );
            let demonstrated_index = candidates
                .iter()
                .position(|candidate| candidate == demonstrated)
                .ok_or_else(|| {
                    format!(
                        "demonstration {:?} action {action_index} is absent from its candidate surface",
                        demonstration.id
                    )
                })?;
            if candidates.len() > 1 {
                scanned_decisions = scanned_decisions.saturating_add(1);
                profiles.push(profile_decision(
                    &demonstration.id,
                    &demonstration.case_path,
                    &demonstration.action_paths,
                    demonstration.actions.len(),
                    action_index,
                    demonstrated_index,
                    demonstrated,
                    &demonstration.actions,
                    &position,
                    &candidates,
                    policy.as_ref(),
                    args.max_engine_steps_per_transition,
                )?);
            } else {
                forced_decisions = forced_decisions.saturating_add(1);
            }

            let step = EngineCombatStepper.apply_to_stable(
                &position,
                demonstrated.clone(),
                CombatStepLimits {
                    max_engine_steps: args.max_engine_steps_per_transition,
                    deadline: None,
                },
            );
            if step.truncated || step.timed_out {
                return Err(format!(
                    "demonstration {:?} action {action_index} did not reach a stable successor",
                    demonstration.id
                ));
            }
            position = step.position;
        }
        if EngineCombatStepper.terminal(&position) != CombatTerminal::Win
            || position.combat.runtime.combat_smoked
        {
            return Err(format!(
                "demonstration {:?} is not an exact non-smoke victory",
                demonstration.id
            ));
        }
        profiles.sort_by(compare_queue_items);
        profiles.truncate(args.max_states_per_demonstration);
        per_demonstration.extend(profiles);
    }

    per_demonstration.sort_by(compare_queue_items);
    let mut seen_hashes = HashSet::new();
    let mut duplicate_state_count = 0usize;
    let mut queue = Vec::new();
    for mut item in per_demonstration {
        if !seen_hashes.insert(item.exact_state_hash.clone()) {
            duplicate_state_count = duplicate_state_count.saturating_add(1);
            continue;
        }
        item.queue_rank = queue.len().saturating_add(1);
        queue.push(item);
        if queue.len() == args.max_states {
            break;
        }
    }

    let corpus = json!({
        "schema_name": QUEUE_SCHEMA,
        "schema_version": 1,
        "runtime": oracle_lab_runtime_identity(),
        "manifest": args.manifest,
        "action_imitation_artifact": args.action_imitation_artifact,
        "selection_contract": {
            "authority": "compute_order_only",
            "known_witness": "exact_terminal_non_smoke_win",
            "accepted_support": "demonstrated_action_plus_exact_adjacent_swap_wins",
            "ordering": [
                "top1_outside_accepted_first",
                "lower_accepted_policy_probability_first",
                "earlier_witness_progress_first",
                "higher_candidate_count_first",
                "stable_identity"
            ],
            "non_claims": [
                "low_probability_is_not_negative_evidence",
                "witness_action_is_not_assumed_unique",
                "budget_unknown_is_not_a_loss"
            ]
        },
        "config": {
            "max_states": args.max_states,
            "max_states_per_demonstration": args.max_states_per_demonstration,
            "max_structured_alternatives": args.max_structured_alternatives,
            "max_engine_steps_per_transition": args.max_engine_steps_per_transition,
        },
        "scanned_ranked_decision_count": scanned_decisions,
        "skipped_forced_decision_count": forced_decisions,
        "deduplicated_state_count": queue.len(),
        "duplicate_state_count": duplicate_state_count,
        "queue": queue,
    });
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        &args.output,
        serde_json::to_vec_pretty(&corpus).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "schema_name": "ActionReanalysisQueueBuildReportV1",
        "schema_version": 1,
        "output": args.output,
        "scanned_ranked_decision_count": scanned_decisions,
        "skipped_forced_decision_count": forced_decisions,
        "deduplicated_state_count": queue.len(),
        "duplicate_state_count": duplicate_state_count,
        "queue": queue,
    }))
}

#[allow(clippy::too_many_arguments)]
fn profile_decision(
    demonstration_id: &str,
    source_case: &PathBuf,
    source_actions: &[PathBuf],
    source_action_count: usize,
    action_index: usize,
    demonstrated_index: usize,
    demonstrated: &ClientInput,
    witness_actions: &[ClientInput],
    position: &CombatPosition,
    candidates: &[ClientInput],
    policy: &dyn CombatActionPolicy,
    max_engine_steps_per_transition: usize,
) -> Result<ActionReanalysisQueueItem, String> {
    let choices = candidates
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .collect::<Vec<_>>();
    let raw_weights = policy.weights(position, &choices);
    let safe_weights = if raw_weights.len() == candidates.len() {
        raw_weights
            .into_iter()
            .map(|weight| {
                if weight.is_finite() && weight > 0.0 {
                    weight
                } else {
                    1.0
                }
            })
            .collect::<Vec<_>>()
    } else {
        vec![1.0; candidates.len()]
    };
    let max_weight = safe_weights
        .iter()
        .copied()
        .fold(f64::MIN_POSITIVE, f64::max);
    let scaled_total = safe_weights
        .iter()
        .map(|weight| weight / max_weight)
        .sum::<f64>()
        .max(f64::MIN_POSITIVE);
    let uniform = 1.0 / candidates.len() as f64;
    let probabilities = safe_weights
        .iter()
        .map(|weight| 0.95 * ((*weight / max_weight) / scaled_total) + 0.05 * uniform)
        .collect::<Vec<_>>();
    let accepted_indices = exact_witness_adjacent_accepted_indices_v1(
        &EngineCombatStepper,
        position,
        witness_actions,
        action_index,
        candidates,
        demonstrated_index,
        max_engine_steps_per_transition,
    );
    let best_index = safe_weights
        .iter()
        .enumerate()
        .max_by(|(left_index, left), (right_index, right)| {
            left.total_cmp(right)
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index)
        .unwrap_or_default();
    let demonstrated_weight = safe_weights[demonstrated_index];
    let demonstrated_rank = 1 + safe_weights
        .iter()
        .enumerate()
        .filter(|(candidate_index, weight)| {
            weight.total_cmp(&demonstrated_weight).is_gt()
                || (weight.total_cmp(&demonstrated_weight).is_eq()
                    && *candidate_index < demonstrated_index)
        })
        .count();
    let accepted_policy_probability = accepted_indices
        .iter()
        .map(|index| probabilities[*index])
        .sum::<f64>();
    let best_is_witness_compatible = accepted_indices.contains(&best_index);
    let selection_class = if !best_is_witness_compatible {
        "Top1OutsideAccepted"
    } else if accepted_policy_probability < uniform {
        "AcceptedSupportBelowUniform"
    } else {
        "WitnessCompatibleTop1"
    };
    let entropy = -probabilities
        .iter()
        .filter(|probability| **probability > 0.0)
        .map(|probability| probability * probability.ln())
        .sum::<f64>()
        / (candidates.len() as f64).ln().max(f64::MIN_POSITIVE);

    Ok(ActionReanalysisQueueItem {
        queue_rank: 0,
        demonstration_id: demonstration_id.to_string(),
        source_case: source_case.clone(),
        source_actions: source_actions.to_vec(),
        through: action_index,
        source_action_count,
        witness_progress: action_index as f64 / source_action_count.max(1) as f64,
        player_turn: position.combat.turn.turn_count,
        exact_state_hash: combat_exact_state_hash_v2(&position.engine, &position.combat),
        candidate_count: candidates.len(),
        accepted_action_count: accepted_indices.len(),
        accepted_policy_probability,
        demonstrated_rank,
        demonstrated_policy_probability: probabilities[demonstrated_index],
        demonstrated_input: demonstrated.clone(),
        demonstrated_action_key: combat_action_key(&position.combat, demonstrated),
        demonstrated_label: combat_action_label(position, demonstrated),
        best_input: candidates[best_index].clone(),
        best_action_key: combat_action_key(&position.combat, &candidates[best_index]),
        best_label: combat_action_label(position, &candidates[best_index]),
        best_is_witness_compatible,
        policy_entropy: entropy,
        selection_class: selection_class.to_string(),
    })
}

fn compare_queue_items(
    left: &ActionReanalysisQueueItem,
    right: &ActionReanalysisQueueItem,
) -> Ordering {
    left.best_is_witness_compatible
        .cmp(&right.best_is_witness_compatible)
        .then_with(|| {
            left.accepted_policy_probability
                .total_cmp(&right.accepted_policy_probability)
        })
        .then_with(|| left.witness_progress.total_cmp(&right.witness_progress))
        .then_with(|| right.candidate_count.cmp(&left.candidate_count))
        .then_with(|| left.demonstration_id.cmp(&right.demonstration_id))
        .then_with(|| left.through.cmp(&right.through))
        .then_with(|| left.exact_state_hash.cmp(&right.exact_state_hash))
}

pub(crate) fn load_queue(path: &PathBuf) -> Result<ActionReanalysisQueueCorpus, String> {
    let queue = serde_json::from_slice::<ActionReanalysisQueueCorpus>(
        &std::fs::read(path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "invalid action reanalysis queue {}: {error}",
            path.display()
        )
    })?;
    if queue.schema_name != QUEUE_SCHEMA || queue.schema_version != 1 {
        return Err(format!(
            "unsupported action reanalysis queue schema in {}",
            path.display()
        ));
    }
    Ok(queue)
}

pub(crate) fn build_batch(args: ActionReanalysisBatchArgs) -> Result<Value, String> {
    if args.take == 0
        || args.solve_work_per_candidate == 0
        || args.candidate_jobs == 0
        || args.max_structured_alternatives == 0
        || args.max_engine_steps_per_transition == 0
    {
        return Err("action reanalysis batch limits must be positive".to_string());
    }
    if args.v2_teacher_wall_ms_per_candidate > 0 && args.v2_teacher_max_nodes_per_candidate == 0 {
        return Err(
            "--v2-teacher-max-nodes-per-candidate must be positive when the teacher is enabled"
                .to_string(),
        );
    }
    let queue = load_queue(&args.queue)?;
    if queue.queue.is_empty() {
        return Err("action reanalysis queue contains no states".to_string());
    }
    std::fs::create_dir_all(&args.output_dir).map_err(|error| error.to_string())?;
    let selected = queue
        .queue
        .into_iter()
        .skip(args.skip)
        .take(args.take)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(format!(
            "action reanalysis batch --skip {} is beyond the saved queue",
            args.skip
        ));
    }
    let mut generated = Vec::with_capacity(selected.len());
    for item in selected {
        let safe_id = item
            .demonstration_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let output = args.output_dir.join(format!(
            "{:03}-{safe_id}-a{:04}.action-successor.json",
            item.queue_rank, item.through
        ));
        let build_report = super::action_successor_reanalysis::build(
            super::action_successor_reanalysis::ActionSuccessorReanalysisArgs {
                case: Some(item.source_case.clone()),
                artifact: None,
                expected_roots: None,
                root_slot: 0,
                max_artifact_bytes: 16 * 1024 * 1024,
                actions: item.source_actions.clone(),
                through: item.through,
                output: output.clone(),
                solve_work_per_candidate: args.solve_work_per_candidate,
                candidate_jobs: args.candidate_jobs,
                no_potions: false,
                v2_teacher_wall_ms_per_candidate: args.v2_teacher_wall_ms_per_candidate,
                v2_teacher_max_nodes_per_candidate: args.v2_teacher_max_nodes_per_candidate,
                max_structured_alternatives: args.max_structured_alternatives,
                action_imitation_artifact: Some(queue.action_imitation_artifact.clone()),
                max_engine_steps_per_transition: args.max_engine_steps_per_transition,
            },
        )?;
        generated.push(json!({
            "queue_rank": item.queue_rank,
            "demonstration_id": item.demonstration_id,
            "through": item.through,
            "exact_state_hash": item.exact_state_hash,
            "selection_class": item.selection_class,
            "accepted_policy_probability": item.accepted_policy_probability,
            "output": output,
            "build": build_report,
        }));
    }
    let report = json!({
        "schema_name": "ActionReanalysisBatchReportV1",
        "schema_version": 1,
        "runtime": oracle_lab_runtime_identity(),
        "queue": args.queue,
        "action_imitation_artifact": queue.action_imitation_artifact,
        "config": {
            "take": args.take,
            "skip": args.skip,
            "solve_work_per_candidate": args.solve_work_per_candidate,
            "candidate_jobs": args.candidate_jobs,
            "v2_teacher_wall_ms_per_candidate": args.v2_teacher_wall_ms_per_candidate,
            "v2_teacher_max_nodes_per_candidate": args.v2_teacher_max_nodes_per_candidate,
            "max_structured_alternatives": args.max_structured_alternatives,
            "max_engine_steps_per_transition": args.max_engine_steps_per_transition,
        },
        "generated": generated,
    });
    if let Some(parent) = args.report.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        &args.report,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "schema_name": "ActionReanalysisBatchBuildReportV1",
        "schema_version": 1,
        "report": args.report,
        "generated_count": generated.len(),
        "generated": generated,
    }))
}

#[cfg(test)]
mod tests {
    use super::{compare_queue_items, ActionReanalysisQueueItem};
    use std::path::PathBuf;
    use sts_oracle_runtime::state::core::ClientInput;

    fn item(
        id: &str,
        top1_accepted: bool,
        accepted_probability: f64,
        progress: f64,
    ) -> ActionReanalysisQueueItem {
        ActionReanalysisQueueItem {
            queue_rank: 0,
            demonstration_id: id.to_string(),
            source_case: PathBuf::from("case.json"),
            source_actions: vec![PathBuf::from("actions.json")],
            through: 0,
            source_action_count: 1,
            witness_progress: progress,
            player_turn: 1,
            exact_state_hash: id.to_string(),
            candidate_count: 2,
            accepted_action_count: 1,
            accepted_policy_probability: accepted_probability,
            demonstrated_rank: 1,
            demonstrated_policy_probability: accepted_probability,
            demonstrated_input: ClientInput::EndTurn,
            demonstrated_action_key: "end".to_string(),
            demonstrated_label: "end".to_string(),
            best_input: ClientInput::EndTurn,
            best_action_key: "end".to_string(),
            best_label: "end".to_string(),
            best_is_witness_compatible: top1_accepted,
            policy_entropy: 1.0,
            selection_class: "test".to_string(),
        }
    }

    #[test]
    fn top1_misses_precede_supported_states_without_a_magic_score() {
        let mut items = [
            item("supported", true, 0.01, 0.0),
            item("miss", false, 0.90, 0.9),
        ];
        items.sort_by(compare_queue_items);
        assert_eq!(items[0].demonstration_id, "miss");
    }

    #[test]
    fn lower_accepted_support_then_earlier_progress_break_ties() {
        let mut items = [
            item("later", false, 0.2, 0.8),
            item("earlier", false, 0.2, 0.1),
            item("lower", false, 0.1, 0.9),
        ];
        items.sort_by(compare_queue_items);
        assert_eq!(
            items
                .iter()
                .map(|item| item.demonstration_id.as_str())
                .collect::<Vec<_>>(),
            vec!["lower", "earlier", "later"]
        );
    }
}
