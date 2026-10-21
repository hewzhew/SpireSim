use crate::{DomainEvidenceV1, ParityDomainV1, ParitySnapshotV1};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

const MAX_DIFFERENCE_VALUE_CHARS: usize = 72;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockstepModeV1 {
    Audit,
    Certification,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonStatusV1 {
    Exact,
    Mismatch,
    Unsupported,
    Unobserved,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DomainComparisonV1 {
    pub domain: ParityDomainV1,
    pub status: ComparisonStatusV1,
    pub critical: bool,
    pub reason: Option<String>,
    pub java: Option<Value>,
    pub rust: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotComparisonV1 {
    pub status: ComparisonStatusV1,
    pub contract_exact: bool,
    pub contract_reason: Option<String>,
    pub boundary_exact: bool,
    pub boundary_reason: Option<String>,
    pub domains: Vec<DomainComparisonV1>,
}

impl SnapshotComparisonV1 {
    pub fn may_continue(&self, mode: LockstepModeV1) -> bool {
        self.contract_exact
            && self.boundary_exact
            && self.domains.iter().all(|domain| match domain.status {
                ComparisonStatusV1::Exact => true,
                ComparisonStatusV1::Unobserved => mode == LockstepModeV1::Audit && !domain.critical,
                ComparisonStatusV1::Mismatch | ComparisonStatusV1::Unsupported => false,
            })
    }

    pub fn first_stop(&self, mode: LockstepModeV1) -> Option<String> {
        if let Some(reason) = self
            .contract_reason
            .as_ref()
            .or(self.boundary_reason.as_ref())
        {
            return Some(reason.clone());
        }
        self.domains
            .iter()
            .find(|domain| match domain.status {
                ComparisonStatusV1::Exact => false,
                ComparisonStatusV1::Unobserved => {
                    mode == LockstepModeV1::Certification || domain.critical
                }
                ComparisonStatusV1::Mismatch | ComparisonStatusV1::Unsupported => true,
            })
            .map(|domain| {
                domain.reason.clone().unwrap_or_else(|| {
                    format!("{:?} comparison stopped", domain.domain).to_lowercase()
                })
            })
    }
}

pub fn compare_snapshots_v1(
    java: &ParitySnapshotV1,
    rust: &ParitySnapshotV1,
) -> Result<SnapshotComparisonV1, String> {
    let _timing = crate::timing_span_v1("compare.snapshot");
    java.validate()?;
    rust.validate()?;
    let contract_exact = java.contract == rust.contract;
    let contract_reason = (!contract_exact).then(|| {
        format!(
            "snapshot contract differs: Java={} Rust={}",
            java.contract.schema, rust.contract.schema
        )
    });
    let boundary_exact = java.boundary_kind == rust.boundary_kind;
    let boundary_reason = (!boundary_exact).then(|| {
        format!(
            "stable boundary differs: Java={} Rust={}",
            java.boundary_kind, rust.boundary_kind
        )
    });

    let domains = java
        .domains
        .keys()
        .chain(rust.domains.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|domain| compare_domain(domain, java, rust))
        .collect::<Vec<_>>();
    let status = if !boundary_exact
        || domains
            .iter()
            .any(|domain| domain.status == ComparisonStatusV1::Mismatch)
    {
        ComparisonStatusV1::Mismatch
    } else if !contract_exact
        || domains
            .iter()
            .any(|domain| domain.status == ComparisonStatusV1::Unsupported)
    {
        ComparisonStatusV1::Unsupported
    } else if domains
        .iter()
        .any(|domain| domain.status == ComparisonStatusV1::Unobserved)
    {
        ComparisonStatusV1::Unobserved
    } else {
        ComparisonStatusV1::Exact
    };
    Ok(SnapshotComparisonV1 {
        status,
        contract_exact,
        contract_reason,
        boundary_exact,
        boundary_reason,
        domains,
    })
}

fn compare_domain(
    domain: ParityDomainV1,
    java: &ParitySnapshotV1,
    rust: &ParitySnapshotV1,
) -> DomainComparisonV1 {
    let left = java.domains.get(&domain);
    let right = rust.domains.get(&domain);
    let critical = java.contract.critical(domain).unwrap_or(false)
        || rust.contract.critical(domain).unwrap_or(false);
    let (status, reason, java_value, rust_value) = match (
        left.map(|value| &value.evidence),
        right.map(|value| &value.evidence),
    ) {
        (
            Some(DomainEvidenceV1::Observed {
                schema: java_schema,
                value: java_value,
            }),
            Some(DomainEvidenceV1::Observed {
                schema: rust_schema,
                value: rust_value,
            }),
        ) if java_schema != rust_schema => (
            ComparisonStatusV1::Unsupported,
            Some(format!(
                "{domain:?} schema differs: Java={java_schema} Rust={rust_schema}"
            )),
            Some(java_value.clone()),
            Some(rust_value.clone()),
        ),
        (
            Some(DomainEvidenceV1::Observed {
                value: java_value, ..
            }),
            Some(DomainEvidenceV1::Observed {
                value: rust_value, ..
            }),
        ) if java_value == rust_value => (
            ComparisonStatusV1::Exact,
            None,
            Some(java_value.clone()),
            Some(rust_value.clone()),
        ),
        (
            Some(DomainEvidenceV1::Observed {
                value: java_value, ..
            }),
            Some(DomainEvidenceV1::Observed {
                value: rust_value, ..
            }),
        ) => (
            ComparisonStatusV1::Mismatch,
            Some(value_mismatch_reason(domain, java_value, rust_value)),
            Some(java_value.clone()),
            Some(rust_value.clone()),
        ),
        (Some(DomainEvidenceV1::Unobserved { reason }), _) => (
            ComparisonStatusV1::Unobserved,
            Some(format!("Java did not observe {domain:?}: {reason}")),
            None,
            evidence_value(right.map(|value| &value.evidence)),
        ),
        (_, Some(DomainEvidenceV1::Unobserved { reason })) => (
            ComparisonStatusV1::Unobserved,
            Some(format!("Rust did not observe {domain:?}: {reason}")),
            evidence_value(left.map(|value| &value.evidence)),
            None,
        ),
        (None, Some(_)) => (
            ComparisonStatusV1::Unobserved,
            Some(format!("Java omitted declared domain {domain:?}")),
            None,
            evidence_value(right.map(|value| &value.evidence)),
        ),
        (Some(_), None) => (
            ComparisonStatusV1::Unobserved,
            Some(format!("Rust omitted declared domain {domain:?}")),
            evidence_value(left.map(|value| &value.evidence)),
            None,
        ),
        (None, None) => unreachable!("domain came from the union"),
    };
    DomainComparisonV1 {
        domain,
        status,
        critical,
        reason,
        java: java_value,
        rust: rust_value,
    }
}

fn value_mismatch_reason(domain: ParityDomainV1, java: &Value, rust: &Value) -> String {
    let Some(difference) = first_json_difference("$", Some(java), Some(rust)) else {
        return format!("{domain:?} values differ");
    };
    format!(
        "{domain:?} values differ at {}: Java={} Rust={}",
        difference.path,
        short_json_value(difference.java),
        short_json_value(difference.rust),
    )
}

struct JsonDifference<'a> {
    path: String,
    java: Option<&'a Value>,
    rust: Option<&'a Value>,
}

fn first_json_difference<'a>(
    path: &str,
    java: Option<&'a Value>,
    rust: Option<&'a Value>,
) -> Option<JsonDifference<'a>> {
    match (java, rust) {
        (Some(left), Some(right)) if left == right => None,
        (Some(Value::Array(left)), Some(Value::Array(right))) => {
            for index in 0..left.len().max(right.len()) {
                if let Some(difference) = first_json_difference(
                    &format!("{path}[{index}]"),
                    left.get(index),
                    right.get(index),
                ) {
                    return Some(difference);
                }
            }
            None
        }
        (Some(Value::Object(left)), Some(Value::Object(right))) => {
            let keys = left
                .keys()
                .chain(right.keys())
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            for key in keys {
                if let Some(difference) =
                    first_json_difference(&format!("{path}.{key}"), left.get(key), right.get(key))
                {
                    return Some(difference);
                }
            }
            None
        }
        (java, rust) => Some(JsonDifference {
            path: path.to_owned(),
            java,
            rust,
        }),
    }
}

fn short_json_value(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "<missing>".to_owned();
    };
    let encoded = serde_json::to_string(value).unwrap_or_else(|_| "<unprintable>".to_owned());
    if encoded.chars().count() <= MAX_DIFFERENCE_VALUE_CHARS {
        return encoded;
    }
    let prefix = encoded
        .chars()
        .take(MAX_DIFFERENCE_VALUE_CHARS - 3)
        .collect::<String>();
    format!("{prefix}...")
}

fn evidence_value(value: Option<&DomainEvidenceV1>) -> Option<Value> {
    match value {
        Some(DomainEvidenceV1::Observed { value, .. }) => Some(value.clone()),
        Some(DomainEvidenceV1::Unobserved { .. }) | None => None,
    }
}
