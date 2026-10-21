use serde::Serialize;

use crate::ai::combat_search_v2::{
    run_combat_mechanism_horizon_probe_v1, CombatMechanismHorizonProbeConfigV1,
    CombatMechanismHorizonProbeReportV1,
};
use crate::ai::noncombat_strategy_v1::{
    build_run_strategy_snapshot_from_run_state_v2, StrategyCapabilityCoverageV1,
    StrategyCapabilityKindV1,
};
use crate::content::monsters::factory::EncounterId;
use crate::runtime::rng::RngPool;
use crate::sim::combat_start::build_natural_combat_start;
use crate::state::map::node::RoomType;
use crate::state::run::RunState;

use super::StrategicCapabilityPredictionV1;

pub const STRATEGIC_MECHANISM_PROBE_SCHEMA_NAME: &str = "StrategicMechanismProbeReport";
pub const STRATEGIC_MECHANISM_PROBE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicMechanismKindV1 {
    OpeningFrontload,
    MultiTargetOpening,
    StatusPollutionRecovery,
    SetupSpeed,
    SustainedDefense,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StrategicMechanismProbeSpecV1 {
    pub probe_id: &'static str,
    pub mechanism: StrategicMechanismKindV1,
    pub encounter: EncounterId,
    pub room_type: RoomType,
    pub probe_seed: u64,
    pub horizon_turns: u32,
    pub capabilities_under_test: Vec<StrategyCapabilityKindV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicMechanismProbeReportV1 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub information_boundary: &'static str,
    pub authority: &'static str,
    pub normalized_hp: &'static str,
    pub act: u8,
    pub floor: i32,
    pub observations: Vec<StrategicMechanismProbeObservationV1>,
    pub unsupported_questions: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicMechanismProbeObservationV1 {
    pub probe_id: &'static str,
    pub mechanism: StrategicMechanismKindV1,
    pub encounter: EncounterId,
    pub probe_seed: u64,
    pub capabilities_under_test: Vec<StrategicCapabilityPredictionV1>,
    pub outcome: StrategicMechanismProbeOutcomeV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StrategicMechanismProbeOutcomeV1 {
    /// Simulator transitions and endpoint hashes are exact, but the bounded
    /// endpoint surface is only a heuristic estimate of the named strategic
    /// capability and never a whole-combat verdict.
    HeuristicEstimate {
        endpoint_envelope: CombatMechanismHorizonProbeReportV1,
    },
    SetupError {
        message: String,
    },
}

/// A compact, act-independent battery. These are finite-horizon mechanism
/// questions, not whole-encounter win tests. Phase-burst is intentionally not
/// included until a controlled pre-transition combat fixture exists.
pub fn strategic_mechanism_probe_plan_v1() -> Vec<StrategicMechanismProbeSpecV1> {
    use StrategyCapabilityKindV1 as Capability;

    vec![
        mechanism_spec(
            "mechanism_opening_frontload",
            StrategicMechanismKindV1::OpeningFrontload,
            EncounterId::GremlinNob,
            RoomType::MonsterRoomElite,
            0xC0_001,
            2,
            &[Capability::SingleTargetFrontload],
        ),
        mechanism_spec(
            "mechanism_multitarget_opening",
            StrategicMechanismKindV1::MultiTargetOpening,
            EncounterId::ThreeByrds,
            RoomType::MonsterRoom,
            0xC0_002,
            2,
            &[Capability::MultiTargetControl],
        ),
        mechanism_spec(
            "mechanism_status_pollution_recovery",
            StrategicMechanismKindV1::StatusPollutionRecovery,
            EncounterId::ThreeSentries,
            RoomType::MonsterRoomElite,
            0xC0_003,
            3,
            &[
                Capability::ArtifactStripAccess,
                Capability::DrawEnergyConsistency,
            ],
        ),
        mechanism_spec(
            "mechanism_setup_speed",
            StrategicMechanismKindV1::SetupSpeed,
            EncounterId::Lagavulin,
            RoomType::MonsterRoomElite,
            0xC0_004,
            3,
            &[Capability::LongFightScaling],
        ),
        mechanism_spec(
            "mechanism_sustained_defense",
            StrategicMechanismKindV1::SustainedDefense,
            EncounterId::BookOfStabbing,
            RoomType::MonsterRoomElite,
            0xC0_005,
            3,
            &[Capability::SustainedDefense],
        ),
    ]
}

pub fn run_strategic_mechanism_probes_v1(
    run_state: &RunState,
    probes: &[StrategicMechanismProbeSpecV1],
) -> StrategicMechanismProbeReportV1 {
    let strategy = build_run_strategy_snapshot_from_run_state_v2(run_state);
    let observations = probes
        .iter()
        .map(|probe| {
            let capabilities_under_test = probe
                .capabilities_under_test
                .iter()
                .map(|capability| StrategicCapabilityPredictionV1 {
                    capability: *capability,
                    predicted_coverage: strategy
                        .threat_coverage
                        .capability(*capability)
                        .map(|evidence| evidence.coverage)
                        .unwrap_or(StrategyCapabilityCoverageV1::Unknown),
                })
                .collect();
            run_one_mechanism_probe(run_state, probe, capabilities_under_test)
        })
        .collect();

    StrategicMechanismProbeReportV1 {
        schema_name: STRATEGIC_MECHANISM_PROBE_SCHEMA_NAME,
        schema_version: STRATEGIC_MECHANISM_PROBE_SCHEMA_VERSION,
        information_boundary: "offline_fixed_rng_public_encounters_no_real_future_read",
        authority: "calibration_only_no_owner_action_candidate_elimination_or_successor_authority",
        normalized_hp: "current_hp_set_to_current_max_hp_to_remove_arrival_hp_debt_only",
        act: run_state.act_num,
        floor: run_state.floor_num,
        observations,
        unsupported_questions: vec![
            "phase_burst_requires_a_controlled_pre_transition_combat_fixture",
            "finite_horizon_endpoint_envelopes_do_not_estimate_whole_encounter_win_probability",
        ],
    }
}

fn run_one_mechanism_probe(
    run_state: &RunState,
    probe: &StrategicMechanismProbeSpecV1,
    capabilities_under_test: Vec<StrategicCapabilityPredictionV1>,
) -> StrategicMechanismProbeObservationV1 {
    let mut probe_run = run_state.clone();
    probe_run.current_hp = probe_run.max_hp;
    probe_run.rng_pool = RngPool::new(probe.probe_seed);
    let outcome = match build_natural_combat_start(&mut probe_run, probe.encounter, probe.room_type)
    {
        Ok((engine, combat)) => StrategicMechanismProbeOutcomeV1::HeuristicEstimate {
            endpoint_envelope: run_combat_mechanism_horizon_probe_v1(
                &engine,
                &combat,
                CombatMechanismHorizonProbeConfigV1 {
                    horizon_turns: probe.horizon_turns,
                    max_active_states_per_depth: 48,
                    max_inner_nodes_per_turn: 256,
                    max_end_states_per_turn: 16,
                    per_bucket_limit: 4,
                    max_engine_steps_per_action: 250,
                },
            ),
        },
        Err(message) => StrategicMechanismProbeOutcomeV1::SetupError { message },
    };

    StrategicMechanismProbeObservationV1 {
        probe_id: probe.probe_id,
        mechanism: probe.mechanism,
        encounter: probe.encounter,
        probe_seed: probe.probe_seed,
        capabilities_under_test,
        outcome,
    }
}

fn mechanism_spec(
    probe_id: &'static str,
    mechanism: StrategicMechanismKindV1,
    encounter: EncounterId,
    room_type: RoomType,
    probe_seed: u64,
    horizon_turns: u32,
    capabilities: &[StrategyCapabilityKindV1],
) -> StrategicMechanismProbeSpecV1 {
    StrategicMechanismProbeSpecV1 {
        probe_id,
        mechanism,
        encounter,
        room_type,
        probe_seed,
        horizon_turns,
        capabilities_under_test: capabilities.to_vec(),
    }
}
