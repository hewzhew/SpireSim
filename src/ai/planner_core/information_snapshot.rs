//! Content identity for a player-visible decision snapshot.
//!
//! This is deliberately capture-only. It has no exact checkpoint, combat
//! state hash, live RNG cursor, replay witness, trajectory instance, or claim
//! that a normalized public event prefix exists. A later trajectory owner must
//! promote snapshots into information states; this type grants no search or
//! training authority by itself.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::stable_planner_id;

pub const PUBLIC_INFORMATION_SNAPSHOT_SCHEMA_NAME: &str = "PublicInformationSnapshotV1";
pub const PUBLIC_INFORMATION_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicDecisionDomainV1 {
    Strategic,
    Combat,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicObservationScopeV1 {
    StrategicRunDecision,
    CombatDecisionOnly,
    CombatDecisionWithRunContinuation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicObservationReferenceV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub observation_id: String,
    pub scope: PublicObservationScopeV1,
}

impl PublicObservationReferenceV1 {
    /// Content-addresses a payload already sanitized by its typed projection
    /// owner. This reference cannot itself prove that an arbitrary payload is
    /// public and therefore confers no search/training authority.
    pub fn from_sanitized_payload<T: Serialize>(
        schema_name: impl Into<String>,
        schema_version: u32,
        scope: PublicObservationScopeV1,
        sanitized_payload: &T,
    ) -> Result<Self, String> {
        let schema_name = nonempty("public observation schema name", schema_name.into())?;
        let observation_id = stable_planner_id(
            "public_observation_v1",
            &(
                schema_name.as_str(),
                schema_version,
                scope,
                sanitized_payload,
            ),
        )?;
        Ok(Self {
            schema_name,
            schema_version,
            observation_id,
            scope,
        })
    }

    fn validate(&self) -> Result<(), String> {
        ensure_nonempty("public observation schema name", &self.schema_name)?;
        ensure_nonempty("public observation id", &self.observation_id)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicHistorySnapshotReferenceV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub history_snapshot_id: String,
}

impl PublicHistorySnapshotReferenceV1 {
    /// Captures only history fields already present in the public observation.
    /// It is not a normalized trajectory event prefix.
    pub fn from_sanitized_payload<T: Serialize>(
        schema_name: impl Into<String>,
        schema_version: u32,
        sanitized_payload: &T,
    ) -> Result<Self, String> {
        let schema_name = nonempty("public history schema name", schema_name.into())?;
        let history_snapshot_id = stable_planner_id(
            "public_history_snapshot_v1",
            &(schema_name.as_str(), schema_version, sanitized_payload),
        )?;
        Ok(Self {
            schema_name,
            schema_version,
            history_snapshot_id,
        })
    }

    fn validate(&self) -> Result<(), String> {
        ensure_nonempty("public history schema name", &self.schema_name)?;
        ensure_nonempty("public history snapshot id", &self.history_snapshot_id)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCandidateSurfaceKindV1 {
    /// The complete action surface actually exposed to the deployed learning
    /// policy. It may be narrower than the engine-legal UI surface.
    DeployablePolicy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCandidateSurfaceReferenceV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub candidate_surface_id: String,
    pub kind: PublicCandidateSurfaceKindV1,
    /// Ordered exactly as the deployed model-facing action surface.
    pub ordered_candidate_ids: Vec<String>,
}

impl PublicCandidateSurfaceReferenceV1 {
    pub fn from_candidate_ids(
        schema_name: impl Into<String>,
        schema_version: u32,
        kind: PublicCandidateSurfaceKindV1,
        ordered_candidate_ids: Vec<String>,
    ) -> Result<Self, String> {
        let schema_name = nonempty("public candidate schema name", schema_name.into())?;
        validate_candidate_ids(&ordered_candidate_ids)?;
        let candidate_surface_id = stable_planner_id(
            "public_candidate_surface_v1",
            &(
                schema_name.as_str(),
                schema_version,
                kind,
                ordered_candidate_ids.as_slice(),
            ),
        )?;
        Ok(Self {
            schema_name,
            schema_version,
            candidate_surface_id,
            kind,
            ordered_candidate_ids,
        })
    }

    fn validate(&self) -> Result<(), String> {
        ensure_nonempty("public candidate schema name", &self.schema_name)?;
        ensure_nonempty("public candidate-surface id", &self.candidate_surface_id)?;
        validate_candidate_ids(&self.ordered_candidate_ids)?;
        let expected = stable_planner_id(
            "public_candidate_surface_v1",
            &(
                self.schema_name.as_str(),
                self.schema_version,
                self.kind,
                self.ordered_candidate_ids.as_slice(),
            ),
        )?;
        if self.candidate_surface_id != expected {
            return Err("public candidate-surface id does not match its ordered candidates".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicInformationSnapshotV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub snapshot_id: String,
    pub domain: PublicDecisionDomainV1,
    pub observation: PublicObservationReferenceV1,
    pub history_snapshot: PublicHistorySnapshotReferenceV1,
    pub candidate_surface: PublicCandidateSurfaceReferenceV1,
}

impl PublicInformationSnapshotV1 {
    pub fn new(
        domain: PublicDecisionDomainV1,
        observation: PublicObservationReferenceV1,
        history_snapshot: PublicHistorySnapshotReferenceV1,
        candidate_surface: PublicCandidateSurfaceReferenceV1,
    ) -> Result<Self, String> {
        observation.validate()?;
        history_snapshot.validate()?;
        candidate_surface.validate()?;
        validate_domain_scope(domain, observation.scope)?;
        let mut snapshot = Self {
            schema_name: PUBLIC_INFORMATION_SNAPSHOT_SCHEMA_NAME.to_string(),
            schema_version: PUBLIC_INFORMATION_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::new(),
            domain,
            observation,
            history_snapshot,
            candidate_surface,
        };
        snapshot.snapshot_id = snapshot.expected_id()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_name != PUBLIC_INFORMATION_SNAPSHOT_SCHEMA_NAME
            || self.schema_version != PUBLIC_INFORMATION_SNAPSHOT_SCHEMA_VERSION
        {
            return Err("unsupported public information-snapshot schema".into());
        }
        self.observation.validate()?;
        self.history_snapshot.validate()?;
        self.candidate_surface.validate()?;
        validate_domain_scope(self.domain, self.observation.scope)?;
        if self.snapshot_id != self.expected_id()? {
            return Err("public information-snapshot id does not match its public bindings".into());
        }
        Ok(())
    }

    fn expected_id(&self) -> Result<String, String> {
        stable_planner_id(
            "public_information_snapshot_v1",
            &(
                self.schema_name.as_str(),
                self.schema_version,
                self.domain,
                &self.observation,
                &self.history_snapshot,
                &self.candidate_surface,
            ),
        )
    }
}

fn validate_domain_scope(
    domain: PublicDecisionDomainV1,
    scope: PublicObservationScopeV1,
) -> Result<(), String> {
    match (domain, scope) {
        (PublicDecisionDomainV1::Strategic, PublicObservationScopeV1::StrategicRunDecision)
        | (PublicDecisionDomainV1::Combat, PublicObservationScopeV1::CombatDecisionOnly)
        | (
            PublicDecisionDomainV1::Combat,
            PublicObservationScopeV1::CombatDecisionWithRunContinuation,
        ) => Ok(()),
        _ => Err("public observation scope does not match its decision domain".into()),
    }
}

fn validate_candidate_ids(candidate_ids: &[String]) -> Result<(), String> {
    if candidate_ids.is_empty() {
        return Err("public candidate surface must not be empty".into());
    }
    let mut unique = BTreeSet::new();
    for candidate_id in candidate_ids {
        ensure_nonempty("public candidate id", candidate_id)?;
        if !unique.insert(candidate_id.as_str()) {
            return Err("public candidate surface repeats a candidate id".into());
        }
    }
    Ok(())
}

fn nonempty(label: &str, value: String) -> Result<String, String> {
    ensure_nonempty(label, &value)?;
    Ok(value)
}

pub(super) fn ensure_nonempty(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} must not be empty"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn snapshot_identity_binds_only_capture_evidence() {
        let observation = PublicObservationReferenceV1::from_sanitized_payload(
            "CombatLearningPublicObservationV1",
            1,
            PublicObservationScopeV1::CombatDecisionWithRunContinuation,
            &json!({"turn": 1, "draw": ["Defend", "Strike"]}),
        )
        .expect("public observation");
        let history = PublicHistorySnapshotReferenceV1::from_sanitized_payload(
            "CombatPublicHistorySnapshotV1",
            1,
            &json!({"executed_moves": [1]}),
        )
        .expect("public history snapshot");
        let candidates = PublicCandidateSurfaceReferenceV1::from_candidate_ids(
            "CombatPublicCandidateSurfaceV1",
            1,
            PublicCandidateSurfaceKindV1::DeployablePolicy,
            vec!["defend".into(), "strike".into(), "end_turn".into()],
        )
        .expect("public candidates");
        let snapshot = PublicInformationSnapshotV1::new(
            PublicDecisionDomainV1::Combat,
            observation,
            history,
            candidates,
        )
        .expect("public snapshot");

        let encoded = serde_json::to_string(&snapshot).expect("serialize snapshot");
        assert!(!encoded.contains("exact_combat_state_hash"));
        assert!(!encoded.contains("root_id"));
        assert!(!encoded.contains("analysis_seed"));
        assert!(!encoded.contains("event_prefix"));
    }
}
