//! Clean-room contracts for outcome-learned run planning.
//!
//! This module owns representation and provenance only. Production policies,
//! strategic verdicts, and run-control orchestration must stay outside it.

use blake2::{Blake2b512, Digest};
use serde::Serialize;

mod information_snapshot;
mod public_map_preparation;
mod public_map_route;
mod types;

pub use information_snapshot::*;
pub use public_map_preparation::*;
pub use public_map_route::*;
pub use types::*;

pub const PLANNER_OBSERVATION_SCHEMA_NAME: &str = "PlannerObservation";
pub const PLANNER_OBSERVATION_SCHEMA_VERSION: u32 = 7;
pub const LEGAL_CANDIDATE_SET_SCHEMA_NAME: &str = "LegalCandidateSet";
pub const LEGAL_CANDIDATE_SET_SCHEMA_VERSION: u32 = 3;
pub const PLANNER_BEHAVIOR_EVENT_SCHEMA_NAME: &str = "PlannerBehaviorEvent";
pub const PLANNER_BEHAVIOR_EVENT_SCHEMA_VERSION: u32 = 1;
pub const PLANNER_MECHANICS_ID: &str = "sts_simulator_rust_mechanics";
pub const PLANNER_MECHANICS_VERSION: u32 = 1;

pub fn stable_planner_id<T: Serialize>(namespace: &str, value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut hasher = Blake2b512::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    let digest = hasher.finalize();
    Ok(format!("{namespace}:{}", hex_prefix(&digest, 20)))
}

fn hex_prefix(bytes: &[u8], byte_count: usize) -> String {
    let mut out = String::with_capacity(byte_count.saturating_mul(2));
    for byte in bytes.iter().take(byte_count) {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids_include_namespace_and_ignore_map_insertion_order() {
        let left = std::collections::BTreeMap::from([("b", 2), ("a", 1)]);
        let right = std::collections::BTreeMap::from([("a", 1), ("b", 2)]);
        let left_id = stable_planner_id("test", &left).expect("hash left");
        let right_id = stable_planner_id("test", &right).expect("hash right");

        assert_eq!(left_id, right_id);
        assert!(left_id.starts_with("test:"));
    }
}
