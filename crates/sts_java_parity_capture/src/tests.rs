use super::*;
use serde_json::{json, Value};
use std::io::Cursor;
use sts_java_parity_protocol::{
    FRAME_SCHEMA_ID, OBSERVATION_GRANTS, PROTOCOL_MAJOR, PROTOCOL_MINOR, PROTOCOL_NAME,
};

const CAPTURE_ID: &str = "123e4567-e89b-12d3-a456-426614174000";
const PRODUCER_ID: &str = "223e4567-e89b-12d3-a456-426614174000";
const CONNECTION_ID: &str = "323e4567-e89b-12d3-a456-426614174000";
const HEX_64: &str = "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF";

#[test]
fn output_directory_derives_unique_create_new_paths() {
    let capture_id = Uuid::parse_str("11111111-2222-4333-8444-555555555555").unwrap();
    let paths =
        derive_fresh_capture_paths(Path::new("captures"), "targeted-strike", capture_id, 42)
            .unwrap();

    assert_eq!(
        paths.artifact_path,
        Path::new("captures").join("targeted-strike-42-11111111222243338444555555555555.jsonl")
    );
    assert_eq!(
        paths.receipt_path,
        Path::new("captures")
            .join("targeted-strike-42-11111111222243338444555555555555.receipt.json")
    );
}

#[test]
fn output_directory_rejects_path_like_labels() {
    let error = derive_fresh_capture_paths(Path::new("captures"), "../escape", Uuid::nil(), 42)
        .unwrap_err();

    assert!(error.contains("capture label"), "{error}");
}

#[test]
fn captures_valid_sequence_and_writes_completion_receipt() {
    let sandbox = sandbox("complete");
    let config = config(&sandbox, 2);
    let input = format!("{}\n{}\n{}\n", ack(), frame(1, None), frame(2, Some(1)));
    let mut input = Cursor::new(input.into_bytes());
    let mut protocol_output = Vec::new();

    let receipt = capture_with_io(
        &config,
        &mut input,
        &mut protocol_output,
        Uuid::parse_str(CAPTURE_ID).expect("capture UUID"),
    )
    .expect("capture succeeds");

    assert_eq!(receipt.captured_frames, 2);
    assert_eq!(receipt.first_frame_id, 1);
    assert_eq!(receipt.last_frame_id, 2);
    assert_eq!(receipt.collector.name, CLIENT_NAME);
    validate_hex(
        "collector executable hash",
        &receipt.collector.executable_sha256,
        64,
    )
    .expect("collector hash");
    assert!(config.receipt_path.is_file());
    let artifact_bytes = fs::read(&config.artifact_path).expect("artifact bytes");
    assert_eq!(receipt.artifact.bytes, artifact_bytes.len() as u64);
    assert_eq!(
        receipt.artifact.sha256,
        format!("{:X}", Sha256::digest(&artifact_bytes))
    );
    let artifact = String::from_utf8(artifact_bytes).expect("UTF-8 artifact");
    assert_eq!(artifact.lines().count(), 4);
    let hello: Value = serde_json::from_slice(&protocol_output).expect("hello");
    assert_eq!(hello["kind"], "hello");
    assert_eq!(hello["client"]["capture_id"], CAPTURE_ID);
    fs::remove_dir_all(sandbox).expect("remove owned test sandbox");
}

#[test]
fn broken_frame_sequence_never_creates_completion_receipt() {
    let sandbox = sandbox("broken-sequence");
    let config = config(&sandbox, 2);
    let input = format!("{}\n{}\n", ack(), frame(2, None));
    let mut input = Cursor::new(input.into_bytes());
    let mut protocol_output = Vec::new();

    let error = capture_with_io(
        &config,
        &mut input,
        &mut protocol_output,
        Uuid::parse_str(CAPTURE_ID).expect("capture UUID"),
    )
    .expect_err("sequence must fail");

    assert!(error.contains("broken frame sequence"));
    assert!(!config.receipt_path.exists());
    let artifact = fs::read_to_string(&config.artifact_path).expect("partial artifact");
    assert_eq!(artifact.lines().count(), 2);
    fs::remove_dir_all(sandbox).expect("remove owned test sandbox");
}

#[test]
fn unsettled_boundary_never_creates_completion_receipt() {
    let sandbox = sandbox("unsettled-boundary");
    let config = config(&sandbox, 1);
    let mut unsettled: Value = serde_json::from_str(&frame(1, None)).expect("frame fixture");
    unsettled["boundary"]["evidence"]["card_queue_pending"] = json!(1);
    let input = format!("{}\n{}\n", ack(), unsettled);
    let mut input = Cursor::new(input.into_bytes());
    let mut protocol_output = Vec::new();

    let error = capture_with_io(
        &config,
        &mut input,
        &mut protocol_output,
        Uuid::parse_str(CAPTURE_ID).expect("capture UUID"),
    )
    .expect_err("unsettled boundary must fail");

    assert!(error.contains("boundary evidence is not stable"));
    assert!(!config.receipt_path.exists());
    fs::remove_dir_all(sandbox).expect("remove owned test sandbox");
}

fn config(sandbox: &Path, frame_count: usize) -> CaptureConfig {
    CaptureConfig {
        artifact_path: sandbox.join("capture.jsonl"),
        receipt_path: sandbox.join("receipt.json"),
        frame_count,
    }
}

fn sandbox(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sts-java-parity-capture-{label}-{}",
        Uuid::new_v4()
    ))
}

fn ack() -> String {
    json!({
        "kind": "hello_ack",
        "protocol": protocol(),
        "producer_session_id": PRODUCER_ID,
        "connection_id": CONNECTION_ID,
        "capture_id": CAPTURE_ID,
        "profile": PROFILE,
        "grants": grants(),
        "producer": {
            "source_commit": "0123456789abcdef0123456789abcdef01234567",
            "source_dirty": false,
            "source_worktree_sha256": HEX_64,
            "build_timestamp": "2026-08-28T00:00:00Z",
            "project_version": "1.2.1",
            "loaded_mod_jar_sha256": HEX_64,
            "sts_version": "11-30-2020",
            "sts_jar_sha256": HEX_64,
            "mod_the_spire_version": "3.18.1",
            "mod_the_spire_jar_sha256": HEX_64,
            "loaded_mods": [
                {"id": "CommunicationMod", "version": "1.2.1", "jar_sha256": HEX_64},
                {"id": "basemod", "version": "5.27.0", "jar_sha256": HEX_64}
            ]
        }
    })
    .to_string()
}

fn frame(frame_id: u64, previous_frame_id: Option<u64>) -> String {
    json!({
        "kind": "state_frame",
        "protocol": protocol(),
        "producer_session_id": PRODUCER_ID,
        "connection_id": CONNECTION_ID,
        "capture_id": CAPTURE_ID,
        "frame_id": frame_id,
        "previous_frame_id": previous_frame_id,
        "profile": PROFILE,
        "grants": grants(),
        "boundary": {
            "kind": "combat_player_decision",
            "stable": true,
            "capture_point": "basemod_post_update_before_input_update_last",
            "evidence": {
                "in_dungeon": true,
                "room_phase": "COMBAT",
                "room_wait_timer": 0.0,
                "screen_up": false,
                "fading_in": false,
                "fading_out": false,
                "battle_over": false,
                "player_alive": true,
                "monsters_basically_dead": false,
                "action_manager_phase": "WAITING_ON_USER",
                "current_action_class": null,
                "actions_pending": 0,
                "pre_turn_actions_pending": 0,
                "card_queue_pending": 0,
                "monster_queue_pending": 0,
                "using_card": false,
                "end_turn_queued": false,
                "turn_has_ended": false,
                "monster_attacks_queued": true,
                "action_manager_has_control": false,
                "mod_update_blocked": false
            }
        },
        "public_base": {"run": {}, "combat": {}},
        "parity_internal": {"run_seed": "0", "zones": {}},
        "rng_internal": {"streams": rng_streams()}
    })
    .to_string()
}

fn protocol() -> Value {
    json!({
        "name": PROTOCOL_NAME,
        "major": PROTOCOL_MAJOR,
        "minor": PROTOCOL_MINOR,
        "schema_id": FRAME_SCHEMA_ID
    })
}

fn grants() -> Value {
    json!({
        "observation": OBSERVATION_GRANTS,
        "control": []
    })
}

fn rng_streams() -> Vec<Value> {
    [
        (
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.aiRng",
            "floor",
        ),
        (
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.shuffleRng",
            "floor",
        ),
        (
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRandomRng",
            "floor",
        ),
        (
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.miscRng",
            "floor",
        ),
        (
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterHpRng",
            "floor",
        ),
        (
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.potionRng",
            "run",
        ),
    ]
    .into_iter()
    .map(|(owner, scope)| {
        json!({
            "owner": owner,
            "algorithm": "com.badlogic.gdx.math.RandomXS128",
            "scope": scope,
            "state_hex": ["0123456789ABCDEF", "FEDCBA9876543210"],
            "counter": 0
        })
    })
    .collect()
}
