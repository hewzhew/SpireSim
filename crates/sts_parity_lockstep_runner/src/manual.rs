use crate::presentation_names::{name, power};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use sts_oracle_run_control::content::{
    cards::CardId, monsters::EnemyId, potions::PotionId, relics::RelicId,
};
use sts_oracle_run_control::eval::run_control::RunControlSessionCheckpointV1;
use sts_parity_lockstep::{
    DomainEvidenceV1, NaturalRunCombatV4, NaturalRunStateV2, ParityDomainV1, ParitySnapshotV1,
    SemanticActionHandleV1, SemanticActionSpaceV1, SemanticActionV1, SemanticChoiceV1,
    SemanticScalarV1,
};
use sts_parity_lockstep_java::RecordedJavaEvidenceStoreV1;
use uuid::Uuid;

const MANUAL_ROOT: &str = "parity-manual";
const ACTIVE_SELECTOR_SCHEMA_V1: &str = "sts-parity-manual-active-v1";
const RUST_ROOT_SCHEMA_V1: &str = "sts-parity-manual-rust-root-v1";
const SEEDED_COMBAT_CHECKPOINT_SCHEMA_V1: &str = "sts-parity-manual-seeded-combat-checkpoint-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManualCommandV1 {
    Observe,
    Map,
    Drafts,
    Step(String),
    PlanRewards(Vec<crate::RewardPlanStepV1>),
    Adopt(PathBuf),
    Park,
    Help,
}

pub fn parse_manual_command_v1(line: &str) -> Result<ManualCommandV1, String> {
    if let Some(path) = line
        .trim()
        .strip_prefix("adopt ")
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let path = path
            .strip_prefix('"')
            .and_then(|p| p.strip_suffix('"'))
            .unwrap_or(path);
        return Ok(ManualCommandV1::Adopt(PathBuf::from(path)));
    }
    let words = line.split_whitespace().collect::<Vec<_>>();
    match words.as_slice() {
        [] | ["observe"] => Ok(ManualCommandV1::Observe),
        ["map"] => Ok(ManualCommandV1::Map),
        ["drafts"] => Ok(ManualCommandV1::Drafts),
        ["park"] => Ok(ManualCommandV1::Park),
        ["help"] | ["?"] => Ok(ManualCommandV1::Help),
        ["plan", "rewards", steps @ ..] => crate::parse_reward_plan_steps_v1(steps).map(ManualCommandV1::PlanRewards),
        [handle] => normalize_action_handle(handle).map(ManualCommandV1::Step),
        ["step", handle] => normalize_action_handle(handle).map(ManualCommandV1::Step),
        _ => Err(
            "use observe, map, drafts, V…:0, plan rewards potion gold cards, adopt <handle or path>, park, or help"
                .to_owned(),
        ),
    }
}

fn normalize_action_handle(value: &str) -> Result<String, String> {
    let (view, index) = value
        .split_once(':')
        .ok_or("action reference needs its source view; copy V…:0 from observe")?;
    let token = view
        .strip_prefix('V')
        .ok_or("action reference must start with V")?;
    if token.len() != 22
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || index.is_empty()
        || !index.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("invalid action reference; copy V…:0 from observe".into());
    }
    let parsed = index
        .parse::<usize>()
        .map_err(|_| format!("action handle {value:?} is too large"))?;
    Ok(format!("V{token}:{parsed}"))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ActiveManualSessionV1 {
    schema: String,
    session_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualRustRootV1<T = RunControlSessionCheckpointV1> {
    schema: String,
    pub source_session_id: String,
    pub source_committed_actions: usize,
    pub checkpoint: T,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ManualSeededCombatCheckpointManifestV1 {
    schema: String,
    source_session_id: String,
    source_committed_actions: usize,
    act: u64,
    floor: i64,
    autosave_name: String,
    backup_present: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualSeededCombatCheckpointV1 {
    pub source_session_id: String,
    pub source_committed_actions: usize,
    pub act: u64,
    pub floor: i64,
    directory: PathBuf,
    autosave_name: String,
    backup_present: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualSessionV1 {
    repository: PathBuf,
    session_id: String,
    directory: PathBuf,
}

impl ManualSessionV1 {
    /// Reserve a private session directory without publishing it as active.
    /// The caller publishes only after Java, Rust, raw evidence, and the first
    /// epoch have all reached one comparable boundary.
    pub fn prepare(repository: &Path) -> Result<Self, String> {
        Self::prepare_inner(repository, false)
    }

    /// Prepare a new, independent world line while leaving the currently
    /// selected session and all of its evidence untouched. The caller must not
    /// replace the selector until both endpoints have reached an Exact initial
    /// boundary.
    pub fn prepare_replacement(repository: &Path) -> Result<Self, String> {
        Self::load_active(repository)?;
        Self::prepare_inner(repository, true)
    }

    fn prepare_inner(repository: &Path, allow_active: bool) -> Result<Self, String> {
        let root = manual_root(repository);
        fs::create_dir_all(root.join("sessions"))
            .map_err(|error| format!("failed to create manual session store: {error}"))?;
        let active = root.join("active.json");
        if active.exists() && !allow_active {
            return Err(
                "a manual lockstep session is already active; use manual-resume".to_owned(),
            );
        }
        let session_id = format!("manual-{}", Uuid::new_v4().simple());
        let directory = root.join("sessions").join(&session_id);
        fs::create_dir(&directory)
            .map_err(|error| format!("failed to create manual session: {error}"))?;
        fs::create_dir(directory.join("epochs"))
            .map_err(|error| format!("failed to create manual epoch store: {error}"))?;
        Ok(Self {
            repository: repository.to_path_buf(),
            session_id,
            directory,
        })
    }

    pub fn publish_active(&self) -> Result<(), String> {
        write_json_create_new(
            &manual_root(&self.repository).join("active.json"),
            &ActiveManualSessionV1 {
                schema: ACTIVE_SELECTOR_SCHEMA_V1.to_owned(),
                session_id: self.session_id.clone(),
            },
        )
    }

    pub fn publish_replacing_active(&self) -> Result<(), String> {
        // Revalidate the old selector immediately before replacement. A stale
        // or malformed selector must never be silently hidden by a new run.
        Self::load_active(&self.repository)?;
        write_json_replace(
            &manual_root(&self.repository).join("active.json"),
            &ActiveManualSessionV1 {
                schema: ACTIVE_SELECTOR_SCHEMA_V1.to_owned(),
                session_id: self.session_id.clone(),
            },
        )
    }

    pub fn load_active(repository: &Path) -> Result<Self, String> {
        let path = manual_root(repository).join("active.json");
        let bytes = fs::read(&path)
            .map_err(|error| format!("no resumable manual lockstep session: {error}"))?;
        let selector: ActiveManualSessionV1 = serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to decode active manual session: {error}"))?;
        if selector.schema != ACTIVE_SELECTOR_SCHEMA_V1 || !valid_session_id(&selector.session_id) {
            return Err("active manual session selector is invalid".to_owned());
        }
        Self::load_existing(repository, &selector.session_id)
    }

    pub fn load_existing(repository: &Path, session_id: &str) -> Result<Self, String> {
        if !valid_session_id(session_id) {
            return Err("manual session identity is invalid".to_owned());
        }
        let directory = manual_root(repository).join("sessions").join(session_id);
        if !directory.is_dir() || !directory.join("epochs").is_dir() {
            return Err("manual session directory is incomplete".to_owned());
        }
        Ok(Self {
            repository: repository.to_path_buf(),
            session_id: session_id.to_owned(),
            directory,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn evidence_directory(&self) -> PathBuf {
        self.directory.join("java-evidence")
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn publish_rust_root(
        &self,
        source_session_id: &str,
        source_committed_actions: usize,
        checkpoint: RunControlSessionCheckpointV1,
    ) -> Result<(), String> {
        // Zero names the independently observed initial boundary. A prepared
        // Act entrance may need no committed suffix after native settlement.
        if !valid_session_id(source_session_id) {
            return Err("manual Rust root has invalid source provenance".to_owned());
        }
        write_json_create_new(
            &self.directory.join("rust-root.json"),
            &ManualRustRootV1 {
                schema: RUST_ROOT_SCHEMA_V1.to_owned(),
                source_session_id: source_session_id.to_owned(),
                source_committed_actions,
                checkpoint,
            },
        )
    }

    pub fn load_rust_root(&self) -> Result<Option<ManualRustRootV1>, String> {
        self.read_rust_root()
    }

    /// Replay follows source coordinates and validates every raw Java transition.
    /// The diagnostic Rust snapshot is neither a restore input nor a schema gate.
    pub(crate) fn load_source_prefix(
        &self,
    ) -> Result<Option<ManualRustRootV1<serde::de::IgnoredAny>>, String> {
        self.read_rust_root()
    }

    fn read_rust_root<T: serde::de::DeserializeOwned>(
        &self,
    ) -> Result<Option<ManualRustRootV1<T>>, String> {
        let path = self.directory.join("rust-root.json");
        if !path.exists() {
            return Ok(None);
        }
        let bytes =
            fs::read(&path).map_err(|error| format!("failed to read manual Rust root: {error}"))?;
        let root: ManualRustRootV1<T> = serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to decode manual Rust root: {error}"))?;
        if root.schema != RUST_ROOT_SCHEMA_V1 || !valid_session_id(&root.source_session_id) {
            return Err("manual Rust root is invalid".to_owned());
        }
        Ok(Some(root))
    }

    /// Captures a shared-Exact combat entry for seeded research branching.
    /// The payload is the original game's autosave, but the ability to restore
    /// it later is a lab capability and is not limited by the game's UI.
    pub fn capture_seeded_combat_checkpoint(
        &self,
        storage_directory: &Path,
        source_committed_actions: usize,
        act: u64,
        floor: i64,
    ) -> Result<ManualSeededCombatCheckpointV1, String> {
        if source_committed_actions == 0 || act == 0 || floor <= 0 {
            return Err("manual seeded combat checkpoint has an invalid coordinate".to_owned());
        }
        let storage_directory = storage_directory
            .canonicalize()
            .map_err(|error| format!("failed to resolve isolated Java storage: {error}"))?;
        let saves = storage_directory.join("saves");
        let saves = saves
            .canonicalize()
            .map_err(|error| format!("failed to resolve isolated Java saves: {error}"))?;
        if !saves.starts_with(&storage_directory) {
            return Err("isolated Java saves escaped their storage directory".to_owned());
        }
        let expected_class = RecordedJavaEvidenceStoreV1::load_manifest(
            &self.evidence_directory().join("manifest.json"),
        )?
        .bootstrap
        .player_class;
        let autosaves = fs::read_dir(&saves)
            .map_err(|error| format!("failed to read isolated Java saves: {error}"))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                crate::native::autosave_name_matches_class(&name, &expected_class)
            })
            .collect::<Vec<_>>();
        if autosaves.len() != 1 {
            return Err(format!(
                "manual combat checkpoint requires exactly one supported character autosave; found {}",
                autosaves.len()
            ));
        }
        let autosave = autosaves[0].path();
        let autosave_name = autosaves[0]
            .file_name()
            .into_string()
            .map_err(|_| "character autosave name is not UTF-8".to_owned())?;
        let checkpoint_root = self.directory.join("seeded-combat-checkpoints");
        fs::create_dir_all(&checkpoint_root)
            .map_err(|error| format!("failed to create seeded checkpoint store: {error}"))?;
        let directory = checkpoint_root.join(format!(
            "{source_committed_actions:08}-a{act:02}-f{floor:02}"
        ));
        if directory.exists() {
            let checkpoint = self.load_seeded_combat_checkpoint(source_committed_actions)?;
            if checkpoint.act != act || checkpoint.floor != floor {
                return Err(
                    "existing seeded checkpoint disagrees with its combat coordinate".to_owned(),
                );
            }
            return Ok(checkpoint);
        }
        fs::create_dir(&directory)
            .map_err(|error| format!("failed to create seeded combat checkpoint: {error}"))?;
        let captured_autosave = directory.join("autosave");
        fs::copy(&autosave, &captured_autosave)
            .map_err(|error| format!("failed to capture native combat autosave: {error}"))?;
        if fs::read(&autosave).map_err(|error| error.to_string())?
            != fs::read(&captured_autosave).map_err(|error| error.to_string())?
        {
            return Err("captured native combat autosave changed while copying".to_owned());
        }
        let backup = autosave.with_file_name(format!("{autosave_name}.backUp"));
        let backup_present = backup.is_file();
        if backup_present {
            fs::copy(&backup, directory.join("autosave.backUp"))
                .map_err(|error| format!("failed to capture native autosave backup: {error}"))?;
        }
        write_json_create_new(
            &directory.join("checkpoint.json"),
            &ManualSeededCombatCheckpointManifestV1 {
                schema: SEEDED_COMBAT_CHECKPOINT_SCHEMA_V1.to_owned(),
                source_session_id: self.session_id.clone(),
                source_committed_actions,
                act,
                floor,
                autosave_name,
                backup_present,
            },
        )?;
        self.load_seeded_combat_checkpoint(source_committed_actions)
    }

    pub fn load_seeded_combat_checkpoint(
        &self,
        source_committed_actions: usize,
    ) -> Result<ManualSeededCombatCheckpointV1, String> {
        let matches = self
            .seeded_combat_checkpoints()?
            .into_iter()
            .filter(|checkpoint| checkpoint.source_committed_actions == source_committed_actions)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "manual retry requires one seeded checkpoint for action {source_committed_actions}; found {}",
                matches.len()
            ));
        }
        Ok(matches.into_iter().next().unwrap())
    }

    pub fn latest_seeded_combat_checkpoint(
        &self,
    ) -> Result<Option<ManualSeededCombatCheckpointV1>, String> {
        let mut checkpoints = self.seeded_combat_checkpoints()?;
        checkpoints.sort_by_key(|checkpoint| checkpoint.source_committed_actions);
        if checkpoints
            .windows(2)
            .any(|pair| pair[0].source_committed_actions == pair[1].source_committed_actions)
        {
            return Err("manual seeded checkpoint action identities are duplicated".to_owned());
        }
        Ok(checkpoints.pop())
    }

    fn seeded_combat_checkpoints(&self) -> Result<Vec<ManualSeededCombatCheckpointV1>, String> {
        let root = self.directory.join("seeded-combat-checkpoints");
        let mut matches = Vec::new();
        if root.is_dir() {
            for entry in fs::read_dir(&root)
                .map_err(|error| format!("failed to read seeded checkpoint store: {error}"))?
            {
                let entry = entry
                    .map_err(|error| format!("failed to read native checkpoint entry: {error}"))?;
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                let manifest_path = entry.path().join("checkpoint.json");
                if !manifest_path.is_file() {
                    continue;
                }
                let bytes = fs::read(&manifest_path).map_err(|error| {
                    format!("failed to read seeded checkpoint manifest: {error}")
                })?;
                let manifest: ManualSeededCombatCheckpointManifestV1 =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        format!("failed to decode seeded checkpoint manifest: {error}")
                    })?;
                if manifest.schema != SEEDED_COMBAT_CHECKPOINT_SCHEMA_V1
                    || manifest.source_session_id != self.session_id
                    || manifest.source_committed_actions == 0
                    || manifest.act == 0
                    || manifest.floor <= 0
                    || !safe_file_name(&manifest.autosave_name)
                {
                    return Err("manual seeded combat checkpoint manifest is invalid".to_owned());
                }
                let directory = entry.path();
                if !directory.join("autosave").is_file()
                    || (manifest.backup_present && !directory.join("autosave.backUp").is_file())
                {
                    return Err("manual seeded combat checkpoint payload is incomplete".to_owned());
                }
                matches.push(ManualSeededCombatCheckpointV1 {
                    source_session_id: manifest.source_session_id,
                    source_committed_actions: manifest.source_committed_actions,
                    act: manifest.act,
                    floor: manifest.floor,
                    directory,
                    autosave_name: manifest.autosave_name,
                    backup_present: manifest.backup_present,
                });
            }
        }
        Ok(matches)
    }

    pub fn restore_seeded_combat_checkpoint(
        &self,
        checkpoint: &ManualSeededCombatCheckpointV1,
        storage_directory: &Path,
    ) -> Result<(), String> {
        if checkpoint.source_session_id != self.session_id {
            return Err(
                "seeded combat checkpoint belongs to a different manual session".to_owned(),
            );
        }
        let storage_directory = storage_directory
            .canonicalize()
            .map_err(|error| format!("failed to resolve isolated Java storage: {error}"))?;
        let saves = storage_directory
            .join("saves")
            .canonicalize()
            .map_err(|error| format!("failed to resolve isolated Java saves: {error}"))?;
        if !saves.starts_with(&storage_directory) || !safe_file_name(&checkpoint.autosave_name) {
            return Err("native combat restore target escaped isolated Java storage".to_owned());
        }
        let source = checkpoint.directory.join("autosave");
        let target = saves.join(&checkpoint.autosave_name);
        fs::copy(&source, &target)
            .map_err(|error| format!("failed to restore native combat autosave: {error}"))?;
        if fs::read(&source).map_err(|error| error.to_string())?
            != fs::read(&target).map_err(|error| error.to_string())?
        {
            return Err("restored native combat autosave differs from its checkpoint".to_owned());
        }
        let target_backup = saves.join(format!("{}.backUp", checkpoint.autosave_name));
        if checkpoint.backup_present {
            fs::copy(checkpoint.directory.join("autosave.backUp"), &target_backup)
                .map_err(|error| format!("failed to restore native autosave backup: {error}"))?;
        } else if target_backup.is_file() {
            fs::remove_file(&target_backup)
                .map_err(|error| format!("failed to remove stale autosave backup: {error}"))?;
        }
        Ok(())
    }

    pub fn next_epoch_directory(&self) -> Result<(u64, PathBuf), String> {
        let epochs = self.directory.join("epochs");
        let mut ordinals = fs::read_dir(&epochs)
            .map_err(|error| format!("failed to read manual epochs: {error}"))?
            .map(|entry| {
                let entry =
                    entry.map_err(|error| format!("failed to read manual epoch: {error}"))?;
                let kind = entry
                    .file_type()
                    .map_err(|error| format!("failed to inspect manual epoch: {error}"))?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "manual epoch name is not UTF-8".to_owned())?;
                if !kind.is_dir()
                    || name.len() != 8
                    || !name.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return Err(format!(
                        "manual epoch store contains unexpected entry {name:?}"
                    ));
                }
                name.parse::<u64>()
                    .map_err(|_| format!("manual epoch ordinal {name:?} is invalid"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        ordinals.sort_unstable();
        for (expected, actual) in ordinals.iter().copied().enumerate() {
            if actual != expected as u64 {
                return Err("manual epoch ordinals are not contiguous".to_owned());
            }
        }
        let ordinal = u64::try_from(ordinals.len())
            .map_err(|_| "manual epoch count does not fit u64".to_owned())?;
        Ok((ordinal, epochs.join(format!("{ordinal:08}"))))
    }

    pub fn repository(&self) -> &Path {
        &self.repository
    }
}

fn valid_session_id(value: &str) -> bool {
    value.strip_prefix("manual-").is_some_and(|suffix| {
        suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn safe_file_name(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .file_name()
            .is_some_and(|name| name == value)
}

fn manual_root(repository: &Path) -> PathBuf {
    repository.join(".oracle-lab").join(MANUAL_ROOT)
}

pub(crate) fn write_json_create_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode active manual session: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to publish active manual session: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist active manual session: {error}"))
}

fn write_json_replace<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode active manual session: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("failed to replace active manual session: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist active manual session: {error}"))
}

/// Show the source once, not once per row. Bare row numbers are not commands.
pub fn render_manual_action_view_v1(
    snapshot: &ParitySnapshotV1,
    view: &sts_parity_lockstep::ActionViewV1,
    labels: &[(SemanticActionV1, String)],
) -> Result<String, String> {
    let mut rows = view.actions().clone();
    for (index, row) in rows.actions.iter_mut().enumerate() {
        row.handle = index.to_string();
    }
    let text = render_manual_observation_v1(snapshot, &rows, labels)?;
    Ok(text.replacen(
        "Actions:",
        &format!("Actions | select {}:<number>:", view.reference_prefix()),
        1,
    ))
}

pub fn render_manual_observation_v1(
    snapshot: &ParitySnapshotV1,
    actions: &SemanticActionSpaceV1,
    labels: &[(SemanticActionV1, String)],
) -> Result<String, String> {
    let run: NaturalRunStateV2 = decode_domain(snapshot, ParityDomainV1::Run)?;
    let room: Value = decode_domain(snapshot, ParityDomainV1::Room)?;
    let deck: Value = decode_domain(snapshot, ParityDomainV1::MasterDeck)?;
    let relics: Value = decode_domain(snapshot, ParityDomainV1::Relics)?;
    let potions: Value = decode_domain(snapshot, ParityDomainV1::Potions)?;
    let combat: NaturalRunCombatV4 = decode_domain(snapshot, ParityDomainV1::Combat)?;

    let mut lines = vec![format!(
        "Exact | Act {} F{} | HP {}/{} | Gold {} | {}",
        run.act,
        run.floor,
        run.current_hp,
        run.max_hp,
        run.gold,
        room_summary(&room),
    )];
    lines.push(format!("Deck: {}", card_collection_summary(&deck)?));
    lines.push(format!("Relics: {}", relic_summary(&relics)?));
    lines.push(format!("Potions: {}", potion_summary(&potions)?));

    let combat_cards = match &combat {
        NaturalRunCombatV4::Inactive => BTreeMap::new(),
        NaturalRunCombatV4::PlayerDecision { state } => {
            lines.push(format!(
                "Turn {} | Energy {} | Block {} | Draw {} Discard {} Exhaust {}",
                state.turn,
                state.player.energy,
                state.player.block,
                state.zones.draw_next_first.len(),
                state.zones.discard_bottom_first.len(),
                state.zones.exhaust_by_binding.len(),
            ));
            lines.push(format!(
                "Hand: {}",
                state
                    .zones
                    .hand
                    .iter()
                    .map(|card| {
                        let label = format_card(
                            &card.card_id,
                            u64::from(card.upgrades),
                            Some(card.cost_for_turn),
                        );
                        match card.base_damage {
                            Some(damage) => format!("{label} {{base damage {damage}}}"),
                            None => label,
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            for (index, monster) in state.monsters.iter().enumerate() {
                let intent = monster.public_intent.kind.as_deref().map_or_else(
                    || "hidden intent".to_owned(),
                    |kind| {
                        render_public_intent(
                            kind,
                            monster.public_intent.damage_per_hit,
                            monster.public_intent.hits,
                        )
                    },
                );
                let powers = power_summary(&monster.powers);
                lines.push(format!(
                    "M{index} {} {}/{} +{} | {}{}",
                    name::<EnemyId>(&monster.enemy_id),
                    monster.current_hp,
                    monster.max_hp,
                    monster.block,
                    intent,
                    if powers.is_empty() {
                        String::new()
                    } else {
                        format!(" | {powers}")
                    }
                ));
            }
            if !state.player_powers.is_empty() {
                lines.push(format!("Powers: {}", power_summary(&state.player_powers)));
            }
            state
                .zones
                .hand
                .iter()
                .map(|card| {
                    (
                        card.binding.clone(),
                        format_card(
                            &card.card_id,
                            u64::from(card.upgrades),
                            Some(card.cost_for_turn),
                        ),
                    )
                })
                .collect()
        }
    };
    let deck_cards = deck_card_labels(&deck)?;

    lines.push("Actions:".to_owned());
    lines.extend(actions.actions.iter().map(|action| {
        if let Some((_, label)) = labels
            .iter()
            .find(|(semantic, _)| semantic == &action.action)
        {
            format!(
                "{} {}",
                action.handle,
                label.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        } else {
            format_action(action, &combat_cards, &deck_cards)
        }
    }));
    Ok(lines.join("\n"))
}

pub fn manual_natural_combat_entry_coordinate_v1(
    snapshot: &ParitySnapshotV1,
) -> Result<Option<(u64, i64)>, String> {
    if snapshot.boundary_kind != "natural_combat" {
        return Ok(None);
    }
    let combat: NaturalRunCombatV4 = decode_domain(snapshot, ParityDomainV1::Combat)?;
    if !matches!(combat, NaturalRunCombatV4::PlayerDecision { ref state } if state.turn == 1) {
        return Ok(None);
    }
    let run: NaturalRunStateV2 = decode_domain(snapshot, ParityDomainV1::Run)?;
    Ok(Some((run.act, run.floor)))
}

pub fn render_manual_map_v1(snapshot: &ParitySnapshotV1) -> Result<String, String> {
    let map: Value = decode_domain(snapshot, ParityDomainV1::Map)?;
    let nodes = map["nodes"]
        .as_array()
        .ok_or_else(|| "canonical map omitted its nodes".to_owned())?;
    let labels = nodes
        .iter()
        .map(|node| {
            Ok((
                required_str(node, "id")?.to_owned(),
                map_room_label(
                    required_str(node, "room")?,
                    node["emerald_key"].as_bool() == Some(true),
                ),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut lines = vec![format!(
        "Map | current {} | boss {}",
        map["current"].as_str().unwrap_or("start"),
        map["boss"].as_str().unwrap_or("unknown")
    )];
    for node in nodes {
        let id = required_str(node, "id")?;
        let floor = node["floor"]
            .as_i64()
            .ok_or_else(|| "canonical map node omitted its floor".to_owned())?;
        let x = node["x"]
            .as_i64()
            .ok_or_else(|| "canonical map node omitted its x coordinate".to_owned())?;
        let label = labels
            .get(id)
            .ok_or_else(|| "canonical map node lost its label".to_owned())?;
        let links = node["links"]
            .as_array()
            .ok_or_else(|| "canonical map node omitted its links".to_owned())?
            .iter()
            .map(|target| {
                let target = target
                    .as_str()
                    .ok_or_else(|| "canonical map link is not a node ID".to_owned())?;
                Ok(format!(
                    "{} [{}]",
                    node_coordinate(target).unwrap_or_else(|| target.to_owned()),
                    labels.get(target).map(String::as_str).unwrap_or("boss")
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        lines.push(format!(
            "F{floor:02} X{x} [{label}] -> {}",
            if links.is_empty() {
                "-".to_owned()
            } else {
                links.join(", ")
            }
        ));
    }
    Ok(lines.join("\n"))
}

pub(crate) fn decode_domain<T>(
    snapshot: &ParitySnapshotV1,
    domain: ParityDomainV1,
) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let observation = snapshot
        .domains
        .get(&domain)
        .ok_or_else(|| format!("manual view omitted {domain:?}"))?;
    let DomainEvidenceV1::Observed { value, .. } = &observation.evidence else {
        return Err(format!("manual view cannot display unobserved {domain:?}"));
    };
    serde_json::from_value(value.clone())
        .map_err(|error| format!("manual view cannot decode {domain:?}: {error}"))
}

fn room_summary(room: &Value) -> String {
    let kind = room["kind"].as_str().unwrap_or("unknown");
    let detail = room["stage"]
        .as_str()
        .or_else(|| room["event_id"].as_str())
        .or_else(|| room["current"].as_str());
    detail.map_or_else(|| kind.to_owned(), |detail| format!("{kind} {detail}"))
}

pub(crate) fn card_collection_summary(value: &Value) -> Result<String, String> {
    let cards = value
        .as_array()
        .ok_or_else(|| "canonical deck is not an array".to_owned())?;
    let mut counts = BTreeMap::<(String, u64), usize>::new();
    for card in cards {
        let id = required_str(card, "card_id")?.to_owned();
        let upgrades = card["upgrades"]
            .as_u64()
            .ok_or_else(|| "canonical deck card omitted upgrades".to_owned())?;
        *counts.entry((id, upgrades)).or_default() += 1;
    }
    Ok(counts
        .into_iter()
        .map(|((id, upgrades), count)| {
            let card = format_card(&id, upgrades, None);
            if count == 1 {
                card
            } else {
                format!("{count}x {card}")
            }
        })
        .collect::<Vec<_>>()
        .join(", "))
}

pub(crate) fn deck_card_labels(value: &Value) -> Result<BTreeMap<String, String>, String> {
    let cards = value
        .as_array()
        .ok_or_else(|| "canonical deck is not an array".to_owned())?;
    let mut totals = BTreeMap::<String, usize>::new();
    for card in cards {
        *totals
            .entry(required_str(card, "card_id")?.to_owned())
            .or_default() += 1;
    }
    let mut ordinals = BTreeMap::<String, usize>::new();
    let mut labels = BTreeMap::new();
    for card in cards {
        let binding = required_str(card, "binding")?;
        let card_id = required_str(card, "card_id")?;
        let upgrades = card["upgrades"]
            .as_u64()
            .ok_or_else(|| "canonical deck card omitted upgrades".to_owned())?;
        let ordinal = ordinals.entry(card_id.to_owned()).or_default();
        *ordinal += 1;
        let mut label = format_card(card_id, upgrades, None);
        if totals.get(card_id).copied().unwrap_or_default() > 1 {
            label.push_str(&format!(" #{}", *ordinal));
        }
        if labels.insert(binding.to_owned(), label).is_some() {
            return Err("canonical deck repeats a card binding".to_owned());
        }
    }
    Ok(labels)
}

pub(crate) fn relic_summary(value: &Value) -> Result<String, String> {
    let relics = value
        .as_array()
        .ok_or_else(|| "canonical relics are not an array".to_owned())?;
    if relics.is_empty() {
        return Ok("-".to_owned());
    }
    relics
        .iter()
        .map(|relic| {
            let id = name::<RelicId>(required_str(relic, "id")?);
            Ok(match relic["counter"].as_i64() {
                Some(counter) if counter >= 0 => format!("{id}({counter})"),
                _ => id.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map(|items| items.join(", "))
}

fn potion_summary(value: &Value) -> Result<String, String> {
    let potions = value
        .as_array()
        .ok_or_else(|| "canonical potions are not an array".to_owned())?;
    Ok(potions
        .iter()
        .map(|potion| {
            let slot = potion["slot"].as_u64().unwrap_or_default();
            format!(
                "P{slot} {}",
                name::<PotionId>(potion["id"].as_str().unwrap_or("empty"))
            )
        })
        .collect::<Vec<_>>()
        .join(", "))
}

fn power_summary(powers: &[sts_parity_lockstep::NaturalCombatPowerV4]) -> String {
    powers
        .iter()
        .map(|p| power(&p.power_id, i64::from(p.amount)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_public_intent(kind: &str, damage_per_hit: Option<i32>, hits: Option<u8>) -> String {
    if !kind.starts_with("attack") {
        return kind.replace('_', " ");
    }
    let mut rendered = match (damage_per_hit, hits) {
        (Some(damage), Some(hits)) if hits > 1 => format!("attack {damage}x{hits}"),
        (Some(damage), _) => format!("attack {damage}"),
        _ => "attack".to_owned(),
    };
    if let Some(extra) = kind.strip_prefix("attack_") {
        rendered.push_str(" + ");
        rendered.push_str(&extra.replace('_', " "));
    }
    rendered
}

fn format_action(
    candidate: &SemanticActionHandleV1,
    combat_cards: &BTreeMap<String, String>,
    deck_cards: &BTreeMap<String, String>,
) -> String {
    format!(
        "{} {}",
        candidate.handle,
        action_label(&candidate.action, combat_cards, deck_cards)
    )
}

fn action_label(
    action: &SemanticActionV1,
    combat_cards: &BTreeMap<String, String>,
    deck_cards: &BTreeMap<String, String>,
) -> String {
    match action {
        SemanticActionV1::PlayCard {
            card_binding,
            card_id,
            target,
        } => {
            let card = combat_cards
                .get(card_binding)
                .cloned()
                .unwrap_or_else(|| name::<CardId>(card_id));
            target.as_ref().map_or_else(
                || format!("play {card}"),
                |target| format!("play {card} -> {}", target_summary(target, deck_cards)),
            )
        }
        SemanticActionV1::UsePotion {
            slot,
            potion_id,
            target,
        } => {
            let potion_id = name::<PotionId>(potion_id);
            target.as_ref().map_or_else(
                || format!("use P{slot} {potion_id}"),
                |target| {
                    format!(
                        "use P{slot} {potion_id} -> {}",
                        target_summary(target, deck_cards)
                    )
                },
            )
        }
        SemanticActionV1::DiscardPotion { slot, potion_id } => {
            format!("discard P{slot} {}", name::<PotionId>(potion_id))
        }
        SemanticActionV1::EndTurn => "end turn".to_owned(),
        SemanticActionV1::Proceed { .. } => "Continue".into(),
        SemanticActionV1::Cancel { surface } => match surface {
            sts_parity_lockstep::DecisionSurfaceV1::Map => "Back to room".into(),
            sts_parity_lockstep::DecisionSurfaceV1::CardReward => "Skip card reward".into(),
            sts_parity_lockstep::DecisionSurfaceV1::BossRelic => "Skip Boss relic".into(),
            _ => "Back".into(),
        },
        SemanticActionV1::Select { surface, choice } => {
            if *surface == sts_parity_lockstep::DecisionSurfaceV1::Shop
                && choice.kind == "deck_card"
            {
                let price = choice
                    .attributes
                    .get("price")
                    .map(|value| match value {
                        SemanticScalarV1::Integer(price) => price.to_string(),
                        _ => "?".to_owned(),
                    })
                    .unwrap_or_else(|| "?".to_owned());
                format!(
                    "purge {} ({price} gold)",
                    target_summary(choice, deck_cards)
                )
            } else if *surface == sts_parity_lockstep::DecisionSurfaceV1::Shop
                && matches!(
                    choice.kind.as_str(),
                    "shop_card" | "shop_relic" | "shop_potion"
                )
            {
                let title = match choice.kind.as_str() {
                    "shop_card" => format_card(
                        &choice.id,
                        match choice.attributes.get("upgrades") {
                            Some(SemanticScalarV1::Integer(n)) => (*n).max(0) as u64,
                            _ => 0,
                        },
                        None,
                    ),
                    "shop_relic" => name::<RelicId>(&choice.id),
                    _ => name::<PotionId>(&choice.id),
                };
                let price = match choice.attributes.get("price") {
                    Some(SemanticScalarV1::Integer(n)) => n.to_string(),
                    _ => "?".into(),
                };
                format!("Buy {title} ({price} gold)")
            } else if choice.kind == "event_option" && choice.id == "matchandkeep" {
                match choice.attributes.get("screen") {
                    Some(SemanticScalarV1::Integer(0)) => "Play Match and Keep".into(),
                    Some(SemanticScalarV1::Integer(1)) => "Start matching".into(),
                    Some(SemanticScalarV1::Integer(3)) => "Leave".into(),
                    _ => format!("Choose {}", target_summary(choice, deck_cards)),
                }
            } else if choice.kind == "map_node" {
                let room = choice
                    .attributes
                    .get("room")
                    .and_then(|value| match value {
                        SemanticScalarV1::Text(room) => Some(room.as_str()),
                        _ => None,
                    })
                    .unwrap_or("Room");
                if room == "boss" {
                    "Enter Boss".into()
                } else {
                    let coordinate = choice.id.trim_start_matches(['F', 'f']);
                    let (floor, column) = coordinate
                        .split_once(['X', 'x'])
                        .unwrap_or((coordinate, "?"));
                    let column = column
                        .parse::<usize>()
                        .map(|x| (x + 1).to_string())
                        .unwrap_or_else(|_| column.into());
                    let row = floor
                        .parse::<u64>()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|_| floor.into());
                    format!(
                        "{} (map row {row}, column {column})",
                        crate::presentation_names::room(room)
                    )
                }
            } else if matches!(
                choice.kind.as_str(),
                "run_reward" | "reward_card" | "discovery_card"
            ) {
                format!(
                    "{} {}",
                    if choice.kind == "run_reward" {
                        "Take"
                    } else {
                        "Choose"
                    },
                    crate::decision_presentation::offer(choice)
                )
            } else if *choice == sts_parity_lockstep::natural_singing_bowl_choice_v1() {
                "Singing Bowl | gain 2 max HP".into()
            } else if choice.kind == "boss_relic" {
                format!("Take {}", name::<RelicId>(&choice.id))
            } else if choice.kind == "treasure_chest" && choice.id == "open" {
                "Open chest".into()
            } else if choice.kind == "campfire_option" {
                match choice.id.as_str() {
                    "smith" => "Upgrade a card".into(),
                    "rest" => "Rest".into(),
                    "recall" => "Recall (Ruby Key)".into(),
                    other => other.into(),
                }
            } else {
                format!("Choose {}", target_summary(choice, deck_cards))
            }
        }
        SemanticActionV1::SubmitSelection { choices, .. } => format!(
            "Choose [{}]",
            choices
                .iter()
                .map(|choice| target_summary(choice, deck_cards))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Descriptions share the exact semantic actions, never row indices or names.
pub fn describe_manual_actions_v1(
    snapshot: &ParitySnapshotV1,
    actions: &SemanticActionSpaceV1,
    overrides: &[(SemanticActionV1, String)],
) -> Result<BTreeMap<String, String>, String> {
    let deck: Value = decode_domain(snapshot, ParityDomainV1::MasterDeck)?;
    let deck = deck_card_labels(&deck)?;
    let combat: sts_parity_lockstep::NaturalRunCombatV4 =
        decode_domain(snapshot, ParityDomainV1::Combat)?;
    let room: Value = decode_domain(snapshot, ParityDomainV1::Room)?;
    let scry = room["kind"] == "combat_grid_select" && room["selection_kind"] == "scry";
    let cards = match combat {
        sts_parity_lockstep::NaturalRunCombatV4::PlayerDecision { state } => state
            .zones
            .hand
            .iter()
            .map(|c| {
                (
                    c.binding.clone(),
                    format_card(&c.card_id, u64::from(c.upgrades), Some(c.cost_for_turn)),
                )
            })
            .collect(),
        _ => BTreeMap::new(),
    };
    Ok(actions
        .actions
        .iter()
        .map(|candidate| {
            let label = overrides
                .iter()
                .find(|(a, _)| a == &candidate.action)
                .map(|(_, text)| text.clone())
                .unwrap_or_else(|| match &candidate.action {
                    SemanticActionV1::SubmitSelection { choices, .. } if scry => {
                        if choices.is_empty() {
                            "Discard none".into()
                        } else {
                            format!(
                                "Discard {}",
                                choices
                                    .iter()
                                    .map(|choice| target_summary(choice, &deck))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        }
                    }
                    _ => action_label(&candidate.action, &cards, &deck),
                });
            (candidate.handle.clone(), label)
        })
        .collect())
}

fn target_summary(choice: &SemanticChoiceV1, deck_cards: &BTreeMap<String, String>) -> String {
    if choice.kind == "match_card" {
        return choice.id.clone();
    }
    if matches!(
        choice.kind.as_str(),
        "hand_card" | "draw_card" | "discard_card" | "exhaust_card"
    ) {
        let id = match choice.attributes.get("card_id") {
            Some(SemanticScalarV1::Text(id)) => id.as_str(),
            _ => "Card",
        };
        let upgrades = match choice.attributes.get("upgrades") {
            Some(SemanticScalarV1::Integer(n)) => (*n).max(0) as u64,
            _ => 0,
        };
        return format_card(id, upgrades, None);
    }
    if choice.kind == "monster" {
        return format!("{} #{}", name::<EnemyId>(&choice.id), choice.occurrence + 1);
    }
    if choice.kind == "deck_card" {
        return deck_cards
            .get(&choice.id)
            .cloned()
            .unwrap_or_else(|| "unresolved deck card".to_owned());
    }
    let id = if choice.kind == "map_node" {
        node_coordinate(&choice.id).unwrap_or_else(|| choice.id.clone())
    } else {
        choice.id.clone()
    };
    let mut detail = format!("{}:{id}", choice.kind);
    if choice.occurrence > 0 {
        detail.push_str(&format!("#{}", choice.occurrence));
    }
    if !choice.attributes.is_empty() {
        let attributes = choice
            .attributes
            .iter()
            .map(|(key, value)| match value {
                sts_parity_lockstep::SemanticScalarV1::Text(value) => format!("{key}={value}"),
                sts_parity_lockstep::SemanticScalarV1::Integer(value) => format!("{key}={value}"),
                sts_parity_lockstep::SemanticScalarV1::Boolean(value) => format!("{key}={value}"),
            })
            .collect::<Vec<_>>();
        detail.push_str(&format!(" ({})", attributes.join(", ")));
    }
    detail
}

/// Display-only notation shared by decision views and CLI diagnostics.
pub(crate) fn format_card(id: &str, upgrades: u64, cost: Option<i32>) -> String {
    let mut card = name::<CardId>(id);
    if upgrades > 0 {
        card.push('+');
        if upgrades > 1 || sts_java_parity_protocol::canonical_game_id(id) == "searingblow" {
            card.push_str(&upgrades.to_string());
        }
    }
    if let Some(cost) = cost {
        card.push_str(&match cost {
            -1 => "[Xc]".into(),
            -2 => "[unplayable]".into(),
            n => format!("[{n}c]"),
        });
    }
    card
}

fn map_room_label(room: &str, emerald_key: bool) -> String {
    let label = match room {
        "monster" => "M",
        "elite" => "E",
        "event" | "question_mark" => "?",
        "shop" => "$",
        "rest" => "R",
        "treasure" => "T",
        "boss" => "B",
        other => other,
    };
    if emerald_key {
        format!("{label}+key")
    } else {
        label.to_owned()
    }
}

fn node_coordinate(id: &str) -> Option<String> {
    let body = id.strip_prefix('F').or_else(|| id.strip_prefix('f'))?;
    let (floor, x) = body.split_once('X').or_else(|| body.split_once('x'))?;
    let x = x.parse::<i64>().ok()?;
    Some(format!("F{floor} X{x}"))
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("canonical value omitted {field:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use sts_oracle_run_control::{
        content::events::neow::NeowEligibilityV1,
        eval::run_control::{RunControlConfig, RunControlSession},
    };
    use sts_parity_lockstep::{LockstepAdapterV1, SemanticActionV1};
    use sts_parity_lockstep_rust::{natural_run_bootstrap_v1, RustLockstepAdapterV1};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn parser_accepts_only_short_unambiguous_manual_commands() {
        assert_eq!(parse_manual_command_v1(""), Ok(ManualCommandV1::Observe));
        assert_eq!(
            parse_manual_command_v1("VabcdefghijLMNOPQRST_-z:007"),
            Ok(ManualCommandV1::Step(
                "VabcdefghijLMNOPQRST_-z:7".to_owned()
            ))
        );
        assert_eq!(
            parse_manual_command_v1("step VabcdefghijLMNOPQRST_-z:2"),
            Ok(ManualCommandV1::Step(
                "VabcdefghijLMNOPQRST_-z:2".to_owned()
            ))
        );
        assert_eq!(parse_manual_command_v1("map"), Ok(ManualCommandV1::Map));
        assert_eq!(
            parse_manual_command_v1("drafts"),
            Ok(ManualCommandV1::Drafts)
        );
        assert_eq!(
            parse_manual_command_v1("adopt D01234567"),
            Ok(ManualCommandV1::Adopt(PathBuf::from("D01234567")))
        );
        assert_eq!(
            parse_manual_command_v1(r#"adopt "D:\private drafts\win.json""#),
            Ok(ManualCommandV1::Adopt(PathBuf::from(
                r"D:\private drafts\win.json"
            )))
        );
        assert!(parse_manual_command_v1("step").is_err());
        assert!(parse_manual_command_v1("A2").is_err());
        assert!(parse_manual_command_v1("step A2").is_err());
        assert!(parse_manual_command_v1("plan rewards potion potion cards").is_err());
        assert!(parse_manual_command_v1("plan rewards cards gold").is_err());
        assert!(parse_manual_command_v1("plan rewards potion gold cards").is_ok());
        assert!(parse_manual_command_v1("A1 A2").is_err());
        assert!(parse_manual_command_v1("owner auto").is_err());
    }

    #[test]
    fn one_active_selector_owns_contiguous_process_epochs() {
        let repository = temp_repository("manual-session");
        let session = ManualSessionV1::prepare(&repository).unwrap();
        session.publish_active().unwrap();
        assert!(ManualSessionV1::prepare(&repository)
            .unwrap_err()
            .contains("already active"));
        assert_eq!(ManualSessionV1::load_active(&repository).unwrap(), session);
        assert_eq!(session.next_epoch_directory().unwrap().0, 0);
        fs::create_dir(session.next_epoch_directory().unwrap().1).unwrap();
        assert_eq!(session.next_epoch_directory().unwrap().0, 1);
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn replacement_preserves_old_evidence_until_explicit_publish() {
        let repository = temp_repository("manual-replacement");
        let old = ManualSessionV1::prepare(&repository).unwrap();
        old.publish_active().unwrap();
        fs::write(old.directory.join("old-evidence"), b"kept").unwrap();

        let replacement = ManualSessionV1::prepare_replacement(&repository).unwrap();
        assert_eq!(ManualSessionV1::load_active(&repository).unwrap(), old);
        let checkpoint = RunControlSessionCheckpointV1::from_session(&RunControlSession::new(
            RunControlConfig::default(),
        ));
        replacement
            .publish_rust_root(old.session_id(), 17, checkpoint.clone())
            .unwrap();
        let root = replacement.load_rust_root().unwrap().unwrap();
        assert_eq!(root.source_session_id, old.session_id());
        assert_eq!(root.source_committed_actions, 17);
        assert_eq!(root.checkpoint, checkpoint);
        // A diagnostic payload from another engine schema cannot prevent replay
        // of the still valid source coordinates and original Java evidence.
        let path = replacement.directory().join("rust-root.json");
        let mut envelope: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        envelope["checkpoint"] = serde_json::json!({"diagnostic_schema": "unavailable"});
        fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(replacement.load_rust_root().is_err());
        let source = replacement.load_source_prefix().unwrap().unwrap();
        assert_eq!(source.source_session_id, old.session_id());
        assert_eq!(source.source_committed_actions, 17);
        envelope["source_session_id"] = serde_json::json!("../wrong-source");
        fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(replacement.load_source_prefix().is_err());

        replacement.publish_replacing_active().unwrap();
        assert_eq!(
            ManualSessionV1::load_active(&repository).unwrap(),
            replacement
        );
        assert_eq!(
            fs::read(old.directory.join("old-evidence")).unwrap(),
            b"kept"
        );
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn seeded_combat_checkpoint_restores_only_its_isolated_save_generation() {
        let repository = temp_repository("manual-seeded-combat-checkpoint");
        let session = ManualSessionV1::prepare(&repository).unwrap();
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        RecordedJavaEvidenceStoreV1::create_new(&session.evidence_directory(), bootstrap).unwrap();
        let storage = repository.join("isolated-java-storage");
        let saves = storage.join("saves");
        fs::create_dir_all(&saves).unwrap();
        fs::write(saves.join("IRONCLAD.autosave"), b"combat-entry").unwrap();
        fs::write(saves.join("THE_SILENT.autosave"), b"foreign-character").unwrap();
        fs::write(
            saves.join("IRONCLAD.autosave.backUp"),
            b"previous-generation",
        )
        .unwrap();

        let checkpoint = session
            .capture_seeded_combat_checkpoint(&storage, 17, 1, 7)
            .unwrap();
        assert_eq!(checkpoint.source_committed_actions, 17);
        assert_eq!(checkpoint.act, 1);
        assert_eq!(checkpoint.floor, 7);
        fs::write(saves.join("IRONCLAD.autosave"), b"post-combat").unwrap();
        fs::write(saves.join("IRONCLAD.autosave.backUp"), b"later").unwrap();
        let later = session
            .capture_seeded_combat_checkpoint(&storage, 29, 1, 8)
            .unwrap();
        assert_eq!(later.source_committed_actions, 29);
        assert_eq!(later.floor, 8);
        assert_eq!(
            session.latest_seeded_combat_checkpoint().unwrap().unwrap(),
            later
        );

        session
            .restore_seeded_combat_checkpoint(&checkpoint, &storage)
            .unwrap();
        assert_eq!(
            fs::read(saves.join("IRONCLAD.autosave")).unwrap(),
            b"combat-entry"
        );
        assert_eq!(
            fs::read(saves.join("IRONCLAD.autosave.backUp")).unwrap(),
            b"previous-generation"
        );
        assert!(session.load_seeded_combat_checkpoint(18).is_err());

        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn compact_view_has_actions_but_no_hidden_draw_order_or_internal_identity() {
        let config = RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            player_class: "Ironclad",
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut adapter = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap,
            "test-build",
            "manual-view",
        )
        .unwrap();
        let snapshot = adapter.observe().unwrap();
        let actions = action_space(&snapshot);
        let rendered = render_manual_observation_v1(&snapshot, &actions, &[]).unwrap();

        assert!(rendered.starts_with("Exact | Act 1 F0"));
        assert!(rendered.contains("\nActions:\nA0 "));
        assert!(!rendered.contains("draw_next_first"));
        assert!(!rendered.contains("natural_run/starter"));
        assert!(!rendered.contains("test-build"));
    }

    #[test]
    fn shop_deck_target_is_explicitly_a_paid_purge() {
        let mut choice = SemanticChoiceV1::new("deck_card", "natural_run/starter/0", 0);
        choice
            .attributes
            .insert("price".to_owned(), SemanticScalarV1::Integer(75));
        let action = SemanticActionHandleV1 {
            handle: "A4".to_owned(),
            action: SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Shop,
                choice,
            },
        };
        let labels = BTreeMap::from([("natural_run/starter/0".to_owned(), "strike #1".to_owned())]);
        let text = format_action(&action, &BTreeMap::new(), &labels);
        assert!(text.starts_with("A4 purge strike #1"));
        assert!(text.contains("75 gold"));
        assert!(!text.contains("natural_run"));
    }

    #[test]
    fn manual_map_notation_keeps_coordinates_readable_and_marks_question_rooms() {
        let mut choice = SemanticChoiceV1::new("map_node", "F02X00", 0);
        choice.attributes.insert(
            "room".to_owned(),
            sts_parity_lockstep::SemanticScalarV1::Text("question_mark".to_owned()),
        );

        assert!(target_summary(&choice, &BTreeMap::new()).starts_with("map_node:F02 X0"));
        assert_eq!(map_room_label("question_mark", false), "?");
        assert_eq!(node_coordinate("F15X03").as_deref(), Some("F15 X3"));
    }

    #[test]
    fn deck_action_labels_distinguish_duplicates_without_exposing_bindings() {
        let deck = serde_json::json!([
            {"binding": "natural_run/starter/0", "card_id": "strike", "upgrades": 0},
            {"binding": "natural_run/starter/1", "card_id": "strike", "upgrades": 0},
            {"binding": "natural_run/starter/9", "card_id": "bash", "upgrades": 0}
        ]);
        let labels = deck_card_labels(&deck).unwrap();
        let choice = SemanticChoiceV1::new("deck_card", "natural_run/starter/0", 0);

        assert_eq!(target_summary(&choice, &labels), "Strike #1");
        assert_eq!(labels["natural_run/starter/1"], "Strike #2");
        assert_eq!(labels["natural_run/starter/9"], "Bash");
        assert!(!target_summary(&choice, &labels).contains("natural_run"));
    }

    #[test]
    fn compound_attack_intents_keep_damage_and_secondary_effect() {
        assert_eq!(
            render_public_intent("attack_defend", Some(7), Some(1)),
            "attack 7 + defend"
        );
        assert_eq!(
            render_public_intent("attack_debuff", Some(2), Some(3)),
            "attack 2x3 + debuff"
        );
        assert_eq!(render_public_intent("buff", None, None), "buff");
    }

    #[test]
    fn shop_purchases_show_game_names_and_prices() {
        for (kind, id, title) in [
            ("shop_card", "flamebarrier", "Flame Barrier+"),
            ("shop_relic", "sling", "Sling of Courage"),
            ("shop_potion", "dexteritypotion", "Dexterity Potion"),
        ] {
            let mut choice = SemanticChoiceV1::new(kind, id, 0);
            choice
                .attributes
                .insert("price".into(), SemanticScalarV1::Integer(84));
            choice
                .attributes
                .insert("upgrades".into(), SemanticScalarV1::Integer(1));
            let text = action_label(
                &SemanticActionV1::Select {
                    surface: sts_parity_lockstep::DecisionSurfaceV1::Shop,
                    choice,
                },
                &BTreeMap::new(),
                &BTreeMap::new(),
            );
            assert!(text.contains(title), "{text}");
            assert!(text.contains("84 gold"));
            assert!(!text.contains("shop_"));
        }
    }

    fn action_space(snapshot: &ParitySnapshotV1) -> SemanticActionSpaceV1 {
        let DomainEvidenceV1::Observed { value, .. } =
            &snapshot.domains[&ParityDomainV1::ActionSpace].evidence
        else {
            panic!("fixture action space is unobserved")
        };
        let actions = serde_json::from_value::<Vec<SemanticActionV1>>(value.clone()).unwrap();
        SemanticActionSpaceV1::new(actions).unwrap()
    }

    fn temp_repository(label: &str) -> PathBuf {
        let unique = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "sts-parity-{label}-{}-{unique}",
            std::process::id()
        ))
    }
}
