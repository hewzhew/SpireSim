use crate::{
    runtime_ownership::same_path, subprocess::run_bounded, ResidentBindingV1, RootBindingV1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sts_java_parity_live::{self, ActionReceipt, LiveStatus};
use sts_java_parity_runtime::JavaParityRuntimeOwnerV1;
use sts_workbench_supervisor::{
    ExecutableImageStoreV1, ExistingProcessCandidateV1, JavaRuntimeIdentityV1,
    OwnedStorageRegistryV1, OwnedStorageV1, RuntimeSessionBindingV1, SupervisedChildV1,
    SupervisedProcessSetV1, SupervisedProcessV1, SupervisedResourcesV1, SupervisedRuntimeV1,
};
use uuid::Uuid;
use zip::ZipArchive;

const WORKSHOP_DIRECTORY: &str = r"C:\Program Files (x86)\Steam\steamapps\workshop\content\646570";
const RUN_CONTROL_PROFILE: &str = "parity_run_control";
const SCENARIO_CONTROL_PROFILE: &str = "parity_scenario_control";
const LAUNCH_PROFILE: &str = "accelerated";
const CANONICAL_FRESH_PROFILE_SUBSTRATE: &str = "canonical_tutorial_complete_preferences_v1";
const CANONICAL_TUTORIAL_POLICY: &str = "precompleted_via_profile_v1";
const OBSERVED_TUTORIAL_POLICY: &str = "observe_and_acknowledge_v1";
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
const CAPTURED_NATIVE_PROFILE_SUBSTRATE: &str = "captured_native_profile_v1";
const MATERIALIZATION_SCHEMA: &str = "sts-paired-root-native-materialization-report-v1";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const NEOW_EVENT_ID: &str = "Neow";
const MANAGED_JAVA_STDOUT_LOG: &str = "managed-java-stdout.log";
const MANAGED_JAVA_STDERR_LOG: &str = "managed-java-stderr.log";
const RUN_PLAY_ADAPTER_FAILURE_SCHEMA_V1: &str = "sts-run-play-adapter-failure-v1";
const RUN_PLAY_ADAPTER_FAILED_CODE: &str = "run_play_adapter_failed";
const RUN_PLAY_ADAPTER_PROTOCOL_INVALID_CODE: &str = "run_play_adapter_protocol_invalid";
const PAIRED_ROOT_VERSION_DRIFT_CODE: &str = "paired_root_version_drift";

fn neow_intro_screen(live: &LiveStatus) -> Option<i64> {
    let decision = &live.frame["public_base"]["decision"];
    let screen = decision["event_screen_index"].as_i64();
    (live.frame["boundary"]["kind"].as_str() == Some("run_decision")
        && decision["kind"].as_str() == Some("event")
        && decision["event_id"].as_str() == Some(NEOW_EVENT_ID)
        && matches!(screen, Some(0) | Some(1)))
    .then_some(screen.expect("accepted Neow intro screen is present"))
}

fn neow_intro_action_id(screen: i64) -> String {
    debug_assert!(matches!(screen, 0 | 1));
    format!("event_option:{NEOW_EVENT_ID}:index:{screen}:0")
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeMaterializationV1 {
    pub schema: String,
    pub requested_target_root_id: String,
    pub native_root_id: String,
    #[serde(default)]
    pub recovery_certificate_id: Option<String>,
    pub java_bundle_id: String,
    pub content_profile: String,
    pub java_entrypoint: String,
    pub materialized_directory: String,
    pub storage_directory: String,
    pub file_count: usize,
    #[serde(rename = "recovery_steps")]
    pub _recovery_steps: usize,
}

#[derive(Debug)]
pub(super) struct RuntimeOpenV1 {
    pub runtime: ManagedJavaRuntimeV1,
    pub runtime_ready_ms: u128,
}

#[derive(Debug)]
pub(super) struct NaturalNeowBootstrapOpenV1 {
    pub runtime: ManagedJavaRuntimeV1,
    pub root: RootBindingV1,
    pub seed: u64,
    pub ascension: u8,
    pub neow_offer: String,
    pub exact_components: usize,
    pub runtime_ready_ms: u128,
    pub adapter_ms: u128,
    pub total_ms: u128,
}

#[derive(Debug)]
pub(super) struct ConstructedCombatBootstrapOpenV1 {
    pub runtime: ManagedJavaRuntimeV1,
    pub root: RootBindingV1,
    pub case_id: String,
    pub scenario_id: String,
    pub exact_components: usize,
    pub runtime_ready_ms: u128,
    pub scenario_ms: u128,
    pub adapter_ms: u128,
    pub total_ms: u128,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParentlessNaturalNeowAdapterReportV1 {
    schema: String,
    status: String,
    seed: String,
    ascension: u8,
    neow_offer: String,
    source_session: String,
    source_archive_node_id: String,
    java_frame_path: String,
    components: Vec<BootstrapProjectionSummaryV1>,
    source_head_guard: BootstrapSourceHeadGuardV1,
    root_id: String,
    activation_id: String,
    action_space_id: String,
    java_runtime_bundle: sts_paired_root::BundleRefV1,
    java_runtime_entrypoint: String,
    recovery_certificate: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstructedCombatAdapterReportV1 {
    schema: String,
    status: String,
    case_id: String,
    scenario_id: String,
    recipe_program_id: String,
    source_session: String,
    source_archive_node_id: String,
    scenario_plan_path: String,
    scenario_report_path: String,
    java_frame_path: String,
    components: Vec<BootstrapProjectionSummaryV1>,
    gaps: Vec<Value>,
    source_head_guard: BootstrapSourceHeadGuardV1,
    root_id: Option<String>,
    activation_id: Option<String>,
    action_space_id: Option<String>,
    java_runtime_bundle: Option<sts_paired_root::BundleRefV1>,
    java_runtime_entrypoint: Option<String>,
    java_cold_recovery: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapProjectionSummaryV1 {
    domain: String,
    schema: String,
    status: String,
    #[serde(default)]
    first_difference: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapSourceHeadGuardV1 {
    before_sha256: String,
    after_sha256: String,
    file_count: usize,
    unchanged: bool,
}

#[derive(Debug)]
pub(super) struct ManagedJavaRuntimeV1 {
    physical: JavaPhysicalRuntimeV1,
    engine: JavaEngineRuntimeV1,
}

/// Supervisor-owned resources and identities. This half has shutdown
/// authority but no PairedRoot recovery or source-session semantics.
#[derive(Debug)]
pub(super) struct JavaPhysicalRuntimeV1 {
    repository: PathBuf,
    owner: JavaParityRuntimeOwnerV1,
    build_sha256: String,
    bridge_build_sha256: String,
}

/// Replaceable Engine context for interpreting one physical Java runtime.
/// It owns no process, bridge handle, sandbox, frame, or autosave authority.
#[derive(Debug)]
pub(super) struct JavaEngineRuntimeV1 {
    resident_id: String,
    repository: PathBuf,
    materialization: NativeMaterializationV1,
    native_root_id: String,
    source_session: Option<PathBuf>,
}

/// One operation's borrowed view across the separately owned Java halves.
/// Dropping this view never closes the physical runtime or retains Engine
/// context outside its owner.
pub(super) struct JavaRuntimeAccessV1<'a> {
    physical: &'a mut JavaPhysicalRuntimeV1,
    engine: &'a mut JavaEngineRuntimeV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ResetFrameTrustV1 {
    Accepted,
    Dirty,
}

#[derive(Debug)]
pub(super) struct BootstrapJavaRuntimeV1 {
    repository: PathBuf,
    supervised: Option<SupervisedRuntimeV1>,
    launch_plan: PathBuf,
    live_output: PathBuf,
    build_sha256: String,
    bridge_build_sha256: String,
}

#[derive(Debug)]
struct ManagedJavaLaunchScopeV1 {
    launch_plan: PathBuf,
    live_output: PathBuf,
    bridge_executable: PathBuf,
    bridge_build_sha256: String,
}

impl BootstrapJavaRuntimeV1 {
    fn open_fresh(
        repository: &Path,
        storage_registry: &OwnedStorageRegistryV1,
    ) -> Result<(Self, u128), String> {
        Self::open_seeded(
            repository,
            None,
            storage_registry,
            false,
            RUN_CONTROL_PROFILE,
        )
    }

    fn open_fresh_scenario(
        repository: &Path,
        storage_registry: &OwnedStorageRegistryV1,
    ) -> Result<(Self, u128), String> {
        Self::open_seeded(
            repository,
            None,
            storage_registry,
            false,
            SCENARIO_CONTROL_PROFILE,
        )
    }

    pub(super) fn open_recovery(
        repository: &Path,
        recovery_storage: &Path,
        storage_registry: &OwnedStorageRegistryV1,
    ) -> Result<(Self, u128), String> {
        Self::open_seeded(
            repository,
            Some(recovery_storage),
            storage_registry,
            true,
            RUN_CONTROL_PROFILE,
        )
    }

    fn open_seeded(
        repository: &Path,
        recovery_storage: Option<&Path>,
        storage_registry: &OwnedStorageRegistryV1,
        expect_continue: bool,
        control_profile: &str,
    ) -> Result<(Self, u128), String> {
        let started = Instant::now();
        let started_at_unix_ms = now_unix_ms();
        let build_sha256 = file_sha256(&run_play_executable(repository))?;
        let game_directory = storage_registry.source_game_directory().to_path_buf();
        validate_runtime_configuration(repository, &game_directory)?;
        let storage = match recovery_storage {
            Some(seed) => IsolatedStorageV1::create(seed, storage_registry)?,
            None => IsolatedStorageV1::create_fresh(storage_registry)?,
        };
        let launch_scope = create_managed_java_launch_scope(repository)?;
        let launch_plan = create_launch_plan(
            repository,
            &game_directory,
            &storage,
            control_profile,
            &launch_scope,
        )?;
        let live_output = launch_scope.live_output.clone();
        let bridge_executable = launch_scope.bridge_executable.clone();
        let bridge_build_sha256 = launch_scope.bridge_build_sha256.clone();
        let storage_root = storage.root().to_path_buf();
        let isolation_id = storage.isolation_id();
        let game = launch_game(
            repository,
            &game_directory,
            &storage_root,
            &launch_plan,
            control_profile,
        )?;
        let processes = SupervisedProcessSetV1::new(game);
        let mut resources = SupervisedResourcesV1::new(processes, storage.into_owned());
        let live = match wait_for_owned_session(
            &mut resources,
            &live_output,
            &launch_plan,
            &storage_root.join("communication_mod_errors.log"),
        ) {
            Ok(live) => live,
            Err(error) => {
                let diagnostics = snapshot_startup_diagnostics(
                    repository,
                    &storage_root,
                    isolation_id,
                    &launch_plan,
                    &live_output,
                    started_at_unix_ms,
                    "stable_session_timeout",
                );
                let _ = resources.shutdown(
                    Duration::from_secs(10),
                    Duration::from_secs(5),
                    Duration::from_secs(5),
                );
                return Err(with_startup_diagnostics(error, diagnostics));
            }
        };
        let bridge = admit_live_bridge_process(&bridge_executable, &live_output, &live)?;
        resources
            .admit_companion(bridge)
            .map_err(|error| format!("failed to admit Java bridge process: {error}"))?;
        let mut supervised =
            SupervisedRuntimeV1::new(resources, runtime_session_binding(&live)?)
                .map_err(|error| format!("failed to bind Java runtime session: {error}"))?;
        let boundary = if expect_continue {
            validate_neutral_continue_boundary(&live)
        } else {
            validate_fresh_start_boundary(&live)
        };
        if let Err(error) = boundary {
            let diagnostics = snapshot_startup_diagnostics(
                repository,
                &storage_root,
                isolation_id,
                &launch_plan,
                &live_output,
                started_at_unix_ms,
                "unexpected_main_menu_boundary",
            );
            let _ = supervised.shutdown(
                Duration::from_secs(10),
                Duration::from_secs(5),
                Duration::from_secs(5),
            );
            return Err(with_startup_diagnostics(error, diagnostics));
        }
        Ok((
            Self {
                repository: repository.to_path_buf(),
                supervised: Some(supervised),
                launch_plan,
                live_output,
                build_sha256,
                bridge_build_sha256,
            },
            started.elapsed().as_millis(),
        ))
    }

    fn start_run_to_neow_intro(&mut self, seed: u64, ascension: u8) -> Result<(), String> {
        self.validate_epoch()?;
        let seed_token = sts_java_parity_live::java_seed_token_for_engine_seed(&seed.to_string())?;
        sts_java_parity_live::start_run_with_content_profile(
            &self.live_output,
            "IRONCLAD",
            u32::from(ascension),
            Some(&seed_token),
            "all_unlocked",
            Duration::from_secs(30),
        )?;
        self.validate_epoch()?;
        self.advance_ftue_to_neow_intro()
    }

    pub(super) fn continue_from_native_storage(&mut self) -> Result<LiveStatus, String> {
        self.validate_epoch()?;
        sts_java_parity_live::continue_run_with_content_profile(
            &self.live_output,
            "all_unlocked",
            Duration::from_secs(30),
        )?;
        self.validate_epoch()
    }

    fn advance_ftue_to_neow_intro(&mut self) -> Result<(), String> {
        for _ in 0..16 {
            let live = self.validate_epoch()?;
            let decision = &live.frame["public_base"]["decision"];
            if neow_intro_screen(&live).is_some() {
                return Ok(());
            }
            if decision["kind"].as_str() != Some("ftue") {
                return Err(format!(
                    "fresh natural run reached {:?} instead of FTUE or Neow intro",
                    decision["kind"].as_str()
                ));
            }
            let mut acknowledgements = live
                .actions
                .iter()
                .filter(|action| action.kind == "acknowledge_ftue");
            let action = acknowledgements
                .next()
                .ok_or_else(|| "FTUE surface omitted its typed acknowledgement".to_owned())?;
            if acknowledgements.next().is_some() || action.parameters_required {
                return Err("FTUE acknowledgement is not uniquely typed".to_owned());
            }
            sts_java_parity_live::act(&self.live_output, &action.handle, Duration::from_secs(30))?;
        }
        Err("fresh natural run exceeded the bounded FTUE sequence".to_owned())
    }

    fn submit_neow_intro(&mut self) -> Result<(ActionReceipt, LiveStatus), String> {
        let live = self.validate_epoch()?;
        let intro_screen = neow_intro_screen(&live)
            .ok_or_else(|| "runtime is not at an original-game Neow intro screen".to_owned())?;
        let expected_action_id = neow_intro_action_id(intro_screen);
        let mut intros = live.actions.iter().filter(|action| {
            action.kind == "choose_event_option"
                && action.action_id == expected_action_id
                && action.event_id.as_deref() == Some(NEOW_EVENT_ID)
                && action.option_index == Some(0)
                && action.neow_reward_type.is_none()
                && action.neow_drawback.is_none()
        });
        let action = intros
            .next()
            .ok_or_else(|| "Neow intro omitted its typed Talk action".to_owned())?;
        if intros.next().is_some() || action.parameters_required {
            return Err("Neow intro Talk action is not uniquely typed".to_owned());
        }
        let receipt =
            sts_java_parity_live::act(&self.live_output, &action.handle, Duration::from_secs(30))?;
        let successor = self.validate_epoch()?;
        let decision = &successor.frame["public_base"]["decision"];
        if successor.frame["boundary"]["kind"].as_str() != Some("run_decision")
            || decision["kind"].as_str() != Some("event")
            || decision["event_id"].as_str() != Some(NEOW_EVENT_ID)
            || decision["event_screen_index"].as_i64() != Some(3)
        {
            return Err("typed Neow Talk did not reach the reward-choice screen".to_owned());
        }
        Ok((receipt, successor))
    }

    fn snapshot_recovery_storage(&self, destination: &Path) -> Result<PathBuf, String> {
        snapshot_profile_storage(
            self.supervised
                .as_ref()
                .expect("bootstrap runtime retains resources")
                .storage_root(),
            destination,
        )?;
        let autosave = destination.join("saves");
        single_ironclad_autosave_in(&autosave)
    }

    fn runtime_epoch(&self) -> &str {
        self.supervised
            .as_ref()
            .expect("bootstrap runtime retains resources")
            .session()
            .runtime_epoch()
    }

    fn validate_epoch(&mut self) -> Result<LiveStatus, String> {
        let supervised = self
            .supervised
            .as_mut()
            .ok_or_else(|| "bootstrap Java runtime is already closed".to_owned())?;
        let live = sts_java_parity_live::status(&self.live_output)?;
        supervised
            .validate_running(&runtime_session_binding(&live)?)
            .map_err(|error| format!("bootstrap Java runtime is unavailable: {error}"))?;
        let launch = live
            .manifest
            .launch
            .as_ref()
            .ok_or_else(|| "bootstrap Java session omitted launch evidence".to_owned())?;
        if !same_path(Path::new(&launch.plan_path), &self.launch_plan)
            || file_sha256(&run_play_executable(&self.repository))? != self.build_sha256
            || file_sha256(&live_bridge_executable(&self.repository))? != self.bridge_build_sha256
        {
            return Err("bootstrap Java runtime identity drifted".to_owned());
        }
        Ok(live)
    }

    fn into_managed(
        mut self,
        materialization: NativeMaterializationV1,
        source_session: PathBuf,
        native_autosave_sha256: String,
    ) -> Result<ManagedJavaRuntimeV1, String> {
        let autosave_path = self
            .supervised
            .as_ref()
            .expect("bootstrap runtime retains resources")
            .storage_root()
            .join("saves");
        let autosave_path = single_ironclad_autosave_in(&autosave_path)?;
        let autosave_sha256 = file_sha256(&autosave_path)?;
        if !autosave_sha256.eq_ignore_ascii_case(&native_autosave_sha256) {
            return Err("restored bootstrap runtime changed its native autosave".to_owned());
        }
        let live = sts_java_parity_live::status(&self.live_output)?;
        let session = runtime_session_binding(&live)?;
        let frame_token = runtime_frame_token(&live)?;
        let supervised = self
            .supervised
            .as_mut()
            .expect("bootstrap runtime retains resources");
        supervised
            .validate_running(&session)
            .map_err(|error| format!("managed Java runtime is unavailable: {error}"))?;
        supervised
            .bind_autosave(&autosave_path, &native_autosave_sha256)
            .map_err(|error| format!("failed to bind managed autosave: {error}"))?;
        supervised
            .bind_frame(frame_token)
            .map_err(|error| format!("failed to bind managed frame: {error}"))?;
        Ok(ManagedJavaRuntimeV1 {
            physical: JavaPhysicalRuntimeV1 {
                repository: self.repository.clone(),
                owner: JavaParityRuntimeOwnerV1::new(
                    self.supervised
                        .take()
                        .expect("bootstrap runtime retains resources"),
                    self.launch_plan.clone(),
                    self.live_output.clone(),
                )
                .map_err(|error| format!("failed to own Java parity runtime: {error}"))?,
                build_sha256: self.build_sha256.clone(),
                bridge_build_sha256: self.bridge_build_sha256.clone(),
            },
            engine: JavaEngineRuntimeV1 {
                resident_id: format!("workbench-resident-v1-{}", Uuid::new_v4().simple()),
                repository: self.repository.clone(),
                materialization: materialization.clone(),
                native_root_id: materialization.native_root_id,
                source_session: Some(source_session),
            },
        })
    }

    pub(super) fn shutdown(&mut self) -> Result<(), String> {
        let Some(mut supervised) = self.supervised.take() else {
            return Ok(());
        };
        supervised
            .shutdown(
                Duration::from_secs(10),
                Duration::from_secs(5),
                Duration::from_secs(5),
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl Drop for BootstrapJavaRuntimeV1 {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

impl ManagedJavaRuntimeV1 {
    pub fn bootstrap_constructed_combat(
        repository: &Path,
        case_id: &str,
        request_count: u64,
        storage_registry: &OwnedStorageRegistryV1,
    ) -> Result<ConstructedCombatBootstrapOpenV1, String> {
        let total_started = Instant::now();
        if case_id != "cultist-two-strikes-root" {
            return Err(
                "constructed combat bootstrap v1 supports only cultist-two-strikes-root".to_owned(),
            );
        }
        let attempt =
            create_operation_directory(repository, "bootstrap-constructed", request_count)?;
        let plan_path = attempt.join("scenario-plan.json");
        let scenario_report_path = attempt.join("scenario-report.json");
        let adapter_report_path = attempt.join("constructed-root.json");
        let scenario_catalog = repository.join("fixtures").join("java_parity_cases");
        let plan =
            sts_java_parity_live::compile_scenario_case(&scenario_catalog, case_id, &plan_path)?;
        if plan.recipe.case_id != case_id {
            return Err("constructed scenario compiler returned another case".to_owned());
        }

        let (mut bootstrap, runtime_ready_ms) =
            BootstrapJavaRuntimeV1::open_fresh_scenario(repository, storage_registry)?;
        let scenario_started = Instant::now();
        let scenario_report = sts_java_parity_live::run_constructed_combat_scenario(
            &bootstrap.live_output,
            &plan_path,
            &bootstrap.launch_plan,
            &scenario_report_path,
            Duration::from_secs(60),
        )?;
        let scenario_ms = scenario_started.elapsed().as_millis();
        bootstrap.validate_epoch()?;
        if !scenario_report.passed
            || !scenario_report.evidence_scope.constructed_root_verified
            || scenario_report.root.is_none()
        {
            return Err("constructed Java scenario did not publish a verified root".to_owned());
        }

        let session = repository
            .join(".oracle-lab")
            .join("run-play")
            .join(format!(
                "constructed-{}-{}-{}",
                case_id,
                now_unix_ms(),
                Uuid::new_v4().simple()
            ));
        let adapter_arguments = vec![
            "paired-root-bootstrap-constructed-combat".to_owned(),
            "--session".to_owned(),
            path_text(&session),
            "--scenario-plan".to_owned(),
            path_text(&plan_path),
            "--scenario-report".to_owned(),
            path_text(&scenario_report_path),
            "--java-live-output".to_owned(),
            path_text(&bootstrap.live_output),
            "--store".to_owned(),
            path_text(&repository.join(".oracle-lab").join("paired-root-store")),
            "--attempt".to_owned(),
            path_text(&attempt.join("adapter")),
            "--report".to_owned(),
            path_text(&adapter_report_path),
        ];
        let adapter_started = Instant::now();
        run_typed_adapter(
            repository,
            &attempt,
            "bootstrap-constructed",
            &adapter_arguments,
            Duration::from_secs(120),
        )?;
        let adapter_ms = adapter_started.elapsed().as_millis();
        let report: ConstructedCombatAdapterReportV1 = read_json(&adapter_report_path)?;
        validate_constructed_combat_adapter_report(&report, case_id, &plan.recipe.scenario_id)?;
        let root_id = report
            .root_id
            .clone()
            .ok_or_else(|| "constructed adapter ready report omitted root_id".to_owned())?;
        let activation_id = report
            .activation_id
            .clone()
            .ok_or_else(|| "constructed adapter ready report omitted activation_id".to_owned())?;
        let action_space_id = report
            .action_space_id
            .clone()
            .ok_or_else(|| "constructed adapter ready report omitted action_space_id".to_owned())?;
        let java_runtime_bundle = report.java_runtime_bundle.clone().ok_or_else(|| {
            "constructed adapter ready report omitted Java runtime bundle".to_owned()
        })?;
        let java_runtime_entrypoint = report.java_runtime_entrypoint.clone().ok_or_else(|| {
            "constructed adapter ready report omitted Java runtime entrypoint".to_owned()
        })?;

        let store_root = repository.join(".oracle-lab").join("paired-root-store");
        let store =
            sts_paired_root::StateRootStoreV2::new(&store_root).with_operation_verification_cache();
        store.initialize()?;
        let root = store.load_root(&root_id)?;
        let activation = store.load_activation(&activation_id)?;
        if root.root.parent.is_some()
            || root.root.provenance != sts_paired_root::RootProvenanceV1::Forge
            || root.root.decision_profile
                != sts_paired_root::DecisionProfileV1::CombatPlayerDecision
            || root.root.content_profile != "all_unlocked"
            || report.java_cold_recovery != "unsupported"
            || java_runtime_entrypoint != "scenario_recipe"
            || activation.activation.root_id != root.root_id
            || activation.activation.action_space_id != action_space_id
            || activation.activation.java_runtime_epoch != bootstrap.runtime_epoch()
        {
            return Err("constructed adapter published an invalid root/activation".to_owned());
        }
        let java_manifest = store.verify_bundle(&java_runtime_bundle)?;
        let storage_root = bootstrap
            .supervised
            .as_ref()
            .expect("constructed bootstrap retains resources")
            .storage_root();
        let autosave = single_ironclad_autosave_in(&storage_root.join("saves"))?;
        let autosave_sha256 = file_sha256(&autosave)?;
        let materialization = NativeMaterializationV1 {
            schema: MATERIALIZATION_SCHEMA.to_owned(),
            requested_target_root_id: root.root_id.clone(),
            native_root_id: root.root_id.clone(),
            recovery_certificate_id: None,
            java_bundle_id: java_runtime_bundle.bundle_id,
            content_profile: root.root.content_profile.clone(),
            java_entrypoint: java_runtime_entrypoint,
            materialized_directory: attempt.to_string_lossy().into_owned(),
            storage_directory: storage_root.to_string_lossy().into_owned(),
            file_count: java_manifest.files.len(),
            _recovery_steps: 0,
        };
        let runtime = bootstrap.into_managed(materialization, session, autosave_sha256)?;
        Ok(ConstructedCombatBootstrapOpenV1 {
            runtime,
            root: RootBindingV1 {
                root_id,
                activation_id,
                action_space_id,
                source_head_sha256: report.source_head_guard.after_sha256,
            },
            case_id: case_id.to_owned(),
            scenario_id: plan.recipe.scenario_id,
            exact_components: report.components.len(),
            runtime_ready_ms,
            scenario_ms,
            adapter_ms,
            total_ms: total_started.elapsed().as_millis(),
        })
    }

    pub fn bootstrap_natural_neow(
        repository: &Path,
        seed: u64,
        ascension: u8,
        request_count: u64,
        storage_registry: &OwnedStorageRegistryV1,
    ) -> Result<NaturalNeowBootstrapOpenV1, String> {
        let total_started = Instant::now();
        if ascension != 20 {
            return Err("natural parentless workbench bootstrap is scoped to A20".to_owned());
        }
        let attempt = create_operation_directory(repository, "bootstrap-neow", request_count)?;
        let recovery_storage = attempt.join("native-storage");
        let first_frame_path = attempt.join("first-neow-root-frame.json");
        let first_receipt_path = attempt.join("first-neow-talk-receipt.json");

        let (mut first, first_runtime_ready_ms) =
            BootstrapJavaRuntimeV1::open_fresh(repository, storage_registry)?;
        first.start_run_to_neow_intro(seed, ascension)?;
        let first_autosave = first.snapshot_recovery_storage(&recovery_storage)?;
        let native_autosave_sha256 = file_sha256(&first_autosave)?;
        let (first_receipt, first_live) = first.submit_neow_intro()?;
        let receipt_successor_path = first_receipt
            .successor_frame_path
            .as_deref()
            .map(Path::new)
            .ok_or_else(|| "first Neow Talk receipt omitted its successor frame".to_owned())?;
        let first_frame_bytes = fs::read(receipt_successor_path).map_err(|error| {
            format!(
                "failed to read first Neow Talk successor {}: {error}",
                receipt_successor_path.display()
            )
        })?;
        let receipt_successor: Value = serde_json::from_slice(&first_frame_bytes)
            .map_err(|error| format!("failed to decode first Neow Talk successor: {error}"))?;
        if receipt_successor != first_live.frame {
            return Err(
                "first Neow Talk receipt frame differs from the settled live observation"
                    .to_owned(),
            );
        }
        write_bytes_new(&first_frame_path, &first_frame_bytes)?;
        write_json_new(
            &first_receipt_path,
            &serde_json::to_value(&first_receipt)
                .map_err(|error| format!("failed to encode first Neow Talk receipt: {error}"))?,
        )?;

        let session = repository
            .join(".oracle-lab")
            .join("run-play")
            .join(format!(
                "parentless-neow-{seed}-{}-{}",
                now_unix_ms(),
                Uuid::new_v4().simple()
            ));
        let adapter_attempt = attempt.join("adapter");
        let report_path = attempt.join("parentless-neow.json");
        let arguments = vec![
            "paired-root-bootstrap-natural-neow".to_owned(),
            "--session".to_owned(),
            path_text(&session),
            "--seed".to_owned(),
            seed.to_string(),
            "--ascension".to_owned(),
            ascension.to_string(),
            "--first-java-frame".to_owned(),
            path_text(&first_frame_path),
            "--first-java-talk-receipt".to_owned(),
            path_text(&first_receipt_path),
            "--java-recovery-storage".to_owned(),
            path_text(&recovery_storage),
            "--native-autosave-sha256".to_owned(),
            native_autosave_sha256.clone(),
            "--java-live-output".to_owned(),
            path_text(&first.live_output),
            "--store".to_owned(),
            path_text(&repository.join(".oracle-lab").join("paired-root-store")),
            "--attempt".to_owned(),
            path_text(&adapter_attempt),
            "--report".to_owned(),
            path_text(&report_path),
        ];
        let adapter_started = Instant::now();
        run_typed_adapter(
            repository,
            &attempt,
            "bootstrap-neow",
            &arguments,
            Duration::from_secs(120),
        )?;
        let adapter_ms = adapter_started.elapsed().as_millis();
        let report: ParentlessNaturalNeowAdapterReportV1 = read_json(&report_path)?;
        validate_parentless_neow_adapter_report(&report, seed, ascension)?;

        let store_root = repository.join(".oracle-lab").join("paired-root-store");
        let store =
            sts_paired_root::StateRootStoreV2::new(&store_root).with_operation_verification_cache();
        store.initialize()?;
        let root = store.load_root(&report.root_id)?;
        let activation = store.load_activation(&report.activation_id)?;
        if root.root.parent.is_some()
            || root.root.provenance != sts_paired_root::RootProvenanceV1::Probe
            || root.root.decision_profile != sts_paired_root::DecisionProfileV1::Neow
            || root.root.content_profile != "all_unlocked"
            || activation.activation.root_id != root.root_id
            || activation.activation.action_space_id != report.action_space_id
            || activation.activation.java_runtime_epoch != first.runtime_epoch()
        {
            return Err("parentless Neow adapter published an invalid root/activation".to_owned());
        }
        let java_manifest = store.verify_bundle(&report.java_runtime_bundle)?;
        if report.java_runtime_entrypoint != "native_checkpoint_continue_then_neow_intro"
            || report.recovery_certificate != "pending"
        {
            return Err(
                "parentless Neow adapter published invalid runtime qualification".to_owned(),
            );
        }
        let storage_root = first
            .supervised
            .as_ref()
            .expect("parentless bootstrap runtime retains resources")
            .storage_root();
        let materialization = NativeMaterializationV1 {
            schema: MATERIALIZATION_SCHEMA.to_owned(),
            requested_target_root_id: root.root_id.clone(),
            native_root_id: root.root_id.clone(),
            recovery_certificate_id: None,
            java_bundle_id: report.java_runtime_bundle.bundle_id.clone(),
            content_profile: root.root.content_profile.clone(),
            java_entrypoint: report.java_runtime_entrypoint.clone(),
            materialized_directory: attempt.to_string_lossy().into_owned(),
            storage_directory: storage_root.to_string_lossy().into_owned(),
            file_count: java_manifest.files.len(),
            _recovery_steps: 0,
        };
        let runtime = first.into_managed(materialization, session, native_autosave_sha256)?;
        Ok(NaturalNeowBootstrapOpenV1 {
            runtime,
            root: RootBindingV1 {
                root_id: report.root_id,
                activation_id: report.activation_id,
                action_space_id: report.action_space_id,
                source_head_sha256: report.source_head_guard.after_sha256,
            },
            seed,
            ascension,
            neow_offer: report.neow_offer,
            exact_components: report.components.len(),
            runtime_ready_ms: first_runtime_ready_ms,
            adapter_ms,
            total_ms: total_started.elapsed().as_millis(),
        })
    }

    pub fn open(
        repository: &Path,
        target_root_id: &str,
        request_count: u64,
        storage_registry: &OwnedStorageRegistryV1,
    ) -> Result<RuntimeOpenV1, TypedAdapterFailureV1> {
        let started = Instant::now();
        let build_sha256 = file_sha256(&run_play_executable(repository))?;
        let game_directory = storage_registry.source_game_directory().to_path_buf();
        validate_runtime_configuration(repository, &game_directory)?;
        validate_target_java_build_versions(repository, &game_directory, target_root_id)?;
        let attempt = create_operation_directory(repository, "open", request_count)?;
        let materialization = materialize_native(repository, target_root_id, &attempt)?;
        let storage = IsolatedStorageV1::create(
            Path::new(&materialization.storage_directory),
            storage_registry,
        )?;
        let launch_scope = create_managed_java_launch_scope(repository)?;
        let launch_plan = create_launch_plan(
            repository,
            &game_directory,
            &storage,
            RUN_CONTROL_PROFILE,
            &launch_scope,
        )?;
        let live_output = launch_scope.live_output.clone();
        let bridge_executable = launch_scope.bridge_executable.clone();
        let bridge_build_sha256 = launch_scope.bridge_build_sha256.clone();
        let storage_root = storage.root().to_path_buf();
        let game = launch_game(
            repository,
            &game_directory,
            &storage_root,
            &launch_plan,
            RUN_CONTROL_PROFILE,
        )?;
        let processes = SupervisedProcessSetV1::new(game);
        let mut resources = SupervisedResourcesV1::new(processes, storage.into_owned());
        let live = match wait_for_owned_session(
            &mut resources,
            &live_output,
            &launch_plan,
            &storage_root.join("communication_mod_errors.log"),
        ) {
            Ok(live) => live,
            Err(error) => {
                let _ = resources.shutdown(
                    Duration::from_secs(10),
                    Duration::from_secs(5),
                    Duration::from_secs(5),
                );
                return Err(error.into());
            }
        };
        let bridge = admit_live_bridge_process(&bridge_executable, &live_output, &live)?;
        resources
            .admit_companion(bridge)
            .map_err(|error| format!("failed to admit Java bridge process: {error}"))?;
        let mut supervised =
            SupervisedRuntimeV1::new(resources, runtime_session_binding(&live)?)
                .map_err(|error| format!("failed to bind Java runtime session: {error}"))?;
        let prepared = (|| {
            validate_neutral_continue_boundary(&live)?;
            let autosave_path =
                single_ironclad_autosave_in(&supervised.storage_root().join("saves"))?;
            let autosave_sha256 = file_sha256(&autosave_path)?;
            if file_sha256(&run_play_executable(repository))? != build_sha256 {
                return Err(
                    "typed run-play adapter changed while the Java runtime was opening".to_owned(),
                );
            }
            Ok::<_, String>((autosave_path, autosave_sha256))
        })();
        let (autosave_path, autosave_sha256) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = supervised.shutdown(
                    Duration::from_secs(10),
                    Duration::from_secs(5),
                    Duration::from_secs(5),
                );
                return Err(error.into());
            }
        };
        supervised
            .bind_autosave(&autosave_path, &autosave_sha256)
            .map_err(|error| format!("failed to bind managed autosave: {error}"))?;
        supervised
            .bind_frame(runtime_frame_token(&live)?)
            .map_err(|error| format!("failed to bind managed frame: {error}"))?;
        Ok(RuntimeOpenV1 {
            runtime: Self {
                physical: JavaPhysicalRuntimeV1 {
                    repository: repository.to_path_buf(),
                    owner: JavaParityRuntimeOwnerV1::new(supervised, launch_plan, live_output)
                        .map_err(|error| format!("failed to own Java parity runtime: {error}"))?,
                    build_sha256,
                    bridge_build_sha256,
                },
                engine: JavaEngineRuntimeV1 {
                    resident_id: format!("workbench-resident-v1-{}", Uuid::new_v4().simple()),
                    repository: repository.to_path_buf(),
                    materialization: materialization.clone(),
                    native_root_id: materialization.native_root_id.clone(),
                    source_session: None,
                },
            },
            runtime_ready_ms: started.elapsed().as_millis(),
        })
    }

    pub fn resident_binding(&self) -> ResidentBindingV1 {
        ResidentBindingV1 {
            resident_id: self.engine.resident_id.clone(),
            process_id: self
                .physical
                .owner
                .primary_process_id()
                .expect("managed runtime is open"),
            native_root_id: self.engine.native_root_id.clone(),
            runtime_epoch: self
                .physical
                .owner
                .runtime_epoch()
                .expect("managed runtime is open")
                .to_owned(),
            autosave_sha256: self
                .physical
                .owner
                .autosave_identity()
                .expect("managed runtime has a bound autosave")
                .to_owned(),
            build_sha256: self.physical.build_sha256.clone(),
        }
    }

    pub(super) fn into_parts(self) -> (JavaPhysicalRuntimeV1, JavaEngineRuntimeV1) {
        (self.physical, self.engine)
    }
}

impl JavaPhysicalRuntimeV1 {
    pub(super) fn bind_native_recovery_scope(
        &self,
        relative_directories: &[&str],
    ) -> Result<sts_workbench_supervisor::NativeRecoveryCaptureScopeV1, String> {
        self.owner
            .bind_native_recovery_scope(relative_directories)
            .map_err(|error| error.to_string())
    }

    pub(super) fn supervisor_identity(&self) -> JavaRuntimeIdentityV1 {
        self.owner
            .runtime_identity()
            .expect("managed runtime is open")
    }

    pub(super) fn accepted_frame_token(&self) -> &str {
        self.owner
            .frame_token()
            .expect("managed runtime has a bound frame")
    }

    pub(super) fn autosave_sha256(&self) -> &str {
        self.owner
            .autosave_identity()
            .expect("managed runtime has a bound autosave")
    }

    pub(super) fn validate_parked_binding(
        &mut self,
        expected_frame_token: &str,
        expected_autosave_identity: &str,
    ) -> Result<(), String> {
        if self.accepted_frame_token() != expected_frame_token
            || self.autosave_sha256() != expected_autosave_identity
        {
            return Err(
                "physical Java runtime differs from the parked Supervisor boundary".to_owned(),
            );
        }
        self.validate_owned_session().map(|_| ())
    }

    pub(super) fn natural_reset_available(&mut self) -> Result<bool, String> {
        let live = self.validate_owned_session()?;
        natural_reset_action_available(&live)
    }

    pub(super) fn reset_to_native(
        &mut self,
        request_count: u64,
        frame_trust: ResetFrameTrustV1,
    ) -> Result<(), String> {
        let current = match frame_trust {
            ResetFrameTrustV1::Accepted => self.validate_owned_session()?,
            ResetFrameTrustV1::Dirty => self.validate_owned_process_and_autosave()?,
        };
        if !self
            .owner
            .native_autosave_is_current()
            .map_err(|error| error.to_string())?
        {
            return Err("save-preserving reset cannot recover a runtime whose autosave changed from its native root".to_owned());
        }
        let before_autosave = self.autosave_sha256().to_owned();
        if validate_neutral_continue_boundary(&current).is_ok() {
            self.owner
                .accept_frame(runtime_frame_token(&current)?)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        if !natural_reset_action_available(&current)? {
            return Err(
                "natural native reset is unavailable at the current Java boundary".to_owned(),
            );
        }
        let attempt = create_operation_directory(&self.repository, "reset", request_count)?;
        let report_path = attempt.join("natural-reset.json");
        let report = sts_java_parity_live::reset_natural_run_to_main_menu_preserving_save(
            self.owner.live_output(),
            &report_path,
            Duration::from_secs(30),
        )?;
        if report.schema != "sts_java_parity_natural_run_reset_v1"
            || report.provenance != "natural_native_checkpoint"
            || !report.passed
            || report.session_id
                != self
                    .owner
                    .client_session_id()
                    .map_err(|error| error.to_string())?
            || report.connection_id
                != self
                    .owner
                    .connection_id()
                    .map_err(|error| error.to_string())?
            || !report.preserved_save_main_menu_verified
        {
            return Err("natural run reset returned invalid scoped evidence".to_owned());
        }
        let after = self.observe_epoch()?;
        validate_neutral_continue_boundary(&after)?;
        self.owner
            .verify_autosave()
            .map_err(|error| error.to_string())?;
        if self.autosave_sha256() != before_autosave {
            return Err("save-preserving reset changed the native autosave".to_owned());
        }
        self.owner
            .accept_frame(runtime_frame_token(&after)?)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn validate_owned_session(&mut self) -> Result<LiveStatus, String> {
        let live = self.validate_owned_process_and_autosave()?;
        self.owner
            .verify_frame(&runtime_frame_token(&live)?)
            .map_err(|error| error.to_string())?;
        Ok(live)
    }

    fn validate_owned_process_and_autosave(&mut self) -> Result<LiveStatus, String> {
        let live = self.observe_epoch()?;
        self.owner
            .verify_autosave()
            .map_err(|error| error.to_string())?;
        Ok(live)
    }

    fn observe_epoch(&mut self) -> Result<LiveStatus, String> {
        let live = self
            .owner
            .status()
            .map_err(|error| format!("managed Java runtime epoch changed: {error}"))?
            .into_live();
        self.validate_adapter_build()?;
        Ok(live)
    }

    fn validate_adapter_build(&self) -> Result<(), String> {
        if file_sha256(&run_play_executable(&self.repository))? != self.build_sha256 {
            return Err(
                "typed run-play adapter changed while a Java runtime was parked".to_owned(),
            );
        }
        if file_sha256(&live_bridge_executable(&self.repository))? != self.bridge_build_sha256 {
            return Err("Java parity bridge changed while a Java runtime was parked".to_owned());
        }
        Ok(())
    }

    pub(super) fn shutdown(&mut self) -> Result<(), String> {
        self.owner.shutdown().map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl<'a> JavaRuntimeAccessV1<'a> {
    pub(super) fn new(
        physical: &'a mut JavaPhysicalRuntimeV1,
        engine: &'a mut JavaEngineRuntimeV1,
    ) -> Self {
        Self { physical, engine }
    }

    pub fn content_profile(materialization: &NativeMaterializationV1) -> &str {
        &materialization.content_profile
    }

    pub fn materialization(&self) -> &NativeMaterializationV1 {
        &self.engine.materialization
    }

    pub fn live_output(&self) -> &Path {
        self.physical.owner.live_output()
    }

    pub fn run_root(&self) -> PathBuf {
        self.engine.repository.join(".oracle-lab").join("run-play")
    }

    pub fn source_session(&self) -> Option<&Path> {
        self.engine.source_session.as_deref()
    }

    pub fn bind_source_session(&mut self, source_session: PathBuf) -> Result<(), String> {
        if !source_session.is_dir() {
            return Err(format!(
                "paired source session is absent: {}",
                source_session.display()
            ));
        }
        match &self.engine.source_session {
            Some(current) if !same_path(current, &source_session) => {
                Err("managed runtime attempted to change paired source session".to_owned())
            }
            Some(_) => Ok(()),
            None => {
                self.engine.source_session = Some(source_session);
                Ok(())
            }
        }
    }

    pub fn autosave_sha256(&self) -> &str {
        self.physical
            .owner
            .autosave_identity()
            .expect("managed runtime has a bound autosave")
    }

    pub fn current_autosave_sha256_optional(&self) -> Result<Option<String>, String> {
        self.physical
            .owner
            .current_autosave_identity_optional()
            .map_err(|error| error.to_string())
    }

    pub fn runtime_epoch(&self) -> &str {
        self.physical
            .owner
            .runtime_epoch()
            .expect("managed runtime is open")
    }

    pub fn can_recover_target(&self, target_root_id: &str) -> Result<bool, String> {
        if !self
            .physical
            .owner
            .native_autosave_is_current()
            .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        let store = sts_paired_root::StateRootStoreV2::new(
            self.engine
                .repository
                .join(".oracle-lab")
                .join("paired-root-store"),
        )
        .with_operation_verification_cache();
        store.initialize()?;
        Ok(store.recovery_plan(target_root_id)?.native_root_id == self.engine.native_root_id)
    }

    pub fn natural_reset_available(&mut self) -> Result<bool, String> {
        self.physical.natural_reset_available()
    }

    pub fn validate_owned_session(&mut self) -> Result<LiveStatus, String> {
        self.physical.validate_owned_session()
    }

    pub fn validate_transition_identity(&mut self) -> Result<(), String> {
        self.validate_epoch_only()
    }

    pub fn restore_native(
        &mut self,
        materialization: &NativeMaterializationV1,
    ) -> Result<(), String> {
        if materialization.native_root_id != self.engine.native_root_id
            || materialization.java_bundle_id != self.engine.materialization.java_bundle_id
        {
            return Err("managed runtime received another native materialization".to_owned());
        }
        self.validate_owned_session()?;
        sts_java_parity_live::continue_run_with_content_profile(
            self.physical.owner.live_output(),
            Self::content_profile(materialization),
            Duration::from_secs(30),
        )?;
        self.validate_epoch_only()?;
        match materialization.java_entrypoint.as_str() {
            "native_checkpoint_continue"
            | sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1 => {}
            "native_checkpoint_continue_then_neow_intro" => {
                let live = sts_java_parity_live::status(self.physical.owner.live_output())?;
                let Some(intro_screen) = neow_intro_screen(&live) else {
                    return Err(
                        "parentless Neow native Continue did not reach its intro screen".to_owned(),
                    );
                };
                let expected_action_id = neow_intro_action_id(intro_screen);
                let mut intro = live.actions.iter().filter(|action| {
                    action.kind == "choose_event_option"
                        && action.action_id == expected_action_id
                        && action.event_id.as_deref() == Some(NEOW_EVENT_ID)
                        && action.option_index == Some(0)
                        && action.neow_reward_type.is_none()
                        && action.neow_drawback.is_none()
                });
                let action = intro
                    .next()
                    .ok_or_else(|| "parentless Neow intro omitted its typed Talk".to_owned())?;
                if intro.next().is_some() || action.parameters_required {
                    return Err("parentless Neow intro Talk is not uniquely typed".to_owned());
                }
                sts_java_parity_live::act(
                    self.physical.owner.live_output(),
                    &action.handle,
                    Duration::from_secs(30),
                )?;
                self.validate_epoch_only()?;
            }
            other => {
                return Err(format!(
                    "managed runtime does not support Java entrypoint {other:?}"
                ))
            }
        }
        Ok(())
    }

    pub fn accept_successor_autosave(&mut self) -> Result<(), String> {
        let live = self.observe_epoch()?;
        let frame_token = runtime_frame_token(&live)?;
        self.physical
            .owner
            .accept_current_autosave()
            .map_err(|error| error.to_string())?;
        self.physical
            .owner
            .accept_frame(frame_token)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn validate_epoch_only(&mut self) -> Result<(), String> {
        self.observe_epoch().map(|_| ())
    }

    fn observe_epoch(&mut self) -> Result<LiveStatus, String> {
        self.physical.observe_epoch()
    }
}

impl JavaEngineRuntimeV1 {
    pub(super) fn accept_native_reset(&mut self) {
        self.source_session = None;
    }
}

#[derive(Debug)]
struct IsolatedStorageV1 {
    owned: OwnedStorageV1,
    profile_substrate: String,
    tutorial_policy: String,
}

impl IsolatedStorageV1 {
    fn create(seed_root: &Path, storage_registry: &OwnedStorageRegistryV1) -> Result<Self, String> {
        let owned = storage_registry
            .create()
            .map_err(|error| error.to_string())?;
        let mut storage = Self {
            owned,
            profile_substrate: CAPTURED_NATIVE_PROFILE_SUBSTRATE.to_owned(),
            tutorial_policy: OBSERVED_TUTORIAL_POLICY.to_owned(),
        };
        storage.populate(seed_root, true, true)?;
        Ok(storage)
    }

    fn create_fresh(storage_registry: &OwnedStorageRegistryV1) -> Result<Self, String> {
        let mut storage = Self::create_empty(
            storage_registry,
            CANONICAL_FRESH_PROFILE_SUBSTRATE,
            CANONICAL_TUTORIAL_POLICY,
        )?;
        let profile_root = storage.game_directory().to_path_buf();
        storage.populate(&profile_root, false, false)?;
        storage.write_canonical_fresh_preferences()?;
        Ok(storage)
    }

    fn write_canonical_fresh_preferences(&self) -> Result<(), String> {
        // This is the routine A20H workbench profile, not the tutorial sentinel.
        // It mirrors TipTracker.disableAllFtues(), adds the separate Neow intro
        // flag, and keeps the original game's counter-intuitive routing value:
        // NEOW_SKIP=false skips Neow on the first run, while true enters Neow.
        write_bytes_new(
            &self.root().join("preferences").join("STSTips"),
            CANONICAL_STS_TIPS,
        )
    }

    fn create_empty(
        storage_registry: &OwnedStorageRegistryV1,
        profile_substrate: &str,
        tutorial_policy: &str,
    ) -> Result<Self, String> {
        let owned = storage_registry
            .create()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            owned,
            profile_substrate: profile_substrate.to_owned(),
            tutorial_policy: tutorial_policy.to_owned(),
        })
    }

    fn populate(
        &mut self,
        seed_root: &Path,
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
            fs::create_dir(&destination).map_err(|error| {
                format!("failed to create isolated {directory} directory: {error}")
            })?;
            let source = seed_root.join(directory);
            let include = match directory {
                "preferences" | "betaPreferences" => include_preferences,
                "saves" => include_saves,
                _ => false,
            };
            if source.is_dir() && include {
                copy_directory_contents(&source, &destination)?;
            }
        }
        for directory in ["runs", "sendToDevs"] {
            fs::create_dir(self.root().join(directory)).map_err(|error| {
                format!("failed to create isolated {directory} directory: {error}")
            })?;
        }
        for file in [
            "config.json",
            "info.displayconfig",
            "steam_appid.txt",
            "twitchconfig.txt",
        ] {
            let source = self.game_directory().join(file);
            if source.is_file() {
                fs::copy(&source, self.root().join(file)).map_err(|error| {
                    format!("failed to copy isolated launch file {file}: {error}")
                })?;
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

    fn isolation_id(&self) -> Uuid {
        self.owned.isolation_id()
    }

    fn into_owned(self) -> OwnedStorageV1 {
        self.owned
    }
}

fn snapshot_profile_storage(storage_root: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err(format!(
            "bootstrap storage snapshot must be create-new: {}",
            destination.display()
        ));
    }
    fs::create_dir(destination)
        .map_err(|error| format!("failed to create bootstrap storage snapshot: {error}"))?;
    for directory in ["preferences", "betaPreferences", "saves"] {
        let source = storage_root.join(directory);
        let target = destination.join(directory);
        if !source.is_dir() {
            return Err(format!(
                "bootstrap runtime omitted its {directory} directory"
            ));
        }
        fs::create_dir(&target)
            .map_err(|error| format!("failed to create snapshot {directory}: {error}"))?;
        copy_directory_contents(&source, &target)?;
    }
    Ok(())
}

fn single_ironclad_autosave_in(saves: &Path) -> Result<PathBuf, String> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(saves)
        .map_err(|error| format!("failed to inspect isolated autosaves: {error}"))?
    {
        let entry = entry.map_err(|error| format!("failed to enumerate autosaves: {error}"))?;
        if entry
            .file_type()
            .map_err(|error| format!("failed to inspect autosave: {error}"))?
            .is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .ends_with("IRONCLAD.autosave")
        {
            matches.push(entry.path());
        }
    }
    if matches.len() != 1 {
        return Err(format!(
            "isolated native runtime contains {} Ironclad autosaves instead of one",
            matches.len()
        ));
    }
    Ok(matches.remove(0))
}

#[derive(Clone, Debug, Serialize)]
struct FileIdentityV1 {
    path: String,
    length: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ModMetadataV1 {
    modid: String,
    version: String,
}

#[derive(Clone, Debug, Serialize)]
struct ModIdentityV1 {
    id: String,
    version: String,
    jar: FileIdentityV1,
}

fn materialize_native(
    repository: &Path,
    target_root_id: &str,
    attempt: &Path,
) -> Result<NativeMaterializationV1, String> {
    let output_directory = attempt.join("native-payload");
    let report_path = attempt.join("native-materialization.json");
    let mut command = Command::new(run_play_executable(repository));
    command
        .current_dir(repository)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("paired-root-materialize-native")
        .arg("--target-root")
        .arg(target_root_id)
        .arg("--store")
        .arg(repository.join(".oracle-lab").join("paired-root-store"))
        .arg("--output")
        .arg(&output_directory)
        .arg("--report")
        .arg(&report_path);
    let output = run_bounded(command, Duration::from_secs(30))?;
    persist_subprocess_output(attempt, "materialize", &output)?;
    if !output.status.success() {
        return Err(format!(
            "native materialization exited with {}: {}",
            output.status,
            crate::subprocess::failure_detail(&output.stdout, &output.stderr)
        ));
    }
    let report: NativeMaterializationV1 = read_json(&report_path)?;
    if report.schema != MATERIALIZATION_SCHEMA
        || report.requested_target_root_id != target_root_id
        || !content_id_has_shape(&report.native_root_id, "state-root-v2")
        || report
            .recovery_certificate_id
            .as_deref()
            .is_none_or(|value| !content_id_has_shape(value, "state-root-recovery-certificate-v2"))
        || !content_id_has_shape(&report.java_bundle_id, "paired-bundle-v1")
        || !Path::new(&report.materialized_directory).is_dir()
        || !Path::new(&report.storage_directory).is_dir()
        || report.file_count == 0
        || !matches!(
            report.java_entrypoint.as_str(),
            "native_checkpoint_continue"
                | sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1
                | "native_checkpoint_continue_then_neow_intro"
        )
    {
        return Err("native materialization returned invalid scoped evidence".to_owned());
    }
    Ok(report)
}

fn validate_runtime_configuration(repository: &Path, game_directory: &Path) -> Result<(), String> {
    let run_play = run_play_executable(repository);
    let live_bridge = live_bridge_executable(repository);
    let communication_mod = game_directory.join("mods").join("CommunicationMod.jar");
    for required in [
        run_play.clone(),
        live_bridge.clone(),
        game_directory.join("jre").join("bin").join("javaw.exe"),
        repository
            .parent()
            .ok_or_else(|| "repository has no parent for ModTheSpire".to_owned())?
            .join("lib")
            .join("ModTheSpire.jar"),
        communication_mod.clone(),
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
        &communication_mod,
        "communicationmod/ManagedBridgeLaunchV1.class",
        "installed CommunicationMod does not support launch-plan-scoped bridge endpoints",
    )?;
    require_cargo_binary_fresh(
        repository,
        &run_play,
        &[
            repository.join("Cargo.toml"),
            repository.join("Cargo.lock"),
            repository.join(".cargo/config.toml"),
            repository.join("crates/sts_run_play/Cargo.toml"),
        ],
        "run-play adapter",
    )?;
    require_cargo_binary_fresh(
        repository,
        &live_bridge,
        &[
            repository.join("Cargo.toml"),
            repository.join("Cargo.lock"),
            repository.join(".cargo/config.toml"),
            repository.join("crates/sts_java_parity_live/Cargo.toml"),
            repository.join("crates/sts_java_parity_protocol/Cargo.toml"),
        ],
        "Java parity live bridge",
    )?;
    let local_app_data = PathBuf::from(
        env::var_os("LOCALAPPDATA")
            .ok_or_else(|| "LOCALAPPDATA is unavailable for Mod configuration".to_owned())?,
    );
    let acceleration = local_app_data
        .join("ModTheSpire")
        .join("SuperFastMode")
        .join("SuperFastModeConfig.properties");
    let properties = read_properties(&acceleration)?;
    let multiplier = properties
        .get("deltaMultiplier")
        .and_then(|value| value.parse::<f64>().ok())
        .ok_or_else(|| "SuperFastMode deltaMultiplier is invalid".to_owned())?;
    if properties.get("isDeltaMultiplied").map(String::as_str) != Some("true")
        || properties.get("isInstantLerp").map(String::as_str) != Some("true")
        || multiplier <= 1.0
    {
        return Err("SuperFastMode accelerated settings are not active".to_owned());
    }
    Ok(())
}

fn validate_target_java_build_versions(
    repository: &Path,
    game_directory: &Path,
    target_root_id: &str,
) -> Result<(), TypedAdapterFailureV1> {
    let store = sts_paired_root::StateRootStoreV2::new(
        repository.join(".oracle-lab").join("paired-root-store"),
    )
    .with_operation_verification_cache();
    store.initialize()?;
    let root = store.load_root(target_root_id)?;
    validate_java_build_versions(
        &root.root.versions,
        &file_sha256(&game_directory.join("mods").join("CommunicationMod.jar"))?,
        &file_sha256(&game_directory.join("desktop-1.0.jar"))?,
    )
}

fn validate_java_build_versions(
    versions: &[sts_paired_root::VersionIdentityV1],
    communication_mod_jar: &str,
    slay_the_spire_jar: &str,
) -> Result<(), TypedAdapterFailureV1> {
    for (component, active_identity) in [
        ("communicationmod_jar", communication_mod_jar),
        ("slay_the_spire_jar", slay_the_spire_jar),
    ] {
        let stored_identity = versions
            .iter()
            .find(|version| version.component == component)
            .map(|version| version.identity.as_str())
            .ok_or_else(|| {
                TypedAdapterFailureV1::new(
                    PAIRED_ROOT_VERSION_DRIFT_CODE,
                    format!("paired root omits required version component {component}"),
                )
            })?;
        if !stored_identity.eq_ignore_ascii_case(active_identity) {
            return Err(TypedAdapterFailureV1::new(
                PAIRED_ROOT_VERSION_DRIFT_CODE,
                format!(
                    "paired-root version drift for {component}: stored={stored_identity:?} active={active_identity:?}"
                ),
            ));
        }
    }
    Ok(())
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

fn require_cargo_binary_fresh(
    repository: &Path,
    executable: &Path,
    cargo_inputs: &[PathBuf],
    label: &str,
) -> Result<(), String> {
    oracle_artifact_contract::ensure_depfile_artifact_fresh(
        executable,
        repository,
        cargo_inputs,
        label,
        ".\\workbench.ps1 build",
    )
}

fn create_launch_plan(
    repository: &Path,
    game_directory: &Path,
    storage: &IsolatedStorageV1,
    control_profile: &str,
    launch_scope: &ManagedJavaLaunchScopeV1,
) -> Result<PathBuf, String> {
    if !matches!(
        control_profile,
        RUN_CONTROL_PROFILE | SCENARIO_CONTROL_PROFILE
    ) {
        return Err("workbench launch requested an unsupported control profile".to_owned());
    }
    let base_mod = resolve_base_mod(game_directory)?;
    let super_fast = game_directory.join("mods").join("SuperFastMode.jar");
    let communication = game_directory.join("mods").join("CommunicationMod.jar");
    let selected_mods = vec![
        mod_identity(&base_mod, "basemod")?,
        mod_identity(&super_fast, "superfastmode")?,
        mod_identity(&communication, "CommunicationMod")?,
    ];
    let local_app_data = PathBuf::from(
        env::var_os("LOCALAPPDATA")
            .ok_or_else(|| "LOCALAPPDATA is unavailable for launch plan".to_owned())?,
    );
    let acceleration_config = local_app_data
        .join("ModTheSpire")
        .join("SuperFastMode")
        .join("SuperFastModeConfig.properties");
    let properties = read_properties(&acceleration_config)?;
    let multiplier = properties
        .get("deltaMultiplier")
        .and_then(|value| value.parse::<f64>().ok())
        .ok_or_else(|| "SuperFastMode deltaMultiplier is invalid".to_owned())?;
    let mut bridge_identity = file_identity(&launch_scope.bridge_executable)?;
    bridge_identity.path = java_8_compatible_path(&launch_scope.bridge_executable)?;
    let plan = json!({
        "schema": "sts_java_play_launch_plan_v4",
        "created_at_unix_ms": now_unix_ms(),
        "profile": LAUNCH_PROFILE,
        "control_profile": control_profile,
        "selected_mod_ids": ["basemod", "superfastmode", "CommunicationMod"],
        "selected_mods": selected_mods,
        "game_java": file_identity(&game_directory.join("jre").join("bin").join("javaw.exe"))?,
        "mod_the_spire": file_identity(&repository.parent().expect("validated repository parent").join("lib").join("ModTheSpire.jar"))?,
        "acceleration": {
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
            "output_directory": java_8_compatible_path(&launch_scope.live_output)?
        },
        "storage": {
            "class": "isolated_working_directory_v1",
            "profile_substrate": storage.profile_substrate,
            "tutorial_policy": storage.tutorial_policy,
            "working_directory": storage.root().to_string_lossy(),
            "source_game_directory": game_directory.to_string_lossy(),
            "isolation_id": storage.isolation_id().hyphenated().to_string()
        }
    });
    write_json_new(&launch_scope.launch_plan, &plan)?;
    Ok(launch_scope.launch_plan.clone())
}

fn create_managed_java_launch_scope(repository: &Path) -> Result<ManagedJavaLaunchScopeV1, String> {
    let parent = repository
        .join(".oracle-lab")
        .join("workbench-runtime")
        .join("java-launches");
    fs::create_dir_all(&parent)
        .map_err(|error| format!("failed to create managed Java launch directory: {error}"))?;
    let root = parent.join(Uuid::new_v4().simple().to_string());
    fs::create_dir(&root)
        .map_err(|error| format!("failed to create managed Java launch scope: {error}"))?;
    let live_output = root.join("java-live");
    fs::create_dir(&live_output)
        .map_err(|error| format!("failed to create managed Java live output: {error}"))?;
    let bridge_image = ExecutableImageStoreV1::new(
        repository
            .join(".oracle-lab")
            .join("workbench-runtime")
            .join("live-bridge-images"),
        "sts-java-parity-live",
    )
    .map_err(|error| format!("failed to configure Java bridge images: {error}"))?
    .materialize(&live_bridge_executable(repository))
    .map_err(|error| format!("failed to materialize Java bridge image: {error}"))?;
    let bridge_executable = bridge_image.executable().to_path_buf();
    let bridge_build_sha256 = bridge_image.image_id().to_owned();
    Ok(ManagedJavaLaunchScopeV1 {
        launch_plan: root.join("launch-plan.json"),
        live_output,
        bridge_executable,
        bridge_build_sha256,
    })
}

fn launch_game(
    repository: &Path,
    game_directory: &Path,
    storage_root: &Path,
    launch_plan: &Path,
    control_profile: &str,
) -> Result<SupervisedChildV1, String> {
    if !matches!(
        control_profile,
        RUN_CONTROL_PROFILE | SCENARIO_CONTROL_PROFILE
    ) {
        return Err("managed Java launch requested an unsupported control profile".to_owned());
    }
    let java = game_directory.join("jre").join("bin").join("javaw.exe");
    let mod_the_spire = repository
        .parent()
        .ok_or_else(|| "repository has no parent for ModTheSpire".to_owned())?
        .join("lib")
        .join("ModTheSpire.jar");
    let stdout = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(storage_root.join(MANAGED_JAVA_STDOUT_LOG))
        .map_err(|error| format!("failed to create managed Java stdout log: {error}"))?;
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(storage_root.join(MANAGED_JAVA_STDERR_LOG))
        .map_err(|error| format!("failed to create managed Java stderr log: {error}"))?;
    let java_boundary = Java8ProcessBoundaryV1::new(&mod_the_spire, storage_root, launch_plan)?;
    let mut command = Command::new(java);
    command
        .current_dir(&java_boundary.working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .arg("-jar")
        .arg(&java_boundary.mod_the_spire_argument)
        .arg("--skip-launcher")
        .arg("--skip-intro")
        .arg("--mods")
        .arg("basemod,superfastmode,CommunicationMod")
        .env("STS_JAVA_PLAY_LAUNCH_PROFILE", LAUNCH_PROFILE)
        .env(
            "STS_JAVA_PLAY_LAUNCH_PLAN",
            &java_boundary.launch_plan_environment,
        )
        .env("STS_JAVA_PLAY_CONTROL_PROFILE", control_profile);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map(SupervisedChildV1::adopt)
        .map_err(|error| format!("failed to launch managed Java runtime: {error}"))
}

fn wait_for_owned_session(
    resources: &mut SupervisedResourcesV1,
    live_output: &Path,
    launch_plan: &Path,
    communication_error_log: &Path,
) -> Result<LiveStatus, String> {
    let started = Instant::now();
    while started.elapsed() < STARTUP_TIMEOUT {
        resources.validate_primary_running().map_err(|error| {
            format!("Java runtime exited before publishing its session: {error}")
        })?;
        if communication_error_log.is_file() {
            let detail = fs::read_to_string(communication_error_log).map_err(|error| {
                format!(
                    "failed to read CommunicationMod startup error {}: {error}",
                    communication_error_log.display()
                )
            })?;
            let detail = detail.trim();
            if !detail.is_empty() {
                return Err(format!(
                    "CommunicationMod rejected the launch before its first stable frame: {detail}"
                ));
            }
        }
        if let Ok(live) = sts_java_parity_live::status(live_output) {
            if live
                .manifest
                .launch
                .as_ref()
                .is_some_and(|launch| same_path(Path::new(&launch.plan_path), launch_plan))
            {
                return Ok(live);
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "managed Java runtime did not publish its launch-bound session within {} seconds",
        STARTUP_TIMEOUT.as_secs()
    ))
}

fn admit_live_bridge_process(
    bridge_executable: &Path,
    live_output: &Path,
    observed: &LiveStatus,
) -> Result<SupervisedProcessV1, String> {
    let candidate =
        ExistingProcessCandidateV1::open_running(observed.manifest.process_id, bridge_executable)
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

fn validate_neutral_continue_boundary(live: &LiveStatus) -> Result<(), String> {
    if live.frame["boundary"]["kind"].as_str() != Some("main_menu_ready")
        || live.frame["public_base"]["decision"]["kind"].as_str() != Some("main_menu_ready")
        || live.actions.len() != 1
        || live.actions[0].kind != "continue_run"
        || live.actions[0].action_id != "continue_run"
    {
        return Err("managed Java runtime did not reach one neutral Continue decision".to_owned());
    }
    Ok(())
}

fn natural_reset_action_available(live: &LiveStatus) -> Result<bool, String> {
    let natural_combat = live.frame["boundary"]["kind"].as_str() == Some("combat_player_decision")
        && live.frame["scenario"].is_null()
        && live.frame["public_base"]["run"].is_object();
    if !natural_combat {
        return Ok(false);
    }
    let mut actions = live
        .actions
        .iter()
        .filter(|action| action.kind == "run_reset_to_main_menu_preserving_save");
    let Some(action) = actions.next() else {
        return Err("natural combat omitted its typed save-preserving reset action".to_owned());
    };
    if actions.next().is_some()
        || action.action_id != "run_reset_to_main_menu_preserving_save"
        || action.parameters_required
    {
        return Err("natural combat exposed an invalid save-preserving reset action".to_owned());
    }
    Ok(true)
}

fn validate_fresh_start_boundary(live: &LiveStatus) -> Result<(), String> {
    if live.frame["boundary"]["kind"].as_str() != Some("main_menu_ready")
        || live.frame["public_base"]["decision"]["kind"].as_str() != Some("main_menu_ready")
        || !live.frame["public_base"]["run"].is_null()
        || live.actions.len() != 1
        || live.actions[0].kind != "start_run"
        || live.actions[0].action_id != "start_run"
        || !live.actions[0].parameters_required
    {
        return Err("bootstrap Java runtime did not reach one fresh Start Run decision".to_owned());
    }
    Ok(())
}

fn snapshot_startup_diagnostics(
    repository: &Path,
    storage_root: &Path,
    isolation_id: Uuid,
    launch_plan: &Path,
    live_output: &Path,
    started_at_unix_ms: u128,
    phase: &str,
) -> Result<PathBuf, String> {
    let parent = repository
        .join(".oracle-lab")
        .join("workbench-runtime")
        .join("startup-diagnostics");
    fs::create_dir_all(&parent)
        .map_err(|error| format!("failed to create startup diagnostic root: {error}"))?;
    let destination = parent.join(format!(
        "startup-{}-{}",
        now_unix_ms(),
        Uuid::new_v4().simple()
    ));
    fs::create_dir(&destination)
        .map_err(|error| format!("failed to create startup diagnostic directory: {error}"))?;
    write_json_new(
        &destination.join("scope.json"),
        &json!({
            "schema": "sts-workbench-java-startup-diagnostic-v1",
            "phase": phase,
            "started_at_unix_ms": started_at_unix_ms,
            "isolation_id": isolation_id.to_string(),
        }),
    )?;
    copy_diagnostic_file(launch_plan, &destination.join("launch-plan.json"))?;
    copy_diagnostic_file(
        &storage_root.join("communication_mod_errors.log"),
        &destination.join("communication_mod_errors.log"),
    )?;
    copy_diagnostic_file(
        &storage_root
            .join("sendToDevs")
            .join("logs")
            .join("SlayTheSpire.log"),
        &destination.join("SlayTheSpire.log"),
    )?;
    for log in [MANAGED_JAVA_STDOUT_LOG, MANAGED_JAVA_STDERR_LOG] {
        copy_diagnostic_file(&storage_root.join(log), &destination.join(log))?;
    }
    if live_output.is_dir() {
        for entry in fs::read_dir(live_output)
            .map_err(|error| format!("failed to inspect live startup evidence: {error}"))?
        {
            let entry = entry
                .map_err(|error| format!("failed to enumerate live startup evidence: {error}"))?;
            if !entry
                .file_type()
                .map_err(|error| format!("failed to inspect live startup session: {error}"))?
                .is_dir()
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let session_started = name
                .strip_prefix("session-")
                .and_then(|tail| tail.split('-').next())
                .and_then(|value| value.parse::<u128>().ok());
            if session_started.is_none_or(|value| value < started_at_unix_ms) {
                continue;
            }
            let session_destination = destination.join(&name);
            fs::create_dir(&session_destination).map_err(|error| {
                format!("failed to create live startup diagnostic session: {error}")
            })?;
            for name in ["transcript.jsonl", "ended.json"] {
                copy_diagnostic_file(&entry.path().join(name), &session_destination.join(name))?;
            }
        }
    }
    Ok(destination)
}

fn copy_diagnostic_file(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Ok(());
    }
    fs::copy(source, destination).map_err(|error| {
        format!(
            "failed to copy startup diagnostic {}: {error}",
            source.display()
        )
    })?;
    Ok(())
}

fn with_startup_diagnostics(error: String, diagnostics: Result<PathBuf, String>) -> String {
    match diagnostics {
        Ok(path) => format!("{error}; startup_diagnostics={}", path.display()),
        Err(diagnostic_error) => {
            format!("{error}; startup diagnostic capture also failed: {diagnostic_error}")
        }
    }
}

fn validate_constructed_combat_adapter_report(
    report: &ConstructedCombatAdapterReportV1,
    case_id: &str,
    scenario_id: &str,
) -> Result<(), String> {
    let exact_components = !report.components.is_empty()
        && report.components.iter().all(|component| {
            !component.domain.trim().is_empty()
                && !component.schema.trim().is_empty()
                && component.status == "exact"
                && component.first_difference.is_none()
        });
    if report.schema != "sts-paired-root-constructed-combat-report-v1"
        || report.status != "ready"
        || report.case_id != case_id
        || report.scenario_id != scenario_id
        || report.recipe_program_id != "fresh_neow_to_combat_root.v1"
        || report.source_session.trim().is_empty()
        || report.source_archive_node_id.trim().is_empty()
        || !Path::new(&report.scenario_plan_path).is_file()
        || !Path::new(&report.scenario_report_path).is_file()
        || !Path::new(&report.java_frame_path).is_file()
        || !exact_components
        || report.components.len() != 9
        || !report.gaps.is_empty()
        || report.java_cold_recovery != "unsupported"
        || !report
            .root_id
            .as_deref()
            .is_some_and(|value| content_id_has_shape(value, "state-root-v2"))
        || !report
            .activation_id
            .as_deref()
            .is_some_and(|value| content_id_has_shape(value, "state-root-activation-v2"))
        || !report
            .action_space_id
            .as_deref()
            .is_some_and(|value| content_id_has_shape(value, "paired-action-space-v1"))
        || !report
            .java_runtime_bundle
            .as_ref()
            .is_some_and(|bundle| content_id_has_shape(&bundle.bundle_id, "paired-bundle-v1"))
        || report.java_runtime_entrypoint.as_deref() != Some("scenario_recipe")
        || !is_sha256(&report.source_head_guard.before_sha256)
        || !is_sha256(&report.source_head_guard.after_sha256)
        || report.source_head_guard.before_sha256 != report.source_head_guard.after_sha256
        || report.source_head_guard.file_count == 0
        || !report.source_head_guard.unchanged
    {
        return Err("constructed combat adapter report failed its typed scope checks".to_owned());
    }
    Ok(())
}

fn validate_parentless_neow_adapter_report(
    report: &ParentlessNaturalNeowAdapterReportV1,
    seed: u64,
    ascension: u8,
) -> Result<(), String> {
    let exact = |components: &[BootstrapProjectionSummaryV1]| {
        !components.is_empty()
            && components.iter().all(|component| {
                !component.domain.trim().is_empty()
                    && !component.schema.trim().is_empty()
                    && component.status == "exact"
                    && component.first_difference.is_none()
            })
    };
    if report.schema != "sts-state-root-parentless-neow-report-v2"
        || report.status != "ready"
        || report.seed != seed.to_string()
        || report.ascension != ascension
        || !matches!(report.neow_offer.as_str(), "mini" | "rich")
        || report.source_session.trim().is_empty()
        || report.source_archive_node_id.trim().is_empty()
        || report.java_frame_path.trim().is_empty()
        || !Path::new(&report.java_frame_path).is_file()
        || !exact(&report.components)
        || report.java_runtime_entrypoint != "native_checkpoint_continue_then_neow_intro"
        || report.recovery_certificate != "pending"
        || !content_id_has_shape(&report.java_runtime_bundle.bundle_id, "paired-bundle-v1")
        || !content_id_has_shape(&report.root_id, "state-root-v2")
        || !content_id_has_shape(&report.activation_id, "state-root-activation-v2")
        || !content_id_has_shape(&report.action_space_id, "paired-action-space-v1")
        || !is_sha256(&report.source_head_guard.before_sha256)
        || !is_sha256(&report.source_head_guard.after_sha256)
        || report.source_head_guard.before_sha256 != report.source_head_guard.after_sha256
        || report.source_head_guard.file_count == 0
        || !report.source_head_guard.unchanged
    {
        return Err("parentless Neow adapter report failed its typed scope checks".to_owned());
    }
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn resolve_base_mod(game_directory: &Path) -> Result<PathBuf, String> {
    let local = game_directory.join("mods").join("BaseMod.jar");
    if local.is_file() {
        return Ok(local);
    }
    let workshop = Path::new(WORKSHOP_DIRECTORY);
    let mut matches = Vec::new();
    for entry in fs::read_dir(workshop)
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

fn file_sha256(path: &Path) -> Result<String, String> {
    file_sha256_impl(path, false)
}

fn file_sha256_upper(path: &Path) -> Result<String, String> {
    file_sha256_impl(path, true)
}

fn file_sha256_impl(path: &Path, uppercase: bool) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|error| format!("failed to hash {}: {error}", path.display()))?;
    let digest = hasher.finalize();
    Ok(if uppercase {
        format!("{digest:X}")
    } else {
        format!("{digest:x}")
    })
}

fn read_properties(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let encoded = fs::read_to_string(path)
        .map_err(|error| format!("failed to read properties {}: {error}", path.display()))?;
    let mut properties = BTreeMap::new();
    for line in encoded.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            properties.insert(name.trim().to_owned(), value.trim().to_owned());
        }
    }
    Ok(properties)
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
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect seed entry: {error}"))?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir(&target)
                .map_err(|error| format!("failed to create seed subdirectory: {error}"))?;
            copy_directory_contents(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)
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
    let mut command = Command::new("cmd.exe");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("/D")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target);
    let output = run_bounded(command, Duration::from_secs(10))?;
    if !output.status.success() || !link.is_dir() {
        return Err(format!(
            "failed to create managed junction {} -> {}: {}",
            link.display(),
            target.display(),
            crate::subprocess::failure_detail(&output.stdout, &output.stderr)
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn create_junction(_target: &Path, _link: &Path) -> Result<(), String> {
    Err("Java workbench runtime currently requires Windows junctions".to_owned())
}

fn run_play_executable(repository: &Path) -> PathBuf {
    repository
        .join("target")
        .join("dev-opt")
        .join(if cfg!(windows) {
            "run_play.exe"
        } else {
            "run_play"
        })
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

pub(super) fn create_operation_directory(
    repository: &Path,
    operation: &str,
    request_count: u64,
) -> Result<PathBuf, String> {
    let parent = repository
        .join(".oracle-lab")
        .join("workbench-runtime")
        .join("attempts");
    fs::create_dir_all(&parent)
        .map_err(|error| format!("failed to create workbench attempt root: {error}"))?;
    for collision in 0..16_u8 {
        let path = parent.join(format!(
            "{operation}-{}-{request_count}-{}-{collision}",
            std::process::id(),
            now_unix_ms()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create workbench attempt {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Err("failed to allocate create-new workbench attempt".to_owned())
}

pub(super) fn create_preview_directory(
    repository: &Path,
    request_count: u64,
) -> Result<PathBuf, String> {
    create_operation_directory(repository, "preview", request_count)
}

pub(super) fn run_typed_adapter(
    repository: &Path,
    attempt: &Path,
    label: &str,
    arguments: &[String],
    timeout: Duration,
) -> Result<(), TypedAdapterFailureV1> {
    let failure_report = attempt.join(format!("{label}.failure.json"));
    if failure_report.exists() {
        return Err(TypedAdapterFailureV1::new(
            RUN_PLAY_ADAPTER_PROTOCOL_INVALID_CODE,
            "typed adapter failure report path was not create-new",
        ));
    }
    let mut command = Command::new(run_play_executable(repository));
    command
        .current_dir(repository)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(arguments)
        .arg("--adapter-failure-report")
        .arg(&failure_report);
    let output = run_bounded(command, timeout)?;
    persist_subprocess_output(attempt, label, &output)?;
    if output.status.success() {
        if failure_report.exists() {
            return Err(TypedAdapterFailureV1::new(
                RUN_PLAY_ADAPTER_PROTOCOL_INVALID_CODE,
                "typed adapter succeeded after publishing a failure report",
            ));
        }
        return Ok(());
    }
    match read_typed_adapter_failure(&failure_report) {
        Ok(failure) => Err(failure),
        Err(report_error) => Err(TypedAdapterFailureV1::new(
            RUN_PLAY_ADAPTER_FAILED_CODE,
            format!(
                "typed adapter {label} exited with {}; typed failure report was unavailable: {report_error}; process detail: {}",
                output.status,
                crate::subprocess::failure_detail(&output.stdout, &output.stderr)
            ),
        )),
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct AdapterFailureReportV1 {
    schema: String,
    code: String,
    detail: String,
    #[serde(default)]
    details: Option<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TypedAdapterFailureV1 {
    pub code: String,
    pub detail: String,
}

impl TypedAdapterFailureV1 {
    fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
        }
    }
}

impl From<String> for TypedAdapterFailureV1 {
    fn from(detail: String) -> Self {
        Self::new(RUN_PLAY_ADAPTER_FAILED_CODE, detail)
    }
}

impl From<TypedAdapterFailureV1> for String {
    fn from(failure: TypedAdapterFailureV1) -> Self {
        failure.detail
    }
}

fn read_typed_adapter_failure(path: &Path) -> Result<TypedAdapterFailureV1, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read typed adapter failure report: {error}"))?;
    let report: AdapterFailureReportV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode typed adapter failure report: {error}"))?;
    if report.schema != RUN_PLAY_ADAPTER_FAILURE_SCHEMA_V1
        || !valid_adapter_failure_code(&report.code)
        || report.detail.trim().is_empty()
        || report.detail.len() > 16_384
    {
        return Err("typed adapter failure report violated its closed shape".to_owned());
    }
    let _ = report.details;
    Ok(TypedAdapterFailureV1::new(report.code, report.detail))
}

fn valid_adapter_failure_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn persist_subprocess_output(
    attempt: &Path,
    label: &str,
    output: &std::process::Output,
) -> Result<(), String> {
    fs::write(attempt.join(format!("{label}.stdout.log")), &output.stdout)
        .map_err(|error| format!("failed to store {label} stdout: {error}"))?;
    fs::write(attempt.join(format!("{label}.stderr.log")), &output.stderr)
        .map_err(|error| format!("failed to store {label} stderr: {error}"))?;
    Ok(())
}

pub(super) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let encoded = fs::read(path)
        .map_err(|error| format!("failed to read typed report {}: {error}", path.display()))?;
    serde_json::from_slice(&encoded)
        .map_err(|error| format!("failed to decode typed report {}: {error}", path.display()))
}

fn write_json_new(path: &Path, value: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode create-new JSON: {error}"))?;
    write_bytes_new(path, &encoded)
}

fn write_bytes_new(path: &Path, encoded: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    file.write_all(encoded)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        env::current_dir()
            .map(|directory| directory.join(path))
            .map_err(|error| format!("failed to resolve current directory: {error}"))
    }
}

fn java_8_compatible_path(path: &Path) -> Result<String, String> {
    let absolute = absolute_path(path)?;
    Ok(normalize_java_8_windows_path(&absolute.to_string_lossy()))
}

#[derive(Debug, Eq, PartialEq)]
struct Java8ProcessBoundaryV1 {
    mod_the_spire_argument: String,
    working_directory: String,
    launch_plan_environment: String,
}

impl Java8ProcessBoundaryV1 {
    fn new(
        mod_the_spire: &Path,
        working_directory: &Path,
        launch_plan: &Path,
    ) -> Result<Self, String> {
        Ok(Self {
            mod_the_spire_argument: java_8_compatible_path(mod_the_spire)?,
            working_directory: java_8_compatible_path(working_directory)?,
            launch_plan_environment: java_8_compatible_path(launch_plan)?,
        })
    }
}

fn normalize_java_8_windows_path(rendered: &str) -> String {
    if let Some(tail) = rendered.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{tail}")
    } else if let Some(tail) = rendered.strip_prefix(r"\\?\") {
        tail.to_owned()
    } else {
        rendered.to_owned()
    }
}

fn runtime_session_binding(live: &LiveStatus) -> Result<RuntimeSessionBindingV1, String> {
    let runtime_epoch = format!(
        "{}:{}:{}",
        live.manifest.producer_session_id, live.manifest.connection_id, live.manifest.capture_id
    );
    let protocol_version = live.frame["protocol"]["minor"]
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| "managed Java frame omitted its protocol minor identity".to_owned())?;
    RuntimeSessionBindingV1::new(
        runtime_epoch,
        &live.manifest.session_id,
        &live.manifest.connection_id,
        live.manifest.process_id,
        &live.manifest.sts_jar_sha256,
        &live.manifest.loaded_mod_jar_sha256,
        protocol_version,
    )
    .map_err(|error| error.to_string())
}

fn runtime_frame_token(live: &LiveStatus) -> Result<String, String> {
    live.frame["frame_id"]
        .as_u64()
        .map(|frame_id| frame_id.to_string())
        .ok_or_else(|| "managed Java frame omitted its numeric identity".to_owned())
}

fn content_id_has_shape(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|tail| tail.strip_prefix('-'))
        .is_some_and(|tail| tail.len() == 64 && tail.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
    fn typed_adapter_failure_preserves_code_without_parsing_process_text() {
        let directory = env::temp_dir().join(format!(
            "sts-workbench-adapter-failure-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir(&directory).unwrap();
        let report = directory.join("failure.json");
        fs::write(
            &report,
            serde_json::to_vec(&json!({
                "schema": RUN_PLAY_ADAPTER_FAILURE_SCHEMA_V1,
                "code": "persistent_master_deck_diverged",
                "detail": "private Java/Rust deck comparison",
            }))
            .unwrap(),
        )
        .unwrap();

        let failure = read_typed_adapter_failure(&report).unwrap();

        assert_eq!(failure.code, "persistent_master_deck_diverged");
        assert_eq!(failure.detail, "private Java/Rust deck comparison");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_adapter_failure_is_rejected_instead_of_guessed() {
        let directory = env::temp_dir().join(format!(
            "sts-workbench-adapter-failure-invalid-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir(&directory).unwrap();
        let report = directory.join("failure.json");
        fs::write(
            &report,
            br#"{"schema":"sts-run-play-adapter-failure-v1","code":"bad code","detail":"private"}"#,
        )
        .unwrap();

        assert!(read_typed_adapter_failure(&report).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn native_materialization_decodes_the_producer_owned_certificate_field() {
        let anchor_id = format!("state-root-recovery-certificate-v2-{}", "a".repeat(64));
        let report: NativeMaterializationV1 = serde_json::from_value(serde_json::json!({
            "schema": MATERIALIZATION_SCHEMA,
            "requested_target_root_id": format!("state-root-v2-{}", "b".repeat(64)),
            "native_root_id": format!("state-root-v2-{}", "c".repeat(64)),
            "recovery_certificate_id": anchor_id,
            "java_bundle_id": format!("paired-bundle-v1-{}", "d".repeat(64)),
            "content_profile": "all_unlocked",
            "java_entrypoint": sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1,
            "materialized_directory": "materialized",
            "storage_directory": "storage",
            "file_count": 1,
            "recovery_steps": 1,
        }))
        .unwrap();

        assert_eq!(
            report.recovery_certificate_id.as_deref(),
            Some(anchor_id.as_str())
        );
    }

    #[test]
    fn canonical_fresh_profile_precompletes_every_original_tip_tracker_flag() {
        let tips: BTreeMap<String, String> =
            serde_json::from_slice(CANONICAL_STS_TIPS).expect("canonical STSTips is valid JSON");
        let expected = [
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
        ];

        assert_eq!(tips.len(), expected.len());
        for key in expected {
            assert_eq!(tips.get(key).map(String::as_str), Some("true"), "{key}");
        }
    }

    #[test]
    fn java_build_preflight_rejects_version_drift_before_launch() {
        let versions = vec![
            sts_paired_root::VersionIdentityV1 {
                component: "communicationmod_jar".to_owned(),
                identity: "old-mod".to_owned(),
            },
            sts_paired_root::VersionIdentityV1 {
                component: "slay_the_spire_jar".to_owned(),
                identity: "game".to_owned(),
            },
        ];

        let failure = validate_java_build_versions(&versions, "new-mod", "game").unwrap_err();

        assert_eq!(failure.code, PAIRED_ROOT_VERSION_DRIFT_CODE);
        assert!(failure.detail.contains("communicationmod_jar"));
    }

    #[test]
    fn java_build_preflight_accepts_case_insensitive_hashes() {
        let versions = vec![
            sts_paired_root::VersionIdentityV1 {
                component: "communicationmod_jar".to_owned(),
                identity: "ABCDEF".to_owned(),
            },
            sts_paired_root::VersionIdentityV1 {
                component: "slay_the_spire_jar".to_owned(),
                identity: "123ABC".to_owned(),
            },
        ];

        validate_java_build_versions(&versions, "abcdef", "123abc").unwrap();
    }

    #[test]
    fn java_8_launch_paths_drop_only_the_windows_verbatim_prefix() {
        assert_eq!(
            normalize_java_8_windows_path(r"\\?\D:\work bench\launch-plan.json"),
            r"D:\work bench\launch-plan.json"
        );
        assert_eq!(
            normalize_java_8_windows_path(r"\\?\UNC\server\share\bridge.exe"),
            r"\\server\share\bridge.exe"
        );
        assert_eq!(
            normalize_java_8_windows_path(r"D:\work bench\launch-plan.json"),
            r"D:\work bench\launch-plan.json"
        );
    }

    #[cfg(windows)]
    #[test]
    fn managed_java_process_boundary_normalizes_mod_the_spire_code_source_input() {
        let boundary = Java8ProcessBoundaryV1::new(
            Path::new(r"\\?\D:\work bench\lib\ModTheSpire.jar"),
            Path::new(r"\\?\C:\game data\isolated storage"),
            Path::new(r"\\?\UNC\server\share\launch plan.json"),
        )
        .unwrap();

        assert_eq!(
            boundary,
            Java8ProcessBoundaryV1 {
                mod_the_spire_argument: r"D:\work bench\lib\ModTheSpire.jar".to_owned(),
                working_directory: r"C:\game data\isolated storage".to_owned(),
                launch_plan_environment: r"\\server\share\launch plan.json".to_owned(),
            }
        );
    }
}
