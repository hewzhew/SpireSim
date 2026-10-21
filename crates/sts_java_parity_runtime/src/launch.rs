use oracle_artifact_contract::ensure_depfile_artifact_fresh;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sts_java_parity_live::LiveStatus;
use sts_workbench_supervisor::{
    ExecutableImageStoreV1, ExistingProcessCandidateV1, OwnedStorageRegistryV1, OwnedStorageV1,
    RuntimeSessionBindingV1, SupervisedChildV1, SupervisedProcessSetV1, SupervisedProcessV1,
    SupervisedResourcesV1, SupervisedRuntimeV1,
};
use uuid::Uuid;
use zip::ZipArchive;

use crate::{JavaParityRuntimeErrorV1, JavaParityRuntimeOwnerV1};

const WORKSHOP_DIRECTORY: &str = r"C:\Program Files (x86)\Steam\steamapps\workshop\content\646570";
const LAUNCH_PROFILE: &str = "accelerated";
const RUN_CONTROL_PROFILE: &str = "parity_run_control";
const SCENARIO_CONTROL_PROFILE: &str = "parity_scenario_control";
const CANONICAL_FRESH_PROFILE_SUBSTRATE: &str = "canonical_a20h_lab_preferences_v1";
const CANONICAL_TUTORIAL_POLICY: &str = "precompleted_via_profile_v1";
const CAPTURED_NATIVE_PROFILE_SUBSTRATE: &str = "captured_native_lab_preferences_v1";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const MANAGED_JAVA_STDOUT_LOG: &str = "managed-java-stdout.log";
const MANAGED_JAVA_STDERR_LOG: &str = "managed-java-stderr.log";
const PROTOCOL_DESCRIPTOR_ENTRY: &str = "communicationmod-parity-protocol.json";
const CANONICAL_STS_TIPS: &[u8] = br#"{
  "NEOW_SKIP": "true",
  "NEOW_INTRO": "true",
  "NO_FTUE": "true",
  "COMBAT_TIP": "true",
  "BLOCK TIP": "true",
  "POWER_TIP": "true",
  "M_POWER_TIP": "true",
  "ENERGY_USE_TIP": "true",
  "SHUFFLE_TIP": "true",
  "POTION_TIP": "true",
  "CARD_REWARD_TIP": "true",
  "INTENT_TIP": "true",
  "RELIC_TIP": "true"
}
"#;
const CANONICAL_STS_SAVE_SLOTS: &[u8] = br#"{
  "DEFAULT_SLOT": "0",
  "PROFILE_NAME": "A20H Parity Lab",
  "COMPLETION": "0.0",
  "PLAYTIME": "0"
}
"#;
const CANONICAL_STS_PLAYER: &[u8] = br#"{
  "name": "A20H Parity Lab",
  "alias": "A20H_PARITY_LAB",
  "ftuePopupShown": "true",
  "IRONCLAD_SPIRITS": "1"
}
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JavaParityLaunchProfileV1 {
    RunControl,
    ScenarioControl,
}

impl JavaParityLaunchProfileV1 {
    fn as_str(self) -> &'static str {
        match self {
            Self::RunControl => RUN_CONTROL_PROFILE,
            Self::ScenarioControl => SCENARIO_CONTROL_PROFILE,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JavaParityInitialBoundaryV1 {
    FreshStart,
    NeutralContinue,
}

#[derive(Clone, Copy, Debug)]
pub enum JavaParityStorageSeedV1<'a> {
    Fresh,
    Native(&'a Path),
}

#[derive(Clone, Copy, Debug)]
pub struct JavaParityLaunchRequestV1<'a> {
    pub storage_registry: &'a OwnedStorageRegistryV1,
    pub storage_seed: JavaParityStorageSeedV1<'a>,
    pub profile: JavaParityLaunchProfileV1,
    pub initial_boundary: JavaParityInitialBoundaryV1,
}

/// A launch whose process tree and isolated storage are already supervised,
/// but whose stable frame/autosave binding may still be completed by a caller.
pub struct PreparedJavaParityRuntimeV1 {
    supervised: Option<SupervisedRuntimeV1>,
    launch_plan: PathBuf,
    live_output: PathBuf,
    bridge_image_id: String,
}

impl std::fmt::Debug for PreparedJavaParityRuntimeV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedJavaParityRuntimeV1")
            .field("closed", &self.supervised.is_none())
            .finish_non_exhaustive()
    }
}

impl PreparedJavaParityRuntimeV1 {
    pub fn launch_plan(&self) -> &Path {
        &self.launch_plan
    }

    pub fn live_output(&self) -> &Path {
        &self.live_output
    }

    pub fn storage_root(&self) -> Result<&Path, String> {
        self.supervised
            .as_ref()
            .map(SupervisedRuntimeV1::storage_root)
            .ok_or_else(|| "prepared Java parity runtime is closed".to_owned())
    }

    pub fn bridge_image_id(&self) -> &str {
        &self.bridge_image_id
    }

    pub fn runtime_epoch(&self) -> Result<&str, String> {
        self.supervised
            .as_ref()
            .map(|runtime| runtime.session().runtime_epoch())
            .ok_or_else(|| "prepared Java parity runtime is closed".to_owned())
    }

    pub fn status(&mut self) -> Result<LiveStatus, String> {
        let supervised = self
            .supervised
            .as_mut()
            .ok_or_else(|| "prepared Java parity runtime is closed".to_owned())?;
        let live = sts_java_parity_live::status(&self.live_output)?;
        let launch = live
            .manifest
            .launch
            .as_ref()
            .ok_or_else(|| "Java parity session omitted launch evidence".to_owned())?;
        if !same_existing_path(Path::new(&launch.plan_path), &self.launch_plan)? {
            return Err("Java parity session changed its launch plan".to_owned());
        }
        supervised
            .validate_running(&runtime_session_binding(&live)?)
            .map_err(|error| format!("Java parity runtime is unavailable: {error}"))?;
        Ok(live)
    }

    pub fn bind_autosave(&mut self, path: &Path, identity: &str) -> Result<(), String> {
        self.supervised
            .as_mut()
            .ok_or_else(|| "prepared Java parity runtime is closed".to_owned())?
            .bind_autosave(path, identity)
            .map_err(|error| error.to_string())
    }

    pub fn bind_frame(&mut self, frame_token: impl Into<String>) -> Result<(), String> {
        self.supervised
            .as_mut()
            .ok_or_else(|| "prepared Java parity runtime is closed".to_owned())?
            .bind_frame(frame_token)
            .map_err(|error| error.to_string())
    }

    pub fn into_owner(mut self) -> Result<JavaParityRuntimeOwnerV1, JavaParityRuntimeErrorV1> {
        JavaParityRuntimeOwnerV1::new(
            self.supervised
                .take()
                .ok_or(JavaParityRuntimeErrorV1::Closed)?,
            self.launch_plan.clone(),
            self.live_output.clone(),
        )
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        let Some(mut supervised) = self.supervised.take() else {
            return Ok(());
        };
        supervised
            .shutdown(
                Duration::from_secs(10),
                Duration::from_secs(5),
                Duration::from_secs(5),
            )
            .map_err(|error| error.to_string())
    }
}

impl Drop for PreparedJavaParityRuntimeV1 {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

pub fn launch_java_parity_runtime_v1(
    request: JavaParityLaunchRequestV1<'_>,
) -> Result<(PreparedJavaParityRuntimeV1, Duration), String> {
    validate_request(request)?;
    let started = Instant::now();
    let repository = request.storage_registry.repository();
    let game_directory = request.storage_registry.source_game_directory();
    validate_runtime_configuration(repository, game_directory)?;
    let storage = match request.storage_seed {
        JavaParityStorageSeedV1::Fresh => IsolatedStorageV1::fresh(request.storage_registry)?,
        JavaParityStorageSeedV1::Native(seed) => {
            IsolatedStorageV1::from_native(seed, request.storage_registry)?
        }
    };
    let launch_scope = create_launch_scope(repository)?;
    let launch_plan = create_launch_plan(
        repository,
        game_directory,
        &storage,
        request.profile,
        &launch_scope,
    )?;
    let live_output = launch_scope.live_output.clone();
    let storage_root = storage.root().to_path_buf();
    let game = launch_game(
        repository,
        game_directory,
        &storage_root,
        &launch_plan,
        request.profile,
    )?;
    let processes = SupervisedProcessSetV1::new(game);
    let mut resources = SupervisedResourcesV1::new(processes, storage.into_owned());
    let live = wait_for_owned_session(
        &mut resources,
        &live_output,
        &launch_plan,
        &storage_root.join("communication_mod_errors.log"),
    )?;
    let bridge = admit_live_bridge_process(&launch_scope.bridge_executable, &live_output, &live)?;
    resources
        .admit_companion(bridge)
        .map_err(|error| format!("failed to admit Java bridge process: {error}"))?;
    let supervised = SupervisedRuntimeV1::new(resources, runtime_session_binding(&live)?)
        .map_err(|error| format!("failed to bind Java runtime session: {error}"))?;
    validate_initial_boundary(&live, request.initial_boundary)?;
    Ok((
        PreparedJavaParityRuntimeV1 {
            supervised: Some(supervised),
            launch_plan,
            live_output,
            bridge_image_id: launch_scope.bridge_image_id,
        },
        started.elapsed(),
    ))
}

fn validate_request(request: JavaParityLaunchRequestV1<'_>) -> Result<(), String> {
    if storage_seed_matches_boundary(request.storage_seed, request.initial_boundary) {
        Ok(())
    } else {
        Err("Java parity storage seed and initial boundary disagree".to_owned())
    }
}

fn storage_seed_matches_boundary(
    seed: JavaParityStorageSeedV1<'_>,
    boundary: JavaParityInitialBoundaryV1,
) -> bool {
    matches!(
        (seed, boundary),
        (
            JavaParityStorageSeedV1::Fresh,
            JavaParityInitialBoundaryV1::FreshStart
        ) | (
            JavaParityStorageSeedV1::Native(_),
            JavaParityInitialBoundaryV1::NeutralContinue
        )
    )
}

struct IsolatedStorageV1 {
    owned: OwnedStorageV1,
    profile_substrate: &'static str,
    tutorial_policy: &'static str,
}

impl IsolatedStorageV1 {
    fn fresh(registry: &OwnedStorageRegistryV1) -> Result<Self, String> {
        let mut storage = Self::empty(
            registry,
            CANONICAL_FRESH_PROFILE_SUBSTRATE,
            CANONICAL_TUTORIAL_POLICY,
        )?;
        let game = storage.game_directory().to_path_buf();
        storage.populate(&game, false, false)?;
        storage.write_canonical_a20h_lab_preferences()?;
        Ok(storage)
    }

    fn write_canonical_a20h_lab_preferences(&self) -> Result<(), String> {
        configure_lab_preferences(self.root(), false)
    }

    fn from_native(seed: &Path, registry: &OwnedStorageRegistryV1) -> Result<Self, String> {
        let mut storage = Self::empty(
            registry,
            CAPTURED_NATIVE_PROFILE_SUBSTRATE,
            CANONICAL_TUTORIAL_POLICY,
        )?;
        storage.populate(seed, true, true)?;
        configure_lab_preferences(storage.root(), true)?;
        Ok(storage)
    }

    fn empty(
        registry: &OwnedStorageRegistryV1,
        profile_substrate: &'static str,
        tutorial_policy: &'static str,
    ) -> Result<Self, String> {
        Ok(Self {
            owned: registry.create().map_err(|error| error.to_string())?,
            profile_substrate,
            tutorial_policy,
        })
    }

    fn populate(
        &mut self,
        seed: &Path,
        include_preferences: bool,
        include_saves: bool,
    ) -> Result<(), String> {
        fs::hard_link(
            self.game_directory().join("desktop-1.0.jar"),
            self.root().join("desktop-1.0.jar"),
        )
        .map_err(|error| format!("failed to bind original game JAR: {error}"))?;
        create_junction(
            &self.game_directory().join("mods"),
            &self.root().join("mods"),
        )?;
        create_junction(&self.game_directory().join("jre"), &self.root().join("jre"))?;
        for directory in ["preferences", "betaPreferences", "saves"] {
            let destination = self.root().join(directory);
            fs::create_dir(&destination)
                .map_err(|error| format!("failed to create isolated {directory}: {error}"))?;
            let include = match directory {
                "preferences" | "betaPreferences" => include_preferences,
                "saves" => include_saves,
                _ => false,
            };
            let source = seed.join(directory);
            if include && source.is_dir() {
                copy_directory_contents(&source, &destination)?;
            }
        }
        for directory in ["runs", "sendToDevs"] {
            fs::create_dir(self.root().join(directory))
                .map_err(|error| format!("failed to create isolated {directory}: {error}"))?;
        }
        for file in [
            "config.json",
            "info.displayconfig",
            "steam_appid.txt",
            "twitchconfig.txt",
        ] {
            let source = self.game_directory().join(file);
            if source.is_file() {
                fs::copy(&source, self.root().join(file))
                    .map_err(|error| format!("failed to copy isolated launch file: {error}"))?;
            }
        }
        Ok(())
    }

    fn root(&self) -> &Path {
        self.owned.root()
    }

    fn game_directory(&self) -> &Path {
        self.owned.source_game_directory()
    }

    fn into_owned(self) -> OwnedStorageV1 {
        self.owned
    }
}

/// Apply only to the owned copy, before Java starts. Native save bytes and
/// progression preferences remain intact; slot, tutorial UI, fast mode, and muted audio are lab policy.
fn configure_lab_preferences(root: &Path, native: bool) -> Result<(), String> {
    for directory in ["preferences", "betaPreferences"] {
        let preferences = root.join(directory);
        let mut slots = read_preferences(&preferences.join("STSSaveSlots"))?;
        let slot = if native {
            native_profile_slot(root, &slots, directory == "betaPreferences")?
        } else {
            0
        };
        let slot_key = |name: &str| {
            if slot == 0 {
                name.to_owned()
            } else {
                format!("{slot}_{name}")
            }
        };
        slots.insert("DEFAULT_SLOT".to_owned(), slot.to_string());
        let name = slots.entry(slot_key("PROFILE_NAME")).or_default();
        if name.is_empty() {
            *name = "A20H Parity Lab".to_owned();
        }
        let profile_name = name.clone();
        let mut player = read_preferences(&preferences.join(slot_key("STSPlayer")))?;
        if !native {
            slots.extend(
                serde_json::from_slice::<BTreeMap<String, String>>(CANONICAL_STS_SAVE_SLOTS)
                    .map_err(|error| error.to_string())?,
            );
            player.extend(
                serde_json::from_slice::<BTreeMap<String, String>>(CANONICAL_STS_PLAYER)
                    .map_err(|error| error.to_string())?,
            );
        }
        player.entry("name".to_owned()).or_insert(profile_name);
        player.insert("ftuePopupShown".to_owned(), "true".to_owned());
        let mut tips = read_preferences(&preferences.join(slot_key("STSTips")))?;
        tips.extend(
            serde_json::from_slice::<BTreeMap<String, String>>(CANONICAL_STS_TIPS)
                .map_err(|error| error.to_string())?,
        );
        let mut gameplay = read_preferences(&preferences.join(slot_key("STSGameplaySettings")))?;
        gameplay.insert("Fast Mode".to_owned(), "true".to_owned());
        let mut sound = read_preferences(&preferences.join(slot_key("STSSound")))?;
        sound.insert("Master Volume".to_owned(), "0.0".to_owned());
        for (name, values) in [
            ("STSSaveSlots".to_owned(), slots),
            (slot_key("STSPlayer"), player),
            (slot_key("STSTips"), tips),
            (slot_key("STSGameplaySettings"), gameplay),
            (slot_key("STSSound"), sound),
        ] {
            let path = preferences.join(name);
            fs::write(
                &path,
                serde_json::to_vec_pretty(&values).map_err(|error| error.to_string())?,
            )
            .map_err(|error| {
                format!(
                    "failed to configure isolated preference {}: {error}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn read_preferences(path: &Path) -> Result<BTreeMap<String, String>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid isolated preference {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(format!(
            "failed to read isolated preference {}: {error}",
            path.display()
        )),
    }
}

fn native_profile_slot(
    root: &Path,
    slots: &BTreeMap<String, String>,
    beta: bool,
) -> Result<u8, String> {
    let configured = slots
        .get("DEFAULT_SLOT")
        .and_then(|slot| slot.parse::<u8>().ok())
        .filter(|slot| *slot < 3);
    let mut saved_slots = Vec::new();
    for slot in 0..3 {
        let prefix = if slot == 0 {
            String::new()
        } else {
            format!("{slot}_")
        };
        let suffix = if beta { ".autosaveBETA" } else { ".autosave" };
        if ["IRONCLAD", "THE_SILENT", "DEFECT", "WATCHER"]
            .iter()
            .any(|character| {
                ["", ".backUp"].iter().any(|backup| {
                    root.join("saves")
                        .join(format!("{prefix}{character}{suffix}{backup}"))
                        .is_file()
                })
            })
        {
            saved_slots.push(slot);
        }
    }
    if let Some(slot) =
        configured.filter(|slot| saved_slots.is_empty() || saved_slots.contains(slot))
    {
        return Ok(slot);
    }
    match saved_slots.as_slice() {
        [] => Ok(0),
        [slot] => Ok(*slot),
        _ => Err(
            "native storage has multiple save slots without an unambiguous DEFAULT_SLOT".to_owned(),
        ),
    }
}

struct LaunchScopeV1 {
    launch_plan: PathBuf,
    live_output: PathBuf,
    bridge_executable: PathBuf,
    bridge_image_id: String,
}

fn create_launch_scope(repository: &Path) -> Result<LaunchScopeV1, String> {
    let runtime_root = repository.join(".oracle-lab").join("java-parity-runtime");
    let launches = runtime_root.join("launches");
    fs::create_dir_all(&launches)
        .map_err(|error| format!("failed to create Java launch directory: {error}"))?;
    let root = launches.join(Uuid::new_v4().simple().to_string());
    fs::create_dir(&root)
        .map_err(|error| format!("failed to create Java launch scope: {error}"))?;
    let live_output = root.join("java-live");
    fs::create_dir(&live_output)
        .map_err(|error| format!("failed to create Java live output: {error}"))?;
    let bridge = ExecutableImageStoreV1::new(
        runtime_root.join("live-bridge-images"),
        "sts-java-parity-live",
    )
    .map_err(|error| format!("failed to configure Java bridge images: {error}"))?
    .materialize(&live_bridge_executable(repository))
    .map_err(|error| format!("failed to materialize Java bridge image: {error}"))?;
    Ok(LaunchScopeV1 {
        launch_plan: root.join("launch-plan.json"),
        live_output,
        bridge_executable: bridge.executable().to_path_buf(),
        bridge_image_id: bridge.image_id().to_owned(),
    })
}

#[derive(Serialize)]
struct FileIdentityV1 {
    path: String,
    length: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct ModMetadataV1 {
    modid: String,
    version: String,
}

#[derive(Serialize)]
struct ModIdentityV1 {
    id: String,
    version: String,
    jar: FileIdentityV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackagedProtocolDescriptorV1 {
    schema: String,
    name: String,
    major: u32,
    minor: u32,
    frame_schema: String,
}

fn create_launch_plan(
    repository: &Path,
    game_directory: &Path,
    storage: &IsolatedStorageV1,
    profile: JavaParityLaunchProfileV1,
    scope: &LaunchScopeV1,
) -> Result<PathBuf, String> {
    let base_mod = resolve_base_mod(game_directory)?;
    let super_fast = game_directory.join("mods").join("SuperFastMode.jar");
    let communication = game_directory.join("mods").join("CommunicationMod.jar");
    let selected_mods = vec![
        mod_identity(&base_mod, "basemod")?,
        mod_identity(&super_fast, "superfastmode")?,
        mod_identity(&communication, "CommunicationMod")?,
    ];
    let acceleration_config = acceleration_config()?;
    let properties = read_properties(&acceleration_config)?;
    let multiplier = acceleration_multiplier(&properties)?;
    let mut bridge_identity = file_identity(&scope.bridge_executable)?;
    bridge_identity.path = java_8_compatible_path(&scope.bridge_executable)?;
    let mod_the_spire = repository
        .parent()
        .ok_or_else(|| "repository has no parent for ModTheSpire".to_owned())?
        .join("lib")
        .join("ModTheSpire.jar");
    let plan = json!({
        "schema": "sts_java_play_launch_plan_v4",
        "created_at_unix_ms": now_unix_ms(),
        "profile": LAUNCH_PROFILE,
        "control_profile": profile.as_str(),
        "selected_mod_ids": ["basemod", "superfastmode", "CommunicationMod"],
        "selected_mods": selected_mods,
        "game_java": file_identity(&game_directory.join("jre").join("bin").join("javaw.exe"))?,
        "mod_the_spire": file_identity(&mod_the_spire)?,
        "acceleration": {
            "game_fast_mode": true,
            "mod": mod_identity(&super_fast, "superfastmode")?,
            "settings": {
                "config": file_identity(&acceleration_config)?,
                "values": {
                    "is_delta_multiplied": true,
                    "delta_multiplier": multiplier,
                    "is_instant_lerp": true
                }
            }
        },
        "bridge": {
            "kind": "communication_mod_stdio_v1",
            "executable": bridge_identity,
            "output_directory": java_8_compatible_path(&scope.live_output)?
        },
        "storage": {
            "class": "isolated_working_directory_v1",
            "profile_substrate": storage.profile_substrate,
            "tutorial_policy": storage.tutorial_policy,
            "working_directory": java_8_compatible_path(storage.root())?,
            "source_game_directory": java_8_compatible_path(game_directory)?,
            "isolation_id": storage.owned.isolation_id().hyphenated().to_string()
        }
    });
    write_json_new(&scope.launch_plan, &plan)?;
    Ok(scope.launch_plan.clone())
}

fn validate_runtime_configuration(repository: &Path, game_directory: &Path) -> Result<(), String> {
    let bridge = live_bridge_executable(repository);
    let communication = game_directory.join("mods").join("CommunicationMod.jar");
    let mod_the_spire = repository
        .parent()
        .ok_or_else(|| "repository has no parent for ModTheSpire".to_owned())?
        .join("lib")
        .join("ModTheSpire.jar");
    for required in [
        bridge.clone(),
        game_directory.join("desktop-1.0.jar"),
        game_directory.join("jre").join("bin").join("javaw.exe"),
        mod_the_spire,
        communication.clone(),
        game_directory.join("mods").join("SuperFastMode.jar"),
    ] {
        if !required.is_file() {
            return Err(format!(
                "required Java runtime artifact is absent: {}",
                required.display()
            ));
        }
    }
    require_jar_entry(
        &communication,
        "communicationmod/ManagedBridgeLaunchV1.class",
        "installed CommunicationMod does not support managed bridge endpoints",
    )?;
    validate_packaged_protocol_descriptor(&communication)?;
    ensure_depfile_artifact_fresh(
        &bridge,
        repository,
        &[
            repository.join("Cargo.toml"),
            repository.join("Cargo.lock"),
            repository.join(".cargo/config.toml"),
            repository.join("crates/sts_java_parity_live/Cargo.toml"),
            repository.join("crates/sts_java_parity_protocol/Cargo.toml"),
        ],
        "Java parity live bridge",
        "build sts_java_parity_live before launching lockstep",
    )?;
    let properties = read_properties(&acceleration_config()?)?;
    let multiplier = acceleration_multiplier(&properties)?;
    if properties.get("isDeltaMultiplied").map(String::as_str) != Some("true")
        || properties.get("isInstantLerp").map(String::as_str) != Some("true")
        || multiplier <= 1.0
    {
        return Err("SuperFastMode accelerated settings are not active".to_owned());
    }
    Ok(())
}

fn launch_game(
    repository: &Path,
    game_directory: &Path,
    storage_root: &Path,
    launch_plan: &Path,
    profile: JavaParityLaunchProfileV1,
) -> Result<SupervisedChildV1, String> {
    let mod_the_spire = repository
        .parent()
        .ok_or_else(|| "repository has no parent for ModTheSpire".to_owned())?
        .join("lib")
        .join("ModTheSpire.jar");
    // Runtime cleanup removes the isolated game storage. Keep process output
    // beside the durable launch evidence so a failed host remains diagnosable.
    let log_directory = launch_plan
        .parent()
        .ok_or("Java launch plan has no log directory")?;
    let stdout = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(log_directory.join(MANAGED_JAVA_STDOUT_LOG))
        .map_err(|error| format!("failed to create managed Java stdout log: {error}"))?;
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(log_directory.join(MANAGED_JAVA_STDERR_LOG))
        .map_err(|error| format!("failed to create managed Java stderr log: {error}"))?;
    let boundary = Java8ProcessBoundaryV1::new(&mod_the_spire, storage_root, launch_plan)?;
    let mut command = Command::new(game_directory.join("jre").join("bin").join("javaw.exe"));
    command
        .current_dir(&boundary.working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .arg("-jar")
        .arg(&boundary.mod_the_spire_argument)
        .arg("--skip-launcher")
        .arg("--skip-intro")
        .arg("--mods")
        .arg("basemod,superfastmode,CommunicationMod")
        .env("STS_JAVA_PLAY_LAUNCH_PROFILE", LAUNCH_PROFILE)
        .env(
            "STS_JAVA_PLAY_LAUNCH_PLAN",
            &boundary.launch_plan_environment,
        )
        .env("STS_JAVA_PLAY_CONTROL_PROFILE", profile.as_str());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map(SupervisedChildV1::adopt)
        .map_err(|error| format!("failed to launch Java parity runtime: {error}"))
}

fn wait_for_owned_session(
    resources: &mut SupervisedResourcesV1,
    live_output: &Path,
    launch_plan: &Path,
    communication_error_log: &Path,
) -> Result<LiveStatus, String> {
    let started = Instant::now();
    while started.elapsed() < STARTUP_TIMEOUT {
        resources
            .validate_primary_running()
            .map_err(|error| format!("Java runtime exited before publishing a session: {error}"))?;
        if communication_error_log.is_file() {
            let detail = fs::read_to_string(communication_error_log).map_err(|error| {
                format!("failed to read CommunicationMod startup error: {error}")
            })?;
            if !detail.trim().is_empty() {
                return Err(format!(
                    "CommunicationMod rejected the launch: {}",
                    detail.trim()
                ));
            }
        }
        if let Some(detail) = ended_live_session_detail(live_output)? {
            return Err(format!(
                "Java parity bridge ended before publishing a stable session: {detail}"
            ));
        }
        if let Ok(live) = sts_java_parity_live::status(live_output) {
            if live.manifest.launch.as_ref().is_some_and(|launch| {
                same_existing_path(Path::new(&launch.plan_path), launch_plan).unwrap_or(false)
            }) {
                return Ok(live);
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(
        "Java parity runtime did not publish a launch-bound stable session within 60 seconds"
            .to_owned(),
    )
}

fn ended_live_session_detail(live_output: &Path) -> Result<Option<String>, String> {
    if !live_output.is_dir() {
        return Ok(None);
    }
    let mut ended = Vec::new();
    for entry in fs::read_dir(live_output)
        .map_err(|error| format!("failed to inspect Java parity bridge sessions: {error}"))?
    {
        let session = entry
            .map_err(|error| format!("failed to enumerate Java parity bridge sessions: {error}"))?
            .path();
        let path = session.join("ended.json");
        if !path.is_file() {
            continue;
        }
        let raw = fs::read(&path)
            .map_err(|error| format!("failed to read ended Java parity bridge session: {error}"))?;
        let value: serde_json::Value = serde_json::from_slice(&raw).map_err(|error| {
            format!("failed to decode ended Java parity bridge session: {error}")
        })?;
        ended.push((
            value["ended_at_unix_ms"].as_u64().unwrap_or_default(),
            value,
        ));
    }
    ended.sort_by_key(|(ended_at, _)| *ended_at);
    let Some((_, value)) = ended.pop() else {
        return Ok(None);
    };
    let status = value["status"].as_str().unwrap_or("unknown");
    let error = value["error"]
        .as_str()
        .filter(|message| !message.is_empty())
        .unwrap_or("the producer connection closed before its first stable frame");
    Ok(Some(format!("status={status}: {error}")))
}

fn admit_live_bridge_process(
    executable: &Path,
    live_output: &Path,
    observed: &LiveStatus,
) -> Result<SupervisedProcessV1, String> {
    let candidate =
        ExistingProcessCandidateV1::open_running(observed.manifest.process_id, executable)
            .map_err(|error| format!("failed to open exact Java bridge process: {error}"))?;
    let confirmed = sts_java_parity_live::status(live_output)?;
    if confirmed.manifest.process_id != observed.manifest.process_id
        || confirmed.manifest.session_id != observed.manifest.session_id
        || confirmed.manifest.capture_id != observed.manifest.capture_id
        || confirmed.manifest.producer_session_id != observed.manifest.producer_session_id
        || confirmed.manifest.connection_id != observed.manifest.connection_id
        || !candidate
            .is_running()
            .map_err(|error| format!("failed to confirm exact Java bridge process: {error}"))?
    {
        return Err("Java bridge identity changed during exact-handle admission".to_owned());
    }
    Ok(candidate.admit())
}

fn validate_initial_boundary(
    live: &LiveStatus,
    expected: JavaParityInitialBoundaryV1,
) -> Result<(), String> {
    let decision = &live.frame["public_base"]["decision"];
    let common = live.frame["boundary"]["kind"].as_str() == Some("main_menu_ready")
        && decision["kind"].as_str() == Some("main_menu_ready")
        && live.actions.len() == 1;
    let exact = match expected {
        JavaParityInitialBoundaryV1::FreshStart => {
            common
                && live.frame["public_base"]["run"].is_null()
                && live.actions[0].kind == "start_run"
                && live.actions[0].action_id == "start_run"
                && live.actions[0].parameters_required
        }
        JavaParityInitialBoundaryV1::NeutralContinue => {
            common
                && live.actions[0].kind == "continue_run"
                && live.actions[0].action_id == "continue_run"
        }
    };
    if exact {
        Ok(())
    } else {
        Err(format!(
            "Java parity runtime reached an unexpected {expected:?} boundary"
        ))
    }
}

fn runtime_session_binding(live: &LiveStatus) -> Result<RuntimeSessionBindingV1, String> {
    let protocol = live.frame["protocol"]["minor"]
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| "Java parity frame omitted its protocol minor".to_owned())?;
    RuntimeSessionBindingV1::new(
        format!(
            "{}:{}:{}",
            live.manifest.producer_session_id,
            live.manifest.connection_id,
            live.manifest.capture_id
        ),
        &live.manifest.session_id,
        &live.manifest.connection_id,
        live.manifest.process_id,
        &live.manifest.sts_jar_sha256,
        &live.manifest.loaded_mod_jar_sha256,
        protocol,
    )
    .map_err(|error| error.to_string())
}

fn resolve_base_mod(game_directory: &Path) -> Result<PathBuf, String> {
    let local = game_directory.join("mods").join("BaseMod.jar");
    if local.is_file() {
        return Ok(local);
    }
    let mut matches = Vec::new();
    for entry in fs::read_dir(WORKSHOP_DIRECTORY)
        .map_err(|error| format!("failed to inspect Slay the Spire Workshop: {error}"))?
    {
        let candidate = entry
            .map_err(|error| format!("failed to enumerate Workshop entries: {error}"))?
            .path()
            .join("BaseMod.jar");
        if candidate.is_file() {
            matches.push(candidate);
        }
    }
    if matches.len() != 1 {
        return Err(format!(
            "expected one installed BaseMod.jar, found {}",
            matches.len()
        ));
    }
    Ok(matches.remove(0))
}

fn require_jar_entry(path: &Path, entry_name: &str, missing: &str) -> Result<(), String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open Mod JAR {}: {error}", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("failed to read Mod JAR {}: {error}", path.display()))?;
    archive
        .by_name(entry_name)
        .map(|_| ())
        .map_err(|_| missing.to_owned())
}

fn validate_packaged_protocol_descriptor(path: &Path) -> Result<(), String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open CommunicationMod JAR: {error}"))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("failed to read CommunicationMod JAR: {error}"))?;
    let mut entry = archive
        .by_name(PROTOCOL_DESCRIPTOR_ENTRY)
        .map_err(|_| "installed CommunicationMod has no parity protocol descriptor".to_owned())?;
    let mut encoded = String::new();
    entry
        .read_to_string(&mut encoded)
        .map_err(|error| format!("failed to read parity protocol descriptor: {error}"))?;
    let descriptor: PackagedProtocolDescriptorV1 = serde_json::from_str(&encoded)
        .map_err(|error| format!("invalid parity protocol descriptor: {error}"))?;
    if descriptor.schema != "communicationmod-parity-protocol-v1"
        || descriptor.name != "communicationmod.parity"
        || descriptor.major != sts_java_parity_protocol::PROTOCOL_MAJOR
        || descriptor.minor != sts_java_parity_protocol::PROTOCOL_MINOR
        || descriptor.frame_schema != "communicationmod.parity-frame.v1"
    {
        return Err("installed CommunicationMod parity protocol does not match Rust".to_owned());
    }
    Ok(())
}

fn mod_identity(path: &Path, expected_id: &str) -> Result<ModIdentityV1, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open Mod JAR {}: {error}", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("failed to read Mod JAR {}: {error}", path.display()))?;
    let mut entry = archive
        .by_name("ModTheSpire.json")
        .map_err(|error| format!("Mod JAR {} has no metadata: {error}", path.display()))?;
    let mut encoded = String::new();
    entry
        .read_to_string(&mut encoded)
        .map_err(|error| format!("failed to read Mod metadata {}: {error}", path.display()))?;
    let metadata: ModMetadataV1 = serde_json::from_str(&encoded)
        .map_err(|error| format!("failed to decode Mod metadata {}: {error}", path.display()))?;
    if metadata.modid != expected_id || metadata.version.trim().is_empty() {
        return Err(format!(
            "Mod JAR {} identifies as {:?} instead of {:?}",
            path.display(),
            metadata.modid,
            expected_id
        ));
    }
    Ok(ModIdentityV1 {
        id: metadata.modid,
        version: metadata.version,
        jar: file_identity(path)?,
    })
}

fn file_identity(path: &Path) -> Result<FileIdentityV1, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("required file is not regular: {}", path.display()));
    }
    Ok(FileIdentityV1 {
        path: absolute_path(path)?.to_string_lossy().into_owned(),
        length: metadata.len(),
        sha256: file_sha256_upper(path)?,
    })
}

fn file_sha256_upper(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|error| format!("failed to hash {}: {error}", path.display()))?;
    Ok(format!("{:X}", hasher.finalize()))
}

fn acceleration_config() -> Result<PathBuf, String> {
    let local = env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "LOCALAPPDATA is unavailable for Mod configuration".to_owned())?;
    Ok(PathBuf::from(local)
        .join("ModTheSpire")
        .join("SuperFastMode")
        .join("SuperFastModeConfig.properties"))
}

fn acceleration_multiplier(properties: &BTreeMap<String, String>) -> Result<f64, String> {
    properties
        .get("deltaMultiplier")
        .and_then(|value| value.parse::<f64>().ok())
        .ok_or_else(|| "SuperFastMode deltaMultiplier is invalid".to_owned())
}

fn read_properties(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let encoded = fs::read_to_string(path)
        .map_err(|error| format!("failed to read properties {}: {error}", path.display()))?;
    Ok(encoded
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(['#', '!']))
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect())
}

fn copy_directory_contents(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source).map_err(|error| {
        format!(
            "failed to read seed directory {}: {error}",
            source.display()
        )
    })? {
        let entry =
            entry.map_err(|error| format!("failed to enumerate seed directory: {error}"))?;
        let kind = entry
            .file_type()
            .map_err(|error| format!("failed to inspect seed entry: {error}"))?;
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            fs::create_dir(&target)
                .map_err(|error| format!("failed to create seed subdirectory: {error}"))?;
            copy_directory_contents(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)
                .map_err(|error| format!("failed to copy seed file: {error}"))?;
        } else {
            return Err(format!(
                "seed storage contains unsupported link {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn create_junction(target: &Path, link: &Path) -> Result<(), String> {
    let output = Command::new("cmd.exe")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("/D")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .output()
        .map_err(|error| format!("failed to execute junction creation: {error}"))?;
    if !output.status.success() || !link.is_dir() {
        return Err(format!(
            "failed to create managed junction: {}",
            output.status
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn create_junction(_target: &Path, _link: &Path) -> Result<(), String> {
    Err("Java parity runtime currently requires Windows junctions".to_owned())
}

#[derive(Debug, Eq, PartialEq)]
struct Java8ProcessBoundaryV1 {
    mod_the_spire_argument: String,
    working_directory: String,
    launch_plan_environment: String,
}

impl Java8ProcessBoundaryV1 {
    fn new(mod_the_spire: &Path, work: &Path, plan: &Path) -> Result<Self, String> {
        Ok(Self {
            mod_the_spire_argument: java_8_compatible_path(mod_the_spire)?,
            working_directory: java_8_compatible_path(work)?,
            launch_plan_environment: java_8_compatible_path(plan)?,
        })
    }
}

fn java_8_compatible_path(path: &Path) -> Result<String, String> {
    let absolute = absolute_path(path)?;
    let rendered = absolute.to_string_lossy();
    Ok(if let Some(tail) = rendered.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{tail}")
    } else if let Some(tail) = rendered.strip_prefix(r"\\?\") {
        tail.to_owned()
    } else {
        rendered.into_owned()
    })
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| format!("failed to resolve current directory: {error}"))
    }
}

fn same_existing_path(left: &Path, right: &Path) -> Result<bool, String> {
    let left = fs::canonicalize(left).map_err(|error| error.to_string())?;
    let right = fs::canonicalize(right).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    return Ok(left
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .eq_ignore_ascii_case(right.to_string_lossy().trim_start_matches(r"\\?\")));
    #[cfg(not(windows))]
    Ok(left == right)
}

fn live_bridge_executable(repository: &Path) -> PathBuf {
    repository
        .join("target")
        .join("debug")
        .join(if cfg!(windows) {
            "sts_java_parity_live.exe"
        } else {
            "sts_java_parity_live"
        })
}

fn write_json_new(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    write_bytes_new(path, &encoded)
}

fn write_bytes_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_data())
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ended_bridge_session_is_reported_without_waiting_for_startup_timeout() {
        let root = std::env::temp_dir().join(format!(
            "sts-java-parity-ended-session-{}",
            uuid::Uuid::new_v4()
        ));
        let session = root.join("session-1");
        fs::create_dir_all(&session).unwrap();
        fs::write(
            session.join("ended.json"),
            serde_json::to_vec(&serde_json::json!({
                "status": "failed",
                "ended_at_unix_ms": 7,
                "error": "producer rejected a frame"
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            ended_live_session_detail(&root).unwrap().as_deref(),
            Some("status=failed: producer rejected a frame")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn seed_and_boundary_must_describe_the_same_world() {
        let native = Path::new("native");
        assert!(storage_seed_matches_boundary(
            JavaParityStorageSeedV1::Fresh,
            JavaParityInitialBoundaryV1::FreshStart,
        ));
        assert!(storage_seed_matches_boundary(
            JavaParityStorageSeedV1::Native(native),
            JavaParityInitialBoundaryV1::NeutralContinue,
        ));
        assert!(!storage_seed_matches_boundary(
            JavaParityStorageSeedV1::Fresh,
            JavaParityInitialBoundaryV1::NeutralContinue,
        ));
        assert!(!storage_seed_matches_boundary(
            JavaParityStorageSeedV1::Native(native),
            JavaParityInitialBoundaryV1::FreshStart,
        ));
    }

    #[test]
    fn canonical_a20h_lab_profile_has_a_stable_default_slot_and_rich_neow_eligibility() {
        let slots: BTreeMap<String, String> =
            serde_json::from_slice(CANONICAL_STS_SAVE_SLOTS).unwrap();
        let player: BTreeMap<String, String> =
            serde_json::from_slice(CANONICAL_STS_PLAYER).unwrap();

        assert_eq!(slots.get("DEFAULT_SLOT").map(String::as_str), Some("0"));
        assert_eq!(
            slots.get("PROFILE_NAME").map(String::as_str),
            player.get("name").map(String::as_str)
        );
        assert_eq!(
            player.get("IRONCLAD_SPIRITS").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            player.get("ftuePopupShown").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn canonical_a20h_lab_profile_precompletes_original_tutorial_flags() {
        let tips: BTreeMap<String, String> = serde_json::from_slice(CANONICAL_STS_TIPS).unwrap();
        for key in [
            "NEOW_SKIP",
            "NEOW_INTRO",
            "NO_FTUE",
            "COMBAT_TIP",
            "BLOCK TIP",
            "POWER_TIP",
            "M_POWER_TIP",
            "ENERGY_USE_TIP",
            "SHUFFLE_TIP",
            "POTION_TIP",
            "CARD_REWARD_TIP",
            "INTENT_TIP",
            "RELIC_TIP",
        ] {
            assert_eq!(
                tips.get(key).map(String::as_str),
                Some("true"),
                "missing {key}"
            );
        }
    }

    #[test]
    fn fresh_lab_configuration_enables_fast_mode_in_both_preference_roots() {
        let root = std::env::temp_dir().join(format!("sts-fresh-speed-{}", Uuid::new_v4()));
        for directory in ["preferences", "betaPreferences"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        configure_lab_preferences(&root, false).unwrap();
        for directory in ["preferences", "betaPreferences"] {
            assert_eq!(
                read_preferences(&root.join(directory).join("STSGameplaySettings")).unwrap()
                    ["Fast Mode"],
                "true"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_lab_configuration_suppresses_tutorials_without_changing_saves_or_progress() {
        let root = std::env::temp_dir().join(format!("sts-native-lab-{}", Uuid::new_v4()));
        for directory in ["preferences", "betaPreferences", "saves"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        let prefs = root.join("preferences");
        let save = root.join("saves/IRONCLAD.autosave");
        fs::write(&save, b"opaque native save bytes").unwrap();
        fs::write(
            prefs.join("STSSaveSlots"),
            br#"{"DEFAULT_SLOT":"-1","PROFILE_NAME":"","PLAYTIME":"123"}"#,
        )
        .unwrap();
        fs::write(
            prefs.join("STSPlayer"),
            br#"{"ftuePopupShown":"false","IRONCLAD_SPIRITS":"7","alias":"original"}"#,
        )
        .unwrap();
        fs::write(
            prefs.join("STSTips"),
            br#"{"SHUFFLE_TIP":"false","custom_tip":"keep"}"#,
        )
        .unwrap();
        fs::write(
            prefs.join("STSGameplaySettings"),
            br#"{"Fast Mode":"false","LANGUAGE":"ZHS","Screen Shake":"false"}"#,
        )
        .unwrap();
        configure_lab_preferences(&root, true).unwrap();
        let slots = read_preferences(&prefs.join("STSSaveSlots")).unwrap();
        assert_eq!(slots["DEFAULT_SLOT"], "0");
        assert!(!slots["PROFILE_NAME"].is_empty());
        assert_eq!(slots["PLAYTIME"], "123");
        let player = read_preferences(&prefs.join("STSPlayer")).unwrap();
        assert_eq!(player["ftuePopupShown"], "true");
        assert_eq!(player["IRONCLAD_SPIRITS"], "7");
        assert_eq!(player["alias"], "original");
        let tips = read_preferences(&prefs.join("STSTips")).unwrap();
        let canonical: BTreeMap<String, String> =
            serde_json::from_slice(CANONICAL_STS_TIPS).unwrap();
        for (key, value) in canonical {
            assert_eq!(tips[&key], value);
        }
        assert_eq!(tips["custom_tip"], "keep");
        let gameplay = read_preferences(&prefs.join("STSGameplaySettings")).unwrap();
        assert_eq!(gameplay["Fast Mode"], "true");
        assert_eq!(gameplay["LANGUAGE"], "ZHS");
        assert_eq!(gameplay["Screen Shake"], "false");
        assert_eq!(fs::read(&save).unwrap(), b"opaque native save bytes");
        // Repeated preparation is harmless and does not consume or create game state.
        configure_lab_preferences(&root, true).unwrap();
        assert_eq!(fs::read(&save).unwrap(), b"opaque native save bytes");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_lab_configuration_uses_the_saved_slot_without_renaming_its_save() {
        let root = std::env::temp_dir().join(format!("sts-native-slot-{}", Uuid::new_v4()));
        for directory in ["preferences", "betaPreferences", "saves"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(root.join("saves/2_IRONCLAD.autosave"), b"slot two").unwrap();
        configure_lab_preferences(&root, true).unwrap();
        let prefs = root.join("preferences");
        assert_eq!(
            read_preferences(&prefs.join("STSSaveSlots")).unwrap()["DEFAULT_SLOT"],
            "2"
        );
        assert_eq!(
            read_preferences(&prefs.join("2_STSPlayer")).unwrap()["ftuePopupShown"],
            "true"
        );
        assert!(!read_preferences(&prefs.join("2_STSPlayer"))
            .unwrap()
            .contains_key("IRONCLAD_SPIRITS"));
        assert_eq!(
            read_preferences(&prefs.join("2_STSTips")).unwrap()["SHUFFLE_TIP"],
            "true"
        );
        assert_eq!(
            read_preferences(&prefs.join("2_STSGameplaySettings")).unwrap()["Fast Mode"],
            "true"
        );
        assert!(!prefs.join("STSGameplaySettings").exists());
        assert!(!root.join("saves/IRONCLAD.autosave").exists());
        assert_eq!(
            fs::read(root.join("saves/2_IRONCLAD.autosave")).unwrap(),
            b"slot two"
        );
        fs::write(root.join("saves/1_IRONCLAD.autosave"), b"slot one").unwrap();
        assert_eq!(
            native_profile_slot(
                &root,
                &read_preferences(&prefs.join("STSSaveSlots")).unwrap(),
                false
            )
            .unwrap(),
            2
        );
        assert!(native_profile_slot(&root, &BTreeMap::new(), false)
            .unwrap_err()
            .contains("multiple save slots"));
        fs::remove_dir_all(root).unwrap();
    }
}
