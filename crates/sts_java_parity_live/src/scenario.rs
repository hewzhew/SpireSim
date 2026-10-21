use super::{
    empty_object, publish_json_create_new, read_json, receipt_path, reset_scenario_to_main_menu,
    session_paths_from_manifest, sha256_file, start_run_with_content_profile, status,
    submit_action, unix_ms, ActionHandle, ActionReceipt, LiveStatus, SessionManifest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sts_java_parity_protocol::SCENARIO_CONTROL_PROFILE;
use uuid::Uuid;

mod probe;
mod trajectory;

pub use probe::{
    compile_scenario_probe_case, run_constructed_combat_probe, scenario_probe_case_catalog,
    ConstructedScenarioProbeReport, ScenarioProbeCardSelector, ScenarioProbeCaseCatalog,
    ScenarioProbeCaseCatalogEntry, ScenarioProbeDraft, ScenarioProbeEvidenceScope,
    ScenarioProbeMonsterSelector, ScenarioProbePlan, ScenarioProbeTransition,
};
pub use trajectory::{
    compile_scenario_trajectory_case, run_constructed_combat_trajectory,
    scenario_trajectory_case_catalog, ConstructedScenarioTrajectoryReport,
    ScenarioTrajectoryCaseCatalog, ScenarioTrajectoryCaseCatalogEntry, ScenarioTrajectoryDraft,
    ScenarioTrajectoryEvidenceScope, ScenarioTrajectoryFrameProof, ScenarioTrajectoryPlan,
    ScenarioTrajectoryStepReport,
};

const DRAFT_SCHEMA: &str = "sts_java_play_scenario_draft_v3";
pub const SCENARIO_PLAN_SCHEMA: &str = "sts_java_play_scenario_plan_v3";
pub const SCENARIO_CONTENT_PROFILE_ALL_UNLOCKED: &str = "all_unlocked";
const RECIPE_SCHEMA: &str = "communicationmod.parity-combat-recipe.v2";
const RECIPE_ID: &str = "fresh_neow_to_combat_root.v1";
const LEGACY_CAPABILITY_REGISTRY_V1: &str = "communicationmod.parity-combat-capabilities.v1";
const LEGACY_CAPABILITY_REGISTRY_V2: &str = "communicationmod.parity-combat-capabilities.v2";
const CAPABILITY_REGISTRY: &str = "communicationmod.parity-combat-capabilities.v3";
const ENCOUNTER_CAPABILITY: &str = "combat.encounter.exact.v1";
const HEALTH_CAPABILITY: &str = "player.health.v1";
const DECK_CAPABILITY: &str = "player.master_deck.exact.v1";
const FRESH_RELICS_CAPABILITY: &str = "player.relics.fresh_exact.v1";
const FRESH_POTIONS_CAPABILITY: &str = "player.potions.fresh_exact.v1";
const MAX_SCENARIO_RELICS: usize = 50;
const MAX_SCENARIO_POTION_SLOTS: usize = 5;
const MAX_CONTIGUOUS_FTUES_PER_PHASE: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDraft {
    pub schema: String,
    pub provenance: String,
    pub case_id: String,
    pub run: ScenarioRunStart,
    pub recipe: ScenarioDraftRecipe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDraftRecipe {
    pub recipe_id: String,
    pub capabilities: Vec<ScenarioCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPlan {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub run: ScenarioRunStart,
    pub recipe: CombatScenarioRecipe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioRunStart {
    pub character: String,
    pub ascension: u32,
    pub seed: Option<String>,
    pub content_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CombatScenarioRecipe {
    pub schema: String,
    pub provenance: String,
    pub scenario_id: String,
    pub case_id: String,
    pub recipe_id: String,
    pub capability_registry: String,
    pub capabilities: Vec<ScenarioCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "capability_id", content = "payload", deny_unknown_fields)]
pub enum ScenarioCapability {
    #[serde(rename = "combat.encounter.exact.v1")]
    Encounter(ScenarioEncounterSpec),
    #[serde(rename = "player.health.v1")]
    PlayerHealth(ScenarioPlayerSpec),
    #[serde(rename = "player.master_deck.exact.v1")]
    MasterDeck(ScenarioDeckSpec),
    #[serde(rename = "player.relics.fresh_exact.v1")]
    FreshRelics(ScenarioRelicsSpec),
    #[serde(rename = "player.potions.fresh_exact.v1")]
    FreshPotions(ScenarioPotionsSpec),
}

impl ScenarioCapability {
    fn id(&self) -> &'static str {
        match self {
            Self::Encounter(_) => ENCOUNTER_CAPABILITY,
            Self::PlayerHealth(_) => HEALTH_CAPABILITY,
            Self::MasterDeck(_) => DECK_CAPABILITY,
            Self::FreshRelics(_) => FRESH_RELICS_CAPABILITY,
            Self::FreshPotions(_) => FRESH_POTIONS_CAPABILITY,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioEncounterSpec {
    pub encounter_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPlayerSpec {
    pub current_hp: u32,
    pub max_hp: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDeckSpec {
    pub cards: Vec<ScenarioCardSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioCardSpec {
    pub card_id: String,
    pub upgrades: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioRelicsSpec {
    pub relics: Vec<ScenarioRelicSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioRelicSpec {
    pub relic_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPotionsSpec {
    pub slots: Vec<ScenarioPotionSlotSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPotionSlotSpec {
    pub slot_index: usize,
    pub potion_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioRegistry {
    pub schema: &'static str,
    pub recipes: Vec<ScenarioRecipeDescriptor>,
    pub capabilities: Vec<ScenarioCapabilityDescriptor>,
    pub transitions: Vec<ScenarioTransitionDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioRecipeDescriptor {
    pub recipe_id: &'static str,
    pub anchor: &'static str,
    pub commit: &'static str,
    pub intermediate_boundaries: Vec<ScenarioIntermediateBoundaryDescriptor>,
    pub required_capabilities: Vec<&'static str>,
    pub optional_capabilities: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioIntermediateBoundaryDescriptor {
    pub boundary_kind: &'static str,
    pub allowed_phases: Vec<&'static str>,
    pub max_contiguous_per_phase: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioCapabilityDescriptor {
    pub capability_id: &'static str,
    pub phase: &'static str,
    pub cardinality: &'static str,
    pub requires: Vec<&'static str>,
    pub conflicts: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioTransitionDescriptor {
    pub transition_id: &'static str,
    pub phase: &'static str,
    pub action_kind: &'static str,
    pub parameters: &'static str,
    pub cardinality: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioCaseCatalog {
    pub schema: &'static str,
    pub catalog_directory: String,
    pub cases: Vec<ScenarioCaseCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioCaseCatalogEntry {
    pub case_id: String,
    pub draft_file: String,
    pub draft_sha256: String,
    pub provenance: String,
    pub recipe_id: String,
    pub character: String,
    pub ascension: u32,
    pub seed: Option<String>,
    pub content_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedScenarioReport {
    pub schema: String,
    pub provenance: String,
    pub evidence_scope: ScenarioEvidenceScope,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub failure: Option<String>,
    pub scenario_plan_path: String,
    pub scenario_plan_sha256: String,
    pub expected_launch_plan: String,
    pub session: SessionManifest,
    pub steps: Vec<ScenarioStepProof>,
    pub checks: Vec<ScenarioCheck>,
    pub root: Option<ScenarioRootProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioEvidenceScope {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub constructed_root_verified: bool,
    pub simulator_comparison: bool,
}

impl ScenarioEvidenceScope {
    fn constructed_root(verified: bool) -> Self {
        Self {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            constructed_root_verified: verified,
            simulator_comparison: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioStepProof {
    pub name: String,
    pub request_id: String,
    pub action_id: String,
    pub based_on_frame_id: u64,
    pub successor_frame_id: Option<u64>,
    pub status: String,
    pub settlement_wall_time_ms: Option<u128>,
    pub parameters: Value,
    pub receipt_path: String,
    pub receipt_sha256: String,
    pub successor_frame_path: Option<String>,
    pub successor_frame_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioRootProof {
    pub frame_id: u64,
    pub frame_path: String,
    pub frame_sha256: String,
    pub boundary: String,
    pub scenario_context: Value,
    pub observed_player: Value,
    pub observed_monsters: Value,
    #[serde(default)]
    pub observed_relics: Value,
    #[serde(default)]
    pub observed_potions: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioArtifactIdentity {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioReuseEvidenceScope {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub simulator_comparison: bool,
    pub constructed_roots_verified: bool,
    pub reset_boundary_verified: bool,
    pub fresh_runtime_per_member: bool,
    pub shared_runtime_reuse: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioReuseTimings {
    pub first_root_ms: u128,
    pub reset_ms: u128,
    pub second_root_ms: u128,
    pub total_after_session_ready_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioReuseProbeReport {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub evidence_scope: ScenarioReuseEvidenceScope,
    pub session_id: String,
    pub connection_id: String,
    pub case_id: String,
    pub first_scenario_id: String,
    pub second_scenario_id: String,
    pub first_root_frame_id: u64,
    pub reset_from_frame_id: u64,
    pub reset_successor_frame_id: u64,
    pub second_root_frame_id: u64,
    pub first_report: ScenarioArtifactIdentity,
    pub reset_report: ScenarioArtifactIdentity,
    pub second_report: ScenarioArtifactIdentity,
    pub timings_ms: ScenarioReuseTimings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTransitionReuseEvidenceScope {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub simulator_comparison: bool,
    pub constructed_roots_verified: bool,
    pub original_game_transitions_captured: bool,
    pub reset_boundary_verified: bool,
    pub fresh_runtime_per_member: bool,
    pub shared_runtime_reuse: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTransitionReuseTimings {
    pub first_probe_ms: u128,
    pub reset_ms: u128,
    pub second_probe_ms: u128,
    pub total_after_session_ready_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTransitionReuseProbeReport {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub evidence_scope: ScenarioTransitionReuseEvidenceScope,
    pub session_id: String,
    pub connection_id: String,
    pub probe_case_id: String,
    pub transition_id: String,
    pub first_scenario_id: String,
    pub second_scenario_id: String,
    pub first_root_frame_id: u64,
    pub first_successor_frame_id: u64,
    pub reset_from_frame_id: u64,
    pub reset_successor_frame_id: u64,
    pub second_root_frame_id: u64,
    pub second_successor_frame_id: u64,
    pub first_root_report: ScenarioArtifactIdentity,
    pub first_probe_report: ScenarioArtifactIdentity,
    pub reset_report: ScenarioArtifactIdentity,
    pub second_root_report: ScenarioArtifactIdentity,
    pub second_probe_report: ScenarioArtifactIdentity,
    pub timings_ms: ScenarioTransitionReuseTimings,
}

struct ScenarioContext {
    report: ConstructedScenarioReport,
}

pub fn scenario_registry() -> ScenarioRegistry {
    ScenarioRegistry {
        schema: "sts_java_parity_scenario_registry_v4",
        recipes: vec![ScenarioRecipeDescriptor {
            recipe_id: RECIPE_ID,
            anchor: "fresh_run_initial_neow",
            commit: "single_prepared_combat_root_transition",
            intermediate_boundaries: vec![ScenarioIntermediateBoundaryDescriptor {
                boundary_kind: "ftue",
                allowed_phases: vec!["before_initial_anchor", "after_commit_before_root"],
                max_contiguous_per_phase: MAX_CONTIGUOUS_FTUES_PER_PHASE,
            }],
            required_capabilities: required_capability_ids(),
            optional_capabilities: optional_capability_ids(),
        }],
        capabilities: all_capability_ids()
            .into_iter()
            .map(|capability_id| ScenarioCapabilityDescriptor {
                capability_id,
                phase: "prepare_before_atomic_commit",
                cardinality: if required_capability_ids().contains(&capability_id) {
                    "exactly_one"
                } else {
                    "zero_or_one"
                },
                requires: if capability_id == FRESH_RELICS_CAPABILITY {
                    vec![HEALTH_CAPABILITY, DECK_CAPABILITY]
                } else {
                    Vec::new()
                },
                conflicts: match capability_id {
                    FRESH_RELICS_CAPABILITY => vec![FRESH_POTIONS_CAPABILITY],
                    FRESH_POTIONS_CAPABILITY => vec![FRESH_RELICS_CAPABILITY],
                    _ => Vec::new(),
                },
            })
            .collect(),
        transitions: probe::scenario_transition_descriptors(),
    }
}

pub(super) fn validate_scenario_context_value(value: &Value) -> Result<(), String> {
    let recipe: CombatScenarioRecipe = serde_json::from_value(value.clone())
        .map_err(|error| format!("scenario context is not a typed recipe: {error}"))?;
    validate_combat_scenario_recipe(&recipe)
}

pub(super) fn validate_scenario_parameter_schema_value(value: &Value) -> Result<(), String> {
    let expected = serde_json::json!({
        "schema": RECIPE_SCHEMA,
        "provenance": "constructed_validated",
        "scenario_id_format": "canonical_uuid",
        "case_id_format": "lowercase_ascii_slug_max_96",
        "recipe_id": RECIPE_ID,
        "capability_registry": CAPABILITY_REGISTRY,
        "required_capabilities": required_capability_ids(),
        "optional_capabilities": optional_capability_ids(),
        "phase": "prepare_before_atomic_commit",
        "commit": "single_combat_root_transition"
    });
    if value != &expected {
        return Err(
            "scenario constructor parameter schema disagrees with the typed registry".to_owned(),
        );
    }
    Ok(())
}

pub fn compile_scenario_plan(draft_path: &Path, plan_path: &Path) -> Result<ScenarioPlan, String> {
    let draft = read_and_validate_draft(draft_path)?;
    compile_validated_draft(draft, plan_path)
}

pub fn scenario_case_catalog(catalog_directory: &Path) -> Result<ScenarioCaseCatalog, String> {
    let entries = fs::read_dir(catalog_directory).map_err(|error| {
        format!(
            "failed to read scenario case catalog {}: {error}",
            catalog_directory.display()
        )
    })?;
    let mut draft_paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to enumerate scenario case catalog {}: {error}",
                catalog_directory.display()
            )
        })?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| {
                format!(
                    "failed to inspect scenario case {}: {error}",
                    path.display()
                )
            })?
            .is_file()
            && path.extension().and_then(|value| value.to_str()) == Some("json")
        {
            draft_paths.push(path);
        }
    }
    draft_paths.sort();
    if draft_paths.is_empty() {
        return Err("scenario case catalog contains no JSON drafts".to_owned());
    }

    let mut case_ids = BTreeSet::new();
    let mut cases = Vec::with_capacity(draft_paths.len());
    for draft_path in draft_paths {
        let draft = read_and_validate_draft(&draft_path)?;
        let expected_file = format!("{}.json", draft.case_id);
        let draft_file = draft_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                format!(
                    "scenario case filename is not valid Unicode: {}",
                    draft_path.display()
                )
            })?;
        if draft_file != expected_file {
            return Err(format!(
                "scenario case filename must exactly match embedded case_id: expected {expected_file}, found {draft_file}"
            ));
        }
        if !case_ids.insert(draft.case_id.clone()) {
            return Err(format!(
                "scenario case catalog contains duplicate case_id: {}",
                draft.case_id
            ));
        }
        cases.push(ScenarioCaseCatalogEntry {
            case_id: draft.case_id,
            draft_file: draft_file.to_owned(),
            draft_sha256: sha256_file(&draft_path)?,
            provenance: draft.provenance,
            recipe_id: draft.recipe.recipe_id,
            character: draft.run.character,
            ascension: draft.run.ascension,
            seed: draft.run.seed,
            content_profile: draft.run.content_profile,
        });
    }
    cases.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    Ok(ScenarioCaseCatalog {
        schema: "sts_java_parity_scenario_case_catalog_v1",
        catalog_directory: absolute_string(catalog_directory)?,
        cases,
    })
}

pub fn compile_scenario_case(
    catalog_directory: &Path,
    case_id: &str,
    plan_path: &Path,
) -> Result<ScenarioPlan, String> {
    if !valid_case_id(case_id) {
        return Err("scenario case ID is outside the exact lowercase slug schema".to_owned());
    }
    let catalog = scenario_case_catalog(catalog_directory)?;
    let entry = catalog
        .cases
        .iter()
        .find(|entry| entry.case_id == case_id)
        .ok_or_else(|| format!("scenario case catalog has no exact case_id '{case_id}'"))?;
    let draft_path = catalog_directory.join(&entry.draft_file);
    let draft = read_and_validate_draft(&draft_path)?;
    if draft.case_id != case_id {
        return Err("scenario case identity changed during exact catalog resolution".to_owned());
    }
    compile_validated_draft(draft, plan_path)
}

/// Loads one already-compiled scenario plan through the same strict validator
/// used by the live Java runner.
///
/// Downstream parity owners must not deserialize a plan and silently skip the
/// capability, provenance, or recipe checks merely because the Java process
/// has already accepted it.
pub fn load_scenario_plan(plan_path: &Path) -> Result<ScenarioPlan, String> {
    read_and_validate_plan(plan_path)
}

fn read_and_validate_draft(draft_path: &Path) -> Result<ScenarioDraft, String> {
    let bytes = fs::read(draft_path).map_err(|error| {
        format!(
            "failed to read scenario draft {}: {error}",
            draft_path.display()
        )
    })?;
    let draft: ScenarioDraft = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse scenario draft {}: {error}",
            draft_path.display()
        )
    })?;
    validate_draft(&draft)?;
    Ok(draft)
}

fn compile_validated_draft(
    mut draft: ScenarioDraft,
    plan_path: &Path,
) -> Result<ScenarioPlan, String> {
    canonicalize_capabilities(&mut draft.recipe.capabilities);
    let plan = ScenarioPlan {
        schema: SCENARIO_PLAN_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: unix_ms()?,
        run: draft.run,
        recipe: CombatScenarioRecipe {
            schema: RECIPE_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            scenario_id: Uuid::new_v4().to_string(),
            case_id: draft.case_id,
            recipe_id: RECIPE_ID.to_owned(),
            capability_registry: CAPABILITY_REGISTRY.to_owned(),
            capabilities: draft.recipe.capabilities,
        },
    };
    publish_json_create_new(plan_path, &plan)?;
    Ok(plan)
}

pub fn run_constructed_combat_scenario(
    output_dir: &Path,
    scenario_plan_path: &Path,
    expected_launch_plan: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<ConstructedScenarioReport, String> {
    let plan = read_and_validate_plan(scenario_plan_path)?;
    let initial = status(output_dir)?;
    validate_initial_surface(&initial, expected_launch_plan)?;
    let created_at = unix_ms()?;
    let mut context = ScenarioContext {
        report: ConstructedScenarioReport {
            schema: "sts_java_parity_constructed_scenario_report_v2".to_owned(),
            provenance: "constructed_validated".to_owned(),
            evidence_scope: ScenarioEvidenceScope::constructed_root(false),
            created_at_unix_ms: created_at,
            completed_at_unix_ms: created_at,
            passed: false,
            failure: None,
            scenario_plan_path: absolute_string(scenario_plan_path)?,
            scenario_plan_sha256: sha256_file(scenario_plan_path)?,
            expected_launch_plan: absolute_string(expected_launch_plan)?,
            session: initial.manifest.clone(),
            steps: Vec::new(),
            checks: Vec::new(),
            root: None,
        },
    };

    let outcome = execute_scenario_recipe(output_dir, &plan, timeout, &mut context);
    context.report.completed_at_unix_ms = unix_ms()?;
    match outcome {
        Ok(()) => {
            context.report.passed = context.report.checks.iter().all(|check| check.passed);
            context.report.evidence_scope =
                ScenarioEvidenceScope::constructed_root(context.report.passed);
            if !context.report.passed {
                context.report.failure = Some("one or more scenario invariants failed".to_owned());
            }
        }
        Err(error) => {
            context.report.failure = Some(error);
            context.report.passed = false;
        }
    }
    publish_json_create_new(report_path, &context.report)?;
    if context.report.passed {
        Ok(context.report)
    } else {
        Err(format!(
            "constructed scenario failed; durable report={}",
            absolute_string(report_path)?
        ))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_scenario_reuse_probe(
    output_dir: &Path,
    first_scenario_plan_path: &Path,
    second_scenario_plan_path: &Path,
    expected_launch_plan: &Path,
    first_report_path: &Path,
    reset_report_path: &Path,
    second_report_path: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<ScenarioReuseProbeReport, String> {
    let created_at_unix_ms = unix_ms()?;
    let started = Instant::now();
    let first_plan = read_and_validate_plan(first_scenario_plan_path)?;
    let second_plan = read_and_validate_plan(second_scenario_plan_path)?;
    validate_reuse_plan_pair(&first_plan, &second_plan)?;
    let output_paths = [
        first_report_path,
        reset_report_path,
        second_report_path,
        report_path,
    ];
    validate_create_new_output_paths(&output_paths)?;

    let first = run_constructed_combat_scenario(
        output_dir,
        first_scenario_plan_path,
        expected_launch_plan,
        first_report_path,
        timeout,
    )?;
    let first_completed = started.elapsed().as_millis();
    let reset = reset_scenario_to_main_menu(output_dir, reset_report_path, timeout)?;
    let reset_completed = started.elapsed().as_millis();
    let second = run_constructed_combat_scenario(
        output_dir,
        second_scenario_plan_path,
        expected_launch_plan,
        second_report_path,
        timeout,
    )?;
    let second_completed = started.elapsed().as_millis();

    let first_root = first
        .root
        .as_ref()
        .ok_or_else(|| "first reuse member omitted its verified root".to_owned())?;
    let second_root = second
        .root
        .as_ref()
        .ok_or_else(|| "second reuse member omitted its verified root".to_owned())?;
    let second_start = second
        .steps
        .first()
        .ok_or_else(|| "second reuse member omitted its start_run receipt".to_owned())?;
    if !first.passed
        || !second.passed
        || !reset.passed
        || !reset.fresh_main_menu_verified
        || first.session.session_id != second.session.session_id
        || first.session.connection_id != second.session.connection_id
        || first.session.session_id != reset.session_id
        || first.session.connection_id != reset.connection_id
        || first_root.frame_id != reset.from_frame_id
        || second_start.name != "start_run"
        || second_start.based_on_frame_id != reset.successor_frame_id
        || second_root.frame_id <= reset.successor_frame_id
        || first_root.scenario_context["scenario_id"].as_str()
            != Some(first_plan.recipe.scenario_id.as_str())
        || second_root.scenario_context["scenario_id"].as_str()
            != Some(second_plan.recipe.scenario_id.as_str())
    {
        return Err(
            "shared-runtime reuse probe did not preserve its reset and root identity chain"
                .to_owned(),
        );
    }

    let report = ScenarioReuseProbeReport {
        schema: "sts_java_parity_scenario_reuse_probe_v1".to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms,
        completed_at_unix_ms: unix_ms()?,
        passed: true,
        evidence_scope: ScenarioReuseEvidenceScope {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            simulator_comparison: false,
            constructed_roots_verified: true,
            reset_boundary_verified: true,
            fresh_runtime_per_member: false,
            shared_runtime_reuse: true,
        },
        session_id: first.session.session_id,
        connection_id: first.session.connection_id,
        case_id: first_plan.recipe.case_id,
        first_scenario_id: first_plan.recipe.scenario_id,
        second_scenario_id: second_plan.recipe.scenario_id,
        first_root_frame_id: first_root.frame_id,
        reset_from_frame_id: reset.from_frame_id,
        reset_successor_frame_id: reset.successor_frame_id,
        second_root_frame_id: second_root.frame_id,
        first_report: scenario_artifact_identity(first_report_path)?,
        reset_report: scenario_artifact_identity(reset_report_path)?,
        second_report: scenario_artifact_identity(second_report_path)?,
        timings_ms: ScenarioReuseTimings {
            first_root_ms: first_completed,
            reset_ms: reset_completed - first_completed,
            second_root_ms: second_completed - reset_completed,
            total_after_session_ready_ms: second_completed,
        },
    };
    publish_json_create_new(report_path, &report)?;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
pub fn run_scenario_transition_reuse_probe(
    output_dir: &Path,
    first_probe_plan_path: &Path,
    second_probe_plan_path: &Path,
    expected_launch_plan: &Path,
    first_root_report_path: &Path,
    first_probe_report_path: &Path,
    reset_report_path: &Path,
    second_root_report_path: &Path,
    second_probe_report_path: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<ScenarioTransitionReuseProbeReport, String> {
    let created_at_unix_ms = unix_ms()?;
    let started = Instant::now();
    let first_probe_plan = probe::read_and_validate_probe_plan(first_probe_plan_path)?;
    let second_probe_plan = probe::read_and_validate_probe_plan(second_probe_plan_path)?;
    validate_transition_reuse_probe_pair(&first_probe_plan, &second_probe_plan)?;
    let first_scenario_plan =
        read_and_validate_plan(Path::new(&first_probe_plan.scenario_plan_path))?;
    let second_scenario_plan =
        read_and_validate_plan(Path::new(&second_probe_plan.scenario_plan_path))?;
    validate_reuse_plan_pair(&first_scenario_plan, &second_scenario_plan)?;
    let output_paths = [
        first_root_report_path,
        first_probe_report_path,
        reset_report_path,
        second_root_report_path,
        second_probe_report_path,
        report_path,
    ];
    validate_create_new_output_paths(&output_paths)?;

    let first = run_constructed_combat_probe(
        output_dir,
        first_probe_plan_path,
        expected_launch_plan,
        first_root_report_path,
        first_probe_report_path,
        timeout,
    )?;
    let first_completed = started.elapsed().as_millis();
    let first_successor = first
        .successor
        .as_ref()
        .ok_or_else(|| "first reuse transition omitted its verified successor".to_owned())?;
    let first_action = first
        .action
        .as_ref()
        .ok_or_else(|| "first reuse transition omitted its action receipt".to_owned())?;
    let first_successor_frame_id = first_action
        .successor_frame_id
        .ok_or_else(|| "first reuse transition action omitted its successor".to_owned())?;
    if first_successor.frame_id != first_successor_frame_id {
        return Err("first reuse transition successor identity is inconsistent".to_owned());
    }

    let reset = reset_scenario_to_main_menu(output_dir, reset_report_path, timeout)?;
    let reset_completed = started.elapsed().as_millis();
    let second = run_constructed_combat_probe(
        output_dir,
        second_probe_plan_path,
        expected_launch_plan,
        second_root_report_path,
        second_probe_report_path,
        timeout,
    )?;
    let second_completed = started.elapsed().as_millis();
    let second_successor = second
        .successor
        .as_ref()
        .ok_or_else(|| "second reuse transition omitted its verified successor".to_owned())?;
    let second_action = second
        .action
        .as_ref()
        .ok_or_else(|| "second reuse transition omitted its action receipt".to_owned())?;
    let second_successor_frame_id = second_action
        .successor_frame_id
        .ok_or_else(|| "second reuse transition action omitted its successor".to_owned())?;
    let second_root_report: ConstructedScenarioReport = read_json(second_root_report_path)?;
    let second_start = second_root_report
        .steps
        .first()
        .ok_or_else(|| "second reuse transition omitted its start_run receipt".to_owned())?;

    if !first.passed
        || !second.passed
        || !reset.passed
        || !reset.fresh_main_menu_verified
        || first.session.session_id != second.session.session_id
        || first.session.connection_id != second.session.connection_id
        || first.session.session_id != reset.session_id
        || first.session.connection_id != reset.connection_id
        || first_successor_frame_id != reset.from_frame_id
        || reset.scenario_context != first_successor.scenario_context
        || second_start.name != "start_run"
        || second_start.based_on_frame_id != reset.successor_frame_id
        || second.root_frame_id <= reset.successor_frame_id
        || second_successor.frame_id != second_successor_frame_id
        || first_successor.scenario_context["scenario_id"].as_str()
            != Some(first_scenario_plan.recipe.scenario_id.as_str())
        || second_successor.scenario_context["scenario_id"].as_str()
            != Some(second_scenario_plan.recipe.scenario_id.as_str())
    {
        return Err(
            "shared-runtime transition reuse did not preserve its action, reset, and root chain"
                .to_owned(),
        );
    }

    let report = ScenarioTransitionReuseProbeReport {
        schema: "sts_java_parity_scenario_transition_reuse_probe_v1".to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms,
        completed_at_unix_ms: unix_ms()?,
        passed: true,
        evidence_scope: ScenarioTransitionReuseEvidenceScope {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            simulator_comparison: false,
            constructed_roots_verified: true,
            original_game_transitions_captured: true,
            reset_boundary_verified: true,
            fresh_runtime_per_member: false,
            shared_runtime_reuse: true,
        },
        session_id: first.session.session_id,
        connection_id: first.session.connection_id,
        probe_case_id: first_probe_plan.probe_case_id,
        transition_id: first_probe_plan.transition.id().to_owned(),
        first_scenario_id: first_scenario_plan.recipe.scenario_id,
        second_scenario_id: second_scenario_plan.recipe.scenario_id,
        first_root_frame_id: first.root_frame_id,
        first_successor_frame_id,
        reset_from_frame_id: reset.from_frame_id,
        reset_successor_frame_id: reset.successor_frame_id,
        second_root_frame_id: second.root_frame_id,
        second_successor_frame_id,
        first_root_report: scenario_artifact_identity(first_root_report_path)?,
        first_probe_report: scenario_artifact_identity(first_probe_report_path)?,
        reset_report: scenario_artifact_identity(reset_report_path)?,
        second_root_report: scenario_artifact_identity(second_root_report_path)?,
        second_probe_report: scenario_artifact_identity(second_probe_report_path)?,
        timings_ms: ScenarioTransitionReuseTimings {
            first_probe_ms: first_completed,
            reset_ms: reset_completed - first_completed,
            second_probe_ms: second_completed - reset_completed,
            total_after_session_ready_ms: second_completed,
        },
    };
    publish_json_create_new(report_path, &report)?;
    Ok(report)
}

fn validate_reuse_plan_pair(
    first_plan: &ScenarioPlan,
    second_plan: &ScenarioPlan,
) -> Result<(), String> {
    if first_plan.run != second_plan.run
        || first_plan.recipe.scenario_id == second_plan.recipe.scenario_id
        || first_plan.recipe.case_id != second_plan.recipe.case_id
        || first_plan.recipe.recipe_id != second_plan.recipe.recipe_id
        || first_plan.recipe.capability_registry != second_plan.recipe.capability_registry
        || first_plan.recipe.capabilities != second_plan.recipe.capabilities
    {
        return Err(
            "reuse probe requires two distinct immutable plans for one exact scenario case"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_transition_reuse_probe_pair(
    first: &ScenarioProbePlan,
    second: &ScenarioProbePlan,
) -> Result<(), String> {
    if first.probe_case_id != second.probe_case_id
        || first.scenario_case_id != second.scenario_case_id
        || first.transition != second.transition
        || first.scenario_plan_path == second.scenario_plan_path
        || first.scenario_plan_sha256 == second.scenario_plan_sha256
    {
        return Err(
            "transition reuse requires two independently compiled plans for one exact probe case"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_create_new_output_paths(paths: &[&Path]) -> Result<(), String> {
    let canonical_paths = paths
        .iter()
        .map(|path| {
            let absolute = PathBuf::from(absolute_string(path)?);
            let parent = absolute
                .parent()
                .ok_or_else(|| format!("output path has no parent: {}", absolute.display()))?;
            let file_name = absolute
                .file_name()
                .ok_or_else(|| format!("output path has no file name: {}", absolute.display()))?;
            let canonical_parent = fs::canonicalize(parent).map_err(|error| {
                format!(
                    "failed to resolve output parent {}: {error}",
                    parent.display()
                )
            })?;
            Ok::<PathBuf, String>(canonical_parent.join(file_name))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if canonical_paths.iter().collect::<BTreeSet<_>>().len() != paths.len()
        || canonical_paths.iter().any(|path| path.exists())
    {
        return Err("reuse probe output paths must be distinct create-new files".to_owned());
    }
    Ok(())
}

fn scenario_artifact_identity(path: &Path) -> Result<ScenarioArtifactIdentity, String> {
    Ok(ScenarioArtifactIdentity {
        path: absolute_string(path)?,
        sha256: sha256_file(path)?,
    })
}

fn execute_scenario_recipe(
    output_dir: &Path,
    plan: &ScenarioPlan,
    timeout: Duration,
    context: &mut ScenarioContext,
) -> Result<(), String> {
    let start = start_run_with_content_profile(
        output_dir,
        &plan.run.character,
        plan.run.ascension,
        plan.run.seed.as_deref(),
        &plan.run.content_profile,
        timeout,
    )?;
    context.record_step("start_run", &start)?;
    require_completed_successor(&start, "start_run")?;

    acknowledge_ftues(output_dir, timeout, context, "bootstrap_ftue")?;

    let neow = status(output_dir)?;
    if boundary_kind(&neow) != Some("run_decision")
        || decision_kind(&neow) != Some("event")
        || neow.frame["public_base"]["decision"]["event_id"].as_str() != Some("Neow")
        || neow.frame["public_base"]["run"]["act"].as_i64() != Some(1)
        || neow.frame["public_base"]["run"]["floor"].as_i64() != Some(0)
    {
        return Err("scenario runner did not reach the typed initial Neow anchor".to_owned());
    }
    let action = unique_action(&neow, "scenario_create_combat")?;
    let parameters = serde_json::to_value(&plan.recipe)
        .map_err(|error| format!("failed to encode scenario parameters: {error}"))?;
    let constructor = submit_action(&neow, action, parameters, timeout)?;
    context.record_step("scenario_create_combat", &constructor)?;
    require_completed_successor(&constructor, "scenario_create_combat")?;

    acknowledge_ftues(output_dir, timeout, context, "construction_ftue")?;

    let root = status(output_dir)?;
    let root_frame_id = frame_id(&root)?;
    let expected_context = serde_json::to_value(&plan.recipe)
        .map_err(|error| format!("failed to encode expected scenario context: {error}"))?;
    context.check(
        "recipe_has_contiguous_stable_successor_chain",
        context.report.steps.iter().all(|step| {
            step.status == "completed"
                && step.successor_frame_id == Some(step.based_on_frame_id + 1)
        }) && context
            .report
            .steps
            .windows(2)
            .all(|pair| pair[0].successor_frame_id == Some(pair[1].based_on_frame_id))
            && context
                .report
                .steps
                .last()
                .and_then(|step| step.successor_frame_id)
                == Some(root_frame_id),
        format!(
            "steps={} constructor_successor={:?} latest={root_frame_id}",
            context.report.steps.len(),
            constructor.successor_frame_id
        ),
    );
    context.check(
        "root_is_stable_combat_player_decision",
        boundary_kind(&root) == Some("combat_player_decision")
            && root.frame["action_space"]["kind"].as_str() == Some("combat_action_space")
            && !root.actions.is_empty(),
        format!(
            "boundary={} actions={}",
            boundary_kind(&root).unwrap_or("missing"),
            root.actions.len()
        ),
    );
    context.check(
        "root_provenance_matches_exact_spec",
        root.frame["scenario"] == expected_context,
        format!("scenario_id={}", plan.recipe.scenario_id),
    );
    let player = &root.frame["public_base"]["combat"]["player"];
    let expected_player = plan
        .recipe
        .capabilities
        .iter()
        .find_map(|capability| match capability {
            ScenarioCapability::PlayerHealth(spec) => Some(spec),
            _ => None,
        })
        .ok_or_else(|| "validated scenario recipe omitted player health".to_owned())?;
    context.check(
        "root_player_hp_matches_spec",
        player["current_hp"].as_u64() == Some(u64::from(expected_player.current_hp))
            && player["max_hp"].as_u64() == Some(u64::from(expected_player.max_hp)),
        format!(
            "observed={}/{} expected={}/{}",
            player["current_hp"].as_u64().unwrap_or_default(),
            player["max_hp"].as_u64().unwrap_or_default(),
            expected_player.current_hp,
            expected_player.max_hp
        ),
    );
    let monsters = &root.frame["public_base"]["combat"]["monsters"];
    context.check(
        "root_has_live_monster_group",
        monsters.as_array().is_some_and(|values| {
            !values.is_empty()
                && values
                    .iter()
                    .any(|monster| monster["dead_or_escaped"].as_bool() == Some(false))
        }),
        format!("monster_count={}", monsters.as_array().map_or(0, Vec::len)),
    );
    let relics = &root.frame["parity_internal"]["relics"];
    if let Some(expected_relics) =
        plan.recipe
            .capabilities
            .iter()
            .find_map(|capability| match capability {
                ScenarioCapability::FreshRelics(spec) => Some(spec),
                _ => None,
            })
    {
        let expected_ids = expected_relics
            .relics
            .iter()
            .map(|relic| relic.relic_id.as_str())
            .collect::<Vec<_>>();
        let observed_ids = relics.as_array().map(|values| {
            values
                .iter()
                .filter_map(|relic| relic["relic_id"].as_str())
                .collect::<Vec<_>>()
        });
        context.check(
            "root_relic_ids_match_fresh_exact_spec",
            observed_ids.as_deref() == Some(expected_ids.as_slice())
                && relics.as_array().is_some_and(|values| {
                    values.len() == expected_ids.len()
                        && values.iter().enumerate().all(|(index, relic)| {
                            relic["index"].as_u64() == u64::try_from(index).ok()
                        })
                }),
            format!(
                "observed={:?} expected={expected_ids:?}",
                observed_ids.unwrap_or_default()
            ),
        );
    }
    let potions = &root.frame["public_base"]["combat"]["potions"];
    if let Some(expected_potions) =
        plan.recipe
            .capabilities
            .iter()
            .find_map(|capability| match capability {
                ScenarioCapability::FreshPotions(spec) => Some(spec),
                _ => None,
            })
    {
        let observed_slots = potions.as_array();
        let observed_summary = observed_slots.map(|values| {
            values
                .iter()
                .map(|slot| {
                    format!(
                        "{}:{}:{}:{}:{}",
                        slot["slot_index"].as_i64().unwrap_or(-99),
                        slot["potion_slot"].as_i64().unwrap_or(-99),
                        slot["potion_id"].as_str().unwrap_or("<missing>"),
                        slot["empty"].as_bool().unwrap_or(false),
                        slot["potency"].as_i64().unwrap_or(-99),
                    )
                })
                .collect::<Vec<_>>()
        });
        let slots_match = observed_slots.is_some_and(|values| {
            values.len() == expected_potions.slots.len()
                && values
                    .iter()
                    .zip(&expected_potions.slots)
                    .all(|(observed, expected)| {
                        observed["slot_index"].as_u64() == u64::try_from(expected.slot_index).ok()
                            && observed["potion_slot"].as_i64()
                                == Some(if expected.potion_id == "Potion Slot" {
                                    -1
                                } else {
                                    i64::try_from(expected.slot_index).unwrap_or(-99)
                                })
                            && observed["potion_id"].as_str() == Some(expected.potion_id.as_str())
                            && observed["empty"].as_bool()
                                == Some(expected.potion_id == "Potion Slot")
                            && observed["potency"].as_i64().is_some()
                    })
        });
        context.check(
            "root_potion_slots_match_fresh_exact_spec",
            slots_match,
            format!(
                "observed={:?} expected_slots={}",
                observed_summary.unwrap_or_default(),
                expected_potions.slots.len()
            ),
        );
    }
    let frame_path = PathBuf::from(&root.frame_path);
    context.report.root = Some(ScenarioRootProof {
        frame_id: root_frame_id,
        frame_path: absolute_string(&frame_path)?,
        frame_sha256: sha256_file(&frame_path)?,
        boundary: boundary_kind(&root).unwrap_or("missing").to_owned(),
        scenario_context: root.frame["scenario"].clone(),
        observed_player: player.clone(),
        observed_monsters: monsters.clone(),
        observed_relics: relics.clone(),
        observed_potions: potions.clone(),
    });
    Ok(())
}

fn acknowledge_ftues(
    output_dir: &Path,
    timeout: Duration,
    context: &mut ScenarioContext,
    step_prefix: &str,
) -> Result<(), String> {
    for ftue_index in 0..MAX_CONTIGUOUS_FTUES_PER_PHASE {
        let view = status(output_dir)?;
        if decision_kind(&view) != Some("ftue") {
            return Ok(());
        }
        let action = unique_action(&view, "acknowledge_ftue")?;
        let receipt = submit_action(&view, action, empty_object(), timeout)?;
        context.record_step(&format!("{step_prefix}_{}", ftue_index + 1), &receipt)?;
        require_completed_successor(&receipt, "acknowledge_ftue")?;
    }
    if decision_kind(&status(output_dir)?) == Some("ftue") {
        return Err(format!(
            "scenario recipe exceeded {MAX_CONTIGUOUS_FTUES_PER_PHASE} contiguous FTUE boundaries in phase {step_prefix}"
        ));
    }
    Ok(())
}

fn read_and_validate_plan(path: &Path) -> Result<ScenarioPlan, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read scenario plan {}: {error}", path.display()))?;
    let plan: ScenarioPlan = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse scenario plan {}: {error}", path.display()))?;
    if plan.schema != SCENARIO_PLAN_SCHEMA
        || plan.provenance != "constructed_validated"
        || plan.created_at_unix_ms == 0
        || plan.recipe.provenance != plan.provenance
    {
        return Err("scenario plan identity or provenance is invalid".to_owned());
    }
    validate_run_start(&plan.run)?;
    validate_combat_scenario_recipe(&plan.recipe)?;
    validate_capabilities_for_run(&plan.recipe.capabilities, &plan.run)?;
    Ok(plan)
}

fn validate_draft(draft: &ScenarioDraft) -> Result<(), String> {
    if draft.schema != DRAFT_SCHEMA
        || draft.provenance != "constructed_validated"
        || !valid_case_id(&draft.case_id)
        || draft.recipe.recipe_id != RECIPE_ID
    {
        return Err("scenario draft identity, provenance, or recipe is invalid".to_owned());
    }
    validate_run_start(&draft.run)?;
    validate_capabilities(&draft.recipe.capabilities)?;
    validate_capabilities_for_run(&draft.recipe.capabilities, &draft.run)
}

fn validate_capabilities_for_run(
    capabilities: &[ScenarioCapability],
    run: &ScenarioRunStart,
) -> Result<(), String> {
    let Some(potions) = capabilities.iter().find_map(|capability| match capability {
        ScenarioCapability::FreshPotions(spec) => Some(spec),
        _ => None,
    }) else {
        return Ok(());
    };
    let expected_slots = if run.ascension >= 11 { 2 } else { 3 };
    if potions.slots.len() != expected_slots {
        return Err(format!(
            "fresh exact potion capability must specify all {expected_slots} natural slots at ascension {}",
            run.ascension
        ));
    }
    Ok(())
}

fn validate_run_start(run: &ScenarioRunStart) -> Result<(), String> {
    if !["IRONCLAD", "THE_SILENT", "DEFECT", "WATCHER"].contains(&run.character.as_str())
        || run.ascension > 20
        || run.content_profile != SCENARIO_CONTENT_PROFILE_ALL_UNLOCKED
        || run.seed.as_ref().is_some_and(|seed| {
            seed.is_empty()
                || !seed
                    .chars()
                    .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
        })
    {
        return Err(
            "scenario run start requires a supported character, ascension, seed, and the explicit all_unlocked content profile"
                .to_owned(),
        );
    }
    Ok(())
}

pub fn validate_combat_scenario_recipe(recipe: &CombatScenarioRecipe) -> Result<(), String> {
    if recipe.schema != RECIPE_SCHEMA
        || recipe.provenance != "constructed_validated"
        || recipe.recipe_id != RECIPE_ID
        || ![
            LEGACY_CAPABILITY_REGISTRY_V1,
            LEGACY_CAPABILITY_REGISTRY_V2,
            CAPABILITY_REGISTRY,
        ]
        .contains(&recipe.capability_registry.as_str())
        || !valid_case_id(&recipe.case_id)
        || Uuid::parse_str(&recipe.scenario_id)
            .ok()
            .is_none_or(|uuid| uuid.to_string() != recipe.scenario_id)
    {
        return Err("combat scenario recipe identity is invalid".to_owned());
    }
    validate_capabilities_for_registry(&recipe.capabilities, &recipe.capability_registry)?;
    if !recipe
        .capabilities
        .windows(2)
        .all(|pair| capability_rank(pair[0].id()) < capability_rank(pair[1].id()))
    {
        return Err("scenario plan capabilities are not in canonical registry order".to_owned());
    }
    Ok(())
}

fn validate_capabilities(capabilities: &[ScenarioCapability]) -> Result<(), String> {
    validate_capabilities_for_registry(capabilities, CAPABILITY_REGISTRY)
}

fn validate_capabilities_for_registry(
    capabilities: &[ScenarioCapability],
    registry: &str,
) -> Result<(), String> {
    let mut actual = BTreeSet::new();
    for capability in capabilities {
        if !actual.insert(capability.id()) {
            return Err(format!(
                "scenario capability appears more than once: {}",
                capability.id()
            ));
        }
        match capability {
            ScenarioCapability::Encounter(spec) if spec.encounter_id.is_empty() => {
                return Err("encounter capability requires a non-empty exact ID".to_owned());
            }
            ScenarioCapability::PlayerHealth(spec)
                if spec.max_hp == 0
                    || spec.max_hp > 999
                    || spec.current_hp == 0
                    || spec.current_hp > spec.max_hp =>
            {
                return Err("player health capability is outside its closed bounds".to_owned());
            }
            ScenarioCapability::MasterDeck(spec)
                if spec.cards.is_empty()
                    || spec.cards.len() > 100
                    || spec
                        .cards
                        .iter()
                        .any(|card| card.card_id.is_empty() || card.upgrades > 20) =>
            {
                return Err("master-deck capability is outside its closed bounds".to_owned());
            }
            ScenarioCapability::FreshRelics(spec)
                if spec.relics.len() > MAX_SCENARIO_RELICS
                    || spec.relics.iter().any(|relic| relic.relic_id.is_empty())
                    || spec
                        .relics
                        .iter()
                        .map(|relic| relic.relic_id.as_str())
                        .collect::<BTreeSet<_>>()
                        .len()
                        != spec.relics.len() =>
            {
                return Err(
                    "fresh exact relic capability has invalid or duplicate exact IDs".to_owned(),
                );
            }
            ScenarioCapability::FreshPotions(spec)
                if spec.slots.is_empty()
                    || spec.slots.len() > MAX_SCENARIO_POTION_SLOTS
                    || spec.slots.iter().enumerate().any(|(index, slot)| {
                        slot.slot_index != index || slot.potion_id.is_empty()
                    }) =>
            {
                return Err(
                    "fresh exact potion capability requires complete contiguous typed slots"
                        .to_owned(),
                );
            }
            _ => {}
        }
    }
    let required: BTreeSet<_> = required_capability_ids().into_iter().collect();
    let allowed: BTreeSet<_> = match registry {
        LEGACY_CAPABILITY_REGISTRY_V1 => required.clone(),
        LEGACY_CAPABILITY_REGISTRY_V2 => required
            .iter()
            .copied()
            .chain([FRESH_RELICS_CAPABILITY])
            .collect(),
        CAPABILITY_REGISTRY => all_capability_ids().into_iter().collect(),
        _ => return Err("scenario capability registry is unsupported".to_owned()),
    };
    if !required.is_subset(&actual) || !actual.is_subset(&allowed) {
        return Err(format!(
            "recipe {RECIPE_ID} under {registry} requires {required:?} and allows only {allowed:?}; received {actual:?}"
        ));
    }
    if actual.contains(FRESH_RELICS_CAPABILITY) && actual.contains(FRESH_POTIONS_CAPABILITY) {
        return Err(format!(
            "scenario capabilities conflict: {FRESH_RELICS_CAPABILITY} and {FRESH_POTIONS_CAPABILITY}"
        ));
    }
    Ok(())
}

fn valid_case_id(value: &str) -> bool {
    (1..=96).contains(&value.len())
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
}

fn canonicalize_capabilities(capabilities: &mut [ScenarioCapability]) {
    capabilities.sort_by_key(|capability| capability_rank(capability.id()));
}

fn capability_rank(capability_id: &str) -> usize {
    all_capability_ids()
        .iter()
        .position(|candidate| *candidate == capability_id)
        .unwrap_or(usize::MAX)
}

fn required_capability_ids() -> Vec<&'static str> {
    vec![ENCOUNTER_CAPABILITY, HEALTH_CAPABILITY, DECK_CAPABILITY]
}

fn optional_capability_ids() -> Vec<&'static str> {
    vec![FRESH_RELICS_CAPABILITY, FRESH_POTIONS_CAPABILITY]
}

fn all_capability_ids() -> Vec<&'static str> {
    let mut ids = required_capability_ids();
    ids.extend(optional_capability_ids());
    ids
}

fn validate_initial_surface(view: &LiveStatus, expected_launch_plan: &Path) -> Result<(), String> {
    if boundary_kind(view) != Some("main_menu_ready")
        || view.manifest.profile != SCENARIO_CONTROL_PROFILE
    {
        return Err("constructed scenario requires a fresh scenario-profile main menu".to_owned());
    }
    let launch = view
        .manifest
        .launch
        .as_ref()
        .ok_or_else(|| "constructed scenario requires launch evidence".to_owned())?;
    if launch.control_profile != SCENARIO_CONTROL_PROFILE
        || launch.storage_class != "isolated_working_directory_v1"
        || launch.isolation_id.is_none()
        || &launch.plan_path != expected_launch_plan
    {
        return Err("scenario session does not match its expected isolated launch plan".to_owned());
    }
    Ok(())
}

fn require_completed_successor(receipt: &ActionReceipt, name: &str) -> Result<(), String> {
    if receipt.status != "completed"
        || receipt.successor_frame_id != Some(receipt.based_on_frame_id + 1)
        || receipt.successor_frame_path.is_none()
    {
        return Err(format!(
            "{name} did not produce one completed contiguous successor; status={} code={:?}",
            receipt.status, receipt.code
        ));
    }
    Ok(())
}

fn unique_action<'a>(view: &'a LiveStatus, kind: &str) -> Result<&'a ActionHandle, String> {
    let mut matches = view.actions.iter().filter(|action| action.kind == kind);
    let action = matches
        .next()
        .ok_or_else(|| format!("latest frame has no authorized {kind} action"))?;
    if matches.next().is_some() {
        return Err(format!(
            "latest frame has more than one authorized {kind} action"
        ));
    }
    Ok(action)
}

fn frame_id(view: &LiveStatus) -> Result<u64, String> {
    view.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "scenario frame omitted frame_id".to_owned())
}

fn boundary_kind(view: &LiveStatus) -> Option<&str> {
    view.frame["boundary"]["kind"].as_str()
}

fn decision_kind(view: &LiveStatus) -> Option<&str> {
    view.frame["public_base"]["decision"]["kind"].as_str()
}

impl ScenarioContext {
    fn check(&mut self, name: &str, passed: bool, detail: String) {
        self.report.checks.push(ScenarioCheck {
            name: name.to_owned(),
            passed,
            detail,
        });
    }

    fn record_step(&mut self, name: &str, receipt: &ActionReceipt) -> Result<(), String> {
        self.report
            .steps
            .push(scenario_step_proof(&self.report.session, name, receipt)?);
        Ok(())
    }
}

fn scenario_step_proof(
    session: &SessionManifest,
    name: &str,
    receipt: &ActionReceipt,
) -> Result<ScenarioStepProof, String> {
    let paths = session_paths_from_manifest(session);
    let receipt_path = receipt_path(&paths, &receipt.request_id);
    let successor_frame_path = receipt.successor_frame_path.as_ref().map(PathBuf::from);
    Ok(ScenarioStepProof {
        name: name.to_owned(),
        request_id: receipt.request_id.clone(),
        action_id: receipt.action_id.clone(),
        based_on_frame_id: receipt.based_on_frame_id,
        successor_frame_id: receipt.successor_frame_id,
        status: receipt.status.clone(),
        settlement_wall_time_ms: receipt.settlement_wall_time_ms,
        parameters: receipt.parameters.clone(),
        receipt_path: absolute_string(&receipt_path)?,
        receipt_sha256: sha256_file(&receipt_path)?,
        successor_frame_sha256: successor_frame_path
            .as_ref()
            .map(|path| sha256_file(path))
            .transpose()?,
        successor_frame_path: successor_frame_path
            .as_ref()
            .map(|path| absolute_string(path))
            .transpose()?,
    })
}

fn absolute_string(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(path)
    };
    Ok(absolute.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities() -> Vec<ScenarioCapability> {
        vec![
            ScenarioCapability::MasterDeck(ScenarioDeckSpec {
                cards: vec![ScenarioCardSpec {
                    card_id: "Strike_R".to_owned(),
                    upgrades: 0,
                }],
            }),
            ScenarioCapability::Encounter(ScenarioEncounterSpec {
                encounter_id: "Cultist".to_owned(),
            }),
            ScenarioCapability::PlayerHealth(ScenarioPlayerSpec {
                current_hp: 80,
                max_hp: 80,
            }),
        ]
    }

    fn fresh_potions() -> ScenarioCapability {
        ScenarioCapability::FreshPotions(ScenarioPotionsSpec {
            slots: vec![
                ScenarioPotionSlotSpec {
                    slot_index: 0,
                    potion_id: "Block Potion".to_owned(),
                },
                ScenarioPotionSlotSpec {
                    slot_index: 1,
                    potion_id: "Potion Slot".to_owned(),
                },
            ],
        })
    }

    pub(super) fn draft() -> ScenarioDraft {
        ScenarioDraft {
            schema: DRAFT_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            case_id: "cultist-base-root".to_owned(),
            run: ScenarioRunStart {
                character: "IRONCLAD".to_owned(),
                ascension: 20,
                seed: Some("0".to_owned()),
                content_profile: SCENARIO_CONTENT_PROFILE_ALL_UNLOCKED.to_owned(),
            },
            recipe: ScenarioDraftRecipe {
                recipe_id: RECIPE_ID.to_owned(),
                capabilities: capabilities(),
            },
        }
    }

    #[test]
    fn compiler_canonicalizes_a_closed_capability_recipe() {
        let directory = std::env::temp_dir().join(format!("scenario-plan-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let draft_path = directory.join("draft.json");
        let path = directory.join("plan.json");
        fs::write(&draft_path, serde_json::to_vec(&draft()).unwrap()).unwrap();
        let plan = compile_scenario_plan(&draft_path, &path).unwrap();

        assert_eq!(
            plan.recipe
                .capabilities
                .iter()
                .map(ScenarioCapability::id)
                .collect::<Vec<_>>(),
            required_capability_ids()
        );
        assert_eq!(read_and_validate_plan(&path).unwrap(), plan);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reuse_probe_requires_two_distinct_plans_for_one_exact_case() {
        let directory = std::env::temp_dir().join(format!("scenario-reuse-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let draft_path = directory.join("draft.json");
        let first_path = directory.join("first.json");
        let second_path = directory.join("second.json");
        fs::write(&draft_path, serde_json::to_vec(&draft()).unwrap()).unwrap();
        let first = compile_scenario_plan(&draft_path, &first_path).unwrap();
        let mut second = compile_scenario_plan(&draft_path, &second_path).unwrap();

        validate_reuse_plan_pair(&first, &second).unwrap();
        second.recipe.scenario_id = first.recipe.scenario_id.clone();
        assert!(validate_reuse_plan_pair(&first, &second).is_err());
        second.recipe.scenario_id = Uuid::new_v4().to_string();
        second.recipe.capabilities.pop();
        assert!(validate_reuse_plan_pair(&first, &second).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn transition_reuse_requires_independent_plans_for_one_exact_probe() {
        let first = ScenarioProbePlan {
            schema: "sts_java_play_scenario_probe_plan_v2".to_owned(),
            provenance: "constructed_validated".to_owned(),
            created_at_unix_ms: 1,
            probe_case_id: "cultist-base-end-turn".to_owned(),
            probe_draft_path: "probe.json".to_owned(),
            probe_draft_sha256: "a".repeat(64),
            scenario_case_id: "cultist-base-root-smoke".to_owned(),
            scenario_plan_path: "first-scenario.json".to_owned(),
            scenario_plan_sha256: "b".repeat(64),
            transition: ScenarioProbeTransition::CombatEndTurn,
        };
        let mut second = first.clone();
        second.scenario_plan_path = "second-scenario.json".to_owned();
        second.scenario_plan_sha256 = "c".repeat(64);

        validate_transition_reuse_probe_pair(&first, &second).unwrap();
        second.scenario_plan_sha256 = first.scenario_plan_sha256.clone();
        assert!(validate_transition_reuse_probe_pair(&first, &second).is_err());
        second.scenario_plan_sha256 = "c".repeat(64);
        second.probe_case_id = "another-probe".to_owned();
        assert!(validate_transition_reuse_probe_pair(&first, &second).is_err());
    }

    #[test]
    fn reuse_output_preflight_rejects_lexical_aliases() {
        let directory = std::env::temp_dir().join(format!("reuse-output-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let direct = directory.join("report.json");
        let alias = directory.join(".").join("report.json");
        assert!(validate_create_new_output_paths(&[&direct, &alias]).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compiler_rejects_missing_or_duplicate_capabilities() {
        let directory = std::env::temp_dir().join(format!("scenario-plan-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let draft_path = directory.join("draft.json");
        let plan_path = directory.join("plan.json");
        let mut invalid = draft();
        invalid.recipe.capabilities.pop();
        fs::write(&draft_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(compile_scenario_plan(&draft_path, &plan_path).is_err());

        let mut invalid = draft();
        invalid
            .recipe
            .capabilities
            .push(ScenarioCapability::Encounter(ScenarioEncounterSpec {
                encounter_id: "Cultist".to_owned(),
            }));
        fs::write(&draft_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(compile_scenario_plan(&draft_path, &plan_path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compiler_accepts_one_optional_fresh_exact_relic_set_in_registry_order() {
        let directory = std::env::temp_dir().join(format!("scenario-plan-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let draft_path = directory.join("draft.json");
        let plan_path = directory.join("plan.json");
        let mut value = draft();
        value.recipe.capabilities.insert(
            0,
            ScenarioCapability::FreshRelics(ScenarioRelicsSpec {
                relics: vec![ScenarioRelicSpec {
                    relic_id: "Chemical X".to_owned(),
                }],
            }),
        );
        fs::write(&draft_path, serde_json::to_vec(&value).unwrap()).unwrap();

        let plan = compile_scenario_plan(&draft_path, &plan_path).unwrap();
        assert_eq!(
            plan.recipe
                .capabilities
                .iter()
                .map(ScenarioCapability::id)
                .collect::<Vec<_>>(),
            vec![
                ENCOUNTER_CAPABILITY,
                HEALTH_CAPABILITY,
                DECK_CAPABILITY,
                FRESH_RELICS_CAPABILITY,
            ]
        );
        assert_eq!(plan.recipe.capability_registry, CAPABILITY_REGISTRY);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compiler_rejects_duplicate_relic_ids_before_launch() {
        let directory = std::env::temp_dir().join(format!("scenario-plan-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let draft_path = directory.join("draft.json");
        let plan_path = directory.join("plan.json");
        let mut value = draft();
        value
            .recipe
            .capabilities
            .push(ScenarioCapability::FreshRelics(ScenarioRelicsSpec {
                relics: vec![
                    ScenarioRelicSpec {
                        relic_id: "Chemical X".to_owned(),
                    },
                    ScenarioRelicSpec {
                        relic_id: "Chemical X".to_owned(),
                    },
                ],
            }));
        fs::write(&draft_path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(compile_scenario_plan(&draft_path, &plan_path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compiler_accepts_complete_a20_fresh_potion_slots_without_relic_capability() {
        let directory = std::env::temp_dir().join(format!("scenario-plan-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let draft_path = directory.join("draft.json");
        let plan_path = directory.join("plan.json");
        let mut value = draft();
        value.recipe.capabilities.push(fresh_potions());
        fs::write(&draft_path, serde_json::to_vec(&value).unwrap()).unwrap();

        let plan = compile_scenario_plan(&draft_path, &plan_path).unwrap();
        assert_eq!(
            plan.recipe
                .capabilities
                .iter()
                .map(ScenarioCapability::id)
                .collect::<Vec<_>>(),
            vec![
                ENCOUNTER_CAPABILITY,
                HEALTH_CAPABILITY,
                DECK_CAPABILITY,
                FRESH_POTIONS_CAPABILITY,
            ]
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn potion_slots_are_complete_contiguous_and_conflict_with_fresh_relics() {
        let mut wrong_count = draft();
        wrong_count
            .recipe
            .capabilities
            .push(ScenarioCapability::FreshPotions(ScenarioPotionsSpec {
                slots: vec![ScenarioPotionSlotSpec {
                    slot_index: 0,
                    potion_id: "Block Potion".to_owned(),
                }],
            }));
        assert!(validate_draft(&wrong_count).is_err());

        let mut non_contiguous = capabilities();
        non_contiguous.push(ScenarioCapability::FreshPotions(ScenarioPotionsSpec {
            slots: vec![
                ScenarioPotionSlotSpec {
                    slot_index: 0,
                    potion_id: "Block Potion".to_owned(),
                },
                ScenarioPotionSlotSpec {
                    slot_index: 2,
                    potion_id: "Potion Slot".to_owned(),
                },
            ],
        }));
        assert!(validate_capabilities(&non_contiguous).is_err());

        let mut conflicting = capabilities();
        conflicting.push(ScenarioCapability::FreshRelics(ScenarioRelicsSpec {
            relics: vec![],
        }));
        conflicting.push(fresh_potions());
        assert!(validate_capabilities(&conflicting).is_err());
    }

    #[test]
    fn legacy_v1_registry_remains_readable_but_cannot_claim_the_new_capability() {
        let directory = std::env::temp_dir().join(format!("scenario-plan-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let draft_path = directory.join("draft.json");
        let plan_path = directory.join("plan.json");
        fs::write(&draft_path, serde_json::to_vec(&draft()).unwrap()).unwrap();
        let plan = compile_scenario_plan(&draft_path, &plan_path).unwrap();

        let mut legacy = plan.recipe.clone();
        legacy.capability_registry = LEGACY_CAPABILITY_REGISTRY_V1.to_owned();
        assert!(validate_combat_scenario_recipe(&legacy).is_ok());
        legacy
            .capabilities
            .push(ScenarioCapability::FreshRelics(ScenarioRelicsSpec {
                relics: vec![ScenarioRelicSpec {
                    relic_id: "Chemical X".to_owned(),
                }],
            }));
        assert!(validate_combat_scenario_recipe(&legacy).is_err());

        let mut legacy_v2 = plan.recipe;
        legacy_v2.capability_registry = LEGACY_CAPABILITY_REGISTRY_V2.to_owned();
        legacy_v2
            .capabilities
            .push(ScenarioCapability::FreshRelics(ScenarioRelicsSpec {
                relics: vec![ScenarioRelicSpec {
                    relic_id: "Chemical X".to_owned(),
                }],
            }));
        assert!(validate_combat_scenario_recipe(&legacy_v2).is_ok());
        legacy_v2.capabilities.pop();
        legacy_v2.capabilities.push(fresh_potions());
        assert!(validate_combat_scenario_recipe(&legacy_v2).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn catalog_binds_exact_case_ids_to_validated_filename_matched_drafts() {
        let directory = std::env::temp_dir().join(format!("scenario-catalog-{}", Uuid::new_v4()));
        let catalog_directory = directory.join("cases");
        fs::create_dir_all(&catalog_directory).unwrap();
        let expected = draft();
        fs::write(
            catalog_directory.join("cultist-base-root.json"),
            serde_json::to_vec(&expected).unwrap(),
        )
        .unwrap();

        let catalog = scenario_case_catalog(&catalog_directory).unwrap();
        assert_eq!(catalog.cases.len(), 1);
        assert_eq!(catalog.cases[0].case_id, "cultist-base-root");
        assert_eq!(catalog.cases[0].draft_file, "cultist-base-root.json");
        assert!(compile_scenario_case(
            &catalog_directory,
            "cultist-base",
            &directory.join("missing-plan.json")
        )
        .is_err());
        let plan = compile_scenario_case(
            &catalog_directory,
            "cultist-base-root",
            &directory.join("plan.json"),
        )
        .unwrap();
        assert_eq!(plan.recipe.case_id, expected.case_id);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn catalog_rejects_a_filename_that_disagrees_with_embedded_case_id() {
        let directory = std::env::temp_dir().join(format!("scenario-catalog-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("renamed.json"),
            serde_json::to_vec(&draft()).unwrap(),
        )
        .unwrap();

        assert!(scenario_case_catalog(&directory).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn plan_validation_rejects_tampered_provenance_and_capability_order() {
        let directory = std::env::temp_dir().join(format!("scenario-plan-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let draft_path = directory.join("draft.json");
        let path = directory.join("plan.json");
        fs::write(&draft_path, serde_json::to_vec(&draft()).unwrap()).unwrap();
        let plan = compile_scenario_plan(&draft_path, &path).unwrap();

        let mut wrong = plan.clone();
        wrong.provenance = "natural_replay".to_owned();
        fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
        assert!(read_and_validate_plan(&path).is_err());

        let mut wrong = plan;
        wrong.recipe.capabilities.swap(0, 1);
        fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
        assert!(read_and_validate_plan(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn registry_exposes_recipe_dependencies_without_payloads() {
        let registry = scenario_registry();
        assert_eq!(registry.recipes.len(), 1);
        assert_eq!(
            registry.recipes[0].required_capabilities,
            required_capability_ids()
        );
        assert_eq!(
            registry.recipes[0].optional_capabilities,
            optional_capability_ids()
        );
        assert_eq!(registry.capabilities.len(), 5);
        let relic_capability = registry
            .capabilities
            .iter()
            .find(|capability| capability.capability_id == FRESH_RELICS_CAPABILITY)
            .unwrap();
        assert_eq!(relic_capability.cardinality, "zero_or_one");
        assert_eq!(
            relic_capability.requires,
            vec![HEALTH_CAPABILITY, DECK_CAPABILITY]
        );
        assert_eq!(relic_capability.conflicts, vec![FRESH_POTIONS_CAPABILITY]);
        let potion_capability = registry
            .capabilities
            .iter()
            .find(|capability| capability.capability_id == FRESH_POTIONS_CAPABILITY)
            .unwrap();
        assert_eq!(potion_capability.cardinality, "zero_or_one");
        assert_eq!(potion_capability.conflicts, vec![FRESH_RELICS_CAPABILITY]);
        assert_eq!(
            registry
                .transitions
                .iter()
                .map(|transition| transition.transition_id)
                .collect::<Vec<_>>(),
            vec![
                "combat.end_turn.v1",
                "combat.play_card.v1",
                "combat.use_potion.v1",
            ]
        );
        assert_eq!(registry.recipes[0].intermediate_boundaries.len(), 1);
        assert_eq!(
            registry.recipes[0].intermediate_boundaries[0].allowed_phases,
            vec!["before_initial_anchor", "after_commit_before_root"]
        );
        assert_eq!(
            registry.recipes[0].intermediate_boundaries[0].max_contiguous_per_phase,
            MAX_CONTIGUOUS_FTUES_PER_PHASE
        );
        assert!(registry
            .capabilities
            .iter()
            .all(|capability| capability.phase == "prepare_before_atomic_commit"));
    }

    #[test]
    fn constructed_root_scope_does_not_claim_a_simulator_comparison() {
        let scope = ScenarioEvidenceScope::constructed_root(true);
        assert!(scope.constructed_root_verified);
        assert!(!scope.simulator_comparison);
        assert!(!scope.natural_reachability);
        assert!(!scope.strategy_evidence);
        assert!(!scope.training_eligible);
    }
}
