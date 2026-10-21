//! Original autosave generations and short, already adopted recovery suffixes.
//! A saved UI page is never an anchor. Metadata selects a candidate from raw
//! evidence; only current-code replay plus a full live comparison authorizes it.

use crate::{progress::progress_phase, LockstepProgressV1};
use crate::{replay_manual_session_prefix_v1, ManualSessionV1};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use sts_oracle_run_control::eval::run_control::RunControlSessionCheckpointV1;
use sts_parity_lockstep::{
    compare_snapshots_v1, LockstepAdapterV1, LockstepModeV1, LockstepRunnerV1,
    NaturalRunBootstrapV1, ParitySnapshotV1, ProducerIdentityV1, SemanticActionV1,
};
use sts_parity_lockstep_java::{
    JavaLockstepAdapterV1, JavaLockstepResumeStateV1, JavaLockstepTransportV1,
    JavaRuntimeObservationV1, LeasedJavaLockstepTransportV1, RecordedJavaEvidenceStoreV1,
    RecordedJavaEvidenceV1,
};
use sts_parity_lockstep_rust::RustLockstepAdapterV1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct NativeSaveMetadataV1 {
    seed: i64,
    act_num: u64,
    floor_num: i64,
    room_x: i64,
    room_y: i64,
    current_room: String,
    post_combat: bool,
    chose_neow_reward: bool,
}

impl NativeSaveMetadataV1 {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(&decode_native_save(bytes)?)
            .map_err(|e| format!("invalid native autosave metadata: {e}"))
    }

    fn matches_candidate(&self, observation: &JavaRuntimeObservationV1) -> bool {
        let frame = &observation.frame;
        let run = &frame["public_base"]["run"];
        let map = &frame["public_base"]["map"];
        // Original Neow's MapRoomNode is (0, -1); the public map has no
        // selected graph node and exports (-1, -1). Scope this correspondence
        // to the actual Neow class and entrance, never arbitrary sentinels.
        let neow = self.current_room == "com.megacrit.cardcrawl.neow.NeowRoom";
        let coordinates = if neow {
            if self.act_num != 1 || self.floor_num != 0 || (self.room_x, self.room_y) != (0, -1) {
                return false;
            }
            (-1, -1)
        } else {
            (self.room_x, self.room_y)
        };
        if frame["parity_internal"]["run_seed"].as_str() != Some(self.seed.to_string().as_str())
            || run["act"].as_u64() != Some(self.act_num)
            || run["floor"].as_i64() != Some(self.floor_num)
            || map["current_x"].as_i64() != Some(coordinates.0)
            || map["current_y"].as_i64() != Some(coordinates.1)
        {
            return false;
        }
        let decision = frame["public_base"]["decision"]["kind"].as_str();
        if self.post_combat {
            return decision == Some("combat_reward");
        }
        match self.current_room.as_str() {
            "com.megacrit.cardcrawl.rooms.TreasureRoomBoss" => decision == Some("boss_relic"),
            "com.megacrit.cardcrawl.rooms.VictoryRoom" => {
                decision == Some("event")
                    && frame["public_base"]["decision"]["event_id"].as_str() == Some("SpireHeart")
                    && frame["public_base"]["decision"]["event_screen_index"].as_i64() == Some(0)
            }
            "com.megacrit.cardcrawl.rooms.MonsterRoom"
            | "com.megacrit.cardcrawl.rooms.MonsterRoomElite"
            | "com.megacrit.cardcrawl.rooms.MonsterRoomBoss" => {
                frame["boundary"]["kind"].as_str() == Some("combat_player_decision")
            }
            "com.megacrit.cardcrawl.rooms.EventRoom" => {
                // ENTER_ROOM is written before EventHelper.roll replaces a
                // question-mark room with its event, combat, shop or treasure.
                // These are candidate anchors; replay still proves the result.
                matches!(decision, Some("event" | "shop" | "treasure"))
                    || frame["boundary"]["kind"].as_str() == Some("combat_player_decision")
            }
            "com.megacrit.cardcrawl.rooms.ShopRoom" => decision == Some("shop"),
            "com.megacrit.cardcrawl.rooms.RestRoom" => decision == Some("campfire"),
            "com.megacrit.cardcrawl.rooms.TreasureRoom" => decision == Some("treasure"),
            "com.megacrit.cardcrawl.neow.NeowRoom" => {
                self.chose_neow_reward
                    && decision == Some("event")
                    && frame["public_base"]["decision"]["event_id"].as_str() == Some("Neow")
                    && frame["public_base"]["decision"]["event_screen_index"].as_i64() == Some(99)
            }
            _ => false,
        }
    }
}

fn decode_native_save(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoded = STANDARD
        .decode(bytes)
        .map_err(|e| format!("invalid native autosave encoding: {e}"))?;
    for (i, byte) in decoded.iter_mut().enumerate() {
        *byte ^= b"key"[i % 3];
    }
    Ok(decoded)
}

/// SaveFile(POST_COMBAT) writes EventHelper.getChancesPreRoll even when the
/// finished room is an ordinary fight after a previous event. Continue then
/// loads stale probabilities without rerolling that previous event. Such bytes
/// are evidence, but cannot anchor an equivalent continuation. EventRoom has
/// a different reload path and is deliberately not qualified by this check.
fn native_event_chances_preserved(
    bytes: &[u8],
    metadata: &NativeSaveMetadataV1,
    observation: &JavaRuntimeObservationV1,
) -> Result<bool, String> {
    if !metadata.post_combat
        || !matches!(
            metadata.current_room.as_str(),
            "com.megacrit.cardcrawl.rooms.MonsterRoom"
                | "com.megacrit.cardcrawl.rooms.MonsterRoomElite"
                | "com.megacrit.cardcrawl.rooms.MonsterRoomBoss"
        )
    {
        return Ok(true);
    }
    let save: serde_json::Value =
        serde_json::from_slice(&decode_native_save(bytes)?).map_err(|e| e.to_string())?;
    let chances = save["event_chances"]
        .as_array()
        .filter(|values| values.len() == 4)
        .ok_or("native POST_COMBAT save omitted its four event chances")?;
    let observed =
        &observation.frame["parity_internal"]["run_state"]["event_schedule"]["chance_bits"];
    for (chance, name) in chances.iter().zip(["elite", "monster", "shop", "treasure"]) {
        let chance = chance
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or("native event chance is not a finite number")? as f32;
        let bits = observed[name]
            .as_u64()
            .ok_or("native source omitted event chance bits")?;
        if u64::from(chance.to_bits()) != bits {
            return Ok(false);
        }
    }
    Ok(true)
}

/// AbstractDungeon's save constructor initializes shrineList after loadSave;
/// SaveFile never persists this collection. Reject a known lossy anchor before
/// touching Java. This is only a candidate filter: live full-state comparison
/// still has to prove the selected entry and every adopted suffix action.
fn native_shrine_pool_preserved(
    metadata: &NativeSaveMetadataV1,
    observation: &JavaRuntimeObservationV1,
) -> Result<bool, String> {
    let act = u8::try_from(metadata.act_num)
        .ok()
        .filter(|act| (1..=4).contains(act))
        .ok_or("native shrine pool has an unsupported Act")?;
    let observed = observation.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"]
        .as_array()
        .ok_or("native source omitted the active shrine pool")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(sts_java_parity_protocol::canonical_game_id)
                .ok_or("native source has an invalid shrine identity")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if observed
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != observed.len()
    {
        return Err("native source repeats a shrine identity".to_owned());
    }
    let daily = observation.frame["parity_internal"]["run_state"]["daily"]
        .as_bool()
        .ok_or("native source omitted the daily flag")?;
    if let Some(exit) = sts_parity_lockstep_java::java_act_exit_v1(&observation.frame) {
        let expected_room = match exit {
            sts_parity_lockstep::NaturalActExitV1::BossRelicChoice => {
                "com.megacrit.cardcrawl.rooms.TreasureRoomBoss"
            }
            sts_parity_lockstep::NaturalActExitV1::SpireHeart => {
                "com.megacrit.cardcrawl.rooms.VictoryRoom"
            }
        };
        if metadata.current_room == expected_room && exit.retires_shrines(metadata.act_num, daily) {
            return Ok(true);
        }
    }
    let restored = sts_oracle_run_control::state::events::generator::EventGenerator::new(act)
        .shrine_pool
        .iter()
        .map(|event| sts_java_parity_protocol::canonical_game_id(event.stable_id()))
        .collect::<Vec<_>>();
    Ok(observed == restored)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeGenerationManifestV1 {
    schema: String,
    autosave_sha256: String,
    backup_sha256: Option<String>,
    autosave_name: String,
    metadata: NativeSaveMetadataV1,
    source_session_id: String,
    source_committed_actions: usize,
    source_observation_sha256: String,
    source_producer: ProducerIdentityV1,
}

#[derive(Clone, Debug)]
pub struct NativeGenerationV1 {
    directory: PathBuf,
    manifest: NativeGenerationManifestV1,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

fn saves_directory(storage: &Path) -> Result<PathBuf, String> {
    let storage = storage.canonicalize().map_err(|e| e.to_string())?;
    fs::create_dir_all(storage.join("saves")).map_err(|e| e.to_string())?;
    let saves = storage
        .join("saves")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !saves.starts_with(&storage) {
        return Err("native saves escaped isolated storage".to_owned());
    }
    Ok(saves)
}

pub(crate) fn safe_autosave_name(name: &str) -> bool {
    ["IRONCLAD", "THE_SILENT", "DEFECT", "WATCHER"]
        .iter()
        .any(|class| autosave_name_matches_class(name, class))
}

pub(crate) fn autosave_name_matches_class(name: &str, class: &str) -> bool {
    !name.contains(['/', '\\', ':'])
        && ["IRONCLAD", "THE_SILENT", "DEFECT", "WATCHER"].contains(&class)
        && (name == format!("{class}.autosave") || name.ends_with(&format!("_{class}.autosave")))
}

pub fn capture_native_generation_if_ready_v1(
    repository: &Path,
    session: &ManualSessionV1,
    storage: &Path,
) -> Result<(), String> {
    let evidence = RecordedJavaEvidenceV1::load_complete(&session.evidence_directory())?;
    let tail = evidence
        .steps
        .last()
        .map(|s| &s.after.observation)
        .unwrap_or(&evidence.initial.observation);
    // Cold attachment can start at the same terminal boundary that the hot
    // path already excluded. Original victory has deleted the live autosave;
    // the durable generation remains the recovery source.
    if tail.frame["boundary"]["kind"].as_str() == Some("terminal") {
        return Ok(());
    }
    if tail.frame["public_base"]["run"]["floor"].as_i64() == Some(0)
        && tail.frame["public_base"]["decision"]["event_screen_index"].as_i64() != Some(99)
    {
        return Ok(());
    }
    capture_native_generation_v1(repository, session, storage).map(|_| ())
}

/// Capture from the live writer only after the runner has durably committed an
/// Exact transition. Cold attachment/recovery still validates the full source.
pub fn capture_manual_checkpoints_after_step_v1<T: JavaLockstepTransportV1>(
    session: &ManualSessionV1,
    storage: &Path,
    runner: &LockstepRunnerV1<JavaLockstepAdapterV1<T>, RustLockstepAdapterV1>,
) -> Result<(), String> {
    let _timing = sts_parity_lockstep::timing_span_v1("native.capture");
    if runner.is_stopped()
        || !matches!(
            runner.last_transition_outcome(),
            Some(sts_parity_lockstep::TransitionOutcomeV1::Continued {
                with_coverage_gap: false
            })
        )
    {
        return Err("native capture requires a journal-committed Exact step".to_owned());
    }
    let step = runner
        .java_adapter()
        .last_recorded_step_v1()
        .ok_or("native capture has no complete recorded Java step")?;
    capture_native_generation_from_step(session, storage, &step)?;
    if crate::is_natural_combat_entry_transition_v1(step.before(), step.after()) {
        if let Some((act, floor)) =
            crate::manual_natural_combat_entry_coordinate_v1(runner.current_snapshots().0)?
        {
            session.capture_seeded_combat_checkpoint(
                storage,
                step.committed_actions(),
                act,
                floor,
            )?;
        }
    }
    Ok(())
}

fn capture_native_generation_from_step(
    session: &ManualSessionV1,
    storage: &Path,
    step: &sts_parity_lockstep_java::RecordedJavaStepRefV1<'_>,
) -> Result<(), String> {
    if step.directory() != session.evidence_directory() {
        return Err("native capture step belongs to another evidence writer".to_owned());
    }
    let Some(post_combat) = save_scope_transition(step.before(), step.after()) else {
        return Ok(());
    };
    capture_native_generation_at_anchor(
        session,
        storage,
        session,
        step.committed_actions(),
        step.after(),
        post_combat,
    )
    .map(|_| ())
}

/// A cold JVM can have empty isolated storage. Its recovery input comes from
/// the selected trajectory's durable generation store, not the launch profile.
pub fn load_latest_native_generation_v1(
    repository: &Path,
    session: &ManualSessionV1,
) -> Result<NativeGenerationV1, String> {
    load_latest_native_generation_for_scope(repository, session, None)
}

fn load_latest_native_generation_for_scope(
    repository: &Path,
    session: &ManualSessionV1,
    mut limit: Option<usize>,
) -> Result<NativeGenerationV1, String> {
    // Load each adopted segment once. Candidate manifests are only an index;
    // the selected anchor still needs its raw source and current-code replay.
    // In particular, a parent's later abandoned suffix is not on this branch.
    struct AdoptedSegment {
        owner: ManualSessionV1,
        observations: Vec<JavaRuntimeObservationV1>,
        following_actions: usize,
    }
    let mut segments = Vec::<AdoptedSegment>::new();
    let mut current = session.clone();
    let mut following_actions = 0usize;
    loop {
        if segments.len() >= 64
            || segments
                .iter()
                .any(|segment| segment.owner.session_id() == current.session_id())
        {
            return Err("native generation ancestry is cyclic or too deep".to_owned());
        }
        let evidence = match limit {
            Some(count) => {
                RecordedJavaEvidenceV1::load_prefix(&current.evidence_directory(), count)?
            }
            None => RecordedJavaEvidenceV1::load_complete(&current.evidence_directory())?,
        };
        let root = current.load_source_prefix()?;
        let total_actions = following_actions
            .checked_add(evidence.steps.len())
            .ok_or("native adopted trajectory length overflow")?;
        segments.push(AdoptedSegment {
            owner: current,
            observations: evidence
                .steps
                .into_iter()
                .map(|step| step.after.observation)
                .collect(),
            following_actions,
        });
        let Some(root) = root else { break };
        following_actions = total_actions;
        limit = Some(root.source_committed_actions);
        current = ManualSessionV1::load_existing(repository, &root.source_session_id)?;
    }

    let mut candidates = Vec::new();
    for segment in &segments {
        let directory = segment.owner.directory().join("native-generations");
        if directory.is_dir() {
            for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if !entry.path().join("generation.json").is_file() {
                    continue;
                }
                let generation = NativeGenerationV1::load(&entry.path())?;
                if let Some((index, source)) = segments.iter().enumerate().find(|(_, source)| {
                    source.owner.session_id() == generation.manifest.source_session_id
                }) {
                    if let Some(local_remaining) = source
                        .observations
                        .len()
                        .checked_sub(generation.manifest.source_committed_actions)
                    {
                        candidates.push((
                            source.following_actions + local_remaining,
                            index,
                            generation,
                        ));
                    }
                }
            }
        }
    }
    candidates.sort_by_key(|(remaining, _, _)| *remaining);
    let mut selected: Option<(usize, NativeGenerationV1)> = None;
    for (remaining, index, generation) in candidates {
        if selected
            .as_ref()
            .is_some_and(|(shortest, _)| remaining > *shortest)
        {
            break;
        }
        let observation =
            &segments[index].observations[generation.manifest.source_committed_actions - 1];
        generation.validate_source_observation(observation)?;
        if !native_shrine_pool_preserved(&generation.manifest.metadata, observation)?
            || !native_event_chances_preserved(
                &fs::read(generation.directory.join("autosave")).map_err(|e| e.to_string())?,
                &generation.manifest.metadata,
                observation,
            )?
        {
            continue;
        }
        if let Some((_, other)) = &selected {
            if other.manifest.autosave_sha256 != generation.manifest.autosave_sha256 {
                return Err("selected native anchor has ambiguous save generations".to_owned());
            }
        } else {
            selected = Some((remaining, generation));
        }
    }
    selected
        .map(|(_, generation)| generation)
        .ok_or_else(|| "selected trajectory has no faithful durable native generation".to_owned())
}

/// A content-addressed, immutable copy of the actual native generation. Equal
/// bytes reuse the same anchor even when the live page has advanced to another act.
pub fn capture_native_generation_v1(
    repository: &Path,
    session: &ManualSessionV1,
    storage: &Path,
) -> Result<NativeGenerationV1, String> {
    let _timing = sts_parity_lockstep::timing_span_v1("native.capture_cold");
    let (source, count, observation, post_combat) =
        latest_scope_anchor(repository, session, None, &mut Vec::new())?
            .ok_or("selected trajectory has no observed native save boundary")?;
    capture_native_generation_at_anchor(session, storage, &source, count, &observation, post_combat)
}

fn capture_native_generation_at_anchor(
    session: &ManualSessionV1,
    storage: &Path,
    source: &ManualSessionV1,
    count: usize,
    observation: &JavaRuntimeObservationV1,
    post_combat: bool,
) -> Result<NativeGenerationV1, String> {
    let _timing = sts_parity_lockstep::timing_span_v1("native.capture_anchor");
    let saves = saves_directory(storage)?;
    let expected_class = observation.frame["public_base"]["run"]["character"]
        .as_str()
        .ok_or("native capture source omits its character")?;
    let files = fs::read_dir(&saves)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let files = files
        .iter()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_file())
                && autosave_name_matches_class(&e.file_name().to_string_lossy(), expected_class)
        })
        .collect::<Vec<_>>();
    let [entry] = files.as_slice() else {
        return Err("native recovery requires exactly one supported character autosave".to_owned());
    };
    let path = entry.path();
    // The original writer is asynchronous. A stable gameplay frame alone
    // cannot prove the new generation has reached disk. Wait for this scope,
    // never label the previous room's bytes as the new room's checkpoint.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let (bytes, metadata) = loop {
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        if let Ok(metadata) = NativeSaveMetadataV1::decode(&bytes) {
            if metadata.post_combat == post_combat && metadata.matches_candidate(observation) {
                break (bytes, metadata);
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(
                "native writer has not published the selected trajectory's latest save scope"
                    .to_owned(),
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let digest = hash(&bytes);
    let root = session.directory().join("native-generations");
    let directory = root.join(&digest);
    if directory.join("generation.json").is_file() {
        return NativeGenerationV1::load(&directory);
    }
    let name = entry
        .file_name()
        .to_str()
        .ok_or("native save filename is not UTF-8")?
        .to_owned();
    let backup_path = saves.join(format!("{name}.backUp"));
    let backup = if backup_path.is_file() {
        Some(fs::read(&backup_path).map_err(|e| e.to_string())?)
    } else {
        None
    };
    if bytes != fs::read(&path).map_err(|e| e.to_string())? {
        return Err(
            "native autosave changed during capture; retry after its writer settles".to_owned(),
        );
    }
    let manifest = NativeGenerationManifestV1 {
        schema: "sts-parity-native-generation-v1".to_owned(),
        autosave_sha256: digest,
        backup_sha256: backup.as_deref().map(hash),
        autosave_name: name,
        metadata,
        source_session_id: source.session_id().to_owned(),
        source_committed_actions: count,
        source_observation_sha256: hash(
            &serde_json::to_vec(observation).map_err(|e| e.to_string())?,
        ),
        source_producer: observation.producer.clone(),
    };
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let staging_root = session.directory().join("native-generation-staging");
    fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
    let staged = staging_root.join(uuid::Uuid::new_v4().simple().to_string());
    fs::create_dir(&staged).map_err(|e| e.to_string())?;
    write_new(&staged.join("autosave"), &bytes)?;
    if let Some(bytes) = backup {
        write_new(&staged.join("autosave.backUp"), &bytes)?;
    }
    write_new(
        &staged.join("generation.json"),
        &serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
    )?;
    NativeGenerationV1::load(&staged)?;
    // Publish a complete generation at once. An interrupted staging write
    // stays outside discovery and never poisons the next capture attempt.
    let session_root = session
        .directory()
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !staged
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&session_root)
        || !root
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&session_root)
    {
        return Err("native generation publication escaped its session".to_owned());
    }
    fs::rename(staged, &directory).map_err(|e| e.to_string())?;
    NativeGenerationV1::load(&directory)
}

fn latest_scope_anchor(
    repository: &Path,
    session: &ManualSessionV1,
    limit: Option<usize>,
    visited: &mut Vec<String>,
) -> Result<Option<(ManualSessionV1, usize, JavaRuntimeObservationV1, bool)>, String> {
    if visited.len() >= 64 || visited.iter().any(|id| id == session.session_id()) {
        return Err("native source provenance is cyclic or too deep".to_owned());
    }
    visited.push(session.session_id().to_owned());
    let evidence = match limit {
        Some(limit) => RecordedJavaEvidenceV1::load_prefix(&session.evidence_directory(), limit)?,
        None => RecordedJavaEvidenceV1::load_complete(&session.evidence_directory())?,
    };
    let mut found = if let Some(root) = session.load_source_prefix()? {
        let parent = ManualSessionV1::load_existing(repository, &root.source_session_id)?;
        latest_scope_anchor(
            repository,
            &parent,
            Some(root.source_committed_actions),
            visited,
        )?
    } else {
        None
    };
    for (i, step) in evidence.steps.iter().enumerate() {
        if let Some(post_combat) =
            save_scope_transition(&step.before.observation, &step.after.observation)
        {
            found = Some((
                session.clone(),
                i + 1,
                step.after.observation.clone(),
                post_combat,
            ));
        }
    }
    visited.pop();
    Ok(found)
}

fn save_scope_transition(
    before: &JavaRuntimeObservationV1,
    after: &JavaRuntimeObservationV1,
) -> Option<bool> {
    // TrueVictoryRoom advances the floor, but VictoryScreen then deletes the
    // autosave. A verified terminal boundary has no recoverable continuation;
    // retain the last real generation instead of waiting for terminal bytes.
    if after.frame["boundary"]["kind"].as_str() == Some("terminal") {
        return None;
    }
    let old = &before.frame["public_base"];
    let new = &after.frame["public_base"];
    let decision = new["decision"]["kind"].as_str();
    if decision == Some("event")
        && new["decision"]["event_id"].as_str() == Some("Neow")
        && new["decision"]["event_screen_index"].as_i64() == Some(99)
        && old["decision"]["event_screen_index"].as_i64() != Some(99)
    {
        return Some(false);
    }
    if decision == Some("combat_reward")
        && (before.frame["boundary"]["kind"].as_str() == Some("combat_player_decision")
            || old["decision"]["kind"].as_str() == Some("hand_select")
            || old["decision"]["card_reward"]["source_kind"].as_str() == Some("combat_discovery"))
    {
        return Some(true);
    }
    if new["run"]["floor"].as_i64().is_some()
        && new["run"]["floor"] != old["run"]["floor"]
        && decision != Some("map")
    {
        return Some(false);
    }
    None
}

impl NativeGenerationV1 {
    fn source_prefix(&self, repository: &Path) -> Result<RecordedJavaEvidenceV1, String> {
        let manifest = &self.manifest;
        let owner = ManualSessionV1::load_existing(repository, &manifest.source_session_id)?;
        let prefix = RecordedJavaEvidenceV1::load_prefix(
            &owner.evidence_directory(),
            manifest.source_committed_actions,
        )?;
        let observation = &prefix
            .steps
            .last()
            .ok_or("native source prefix is empty")?
            .after
            .observation;
        self.validate_source_observation(observation)?;
        Ok(prefix)
    }

    fn validate_source_observation(
        &self,
        observation: &JavaRuntimeObservationV1,
    ) -> Result<(), String> {
        let manifest = &self.manifest;
        if hash(&serde_json::to_vec(observation).map_err(|e| e.to_string())?)
            != manifest.source_observation_sha256
            || observation.producer != manifest.source_producer
            || !manifest.metadata.matches_candidate(observation)
        {
            return Err("native generation source evidence changed".to_owned());
        }
        Ok(())
    }

    fn load(directory: &Path) -> Result<Self, String> {
        let manifest: NativeGenerationManifestV1 = serde_json::from_slice(
            &fs::read(directory.join("generation.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if manifest.schema != "sts-parity-native-generation-v1"
            || !safe_autosave_name(&manifest.autosave_name)
            || manifest.source_committed_actions == 0
        {
            return Err("invalid native generation provenance".to_owned());
        }
        let result = Self {
            directory: directory.to_path_buf(),
            manifest,
        };
        result.validate_bytes()?;
        Ok(result)
    }

    fn validate_bytes(&self) -> Result<(), String> {
        let bytes = fs::read(self.directory.join("autosave")).map_err(|e| e.to_string())?;
        if hash(&bytes) != self.manifest.autosave_sha256
            || NativeSaveMetadataV1::decode(&bytes)? != self.manifest.metadata
        {
            return Err("native generation payload changed".to_owned());
        }
        if let Some(expected) = &self.manifest.backup_sha256 {
            if &hash(&fs::read(self.directory.join("autosave.backUp")).map_err(|e| e.to_string())?)
                != expected
            {
                return Err("native generation backup changed".to_owned());
            }
        }
        Ok(())
    }

    fn restore(&self, storage: &Path) -> Result<(), String> {
        self.validate_bytes()?;
        let saves = saves_directory(storage)?;
        let target = saves.join(&self.manifest.autosave_name);
        let backup = saves.join(format!("{}.backUp", self.manifest.autosave_name));
        // Resolve existing leaf paths too: an external symlink is not owned storage.
        for path in [&target, &backup] {
            if path.exists()
                && !path
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .starts_with(&saves)
            {
                return Err("native restore leaf escaped isolated storage".to_owned());
            }
        }
        let primary_bytes = fs::read(self.directory.join("autosave")).map_err(|e| e.to_string())?;
        if hash(&primary_bytes) != self.manifest.autosave_sha256 {
            return Err("native generation autosave changed before restore".to_owned());
        }
        // The resident game's menu can inspect this path at any time. Never
        // expose a truncating writer (or a partially copied file) at that name.
        // Prepare the fallback before publishing the primary generation.
        if let Some(expected) = &self.manifest.backup_sha256 {
            let bytes =
                fs::read(self.directory.join("autosave.backUp")).map_err(|e| e.to_string())?;
            if &hash(&bytes) != expected {
                return Err("native generation backup changed before restore".to_owned());
            }
            publish_native_save(&backup, &bytes)?;
        } else if backup.is_file() {
            fs::remove_file(backup).map_err(|e| e.to_string())?;
        }
        publish_native_save(&target, &primary_bytes)?;
        if hash(&fs::read(&target).map_err(|e| e.to_string())?) != self.manifest.autosave_sha256 {
            return Err("native restore did not preserve its bytes".to_owned());
        }
        Ok(())
    }
}

/// A same-directory rename publishes a complete, closed file. In particular,
/// Windows sharing violations must fail without falling back to in-place copy.
fn publish_native_save(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = target.with_file_name(format!(
        ".native-restore-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| format!("could not stage native save: {e}"))?;
    let result = (|| {
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, target)
            .map_err(|e| format!("could not atomically publish native save: {e}"))
    })();
    match fs::remove_file(&temporary) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => {
            return Err(format!(
                "native save staging cleanup failed: {error}; {result:?}"
            ))
        }
    }
    result
}

/// Not deserializable: obtained only by verifying the selected trajectory with
/// current Java projection and Rust semantics. The serialized checkpoint is diagnostic.
pub struct VerifiedNativeRecoveryV1 {
    selected_session_id: String,
    generation: NativeGenerationV1,
    bootstrap: NaturalRunBootstrapV1,
    rust: RustLockstepAdapterV1,
    java_resume: JavaLockstepResumeStateV1,
    suffix: Vec<SemanticActionV1>,
    expected_terminal: ParitySnapshotV1,
}

impl VerifiedNativeRecoveryV1 {
    pub fn suffix_actions(&self) -> usize {
        self.suffix.len()
    }
}

/// Seed only a new isolated JVM's original Continue menu. No game state is
/// executed here; the normal native recovery gate still has to prove its entry.
pub fn prepare_manual_native_startup_storage_v1(repository: &Path) -> Result<PathBuf, String> {
    let source = ManualSessionV1::load_active(repository)?;
    let generation = load_latest_native_generation_v1(repository, &source)?;
    // Cold startup only installs a proved native anchor. A stopped suffix may
    // require new observation fields; it is neither replayed nor authorized here.
    let _ = verify_native_anchor_v1(repository, &generation)?;
    let root = source.directory().join("native-startups");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let directory = root.join(uuid::Uuid::new_v4().simple().to_string());
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    generation.restore(&directory)?;
    write_new(
        &directory.join("source.json"),
        &serde_json::to_vec(&generation.manifest).map_err(|e| e.to_string())?,
    )?;
    Ok(directory)
}

pub fn prepare_native_recovery_v1(
    repository: &Path,
    source: &ManualSessionV1,
    generation: NativeGenerationV1,
) -> Result<VerifiedNativeRecoveryV1, String> {
    prepare_native_recovery_with_progress_v1(repository, source, generation, &mut |_| {})
}

pub fn prepare_native_recovery_with_progress_v1(
    repository: &Path,
    source: &ManualSessionV1,
    generation: NativeGenerationV1,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<VerifiedNativeRecoveryV1, String> {
    prepare_native_recovery_scope(
        repository,
        source,
        None,
        generation,
        source.session_id(),
        observer,
    )
}

/// Retry the selected combat through its real native generation and adopted
/// room suffix. Event fights do not create a new original autosave at combat entry.
pub fn prepare_native_combat_retry_with_progress_v1(
    repository: &Path,
    source: &ManualSessionV1,
    target: Option<(&str, usize)>,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<VerifiedNativeRecoveryV1, String> {
    require_selected_session(repository, source.session_id())?;
    let (owner, checkpoint) = match target {
        Some((session_id, count)) => {
            explicit_combat_retry_target(repository, source, session_id, count)?
        }
        None => combat_retry_target(repository, source)?,
    };
    let count = checkpoint.source_committed_actions;
    let generation = load_latest_native_generation_for_scope(repository, &owner, Some(count))?;
    let plan = prepare_native_recovery_scope(
        repository,
        &owner,
        Some(count),
        generation,
        source.session_id(),
        observer,
    )?;
    if crate::manual_natural_combat_entry_coordinate_v1(&plan.expected_terminal)?
        != Some((checkpoint.act, checkpoint.floor))
    {
        return Err("seeded combat target disagrees with current-code replay".to_owned());
    }
    Ok(plan)
}

fn combat_retry_target(
    repository: &Path,
    source: &ManualSessionV1,
) -> Result<(ManualSessionV1, crate::ManualSeededCombatCheckpointV1), String> {
    // Read only a closed entry prefix: a stopped action can have no successor.
    if let Some(checkpoint) = source.latest_seeded_combat_checkpoint()? {
        let evidence = RecordedJavaEvidenceV1::load_prefix(
            &source.evidence_directory(),
            checkpoint.source_committed_actions,
        )?;
        if let Some(entry) = crate::latest_recorded_natural_combat_entry_action_count_v1(&evidence)
        {
            return Ok((source.clone(), source.load_seeded_combat_checkpoint(entry)?));
        }
    }
    let root = source
        .load_source_prefix()?
        .ok_or("manual retry has not reached a shared-Exact combat checkpoint")?;
    let owner = ManualSessionV1::load_existing(repository, &root.source_session_id)?;
    let checkpoint = owner.load_seeded_combat_checkpoint(root.source_committed_actions)?;
    Ok((owner, checkpoint))
}

fn explicit_combat_retry_target(
    repository: &Path,
    selected: &ManualSessionV1,
    session_id: &str,
    count: usize,
) -> Result<(ManualSessionV1, crate::ManualSeededCombatCheckpointV1), String> {
    let owner = ManualSessionV1::load_existing(repository, session_id)?;
    require_adopted_native_prefix(repository, selected.session_id(), &owner, count)?;
    let evidence = RecordedJavaEvidenceV1::load_prefix(&owner.evidence_directory(), count)?;
    if crate::latest_recorded_natural_combat_entry_action_count_v1(&evidence) != Some(count) {
        return Err("explicit retry target is not a recorded combat entrance".to_owned());
    }
    let checkpoint = owner.load_seeded_combat_checkpoint(count)?;
    Ok((owner, checkpoint))
}

/// Prepare a previously adopted boundary without changing the active selector.
/// The target prefix must belong to that selector's typed ancestry.
pub(crate) fn prepare_native_recovery_prefix_v1(
    repository: &Path,
    selected_session_id: &str,
    source: &ManualSessionV1,
    count: usize,
) -> Result<VerifiedNativeRecoveryV1, String> {
    prepare_native_recovery_prefix_with_progress_v1(
        repository,
        selected_session_id,
        source,
        count,
        &mut |_| {},
    )
}

/// Verify an explicitly selected adopted boundary before any Java mutation.
/// Native bytes, ancestry and the exact replayed suffix remain authoritative.
pub fn prepare_native_recovery_prefix_with_progress_v1(
    repository: &Path,
    selected_session_id: &str,
    source: &ManualSessionV1,
    count: usize,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<VerifiedNativeRecoveryV1, String> {
    require_adopted_native_prefix(repository, selected_session_id, source, count)?;
    let generation = load_latest_native_generation_for_scope(repository, source, Some(count))?;
    prepare_native_recovery_scope(
        repository,
        source,
        Some(count),
        generation,
        selected_session_id,
        observer,
    )
}

fn require_adopted_native_prefix(
    repository: &Path,
    selected_session_id: &str,
    source: &ManualSessionV1,
    count: usize,
) -> Result<(), String> {
    require_selected_session(repository, selected_session_id)?;
    let active = ManualSessionV1::load_active(repository)?;
    let scopes = crate::manual_replay::adopted_scopes(repository, &active, None)?;
    let (_, limit) = scopes
        .iter()
        .find(|(owner, _)| owner.session_id() == source.session_id())
        .ok_or("native preparation target is outside the selected trajectory")?;
    if limit.is_some_and(|limit| count > limit) {
        return Err("native preparation target exceeds its adopted prefix".to_owned());
    }
    Ok(())
}

fn prepare_native_recovery_scope(
    repository: &Path,
    source: &ManualSessionV1,
    limit: Option<usize>,
    generation: NativeGenerationV1,
    selected_session_id: &str,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<VerifiedNativeRecoveryV1, String> {
    let _timing = sts_parity_lockstep::timing_span_v1("native.prepare");
    let prefix = progress_phase(observer, "native save provenance", |_| {
        validate_native_anchor_evidence(repository, &generation)
    })?;
    let manifest = &generation.manifest;
    // Visit the anchor and terminal in one current-code replay. Only one
    // process-local adapter pair is cloned, after its boundary and RNG ledger
    // compare Exact. No persistent checkpoint or cached result is trusted.
    let ((_, mut terminal, _), anchor) = crate::manual_replay::replay_manual_scope_observed(
        repository,
        source,
        limit,
        Some((
            &manifest.source_session_id,
            manifest.source_committed_actions,
        )),
        observer,
    )?;
    let (rust, java_resume) = *anchor.ok_or("native replay did not retain its verified anchor")?;
    if !java_resume.matches_observation(
        &prefix
            .steps
            .last()
            .ok_or("native source prefix is empty")?
            .after
            .observation,
    ) {
        return Err("native source observation changed during preparation".to_owned());
    }
    let expected_terminal = terminal.observe().map_err(|e| e.message)?;
    let suffix = progress_phase(observer, "adopted suffix selection", |_| {
        adopted_suffix(
            repository,
            source,
            limit,
            &manifest.source_session_id,
            manifest.source_committed_actions,
            &mut Vec::new(),
        )
    })?;
    Ok(VerifiedNativeRecoveryV1 {
        selected_session_id: selected_session_id.to_owned(),
        generation,
        bootstrap: prefix.manifest.bootstrap,
        rust,
        java_resume,
        suffix,
        expected_terminal,
    })
}

fn verify_native_anchor_v1(
    repository: &Path,
    generation: &NativeGenerationV1,
) -> Result<
    (
        NaturalRunBootstrapV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    let prefix = validate_native_anchor_evidence(repository, generation)?;
    let manifest = &generation.manifest;
    let owner = ManualSessionV1::load_existing(repository, &manifest.source_session_id)?;
    let (_, rust, java_resume) =
        replay_manual_session_prefix_v1(repository, &owner, manifest.source_committed_actions)?;
    Ok((prefix.manifest.bootstrap, rust, java_resume))
}

fn validate_native_anchor_evidence(
    repository: &Path,
    generation: &NativeGenerationV1,
) -> Result<RecordedJavaEvidenceV1, String> {
    generation.validate_bytes()?;
    let manifest = &generation.manifest;
    let prefix = generation.source_prefix(repository)?;
    let observation = &prefix
        .steps
        .last()
        .ok_or("native source prefix is empty")?
        .after
        .observation;
    if !native_shrine_pool_preserved(&manifest.metadata, observation)? {
        return Err(
            "native save would reset the consumed shrine pool; use an earlier generation"
                .to_owned(),
        );
    }
    if !native_event_chances_preserved(
        &fs::read(generation.directory.join("autosave")).map_err(|e| e.to_string())?,
        &manifest.metadata,
        observation,
    )? {
        return Err(
            "native POST_COMBAT save would rewind event probabilities; use an earlier generation"
                .to_owned(),
        );
    }
    Ok(prefix)
}

fn adopted_suffix(
    repository: &Path,
    session: &ManualSessionV1,
    limit: Option<usize>,
    anchor_session: &str,
    anchor_count: usize,
    visited: &mut Vec<String>,
) -> Result<Vec<SemanticActionV1>, String> {
    if visited.len() >= 64 || visited.iter().any(|id| id == session.session_id()) {
        return Err("native suffix provenance is cyclic or too deep".to_owned());
    }
    visited.push(session.session_id().to_owned());
    let evidence = match limit {
        Some(n) => RecordedJavaEvidenceV1::load_prefix(&session.evidence_directory(), n)?,
        None => RecordedJavaEvidenceV1::load_complete(&session.evidence_directory())?,
    };
    let (mut actions, skip) = if session.session_id() == anchor_session {
        if anchor_count > evidence.steps.len() {
            return Err("native anchor lies beyond the adopted source prefix".to_owned());
        }
        (Vec::new(), anchor_count)
    } else {
        let root = session
            .load_source_prefix()?
            .ok_or("native generation is not an ancestor of the selected trajectory")?;
        let parent = ManualSessionV1::load_existing(repository, &root.source_session_id)?;
        (
            adopted_suffix(
                repository,
                &parent,
                Some(root.source_committed_actions),
                anchor_session,
                anchor_count,
                visited,
            )?,
            0,
        )
    };
    actions.extend(evidence.steps.into_iter().skip(skip).map(|s| s.action));
    visited.pop();
    Ok(actions)
}

pub type NativeRecoveredRunnerV1 =
    LockstepRunnerV1<JavaLockstepAdapterV1<LeasedJavaLockstepTransportV1>, RustLockstepAdapterV1>;

/// Continues the exact original bytes, proves the native entry, then executes
/// only the already adopted suffix. A failure never publishes the new session.
pub fn recover_native_v1(
    repository: &Path,
    transport: LeasedJavaLockstepTransportV1,
    plan: VerifiedNativeRecoveryV1,
) -> Result<(ManualSessionV1, NativeRecoveredRunnerV1, PathBuf), String> {
    recover_native_with_progress_v1(repository, transport, plan, &mut |_| {})
}

pub fn recover_native_with_progress_v1(
    repository: &Path,
    transport: LeasedJavaLockstepTransportV1,
    plan: VerifiedNativeRecoveryV1,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<(ManualSessionV1, NativeRecoveredRunnerV1, PathBuf), String> {
    recover_native_with_publication(repository, transport, plan, true, observer)
}

pub(crate) fn recover_native_for_preparation_v1(
    repository: &Path,
    transport: LeasedJavaLockstepTransportV1,
    plan: VerifiedNativeRecoveryV1,
) -> Result<(ManualSessionV1, NativeRecoveredRunnerV1, PathBuf), String> {
    recover_native_with_publication(repository, transport, plan, false, &mut |_| {})
}

fn recover_native_with_publication(
    repository: &Path,
    mut transport: LeasedJavaLockstepTransportV1,
    plan: VerifiedNativeRecoveryV1,
    publish_active: bool,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<(ManualSessionV1, NativeRecoveredRunnerV1, PathBuf), String> {
    let _timing = sts_parity_lockstep::timing_span_v1("native.recover");
    require_selected_session(repository, &plan.selected_session_id)?;
    let replacement = ManualSessionV1::prepare_replacement(repository)?;
    let result = progress_phase(observer, "Java native reload", |_| {
        recover_native_inner(&mut transport, &plan, &replacement)
    });
    let (storage, execution) = match result {
        Ok(result) => result,
        Err(error) => {
            write_new(
                &replacement.directory().join("native-recovery-failure.json"),
                &serde_json::to_vec(&serde_json::json!({"status":"stopped", "reason":error}))
                    .map_err(|e| e.to_string())?,
            )?;
            return Err(error);
        }
    };
    let rust_root = RunControlSessionCheckpointV1::from_session(plan.rust.session());
    let store = RecordedJavaEvidenceStoreV1::create_new(
        &replacement.evidence_directory(),
        plan.bootstrap.clone(),
    )?;
    let java = JavaLockstepAdapterV1::reattach_after_native_reload_with_evidence_store(
        transport,
        plan.bootstrap,
        store,
        plan.java_resume,
        Some(execution),
    )?;
    let (epoch, directory) = replacement.next_epoch_directory()?;
    let mut runner = LockstepRunnerV1::start_new(
        java,
        plan.rust,
        LockstepModeV1::Audit,
        &directory,
        format!("{}-epoch-{epoch:08}", replacement.session_id()),
    )?;
    crate::require_exact("native restored entry", runner.current_comparison())?;
    progress_phase(observer, "Java adopted suffix", |observer| {
        for (i, action) in plan.suffix.iter().enumerate() {
            runner.step(action.clone())?;
            if runner.is_stopped() {
                return Err(format!(
                    "native suffix action {} stopped: {}",
                    i + 1,
                    runner.stop_reason().unwrap_or("unknown")
                ));
            }
            observer(LockstepProgressV1::NativeSuffix {
                completed: i + 1,
                total: plan.suffix.len(),
            });
        }
        Ok(())
    })?;
    crate::require_exact(
        "native recovered terminal",
        &compare_snapshots_v1(&plan.expected_terminal, runner.current_snapshots().0)?,
    )?;
    replacement.publish_rust_root(
        &plan.generation.manifest.source_session_id,
        plan.generation.manifest.source_committed_actions,
        rust_root,
    )?;
    // Re-read the new raw journal before publishing. Initial preparation is
    // replayed by the same adapter, with source-proven fresh UUID bindings.
    progress_phase(observer, "new recovery journal verification", |observer| {
        crate::replay_manual_session_with_progress_v1(repository, &replacement, observer)
    })?;
    write_new(&replacement.directory().join("native-recovery-result.json"), &serde_json::to_vec(&serde_json::json!({
        "schema":"sts-native-recovery-result-v1", "status":"exact", "generation":plan.generation.manifest,
        "committed_suffix_actions":plan.suffix.len(), "continuous_act_certification":false
    })).map_err(|e| e.to_string())?)?;
    require_selected_session(repository, &plan.selected_session_id)?;
    if publish_active {
        replacement.publish_replacing_active()?;
    }
    Ok((replacement, runner, storage))
}

fn require_selected_session(repository: &Path, expected: &str) -> Result<(), String> {
    if ManualSessionV1::load_active(repository)?.session_id() != expected {
        return Err("selected trajectory changed during native recovery".to_owned());
    }
    Ok(())
}

fn recover_native_inner(
    transport: &mut LeasedJavaLockstepTransportV1,
    plan: &VerifiedNativeRecoveryV1,
    replacement: &ManualSessionV1,
) -> Result<
    (
        PathBuf,
        sts_parity_lockstep_java::JavaNativeReloadExecutionV1,
    ),
    String,
> {
    let storage = transport
        .isolated_storage_directory()
        .map_err(|e| e.message)?;
    let current = transport.status().map_err(|e| e.message)?;
    let boundary = current.frame["boundary"]["kind"].as_str();
    let natural = matches!(boundary, Some("run_decision" | "combat_player_decision"))
        && current.frame["public_base"]["run"].is_object()
        && current.frame["scenario"].is_null();
    let terminal = boundary == Some("terminal")
        && current.frame["public_base"]["decision"]["kind"].as_str() == Some("heart_victory")
        && current.frame["public_base"]["run"].is_object()
        && current.frame["scenario"].is_null();
    let menu = boundary == Some("main_menu_ready") && current.frame["public_base"]["run"].is_null();
    if !natural && !terminal && !menu {
        return Err(
            "native recovery requires a stable natural decision, Heart victory or main menu"
                .to_owned(),
        );
    }
    write_new(&replacement.directory().join("native-recovery-input.json"), &serde_json::to_vec(&serde_json::json!({
        "generation":plan.generation.manifest, "suffix":plan.suffix, "target":plan.expected_terminal
    })).map_err(|e| e.to_string())?)?;
    if natural {
        transport
            .reset_natural_run_to_main_menu_preserving_save(
                &replacement.directory().join("native-reset.json"),
                Duration::from_secs(10),
            )
            .map_err(|e| e.message)?;
    } else if terminal {
        // Original victory deletes the live save. The transport revalidates
        // the terminal room and managed discard receipt before returning.
        transport
            .discard_run_for_next_case()
            .map_err(|e| e.message)?;
    }
    plan.generation.restore(&storage)?;
    let execution = transport
        .continue_run_with_content_profile("all_unlocked", Duration::from_secs(10))
        .map_err(|e| e.message)?;
    Ok((storage, execution))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_save_publication_preserves_open_readers_and_replaces_the_complete_file() {
        use std::io::Read;
        let directory =
            std::env::temp_dir().join(format!("native-publication-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("IRONCLAD.autosave");
        let old_bytes = vec![b'a'; 128 * 1024];
        let new_bytes = vec![b'b'; 256 * 1024];
        publish_native_save(&target, &old_bytes).unwrap();
        let mut reader = fs::File::open(&target).unwrap();
        publish_native_save(&target, &new_bytes).unwrap();
        let mut observed = Vec::new();
        reader.read_to_end(&mut observed).unwrap();
        assert_eq!(observed, old_bytes);
        assert_eq!(fs::read(&target).unwrap(), new_bytes);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        drop(reader);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn native_save_sharing_violation_preserves_original_bytes_and_cleans_staging() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory =
            std::env::temp_dir().join(format!("native-sharing-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("IRONCLAD.autosave");
        fs::write(&target, b"original complete save").unwrap();
        // A reader that does not share deletion prevents atomic replacement.
        let reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ
            .open(&target)
            .unwrap();
        assert!(publish_native_save(&target, b"replacement").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original complete save");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        drop(reader);
        publish_native_save(&target, b"replacement").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"replacement");
        fs::remove_dir_all(directory).unwrap();
    }

    /// Explicit, read-only measurement against an analyst-selected local
    /// session. This never discovers, launches, or sends actions to Java.
    #[test]
    #[ignore = "requires STS_NATIVE_PROFILE_REPOSITORY with recorded manual evidence"]
    fn profile_selected_native_preparation() {
        let repository = PathBuf::from(
            std::env::var_os("STS_NATIVE_PROFILE_REPOSITORY").expect("explicit repository"),
        );
        let source = ManualSessionV1::load_active(&repository).unwrap();
        let started = std::time::Instant::now();
        let generation = load_latest_native_generation_v1(&repository, &source).unwrap();
        eprintln!("select generation {:.3}s", started.elapsed().as_secs_f64());
        let started = std::time::Instant::now();
        let mut replayed_actions = 0;
        let plan = prepare_native_recovery_with_progress_v1(
            &repository,
            &source,
            generation,
            &mut |event| match event {
                LockstepProgressV1::Finished {
                    phase,
                    elapsed,
                    succeeded,
                } => eprintln!(
                    "{phase} {:.3}s | success {succeeded}",
                    elapsed.as_secs_f64()
                ),
                LockstepProgressV1::Replay { completed, .. } if completed > 0 => {
                    replayed_actions += 1
                }
                _ => (),
            },
        )
        .unwrap();
        eprintln!("prepare total {:.3}s", started.elapsed().as_secs_f64());
        eprintln!("replayed actions {replayed_actions}");
        eprintln!(
            "workload {} | anchor {}:{} | suffix {} | terminal_sha256 {:x} | suffix_sha256 {:x}",
            source.session_id(),
            plan.generation.manifest.source_session_id,
            plan.generation.manifest.source_committed_actions,
            plan.suffix.len(),
            Sha256::digest(serde_json::to_vec(&plan.expected_terminal).unwrap()),
            Sha256::digest(serde_json::to_vec(&plan.suffix).unwrap())
        );
        assert_eq!(ManualSessionV1::load_active(&repository).unwrap(), source);
        // Outside the timed preparation: compare the retained entry and final
        // state with independent full-prefix replay, including opaque Java
        // birth/reload bindings rather than only the displayed snapshot.
        let reference_started = std::time::Instant::now();
        let (_, reference_rust, reference_java) =
            verify_native_anchor_v1(&repository, &plan.generation).unwrap();
        assert_eq!(plan.java_resume, reference_java);
        assert_eq!(
            serde_json::to_value(RunControlSessionCheckpointV1::from_session(
                plan.rust.session()
            ))
            .unwrap(),
            serde_json::to_value(RunControlSessionCheckpointV1::from_session(
                reference_rust.session()
            ))
            .unwrap()
        );
        let (_, mut reference_terminal, _) =
            crate::replay_manual_session_v1(&repository, &source).unwrap();
        assert_eq!(
            plan.expected_terminal,
            reference_terminal.observe().unwrap()
        );
        let reference_suffix = adopted_suffix(
            &repository,
            &source,
            None,
            &plan.generation.manifest.source_session_id,
            plan.generation.manifest.source_committed_actions,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(plan.suffix, reference_suffix);
        eprintln!(
            "independent-prefix reference {:.3}s",
            reference_started.elapsed().as_secs_f64()
        );
        eprintln!("independent entry, terminal and Java bindings agree");
    }
    /// Reconstruct one recorded save scope in temporary storage, then compare
    /// full evidence discovery with the live writer on identical native bytes.
    /// No Java process or selected source artifact is mutated.
    #[test]
    #[ignore = "requires explicit native capture profile repository, session and output"]
    fn profile_recorded_native_capture() {
        let repository = PathBuf::from(std::env::var_os("STS_NATIVE_PROFILE_REPOSITORY").unwrap());
        let source_id = std::env::var("STS_NATIVE_CAPTURE_PROFILE_SESSION").unwrap();
        let output = PathBuf::from(std::env::var_os("STS_NATIVE_CAPTURE_PROFILE_OUTPUT").unwrap());
        assert!(output.is_absolute() && !output.exists());
        let source = ManualSessionV1::load_existing(&repository, &source_id).unwrap();
        let generation = fs::read_dir(source.directory().join("native-generations"))
            .unwrap()
            .map(|entry| NativeGenerationV1::load(&entry.unwrap().path()).unwrap())
            .filter(|generation| generation.manifest.source_session_id == source_id)
            .max_by_key(|generation| generation.manifest.source_committed_actions)
            .unwrap();
        let evidence = generation.source_prefix(&repository).unwrap();
        let repo =
            std::env::temp_dir().join(format!("native-capture-profile-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let session = ManualSessionV1::prepare(&repo).unwrap();
        let mut store = RecordedJavaEvidenceStoreV1::create_new(
            &session.evidence_directory(),
            evidence.manifest.bootstrap.clone(),
        )
        .unwrap();
        store
            .append_boundary(&evidence.initial.observation)
            .unwrap();
        let last = evidence.steps.last().unwrap();
        let tail_start = evidence.transitions.len() - last.java_actions.len();
        for transition in &evidence.transitions[..tail_start] {
            store
                .append_action(
                    transition.action.phase,
                    &transition.action.action,
                    &transition.action.receipt,
                )
                .unwrap();
            store
                .append_boundary(&transition.after.observation)
                .unwrap();
        }
        drop(store);
        let (_, rust, resume) =
            crate::replay_manual_session_prefix_v1(&repository, &source, evidence.steps.len() - 1)
                .unwrap();
        let transport = crate::RecordedJavaReplayTransportV1 {
            current: &last.before.observation,
            transitions: &evidence.transitions[tail_start..],
        };
        let java = JavaLockstepAdapterV1::resume_with_evidence_store(
            transport,
            evidence.manifest.bootstrap.clone(),
            RecordedJavaEvidenceStoreV1::open_append(&session.evidence_directory()).unwrap(),
            resume,
        )
        .unwrap();
        let mut runner = LockstepRunnerV1::start_new(
            java,
            rust,
            LockstepModeV1::Certification,
            &repo.join("journal"),
            "capture-probe",
        )
        .unwrap();
        assert!(
            capture_manual_checkpoints_after_step_v1(&session, &repo.join("absent"), &runner)
                .unwrap_err()
                .contains("journal-committed Exact")
        );
        runner.step(last.action.clone()).unwrap();
        assert!(!runner.is_stopped(), "{:?}", runner.stop_reason());
        let step = runner.java_adapter().last_recorded_step_v1().unwrap();
        assert_eq!(step.committed_actions(), evidence.steps.len());
        assert_eq!(
            step.after(),
            &evidence.steps.last().unwrap().after.observation
        );
        let storage = repo.join("isolated");
        fs::create_dir_all(storage.join("saves")).unwrap();
        let bytes = fs::read(generation.directory.join("autosave")).unwrap();
        fs::write(
            storage
                .join("saves")
                .join(&generation.manifest.autosave_name),
            &bytes,
        )
        .unwrap();
        if generation.manifest.backup_sha256.is_some() {
            fs::copy(
                generation.directory.join("autosave.backUp"),
                storage
                    .join("saves")
                    .join(format!("{}.backUp", generation.manifest.autosave_name)),
            )
            .unwrap();
        }
        capture_manual_checkpoints_after_step_v1(&session, &storage, &runner).unwrap();
        let expected = capture_native_generation_v1(&repo, &session, &storage).unwrap();
        capture_native_generation_from_step(&session, &storage, &step).unwrap();
        let mut cold_ms = Vec::new();
        let mut writer_ms = Vec::new();
        for iteration in 0..6 {
            for cold in if iteration % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                let started = std::time::Instant::now();
                if cold {
                    let actual = capture_native_generation_v1(&repo, &session, &storage).unwrap();
                    cold_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                    assert_eq!(
                        serde_json::to_value(actual.manifest).unwrap(),
                        serde_json::to_value(&expected.manifest).unwrap()
                    );
                } else {
                    capture_native_generation_from_step(&session, &storage, &step).unwrap();
                    writer_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                    let actual = NativeGenerationV1::load(&expected.directory).unwrap();
                    assert_eq!(
                        serde_json::to_value(actual.manifest).unwrap(),
                        serde_json::to_value(&expected.manifest).unwrap()
                    );
                }
            }
        }
        let executable = std::env::current_exe().unwrap();
        let report = serde_json::json!({
            "schema": "sts-native-capture-profile-v1",
            "executable_sha256": hash(&fs::read(&executable).unwrap()),
            "source_session": source_id,
            "source_committed_actions": evidence.steps.len(),
            "source_java_actions": evidence.transitions.len(),
            "source_observation_sha256": generation.manifest.source_observation_sha256,
            "autosave_sha256": hash(&bytes),
            "full_discovery_ms": cold_ms, "writer_anchor_ms": writer_ms,
            "all_captures_equal": true,
            "runner_commit_verified": true,
            "uncommitted_capture_rejected": true,
            "scope": "repeated capture of identical recorded scope after warmup; local evidence only, excludes ancestor rereads, source reconstruction, Java, and build time"
        });
        write_new(&output, &serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        eprintln!("capture comparison | {} committed actions | all native bytes and source observations agree", evidence.steps.len());
        assert!(repo
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(repo).unwrap();
    }

    use serde_json::json;
    use sts_parity_lockstep::DecisionSurfaceV1;
    use sts_parity_lockstep_java::{JavaLockstepReceiptV1, RecordedJavaActionPhaseV1};

    fn metadata() -> NativeSaveMetadataV1 {
        NativeSaveMetadataV1 {
            seed: 41,
            act_num: 1,
            floor_num: 17,
            room_x: -1,
            room_y: 15,
            current_room: "com.megacrit.cardcrawl.rooms.TreasureRoomBoss".to_owned(),
            post_combat: false,
            chose_neow_reward: false,
        }
    }
    fn save_bytes(meta: &NativeSaveMetadataV1) -> Vec<u8> {
        encoded_save_value(&serde_json::to_value(meta).unwrap())
    }

    #[test]
    fn native_shrine_filter_keeps_order_and_retires_only_finished_act_pools() {
        let mut meta = metadata();
        meta.act_num = 3;
        meta.current_room = "com.megacrit.cardcrawl.rooms.RestRoom".to_owned();
        let mut source = observation(1, 3, "campfire");
        let complete = json!([
            "MatchAndKeep",
            "GremlinWheelGame",
            "GoldenShrine",
            "Transmorgrifier",
            "Purifier",
            "UpgradeShrine"
        ]);
        source.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] =
            complete.clone();
        assert!(native_shrine_pool_preserved(&meta, &source).unwrap());
        for incomplete in [
            json!([
                "MatchAndKeep",
                "GoldenShrine",
                "Transmorgrifier",
                "Purifier",
                "UpgradeShrine"
            ]),
            json!([
                "GremlinWheelGame",
                "MatchAndKeep",
                "GoldenShrine",
                "Transmorgrifier",
                "Purifier",
                "UpgradeShrine"
            ]),
            json!([]),
        ] {
            source.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] = incomplete;
            assert!(!native_shrine_pool_preserved(&meta, &source).unwrap());
        }
        source.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] =
            serde_json::Value::Null;
        assert!(native_shrine_pool_preserved(&meta, &source).is_err());
        for act in [1, 2, 3] {
            meta.act_num = act;
            meta.current_room = "com.megacrit.cardcrawl.rooms.TreasureRoomBoss".to_owned();
            let mut source = observation(1, act, "boss_relic");
            source.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] = json!([]);
            assert_eq!(
                native_shrine_pool_preserved(&meta, &source).unwrap(),
                act <= 2
            );
        }
        meta.act_num = 4;
        meta.current_room = "com.megacrit.cardcrawl.rooms.RestRoom".to_owned();
        assert!(native_shrine_pool_preserved(&meta, &observation(1, 4, "campfire")).unwrap());
        for act in 1..=4 {
            meta.act_num = act;
            for room in ["VictoryRoom", "EventRoom"] {
                meta.current_room = format!("com.megacrit.cardcrawl.rooms.{room}");
                for daily in [false, true] {
                    let mut source = observation(1, act, "event");
                    source.frame["public_base"]["decision"]["event_id"] = json!("SpireHeart");
                    source.frame["parity_internal"]["run_state"]["daily"] = json!(daily);
                    source.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] =
                        json!(["UpgradeShrine"]);
                    assert_eq!(
                        native_shrine_pool_preserved(&meta, &source).unwrap(),
                        act == 3 && room == "VictoryRoom" && !daily
                    );
                    for malformed in [
                        json!(["Upgrade Shrine", "UpgradeShrine"]),
                        json!([""]),
                        json!(null),
                    ] {
                        source.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] =
                            malformed;
                        assert!(native_shrine_pool_preserved(&meta, &source).is_err());
                    }
                }
            }
        }
    }

    #[test]
    fn native_selection_skips_a_later_save_that_would_restore_a_consumed_shrine() {
        let repo =
            std::env::temp_dir().join(format!("native-shrine-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let source = ManualSessionV1::prepare(&repo).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut store =
            RecordedJavaEvidenceStoreV1::create_new(&source.evidence_directory(), bootstrap)
                .unwrap();
        let mut initial = observation(1, 3, "map");
        initial.frame["public_base"]["run"]["floor"] = json!(16);
        store.append_boundary(&initial).unwrap();
        append(&mut store, 1, observation(2, 3, "campfire"));
        let storage = repo.join("isolated");
        fs::create_dir_all(storage.join("saves")).unwrap();
        let mut meta = metadata();
        meta.act_num = 3;
        meta.current_room = "com.megacrit.cardcrawl.rooms.RestRoom".to_owned();
        let save = storage.join("saves/IRONCLAD.autosave");
        fs::write(&save, save_bytes(&meta)).unwrap();
        let faithful = capture_native_generation_v1(&repo, &source, &storage).unwrap();
        let mut later = observation(3, 3, "campfire");
        later.frame["public_base"]["run"]["floor"] = json!(18);
        later.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        append(&mut store, 2, later);
        meta.floor_num = 18;
        fs::write(&save, save_bytes(&meta)).unwrap();
        let lossy = capture_native_generation_v1(&repo, &source, &storage).unwrap();
        assert_eq!(
            load_latest_native_generation_v1(&repo, &source)
                .unwrap()
                .directory,
            faithful.directory
        );
        assert!(validate_native_anchor_evidence(&repo, &lossy)
            .unwrap_err()
            .contains("consumed shrine pool"));
        // A real later VictoryRoom save is faithful: SpireHeart cannot draw
        // from the consumed pool, and the next Act will initialize its own.
        let mut exit = observation(4, 3, "event");
        exit.frame["public_base"]["run"]["floor"] = json!(52);
        exit.frame["public_base"]["decision"]["event_id"] = json!("SpireHeart");
        exit.frame["public_base"]["decision"]["event_screen_index"] = json!(0);
        exit.frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] =
            json!(["UpgradeShrine"]);
        append(&mut store, 3, exit);
        meta.floor_num = 52;
        meta.current_room = "com.megacrit.cardcrawl.rooms.VictoryRoom".to_owned();
        fs::write(&save, save_bytes(&meta)).unwrap();
        let finished = capture_native_generation_v1(&repo, &source, &storage).unwrap();
        assert_eq!(
            load_latest_native_generation_v1(&repo, &source)
                .unwrap()
                .directory,
            finished.directory
        );
        validate_native_anchor_evidence(&repo, &finished).unwrap();
        assert_eq!(fs::read(&save).unwrap(), save_bytes(&meta));
        assert!(lossy.directory.join("generation.json").is_file());
        drop(store);
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn question_mark_save_matches_resolved_room_entrances_only() {
        let mut meta = metadata();
        meta.current_room = "com.megacrit.cardcrawl.rooms.EventRoom".to_owned();
        for decision in ["event", "shop", "treasure", "combat"] {
            let mut entrance = observation(1, 1, decision);
            if decision == "combat" {
                entrance.frame["boundary"]["kind"] = json!("combat_player_decision");
            }
            assert!(NativeSaveMetadataV1::decode(&save_bytes(&meta))
                .unwrap()
                .matches_candidate(&entrance));
            for field in ["seed", "act_num", "floor_num", "room_x", "room_y"] {
                let mut wrong = serde_json::to_value(&meta).unwrap();
                wrong[field] = json!(wrong[field].as_i64().unwrap() + 1);
                let wrong: NativeSaveMetadataV1 = serde_json::from_value(wrong).unwrap();
                assert!(!wrong.matches_candidate(&entrance), "{decision}: {field}");
            }
        }
        for decision in [
            "map",
            "campfire",
            "boss_relic",
            "combat_reward",
            "card_reward",
        ] {
            assert!(!meta.matches_candidate(&observation(1, 1, decision)));
        }
        meta.post_combat = true;
        for decision in ["event", "shop", "treasure"] {
            assert!(!meta.matches_candidate(&observation(1, 1, decision)));
        }
    }

    #[test]
    fn post_neow_save_matches_only_the_original_room_and_completed_entrance() {
        let mut meta = metadata();
        meta.floor_num = 0;
        meta.room_x = 0;
        meta.room_y = -1;
        meta.current_room = "com.megacrit.cardcrawl.neow.NeowRoom".to_owned();
        meta.chose_neow_reward = true;
        let mut entrance = observation(1, 1, "event");
        entrance.frame["public_base"]["run"]["floor"] = json!(0);
        entrance.frame["public_base"]["map"] = json!({"current_x":-1,"current_y":-1});
        entrance.frame["public_base"]["decision"]["event_id"] = json!("Neow");
        entrance.frame["public_base"]["decision"]["event_screen_index"] = json!(99);
        assert!(NativeSaveMetadataV1::decode(&save_bytes(&meta))
            .unwrap()
            .matches_candidate(&entrance));
        for screen in [0, 1, 3] {
            let mut before_reward = entrance.clone();
            before_reward.frame["public_base"]["decision"]["event_screen_index"] = json!(screen);
            assert!(!meta.matches_candidate(&before_reward));
        }
        let mut invalid = meta.clone();
        invalid.chose_neow_reward = false;
        assert!(!invalid.matches_candidate(&entrance));
        invalid = meta.clone();
        invalid.room_x = -1;
        assert!(!invalid.matches_candidate(&entrance));
        invalid = meta;
        invalid.current_room = "com.megacrit.cardcrawl.rooms.NeowRoom".to_owned();
        assert!(!invalid.matches_candidate(&entrance));
    }

    #[test]
    fn victory_room_save_anchors_only_the_original_spire_heart_entrance() {
        let mut meta = metadata();
        meta.act_num = 3;
        meta.current_room = "com.megacrit.cardcrawl.rooms.VictoryRoom".to_owned();
        let mut entrance = observation(1, 3, "event");
        entrance.frame["public_base"]["decision"]["event_id"] = json!("SpireHeart");
        entrance.frame["public_base"]["decision"]["event_screen_index"] = json!(0);
        assert!(meta.matches_candidate(&entrance));
        for screen in [1, 2, 3, 4] {
            let mut later = entrance.clone();
            later.frame["public_base"]["decision"]["event_screen_index"] = json!(screen);
            assert!(!meta.matches_candidate(&later));
        }
        entrance.frame["public_base"]["decision"]["event_id"] = json!("MindBloom");
        assert!(!meta.matches_candidate(&entrance));
    }
    fn encoded_save_value(value: &serde_json::Value) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(value).unwrap();
        for (i, b) in bytes.iter_mut().enumerate() {
            *b ^= b"key"[i % 3];
        }
        STANDARD.encode(bytes).into_bytes()
    }

    #[test]
    fn recorded_room_transition_skips_save_reads_until_a_new_scope() {
        let repo = std::env::temp_dir().join(format!("native-hot-path-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let session = ManualSessionV1::prepare(&repo).unwrap();
        let storage = repo.join("absent-storage");
        let before = observation(1, 3, "combat");
        let mut after = observation(2, 3, "combat");
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut store =
            RecordedJavaEvidenceStoreV1::create_new(&session.evidence_directory(), bootstrap)
                .unwrap();
        store.append_boundary(&before).unwrap();
        append(&mut store, 1, after.clone());
        capture_native_generation_from_step(
            &session,
            &storage,
            &store.last_complete_step_v1().unwrap(),
        )
        .unwrap();
        after.frame_id = 3;
        after.frame["public_base"]["run"]["floor"] = json!(18);
        append(&mut store, 2, after.clone());
        assert!(capture_native_generation_from_step(
            &session,
            &storage,
            &store.last_complete_step_v1().unwrap()
        )
        .is_err());
        after.frame_id = 4;
        after.frame["boundary"]["kind"] = json!("terminal");
        after.frame["public_base"]["decision"]["kind"] = json!("heart_victory");
        // Victory also changes floors, but deliberately has no new autosave.
        append(&mut store, 3, after);
        capture_native_generation_from_step(
            &session,
            &storage,
            &store.last_complete_step_v1().unwrap(),
        )
        .unwrap();
        assert!(repo.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn writer_anchor_matches_cold_source_and_rejects_another_session() {
        let repo =
            std::env::temp_dir().join(format!("native-writer-anchor-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let session = ManualSessionV1::prepare(&repo).unwrap();
        let other = ManualSessionV1::prepare(&repo).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut store =
            RecordedJavaEvidenceStoreV1::create_new(&session.evidence_directory(), bootstrap)
                .unwrap();
        let mut before = observation(1, 1, "map");
        before.frame["public_base"]["run"]["floor"] = json!(16);
        store.append_boundary(&before).unwrap();
        append(&mut store, 1, observation(2, 1, "boss_relic"));
        let step = store.last_complete_step_v1().unwrap();
        let (source, count, raw, post_combat) =
            latest_scope_anchor(&repo, &session, None, &mut vec![])
                .unwrap()
                .unwrap();
        assert_eq!(source, session);
        assert_eq!(count, step.committed_actions());
        assert_eq!(&raw, step.after());
        assert_eq!(
            Some(post_combat),
            save_scope_transition(step.before(), step.after())
        );
        let storage = repo.join("isolated");
        fs::create_dir_all(storage.join("saves")).unwrap();
        let bytes = save_bytes(&metadata());
        fs::write(storage.join("saves/IRONCLAD.autosave"), &bytes).unwrap();
        assert!(capture_native_generation_from_step(&other, &storage, &step).is_err());
        let timings = sts_parity_lockstep::TimingCaptureV1::start().unwrap();
        capture_native_generation_from_step(&session, &storage, &step).unwrap();
        let timings = timings.finish();
        assert!(timings
            .spans
            .keys()
            .all(|path| !path.contains("evidence.load")));
        let captured = NativeGenerationV1::load(
            &session
                .directory()
                .join("native-generations")
                .join(hash(&bytes)),
        )
        .unwrap();
        assert_eq!(captured.manifest.source_session_id, source.session_id());
        assert_eq!(captured.manifest.source_committed_actions, count);
        captured.validate_source_observation(&raw).unwrap();
        assert_eq!(
            serde_json::to_value(
                capture_native_generation_v1(&repo, &session, &storage)
                    .unwrap()
                    .manifest
            )
            .unwrap(),
            serde_json::to_value(captured.manifest).unwrap()
        );
        assert!(repo.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn cold_terminal_capture_keeps_the_durable_generation_without_a_live_save() {
        let repo =
            std::env::temp_dir().join(format!("native-terminal-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let session = ManualSessionV1::prepare(&repo).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut store =
            RecordedJavaEvidenceStoreV1::create_new(&session.evidence_directory(), bootstrap)
                .unwrap();
        let mut initial = observation(1, 4, "map");
        initial.frame["public_base"]["run"]["floor"] = json!(55);
        store.append_boundary(&initial).unwrap();
        let mut entry = observation(2, 4, "combat");
        entry.frame["boundary"]["kind"] = json!("combat_player_decision");
        entry.frame["public_base"]["run"]["floor"] = json!(56);
        append(&mut store, 1, entry);
        let storage = repo.join("isolated");
        fs::create_dir_all(storage.join("saves")).unwrap();
        // A nonterminal boundary must still fail when its live save is absent.
        assert!(capture_native_generation_if_ready_v1(&repo, &session, &storage).is_err());
        let mut meta = metadata();
        meta.act_num = 4;
        meta.floor_num = 56;
        meta.current_room = "com.megacrit.cardcrawl.rooms.MonsterRoomBoss".to_owned();
        let save = storage.join("saves/IRONCLAD.autosave");
        fs::write(&save, save_bytes(&meta)).unwrap();
        let generation = capture_native_generation_v1(&repo, &session, &storage).unwrap();
        let mut terminal = observation(3, 4, "heart_victory");
        terminal.frame["boundary"]["kind"] = json!("terminal");
        terminal.frame["public_base"]["run"]["floor"] = json!(57);
        append(&mut store, 2, terminal);
        // Reproduce the original VictoryScreen's save deletion.
        fs::remove_file(&save).unwrap();
        capture_native_generation_if_ready_v1(&repo, &session, &storage).unwrap();
        assert!(!save.exists());
        assert_eq!(
            load_latest_native_generation_v1(&repo, &session)
                .unwrap()
                .directory,
            generation.directory
        );
        assert_eq!(
            fs::read(generation.directory.join("autosave")).unwrap(),
            save_bytes(&meta)
        );
        assert!(repo
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn lossy_post_combat_save_keeps_its_evidence_and_selects_the_previous_entry() {
        let repo =
            std::env::temp_dir().join(format!("native-chances-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let session = ManualSessionV1::prepare(&repo).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut store =
            RecordedJavaEvidenceStoreV1::create_new(&session.evidence_directory(), bootstrap)
                .unwrap();
        let initial = observation(1, 1, "map");
        store.append_boundary(&initial).unwrap();
        let mut entry = observation(2, 1, "combat");
        entry.frame["public_base"]["run"]["floor"] = json!(18);
        entry.frame["boundary"]["kind"] = json!("combat_player_decision");
        entry.frame["parity_internal"]["run_state"]["event_schedule"]["chance_bits"] = json!({
            "elite":0.1_f32.to_bits(), "monster":0.2_f32.to_bits(),
            "shop":0.06_f32.to_bits(), "treasure":0.04_f32.to_bits()
        });
        append(&mut store, 1, entry.clone());
        let storage = repo.join("isolated");
        fs::create_dir_all(storage.join("saves")).unwrap();
        let save_path = storage.join("saves/IRONCLAD.autosave");
        let mut meta = metadata();
        meta.floor_num = 18;
        meta.current_room = "com.megacrit.cardcrawl.rooms.MonsterRoom".to_owned();
        fs::write(&save_path, save_bytes(&meta)).unwrap();
        let earlier = capture_native_generation_v1(&repo, &session, &storage).unwrap();
        let mut reward = entry;
        reward.frame_id = 3;
        reward.frame["public_base"]["decision"]["kind"] = json!("combat_reward");
        reward.frame["boundary"]["kind"] = json!("run_decision");
        append(&mut store, 2, reward.clone());
        meta.post_combat = true;
        let mut save = serde_json::to_value(&meta).unwrap();
        save["event_chances"] = json!([0.0, 0.1, 0.03, 0.02]);
        let original_bytes = encoded_save_value(&save);
        fs::write(&save_path, &original_bytes).unwrap();
        let lossy = capture_native_generation_v1(&repo, &session, &storage).unwrap();
        assert_eq!(lossy.manifest.source_committed_actions, 2);
        assert_eq!(
            load_latest_native_generation_v1(&repo, &session)
                .unwrap()
                .directory,
            earlier.directory
        );
        assert_eq!(
            fs::read(lossy.directory.join("autosave")).unwrap(),
            original_bytes
        );
        assert!(prepare_native_recovery_v1(&repo, &session, lossy)
            .err()
            .unwrap()
            .contains("rewind event probabilities"));

        let correct = [0.1_f32, 0.2, 0.06, 0.04];
        save["event_chances"] = json!(correct);
        assert!(
            native_event_chances_preserved(&encoded_save_value(&save), &meta, &reward).unwrap()
        );
        for index in 0..4 {
            let mut different = save.clone();
            different["event_chances"][index] = json!(0.0);
            assert!(!native_event_chances_preserved(
                &encoded_save_value(&different),
                &meta,
                &reward
            )
            .unwrap());
        }
        save.as_object_mut().unwrap().remove("event_chances");
        assert!(
            native_event_chances_preserved(&encoded_save_value(&save), &meta, &reward).is_err()
        );
        fs::remove_dir_all(repo).unwrap();
    }
    fn observation(id: u64, act: u64, decision: &str) -> JavaRuntimeObservationV1 {
        JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".to_owned(),
                version: "test".to_owned(),
                session_epoch: "test".to_owned(),
            },
            frame_id: id,
            actions: Vec::new(),
            frame: json!({"boundary":{"kind":"run_decision"},
            "public_base":{"run":{"character":"IRONCLAD","act":act,"floor":17},"map":{"current_x":-1,"current_y":15},
            "decision":{"kind":decision}},"parity_internal":{"run_seed":"41",
                "run_state":{"daily":false,"event_schedule":{"shrines":
                    sts_oracle_run_control::state::events::generator::EventGenerator::new(act as u8)
                        .shrine_pool.iter().map(|event| event.stable_id()).collect::<Vec<_>>()
                }}}}),
        }
    }
    fn append(
        store: &mut RecordedJavaEvidenceStoreV1,
        before: u64,
        after: JavaRuntimeObservationV1,
    ) {
        store
            .append_action(
                RecordedJavaActionPhaseV1::Committed,
                &SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::BossRelic,
                },
                &JavaLockstepReceiptV1 {
                    status: "completed".to_owned(),
                    request_id: format!("r{before}"),
                    based_on_frame_id: before,
                    action_id: "proceed".to_owned(),
                    successor_frame_id: Some(after.frame_id),
                },
            )
            .unwrap();
        store.append_boundary(&after).unwrap();
    }

    fn publish_test_parent(child: &ManualSessionV1, parent: &ManualSessionV1, count: usize) {
        use sts_oracle_run_control::eval::run_control::RunControlSession;
        child
            .publish_rust_root(
                parent.session_id(),
                count,
                RunControlSessionCheckpointV1::from_session(&RunControlSession::new(
                    Default::default(),
                )),
            )
            .unwrap();
    }

    #[test]
    fn event_combat_retry_keeps_native_event_anchor_and_closed_suffix_before_failed_tail() {
        let repo =
            std::env::temp_dir().join(format!("native-event-combat-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let source = ManualSessionV1::prepare(&repo).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut store =
            RecordedJavaEvidenceStoreV1::create_new(&source.evidence_directory(), bootstrap)
                .unwrap();
        let mut before = observation(1, 1, "map");
        before.frame["public_base"]["run"]["floor"] = json!(16);
        store.append_boundary(&before).unwrap();
        append(&mut store, 1, observation(2, 1, "event"));
        let storage = repo.join("isolated");
        fs::create_dir_all(storage.join("saves")).unwrap();
        let mut meta = metadata();
        meta.current_room = "com.megacrit.cardcrawl.rooms.EventRoom".to_owned();
        fs::write(storage.join("saves/IRONCLAD.autosave"), save_bytes(&meta)).unwrap();
        let event_generation = capture_native_generation_v1(&repo, &source, &storage).unwrap();

        let mut combat = observation(3, 1, "combat");
        combat.frame["boundary"]["kind"] = json!("combat_player_decision");
        combat.frame["public_base"]["combat"] = json!({"turn": 1});
        append(&mut store, 2, combat);
        source
            .capture_seeded_combat_checkpoint(&storage, 2, 1, 17)
            .unwrap();
        // An action was sent but its successor could not be captured. It must
        // remain evidence and must not be replayed to recover this combat entry.
        store
            .append_action(
                RecordedJavaActionPhaseV1::Committed,
                &SemanticActionV1::EndTurn,
                &JavaLockstepReceiptV1 {
                    status: "completed".to_owned(),
                    request_id: "failed-tail".to_owned(),
                    based_on_frame_id: 3,
                    action_id: "end_turn".to_owned(),
                    successor_frame_id: Some(4),
                },
            )
            .unwrap();
        assert!(RecordedJavaEvidenceV1::load_complete(&source.evidence_directory()).is_err());
        let (owner, target) = combat_retry_target(&repo, &source).unwrap();
        assert_eq!(target.source_committed_actions, 2);
        let generation = load_latest_native_generation_for_scope(
            &repo,
            &owner,
            Some(target.source_committed_actions),
        )
        .unwrap();
        assert_eq!(generation.directory, event_generation.directory);
        assert_eq!(generation.manifest.source_committed_actions, 1);
        assert_eq!(generation.manifest.metadata.current_room, meta.current_room);
        let suffix = adopted_suffix(
            &repo,
            &owner,
            Some(target.source_committed_actions),
            &generation.manifest.source_session_id,
            generation.manifest.source_committed_actions,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(suffix.len(), 1);
        assert!(!suffix.contains(&SemanticActionV1::EndTurn));
        // Descendant branches resolve the same entry through their typed root.
        let child = ManualSessionV1::prepare(&repo).unwrap();
        publish_test_parent(&child, &source, 2);
        let (parent, inherited) = combat_retry_target(&repo, &child).unwrap();
        assert_eq!(parent, source);
        assert_eq!(inherited, target);
        let unrelated = ManualSessionV1::prepare(&repo).unwrap();
        child.publish_active().unwrap();
        assert!(prepare_native_recovery_prefix_with_progress_v1(
            &repo,
            child.session_id(),
            &unrelated,
            1,
            &mut |_| {}
        )
        .err()
        .unwrap()
        .contains("outside the selected trajectory"));
        assert_eq!(
            ManualSessionV1::load_active(&repo).unwrap().session_id(),
            child.session_id()
        );
        let (explicit_owner, explicit_target) =
            explicit_combat_retry_target(&repo, &child, source.session_id(), 2).unwrap();
        assert_eq!(explicit_owner, source);
        assert_eq!(explicit_target, target);
        // A later failed suffix is not needed to select this earlier entrance.
        assert!(
            explicit_combat_retry_target(&repo, &child, source.session_id(), 1)
                .unwrap_err()
                .contains("not a recorded combat entrance")
        );
        assert!(
            explicit_combat_retry_target(&repo, &child, source.session_id(), 3)
                .unwrap_err()
                .contains("exceeds its adopted prefix")
        );
        assert!(
            explicit_combat_retry_target(&repo, &child, unrelated.session_id(), 2)
                .unwrap_err()
                .contains("outside the selected trajectory")
        );
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn native_selection_respects_branch_prefix_distance_source_integrity_and_ambiguity() {
        let repo =
            std::env::temp_dir().join(format!("native-branch-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let parent = ManualSessionV1::prepare(&repo).unwrap();
        let child = ManualSessionV1::prepare(&repo).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut parent_store = RecordedJavaEvidenceStoreV1::create_new(
            &parent.evidence_directory(),
            bootstrap.clone(),
        )
        .unwrap();
        let mut initial = observation(1, 1, "combat_reward");
        initial.frame["public_base"]["run"]["floor"] = json!(16);
        parent_store.append_boundary(&initial).unwrap();
        append(&mut parent_store, 1, observation(2, 1, "boss_relic"));
        let storage = repo.join("isolated");
        fs::create_dir_all(storage.join("saves")).unwrap();
        let save_path = storage.join("saves/IRONCLAD.autosave");
        fs::write(&save_path, save_bytes(&metadata())).unwrap();
        let adopted = capture_native_generation_v1(&repo, &parent, &storage).unwrap();

        let mut later = observation(3, 1, "boss_relic");
        later.frame["public_base"]["run"]["floor"] = json!(18);
        append(&mut parent_store, 2, later.clone());
        let mut later_meta = metadata();
        later_meta.floor_num = 18;
        fs::write(&save_path, save_bytes(&later_meta)).unwrap();
        let abandoned = capture_native_generation_v1(&repo, &parent, &storage).unwrap();
        publish_test_parent(&child, &parent, 1);
        let mut child_store =
            RecordedJavaEvidenceStoreV1::create_new(&child.evidence_directory(), bootstrap)
                .unwrap();
        child_store
            .append_boundary(&observation(2, 1, "boss_relic"))
            .unwrap();
        append(&mut child_store, 2, observation(3, 1, "map"));
        let selected = load_latest_native_generation_v1(&repo, &child).unwrap();
        assert_eq!(selected.directory, adopted.directory);
        assert_ne!(selected.directory, abandoned.directory);

        later.frame_id = 4;
        append(&mut child_store, 3, later);
        let recent = capture_native_generation_v1(&repo, &child, &storage).unwrap();
        assert_eq!(
            load_latest_native_generation_v1(&repo, &child)
                .unwrap()
                .directory,
            recent.directory
        );
        // A requested earlier endpoint must not install a later save, even
        // when that generation belongs to the same adopted session.
        assert_eq!(
            load_latest_native_generation_for_scope(&repo, &child, Some(1))
                .unwrap()
                .directory,
            adopted.directory
        );
        assert_eq!(
            load_latest_native_generation_for_scope(&repo, &child, Some(2))
                .unwrap()
                .directory,
            recent.directory
        );
        assert!(load_latest_native_generation_for_scope(&repo, &child, Some(3)).is_err());
        child.publish_active().unwrap();
        assert!(
            prepare_native_recovery_prefix_v1(&repo, parent.session_id(), &parent, 1)
                .err()
                .unwrap()
                .contains("selected trajectory changed")
        );
        assert!(
            prepare_native_recovery_prefix_v1(&repo, child.session_id(), &parent, 2)
                .err()
                .unwrap()
                .contains("exceeds its adopted prefix")
        );

        let manifest_path = recent.directory.join("generation.json");
        let original_manifest = fs::read(&manifest_path).unwrap();
        let mut changed = recent.manifest.clone();
        changed.source_observation_sha256 = "changed".to_owned();
        fs::write(&manifest_path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(load_latest_native_generation_v1(&repo, &child)
            .unwrap_err()
            .contains("source evidence changed"));
        fs::write(&manifest_path, original_manifest).unwrap();

        let mut different_save = serde_json::to_value(&later_meta).unwrap();
        different_save["gold"] = json!(999);
        fs::write(&save_path, encoded_save_value(&different_save)).unwrap();
        capture_native_generation_v1(&repo, &child, &storage).unwrap();
        assert!(load_latest_native_generation_v1(&repo, &child)
            .unwrap_err()
            .contains("ambiguous save generations"));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn native_selection_rejects_cyclic_or_out_of_bounds_ancestry() {
        let repo =
            std::env::temp_dir().join(format!("native-ancestry-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let parent = ManualSessionV1::prepare(&repo).unwrap();
        let cycle = ManualSessionV1::prepare(&repo).unwrap();
        let invalid = ManualSessionV1::prepare(&repo).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        for session in [&parent, &cycle, &invalid] {
            let mut store = RecordedJavaEvidenceStoreV1::create_new(
                &session.evidence_directory(),
                bootstrap.clone(),
            )
            .unwrap();
            store
                .append_boundary(&observation(1, 1, "boss_relic"))
                .unwrap();
            append(&mut store, 1, observation(2, 1, "map"));
        }
        publish_test_parent(&invalid, &parent, 2);
        assert!(load_latest_native_generation_v1(&repo, &invalid)
            .unwrap_err()
            .contains("requested 2 committed actions"));
        publish_test_parent(&cycle, &cycle, 1);
        assert!(load_latest_native_generation_v1(&repo, &cycle)
            .unwrap_err()
            .contains("cyclic or too deep"));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn unchanged_native_generation_keeps_boss_anchor_across_act_transition_and_detects_tampering() {
        let repo =
            std::env::temp_dir().join(format!("native-generation-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&repo).unwrap();
        let session = ManualSessionV1::prepare(&repo).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut store =
            RecordedJavaEvidenceStoreV1::create_new(&session.evidence_directory(), bootstrap)
                .unwrap();
        let mut initial = observation(1, 1, "combat_reward");
        initial.frame["public_base"]["run"]["floor"] = json!(16);
        store.append_boundary(&initial).unwrap();
        append(&mut store, 1, observation(2, 1, "boss_relic"));
        let storage = repo.join("isolated");
        fs::create_dir_all(storage.join("saves")).unwrap();
        let bytes = save_bytes(&metadata());
        fs::write(storage.join("saves/IRONCLAD.autosave"), &bytes).unwrap();
        let first = capture_native_generation_v1(&repo, &session, &storage).unwrap();
        append(&mut store, 2, observation(3, 2, "map"));
        let second = capture_native_generation_v1(&repo, &session, &storage).unwrap();
        assert_eq!(first.directory, second.directory);
        assert_eq!(second.manifest.source_committed_actions, 1);
        session.publish_active().unwrap();
        assert_eq!(
            load_latest_native_generation_v1(&repo, &session)
                .unwrap()
                .directory,
            second.directory
        );
        assert_eq!(
            adopted_suffix(
                &repo,
                &session,
                None,
                session.session_id(),
                1,
                &mut Vec::new()
            )
            .unwrap()
            .len(),
            1
        );
        assert!(!metadata().matches_candidate(&observation(3, 2, "map")));
        assert!(adopted_suffix(
            &repo,
            &session,
            Some(1),
            session.session_id(),
            2,
            &mut Vec::new()
        )
        .is_err());
        fs::write(
            storage.join("saves/IRONCLAD.autosave"),
            b"changed live save",
        )
        .unwrap();
        fs::write(
            storage.join("saves/IRONCLAD.autosave.backUp"),
            b"stale fallback",
        )
        .unwrap();
        second.restore(&storage).unwrap();
        assert!(!storage.join("saves/IRONCLAD.autosave.backUp").exists());
        assert_eq!(
            fs::read(storage.join("saves/IRONCLAD.autosave")).unwrap(),
            bytes
        );
        fs::write(second.directory.join("autosave"), b"tampered").unwrap();
        assert!(second.restore(&storage).is_err());
        assert_eq!(
            fs::read(storage.join("saves/IRONCLAD.autosave")).unwrap(),
            bytes
        );
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn post_combat_generation_cannot_select_the_combat_entry_or_a_claimed_card_page() {
        let mut meta = metadata();
        meta.current_room = "com.megacrit.cardcrawl.rooms.MonsterRoomBoss".to_owned();
        meta.post_combat = true;
        assert!(meta.matches_candidate(&observation(2, 1, "combat_reward")));
        assert!(!meta.matches_candidate(&observation(3, 1, "card_reward")));
        assert!(!meta.matches_candidate(&observation(4, 1, "boss_relic")));
        assert!(NativeSaveMetadataV1::decode(b"corrupt autosave").is_err());
        for name in [
            "../IRONCLAD.autosave",
            "C:IRONCLAD.autosave",
            "x\\IRONCLAD.autosave",
        ] {
            assert!(!safe_autosave_name(name));
        }
    }

    #[test]
    fn save_scope_ignores_page_changes_and_act_construction_but_tracks_victory() {
        let boss = observation(1, 1, "boss_relic");
        let act2 = observation(2, 2, "map");
        assert_eq!(save_scope_transition(&boss, &act2), None);
        let mut combat = observation(3, 2, "combat");
        combat.frame["public_base"]["run"]["floor"] = json!(18);
        combat.frame["boundary"]["kind"] = json!("combat_player_decision");
        assert_eq!(save_scope_transition(&act2, &combat), Some(false));
        let mut reward = combat.clone();
        reward.frame["public_base"]["decision"]["kind"] = json!("combat_reward");
        reward.frame["boundary"]["kind"] = json!("run_decision");
        assert_eq!(save_scope_transition(&combat, &reward), Some(true));
        let mut claim = reward.clone();
        claim.frame["public_base"]["decision"]["kind"] = json!("card_reward");
        assert_eq!(save_scope_transition(&reward, &claim), None);
    }
}
