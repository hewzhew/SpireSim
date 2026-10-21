use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::{
    run_combat_mechanism_horizon_probe_v1, CombatMechanismHorizonProbeConfigV1,
    CombatMechanismHorizonProbeReportV1,
};
use crate::runtime::rng::RngPool;
use crate::sim::combat_start::build_natural_combat_start;
use crate::state::run::RunState;

use super::{
    run_strategic_encounter_probes_v1, RunControlSession, RunDecisionAction,
    StrategicEncounterPrimaryEvidenceV1, StrategicEncounterProbeBudgetV1,
    StrategicEncounterProbeHpBasisV1, StrategicEncounterProbeObservationV1,
    StrategicEncounterProbePotionUseV1, StrategicEncounterProbeSpecV1,
};

pub const STRATEGIC_PROBE_CALIBRATION_SCHEMA_NAME: &str = "StrategicProbeCalibrationReport";
pub const STRATEGIC_PROBE_CALIBRATION_SCHEMA_VERSION: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct StrategicProbeFidelityV1 {
    pub max_nodes: usize,
    pub wall_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct StrategicProbeShadowFidelityV1 {
    pub horizon_turns: u32,
    pub max_active_states_per_depth: usize,
    pub max_inner_nodes_per_turn: usize,
    pub max_end_states_per_turn: usize,
    pub per_bucket_limit: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicProbeCalibrationReportV1 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub authority: &'static str,
    pub paired_sampling: &'static str,
    pub act: u8,
    pub floor: i32,
    pub hp_basis: StrategicEncounterProbeHpBasisV1,
    pub fidelities: Vec<StrategicProbeFidelityV1>,
    pub shadow_fidelities: Vec<StrategicProbeShadowFidelityV1>,
    pub observations: Vec<StrategicProbeCalibrationObservationV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicProbeCalibrationObservationV1 {
    pub probe_id: &'static str,
    pub probe_seed: u64,
    pub shadows: Vec<StrategicProbeShadowObservationV1>,
    pub fidelity_observations: Vec<StrategicEncounterProbeObservationV1>,
    pub first_exact_witness_fidelity_index: Option<usize>,
    pub fidelity_consistency: StrategicProbeFidelityConsistencyV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicProbeFidelityConsistencyV1 {
    pub primary_evidence_kinds: Vec<&'static str>,
    pub primary_evidence_changed: bool,
    pub exact_witness_final_hp_min: Option<i32>,
    pub exact_witness_final_hp_max: Option<i32>,
    pub exact_witness_hp_loss_min: Option<i32>,
    pub exact_witness_hp_loss_max: Option<i32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StrategicProbeShadowObservationV1 {
    HeuristicEstimate {
        fidelity: StrategicProbeShadowFidelityV1,
        endpoint_envelope: CombatMechanismHorizonProbeReportV1,
    },
    SetupError {
        fidelity: StrategicProbeShadowFidelityV1,
        message: String,
    },
}

/// A structured, lexicographic scheduling hint extracted from one bounded
/// finite-horizon endpoint surface.  Higher is preferred.  It is not a value
/// estimate, win probability, pruning bound, or owner score.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct StrategicProbeShadowOrderKeyV1 {
    pub terminal_win_seen: bool,
    pub non_loss_endpoint_seen: bool,
    pub living_enemy_delta: i32,
    pub total_enemy_hp_delta: i32,
    pub survival_margin: i32,
    pub pollution_avoidance: i32,
    pub depth_turns: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicProbeCalibrationPartitionV1 {
    Development,
    HeldOut,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicProbeResolvedLabelV1 {
    ExactWitness,
    ExhaustiveRefutation,
    BudgetUnknown,
    SetupError,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicProbeOrderingCalibrationCaseV1 {
    pub case_id: String,
    pub seed_group: String,
    pub partition: StrategicProbeCalibrationPartitionV1,
    pub shadow_order_key: Option<StrategicProbeShadowOrderKeyV1>,
    pub exact_label: StrategicProbeResolvedLabelV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicProbeSchedulingAuthorityV1 {
    /// No run scheduling authority is granted until a concrete ordering rule
    /// has been evaluated on disjoint held-out cases.  Merely emitting a
    /// finite-horizon endpoint surface is not such a calibration.
    WithheldPendingHeldOutCalibration,
    /// The hint may only decide which already-legal expensive edge receives
    /// exact validation first.  It may not reorder owner candidates, prune a
    /// candidate, materialize a successor, or change primary evidence.
    ShadowCombatEdgeOrderingOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicProbeOwnerAuthorityV1 {
    NotGranted,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicProbeHeldOutOrderingValidationV1 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub scheduling_authority: StrategicProbeSchedulingAuthorityV1,
    pub owner_authority: StrategicProbeOwnerAuthorityV1,
    pub development_seed_groups: usize,
    pub held_out_seed_groups: usize,
    pub held_out_cases: usize,
    pub held_out_exact_witnesses: usize,
    pub held_out_exhaustive_refutations: usize,
    pub held_out_budget_unknown: usize,
    pub held_out_setup_errors: usize,
    pub informative_pairs: usize,
    pub concordant_pairs: usize,
    pub discordant_pairs: usize,
    pub tied_pairs: usize,
    pub unscored_pairs: usize,
}

/// Extracts one deterministic scheduling key without collapsing the endpoint
/// report itself.  The selected endpoint and the exact transition surface
/// remain available to calibration and audit callers.
pub fn strategic_probe_shadow_order_key_v1(
    report: &CombatMechanismHorizonProbeReportV1,
) -> Option<StrategicProbeShadowOrderKeyV1> {
    report
        .depths
        .iter()
        .flat_map(|depth| {
            depth
                .endpoints
                .iter()
                .map(move |endpoint| StrategicProbeShadowOrderKeyV1 {
                    terminal_win_seen: endpoint.state.terminal
                        == crate::ai::combat_search_v2::SearchTerminalLabel::Win,
                    non_loss_endpoint_seen: endpoint.state.terminal
                        != crate::ai::combat_search_v2::SearchTerminalLabel::Loss,
                    living_enemy_delta: endpoint.living_enemy_delta,
                    total_enemy_hp_delta: endpoint.total_enemy_hp_delta,
                    survival_margin: endpoint
                        .state
                        .player_hp
                        .saturating_sub(endpoint.state.visible_incoming_damage),
                    pollution_avoidance: endpoint.status_or_curse_delta.saturating_neg(),
                    depth_turns: depth.depth_turns,
                })
        })
        .max()
}

/// Cheap oracle-only adapter for an *already legal* noncombat candidate.  The
/// candidate is applied to a cloned session.  Only when that exact transition
/// opens a combat does the bounded mechanism surface produce a scheduling
/// hint.  The real session, candidate ranking, and successor graph are not
/// changed.
pub fn strategic_combat_edge_shadow_order_v1(
    session: &RunControlSession,
    candidate_id: &str,
    action: &RunDecisionAction,
) -> Option<StrategicProbeShadowOrderKeyV1> {
    let mut child = session.clone();
    let outcome = child
        .apply_owner_candidate(candidate_id, action.clone())
        .ok()?;
    if outcome.progress_steps.len() != 1 {
        return None;
    }
    let active = child.active_combat.as_ref()?;
    let report = run_combat_mechanism_horizon_probe_v1(
        &active.engine_state,
        &active.combat_state,
        CombatMechanismHorizonProbeConfigV1 {
            horizon_turns: 1,
            max_active_states_per_depth: 12,
            max_inner_nodes_per_turn: 64,
            max_end_states_per_turn: 8,
            per_bucket_limit: 2,
            max_engine_steps_per_action: 250,
        },
    );
    strategic_probe_shadow_order_key_v1(&report)
}

pub fn strategic_probe_resolved_label_v1(
    evidence: &StrategicEncounterPrimaryEvidenceV1,
) -> StrategicProbeResolvedLabelV1 {
    match evidence {
        StrategicEncounterPrimaryEvidenceV1::ExactWitness { .. } => {
            StrategicProbeResolvedLabelV1::ExactWitness
        }
        StrategicEncounterPrimaryEvidenceV1::ExhaustiveRefutation => {
            StrategicProbeResolvedLabelV1::ExhaustiveRefutation
        }
        StrategicEncounterPrimaryEvidenceV1::BudgetUnknown => {
            StrategicProbeResolvedLabelV1::BudgetUnknown
        }
        StrategicEncounterPrimaryEvidenceV1::SetupError { .. } => {
            StrategicProbeResolvedLabelV1::SetupError
        }
    }
}

/// Measures only the ordering question a shadow hint might eventually be
/// allowed to answer. `BudgetUnknown` is counted but never treated as a
/// negative label.
///
/// This function deliberately does not promote the hint. Its inputs are a
/// caller-provided audit surface, so pair counts alone cannot prove that the
/// cases came from immutable exact reports, were selected independently, or
/// represent the production distribution. Promotion remains withheld until a
/// reviewed, reproducible held-out corpus supplies that missing provenance.
pub fn validate_strategic_probe_shadow_ordering_v1(
    cases: &[StrategicProbeOrderingCalibrationCaseV1],
) -> Result<StrategicProbeHeldOutOrderingValidationV1, String> {
    let mut partitions_by_group =
        BTreeMap::<&str, BTreeSet<StrategicProbeCalibrationPartitionV1>>::new();
    for case in cases {
        partitions_by_group
            .entry(case.seed_group.as_str())
            .or_default()
            .insert(case.partition);
    }
    if let Some((group, _)) = partitions_by_group
        .iter()
        .find(|(_, partitions)| partitions.len() > 1)
    {
        return Err(format!(
            "strategic probe seed group '{group}' appears in both development and held-out partitions"
        ));
    }

    let development_seed_groups = partitions_by_group
        .values()
        .filter(|partitions| {
            partitions.contains(&StrategicProbeCalibrationPartitionV1::Development)
        })
        .count();
    let held_out_seed_groups = partitions_by_group
        .values()
        .filter(|partitions| partitions.contains(&StrategicProbeCalibrationPartitionV1::HeldOut))
        .count();
    let held_out = cases
        .iter()
        .filter(|case| case.partition == StrategicProbeCalibrationPartitionV1::HeldOut)
        .collect::<Vec<_>>();
    let count = |label| {
        held_out
            .iter()
            .filter(|case| case.exact_label == label)
            .count()
    };
    let witnesses = held_out
        .iter()
        .filter(|case| case.exact_label == StrategicProbeResolvedLabelV1::ExactWitness)
        .collect::<Vec<_>>();
    let refutations = held_out
        .iter()
        .filter(|case| case.exact_label == StrategicProbeResolvedLabelV1::ExhaustiveRefutation)
        .collect::<Vec<_>>();
    let mut concordant_pairs = 0usize;
    let mut discordant_pairs = 0usize;
    let mut tied_pairs = 0usize;
    let mut unscored_pairs = 0usize;
    for witness in &witnesses {
        for refutation in &refutations {
            match (witness.shadow_order_key, refutation.shadow_order_key) {
                (Some(left), Some(right)) if left > right => {
                    concordant_pairs = concordant_pairs.saturating_add(1)
                }
                (Some(left), Some(right)) if left < right => {
                    discordant_pairs = discordant_pairs.saturating_add(1)
                }
                (Some(_), Some(_)) => tied_pairs = tied_pairs.saturating_add(1),
                _ => unscored_pairs = unscored_pairs.saturating_add(1),
            }
        }
    }
    Ok(StrategicProbeHeldOutOrderingValidationV1 {
        schema_name: "StrategicProbeHeldOutOrderingValidation",
        schema_version: 1,
        scheduling_authority:
            StrategicProbeSchedulingAuthorityV1::WithheldPendingHeldOutCalibration,
        owner_authority: StrategicProbeOwnerAuthorityV1::NotGranted,
        development_seed_groups,
        held_out_seed_groups,
        held_out_cases: held_out.len(),
        held_out_exact_witnesses: witnesses.len(),
        held_out_exhaustive_refutations: refutations.len(),
        held_out_budget_unknown: count(StrategicProbeResolvedLabelV1::BudgetUnknown),
        held_out_setup_errors: count(StrategicProbeResolvedLabelV1::SetupError),
        informative_pairs: witnesses.len().saturating_mul(refutations.len()),
        concordant_pairs,
        discordant_pairs,
        tied_pairs,
        unscored_pairs,
    })
}

/// Runs paired low/high fidelity samples from the same exact counterfactual
/// start. This report estimates neither win probability nor a safe elimination
/// bound; it exists to measure whether a cheap shadow signal is calibrated
/// enough to order later expensive evaluations.
pub fn run_strategic_probe_calibration_v1(
    run_state: &RunState,
    probes: &[StrategicEncounterProbeSpecV1],
    hp_basis: StrategicEncounterProbeHpBasisV1,
    fidelities: &[StrategicProbeFidelityV1],
    shadow_fidelities: &[StrategicProbeShadowFidelityV1],
) -> Result<StrategicProbeCalibrationReportV1, String> {
    if fidelities.is_empty() {
        return Err("strategic probe calibration requires at least one fidelity".to_string());
    }
    if fidelities
        .windows(2)
        .any(|pair| pair[1].max_nodes < pair[0].max_nodes || pair[1].wall_ms < pair[0].wall_ms)
    {
        return Err("strategic probe fidelities must be nondecreasing".to_string());
    }
    if shadow_fidelities.is_empty() {
        return Err(
            "strategic probe calibration requires at least one shadow fidelity".to_string(),
        );
    }

    let observations = probes
        .iter()
        .map(|probe| {
            let shadows = shadow_fidelities
                .iter()
                .copied()
                .map(|fidelity| run_shadow(run_state, probe, hp_basis, fidelity))
                .collect();
            let fidelity_observations = fidelities
                .iter()
                .map(|fidelity| {
                    let report = run_strategic_encounter_probes_v1(
                        run_state,
                        std::slice::from_ref(probe),
                        StrategicEncounterProbeBudgetV1 {
                            max_nodes_per_encounter: fidelity.max_nodes,
                            wall_ms_per_encounter: fidelity.wall_ms,
                            hp_basis,
                            potion_use: StrategicEncounterProbePotionUseV1::Disabled,
                        },
                    );
                    report
                        .observations
                        .into_iter()
                        .next()
                        .expect("one requested calibration probe")
                })
                .collect::<Vec<_>>();
            let first_exact_witness_fidelity_index =
                fidelity_observations.iter().position(|observation| {
                    matches!(
                        observation.primary_evidence,
                        StrategicEncounterPrimaryEvidenceV1::ExactWitness { .. }
                    )
                });
            let fidelity_consistency = fidelity_consistency(&fidelity_observations);
            StrategicProbeCalibrationObservationV1 {
                probe_id: probe.probe_id,
                probe_seed: probe.probe_seed,
                shadows,
                fidelity_observations,
                first_exact_witness_fidelity_index,
                fidelity_consistency,
            }
        })
        .collect();

    Ok(StrategicProbeCalibrationReportV1 {
        schema_name: STRATEGIC_PROBE_CALIBRATION_SCHEMA_NAME,
        schema_version: STRATEGIC_PROBE_CALIBRATION_SCHEMA_VERSION,
        authority: "offline_calibration_only_no_candidate_elimination_or_owner_authority",
        paired_sampling: "same_run_state_encounter_and_diagnostic_rng_restarted_per_fidelity",
        act: run_state.act_num,
        floor: run_state.floor_num,
        hp_basis,
        fidelities: fidelities.to_vec(),
        shadow_fidelities: shadow_fidelities.to_vec(),
        observations,
    })
}

fn fidelity_consistency(
    observations: &[StrategicEncounterProbeObservationV1],
) -> StrategicProbeFidelityConsistencyV1 {
    let primary_evidence_kinds = observations
        .iter()
        .map(|observation| primary_evidence_kind(&observation.primary_evidence))
        .collect::<Vec<_>>();
    let primary_evidence_changed = primary_evidence_kinds
        .windows(2)
        .any(|pair| pair[0] != pair[1]);
    let mut final_hp_min = None;
    let mut final_hp_max = None;
    let mut hp_loss_min = None;
    let mut hp_loss_max = None;
    for observation in observations {
        let StrategicEncounterPrimaryEvidenceV1::ExactWitness { witness } =
            &observation.primary_evidence
        else {
            continue;
        };
        final_hp_min =
            Some(final_hp_min.map_or(witness.final_hp, |value: i32| value.min(witness.final_hp)));
        final_hp_max =
            Some(final_hp_max.map_or(witness.final_hp, |value: i32| value.max(witness.final_hp)));
        hp_loss_min =
            Some(hp_loss_min.map_or(witness.hp_loss, |value: i32| value.min(witness.hp_loss)));
        hp_loss_max =
            Some(hp_loss_max.map_or(witness.hp_loss, |value: i32| value.max(witness.hp_loss)));
    }
    StrategicProbeFidelityConsistencyV1 {
        primary_evidence_kinds,
        primary_evidence_changed,
        exact_witness_final_hp_min: final_hp_min,
        exact_witness_final_hp_max: final_hp_max,
        exact_witness_hp_loss_min: hp_loss_min,
        exact_witness_hp_loss_max: hp_loss_max,
    }
}

fn primary_evidence_kind(evidence: &StrategicEncounterPrimaryEvidenceV1) -> &'static str {
    match evidence {
        StrategicEncounterPrimaryEvidenceV1::ExactWitness { .. } => "exact_witness",
        StrategicEncounterPrimaryEvidenceV1::ExhaustiveRefutation => "exhaustive_refutation",
        StrategicEncounterPrimaryEvidenceV1::BudgetUnknown => "budget_unknown",
        StrategicEncounterPrimaryEvidenceV1::SetupError { .. } => "setup_error",
    }
}

fn run_shadow(
    run_state: &RunState,
    probe: &StrategicEncounterProbeSpecV1,
    hp_basis: StrategicEncounterProbeHpBasisV1,
    fidelity: StrategicProbeShadowFidelityV1,
) -> StrategicProbeShadowObservationV1 {
    let mut probe_run = run_state.clone();
    probe_run.rng_pool = RngPool::new(probe.probe_seed);
    if hp_basis == StrategicEncounterProbeHpBasisV1::Full {
        probe_run.current_hp = probe_run.max_hp;
    }
    match build_natural_combat_start(&mut probe_run, probe.encounter, probe.room_type) {
        Ok((engine, combat)) => StrategicProbeShadowObservationV1::HeuristicEstimate {
            fidelity,
            endpoint_envelope: run_combat_mechanism_horizon_probe_v1(
                &engine,
                &combat,
                CombatMechanismHorizonProbeConfigV1 {
                    horizon_turns: fidelity.horizon_turns,
                    max_active_states_per_depth: fidelity.max_active_states_per_depth,
                    max_inner_nodes_per_turn: fidelity.max_inner_nodes_per_turn,
                    max_end_states_per_turn: fidelity.max_end_states_per_turn,
                    per_bucket_limit: fidelity.per_bucket_limit,
                    max_engine_steps_per_action: 250,
                },
            ),
        },
        Err(message) => StrategicProbeShadowObservationV1::SetupError { fidelity, message },
    }
}
