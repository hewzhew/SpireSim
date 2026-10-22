use super::*;
use crate::agent_protocol::{run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1};
use sts_presentation::Locale;

#[test]
fn collector_summon_probe_keeps_the_boss_identity_when_slots_move() {
    use sts_oracle_learning::content::monsters::factory::EncounterId;
    use sts_oracle_learning::sim::combat_start::build_natural_combat_start;
    use sts_oracle_learning::state::core::{ActiveCombat, CombatContext, RoomCombatContext};
    let directory = TestDirectory::new();
    let mut session = fixture("combat_multiturn");
    let (engine, combat) = build_natural_combat_start(
        &mut session.run_state,
        EncounterId::Collector,
        RoomType::MonsterRoomBoss,
    )
    .unwrap();
    session.engine_state = engine.clone();
    session.active_combat = Some(ActiveCombat::new_for_encounter(
        engine,
        combat,
        EncounterId::Collector,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoomBoss,
        }),
    ));
    let mut game = controller_with_session(&directory.0, "collector_summon", &session);
    let head = game.head().clone();
    let comparison = game
        .compare_action_lines_v1(&[vec![crate::selector::RunPlayActionSelectorV1::EndTurn]])
        .unwrap();
    assert_eq!(game.head(), &head);
    let step = &comparison.lines[0].steps[0];
    assert_eq!(step.monster_entities_before.len(), 1);
    assert_eq!(step.monster_entities_after.len(), 3);
    let boss_id = step.monster_entities_before[0].1;
    assert_eq!(
        step.monster_entities_after
            .iter()
            .find(|(_, id)| *id == boss_id)
            .unwrap()
            .0,
        2
    );
    let trace = crate::combat_trace::trace_step_v1(1, "end", step).unwrap();
    assert!(
        trace.delta.monster_changes.is_empty(),
        "summoning leaves the boss HP and block unchanged: {:?}",
        trace.delta.monster_changes
    );
}

#[test]
fn lethal_transition_hp_is_observed_net_change_not_inferred_healing() {
    let directory = TestDirectory::new();
    for (kind, hp_delta, ends_turn) in [
        // Reaper's 1 HP drain and Burning Blood's 6 HP heal share one transition.
        ("combat_reaper_finish", 7, false),
        // Self-damage and battle-end healing also share one positive transition.
        ("combat_hemokinesis_finish", 4, false),
        // A lethal retaliation settles the heal inside End Turn itself.
        ("combat_thorns_finish", 6, true),
    ] {
        let mut game = controller(&directory.0, kind);
        let before = game.head().clone();
        let commands = [
            serde_json::json!({"command":"combat_research","effort":"fast_witness"}),
            serde_json::json!({"command":"combat_review","candidate_refs":["c0"],
                "effort":"fast_witness","include_steps":true}),
        ];
        let input = commands
            .into_iter()
            .enumerate()
            .map(|(index, mut command)| {
                command["schema"] = "sts-run-play-agent-request-v1".into();
                command["request_id"] = format!("hp-{index}").into();
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
        let responses: Vec<serde_json::Value> = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        for response in &responses {
            assert_eq!(response["status"], "ok", "{kind}: {response}");
        }
        assert_eq!(
            game.head(),
            &before,
            "research and review preserve the head"
        );
        let trace = &responses[1]["result"]["trace"];
        assert_eq!(trace["schema"], crate::combat_trace::COMBAT_TRACE_SCHEMA_V3);
        let candidate = &trace["candidates"][0];
        let turns = candidate["turn_reviews"].as_array().unwrap();
        assert_eq!(turns.len(), 1, "{kind}");
        let turn = &turns[0];
        let step_sum: i64 = candidate["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step["delta"]["player_hp"].as_i64().unwrap())
            .sum();
        assert_eq!(step_sum, hp_delta, "{kind}");
        assert_eq!(turn["observed_player_hp_delta"], hp_delta);
        assert_eq!(
            candidate["final_hp"].as_i64().unwrap(),
            turn["start"]["player_hp"].as_i64().unwrap() + hp_delta
        );
        assert!(turn.get("observed_combat_hp_delta").is_none());
        assert!(turn.get("observed_post_combat_hp_delta").is_none());
        if ends_turn {
            assert_eq!(turn["end_turn"]["observed_player_hp_delta"], hp_delta);
        } else {
            assert!(turn["end_turn"].is_null());
        }
        let view = responses[1]["result"]["view"].as_str().unwrap();
        assert_eq!(
            view.matches(&format!("HP {hp_delta:+}")).count(),
            1,
            "{view}"
        );
        assert!(
            !view.contains("After combat"),
            "net change is not a healing attribution"
        );
        assert_eq!(view.contains("Actions: HP"), !ends_turn);
    }
}
