use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::content::monsters::factory::EncounterId;
use crate::eval::campfire_evaluation::{build_campfire_evaluation_batch, CampfireEvaluationSpec};
use crate::eval::source_identity::current_source_identity;
use crate::state::run::RunState;
use crate::testing::fixtures::combat_start_spec::{
    compile_run_state_from_combat_start_spec, CombatStartSpec,
};

use super::{
    campfire_threat_panel_cell_key_v1, compile_campfire_threat_panel_sample_v1,
    execute_campfire_threat_panel_cell_v1, resolve_campfire_threat_panel_spec_v1,
    summarize_campfire_threat_panel_v1, CampfireThreatPanelArtifactStoreV1,
    CampfireThreatPanelExecutionReuseV1, CampfireThreatPanelManifestV1, CampfireThreatPanelSpecV1,
    CampfireThreatPanelSummaryV1, ResolvedCampfireThreatPanelSpecV1,
    CAMPFIRE_THREAT_PANEL_SCHEMA_VERSION,
};

pub const CAMPFIRE_THREAT_PANEL_EXPERIMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireThreatPanelExperimentV1 {
    pub schema_version: u32,
    /// Reuses the maintained card/relic/potion fixture schema. Its encounter
    /// fields are ignored because the panel supplies an explicit public pool.
    pub root_fixture: CombatStartSpec,
    pub root_public_state: CampfireThreatPanelRootPublicStateV1,
    pub evaluation_spec: CampfireEvaluationSpec,
    pub panel_spec: CampfireThreatPanelSpecV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireThreatPanelRootPublicStateV1 {
    pub act_num: u8,
    pub floor_num: i32,
    pub gold: i32,
    #[serde(default)]
    pub shop_purge_count: i32,
    #[serde(default)]
    pub keys: [bool; 3],
    #[serde(default)]
    pub is_final_act_available: bool,
    #[serde(default)]
    pub card_upgraded_chance: f32,
    #[serde(default)]
    pub boss_key: Option<EncounterId>,
}

pub struct CampfireThreatPanelRunRequestV1 {
    pub experiment_spec_path: PathBuf,
    pub output_dir: PathBuf,
    pub requested_samples: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CampfireThreatPanelRunReportV1 {
    pub output_dir: PathBuf,
    pub requested_samples: u64,
    pub cells_present: usize,
    pub cells_appended: usize,
    pub halted_on_replay_error: bool,
    pub summary: CampfireThreatPanelSummaryV1,
}

pub fn run_campfire_threat_panel_v1(
    request: &CampfireThreatPanelRunRequestV1,
) -> Result<CampfireThreatPanelRunReportV1, String> {
    if request.requested_samples == 0 {
        return Err("Campfire threat panel requested sample target must be nonzero".to_string());
    }
    let output_dir = resolve_panel_output_dir_v1(&request.output_dir)?;
    let experiment = load_experiment(&request.experiment_spec_path)?;
    let root = compile_root(&experiment)?;
    let evaluation = build_campfire_evaluation_batch(&root, experiment.evaluation_spec.clone())
        .map_err(|error| format!("failed to evaluate Campfire candidates: {error:?}"))?;
    let resolved = resolve_campfire_threat_panel_spec_v1(experiment.panel_spec)?;
    let sample_zero = compile_campfire_threat_panel_sample_v1(&root, &evaluation, &resolved, 0)?;
    let mut subjects = Vec::new();
    for subject in sample_zero.cells.iter().map(|(_, cell)| cell.subject) {
        if !subjects.contains(&subject) {
            subjects.push(subject);
        }
    }
    if subjects.is_empty() {
        return Err("Campfire threat panel compiled no aligned subjects".to_string());
    }

    let created_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_millis()
        .try_into()
        .map_err(|_| "Campfire threat panel timestamp exceeds u64".to_string())?;
    let manifest = CampfireThreatPanelManifestV1::new(
        resolved.clone(),
        evaluation.context.clone(),
        subjects,
        sample_zero.gaps.clone(),
        current_source_identity(),
        created_at_unix_ms,
    );
    let mut store = CampfireThreatPanelArtifactStoreV1::create_or_resume(&output_dir, manifest)?;
    let context_fingerprint = evaluation.context.context_fingerprint.as_str();
    let mut cells_appended = 0_usize;
    let mut halted = store.cells().iter().any(|cell| cell.error.is_some());
    let mut exact_state_results = HashMap::new();
    for cell in store.cells().iter().filter(|cell| cell.error.is_none()) {
        exact_state_results
            .entry(cell.state_fingerprint.exact_state_hash.clone())
            .or_insert_with(|| cell.clone());
    }

    for sample_index in 0..request.requested_samples {
        if halted {
            break;
        }
        let sample = if sample_index == 0 {
            sample_zero.clone()
        } else {
            compile_campfire_threat_panel_sample_v1(&root, &evaluation, &resolved, sample_index)?
        };
        for (encounter, cell) in &sample.cells {
            let key = expected_cell_key(
                &resolved,
                context_fingerprint,
                encounter,
                sample_index,
                cell,
            );
            if store.contains_cell(&key) {
                continue;
            }
            let exact_state_hash = &cell.state_fingerprint.exact_state_hash;
            let record = if let Some(source) = exact_state_results.get(exact_state_hash) {
                reuse_identical_state_result(source, &key, encounter, sample_index, cell)?
            } else {
                execute_campfire_threat_panel_cell_v1(
                    &resolved,
                    context_fingerprint,
                    encounter,
                    sample_index,
                    cell,
                )
            };
            if record.cell_key != key {
                return Err(format!(
                    "Campfire threat executor returned cell key '{}'; expected '{key}'",
                    record.cell_key
                ));
            }
            let halt_after_flush = record.error.is_some();
            store.append_cell(&record)?;
            cells_appended += 1;
            if !halt_after_flush {
                exact_state_results
                    .entry(exact_state_hash.clone())
                    .or_insert_with(|| record.clone());
            }
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
    let summary = summarize_campfire_threat_panel_v1(
        &resolved.contract_hash,
        &bounded_cells,
        request.requested_samples,
    )?;
    store.write_summary(&summary)?;

    Ok(CampfireThreatPanelRunReportV1 {
        output_dir,
        requested_samples: request.requested_samples,
        cells_present: store.cells().len(),
        cells_appended,
        halted_on_replay_error: halted,
        summary,
    })
}

pub(super) fn reuse_identical_state_result(
    source: &super::CampfireThreatPanelCellV1,
    expected_key: &str,
    encounter: &super::CampfireThreatEncounterV1,
    sample_index: u64,
    cell: &crate::eval::campfire_survival_scenarios::CampfireSurvivalScenarioCell,
) -> Result<super::CampfireThreatPanelCellV1, String> {
    if source.state_fingerprint != cell.state_fingerprint {
        return Err(format!(
            "exact-state hash collision between '{}' and pending cell '{expected_key}'",
            source.cell_key
        ));
    }
    let mut reused = source.clone();
    reused.cell_key = expected_key.to_string();
    reused.subject = cell.subject;
    reused.lens = cell.lens;
    reused.encounter = encounter.clone();
    reused.sample_index = sample_index;
    reused.analysis_seed = cell.analysis_seed;
    reused.shuffle_seed = cell.shuffle_seed;
    reused.state_fingerprint = cell.state_fingerprint.clone();
    reused.start_hp = cell.start.combat.entities.player.current_hp;
    reused.execution_reuse = Some(CampfireThreatPanelExecutionReuseV1::IdenticalExactState {
        source_cell_key: source.cell_key.clone(),
    });
    Ok(reused)
}

fn load_experiment(path: &Path) -> Result<CampfireThreatPanelExperimentV1, String> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        format!(
            "failed to canonicalize Campfire threat panel spec '{}': {error}",
            path.display()
        )
    })?;
    let experiment: CampfireThreatPanelExperimentV1 = serde_json::from_slice(
        &fs::read(&canonical)
            .map_err(|error| format!("failed to read '{}': {error}", canonical.display()))?,
    )
    .map_err(|error| format!("failed to parse '{}': {error}", canonical.display()))?;
    if experiment.schema_version != CAMPFIRE_THREAT_PANEL_EXPERIMENT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported Campfire threat experiment schema_version {}; expected {}",
            experiment.schema_version, CAMPFIRE_THREAT_PANEL_EXPERIMENT_SCHEMA_VERSION
        ));
    }
    if experiment.panel_spec.schema_version != CAMPFIRE_THREAT_PANEL_SCHEMA_VERSION {
        return Err("Campfire threat experiment embeds an unsupported panel schema".to_string());
    }
    Ok(experiment)
}

fn compile_root(experiment: &CampfireThreatPanelExperimentV1) -> Result<RunState, String> {
    let mut root = compile_run_state_from_combat_start_spec(
        &experiment.root_fixture,
        experiment.root_fixture.seed,
    )?;
    let public = &experiment.root_public_state;
    root.act_num = public.act_num;
    root.floor_num = public.floor_num;
    root.gold = public.gold;
    root.shop_purge_count = public.shop_purge_count;
    root.keys = public.keys;
    root.is_final_act_available = public.is_final_act_available;
    root.card_upgraded_chance = public.card_upgraded_chance;
    root.boss_key = public.boss_key;
    Ok(root)
}

fn expected_cell_key(
    resolved: &ResolvedCampfireThreatPanelSpecV1,
    context_fingerprint: &str,
    encounter: &super::CampfireThreatEncounterV1,
    sample_index: u64,
    cell: &crate::eval::campfire_survival_scenarios::CampfireSurvivalScenarioCell,
) -> String {
    campfire_threat_panel_cell_key_v1(
        &resolved.contract_hash,
        context_fingerprint,
        cell.subject,
        cell.lens,
        encounter,
        sample_index,
        cell.analysis_seed,
        cell.shuffle_seed,
        &resolved.spec.profile.id,
    )
}

fn resolve_panel_output_dir_v1(output_dir: &Path) -> Result<PathBuf, String> {
    let repository_root = fs::canonicalize(crate::eval::repository_root())
        .map_err(|error| format!("failed to canonicalize repository root: {error}"))?;
    let artifact_root = fs::canonicalize(repository_root.join("artifacts").join("runs"))
        .map_err(|error| format!("failed to canonicalize artifact root: {error}"))?;
    let candidate = if output_dir.is_absolute() {
        output_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(output_dir)
    };
    let mut ancestor = candidate.as_path();
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| {
            format!(
                "panel output '{}' has no existing ancestor",
                output_dir.display()
            )
        })?;
    }
    let remainder = candidate
        .strip_prefix(ancestor)
        .map_err(|error| format!("failed to resolve panel output: {error}"))?;
    if remainder.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("Campfire threat panel output contains an unsafe path component".to_string());
    }
    let resolved = fs::canonicalize(ancestor)
        .map_err(|error| format!("failed to canonicalize output ancestor: {error}"))?
        .join(remainder);
    if resolved == artifact_root || !resolved.starts_with(&artifact_root) {
        return Err(format!(
            "Campfire threat panel output '{}' must be a descendant of '{}'",
            output_dir.display(),
            artifact_root.display()
        ));
    }
    Ok(resolved)
}
