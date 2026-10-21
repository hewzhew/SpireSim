use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointV1 {
    Java,
    Rust,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParityDomainV1 {
    Run,
    Map,
    Room,
    ActionSpace,
    MasterDeck,
    Relics,
    Potions,
    Combat,
    Rewards,
    RngStreams,
    CardPools,
    CardBindings,
    RuntimeSettlement,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerIdentityV1 {
    pub implementation: String,
    pub version: String,
    pub session_epoch: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotContractV1 {
    pub schema: String,
    /// Every declared domain must have one observation, even when the
    /// adapter can only report that domain as unobserved.
    pub domains: BTreeMap<ParityDomainV1, bool>,
}

impl SnapshotContractV1 {
    pub fn new(
        schema: impl Into<String>,
        domains: impl IntoIterator<Item = (ParityDomainV1, bool)>,
    ) -> Result<Self, String> {
        let contract = Self {
            schema: schema.into(),
            domains: domains.into_iter().collect(),
        };
        if contract.schema.is_empty() || contract.domains.is_empty() {
            return Err("snapshot contract schema and domains must not be empty".to_owned());
        }
        Ok(contract)
    }

    pub fn critical(&self, domain: ParityDomainV1) -> Option<bool> {
        self.domains.get(&domain).copied()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum DomainEvidenceV1 {
    Observed { schema: String, value: Value },
    Unobserved { reason: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DomainObservationV1 {
    pub evidence: DomainEvidenceV1,
}

impl DomainObservationV1 {
    pub fn observed(schema: impl Into<String>, value: Value) -> Self {
        Self {
            evidence: DomainEvidenceV1::Observed {
                schema: schema.into(),
                value,
            },
        }
    }

    pub fn unobserved(reason: impl Into<String>) -> Self {
        Self {
            evidence: DomainEvidenceV1::Unobserved {
                reason: reason.into(),
            },
        }
    }
}

/// A state captured only after its adapter has reached a stable player-choice
/// boundary. Rendering frames and transient action queues must not construct
/// this type.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParitySnapshotV1 {
    pub observation_id: String,
    pub boundary_kind: String,
    pub producer: ProducerIdentityV1,
    pub contract: SnapshotContractV1,
    pub domains: BTreeMap<ParityDomainV1, DomainObservationV1>,
}

impl ParitySnapshotV1 {
    pub fn validate(&self) -> Result<(), String> {
        for (label, value) in [
            ("observation ID", self.observation_id.as_str()),
            ("boundary kind", self.boundary_kind.as_str()),
            (
                "producer implementation",
                self.producer.implementation.as_str(),
            ),
            ("producer version", self.producer.version.as_str()),
            (
                "producer session epoch",
                self.producer.session_epoch.as_str(),
            ),
        ] {
            if value.is_empty() {
                return Err(format!("parity snapshot {label} is empty"));
            }
        }
        if self.contract.schema.is_empty() || self.contract.domains.is_empty() {
            return Err("parity snapshot has an invalid contract".to_owned());
        }
        if self.domains.keys().ne(self.contract.domains.keys()) {
            return Err(
                "parity snapshot observations do not exactly cover its contract".to_owned(),
            );
        }
        for observation in self.domains.values() {
            match &observation.evidence {
                DomainEvidenceV1::Observed { schema, .. } if schema.is_empty() => {
                    return Err("observed parity domain has an empty schema".to_owned())
                }
                DomainEvidenceV1::Unobserved { reason } if reason.is_empty() => {
                    return Err("unobserved parity domain has no reason".to_owned())
                }
                _ => {}
            }
        }
        Ok(())
    }
}
