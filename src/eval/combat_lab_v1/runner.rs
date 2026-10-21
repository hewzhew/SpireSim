use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::eval::source_identity::current_source_identity;

use super::{
    combat_lab_cell_key_v1, execute_combat_lab_cell_v1, load_and_resolve_combat_lab_spec_v1,
    preflight_combat_lab_scenario_v1, summarize_combat_lab_v1, CombatLabArtifactStoreV1,
    CombatLabCellErrorStageV1, CombatLabCellErrorV1, CombatLabCellRecordV1,
    CombatLabCompiledSampleV1, CombatLabManifestV1, CombatLabOutcomeClassV1,
    CombatLabScenarioCompilerV1, CombatLabSummaryV1, ResolvedCombatLabProfileV1,
    ResolvedCombatLabSpecV1, COMBAT_LAB_CELL_SCHEMA_VERSION,
};

pub struct CombatLabRunRequestV1 {
    pub lab_spec_path: PathBuf,
    pub output_dir: PathBuf,
    pub requested_samples: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatLabRunReportV1 {
    pub output_dir: PathBuf,
    pub requested_samples: u64,
    pub cells_present: usize,
    pub cells_appended: usize,
    pub summary: CombatLabSummaryV1,
}

pub fn run_combat_lab_v1(request: &CombatLabRunRequestV1) -> Result<CombatLabRunReportV1, String> {
    run_combat_lab_v1_with_executor(request, &ExactSearchCellExecutorV1)
}

pub(super) trait CombatLabCellExecutorV1 {
    fn execute_cell(
        &self,
        resolved: &ResolvedCombatLabSpecV1,
        sample: &CombatLabCompiledSampleV1,
        profile: &ResolvedCombatLabProfileV1,
    ) -> CombatLabCellRecordV1;
}

struct ExactSearchCellExecutorV1;

impl CombatLabCellExecutorV1 for ExactSearchCellExecutorV1 {
    fn execute_cell(
        &self,
        resolved: &ResolvedCombatLabSpecV1,
        sample: &CombatLabCompiledSampleV1,
        profile: &ResolvedCombatLabProfileV1,
    ) -> CombatLabCellRecordV1 {
        execute_combat_lab_cell_v1(resolved, sample, profile)
    }
}

pub(super) fn run_combat_lab_v1_with_executor<E: CombatLabCellExecutorV1>(
    request: &CombatLabRunRequestV1,
    executor: &E,
) -> Result<CombatLabRunReportV1, String> {
    run_combat_lab_v1_with_executor_and_sample_compiler(
        request,
        executor,
        CombatLabScenarioCompilerV1::compile_sample,
    )
}

pub(super) fn run_combat_lab_v1_with_executor_and_sample_compiler<E, F>(
    request: &CombatLabRunRequestV1,
    executor: &E,
    mut compile_sample: F,
) -> Result<CombatLabRunReportV1, String>
where
    E: CombatLabCellExecutorV1,
    F: FnMut(&CombatLabScenarioCompilerV1, u64) -> Result<CombatLabCompiledSampleV1, String>,
{
    if request.requested_samples == 0 {
        return Err("combat laboratory requested sample target must be nonzero".to_string());
    }

    let output_dir = resolve_lab_output_dir_v1(&request.output_dir)?;
    let resolved = load_and_resolve_combat_lab_spec_v1(&request.lab_spec_path)?;
    validate_resolved_contract_v1(&resolved)?;
    let compiler = preflight_combat_lab_scenario_v1(&resolved)?;
    let sample_zero = compile_sample(&compiler, 0)?;
    let source_identity = current_source_identity();
    let created_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_millis()
        .try_into()
        .map_err(|_| "combat laboratory creation timestamp exceeds u64".to_string())?;
    let manifest = CombatLabManifestV1::from_resolved_v1(
        resolved.clone(),
        source_identity,
        created_at_unix_ms,
    );
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output_dir, manifest)?;
    let mut cells_appended = 0_usize;
    let mut halted = store.cells().iter().any(|cell| {
        cell.error
            .as_ref()
            .is_some_and(|error| error.halt_experiment)
    });

    for sample_index in 0..request.requested_samples {
        if halted {
            break;
        }
        let expected_keys = expected_sample_keys(&resolved, sample_index);
        if expected_keys.iter().all(|key| store.contains_cell(key)) {
            continue;
        }
        let sample = if sample_index == 0 {
            sample_zero.clone()
        } else {
            match compile_sample(&compiler, sample_index) {
                Ok(sample) => sample,
                Err(error) => {
                    let (profile, expected_key) = resolved
                        .profiles
                        .iter()
                        .zip(&expected_keys)
                        .find(|(_, key)| !store.contains_cell(key))
                        .expect("sample compilation is needed only when a profile is pending");
                    let cell = sample_construction_error_cell_v1(
                        &resolved,
                        sample_index,
                        profile,
                        expected_key,
                        error,
                    );
                    store.append_cell(&cell)?;
                    cells_appended += 1;
                    break;
                }
            }
        };

        for (profile, expected_key) in resolved.profiles.iter().zip(expected_keys) {
            if store.contains_cell(&expected_key) {
                continue;
            }
            let cell = executor.execute_cell(&resolved, &sample, profile);
            validate_executor_cell_identity(&cell, &expected_key)?;
            let halt_after_flush = cell
                .error
                .as_ref()
                .is_some_and(|error| error.halt_experiment);
            store.append_cell(&cell)?;
            cells_appended += 1;
            if halt_after_flush {
                halted = true;
                break;
            }
        }
        if halted {
            break;
        }
        store.checkpoint_sample_boundary(sample_index + 1)?;
    }

    let bounded_cells = store
        .cells()
        .iter()
        .filter(|cell| cell.sample_index < request.requested_samples)
        .cloned()
        .collect::<Vec<_>>();
    let summary =
        summarize_combat_lab_v1(store.manifest(), &bounded_cells, request.requested_samples)?;
    store.write_summary(&summary)?;

    Ok(CombatLabRunReportV1 {
        output_dir,
        requested_samples: request.requested_samples,
        cells_present: store.cells().len(),
        cells_appended,
        summary,
    })
}

fn sample_construction_error_cell_v1(
    resolved: &ResolvedCombatLabSpecV1,
    sample_index: u64,
    profile: &ResolvedCombatLabProfileV1,
    expected_key: &str,
    message: String,
) -> CombatLabCellRecordV1 {
    CombatLabCellRecordV1 {
        schema_version: COMBAT_LAB_CELL_SCHEMA_VERSION,
        cell_key: expected_key.to_string(),
        experiment_hash: resolved.experiment_hash.clone(),
        sample_index,
        shuffle_seed: super::derive_shuffle_seed_v1(&resolved.schedule, sample_index),
        profile_id: profile.spec.id.clone(),
        profile_hash: profile.profile_hash.clone(),
        budget_hash: resolved.budget_hash.clone(),
        initial_state_fingerprint: None,
        non_shuffle_rng_hash: None,
        shuffle_rng_hash: None,
        search_terminal: None,
        coverage_status: None,
        outcome_class: CombatLabOutcomeClassV1::ExecutionError,
        outcome_order_key: None,
        replayed_candidate: None,
        replay_validated: false,
        start_hp: None,
        final_hp: None,
        hp_loss: None,
        turns: None,
        actions: None,
        cards_played: None,
        potions_used: None,
        draw_history: Vec::new(),
        action_history: Vec::new(),
        expanded_nodes: 0,
        generated_nodes: 0,
        nodes_to_first_win: None,
        node_budget_exhausted: false,
        deadline_exhausted: false,
        error: Some(CombatLabCellErrorV1 {
            stage: CombatLabCellErrorStageV1::SampleConstruction,
            code: "sample_construction_or_isolation_failure".to_string(),
            message: format!("combat laboratory sample construction failed: {message}"),
            halt_experiment: true,
        }),
    }
}

fn expected_sample_keys(resolved: &ResolvedCombatLabSpecV1, sample_index: u64) -> Vec<String> {
    let shuffle_seed = super::derive_shuffle_seed_v1(&resolved.schedule, sample_index);
    resolved
        .profiles
        .iter()
        .map(|profile| {
            combat_lab_cell_key_v1(
                &resolved.experiment_hash,
                sample_index,
                shuffle_seed,
                &profile.spec.id,
                &profile.profile_hash,
                &resolved.budget_hash,
            )
        })
        .collect()
}

fn validate_executor_cell_identity(
    cell: &CombatLabCellRecordV1,
    expected_key: &str,
) -> Result<(), String> {
    if cell.cell_key == expected_key {
        return Ok(());
    }
    Err(format!(
        "combat laboratory executor returned cell key '{}'; expected '{expected_key}'",
        cell.cell_key
    ))
}

fn validate_resolved_contract_v1(resolved: &ResolvedCombatLabSpecV1) -> Result<(), String> {
    if resolved.profiles.is_empty() {
        return Err("combat laboratory requires at least one resolved profile".to_string());
    }
    let mut profile_ids = HashSet::new();
    for profile in &resolved.profiles {
        if profile.spec.id.trim().is_empty() {
            return Err("combat laboratory profile id must not be empty".to_string());
        }
        if !profile_ids.insert(profile.spec.id.as_str()) {
            return Err(format!(
                "combat laboratory duplicate resolved profile id '{}'",
                profile.spec.id
            ));
        }
        if profile.profile_hash.is_empty() {
            return Err(format!(
                "combat laboratory profile '{}' has an empty profile hash",
                profile.spec.id
            ));
        }
    }
    if resolved.budget_hash.is_empty() {
        return Err("combat laboratory common budget hash must not be empty".to_string());
    }
    Ok(())
}

fn resolve_lab_output_dir_v1(output_dir: &Path) -> Result<PathBuf, String> {
    resolve_lab_output_dir_under_repository_v1(crate::eval::repository_root(), output_dir)
}

pub(super) fn resolve_lab_output_dir_under_repository_v1(
    repository_root: &Path,
    output_dir: &Path,
) -> Result<PathBuf, String> {
    let repository_root = fs::canonicalize(repository_root).map_err(|error| {
        format!("failed to canonicalize combat laboratory repository root: {error}")
    })?;
    let artifact_parent_path = repository_root.join("artifacts");
    fs::create_dir_all(&artifact_parent_path).map_err(|error| {
        format!(
            "failed to create combat laboratory artifact parent '{}': {error}",
            artifact_parent_path.display()
        )
    })?;
    let artifact_parent = fs::canonicalize(&artifact_parent_path).map_err(|error| {
        format!(
            "failed to canonicalize combat laboratory artifact parent '{}': {error}",
            artifact_parent_path.display()
        )
    })?;
    if artifact_parent == repository_root || !artifact_parent.starts_with(&repository_root) {
        return Err(format!(
            "combat laboratory artifact parent '{}' resolves outside repository '{}'",
            artifact_parent_path.display(),
            repository_root.display()
        ));
    }

    let artifact_root_path = artifact_parent.join("runs");
    fs::create_dir_all(&artifact_root_path).map_err(|error| {
        format!(
            "failed to create combat laboratory artifact root '{}': {error}",
            artifact_root_path.display()
        )
    })?;
    let artifact_root = fs::canonicalize(&artifact_root_path).map_err(|error| {
        format!(
            "failed to canonicalize combat laboratory artifact root '{}': {error}",
            artifact_root_path.display()
        )
    })?;
    if artifact_root == artifact_parent || !artifact_root.starts_with(&artifact_parent) {
        return Err(format!(
            "combat laboratory artifact root '{}' resolves outside artifact parent '{}'",
            artifact_root_path.display(),
            artifact_parent.display()
        ));
    }
    let candidate = if output_dir.is_absolute() {
        output_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(output_dir)
    };

    let mut existing_ancestor = candidate.as_path();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor.parent().ok_or_else(|| {
            format!(
                "combat laboratory output '{}' has no existing ancestor",
                output_dir.display()
            )
        })?;
    }
    let remainder = candidate.strip_prefix(existing_ancestor).map_err(|error| {
        format!(
            "failed to resolve combat laboratory output '{}': {error}",
            output_dir.display()
        )
    })?;
    if remainder.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!(
            "combat laboratory output '{}' contains an unsafe path component",
            output_dir.display()
        ));
    }
    let canonical_ancestor = fs::canonicalize(existing_ancestor).map_err(|error| {
        format!(
            "failed to canonicalize combat laboratory output ancestor '{}': {error}",
            existing_ancestor.display()
        )
    })?;
    let resolved_output = canonical_ancestor.join(remainder);
    if resolved_output == artifact_root || !resolved_output.starts_with(&artifact_root) {
        return Err(format!(
            "combat laboratory output '{}' must be a descendant of '{}'",
            output_dir.display(),
            artifact_root.display()
        ));
    }
    Ok(resolved_output)
}
