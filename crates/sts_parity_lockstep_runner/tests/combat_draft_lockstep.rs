use std::cmp::Reverse;

use sts_combat_planner::{
    CombatPlanningQuantum, CompleteTurnOptionBoundary, TurnOptionGenerationStatus,
    TurnOptionGeneratorConfig,
};
use sts_complete_turn_env::{CombatDraftEnvStatusV1, CombatDraftEnvV1};
use sts_oracle_run_control::{
    content::events::neow::NeowEligibilityV1,
    eval::run_control::{RunControlConfig, RunControlSession},
    sim::combat::CombatPosition,
};
use sts_parity_lockstep::{
    DecisionSurfaceV1, DomainEvidenceV1, LockstepAdapterV1, ParityDomainV1, ParitySnapshotV1,
    SemanticActionV1,
};
use sts_parity_lockstep_rust::{natural_run_bootstrap_v1, RustLockstepAdapterV1};

#[test]
fn analyst_draft_witness_translates_and_replays_through_the_rust_lockstep_adapter() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).expect("natural bootstrap");
    let mut adapter = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        "test-build",
        "draft-lockstep-epoch",
    )
    .expect("Rust lockstep adapter");

    let mut current = advance_mini_neow_to_map(&mut adapter);
    let route = action_space(&current)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    ..
                }
            )
        })
        .expect("Act 1 map should expose a first-floor route");
    current = execute(&mut adapter, &current, &route);
    assert_eq!(current.boundary_kind, "natural_combat");

    let decision_step = adapter.session().decision_step;
    let active = adapter
        .session()
        .active_combat
        .as_ref()
        .expect("the map route should construct an active combat");
    let mut draft = CombatDraftEnvV1::from_position(
        CombatPosition::new(active.engine_state.clone(), active.combat_state.clone()),
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            allow_potion_expenditure: false,
            allow_potion_discard: false,
            ..TurnOptionGeneratorConfig::default()
        },
    )
    .expect("natural combat should be a valid draft root");

    for _ in 0..8 {
        let advanced = draft
            .advance(CombatPlanningQuantum::deterministic(1_024, 65_536))
            .expect("draft generation should advance once");
        assert_eq!(
            advanced.generation.status,
            TurnOptionGenerationStatus::Complete,
            "each selected turn requires a complete candidate surface"
        );
        let selected = advanced
            .view
            .options
            .iter()
            .filter_map(|option| {
                let label = option.handle.to_string();
                let detail = draft
                    .inspect(&label)
                    .expect("every draft index entry should replay for inspection");
                (!matches!(
                    detail.boundary,
                    CompleteTurnOptionBoundary::TerminalLoss | CompleteTurnOptionBoundary::Escape
                ))
                .then_some((
                    detail.boundary != CompleteTurnOptionBoundary::TerminalWin,
                    detail
                        .successor
                        .monsters
                        .iter()
                        .filter(|monster| monster.alive)
                        .map(|monster| monster.hp.max(0))
                        .sum::<i32>(),
                    Reverse(detail.successor.player.hp),
                    option.handle.ordinal(),
                    label,
                ))
            })
            .min()
            .expect("a living combat should expose a non-loss complete turn");
        let boundary = draft
            .choose(&selected.4)
            .expect("the observed current-frame option should be selectable");
        if boundary == CompleteTurnOptionBoundary::TerminalWin {
            break;
        }
    }

    assert!(matches!(
        draft.status(),
        CombatDraftEnvStatusV1::Terminal {
            boundary: CompleteTurnOptionBoundary::TerminalWin,
            ..
        }
    ));
    let witness = draft
        .victory_witness()
        .expect("the selected draft should replay from its first root")
        .expect("the terminal win should export an exact witness");
    let inputs = witness
        .actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let semantics = adapter
        .semantic_winning_combat_trajectory_from_inputs_v1(&inputs)
        .expect("every draft input should resolve to one shared semantic action");

    assert_eq!(semantics.len(), inputs.len());
    assert!(adapter.verify_semantic_combat_victory_v1(&[]).is_err());
    let verified = adapter
        .verify_semantic_combat_victory_v1(&semantics)
        .expect("an explicit analyst line should verify without search or live mutation");
    verify_adopted_draft_execution(&adapter, &current, &semantics, &verified);
    let mut beyond_combat = semantics.clone();
    beyond_combat.push(SemanticActionV1::EndTurn);
    assert!(adapter
        .verify_semantic_combat_victory_v1(&beyond_combat)
        .is_err());
    assert_eq!(
        adapter.session().decision_step,
        decision_step,
        "translation must not mutate the live lockstep endpoint"
    );
    for semantic in semantics {
        assert!(action_space(&current).contains(&semantic));
        current = execute(&mut adapter, &current, &semantic);
    }
    assert_eq!(current.boundary_kind, "natural_combat_reward");
    assert_eq!(current, verified);
}

fn verify_adopted_draft_execution(
    adapter: &RustLockstepAdapterV1,
    root: &ParitySnapshotV1,
    actions: &[SemanticActionV1],
    terminal: &ParitySnapshotV1,
) {
    use std::{cell::Cell, rc::Rc};
    use sts_parity_lockstep::{LockstepModeV1, LockstepRunnerV1};
    use sts_parity_lockstep_runner::{commit_manual_combat_draft_v1, ManualCombatDraftV1};
    let draft = ManualCombatDraftV1 {
        schema: "sts-manual-combat-draft-v1".into(),
        session_id: "fixture".into(),
        rust_version: "fixture".into(),
        source: None,
        root: root.clone(),
        actions: actions.to_vec(),
        terminal: terminal.clone(),
    };
    verify_draft_handles(adapter, &draft);
    for case in [
        "exact",
        "mismatch",
        "capture_failure",
        "foreign_root",
        "incomplete",
    ] {
        let directory = std::env::temp_dir().join(format!("adopt-draft-{}", uuid::Uuid::new_v4()));
        let calls = Rc::new(Cell::new(0));
        let oracle = CountedOracle {
            rust: adapter.clone(),
            calls: calls.clone(),
            mismatch_at: if case == "mismatch" { Some(2) } else { None },
        };
        let mut runner = LockstepRunnerV1::start_new(
            oracle,
            adapter.clone(),
            LockstepModeV1::Audit,
            &directory,
            "test",
        )
        .unwrap();
        let mut candidate = draft.clone();
        if case == "foreign_root" {
            change_gold(&mut candidate.root);
        }
        if case == "incomplete" {
            candidate.actions.truncate(1);
        }
        let mut accepted = 0;
        let result = commit_manual_combat_draft_v1(&mut runner, &candidate, |_, _| {
            accepted += 1;
            if case == "capture_failure" {
                Err("capture unavailable".into())
            } else {
                Ok(())
            }
        });
        match case {
            "exact" => {
                assert_eq!(result.unwrap(), actions.len());
                assert_eq!(calls.get(), actions.len());
            }
            "mismatch" => {
                assert!(result.is_err());
                assert_eq!(calls.get(), 2);
                assert_eq!(accepted, 1);
                assert!(runner.is_stopped());
            }
            "capture_failure" => {
                assert!(result.is_err());
                assert_eq!(calls.get(), 1);
            }
            _ => {
                assert!(result.is_err());
                assert_eq!(calls.get(), 0);
            }
        }
        // This test owns the UUID-scoped temp directory created above.
        assert!(directory.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}

fn verify_draft_handles(
    adapter: &RustLockstepAdapterV1,
    draft: &sts_parity_lockstep_runner::ManualCombatDraftV1,
) {
    use std::{fs, path::Path};
    use sts_oracle_run_control::eval::run_control::RunControlSessionCheckpointV1;
    use sts_parity_lockstep_runner::{
        list_manual_combat_drafts_v1, load_manual_combat_draft_v1, save_manual_combat_draft_v1,
        ManualSessionV1,
    };
    let repository = std::env::temp_dir().join(format!("draft-handles-{}", uuid::Uuid::new_v4()));
    let session = ManualSessionV1::prepare(&repository).unwrap();
    let mut draft = draft.clone();
    draft.session_id = session.session_id().to_owned();
    let handle = save_manual_combat_draft_v1(&session, &draft).unwrap();
    assert_eq!(
        load_manual_combat_draft_v1(&session, Path::new(&handle))
            .unwrap()
            .actions,
        draft.actions
    );
    // Handles survive a native replacement via typed ancestry, without an
    // offline replay or scanning unrelated/abandoned session directories.
    let replacement = ManualSessionV1::prepare(&repository).unwrap();
    replacement
        .publish_rust_root(
            session.session_id(),
            1,
            RunControlSessionCheckpointV1::from_session(adapter.session()),
        )
        .unwrap();
    assert_eq!(
        list_manual_combat_drafts_v1(&replacement, &draft.root).unwrap(),
        vec![(handle.clone(), draft.actions.len())]
    );
    let unrelated = ManualSessionV1::prepare(&repository).unwrap();
    assert!(load_manual_combat_draft_v1(&unrelated, Path::new(&handle)).is_err());
    let mut changed_root = draft.root.clone();
    change_gold(&mut changed_root);
    assert!(list_manual_combat_drafts_v1(&replacement, &changed_root)
        .unwrap()
        .is_empty());

    let original = fs::read_dir(session.directory().join("drafts"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    // Force a short-prefix collision: no implicit first/latest selection.
    let mut other_id = original.file_stem().unwrap().to_str().unwrap().to_owned();
    other_id.replace_range(31..32, if other_id.ends_with('0') { "1" } else { "0" });
    let collision = original.with_file_name(format!("{other_id}.json"));
    fs::copy(&original, &collision).unwrap();
    assert!(
        load_manual_combat_draft_v1(&replacement, Path::new(&handle))
            .unwrap_err()
            .contains("ambiguous")
    );
    let listed = list_manual_combat_drafts_v1(&replacement, &draft.root).unwrap();
    assert_eq!(listed.len(), 2);
    for (longer, _) in listed {
        assert!(longer.len() > handle.len());
        load_manual_combat_draft_v1(&replacement, Path::new(&longer)).unwrap();
    }
    draft.session_id = unrelated.session_id().to_owned();
    fs::write(&collision, serde_json::to_vec(&draft).unwrap()).unwrap();
    assert!(
        load_manual_combat_draft_v1(&replacement, Path::new(&format!("D{other_id}")))
            .unwrap_err()
            .contains("source disagrees")
    );
    assert!(save_manual_combat_draft_v1(&session, &draft).is_err());
    assert!(repository.starts_with(std::env::temp_dir()));
    fs::remove_dir_all(repository).unwrap();
}

fn change_gold(snapshot: &mut ParitySnapshotV1) {
    if let DomainEvidenceV1::Observed { value, .. } = &mut snapshot
        .domains
        .get_mut(&ParityDomainV1::Run)
        .unwrap()
        .evidence
    {
        value["gold"] = serde_json::json!(999_999);
    } else {
        panic!("missing Run evidence");
    }
}

struct CountedOracle {
    rust: RustLockstepAdapterV1,
    calls: std::rc::Rc<std::cell::Cell<usize>>,
    mismatch_at: Option<usize>,
}

impl LockstepAdapterV1 for CountedOracle {
    type PreparedAction = <RustLockstepAdapterV1 as LockstepAdapterV1>::PreparedAction;
    fn observe(&mut self) -> Result<ParitySnapshotV1, sts_parity_lockstep::AdapterFailureV1> {
        self.rust.observe()
    }
    fn prepare(
        &mut self,
        action: &SemanticActionV1,
        based_on: &ParitySnapshotV1,
    ) -> Result<Self::PreparedAction, sts_parity_lockstep::AdapterFailureV1> {
        self.rust.prepare(action, based_on)
    }
    fn execute(
        &mut self,
        action: Self::PreparedAction,
    ) -> Result<ParitySnapshotV1, sts_parity_lockstep::AdapterFailureV1> {
        self.calls.set(self.calls.get() + 1);
        let mut result = self.rust.execute(action)?;
        if self.mismatch_at == Some(self.calls.get()) {
            change_gold(&mut result);
        }
        Ok(result)
    }
    fn last_rng_call_ledger_v1(&self) -> Option<&sts_parity_lockstep::NaturalRunRngCallLedgerV1> {
        self.rust.last_rng_call_ledger_v1()
    }
}

fn advance_mini_neow_to_map(adapter: &mut RustLockstepAdapterV1) -> ParitySnapshotV1 {
    let introduction = adapter.observe().expect("Neow introduction");
    let proceed = action_space(&introduction)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::Neow
                }
            )
        })
        .expect("Neow introduction proceed");
    let reward = execute(adapter, &introduction, &proceed);
    let choose = action_space(&reward)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Neow,
                    ..
                }
            )
        })
        .expect("Mini Neow reward choice");
    let leave = execute(adapter, &reward, &choose);
    let proceed = action_space(&leave)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::Neow
                }
            )
        })
        .expect("completed Neow leave");
    execute(adapter, &leave, &proceed)
}

fn execute(
    adapter: &mut RustLockstepAdapterV1,
    current: &ParitySnapshotV1,
    action: &SemanticActionV1,
) -> ParitySnapshotV1 {
    let prepared = adapter.prepare(action, current).expect("prepare action");
    adapter.execute(prepared).expect("execute action")
}

fn action_space(snapshot: &ParitySnapshotV1) -> Vec<SemanticActionV1> {
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::ActionSpace].evidence
    else {
        panic!("action space should be observed")
    };
    serde_json::from_value(value.clone()).expect("typed semantic action space")
}
