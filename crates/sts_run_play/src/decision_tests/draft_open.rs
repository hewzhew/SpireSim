use super::*;
use crate::agent_combat_draft::{prepare_commit_attempt, AgentCombatDraftRuntimeV1};
use sts_combat_planner::{materialize_exact_action_line, CombatPlanningCounters};
use sts_oracle_learning::sim::combat::EngineCombatStepper;
use sts_oracle_learning::state::core::ClientInput;
use sts_presentation::Locale;

#[test]
fn candidate_draft_keeps_prefix_and_accepts_an_independently_edited_suffix() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_draft_edit");
    let before = game.head().clone();
    let root = game.current_combat_position_for_lab_v1().unwrap();
    let attack = ClientInput::PlayCard {
        card_index: 0,
        target: Some(root.combat.entities.monsters[0].id),
    };
    let witness = materialize_exact_action_line(
        &EngineCombatStepper,
        &root,
        &[attack.clone(), ClientInput::EndTurn, attack],
        250,
    )
    .unwrap();
    let source =
        prepare_commit_attempt(&game, CombatPlanningCounters::default(), &witness).unwrap();
    let candidate = &source.attempt.candidates[0];
    assert_eq!(candidate.outcome.total_potions_consumed, 0);
    let mut draft = AgentCombatDraftRuntimeV1::default();
    let opened = draft
        .open_candidate(&mut game, candidate, 2, false, sts_presentation::Locale::En)
        .unwrap();
    assert!(opened.reading.view.contains("Turn 2"));
    assert!(opened.reading.view.contains("2 earlier actions retained"));
    assert!(opened.commit_candidate_ref.is_none());
    assert!(
        opened.reading.options.is_empty(),
        "opening does not generate options"
    );
    assert!(
        draft.undo(&mut game, sts_presentation::Locale::En).is_err(),
        "undo cannot cross retained prefix"
    );
    assert_eq!(
        draft
            .status(&mut game, false, sts_presentation::Locale::En)
            .unwrap()
            .frame_ref,
        opened.frame_ref
    );
    let generated = draft
        .advance(&mut game, 1024, 65_536, sts_presentation::Locale::En)
        .unwrap();
    let saved = draft
        .status(&mut game, false, sts_presentation::Locale::En)
        .unwrap();
    assert!(draft
        .open_candidate(&mut game, candidate, 1, false, sts_presentation::Locale::En)
        .unwrap_err()
        .contains("replace"));
    assert!(draft
        .open_candidate(&mut game, candidate, 99, true, sts_presentation::Locale::En)
        .is_err());
    let mut corrupt = candidate.clone();
    corrupt.plan.steps[1].source_public_snapshot_id = "unrelated source".into();
    assert!(draft
        .open_candidate(&mut game, &corrupt, 1, true, sts_presentation::Locale::En)
        .is_err());
    assert_eq!(
        draft
            .status(&mut game, false, sts_presentation::Locale::En)
            .unwrap(),
        saved,
        "failed opens preserve frame and generated work"
    );
    assert_eq!(*game.head(), before);

    let changed = generated
        .reading
        .options
        .iter()
        .find(|option| option.boundary == "terminal_win" && option.successor.potions[0].is_none())
        .expect("can add a potion even though source consumed none");
    let chosen = draft
        .choose(
            &mut game,
            &generated.frame_ref,
            &changed.option_ref,
            sts_presentation::Locale::En,
        )
        .unwrap();
    let edited = chosen.commit_attempt.unwrap();
    let result = &edited.attempt.candidates[0];
    assert_eq!(&result.plan.steps[..2], &candidate.plan.steps[..2]);
    assert!(result.plan.steps.len() > candidate.plan.steps.len());
    assert_eq!(result.outcome.total_potions_consumed, 1);
    assert_eq!(*game.head(), before);
    let restored = draft.undo(&mut game, sts_presentation::Locale::En).unwrap();
    assert_eq!(restored.frame_ref, generated.frame_ref);
    let again = draft
        .choose(
            &mut game,
            &restored.frame_ref,
            &changed.option_ref,
            sts_presentation::Locale::En,
        )
        .unwrap();
    assert_eq!(
        again.commit_attempt.unwrap().attempt.candidates[0].plan,
        result.plan
    );
    let id = crate::driver::combat_research_candidate_id_v1(result).unwrap();
    game.anchor_cached_combat_research_candidate_id_v1(before.head_revision, &edited.attempt, &id)
        .unwrap();
    assert_eq!(
        game.decision_reading_v1(&[], None, Locale::En)
            .unwrap()
            .control,
        DecisionControlV1::Strategic
    );
    assert!(draft
        .open_candidate(&mut game, candidate, 2, true, sts_presentation::Locale::En)
        .is_err());
}

#[test]
fn candidate_draft_explicit_replacement_expires_prior_frame() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let position = game.current_combat_position_for_lab_v1().unwrap();
    let witness = materialize_exact_action_line(
        &EngineCombatStepper,
        &position,
        &[ClientInput::PlayCard {
            card_index: 0,
            target: Some(position.combat.entities.monsters[0].id),
        }],
        250,
    )
    .unwrap();
    let source =
        prepare_commit_attempt(&game, CombatPlanningCounters::default(), &witness).unwrap();
    let mut draft = AgentCombatDraftRuntimeV1::default();
    let old = draft
        .advance(&mut game, 1024, 65_536, sts_presentation::Locale::En)
        .unwrap();
    let opened = draft
        .open_candidate(
            &mut game,
            &source.attempt.candidates[0],
            1,
            true,
            sts_presentation::Locale::En,
        )
        .unwrap();
    assert_ne!(opened.frame_ref.as_deref(), Some(old.frame_ref.as_str()));
    assert!(draft
        .choose(
            &mut game,
            &old.frame_ref,
            &old.reading.options[0].option_ref,
            sts_presentation::Locale::En
        )
        .is_err());
    assert!(draft
        .status(&mut game, false, sts_presentation::Locale::En)
        .unwrap()
        .reading
        .options
        .is_empty());
}

#[test]
fn candidate_draft_protocol_checks_cached_effort_and_replacement_atomically() {
    use crate::agent_protocol::{
        run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1,
    };
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_draft_edit");
    let before = game.head().clone();
    let commands = [
        serde_json::json!({"command":"combat_research","effort":"fast_witness"}),
        serde_json::json!({"command":"combat_draft_open","candidate_ref":"c0","effort":"standard","turn":2}),
        serde_json::json!({"command":"combat_draft_open","candidate_ref":"c99","effort":"fast_witness","turn":2}),
        serde_json::json!({"command":"combat_draft_open","candidate_ref":"c0","effort":"fast_witness","turn":0}),
        serde_json::json!({"command":"combat_draft_open","candidate_ref":"c0","effort":"fast_witness","turn":2}),
        serde_json::json!({"command":"combat_draft_advance","generation_work":1024,"engine_steps":65536}),
        serde_json::json!({"command":"combat_draft_status"}),
        serde_json::json!({"command":"combat_draft_open","candidate_ref":"c0","effort":"fast_witness","turn":1}),
        serde_json::json!({"command":"combat_draft_open","candidate_ref":"c0","effort":"fast_witness","turn":99,"replace":true}),
        serde_json::json!({"command":"combat_draft_status"}),
        serde_json::json!({"command":"combat_draft_open","candidate_ref":"c0","effort":"fast_witness","turn":1,"replace":true}),
    ];
    let input = commands
        .into_iter()
        .enumerate()
        .map(|(index, mut command)| {
            command["schema"] = "sts-run-play-agent-request-v1".into();
            command["request_id"] = format!("open-{index}").into();
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
            if [1, 2, 3, 7, 8].contains(&index) {
                "error"
            } else {
                "ok"
            },
            "{index}: {response}"
        );
    }
    assert_eq!(responses[6]["result"], responses[9]["result"]);
    assert_ne!(
        responses[4]["result"]["frame_ref"],
        responses[10]["result"]["frame_ref"]
    );
    assert!(responses[4]["result"]["options"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(responses[4]["result"].get("commit_candidate_ref").is_none());
    assert_eq!(*game.head(), before);
}
