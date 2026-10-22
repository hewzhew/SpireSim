use super::*;
use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
use sts_presentation::Locale;

#[test]
fn combat_locales_share_actual_state_commands_and_sources_across_pending_choices() {
    for kind in [
        "combat_multiturn",
        "combat_discovery",
        "combat_exhaust",
        "combat_discard",
    ] {
        let directory = TestDirectory::new();
        let mut game = controller_with_session(&directory.0, kind, &fixture(kind));
        let before = game.head().clone();
        let details = [DecisionDetailV1::Brief, DecisionDetailV1::State];
        let en = game
            .decision_reading_v1(&details, None, Locale::En)
            .unwrap();
        let zh = game
            .decision_reading_v1(&details, None, Locale::ZhCn)
            .unwrap();
        let full = game.decision_reading_v1(&[], None, Locale::ZhCn).unwrap();
        let public = en.state.as_ref().unwrap();
        let preview = read_preview_decision_v1(public, None, Locale::ZhCn).unwrap();
        assert_eq!(en.state, zh.state);
        assert_eq!(en.source, zh.source);
        assert_ne!(en.summary, zh.summary);
        assert!(zh.summary.contains("手牌："), "{kind}: {}", zh.view);
        assert_eq!(zh.summary, full.summary);
        assert_eq!(zh.summary, preview.summary);
        assert!(full.view.contains("完整机制补充（英文）"));
        let commands = |reading: &RunPlayDecisionReadingV1| {
            reading
                .options
                .iter()
                .map(|row| {
                    crate::selector::semantic_action_for_choice_v1(public, &row.choice).unwrap()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(commands(&en), commands(&zh));
        let selections = |reading: &RunPlayDecisionReadingV1| {
            reading
                .selections
                .iter()
                .map(|row| {
                    (
                        row.choice.clone(),
                        row.min,
                        row.max,
                        row.members
                            .iter()
                            .map(|member| member.ordinal)
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(selections(&en), selections(&zh));
        assert_eq!(*game.head(), before);
    }
}

#[test]
fn context_acknowledgements_are_language_scoped_without_changing_choices_or_state() {
    let dir = TestDirectory::new();
    let mut game = controller_with_session(&dir.0, "locale-context", &fixture("reward"));
    let before = game.head().clone();
    let english = game.decision_reading_v1(&[], None, Locale::En).unwrap();
    let chinese = game.decision_reading_v1(&[], None, Locale::ZhCn).unwrap();
    let from_english = game
        .decision_reading_v1(&[], english.context.as_ref(), Locale::ZhCn)
        .unwrap();
    let repeated = game
        .decision_reading_v1(&[], chinese.context.as_ref(), Locale::ZhCn)
        .unwrap();
    assert_eq!(english.source, chinese.source);
    assert_eq!(
        english
            .options
            .iter()
            .map(|row| (&row.choice, &row.candidate))
            .collect::<Vec<_>>(),
        chinese
            .options
            .iter()
            .map(|row| (&row.choice, &row.candidate))
            .collect::<Vec<_>>()
    );
    assert_ne!(english.context, chinese.context);
    assert_eq!(
        chinese.view, from_english.view,
        "an English acknowledgement must not hide Chinese resources"
    );
    assert!(repeated.view.len() < chinese.view.len());
    assert_eq!(*game.head(), before);
}

#[test]
fn position_summaries_share_public_rendering_across_detail_and_binding_modes() {
    for kind in [
        "reward",
        "route",
        "shop_twins",
        "empty_cage",
        "combat_multiturn",
        "combat_discovery",
        "combat_discard",
    ] {
        let dir = TestDirectory::new();
        let mut game = controller_with_session(&dir.0, kind, &fixture(kind));
        let before = game.head().clone();
        let brief = game
            .decision_reading_v1(&[DecisionDetailV1::Brief], None, Locale::En)
            .unwrap();
        let full = game.decision_reading_v1(&[], None, Locale::En).unwrap();
        let public = before
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let preview = read_preview_decision_v1(&public, None, Locale::En).unwrap();
        assert!(!brief.summary.is_empty(), "{kind}");
        assert!(brief.view.starts_with(&brief.summary), "{kind}");
        assert_eq!(brief.summary, full.summary, "{kind}");
        assert_eq!(brief.summary, preview.summary, "{kind}");
        assert!(!brief.summary.contains("./p "), "{kind}");
        if let TaskPublicObservationV1::Combat { observation, .. } = &public.observation {
            assert!(brief.summary.contains(&format!(
                "Energy {} | Block {}",
                observation.turn.energy, observation.player.block
            )));
            assert!(brief.summary.contains("Hand:"));
            assert!(!brief.summary.contains("Draw "));
            assert!(!brief.summary.contains("M0 "));
        }
        assert_eq!(*game.head(), before, "rendering changed {kind}");
    }
}

#[test]
fn escape_summary_does_not_offer_the_inactive_hand() {
    let dir = TestDirectory::new();
    let game = controller_with_session(&dir.0, "escape-summary", &fixture("combat_multiturn"));
    let mut public = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Combat { observation, .. } = &mut public.observation else {
        panic!("combat fixture");
    };
    assert!(!observation.cards.hand.cards.is_empty());
    observation.turn.player_escaping = true;
    let reading = read_preview_decision_v1(&public, None, Locale::En).unwrap();
    assert!(reading.summary.contains("Escape settlement pending"));
    assert!(!reading.summary.contains("Hand:"));
    assert!(!reading.summary.contains("Player turn"));
    assert!(reading.view.starts_with(&reading.summary));
}

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
        let reading = game.decision_reading_v1(&[], None, Locale::En).unwrap();
        let decision = before
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let preview = read_preview_decision_v1(&decision, None, Locale::En).unwrap();
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
            Locale::En,
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
