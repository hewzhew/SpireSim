use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use sts_parity_lockstep::{
    DomainEvidenceV1, LockstepAdapterV1, NaturalRunBootstrapCardV1, NaturalRunBootstrapV1,
    NaturalRunCardSourceV1, NaturalRunContentProfileV1, NaturalRunNeowEligibilityV1,
    ParityDomainV1, SemanticActionV1, SemanticScalarV1, NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1,
};
use sts_parity_lockstep_java::{
    JavaLockstepAdapterV1, JavaLockstepReceiptV1, JavaLockstepTransportV1, JavaRuntimeActionV1,
    JavaRuntimeObservationV1, RecordedJavaActionPhaseV1, RecordedJavaBoundaryV1,
    RecordedJavaEvidenceStoreV1, RecordedJavaEvidenceV1,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct FakeTransport {
    states: Vec<JavaRuntimeObservationV1>,
    cursor: usize,
    calls: Vec<String>,
}

impl FakeTransport {
    fn advance(
        &mut self,
        runtime_handle: &str,
    ) -> Result<JavaLockstepReceiptV1, sts_parity_lockstep::AdapterFailureV1> {
        let before = &self.states[self.cursor];
        assert!(before
            .actions
            .iter()
            .any(|action| action.runtime_handle == runtime_handle));
        let action_id = before
            .actions
            .iter()
            .find(|action| action.runtime_handle == runtime_handle)
            .unwrap()
            .action_id
            .clone();
        let receipt = JavaLockstepReceiptV1 {
            status: "completed".to_owned(),
            request_id: format!("fixture-request-{}", before.frame_id),
            based_on_frame_id: before.frame_id,
            action_id,
            successor_frame_id: Some(self.states[self.cursor + 1].frame_id),
        };
        self.calls.push(runtime_handle.to_owned());
        self.cursor += 1;
        Ok(receipt)
    }

    fn attach_empty_rng_witness(&mut self, receipt: &JavaLockstepReceiptV1) {
        let successor = &mut self.states[self.cursor];
        if successor.frame["rng_call_witness"].is_null() {
            successor.frame["rng_call_witness"] = json!({
                "kind": "rng_call_witness_v1",
                "schema": NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1,
                "from_frame_id": receipt.based_on_frame_id,
                "to_frame_id": successor.frame_id,
                "request_id": receipt.request_id,
                "action_id": receipt.action_id,
                "complete": true,
                "calls": [],
                "unsupported_reason": null,
            });
        }
    }
}

struct MissingRngWitnessTransport(FakeTransport);

impl JavaLockstepTransportV1 for MissingRngWitnessTransport {
    fn status(
        &mut self,
    ) -> Result<JavaRuntimeObservationV1, sts_parity_lockstep::AdapterFailureV1> {
        self.0.status()
    }

    fn act(
        &mut self,
        runtime_handle: &str,
    ) -> Result<JavaLockstepReceiptV1, sts_parity_lockstep::AdapterFailureV1> {
        self.0.advance(runtime_handle)
    }

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, sts_parity_lockstep::AdapterFailureV1> {
        assert_eq!(selected_card_uuids.len(), 1);
        self.0.advance(runtime_handle)
    }

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, sts_parity_lockstep::AdapterFailureV1> {
        assert_eq!(selected_card_uuids.len(), 1);
        self.0.advance(runtime_handle)
    }
}

impl JavaLockstepTransportV1 for FakeTransport {
    fn status(
        &mut self,
    ) -> Result<JavaRuntimeObservationV1, sts_parity_lockstep::AdapterFailureV1> {
        Ok(self.states[self.cursor].clone())
    }

    fn act(
        &mut self,
        runtime_handle: &str,
    ) -> Result<JavaLockstepReceiptV1, sts_parity_lockstep::AdapterFailureV1> {
        let receipt = self.advance(runtime_handle)?;
        self.attach_empty_rng_witness(&receipt);
        Ok(receipt)
    }

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, sts_parity_lockstep::AdapterFailureV1> {
        assert_eq!(selected_card_uuids.len(), 1);
        let receipt = self.act(runtime_handle)?;
        self.calls
            .last_mut()
            .unwrap()
            .push_str(&format!("|grid={}", selected_card_uuids.join(",")));
        Ok(receipt)
    }

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, sts_parity_lockstep::AdapterFailureV1> {
        assert_eq!(selected_card_uuids.len(), 1);
        let receipt = self.act(runtime_handle)?;
        self.calls
            .last_mut()
            .unwrap()
            .push_str(&format!("|hand={}", selected_card_uuids.join(",")));
        Ok(receipt)
    }
}

#[test]
fn java_projection_exposes_known_domains_and_marks_protocol_gaps() {
    let transport = FakeTransport {
        states: vec![neow_observation(3, 7)],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();

    assert_eq!(snapshot.boundary_kind, "natural_neow");
    for domain in [
        ParityDomainV1::Run,
        ParityDomainV1::Map,
        ParityDomainV1::Room,
        ParityDomainV1::ActionSpace,
        ParityDomainV1::Relics,
        ParityDomainV1::Potions,
        ParityDomainV1::RuntimeSettlement,
    ] {
        assert!(matches!(
            snapshot.domains[&domain].evidence,
            DomainEvidenceV1::Observed { .. }
        ));
    }
    for domain in [
        ParityDomainV1::MasterDeck,
        ParityDomainV1::RngStreams,
        ParityDomainV1::CardPools,
        ParityDomainV1::CardBindings,
    ] {
        assert!(matches!(
            snapshot.domains[&domain].evidence,
            DomainEvidenceV1::Unobserved { .. }
        ));
    }
}

#[test]
fn complete_java_run_state_preserves_future_schedules() {
    let mut observation = neow_observation(3, 7);
    observation.frame["parity_internal"]["run_state"]["encounter_schedule"]["normal"] =
        json!(["2 Louse", "Cultist", "Jaw Worm"]);
    let transport = FakeTransport {
        states: vec![observation],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();
    let DomainEvidenceV1::Observed { value, .. } = &snapshot.domains[&ParityDomainV1::Run].evidence
    else {
        panic!("complete Java run state should be observed")
    };

    assert_eq!(value["shop_purge_base_cost"], 75);
    assert_eq!(value["keys"]["ruby"], false);
    assert_eq!(
        value["encounter_schedule"]["normal"],
        json!(["2louse", "cultist", "jawworm"])
    );
    assert_eq!(
        value["event_schedule"]["events"],
        json!(["bigfish", "cleric"])
    );
}

#[test]
fn raw_java_boundary_round_trip_can_be_reprojected_without_a_transport() {
    let observation = neow_observation(3, 7);
    let expected = {
        let transport = FakeTransport {
            states: vec![observation.clone()],
            cursor: 0,
            calls: Vec::new(),
        };
        let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
        adapter.observe().unwrap()
    };
    let encoded = serde_json::to_vec(&RecordedJavaBoundaryV1::new(observation).unwrap()).unwrap();
    let decoded: RecordedJavaBoundaryV1 = serde_json::from_slice(&encoded).unwrap();

    assert_eq!(decoded.reproject(&bootstrap()).unwrap(), expected);
}

#[test]
fn raw_java_boundary_store_creates_appends_and_loads_evidence() {
    let root = temp_path("raw-store");
    let directory = root.join("java-evidence");
    let observation = neow_observation(3, 7);
    let expected_bootstrap = bootstrap();
    let mut store =
        RecordedJavaEvidenceStoreV1::create_new(&directory, expected_bootstrap.clone()).unwrap();

    let path = store.append_boundary(&observation).unwrap();
    let loaded = RecordedJavaEvidenceStoreV1::load_boundary(&path).unwrap();

    assert_eq!(store.directory(), directory);
    assert_eq!(
        RecordedJavaEvidenceStoreV1::load_manifest(&directory.join("manifest.json"))
            .unwrap()
            .bootstrap,
        expected_bootstrap
    );
    assert_eq!(loaded.observation, observation);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn java_adapter_records_action_receipt_between_raw_boundaries() {
    let root = temp_path("raw-action");
    let directory = root.join("java-evidence");
    let store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap()).unwrap();
    let transport = FakeTransport {
        states: vec![map_observation(30), map_observation(31)],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter =
        JavaLockstepAdapterV1::with_evidence_store(transport, bootstrap(), store).unwrap();
    let before = adapter.observe().unwrap();
    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();

    adapter.execute(prepared).unwrap();

    let recorded = RecordedJavaEvidenceStoreV1::load_action(
        &directory
            .join("actions")
            .join("00000000-from-frame-30.json"),
    )
    .unwrap();
    assert_eq!(recorded.phase, RecordedJavaActionPhaseV1::Committed);
    assert_eq!(recorded.action, action);
    assert_eq!(recorded.receipt.status, "completed");
    assert_eq!(recorded.receipt.successor_frame_id, Some(31));
    assert!(directory
        .join("boundaries")
        .join("00000000-frame-30.json")
        .is_file());
    assert!(directory
        .join("boundaries")
        .join("00000001-frame-31.json")
        .is_file());
    let evidence = RecordedJavaEvidenceV1::load_complete(&directory).unwrap();
    assert_eq!(evidence.steps.len(), 1);
    assert_eq!(evidence.java_action_count, 1);
    assert_eq!(evidence.steps[0].action, action);
    let step = adapter.last_recorded_step_v1().unwrap();
    assert_eq!(step.committed_actions(), 1);
    assert_eq!(step.before(), &evidence.steps[0].before.observation);
    assert_eq!(step.after(), &evidence.steps[0].after.observation);
    drop(adapter);
    let mut store = RecordedJavaEvidenceStoreV1::open_append(&directory).unwrap();
    assert!(store.matches_resume_observation_v1(&evidence.steps[0].after.observation));
    assert!(!store.matches_resume_observation_v1(&evidence.initial.observation));
    let mut changed = evidence.steps[0].after.observation.clone();
    changed.producer.session_epoch.push_str("-new");
    assert!(!store.matches_resume_observation_v1(&changed));
    changed = evidence.steps[0].after.observation.clone();
    changed.frame["diagnostic-change"] = serde_json::json!(true);
    assert!(!store.matches_resume_observation_v1(&changed));
    store
        .append_boundary(&evidence.steps[0].after.observation)
        .unwrap();
    assert!(!store.matches_resume_observation_v1(&evidence.steps[0].after.observation));
    store
        .append_action(
            RecordedJavaActionPhaseV1::Committed,
            &action,
            &JavaLockstepReceiptV1 {
                status: "unknown".to_owned(),
                request_id: "failed-tail".to_owned(),
                based_on_frame_id: 31,
                action_id: "failed-tail".to_owned(),
                successor_frame_id: None,
            },
        )
        .unwrap();
    let prefix = RecordedJavaEvidenceV1::load_prefix(&directory, 1).unwrap();
    assert_eq!(prefix, evidence);
    assert!(RecordedJavaEvidenceV1::load_complete(&directory).is_err());
    assert!(RecordedJavaEvidenceV1::load_prefix(&directory, 2).is_err());
    assert_eq!(
        RecordedJavaEvidenceV1::load_prefix(&directory, 0)
            .unwrap()
            .steps
            .len(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn writer_step_keeps_semantic_count_across_settlement_and_reopen() {
    let directory = temp_path("writer-step");
    let mut store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap()).unwrap();
    let mut raw = JavaRuntimeObservationV1 {
        producer: sts_parity_lockstep::ProducerIdentityV1 {
            implementation: "test".into(),
            version: "1".into(),
            session_epoch: "epoch".into(),
        },
        frame_id: 10,
        frame: json!({}),
        actions: vec![],
    };
    store.append_boundary(&raw).unwrap();
    assert!(store.last_complete_step_v1().is_none());
    let action = SemanticActionV1::EndTurn;
    for (index, phase) in [
        RecordedJavaActionPhaseV1::NativeReloadSettlement,
        RecordedJavaActionPhaseV1::Committed,
        RecordedJavaActionPhaseV1::InterstitialSettlement,
    ]
    .into_iter()
    .enumerate()
    {
        let receipt = JavaLockstepReceiptV1 {
            status: "completed".into(),
            request_id: format!("r{index}"),
            based_on_frame_id: raw.frame_id,
            action_id: "end".into(),
            successor_frame_id: Some(raw.frame_id + 1),
        };
        store.append_action(phase, &action, &receipt).unwrap();
        assert!(store.last_complete_step_v1().is_none());
        raw.frame_id += 1;
        store.append_boundary(&raw).unwrap();
    }
    let step = store.last_complete_step_v1().unwrap();
    assert_eq!(step.committed_actions(), 1);
    assert_eq!(step.before().frame_id, 11);
    assert_eq!(step.after().frame_id, 13);
    drop(store);
    let mut store = RecordedJavaEvidenceStoreV1::open_append(&directory).unwrap();
    assert!(store.last_complete_step_v1().is_none());
    store.append_boundary(&raw).unwrap();
    assert!(store.last_complete_step_v1().is_none());
    store
        .append_action(
            RecordedJavaActionPhaseV1::Committed,
            &action,
            &JavaLockstepReceiptV1 {
                status: "completed".into(),
                request_id: "r3".into(),
                based_on_frame_id: 13,
                action_id: "end".into(),
                successor_frame_id: Some(14),
            },
        )
        .unwrap();
    raw.frame_id = 14;
    store.append_boundary(&raw).unwrap();
    let step = store.last_complete_step_v1().unwrap();
    assert_eq!(step.committed_actions(), 2);
    assert_eq!(step.before().frame_id, 13);
    assert_eq!(step.after().frame_id, 14);
    // A failed durable write must never leave the old step available as new evidence.
    fs::write(
        directory.join("actions/00000004-from-frame-14.json"),
        b"occupied",
    )
    .unwrap();
    assert!(store
        .append_action(
            RecordedJavaActionPhaseV1::Committed,
            &action,
            &JavaLockstepReceiptV1 {
                status: "completed".into(),
                request_id: "r4".into(),
                based_on_frame_id: 14,
                action_id: "end".into(),
                successor_frame_id: Some(15),
            }
        )
        .is_err());
    assert!(store.last_complete_step_v1().is_none());
    assert!(directory.starts_with(std::env::temp_dir()));
    fs::remove_dir_all(directory).unwrap();
}

fn recorded_prefix_fixture(label: &str) -> PathBuf {
    let root = temp_path(label);
    let mut store = RecordedJavaEvidenceStoreV1::create_new(&root, bootstrap()).unwrap();
    store.append_boundary(&map_observation(30)).unwrap();
    for (index, phase) in [
        RecordedJavaActionPhaseV1::NativeReloadSettlement,
        RecordedJavaActionPhaseV1::Committed,
        RecordedJavaActionPhaseV1::InterstitialSettlement,
        RecordedJavaActionPhaseV1::Committed,
        RecordedJavaActionPhaseV1::Committed,
    ]
    .into_iter()
    .enumerate()
    {
        let from = 30 + index as u64;
        store
            .append_action(
                phase,
                &SemanticActionV1::EndTurn,
                &JavaLockstepReceiptV1 {
                    status: "completed".into(),
                    request_id: format!("r{from}"),
                    based_on_frame_id: from,
                    action_id: "end".into(),
                    successor_frame_id: Some(from + 1),
                },
            )
            .unwrap();
        store.append_boundary(&map_observation(from + 1)).unwrap();
    }
    root
}

#[test]
fn bounded_prefix_includes_preparation_and_settlements_but_not_later_payloads() {
    let root = recorded_prefix_fixture("bounded-prefix");
    let complete = RecordedJavaEvidenceV1::load_complete(&root).unwrap();
    let zero = RecordedJavaEvidenceV1::load_prefix(&root, 0).unwrap();
    assert_eq!(zero.java_action_count, 1);
    assert_eq!(zero.transitions, complete.transitions[..1]);
    let one = RecordedJavaEvidenceV1::load_prefix(&root, 1).unwrap();
    assert_eq!(one.java_action_count, 3);
    assert_eq!(one.transitions, complete.transitions[..3]);
    assert_eq!(one.steps, complete.steps[..1]);
    assert_eq!(
        RecordedJavaEvidenceV1::load_prefix(&root, 3).unwrap(),
        complete
    );
    assert!(RecordedJavaEvidenceV1::load_prefix(&root, 4).is_err());

    // Only the next action's phase is needed to close the prefix. Its unchosen
    // action/receipt, successor frame, and all later payloads are out of scope.
    let delimiter = root.join("actions/00000003-from-frame-33.json");
    let mut raw: Value = serde_json::from_slice(&fs::read(&delimiter).unwrap()).unwrap();
    raw["action"] = json!({"kind":"future-unsupported-action"});
    raw["receipt"] = Value::Null;
    fs::write(&delimiter, serde_json::to_vec(&raw).unwrap()).unwrap();
    fs::write(
        root.join("boundaries/00000004-frame-34.json"),
        b"incomplete later frame",
    )
    .unwrap();
    fs::write(
        root.join("actions/00000004-from-frame-34.json"),
        b"incomplete later action",
    )
    .unwrap();
    assert_eq!(RecordedJavaEvidenceV1::load_prefix(&root, 1).unwrap(), one);
    assert_eq!(RecordedJavaEvidenceV1::load_prefix(&root, 0).unwrap(), zero);
    assert!(RecordedJavaEvidenceV1::load_prefix(&root, 2).is_err());
    assert!(RecordedJavaEvidenceV1::load_complete(&root).is_err());
    // Without a readable delimiter we cannot infer whether a settlement is
    // still part of the chosen semantic action.
    fs::write(&delimiter, b"incomplete delimiter").unwrap();
    assert!(RecordedJavaEvidenceV1::load_prefix(&root, 1).is_err());
    assert!(root.starts_with(std::env::temp_dir()));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bounded_prefix_rejects_holes_duplicates_and_invalid_selected_evidence() {
    for defect in [
        "action_gap",
        "boundary_gap",
        "duplicate",
        "duplicate_delimiter",
        "duplicate_terminal_boundary",
        "receipt",
        "producer",
        "settlement",
    ] {
        let root = recorded_prefix_fixture(defect);
        match defect {
            "action_gap" => {
                fs::remove_file(root.join("actions/00000002-from-frame-32.json")).unwrap();
            }
            "boundary_gap" => {
                fs::remove_file(root.join("boundaries/00000002-frame-32.json")).unwrap();
            }
            "duplicate" => {
                fs::copy(
                    root.join("actions/00000002-from-frame-32.json"),
                    root.join("actions/00000002-from-frame-99.json"),
                )
                .unwrap();
            }
            "duplicate_delimiter" => {
                fs::copy(
                    root.join("actions/00000003-from-frame-33.json"),
                    root.join("actions/00000003-from-frame-99.json"),
                )
                .unwrap();
            }
            "duplicate_terminal_boundary" => {
                fs::copy(
                    root.join("boundaries/00000003-frame-33.json"),
                    root.join("boundaries/00000003-frame-99.json"),
                )
                .unwrap();
            }
            "receipt" | "settlement" => {
                let path = root.join("actions/00000002-from-frame-32.json");
                let mut raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                if defect == "receipt" {
                    raw["receipt"]["successor_frame_id"] = json!(99);
                } else {
                    raw["phase"] = json!("native_reload_settlement");
                }
                fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
            }
            "producer" => {
                let path = root.join("boundaries/00000003-frame-33.json");
                let mut raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                raw["observation"]["producer"]["session_epoch"] = json!("changed");
                fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            RecordedJavaEvidenceV1::load_prefix(&root, 1).is_err(),
            "{defect}"
        );
        assert!(
            RecordedJavaEvidenceV1::load_complete(&root).is_err(),
            "{defect}"
        );
        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn writer_step_never_promotes_invalid_raw_evidence() {
    for defect in [
        "receipt",
        "predecessor",
        "successor",
        "producer",
        "settlement",
    ] {
        let directory = temp_path(defect);
        let mut store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap()).unwrap();
        let mut raw = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "test".into(),
                version: "1".into(),
                session_epoch: "epoch".into(),
            },
            frame_id: 10,
            frame: json!({}),
            actions: vec![],
        };
        store.append_boundary(&raw).unwrap();
        store
            .append_action(
                if defect == "settlement" {
                    RecordedJavaActionPhaseV1::InterstitialSettlement
                } else {
                    RecordedJavaActionPhaseV1::Committed
                },
                &SemanticActionV1::EndTurn,
                &JavaLockstepReceiptV1 {
                    status: if defect == "receipt" {
                        "rejected"
                    } else {
                        "completed"
                    }
                    .into(),
                    request_id: "r".into(),
                    based_on_frame_id: if defect == "predecessor" { 9 } else { 10 },
                    action_id: "end".into(),
                    successor_frame_id: Some(11),
                },
            )
            .unwrap();
        raw.frame_id = if defect == "successor" { 12 } else { 11 };
        if defect == "producer" {
            raw.producer.session_epoch = "new-epoch".into();
        }
        store.append_boundary(&raw).unwrap();
        assert!(store.last_complete_step_v1().is_none(), "{defect}");
        assert!(directory.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn resumed_java_adapter_confirms_tail_once_and_continues_ordinals() {
    let root = temp_path("raw-resume-append");
    let directory = root.join("java-evidence");
    let store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap()).unwrap();
    let transport = FakeTransport {
        states: vec![map_observation(30), map_observation(31)],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter =
        JavaLockstepAdapterV1::with_evidence_store(transport, bootstrap(), store).unwrap();
    let before = adapter.observe().unwrap();
    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();
    adapter.execute(prepared).unwrap();
    let resume = adapter.resume_state_v1().unwrap();
    drop(adapter);

    let recorded_tail = RecordedJavaEvidenceV1::load_complete(&directory)
        .unwrap()
        .transitions
        .last()
        .unwrap()
        .after
        .observation
        .clone();
    let store = RecordedJavaEvidenceStoreV1::open_append(&directory).unwrap();
    let transport = FakeTransport {
        states: vec![recorded_tail, map_observation(32)],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut resumed =
        JavaLockstepAdapterV1::resume_with_evidence_store(transport, bootstrap(), store, resume)
            .unwrap();
    let resumed_before = resumed.observe().unwrap();
    assert_eq!(
        fs::read_dir(directory.join("boundaries")).unwrap().count(),
        2,
        "resume confirmation must not duplicate the recorded tail"
    );
    let action = action_space(&resumed_before).remove(0);
    let prepared = resumed.prepare(&action, &resumed_before).unwrap();
    resumed.execute(prepared).unwrap();

    let evidence = RecordedJavaEvidenceV1::load_complete(&directory).unwrap();
    assert_eq!(evidence.steps.len(), 2);
    assert_eq!(evidence.java_action_count, 2);
    assert!(directory
        .join("actions")
        .join("00000001-from-frame-31.json")
        .is_file());
    assert!(directory
        .join("boundaries")
        .join("00000002-frame-32.json")
        .is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resumed_java_adapter_rejects_a_different_live_boundary() {
    let root = temp_path("raw-resume-mismatch");
    let directory = root.join("java-evidence");
    let store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap()).unwrap();
    let transport = FakeTransport {
        states: vec![map_observation(30)],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter =
        JavaLockstepAdapterV1::with_evidence_store(transport, bootstrap(), store).unwrap();
    adapter.observe().unwrap();
    let resume = adapter.resume_state_v1().unwrap();
    drop(adapter);

    let store = RecordedJavaEvidenceStoreV1::open_append(&directory).unwrap();
    let transport = FakeTransport {
        states: vec![map_observation(31)],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut resumed =
        JavaLockstepAdapterV1::resume_with_evidence_store(transport, bootstrap(), store, resume)
            .unwrap();
    let error = resumed.observe().unwrap_err();

    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error
        .message
        .contains("differs from the replayed resume boundary"));
    assert_eq!(
        fs::read_dir(directory.join("boundaries")).unwrap().count(),
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_boss_reload_reveals_with_potions_without_discarding_or_selecting_a_relic() {
    let boss = |frame_id, opened| {
        let mut actions = if opened {
            ["Black Blood", "Coffee Dripper", "Calling Bell"]
                .into_iter()
                .enumerate()
                .map(|(index, id)| {
                    let mut action =
                        parameter_free_action("choose_boss_relic", &format!("choose{index}"));
                    action.action_id = format!("boss_relic:{index}:{id}");
                    action.reward_type = Some("BOSS_RELIC".into());
                    action.reward_index = Some(index);
                    action.reward_id = Some(id.into());
                    action
                })
                .collect::<Vec<_>>()
        } else {
            vec![parameter_free_action("open_boss_chest", "open")]
        };
        for (slot, id) in ["LiquidBronze", "Weak Potion"].into_iter().enumerate() {
            let mut discard = parameter_free_action("discard_potion", &format!("discard{slot}"));
            discard.action_id = format!("discard_potion:{slot}:{id}");
            discard.potion_slot = Some(slot);
            discard.potion_id = Some(id.into());
            actions.push(discard);
        }
        let mut raw = observation(
            frame_id,
            json!({"kind": if opened {"boss_relic"} else {"boss_treasure"}}),
            actions,
        );
        raw.frame["public_base"]["map"]["current_x"] = json!(-1);
        raw.frame["public_base"]["map"]["current_y"] = json!(15);
        raw.frame["parity_internal"]["potions"] = json!([
            {"slot_index": 0, "potion_id": "LiquidBronze", "empty": false},
            {"slot_index": 1, "potion_id": "Weak Potion", "empty": false},
        ]);
        raw.frame["rng_internal"] = complete_rng_internal();
        bind_test_master_deck_sources(&mut raw);
        raw
    };
    let mut original = JavaLockstepAdapterV1::new(
        FakeTransport {
            states: vec![boss(30, true)],
            cursor: 0,
            calls: Vec::new(),
        },
        bootstrap(),
    )
    .unwrap();
    let expected = original.observe().unwrap();
    let resume = original.resume_state_v1().unwrap();
    for case in [
        "reveal",
        "already_open",
        "missing_open",
        "malformed_discard",
        "wrong_successor",
    ] {
        let root = temp_path("native-boss-reveal-potions");
        let mut entry = boss(90, case == "already_open");
        if case == "missing_open" {
            entry.actions.remove(0);
        }
        if case == "malformed_discard" {
            entry.actions[1].potion_id = Some("Fire Potion".into());
        }
        let after = if case == "wrong_successor" {
            map_observation(91)
        } else {
            boss(91, true)
        };
        let store = RecordedJavaEvidenceStoreV1::create_new(&root, bootstrap()).unwrap();
        let mut java = JavaLockstepAdapterV1::reattach_after_native_reload_with_evidence_store(
            FakeTransport {
                states: vec![entry, after],
                cursor: 0,
                calls: Vec::new(),
            },
            bootstrap(),
            store,
            resume.clone(),
            None,
        )
        .unwrap();
        let result = java.observe();
        if matches!(case, "reveal" | "already_open") {
            assert_eq!(result.unwrap().domains, expected.domains);
        } else {
            assert!(result.is_err(), "{case}");
        }
        let calls = java.into_transport().calls;
        if matches!(case, "reveal" | "wrong_successor") {
            assert_eq!(calls, vec!["open"]);
        } else {
            assert!(calls.is_empty());
        }
        if case == "reveal" {
            let evidence = RecordedJavaEvidenceV1::load_complete(&root).unwrap();
            assert!(evidence.steps.is_empty());
            assert_eq!(evidence.java_action_count, 1);
            assert_eq!(
                evidence.transitions[0].action.phase,
                RecordedJavaActionPhaseV1::NativeReloadSettlement
            );
        }
        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn shop_reward_close_keeps_pending_items_and_reopen_reuses_the_same_inventory() {
    use sts_parity_lockstep::DecisionSurfaceV1;
    let fixture = |frame_id, kind| {
        let kinds = match kind {
            "combat_reward" => vec!["close_shop_rewards"],
            "shop_entry" => vec!["enter_shop", "proceed_shop_room", "open_shop_rewards"],
            "shop" => vec!["leave_shop", "open_shop_rewards"],
            _ => unreachable!(),
        };
        let actions = kinds
            .into_iter()
            .map(|k| parameter_free_action(k, k))
            .collect();
        let mut raw = observation(frame_id, json!({"kind":kind}), actions);
        raw.frame["public_base"]["map"]["current_x"] = json!(0);
        raw.frame["public_base"]["map"]["current_y"] = json!(0);
        raw.frame["public_base"]["map"]["nodes"][0]["room_type"] =
            json!("com.megacrit.cardcrawl.rooms.ShopRoom");
        raw.frame["parity_internal"]["shop"] = json!({
            "cards": [], "relics": [], "potions": [],
            "purge_cost":75, "purge_available":false, "purge_cards":[],
        });
        raw.frame["parity_internal"]["combat_rewards"] = json!([
            {"index":0,"type":"GOLD","gold":7,"cards":null,"relic_id":null,"potion_id":null}
        ]);
        raw.frame["rng_internal"] = complete_rng_internal();
        raw.frame["action_space"] = json!({"actions": raw.actions.iter().map(|a| json!({
            "action_id": a.action_id, "kind": a.kind,
        })).collect::<Vec<_>>()});
        bind_test_master_deck_sources(&mut raw);
        raw
    };
    let states = vec![
        fixture(30, "combat_reward"),
        fixture(31, "shop_entry"),
        fixture(32, "shop"),
        fixture(33, "combat_reward"),
    ];
    let mut adapter = JavaLockstepAdapterV1::new(
        FakeTransport {
            states,
            cursor: 0,
            calls: vec![],
        },
        bootstrap(),
    )
    .unwrap();
    let before = adapter.observe().unwrap();
    let back = SemanticActionV1::Cancel {
        surface: DecisionSurfaceV1::CombatReward,
    };
    let prepared = adapter.prepare(&back, &before).unwrap();
    let closed = adapter.execute(prepared).unwrap();
    assert_eq!(closed.boundary_kind, "natural_shop");
    let DomainEvidenceV1::Observed { value: pending, .. } =
        &closed.domains[&ParityDomainV1::Rewards].evidence
    else {
        panic!()
    };
    assert_eq!(pending["items"], json!([{"kind":"gold","amount":7}]));
    assert_eq!(pending["skippable"], false);
    let reopen = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::CombatReward,
    };
    let prepared = adapter.prepare(&reopen, &closed).unwrap();
    let reopened = adapter.execute(prepared).unwrap();
    assert_eq!(
        reopened.domains[&ParityDomainV1::Room],
        before.domains[&ParityDomainV1::Room]
    );
    assert_eq!(
        reopened.domains[&ParityDomainV1::Rewards],
        before.domains[&ParityDomainV1::Rewards]
    );
    assert_eq!(
        adapter.into_transport().calls,
        vec!["close_shop_rewards", "enter_shop", "open_shop_rewards"]
    );
}

#[test]
fn native_shop_reload_reveals_only_the_source_inventory_and_records_preparation() {
    let shop = |frame_id, entry| {
        let actions = if entry {
            vec![
                parameter_free_action("enter_shop", "enter"),
                parameter_free_action("proceed_shop_room", "leave"),
            ]
        } else {
            vec![parameter_free_action("leave_shop", "leave")]
        };
        let mut raw = observation(
            frame_id,
            json!({"kind": if entry { "shop_entry" } else { "shop" }}),
            actions,
        );
        raw.frame["public_base"]["map"]["current_x"] = json!(0);
        raw.frame["public_base"]["map"]["current_y"] = json!(0);
        raw.frame["public_base"]["map"]["nodes"][0]["room_type"] =
            json!("com.megacrit.cardcrawl.rooms.ShopRoom");
        raw.frame["parity_internal"]["shop"] = json!({
            "cards": [], "relics": [], "potions": [],
            "purge_cost": 75, "purge_available": false, "purge_cards": [],
        });
        raw.frame["rng_internal"] = complete_rng_internal();
        raw.frame["action_space"] = json!({"actions": raw.actions.iter().map(|action| json!({
            "action_id": action.action_id, "kind": action.kind,
        })).collect::<Vec<_>>()});
        bind_test_master_deck_sources(&mut raw);
        raw
    };
    let source = shop(30, false);
    let mut original = JavaLockstepAdapterV1::new(
        FakeTransport {
            states: vec![source],
            cursor: 0,
            calls: Vec::new(),
        },
        bootstrap(),
    )
    .unwrap();
    let expected = original.observe().unwrap();
    let resume = original.resume_state_v1().unwrap();
    for case in ["reveal", "already_open", "missing_enter", "wrong_successor"] {
        let root = temp_path("native-shop-reveal");
        let mut entry = shop(90, case != "already_open");
        if case == "missing_enter" {
            entry.actions.remove(0);
            entry.frame["action_space"]["actions"]
                .as_array_mut()
                .unwrap()
                .remove(0);
        }
        let after = if case == "wrong_successor" {
            map_observation(91)
        } else {
            shop(91, false)
        };
        let store = RecordedJavaEvidenceStoreV1::create_new(&root, bootstrap()).unwrap();
        let mut java = JavaLockstepAdapterV1::reattach_after_native_reload_with_evidence_store(
            FakeTransport {
                states: vec![entry, after],
                cursor: 0,
                calls: Vec::new(),
            },
            bootstrap(),
            store,
            resume.clone(),
            None,
        )
        .unwrap();
        let result = java.observe();
        if matches!(case, "reveal" | "already_open") {
            let actual = result.unwrap();
            assert_eq!(actual.boundary_kind, expected.boundary_kind);
            assert_eq!(actual.domains, expected.domains);
        } else {
            assert!(result.is_err());
        }
        let transport = java.into_transport();
        assert_eq!(
            transport.calls.len(),
            usize::from(matches!(case, "reveal" | "wrong_successor"))
        );
        if case == "reveal" {
            let evidence = RecordedJavaEvidenceV1::load_complete(&root).unwrap();
            assert!(evidence.steps.is_empty());
            assert_eq!(evidence.java_action_count, 1);
            assert_eq!(
                evidence.transitions[0].action.phase,
                RecordedJavaActionPhaseV1::NativeReloadSettlement
            );
        }
        // The test owns this unique temp directory.
        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn native_reload_reissues_uuid_leases_from_the_proven_persisted_deck() {
    let root = temp_path("native-reload-rebind");
    let mut source = map_observation(30);
    bind_test_master_deck_sources(&mut source);
    let mut source_adapter = JavaLockstepAdapterV1::new(
        FakeTransport {
            states: vec![source],
            cursor: 0,
            calls: Vec::new(),
        },
        bootstrap(),
    )
    .unwrap();
    let source_snapshot = source_adapter.observe().unwrap();
    let resume = source_adapter.resume_state_v1().unwrap();

    let mut reloaded = map_observation(90);
    reloaded.frame["parity_internal"]["master_deck"][0]["card_uuid"] = json!("reloaded-bane");
    reloaded.frame["parity_internal"]["master_deck"][1]["card_uuid"] = json!("reloaded-starter");
    reloaded.frame["parity_internal"]["master_deck"][0]["source"] = Value::Null;
    reloaded.frame["parity_internal"]["master_deck"][1]["source"] = Value::Null;
    let offline_transport = FakeTransport {
        states: vec![reloaded.clone()],
        cursor: 0,
        calls: Vec::new(),
    };
    let offline_resume = resume.clone();
    let store = RecordedJavaEvidenceStoreV1::create_new(&root, bootstrap()).unwrap();
    let mut reattached = JavaLockstepAdapterV1::reattach_after_native_reload_with_evidence_store(
        FakeTransport {
            states: vec![reloaded],
            cursor: 0,
            calls: Vec::new(),
        },
        bootstrap(),
        store,
        resume,
        None,
    )
    .unwrap();
    let reloaded_snapshot = reattached.observe().unwrap();
    let mut offline = JavaLockstepAdapterV1::reattach_after_native_reload(
        offline_transport,
        bootstrap(),
        offline_resume,
        None,
    )
    .unwrap();
    assert_eq!(offline.observe().unwrap(), reloaded_snapshot);

    assert_eq!(
        reloaded_snapshot.domains[&ParityDomainV1::MasterDeck],
        source_snapshot.domains[&ParityDomainV1::MasterDeck]
    );
    assert_eq!(
        reloaded_snapshot.domains[&ParityDomainV1::CardBindings],
        source_snapshot.domains[&ParityDomainV1::CardBindings]
    );
    assert_eq!(fs::read_dir(root.join("boundaries")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_reload_rejects_a_changed_persisted_card_before_projection() {
    let root = temp_path("native-reload-card-mismatch");
    let mut source = map_observation(30);
    bind_test_master_deck_sources(&mut source);
    let mut source_adapter = JavaLockstepAdapterV1::new(
        FakeTransport {
            states: vec![source],
            cursor: 0,
            calls: Vec::new(),
        },
        bootstrap(),
    )
    .unwrap();
    source_adapter.observe().unwrap();
    let resume = source_adapter.resume_state_v1().unwrap();

    let mut changed = map_observation(90);
    changed.frame["parity_internal"]["master_deck"][0]["card_uuid"] = json!("reloaded-bane");
    changed.frame["parity_internal"]["master_deck"][1]["card_uuid"] = json!("reloaded-starter");
    changed.frame["parity_internal"]["master_deck"][0]["source"] = Value::Null;
    changed.frame["parity_internal"]["master_deck"][1]["source"] = Value::Null;
    changed.frame["parity_internal"]["master_deck"][1]["card_id"] = json!("Defend_R");
    let store = RecordedJavaEvidenceStoreV1::create_new(&root, bootstrap()).unwrap();
    let mut reattached = JavaLockstepAdapterV1::reattach_after_native_reload_with_evidence_store(
        FakeTransport {
            states: vec![changed],
            cursor: 0,
            calls: Vec::new(),
        },
        bootstrap(),
        store,
        resume,
        None,
    )
    .unwrap();
    let error = reattached.observe().unwrap_err();

    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error
        .message
        .contains("changed persisted card identity at master-deck ordinal 1"));
    assert_eq!(fs::read_dir(root.join("boundaries")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn raw_java_boundary_store_never_overwrites_an_existing_boundary() {
    let root = temp_path("raw-store-collision");
    let directory = root.join("java-evidence");
    let mut store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap()).unwrap();
    let observation = neow_observation(3, 7);
    let collision = directory
        .join("boundaries")
        .join(format!("00000000-frame-{}.json", observation.frame_id));
    fs::write(&collision, b"keep this evidence\n").unwrap();

    let error = store.append_boundary(&observation).unwrap_err();

    assert!(error.contains("failed to create recorded Java boundary"));
    assert_eq!(fs::read(&collision).unwrap(), b"keep this evidence\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn raw_java_boundary_survives_projection_failure() {
    let root = temp_path("raw-before-projection");
    let directory = root.join("java-evidence");
    let store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap()).unwrap();
    let mut observation = neow_observation(3, 7);
    observation.frame["parity_internal"]["run_state"]["unknown"] = json!(true);
    let transport = FakeTransport {
        states: vec![observation.clone()],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter =
        JavaLockstepAdapterV1::with_evidence_store(transport, bootstrap(), store).unwrap();

    let error = adapter.observe().unwrap_err();
    let recorded = RecordedJavaEvidenceStoreV1::load_boundary(
        &directory
            .join("boundaries")
            .join(format!("00000000-frame-{}.json", observation.frame_id)),
    )
    .unwrap();

    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert_eq!(recorded.observation, observation);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn absent_java_run_state_remains_unobserved() {
    let mut state = neow_observation(3, 7);
    state.frame["parity_internal"]["run_state"] = Value::Null;
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();

    assert!(matches!(
        snapshot.domains[&ParityDomainV1::Run].evidence,
        DomainEvidenceV1::Unobserved { .. }
    ));
}

#[test]
fn malformed_java_run_state_is_an_evidence_failure() {
    let mut state = neow_observation(3, 7);
    state.frame["parity_internal"]["run_state"]["unknown"] = json!(true);
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();

    let error = adapter.observe().unwrap_err();
    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error.message.contains("unknown field"));
}

#[test]
fn source_proven_java_cards_bind_without_using_current_deck_order() {
    let mut state = neow_observation(3, 8);
    state.frame["parity_internal"]["master_deck"] = json!([
        {
            "ordinal": 0,
            "card_uuid": "java-starter",
            "card_id": "Strike_R",
            "upgrades": 0,
            "source": {
                "kind": "starter_deck",
                "player_class": "IRONCLAD",
                "source_slot": 0,
            },
        },
        {
            "ordinal": 1,
            "card_uuid": "java-bane",
            "card_id": "AscendersBane",
            "upgrades": 0,
            "source": {"kind": "ascenders_bane", "rule": "ascension_10"},
        },
    ]);
    state.frame["parity_internal"]["relics"] = json!([
        {
            "index": 0,
            "relic_id": "Burning Blood",
            "counter": -1,
            "used_up": false,
        },
        {
            "index": 1,
            "relic_id": "Bottled Flame",
            "counter": -1,
            "used_up": false,
            "bound_card_uuid": "java-starter",
        },
    ]);
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();
    let DomainEvidenceV1::Observed { value: deck, .. } =
        &snapshot.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("source-proven deck should be observed")
    };

    assert_eq!(deck[0]["binding"], "natural_run/starter/0");
    assert_eq!(
        deck[1]["binding"],
        "natural_run/ascension_10/ascenders_bane"
    );
    assert!(matches!(
        snapshot.domains[&ParityDomainV1::CardBindings].evidence,
        DomainEvidenceV1::Observed { .. }
    ));
    let DomainEvidenceV1::Observed { value: relics, .. } =
        &snapshot.domains[&ParityDomainV1::Relics].evidence
    else {
        panic!("source-proven bottled relic should be observed")
    };
    assert_eq!(relics[1]["bound_card"], "natural_run/starter/0");
}

#[test]
fn java_map_projection_excludes_expired_rows_after_native_reload() {
    let make_observation = |frame_id: u64, past_room: &str| {
        let mut state = map_observation(frame_id);
        bind_test_master_deck_sources(&mut state);
        state.actions[0].action_id = "map_node:0:2".to_owned();
        state.actions[0].map_y = Some(2);
        state.actions[0].map_room_type = Some("com.megacrit.cardcrawl.rooms.ShopRoom".to_owned());
        state.frame["public_base"]["map"]["current_x"] = json!(0);
        state.frame["public_base"]["map"]["current_y"] = json!(1);
        state.frame["public_base"]["map"]["nodes"] = json!([
            {
                "x": 0,
                "y": 0,
                "room_type": past_room,
                "has_emerald_key": false,
                "edges": [{"destination_x": 0, "destination_y": 1}],
            },
            {
                "x": 0,
                "y": 1,
                "room_type": "com.megacrit.cardcrawl.rooms.RestRoom",
                "has_emerald_key": false,
                "edges": [{"destination_x": 0, "destination_y": 2}],
            },
            {
                "x": 0,
                "y": 2,
                "room_type": "com.megacrit.cardcrawl.rooms.ShopRoom",
                "has_emerald_key": false,
                "edges": [],
            },
        ]);
        state
    };
    let project = |state| {
        let mut adapter = JavaLockstepAdapterV1::new(
            FakeTransport {
                states: vec![state],
                cursor: 0,
                calls: Vec::new(),
            },
            bootstrap(),
        )
        .unwrap();
        let snapshot = adapter.observe().unwrap();
        snapshot.domains[&ParityDomainV1::Map].clone()
    };

    let before = project(make_observation(
        80,
        "com.megacrit.cardcrawl.rooms.MonsterRoom",
    ));
    let after = project(make_observation(
        81,
        "com.megacrit.cardcrawl.rooms.EventRoom",
    ));
    assert_eq!(before, after);
    let DomainEvidenceV1::Observed { value, .. } = &before.evidence else {
        panic!("active map should be observed")
    };
    let nodes = value["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["id"], "F02X00");
    assert_eq!(nodes[0]["room"], "rest");
}

#[test]
fn source_proven_java_card_cannot_regress_behind_its_bootstrap_upgrades() {
    let mut expected = bootstrap();
    expected.cards[0].upgrades = 1;

    let mut state = neow_observation(3, 9);
    bind_test_master_deck_sources(&mut state);
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, expected).unwrap();

    let error = adapter.observe().unwrap_err();
    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error.message.contains("upgrades regressed"));
}

#[test]
fn unproven_bottled_card_keeps_relics_unobserved() {
    let mut state = neow_observation(3, 8);
    state.frame["parity_internal"]["relics"][0]["relic_id"] = json!("Bottled Flame");
    state.frame["parity_internal"]["relics"][0]["bound_card_uuid"] = json!("java-starter");
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();

    assert!(matches!(
        snapshot.domains[&ParityDomainV1::Relics].evidence,
        DomainEvidenceV1::Unobserved { .. }
    ));
}

#[test]
fn non_bottled_relic_cannot_claim_a_bound_card() {
    let mut state = neow_observation(3, 8);
    state.frame["parity_internal"]["relics"][0]["bound_card_uuid"] = json!("java-starter");
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();

    let error = adapter.observe().unwrap_err();
    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error.message.contains("non-bottled relic"));
}

#[test]
fn complete_java_rng_family_uses_stable_owners_and_comparable_state() {
    let mut state = neow_observation(3, 9);
    state.frame["rng_internal"] = complete_rng_internal();
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("complete Java RNG family should be observed")
    };
    let streams = value.as_array().unwrap();

    assert_eq!(streams.len(), 8);
    assert!(streams
        .iter()
        .any(|stream| stream["owner"] == "run.monster"));
    assert!(streams.iter().all(|stream| stream["owner"] != "run.neow"));
    assert!(streams.iter().all(|stream| stream["scope"] != "floor"));
    assert_eq!(
        streams
            .iter()
            .find(|stream| stream["owner"] == "run.map")
            .unwrap()["counter"],
        Value::Null
    );
    assert!(streams
        .iter()
        .all(|stream| stream["algorithm"] == "random_xs128"));
}

#[test]
fn incomplete_java_rng_family_remains_unobserved() {
    let mut state = neow_observation(3, 9);
    state.frame["rng_internal"] = complete_rng_internal();
    state.frame["rng_internal"]["run_streams"]
        .as_array_mut()
        .unwrap()
        .retain(|stream| {
            stream["owner"] != "com.megacrit.cardcrawl.dungeons.AbstractDungeon.relicRng"
        });
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();

    assert!(matches!(
        snapshot.domains[&ParityDomainV1::RngStreams].evidence,
        DomainEvidenceV1::Unobserved { .. }
    ));
}

#[test]
fn mini_neow_does_not_require_or_retain_the_unused_neow_rng() {
    let mut state = neow_observation(3, 9);
    state.frame["rng_internal"] = complete_rng_internal();
    state.frame["rng_internal"]["run_streams"]
        .as_array_mut()
        .unwrap()
        .retain(|stream| stream["owner"] != "com.megacrit.cardcrawl.neow.NeowEvent.rng");
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();

    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("Mini Neow should observe its complete active RNG family")
    };
    assert!(value
        .as_array()
        .unwrap()
        .iter()
        .all(|stream| stream["owner"] != "run.neow"));
}

#[test]
fn rich_neow_reward_page_requires_and_retains_the_active_neow_rng() {
    let mut rich = bootstrap();
    rich.neow_eligibility = NaturalRunNeowEligibilityV1::Rich;
    let mut state = neow_observation(3, 9);
    state.frame["rng_internal"] = complete_rng_internal();
    let transport = FakeTransport {
        states: vec![state.clone()],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, rich.clone()).unwrap();
    let snapshot = adapter.observe().unwrap();
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("Rich Neow reward page should observe its active RNG")
    };
    assert!(value
        .as_array()
        .unwrap()
        .iter()
        .any(|stream| stream["owner"] == "run.neow"));

    state.frame["rng_internal"]["run_streams"]
        .as_array_mut()
        .unwrap()
        .retain(|stream| stream["owner"] != "com.megacrit.cardcrawl.neow.NeowEvent.rng");
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, rich).unwrap();
    let snapshot = adapter.observe().unwrap();
    assert!(matches!(
        snapshot.domains[&ParityDomainV1::RngStreams].evidence,
        DomainEvidenceV1::Unobserved { .. }
    ));
}

#[test]
fn complete_java_card_pools_preserve_each_original_order() {
    let mut state = neow_observation(3, 9);
    state.frame["parity_internal"]["card_pools"] = complete_card_pools();
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::CardPools].evidence
    else {
        panic!("complete Java card pools should be observed")
    };

    assert_eq!(value["common"], json!(["strike", "defend"]));
    assert_eq!(value["colorless_uncommon"], json!(["darkshackles", "trip"]));
    assert_eq!(
        value["colorless_combat"],
        json!(["trip", "panache", "darkshackles"])
    );
}

#[test]
fn incomplete_java_card_pools_remain_unobserved() {
    let mut state = neow_observation(3, 9);
    let mut pools = complete_card_pools();
    pools.as_object_mut().unwrap().remove("colorless_rare");
    state.frame["parity_internal"]["card_pools"] = pools;
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();

    assert!(matches!(
        snapshot.domains[&ParityDomainV1::CardPools].evidence,
        DomainEvidenceV1::Unobserved { .. }
    ));
}

#[test]
fn malformed_java_card_pools_are_evidence_failures() {
    let mut duplicate = complete_card_pools();
    duplicate["common"] = json!(["Strike_R", "strike"]);
    let mut unexpected = complete_card_pools();
    unexpected["future_pool"] = json!(["Feed"]);

    for (pools, expected_message) in [
        (duplicate, "repeats card"),
        (unexpected, "contains an unexpected pool"),
    ] {
        let mut state = neow_observation(3, 9);
        state.frame["parity_internal"]["card_pools"] = pools;
        let transport = FakeTransport {
            states: vec![state],
            cursor: 0,
            calls: Vec::new(),
        };
        let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();

        let error = adapter.observe().unwrap_err();
        assert_eq!(
            error.kind,
            sts_parity_lockstep::AdapterFailureKindV1::Evidence
        );
        assert!(error.message.contains(expected_message));
    }
}

#[test]
fn deterministic_java_neow_talk_pages_settle_under_one_shared_action() {
    let root = temp_path("settled-action");
    let directory = root.join("java-evidence");
    let store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap()).unwrap();
    let transport = FakeTransport {
        states: vec![
            neow_observation(0, 10),
            neow_observation(1, 11),
            neow_observation(3, 12),
        ],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter =
        JavaLockstepAdapterV1::with_evidence_store(transport, bootstrap(), store).unwrap();
    let introduction = adapter.observe().unwrap();
    let proceed = action_space(&introduction).remove(0);
    let prepared = adapter.prepare(&proceed, &introduction).unwrap();
    let reward = adapter.execute(prepared).unwrap();

    let DomainEvidenceV1::Observed { value, .. } = &reward.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("Neow room should be observed")
    };
    assert_eq!(value["stage"], "choose_reward");
    assert_eq!(adapter.into_transport().calls, ["h10", "h11"]);
    let evidence = RecordedJavaEvidenceV1::load_complete(&directory).unwrap();
    assert_eq!(evidence.steps.len(), 1);
    assert_eq!(evidence.java_action_count, 2);
    assert_eq!(evidence.steps[0].action, proceed);
    assert_eq!(
        evidence.steps[0]
            .java_actions
            .iter()
            .map(|action| action.phase)
            .collect::<Vec<_>>(),
        [
            RecordedJavaActionPhaseV1::Committed,
            RecordedJavaActionPhaseV1::InterstitialSettlement,
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_preparation_is_retained_in_zero_action_prefix_and_cannot_appear_mid_trajectory() {
    let root = temp_path("native-preparation-evidence");
    let mut store = RecordedJavaEvidenceStoreV1::create_new(&root, bootstrap()).unwrap();
    store.append_boundary(&map_observation(30)).unwrap();
    let action = SemanticActionV1::Proceed {
        surface: sts_parity_lockstep::DecisionSurfaceV1::BossRelic,
    };
    for (from, phase) in [
        (30, RecordedJavaActionPhaseV1::NativeReloadSettlement),
        (31, RecordedJavaActionPhaseV1::Committed),
    ] {
        store
            .append_action(
                phase,
                &action,
                &JavaLockstepReceiptV1 {
                    status: "completed".to_owned(),
                    request_id: format!("request-{from}"),
                    based_on_frame_id: from,
                    action_id: "proceed".to_owned(),
                    successor_frame_id: Some(from + 1),
                },
            )
            .unwrap();
        store.append_boundary(&map_observation(from + 1)).unwrap();
    }
    let zero = RecordedJavaEvidenceV1::load_prefix(&root, 0).unwrap();
    assert!(zero.steps.is_empty());
    assert_eq!(zero.java_action_count, 1);
    assert_eq!(zero.transitions[0].after.observation.frame_id, 31);
    let complete = RecordedJavaEvidenceV1::load_complete(&root).unwrap();
    assert_eq!(complete.steps.len(), 1);
    assert_eq!(complete.steps[0].before.observation.frame_id, 31);
    assert_eq!(complete.java_action_count, 2);
    store
        .append_action(
            RecordedJavaActionPhaseV1::NativeReloadSettlement,
            &action,
            &JavaLockstepReceiptV1 {
                status: "completed".to_owned(),
                request_id: "late".to_owned(),
                based_on_frame_id: 32,
                action_id: "proceed".to_owned(),
                successor_frame_id: Some(33),
            },
        )
        .unwrap();
    store.append_boundary(&map_observation(33)).unwrap();
    assert!(RecordedJavaEvidenceV1::load_complete(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn java_action_requires_a_transition_bound_rng_witness() {
    let transport = MissingRngWitnessTransport(FakeTransport {
        states: vec![map_observation(30), map_observation(31)],
        cursor: 0,
        calls: Vec::new(),
    });
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let before = adapter.observe().unwrap();
    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();

    let error = adapter.execute(prepared).unwrap_err();

    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error.message.contains("omitted its typed RNG-call witness"));
    assert!(adapter.last_rng_call_ledger_v1().is_none());
}

#[test]
fn java_action_rejects_an_incomplete_rng_witness() {
    let mut successor = map_observation(31);
    successor.frame["rng_call_witness"] = json!({
        "kind": "rng_call_witness_v1",
        "schema": NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1,
        "from_frame_id": 30,
        "to_frame_id": 31,
        "request_id": "fixture-request-30",
        "action_id": "map_node:0:0",
        "complete": false,
        "calls": [],
        "unsupported_reason": "unowned derived RNG",
    });
    let transport = FakeTransport {
        states: vec![map_observation(30), successor],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let before = adapter.observe().unwrap();
    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();

    let error = adapter.execute(prepared).unwrap_err();

    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error.message.contains("incomplete: unowned derived RNG"));
    assert!(adapter.last_rng_call_ledger_v1().is_none());
}

#[test]
fn java_action_rejects_rng_calls_outside_the_shared_contract() {
    let mut successor = map_observation(31);
    successor.frame["rng_call_witness"] = fixture_rng_witness(
        30,
        31,
        "map_node:0:0",
        vec![fixture_rng_call("run.unknown", 0, true)],
    );
    let transport = FakeTransport {
        states: vec![map_observation(30), successor],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let before = adapter.observe().unwrap();
    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();

    let error = adapter.execute(prepared).unwrap_err();

    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error.message.contains("unknown owner"));
    assert!(adapter.last_rng_call_ledger_v1().is_none());
}

#[test]
fn failed_interstitial_does_not_publish_a_partial_rng_ledger() {
    let mut talk = neow_observation(1, 11);
    talk.frame["rng_call_witness"] = fixture_rng_witness(
        10,
        11,
        "event_option:Neow:index:0:0",
        vec![fixture_rng_call("run.card", 0, true)],
    );
    let transport = MissingRngWitnessTransport(FakeTransport {
        states: vec![neow_observation(0, 10), talk, neow_observation(3, 12)],
        cursor: 0,
        calls: Vec::new(),
    });
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let before = adapter.observe().unwrap();
    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();

    let error = adapter.execute(prepared).unwrap_err();

    assert!(error.message.contains("omitted its typed RNG-call witness"));
    assert!(adapter.last_rng_call_ledger_v1().is_none());
}

#[test]
fn java_semantic_action_aggregates_interstitial_rng_ordinals() {
    let mut talk = neow_observation(1, 11);
    talk.frame["rng_call_witness"] = fixture_rng_witness(
        10,
        11,
        "event_option:Neow:index:0:0",
        vec![fixture_rng_call("run.card", 0, true)],
    );
    let mut reward = neow_observation(3, 12);
    reward.frame["rng_call_witness"] = fixture_rng_witness(
        11,
        12,
        "event_option:Neow:index:1:0",
        vec![fixture_rng_call("run.card", 0, false)],
    );
    let transport = FakeTransport {
        states: vec![neow_observation(0, 10), talk, reward],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let before = adapter.observe().unwrap();
    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();

    adapter.execute(prepared).unwrap();

    let ledger = adapter.last_rng_call_ledger_v1().unwrap();
    assert_eq!(ledger.0.len(), 2);
    assert_eq!(ledger.0[0].owner_ordinal, 0);
    assert_eq!(ledger.0[1].owner_ordinal, 1);
}

#[test]
fn stale_java_observation_is_rejected_before_transport_mutation() {
    let transport = FakeTransport {
        states: vec![neow_observation(3, 20)],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();
    let action = action_space(&snapshot).remove(0);
    let mut foreign = snapshot.clone();
    foreign.observation_id.push_str("-foreign");

    let error = adapter.prepare(&action, &foreign).unwrap_err();
    assert!(error.message.contains("stale or foreign"));
    assert!(adapter.into_transport().calls.is_empty());
}

#[test]
fn java_map_actions_use_shared_floor_coordinate_identity() {
    let transport = FakeTransport {
        states: vec![map_observation(30)],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let snapshot = adapter.observe().unwrap();
    let actions = action_space(&snapshot);
    let encoded = serde_json::to_value(&actions[0]).unwrap();

    assert_eq!(snapshot.boundary_kind, "natural_map");
    assert_eq!(encoded["choice"]["id"], "F01X00");
    assert_eq!(encoded["choice"]["attributes"]["room"], "monster");
}

#[test]
fn java_bonfire_grid_maps_runtime_uuid_to_one_typed_deck_card_submission() {
    let grid_id =
        "grid_select:BonfireElementals:MasterDeck:DeckRemove:1:false:false:java-starter:selected:";
    let raw_action = json!({
        "action_id": format!("submit_grid_select:{grid_id}"),
        "kind": "submit_grid_select",
        "grid_select_id": grid_id,
        "source_pile": "MasterDeck",
        "selection_reason": "DeckRemove",
        "event_id": "BonfireElementals",
        "parameter_schema": {
            "field": "selected_card_uuids",
            "item_format": "uuid",
            "ordered": true,
            "unique_items": true,
            "allowed_card_uuids": ["java-starter"],
            "allowed_counts": [1],
        },
    });
    let mut grid = observation(
        50,
        json!({
            "kind": "grid_select",
            "event_id": "BonfireElementals",
            "event_screen_index": 1,
            "event_screen_key": "CHOOSE",
            "event_identity": {
                "status": "supported",
                "event_id": "BonfireElementals",
                "runtime_class": "com.megacrit.cardcrawl.events.shrines.Bonfire",
                "unsupported_reason": null,
            },
            "grid_select": {
                "grid_select_id": grid_id,
                "source_pile": "MasterDeck",
                "selection_reason": "DeckRemove",
                "requested_cards": 1,
                "any_number": false,
                "can_cancel": false,
                "allowed_counts": [1],
                "confirm_supported": false,
                "cancel_supported": false,
                "candidate_cards": [{
                    "index": 0,
                    "card_uuid": "java-starter",
                    "card_id": "Strike_R",
                    "name": "Strike",
                    "upgrades": 0,
                    "cost_for_turn": 1,
                }],
                "selected_cards": [],
            },
        }),
        vec![JavaRuntimeActionV1 {
            runtime_handle: "grid-handle".to_owned(),
            action_id: format!("submit_grid_select:{grid_id}"),
            kind: "submit_grid_select".to_owned(),
            event_id: Some("BonfireElementals".to_owned()),
            option_index: None,
            neow_reward_type: None,
            neow_drawback: None,
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: None,
            reward_index: None,
            reward_id: None,
            reward_gold: None,
            potion_slot: None,
            potion_id: None,
            parameters_required: true,
        }],
    );
    grid.frame["action_space"] = json!({"actions": [raw_action]});
    grid.frame["public_base"]["map"]["current_x"] = json!(0);
    grid.frame["public_base"]["map"]["current_y"] = json!(0);
    grid.frame["public_base"]["map"]["nodes"][0]["room_type"] =
        json!("com.megacrit.cardcrawl.rooms.EventRoom");
    grid.frame["parity_internal"]["master_deck"][0]["source"] =
        json!({"kind": "ascenders_bane", "rule": "ascension_10"});
    grid.frame["parity_internal"]["master_deck"][1]["source"] = json!({
        "kind": "starter_deck",
        "player_class": "IRONCLAD",
        "source_slot": 0,
    });
    grid.frame["rng_internal"] = complete_rng_internal();

    let mut complete = grid.clone();
    complete.frame_id = 51;
    complete.frame["frame_id"] = json!(51);
    complete.frame["public_base"]["decision"] = json!({
        "kind": "event",
        "event_id": "BonfireElementals",
        "event_screen_index": 2,
        "event_identity": {
            "status": "supported",
            "event_id": "BonfireElementals",
            "runtime_class": "com.megacrit.cardcrawl.events.shrines.Bonfire",
            "unsupported_reason": null,
        },
    });
    complete.frame["parity_internal"]["master_deck"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    complete.frame["action_space"] = Value::Null;
    complete.actions = vec![JavaRuntimeActionV1 {
        runtime_handle: "leave-handle".to_owned(),
        action_id: "event_option:BonfireElementals:index:2:0".to_owned(),
        kind: "choose_event_option".to_owned(),
        event_id: Some("BonfireElementals".to_owned()),
        option_index: Some(0),
        neow_reward_type: None,
        neow_drawback: None,
        map_x: None,
        map_y: None,
        map_room_type: None,
        reward_type: None,
        reward_index: None,
        reward_id: None,
        reward_gold: None,
        potion_slot: None,
        potion_id: None,
        parameters_required: false,
    }];

    let transport = FakeTransport {
        states: vec![grid, complete],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
    let before = adapter.observe().unwrap();
    assert_eq!(before.boundary_kind, "natural_grid_select");
    let actions = action_space(&before);
    let [action] = actions.as_slice() else {
        panic!("Bonfire grid should expose one removable card")
    };
    let SemanticActionV1::SubmitSelection { surface, choices } = action else {
        panic!("Bonfire grid should expose a shared selection action")
    };
    assert_eq!(*surface, sts_parity_lockstep::DecisionSurfaceV1::GridSelect);
    assert_eq!(choices[0].id, "natural_run/starter/0");
    assert_eq!(
        choices[0].attributes["card_id"],
        SemanticScalarV1::Text("strike".to_owned())
    );
    let prepared = adapter.prepare(action, &before).unwrap();
    let after = adapter.execute(prepared).unwrap();
    assert_eq!(after.boundary_kind, "natural_event");
    assert_eq!(
        adapter.into_transport().calls,
        ["grid-handle|grid=java-starter"]
    );
}

#[test]
fn java_neow_transform_derives_and_resumes_the_replacement_card_binding() {
    let root = temp_path("neow-transform-resume");
    let mut rich = bootstrap();
    rich.neow_eligibility = NaturalRunNeowEligibilityV1::Rich;
    let grid = neow_deck_grid_observation(50, "DeckTransform");
    let mut successor = neow_observation(99, 51);
    bind_test_master_deck_sources(&mut successor);
    successor.frame["parity_internal"]["master_deck"][1] = json!({
        "ordinal": 1,
        "card_uuid": "java-transformed",
        "card_id": "Inflame",
        "upgrades": 0,
        "source": null,
    });
    successor.frame["rng_internal"] = complete_rng_internal();
    let transport = FakeTransport {
        states: vec![grid, successor],
        cursor: 0,
        calls: Vec::new(),
    };
    let evidence = RecordedJavaEvidenceStoreV1::create_new(&root, rich.clone()).unwrap();
    let mut adapter =
        JavaLockstepAdapterV1::with_evidence_store(transport, rich.clone(), evidence).unwrap();

    let before = adapter.observe().unwrap();
    assert_eq!(before.boundary_kind, "natural_neow_grid_select");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &before.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("Neow transform room should be observed")
    };
    assert_eq!(room["kind"], "neow_selection");
    assert_eq!(room["current"], Value::Null);
    assert_eq!(room["event_id"], "neow");
    assert_eq!(room["screen"], 2);
    assert_eq!(room["selection_kind"], "deck_transform");
    let DomainEvidenceV1::Observed { value: rng, .. } =
        &before.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("Neow transform RNG should be observed")
    };
    assert!(rng
        .as_array()
        .unwrap()
        .iter()
        .any(|stream| stream["owner"] == "run.neow"));
    assert!(rng
        .as_array()
        .unwrap()
        .iter()
        .all(|stream| stream["scope"] != "floor"));

    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();
    let after = adapter.execute(prepared).unwrap();
    let deck = match &after.domains[&ParityDomainV1::MasterDeck].evidence {
        DomainEvidenceV1::Observed { value, .. } => value,
        _ => panic!("transform successor deck should remain observed"),
    };
    let transformed = deck
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["binding"] == "natural_run/starter/0")
        .expect("replacement card should retain the selected stable binding");
    assert_eq!(transformed["card_id"], "inflame");
    let relations = match &after.domains[&ParityDomainV1::CardBindings].evidence {
        DomainEvidenceV1::Observed { value, .. } => value,
        _ => panic!("transform successor bindings should remain observed"),
    };
    assert!(relations.as_array().unwrap().iter().any(|relation| {
        relation["binding"] == "natural_run/starter/0"
            && relation["source"]["kind"] == "starter_deck"
    }));

    let resume = adapter.resume_state_v1().unwrap();
    let transport = adapter.into_transport();
    let evidence = RecordedJavaEvidenceStoreV1::open_append(&root).unwrap();
    let mut resumed =
        JavaLockstepAdapterV1::resume_with_evidence_store(transport, rich, evidence, resume)
            .unwrap();
    let resumed_after = resumed.observe().unwrap();
    assert_eq!(
        resumed_after.domains[&ParityDomainV1::MasterDeck],
        after.domains[&ParityDomainV1::MasterDeck]
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn java_neow_upgrade_keeps_the_source_binding_and_requires_the_complete_screen() {
    let mut rich = bootstrap();
    rich.neow_eligibility = NaturalRunNeowEligibilityV1::Rich;
    let grid = neow_deck_grid_observation(50, "DeckUpgrade");
    let mut complete = neow_observation(99, 51);
    bind_test_master_deck_sources(&mut complete);
    complete.frame["parity_internal"]["master_deck"][1]["upgrades"] = json!(1);
    complete.frame["rng_internal"] = complete_rng_internal();
    let transport = FakeTransport {
        states: vec![grid.clone(), complete],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, rich.clone()).unwrap();
    let before = adapter.observe().unwrap();
    assert_eq!(before.boundary_kind, "natural_neow_grid_select");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &before.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("Neow upgrade selection should be observed")
    };
    assert_eq!(room["screen"], 2);
    assert_eq!(room["selection_kind"], "deck_upgrade");
    let action = action_space(&before).remove(0);
    let prepared = adapter.prepare(&action, &before).unwrap();
    let after = adapter.execute(prepared).unwrap();
    assert_eq!(after.boundary_kind, "natural_neow");
    let DomainEvidenceV1::Observed { value: deck, .. } =
        &after.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("Neow upgraded deck should be observed")
    };
    assert_eq!(deck[1]["binding"], "natural_run/starter/0");
    assert_eq!(deck[1]["card_id"], "strike");
    assert_eq!(deck[1]["upgrades"], 1);
    assert_eq!(
        adapter.into_transport().calls,
        ["neow-deck-grid|grid=java-starter"]
    );

    for (field, invalid) in [
        ("event_screen_index", json!(3)),
        ("event_screen_index", json!(2)),
        (
            "grid_select",
            json!({"source_pile": "Draw", "selection_reason": "DeckUpgrade"}),
        ),
    ] {
        let mut invalid_grid = grid.clone();
        invalid_grid.frame["public_base"]["decision"][field] = invalid;
        let transport = FakeTransport {
            states: vec![invalid_grid],
            cursor: 0,
            calls: Vec::new(),
        };
        let mut adapter = JavaLockstepAdapterV1::new(transport, rich.clone()).unwrap();
        assert!(adapter.observe().is_err());
        assert!(adapter.into_transport().calls.is_empty());
    }
}

#[test]
fn java_campfire_rest_settles_only_the_completed_room_page() {
    let campfire = campfire_observation(
        60,
        "campfire",
        vec![parameter_free_action("begin_campfire_rest", "rest-handle")],
    );
    let complete = campfire_observation(
        61,
        "campfire_complete",
        vec![parameter_free_action("proceed_rest_room", "proceed-handle")],
    );
    let map = map_observation(62);
    let transport = FakeTransport {
        states: vec![campfire, complete, map],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();

    let before = adapter.observe().unwrap();
    assert_eq!(before.boundary_kind, "natural_campfire");
    let rest = action_space(&before)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::Select { choice, .. } if choice.id == "rest"))
        .unwrap();
    let prepared = adapter.prepare(&rest, &before).unwrap();
    let after = adapter.execute(prepared).unwrap();

    assert_eq!(after.boundary_kind, "natural_map");
    assert_eq!(
        adapter.into_transport().calls,
        ["rest-handle", "proceed-handle"]
    );
}

#[test]
fn java_campfire_smith_keeps_card_choice_and_settles_confirmation_and_completion() {
    let mut campfire = campfire_observation(
        70,
        "campfire",
        vec![parameter_free_action(
            "begin_campfire_smith",
            "smith-handle",
        )],
    );
    bind_test_master_deck_sources(&mut campfire);

    let choose_raw = smith_raw_action("smith_card", "choose_smith_card");
    let mut choose = campfire_observation(
        71,
        "smith_card",
        vec![parameter_free_action("choose_smith_card", "choose-handle")],
    );
    choose.actions[0].action_id = choose_raw["action_id"].as_str().unwrap().to_owned();
    choose.frame["action_space"] = json!({"actions": [choose_raw]});
    bind_test_master_deck_sources(&mut choose);

    let confirm_raw = smith_raw_action("confirm_smith_card", "confirm_smith_card");
    let mut confirm = campfire_observation(
        72,
        "smith_confirm",
        vec![parameter_free_action(
            "confirm_smith_card",
            "confirm-handle",
        )],
    );
    confirm.actions[0].action_id = confirm_raw["action_id"].as_str().unwrap().to_owned();
    confirm.frame["action_space"] = json!({"actions": [confirm_raw]});
    bind_test_master_deck_sources(&mut confirm);

    let mut complete = campfire_observation(
        73,
        "campfire_complete",
        vec![parameter_free_action("proceed_rest_room", "proceed-handle")],
    );
    bind_test_master_deck_sources(&mut complete);
    complete.frame["parity_internal"]["master_deck"][1]["upgrades"] = json!(1);

    let mut map = map_observation(74);
    bind_test_master_deck_sources(&mut map);
    map.frame["parity_internal"]["master_deck"][1]["upgrades"] = json!(1);
    let transport = FakeTransport {
        states: vec![campfire, choose, confirm, complete, map],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();

    let initial = adapter.observe().unwrap();
    let smith = action_space(&initial)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::Select { choice, .. } if choice.id == "smith"))
        .unwrap();
    let prepared = adapter.prepare(&smith, &initial).unwrap();
    let selection = adapter.execute(prepared).unwrap();
    assert_eq!(selection.boundary_kind, "natural_campfire_smith");

    let card = action_space(&selection)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select { choice, .. }
                    if choice.kind == "deck_card" && choice.id == "natural_run/starter/0"
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&card, &selection).unwrap();
    let after = adapter.execute(prepared).unwrap();

    assert_eq!(after.boundary_kind, "natural_map");
    let DomainEvidenceV1::Observed { value: deck, .. } =
        &after.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("Smith successor deck should be observed")
    };
    assert_eq!(deck[1]["binding"], "natural_run/starter/0");
    assert_eq!(deck[1]["upgrades"], 1);
    assert_eq!(
        adapter.into_transport().calls,
        [
            "smith-handle",
            "choose-handle",
            "confirm-handle",
            "proceed-handle"
        ]
    );
}

#[test]
fn oracle_lifecycle_control_is_not_projected_as_a_neow_or_map_choice() {
    for (mut state, expected_actions) in [(neow_observation(3, 31), 2), (map_observation(32), 1)] {
        state.actions.push(lifecycle_action(
            "run_discard_to_main_menu",
            "discard-handle",
        ));
        let transport = FakeTransport {
            states: vec![state],
            cursor: 0,
            calls: Vec::new(),
        };
        let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();

        let snapshot = adapter.observe().unwrap();

        assert_eq!(action_space(&snapshot).len(), expected_actions);
    }
}

#[test]
fn malformed_or_repeated_oracle_lifecycle_control_is_rejected() {
    let mut wrong_id = lifecycle_action("run_discard_to_main_menu", "discard-handle");
    wrong_id.action_id = "not-the-kind".to_owned();
    let mut carries_player_identity =
        lifecycle_action("run_discard_to_main_menu", "discard-handle");
    carries_player_identity.map_x = Some(0);

    for lifecycle_actions in [
        vec![wrong_id],
        vec![carries_player_identity],
        vec![
            lifecycle_action("run_discard_to_main_menu", "discard-one"),
            lifecycle_action("run_discard_to_main_menu", "discard-two"),
        ],
    ] {
        let mut state = map_observation(33);
        state.actions.extend(lifecycle_actions);
        let transport = FakeTransport {
            states: vec![state],
            cursor: 0,
            calls: Vec::new(),
        };
        let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();
        let error = adapter.observe().unwrap_err();

        assert_eq!(
            error.kind,
            sts_parity_lockstep::AdapterFailureKindV1::Evidence
        );
        assert!(error.message.contains("invalid oracle lifecycle action"));
    }
}

#[test]
fn malformed_observed_map_is_evidence_failure_not_an_unobserved_gap() {
    let mut state = map_observation(40);
    state.frame["public_base"]["map"]["nodes"][0]["edges"] = json!([
        {"destination_x": 3, "destination_y": 16},
        {"destination_x": 3, "destination_y": 16},
    ]);
    let transport = FakeTransport {
        states: vec![state],
        cursor: 0,
        calls: Vec::new(),
    };
    let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap()).unwrap();

    let error = adapter.observe().unwrap_err();
    assert_eq!(
        error.kind,
        sts_parity_lockstep::AdapterFailureKindV1::Evidence
    );
    assert!(error.message.contains("repeats an edge"));
}

#[test]
fn java_event_grant_construction_sources_distinguish_same_named_cards() {
    for (event, card, count, limit) in [
        ("ghosts", "Ghostly", 3, 5),
        ("windinghalls", "Madness", 2, 2),
        ("windinghalls", "Writhe", 1, 2),
        ("goldenshrine", "Regret", 1, 1),
        ("drugdealer", "J.A.X.", 1, 1),
    ] {
        let mut frame = map_observation(50);
        bind_test_master_deck_sources(&mut frame);
        let deck = frame.frame["parity_internal"]["master_deck"]
            .as_array_mut()
            .unwrap();
        for slot in 0..count {
            deck.push(json!({"ordinal":deck.len(),"card_uuid":format!("ghost-{slot}"),"card_id":card,"upgrades":0,"source":{"kind":"event_grant","event_id":event,"room_id":"F05X03","product_slot":slot}}));
        }
        let observe = |frame: JavaRuntimeObservationV1| {
            JavaLockstepAdapterV1::new(
                FakeTransport {
                    states: vec![frame],
                    cursor: 0,
                    calls: Vec::new(),
                },
                bootstrap(),
            )
            .unwrap()
            .observe()
        };
        let snapshot = observe(frame.clone()).unwrap();
        let DomainEvidenceV1::Observed { value, .. } =
            &snapshot.domains[&ParityDomainV1::CardBindings].evidence
        else {
            panic!()
        };
        let sources = value
            .as_array()
            .unwrap()
            .iter()
            .filter(|bound| bound["source"]["kind"] == "event_grant")
            .collect::<Vec<_>>();
        assert_eq!(sources.len(), count);
        for slot in 0..count {
            assert!(sources
                .iter()
                .any(|bound| bound["source"]["product_slot"] == slot));
        }
        for fault in 0..4 {
            let mut invalid = frame.clone();
            let last = invalid.frame["parity_internal"]["master_deck"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap();
            match fault {
                0 => last["source"]["product_slot"] = json!(limit),
                1 => {
                    last.as_object_mut().unwrap().remove("source");
                }
                2 => last["source"]["room_id"] = json!("not-a-room"),
                _ => last["source"]["event_id"] = json!("unrelated"),
            }
            if fault == 1 {
                let incomplete = observe(invalid).unwrap();
                for domain in [ParityDomainV1::MasterDeck, ParityDomainV1::CardBindings] {
                    assert!(matches!(
                        incomplete.domains[&domain].evidence,
                        DomainEvidenceV1::Unobserved { .. }
                    ));
                }
            } else {
                assert!(observe(invalid).is_err(), "fault {fault}");
            }
        }
        let mut duplicate = frame.clone();
        let deck = duplicate.frame["parity_internal"]["master_deck"]
            .as_array_mut()
            .unwrap();
        let mut extra = deck.last().unwrap().clone();
        extra["card_uuid"] = json!("extra-instance");
        extra["ordinal"] = json!(deck.len());
        deck.push(extra);
        assert!(observe(duplicate).is_err());
    }
}

#[test]
fn bottled_card_selection_executes_without_event_or_card_transform() {
    use sts_parity_lockstep::DecisionSurfaceV1;
    let mut before = map_observation(1);
    bind_test_master_deck_sources(&mut before);
    before.frame["public_base"]["map"]["current_x"] = json!(0);
    before.frame["public_base"]["map"]["current_y"] = json!(0);
    before.frame["rng_internal"] = complete_rng_internal();
    let id = "grid_select:none:MasterDeck:BottleFlame:1:false:false:java-starter:selected:";
    let action_id = format!("submit_grid_select:{id}");
    before.frame["public_base"]["decision"] = json!({
        "kind":"grid_select", "event_id":null,
        "grid_select":{"grid_select_id":id,"source_pile":"MasterDeck","selection_reason":"BottleFlame",
            "requested_cards":1,"any_number":false,"can_cancel":false,"allowed_counts":[1],
            "confirm_supported":false,"cancel_supported":false,"selected_cards":[],
            "candidate_cards":[{"index":0,"card_uuid":"java-starter","card_id":"Strike_R","upgrades":0}]}
    });
    before.frame["action_space"]["actions"] = json!([{
        "action_id":action_id,"kind":"submit_grid_select","grid_select_id":id,
        "source_pile":"MasterDeck","selection_reason":"BottleFlame","event_id":null,
        "parameter_schema":{"field":"selected_card_uuids","item_format":"uuid","ordered":true,
            "unique_items":true,"allowed_counts":[1],"allowed_card_uuids":["java-starter"]}
    }]);
    before.actions =
        vec![
            serde_json::from_value(json!({"runtime_handle":"bottle","action_id":action_id,
        "kind":"submit_grid_select","parameters_required":true}))
            .unwrap(),
        ];
    before.frame["parity_internal"]["relics"] = json!([{"index":0,"relic_id":"Bottled Flame","counter":-1,"used_up":false,"bound_card_uuid":null}]);
    let mut after = map_observation(2);
    bind_test_master_deck_sources(&mut after);
    after.frame["public_base"]["map"]["current_x"] = json!(0);
    after.frame["public_base"]["map"]["current_y"] = json!(0);
    after.frame["rng_internal"] = complete_rng_internal();
    after.frame["parity_internal"]["relics"] = before.frame["parity_internal"]["relics"].clone();
    after.frame["parity_internal"]["relics"][0]["bound_card_uuid"] = json!("java-starter");
    let mut adapter = JavaLockstepAdapterV1::new(
        FakeTransport {
            states: vec![before, after],
            cursor: 0,
            calls: vec![],
        },
        bootstrap(),
    )
    .unwrap();
    let before = adapter.observe().unwrap();
    let action = SemanticActionV1::SubmitSelection {
        surface: DecisionSurfaceV1::GridSelect,
        choices: vec![sts_parity_lockstep::natural_deck_card_choice_v1(
            "natural_run/starter/0",
            "strike",
            0,
        )
        .unwrap()],
    };
    let prepared = adapter.prepare(&action, &before).unwrap();
    let after = adapter.execute(prepared).unwrap();
    assert_eq!(
        before.domains[&ParityDomainV1::MasterDeck],
        after.domains[&ParityDomainV1::MasterDeck]
    );
    let DomainEvidenceV1::Observed { value, .. } = &after.domains[&ParityDomainV1::Relics].evidence
    else {
        panic!("unobserved bottle")
    };
    assert_eq!(value[0]["bound_card"], "natural_run/starter/0");
}

fn bootstrap() -> NaturalRunBootstrapV1 {
    NaturalRunBootstrapV1::new(
        "2026090301",
        20,
        true,
        NaturalRunContentProfileV1::AllUnlocked,
        NaturalRunNeowEligibilityV1::Mini,
        "IRONCLAD",
        vec![
            NaturalRunBootstrapCardV1 {
                binding: "natural_run/starter/0".to_owned(),
                source: NaturalRunCardSourceV1::StarterDeck {
                    player_class: "IRONCLAD".to_owned(),
                    source_slot: 0,
                },
                card_id: "strike".to_owned(),
                upgrades: 0,
            },
            NaturalRunBootstrapCardV1 {
                binding: "natural_run/ascension_10/ascenders_bane".to_owned(),
                source: NaturalRunCardSourceV1::AscendersBane {
                    rule: "ascension_10".to_owned(),
                },
                card_id: "ascendersbane".to_owned(),
                upgrades: 0,
            },
        ],
    )
    .unwrap()
}

fn complete_card_pools() -> Value {
    json!({
        "common": ["Strike_R", "Defend_R"],
        "uncommon": ["Inflame"],
        "rare": ["Feed"],
        "colorless_uncommon": ["Dark Shackles", "Trip"],
        "colorless_rare": ["The Bomb", "Panache"],
        "colorless_combat": ["Trip", "Panache", "Dark Shackles"],
    })
}

fn neow_deck_grid_observation(frame_id: u64, reason: &str) -> JavaRuntimeObservationV1 {
    let grid_id =
        format!("grid_select:Neow:MasterDeck:{reason}:1:false:false:java-starter:selected:");
    let raw_action = json!({
        "action_id": format!("submit_grid_select:{grid_id}"),
        "kind": "submit_grid_select",
        "grid_select_id": grid_id,
        "source_pile": "MasterDeck",
        "selection_reason": reason,
        "event_id": "Neow",
        "parameter_schema": {
            "field": "selected_card_uuids",
            "item_format": "uuid",
            "ordered": true,
            "unique_items": true,
            "allowed_card_uuids": ["java-starter"],
            "allowed_counts": [1],
        },
    });
    let mut observation = observation(
        frame_id,
        json!({
            "kind": "grid_select",
            "event_id": "Neow",
            "event_screen_index": 99,
            "event_screen_key": "COMPLETE",
            "event_identity": {
                "status": "supported",
                "event_id": "Neow",
                "runtime_class": "com.megacrit.cardcrawl.neow.NeowEvent",
                "unsupported_reason": null,
            },
            "grid_select": {
                "grid_select_id": grid_id,
                "source_pile": "MasterDeck",
                "selection_reason": reason,
                "requested_cards": 1,
                "any_number": false,
                "can_cancel": false,
                "allowed_counts": [1],
                "confirm_supported": false,
                "cancel_supported": false,
                "candidate_cards": [{
                    "index": 0,
                    "card_uuid": "java-starter",
                    "card_id": "Strike_R",
                    "name": "Strike",
                    "upgrades": 0,
                    "cost_for_turn": 1,
                }],
                "selected_cards": [],
            },
        }),
        vec![JavaRuntimeActionV1 {
            runtime_handle: "neow-deck-grid".to_owned(),
            action_id: format!("submit_grid_select:{grid_id}"),
            kind: "submit_grid_select".to_owned(),
            event_id: Some("Neow".to_owned()),
            option_index: None,
            neow_reward_type: None,
            neow_drawback: None,
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: None,
            reward_index: None,
            reward_id: None,
            reward_gold: None,
            potion_slot: None,
            potion_id: None,
            parameters_required: true,
        }],
    );
    observation.frame["action_space"] = json!({"actions": [raw_action]});
    bind_test_master_deck_sources(&mut observation);
    observation.frame["rng_internal"] = complete_rng_internal();
    observation
}

fn neow_observation(screen: i64, frame_id: u64) -> JavaRuntimeObservationV1 {
    let actions = match screen {
        0 | 1 | 99 => vec![JavaRuntimeActionV1 {
            runtime_handle: format!("h{frame_id}"),
            action_id: format!("event_option:Neow:index:{screen}:0"),
            kind: "choose_event_option".to_owned(),
            event_id: Some("Neow".to_owned()),
            option_index: Some(0),
            neow_reward_type: None,
            neow_drawback: None,
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: None,
            reward_index: None,
            reward_id: None,
            reward_gold: None,
            potion_slot: None,
            potion_id: None,
            parameters_required: false,
        }],
        3 => [
            ("HUNDRED_GOLD", "NONE"),
            ("ONE_RARE_RELIC", "TEN_PERCENT_HP_LOSS"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (reward, drawback))| JavaRuntimeActionV1 {
            runtime_handle: format!("h{frame_id}-{index}"),
            action_id: format!("event_option:Neow:index:3:{index}"),
            kind: "choose_event_option".to_owned(),
            event_id: Some("Neow".to_owned()),
            option_index: Some(index),
            neow_reward_type: Some(reward.to_owned()),
            neow_drawback: Some(drawback.to_owned()),
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: None,
            reward_index: None,
            reward_id: None,
            reward_gold: None,
            potion_slot: None,
            potion_id: None,
            parameters_required: false,
        })
        .collect(),
        _ => panic!("unsupported test screen"),
    };
    observation(
        frame_id,
        json!({
            "kind": "event",
            "event_id": "Neow",
            "event_screen_index": screen,
        }),
        actions,
    )
}

fn map_observation(frame_id: u64) -> JavaRuntimeObservationV1 {
    observation(
        frame_id,
        json!({"kind": "map"}),
        vec![JavaRuntimeActionV1 {
            runtime_handle: format!("h{frame_id}"),
            action_id: "map_node:0:0".to_owned(),
            kind: "choose_map_node".to_owned(),
            event_id: None,
            option_index: None,
            neow_reward_type: None,
            neow_drawback: None,
            map_x: Some(0),
            map_y: Some(0),
            map_room_type: Some("com.megacrit.cardcrawl.rooms.MonsterRoom".to_owned()),
            reward_type: None,
            reward_index: None,
            reward_id: None,
            reward_gold: None,
            potion_slot: None,
            potion_id: None,
            parameters_required: false,
        }],
    )
}

fn lifecycle_action(kind: &str, runtime_handle: &str) -> JavaRuntimeActionV1 {
    JavaRuntimeActionV1 {
        runtime_handle: runtime_handle.to_owned(),
        action_id: kind.to_owned(),
        kind: kind.to_owned(),
        event_id: None,
        option_index: None,
        neow_reward_type: None,
        neow_drawback: None,
        map_x: None,
        map_y: None,
        map_room_type: None,
        reward_type: None,
        reward_index: None,
        reward_id: None,
        reward_gold: None,
        potion_slot: None,
        potion_id: None,
        parameters_required: false,
    }
}

fn parameter_free_action(kind: &str, runtime_handle: &str) -> JavaRuntimeActionV1 {
    lifecycle_action(kind, runtime_handle)
}

fn campfire_observation(
    frame_id: u64,
    decision_kind: &str,
    actions: Vec<JavaRuntimeActionV1>,
) -> JavaRuntimeObservationV1 {
    let mut observation = observation(frame_id, json!({"kind": decision_kind}), actions);
    observation.frame["public_base"]["map"]["current_x"] = json!(0);
    observation.frame["public_base"]["map"]["current_y"] = json!(0);
    observation.frame["public_base"]["map"]["nodes"][0]["room_type"] =
        json!("com.megacrit.cardcrawl.rooms.RestRoom");
    observation.frame["rng_internal"] = complete_rng_internal();
    observation
}

fn bind_test_master_deck_sources(observation: &mut JavaRuntimeObservationV1) {
    observation.frame["parity_internal"]["master_deck"][0]["source"] =
        json!({"kind": "ascenders_bane", "rule": "ascension_10"});
    observation.frame["parity_internal"]["master_deck"][1]["source"] = json!({
        "kind": "starter_deck",
        "player_class": "IRONCLAD",
        "source_slot": 0,
    });
}

fn smith_raw_action(action_prefix: &str, kind: &str) -> Value {
    json!({
        "action_id": format!("{action_prefix}:1:java-starter:Strike_R:0"),
        "kind": kind,
        "card_ordinal": 1,
        "card_uuid": "java-starter",
        "card_id": "Strike_R",
        "card_upgrades": 0,
    })
}

fn observation(
    frame_id: u64,
    decision: Value,
    actions: Vec<JavaRuntimeActionV1>,
) -> JavaRuntimeObservationV1 {
    JavaRuntimeObservationV1 {
        producer: sts_parity_lockstep::ProducerIdentityV1 {
            implementation: "communicationmod".to_owned(),
            version: "test-mod".to_owned(),
            session_epoch: "test-epoch".to_owned(),
        },
        frame_id,
        frame: json!({
            "kind": "state_frame",
            "profile": "parity_run_control",
            "frame_id": frame_id,
            "boundary": {"kind": "run_decision", "stable": true},
            "scenario": null,
            "public_base": {
                "run": {
                    "ascension": 20,
                    "character": "IRONCLAD",
                    "act": 1,
                    "floor": 0,
                    "current_hp": 68,
                    "max_hp": 75,
                    "gold": 99,
                    "final_act_available": true,
                    "has_ruby_key": false,
                    "has_sapphire_key": false,
                    "has_emerald_key": false,
                },
                "decision": decision,
                "map": {
                    "current_x": -1,
                    "current_y": -1,
                    "boss": "TheGuardian",
                    "nodes": [{
                        "x": 0,
                        "y": 0,
                        "room_type": "com.megacrit.cardcrawl.rooms.MonsterRoom",
                        "has_emerald_key": false,
                        "edges": [{"destination_x": 3, "destination_y": 16}],
                    }],
                },
            },
            "parity_internal": {
                "run_seed": "2026090301",
                "content_profile": "all_unlocked",
                "run_state": {
                    "daily": false,
                    "shop_purge_base_cost": 75,
                    "drop_state": {
                        "potion_chance_modifier": 0,
                        "card_rarity_offset": 5,
                        "card_upgrade_chance_bits": 0,
                    },
                    "room_flags": {"mugged": false, "smoked": false},
                    "relic_pools": {
                        "common": ["Akabeko", "Anchor"],
                        "uncommon": ["Blue Candle"],
                        "rare": ["Calipers"],
                        "shop": ["Cauldron"],
                        "boss": ["Astrolabe"],
                    },
                    "encounter_schedule": {
                        "normal": ["Jaw Worm", "Cultist", "Jaw Worm"],
                        "elite": ["Gremlin Nob"],
                        "boss_key": "The Guardian",
                        "boss_queue": ["The Guardian"],
                    },
                    "event_schedule": {
                        "events": ["Big Fish", "Cleric"],
                        "shrines": ["Golden Shrine"],
                        "one_time_events": ["Duplicator"],
                        "chance_bits": {
                            "elite": 0,
                            "monster": 1036831949,
                            "shop": 1022739087,
                            "treasure": 1017370378,
                            "shrine": 1048576000,
                        },
                    },
                },
                "master_deck": [
                    {
                        "ordinal": 0,
                        "card_uuid": "java-bane",
                        "card_id": "AscendersBane",
                        "upgrades": 0,
                    },
                    {
                        "ordinal": 1,
                        "card_uuid": "java-starter",
                        "card_id": "Strike_R",
                        "upgrades": 0,
                    },
                ],
                "relics": [{
                    "index": 0,
                    "relic_id": "Burning Blood",
                    "counter": -1,
                    "used_up": false,
                }],
                "potions": [
                    {"slot_index": 0, "empty": true},
                    {"slot_index": 1, "empty": true},
                    {"slot_index": 2, "empty": true},
                ],
            },
        }),
        actions,
    }
}

fn complete_rng_internal() -> Value {
    let stream = |owner: &str, scope: &str, counter: u32| {
        json!({
            "owner": owner,
            "algorithm": "com.badlogic.gdx.math.RandomXS128",
            "scope": scope,
            "state_hex": [
                format!("{counter:016X}"),
                format!("{:016X}", counter + 1),
            ],
            "counter": counter,
        })
    };
    json!({
        "streams": [
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.aiRng", "floor", 1),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.shuffleRng", "floor", 2),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRandomRng", "floor", 3),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.miscRng", "floor", 4),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterHpRng", "floor", 5),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.potionRng", "run", 6),
        ],
        "run_streams": [
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterRng", "run", 7),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.eventRng", "run", 8),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.merchantRng", "run", 9),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng", "run", 10),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRng", "run", 11),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.treasureRng", "run", 12),
            stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.relicRng", "run", 13),
            stream("com.megacrit.cardcrawl.neow.NeowEvent.rng", "run", 14),
        ],
    })
}

fn action_space(snapshot: &sts_parity_lockstep::ParitySnapshotV1) -> Vec<SemanticActionV1> {
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::ActionSpace].evidence
    else {
        panic!("action space should be observed")
    };
    serde_json::from_value(value.clone()).unwrap()
}

fn fixture_rng_witness(
    from_frame_id: u64,
    to_frame_id: u64,
    action_id: &str,
    calls: Vec<Value>,
) -> Value {
    json!({
        "kind": "rng_call_witness_v1",
        "schema": NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1,
        "from_frame_id": from_frame_id,
        "to_frame_id": to_frame_id,
        "request_id": format!("fixture-request-{from_frame_id}"),
        "action_id": action_id,
        "complete": true,
        "calls": calls,
        "unsupported_reason": null,
    })
}

fn fixture_rng_call(owner: &str, owner_ordinal: u32, result: bool) -> Value {
    json!({
        "owner": owner,
        "owner_ordinal": owner_ordinal,
        "operation": {
            "method": "random_boolean",
            "result": result,
        },
    })
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sts-parity-lockstep-java-{label}-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ))
}
