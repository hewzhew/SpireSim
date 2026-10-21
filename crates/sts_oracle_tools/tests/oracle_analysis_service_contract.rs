use std::fs;
use std::io::Cursor;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sts_oracle_runtime::eval::run_control::OracleCombatSearchResumeKindV1;
use sts_oracle_runtime::runtime::branch::{
    call_oracle_analysis_tcp_v1, load_oracle_analysis_workspace_v1,
    load_oracle_run_continuation_v1, serve_oracle_analysis_jsonl_v1, serve_oracle_analysis_tcp_v1,
    OracleAnalysisAdvanceRequestV1, OracleAnalysisAdvanceStatusV1, OracleAnalysisServiceResponseV1,
    OracleAnalysisWorkspaceV1, OracleRunBudget, OracleRunConfig,
};

const SEED: u64 = 20_260_713_006;

#[test]
fn service_keeps_one_session_alive_autosaves_and_survives_bad_commands() {
    let workspace_path = unique_workspace_path();
    let continuation_path = workspace_path.with_extension("continuation.json");
    let workspace = OracleAnalysisWorkspaceV1::new(OracleRunConfig {
        seed: SEED,
        ascension: 0,
        budget: OracleRunBudget::default(),
    })
    .expect("analysis workspace");
    let root = workspace.view().expect("root view");
    let root_id = root.node_id;
    let candidate_id = root
        .choices
        .first()
        .expect("root choice")
        .candidate_id
        .clone();

    let requests = [
        json!({"id": "view", "command": "view"}),
        json!({"id": "bad", "command": "try", "choice_ref": "tampered"}),
        json!({"id": "ping", "command": "ping"}),
        json!({
            "id": "choose_path",
            "command": "choose_path",
            "node": root_id,
            "candidate_ids": [candidate_id]
        }),
        json!({"id": "back", "command": "back"}),
        json!({
            "id": "export_continuation",
            "command": "export_continuation",
            "node": root_id,
            "path": continuation_path,
        }),
        json!({
            "id": "verify_run_witness",
            "command": "verify_run_witness",
            "node": root_id,
        }),
        json!({"id": "save", "command": "save"}),
        json!({"id": "shutdown", "command": "shutdown"}),
    ];
    let input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut output = Vec::new();
    let exit = serve_oracle_analysis_jsonl_v1(
        &workspace_path,
        workspace,
        Cursor::new(input.into_bytes()),
        &mut output,
    )
    .expect("service loop");

    let responses = String::from_utf8(output)
        .expect("utf8 output")
        .lines()
        .map(|line| {
            serde_json::from_str::<OracleAnalysisServiceResponseV1>(line).expect("JSONL response")
        })
        .collect::<Vec<_>>();
    assert_eq!(responses.first().expect("ready").event, "ready");
    assert!(!response(&responses, "bad").ok);
    assert!(response(&responses, "ping").ok, "service continued");
    assert_eq!(response(&responses, "choose_path").revision, 1);
    assert_eq!(response(&responses, "choose_path").saved_revision, 1);
    let choose_path = response(&responses, "choose_path")
        .result
        .as_ref()
        .expect("choose_path result");
    assert_eq!(choose_path["completed"], true);
    assert_eq!(choose_path["applied"].as_array().map(Vec::len), Some(1));
    assert_eq!(response(&responses, "back").revision, 2);
    assert_eq!(response(&responses, "back").saved_revision, 2);
    assert!(response(&responses, "export_continuation").ok);
    let verification = response(&responses, "verify_run_witness")
        .result
        .as_ref()
        .expect("verification result");
    assert_eq!(verification["schema_name"], "ExactOracleRunWitnessReplayV1");
    assert_eq!(verification["report"]["seed"], SEED);
    assert_eq!(response(&responses, "shutdown").event, "shutdown");
    assert_eq!(exit.revision, 2);
    assert_eq!(exit.saved_revision, 2);

    let restored = load_oracle_analysis_workspace_v1(&workspace_path).expect("saved workspace");
    assert_eq!(restored.session.cursor_node_id(), root_id);
    assert_eq!(restored.view().expect("restored view").children.len(), 1);
    let continuation =
        load_oracle_run_continuation_v1(&continuation_path).expect("exported continuation");
    assert_eq!(continuation.seed, SEED);
    assert!(continuation.explorer_frontier.is_none());

    let _ = fs::remove_file(workspace_path);
    let _ = fs::remove_file(continuation_path);
}

#[test]
fn loopback_endpoint_accepts_independent_calls_and_removes_discovery_file_on_shutdown() {
    let workspace_path = unique_workspace_path();
    let endpoint_path = workspace_path.with_extension("endpoint.json");
    let workspace = OracleAnalysisWorkspaceV1::new(OracleRunConfig {
        seed: SEED,
        ascension: 0,
        budget: OracleRunBudget::default(),
    })
    .expect("analysis workspace");
    let server_workspace_path = workspace_path.clone();
    let server_endpoint_path = endpoint_path.clone();
    let server = thread::spawn(move || {
        serve_oracle_analysis_tcp_v1(
            &server_workspace_path,
            workspace,
            "127.0.0.1:0".parse::<SocketAddr>().expect("bind address"),
            &server_endpoint_path,
        )
    });

    for _ in 0..200 {
        if endpoint_path.is_file() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(endpoint_path.is_file(), "endpoint discovery file appeared");

    let ping = call_oracle_analysis_tcp_v1(&endpoint_path, r#"{"id":"ping","command":"ping"}"#)
        .expect("ping resident service");
    assert!(ping.ok);
    assert_eq!(ping.id, Some(json!("ping")));
    let status =
        call_oracle_analysis_tcp_v1(&endpoint_path, r#"{"id":"status","command":"status"}"#)
            .expect("summarize resident service");
    let summary = status.result.expect("status result");
    assert!(summary.get("deck").is_none(), "status stays compact");
    assert!(
        summary["choice_count"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "status retains actionable choices"
    );
    let node = summary["node_id"].as_u64().expect("status node id");
    let explain = call_oracle_analysis_tcp_v1(
        &endpoint_path,
        &format!(r#"{{"id":"explain","command":"explain","node":{node},"owner_rank":0}}"#),
    )
    .expect("explain one resident choice");
    assert!(explain.ok);
    assert!(explain.result.expect("explanation")["label"].is_string());
    let view = call_oracle_analysis_tcp_v1(&endpoint_path, r#"{"id":"view","command":"view"}"#)
        .expect("view resident service");
    assert!(view.ok);
    let shutdown =
        call_oracle_analysis_tcp_v1(&endpoint_path, r#"{"id":"shutdown","command":"shutdown"}"#)
            .expect("shutdown resident service");
    assert_eq!(shutdown.event, "shutdown");

    let exit = server.join().expect("server thread").expect("server exit");
    assert_eq!(exit.revision, 0);
    assert_eq!(exit.saved_revision, 0);
    assert!(!endpoint_path.exists(), "endpoint file removed on exit");
    assert!(workspace_path.is_file(), "workspace saved on shutdown");

    let _ = fs::remove_file(workspace_path);
}

#[test]
fn resident_service_keeps_combat_scratch_alive_across_typed_calls() {
    let workspace_path = unique_workspace_path();
    let endpoint_path = workspace_path.with_extension("endpoint.json");
    let mut workspace = OracleAnalysisWorkspaceV1::new(OracleRunConfig {
        seed: SEED,
        ascension: 0,
        budget: OracleRunBudget::default(),
    })
    .expect("analysis workspace");
    for _ in 0..32 {
        let view = workspace.view().expect("analysis view");
        if view.boundary == sts_oracle_runtime::eval::run_control::OracleRunBoundaryV1::Combat {
            break;
        }
        let choice_ref = view
            .choices
            .first()
            .unwrap_or_else(|| panic!("node {} has no route to combat", view.node_id))
            .choice_ref
            .clone();
        workspace
            .try_choice(&choice_ref)
            .expect("choose toward combat");
    }
    let combat_node = workspace.view().expect("combat view").node_id;

    let server_workspace_path = workspace_path.clone();
    let server_endpoint_path = endpoint_path.clone();
    let server = thread::spawn(move || {
        serve_oracle_analysis_tcp_v1(
            &server_workspace_path,
            workspace,
            "127.0.0.1:0".parse::<SocketAddr>().expect("bind address"),
            &server_endpoint_path,
        )
    });
    for _ in 0..200 {
        if endpoint_path.is_file() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let start_request = json!({
        "id": "scratch-start",
        "command": "combat_scratch_start",
        "node": combat_node,
    })
    .to_string();
    let start = call_oracle_analysis_tcp_v1(&endpoint_path, &start_request)
        .expect("start resident scratch");
    assert!(start.ok);
    let start_result = start.result.as_ref().expect("scratch start result");
    let encoded_start = start_result.to_string();
    assert!(!encoded_start.contains("\"uuid\""));
    assert!(!encoded_start.contains("\"entity_id\""));
    let card = start_result["hand"]
        .as_array()
        .expect("scratch hand")
        .iter()
        .find(|card| {
            card["playable_without_target"].as_bool() == Some(true)
                || card["playable_target_indices"]
                    .as_array()
                    .is_some_and(|targets| !targets.is_empty())
        })
        .expect("at least one locally playable opening card");
    let target_index = card["playable_target_indices"]
        .as_array()
        .and_then(|targets| targets.first())
        .and_then(serde_json::Value::as_u64);
    let play_request = json!({
        "id": "scratch-play",
        "command": "combat_scratch_hand_card",
        "scratch_node": 0,
        "hand_index": card["hand_index"],
        "target_index": target_index,
    })
    .to_string();
    let play = call_oracle_analysis_tcp_v1(&endpoint_path, &play_request)
        .expect("play resident scratch action");
    assert!(play.ok);
    assert_eq!(play.revision, 2);
    assert_eq!(
        play.result.as_ref().expect("scratch play result")["kind"],
        "combat_scratch_decision_delta_v1"
    );
    assert_eq!(
        play.result.as_ref().expect("scratch play result")["base_scratch_node_id"],
        0
    );
    assert_eq!(
        play.result.as_ref().expect("scratch play result")["scratch_node_count"],
        2
    );
    assert!(play.result.as_ref().expect("scratch play result")["service_timing"].is_null());
    assert!(play.timing.is_some());
    let end = call_oracle_analysis_tcp_v1(
        &endpoint_path,
        r#"{"id":"scratch-end","command":"combat_scratch_end"}"#,
    )
    .expect("end turn from resident scratch cursor");
    assert!(end.ok);
    assert_eq!(end.revision, 3);
    assert_eq!(
        end.result.as_ref().expect("scratch end result")["kind"],
        "combat_scratch_decision_delta_v1"
    );
    assert_eq!(
        end.result.as_ref().expect("scratch end result")["scratch_node_count"],
        3
    );
    let tree = call_oracle_analysis_tcp_v1(
        &endpoint_path,
        r#"{"id":"scratch-tree","command":"combat_scratch_tree"}"#,
    )
    .expect("view resident scratch tree");
    assert_eq!(
        tree.result
            .as_ref()
            .and_then(|result| result["nodes"].as_array())
            .map(Vec::len),
        Some(3)
    );
    let back = call_oracle_analysis_tcp_v1(
        &endpoint_path,
        r#"{"id":"scratch-back","command":"combat_scratch_back"}"#,
    )
    .expect("navigate to cached scratch parent");
    assert!(back.ok);
    assert_eq!(
        back.result.as_ref().expect("scratch back result")["kind"],
        "combat_scratch_navigation_v1"
    );
    assert_eq!(
        back.result.as_ref().expect("scratch back result")["source_scratch_node_id"],
        2
    );
    assert_eq!(
        back.result.as_ref().expect("scratch back result")["cursor_scratch_node_id"],
        1
    );
    assert!(back.result.as_ref().expect("scratch back result")["hand"].is_null());
    let focus_full = call_oracle_analysis_tcp_v1(
        &endpoint_path,
        r#"{"id":"scratch-focus-full","command":"combat_scratch_focus","scratch_node":2,"full_observation":true}"#,
    )
    .expect("recover full focused scratch observation");
    assert!(focus_full.ok);
    assert!(focus_full
        .result
        .as_ref()
        .expect("full scratch focus result")["hand"]
        .is_array());
    let full = call_oracle_analysis_tcp_v1(
        &endpoint_path,
        &json!({
            "id": "scratch-full",
            "command": "combat_scratch_hand_card",
            "scratch_node": 0,
            "hand_index": card["hand_index"],
            "target_index": target_index,
            "full_observation": true,
        })
        .to_string(),
    )
    .expect("request full scratch observation fallback");
    assert!(full.ok);
    assert!(full.result.as_ref().expect("full scratch result")["hand"].is_array());
    assert!(full.result.as_ref().expect("full scratch result")["kind"].is_null());
    call_oracle_analysis_tcp_v1(&endpoint_path, r#"{"command":"shutdown"}"#)
        .expect("shutdown resident scratch service");
    server
        .join()
        .expect("server thread")
        .expect("resident server exit");

    let restored = load_oracle_analysis_workspace_v1(&workspace_path).expect("saved scratch");
    assert_eq!(
        restored
            .artifact()
            .expect("restored artifact")
            .session
            .combat_scratch
            .expect("persisted combat scratch")
            .nodes
            .len(),
        3
    );
    let _ = fs::remove_file(workspace_path);
}

#[test]
fn analysis_workspace_either_resumes_or_materializes_a_verified_combat_witness() {
    let budget = OracleRunBudget {
        hallway_generation_work: 1,
        hallway_ms: 100,
        ..Default::default()
    };
    let mut workspace = OracleAnalysisWorkspaceV1::new(OracleRunConfig {
        seed: SEED,
        ascension: 0,
        budget,
    })
    .expect("analysis workspace");

    for _ in 0..32 {
        let view = workspace.view().expect("analysis view");
        if view.boundary == sts_oracle_runtime::eval::run_control::OracleRunBoundaryV1::Combat {
            break;
        }
        let choice_ref = view
            .choices
            .first()
            .unwrap_or_else(|| panic!("node {} has no route to combat", view.node_id))
            .choice_ref
            .clone();
        workspace
            .try_choice(&choice_ref)
            .expect("choose toward combat");
    }
    assert_eq!(
        workspace.view().expect("combat view").boundary,
        sts_oracle_runtime::eval::run_control::OracleRunBoundaryV1::Combat
    );

    let request = OracleAnalysisAdvanceRequestV1 {
        max_quanta: 1,
        quantum_generation_work: 2,
        quantum_ms: Some(100),
        wall_ms: Some(100),
        improve_incumbent: false,
    };
    let (first, _) = workspace.advance(request).expect("first advance");
    let first_progress = first.combat.expect("first progress");
    assert!(
        first_progress
            .generation_work
            .saturating_add(
                u64::try_from(first_progress.remaining_generation_work).expect("remaining nodes fit u64"),
            )
            >= u64::try_from(request.quantum_generation_work).expect("requested nodes fit u64"),
        "the first advance request must enlarge the default combat allowance just like a resumed request"
    );
    assert_eq!(first_progress.restart_count, 0);
    assert_eq!(
        first_progress.resume_kind,
        OracleCombatSearchResumeKindV1::Fresh
    );

    if let OracleAnalysisAdvanceStatusV1::BoundaryReached { child_node_id } = first.status {
        assert_eq!(
            workspace.view().expect("materialized child").node_id,
            child_node_id
        );
        assert!(
            first_progress.incumbent_action_count.is_some(),
            "a combat boundary may be materialized early only from a verified witness"
        );
        return;
    }
    assert_eq!(first.status, OracleAnalysisAdvanceStatusV1::BudgetUnknown);

    let (second, _) = workspace.advance(request).expect("resumed advance");
    let second_progress = second.combat.expect("second progress");
    if let OracleAnalysisAdvanceStatusV1::BoundaryReached { child_node_id } = second.status {
        assert_eq!(
            workspace.view().expect("materialized child").node_id,
            child_node_id
        );
        assert!(second_progress.incumbent_action_count.is_some());
        return;
    }
    assert_eq!(second.status, OracleAnalysisAdvanceStatusV1::BudgetUnknown);
    assert!(
        second_progress.generation_work > first_progress.generation_work,
        "the second advance must continue the resident tactical frontier"
    );
    assert_eq!(second_progress.restart_count, 0);
    assert_eq!(
        second_progress.resume_kind,
        OracleCombatSearchResumeKindV1::SearchResumeExact
    );
}

fn response<'a>(
    responses: &'a [OracleAnalysisServiceResponseV1],
    id: &str,
) -> &'a OracleAnalysisServiceResponseV1 {
    responses
        .iter()
        .find(|response| response.id.as_ref() == Some(&json!(id)))
        .unwrap_or_else(|| panic!("missing response {id}"))
}

fn unique_workspace_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oracle-analysis-service-{}-{nonce}.json",
        std::process::id()
    ))
}
