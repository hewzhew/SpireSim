use super::probe::{
    resolve_scenario_transition_action, validate_transition_root_dependencies,
    ScenarioProbeTransition,
};
use super::*;
use std::collections::BTreeSet;

const TRAJECTORY_DRAFT_SCHEMA: &str = "sts_java_play_scenario_trajectory_draft_v1";
const TRAJECTORY_PLAN_SCHEMA: &str = "sts_java_play_scenario_trajectory_plan_v1";
const MIN_TRAJECTORY_STEPS: usize = 2;
const MAX_TRAJECTORY_STEPS: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTrajectoryDraft {
    pub schema: String,
    pub provenance: String,
    pub trajectory_case_id: String,
    pub scenario_case_id: String,
    pub steps: Vec<ScenarioProbeTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTrajectoryPlan {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub trajectory_case_id: String,
    pub trajectory_draft_path: String,
    pub trajectory_draft_sha256: String,
    pub scenario_case_id: String,
    pub scenario_plan_path: String,
    pub scenario_plan_sha256: String,
    pub steps: Vec<ScenarioProbeTransition>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTrajectoryCaseCatalog {
    pub schema: &'static str,
    pub catalog_directory: String,
    pub cases: Vec<ScenarioTrajectoryCaseCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTrajectoryCaseCatalogEntry {
    pub trajectory_case_id: String,
    pub draft_file: String,
    pub draft_sha256: String,
    pub provenance: String,
    pub scenario_case_id: String,
    pub steps: Vec<ScenarioProbeTransition>,
}

pub fn scenario_trajectory_case_catalog(
    catalog_directory: &Path,
) -> Result<ScenarioTrajectoryCaseCatalog, String> {
    let entries = fs::read_dir(catalog_directory).map_err(|error| {
        format!(
            "failed to read scenario trajectory case catalog {}: {error}",
            catalog_directory.display()
        )
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to enumerate scenario trajectory case catalog {}: {error}",
                catalog_directory.display()
            )
        })?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| {
                format!(
                    "failed to inspect scenario trajectory case {}: {error}",
                    path.display()
                )
            })?
            .is_file()
            && path.extension().and_then(|value| value.to_str()) == Some("json")
        {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err("scenario trajectory case catalog contains no JSON drafts".to_owned());
    }

    let mut ids = BTreeSet::new();
    let mut cases = Vec::with_capacity(paths.len());
    for path in paths {
        let draft = read_and_validate_trajectory_draft(&path)?;
        let expected_file = format!("{}.json", draft.trajectory_case_id);
        let draft_file = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                format!(
                    "scenario trajectory filename is not valid Unicode: {}",
                    path.display()
                )
            })?;
        if draft_file != expected_file {
            return Err(format!(
                "scenario trajectory filename must exactly match embedded trajectory_case_id: expected {expected_file}, found {draft_file}"
            ));
        }
        if !ids.insert(draft.trajectory_case_id.clone()) {
            return Err(format!(
                "scenario trajectory catalog contains duplicate trajectory_case_id: {}",
                draft.trajectory_case_id
            ));
        }
        cases.push(ScenarioTrajectoryCaseCatalogEntry {
            trajectory_case_id: draft.trajectory_case_id,
            draft_file: draft_file.to_owned(),
            draft_sha256: sha256_file(&path)?,
            provenance: draft.provenance,
            scenario_case_id: draft.scenario_case_id,
            steps: draft.steps,
        });
    }
    cases.sort_by(|left, right| left.trajectory_case_id.cmp(&right.trajectory_case_id));
    Ok(ScenarioTrajectoryCaseCatalog {
        schema: "sts_java_parity_scenario_trajectory_case_catalog_v1",
        catalog_directory: absolute_string(catalog_directory)?,
        cases,
    })
}

pub fn compile_scenario_trajectory_case(
    trajectory_catalog_directory: &Path,
    trajectory_case_id: &str,
    scenario_catalog_directory: &Path,
    scenario_plan_path: &Path,
    trajectory_plan_path: &Path,
) -> Result<ScenarioTrajectoryPlan, String> {
    if !valid_case_id(trajectory_case_id) {
        return Err(
            "scenario trajectory case ID is outside the exact lowercase slug schema".to_owned(),
        );
    }
    let catalog = scenario_trajectory_case_catalog(trajectory_catalog_directory)?;
    let entry = catalog
        .cases
        .iter()
        .find(|entry| entry.trajectory_case_id == trajectory_case_id)
        .ok_or_else(|| {
            format!(
                "scenario trajectory catalog has no exact trajectory_case_id '{trajectory_case_id}'"
            )
        })?;
    let trajectory_draft_path = trajectory_catalog_directory.join(&entry.draft_file);
    let draft = read_and_validate_trajectory_draft(&trajectory_draft_path)?;
    if draft.trajectory_case_id != trajectory_case_id {
        return Err("scenario trajectory identity changed during catalog resolution".to_owned());
    }

    let scenario_catalog = scenario_case_catalog(scenario_catalog_directory)?;
    let scenario_entry = scenario_catalog
        .cases
        .iter()
        .find(|candidate| candidate.case_id == draft.scenario_case_id)
        .ok_or_else(|| {
            format!(
                "scenario trajectory references missing exact scenario_case_id '{}'",
                draft.scenario_case_id
            )
        })?;
    let scenario_draft =
        read_and_validate_draft(&scenario_catalog_directory.join(&scenario_entry.draft_file))?;
    for transition in &draft.steps {
        validate_transition_root_dependencies(transition, &scenario_draft)?;
    }

    let scenario_plan = compile_scenario_case(
        scenario_catalog_directory,
        &draft.scenario_case_id,
        scenario_plan_path,
    )?;
    if scenario_plan.recipe.case_id != draft.scenario_case_id {
        return Err("compiled scenario plan disagrees with its trajectory case".to_owned());
    }
    let plan = ScenarioTrajectoryPlan {
        schema: TRAJECTORY_PLAN_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: unix_ms()?,
        trajectory_case_id: draft.trajectory_case_id,
        trajectory_draft_path: absolute_string(&trajectory_draft_path)?,
        trajectory_draft_sha256: sha256_file(&trajectory_draft_path)?,
        scenario_case_id: draft.scenario_case_id,
        scenario_plan_path: absolute_string(scenario_plan_path)?,
        scenario_plan_sha256: sha256_file(scenario_plan_path)?,
        steps: draft.steps,
    };
    publish_json_create_new(trajectory_plan_path, &plan)?;
    read_and_validate_trajectory_plan(trajectory_plan_path)
}

fn read_and_validate_trajectory_draft(path: &Path) -> Result<ScenarioTrajectoryDraft, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read scenario trajectory draft {}: {error}",
            path.display()
        )
    })?;
    let draft: ScenarioTrajectoryDraft = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse scenario trajectory draft {}: {error}",
            path.display()
        )
    })?;
    if draft.schema != TRAJECTORY_DRAFT_SCHEMA
        || draft.provenance != "constructed_validated"
        || !valid_case_id(&draft.trajectory_case_id)
        || !valid_case_id(&draft.scenario_case_id)
    {
        return Err("scenario trajectory draft identity or provenance is invalid".to_owned());
    }
    if !(MIN_TRAJECTORY_STEPS..=MAX_TRAJECTORY_STEPS).contains(&draft.steps.len()) {
        return Err(format!(
            "scenario trajectory must contain {MIN_TRAJECTORY_STEPS} through {MAX_TRAJECTORY_STEPS} typed steps"
        ));
    }
    for transition in &draft.steps {
        transition.validate()?;
    }
    Ok(draft)
}

pub(super) fn read_and_validate_trajectory_plan(
    path: &Path,
) -> Result<ScenarioTrajectoryPlan, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read scenario trajectory plan {}: {error}",
            path.display()
        )
    })?;
    let plan: ScenarioTrajectoryPlan = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse scenario trajectory plan {}: {error}",
            path.display()
        )
    })?;
    if plan.schema != TRAJECTORY_PLAN_SCHEMA
        || plan.provenance != "constructed_validated"
        || plan.created_at_unix_ms == 0
        || !valid_case_id(&plan.trajectory_case_id)
        || !valid_case_id(&plan.scenario_case_id)
        || !(MIN_TRAJECTORY_STEPS..=MAX_TRAJECTORY_STEPS).contains(&plan.steps.len())
    {
        return Err("scenario trajectory plan identity is invalid".to_owned());
    }
    for transition in &plan.steps {
        transition.validate()?;
    }

    let trajectory_draft_path = PathBuf::from(&plan.trajectory_draft_path);
    if sha256_file(&trajectory_draft_path)? != plan.trajectory_draft_sha256 {
        return Err("scenario trajectory draft changed after plan compilation".to_owned());
    }
    let draft = read_and_validate_trajectory_draft(&trajectory_draft_path)?;
    if draft.trajectory_case_id != plan.trajectory_case_id
        || draft.scenario_case_id != plan.scenario_case_id
        || draft.steps != plan.steps
    {
        return Err("scenario trajectory plan disagrees with its bound draft".to_owned());
    }

    let scenario_plan_path = PathBuf::from(&plan.scenario_plan_path);
    if sha256_file(&scenario_plan_path)? != plan.scenario_plan_sha256 {
        return Err("scenario plan changed after trajectory plan compilation".to_owned());
    }
    let scenario_plan = read_and_validate_plan(&scenario_plan_path)?;
    if scenario_plan.recipe.case_id != plan.scenario_case_id {
        return Err("scenario trajectory plan disagrees with its bound scenario plan".to_owned());
    }
    Ok(plan)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedScenarioTrajectoryReport {
    pub schema: String,
    pub provenance: String,
    pub evidence_scope: ScenarioTrajectoryEvidenceScope,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub failure: Option<String>,
    pub failed_step_index: Option<usize>,
    pub trajectory_case_id: String,
    pub trajectory_plan_path: String,
    pub trajectory_plan_sha256: String,
    pub scenario_report_path: String,
    pub scenario_report_sha256: String,
    pub session: SessionManifest,
    pub root_frame_id: u64,
    pub planned_steps: usize,
    pub completed_steps: usize,
    pub steps: Vec<ScenarioTrajectoryStepReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTrajectoryEvidenceScope {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub constructed_root_verified: bool,
    pub original_game_trajectory_captured: bool,
    pub simulator_comparison: bool,
}

impl ScenarioTrajectoryEvidenceScope {
    fn after_verified_root(trajectory_captured: bool) -> Self {
        Self {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            constructed_root_verified: true,
            original_game_trajectory_captured: trajectory_captured,
            simulator_comparison: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTrajectoryStepReport {
    pub index: usize,
    pub transition_id: String,
    pub transition: ScenarioProbeTransition,
    pub from_frame_id: u64,
    pub action: ScenarioStepProof,
    pub successor: Option<ScenarioTrajectoryFrameProof>,
    pub adjacent_stable_successor_verified: bool,
    pub constructed_context_preserved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTrajectoryFrameProof {
    pub frame_id: u64,
    pub frame_path: String,
    pub frame_sha256: String,
    pub boundary: String,
    pub scenario_context: Value,
}

pub fn run_constructed_combat_trajectory(
    output_dir: &Path,
    trajectory_plan_path: &Path,
    expected_launch_plan: &Path,
    scenario_report_path: &Path,
    trajectory_report_path: &Path,
    timeout: Duration,
) -> Result<ConstructedScenarioTrajectoryReport, String> {
    let trajectory_plan = read_and_validate_trajectory_plan(trajectory_plan_path)?;
    let scenario_plan_path = PathBuf::from(&trajectory_plan.scenario_plan_path);
    let root_report = run_constructed_combat_scenario(
        output_dir,
        &scenario_plan_path,
        expected_launch_plan,
        scenario_report_path,
        timeout,
    )?;
    let root = root_report
        .root
        .as_ref()
        .ok_or_else(|| "passing constructed scenario report omitted its root".to_owned())?;
    if root.scenario_context["case_id"].as_str() != Some(trajectory_plan.scenario_case_id.as_str())
    {
        return Err(
            "constructed trajectory root does not match its bound scenario case".to_owned(),
        );
    }

    let created_at = unix_ms()?;
    let mut report = ConstructedScenarioTrajectoryReport {
        schema: "sts_java_parity_constructed_trajectory_report_v1".to_owned(),
        provenance: "constructed_validated".to_owned(),
        evidence_scope: ScenarioTrajectoryEvidenceScope::after_verified_root(false),
        created_at_unix_ms: created_at,
        completed_at_unix_ms: created_at,
        passed: false,
        failure: None,
        failed_step_index: None,
        trajectory_case_id: trajectory_plan.trajectory_case_id,
        trajectory_plan_path: absolute_string(trajectory_plan_path)?,
        trajectory_plan_sha256: sha256_file(trajectory_plan_path)?,
        scenario_report_path: absolute_string(scenario_report_path)?,
        scenario_report_sha256: sha256_file(scenario_report_path)?,
        session: root_report.session.clone(),
        root_frame_id: root.frame_id,
        planned_steps: trajectory_plan.steps.len(),
        completed_steps: 0,
        steps: Vec::with_capacity(trajectory_plan.steps.len()),
    };

    let outcome = execute_trajectory(
        output_dir,
        &trajectory_plan.steps,
        &root_report,
        timeout,
        &mut report,
    );
    report.completed_at_unix_ms = unix_ms()?;
    match outcome {
        Ok(()) => {
            report.passed = report.completed_steps == report.planned_steps;
            report.failed_step_index = None;
            report.evidence_scope =
                ScenarioTrajectoryEvidenceScope::after_verified_root(report.passed);
            if !report.passed {
                report.failure =
                    Some("constructed trajectory ended without every planned successor".to_owned());
            }
        }
        Err(error) => report.failure = Some(error),
    }
    publish_json_create_new(trajectory_report_path, &report)?;
    if report.passed {
        Ok(report)
    } else {
        Err(format!(
            "constructed scenario trajectory failed; durable report={}",
            trajectory_report_path.display()
        ))
    }
}

fn execute_trajectory(
    output_dir: &Path,
    transitions: &[ScenarioProbeTransition],
    root_report: &ConstructedScenarioReport,
    timeout: Duration,
    report: &mut ConstructedScenarioTrajectoryReport,
) -> Result<(), String> {
    let root = root_report.root.as_ref().unwrap();
    let mut expected_frame_id = root.frame_id;
    for (index, transition) in transitions.iter().enumerate() {
        report.failed_step_index = Some(index);
        let before = status(output_dir)?;
        if frame_id(&before)? != expected_frame_id
            || boundary_kind(&before) != Some("combat_player_decision")
            || before.frame["scenario"] != root.scenario_context
        {
            return Err(format!(
                "trajectory step {index} no longer owns its expected constructed combat boundary"
            ));
        }

        let action = resolve_scenario_transition_action(&before, transition)?;
        let receipt = submit_action(&before, action, empty_object(), timeout)?;
        let action_proof = scenario_step_proof(
            &root_report.session,
            &format!("trajectory_step_{index}"),
            &receipt,
        )?;
        report.steps.push(ScenarioTrajectoryStepReport {
            index,
            transition_id: transition.id().to_owned(),
            transition: transition.clone(),
            from_frame_id: expected_frame_id,
            action: action_proof,
            successor: None,
            adjacent_stable_successor_verified: false,
            constructed_context_preserved: false,
        });
        require_completed_successor(&receipt, transition.id())?;

        let after = status(output_dir)?;
        let after_frame_id = frame_id(&after)?;
        let adjacent = receipt.successor_frame_id == Some(expected_frame_id + 1)
            && receipt.successor_frame_id == Some(after_frame_id);
        let context_preserved = after.frame["scenario"] == root.scenario_context;
        let has_next_step = index + 1 < transitions.len();
        if !adjacent
            || !context_preserved
            || (has_next_step && boundary_kind(&after) != Some("combat_player_decision"))
        {
            return Err(format!(
                "trajectory step {index} did not reach the required adjacent constructed successor"
            ));
        }
        let successor = ScenarioTrajectoryFrameProof {
            frame_id: after_frame_id,
            frame_path: absolute_string(Path::new(&after.frame_path))?,
            frame_sha256: sha256_file(Path::new(&after.frame_path))?,
            boundary: boundary_kind(&after).unwrap_or_default().to_owned(),
            scenario_context: after.frame["scenario"].clone(),
        };
        let step = report.steps.last_mut().unwrap();
        step.successor = Some(successor);
        step.adjacent_stable_successor_verified = true;
        step.constructed_context_preserved = true;
        report.completed_steps += 1;
        expected_frame_id = after_frame_id;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strike(occurrence: usize) -> ScenarioProbeTransition {
        ScenarioProbeTransition::CombatPlayCard {
            card: super::super::probe::ScenarioProbeCardSelector {
                card_id: "Strike_R".to_owned(),
                upgrades: 0,
                occurrence,
            },
            target: Some(super::super::probe::ScenarioProbeMonsterSelector {
                monster_id: "Cultist".to_owned(),
                occurrence: 0,
            }),
        }
    }

    #[test]
    fn compiler_binds_complete_ordered_trajectory_before_launch() {
        let directory =
            std::env::temp_dir().join(format!("scenario-trajectory-{}", Uuid::new_v4()));
        let scenario_catalog = directory.join("scenarios");
        let trajectory_catalog = directory.join("trajectories");
        fs::create_dir_all(&scenario_catalog).unwrap();
        fs::create_dir_all(&trajectory_catalog).unwrap();

        let mut scenario_draft = super::super::tests::draft();
        scenario_draft.case_id = "test-two-strikes-root".to_owned();
        let deck = scenario_draft
            .recipe
            .capabilities
            .iter_mut()
            .find_map(|capability| match capability {
                ScenarioCapability::MasterDeck(deck) => Some(deck),
                _ => None,
            })
            .unwrap();
        deck.cards.push(ScenarioCardSpec {
            card_id: "Strike_R".to_owned(),
            upgrades: 0,
        });
        fs::write(
            scenario_catalog.join("test-two-strikes-root.json"),
            serde_json::to_vec(&scenario_draft).unwrap(),
        )
        .unwrap();

        let draft = ScenarioTrajectoryDraft {
            schema: TRAJECTORY_DRAFT_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            trajectory_case_id: "test-two-strikes".to_owned(),
            scenario_case_id: "test-two-strikes-root".to_owned(),
            steps: vec![strike(1), strike(0)],
        };
        fs::write(
            trajectory_catalog.join("test-two-strikes.json"),
            serde_json::to_vec(&draft).unwrap(),
        )
        .unwrap();

        let scenario_plan_path = directory.join("scenario-plan.json");
        let trajectory_plan_path = directory.join("trajectory-plan.json");
        let plan = compile_scenario_trajectory_case(
            &trajectory_catalog,
            "test-two-strikes",
            &scenario_catalog,
            &scenario_plan_path,
            &trajectory_plan_path,
        )
        .unwrap();
        assert_eq!(plan.trajectory_case_id, "test-two-strikes");
        assert_eq!(plan.scenario_case_id, "test-two-strikes-root");
        assert_eq!(plan.steps, draft.steps);
        assert!(scenario_plan_path.is_file());
        assert!(trajectory_plan_path.is_file());
        assert_eq!(
            read_and_validate_trajectory_plan(&trajectory_plan_path).unwrap(),
            plan
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn trajectory_requires_multiple_bounded_typed_steps() {
        let directory =
            std::env::temp_dir().join(format!("scenario-trajectory-invalid-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let draft = ScenarioTrajectoryDraft {
            schema: TRAJECTORY_DRAFT_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            trajectory_case_id: "test-one-step".to_owned(),
            scenario_case_id: "test-root".to_owned(),
            steps: vec![ScenarioProbeTransition::CombatEndTurn],
        };
        let path = directory.join("test-one-step.json");
        fs::write(&path, serde_json::to_vec(&draft).unwrap()).unwrap();
        let error = read_and_validate_trajectory_draft(&path).unwrap_err();
        assert!(error.contains("2 through 16 typed steps"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn captured_trajectory_scope_keeps_comparison_and_training_false() {
        let scope = ScenarioTrajectoryEvidenceScope::after_verified_root(true);
        assert!(scope.constructed_root_verified);
        assert!(scope.original_game_trajectory_captured);
        assert!(!scope.simulator_comparison);
        assert!(!scope.natural_reachability);
        assert!(!scope.strategy_evidence);
        assert!(!scope.training_eligible);
    }
}
