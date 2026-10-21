use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use sts_combat_planner::{SharedCombatActionPolicy, UniformCombatActionPolicy};
use sts_oracle_runtime::eval::combat_action_imitation::{
    audit_combat_action_imitation_v1,
    train_combat_action_imitation_from_demonstrations_with_base_v1,
    train_combat_action_imitation_v1, CombatActionImitationArtifactV1,
    CombatActionImitationDemonstrationV1, CombatActionImitationTrainingConfigV1,
};
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::combat_guidance_bundle::{
    CombatGuidanceBundleV1, CombatValuePrototypeArtifactV1,
};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;

use super::exact_turn_corridor::{self, ExactTurnCorridor};

pub(super) fn load_value_prototype(path: &Path) -> Result<CombatValuePrototypeArtifactV1, String> {
    CombatValuePrototypeArtifactV1::load(path)
}

pub(super) fn build_value_prototype(
    case: &Path,
    actions: &[PathBuf],
    output: &Path,
    max_engine_steps_per_transition: usize,
) -> Result<Value, String> {
    let corridor = exact_turn_corridor::load(case, actions, max_engine_steps_per_transition)?;
    let artifact = value_prototype_from_corridor(&corridor)?;
    artifact.save(output)?;
    Ok(json!({
        "output": output,
        "artifact": artifact.report(),
    }))
}

pub(super) fn build_value_prototype_corpus(
    manifest: &Path,
    output: &Path,
    max_engine_steps_per_transition: usize,
) -> Result<Value, String> {
    let demonstrations = exact_turn_corridor::load_corpus(manifest)?;
    let ids = demonstrations
        .iter()
        .map(|demonstration| demonstration.id.clone())
        .collect::<Vec<_>>();
    let corridors = demonstrations
        .into_iter()
        .map(|demonstration| {
            exact_turn_corridor::from_position_and_actions(
                demonstration.position,
                demonstration.actions,
                max_engine_steps_per_transition,
            )
            .map_err(|error| format!("demonstration {:?}: {error}", demonstration.id))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let artifact = value_prototype_from_corridors(&corridors)?;
    artifact.save(output)?;
    Ok(json!({
        "output": output,
        "manifest": manifest,
        "demonstration_ids": ids,
        "artifact": artifact.report(),
    }))
}

pub(super) fn build_guidance_bundle(
    action_imitation_artifact: &Path,
    value_prototype_artifact: &Path,
    output: &Path,
) -> Result<Value, String> {
    let action = CombatActionImitationArtifactV1::load(action_imitation_artifact)?;
    let value = CombatValuePrototypeArtifactV1::load(value_prototype_artifact)?;
    let bundle =
        CombatGuidanceBundleV1::new("verified_exact_combat_witness_distillation", action, value)?;
    bundle.save(output)?;
    Ok(json!({
        "output": output,
        "schema_name": bundle.schema_name,
        "schema_version": bundle.schema_version,
        "training_authority": bundle.training_authority,
        "action_source_trajectory_count": bundle.action_imitation.source_trajectory_count,
        "action_source_action_count": bundle.action_imitation.source_action_count,
        "value_source_trajectory_count": bundle.boundary_value.source_trajectory_count,
        "value_source_action_count": bundle.boundary_value.source_action_count,
        "runtime_reads_exact_hashes": false,
        "runtime_reads_witness_actions": false,
    }))
}

pub(super) fn build_action_imitation(
    case: &Path,
    action_paths: &[PathBuf],
    output: &Path,
    max_engine_steps_per_transition: usize,
) -> Result<Value, String> {
    let loaded = load_combat_case(case)?;
    let actions = exact_turn_corridor::load_action_segments(action_paths)?;
    let training_config = CombatActionImitationTrainingConfigV1 {
        max_engine_steps_per_transition,
        ..CombatActionImitationTrainingConfigV1::default()
    };
    let artifact =
        train_combat_action_imitation_v1(&loaded.core.position, &actions, training_config)?;
    let training_audit = audit_combat_action_imitation_v1(
        &loaded.core.position,
        &actions,
        &artifact,
        &UniformCombatActionPolicy,
        training_config.max_structured_alternatives,
        max_engine_steps_per_transition,
    )?;
    artifact.save(output)?;
    Ok(json!({
        "schema_name": "OracleCombatActionImitationBuildV1",
        "schema_version": 1,
        "case": case,
        "output": output,
        "artifact": artifact,
        "training_audit": training_audit,
    }))
}

pub(super) fn build_action_imitation_corpus(
    manifest: &Path,
    output: &Path,
    residual_over_existing_policy: bool,
    max_engine_steps_per_transition: usize,
) -> Result<Value, String> {
    let demonstrations = exact_turn_corridor::load_corpus(manifest)?;
    let training_config = CombatActionImitationTrainingConfigV1 {
        max_engine_steps_per_transition,
        base_weight_exponent: if residual_over_existing_policy {
            1.0
        } else {
            0.0
        },
        ..CombatActionImitationTrainingConfigV1::default()
    };
    let base_policy: SharedCombatActionPolicy = if residual_over_existing_policy {
        retained_combat_tactical_policy_v1()
    } else {
        Arc::new(UniformCombatActionPolicy)
    };
    let borrowed = demonstrations
        .iter()
        .map(|demonstration| CombatActionImitationDemonstrationV1 {
            root: &demonstration.position,
            actions: &demonstration.actions,
        })
        .collect::<Vec<_>>();
    let artifact = train_combat_action_imitation_from_demonstrations_with_base_v1(
        &borrowed,
        training_config,
        base_policy.clone(),
    )?;
    let audits = demonstrations
        .iter()
        .map(|demonstration| {
            audit_combat_action_imitation_v1(
                &demonstration.position,
                &demonstration.actions,
                &artifact,
                base_policy.as_ref(),
                training_config.max_structured_alternatives,
                max_engine_steps_per_transition,
            )
            .map(|audit| {
                json!({
                    "id": demonstration.id,
                    "source_action_count": audit.source_action_count,
                    "ranked_decision_count": audit.ranked_decision_count,
                    "skipped_forced_decision_count": audit.skipped_forced_decision_count,
                    "miss_count": audit.misses.len(),
                })
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    artifact.save(output)?;
    Ok(json!({
        "schema_name": "OracleCombatActionImitationCorpusBuildV1",
        "schema_version": 1,
        "manifest": manifest,
        "output": output,
        "training_base": if residual_over_existing_policy {
            "existing_combat_knowledge_v1"
        } else {
            "uniform"
        },
        "artifact": {
            "schema_name": artifact.schema_name,
            "schema_version": artifact.schema_version,
            "feature_schema": artifact.feature_schema,
            "runtime_compatibility_id": artifact.runtime_compatibility_id,
            "source_trajectory_count": artifact.source_trajectory_count,
            "source_action_count": artifact.source_action_count,
            "ranked_decision_count": artifact.ranked_decision_count,
            "pairwise_comparison_count": artifact.pairwise_comparison_count,
            "skipped_forced_decision_count": artifact.skipped_forced_decision_count,
            "training_top1_correct": artifact.training_top1_correct,
            "training_top1_total": artifact.training_top1_total,
            "coefficient_count": artifact.coefficients.len(),
        },
        "demonstrations": audits,
    }))
}

pub(super) fn audit_action_imitation(
    case: &Path,
    action_paths: &[PathBuf],
    artifact: &Path,
    max_engine_steps_per_transition: usize,
) -> Result<Value, String> {
    let loaded = load_combat_case(case)?;
    let actions = exact_turn_corridor::load_action_segments(action_paths)?;
    let artifact_value = CombatActionImitationArtifactV1::load(artifact)?;
    let base_policy = retained_combat_tactical_policy_v1();
    let audit = audit_combat_action_imitation_v1(
        &loaded.core.position,
        &actions,
        &artifact_value,
        base_policy.as_ref(),
        CombatActionImitationTrainingConfigV1::default().max_structured_alternatives,
        max_engine_steps_per_transition,
    )?;
    Ok(json!({
        "schema_name": "OracleCombatActionImitationAuditV1",
        "schema_version": 1,
        "case": case,
        "artifact": artifact,
        "artifact_source_trajectory_count": artifact_value.source_trajectory_count,
        "audit": audit,
    }))
}

fn value_prototype_from_corridor(
    corridor: &ExactTurnCorridor,
) -> Result<CombatValuePrototypeArtifactV1, String> {
    CombatValuePrototypeArtifactV1::from_ranked_features(
        "exact_terminal_win_demonstration",
        corridor.action_count,
        corridor.terminal_final_hp,
        corridor
            .typed_target_by_turn
            .iter()
            .map(|(player_turn, (value_rank, features))| {
                (*player_turn, *value_rank, features.clone())
            }),
    )
}

fn value_prototype_from_corridors(
    corridors: &[ExactTurnCorridor],
) -> Result<CombatValuePrototypeArtifactV1, String> {
    CombatValuePrototypeArtifactV1::from_ranked_feature_trajectories(
        "exact_terminal_win_demonstration_corpus",
        corridors.iter().map(|corridor| {
            (
                corridor.action_count,
                corridor.terminal_final_hp,
                corridor
                    .typed_target_by_turn
                    .iter()
                    .map(|(player_turn, (value_rank, features))| {
                        (*player_turn, *value_rank, features.clone())
                    })
                    .collect(),
            )
        }),
    )
}
