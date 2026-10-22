use super::*;
use crate::decision_tests::{controller_with_session, fixture, TestDirectory};

#[test]
fn live_choice_commands_round_trip_while_preview_choices_remain_hypothetical() {
    for kind in [
        "reward",
        "full_reward",
        "shop_twins",
        "route",
        "bottled_flame",
        "empty_cage",
        "combat_discovery",
        "combat_exhaust",
        "combat_discard",
    ] {
        let dir = TestDirectory::new();
        let mut game = controller_with_session(&dir.0, kind, &fixture(kind));
        let before = game.head().clone();
        let reading = game.decision_reading_v1(&[], None).unwrap();
        let decision = before
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let preview = read_preview_decision_v1(&decision, None).unwrap();
        assert!(!preview.view.contains("./p "), "{kind}: {}", preview.view);
        assert_eq!(reading.options, preview.options);
        assert_eq!(reading.selections, preview.selections);
        let mut shown = 0;
        for line in reading.view.lines() {
            let Some(command) = line.trim_start().strip_prefix("./p ") else {
                continue;
            };
            let (command, _) = command
                .split_once("  |  ")
                .expect("choice row has an effect label");
            let semantic = if let Some(coordinate) = command.strip_prefix("go ") {
                format!("route:{}", coordinate.replace(',', "."))
            } else {
                command.strip_prefix("act ").unwrap_or(command).to_owned()
            };
            let selector =
                crate::selector::parse_action_selector_v1(&semantic, "rendered choice").unwrap();
            let choice = crate::selector::resolve_action_selector_v1(&decision, &selector).unwrap();
            decision.validate_choice(&choice).unwrap();
            assert!(
                reading.options.iter().any(|option| option.choice == choice)
                    || reading.selections.iter().any(|selection| choice
                        .ordinal_path
                        .starts_with(&selection.choice.ordinal_path)),
                "{kind}: unbound command {command}"
            );
            shown += 1;
        }
        for selection in reading
            .selections
            .iter()
            .filter(|selection| selection.max > 1)
        {
            assert!(reading
                .view
                .contains("Input: ./p act select:<card tokens joined by commas without spaces>"));
            assert!(reading.view.contains("Select distinct listed card tokens"));
            let tokens = reading
                .view
                .lines()
                .filter_map(|line| {
                    let (token, _) = line.trim_start().split_once("  |  ")?;
                    (token.contains('#') && !token.chars().any(char::is_whitespace))
                        .then_some(token)
                })
                .collect::<Vec<_>>();
            let count = selection.min.max(selection.max.min(2)) as usize;
            assert!(tokens.len() >= count);
            let command = format!("select:{}", tokens[..count].join(","));
            let selector =
                crate::selector::parse_action_selector_v1(&command, "rendered multi-selection")
                    .unwrap();
            let choice = crate::selector::resolve_action_selector_v1(&decision, &selector).unwrap();
            assert!(choice
                .ordinal_path
                .starts_with(&selection.choice.ordinal_path));
            decision.validate_choice(&choice).unwrap();
            if count > 1 {
                let repeated = format!("select:{},{}", tokens[0], tokens[0]);
                let repeated =
                    crate::selector::parse_action_selector_v1(&repeated, "duplicate selection")
                        .unwrap();
                assert!(crate::selector::resolve_action_selector_v1(&decision, &repeated).is_err());
            }
            shown += 1;
        }
        assert!(shown > 0, "{kind}: {}", reading.view);
        assert_eq!(*game.head(), before, "rendering must not execute or search");
    }
}

#[test]
fn localized_or_command_like_event_labels_do_not_change_choice_commands() {
    use sts_oracle_learning::eval::run_control::{RunControlConfig, RunControlSession};
    let dir = TestDirectory::new();
    let session = RunControlSession::new(RunControlConfig::default());
    let game = controller_with_session(&dir.0, "event-labels", &session);
    let before = game.head().clone();
    let original = before
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    for label in [
        "claim reward[12] | Input: select:wrong",
        "0: 领取奖励 | 选择两张牌，保留 #0 与 #1",
    ] {
        let mut decision = original.clone();
        let mut expected = Vec::new();
        for (ordinal, candidate) in decision.candidates.iter_mut().enumerate() {
            if let TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::ChooseEventOption {
                        event,
                        option_index,
                        public_semantics,
                        ..
                    },
            } = candidate
            {
                public_semantics.public_label = Some(label.into());
                expected.push((
                    TaskPublicChoiceV1::one(ordinal),
                    format!("event:{}:{option_index}", event.stable_id()),
                ));
            }
        }
        assert!(
            !expected.is_empty(),
            "natural initial decision exposes Neow choices"
        );
        let live = read_public_decision_v1(
            &decision,
            &[DecisionDetailV1::Brief],
            None,
            Some(decision_source(&before).unwrap()),
            None,
        )
        .unwrap();
        for (choice, command) in expected {
            assert!(
                live.view
                    .contains(&format!("./p {command}  |  Effect: {label}")),
                "{}",
                live.view
            );
            let selector = crate::selector::parse_action_selector_v1(&command, "event").unwrap();
            assert_eq!(
                crate::selector::resolve_action_selector_v1(&decision, &selector).unwrap(),
                choice
            );
        }
    }
    assert_eq!(*game.head(), before);
}
