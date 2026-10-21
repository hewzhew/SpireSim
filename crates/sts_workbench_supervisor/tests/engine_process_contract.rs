use serde_json::json;
use std::env;
use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use sts_workbench_supervisor::{
    EngineContractV1, EngineHelloV1, EngineImageStoreV1, EngineImageV1, EngineOperationV1,
    EngineOutcomeV1, EngineProcessErrorV1, EngineProcessV1, EngineRequestV1, EngineResponseV1,
    ExchangeSettlementV1, JavaAfterRotationV1, JavaRuntimeIdentityV1,
    ParkedRevalidationSettlementV1, PhysicalRuntimeCallV1, PhysicalRuntimeOutcomeV1,
    PhysicalRuntimeResultV1, PrepareEngineOutcomeV1, ProcessContainmentV1, RecoverySettlementV1,
    RotationOutcomeV1, SupervisorCallOutcomeV1, SupervisorCallV1, SupervisorModelV1,
    SupervisorPhaseV1, ENGINE_REQUEST_SCHEMA_V1, SUPERVISOR_PROTOCOL_V2,
};

const PROCESS_MODE: &str = "STS_WORKBENCH_SUPERVISOR_TEST_MODE";
const ENGINE_IMAGE: &str = "STS_WORKBENCH_SUPERVISOR_TEST_ENGINE_IMAGE";
const JAVA_PID_PATH: &str = "STS_WORKBENCH_SUPERVISOR_TEST_JAVA_PID";
const ROTATION_MARKER_PATH: &str = "STS_WORKBENCH_SUPERVISOR_TEST_ROTATION_MARKER";
const IO_TIMEOUT: Duration = Duration::from_secs(5);

enum FakePendingSupervisorCall {
    Echo {
        call_id: String,
    },
    NativeRecoveryBegin {
        call_id: String,
    },
    NativeRecoverySeal {
        call_id: String,
        observation_ref: String,
    },
}

impl FakePendingSupervisorCall {
    fn call_id(&self) -> &str {
        match self {
            Self::Echo { call_id }
            | Self::NativeRecoveryBegin { call_id }
            | Self::NativeRecoverySeal { call_id, .. } => call_id,
        }
    }
}

fn main() {
    match env::var(PROCESS_MODE).ok().as_deref() {
        Some("engine") => run_fake_engine(),
        #[cfg(windows)]
        Some("supervisor") => run_fake_supervisor(),
        _ => run_contract(),
    }
}

fn contract(image_id: &str) -> EngineContractV1 {
    EngineContractV1 {
        image_id: image_id.to_owned(),
        supervisor_protocol: SUPERVISOR_PROTOCOL_V2,
        java_build_id: "java-build-1".to_owned(),
        mod_build_id: "mod-build-1".to_owned(),
        minimum_java_protocol: 41,
        maximum_java_protocol: 41,
    }
}

fn engine_command(image_id: &str) -> Command {
    let mut command = Command::new(env::current_exe().expect("current test executable"));
    configure_engine_command(&mut command, image_id);
    command
}

fn configure_engine_command(command: &mut Command, image_id: &str) {
    command
        .env(PROCESS_MODE, "engine")
        .env(ENGINE_IMAGE, image_id)
        .stderr(Stdio::null());
}

fn run_fake_engine() {
    let image_id = env::var(ENGINE_IMAGE).expect("fake Engine image identity");
    let mut stdout = io::stdout().lock();
    write_json(&mut stdout, &EngineHelloV1::new(contract(&image_id)));
    let mut bound_epoch = None;
    let mut pending_supervisor_call = None;
    for line in io::stdin().lock().lines() {
        let line = line.expect("fake Engine request line");
        let request: EngineRequestV1 =
            serde_json::from_str(&line).expect("typed fake Engine request");
        let outcome = if request.schema != ENGINE_REQUEST_SCHEMA_V1 {
            EngineOutcomeV1::Rejected {
                code: "request_schema_mismatch".to_owned(),
            }
        } else {
            match request.operation {
                EngineOperationV1::Bind if bound_epoch.is_none() && request.engine_epoch != 0 => {
                    bound_epoch = Some(request.engine_epoch);
                    EngineOutcomeV1::Bound
                }
                EngineOperationV1::Execute { input }
                    if bound_epoch == Some(request.engine_epoch) =>
                {
                    match input.get("kind").and_then(|value| value.as_str()) {
                        Some("hang") => {
                            std::thread::sleep(Duration::from_secs(60));
                            EngineOutcomeV1::Rejected {
                                code: "hang_finished".to_owned(),
                            }
                        }
                        Some("reject") => EngineOutcomeV1::Rejected {
                            code: "fake_rejected".to_owned(),
                        },
                        Some("reject_engine_domain") => EngineOutcomeV1::Rejected {
                            code: "CurrentManifestUnavailable".to_owned(),
                        },
                        Some("reject_invalid_code") => EngineOutcomeV1::Rejected {
                            code: "invalid code!".to_owned(),
                        },
                        Some("supervisor_roundtrip") if pending_supervisor_call.is_none() => {
                            let call_id = "java-frame-1".to_owned();
                            pending_supervisor_call = Some(FakePendingSupervisorCall::Echo {
                                call_id: call_id.clone(),
                            });
                            EngineOutcomeV1::SupervisorCall {
                                call: SupervisorCallV1 {
                                    call_id,
                                    input: json!({"kind": "observe_fake_java"}),
                                },
                            }
                        }
                        Some("native_recovery_roundtrip") if pending_supervisor_call.is_none() => {
                            let call_id = "native-recovery-begin-1".to_owned();
                            pending_supervisor_call =
                                Some(FakePendingSupervisorCall::NativeRecoveryBegin {
                                    call_id: call_id.clone(),
                                });
                            EngineOutcomeV1::SupervisorCall {
                                call: SupervisorCallV1 {
                                    call_id,
                                    input: serde_json::to_value(
                                        PhysicalRuntimeCallV1::begin_native_recovery_validation(),
                                    )
                                    .expect("native recovery begin call serializes"),
                                },
                            }
                        }
                        _ => EngineOutcomeV1::Completed {
                            output: json!({"echo": input, "engine_image": image_id}),
                        },
                    }
                }
                EngineOperationV1::ResumeSupervisorCall { call_id, outcome }
                    if bound_epoch == Some(request.engine_epoch)
                        && pending_supervisor_call
                            .as_ref()
                            .is_some_and(|pending| pending.call_id() == call_id) =>
                {
                    let pending = pending_supervisor_call
                        .take()
                        .expect("matched pending Supervisor call exists");
                    match (pending, outcome) {
                        (
                            FakePendingSupervisorCall::Echo { .. },
                            SupervisorCallOutcomeV1::Completed { output },
                        ) => EngineOutcomeV1::Completed {
                            output: json!({
                                "supervisor_output": output,
                                "engine_image": image_id,
                            }),
                        },
                        (
                            FakePendingSupervisorCall::NativeRecoveryBegin { .. },
                            SupervisorCallOutcomeV1::Completed { output },
                        ) => {
                            let result = serde_json::from_value::<PhysicalRuntimeResultV1>(output)
                                .expect("typed native recovery begin result");
                            result
                                .validate()
                                .expect("valid native recovery begin result");
                            let PhysicalRuntimeOutcomeV1::NativeRecoveryValidatorReady {
                                observation_ref,
                            } = result.outcome
                            else {
                                panic!("native recovery begin returned another result")
                            };
                            let next_call_id = "native-recovery-seal-1".to_owned();
                            pending_supervisor_call =
                                Some(FakePendingSupervisorCall::NativeRecoverySeal {
                                    call_id: next_call_id.clone(),
                                    observation_ref,
                                });
                            EngineOutcomeV1::SupervisorCall {
                                call: SupervisorCallV1 {
                                    call_id: next_call_id,
                                    input: serde_json::to_value(
                                        PhysicalRuntimeCallV1::seal_native_recovery_validation(),
                                    )
                                    .expect("native recovery seal call serializes"),
                                },
                            }
                        }
                        (
                            FakePendingSupervisorCall::NativeRecoverySeal {
                                observation_ref, ..
                            },
                            SupervisorCallOutcomeV1::Completed { output },
                        ) => {
                            let result = serde_json::from_value::<PhysicalRuntimeResultV1>(output)
                                .expect("typed native recovery seal result");
                            result
                                .validate()
                                .expect("valid native recovery seal result");
                            let PhysicalRuntimeOutcomeV1::NativeRecoverySealed { recovery_ref } =
                                result.outcome
                            else {
                                panic!("native recovery seal returned another result")
                            };
                            EngineOutcomeV1::Completed {
                                output: json!({
                                    "observation_ref": observation_ref,
                                    "recovery_ref": recovery_ref,
                                    "engine_image": image_id,
                                }),
                            }
                        }
                        (_, SupervisorCallOutcomeV1::Rejected { code }) => {
                            EngineOutcomeV1::Rejected { code }
                        }
                    }
                }
                EngineOperationV1::Shutdown
                    if bound_epoch == Some(request.engine_epoch)
                        || (bound_epoch.is_none() && request.engine_epoch == 0) =>
                {
                    write_json(
                        &mut stdout,
                        &EngineResponseV1::new(
                            request.request_id,
                            request.engine_epoch,
                            EngineOutcomeV1::Stopped,
                        ),
                    );
                    return;
                }
                _ => EngineOutcomeV1::Rejected {
                    code: "engine_epoch_mismatch".to_owned(),
                },
            }
        };
        write_json(
            &mut stdout,
            &EngineResponseV1::new(request.request_id, request.engine_epoch, outcome),
        );
    }
}

fn write_json(output: &mut impl Write, value: &impl serde::Serialize) {
    serde_json::to_writer(&mut *output, value).expect("encode fake Engine frame");
    output
        .write_all(b"\n")
        .expect("terminate fake Engine frame");
    output.flush().expect("flush fake Engine frame");
}

#[cfg(not(windows))]
fn attach_engine(
    supervisor: &mut SupervisorModelV1,
    image_id: &str,
) -> (EngineProcessV1, RotationOutcomeV1) {
    let engine_contract = contract(image_id);
    let prepared = match supervisor.prepare_engine(engine_contract).unwrap() {
        PrepareEngineOutcomeV1::Prepared(prepared) => prepared,
        PrepareEngineOutcomeV1::DeferredBusy { .. } => {
            panic!("test Engine preparation was deferred")
        }
    };
    let mut command = engine_command(image_id);
    let mut process =
        EngineProcessV1::spawn(&mut command, prepared.contract().clone(), IO_TIMEOUT).unwrap();
    process.bind(prepared.engine_epoch(), IO_TIMEOUT).unwrap();
    let outcome = supervisor.attach_prepared_engine(prepared).unwrap();
    (process, outcome)
}

fn attach_materialized_engine(
    supervisor: &mut SupervisorModelV1,
    image: &EngineImageV1,
) -> (EngineProcessV1, RotationOutcomeV1) {
    let engine_contract = contract(image.image_id());
    let prepared = match supervisor.prepare_engine(engine_contract).unwrap() {
        PrepareEngineOutcomeV1::Prepared(prepared) => prepared,
        PrepareEngineOutcomeV1::DeferredBusy { .. } => {
            panic!("test Engine preparation was deferred")
        }
    };
    let mut command = image.command(prepared.contract()).unwrap();
    configure_engine_command(&mut command, image.image_id());
    let mut process =
        EngineProcessV1::spawn(&mut command, prepared.contract().clone(), IO_TIMEOUT).unwrap();
    process.bind(prepared.engine_epoch(), IO_TIMEOUT).unwrap();
    let outcome = supervisor.attach_prepared_engine(prepared).unwrap();
    (process, outcome)
}

fn execute_neutral(
    supervisor: &mut SupervisorModelV1,
    process: &mut EngineProcessV1,
    engine_epoch: u64,
    image_id: &str,
) {
    let job_id = format!("job-{engine_epoch}");
    supervisor
        .begin_exchange(engine_epoch, &job_id, None)
        .unwrap();
    let input = json!({"kind": "probe", "engine_epoch": engine_epoch});
    let output = process.execute(input.clone(), IO_TIMEOUT).unwrap();
    assert_eq!(output, json!({"echo": input, "engine_image": image_id}));
    supervisor
        .complete_exchange(engine_epoch, &job_id, ExchangeSettlementV1::Neutral)
        .unwrap();
}

fn assert_engine_owned_domain_rejection_crosses_the_stable_wire() {
    let mut command = engine_command("engine-domain-rejection");
    let mut process = EngineProcessV1::spawn(
        &mut command,
        contract("engine-domain-rejection"),
        IO_TIMEOUT,
    )
    .unwrap();
    process.bind(71, IO_TIMEOUT).unwrap();
    assert!(matches!(
        process.execute(json!({"kind": "reject_engine_domain"}), IO_TIMEOUT),
        Err(EngineProcessErrorV1::Rejected { code })
            if code == "CurrentManifestUnavailable"
    ));
    process.shutdown(IO_TIMEOUT).unwrap();
}

fn assert_unstructured_rejection_code_is_still_fail_closed() {
    let mut command = engine_command("engine-invalid-rejection");
    let mut process = EngineProcessV1::spawn(
        &mut command,
        contract("engine-invalid-rejection"),
        IO_TIMEOUT,
    )
    .unwrap();
    process.bind(72, IO_TIMEOUT).unwrap();
    assert!(matches!(
        process.execute(json!({"kind": "reject_invalid_code"}), IO_TIMEOUT),
        Err(EngineProcessErrorV1::InvalidFrame(_))
    ));
    assert!(matches!(
        process.execute(json!({"kind": "probe"}), IO_TIMEOUT),
        Err(EngineProcessErrorV1::Disconnected)
    ));
}

fn execute_with_supervisor_callback(
    supervisor: &mut SupervisorModelV1,
    process: &mut EngineProcessV1,
    engine_epoch: u64,
    image_id: &str,
) {
    let job_id = format!("callback-job-{engine_epoch}");
    supervisor
        .begin_exchange(engine_epoch, &job_id, None)
        .unwrap();
    let output = process
        .execute_with_supervisor(
            json!({"kind": "supervisor_roundtrip"}),
            IO_TIMEOUT,
            |call| {
                assert_eq!(call.call_id, "java-frame-1");
                assert_eq!(call.input, json!({"kind": "observe_fake_java"}));
                Ok(SupervisorCallOutcomeV1::Completed {
                    output: json!({"frame": "fake-java-frame-1"}),
                })
            },
        )
        .unwrap();
    assert_eq!(output["engine_image"], image_id);
    assert_eq!(output["supervisor_output"]["frame"], "fake-java-frame-1");
    supervisor
        .complete_exchange(engine_epoch, &job_id, ExchangeSettlementV1::Neutral)
        .unwrap();
}

fn assert_contract_mismatch_is_closed() {
    let supervisor = SupervisorModelV1::default();
    let before = supervisor.clone();
    let prepared = match supervisor
        .prepare_engine(contract("engine-expected"))
        .unwrap()
    {
        PrepareEngineOutcomeV1::Prepared(prepared) => prepared,
        PrepareEngineOutcomeV1::DeferredBusy { .. } => panic!("unexpected busy preparation"),
    };
    let mut command = engine_command("engine-actual");
    let result = EngineProcessV1::spawn(&mut command, prepared.contract().clone(), IO_TIMEOUT);
    assert!(matches!(
        result,
        Err(EngineProcessErrorV1::ContractMismatch)
    ));
    assert_eq!(supervisor, before);
}

fn assert_supervisor_call_without_a_handler_poisoned_only_that_engine() {
    let mut command = engine_command("engine-unhandled-call");
    let mut process =
        EngineProcessV1::spawn(&mut command, contract("engine-unhandled-call"), IO_TIMEOUT)
            .unwrap();
    process.bind(74, IO_TIMEOUT).unwrap();
    assert_eq!(
        process.execute(json!({"kind": "supervisor_roundtrip"}), IO_TIMEOUT),
        Err(EngineProcessErrorV1::UnexpectedOutcome)
    );
    assert!(matches!(
        process.execute(json!({"kind": "probe"}), IO_TIMEOUT),
        Err(EngineProcessErrorV1::Disconnected)
    ));
}

fn assert_native_recovery_callbacks_are_ordered_and_clear_pending_state() {
    let mut command = engine_command("engine-native-recovery");
    let mut process =
        EngineProcessV1::spawn(&mut command, contract("engine-native-recovery"), IO_TIMEOUT)
            .unwrap();
    process.bind(75, IO_TIMEOUT).unwrap();
    let observation_ref = format!(
        "workbench-native-recovery-observation-v1-{}",
        "a".repeat(64)
    );
    let recovery_ref = format!("workbench-native-recovery-capsule-v1-{}", "b".repeat(64));
    let mut calls = Vec::new();
    let output = process
        .execute_with_supervisor(
            json!({"kind": "native_recovery_roundtrip"}),
            IO_TIMEOUT,
            |call| {
                let request = serde_json::from_value::<PhysicalRuntimeCallV1>(call.input).unwrap();
                calls.push(request.operation.clone());
                Ok(SupervisorCallOutcomeV1::Completed {
                    output: match request.operation {
                        sts_workbench_supervisor::PhysicalRuntimeOperationV1::BeginNativeRecoveryValidation {} => {
                            serde_json::to_value(
                                PhysicalRuntimeResultV1::native_recovery_validator_ready(
                                    observation_ref.clone(),
                                ),
                            )
                            .unwrap()
                        }
                        sts_workbench_supervisor::PhysicalRuntimeOperationV1::SealNativeRecoveryValidation {} => {
                            serde_json::to_value(
                                PhysicalRuntimeResultV1::native_recovery_sealed(
                                    recovery_ref.clone(),
                                ),
                            )
                            .unwrap()
                        }
                        other => panic!("unexpected native recovery callback {other:?}"),
                    },
                })
            },
        )
        .unwrap();
    assert_eq!(
        calls,
        vec![
            sts_workbench_supervisor::PhysicalRuntimeOperationV1::BeginNativeRecoveryValidation {},
            sts_workbench_supervisor::PhysicalRuntimeOperationV1::SealNativeRecoveryValidation {},
        ]
    );
    assert_eq!(output["observation_ref"], observation_ref);
    assert_eq!(output["recovery_ref"], recovery_ref);

    assert!(matches!(
        process.execute_with_supervisor(
            json!({"kind": "native_recovery_roundtrip"}),
            IO_TIMEOUT,
            |_| Ok(SupervisorCallOutcomeV1::Rejected {
                code: "capture_rejected".to_owned(),
            }),
        ),
        Err(EngineProcessErrorV1::Rejected { code }) if code == "capture_rejected"
    ));
    let probe = json!({"kind": "probe_after_rejected_native_recovery"});
    assert_eq!(
        process.execute(probe.clone(), IO_TIMEOUT).unwrap()["echo"],
        probe
    );
    process.shutdown(IO_TIMEOUT).unwrap();
}

#[cfg(not(windows))]
fn assert_timeout_poisoned_only_that_engine() {
    let mut command = engine_command("engine-timeout");
    let mut process =
        EngineProcessV1::spawn(&mut command, contract("engine-timeout"), IO_TIMEOUT).unwrap();
    process.bind(73, IO_TIMEOUT).unwrap();
    let result = process.execute(json!({"kind": "hang"}), Duration::from_millis(50));
    assert!(matches!(
        result,
        Err(EngineProcessErrorV1::Deadline("engine response"))
    ));
    assert!(matches!(
        process.execute(json!({"kind": "probe"}), IO_TIMEOUT),
        Err(EngineProcessErrorV1::Disconnected)
    ));
}

#[cfg(not(windows))]
fn run_contract() {
    assert_contract_mismatch_is_closed();
    assert_engine_owned_domain_rejection_crosses_the_stable_wire();
    assert_unstructured_rejection_code_is_still_fail_closed();
    assert_timeout_poisoned_only_that_engine();
    assert_supervisor_call_without_a_handler_poisoned_only_that_engine();
    assert_native_recovery_callbacks_are_ordered_and_clear_pending_state();
    let mut supervisor = SupervisorModelV1::default();
    let (mut first, first_outcome) = attach_engine(&mut supervisor, "engine-v1");
    let first_epoch = attached_epoch(&first_outcome);
    execute_neutral(&mut supervisor, &mut first, first_epoch, "engine-v1");
    execute_with_supervisor_callback(&mut supervisor, &mut first, first_epoch, "engine-v1");
    first.shutdown(IO_TIMEOUT).unwrap();

    let (mut second, second_outcome) = attach_engine(&mut supervisor, "engine-v2");
    let second_epoch = attached_epoch(&second_outcome);
    execute_neutral(&mut supervisor, &mut second, second_epoch, "engine-v2");
    second.shutdown(IO_TIMEOUT).unwrap();
}

#[cfg(windows)]
fn run_contract() {
    use std::fs;
    use std::thread;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = env::temp_dir().join(format!(
        "sts-workbench-engine-process-contract-{}-{unique}",
        std::process::id()
    ));
    let java_pid_path = base.join("java.pid");
    let rotation_marker = base.join("rotation.ok");
    fs::create_dir_all(&base).unwrap();

    let status = Command::new(env::current_exe().unwrap())
        .env(PROCESS_MODE, "supervisor")
        .env(JAVA_PID_PATH, &java_pid_path)
        .env(ROTATION_MARKER_PATH, &rotation_marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read_to_string(&rotation_marker).unwrap(), "java_alive");

    let java_pid = fs::read_to_string(&java_pid_path)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_is_running(java_pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(!process_is_running(java_pid));
    fs::remove_dir_all(base).unwrap();
}

fn attached_epoch(outcome: &RotationOutcomeV1) -> u64 {
    match outcome {
        RotationOutcomeV1::Attached { engine_epoch, .. } => *engine_epoch,
        RotationOutcomeV1::DeferredBusy { .. } => panic!("test Engine rotation was deferred"),
    }
}

#[cfg(windows)]
fn run_fake_supervisor() {
    use std::fs;

    assert_contract_mismatch_is_closed();
    assert_engine_owned_domain_rejection_crosses_the_stable_wire();
    assert_unstructured_rejection_code_is_still_fail_closed();
    assert_supervisor_call_without_a_handler_poisoned_only_that_engine();
    assert_native_recovery_callbacks_are_ordered_and_clear_pending_state();
    let _process_containment = ProcessContainmentV1::acquire_current_process().unwrap();

    let java_pid_path = env::var_os(JAVA_PID_PATH).unwrap();
    let rotation_marker = env::var_os(ROTATION_MARKER_PATH).unwrap();
    let base = std::path::Path::new(&java_pid_path).parent().unwrap();
    let canonical_engine = base.join("canonical-engine.exe");
    let image_store = EngineImageStoreV1::new(base.join("engine-images"));
    write_engine_variant(&canonical_engine, b"engine-v1");
    let first_image = image_store.materialize(&canonical_engine).unwrap();
    let java = sleeping_child();
    fs::write(java_pid_path, java.id().to_string()).unwrap();

    let mut supervisor = SupervisorModelV1::default();
    let (mut first, first_outcome) = attach_materialized_engine(&mut supervisor, &first_image);
    assert!(matches!(
        first_outcome,
        RotationOutcomeV1::Attached {
            java: JavaAfterRotationV1::Absent,
            ..
        }
    ));
    supervisor
        .runtime_opened(JavaRuntimeIdentityV1 {
            process_id: java.id(),
            process_start_token: "fake-java-start-1".to_owned(),
            runtime_epoch: "fake-java-epoch-1".to_owned(),
            java_build_id: "java-build-1".to_owned(),
            mod_build_id: "mod-build-1".to_owned(),
            protocol_version: 41,
        })
        .unwrap();
    let first_epoch = attached_epoch(&first_outcome);
    execute_neutral(
        &mut supervisor,
        &mut first,
        first_epoch,
        first_image.image_id(),
    );
    execute_with_supervisor_callback(
        &mut supervisor,
        &mut first,
        first_epoch,
        first_image.image_id(),
    );

    // A build replaces only the canonical artifact. The running Engine owns an
    // immutable image, so Windows must permit this write while Engine v1 lives.
    write_engine_variant(&canonical_engine, b"engine-v2");
    let second_image = image_store.materialize(&canonical_engine).unwrap();
    assert_ne!(first_image.image_id(), second_image.image_id());
    assert_ne!(first_image.executable(), second_image.executable());
    assert!(process_is_running(first.process_id()));
    assert!(process_is_running(java.id()));
    supervisor
        .begin_exchange(first_epoch, "job-timeout", None)
        .unwrap();
    assert!(matches!(
        first.execute(json!({"kind": "hang"}), Duration::from_millis(50)),
        Err(EngineProcessErrorV1::Deadline("engine response"))
    ));
    supervisor.engine_disconnected(first_epoch).unwrap();
    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Dirty {
            reason: "engine_disconnected_during_exchange"
        }
    );
    drop(first);
    assert!(process_is_running(java.id()));

    let (mut second, second_outcome) = attach_materialized_engine(&mut supervisor, &second_image);
    assert!(matches!(
        second_outcome,
        RotationOutcomeV1::Attached {
            java: JavaAfterRotationV1::DirtyRequiresRecovery { process_id },
            ..
        } if process_id == java.id()
    ));
    let second_epoch = attached_epoch(&second_outcome);
    supervisor
        .begin_recovery(second_epoch, "recover-after-timeout")
        .unwrap();
    let recovery = second
        .execute(json!({"kind": "recover_to_neutral"}), IO_TIMEOUT)
        .unwrap();
    assert_eq!(recovery["engine_image"], second_image.image_id());
    supervisor
        .complete_recovery(
            second_epoch,
            "recover-after-timeout",
            RecoverySettlementV1::Neutral,
        )
        .unwrap();
    supervisor
        .begin_exchange(second_epoch, "park-successor", None)
        .unwrap();
    let parked = second
        .execute(json!({"kind": "park_successor"}), IO_TIMEOUT)
        .unwrap();
    assert_eq!(parked["engine_image"], second_image.image_id());
    supervisor
        .complete_exchange(
            second_epoch,
            "park-successor",
            ExchangeSettlementV1::Parked {
                frame_token: "frame-2".to_owned(),
                autosave_identity: "autosave-2".to_owned(),
            },
        )
        .unwrap();

    write_engine_variant(&canonical_engine, b"engine-v3");
    let third_image = image_store.materialize(&canonical_engine).unwrap();
    second.shutdown(IO_TIMEOUT).unwrap();
    assert!(process_is_running(java.id()));

    let (mut third, third_outcome) = attach_materialized_engine(&mut supervisor, &third_image);
    assert!(matches!(
        third_outcome,
        RotationOutcomeV1::Attached {
            java: JavaAfterRotationV1::ParkedRequiresRevalidation { process_id, .. },
            ..
        } if process_id == java.id()
    ));
    let third_epoch = attached_epoch(&third_outcome);
    assert_eq!(
        supervisor.begin_exchange(third_epoch, "too-early", Some("frame-2")),
        Err(sts_workbench_supervisor::SupervisorErrorV1::ParkedFrameRequiresRevalidation)
    );
    supervisor
        .begin_parked_revalidation(third_epoch, "revalidate-frame", "frame-2")
        .unwrap();
    let revalidated = third
        .execute(json!({"kind": "revalidate_frame"}), IO_TIMEOUT)
        .unwrap();
    assert_eq!(revalidated["engine_image"], third_image.image_id());
    supervisor
        .complete_parked_revalidation(
            third_epoch,
            "revalidate-frame",
            ParkedRevalidationSettlementV1::Verified,
        )
        .unwrap();
    supervisor
        .begin_exchange(third_epoch, "from-parked", Some("frame-2"))
        .unwrap();
    let resumed = third
        .execute(json!({"kind": "resume_parked"}), IO_TIMEOUT)
        .unwrap();
    assert_eq!(resumed["engine_image"], third_image.image_id());
    supervisor
        .complete_exchange(third_epoch, "from-parked", ExchangeSettlementV1::Neutral)
        .unwrap();
    third.shutdown(IO_TIMEOUT).unwrap();
    assert!(process_is_running(java.id()));
    fs::write(rotation_marker, "java_alive").unwrap();

    std::mem::forget(java);
    std::process::exit(0);
}

#[cfg(windows)]
fn write_engine_variant(target: &std::path::Path, marker: &[u8]) {
    std::fs::copy(env::current_exe().unwrap(), target).unwrap();
    let mut output = std::fs::OpenOptions::new()
        .append(true)
        .open(target)
        .unwrap();
    output.write_all(marker).unwrap();
    output.sync_all().unwrap();
}

#[cfg(windows)]
fn sleeping_child() -> std::process::Child {
    Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 60",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

#[cfg(windows)]
fn process_is_running(process_id: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, process_id) };
    if process.is_null() {
        return false;
    }
    let running = unsafe { WaitForSingleObject(process, 0) } == WAIT_TIMEOUT;
    unsafe { CloseHandle(process) };
    running
}
