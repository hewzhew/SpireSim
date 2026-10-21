use serde::{Deserialize, Serialize};

use crate::state::run::RunState;

pub const ARTIFACT_PRODUCER: &str = "sts_simulator_rust";
pub const ARTIFACT_LINEAGE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactHeaderV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub artifact_kind: String,
    pub producer: String,
    pub lineage_schema_version: u32,
}

impl ArtifactHeaderV1 {
    pub fn new(
        schema_name: impl Into<String>,
        schema_version: u32,
        artifact_kind: impl Into<String>,
    ) -> Self {
        Self {
            schema_name: schema_name.into(),
            schema_version,
            artifact_kind: artifact_kind.into(),
            producer: ARTIFACT_PRODUCER.to_string(),
            lineage_schema_version: ARTIFACT_LINEAGE_SCHEMA_VERSION,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactTrustLevel {
    Scratch,
    Restorable,
    ReplayVerified,
    BlessedBenchmark,
    Retired,
}

impl ArtifactTrustLevel {
    pub fn satisfies_minimum(self, minimum: ArtifactTrustLevel) -> bool {
        if matches!(self, ArtifactTrustLevel::Retired) {
            return false;
        }
        self.rank() >= minimum.rank()
    }

    fn rank(self) -> u8 {
        match self {
            ArtifactTrustLevel::Retired => 0,
            ArtifactTrustLevel::Scratch => 1,
            ArtifactTrustLevel::Restorable => 2,
            ArtifactTrustLevel::ReplayVerified => 3,
            ArtifactTrustLevel::BlessedBenchmark => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSourceKind {
    Unknown,
    ExactCombatPosition,
    ManualRunControl,
    RuntimeProgress,
    IntentReplay,
    FixtureStartSpec,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRunConfigV1 {
    pub seed: Option<u64>,
    pub ascension_level: Option<u8>,
    pub player_class: Option<String>,
    pub act_num: Option<u8>,
    pub floor_num: Option<i32>,
}

impl ArtifactRunConfigV1 {
    pub fn from_run_state(run: &RunState) -> Self {
        Self {
            seed: Some(run.seed),
            ascension_level: Some(run.ascension_level),
            player_class: Some(run.player_class.to_string()),
            act_num: Some(run.act_num),
            floor_num: Some(run.floor_num),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactParentRefV1 {
    pub artifact_kind: String,
    pub artifact_id: String,
    pub relationship: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactProvenanceV1 {
    pub source_kind: ArtifactSourceKind,
    pub producer: String,
    pub capture_method: String,
    pub run_config: Option<ArtifactRunConfigV1>,
    pub parent_artifacts: Vec<ArtifactParentRefV1>,
    pub label_role: String,
    pub trainable_as_action_label: bool,
    pub policy_quality_claim: bool,
}

impl ArtifactProvenanceV1 {
    pub fn exact_combat_position() -> Self {
        Self::new(
            ArtifactSourceKind::ExactCombatPosition,
            "exact_combat_position",
            None,
        )
    }

    pub fn manual_run_control(run: &RunState) -> Self {
        Self::new(
            ArtifactSourceKind::ManualRunControl,
            "run_control_manual_capture",
            Some(ArtifactRunConfigV1::from_run_state(run)),
        )
    }

    pub fn runtime_progress(run: &RunState) -> Self {
        Self::new(
            ArtifactSourceKind::RuntimeProgress,
            "runtime_progress_capture",
            Some(ArtifactRunConfigV1::from_run_state(run)),
        )
    }

    pub fn unknown() -> Self {
        Self::new(ArtifactSourceKind::Unknown, "unknown", None)
    }

    fn new(
        source_kind: ArtifactSourceKind,
        capture_method: impl Into<String>,
        run_config: Option<ArtifactRunConfigV1>,
    ) -> Self {
        Self {
            source_kind,
            producer: ARTIFACT_PRODUCER.to_string(),
            capture_method: capture_method.into(),
            run_config,
            parent_artifacts: Vec::new(),
            label_role: "diagnostic_not_teacher_label".to_string(),
            trainable_as_action_label: false,
            policy_quality_claim: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ArtifactSourceKind;

    #[test]
    fn retired_artifact_source_alias_is_rejected() {
        assert!(serde_json::from_str::<ArtifactSourceKind>(r#""auto_run_control""#).is_err());
        assert_eq!(
            serde_json::from_str::<ArtifactSourceKind>(r#""runtime_progress""#).unwrap(),
            ArtifactSourceKind::RuntimeProgress
        );
    }
}
