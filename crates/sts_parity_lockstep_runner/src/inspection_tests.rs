use super::*;
use crate::{replay_recorded_evidence_with_observer_v1, replay_training_trajectory_v2};
use serde_json::{json, Value};
use std::path::PathBuf;
use sts_oracle_run_control::eval::run_control::{RunControlSession, RunControlSessionCheckpointV1};
use sts_parity_lockstep::{ComparisonStatusV1, ParityDomainV1};
use sts_parity_lockstep_java::{RecordedJavaActionPhaseV1, RecordedJavaEvidenceStoreV1};

struct Fixture {
    repository: PathBuf,
    session: ManualSessionV1,
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl Fixture {
    fn new() -> Self {
        let repository =
            std::env::temp_dir().join(format!("inspection-test-{}", uuid::Uuid::new_v4()));
        let session = ManualSessionV1::prepare(&repository).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus_projection_drift");
        let sources: Value =
            serde_json::from_slice(&fs::read(fixture.join("sources.json")).unwrap()).unwrap();
        let mut files = Vec::new();
        for (name, source) in sources["files"].as_object().unwrap() {
            if !name.starts_with("java-evidence/") {
                continue;
            }
            let bytes = fs::read(fixture.join(name)).unwrap();
            assert_eq!(format!("{:x}", Sha256::digest(&bytes)), source["sha256"]);
            let path = session.directory().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &bytes).unwrap();
            files.push((path, bytes));
        }
        Self {
            repository,
            session,
            files,
        }
    }
    fn inspect(&self, action: usize) -> ManualTransitionInspectionV1 {
        inspect_manual_transition_v1(&self.session, action, &mut |_| {}).unwrap()
    }
    fn mutate(&self, name: &str, change: impl FnOnce(&mut Value)) {
        let path = self
            .files
            .iter()
            .find(|(p, _)| p.file_name().unwrap() == name)
            .unwrap()
            .0
            .clone();
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        change(&mut value);
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    fn append_unexecutable_step(&self) {
        let mut store =
            RecordedJavaEvidenceStoreV1::open_append(&self.session.evidence_directory()).unwrap();
        let evidence =
            RecordedJavaEvidenceV1::load_complete(&self.session.evidence_directory()).unwrap();
        let step = &evidence.steps[0];
        store.append_boundary(&step.after.observation).unwrap();
        let mut receipt = step.java_actions[0].receipt.clone();
        receipt.request_id = "constructed-follow-up".into();
        receipt.based_on_frame_id = step.after.observation.frame_id;
        receipt.successor_frame_id = Some(receipt.based_on_frame_id + 1);
        store
            .append_action(RecordedJavaActionPhaseV1::Committed, &step.action, &receipt)
            .unwrap();
        let mut after = step.after.observation.clone();
        after.frame_id = receipt.successor_frame_id.unwrap();
        store.append_boundary(&after).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.repository.is_absolute() && self.repository.starts_with(std::env::temp_dir()));
        assert!(self
            .repository
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("inspection-test-"));
        fs::remove_dir_all(&self.repository).unwrap();
    }
}

#[test]
fn timing_capture_preserves_replay_results_and_does_not_claim_live_settlement() {
    let fixture = Fixture::new();
    let plain = fixture.inspect(1);
    let capture = sts_parity_lockstep::TimingCaptureV1::start().unwrap();
    let measured = fixture.inspect(1);
    let timing = capture.finish();
    assert!(timing.balanced);
    assert_eq!(plain.reached_exact, measured.reached_exact);
    assert_eq!(
        serde_json::to_value(plain.transition).unwrap(),
        serde_json::to_value(measured.transition).unwrap()
    );
    assert!(timing
        .spans
        .keys()
        .any(|path| path.ends_with("java.command")));
    assert!(timing.observations.is_empty());
    for (path, bytes) in &fixture.files {
        assert_eq!(&fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn replay_observer_visits_exact_commits_and_propagates_abort_without_writing_evidence() {
    let fixture = Fixture::new();
    let directory = fixture.session.evidence_directory();
    let mut indices = Vec::new();
    let report = replay_recorded_evidence_with_observer_v1(&directory, &mut |index, _, _| {
        indices.push(index);
        Ok(())
    })
    .unwrap();
    assert_eq!(indices, (0..=report.committed_actions).collect::<Vec<_>>());
    let mut indices = Vec::new();
    let error = replay_recorded_evidence_with_observer_v1(&directory, &mut |index, _, _| {
        indices.push(index);
        if index == 1 {
            Err("caller stopped at exact commit".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(indices, vec![0, 1]);
    assert!(error.contains("caller stopped at exact commit"));
    for (path, bytes) in &fixture.files {
        assert_eq!(&fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn training_trajectory_derives_one_real_commit_from_the_same_raw_replay() {
    let fixture = Fixture::new();
    let trajectory = replay_training_trajectory_v2(&fixture.session.evidence_directory()).unwrap();
    assert_eq!(trajectory.replay.committed_actions, 1);
    assert_eq!(trajectory.commits.len(), 1);
    let commit = &trajectory.commits[0];
    assert_eq!(commit.semantic_actions, vec![1]);
    assert_eq!(commit.choice_snapshots.len(), commit.ordinals.len());
    assert_eq!(commit.before_continuation, trajectory.initial_continuation);
    assert_ne!(commit.before_continuation, commit.after_continuation);
    assert!(!commit.terminated);
    assert_eq!(commit.reward, 0);
    for (path, bytes) in &fixture.files {
        assert_eq!(&fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn manual_training_prefix_freezes_real_source_and_rejects_out_of_range() {
    let fixture = Fixture::new();
    let replay = |through| {
        crate::replay_manual_training_trajectory_v2(
            &fixture.repository,
            fixture.session.session_id(),
            Some(through),
        )
    };
    let initial = replay(0).unwrap();
    assert_eq!(initial.replay.committed_actions, 0);
    assert!(initial.commits.is_empty());
    assert_eq!(initial.sources.len(), 1);
    assert_eq!(
        initial.sources[0].session_id.as_deref(),
        Some(fixture.session.session_id())
    );
    let first = replay(1).unwrap();
    assert_eq!(first.replay.committed_actions, 1);
    assert_eq!(first.commits[0].semantic_actions, [1]);
    assert_eq!(first.initial_continuation, initial.initial_continuation);
    assert!(replay(2).is_err());
    for (path, bytes) in &fixture.files {
        assert_eq!(&fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn borrowed_replay_rejects_a_wrong_handle_without_consuming_the_transition() {
    use sts_parity_lockstep_java::JavaLockstepTransportV1;
    let fixture = Fixture::new();
    let evidence =
        RecordedJavaEvidenceV1::load_complete(&fixture.session.evidence_directory()).unwrap();
    let mut replay = crate::RecordedJavaReplayTransportV1::new(&evidence).unwrap();
    let before = replay.status().unwrap();
    assert!(replay.act("not-a-recorded-handle").is_err());
    assert_eq!(replay.remaining_actions(), evidence.java_action_count);
    assert_eq!(replay.status().unwrap(), before);
    let transition = &evidence.transitions[0];
    let action = before
        .actions
        .iter()
        .find(|a| a.action_id == transition.action.receipt.action_id)
        .unwrap();
    assert_eq!(
        replay.act(&action.runtime_handle).unwrap(),
        transition.action.receipt
    );
    assert_eq!(replay.status().unwrap(), transition.after.observation);
    assert_eq!(replay.remaining_actions(), evidence.java_action_count - 1);
    assert_eq!(evidence.initial.observation, before);
}

#[test]
fn targeted_inspection_is_exact_read_only_and_does_not_require_an_active_session() {
    let fixture = Fixture::new();
    assert!(ManualSessionV1::load_active(&fixture.repository).is_err());
    let report = fixture.inspect(1);
    assert!(report.reached_exact, "{:?}", report.stop_reason);
    assert_eq!(report.scopes.len(), 1);
    assert_eq!(report.scopes[0].exact_actions, 1);
    let transition = report.transition.as_ref().unwrap();
    assert_eq!(transition.coordinate.action_number, 1);
    assert_eq!(transition.raw_java.len(), 1);
    assert_eq!(
        transition.snapshot_comparison.as_ref().unwrap().status,
        ComparisonStatusV1::Exact
    );
    assert!(transition.rng_comparison.as_ref().unwrap().exact);
    for (file, bytes) in &fixture.files {
        assert_eq!(fs::read(file).unwrap(), *bytes);
    }
    assert!(ManualSessionV1::load_active(&fixture.repository).is_err());
    let output = report.summary();
    assert!(!output.contains(fixture.session.session_id()));
    assert!(!output.contains("card_bindings"));
    assert!(!output.contains("frame_id"));
    assert!(output.len() < 300);
    assert!(inspect_manual_transition_v1(&fixture.session, 0, &mut |_| {}).is_err());
}

#[test]
fn targeted_inspection_preserves_the_first_earlier_state_difference_and_stops() {
    let fixture = Fixture::new();
    fixture.append_unexecutable_step();
    fixture.mutate("00000001-frame-9.json", |v| {
        v["observation"]["frame"]["public_base"]["run"]["gold"] = json!(999)
    });
    let report = fixture.inspect(2);
    assert!(!report.reached_exact);
    assert_eq!(report.scopes[0].exact_actions, 0);
    let transition = report.transition.unwrap();
    assert_eq!(transition.coordinate.action_number, 1);
    assert!(transition.stop_reason.as_ref().unwrap().contains("gold"));
    let comparison = transition.snapshot_comparison.unwrap();
    assert_eq!(comparison.status, ComparisonStatusV1::Mismatch);
    let run = comparison
        .domains
        .iter()
        .find(|d| d.domain == ParityDomainV1::Run)
        .unwrap();
    assert_eq!(run.status, ComparisonStatusV1::Mismatch);
    assert!(run.java.is_some() && run.rust.is_some());
}

#[test]
fn targeted_inspection_retains_prepare_and_initial_observation_failures_without_inventing_successors(
) {
    let fixture = Fixture::new();
    fixture.mutate("00000000-from-frame-8.json", |v| {
        v["action"] = json!({"kind":"end_turn"})
    });
    let report = fixture.inspect(1);
    assert!(!report.reached_exact);
    let transition = report.transition.unwrap();
    assert!(transition.java_after.is_none() && transition.rust_after.is_none());
    assert!(transition.java_rng_calls.is_none() && transition.rust_rng_calls.is_none());
    assert!(transition.snapshot_comparison.is_none());
    let fixture = Fixture::new();
    fixture.mutate("00000000-frame-8.json", |v| {
        v["observation"]["frame"]["public_base"]["run"]
            .as_object_mut()
            .unwrap()
            .remove("gold");
    });
    let report = fixture.inspect(1);
    assert!(!report.reached_exact);
    assert!(report
        .stop_reason
        .unwrap()
        .contains("Java initial observation failed"));
    assert!(report.transition.is_none());
    assert!(report.scopes[0].stopped);
}

#[test]
fn targeted_inspection_does_not_promote_the_unfinished_tail() {
    let fixture = Fixture::new();
    let evidence =
        RecordedJavaEvidenceV1::load_complete(&fixture.session.evidence_directory()).unwrap();
    let mut store =
        RecordedJavaEvidenceStoreV1::open_append(&fixture.session.evidence_directory()).unwrap();
    let step = &evidence.steps[0];
    store.append_boundary(&step.after.observation).unwrap();
    let mut receipt = step.java_actions[0].receipt.clone();
    receipt.request_id = "interrupted".into();
    receipt.based_on_frame_id = step.after.observation.frame_id;
    receipt.successor_frame_id = None;
    receipt.status = "rejected".into();
    store
        .append_action(RecordedJavaActionPhaseV1::Committed, &step.action, &receipt)
        .unwrap();
    assert!(RecordedJavaEvidenceV1::load_complete(&fixture.session.evidence_directory()).is_err());
    assert!(fixture.inspect(1).reached_exact);
    assert!(!fixture.inspect(2).reached_exact);
    let missing = fixture.inspect(3);
    assert!(!missing.reached_exact);
    assert_eq!(missing.scopes[0].recorded_actions, None);
    assert!(RecordedJavaEvidenceV1::load_complete(&fixture.session.evidence_directory()).is_err());
}

#[test]
fn targeted_inspection_reconstructs_ancestry_instead_of_trusting_a_saved_rust_checkpoint() {
    let fixture = Fixture::new();
    fixture.append_unexecutable_step();
    let evidence =
        RecordedJavaEvidenceV1::load_prefix(&fixture.session.evidence_directory(), 1).unwrap();
    let child = ManualSessionV1::prepare(&fixture.repository).unwrap();
    child
        .publish_rust_root(
            fixture.session.session_id(),
            1,
            RunControlSessionCheckpointV1::from_session(
                &RunControlSession::new(Default::default()),
            ),
        )
        .unwrap();
    let mut store = RecordedJavaEvidenceStoreV1::create_new(
        &child.evidence_directory(),
        evidence.manifest.bootstrap,
    )
    .unwrap();
    let step = &evidence.steps[0];
    store.append_boundary(&step.after.observation).unwrap();
    let mut receipt = step.java_actions[0].receipt.clone();
    receipt.request_id = "child-invalid-action".into();
    receipt.based_on_frame_id = step.after.observation.frame_id;
    receipt.successor_frame_id = Some(receipt.based_on_frame_id + 1);
    store
        .append_action(RecordedJavaActionPhaseV1::Committed, &step.action, &receipt)
        .unwrap();
    let mut after = step.after.observation.clone();
    after.frame_id += 1;
    store.append_boundary(&after).unwrap();
    let report = inspect_manual_transition_v1(&child, 1, &mut |_| {}).unwrap();
    assert_eq!(report.scopes.len(), 2);
    assert_eq!(report.scopes[0].exact_actions, 1);
    assert_eq!(report.scopes[0].recorded_actions, Some(1));
    assert!(!report.scopes[0].stopped);
    assert!(!report.reached_exact);
    let transition = report.transition.unwrap();
    assert_eq!(transition.coordinate.session_id, child.session_id());
    assert_eq!(
        compare_snapshots_v1(&transition.java_before, &transition.rust_before)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
}

#[test]
fn journal_split_can_reference_an_exact_preparation_with_no_committed_suffix() {
    let fixture = Fixture::new();
    let (_, rust, expected_resume) =
        crate::replay_manual_session_v1(&fixture.repository, &fixture.session).unwrap();
    let evidence =
        RecordedJavaEvidenceV1::load_complete(&fixture.session.evidence_directory()).unwrap();
    let checkpoint = RunControlSessionCheckpointV1::from_session(rust.session());
    let preparation = ManualSessionV1::prepare(&fixture.repository).unwrap();
    let certification = ManualSessionV1::prepare(&fixture.repository).unwrap();
    for (child, source, count) in [
        (&preparation, &fixture.session, 1),
        (&certification, &preparation, 0),
    ] {
        child
            .publish_rust_root(source.session_id(), count, checkpoint.clone())
            .unwrap();
        let mut store = RecordedJavaEvidenceStoreV1::create_new(
            &child.evidence_directory(),
            evidence.manifest.bootstrap.clone(),
        )
        .unwrap();
        store
            .append_boundary(&evidence.steps.last().unwrap().after.observation)
            .unwrap();
    }
    let (report, _, resume) =
        crate::replay_manual_session_v1(&fixture.repository, &certification).unwrap();
    assert_eq!(report.committed_actions, 0);
    assert_eq!(report.java_actions, 0);
    assert_eq!(resume, expected_resume);

    // Its source still has to replay; a zero-action reference is not permission
    // to trust a checkpoint or skip the preparation's raw boundary.
    let boundary = preparation.evidence_directory().join("boundaries");
    let path = fs::read_dir(boundary)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw["observation"]["frame"]["public_base"]["run"]["gold"] = json!(999);
    fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(crate::replay_manual_session_v1(&fixture.repository, &certification).is_err());
}

#[test]
fn targeted_inspection_retains_rng_difference_even_when_snapshots_match() {
    let fixture = Fixture::new();
    fixture.mutate("00000001-frame-9.json", |v| v["observation"]["frame"]["rng_call_witness"]["calls"] = json!([
        {"owner":"floor.card_random","owner_ordinal":0,"operation":{"method":"random_int","range":6,"result":3}}
    ]));
    let report = fixture.inspect(1);
    assert!(!report.reached_exact);
    let transition = report.transition.unwrap();
    let calls = transition.rng_comparison.unwrap();
    assert!(!calls.exact);
    assert_eq!(calls.first_difference.unwrap().call_index, 0);
    assert_eq!(
        transition.snapshot_comparison.unwrap().status,
        ComparisonStatusV1::Exact
    );
}

#[test]
fn inspection_artifact_records_producer_and_never_overwrites_evidence() {
    let fixture = Fixture::new();
    let report = fixture.inspect(1);
    let output = fixture.repository.join("inspection.json");
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    save_manual_transition_inspection_v1(repository, &output, &report).unwrap();
    let bytes = fs::read(&output).unwrap();
    let artifact: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        artifact["producer"]["executable_sha256"],
        format!(
            "{:x}",
            Sha256::digest(fs::read(std::env::current_exe().unwrap()).unwrap())
        )
    );
    assert_eq!(
        artifact["inspection"]["requested"]["session_id"],
        fixture.session.session_id()
    );
    assert!(save_manual_transition_inspection_v1(repository, &output, &report).is_err());
    assert_eq!(fs::read(&output).unwrap(), bytes);
}

#[test]
fn observed_madness_rejection_case_replays_the_real_cost_mutation_and_rng_calls() {
    use sts_oracle_run_control::{
        content::cards::CardId,
        engine::action_handlers::cards::handle_madness,
        runtime::{
            combat::CombatCard,
            rng::{begin_sts_rng_trace, StsRng, StsRngCall},
        },
    };
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/madness_rejection");
    let sources: Value =
        serde_json::from_slice(&fs::read(directory.join("sources.json")).unwrap()).unwrap();
    let mut files = std::collections::BTreeMap::new();
    for (name, source) in sources["files"].as_object().unwrap() {
        let bytes = fs::read(directory.join(name)).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), source["sha256"]);
        files.insert(
            name.clone(),
            serde_json::from_slice::<Value>(&bytes).unwrap(),
        );
    }
    let before = &files["before.json"]["observation"];
    let after = &files["after.json"]["observation"];
    let action = &files["action.json"];
    assert_eq!(before["producer"], sources["producer"]);
    assert_eq!(after["producer"], sources["producer"]);
    assert_eq!(before["frame_id"], action["receipt"]["based_on_frame_id"]);
    assert_eq!(after["frame_id"], action["receipt"]["successor_frame_id"]);
    let before = &before["frame"];
    let after = &after["frame"];
    assert_eq!(
        after["rng_call_witness"]["request_id"],
        action["receipt"]["request_id"]
    );
    assert_eq!(action["action"]["card_id"], "madness");
    let hand = before["parity_internal"]["zones"]["hand"]["cards"]
        .as_array()
        .unwrap();
    assert_eq!(hand.len(), 8);
    assert_eq!(hand[7]["card_id"], "Madness");
    assert_eq!(
        action["receipt"]["action_id"],
        format!(
            "play_card:{}:none",
            hand[7]["instance_uuid"].as_str().unwrap()
        )
    );
    // Explicit constructed handler input after the played Madness leaves the hand.
    // Scenario lineages below are local; this does not import a natural battle or
    // bind any Java UUID to a Rust lineage. UUIDs compare Java before/after only.
    let ids = [
        (CardId::Defend, "Defend_R"),
        (CardId::Clothesline, "Clothesline"),
        (CardId::Whirlwind, "Whirlwind"),
        (CardId::Defend, "Defend_R"),
        (CardId::FlameBarrier, "Flame Barrier"),
        (CardId::AscendersBane, "AscendersBane"),
        (CardId::BattleTrance, "Battle Trance"),
    ];
    let mut state = sts_oracle_run_control::test_support::blank_test_combat();
    state.zones.hand = ids
        .into_iter()
        .enumerate()
        .map(|(i, (id, name))| {
            assert_eq!(hand[i]["card_id"], name);
            let mut card = CombatCard::for_scenario(id, i as u32);
            card.set_combat_and_turn_cost_java(hand[i]["combat_cost"].as_i64().unwrap() as i32);
            card.set_cost_for_turn_java(hand[i]["cost_for_turn"].as_i64().unwrap() as i32);
            card
        })
        .collect();
    let stream = |frame: &Value| {
        frame["rng_internal"]["streams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["owner"] == "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRandomRng")
            .unwrap()
            .clone()
    };
    let rng = |s: Value| {
        StsRng::from_state(
            u64::from_str_radix(s["state_hex"][0].as_str().unwrap(), 16).unwrap(),
            u64::from_str_radix(s["state_hex"][1].as_str().unwrap(), 16).unwrap(),
            s["counter"].as_u64().unwrap() as u32,
        )
    };
    state.rng.card_random_rng = rng(stream(before));
    let trace = begin_sts_rng_trace().unwrap();
    handle_madness(&mut state);
    let actual: Vec<_> = trace
        .finish()
        .unwrap()
        .into_iter()
        .map(|e| e.call)
        .collect();
    let raw_calls = after["rng_call_witness"]["calls"].as_array().unwrap();
    let expected: Vec<_> = raw_calls
        .iter()
        .map(|call| {
            assert_eq!(call["owner"], "floor.card_random");
            assert_eq!(call["operation"]["method"], "random_int");
            StsRngCall::RandomInt {
                range: call["operation"]["range"].as_i64().unwrap() as i32,
                result: call["operation"]["result"].as_i64().unwrap() as i32,
            }
        })
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 3);
    assert_eq!(state.rng.card_random_rng, rng(stream(after)));
    let after_hand = after["parity_internal"]["zones"]["hand"]["cards"]
        .as_array()
        .unwrap();
    for (card, source) in state.zones.hand.iter().zip(hand) {
        let java = after_hand
            .iter()
            .find(|j| j["instance_uuid"] == source["instance_uuid"])
            .unwrap();
        assert_eq!(
            card.combat_cost_without_turn_override_java(),
            java["combat_cost"].as_i64().unwrap() as i32
        );
        assert_eq!(
            card.cost_for_turn_java(),
            java["cost_for_turn"].as_i64().unwrap() as i32
        );
    }
}
