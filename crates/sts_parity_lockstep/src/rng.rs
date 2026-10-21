use crate::{natural_run_active_rng_owners_v4, NaturalRunRngRoomV3};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const NATURAL_RUN_RNG_ALGORITHM_V2: &str = "random_xs128";
pub const NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1: &str = "sts-parity-natural-run-rng-call-ledger-v1";
pub const NATURAL_RUN_RNG_CALL_COMPARISON_SCHEMA_V1: &str =
    "sts-parity-natural-run-rng-call-comparison-v1";

/// One canonical RNG stream. Runtime-specific field names never enter this
/// contract; Java and Rust adapters resolve them before construction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunRngStreamV2 {
    pub owner: String,
    pub algorithm: String,
    pub scope: String,
    pub state_hex: [String; 2],
    pub counter: Option<u32>,
}

impl NaturalRunRngStreamV2 {
    pub fn new(
        owner: impl Into<String>,
        scope: impl Into<String>,
        state_hex: [String; 2],
        counter: Option<u32>,
    ) -> Result<Self, String> {
        let stream = Self {
            owner: owner.into(),
            algorithm: NATURAL_RUN_RNG_ALGORITHM_V2.to_owned(),
            scope: scope.into(),
            state_hex,
            counter,
        };
        stream.validate()?;
        Ok(stream)
    }

    pub fn validate(&self) -> Result<(), String> {
        let expected_scope = if self.owner.starts_with("floor.") {
            "floor"
        } else if self.owner.starts_with("run.") {
            "run"
        } else {
            return Err(format!(
                "canonical RNG owner {:?} has no known scope",
                self.owner
            ));
        };
        if self.algorithm != NATURAL_RUN_RNG_ALGORITHM_V2 || self.scope != expected_scope {
            return Err(format!(
                "canonical RNG metadata is invalid for {:?}",
                self.owner
            ));
        }
        if self.state_hex.iter().any(|word| {
            word.len() != 16
                || !word.bytes().all(|byte| byte.is_ascii_hexdigit())
                || word.bytes().any(|byte| byte.is_ascii_lowercase())
        }) {
            return Err(format!(
                "canonical RNG state is invalid for {:?}",
                self.owner
            ));
        }
        if (self.owner == "run.map") != self.counter.is_none() {
            return Err(format!(
                "canonical RNG counter policy is invalid for {:?}",
                self.owner
            ));
        }
        Ok(())
    }
}

/// Canonically ordered active streams for one stable natural boundary.
///
/// The transparent representation deliberately preserves the existing JSON
/// array while moving its shape, validation, and ordering to one owner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NaturalRunRngStreamsV2(pub Vec<NaturalRunRngStreamV2>);

impl NaturalRunRngStreamsV2 {
    pub fn for_boundary(
        mut streams: Vec<NaturalRunRngStreamV2>,
        boundary_kind: &str,
        neow_rng_active: bool,
        room: NaturalRunRngRoomV3,
        emerald_key_acquired: bool,
    ) -> Result<Self, String> {
        let active = natural_run_active_rng_owners_v4(
            boundary_kind,
            neow_rng_active,
            room,
            emerald_key_acquired,
        )?;
        streams.retain(|stream| active.contains(&stream.owner.as_str()));
        streams.sort_by(|left, right| left.owner.cmp(&right.owner));
        let result = Self(streams);
        result.validate_for_boundary(boundary_kind, neow_rng_active, room, emerald_key_acquired)?;
        Ok(result)
    }

    pub fn validate_for_boundary(
        &self,
        boundary_kind: &str,
        neow_rng_active: bool,
        room: NaturalRunRngRoomV3,
        emerald_key_acquired: bool,
    ) -> Result<(), String> {
        let expected = natural_run_active_rng_owners_v4(
            boundary_kind,
            neow_rng_active,
            room,
            emerald_key_acquired,
        )?
        .into_iter()
        .collect::<BTreeSet<_>>();
        let mut actual = BTreeSet::new();
        for stream in &self.0 {
            stream.validate()?;
            if !actual.insert(stream.owner.as_str()) {
                return Err(format!(
                    "canonical RNG streams repeat owner {:?}",
                    stream.owner
                ));
            }
        }
        if actual != expected {
            return Err("canonical RNG streams do not exactly cover the active owners".to_owned());
        }
        if self.0.windows(2).any(|pair| pair[0].owner >= pair[1].owner) {
            return Err("canonical RNG streams are not ordered by owner".to_owned());
        }
        Ok(())
    }
}

/// One original-game `com.megacrit.cardcrawl.random.Random` wrapper call.
///
/// Float values use exact IEEE-754 bit strings and raw `randomLong()` results
/// use exact two's-complement bit strings. This avoids decimal or signedness
/// differences at the Java/Rust boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalRunRngOperationV1 {
    RandomInt {
        range: i32,
        result: i32,
    },
    RandomIntRange {
        start: i32,
        end: i32,
        result: i32,
    },
    RandomLong {
        range: i64,
        result: i64,
    },
    RandomLongRange {
        start: i64,
        end: i64,
        result: i64,
    },
    RandomLongBits {
        result_hex: String,
    },
    RandomBoolean {
        result: bool,
    },
    RandomBooleanChance {
        chance_bits_hex: String,
        result: bool,
    },
    RandomFloat {
        result_bits_hex: String,
    },
    RandomFloatRange {
        range_bits_hex: String,
        result_bits_hex: String,
    },
    RandomFloatMinMax {
        minimum_bits_hex: String,
        maximum_bits_hex: String,
        result_bits_hex: String,
    },
}

impl NaturalRunRngOperationV1 {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::RandomInt { range, result } => {
                if *range < 0 || *result < 0 || result > range {
                    return Err(
                        "canonical RNG integer call is outside its inclusive range".to_owned()
                    );
                }
            }
            Self::RandomIntRange { start, end, result } => {
                if start > end || result < start || result > end {
                    return Err("canonical RNG integer range call has invalid bounds".to_owned());
                }
            }
            Self::RandomLong { range, result } => {
                if *range < 0
                    || *result < 0
                    || (*range > 0 && result >= range)
                    || (*range == 0 && *result != 0)
                {
                    return Err("canonical RNG long call is outside its exclusive range".to_owned());
                }
            }
            Self::RandomLongRange { start, end, result } => {
                if start > end
                    || result < start
                    || (start < end && result >= end)
                    || (start == end && result != start)
                {
                    return Err("canonical RNG long range call has invalid bounds".to_owned());
                }
            }
            Self::RandomLongBits { result_hex } => validate_hex(result_hex, 16, "long result")?,
            Self::RandomBoolean { .. } => {}
            Self::RandomBooleanChance {
                chance_bits_hex, ..
            } => {
                let chance = f32_from_hex(chance_bits_hex, "boolean chance")?;
                if !(0.0..=1.0).contains(&chance) {
                    return Err("canonical RNG boolean chance is outside zero to one".to_owned());
                }
            }
            Self::RandomFloat { result_bits_hex } => {
                validate_unit_float(result_bits_hex, "float result")?;
            }
            Self::RandomFloatRange {
                range_bits_hex,
                result_bits_hex,
            } => {
                let range = f32_from_hex(range_bits_hex, "float range")?;
                let result = f32_from_hex(result_bits_hex, "float result")?;
                if range < 0.0
                    || result < 0.0
                    || (range > 0.0 && result >= range)
                    || (range == 0.0 && result != 0.0)
                {
                    return Err(
                        "canonical RNG float call is outside its exclusive range".to_owned()
                    );
                }
            }
            Self::RandomFloatMinMax {
                minimum_bits_hex,
                maximum_bits_hex,
                result_bits_hex,
            } => {
                let minimum = f32_from_hex(minimum_bits_hex, "float minimum")?;
                let maximum = f32_from_hex(maximum_bits_hex, "float maximum")?;
                let result = f32_from_hex(result_bits_hex, "float result")?;
                if minimum > maximum
                    || result < minimum
                    || (minimum < maximum && result >= maximum)
                    || (minimum == maximum && result != minimum)
                {
                    return Err("canonical RNG float min/max call has invalid bounds".to_owned());
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunRngCallV1 {
    pub owner: String,
    pub owner_ordinal: u32,
    pub operation: NaturalRunRngOperationV1,
}

/// Calls remain in execution order while each owner's ordinal must be
/// contiguous. Cross-owner order is therefore available for diagnostics
/// without using a process-global sequence identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NaturalRunRngCallLedgerV1(pub Vec<NaturalRunRngCallV1>);

impl NaturalRunRngCallLedgerV1 {
    pub fn new(calls: Vec<NaturalRunRngCallV1>) -> Result<Self, String> {
        let ledger = Self(calls);
        ledger.validate()?;
        Ok(ledger)
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut next_ordinals = BTreeMap::<&str, u32>::new();
        for call in &self.0 {
            if !known_rng_owner(&call.owner) {
                return Err(format!(
                    "canonical RNG call has unknown owner {:?}",
                    call.owner
                ));
            }
            let next = next_ordinals.entry(&call.owner).or_default();
            if call.owner_ordinal != *next {
                return Err(format!(
                    "canonical RNG calls for {:?} have a non-contiguous ordinal",
                    call.owner
                ));
            }
            call.operation.validate()?;
            *next = next
                .checked_add(1)
                .ok_or_else(|| "canonical RNG call ordinal overflowed".to_owned())?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunRngCallDifferenceV1 {
    pub call_index: u64,
    pub java: Option<NaturalRunRngCallV1>,
    pub rust: Option<NaturalRunRngCallV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunRngCallComparisonV1 {
    pub schema: String,
    pub exact: bool,
    pub java_digest: String,
    pub rust_digest: String,
    pub first_difference: Option<NaturalRunRngCallDifferenceV1>,
}

impl NaturalRunRngCallComparisonV1 {
    pub fn stop_reason(&self) -> Option<String> {
        let difference = self.first_difference.as_ref()?;
        Some(format!(
            "RNG calls differ at transition call {}: Java {}; Rust {}",
            difference.call_index,
            describe_rng_call(difference.java.as_ref()),
            describe_rng_call(difference.rust.as_ref()),
        ))
    }
}

pub fn compare_natural_run_rng_calls_v1(
    java: &NaturalRunRngCallLedgerV1,
    rust: &NaturalRunRngCallLedgerV1,
) -> Result<NaturalRunRngCallComparisonV1, String> {
    let _timing = crate::timing_span_v1("compare.rng");
    java.validate()?;
    rust.validate()?;
    let first_difference = java
        .0
        .iter()
        .zip(&rust.0)
        .position(|(java, rust)| java != rust)
        .or_else(|| (java.0.len() != rust.0.len()).then(|| java.0.len().min(rust.0.len())))
        .map(|index| -> Result<_, String> {
            Ok(NaturalRunRngCallDifferenceV1 {
                call_index: u64::try_from(index)
                    .map_err(|_| "RNG-call ledger index exceeds u64".to_owned())?,
                java: java.0.get(index).cloned(),
                rust: rust.0.get(index).cloned(),
            })
        })
        .transpose()?;
    Ok(NaturalRunRngCallComparisonV1 {
        schema: NATURAL_RUN_RNG_CALL_COMPARISON_SCHEMA_V1.to_owned(),
        exact: first_difference.is_none(),
        java_digest: rng_call_ledger_digest(java)?,
        rust_digest: rng_call_ledger_digest(rust)?,
        first_difference,
    })
}

fn rng_call_ledger_digest(ledger: &NaturalRunRngCallLedgerV1) -> Result<String, String> {
    let encoded = serde_json::to_vec(ledger)
        .map_err(|error| format!("failed to encode canonical RNG-call ledger: {error}"))?;
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn describe_rng_call(call: Option<&NaturalRunRngCallV1>) -> String {
    call.map(|call| format!("{}#{} {:?}", call.owner, call.owner_ordinal, call.operation))
        .unwrap_or_else(|| "<missing>".to_owned())
}

fn known_rng_owner(owner: &str) -> bool {
    owner == "run.neow"
        || owner.starts_with("run.") && crate::NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2.contains(&owner)
        || owner.starts_with("floor.") && crate::NATURAL_RUN_FLOOR_RNG_OWNERS_V2.contains(&owner)
}

fn validate_hex(value: &str, width: usize, label: &str) -> Result<(), String> {
    if value.len() != width
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().any(|byte| byte.is_ascii_lowercase())
    {
        return Err(format!(
            "canonical RNG {label} is not uppercase hexadecimal"
        ));
    }
    Ok(())
}

fn f32_from_hex(value: &str, label: &str) -> Result<f32, String> {
    validate_hex(value, 8, label)?;
    let bits = u32::from_str_radix(value, 16)
        .map_err(|_| format!("canonical RNG {label} is not hexadecimal"))?;
    let value = f32::from_bits(bits);
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| format!("canonical RNG {label} is not finite"))
}

fn validate_unit_float(value: &str, label: &str) -> Result<(), String> {
    let value = f32_from_hex(value, label)?;
    (0.0..1.0)
        .contains(&value)
        .then_some(())
        .ok_or_else(|| format!("canonical RNG {label} is outside zero to one"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NATURAL_RUN_FLOOR_RNG_OWNERS_V2, NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2};

    #[test]
    fn boundary_constructor_filters_dormant_streams_and_orders_active_owners() {
        let mut streams = NATURAL_RUN_FLOOR_RNG_OWNERS_V2
            .iter()
            .chain(NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2.iter())
            .map(|owner| stream(owner))
            .collect::<Vec<_>>();
        streams.reverse();

        let map = NaturalRunRngStreamsV2::for_boundary(
            streams,
            "natural_map",
            false,
            NaturalRunRngRoomV3::Other,
            false,
        )
        .unwrap();

        assert_eq!(map.0.len(), NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2.len());
        assert!(map.0.iter().all(|stream| stream.scope == "run"));
        assert!(map.0.windows(2).all(|pair| pair[0].owner < pair[1].owner));
    }

    #[test]
    fn boundary_constructor_rejects_an_incomplete_active_family() {
        let streams = NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2[..7]
            .iter()
            .map(|owner| stream(owner))
            .collect();

        let error = NaturalRunRngStreamsV2::for_boundary(
            streams,
            "natural_map",
            false,
            NaturalRunRngRoomV3::Other,
            false,
        )
        .unwrap_err();

        assert!(error.contains("exactly cover the active owners"));
    }

    #[test]
    fn emerald_key_retires_only_the_obsolete_map_cursor_and_keeps_call_evidence() {
        let all = NATURAL_RUN_FLOOR_RNG_OWNERS_V2
            .iter()
            .chain(NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2.iter())
            .map(|owner| stream(owner))
            .collect::<Vec<_>>();
        let project = |streams, has_key| {
            NaturalRunRngStreamsV2::for_boundary(
                streams,
                "natural_combat",
                false,
                NaturalRunRngRoomV3::Monster,
                has_key,
            )
            .unwrap()
        };
        let mut native_map = all.clone();
        native_map
            .iter_mut()
            .find(|s| s.owner == "run.map")
            .unwrap()
            .state_hex = ["000000000000000A".to_owned(), "000000000000000B".to_owned()];
        assert_ne!(
            project(all.clone(), false),
            project(native_map.clone(), false)
        );
        assert_eq!(project(all.clone(), true), project(native_map, true));
        assert_eq!(project(all.clone(), true).0.len(), 12);
        for owner in NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2
            .iter()
            .chain(NATURAL_RUN_FLOOR_RNG_OWNERS_V2.iter())
            .filter(|owner| **owner != "run.map")
        {
            let mut changed = all.clone();
            changed
                .iter_mut()
                .find(|s| &s.owner == owner)
                .unwrap()
                .counter = Some(8);
            assert_ne!(
                project(all.clone(), true),
                project(changed, true),
                "{owner}"
            );
        }
        // A map-generation or burning-elite draw remains transition evidence,
        // including on the action that retires the persistent cursor.
        let calls = NaturalRunRngCallLedgerV1::new(vec![NaturalRunRngCallV1 {
            owner: "run.map".to_owned(),
            owner_ordinal: 0,
            operation: NaturalRunRngOperationV1::RandomIntRange {
                start: 0,
                end: 3,
                result: 2,
            },
        }])
        .unwrap();
        let missing = NaturalRunRngCallLedgerV1::new(vec![]).unwrap();
        assert!(
            !compare_natural_run_rng_calls_v1(&calls, &missing)
                .unwrap()
                .exact
        );
    }

    #[test]
    fn ordinary_rewards_retire_combat_rng_but_keep_reward_relic_rng() {
        let all = NATURAL_RUN_FLOOR_RNG_OWNERS_V2
            .iter()
            .chain(NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2.iter())
            .map(|owner| stream(owner))
            .collect::<Vec<_>>();
        for room in [NaturalRunRngRoomV3::Monster, NaturalRunRngRoomV3::Elite] {
            for boundary in ["natural_combat_reward", "natural_card_reward"] {
                let canonical =
                    NaturalRunRngStreamsV2::for_boundary(all.clone(), boundary, false, room, false)
                        .unwrap();
                let mut reloaded = all.clone();
                for stream in &mut reloaded {
                    if stream.scope == "floor" && stream.owner != "floor.misc" {
                        stream.counter = Some(6);
                        stream.state_hex =
                            ["0000000000000001".to_owned(), "0000000000000002".to_owned()];
                    }
                }
                assert_eq!(
                    canonical,
                    NaturalRunRngStreamsV2::for_boundary(reloaded, boundary, false, room, false)
                        .unwrap()
                );
                assert_eq!(canonical.0.len(), 9);
                let mut missing_misc = all.clone();
                missing_misc.retain(|stream| stream.owner != "floor.misc");
                assert!(NaturalRunRngStreamsV2::for_boundary(
                    missing_misc,
                    boundary,
                    false,
                    room,
                    false
                )
                .is_err());
                let mut different_misc = all.clone();
                different_misc
                    .iter_mut()
                    .find(|stream| stream.owner == "floor.misc")
                    .unwrap()
                    .counter = Some(42);
                assert_ne!(
                    canonical,
                    NaturalRunRngStreamsV2::for_boundary(
                        different_misc,
                        boundary,
                        false,
                        room,
                        false
                    )
                    .unwrap()
                );
            }
        }
        // An event can reopen into a second fight without nextRoomTransition.
        // Bosses are conservative too; ordinary combat itself keeps all owners.
        for (boundary, room) in [
            ("natural_combat_reward", NaturalRunRngRoomV3::Other),
            ("natural_double_boss_complete", NaturalRunRngRoomV3::Other),
            ("natural_card_reward", NaturalRunRngRoomV3::Other),
            ("natural_combat", NaturalRunRngRoomV3::Monster),
            ("natural_hand_select", NaturalRunRngRoomV3::Elite),
            ("natural_combat_discovery", NaturalRunRngRoomV3::Monster),
            ("natural_combat_codex", NaturalRunRngRoomV3::Monster),
        ] {
            let canonical =
                NaturalRunRngStreamsV2::for_boundary(all.clone(), boundary, false, room, false)
                    .unwrap();
            assert_eq!(canonical.0.len(), 13);
            let mut missing_ai = all.clone();
            missing_ai.retain(|stream| stream.owner != "floor.ai");
            assert!(
                NaturalRunRngStreamsV2::for_boundary(missing_ai, boundary, false, room, false)
                    .is_err()
            );
        }
    }

    #[test]
    fn call_ledger_preserves_cross_owner_order_and_owner_ordinals() {
        let ledger = NaturalRunRngCallLedgerV1::new(vec![
            NaturalRunRngCallV1 {
                owner: "run.neow".to_owned(),
                owner_ordinal: 0,
                operation: NaturalRunRngOperationV1::RandomIntRange {
                    start: 0,
                    end: 5,
                    result: 3,
                },
            },
            NaturalRunRngCallV1 {
                owner: "run.card".to_owned(),
                owner_ordinal: 0,
                operation: NaturalRunRngOperationV1::RandomBooleanChance {
                    chance_bits_hex: format!("{:08X}", 0.33_f32.to_bits()),
                    result: false,
                },
            },
            NaturalRunRngCallV1 {
                owner: "run.neow".to_owned(),
                owner_ordinal: 1,
                operation: NaturalRunRngOperationV1::RandomLongBits {
                    result_hex: "FFFFFFFFFFFFFFFF".to_owned(),
                },
            },
        ])
        .unwrap();

        let encoded = serde_json::to_value(&ledger).unwrap();
        assert_eq!(encoded[0]["operation"]["method"], "random_int_range");
        assert_eq!(
            serde_json::from_value::<NaturalRunRngCallLedgerV1>(encoded).unwrap(),
            ledger
        );
    }

    #[test]
    fn call_ledger_rejects_unknown_owners_and_skipped_ordinals() {
        let call = |owner: &str, owner_ordinal| NaturalRunRngCallV1 {
            owner: owner.to_owned(),
            owner_ordinal,
            operation: NaturalRunRngOperationV1::RandomBoolean { result: true },
        };

        assert!(NaturalRunRngCallLedgerV1::new(vec![call("sim.math", 0)]).is_err());
        assert!(NaturalRunRngCallLedgerV1::new(vec![call("run.card", 1)]).is_err());
    }

    #[test]
    fn call_comparison_is_exact_without_exposing_the_ledger() {
        let ledger = NaturalRunRngCallLedgerV1::new(vec![NaturalRunRngCallV1 {
            owner: "run.card".to_owned(),
            owner_ordinal: 0,
            operation: NaturalRunRngOperationV1::RandomIntRange {
                start: 1,
                end: 3,
                result: 2,
            },
        }])
        .unwrap();

        let comparison = compare_natural_run_rng_calls_v1(&ledger, &ledger).unwrap();

        assert!(comparison.exact);
        assert_eq!(comparison.java_digest, comparison.rust_digest);
        assert!(comparison.first_difference.is_none());
        assert!(comparison.stop_reason().is_none());
    }

    #[test]
    fn call_comparison_locates_parameter_or_result_divergence() {
        let call = |end, result| {
            NaturalRunRngCallLedgerV1::new(vec![NaturalRunRngCallV1 {
                owner: "run.card".to_owned(),
                owner_ordinal: 0,
                operation: NaturalRunRngOperationV1::RandomIntRange {
                    start: 1,
                    end,
                    result,
                },
            }])
            .unwrap()
        };

        for rust in [call(4, 2), call(3, 3)] {
            let comparison = compare_natural_run_rng_calls_v1(&call(3, 2), &rust).unwrap();
            assert!(!comparison.exact);
            assert_eq!(comparison.first_difference.unwrap().call_index, 0);
        }
    }

    #[test]
    fn call_comparison_distinguishes_order_and_missing_calls() {
        let call = |owner: &str| NaturalRunRngCallV1 {
            owner: owner.to_owned(),
            owner_ordinal: 0,
            operation: NaturalRunRngOperationV1::RandomBoolean { result: true },
        };
        let java =
            NaturalRunRngCallLedgerV1::new(vec![call("run.card"), call("run.event")]).unwrap();
        let reordered =
            NaturalRunRngCallLedgerV1::new(vec![call("run.event"), call("run.card")]).unwrap();
        let shorter = NaturalRunRngCallLedgerV1::new(vec![call("run.card")]).unwrap();

        let order = compare_natural_run_rng_calls_v1(&java, &reordered).unwrap();
        assert_eq!(order.first_difference.unwrap().call_index, 0);
        let missing = compare_natural_run_rng_calls_v1(&java, &shorter).unwrap();
        let difference = missing.first_difference.unwrap();
        assert_eq!(difference.call_index, 1);
        assert_eq!(difference.java.unwrap().owner, "run.event");
        assert!(difference.rust.is_none());
    }

    fn stream(owner: &str) -> NaturalRunRngStreamV2 {
        NaturalRunRngStreamV2::new(
            owner,
            if owner.starts_with("floor.") {
                "floor"
            } else {
                "run"
            },
            ["0000000000000001".to_owned(), "0000000000000002".to_owned()],
            (owner != "run.map").then_some(0),
        )
        .unwrap()
    }
}
