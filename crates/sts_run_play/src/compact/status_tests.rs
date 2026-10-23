use super::*;
use crate::decision_tests::{controller_with_session, fixture, TestDirectory};

#[test]
fn ordinary_preview_uses_shared_localized_actions_and_terminal_health() {
    let dir = TestDirectory::new();
    for (kind, action) in [("combat_finish", "play:Strike@0"), ("combat_defeat", "end")] {
        let mut game = controller_with_session(&dir.0, kind, &fixture(kind));
        let head = game.head().clone();
        let fingerprints = game.current_state_fingerprints_v1().unwrap();
        let actions = crate::selector::parse_action_line_v1(action, "locale test").unwrap();
        let comparison = game.compare_action_lines_v1(&[actions]).unwrap();
        let en = render_preview_v1(&comparison, false, Locale::En).unwrap();
        let zh = render_preview_v1(&comparison, false, Locale::ZhCn).unwrap();
        assert!(en.contains("Preview only") && zh.contains("预演结果"));
        assert!(zh.contains("结果："));
        if kind == "combat_finish" {
            assert!(zh.contains("打击 @M0"), "{zh}");
        } else {
            assert!(zh.contains("失败 | 生命 1/75 → 0/75"), "{zh}");
        }
        assert_eq!(game.head(), &head);
        assert_eq!(game.current_state_fingerprints_v1().unwrap(), fingerprints);
    }
}

#[test]
fn detailed_comparisons_localize_steps_successors_draws_and_exhaust_without_mutation() {
    let dir = TestDirectory::new();
    for (kind, action, expected) in [
        ("combat_multiturn", "end", "回合末说明"),
        (
            "combat_pact_draft",
            "play:BurningPact/select:Strike#0",
            "实际新增消耗",
        ),
        ("combat_finish", "play:Strike@0", "打击 @M0"),
        ("combat_defeat", "end", "失败 | 生命 1/75 → 0/75"),
    ] {
        let mut game = controller_with_session(&dir.0, kind, &fixture(kind));
        let head = game.head().clone();
        let fingerprint = game.current_state_fingerprints_v1().unwrap();
        let actions =
            crate::selector::parse_action_line_v1(action, "localized comparison").unwrap();
        let comparison = game.compare_action_lines_v1(&[actions]).unwrap();
        let facts = || {
            comparison
                .lines
                .iter()
                .map(|line| {
                    line.steps.iter().map(|step| serde_json::json!({
            "choice": step.choice, "selected_candidate": step.selected_candidate,
            "source": step.source_decision, "next": step.next_decision, "outcome": step.outcome,
            "action_result": step.action_result,
        })).collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let before = facts();
        let en = render_preview_v1(&comparison, true, Locale::En).unwrap();
        let zh = render_preview_v1(&comparison, true, Locale::ZhCn).unwrap();
        assert!(en.contains("line 1 choices=") && zh.contains("线路 1 选择="));
        assert!(zh.contains(expected), "{kind}: {zh}");
        assert!(!zh.contains("{{") && !en.contains("{{"));
        if kind == "combat_multiturn" {
            assert!(en.contains("observed turn=") && en.contains("actual_draw="));
            assert!(zh.contains("实际变化：回合") && zh.contains("实际抽牌="));
        }
        assert_eq!(facts(), before);
        assert_eq!(game.head(), &head);
        assert_eq!(game.current_state_fingerprints_v1().unwrap(), fingerprint);
    }
}

#[test]
fn permanent_card_changes_do_not_cancel_same_name_card_identities() {
    let dir = TestDirectory::new();
    let game = controller_with_session(&dir.0, "card-identity-change", &fixture("reward"));
    let mut before = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = &mut before.observation else {
        panic!()
    };
    let mut card = observation.cards[0];
    card.card = cards::CardId::Strike;
    observation.cards = vec![card];
    let mut after = before.clone();
    let TaskPublicObservationV1::Strategic { observation } = &mut after.observation else {
        panic!()
    };
    observation.cards[0].card = cards::CardId::StrikeG;
    for locale in [Locale::En, Locale::ZhCn] {
        let change = render_deck_change_v1(&before, &after, locale)
            .expect("same-name identities are distinct");
        assert!(change.contains("[Strike]"), "{change}");
        assert!(change.contains("[StrikeG]"), "{change}");
        assert!(
            change.starts_with('-') && change.contains(", +"),
            "{change}"
        );
    }
}

#[test]
fn permanent_card_changes_track_all_modifiers_and_ignore_page_ordinals() {
    let dir = TestDirectory::new();
    let game = controller_with_session(&dir.0, "permanent-modifiers", &fixture("reward"));
    let mut before = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = &mut before.observation else {
        panic!()
    };
    let mut card = observation.cards[0];
    card.card = cards::CardId::Strike;
    card.upgrades = 0;
    card.misc_value = 0;
    card.base_damage_override = None;
    card.base_block_override = None;
    card.cost_modifier = 0;
    card.bottled_by = None;
    observation.cards = vec![card, card];
    let variants = [
        TaskStrategicCardV1 {
            upgrades: 1,
            ..card
        },
        TaskStrategicCardV1 {
            misc_value: 12,
            ..card
        },
        TaskStrategicCardV1 {
            base_damage_override: Some(23),
            ..card
        },
        TaskStrategicCardV1 {
            base_block_override: Some(17),
            ..card
        },
        TaskStrategicCardV1 {
            cost_modifier: -1,
            ..card
        },
        TaskStrategicCardV1 {
            bottled_by: Some(relics::RelicId::BottledFlame),
            ..card
        },
    ];
    for variant in variants {
        let mut after = before.clone();
        let TaskPublicObservationV1::Strategic { observation } = &mut after.observation else {
            panic!()
        };
        observation.cards = vec![variant, variant];
        for locale in [Locale::En, Locale::ZhCn] {
            let change = render_deck_change_v1(&before, &after, locale).unwrap();
            assert!(
                change.contains("-2× ") && change.contains("+2× "),
                "{change}"
            );
        }
    }
    let mut after = before.clone();
    let TaskPublicObservationV1::Strategic { observation } = &mut after.observation else {
        panic!()
    };
    observation.cards[0].ordinal = 19;
    observation.cards[1].ordinal = 5;
    observation.cards.reverse();
    for locale in [Locale::En, Locale::ZhCn] {
        assert_eq!(render_deck_change_v1(&before, &after, locale), None);
    }
}

#[test]
fn compact_status_layout_keeps_the_same_public_choices_and_does_not_advance() {
    let dir = TestDirectory::new();
    for kind in [
        "reward",
        "reward_late",
        "reward_potions",
        "full_reward",
        "reward_sequence",
        "shop",
        "shop_poor",
        "shop_twins",
        "route",
        "route_question",
        "astrolabe",
        "empty_cage",
        "bottled_flame",
        "combat_finish",
        "combat_discovery",
    ] {
        let game = controller_with_session(&dir.0, kind, &fixture(kind));
        let head = game.head().clone();
        let decision = head
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let mut choices = None;
        for locale in [Locale::En, Locale::ZhCn] {
            let standard = crate::decision::read_decision(&head, &[], None, locale).unwrap();
            let compact = crate::decision::read_compact_decision_v1(
                &decision,
                standard.source.clone(),
                false,
                None,
                locale,
            )
            .unwrap();
            assert_eq!(compact.options, standard.options, "{kind} {locale}");
            assert_eq!(compact.selections, standard.selections, "{kind} {locale}");
            assert_eq!(compact.source, standard.source);
            assert_eq!(compact.summary, standard.summary);
            let identities = compact
                .options
                .iter()
                .map(|option| option.choice.clone())
                .collect::<Vec<_>>();
            if let Some(previous) = &choices {
                assert_eq!(previous, &identities);
            } else {
                choices = Some(identities);
            }
            let status = render_status_v1(&head, locale).unwrap();
            assert!(!status.is_empty());
            if locale == Locale::ZhCn {
                assert!(!status.contains("Deck: "), "{kind}: {status}");
                assert!(!status.contains("Relics: "), "{kind}: {status}");
            }
            assert_eq!(game.head(), &head);
        }
    }
}

#[test]
fn localized_shop_status_keeps_exact_offer_commands_prices_and_blocks() {
    let dir = TestDirectory::new();
    let mut session = fixture("shop");
    session.run_state.add_card_to_deck(cards::CardId::Doubt);
    let game = controller_with_session(&dir.0, "localized-shop", &session);
    let head = game.head().clone();
    let en = render_status_v1(&head, Locale::En).unwrap();
    let zh = render_status_v1(&head, Locale::ZhCn).unwrap();
    let commands = |value: &str| {
        value
            .lines()
            .filter_map(|line| {
                line.split_once("  |  ")
                    .map(|(command, _)| command.trim().to_owned())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(commands(&en), commands(&zh));
    assert!(zh.contains("出售的卡牌"));
    assert!(zh.contains("耸肩无视"));
    assert!(zh.contains("力量药水") || zh.contains("能力药水"), "{zh}");
    assert!(zh.contains("药水槽已满"), "{zh}");
    assert!(zh.contains("remove:Doubt+0#0  |  疑虑"), "{zh}");
    assert!(zh.contains("离开：leave"));
    assert_eq!(game.head(), &head);
}

#[test]
fn compact_combat_status_lists_each_actual_hand_card_on_its_own_line() {
    let dir = TestDirectory::new();
    let game = controller_with_session(&dir.0, "combat_finish", &fixture("combat_finish"));
    let head = game.head().clone();
    let TaskPublicObservationV1::Combat { observation, .. } =
        &head.current_decision.as_ref().unwrap().observation
    else {
        panic!()
    };
    for locale in [Locale::En, Locale::ZhCn] {
        let status = render_status_v1(&head, locale).unwrap();
        let heading = compact_text(locale, "status.hand", &[]);
        assert!(status.contains(&format!("{heading}\n  ")));
        for card in &observation.cards.hand.cards {
            let start = format!(
                "  {}[",
                locale.card_name(card.card_id.stable_id(), card.upgrades)
            );
            assert!(
                status.lines().any(|line| line.starts_with(&start)),
                "{status}"
            );
        }
        assert!(!status.contains("Damage: per-target hit"));
        assert_eq!(game.head(), &head);
    }
}
