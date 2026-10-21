use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sts_oracle_runtime::eval::combat_case::{load_combat_case, CombatCase};
use sts_oracle_runtime::sim::combat::CombatPosition;
use sts_oracle_runtime::state::core::ClientInput;

use super::{
    export_local_graph_paths, LocalGraphExportActions, LocalGraphExportPaths, LocalGraphExports,
};
use crate::combat_evidence_manifest::{
    combat_evidence_manifest_path_for_actions, decode_combat_evidence_manifest,
    CombatEvidenceProducerV1,
};
use crate::combat_replay_tools::replay_combat_inputs;

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
            "sts-oracle-local-graph-export-{label}-{}-{nonce}",
            std::process::id()
        ))
}

fn fixture() -> (PathBuf, CombatCase, Vec<ClientInput>) {
    let case_path = fixture_path("seed20260713009_a0_slime_boss.combat-case.json");
    let actions_path = fixture_path("seed20260713009_a0_slime_boss.local-turn-graph.actions.json");
    let case = load_combat_case(&case_path).expect("fixture case should load");
    let actions = serde_json::from_slice::<Vec<ClientInput>>(
        &fs::read(actions_path).expect("fixture actions should load"),
    )
    .expect("fixture actions should decode");
    (case_path, case, actions)
}

fn export_fixture(
    case_path: Option<&Path>,
    case: &CombatCase,
    actions: &[ClientInput],
    final_position: &CombatPosition,
    action_output: &Path,
) -> LocalGraphExports {
    export_local_graph_paths(
        case,
        case_path,
        LocalGraphExportPaths {
            witness_actions: Some(action_output),
            deepest_survival_case: None,
            deepest_progress_case: None,
        },
        LocalGraphExportActions {
            witness: Some(actions),
            witness_final_position: Some(final_position),
            deepest_survival: &[],
            deepest_progress: &[],
        },
        MAX_ENGINE_STEPS_PER_TRANSITION,
    )
    .expect("local-graph witness should export")
}

#[test]
fn ordinary_complete_win_exports_a_local_graph_manifest() {
    let (case_path, case, actions) = fixture();
    let final_position = replay_combat_inputs(
        case.core.position.clone(),
        &actions,
        MAX_ENGINE_STEPS_PER_TRANSITION,
    )
    .expect("fixture actions should replay");
    let directory = temp_directory("ordinary");
    let action_output = directory.join("win.actions.json");
    let exports = export_fixture(
        Some(&case_path),
        &case,
        &actions,
        &final_position,
        &action_output,
    );

    let manifest_path = exports
        .witness_manifest
        .expect("ordinary root should receive a manifest");
    let manifest = decode_combat_evidence_manifest(
        &manifest_path,
        &fs::read(&manifest_path).expect("manifest should exist"),
    )
    .expect("manifest should decode");
    assert_eq!(
        manifest.producer,
        CombatEvidenceProducerV1::LocalGraphSearch
    );
    let manifest_base = manifest_path.parent().unwrap();
    assert_eq!(
        manifest_base
            .join(&manifest.case_path)
            .canonicalize()
            .unwrap(),
        case_path.canonicalize().unwrap()
    );
    assert_eq!(
        manifest_base
            .join(&manifest.entries[0].action_paths[0])
            .canonicalize()
            .unwrap(),
        action_output.canonicalize().unwrap()
    );

    fs::remove_dir_all(directory).expect("temporary export should clean up");
}

#[test]
fn full_health_counterfactual_exports_actions_without_claiming_the_original_case() {
    let (_, mut case, actions) = fixture();
    case.core.position.combat.entities.player.current_hp =
        case.core.position.combat.entities.player.max_hp;
    let final_position = replay_combat_inputs(
        case.core.position.clone(),
        &actions,
        MAX_ENGINE_STEPS_PER_TRANSITION,
    )
    .expect("counterfactual fixture actions should replay");
    let directory = temp_directory("full-health");
    let action_output = directory.join("win.actions.json");
    let exports = export_fixture(None, &case, &actions, &final_position, &action_output);

    assert_eq!(exports.witness_actions, Some(action_output.clone()));
    assert!(exports.witness_manifest.is_none());
    assert!(!combat_evidence_manifest_path_for_actions(&action_output).exists());

    fs::remove_dir_all(directory).expect("temporary export should clean up");
}
