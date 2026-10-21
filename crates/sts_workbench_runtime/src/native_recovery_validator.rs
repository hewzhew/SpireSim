use crate::runtime_manager::BootstrapJavaRuntimeV1;
use std::fmt;
use std::path::Path;
use sts_java_parity_live::LiveStatus;
use sts_java_parity_projection::{
    validate_natural_run_decision_boundary, NativeRecoveryObservationScopeV1,
    NativeRecoveryObservationStoreV1,
};
use sts_paired_root::VersionIdentityV1;
use sts_workbench_supervisor::{
    NativeRecoveryValidatorOpenV1, NativeRecoveryValidatorV1, OwnedStorageRegistryV1,
};

trait DisposableRecoveryRuntimeV1: fmt::Debug {
    fn continue_from_native_storage(&mut self) -> Result<LiveStatus, String>;
    fn shutdown(&mut self) -> Result<(), String>;
}

impl DisposableRecoveryRuntimeV1 for BootstrapJavaRuntimeV1 {
    fn continue_from_native_storage(&mut self) -> Result<LiveStatus, String> {
        BootstrapJavaRuntimeV1::continue_from_native_storage(self)
    }

    fn shutdown(&mut self) -> Result<(), String> {
        BootstrapJavaRuntimeV1::shutdown(self)
    }
}

#[derive(Debug)]
struct DisposableNativeRecoveryValidatorV1<R> {
    runtime: R,
}

impl<R: DisposableRecoveryRuntimeV1> NativeRecoveryValidatorV1
    for DisposableNativeRecoveryValidatorV1<R>
{
    fn shutdown(&mut self) -> Result<(), String> {
        self.runtime.shutdown()
    }
}

pub(super) fn open_native_recovery_validator(
    repository: &Path,
    recovery_storage: &Path,
    storage_registry: &OwnedStorageRegistryV1,
    scope: NativeRecoveryObservationScopeV1,
) -> Result<NativeRecoveryValidatorOpenV1, String> {
    let (runtime, _) =
        BootstrapJavaRuntimeV1::open_recovery(repository, recovery_storage, storage_registry)?;
    finish_native_recovery_validator(repository, scope, runtime)
}

fn finish_native_recovery_validator<R: DisposableRecoveryRuntimeV1 + 'static>(
    repository: &Path,
    scope: NativeRecoveryObservationScopeV1,
    mut runtime: R,
) -> Result<NativeRecoveryValidatorOpenV1, String> {
    let observation = match (|| {
        let live = runtime.continue_from_native_storage()?;
        validate_natural_run_decision_boundary(&live.frame)?;
        NativeRecoveryObservationStoreV1::new(repository)?.publish(
            scope,
            java_versions(&live),
            live.frame,
        )
    })() {
        Ok(observation) => observation,
        Err(error) => {
            return match runtime.shutdown() {
                Ok(()) => Err(error),
                Err(shutdown) => Err(format!(
                    "{error}; disposable native recovery validator shutdown also failed: {shutdown}"
                )),
            };
        }
    };
    Ok(NativeRecoveryValidatorOpenV1::new(
        DisposableNativeRecoveryValidatorV1 { runtime },
        observation.observation_ref,
    ))
}

fn java_versions(live: &LiveStatus) -> Vec<VersionIdentityV1> {
    vec![
        VersionIdentityV1 {
            component: "communicationmod_jar".to_owned(),
            identity: live.manifest.loaded_mod_jar_sha256.clone(),
        },
        VersionIdentityV1 {
            component: "communicationmod_source".to_owned(),
            identity: live.manifest.source_commit.clone(),
        },
        VersionIdentityV1 {
            component: "slay_the_spire_jar".to_owned(),
            identity: live.manifest.sts_jar_sha256.clone(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;
    use std::fs;
    use std::rc::Rc;
    use sts_java_parity_live::SessionManifest;
    use sts_java_parity_protocol::{
        FRAME_SCHEMA_ID, PROTOCOL_MAJOR, PROTOCOL_MINOR, PROTOCOL_NAME, RUN_CONTROL_PROFILE,
    };
    use uuid::Uuid;

    #[derive(Debug)]
    struct FakeRecoveryRuntimeV1 {
        live: Option<LiveStatus>,
        continues: Rc<Cell<usize>>,
        shutdowns: Rc<Cell<usize>>,
        drops: Rc<Cell<usize>>,
        shutdown_error: Option<&'static str>,
    }

    impl DisposableRecoveryRuntimeV1 for FakeRecoveryRuntimeV1 {
        fn continue_from_native_storage(&mut self) -> Result<LiveStatus, String> {
            self.continues.set(self.continues.get() + 1);
            self.live
                .take()
                .ok_or_else(|| "fake recovery runtime was already consumed".to_owned())
        }

        fn shutdown(&mut self) -> Result<(), String> {
            self.shutdowns.set(self.shutdowns.get() + 1);
            self.shutdown_error
                .map_or(Ok(()), |error| Err(error.to_owned()))
        }
    }

    impl Drop for FakeRecoveryRuntimeV1 {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }

    #[test]
    fn offline_validator_publishes_one_job_scoped_natural_map_observation() {
        let repository = temporary_repository("publish");
        let scope = NativeRecoveryObservationScopeV1::new(7, "job-1").unwrap();
        let continues = Rc::new(Cell::new(0));
        let shutdowns = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let runtime = fake_runtime(natural_map_live(), &continues, &shutdowns, &drops);

        let open = finish_native_recovery_validator(&repository, scope.clone(), runtime).unwrap();
        assert_eq!(continues.get(), 1);
        assert_eq!(shutdowns.get(), 0);

        let observation_directory =
            repository.join(".oracle-lab/workbench-service/native-recovery-observations");
        let files = fs::read_dir(&observation_directory)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(files.len(), 1);
        let observation_ref = files[0]
            .path()
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let observed = NativeRecoveryObservationStoreV1::new(&repository)
            .unwrap()
            .load(&observation_ref, &scope)
            .unwrap();
        assert_eq!(
            observed
                .observation
                .java_versions
                .iter()
                .map(|version| (version.component.as_str(), version.identity.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("communicationmod_jar", "mod-jar"),
                ("communicationmod_source", "mod-source"),
                ("slay_the_spire_jar", "game-jar"),
            ]
        );

        drop(open);
        assert_eq!(drops.get(), 1);
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn offline_validator_is_scene_neutral_for_natural_run_decisions() {
        let repository = temporary_repository("scene-neutral");
        let scope = NativeRecoveryObservationScopeV1::new(7, "job-neow").unwrap();
        let continues = Rc::new(Cell::new(0));
        let shutdowns = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let mut live = natural_map_live();
        live.frame["boundary"]["evidence"] = json!({
            "screen_type": "EVENT",
            "room_phase": "EVENT",
        });
        live.frame["public_base"]["decision"] = json!({
            "kind": "event",
            "event_id": "Neow",
            "event_screen_index": 99,
        });
        let runtime = fake_runtime(live, &continues, &shutdowns, &drops);

        let open = finish_native_recovery_validator(&repository, scope, runtime).unwrap();
        assert_eq!(continues.get(), 1);
        assert_eq!(shutdowns.get(), 0);
        drop(open);
        assert_eq!(drops.get(), 1);
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn offline_validator_rejects_scenario_map_before_publishing() {
        let repository = temporary_repository("scenario");
        let scope = NativeRecoveryObservationScopeV1::new(7, "job-2").unwrap();
        let continues = Rc::new(Cell::new(0));
        let shutdowns = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let mut live = natural_map_live();
        live.frame["scenario"] = json!({"scenario_id": "not-natural"});
        let runtime = fake_runtime(live, &continues, &shutdowns, &drops);

        let error = finish_native_recovery_validator(&repository, scope, runtime).unwrap_err();
        assert!(error.contains("not one stable natural run decision"));
        assert_eq!(continues.get(), 1);
        assert_eq!(shutdowns.get(), 1);
        assert_eq!(drops.get(), 1);
        assert!(!repository
            .join(".oracle-lab/workbench-service/native-recovery-observations")
            .exists());
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn validator_setup_reports_both_observation_and_shutdown_failure() {
        let repository = temporary_repository("shutdown-failure");
        let scope = NativeRecoveryObservationScopeV1::new(7, "job-3").unwrap();
        let continues = Rc::new(Cell::new(0));
        let shutdowns = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let mut live = natural_map_live();
        live.frame["scenario"] = json!({"scenario_id": "not-natural"});
        let mut runtime = fake_runtime(live, &continues, &shutdowns, &drops);
        runtime.shutdown_error = Some("fake shutdown failed");

        let error = finish_native_recovery_validator(&repository, scope, runtime).unwrap_err();
        assert!(error.contains("not one stable natural run decision"));
        assert!(error.contains("validator shutdown also failed: fake shutdown failed"));
        assert_eq!(continues.get(), 1);
        assert_eq!(shutdowns.get(), 1);
        assert_eq!(drops.get(), 1);
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn validator_shutdown_is_delegated_to_the_disposable_runtime() {
        let continues = Rc::new(Cell::new(0));
        let shutdowns = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let runtime = fake_runtime(natural_map_live(), &continues, &shutdowns, &drops);
        let mut validator = DisposableNativeRecoveryValidatorV1 { runtime };

        NativeRecoveryValidatorV1::shutdown(&mut validator).unwrap();

        assert_eq!(shutdowns.get(), 1);
        drop(validator);
        assert_eq!(drops.get(), 1);
    }

    fn temporary_repository(label: &str) -> std::path::PathBuf {
        let repository = std::env::temp_dir().join(format!(
            "sts-native-recovery-validator-{label}-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&repository).unwrap();
        repository
    }

    fn fake_runtime(
        live: LiveStatus,
        continues: &Rc<Cell<usize>>,
        shutdowns: &Rc<Cell<usize>>,
        drops: &Rc<Cell<usize>>,
    ) -> FakeRecoveryRuntimeV1 {
        FakeRecoveryRuntimeV1 {
            live: Some(live),
            continues: Rc::clone(continues),
            shutdowns: Rc::clone(shutdowns),
            drops: Rc::clone(drops),
            shutdown_error: None,
        }
    }

    fn natural_map_live() -> LiveStatus {
        LiveStatus {
            manifest: SessionManifest {
                schema: "session-manifest-v1".to_owned(),
                session_id: "session-1".to_owned(),
                created_at_unix_ms: 1,
                process_id: 1,
                session_dir: "session".to_owned(),
                capture_id: "capture-1".to_owned(),
                producer_session_id: "producer-1".to_owned(),
                connection_id: "connection-1".to_owned(),
                profile: RUN_CONTROL_PROFILE.to_owned(),
                source_commit: "mod-source".to_owned(),
                source_dirty: false,
                loaded_mod_jar_sha256: "mod-jar".to_owned(),
                sts_jar_sha256: "game-jar".to_owned(),
                mod_the_spire_jar_sha256: "mts-jar".to_owned(),
                launch: None,
            },
            frame_path: "frame.json".to_owned(),
            frame: json!({
                "kind": "state_frame",
                "protocol": {
                    "name": PROTOCOL_NAME,
                    "major": PROTOCOL_MAJOR,
                    "minor": PROTOCOL_MINOR,
                    "schema_id": FRAME_SCHEMA_ID,
                },
                "producer_session_id": "producer-1",
                "connection_id": "connection-1",
                "capture_id": "capture-1",
                "frame_id": 1,
                "profile": RUN_CONTROL_PROFILE,
                "scenario": null,
                "boundary": {
                    "stable": true,
                    "kind": "run_decision",
                    "evidence": {
                        "screen_type": "MAP",
                        "room_phase": "COMPLETE",
                    }
                },
                "public_base": {"decision": {"kind": "map"}},
            }),
            actions: Vec::new(),
        }
    }
}
