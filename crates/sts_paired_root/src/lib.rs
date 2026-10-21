//! Immutable Java/Rust paired roots and receipt-only head movement.
//!
//! This crate owns durable cross-runtime identities. It deliberately does not
//! launch Java, execute simulator actions, choose strategy, or expose a CLI.

use fs2::FileExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};
use sts_content_bundle::ContentBundleStoreV1;
pub use sts_content_bundle::{
    BlobRefV1, BundleFileV1, BundleManifestV1, BundleRefV1, BUNDLE_SCHEMA_V1,
};

pub const STATE_ROOT_SCHEMA_V2: &str = "sts-state-root-v2";
pub const STATE_ROOT_ACTIVATION_SCHEMA_V2: &str = "sts-state-root-activation-v2";
pub const RECOVERY_CERTIFICATE_SCHEMA_V2: &str = "sts-state-root-recovery-certificate-v2";
pub const NATIVE_RECOVERY_CANDIDATE_SCHEMA_V2: &str = "sts-state-root-native-recovery-candidate-v2";
pub const TRANSITION_COMMITMENT_SCHEMA_V1: &str = "sts-state-transition-commitment-v2";
pub const TRANSITION_RECEIPT_SCHEMA_V1: &str = "sts-state-transition-receipt-v2";
pub const DIVERGENCE_SCHEMA_V1: &str = "sts-state-divergence-v2";
pub const PAIRED_HEAD_SCHEMA_V1: &str = "sts-state-head-v2";
pub const PAIRED_HEAD_SELECTOR_SCHEMA_V1: &str = "sts-state-head-selector-v2";
pub const OFFICIAL_HEAD_SELECTOR_V1: &str = "official";
pub const NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1: &str = "native_storage_continue";
pub const NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1: &[&str] =
    &["betaPreferences", "preferences", "saves"];
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootProvenanceV1 {
    Official,
    Probe,
    Forge,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldProvenanceV1 {
    Natural,
    ResampledContinuation,
    ValidatedConstructed,
    Adversarial,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKindV1 {
    JavaOriginalGame,
    RustSimulator,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreQualityV1 {
    NativeRoundTripVerified,
    TypedReconstructedVerified,
    ReplayFromParentVerified,
    UnstableBoundary,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionProfileV1 {
    CombatPlayerDecision,
    CombatReward,
    CardReward,
    Event,
    MapNavigation,
    Neow,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityClaimV1 {
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VersionIdentityV1 {
    pub component: String,
    pub identity: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPayloadV1 {
    pub runtime: RuntimeKindV1,
    pub restore_quality: RestoreQualityV1,
    pub bundle: BundleRefV1,
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub source_identity: Vec<IdentityClaimV1>,
}

/// A self-contained Rust representation of one exact state.
///
/// Java recovery quality deliberately does not have a corresponding field in
/// the state root. Cross-JVM Java recovery is qualified only by a separately
/// published `RecoveryCertificateArtifactV2`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RustStateSnapshotV2 {
    pub bundle: BundleRefV1,
    pub entrypoint: String,
    pub source_identity: Vec<IdentityClaimV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionDomainV1 {
    RunScalars,
    Map,
    MasterDeck,
    Relics,
    Potions,
    RngStreams,
    CardPools,
    Room,
    Combat,
    ActionSpace,
    CardBindings,
    RuntimeSettlement,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonStatusV1 {
    Exact,
    Diverged,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TypedDifferenceV1 {
    pub path: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionComparisonV1 {
    pub domain: ProjectionDomainV1,
    pub projection_schema: String,
    pub java: BlobRefV1,
    pub rust: BlobRefV1,
    pub status: ComparisonStatusV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_difference: Option<TypedDifferenceV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageGapV1 {
    pub gap_id: String,
    pub domain: ProjectionDomainV1,
    /// Whether this gap prevents equality of the current recoverable state or
    /// only withholds permission for a future action shape.
    ///
    /// Historical roots did not declare this distinction. They remain
    /// readable with `LegacyUnspecified`, which deliberately continues to
    /// block native recovery certificates instead of guessing from `kind` text.
    #[serde(
        default,
        skip_serializing_if = "CoverageGapScopeV1::is_legacy_unspecified"
    )]
    pub scope: CoverageGapScopeV1,
    pub kind: String,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageGapScopeV1 {
    #[default]
    LegacyUnspecified,
    StateProjection,
    ActionCapability,
}

impl CoverageGapScopeV1 {
    fn is_legacy_unspecified(&self) -> bool {
        *self == Self::LegacyUnspecified
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RootPreflightV1 {
    pub action_space_id: String,
    pub components: Vec<ProjectionComparisonV1>,
    pub uncovered: Vec<CoverageGapV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum RustCardLineageV1 {
    MasterDeck {
        acquisition_ordinal: u64,
    },
    CombatGenerated {
        combat_sequence: u64,
        generation_ordinal: u64,
    },
    ConstructedScenario {
        recipe_id: String,
        slot: u64,
    },
    ConstructedScenarioInstance {
        scenario_instance_id: String,
        slot: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CardBindingEvidenceV1 {
    MasterDeckObservation {
        java_master_deck_ordinal: u64,
        rust_master_deck_ordinal: u64,
        java_projection: BlobRefV1,
        rust_projection: BlobRefV1,
    },
    CapturedBirth {
        java_witness: BlobRefV1,
        rust_receipt: BlobRefV1,
        generation_index: u64,
    },
    ConstructedRecipe {
        recipe_id: String,
        slot: u64,
        recipe: BlobRefV1,
    },
    ConstructedScenario {
        scenario_instance_id: String,
        slot: u64,
        recipe: BlobRefV1,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossRuntimeCardBindingV1 {
    pub binding_id: String,
    pub rust_lineage: RustCardLineageV1,
    pub card_id: String,
    pub upgrades: u8,
    pub evidence: CardBindingEvidenceV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKindV1 {
    Observe,
    ChooseNeowHundredGoldNoDrawback,
    ChooseNeowThreeEnemyKillNoDrawback,
    ChooseNeowClosed,
    ChooseNeowDeckSelection,
    ChooseNeowReward,
    LeaveNeow,
    EnterQuestionMark,
    ChooseMapNode,
    ResolveEvent,
    ResolveCombatReward,
    ResolveCardReward,
    EnterCombat,
    RestoreCombatEntry,
    PlayCard,
    EndTurn,
    UsePotion,
    SubmitHandSelect,
    SubmitGridSelect,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CapabilityStatusV1 {
    Verified { proof: BlobRefV1 },
    Unknown { gap_id: String },
    Unsupported { reason: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityProofV1 {
    pub capability: CapabilityKindV1,
    pub required_domains: Vec<ProjectionDomainV1>,
    pub status: CapabilityStatusV1,
}

/// A capability disposition before a comparison proof exists.
///
/// Unknown capabilities own their complete gap instead of referring to an ID
/// assembled elsewhere. This makes an absent-gap root impossible to produce
/// through the plan API and lets callers validate successor structure before
/// performing an external runtime action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityPlanStatusV1 {
    Verified,
    Unknown { gap: CoverageGapV1 },
    Unsupported { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityPlanV1 {
    pub capability: CapabilityKindV1,
    pub required_domains: Vec<ProjectionDomainV1>,
    pub status: CapabilityPlanStatusV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilitySurfacePlanV1 {
    uncovered: Vec<CoverageGapV1>,
    capabilities: Vec<CapabilityPlanV1>,
}

impl CapabilitySurfacePlanV1 {
    pub fn try_new(
        mut uncovered: Vec<CoverageGapV1>,
        mut capabilities: Vec<CapabilityPlanV1>,
    ) -> Result<Self, String> {
        capabilities.sort_by_key(|plan| plan.capability);
        if capabilities
            .windows(2)
            .any(|pair| pair[0].capability >= pair[1].capability)
        {
            return Err("capability surface plan repeats a capability".to_owned());
        }
        for plan in &capabilities {
            if plan
                .required_domains
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            {
                return Err(
                    "capability surface plan domains are repeated or not canonical".to_owned(),
                );
            }
            match &plan.status {
                CapabilityPlanStatusV1::Verified => {}
                CapabilityPlanStatusV1::Unknown { gap } => {
                    if gap.scope != CoverageGapScopeV1::ActionCapability {
                        return Err(
                            "unknown capability requires an action-capability coverage gap"
                                .to_owned(),
                        );
                    }
                    uncovered.push(gap.clone());
                }
                CapabilityPlanStatusV1::Unsupported { reason } => {
                    require_token(reason, "unsupported capability reason")?;
                }
            }
        }
        uncovered.sort_by(|left, right| left.gap_id.cmp(&right.gap_id));
        if uncovered.iter().any(|gap| gap.gap_id.is_empty())
            || uncovered
                .windows(2)
                .any(|pair| pair[0].gap_id >= pair[1].gap_id)
        {
            return Err("capability surface plan gap IDs are empty or duplicated".to_owned());
        }
        Ok(Self {
            uncovered,
            capabilities,
        })
    }

    pub fn materialize(&self, proof: &BlobRefV1) -> (Vec<CoverageGapV1>, Vec<CapabilityProofV1>) {
        let capabilities = self
            .capabilities
            .iter()
            .map(|plan| CapabilityProofV1 {
                capability: plan.capability,
                required_domains: plan.required_domains.clone(),
                status: match &plan.status {
                    CapabilityPlanStatusV1::Verified => CapabilityStatusV1::Verified {
                        proof: proof.clone(),
                    },
                    CapabilityPlanStatusV1::Unknown { gap } => CapabilityStatusV1::Unknown {
                        gap_id: gap.gap_id.clone(),
                    },
                    CapabilityPlanStatusV1::Unsupported { reason } => {
                        CapabilityStatusV1::Unsupported {
                            reason: reason.clone(),
                        }
                    }
                },
            })
            .collect();
        (self.uncovered.clone(), capabilities)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParentStateRootV2 {
    pub root_id: String,
    pub transition_commitment_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateRootBodyV2 {
    pub provenance: RootProvenanceV1,
    pub world_provenance: WorldProvenanceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentStateRootV2>,
    pub decision_profile: DecisionProfileV1,
    pub versions: Vec<VersionIdentityV1>,
    pub content_profile: String,
    pub rust_snapshot: RustStateSnapshotV2,
    pub preflight: RootPreflightV1,
    pub card_bindings: Vec<CrossRuntimeCardBindingV1>,
    pub capabilities: Vec<CapabilityProofV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateRootArtifactV2 {
    pub schema: String,
    pub root_id: String,
    pub root: StateRootBodyV2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaCardLeaseV1 {
    pub binding_id: String,
    pub java_card_uuid: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateRootActivationBodyV2 {
    pub root_id: String,
    pub java_runtime_epoch: String,
    pub java_frame: BlobRefV1,
    pub action_space_id: String,
    pub card_leases: Vec<JavaCardLeaseV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateRootActivationArtifactV2 {
    pub schema: String,
    pub activation_id: String,
    pub activation: StateRootActivationBodyV2,
}

/// A separately verified native recovery route for an existing immutable
/// logical state root. Adding one must not rewrite the root or fabricate a
/// gameplay transition. The two activations bind the capture and a
/// fresh-runtime round trip back to the same root.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryCertificateBodyV2 {
    pub target_root_id: String,
    pub versions: Vec<VersionIdentityV1>,
    pub java_recovery: RecoveryPayloadV1,
    pub captured_activation_id: String,
    pub restored_activation_id: String,
    pub restored_components: Vec<ProjectionComparisonV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryCertificateArtifactV2 {
    pub schema: String,
    pub recovery_certificate_id: String,
    pub certificate: RecoveryCertificateBodyV2,
}

/// Immutable facts an Engine may carry across one native round-trip job.
///
/// Preparing this candidate is read-only and rejects an ineligible target
/// before any external runtime or storage capture is started. Publishing an
/// certificate still revalidates every reference from disk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryCertificateCandidateV2 {
    pub target_root_id: String,
    pub captured_activation_id: String,
    pub versions: Vec<VersionIdentityV1>,
    pub content_profile: String,
    pub expected_components: Vec<ProjectionComparisonV1>,
}

/// A durable native-storage capture that has not completed a fresh-runtime
/// round trip.
///
/// This artifact is intentionally not a recovery payload or certificate. Recovery
/// planning never enumerates it. A later validator may consume its exact
/// capsule and, only after an Exact round trip, publish a separate
/// `RecoveryCertificateArtifactV2`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRecoveryCandidateBodyV2 {
    pub target_root_id: String,
    pub captured_activation_id: String,
    pub versions: Vec<VersionIdentityV1>,
    pub content_profile: String,
    pub recovery_ref: String,
    pub native_storage_bundle: BundleRefV1,
    pub native_autosave_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRecoveryCandidateArtifactV2 {
    pub schema: String,
    pub native_recovery_candidate_id: String,
    pub candidate: NativeRecoveryCandidateBodyV2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PairedActionV1 {
    ChooseNeowOption {
        reward: NeowRewardIdentityV1,
        drawback: NeowDrawbackIdentityV1,
    },
    LeaveNeow,
    PlayCard {
        binding_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<MonsterTargetV1>,
    },
    EndTurn,
    UsePotion {
        slot: u8,
        potion_id: String,
        potency: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<MonsterTargetV1>,
    },
    ClaimCombatReward {
        reward: CombatRewardIdentityV1,
        same_reward_ordinal: usize,
    },
    ChooseCardReward {
        card_id: String,
        upgrades: u8,
        same_card_ordinal: usize,
    },
    SkipCardReward,
    CloseCombatReward,
    ChooseEventOption {
        event_id: String,
        screen: usize,
        option_index: usize,
    },
    ChooseMapNode {
        x: i32,
        y: i32,
        room_type: MapRoomIdentityV1,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeowRewardIdentityV1 {
    ThreeEnemyKill,
    TenPercentHpBonus,
    ThreeCards,
    OneRandomRareCard,
    RemoveCard,
    UpgradeCard,
    TransformCard,
    RandomColorless,
    ThreeSmallPotions,
    RandomCommonRelic,
    HundredGold,
    RandomColorless2,
    RemoveTwo,
    OneRareRelic,
    ThreeRareCards,
    TwoFiftyGold,
    TransformTwoCards,
    TwentyPercentHpBonus,
    BossRelic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeowDrawbackIdentityV1 {
    None,
    TenPercentHpLoss,
    NoGold,
    Curse,
    PercentDamage,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MapRoomIdentityV1 {
    QuestionMark,
    Monster,
    Elite,
    Boss,
    Rest,
    Shop,
    Treasure,
    Victory,
    Neow,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatRewardIdentityV1 {
    Gold { amount: i32 },
    StolenGold { amount: i32 },
    Card { choice_count: usize },
    Potion { potion_id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterTargetV1 {
    pub enemy_id: String,
    pub same_enemy_ordinal: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedActionHandleV1 {
    pub handle_id: String,
    pub source_root_id: String,
    pub action_space_id: String,
    pub action: PairedActionV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionTimingV1 {
    pub rust_execution_ms: u128,
    pub java_execution_ms: u128,
    pub comparison_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionCommitmentBodyV1 {
    pub source_root_id: String,
    pub action: PairedActionHandleV1,
    pub rust_transition_receipt: BlobRefV1,
    pub successor_comparisons: Vec<ProjectionComparisonV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionCommitmentArtifactV1 {
    pub schema: String,
    pub transition_commitment_id: String,
    pub transition: TransitionCommitmentBodyV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionReceiptBodyV1 {
    pub transition_commitment_id: String,
    pub source_root_id: String,
    pub successor_root_id: String,
    pub action: PairedActionHandleV1,
    pub java_action_receipt: BlobRefV1,
    pub rust_transition_receipt: BlobRefV1,
    pub successor_comparisons: Vec<ProjectionComparisonV1>,
    pub timings: TransitionTimingV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionReceiptArtifactV1 {
    pub schema: String,
    pub receipt_id: String,
    pub receipt: TransitionReceiptBodyV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateRootRecoveryStepV2 {
    pub source_root_id: String,
    pub target_root_id: String,
    pub transition_commitment_id: String,
    pub action: PairedActionHandleV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateRootRecoveryPlanV2 {
    pub native_root_id: String,
    pub recovery_certificate_id: String,
    pub target_root_id: String,
    pub steps: Vec<StateRootRecoveryStepV2>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DivergenceStageV1 {
    Restore,
    RootPreflight,
    ActionResolution,
    RustExecution,
    JavaExecution,
    SuccessorComparison,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DivergenceBodyV1 {
    pub source_root_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<PairedActionHandleV1>,
    pub stage: DivergenceStageV1,
    pub kind: String,
    pub difference: TypedDifferenceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_evidence: Option<BlobRefV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust_evidence: Option<BlobRefV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DivergenceArtifactV1 {
    pub schema: String,
    pub divergence_id: String,
    pub divergence: DivergenceBodyV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedHeadV1 {
    pub schema: String,
    pub head_name: String,
    pub revision: u64,
    pub root_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_receipt_id: Option<String>,
    pub created_at_unix_ms: u128,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairedHeadCommitStatusV1 {
    Committed,
    AlreadyCommitted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedHeadCommitOutcomeV1 {
    pub status: PairedHeadCommitStatusV1,
    pub receipt_id: String,
    pub committed_head: PairedHeadV1,
    pub current_head: PairedHeadV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedHeadSelectorV1 {
    pub schema: String,
    pub selector_name: String,
    pub revision: u64,
    pub head_name: String,
    pub selected_head_revision: u64,
    pub selected_root_id: String,
    pub created_at_unix_ms: u128,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairedHeadSelectorStatusV1 {
    Selected,
    AlreadySelected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedHeadSelectorOutcomeV1 {
    pub status: PairedHeadSelectorStatusV1,
    pub selector: PairedHeadSelectorV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedHeadSelectionResolutionV1 {
    pub selector: PairedHeadSelectorV1,
    pub head: PairedHeadV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedHeadCommitOutcomeV1 {
    pub selector: PairedHeadSelectorV1,
    pub commit: PairedHeadCommitOutcomeV1,
}

pub struct StateRootStoreV2 {
    root: PathBuf,
    content_store: ContentBundleStoreV1,
    verification_cache: Option<Mutex<VerificationCacheV1>>,
}

#[derive(Default)]
struct VerificationCacheV1 {
    roots: BTreeMap<String, StateRootArtifactV2>,
    roots_in_progress: BTreeSet<String>,
    activations: BTreeMap<String, StateRootActivationArtifactV2>,
    recovery_certificates: BTreeMap<String, RecoveryCertificateArtifactV2>,
    transition_commitments: BTreeMap<String, TransitionCommitmentArtifactV1>,
    receipts: BTreeMap<String, TransitionReceiptArtifactV1>,
}

impl StateRootStoreV2 {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            content_store: ContentBundleStoreV1::new(&root),
            root,
            verification_cache: None,
        }
    }

    /// Reuses successful immutable-artifact verification only within this
    /// store value. Create a fresh store for every independent command so the
    /// next operation re-audits persisted bytes from an empty cache.
    pub fn with_operation_verification_cache(mut self) -> Self {
        self.content_store =
            ContentBundleStoreV1::new(&self.root).with_operation_verification_cache();
        self.verification_cache = Some(Mutex::new(VerificationCacheV1::default()));
        self
    }

    fn lock_operation_verification_cache(
        &self,
    ) -> Result<Option<MutexGuard<'_, VerificationCacheV1>>, String> {
        self.verification_cache
            .as_ref()
            .map(|cache| {
                cache
                    .lock()
                    .map_err(|_| "paired-root operation verification cache is poisoned".to_owned())
            })
            .transpose()
    }

    #[cfg(test)]
    fn operation_verification_cache_counts(
        &self,
    ) -> Option<(usize, usize, usize, usize, usize, usize, usize)> {
        self.verification_cache.as_ref().map(|cache| {
            let cache = cache.lock().unwrap();
            let (blob_count, bundle_count) = self
                .content_store
                .operation_verification_cache_counts()
                .expect("paired operation cache enables content cache");
            (
                blob_count,
                bundle_count,
                cache.roots.len(),
                cache.activations.len(),
                cache.recovery_certificates.len(),
                cache.transition_commitments.len(),
                cache.receipts.len(),
            )
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn initialize(&self) -> Result<(), String> {
        self.content_store.initialize()?;
        for path in [
            self.roots_dir(),
            self.activations_dir(),
            self.native_recovery_candidates_dir(),
            self.recovery_certificates_dir(),
            self.transition_commitments_dir(),
            self.receipts_dir(),
            self.divergences_dir(),
            self.heads_dir(),
            self.head_selectors_dir(),
        ] {
            fs::create_dir_all(&path)
                .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
        }
        Ok(())
    }

    pub fn put_bytes(&self, media_type: &str, bytes: &[u8]) -> Result<BlobRefV1, String> {
        self.content_store.put_bytes(media_type, bytes)
    }

    pub fn put_file(&self, media_type: &str, path: &Path) -> Result<BlobRefV1, String> {
        self.content_store.put_file(media_type, path)
    }

    pub fn put_directory(&self, directory: &Path) -> Result<BundleRefV1, String> {
        self.content_store.put_directory(directory)
    }

    pub fn create_root(&self, root: StateRootBodyV2) -> Result<StateRootArtifactV2, String> {
        validate_root(&root)?;
        self.validate_root_references(&root)?;
        let root_id = content_id("state-root-v2", &canonical_bytes(&root)?);
        let artifact = StateRootArtifactV2 {
            schema: STATE_ROOT_SCHEMA_V2.to_owned(),
            root_id: root_id.clone(),
            root,
        };
        publish_json_immutable(&self.root_path(&root_id), &artifact)?;
        self.load_root(&root_id)
    }

    pub fn load_root(&self, root_id: &str) -> Result<StateRootArtifactV2, String> {
        require_content_id(root_id, "state-root-v2", "paired root ID")?;
        if let Some(mut cache) = self.lock_operation_verification_cache()? {
            if let Some(artifact) = cache.roots.get(root_id).cloned() {
                return Ok(artifact);
            }
            if !cache.roots_in_progress.insert(root_id.to_owned()) {
                return Err("paired root reference graph contains a cycle".to_owned());
            }
        }

        let loaded = (|| {
            let artifact: StateRootArtifactV2 = read_json(&self.root_path(root_id))?;
            if artifact.schema != STATE_ROOT_SCHEMA_V2 || artifact.root_id != root_id {
                return Err("paired root envelope identity is invalid".to_owned());
            }
            validate_root(&artifact.root)?;
            self.validate_root_references(&artifact.root)?;
            let expected = content_id("state-root-v2", &canonical_bytes(&artifact.root)?);
            if expected != artifact.root_id {
                return Err("paired root content hash is invalid".to_owned());
            }
            Ok(artifact)
        })();

        if let Some(mut cache) = self.lock_operation_verification_cache()? {
            cache.roots_in_progress.remove(root_id);
            if let Ok(artifact) = &loaded {
                cache.roots.insert(root_id.to_owned(), artifact.clone());
            }
        }
        loaded
    }

    /// Reads one Java/Rust Exact projection through the store that owns its
    /// immutable evidence. Callers receive the shared value once and cannot
    /// accidentally choose one runtime's blob or weaken the comparison check.
    pub fn load_exact_projection<T: DeserializeOwned>(
        &self,
        root_id: &str,
        domain: ProjectionDomainV1,
    ) -> Result<T, String> {
        let root = self.load_root(root_id)?;
        let mut matches = root
            .root
            .preflight
            .components
            .iter()
            .filter(|component| component.domain == domain);
        let component = matches
            .next()
            .ok_or_else(|| format!("paired root omitted required {domain:?} projection"))?;
        if matches.next().is_some() {
            return Err(format!("paired root repeats {domain:?} projection"));
        }
        if component.status != ComparisonStatusV1::Exact
            || component.java.sha256 != component.rust.sha256
            || component.java.bytes != component.rust.bytes
            || component.java.media_type != component.rust.media_type
        {
            return Err(format!(
                "paired root {domain:?} is not one Exact shared projection"
            ));
        }
        let bytes = self.read_blob(&component.rust)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to decode {domain:?} projection: {error}"))
    }

    pub fn create_activation(
        &self,
        activation: StateRootActivationBodyV2,
    ) -> Result<StateRootActivationArtifactV2, String> {
        validate_activation(&activation)?;
        self.validate_activation_references(&activation)?;
        let activation_id = content_id("state-root-activation-v2", &canonical_bytes(&activation)?);
        let artifact = StateRootActivationArtifactV2 {
            schema: STATE_ROOT_ACTIVATION_SCHEMA_V2.to_owned(),
            activation_id: activation_id.clone(),
            activation,
        };
        publish_json_immutable(&self.activation_path(&activation_id), &artifact)?;
        self.load_activation(&activation_id)
    }

    pub fn load_activation(
        &self,
        activation_id: &str,
    ) -> Result<StateRootActivationArtifactV2, String> {
        require_content_id(
            activation_id,
            "state-root-activation-v2",
            "paired root activation ID",
        )?;
        if let Some(cache) = self.lock_operation_verification_cache()? {
            if let Some(activation) = cache.activations.get(activation_id).cloned() {
                return Ok(activation);
            }
        }
        let artifact: StateRootActivationArtifactV2 =
            read_json(&self.activation_path(activation_id))?;
        if artifact.schema != STATE_ROOT_ACTIVATION_SCHEMA_V2
            || artifact.activation_id != activation_id
        {
            return Err("paired root activation envelope identity is invalid".to_owned());
        }
        validate_activation(&artifact.activation)?;
        self.validate_activation_references(&artifact.activation)?;
        let expected = content_id(
            "state-root-activation-v2",
            &canonical_bytes(&artifact.activation)?,
        );
        if expected != artifact.activation_id {
            return Err("paired root activation content hash is invalid".to_owned());
        }
        if let Some(mut cache) = self.lock_operation_verification_cache()? {
            cache
                .activations
                .insert(activation_id.to_owned(), artifact.clone());
        }
        Ok(artifact)
    }

    pub fn create_recovery_certificate(
        &self,
        certificate: RecoveryCertificateBodyV2,
    ) -> Result<RecoveryCertificateArtifactV2, String> {
        validate_recovery_certificate(&certificate)?;
        self.validate_recovery_certificate_references(&certificate)?;
        let recovery_certificate_id = content_id(
            "state-root-recovery-certificate-v2",
            &canonical_bytes(&certificate)?,
        );
        let artifact = RecoveryCertificateArtifactV2 {
            schema: RECOVERY_CERTIFICATE_SCHEMA_V2.to_owned(),
            recovery_certificate_id: recovery_certificate_id.clone(),
            certificate,
        };
        publish_json_immutable(
            &self.recovery_certificate_path(
                &artifact.certificate.target_root_id,
                &artifact.recovery_certificate_id,
            ),
            &artifact,
        )?;
        self.load_recovery_certificate(
            &artifact.certificate.target_root_id,
            &artifact.recovery_certificate_id,
        )
    }

    pub fn create_native_recovery_candidate(
        &self,
        candidate: NativeRecoveryCandidateBodyV2,
    ) -> Result<NativeRecoveryCandidateArtifactV2, String> {
        validate_native_recovery_candidate(&candidate)?;
        self.validate_native_recovery_candidate_references(&candidate)?;
        let native_recovery_candidate_id = content_id(
            "state-root-native-recovery-candidate-v2",
            &canonical_bytes(&candidate)?,
        );
        let artifact = NativeRecoveryCandidateArtifactV2 {
            schema: NATIVE_RECOVERY_CANDIDATE_SCHEMA_V2.to_owned(),
            native_recovery_candidate_id: native_recovery_candidate_id.clone(),
            candidate,
        };
        publish_json_immutable(
            &self.native_recovery_candidate_path(
                &artifact.candidate.target_root_id,
                &artifact.native_recovery_candidate_id,
            ),
            &artifact,
        )?;
        self.load_native_recovery_candidate(
            &artifact.candidate.target_root_id,
            &artifact.native_recovery_candidate_id,
        )
    }

    pub fn load_native_recovery_candidate(
        &self,
        target_root_id: &str,
        native_recovery_candidate_id: &str,
    ) -> Result<NativeRecoveryCandidateArtifactV2, String> {
        require_content_id(
            target_root_id,
            "state-root-v2",
            "native recovery candidate root ID",
        )?;
        require_content_id(
            native_recovery_candidate_id,
            "state-root-native-recovery-candidate-v2",
            "native recovery candidate ID",
        )?;
        let artifact: NativeRecoveryCandidateArtifactV2 = read_json(
            &self.native_recovery_candidate_path(target_root_id, native_recovery_candidate_id),
        )?;
        if artifact.schema != NATIVE_RECOVERY_CANDIDATE_SCHEMA_V2
            || artifact.native_recovery_candidate_id != native_recovery_candidate_id
            || artifact.candidate.target_root_id != target_root_id
        {
            return Err("native recovery candidate envelope identity is invalid".to_owned());
        }
        validate_native_recovery_candidate(&artifact.candidate)?;
        self.validate_native_recovery_candidate_references(&artifact.candidate)?;
        let expected = content_id(
            "state-root-native-recovery-candidate-v2",
            &canonical_bytes(&artifact.candidate)?,
        );
        if expected != artifact.native_recovery_candidate_id {
            return Err("native recovery candidate content hash is invalid".to_owned());
        }
        Ok(artifact)
    }

    /// Enumerates only retained, unverified native candidates for one exact
    /// logical root. Recovery planning deliberately does not call this API.
    pub fn native_recovery_candidate_ids(
        &self,
        target_root_id: &str,
    ) -> Result<Vec<String>, String> {
        require_content_id(
            target_root_id,
            "state-root-v2",
            "native recovery candidate root ID",
        )?;
        let directory = self.native_recovery_candidate_root_dir(target_root_id);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(format!(
                    "failed to read native recovery candidates for {target_root_id}: {error}"
                ))
            }
        };
        let mut candidate_ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("failed to read native recovery candidate directory entry: {error}")
            })?;
            let file_type = entry.file_type().map_err(|error| {
                format!(
                    "failed to inspect native recovery candidate {}: {error}",
                    entry.path().display()
                )
            })?;
            if !file_type.is_file() {
                return Err(
                    "native recovery candidate directory contains a non-file entry".to_owned(),
                );
            }
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|_| "native recovery candidate filename is not UTF-8".to_owned())?;
            let candidate_id = file_name.strip_suffix(".json").ok_or_else(|| {
                "native recovery candidate filename has no JSON suffix".to_owned()
            })?;
            self.load_native_recovery_candidate(target_root_id, candidate_id)?;
            candidate_ids.push(candidate_id.to_owned());
        }
        candidate_ids.sort();
        Ok(candidate_ids)
    }

    pub fn prepare_recovery_certificate_candidate(
        &self,
        target_root_id: &str,
        captured_activation_id: &str,
    ) -> Result<RecoveryCertificateCandidateV2, String> {
        let target = self.load_root(target_root_id)?;
        validate_native_recovery_target(&target)?;
        let captured = self.load_activation(captured_activation_id)?;
        if captured.activation.root_id != target.root_id {
            return Err(
                "recovery certificate capture activation belongs to another root".to_owned(),
            );
        }
        Ok(RecoveryCertificateCandidateV2 {
            target_root_id: target.root_id,
            captured_activation_id: captured.activation_id,
            versions: target.root.versions,
            content_profile: target.root.content_profile,
            expected_components: target.root.preflight.components,
        })
    }

    pub fn load_recovery_certificate(
        &self,
        target_root_id: &str,
        recovery_certificate_id: &str,
    ) -> Result<RecoveryCertificateArtifactV2, String> {
        require_content_id(
            target_root_id,
            "state-root-v2",
            "recovery certificate root ID",
        )?;
        require_content_id(
            recovery_certificate_id,
            "state-root-recovery-certificate-v2",
            "recovery certificate ID",
        )?;
        if let Some(cache) = self.lock_operation_verification_cache()? {
            if let Some(certificate) = cache
                .recovery_certificates
                .get(recovery_certificate_id)
                .filter(|certificate| certificate.certificate.target_root_id == target_root_id)
                .cloned()
            {
                return Ok(certificate);
            }
        }
        let artifact: RecoveryCertificateArtifactV2 =
            read_json(&self.recovery_certificate_path(target_root_id, recovery_certificate_id))?;
        if artifact.schema != RECOVERY_CERTIFICATE_SCHEMA_V2
            || artifact.recovery_certificate_id != recovery_certificate_id
            || artifact.certificate.target_root_id != target_root_id
        {
            return Err("paired recovery certificate envelope identity is invalid".to_owned());
        }
        validate_recovery_certificate(&artifact.certificate)?;
        self.validate_recovery_certificate_references(&artifact.certificate)?;
        let expected = content_id(
            "state-root-recovery-certificate-v2",
            &canonical_bytes(&artifact.certificate)?,
        );
        if expected != artifact.recovery_certificate_id {
            return Err("paired recovery certificate content hash is invalid".to_owned());
        }
        if let Some(mut cache) = self.lock_operation_verification_cache()? {
            cache
                .recovery_certificates
                .insert(recovery_certificate_id.to_owned(), artifact.clone());
        }
        Ok(artifact)
    }

    pub fn create_transition_commitment(
        &self,
        transition: TransitionCommitmentBodyV1,
    ) -> Result<TransitionCommitmentArtifactV1, String> {
        validate_transition_commitment(&transition)?;
        self.validate_transition_commitment_references(&transition)?;
        let transition_commitment_id = transition_commitment_id_from_body(&transition)?;
        let artifact = TransitionCommitmentArtifactV1 {
            schema: TRANSITION_COMMITMENT_SCHEMA_V1.to_owned(),
            transition_commitment_id: transition_commitment_id.clone(),
            transition,
        };
        publish_json_immutable(
            &self.transition_commitment_path(&transition_commitment_id),
            &artifact,
        )?;
        self.load_transition_commitment(&transition_commitment_id)
    }

    pub fn load_transition_commitment(
        &self,
        transition_commitment_id: &str,
    ) -> Result<TransitionCommitmentArtifactV1, String> {
        require_content_id(
            transition_commitment_id,
            "state-transition-commitment-v2",
            "paired transition commitment ID",
        )?;
        if let Some(cache) = self.lock_operation_verification_cache()? {
            if let Some(commitment) = cache
                .transition_commitments
                .get(transition_commitment_id)
                .cloned()
            {
                return Ok(commitment);
            }
        }
        let artifact: TransitionCommitmentArtifactV1 =
            read_json(&self.transition_commitment_path(transition_commitment_id))?;
        if artifact.schema != TRANSITION_COMMITMENT_SCHEMA_V1
            || artifact.transition_commitment_id != transition_commitment_id
        {
            return Err("paired transition commitment envelope identity is invalid".to_owned());
        }
        validate_transition_commitment(&artifact.transition)?;
        self.validate_transition_commitment_references(&artifact.transition)?;
        if transition_commitment_id_from_body(&artifact.transition)?
            != artifact.transition_commitment_id
        {
            return Err("paired transition commitment content hash is invalid".to_owned());
        }
        if let Some(mut cache) = self.lock_operation_verification_cache()? {
            cache
                .transition_commitments
                .insert(transition_commitment_id.to_owned(), artifact.clone());
        }
        Ok(artifact)
    }

    pub fn recovery_plan(&self, target_root_id: &str) -> Result<StateRootRecoveryPlanV2, String> {
        let target = self.load_root(target_root_id)?;
        let mut current = target;
        let mut seen = BTreeSet::new();
        let mut reverse_steps = Vec::new();
        let recovery_certificate_id = loop {
            if !seen.insert(current.root_id.clone()) {
                return Err("paired root recovery ancestry contains a cycle".to_owned());
            }
            if let Some(certificate_id) =
                self.preferred_recovery_certificate_id(&current.root_id)?
            {
                break certificate_id;
            }
            let Some(parent) = &current.root.parent else {
                return Err(
                    "state root recovery chain has no fresh-JVM recovery certificate".to_owned(),
                );
            };
            if reverse_steps.len() >= 4096 {
                return Err("paired root recovery ancestry exceeds 4096 transitions".to_owned());
            }
            let commitment = self.load_transition_commitment(&parent.transition_commitment_id)?;
            if commitment.transition.source_root_id != parent.root_id
                || commitment.transition.action.source_root_id != parent.root_id
                || commitment.transition.successor_comparisons != current.root.preflight.components
            {
                return Err(
                    "paired root parent commitment does not describe this logical successor"
                        .to_owned(),
                );
            }
            reverse_steps.push(StateRootRecoveryStepV2 {
                source_root_id: parent.root_id.clone(),
                target_root_id: current.root_id.clone(),
                transition_commitment_id: commitment.transition_commitment_id,
                action: commitment.transition.action,
            });
            current = self.load_root(&parent.root_id)?;
        };
        reverse_steps.reverse();
        Ok(StateRootRecoveryPlanV2 {
            native_root_id: current.root_id,
            recovery_certificate_id,
            target_root_id: target_root_id.to_owned(),
            steps: reverse_steps,
        })
    }

    /// Returns the complete logical gameplay ancestry, independent of which
    /// recovery certificate can shorten physical reconstruction. Callers rendering
    /// committed history must not use `recovery_plan`, whose suffix is
    /// intentionally allowed to start at a nearer recovery certificate.
    pub fn logical_ancestry(&self, target_root_id: &str) -> Result<Vec<String>, String> {
        let mut current = self.load_root(target_root_id)?;
        let mut seen = BTreeSet::new();
        let mut reverse_roots = Vec::new();
        loop {
            if !seen.insert(current.root_id.clone()) {
                return Err("paired root logical ancestry contains a cycle".to_owned());
            }
            reverse_roots.push(current.root_id.clone());
            let Some(parent) = &current.root.parent else {
                break;
            };
            if reverse_roots.len() > 4096 {
                return Err("paired root logical ancestry exceeds 4096 transitions".to_owned());
            }
            let commitment = self.load_transition_commitment(&parent.transition_commitment_id)?;
            if commitment.transition.source_root_id != parent.root_id
                || commitment.transition.action.source_root_id != parent.root_id
                || commitment.transition.successor_comparisons != current.root.preflight.components
            {
                return Err(
                    "paired root parent commitment does not describe this logical successor"
                        .to_owned(),
                );
            }
            current = self.load_root(&parent.root_id)?;
        }
        reverse_roots.reverse();
        Ok(reverse_roots)
    }

    fn preferred_recovery_certificate_id(
        &self,
        target_root_id: &str,
    ) -> Result<Option<String>, String> {
        let directory = self.recovery_certificate_root_dir(target_root_id);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "failed to read recovery certificates for {target_root_id}: {error}"
                ))
            }
        };
        let mut certificates = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("failed to read recovery certificate directory entry: {error}")
            })?;
            let file_type = entry.file_type().map_err(|error| {
                format!(
                    "failed to inspect recovery certificate {}: {error}",
                    entry.path().display()
                )
            })?;
            if !file_type.is_file() {
                return Err(
                    "paired recovery certificate directory contains a non-file entry".to_owned(),
                );
            }
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|_| "paired recovery certificate filename is not UTF-8".to_owned())?;
            let recovery_certificate_id = file_name.strip_suffix(".json").ok_or_else(|| {
                "paired recovery certificate filename has no JSON suffix".to_owned()
            })?;
            self.load_recovery_certificate(target_root_id, recovery_certificate_id)?;
            certificates.push(recovery_certificate_id.to_owned());
        }
        certificates.sort();
        Ok(certificates.into_iter().next())
    }

    pub fn create_receipt(
        &self,
        receipt: TransitionReceiptBodyV1,
    ) -> Result<TransitionReceiptArtifactV1, String> {
        validate_receipt(&receipt)?;
        self.validate_receipt_references(&receipt)?;
        self.validate_receipt_roots(&receipt)?;
        let commitment = self.load_transition_commitment(&receipt.transition_commitment_id)?;
        if commitment.transition != transition_commitment_body(&receipt) {
            return Err(
                "paired receipt disagrees with its stable transition commitment".to_owned(),
            );
        }
        let receipt_id = content_id("state-transition-receipt-v2", &canonical_bytes(&receipt)?);
        let artifact = TransitionReceiptArtifactV1 {
            schema: TRANSITION_RECEIPT_SCHEMA_V1.to_owned(),
            receipt_id: receipt_id.clone(),
            receipt,
        };
        publish_json_immutable(&self.receipt_path(&receipt_id), &artifact)?;
        self.load_receipt(&receipt_id)
    }

    pub fn load_receipt(&self, receipt_id: &str) -> Result<TransitionReceiptArtifactV1, String> {
        require_content_id(
            receipt_id,
            "state-transition-receipt-v2",
            "paired receipt ID",
        )?;
        if let Some(cache) = self.lock_operation_verification_cache()? {
            if let Some(receipt) = cache.receipts.get(receipt_id).cloned() {
                return Ok(receipt);
            }
        }
        let artifact: TransitionReceiptArtifactV1 = read_json(&self.receipt_path(receipt_id))?;
        if artifact.schema != TRANSITION_RECEIPT_SCHEMA_V1 || artifact.receipt_id != receipt_id {
            return Err("paired transition receipt envelope identity is invalid".to_owned());
        }
        validate_receipt(&artifact.receipt)?;
        self.validate_receipt_references(&artifact.receipt)?;
        self.validate_receipt_roots(&artifact.receipt)?;
        let commitment =
            self.load_transition_commitment(&artifact.receipt.transition_commitment_id)?;
        if commitment.transition != transition_commitment_body(&artifact.receipt) {
            return Err(
                "paired receipt disagrees with its stable transition commitment".to_owned(),
            );
        }
        let expected = content_id(
            "state-transition-receipt-v2",
            &canonical_bytes(&artifact.receipt)?,
        );
        if expected != artifact.receipt_id {
            return Err("paired transition receipt content hash is invalid".to_owned());
        }
        if let Some(mut cache) = self.lock_operation_verification_cache()? {
            cache
                .receipts
                .insert(receipt_id.to_owned(), artifact.clone());
        }
        Ok(artifact)
    }

    pub fn create_divergence(
        &self,
        divergence: DivergenceBodyV1,
    ) -> Result<DivergenceArtifactV1, String> {
        require_id(&divergence.source_root_id, "divergence source_root_id")?;
        require_token(&divergence.kind, "divergence kind")?;
        self.load_root(&divergence.source_root_id)?;
        for reference in [
            divergence.java_evidence.as_ref(),
            divergence.rust_evidence.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            self.verify_blob(reference)?;
        }
        let divergence_id = content_id("state-divergence-v2", &canonical_bytes(&divergence)?);
        let artifact = DivergenceArtifactV1 {
            schema: DIVERGENCE_SCHEMA_V1.to_owned(),
            divergence_id: divergence_id.clone(),
            divergence,
        };
        publish_json_immutable(&self.divergence_path(&divergence_id), &artifact)?;
        Ok(artifact)
    }

    pub fn create_head(&self, head_name: &str, root_id: &str) -> Result<PairedHeadV1, String> {
        validate_head_name(head_name)?;
        self.load_root(root_id)?;
        let directory = self.head_dir(head_name);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        if latest_revision(&directory)?.is_some() {
            return Err(format!("paired head {head_name:?} already exists"));
        }
        let head = PairedHeadV1 {
            schema: PAIRED_HEAD_SCHEMA_V1.to_owned(),
            head_name: head_name.to_owned(),
            revision: 0,
            root_id: root_id.to_owned(),
            committed_receipt_id: None,
            created_at_unix_ms: unix_ms()?,
        };
        publish_json_immutable(&head_revision_path(&directory, 0), &head)?;
        Ok(head)
    }

    pub fn load_head(&self, head_name: &str) -> Result<PairedHeadV1, String> {
        self.load_head_chain(head_name)?
            .pop()
            .ok_or_else(|| format!("paired head {head_name:?} does not exist"))
    }

    pub fn commit_receipt(
        &self,
        head_name: &str,
        expected_revision: u64,
        receipt_id: &str,
    ) -> Result<PairedHeadV1, String> {
        self.initialize()?;
        let _official_lease = self.acquire_selector_lease(OFFICIAL_HEAD_SELECTOR_V1)?;
        self.commit_receipt_unlocked(head_name, expected_revision, receipt_id)
    }

    fn commit_receipt_unlocked(
        &self,
        head_name: &str,
        expected_revision: u64,
        receipt_id: &str,
    ) -> Result<PairedHeadV1, String> {
        let current = self.load_head(head_name)?;
        if current.revision != expected_revision {
            return Err(format!(
                "paired head advanced concurrently from revision {expected_revision} to {}",
                current.revision
            ));
        }
        let receipt = self.load_receipt(receipt_id)?;
        if receipt.receipt.source_root_id != current.root_id {
            return Err("paired receipt is not based on the current head root".to_owned());
        }
        if self.official_selects_head_unlocked(head_name)? {
            self.require_durable_root_for_selector(
                OFFICIAL_HEAD_SELECTOR_V1,
                &receipt.receipt.successor_root_id,
            )?;
        }
        self.load_root(&receipt.receipt.successor_root_id)?;
        let revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| "paired head revision overflowed".to_owned())?;
        let head = PairedHeadV1 {
            schema: PAIRED_HEAD_SCHEMA_V1.to_owned(),
            head_name: head_name.to_owned(),
            revision,
            root_id: receipt.receipt.successor_root_id.clone(),
            committed_receipt_id: Some(receipt_id.to_owned()),
            created_at_unix_ms: unix_ms()?,
        };
        publish_json_immutable(
            &head_revision_path(&self.head_dir(head_name), revision),
            &head,
        )?;
        Ok(head)
    }

    pub fn commit_receipt_idempotent(
        &self,
        head_name: &str,
        receipt_id: &str,
    ) -> Result<PairedHeadCommitOutcomeV1, String> {
        self.initialize()?;
        let _official_lease = self.acquire_selector_lease(OFFICIAL_HEAD_SELECTOR_V1)?;
        self.commit_receipt_idempotent_unlocked(head_name, receipt_id)
    }

    fn commit_receipt_idempotent_unlocked(
        &self,
        head_name: &str,
        receipt_id: &str,
    ) -> Result<PairedHeadCommitOutcomeV1, String> {
        let receipt = self.load_receipt(receipt_id)?;
        let history = self.load_head_chain(head_name)?;
        let current = history
            .last()
            .cloned()
            .ok_or_else(|| format!("paired head {head_name:?} does not exist"))?;
        if let Some(committed_head) = history
            .iter()
            .find(|head| head.committed_receipt_id.as_deref() == Some(receipt_id))
            .cloned()
        {
            return Ok(PairedHeadCommitOutcomeV1 {
                status: PairedHeadCommitStatusV1::AlreadyCommitted,
                receipt_id: receipt_id.to_owned(),
                committed_head,
                current_head: current,
            });
        }
        if receipt.receipt.source_root_id != current.root_id {
            return Err("paired receipt is not based on the current head root".to_owned());
        }
        match self.commit_receipt_unlocked(head_name, current.revision, receipt_id) {
            Ok(committed_head) => Ok(PairedHeadCommitOutcomeV1 {
                status: PairedHeadCommitStatusV1::Committed,
                receipt_id: receipt_id.to_owned(),
                committed_head: committed_head.clone(),
                current_head: committed_head,
            }),
            Err(commit_error) => {
                let refreshed = self.load_head_chain(head_name)?;
                let refreshed_current = refreshed
                    .last()
                    .cloned()
                    .ok_or_else(|| format!("paired head {head_name:?} does not exist"))?;
                if let Some(committed_head) = refreshed
                    .iter()
                    .find(|head| head.committed_receipt_id.as_deref() == Some(receipt_id))
                    .cloned()
                {
                    return Ok(PairedHeadCommitOutcomeV1 {
                        status: PairedHeadCommitStatusV1::AlreadyCommitted,
                        receipt_id: receipt_id.to_owned(),
                        committed_head,
                        current_head: refreshed_current,
                    });
                }
                Err(commit_error)
            }
        }
    }

    pub fn select_head(
        &self,
        selector_name: &str,
        expected_revision: Option<u64>,
        head_name: &str,
    ) -> Result<PairedHeadSelectorOutcomeV1, String> {
        validate_selector_name(selector_name)?;
        validate_head_name(head_name)?;
        self.initialize()?;
        let _lease = self.acquire_selector_lease(selector_name)?;
        let current = match latest_selection_revision(&self.head_selector_dir(selector_name))? {
            Some(_) => Some(
                self.load_head_selector_chain(selector_name)?
                    .pop()
                    .ok_or_else(|| {
                        format!("paired head selector {selector_name:?} does not exist")
                    })?,
            ),
            None => None,
        };
        if let Some(selection) = &current {
            if selection.head_name == head_name {
                let head = self.load_head(head_name)?;
                self.require_durable_root_for_selector(selector_name, &head.root_id)?;
                return Ok(PairedHeadSelectorOutcomeV1 {
                    status: PairedHeadSelectorStatusV1::AlreadySelected,
                    selector: selection.clone(),
                });
            }
            if expected_revision != Some(selection.revision) {
                return Err(format!(
                    "paired head selector advanced concurrently from revision {} to {}",
                    expected_revision
                        .map(|revision| revision.to_string())
                        .unwrap_or_else(|| "none".to_owned()),
                    selection.revision
                ));
            }
        } else if expected_revision.is_some() {
            return Err("new paired head selector requires no expected revision".to_owned());
        }
        let head = self.load_head(head_name)?;
        self.require_durable_root_for_selector(selector_name, &head.root_id)?;
        let revision = match current {
            Some(selection) => selection
                .revision
                .checked_add(1)
                .ok_or_else(|| "paired head selector revision overflowed".to_owned())?,
            None => 0,
        };
        let selector = PairedHeadSelectorV1 {
            schema: PAIRED_HEAD_SELECTOR_SCHEMA_V1.to_owned(),
            selector_name: selector_name.to_owned(),
            revision,
            head_name: head_name.to_owned(),
            selected_head_revision: head.revision,
            selected_root_id: head.root_id,
            created_at_unix_ms: unix_ms()?,
        };
        publish_json_immutable(
            &head_selector_revision_path(&self.head_selector_dir(selector_name), revision),
            &selector,
        )?;
        Ok(PairedHeadSelectorOutcomeV1 {
            status: PairedHeadSelectorStatusV1::Selected,
            selector,
        })
    }

    pub fn resolve_head_selector(
        &self,
        selector_name: &str,
    ) -> Result<PairedHeadSelectionResolutionV1, String> {
        validate_selector_name(selector_name)?;
        self.initialize()?;
        let _lease = self.acquire_selector_lease(selector_name)?;
        self.resolve_head_selector_unlocked(selector_name)
    }

    pub fn commit_selected_receipt_idempotent(
        &self,
        selector_name: &str,
        receipt_id: &str,
    ) -> Result<SelectedHeadCommitOutcomeV1, String> {
        validate_selector_name(selector_name)?;
        self.initialize()?;
        let _official_lease = self.acquire_selector_lease(OFFICIAL_HEAD_SELECTOR_V1)?;
        let _selector_lease = if selector_name == OFFICIAL_HEAD_SELECTOR_V1 {
            None
        } else {
            Some(self.acquire_selector_lease(selector_name)?)
        };
        let selected = self.resolve_head_selector_unlocked(selector_name)?;
        let commit =
            self.commit_receipt_idempotent_unlocked(&selected.selector.head_name, receipt_id)?;
        Ok(SelectedHeadCommitOutcomeV1 {
            selector: selected.selector,
            commit,
        })
    }

    pub fn verify_blob(&self, reference: &BlobRefV1) -> Result<(), String> {
        self.content_store.verify_blob(reference)
    }

    /// Reads one content-addressed blob after revalidating its length and
    /// digest. Runtime adapters use this to restore typed payloads without
    /// reaching into the store's private directory layout.
    pub fn read_blob(&self, reference: &BlobRefV1) -> Result<Vec<u8>, String> {
        self.content_store.read_blob(reference)
    }

    /// Reads one exact file from an immutable bundle. The requested path is
    /// matched against the validated manifest; callers cannot traverse or
    /// guess store paths.
    pub fn read_bundle_file(
        &self,
        reference: &BundleRefV1,
        relative_path: &str,
    ) -> Result<Vec<u8>, String> {
        self.content_store
            .read_bundle_file(reference, relative_path)
    }

    /// Materializes one validated immutable bundle into a create-new
    /// directory. Runtime adapters use this instead of reaching into the
    /// store's private blob layout or trusting stale source paths embedded in
    /// an archived payload.
    pub fn materialize_bundle_new(
        &self,
        reference: &BundleRefV1,
        target: &Path,
    ) -> Result<BundleManifestV1, String> {
        self.content_store.materialize_bundle_new(reference, target)
    }

    pub fn verify_bundle(&self, reference: &BundleRefV1) -> Result<BundleManifestV1, String> {
        self.content_store.verify_bundle(reference)
    }

    /// Constructs the only new native Java recovery payload accepted for a
    /// sealed storage capsule. Legacy checkpoint-shaped payloads remain
    /// readable, but new certificates must not manufacture their metadata files.
    pub fn native_storage_recovery_payload(
        &self,
        bundle: &BundleRefV1,
        content_profile: &str,
    ) -> Result<RecoveryPayloadV1, String> {
        require_token(content_profile, "native storage content profile")?;
        let native_autosave_sha256 = self.native_storage_autosave_identity(bundle)?;
        Ok(RecoveryPayloadV1 {
            runtime: RuntimeKindV1::JavaOriginalGame,
            restore_quality: RestoreQualityV1::NativeRoundTripVerified,
            bundle: bundle.clone(),
            entrypoint: NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1.to_owned(),
            reason_code: None,
            source_identity: vec![
                IdentityClaimV1 {
                    kind: "content_profile".to_owned(),
                    value: content_profile.to_owned(),
                },
                IdentityClaimV1 {
                    kind: "native_autosave_sha256".to_owned(),
                    value: native_autosave_sha256,
                },
            ],
        })
    }

    /// Validates the original-game storage shape without granting recovery
    /// quality. Unverified candidates use this narrower query so they never
    /// need to construct a `NativeRoundTripVerified` payload prematurely.
    pub fn native_storage_autosave_identity(&self, bundle: &BundleRefV1) -> Result<String, String> {
        let manifest = self.verify_bundle(bundle)?;
        Ok(validate_native_storage_bundle(&manifest)?
            .blob
            .sha256
            .clone())
    }

    fn validate_root_references(&self, root: &StateRootBodyV2) -> Result<(), String> {
        self.verify_bundle(&root.rust_snapshot.bundle)?;
        if let Some(parent) = &root.parent {
            let parent_root = self.load_root(&parent.root_id)?;
            if parent_root.root.world_provenance != root.world_provenance {
                return Err("paired successor world provenance must equal its parent".to_owned());
            }
            if parent_root.root.versions != root.versions {
                return Err(
                    "paired successor versions must equal its parent versions; start a new native root for a version migration"
                        .to_owned(),
                );
            }
            let commitment = self.load_transition_commitment(&parent.transition_commitment_id)?;
            if commitment.transition.source_root_id != parent.root_id
                || commitment.transition.successor_comparisons != root.preflight.components
            {
                return Err(
                    "paired root parent commitment does not describe this successor".to_owned(),
                );
            }
        }
        for component in &root.preflight.components {
            self.verify_blob(&component.java)?;
            self.verify_blob(&component.rust)?;
        }
        for binding in &root.card_bindings {
            match &binding.evidence {
                CardBindingEvidenceV1::MasterDeckObservation {
                    java_projection,
                    rust_projection,
                    ..
                } => {
                    self.verify_blob(java_projection)?;
                    self.verify_blob(rust_projection)?;
                }
                CardBindingEvidenceV1::CapturedBirth {
                    java_witness,
                    rust_receipt,
                    ..
                } => {
                    self.verify_blob(java_witness)?;
                    self.verify_blob(rust_receipt)?;
                }
                CardBindingEvidenceV1::ConstructedRecipe { recipe, .. }
                | CardBindingEvidenceV1::ConstructedScenario { recipe, .. } => {
                    self.verify_blob(recipe)?;
                }
            }
        }
        for capability in &root.capabilities {
            if let CapabilityStatusV1::Verified { proof } = &capability.status {
                self.verify_blob(proof)?;
            }
        }
        Ok(())
    }

    fn validate_activation_references(
        &self,
        activation: &StateRootActivationBodyV2,
    ) -> Result<(), String> {
        let root = self.load_root(&activation.root_id)?;
        if activation.action_space_id != root.root.preflight.action_space_id {
            return Err("paired activation belongs to another root action space".to_owned());
        }
        let root_bindings = root
            .root
            .card_bindings
            .iter()
            .map(|binding| binding.binding_id.as_str())
            .collect::<BTreeSet<_>>();
        let leased_bindings = activation
            .card_leases
            .iter()
            .map(|lease| lease.binding_id.as_str())
            .collect::<BTreeSet<_>>();
        if leased_bindings != root_bindings {
            return Err(
                "paired activation leases do not cover exactly the root bindings".to_owned(),
            );
        }
        self.verify_blob(&activation.java_frame)
    }

    fn validate_recovery_certificate_references(
        &self,
        certificate: &RecoveryCertificateBodyV2,
    ) -> Result<(), String> {
        let target = self.load_root(&certificate.target_root_id)?;
        validate_native_recovery_target(&target)?;
        if certificate.versions != target.root.versions {
            return Err("recovery certificate versions differ from its target root".to_owned());
        }
        if certificate.restored_components != target.root.preflight.components {
            return Err(
                "recovery certificate round-trip projections differ from its target root"
                    .to_owned(),
            );
        }
        let java_manifest = self.verify_bundle(&certificate.java_recovery.bundle)?;
        if certificate.java_recovery.entrypoint == NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1 {
            let autosave = validate_native_storage_bundle(&java_manifest)?;
            let autosave_claim = certificate
                .java_recovery
                .source_identity
                .iter()
                .find(|claim| claim.kind == "native_autosave_sha256")
                .expect("native storage recovery claim checked before references");
            if !autosave_claim
                .value
                .eq_ignore_ascii_case(&autosave.blob.sha256)
            {
                return Err(
                    "native storage recovery autosave disagrees with its identity claim".to_owned(),
                );
            }
        }
        for component in &certificate.restored_components {
            self.verify_blob(&component.java)?;
            self.verify_blob(&component.rust)?;
        }
        let captured = self.load_activation(&certificate.captured_activation_id)?;
        let restored = self.load_activation(&certificate.restored_activation_id)?;
        if captured.activation.root_id != certificate.target_root_id
            || restored.activation.root_id != certificate.target_root_id
        {
            return Err("recovery certificate activations belong to another root".to_owned());
        }
        if captured.activation.java_runtime_epoch == restored.activation.java_runtime_epoch {
            return Err(
                "recovery certificate round trip must use a fresh Java runtime epoch".to_owned(),
            );
        }
        Ok(())
    }

    fn validate_native_recovery_candidate_references(
        &self,
        candidate: &NativeRecoveryCandidateBodyV2,
    ) -> Result<(), String> {
        let prepared = self.prepare_recovery_certificate_candidate(
            &candidate.target_root_id,
            &candidate.captured_activation_id,
        )?;
        if candidate.versions != prepared.versions
            || candidate.content_profile != prepared.content_profile
        {
            return Err(
                "native recovery candidate drifted from its target root identity".to_owned(),
            );
        }
        let manifest = self.verify_bundle(&candidate.native_storage_bundle)?;
        let autosave = validate_native_storage_bundle(&manifest)?;
        if !candidate
            .native_autosave_sha256
            .eq_ignore_ascii_case(&autosave.blob.sha256)
        {
            return Err(
                "native recovery candidate autosave disagrees with its storage bundle".to_owned(),
            );
        }
        Ok(())
    }

    fn validate_transition_commitment_references(
        &self,
        transition: &TransitionCommitmentBodyV1,
    ) -> Result<(), String> {
        let source = self.load_root(&transition.source_root_id)?;
        if transition.action.action_space_id != source.root.preflight.action_space_id {
            return Err("paired transition commitment uses another action space".to_owned());
        }
        self.verify_blob(&transition.rust_transition_receipt)?;
        for comparison in &transition.successor_comparisons {
            self.verify_blob(&comparison.java)?;
            self.verify_blob(&comparison.rust)?;
        }
        Ok(())
    }

    fn validate_receipt_roots(&self, receipt: &TransitionReceiptBodyV1) -> Result<(), String> {
        let source = self.load_root(&receipt.source_root_id)?;
        if receipt.action.action_space_id != source.root.preflight.action_space_id {
            return Err("paired action handle is based on another action space".to_owned());
        }
        let successor = self.load_root(&receipt.successor_root_id)?;
        if successor.root.parent.as_ref()
            != Some(&ParentStateRootV2 {
                root_id: receipt.source_root_id.clone(),
                transition_commitment_id: receipt.transition_commitment_id.clone(),
            })
        {
            return Err("successor root parent must bind this transition commitment".to_owned());
        }
        if receipt.successor_comparisons != successor.root.preflight.components {
            return Err("receipt comparisons must equal the successor root preflight".to_owned());
        }
        Ok(())
    }

    fn validate_receipt_references(&self, receipt: &TransitionReceiptBodyV1) -> Result<(), String> {
        self.verify_blob(&receipt.java_action_receipt)?;
        self.verify_blob(&receipt.rust_transition_receipt)?;
        for comparison in &receipt.successor_comparisons {
            self.verify_blob(&comparison.java)?;
            self.verify_blob(&comparison.rust)?;
        }
        Ok(())
    }

    fn load_head_chain(&self, head_name: &str) -> Result<Vec<PairedHeadV1>, String> {
        validate_head_name(head_name)?;
        let directory = self.head_dir(head_name);
        let latest = latest_revision(&directory)?
            .ok_or_else(|| format!("paired head {head_name:?} does not exist"))?;
        let capacity = usize::try_from(latest)
            .ok()
            .and_then(|revision| revision.checked_add(1))
            .ok_or_else(|| "paired head revision count overflowed".to_owned())?;
        let mut history: Vec<PairedHeadV1> = Vec::with_capacity(capacity);
        let mut receipt_ids = BTreeSet::new();
        for revision in 0..=latest {
            let head: PairedHeadV1 = read_json(&head_revision_path(&directory, revision))?;
            if head.schema != PAIRED_HEAD_SCHEMA_V1
                || head.head_name != head_name
                || head.revision != revision
            {
                return Err(format!(
                    "paired head revision {revision} identity is invalid"
                ));
            }
            self.load_root(&head.root_id)?;
            if revision == 0 {
                if head.committed_receipt_id.is_some() {
                    return Err("paired head revision 0 cannot commit a receipt".to_owned());
                }
            } else {
                let receipt_id = head.committed_receipt_id.as_deref().ok_or_else(|| {
                    format!("paired head revision {revision} omitted its receipt")
                })?;
                if !receipt_ids.insert(receipt_id.to_owned()) {
                    return Err(format!(
                        "paired head revision {revision} repeats a committed receipt"
                    ));
                }
                let receipt = self.load_receipt(receipt_id)?;
                let previous = history
                    .last()
                    .ok_or_else(|| format!("paired head revision {revision} has no predecessor"))?;
                if receipt.receipt.source_root_id != previous.root_id
                    || receipt.receipt.successor_root_id != head.root_id
                {
                    return Err(format!(
                        "paired head revision {revision} is not linked by its receipt"
                    ));
                }
            }
            history.push(head);
        }
        Ok(history)
    }

    fn load_head_selector_chain(
        &self,
        selector_name: &str,
    ) -> Result<Vec<PairedHeadSelectorV1>, String> {
        validate_selector_name(selector_name)?;
        let directory = self.head_selector_dir(selector_name);
        let latest = latest_selection_revision(&directory)?
            .ok_or_else(|| format!("paired head selector {selector_name:?} does not exist"))?;
        let capacity = usize::try_from(latest)
            .ok()
            .and_then(|revision| revision.checked_add(1))
            .ok_or_else(|| "paired head selector revision count overflowed".to_owned())?;
        let mut history = Vec::with_capacity(capacity);
        for revision in 0..=latest {
            let selector: PairedHeadSelectorV1 =
                read_json(&head_selector_revision_path(&directory, revision))?;
            if selector.schema != PAIRED_HEAD_SELECTOR_SCHEMA_V1
                || selector.selector_name != selector_name
                || selector.revision != revision
            {
                return Err(format!(
                    "paired head selector revision {revision} identity is invalid"
                ));
            }
            validate_head_name(&selector.head_name)?;
            let head_history = self.load_head_chain(&selector.head_name)?;
            let selected_index = usize::try_from(selector.selected_head_revision)
                .map_err(|_| "selected paired head revision overflowed usize".to_owned())?;
            let selected_head = head_history.get(selected_index).ok_or_else(|| {
                format!("paired head selector revision {revision} points beyond its head history")
            })?;
            if selected_head.root_id != selector.selected_root_id {
                return Err(format!(
                    "paired head selector revision {revision} changed its selected root"
                ));
            }
            history.push(selector);
        }
        Ok(history)
    }

    fn resolve_head_selector_unlocked(
        &self,
        selector_name: &str,
    ) -> Result<PairedHeadSelectionResolutionV1, String> {
        let selector = self
            .load_head_selector_chain(selector_name)?
            .pop()
            .ok_or_else(|| format!("paired head selector {selector_name:?} does not exist"))?;
        let head = self.load_head(&selector.head_name)?;
        self.require_durable_root_for_selector(selector_name, &head.root_id)?;
        Ok(PairedHeadSelectionResolutionV1 { selector, head })
    }

    fn require_durable_root_for_selector(
        &self,
        selector_name: &str,
        root_id: &str,
    ) -> Result<(), String> {
        if selector_name != OFFICIAL_HEAD_SELECTOR_V1 {
            return Ok(());
        }
        self.recovery_plan(root_id).map(|_| ()).map_err(|error| {
            format!(
                "official head requires a verified fresh-JVM recovery certificate chain: {error}"
            )
        })
    }

    fn official_selects_head_unlocked(&self, head_name: &str) -> Result<bool, String> {
        let directory = self.head_selector_dir(OFFICIAL_HEAD_SELECTOR_V1);
        if latest_selection_revision(&directory)?.is_none() {
            return Ok(false);
        }
        Ok(self
            .load_head_selector_chain(OFFICIAL_HEAD_SELECTOR_V1)?
            .last()
            .is_some_and(|selector| selector.head_name == head_name))
    }

    fn acquire_selector_lease(&self, selector_name: &str) -> Result<std::fs::File, String> {
        let directory = self.head_selector_dir(selector_name);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        let path = directory.join("selector.lease");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
        file.try_lock_exclusive()
            .map_err(|error| format!("paired head selector {selector_name:?} is busy: {error}"))?;
        Ok(file)
    }

    fn roots_dir(&self) -> PathBuf {
        self.root.join("state-roots-v2")
    }
    fn activations_dir(&self) -> PathBuf {
        self.root.join("state-root-activations-v2")
    }
    fn native_recovery_candidates_dir(&self) -> PathBuf {
        self.root.join("state-root-native-recovery-candidates-v2")
    }
    fn native_recovery_candidate_root_dir(&self, target_root_id: &str) -> PathBuf {
        self.native_recovery_candidates_dir().join(target_root_id)
    }
    fn recovery_certificates_dir(&self) -> PathBuf {
        self.root.join("state-root-recovery-certificates-v2")
    }
    fn recovery_certificate_root_dir(&self, target_root_id: &str) -> PathBuf {
        self.recovery_certificates_dir().join(target_root_id)
    }
    fn transition_commitments_dir(&self) -> PathBuf {
        self.root.join("state-transition-commitments-v2")
    }
    fn receipts_dir(&self) -> PathBuf {
        self.root.join("state-transition-receipts-v2")
    }
    fn divergences_dir(&self) -> PathBuf {
        self.root.join("state-divergences-v2")
    }
    fn heads_dir(&self) -> PathBuf {
        self.root.join("state-heads-v2")
    }
    fn head_selectors_dir(&self) -> PathBuf {
        self.root.join("state-head-selectors-v2")
    }
    fn root_path(&self, root_id: &str) -> PathBuf {
        self.roots_dir().join(format!("{root_id}.json"))
    }
    fn activation_path(&self, activation_id: &str) -> PathBuf {
        self.activations_dir().join(format!("{activation_id}.json"))
    }
    fn native_recovery_candidate_path(
        &self,
        target_root_id: &str,
        native_recovery_candidate_id: &str,
    ) -> PathBuf {
        self.native_recovery_candidate_root_dir(target_root_id)
            .join(format!("{native_recovery_candidate_id}.json"))
    }
    fn recovery_certificate_path(
        &self,
        target_root_id: &str,
        recovery_certificate_id: &str,
    ) -> PathBuf {
        self.recovery_certificate_root_dir(target_root_id)
            .join(format!("{recovery_certificate_id}.json"))
    }
    fn transition_commitment_path(&self, transition_commitment_id: &str) -> PathBuf {
        self.transition_commitments_dir()
            .join(format!("{transition_commitment_id}.json"))
    }
    fn receipt_path(&self, receipt_id: &str) -> PathBuf {
        self.receipts_dir().join(format!("{receipt_id}.json"))
    }
    fn divergence_path(&self, divergence_id: &str) -> PathBuf {
        self.divergences_dir().join(format!("{divergence_id}.json"))
    }
    fn head_dir(&self, head_name: &str) -> PathBuf {
        self.heads_dir().join(head_name)
    }
    fn head_selector_dir(&self, selector_name: &str) -> PathBuf {
        self.head_selectors_dir().join(selector_name)
    }
}

/// Commits to the cross-epoch logical transition. Runtime-local Java receipts
/// and timings remain in the receipt artifact but cannot change the child
/// root's parent identity when the same transition is replayed in a fresh JVM.
pub fn transition_commitment_id(receipt: &TransitionReceiptBodyV1) -> Result<String, String> {
    transition_commitment_id_from_body(&transition_commitment_body(receipt))
}

pub fn transition_commitment_body(receipt: &TransitionReceiptBodyV1) -> TransitionCommitmentBodyV1 {
    TransitionCommitmentBodyV1 {
        source_root_id: receipt.source_root_id.clone(),
        action: receipt.action.clone(),
        rust_transition_receipt: receipt.rust_transition_receipt.clone(),
        successor_comparisons: receipt.successor_comparisons.clone(),
    }
}

fn transition_commitment_id_from_body(
    transition: &TransitionCommitmentBodyV1,
) -> Result<String, String> {
    Ok(content_id(
        "state-transition-commitment-v2",
        &canonical_bytes(&(
            &transition.source_root_id,
            &transition.action,
            &transition.rust_transition_receipt,
            &transition.successor_comparisons,
        ))?,
    ))
}

pub fn action_handle(
    source_root_id: &str,
    action_space_id: &str,
    action: PairedActionV1,
) -> Result<PairedActionHandleV1, String> {
    require_content_id(source_root_id, "state-root-v2", "action source_root_id")?;
    require_id(action_space_id, "action action_space_id")?;
    validate_action(&action)?;
    let handle_id = content_id(
        "paired-action-v1",
        &canonical_bytes(&(source_root_id, action_space_id, &action))?,
    );
    Ok(PairedActionHandleV1 {
        handle_id,
        source_root_id: source_root_id.to_owned(),
        action_space_id: action_space_id.to_owned(),
        action,
    })
}

pub fn card_binding_id(binding: &CrossRuntimeCardBindingV1) -> Result<String, String> {
    let mut body = binding.clone();
    body.binding_id.clear();
    Ok(content_id(
        "paired-card-binding-v1",
        &canonical_bytes(&body)?,
    ))
}

fn validate_root(root: &StateRootBodyV2) -> Result<(), String> {
    require_token(&root.content_profile, "state root content profile")?;
    require_id(&root.preflight.action_space_id, "preflight action_space_id")?;
    require_strict_order(&root.versions, "paired root versions", |left, right| {
        (&left.component, &left.identity) < (&right.component, &right.identity)
    })?;
    validate_rust_state_snapshot(&root.rust_snapshot)?;
    if let Some(parent) = &root.parent {
        require_content_id(&parent.root_id, "state-root-v2", "parent root ID")?;
        require_content_id(
            &parent.transition_commitment_id,
            "state-transition-commitment-v2",
            "parent transition commitment ID",
        )?;
    }
    let mut domains = BTreeMap::new();
    let mut last_domain = None;
    for component in &root.preflight.components {
        if last_domain.is_some_and(|previous| previous >= component.domain) {
            return Err("paired root projection domains are not in canonical order".to_owned());
        }
        last_domain = Some(component.domain);
        if domains.insert(component.domain, component.status).is_some() {
            return Err(format!(
                "paired root repeats projection domain {:?}",
                component.domain
            ));
        }
        match (component.status, component.first_difference.is_some()) {
            (ComparisonStatusV1::Exact, true)
            | (ComparisonStatusV1::Diverged, false)
            | (ComparisonStatusV1::Unknown, true) => {
                return Err(format!(
                    "projection domain {:?} has inconsistent difference evidence",
                    component.domain
                ));
            }
            _ => {}
        }
        if component.status == ComparisonStatusV1::Diverged {
            return Err(format!(
                "a diverged projection cannot become a paired root: {:?}",
                component.domain
            ));
        }
    }
    let gap_ids = root
        .preflight
        .uncovered
        .iter()
        .map(|gap| gap.gap_id.as_str())
        .collect::<BTreeSet<_>>();
    if gap_ids.len() != root.preflight.uncovered.len() || gap_ids.contains("") {
        return Err("paired root coverage gap IDs are empty or duplicated".to_owned());
    }
    require_strict_order(
        &root.preflight.uncovered,
        "paired root coverage gaps",
        |left, right| left.gap_id < right.gap_id,
    )?;

    let mut rust_lineages = BTreeSet::new();
    let mut binding_ids = BTreeSet::new();
    for binding in &root.card_bindings {
        let evidence_matches_lineage = match (&binding.rust_lineage, &binding.evidence) {
            (
                RustCardLineageV1::MasterDeck { .. },
                CardBindingEvidenceV1::MasterDeckObservation { .. },
            )
            | (
                RustCardLineageV1::CombatGenerated { .. },
                CardBindingEvidenceV1::CapturedBirth { .. },
            ) => true,
            (
                RustCardLineageV1::ConstructedScenario { recipe_id, slot },
                CardBindingEvidenceV1::ConstructedRecipe {
                    recipe_id: evidence_recipe_id,
                    slot: evidence_slot,
                    ..
                },
            ) => recipe_id == evidence_recipe_id && slot == evidence_slot,
            (
                RustCardLineageV1::ConstructedScenarioInstance {
                    scenario_instance_id,
                    slot,
                },
                CardBindingEvidenceV1::ConstructedScenario {
                    scenario_instance_id: evidence_scenario_instance_id,
                    slot: evidence_slot,
                    ..
                },
            ) => scenario_instance_id == evidence_scenario_instance_id && slot == evidence_slot,
            _ => false,
        };
        let expected = card_binding_id(binding)?;
        if binding.binding_id != expected
            || !binding_ids.insert(binding.binding_id.as_str())
            || !rust_lineages.insert(&binding.rust_lineage)
            || !evidence_matches_lineage
        {
            return Err("paired root card bindings are invalid, duplicated, or aliased".to_owned());
        }
    }
    require_strict_order(
        &root.card_bindings,
        "paired root card bindings",
        |left, right| left.binding_id < right.binding_id,
    )?;

    let mut capabilities = BTreeSet::new();
    for capability in &root.capabilities {
        if !capabilities.insert(capability.capability) {
            return Err("paired root repeats a capability".to_owned());
        }
        let required = capability
            .required_domains
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if required.len() != capability.required_domains.len() {
            return Err("paired root capability repeats a required domain".to_owned());
        }
        if capability
            .required_domains
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err("paired root capability domains are not in canonical order".to_owned());
        }
        match &capability.status {
            CapabilityStatusV1::Verified { .. } => {
                if capability
                    .required_domains
                    .iter()
                    .any(|domain| domains.get(domain) != Some(&ComparisonStatusV1::Exact))
                {
                    return Err("verified capability depends on a non-exact domain".to_owned());
                }
            }
            CapabilityStatusV1::Unknown { gap_id } => {
                if !gap_ids.contains(gap_id.as_str()) {
                    return Err("unknown capability references an absent coverage gap".to_owned());
                }
            }
            CapabilityStatusV1::Unsupported { reason } => {
                require_token(reason, "unsupported capability reason")?;
            }
        }
    }
    if root
        .capabilities
        .windows(2)
        .any(|pair| pair[0].capability >= pair[1].capability)
    {
        return Err("paired root capabilities are not in canonical order".to_owned());
    }
    Ok(())
}

fn validate_activation(activation: &StateRootActivationBodyV2) -> Result<(), String> {
    require_content_id(&activation.root_id, "state-root-v2", "activation root_id")?;
    require_id(
        &activation.java_runtime_epoch,
        "activation Java runtime epoch",
    )?;
    require_id(&activation.action_space_id, "activation action_space_id")?;
    let mut binding_ids = BTreeSet::new();
    let mut java_uuids = BTreeSet::new();
    for lease in &activation.card_leases {
        require_content_id(
            &lease.binding_id,
            "paired-card-binding-v1",
            "activation binding ID",
        )?;
        require_id(&lease.java_card_uuid, "activation Java card UUID")?;
        if !binding_ids.insert(lease.binding_id.as_str())
            || !java_uuids.insert(lease.java_card_uuid.as_str())
        {
            return Err("paired activation card leases are duplicated or aliased".to_owned());
        }
    }
    require_strict_order(
        &activation.card_leases,
        "paired activation card leases",
        |left, right| left.binding_id < right.binding_id,
    )
}

fn validate_recovery_certificate(certificate: &RecoveryCertificateBodyV2) -> Result<(), String> {
    require_content_id(
        &certificate.target_root_id,
        "state-root-v2",
        "recovery certificate target root ID",
    )?;
    let supported_recovery = matches!(
        (
            certificate.java_recovery.restore_quality,
            certificate.java_recovery.entrypoint.as_str(),
        ),
        (
            RestoreQualityV1::NativeRoundTripVerified,
            "native_checkpoint_continue"
        ) | (
            RestoreQualityV1::NativeRoundTripVerified,
            NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1
        ) | (
            RestoreQualityV1::TypedReconstructedVerified,
            "native_checkpoint_continue_then_neow_intro"
        )
    );
    if certificate.java_recovery.runtime != RuntimeKindV1::JavaOriginalGame || !supported_recovery {
        return Err(
            "recovery certificate must contain a supported native round-trip Java recovery"
                .to_owned(),
        );
    }
    validate_recovery_payload(&certificate.java_recovery)?;
    require_strict_order(
        &certificate.versions,
        "recovery certificate versions",
        |left, right| (&left.component, &left.identity) < (&right.component, &right.identity),
    )?;
    require_content_id(
        &certificate.captured_activation_id,
        "state-root-activation-v2",
        "recovery certificate captured activation ID",
    )?;
    require_content_id(
        &certificate.restored_activation_id,
        "state-root-activation-v2",
        "recovery certificate restored activation ID",
    )?;
    if certificate.captured_activation_id == certificate.restored_activation_id {
        return Err(
            "recovery certificate requires distinct capture and restore activations".to_owned(),
        );
    }
    if certificate.restored_components.is_empty()
        || certificate
            .restored_components
            .iter()
            .any(|component| component.status != ComparisonStatusV1::Exact)
        || certificate
            .restored_components
            .windows(2)
            .any(|pair| pair[0].domain >= pair[1].domain)
    {
        return Err(
            "recovery certificate requires a canonical non-empty Exact projection set".to_owned(),
        );
    }
    let native_autosave_claims = certificate
        .java_recovery
        .source_identity
        .iter()
        .filter(|claim| claim.kind == "native_autosave_sha256")
        .collect::<Vec<_>>();
    if native_autosave_claims.len() != 1 {
        return Err("recovery certificate requires one native autosave identity".to_owned());
    }
    require_sha256(
        &native_autosave_claims[0].value,
        "recovery certificate native autosave identity",
    )?;
    let checkpoint_claims = certificate
        .java_recovery
        .source_identity
        .iter()
        .filter(|claim| claim.kind == "checkpoint_manifest_sha256")
        .collect::<Vec<_>>();
    if certificate.java_recovery.entrypoint == "native_checkpoint_continue" {
        if checkpoint_claims.len() != 1 {
            return Err(
                "recovery certificate requires one checkpoint manifest identity".to_owned(),
            );
        }
        require_sha256(
            &checkpoint_claims[0].value,
            "recovery certificate checkpoint manifest identity",
        )?;
    } else if !checkpoint_claims.is_empty() {
        return Err("native storage recovery must not claim legacy checkpoint metadata".to_owned());
    }
    let content_profiles = certificate
        .java_recovery
        .source_identity
        .iter()
        .filter(|claim| claim.kind == "content_profile")
        .collect::<Vec<_>>();
    if content_profiles.len() != 1 {
        return Err("recovery certificate requires one content profile identity".to_owned());
    }
    require_token(
        &content_profiles[0].value,
        "recovery certificate content profile identity",
    )?;
    if certificate.java_recovery.entrypoint == NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1
        && (certificate.java_recovery.source_identity.len() != 2
            || certificate.java_recovery.source_identity[0].kind != "content_profile"
            || certificate.java_recovery.source_identity[1].kind != "native_autosave_sha256")
    {
        return Err(
            "native storage recovery requires exactly content profile and autosave identities"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_native_recovery_candidate(
    candidate: &NativeRecoveryCandidateBodyV2,
) -> Result<(), String> {
    require_content_id(
        &candidate.target_root_id,
        "state-root-v2",
        "native recovery candidate target root ID",
    )?;
    require_content_id(
        &candidate.captured_activation_id,
        "state-root-activation-v2",
        "native recovery candidate capture activation ID",
    )?;
    require_strict_order(
        &candidate.versions,
        "native recovery candidate versions",
        |left, right| (&left.component, &left.identity) < (&right.component, &right.identity),
    )?;
    require_token(
        &candidate.content_profile,
        "native recovery candidate content profile",
    )?;
    require_token(
        &candidate.recovery_ref,
        "native recovery candidate capsule reference",
    )?;
    require_sha256(
        &candidate.native_autosave_sha256,
        "native recovery candidate autosave identity",
    )
}

fn validate_native_storage_bundle(manifest: &BundleManifestV1) -> Result<&BundleFileV1, String> {
    let mut has_preferences = false;
    let mut has_saves = false;
    for file in &manifest.files {
        let Some((directory, relative)) = file.relative_path.split_once('/') else {
            return Err(
                "native storage recovery bundle contains a file outside its storage directories"
                    .to_owned(),
            );
        };
        if relative.is_empty() || !NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1.contains(&directory) {
            return Err(
                "native storage recovery bundle contains an unsupported top-level path".to_owned(),
            );
        }
        has_preferences |= directory == "preferences";
        has_saves |= directory == "saves";
    }
    if !has_preferences || !has_saves {
        return Err(
            "native storage recovery bundle requires preferences and saves content".to_owned(),
        );
    }
    let autosaves = manifest
        .files
        .iter()
        .filter(|file| {
            let Some(name) = file.relative_path.strip_prefix("saves/") else {
                return false;
            };
            !name.contains('/')
                && name
                    .strip_suffix("IRONCLAD.autosave")
                    .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with('_'))
        })
        .collect::<Vec<_>>();
    if autosaves.len() != 1 {
        return Err(format!(
            "native storage recovery bundle contains {} Ironclad autosaves instead of one",
            autosaves.len()
        ));
    }
    Ok(autosaves[0])
}

fn validate_native_recovery_target(target: &StateRootArtifactV2) -> Result<(), String> {
    if target.root.world_provenance != WorldProvenanceV1::Natural {
        return Err("native recovery certificates require a natural target root".to_owned());
    }
    if target.root.preflight.components.is_empty()
        || target
            .root
            .preflight
            .components
            .iter()
            .any(|component| component.status != ComparisonStatusV1::Exact)
    {
        return Err(
            "native recovery certificate target requires an Exact state projection".to_owned(),
        );
    }
    if target
        .root
        .preflight
        .uncovered
        .iter()
        .any(|gap| gap.scope != CoverageGapScopeV1::ActionCapability)
    {
        return Err(
            "native recovery certificate target has an unresolved state-projection gap".to_owned(),
        );
    }
    Ok(())
}

fn validate_transition_commitment(transition: &TransitionCommitmentBodyV1) -> Result<(), String> {
    require_content_id(
        &transition.source_root_id,
        "state-root-v2",
        "transition commitment source_root_id",
    )?;
    if transition.action.source_root_id != transition.source_root_id {
        return Err("transition commitment action belongs to another source root".to_owned());
    }
    validate_action_handle(&transition.action)?;
    if transition
        .successor_comparisons
        .iter()
        .any(|comparison| comparison.status != ComparisonStatusV1::Exact)
    {
        return Err("a transition commitment may contain only exact comparisons".to_owned());
    }
    if transition
        .successor_comparisons
        .windows(2)
        .any(|pair| pair[0].domain >= pair[1].domain)
    {
        return Err("transition commitment comparisons are not in canonical order".to_owned());
    }
    Ok(())
}

fn validate_receipt(receipt: &TransitionReceiptBodyV1) -> Result<(), String> {
    if receipt.transition_commitment_id != transition_commitment_id(receipt)? {
        return Err("paired transition commitment identity is invalid".to_owned());
    }
    require_content_id(
        &receipt.transition_commitment_id,
        "state-transition-commitment-v2",
        "receipt transition commitment ID",
    )?;
    require_content_id(
        &receipt.source_root_id,
        "state-root-v2",
        "receipt source_root_id",
    )?;
    require_content_id(
        &receipt.successor_root_id,
        "state-root-v2",
        "receipt successor_root_id",
    )?;
    if receipt.source_root_id == receipt.successor_root_id {
        return Err("paired transition must create a distinct successor root".to_owned());
    }
    if receipt.action.source_root_id != receipt.source_root_id {
        return Err("paired action handle is based on another source root".to_owned());
    }
    validate_action_handle(&receipt.action)?;
    if receipt
        .successor_comparisons
        .iter()
        .any(|comparison| comparison.status != ComparisonStatusV1::Exact)
    {
        return Err("a transition receipt may contain only exact successor comparisons".to_owned());
    }
    if receipt
        .successor_comparisons
        .windows(2)
        .any(|pair| pair[0].domain >= pair[1].domain)
    {
        return Err("transition receipt comparisons are not in canonical order".to_owned());
    }
    Ok(())
}

fn validate_recovery_payload(payload: &RecoveryPayloadV1) -> Result<(), String> {
    require_token(&payload.entrypoint, "recovery entrypoint")?;
    if let Some(reason_code) = &payload.reason_code {
        require_token(reason_code, "recovery reason code")?;
    }
    if payload.restore_quality != RestoreQualityV1::Unsupported && payload.reason_code.is_some() {
        return Err("only unsupported recovery may carry a reason code".to_owned());
    }
    require_strict_order(
        &payload.source_identity,
        "recovery source identities",
        |left, right| (&left.kind, &left.value) < (&right.kind, &right.value),
    )
}

fn validate_rust_state_snapshot(snapshot: &RustStateSnapshotV2) -> Result<(), String> {
    require_token(&snapshot.entrypoint, "Rust state snapshot entrypoint")?;
    require_strict_order(
        &snapshot.source_identity,
        "Rust state snapshot source identities",
        |left, right| (&left.kind, &left.value) < (&right.kind, &right.value),
    )
}

fn require_strict_order<T>(
    values: &[T],
    label: &str,
    less: impl Fn(&T, &T) -> bool,
) -> Result<(), String> {
    if values.windows(2).any(|pair| !less(&pair[0], &pair[1])) {
        return Err(format!("{label} are not in canonical order"));
    }
    Ok(())
}

fn latest_revision(directory: &Path) -> Result<Option<u64>, String> {
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut latest = None;
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("failed to inspect {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(value) = name
            .strip_prefix("head-")
            .and_then(|value| value.strip_suffix(".json"))
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        latest = Some(latest.map_or(value, |current: u64| current.max(value)));
    }
    Ok(latest)
}

fn latest_selection_revision(directory: &Path) -> Result<Option<u64>, String> {
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut latest = None;
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("failed to inspect {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(value) = name
            .strip_prefix("selection-")
            .and_then(|value| value.strip_suffix(".json"))
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        latest = Some(latest.map_or(value, |current: u64| current.max(value)));
    }
    Ok(latest)
}

fn head_revision_path(directory: &Path, revision: u64) -> PathBuf {
    directory.join(format!("head-{revision:020}.json"))
}

fn head_selector_revision_path(directory: &Path, revision: u64) -> PathBuf {
    directory.join(format!("selection-{revision:020}.json"))
}

fn publish_json_immutable<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {}: {error}", path.display()))?;
    publish_immutable(path, &bytes)
}

fn publish_immutable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file
            .write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("failed to publish {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(path)
                .map_err(|read_error| format!("failed to read {}: {read_error}", path.display()))?;
            if existing == bytes {
                Ok(())
            } else {
                Err(format!(
                    "immutable artifact already exists with different bytes: {}",
                    path.display()
                ))
            }
        }
        Err(error) => Err(format!("failed to create {}: {error}", path.display())),
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))
}

fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| format!("failed to encode canonical JSON: {error}"))
}

fn content_id(domain: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    format!("{domain}-{}", hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn require_id(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 256 {
        return Err(format!("{label} must contain 1..=256 bytes"));
    }
    Ok(())
}

fn require_sha256(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} must be 64 lowercase hexadecimal bytes"));
    }
    Ok(())
}

fn require_content_id(value: &str, domain: &str, label: &str) -> Result<(), String> {
    let Some(digest) = value
        .strip_prefix(domain)
        .and_then(|value| value.strip_prefix('-'))
    else {
        return Err(format!("{label} has the wrong content-address domain"));
    };
    require_sha256(digest, label)
}

fn validate_action_handle(handle: &PairedActionHandleV1) -> Result<(), String> {
    require_content_id(
        &handle.source_root_id,
        "state-root-v2",
        "action source_root_id",
    )?;
    require_id(&handle.action_space_id, "action action_space_id")?;
    validate_action(&handle.action)?;
    let expected = content_id(
        "paired-action-v1",
        &canonical_bytes(&(
            &handle.source_root_id,
            &handle.action_space_id,
            &handle.action,
        ))?,
    );
    if handle.handle_id != expected {
        return Err("paired action handle identity is invalid".to_owned());
    }
    Ok(())
}

fn validate_action(action: &PairedActionV1) -> Result<(), String> {
    match action {
        PairedActionV1::ChooseNeowOption { .. } | PairedActionV1::LeaveNeow => {}
        PairedActionV1::PlayCard { binding_id, target } => {
            require_content_id(binding_id, "paired-card-binding-v1", "play-card binding ID")?;
            if let Some(target) = target {
                require_token(&target.enemy_id, "play-card target enemy ID")?;
            }
        }
        PairedActionV1::UsePotion {
            potion_id,
            potency,
            target,
            ..
        } => {
            require_token(potion_id, "potion action identity")?;
            if *potency < 0 {
                return Err("potion action potency is negative".to_owned());
            }
            if let Some(target) = target {
                require_token(&target.enemy_id, "potion target enemy ID")?;
            }
        }
        PairedActionV1::ClaimCombatReward { reward, .. } => match reward {
            CombatRewardIdentityV1::Gold { amount }
            | CombatRewardIdentityV1::StolenGold { amount } => {
                if *amount < 0 {
                    return Err("combat reward amount is negative".to_owned());
                }
            }
            CombatRewardIdentityV1::Card { choice_count } => {
                if *choice_count == 0 {
                    return Err("card reward choice count is zero".to_owned());
                }
            }
            CombatRewardIdentityV1::Potion { potion_id } => {
                require_token(potion_id, "combat reward potion identity")?;
            }
        },
        PairedActionV1::ChooseCardReward { card_id, .. } => {
            require_token(card_id, "card reward identity")?;
        }
        PairedActionV1::SkipCardReward => {}
        PairedActionV1::CloseCombatReward => {}
        PairedActionV1::ChooseEventOption { event_id, .. } => {
            require_token(event_id, "event action identity")?;
        }
        PairedActionV1::ChooseMapNode { x, y, .. } => {
            if *x < 0 || *y < 0 {
                return Err("paired map action coordinates must be non-negative".to_owned());
            }
        }
        PairedActionV1::EndTurn => {}
    }
    Ok(())
}

fn require_token(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!(
            "{label} must be a non-control token of 1..=256 bytes"
        ));
    }
    Ok(())
}

fn validate_head_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("paired head name must match [A-Za-z0-9_-]{1,80}".to_owned());
    }
    Ok(())
}

fn validate_selector_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("paired head selector name must match [A-Za-z0-9_-]{1,80}".to_owned());
    }
    Ok(())
}

fn unix_ms() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blobs_and_roots_are_content_addressed() {
        let directory = temporary_directory("content");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        assert_eq!(blob, store.put_bytes("application/json", b"{}").unwrap());
        let root = fixture_root(&store, blob);
        let first = store.create_root(root.clone()).unwrap();
        let second = store.create_root(root).unwrap();
        assert_eq!(first.root_id, second.root_id);
        assert_eq!(first, store.load_root(&first.root_id).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn bundle_files_are_read_only_through_validated_relative_paths() {
        let directory = temporary_directory("bundle-read");
        let payload = directory.join("payload");
        fs::create_dir_all(payload.join("nested")).unwrap();
        fs::write(payload.join("nested").join("checkpoint.bin"), b"exact").unwrap();
        let store = StateRootStoreV2::new(directory.join("store"));
        let bundle = store.put_directory(&payload).unwrap();

        assert_eq!(
            store
                .read_bundle_file(&bundle, "nested/checkpoint.bin")
                .unwrap(),
            b"exact"
        );
        assert!(store.read_bundle_file(&bundle, "../escape").is_err());
        assert!(store.read_bundle_file(&bundle, "missing.bin").is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn bundle_materialization_is_create_new_and_content_exact() {
        let directory = temporary_directory("bundle-materialize");
        let payload = directory.join("payload");
        fs::create_dir_all(payload.join("storage").join("saves")).unwrap();
        fs::write(payload.join("checkpoint.json"), b"manifest").unwrap();
        fs::write(
            payload.join("storage").join("saves").join("run.autosave"),
            b"native",
        )
        .unwrap();
        let store = StateRootStoreV2::new(directory.join("store"));
        let bundle = store.put_directory(&payload).unwrap();
        let target = directory.join("materialized");

        let manifest = store.materialize_bundle_new(&bundle, &target).unwrap();
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(
            fs::read(target.join("checkpoint.json")).unwrap(),
            b"manifest"
        );
        assert_eq!(
            fs::read(target.join("storage").join("saves").join("run.autosave")).unwrap(),
            b"native"
        );
        assert!(store.materialize_bundle_new(&bundle, &target).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn diverged_preflight_cannot_become_a_root() {
        let directory = temporary_directory("diverged");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut root = fixture_root(&store, blob);
        root.preflight.components[0].status = ComparisonStatusV1::Diverged;
        root.preflight.components[0].first_difference = Some(TypedDifferenceV1 {
            path: "run.hp".to_owned(),
            kind: "value_mismatch".to_owned(),
            detail: "Java=1 Rust=2".to_owned(),
        });
        assert!(store
            .create_root(root)
            .unwrap_err()
            .contains("cannot become"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn capability_surface_plan_materializes_unknown_with_its_owned_gap() {
        let gap = CoverageGapV1 {
            gap_id: "dynamic-successor-unverified-v1".to_owned(),
            domain: ProjectionDomainV1::ActionSpace,
            scope: CoverageGapScopeV1::ActionCapability,
            kind: "capability_not_verified".to_owned(),
            detail: "fixture dynamic successor is not verified".to_owned(),
        };
        let plan = CapabilitySurfacePlanV1::try_new(
            Vec::new(),
            vec![CapabilityPlanV1 {
                capability: CapabilityKindV1::EnterQuestionMark,
                required_domains: vec![ProjectionDomainV1::ActionSpace],
                status: CapabilityPlanStatusV1::Unknown { gap: gap.clone() },
            }],
        )
        .unwrap();
        let proof = BlobRefV1 {
            sha256: "a".repeat(64),
            bytes: 1,
            media_type: "application/json".to_owned(),
        };

        let (uncovered, capabilities) = plan.materialize(&proof);

        assert_eq!(uncovered, vec![gap.clone()]);
        assert_eq!(
            capabilities[0].status,
            CapabilityStatusV1::Unknown { gap_id: gap.gap_id }
        );
    }

    #[test]
    fn capability_surface_plan_rejects_duplicate_owned_gap_identity() {
        let gap = CoverageGapV1 {
            gap_id: "duplicate-gap-v1".to_owned(),
            domain: ProjectionDomainV1::ActionSpace,
            scope: CoverageGapScopeV1::ActionCapability,
            kind: "capability_not_verified".to_owned(),
            detail: "fixture duplicate".to_owned(),
        };
        let error = CapabilitySurfacePlanV1::try_new(
            vec![gap.clone()],
            vec![CapabilityPlanV1 {
                capability: CapabilityKindV1::EnterQuestionMark,
                required_domains: vec![ProjectionDomainV1::ActionSpace],
                status: CapabilityPlanStatusV1::Unknown { gap },
            }],
        )
        .unwrap_err();

        assert!(error.contains("gap IDs are empty or duplicated"));
    }

    #[test]
    fn head_commit_only_moves_across_a_bound_receipt() {
        let directory = temporary_directory("commit");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let source = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        store.create_head("probe", &source.root_id).unwrap();
        let action = action_handle(
            &source.root_id,
            &source.root.preflight.action_space_id,
            PairedActionV1::EndTurn,
        )
        .unwrap();
        let mut receipt_body = TransitionReceiptBodyV1 {
            transition_commitment_id: String::new(),
            source_root_id: source.root_id.clone(),
            successor_root_id: "pending".to_owned(),
            action,
            java_action_receipt: blob.clone(),
            rust_transition_receipt: blob.clone(),
            successor_comparisons: source.root.preflight.components.clone(),
            timings: TransitionTimingV1 {
                rust_execution_ms: 1,
                java_execution_ms: 2,
                comparison_ms: 1,
            },
        };
        let commitment = store
            .create_transition_commitment(transition_commitment_body(&receipt_body))
            .unwrap();
        receipt_body.transition_commitment_id = commitment.transition_commitment_id;
        let mut successor_body = source.root.clone();
        successor_body.parent = Some(ParentStateRootV2 {
            root_id: source.root_id.clone(),
            transition_commitment_id: receipt_body.transition_commitment_id.clone(),
        });
        let successor = store.create_root(successor_body).unwrap();
        receipt_body.successor_root_id = successor.root_id.clone();
        let receipt = store.create_receipt(receipt_body).unwrap();
        let head = store
            .commit_receipt("probe", 0, &receipt.receipt_id)
            .unwrap();
        assert_eq!(head.revision, 1);
        assert_eq!(head.root_id, successor.root_id);
        assert!(store
            .commit_receipt("probe", 0, &receipt.receipt_id)
            .unwrap_err()
            .contains("advanced concurrently"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn idempotent_head_commit_finds_the_original_revision_after_later_progress() {
        let directory = temporary_directory("idempotent-commit");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let source = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (first, first_receipt) = fixture_successor(&store, &source, blob.clone());
        let (second, second_receipt) = fixture_successor(&store, &first, blob);
        store.create_head("probe", &source.root_id).unwrap();

        let first_commit = store
            .commit_receipt_idempotent("probe", &first_receipt.receipt_id)
            .unwrap();
        assert_eq!(first_commit.status, PairedHeadCommitStatusV1::Committed);
        assert_eq!(first_commit.committed_head.revision, 1);
        let immediate_retry = store
            .commit_receipt_idempotent("probe", &first_receipt.receipt_id)
            .unwrap();
        assert_eq!(
            immediate_retry.status,
            PairedHeadCommitStatusV1::AlreadyCommitted
        );
        assert_eq!(immediate_retry.current_head.revision, 1);

        let second_commit = store
            .commit_receipt_idempotent("probe", &second_receipt.receipt_id)
            .unwrap();
        assert_eq!(second_commit.status, PairedHeadCommitStatusV1::Committed);
        assert_eq!(second_commit.current_head.root_id, second.root_id);
        let delayed_retry = store
            .commit_receipt_idempotent("probe", &first_receipt.receipt_id)
            .unwrap();
        assert_eq!(
            delayed_retry.status,
            PairedHeadCommitStatusV1::AlreadyCommitted
        );
        assert_eq!(delayed_retry.committed_head.revision, 1);
        assert_eq!(delayed_retry.current_head.revision, 2);
        assert_eq!(
            store.load_head("probe").unwrap(),
            delayed_retry.current_head
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn loading_a_head_revalidates_every_receipt_link() {
        let directory = temporary_directory("head-chain-validation");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let source = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (_, receipt) = fixture_successor(&store, &source, blob);
        store.create_head("probe", &source.root_id).unwrap();
        store
            .commit_receipt_idempotent("probe", &receipt.receipt_id)
            .unwrap();

        let path = head_revision_path(&store.head_dir("probe"), 1);
        let mut revision: PairedHeadV1 = read_json(&path).unwrap();
        revision.committed_receipt_id = None;
        fs::write(&path, serde_json::to_vec_pretty(&revision).unwrap()).unwrap();
        assert!(store
            .load_head("probe")
            .unwrap_err()
            .contains("omitted its receipt"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn official_selector_resolves_live_head_and_switches_with_cas() {
        let directory = temporary_directory("head-selector");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let source = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (successor, receipt) = fixture_successor(&store, &source, blob);
        store.create_head("run_a", &source.root_id).unwrap();
        store.create_head("run_b", &source.root_id).unwrap();
        fixture_recovery_certificate(
            &store,
            &source,
            store
                .put_bytes("application/json", br#"{"certificate":true}"#)
                .unwrap(),
        );

        let selected = store.select_head("official", None, "run_a").unwrap();
        assert_eq!(selected.status, PairedHeadSelectorStatusV1::Selected);
        assert_eq!(selected.selector.revision, 0);
        let commit = store
            .commit_selected_receipt_idempotent("official", &receipt.receipt_id)
            .unwrap();
        assert_eq!(commit.selector.head_name, "run_a");
        assert_eq!(commit.commit.status, PairedHeadCommitStatusV1::Committed);
        assert_eq!(commit.commit.current_head.root_id, successor.root_id);
        let resolved = store.resolve_head_selector("official").unwrap();
        assert_eq!(resolved.selector.revision, 0);
        assert_eq!(resolved.head.revision, 1);

        let repeated = store.select_head("official", None, "run_a").unwrap();
        assert_eq!(repeated.status, PairedHeadSelectorStatusV1::AlreadySelected);
        assert!(store
            .select_head("official", Some(9), "run_b")
            .unwrap_err()
            .contains("advanced concurrently"));
        assert_eq!(
            store
                .resolve_head_selector("official")
                .unwrap()
                .head
                .head_name,
            "run_a"
        );

        let switched = store.select_head("official", Some(0), "run_b").unwrap();
        assert_eq!(switched.status, PairedHeadSelectorStatusV1::Selected);
        assert_eq!(switched.selector.revision, 1);
        let switched_resolution = store.resolve_head_selector("official").unwrap();
        assert_eq!(switched_resolution.selector.head_name, "run_b");
        assert_eq!(switched_resolution.head.revision, 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn direct_head_commit_cannot_bypass_a_lost_official_certificate() {
        let directory = temporary_directory("official-certificate-gate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let source = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (_, receipt) = fixture_successor(&store, &source, blob.clone());
        let certificate = fixture_recovery_certificate(&store, &source, blob);
        store.create_head("run", &source.root_id).unwrap();
        store
            .select_head(OFFICIAL_HEAD_SELECTOR_V1, None, "run")
            .unwrap();
        fs::remove_file(
            store.recovery_certificate_path(&source.root_id, &certificate.recovery_certificate_id),
        )
        .unwrap();

        let error = store
            .commit_receipt("run", 0, &receipt.receipt_id)
            .unwrap_err();

        assert!(error.contains("official head requires"));
        assert_eq!(store.load_head("run").unwrap().revision, 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_plan_walks_stable_commitments_to_the_native_root() {
        let directory = temporary_directory("recovery-plan");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let native = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (first, _) = fixture_successor(&store, &native, blob.clone());
        let (second, _) = fixture_successor(&store, &first, blob.clone());
        fixture_recovery_certificate(&store, &native, blob);

        let plan = store.recovery_plan(&second.root_id).unwrap();
        assert_eq!(plan.native_root_id, native.root_id);
        assert_eq!(plan.target_root_id, second.root_id);
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].source_root_id, native.root_id);
        assert_eq!(plan.steps[0].target_root_id, first.root_id);
        assert_eq!(plan.steps[1].source_root_id, first.root_id);
        assert_eq!(plan.steps[1].target_root_id, second.root_id);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exact_state_root_without_a_certificate_is_not_cross_jvm_recoverable() {
        let directory = temporary_directory("exact-root-without-certificate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let root = store.create_root(fixture_root(&store, blob)).unwrap();

        let error = store.recovery_plan(&root.root_id).unwrap_err();
        assert!(error.contains("no fresh-JVM recovery certificate"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn activated_exact_root_without_a_certificate_cannot_become_official() {
        let directory = temporary_directory("uncertified-official-root");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let root = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        fixture_activation(&store, &root, "java-epoch-current", blob);
        let projection: serde_json::Value = store
            .load_exact_projection(&root.root_id, ProjectionDomainV1::RunScalars)
            .unwrap();
        assert_eq!(projection, serde_json::json!({}));
        store.create_head("local", &root.root_id).unwrap();

        let error = store
            .select_head(OFFICIAL_HEAD_SELECTOR_V1, None, "local")
            .unwrap_err();

        assert!(error.contains("official head requires"));
        assert!(error.contains("no fresh-JVM recovery certificate"));
        assert!(
            latest_selection_revision(&store.head_selector_dir(OFFICIAL_HEAD_SELECTOR_V1))
                .unwrap()
                .is_none()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_plan_prefers_the_nearest_recovery_certificate() {
        let directory = temporary_directory("nearest-recovery-certificate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let native = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (room_entry, _) = fixture_successor(&store, &native, blob.clone());
        let (target, _) = fixture_successor(&store, &room_entry, blob.clone());
        let certificate = fixture_recovery_certificate(&store, &room_entry, blob);

        let plan = store.recovery_plan(&target.root_id).unwrap();

        assert_eq!(plan.native_root_id, room_entry.root_id);
        assert_eq!(
            plan.recovery_certificate_id,
            certificate.recovery_certificate_id
        );
        assert_eq!(plan.target_root_id, target.root_id);
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].source_root_id, room_entry.root_id);
        assert_eq!(plan.steps[0].target_root_id, target.root_id);
        assert_eq!(
            store.load_root(&room_entry.root_id).unwrap().root,
            room_entry.root
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_certificate_can_recover_its_exact_target_without_replay() {
        let directory = temporary_directory("exact-recovery-certificate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let native = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (target, _) = fixture_successor(&store, &native, blob.clone());
        let certificate = fixture_recovery_certificate(&store, &target, blob);

        let plan = store.recovery_plan(&target.root_id).unwrap();

        assert_eq!(plan.native_root_id, target.root_id);
        assert_eq!(
            plan.recovery_certificate_id,
            certificate.recovery_certificate_id
        );
        assert!(plan.steps.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn external_recovery_certificate_does_not_truncate_logical_ancestry() {
        let directory = temporary_directory("certificate-independent-ancestry");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let native = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (room_entry, _) = fixture_successor(&store, &native, blob.clone());
        let (target, _) = fixture_successor(&store, &room_entry, blob.clone());
        fixture_recovery_certificate(&store, &room_entry, blob);

        assert_eq!(
            store.logical_ancestry(&target.root_id).unwrap(),
            vec![native.root_id, room_entry.root_id, target.root_id]
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_certificate_rejects_a_same_epoch_round_trip() {
        let directory = temporary_directory("same-epoch-recovery-certificate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let root = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let first = fixture_activation(&store, &root, "java-epoch-a", blob.clone());
        let restored_frame = store
            .put_bytes("application/json", br#"{"restored":true}"#)
            .unwrap();
        let second = fixture_activation(&store, &root, "java-epoch-a", restored_frame);
        let certificate = RecoveryCertificateBodyV2 {
            target_root_id: root.root_id,
            versions: root.root.versions.clone(),
            java_recovery: fixture_anchor_recovery(&store),
            captured_activation_id: first.activation_id,
            restored_activation_id: second.activation_id,
            restored_components: root.root.preflight.components.clone(),
        };

        assert!(store
            .create_recovery_certificate(certificate)
            .unwrap_err()
            .contains("fresh Java runtime epoch"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn native_storage_payload_rejects_legacy_checkpoint_files() {
        let directory = temporary_directory("native-storage-payload-shape");
        let store = StateRootStoreV2::new(&directory);
        let payload = directory.join("invalid-native-storage");
        for relative in NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1 {
            fs::create_dir_all(payload.join(relative)).unwrap();
        }
        fs::write(payload.join("preferences/prefs"), b"prefs").unwrap();
        fs::write(payload.join("saves/IRONCLAD.autosave"), b"save").unwrap();
        fs::write(payload.join("checkpoint.json"), b"legacy").unwrap();
        let bundle = store.put_directory(&payload).unwrap();

        let error = store
            .native_storage_recovery_payload(&bundle, "all_unlocked")
            .unwrap_err();

        assert!(error.contains("outside its storage directories"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn native_storage_certificate_binds_the_autosave_blob() {
        let directory = temporary_directory("native-storage-autosave-claim");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let root = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let captured = fixture_activation(&store, &root, "java-epoch-a", blob.clone());
        let restored = fixture_activation(&store, &root, "java-epoch-b", blob);
        let mut recovery = fixture_anchor_recovery(&store);
        recovery
            .source_identity
            .iter_mut()
            .find(|claim| claim.kind == "native_autosave_sha256")
            .unwrap()
            .value = "f".repeat(64);

        let error = store
            .create_recovery_certificate(RecoveryCertificateBodyV2 {
                target_root_id: root.root_id,
                versions: root.root.versions.clone(),
                java_recovery: recovery,
                captured_activation_id: captured.activation_id,
                restored_activation_id: restored.activation_id,
                restored_components: root.root.preflight.components,
            })
            .unwrap_err();

        assert!(error.contains("autosave disagrees"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_certificate_candidate_rejects_an_incomplete_target_before_capture() {
        let directory = temporary_directory("incomplete-recovery-certificate-candidate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut incomplete = fixture_root(&store, blob.clone());
        incomplete.preflight.uncovered.push(CoverageGapV1 {
            gap_id: "fixture-gap".to_owned(),
            domain: ProjectionDomainV1::Room,
            scope: CoverageGapScopeV1::StateProjection,
            kind: "fixture_missing_room".to_owned(),
            detail: "fixture intentionally omits one domain".to_owned(),
        });
        let incomplete = store.create_root(incomplete).unwrap();
        let captured = fixture_activation(&store, &incomplete, "java-epoch-a", blob);

        let error = store
            .prepare_recovery_certificate_candidate(&incomplete.root_id, &captured.activation_id)
            .unwrap_err();

        assert!(error.contains("state-projection gap"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_certificate_candidate_allows_an_action_capability_gap() {
        let directory = temporary_directory("action-gap-recovery-certificate-candidate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut target = fixture_root(&store, blob.clone());
        target.preflight.uncovered.push(CoverageGapV1 {
            gap_id: "fixture-action-gap".to_owned(),
            domain: ProjectionDomainV1::ActionSpace,
            scope: CoverageGapScopeV1::ActionCapability,
            kind: "capability_not_verified".to_owned(),
            detail: "fixture future action is not verified".to_owned(),
        });
        target.capabilities.push(CapabilityProofV1 {
            capability: CapabilityKindV1::ChooseMapNode,
            required_domains: vec![ProjectionDomainV1::RunScalars],
            status: CapabilityStatusV1::Unknown {
                gap_id: "fixture-action-gap".to_owned(),
            },
        });
        let target = store.create_root(target).unwrap();
        let captured = fixture_activation(&store, &target, "java-epoch-a", blob.clone());

        let candidate = store
            .prepare_recovery_certificate_candidate(&target.root_id, &captured.activation_id)
            .unwrap();

        assert_eq!(candidate.target_root_id, target.root_id);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_certificate_candidate_rejects_a_legacy_unscoped_gap() {
        let directory = temporary_directory("legacy-gap-recovery-certificate-candidate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut target = fixture_root(&store, blob.clone());
        target.preflight.uncovered.push(CoverageGapV1 {
            gap_id: "fixture-legacy-gap".to_owned(),
            domain: ProjectionDomainV1::ActionSpace,
            scope: CoverageGapScopeV1::LegacyUnspecified,
            kind: "capability_not_verified".to_owned(),
            detail: "fixture predates typed gap scope".to_owned(),
        });
        let target = store.create_root(target).unwrap();
        let captured = fixture_activation(&store, &target, "java-epoch-a", blob);

        let error = store
            .prepare_recovery_certificate_candidate(&target.root_id, &captured.activation_id)
            .unwrap_err();

        assert!(error.contains("state-projection gap"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_certificate_candidate_rejects_a_non_natural_target() {
        let directory = temporary_directory("non-natural-recovery-certificate-candidate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut target = fixture_root(&store, blob.clone());
        target.world_provenance = WorldProvenanceV1::ValidatedConstructed;
        let target = store.create_root(target).unwrap();
        let captured = fixture_activation(&store, &target, "java-epoch-a", blob);

        let error = store
            .prepare_recovery_certificate_candidate(&target.root_id, &captured.activation_id)
            .unwrap_err();

        assert!(error.contains("natural target root"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn state_root_rejects_an_invalid_rust_snapshot() {
        let directory = temporary_directory("invalid-rust-state-snapshot");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut target = fixture_root(&store, blob);
        target.rust_snapshot.entrypoint.clear();

        let error = store.create_root(target).unwrap_err();

        assert!(error.contains("Rust state snapshot entrypoint"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_certificate_candidate_rejects_another_roots_activation() {
        let directory = temporary_directory("wrong-activation-recovery-certificate-candidate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let target = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let mut other_body = fixture_root(&store, blob.clone());
        other_body.versions[0].identity = "v2".to_owned();
        let other = store.create_root(other_body).unwrap();
        let captured = fixture_activation(&store, &other, "java-epoch-a", blob);

        let error = store
            .prepare_recovery_certificate_candidate(&target.root_id, &captured.activation_id)
            .unwrap_err();

        assert!(error.contains("belongs to another root"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_certificate_candidate_reads_profile_from_the_state_root() {
        let directory = temporary_directory("recovery-candidate-state-profile");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let native = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (target, _) = fixture_successor(&store, &native, blob.clone());
        let captured = fixture_activation(&store, &target, "java-epoch-a", blob);

        let candidate = store
            .prepare_recovery_certificate_candidate(&target.root_id, &captured.activation_id)
            .unwrap();

        assert_eq!(candidate.content_profile, "all_unlocked");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retained_native_candidate_is_durable_but_invisible_to_recovery_planning() {
        let directory = temporary_directory("retained-native-recovery-candidate");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let native = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (target, _) = fixture_successor(&store, &native, blob.clone());
        let captured = fixture_activation(&store, &target, "java-epoch-a", blob.clone());
        let body = fixture_native_recovery_candidate(&store, &target, &captured);

        let candidate = store.create_native_recovery_candidate(body).unwrap();
        assert_eq!(
            store
                .load_native_recovery_candidate(
                    &target.root_id,
                    &candidate.native_recovery_candidate_id,
                )
                .unwrap(),
            candidate
        );
        assert_eq!(
            store
                .native_recovery_candidate_ids(&target.root_id)
                .unwrap(),
            vec![candidate.native_recovery_candidate_id.clone()]
        );

        let error = store.recovery_plan(&target.root_id).unwrap_err();
        assert!(error.contains("no fresh-JVM recovery certificate"));
        store
            .create_head("candidate-only", &target.root_id)
            .unwrap();
        assert!(store
            .select_head(OFFICIAL_HEAD_SELECTOR_V1, None, "candidate-only")
            .unwrap_err()
            .contains("no fresh-JVM recovery certificate"));

        fixture_recovery_certificate(&store, &target, blob);
        assert_eq!(
            store
                .select_head(OFFICIAL_HEAD_SELECTOR_V1, None, "candidate-only")
                .unwrap()
                .status,
            PairedHeadSelectorStatusV1::Selected
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retained_native_candidate_rechecks_target_and_autosave_identity() {
        let directory = temporary_directory("retained-native-candidate-drift");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let target = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let captured = fixture_activation(&store, &target, "java-epoch-a", blob);
        let mut candidate = fixture_native_recovery_candidate(&store, &target, &captured);
        candidate.native_autosave_sha256 = "f".repeat(64);

        let error = store
            .create_native_recovery_candidate(candidate)
            .unwrap_err();

        assert!(error.contains("autosave disagrees"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retained_native_candidate_allows_only_action_capability_gaps() {
        let directory = temporary_directory("retained-native-candidate-action-gap");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut target = fixture_root(&store, blob.clone());
        target.preflight.uncovered.push(CoverageGapV1 {
            gap_id: "fixture-action-gap".to_owned(),
            domain: ProjectionDomainV1::ActionSpace,
            scope: CoverageGapScopeV1::ActionCapability,
            kind: "capability_not_verified".to_owned(),
            detail: "fixture future action is not verified".to_owned(),
        });
        let target = store.create_root(target).unwrap();
        let captured = fixture_activation(&store, &target, "java-epoch-a", blob);
        let candidate = fixture_native_recovery_candidate(&store, &target, &captured);

        assert!(store.create_native_recovery_candidate(candidate).is_ok());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn publishing_a_certificate_does_not_change_the_state_root_identity() {
        let directory = temporary_directory("certificate-independent-root-identity");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let root = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let before = store.load_root(&root.root_id).unwrap();

        fixture_recovery_certificate(&store, &root, blob);

        assert_eq!(store.load_root(&root.root_id).unwrap(), before);
        assert!(store.recovery_plan(&root.root_id).is_ok());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn operation_verification_cache_reuses_one_fully_verified_recovery_graph() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StateRootStoreV2>();

        let directory = temporary_directory("operation-verification-cache");
        let writer = StateRootStoreV2::new(&directory);
        let blob = writer.put_bytes("application/json", b"{}").unwrap();
        let native = writer
            .create_root(fixture_root(&writer, blob.clone()))
            .unwrap();
        let (first, _) = fixture_successor(&writer, &native, blob.clone());
        let (second, _) = fixture_successor(&writer, &first, blob.clone());
        fixture_recovery_certificate(&writer, &native, blob);

        let reader = StateRootStoreV2::new(&directory).with_operation_verification_cache();
        let first_plan = reader.recovery_plan(&second.root_id).unwrap();
        let after_first = reader.operation_verification_cache_counts().unwrap();
        let second_plan = reader.recovery_plan(&second.root_id).unwrap();
        let after_second = reader.operation_verification_cache_counts().unwrap();

        assert_eq!(first_plan, second_plan);
        assert_eq!(after_first, after_second);
        assert_eq!(after_first.2, 3);
        assert_eq!(after_first.4, 1);
        assert_eq!(after_first.5, 2);
        assert_eq!(
            StateRootStoreV2::new(&directory).operation_verification_cache_counts(),
            None
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn successor_cannot_silently_cross_a_version_boundary() {
        let directory = temporary_directory("successor-version-boundary");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let native = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let (successor, _) = fixture_successor(&store, &native, blob);
        let mut incompatible = successor.root;
        incompatible.versions[0].identity = "v2".to_owned();

        let error = store.create_root(incompatible).unwrap_err();
        assert!(error.contains("versions must equal its parent versions"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn stable_root_identity_excludes_java_runtime_card_identity() {
        let directory = temporary_directory("activation-identity");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut root_body = fixture_root(&store, blob.clone());
        let binding = fixture_master_deck_binding(blob.clone());
        root_body.card_bindings = vec![binding.clone()];
        let root = store.create_root(root_body.clone()).unwrap();
        assert_eq!(root.root_id, store.create_root(root_body).unwrap().root_id);

        let first = store
            .create_activation(StateRootActivationBodyV2 {
                root_id: root.root_id.clone(),
                java_runtime_epoch: "jvm:first".to_owned(),
                java_frame: blob.clone(),
                action_space_id: root.root.preflight.action_space_id.clone(),
                card_leases: vec![JavaCardLeaseV1 {
                    binding_id: binding.binding_id.clone(),
                    java_card_uuid: "java-uuid-first".to_owned(),
                }],
            })
            .unwrap();
        let second = store
            .create_activation(StateRootActivationBodyV2 {
                root_id: root.root_id.clone(),
                java_runtime_epoch: "jvm:second".to_owned(),
                java_frame: blob,
                action_space_id: root.root.preflight.action_space_id.clone(),
                card_leases: vec![JavaCardLeaseV1 {
                    binding_id: binding.binding_id,
                    java_card_uuid: "java-uuid-second".to_owned(),
                }],
            })
            .unwrap();

        assert_eq!(first.activation.root_id, second.activation.root_id);
        assert_ne!(first.activation_id, second.activation_id);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn activation_must_lease_every_root_binding_exactly_once() {
        let directory = temporary_directory("activation-coverage");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let mut root_body = fixture_root(&store, blob.clone());
        root_body.card_bindings = vec![fixture_master_deck_binding(blob.clone())];
        let root = store.create_root(root_body).unwrap();

        let error = store
            .create_activation(StateRootActivationBodyV2 {
                root_id: root.root_id,
                java_runtime_epoch: "jvm:missing-lease".to_owned(),
                java_frame: blob,
                action_space_id: root.root.preflight.action_space_id,
                card_leases: Vec::new(),
            })
            .expect_err("activation without the root binding unexpectedly succeeded");
        assert!(error.contains("cover exactly"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn transition_commitment_excludes_runtime_local_java_evidence() {
        let directory = temporary_directory("transition-commitment");
        let store = StateRootStoreV2::new(&directory);
        let comparison_blob = store.put_bytes("application/json", b"{}").unwrap();
        let java_first = store.put_bytes("application/json", b"{\"jvm\":1}").unwrap();
        let java_second = store.put_bytes("application/json", b"{\"jvm\":2}").unwrap();
        let rust_receipt = store
            .put_bytes("application/json", b"{\"rust\":1}")
            .unwrap();
        let source = store
            .create_root(fixture_root(&store, comparison_blob.clone()))
            .unwrap();
        let action = action_handle(
            &source.root_id,
            &source.root.preflight.action_space_id,
            PairedActionV1::EndTurn,
        )
        .unwrap();
        let base = TransitionReceiptBodyV1 {
            transition_commitment_id: String::new(),
            source_root_id: source.root_id,
            successor_root_id:
                "state-root-v2-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
            action,
            java_action_receipt: java_first,
            rust_transition_receipt: rust_receipt,
            successor_comparisons: source.root.preflight.components,
            timings: TransitionTimingV1 {
                rust_execution_ms: 1,
                java_execution_ms: 2,
                comparison_ms: 3,
            },
        };
        let mut replayed = base.clone();
        replayed.successor_root_id =
            "state-root-v2-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_owned();
        replayed.java_action_receipt = java_second;
        replayed.timings = TransitionTimingV1 {
            rust_execution_ms: 10,
            java_execution_ms: 20,
            comparison_ms: 30,
        };

        assert_eq!(
            transition_commitment_id(&base).unwrap(),
            transition_commitment_id(&replayed).unwrap()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    fn fixture_root(store: &StateRootStoreV2, blob: BlobRefV1) -> StateRootBodyV2 {
        let payload = store.root.join("fixture-payload");
        fs::create_dir_all(&payload).unwrap();
        let bundle = store.put_directory(&payload).unwrap();
        StateRootBodyV2 {
            provenance: RootProvenanceV1::Probe,
            world_provenance: WorldProvenanceV1::Natural,
            parent: None,
            decision_profile: DecisionProfileV1::CombatPlayerDecision,
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

    fn fixture_master_deck_binding(blob: BlobRefV1) -> CrossRuntimeCardBindingV1 {
        let mut binding = CrossRuntimeCardBindingV1 {
            binding_id: String::new(),
            rust_lineage: RustCardLineageV1::MasterDeck {
                acquisition_ordinal: 0,
            },
            card_id: "Strike_R".to_owned(),
            upgrades: 0,
            evidence: CardBindingEvidenceV1::MasterDeckObservation {
                java_master_deck_ordinal: 0,
                rust_master_deck_ordinal: 0,
                java_projection: blob.clone(),
                rust_projection: blob,
            },
        };
        binding.binding_id = card_binding_id(&binding).unwrap();
        binding
    }

    #[test]
    fn state_root_v2_requires_explicit_profile_and_world_provenance() {
        let directory = temporary_directory("explicit-state-root-axes");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let root = fixture_root(&store, blob);
        let mut value = serde_json::to_value(root).unwrap();

        value.as_object_mut().unwrap().remove("decision_profile");
        assert!(serde_json::from_value::<StateRootBodyV2>(value.clone()).is_err());
        value["decision_profile"] = serde_json::json!("combat_player_decision");
        value.as_object_mut().unwrap().remove("world_provenance");
        assert!(serde_json::from_value::<StateRootBodyV2>(value).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn successor_cannot_change_world_provenance() {
        let directory = temporary_directory("successor-world-provenance");
        let store = StateRootStoreV2::new(&directory);
        let blob = store.put_bytes("application/json", b"{}").unwrap();
        let source = store
            .create_root(fixture_root(&store, blob.clone()))
            .unwrap();
        let action = action_handle(
            &source.root_id,
            &source.root.preflight.action_space_id,
            PairedActionV1::EndTurn,
        )
        .unwrap();
        let receipt = TransitionReceiptBodyV1 {
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
        let mut successor = source.root.clone();
        successor.parent = Some(ParentStateRootV2 {
            root_id: source.root_id,
            transition_commitment_id: commitment.transition_commitment_id,
        });
        successor.world_provenance = WorldProvenanceV1::Adversarial;

        assert!(store
            .create_root(successor)
            .unwrap_err()
            .contains("successor world provenance must equal its parent"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exact_projection_is_loaded_once_as_shared_typed_state() {
        let directory = temporary_directory("exact-projection");
        let store = StateRootStoreV2::new(&directory).with_operation_verification_cache();
        store.initialize().unwrap();
        let blob = store
            .put_bytes("application/json", br#"{"act":1,"floor":3}"#)
            .unwrap();
        let root = store.create_root(fixture_root(&store, blob)).unwrap();

        let projection: serde_json::Value = store
            .load_exact_projection(&root.root_id, ProjectionDomainV1::RunScalars)
            .unwrap();
        assert_eq!(projection["act"], 1);
        assert_eq!(projection["floor"], 3);
        assert!(store
            .load_exact_projection::<serde_json::Value>(&root.root_id, ProjectionDomainV1::Map,)
            .unwrap_err()
            .contains("omitted required Map"));
        fs::remove_dir_all(directory).unwrap();
    }

    fn fixture_successor(
        store: &StateRootStoreV2,
        source: &StateRootArtifactV2,
        blob: BlobRefV1,
    ) -> (StateRootArtifactV2, TransitionReceiptArtifactV1) {
        let action = action_handle(
            &source.root_id,
            &source.root.preflight.action_space_id,
            PairedActionV1::EndTurn,
        )
        .unwrap();
        let mut receipt_body = TransitionReceiptBodyV1 {
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
            .create_transition_commitment(transition_commitment_body(&receipt_body))
            .unwrap();
        receipt_body.transition_commitment_id = commitment.transition_commitment_id;
        let mut successor_body = source.root.clone();
        successor_body.parent = Some(ParentStateRootV2 {
            root_id: source.root_id.clone(),
            transition_commitment_id: receipt_body.transition_commitment_id.clone(),
        });
        let successor = store.create_root(successor_body).unwrap();
        receipt_body.successor_root_id = successor.root_id.clone();
        let receipt = store.create_receipt(receipt_body).unwrap();
        (successor, receipt)
    }

    fn fixture_activation(
        store: &StateRootStoreV2,
        root: &StateRootArtifactV2,
        java_runtime_epoch: &str,
        frame: BlobRefV1,
    ) -> StateRootActivationArtifactV2 {
        store
            .create_activation(StateRootActivationBodyV2 {
                root_id: root.root_id.clone(),
                java_runtime_epoch: java_runtime_epoch.to_owned(),
                java_frame: frame,
                action_space_id: root.root.preflight.action_space_id.clone(),
                card_leases: Vec::new(),
            })
            .unwrap()
    }

    fn fixture_anchor_recovery(store: &StateRootStoreV2) -> RecoveryPayloadV1 {
        let payload = store.root.join("fixture-native-storage");
        for relative in NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1 {
            fs::create_dir_all(payload.join(relative)).unwrap();
        }
        fs::write(payload.join("preferences/prefs"), b"prefs").unwrap();
        fs::write(payload.join("saves/IRONCLAD.autosave"), b"save").unwrap();
        let bundle = store.put_directory(&payload).unwrap();
        store
            .native_storage_recovery_payload(&bundle, "all_unlocked")
            .unwrap()
    }

    fn fixture_native_recovery_candidate(
        store: &StateRootStoreV2,
        root: &StateRootArtifactV2,
        captured: &StateRootActivationArtifactV2,
    ) -> NativeRecoveryCandidateBodyV2 {
        let recovery = fixture_anchor_recovery(store);
        let native_autosave_sha256 = recovery
            .source_identity
            .iter()
            .find(|claim| claim.kind == "native_autosave_sha256")
            .unwrap()
            .value
            .clone();
        NativeRecoveryCandidateBodyV2 {
            target_root_id: root.root_id.clone(),
            captured_activation_id: captured.activation_id.clone(),
            versions: root.root.versions.clone(),
            content_profile: "all_unlocked".to_owned(),
            recovery_ref: format!("workbench-native-recovery-capsule-v1-{}", "a".repeat(64)),
            native_storage_bundle: recovery.bundle,
            native_autosave_sha256,
        }
    }

    fn fixture_recovery_certificate(
        store: &StateRootStoreV2,
        root: &StateRootArtifactV2,
        frame: BlobRefV1,
    ) -> RecoveryCertificateArtifactV2 {
        let captured = fixture_activation(store, root, "java-epoch-a", frame.clone());
        let restored = fixture_activation(store, root, "java-epoch-b", frame);
        let candidate = store
            .prepare_recovery_certificate_candidate(&root.root_id, &captured.activation_id)
            .unwrap();
        assert_eq!(candidate.target_root_id, root.root_id);
        assert_eq!(candidate.captured_activation_id, captured.activation_id);
        assert_eq!(candidate.versions, root.root.versions);
        assert_eq!(
            candidate.expected_components,
            root.root.preflight.components
        );
        store
            .create_recovery_certificate(RecoveryCertificateBodyV2 {
                target_root_id: root.root_id.clone(),
                versions: root.root.versions.clone(),
                java_recovery: fixture_anchor_recovery(store),
                captured_activation_id: captured.activation_id,
                restored_activation_id: restored.activation_id,
                restored_components: root.root.preflight.components.clone(),
            })
            .unwrap()
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "sts-paired-root-{label}-{}-{}",
            std::process::id(),
            unix_ms().unwrap()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
