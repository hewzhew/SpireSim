use serde_json::{json, Value};
use sts_oracle_run_control::runtime::{
    combat::CombatState,
    rng::{StsRngCall, StsRngTraceEvent},
};
use sts_parity_lockstep::{
    compare_snapshots_v1, DomainObservationV1, NaturalRunRngCallLedgerV1, NaturalRunRngOperationV1,
    ParityDomainV1, ParitySnapshotV1, ProducerIdentityV1, SnapshotComparisonV1, SnapshotContractV1,
};
use sts_parity_lockstep_java::{
    project_combat_rng_mechanism_v1, project_monster_ai_mechanism_v1, project_player_mechanism_v1,
    RecordedJavaActionV1, RecordedJavaBoundaryV1,
};

pub(super) fn compare_creature_end_turn(
    contract: &str,
    combat: &CombatState,
    before: &RecordedJavaBoundaryV1,
    action: &RecordedJavaActionV1,
    after: &RecordedJavaBoundaryV1,
    calls: &[StsRngTraceEvent],
) -> Result<SnapshotComparisonV1, String> {
    compare_end_turn_with_cards(contract, combat, before, action, after, calls, None)
}

pub(super) fn compare_end_turn_with_cards(
    contract: &str,
    combat: &CombatState,
    before: &RecordedJavaBoundaryV1,
    action: &RecordedJavaActionV1,
    after: &RecordedJavaBoundaryV1,
    calls: &[StsRngTraceEvent],
    cards: Option<
        [(
            sts_parity_lockstep::NaturalCombatZonesV4,
            Vec<sts_parity_lockstep::NaturalRelicV3>,
        ); 2],
    >,
) -> Result<SnapshotComparisonV1, String> {
    let java_calls = validated_calls(before, action, after)?;
    if calls != java_calls {
        let first = calls
            .iter()
            .zip(&java_calls)
            .position(|(r, j)| r != j)
            .unwrap_or(calls.len().min(java_calls.len()));
        return Err(format!(
            "mechanism RNG calls differ at {first}: Rust {:?}, Java {:?}",
            calls.get(first),
            java_calls.get(first)
        ));
    }
    let (player, powers, monsters) =
        sts_parity_lockstep_rust::project_creatures_mechanism_v1(combat)?;
    let streams = sts_parity_lockstep_rust::project_ai_shuffle_mechanism_v1(combat)?;
    let mut rust = scoped_snapshot(
        contract,
        ProducerIdentityV1 {
            implementation: "sts_oracle_run_control".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            session_epoch: "constructed-offline".into(),
        },
        json!({"player":player,"player_powers":powers,"monsters":monsters}),
        json!(streams),
    )?;
    let (player, powers) = project_player_mechanism_v1(&after.observation)?;
    let (monsters, _) = project_monster_ai_mechanism_v1(&after.observation)?;
    let streams = project_combat_rng_mechanism_v1(&after.observation)?
        .0
        .into_iter()
        .filter(|s| s.owner == "floor.ai" || s.owner == "floor.shuffle")
        .collect::<Vec<_>>();
    if streams.len() != 2 {
        return Err("Java mechanism RNG streams missing".into());
    }
    let mut java = scoped_snapshot(
        contract,
        after.observation.producer.clone(),
        json!({"player":player,"player_powers":powers,"monsters":monsters}),
        json!(streams),
    )?;
    if let Some([java_cards, rust_cards]) = cards {
        for (snapshot, (zones, relics)) in [(&mut java, java_cards), (&mut rust, rust_cards)] {
            snapshot.contract = SnapshotContractV1::new(
                contract,
                [
                    (ParityDomainV1::Combat, true),
                    (ParityDomainV1::Relics, true),
                    (ParityDomainV1::RngStreams, true),
                ],
            )?;
            let Some(DomainObservationV1 {
                evidence: sts_parity_lockstep::DomainEvidenceV1::Observed { value, .. },
                ..
            }) = snapshot.domains.get(&ParityDomainV1::Combat)
            else {
                return Err("mechanism creature state missing".into());
            };
            let mut combined = value.clone();
            combined["zones"] = json!(zones);
            snapshot.domains.insert(
                ParityDomainV1::Combat,
                DomainObservationV1::observed("sts-mechanism-creatures-cards-v4", combined),
            );
            snapshot.domains.insert(
                ParityDomainV1::Relics,
                DomainObservationV1::observed(
                    sts_parity_lockstep::NATURAL_RUN_RELICS_SCHEMA_V3,
                    json!(relics),
                ),
            );
        }
    }
    compare_snapshots_v1(&java, &rust)
}

fn validated_calls(
    before: &RecordedJavaBoundaryV1,
    action: &RecordedJavaActionV1,
    after: &RecordedJavaBoundaryV1,
) -> Result<Vec<StsRngTraceEvent>, String> {
    let witness = &after.observation.frame["rng_call_witness"];
    let before_turn = before.observation.frame["public_base"]["combat"]["turn"]
        .as_u64()
        .ok_or("turn missing")?;
    if after.observation.frame["public_base"]["combat"]["turn"] != json!(before_turn + 1)
        || witness["kind"] != "rng_call_witness_v1"
        || witness["schema"] != sts_parity_lockstep::NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1
        || witness["complete"] != true
        || !witness["unsupported_reason"].is_null()
        || witness["request_id"] != json!(action.receipt.request_id)
        || witness["action_id"] != json!(action.receipt.action_id)
        || witness["from_frame_id"] != json!(before.observation.frame_id)
        || witness["to_frame_id"] != json!(after.observation.frame_id)
    {
        return Err("mechanism omitted its next turn or complete adjacent RNG witness".into());
    }
    let ledger = NaturalRunRngCallLedgerV1::new(
        serde_json::from_value(witness["calls"].clone()).map_err(|e| e.to_string())?,
    )?;
    ledger
        .0
        .iter()
        .map(|call| match (&*call.owner, &call.operation) {
            ("floor.ai", NaturalRunRngOperationV1::RandomInt { range: 99, result }) => {
                Ok(StsRngTraceEvent {
                    owner: Some("floor.ai"),
                    call: StsRngCall::RandomInt {
                        range: 99,
                        result: *result,
                    },
                })
            }
            ("floor.shuffle", NaturalRunRngOperationV1::RandomLongBits { result_hex }) => {
                Ok(StsRngTraceEvent {
                    owner: Some("floor.shuffle"),
                    call: StsRngCall::RandomLongBits {
                        result: u64::from_str_radix(result_hex, 16).map_err(|e| e.to_string())?,
                    },
                })
            }
            _ => Err("unexpected mechanism RNG operation or owner".into()),
        })
        .collect()
}

fn scoped_snapshot(
    contract: &str,
    producer: ProducerIdentityV1,
    creatures: Value,
    streams: Value,
) -> Result<ParitySnapshotV1, String> {
    Ok(ParitySnapshotV1 {
        observation_id: "mechanism-successor".into(),
        boundary_kind: "mechanism_creature_end_turn".into(),
        producer,
        contract: SnapshotContractV1::new(
            contract,
            [
                (ParityDomainV1::Combat, true),
                (ParityDomainV1::RngStreams, true),
            ],
        )?,
        domains: [
            (
                ParityDomainV1::Combat,
                DomainObservationV1::observed("sts-mechanism-creatures-v4", creatures),
            ),
            (
                ParityDomainV1::RngStreams,
                DomainObservationV1::observed("sts-mechanism-ai-shuffle-v2", streams),
            ),
        ]
        .into_iter()
        .collect(),
    })
}
