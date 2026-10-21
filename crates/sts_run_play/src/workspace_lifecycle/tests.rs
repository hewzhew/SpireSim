use super::*;

fn fresh_test_root_v1(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sts_run_play_workspace_lifecycle_{label}_{}_{}",
        std::process::id(),
        unix_ms_v1().expect("time")
    ))
}

fn create_managed_fixture_v1(root: &Path, name: &str) -> PathBuf {
    let workspace = root.join(name);
    for child in ["heads", "states", "deltas"] {
        fs::create_dir_all(workspace.join(child)).expect("workspace shape");
    }
    let manifest = ManagedWorkspaceManifestV1 {
        schema: MANAGED_WORKSPACE_MANIFEST_SCHEMA_V1.to_string(),
        kind: "disposable_fork".to_string(),
        workspace_name: name.to_string(),
        target_session_id: format!("target-{name}"),
        source: ManagedWorkspaceSourceV1 {
            session_path: "source".to_string(),
            session_id: "source-session".to_string(),
            head_revision: 7,
            archive_node_id: "node".to_string(),
        },
        created_unix_ms: unix_ms_v1().expect("time"),
        expected_cleanup_phase: "after probe review".to_string(),
    };
    write_json_new_v1(
        &workspace.join(MANAGED_WORKSPACE_MANIFEST_FILE_V1),
        &manifest,
        "fixture manifest",
    )
    .expect("manifest");
    workspace
}

#[test]
fn summary_only_admits_unprotected_registered_direct_children() {
    let sandbox = fresh_test_root_v1("summary");
    let root = sandbox.join("managed");
    fs::create_dir_all(&root).expect("managed root");
    let eligible = create_managed_fixture_v1(&root, "eligible");
    let active = create_managed_fixture_v1(&root, "active");
    let handed = create_managed_fixture_v1(&root, "handed");
    fs::create_dir_all(handed.join("handoffs")).expect("handoffs");
    fs::write(handed.join("handoffs/handoff-000.json"), b"{}").expect("handoff");
    fs::create_dir_all(root.join("unmanaged")).expect("unmanaged");

    let summary = inspect_workspace_gc_v1(&root, Some(&active)).expect("summary");
    assert_eq!(summary.eligible.len(), 2);
    assert_eq!(
        summary
            .eligible
            .iter()
            .map(|target| target.workspace_name.as_str())
            .collect::<Vec<_>>(),
        ["eligible", "handed"]
    );
    assert_eq!(summary.protected_focus, 1);
    assert_eq!(summary.unmanaged_directories, 1);
    assert!(eligible.is_dir());

    fs::remove_dir_all(&sandbox).expect("cleanup");
}

#[test]
fn execution_revalidates_a_plan_and_deletes_only_its_exact_target() {
    let sandbox = fresh_test_root_v1("execute");
    let root = sandbox.join("managed");
    fs::create_dir_all(&root).expect("managed root");
    let target = create_managed_fixture_v1(&root, "target");
    let unmanaged = root.join("unmanaged");
    fs::create_dir_all(&unmanaged).expect("unmanaged");
    let capsule = root.join("reroot-capsule.msgpack");
    fs::write(&capsule, b"capsule").expect("capsule");
    let plan = sandbox.join("plan.json");
    let receipt = sandbox.join("execution.jsonl");

    let planned = write_workspace_gc_plan_v1(&root, None, &plan).expect("plan");
    assert_eq!(planned.targets.len(), 1);
    fs::create_dir_all(target.join("handoffs")).expect("handoffs");
    fs::write(target.join("handoffs/handoff-000.json"), b"{}").expect("handoff");
    let (count, _) = execute_workspace_gc_plan_v1(&root, None, &plan, &receipt).expect("execute");
    assert_eq!(count, 1);
    assert!(!target.exists());
    assert!(unmanaged.is_dir());
    assert!(capsule.is_file());
    let lines = fs::read_to_string(&receipt).expect("receipt");
    assert!(lines.contains(WORKSPACE_GC_EXECUTION_START_SCHEMA_V2));
    assert!(lines.contains(WORKSPACE_GC_EXECUTION_COMPLETE_SCHEMA_V1));

    fs::remove_dir_all(&sandbox).expect("cleanup");
}
