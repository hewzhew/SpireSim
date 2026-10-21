//! Offline complete-turn successor evidence for search-guidance experiments.
//!
//! A verified witness supplies one known winning successor at each selected
//! turn boundary. Other generated successors are evaluated independently by
//! the exact local-turn graph search. Bounded misses remain `BudgetUnknown`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sts_combat_planner::{
    CombatDecisionRoot, CombatPlanningQuantum, CompleteTurnOption, CompleteTurnOptionBoundary,
    TurnOptionGeneratorConfig, TurnOptionGeneratorSession,
};
use sts_oracle_runtime::ai::combat_search_v2::oracle_search_witness_proposal_v1;
use sts_oracle_runtime::eval::combat_action_imitation::typed_combat_feature_components_v1;
use sts_oracle_runtime::eval::combat_state_features::{
    semantic_combat_state_features_v1, CombatStateFeatureV1, COMBAT_STATE_FEATURE_SCHEMA_V1,
};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{CombatPosition, CombatStepper, EngineCombatStepper};
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_replay_tools::replay_combat_inputs;
use super::exact_turn_corridor::load as load_exact_turn_corridor;
use super::{
    exact_combat_evidence::{
        evaluate_nonterminal_position, exact_terminal_non_win, known_exact_win, ExactCombatEvidence,
    },
    source_content_fingerprint,
};

const MANIFEST_SCHEMA_V1: &str = "BoundarySuccessorCorpusManifestV1";
const MANIFEST_SCHEMA_V2: &str = "BoundarySuccessorCorpusManifestV2";
const CORPUS_SCHEMA: &str = "BoundarySuccessorCorpusV2";
const GUIDE_FEATURE_SCHEMA: &str = "combat_action_imitation/typed_combat_feature_components_v1";

#[derive(Debug, Args)]
pub struct BoundarySuccessorCorpusArgs {
    /// Compact manifest naming exact cases, verified action lists, and either
    /// selected player-turn boundaries or whole verified trajectories.
    #[arg(long)]
    manifest: PathBuf,
    /// Destination for the typed evidence corpus.
    #[arg(long)]
    output: PathBuf,
    /// Deterministic work used to expose complete-turn successors.
    #[arg(long, default_value_t = 40_000)]
    generation_work: usize,
    /// Maximum number of successors evaluated per boundary. The verified
    /// witness successor is always retained when it was generated.
    #[arg(long, default_value_t = 12)]
    candidate_limit: usize,
    /// Deterministic candidate coverage. Rank-stratified keeps a policy head,
    /// logarithmically spaced deeper ranks, and the verified successor.
    #[arg(long, value_enum, default_value_t = CandidateSelection::RankStratified)]
    candidate_selection: CandidateSelection,
    /// Deterministic exact-search work for each non-terminal successor.
    #[arg(long, default_value_t = 5_000)]
    solve_work_per_candidate: usize,
    /// Maximum independent successor searches evaluated concurrently.
    #[arg(long, default_value_t = 4)]
    candidate_jobs: usize,
    /// Optional legacy-teacher allowance after the exact successor search
    /// returns BudgetUnknown. Zero disables the teacher. A teacher proposal
    /// becomes ExactWin only after full replay from the candidate succeeds.
    #[arg(long, default_value_t = 0)]
    v2_teacher_wall_ms_per_candidate: u64,
    #[arg(long, default_value_t = 800_000)]
    v2_teacher_max_nodes_per_candidate: usize,
    /// Ignore a matching content-addressed output and recompute all evidence.
    #[arg(long)]
    force_rebuild: bool,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusManifest {
    schema_name: String,
    schema_version: u32,
    entries: Vec<CorpusManifestEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusManifestEntry {
    id: String,
    split: CorpusSplit,
    case: PathBuf,
    actions: Vec<PathBuf>,
    /// One selected outgoing player-turn boundary. Required by manifest V1;
    /// mutually exclusive with `all_boundaries` in V2.
    boundary_rank: Option<usize>,
    /// Expands one verified trajectory into every outgoing player-turn
    /// boundary. Supported by manifest V2 only.
    #[serde(default)]
    all_boundaries: bool,
    /// Optional deterministic exact-search work override for this boundary.
    /// This lets a held-out evaluation boundary receive more evidence work
    /// without over-spending on every training boundary.
    solve_work_per_candidate: Option<usize>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum CorpusSplit {
    Train,
    Eval,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum CandidateSelection {
    PolicyHead,
    RankStratified,
}

#[derive(Clone, Debug, Serialize)]
struct BoundarySuccessorCorpus {
    schema_name: String,
    schema_version: u32,
    source_identity: String,
    input_fingerprint: String,
    guide_feature_schema: String,
    semantic_feature_schema: String,
    manifest: PathBuf,
    config: CorpusConfig,
    groups: Vec<BoundarySuccessorGroup>,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct CorpusConfig {
    generation_work: usize,
    candidate_limit: usize,
    candidate_selection: CandidateSelection,
    solve_work_per_candidate: usize,
    candidate_jobs: usize,
    v2_teacher_wall_ms_per_candidate: u64,
    v2_teacher_max_nodes_per_candidate: usize,
    max_engine_steps_per_transition: usize,
}

#[derive(Clone, Debug, Serialize)]
struct BoundarySuccessorGroup {
    id: String,
    split: CorpusSplit,
    source_case: PathBuf,
    source_actions: Vec<PathBuf>,
    boundary_rank: usize,
    player_turn: u32,
    root_exact_state_hash: String,
    root_features: Vec<i32>,
    root_semantic_features: Vec<CombatStateFeatureV1>,
    verified_successor_exact_state_hash: String,
    verified_successor_generated: bool,
    verified_successor_policy_rank: Option<usize>,
    verified_suffix_action_count: usize,
    solve_work_per_candidate: usize,
    candidate_jobs: usize,
    generation: GenerationEvidence,
    candidates: Vec<BoundarySuccessorCandidate>,
}

#[derive(Clone, Debug, Serialize)]
struct GenerationEvidence {
    status: String,
    generation_work: usize,
    engine_steps: usize,
    completed_options: usize,
    retained_work_items: usize,
    gap_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct BoundarySuccessorCandidate {
    policy_rank: usize,
    exact_successor_hash: String,
    boundary: String,
    action_count: usize,
    actions: Vec<ClientInput>,
    action_labels: Vec<String>,
    negative_log_policy: f64,
    /// Exact Oracle state for offline representation research. Runtime search
    /// never reads this corpus.
    successor_position: CombatPosition,
    successor_features: Vec<i32>,
    successor_semantic_features: Vec<CombatStateFeatureV1>,
    evidence: ExactCombatEvidence,
}

pub fn build(args: BoundarySuccessorCorpusArgs) -> Result<Value, String> {
    if args.candidate_limit == 0 {
        return Err("--candidate-limit must be positive".to_string());
    }
    if args.generation_work == 0 || args.solve_work_per_candidate == 0 {
        return Err("generation and successor solve work must be positive".to_string());
    }
    if args.candidate_jobs == 0 {
        return Err("--candidate-jobs must be positive".to_string());
    }
    if args.v2_teacher_wall_ms_per_candidate > 0 && args.v2_teacher_max_nodes_per_candidate == 0 {
        return Err(
            "--v2-teacher-max-nodes-per-candidate must be positive when the teacher is enabled"
                .to_string(),
        );
    }
    let manifest_path = args
        .manifest
        .canonicalize()
        .map_err(|error| format!("cannot resolve manifest: {error}"))?;
    let manifest = serde_json::from_slice::<CorpusManifest>(
        &std::fs::read(&manifest_path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("invalid boundary-successor manifest: {error}"))?;
    let manifest_supports_all_boundaries =
        match (manifest.schema_name.as_str(), manifest.schema_version) {
            (MANIFEST_SCHEMA_V1, 1) => false,
            (MANIFEST_SCHEMA_V2, 2) => true,
            _ => {
                return Err("unsupported boundary-successor manifest schema".to_string());
            }
        };
    if manifest.entries.is_empty() {
        return Err("boundary-successor manifest has no entries".to_string());
    }
    let base = manifest_path
        .parent()
        .ok_or_else(|| "manifest has no parent directory".to_string())?;
    let config = CorpusConfig {
        generation_work: args.generation_work,
        candidate_limit: args.candidate_limit,
        candidate_selection: args.candidate_selection,
        solve_work_per_candidate: args.solve_work_per_candidate,
        candidate_jobs: args.candidate_jobs,
        v2_teacher_wall_ms_per_candidate: args.v2_teacher_wall_ms_per_candidate,
        v2_teacher_max_nodes_per_candidate: args.v2_teacher_max_nodes_per_candidate,
        max_engine_steps_per_transition: args.max_engine_steps_per_transition,
    };
    let mut input_paths = vec![manifest_path.clone()];
    for entry in &manifest.entries {
        input_paths.push(resolve_relative(base, &entry.case));
        input_paths.extend(
            entry
                .actions
                .iter()
                .map(|path| resolve_relative(base, path)),
        );
    }
    let entries = expand_manifest_entries(
        manifest.entries,
        base,
        manifest_supports_all_boundaries,
        args.max_engine_steps_per_transition,
    )?;
    let source_identity = current_source_identity()?;
    let input_fingerprint = source_content_fingerprint(base, &input_paths)?;
    if !args.force_rebuild && args.output.is_file() {
        let cached = std::fs::read(&args.output)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(cached) = cached {
            let config_value = serde_json::to_value(config).map_err(|error| error.to_string())?;
            if cached.get("schema_name").and_then(Value::as_str) == Some(CORPUS_SCHEMA)
                && cached.get("schema_version").and_then(Value::as_u64) == Some(2)
                && cached.get("source_identity").and_then(Value::as_str)
                    == Some(source_identity.as_str())
                && cached.get("input_fingerprint").and_then(Value::as_str)
                    == Some(input_fingerprint.as_str())
                && cached.get("config") == Some(&config_value)
            {
                return corpus_report(&cached, &args.output, true);
            }
        }
    }
    let mut groups = Vec::with_capacity(entries.len());
    let mut seen_ids = BTreeSet::new();
    for entry in entries {
        if !seen_ids.insert(entry.id.clone()) {
            return Err(format!(
                "duplicate boundary-successor manifest entry id: {}",
                entry.id
            ));
        }
        groups.push(build_group(entry, base, config)?);
    }
    let corpus = BoundarySuccessorCorpus {
        schema_name: CORPUS_SCHEMA.to_string(),
        schema_version: 2,
        source_identity,
        input_fingerprint,
        guide_feature_schema: GUIDE_FEATURE_SCHEMA.to_string(),
        semantic_feature_schema: COMBAT_STATE_FEATURE_SCHEMA_V1.to_string(),
        manifest: manifest_path,
        config,
        groups,
    };
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let corpus = serde_json::to_value(corpus).map_err(|error| error.to_string())?;
    std::fs::write(
        &args.output,
        serde_json::to_vec_pretty(&corpus).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    corpus_report(&corpus, &args.output, false)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundarySelection {
    One(usize),
    All,
}

fn boundary_selection(
    entry: &CorpusManifestEntry,
    supports_all_boundaries: bool,
) -> Result<BoundarySelection, String> {
    match (entry.boundary_rank, entry.all_boundaries) {
        (Some(rank), false) => Ok(BoundarySelection::One(rank)),
        (None, true) if supports_all_boundaries => Ok(BoundarySelection::All),
        (None, true) => Err(format!(
            "entry {} uses all_boundaries but the manifest is not V2",
            entry.id
        )),
        (Some(_), true) => Err(format!(
            "entry {} must choose boundary_rank or all_boundaries, not both",
            entry.id
        )),
        (None, false) => Err(format!(
            "entry {} must choose boundary_rank or all_boundaries",
            entry.id
        )),
    }
}

fn expand_manifest_entries(
    entries: Vec<CorpusManifestEntry>,
    base: &Path,
    supports_all_boundaries: bool,
    max_engine_steps_per_transition: usize,
) -> Result<Vec<CorpusManifestEntry>, String> {
    let mut expanded = Vec::new();
    for mut entry in entries {
        match boundary_selection(&entry, supports_all_boundaries)? {
            BoundarySelection::One(rank) => {
                entry.boundary_rank = Some(rank);
                expanded.push(entry);
            }
            BoundarySelection::All => {
                let case_path = resolve_relative(base, &entry.case);
                let action_paths = entry
                    .actions
                    .iter()
                    .map(|path| resolve_relative(base, path))
                    .collect::<Vec<_>>();
                if action_paths.is_empty() {
                    return Err(format!("entry {} has no verified action list", entry.id));
                }
                let corridor = load_exact_turn_corridor(
                    &case_path,
                    &action_paths,
                    max_engine_steps_per_transition,
                )?;
                for rank in 0..corridor.transition_actions.len() {
                    let player_turn = corridor.positions_by_rank[rank].combat.turn.turn_count;
                    let mut boundary = entry.clone();
                    boundary.id = format!("{}_t{player_turn}", entry.id);
                    boundary.boundary_rank = Some(rank);
                    boundary.all_boundaries = false;
                    expanded.push(boundary);
                }
            }
        }
    }
    Ok(expanded)
}

fn current_source_identity() -> Result<String, String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("cannot locate oracle_lab: {error}"))?;
    let fingerprint_path = executable.with_extension("source-fingerprint");
    let fingerprint = std::fs::read_to_string(&fingerprint_path).map_err(|error| {
        format!(
            "cannot read canonical source identity '{}': {error}",
            fingerprint_path.display()
        )
    })?;
    fingerprint
        .lines()
        .nth(1)
        .filter(|digest| !digest.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "canonical source identity '{}' is malformed",
                fingerprint_path.display()
            )
        })
}

fn corpus_report(corpus: &Value, output: &Path, cache_hit: bool) -> Result<Value, String> {
    let groups = corpus
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| "boundary-successor corpus has no groups array".to_string())?;
    let train_groups = groups
        .iter()
        .filter(|group| group.get("split").and_then(Value::as_str) == Some("train"))
        .count();
    let mut candidates = 0usize;
    let mut evidence_counts = [0usize; 4];
    let mut verified_successors_generated = 0usize;
    for group in groups {
        if group
            .get("verified_successor_generated")
            .and_then(Value::as_bool)
            == Some(true)
        {
            verified_successors_generated = verified_successors_generated.saturating_add(1);
        }
        let group_candidates = group
            .get("candidates")
            .and_then(Value::as_array)
            .ok_or_else(|| "boundary-successor group has no candidates array".to_string())?;
        candidates = candidates.saturating_add(group_candidates.len());
        for candidate in group_candidates {
            match candidate
                .get("evidence")
                .and_then(|evidence| evidence.get("kind"))
                .and_then(Value::as_str)
            {
                Some("exact_win") => evidence_counts[0] += 1,
                Some("exact_refutation") => evidence_counts[1] += 1,
                Some("exact_terminal_non_win") => evidence_counts[2] += 1,
                Some("budget_unknown") => evidence_counts[3] += 1,
                Some(kind) => return Err(format!("unknown successor evidence kind: {kind}")),
                None => return Err("candidate has no successor evidence kind".to_string()),
            }
        }
    }
    Ok(json!({
        "schema_name": "BoundarySuccessorCorpusBuildReportV1",
        "schema_version": 1,
        "cache_hit": cache_hit,
        "output": output,
        "source_identity": corpus.get("source_identity"),
        "input_fingerprint": corpus.get("input_fingerprint"),
        "groups": groups.len(),
        "train_groups": train_groups,
        "eval_groups": groups.len().saturating_sub(train_groups),
        "candidates": candidates,
        "evidence": {
            "exact_win": evidence_counts[0],
            "exact_refutation": evidence_counts[1],
            "exact_terminal_non_win": evidence_counts[2],
            "budget_unknown": evidence_counts[3],
        },
        "verified_successors_generated": verified_successors_generated,
    }))
}

fn build_group(
    entry: CorpusManifestEntry,
    base: &Path,
    config: CorpusConfig,
) -> Result<BoundarySuccessorGroup, String> {
    if entry.id.trim().is_empty() {
        return Err("boundary-successor entry id is empty".to_string());
    }
    let solve_work_per_candidate = entry
        .solve_work_per_candidate
        .unwrap_or(config.solve_work_per_candidate);
    if solve_work_per_candidate == 0 {
        return Err(format!(
            "entry {} solve_work_per_candidate must be positive",
            entry.id
        ));
    }
    let case_path = resolve_relative(base, &entry.case);
    let action_paths = entry
        .actions
        .iter()
        .map(|path| resolve_relative(base, path))
        .collect::<Vec<_>>();
    if action_paths.is_empty() {
        return Err(format!("entry {} has no verified action list", entry.id));
    }
    let corridor = load_exact_turn_corridor(
        &case_path,
        &action_paths,
        config.max_engine_steps_per_transition,
    )?;
    let boundary_rank = entry
        .boundary_rank
        .ok_or_else(|| format!("entry {} did not resolve to one boundary rank", entry.id))?;
    let root_position = corridor
        .positions_by_rank
        .get(boundary_rank)
        .cloned()
        .ok_or_else(|| {
            format!(
                "entry {} boundary rank {} exceeds {} exact turn roots",
                entry.id,
                boundary_rank,
                corridor.positions_by_rank.len()
            )
        })?;
    let verified_turn_actions = corridor
        .transition_actions
        .get(boundary_rank)
        .ok_or_else(|| format!("entry {} has no outgoing verified turn", entry.id))?;
    let verified_successor = replay_combat_inputs(
        root_position.clone(),
        verified_turn_actions,
        config.max_engine_steps_per_transition,
    )?;
    let verified_successor_hash =
        sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
            &verified_successor.engine,
            &verified_successor.combat,
        );
    let verified_suffix_action_count = corridor.transition_actions[boundary_rank..]
        .iter()
        .map(Vec::len)
        .sum();

    let root = CombatDecisionRoot::new(root_position.clone())
        .map_err(|error| format!("entry {} has invalid root: {error:?}", entry.id))?;
    let mut generator = TurnOptionGeneratorSession::with_policy(
        root,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: config.max_engine_steps_per_transition,
            ..TurnOptionGeneratorConfig::default()
        },
        retained_combat_tactical_policy_v1(),
    );
    let report = generator.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(
            config.generation_work,
            config
                .generation_work
                .saturating_mul(config.max_engine_steps_per_transition),
        ),
    );
    let mut ranked = generator.completed_options().iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        left.negative_log_policy()
            .total_cmp(&right.negative_log_policy())
            .then_with(|| {
                left.exact_successor_hash()
                    .cmp(right.exact_successor_hash())
            })
    });
    let verified_policy_rank = ranked
        .iter()
        .position(|option| option.exact_successor_hash() == verified_successor_hash)
        .map(|index| index + 1);
    let selected = selected_candidates(
        &ranked,
        &verified_successor_hash,
        config.candidate_limit,
        config.candidate_selection,
    )
    .into_iter()
    .map(|(policy_rank, option)| (policy_rank, option.clone()))
    .collect::<Vec<_>>();
    let candidates = build_candidates(
        &selected,
        &root_position,
        &verified_successor_hash,
        corridor.terminal_final_hp,
        verified_suffix_action_count,
        config,
        solve_work_per_candidate,
    )?;

    Ok(BoundarySuccessorGroup {
        id: entry.id,
        split: entry.split,
        source_case: case_path,
        source_actions: action_paths,
        boundary_rank,
        player_turn: root_position.combat.turn.turn_count,
        root_exact_state_hash: sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
            &root_position.engine,
            &root_position.combat,
        ),
        root_features: typed_combat_feature_components_v1(&root_position),
        root_semantic_features: semantic_combat_state_features_v1(&root_position),
        verified_successor_exact_state_hash: verified_successor_hash,
        verified_successor_generated: verified_policy_rank.is_some(),
        verified_successor_policy_rank: verified_policy_rank,
        verified_suffix_action_count,
        solve_work_per_candidate,
        candidate_jobs: config.candidate_jobs.min(selected.len().max(1)),
        generation: GenerationEvidence {
            status: format!("{:?}", report.status),
            generation_work: report.after.generation_work,
            engine_steps: report.after.engine_steps,
            completed_options: ranked.len(),
            retained_work_items: report.retained_work_items,
            gap_count: report.gaps.len(),
        },
        candidates,
    })
}

fn build_candidates(
    selected: &[(usize, CompleteTurnOption)],
    root_position: &CombatPosition,
    verified_successor_hash: &str,
    verified_final_hp: i32,
    verified_suffix_action_count: usize,
    config: CorpusConfig,
    solve_work_per_candidate: usize,
) -> Result<Vec<BoundarySuccessorCandidate>, String> {
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let jobs = config.candidate_jobs.min(selected.len()).max(1);
    let chunk_len = selected.len().div_ceil(jobs);
    let batches = std::thread::scope(|scope| {
        let handles = selected
            .chunks(chunk_len)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|(policy_rank, option)| {
                            build_candidate(
                                *policy_rank,
                                option,
                                root_position,
                                verified_successor_hash,
                                verified_final_hp,
                                verified_suffix_action_count,
                                config,
                                solve_work_per_candidate,
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
                    .map_err(|_| "boundary-successor candidate worker panicked".to_string())
            })
            .collect::<Result<Vec<_>, _>>()
    })?;
    batches.into_iter().flatten().collect()
}

fn build_candidate(
    policy_rank: usize,
    option: &CompleteTurnOption,
    root_position: &CombatPosition,
    verified_successor_hash: &str,
    verified_final_hp: i32,
    verified_suffix_action_count: usize,
    config: CorpusConfig,
    solve_work_per_candidate: usize,
) -> Result<BoundarySuccessorCandidate, String> {
    let evidence = if option.exact_successor_hash() == verified_successor_hash {
        known_exact_win(
            "verified_witness_suffix",
            verified_final_hp,
            verified_suffix_action_count,
        )
    } else {
        evaluate_successor(option, config, solve_work_per_candidate)?
    };
    let action_labels = super::combat_trace_view::readable_turn_option_action_labels(
        root_position,
        option.actions(),
        config.max_engine_steps_per_transition,
    )?;
    Ok(BoundarySuccessorCandidate {
        policy_rank,
        exact_successor_hash: option.exact_successor_hash().to_string(),
        boundary: format!("{:?}", option.boundary()),
        action_count: option.actions().len(),
        actions: option
            .actions()
            .iter()
            .map(|action| action.input.clone())
            .collect(),
        action_labels,
        negative_log_policy: option.negative_log_policy(),
        successor_position: option.exact_successor().clone(),
        successor_features: typed_combat_feature_components_v1(option.exact_successor()),
        successor_semantic_features: semantic_combat_state_features_v1(option.exact_successor()),
        evidence,
    })
}

fn selected_candidates<'a>(
    ranked: &[&'a CompleteTurnOption],
    verified_successor_hash: &str,
    limit: usize,
    selection: CandidateSelection,
) -> Vec<(usize, &'a CompleteTurnOption)> {
    let verified_index = ranked
        .iter()
        .enumerate()
        .find(|(_, option)| option.exact_successor_hash() == verified_successor_hash)
        .map(|(index, _)| index);
    selected_candidate_indices(ranked.len(), verified_index, limit, selection)
        .into_iter()
        .map(|index| (index + 1, ranked[index]))
        .collect()
}

fn selected_candidate_indices(
    ranked_len: usize,
    verified_index: Option<usize>,
    limit: usize,
    selection: CandidateSelection,
) -> Vec<usize> {
    if ranked_len == 0 || limit == 0 {
        return Vec::new();
    }
    let target_len = limit.min(ranked_len);
    if matches!(selection, CandidateSelection::PolicyHead) {
        let mut selected = (0..target_len).collect::<BTreeSet<_>>();
        if let Some(verified_index) = verified_index.filter(|index| *index < ranked_len) {
            if !selected.contains(&verified_index) {
                selected.pop_last();
                selected.insert(verified_index);
            }
        }
        return selected.into_iter().collect();
    }

    let reserve_verified = usize::from(verified_index.is_some());
    let coverage_slots = target_len.saturating_sub(reserve_verified);
    let head_slots = coverage_slots.div_ceil(2);
    let tail_slots = coverage_slots.saturating_sub(head_slots);
    let mut selected = (0..head_slots.min(ranked_len)).collect::<BTreeSet<_>>();
    if tail_slots > 0 && ranked_len > head_slots {
        let first_rank = head_slots.saturating_add(1).max(1) as f64;
        let last_rank = ranked_len as f64;
        for slot in 1..=tail_slots {
            let fraction = slot as f64 / tail_slots as f64;
            let rank = (first_rank.ln() + (last_rank.ln() - first_rank.ln()) * fraction)
                .exp()
                .round()
                .clamp(1.0, last_rank) as usize;
            selected.insert(rank.saturating_sub(1));
        }
    }
    if let Some(verified_index) = verified_index.filter(|index| *index < ranked_len) {
        selected.insert(verified_index);
    }
    for index in 0..ranked_len {
        if selected.len() >= target_len {
            break;
        }
        selected.insert(index);
    }
    while selected.len() > target_len {
        let removable = selected
            .iter()
            .rev()
            .copied()
            .find(|index| Some(*index) != verified_index);
        if let Some(index) = removable {
            selected.remove(&index);
        } else {
            break;
        }
    }
    selected.into_iter().collect()
}

fn evaluate_successor(
    option: &CompleteTurnOption,
    config: CorpusConfig,
    solve_work_per_candidate: usize,
) -> Result<ExactCombatEvidence, String> {
    match option.boundary() {
        CompleteTurnOptionBoundary::TerminalWin => {
            return Ok(known_exact_win(
                "immediate_terminal_replay",
                option.exact_successor().combat.entities.player.current_hp,
                0,
            ));
        }
        CompleteTurnOptionBoundary::TerminalLoss | CompleteTurnOptionBoundary::Escape => {
            return Ok(exact_terminal_non_win(format!("{:?}", option.boundary())));
        }
        CompleteTurnOptionBoundary::NextPlayerTurn => {}
    }
    let exact = evaluate_nonterminal_position(
        option.exact_successor(),
        solve_work_per_candidate,
        config.max_engine_steps_per_transition,
        None,
    )?;
    if !matches!(&exact.evidence, ExactCombatEvidence::BudgetUnknown { .. })
        || config.v2_teacher_wall_ms_per_candidate == 0
    {
        return Ok(exact.evidence);
    }
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(
            config.v2_teacher_wall_ms_per_candidate,
        ))
        .ok_or_else(|| "V2 teacher deadline overflowed".to_string())?;
    let Some(proposal) = oracle_search_witness_proposal_v1(
        option.exact_successor(),
        config.v2_teacher_max_nodes_per_candidate,
        Some(deadline),
    ) else {
        return Ok(exact.evidence);
    };
    let action_count = proposal.actions.len();
    let Ok(replayed) = replay_combat_inputs(
        option.exact_successor().clone(),
        &proposal.actions,
        config.max_engine_steps_per_transition,
    ) else {
        return Ok(exact.evidence);
    };
    if EngineCombatStepper.terminal(&replayed)
        != sts_oracle_runtime::sim::combat::CombatTerminal::Win
        || replayed.combat.runtime.combat_smoked
    {
        return Ok(exact.evidence);
    }
    Ok(known_exact_win(
        "v2_teacher_exact_replay",
        replayed.combat.entities.player.current_hp,
        action_count,
    ))
}

fn resolve_relative(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        boundary_selection, selected_candidate_indices, BoundarySelection, CandidateSelection,
        CorpusManifestEntry, CorpusSplit,
    };

    fn manifest_entry(boundary_rank: Option<usize>, all_boundaries: bool) -> CorpusManifestEntry {
        CorpusManifestEntry {
            id: "fight".to_string(),
            split: CorpusSplit::Train,
            case: PathBuf::from("fight.json"),
            actions: vec![PathBuf::from("win.actions.json")],
            boundary_rank,
            all_boundaries,
            solve_work_per_candidate: None,
        }
    }

    #[test]
    fn policy_head_replaces_last_slot_with_deep_verified_successor() {
        assert_eq!(
            selected_candidate_indices(100, Some(42), 4, CandidateSelection::PolicyHead),
            vec![0, 1, 2, 42]
        );
    }

    #[test]
    fn rank_stratified_covers_head_tail_and_verified_successor() {
        let selected =
            selected_candidate_indices(12_000, Some(432), 8, CandidateSelection::RankStratified);
        assert_eq!(selected.len(), 8);
        assert!(selected.starts_with(&[0, 1, 2, 3]));
        assert!(selected.contains(&432));
        assert!(selected.contains(&11_999));
    }

    #[test]
    fn manifest_v2_accepts_one_boundary_or_all_but_not_both() {
        assert_eq!(
            boundary_selection(&manifest_entry(Some(3), false), true),
            Ok(BoundarySelection::One(3))
        );
        assert_eq!(
            boundary_selection(&manifest_entry(None, true), true),
            Ok(BoundarySelection::All)
        );
        assert!(boundary_selection(&manifest_entry(Some(3), true), true).is_err());
        assert!(boundary_selection(&manifest_entry(None, false), true).is_err());
    }

    #[test]
    fn manifest_v1_rejects_all_boundaries_extension() {
        assert!(boundary_selection(&manifest_entry(None, true), false).is_err());
    }
}
