use std::fs;
use std::path::{Path, PathBuf};
use sts_paired_root::{
    action_handle, transition_commitment_body, BlobRefV1, CapabilityKindV1, CapabilityProofV1,
    CapabilityStatusV1, ComparisonStatusV1, PairedActionV1, ParentStateRootV2,
    ProjectionComparisonV1, ProjectionDomainV1, RecoveryCertificateBodyV2, RootPreflightV1,
    RootProvenanceV1, RustStateSnapshotV2, StateRootActivationBodyV2, StateRootArtifactV2,
    StateRootBodyV2, StateRootStoreV2, TransitionReceiptArtifactV1, TransitionReceiptBodyV1,
    TransitionTimingV1, VersionIdentityV1, WorldProvenanceV1,
    NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1,
};

pub(crate) struct PairedStoreFixture {
    pub repository: PathBuf,
    pub store: StateRootStoreV2,
    pub source: StateRootArtifactV2,
    pub first: TransitionReceiptArtifactV1,
    pub second: TransitionReceiptArtifactV1,
}

impl Drop for PairedStoreFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.repository);
    }
}

pub(crate) fn paired_store_fixture() -> PairedStoreFixture {
    let repository =
        std::env::temp_dir().join(format!("sts-workbench-store-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&repository).unwrap();
    let store = StateRootStoreV2::new(repository.join(".oracle-lab").join("paired-root-store"));
    let blob = store.put_bytes("application/json", b"{}").unwrap();
    let source = store
        .create_root(fixture_root(&repository, &store, blob.clone()))
        .unwrap();
    let first = fixture_successor(&store, &source, blob.clone(), PairedActionV1::EndTurn);
    let second = fixture_successor(&store, &source, blob, PairedActionV1::SkipCardReward);
    add_fixture_recovery_certificate(&repository, &store, &source);
    store.create_head("probe", &source.root_id).unwrap();
    store.select_head("official", None, "probe").unwrap();
    PairedStoreFixture {
        repository,
        store,
        source,
        first,
        second,
    }
}

fn add_fixture_recovery_certificate(
    repository: &Path,
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
) {
    let frame = store.put_bytes("application/json", b"{}").unwrap();
    let captured = store
        .create_activation(StateRootActivationBodyV2 {
            root_id: root.root_id.clone(),
            java_runtime_epoch: "fixture-java-capture".to_owned(),
            java_frame: frame.clone(),
            action_space_id: root.root.preflight.action_space_id.clone(),
            card_leases: Vec::new(),
        })
        .unwrap();
    let restored = store
        .create_activation(StateRootActivationBodyV2 {
            root_id: root.root_id.clone(),
            java_runtime_epoch: "fixture-java-restore".to_owned(),
            java_frame: frame,
            action_space_id: root.root.preflight.action_space_id.clone(),
            card_leases: Vec::new(),
        })
        .unwrap();
    let native_storage = repository.join("fixture-native-storage");
    for relative in NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1 {
        fs::create_dir_all(native_storage.join(relative)).unwrap();
    }
    fs::write(native_storage.join("preferences/prefs"), b"prefs").unwrap();
    fs::write(native_storage.join("saves/IRONCLAD.autosave"), b"save").unwrap();
    let bundle = store.put_directory(&native_storage).unwrap();
    let java_recovery = store
        .native_storage_recovery_payload(&bundle, &root.root.content_profile)
        .unwrap();
    store
        .create_recovery_certificate(RecoveryCertificateBodyV2 {
            target_root_id: root.root_id.clone(),
            versions: root.root.versions.clone(),
            java_recovery,
            captured_activation_id: captured.activation_id,
            restored_activation_id: restored.activation_id,
            restored_components: root.root.preflight.components.clone(),
        })
        .unwrap();
}

fn fixture_successor(
    store: &StateRootStoreV2,
    source: &StateRootArtifactV2,
    blob: BlobRefV1,
    action: PairedActionV1,
) -> TransitionReceiptArtifactV1 {
    let action = action_handle(
        &source.root_id,
        &source.root.preflight.action_space_id,
        action,
    )
    .unwrap();
    let mut receipt = TransitionReceiptBodyV1 {
        transition_commitment_id: String::new(),
        source_root_id: source.root_id.clone(),
        successor_root_id: "pending".to_owned(),
        action,
        java_action_receipt: blob.clone(),
        rust_transition_receipt: blob,
        successor_comparisons: source.root.preflight.components.clone(),
        timings: TransitionTimingV1 {
            rust_execution_ms: 1,
            java_execution_ms: 2,
            comparison_ms: 3,
        },
    };
    let commitment = store
        .create_transition_commitment(transition_commitment_body(&receipt))
        .unwrap();
    receipt.transition_commitment_id = commitment.transition_commitment_id;
    let mut successor = source.root.clone();
    successor.parent = Some(ParentStateRootV2 {
        root_id: source.root_id.clone(),
        transition_commitment_id: receipt.transition_commitment_id.clone(),
    });
    receipt.successor_root_id = store.create_root(successor).unwrap().root_id;
    store.create_receipt(receipt).unwrap()
}

pub(crate) fn add_fixture_successor(
    store: &StateRootStoreV2,
    source: &StateRootArtifactV2,
    action: PairedActionV1,
) -> TransitionReceiptArtifactV1 {
    let blob = store.put_bytes("application/json", b"{}").unwrap();
    fixture_successor(store, source, blob, action)
}

fn fixture_root(repository: &Path, store: &StateRootStoreV2, blob: BlobRefV1) -> StateRootBodyV2 {
    let payload = repository.join("fixture-payload");
    fs::create_dir_all(&payload).unwrap();
    let bundle = store.put_directory(&payload).unwrap();
    StateRootBodyV2 {
        provenance: RootProvenanceV1::Probe,
        world_provenance: WorldProvenanceV1::Natural,
        parent: None,
        decision_profile: sts_paired_root::DecisionProfileV1::CombatPlayerDecision,
        versions: vec![VersionIdentityV1 {
            component: "fixture".to_owned(),
            identity: "v1".to_owned(),
        }],
        content_profile: "all_unlocked".to_owned(),
        rust_snapshot: RustStateSnapshotV2 {
            bundle,
            entrypoint: "exact_checkpoint".to_owned(),
            source_identity: Vec::new(),
        },
        preflight: RootPreflightV1 {
            action_space_id: "action-space:fixture".to_owned(),
            components: vec![ProjectionComparisonV1 {
                domain: ProjectionDomainV1::RunScalars,
                projection_schema: "fixture-v1".to_owned(),
                java: blob.clone(),
                rust: blob.clone(),
                status: ComparisonStatusV1::Exact,
                first_difference: None,
            }],
            uncovered: Vec::new(),
        },
        card_bindings: Vec::new(),
        capabilities: vec![CapabilityProofV1 {
            capability: CapabilityKindV1::Observe,
            required_domains: vec![ProjectionDomainV1::RunScalars],
            status: CapabilityStatusV1::Verified { proof: blob },
        }],
    }
}
