use super::*;
use sts_oracle_run_control::eval::run_control::{RunControlConfig, RunControlSession};
use sts_parity_lockstep::{DomainEvidenceV1, LockstepAdapterV1, ParityDomainV1};
use sts_parity_lockstep_rust::{natural_run_bootstrap_v1, RustLockstepAdapterV1};

fn initial() -> (ParitySnapshotV1, NaturalRunBootstrapV1) {
    let config = RunControlConfig {
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let snapshot = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "test",
        "scope",
    )
    .unwrap()
    .observe()
    .unwrap();
    (snapshot, bootstrap)
}

// Protocol-only boundaries test qualification, not game execution or Java parity.
fn protocol_boundary(
    base: &ParitySnapshotV1,
    kind: &str,
    act: u64,
    floor: i64,
) -> ParitySnapshotV1 {
    let mut snapshot = base.clone();
    snapshot.observation_id = format!("{kind}-{act}-{floor}");
    snapshot.boundary_kind = kind.to_owned();
    let DomainEvidenceV1::Observed { value, .. } = &mut snapshot
        .domains
        .get_mut(&ParityDomainV1::Run)
        .unwrap()
        .evidence
    else {
        unreachable!()
    };
    value["act"] = act.into();
    value["floor"] = floor.into();
    snapshot
}

fn lifecycle() -> OfflineReplayReportV1 {
    let (mut before, bootstrap) = initial();
    let mut coverage = ReplayCoverageBuilderV1::new(&before, &bootstrap).unwrap();
    for (kind, act, floor, surface) in [
        ("natural_map", 2, 18, DecisionSurfaceV1::BossRelic),
        ("natural_map", 3, 35, DecisionSurfaceV1::BossRelic),
        ("natural_map", 4, 52, DecisionSurfaceV1::Act3Victory),
        (
            "natural_true_victory_complete",
            4,
            56,
            DecisionSurfaceV1::Combat,
        ),
        (
            "natural_heart_victory",
            4,
            57,
            DecisionSurfaceV1::TrueVictory,
        ),
    ] {
        let after = protocol_boundary(&before, kind, act, floor);
        coverage
            .observe_exact(&before, &SemanticActionV1::Proceed { surface }, &after)
            .unwrap();
        if kind == "natural_true_victory_complete" {
            assert!(
                !coverage.report.complete_heart_run,
                "Heart kill still awaits Proceed"
            );
        }
        before = after;
    }
    OfflineReplayReportV1 {
        domains: 13,
        committed_actions: 5,
        java_actions: 5,
        coverage: coverage.finish(),
    }
}

fn case(role: ReplayCaseRoleV1) -> ReplayCaseV1 {
    ReplayCaseV1 {
        id: "baseline".into(),
        purpose: "Whole-run qualification".into(),
        role,
        evidence: "evidence".into(),
        required_flows: vec![],
    }
}

#[test]
fn baseline_qualification_requires_natural_start_all_acts_and_final_proceed() {
    let replay = lifecycle();
    assert!(replay.coverage.complete_heart_run);
    assert!(case_failures(
        &replay.coverage.task,
        &case(ReplayCaseRoleV1::Baseline),
        &replay
    )
    .is_empty());
    let (initial, bootstrap) = initial();
    for (kind, act, floor) in [
        ("natural_map", 1, 1),
        ("natural_map", 2, 18),
        ("natural_true_victory_complete", 4, 56),
        ("natural_heart_victory", 4, 57),
    ] {
        let root = protocol_boundary(&initial, kind, act, floor);
        let builder = ReplayCoverageBuilderV1::new(&root, &bootstrap).unwrap();
        assert!(
            !builder.finish().complete_heart_run,
            "a terminal or middle root is not a whole run"
        );
    }
    let mut partial = replay.clone();
    partial.coverage.complete_heart_run = false;
    assert!(!case_failures(
        &replay.coverage.task,
        &case(ReplayCaseRoleV1::Baseline),
        &partial
    )
    .is_empty());
    assert!(case_failures(
        &replay.coverage.task,
        &case(ReplayCaseRoleV1::Supplement),
        &partial
    )
    .is_empty());
}

#[test]
fn flow_witnesses_deduplicate_and_reject_discontinuous_or_backwards_progress() {
    let (initial, bootstrap) = initial();
    let action = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::Neow,
    };
    let mut builder = ReplayCoverageBuilderV1::new(&initial, &bootstrap).unwrap();
    let mut next = initial.clone();
    next.observation_id = "second".into();
    builder.observe_exact(&initial, &action, &next).unwrap();
    assert!(builder.observe_exact(&initial, &action, &next).is_err());
    let mut last = next.clone();
    last.observation_id = "third".into();
    builder.observe_exact(&next, &action, &last).unwrap();
    let report = builder.finish();
    assert_eq!(report.flows.len(), 1);
    assert_eq!(
        (report.flows[0].first_action, report.flows[0].occurrences),
        (1, 2)
    );
    for (act, floor) in [(3, 35), (1, -1)] {
        let mut builder = ReplayCoverageBuilderV1::new(&initial, &bootstrap).unwrap();
        let after = protocol_boundary(&initial, "natural_map", act, floor);
        assert!(builder.observe_exact(&initial, &action, &after).is_err());
    }
}

#[test]
fn requirements_are_checked_against_executed_flows_and_task() {
    let replay = lifecycle();
    let mut spec = case(ReplayCaseRoleV1::Baseline);
    spec.required_flows = vec![replay.coverage.flows[0].flow.clone()];
    assert!(case_failures(&replay.coverage.task, &spec, &replay).is_empty());
    spec.required_flows[0].after = "natural_hand_select".into();
    assert!(case_failures(&replay.coverage.task, &spec, &replay)[0].contains("flow absent"));
    let mut task = replay.coverage.task.clone();
    task.ascension = 0;
    assert!(case_failures(&task, &spec, &replay)[0].contains("task differs"));
    spec.required_flows[0].surface = None;
    assert!(validate_flows(&spec.required_flows).is_err());
}

#[test]
fn suite_requires_a_baseline_and_rejects_duplicate_cases_and_requirements() {
    let replay = lifecycle();
    let mut suite = ReplaySuiteV1 {
        schema: "sts-parity-replay-suite-v1".into(),
        task: replay.coverage.task,
        cases: vec![case(ReplayCaseRoleV1::Supplement)],
        required_flows: vec![],
    };
    assert!(validate_suite(&suite).unwrap_err().contains("baseline"));
    suite.cases[0].role = ReplayCaseRoleV1::Baseline;
    validate_suite(&suite).unwrap();
    suite.cases.push(case(ReplayCaseRoleV1::Baseline));
    assert!(validate_suite(&suite).is_err());
    let flow = replay.coverage.flows[0].flow.clone();
    assert!(validate_flows(&[flow.clone(), flow]).is_err());
}

#[test]
fn recorded_prefix_is_replayed_but_does_not_qualify_as_a_baseline_or_fill_suite_rows() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/corpus_projection_drift/java-evidence");
    let replay = crate::replay_recorded_evidence_v1(&source).unwrap();
    assert_eq!(replay.coverage.flows.len(), 1);
    assert!(!replay.coverage.complete_heart_run);
    let directory = std::env::temp_dir().join(format!("sts-replay-suite-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("suite.json");
    let suite = serde_json::json!({
        "schema":"sts-parity-replay-suite-v1", "task":replay.coverage.task,
        "cases":[
            {"id":"missing-regression", "purpose":"Missing source is a case failure", "role":"regression", "evidence":"missing", "required_flows":[]},
            {"id":"short-prefix", "purpose":"Reject incomplete baseline", "role":"baseline", "evidence":source, "required_flows":[]}
        ],
        "required_flows":[replay.coverage.flows[0].flow]
    });
    fs::write(&path, serde_json::to_vec(&suite).unwrap()).unwrap();
    let report = replay_suite_v1(&path).unwrap();
    assert!(!report.passed);
    assert_eq!(report.cases.len(), 2);
    assert!(report.cases[0].replay.is_none());
    assert!(
        report.cases[1].replay.is_some(),
        "one missing source must not prevent other cases replaying"
    );
    assert!(report.cases[1].failures[0].contains("Neow through formal Heart victory"));
    assert!(report.required_flows[0].witnesses.is_empty());
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}
