use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use sts_java_parity_protocol::{
    Boundary, Hello, HelloAck, ProducerIdentity, RngInternal, StateFrame,
    SNAPSHOT_PROFILE as PROFILE,
};
use uuid::Uuid;

const CLIENT_NAME: &str = "sts_java_parity_capture";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_PROTOCOL_LINE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub artifact_path: PathBuf,
    pub receipt_path: PathBuf,
    pub frame_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturePaths {
    pub artifact_path: PathBuf,
    pub receipt_path: PathBuf,
}

pub fn derive_fresh_capture_paths(
    directory: &Path,
    label: &str,
    capture_id: Uuid,
    created_at_unix_ms: u128,
) -> Result<CapturePaths, String> {
    if label.is_empty()
        || label.len() > 64
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(
            "capture label must contain 1-64 ASCII letters, digits, hyphens, or underscores"
                .to_owned(),
        );
    }
    let stem = format!("{label}-{created_at_unix_ms}-{}", capture_id.simple());
    Ok(CapturePaths {
        artifact_path: directory.join(format!("{stem}.jsonl")),
        receipt_path: directory.join(format!("{stem}.receipt.json")),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureReceipt {
    pub schema: String,
    pub status: String,
    pub completed_at_unix_ms: u128,
    pub capture_id: String,
    pub producer_session_id: String,
    pub connection_id: String,
    pub source_commit: String,
    pub source_dirty: bool,
    pub loaded_mod_jar_sha256: String,
    pub sts_jar_sha256: String,
    pub mod_the_spire_jar_sha256: String,
    pub requested_frames: usize,
    pub captured_frames: usize,
    pub first_frame_id: u64,
    pub last_frame_id: u64,
    pub collector: CollectorReceipt,
    pub artifact: ArtifactReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectorReceipt {
    pub name: String,
    pub version: String,
    pub executable_path: String,
    pub executable_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactReceipt {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

pub fn capture_with_io<R: BufRead, W: Write>(
    config: &CaptureConfig,
    input: &mut R,
    protocol_output: &mut W,
    capture_id: Uuid,
) -> Result<CaptureReceipt, String> {
    validate_paths(config)?;
    let artifact_path = absolute_path(&config.artifact_path)?;
    let receipt_path = absolute_path(&config.receipt_path)?;
    ensure_parent(&artifact_path)?;
    ensure_parent(&receipt_path)?;
    if receipt_path.exists() {
        return Err(format!(
            "completion receipt already exists: {}",
            receipt_path.display()
        ));
    }

    let mut artifact = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&artifact_path)
        .map_err(|error| {
            format!(
                "failed to create fresh capture artifact {}: {error}",
                artifact_path.display()
            )
        })?;
    let mut artifact_hash = Sha256::new();
    let mut artifact_bytes = 0_u64;

    let capture_id_text = capture_id.to_string();
    let hello = Hello::new(CLIENT_NAME, CLIENT_VERSION, &capture_id_text, PROFILE)?;
    let hello_line = serde_json::to_string(&hello)
        .map_err(|error| format!("failed to serialize parity hello: {error}"))?;
    append_artifact_line(
        &mut artifact,
        &mut artifact_hash,
        &mut artifact_bytes,
        &hello_line,
    )?;
    write_protocol_line(protocol_output, &hello_line)?;

    let ack_line = read_protocol_line(input)?;
    let ack: HelloAck = serde_json::from_str(&ack_line)
        .map_err(|error| format!("producer did not return a valid hello_ack: {error}"))?;
    validate_ack(&ack, &capture_id_text)?;
    append_artifact_line(
        &mut artifact,
        &mut artifact_hash,
        &mut artifact_bytes,
        &ack_line,
    )?;

    let mut previous_frame_id = None;
    for index in 0..config.frame_count {
        let frame_line = read_protocol_line(input)?;
        let frame: StateFrame = serde_json::from_str(&frame_line)
            .map_err(|error| format!("producer returned an invalid state_frame: {error}"))?;
        validate_frame(&frame, &ack, previous_frame_id, index)?;
        previous_frame_id = Some(frame.frame_id);
        append_artifact_line(
            &mut artifact,
            &mut artifact_hash,
            &mut artifact_bytes,
            &frame_line,
        )?;
    }

    artifact
        .sync_all()
        .map_err(|error| format!("failed to sync capture artifact: {error}"))?;
    let artifact_sha256 = format!("{:X}", artifact_hash.finalize());
    let last_frame_id = previous_frame_id.ok_or_else(|| "no frame was captured".to_owned())?;
    let executable_path = std::env::current_exe()
        .map_err(|error| format!("failed to locate collector executable: {error}"))?
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize collector executable: {error}"))?;
    let receipt = CaptureReceipt {
        schema: "sts_java_parity_capture_receipt_v1".to_owned(),
        status: "completed".to_owned(),
        completed_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock precedes UNIX epoch: {error}"))?
            .as_millis(),
        capture_id: capture_id_text,
        producer_session_id: ack.producer_session_id,
        connection_id: ack.connection_id,
        source_commit: ack.producer.source_commit,
        source_dirty: ack.producer.source_dirty,
        loaded_mod_jar_sha256: ack.producer.loaded_mod_jar_sha256,
        sts_jar_sha256: ack.producer.sts_jar_sha256,
        mod_the_spire_jar_sha256: ack.producer.mod_the_spire_jar_sha256,
        requested_frames: config.frame_count,
        captured_frames: config.frame_count,
        first_frame_id: 1,
        last_frame_id,
        collector: CollectorReceipt {
            name: CLIENT_NAME.to_owned(),
            version: CLIENT_VERSION.to_owned(),
            executable_path: executable_path.to_string_lossy().into_owned(),
            executable_sha256: sha256_file(&executable_path)?,
        },
        artifact: ArtifactReceipt {
            path: artifact_path.to_string_lossy().into_owned(),
            bytes: artifact_bytes,
            sha256: artifact_sha256,
        },
    };
    write_receipt(&receipt_path, &receipt)?;
    Ok(receipt)
}

fn validate_paths(config: &CaptureConfig) -> Result<(), String> {
    if config.frame_count == 0 {
        return Err("--frames must be greater than zero".to_owned());
    }
    let artifact = absolute_path(&config.artifact_path)?;
    let receipt = absolute_path(&config.receipt_path)?;
    if artifact == receipt {
        return Err("capture artifact and completion receipt must be different files".to_owned());
    }
    Ok(())
}

fn validate_ack(ack: &HelloAck, capture_id: &str) -> Result<(), String> {
    if ack.kind != "hello_ack" {
        return Err(format!("expected hello_ack, received {}", ack.kind));
    }
    ack.protocol.validate()?;
    parse_uuid("producer_session_id", &ack.producer_session_id)?;
    parse_uuid("connection_id", &ack.connection_id)?;
    if ack.capture_id != capture_id {
        return Err("hello_ack capture_id does not echo this collector".to_owned());
    }
    if ack.profile != PROFILE {
        return Err("hello_ack profile is not parity_snapshot".to_owned());
    }
    ack.grants.validate_for(PROFILE)?;
    validate_producer(&ack.producer)
}

fn validate_producer(producer: &ProducerIdentity) -> Result<(), String> {
    validate_hex("source_commit", &producer.source_commit, 40)?;
    validate_hex(
        "source_worktree_sha256",
        &producer.source_worktree_sha256,
        64,
    )?;
    validate_non_blank("build_timestamp", &producer.build_timestamp)?;
    validate_non_blank("project_version", &producer.project_version)?;
    validate_hex("loaded_mod_jar_sha256", &producer.loaded_mod_jar_sha256, 64)?;
    validate_non_blank("sts_version", &producer.sts_version)?;
    validate_hex("sts_jar_sha256", &producer.sts_jar_sha256, 64)?;
    validate_non_blank("mod_the_spire_version", &producer.mod_the_spire_version)?;
    validate_hex(
        "mod_the_spire_jar_sha256",
        &producer.mod_the_spire_jar_sha256,
        64,
    )?;
    let mut ids = BTreeSet::new();
    let mut own_jar_sha256 = None;
    for loaded_mod in &producer.loaded_mods {
        validate_non_blank("loaded mod id", &loaded_mod.id)?;
        validate_non_blank("loaded mod version", &loaded_mod.version)?;
        validate_hex("loaded mod jar_sha256", &loaded_mod.jar_sha256, 64)?;
        if !ids.insert(loaded_mod.id.as_str()) {
            return Err(format!("duplicate loaded mod id: {}", loaded_mod.id));
        }
        if loaded_mod.id == "CommunicationMod" {
            own_jar_sha256 = Some(loaded_mod.jar_sha256.as_str());
        }
    }
    if own_jar_sha256 != Some(producer.loaded_mod_jar_sha256.as_str()) {
        return Err(
            "loaded_mod_jar_sha256 does not match the CommunicationMod manifest entry".to_owned(),
        );
    }
    Ok(())
}

fn validate_frame(
    frame: &StateFrame,
    ack: &HelloAck,
    previous_frame_id: Option<u64>,
    frame_index: usize,
) -> Result<(), String> {
    if frame.kind != "state_frame" {
        return Err(format!("expected state_frame, received {}", frame.kind));
    }
    frame.protocol.validate()?;
    if frame.producer_session_id != ack.producer_session_id
        || frame.connection_id != ack.connection_id
        || frame.capture_id != ack.capture_id
    {
        return Err("state_frame identity does not match hello_ack".to_owned());
    }
    let expected_frame_id =
        u64::try_from(frame_index).map_err(|_| "frame index does not fit u64".to_owned())? + 1;
    if frame.frame_id != expected_frame_id || frame.previous_frame_id != previous_frame_id {
        return Err(format!(
            "broken frame sequence: expected frame {expected_frame_id} after {previous_frame_id:?}, got {} after {:?}",
            frame.frame_id, frame.previous_frame_id
        ));
    }
    if frame.profile != PROFILE {
        return Err("state_frame profile is not parity_snapshot".to_owned());
    }
    frame.grants.validate_for(PROFILE)?;
    validate_boundary(&frame.boundary)?;
    if !frame.public_base.run.is_object() || !frame.public_base.combat.is_object() {
        return Err("public_base run and combat must be objects".to_owned());
    }
    if frame.parity_internal.run_seed.is_none() {
        return Err("parity_internal run_seed is unavailable".to_owned());
    }
    if !frame.parity_internal.zones.is_object() {
        return Err("parity_internal zones must be an object".to_owned());
    }
    validate_rng(&frame.rng_internal)
}

fn validate_boundary(boundary: &Boundary) -> Result<(), String> {
    if boundary.kind != "combat_player_decision"
        || !boundary.stable
        || boundary.capture_point != "basemod_post_update_before_input_update_last"
    {
        return Err("state_frame does not declare the Stage 1 stable boundary".to_owned());
    }
    let evidence = &boundary.evidence;
    let stable = evidence.in_dungeon
        && evidence.room_phase == "COMBAT"
        && evidence.room_wait_timer <= 0.0
        && !evidence.screen_up
        && !evidence.fading_in
        && !evidence.fading_out
        && !evidence.battle_over
        && evidence.player_alive
        && !evidence.monsters_basically_dead
        && evidence.action_manager_phase == "WAITING_ON_USER"
        && evidence.current_action_class.is_none()
        && evidence.actions_pending == 0
        && evidence.pre_turn_actions_pending == 0
        && evidence.card_queue_pending == 0
        && evidence.monster_queue_pending == 0
        && !evidence.using_card
        && !evidence.end_turn_queued
        && !evidence.turn_has_ended
        && evidence.monster_attacks_queued
        && !evidence.mod_update_blocked;
    let _recorded_but_not_required = evidence.action_manager_has_control;
    if !stable {
        return Err("state_frame boundary evidence is not stable".to_owned());
    }
    Ok(())
}

fn validate_rng(rng: &RngInternal) -> Result<(), String> {
    let expected = [
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
    ];
    if rng.streams.len() != expected.len() {
        return Err("rng_internal must contain exactly six streams".to_owned());
    }
    let mut owners = BTreeSet::new();
    for stream in &rng.streams {
        let Some((_, expected_scope)) = expected
            .iter()
            .find(|(owner, _)| *owner == stream.owner.as_str())
        else {
            return Err(format!("unexpected RNG owner: {}", stream.owner));
        };
        if !owners.insert(stream.owner.as_str()) {
            return Err(format!("duplicate RNG owner: {}", stream.owner));
        }
        if stream.algorithm != "com.badlogic.gdx.math.RandomXS128"
            || stream.scope != *expected_scope
            || stream.counter < 0
            || stream.state_hex.len() != 2
        {
            return Err(format!("invalid RNG metadata for {}", stream.owner));
        }
        for state in &stream.state_hex {
            validate_hex("RNG state", state, 16)?;
        }
    }
    Ok(())
}

fn append_artifact_line(
    artifact: &mut File,
    hasher: &mut Sha256,
    byte_count: &mut u64,
    line: &str,
) -> Result<(), String> {
    let bytes = line.as_bytes();
    artifact
        .write_all(bytes)
        .and_then(|_| artifact.write_all(b"\n"))
        .and_then(|_| artifact.flush())
        .map_err(|error| format!("failed to append capture artifact: {error}"))?;
    hasher.update(bytes);
    hasher.update(b"\n");
    *byte_count += u64::try_from(bytes.len() + 1)
        .map_err(|_| "capture artifact byte count overflowed u64".to_owned())?;
    Ok(())
}

fn write_protocol_line<W: Write>(output: &mut W, line: &str) -> Result<(), String> {
    output
        .write_all(line.as_bytes())
        .and_then(|_| output.write_all(b"\n"))
        .and_then(|_| output.flush())
        .map_err(|error| format!("failed to send hello to CommunicationMod: {error}"))
}

fn read_protocol_line<R: BufRead>(input: &mut R) -> Result<String, String> {
    let mut bytes = Vec::new();
    loop {
        let available = input
            .fill_buf()
            .map_err(|error| format!("failed to read CommunicationMod output: {error}"))?;
        if available.is_empty() {
            return Err("CommunicationMod output reached EOF before capture completed".to_owned());
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
    String::from_utf8(bytes).map_err(|_| "CommunicationMod output was not valid UTF-8".to_owned())
}

fn write_receipt(path: &Path, receipt: &CaptureReceipt) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "completion receipt has no UTF-8 file name: {}",
                path.display()
            )
        })?;
    let temporary_path = path.with_file_name(format!(".{file_name}.{}.pending", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(|error| {
            format!(
                "failed to create temporary completion receipt {}: {error}",
                temporary_path.display()
            )
        })?;
    let write_result = (|| {
        serde_json::to_writer_pretty(&mut file, receipt)
            .map_err(|error| format!("failed to encode completion receipt: {error}"))?;
        file.write_all(b"\n")
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("failed to sync completion receipt: {error}"))
    })();
    drop(file);
    let publish_result = write_result.and_then(|_| {
        fs::hard_link(&temporary_path, path).map_err(|error| {
            format!(
                "failed to publish fresh completion receipt {}: {error}",
                path.display()
            )
        })
    });
    let cleanup_result = fs::remove_file(&temporary_path).map_err(|error| {
        format!(
            "failed to remove temporary completion receipt {}: {error}",
            temporary_path.display()
        )
    });
    publish_result.and(cleanup_result)
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

fn ensure_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create directory {}: {error}", parent.display()))
}

fn parse_uuid(name: &str, value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| format!("{name} is not a UUID"))
}

fn validate_non_blank(name: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{name} is blank"))
    } else {
        Ok(())
    }
}

fn validate_hex(name: &str, value: &str, width: usize) -> Result<(), String> {
    if value.len() != width || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} must be exactly {width} hexadecimal characters"
        ));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open {} for hashing: {error}", path.display()))?;
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

#[cfg(test)]
mod tests;
