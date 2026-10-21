use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sts_java_parity_protocol::{
    ActionRequest, ActionResult, CardBirthWitness, ExecutionInputWitness, Grants, Hello, HelloAck,
    LoadedModIdentity, ProtocolDescriptor, ProtocolError, RngCallWitness, StateFrame,
    CONTROL_PROFILE, RUN_CONTROL_PROFILE, SCENARIO_CONTROL_PROFILE,
};
use uuid::Uuid;

mod mirror;
mod probe;
mod scenario;
pub use mirror::{
    compile_mirror_plan, compile_mirror_plan_with_scope, compile_mirror_suffix_batch,
    java_seed_token_for_engine_seed, run_mirror_plan, run_mirror_suffix,
    validate_native_combat_checkpoint_v1, validate_resumed_native_checkpoint_root_v1,
    MirrorBoundaryStep, MirrorCoverage, MirrorExecutionEvidenceScope, MirrorExecutionReport,
    MirrorExecutionStep, MirrorFileIdentity, MirrorGap, MirrorLineageAudit, MirrorMismatch,
    MirrorPlan, MirrorPublicScalars, MirrorResumeEvidence, MirrorRunStart, MirrorScalarComparison,
    MirrorScalarField, MirrorScope, MirrorSiteActionCount, MirrorSource, MirrorStep,
    MirrorSuffixBatchEvidenceScope, MirrorSuffixBatchMember, MirrorSuffixBatchPlan,
    ValidatedNativeCombatCheckpointV1,
};
pub use probe::{
    bundle_elixir_hand_select_reports, run_elixir_hand_select_probe, run_natural_base_strike_probe,
    ElixirProbeVariant, NaturalCombatActionObservation, NaturalCombatActionReport,
    NaturalProbeBundle, NaturalProbeReport, ProbeCheck, ProbeFileIdentity, ProbeStepProof,
};
pub use scenario::{
    compile_scenario_case, compile_scenario_plan, compile_scenario_probe_case,
    compile_scenario_trajectory_case, load_scenario_plan, run_constructed_combat_probe,
    run_constructed_combat_scenario, run_constructed_combat_trajectory, run_scenario_reuse_probe,
    run_scenario_transition_reuse_probe, scenario_case_catalog, scenario_probe_case_catalog,
    scenario_registry, scenario_trajectory_case_catalog, validate_combat_scenario_recipe,
    CombatScenarioRecipe, ConstructedScenarioProbeReport, ConstructedScenarioReport,
    ConstructedScenarioTrajectoryReport, ScenarioCapability, ScenarioCapabilityDescriptor,
    ScenarioCardSpec, ScenarioCaseCatalog, ScenarioCaseCatalogEntry, ScenarioDeckSpec,
    ScenarioDraft, ScenarioDraftRecipe, ScenarioEncounterSpec, ScenarioEvidenceScope,
    ScenarioIntermediateBoundaryDescriptor, ScenarioPlan, ScenarioPlayerSpec,
    ScenarioProbeCardSelector, ScenarioProbeCaseCatalog, ScenarioProbeCaseCatalogEntry,
    ScenarioProbeDraft, ScenarioProbeEvidenceScope, ScenarioProbeMonsterSelector,
    ScenarioProbePlan, ScenarioProbeTransition, ScenarioRecipeDescriptor, ScenarioRegistry,
    ScenarioReuseProbeReport, ScenarioRunStart, ScenarioStepProof, ScenarioTrajectoryCaseCatalog,
    ScenarioTrajectoryCaseCatalogEntry, ScenarioTrajectoryDraft, ScenarioTrajectoryEvidenceScope,
    ScenarioTrajectoryFrameProof, ScenarioTrajectoryPlan, ScenarioTrajectoryStepReport,
    ScenarioTransitionDescriptor, ScenarioTransitionReuseProbeReport,
    SCENARIO_CONTENT_PROFILE_ALL_UNLOCKED, SCENARIO_PLAN_SCHEMA,
};

const CLIENT_NAME: &str = "sts_java_parity_live";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_PROTOCOL_LINE_BYTES: usize = 16 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const ACTION_RESULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionManifest {
    pub schema: String,
    pub session_id: String,
    pub created_at_unix_ms: u128,
    pub process_id: u32,
    pub session_dir: String,
    pub capture_id: String,
    pub producer_session_id: String,
    pub connection_id: String,
    pub profile: String,
    pub source_commit: String,
    pub source_dirty: bool,
    pub loaded_mod_jar_sha256: String,
    pub sts_jar_sha256: String,
    pub mod_the_spire_jar_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<LaunchEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchEvidence {
    pub profile: String,
    pub control_profile: String,
    pub plan_path: String,
    pub plan_sha256: String,
    #[serde(default)]
    pub storage_class: String,
    #[serde(default)]
    pub working_directory: String,
    #[serde(default)]
    pub source_game_directory: String,
    #[serde(default)]
    pub profile_substrate: String,
    #[serde(default)]
    pub tutorial_policy: String,
    pub isolation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientActionRequest {
    pub schema: String,
    pub session_id: String,
    pub connection_id: String,
    pub request_id: String,
    pub created_at_unix_ms: u128,
    pub based_on_frame_id: u64,
    pub action_id: String,
    #[serde(default = "empty_object")]
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReceipt {
    pub schema: String,
    pub status: String,
    pub code: Option<String>,
    pub message: String,
    pub session_id: String,
    pub connection_id: String,
    pub request_id: String,
    pub based_on_frame_id: u64,
    pub action_id: String,
    #[serde(default = "empty_object")]
    pub parameters: Value,
    pub completed_at_unix_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement_wall_time_ms: Option<u128>,
    pub producer_result: Option<Value>,
    pub successor_frame_id: Option<u64>,
    pub successor_frame_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResetReport {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub session_id: String,
    pub connection_id: String,
    pub from_frame_id: u64,
    pub scenario_context: Value,
    pub action_receipt_path: String,
    pub action_receipt_sha256: String,
    pub action: ActionReceipt,
    pub successor_frame_id: u64,
    pub successor_frame_path: String,
    pub successor_frame_sha256: String,
    pub fresh_main_menu_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunResetReport {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub session_id: String,
    pub connection_id: String,
    pub from_frame_id: u64,
    pub predecessor_run: Value,
    pub action_receipt_path: String,
    pub action_receipt_sha256: String,
    pub action: ActionReceipt,
    pub successor_frame_id: u64,
    pub successor_frame_path: String,
    pub successor_frame_sha256: String,
    pub preserved_save_main_menu_verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveStatus {
    pub manifest: SessionManifest,
    pub frame_path: String,
    pub frame: Value,
    pub actions: Vec<ActionHandle>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionHandle {
    pub handle: String,
    pub action_id: String,
    pub kind: String,
    pub card_uuid: Option<String>,
    pub card_id: Option<String>,
    pub card_upgrades: Option<usize>,
    pub card_ordinal: Option<usize>,
    pub hand_index: Option<usize>,
    pub potion_slot: Option<usize>,
    pub potion_id: Option<String>,
    pub potion_potency: Option<i32>,
    pub target_monster_id: Option<String>,
    pub target_monster_instance_id: Option<u64>,
    pub target_monster_index: Option<usize>,
    pub event_id: Option<String>,
    pub option_index: Option<usize>,
    pub option_text: Option<String>,
    pub neow_reward_type: Option<String>,
    pub neow_drawback: Option<String>,
    pub map_x: Option<i64>,
    pub map_y: Option<i64>,
    pub map_room_type: Option<String>,
    pub reward_type: Option<String>,
    pub reward_index: Option<usize>,
    pub reward_id: Option<String>,
    pub reward_gold: Option<i64>,
    pub shop_slot: Option<usize>,
    pub shop_price: Option<i64>,
    pub ftue_id: Option<String>,
    pub hand_select_id: Option<String>,
    pub hand_select_allowed_card_uuids: Vec<String>,
    pub hand_select_allowed_counts: Vec<usize>,
    pub grid_select_id: Option<String>,
    pub grid_select_source_pile: Option<String>,
    pub grid_select_reason: Option<String>,
    pub grid_select_allowed_card_uuids: Vec<String>,
    pub grid_select_allowed_counts: Vec<usize>,
    pub parameters_required: bool,
}

struct SessionPaths {
    session_dir: PathBuf,
    frames_dir: PathBuf,
    requests_dir: PathBuf,
    receipts_dir: PathBuf,
    transcript: PathBuf,
    manifest: PathBuf,
    ended: PathBuf,
}

struct PendingRequest {
    request: ClientActionRequest,
    receipt_path: PathBuf,
    sent_at: Instant,
    producer_result: Option<Value>,
    source_frame_identity: Option<Value>,
}

enum ProducerEvent {
    Line(String),
    Eof,
    Error(String),
}

pub fn serve(output_dir: &Path) -> Result<(), String> {
    let created_at = unix_ms()?;
    let session_id = Uuid::new_v4().to_string();
    let capture_id = Uuid::new_v4().to_string();
    let paths = create_session_paths(output_dir, created_at, &session_id)?;
    let mut transcript = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.transcript)
        .map_err(|error| format!("failed to create live transcript: {error}"))?;

    let (event_sender, events) = mpsc::channel();
    thread::spawn(move || read_producer(event_sender));
    let stdout = io::stdout();
    let mut protocol_output = stdout.lock();

    let control_profile = live_control_profile()?;
    let hello = Hello::new(CLIENT_NAME, CLIENT_VERSION, &capture_id, &control_profile)?;
    let hello_line = serde_json::to_string(&hello)
        .map_err(|error| format!("failed to encode control hello: {error}"))?;
    append_transcript(&mut transcript, &hello_line)?;
    write_protocol_line(&mut protocol_output, &hello_line)?;

    let ack_line = receive_initial_ack(&events)?;
    let ack: HelloAck = serde_json::from_str(&ack_line)
        .map_err(|error| format!("producer did not return a valid hello_ack: {error}"))?;
    validate_ack(&ack, &capture_id, &control_profile)?;
    append_transcript(&mut transcript, &ack_line)?;

    let launch = capture_launch_evidence(&control_profile, output_dir)?;
    if let Some(evidence) = &launch {
        validate_launch_loaded_mods(evidence, &ack.producer.loaded_mods)?;
    }
    let manifest = SessionManifest {
        schema: "sts_java_parity_live_session_v3".to_owned(),
        session_id: session_id.clone(),
        created_at_unix_ms: created_at,
        process_id: std::process::id(),
        session_dir: absolute_path(&paths.session_dir)?
            .to_string_lossy()
            .into_owned(),
        capture_id,
        producer_session_id: ack.producer_session_id.clone(),
        connection_id: ack.connection_id.clone(),
        profile: ack.profile.clone(),
        source_commit: ack.producer.source_commit.clone(),
        source_dirty: ack.producer.source_dirty,
        loaded_mod_jar_sha256: ack.producer.loaded_mod_jar_sha256.clone(),
        sts_jar_sha256: ack.producer.sts_jar_sha256.clone(),
        mod_the_spire_jar_sha256: ack.producer.mod_the_spire_jar_sha256.clone(),
        launch,
    };
    publish_json_create_new(&paths.manifest, &manifest)?;

    let loop_result = serve_loop(
        &events,
        &mut protocol_output,
        &mut transcript,
        &manifest,
        &ack,
        &paths,
    );
    transcript
        .sync_all()
        .map_err(|error| format!("failed to sync live transcript: {error}"))?;
    let ended = serde_json::json!({
        "schema": "sts_java_parity_live_end_v1",
        "session_id": manifest.session_id,
        "ended_at_unix_ms": unix_ms()?,
        "status": if loop_result.is_ok() { "closed" } else { "failed" },
        "error": loop_result.as_ref().err(),
        "transcript_sha256": sha256_file(&paths.transcript)?,
    });
    publish_json_create_new(&paths.ended, &ended)?;
    loop_result
}

fn serve_loop<W: Write>(
    events: &Receiver<ProducerEvent>,
    protocol_output: &mut W,
    transcript: &mut File,
    manifest: &SessionManifest,
    ack: &HelloAck,
    paths: &SessionPaths,
) -> Result<(), String> {
    let mut previous_frame_id = None;
    let mut previous_frame_identity = None;
    let mut pending: Option<PendingRequest> = None;
    let mut processed_requests = HashSet::new();
    let mut control_uncertain = false;

    loop {
        match events.recv_timeout(POLL_INTERVAL) {
            Ok(ProducerEvent::Line(line)) => {
                let value: Value = serde_json::from_str(&line)
                    .map_err(|error| format!("producer emitted invalid JSON: {error}"))?;
                let kind = value.get("kind").and_then(Value::as_str).unwrap_or("");
                match kind {
                    "state_frame" => {
                        let frame_id = match validate_frame_value(&value, ack, previous_frame_id) {
                            Ok(frame_id) => frame_id,
                            Err(reason) => {
                                save_rejected_frame(paths, &line, &reason)?;
                                return Err(reason);
                            }
                        };
                        let frame_identity = state_frame_settlement_identity(&value);
                        previous_frame_id = Some(frame_id);
                        append_transcript(transcript, &line)?;
                        let frame_path = frame_path(paths, frame_id);
                        publish_bytes_create_new(&frame_path, format!("{line}\n").as_bytes())?;
                        if pending.as_ref().is_some_and(|request| {
                            request.producer_result.is_some()
                                && request.source_frame_identity.as_ref() != Some(&frame_identity)
                        }) {
                            let request = pending.take().expect("accepted request exists");
                            complete_request(
                                manifest,
                                request,
                                "completed",
                                None,
                                "producer accepted the action and emitted its stable successor",
                                Some(frame_id),
                                Some(&frame_path),
                            )?;
                        }
                        previous_frame_identity = Some(frame_identity);
                    }
                    "action_result" => {
                        let result: ActionResult =
                            serde_json::from_value(value.clone()).map_err(|error| {
                                format!("producer emitted invalid action_result: {error}")
                            })?;
                        validate_action_result(&result, ack, pending.as_ref())?;
                        append_transcript(transcript, &line)?;
                        if result.status == "accepted" {
                            pending
                                .as_mut()
                                .expect("validated pending request")
                                .producer_result = Some(value);
                        } else {
                            pending
                                .as_mut()
                                .expect("validated pending request")
                                .producer_result = Some(value);
                            let request = pending.take().expect("validated pending request");
                            let receipt_status = if result.status == "unknown" {
                                control_uncertain = true;
                                "unknown"
                            } else {
                                "rejected"
                            };
                            complete_request(
                                manifest,
                                request,
                                receipt_status,
                                result.code.clone(),
                                &result.message,
                                None,
                                None,
                            )?;
                        }
                    }
                    "protocol_error" => {
                        let error: ProtocolError =
                            serde_json::from_value(value.clone()).map_err(|parse| {
                                format!("producer emitted invalid protocol_error: {parse}")
                            })?;
                        append_transcript(transcript, &line)?;
                        if let Some(mut request) = pending.take() {
                            request.producer_result = Some(value);
                            complete_request(
                                manifest,
                                request,
                                "rejected",
                                Some(error.code),
                                &error.message,
                                None,
                                None,
                            )?;
                        }
                    }
                    _ => return Err(format!("unexpected producer message kind: {kind}")),
                }
            }
            Ok(ProducerEvent::Eof) => return Ok(()),
            Ok(ProducerEvent::Error(error)) => return Err(error),
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => {}
        }

        if let Some(request) = pending.as_ref() {
            if request.sent_at.elapsed() >= ACTION_RESULT_TIMEOUT {
                let request = pending.take().expect("timed-out request exists");
                complete_request(
                    manifest,
                    request,
                    "unknown",
                    Some("producer_response_timeout".to_owned()),
                    "the action outcome is uncertain; control is frozen for this session",
                    None,
                    None,
                )?;
                control_uncertain = true;
            }
        }

        if pending.is_none() {
            if let Some((path, request)) = next_request(paths, &mut processed_requests)? {
                let receipt_path = receipt_path(paths, &request.request_id);
                if let Some((code, message)) = validate_client_request(
                    &request,
                    manifest,
                    previous_frame_id,
                    control_uncertain,
                ) {
                    let pending_request = PendingRequest {
                        request,
                        receipt_path,
                        sent_at: Instant::now(),
                        producer_result: None,
                        source_frame_identity: previous_frame_identity.clone(),
                    };
                    complete_request(
                        manifest,
                        pending_request,
                        "rejected",
                        Some(code),
                        &message,
                        None,
                        None,
                    )?;
                    continue;
                }
                let wire = ActionRequest::new(
                    &request.connection_id,
                    &request.request_id,
                    request.based_on_frame_id,
                    &request.action_id,
                )
                .with_parameters(request.parameters.clone())?;
                let line = serde_json::to_string(&wire)
                    .map_err(|error| format!("failed to encode action request: {error}"))?;
                append_transcript(transcript, &line)?;
                write_protocol_line(protocol_output, &line)?;
                pending = Some(PendingRequest {
                    request,
                    receipt_path,
                    sent_at: Instant::now(),
                    producer_result: None,
                    source_frame_identity: previous_frame_identity.clone(),
                });
                let _ = path;
            }
        }
    }
}

fn state_frame_settlement_identity(frame: &Value) -> Value {
    serde_json::json!({
        "boundary_kind": frame["boundary"]["kind"],
        "public_base": frame["public_base"],
        "parity_internal": frame["parity_internal"],
        "rng_internal": frame["rng_internal"],
        "action_space": frame["action_space"],
        "scenario": frame["scenario"],
    })
}

pub fn status(output_dir: &Path) -> Result<LiveStatus, String> {
    let (manifest, paths) = discover_live_session(output_dir)?;
    let (frame_path, frame_value) = latest_frame(&paths)?;
    let frame_id = frame_value["frame_id"]
        .as_u64()
        .ok_or_else(|| "latest frame has no numeric frame_id".to_owned())?;
    let actions = frame_value["action_space"]["actions"]
        .as_array()
        .ok_or_else(|| "latest control frame has no action_space.actions array".to_owned())?
        .iter()
        .enumerate()
        .map(|(index, action)| action_handle(frame_id, index, action))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LiveStatus {
        manifest,
        frame_path: frame_path.to_string_lossy().into_owned(),
        frame: frame_value,
        actions,
    })
}

pub fn render_status(view: &LiveStatus) -> String {
    let frame_id = view.frame["frame_id"].as_u64().unwrap_or(0);
    let boundary_kind = view.frame["boundary"]["kind"].as_str().unwrap_or("unknown");
    let combat = &view.frame["public_base"]["combat"];
    let launch_profile = view
        .manifest
        .launch
        .as_ref()
        .map(|launch| launch.profile.as_str())
        .unwrap_or("unrecorded");
    let mut lines = if combat.is_object() {
        let player = &combat["player"];
        vec![format!(
            "session={} launch={} control={} frame={} boundary={} turn={} hp={}/{} block={} energy={}",
            view.manifest.session_id,
            launch_profile,
            view.manifest.profile,
            frame_id,
            boundary_kind,
            combat["turn"].as_i64().unwrap_or_default(),
            player["current_hp"].as_i64().unwrap_or_default(),
            player["max_hp"].as_i64().unwrap_or_default(),
            player["block"].as_i64().unwrap_or_default(),
            player["energy"].as_i64().unwrap_or_default(),
        )]
    } else {
        let run = &view.frame["public_base"]["run"];
        let location = if run.is_object() {
            format!(
                " character={} ascension={} act={} floor={} hp={}/{} gold={}",
                run["character"].as_str().unwrap_or("unknown"),
                run["ascension"].as_i64().unwrap_or_default(),
                run["act"].as_i64().unwrap_or_default(),
                run["floor"].as_i64().unwrap_or_default(),
                run["current_hp"].as_i64().unwrap_or_default(),
                run["max_hp"].as_i64().unwrap_or_default(),
                run["gold"].as_i64().unwrap_or_default(),
            )
        } else {
            String::new()
        };
        vec![format!(
            "session={} launch={} control={} frame={} boundary={}{}",
            view.manifest.session_id,
            launch_profile,
            view.manifest.profile,
            frame_id,
            boundary_kind,
            location,
        )]
    };
    for action in &view.actions {
        let detail = match action.kind.as_str() {
            "play_card" => match (&action.card_id, &action.target_monster_id) {
                (Some(card), Some(target)) => format!("{card} -> {target}"),
                (Some(card), None) => card.clone(),
                _ => "play_card".to_owned(),
            },
            "use_potion" => {
                let target = action
                    .target_monster_instance_id
                    .map(|instance| format!(" -> monster:{instance}"))
                    .unwrap_or_default();
                format!(
                    "slot={} id={} potency={}{}",
                    action.potion_slot.unwrap_or_default(),
                    action.potion_id.as_deref().unwrap_or("unknown"),
                    action.potion_potency.unwrap_or_default(),
                    target
                )
            }
            "discard_potion" => format!(
                "slot={} id={}",
                action.potion_slot.unwrap_or_default(),
                action.potion_id.as_deref().unwrap_or("unknown")
            ),
            "start_run" => "use start-run with character/ascension[/seed]".to_owned(),
            "choose_event_option" => format!(
                "option={} {}",
                action.option_index.unwrap_or_default(),
                action.option_text.as_deref().unwrap_or("")
            ),
            "choose_map_node" => format!(
                "node=({}, {}) {}",
                action.map_x.unwrap_or_default(),
                action.map_y.unwrap_or_default(),
                action.map_room_type.as_deref().unwrap_or("unknown")
            ),
            "claim_combat_reward" => format!(
                "reward={} index={} id={} gold={}",
                action.reward_type.as_deref().unwrap_or("unknown"),
                action.reward_index.unwrap_or_default(),
                action.reward_id.as_deref().unwrap_or("unknown"),
                action
                    .reward_gold
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "n/a".to_owned())
            ),
            "choose_boss_relic" => format!(
                "index={} id={}",
                action.reward_index.unwrap_or_default(),
                action.reward_id.as_deref().unwrap_or("unknown")
            ),
            "buy_shop_relic" => format!(
                "slot={} id={} price={}",
                action.shop_slot.unwrap_or_default(),
                action.reward_id.as_deref().unwrap_or("unknown"),
                action.shop_price.unwrap_or_default()
            ),
            "buy_shop_potion" => format!(
                "slot={} id={} price={}",
                action.shop_slot.unwrap_or_default(),
                action.potion_id.as_deref().unwrap_or("unknown"),
                action.shop_price.unwrap_or_default()
            ),
            "acknowledge_ftue" => format!(
                "tutorial={}",
                action.ftue_id.as_deref().unwrap_or("unknown")
            ),
            "submit_hand_select" => format!(
                "allowed_counts={:?} cards={}; use hand-select [--card-uuid UUID ...]",
                action.hand_select_allowed_counts,
                action.hand_select_allowed_card_uuids.len()
            ),
            "submit_grid_select" => format!(
                "source={} reason={} allowed_counts={:?} cards={}; use grid-select --card-uuid UUID",
                action.grid_select_source_pile.as_deref().unwrap_or("unknown"),
                action.grid_select_reason.as_deref().unwrap_or("unknown"),
                action.grid_select_allowed_counts,
                action.grid_select_allowed_card_uuids.len()
            ),
            "scenario_create_combat" => {
                "use java-play scenario create-combat with a typed draft".to_owned()
            }
            "scenario_reset_to_main_menu" => {
                "use the dedicated typed scenario reset command".to_owned()
            }
            "run_reset_to_main_menu_preserving_save" => {
                "use the dedicated typed natural-run reset command".to_owned()
            }
            "run_discard_to_main_menu" => {
                "use the dedicated typed managed-run discard command".to_owned()
            }
            _ => action.kind.clone(),
        };
        lines.push(format!("{}  {}  {}", action.handle, action.kind, detail));
    }
    if boundary_kind == "run_decision"
        && view.frame["public_base"]["decision"]["kind"].as_str() == Some("hand_select")
    {
        if let Some(cards) =
            view.frame["public_base"]["decision"]["hand_select"]["candidate_cards"].as_array()
        {
            for card in cards {
                lines.push(format!(
                    "  hand[{}] uuid={} id={} name={} upgrades={} cost_for_turn={}",
                    card["index"].as_u64().unwrap_or_default(),
                    card["card_uuid"].as_str().unwrap_or("unknown"),
                    card["card_id"].as_str().unwrap_or("unknown"),
                    card["name"].as_str().unwrap_or("unknown"),
                    card["upgrades"].as_i64().unwrap_or_default(),
                    card["cost_for_turn"].as_i64().unwrap_or_default(),
                ));
            }
        }
    }
    if boundary_kind == "unsupported_run_decision" && view.actions.is_empty() {
        lines.push(format!(
            "no authorized actions; unsupported_choice={}",
            view.frame["public_base"]["decision"]["choice_type"]
                .as_str()
                .unwrap_or("unknown")
        ));
    }
    lines.join("\n")
}

pub fn act(output_dir: &Path, selector: &str, timeout: Duration) -> Result<ActionReceipt, String> {
    let view = status(output_dir)?;
    act_on_observation(&view, selector, timeout)
}

/// Submit against one exact previously observed session and frame.
///
/// Unlike [`act`], this does not rediscover the latest session through an
/// output directory. Lifecycle owners use it to ensure a concurrent session
/// replacement can only reject the old request, never redirect an action to a
/// different runtime.
pub fn act_on_observation(
    view: &LiveStatus,
    selector: &str,
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    let frame_id = view.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "latest frame has no numeric frame_id".to_owned())?;
    let action = resolve_selector(&view.actions, frame_id, selector)?;
    if action.parameters_required {
        let command = match action.kind.as_str() {
            "start_run" => "start-run",
            "continue_run" => "continue-run",
            "submit_hand_select" => "hand-select",
            "submit_grid_select" => "grid-select",
            "scenario_create_combat" => "java-play scenario create-combat",
            _ => "the dedicated structured command",
        };
        return Err(format!(
            "this action requires structured parameters; use {command}"
        ));
    }
    submit_action(view, action, empty_object(), timeout)
}

pub fn start_run(
    output_dir: &Path,
    character: &str,
    ascension: u32,
    seed: Option<&str>,
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    start_run_with_content_profile(
        output_dir,
        character,
        ascension,
        seed,
        "current_profile",
        timeout,
    )
}

pub fn start_run_with_content_profile(
    output_dir: &Path,
    character: &str,
    ascension: u32,
    seed: Option<&str>,
    content_profile: &str,
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    let character = character.to_ascii_uppercase();
    if !["IRONCLAD", "THE_SILENT", "DEFECT", "WATCHER"].contains(&character.as_str()) {
        return Err("character must be IRONCLAD, THE_SILENT, DEFECT, or WATCHER".to_owned());
    }
    if ascension > 20 {
        return Err("ascension must be from 0 through 20".to_owned());
    }
    let seed = seed.map(str::to_ascii_uppercase);
    if seed.as_ref().is_some_and(|value| {
        value.is_empty() || !value.chars().all(|ch| ch.is_ascii_alphanumeric())
    }) {
        return Err("seed must contain only A-Z and 0-9".to_owned());
    }
    if !["current_profile", "all_unlocked"].contains(&content_profile) {
        return Err("content profile must be current_profile or all_unlocked".to_owned());
    }
    let view = status(output_dir)?;
    let mut starts = view
        .actions
        .iter()
        .filter(|action| action.kind == "start_run");
    let action = starts
        .next()
        .ok_or_else(|| "latest frame is not a main-menu start decision".to_owned())?;
    if starts.next().is_some() {
        return Err("latest frame contains more than one start_run action".to_owned());
    }
    let mut parameters = serde_json::Map::new();
    parameters.insert("character".to_owned(), Value::String(character));
    parameters.insert("ascension".to_owned(), Value::from(ascension));
    parameters.insert(
        "content_profile".to_owned(),
        Value::String(content_profile.to_owned()),
    );
    if let Some(seed) = seed {
        parameters.insert("seed".to_owned(), Value::String(seed));
    }
    submit_action(&view, action, Value::Object(parameters), timeout)
}

pub fn continue_run_with_content_profile(
    output_dir: &Path,
    content_profile: &str,
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    if !["current_profile", "all_unlocked"].contains(&content_profile) {
        return Err("content profile must be current_profile or all_unlocked".to_owned());
    }
    let view = status(output_dir)?;
    let mut actions = view
        .actions
        .iter()
        .filter(|action| action.kind == "continue_run" && action.action_id == "continue_run");
    let action = actions
        .next()
        .ok_or_else(|| "latest frame is not a main-menu Continue decision".to_owned())?;
    if actions.next().is_some() || !action.parameters_required {
        return Err(
            "latest frame does not contain one content-profile-bound continue_run action"
                .to_owned(),
        );
    }
    submit_action(
        &view,
        action,
        serde_json::json!({"content_profile": content_profile}),
        timeout,
    )
}

pub fn submit_hand_select(
    output_dir: &Path,
    selected_card_uuids: &[String],
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    let view = status(output_dir)?;
    let mut actions = view
        .actions
        .iter()
        .filter(|action| action.kind == "submit_hand_select");
    let action = actions
        .next()
        .ok_or_else(|| "latest frame is not an authorized HAND_SELECT decision".to_owned())?;
    if actions.next().is_some() {
        return Err("latest frame contains more than one submit_hand_select action".to_owned());
    }

    submit_hand_select_on_observation(&view, &action.handle, selected_card_uuids, timeout)
}

/// Submit one typed HAND_SELECT choice against the exact observed frame.
///
/// Lifecycle owners use this variant so a session replacement can reject a
/// stale request, never redirect it to a newly discovered session.
pub fn submit_hand_select_on_observation(
    view: &LiveStatus,
    selector: &str,
    selected_card_uuids: &[String],
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    let frame_id = view.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "latest frame has no numeric frame_id".to_owned())?;
    let action = resolve_selector(&view.actions, frame_id, selector)?;
    if action.kind != "submit_hand_select" {
        return Err("selected action is not a typed HAND_SELECT submission".to_owned());
    }
    let parameters = hand_select_parameters(action, selected_card_uuids)?;
    submit_action(view, action, parameters, timeout)
}

pub fn submit_grid_select(
    output_dir: &Path,
    selected_card_uuids: &[String],
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    let view = status(output_dir)?;
    let mut actions = view
        .actions
        .iter()
        .filter(|action| action.kind == "submit_grid_select");
    let action = actions
        .next()
        .ok_or_else(|| "latest frame is not an authorized GRID_SELECT decision".to_owned())?;
    if actions.next().is_some() {
        return Err("latest frame contains more than one submit_grid_select action".to_owned());
    }

    submit_grid_select_on_observation(&view, &action.handle, selected_card_uuids, timeout)
}

/// Submit one typed GRID_SELECT choice against the exact observed frame.
///
/// Lifecycle owners use this variant so a session replacement can reject a
/// stale request, never redirect it to a newly discovered session.
pub fn submit_grid_select_on_observation(
    view: &LiveStatus,
    selector: &str,
    selected_card_uuids: &[String],
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    let frame_id = view.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "latest frame has no numeric frame_id".to_owned())?;
    let action = resolve_selector(&view.actions, frame_id, selector)?;
    if action.kind != "submit_grid_select" {
        return Err("selected action is not a typed GRID_SELECT submission".to_owned());
    }
    let parameters = grid_select_parameters(action, selected_card_uuids)?;
    submit_action(view, action, parameters, timeout)
}

pub fn reset_scenario_to_main_menu(
    output_dir: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<ScenarioResetReport, String> {
    let created_at_unix_ms = unix_ms()?;
    let absolute_report_path = absolute_path(report_path)?;
    if absolute_report_path.exists() {
        return Err(format!(
            "scenario reset report path must be create-new: {}",
            absolute_report_path.display()
        ));
    }
    let report_parent = absolute_report_path.parent().ok_or_else(|| {
        format!(
            "scenario reset report path has no parent: {}",
            absolute_report_path.display()
        )
    })?;
    if !report_parent.is_dir() {
        return Err(format!(
            "scenario reset report parent does not exist: {}",
            report_parent.display()
        ));
    }
    let before = status(output_dir)?;
    if before.manifest.profile != SCENARIO_CONTROL_PROFILE
        || before.frame["boundary"]["kind"].as_str() != Some("combat_player_decision")
        || before.frame["scenario"].is_null()
    {
        return Err("scenario reset requires a constructed scenario combat decision".to_owned());
    }
    let from_frame_id = before.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "scenario reset predecessor has no frame ID".to_owned())?;
    let scenario_context = before.frame["scenario"].clone();
    let mut actions = before
        .actions
        .iter()
        .filter(|action| action.kind == "scenario_reset_to_main_menu");
    let action = actions
        .next()
        .ok_or_else(|| "constructed combat omitted its typed scenario reset action".to_owned())?;
    if actions.next().is_some()
        || action.action_id != "scenario_reset_to_main_menu"
        || action.parameters_required
    {
        return Err("scenario reset action is not uniquely and exactly typed".to_owned());
    }
    let receipt = submit_action(&before, action, empty_object(), timeout)?;
    let successor_frame_id = receipt
        .successor_frame_id
        .ok_or_else(|| "scenario reset receipt omitted its stable successor".to_owned())?;
    let successor_frame_path = receipt
        .successor_frame_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "scenario reset receipt omitted its successor path".to_owned())?;
    if receipt.status != "completed"
        || receipt.code.is_some()
        || receipt.based_on_frame_id != from_frame_id
        || receipt.action_id != "scenario_reset_to_main_menu"
        || receipt.parameters != empty_object()
    {
        return Err("scenario reset did not produce an exact completed receipt".to_owned());
    }
    let after = status(output_dir)?;
    let after_frame_id = after.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "scenario reset successor has no frame ID".to_owned())?;
    let fresh_main_menu_verified = after.manifest.session_id == before.manifest.session_id
        && after.manifest.connection_id == before.manifest.connection_id
        && after_frame_id == successor_frame_id
        && after.frame_path == absolute_path(&successor_frame_path)?.display().to_string()
        && after.frame["boundary"]["kind"].as_str() == Some("main_menu_ready")
        && after.frame["public_base"]["decision"]["kind"].as_str() == Some("main_menu_ready")
        && after.frame["public_base"]["run"].is_null()
        && after.frame["scenario"].is_null()
        && after.frame["parity_internal"]["run_seed"].is_null()
        && after.frame["rng_internal"]["streams"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && after.actions.len() == 1
        && after.actions[0].kind == "start_run"
        && after.actions[0].action_id == "start_run"
        && after.actions[0].parameters_required;
    if !fresh_main_menu_verified {
        return Err("scenario reset successor is not a proven fresh main-menu epoch".to_owned());
    }
    let paths = session_paths_from_manifest(&before.manifest);
    let action_receipt_path = receipt_path(&paths, &receipt.request_id);
    let report = ScenarioResetReport {
        schema: "sts_java_parity_scenario_reset_v1".to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms,
        completed_at_unix_ms: unix_ms()?,
        passed: true,
        session_id: before.manifest.session_id,
        connection_id: before.manifest.connection_id,
        from_frame_id,
        scenario_context,
        action_receipt_path: absolute_path(&action_receipt_path)?.display().to_string(),
        action_receipt_sha256: sha256_file(&action_receipt_path)?,
        action: receipt,
        successor_frame_id,
        successor_frame_path: absolute_path(&successor_frame_path)?.display().to_string(),
        successor_frame_sha256: sha256_file(&successor_frame_path)?,
        fresh_main_menu_verified,
    };
    publish_json_create_new(report_path, &report)?;
    Ok(report)
}

pub fn reset_natural_run_to_main_menu_preserving_save(
    output_dir: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<NaturalRunResetReport, String> {
    let created_at_unix_ms = unix_ms()?;
    let absolute_report_path = absolute_path(report_path)?;
    if absolute_report_path.exists() {
        return Err(format!(
            "natural run reset report path must be create-new: {}",
            absolute_report_path.display()
        ));
    }
    let report_parent = absolute_report_path.parent().ok_or_else(|| {
        format!(
            "natural run reset report path has no parent: {}",
            absolute_report_path.display()
        )
    })?;
    if !report_parent.is_dir() {
        return Err(format!(
            "natural run reset report parent does not exist: {}",
            report_parent.display()
        ));
    }

    let before = status(output_dir)?;
    if before.manifest.profile != RUN_CONTROL_PROFILE
        || !matches!(
            before.frame["boundary"]["kind"].as_str(),
            Some("run_decision" | "combat_player_decision")
        )
        || !before.frame["scenario"].is_null()
        || !before.frame["public_base"]["run"].is_object()
    {
        return Err("natural run reset requires a stable natural run decision".to_owned());
    }
    let from_frame_id = before.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "natural run reset predecessor has no frame ID".to_owned())?;
    let predecessor_run = before.frame["public_base"]["run"].clone();
    let mut actions = before
        .actions
        .iter()
        .filter(|action| action.kind == "run_reset_to_main_menu_preserving_save");
    let action = actions.next().ok_or_else(|| {
        "natural run decision omitted its typed save-preserving reset action".to_owned()
    })?;
    if actions.next().is_some()
        || action.action_id != "run_reset_to_main_menu_preserving_save"
        || action.parameters_required
    {
        return Err("natural run reset action is not uniquely and exactly typed".to_owned());
    }

    let receipt = submit_action(&before, action, empty_object(), timeout)?;
    let successor_frame_id = receipt
        .successor_frame_id
        .ok_or_else(|| "natural run reset receipt omitted its stable successor".to_owned())?;
    let successor_frame_path = receipt
        .successor_frame_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "natural run reset receipt omitted its successor path".to_owned())?;
    if receipt.status != "completed"
        || receipt.code.is_some()
        || receipt.based_on_frame_id != from_frame_id
        || receipt.action_id != "run_reset_to_main_menu_preserving_save"
        || receipt.parameters != empty_object()
    {
        return Err("natural run reset did not produce an exact completed receipt".to_owned());
    }

    let after = status(output_dir)?;
    let after_frame_id = after.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "natural run reset successor has no frame ID".to_owned())?;
    let preserved_save_main_menu_verified = after.manifest.session_id == before.manifest.session_id
        && after.manifest.connection_id == before.manifest.connection_id
        && after_frame_id == successor_frame_id
        && after.frame_path == absolute_path(&successor_frame_path)?.display().to_string()
        && after.frame["boundary"]["kind"].as_str() == Some("main_menu_ready")
        && after.frame["public_base"]["decision"]["kind"].as_str() == Some("main_menu_ready")
        && after.frame["public_base"]["run"].is_null()
        && after.frame["scenario"].is_null()
        && after.frame["parity_internal"]["run_seed"].is_null()
        && after.frame["rng_internal"]["streams"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && after.actions.len() == 1
        && after.actions[0].kind == "continue_run"
        && after.actions[0].action_id == "continue_run"
        && after.actions[0].parameters_required;
    if !preserved_save_main_menu_verified {
        return Err(
            "natural run reset successor is not a proven save-preserving Continue epoch".to_owned(),
        );
    }

    let paths = session_paths_from_manifest(&before.manifest);
    let action_receipt_path = receipt_path(&paths, &receipt.request_id);
    let report = NaturalRunResetReport {
        schema: "sts_java_parity_natural_run_reset_v1".to_owned(),
        provenance: "natural_native_checkpoint".to_owned(),
        created_at_unix_ms,
        completed_at_unix_ms: unix_ms()?,
        passed: true,
        session_id: before.manifest.session_id,
        connection_id: before.manifest.connection_id,
        from_frame_id,
        predecessor_run,
        action_receipt_path: absolute_path(&action_receipt_path)?.display().to_string(),
        action_receipt_sha256: sha256_file(&action_receipt_path)?,
        action: receipt,
        successor_frame_id,
        successor_frame_path: absolute_path(&successor_frame_path)?.display().to_string(),
        successor_frame_sha256: sha256_file(&successor_frame_path)?,
        preserved_save_main_menu_verified,
    };
    publish_json_create_new(report_path, &report)?;
    Ok(report)
}

fn hand_select_parameters(
    action: &ActionHandle,
    selected_card_uuids: &[String],
) -> Result<Value, String> {
    let mut canonical = Vec::new();
    let mut unique = HashSet::new();
    for raw in selected_card_uuids {
        let uuid = Uuid::parse_str(raw)
            .map_err(|_| format!("HAND_SELECT card UUID is invalid: {raw}"))?
            .to_string();
        if !unique.insert(uuid.clone()) {
            return Err(format!("HAND_SELECT card UUID is duplicated: {uuid}"));
        }
        if !action.hand_select_allowed_card_uuids.contains(&uuid) {
            return Err(format!(
                "HAND_SELECT card UUID is not in the current typed candidate set: {uuid}"
            ));
        }
        canonical.push(uuid);
    }
    if !action.hand_select_allowed_counts.contains(&canonical.len()) {
        return Err(format!(
            "HAND_SELECT selected {} cards; allowed counts are {:?}",
            canonical.len(),
            action.hand_select_allowed_counts
        ));
    }
    Ok(serde_json::json!({"selected_card_uuids": canonical}))
}

fn grid_select_parameters(
    action: &ActionHandle,
    selected_card_uuids: &[String],
) -> Result<Value, String> {
    let mut canonical = Vec::new();
    let mut unique = HashSet::new();
    for raw in selected_card_uuids {
        let uuid = Uuid::parse_str(raw)
            .map_err(|_| format!("GRID_SELECT card UUID is invalid: {raw}"))?
            .to_string();
        if !unique.insert(uuid.clone()) {
            return Err(format!("GRID_SELECT card UUID is duplicated: {uuid}"));
        }
        if !action.grid_select_allowed_card_uuids.contains(&uuid) {
            return Err(format!(
                "GRID_SELECT card UUID is not in the current typed candidate set: {uuid}"
            ));
        }
        canonical.push(uuid);
    }
    if !action.grid_select_allowed_counts.contains(&canonical.len()) {
        return Err(format!(
            "GRID_SELECT selected {} cards; allowed counts are {:?}",
            canonical.len(),
            action.grid_select_allowed_counts
        ));
    }
    Ok(serde_json::json!({"selected_card_uuids": canonical}))
}

fn submit_action(
    view: &LiveStatus,
    action: &ActionHandle,
    parameters: Value,
    timeout: Duration,
) -> Result<ActionReceipt, String> {
    let frame_id = view.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "latest frame has no numeric frame_id".to_owned())?;
    let request_id = Uuid::new_v4().to_string();
    let request = ClientActionRequest {
        schema: "sts_java_parity_live_action_request_v2".to_owned(),
        session_id: view.manifest.session_id.clone(),
        connection_id: view.manifest.connection_id.clone(),
        request_id: request_id.clone(),
        created_at_unix_ms: unix_ms()?,
        based_on_frame_id: frame_id,
        action_id: action.action_id.clone(),
        parameters,
    };
    let paths = session_paths_from_manifest(&view.manifest);
    let request_path = paths.requests_dir.join(format!("{request_id}.json"));
    let receipt_path = receipt_path(&paths, &request_id);
    publish_json_create_new(&request_path, &request)?;

    let started = Instant::now();
    loop {
        if receipt_path.is_file() {
            return read_json(&receipt_path);
        }
        if paths.ended.is_file() {
            let ended: Value = read_json(&paths.ended)?;
            let status = ended["status"].as_str().unwrap_or("unknown");
            let detail = ended["error"]
                .as_str()
                .filter(|value| !value.is_empty())
                .unwrap_or("the producer session closed without a detailed error");
            return Err(format!(
                "live session ended with status {status} before action receipt {}: {detail}; request remains durable at {}",
                receipt_path.display(),
                request_path.display()
            ));
        }
        if started.elapsed() >= timeout {
            return Err(format!(
                "timed out waiting for action receipt {}; request remains durable at {}",
                receipt_path.display(),
                request_path.display()
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn create_session_paths(
    output_dir: &Path,
    created_at: u128,
    session_id: &str,
) -> Result<SessionPaths, String> {
    fs::create_dir_all(output_dir)
        .map_err(|error| format!("failed to create {}: {error}", output_dir.display()))?;
    let output_dir = absolute_path(output_dir)?;
    let session_dir = output_dir.join(format!(
        "session-{created_at}-{}",
        session_id.replace('-', "")
    ));
    fs::create_dir(&session_dir)
        .map_err(|error| format!("failed to create {}: {error}", session_dir.display()))?;
    let frames_dir = session_dir.join("frames");
    let requests_dir = session_dir.join("requests");
    let receipts_dir = session_dir.join("receipts");
    for directory in [&frames_dir, &requests_dir, &receipts_dir] {
        fs::create_dir(directory)
            .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
    }
    Ok(SessionPaths {
        transcript: session_dir.join("transcript.jsonl"),
        manifest: session_dir.join("session.json"),
        ended: session_dir.join("ended.json"),
        session_dir,
        frames_dir,
        requests_dir,
        receipts_dir,
    })
}

fn session_paths_from_manifest(manifest: &SessionManifest) -> SessionPaths {
    let session_dir = PathBuf::from(&manifest.session_dir);
    SessionPaths {
        transcript: session_dir.join("transcript.jsonl"),
        manifest: session_dir.join("session.json"),
        ended: session_dir.join("ended.json"),
        frames_dir: session_dir.join("frames"),
        requests_dir: session_dir.join("requests"),
        receipts_dir: session_dir.join("receipts"),
        session_dir,
    }
}

fn receive_initial_ack(events: &Receiver<ProducerEvent>) -> Result<String, String> {
    match events.recv_timeout(HELLO_TIMEOUT) {
        Ok(ProducerEvent::Line(line)) => Ok(line),
        Ok(ProducerEvent::Eof) | Err(RecvTimeoutError::Disconnected) => {
            Err("producer closed before hello_ack".to_owned())
        }
        Ok(ProducerEvent::Error(error)) => Err(error),
        Err(RecvTimeoutError::Timeout) => Err("timed out waiting for hello_ack".to_owned()),
    }
}

fn validate_ack(ack: &HelloAck, capture_id: &str, profile: &str) -> Result<(), String> {
    if ack.kind != "hello_ack" || ack.capture_id != capture_id || ack.profile != profile {
        return Err("hello_ack does not match the requested live control session".to_owned());
    }
    ack.protocol.validate()?;
    ack.grants.validate_for(profile)?;
    parse_uuid("producer_session_id", &ack.producer_session_id)?;
    parse_uuid("connection_id", &ack.connection_id)?;
    Ok(())
}

fn validate_frame_value(
    value: &Value,
    ack: &HelloAck,
    previous_frame_id: Option<u64>,
) -> Result<u64, String> {
    if ack.profile == CONTROL_PROFILE {
        let frame: StateFrame = serde_json::from_value(value.clone())
            .map_err(|error| format!("producer emitted invalid combat state_frame: {error}"))?;
        validate_control_frame(&frame, ack, previous_frame_id)?;
        return Ok(frame.frame_id);
    }
    if ack.profile == RUN_CONTROL_PROFILE {
        let frame: RunControlFrame = serde_json::from_value(value.clone())
            .map_err(|error| format!("producer emitted invalid run state_frame: {error}"))?;
        validate_run_control_frame(&frame, ack, previous_frame_id)?;
        return Ok(frame.frame_id);
    }
    if ack.profile == SCENARIO_CONTROL_PROFILE {
        let frame: RunControlFrame = serde_json::from_value(value.clone())
            .map_err(|error| format!("producer emitted invalid scenario state_frame: {error}"))?;
        validate_scenario_control_frame(&frame, ack, previous_frame_id)?;
        return Ok(frame.frame_id);
    }
    Err(format!("unsupported live control profile: {}", ack.profile))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunControlFrame {
    kind: String,
    protocol: ProtocolDescriptor,
    producer_session_id: String,
    connection_id: String,
    capture_id: String,
    frame_id: u64,
    previous_frame_id: Option<u64>,
    profile: String,
    grants: Grants,
    boundary: RunControlBoundary,
    public_base: Value,
    parity_internal: Value,
    rng_internal: Value,
    action_space: GenericActionSpace,
    #[serde(default)]
    rng_call_witness: Option<RngCallWitness>,
    #[serde(default)]
    execution_input_witness: Option<ExecutionInputWitness>,
    #[serde(default)]
    card_birth_witness: Option<CardBirthWitness>,
    #[serde(default)]
    scenario: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunControlBoundary {
    kind: String,
    stable: bool,
    capture_point: String,
    evidence: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenericActionSpace {
    kind: String,
    actions: Vec<Value>,
}

fn validate_card_birth_witness_envelope(frame: &RunControlFrame) -> Result<(), String> {
    let Some(witness) = &frame.card_birth_witness else {
        return Ok(());
    };
    if witness.kind != "card_birth_witness_v1"
        || Some(witness.from_frame_id) != frame.previous_frame_id
        || witness.to_frame_id != frame.frame_id
    {
        return Err(
            "card-birth witness does not bind the adjacent live frame transition".to_owned(),
        );
    }
    Ok(())
}

fn validate_rng_call_witness_envelope(frame: &RunControlFrame) -> Result<(), String> {
    if let Some(witness) = &frame.execution_input_witness {
        if witness.kind != "execution_input_witness_v1"
            || Some(witness.from_frame_id) != frame.previous_frame_id
            || witness.to_frame_id != frame.frame_id
        {
            return Err(
                "execution-input witness does not bind the adjacent live frame transition"
                    .to_owned(),
            );
        }
    }
    let Some(witness) = &frame.rng_call_witness else {
        return Ok(());
    };
    if witness.kind != "rng_call_witness_v1"
        || Some(witness.from_frame_id) != frame.previous_frame_id
        || witness.to_frame_id != frame.frame_id
    {
        return Err("RNG-call witness does not bind the adjacent live frame transition".to_owned());
    }
    Ok(())
}

fn validate_run_control_frame(
    frame: &RunControlFrame,
    ack: &HelloAck,
    previous_frame_id: Option<u64>,
) -> Result<(), String> {
    if frame.kind != "state_frame"
        || frame.producer_session_id != ack.producer_session_id
        || frame.connection_id != ack.connection_id
        || frame.capture_id != ack.capture_id
        || frame.profile != RUN_CONTROL_PROFILE
        || frame.previous_frame_id != previous_frame_id
        || frame.frame_id != previous_frame_id.unwrap_or(0) + 1
    {
        return Err(
            "run state_frame identity or sequence does not match the live session".to_owned(),
        );
    }
    frame.protocol.validate()?;
    frame.grants.validate_for(RUN_CONTROL_PROFILE)?;
    validate_rng_call_witness_envelope(frame)?;
    validate_card_birth_witness_envelope(frame)?;
    if !frame.boundary.stable
        || ![
            "main_menu_ready",
            "run_decision",
            "combat_player_decision",
            "unsupported_run_decision",
            "terminal",
        ]
        .contains(&frame.boundary.kind.as_str())
        || frame.boundary.capture_point.is_empty()
        || !frame.boundary.evidence.is_object()
        || !frame.public_base.is_object()
        || !frame.parity_internal.is_object()
        || !frame.rng_internal.is_object()
    {
        return Err("run state_frame boundary or observation payload is invalid".to_owned());
    }
    validate_run_decision_surface(frame)
}

fn validate_scenario_control_frame(
    frame: &RunControlFrame,
    ack: &HelloAck,
    previous_frame_id: Option<u64>,
) -> Result<(), String> {
    if frame.kind != "state_frame"
        || frame.producer_session_id != ack.producer_session_id
        || frame.connection_id != ack.connection_id
        || frame.capture_id != ack.capture_id
        || frame.profile != SCENARIO_CONTROL_PROFILE
        || frame.previous_frame_id != previous_frame_id
        || frame.frame_id != previous_frame_id.unwrap_or(0) + 1
    {
        return Err(
            "scenario state_frame identity or sequence does not match the live session".to_owned(),
        );
    }
    frame.protocol.validate()?;
    frame.grants.validate_for(SCENARIO_CONTROL_PROFILE)?;
    validate_rng_call_witness_envelope(frame)?;
    validate_card_birth_witness_envelope(frame)?;
    if !frame.boundary.stable
        || ![
            "main_menu_ready",
            "run_decision",
            "combat_player_decision",
            "unsupported_run_decision",
        ]
        .contains(&frame.boundary.kind.as_str())
        || frame.boundary.capture_point.is_empty()
        || !frame.boundary.evidence.is_object()
        || !frame.public_base.is_object()
        || !frame.parity_internal.is_object()
        || !frame.rng_internal.is_object()
    {
        return Err("scenario state_frame boundary or observation payload is invalid".to_owned());
    }
    validate_scenario_decision_surface(frame)
}

fn validate_scenario_decision_surface(frame: &RunControlFrame) -> Result<(), String> {
    if let Some(context) = &frame.scenario {
        validate_scenario_context(context)?;
    }
    let expected_space = if frame.boundary.kind == "combat_player_decision" {
        "combat_action_space"
    } else {
        "run_action_space"
    };
    if frame.action_space.kind != expected_space {
        return Err("scenario state_frame action space does not match its boundary".to_owned());
    }
    let decision_kind = frame.public_base["decision"]["kind"].as_str();
    if frame.boundary.kind == "unsupported_run_decision" {
        if !frame.action_space.actions.is_empty() || decision_kind.is_none() {
            return Err(
                "unsupported scenario decision must retain typed context and no authorized actions"
                    .to_owned(),
            );
        }
        return Ok(());
    }
    if frame.boundary.kind == "combat_player_decision" && frame.scenario.is_none() {
        return Err("scenario combat frame omitted constructed provenance context".to_owned());
    }
    if frame.action_space.actions.is_empty() {
        return Err("authorized scenario decision has an empty action space".to_owned());
    }
    if decision_kind == Some("hand_select") && frame.action_space.actions.len() != 1 {
        return Err("scenario HAND_SELECT must expose exactly one atomic submit action".to_owned());
    }

    let mut ids = BTreeSet::new();
    for action in &frame.action_space.actions {
        let action_id = action["action_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "scenario control action has no action_id".to_owned())?;
        let kind = action["kind"]
            .as_str()
            .ok_or_else(|| "scenario control action has no kind".to_owned())?;
        if !ids.insert(action_id) {
            return Err("scenario control action IDs are duplicated".to_owned());
        }
        let valid = match frame.boundary.kind.as_str() {
            "main_menu_ready" => {
                kind == "start_run"
                    && action_id == "start_run"
                    && action["parameter_schema"].is_object()
            }
            "run_decision" if decision_kind == Some("ftue") => kind == "acknowledge_ftue",
            "run_decision" if decision_kind == Some("hand_select") => {
                validate_hand_select_action(&frame.public_base["decision"], action)?;
                kind == "submit_hand_select"
            }
            "run_decision" if decision_kind == Some("event") => {
                validate_scenario_create_combat_action(frame, action)?;
                true
            }
            "run_decision" if decision_kind == Some("combat_reward") => {
                kind == "claim_combat_reward" || kind == "close_combat_reward"
            }
            "combat_player_decision" => {
                kind == "play_card"
                    || kind == "use_potion"
                    || kind == "end_turn"
                    || validate_scenario_reset_action(action)?
            }
            _ => false,
        };
        if !valid {
            return Err(
                "scenario control action kind does not match its typed decision".to_owned(),
            );
        }
    }
    Ok(())
}

fn validate_scenario_reset_action(action: &Value) -> Result<bool, String> {
    if action["kind"].as_str() != Some("scenario_reset_to_main_menu") {
        return Ok(false);
    }
    if action["action_id"].as_str() != Some("scenario_reset_to_main_menu")
        || action["parameter_schema"].is_object()
    {
        return Err("scenario reset action is not an exact parameter-free action".to_owned());
    }
    Ok(true)
}

fn validate_natural_run_reset_action(action: &Value) -> Result<bool, String> {
    if action["kind"].as_str() != Some("run_reset_to_main_menu_preserving_save") {
        return Ok(false);
    }
    if action["action_id"].as_str() != Some("run_reset_to_main_menu_preserving_save")
        || action["parameter_schema"].is_object()
    {
        return Err("natural run reset action is not an exact parameter-free action".to_owned());
    }
    Ok(true)
}

fn validate_natural_run_discard_action(action: &Value) -> Result<bool, String> {
    if action["kind"].as_str() != Some("run_discard_to_main_menu") {
        return Ok(false);
    }
    if action["action_id"].as_str() != Some("run_discard_to_main_menu")
        || action.as_object().is_none_or(|fields| {
            fields.iter().any(|(key, value)| {
                !matches!(key.as_str(), "action_id" | "kind") && !value.is_null()
            })
        })
    {
        return Err("natural run discard action is not an exact parameter-free action".to_owned());
    }
    Ok(true)
}

fn validate_scenario_context(context: &Value) -> Result<(), String> {
    scenario::validate_scenario_context_value(context)
}

fn validate_scenario_create_combat_action(
    frame: &RunControlFrame,
    action: &Value,
) -> Result<(), String> {
    let event_id = validate_supported_event_identity(&frame.public_base["decision"])?;
    if frame.action_space.actions.len() != 1
        || frame.scenario.is_some()
        || event_id != "Neow"
        || frame.public_base["run"]["act"].as_i64() != Some(1)
        || frame.public_base["run"]["floor"].as_i64() != Some(0)
        || action["action_id"].as_str() != Some("scenario_create_combat")
        || action["kind"].as_str() != Some("scenario_create_combat")
    {
        return Err(
            "scenario_create_combat is not uniquely bound to the initial Neow decision".to_owned(),
        );
    }
    scenario::validate_scenario_parameter_schema_value(&action["parameter_schema"])
}

fn validate_supported_event_identity(decision: &Value) -> Result<&str, String> {
    let event_id = decision["event_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "event decision omitted its stable identity".to_owned())?;
    let identity = &decision["event_identity"];
    if identity["status"].as_str() != Some("supported")
        || identity["event_id"].as_str() != Some(event_id)
        || identity["runtime_class"]
            .as_str()
            .filter(|value| !value.is_empty())
            .is_none()
        || !identity["unsupported_reason"].is_null()
    {
        return Err("event decision has an invalid producer-owned identity".to_owned());
    }
    Ok(event_id)
}

fn validate_run_decision_surface(frame: &RunControlFrame) -> Result<(), String> {
    let expected_space = if frame.boundary.kind == "combat_player_decision" {
        "combat_action_space"
    } else {
        "run_action_space"
    };
    if frame.action_space.kind != expected_space {
        return Err("run state_frame action space does not match its boundary".to_owned());
    }
    let decision_kind = frame.public_base["decision"]["kind"].as_str();
    if frame.boundary.kind == "terminal" {
        let run = &frame.public_base["run"];
        let management_only = match frame.action_space.actions.as_slice() {
            [] => true,
            [action] => validate_natural_run_discard_action(action)?,
            _ => false,
        };
        if decision_kind != Some("heart_victory")
            || run["act"].as_i64() != Some(4)
            || run["has_ruby_key"].as_bool() != Some(true)
            || run["has_sapphire_key"].as_bool() != Some(true)
            || run["has_emerald_key"].as_bool() != Some(true)
            || !management_only
        {
            return Err(
                "terminal run boundary requires an Act 4 three-key Heart victory with only optional managed discard"
                    .to_owned(),
            );
        }
        return Ok(());
    }
    if frame.boundary.kind == "unsupported_run_decision" {
        let choice_type = frame.public_base["decision"]["choice_type"]
            .as_str()
            .filter(|value| !value.is_empty());
        if decision_kind != Some("unsupported")
            || choice_type.is_none()
            || choice_type.is_some_and(|value| {
                [
                    "NONE",
                    "EVENT",
                    "MAP",
                    "COMBAT_REWARD",
                    "CARD_REWARD",
                    "BOSS_REWARD",
                    "SHOP_ROOM",
                    "SHOP_SCREEN",
                    "CHEST",
                    "HAND_SELECT",
                ]
                .contains(&value)
            })
            || !frame.action_space.actions.is_empty()
        {
            return Err(
                "unsupported run decision must expose one typed choice and no authorized actions"
                    .to_owned(),
            );
        }
        return Ok(());
    }
    if frame.action_space.actions.is_empty() {
        return Err("authorized run decision has an empty action space".to_owned());
    }
    let primary_actions = frame
        .action_space
        .actions
        .iter()
        .filter(|action| match action["kind"].as_str() {
            Some("use_potion") => frame.boundary.kind != "run_decision",
            Some(
                "discard_potion"
                | "run_reset_to_main_menu_preserving_save"
                | "run_discard_to_main_menu",
            ) => false,
            _ => true,
        })
        .cloned()
        .collect::<Vec<_>>();
    if primary_actions.is_empty() {
        return Err("authorized run decision has no decision-owning action".to_owned());
    }
    if frame.boundary.kind == "main_menu_ready" && primary_actions.len() != 1 {
        return Err("main menu must expose exactly one start-or-continue action".to_owned());
    }
    if decision_kind == Some("hand_select") && primary_actions.len() != 1 {
        return Err("HAND_SELECT must expose exactly one atomic submit action".to_owned());
    }
    if decision_kind == Some("grid_select") && primary_actions.len() != 1 {
        return Err("GRID_SELECT must expose exactly one atomic submit action".to_owned());
    }
    if decision_kind == Some("shop_entry")
        && (primary_actions
            .iter()
            .filter(|a| a["kind"] == "enter_shop")
            .count()
            != 1
            || primary_actions
                .iter()
                .filter(|a| a["kind"] == "proceed_shop_room")
                .count()
                != 1
            || !(2..=3).contains(&primary_actions.len()))
    {
        return Err("SHOP_ROOM must expose exactly one enter and one proceed action".to_owned());
    }
    if decision_kind == Some("treasure") && primary_actions.len() != 2 {
        return Err("treasure must expose exactly one open and one proceed action".to_owned());
    }
    if [
        Some("smith_confirm"),
        Some("shop_purge_confirm"),
        Some("campfire_complete"),
        Some("boss_treasure"),
        Some("boss_treasure_complete"),
        Some("double_boss_complete"),
        Some("act3_victory_complete"),
        Some("true_victory_complete"),
    ]
    .contains(&decision_kind)
        && primary_actions.len() != 1
    {
        return Err("single-action run transition must expose exactly one typed action".to_owned());
    }
    if decision_kind == Some("campfire") && !(1..=4).contains(&primary_actions.len()) {
        return Err("campfire must expose one to four distinct native options".to_owned());
    }
    if decision_kind == Some("card_reward") {
        validate_card_reward_actions(&frame.public_base["decision"], &primary_actions)?;
    }
    if decision_kind == Some("boss_relic") {
        validate_boss_relic_actions(&primary_actions)?;
    }
    if decision_kind == Some("event") {
        validate_event_actions(&frame.public_base["decision"], &primary_actions)?;
    }
    let mut ids = BTreeSet::new();
    let mut discarded_slots = BTreeSet::new();
    let mut used_potion_slots = BTreeSet::new();
    let mut map_coordinates = BTreeSet::new();
    let mut saw_close_map = false;
    for action in &frame.action_space.actions {
        let action_id = action["action_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "run control action has no action_id".to_owned())?;
        let kind = action["kind"]
            .as_str()
            .ok_or_else(|| "run control action has no kind".to_owned())?;
        if !ids.insert(action_id) {
            return Err("run control action IDs are duplicated".to_owned());
        }
        let valid = if kind == "run_reset_to_main_menu_preserving_save" {
            matches!(
                frame.boundary.kind.as_str(),
                "run_decision" | "combat_player_decision"
            ) && validate_natural_run_reset_action(action)?
        } else if kind == "run_discard_to_main_menu" {
            matches!(
                frame.boundary.kind.as_str(),
                "run_decision" | "combat_player_decision"
            ) && validate_natural_run_discard_action(action)?
        } else if kind == "use_potion" && frame.boundary.kind == "run_decision" {
            validate_run_potion_action(action, "use_potion", &mut used_potion_slots)?
        } else if kind == "discard_potion" {
            matches!(
                frame.boundary.kind.as_str(),
                "run_decision" | "combat_player_decision"
            ) && validate_run_potion_action(action, "discard_potion", &mut discarded_slots)?
        } else {
            match frame.boundary.kind.as_str() {
                "main_menu_ready" => {
                    (kind == "start_run"
                        && action_id == "start_run"
                        && action["parameter_schema"].is_object())
                        || (kind == "continue_run"
                            && action_id == "continue_run"
                            && action["parameter_schema"].is_object())
                }
                "run_decision" if decision_kind == Some("event") => kind == "choose_event_option",
                "run_decision" if decision_kind == Some("map") => {
                    validate_map_action(action, &mut map_coordinates, &mut saw_close_map)?
                }
                "run_decision" if decision_kind == Some("combat_reward") => {
                    kind == "claim_combat_reward"
                        || kind == "close_combat_reward"
                        || kind == "close_shop_rewards"
                            && action_id == "close_shop_rewards"
                            && frame.parity_internal["shop"].is_object()
                            && frame.parity_internal["combat_rewards"].is_array()
                }
                "run_decision" if decision_kind == Some("card_reward") => {
                    kind == "choose_card_reward"
                        || kind == "skip_card_reward"
                        || kind == "take_singing_bowl"
                }
                "run_decision" if decision_kind == Some("boss_relic") => {
                    kind == "choose_boss_relic" || kind == "skip_boss_relic"
                }
                "run_decision" if decision_kind == Some("shop_entry") => {
                    (kind == "enter_shop" && action_id == "enter_shop")
                        || (kind == "proceed_shop_room" && action_id == "proceed_shop_room")
                        || validate_shop_reward_reopen_action(frame, action)
                }
                "run_decision" if decision_kind == Some("shop") => {
                    if kind == "open_shop_rewards" {
                        validate_shop_reward_reopen_action(frame, action)
                    } else {
                        validate_shop_action(action)?;
                        kind == "buy_shop_card"
                            || kind == "buy_shop_relic"
                            || kind == "buy_shop_potion"
                            || kind == "begin_shop_purge"
                            || kind == "leave_shop"
                    }
                }
                "run_decision" if decision_kind == Some("shop_purge_card") => {
                    validate_shop_purge_card_action(
                        action,
                        "choose_shop_purge_card",
                        "shop_purge_card",
                    )?
                }
                "run_decision" if decision_kind == Some("shop_purge_confirm") => {
                    validate_shop_purge_card_action(
                        action,
                        "confirm_shop_purge_card",
                        "confirm_shop_purge_card",
                    )?
                }
                "run_decision" if decision_kind == Some("campfire") => {
                    (kind == "begin_campfire_rest" && action_id == "begin_campfire_rest")
                        || (kind == "begin_campfire_recall" && action_id == "begin_campfire_recall")
                        || (kind == "begin_campfire_smith" && action_id == "begin_campfire_smith")
                        || (kind == "begin_campfire_dig" && action_id == "begin_campfire_dig")
                }
                "run_decision" if decision_kind == Some("smith_card") => {
                    validate_smith_action(action, "choose_smith_card", "smith_card")?
                }
                "run_decision" if decision_kind == Some("smith_confirm") => {
                    validate_smith_action(action, "confirm_smith_card", "confirm_smith_card")?
                }
                "run_decision" if decision_kind == Some("campfire_complete") => {
                    kind == "proceed_rest_room" && action_id == "proceed_rest_room"
                }
                "run_decision" if decision_kind == Some("treasure") => {
                    (kind == "open_chest" && action_id == "open_chest")
                        || (kind == "proceed_treasure" && action_id == "proceed_treasure")
                }
                "run_decision" if decision_kind == Some("boss_treasure") => {
                    kind == "open_boss_chest" && action_id == "open_boss_chest"
                }
                "run_decision" if decision_kind == Some("boss_treasure_complete") => {
                    kind == "proceed_boss_treasure" && action_id == "proceed_boss_treasure"
                }
                "run_decision" if decision_kind == Some("double_boss_complete") => {
                    kind == "proceed_double_boss" && action_id == "proceed_double_boss"
                }
                "run_decision" if decision_kind == Some("act3_victory_complete") => {
                    kind == "proceed_act3_victory" && action_id == "proceed_act3_victory"
                }
                "run_decision" if decision_kind == Some("true_victory_complete") => {
                    kind == "proceed_true_victory" && action_id == "proceed_true_victory"
                }
                "run_decision" if decision_kind == Some("ftue") => kind == "acknowledge_ftue",
                "run_decision" if decision_kind == Some("hand_select") => {
                    validate_hand_select_action(&frame.public_base["decision"], action)?;
                    kind == "submit_hand_select"
                }
                "run_decision" if decision_kind == Some("grid_select") => {
                    validate_grid_select_action(&frame.public_base["decision"], action)?;
                    kind == "submit_grid_select"
                }
                "combat_player_decision" => {
                    kind == "play_card" || kind == "use_potion" || kind == "end_turn"
                }
                _ => false,
            }
        };
        if !valid {
            return Err("run control action kind does not match its typed decision".to_owned());
        }
    }
    Ok(())
}

fn validate_event_actions(decision: &Value, actions: &[Value]) -> Result<(), String> {
    let event_id = validate_supported_event_identity(decision)?;
    let mut option_indices = BTreeSet::new();
    for action in actions {
        let option_index = action["option_index"]
            .as_u64()
            .ok_or_else(|| "event action omitted its option index".to_owned())?;
        if action["event_id"].as_str() != Some(event_id) || !option_indices.insert(option_index) {
            return Err("event action changed identity or repeated an option index".to_owned());
        }
    }
    validate_neow_event_actions(decision, actions)
}

fn validate_neow_event_actions(decision: &Value, actions: &[Value]) -> Result<(), String> {
    if decision["event_id"].as_str() != Some("Neow") {
        return Ok(());
    }
    let screen = decision["event_screen_index"]
        .as_i64()
        .ok_or_else(|| "Neow decision omitted its screen index".to_owned())?;
    if screen != 3 {
        if actions.iter().any(|action| {
            !action["neow_reward_type"].is_null() || !action["neow_drawback"].is_null()
        }) {
            return Err("non-choice Neow action exposed a reward identity".to_owned());
        }
        return Ok(());
    }
    if !matches!(actions.len(), 2 | 4) {
        return Err("Neow choice screen must expose exactly two or four rewards".to_owned());
    }
    let reward_types = [
        "THREE_ENEMY_KILL",
        "TEN_PERCENT_HP_BONUS",
        "THREE_CARDS",
        "ONE_RANDOM_RARE_CARD",
        "REMOVE_CARD",
        "UPGRADE_CARD",
        "TRANSFORM_CARD",
        "RANDOM_COLORLESS",
        "THREE_SMALL_POTIONS",
        "RANDOM_COMMON_RELIC",
        "HUNDRED_GOLD",
        "RANDOM_COLORLESS_2",
        "REMOVE_TWO",
        "ONE_RARE_RELIC",
        "THREE_RARE_CARDS",
        "TWO_FIFTY_GOLD",
        "TRANSFORM_TWO_CARDS",
        "TWENTY_PERCENT_HP_BONUS",
        "BOSS_RELIC",
    ];
    let drawbacks = [
        "NONE",
        "TEN_PERCENT_HP_LOSS",
        "NO_GOLD",
        "CURSE",
        "PERCENT_DAMAGE",
    ];
    let mut identities = BTreeSet::new();
    let mut option_indices = BTreeSet::new();
    for action in actions {
        if action["kind"].as_str() != Some("choose_event_option")
            || action["event_id"].as_str() != Some("Neow")
        {
            return Err("Neow choice action belongs to another decision surface".to_owned());
        }
        let option_index = action["option_index"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < actions.len())
            .ok_or_else(|| "Neow choice omitted an in-range option index".to_owned())?;
        if !option_indices.insert(option_index) {
            return Err("Neow choice repeats an option index".to_owned());
        }
        let reward = action["neow_reward_type"]
            .as_str()
            .filter(|value| reward_types.contains(value))
            .ok_or_else(|| "Neow choice omitted a recognized reward identity".to_owned())?;
        let drawback = action["neow_drawback"]
            .as_str()
            .filter(|value| drawbacks.contains(value))
            .ok_or_else(|| "Neow choice omitted a recognized drawback identity".to_owned())?;
        if !identities.insert((reward, drawback)) {
            return Err("Neow choice repeats a reward/drawback identity".to_owned());
        }
    }
    Ok(())
}

fn validate_map_action(
    action: &Value,
    map_coordinates: &mut BTreeSet<(i64, i64)>,
    saw_close_map: &mut bool,
) -> Result<bool, String> {
    if action["kind"].as_str() == Some("close_map") {
        if *saw_close_map
            || action["action_id"].as_str() != Some("close_map")
            || !action["parameter_schema"].is_null()
            || !action["event_id"].is_null()
            || !action["option_index"].is_null()
            || !action["neow_reward_type"].is_null()
            || !action["neow_drawback"].is_null()
            || !action["map_x"].is_null()
            || !action["map_y"].is_null()
            || !action["map_room_type"].is_null()
            || !action["reward_type"].is_null()
            || !action["reward_index"].is_null()
            || !action["reward_id"].is_null()
            || !action["reward_gold"].is_null()
        {
            return Err("map close action is not one exact parameter-free action".to_owned());
        }
        *saw_close_map = true;
        return Ok(true);
    }
    if action["kind"].as_str() != Some("choose_map_node") {
        return Ok(false);
    }
    let x = action["map_x"]
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or_else(|| "map action omitted its non-negative x coordinate".to_owned())?;
    let y = action["map_y"]
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or_else(|| "map action omitted its non-negative y coordinate".to_owned())?;
    let _room_type = action["map_room_type"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "map action omitted its room type".to_owned())?;
    if !map_coordinates.insert((x, y)) {
        return Err("map action space exposed the same coordinate more than once".to_owned());
    }
    if action["action_id"].as_str() != Some(format!("map_node:{x}:{y}").as_str())
        || !action["parameter_schema"].is_null()
    {
        return Err("map action does not bind one parameter-free typed node".to_owned());
    }
    Ok(true)
}

fn validate_run_potion_action(
    action: &Value,
    kind: &str,
    slots: &mut BTreeSet<u64>,
) -> Result<bool, String> {
    let slot = action["potion_slot"]
        .as_u64()
        .ok_or_else(|| "run potion action omitted its non-negative slot".to_owned())?;
    let potion_id = action["potion_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "run potion action omitted its potion ID".to_owned())?;
    if !slots.insert(slot) {
        return Err("run potion action exposed the same slot more than once".to_owned());
    }
    let expected_action_id = format!("{kind}:{slot}:{potion_id}");
    if action["action_id"].as_str() != Some(expected_action_id.as_str())
        || !action["parameter_schema"].is_null()
        || !action["target_monster_instance_id"].is_null()
        || !action["target_monster_id"].is_null()
    {
        return Err("run potion action does not bind one parameter-free typed potion".to_owned());
    }
    Ok(true)
}

fn validate_shop_reward_reopen_action(frame: &RunControlFrame, action: &Value) -> bool {
    action["kind"] == "open_shop_rewards"
        && action["action_id"] == "open_shop_rewards"
        && frame.parity_internal["shop"].is_object()
        && frame.parity_internal["combat_rewards"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
}

fn validate_shop_action(action: &Value) -> Result<(), String> {
    let kind = action["kind"].as_str().unwrap_or_default();
    if kind == "leave_shop" {
        return if action["action_id"].as_str() == Some("leave_shop") {
            Ok(())
        } else {
            Err("SHOP_SCREEN leave action has the wrong identity".to_owned())
        };
    }
    if kind == "buy_shop_relic" {
        let slot = action["shop_slot"]
            .as_u64()
            .ok_or_else(|| "SHOP_SCREEN relic action omitted its non-negative slot".to_owned())?;
        let price = action["shop_price"]
            .as_i64()
            .filter(|value| *value >= 0)
            .ok_or_else(|| "SHOP_SCREEN relic action omitted its non-negative price".to_owned())?;
        let relic_id = action["reward_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "SHOP_SCREEN relic action omitted its relic ID".to_owned())?;
        let expected_action_id = format!("shop_relic:{slot}:{relic_id}:{price}");
        if action["reward_type"].as_str() != Some("SHOP_RELIC")
            || action["action_id"].as_str() != Some(expected_action_id.as_str())
        {
            return Err("SHOP_SCREEN relic action does not bind its typed merchandise".to_owned());
        }
        return Ok(());
    }
    if kind == "buy_shop_potion" {
        let slot = action["shop_slot"]
            .as_u64()
            .ok_or_else(|| "SHOP_SCREEN potion action omitted its non-negative slot".to_owned())?;
        let price = action["shop_price"]
            .as_i64()
            .filter(|value| *value >= 0)
            .ok_or_else(|| "SHOP_SCREEN potion action omitted its non-negative price".to_owned())?;
        let potion_id = action["potion_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "SHOP_SCREEN potion action omitted its potion ID".to_owned())?;
        let expected_action_id = format!("shop_potion:{slot}:{potion_id}:{price}");
        if action["reward_type"].as_str() != Some("SHOP_POTION")
            || action["action_id"].as_str() != Some(expected_action_id.as_str())
        {
            return Err("SHOP_SCREEN potion action does not bind its typed merchandise".to_owned());
        }
        return Ok(());
    }
    if kind == "begin_shop_purge" {
        let price = action["shop_price"]
            .as_i64()
            .filter(|value| *value >= 0)
            .ok_or_else(|| "SHOP_SCREEN purge action omitted its non-negative price".to_owned())?;
        let expected_action_id = format!("begin_shop_purge:{price}");
        return if action["action_id"].as_str() == Some(expected_action_id.as_str()) {
            Ok(())
        } else {
            Err("SHOP_SCREEN purge action does not bind its typed price".to_owned())
        };
    }
    if kind != "buy_shop_card" {
        return Ok(());
    }

    let slot = action["shop_slot"]
        .as_u64()
        .ok_or_else(|| "SHOP_SCREEN card action omitted its non-negative slot".to_owned())?;
    let price = action["shop_price"]
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or_else(|| "SHOP_SCREEN card action omitted its non-negative price".to_owned())?;
    let card_uuid = action["card_uuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "SHOP_SCREEN card action omitted its Java UUID".to_owned())?;
    let card_id = action["card_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "SHOP_SCREEN card action omitted its card ID".to_owned())?;
    let upgrades = action["card_upgrades"]
        .as_u64()
        .ok_or_else(|| "SHOP_SCREEN card action omitted its upgrade count".to_owned())?;
    let expected_action_id = format!("shop_card:{slot}:{card_uuid}:{card_id}:{upgrades}:{price}");
    if action["action_id"].as_str() != Some(expected_action_id.as_str()) {
        return Err("SHOP_SCREEN card action does not bind its typed merchandise".to_owned());
    }
    Ok(())
}

fn validate_shop_purge_card_action(
    action: &Value,
    expected_kind: &str,
    action_id_prefix: &str,
) -> Result<bool, String> {
    if action["kind"].as_str() != Some(expected_kind) {
        return Ok(false);
    }
    let ordinal = action["card_ordinal"]
        .as_u64()
        .ok_or_else(|| "shop purge action omitted its master-deck ordinal".to_owned())?;
    let card_uuid = action["card_uuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "shop purge action omitted its Java UUID".to_owned())?;
    let card_id = action["card_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "shop purge action omitted its card ID".to_owned())?;
    let upgrades = action["card_upgrades"]
        .as_u64()
        .ok_or_else(|| "shop purge action omitted its upgrade count".to_owned())?;
    let expected_action_id =
        format!("{action_id_prefix}:{ordinal}:{card_uuid}:{card_id}:{upgrades}");
    if action["action_id"].as_str() != Some(expected_action_id.as_str()) {
        return Err("shop purge action does not bind its typed card".to_owned());
    }
    Ok(true)
}

fn validate_smith_action(
    action: &Value,
    expected_kind: &str,
    action_id_prefix: &str,
) -> Result<bool, String> {
    if action["kind"].as_str() != Some(expected_kind) {
        return Ok(false);
    }
    let ordinal = action["card_ordinal"]
        .as_u64()
        .ok_or_else(|| "campfire smith action omitted its master-deck ordinal".to_owned())?;
    let card_uuid = action["card_uuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "campfire smith action omitted its Java UUID".to_owned())?;
    let card_id = action["card_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "campfire smith action omitted its card ID".to_owned())?;
    let upgrades = action["card_upgrades"]
        .as_u64()
        .ok_or_else(|| "campfire smith action omitted its upgrade count".to_owned())?;
    let expected_action_id =
        format!("{action_id_prefix}:{ordinal}:{card_uuid}:{card_id}:{upgrades}");
    if action["action_id"].as_str() != Some(expected_action_id.as_str()) {
        return Err("campfire smith action does not bind its typed card".to_owned());
    }
    Ok(true)
}

fn validate_card_reward_actions(decision: &Value, actions: &[Value]) -> Result<(), String> {
    let reward = decision["card_reward"]
        .as_object()
        .ok_or_else(|| "CARD_REWARD decision omitted typed card_reward state".to_owned())?;
    let source_kind = reward
        .get("source_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "CARD_REWARD source_kind is not a string".to_owned())?;
    if !matches!(
        source_kind,
        "ordinary_reward" | "neow_reward" | "combat_discovery" | "choose_one" | "draft" | "codex"
    ) {
        return Err(format!(
            "CARD_REWARD source_kind {source_kind:?} is not recognized"
        ));
    }
    let cards = reward
        .get("cards")
        .and_then(Value::as_array)
        .ok_or_else(|| "CARD_REWARD cards is not an array".to_owned())?;
    let skip_available = reward
        .get("skip_available")
        .and_then(Value::as_bool)
        .ok_or_else(|| "CARD_REWARD skip_available is not boolean".to_owned())?;
    let singing_bowl_available = reward
        .get("singing_bowl_available")
        .and_then(Value::as_bool)
        .ok_or_else(|| "CARD_REWARD singing_bowl_available is not boolean".to_owned())?;
    let singing_bowl_supported = reward
        .get("singing_bowl_control_supported")
        .and_then(Value::as_bool)
        .ok_or_else(|| "CARD_REWARD singing_bowl_control_supported is not boolean".to_owned())?;

    let mut seen_cards = vec![false; cards.len()];
    let mut skip_count = 0usize;
    let mut bowl_count = 0usize;
    for action in actions {
        match action["kind"].as_str() {
            Some("choose_card_reward") => {
                let index = action["reward_index"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| "card reward action omitted reward_index".to_owned())?;
                let card = cards
                    .get(index)
                    .ok_or_else(|| "card reward action index is out of bounds".to_owned())?;
                let uuid = card["card_uuid"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "CARD_REWARD card omitted card_uuid".to_owned())?;
                let card_id = card["card_id"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "CARD_REWARD card omitted card_id".to_owned())?;
                let upgrades = card["upgrades"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| "CARD_REWARD card omitted upgrades".to_owned())?;
                if card["reward_index"].as_u64() != Some(index as u64)
                    || action["reward_type"].as_str() != Some("CARD")
                    || action["card_uuid"].as_str() != Some(uuid)
                    || action["card_id"].as_str() != Some(card_id)
                    || action["card_upgrades"].as_u64() != Some(upgrades as u64)
                    || action["action_id"].as_str()
                        != Some(format!("card_reward:{index}:{uuid}:{card_id}:{upgrades}").as_str())
                    || seen_cards[index]
                {
                    return Err("card reward action does not bind one unique typed card".to_owned());
                }
                seen_cards[index] = true;
            }
            Some("skip_card_reward") => {
                skip_count += 1;
                if action["action_id"].as_str() != Some("skip_card_reward")
                    || action["reward_type"].as_str() != Some("CARD")
                {
                    return Err("card reward skip action is not exact".to_owned());
                }
            }
            Some("take_singing_bowl") => {
                bowl_count += 1;
                if action["action_id"].as_str() != Some("take_singing_bowl")
                    || action["reward_type"].as_str() != Some("CARD")
                    || !action["reward_index"].is_null()
                    || !action["reward_id"].is_null()
                    || !action["reward_gold"].is_null()
                    || !singing_bowl_available
                    || !singing_bowl_supported
                    || !matches!(source_kind, "ordinary_reward" | "neow_reward")
                {
                    return Err("card reward Singing Bowl action is not exact or authorized".into());
                }
            }
            _ => return Err("CARD_REWARD exposed an unrelated action kind".to_owned()),
        }
    }
    if seen_cards.iter().any(|seen| !seen)
        || skip_count != usize::from(skip_available)
        || bowl_count != usize::from(singing_bowl_available && singing_bowl_supported)
    {
        return Err("CARD_REWARD action space is incomplete or over-authorized".to_owned());
    }
    Ok(())
}

fn validate_boss_relic_actions(actions: &[Value]) -> Result<(), String> {
    let mut relic_indices = BTreeSet::new();
    let mut relic_ids = BTreeSet::new();
    let mut skip_count = 0usize;
    for action in actions {
        match action["kind"].as_str() {
            Some("choose_boss_relic") => {
                let index = action["reward_index"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| "boss relic action omitted reward_index".to_owned())?;
                let relic_id = action["reward_id"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "boss relic action omitted reward_id".to_owned())?;
                if action["reward_type"].as_str() != Some("BOSS_RELIC")
                    || action["action_id"].as_str()
                        != Some(format!("boss_relic:{index}:{relic_id}").as_str())
                    || !action["parameter_schema"].is_null()
                    || !relic_indices.insert(index)
                    || !relic_ids.insert(relic_id)
                {
                    return Err("boss relic action does not bind one unique typed relic".to_owned());
                }
            }
            Some("skip_boss_relic") => {
                skip_count += 1;
                if action["action_id"].as_str() != Some("skip_boss_relic")
                    || action["reward_type"].as_str() != Some("BOSS_RELIC")
                    || !action["parameter_schema"].is_null()
                {
                    return Err("boss relic skip action is not exact".to_owned());
                }
            }
            _ => return Err("BOSS_REWARD exposed an unrelated action kind".to_owned()),
        }
    }
    let contiguous = relic_indices.iter().copied().eq(0..relic_indices.len());
    if relic_indices.is_empty() || !contiguous || skip_count != 1 {
        return Err("BOSS_REWARD action space is incomplete or over-authorized".to_owned());
    }
    Ok(())
}

fn validate_hand_select_action(decision: &Value, action: &Value) -> Result<(), String> {
    let hand_select = decision["hand_select"]
        .as_object()
        .ok_or_else(|| "HAND_SELECT decision omitted typed hand_select state".to_owned())?;
    let hand_select_id = hand_select
        .get("hand_select_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "HAND_SELECT decision has no identity".to_owned())?;
    if action["kind"].as_str() != Some("submit_hand_select")
        || action["hand_select_id"].as_str() != Some(hand_select_id)
        || action["action_id"].as_str()
            != Some(format!("submit_hand_select:{hand_select_id}").as_str())
    {
        return Err("HAND_SELECT submit action does not bind the typed decision".to_owned());
    }

    let selected = hand_select
        .get("selected_cards")
        .and_then(Value::as_array)
        .ok_or_else(|| "HAND_SELECT selected_cards is not an array".to_owned())?;
    if !selected.is_empty() {
        return Err("authorized atomic HAND_SELECT must start with no selected cards".to_owned());
    }
    let requested_cards = hand_select
        .get("requested_cards")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "HAND_SELECT requested_cards is not a non-negative integer".to_owned())?;
    let any_number = hand_select
        .get("any_number")
        .and_then(Value::as_bool)
        .ok_or_else(|| "HAND_SELECT any_number is not boolean".to_owned())?;
    let can_pick_zero = hand_select
        .get("can_pick_zero")
        .and_then(Value::as_bool)
        .ok_or_else(|| "HAND_SELECT can_pick_zero is not boolean".to_owned())?;
    let up_to = hand_select
        .get("up_to")
        .and_then(Value::as_bool)
        .ok_or_else(|| "HAND_SELECT up_to is not boolean".to_owned())?;
    if hand_select
        .get("confirm_supported")
        .and_then(Value::as_bool)
        != Some(true)
        || hand_select.get("cancel_supported").and_then(Value::as_bool) != Some(false)
    {
        return Err(
            "HAND_SELECT confirm/cancel capabilities are not the supported atomic surface"
                .to_owned(),
        );
    }
    let candidates = hand_select
        .get("candidate_cards")
        .and_then(Value::as_array)
        .ok_or_else(|| "HAND_SELECT candidate_cards is not an array".to_owned())?;
    let mut candidate_uuids = Vec::new();
    let mut unique_uuids = BTreeSet::new();
    for (index, card) in candidates.iter().enumerate() {
        let uuid = card["card_uuid"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "HAND_SELECT candidate has no card_uuid".to_owned())?;
        let parsed_uuid = Uuid::parse_str(uuid)
            .map_err(|_| "HAND_SELECT candidate card metadata is invalid".to_owned())?;
        if parsed_uuid.to_string() != uuid
            || !unique_uuids.insert(uuid)
            || card["index"].as_u64() != Some(index as u64)
            || card["card_id"].as_str().is_none_or(str::is_empty)
            || card["name"].as_str().is_none_or(str::is_empty)
            || card["upgrades"].as_u64().is_none()
            || card["cost_for_turn"].as_i64().is_none()
        {
            return Err("HAND_SELECT candidate card metadata is invalid or duplicated".to_owned());
        }
        candidate_uuids.push(uuid);
    }
    let allowed_counts = hand_select
        .get("allowed_counts")
        .and_then(Value::as_array)
        .ok_or_else(|| "HAND_SELECT allowed_counts is not an array".to_owned())?;
    let allowed_count_values = allowed_counts
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|count| usize::try_from(count).ok())
                .ok_or_else(|| "HAND_SELECT allowed_counts is not unsigned integers".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let maximum = requested_cards.min(candidates.len());
    let expected_allowed_counts = if any_number || up_to {
        let minimum = if can_pick_zero { 0 } else { 1 };
        if minimum <= maximum {
            (minimum..=maximum).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    } else {
        let mut counts = Vec::new();
        if can_pick_zero {
            counts.push(0);
        }
        if requested_cards <= candidates.len() && !counts.contains(&requested_cards) {
            counts.push(requested_cards);
        }
        counts
    };
    if allowed_count_values.is_empty() || allowed_count_values != expected_allowed_counts {
        return Err("HAND_SELECT allowed_counts is empty or outside the candidate set".to_owned());
    }
    let expected_hand_select_id = format!(
        "hand_select:{requested_cards}:{any_number}:{can_pick_zero}:{up_to}:{}:selected:",
        candidate_uuids.join(",")
    );
    if hand_select_id != expected_hand_select_id {
        return Err("HAND_SELECT identity does not match its typed selection state".to_owned());
    }

    let schema = action["parameter_schema"]
        .as_object()
        .ok_or_else(|| "HAND_SELECT submit action omitted its parameter schema".to_owned())?;
    let expected_candidate_uuids = candidate_uuids
        .iter()
        .map(|uuid| Value::String((*uuid).to_owned()))
        .collect::<Vec<_>>();
    if schema.get("field").and_then(Value::as_str) != Some("selected_card_uuids")
        || schema.get("item_format").and_then(Value::as_str) != Some("uuid")
        || schema.get("ordered").and_then(Value::as_bool) != Some(true)
        || schema.get("unique_items").and_then(Value::as_bool) != Some(true)
        || schema.get("allowed_card_uuids").and_then(Value::as_array)
            != Some(&expected_candidate_uuids)
        || schema.get("allowed_counts").and_then(Value::as_array) != Some(allowed_counts)
    {
        return Err("HAND_SELECT parameter schema does not match its typed decision".to_owned());
    }
    Ok(())
}

fn validate_grid_select_action(decision: &Value, action: &Value) -> Result<(), String> {
    let grid_select = decision["grid_select"]
        .as_object()
        .ok_or_else(|| "GRID_SELECT decision omitted typed grid_select state".to_owned())?;
    let grid_select_id = grid_select
        .get("grid_select_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "GRID_SELECT decision has no identity".to_owned())?;
    let source_pile = grid_select
        .get("source_pile")
        .and_then(Value::as_str)
        .ok_or_else(|| "GRID_SELECT source_pile is not a string".to_owned())?;
    let selection_reason = grid_select
        .get("selection_reason")
        .and_then(Value::as_str)
        .ok_or_else(|| "GRID_SELECT selection_reason is not a string".to_owned())?;
    let origin_event_id = decision.get("event_id").and_then(Value::as_str);
    let event_deck_mutation = source_pile == "MasterDeck"
        && origin_event_id.is_some_and(|value| !value.is_empty())
        && matches!(
            selection_reason,
            "DeckRemove" | "DeckTransform" | "DeckUpgrade"
        );
    let library_offer = source_pile == "EventOffer"
        && selection_reason == "TheLibrary"
        && origin_event_id.is_some_and(|value| value.rsplit('.').next() == Some("TheLibrary"));
    let bottle_selection = source_pile == "MasterDeck"
        && origin_event_id.is_none()
        && matches!(
            selection_reason,
            "BottleFlame" | "BottleLightning" | "BottleTornado"
        );
    let astrolabe_selection =
        source_pile == "MasterDeck" && origin_event_id.is_none() && selection_reason == "Astrolabe";
    let empty_cage_selection =
        source_pile == "MasterDeck" && origin_event_id.is_none() && selection_reason == "EmptyCage";
    let drug_dealer_selection = source_pile == "MasterDeck"
        && origin_event_id == Some("DrugDealer")
        && selection_reason == "DeckTransform"
        && decision["event_screen_index"].as_i64() == Some(1);
    let scry_selection =
        source_pile == "Draw" && origin_event_id.is_none() && selection_reason == "Scry";
    let authorized_surface = sts_java_parity_protocol::natural_combat_grid_kind_v1(
        Some(source_pile),
        Some(selection_reason),
    )
    .is_some()
        || library_offer
        || bottle_selection
        || astrolabe_selection
        || empty_cage_selection
        || scry_selection
        || event_deck_mutation;
    if !authorized_surface {
        return Err("GRID_SELECT is outside the authorized closed atomic scope".to_owned());
    }
    if action["kind"].as_str() != Some("submit_grid_select")
        || action["grid_select_id"].as_str() != Some(grid_select_id)
        || action["source_pile"].as_str() != Some(source_pile)
        || action["selection_reason"].as_str() != Some(selection_reason)
        || action["event_id"].as_str() != origin_event_id
        || action["action_id"].as_str()
            != Some(format!("submit_grid_select:{grid_select_id}").as_str())
    {
        return Err("GRID_SELECT submit action does not bind the typed decision".to_owned());
    }

    let selected = grid_select
        .get("selected_cards")
        .and_then(Value::as_array)
        .ok_or_else(|| "GRID_SELECT selected_cards is not an array".to_owned())?;
    if if astrolabe_selection {
        selected.len() >= 3
    } else {
        !selected.is_empty()
    } {
        return Err("GRID_SELECT selected prefix is outside its authorized scope".to_owned());
    }
    let requested_cards = grid_select
        .get("requested_cards")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "GRID_SELECT requested_cards is not a non-negative integer".to_owned())?;
    let any_number = grid_select
        .get("any_number")
        .and_then(Value::as_bool)
        .ok_or_else(|| "GRID_SELECT any_number is not boolean".to_owned())?;
    let can_cancel = grid_select
        .get("can_cancel")
        .and_then(Value::as_bool)
        .ok_or_else(|| "GRID_SELECT can_cancel is not boolean".to_owned())?;
    if (if scry_selection {
        requested_cards == 0
    } else {
        requested_cards
            != if astrolabe_selection {
                3
            } else if drug_dealer_selection || empty_cage_selection {
                2
            } else {
                1
            }
    }) || any_number != scry_selection
        || can_cancel
        || grid_select
            .get("confirm_supported")
            .and_then(Value::as_bool)
            != Some(false)
        || grid_select.get("cancel_supported").and_then(Value::as_bool) != Some(false)
    {
        return Err("GRID_SELECT capabilities exceed its declared atomic surface".to_owned());
    }
    let candidates = grid_select
        .get("candidate_cards")
        .and_then(Value::as_array)
        .ok_or_else(|| "GRID_SELECT candidate_cards is not an array".to_owned())?;
    if candidates.is_empty() {
        return Err("GRID_SELECT candidate_cards is empty".to_owned());
    }
    if scry_selection && candidates.len() > requested_cards {
        return Err("GRID_SELECT Scry candidates exceed its requested cards".to_owned());
    }
    let mut candidate_uuids = Vec::new();
    let mut unique_uuids = BTreeSet::new();
    for (index, card) in candidates.iter().enumerate() {
        let uuid = card["card_uuid"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "GRID_SELECT candidate has no card_uuid".to_owned())?;
        let parsed_uuid = Uuid::parse_str(uuid)
            .map_err(|_| "GRID_SELECT candidate card metadata is invalid".to_owned())?;
        if parsed_uuid.to_string() != uuid
            || !unique_uuids.insert(uuid)
            || card["index"].as_u64() != Some(index as u64)
            || card["card_id"].as_str().is_none_or(str::is_empty)
            || card["name"].as_str().is_none_or(str::is_empty)
            || card["upgrades"].as_u64().is_none()
            || card["cost_for_turn"].as_i64().is_none()
        {
            return Err("GRID_SELECT candidate card metadata is invalid or duplicated".to_owned());
        }
        candidate_uuids.push(uuid);
    }
    let mut selected_uuids = Vec::new();
    for (index, card) in selected.iter().enumerate() {
        let uuid = card["card_uuid"]
            .as_str()
            .ok_or("GRID_SELECT selected card UUID missing")?;
        if Uuid::parse_str(uuid)
            .map_err(|_| "GRID_SELECT selected UUID invalid")?
            .to_string()
            != uuid
            || !unique_uuids.insert(uuid)
            || card["index"].as_u64() != Some(index as u64)
            || card["card_id"].as_str().is_none_or(str::is_empty)
            || card["name"].as_str().is_none_or(str::is_empty)
            || card["upgrades"].as_u64().is_none()
            || card["cost_for_turn"].as_i64().is_none()
        {
            return Err(
                "GRID_SELECT selected metadata is invalid or overlaps its candidates".into(),
            );
        }
        selected_uuids.push(uuid);
    }
    let allowed_counts = grid_select
        .get("allowed_counts")
        .and_then(Value::as_array)
        .ok_or_else(|| "GRID_SELECT allowed_counts is not an array".to_owned())?;
    let expected_counts = if scry_selection {
        (0..=candidates.len()).map(Value::from).collect::<Vec<_>>()
    } else if drug_dealer_selection || empty_cage_selection {
        vec![Value::from(2)]
    } else {
        vec![Value::from(1)]
    };
    if *allowed_counts != expected_counts {
        return Err(
            "GRID_SELECT allowed_counts do not match its atomic selection scope".to_owned(),
        );
    }
    let expected_grid_select_id = format!(
        "grid_select:{}:{source_pile}:{selection_reason}:{requested_cards}:{any_number}:false:{}:selected:{}",
        origin_event_id.unwrap_or("none"),
        candidate_uuids.join(","), selected_uuids.join(",")
    );
    if grid_select_id != expected_grid_select_id {
        return Err("GRID_SELECT identity does not match its typed selection state".to_owned());
    }

    let schema = action["parameter_schema"]
        .as_object()
        .ok_or_else(|| "GRID_SELECT submit action omitted its parameter schema".to_owned())?;
    let expected_candidate_uuids = candidate_uuids
        .iter()
        .map(|uuid| Value::String((*uuid).to_owned()))
        .collect::<Vec<_>>();
    if schema.get("field").and_then(Value::as_str) != Some("selected_card_uuids")
        || schema.get("item_format").and_then(Value::as_str) != Some("uuid")
        || schema.get("ordered").and_then(Value::as_bool) != Some(true)
        || schema.get("unique_items").and_then(Value::as_bool) != Some(true)
        || schema.get("allowed_card_uuids").and_then(Value::as_array)
            != Some(&expected_candidate_uuids)
        || schema.get("allowed_counts").and_then(Value::as_array) != Some(allowed_counts)
    {
        return Err("GRID_SELECT parameter schema does not match its typed decision".to_owned());
    }
    Ok(())
}

fn validate_control_frame(
    frame: &StateFrame,
    ack: &HelloAck,
    previous_frame_id: Option<u64>,
) -> Result<(), String> {
    if frame.kind != "state_frame"
        || frame.producer_session_id != ack.producer_session_id
        || frame.connection_id != ack.connection_id
        || frame.capture_id != ack.capture_id
        || frame.profile != CONTROL_PROFILE
        || frame.previous_frame_id != previous_frame_id
        || frame.frame_id != previous_frame_id.unwrap_or(0) + 1
    {
        return Err("state_frame identity or sequence does not match the live session".to_owned());
    }
    frame.protocol.validate()?;
    frame.grants.validate_for(CONTROL_PROFILE)?;
    if !frame.boundary.stable || frame.boundary.kind != "combat_player_decision" {
        return Err("state_frame is not a stable combat player decision".to_owned());
    }
    let actions = frame
        .action_space
        .as_ref()
        .ok_or_else(|| "control frame omitted action_space".to_owned())?;
    if actions.kind != "combat_action_space" {
        return Err("control frame action_space kind is invalid".to_owned());
    }
    let mut ids = BTreeSet::new();
    if actions.actions.is_empty()
        || actions
            .actions
            .iter()
            .any(|action| action.action_id.is_empty() || !ids.insert(&action.action_id))
    {
        return Err("control frame action IDs are empty or duplicated".to_owned());
    }
    Ok(())
}

fn validate_action_result(
    result: &ActionResult,
    ack: &HelloAck,
    pending: Option<&PendingRequest>,
) -> Result<(), String> {
    result.protocol.validate()?;
    let pending = pending.ok_or_else(|| "action_result has no pending request".to_owned())?;
    if result.kind != "action_result"
        || result.connection_id != ack.connection_id
        || result.request_id != pending.request.request_id
        || result.based_on_frame_id != pending.request.based_on_frame_id
        || result.action_id != pending.request.action_id
        || (result.status != "accepted"
            && result.status != "rejected"
            && result.status != "unknown")
    {
        return Err("action_result identity does not match the pending request".to_owned());
    }
    Ok(())
}

fn validate_client_request(
    request: &ClientActionRequest,
    manifest: &SessionManifest,
    latest_frame_id: Option<u64>,
    control_uncertain: bool,
) -> Option<(String, String)> {
    if request.schema != "sts_java_parity_live_action_request_v2" {
        return Some((
            "unsupported_request_schema".to_owned(),
            "request schema is invalid".to_owned(),
        ));
    }
    if !request.parameters.is_object() {
        return Some((
            "invalid_action_parameters".to_owned(),
            "request parameters must be a JSON object".to_owned(),
        ));
    }
    if request.session_id != manifest.session_id || request.connection_id != manifest.connection_id
    {
        return Some((
            "session_mismatch".to_owned(),
            "request does not belong to this live session".to_owned(),
        ));
    }
    if control_uncertain {
        return Some((
            "control_uncertain".to_owned(),
            "a prior action outcome is uncertain; restart the live session".to_owned(),
        ));
    }
    if latest_frame_id != Some(request.based_on_frame_id) {
        return Some((
            "stale_frame".to_owned(),
            "request is not based on the latest frame".to_owned(),
        ));
    }
    None
}

fn complete_request(
    manifest: &SessionManifest,
    request: PendingRequest,
    status: &str,
    code: Option<String>,
    message: &str,
    successor_frame_id: Option<u64>,
    successor_frame_path: Option<&Path>,
) -> Result<(), String> {
    let settlement_wall_time_ms = request.sent_at.elapsed().as_millis();
    let receipt = ActionReceipt {
        schema: "sts_java_parity_live_action_receipt_v2".to_owned(),
        status: status.to_owned(),
        code,
        message: message.to_owned(),
        session_id: manifest.session_id.clone(),
        connection_id: manifest.connection_id.clone(),
        request_id: request.request.request_id,
        based_on_frame_id: request.request.based_on_frame_id,
        action_id: request.request.action_id,
        parameters: request.request.parameters,
        completed_at_unix_ms: unix_ms()?,
        settlement_wall_time_ms: Some(settlement_wall_time_ms),
        producer_result: request.producer_result,
        successor_frame_id,
        successor_frame_path: successor_frame_path.map(|path| path.to_string_lossy().into_owned()),
    };
    publish_json_create_new(&request.receipt_path, &receipt)
}

fn live_control_profile() -> Result<String, String> {
    let profile = optional_env("STS_JAVA_PLAY_CONTROL_PROFILE")?
        .unwrap_or_else(|| CONTROL_PROFILE.to_owned());
    if profile != CONTROL_PROFILE
        && profile != RUN_CONTROL_PROFILE
        && profile != SCENARIO_CONTROL_PROFILE
    {
        return Err(format!("unsupported Java live control profile: {profile}"));
    }
    Ok(profile)
}

fn capture_launch_evidence(
    control_profile: &str,
    output_dir: &Path,
) -> Result<Option<LaunchEvidence>, String> {
    let profile = optional_env("STS_JAVA_PLAY_LAUNCH_PROFILE")?;
    let plan_path = optional_env("STS_JAVA_PLAY_LAUNCH_PLAN")?;
    let (profile, plan_path) =
        match (profile, plan_path) {
            (None, None) => return Ok(None),
            (Some(profile), Some(plan_path)) => (profile, plan_path),
            _ => return Err(
                "Java launch evidence is incomplete; profile and launch plan must travel together"
                    .to_owned(),
            ),
        };
    validate_launch_evidence(&profile, control_profile, Path::new(&plan_path), output_dir).map(Some)
}

fn validate_launch_evidence(
    profile: &str,
    control_profile: &str,
    plan_path: &Path,
    output_dir: &Path,
) -> Result<LaunchEvidence, String> {
    if profile != "strict" && profile != "accelerated" {
        return Err(format!("unsupported Java launch profile: {profile}"));
    }
    let path = absolute_path(plan_path)?;
    if !path.is_file() {
        return Err(format!(
            "Java launch plan is not a regular file: {}",
            path.display()
        ));
    }
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "failed to read Java launch plan {}: {error}",
            path.display()
        )
    })?;
    let plan: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse Java launch plan {}: {error}",
            path.display()
        )
    })?;
    let schema = plan["schema"]
        .as_str()
        .ok_or_else(|| "Java launch plan has no schema".to_owned())?;
    if !matches!(
        schema,
        "sts_java_play_launch_plan_v3" | "sts_java_play_launch_plan_v4"
    ) || plan["profile"].as_str() != Some(profile)
        || plan["control_profile"].as_str() != Some(control_profile)
    {
        return Err(
            "Java launch plan identity does not match the inherited launch/control profiles"
                .to_owned(),
        );
    }
    let selected_mods = plan["selected_mod_ids"]
        .as_array()
        .ok_or_else(|| "Java launch plan has no selected_mod_ids array".to_owned())?;
    let has_super_fast_mode = selected_mods
        .iter()
        .any(|value| value.as_str() == Some("superfastmode"));
    if has_super_fast_mode != (profile == "accelerated") {
        return Err("Java launch plan profile disagrees with its selected Mods".to_owned());
    }
    let storage = plan["storage"]
        .as_object()
        .ok_or_else(|| "Java launch plan has no typed storage object".to_owned())?;
    let storage_class = storage
        .get("class")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java launch plan storage has no class".to_owned())?;
    if storage_class != "installed_game" && storage_class != "isolated_working_directory_v1" {
        return Err(format!(
            "unsupported Java launch storage class: {storage_class}"
        ));
    }
    let working_directory = storage
        .get("working_directory")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java launch plan storage has no working_directory".to_owned())?;
    let source_game_directory = storage
        .get("source_game_directory")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java launch plan storage has no source_game_directory".to_owned())?;
    let profile_substrate = storage
        .get("profile_substrate")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java launch plan storage has no profile_substrate".to_owned())?;
    let tutorial_policy = storage
        .get("tutorial_policy")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java launch plan storage has no tutorial_policy".to_owned())?;
    validate_profile_tutorial_pair(profile_substrate, tutorial_policy)?;
    let isolation_id = match storage.get("isolation_id") {
        Some(Value::String(value)) => {
            parse_uuid("storage.isolation_id", value)?;
            Some(value.clone())
        }
        Some(Value::Null) | None => None,
        _ => return Err("Java launch storage isolation_id must be a UUID or null".to_owned()),
    };
    if (storage_class == "installed_game" && isolation_id.is_some())
        || (storage_class == "isolated_working_directory_v1" && isolation_id.is_none())
    {
        return Err("Java launch storage class and isolation_id disagree".to_owned());
    }
    if control_profile == SCENARIO_CONTROL_PROFILE
        && storage_class != "isolated_working_directory_v1"
    {
        return Err("scenario control requires isolated launch storage".to_owned());
    }
    let actual_working_directory = std::env::current_dir()
        .map_err(|error| format!("failed to read Java bridge working directory: {error}"))?;
    if !same_existing_directory(Path::new(working_directory), &actual_working_directory)? {
        return Err(format!(
            "Java launch storage working directory does not match the bridge process: planned={} actual={}",
            working_directory,
            actual_working_directory.display()
        ));
    }
    let working_aliases_source = same_existing_directory(
        Path::new(working_directory),
        Path::new(source_game_directory),
    )?;
    if storage_class == "installed_game" && !working_aliases_source {
        return Err("installed-game launch storage must use the source game directory".to_owned());
    }
    if storage_class == "isolated_working_directory_v1" && working_aliases_source {
        return Err("isolated launch storage cannot alias the source game directory".to_owned());
    }
    if schema == "sts_java_play_launch_plan_v4" {
        validate_managed_bridge_binding(&plan, output_dir)?;
    }
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(LaunchEvidence {
        profile: profile.to_owned(),
        control_profile: control_profile.to_owned(),
        plan_path: path.to_string_lossy().into_owned(),
        plan_sha256: format!("{:X}", hasher.finalize()),
        storage_class: storage_class.to_owned(),
        working_directory: working_directory.to_owned(),
        source_game_directory: source_game_directory.to_owned(),
        profile_substrate: profile_substrate.to_owned(),
        tutorial_policy: tutorial_policy.to_owned(),
        isolation_id,
    })
}

fn validate_profile_tutorial_pair(
    profile_substrate: &str,
    tutorial_policy: &str,
) -> Result<(), String> {
    let supported = matches!(
        (profile_substrate, tutorial_policy),
        (
            "installed_profile_v1"
                | "copied_source_profile_v1"
                | "captured_native_profile_v1"
                | "canonical_neow_ready_preferences_v1",
            "observe_and_acknowledge_v1"
        ) | (
            "canonical_tutorial_complete_preferences_v1"
                | "canonical_a20h_lab_preferences_v1"
                | "captured_native_lab_preferences_v1",
            "precompleted_via_profile_v1"
        )
    );
    if supported {
        Ok(())
    } else {
        Err(format!(
            "unsupported Java profile substrate/tutorial policy pair: {profile_substrate}/{tutorial_policy}"
        ))
    }
}

fn validate_managed_bridge_binding(plan: &Value, output_dir: &Path) -> Result<(), String> {
    let bridge = plan["bridge"]
        .as_object()
        .ok_or_else(|| "managed Java launch plan has no bridge object".to_owned())?;
    if bridge.get("kind").and_then(Value::as_str) != Some("communication_mod_stdio_v1") {
        return Err("managed Java launch plan has an unsupported bridge kind".to_owned());
    }
    let executable = bridge
        .get("executable")
        .and_then(Value::as_object)
        .ok_or_else(|| "managed Java launch plan has no bridge executable identity".to_owned())?;
    let executable_path = executable
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "managed Java launch plan bridge has no executable path".to_owned())?;
    let expected_length = executable
        .get("length")
        .and_then(Value::as_u64)
        .ok_or_else(|| "managed Java launch plan bridge has no executable length".to_owned())?;
    let expected_sha256 = executable
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "managed Java launch plan bridge has no executable digest".to_owned())?;
    let current_executable = std::env::current_exe()
        .map_err(|error| format!("failed to locate running Java bridge executable: {error}"))?;
    if !same_existing_file(Path::new(executable_path), &current_executable)? {
        return Err("managed Java launch plan selected another bridge executable".to_owned());
    }
    let metadata = fs::metadata(&current_executable)
        .map_err(|error| format!("failed to inspect running Java bridge executable: {error}"))?;
    if metadata.len() != expected_length
        || !sha256_file(&current_executable)?.eq_ignore_ascii_case(expected_sha256)
    {
        return Err("managed Java launch plan bridge executable identity changed".to_owned());
    }
    let planned_output = bridge
        .get("output_directory")
        .and_then(Value::as_str)
        .ok_or_else(|| "managed Java launch plan bridge has no output directory".to_owned())?;
    if !same_existing_directory(Path::new(planned_output), output_dir)? {
        return Err("managed Java launch plan selected another bridge output directory".to_owned());
    }
    Ok(())
}

fn same_existing_directory(left: &Path, right: &Path) -> Result<bool, String> {
    let canonical = |path: &Path| {
        let resolved = fs::canonicalize(path).map_err(|error| {
            format!(
                "failed to resolve Java launch directory {}: {error}",
                path.display()
            )
        })?;
        if !resolved.is_dir() {
            return Err(format!(
                "Java launch directory is not a directory: {}",
                path.display()
            ));
        }
        Ok::<_, String>(resolved)
    };
    let left = canonical(left)?;
    let right = canonical(right)?;
    #[cfg(windows)]
    {
        Ok(left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()))
    }
    #[cfg(not(windows))]
    {
        Ok(left == right)
    }
}

fn same_existing_file(left: &Path, right: &Path) -> Result<bool, String> {
    let canonical = |path: &Path| {
        let resolved = fs::canonicalize(path).map_err(|error| {
            format!(
                "failed to resolve Java bridge executable {}: {error}",
                path.display()
            )
        })?;
        if !resolved.is_file() {
            return Err(format!(
                "Java bridge executable is not a file: {}",
                path.display()
            ));
        }
        Ok::<_, String>(resolved)
    };
    let left = canonical(left)?;
    let right = canonical(right)?;
    #[cfg(windows)]
    {
        Ok(left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()))
    }
    #[cfg(not(windows))]
    {
        Ok(left == right)
    }
}

fn validate_launch_loaded_mods(
    evidence: &LaunchEvidence,
    loaded_mods: &[LoadedModIdentity],
) -> Result<(), String> {
    let bytes = fs::read(&evidence.plan_path).map_err(|error| {
        format!(
            "failed to reread Java launch plan {}: {error}",
            evidence.plan_path
        )
    })?;
    let plan: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse Java launch plan {}: {error}",
            evidence.plan_path
        )
    })?;
    let planned = plan["selected_mods"]
        .as_array()
        .ok_or_else(|| "Java launch plan has no selected_mods array".to_owned())?;
    let mut expected = BTreeMap::new();
    for mod_identity in planned {
        let id = mod_identity["id"]
            .as_str()
            .ok_or_else(|| "Java launch plan Mod has no id".to_owned())?;
        let version = mod_identity["version"]
            .as_str()
            .ok_or_else(|| format!("Java launch plan Mod {id} has no version"))?;
        let jar_sha256 = mod_identity["jar"]["sha256"]
            .as_str()
            .ok_or_else(|| format!("Java launch plan Mod {id} has no JAR SHA-256"))?;
        if expected
            .insert(id.to_owned(), (version.to_owned(), jar_sha256.to_owned()))
            .is_some()
        {
            return Err(format!("Java launch plan contains duplicate Mod {id}"));
        }
    }
    let mut actual = BTreeMap::new();
    for mod_identity in loaded_mods {
        if actual
            .insert(
                mod_identity.id.clone(),
                (
                    mod_identity.version.clone(),
                    mod_identity.jar_sha256.clone(),
                ),
            )
            .is_some()
        {
            return Err(format!(
                "producer loaded-Mod manifest contains duplicate Mod {}",
                mod_identity.id
            ));
        }
    }
    if expected != actual {
        return Err("Java launch plan does not match the producer loaded-Mod manifest".to_owned());
    }
    Ok(())
}

fn optional_env(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) if value.trim().is_empty() => Err(format!("{name} is empty")),
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} is not valid Unicode")),
    }
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

fn next_request(
    paths: &SessionPaths,
    processed: &mut HashSet<PathBuf>,
) -> Result<Option<(PathBuf, ClientActionRequest)>, String> {
    let mut candidates = fs::read_dir(&paths.requests_dir)
        .map_err(|error| format!("failed to read request directory: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    candidates.sort();
    for path in candidates {
        if processed.insert(path.clone()) {
            return read_json(&path).map(|request| Some((path, request)));
        }
    }
    Ok(None)
}

fn discover_live_session(output_dir: &Path) -> Result<(SessionManifest, SessionPaths), String> {
    if !output_dir.is_dir() {
        return Err(format!(
            "no Java parity live workspace exists at {}; run .\\java-play.ps1 launch and enter a stable combat decision first",
            output_dir.display()
        ));
    }
    let mut manifests = Vec::new();
    for entry in fs::read_dir(output_dir)
        .map_err(|error| format!("failed to read {}: {error}", output_dir.display()))?
    {
        let path = entry
            .map_err(|error| format!("failed to enumerate live sessions: {error}"))?
            .path();
        let manifest_path = path.join("session.json");
        if !manifest_path.is_file() || path.join("ended.json").exists() {
            continue;
        }
        let raw: Value = read_json(&manifest_path)?;
        let Some(process_id) = raw["process_id"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
        else {
            continue;
        };
        if !process_is_running(process_id) {
            continue;
        }
        let manifest: SessionManifest = serde_json::from_value(raw).map_err(|error| {
            format!(
                "failed to parse running live session {}: {error}",
                manifest_path.display()
            )
        })?;
        manifests.push(manifest);
    }
    manifests.sort_by_key(|manifest| manifest.created_at_unix_ms);
    let manifest = manifests
        .pop()
        .ok_or_else(|| "no running Java parity live session was found".to_owned())?;
    let paths = session_paths_from_manifest(&manifest);
    Ok((manifest, paths))
}

fn latest_frame(paths: &SessionPaths) -> Result<(PathBuf, Value), String> {
    let mut frames = fs::read_dir(&paths.frames_dir)
        .map_err(|error| format!("failed to read frame directory: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    frames.sort();
    let path = frames
        .pop()
        .ok_or_else(|| "live session has not emitted a stable control frame yet".to_owned())?;
    let value: Value = read_json(&path)?;
    Ok((path, value))
}

fn resolve_selector<'a>(
    actions: &'a [ActionHandle],
    frame_id: u64,
    selector: &str,
) -> Result<&'a ActionHandle, String> {
    if let Some((frame, action)) = selector
        .strip_prefix('f')
        .and_then(|rest| rest.split_once(":a"))
    {
        let selected_frame = frame
            .parse::<u64>()
            .map_err(|_| "action handle frame is invalid".to_owned())?;
        let index = action
            .parse::<usize>()
            .map_err(|_| "action handle index is invalid".to_owned())?;
        if selected_frame != frame_id {
            return Err(format!(
                "action handle belongs to frame {selected_frame}, but the latest frame is {frame_id}"
            ));
        }
        return actions
            .get(index)
            .ok_or_else(|| "action handle index is out of bounds".to_owned());
    }
    actions
        .iter()
        .find(|action| action.action_id == selector)
        .ok_or_else(|| {
            "selector is neither a current fN:aN handle nor an exact action_id".to_owned()
        })
}

fn action_handle(frame_id: u64, index: usize, action: &Value) -> Result<ActionHandle, String> {
    let action_id = action["action_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "control action has no action_id".to_owned())?;
    let kind = action["kind"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "control action has no kind".to_owned())?;
    Ok(ActionHandle {
        handle: format!("f{frame_id}:a{index}"),
        action_id: action_id.to_owned(),
        kind: kind.to_owned(),
        card_uuid: action["card_uuid"].as_str().map(str::to_owned),
        card_id: action["card_id"].as_str().map(str::to_owned),
        card_upgrades: action["card_upgrades"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        card_ordinal: action["card_ordinal"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        hand_index: action["hand_index"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        potion_slot: action["potion_slot"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        potion_id: action["potion_id"].as_str().map(str::to_owned),
        potion_potency: action["potion_potency"]
            .as_i64()
            .and_then(|value| i32::try_from(value).ok()),
        target_monster_id: action["target_monster_id"].as_str().map(str::to_owned),
        target_monster_instance_id: action["target_monster_instance_id"].as_u64(),
        target_monster_index: action["target_monster_index"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        event_id: action["event_id"].as_str().map(str::to_owned),
        option_index: action["option_index"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        option_text: action["option_text"].as_str().map(str::to_owned),
        neow_reward_type: action["neow_reward_type"].as_str().map(str::to_owned),
        neow_drawback: action["neow_drawback"].as_str().map(str::to_owned),
        map_x: action["map_x"].as_i64(),
        map_y: action["map_y"].as_i64(),
        map_room_type: action["map_room_type"].as_str().map(str::to_owned),
        reward_type: action["reward_type"].as_str().map(str::to_owned),
        reward_index: action["reward_index"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        reward_id: action["reward_id"].as_str().map(str::to_owned),
        reward_gold: action["reward_gold"].as_i64(),
        shop_slot: action["shop_slot"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        shop_price: action["shop_price"].as_i64(),
        ftue_id: action["ftue_id"].as_str().map(str::to_owned),
        hand_select_id: action["hand_select_id"].as_str().map(str::to_owned),
        hand_select_allowed_card_uuids: if action["kind"].as_str() == Some("submit_hand_select") {
            action["parameter_schema"]["allowed_card_uuids"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        },
        hand_select_allowed_counts: if action["kind"].as_str() == Some("submit_hand_select") {
            action["parameter_schema"]["allowed_counts"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_u64)
                        .filter_map(|value| usize::try_from(value).ok())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        },
        grid_select_id: action["grid_select_id"].as_str().map(str::to_owned),
        grid_select_source_pile: action["source_pile"].as_str().map(str::to_owned),
        grid_select_reason: action["selection_reason"].as_str().map(str::to_owned),
        grid_select_allowed_card_uuids: if action["kind"].as_str() == Some("submit_grid_select") {
            action["parameter_schema"]["allowed_card_uuids"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        },
        grid_select_allowed_counts: if action["kind"].as_str() == Some("submit_grid_select") {
            action["parameter_schema"]["allowed_counts"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_u64)
                        .filter_map(|value| usize::try_from(value).ok())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        },
        parameters_required: action["parameter_schema"].is_object(),
    })
}

fn frame_path(paths: &SessionPaths, frame_id: u64) -> PathBuf {
    paths.frames_dir.join(format!("frame-{frame_id:020}.json"))
}

fn receipt_path(paths: &SessionPaths, request_id: &str) -> PathBuf {
    paths.receipts_dir.join(format!("{request_id}.json"))
}

fn read_producer(sender: mpsc::Sender<ProducerEvent>) {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        match read_protocol_line(&mut input) {
            Ok(Some(line)) => {
                if sender.send(ProducerEvent::Line(line)).is_err() {
                    return;
                }
            }
            Ok(None) => {
                let _ = sender.send(ProducerEvent::Eof);
                return;
            }
            Err(error) => {
                let _ = sender.send(ProducerEvent::Error(error));
                return;
            }
        }
    }
}

fn read_protocol_line<R: BufRead>(input: &mut R) -> Result<Option<String>, String> {
    let mut bytes = Vec::new();
    loop {
        let available = input
            .fill_buf()
            .map_err(|error| format!("failed to read CommunicationMod output: {error}"))?;
        if available.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            return Err("CommunicationMod output ended in a partial line".to_owned());
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.unwrap_or(available.len());
        if bytes.len() + take > MAX_PROTOCOL_LINE_BYTES {
            return Err(format!(
                "CommunicationMod line exceeded {MAX_PROTOCOL_LINE_BYTES} bytes"
            ));
        }
        bytes.extend_from_slice(&available[..take]);
        input.consume(take + usize::from(newline.is_some()));
        if newline.is_some() {
            break;
        }
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "CommunicationMod output was not valid UTF-8".to_owned())
}

fn write_protocol_line<W: Write>(output: &mut W, line: &str) -> Result<(), String> {
    output
        .write_all(line.as_bytes())
        .and_then(|_| output.write_all(b"\n"))
        .and_then(|_| output.flush())
        .map_err(|error| format!("failed to write CommunicationMod protocol line: {error}"))
}

fn append_transcript(file: &mut File, line: &str) -> Result<(), String> {
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.flush())
        .map_err(|error| format!("failed to append live transcript: {error}"))
}

fn save_rejected_frame(paths: &SessionPaths, line: &str, reason: &str) -> Result<(), String> {
    publish_json_create_new(
        &paths.session_dir.join("rejected-frame.json"),
        &serde_json::json!({"reason": reason, "raw_line": line}),
    )
}

fn publish_json_create_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {}: {error}", path.display()))?;
    let mut terminated = bytes;
    terminated.push(b'\n');
    publish_bytes_create_new(path, &terminated)
}

fn publish_bytes_create_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("path has no UTF-8 file name: {}", path.display()))?;
    let temporary = parent.join(format!(".{file_name}.{}.pending", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("failed to create {}: {error}", temporary.display()))?;
    let write_result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to write {}: {error}", temporary.display()));
    drop(file);
    let publish_result = write_result.and_then(|_| {
        fs::hard_link(&temporary, path)
            .map_err(|error| format!("failed to publish {}: {error}", path.display()))
    });
    let cleanup_result = fs::remove_file(&temporary)
        .map_err(|error| format!("failed to clean {}: {error}", temporary.display()));
    publish_result.and(cleanup_result)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| format!("failed to resolve current directory: {error}"))
    }
}

fn unix_ms() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("system clock precedes UNIX epoch: {error}"))
}

fn parse_uuid(name: &str, value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| format!("{name} is not a UUID"))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:X}", hasher.finalize()))
}

#[cfg(windows)]
fn process_is_running(process_id: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};
    const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE_ACCESS, 0, process_id);
        if handle.is_null() {
            return false;
        }
        let result = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        CloseHandle(handle);
        result
    }
}

#[cfg(not(windows))]
fn process_is_running(_process_id: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_bound_handle_rejects_a_newer_frame() {
        let actions = vec![ActionHandle {
            handle: "f3:a0".to_owned(),
            action_id: "end_turn".to_owned(),
            kind: "end_turn".to_owned(),
            card_uuid: None,
            card_id: None,
            card_upgrades: None,
            card_ordinal: None,
            hand_index: None,
            potion_slot: None,
            potion_id: None,
            potion_potency: None,
            target_monster_id: None,
            target_monster_instance_id: None,
            target_monster_index: None,
            event_id: None,
            option_index: None,
            option_text: None,
            neow_reward_type: None,
            neow_drawback: None,
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: None,
            reward_index: None,
            reward_id: None,
            reward_gold: None,
            shop_slot: None,
            shop_price: None,
            ftue_id: None,
            hand_select_id: None,
            hand_select_allowed_card_uuids: Vec::new(),
            hand_select_allowed_counts: Vec::new(),
            grid_select_id: None,
            grid_select_source_pile: None,
            grid_select_reason: None,
            grid_select_allowed_card_uuids: Vec::new(),
            grid_select_allowed_counts: Vec::new(),
            parameters_required: false,
        }];
        let error = resolve_selector(&actions, 4, "f3:a0").unwrap_err();
        assert!(error.contains("latest frame is 4"));
    }

    #[test]
    fn potion_handle_preserves_typed_action_details() {
        let action = serde_json::json!({
            "action_id": "use_potion:1:Block Potion:none",
            "kind": "use_potion",
            "potion_slot": 1,
            "potion_id": "Block Potion",
            "potion_potency": 12
        });
        let handle = action_handle(8, 6, &action).unwrap();

        assert_eq!(handle.handle, "f8:a6");
        assert_eq!(handle.potion_slot, Some(1));
        assert_eq!(handle.potion_id.as_deref(), Some("Block Potion"));
        assert_eq!(handle.potion_potency, Some(12));
        assert_eq!(handle.target_monster_instance_id, None);
    }

    #[test]
    fn hand_select_surface_binds_uuid_candidates_counts_and_atomic_parameters() {
        let card_uuid = Uuid::new_v4().to_string();
        let hand_select_id = format!("hand_select:99:true:true:false:{card_uuid}:selected:");
        let action = serde_json::json!({
            "action_id": format!("submit_hand_select:{hand_select_id}"),
            "kind": "submit_hand_select",
            "hand_select_id": hand_select_id,
            "parameter_schema": {
                "field": "selected_card_uuids",
                "item_format": "uuid",
                "ordered": true,
                "unique_items": true,
                "allowed_card_uuids": [card_uuid],
                "allowed_counts": [0, 1]
            }
        });
        let mut frame = unsupported_run_frame(vec![action.clone()]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({
            "decision": {
                "kind": "hand_select",
                "hand_select": {
                    "hand_select_id": hand_select_id,
                    "selection_reason": "Exhaust",
                    "requested_cards": 99,
                    "any_number": true,
                    "can_pick_zero": true,
                    "up_to": false,
                    "allowed_counts": [0, 1],
                    "confirm_supported": true,
                    "cancel_supported": false,
                    "candidate_cards": [{
                        "index": 0,
                        "card_uuid": card_uuid,
                        "card_id": "Strike_R",
                        "name": "Strike",
                        "upgrades": 0,
                        "cost_for_turn": 1
                    }],
                    "selected_cards": []
                }
            }
        });
        validate_run_decision_surface(&frame).unwrap();

        let mut inconsistent = frame.clone();
        inconsistent.public_base["decision"]["hand_select"]["allowed_counts"] =
            serde_json::json!([0]);
        inconsistent.action_space.actions[0]["parameter_schema"]["allowed_counts"] =
            serde_json::json!([0]);
        assert!(validate_run_decision_surface(&inconsistent)
            .unwrap_err()
            .contains("allowed_counts"));

        let handle = action_handle(9, 0, &action).unwrap();
        assert_eq!(
            handle.hand_select_allowed_card_uuids,
            vec![card_uuid.clone()]
        );
        assert_eq!(handle.hand_select_allowed_counts, vec![0, 1]);
        assert_eq!(
            hand_select_parameters(&handle, &[]).unwrap(),
            serde_json::json!({"selected_card_uuids": []})
        );
        assert_eq!(
            hand_select_parameters(&handle, std::slice::from_ref(&card_uuid)).unwrap(),
            serde_json::json!({"selected_card_uuids": [card_uuid]})
        );
    }

    #[test]
    fn hand_select_parameters_reject_unknown_duplicate_and_wrong_count() {
        let card_uuid = Uuid::new_v4().to_string();
        let other_uuid = Uuid::new_v4().to_string();
        let action = action_handle(
            4,
            0,
            &serde_json::json!({
                "action_id": "submit_hand_select:test",
                "kind": "submit_hand_select",
                "hand_select_id": "test",
                "parameter_schema": {
                    "allowed_card_uuids": [card_uuid],
                    "allowed_counts": [1]
                }
            }),
        )
        .unwrap();

        assert!(hand_select_parameters(&action, &[])
            .unwrap_err()
            .contains("allowed counts"));
        assert!(hand_select_parameters(&action, &[other_uuid])
            .unwrap_err()
            .contains("candidate set"));
        assert!(
            hand_select_parameters(&action, &[card_uuid.clone(), card_uuid])
                .unwrap_err()
                .contains("duplicated")
        );
    }

    #[test]
    fn grid_select_surface_binds_discard_reason_uuid_and_single_card_count() {
        let card_uuid = Uuid::new_v4().to_string();
        let grid_select_id =
            format!("grid_select:none:Discard:DiscardToHand:1:false:false:{card_uuid}:selected:");
        let action = serde_json::json!({
            "action_id": format!("submit_grid_select:{grid_select_id}"),
            "kind": "submit_grid_select",
            "grid_select_id": grid_select_id,
            "source_pile": "Discard",
            "selection_reason": "DiscardToHand",
            "parameter_schema": {
                "field": "selected_card_uuids",
                "item_format": "uuid",
                "ordered": true,
                "unique_items": true,
                "allowed_card_uuids": [card_uuid],
                "allowed_counts": [1]
            }
        });
        let mut frame = unsupported_run_frame(vec![action.clone()]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({
            "decision": {
                "kind": "grid_select",
                "grid_select": {
                    "grid_select_id": grid_select_id,
                    "source_pile": "Discard",
                    "selection_reason": "DiscardToHand",
                    "requested_cards": 1,
                    "any_number": false,
                    "can_cancel": false,
                    "allowed_counts": [1],
                    "confirm_supported": false,
                    "cancel_supported": false,
                    "candidate_cards": [{
                        "index": 0,
                        "card_uuid": card_uuid,
                        "card_id": "True Grit",
                        "name": "True Grit",
                        "upgrades": 0,
                        "cost_for_turn": 1
                    }],
                    "selected_cards": []
                }
            }
        });
        validate_run_decision_surface(&frame).unwrap();

        let handle = action_handle(9, 0, &action).unwrap();
        assert_eq!(
            handle.grid_select_id.as_deref(),
            Some(grid_select_id.as_str())
        );
        assert_eq!(handle.grid_select_source_pile.as_deref(), Some("Discard"));
        assert_eq!(handle.grid_select_reason.as_deref(), Some("DiscardToHand"));
        assert_eq!(
            grid_select_parameters(&handle, std::slice::from_ref(&card_uuid)).unwrap(),
            serde_json::json!({"selected_card_uuids": [card_uuid]})
        );

        for reason in ["DiscardToHandRetain", "DiscardToHandNoCostChange"] {
            let selection_id =
                format!("grid_select:none:Discard:{reason}:1:false:false:{card_uuid}:selected:");
            let mut selection_frame = frame.clone();
            let grid = &mut selection_frame.public_base["decision"]["grid_select"];
            grid["grid_select_id"] = serde_json::json!(selection_id);
            grid["selection_reason"] = serde_json::json!(reason);
            let mut selection_action = action.clone();
            selection_action["action_id"] =
                serde_json::json!(format!("submit_grid_select:{selection_id}"));
            selection_action["grid_select_id"] = serde_json::json!(selection_id);
            selection_action["selection_reason"] = serde_json::json!(reason);
            selection_frame.action_space.actions = vec![selection_action.clone()];
            validate_run_decision_surface(&selection_frame).unwrap();
            assert_eq!(
                action_handle(9, 0, &selection_action)
                    .unwrap()
                    .grid_select_reason
                    .as_deref(),
                Some(reason)
            );
            selection_frame.public_base["decision"]["grid_select"]["requested_cards"] =
                serde_json::json!(2);
            assert!(validate_run_decision_surface(&selection_frame).is_err());
        }

        let exhume_id =
            format!("grid_select:none:Exhaust:Exhume:1:false:false:{card_uuid}:selected:");
        let mut exhume_action = action.clone();
        exhume_action["action_id"] = serde_json::json!(format!("submit_grid_select:{exhume_id}"));
        exhume_action["grid_select_id"] = serde_json::json!(exhume_id);
        exhume_action["source_pile"] = serde_json::json!("Exhaust");
        exhume_action["selection_reason"] = serde_json::json!("Exhume");
        let mut exhume_frame = frame.clone();
        let grid = &mut exhume_frame.public_base["decision"]["grid_select"];
        grid["grid_select_id"] = serde_json::json!(exhume_id);
        grid["source_pile"] = serde_json::json!("Exhaust");
        grid["selection_reason"] = serde_json::json!("Exhume");
        exhume_frame.action_space.actions = vec![exhume_action.clone()];
        validate_run_decision_surface(&exhume_frame).unwrap();
        let handle = action_handle(9, 0, &exhume_action).unwrap();
        assert_eq!(handle.grid_select_source_pile.as_deref(), Some("Exhaust"));
        assert_eq!(handle.grid_select_reason.as_deref(), Some("Exhume"));
        assert_eq!(
            grid_select_parameters(&handle, std::slice::from_ref(&card_uuid)).unwrap(),
            serde_json::json!({"selected_card_uuids": [card_uuid]})
        );
        for (pile, reason) in [
            ("Draw", "SkillFromDeckToHand"),
            ("Draw", "AttackFromDeckToHand"),
            ("Draw", "Omniscience"),
            ("Discard", "MoveToDrawPile"),
            ("MasterDeck", "BottleFlame"),
            ("MasterDeck", "BottleLightning"),
            ("MasterDeck", "BottleTornado"),
        ] {
            let mut draw_frame = exhume_frame.clone();
            let grid_id =
                format!("grid_select:none:{pile}:{reason}:1:false:false:{card_uuid}:selected:");
            let grid = &mut draw_frame.public_base["decision"]["grid_select"];
            grid["grid_select_id"] = serde_json::json!(grid_id);
            grid["source_pile"] = serde_json::json!(pile);
            grid["selection_reason"] = serde_json::json!(reason);
            let action = &mut draw_frame.action_space.actions[0];
            action["action_id"] = serde_json::json!(format!("submit_grid_select:{grid_id}"));
            action["grid_select_id"] = serde_json::json!(grid_id);
            action["source_pile"] = serde_json::json!(pile);
            action["selection_reason"] = serde_json::json!(reason);
            validate_run_decision_surface(&draw_frame).unwrap();
            let handle = action_handle(9, 0, &draw_frame.action_space.actions[0]).unwrap();
            assert_eq!(
                grid_select_parameters(&handle, std::slice::from_ref(&card_uuid)).unwrap(),
                serde_json::json!({"selected_card_uuids": [card_uuid]})
            );
            let mut invalid = draw_frame.clone();
            invalid.public_base["decision"]["grid_select"]["source_pile"] =
                serde_json::json!("Exhaust");
            assert!(validate_run_decision_surface(&invalid)
                .unwrap_err()
                .contains("closed atomic scope"));
            for (field, value) in [
                ("requested_cards", serde_json::json!(2)),
                ("any_number", serde_json::json!(true)),
                ("can_cancel", serde_json::json!(true)),
            ] {
                let mut invalid = draw_frame.clone();
                invalid.public_base["decision"]["grid_select"][field] = value;
                assert!(validate_run_decision_surface(&invalid)
                    .unwrap_err()
                    .contains("atomic surface"));
            }
        }
        let mut invalid_exhume = exhume_frame.clone();
        invalid_exhume.public_base["decision"]["grid_select"]["selection_reason"] =
            serde_json::json!("UnknownExhaustChoice");
        assert!(validate_run_decision_surface(&invalid_exhume)
            .unwrap_err()
            .contains("closed atomic scope"));
        exhume_frame.public_base["decision"]["grid_select"]["can_cancel"] = serde_json::json!(true);
        assert!(validate_run_decision_surface(&exhume_frame)
            .unwrap_err()
            .contains("atomic surface"));

        let library_event_id = "TheLibrary";
        let library_grid_select_id = format!(
            "grid_select:{library_event_id}:EventOffer:TheLibrary:1:false:false:{card_uuid}:selected:"
        );
        let mut library_action = action.clone();
        library_action["action_id"] =
            serde_json::json!(format!("submit_grid_select:{library_grid_select_id}"));
        library_action["grid_select_id"] = serde_json::json!(library_grid_select_id);
        library_action["source_pile"] = serde_json::json!("EventOffer");
        library_action["selection_reason"] = serde_json::json!("TheLibrary");
        library_action["event_id"] = serde_json::json!(library_event_id);
        let mut library_frame = frame.clone();
        library_frame.public_base["decision"]["event_id"] = serde_json::json!(library_event_id);
        library_frame.public_base["decision"]["grid_select"]["grid_select_id"] =
            serde_json::json!(library_grid_select_id);
        library_frame.public_base["decision"]["grid_select"]["source_pile"] =
            serde_json::json!("EventOffer");
        library_frame.public_base["decision"]["grid_select"]["selection_reason"] =
            serde_json::json!("TheLibrary");
        library_frame.action_space.actions = vec![library_action];
        validate_run_decision_surface(&library_frame).unwrap();

        for (event_id, operation) in [
            ("BonfireElementals", "DeckRemove"),
            ("FutureDeckEvent", "DeckTransform"),
            ("FutureDeckEvent", "DeckUpgrade"),
        ] {
            let event_grid_select_id = format!(
                "grid_select:{event_id}:MasterDeck:{operation}:1:false:false:{card_uuid}:selected:"
            );
            let mut event_action = action.clone();
            event_action["action_id"] =
                serde_json::json!(format!("submit_grid_select:{event_grid_select_id}"));
            event_action["grid_select_id"] = serde_json::json!(event_grid_select_id);
            event_action["source_pile"] = serde_json::json!("MasterDeck");
            event_action["selection_reason"] = serde_json::json!(operation);
            event_action["event_id"] = serde_json::json!(event_id);
            let mut event_frame = frame.clone();
            event_frame.public_base["decision"]["event_id"] = serde_json::json!(event_id);
            event_frame.public_base["decision"]["grid_select"]["grid_select_id"] =
                serde_json::json!(event_grid_select_id);
            event_frame.public_base["decision"]["grid_select"]["source_pile"] =
                serde_json::json!("MasterDeck");
            event_frame.public_base["decision"]["grid_select"]["selection_reason"] =
                serde_json::json!(operation);
            event_frame.action_space.actions = vec![event_action];
            validate_run_decision_surface(&event_frame).unwrap();
        }

        let mut wrong_source = frame.clone();
        wrong_source.public_base["decision"]["grid_select"]["source_pile"] =
            serde_json::json!("Draw");
        assert!(validate_run_decision_surface(&wrong_source)
            .unwrap_err()
            .contains("closed atomic scope"));
    }

    #[test]
    fn grid_select_parameters_reject_unknown_duplicate_and_wrong_count() {
        let card_uuid = Uuid::new_v4().to_string();
        let other_uuid = Uuid::new_v4().to_string();
        let action = action_handle(
            4,
            0,
            &serde_json::json!({
                "action_id": "submit_grid_select:test",
                "kind": "submit_grid_select",
                "grid_select_id": "test",
                "source_pile": "Discard",
                "selection_reason": "DiscardToHand",
                "parameter_schema": {
                    "allowed_card_uuids": [card_uuid],
                    "allowed_counts": [1]
                }
            }),
        )
        .unwrap();

        assert!(grid_select_parameters(&action, &[])
            .unwrap_err()
            .contains("allowed counts"));
        assert!(grid_select_parameters(&action, &[other_uuid])
            .unwrap_err()
            .contains("candidate set"));
        assert!(
            grid_select_parameters(&action, &[card_uuid.clone(), card_uuid])
                .unwrap_err()
                .contains("duplicated")
        );
    }

    #[test]
    fn scry_grid_accepts_atomic_subsets_and_binds_scope_and_card_order() {
        let cards = (0..3)
            .map(|index| {
                serde_json::json!({
                    "index":index,"card_uuid":Uuid::new_v4().to_string(),
                    "card_id":"Defend_P","name":"Defend","upgrades":0,"cost_for_turn":1
                })
            })
            .collect::<Vec<_>>();
        let uuids = cards
            .iter()
            .map(|card| card["card_uuid"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let grid_id = format!(
            "grid_select:none:Draw:Scry:3:true:false:{}:selected:",
            uuids.join(",")
        );
        let decision = serde_json::json!({"kind":"grid_select","grid_select":{
            "grid_select_id":grid_id,"source_pile":"Draw","selection_reason":"Scry",
            "requested_cards":3,"any_number":true,"can_cancel":false,"allowed_counts":[0,1,2,3],
            "confirm_supported":false,"cancel_supported":false,"candidate_cards":cards,"selected_cards":[]}});
        let action = serde_json::json!({"action_id":format!("submit_grid_select:{grid_id}"),"kind":"submit_grid_select",
            "grid_select_id":grid_id,"source_pile":"Draw","selection_reason":"Scry",
            "parameter_schema":{"field":"selected_card_uuids","item_format":"uuid","ordered":true,"unique_items":true,
                "allowed_card_uuids":uuids,"allowed_counts":[0,1,2,3]}});
        validate_grid_select_action(&decision, &action).unwrap();
        let handle = action_handle(9, 0, &action).unwrap();
        for mask in 0..8 {
            let subset = uuids
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, uuid)| uuid.clone())
                .collect::<Vec<_>>();
            grid_select_parameters(&handle, &subset).unwrap();
        }
        for (field, value) in [
            ("requested_cards", serde_json::json!(0)),
            ("requested_cards", serde_json::json!(2)),
            ("any_number", serde_json::json!(false)),
            ("can_cancel", serde_json::json!(true)),
            ("allowed_counts", serde_json::json!([1])),
            ("source_pile", serde_json::json!("Discard")),
            ("selected_cards", serde_json::json!([cards[0]])),
        ] {
            let mut invalid = decision.clone();
            invalid["grid_select"][field] = value;
            assert!(
                validate_grid_select_action(&invalid, &action).is_err(),
                "{field}"
            );
        }
        let mut reordered = decision.clone();
        reordered["grid_select"]["candidate_cards"]
            .as_array_mut()
            .unwrap()
            .reverse();
        for (index, card) in reordered["grid_select"]["candidate_cards"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            card["index"] = Value::from(index);
        }
        assert!(validate_grid_select_action(&reordered, &action)
            .unwrap_err()
            .contains("identity"));
        let mut stale_schema = action.clone();
        stale_schema["parameter_schema"]["allowed_counts"] = serde_json::json!([1]);
        assert!(validate_grid_select_action(&decision, &stale_schema).is_err());
    }

    #[test]
    fn drug_dealer_grid_requires_two_ordered_cards_and_its_result_screen() {
        let cards=(0..3).map(|index| serde_json::json!({"index":index,"card_uuid":Uuid::new_v4().to_string(),"card_id":"Strike_R","name":"Strike","upgrades":0,"cost_for_turn":1})).collect::<Vec<_>>();
        let uuids = cards
            .iter()
            .map(|c| c["card_uuid"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let grid_id = format!(
            "grid_select:DrugDealer:MasterDeck:DeckTransform:2:false:false:{}:selected:",
            uuids.join(",")
        );
        let decision = serde_json::json!({"kind":"grid_select","event_id":"DrugDealer","event_screen_index":1,
            "grid_select":{"grid_select_id":grid_id,"source_pile":"MasterDeck","selection_reason":"DeckTransform",
                "requested_cards":2,"any_number":false,"can_cancel":false,"allowed_counts":[2],
                "confirm_supported":false,"cancel_supported":false,"candidate_cards":cards,"selected_cards":[]}});
        let action = serde_json::json!({"action_id":format!("submit_grid_select:{grid_id}"),"kind":"submit_grid_select",
            "grid_select_id":grid_id,"source_pile":"MasterDeck","selection_reason":"DeckTransform","event_id":"DrugDealer",
            "parameter_schema":{"field":"selected_card_uuids","item_format":"uuid","ordered":true,"unique_items":true,"allowed_card_uuids":uuids,"allowed_counts":[2]}});
        validate_grid_select_action(&decision, &action).unwrap();
        let handle = action_handle(9, 0, &action).unwrap();
        let chosen = vec![uuids[2].clone(), uuids[0].clone()];
        assert_eq!(
            grid_select_parameters(&handle, &chosen).unwrap()["selected_card_uuids"],
            serde_json::json!(chosen)
        );
        assert!(grid_select_parameters(&handle, &uuids).is_err());
        assert!(grid_select_parameters(&handle, &uuids[..1]).is_err());
        let mut cage = decision.clone();
        cage["event_id"] = Value::Null;
        cage["event_screen_index"] = Value::Null;
        let cage_id = format!(
            "grid_select:none:MasterDeck:EmptyCage:2:false:false:{}:selected:",
            uuids.join(",")
        );
        cage["grid_select"]["selection_reason"] = serde_json::json!("EmptyCage");
        cage["grid_select"]["grid_select_id"] = serde_json::json!(cage_id);
        let mut cage_action = action.clone();
        cage_action["event_id"] = Value::Null;
        cage_action["selection_reason"] = serde_json::json!("EmptyCage");
        cage_action["grid_select_id"] = serde_json::json!(cage_id);
        cage_action["action_id"] = serde_json::json!(format!("submit_grid_select:{cage_id}"));
        validate_grid_select_action(&cage, &cage_action).unwrap();
        let cage_handle = action_handle(9, 0, &cage_action).unwrap();
        assert!(grid_select_parameters(&cage_handle, &chosen).is_ok());
        assert!(grid_select_parameters(&cage_handle, &uuids[..1]).is_err());
        let mut partial_cage = cage.clone();
        partial_cage["grid_select"]["selected_cards"] = serde_json::json!([cards[0].clone()]);
        assert!(validate_grid_select_action(&partial_cage, &cage_action).is_err());
        for (field, value) in [
            ("event_id", serde_json::json!("Neow")),
            ("event_screen_index", serde_json::json!(0)),
        ] {
            let mut invalid = decision.clone();
            invalid[field] = value;
            assert!(validate_grid_select_action(&invalid, &action).is_err());
        }
    }

    #[test]
    fn astrolabe_grid_accepts_ordered_partial_clicks_without_widening_other_grids() {
        for count in 0..3 {
            let card = |index| {
                serde_json::json!({"index":index,"card_uuid":Uuid::new_v4().to_string(),
                "card_id":"Strike_R","name":"Strike","upgrades":0,"cost_for_turn":1})
            };
            let candidate = card(0);
            let selected = (0..count).map(card).collect::<Vec<_>>();
            let candidate_uuid = candidate["card_uuid"].as_str().unwrap();
            let grid_id = format!(
                "grid_select:none:MasterDeck:Astrolabe:3:false:false:{candidate_uuid}:selected:{}",
                selected
                    .iter()
                    .map(|c| c["card_uuid"].as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let decision = serde_json::json!({"kind":"grid_select","grid_select":{
                "grid_select_id":grid_id,"source_pile":"MasterDeck","selection_reason":"Astrolabe",
                "requested_cards":3,"any_number":false,"can_cancel":false,"allowed_counts":[1],
                "confirm_supported":false,"cancel_supported":false,"candidate_cards":[candidate],"selected_cards":selected}});
            let action = serde_json::json!({"action_id":format!("submit_grid_select:{grid_id}"),"kind":"submit_grid_select",
                "grid_select_id":grid_id,"source_pile":"MasterDeck","selection_reason":"Astrolabe",
                "parameter_schema":{"field":"selected_card_uuids","item_format":"uuid","ordered":true,"unique_items":true,
                    "allowed_card_uuids":[candidate_uuid],"allowed_counts":[1]}});
            validate_grid_select_action(&decision, &action).unwrap();
            let handle = action_handle(9, 0, &action).unwrap();
            grid_select_parameters(&handle, &[candidate_uuid.to_owned()]).unwrap();
            let mut invalid = decision.clone();
            invalid["grid_select"]["requested_cards"] = serde_json::json!(1);
            assert!(validate_grid_select_action(&invalid, &action).is_err());
            invalid = decision.clone();
            invalid["grid_select"]["selected_cards"]
                .as_array_mut()
                .unwrap()
                .push(candidate.clone());
            assert!(validate_grid_select_action(&invalid, &action).is_err());
            if count == 2 {
                invalid = decision.clone();
                invalid["grid_select"]["selected_cards"]
                    .as_array_mut()
                    .unwrap()
                    .reverse();
                assert!(
                    validate_grid_select_action(&invalid, &action).is_err(),
                    "order must bind the grid identity"
                );
            }
        }
    }

    #[test]
    fn card_reward_surface_binds_each_typed_card_and_skip_without_bowl_authority() {
        let first_uuid = Uuid::new_v4().to_string();
        let second_uuid = Uuid::new_v4().to_string();
        let actions = vec![
            serde_json::json!({
                "action_id": format!("card_reward:0:{first_uuid}:Sword Boomerang:0"),
                "kind": "choose_card_reward",
                "reward_type": "CARD",
                "reward_index": 0,
                "card_uuid": first_uuid,
                "card_id": "Sword Boomerang",
                "card_upgrades": 0
            }),
            serde_json::json!({
                "action_id": format!("card_reward:1:{second_uuid}:True Grit:1"),
                "kind": "choose_card_reward",
                "reward_type": "CARD",
                "reward_index": 1,
                "card_uuid": second_uuid,
                "card_id": "True Grit",
                "card_upgrades": 1
            }),
            serde_json::json!({
                "action_id": "skip_card_reward",
                "kind": "skip_card_reward",
                "reward_type": "CARD"
            }),
        ];
        let mut frame = unsupported_run_frame(actions.clone());
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({
            "decision": {
                "kind": "card_reward",
                "card_reward": {
                    "source_kind": "ordinary_reward",
                    "cards": [
                        {
                            "reward_index": 0,
                            "card_uuid": first_uuid,
                            "card_id": "Sword Boomerang",
                            "name": "Sword Boomerang",
                            "upgrades": 0
                        },
                        {
                            "reward_index": 1,
                            "card_uuid": second_uuid,
                            "card_id": "True Grit",
                            "name": "True Grit+",
                            "upgrades": 1
                        }
                    ],
                    "skip_available": true,
                    "singing_bowl_available": true,
                    "singing_bowl_control_supported": false
                }
            }
        });

        validate_run_decision_surface(&frame).unwrap();
        let handle = action_handle(12, 1, &actions[1]).unwrap();
        assert_eq!(handle.card_id.as_deref(), Some("True Grit"));
        assert_eq!(handle.card_upgrades, Some(1));

        frame.public_base["decision"]["card_reward"]["source_kind"] =
            serde_json::json!("neow_reward");
        validate_run_decision_surface(&frame).unwrap();

        frame.public_base["decision"]["card_reward"]["singing_bowl_control_supported"] =
            serde_json::json!(true);
        assert!(
            validate_run_decision_surface(&frame).is_err(),
            "an authorized visible bowl needs its action"
        );
        let bowl = serde_json::json!({"action_id": "take_singing_bowl", "kind": "take_singing_bowl", "reward_type": "CARD"});
        frame.action_space.actions.push(bowl.clone());
        validate_run_decision_surface(&frame).unwrap();
        frame.action_space.actions.push(bowl);
        assert!(
            validate_run_decision_surface(&frame).is_err(),
            "bowl actions must be unique"
        );
        frame.action_space.actions.pop();
        frame.public_base["decision"]["card_reward"]["singing_bowl_available"] =
            serde_json::json!(false);
        assert!(
            validate_run_decision_surface(&frame).is_err(),
            "a hidden bowl cannot authorize an action"
        );
        frame.public_base["decision"]["card_reward"]["singing_bowl_available"] =
            serde_json::json!(true);
        frame.action_space.actions.last_mut().unwrap()["reward_index"] = serde_json::json!(0);
        assert!(
            validate_run_decision_surface(&frame).is_err(),
            "bowl is not a card index"
        );
        frame.action_space.actions.pop();
        frame.public_base["decision"]["card_reward"]["singing_bowl_control_supported"] =
            serde_json::json!(false);

        frame.action_space.actions[1]["card_upgrades"] = serde_json::json!(0);
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("typed card"));
    }

    #[test]
    fn boss_relic_surface_binds_each_typed_relic_and_one_skip() {
        let actions = vec![
            serde_json::json!({
                "action_id": "boss_relic:0:Sacred Bark",
                "kind": "choose_boss_relic",
                "reward_type": "BOSS_RELIC",
                "reward_index": 0,
                "reward_id": "Sacred Bark"
            }),
            serde_json::json!({
                "action_id": "boss_relic:1:Ectoplasm",
                "kind": "choose_boss_relic",
                "reward_type": "BOSS_RELIC",
                "reward_index": 1,
                "reward_id": "Ectoplasm"
            }),
            serde_json::json!({
                "action_id": "boss_relic:2:Runic Pyramid",
                "kind": "choose_boss_relic",
                "reward_type": "BOSS_RELIC",
                "reward_index": 2,
                "reward_id": "Runic Pyramid"
            }),
            serde_json::json!({
                "action_id": "skip_boss_relic",
                "kind": "skip_boss_relic",
                "reward_type": "BOSS_RELIC"
            }),
        ];
        let mut frame = unsupported_run_frame(actions.clone());
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "boss_relic"}});

        validate_run_decision_surface(&frame).unwrap();
        let handle = action_handle(20, 2, &actions[2]).unwrap();
        assert_eq!(handle.reward_index, Some(2));
        assert_eq!(handle.reward_id.as_deref(), Some("Runic Pyramid"));

        frame.action_space.actions[2]["reward_id"] = serde_json::json!("Ectoplasm");
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("unique typed relic"));
    }

    #[test]
    fn shop_surfaces_bind_entry_and_typed_merchandise() {
        let mut entry = unsupported_run_frame(vec![
            serde_json::json!({
                "action_id": "enter_shop",
                "kind": "enter_shop"
            }),
            serde_json::json!({
                "action_id": "proceed_shop_room",
                "kind": "proceed_shop_room"
            }),
        ]);
        entry.boundary.kind = "run_decision".to_owned();
        entry.public_base = serde_json::json!({"decision": {"kind": "shop_entry"}});
        validate_run_decision_surface(&entry).unwrap();

        let card_uuid = Uuid::new_v4().to_string();
        let card = serde_json::json!({
            "action_id": format!("shop_card:2:{card_uuid}:Battle Trance:0:52"),
            "kind": "buy_shop_card",
            "shop_slot": 2,
            "shop_price": 52,
            "card_uuid": card_uuid,
            "card_id": "Battle Trance",
            "card_upgrades": 0
        });
        let relic = serde_json::json!({
            "action_id": "shop_relic:1:Membership Card:168",
            "kind": "buy_shop_relic",
            "reward_type": "SHOP_RELIC",
            "reward_id": "Membership Card",
            "shop_slot": 1,
            "shop_price": 168
        });
        let potion = serde_json::json!({
            "action_id": "shop_potion:2:Fire Potion:57",
            "kind": "buy_shop_potion",
            "reward_type": "SHOP_POTION",
            "potion_id": "Fire Potion",
            "shop_slot": 2,
            "shop_price": 57
        });
        let purge = serde_json::json!({
            "action_id": "begin_shop_purge:75",
            "kind": "begin_shop_purge",
            "shop_price": 75
        });
        let mut shop = unsupported_run_frame(vec![
            card.clone(),
            relic.clone(),
            potion.clone(),
            purge.clone(),
            serde_json::json!({"action_id": "leave_shop", "kind": "leave_shop"}),
        ]);
        shop.boundary.kind = "run_decision".to_owned();
        shop.public_base = serde_json::json!({"decision": {"kind": "shop"}});
        validate_run_decision_surface(&shop).unwrap();

        let handle = action_handle(30, 0, &card).unwrap();
        assert_eq!(handle.shop_slot, Some(2));
        assert_eq!(handle.shop_price, Some(52));
        let relic_handle = action_handle(30, 1, &relic).unwrap();
        assert_eq!(relic_handle.shop_slot, Some(1));
        assert_eq!(relic_handle.shop_price, Some(168));
        assert_eq!(relic_handle.reward_id.as_deref(), Some("Membership Card"));
        let potion_handle = action_handle(30, 2, &potion).unwrap();
        assert_eq!(potion_handle.shop_slot, Some(2));
        assert_eq!(potion_handle.shop_price, Some(57));
        assert_eq!(potion_handle.potion_id.as_deref(), Some("Fire Potion"));
        let purge_handle = action_handle(30, 3, &purge).unwrap();
        assert_eq!(purge_handle.shop_price, Some(75));

        shop.action_space.actions[0]["shop_price"] = serde_json::json!(51);
        assert!(validate_run_decision_surface(&shop)
            .unwrap_err()
            .contains("typed merchandise"));
    }

    #[test]
    fn shop_purge_surface_binds_master_deck_ordinal_and_java_uuid() {
        let card_uuid = Uuid::new_v4().to_string();
        let choose = serde_json::json!({
            "action_id": format!("shop_purge_card:4:{card_uuid}:Strike_R:0"),
            "kind": "choose_shop_purge_card",
            "card_ordinal": 4,
            "card_uuid": card_uuid,
            "card_id": "Strike_R",
            "card_upgrades": 0
        });
        let mut selection = unsupported_run_frame(vec![choose.clone()]);
        selection.boundary.kind = "run_decision".to_owned();
        selection.public_base = serde_json::json!({"decision": {"kind": "shop_purge_card"}});
        validate_run_decision_surface(&selection).unwrap();

        let confirm = serde_json::json!({
            "action_id": format!("confirm_shop_purge_card:4:{card_uuid}:Strike_R:0"),
            "kind": "confirm_shop_purge_card",
            "card_ordinal": 4,
            "card_uuid": card_uuid,
            "card_id": "Strike_R",
            "card_upgrades": 0
        });
        let mut confirmation = unsupported_run_frame(vec![confirm]);
        confirmation.boundary.kind = "run_decision".to_owned();
        confirmation.public_base = serde_json::json!({"decision": {"kind": "shop_purge_confirm"}});
        validate_run_decision_surface(&confirmation).unwrap();

        selection.action_space.actions[0]["card_ordinal"] = serde_json::json!(3);
        assert!(validate_run_decision_surface(&selection)
            .unwrap_err()
            .contains("typed card"));
    }

    #[test]
    fn campfire_smith_surface_binds_master_deck_ordinal_and_java_uuid() {
        let mut campfire = unsupported_run_frame(vec![
            serde_json::json!({
                "action_id": "begin_campfire_rest",
                "kind": "begin_campfire_rest"
            }),
            serde_json::json!({
                "action_id": "begin_campfire_recall",
                "kind": "begin_campfire_recall"
            }),
            serde_json::json!({
                "action_id": "begin_campfire_smith",
                "kind": "begin_campfire_smith"
            }),
            serde_json::json!({
                "action_id": "begin_campfire_dig",
                "kind": "begin_campfire_dig"
            }),
        ]);
        campfire.boundary.kind = "run_decision".to_owned();
        campfire.public_base = serde_json::json!({"decision": {"kind": "campfire"}});
        validate_run_decision_surface(&campfire).unwrap();

        let card_uuid = Uuid::new_v4().to_string();
        let action = serde_json::json!({
            "action_id": format!("smith_card:12:{card_uuid}:Battle Trance:0"),
            "kind": "choose_smith_card",
            "card_ordinal": 12,
            "card_uuid": card_uuid,
            "card_id": "Battle Trance",
            "card_upgrades": 0
        });
        let mut frame = unsupported_run_frame(vec![action.clone()]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "smith_card"}});
        validate_run_decision_surface(&frame).unwrap();

        let handle = action_handle(40, 0, &action).unwrap();
        assert_eq!(handle.card_ordinal, Some(12));
        assert_eq!(handle.card_upgrades, Some(0));

        frame.action_space.actions[0]["card_ordinal"] = serde_json::json!(11);
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("typed card"));
    }

    #[test]
    fn ordinary_treasure_surface_exposes_distinct_open_and_proceed() {
        let mut frame = unsupported_run_frame(vec![
            serde_json::json!({"action_id": "open_chest", "kind": "open_chest"}),
            serde_json::json!({"action_id": "proceed_treasure", "kind": "proceed_treasure"}),
        ]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "treasure"}});
        validate_run_decision_surface(&frame).unwrap();

        for index in 0..2 {
            let mut missing = frame.clone();
            missing.action_space.actions.remove(index);
            assert!(validate_run_decision_surface(&missing).is_err());
            let mut duplicate = frame.clone();
            duplicate.action_space.actions[1 - index] = frame.action_space.actions[index].clone();
            assert!(validate_run_decision_surface(&duplicate).is_err());
            let mut wrong_id = frame.clone();
            wrong_id.action_space.actions[index]["action_id"] = serde_json::json!("any_chest");
            assert!(validate_run_decision_surface(&wrong_id).is_err());
        }
    }

    #[test]
    fn boss_treasure_surface_is_distinct_and_parameter_free() {
        let mut frame = unsupported_run_frame(vec![serde_json::json!({
            "action_id": "open_boss_chest",
            "kind": "open_boss_chest"
        })]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "boss_treasure"}});
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions[0]["kind"] = serde_json::json!("open_chest");
        assert!(validate_run_decision_surface(&frame).is_err());
    }

    #[test]
    fn completed_boss_treasure_exposes_only_typed_proceed() {
        let mut frame = unsupported_run_frame(vec![serde_json::json!({
            "action_id": "proceed_boss_treasure",
            "kind": "proceed_boss_treasure"
        })]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "boss_treasure_complete"}});
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions[0]["action_id"] = serde_json::json!("proceed_rest_room");
        assert!(validate_run_decision_surface(&frame).is_err());
    }

    #[test]
    fn completed_first_a20_boss_exposes_only_typed_double_boss_proceed() {
        let mut frame = unsupported_run_frame(vec![serde_json::json!({
            "action_id": "proceed_double_boss",
            "kind": "proceed_double_boss"
        })]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "double_boss_complete"}});
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions[0]["action_id"] = serde_json::json!("proceed_boss_treasure");
        assert!(validate_run_decision_surface(&frame).is_err());
    }

    #[test]
    fn completed_act3_victory_exposes_only_typed_victory_door_proceed() {
        let mut frame = unsupported_run_frame(vec![serde_json::json!({
            "action_id": "proceed_act3_victory",
            "kind": "proceed_act3_victory"
        })]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "act3_victory_complete"}});
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions[0]["action_id"] = serde_json::json!("proceed_double_boss");
        assert!(validate_run_decision_surface(&frame).is_err());
    }

    #[test]
    fn completed_true_victory_exposes_only_typed_ending_proceed() {
        let mut frame = unsupported_run_frame(vec![serde_json::json!({
            "action_id": "proceed_true_victory",
            "kind": "proceed_true_victory"
        })]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "true_victory_complete"}});
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions[0]["action_id"] = serde_json::json!("proceed_act3_victory");
        assert!(validate_run_decision_surface(&frame).is_err());
    }

    #[test]
    fn terminal_surface_keeps_heart_victory_actionless_except_for_exact_managed_discard() {
        let mut frame = unsupported_run_frame(Vec::new());
        frame.boundary.kind = "terminal".to_owned();
        frame.public_base = serde_json::json!({
            "run": {
                "act": 4,
                "has_ruby_key": true,
                "has_sapphire_key": true,
                "has_emerald_key": true
            },
            "decision": {"kind": "heart_victory"}
        });
        validate_run_decision_surface(&frame).unwrap();

        let discard = serde_json::json!({
            "action_id": "run_discard_to_main_menu", "kind": "run_discard_to_main_menu",
            "parameter_schema": null
        });
        frame.action_space.actions = vec![discard.clone()];
        validate_run_decision_surface(&frame).unwrap();
        for malformed in [
            "duplicate",
            "parameters",
            "scalar_schema",
            "potion",
            "reset",
            "alias",
        ] {
            let mut invalid = frame.clone();
            match malformed {
                "duplicate" => invalid.action_space.actions.push(discard.clone()),
                "parameters" => {
                    invalid.action_space.actions[0]["parameter_schema"] = serde_json::json!({})
                }
                "scalar_schema" => {
                    invalid.action_space.actions[0]["parameter_schema"] = serde_json::json!(false)
                }
                "potion" => invalid.action_space.actions[0]["potion_slot"] = serde_json::json!(0),
                "reset" => {
                    invalid.action_space.actions[0]["kind"] =
                        serde_json::json!("run_reset_to_main_menu_preserving_save")
                }
                "alias" => {
                    invalid.action_space.actions[0]["action_id"] = serde_json::json!("alias")
                }
                _ => unreachable!(),
            }
            assert!(
                validate_run_decision_surface(&invalid).is_err(),
                "{malformed}"
            );
        }
        frame.action_space.actions.clear();

        frame.public_base["run"]["has_emerald_key"] = serde_json::json!(false);
        assert!(validate_run_decision_surface(&frame).is_err());
        frame.public_base["run"]["has_emerald_key"] = serde_json::json!(true);
        frame.action_space.actions.push(serde_json::json!({
            "action_id": "proceed_true_victory",
            "kind": "proceed_true_victory"
        }));
        assert!(validate_run_decision_surface(&frame).is_err());
    }

    #[test]
    fn run_potion_discard_is_typed_and_orthogonal_to_the_decision_surface() {
        let discard = serde_json::json!({
            "action_id": "discard_potion:0:Swift Potion",
            "kind": "discard_potion",
            "potion_slot": 0,
            "potion_id": "Swift Potion"
        });
        let mut frame = unsupported_run_frame(vec![
            serde_json::json!({
                "action_id": "map_node:0:15",
                "kind": "choose_map_node",
                "map_x": 0,
                "map_y": 15,
                "map_room_type": "com.megacrit.cardcrawl.rooms.MonsterRoomBoss"
            }),
            discard.clone(),
        ]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "map"}});

        validate_run_decision_surface(&frame).unwrap();
        let handle = action_handle(41, 1, &discard).unwrap();
        assert_eq!(handle.potion_slot, Some(0));
        assert_eq!(handle.potion_id.as_deref(), Some("Swift Potion"));

        frame.boundary.kind = "combat_player_decision".to_owned();
        frame.action_space.kind = "combat_action_space".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "combat"}});
        frame.action_space.actions = vec![
            serde_json::json!({"action_id": "end_turn", "kind": "end_turn"}),
            discard.clone(),
        ];
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions[1]["action_id"] =
            serde_json::json!("discard_potion:1:Swift Potion");
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("parameter-free typed potion"));
    }

    #[test]
    fn run_potion_support_preserves_combat_potions_and_distinct_targets() {
        let mut frame = unsupported_run_frame(vec![
            serde_json::json!({"action_id":"end_turn", "kind":"end_turn"}),
            serde_json::json!({"action_id":"use_potion:0:BloodPotion:none", "kind":"use_potion",
                "potion_slot":0, "potion_id":"BloodPotion", "potion_potency":15}),
            serde_json::json!({"action_id":"use_potion:1:Fire Potion:0", "kind":"use_potion",
                "potion_slot":1, "potion_id":"Fire Potion", "potion_potency":20,
                "target_monster_instance_id":0, "target_monster_id":"FuzzyLouseNormal"}),
            serde_json::json!({"action_id":"use_potion:1:Fire Potion:1", "kind":"use_potion",
                "potion_slot":1, "potion_id":"Fire Potion", "potion_potency":20,
                "target_monster_instance_id":1, "target_monster_id":"FuzzyLouseDefensive"}),
        ]);
        frame.boundary.kind = "combat_player_decision".into();
        frame.action_space.kind = "combat_action_space".into();
        frame.public_base = serde_json::json!({"decision":{"kind":"combat"}});
        validate_run_decision_surface(&frame).unwrap();
        for (index, action) in frame.action_space.actions.iter().enumerate().skip(2) {
            let handle = action_handle(1, index, action).unwrap();
            assert_eq!(handle.potion_slot, Some(1));
            assert_eq!(handle.target_monster_instance_id, Some((index - 2) as u64));
        }
    }

    #[test]
    fn run_potion_use_keeps_its_slot_identity_and_primary_reward_action() {
        let use_potion = serde_json::json!({
            "action_id": "use_potion:0:EntropicBrew", "kind": "use_potion",
            "potion_slot": 0, "potion_id": "EntropicBrew"
        });
        let mut frame = unsupported_run_frame(vec![
            serde_json::json!({"action_id":"close_combat_reward", "kind":"close_combat_reward"}),
            use_potion.clone(),
            serde_json::json!({"action_id":"discard_potion:0:EntropicBrew", "kind":"discard_potion",
                "potion_slot":0,"potion_id":"EntropicBrew"}),
        ]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision":{"kind":"combat_reward"}});
        validate_run_decision_surface(&frame).unwrap();
        let handle = action_handle(41, 1, &use_potion).unwrap();
        assert_eq!(handle.potion_slot, Some(0));
        assert_eq!(handle.potion_id.as_deref(), Some("EntropicBrew"));
        for invalid_kind in ["duplicate", "slot", "target", "parameters", "no_primary"] {
            let mut invalid = frame.clone();
            match invalid_kind {
                "duplicate" => invalid.action_space.actions.push(use_potion.clone()),
                "slot" => invalid.action_space.actions[1]["potion_slot"] = serde_json::json!(1),
                "target" => {
                    invalid.action_space.actions[1]["target_monster_instance_id"] =
                        serde_json::json!(0)
                }
                "parameters" => {
                    invalid.action_space.actions[1]["parameter_schema"] = serde_json::json!({})
                }
                "no_primary" => {
                    invalid.action_space.actions.remove(0);
                }
                _ => unreachable!(),
            }
            assert!(
                validate_run_decision_surface(&invalid).is_err(),
                "{invalid_kind}"
            );
        }
    }

    #[test]
    fn boss_map_surface_binds_one_unique_typed_node() {
        let boss = serde_json::json!({
            "action_id": "map_node:0:15",
            "kind": "choose_map_node",
            "map_x": 0,
            "map_y": 15,
            "map_room_type": "com.megacrit.cardcrawl.rooms.MonsterRoomBoss"
        });
        let mut frame = unsupported_run_frame(vec![boss.clone()]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "map"}});

        validate_run_decision_surface(&frame).unwrap();
        let handle = action_handle(42, 0, &boss).unwrap();
        assert_eq!(handle.map_x, Some(0));
        assert_eq!(handle.map_y, Some(15));
        assert_eq!(
            handle.map_room_type.as_deref(),
            Some("com.megacrit.cardcrawl.rooms.MonsterRoomBoss")
        );

        frame.action_space.actions.push(serde_json::json!({
            "action_id": "map_node:0:15:duplicate",
            "kind": "choose_map_node",
            "map_x": 0,
            "map_y": 15,
            "map_room_type": "com.megacrit.cardcrawl.rooms.MonsterRoomBoss"
        }));
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("same coordinate"));
    }

    #[test]
    fn reward_map_preview_accepts_one_exact_close_action() {
        let close = serde_json::json!({
            "action_id": "close_map",
            "kind": "close_map"
        });
        let mut frame = unsupported_run_frame(vec![
            serde_json::json!({
                "action_id": "map_node:2:1",
                "kind": "choose_map_node",
                "map_x": 2,
                "map_y": 1,
                "map_room_type": "com.megacrit.cardcrawl.rooms.MonsterRoom"
            }),
            close.clone(),
        ]);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "map"}});

        validate_run_decision_surface(&frame).unwrap();
        let handle = action_handle(43, 1, &close).unwrap();
        assert_eq!(handle.action_id, "close_map");

        frame.action_space.actions.push(serde_json::json!({
            "action_id": "close_map:duplicate",
            "kind": "close_map"
        }));
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("exact parameter-free"));
        frame.action_space.actions.pop();
        frame.action_space.actions[1]["map_x"] = serde_json::json!(2);
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("exact parameter-free"));
    }

    #[test]
    fn unsupported_run_decision_requires_typed_choice_and_zero_actions() {
        let mut frame = unsupported_run_frame(Vec::new());
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions.push(serde_json::json!({
            "action_id": "choose_anyway",
            "kind": "choose_event_option"
        }));
        let error = validate_run_decision_surface(&frame).unwrap_err();
        assert!(error.contains("no authorized actions"), "{error}");

        frame.action_space.actions.clear();
        frame.public_base["decision"]["choice_type"] = Value::String("MAP".to_owned());
        assert!(validate_run_decision_surface(&frame).is_err());
    }

    #[test]
    fn neow_choice_surface_requires_original_typed_reward_identities() {
        let rewards = [
            ("TRANSFORM_CARD", "NONE"),
            ("THREE_SMALL_POTIONS", "NONE"),
            ("TWO_FIFTY_GOLD", "TEN_PERCENT_HP_LOSS"),
            ("BOSS_RELIC", "NONE"),
        ];
        let actions = rewards
            .iter()
            .enumerate()
            .map(|(index, (reward, drawback))| {
                serde_json::json!({
                    "action_id": format!("event_option:neow:index:3:{index}"),
                    "kind": "choose_event_option",
                    "event_id": "Neow",
                    "option_index": index,
                    "option_text": "localized display text is not identity",
                    "neow_reward_type": reward,
                    "neow_drawback": drawback,
                })
            })
            .collect::<Vec<_>>();
        let mut frame = unsupported_run_frame(actions);
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({
            "decision": {
                "kind": "event",
                "event_id": "Neow",
                "event_screen_index": 3,
                "event_identity": {
                    "status": "supported",
                    "event_id": "Neow",
                    "runtime_class": "com.megacrit.cardcrawl.neow.NeowEvent"
                }
            }
        });

        validate_run_decision_surface(&frame).unwrap();
        let handle = action_handle(51, 2, &frame.action_space.actions[2]).unwrap();
        assert_eq!(handle.neow_reward_type.as_deref(), Some("TWO_FIFTY_GOLD"));
        assert_eq!(handle.neow_drawback.as_deref(), Some("TEN_PERCENT_HP_LOSS"));

        frame.action_space.actions[2]["neow_reward_type"] = Value::Null;
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("reward identity"));

        frame.action_space.actions[2]["neow_reward_type"] =
            Value::String("TWO_FIFTY_GOLD".to_owned());
        frame.action_space.actions[2]["option_index"] = Value::from(1);
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("option index"));

        frame.action_space.actions[2]["option_index"] = Value::from(2);
        frame.action_space.actions[2]["event_id"] = Value::String("another.Event".to_owned());
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("changed identity"));

        frame.action_space.actions[2]["event_id"] = Value::String("Neow".to_owned());
        frame.public_base["decision"]["event_identity"]["event_id"] =
            Value::String("BigFish".to_owned());
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("producer-owned identity"));
    }

    #[test]
    fn scenario_neow_surface_grants_only_the_closed_constructor() {
        let mut frame = unsupported_run_frame(vec![serde_json::json!({
            "action_id": "scenario_create_combat",
            "kind": "scenario_create_combat",
            "parameter_schema": {
                "schema": "communicationmod.parity-combat-recipe.v2",
                "provenance": "constructed_validated",
                "scenario_id_format": "canonical_uuid",
                "case_id_format": "lowercase_ascii_slug_max_96",
                "recipe_id": "fresh_neow_to_combat_root.v1",
                "capability_registry": "communicationmod.parity-combat-capabilities.v3",
                "required_capabilities": [
                    "combat.encounter.exact.v1",
                    "player.health.v1",
                    "player.master_deck.exact.v1"
                ],
                "optional_capabilities": [
                    "player.relics.fresh_exact.v1",
                    "player.potions.fresh_exact.v1"
                ],
                "phase": "prepare_before_atomic_commit",
                "commit": "single_combat_root_transition"
            }
        })]);
        frame.profile = SCENARIO_CONTROL_PROFILE.to_owned();
        frame.grants = Grants::for_profile(SCENARIO_CONTROL_PROFILE).unwrap();
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({
            "run": {"act": 1, "floor": 0},
            "decision": {
                "kind": "event",
                "event_id": "Neow",
                "event_identity": {
                    "status": "supported",
                    "event_id": "Neow",
                    "runtime_class": "com.megacrit.cardcrawl.neow.NeowEvent"
                }
            }
        });

        validate_scenario_decision_surface(&frame).unwrap();

        frame.action_space.actions[0] = serde_json::json!({
            "action_id": "event_option:neow:0",
            "kind": "choose_event_option"
        });
        assert!(validate_scenario_decision_surface(&frame).is_err());
    }

    #[test]
    fn scenario_context_rejects_untyped_extensions() {
        let mut context = serde_json::json!({
            "schema": "communicationmod.parity-combat-recipe.v2",
            "provenance": "constructed_validated",
            "scenario_id": "123e4567-e89b-12d3-a456-426614174000",
            "case_id": "cultist-base-root",
            "recipe_id": "fresh_neow_to_combat_root.v1",
            "capability_registry": "communicationmod.parity-combat-capabilities.v1",
            "capabilities": [
                {
                    "capability_id": "combat.encounter.exact.v1",
                    "payload": {"encounter_id": "Cultist"}
                },
                {
                    "capability_id": "player.health.v1",
                    "payload": {"current_hp": 80, "max_hp": 80}
                },
                {
                    "capability_id": "player.master_deck.exact.v1",
                    "payload": {"cards": [{"card_id": "Strike_R", "upgrades": 0}]}
                }
            ]
        });
        validate_scenario_context(&context).unwrap();

        context["rng_override"] = serde_json::json!(true);
        assert!(validate_scenario_context(&context).is_err());
    }

    #[test]
    fn scenario_reset_is_parameter_free_and_requires_constructed_context() {
        let mut frame = unsupported_run_frame(vec![
            serde_json::json!({"action_id": "end_turn", "kind": "end_turn"}),
            serde_json::json!({
                "action_id": "scenario_reset_to_main_menu",
                "kind": "scenario_reset_to_main_menu"
            }),
        ]);
        frame.profile = SCENARIO_CONTROL_PROFILE.to_owned();
        frame.grants = Grants::for_profile(SCENARIO_CONTROL_PROFILE).unwrap();
        frame.boundary.kind = "combat_player_decision".to_owned();
        frame.action_space.kind = "combat_action_space".to_owned();
        frame.scenario = Some(serde_json::json!({
            "schema": "communicationmod.parity-combat-recipe.v2",
            "provenance": "constructed_validated",
            "scenario_id": "123e4567-e89b-12d3-a456-426614174000",
            "case_id": "cultist-base-root",
            "recipe_id": "fresh_neow_to_combat_root.v1",
            "capability_registry": "communicationmod.parity-combat-capabilities.v1",
            "capabilities": [
                {
                    "capability_id": "combat.encounter.exact.v1",
                    "payload": {"encounter_id": "Cultist"}
                },
                {
                    "capability_id": "player.health.v1",
                    "payload": {"current_hp": 80, "max_hp": 80}
                },
                {
                    "capability_id": "player.master_deck.exact.v1",
                    "payload": {"cards": [{"card_id": "Strike_R", "upgrades": 0}]}
                }
            ]
        }));
        validate_scenario_decision_surface(&frame).unwrap();

        frame.action_space.actions[1]["parameter_schema"] = serde_json::json!({});
        assert!(validate_scenario_decision_surface(&frame).is_err());
        frame.action_space.actions[1]
            .as_object_mut()
            .unwrap()
            .remove("parameter_schema");
        frame.scenario = None;
        assert!(validate_scenario_decision_surface(&frame).is_err());
    }

    #[test]
    fn actual_seek_frame_reaches_closed_atomic_submission_without_a_second_scope_list() {
        let frame: RunControlFrame =
            serde_json::from_str(include_str!("../tests/fixtures/seek/open.raw.json")).unwrap();
        validate_run_decision_surface(&frame).unwrap();
        let index = frame
            .action_space
            .actions
            .iter()
            .position(|a| a["kind"] == "submit_grid_select")
            .unwrap();
        let handle =
            action_handle(frame.frame_id, index, &frame.action_space.actions[index]).unwrap();
        let uuid = frame.public_base["decision"]["grid_select"]["candidate_cards"][0]["card_uuid"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(grid_select_parameters(&handle, &[uuid]).is_ok());
        assert!(grid_select_parameters(&handle, &["not-from-this-grid".to_owned()]).is_err());
    }

    #[test]
    fn natural_orrery_native_return_and_retained_reward_reopen_are_validated() {
        let mut frame: RunControlFrame =
            serde_json::from_str(include_str!("../tests/fixtures/shop_orrery/open.raw.json"))
                .unwrap();
        validate_run_decision_surface(&frame).unwrap();
        let return_action = frame
            .action_space
            .actions
            .iter()
            .find(|a| a["kind"] == "close_shop_rewards")
            .unwrap()
            .clone();
        frame.parity_internal["shop"] = Value::Null;
        assert!(validate_run_decision_surface(&frame).is_err());
        frame.parity_internal["shop"] = serde_json::json!({});
        frame.public_base["decision"]["kind"] = serde_json::json!("shop");
        frame.action_space.actions =
            vec![serde_json::json!({"kind":"open_shop_rewards","action_id":"open_shop_rewards"})];
        validate_run_decision_surface(&frame).unwrap();
        frame.action_space.actions.extend([
            serde_json::json!({"kind":"enter_shop","action_id":"enter_shop"}),
            serde_json::json!({"kind":"proceed_shop_room","action_id":"proceed_shop_room"}),
        ]);
        frame.public_base["decision"]["kind"] = serde_json::json!("shop_entry");
        validate_run_decision_surface(&frame).unwrap();
        frame.parity_internal["combat_rewards"] = serde_json::json!([]);
        assert!(validate_run_decision_surface(&frame).is_err());
        frame.action_space.actions = vec![return_action];
        assert!(validate_run_decision_surface(&frame).is_err());
    }

    #[test]
    fn scenario_combat_reward_accepts_only_claim_or_close_actions() {
        let claim = serde_json::json!({
            "action_id": "claim_combat_reward:gold:0",
            "kind": "claim_combat_reward"
        });
        let close = serde_json::json!({
            "action_id": "close_combat_reward",
            "kind": "close_combat_reward"
        });
        let mut frame = unsupported_run_frame(vec![claim, close]);
        frame.profile = SCENARIO_CONTROL_PROFILE.to_owned();
        frame.grants = Grants::for_profile(SCENARIO_CONTROL_PROFILE).unwrap();
        frame.boundary.kind = "run_decision".to_owned();
        frame.public_base = serde_json::json!({
            "run": {"act": 1, "floor": 1},
            "decision": {"kind": "combat_reward"}
        });
        validate_scenario_decision_surface(&frame).unwrap();

        frame.action_space.actions.push(serde_json::json!({
            "action_id": "choose_card_reward:0",
            "kind": "choose_card_reward"
        }));
        assert!(validate_scenario_decision_surface(&frame)
            .unwrap_err()
            .contains("does not match its typed decision"));
    }

    #[test]
    fn natural_run_reset_is_parameter_free_and_valid_at_stable_run_decisions() {
        let reset = serde_json::json!({
            "action_id": "run_reset_to_main_menu_preserving_save",
            "kind": "run_reset_to_main_menu_preserving_save"
        });
        let mut frame = unsupported_run_frame(vec![
            serde_json::json!({"action_id": "end_turn", "kind": "end_turn"}),
            reset.clone(),
        ]);
        frame.boundary.kind = "combat_player_decision".to_owned();
        frame.action_space.kind = "combat_action_space".to_owned();
        frame.public_base = serde_json::json!({
            "run": {"act": 1, "floor": 1},
            "decision": {"kind": "combat"}
        });
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions[1]["parameter_schema"] = serde_json::json!({});
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("parameter-free"));
        frame.action_space.actions[1]
            .as_object_mut()
            .unwrap()
            .remove("parameter_schema");
        frame.boundary.kind = "run_decision".to_owned();
        frame.action_space.kind = "run_action_space".to_owned();
        frame.public_base["decision"] = serde_json::json!({"kind": "map"});
        frame.action_space.actions = vec![
            serde_json::json!({
                "action_id": "map_node:0:15",
                "kind": "choose_map_node",
                "map_x": 0,
                "map_y": 15,
                "map_room_type": "com.megacrit.cardcrawl.rooms.MonsterRoomBoss"
            }),
            reset,
        ];
        validate_run_decision_surface(&frame).unwrap();
    }

    #[test]
    fn natural_run_discard_is_parameter_free_and_valid_at_stable_run_decisions() {
        let discard = serde_json::json!({
            "action_id": "run_discard_to_main_menu",
            "kind": "run_discard_to_main_menu"
        });
        let mut frame = unsupported_run_frame(vec![
            serde_json::json!({"action_id": "end_turn", "kind": "end_turn"}),
            discard,
        ]);
        frame.boundary.kind = "combat_player_decision".to_owned();
        frame.action_space.kind = "combat_action_space".to_owned();
        frame.public_base = serde_json::json!({
            "run": {"act": 1, "floor": 1},
            "decision": {"kind": "combat"}
        });
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions[1]["parameter_schema"] = serde_json::json!({});
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("parameter-free"));
        frame.action_space.actions[1]
            .as_object_mut()
            .unwrap()
            .remove("parameter_schema");
        frame.boundary.kind = "run_decision".to_owned();
        frame.action_space.kind = "run_action_space".to_owned();
        frame.public_base["decision"] = serde_json::json!({"kind": "map"});
        frame.action_space.actions[0] = serde_json::json!({
            "action_id": "map_node:1:0",
            "kind": "choose_map_node",
            "map_x": 1,
            "map_y": 0,
            "map_room_type": "com.megacrit.cardcrawl.rooms.MonsterRoom"
        });
        validate_run_decision_surface(&frame).unwrap();
    }

    #[test]
    fn run_control_rng_witness_must_bind_the_adjacent_transition() {
        let mut frame = unsupported_run_frame(Vec::new());
        frame.rng_call_witness = Some(RngCallWitness {
            kind: "rng_call_witness_v1".to_owned(),
            schema: "sts-parity-natural-run-rng-call-ledger-v1".to_owned(),
            from_frame_id: 1,
            to_frame_id: 2,
            request_id: "request".to_owned(),
            action_id: "action".to_owned(),
            complete: true,
            calls: Vec::new(),
            unsupported_reason: None,
        });
        validate_rng_call_witness_envelope(&frame).unwrap();

        frame.rng_call_witness.as_mut().unwrap().to_frame_id = 3;
        assert!(validate_rng_call_witness_envelope(&frame)
            .unwrap_err()
            .contains("adjacent live frame transition"));
    }

    fn unsupported_run_frame(actions: Vec<Value>) -> RunControlFrame {
        RunControlFrame {
            kind: "state_frame".to_owned(),
            protocol: ProtocolDescriptor {
                name: "communicationmod.parity".to_owned(),
                major: 1,
                minor: sts_java_parity_protocol::PROTOCOL_MINOR,
                schema_id: "communicationmod.parity-frame.v1".to_owned(),
            },
            producer_session_id: "producer".to_owned(),
            connection_id: "connection".to_owned(),
            capture_id: "capture".to_owned(),
            frame_id: 2,
            previous_frame_id: Some(1),
            profile: RUN_CONTROL_PROFILE.to_owned(),
            grants: Grants::for_profile(RUN_CONTROL_PROFILE).unwrap(),
            boundary: RunControlBoundary {
                kind: "unsupported_run_decision".to_owned(),
                stable: true,
                capture_point: "post_update_waiting_for_command".to_owned(),
                evidence: serde_json::json!({}),
            },
            public_base: serde_json::json!({
                "decision": {"kind": "unsupported", "choice_type": "GRID"}
            }),
            parity_internal: serde_json::json!({}),
            rng_internal: serde_json::json!({}),
            action_space: GenericActionSpace {
                kind: "run_action_space".to_owned(),
                actions,
            },
            rng_call_witness: None,
            execution_input_witness: None,
            card_birth_witness: None,
            scenario: None,
        }
    }

    #[test]
    fn main_menu_continue_is_a_unique_content_profile_bound_run_action() {
        let continue_action = serde_json::json!({
            "action_id": "continue_run",
            "kind": "continue_run",
            "parameter_schema": {
                "content_profile": ["current_profile", "all_unlocked"],
                "content_profile_required": true
            }
        });
        let mut frame = unsupported_run_frame(vec![continue_action.clone()]);
        frame.boundary.kind = "main_menu_ready".to_owned();
        frame.public_base = serde_json::json!({"decision": {"kind": "main_menu_ready"}});
        validate_run_decision_surface(&frame).unwrap();

        frame.action_space.actions.push(serde_json::json!({
            "action_id": "start_run",
            "kind": "start_run",
            "parameter_schema": {"type": "object"}
        }));
        assert!(validate_run_decision_surface(&frame)
            .unwrap_err()
            .contains("exactly one start-or-continue"));
    }

    #[test]
    fn settlement_identity_ignores_frame_revision_but_requires_typed_state_change() {
        let source = serde_json::json!({
            "frame_id": 24,
            "previous_frame_id": 23,
            "boundary": {"kind": "run_decision", "evidence": {"wait_timer": 0.0}},
            "public_base": {"run": {"floor": 1}, "decision": {"kind": "map"}},
            "parity_internal": {"zones": {}},
            "rng_internal": {"run": []},
            "action_space": {"kind": "run_action_space", "actions": [{"action_id": "map_node:6:1"}]},
            "scenario": null
        });
        let mut repeated = source.clone();
        repeated["frame_id"] = Value::from(25);
        repeated["previous_frame_id"] = Value::from(24);
        repeated["boundary"]["evidence"]["wait_timer"] = Value::from(-0.1);
        assert_eq!(
            state_frame_settlement_identity(&source),
            state_frame_settlement_identity(&repeated)
        );

        let mut successor = repeated;
        successor["boundary"]["kind"] = Value::from("combat_player_decision");
        successor["public_base"] = serde_json::json!({"run": {"floor": 2}, "combat": {}});
        successor["action_space"] = serde_json::json!({
            "kind": "combat_action_space",
            "actions": [{"action_id": "end_turn"}]
        });
        assert_ne!(
            state_frame_settlement_identity(&source),
            state_frame_settlement_identity(&successor)
        );
    }

    #[test]
    fn rejected_frame_retains_exact_input_outside_verified_frames_and_transcript() {
        let directory = std::env::temp_dir().join(format!("sts-java-live-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let paths = create_session_paths(&directory, 1, &Uuid::new_v4().to_string()).unwrap();
        let raw = "{ \"kind\":\"state_frame\",\"name\":\"占卜\" }";
        save_rejected_frame(&paths, raw, "unsupported grid").unwrap();
        let saved: Value = serde_json::from_slice(
            &fs::read(paths.session_dir.join("rejected-frame.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(saved["raw_line"], raw);
        assert_eq!(saved["reason"], "unsupported grid");
        assert_eq!(fs::read_dir(&paths.frames_dir).unwrap().count(), 0);
        assert!(!paths.transcript.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn create_new_publication_refuses_overwrite() {
        let directory = std::env::temp_dir().join(format!("sts-java-live-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("receipt.json");
        publish_json_create_new(&path, &serde_json::json!({"value": 1})).unwrap();
        let error = publish_json_create_new(&path, &serde_json::json!({"value": 2})).unwrap_err();
        assert!(error.contains("failed to publish"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn accelerated_launch_evidence_binds_plan_selected_mod_and_bridge_endpoint() {
        let directory = std::env::temp_dir().join(format!("sts-java-live-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let plan_path = directory.join("launch-plan.json");
        let working_directory = std::env::current_dir().unwrap();
        let bridge_executable = std::env::current_exe().unwrap();
        let bridge_length = fs::metadata(&bridge_executable).unwrap().len();
        let bridge_sha256 = sha256_file(&bridge_executable).unwrap();
        fs::write(
            &plan_path,
            serde_json::to_vec(&serde_json::json!({
                "schema": "sts_java_play_launch_plan_v4",
                "profile": "accelerated",
                "control_profile": "parity_run_control",
                "selected_mod_ids": ["basemod", "superfastmode", "CommunicationMod"],
                "selected_mods": [
                    {"id": "basemod", "version": "1.0.0", "jar": {"sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}},
                    {"id": "superfastmode", "version": "1.0.8", "jar": {"sha256": "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"}},
                    {"id": "CommunicationMod", "version": "1.2.1", "jar": {"sha256": "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC"}}
                ],
                "bridge": {
                    "kind": "communication_mod_stdio_v1",
                    "executable": {
                        "path": bridge_executable.to_string_lossy(),
                        "length": bridge_length,
                        "sha256": bridge_sha256
                    },
                    "output_directory": directory.to_string_lossy()
                },
                "storage": {
                    "class": "installed_game",
                    "profile_substrate": "installed_profile_v1",
                    "tutorial_policy": "observe_and_acknowledge_v1",
                    "working_directory": working_directory,
                    "source_game_directory": working_directory,
                    "isolation_id": null
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let evidence =
            validate_launch_evidence("accelerated", RUN_CONTROL_PROFILE, &plan_path, &directory)
                .unwrap();
        assert_eq!(evidence.profile, "accelerated");
        assert_eq!(evidence.control_profile, RUN_CONTROL_PROFILE);
        assert_eq!(evidence.plan_sha256.len(), 64);
        assert_eq!(evidence.storage_class, "installed_game");
        assert_eq!(evidence.isolation_id, None);
        validate_launch_loaded_mods(
            &evidence,
            &[
                LoadedModIdentity {
                    id: "CommunicationMod".to_owned(),
                    version: "1.2.1".to_owned(),
                    jar_sha256: "C".repeat(64),
                },
                LoadedModIdentity {
                    id: "basemod".to_owned(),
                    version: "1.0.0".to_owned(),
                    jar_sha256: "A".repeat(64),
                },
                LoadedModIdentity {
                    id: "superfastmode".to_owned(),
                    version: "1.0.8".to_owned(),
                    jar_sha256: "B".repeat(64),
                },
            ],
        )
        .unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn tutorial_policy_is_bound_to_its_profile_substrate() {
        validate_profile_tutorial_pair(
            "canonical_tutorial_complete_preferences_v1",
            "precompleted_via_profile_v1",
        )
        .unwrap();
        validate_profile_tutorial_pair(
            "canonical_a20h_lab_preferences_v1",
            "precompleted_via_profile_v1",
        )
        .unwrap();
        validate_profile_tutorial_pair("captured_native_profile_v1", "observe_and_acknowledge_v1")
            .unwrap();
        validate_profile_tutorial_pair(
            "captured_native_lab_preferences_v1",
            "precompleted_via_profile_v1",
        )
        .unwrap();

        assert!(validate_profile_tutorial_pair(
            "captured_native_profile_v1",
            "precompleted_via_profile_v1",
        )
        .is_err());
        assert!(validate_profile_tutorial_pair(
            "canonical_tutorial_complete_preferences_v1",
            "observe_and_acknowledge_v1",
        )
        .is_err());
    }

    #[cfg(windows)]
    #[test]
    fn launch_directory_identity_accepts_the_windows_extended_prefix_alias() {
        let current = std::env::current_dir().unwrap();
        let ordinary = current.to_string_lossy();
        assert!(!ordinary.starts_with(r"\\?\"));
        let extended = PathBuf::from(format!(r"\\?\{ordinary}"));
        assert!(same_existing_directory(&current, &extended).unwrap());
    }

    #[test]
    fn launch_evidence_rejects_profile_mod_mismatch() {
        let directory = std::env::temp_dir().join(format!("sts-java-live-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let plan_path = directory.join("launch-plan.json");
        fs::write(
            &plan_path,
            serde_json::to_vec(&serde_json::json!({
                "schema": "sts_java_play_launch_plan_v3",
                "profile": "strict",
                "control_profile": "parity_combat_control",
                "selected_mod_ids": ["basemod", "superfastmode", "CommunicationMod"]
            }))
            .unwrap(),
        )
        .unwrap();

        let error = validate_launch_evidence("strict", CONTROL_PROFILE, &plan_path, &directory)
            .unwrap_err();
        assert!(error.contains("selected Mods"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn scenario_launch_evidence_rejects_installed_game_storage() {
        let directory = std::env::temp_dir().join(format!("sts-java-live-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let plan_path = directory.join("launch-plan.json");
        let working_directory = std::env::current_dir().unwrap();
        fs::write(
            &plan_path,
            serde_json::to_vec(&serde_json::json!({
                "schema": "sts_java_play_launch_plan_v3",
                "profile": "strict",
                "control_profile": SCENARIO_CONTROL_PROFILE,
                "selected_mod_ids": ["basemod", "CommunicationMod"],
                "storage": {
                    "class": "installed_game",
                    "profile_substrate": "installed_profile_v1",
                    "tutorial_policy": "observe_and_acknowledge_v1",
                    "working_directory": working_directory,
                    "source_game_directory": working_directory,
                    "isolation_id": null
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let error =
            validate_launch_evidence("strict", SCENARIO_CONTROL_PROFILE, &plan_path, &directory)
                .unwrap_err();
        assert!(
            error.contains("requires isolated launch storage"),
            "{error}"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn historical_launch_evidence_without_storage_fields_remains_readable() {
        let evidence: LaunchEvidence = serde_json::from_value(serde_json::json!({
            "profile": "strict",
            "control_profile": "parity_run_control",
            "plan_path": "launch-plan.json",
            "plan_sha256": "A"
        }))
        .unwrap();

        assert!(evidence.storage_class.is_empty());
        assert!(evidence.working_directory.is_empty());
        assert!(evidence.source_game_directory.is_empty());
        assert_eq!(evidence.isolation_id, None);
    }

    #[test]
    fn rejected_producer_result_is_preserved_in_receipt() {
        let directory = std::env::temp_dir().join(format!("sts-java-live-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let receipt_path = directory.join("receipt.json");
        let producer_result = serde_json::json!({
            "kind": "action_result",
            "status": "rejected",
            "code": "stale_frame"
        });
        let manifest = SessionManifest {
            schema: "sts_java_parity_live_session_v1".to_owned(),
            session_id: Uuid::new_v4().to_string(),
            created_at_unix_ms: 0,
            process_id: std::process::id(),
            session_dir: directory.to_string_lossy().into_owned(),
            capture_id: Uuid::new_v4().to_string(),
            producer_session_id: Uuid::new_v4().to_string(),
            connection_id: Uuid::new_v4().to_string(),
            profile: CONTROL_PROFILE.to_owned(),
            source_commit: "0".repeat(40),
            source_dirty: false,
            loaded_mod_jar_sha256: "A".repeat(64),
            sts_jar_sha256: "B".repeat(64),
            mod_the_spire_jar_sha256: "C".repeat(64),
            launch: None,
        };
        let pending = PendingRequest {
            request: ClientActionRequest {
                schema: "sts_java_parity_live_action_request_v2".to_owned(),
                session_id: manifest.session_id.clone(),
                connection_id: manifest.connection_id.clone(),
                request_id: Uuid::new_v4().to_string(),
                created_at_unix_ms: 0,
                based_on_frame_id: 1,
                action_id: "end_turn".to_owned(),
                parameters: empty_object(),
            },
            receipt_path: receipt_path.clone(),
            sent_at: Instant::now(),
            producer_result: Some(producer_result.clone()),
            source_frame_identity: None,
        };

        complete_request(
            &manifest,
            pending,
            "rejected",
            Some("stale_frame".to_owned()),
            "rejected",
            None,
            None,
        )
        .unwrap();

        let receipt: ActionReceipt = read_json(&receipt_path).unwrap();
        assert_eq!(receipt.producer_result, Some(producer_result));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_live_root_explains_how_to_start() {
        let directory = std::env::temp_dir().join(format!("sts-java-live-{}", Uuid::new_v4()));
        let error = discover_live_session(&directory).err().unwrap();
        assert!(error.contains("java-play.ps1 launch"), "{error}");
    }
}
