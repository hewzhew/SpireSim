//! Same-process ablation of action and boundary-value combat guidance.
//!
//! The command loads one exact combat root and one immutable guidance bundle,
//! then runs four isolated local-graph sessions with identical deterministic
//! allowances.  It is intentionally an audit, not another search owner.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use blake2::{Blake2b512, Digest};
use clap::Args;
use serde::Serialize;
use serde_json::Value;
use sts_combat_planner::{
    CombatDecisionRoot, LocalTurnGraphWitnessReport, LocalTurnGraphWitnessStatus,
    OracleCombatWitnessProgressSnapshot, OracleCombatWitnessSatisfaction, SharedCombatActionPolicy,
};
use sts_oracle_runtime::eval::combat_action_imitation::combat_action_imitation_policy_v1;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::combat_guidance_bundle::{
    combat_value_prototype_policy_v1, CombatGuidanceBundleV1,
};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{CombatPosition, EngineCombatStepper};

use super::canonical_launch::{runtime_identity, runtime_source_content_fingerprint};
use super::combat_graph_execution::LocalGraphExecutionProfile;
use super::combat_graph_search_spec::LocalGraphSearchSpec;

#[derive(Debug, Args)]
pub(super) struct GuidanceCombinationAuditArgs {
    /// Exact combat root used by all four isolated controls.
    #[arg(long)]
    case: PathBuf,
    /// Immutable bundle whose embedded action and value artifacts are ablated.
    #[arg(long)]
    guidance_bundle: PathBuf,
    #[arg(long, default_value_t = 250_000)]
    max_nodes: usize,
    #[arg(long, default_value_t = 1_000_000)]
    max_selections: usize,
    /// Independent wall allowance for each of the four controls.
    #[arg(long, default_value_t = 1_000)]
    wall_ms: u64,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    #[arg(long, default_value_t = 4)]
    generation_quantum_work: usize,
    #[arg(long, default_value_t = 32)]
    max_turn_depth: usize,
    /// Keep guidance comparisons potion-free unless an explicit positive
    /// expenditure budget is part of the experiment.
    #[arg(long, default_value = "0")]
    max_potions_used: Option<u32>,
    /// Fail unless combined guidance finds a replay-verified witness.
    #[arg(long)]
    expect_combined_witness: bool,
    /// Fail unless the combined witness finishes at or above this HP.
    #[arg(long, requires = "expect_combined_witness")]
    expect_combined_min_final_hp: Option<i32>,
    /// Fail unless action-only and value-only both miss while their
    /// combination finds a replay-verified witness.
    #[arg(long)]
    expect_combination_required: bool,
}

#[derive(Clone, Debug, Serialize)]
struct GuidanceBundleIdentity {
    path: PathBuf,
    bytes: u64,
    blake2b_512: String,
    bundle_schema_name: String,
    bundle_schema_version: u32,
    bundle_training_authority: String,
    action_runtime_compatibility_id: String,
    action_source_trajectory_count: usize,
    action_source_action_count: usize,
    value_runtime_compatibility_id: String,
    value_source_trajectory_count: usize,
    value_source_action_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum GuidanceControl {
    Base,
    ActionOnly,
    ValueOnly,
    ActionAndValue,
}

#[derive(Clone, Debug, Serialize)]
struct GuidanceControlResult {
    control: GuidanceControl,
    status: String,
    witness_found: bool,
    final_hp: Option<i32>,
    witness_action_count: Option<usize>,
    elapsed_ms: u128,
    generation_work: usize,
    engine_steps: usize,
    exact_nodes: usize,
    exact_edges: usize,
    completed_turn_options: usize,
    applied_action_transitions: usize,
    max_player_turn: u32,
    retained_states: usize,
    generation_work_per_second: f64,
    applied_action_transitions_per_second: f64,
}

impl GuidanceControlResult {
    fn from_search(
        control: GuidanceControl,
        elapsed: Duration,
        report: &LocalTurnGraphWitnessReport,
        progress: &OracleCombatWitnessProgressSnapshot,
    ) -> Self {
        let seconds = elapsed.as_secs_f64();
        let per_second = |count: usize| {
            if seconds > 0.0 {
                count as f64 / seconds
            } else {
                0.0
            }
        };
        Self {
            control,
            status: format!("{:?}", report.status),
            witness_found: matches!(report.status, LocalTurnGraphWitnessStatus::WitnessFound),
            final_hp: report
                .witness
                .as_ref()
                .map(|witness| witness.final_position.combat.entities.player.current_hp),
            witness_action_count: report.witness.as_ref().map(|witness| witness.actions.len()),
            elapsed_ms: elapsed.as_millis(),
            generation_work: report.counters.generation_work,
            engine_steps: report.counters.engine_steps,
            exact_nodes: report.counters.exact_nodes,
            exact_edges: report.counters.exact_edges,
            completed_turn_options: report.counters.completed_turn_options,
            applied_action_transitions: report.counters.applied_action_transitions,
            max_player_turn: progress.max_player_turn,
            retained_states: progress.retained_states,
            generation_work_per_second: per_second(report.counters.generation_work),
            applied_action_transitions_per_second: per_second(
                report.counters.applied_action_transitions,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct CombinationAssessment {
    combined_witness: bool,
    base_witness: bool,
    action_only_witness: bool,
    value_only_witness: bool,
    combination_required_for_witness: bool,
}

impl CombinationAssessment {
    fn new(results: &[GuidanceControlResult]) -> Self {
        let witness = |control| {
            results
                .iter()
                .find(|result| result.control == control)
                .is_some_and(|result| result.witness_found)
        };
        let base_witness = witness(GuidanceControl::Base);
        let combined_witness = witness(GuidanceControl::ActionAndValue);
        let action_only_witness = witness(GuidanceControl::ActionOnly);
        let value_only_witness = witness(GuidanceControl::ValueOnly);
        Self {
            combined_witness,
            base_witness,
            action_only_witness,
            value_only_witness,
            combination_required_for_witness: combined_witness
                && !base_witness
                && !action_only_witness
                && !value_only_witness,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct GuidanceCombinationAuditReport {
    schema_name: &'static str,
    schema_version: u32,
    case: PathBuf,
    runtime_identity: Value,
    runtime_source_content_fingerprint: String,
    guidance_bundle: GuidanceBundleIdentity,
    search_spec: LocalGraphSearchSpec,
    total_elapsed_ms: u128,
    controls: Vec<GuidanceControlResult>,
    assessment: CombinationAssessment,
}

pub(super) fn run(
    args: GuidanceCombinationAuditArgs,
) -> Result<GuidanceCombinationAuditReport, String> {
    let started = Instant::now();
    let case = args.case.clone();
    let guidance_bundle_path = args.guidance_bundle.clone();
    let loaded = load_combat_case(&case)?;
    let position = loaded.core.position;
    let root_player_turn = position.combat.turn.turn_count;
    let bundle = CombatGuidanceBundleV1::load(&guidance_bundle_path)?;
    let bundle_identity = bundle_identity(&guidance_bundle_path, &bundle)?;
    let runtime_identity = runtime_identity();
    let runtime_source_content_fingerprint = runtime_source_content_fingerprint()?;
    let search_spec = LocalGraphSearchSpec::from_controls(
        args.max_nodes,
        args.max_selections,
        args.wall_ms,
        args.max_engine_steps_per_transition,
        50_000,
        args.generation_quantum_work,
        args.max_turn_depth,
        args.max_potions_used,
        false,
        None,
        None,
        None,
    );
    let execution_profile =
        LocalGraphExecutionProfile::from_controls(false, false, false, false, None)?;

    let controls = vec![
        run_control(
            GuidanceControl::Base,
            &position,
            root_player_turn,
            execution_profile,
            search_spec,
            retained_combat_tactical_policy_v1(),
        )?,
        run_control(
            GuidanceControl::ActionOnly,
            &position,
            root_player_turn,
            execution_profile,
            search_spec,
            combat_action_imitation_policy_v1(
                retained_combat_tactical_policy_v1(),
                bundle.action_imitation.clone(),
            )?,
        )?,
        run_control(
            GuidanceControl::ValueOnly,
            &position,
            root_player_turn,
            execution_profile,
            search_spec,
            combat_value_prototype_policy_v1(
                retained_combat_tactical_policy_v1(),
                &bundle.boundary_value,
            ),
        )?,
        run_control(
            GuidanceControl::ActionAndValue,
            &position,
            root_player_turn,
            execution_profile,
            search_spec,
            bundle.policy(retained_combat_tactical_policy_v1())?,
        )?,
    ];
    let assessment = CombinationAssessment::new(&controls);
    validate_expectations(&args, &controls, assessment)?;

    Ok(GuidanceCombinationAuditReport {
        schema_name: "GuidanceCombinationAuditV2",
        schema_version: 2,
        case,
        runtime_identity,
        runtime_source_content_fingerprint,
        guidance_bundle: bundle_identity,
        search_spec,
        total_elapsed_ms: started.elapsed().as_millis(),
        controls,
        assessment,
    })
}

fn run_control(
    control: GuidanceControl,
    position: &CombatPosition,
    root_player_turn: u32,
    execution_profile: LocalGraphExecutionProfile,
    search_spec: LocalGraphSearchSpec,
    policy: SharedCombatActionPolicy,
) -> Result<GuidanceControlResult, String> {
    let root = CombatDecisionRoot::new(position.clone())
        .map_err(|error| format!("invalid combat case root: {error:?}"))?;
    let config = search_spec.planner_config(OracleCombatWitnessSatisfaction::FirstWitness);
    let mut session = execution_profile.prepare_session(root, root_player_turn, config, policy);
    let started = Instant::now();
    let report = session.advance(search_spec.quantum(), &EngineCombatStepper);
    let elapsed = started.elapsed();
    let progress = session.progress_snapshot();
    Ok(GuidanceControlResult::from_search(
        control, elapsed, &report, &progress,
    ))
}

fn validate_expectations(
    args: &GuidanceCombinationAuditArgs,
    results: &[GuidanceControlResult],
    assessment: CombinationAssessment,
) -> Result<(), String> {
    if args.expect_combined_witness && !assessment.combined_witness {
        return Err("combined action and value guidance did not find a witness".to_string());
    }
    if let Some(minimum) = args.expect_combined_min_final_hp {
        let actual = results
            .iter()
            .find(|result| result.control == GuidanceControl::ActionAndValue)
            .and_then(|result| result.final_hp)
            .ok_or_else(|| "combined guidance has no final HP".to_string())?;
        if actual < minimum {
            return Err(format!(
                "combined guidance final HP {actual} is below required {minimum}"
            ));
        }
    }
    if args.expect_combination_required && !assessment.combination_required_for_witness {
        return Err(format!(
            "combination-required contract failed: combined={}, base={}, action_only={}, value_only={}",
            assessment.combined_witness,
            assessment.base_witness,
            assessment.action_only_witness,
            assessment.value_only_witness,
        ));
    }
    Ok(())
}

fn bundle_identity(
    path: &Path,
    bundle: &CombatGuidanceBundleV1,
) -> Result<GuidanceBundleIdentity, String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "failed to fingerprint guidance bundle '{}': {error}",
            path.display()
        )
    })?;
    let mut digest = Blake2b512::new();
    digest.update(&bytes);
    Ok(GuidanceBundleIdentity {
        path: path.to_path_buf(),
        bytes: bytes.len() as u64,
        blake2b_512: digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        bundle_schema_name: bundle.schema_name.clone(),
        bundle_schema_version: bundle.schema_version,
        bundle_training_authority: bundle.training_authority.clone(),
        action_runtime_compatibility_id: bundle.action_imitation.runtime_compatibility_id.clone(),
        action_source_trajectory_count: bundle.action_imitation.source_trajectory_count,
        action_source_action_count: bundle.action_imitation.source_action_count,
        value_runtime_compatibility_id: bundle.boundary_value.runtime_compatibility_id.clone(),
        value_source_trajectory_count: bundle.boundary_value.source_trajectory_count,
        value_source_action_count: bundle.boundary_value.source_action_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(control: GuidanceControl, witness_found: bool) -> GuidanceControlResult {
        GuidanceControlResult {
            control,
            status: if witness_found {
                "WitnessFound".to_string()
            } else {
                "Partial(Deadline)".to_string()
            },
            witness_found,
            final_hp: witness_found.then_some(13),
            witness_action_count: witness_found.then_some(68),
            elapsed_ms: 1,
            generation_work: 2,
            engine_steps: 3,
            exact_nodes: 4,
            exact_edges: 5,
            completed_turn_options: 6,
            applied_action_transitions: 7,
            max_player_turn: 8,
            retained_states: 9,
            generation_work_per_second: 2.0,
            applied_action_transitions_per_second: 7.0,
        }
    }

    #[test]
    fn assessment_distinguishes_combination_only_capability() {
        let results = vec![
            result(GuidanceControl::Base, false),
            result(GuidanceControl::ActionOnly, false),
            result(GuidanceControl::ValueOnly, false),
            result(GuidanceControl::ActionAndValue, true),
        ];

        assert_eq!(
            CombinationAssessment::new(&results),
            CombinationAssessment {
                combined_witness: true,
                base_witness: false,
                action_only_witness: false,
                value_only_witness: false,
                combination_required_for_witness: true,
            }
        );
    }

    #[test]
    fn standalone_witness_disproves_combination_required_claim() {
        let results = vec![
            result(GuidanceControl::ActionOnly, true),
            result(GuidanceControl::ValueOnly, false),
            result(GuidanceControl::ActionAndValue, true),
        ];

        assert!(!CombinationAssessment::new(&results).combination_required_for_witness);
    }
}
