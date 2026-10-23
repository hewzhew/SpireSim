use super::*;
use crate::agent_protocol::{run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1};
use sts_presentation::Locale;

#[test]
fn work_exhaustion_is_reported_and_identical_requests_explain_cached_results() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_multiturn");
    let before = game.head().clone();
    let input = (0..2).map(|index| format!("{}\n", serde_json::json!({
        "schema":"sts-run-play-agent-request-v1", "request_id":format!("exhaustion-{index}"),
        "command":"combat_research", "effort":"whole_combat", "max_search_ms":2000, "max_generation_work":1
    }))).collect::<String>();
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None)),
        std::io::Cursor::new(input),
        &mut output,
        Locale::En,
    )
    .unwrap();
    let rows = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    for row in &rows {
        assert_eq!(row["status"], "ok", "{row}");
        assert_eq!(row["result"]["search_budget_exhausted"], true);
    }
    assert_eq!(rows[0]["result"]["reused_cached_frontier"], false);
    assert_eq!(rows[1]["result"]["reused_cached_frontier"], true);
    assert!(!rows[0]["result"]["view"]
        .as_str()
        .unwrap()
        .contains("no additional search work"));
    assert!(rows[1]["result"]["view"]
        .as_str()
        .unwrap()
        .contains("no additional search work"));
    assert_eq!(
        rows[0]["result"]["report_id"],
        rows[1]["result"]["report_id"]
    );
    assert_eq!(*game.head(), before);
}

#[test]
fn whole_combat_retained_replay_matches_independent_probe_through_settlement() {
    use sts_oracle_learning::eval::run_control::CombatResearchLaneScheduleV1;
    for kind in [
        "combat_finish",
        "combat_multiturn",
        "combat_reaper_finish",
        "combat_hemokinesis_finish",
        "combat_thorns_finish",
        "combat_heart_finish",
    ] {
        let dir = TestDirectory::new();
        let mut game = if kind == "combat_heart_finish" {
            use sts_oracle_learning::content::monsters::factory::EncounterId;
            use sts_oracle_learning::sim::combat_start::build_natural_combat_start;
            use sts_oracle_learning::state::core::{
                ActiveCombat, CombatContext, RoomCombatContext,
            };
            let mut session = fixture("combat_finish");
            session.run_state.act_num = 4;
            session.run_state.floor_num = 56;
            let (engine, mut combat) = build_natural_combat_start(
                &mut session.run_state,
                EncounterId::TheHeart,
                RoomType::MonsterRoomBoss,
            )
            .unwrap();
            combat.entities.monsters[0].current_hp = 1;
            session.engine_state = engine.clone();
            session.active_combat = Some(ActiveCombat::new_for_encounter(
                engine,
                combat,
                EncounterId::TheHeart,
                CombatContext::Room(RoomCombatContext {
                    room_type: RoomType::MonsterRoomBoss,
                }),
            ));
            controller_with_session(&dir.0, kind, &session)
        } else {
            controller(&dir.0, kind)
        };
        let root = game.head().clone();
        let options = CombatResearchPlanOptionsV1 {
            lane_schedule: CombatResearchLaneScheduleV1::WholeCombat,
            max_generation_work: 256,
            wall_ms: 1000,
            ..CombatResearchPlanOptionsV1::default()
        };
        let research = game
            .combat_research_attempt_with_options_and_progress_v1(Some(options), None, &mut |_| {})
            .unwrap();
        assert!(!research.witness_lines.is_empty(), "{kind}");
        for retained in research.witness_lines {
            let selectors = retained
                .steps
                .iter()
                .map(|step| {
                    let action = crate::selector::semantic_action_for_choice_v1(
                        &step.source_decision,
                        &step.choice,
                    )
                    .unwrap();
                    crate::selector::parse_action_selector_v1(
                        &action,
                        "compare",
                        sts_presentation::Locale::En,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            let independent = game
                .compare_action_lines_v1(&[selectors])
                .unwrap()
                .lines
                .remove(0);
            assert_eq!(retained.steps.len(), independent.steps.len(), "{kind}");
            for (actual, expected) in retained.steps.iter().zip(&independent.steps) {
                assert_eq!(actual.source_decision, expected.source_decision, "{kind}");
                assert_eq!(actual.choice, expected.choice, "{kind}");
                assert_eq!(
                    actual.selected_candidate, expected.selected_candidate,
                    "{kind}"
                );
                assert_eq!(
                    serde_json::to_value(&actual.action_result).unwrap(),
                    serde_json::to_value(&expected.action_result).unwrap(),
                    "{kind}"
                );
                assert_eq!(actual.outcome, expected.outcome, "{kind}");
                assert_eq!(actual.next_decision, expected.next_decision, "{kind}");
            }
        }
        assert_eq!(*game.head(), root, "{kind}");
    }
}

#[test]
fn whole_combat_work_budget_reaches_solver_saved_plans_and_cache_identity() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let before = game.head().clone();
    let commands = [
        serde_json::json!({"command":"combat_research","effort":"whole_combat","max_search_ms":1000,"max_generation_work":64}),
        serde_json::json!({"command":"combat_research","effort":"whole_combat","max_search_ms":1000,"max_generation_work":64}),
        serde_json::json!({"command":"combat_research","effort":"whole_combat","max_search_ms":1000,"max_generation_work":128}),
        serde_json::json!({"command":"combat_research","effort":"whole_combat","max_generation_work":0}),
        serde_json::json!({"command":"combat_research","effort":"whole_combat","max_generation_work":1000001}),
        serde_json::json!({"command":"combat_research","effort":"expanded","max_generation_work":128}),
        serde_json::json!({"command":"combat_plans","diagnostics":true}),
        serde_json::json!({"command":"combat_research","effort":"whole_combat","max_search_ms":1000}),
        serde_json::json!({"command":"combat_research","effort":"whole_combat","max_search_ms":1000,"max_generation_work":30000}),
    ];
    let input = commands
        .into_iter()
        .enumerate()
        .map(|(index, mut command)| {
            command["schema"] = "sts-run-play-agent-request-v1".into();
            command["request_id"] = format!("work-budget-{index}").into();
            format!("{command}\n")
        })
        .collect::<String>();
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None)),
        std::io::Cursor::new(input),
        &mut output,
        Locale::En,
    )
    .unwrap();
    let responses = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    for (index, response) in responses.iter().enumerate() {
        assert_eq!(
            response["status"],
            if [3, 4, 5].contains(&index) {
                "error"
            } else {
                "ok"
            },
            "{index}: {response}"
        );
    }
    for (index, limit) in [(0, 64), (2, 128), (7, 30000)] {
        assert_eq!(responses[index]["result"]["max_generation_work"], limit);
        assert_eq!(responses[index]["result"]["reused_cached_frontier"], false);
        let spent = responses[index]["result"]["lanes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|lane| lane["work_consumed"].as_u64().unwrap())
            .sum::<u64>();
        assert!(
            spent <= limit as u64,
            "solver exceeded requested work: {spent} > {limit}"
        );
    }
    assert_eq!(responses[1]["result"]["reused_cached_frontier"], true);
    assert_eq!(responses[8]["result"]["reused_cached_frontier"], true);
    assert_ne!(
        responses[0]["result"]["report_id"],
        responses[2]["result"]["report_id"]
    );
    assert_eq!(
        responses[6]["result"]["report_id"],
        responses[2]["result"]["report_id"]
    );
    assert!(responses[6]["result"]["view"]
        .as_str()
        .unwrap()
        .contains("Work limit: 128 transitions"));
    let saved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(game.paths().root().join("combat-plans.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["max_generation_work"], 30000);
    assert_eq!(*game.head(), before);
}

#[test]
fn research_budget_is_part_of_the_cache_identity_and_preserves_resident_draft() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let before = game.head().clone();
    let commands = [
        serde_json::json!({"command":"combat_draft_advance","generation_work":1024,"engine_steps":65536}),
        serde_json::json!({"command":"combat_draft_status"}),
        serde_json::json!({"command":"combat_research","effort":"fast_witness","max_search_ms":1000}),
        serde_json::json!({"command":"combat_research","effort":"fast_witness","max_search_ms":1000}),
        serde_json::json!({"command":"combat_research","effort":"fast_witness","max_search_ms":2000}),
        serde_json::json!({"command":"combat_research","max_search_ms":0}),
        serde_json::json!({"command":"combat_research","max_search_ms":20001}),
        serde_json::json!({"command":"combat_draft_status"}),
    ];
    let input = commands
        .into_iter()
        .enumerate()
        .map(|(index, mut command)| {
            command["schema"] = "sts-run-play-agent-request-v1".into();
            command["request_id"] = format!("budget-{index}").into();
            format!("{command}\n")
        })
        .collect::<String>();
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None)),
        std::io::Cursor::new(input),
        &mut output,
        Locale::En,
    )
    .unwrap();
    let responses = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    for (index, response) in responses.iter().enumerate() {
        assert_eq!(
            response["status"],
            if [5, 6].contains(&index) {
                "error"
            } else {
                "ok"
            },
            "{index}: {response}"
        );
    }
    assert_eq!(responses[2]["result"]["reused_cached_frontier"], false);
    assert_eq!(responses[3]["result"]["reused_cached_frontier"], true);
    assert_eq!(responses[4]["result"]["reused_cached_frontier"], false);
    assert_eq!(responses[4]["result"]["max_search_ms"], 2000);
    assert_eq!(responses[4]["result"]["search_budget_exhausted"], false);
    assert_eq!(responses[1]["result"], responses[7]["result"]);
    assert_eq!(*game.head(), before);
}
