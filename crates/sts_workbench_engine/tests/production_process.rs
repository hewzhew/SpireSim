use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use sts_workbench_engine::{
    EngineDomainRequestV1, EngineNativeResetStatusV1, EngineNativeResetV1,
    EngineParkedRevalidationStatusV1, EngineParkedRevalidationV1, ENGINE_NATIVE_RESET_SCHEMA_V1,
    ENGINE_PARKED_REVALIDATION_SCHEMA_V1,
};
use sts_workbench_supervisor::{
    EngineContractV1, EngineHostConfigurationV1, EngineHostErrorV1, EngineHostV1,
    EngineImageStoreV1, EngineProcessErrorV1, EngineProcessV1, EnginePublicRequestV1,
    ExchangeSettlementV1, JavaRuntimeIdentityV1, ParkedRevalidationSettlementV1,
    PhysicalRuntimeCallV1, PhysicalRuntimeOperationV1, PhysicalRuntimeResultV1,
    RecoverySettlementV1, SupervisorCallOutcomeV1, SupervisorErrorV1, SupervisorModelV1,
    SupervisorPhaseV1, PHYSICAL_RUNTIME_CALL_SCHEMA_V1, SUPERVISOR_PROTOCOL_V2,
};
use uuid::Uuid;

const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn production_engine_self_identifies_inspects_and_calls_supervisor() {
    let base = std::env::temp_dir().join(format!("sts-engine-production-{}", Uuid::new_v4()));
    let repository = base.join("repository");
    let java_jar = base.join("desktop-1.0.jar");
    let mod_jar = base.join("CommunicationMod.jar");
    fs::create_dir_all(&repository).unwrap();
    fs::write(&java_jar, b"java-build-fixture").unwrap();
    fs::write(&mod_jar, b"mod-build-fixture").unwrap();
    let handle = sts_workbench_artifact_store::publish_artifact(
        &repository,
        "paired_root_preview_detail",
        &json!({"meaning": "real artifact", "value": 7}),
    )
    .unwrap();

    let built_engine = PathBuf::from(env!("CARGO_BIN_EXE_sts_workbench_engine"));
    let canonical = base.join(if cfg!(windows) {
        "sts_workbench_engine.exe"
    } else {
        "sts_workbench_engine"
    });
    fs::copy(&built_engine, &canonical).unwrap();
    let protocol = u16::try_from(sts_java_parity_protocol::PROTOCOL_MINOR).unwrap();
    let configuration = EngineHostConfigurationV1::new(
        &repository,
        &canonical,
        base.join("host-engine-images"),
        &java_jar,
        &mod_jar,
        protocol,
    )
    .unwrap();
    let mut model = SupervisorModelV1::default();
    let mut host = EngineHostV1::default();
    let request = serde_json::to_value(EnginePublicRequestV1::new(json!({
        "kind": "inspect",
        "handle": handle,
    })))
    .unwrap();
    let output = host
        .execute(&mut model, &configuration, request.clone(), IO_TIMEOUT)
        .unwrap();
    let first_epoch = model.engine().unwrap().epoch;
    assert_eq!(output["schema"], "sts-workbench-inspect-response-v1");
    assert_eq!(output["handle"], handle);
    assert_eq!(output["document"]["meaning"], "real artifact");
    let repeated = host
        .execute(&mut model, &configuration, request, IO_TIMEOUT)
        .unwrap();
    assert_eq!(repeated["handle"], handle);
    assert_eq!(model.engine().unwrap().epoch, first_epoch);

    let request = serde_json::to_value(EngineDomainRequestV1::revalidate_parked_runtime()).unwrap();
    let mut callback_count = 0_usize;
    let output = host
        .execute_bound_with_supervisor(&mut model, first_epoch, request, IO_TIMEOUT, |call| {
            callback_count += 1;
            let request = serde_json::from_value::<PhysicalRuntimeCallV1>(call.input).unwrap();
            assert_eq!(request.schema, PHYSICAL_RUNTIME_CALL_SCHEMA_V1);
            assert_eq!(
                request.operation,
                PhysicalRuntimeOperationV1::ValidateParked {}
            );
            Ok(SupervisorCallOutcomeV1::Completed {
                output: serde_json::to_value(PhysicalRuntimeResultV1::parked_validated()).unwrap(),
            })
        })
        .unwrap();
    let output = serde_json::from_value::<EngineParkedRevalidationV1>(output).unwrap();
    assert_eq!(output.schema, ENGINE_PARKED_REVALIDATION_SCHEMA_V1);
    assert_eq!(output.status, EngineParkedRevalidationStatusV1::Validated);
    assert_eq!(callback_count, 1);
    assert_eq!(model.engine().unwrap().epoch, first_epoch);

    let invalid_request =
        serde_json::to_value(EngineDomainRequestV1::revalidate_parked_runtime()).unwrap();
    let error = host
        .execute_bound_with_supervisor(&mut model, first_epoch, invalid_request, IO_TIMEOUT, |_| {
            Ok(SupervisorCallOutcomeV1::Completed {
                output: json!({"status": "not-a-physical-result"}),
            })
        })
        .unwrap_err();
    assert!(matches!(
        error,
        EngineHostErrorV1::Process(EngineProcessErrorV1::Rejected { code })
            if code == "invalid_physical_runtime_result"
    ));
    let request = serde_json::to_value(EnginePublicRequestV1::new(json!({
        "kind": "inspect",
        "handle": handle,
    })))
    .unwrap();
    let after_rejection = host
        .execute(&mut model, &configuration, request, IO_TIMEOUT)
        .unwrap();
    assert_eq!(after_rejection["handle"], handle);
    assert_eq!(model.engine().unwrap().epoch, first_epoch);

    let request = serde_json::to_value(EngineDomainRequestV1::reset_to_native()).unwrap();
    let mut reset_callback_count = 0_usize;
    let output = host
        .execute_bound_with_supervisor(&mut model, first_epoch, request, IO_TIMEOUT, |call| {
            reset_callback_count += 1;
            let request = serde_json::from_value::<PhysicalRuntimeCallV1>(call.input).unwrap();
            assert_eq!(
                request.operation,
                PhysicalRuntimeOperationV1::ResetToNative {}
            );
            Ok(SupervisorCallOutcomeV1::Completed {
                output: serde_json::to_value(PhysicalRuntimeResultV1::native_reset()).unwrap(),
            })
        })
        .unwrap();
    let output = serde_json::from_value::<EngineNativeResetV1>(output).unwrap();
    assert_eq!(output.schema, ENGINE_NATIVE_RESET_SCHEMA_V1);
    assert_eq!(output.status, EngineNativeResetStatusV1::Reset);
    assert_eq!(reset_callback_count, 1);
    assert_eq!(model.engine().unwrap().epoch, first_epoch);

    let request = serde_json::to_value(EngineDomainRequestV1::reset_to_native()).unwrap();
    let error = host
        .execute_bound_with_supervisor(&mut model, first_epoch + 1, request, IO_TIMEOUT, |_| {
            panic!("wrong Engine epoch must reject before dispatch")
        })
        .unwrap_err();
    assert_eq!(
        error,
        EngineHostErrorV1::Supervisor(SupervisorErrorV1::EngineEpochMismatch)
    );

    let runtime_identity = JavaRuntimeIdentityV1 {
        process_id: 4242,
        process_start_token: "fake-process-start-1".to_owned(),
        runtime_epoch: "fake-java-runtime-1".to_owned(),
        java_build_id: hash(&java_jar, true),
        mod_build_id: hash(&mod_jar, true),
        protocol_version: protocol,
    };
    model.runtime_opened(runtime_identity.clone()).unwrap();

    model
        .begin_exchange(first_epoch, "disconnect-during-reset", None)
        .unwrap();
    let request = serde_json::to_value(EngineDomainRequestV1::reset_to_native()).unwrap();
    let error = host
        .execute_bound_with_supervisor(&mut model, first_epoch, request, IO_TIMEOUT, |_| {
            Err("synthetic physical callback disconnect".to_owned())
        })
        .unwrap_err();
    assert!(matches!(
        error,
        EngineHostErrorV1::Process(EngineProcessErrorV1::SupervisorCallFailed(detail))
            if detail == "synthetic physical callback disconnect"
    ));
    assert!(model.engine().is_none());
    assert_eq!(model.runtime(), Some(&runtime_identity));
    assert!(matches!(model.phase(), SupervisorPhaseV1::Dirty { .. }));

    let recovered_epoch = host
        .ensure_engine(&mut model, &configuration, IO_TIMEOUT)
        .unwrap();
    assert!(recovered_epoch > first_epoch);
    model
        .begin_recovery(recovered_epoch, "recover-native-reset")
        .unwrap();
    let request = serde_json::to_value(EngineDomainRequestV1::reset_to_native()).unwrap();
    let recovered = host
        .execute_bound_with_supervisor(&mut model, recovered_epoch, request, IO_TIMEOUT, |_| {
            Ok(SupervisorCallOutcomeV1::Completed {
                output: serde_json::to_value(PhysicalRuntimeResultV1::native_reset()).unwrap(),
            })
        })
        .unwrap();
    assert_eq!(
        serde_json::from_value::<EngineNativeResetV1>(recovered).unwrap(),
        EngineNativeResetV1::reset()
    );
    model
        .complete_recovery(
            recovered_epoch,
            "recover-native-reset",
            RecoverySettlementV1::Neutral,
        )
        .unwrap();
    assert_eq!(model.runtime(), Some(&runtime_identity));
    assert!(matches!(model.phase(), SupervisorPhaseV1::Neutral));

    model
        .begin_exchange(recovered_epoch, "park-before-rotation", None)
        .unwrap();
    model
        .complete_exchange(
            recovered_epoch,
            "park-before-rotation",
            ExchangeSettlementV1::Parked {
                frame_token: "fake-frame-1".to_owned(),
                autosave_identity: "fake-autosave-1".to_owned(),
            },
        )
        .unwrap();

    OpenOptions::new()
        .append(true)
        .open(&canonical)
        .unwrap()
        .write_all(b"engine-rotation-fixture-v2")
        .unwrap();
    let request = serde_json::to_value(EnginePublicRequestV1::new(json!({
        "kind": "inspect",
        "handle": handle,
    })))
    .unwrap();
    let rotated = host
        .execute(&mut model, &configuration, request, IO_TIMEOUT)
        .unwrap();
    assert_eq!(rotated["handle"], handle);
    let second_epoch = model.engine().unwrap().epoch;
    assert!(second_epoch > recovered_epoch);
    assert_eq!(model.runtime(), Some(&runtime_identity));
    assert!(matches!(
        model.phase(),
        SupervisorPhaseV1::Parked {
            frame_token,
            validated_engine_epoch: None,
            ..
        } if frame_token == "fake-frame-1"
    ));

    model
        .begin_parked_revalidation(second_epoch, "revalidate-after-rotation", "fake-frame-1")
        .unwrap();
    let request = serde_json::to_value(EngineDomainRequestV1::revalidate_parked_runtime()).unwrap();
    let revalidated = host
        .execute_bound_with_supervisor(&mut model, second_epoch, request, IO_TIMEOUT, |call| {
            let request = serde_json::from_value::<PhysicalRuntimeCallV1>(call.input).unwrap();
            assert_eq!(
                request.operation,
                PhysicalRuntimeOperationV1::ValidateParked {}
            );
            Ok(SupervisorCallOutcomeV1::Completed {
                output: serde_json::to_value(PhysicalRuntimeResultV1::parked_validated()).unwrap(),
            })
        })
        .unwrap();
    let revalidated = serde_json::from_value::<EngineParkedRevalidationV1>(revalidated).unwrap();
    assert_eq!(
        revalidated.status,
        EngineParkedRevalidationStatusV1::Validated
    );
    model
        .complete_parked_revalidation(
            second_epoch,
            "revalidate-after-rotation",
            ParkedRevalidationSettlementV1::Verified,
        )
        .unwrap();
    assert_eq!(model.runtime(), Some(&runtime_identity));
    assert!(matches!(
        model.phase(),
        SupervisorPhaseV1::Parked {
            validated_engine_epoch: Some(epoch),
            ..
        } if *epoch == second_epoch
    ));

    host.shutdown(&mut model, IO_TIMEOUT).unwrap();
    assert!(model.engine().is_none());
    assert_eq!(model.runtime(), Some(&runtime_identity));

    let store = EngineImageStoreV1::new(base.join("engine-images"));
    let image = store.materialize(&built_engine).unwrap();
    let mut wrong_contract = engine_contract(image.image_id(), &java_jar, &mod_jar);
    wrong_contract.java_build_id = "NOT-THE-FILE-HASH".to_owned();
    let mut command = image.command(&wrong_contract).unwrap();
    configure(&mut command, &repository, &java_jar, &mod_jar);
    assert!(matches!(
        EngineProcessV1::spawn(&mut command, wrong_contract, IO_TIMEOUT),
        Err(EngineProcessErrorV1::ContractMismatch)
    ));

    fs::remove_dir_all(base).unwrap();
}

fn configure(command: &mut std::process::Command, repository: &Path, java: &Path, module: &Path) {
    command
        .arg("--stdio")
        .arg("--repository")
        .arg(repository)
        .arg("--java-jar")
        .arg(java)
        .arg("--mod-jar")
        .arg(module)
        .stderr(std::process::Stdio::null());
}

fn engine_contract(image_id: &str, java: &Path, module: &Path) -> EngineContractV1 {
    let protocol = u16::try_from(sts_java_parity_protocol::PROTOCOL_MINOR).unwrap();
    EngineContractV1 {
        image_id: image_id.to_owned(),
        supervisor_protocol: SUPERVISOR_PROTOCOL_V2,
        java_build_id: hash(java, true),
        mod_build_id: hash(module, true),
        minimum_java_protocol: protocol,
        maximum_java_protocol: protocol,
    }
}

fn hash(path: &Path, uppercase: bool) -> String {
    let digest = Sha256::digest(fs::read(path).unwrap());
    if uppercase {
        format!("{digest:X}")
    } else {
        format!("{digest:x}")
    }
}
