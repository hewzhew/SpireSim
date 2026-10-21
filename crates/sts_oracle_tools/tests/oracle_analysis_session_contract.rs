use sts_oracle_runtime::eval::combat_guidance_bundle::CombatValuePrototypeArtifactV1;
use sts_oracle_runtime::eval::run_control::{
    exact_replay_run_progress_journal_v1, run_progress_journal_fingerprint_v1,
};
use sts_oracle_runtime::runtime::branch::{
    OracleAnalysisWorkspaceArtifactV1, OracleAnalysisWorkspaceV1, OracleRunBudget, OracleRunConfig,
};

const SEED: u64 = 20_260_713_006;

fn workspace() -> OracleAnalysisWorkspaceV1 {
    OracleAnalysisWorkspaceV1::new(OracleRunConfig {
        seed: SEED,
        ascension: 0,
        budget: OracleRunBudget::default(),
    })
    .expect("analysis workspace")
}

#[test]
fn value_corpus_preserves_same_turn_evidence_from_distinct_witnesses() {
    let artifact = CombatValuePrototypeArtifactV1::from_ranked_feature_trajectories(
        "two verified wins",
        [
            (3, 11, vec![(1, 0, vec![10, 20]), (2, 1, vec![11, 21])]),
            (4, 7, vec![(1, 0, vec![30, 40]), (2, 1, vec![31, 41])]),
        ],
    )
    .expect("two exact trajectories form one value corpus");

    assert_eq!(artifact.source_trajectory_count, 2);
    assert_eq!(artifact.source_action_count, 7);
    assert_eq!(artifact.source_terminal_final_hp, 7);
    assert_eq!(artifact.targets_by_turn()[&1].len(), 2);
}

#[test]
fn variations_are_created_without_mutating_the_parent_and_can_be_navigated() {
    let mut workspace = workspace();
    let root = workspace.view().expect("root view");
    let choice = root.choices.first().expect("root map choice").clone();
    let root_hp = root.current_hp;

    let child = workspace
        .try_choice(&choice.choice_ref)
        .expect("materialize child variation");
    assert_ne!(child.node_id, root.node_id);
    assert_eq!(workspace.session.cursor_node_id(), child.node_id);

    let parent_after = workspace
        .session
        .view_node(root.node_id)
        .expect("parent remains inspectable");
    assert_eq!(parent_after.current_hp, root_hp);
    assert!(parent_after
        .choices
        .iter()
        .any(|candidate| candidate.choice_ref == choice.choice_ref));
    let edge = parent_after
        .children
        .iter()
        .find(|edge| edge.child_node_id == child.node_id)
        .expect("variation edge")
        .edge_id;

    assert_eq!(workspace.session.back().expect("back"), root.node_id);
    workspace.session.follow_edge(edge).expect("follow child");
    assert_eq!(workspace.session.cursor_node_id(), child.node_id);
    workspace.session.promote_cursor();
    assert_eq!(workspace.session.mainline_node_id(), child.node_id);
}

#[test]
fn engine_owned_checkpoint_roundtrips_navigation_and_rejects_tampered_choices() {
    let mut workspace = workspace();
    let root = workspace.view().expect("root view");
    let choice = root.choices.first().expect("root map choice").clone();
    let child = workspace
        .try_choice(&choice.choice_ref)
        .expect("materialize child variation");
    workspace.session.promote_cursor();

    let mut tampered = choice.choice_ref.clone();
    tampered.push('0');
    assert!(workspace.try_choice(&tampered).is_err());

    let bytes =
        serde_json::to_vec(&workspace.artifact().expect("artifact")).expect("serialize artifact");
    let artifact = serde_json::from_slice::<OracleAnalysisWorkspaceArtifactV1>(&bytes)
        .expect("deserialize artifact");
    let mut restored = OracleAnalysisWorkspaceV1::restore(artifact).expect("restore workspace");
    let restored_view = restored.view().expect("restored view");
    assert_eq!(restored_view.node_id, child.node_id);
    assert_eq!(restored.session.mainline_node_id(), child.node_id);
    assert_eq!(
        restored.session.back().expect("restored back"),
        root.node_id
    );
}

#[test]
fn compact_workspace_keeps_one_exact_committed_node_and_its_journal() {
    let mut workspace = workspace();
    let root = workspace.view().expect("root view");
    let choice = root.choices.first().expect("root choice").clone();
    let child = workspace
        .try_choice(&choice.choice_ref)
        .expect("materialize child");
    assert!(workspace.session.tree().nodes.len() > 1);

    let source = workspace
        .continuation(child.node_id)
        .expect("source continuation");
    let source_expected = source
        .session
        .clone()
        .into_session()
        .expect("source session");
    let source_replay = exact_replay_run_progress_journal_v1(
        source.seed,
        source.ascension,
        &source.journal,
        &source_expected,
    )
    .expect("source replay");

    let compact = workspace
        .compact_from_node(child.node_id)
        .expect("compact workspace");
    assert_eq!(compact.session.tree().nodes.len(), 1);
    let compact_node_id = compact.session.cursor_node_id();
    let compact_continuation = compact
        .continuation(compact_node_id)
        .expect("compact continuation");
    let compact_expected = compact_continuation
        .session
        .clone()
        .into_session()
        .expect("compact session");
    let compact_replay = exact_replay_run_progress_journal_v1(
        compact_continuation.seed,
        compact_continuation.ascension,
        &compact_continuation.journal,
        &compact_expected,
    )
    .expect("compact replay");

    assert_eq!(source_replay, compact_replay);
    assert_eq!(
        run_progress_journal_fingerprint_v1(&source.journal),
        run_progress_journal_fingerprint_v1(&compact_continuation.journal)
    );
}

#[test]
fn workspace_checkpoint_pools_branch_payloads_and_rejects_tampering() {
    let mut workspace = workspace();
    let root = workspace.view().expect("root view");
    let choice = root.choices.first().expect("root choice").clone();
    workspace
        .try_choice(&choice.choice_ref)
        .expect("materialize child");

    let artifact = workspace.artifact().expect("pooled artifact");
    let explorer = &artifact.session.explorer;
    assert!(explorer.branches.len() > 1);
    assert_eq!(explorer.payloads.map_graphs.len(), 1);
    assert!(explorer.payloads.maps.len() < explorer.branches.len());
    assert!(explorer
        .branches
        .iter()
        .all(|branch| branch.session_payload_refs.map_id.is_some()));
    assert!(!explorer.payloads.replay_nodes.is_empty());
    assert!(explorer
        .branches
        .iter()
        .any(|branch| branch.replay_tip.is_some()));

    let mut missing_algorithm = artifact.clone();
    missing_algorithm
        .session
        .explorer
        .payloads
        .fingerprint_algorithm = None;
    let error = match OracleAnalysisWorkspaceV1::restore(missing_algorithm) {
        Ok(_) => panic!("pooled payloads without a declared hash algorithm must not restore"),
        Err(error) => error,
    };
    assert!(error.contains("fingerprint algorithm is missing"));

    let mut tampered = artifact;
    tampered.session.explorer.payloads.maps[0].map.current_x += 1;
    let error = match OracleAnalysisWorkspaceV1::restore(tampered) {
        Ok(_) => panic!("tampered pooled map must not restore"),
        Err(error) => error,
    };
    assert!(error.contains("fingerprint validation"));
}
