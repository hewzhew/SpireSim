use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::ai::combat_search_v2::CombatSearchV2Satisfaction;
use crate::ai::combat_search_v2::{
    compare_outcome_metrics, run_combat_search_v2, CombatSearchV2DiagnosticsReport,
    CombatSearchV2FrontierValueReport, CombatSearchV2OutcomeMetrics, CombatSearchV2OutcomeReport,
    CombatSearchV2Report, CombatSearchV2RolloutReport, CombatSearchV2Stats,
    CombatSearchV2TrajectoryReport, SearchCoverageStatus, SearchTerminalLabel,
    WHOLE_COMBAT_OUTCOME_CRITERIA,
};
use crate::eval::artifact::ArtifactTrustLevel;
use crate::eval::combat_baseline_outcome::load_combat_baseline_outcome_v1;
use crate::eval::fingerprint::StateFingerprintV3;
use crate::sim::combat::CombatTerminal;

use super::benchmark_gate::{
    build_combat_search_v2_benchmark_gate_report, CombatSearchV2BenchmarkGateReport,
};
use super::{
    load_combat_search_v2_snapshot, load_combat_search_v2_start, CombatSearchV2LoadedStart,
    CombatSearchV2RunOptions,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSearchV2BenchmarkSpec {
    pub schema_name: String,
    pub schema_version: u32,
    pub name: String,
    #[serde(default = "default_benchmark_min_trust_level")]
    pub min_trust_level: ArtifactTrustLevel,
    pub cases: Vec<CombatSearchV2BenchmarkCaseSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSearchV2BenchmarkCaseSpec {
    pub id: String,
    #[serde(default)]
    pub start_spec: Option<PathBuf>,
    #[serde(default)]
    pub combat_snapshot: Option<PathBuf>,
    #[serde(default)]
    pub expected_fingerprints: Option<CombatSearchV2BenchmarkExpectedFingerprints>,
    #[serde(default)]
    pub baseline: Option<CombatSearchV2BenchmarkBaselineSpec>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSearchV2BenchmarkExpectedFingerprints {
    pub public_observation_hash: String,
    pub legal_input_language_hash: String,
    pub action_enumeration_domain_hash: String,
    #[serde(default)]
    pub exact_state_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSearchV2BaselineOutcomeSpec {
    pub terminal: SearchTerminalLabel,
    pub final_hp: i32,
    pub potions_used: u32,
    pub turns: u32,
    pub cards_played: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum CombatSearchV2BenchmarkBaselineSpec {
    Inline(CombatSearchV2BaselineOutcomeSpec),
    Path(PathBuf),
}

#[derive(Clone)]
pub struct CombatSearchV2LoadedBenchmark {
    pub name: String,
    pub min_trust_level: ArtifactTrustLevel,
    pub cases: Vec<CombatSearchV2LoadedBenchmarkCase>,
}

#[derive(Clone)]
pub struct CombatSearchV2LoadedBenchmarkCase {
    pub id: String,
    pub input: CombatSearchV2LoadedBenchmarkInput,
    pub start: CombatSearchV2LoadedStart,
    pub expected_fingerprints: Option<CombatSearchV2BenchmarkExpectedFingerprints>,
    pub baseline: Option<CombatSearchV2BaselineOutcomeSpec>,
    pub baseline_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatSearchV2LoadedBenchmarkInput {
    pub kind: CombatSearchV2BenchmarkInputKind,
    pub path: PathBuf,
}

impl CombatSearchV2LoadedBenchmarkInput {
    fn new(kind: CombatSearchV2BenchmarkInputKind, path: PathBuf) -> Self {
        Self { kind, path }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2BenchmarkInputKind {
    StartSpec,
    CombatSnapshot,
}

impl CombatSearchV2BenchmarkInputKind {
    fn as_label(self) -> &'static str {
        match self {
            CombatSearchV2BenchmarkInputKind::StartSpec => "start_spec",
            CombatSearchV2BenchmarkInputKind::CombatSnapshot => "combat_snapshot",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2BenchmarkReport {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub benchmark_name: String,
    pub min_trust_level: ArtifactTrustLevel,
    pub case_count: usize,
    pub summary: CombatSearchV2BenchmarkSummary,
    pub gate: CombatSearchV2BenchmarkGateReport,
    pub cases: Vec<CombatSearchV2BenchmarkCaseReport>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2BenchmarkSummary {
    pub complete_candidate_wins: usize,
    pub complete_candidate_escapes: usize,
    pub complete_candidate_losses: usize,
    pub complete_candidate_unresolved: usize,
    pub complete_candidate_missing: usize,
    pub complete_trajectory_found: usize,
    pub coverage_exhaustive: usize,
    pub coverage_accepted_complete_candidate: usize,
    pub coverage_node_budget_limited: usize,
    pub coverage_action_prefix_budget_limited: usize,
    pub coverage_action_surface_incomplete: usize,
    pub coverage_time_budget_limited: usize,
    pub coverage_frontier_open: usize,
    pub baseline_cases: usize,
    pub search_better: usize,
    pub search_tied: usize,
    pub baseline_better: usize,
    pub inconclusive: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2BenchmarkCaseReport {
    pub id: String,
    pub start_label: String,
    pub input_kind: CombatSearchV2BenchmarkInputKind,
    pub input_path: String,
    pub start_spec_path: Option<String>,
    pub combat_snapshot_path: Option<String>,
    pub input_trust_level: Option<ArtifactTrustLevel>,
    pub input_fingerprints: Option<CombatSearchV2InputFingerprintReport>,
    pub outcome: CombatSearchV2OutcomeReport,
    pub best_complete_trajectory: Option<CombatSearchV2TrajectoryReport>,
    pub best_frontier_value: Option<CombatSearchV2FrontierValueReport>,
    pub rollout: CombatSearchV2RolloutReport,
    pub diagnostics: CombatSearchV2DiagnosticsReport,
    pub stats: CombatSearchV2Stats,
    pub baseline: Option<CombatSearchV2BaselineOutcomeSpec>,
    pub baseline_path: Option<String>,
    pub baseline_comparison: Option<CombatSearchV2BaselineComparison>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2InputFingerprintReport {
    pub boundary: crate::eval::fingerprint::DecisionBoundaryFingerprintV3,
    pub public_observation_hash: String,
    pub legal_input_language_hash: String,
    pub action_enumeration_domain_hash: String,
    pub exact_state_hash: String,
    pub stable_outcome_hash: Option<String>,
    pub rng_boundary_status: crate::eval::fingerprint::RngFingerprintStatus,
    pub rng_boundary_stream_count: usize,
    pub rng_boundary_digest: String,
}

impl From<&StateFingerprintV3> for CombatSearchV2InputFingerprintReport {
    fn from(value: &StateFingerprintV3) -> Self {
        Self {
            boundary: value.boundary.clone(),
            public_observation_hash: value.public_observation_hash.clone(),
            legal_input_language_hash: value.legal_input_language_hash.clone(),
            action_enumeration_domain_hash: value.action_enumeration_domain_hash.clone(),
            exact_state_hash: value.exact_state_hash.clone(),
            stable_outcome_hash: value.stable_outcome_hash.clone(),
            rng_boundary_status: value.rng_boundary.status,
            rng_boundary_stream_count: value.rng_boundary.stream_count,
            rng_boundary_digest: value.rng_boundary.digest.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2BaselineComparison {
    pub verdict: CombatSearchV2BaselineVerdict,
    pub basis: &'static str,
    pub reason: Option<&'static str>,
    pub criteria_order: Vec<&'static str>,
    pub search_terminal: Option<SearchTerminalLabel>,
    pub baseline_terminal: SearchTerminalLabel,
    pub search_final_hp: Option<i32>,
    pub baseline_final_hp: i32,
    pub search_potions_used: Option<u32>,
    pub baseline_potions_used: u32,
    pub search_turns: Option<u32>,
    pub baseline_turns: u32,
    pub search_cards_played: Option<u32>,
    pub baseline_cards_played: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2BaselineVerdict {
    SearchBetter,
    SearchTied,
    BaselineBetter,
    Inconclusive,
}

pub fn load_combat_search_v2_benchmark(
    path: &Path,
) -> Result<CombatSearchV2LoadedBenchmark, String> {
    let payload = fs::read_to_string(path).map_err(|err| err.to_string())?;
    validate_combat_search_v2_benchmark_schema_header(&payload)?;
    let spec: CombatSearchV2BenchmarkSpec =
        serde_json::from_str(&payload).map_err(|err| err.to_string())?;
    if spec.schema_name != default_benchmark_schema_name() {
        return Err(format!(
            "unsupported combat search benchmark schema '{}'",
            spec.schema_name
        ));
    }
    if spec.schema_version != default_benchmark_schema_version() {
        return Err(format!(
            "unsupported combat search benchmark schema_version {}",
            spec.schema_version
        ));
    }
    if spec.name.trim().is_empty() {
        return Err("CombatSearchV2BenchmarkSpec requires a non-empty name".to_string());
    }
    if spec.cases.is_empty() {
        return Err("CombatSearchV2BenchmarkSpec requires at least one case".to_string());
    }

    let base_dir = path.parent().unwrap_or_else(|| Path::new(""));
    let mut ids = HashSet::new();
    let mut cases = Vec::with_capacity(spec.cases.len());
    for case in spec.cases {
        if case.id.trim().is_empty() {
            return Err("CombatSearchV2BenchmarkSpec case id cannot be empty".to_string());
        }
        if !ids.insert(case.id.clone()) {
            return Err(format!(
                "duplicate CombatSearchV2 benchmark case id '{}'",
                case.id
            ));
        }
        let (input, mut start) = load_benchmark_case_input(base_dir, &spec.name, &case)?;
        validate_benchmark_case_artifact(&case, &start, spec.min_trust_level)?;
        let (baseline, baseline_path) = load_benchmark_case_baseline(base_dir, &case)?;
        start.label = format!(
            "benchmark:{}:case:{}:{}:{}",
            spec.name,
            case.id,
            input.kind.as_label(),
            input.path.display()
        );
        cases.push(CombatSearchV2LoadedBenchmarkCase {
            id: case.id,
            input,
            start,
            expected_fingerprints: case.expected_fingerprints,
            baseline,
            baseline_path,
        });
    }

    Ok(CombatSearchV2LoadedBenchmark {
        name: spec.name,
        min_trust_level: spec.min_trust_level,
        cases,
    })
}

pub fn validate_combat_search_v2_benchmark_schema_header(payload: &str) -> Result<(), String> {
    #[derive(Deserialize)]
    struct BenchmarkSchemaProbe {
        schema_name: Option<String>,
        schema_version: Option<u32>,
    }

    let probe: BenchmarkSchemaProbe =
        serde_json::from_str(payload).map_err(|err| err.to_string())?;
    let schema_name = probe
        .schema_name
        .ok_or_else(|| "combat search benchmark is missing required schema_name".to_string())?;
    let schema_version = probe
        .schema_version
        .ok_or_else(|| "combat search benchmark is missing required schema_version".to_string())?;
    if schema_name != default_benchmark_schema_name()
        || schema_version != default_benchmark_schema_version()
    {
        return Err(format!(
            "unsupported combat search benchmark schema '{}'/{}; expected '{}'/{}",
            schema_name,
            schema_version,
            default_benchmark_schema_name(),
            default_benchmark_schema_version()
        ));
    }
    Ok(())
}

pub fn run_combat_search_v2_benchmark(
    loaded: &CombatSearchV2LoadedBenchmark,
    options: CombatSearchV2RunOptions,
) -> CombatSearchV2BenchmarkReport {
    let cases = loaded
        .cases
        .iter()
        .map(|case| run_combat_search_v2_benchmark_case(case, options.clone()))
        .collect::<Vec<_>>();
    let summary = summarize_benchmark_cases(&cases);
    let gate = build_combat_search_v2_benchmark_gate_report(&summary, &cases);

    CombatSearchV2BenchmarkReport {
        schema_name: "CombatSearchV2BenchmarkReport",
        schema_version: 4,
        benchmark_name: loaded.name.clone(),
        min_trust_level: loaded.min_trust_level,
        case_count: cases.len(),
        summary,
        gate,
        cases,
    }
}

fn run_combat_search_v2_benchmark_case(
    case: &CombatSearchV2LoadedBenchmarkCase,
    options: CombatSearchV2RunOptions,
) -> CombatSearchV2BenchmarkCaseReport {
    let options = benchmark_case_search_options(case, options);
    let search_report = run_combat_search_v2(
        &case.start.position.engine,
        &case.start.position.combat,
        options.to_search_config_for_position(case.start.label.clone(), &case.start.position),
    );
    let baseline_comparison = case
        .baseline
        .as_ref()
        .map(|baseline| compare_complete_candidate_to_baseline(&search_report, baseline));

    CombatSearchV2BenchmarkCaseReport {
        id: case.id.clone(),
        start_label: case.start.label.clone(),
        input_kind: case.input.kind,
        input_path: case.input.path.display().to_string(),
        start_spec_path: (case.input.kind == CombatSearchV2BenchmarkInputKind::StartSpec)
            .then(|| case.input.path.display().to_string()),
        combat_snapshot_path: (case.input.kind == CombatSearchV2BenchmarkInputKind::CombatSnapshot)
            .then(|| case.input.path.display().to_string()),
        input_trust_level: case.start.artifact_trust_level,
        input_fingerprints: case
            .start
            .fingerprints
            .as_ref()
            .map(CombatSearchV2InputFingerprintReport::from),
        outcome: search_report.outcome.clone(),
        best_complete_trajectory: search_report.best_complete_trajectory.clone(),
        best_frontier_value: search_report.frontier.best_estimated_value.clone(),
        rollout: search_report.rollout.clone(),
        diagnostics: search_report.diagnostics.clone(),
        stats: search_report.stats.clone(),
        baseline: case.baseline.clone(),
        baseline_path: case
            .baseline_path
            .as_ref()
            .map(|path| path.display().to_string()),
        baseline_comparison,
    }
}

fn benchmark_case_search_options(
    case: &CombatSearchV2LoadedBenchmarkCase,
    mut options: CombatSearchV2RunOptions,
) -> CombatSearchV2RunOptions {
    if case.baseline.is_some() {
        options.satisfaction =
            Some(crate::ai::combat_search_v2::CombatSearchV2Satisfaction::BudgetOrExhaustion);
    }
    options
}

fn validate_benchmark_case_artifact(
    case: &CombatSearchV2BenchmarkCaseSpec,
    start: &CombatSearchV2LoadedStart,
    min_trust_level: ArtifactTrustLevel,
) -> Result<(), String> {
    if case.combat_snapshot.is_some() {
        let trust = start
            .artifact_trust_level
            .ok_or_else(|| format!("case '{}' combat_snapshot is missing trust level", case.id))?;
        if !trust.satisfies_minimum(min_trust_level) {
            return Err(format!(
                "case '{}' combat_snapshot trust level {:?} does not satisfy minimum {:?}",
                case.id, trust, min_trust_level
            ));
        }
    }
    if let Some(expected) = case.expected_fingerprints.as_ref() {
        let actual = start.fingerprints.as_ref().ok_or_else(|| {
            format!(
                "case '{}' has expected_fingerprints but input has no artifact fingerprints",
                case.id
            )
        })?;
        if actual.public_observation_hash != expected.public_observation_hash {
            return Err(format!(
                "case '{}' public_observation_hash drift: expected {}, got {}",
                case.id, expected.public_observation_hash, actual.public_observation_hash
            ));
        }
        if actual.legal_input_language_hash != expected.legal_input_language_hash {
            return Err(format!(
                "case '{}' legal_input_language_hash drift: expected {}, got {}",
                case.id, expected.legal_input_language_hash, actual.legal_input_language_hash
            ));
        }
        if actual.action_enumeration_domain_hash != expected.action_enumeration_domain_hash {
            return Err(format!(
                "case '{}' action_enumeration_domain_hash drift: expected {}, got {}",
                case.id,
                expected.action_enumeration_domain_hash,
                actual.action_enumeration_domain_hash
            ));
        }
        if let Some(expected_exact) = expected.exact_state_hash.as_ref() {
            if actual.exact_state_hash != *expected_exact {
                return Err(format!(
                    "case '{}' exact_state_hash drift: expected {}, got {}",
                    case.id, expected_exact, actual.exact_state_hash
                ));
            }
        }
    }
    Ok(())
}

fn summarize_benchmark_cases(
    cases: &[CombatSearchV2BenchmarkCaseReport],
) -> CombatSearchV2BenchmarkSummary {
    let mut summary = CombatSearchV2BenchmarkSummary::default();
    for case in cases {
        if case.outcome.complete_trajectory_found {
            summary.complete_trajectory_found += 1;
        }
        summarize_complete_candidate_terminal(
            &mut summary,
            case.best_complete_trajectory
                .as_ref()
                .map(|trajectory| trajectory.terminal),
        );
        match case.outcome.coverage_status {
            SearchCoverageStatus::Exhaustive => summary.coverage_exhaustive += 1,
            SearchCoverageStatus::AcceptedCompleteCandidate => {
                summary.coverage_accepted_complete_candidate += 1
            }
            SearchCoverageStatus::NodeBudgetLimited => summary.coverage_node_budget_limited += 1,
            SearchCoverageStatus::ActionPrefixBudgetLimited => {
                summary.coverage_action_prefix_budget_limited += 1
            }
            SearchCoverageStatus::ActionSurfaceIncomplete => {
                summary.coverage_action_surface_incomplete += 1
            }
            SearchCoverageStatus::TimeBudgetLimited => summary.coverage_time_budget_limited += 1,
            SearchCoverageStatus::FrontierOpen => summary.coverage_frontier_open += 1,
        }
        if let Some(comparison) = &case.baseline_comparison {
            summary.baseline_cases += 1;
            match comparison.verdict {
                CombatSearchV2BaselineVerdict::SearchBetter => summary.search_better += 1,
                CombatSearchV2BaselineVerdict::SearchTied => summary.search_tied += 1,
                CombatSearchV2BaselineVerdict::BaselineBetter => summary.baseline_better += 1,
                _ => summary.inconclusive += 1,
            }
        }
    }
    summary
}

fn summarize_complete_candidate_terminal(
    summary: &mut CombatSearchV2BenchmarkSummary,
    terminal: Option<SearchTerminalLabel>,
) {
    match terminal {
        Some(SearchTerminalLabel::Win) => summary.complete_candidate_wins += 1,
        Some(SearchTerminalLabel::Escape) => summary.complete_candidate_escapes += 1,
        Some(SearchTerminalLabel::Loss) => summary.complete_candidate_losses += 1,
        Some(SearchTerminalLabel::Unresolved) => summary.complete_candidate_unresolved += 1,
        None => summary.complete_candidate_missing += 1,
    }
}

fn compare_complete_candidate_to_baseline(
    search_report: &CombatSearchV2Report,
    baseline: &CombatSearchV2BaselineOutcomeSpec,
) -> CombatSearchV2BaselineComparison {
    let criteria_order = WHOLE_COMBAT_OUTCOME_CRITERIA.to_vec();
    let Some(search) = search_report.best_complete_trajectory.as_ref() else {
        return inconclusive_baseline_comparison(
            baseline,
            criteria_order,
            "no_search_complete_trajectory",
        );
    };
    if search.terminal == SearchTerminalLabel::Unresolved {
        return inconclusive_baseline_comparison(
            baseline,
            criteria_order,
            "search_complete_candidate_is_unresolved",
        );
    }

    let ordering = compare_outcome_metrics(
        CombatSearchV2OutcomeMetrics::from_trajectory(search),
        CombatSearchV2OutcomeMetrics {
            terminal: baseline.terminal,
            final_hp: baseline.final_hp,
            potions_used: baseline.potions_used,
            turns: baseline.turns,
            cards_played: baseline.cards_played,
        },
    );
    CombatSearchV2BaselineComparison {
        verdict: match ordering {
            std::cmp::Ordering::Greater => CombatSearchV2BaselineVerdict::SearchBetter,
            std::cmp::Ordering::Equal => CombatSearchV2BaselineVerdict::SearchTied,
            std::cmp::Ordering::Less => CombatSearchV2BaselineVerdict::BaselineBetter,
        },
        basis: "complete_candidate_whole_combat_outcome",
        reason: None,
        criteria_order,
        search_terminal: Some(search.terminal),
        baseline_terminal: baseline.terminal,
        search_final_hp: Some(search.final_hp),
        baseline_final_hp: baseline.final_hp,
        search_potions_used: Some(search.potions_used),
        baseline_potions_used: baseline.potions_used,
        search_turns: Some(search.turns),
        baseline_turns: baseline.turns,
        search_cards_played: Some(search.cards_played),
        baseline_cards_played: baseline.cards_played,
    }
}

fn inconclusive_baseline_comparison(
    baseline: &CombatSearchV2BaselineOutcomeSpec,
    criteria_order: Vec<&'static str>,
    reason: &'static str,
) -> CombatSearchV2BaselineComparison {
    CombatSearchV2BaselineComparison {
        verdict: CombatSearchV2BaselineVerdict::Inconclusive,
        basis: "whole_combat_outcome",
        reason: Some(reason),
        criteria_order,
        search_terminal: None,
        baseline_terminal: baseline.terminal,
        search_final_hp: None,
        baseline_final_hp: baseline.final_hp,
        search_potions_used: None,
        baseline_potions_used: baseline.potions_used,
        search_turns: None,
        baseline_turns: baseline.turns,
        search_cards_played: None,
        baseline_cards_played: baseline.cards_played,
    }
}

fn load_benchmark_case_input(
    base_dir: &Path,
    benchmark_name: &str,
    case: &CombatSearchV2BenchmarkCaseSpec,
) -> Result<
    (
        CombatSearchV2LoadedBenchmarkInput,
        CombatSearchV2LoadedStart,
    ),
    String,
> {
    match (case.start_spec.as_ref(), case.combat_snapshot.as_ref()) {
        (Some(start_spec), None) => {
            let path = resolve_manifest_relative_path(base_dir, start_spec);
            let start = load_combat_search_v2_start(&path)
                .map_err(|err| format!("case '{}' start_spec failed: {err}", case.id))?;
            Ok((
                CombatSearchV2LoadedBenchmarkInput::new(
                    CombatSearchV2BenchmarkInputKind::StartSpec,
                    path,
                ),
                start,
            ))
        }
        (None, Some(combat_snapshot)) => {
            let path = resolve_manifest_relative_path(base_dir, combat_snapshot);
            let start = load_combat_search_v2_snapshot(&path)
                .map_err(|err| format!("case '{}' combat_snapshot failed: {err}", case.id))?;
            Ok((
                CombatSearchV2LoadedBenchmarkInput::new(
                    CombatSearchV2BenchmarkInputKind::CombatSnapshot,
                    path,
                ),
                start,
            ))
        }
        (None, None) => Err(format!(
            "benchmark '{benchmark_name}' case '{}' requires exactly one of start_spec or combat_snapshot",
            case.id
        )),
        (Some(_), Some(_)) => Err(format!(
            "benchmark '{benchmark_name}' case '{}' cannot set both start_spec and combat_snapshot",
            case.id
        )),
    }
}

fn load_benchmark_case_baseline(
    base_dir: &Path,
    case: &CombatSearchV2BenchmarkCaseSpec,
) -> Result<(Option<CombatSearchV2BaselineOutcomeSpec>, Option<PathBuf>), String> {
    match case.baseline.as_ref() {
        None => Ok((None, None)),
        Some(CombatSearchV2BenchmarkBaselineSpec::Inline(inline)) => {
            Ok((Some(inline.clone()), None))
        }
        Some(CombatSearchV2BenchmarkBaselineSpec::Path(path)) => {
            let path = resolve_manifest_relative_path(base_dir, path);
            let baseline = load_combat_baseline_outcome_v1(&path)
                .map_err(|err| format!("case '{}' baseline failed: {err}", case.id))?;
            Ok((
                Some(CombatSearchV2BaselineOutcomeSpec {
                    terminal: search_terminal_from_combat_terminal(baseline.terminal()),
                    final_hp: baseline.final_hp,
                    potions_used: baseline.potions_used,
                    turns: baseline.turns,
                    cards_played: baseline.cards_played,
                }),
                Some(path),
            ))
        }
    }
}

fn search_terminal_from_combat_terminal(terminal: CombatTerminal) -> SearchTerminalLabel {
    match terminal {
        CombatTerminal::Win => SearchTerminalLabel::Win,
        CombatTerminal::Loss => SearchTerminalLabel::Loss,
        CombatTerminal::Unresolved => SearchTerminalLabel::Unresolved,
    }
}

fn resolve_manifest_relative_path(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn default_benchmark_schema_name() -> String {
    "CombatSearchV2BenchmarkSuiteV2".to_string()
}

fn default_benchmark_schema_version() -> u32 {
    2
}

fn default_benchmark_min_trust_level() -> ArtifactTrustLevel {
    ArtifactTrustLevel::Restorable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::combat_capture::{capture_combat_position_v3, save_combat_capture_v3};
    use crate::fixtures::combat_start_spec::{compile_combat_start_spec, CombatStartSpec};
    use crate::sim::combat::CombatPosition;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn legacy_benchmark_schema_is_rejected_before_v1_fingerprint_shape() {
        let payload = r#"{
            "schema_name": "CombatSearchV2BenchmarkSuiteV1",
            "schema_version": 1,
            "cases": [{
                "expected_fingerprints": {
                    "legal_candidate_set_hash": "legacy"
                }
            }]
        }"#;

        let error = validate_combat_search_v2_benchmark_schema_header(payload)
            .expect_err("V1 benchmark fingerprints must be rejected before full parsing");

        assert!(error.contains("unsupported combat search benchmark schema"));
        assert!(error.contains("CombatSearchV2BenchmarkSuiteV1"));

        let missing = validate_combat_search_v2_benchmark_schema_header(r#"{"name":"legacy"}"#)
            .expect_err("headerless manifests must not default into V2");
        assert!(missing.contains("missing required schema_name"));
    }

    #[test]
    fn benchmark_loader_accepts_relative_start_spec_only() {
        let dir = unique_temp_dir("combat_search_v2_benchmark_loader");
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let start_path = dir.join("jaw_worm.json");
        let benchmark_path = dir.join("benchmark.json");
        fs::write(&start_path, starter_jaw_worm_start_spec())
            .expect("start spec should be written");
        fs::write(
            &benchmark_path,
            r#"{
                "schema_name": "CombatSearchV2BenchmarkSuiteV2",
                "schema_version": 2,
                "name": "smoke",
                "cases": [
                    {
                        "id": "jaw_worm",
                        "start_spec": "jaw_worm.json",
                        "baseline": {
                            "terminal": "win",
                            "final_hp": 70,
                            "potions_used": 0,
                            "turns": 4,
                            "cards_played": 9
                        }
                    }
                ]
            }"#,
        )
        .expect("benchmark spec should be written");

        let loaded = load_combat_search_v2_benchmark(&benchmark_path)
            .expect("benchmark should load from relative start-spec path");

        assert_eq!(loaded.name, "smoke");
        assert_eq!(loaded.cases.len(), 1);
        assert_eq!(loaded.cases[0].id, "jaw_worm");
        assert_eq!(
            loaded.cases[0].input.kind,
            CombatSearchV2BenchmarkInputKind::StartSpec
        );
        assert!(loaded.cases[0].baseline.is_some());

        let _ = fs::remove_file(start_path);
        let _ = fs::remove_file(benchmark_path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn benchmark_loader_accepts_relative_combat_snapshot_only() {
        let dir = unique_temp_dir("combat_search_v2_snapshot_benchmark_loader");
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let snapshot_path = dir.join("jaw_worm.capture.json");
        let benchmark_path = dir.join("benchmark.json");

        let position = jaw_worm_position();
        let capture = capture_combat_position_v3(Some("jaw_worm".to_string()), &position)
            .expect("stable position should capture");
        save_combat_capture_v3(&snapshot_path, &capture).expect("capture should be written");
        fs::write(
            &benchmark_path,
            r#"{
                "schema_name": "CombatSearchV2BenchmarkSuiteV2",
                "schema_version": 2,
                "name": "smoke",
                "cases": [
                    {
                        "id": "jaw_worm_snapshot",
                        "combat_snapshot": "jaw_worm.capture.json"
                    }
                ]
            }"#,
        )
        .expect("benchmark spec should be written");

        let loaded = load_combat_search_v2_benchmark(&benchmark_path)
            .expect("benchmark should load from relative combat-snapshot path");

        assert_eq!(loaded.name, "smoke");
        assert_eq!(loaded.cases.len(), 1);
        assert_eq!(loaded.cases[0].id, "jaw_worm_snapshot");
        assert_eq!(
            loaded.cases[0].input.kind,
            CombatSearchV2BenchmarkInputKind::CombatSnapshot
        );
        assert_eq!(loaded.cases[0].start.position, position);
        assert_eq!(
            loaded.cases[0].start.artifact_trust_level,
            Some(ArtifactTrustLevel::Restorable)
        );
        assert!(loaded.cases[0].start.fingerprints.is_some());

        let _ = fs::remove_file(snapshot_path);
        let _ = fs::remove_file(benchmark_path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn benchmark_loader_accepts_expected_snapshot_fingerprints() {
        let dir = unique_temp_dir("combat_search_v2_expected_fingerprint_loader");
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let snapshot_path = dir.join("jaw_worm.capture.json");
        let benchmark_path = dir.join("benchmark.json");

        let capture =
            capture_combat_position_v3(Some("jaw_worm".to_string()), &jaw_worm_position())
                .expect("stable position should capture");
        let fingerprints = capture.fingerprints.clone();
        save_combat_capture_v3(&snapshot_path, &capture).expect("capture should be written");
        fs::write(
            &benchmark_path,
            format!(
                r#"{{
                    "schema_name": "CombatSearchV2BenchmarkSuiteV2",
                    "schema_version": 2,
                    "name": "smoke",
                    "min_trust_level": "restorable",
                    "cases": [
                        {{
                            "id": "jaw_worm_snapshot",
                            "combat_snapshot": "jaw_worm.capture.json",
                            "expected_fingerprints": {{
                                "public_observation_hash": "{}",
                                "legal_input_language_hash": "{}",
                                "action_enumeration_domain_hash": "{}",
                                "exact_state_hash": "{}"
                            }}
                        }}
                    ]
                }}"#,
                fingerprints.public_observation_hash,
                fingerprints.legal_input_language_hash,
                fingerprints.action_enumeration_domain_hash,
                fingerprints.exact_state_hash
            ),
        )
        .expect("benchmark spec should be written");

        let loaded = load_combat_search_v2_benchmark(&benchmark_path)
            .expect("benchmark should accept matching fingerprints");

        assert!(loaded.cases[0].expected_fingerprints.is_some());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_loader_rejects_expected_snapshot_fingerprint_drift() {
        let dir = unique_temp_dir("combat_search_v2_expected_fingerprint_drift");
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let snapshot_path = dir.join("jaw_worm.capture.json");
        let benchmark_path = dir.join("benchmark.json");

        let capture =
            capture_combat_position_v3(Some("jaw_worm".to_string()), &jaw_worm_position())
                .expect("stable position should capture");
        let fingerprints = capture.fingerprints.clone();
        save_combat_capture_v3(&snapshot_path, &capture).expect("capture should be written");
        fs::write(
            &benchmark_path,
            format!(
                r#"{{
                    "schema_name": "CombatSearchV2BenchmarkSuiteV2",
                    "schema_version": 2,
                    "name": "smoke",
                    "cases": [
                        {{
                            "id": "jaw_worm_snapshot",
                            "combat_snapshot": "jaw_worm.capture.json",
                            "expected_fingerprints": {{
                                "public_observation_hash": "0000000000000000000000000000000000000000000000000000000000000000",
                                "legal_input_language_hash": "{}",
                                "action_enumeration_domain_hash": "{}"
                            }}
                        }}
                    ]
                }}"#,
                fingerprints.legal_input_language_hash,
                fingerprints.action_enumeration_domain_hash
            ),
        )
        .expect("benchmark spec should be written");

        let err = match load_combat_search_v2_benchmark(&benchmark_path) {
            Ok(_) => panic!("benchmark should reject fingerprint drift"),
            Err(err) => err,
        };

        assert!(err.contains("public_observation_hash drift"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_loader_accepts_relative_baseline_path() {
        let dir = unique_temp_dir("combat_search_v2_baseline_path_loader");
        fs::create_dir_all(dir.join("captures")).expect("capture dir should be created");
        fs::create_dir_all(dir.join("baselines")).expect("baseline dir should be created");
        let snapshot_path = dir.join("captures").join("jaw.capture.json");
        let baseline_path = dir.join("baselines").join("jaw.baseline.json");
        let benchmark_path = dir.join("benchmark.json");

        let position = jaw_worm_position();
        let capture = capture_combat_position_v3(Some("jaw".to_string()), &position)
            .expect("stable position should capture");
        save_combat_capture_v3(&snapshot_path, &capture).expect("capture should be written");
        fs::write(
            &baseline_path,
            r#"{
                "schema_name": "CombatBaselineOutcomeV1",
                "schema_version": 1,
                "case_id": "jaw",
                "terminal": "win",
                "start_hp": 80,
                "final_hp": 70,
                "hp_loss": 10,
                "turns": 4,
                "potions_used": 0,
                "potions_discarded": 0,
                "cards_played": 9
            }"#,
        )
        .expect("baseline should be written");
        fs::write(
            &benchmark_path,
            r#"{
                "schema_name": "CombatSearchV2BenchmarkSuiteV2",
                "schema_version": 2,
                "name": "baseline_path",
                "cases": [
                    {
                        "id": "jaw",
                        "combat_snapshot": "captures/jaw.capture.json",
                        "baseline": "baselines/jaw.baseline.json"
                    }
                ]
            }"#,
        )
        .expect("benchmark should be written");

        let loaded = load_combat_search_v2_benchmark(&benchmark_path)
            .expect("benchmark should load baseline path");

        assert_eq!(loaded.cases[0].baseline.as_ref().unwrap().final_hp, 70);
        assert_eq!(
            loaded.cases[0].baseline_path.as_ref().unwrap(),
            &baseline_path
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_summary_counts_complete_candidate_outcomes_without_strict_outcome_counts() {
        let mut summary = CombatSearchV2BenchmarkSummary::default();

        summarize_complete_candidate_terminal(&mut summary, Some(SearchTerminalLabel::Win));

        assert_eq!(summary.complete_candidate_wins, 1);
        assert_eq!(summary.complete_candidate_losses, 0);
        assert_eq!(summary.complete_candidate_unresolved, 0);
        assert_eq!(summary.complete_candidate_missing, 0);
    }

    #[test]
    fn benchmark_summary_counts_missing_complete_candidate_as_unresolved_candidate() {
        let mut summary = CombatSearchV2BenchmarkSummary::default();

        summarize_complete_candidate_terminal(&mut summary, None);

        assert_eq!(summary.complete_candidate_missing, 1);
    }

    #[test]
    fn benchmark_case_with_baseline_disables_hp_loss_early_acceptance() {
        let case = loaded_case_with_baseline(Some(CombatSearchV2BaselineOutcomeSpec {
            terminal: SearchTerminalLabel::Win,
            final_hp: 70,
            potions_used: 0,
            turns: 4,
            cards_played: 9,
        }));
        let options = CombatSearchV2RunOptions {
            satisfaction: Some(CombatSearchV2Satisfaction::HpLossAtMost(8)),
            ..CombatSearchV2RunOptions::default()
        };

        let guarded = benchmark_case_search_options(&case, options);

        assert_eq!(
            guarded.satisfaction,
            Some(CombatSearchV2Satisfaction::BudgetOrExhaustion)
        );
    }

    #[test]
    fn benchmark_case_without_baseline_keeps_hp_loss_early_acceptance() {
        let case = loaded_case_with_baseline(None);
        let options = CombatSearchV2RunOptions {
            satisfaction: Some(CombatSearchV2Satisfaction::HpLossAtMost(8)),
            ..CombatSearchV2RunOptions::default()
        };

        let guarded = benchmark_case_search_options(&case, options);

        assert_eq!(
            guarded.satisfaction,
            Some(CombatSearchV2Satisfaction::HpLossAtMost(8))
        );
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{label}_{}_{}", std::process::id(), nanos))
    }

    fn starter_jaw_worm_start_spec() -> &'static str {
        r#"{
            "name": "jaw_worm_starter",
            "player_class": "Ironclad",
            "ascension_level": 0,
            "encounter_id": "JawWorm",
            "room_type": "monster",
            "seed": 1,
            "player_current_hp": 80,
            "player_max_hp": 80,
            "master_deck": [
                {"id": "Strike_R", "count": 5},
                {"id": "Defend_R", "count": 4},
                "Bash"
            ]
        }"#
    }

    fn jaw_worm_position() -> CombatPosition {
        let spec: CombatStartSpec = serde_json::from_str(starter_jaw_worm_start_spec())
            .expect("test start spec should parse");
        let (engine, combat) =
            compile_combat_start_spec(&spec).expect("test start spec should compile");
        CombatPosition::new(engine, combat)
    }

    fn loaded_case_with_baseline(
        baseline: Option<CombatSearchV2BaselineOutcomeSpec>,
    ) -> CombatSearchV2LoadedBenchmarkCase {
        CombatSearchV2LoadedBenchmarkCase {
            id: "jaw".to_string(),
            input: CombatSearchV2LoadedBenchmarkInput::new(
                CombatSearchV2BenchmarkInputKind::StartSpec,
                PathBuf::from("jaw.json"),
            ),
            start: CombatSearchV2LoadedStart {
                label: "jaw".to_string(),
                position: jaw_worm_position(),
                artifact_trust_level: None,
                fingerprints: None,
            },
            expected_fingerprints: None,
            baseline,
            baseline_path: None,
        }
    }
}
