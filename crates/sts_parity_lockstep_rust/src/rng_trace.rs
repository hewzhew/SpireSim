use std::collections::BTreeMap;
use sts_oracle_run_control::{
    eval::run_control::RunControlSession,
    runtime::rng::{begin_sts_rng_trace, StsRngCall, StsRngTraceGuard},
};
use sts_parity_lockstep::{
    NaturalRunRngCallLedgerV1, NaturalRunRngCallV1, NaturalRunRngOperationV1,
};

pub(super) struct RustRngTraceScopeV1 {
    guard: StsRngTraceGuard,
}

impl RustRngTraceScopeV1 {
    pub(super) fn begin(session: &mut RunControlSession) -> Result<Self, String> {
        bind_session_rng_owners(session);
        Ok(Self {
            guard: begin_sts_rng_trace()?,
        })
    }

    pub(super) fn finish(self) -> Result<NaturalRunRngCallLedgerV1, String> {
        let events = self.guard.finish()?;
        let mut next_ordinals = BTreeMap::<&str, u32>::new();
        let calls = events
            .into_iter()
            .map(|event| {
                let owner = event.owner.ok_or_else(|| {
                    "Rust RNG call came from an unnamed transition-local instance".to_owned()
                })?;
                if owner == "sim.math" {
                    return Err(
                        "Rust MathUtils stand-in has no comparable original-game RNG owner"
                            .to_owned(),
                    );
                }
                let ordinal = next_ordinals.entry(owner).or_default();
                let call = NaturalRunRngCallV1 {
                    owner: owner.to_owned(),
                    owner_ordinal: *ordinal,
                    operation: canonical_operation(event.call),
                };
                *ordinal = ordinal
                    .checked_add(1)
                    .ok_or_else(|| "Rust RNG call ordinal overflowed".to_owned())?;
                Ok(call)
            })
            .collect::<Result<Vec<_>, String>>()?;
        NaturalRunRngCallLedgerV1::new(calls)
    }
}

fn bind_session_rng_owners(session: &mut RunControlSession) {
    session.run_state.rng_pool.bind_trace_owners();
    session.run_state.map_rng.bind_trace_owner("run.map");
    if let Some(rng) = &mut session.run_state.neow_rng {
        rng.bind_trace_owner("run.neow");
    }
    if let Some(active) = &mut session.active_combat {
        active.combat_state.rng.pool.bind_trace_owners();
    }
}

fn canonical_operation(call: StsRngCall) -> NaturalRunRngOperationV1 {
    match call {
        StsRngCall::RandomInt { range, result } => {
            NaturalRunRngOperationV1::RandomInt { range, result }
        }
        StsRngCall::RandomIntRange { start, end, result } => {
            NaturalRunRngOperationV1::RandomIntRange { start, end, result }
        }
        StsRngCall::RandomBoolean { result } => NaturalRunRngOperationV1::RandomBoolean { result },
        StsRngCall::RandomBooleanChance { chance, result } => {
            NaturalRunRngOperationV1::RandomBooleanChance {
                chance_bits_hex: format!("{:08X}", chance.to_bits()),
                result,
            }
        }
        StsRngCall::RandomFloat { result } => NaturalRunRngOperationV1::RandomFloat {
            result_bits_hex: format!("{:08X}", result.to_bits()),
        },
        StsRngCall::RandomFloatRange { range, result } => {
            NaturalRunRngOperationV1::RandomFloatRange {
                range_bits_hex: format!("{:08X}", range.to_bits()),
                result_bits_hex: format!("{:08X}", result.to_bits()),
            }
        }
        StsRngCall::RandomFloatMinMax {
            minimum,
            maximum,
            result,
        } => NaturalRunRngOperationV1::RandomFloatMinMax {
            minimum_bits_hex: format!("{:08X}", minimum.to_bits()),
            maximum_bits_hex: format!("{:08X}", maximum.to_bits()),
            result_bits_hex: format!("{:08X}", result.to_bits()),
        },
        StsRngCall::RandomLongBits { result } => NaturalRunRngOperationV1::RandomLongBits {
            result_hex: format!("{result:016X}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_run_control::{
        eval::run_control::RunControlConfig,
        runtime::rng::{RngPool, StsRng},
    };

    #[test]
    fn original_orrery_purchase_matches_all_five_offers_and_every_rng_call() {
        use sts_oracle_run_control::{
            content::relics::RelicId,
            eval::run_control::RunControlSessionCheckpointV1,
            state::{
                core::{ClientInput, EngineState},
                rewards::RewardItem,
            },
        };
        let root: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/shop_orrery/rust-before.json"
        ))
        .unwrap();
        let checkpoint: RunControlSessionCheckpointV1 =
            serde_json::from_value(root["checkpoint"].clone()).unwrap();
        let mut session = checkpoint.into_session().unwrap();
        assert_eq!(
            (session.run_state.floor_num, session.run_state.gold),
            (37, 225)
        );
        let EngineState::Shop(shop) = &session.engine_state else {
            panic!("source is not the original shop entry")
        };
        let slot = shop
            .relics
            .iter()
            .position(|r| r.relic_id == RelicId::Orrery)
            .unwrap();
        let scope = RustRngTraceScopeV1::begin(&mut session).unwrap();
        session
            .apply_decision_action(
                sts_oracle_run_control::eval::run_control::RunDecisionAction::Input(
                    ClientInput::BuyRelic(slot),
                ),
            )
            .unwrap();
        let calls = scope.finish().unwrap().0;
        let original: serde_json::Value = serde_json::from_str(include_str!(
            "../../sts_java_parity_live/tests/fixtures/shop_orrery/open.raw.json"
        ))
        .unwrap();
        let expected: Vec<NaturalRunRngCallV1> =
            serde_json::from_value(original["rng_call_witness"]["calls"].clone()).unwrap();
        assert_eq!(calls.len(), expected.len());
        for (i, (actual, expected)) in calls.iter().zip(&expected).enumerate() {
            assert_eq!(
                actual, expected,
                "first original Orrery RNG difference at call {i}"
            );
        }
        let EngineState::RewardOverlay { reward_state, .. } = &session.engine_state else {
            panic!("Orrery did not open its rewards")
        };
        let offers = reward_state
            .items
            .iter()
            .map(|item| {
                let RewardItem::Card { cards } = item else {
                    panic!("unexpected Orrery reward")
                };
                cards
                    .iter()
                    .map(|c| {
                        (
                            sts_java_parity_protocol::canonical_game_id(c.id.stable_id()),
                            u64::from(c.upgrades),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let expected_offers = original["parity_internal"]["combat_rewards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                item["cards"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| {
                        (
                            sts_java_parity_protocol::canonical_game_id(
                                c["card_id"].as_str().unwrap(),
                            ),
                            c["upgrades"].as_u64().unwrap(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(offers, expected_offers);
        assert_eq!(
            i64::from(session.run_state.card_blizz_randomizer),
            original["parity_internal"]["run_state"]["drop_state"]["card_rarity_offset"]
                .as_i64()
                .unwrap()
        );
    }

    #[test]
    fn scope_rebinds_restored_streams_and_keeps_owner_across_moves() {
        let mut session = RunControlSession::new(RunControlConfig {
            seed: 42,
            ..Default::default()
        });
        session.run_state.neow_rng = Some(StsRng::new(42));
        let scope = RustRngTraceScopeV1::begin(&mut session).unwrap();
        let card_result = session.run_state.rng_pool.card_rng.random(7);
        let neow_result = session
            .run_state
            .neow_rng
            .take()
            .map(|mut rng| rng.random_range(0, 5))
            .unwrap();
        let mut transition_local_pool = RngPool::new(99);
        let moved_result = transition_local_pool.event_rng.random_boolean();
        let ledger = scope.finish().unwrap();

        assert_eq!(
            ledger.0,
            [
                NaturalRunRngCallV1 {
                    owner: "run.card".to_owned(),
                    owner_ordinal: 0,
                    operation: NaturalRunRngOperationV1::RandomInt {
                        range: 7,
                        result: card_result,
                    },
                },
                NaturalRunRngCallV1 {
                    owner: "run.neow".to_owned(),
                    owner_ordinal: 0,
                    operation: NaturalRunRngOperationV1::RandomIntRange {
                        start: 0,
                        end: 5,
                        result: neow_result,
                    },
                },
                NaturalRunRngCallV1 {
                    owner: "run.event".to_owned(),
                    owner_ordinal: 0,
                    operation: NaturalRunRngOperationV1::RandomBoolean {
                        result: moved_result,
                    },
                },
            ]
        );
    }

    #[test]
    fn scope_rejects_unnamed_and_simulator_only_rng_calls() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut unnamed = StsRng::new(7);
        let unnamed_scope = RustRngTraceScopeV1::begin(&mut session).unwrap();
        let _ = unnamed.random_boolean();
        assert!(unnamed_scope.finish().is_err());

        let math_scope = RustRngTraceScopeV1::begin(&mut session).unwrap();
        let _ = session.run_state.rng_pool.math_rng.random_boolean();
        assert!(math_scope.finish().is_err());
    }
}
