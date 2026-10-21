use super::{
    derive_shuffle_seed_v1, load_and_resolve_combat_lab_spec_v1, preflight_combat_lab_scenario_v1,
    profile_config_v1, summarize_combat_lab_v1, CombatLabArtifactStoreV1, CombatLabCellRecordV1,
    CombatLabManifestV1, CombatLabOutcomeClassV1, CombatLabShuffleGeneratorV1,
    CombatLabShuffleScheduleV1, CombatLabSpecV1,
};
use crate::ai::combat_search_v2::CombatSearchV2Satisfaction;
use crate::eval::fingerprint::combat_state_fingerprint_v3;
use crate::fixtures::combat_start_spec::compile_combat_start_spec;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;

#[test]
fn seed006_derived_fixture_resolves() {
    let fixture =
        crate::eval::repository_root().join("fixtures/combat_lab/seed006_reptomancer_8x2.lab.json");

    let resolved = load_and_resolve_combat_lab_spec_v1(&fixture)
        .expect("seed006-derived laboratory fixture should resolve");
    preflight_combat_lab_scenario_v1(&resolved)
        .expect("seed006-derived start fixture should compile without approximation");

    assert_eq!(resolved.scenario_id, "seed006_derived_reptomancer");
    assert_eq!(resolved.experiment_id, "seed006_reptomancer_8x2");
    assert_eq!(
        resolved.start_spec_snapshot.name,
        "seed006_derived_reptomancer"
    );
    assert_eq!(resolved.schedule.seed, 20_260_713_006);
    assert_eq!(
        resolved.schedule.generator,
        CombatLabShuffleGeneratorV1::SplitMix64V1
    );
    assert_eq!(resolved.profiles.len(), 2);
    assert!(resolved.profiles.iter().all(|profile| {
        profile.spec.information_scope == super::CombatLabInformationScopeV1::ExactStateOracle
    }));
    assert_eq!(
        resolved
            .profiles
            .iter()
            .map(|profile| profile.spec.id.as_str())
            .collect::<Vec<_>>(),
        vec!["lazy_on_pop", "immediate"]
    );
    assert_eq!(
        resolved
            .profiles
            .iter()
            .map(|profile| profile.spec.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Lazy on pop", "Immediate"]
    );

    let budget = &resolved.common_budget;
    assert_eq!(budget.max_nodes, 200_000);
    assert_eq!(budget.max_actions_per_line, 200);
    assert_eq!(budget.max_engine_steps_per_action, 250);
    assert_eq!(budget.wall_ms, Some(3_000));
    assert_eq!(
        budget.satisfaction,
        CombatSearchV2Satisfaction::ZeroLossOrBudget
    );
    assert_eq!(budget.max_potions_used, Some(1));
    assert_eq!(budget.rollout_max_evaluations, 384);
    assert_eq!(budget.rollout_max_actions, 80);
    assert_eq!(budget.rollout_beam_width, 3);
    assert_eq!(budget.turn_plan_probe_max_inner_nodes, None);
    assert_eq!(budget.turn_plan_probe_max_end_states, None);
    assert_eq!(budget.turn_plan_probe_per_bucket_limit, None);
}

#[test]
fn seed006_feasibility_fixture_resolves_with_any_surviving_win_threshold() {
    let fixtures = crate::eval::repository_root().join("fixtures/combat_lab");
    let feasibility = load_and_resolve_combat_lab_spec_v1(
        &fixtures.join("seed006_reptomancer_feasibility_4x2.lab.json"),
    )
    .expect("seed006 feasibility fixture should resolve");
    preflight_combat_lab_scenario_v1(&feasibility)
        .expect("seed006 feasibility start should compile without approximation");

    assert_eq!(
        feasibility.experiment_id,
        "seed006_reptomancer_feasibility_release_3s_4x2"
    );
    assert_eq!(feasibility.scenario_id, "seed006_derived_reptomancer");
    assert_eq!(feasibility.schedule.seed, 20_260_713_006);
    assert_eq!(
        feasibility.schedule.generator,
        CombatLabShuffleGeneratorV1::SplitMix64V1
    );
    assert_eq!(
        feasibility
            .profiles
            .iter()
            .map(|profile| profile.spec.id.as_str())
            .collect::<Vec<_>>(),
        vec!["lazy_on_pop", "immediate"]
    );
    assert!(feasibility.profiles.iter().all(|profile| {
        profile.spec.information_scope == super::CombatLabInformationScopeV1::ExactStateOracle
    }));

    let budget = &feasibility.common_budget;
    assert_eq!(budget.max_nodes, 200_000);
    assert_eq!(budget.max_actions_per_line, 200);
    assert_eq!(budget.max_engine_steps_per_action, 250);
    assert_eq!(budget.wall_ms, Some(3_000));
    assert_eq!(
        budget.satisfaction,
        CombatSearchV2Satisfaction::HpLossAtMost(87)
    );
    assert_eq!(budget.max_potions_used, Some(1));
    assert_eq!(budget.rollout_max_evaluations, 384);
    assert_eq!(budget.rollout_max_actions, 80);
    assert_eq!(budget.rollout_beam_width, 3);
    assert_eq!(budget.turn_plan_probe_max_inner_nodes, None);
    assert_eq!(budget.turn_plan_probe_max_end_states, None);
    assert_eq!(budget.turn_plan_probe_per_bucket_limit, None);

    let maintained =
        load_and_resolve_combat_lab_spec_v1(&fixtures.join("seed006_reptomancer_8x2.lab.json"))
            .expect("maintained seed006 fixture should resolve");
    assert_eq!(feasibility.schema_version, maintained.schema_version);
    assert_eq!(feasibility.scenario_id, maintained.scenario_id);
    assert_eq!(feasibility.scenario_hash, maintained.scenario_hash);
    assert_eq!(
        serde_json::to_value(&feasibility.start_spec_snapshot)
            .expect("serialize feasibility start"),
        serde_json::to_value(&maintained.start_spec_snapshot).expect("serialize maintained start")
    );
    assert_eq!(
        serde_json::to_value(&feasibility.schedule).expect("serialize feasibility schedule"),
        serde_json::to_value(&maintained.schedule).expect("serialize maintained schedule")
    );
    assert_eq!(
        serde_json::to_value(&feasibility.profiles).expect("serialize feasibility profiles"),
        serde_json::to_value(&maintained.profiles).expect("serialize maintained profiles")
    );
    let mut normalized_budget = feasibility.common_budget.clone();
    normalized_budget.satisfaction = CombatSearchV2Satisfaction::ZeroLossOrBudget;
    assert_eq!(
        serde_json::to_value(normalized_budget).expect("serialize normalized budget"),
        serde_json::to_value(&maintained.common_budget).expect("serialize maintained budget")
    );
}

#[test]
fn artifact_new_run_writes_manifest_before_cells() {
    let (directory, resolved) = resolved_lab_fixture("artifact_new_run_manifest_first");
    let output = directory.join("run");
    let expected = artifact_manifest(resolved, 123);

    let store = CombatLabArtifactStoreV1::create_or_resume(&output, expected)
        .expect("create artifact store");

    assert!(output.join("manifest.json").is_file());
    assert!(!output.join("cells.jsonl").exists());
    assert!(store.cells().is_empty());
    assert_eq!(store.manifest().created_at_unix_ms, 123);
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_store_rejects_legacy_cell_schema_before_append() {
    let (directory, resolved) = resolved_lab_fixture("artifact_rejects_legacy_cell");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 123);
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .expect("create artifact store");
    let mut legacy = artifact_cell(&resolved, 0);
    legacy.schema_version = 1;

    let error = store
        .append_cell(&legacy)
        .expect_err("legacy cell schema must not enter a V2-fingerprint journal");

    assert!(error.contains("unsupported combat laboratory cell schema_version"));
    assert!(!output.join("cells.jsonl").exists());
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_resume_rejects_legacy_cell_schema_in_journal() {
    let (directory, resolved) = resolved_lab_fixture("artifact_rejects_legacy_journal_cell");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 123);
    drop(
        CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
            .expect("create artifact store"),
    );
    fs::write(
        output.join("cells.jsonl"),
        b"{\"schema_version\":1,\"legacy_fingerprint\":{}}\n",
    )
    .expect("write legacy journal fixture");

    let error = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .err()
        .expect("legacy journal cell must fail resume");

    assert!(error.contains("unsupported combat laboratory cell schema_version"));
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_manifest_records_build_identity() {
    let (directory, resolved) = resolved_lab_fixture("artifact_build_identity");
    let manifest = artifact_manifest(resolved, 123);

    assert_eq!(
        manifest.environment.cargo_profile.as_deref(),
        Some(env!("STS_CARGO_PROFILE"))
    );
    assert_eq!(
        manifest.environment.debug_assertions,
        Some(cfg!(debug_assertions))
    );

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_resume_rejects_missing_or_changed_build_identity() {
    let (directory, resolved) = resolved_lab_fixture("artifact_build_identity_resume");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
        .expect("create build-identified artifact");
    store
        .append_cell(&artifact_cell(&resolved, 0))
        .expect("append identity-test cell");
    drop(store);

    let manifest_path = output.join("manifest.json");
    let canonical_manifest = fs::read(&manifest_path).expect("read canonical manifest");
    let canonical_journal = fs::read(output.join("cells.jsonl")).expect("read canonical journal");
    for (field, mutation) in [
        ("environment.cargo_profile", "remove_cargo_profile"),
        ("environment.cargo_profile", "change_cargo_profile"),
        ("environment.debug_assertions", "change_debug_assertions"),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&canonical_manifest).expect("parse canonical manifest");
        match mutation {
            "remove_cargo_profile" => {
                value["environment"]
                    .as_object_mut()
                    .expect("environment object")
                    .remove("cargo_profile");
            }
            "change_cargo_profile" => {
                value["environment"]["cargo_profile"] = json!("changed-profile");
            }
            "change_debug_assertions" => {
                value["environment"]["debug_assertions"] = json!(!cfg!(debug_assertions));
            }
            _ => unreachable!(),
        }
        fs::write(
            &manifest_path,
            serde_json::to_vec(&value).expect("serialize mutated manifest"),
        )
        .expect("write mutated manifest");

        let error = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
            .err()
            .expect("build identity mismatch must reject resume");
        assert!(error.contains(field), "{error}");
        assert_eq!(
            fs::read(output.join("cells.jsonl")).expect("read preserved journal"),
            canonical_journal
        );
        fs::write(&manifest_path, &canonical_manifest).expect("restore canonical manifest");
    }

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_legacy_manifest_without_build_identity_still_deserializes() {
    let (directory, resolved) = resolved_lab_fixture("artifact_legacy_build_identity");
    let manifest = artifact_manifest(resolved, 100);
    let mut value = serde_json::to_value(manifest).expect("serialize manifest value");
    let environment = value["environment"]
        .as_object_mut()
        .expect("environment object");
    environment.remove("cargo_profile");
    environment.remove("debug_assertions");

    let legacy: CombatLabManifestV1 =
        serde_json::from_value(value).expect("legacy manifest should remain readable");
    assert_eq!(legacy.environment.cargo_profile, None);
    assert_eq!(legacy.environment.debug_assertions, None);

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_new_run_refuses_canonical_orphan_files() {
    let (directory, resolved) = resolved_lab_fixture("artifact_canonical_orphans");
    for file_name in ["cells.jsonl", "checkpoint.json", "summary.json"] {
        let output = directory.join(file_name.replace('.', "_"));
        fs::create_dir_all(&output).expect("create orphan artifact directory");
        let orphan_path = output.join(file_name);
        fs::write(&orphan_path, b"orphan-evidence").expect("write orphan artifact");

        let error = CombatLabArtifactStoreV1::create_or_resume(
            &output,
            artifact_manifest(resolved.clone(), 100),
        )
        .err()
        .expect("canonical orphan must prevent new-run creation");
        assert!(error.contains("orphan"), "{error}");
        assert!(error.contains(file_name), "{error}");
        assert!(!output.join("manifest.json").exists());
        assert_eq!(
            fs::read(orphan_path).expect("read untouched orphan"),
            b"orphan-evidence"
        );
    }
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn resume_does_not_duplicate_cells() {
    let (directory, resolved) = resolved_lab_fixture("resume_does_not_duplicate_cells");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
        .expect("create artifact store");
    let cell = artifact_cell(&resolved, 0);

    store.append_cell(&cell).expect("append first cell");
    let duplicate = store
        .append_cell(&cell)
        .expect_err("duplicate cell key must be rejected");
    assert!(duplicate.contains("duplicate cell key"), "{duplicate}");
    drop(store);

    let reopened =
        CombatLabArtifactStoreV1::create_or_resume(&output, artifact_manifest(resolved, 200))
            .expect("resume artifact store");
    assert_eq!(reopened.cells().len(), 1);
    assert!(reopened.contains_cell(&cell.cell_key));

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_truncated_final_line_is_ignored_and_pending_cell_can_append() {
    let (directory, resolved) = resolved_lab_fixture("artifact_truncated_final_line");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let first = artifact_cell(&resolved, 0);
    let pending = artifact_cell(&resolved, 1);
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
        .expect("create artifact store");
    store.append_cell(&first).expect("append first cell");
    drop(store);

    let pending_bytes = serde_json::to_vec(&pending).expect("serialize pending cell");
    let partial_len = pending_bytes.len() / 2;
    let mut journal = fs::OpenOptions::new()
        .append(true)
        .open(output.join("cells.jsonl"))
        .expect("open journal for interrupted append");
    journal
        .write_all(&pending_bytes[..partial_len])
        .expect("write partial final line");
    journal.sync_data().expect("sync partial final line");
    drop(journal);

    let mut resumed = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
        .expect("ignore partial final line");
    assert_eq!(resumed.cells().len(), 1);
    assert!(resumed.contains_cell(&first.cell_key));
    assert!(!resumed.contains_cell(&pending.cell_key));
    resumed
        .append_cell(&pending)
        .expect("append exact pending cell after tail repair");
    drop(resumed);

    let reopened = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .expect("reopen repaired journal");
    assert_eq!(reopened.cells().len(), 2);
    assert!(reopened.contains_cell(&pending.cell_key));
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_journal_persistence_failure_poisons_store_until_reopen() {
    let (directory, resolved) = resolved_lab_fixture("artifact_journal_failure_poison");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let cell = artifact_cell(&resolved, 0);
    let mut store = CombatLabArtifactStoreV1::create_or_resume_with_journal_sync(
        &output,
        manifest.clone(),
        fail_journal_sync_after_write,
    )
    .expect("create artifact store with deterministic journal sync seam");

    let persistence_error = store
        .append_cell(&cell)
        .expect_err("injected sync failure must fail append");
    assert!(persistence_error.contains("injected sync failure"));
    let mut expected_journal = serde_json::to_vec(&cell).expect("serialize expected cell");
    expected_journal.push(b'\n');
    assert_eq!(
        fs::read(output.join("cells.jsonl")).expect("read journal after failed persistence"),
        expected_journal
    );
    assert!(!store.contains_cell(&cell.cell_key));

    for error in [
        store
            .append_cell(&cell)
            .expect_err("same-store append retry must be poisoned"),
        store
            .checkpoint_sample_boundary(0)
            .expect_err("same-store checkpoint must be poisoned"),
        store
            .write_summary(&json!({"must_not_write": true}))
            .expect_err("same-store summary must be poisoned"),
    ] {
        assert!(error.contains("reopen required"), "{error}");
    }
    assert!(!output.join("checkpoint.json").exists());
    assert!(!output.join("summary.json").exists());
    drop(store);

    let reopened = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .expect("reopen reconciles authoritative journal");
    assert_eq!(reopened.cells().len(), 1);
    assert!(reopened.contains_cell(&cell.cell_key));
    reopened
        .checkpoint_sample_boundary(1)
        .expect("reopened store can checkpoint reconciled evidence");
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_malformed_newline_terminated_journal_entry_is_error() {
    let (directory, resolved) = resolved_lab_fixture("artifact_malformed_terminated_line");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved, 100);
    drop(
        CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
            .expect("create artifact store"),
    );
    fs::write(output.join("cells.jsonl"), b"{not-json}\n")
        .expect("write malformed terminated entry");

    let error = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .err()
        .expect("terminated malformed entry must fail resume");
    assert!(error.contains("line 1"), "{error}");
    assert!(error.contains("malformed"), "{error}");
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_empty_newline_terminated_journal_entry_is_error() {
    let (directory, resolved) = resolved_lab_fixture("artifact_empty_terminated_line");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved, 100);
    drop(
        CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
            .expect("create artifact store"),
    );
    fs::write(output.join("cells.jsonl"), b"\n").expect("write empty terminated entry");

    let error = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .err()
        .expect("empty terminated entry must fail resume");
    assert!(error.contains("line 1"), "{error}");
    assert!(error.contains("malformed"), "{error}");
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_checkpoint_disagreement_is_repaired_from_journal() {
    let (directory, resolved) = resolved_lab_fixture("artifact_checkpoint_repair");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let cell = artifact_cell(&resolved, 0);
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
        .expect("create artifact store");
    store.append_cell(&cell).expect("append cell");
    store
        .checkpoint_sample_boundary(1)
        .expect("write checkpoint");
    drop(store);

    let bad_checkpoint = super::CombatLabCheckpointV1 {
        schema_version: 1,
        journal_digest: "invented".to_string(),
        completed_cell_keys: BTreeSet::from(["ghost-cell".to_string()]),
        next_sample_hint: 99,
    };
    fs::write(
        output.join("checkpoint.json"),
        serde_json::to_vec(&bad_checkpoint).expect("serialize bad checkpoint"),
    )
    .expect("replace checkpoint with disagreement");

    let resumed = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .expect("repair checkpoint from journal");
    assert!(resumed.contains_cell(&cell.cell_key));
    let repaired: super::CombatLabCheckpointV1 = serde_json::from_slice(
        &fs::read(output.join("checkpoint.json")).expect("read repaired checkpoint"),
    )
    .expect("parse repaired checkpoint");
    assert_eq!(
        repaired.completed_cell_keys,
        BTreeSet::from([cell.cell_key])
    );
    assert_eq!(
        repaired.journal_digest,
        artifact_journal_digest(&fs::read(output.join("cells.jsonl")).expect("read journal"))
    );
    assert_ne!(repaired.next_sample_hint, 99);
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_checkpoint_writer_rejects_nonconservative_hint() {
    let (directory, resolved) = resolved_lab_fixture("artifact_checkpoint_writer_hint");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let cell = artifact_cell(&resolved, 0);
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .expect("create artifact store");
    store.append_cell(&cell).expect("append sample zero");

    let error = store
        .checkpoint_sample_boundary(99)
        .expect_err("caller hint cannot exceed journal-derived boundary");
    assert!(error.contains("next_sample_hint"), "{error}");
    assert!(error.contains("journal-derived 1"), "{error}");
    assert!(!output.join("checkpoint.json").exists());
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_checkpoint_forged_high_hint_is_repaired_from_journal() {
    let (directory, resolved) = resolved_lab_fixture("artifact_checkpoint_forged_hint");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let cell = artifact_cell(&resolved, 0);
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
        .expect("create artifact store");
    store.append_cell(&cell).expect("append sample zero");
    store
        .checkpoint_sample_boundary(1)
        .expect("write conservative checkpoint");
    drop(store);

    let checkpoint_path = output.join("checkpoint.json");
    let mut forged: super::CombatLabCheckpointV1 =
        serde_json::from_slice(&fs::read(&checkpoint_path).expect("read checkpoint"))
            .expect("parse checkpoint");
    forged.next_sample_hint = 99;
    fs::write(
        &checkpoint_path,
        serde_json::to_vec(&forged).expect("serialize forged checkpoint"),
    )
    .expect("write forged checkpoint");

    drop(
        CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
            .expect("repair forged checkpoint hint"),
    );
    let repaired: super::CombatLabCheckpointV1 =
        serde_json::from_slice(&fs::read(checkpoint_path).expect("read repaired checkpoint"))
            .expect("parse repaired checkpoint");
    assert_eq!(repaired.next_sample_hint, 1);
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn resume_mismatch_reports_profile_budget_scenario_and_code_fields() {
    let (directory, resolved) = resolved_lab_fixture("resume_field_mismatch");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved, 100);
    drop(
        CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
            .expect("create artifact store"),
    );

    let mut profile = manifest.clone();
    profile.resolved_spec.profiles[0].profile_hash = "changed-profile".to_string();
    profile.resolved_spec.experiment_hash = "changed-experiment".to_string();
    profile.experiment_hash = "changed-experiment".to_string();
    assert_resume_mismatch(&output, profile, "resolved_spec.profiles[0].profile_hash");

    let mut budget = manifest.clone();
    budget.resolved_spec.budget_hash = "changed-budget".to_string();
    budget.resolved_spec.experiment_hash = "changed-experiment".to_string();
    budget.experiment_hash = "changed-experiment".to_string();
    assert_resume_mismatch(&output, budget, "resolved_spec.budget_hash");

    let mut scenario = manifest.clone();
    scenario.resolved_spec.scenario_hash = "changed-scenario".to_string();
    scenario.resolved_spec.experiment_hash = "changed-experiment".to_string();
    scenario.experiment_hash = "changed-experiment".to_string();
    assert_resume_mismatch(&output, scenario, "resolved_spec.scenario_hash");

    let mut commit = manifest.clone();
    commit.source_identity.git_commit = Some("changed-commit".to_string());
    assert_resume_mismatch(&output, commit, "source_identity.git_commit");

    let mut dirty = manifest.clone();
    dirty.source_identity.git_dirty = Some(true);
    assert_resume_mismatch(&output, dirty, "source_identity.git_dirty");

    for (field, mut environment) in [
        ("environment.package_version", manifest.clone()),
        ("environment.target_os", manifest.clone()),
        ("environment.target_arch", manifest.clone()),
    ] {
        match field {
            "environment.package_version" => {
                environment.environment.package_version = "changed".to_string()
            }
            "environment.target_os" => environment.environment.target_os = "changed".to_string(),
            "environment.target_arch" => {
                environment.environment.target_arch = "changed".to_string()
            }
            _ => unreachable!(),
        }
        assert_resume_mismatch(&output, environment, field);
    }

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn resume_target_extension_returns_only_new_or_missing_cells() {
    let (directory, resolved) = resolved_lab_fixture("resume_target_extension");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let requested = (0..3)
        .map(|sample_index| artifact_cell(&resolved, sample_index))
        .collect::<Vec<_>>();
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
        .expect("create artifact store");
    store.append_cell(&requested[0]).expect("append old cell");
    drop(store);

    let resumed = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .expect("resume for larger requested target");
    let missing = requested
        .iter()
        .filter(|cell| !resumed.contains_cell(&cell.cell_key))
        .map(|cell| cell.sample_index)
        .collect::<Vec<_>>();
    assert_eq!(missing, vec![1, 2]);
    assert!(resumed.contains_cell(&requested[0].cell_key));
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn resume_target_decrease_deletes_no_journal_cells() {
    let (directory, resolved) = resolved_lab_fixture("resume_target_decrease");
    let output = directory.join("run");
    let manifest = artifact_manifest(resolved.clone(), 100);
    let cells = (0..3)
        .map(|sample_index| artifact_cell(&resolved, sample_index))
        .collect::<Vec<_>>();
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output, manifest.clone())
        .expect("create artifact store");
    for cell in &cells {
        store.append_cell(cell).expect("append cell");
    }
    drop(store);

    let resumed = CombatLabArtifactStoreV1::create_or_resume(&output, manifest)
        .expect("resume for smaller requested target");
    let requested_sample_count = 1_u64;
    let pending_within_decreased_target = cells
        .iter()
        .filter(|cell| cell.sample_index < requested_sample_count)
        .filter(|cell| !resumed.contains_cell(&cell.cell_key))
        .count();
    assert_eq!(pending_within_decreased_target, 0);
    assert_eq!(resumed.cells().len(), 3);
    assert!(cells
        .iter()
        .all(|cell| resumed.contains_cell(&cell.cell_key)));
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn artifact_atomic_json_replacement_overwrites_existing_destination_on_windows() {
    let (directory, resolved) = resolved_lab_fixture("artifact_replace_existing");
    let output = directory.join("run");
    let store =
        CombatLabArtifactStoreV1::create_or_resume(&output, artifact_manifest(resolved, 100))
            .expect("create artifact store");
    store
        .write_summary(&json!({"generation": 1}))
        .expect("write first summary");
    store
        .write_summary(&json!({"generation": 2}))
        .expect("replace existing summary");

    let summary: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("summary.json")).expect("read replaced summary"),
    )
    .expect("parse replaced summary");
    assert_eq!(summary["generation"], 2);
    assert!(fs::read_dir(&output)
        .expect("list artifact directory")
        .all(|entry| !entry
            .expect("read artifact entry")
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")));
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn sample_is_shared_across_profiles() {
    let directory = test_directory("sample_is_shared_across_profiles");
    fs::create_dir_all(&directory).expect("create test directory");
    let mut lab = valid_lab_spec_value();
    let mut second_profile = lab["profiles"][0].clone();
    second_profile["id"] = json!("comparison");
    second_profile["label"] = json!("Comparison");
    lab["profiles"] = json!([lab["profiles"][0].clone(), second_profile]);
    write_json(&directory.join("start.json"), &valid_start_spec_value());
    write_json(&directory.join("lab.json"), &lab);
    let resolved = load_and_resolve_combat_lab_spec_v1(&directory.join("lab.json"))
        .expect("resolve laboratory spec");
    let compiler = preflight_combat_lab_scenario_v1(&resolved).expect("preflight scenario");

    let sample = compiler.compile_sample(0).expect("compile sample once");
    let mut profile_starts = resolved
        .profiles
        .iter()
        .map(|_| sample.start.clone())
        .collect::<Vec<_>>();

    assert_eq!(profile_starts.len(), 2);
    assert_eq!(profile_starts[0], profile_starts[1]);
    profile_starts[0].combat.entities.player.current_hp = 1;
    assert_eq!(profile_starts[1], sample.start);

    let baseline = compile_combat_start_spec(&resolved.start_spec_snapshot)
        .map(|(engine, combat)| crate::sim::combat::CombatPosition::new(engine, combat))
        .expect("compile no-override baseline");
    let baseline_fingerprint = combat_state_fingerprint_v3(&baseline);
    let changed_rng_streams = baseline_fingerprint
        .rng_boundary
        .streams
        .iter()
        .zip(&sample.state_fingerprint.rng_boundary.streams)
        .filter_map(|(baseline, sampled)| (baseline != sampled).then_some(baseline.name.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(changed_rng_streams, vec!["shuffle_rng"]);

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn sample_invariant_error_names_first_field() {
    let spec: crate::fixtures::combat_start_spec::CombatStartSpec =
        serde_json::from_value(valid_start_spec_value()).expect("parse start spec");
    let baseline = compile_combat_start_spec(&spec)
        .map(|(engine, combat)| crate::sim::combat::CombatPosition::new(engine, combat))
        .expect("compile baseline");
    let mut sampled = baseline.clone();
    sampled.combat.entities.monsters[0].enemy_id = crate::content::monsters::EnemyId::Cultist;
    sampled.combat.entities.monsters[0].current_hp += 1;

    let error = super::scenario::validate_combat_lab_sample_invariants_v1(&baseline, &sampled)
        .expect_err("changed monster identity must fail the invariant check");

    assert!(error.contains("monsters[0].enemy_id"), "{error}");
}

#[test]
fn runner_creates_the_owned_artifact_root_before_resolving_a_new_output() {
    let repository = test_directory("runner_creates_artifact_root");
    fs::create_dir_all(&repository).expect("create isolated repository root");
    let requested = repository
        .join("artifacts")
        .join("runs")
        .join("new-experiment");

    let resolved =
        super::runner::resolve_lab_output_dir_under_repository_v1(&repository, &requested)
            .expect("resolve output below a newly created artifact root");

    assert!(repository.join("artifacts").join("runs").is_dir());
    assert_eq!(
        resolved,
        fs::canonicalize(&repository)
            .expect("canonical isolated repository root")
            .join("artifacts")
            .join("runs")
            .join("new-experiment")
    );
    fs::remove_dir_all(repository).expect("remove isolated repository root");
}

#[test]
fn runner_resumes_sample_major_and_bounds_smaller_requests() {
    let (fixture_directory, lab_spec_path, output_dir) =
        runner_fixture("runner_resumes_sample_major");
    let executor = RecordingCellExecutor::default();
    let mut request = super::CombatLabRunRequestV1 {
        lab_spec_path,
        output_dir: output_dir.clone(),
        requested_samples: 1,
    };

    let first = super::runner::run_combat_lab_v1_with_executor(&request, &executor)
        .expect("run first sample");
    assert_eq!(first.cells_present, 2);
    assert_eq!(first.cells_appended, 2);
    assert_eq!(
        executor.calls.borrow().as_slice(),
        &[(0, "baseline"), (0, "comparison")]
    );
    let first_journal = fs::read(output_dir.join("cells.jsonl")).expect("read first journal");
    let first_keys = journal_cell_keys(&first_journal);
    assert_eq!(first_keys.len(), 2);
    assert!(read_journal_cells(&output_dir).iter().all(|cell| {
        cell.initial_state_fingerprint.is_some()
            && cell.non_shuffle_rng_hash.is_some()
            && cell.shuffle_rng_hash.is_some()
            && cell.start_hp.is_some()
    }));

    request.requested_samples = 2;
    let second = super::runner::run_combat_lab_v1_with_executor(&request, &executor)
        .expect("extend to second sample");
    assert_eq!(second.cells_present, 4);
    assert_eq!(second.cells_appended, 2);
    assert_eq!(
        executor.calls.borrow().as_slice(),
        &[
            (0, "baseline"),
            (0, "comparison"),
            (1, "baseline"),
            (1, "comparison"),
        ]
    );
    let extended_journal = fs::read(output_dir.join("cells.jsonl")).expect("read extended journal");
    assert!(extended_journal.starts_with(&first_journal));
    let extended_keys = journal_cell_keys(&extended_journal);
    assert_eq!(extended_keys.len(), 4);
    assert!(first_keys.is_subset(&extended_keys));
    let extended_summary = fs::read(output_dir.join("summary.json")).expect("read summary");

    let third = super::runner::run_combat_lab_v1_with_executor(&request, &executor)
        .expect("resume completed target");
    assert_eq!(third.cells_appended, 0);
    assert_eq!(executor.calls.borrow().len(), 4);
    assert_eq!(
        fs::read(output_dir.join("summary.json")).expect("read regenerated summary"),
        extended_summary
    );

    let manifest_before_smaller =
        fs::read(output_dir.join("manifest.json")).expect("read manifest before smaller target");
    let checkpoint_before_smaller = fs::read(output_dir.join("checkpoint.json"))
        .expect("read checkpoint before smaller target");
    request.requested_samples = 1;
    let smaller = super::runner::run_combat_lab_v1_with_executor(&request, &executor)
        .expect("accept smaller positive target");
    assert_eq!(smaller.cells_present, 4);
    assert_eq!(smaller.cells_appended, 0);
    assert_eq!(smaller.summary.completed_cells, 2);
    assert!(smaller
        .summary
        .profiles
        .iter()
        .all(|profile| profile.completed_cells == 1));
    assert_eq!(executor.calls.borrow().len(), 4);
    assert_eq!(
        fs::read(output_dir.join("manifest.json")).expect("read preserved manifest"),
        manifest_before_smaller
    );
    assert_eq!(
        fs::read(output_dir.join("cells.jsonl")).expect("read preserved journal"),
        extended_journal
    );
    assert_eq!(
        fs::read(output_dir.join("checkpoint.json")).expect("read preserved checkpoint"),
        checkpoint_before_smaller
    );
    assert_eq!(
        journal_cell_keys(&fs::read(output_dir.join("cells.jsonl")).expect("read final journal")),
        extended_keys
    );
    for file_name in [
        "manifest.json",
        "cells.jsonl",
        "checkpoint.json",
        "summary.json",
    ] {
        assert!(output_dir.join(file_name).is_file(), "missing {file_name}");
    }

    fs::remove_dir_all(output_dir).expect("remove runner output");
    fs::remove_dir_all(fixture_directory).expect("remove runner fixture");
}

#[test]
fn runner_skips_existing_cells_individually_and_checkpoints_completed_row() {
    let (fixture_directory, lab_spec_path, output_dir) = runner_fixture("runner_partial_row");
    let resolved = load_and_resolve_combat_lab_spec_v1(&lab_spec_path).expect("resolve lab spec");
    let compiler = preflight_combat_lab_scenario_v1(&resolved).expect("preflight scenario");
    let sample = compiler.compile_sample(0).expect("compile sample zero");
    let setup_executor = RecordingCellExecutor::default();
    let first_cell = super::runner::CombatLabCellExecutorV1::execute_cell(
        &setup_executor,
        &resolved,
        &sample,
        &resolved.profiles[0],
    );
    let manifest = CombatLabManifestV1::from_resolved_v1(
        resolved,
        crate::eval::source_identity::current_source_identity(),
        1,
    );
    let mut store = CombatLabArtifactStoreV1::create_or_resume(&output_dir, manifest)
        .expect("create partial artifact");
    store
        .append_cell(&first_cell)
        .expect("append first profile only");
    drop(store);

    let executor = RecordingCellExecutor::default();
    let report = super::runner::run_combat_lab_v1_with_executor(
        &super::CombatLabRunRequestV1 {
            lab_spec_path,
            output_dir: output_dir.clone(),
            requested_samples: 1,
        },
        &executor,
    )
    .expect("resume partial row");

    assert_eq!(report.cells_present, 2);
    assert_eq!(report.cells_appended, 1);
    assert_eq!(executor.calls.borrow().as_slice(), &[(0, "comparison")]);
    let checkpoint: super::CombatLabCheckpointV1 = serde_json::from_slice(
        &fs::read(output_dir.join("checkpoint.json")).expect("read checkpoint"),
    )
    .expect("parse checkpoint");
    assert_eq!(checkpoint.next_sample_hint, 1);
    assert_eq!(checkpoint.completed_cell_keys.len(), 2);

    fs::remove_dir_all(output_dir).expect("remove runner output");
    fs::remove_dir_all(fixture_directory).expect("remove runner fixture");
}

#[test]
fn runner_recovers_missing_checkpoint_from_complete_journal_without_rerunning_cells() {
    let (fixture_directory, lab_spec_path, output_dir) =
        runner_fixture("runner_recovers_missing_checkpoint");
    let request = super::CombatLabRunRequestV1 {
        lab_spec_path,
        output_dir: output_dir.clone(),
        requested_samples: 1,
    };
    let initial_executor = RecordingCellExecutor::default();
    let initial = super::runner::run_combat_lab_v1_with_executor(&request, &initial_executor)
        .expect("produce one complete sample row");
    assert_eq!(initial.cells_appended, 2);
    let journal_before = fs::read(output_dir.join("cells.jsonl")).expect("read complete journal");
    let keys_before = journal_cell_keys(&journal_before);
    fs::remove_file(output_dir.join("checkpoint.json"))
        .expect("model crash after cell sync and before checkpoint replacement");

    let resume_executor = RecordingCellExecutor::default();
    let resumed = super::runner::run_combat_lab_v1_with_executor(&request, &resume_executor)
        .expect("resume complete row after checkpoint loss");

    assert_eq!(resumed.cells_appended, 0);
    assert!(resume_executor.calls.borrow().is_empty());
    let journal_after = fs::read(output_dir.join("cells.jsonl")).expect("read preserved journal");
    assert_eq!(journal_after, journal_before);
    assert_eq!(journal_cell_keys(&journal_after), keys_before);
    let checkpoint: super::CombatLabCheckpointV1 = serde_json::from_slice(
        &fs::read(output_dir.join("checkpoint.json")).expect("read recovered checkpoint"),
    )
    .expect("parse recovered checkpoint");
    assert_eq!(checkpoint.next_sample_hint, 1);
    assert_eq!(checkpoint.completed_cell_keys, keys_before);

    fs::remove_dir_all(output_dir).expect("remove runner output");
    fs::remove_dir_all(fixture_directory).expect("remove runner fixture");
}

#[test]
fn runner_flushes_replay_error_and_halts_every_later_cell() {
    let (fixture_directory, lab_spec_path, output_dir) =
        runner_fixture("runner_replay_error_halts");
    let executor = HaltingReplayExecutor::default();
    let report = super::runner::run_combat_lab_v1_with_executor(
        &super::CombatLabRunRequestV1 {
            lab_spec_path,
            output_dir: output_dir.clone(),
            requested_samples: 2,
        },
        &executor,
    )
    .expect("record halting replay error");

    assert_eq!(report.cells_present, 1);
    assert_eq!(report.cells_appended, 1);
    assert_eq!(executor.calls.borrow().as_slice(), &[(0, "baseline")]);
    let journal = read_journal_cells(&output_dir);
    assert_eq!(journal.len(), 1);
    assert_eq!(
        journal[0].outcome_class,
        CombatLabOutcomeClassV1::ExecutionError
    );
    assert!(journal[0]
        .error
        .as_ref()
        .is_some_and(|error| error.halt_experiment));
    assert!(!output_dir.join("checkpoint.json").exists());

    fs::remove_dir_all(output_dir).expect("remove runner output");
    fs::remove_dir_all(fixture_directory).expect("remove runner fixture");
}

#[test]
fn runner_records_construction_error_without_fabricated_start_evidence() {
    let (fixture_directory, lab_spec_path, output_dir) =
        runner_fixture("runner_construction_error");
    let executor = RecordingCellExecutor::default();
    let compiled_samples = std::cell::RefCell::new(Vec::new());
    let report = super::runner::run_combat_lab_v1_with_executor_and_sample_compiler(
        &super::CombatLabRunRequestV1 {
            lab_spec_path,
            output_dir: output_dir.clone(),
            requested_samples: 3,
        },
        &executor,
        |compiler, sample_index| {
            compiled_samples.borrow_mut().push(sample_index);
            if sample_index == 1 {
                Err("injected sample isolation failure".to_string())
            } else {
                compiler.compile_sample(sample_index)
            }
        },
    )
    .expect("record construction error and stop");

    assert_eq!(compiled_samples.borrow().as_slice(), &[0, 1]);
    assert_eq!(
        executor.calls.borrow().as_slice(),
        &[(0, "baseline"), (0, "comparison")]
    );
    assert_eq!(report.cells_present, 3);
    assert_eq!(report.cells_appended, 3);
    let journal = read_journal_cells(&output_dir);
    let error_cell = journal.last().expect("construction error cell");
    assert_eq!(error_cell.sample_index, 1);
    assert_eq!(error_cell.profile_id, "baseline");
    assert_eq!(
        error_cell.outcome_class,
        CombatLabOutcomeClassV1::ExecutionError
    );
    assert!(error_cell.initial_state_fingerprint.is_none());
    assert!(error_cell.non_shuffle_rng_hash.is_none());
    assert!(error_cell.shuffle_rng_hash.is_none());
    assert!(error_cell.start_hp.is_none());
    assert!(error_cell.search_terminal.is_none());
    assert!(error_cell.coverage_status.is_none());
    assert!(error_cell.outcome_order_key.is_none());
    assert!(!error_cell.replay_validated);
    assert!(error_cell.final_hp.is_none());
    assert!(error_cell.hp_loss.is_none());
    assert!(error_cell.turns.is_none());
    assert!(error_cell.actions.is_none());
    assert!(error_cell.cards_played.is_none());
    assert!(error_cell.potions_used.is_none());
    assert_eq!(error_cell.expanded_nodes, 0);
    assert_eq!(error_cell.generated_nodes, 0);
    assert!(error_cell.nodes_to_first_win.is_none());
    assert!(!error_cell.node_budget_exhausted);
    assert!(!error_cell.deadline_exhausted);
    assert!(error_cell.action_history.is_empty());
    assert!(error_cell.draw_history.is_empty());
    let error = error_cell
        .error
        .as_ref()
        .expect("structured construction error");
    assert_eq!(
        error.stage,
        super::CombatLabCellErrorStageV1::SampleConstruction
    );
    assert_eq!(error.code, "sample_construction_or_isolation_failure");
    assert!(error.halt_experiment);
    assert!(error.message.contains("injected sample isolation failure"));
    let checkpoint: super::CombatLabCheckpointV1 = serde_json::from_slice(
        &fs::read(output_dir.join("checkpoint.json")).expect("read last complete checkpoint"),
    )
    .expect("parse checkpoint");
    assert_eq!(checkpoint.next_sample_hint, 1);

    fs::remove_dir_all(output_dir).expect("remove runner output");
    fs::remove_dir_all(fixture_directory).expect("remove runner fixture");
}

#[test]
fn cell_coverage_limits_are_never_resolved_losses() {
    use crate::ai::combat_search_v2::{SearchCoverageStatus, SearchTerminalLabel};

    for coverage in [
        SearchCoverageStatus::NodeBudgetLimited,
        SearchCoverageStatus::ActionPrefixBudgetLimited,
        SearchCoverageStatus::ActionSurfaceIncomplete,
        SearchCoverageStatus::TimeBudgetLimited,
        SearchCoverageStatus::FrontierOpen,
    ] {
        for terminal in [SearchTerminalLabel::Unresolved, SearchTerminalLabel::Loss] {
            assert_eq!(
                super::replay::classify_combat_lab_outcome_v1(coverage, Some(terminal), false,),
                super::CombatLabOutcomeClassV1::CoverageLimited
            );
        }
    }
    assert_eq!(
        super::replay::classify_combat_lab_outcome_v1(
            SearchCoverageStatus::Exhaustive,
            Some(SearchTerminalLabel::Loss),
            false,
        ),
        super::CombatLabOutcomeClassV1::ResolvedLoss
    );
}

#[test]
fn cell_replayed_win_carries_metrics_actions_and_draws() {
    use crate::ai::combat_search_v2::{replay_combat_search_witness_line_v1, SearchCoverageStatus};

    let (directory, resolved) = resolved_lab_fixture("cell_replayed_win");
    let (sample, trajectory) = replayable_win_sample();
    let stats = crate::ai::combat_search_v2::CombatSearchV2Stats {
        nodes_expanded: 11,
        nodes_generated: 17,
        nodes_to_first_win: Some(9),
        ..Default::default()
    };

    let record = super::replay::combat_lab_cell_record_from_trajectory_with_replayer_v1(
        &resolved,
        &sample,
        &resolved.profiles[0],
        SearchCoverageStatus::AcceptedCompleteCandidate,
        Some(&trajectory),
        &stats,
        replay_combat_search_witness_line_v1,
    );

    assert_eq!(
        record.outcome_class,
        super::CombatLabOutcomeClassV1::ResolvedWin
    );
    assert!(record.replay_validated);
    assert!(record.initial_state_fingerprint.is_some());
    assert_eq!(record.non_shuffle_rng_hash.as_deref(), Some("non-shuffle"));
    assert_eq!(record.shuffle_rng_hash.as_deref(), Some("shuffle"));
    assert_eq!(record.start_hp, Some(80));
    assert_eq!(record.final_hp, Some(80));
    assert_eq!(record.hp_loss, Some(0));
    assert_eq!(record.actions, Some(2));
    assert_eq!(record.action_history.len(), 2);
    assert!(!record.draw_history.is_empty());
    assert!(record.outcome_order_key.is_some());
    let candidate = record
        .replayed_candidate
        .as_ref()
        .expect("resolved win should also retain the replayed candidate");
    assert_eq!(
        candidate.final_hp,
        record.final_hp.expect("resolved final HP")
    );
    assert_eq!(candidate.hp_loss, record.hp_loss.expect("resolved HP loss"));
    assert_eq!(candidate.action_history, record.action_history);
    assert_eq!(candidate.draw_history, record.draw_history);
    assert_eq!(record.expanded_nodes, 11);
    assert_eq!(record.generated_nodes, 17);
    assert_eq!(record.nodes_to_first_win, Some(9));
    assert!(record.search_terminal.is_some());
    assert!(record.coverage_status.is_some());
    assert!(record.error.is_none());
    let trajectory_json = serde_json::to_value(&trajectory).expect("serialize trajectory report");
    assert!(trajectory_json.get("outcome_order_key").is_some());
    assert_eq!(
        crate::ai::combat_search_v2::COMBAT_SEARCH_V2_REPORT_SCHEMA_VERSION,
        21
    );

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn shared_exact_replay_adapter_returns_the_validated_candidate() {
    let (sample, trajectory) = replayable_win_sample();

    let candidate =
        super::exact_replay_combat_search_trajectory_v1(&sample.start, &trajectory, 250)
            .expect("shared exact replay should validate the complete trajectory");

    assert_eq!(candidate.terminal, trajectory.terminal);
    assert_eq!(candidate.final_hp, trajectory.final_hp);
    assert_eq!(candidate.hp_loss, trajectory.hp_loss);
    assert_eq!(candidate.actions, trajectory.actions.len());
    assert!(!candidate.draw_history.is_empty());
    assert_eq!(candidate.action_history.len(), trajectory.actions.len());
}

#[test]
fn cell_time_limited_replayed_win_retains_candidate_without_resolving() {
    use crate::ai::combat_search_v2::{replay_combat_search_witness_line_v1, SearchCoverageStatus};

    let (directory, resolved) = resolved_lab_fixture("cell_limited_win_candidate");
    let (sample, trajectory) = replayable_win_sample();
    let stats = crate::ai::combat_search_v2::CombatSearchV2Stats {
        nodes_to_first_win: Some(9),
        ..Default::default()
    };

    let record = super::replay::combat_lab_cell_record_from_trajectory_with_replayer_v1(
        &resolved,
        &sample,
        &resolved.profiles[0],
        SearchCoverageStatus::TimeBudgetLimited,
        Some(&trajectory),
        &stats,
        replay_combat_search_witness_line_v1,
    );

    assert_eq!(
        record.outcome_class,
        super::CombatLabOutcomeClassV1::CoverageLimited
    );
    assert!(record.replay_validated);
    let candidate = record
        .replayed_candidate
        .as_ref()
        .expect("replayed complete win should be retained as a candidate");
    assert_eq!(
        candidate.terminal,
        crate::ai::combat_search_v2::SearchTerminalLabel::Win
    );
    assert_eq!(candidate.final_hp, trajectory.final_hp);
    assert_eq!(candidate.hp_loss, trajectory.hp_loss);
    assert_eq!(candidate.turns, trajectory.turns);
    assert_eq!(candidate.actions, trajectory.actions.len());
    assert_eq!(candidate.cards_played, trajectory.cards_played);
    assert_eq!(candidate.potions_used, trajectory.potions_used);
    assert_eq!(candidate.action_history.len(), trajectory.actions.len());
    assert!(!candidate.draw_history.is_empty());
    assert!(record.final_hp.is_none());
    assert!(record.hp_loss.is_none());
    assert!(record.turns.is_none());
    assert!(record.actions.is_none());
    assert!(record.action_history.is_empty());
    assert!(record.draw_history.is_empty());

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn cell_time_limited_replayed_loss_retains_candidate_without_resolving() {
    use crate::ai::combat_search_v2::{replay_combat_search_witness_line_v1, SearchCoverageStatus};

    let (directory, resolved) = resolved_lab_fixture("cell_limited_loss_candidate");
    let (sample, trajectory) = replayable_loss_sample();
    let stats = crate::ai::combat_search_v2::CombatSearchV2Stats::default();

    let record = super::replay::combat_lab_cell_record_from_trajectory_with_replayer_v1(
        &resolved,
        &sample,
        &resolved.profiles[0],
        SearchCoverageStatus::TimeBudgetLimited,
        Some(&trajectory),
        &stats,
        replay_combat_search_witness_line_v1,
    );

    assert_eq!(
        record.outcome_class,
        super::CombatLabOutcomeClassV1::CoverageLimited
    );
    assert!(record.replay_validated);
    let candidate = record
        .replayed_candidate
        .as_ref()
        .expect("replayed complete loss should be retained as a candidate");
    assert_eq!(
        candidate.terminal,
        crate::ai::combat_search_v2::SearchTerminalLabel::Loss
    );
    assert_eq!(candidate.final_hp, 0);
    assert_eq!(candidate.hp_loss, 1);
    assert_eq!(candidate.actions, 1);
    assert_eq!(
        candidate.action_history,
        vec![crate::state::core::ClientInput::EndTurn]
    );
    assert!(record.final_hp.is_none());
    assert!(record.action_history.is_empty());

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn cell_without_complete_trajectory_has_no_replayed_candidate() {
    use crate::ai::combat_search_v2::SearchCoverageStatus;

    let (directory, resolved) = resolved_lab_fixture("cell_no_complete_candidate");
    let (sample, _) = replayable_win_sample();
    let stats = crate::ai::combat_search_v2::CombatSearchV2Stats::default();

    let record = super::replay::combat_lab_cell_record_from_trajectory_with_replayer_v1(
        &resolved,
        &sample,
        &resolved.profiles[0],
        SearchCoverageStatus::TimeBudgetLimited,
        None,
        &stats,
        crate::ai::combat_search_v2::replay_combat_search_witness_line_v1,
    );

    assert_eq!(
        record.outcome_class,
        super::CombatLabOutcomeClassV1::CoverageLimited
    );
    assert!(!record.replay_validated);
    assert!(record.replayed_candidate.is_none());

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn cell_replay_mismatch_is_halting_execution_error() {
    use crate::ai::combat_search_v2::SearchCoverageStatus;

    let (directory, resolved) = resolved_lab_fixture("cell_replay_mismatch");
    let (sample, trajectory) = replayable_win_sample();
    let stats = crate::ai::combat_search_v2::CombatSearchV2Stats::default();

    let record = super::replay::combat_lab_cell_record_from_trajectory_with_replayer_v1(
        &resolved,
        &sample,
        &resolved.profiles[0],
        SearchCoverageStatus::NodeBudgetLimited,
        Some(&trajectory),
        &stats,
        |_, _, _| Err("forced replay mismatch".to_string()),
    );

    assert_eq!(
        record.outcome_class,
        super::CombatLabOutcomeClassV1::ExecutionError
    );
    assert!(!record.replay_validated);
    assert!(record.search_terminal.is_some());
    assert!(record.coverage_status.is_some());
    assert!(record.outcome_order_key.is_none());
    assert!(record.replayed_candidate.is_none());
    let error = record.error.expect("replay mismatch should be recorded");
    assert_eq!(error.stage, super::CombatLabCellErrorStageV1::ExactReplay);
    assert_eq!(error.code, "exact_replay_invariant_mismatch");
    assert!(error.halt_experiment);
    assert!(error.message.contains("forced replay mismatch"));

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn cell_key_is_stable_and_commits_every_identity_input() {
    let baseline = super::combat_lab_cell_key_v1("experiment", 3, 5, "profile", "p-hash", "b-hash");
    assert_eq!(
        baseline,
        super::combat_lab_cell_key_v1("experiment", 3, 5, "profile", "p-hash", "b-hash")
    );
    for changed in [
        super::combat_lab_cell_key_v1("other", 3, 5, "profile", "p-hash", "b-hash"),
        super::combat_lab_cell_key_v1("experiment", 4, 5, "profile", "p-hash", "b-hash"),
        super::combat_lab_cell_key_v1("experiment", 3, 6, "profile", "p-hash", "b-hash"),
        super::combat_lab_cell_key_v1("experiment", 3, 5, "other", "p-hash", "b-hash"),
        super::combat_lab_cell_key_v1("experiment", 3, 5, "profile", "other", "b-hash"),
        super::combat_lab_cell_key_v1("experiment", 3, 5, "profile", "p-hash", "other"),
    ] {
        assert_ne!(baseline, changed);
    }
}

#[test]
fn summary_separates_coverage_from_loss() {
    let (directory, manifest, fingerprint) = summary_manifest("summary_profile", &["baseline"]);
    let mut cells = vec![
        summary_cell(
            &manifest,
            &fingerprint,
            0,
            "baseline",
            CombatLabOutcomeClassV1::ResolvedWin,
            Some(20),
            Some(0),
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            1,
            "baseline",
            CombatLabOutcomeClassV1::ResolvedWin,
            Some(18),
            Some(2),
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            2,
            "baseline",
            CombatLabOutcomeClassV1::ResolvedWin,
            Some(16),
            Some(4),
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            3,
            "baseline",
            CombatLabOutcomeClassV1::ResolvedWin,
            Some(10),
            Some(10),
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            4,
            "baseline",
            CombatLabOutcomeClassV1::ResolvedLoss,
            Some(0),
            Some(20),
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            5,
            "baseline",
            CombatLabOutcomeClassV1::CoverageLimited,
            Some(999),
            Some(999),
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            6,
            "baseline",
            CombatLabOutcomeClassV1::ExecutionError,
            Some(999),
            Some(999),
        ),
    ];
    for (index, cell) in cells.iter_mut().enumerate() {
        cell.turns = (index < 5).then_some((index + 1) as u32);
        cell.potions_used = (index < 5).then_some((index % 2) as u32);
        cell.expanded_nodes = (index + 1) as u64;
    }
    cells[5].node_budget_exhausted = true;
    cells[6].deadline_exhausted = true;

    let summary = summarize_combat_lab_v1(&manifest, &cells, 7).expect("summarize cells");
    assert_eq!(summary.schema_version, 1);
    assert_eq!(summary.experiment_hash, manifest.experiment_hash);
    assert_eq!(summary.requested_samples, 7);
    assert_eq!(summary.completed_cells, 7);
    assert_eq!(summary.profiles.len(), 1);

    let profile = &summary.profiles[0];
    assert_eq!(profile.profile_id, "baseline");
    assert_eq!(profile.requested_cells, 7);
    assert_eq!(profile.completed_cells, 7);
    assert_eq!(profile.resolved_cells, 5);
    assert_eq!(profile.wins, 4);
    assert_eq!(profile.losses, 1);
    assert_eq!(profile.coverage_limited, 1);
    assert_eq!(profile.errors, 1);
    assert_eq!(profile.win_rate_all_non_error_denominator, 6);
    assert_approx_eq(profile.win_rate_all_non_error, 4.0 / 6.0);
    assert_eq!(profile.win_rate_resolved_denominator, 5);
    assert_approx_eq(profile.win_rate_resolved, 4.0 / 5.0);

    assert_approx_eq(profile.hp_loss_mean, 4.0);
    assert_approx_eq(profile.hp_loss_stddev_population, 14.0_f64.sqrt());
    assert_approx_eq(profile.hp_loss_median, 3.0);
    assert_eq!(profile.hp_loss_p90_nearest_rank, Some(10));
    assert_approx_eq(profile.terminal_hp_mean, 12.8);
    assert_approx_eq(profile.terminal_hp_stddev_population, 52.16_f64.sqrt());
    assert_approx_eq(profile.terminal_hp_median, 16.0);
    assert_eq!(profile.terminal_hp_p10_nearest_rank, Some(0));

    assert_eq!(profile.turns.count, 5);
    assert_approx_eq(profile.turns.mean, 3.0);
    assert_approx_eq(profile.turns.stddev_population, 2.0_f64.sqrt());
    assert_approx_eq(profile.turns.median, 3.0);
    assert_eq!(profile.potions_used.count, 5);
    assert_approx_eq(profile.potions_used.mean, 0.4);
    assert_approx_eq(profile.potions_used.stddev_population, 0.24_f64.sqrt());
    assert_approx_eq(profile.potions_used.median, 0.0);
    assert_eq!(profile.expanded_nodes.count, 7);
    assert_approx_eq(profile.expanded_nodes.mean, 4.0);
    assert_approx_eq(profile.expanded_nodes.stddev_population, 4.0_f64.sqrt());
    assert_approx_eq(profile.expanded_nodes.median, 4.0);
    assert_approx_eq(profile.deadline_exhaustion_rate, 1.0 / 7.0);
    assert_approx_eq(profile.node_budget_exhaustion_rate, 1.0 / 7.0);

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn summary_reports_replayed_candidates_without_promoting_coverage() {
    let (directory, manifest, fingerprint) =
        summary_manifest("summary_candidates", &["baseline", "empty"]);
    let mut cells = vec![
        summary_cell(
            &manifest,
            &fingerprint,
            0,
            "baseline",
            CombatLabOutcomeClassV1::CoverageLimited,
            None,
            None,
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            1,
            "baseline",
            CombatLabOutcomeClassV1::CoverageLimited,
            None,
            None,
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            2,
            "baseline",
            CombatLabOutcomeClassV1::CoverageLimited,
            None,
            None,
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            3,
            "baseline",
            CombatLabOutcomeClassV1::ExecutionError,
            None,
            None,
        ),
        summary_cell(
            &manifest,
            &fingerprint,
            4,
            "baseline",
            CombatLabOutcomeClassV1::ResolvedWin,
            Some(18),
            Some(2),
        ),
    ];
    cells[0].replayed_candidate = Some(summary_candidate(
        crate::ai::combat_search_v2::SearchTerminalLabel::Win,
        15,
        5,
        3,
        1,
    ));
    cells[0].nodes_to_first_win = Some(100);
    cells[1].replayed_candidate = Some(summary_candidate(
        crate::ai::combat_search_v2::SearchTerminalLabel::Loss,
        0,
        20,
        4,
        0,
    ));
    cells[4].replayed_candidate = Some(summary_candidate(
        crate::ai::combat_search_v2::SearchTerminalLabel::Win,
        18,
        2,
        2,
        0,
    ));
    cells[4].nodes_to_first_win = Some(50);

    let summary = summarize_combat_lab_v1(&manifest, &cells, 5).expect("summarize candidates");
    let baseline = &summary.profiles[0];
    assert_eq!(baseline.resolved_cells, 1);
    assert_eq!(baseline.wins, 1);
    assert_eq!(baseline.losses, 0);
    assert_eq!(baseline.coverage_limited, 3);
    assert_eq!(baseline.errors, 1);

    let candidates = &baseline.candidates;
    assert_eq!(candidates.replayed_complete_candidates, 3);
    assert_eq!(candidates.replayed_win_candidates, 2);
    assert_eq!(candidates.replayed_loss_candidates, 1);
    assert_eq!(candidates.replayed_win_rate_all_non_error_denominator, 4);
    assert_approx_eq(candidates.replayed_win_rate_all_non_error, 0.5);
    assert_eq!(candidates.win_hp_loss.count, 2);
    assert_approx_eq(candidates.win_hp_loss.mean, 3.5);
    assert_approx_eq(candidates.win_hp_loss.stddev_population, 1.5);
    assert_approx_eq(candidates.win_hp_loss.median, 3.5);
    assert_eq!(candidates.terminal_hp.count, 3);
    assert_approx_eq(candidates.terminal_hp.mean, 11.0);
    assert_eq!(candidates.turns.count, 3);
    assert_approx_eq(candidates.turns.mean, 3.0);
    assert_eq!(candidates.potions_used.count, 3);
    assert_approx_eq(candidates.potions_used.mean, 1.0 / 3.0);
    assert_eq!(candidates.nodes_to_first_win.count, 2);
    assert_approx_eq(candidates.nodes_to_first_win.mean, 75.0);

    let empty = &summary.profiles[1].candidates;
    assert_eq!(empty.replayed_complete_candidates, 0);
    assert_eq!(empty.replayed_win_candidates, 0);
    assert_eq!(empty.replayed_loss_candidates, 0);
    assert_eq!(empty.replayed_win_rate_all_non_error_denominator, 0);
    assert_eq!(empty.replayed_win_rate_all_non_error, None);
    for numeric in [
        &empty.win_hp_loss,
        &empty.terminal_hp,
        &empty.turns,
        &empty.potions_used,
        &empty.nodes_to_first_win,
    ] {
        assert_eq!(numeric.count, 0);
        assert_eq!(numeric.mean, None);
        assert_eq!(numeric.stddev_population, None);
        assert_eq!(numeric.median, None);
    }

    assert_eq!(summary.pairs[0].shared_samples, 0);
    assert_eq!(summary.pairs[0].comparable_resolved_samples, 0);
    assert!(summary.interaction.is_none());

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn summary_historical_cell_without_candidate_field_contributes_zero_candidate_evidence() {
    let (directory, manifest, fingerprint) =
        summary_manifest("summary_historical_candidate", &["baseline"]);
    let mut historical = summary_cell(
        &manifest,
        &fingerprint,
        0,
        "baseline",
        CombatLabOutcomeClassV1::CoverageLimited,
        None,
        None,
    );
    historical.search_terminal = Some(crate::ai::combat_search_v2::SearchTerminalLabel::Win);
    historical.coverage_status =
        Some(crate::ai::combat_search_v2::SearchCoverageStatus::TimeBudgetLimited);
    historical.replay_validated = true;
    let encoded = serde_json::to_value(&historical).expect("serialize historical cell shape");
    assert!(encoded.get("replayed_candidate").is_none());
    let decoded: CombatLabCellRecordV1 =
        serde_json::from_value(encoded).expect("historical cell should remain readable");
    assert!(decoded.replayed_candidate.is_none());

    let summary = summarize_combat_lab_v1(&manifest, &[decoded], 1)
        .expect("summarize historical candidate-less cell");
    let profile = &summary.profiles[0];
    assert_eq!(profile.coverage_limited, 1);
    assert_eq!(profile.resolved_cells, 0);
    assert_eq!(profile.candidates.replayed_complete_candidates, 0);
    assert_eq!(profile.candidates.replayed_win_candidates, 0);
    assert_eq!(profile.candidates.replayed_loss_candidates, 0);
    assert_eq!(
        profile
            .candidates
            .replayed_win_rate_all_non_error_denominator,
        1
    );
    assert_approx_eq(profile.candidates.replayed_win_rate_all_non_error, 0.0);
    for numeric in [
        &profile.candidates.win_hp_loss,
        &profile.candidates.terminal_hp,
        &profile.candidates.turns,
        &profile.candidates.potions_used,
        &profile.candidates.nodes_to_first_win,
    ] {
        assert_eq!(numeric.count, 0);
        assert_eq!(numeric.mean, None);
    }

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn summary_pairs_use_only_shared_samples_and_report_divergences() {
    use crate::content::cards::CardId;
    use crate::state::core::ClientInput;
    use crate::state::DomainCardSnapshot;

    let (directory, manifest, fingerprint) =
        summary_manifest("summary_pairs", &["zeta", "alpha", "mid"]);
    let mut cells = Vec::new();
    for (sample_index, left_outcome, left_hp, right_outcome, right_hp) in [
        (
            0,
            CombatLabOutcomeClassV1::ResolvedWin,
            20,
            CombatLabOutcomeClassV1::ResolvedWin,
            18,
        ),
        (
            1,
            CombatLabOutcomeClassV1::ResolvedWin,
            15,
            CombatLabOutcomeClassV1::ResolvedLoss,
            0,
        ),
        (
            2,
            CombatLabOutcomeClassV1::ResolvedLoss,
            0,
            CombatLabOutcomeClassV1::ResolvedWin,
            12,
        ),
        (
            3,
            CombatLabOutcomeClassV1::ResolvedLoss,
            0,
            CombatLabOutcomeClassV1::ResolvedLoss,
            0,
        ),
        (
            4,
            CombatLabOutcomeClassV1::CoverageLimited,
            999,
            CombatLabOutcomeClassV1::ResolvedWin,
            10,
        ),
    ] {
        cells.push(summary_cell(
            &manifest,
            &fingerprint,
            sample_index,
            "zeta",
            left_outcome,
            Some(left_hp),
            Some(20 - left_hp),
        ));
        cells.push(summary_cell(
            &manifest,
            &fingerprint,
            sample_index,
            "alpha",
            right_outcome,
            Some(right_hp),
            Some(20 - right_hp),
        ));
    }
    cells.push(summary_cell(
        &manifest,
        &fingerprint,
        5,
        "zeta",
        CombatLabOutcomeClassV1::ResolvedWin,
        Some(11),
        Some(9),
    ));
    cells.push(summary_cell(
        &manifest,
        &fingerprint,
        6,
        "alpha",
        CombatLabOutcomeClassV1::ResolvedWin,
        Some(13),
        Some(7),
    ));

    summary_cell_mut(&mut cells, 0, "zeta").action_history =
        vec![ClientInput::EndTurn, ClientInput::Proceed];
    summary_cell_mut(&mut cells, 0, "alpha").action_history =
        vec![ClientInput::EndTurn, ClientInput::DiscardPotion(0)];
    summary_cell_mut(&mut cells, 0, "zeta").draw_history = vec![DomainCardSnapshot {
        id: CardId::Bash,
        upgrades: 0,
        lineage: crate::test_support::scenario_lineage(1),
    }];
    summary_cell_mut(&mut cells, 0, "alpha").draw_history = vec![DomainCardSnapshot {
        id: CardId::Defend,
        upgrades: 0,
        lineage: crate::test_support::scenario_lineage(2),
    }];
    summary_cell_mut(&mut cells, 1, "zeta").action_history = vec![ClientInput::EndTurn];
    summary_cell_mut(&mut cells, 1, "alpha").action_history =
        vec![ClientInput::EndTurn, ClientInput::Proceed];
    summary_cell_mut(&mut cells, 2, "zeta").action_history = vec![ClientInput::EndTurn];
    summary_cell_mut(&mut cells, 2, "alpha").action_history = vec![ClientInput::EndTurn];
    summary_cell_mut(&mut cells, 3, "zeta").action_history = vec![ClientInput::EndTurn];
    summary_cell_mut(&mut cells, 3, "alpha").action_history = vec![ClientInput::Proceed];
    summary_cell_mut(&mut cells, 3, "zeta").replay_validated = false;
    summary_cell_mut(&mut cells, 4, "zeta").action_history = vec![ClientInput::EndTurn];
    summary_cell_mut(&mut cells, 4, "alpha").action_history = vec![ClientInput::Proceed];
    cells.reverse();

    let summary = summarize_combat_lab_v1(&manifest, &cells, 7).expect("summarize pairs");
    assert_eq!(
        summary
            .profiles
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect::<Vec<_>>(),
        vec!["zeta", "alpha", "mid"]
    );
    assert_eq!(
        summary
            .pairs
            .iter()
            .map(|pair| (
                pair.left_profile_id.as_str(),
                pair.right_profile_id.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![("zeta", "alpha"), ("zeta", "mid"), ("alpha", "mid")]
    );

    let pair = &summary.pairs[0];
    assert_eq!(pair.shared_samples, 5);
    assert_eq!(pair.incomplete_pair_samples, 2);
    assert_eq!(summary.pairs[1].incomplete_pair_samples, 7);
    assert_eq!(summary.pairs[2].incomplete_pair_samples, 7);
    assert_eq!(pair.both_win, 1);
    assert_eq!(pair.left_only_win, 1);
    assert_eq!(pair.right_only_win, 1);
    assert_eq!(pair.both_loss, 1);
    assert_eq!(pair.unresolved_or_error, 1);
    assert_eq!(pair.comparable_resolved_samples, 4);
    assert_eq!(pair.final_hp_delta_left_minus_right.count, 4);
    assert_approx_eq(pair.final_hp_delta_left_minus_right.mean, 1.25);
    assert_approx_eq(
        pair.final_hp_delta_left_minus_right.stddev_population,
        91.6875_f64.sqrt(),
    );
    assert_approx_eq(pair.final_hp_delta_left_minus_right.median, 1.0);
    assert_eq!(pair.hp_loss_delta_left_minus_right.count, 4);
    assert_approx_eq(pair.hp_loss_delta_left_minus_right.mean, -1.25);
    assert_approx_eq(
        pair.hp_loss_delta_left_minus_right.stddev_population,
        91.6875_f64.sqrt(),
    );
    assert_approx_eq(pair.hp_loss_delta_left_minus_right.median, -1.0);
    assert_eq!(pair.left_strictly_better, 2);
    assert_eq!(pair.right_strictly_better, 1);
    assert_eq!(pair.tied, 1);
    assert_eq!(pair.divergences.len(), 2);
    assert_eq!(pair.divergences[0].sample_index, 0);
    assert_eq!(pair.divergences[0].first_action_divergence, Some(1));
    assert_eq!(pair.divergences[0].first_draw_divergence, Some(0));
    assert_eq!(pair.divergences[1].sample_index, 1);
    assert_eq!(pair.divergences[1].first_action_divergence, Some(1));
    assert_eq!(pair.divergences[1].first_draw_divergence, None);

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn summary_pairs_count_requested_samples_missing_from_both_profiles() {
    let (directory, manifest, fingerprint) =
        summary_manifest("summary_pairs_both_missing", &["left", "right"]);
    let cells = ["left", "right"]
        .into_iter()
        .map(|profile_id| {
            summary_cell(
                &manifest,
                &fingerprint,
                0,
                profile_id,
                CombatLabOutcomeClassV1::ResolvedWin,
                Some(20),
                Some(0),
            )
        })
        .collect::<Vec<_>>();

    let summary = summarize_combat_lab_v1(&manifest, &cells, 3).expect("summarize pair gaps");
    assert_eq!(summary.pairs[0].shared_samples, 1);
    assert_eq!(summary.pairs[0].incomplete_pair_samples, 2);

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn interaction_decomposes_balanced_terminal_hp() {
    let (directory, manifest, fingerprint) =
        summary_manifest("interaction_balanced", &["left", "right"]);
    let cells = [
        (0, "left", 10),
        (0, "right", 20),
        (1, "left", 30),
        (1, "right", 20),
    ]
    .into_iter()
    .map(|(sample_index, profile_id, final_hp)| {
        summary_cell(
            &manifest,
            &fingerprint,
            sample_index,
            profile_id,
            CombatLabOutcomeClassV1::ResolvedWin,
            Some(final_hp),
            Some(30 - final_hp),
        )
    })
    .collect::<Vec<_>>();

    let summary = summarize_combat_lab_v1(&manifest, &cells, 2).expect("summarize interaction");
    assert_eq!(summary.interaction_omitted_reason, None);
    let interaction = summary.interaction.expect("balanced decomposition");
    assert_eq!(interaction.eligible_samples, 2);
    assert_eq!(interaction.profile_count, 2);
    assert_eq!(interaction.total_sum_squares, 200.0);
    assert_eq!(interaction.shuffle_sum_squares, 100.0);
    assert_eq!(interaction.profile_sum_squares, 0.0);
    assert_eq!(interaction.interaction_sum_squares, 100.0);
    assert_approx_eq(interaction.shuffle_share, 0.5);
    assert_approx_eq(interaction.profile_share, 0.0);
    assert_approx_eq(interaction.interaction_share, 0.5);

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn interaction_omits_unbalanced_matrices_with_precise_reasons() {
    let (one_directory, one_manifest, _one_fingerprint) =
        summary_manifest("interaction_one_profile", &["only"]);
    let one_profile =
        summarize_combat_lab_v1(&one_manifest, &[], 2).expect("summarize one-profile interaction");
    assert!(one_profile.interaction.is_none());
    assert_eq!(
        one_profile.interaction_omitted_reason.as_deref(),
        Some("interaction requires at least two profiles")
    );

    let (zero_directory, zero_manifest, zero_fingerprint) =
        summary_manifest("interaction_zero_balanced", &["left", "right"]);
    let zero_cells = vec![
        summary_cell(
            &zero_manifest,
            &zero_fingerprint,
            0,
            "left",
            CombatLabOutcomeClassV1::ResolvedWin,
            Some(10),
            Some(10),
        ),
        summary_cell(
            &zero_manifest,
            &zero_fingerprint,
            0,
            "right",
            CombatLabOutcomeClassV1::CoverageLimited,
            None,
            None,
        ),
    ];
    let zero_balanced = summarize_combat_lab_v1(&zero_manifest, &zero_cells, 2)
        .expect("summarize zero-balanced interaction");
    assert!(zero_balanced.interaction.is_none());
    assert_eq!(
        zero_balanced.interaction_omitted_reason.as_deref(),
        Some(
            "interaction requires a balanced resolved matrix; found 0 fully resolved shared samples"
        )
    );

    let (one_sample_directory, one_sample_manifest, one_sample_fingerprint) =
        summary_manifest("interaction_one_balanced", &["left", "right"]);
    let one_sample_cells = ["left", "right"]
        .into_iter()
        .map(|profile_id| {
            summary_cell(
                &one_sample_manifest,
                &one_sample_fingerprint,
                0,
                profile_id,
                CombatLabOutcomeClassV1::ResolvedWin,
                Some(10),
                Some(10),
            )
        })
        .collect::<Vec<_>>();
    let one_balanced = summarize_combat_lab_v1(&one_sample_manifest, &one_sample_cells, 2)
        .expect("summarize one-balanced interaction");
    assert!(one_balanced.interaction.is_none());
    assert_eq!(
        one_balanced.interaction_omitted_reason.as_deref(),
        Some("interaction requires at least two balanced resolved samples; found 1")
    );

    for directory in [one_directory, zero_directory, one_sample_directory] {
        fs::remove_dir_all(directory).expect("remove test directory");
    }
}

#[test]
fn summary_rejects_cells_from_another_experiment() {
    let (directory, manifest, fingerprint) =
        summary_manifest("summary_foreign_experiment", &["baseline"]);
    let mut foreign = summary_cell(
        &manifest,
        &fingerprint,
        0,
        "baseline",
        CombatLabOutcomeClassV1::ResolvedWin,
        Some(20),
        Some(0),
    );
    foreign.experiment_hash = "foreign-experiment".to_string();

    let error = summarize_combat_lab_v1(&manifest, &[foreign], 1)
        .expect_err("foreign cell must be rejected");
    assert!(error.contains("foreign experiment hash"), "{error}");
    assert!(error.contains("foreign-experiment"), "{error}");
    assert!(error.contains(&manifest.experiment_hash), "{error}");

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn summary_json_is_byte_identical_across_regeneration() {
    let (directory, manifest, fingerprint) =
        summary_manifest("summary_deterministic_json", &["left", "right"]);
    let cells = [
        (1, "right", 18),
        (0, "left", 20),
        (1, "left", 16),
        (0, "right", 14),
    ]
    .into_iter()
    .map(|(sample_index, profile_id, final_hp)| {
        summary_cell(
            &manifest,
            &fingerprint,
            sample_index,
            profile_id,
            CombatLabOutcomeClassV1::ResolvedWin,
            Some(final_hp),
            Some(20 - final_hp),
        )
    })
    .collect::<Vec<_>>();

    let first = summarize_combat_lab_v1(&manifest, &cells, 2).expect("first regeneration");
    let second = summarize_combat_lab_v1(&manifest, &cells, 2).expect("second regeneration");
    let first_json = serde_json::to_vec(&first).expect("serialize first compact summary");
    let second_json = serde_json::to_vec(&second).expect("serialize second compact summary");
    assert_eq!(first_json, second_json);
    assert!(!first_json
        .windows(b"created_at".len())
        .any(|window| window == b"created_at"));

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn interaction_zero_variation_has_no_shares() {
    let (directory, manifest, fingerprint) =
        summary_manifest("interaction_zero_variation", &["left", "right"]);
    let cells = [(0, "left"), (0, "right"), (1, "left"), (1, "right")]
        .into_iter()
        .map(|(sample_index, profile_id)| {
            summary_cell(
                &manifest,
                &fingerprint,
                sample_index,
                profile_id,
                CombatLabOutcomeClassV1::ResolvedWin,
                Some(10),
                Some(10),
            )
        })
        .collect::<Vec<_>>();

    let interaction = summarize_combat_lab_v1(&manifest, &cells, 2)
        .expect("summarize zero variation")
        .interaction
        .expect("zero-variation decomposition");
    assert_eq!(interaction.total_sum_squares, 0.0);
    assert_eq!(interaction.shuffle_sum_squares, 0.0);
    assert_eq!(interaction.profile_sum_squares, 0.0);
    assert_eq!(interaction.interaction_sum_squares, 0.0);
    assert_eq!(interaction.shuffle_share, None);
    assert_eq!(interaction.profile_share, None);
    assert_eq!(interaction.interaction_share, None);

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn summary_empty_distributions_expose_zero_denominators() {
    let (directory, manifest, _fingerprint) =
        summary_manifest("summary_empty_distributions", &["empty"]);

    let summary = summarize_combat_lab_v1(&manifest, &[], 3).expect("summarize empty profile");
    assert_eq!(summary.completed_cells, 0);
    let profile = &summary.profiles[0];
    assert_eq!(profile.requested_cells, 3);
    assert_eq!(profile.completed_cells, 0);
    assert_eq!(profile.win_rate_all_non_error_denominator, 0);
    assert_eq!(profile.win_rate_all_non_error, None);
    assert_eq!(profile.win_rate_resolved_denominator, 0);
    assert_eq!(profile.win_rate_resolved, None);
    assert_eq!(profile.hp_loss_mean, None);
    assert_eq!(profile.hp_loss_stddev_population, None);
    assert_eq!(profile.hp_loss_median, None);
    assert_eq!(profile.hp_loss_p90_nearest_rank, None);
    assert_eq!(profile.terminal_hp_mean, None);
    assert_eq!(profile.terminal_hp_stddev_population, None);
    assert_eq!(profile.terminal_hp_median, None);
    assert_eq!(profile.terminal_hp_p10_nearest_rank, None);
    for numeric in [
        &profile.turns,
        &profile.potions_used,
        &profile.expanded_nodes,
    ] {
        assert_eq!(numeric.count, 0);
        assert_eq!(numeric.mean, None);
        assert_eq!(numeric.stddev_population, None);
        assert_eq!(numeric.median, None);
    }
    assert_eq!(profile.deadline_exhaustion_rate, None);
    assert_eq!(profile.node_budget_exhaustion_rate, None);

    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn schedule_is_frozen() {
    let schedule = CombatLabShuffleScheduleV1 {
        generator: CombatLabShuffleGeneratorV1::SplitMix64V1,
        seed: 42,
    };

    assert_eq!(
        derive_shuffle_seed_v1(&schedule, 0),
        13_679_457_532_755_275_413
    );
    assert_eq!(
        derive_shuffle_seed_v1(&schedule, 1),
        2_949_826_092_126_892_291
    );
    assert_eq!(
        derive_shuffle_seed_v1(&schedule, 2),
        5_139_283_748_462_763_858
    );
}

#[test]
fn profile_local_resource_budget_is_rejected() {
    let mut value = valid_lab_spec_value();
    value["profiles"][0]["max_nodes"] = json!(123);

    let error = serde_json::from_value::<CombatLabSpecV1>(value)
        .expect_err("profile-local resource budgets must be rejected");

    assert!(error.to_string().contains("unknown field `max_nodes`"));
}

#[test]
fn empty_profile_list_is_rejected() {
    let directory = test_directory("empty_profiles");
    fs::create_dir_all(&directory).expect("create test directory");
    let mut value = valid_lab_spec_value();
    value["profiles"] = json!([]);
    let lab_spec_path = directory.join("lab.json");
    fs::write(
        &lab_spec_path,
        serde_json::to_vec(&value).expect("serialize lab spec"),
    )
    .expect("write lab spec");

    let error = load_and_resolve_combat_lab_spec_v1(&lab_spec_path)
        .expect_err("an experiment must contain at least one profile");

    assert!(error.contains("at least one profile"), "{error}");
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn duplicate_profile_ids_are_rejected() {
    let directory = test_directory("duplicate_profiles");
    fs::create_dir_all(&directory).expect("create test directory");
    let mut value = valid_lab_spec_value();
    let profile = value["profiles"][0].clone();
    value["profiles"] = json!([profile.clone(), profile]);
    let lab_spec_path = directory.join("lab.json");
    fs::write(
        &lab_spec_path,
        serde_json::to_vec(&value).expect("serialize lab spec"),
    )
    .expect("write lab spec");

    let error = load_and_resolve_combat_lab_spec_v1(&lab_spec_path)
        .expect_err("profile IDs must be unique");

    assert!(error.contains("duplicate profile id 'baseline'"), "{error}");
    fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn canonical_hashes_ignore_json_key_order_and_provenance_paths() {
    let first_directory = test_directory("canonical_hashes_first");
    let second_directory = test_directory("canonical_hashes_second");
    fs::create_dir_all(&first_directory).expect("create first test directory");
    fs::create_dir_all(&second_directory).expect("create second test directory");
    let lab_value = valid_lab_spec_value();
    let start_value = valid_start_spec_value();
    let ordinary_lab_json = serde_json::to_string_pretty(&lab_value).expect("serialize lab spec");
    let reordered_lab_json = json_with_reversed_object_keys(&lab_value);
    let ordinary_start_json =
        serde_json::to_string_pretty(&start_value).expect("serialize start spec");
    let reordered_start_json = json_with_reversed_object_keys(&start_value);
    assert_ne!(ordinary_lab_json, reordered_lab_json);
    assert_ne!(ordinary_start_json, reordered_start_json);
    fs::write(first_directory.join("lab.json"), ordinary_lab_json).expect("write first lab");
    fs::write(second_directory.join("lab.json"), reordered_lab_json).expect("write second lab");
    fs::write(first_directory.join("start.json"), ordinary_start_json).expect("write first start");
    fs::write(second_directory.join("start.json"), reordered_start_json)
        .expect("write second start");

    let first = load_and_resolve_combat_lab_spec_v1(&first_directory.join("lab.json"))
        .expect("resolve first spec");
    let second = load_and_resolve_combat_lab_spec_v1(&second_directory.join("lab.json"))
        .expect("resolve reordered spec");

    assert_eq!(first.scenario_hash, second.scenario_hash);
    assert_eq!(
        first.profiles[0].profile_hash,
        second.profiles[0].profile_hash
    );
    assert_eq!(first.budget_hash, second.budget_hash);
    assert_eq!(first.experiment_hash, second.experiment_hash);
    for hash in [
        &first.scenario_hash,
        &first.profiles[0].profile_hash,
        &first.budget_hash,
        &first.experiment_hash,
    ] {
        assert_eq!(hash.len(), 64);
        assert!(
            hash.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "hash must use lowercase hex: {hash}"
        );
    }
    assert_ne!(first.lab_spec_path, second.lab_spec_path);
    assert_ne!(first.start_spec_path, second.start_spec_path);
    fs::remove_dir_all(first_directory).expect("remove first test directory");
    fs::remove_dir_all(second_directory).expect("remove second test directory");
}

#[test]
fn changed_start_snapshot_changes_scenario_and_experiment_hashes() {
    let first_directory = test_directory("scenario_hash_first");
    let second_directory = test_directory("scenario_hash_second");
    fs::create_dir_all(&first_directory).expect("create first test directory");
    fs::create_dir_all(&second_directory).expect("create second test directory");
    let lab_value = valid_lab_spec_value();
    let first_start = valid_start_spec_value();
    let mut second_start = first_start.clone();
    second_start["player_current_hp"] = json!(71);
    write_json(&first_directory.join("lab.json"), &lab_value);
    write_json(&second_directory.join("lab.json"), &lab_value);
    write_json(&first_directory.join("start.json"), &first_start);
    write_json(&second_directory.join("start.json"), &second_start);

    let first = load_and_resolve_combat_lab_spec_v1(&first_directory.join("lab.json"))
        .expect("resolve first spec");
    let second = load_and_resolve_combat_lab_spec_v1(&second_directory.join("lab.json"))
        .expect("resolve second spec");

    assert_ne!(first.scenario_hash, second.scenario_hash);
    assert_ne!(first.experiment_hash, second.experiment_hash);
    assert_eq!(
        first.profiles[0].profile_hash,
        second.profiles[0].profile_hash
    );
    assert_eq!(first.budget_hash, second.budget_hash);
    fs::remove_dir_all(first_directory).expect("remove first test directory");
    fs::remove_dir_all(second_directory).expect("remove second test directory");
}

#[test]
fn changed_profile_policy_changes_profile_and_experiment_hashes() {
    let first_directory = test_directory("profile_hash_first");
    let second_directory = test_directory("profile_hash_second");
    fs::create_dir_all(&first_directory).expect("create first test directory");
    fs::create_dir_all(&second_directory).expect("create second test directory");
    let first_lab = valid_lab_spec_value();
    let mut second_lab = first_lab.clone();
    second_lab["profiles"][0]["rollout_policy"] = json!("turn_beam_no_potion");
    let start = valid_start_spec_value();
    write_json(&first_directory.join("lab.json"), &first_lab);
    write_json(&second_directory.join("lab.json"), &second_lab);
    write_json(&first_directory.join("start.json"), &start);
    write_json(&second_directory.join("start.json"), &start);

    let first = load_and_resolve_combat_lab_spec_v1(&first_directory.join("lab.json"))
        .expect("resolve first spec");
    let second = load_and_resolve_combat_lab_spec_v1(&second_directory.join("lab.json"))
        .expect("resolve second spec");

    assert_ne!(
        first.profiles[0].profile_hash,
        second.profiles[0].profile_hash
    );
    assert_ne!(first.experiment_hash, second.experiment_hash);
    assert_eq!(first.scenario_hash, second.scenario_hash);
    assert_eq!(first.budget_hash, second.budget_hash);
    fs::remove_dir_all(first_directory).expect("remove first test directory");
    fs::remove_dir_all(second_directory).expect("remove second test directory");
}

#[test]
fn every_serialized_profile_policy_is_hashed() {
    let baseline_directory = test_directory("all_profile_policies_baseline");
    fs::create_dir_all(&baseline_directory).expect("create baseline test directory");
    let baseline_lab = valid_lab_spec_value();
    let start = valid_start_spec_value();
    write_json(&baseline_directory.join("lab.json"), &baseline_lab);
    write_json(&baseline_directory.join("start.json"), &start);
    let baseline = load_and_resolve_combat_lab_spec_v1(&baseline_directory.join("lab.json"))
        .expect("resolve baseline spec");

    for (field, changed_value) in [
        ("potion_policy", "all"),
        ("child_rollout_policy", "lazy_on_pop"),
        ("turn_plan_policy", "root_frontier_seed"),
        ("phase_guard_policy", "champ_split_guard"),
        ("setup_bias_policy", "key_card_online"),
    ] {
        let directory = test_directory(field);
        fs::create_dir_all(&directory).expect("create variant test directory");
        let mut lab = baseline_lab.clone();
        lab["profiles"][0][field] = json!(changed_value);
        write_json(&directory.join("lab.json"), &lab);
        write_json(&directory.join("start.json"), &start);
        let changed = load_and_resolve_combat_lab_spec_v1(&directory.join("lab.json"))
            .expect("resolve changed profile policy");

        assert_ne!(
            baseline.profiles[0].profile_hash, changed.profiles[0].profile_hash,
            "profile hash omitted {field}"
        );
        fs::remove_dir_all(directory).expect("remove variant test directory");
    }

    fs::remove_dir_all(baseline_directory).expect("remove baseline test directory");
}

#[test]
fn changed_common_budget_changes_budget_and_experiment_hashes() {
    let first_directory = test_directory("budget_hash_first");
    let second_directory = test_directory("budget_hash_second");
    fs::create_dir_all(&first_directory).expect("create first test directory");
    fs::create_dir_all(&second_directory).expect("create second test directory");
    let first_lab = valid_lab_spec_value();
    let mut second_lab = first_lab.clone();
    second_lab["common_budget"]["max_nodes"] = json!(1001);
    let start = valid_start_spec_value();
    write_json(&first_directory.join("lab.json"), &first_lab);
    write_json(&second_directory.join("lab.json"), &second_lab);
    write_json(&first_directory.join("start.json"), &start);
    write_json(&second_directory.join("start.json"), &start);

    let first = load_and_resolve_combat_lab_spec_v1(&first_directory.join("lab.json"))
        .expect("resolve first spec");
    let second = load_and_resolve_combat_lab_spec_v1(&second_directory.join("lab.json"))
        .expect("resolve second spec");

    assert_ne!(first.budget_hash, second.budget_hash);
    assert_ne!(first.experiment_hash, second.experiment_hash);
    assert_eq!(first.scenario_hash, second.scenario_hash);
    assert_eq!(
        first.profiles[0].profile_hash,
        second.profiles[0].profile_hash
    );
    fs::remove_dir_all(first_directory).expect("remove first test directory");
    fs::remove_dir_all(second_directory).expect("remove second test directory");
}

#[test]
fn every_common_budget_field_is_hashed() {
    let baseline_directory = test_directory("all_budget_fields_baseline");
    fs::create_dir_all(&baseline_directory).expect("create baseline test directory");
    let baseline_lab = valid_lab_spec_value();
    let start = valid_start_spec_value();
    write_json(&baseline_directory.join("lab.json"), &baseline_lab);
    write_json(&baseline_directory.join("start.json"), &start);
    let baseline = load_and_resolve_combat_lab_spec_v1(&baseline_directory.join("lab.json"))
        .expect("resolve baseline spec");

    for (field, changed_value) in [
        ("max_actions_per_line", json!(101)),
        ("max_engine_steps_per_action", json!(201)),
        ("wall_ms", json!(1)),
        ("satisfaction", json!({"hp_loss_at_most": 2})),
        ("max_potions_used", json!(1)),
        ("rollout_max_evaluations", json!(11)),
        ("rollout_max_actions", json!(21)),
        ("rollout_beam_width", json!(4)),
        ("turn_plan_probe_max_inner_nodes", json!(5)),
        ("turn_plan_probe_max_end_states", json!(6)),
        ("turn_plan_probe_per_bucket_limit", json!(7)),
    ] {
        let directory = test_directory(field);
        fs::create_dir_all(&directory).expect("create variant test directory");
        let mut lab = baseline_lab.clone();
        lab["common_budget"][field] = changed_value;
        write_json(&directory.join("lab.json"), &lab);
        write_json(&directory.join("start.json"), &start);
        let changed = load_and_resolve_combat_lab_spec_v1(&directory.join("lab.json"))
            .expect("resolve changed common budget");

        assert_ne!(
            baseline.budget_hash, changed.budget_hash,
            "budget hash omitted {field}"
        );
        fs::remove_dir_all(directory).expect("remove variant test directory");
    }

    fs::remove_dir_all(baseline_directory).expect("remove baseline test directory");
}

#[test]
fn profile_identity_is_excluded_from_profile_hash_but_included_in_experiment_hash() {
    let first_directory = test_directory("profile_identity_first");
    let second_directory = test_directory("profile_identity_second");
    fs::create_dir_all(&first_directory).expect("create first test directory");
    fs::create_dir_all(&second_directory).expect("create second test directory");
    let first_lab = valid_lab_spec_value();
    let mut second_lab = first_lab.clone();
    second_lab["profiles"][0]["id"] = json!("renamed");
    second_lab["profiles"][0]["label"] = json!("Renamed Profile");
    let start = valid_start_spec_value();
    write_json(&first_directory.join("lab.json"), &first_lab);
    write_json(&second_directory.join("lab.json"), &second_lab);
    write_json(&first_directory.join("start.json"), &start);
    write_json(&second_directory.join("start.json"), &start);

    let first = load_and_resolve_combat_lab_spec_v1(&first_directory.join("lab.json"))
        .expect("resolve first spec");
    let second = load_and_resolve_combat_lab_spec_v1(&second_directory.join("lab.json"))
        .expect("resolve second spec");

    assert_eq!(
        first.profiles[0].profile_hash,
        second.profiles[0].profile_hash
    );
    assert_ne!(first.experiment_hash, second.experiment_hash);
    fs::remove_dir_all(first_directory).expect("remove first test directory");
    fs::remove_dir_all(second_directory).expect("remove second test directory");
}

#[test]
fn experiment_identity_and_schedule_are_hashed() {
    let baseline_directory = test_directory("experiment_identity_baseline");
    fs::create_dir_all(&baseline_directory).expect("create baseline test directory");
    let baseline_lab = valid_lab_spec_value();
    let start = valid_start_spec_value();
    write_json(&baseline_directory.join("lab.json"), &baseline_lab);
    write_json(&baseline_directory.join("start.json"), &start);
    let baseline = load_and_resolve_combat_lab_spec_v1(&baseline_directory.join("lab.json"))
        .expect("resolve baseline spec");

    for (label, pointer, changed_value) in [
        (
            "experiment_id",
            "/experiment_id",
            json!("experiment_changed"),
        ),
        ("scenario_id", "/scenario_id", json!("scenario_changed")),
        ("schedule_seed", "/schedule/seed", json!(43)),
    ] {
        let directory = test_directory(label);
        fs::create_dir_all(&directory).expect("create variant test directory");
        let mut lab = baseline_lab.clone();
        *lab.pointer_mut(pointer).expect("fixture path should exist") = changed_value;
        write_json(&directory.join("lab.json"), &lab);
        write_json(&directory.join("start.json"), &start);
        let changed = load_and_resolve_combat_lab_spec_v1(&directory.join("lab.json"))
            .expect("resolve changed experiment identity");

        assert_ne!(
            baseline.experiment_hash, changed.experiment_hash,
            "experiment hash omitted {label}"
        );
        assert_eq!(baseline.scenario_hash, changed.scenario_hash);
        assert_eq!(baseline.budget_hash, changed.budget_hash);
        assert_eq!(
            baseline.profiles[0].profile_hash,
            changed.profiles[0].profile_hash
        );
        fs::remove_dir_all(directory).expect("remove variant test directory");
    }

    fs::remove_dir_all(baseline_directory).expect("remove baseline test directory");
}

#[test]
fn profile_config_assigns_the_complete_contract() {
    let mut value = valid_lab_spec_value();
    value["common_budget"]["wall_ms"] = json!(250);
    value["common_budget"]["satisfaction"] = json!({"hp_loss_at_most": 4});
    value["common_budget"]["max_potions_used"] = json!(2);
    value["common_budget"]["turn_plan_probe_max_inner_nodes"] = json!(31);
    value["common_budget"]["turn_plan_probe_max_end_states"] = json!(32);
    value["common_budget"]["turn_plan_probe_per_bucket_limit"] = json!(33);
    let spec: CombatLabSpecV1 = serde_json::from_value(value).expect("parse lab spec");

    let config = profile_config_v1(&spec.experiment_id, &spec.profiles[0], &spec.common_budget);

    assert_eq!(
        config.input_label.as_deref(),
        Some("combat_lab_v1/experiment/baseline")
    );
    assert_eq!(config.max_nodes, 1000);
    assert_eq!(config.max_actions_per_line, 100);
    assert_eq!(config.max_engine_steps_per_action, 200);
    assert_eq!(
        config.wall_time,
        Some(std::time::Duration::from_millis(250))
    );
    assert_eq!(
        config.satisfaction,
        CombatSearchV2Satisfaction::HpLossAtMost(4)
    );
    assert_eq!(config.potion_policy, spec.profiles[0].potion_policy);
    assert_eq!(config.max_potions_used, Some(2));
    assert_eq!(config.rollout_policy, spec.profiles[0].rollout_policy);
    assert_eq!(
        config.child_rollout_policy,
        spec.profiles[0].child_rollout_policy
    );
    assert_eq!(config.rollout_max_evaluations, 10);
    assert_eq!(config.rollout_max_actions, 20);
    assert_eq!(config.rollout_beam_width, 3);
    assert_eq!(config.turn_plan_policy, spec.profiles[0].turn_plan_policy);
    assert_eq!(
        config.phase_guard_policy,
        spec.profiles[0].phase_guard_policy
    );
    assert_eq!(config.setup_bias_policy, spec.profiles[0].setup_bias_policy);
    assert_eq!(config.turn_plan_probe_max_inner_nodes, Some(31));
    assert_eq!(config.turn_plan_probe_max_end_states, Some(32));
    assert_eq!(config.turn_plan_probe_per_bucket_limit, Some(33));
    assert!(config.root_action_prior.is_none());
    assert!(config.turn_plan_prior.is_none());
}

#[test]
fn unsupported_nested_start_entry_fails_resolution_preflight() {
    let directory = test_directory("unknown_nested_start_field");
    fs::create_dir_all(&directory).expect("create test directory");
    let lab = valid_lab_spec_value();
    let mut start = valid_start_spec_value();
    start["master_deck"] = json!([{
        "id": "Bash",
        "unsupported": true
    }]);
    write_json(&directory.join("lab.json"), &lab);
    write_json(&directory.join("start.json"), &start);

    let error = load_and_resolve_combat_lab_spec_v1(&directory.join("lab.json"))
        .expect_err("unknown nested start fields must fail preflight");

    assert!(
        error.contains("failed to parse combat start spec"),
        "{error}"
    );
    assert!(
        error.contains("did not match any variant of untagged enum StartSpecCardSpec"),
        "{error}"
    );
    fs::remove_dir_all(directory).expect("remove test directory");
}

fn valid_lab_spec_value() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "experiment_id": "experiment",
        "scenario_id": "scenario",
        "start_spec": "start.json",
        "schedule": {
            "generator": "split_mix64_v1",
            "seed": 42
        },
        "profiles": [{
            "id": "baseline",
            "label": "Baseline",
            "information_scope": "exact_state_oracle",
            "potion_policy": "never",
            "rollout_policy": "disabled",
            "child_rollout_policy": "immediate",
            "turn_plan_policy": "disabled",
            "phase_guard_policy": "default",
            "setup_bias_policy": "default"
        }],
        "common_budget": {
            "max_nodes": 1000,
            "max_actions_per_line": 100,
            "max_engine_steps_per_action": 200,
            "wall_ms": null,
            "satisfaction": "zero_loss_or_budget",
            "max_potions_used": null,
            "rollout_max_evaluations": 10,
            "rollout_max_actions": 20,
            "rollout_beam_width": 3,
            "turn_plan_probe_max_inner_nodes": null,
            "turn_plan_probe_max_end_states": null,
            "turn_plan_probe_per_bucket_limit": null
        }
    })
}

fn valid_start_spec_value() -> serde_json::Value {
    json!({
        "name": "combat_lab_start",
        "player_class": "Ironclad",
        "ascension_level": 0,
        "encounter_id": "JawWorm",
        "room_type": "monster",
        "seed": 123,
        "player_current_hp": 72,
        "player_max_hp": 80,
        "relics": ["Burning Blood"],
        "potions": [],
        "master_deck": ["Bash"]
    })
}

fn resolved_lab_fixture(label: &str) -> (std::path::PathBuf, super::ResolvedCombatLabSpecV1) {
    let directory = test_directory(label);
    fs::create_dir_all(&directory).expect("create test directory");
    write_json(&directory.join("start.json"), &valid_start_spec_value());
    write_json(&directory.join("lab.json"), &valid_lab_spec_value());
    let resolved = load_and_resolve_combat_lab_spec_v1(&directory.join("lab.json"))
        .expect("resolve laboratory fixture");
    (directory, resolved)
}

fn replayable_win_sample() -> (
    super::CombatLabCompiledSampleV1,
    crate::ai::combat_search_v2::CombatSearchV2TrajectoryReport,
) {
    use crate::ai::combat_search_v2::{trajectory_from_state, CombatSearchV2ActionTrace};
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::CombatCard;
    use crate::sim::combat::{
        apply_combat_input_to_stable_observed_v1, CombatPosition, CombatStepLimits, CombatTerminal,
    };
    use crate::state::core::{ClientInput, EngineState};
    use crate::test_support::{blank_test_combat, test_monster};

    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.current_hp = 1;
    monster.max_hp = 1;
    let target = monster.id;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::BattleTrance, 1),
        CombatCard::for_scenario(CardId::Strike, 2),
    ];
    combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Defend, 3)]).into();
    let start = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(target),
        },
    ];
    let mut final_position = start.clone();
    for input in &inputs {
        let observed = apply_combat_input_to_stable_observed_v1(
            &final_position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: 100,
                deadline: None,
            },
        );
        assert!(!observed.step.truncated);
        assert!(!observed.step.timed_out);
        final_position = observed.step.position;
    }
    assert_eq!(
        crate::sim::combat::combat_terminal(&final_position.engine, &final_position.combat),
        CombatTerminal::Win
    );
    let actions = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| CombatSearchV2ActionTrace {
            step_index: index,
            action_id: index,
            action_key: format!("action-{index}"),
            action_debug: format!("{input:?}"),
            input: input.clone(),
        })
        .collect();
    let trajectory = trajectory_from_state(
        final_position.engine,
        final_position.combat,
        start.combat.entities.player.current_hp,
        actions,
        0,
        0,
        2,
        false,
    );
    let sample = super::CombatLabCompiledSampleV1 {
        sample_index: 7,
        shuffle_seed: 99,
        state_fingerprint: combat_state_fingerprint_v3(&start),
        start,
        non_shuffle_rng_hash: "non-shuffle".to_string(),
        shuffle_rng_hash: "shuffle".to_string(),
        monster_snapshot_hash: "monsters".to_string(),
    };
    (sample, trajectory)
}

fn replayable_loss_sample() -> (
    super::CombatLabCompiledSampleV1,
    crate::ai::combat_search_v2::CombatSearchV2TrajectoryReport,
) {
    use crate::ai::combat_search_v2::{trajectory_from_state, CombatSearchV2ActionTrace};
    use crate::content::monsters::EnemyId;
    use crate::semantics::combat::{AttackSpec, DamageKind, MonsterMoveSpec};
    use crate::sim::combat::{
        apply_combat_input_to_stable_observed_v1, CombatPosition, CombatStepLimits, CombatTerminal,
    };
    use crate::state::core::{ClientInput, EngineState};
    use crate::test_support::{blank_test_combat, test_monster};

    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 1;
    let mut monster = test_monster(EnemyId::JawWorm);
    let attack = MonsterMoveSpec::Attack(AttackSpec {
        base_damage: 1,
        hits: 1,
        damage_kind: DamageKind::Normal,
    });
    monster.set_planned_move_id(1);
    monster.move_state.planned_steps = Some(attack.to_steps());
    monster.move_state.planned_visible_spec = Some(attack);
    combat.entities.monsters = vec![monster];
    let start = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::EndTurn;
    let observed = apply_combat_input_to_stable_observed_v1(
        &start,
        input.clone(),
        CombatStepLimits {
            max_engine_steps: 100,
            deadline: None,
        },
    );
    assert!(!observed.step.truncated);
    assert!(!observed.step.timed_out);
    let final_position = observed.step.position;
    assert_eq!(
        crate::sim::combat::combat_terminal(&final_position.engine, &final_position.combat),
        CombatTerminal::Loss
    );
    let actions = vec![CombatSearchV2ActionTrace {
        step_index: 0,
        action_id: 0,
        action_key: "end-turn".to_string(),
        action_debug: format!("{input:?}"),
        input,
    }];
    let trajectory = trajectory_from_state(
        final_position.engine,
        final_position.combat,
        start.combat.entities.player.current_hp,
        actions,
        0,
        0,
        1,
        false,
    );
    let sample = super::CombatLabCompiledSampleV1 {
        sample_index: 8,
        shuffle_seed: 100,
        state_fingerprint: combat_state_fingerprint_v3(&start),
        start,
        non_shuffle_rng_hash: "non-shuffle".to_string(),
        shuffle_rng_hash: "shuffle".to_string(),
        monster_snapshot_hash: "monsters".to_string(),
    };
    (sample, trajectory)
}

fn json_with_reversed_object_keys(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(object) => {
            let fields = object
                .iter()
                .rev()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("serialize object key"),
                        json_with_reversed_object_keys(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", fields.join(","))
        }
        serde_json::Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(json_with_reversed_object_keys)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(value).expect("serialize scalar JSON value"),
    }
}

fn write_json(path: &std::path::Path, value: &serde_json::Value) {
    fs::write(
        path,
        serde_json::to_vec(value).expect("serialize fixture JSON"),
    )
    .expect("write fixture JSON");
}

fn artifact_manifest(
    resolved: super::ResolvedCombatLabSpecV1,
    created_at_unix_ms: u64,
) -> CombatLabManifestV1 {
    CombatLabManifestV1::from_resolved_v1(
        resolved,
        crate::eval::source_identity::SourceIdentity {
            git_commit: Some("test-commit".to_string()),
            git_dirty: Some(false),
        },
        created_at_unix_ms,
    )
}

fn artifact_cell(
    resolved: &super::ResolvedCombatLabSpecV1,
    sample_index: u64,
) -> super::CombatLabCellRecordV1 {
    use crate::ai::combat_search_v2::SearchCoverageStatus;

    let (mut sample, trajectory) = replayable_win_sample();
    sample.sample_index = sample_index;
    sample.shuffle_seed = derive_shuffle_seed_v1(&resolved.schedule, sample_index);
    super::replay::combat_lab_cell_record_from_trajectory_with_replayer_v1(
        resolved,
        &sample,
        &resolved.profiles[0],
        SearchCoverageStatus::AcceptedCompleteCandidate,
        Some(&trajectory),
        &crate::ai::combat_search_v2::CombatSearchV2Stats::default(),
        crate::ai::combat_search_v2::replay_combat_search_witness_line_v1,
    )
}

fn assert_resume_mismatch(
    output: &std::path::Path,
    manifest: CombatLabManifestV1,
    differing_field: &str,
) {
    let error = CombatLabArtifactStoreV1::create_or_resume(output, manifest)
        .err()
        .expect("resume identity mismatch must be rejected");
    assert!(error.contains(differing_field), "{error}");
}

fn artifact_journal_digest(bytes: &[u8]) -> String {
    use blake2::{Blake2b512, Digest};

    let mut hasher = Blake2b512::new();
    hasher.update(bytes);
    hasher.finalize()[..32]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn summary_manifest(
    label: &str,
    profile_ids: &[&str],
) -> (
    std::path::PathBuf,
    CombatLabManifestV1,
    crate::eval::fingerprint::StateFingerprintV3,
) {
    let (directory, mut resolved) = resolved_lab_fixture(label);
    let prototype = resolved.profiles[0].clone();
    resolved.profiles = profile_ids
        .iter()
        .map(|profile_id| {
            let mut profile = prototype.clone();
            profile.spec.id = (*profile_id).to_string();
            profile.spec.label = (*profile_id).to_string();
            profile.profile_hash = format!("profile-hash-{profile_id}");
            profile
        })
        .collect();
    let (engine, combat) = compile_combat_start_spec(&resolved.start_spec_snapshot)
        .expect("compile summary start state");
    let fingerprint =
        combat_state_fingerprint_v3(&crate::sim::combat::CombatPosition::new(engine, combat));
    (directory, artifact_manifest(resolved, 123), fingerprint)
}

fn summary_cell(
    manifest: &CombatLabManifestV1,
    fingerprint: &crate::eval::fingerprint::StateFingerprintV3,
    sample_index: u64,
    profile_id: &str,
    outcome_class: CombatLabOutcomeClassV1,
    final_hp: Option<i32>,
    hp_loss: Option<i32>,
) -> CombatLabCellRecordV1 {
    let profile = manifest
        .resolved_spec
        .profiles
        .iter()
        .find(|profile| profile.spec.id == profile_id)
        .expect("summary profile");
    let resolved = matches!(
        outcome_class,
        CombatLabOutcomeClassV1::ResolvedWin | CombatLabOutcomeClassV1::ResolvedLoss
    );
    CombatLabCellRecordV1 {
        schema_version: super::COMBAT_LAB_CELL_SCHEMA_VERSION,
        cell_key: format!("summary-cell-{sample_index}-{profile_id}"),
        experiment_hash: manifest.experiment_hash.clone(),
        sample_index,
        shuffle_seed: sample_index + 100,
        profile_id: profile_id.to_string(),
        profile_hash: profile.profile_hash.clone(),
        budget_hash: manifest.resolved_spec.budget_hash.clone(),
        initial_state_fingerprint: Some(fingerprint.clone()),
        non_shuffle_rng_hash: Some("non-shuffle".to_string()),
        shuffle_rng_hash: Some(format!("shuffle-{sample_index}")),
        search_terminal: None,
        coverage_status: None,
        outcome_class,
        outcome_order_key: resolved.then(|| summary_outcome_key(final_hp.unwrap_or_default())),
        replayed_candidate: None,
        replay_validated: resolved,
        start_hp: Some(20),
        final_hp,
        hp_loss,
        turns: None,
        actions: None,
        cards_played: None,
        potions_used: None,
        draw_history: Vec::new(),
        action_history: Vec::new(),
        expanded_nodes: 0,
        generated_nodes: 0,
        nodes_to_first_win: None,
        node_budget_exhausted: false,
        deadline_exhausted: false,
        error: None,
    }
}

fn summary_outcome_key(
    final_hp: i32,
) -> crate::ai::combat_search_v2::CombatSearchV2OutcomeOrderKeyReport {
    crate::ai::combat_search_v2::CombatSearchV2OutcomeOrderKeyReport {
        terminal_rank: 0,
        run_hygiene: 0,
        persistent_adjusted_hp: 0,
        final_hp,
        persistent_run_value: 0,
        potion_conservation: 0,
        faster_turns: 0,
        fewer_cards_played: 0,
        enemy_progress: 0,
        shorter_line: 0,
    }
}

fn summary_candidate(
    terminal: crate::ai::combat_search_v2::SearchTerminalLabel,
    final_hp: i32,
    hp_loss: i32,
    turns: u32,
    potions_used: u32,
) -> super::CombatLabReplayedCandidateV1 {
    super::CombatLabReplayedCandidateV1 {
        terminal,
        outcome_order_key: summary_outcome_key(final_hp),
        final_hp,
        hp_loss,
        turns,
        actions: 0,
        cards_played: 0,
        potions_used,
        draw_history: Vec::new(),
        action_history: Vec::new(),
    }
}

fn summary_cell_mut<'a>(
    cells: &'a mut [CombatLabCellRecordV1],
    sample_index: u64,
    profile_id: &str,
) -> &'a mut CombatLabCellRecordV1 {
    cells
        .iter_mut()
        .find(|cell| cell.sample_index == sample_index && cell.profile_id == profile_id)
        .expect("pair fixture cell")
}

fn assert_approx_eq(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("expected numeric summary");
    assert!(
        (actual - expected).abs() < 1.0e-12,
        "expected {expected}, got {actual}"
    );
}

fn fail_journal_sync_after_write(_: &std::fs::File) -> std::io::Result<()> {
    Err(std::io::Error::other("injected sync failure"))
}

#[derive(Default)]
struct RecordingCellExecutor {
    calls: std::cell::RefCell<Vec<(u64, &'static str)>>,
}

impl super::runner::CombatLabCellExecutorV1 for RecordingCellExecutor {
    fn execute_cell(
        &self,
        resolved: &super::ResolvedCombatLabSpecV1,
        sample: &super::CombatLabCompiledSampleV1,
        profile: &super::ResolvedCombatLabProfileV1,
    ) -> CombatLabCellRecordV1 {
        let profile_name = match profile.spec.id.as_str() {
            "baseline" => "baseline",
            "comparison" => "comparison",
            other => panic!("unexpected test profile {other}"),
        };
        self.calls
            .borrow_mut()
            .push((sample.sample_index, profile_name));
        CombatLabCellRecordV1 {
            schema_version: super::COMBAT_LAB_CELL_SCHEMA_VERSION,
            cell_key: super::combat_lab_cell_key_v1(
                &resolved.experiment_hash,
                sample.sample_index,
                sample.shuffle_seed,
                &profile.spec.id,
                &profile.profile_hash,
                &resolved.budget_hash,
            ),
            experiment_hash: resolved.experiment_hash.clone(),
            sample_index: sample.sample_index,
            shuffle_seed: sample.shuffle_seed,
            profile_id: profile.spec.id.clone(),
            profile_hash: profile.profile_hash.clone(),
            budget_hash: resolved.budget_hash.clone(),
            initial_state_fingerprint: Some(sample.state_fingerprint.clone()),
            non_shuffle_rng_hash: Some(sample.non_shuffle_rng_hash.clone()),
            shuffle_rng_hash: Some(sample.shuffle_rng_hash.clone()),
            search_terminal: Some(crate::ai::combat_search_v2::SearchTerminalLabel::Unresolved),
            coverage_status: Some(crate::ai::combat_search_v2::SearchCoverageStatus::FrontierOpen),
            outcome_class: CombatLabOutcomeClassV1::CoverageLimited,
            outcome_order_key: None,
            replayed_candidate: None,
            replay_validated: false,
            start_hp: Some(sample.start.combat.entities.player.current_hp),
            final_hp: None,
            hp_loss: None,
            turns: None,
            actions: None,
            cards_played: None,
            potions_used: None,
            draw_history: Vec::new(),
            action_history: Vec::new(),
            expanded_nodes: 1,
            generated_nodes: 1,
            nodes_to_first_win: None,
            node_budget_exhausted: true,
            deadline_exhausted: false,
            error: None,
        }
    }
}

#[derive(Default)]
struct HaltingReplayExecutor {
    calls: std::cell::RefCell<Vec<(u64, &'static str)>>,
}

impl super::runner::CombatLabCellExecutorV1 for HaltingReplayExecutor {
    fn execute_cell(
        &self,
        resolved: &super::ResolvedCombatLabSpecV1,
        sample: &super::CombatLabCompiledSampleV1,
        profile: &super::ResolvedCombatLabProfileV1,
    ) -> CombatLabCellRecordV1 {
        let recording = RecordingCellExecutor::default();
        let mut cell = super::runner::CombatLabCellExecutorV1::execute_cell(
            &recording, resolved, sample, profile,
        );
        self.calls.borrow_mut().extend(recording.calls.into_inner());
        cell.outcome_class = CombatLabOutcomeClassV1::ExecutionError;
        cell.error = Some(super::CombatLabCellErrorV1 {
            stage: super::CombatLabCellErrorStageV1::ExactReplay,
            code: "injected_exact_replay_failure".to_string(),
            message: "injected deterministic replay invariant failure".to_string(),
            halt_experiment: true,
        });
        cell
    }
}

fn runner_fixture(label: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let fixture_directory = test_directory(label);
    fs::create_dir_all(&fixture_directory).expect("create runner fixture directory");
    let mut lab = valid_lab_spec_value();
    let mut comparison = lab["profiles"][0].clone();
    comparison["id"] = json!("comparison");
    comparison["label"] = json!("Comparison");
    lab["profiles"] = json!([lab["profiles"][0].clone(), comparison]);
    lab["common_budget"]["max_nodes"] = json!(1);
    lab["common_budget"]["max_actions_per_line"] = json!(1);
    lab["common_budget"]["max_engine_steps_per_action"] = json!(8);
    lab["common_budget"]["rollout_max_evaluations"] = json!(1);
    lab["common_budget"]["rollout_max_actions"] = json!(1);
    lab["common_budget"]["rollout_beam_width"] = json!(1);
    let lab_spec_path = fixture_directory.join("lab.json");
    write_json(
        &fixture_directory.join("start.json"),
        &valid_start_spec_value(),
    );
    write_json(&lab_spec_path, &lab);

    let output_dir = crate::eval::repository_root()
        .join("artifacts")
        .join("runs")
        .join(format!("combat-lab-test-{label}-{}", std::process::id()));
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir).expect("remove stale runner output");
    }
    (fixture_directory, lab_spec_path, output_dir)
}

fn journal_cell_keys(bytes: &[u8]) -> BTreeSet<String> {
    std::str::from_utf8(bytes)
        .expect("journal is utf8")
        .lines()
        .map(|line| {
            serde_json::from_str::<CombatLabCellRecordV1>(line)
                .expect("parse journal cell")
                .cell_key
        })
        .collect()
}

fn read_journal_cells(output_dir: &std::path::Path) -> Vec<CombatLabCellRecordV1> {
    let bytes = fs::read(output_dir.join("cells.jsonl")).expect("read journal");
    std::str::from_utf8(&bytes)
        .expect("journal is utf8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("parse journal cell"))
        .collect()
}

fn test_directory(label: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sts_simulator_combat_lab_v1_{label}_{}_{}",
        std::process::id(),
        nonce
    ))
}
