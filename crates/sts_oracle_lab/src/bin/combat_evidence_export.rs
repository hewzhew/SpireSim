//! Exact replay gate shared by complete combat-witness exporters.

use std::path::{Path, PathBuf};

use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_evidence_manifest::{
    combat_evidence_manifest_path_for_actions, write_combat_evidence_manifest,
    CombatEvidenceManifestEntryV1, CombatEvidenceProducerV1,
};
use super::combat_replay_tools::{replay_combat_inputs, save_combat_inputs};

pub(super) struct CombatEvidenceManifestExport<'a> {
    pub(super) producer: CombatEvidenceProducerV1,
    pub(super) case_path: &'a Path,
    pub(super) evidence_id: &'a str,
}

pub(super) struct VerifiedCombatWitnessExport<'a> {
    pub(super) root_position: &'a CombatPosition,
    pub(super) action_output: &'a Path,
    pub(super) actions: &'a [ClientInput],
    pub(super) expected_final_position: &'a CombatPosition,
    pub(super) max_engine_steps_per_transition: usize,
    /// Omit only when the search root is an undeclared counterfactual. The
    /// action list is still replay-verified, but it must not claim the
    /// caller-supplied case as its provenance root.
    pub(super) manifest: Option<CombatEvidenceManifestExport<'a>>,
}

pub(super) fn export_verified_combat_witness(
    request: VerifiedCombatWitnessExport<'_>,
) -> Result<Option<PathBuf>, String> {
    let replayed = replay_combat_inputs(
        request.root_position.clone(),
        request.actions,
        request.max_engine_steps_per_transition,
    )?;
    if EngineCombatStepper.terminal(&replayed) != CombatTerminal::Win {
        return Err("exported combat witness did not replay to a win".to_string());
    }
    let replayed_exact_state_hash = combat_exact_state_hash_v2(&replayed.engine, &replayed.combat);
    let expected_exact_state_hash = combat_exact_state_hash_v2(
        &request.expected_final_position.engine,
        &request.expected_final_position.combat,
    );
    if replayed_exact_state_hash != expected_exact_state_hash {
        return Err("exported combat witness replay did not match the search witness".to_string());
    }

    let manifest_output = request
        .manifest
        .as_ref()
        .map(|_| combat_evidence_manifest_path_for_actions(request.action_output));
    save_combat_inputs(request.action_output, request.actions.iter().cloned())?;
    if let (Some(spec), Some(path)) = (request.manifest, manifest_output.as_ref()) {
        write_combat_evidence_manifest(
            path,
            spec.producer,
            combat_exact_state_hash_v2(
                &request.root_position.engine,
                &request.root_position.combat,
            ),
            spec.case_path.to_path_buf(),
            vec![CombatEvidenceManifestEntryV1::from_actions(
                spec.evidence_id.to_string(),
                vec![request.action_output.to_path_buf()],
                request.actions,
                CombatTerminal::Win,
                Some(replayed.combat.entities.player.current_hp),
            )?],
        )?;
    }
    Ok(manifest_output)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use sts_combat_planner::CombatDecisionRoot;
    use sts_oracle_runtime::eval::combat_case::load_combat_case;

    use super::*;
    use crate::combat_evidence_manifest::decode_combat_evidence_manifest;

    const MAX_ENGINE_STEPS_PER_TRANSITION: usize = 10_000;

    fn fixture_path(file_name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/oracle_witnesses")
            .join(file_name)
    }

    fn temp_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-artifacts")
            .join(format!(
                "sts-oracle-combat-evidence-export-{label}-{}-{nonce}",
                std::process::id()
            ))
    }

    fn fixture() -> (PathBuf, CombatPosition, Vec<ClientInput>, CombatPosition) {
        let case_path =
            fixture_path("seed20260713008_a0_body_slam_fiend_fire_donu_deca.combat-case.json");
        let actions_path = fixture_path(
            "seed20260713008_a0_body_slam_fiend_fire_donu_deca.policy-discrepancy.actions.json",
        );
        let case = load_combat_case(&case_path).expect("fixture case should load");
        let actions = serde_json::from_slice::<Vec<ClientInput>>(
            &fs::read(&actions_path).expect("fixture actions should load"),
        )
        .expect("fixture actions should decode");
        let final_position = replay_combat_inputs(
            case.core.position.clone(),
            &actions,
            MAX_ENGINE_STEPS_PER_TRANSITION,
        )
        .expect("fixture actions should replay");
        assert_eq!(
            EngineCombatStepper.terminal(&final_position),
            CombatTerminal::Win
        );
        (case_path, case.core.position, actions, final_position)
    }

    #[test]
    fn verified_witness_export_carries_original_root_identity() {
        let (case_path, root_position, actions, final_position) = fixture();
        let directory = temp_directory("manifest");
        let action_output = directory.join("complete-win.actions.json");
        let manifest_output = export_verified_combat_witness(VerifiedCombatWitnessExport {
            root_position: &root_position,
            action_output: &action_output,
            actions: &actions,
            expected_final_position: &final_position,
            max_engine_steps_per_transition: MAX_ENGINE_STEPS_PER_TRANSITION,
            manifest: Some(CombatEvidenceManifestExport {
                producer: CombatEvidenceProducerV1::PolicyDiscrepancySearch,
                case_path: &case_path,
                evidence_id: "policy_discrepancy_complete_win",
            }),
        })
        .expect("verified witness should export")
        .expect("manifest should be written");

        let manifest = decode_combat_evidence_manifest(
            &manifest_output,
            &fs::read(&manifest_output).expect("manifest should exist"),
        )
        .expect("manifest should decode");
        let expected_root_hash = CombatDecisionRoot::new(root_position)
            .expect("fixture root should be valid")
            .exact_state_hash()
            .to_string();
        assert_eq!(
            manifest.producer,
            CombatEvidenceProducerV1::PolicyDiscrepancySearch
        );
        assert!(!manifest.case_path.is_absolute());
        let manifest_base = manifest_output.parent().unwrap();
        assert_eq!(
            manifest_base
                .join(&manifest.case_path)
                .canonicalize()
                .unwrap(),
            case_path.canonicalize().unwrap()
        );
        let identity = manifest.case_identity.as_ref().unwrap();
        assert_eq!(identity.root_exact_state_hash, expected_root_hash);
        assert_eq!(
            identity.capability,
            sts_oracle_runtime::eval::combat_case_context::CombatCaseReplayCapabilityV2::IsolatedProjection
        );
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(
            manifest_base
                .join(&manifest.entries[0].action_paths[0])
                .canonicalize()
                .unwrap(),
            action_output.canonicalize().unwrap()
        );
        assert_eq!(manifest.entries[0].supplied_action_count, actions.len());
        assert_eq!(manifest.entries[0].expected_terminal, CombatTerminal::Win);
        assert_eq!(
            manifest.entries[0].expected_final_player_hp,
            Some(final_position.combat.entities.player.current_hp)
        );

        fs::remove_dir_all(directory).expect("temporary export should clean up");
    }

    #[test]
    fn witness_mismatch_is_rejected_before_any_export_is_written() {
        let (case_path, root_position, actions, _) = fixture();
        let directory = temp_directory("mismatch");
        let action_output = directory.join("mismatch.actions.json");
        let error = export_verified_combat_witness(VerifiedCombatWitnessExport {
            root_position: &root_position,
            action_output: &action_output,
            actions: &actions,
            expected_final_position: &root_position,
            max_engine_steps_per_transition: MAX_ENGINE_STEPS_PER_TRANSITION,
            manifest: Some(CombatEvidenceManifestExport {
                producer: CombatEvidenceProducerV1::PolicyDiscrepancySearch,
                case_path: &case_path,
                evidence_id: "policy_discrepancy_complete_win",
            }),
        })
        .expect_err("a mismatched expected final state must be rejected");

        assert!(error.contains("did not match the search witness"));
        assert!(!action_output.exists());
        assert!(!combat_evidence_manifest_path_for_actions(&action_output).exists());
    }
}
