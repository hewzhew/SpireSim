use serde::{Deserialize, Serialize};
use sts_oracle_learning::agent::task::{
    TaskBudgetV1, TaskDecisionSiteV1, TaskEpisodeOutcomeV1, TaskPrivilegedStrategicSupportV1,
    TaskPrivilegedSupportProvenanceV1, TaskPublicCandidateV1, TaskPublicChoiceV1,
    TaskResearchPlanOutcomeV1, TaskRunArchiveAttemptV1, TaskRunArchiveNodeV1,
    TaskRunSolutionArchiveV1,
};
use sts_oracle_learning::eval::run_control::{
    CombatResearchPlanOptionsV1, TaskRunBranchDecisionInventoryV1,
};

pub const RUN_PLAY_HEAD_SCHEMA_V1: &str = "sts-run-play-head-v1";
pub const RUN_PLAY_STATE_SCHEMA_V1: &str = "sts-run-play-state-v1";
pub const RUN_PLAY_DELTA_SCHEMA_V1: &str = "sts-run-play-delta-v1";
pub const RUN_PLAY_EVIDENCE_IDENTITY_SCHEMA_V1: &str = "sts-run-play-evidence-identity-v1";
pub const RUN_PLAY_SIMULATOR_SEMANTICS_ID_V1: &str = "sts-simulator-run-semantics-20260902-v14";
pub const RUN_PLAY_RNG_SEMANTICS_ID_V1: &str = "sts-simulator-run-rng-neow-card-parity-20260829-v1";
pub const RUN_PLAY_PUBLIC_OBSERVATION_ID_V1: &str = "sts-run-play-public-observation-20260906-v4";

/// Embedded at Cargo build time. A conservative source/configuration identity,
/// not a binary attestation, semantic equivalence claim, or parity certificate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayBuildSourceV1 {
    pub schema: String,
    pub input_scope: String,
    pub source_sha256: String,
    pub configuration: std::collections::BTreeMap<String, String>,
}

impl RunPlayBuildSourceV1 {
    fn current_v1() -> Self {
        static CURRENT: std::sync::OnceLock<RunPlayBuildSourceV1> = std::sync::OnceLock::new();
        CURRENT
            .get_or_init(|| {
                serde_json::from_str(env!("STS_RUN_PLAY_BUILD_SOURCE"))
                    .expect("build script emits typed source identity")
            })
            .clone()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayEvidenceIdentityV1 {
    pub schema: String,
    pub simulator_semantics_id: String,
    pub rng_semantics_id: String,
    pub public_observation_id: String,
    pub run_play_head_schema: String,
    /// Missing in older evidence. Deserialization preserves that absence;
    /// observing old evidence cannot manufacture its original producer.
    /// A verified reroot is a distinct, current-build artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_source: Option<RunPlayBuildSourceV1>,
}

impl RunPlayEvidenceIdentityV1 {
    pub fn current_v1() -> Self {
        Self {
            schema: RUN_PLAY_EVIDENCE_IDENTITY_SCHEMA_V1.to_string(),
            simulator_semantics_id: RUN_PLAY_SIMULATOR_SEMANTICS_ID_V1.to_string(),
            rng_semantics_id: RUN_PLAY_RNG_SEMANTICS_ID_V1.to_string(),
            public_observation_id: RUN_PLAY_PUBLIC_OBSERVATION_ID_V1.to_string(),
            run_play_head_schema: RUN_PLAY_HEAD_SCHEMA_V1.to_string(),
            build_source: Some(RunPlayBuildSourceV1::current_v1()),
        }
    }

    pub fn compatibility_label_v1(identity: Option<&Self>) -> &'static str {
        match identity {
            None => "legacy_unscoped",
            Some(identity) if identity.build_source.is_none() => "source_unrecorded",
            Some(identity) if *identity == Self::current_v1() => "current",
            Some(identity)
                if identity.schema != RUN_PLAY_EVIDENCE_IDENTITY_SCHEMA_V1
                    || identity.simulator_semantics_id != RUN_PLAY_SIMULATOR_SEMANTICS_ID_V1
                    || identity.rng_semantics_id != RUN_PLAY_RNG_SEMANTICS_ID_V1
                    || identity.public_observation_id != RUN_PLAY_PUBLIC_OBSERVATION_ID_V1
                    || identity.run_play_head_schema != RUN_PLAY_HEAD_SCHEMA_V1 =>
            {
                "contract_changed_unverified"
            }
            Some(_) => "source_changed_unverified",
        }
    }

    /// Shared by controller restoration and detached current-decision export.
    /// Stored evidence can still be read without reconstructing it in a new runtime.
    pub fn require_current_v1(identity: Option<&Self>) -> Result<(), String> {
        if identity.is_some_and(|stored| *stored == Self::current_v1()) {
            Ok(())
        } else {
            let status = Self::compatibility_label_v1(identity);
            Err(format!(
                "run-play source is {status}; in-place continuation requires the original build identity. This label is not a semantic incompatibility finding. Retained status/history remain readable; a verified exact-state reroot creates a new current-build session without certifying the old trajectory"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RunPlayEvidenceIdentityV1;

    #[test]
    fn unchanged_contracts_do_not_accept_a_different_source_or_configuration() {
        let current = RunPlayEvidenceIdentityV1::current_v1();
        assert!(RunPlayEvidenceIdentityV1::require_current_v1(Some(&current)).is_ok());
        let mut old = current.clone();
        old.build_source.as_mut().unwrap().source_sha256 = "old-source".into();
        assert_eq!(
            RunPlayEvidenceIdentityV1::compatibility_label_v1(Some(&old)),
            "source_changed_unverified"
        );
        assert!(RunPlayEvidenceIdentityV1::require_current_v1(Some(&old)).is_err());
        let mut contract_changed = current.clone();
        contract_changed.simulator_semantics_id = "old-contract".into();
        assert_eq!(
            RunPlayEvidenceIdentityV1::compatibility_label_v1(Some(&contract_changed)),
            "contract_changed_unverified"
        );
        let mut other = current;
        other
            .build_source
            .as_mut()
            .unwrap()
            .configuration
            .insert("profile".into(), "other".into());
        assert!(RunPlayEvidenceIdentityV1::require_current_v1(Some(&other)).is_err());
    }

    #[test]
    fn legacy_source_stays_readable_and_unrecorded() {
        let mut value = serde_json::to_value(RunPlayEvidenceIdentityV1::current_v1()).unwrap();
        value.as_object_mut().unwrap().remove("build_source");
        let source: RunPlayEvidenceIdentityV1 = serde_json::from_value(value.clone()).unwrap();
        assert!(source.build_source.is_none());
        assert_eq!(serde_json::to_value(&source).unwrap(), value);
        assert!(RunPlayEvidenceIdentityV1::require_current_v1(Some(&source)).is_err());
        assert!(RunPlayEvidenceIdentityV1::require_current_v1(None).is_err());
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayStrategicOwnerAdviceV1 {
    pub support: TaskPrivilegedStrategicSupportV1,
    pub semantic_action: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunPlayPositionV1 {
    Decision {
        archive_node_id: String,
    },
    Terminal {
        archive_node_id: String,
        outcome: TaskEpisodeOutcomeV1,
    },
}

impl RunPlayPositionV1 {
    pub fn archive_node_id(&self) -> &str {
        match self {
            Self::Decision { archive_node_id }
            | Self::Terminal {
                archive_node_id, ..
            } => archive_node_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunPlayEventV1 {
    Choice {
        head_revision: u64,
        from_archive_node_id: String,
        to_archive_node_id: Option<String>,
        site: TaskDecisionSiteV1,
        choice: TaskPublicChoiceV1,
        #[serde(default)]
        strategic_owner_advice: Option<RunPlayStrategicOwnerAdviceV1>,
        executed_new_branch: bool,
        attempt_id: Option<String>,
        outcome: Option<TaskEpisodeOutcomeV1>,
    },
    AnchorLine {
        head_revision: u64,
        from_archive_node_id: String,
        to_archive_node_id: String,
        choices: Vec<TaskPublicChoiceV1>,
        state_revision: u64,
        before_nodes: usize,
        after_nodes: usize,
        before_attempts: usize,
        after_attempts: usize,
        before_checkpoints: usize,
        after_checkpoints: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        combat_research_candidate: Option<RunPlayCombatResearchCandidateV1>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        combat_milestone_candidate: Option<RunPlayCombatMilestoneCandidateV1>,
    },
    Back {
        head_revision: u64,
        from_archive_node_id: String,
        to_archive_node_id: String,
        steps: usize,
    },
    Jump {
        head_revision: u64,
        from_archive_node_id: String,
        to_archive_node_id: String,
        selector: String,
    },
    Checkpoint {
        head_revision: u64,
        archive_node_id: String,
        state_revision: u64,
        checkpoint_count: usize,
    },
    CompactActive {
        head_revision: u64,
        archive_node_id: String,
        state_revision: u64,
        before_nodes: usize,
        after_nodes: usize,
        before_checkpoints: usize,
        after_checkpoints: usize,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatResearchCandidateV1 {
    pub supporting_provenances: Vec<TaskPrivilegedSupportProvenanceV1>,
    pub outcome: TaskResearchPlanOutcomeV1,
    pub satisfies_requested_quality: bool,
}

/// Durable receipt for a finite replay-exact combat prefix. It proves only
/// the typed transition named here; unlike a combat-research candidate it
/// carries no terminal outcome or resource-frontier standing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatMilestoneCandidateV1 {
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub transition: sts_combat_strategy::CombatPlanTransitionAnnotationV1,
    pub successor_plan: sts_combat_strategy::CombatPlanProjectionV1,
    pub final_player_hp: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayBoundaryKindV1 {
    Decision,
    CommitReady,
    ExploredScopeExit,
    CommittedNextScope,
    CommittedGameTerminal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayPublicScalarsV1 {
    pub act: Option<u8>,
    pub floor: Option<i32>,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
}

/// Small public combat projection carried by each retained path step for
/// original-game parity. This deliberately excludes draw order, card lineages,
/// potion UUIDs, RNG, queued actions, and simulator runtime bundles.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatParityProjectionV1 {
    pub turn_index: u32,
    pub energy: u8,
    pub player_block: i32,
    pub hand: Vec<RunPlayCombatParityCardV1>,
    pub potions: Vec<Option<String>>,
    pub player_powers: Vec<RunPlayCombatParityPowerV1>,
    pub monsters: Vec<RunPlayCombatParityMonsterV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatParityCardV1 {
    pub card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatParityPowerV1 {
    pub power_id: String,
    pub amount: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayCombatParityMonsterV1 {
    /// Rust's public group position. It is diagnostic only and is never used
    /// as a cross-runtime identity.
    pub source_index: usize,
    pub source_slot: u8,
    pub enemy_id: String,
    /// Occurrence among monsters with this enemy ID in current group order.
    pub same_enemy_ordinal: usize,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub intent: Option<String>,
    pub powers: Vec<RunPlayCombatParityPowerV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayPathStepV1 {
    pub archive_node_id: String,
    pub depth: usize,
    pub site: TaskDecisionSiteV1,
    pub before: RunPlayPublicScalarsV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_before: Option<RunPlayCombatParityProjectionV1>,
    pub choice: TaskPublicChoiceV1,
    pub selected_candidate: TaskPublicCandidateV1,
    /// Stable, executable rendering derived from the source decision and
    /// validated choice. Older persisted heads did not cache this field; a
    /// subsequent exact navigation rebuilds it from the archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_action: Option<String>,
    pub observed_boundary: RunPlayBoundaryKindV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayHeadV1 {
    pub schema: String,
    /// Disposable play state. Its checkpoint is authoritative; the archive is
    /// only a bounded execution workspace, not durable trajectory evidence.
    #[serde(default)]
    pub scratch: bool,
    /// Previous retained scratch decision. This is a bounded undo cursor,
    /// not a proof or a complete action history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch_parent_revision: Option<u64>,
    /// Exact decisions at settled floor boundaries (usually the map page).
    /// Older scratch heads simply have no recorded floor landmarks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scratch_floor_ends: Vec<RunPlayScratchFloorEndV1>,
    /// Explicitly kept alternatives in this disposable run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scratch_marks: Vec<RunPlayScratchMarkV1>,
    /// One automatically retained endpoint for returning after a checked rewind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch_return_revision: Option<u64>,
    /// Immutable evidence compatibility of the root that created this
    /// workspace. Missing means legacy/unscoped, never implicitly current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_identity: Option<RunPlayEvidenceIdentityV1>,
    pub session_id: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub delta_revisions: Vec<u64>,
    pub position: RunPlayPositionV1,
    pub default_budget: TaskBudgetV1,
    pub combat_plan_options: CombatResearchPlanOptionsV1,
    pub current_decision: Option<TaskRunBranchDecisionInventoryV1>,
    pub current_path: Vec<RunPlayPathStepV1>,
    pub events: Vec<RunPlayEventV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayScratchFloorEndV1 {
    pub floor: i32,
    pub head_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayScratchMarkV1 {
    pub name: String,
    pub head_revision: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayStateV1 {
    pub schema: String,
    pub state_revision: u64,
    pub archive: TaskRunSolutionArchiveV1,
    #[serde(with = "serde_bytes")]
    pub checkpoint_payload: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayDeltaV1 {
    pub schema: String,
    pub delta_revision: u64,
    pub base_state_revision: u64,
    pub expected_archive_nodes: usize,
    pub expected_archive_attempts: usize,
    pub appended_nodes: Vec<TaskRunArchiveNodeV1>,
    pub appended_attempts: Vec<TaskRunArchiveAttemptV1>,
}
