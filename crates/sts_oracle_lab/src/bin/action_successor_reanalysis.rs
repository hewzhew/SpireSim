//! Action-level exact successor reanalysis at one exact combat state.
//!
//! This is the narrow DAgger/Expert-Iteration seam: optionally replay to a
//! state reached by a verified witness, independently search every bounded
//! legal action successor, and preserve exact wins, exact refutations, and
//! budget-unknown results as different evidence kinds. A case-root audit does
//! not privilege one action with a pre-existing witness floor. The command
//! only writes an offline corpus; it cannot alter the production policy.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sts_combat_planner::CombatPolicyChoice;
use sts_oracle_learning::eval::run_control::{
    CombatLearningPotionPolicyV1, CombatLearningRootBatchArtifactV1, LearningCombatBoundaryV1,
    LearningModelDecisionV1, LearningObservationCompletenessV1,
};
use sts_oracle_runtime::agent::information::action::project_public_combat_actions_v1;
use sts_oracle_runtime::agent::information::run::{
    PublicCombatRunContextGapV1, PublicCombatRunContextV1,
};
use sts_oracle_runtime::agent::information::state::public_combat_state_v1;
use sts_oracle_runtime::ai::combat_search_v2::oracle_search_witness_proposal_v1;
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_action_imitation::concrete_combat_action_candidates_v1;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::sim::combat_action::combat_action_key;
use sts_oracle_runtime::sim::combat_action_equivalence::canonical_combat_action_representatives_v1;
use sts_oracle_runtime::sim::combat_action_surface::combat_legal_action_surface_v2;
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_replay_tools::replay_combat_inputs;
use super::combat_trace_view::combat_action_label;
use super::exact_combat_evidence::{
    evaluate_unresolved_position, exact_terminal_non_win, known_exact_win,
    retain_verified_win_floor, ExactCombatEvaluation, ExactCombatEvidence,
};
use super::exact_turn_corridor::load_action_segments as load_combat_action_segments;
use super::oracle_lab_runtime_identity;

const CORPUS_SCHEMA: &str = "ActionSuccessorReanalysisCorpusV2";
const MAX_WITNESS_CORPUS_ACTIONS: usize = 4_096;

#[derive(Debug, Args)]
pub(crate) struct ActionSuccessorReanalysisArgs {
    /// Exact combat case at the beginning of the optional verified witness.
    #[arg(
        long,
        required_unless_present = "artifact",
        conflicts_with = "artifact"
    )]
    pub(crate) case: Option<PathBuf>,
    /// Opaque production combat-root artifact used directly without a case export.
    #[arg(long, required_unless_present = "case", conflicts_with = "case")]
    pub(crate) artifact: Option<PathBuf>,
    /// Declared root width of the opaque artifact.
    #[arg(long)]
    pub(crate) expected_roots: Option<usize>,
    /// Exact root slot selected from the opaque artifact.
    #[arg(long, default_value_t = 0)]
    pub(crate) root_slot: usize,
    /// Maximum accepted opaque artifact bytes.
    #[arg(long, default_value_t = 16 * 1024 * 1024)]
    pub(crate) max_artifact_bytes: usize,
    /// Consecutive exact action segments forming an optional verified witness.
    #[arg(long)]
    pub(crate) actions: Vec<PathBuf>,
    /// Number of witness actions replayed before auditing the next action.
    /// Must remain zero when no witness actions are supplied.
    #[arg(long, default_value_t = 0)]
    pub(crate) through: usize,
    /// Destination for the typed offline evidence corpus.
    #[arg(long)]
    pub(crate) output: PathBuf,
    /// Deterministic exact-search work for each non-terminal successor.
    #[arg(long, default_value_t = 5_000)]
    pub(crate) solve_work_per_candidate: usize,
    /// Maximum independent successor searches evaluated concurrently.
    #[arg(long, default_value_t = 4)]
    pub(crate) candidate_jobs: usize,
    /// Exclude potion use/discard at the root and throughout successor search.
    #[arg(long)]
    pub(crate) no_potions: bool,
    /// Optional legacy-teacher allowance after the exact successor search
    /// returns BudgetUnknown. Zero disables the teacher. A proposal becomes
    /// ExactWin only after full replay from that action successor succeeds.
    #[arg(long, default_value_t = 0)]
    pub(crate) v2_teacher_wall_ms_per_candidate: u64,
    #[arg(long, default_value_t = 800_000)]
    pub(crate) v2_teacher_max_nodes_per_candidate: usize,
    /// Maximum canonical structured-selection inputs materialized.
    #[arg(long, default_value_t = 256)]
    pub(crate) max_structured_alternatives: usize,
    /// Optional learned residual policy whose current action order is recorded.
    /// The artifact affects reporting only; every selected successor receives
    /// the same exact-search budget.
    #[arg(long)]
    pub(crate) action_imitation_artifact: Option<PathBuf>,
    #[arg(long, default_value_t = 250)]
    pub(crate) max_engine_steps_per_transition: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActionSuccessorCandidate {
    canonical_index: usize,
    learning_candidate_ordinal: Option<usize>,
    input: ClientInput,
    label: String,
    action_key: String,
    known_witness_action: bool,
    policy_rank: usize,
    raw_policy_weight: f64,
    policy_probability: f64,
    transition_engine_steps: usize,
    exact_successor_hash: String,
    evidence: ExactCombatEvidence,
    continuation_witness_actions: Option<Vec<ClientInput>>,
}

#[derive(Debug, Deserialize)]
struct ExactWinWitnessCorpusV2 {
    schema_name: String,
    schema_version: u32,
    source_mode: String,
    source_root_id: Option<String>,
    source_expected_roots: Option<usize>,
    source_root_slot: usize,
    source_actions: Vec<PathBuf>,
    through: usize,
    root_exact_state_hash: String,
    config: ExactWinWitnessCorpusConfigV2,
    candidates: Vec<ActionSuccessorCandidate>,
}

#[derive(Debug, Deserialize)]
struct ExactWinWitnessCorpusConfigV2 {
    no_potions: bool,
}

pub(super) struct ExactWinCandidateWitnessV2 {
    pub(super) source_root_id: String,
    pub(super) source_exact_state_hash: String,
    pub(super) source_expected_roots: usize,
    pub(super) source_root_slot: usize,
    pub(super) actions: Vec<ClientInput>,
    pub(super) final_hp: i32,
}

pub(super) fn load_exact_win_candidate_witness_v2(
    path: &Path,
    learning_candidate_ordinal: usize,
    max_bytes: usize,
) -> Result<ExactWinCandidateWitnessV2, String> {
    if max_bytes == 0 {
        return Err("exact-win witness corpus byte limit must be positive".to_owned());
    }
    let mut payload = Vec::new();
    File::open(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?
        .take(
            u64::try_from(max_bytes)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
        .read_to_end(&mut payload)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if payload.is_empty() || payload.len() > max_bytes {
        return Err(format!(
            "exact-win witness corpus bytes must be in 1..={max_bytes}"
        ));
    }
    let corpus = serde_json::from_slice::<ExactWinWitnessCorpusV2>(&payload)
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))?;
    if corpus.schema_name != CORPUS_SCHEMA || corpus.schema_version != 2 {
        return Err("exact-win witness corpus schema is unsupported".to_owned());
    }
    if corpus.source_mode != "opaque_combat_root_artifact"
        || !corpus.source_actions.is_empty()
        || corpus.through != 0
    {
        return Err(
            "exact-win witness corpus is not rooted directly in one opaque artifact".to_owned(),
        );
    }
    if !corpus.config.no_potions {
        return Err("exact-win witness corpus must use the no-potion search lane".to_owned());
    }
    let source_root_id = corpus
        .source_root_id
        .filter(|identity| !identity.is_empty())
        .ok_or_else(|| "exact-win witness corpus is missing its source root identity".to_owned())?;
    let source_expected_roots = corpus
        .source_expected_roots
        .filter(|count| *count > 0)
        .ok_or_else(|| "exact-win witness corpus is missing its source root count".to_owned())?;
    if corpus.source_root_slot >= source_expected_roots {
        return Err("exact-win witness corpus root slot is outside its source width".to_owned());
    }
    let mut candidates = corpus.candidates.into_iter().filter(|candidate| {
        candidate.learning_candidate_ordinal == Some(learning_candidate_ordinal)
    });
    let candidate = candidates.next().ok_or_else(|| {
        "exact-win witness corpus does not contain the requested candidate".to_owned()
    })?;
    if candidates.next().is_some() {
        return Err("exact-win witness corpus repeats the requested learning candidate".to_owned());
    }
    let (final_hp, suffix_action_count) = match candidate.evidence {
        ExactCombatEvidence::ExactWin {
            final_hp,
            suffix_action_count,
            ..
        } => (final_hp, suffix_action_count),
        _ => return Err("requested successor candidate is not an exact win".to_owned()),
    };
    let continuation = candidate
        .continuation_witness_actions
        .ok_or_else(|| "exact-win successor candidate is missing its replay witness".to_owned())?;
    if continuation.len() != suffix_action_count {
        return Err("exact-win successor witness length does not match its evidence".to_owned());
    }
    let actions = std::iter::once(candidate.input)
        .chain(continuation)
        .collect::<Vec<_>>();
    if actions.is_empty() || actions.len() > MAX_WITNESS_CORPUS_ACTIONS {
        return Err(format!(
            "exact-win successor action count must be in 1..={MAX_WITNESS_CORPUS_ACTIONS}"
        ));
    }
    Ok(ExactWinCandidateWitnessV2 {
        source_root_id,
        source_exact_state_hash: corpus.root_exact_state_hash,
        source_expected_roots,
        source_root_slot: corpus.source_root_slot,
        actions,
        final_hp,
    })
}

pub(crate) fn build(args: ActionSuccessorReanalysisArgs) -> Result<Value, String> {
    if args.solve_work_per_candidate == 0
        || args.candidate_jobs == 0
        || args.max_structured_alternatives == 0
        || args.max_engine_steps_per_transition == 0
        || args.max_artifact_bytes == 0
    {
        return Err("action-successor reanalysis budgets must be positive".to_string());
    }
    if args.v2_teacher_wall_ms_per_candidate > 0 && args.v2_teacher_max_nodes_per_candidate == 0 {
        return Err(
            "--v2-teacher-max-nodes-per-candidate must be positive when the teacher is enabled"
                .to_string(),
        );
    }
    if args.no_potions && args.v2_teacher_wall_ms_per_candidate > 0 {
        return Err("--no-potions is incompatible with the legacy V2 teacher".to_string());
    }

    let (source_seed, case_position, root_source_mode, source_root_id) = load_root_position(&args)?;
    let witness_actions = load_combat_action_segments(&args.actions)?;
    if args.artifact.is_some() && !witness_actions.is_empty() {
        return Err("opaque artifact roots do not accept external witness actions".to_string());
    }
    if witness_actions.is_empty() && args.through != 0 {
        return Err("--through must be zero when no witness actions are supplied".to_string());
    }
    if args.through > witness_actions.len() {
        return Err(format!(
            "--through {} exceeds the {} available witness actions",
            args.through,
            witness_actions.len()
        ));
    }
    let (
        root_position,
        known_witness_action,
        known_witness_continuation,
        known_witness_final_hp,
        source_mode,
    ) = if witness_actions.is_empty() {
        (case_position, None, Vec::new(), None, root_source_mode)
    } else {
        let final_position = replay_combat_inputs(
            case_position.clone(),
            &witness_actions,
            args.max_engine_steps_per_transition,
        )?;
        if EngineCombatStepper.terminal(&final_position) != CombatTerminal::Win
            || final_position.combat.runtime.combat_smoked
        {
            return Err("action-successor source is not an exact non-smoke victory".to_string());
        }
        (
            replay_combat_inputs(
                case_position,
                &witness_actions[..args.through],
                args.max_engine_steps_per_transition,
            )?,
            witness_actions.get(args.through).cloned(),
            witness_actions
                .get(args.through.saturating_add(1)..)
                .unwrap_or_default()
                .to_vec(),
            Some(final_position.combat.entities.player.current_hp),
            "verified_witness_prefix",
        )
    };
    if EngineCombatStepper.terminal(&root_position) != CombatTerminal::Unresolved {
        return Err("action-successor audit root is already terminal".to_string());
    }
    let legal_actions =
        combat_legal_action_surface_v2(&root_position.engine, &root_position.combat);
    let atomic_action_representatives = canonical_combat_action_representatives_v1(
        &root_position.engine,
        &root_position.combat,
        &legal_actions.atomic_actions,
    );
    let mut inputs =
        concrete_combat_action_candidates_v1(&root_position, args.max_structured_alternatives);
    inputs.retain(|input| {
        let Some(atomic_ordinal) = legal_actions
            .atomic_actions
            .iter()
            .position(|candidate| candidate == input)
        else {
            return true;
        };
        atomic_action_representatives[atomic_ordinal] == atomic_ordinal
    });
    if args.no_potions {
        inputs.retain(|input| {
            !matches!(
                input,
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            )
        });
    }
    if let Some(known) = &known_witness_action {
        if args.no_potions
            && matches!(
                known,
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            )
        {
            return Err("--no-potions rejects the verified witness root action".to_string());
        }
        if !inputs.contains(known) {
            inputs.push(known.clone());
        }
    }
    if inputs.is_empty() {
        return Err("action-successor audit root has no materialized legal actions".to_string());
    }

    let public_actions =
        project_public_combat_actions_v1(&root_position.engine, &root_position.combat)
            .map_err(|error| format!("cannot project public combat actions: {error}"))?;
    let learning_boundary = LearningCombatBoundaryV1 {
        observation: public_combat_state_v1(&root_position.combat),
        public_run_context: PublicCombatRunContextV1::Unavailable {
            reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
        },
        observation_completeness: LearningObservationCompletenessV1::Complete,
        public_actions: public_actions.public,
        private_resolution: public_actions.private_resolution,
    };
    let potion_policy = if args.no_potions {
        CombatLearningPotionPolicyV1::never()
    } else {
        CombatLearningPotionPolicyV1::All
    };
    let learning_decision = LearningModelDecisionV1::from_combat_boundary_with_potion_policy(
        &learning_boundary,
        &potion_policy,
    )
    .map_err(|error| format!("cannot construct learning candidate surface: {error:?}"))?;
    let learning_ordinals = inputs
        .iter()
        .map(|input| {
            let atomic_ordinal = legal_actions
                .atomic_actions
                .iter()
                .position(|candidate| candidate == input)?;
            let representative_ordinal = atomic_action_representatives[atomic_ordinal];
            let representative_input = legal_actions.atomic_actions.get(representative_ordinal)?;
            learning_decision.combat_atomic_ordinal_for_input(representative_input)
        })
        .collect::<Vec<_>>();

    let policy = args
        .action_imitation_artifact
        .as_deref()
        .map(|path| {
            super::combat_policy_controls::load_action_imitation_policy(
                path,
                retained_combat_tactical_policy_v1(),
            )
        })
        .transpose()?
        .unwrap_or_else(retained_combat_tactical_policy_v1);
    let choices = inputs
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .collect::<Vec<_>>();
    let raw_weights = policy.weights(&root_position, &choices);
    let safe_weights = if raw_weights.len() == inputs.len() {
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
        vec![1.0; inputs.len()]
    };
    let weight_total = safe_weights.iter().sum::<f64>();
    let uniform = 1.0 / inputs.len() as f64;
    let probabilities = safe_weights
        .iter()
        .map(|weight| 0.95 * (*weight / weight_total) + 0.05 * uniform)
        .collect::<Vec<_>>();
    let policy_ranks = safe_weights
        .iter()
        .map(|weight| {
            1 + safe_weights
                .iter()
                .filter(|candidate| **candidate > *weight)
                .count()
        })
        .collect::<Vec<_>>();

    let jobs = args.candidate_jobs.min(inputs.len()).max(1);
    let chunk_len = inputs.len().div_ceil(jobs);
    let batches = std::thread::scope(|scope| {
        let handles = inputs
            .chunks(chunk_len)
            .enumerate()
            .map(|(chunk_index, chunk)| {
                let root_position = &root_position;
                let known_witness_action = &known_witness_action;
                let known_witness_continuation = &known_witness_continuation;
                let known_witness_final_hp = known_witness_final_hp;
                let safe_weights = &safe_weights;
                let probabilities = &probabilities;
                let policy_ranks = &policy_ranks;
                let learning_ordinals = &learning_ordinals;
                let args = &args;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .enumerate()
                        .map(|(offset, input)| {
                            let canonical_index = chunk_index * chunk_len + offset;
                            build_candidate(
                                canonical_index,
                                learning_ordinals[canonical_index],
                                input,
                                root_position,
                                known_witness_action.as_ref(),
                                known_witness_continuation,
                                known_witness_final_hp,
                                safe_weights[canonical_index],
                                probabilities[canonical_index],
                                policy_ranks[canonical_index],
                                args.solve_work_per_candidate,
                                args.v2_teacher_wall_ms_per_candidate,
                                args.v2_teacher_max_nodes_per_candidate,
                                args.max_structured_alternatives,
                                args.max_engine_steps_per_transition,
                                args.no_potions,
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| "action-successor candidate worker panicked".to_string())
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let mut candidates = batches
        .into_iter()
        .flatten()
        .collect::<Result<Vec<_>, String>>()?;
    candidates.sort_by_key(|candidate| candidate.canonical_index);

    let legal_surface = EngineCombatStepper.legal_action_surface(&root_position);
    let mut evidence_counts = std::collections::BTreeMap::<&'static str, usize>::new();
    for candidate in &candidates {
        *evidence_counts
            .entry(candidate.evidence.kind())
            .or_default() += 1;
    }
    let aligned_learning_ordinals = learning_ordinals
        .iter()
        .flatten()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let learning_surface_complete = learning_ordinals.iter().all(Option::is_some)
        && aligned_learning_ordinals.len() == learning_decision.candidates.len();
    let corpus = json!({
        "schema_name": CORPUS_SCHEMA,
        "schema_version": 2,
        "runtime": oracle_lab_runtime_identity(),
        "source_mode": source_mode,
        "source_seed": source_seed,
        "source_root_id": source_root_id,
        "source_case": args.case,
        "source_artifact": args.artifact,
        "source_expected_roots": args.expected_roots,
        "source_root_slot": args.root_slot,
        "source_actions": args.actions,
        "through": args.through,
        "root_exact_state_hash": combat_exact_state_hash_v2(
            &root_position.engine,
            &root_position.combat,
        ),
        "root_position": root_position,
        "known_witness_action": known_witness_action,
        "known_witness_final_hp": known_witness_final_hp,
        "action_imitation_artifact": args.action_imitation_artifact,
        "surface": {
            "materialized_candidates": candidates.len(),
            "atomic_actions": legal_surface.atomic_actions.len(),
            "canonical_atomic_representatives": learning_boundary
                .private_resolution
                .atomic_inputs
                .iter()
                .count(),
            "equivalence_compression": "model_canonical_representatives_v1",
            "structured_family_count": legal_surface.selection_families.len(),
            "max_structured_alternatives": args.max_structured_alternatives,
            "complete": legal_surface.selection_families.is_empty(),
        },
        "learning_surface": {
            "candidate_count": learning_decision.candidates.len(),
            "aligned_candidate_count": learning_ordinals.iter().filter(|ordinal| ordinal.is_some()).count(),
            "one_to_one_candidate_count": aligned_learning_ordinals.len(),
            "complete": learning_surface_complete,
        },
        "config": {
            "solve_work_per_candidate": args.solve_work_per_candidate,
            "candidate_jobs": jobs,
            "no_potions": args.no_potions,
            "v2_teacher_wall_ms_per_candidate": args.v2_teacher_wall_ms_per_candidate,
            "v2_teacher_max_nodes_per_candidate": args.v2_teacher_max_nodes_per_candidate,
            "max_engine_steps_per_transition": args.max_engine_steps_per_transition,
        },
        "evidence_counts": evidence_counts,
        "candidates": candidates,
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
        "schema_name": "ActionSuccessorReanalysisBuildReportV2",
        "schema_version": 2,
        "output": args.output,
        "root_exact_state_hash": corpus["root_exact_state_hash"],
        "known_witness_action": corpus["known_witness_action"],
        "surface": corpus["surface"],
        "learning_surface": corpus["learning_surface"],
        "evidence_counts": corpus["evidence_counts"],
    }))
}

fn load_root_position(
    args: &ActionSuccessorReanalysisArgs,
) -> Result<(u64, CombatPosition, &'static str, Option<String>), String> {
    if let Some(path) = args.case.as_deref() {
        if args.expected_roots.is_some() || args.root_slot != 0 {
            return Err("--expected-roots and nonzero --root-slot require --artifact".to_string());
        }
        let case = load_combat_case(path)?;
        return Ok((
            case.core.source.seed,
            case.core.position,
            "exact_case_root",
            None,
        ));
    }
    let path = args
        .artifact
        .as_deref()
        .ok_or_else(|| "action-successor source is missing".to_string())?;
    let expected_roots = args
        .expected_roots
        .ok_or_else(|| "--artifact requires --expected-roots".to_string())?;
    if expected_roots == 0 {
        return Err("--expected-roots must be positive".to_string());
    }
    if args.root_slot >= expected_roots {
        return Err("--root-slot must be inside --expected-roots".to_string());
    }
    let payload = std::fs::read(path).map_err(|error| error.to_string())?;
    let artifact = CombatLearningRootBatchArtifactV1::decode(
        &payload,
        expected_roots,
        args.max_artifact_bytes,
    )?;
    let source_root_id = artifact
        .roots()
        .get(args.root_slot)
        .ok_or_else(|| "opaque artifact root slot is out of range".to_string())?
        .identity()
        .root_id
        .clone();
    let checkpoint = artifact
        .into_checkpoints()?
        .into_iter()
        .nth(args.root_slot)
        .ok_or_else(|| "opaque artifact root slot is out of range".to_string())?;
    let session = checkpoint.into_session()?;
    let source_seed = session.run_state.seed;
    let position = session.current_active_combat_position()?;
    Ok((
        source_seed,
        position,
        "opaque_combat_root_artifact",
        Some(source_root_id),
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_candidate(
    canonical_index: usize,
    learning_candidate_ordinal: Option<usize>,
    input: &ClientInput,
    root_position: &CombatPosition,
    known_witness_action: Option<&ClientInput>,
    known_witness_continuation: &[ClientInput],
    known_final_hp: Option<i32>,
    raw_policy_weight: f64,
    policy_probability: f64,
    policy_rank: usize,
    solve_work_per_candidate: usize,
    v2_teacher_wall_ms_per_candidate: u64,
    v2_teacher_max_nodes_per_candidate: usize,
    max_structured_alternatives: usize,
    max_engine_steps_per_transition: usize,
    no_potions: bool,
) -> Result<ActionSuccessorCandidate, String> {
    let step = EngineCombatStepper.apply_to_stable(
        root_position,
        input.clone(),
        CombatStepLimits {
            max_engine_steps: max_engine_steps_per_transition,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err(format!(
            "action-successor candidate {canonical_index} did not reach a stable state"
        ));
    }
    let is_known_witness_action = known_witness_action == Some(input);
    let mut evaluation = match step.terminal {
        CombatTerminal::Win if !step.position.combat.runtime.combat_smoked => {
            ExactCombatEvaluation {
                evidence: known_exact_win(
                    "immediate_terminal_replay",
                    step.position.combat.entities.player.current_hp,
                    0,
                ),
                witness_actions: Some(Vec::new()),
            }
        }
        CombatTerminal::Win => ExactCombatEvaluation {
            evidence: exact_terminal_non_win("SmokeEscape"),
            witness_actions: None,
        },
        CombatTerminal::Loss => ExactCombatEvaluation {
            evidence: exact_terminal_non_win("Loss"),
            witness_actions: None,
        },
        CombatTerminal::Unresolved => evaluate_unresolved_position(
            &step.position,
            solve_work_per_candidate,
            max_structured_alternatives,
            max_engine_steps_per_transition,
            no_potions.then_some(0),
        )?,
    };
    if matches!(
        &evaluation.evidence,
        ExactCombatEvidence::BudgetUnknown { .. }
    ) && v2_teacher_wall_ms_per_candidate > 0
    {
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(v2_teacher_wall_ms_per_candidate))
            .ok_or_else(|| "V2 teacher deadline overflowed".to_string())?;
        if let Some(proposal) = oracle_search_witness_proposal_v1(
            &step.position,
            v2_teacher_max_nodes_per_candidate,
            Some(deadline),
        ) {
            let replayed = replay_combat_inputs(
                step.position.clone(),
                &proposal.actions,
                max_engine_steps_per_transition,
            )?;
            if EngineCombatStepper.terminal(&replayed) == CombatTerminal::Win
                && !replayed.combat.runtime.combat_smoked
            {
                evaluation = ExactCombatEvaluation {
                    evidence: known_exact_win(
                        "v2_teacher_exact_replay",
                        replayed.combat.entities.player.current_hp,
                        proposal.actions.len(),
                    ),
                    witness_actions: Some(proposal.actions),
                };
            }
        }
    }
    if let (true, Some(known_final_hp)) = (is_known_witness_action, known_final_hp) {
        evaluation = retain_verified_win_floor(
            evaluation,
            "verified_witness_floor_after_equal_work_search",
            known_final_hp,
            known_witness_continuation.to_vec(),
        );
    }
    let evidence = evaluation.evidence;
    let continuation_witness_actions = evaluation.witness_actions;
    if let Some(continuation) = continuation_witness_actions.as_ref() {
        let complete = std::iter::once(input.clone())
            .chain(continuation.iter().cloned())
            .collect::<Vec<_>>();
        let replayed = replay_combat_inputs(
            root_position.clone(),
            &complete,
            max_engine_steps_per_transition,
        )?;
        if EngineCombatStepper.terminal(&replayed) != CombatTerminal::Win
            || replayed.combat.runtime.combat_smoked
        {
            return Err(format!(
                "action-successor candidate {canonical_index} produced a non-replayable win"
            ));
        }
    }
    Ok(ActionSuccessorCandidate {
        canonical_index,
        learning_candidate_ordinal,
        input: input.clone(),
        label: combat_action_label(root_position, input),
        action_key: combat_action_key(&root_position.combat, input),
        known_witness_action: is_known_witness_action,
        policy_rank,
        raw_policy_weight,
        policy_probability,
        transition_engine_steps: step.engine_steps,
        exact_successor_hash: combat_exact_state_hash_v2(
            &step.position.engine,
            &step.position.combat,
        ),
        evidence,
        continuation_witness_actions,
    })
}
