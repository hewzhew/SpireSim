use crate::lifecycle::{verify_isolated_lifecycle, IsolatedLifecycleExpectation};
use crate::{
    compare_live_action_typed, current_unix_ms, file_identity, file_identity_from_bytes,
    write_json_report_new, ComparisonFileIdentityV1, JavaParityLiveComparisonReportV1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use sts_java_parity_live::{
    ActionReceipt, NaturalCombatActionObservation, NaturalCombatActionReport, ProbeFileIdentity,
    ProbeStepProof, SessionManifest,
};

const NATURAL_COMBAT_REPORT_SCHEMA: &str = "sts_java_parity_natural_combat_action_report_v1";
const NATURAL_WORKFLOW_SCHEMA: &str = "sts_java_parity_natural_combat_workflow_v1";
const NATURAL_STRIKE_RECIPE_ID: &str = "seed_0_a20_ironclad_first_combat_base_strike";
const RUN_CONTROL_PROFILE: &str = "parity_run_control";
const MAX_NATURAL_FTUE_STEPS: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatWorkflowV1 {
    pub schema: String,
    pub created_at_unix_ms: u128,
    pub passed: bool,
    pub provenance: String,
    pub recipe_id: String,
    pub recipe_version: u32,
    pub session_id: String,
    pub process_id: u32,
    pub isolation_id: String,
    pub launch_profile: String,
    pub comparison_kind: String,
    pub action: NaturalCombatActionObservation,
    pub evidence_scope: NaturalCombatWorkflowScopeV1,
    pub producer_report: ComparisonFileIdentityV1,
    pub launch_plan: ComparisonFileIdentityV1,
    pub session_manifest: ComparisonFileIdentityV1,
    pub initial_frame: ComparisonFileIdentityV1,
    pub steps: Vec<NaturalCombatWorkflowStepV1>,
    pub action_receipt: ComparisonFileIdentityV1,
    pub before_frame: ComparisonFileIdentityV1,
    pub after_frame: ComparisonFileIdentityV1,
    pub comparison_report: ComparisonFileIdentityV1,
    pub lifecycle_receipt: ComparisonFileIdentityV1,
    pub lifecycle_total_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatWorkflowScopeV1 {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub constructed_root_verified: bool,
    pub original_game_navigation_captured: bool,
    pub original_game_transition_captured: bool,
    pub simulator_comparison: bool,
    pub narrow_mechanics_parity: bool,
    pub managed_lifecycle_verified: bool,
    pub strict_runtime_profile: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatWorkflowStepV1 {
    pub index: usize,
    pub name: String,
    pub request_id: String,
    pub action_id: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub action_receipt: ComparisonFileIdentityV1,
    pub successor_frame: ComparisonFileIdentityV1,
}

struct ValidatedNaturalTrace {
    producer_identity: ComparisonFileIdentityV1,
    report: NaturalCombatActionReport,
    launch_plan: ComparisonFileIdentityV1,
    session_manifest: ComparisonFileIdentityV1,
    initial_frame: ComparisonFileIdentityV1,
    steps: Vec<NaturalCombatWorkflowStepV1>,
    action_receipt: ComparisonFileIdentityV1,
    before_frame: ComparisonFileIdentityV1,
    after_frame: ComparisonFileIdentityV1,
}

pub fn verify_natural_combat_action_new(
    producer_report_path: &Path,
    lifecycle_receipt_path: &Path,
    comparison_report_path: &Path,
    workflow_path: &Path,
) -> Result<NaturalCombatWorkflowV1, String> {
    if comparison_report_path.exists() || workflow_path.exists() {
        return Err("natural comparison report and workflow must both be create-new".to_owned());
    }
    let validated = validate_natural_trace(producer_report_path)?;
    let launch = validated
        .report
        .session
        .launch
        .as_ref()
        .ok_or_else(|| "natural combat report omitted launch evidence".to_owned())?;
    let isolation_id = launch
        .isolation_id
        .as_deref()
        .ok_or_else(|| "natural combat report omitted isolation ID".to_owned())?;
    let lifecycle = verify_isolated_lifecycle(
        lifecycle_receipt_path,
        IsolatedLifecycleExpectation {
            case_kind: "natural_combat_base_strike",
            profile: &launch.profile,
            control_profile: RUN_CONTROL_PROFILE,
            session_id: &validated.report.session.session_id,
            isolation_id,
            launch_plan_sha256: &launch.plan_sha256,
            case_evidence: &validated.producer_identity,
        },
    )?;

    let comparison = match compare_live_action_typed(Path::new(&validated.action_receipt.path)) {
        Ok(comparison) => comparison,
        Err(failure) => {
            write_json_report_new(comparison_report_path, &failure)?;
            return Err(format!(
                "natural combat comparator returned {:?}; durable failure={}",
                failure.kind,
                comparison_report_path.display()
            ));
        }
    };
    write_json_report_new(comparison_report_path, &comparison)?;
    validate_natural_strike_comparison(&validated, &comparison)?;

    let workflow = NaturalCombatWorkflowV1 {
        schema: NATURAL_WORKFLOW_SCHEMA.to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        passed: true,
        provenance: "natural_replay".to_owned(),
        recipe_id: validated.report.recipe_id.clone(),
        recipe_version: validated.report.recipe_version,
        session_id: validated.report.session.session_id.clone(),
        process_id: validated.report.session.process_id,
        isolation_id: isolation_id.to_owned(),
        launch_profile: launch.profile.clone(),
        comparison_kind: comparison.transition_kind().to_owned(),
        action: validated.report.action.clone(),
        evidence_scope: NaturalCombatWorkflowScopeV1 {
            natural_reachability: true,
            strategy_evidence: false,
            training_eligible: false,
            constructed_root_verified: false,
            original_game_navigation_captured: true,
            original_game_transition_captured: true,
            simulator_comparison: true,
            narrow_mechanics_parity: true,
            managed_lifecycle_verified: true,
            strict_runtime_profile: launch.profile == "strict",
        },
        producer_report: validated.producer_identity,
        launch_plan: validated.launch_plan,
        session_manifest: validated.session_manifest,
        initial_frame: validated.initial_frame,
        steps: validated.steps,
        action_receipt: validated.action_receipt,
        before_frame: validated.before_frame,
        after_frame: validated.after_frame,
        comparison_report: file_identity(comparison_report_path)?,
        lifecycle_receipt: lifecycle.identity,
        lifecycle_total_ms: lifecycle.total_ms,
    };
    write_json_report_new(workflow_path, &workflow)?;
    Ok(workflow)
}

fn validate_natural_trace(path: &Path) -> Result<ValidatedNaturalTrace, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read natural combat report '{}': {error}",
            path.display()
        )
    })?;
    let report: NaturalCombatActionReport = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode natural combat report: {error}"))?;
    let producer_identity = file_identity_from_bytes(path, &bytes)?;
    if report.schema != NATURAL_COMBAT_REPORT_SCHEMA
        || report.recipe_id != NATURAL_STRIKE_RECIPE_ID
        || report.recipe_version != 1
        || report.provenance != "natural_replay"
        || report.created_at_unix_ms == 0
        || report.completed_at_unix_ms < report.created_at_unix_ms
        || !report.passed
        || report.failure.is_some()
        || report.checks.is_empty()
        || report.checks.iter().any(|check| !check.passed)
        || report.steps.is_empty()
        || report.compared_action_step_index.checked_add(1) != Some(report.steps.len())
    {
        return Err("natural combat report is not one complete passing closed recipe".to_owned());
    }
    let launch = report
        .session
        .launch
        .as_ref()
        .ok_or_else(|| "natural combat report omitted launch evidence".to_owned())?;
    if report.session.schema != "sts_java_parity_live_session_v3"
        || report.session.profile != RUN_CONTROL_PROFILE
        || launch.profile != "accelerated"
        || launch.control_profile != RUN_CONTROL_PROFILE
        || launch.storage_class != "isolated_working_directory_v1"
        || launch.isolation_id.as_deref().is_none_or(str::is_empty)
        || !same_path(
            Path::new(&report.expected_launch_plan),
            Path::new(&launch.plan_path),
        )
    {
        return Err("natural combat report did not bind an isolated run-control launch".to_owned());
    }
    let launch_plan = file_identity(Path::new(&launch.plan_path))?;
    if launch_plan.sha256 != launch.plan_sha256 {
        return Err("natural combat launch plan changed after capture".to_owned());
    }
    let session_manifest = validate_probe_file_identity(&report.session_manifest)?;
    let stored_manifest: SessionManifest = read_json(Path::new(&session_manifest.path))?;
    if serde_json::to_value(&stored_manifest).map_err(|error| error.to_string())?
        != serde_json::to_value(&report.session).map_err(|error| error.to_string())?
    {
        return Err("natural combat report embedded a different session manifest".to_owned());
    }
    let initial_frame = validate_probe_file_identity(&report.initial_frame)?;
    let initial: Value = read_json(Path::new(&initial_frame.path))?;
    let initial_frame_id = initial["frame_id"]
        .as_u64()
        .ok_or_else(|| "natural initial frame omitted frame_id".to_owned())?;
    if initial_frame_id != 1
        || initial["capture_id"].as_str() != Some(&report.session.capture_id)
        || initial["producer_session_id"].as_str() != Some(&report.session.producer_session_id)
        || initial["connection_id"].as_str() != Some(&report.session.connection_id)
        || initial["profile"].as_str() != Some(RUN_CONTROL_PROFILE)
        || initial["boundary"]["kind"].as_str() != Some("main_menu_ready")
        || initial["boundary"]["stable"].as_bool() != Some(true)
        || initial["public_base"]["decision"]["kind"].as_str() != Some("main_menu_ready")
        || !initial["scenario"].is_null()
        || !initial["parity_internal"]["run_seed"].is_null()
    {
        return Err("natural trace did not begin at its typed fresh main menu".to_owned());
    }

    let core_names = report
        .steps
        .iter()
        .filter(|step| step.name != "acknowledge_ftue")
        .map(|step| step.name.as_str())
        .collect::<Vec<_>>();
    if core_names
        != [
            "start_seed_0_a20_ironclad",
            "neow_talk",
            "neow_choose_three_potions",
            "claim_first_elixir",
            "claim_block_potion",
            "close_neow_reward",
            "choose_first_monster_node",
            "play_first_base_strike",
        ]
        || report
            .steps
            .iter()
            .filter(|step| step.name == "acknowledge_ftue")
            .count()
            > MAX_NATURAL_FTUE_STEPS
    {
        return Err("natural trace does not follow the closed navigation recipe".to_owned());
    }

    let mut previous_frame = initial_frame.clone();
    let mut previous_frame_id = initial_frame_id;
    let mut steps = Vec::with_capacity(report.steps.len());
    for (index, proof) in report.steps.iter().enumerate() {
        let validated = validate_step(&report.session, proof, previous_frame_id)?;
        let predecessor: Value = read_json(Path::new(&previous_frame.path))?;
        validate_recipe_action(&predecessor, proof, &validated.receipt, &report.action)?;
        previous_frame = validated.successor_frame.clone();
        previous_frame_id = proof.successor_frame_id.unwrap();
        steps.push(NaturalCombatWorkflowStepV1 {
            index,
            name: proof.name.clone(),
            request_id: proof.request_id.clone(),
            action_id: proof.action_id.clone(),
            from_frame_id: proof.based_on_frame_id,
            to_frame_id: previous_frame_id,
            action_receipt: validated.action_receipt,
            successor_frame: validated.successor_frame,
        });
    }
    let action_index = report.compared_action_step_index;
    let action_step = &report.steps[action_index];
    if action_step.name != "play_first_base_strike"
        || action_step.action_id != report.action.action_id
        || action_step.based_on_frame_id != report.action.from_frame_id
        || action_step.successor_frame_id != Some(report.action.to_frame_id)
        || report.action.card_id != "Strike_R"
        || report.action.card_uuid.is_empty()
        || report.action.target_monster_index != 0
        || report.action.target_monster_id.is_empty()
        || report.action.from_frame_id.checked_add(1) != Some(report.action.to_frame_id)
    {
        return Err("natural report did not bind its declared base Strike step".to_owned());
    }
    let action_receipt = steps[action_index].action_receipt.clone();
    let before_frame = if action_index == 0 {
        initial_frame.clone()
    } else {
        steps[action_index - 1].successor_frame.clone()
    };
    let after_frame = steps[action_index].successor_frame.clone();
    Ok(ValidatedNaturalTrace {
        producer_identity,
        report,
        launch_plan,
        session_manifest,
        initial_frame,
        steps,
        action_receipt,
        before_frame,
        after_frame,
    })
}

struct ValidatedStep {
    receipt: ActionReceipt,
    action_receipt: ComparisonFileIdentityV1,
    successor_frame: ComparisonFileIdentityV1,
}

fn validate_step(
    session: &SessionManifest,
    proof: &ProbeStepProof,
    expected_frame_id: u64,
) -> Result<ValidatedStep, String> {
    let expected_successor_frame_id = expected_frame_id
        .checked_add(1)
        .ok_or_else(|| "natural trace frame ID overflowed".to_owned())?;
    let receipt_path = Path::new(&proof.receipt_path);
    let action_receipt = file_identity(receipt_path)?;
    let successor_path = proof
        .successor_frame_path
        .as_deref()
        .map(Path::new)
        .ok_or_else(|| "natural step omitted successor frame path".to_owned())?;
    let successor_frame = file_identity(successor_path)?;
    if action_receipt.sha256 != proof.receipt_sha256
        || successor_frame.sha256 != proof.successor_frame_sha256.as_deref().unwrap_or("")
        || proof.status != "completed"
        || proof.based_on_frame_id != expected_frame_id
        || proof.successor_frame_id != Some(expected_successor_frame_id)
    {
        return Err(format!(
            "natural step '{}' evidence changed or is discontinuous",
            proof.name
        ));
    }
    let receipt: ActionReceipt = read_json(receipt_path)?;
    if receipt.status != proof.status
        || receipt.request_id != proof.request_id
        || receipt.action_id != proof.action_id
        || receipt.based_on_frame_id != proof.based_on_frame_id
        || receipt.successor_frame_id != proof.successor_frame_id
        || receipt.parameters != proof.parameters
        || receipt.session_id != session.session_id
        || receipt.connection_id != session.connection_id
        || receipt
            .successor_frame_path
            .as_deref()
            .is_none_or(|path| !same_path(Path::new(path), successor_path))
    {
        return Err(format!(
            "natural step '{}' receipt disagrees with its proof",
            proof.name
        ));
    }
    let frame: Value = read_json(successor_path)?;
    if frame["frame_id"].as_u64() != proof.successor_frame_id
        || frame["capture_id"].as_str() != Some(&session.capture_id)
        || frame["producer_session_id"].as_str() != Some(&session.producer_session_id)
        || frame["connection_id"].as_str() != Some(&session.connection_id)
    {
        return Err(format!(
            "natural step '{}' successor has the wrong identity",
            proof.name
        ));
    }
    Ok(ValidatedStep {
        receipt,
        action_receipt,
        successor_frame,
    })
}

fn validate_recipe_action(
    predecessor: &Value,
    proof: &ProbeStepProof,
    receipt: &ActionReceipt,
    compared_action: &NaturalCombatActionObservation,
) -> Result<(), String> {
    let actions = predecessor["action_space"]["actions"]
        .as_array()
        .ok_or_else(|| format!("natural step '{}' predecessor omitted actions", proof.name))?;
    let matches = actions
        .iter()
        .filter(|action| action["action_id"].as_str() == Some(&proof.action_id))
        .collect::<Vec<_>>();
    let action = match matches.as_slice() {
        [action] => *action,
        [] => return Err(format!("natural step '{}' action is absent", proof.name)),
        _ => return Err(format!("natural step '{}' action is aliased", proof.name)),
    };
    if proof.name != "start_seed_0_a20_ironclad"
        && receipt
            .parameters
            .as_object()
            .is_none_or(|value| !value.is_empty())
    {
        return Err(format!(
            "natural step '{}' has unexpected parameters",
            proof.name
        ));
    }
    let matches_recipe = match proof.name.as_str() {
        "start_seed_0_a20_ironclad" => {
            action["kind"].as_str() == Some("start_run")
                && receipt.parameters
                    == serde_json::json!({
                        "ascension": 20,
                        "character": "IRONCLAD",
                        "seed": "0"
                    })
        }
        "neow_talk" => {
            action["kind"].as_str() == Some("choose_event_option")
                && action["event_id"].as_str() == Some("Neow")
                && action["option_index"].as_u64() == Some(0)
        }
        "neow_choose_three_potions" => {
            action["kind"].as_str() == Some("choose_event_option")
                && action["event_id"].as_str() == Some("Neow")
                && action["option_index"].as_u64() == Some(1)
        }
        "claim_first_elixir" => {
            action["kind"].as_str() == Some("claim_combat_reward")
                && action["reward_type"].as_str() == Some("POTION")
                && action["reward_id"].as_str() == Some("ElixirPotion")
                && action["reward_index"].as_u64() == Some(0)
        }
        "claim_block_potion" => {
            action["kind"].as_str() == Some("claim_combat_reward")
                && action["reward_type"].as_str() == Some("POTION")
                && action["reward_id"].as_str() == Some("Block Potion")
                && action["reward_index"].as_u64() == Some(1)
        }
        "close_neow_reward" => action["kind"].as_str() == Some("close_combat_reward"),
        "choose_first_monster_node" => {
            action["kind"].as_str() == Some("choose_map_node")
                && action["map_x"].as_i64() == Some(1)
                && action["map_y"].as_i64() == Some(0)
                && action["map_room_type"].as_str()
                    == Some("com.megacrit.cardcrawl.rooms.MonsterRoom")
        }
        "acknowledge_ftue" => action["kind"].as_str() == Some("acknowledge_ftue"),
        "play_first_base_strike" => {
            let strike_candidates = actions
                .iter()
                .filter(|candidate| {
                    candidate["kind"].as_str() == Some("play_card")
                        && candidate["card_id"].as_str() == Some("Strike_R")
                        && candidate["target_monster_index"].as_u64() == Some(0)
                })
                .collect::<Vec<_>>();
            let minimum_hand_index = strike_candidates
                .iter()
                .filter_map(|candidate| candidate["hand_index"].as_u64())
                .min();
            let minimum_occurrences = strike_candidates
                .iter()
                .filter(|candidate| candidate["hand_index"].as_u64() == minimum_hand_index)
                .count();
            action["kind"].as_str() == Some("play_card")
                && action["card_uuid"].as_str() == Some(&compared_action.card_uuid)
                && action["card_id"].as_str() == Some(&compared_action.card_id)
                && action["hand_index"].as_u64() == u64::try_from(compared_action.hand_index).ok()
                && action["target_monster_index"].as_u64()
                    == u64::try_from(compared_action.target_monster_index).ok()
                && action["target_monster_instance_id"].as_u64()
                    == Some(compared_action.target_monster_instance_id)
                && action["target_monster_id"].as_str() == Some(&compared_action.target_monster_id)
                && minimum_hand_index == u64::try_from(compared_action.hand_index).ok()
                && minimum_occurrences == 1
        }
        _ => false,
    };
    if !matches_recipe {
        return Err(format!(
            "natural step '{}' action does not match the closed typed recipe",
            proof.name
        ));
    }
    Ok(())
}

fn validate_natural_strike_comparison(
    trace: &ValidatedNaturalTrace,
    comparison: &JavaParityLiveComparisonReportV1,
) -> Result<(), String> {
    let strike = match comparison {
        JavaParityLiveComparisonReportV1::Strike(report) => report,
        _ => return Err("natural base Strike receipt selected a different comparator".to_owned()),
    };
    let scope = &strike.evidence_scope;
    let source = &strike.source;
    let transition = &strike.transition;
    if strike.status != "matched"
        || scope.natural_reachability
        || scope.strategy_evidence
        || scope.training_eligible
        || scope.constructed_root_verified
        || !scope.original_game_transition_captured
        || !scope.simulator_comparison
        || !scope.narrow_mechanics_parity
        || scope.strict_runtime_profile
        || source.transition_origin != "live_unclassified"
        || source.scenario_context.is_some()
        || source.session_id != trace.report.session.session_id
        || source.capture_id != trace.report.session.capture_id
        || source.session_manifest_sha256 != trace.session_manifest.sha256
        || source.action_receipt_sha256 != trace.action_receipt.sha256
        || source.before_frame_sha256 != trace.before_frame.sha256
        || source.after_frame_sha256 != trace.after_frame.sha256
        || !same_path(
            Path::new(&source.session_manifest_path),
            Path::new(&trace.session_manifest.path),
        )
        || !same_path(
            Path::new(&source.action_receipt_path),
            Path::new(&trace.action_receipt.path),
        )
        || !same_path(
            Path::new(&source.before_frame_path),
            Path::new(&trace.before_frame.path),
        )
        || !same_path(
            Path::new(&source.after_frame_path),
            Path::new(&trace.after_frame.path),
        )
        || transition.from_frame_id != trace.report.action.from_frame_id
        || transition.to_frame_id != trace.report.action.to_frame_id
        || transition.action_id != trace.report.action.action_id
        || transition.java_instance_uuid != trace.report.action.card_uuid
        || transition.java_card_id != trace.report.action.card_id
        || transition.target_monster_index != trace.report.action.target_monster_index
        || transition.target_monster_instance_id != trace.report.action.target_monster_instance_id
        || transition.target_monster_id != trace.report.action.target_monster_id
    {
        return Err(
            "base Strike comparison does not bind the exact natural trace without scope promotion"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_probe_file_identity(
    recorded: &ProbeFileIdentity,
) -> Result<ComparisonFileIdentityV1, String> {
    let actual = file_identity(Path::new(&recorded.path))?;
    if actual.bytes != recorded.bytes || actual.sha256 != recorded.sha256 {
        return Err(format!("natural probe input changed: {}", recorded.path));
    }
    Ok(actual)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode '{}': {error}", path.display()))
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fresh_test_directory() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sts-java-natural-trace-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn test_session() -> SessionManifest {
        SessionManifest {
            schema: "sts_java_parity_live_session_v3".to_owned(),
            session_id: "session".to_owned(),
            created_at_unix_ms: 1,
            process_id: 1,
            session_dir: "session".to_owned(),
            capture_id: "capture".to_owned(),
            producer_session_id: "producer".to_owned(),
            connection_id: "connection".to_owned(),
            profile: RUN_CONTROL_PROFILE.to_owned(),
            source_commit: "commit".to_owned(),
            source_dirty: false,
            loaded_mod_jar_sha256: "mod".to_owned(),
            sts_jar_sha256: "sts".to_owned(),
            mod_the_spire_jar_sha256: "mts".to_owned(),
            launch: None,
        }
    }

    #[test]
    fn natural_workflow_scope_is_not_strategy_or_training_evidence() {
        let scope = NaturalCombatWorkflowScopeV1 {
            natural_reachability: true,
            strategy_evidence: false,
            training_eligible: false,
            constructed_root_verified: false,
            original_game_navigation_captured: true,
            original_game_transition_captured: true,
            simulator_comparison: true,
            narrow_mechanics_parity: true,
            managed_lifecycle_verified: true,
            strict_runtime_profile: false,
        };
        assert!(scope.natural_reachability);
        assert!(!scope.strategy_evidence);
        assert!(!scope.training_eligible);
        assert!(!scope.constructed_root_verified);
    }

    #[test]
    fn natural_step_binds_receipt_successor_and_rejects_mutation() {
        let directory = fresh_test_directory();
        let frame_path = directory.join("frame-2.json");
        let receipt_path = directory.join("receipt.json");
        let frame = serde_json::json!({
            "frame_id": 2,
            "capture_id": "capture",
            "producer_session_id": "producer",
            "connection_id": "connection"
        });
        fs::write(&frame_path, serde_json::to_vec_pretty(&frame).unwrap()).unwrap();
        let receipt = ActionReceipt {
            schema: "sts_java_parity_live_action_receipt_v1".to_owned(),
            status: "completed".to_owned(),
            code: None,
            message: "completed".to_owned(),
            session_id: "session".to_owned(),
            connection_id: "connection".to_owned(),
            request_id: "request".to_owned(),
            based_on_frame_id: 1,
            action_id: "action".to_owned(),
            parameters: serde_json::json!({}),
            completed_at_unix_ms: 2,
            settlement_wall_time_ms: Some(1),
            producer_result: None,
            successor_frame_id: Some(2),
            successor_frame_path: Some(frame_path.to_string_lossy().into_owned()),
        };
        fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
        let receipt_identity = file_identity(&receipt_path).unwrap();
        let frame_identity = file_identity(&frame_path).unwrap();
        let proof = ProbeStepProof {
            name: "test".to_owned(),
            request_id: "request".to_owned(),
            action_id: "action".to_owned(),
            based_on_frame_id: 1,
            successor_frame_id: Some(2),
            status: "completed".to_owned(),
            settlement_wall_time_ms: Some(1),
            parameters: serde_json::json!({}),
            receipt_path: receipt_identity.path,
            receipt_sha256: receipt_identity.sha256,
            successor_frame_path: Some(frame_identity.path),
            successor_frame_sha256: Some(frame_identity.sha256),
        };
        validate_step(&test_session(), &proof, 1).unwrap();

        fs::write(&frame_path, b"mutated").unwrap();
        assert!(validate_step(&test_session(), &proof, 1).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
