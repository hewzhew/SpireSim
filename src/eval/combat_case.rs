use std::fs;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::ai::strategy::trajectory_comparison::TrajectorySnapshot;
use crate::eval::combat_case_context::{
    capture_combat_case_production_context_v2, combat_case_replay_identity_v2,
    CombatCaseProductionContextV2, CombatCaseReplayCapabilityV2, CombatCaseReplayIdentityV2,
};
use crate::eval::run_control::AtomicCombatSearchTraceSummaryV2;
use crate::eval::run_control::RunControlSession;
use crate::sim::combat::CombatPosition;

pub use crate::eval::combat_case_core::{
    card_summary, combat_summary, living_enemy_names, CombatCaseCardSummary,
    CombatCaseCombatSummary, CombatCaseCoreV2, CombatCaseGap, CombatCaseRngSummary,
    CombatCaseRunSummary, CombatCaseSource, CombatCaseWitnessBudgetV1, COMBAT_CASE_SCHEMA,
};

#[derive(Clone, Debug)]
pub struct CombatCase {
    pub core: CombatCaseCoreV2,
    pub branch_evidence: Option<CombatCaseBranchEvidence>,
    pub production_context: Option<CombatCaseProductionContextV2>,
    pub atomic_combat_search_attempts: Vec<AtomicCombatSearchTraceSummaryV2>,
    pub failed_atomic_combat_search: Option<AtomicCombatSearchTraceSummaryV2>,
    pub path: Vec<CombatCasePathStep>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CombatCaseWireV2 {
    schema: String,
    source: CombatCaseSource,
    gap: CombatCaseGap,
    run: CombatCaseRunSummary,
    combat: CombatCaseCombatSummary,
    #[serde(default)]
    branch_evidence: Option<CombatCaseBranchEvidence>,
    #[serde(default)]
    production_context: Option<CombatCaseProductionContextV2>,
    #[serde(default)]
    atomic_combat_search_attempts: Vec<AtomicCombatSearchTraceSummaryV2>,
    #[serde(default)]
    failed_atomic_combat_search: Option<AtomicCombatSearchTraceSummaryV2>,
    #[serde(default)]
    path: Vec<CombatCasePathStep>,
    run_rng: CombatCaseRngSummary,
    position: CombatPosition,
}

#[derive(Serialize)]
struct CombatCaseWireRefV2<'a> {
    schema: &'a str,
    source: &'a CombatCaseSource,
    gap: &'a CombatCaseGap,
    run: &'a CombatCaseRunSummary,
    combat: &'a CombatCaseCombatSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch_evidence: Option<&'a CombatCaseBranchEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    production_context: Option<&'a CombatCaseProductionContextV2>,
    #[serde(skip_serializing_if = "slice_ref_is_empty")]
    atomic_combat_search_attempts: &'a [AtomicCombatSearchTraceSummaryV2],
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_atomic_combat_search: Option<&'a AtomicCombatSearchTraceSummaryV2>,
    #[serde(skip_serializing_if = "slice_ref_is_empty")]
    path: &'a [CombatCasePathStep],
    run_rng: &'a CombatCaseRngSummary,
    position: &'a CombatPosition,
}

impl<'de> Deserialize<'de> for CombatCase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CombatCaseWireV2::deserialize(deserializer)?;
        Ok(Self {
            core: CombatCaseCoreV2 {
                schema: wire.schema,
                source: wire.source,
                gap: wire.gap,
                run: wire.run,
                combat: wire.combat,
                run_rng: wire.run_rng,
                position: wire.position,
            },
            branch_evidence: wire.branch_evidence,
            production_context: wire.production_context,
            atomic_combat_search_attempts: wire.atomic_combat_search_attempts,
            failed_atomic_combat_search: wire.failed_atomic_combat_search,
            path: wire.path,
        })
    }
}

impl Serialize for CombatCase {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CombatCaseWireRefV2 {
            schema: &self.core.schema,
            source: &self.core.source,
            gap: &self.core.gap,
            run: &self.core.run,
            combat: &self.core.combat,
            branch_evidence: self.branch_evidence.as_ref(),
            production_context: self.production_context.as_ref(),
            atomic_combat_search_attempts: &self.atomic_combat_search_attempts,
            failed_atomic_combat_search: self.failed_atomic_combat_search.as_ref(),
            path: &self.path,
            run_rng: &self.core.run_rng,
            position: &self.core.position,
        }
        .serialize(serializer)
    }
}

fn slice_ref_is_empty<T>(value: &&[T]) -> bool {
    value.is_empty()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCasePathStep {
    pub key: Value,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_before: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_evidence: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseBranchEvidence {
    pub schema: String,
    pub policy_lane: Value,
    pub trajectory_snapshot: TrajectorySnapshot,
}

impl CombatCase {
    pub fn new(
        source: CombatCaseSource,
        gap: CombatCaseGap,
        run: CombatCaseRunSummary,
        atomic_combat_search_attempts: Vec<AtomicCombatSearchTraceSummaryV2>,
        failed_atomic_combat_search: Option<AtomicCombatSearchTraceSummaryV2>,
        path: Vec<CombatCasePathStep>,
        run_rng: CombatCaseRngSummary,
        position: CombatPosition,
    ) -> Self {
        Self {
            core: CombatCaseCoreV2::new(source, gap, run, run_rng, position),
            branch_evidence: None,
            production_context: None,
            atomic_combat_search_attempts,
            failed_atomic_combat_search,
            path,
        }
    }

    pub fn replay_capability_v2(&self) -> Result<CombatCaseReplayCapabilityV2, String> {
        Ok(self.replay_identity_v2()?.capability)
    }

    pub fn replay_identity_v2(&self) -> Result<CombatCaseReplayIdentityV2, String> {
        combat_case_replay_identity_v2(&self.core, self.production_context.as_ref())
    }

    pub fn clear_production_context(&mut self) {
        self.production_context = None;
    }

    pub fn refresh_derived_summaries_and_clear_production_context(&mut self) {
        self.core.refresh_derived_summaries();
        self.clear_production_context();
    }
}

pub fn load_combat_case(path: &Path) -> Result<CombatCase, String> {
    let payload = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let case: CombatCase = serde_json::from_str(&payload).map_err(|err| err.to_string())?;
    case.core.validate_schema()?;
    Ok(case)
}

pub fn save_combat_case(path: &Path, case: &CombatCase) -> Result<(), String> {
    let payload = serde_json::to_string_pretty(case).map_err(|err| err.to_string())?;
    fs::write(path, payload).map_err(|err| err.to_string())
}

/// Capture the standard replayable production combat-case boundary directly
/// from the exact run session that owns the current combat input.
pub fn capture_production_combat_case_v2(
    session: &RunControlSession,
    source: CombatCaseSource,
    gap: CombatCaseGap,
) -> Result<CombatCase, String> {
    let position = session.current_active_combat_position()?;
    let mut case = CombatCase::new(
        source,
        gap,
        CombatCaseRunSummary {
            act: session.run_state.act_num,
            floor: session.run_state.floor_num,
            hp: session.run_state.current_hp,
            max_hp: session.run_state.max_hp,
            gold: session.run_state.gold,
            deck_size: session.run_state.master_deck.len(),
            relic_count: session.run_state.relics.len(),
            potion_slots: session.run_state.potions.len(),
        },
        Vec::new(),
        None,
        Vec::new(),
        CombatCaseRngSummary::from_pool(&session.run_state.rng_pool),
        position,
    );
    case.production_context = Some(capture_combat_case_production_context_v2(
        &case.core, session,
    )?);
    case.replay_identity_v2()?;
    Ok(case)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::ai::potion_continuation_context_v1::potion_run_continuation_context_v1;
    use crate::ai::potion_continuation_pressure_v1::potion_continuation_pressure_v1;
    use crate::ai::strategy::challenger_signature::DeckBurdenBand;
    use crate::ai::strategy::trajectory_comparison::{
        TrajectoryConstruction, TrajectoryDeployabilityEvidence, TrajectoryPressureEvidence,
        TrajectoryProgress, TrajectoryResources, TrajectorySearchComparability, TrajectorySnapshot,
        TrajectoryTerminal,
    };
    use crate::state::core::EngineState;

    fn sample_snapshot() -> TrajectorySnapshot {
        TrajectorySnapshot {
            lane: "challenger-1".to_string(),
            terminal: TrajectoryTerminal::CoverageLimited,
            progress: TrajectoryProgress { act: 3, floor: 48 },
            pressure: TrajectoryPressureEvidence::Unknown,
            deployability: TrajectoryDeployabilityEvidence::Unknown,
            resources: TrajectoryResources {
                hp: 47,
                max_hp: 81,
                gold: 595,
                potion_count: 2,
            },
            construction: TrajectoryConstruction {
                burden: DeckBurdenBand::Clean,
                completed_commitments: 0,
                active_commitments: 0,
                failed_commitments: 0,
            },
            search_comparability: TrajectorySearchComparability::default(),
            full_search_comparability: TrajectorySearchComparability::default(),
        }
    }

    fn sample_case() -> CombatCase {
        let run = crate::state::run::RunState::new(7, 0, false, "IRONCLAD");
        let position = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            crate::test_support::blank_test_combat(),
        );
        CombatCase::new(
            CombatCaseSource {
                seed: 7,
                ascension: 0,
                generation: 4,
                branch_id: 2,
                parent_id: Some(1),
            },
            CombatCaseGap {
                boundary: "Combat".to_string(),
                reason: "no win".to_string(),
                witness_budget: CombatCaseWitnessBudgetV1::AtomicExactV2 {
                    primary_nodes: 100,
                    primary_wall_ms: 10,
                    rescue_nodes: 200,
                    rescue_wall_ms: 20,
                },
            },
            CombatCaseRunSummary {
                act: 3,
                floor: 48,
                hp: 47,
                max_hp: 81,
                gold: 595,
                deck_size: 14,
                relic_count: 11,
                potion_slots: 3,
            },
            Vec::new(),
            None,
            vec![CombatCasePathStep {
                key: Value::Null,
                label: "Skip card reward".to_string(),
                state_before: None,
                decision_evidence: None,
            }],
            CombatCaseRngSummary::from_pool(&run.rng_pool),
            position,
        )
    }

    fn sample_branch_evidence() -> CombatCaseBranchEvidence {
        CombatCaseBranchEvidence {
            schema: "branch_policy_combat_evidence_v0".to_string(),
            policy_lane: json!({"kind": "challenger", "policy": {"lane_id": 1}}),
            trajectory_snapshot: sample_snapshot(),
        }
    }

    #[test]
    fn isolated_case_without_production_context_remains_replayable() {
        let value = serde_json::to_value(sample_case()).unwrap();
        let mut object = value.as_object().unwrap().clone();
        object.remove("branch_evidence");
        object.remove("production_context");
        for step in object["path"].as_array_mut().unwrap() {
            step.as_object_mut().unwrap().remove("decision_evidence");
        }

        let restored: CombatCase = serde_json::from_value(Value::Object(object)).unwrap();

        assert!(restored.branch_evidence.is_none());
        assert!(restored.production_context.is_none());
        assert_eq!(
            restored.replay_capability_v2().unwrap(),
            CombatCaseReplayCapabilityV2::IsolatedProjection
        );
        assert!(restored
            .path
            .iter()
            .all(|step| step.decision_evidence.is_none()));
    }

    #[test]
    fn branch_and_decision_evidence_round_trip_without_changing_position() {
        let mut case = sample_case();
        let original_position = serde_json::to_value(&case.core.position).unwrap();
        case.branch_evidence = Some(sample_branch_evidence());
        case.path[0].decision_evidence = Some(json!({
            "policy_lane": "challenger-1",
            "candidate_pool": [{"rank": 1, "selected": true}],
            "annotation": {"kind": "candidate"},
            "decision_delta": {"gold_delta": -50},
            "shop_boss_preview_candidates": [{"rank": 1}]
        }));

        let restored: CombatCase =
            serde_json::from_value(serde_json::to_value(&case).unwrap()).unwrap();

        assert_eq!(
            serde_json::to_value(&restored.core.position).unwrap(),
            original_position
        );
        assert_eq!(
            restored.branch_evidence.unwrap().trajectory_snapshot.lane,
            "challenger-1"
        );
        assert_eq!(
            restored.path[0].decision_evidence.as_ref().unwrap()["candidate_pool"][0]["selected"],
            true
        );
    }

    #[test]
    fn production_independent_core_keeps_the_flat_v2_case_schema() {
        let case = sample_case();
        let payload = serde_json::to_value(&case).expect("serialize combat case");

        assert!(payload.get("core").is_none());
        assert_eq!(payload["schema"], COMBAT_CASE_SCHEMA);
        assert_eq!(payload["source"]["seed"], 7);
        assert!(payload.get("position").is_some());

        let core: CombatCaseCoreV2 =
            serde_json::from_value(payload).expect("decode production-independent core");
        core.validate_schema().expect("validate combat case core");
        assert_eq!(core.source.seed, 7);
        assert_eq!(core.gap.boundary, "Combat");
        assert_eq!(
            serde_json::to_value(core.position).unwrap(),
            serde_json::to_value(case.core.position).unwrap()
        );
    }

    #[test]
    fn combat_case_v2_rejects_unknown_top_level_fields() {
        let mut payload = serde_json::to_value(sample_case()).expect("serialize combat case");
        payload["removed_compatibility_field"] = json!(true);

        let error = serde_json::from_value::<CombatCase>(payload)
            .expect_err("combat case V2 must fail closed on unknown fields");
        assert!(
            error
                .to_string()
                .contains("unknown field `removed_compatibility_field`"),
            "{error}"
        );
    }

    #[test]
    fn combat_case_round_trip_preserves_potion_continuation_context() {
        let run = crate::state::run::RunState::new(7, 0, false, "IRONCLAD");
        let combat = crate::test_support::blank_test_combat();
        let continuation = potion_run_continuation_context_v1(&run, &combat);
        let pressure = potion_continuation_pressure_v1(&run, &continuation);
        let attempt = AtomicCombatSearchTraceSummaryV2 {
            potion_continuation_context: Some(continuation),
            potion_continuation_pressure: Some(pressure),
            ..AtomicCombatSearchTraceSummaryV2::default()
        };
        let mut case = sample_case();
        case.atomic_combat_search_attempts = vec![attempt.clone()];
        case.failed_atomic_combat_search = Some(attempt);

        let payload = serde_json::to_value(&case).expect("serialize combat case");
        assert_eq!(
            payload["atomic_combat_search_attempts"][0]["potion_continuation_context"]
                ["capture_boundary"],
            "before_atomic_combat_search"
        );
        assert_eq!(
            payload["atomic_combat_search_attempts"][0]["potion_continuation_pressure"]
                ["capture_boundary"],
            "before_atomic_combat_search"
        );

        let restored: CombatCase =
            serde_json::from_value(payload).expect("deserialize combat case");
        assert!(restored.atomic_combat_search_attempts[0]
            .potion_continuation_context
            .is_some());
        assert!(restored
            .failed_atomic_combat_search
            .as_ref()
            .and_then(|attempt| attempt.potion_continuation_context.as_ref())
            .is_some());
        assert!(restored.atomic_combat_search_attempts[0]
            .potion_continuation_pressure
            .is_some());
        assert!(restored
            .failed_atomic_combat_search
            .as_ref()
            .and_then(|attempt| attempt.potion_continuation_pressure.as_ref())
            .is_some());
    }
}
