#![cfg(windows)]

use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use sts_java_parity_runtime::{JavaParityRuntimeErrorV1, JavaParityRuntimeOwnerV1};
use sts_workbench_supervisor::{
    ExistingProcessCandidateV1, OwnedStorageRegistryV1, RepositoryLifecycleOwnerV1,
    RuntimeSessionBindingV1, SupervisedChildV1, SupervisedProcessSetV1, SupervisedResourcesV1,
    SupervisedRuntimeErrorV1, SupervisedRuntimeV1,
};
use uuid::Uuid;

const PROTOCOL_MINOR: u16 = 56;

struct OwnedRuntimeFixture {
    base: PathBuf,
    storage_root: PathBuf,
    launch_plan: PathBuf,
    live_output: PathBuf,
    session_manifest: PathBuf,
    companion: Child,
    supervised: Option<SupervisedRuntimeV1>,
}

impl OwnedRuntimeFixture {
    fn create() -> Self {
        let base = std::env::temp_dir().join(format!(
            "sts-java-parity-runtime-{}",
            Uuid::new_v4().simple()
        ));
        let repository = base.join("repository");
        let game_directory = base.join("game");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&game_directory).unwrap();

        let lifecycle = RepositoryLifecycleOwnerV1::acquire(&repository).unwrap();
        let registry = OwnedStorageRegistryV1::new(&lifecycle, &game_directory);
        let storage = registry.create().unwrap();
        let storage_root = storage.root().to_path_buf();

        let executable = PathBuf::from(std::env::var_os("COMSPEC").unwrap());
        let primary = spawn_waiting_process(&executable);
        let companion = spawn_waiting_process(&executable);
        let companion_process_id = companion.id();
        let admitted = ExistingProcessCandidateV1::open_running(companion_process_id, &executable)
            .unwrap()
            .admit();
        let mut processes = SupervisedProcessSetV1::new(SupervisedChildV1::adopt(primary));
        processes.admit_companion(admitted).unwrap();
        let resources = SupervisedResourcesV1::new(processes, storage);

        let launch_plan = base.join("launch-plan.json");
        fs::write(&launch_plan, b"{}\n").unwrap();
        let live_output = base.join("live");
        let session_directory = live_output.join("session-test");
        let frames = session_directory.join("frames");
        fs::create_dir_all(&frames).unwrap();
        fs::create_dir(session_directory.join("requests")).unwrap();
        fs::create_dir(session_directory.join("receipts")).unwrap();

        let session_id = Uuid::new_v4().to_string();
        let producer_session_id = Uuid::new_v4().to_string();
        let connection_id = Uuid::new_v4().to_string();
        let capture_id = Uuid::new_v4().to_string();
        let session_manifest = session_directory.join("session.json");
        write_json(
            &session_manifest,
            &json!({
                "schema": "sts_java_parity_live_session_v1",
                "session_id": session_id,
                "created_at_unix_ms": 1,
                "process_id": companion_process_id,
                "session_dir": session_directory,
                "capture_id": capture_id,
                "producer_session_id": producer_session_id,
                "connection_id": connection_id,
                "profile": "parity_run_control",
                "source_commit": "0".repeat(40),
                "source_dirty": false,
                "loaded_mod_jar_sha256": "A".repeat(64),
                "sts_jar_sha256": "B".repeat(64),
                "mod_the_spire_jar_sha256": "C".repeat(64),
                "launch": {
                    "profile": "accelerated",
                    "control_profile": "parity_run_control",
                    "plan_path": launch_plan,
                    "plan_sha256": "D".repeat(64),
                    "storage_class": "isolated",
                    "working_directory": storage_root,
                    "source_game_directory": game_directory,
                    "profile_substrate": "canonical_tutorial_complete_preferences_v1",
                    "tutorial_policy": "precompleted_via_profile_v1",
                    "isolation_id": null
                }
            }),
        );
        write_json(
            &frames.join("frame-00000000000000000001.json"),
            &json!({
                "frame_id": 1,
                "protocol": {"minor": PROTOCOL_MINOR},
                "action_space": {"actions": []}
            }),
        );

        let binding = RuntimeSessionBindingV1::new(
            format!("{producer_session_id}:{connection_id}:{capture_id}"),
            session_id,
            connection_id,
            companion_process_id,
            "B".repeat(64),
            "A".repeat(64),
            PROTOCOL_MINOR,
        )
        .unwrap();
        let supervised = SupervisedRuntimeV1::new(resources, binding).unwrap();

        drop(registry);
        drop(lifecycle);
        Self {
            base,
            storage_root,
            launch_plan,
            live_output,
            session_manifest,
            companion,
            supervised: Some(supervised),
        }
    }

    fn owner(&mut self) -> JavaParityRuntimeOwnerV1 {
        JavaParityRuntimeOwnerV1::new(
            self.supervised.take().unwrap(),
            &self.launch_plan,
            &self.live_output,
        )
        .unwrap()
    }
}

impl Drop for OwnedRuntimeFixture {
    fn drop(&mut self) {
        drop(self.supervised.take());
        let _ = self.companion.wait();
        if self.base.exists() {
            fs::remove_dir_all(&self.base).unwrap();
        }
    }
}

#[test]
fn owner_revalidates_the_exact_live_epoch_and_exposes_no_workbench_state() {
    let mut fixture = OwnedRuntimeFixture::create();
    let mut owner = fixture.owner();

    let status = owner.status().unwrap();
    assert_eq!(status.live().frame["frame_id"], 1);
    assert_eq!(status.runtime_identity().protocol_version, PROTOCOL_MINOR);
    assert_eq!(owner.launch_plan(), fixture.launch_plan);
    assert_eq!(owner.live_output(), fixture.live_output);

    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.session_manifest).unwrap()).unwrap();
    manifest["capture_id"] = json!(Uuid::new_v4().to_string());
    write_json(&fixture.session_manifest, &manifest);
    assert!(matches!(
        owner.status(),
        Err(JavaParityRuntimeErrorV1::Supervised(
            SupervisedRuntimeErrorV1::SessionIdentityMismatch
        ))
    ));
}

#[test]
fn rejected_handle_does_not_close_or_replace_the_owned_session() {
    let mut fixture = OwnedRuntimeFixture::create();
    let mut owner = fixture.owner();

    let error = owner.act("f1:a0", Duration::ZERO).unwrap_err();
    assert!(matches!(error, JavaParityRuntimeErrorV1::LiveAction(_)));
    assert_eq!(owner.status().unwrap().live().frame["frame_id"], 1);
}

#[test]
fn explicit_shutdown_is_idempotent_and_cleans_owned_storage() {
    let mut fixture = OwnedRuntimeFixture::create();
    let mut owner = fixture.owner();

    owner.shutdown().unwrap();
    owner.shutdown().unwrap();
    assert!(owner.is_closed());
    assert!(!fixture.storage_root.exists());
    wait_for_child_exit(&mut fixture.companion);
    assert!(matches!(
        owner.status(),
        Err(JavaParityRuntimeErrorV1::Closed)
    ));
}

#[test]
fn drop_closes_processes_and_storage_without_an_explicit_shutdown() {
    let mut fixture = OwnedRuntimeFixture::create();
    let owner = fixture.owner();
    drop(owner);

    assert!(!fixture.storage_root.exists());
    wait_for_child_exit(&mut fixture.companion);
}

fn spawn_waiting_process(executable: &Path) -> Child {
    Command::new(executable)
        .args(["/D", "/C", "ping", "-n", "30", "127.0.0.1"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn write_json(path: &Path, value: &serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn wait_for_child_exit(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child.try_wait().unwrap().is_some());
}
