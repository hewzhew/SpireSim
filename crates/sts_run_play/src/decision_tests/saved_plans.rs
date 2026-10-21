use super::*;
use crate::agent_protocol::{run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1};
use serde_json::{json, Value};

// Each exchange starts a new transport/cache, as separate ./p invocations do.
fn exchange(game: &mut RunPlayControllerV1, mut request: Value) -> Value {
    request["schema"] = "sts-run-play-agent-request-v1".into();
    request["request_id"] = "saved-plans-test".into();
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None)),
        std::io::Cursor::new(format!("{request}\n")),
        &mut output,
    )
    .unwrap();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn public_manual_play_disables_private_operations_and_cached_wins_but_can_act() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let searched = exchange(
        &mut game,
        json!({"command":"combat_research", "effort":"whole_combat", "max_search_ms":1000}),
    );
    assert_eq!(searched["status"], "ok");
    assert!(!searched["result"]["candidates"]
        .as_array()
        .unwrap()
        .is_empty());
    let before = game.head().head_revision;
    let requests = [
        json!({"command":"hello"}),
        json!({"command":"decision"}),
        json!({"command":"sl_piles"}),
        json!({"command":"probe", "lines":[["end"]]}),
        json!({"command":"combat_research", "effort":"whole_combat", "max_search_ms":1}),
        json!({"command":"combat_plans"}),
        json!({"command":"act", "actions":["end"]}),
    ];
    let input = requests
        .into_iter()
        .enumerate()
        .map(|(id, mut request)| {
            request["schema"] = "sts-run-play-agent-request-v1".into();
            request["request_id"] = id.to_string().into();
            format!("{request}\n")
        })
        .collect::<String>();
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::public_manual_v1(None)),
        std::io::Cursor::new(input),
        &mut output,
    )
    .unwrap();
    let responses = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        responses[0]["result"]["capabilities"]["available_commands"],
        json!([
            "decision",
            "choose",
            "hello",
            "observe",
            "game_facts",
            "info",
            "act",
            "quit"
        ])
    );
    assert!(!responses[1]["result"]["reading"]["view"]
        .as_str()
        .unwrap()
        .contains("Saved combat wins"));
    for response in &responses[2..6] {
        assert_eq!(response["status"], "error", "{response}");
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Public manual play disables"));
    }
    assert_eq!(responses[6]["status"], "ok", "{}", responses[6]);
    assert!(game.head().head_revision > before);
}

#[test]
fn ordinary_read_exposes_current_saved_wins_without_search_or_publication() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let first = exchange(&mut game, json!({"command":"decision"}));
    assert_eq!(first["status"], "ok");
    assert!(!first["result"]["reading"]["view"]
        .as_str()
        .unwrap()
        .contains("Saved combat wins"));
    let searched = exchange(
        &mut game,
        json!({"command":"combat_research", "effort":"whole_combat", "max_search_ms":1000}),
    );
    assert_eq!(searched["status"], "ok", "{searched}");
    assert!(!searched["result"]["candidates"]
        .as_array()
        .unwrap()
        .is_empty());
    let before = game.head().clone();
    let path = game.paths().root().join("combat-plans.json");
    let bytes = fs::read(&path).unwrap();
    let plans = exchange(&mut game, json!({"command":"combat_plans"}));
    let reading = exchange(&mut game, json!({"command":"decision"}));
    assert_eq!(reading["status"], "ok");
    let view = reading["result"]["reading"]["view"].as_str().unwrap();
    assert!(
        view.contains("Saved combat wins (already replayed; plans/review/commit)"),
        "{view}"
    );
    assert!(view.contains(plans["result"]["view"].as_str().unwrap()));
    assert_eq!(*game.head(), before);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let mut public_permission = AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None);
    public_permission.information_ceiling = "public_observation_only";
    let mut public_output = Vec::new();
    run_agent_jsonl_v1(&mut game, AgentCapabilitiesV1::new(public_permission),
        std::io::Cursor::new("{\"schema\":\"sts-run-play-agent-request-v1\",\"request_id\":\"public-read\",\"command\":\"decision\"}\n"),
        &mut public_output).unwrap();
    let public_read: Value = serde_json::from_slice(&public_output).unwrap();
    assert_eq!(public_read["status"], "ok");
    assert!(
        !public_read["result"]["reading"]["view"]
            .as_str()
            .unwrap()
            .contains("Saved combat wins"),
        "public-only reads must not inherit privately computed future results"
    );
    let preview = exchange(&mut game, json!({"command":"probe", "lines":[["end"]]}));
    assert_eq!(preview["status"], "ok");
    assert!(
        !preview["result"]["lines"][0]["view"]
            .as_str()
            .unwrap()
            .contains("Saved combat wins"),
        "private successors must not borrow the actual root's saved plans"
    );
    let acted = exchange(&mut game, json!({"command":"act","actions":["end"]}));
    assert_eq!(acted["status"], "ok");
    let reading = exchange(&mut game, json!({"command":"decision"}));
    assert_eq!(reading["status"], "ok");
    assert!(!reading["result"]["reading"]["view"]
        .as_str()
        .unwrap()
        .contains("Saved combat wins"));
    assert_eq!(
        fs::read(path).unwrap(),
        bytes,
        "read must neither search nor replace stale plans"
    );
    assert_eq!(
        exchange(&mut game, json!({"command":"combat_plans"}))["status"],
        "error"
    );
}

#[test]
fn corrupt_optional_plan_cache_does_not_hide_the_actual_game_or_repair_it_silently() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_multiturn");
    let before = game.head().clone();
    let path = game.paths().root().join("combat-plans.json");
    fs::write(&path, b"not a valid report").unwrap();
    let reading = exchange(&mut game, json!({"command":"decision"}));
    assert_eq!(reading["status"], "ok");
    let view = reading["result"]["reading"]["view"].as_str().unwrap();
    assert!(
        view.contains("Hand:") && view.contains("Saved plan cache unavailable"),
        "{view}"
    );
    assert!(!view.contains("Saved combat wins"));
    assert_eq!(*game.head(), before);
    assert_eq!(fs::read(path).unwrap(), b"not a valid report");
}

#[test]
fn chosen_successors_keep_bound_choices_and_resources_in_a_brief_response() {
    let dir = TestDirectory::new();
    for kind in ["reward", "full_reward"] {
        let mut game = controller(&dir.0, kind);
        let before = game.decision_reading_v1(&[], None).unwrap();
        let choice = pick(&before, |a| match kind {
            "reward" => matches!(a, TaskStrategicActionV1::SkipCardReward { .. }),
            _ => matches!(
                a,
                TaskStrategicActionV1::ClaimReward {
                    reward: PlannerRewardDescriptor::Gold { .. },
                    ..
                }
            ),
        });
        let response = exchange(&mut game, json!({"command":"choose", "selection":choice}));
        assert_eq!(response["status"], "ok", "{response}");
        let brief = game
            .decision_reading_v1(&[DecisionDetailV1::Brief], None)
            .unwrap();
        assert_eq!(response["result"]["reading"], json!(brief));
        let full = game.decision_reading_v1(&[], None).unwrap();
        assert_eq!(brief.source, full.source);
        assert_eq!(brief.options, full.options);
        assert_eq!(brief.selections, full.selections);
        assert_eq!(brief.last_choice, full.last_choice);
        assert!(brief.view.contains("HP ") && brief.view.contains("Potions:"));
        assert!(full.view.len() > brief.view.len());
        if kind == "full_reward" {
            assert!(brief.view.contains("Potion slots full."));
        }
    }
}

#[test]
fn private_preview_exposes_the_same_followup_choices_and_rules_as_real_execution() {
    let dir = TestDirectory::new();
    for (kind, action, prompt) in [
        (
            "combat_discovery_preview",
            "use-potion:PowerPotion#0",
            "Discovery | Choose 1",
        ),
        ("combat_brew_draft", "use-potion:GamblersBrew#0", "Discard"),
        ("combat_pact_draft", "play:BurningPact", "Exhaust"),
    ] {
        let mut game = controller(&dir.0, kind);
        let root = game.head().clone();
        let preview = exchange(&mut game, json!({"command":"probe", "lines":[[action]]}));
        assert_eq!(preview["status"], "ok", "{preview}");
        assert_eq!(*game.head(), root);
        assert_eq!(game.paths().load_head().unwrap(), root);
        let view = preview["result"]["lines"][0]["view"].as_str().unwrap();
        assert!(view.contains(prompt), "{view}");
        if kind == "combat_discovery_preview" {
            assert!(view.contains("Choice cards:"), "{view}");
            assert!(
                view.lines()
                    .any(|line| line.trim_start().starts_with("choose:")),
                "{view}"
            );
        } else {
            assert!(
                view.lines().any(|line| {
                    let line = line.trim_start();
                    line.starts_with("select") || line.starts_with("Input: select")
                }),
                "{view}"
            );
        }
        let acted = exchange(&mut game, json!({"command":"act", "actions":[action]}));
        assert_eq!(acted["status"], "ok", "{acted}");
        assert_eq!(
            preview["result"]["lines"][0]["view"],
            acted["result"]["reading"]["view"]
        );
    }
}

#[test]
fn displayed_combat_choices_resume_private_sequences_without_client_aliases() {
    for (kind, opening, multi) in [
        ("combat_pact_draft", "play:BurningPact", false),
        ("combat_brew_draft", "use-potion:GamblersBrew#0", true),
        (
            "combat_discovery_preview",
            "use-potion:PowerPotion#0",
            false,
        ),
    ] {
        let dir = TestDirectory::new();
        let mut game = controller(&dir.0, kind);
        let root = game.head().clone();
        let pending = exchange(&mut game, json!({"command":"probe", "lines":[[opening]]}));
        assert_eq!(pending["status"], "ok", "{pending}");
        let view = pending["result"]["lines"][0]["view"].as_str().unwrap();
        let printed = view
            .lines()
            .find_map(|line| {
                let (token, _) = line.trim_start().split_once("  |  ")?;
                if token.starts_with("choose:")
                    || token.starts_with("select:") && token != "select:none"
                {
                    Some(token.to_owned())
                } else if multi && token.contains('#') {
                    Some(format!("select:{token}"))
                } else {
                    None
                }
            })
            .expect("pending view must contain a directly usable choice");
        assert!(!view
            .lines()
            .any(|line| line.trim_start().starts_with("select ")));
        assert!(!view
            .lines()
            .any(|line| line.trim_start().starts_with("0: ")));
        if multi {
            assert!(view.contains("Input: select:<card tokens joined by commas without spaces>"));
        }
        let line = [opening, printed.as_str()];
        let probed = exchange(&mut game, json!({"command":"probe", "lines":[line]}));
        assert_eq!(probed["status"], "ok", "{probed}");
        assert_eq!(probed["result"]["lines"][0]["stop"]["kind"], "complete");
        assert_eq!(*game.head(), root);
        assert_eq!(game.paths().load_head().unwrap(), root);
        let acted = exchange(&mut game, json!({"command":"act", "actions":line}));
        assert_eq!(acted["status"], "ok", "{acted}");
        assert_eq!(
            probed["result"]["lines"][0]["view"],
            acted["result"]["reading"]["view"]
        );
    }
}

#[test]
fn private_preview_embeds_choices_and_matches_atomic_execution() {
    for (kind, opening) in [
        ("combat_pact_draft", "play:BurningPact"),
        ("combat_brew_draft", "use-potion:GamblersBrew#0"),
    ] {
        let dir = TestDirectory::new();
        let mut game = controller(&dir.0, kind);
        let root = game.head().clone();
        let line = [opening, "select:Strike#0", "play:Defend"];
        let preview = exchange(&mut game, json!({"command":"probe","lines":[line]}));
        assert_eq!(preview["status"], "ok", "{preview}");
        assert_eq!(*game.head(), root);
        assert_eq!(game.paths().load_head().unwrap(), root);
        let skipped_choice = [opening, "play:Defend"];
        for command in ["act", "probe"] {
            let request = if command == "act" {
                json!({"command":command,"actions":skipped_choice})
            } else {
                json!({"command":command,"lines":[skipped_choice]})
            };
            let rejected = exchange(&mut game, request);
            let error = if command == "act" {
                assert_eq!(rejected["status"], "error", "{rejected}");
                rejected["error"]["message"].as_str().unwrap()
            } else {
                assert_eq!(rejected["status"], "ok", "{rejected}");
                let partial = &rejected["result"]["lines"][0];
                assert_eq!(partial["stop"]["kind"], "unresolved");
                assert_eq!(partial["stop"]["requested_step"], 2);
                assert_eq!(partial["steps"].as_array().unwrap().len(), 1);
                assert!(partial["view"].as_str().unwrap().contains("Choice cards:"));
                partial["stop"]["error"].as_str().unwrap()
            };
            assert!(
                error.contains("Both act and preview support choices"),
                "{error}"
            );
            assert_eq!(*game.head(), root);
            assert_eq!(game.paths().load_head().unwrap(), root);
        }
        let acted = exchange(&mut game, json!({"command":"act","actions":line}));
        assert_eq!(acted["status"], "ok", "{acted}");
        let current = game
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        assert_eq!(
            json!(crate::agent_protocol::project_agent_decision_v1(&current).unwrap()),
            preview["result"]["lines"][0]["endpoint"]
        );
    }
}

#[test]
fn continuation_report_binds_midturn_root_and_counts_only_remaining_potion_spending() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_draft_edit");
    let acted = exchange(
        &mut game,
        json!({"command":"act",
        "actions":["use-potion:BlockPotion#0", "play:Strike#0@0"]}),
    );
    assert_eq!(acted["status"], "ok", "{acted}");
    let searched = exchange(
        &mut game,
        json!({"command":"combat_research",
        "effort":"fast_witness", "max_search_ms":1000}),
    );
    assert_eq!(searched["status"], "ok", "{searched}");
    let view = searched["result"]["view"].as_str().unwrap();
    assert!(view.contains("from T1 HP 68/75"), "{view}");
    assert!(view.contains("Additional potions 0"), "{view}");
    let reopened = exchange(&mut game, json!({"command":"combat_plans"}));
    assert_eq!(reopened["status"], "ok", "{reopened}");
    assert_eq!(reopened["result"]["view"], searched["result"]["view"]);
    let full = exchange(
        &mut game,
        json!({"command":"combat_plans", "diagnostics":true}),
    );
    assert_eq!(full["status"], "ok", "{full}");
    let view = full["result"]["view"].as_str().unwrap();
    assert!(view.contains("Search starts here: Turn 1 | HP 68/75 | Energy 2 | Block 12 | Cards played this turn 1"), "{view}");
    assert!(view.contains("Potions: P0 empty"), "{view}");
    assert!(view.contains("Additional potions 0"), "{view}");
}

#[test]
fn saved_plans_survive_mark_metadata_edits_without_rewriting_the_report() {
    let dir = TestDirectory::new();
    let source = controller(&dir.0, "combat_finish");
    let paths = RunPlayPathsV1::new(dir.0.join("scratch-finish"));
    crate::driver::adopt_current_session_v1(source.paths().clone(), &paths).unwrap();
    let mut game = RunPlayControllerV1::open(paths.clone()).unwrap();
    let searched = exchange(
        &mut game,
        json!({"command":"combat_research",
        "effort":"whole_combat","max_search_ms":1000}),
    );
    assert_eq!(searched["status"], "ok", "{searched}");
    let report_id = searched["result"]["report_id"].clone();
    let root_state_revision = game.head().state_revision;
    let bytes = fs::read(paths.root().join("combat-plans.json")).unwrap();
    drop(game);
    crate::driver::save_scratch_mark_v1(&paths, "before-adoption").unwrap();
    let mut game = RunPlayControllerV1::open(paths.clone()).unwrap();
    assert_eq!(game.head().state_revision, root_state_revision);
    let plans = exchange(&mut game, json!({"command":"combat_plans"}));
    assert_eq!(plans["status"], "ok", "{plans}");
    assert_eq!(plans["result"]["report_id"], report_id);
    let read = exchange(&mut game, json!({"command":"decision"}));
    assert!(read["result"]["reading"]["view"]
        .as_str()
        .unwrap()
        .contains("Saved combat wins"));
    assert_eq!(plans["result"]["candidates"][0]["minimum_observed_hp"], 68);
    let review = exchange(
        &mut game,
        json!({"command":"combat_review",
        "candidate_refs":["c0"],"effort":"whole_combat","report_id":report_id,"compact":true}),
    );
    assert_eq!(review["status"], "ok", "{review}");
    let review_view = review["result"]["view"].as_str().unwrap();
    assert!(review_view.contains("Settled HP 74/75"));
    assert!(review_view.contains("Minimum observed HP: 68"));
    assert!(review_view.contains("T1 | Start HP 68/75 | HP change +6 | Start Block 0"));
    assert!(review_view.contains("Strike @M0"));
    assert!(!review_view.contains("Settled HP includes"));
    drop(game);
    crate::driver::drop_scratch_mark_v1(&paths, "before-adoption").unwrap();
    let mut game = RunPlayControllerV1::open(paths.clone()).unwrap();
    assert_eq!(
        fs::read(paths.root().join("combat-plans.json")).unwrap(),
        bytes
    );
    let committed = exchange(
        &mut game,
        json!({"command":"commit_combat_plan",
        "candidate_ref":"c0","effort":"whole_combat","report_id":report_id}),
    );
    assert_eq!(committed["status"], "ok", "{committed}");
    assert_eq!(
        exchange(&mut game, json!({"command":"combat_plans"}))["status"],
        "error"
    );
}

#[test]
fn saved_plans_survive_reopen_and_commit_the_reviewed_execution_without_research() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let paths = game.paths().clone();
    let root = game.head().clone();
    assert_eq!(
        exchange(&mut game, json!({"command":"combat_plans"}))["status"],
        "error"
    );
    let searched = exchange(
        &mut game,
        json!({"command":"combat_research","effort":"whole_combat","max_search_ms":1000}),
    );
    assert_eq!(searched["status"], "ok", "{searched}");
    assert!(searched["result"]["view"]
        .as_str()
        .unwrap()
        .starts_with("CombatSolver | from T1 HP 68/75"));
    assert!(searched["result"]["view"]
        .as_str()
        .unwrap()
        .contains("./p commit 0"));
    let report_id = searched["result"]["report_id"].clone();
    let outcome = searched["result"]["candidates"][0]["outcome"].clone();
    assert!(report_id.is_string() && outcome.is_object());
    drop(game);
    let mut game = RunPlayControllerV1::open(paths.clone()).unwrap();
    let saved_bytes = fs::read(paths.root().join("combat-plans.json")).unwrap();
    let plans = exchange(&mut game, json!({"command":"combat_plans"}));
    assert_eq!(plans["status"], "ok", "{plans}");
    assert_eq!(plans["result"]["report_id"], report_id);
    assert_eq!(plans["result"]["view"], searched["result"]["view"]);
    assert_eq!(
        plans["result"]["candidates"],
        searched["result"]["candidates"]
    );
    let full = exchange(
        &mut game,
        json!({"command":"combat_plans", "diagnostics":true}),
    );
    assert_eq!(full["status"], "ok", "{full}");
    assert_eq!(full["result"]["report_id"], report_id);
    assert_eq!(full["result"]["candidates"], plans["result"]["candidates"]);
    let full_view = full["result"]["view"].as_str().unwrap();
    assert!(full_view.contains("Search target:"), "{full_view}");
    assert!(full_view.contains("Lane 0: work"), "{full_view}");
    assert!(
        full_view.contains("Next actions: play:Strike"),
        "{full_view}"
    );
    assert_eq!(*game.head(), root);
    assert_eq!(
        fs::read(paths.root().join("combat-plans.json")).unwrap(),
        saved_bytes
    );
    let reviewed = exchange(
        &mut game,
        json!({"command":"combat_review","candidate_refs":["c0"],
        "effort":"whole_combat","report_id":report_id,"compact":true}),
    );
    assert_eq!(reviewed["status"], "ok", "{reviewed}");
    assert_eq!(*game.head(), root);
    assert_eq!(
        fs::read(paths.root().join("combat-plans.json")).unwrap(),
        saved_bytes
    );
    let bad = exchange(
        &mut game,
        json!({"command":"commit_combat_plan","candidate_ref":"c0",
        "effort":"whole_combat","report_id":"wrong-report"}),
    );
    assert_eq!(bad["status"], "error");
    assert_eq!(*game.head(), root);
    let wrong_effort = exchange(
        &mut game,
        json!({"command":"commit_combat_plan","candidate_ref":"c0",
        "effort":"expanded","report_id":report_id}),
    );
    assert_eq!(wrong_effort["status"], "error");
    assert_eq!(*game.head(), root);
    let committed = exchange(
        &mut game,
        json!({"command":"commit_combat_plan","candidate_ref":"c0",
        "effort":"whole_combat","report_id":report_id}),
    );
    assert_eq!(committed["status"], "ok", "{committed}");
    assert!(game.head().head_revision > root.head_revision);
    assert_eq!(
        committed["result"]["reading"],
        json!(game
            .decision_reading_v1(&[DecisionDetailV1::Brief], None)
            .unwrap())
    );
    let current = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = current.observation else {
        panic!("expected reward");
    };
    assert_eq!(json!(observation.run.current_hp), outcome["final_hp"]);
    assert_eq!(
        exchange(&mut game, json!({"command":"combat_plans"}))["status"],
        "error"
    );
    // A fresh head revision after undo does not silently resurrect a handle.
    drop(game);
    crate::driver::back_v1(&paths, 1).unwrap();
    let mut game = RunPlayControllerV1::open(paths).unwrap();
    assert_eq!(
        exchange(&mut game, json!({"command":"combat_plans"}))["status"],
        "error"
    );
}

#[test]
fn saved_plans_reject_replaced_corrupt_and_externally_stale_reports() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let paths = game.paths().clone();
    let root = game.head().clone();
    let first = exchange(
        &mut game,
        json!({"command":"combat_research","effort":"fast_witness","max_search_ms":1000}),
    );
    assert_eq!(first["status"], "ok", "{first}");
    let second = exchange(
        &mut game,
        json!({"command":"combat_research","effort":"fast_witness","max_search_ms":2000}),
    );
    assert_eq!(second["status"], "ok", "{second}");
    let current_id = second["result"]["report_id"].clone();
    assert_ne!(first["result"]["report_id"], current_id);
    let stale = exchange(
        &mut game,
        json!({"command":"commit_combat_plan","candidate_ref":"c0",
        "effort":"fast_witness","report_id":first["result"]["report_id"]}),
    );
    assert_eq!(stale["status"], "error");
    assert!(stale["error"]["message"]
        .as_str()
        .unwrap()
        .contains("report changed"));
    assert_eq!(*game.head(), root);

    let saved_path = paths.root().join("combat-plans.json");
    let bytes = fs::read(&saved_path).unwrap();
    fs::write(&saved_path, b"not JSON").unwrap();
    let corrupt = exchange(&mut game, json!({"command":"combat_plans"}));
    assert_eq!(corrupt["status"], "error");
    assert_eq!(*game.head(), root);
    fs::write(&saved_path, &bytes).unwrap();

    // Another controller advances the published head; the old resident
    // controller must reject its plans before even reviewing them.
    let mut other = RunPlayControllerV1::open(paths.clone()).unwrap();
    let committed = exchange(
        &mut other,
        json!({"command":"commit_combat_plan","candidate_ref":"c0",
        "effort":"fast_witness","report_id":current_id}),
    );
    assert_eq!(committed["status"], "ok", "{committed}");
    let reviewed = exchange(
        &mut game,
        json!({"command":"combat_review","candidate_refs":["c0"],
        "effort":"fast_witness","report_id":current_id}),
    );
    assert_eq!(reviewed["status"], "error");
    assert_eq!(paths.load_head().unwrap(), *other.head());
}

#[test]
fn saved_plans_reject_missing_or_changed_producers_without_advancing_the_game() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let root = game.head().clone();
    let searched = exchange(
        &mut game,
        json!({"command":"combat_research","effort":"fast_witness","max_search_ms":1000}),
    );
    assert_eq!(searched["status"], "ok", "{searched}");
    let report_id = searched["result"]["report_id"].clone();
    let saved_path = game.paths().root().join("combat-plans.json");
    let bytes = fs::read(&saved_path).unwrap();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    let producer = json!(crate::model::RunPlayEvidenceIdentityV1::current_v1());

    for missing in [false, true] {
        let mut saved = original.clone();
        if missing {
            saved.as_object_mut().unwrap().remove("producer_identity");
        } else {
            saved["producer_identity"] = producer.clone();
            saved["producer_identity"]["build_source"]["source_sha256"] = "old-build".into();
        }
        fs::write(&saved_path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let read = exchange(&mut game, json!({"command":"decision"}));
        assert_eq!(read["status"], "ok");
        assert!(
            !read["result"]["reading"]["view"]
                .as_str()
                .unwrap()
                .contains("Saved combat wins"),
            "absent or changed producers must not expose the old report as a live win"
        );
        for request in [
            json!({"command":"combat_plans"}),
            json!({"command":"combat_review","candidate_refs":["c0"],
                "effort":"fast_witness","report_id":report_id,"compact":true}),
            json!({"command":"commit_combat_plan","candidate_ref":"c0",
                "effort":"fast_witness","report_id":report_id}),
        ] {
            let rejected = exchange(&mut game, request);
            assert_eq!(rejected["status"], "error", "{rejected}");
            assert!(
                rejected["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("producer"),
                "{rejected}"
            );
            assert_eq!(*game.head(), root);
            assert_eq!(game.paths().load_head().unwrap(), root);
        }
    }

    let refreshed = exchange(
        &mut game,
        json!({"command":"combat_research","effort":"fast_witness","max_search_ms":1000}),
    );
    assert_eq!(refreshed["status"], "ok", "{refreshed}");
    let saved: Value = serde_json::from_slice(&fs::read(saved_path).unwrap()).unwrap();
    assert_eq!(saved["producer_identity"], producer);
    assert_eq!(
        exchange(&mut game, json!({"command":"combat_plans"}))["status"],
        "ok"
    );
    assert_eq!(*game.head(), root);
}

#[test]
fn private_preview_keeps_the_head_on_success_and_illegal_later_action() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_multiturn");
    let root = game.head().clone();
    let line = [
        "play:Defend",
        "play:Strike",
        "end",
        "play:Defend",
        "play:Strike",
        "end",
    ];
    let before_files = fs::read_dir(game.paths().root().join("heads"))
        .unwrap()
        .count();
    let preview = exchange(&mut game, json!({"command":"probe","lines":[line]}));
    assert_eq!(preview["status"], "ok", "{preview}");
    assert_eq!(preview["result"]["durable_head_unchanged"], true);
    assert_eq!(
        preview["result"]["lines"][0]["steps"]
            .as_array()
            .unwrap()
            .len(),
        line.len()
    );
    assert!(preview["result"]["lines"][0]["view"]
        .as_str()
        .unwrap()
        .contains("HP"));
    assert_eq!(*game.head(), root);
    let mut illegal_line = line.to_vec();
    illegal_line.push("play:DefinitelyAbsent");
    let failed = exchange(&mut game, json!({"command":"probe","lines":[illegal_line]}));
    assert_eq!(failed["status"], "ok", "{failed}");
    let partial = &failed["result"]["lines"][0];
    assert_eq!(partial["stop"]["kind"], "unresolved");
    assert_eq!(partial["stop"]["requested_step"], 7);
    assert_eq!(partial["stop"]["unapplied_actions"], 1);
    assert_eq!(
        partial["endpoint"],
        preview["result"]["lines"][0]["endpoint"]
    );
    assert_eq!(*game.head(), root);
    assert_eq!(game.paths().load_head().unwrap(), root);
    assert_eq!(
        fs::read_dir(game.paths().root().join("heads"))
            .unwrap()
            .count(),
        before_files
    );

    let executed = exchange(&mut game, json!({"command":"act","actions":line}));
    assert_eq!(executed["status"], "ok", "{executed}");
    let current = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    assert_eq!(
        json!(crate::agent_protocol::project_agent_decision_v1(&current).unwrap()),
        preview["result"]["lines"][0]["endpoint"]
    );
}
