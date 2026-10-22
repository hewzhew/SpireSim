use super::*;
use crate::decision_tests::{controller_with_session, fixture, TestDirectory};

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
