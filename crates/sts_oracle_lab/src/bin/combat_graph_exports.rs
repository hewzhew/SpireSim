//! Persistence orchestration for selected local combat graph paths.

use std::path::{Path, PathBuf};

use sts_combat_planner::TurnOptionAction;
use sts_oracle_runtime::eval::combat_case::CombatCase;
use sts_oracle_runtime::sim::combat::CombatPosition;
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_evidence_export::{
    export_verified_combat_witness, CombatEvidenceManifestExport, VerifiedCombatWitnessExport,
};
use super::combat_evidence_manifest::CombatEvidenceProducerV1;
use super::combat_replay_tools::export_descendant_combat_case;

pub(super) struct LocalGraphExportPaths<'a> {
    pub(super) witness_actions: Option<&'a Path>,
    pub(super) deepest_survival_case: Option<&'a Path>,
    pub(super) deepest_progress_case: Option<&'a Path>,
}

pub(super) struct LocalGraphExportActions<'a> {
    pub(super) witness: Option<&'a [ClientInput]>,
    pub(super) witness_final_position: Option<&'a CombatPosition>,
    pub(super) deepest_survival: &'a [TurnOptionAction],
    pub(super) deepest_progress: &'a [TurnOptionAction],
}

pub(super) struct LocalGraphExports {
    pub(super) witness_actions: Option<PathBuf>,
    pub(super) witness_manifest: Option<PathBuf>,
    pub(super) deepest_survival_case: Option<PathBuf>,
    pub(super) deepest_survival_actions: Option<PathBuf>,
    pub(super) deepest_progress_case: Option<PathBuf>,
    pub(super) deepest_progress_actions: Option<PathBuf>,
}

pub(super) fn export_local_graph_paths(
    base: &CombatCase,
    witness_manifest_case: Option<&Path>,
    paths: LocalGraphExportPaths<'_>,
    actions: LocalGraphExportActions<'_>,
    max_engine_steps_per_transition: usize,
) -> Result<LocalGraphExports, String> {
    let (witness_actions, witness_manifest) = match (
        paths.witness_actions,
        actions.witness,
        actions.witness_final_position,
    ) {
        (Some(path), Some(actions), Some(expected_final_position)) => {
            let manifest = export_verified_combat_witness(VerifiedCombatWitnessExport {
                root_position: &base.core.position,
                action_output: path,
                actions,
                expected_final_position,
                max_engine_steps_per_transition,
                manifest: witness_manifest_case.map(|case_path| CombatEvidenceManifestExport {
                    producer: CombatEvidenceProducerV1::LocalGraphSearch,
                    case_path,
                    evidence_id: "local_graph_complete_win",
                }),
            })?;
            (Some(path.to_path_buf()), manifest)
        }
        (Some(_), Some(_), None) => {
            return Err("local-graph witness export is missing its final position".to_string())
        }
        _ => (None, None),
    };
    let (deepest_survival_case, deepest_survival_actions) =
        if let Some(path) = paths.deepest_survival_case {
            let actions_path = export_descendant_combat_case(
                base,
                actions.deepest_survival,
                path,
                max_engine_steps_per_transition,
                "local_turn_graph_deepest_survival",
            )?;
            (Some(path.to_path_buf()), Some(actions_path))
        } else {
            (None, None)
        };
    let (deepest_progress_case, deepest_progress_actions) =
        if let Some(path) = paths.deepest_progress_case {
            let actions_path = export_descendant_combat_case(
                base,
                actions.deepest_progress,
                path,
                max_engine_steps_per_transition,
                "local_turn_graph_deepest_progress",
            )?;
            (Some(path.to_path_buf()), Some(actions_path))
        } else {
            (None, None)
        };

    Ok(LocalGraphExports {
        witness_actions,
        witness_manifest,
        deepest_survival_case,
        deepest_survival_actions,
        deepest_progress_case,
        deepest_progress_actions,
    })
}

#[cfg(test)]
mod tests;
