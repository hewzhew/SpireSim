use super::*;
use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
use sts_presentation::Locale;

#[test]
fn neow_original_binding_follows_rich_and_mini_offers_without_changing_execution() {
    use sts_oracle_learning::content::events::neow::{setup_neow_event, NeowEligibilityV1};
    use sts_oracle_learning::eval::run_control::RunControlSessionCheckpointV1;
    use sts_oracle_learning::state::core::EngineState;
    for eligibility in [NeowEligibilityV1::Mini, NeowEligibilityV1::Rich] {
        let mut checkpoints = Vec::new();
        for locale in [Locale::En, Locale::ZhCn] {
            let directory = TestDirectory::new();
            let mut session = fixture("reward");
            session.engine_state = EngineState::EventRoom;
            setup_neow_event(&mut session.run_state, eligibility);
            let mut game = controller_with_session(&directory.0, "neow-original-offers", &session);
            let initial = game
                .decision_reading_v1(&[DecisionDetailV1::Options], None, locale)
                .unwrap();
            if initial.options.iter().any(|option| {
                matches!(
                    &option.candidate,
                    TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::ChooseEventOption { screen: 0, .. }
                    }
                )
            }) {
                game.choose_bound_v1(&crate::decision::RunPlayBoundChoiceV1 {
                    source: initial.source.unwrap(),
                    choice: initial.options[0].choice.clone(),
                })
                .unwrap();
            }
            let before = game.head().clone();
            let reading = game
                .decision_reading_v1(&[DecisionDetailV1::Options], None, locale)
                .unwrap();
            let other = game
                .decision_reading_v1(
                    &[DecisionDetailV1::Options],
                    None,
                    if locale == Locale::En {
                        Locale::ZhCn
                    } else {
                        Locale::En
                    },
                )
                .unwrap();
            assert_eq!(
                reading.options.len(),
                if eligibility == NeowEligibilityV1::Mini {
                    2
                } else {
                    4
                }
            );
            assert_eq!(reading.source, other.source);
            assert_eq!(reading.state, other.state);
            for (option, other) in reading.options.iter().zip(&other.options) {
                assert_eq!(option.candidate, other.candidate);
                assert_eq!(option.choice, other.choice);
                assert_ne!(option.label, other.label);
                assert!(
                    option.label.starts_with('[') && option.label.ends_with(']'),
                    "{}",
                    option.label
                );
                let TaskPublicCandidateV1::Strategic {
                    action:
                        TaskStrategicActionV1::ChooseEventOption {
                            public_semantics, ..
                        },
                } = &option.candidate
                else {
                    unreachable!()
                };
                assert_ne!(
                    option.label,
                    public_semantics.public_label.as_deref().unwrap()
                );
            }
            assert_eq!(*game.head(), before);
            game.choose_bound_v1(&crate::decision::RunPlayBoundChoiceV1 {
                source: reading.source.unwrap(),
                choice: reading.options[0].choice.clone(),
            })
            .unwrap();
            let actual = game
                .current_external_exact_checkpoint_v1()
                .unwrap()
                .into_session()
                .unwrap();
            checkpoints.push(
                serde_json::to_value(RunControlSessionCheckpointV1::from_session(&actual)).unwrap(),
            );
        }
        assert_eq!(
            checkpoints[0], checkpoints[1],
            "{eligibility:?}: translated Neow offers must preserve effects and RNG"
        );
    }
}

#[test]
fn original_event_button_bindings_preserve_public_actions_sources_and_state() {
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventState},
    };
    let events: &[(EventId, &[usize])] = &[
        (EventId::FaceTrader, &[0, 1, 2]),
        (EventId::BigFish, &[0, 1]),
        (EventId::Lab, &[0]),
        (EventId::LivingWall, &[0, 1]),
        (EventId::ShiningLight, &[0, 1]),
        (EventId::WorldOfGoop, &[0, 1]),
        (EventId::Ssssserpent, &[0, 1, 2]),
        (EventId::GoldenShrine, &[0, 1]),
        (EventId::AccursedBlacksmith, &[0, 1]),
        (EventId::Duplicator, &[0, 1]),
        (EventId::UpgradeShrine, &[0, 1]),
        (EventId::Transmorgrifier, &[0, 1]),
        (EventId::BackTotheBasics, &[0, 1]),
        (EventId::Mushrooms, &[0, 1, 2]),
        (EventId::Cleric, &[0, 1]),
        (EventId::Addict, &[0, 1]),
        (EventId::Beggar, &[0, 1, 2]),
        (EventId::Ghosts, &[0, 1, 2]),
        (EventId::Nest, &[0, 1, 2]),
        (EventId::MindBloom, &[0, 1]),
        (EventId::MoaiHead, &[0, 1]),
        (EventId::WindingHalls, &[0, 1, 2]),
        (EventId::WomanInBlue, &[0, 1]),
        (EventId::SecretPortal, &[0, 1, 2]),
        (EventId::Purifier, &[0, 1]),
        (EventId::FountainOfCurseCleansing, &[0, 1]),
        (EventId::ForgottenAltar, &[0, 1]),
        (EventId::TombRedMask, &[0, 1]),
        (EventId::GoldenWing, &[0, 1, 2]),
        (EventId::MysteriousSphere, &[0, 1, 2]),
        (EventId::GoldenIdol, &[0, 1, 2]),
        (EventId::DrugDealer, &[0, 1]),
        (EventId::Mausoleum, &[0, 1]),
        (EventId::MaskedBandits, &[0, 1, 2, 3]),
        (EventId::TheLibrary, &[0, 1, 2]),
        (EventId::Vampires, &[0, 1, 2]),
        (EventId::BonfireElementals, &[0, 1, 2, 3]),
        (EventId::Colosseum, &[0, 1, 2, 3]),
        (EventId::SpireHeart, &[0, 1, 2, 3, 4]),
        (EventId::CursedTome, &[0, 1, 2, 3, 4, 5]),
        (EventId::DeadAdventurer, &[0, 1, 2]),
        (EventId::ScrapOoze, &[0, 1]),
        (EventId::Designer, &[0, 1, 2]),
        (EventId::Falling, &[0, 1, 2]),
        (EventId::GremlinWheelGame, &[0, 1, 2, 3]),
        (EventId::KnowingSkull, &[0, 1, 2]),
        (EventId::MatchAndKeep, &[0, 1, 2, 3]),
        (EventId::Nloth, &[0, 1]),
        (EventId::NoteForYourself, &[0, 1, 2]),
        (EventId::SensoryStone, &[0, 1, 2, 3]),
        (EventId::TheJoust, &[0, 1, 2, 3, 4, 5]),
        (EventId::WeMeetAgain, &[0, 1]),
    ];
    for &(event, screens) in events {
        for &screen in screens {
            for ascension in [14, 15] {
                let directory = TestDirectory::new();
                let mut session = fixture("reward");
                session.engine_state = EngineState::EventRoom;
                session.run_state.ascension_level = ascension;
                session.run_state.max_hp = 83;
                session.run_state.current_hp = 17;
                session.run_state.gold = 250;
                let mut state = EventState::new(event);
                state.current_screen = screen;
                state.internal_state = if event == EventId::FaceTrader && screen == 2 {
                    1
                } else {
                    23
                };
                session.run_state.event_state = Some(state);
                if event == EventId::TheLibrary && screen == 1 {
                    session
                        .run_state
                        .event_state
                        .as_mut()
                        .unwrap()
                        .current_screen = 0;
                    sts_oracle_learning::content::events::the_library::handle_choice(
                        &mut session.engine_state,
                        &mut session.run_state,
                        0,
                    );
                }
                if event == EventId::DeadAdventurer {
                    let state = sts_oracle_learning::content::events::dead_adventurer::init_dead_adventurer_state(&mut session.run_state);
                    session
                        .run_state
                        .event_state
                        .as_mut()
                        .unwrap()
                        .internal_state = state;
                }
                if event == EventId::BonfireElementals {
                    session
                        .run_state
                        .event_state
                        .as_mut()
                        .unwrap()
                        .internal_state = 2;
                }
                if event == EventId::ScrapOoze {
                    session
                        .run_state
                        .event_state
                        .as_mut()
                        .unwrap()
                        .internal_state = 0;
                }
                if event == EventId::GremlinWheelGame {
                    session
                        .run_state
                        .event_state
                        .as_mut()
                        .unwrap()
                        .internal_state = 0;
                }
                if event == EventId::Falling {
                    let state = sts_oracle_learning::content::events::falling::init_falling_state(
                        &mut session.run_state,
                    );
                    session
                        .run_state
                        .event_state
                        .as_mut()
                        .unwrap()
                        .internal_state = state;
                }
                if event == EventId::Nloth {
                    session.run_state.relics.push(
                        sts_oracle_learning::content::relics::RelicState::new(
                            sts_oracle_learning::content::relics::RelicId::Anchor,
                        ),
                    );
                    let state = sts_oracle_learning::content::events::nloth::init_nloth_state(
                        &mut session.run_state,
                    );
                    session
                        .run_state
                        .event_state
                        .as_mut()
                        .unwrap()
                        .internal_state = state;
                }
                if matches!(event, EventId::MatchAndKeep | EventId::WeMeetAgain) {
                    let mut state = session.run_state.event_state.take().unwrap();
                    if event == EventId::MatchAndKeep {
                        sts_oracle_learning::content::events::match_and_keep::init_match_game_board(
                            &mut session.run_state,
                            &mut state,
                        );
                    } else {
                        session.run_state.potions[0] =
                            Some(sts_oracle_learning::content::potions::Potion::new(
                                sts_oracle_learning::content::potions::PotionId::FirePotion,
                                42,
                            ));
                        sts_oracle_learning::content::events::we_meet_again::init_we_meet_again_event_state(&mut session.run_state, &mut state);
                    }
                    session.run_state.event_state = Some(state);
                }
                let mut game = controller_with_session(&directory.0, "original-buttons", &session);
                let before = game.head().clone();
                let en = game
                    .decision_reading_v1(
                        &[DecisionDetailV1::Options, DecisionDetailV1::State],
                        None,
                        Locale::En,
                    )
                    .unwrap();
                let zh = game
                    .decision_reading_v1(
                        &[DecisionDetailV1::Options, DecisionDetailV1::State],
                        None,
                        Locale::ZhCn,
                    )
                    .unwrap();
                assert_eq!(en.source, zh.source);
                assert_eq!(en.state, zh.state);
                assert_eq!(en.options.len(), zh.options.len());
                assert!(!zh.options.is_empty(), "{event:?} screen {screen}");
                for (en_option, zh_option) in en.options.iter().zip(&zh.options) {
                    assert_eq!(en_option.candidate, zh_option.candidate);
                    assert_eq!(en_option.choice, zh_option.choice);
                    assert_ne!(
                        en_option.label, zh_option.label,
                        "{event:?} screen {screen}: {}",
                        zh_option.label
                    );
                    assert!(
                        zh_option.label == "……"
                            || zh_option
                                .label
                                .chars()
                                .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch)),
                        "{}",
                        zh_option.label
                    );
                    assert!(!zh_option.label.contains("#g") && !zh_option.label.contains("#r"));
                }
                assert_eq!(
                    *game.head(),
                    before,
                    "{event:?} screen {screen}: language must not change state or RNG"
                );
                if event == EventId::FaceTrader && screen == 1 {
                    let gold = if ascension < 15 { 75 } else { 50 };
                    assert!(
                        zh.options[0].label.contains(&format!("{gold} 金币")),
                        "{}",
                        zh.options[0].label
                    );
                    assert!(zh.options[0].label.contains("失去8"));
                }
                if event == EventId::BigFish && screen == 0 {
                    assert!(
                        zh.options[0].label.contains("23"),
                        "frozen heal must not be recomputed from Max HP"
                    );
                }
                if event == EventId::ShiningLight && screen == 0 {
                    let damage = if ascension < 15 { 17 } else { 25 };
                    assert!(zh.options[0].label.contains(&damage.to_string()));
                }
                if event == EventId::WorldOfGoop && screen == 0 {
                    assert!(zh.options[1].label.contains("23"));
                }
                if event == EventId::Mausoleum && screen == 0 {
                    let chance = if ascension < 15 { 50 } else { 100 };
                    assert!(zh.options[0].label.contains(&format!("{chance}%")));
                }
                if event == EventId::DeadAdventurer && screen == 0 {
                    let chance = if ascension < 15 { 25 } else { 35 };
                    assert!(zh.options[0].label.contains(&format!("{chance}%")));
                }
                if event == EventId::ScrapOoze && screen == 0 {
                    let damage = if ascension < 15 { 3 } else { 5 };
                    assert!(zh.options[0].label.contains("25%"));
                    assert!(zh.options[0].label.contains(&damage.to_string()));
                }
                if event == EventId::Colosseum && screen == 1 {
                    assert!(en.options[0]
                        .label
                        .contains("No item rewards; after victory return to this event"));
                    assert!(zh.options[0].label.contains("ColosseumSlavers"));
                    assert!(zh.options[0].label.contains("获胜后"));
                }
            }
        }
    }
}

#[test]
fn original_event_variants_keep_costs_prerequisites_and_late_floor_outcomes() {
    use sts_oracle_learning::content::{
        cards::CardId,
        relics::{RelicId, RelicState},
    };
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventState},
    };
    for (event, variant, expected) in [
        (EventId::Cleric, "no_removable", "没有可移除的牌"),
        (EventId::MindBloom, "late_floor", "我无伤痛"),
        (EventId::TombRedMask, "mask", "222"),
        (EventId::MoaiHead, "idol", "333"),
        (EventId::ForgottenAltar, "idol", "金神像"),
        (EventId::WomanInBlue, "poor", "5 点伤害"),
        (EventId::Cleric, "poor", "离开"),
        (EventId::Vampires, "vial", "小血瓶"),
        (EventId::ScrapOoze, "ooze_65", "65%"),
        (
            EventId::Designer,
            "designer_random_transform",
            "变化 2 张牌",
        ),
        (
            EventId::Designer,
            "designer_upgrade_transform",
            "升级一张牌",
        ),
        (EventId::Designer, "designer_random_remove", "移除一张牌"),
        (EventId::Designer, "designer_upgrade_remove", "移除一张牌"),
        (EventId::Falling, "falling_empty", "无事发生"),
    ] {
        let directory = TestDirectory::new();
        let mut session = fixture("reward");
        session.engine_state = EngineState::EventRoom;
        session.run_state.current_hp = 17;
        session.run_state.max_hp = 83;
        session.run_state.gold = 250;
        session.run_state.ascension_level = 20;
        session.run_state.event_state = Some(EventState::new(event));
        match variant {
            "no_removable" => session
                .run_state
                .master_deck
                .retain(|card| card.id == CardId::AscendersBane),
            "late_floor" => session.run_state.floor_num = 44,
            "mask" => session
                .run_state
                .relics
                .push(RelicState::new(RelicId::RedMask)),
            "idol" => session
                .run_state
                .relics
                .push(RelicState::new(RelicId::GoldenIdol)),
            "poor" => session.run_state.gold = 0,
            "vial" => session
                .run_state
                .relics
                .push(RelicState::new(RelicId::BloodVial)),
            "ooze_65" => {
                session
                    .run_state
                    .event_state
                    .as_mut()
                    .unwrap()
                    .internal_state = (7 << 16) | 65
            }
            "designer_random_transform"
            | "designer_upgrade_transform"
            | "designer_random_remove"
            | "designer_upgrade_remove" => {
                let state = session.run_state.event_state.as_mut().unwrap();
                state.current_screen = 1;
                state.internal_state = match variant {
                    "designer_random_transform" => 0,
                    "designer_upgrade_transform" => 1,
                    "designer_random_remove" => 2,
                    _ => 3,
                };
            }
            "falling_empty" => {
                session
                    .run_state
                    .master_deck
                    .retain(|card| card.id == CardId::AscendersBane);
                let state = sts_oracle_learning::content::events::falling::init_falling_state(
                    &mut session.run_state,
                );
                let event = session.run_state.event_state.as_mut().unwrap();
                event.current_screen = 1;
                event.internal_state = state;
            }
            _ => unreachable!(),
        }
        let mut game = controller_with_session(&directory.0, "event-variant-locale", &session);
        let before = game.head().clone();
        let en = game
            .decision_reading_v1(&[DecisionDetailV1::Options], None, Locale::En)
            .unwrap();
        let zh = game
            .decision_reading_v1(&[DecisionDetailV1::Options], None, Locale::ZhCn)
            .unwrap();
        assert_eq!(en.source, zh.source);
        assert_eq!(en.options.len(), zh.options.len());
        assert!(
            zh.options
                .iter()
                .any(|option| option.label.contains(expected)),
            "{event:?}/{variant}: {:?}",
            zh.options
        );
        for (english, chinese) in en.options.iter().zip(&zh.options) {
            assert_eq!(english.candidate, chinese.candidate);
            assert_eq!(english.choice, chinese.choice);
            assert_ne!(english.label, chinese.label, "{event:?}/{variant}");
        }
        if variant == "mask" {
            assert!(
                zh.options[1].label.contains("离开"),
                "the second original option is Leave when the mask is held"
            );
        }
        if variant == "vial" {
            assert_eq!(zh.options.len(), 3);
            assert!(zh.options[1].label.contains("小血瓶"));
            assert!(zh.options[2].label.contains("拒绝"));
        }
        if variant == "poor" {
            // Original WomanInBlue leaves all potion purchases enabled below price.
            let (count, position, ordinal) = if event == EventId::WomanInBlue {
                (4, 3, 3)
            } else {
                (1, 0, 2)
            };
            assert_eq!(zh.options.len(), count);
            assert!(matches!(
                &zh.options[position].candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::ChooseEventOption {
                        option_index,
                        ..
                    }
                } if *option_index == ordinal
            ));
        }
        assert_eq!(*game.head(), before);
    }
}

#[test]
fn visible_event_results_localize_without_reapplying_settled_rewards() {
    use sts_oracle_learning::content::cards::CardId;
    use sts_oracle_learning::eval::run_control::RunControlSessionCheckpointV1;
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventOptionPublicDetail, EventState},
    };
    for (event, screen, results) in [
        (EventId::GremlinWheelGame, 1, 0..6),
        (EventId::TheJoust, 4, 0..4),
    ] {
        for result in results {
            let mut checkpoints = Vec::new();
            for locale in [Locale::En, Locale::ZhCn] {
                let directory = TestDirectory::new();
                let mut session = fixture("reward");
                session.engine_state = EngineState::EventRoom;
                session.run_state.gold = 400;
                session.run_state.current_hp = 17;
                session.run_state.max_hp = 83;
                let mut state = EventState::new(event);
                state.current_screen = screen;
                state.internal_state = result;
                session.run_state.event_state = Some(state);
                let mut game =
                    controller_with_session(&directory.0, "visible-result-locale", &session);
                let reading = game
                    .decision_reading_v1(&[DecisionDetailV1::Options], None, locale)
                    .unwrap();
                let option = &reading.options[0];
                let TaskPublicCandidateV1::Strategic {
                    action:
                        TaskStrategicActionV1::ChooseEventOption {
                            public_semantics, ..
                        },
                } = &option.candidate
                else {
                    panic!("event choice");
                };
                assert!(matches!(
                    public_semantics.public_detail,
                    Some(
                        EventOptionPublicDetail::WheelReward { .. }
                            | EventOptionPublicDetail::JoustResult { .. }
                    )
                ));
                assert_eq!(
                    public_semantics.gold_delta_min, 0,
                    "these results have already paid gold"
                );
                if event == EventId::GremlinWheelGame && result == 0 {
                    assert!(option.label.contains("100"));
                }
                if event == EventId::GremlinWheelGame && result == 5 {
                    assert!(option.label.contains("12"));
                }
                game.choose_bound_v1(&crate::decision::RunPlayBoundChoiceV1 {
                    source: reading.source.unwrap(),
                    choice: option.choice.clone(),
                })
                .unwrap();
                let actual = game
                    .current_external_exact_checkpoint_v1()
                    .unwrap()
                    .into_session()
                    .unwrap();
                assert_eq!(actual.run_state.gold, 400);
                if event == EventId::GremlinWheelGame {
                    match result {
                        0 => assert_eq!(actual.run_state.current_hp, 17),
                        1 => assert!(matches!(actual.engine_state, EngineState::RewardScreen(_))),
                        2 => assert_eq!(actual.run_state.current_hp, 83),
                        3 => assert!(actual
                            .run_state
                            .master_deck
                            .iter()
                            .any(|card| card.id == CardId::Decay)),
                        4 => assert!(matches!(
                            actual.engine_state,
                            EngineState::RunPendingChoice(_)
                        )),
                        5 => assert_eq!(actual.run_state.current_hp, 5),
                        _ => unreachable!(),
                    }
                }
                checkpoints.push(
                    serde_json::to_value(RunControlSessionCheckpointV1::from_session(&actual))
                        .unwrap(),
                );
            }
            assert_eq!(checkpoints[0], checkpoints[1], "{event:?}/{result}");
        }
    }
}

#[test]
fn unrevealed_event_results_do_not_enter_public_button_facts() {
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventState},
    };
    for (event, screen) in [(EventId::GremlinWheelGame, 3), (EventId::TheJoust, 3)] {
        let mut candidates = Vec::new();
        for result in [0, 2] {
            let directory = TestDirectory::new();
            let mut session = fixture("reward");
            session.engine_state = EngineState::EventRoom;
            let mut state = EventState::new(event);
            state.current_screen = screen;
            state.internal_state = result;
            session.run_state.event_state = Some(state);
            let mut game = controller_with_session(&directory.0, "hidden-event-result", &session);
            let reading = game
                .decision_reading_v1(&[DecisionDetailV1::Options], None, Locale::ZhCn)
                .unwrap();
            let candidate = reading.options[0].candidate.clone();
            let TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::ChooseEventOption {
                        public_semantics, ..
                    },
            } = &candidate
            else {
                panic!("event choice");
            };
            assert_eq!(public_semantics.public_detail, None);
            candidates.push((candidate, reading.options[0].label.clone()));
        }
        assert_eq!(
            candidates[0], candidates[1],
            "{event:?}: hidden result must not affect a public button"
        );
    }
}

#[test]
fn original_card_and_relic_trade_buttons_preserve_upgrades_and_settlement() {
    use sts_oracle_learning::content::{
        cards::CardId,
        relics::{RelicId, RelicState},
    };
    use sts_oracle_learning::eval::run_control::RunControlSessionCheckpointV1;
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventState},
    };
    for event in [EventId::TheLibrary, EventId::Vampires] {
        let mut checkpoints = Vec::new();
        for locale in [Locale::En, Locale::ZhCn] {
            let directory = TestDirectory::new();
            let mut session = fixture("reward");
            session.engine_state = EngineState::EventRoom;
            session.run_state.event_state = Some(EventState::new(event));
            session
                .run_state
                .relics
                .push(RelicState::new(if event == EventId::TheLibrary {
                    RelicId::ToxicEgg
                } else {
                    RelicId::BloodVial
                }));
            let hp = session.run_state.current_hp;
            let max_hp = session.run_state.max_hp;
            let count = session.run_state.master_deck.len();
            if event == EventId::TheLibrary {
                sts_oracle_learning::content::events::the_library::handle_choice(
                    &mut session.engine_state,
                    &mut session.run_state,
                    0,
                );
            }
            let expected_card = session
                .run_state
                .event_state
                .as_ref()
                .unwrap()
                .card_payload
                .iter()
                .position(|card| card.upgrades > 0);
            let option_index = if event == EventId::TheLibrary {
                expected_card.expect("Toxic Egg must upgrade a generated skill")
            } else {
                1
            };
            let offered = session
                .run_state
                .event_state
                .as_ref()
                .unwrap()
                .card_payload
                .get(option_index)
                .cloned();
            let mut game = controller_with_session(&directory.0, "translated-card-trade", &session);
            let reading = game
                .decision_reading_v1(&[DecisionDetailV1::Options], None, locale)
                .unwrap();
            if event == EventId::TheLibrary {
                assert_eq!(reading.options.len(), 20);
            }
            let option = reading.options.iter().find(|option| matches!(&option.candidate,
                TaskPublicCandidateV1::Strategic { action: TaskStrategicActionV1::ChooseEventOption { option_index: index, .. } } if *index == option_index)).unwrap();
            if let Some(card) = &offered {
                assert!(option
                    .label
                    .contains(&locale.card_name(card.card_id.stable_id(), card.upgrades)));
                assert!(
                    option.label.contains(if locale == Locale::En {
                        "uncommon"
                    } else {
                        "罕见"
                    }) || option.label.contains(if locale == Locale::En {
                        "common"
                    } else {
                        "普通"
                    }) || option.label.contains(if locale == Locale::En {
                        "rare"
                    } else {
                        "稀有"
                    })
                );
            } else {
                assert!(option.label.contains(locale.entity("relic", "BloodVial")));
            }
            game.choose_bound_v1(&crate::decision::RunPlayBoundChoiceV1 {
                source: reading.source.unwrap(),
                choice: option.choice.clone(),
            })
            .unwrap();
            let actual = game
                .current_external_exact_checkpoint_v1()
                .unwrap()
                .into_session()
                .unwrap();
            assert_eq!(
                (actual.run_state.current_hp, actual.run_state.max_hp),
                (hp, max_hp)
            );
            if let Some(card) = offered {
                assert_eq!(actual.run_state.master_deck.len(), count + 1);
                assert!(actual
                    .run_state
                    .master_deck
                    .iter()
                    .any(|held| held.id == card.card_id && held.upgrades == card.upgrades));
            } else {
                assert!(!actual
                    .run_state
                    .relics
                    .iter()
                    .any(|held| held.id == RelicId::BloodVial));
                assert_eq!(
                    actual
                        .run_state
                        .master_deck
                        .iter()
                        .filter(|card| card.id == CardId::Bite)
                        .count(),
                    5
                );
                assert!(!actual
                    .run_state
                    .master_deck
                    .iter()
                    .any(|card| card.id == CardId::Strike));
            }
            checkpoints.push(
                serde_json::to_value(RunControlSessionCheckpointV1::from_session(&actual)).unwrap(),
            );
        }
        assert_eq!(
            checkpoints[0], checkpoints[1],
            "{event:?}: translated choices must settle identically"
        );
    }
}

#[test]
fn choices_from_original_translated_buttons_execute_the_same_actions_and_rng() {
    use sts_oracle_learning::content::{cards::CardId, relics::RelicId};
    use sts_oracle_learning::eval::run_control::RunControlSessionCheckpointV1;
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventState},
    };
    for (event, screen, option_index) in [
        (EventId::FaceTrader, 1, 0),
        (EventId::BigFish, 0, 0),
        (EventId::WorldOfGoop, 0, 1),
        (EventId::LivingWall, 0, 0),
        (EventId::Mushrooms, 0, 1),
        (EventId::Lab, 0, 0),
        (EventId::Cleric, 0, 0),
        (EventId::Addict, 0, 0),
        (EventId::Beggar, 0, 0),
        (EventId::Ghosts, 0, 0),
        (EventId::Nest, 1, 1),
        (EventId::MindBloom, 0, 1),
        (EventId::MoaiHead, 0, 0),
        (EventId::WindingHalls, 1, 0),
        (EventId::WomanInBlue, 0, 2),
        (EventId::SecretPortal, 0, 0),
        (EventId::Purifier, 0, 0),
        (EventId::FountainOfCurseCleansing, 0, 0),
        (EventId::ForgottenAltar, 0, 2),
        (EventId::TombRedMask, 0, 1),
        (EventId::GoldenWing, 0, 0),
        (EventId::MysteriousSphere, 0, 1),
        (EventId::GoldenIdol, 0, 0),
        (EventId::DrugDealer, 0, 0),
        (EventId::Mausoleum, 0, 0),
        (EventId::MaskedBandits, 0, 0),
        (EventId::TheLibrary, 0, 1),
        (EventId::Vampires, 0, 0),
        (EventId::BonfireElementals, 1, 0),
        (EventId::Colosseum, 1, 0),
        (EventId::SpireHeart, 0, 0),
        (EventId::CursedTome, 4, 0),
        (EventId::DeadAdventurer, 0, 1),
        (EventId::ScrapOoze, 0, 1),
        (EventId::Designer, 1, 0),
        (EventId::Falling, 1, 0),
        (EventId::GremlinWheelGame, 0, 0),
        (EventId::KnowingSkull, 1, 1),
        (EventId::MatchAndKeep, 2, 0),
        (EventId::Nloth, 0, 0),
        (EventId::NoteForYourself, 1, 0),
        (EventId::SensoryStone, 1, 2),
        (EventId::TheJoust, 1, 1),
        (EventId::WeMeetAgain, 0, 0),
    ] {
        let mut checkpoints = Vec::new();
        let mut labels = Vec::new();
        for locale in [Locale::En, Locale::ZhCn] {
            let directory = TestDirectory::new();
            let mut session = fixture("reward");
            session.engine_state = EngineState::EventRoom;
            session.run_state.current_hp = 17;
            session.run_state.max_hp = 83;
            session.run_state.gold = 250;
            let mut state = EventState::new(event);
            state.current_screen = screen;
            state.internal_state = 23;
            session.run_state.event_state = Some(state);
            if event == EventId::Designer {
                session
                    .run_state
                    .event_state
                    .as_mut()
                    .unwrap()
                    .internal_state = 0;
            }
            if event == EventId::Falling {
                let state = sts_oracle_learning::content::events::falling::init_falling_state(
                    &mut session.run_state,
                );
                session
                    .run_state
                    .event_state
                    .as_mut()
                    .unwrap()
                    .internal_state = state;
            }
            if event == EventId::Nloth {
                session.run_state.relics.push(
                    sts_oracle_learning::content::relics::RelicState::new(RelicId::Anchor),
                );
                let state = sts_oracle_learning::content::events::nloth::init_nloth_state(
                    &mut session.run_state,
                );
                session
                    .run_state
                    .event_state
                    .as_mut()
                    .unwrap()
                    .internal_state = state;
            }
            if event == EventId::NoteForYourself {
                session.run_state.note_for_yourself_card = CardId::SearingBlow;
                session.run_state.note_for_yourself_upgrades = 2;
            }
            if matches!(event, EventId::MatchAndKeep | EventId::WeMeetAgain) {
                let mut state = session.run_state.event_state.take().unwrap();
                if event == EventId::MatchAndKeep {
                    sts_oracle_learning::content::events::match_and_keep::init_match_game_board(
                        &mut session.run_state,
                        &mut state,
                    );
                } else {
                    session.run_state.potions[0] =
                        Some(sts_oracle_learning::content::potions::Potion::new(
                            sts_oracle_learning::content::potions::PotionId::FirePotion,
                            42,
                        ));
                    sts_oracle_learning::content::events::we_meet_again::init_we_meet_again_event_state(&mut session.run_state, &mut state);
                }
                session.run_state.event_state = Some(state);
            }
            let deck_size = session.run_state.master_deck.len();
            let mut game =
                controller_with_session(&directory.0, "translated-event-choice", &session);
            let reading = game
                .decision_reading_v1(&[DecisionDetailV1::Options], None, locale)
                .unwrap();
            let option = reading.options.iter().find(|option| matches!(&option.candidate,
                TaskPublicCandidateV1::Strategic { action: TaskStrategicActionV1::ChooseEventOption { option_index: index, .. } } if *index == option_index)).unwrap();
            labels.push(option.label.clone());
            if event == EventId::NoteForYourself {
                let TaskPublicCandidateV1::Strategic {
                    action:
                        TaskStrategicActionV1::ChooseEventOption {
                            public_semantics, ..
                        },
                } = &option.candidate
                else {
                    panic!("paper note choice");
                };
                assert_eq!(public_semantics.specific_cards_gained[0].upgrades, 2);
                assert!(option.label.contains("+2"));
            }
            game.choose_bound_v1(&crate::decision::RunPlayBoundChoiceV1 {
                source: reading.source.clone().expect("bound event reading"),
                choice: option.choice.clone(),
            })
            .unwrap();
            let actual = game
                .current_external_exact_checkpoint_v1()
                .unwrap()
                .into_session()
                .unwrap();
            match event {
                EventId::FaceTrader => {
                    assert_eq!(actual.run_state.current_hp, 9);
                    assert_eq!(actual.run_state.gold, 300);
                }
                EventId::BigFish => assert_eq!(actual.run_state.current_hp, 40),
                EventId::WorldOfGoop => assert_eq!(actual.run_state.gold, 227),
                EventId::LivingWall | EventId::Purifier | EventId::BonfireElementals => assert!(
                    matches!(actual.engine_state, EngineState::RunPendingChoice(_))
                ),
                EventId::Mushrooms => {
                    assert_eq!(actual.run_state.current_hp, 37);
                    assert!(actual.run_state.master_deck.iter().any(
                        |card| card.id == sts_oracle_learning::content::cards::CardId::Parasite
                    ));
                }
                EventId::Lab => {
                    let EngineState::RewardScreen(ref rewards) = actual.engine_state else {
                        panic!("Lab must open its reward screen")
                    };
                    assert_eq!(rewards.items.len(), 2);
                }
                EventId::Cleric => {
                    assert_eq!(actual.run_state.current_hp, 37);
                    assert_eq!(actual.run_state.gold, 215);
                }
                EventId::Addict => assert_eq!(actual.run_state.gold, 165),
                EventId::Beggar => {
                    assert_eq!(actual.run_state.gold, 175);
                    assert_eq!(
                        actual
                            .run_state
                            .event_state
                            .as_ref()
                            .unwrap()
                            .current_screen,
                        2
                    );
                }
                EventId::Ghosts => {
                    assert_eq!(actual.run_state.max_hp, 41);
                    assert_eq!(actual.run_state.master_deck.len(), deck_size + 3);
                    assert!(actual
                        .run_state
                        .master_deck
                        .iter()
                        .any(|card| card.id == CardId::Apparition));
                }
                EventId::Nest => {
                    assert_eq!(actual.run_state.current_hp, 11);
                    assert!(actual
                        .run_state
                        .master_deck
                        .iter()
                        .any(|card| card.id == CardId::RitualDagger));
                }
                EventId::MindBloom => assert!(actual
                    .run_state
                    .relics
                    .iter()
                    .any(|relic| relic.id == RelicId::MarkOfTheBloom)),
                EventId::MoaiHead => {
                    assert_eq!(actual.run_state.max_hp, 68);
                    assert_eq!(actual.run_state.current_hp, 68);
                }
                EventId::WindingHalls => {
                    assert_eq!(actual.run_state.current_hp, 2);
                    assert_eq!(actual.run_state.master_deck.len(), deck_size + 2);
                }
                EventId::WomanInBlue => {
                    assert_eq!(actual.run_state.gold, 210);
                    let EngineState::RewardScreen(ref rewards) = actual.engine_state else {
                        panic!("potions must enter rewards")
                    };
                    assert_eq!(rewards.items.len(), 3);
                }
                EventId::SecretPortal | EventId::FountainOfCurseCleansing | EventId::GoldenWing => {
                    assert_eq!(
                        actual
                            .run_state
                            .event_state
                            .as_ref()
                            .unwrap()
                            .current_screen,
                        1
                    );
                    if event == EventId::GoldenWing {
                        assert_eq!(actual.run_state.current_hp, 10);
                    }
                }
                EventId::ForgottenAltar => assert!(actual
                    .run_state
                    .master_deck
                    .iter()
                    .any(|card| card.id == CardId::Decay)),
                EventId::TombRedMask => {
                    assert_eq!(actual.run_state.gold, 0);
                    assert!(actual
                        .run_state
                        .relics
                        .iter()
                        .any(|relic| relic.id == RelicId::RedMask));
                }
                EventId::MysteriousSphere => assert_eq!(
                    actual
                        .run_state
                        .event_state
                        .as_ref()
                        .unwrap()
                        .current_screen,
                    2
                ),
                EventId::GoldenIdol => assert!(actual
                    .run_state
                    .relics
                    .iter()
                    .any(|relic| relic.id == RelicId::GoldenIdol)),
                EventId::DrugDealer => assert!(actual
                    .run_state
                    .master_deck
                    .iter()
                    .any(|card| card.id == CardId::JAX)),
                EventId::Mausoleum => assert_eq!(
                    actual
                        .run_state
                        .event_state
                        .as_ref()
                        .unwrap()
                        .current_screen,
                    1
                ),
                EventId::MaskedBandits => assert_eq!(actual.run_state.gold, 0),
                EventId::TheLibrary => assert_eq!(actual.run_state.current_hp, 34),
                EventId::Vampires => {
                    assert_eq!(actual.run_state.max_hp, 58);
                    assert_eq!(
                        actual
                            .run_state
                            .master_deck
                            .iter()
                            .filter(|card| card.id == CardId::Bite)
                            .count(),
                        5
                    );
                }
                EventId::Colosseum => assert!(actual.active_combat.is_some()),
                EventId::SpireHeart | EventId::ScrapOoze => assert_eq!(
                    actual
                        .run_state
                        .event_state
                        .as_ref()
                        .unwrap()
                        .current_screen,
                    1
                ),
                EventId::DeadAdventurer => assert_eq!(
                    actual
                        .run_state
                        .event_state
                        .as_ref()
                        .unwrap()
                        .current_screen,
                    2
                ),
                EventId::CursedTome => {
                    assert_eq!(actual.run_state.current_hp, 2);
                    assert!(matches!(actual.engine_state, EngineState::RewardScreen(_)));
                }
                EventId::Designer => {
                    assert_eq!(actual.run_state.gold, 200);
                    assert_eq!(
                        actual
                            .run_state
                            .master_deck
                            .iter()
                            .filter(|card| card.upgrades > 0)
                            .count(),
                        2
                    );
                }
                EventId::Falling => assert_eq!(actual.run_state.master_deck.len(), deck_size - 1),
                EventId::GremlinWheelGame => {
                    assert_eq!(
                        actual
                            .run_state
                            .event_state
                            .as_ref()
                            .unwrap()
                            .current_screen,
                        3
                    );
                    assert_eq!(actual.run_state.gold, 250);
                }
                EventId::KnowingSkull => {
                    assert_eq!(actual.run_state.gold, 340);
                    assert_eq!(actual.run_state.current_hp, 11);
                }
                EventId::MatchAndKeep => assert_eq!(
                    actual.run_state.event_state.as_ref().unwrap().extra_data[14],
                    0
                ),
                EventId::Nloth => assert!(actual
                    .run_state
                    .relics
                    .iter()
                    .any(|relic| relic.id == RelicId::NlothsGift)),
                EventId::NoteForYourself => {
                    assert!(matches!(
                        actual.engine_state,
                        EngineState::RunPendingChoice(_)
                    ));
                    assert_eq!(actual.run_state.master_deck.len(), deck_size + 1);
                    assert!(actual
                        .run_state
                        .master_deck
                        .iter()
                        .any(|card| card.id == CardId::SearingBlow && card.upgrades == 2));
                }
                EventId::SensoryStone => {
                    assert_eq!(actual.run_state.current_hp, 7);
                    let EngineState::RewardScreen(ref rewards) = actual.engine_state else {
                        panic!("three colorless card rewards");
                    };
                    assert_eq!(rewards.items.len(), 3);
                }
                EventId::TheJoust => assert_eq!(actual.run_state.gold, 200),
                EventId::WeMeetAgain => assert!(actual.run_state.potions[0].is_none()),
                _ => unreachable!(),
            }
            checkpoints.push(
                serde_json::to_value(RunControlSessionCheckpointV1::from_session(&actual)).unwrap(),
            );
        }
        assert_ne!(labels[0], labels[1]);
        assert_eq!(
            checkpoints[0], checkpoints[1],
            "{event:?}: actual effects and RNG must be independent of translated button labels"
        );
    }
}

#[test]
fn run_query_languages_keep_bound_recovery_state_and_rng_unchanged() {
    for kind in ["reward", "combat_multiturn"] {
        let directory = TestDirectory::new();
        let game = controller_with_session(&directory.0, "run-query-locale", &fixture(kind));
        let head = game.head().clone();
        let mut public = game
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        for asc in [4, 5, 20] {
            match &mut public.observation {
                TaskPublicObservationV1::Strategic { observation } => {
                    observation.run.ascension_level = asc;
                    observation.run.max_hp = 82;
                }
                TaskPublicObservationV1::Combat {
                    observation,
                    run_context,
                } => {
                    observation.player.ascension_level = asc;
                    observation.player.max_hp = 82;
                    if let sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Available { ascension_level, .. } = run_context {
                        *ascension_level = asc;
                    }
                }
            }
            let before = public.clone();
            for locale in [Locale::En, Locale::ZhCn] {
                for query in ["rules", "rules preparation"] {
                    let text = crate::mechanics_info::render(Some(&public), query, locale).unwrap();
                    let expected = match (locale, asc >= 5) {
                        (Locale::En, true) => "heal 75% of missing HP",
                        (Locale::En, false) => "heal to Max HP",
                        (Locale::ZhCn, true) => "回复已损失生命值的 75%，四舍五入取整",
                        (Locale::ZhCn, false) => "回复至生命上限",
                    };
                    assert!(text.contains(expected), "{kind} A{asc} {locale}: {text}");
                    assert!(text.contains("info transition <HP>"));
                }
                for (hp, a5_recovery, a5_after) in
                    [(25, 43, 68), (42, 30, 72), (80, 2, 82), (82, 0, 82)]
                {
                    let (recovered, after) = if asc < 5 {
                        (82 - hp, 82)
                    } else {
                        (a5_recovery, a5_after)
                    };
                    let text = crate::mechanics_info::render(
                        Some(&public),
                        &format!("transition {hp}"),
                        locale,
                    )
                    .unwrap();
                    let expected = match locale {
                        Locale::En => format!("Transition recovery: +{recovered} HP -> {after}/82"),
                        Locale::ZhCn => format!("跨幕回复：+{recovered} 生命 -> {after}/82"),
                    };
                    assert!(text.contains(&expected), "{text}");
                }
            }
            assert_eq!(public, before);
            assert_eq!(
                *game.head(),
                head,
                "queries cannot change state, candidates or RNG"
            );
        }
    }
}

#[test]
fn localized_rules_distinguish_current_act_from_pending_act_preparation() {
    let directory = TestDirectory::new();
    let game = controller_with_session(&directory.0, "pending-rules-locale", &fixture("reward"));
    let mut public = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = &mut public.observation else {
        unreachable!()
    };
    observation.run_goal = sts_oracle_learning::ai::planner_core::PlannerRunGoal::HeartVictory;
    observation.run.act = 1;
    observation.run.ascension_level = 20;
    observation.run.pending_act_entry = Some(2);
    observation.run.keys = [true, false, true];
    observation.public_map.boss =
        Some(sts_oracle_learning::content::monsters::factory::EncounterId::Hexaghost);
    let before = public.clone();
    for query in ["rules", "rules preparation"] {
        let en = crate::mechanics_info::render(Some(&public), query, Locale::En).unwrap();
        let zh = crate::mechanics_info::render(Some(&public), query, Locale::ZhCn).unwrap();
        assert!(en.contains("Preparation Act 2 elite encounter pool"));
        assert!(zh.contains("即将进入的第 2 幕精英池"));
        assert!(zh.contains("已击败第 1 幕 Boss：六火亡魂"), "{zh}");
        assert!(!zh.contains("第 1 幕地图公开的 Boss"));
        assert!(zh.contains("营火 -> 商店 -> 精英 -> Boss -> 胜利"));
        assert!(zh.contains("第三幕 Boss 战后直接前进，没有普通奖励页"));
        assert!(zh.contains("公开遭遇的准备机制（可能招式与初始效果）"));
        assert!(en.contains("Scouring Whip: add 3 Wound to discard pile"));
        assert!(zh.contains("鞭挞: 将 3 张伤口加入弃牌堆"));
        assert!(zh.contains("蓝宝石"));
        assert!(
            !zh.contains(crate::mechanics_info::KeyRuleV1::RubyOpportunities.text(Locale::ZhCn))
        );
        if query == "rules" {
            assert!(zh.contains("使用 deck-lab list"));
            assert!(!zh.contains("当前第 1 幕的 deck-lab"));
            assert!(zh.contains("遭遇定义：腐化之心 [TheHeart]"), "{zh}");
            assert!(zh.contains("初始状态：死亡律动 2"));
            assert!(zh.contains("初始状态：坚不可摧 200"));
        }
    }
    assert_eq!(public, before);
    let TaskPublicObservationV1::Strategic { observation } = &mut public.observation else {
        unreachable!()
    };
    observation.run.pending_act_entry = None;
    observation.run.act = 2;
    observation.public_map.boss =
        Some(sts_oracle_learning::content::monsters::factory::EncounterId::TheChamp);
    let zh = crate::mechanics_info::render(Some(&public), "rules", Locale::ZhCn).unwrap();
    assert!(zh.contains("当前第 2 幕的 deck-lab"));
    assert!(zh.contains("第 2 幕地图公开的 Boss：第一勇士"));
    assert!(zh.contains("当前第 2 幕的精英池"));
    assert!(!zh.contains("即将进入的第 2 幕"));
}

#[test]
fn localized_query_help_and_errors_preserve_usable_commands() {
    let help = crate::mechanics_info::render(None, "help", Locale::ZhCn).unwrap();
    assert!(help.contains("完整英文名或稳定 ID"));
    for command in [
        "info M0",
        "info rules preparation",
        "info transition <HP>",
        "info keys",
    ] {
        assert!(help.contains(command));
    }
    for (query, expected) in [
        ("rules", "当前进阶不可用"),
        ("rules preparation", "当前进阶不可用"),
        ("transition", "info transition <HP>"),
        ("transition 3.5", "整数"),
        ("transition 25", "当前生命上限不可用"),
        ("enemy Louse", "info M0"),
        ("encounter NotAnEncounter", "稳定遭遇 ID"),
        ("power NotAPower", "未知状态名称"),
        ("card RubyKey", "info RubyKey"),
        ("relic SapphireKey", "info SapphireKey"),
        ("NotACard+", "未知卡牌名称"),
        ("card Bash＋", "半角 '+'"),
        ("card Bash++", "Card+N"),
        ("card Bash+256", "0..=255"),
        ("card +", "卡牌查询格式无效"),
        ("player", "需要正在进行的战斗"),
        ("powers", "需要正在进行的战斗"),
        ("M0", "按槽位查看敌人"),
        ("M256", "槽位超出范围"),
        ("event Neow", "不带参数的 info"),
        ("NotAnEntity", "未自动选取模糊匹配结果"),
    ] {
        let error = crate::mechanics_info::render(None, query, Locale::ZhCn).unwrap_err();
        assert!(error.contains(expected), "{query}: {error}");
        assert!(!error.contains("{{"), "{query}: {error}");
    }
    let directory = TestDirectory::new();
    let game = controller_with_session(
        &directory.0,
        "query-errors-locale",
        &fixture("combat_multiturn"),
    );
    let public = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    for query in ["transition 0", "transition -1", "transition 9999"] {
        let error = crate::mechanics_info::render(Some(&public), query, Locale::ZhCn).unwrap_err();
        assert!(error.contains("假设的存活生命值须在 1..="), "{error}");
    }
    let error = crate::mechanics_info::render(Some(&public), "M255", Locale::ZhCn).unwrap_err();
    assert!(error.contains("没有敌人槽位 M255"));
}

#[test]
fn event_locales_preserve_sources_raw_labels_effects_and_game_state() {
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventState},
    };
    for (event, screen) in [
        (EventId::Neow, 0),
        (EventId::Lab, 0),
        (EventId::Mushrooms, 0),
        (EventId::TheJoust, 1),
        (EventId::Colosseum, 2),
        (EventId::ForgottenAltar, 0),
        (EventId::DrugDealer, 0),
        (EventId::GoldenIdol, 1),
        (EventId::MatchAndKeep, 0),
    ] {
        let directory = TestDirectory::new();
        let mut session = fixture("reward");
        session.engine_state = EngineState::EventRoom;
        let mut state = EventState::new(event);
        state.current_screen = screen;
        session.run_state.event_state = Some(state);
        let mut game = controller_with_session(&directory.0, "event-locale", &session);
        let before = game.head().clone();
        let en = game
            .decision_reading_v1(
                &[DecisionDetailV1::Options, DecisionDetailV1::State],
                None,
                Locale::En,
            )
            .unwrap();
        let zh = game
            .decision_reading_v1(
                &[DecisionDetailV1::Options, DecisionDetailV1::State],
                None,
                Locale::ZhCn,
            )
            .unwrap();
        assert_eq!(en.source, zh.source, "{event:?}");
        assert_eq!(en.state, zh.state, "{event:?}");
        assert_eq!(
            *game.head(),
            before,
            "{event:?}: language cannot advance the event or RNG"
        );
        let candidates = |reading: &RunPlayDecisionReadingV1| {
            reading
                .options
                .iter()
                .map(|o| (o.candidate.clone(), o.choice.clone()))
                .collect::<Vec<_>>()
        };
        // Compare owned values so this also covers source public labels and all
        // typed effects, rather than reconstructing semantics from either text.
        assert_eq!(candidates(&en), candidates(&zh), "{event:?}");
        assert!(zh.view.contains("效果："), "{event:?}: {}", zh.view);
        assert!(!zh.view.contains("事件效果（英文）"));
        if event == EventId::TheJoust {
            assert!(zh.view.contains("宠物主人 30%，杀宠凶手 70%"));
        }
        if event == EventId::Colosseum {
            assert!(zh.view.contains("战斗奖励在胜利后获得"));
            assert!(zh.view.contains("随机稀有遗物"));
        }
    }
}

#[test]
fn localized_key_queries_keep_current_keys_and_distinct_acquisition_rules() {
    let directory = TestDirectory::new();
    let game = controller_with_session(&directory.0, "key-query-locale", &fixture("reward"));
    let mut public = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = &mut public.observation else {
        panic!("strategic fixture")
    };
    observation.run.keys = [true, false, true];
    let before = public.clone();
    let en = crate::mechanics_info::render(Some(&public), "keys", Locale::En).unwrap();
    let zh = crate::mechanics_info::render(Some(&public), "keys", Locale::ZhCn).unwrap();
    assert!(en.starts_with("Keys: Ruby, Emerald\nMissing keys: Sapphire:"));
    assert!(zh.starts_with("钥匙：红宝石, 绿宝石\n缺少钥匙：蓝宝石："));
    assert!(zh.contains("第三幕前集齐三把钥匙"));
    assert!(zh.contains("已获得的钥匙会保留到后续幕"));
    let ruby = crate::mechanics_info::render(None, "RubyKey", Locale::ZhCn).unwrap();
    assert!(ruby.contains("地图 y=14"));
    assert!(ruby.contains("回忆会占用该营火的一次行动"));
    let sapphire = crate::mechanics_info::render(None, "SapphireKey", Locale::ZhCn).unwrap();
    assert!(sapphire.contains("地图 y=8"));
    assert!(sapphire.contains("领取任一项都会移除另一项"));
    assert!(sapphire.contains("问号房间遇到的宝箱也一样"));
    let emerald = crate::mechanics_info::render(None, "EmeraldKey", Locale::ZhCn).unwrap();
    assert!(emerald.contains("绿宝石钥匙与该精英的其他奖励可以全部领取"));
    assert!(emerald.contains("第一、第二、第三幕各标记一只燃烧精英"));
    assert_eq!(public, before);
}

#[test]
fn original_event_names_and_current_scene_share_connection_locale_without_changing_choices() {
    use sts_oracle_learning::eval::run_control::{RunControlConfig, RunControlSession};
    use sts_oracle_learning::state::{
        core::EngineState,
        events::{EventId, EventState},
    };
    let directory = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::EventRoom;
    let mut event = EventState::new(EventId::FaceTrader);
    event.current_screen = 1;
    session.run_state.event_state = Some(event);
    let mut game = controller_with_session(&directory.0, "event-scene-locale", &session);
    let before = game.head().clone();
    let en = game
        .decision_reading_v1(&[DecisionDetailV1::State], None, Locale::En)
        .unwrap();
    let zh = game
        .decision_reading_v1(&[DecisionDetailV1::State], None, Locale::ZhCn)
        .unwrap();
    assert!(en.summary.contains("Face Trader | Event screen 1"));
    assert!(zh.summary.contains("换脸商 | 事件阶段 1"));
    assert!(en.view.contains("Scene: You swerve"));
    assert!(zh.view.contains("事件正文："));
    assert!(!zh.view.contains("事件正文（英文）"));
    assert!(!zh
        .view
        .contains("Unknown (installed game text unavailable)"));
    assert_eq!(en.state, zh.state);
    assert_eq!(en.source, zh.source);
    assert_eq!(
        en.options
            .iter()
            .map(|option| (&option.choice, &option.candidate))
            .collect::<Vec<_>>(),
        zh.options
            .iter()
            .map(|option| (&option.choice, &option.candidate))
            .collect::<Vec<_>>()
    );
    assert_eq!(*game.head(), before);
}

#[test]
fn campfire_locales_keep_upgrade_values_copy_groups_and_legal_bindings() {
    use sts_oracle_learning::content::cards::CardId;
    use sts_oracle_learning::state::core::EngineState;
    let directory = TestDirectory::new();
    let mut session = fixture("reward");
    session.engine_state = EngineState::Campfire;
    session.run_state.master_deck.clear();
    for card in [
        CardId::Prepared,
        CardId::Prepared,
        CardId::TrueGrit,
        CardId::Armaments,
        CardId::Adrenaline,
        CardId::LimitBreak,
        CardId::DoubleTap,
    ] {
        session.run_state.add_card_to_deck(card);
    }
    let mut game = controller_with_session(&directory.0, "upgrade-locale", &session);
    let before = game.head().clone();
    let en = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None, Locale::En)
        .unwrap();
    let zh = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None, Locale::ZhCn)
        .unwrap();
    let full = game.decision_reading_v1(&[], None, Locale::ZhCn).unwrap();
    assert!(en.view.contains("draw 1 -> 2; discard 1 -> 2"));
    assert!(zh.view.contains("抽牌 1 -> 2; 弃牌 1 -> 2"));
    assert!(zh.view.contains("2 张同效果副本中选 1 张"));
    assert!(zh.view.contains("消耗目标：随机 -> 自选"));
    assert!(zh.view.contains("战斗中升级：一张手牌 -> 所有手牌"));
    assert!(zh.view.contains("打出后消耗 是 -> 否"));
    assert!(zh.view.contains("机制补充："));
    assert!(full.view.contains("可升级效果："));
    let public = before
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = &public.observation else {
        panic!("strategic fixture")
    };
    let en_groups = smith::comparisons(observation, &en.options, Locale::En).unwrap();
    let zh_groups = smith::comparisons(observation, &zh.options, Locale::ZhCn).unwrap();
    assert_eq!(
        en_groups
            .iter()
            .map(|group| &group.ordinals)
            .collect::<Vec<_>>(),
        zh_groups
            .iter()
            .map(|group| &group.ordinals)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        en.options
            .iter()
            .map(|option| &option.choice)
            .collect::<Vec<_>>(),
        zh.options
            .iter()
            .map(|option| &option.choice)
            .collect::<Vec<_>>()
    );
    assert_eq!(*game.head(), before);
}

#[test]
fn strategic_locales_preserve_public_choices_and_share_preparation_overviews() {
    for kind in [
        "reward",
        "route",
        "shop_twins",
        "full_reward",
        "empty_cage",
        "bottled_flame",
    ] {
        let dir = TestDirectory::new();
        let mut game = controller_with_session(&dir.0, kind, &fixture(kind));
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
        let preview = read_preview_decision_v1(public, None, true, Locale::ZhCn).unwrap();
        let prepared = preparation::reading(public, Locale::ZhCn).unwrap().unwrap();
        assert_eq!(en.state, zh.state);
        assert_eq!(en.source, zh.source);
        assert_ne!(en.summary, zh.summary);
        assert!(zh.summary.starts_with("第 "), "{kind}: {}", zh.summary);
        assert_eq!(zh.summary, full.summary);
        assert_eq!(zh.summary, preview.summary);
        assert!(prepared.overview.starts_with(&zh.summary));
        assert!(prepared.overview.contains("钥匙："));
        assert!(zh.view.contains("药水："));
        let choices = |reading: &RunPlayDecisionReadingV1| {
            reading
                .options
                .iter()
                .map(|option| (option.choice.clone(), option.candidate.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(choices(&en), choices(&zh));
        assert_eq!(
            en.selections
                .iter()
                .map(|s| (&s.choice, s.min, s.max, s.members.len()))
                .collect::<Vec<_>>(),
            zh.selections
                .iter()
                .map(|s| (&s.choice, s.min, s.max, s.members.len()))
                .collect::<Vec<_>>()
        );
        assert_eq!(*game.head(), before);
    }
}

#[test]
fn strategic_deck_summary_preserves_identities_and_permanent_modifiers() {
    use sts_oracle_learning::content::cards::CardId;
    let dir = TestDirectory::new();
    let game = controller_with_session(&dir.0, "summary-copies", &fixture("reward"));
    let mut public = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = &mut public.observation else {
        panic!("strategic fixture")
    };
    let mut card = observation.cards[0];
    card.card = CardId::Strike;
    card.ordinal = 0;
    card.upgrades = 0;
    card.cost_modifier = 0;
    card.base_damage_override = None;
    card.base_block_override = None;
    card.misc_value = 0;
    card.bottled_by = None;
    observation.cards = vec![
        card,
        TaskStrategicCardV1 { ordinal: 1, ..card },
        TaskStrategicCardV1 {
            card: CardId::StrikeG,
            ordinal: 2,
            ..card
        },
        TaskStrategicCardV1 {
            ordinal: 3,
            cost_modifier: -1,
            ..card
        },
    ];
    let before = public.clone();
    let reading = read_public_decision_v1(
        &public,
        &[],
        None,
        None,
        None,
        Locale::ZhCn,
        DecisionDisplayV1::Standard,
    )
    .unwrap();
    assert!(
        reading.view.contains("2x 打击 [Strike]"),
        "{}",
        reading.view
    );
    assert!(reading.view.contains("打击 [StrikeG]"));
    assert!(reading.view.contains("费用 -1"));
    assert_eq!(public, before);
}

#[test]
fn preparation_window_locales_keep_transition_recovery_and_boss_scope() {
    let dir = TestDirectory::new();
    let game = controller_with_session(&dir.0, "entry-locale", &fixture("reward"));
    let public = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { mut observation } = public.observation else {
        panic!("strategic fixture")
    };
    observation.run.act = 2;
    observation.run.current_hp = 10;
    observation.run.max_hp = 80;
    observation.run.ascension_level = 20;
    observation.run.pending_act_entry = Some(3);
    observation.public_map.boss =
        Some(sts_oracle_learning::content::monsters::factory::EncounterId::Collector);
    let mut en = String::new();
    let mut zh = String::new();
    crate::mechanics_info::write_preparation_window(&mut en, &observation, Locale::En);
    crate::mechanics_info::write_preparation_window(&mut zh, &observation, Locale::ZhCn);
    assert!(en.contains("63/80 HP"), "{en}");
    assert!(zh.contains("63/80 生命"), "{zh}");
    assert!(zh.contains("已击败第 2 幕 Boss：收藏家"));
    assert!(zh.contains("进入该幕时揭晓"));
    assert!(!zh.contains("Boss：收藏家\nBoss："));
}

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
        let preview = read_preview_decision_v1(public, None, true, Locale::ZhCn).unwrap();
        assert_eq!(en.state, zh.state);
        assert_eq!(en.source, zh.source);
        assert_ne!(en.summary, zh.summary);
        assert!(zh.summary.contains("手牌："), "{kind}: {}", zh.view);
        assert_eq!(zh.summary, full.summary);
        assert_eq!(zh.summary, preview.summary);
        assert!(full.view.contains("本局面规则："));
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
        let preview = read_preview_decision_v1(&public, None, true, Locale::En).unwrap();
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
    let reading = read_preview_decision_v1(&public, None, true, Locale::En).unwrap();
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
        let preview = read_preview_decision_v1(&decision, None, true, Locale::En).unwrap();
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
            let selector = crate::selector::parse_action_selector_v1(
                &semantic,
                "rendered choice",
                sts_presentation::Locale::En,
            )
            .unwrap();
            let choice = crate::selector::resolve_action_selector_v1(
                &decision,
                &selector,
                sts_presentation::Locale::En,
            )
            .unwrap();
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
            let selector = crate::selector::parse_action_selector_v1(
                &command,
                "rendered multi-selection",
                sts_presentation::Locale::En,
            )
            .unwrap();
            let choice = crate::selector::resolve_action_selector_v1(
                &decision,
                &selector,
                sts_presentation::Locale::En,
            )
            .unwrap();
            assert!(choice
                .ordinal_path
                .starts_with(&selection.choice.ordinal_path));
            decision.validate_choice(&choice).unwrap();
            if count > 1 {
                let repeated = format!("select:{},{}", tokens[0], tokens[0]);
                let repeated = crate::selector::parse_action_selector_v1(
                    &repeated,
                    "duplicate selection",
                    sts_presentation::Locale::En,
                )
                .unwrap();
                assert!(crate::selector::resolve_action_selector_v1(
                    &decision,
                    &repeated,
                    sts_presentation::Locale::En
                )
                .is_err());
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
    for with_binding in [false, true] {
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
                    if !with_binding {
                        public_semantics.structured_semantics = None;
                    }
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
                DecisionDisplayV1::Standard,
            )
            .unwrap();
            let displayed = if with_binding { "[Continue]" } else { label };
            for (choice, command) in expected {
                assert!(
                    live.view
                        .contains(&format!("./p {command}  |  Effect: {displayed}")),
                    "{}",
                    live.view
                );
                let selector = crate::selector::parse_action_selector_v1(
                    &command,
                    "event",
                    sts_presentation::Locale::En,
                )
                .unwrap();
                assert_eq!(
                    crate::selector::resolve_action_selector_v1(
                        &decision,
                        &selector,
                        sts_presentation::Locale::En
                    )
                    .unwrap(),
                    choice
                );
            }
        }
    }
    assert_eq!(*game.head(), before);
}
