use sts_workbench_supervisor::{
    EngineContractV1, ExchangeSettlementV1, JavaAfterRotationV1, JavaRuntimeIdentityV1,
    NativeRecoveryRoundTripSettlementV1, ParkedRevalidationSettlementV1, PrepareEngineOutcomeV1,
    RecoverySettlementV1, RotationOutcomeV1, SupervisorErrorV1, SupervisorModelV1,
    SupervisorPhaseV1, SUPERVISOR_PROTOCOL_V2,
};

const JAVA_PID: u32 = 42_001;

fn engine(image: &str, minimum: u16, maximum: u16) -> EngineContractV1 {
    EngineContractV1 {
        image_id: image.to_owned(),
        supervisor_protocol: SUPERVISOR_PROTOCOL_V2,
        java_build_id: "java-build-1".to_owned(),
        mod_build_id: "mod-build-1".to_owned(),
        minimum_java_protocol: minimum,
        maximum_java_protocol: maximum,
    }
}

fn java() -> JavaRuntimeIdentityV1 {
    JavaRuntimeIdentityV1 {
        process_id: JAVA_PID,
        process_start_token: "process-start-1".to_owned(),
        runtime_epoch: "java-epoch-1".to_owned(),
        java_build_id: "java-build-1".to_owned(),
        mod_build_id: "mod-build-1".to_owned(),
        protocol_version: 41,
    }
}

fn attach_model(
    supervisor: &mut SupervisorModelV1,
    contract: EngineContractV1,
) -> Result<RotationOutcomeV1, SupervisorErrorV1> {
    match supervisor.prepare_engine(contract)? {
        PrepareEngineOutcomeV1::Prepared(prepared) => supervisor.attach_prepared_engine(prepared),
        PrepareEngineOutcomeV1::DeferredBusy {
            current_engine_epoch,
            job_id,
        } => Ok(RotationOutcomeV1::DeferredBusy {
            current_engine_epoch,
            job_id,
        }),
    }
}

fn parked_supervisor() -> SupervisorModelV1 {
    let mut supervisor = SupervisorModelV1::default();
    attach_model(&mut supervisor, engine("engine-v1", 41, 41)).unwrap();
    supervisor.runtime_opened(java()).unwrap();
    supervisor.begin_exchange(1, "job-1", None).unwrap();
    supervisor
        .complete_exchange(
            1,
            "job-1",
            ExchangeSettlementV1::Parked {
                frame_token: "frame-1".to_owned(),
                autosave_identity: "autosave-1".to_owned(),
            },
        )
        .unwrap();
    supervisor
}

#[test]
fn neutral_rotation_preserves_java_process() {
    let mut supervisor = SupervisorModelV1::default();
    attach_model(&mut supervisor, engine("engine-v1", 41, 41)).unwrap();
    supervisor.runtime_opened(java()).unwrap();

    let result = attach_model(&mut supervisor, engine("engine-v2", 41, 42)).unwrap();
    assert_eq!(
        result,
        RotationOutcomeV1::Attached {
            engine_epoch: 2,
            java: JavaAfterRotationV1::PreservedNeutral {
                process_id: JAVA_PID
            },
        }
    );
    assert_eq!(supervisor.runtime().unwrap().process_id, JAVA_PID);
}

#[test]
fn parked_rotation_requires_revalidation_before_another_exchange() {
    let mut supervisor = parked_supervisor();
    let result = attach_model(&mut supervisor, engine("engine-v2", 41, 41)).unwrap();
    assert!(matches!(
        result,
        RotationOutcomeV1::Attached {
            engine_epoch: 2,
            java: JavaAfterRotationV1::ParkedRequiresRevalidation {
                process_id: JAVA_PID,
                ..
            }
        }
    ));
    assert_eq!(
        supervisor.begin_exchange(2, "job-2", Some("frame-1")),
        Err(SupervisorErrorV1::ParkedFrameRequiresRevalidation)
    );

    supervisor
        .begin_parked_revalidation(2, "revalidate-1", "frame-1")
        .unwrap();
    supervisor.engine_disconnected(2).unwrap();
    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Parked {
            frame_token: "frame-1".to_owned(),
            autosave_identity: "autosave-1".to_owned(),
            validated_engine_epoch: None,
        }
    );

    let result = attach_model(&mut supervisor, engine("engine-v3", 41, 41)).unwrap();
    assert!(matches!(
        result,
        RotationOutcomeV1::Attached {
            engine_epoch: 3,
            java: JavaAfterRotationV1::ParkedRequiresRevalidation {
                process_id: JAVA_PID,
                ..
            }
        }
    ));
    supervisor
        .begin_parked_revalidation(3, "revalidate-2", "frame-1")
        .unwrap();
    supervisor
        .complete_parked_revalidation(3, "revalidate-2", ParkedRevalidationSettlementV1::Verified)
        .unwrap();
    supervisor
        .begin_exchange(3, "job-2", Some("frame-1"))
        .unwrap();
    supervisor
        .complete_exchange(3, "job-2", ExchangeSettlementV1::Neutral)
        .unwrap();
    assert_eq!(supervisor.runtime().unwrap().process_id, JAVA_PID);
}

#[test]
fn busy_exchange_defers_rotation_and_cannot_be_retired() {
    let mut supervisor = parked_supervisor();
    supervisor
        .begin_exchange(1, "job-2", Some("frame-1"))
        .unwrap();

    assert_eq!(
        attach_model(&mut supervisor, engine("engine-v2", 41, 41)).unwrap(),
        RotationOutcomeV1::DeferredBusy {
            current_engine_epoch: 1,
            job_id: "job-2".to_owned(),
        }
    );
    assert_eq!(supervisor.engine().unwrap().contract.image_id, "engine-v1");
    assert_eq!(
        supervisor.begin_retire("engine_update"),
        Err(SupervisorErrorV1::RuntimeBusy)
    );
}

#[test]
fn native_recovery_round_trip_is_read_only_over_the_primary_parked_runtime() {
    let mut supervisor = parked_supervisor();
    supervisor
        .begin_native_recovery_round_trip(1, "anchor-job-1", "frame-1")
        .unwrap();

    assert_eq!(
        attach_model(&mut supervisor, engine("engine-v2", 41, 41)).unwrap(),
        RotationOutcomeV1::DeferredBusy {
            current_engine_epoch: 1,
            job_id: "anchor-job-1".to_owned(),
        }
    );
    assert_eq!(
        supervisor.begin_exchange(1, "unrelated", Some("frame-1")),
        Err(SupervisorErrorV1::RuntimeUnavailable)
    );

    supervisor
        .complete_native_recovery_round_trip(
            1,
            "anchor-job-1",
            NativeRecoveryRoundTripSettlementV1::SourcePreserved,
        )
        .unwrap();
    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Parked {
            frame_token: "frame-1".to_owned(),
            autosave_identity: "autosave-1".to_owned(),
            validated_engine_epoch: Some(1),
        }
    );
    assert_eq!(supervisor.runtime().unwrap().process_id, JAVA_PID);
}

#[test]
fn engine_disconnect_during_native_round_trip_preserves_but_invalidates_source() {
    let mut supervisor = parked_supervisor();
    supervisor
        .begin_native_recovery_round_trip(1, "anchor-job-1", "frame-1")
        .unwrap();
    supervisor.engine_disconnected(1).unwrap();

    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Parked {
            frame_token: "frame-1".to_owned(),
            autosave_identity: "autosave-1".to_owned(),
            validated_engine_epoch: None,
        }
    );
    let result = attach_model(&mut supervisor, engine("engine-v2", 41, 41)).unwrap();
    assert!(matches!(
        result,
        RotationOutcomeV1::Attached {
            engine_epoch: 2,
            java: JavaAfterRotationV1::ParkedRequiresRevalidation { .. },
        }
    ));
    assert_eq!(
        supervisor.begin_native_recovery_round_trip(2, "anchor-job-2", "frame-1"),
        Err(SupervisorErrorV1::ParkedFrameRequiresRevalidation)
    );
}

#[test]
fn native_round_trip_can_fail_closed_when_the_source_is_uncertain() {
    let mut supervisor = parked_supervisor();
    supervisor
        .begin_native_recovery_round_trip(1, "anchor-job-1", "frame-1")
        .unwrap();
    supervisor
        .complete_native_recovery_round_trip(
            1,
            "anchor-job-1",
            NativeRecoveryRoundTripSettlementV1::Dirty {
                reason: "source_changed_during_capture",
            },
        )
        .unwrap();

    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Dirty {
            reason: "source_changed_during_capture",
        }
    );
}

#[test]
fn completed_exchange_can_require_runtime_retirement() {
    let mut supervisor = parked_supervisor();
    supervisor
        .begin_exchange(1, "job-2", Some("frame-1"))
        .unwrap();
    supervisor
        .complete_exchange(
            1,
            "job-2",
            ExchangeSettlementV1::Retire {
                reason: "semantic_settlement_requires_retirement",
            },
        )
        .unwrap();

    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Retiring {
            reason: "semantic_settlement_requires_retirement"
        }
    );
    assert_eq!(supervisor.runtime().unwrap().process_id, JAVA_PID);
    supervisor.complete_retirement().unwrap();
    assert_eq!(supervisor.phase(), &SupervisorPhaseV1::Idle);
    assert!(supervisor.runtime().is_none());
}

#[test]
fn dirty_java_can_recover_under_a_later_engine() {
    let mut supervisor = parked_supervisor();
    supervisor
        .begin_exchange(1, "job-2", Some("frame-1"))
        .unwrap();
    supervisor.engine_disconnected(1).unwrap();

    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Dirty {
            reason: "engine_disconnected_during_exchange"
        }
    );
    let result = attach_model(&mut supervisor, engine("engine-v2", 41, 41)).unwrap();
    assert!(matches!(
        result,
        RotationOutcomeV1::Attached {
            engine_epoch: 2,
            java: JavaAfterRotationV1::DirtyRequiresRecovery {
                process_id: JAVA_PID
            },
        }
    ));
    supervisor.begin_recovery(2, "recovery-1").unwrap();
    assert_eq!(
        supervisor.complete_exchange(2, "recovery-1", ExchangeSettlementV1::Neutral),
        Err(SupervisorErrorV1::RuntimeNotBusy)
    );
    supervisor.engine_disconnected(2).unwrap();
    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Dirty {
            reason: "engine_disconnected_during_recovery"
        }
    );

    let result = attach_model(&mut supervisor, engine("engine-v3", 41, 41)).unwrap();
    assert!(matches!(
        result,
        RotationOutcomeV1::Attached {
            engine_epoch: 3,
            java: JavaAfterRotationV1::DirtyRequiresRecovery {
                process_id: JAVA_PID
            },
        }
    ));
    supervisor.begin_recovery(3, "recovery-2").unwrap();
    supervisor
        .complete_recovery(3, "recovery-2", RecoverySettlementV1::Neutral)
        .unwrap();
    assert_eq!(supervisor.phase(), &SupervisorPhaseV1::Neutral);
    assert_eq!(supervisor.runtime().unwrap().process_id, JAVA_PID);
}

#[test]
fn failed_recovery_remains_dirty_or_explicitly_retires() {
    let mut supervisor = parked_supervisor();
    supervisor
        .begin_exchange(1, "job-2", Some("frame-1"))
        .unwrap();
    supervisor.engine_disconnected(1).unwrap();
    attach_model(&mut supervisor, engine("engine-v2", 41, 41)).unwrap();

    supervisor.begin_recovery(2, "recovery-1").unwrap();
    supervisor
        .complete_recovery(
            2,
            "recovery-1",
            RecoverySettlementV1::Dirty {
                reason: "native_reset_unverified",
            },
        )
        .unwrap();
    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Dirty {
            reason: "native_reset_unverified"
        }
    );
    assert_eq!(supervisor.runtime().unwrap().process_id, JAVA_PID);

    supervisor.begin_recovery(2, "recovery-2").unwrap();
    supervisor
        .complete_recovery(
            2,
            "recovery-2",
            RecoverySettlementV1::Retire {
                reason: "native_reset_impossible",
            },
        )
        .unwrap();
    assert_eq!(
        supervisor.phase(),
        &SupervisorPhaseV1::Retiring {
            reason: "native_reset_impossible"
        }
    );
}

#[test]
fn incompatible_java_contract_requires_retirement() {
    let mut wrong_mod = engine("engine-v2", 41, 41);
    wrong_mod.mod_build_id = "mod-build-2".to_owned();
    for incompatible in [engine("engine-v2", 42, 42), wrong_mod] {
        let mut supervisor = SupervisorModelV1::default();
        attach_model(&mut supervisor, engine("engine-v1", 41, 41)).unwrap();
        supervisor.runtime_opened(java()).unwrap();

        let result = attach_model(&mut supervisor, incompatible).unwrap();
        assert!(matches!(
            result,
            RotationOutcomeV1::Attached {
                java: JavaAfterRotationV1::RetireRequired {
                    process_id: JAVA_PID
                },
                ..
            }
        ));
        supervisor.complete_retirement().unwrap();
        assert_eq!(supervisor.phase(), &SupervisorPhaseV1::Idle);
        assert!(supervisor.runtime().is_none());
    }
}

#[test]
fn incompatible_supervisor_protocol_changes_nothing() {
    let mut supervisor = SupervisorModelV1::default();
    attach_model(&mut supervisor, engine("engine-v1", 41, 41)).unwrap();
    supervisor.runtime_opened(java()).unwrap();
    let before = supervisor.clone();
    let mut incompatible = engine("engine-v2", 41, 41);
    incompatible.supervisor_protocol = 1;

    assert_eq!(
        attach_model(&mut supervisor, incompatible),
        Err(SupervisorErrorV1::SupervisorProtocolMismatch)
    );
    assert_eq!(supervisor, before);
}

#[test]
fn prepared_engine_does_not_mutate_state_and_cannot_be_replayed() {
    let mut supervisor = SupervisorModelV1::default();
    attach_model(&mut supervisor, engine("engine-v1", 41, 41)).unwrap();
    supervisor.runtime_opened(java()).unwrap();
    let before = supervisor.clone();

    let prepared = match supervisor
        .prepare_engine(engine("engine-v2", 41, 41))
        .unwrap()
    {
        PrepareEngineOutcomeV1::Prepared(prepared) => prepared,
        PrepareEngineOutcomeV1::DeferredBusy { .. } => panic!("unexpected busy preparation"),
    };
    let replay = prepared.clone();
    assert_eq!(supervisor, before);
    assert_eq!(prepared.engine_epoch(), 2);

    let outcome = supervisor.attach_prepared_engine(prepared).unwrap();
    assert!(matches!(
        outcome,
        RotationOutcomeV1::Attached {
            engine_epoch: 2,
            java: JavaAfterRotationV1::PreservedNeutral {
                process_id: JAVA_PID
            }
        }
    ));
    assert_eq!(
        supervisor.attach_prepared_engine(replay),
        Err(SupervisorErrorV1::PreparedEngineStale)
    );
}
