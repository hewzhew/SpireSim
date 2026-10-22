// Java: GremlinMatchGame (shrines) — "Match and Keep!"
// Memory card game: 12 cards face-down (6 pairs), player has 5 attempts to match pairs.
//
// Card pool (Java initializeCards):
//   A15+: Rare + Uncommon + Common + Curse + Curse + StartCard (Bash for Ironclad)
//   Non-A15: Rare + Uncommon + Common + ColorlessUncommon + Curse + StartCard
//   Each card duplicated (pair), then shuffled with miscRng.randomLong() seed.
//
// Game state stored in extra_data:
//   [0..12]  = card type indices for each board position (0-5)
//   [12]     = matched bitmask (bit i = position i matched/removed)
//   [13]     = attempts remaining (starts at 5)
//   [14]     = first flipped position (-1 if none)
//   [15]     = last flipped 1 pos (-1 if none)
//   [16]     = last flipped 2 pos (-1 if none)
//   [17]     = revealed bitmask (remembered faces, including removed pairs)
// Card identities and upgrades are retained separately in EventState.card_payload.
//
// Screen flow:
//   0 = Intro: [Play]
//   1 = Rule explanation: [Start matching]
//   2 = Play: one or two flips, with automatic pair settlement
//   3 = Complete: [Leave]

use crate::content::cards::CardId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardState, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventRandomOutcomeKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

const CARD_TYPE_COUNT: usize = 6;
const LAST_FLIPPED_1_OFFSET: usize = 15;
const LAST_FLIPPED_2_OFFSET: usize = LAST_FLIPPED_1_OFFSET + 1;
const REVEALED_MASK_OFFSET: usize = LAST_FLIPPED_2_OFFSET + 1;

/// Initialize the numeric board and typed card payload directly on EventState.
/// Consumes RNG to match Java: getCard(×3) + returnRandomCurse/returnColorlessCard +
/// miscRng.randomLong() for shuffle seed.
pub fn init_match_game_board(run_state: &mut RunState, event_state: &mut EventState) {
    let mut card_types: [CardId; 6] = [CardId::Strike; 6];

    // Java: getCard(RARE) — uses cardRng
    card_types[0] = run_state.random_card_by_rarity(crate::content::cards::CardRarity::Rare);
    // Java: getCard(UNCOMMON) — uses cardRng
    card_types[1] = run_state.random_card_by_rarity(crate::content::cards::CardRarity::Uncommon);
    // Java: getCard(COMMON) — uses cardRng
    card_types[2] = run_state.random_card_by_rarity(crate::content::cards::CardRarity::Common);

    if run_state.ascension_level >= 15 {
        // A15+: 2 curses, no colorless
        card_types[3] = run_state.random_obtainable_curse();
        card_types[4] = run_state.random_obtainable_curse();
    } else {
        // Non-A15: 1 colorless uncommon + 1 curse
        card_types[3] =
            run_state.random_colorless_card(crate::content::cards::CardRarity::Uncommon);
        card_types[4] = run_state.random_obtainable_curse();
    }

    // Java: player.getStartCardForEvent()
    card_types[5] = match run_state.player_class {
        "Ironclad" => CardId::Bash,
        "Silent" => CardId::Neutralize,
        "Defect" => CardId::Zap,
        "Watcher" => CardId::Eruption,
        _ => CardId::Strike,
    };

    let extra_data = &mut event_state.extra_data;
    extra_data.clear();
    // Positions 0-11: card type index (0-5), 6 pairs
    for i in 0..6i32 {
        extra_data.push(i);
    }
    for i in 0..6i32 {
        extra_data.push(i);
    }

    // Java: Collections.shuffle(cards, new Random(miscRng.randomLong()))
    let seed = run_state.rng_pool.misc_rng.random_long();
    {
        let mut jur = crate::runtime::rng::JavaUtilRandom::new(seed);
        let slice = &mut extra_data[0..12];
        for i in (1..slice.len()).rev() {
            let j = jur.next_int((i + 1) as i32) as usize;
            slice.swap(i, j);
        }
    }

    // [12] = matched bitmask (0 = none matched)
    extra_data.push(0);
    // [13] = attempts remaining
    extra_data.push(5);
    // [14] = first flipped position (-1 = none)
    extra_data.push(-1);
    // Java calls onPreviewObtainCard before duplicating the board pair. Keep
    // these identities typed rather than embedding enum discriminants in the
    // numeric board.
    event_state.card_payload.clear();
    for &card_id in card_types.iter().take(CARD_TYPE_COUNT) {
        event_state.card_payload.push(EventCardState {
            card_id,
            upgrades: run_state.preview_obtain_card_upgrades(card_id, 0),
        });
    }
    // [15] = last flipped 1 pos (-1 if none)
    extra_data.push(-1);
    // [16] = last flipped 2 pos (-1 if none)
    extra_data.push(-1);
    extra_data.push(0); // cells observed during this playthrough
}

pub(crate) fn card_entry_at(
    extra_data: &[i32],
    cards: &[EventCardState],
    pos: usize,
) -> Option<(CardId, u8)> {
    if pos >= 12 {
        return None;
    }
    let type_idx = usize::try_from(*extra_data.get(pos)?).ok()?;
    if type_idx >= CARD_TYPE_COUNT {
        return None;
    }
    cards
        .get(type_idx)
        .map(|card| (card.card_id, card.upgrades))
}

pub(crate) fn is_matched(extra_data: &[i32], pos: usize) -> bool {
    (extra_data[12] & (1 << pos)) != 0
}

fn set_matched(extra_data: &mut [i32], pos: usize) {
    extra_data[12] |= 1 << pos;
}

pub(crate) fn attempts_remaining(extra_data: &[i32]) -> i32 {
    extra_data[13]
}

pub(crate) fn first_flipped(extra_data: &[i32]) -> i32 {
    extra_data[14]
}

/// Raw dealt positions stay fixed when matched cards leave the board.
pub fn available_positions(event: &EventState) -> Vec<usize> {
    if event.current_screen != 2 || event.extra_data.len() != REVEALED_MASK_OFFSET + 1 {
        return Vec::new();
    }
    (0..12)
        .filter(|&pos| {
            !is_matched(&event.extra_data, pos) && first_flipped(&event.extra_data) != pos as i32
        })
        .collect()
}

/// Original placeCards uses i % 4 for x and i % 3 for y.
pub fn cell_position(dealt_position: usize) -> usize {
    dealt_position % 4 + 4 * (dealt_position % 3)
}

pub fn get_options(_run_state: &RunState, event: &EventState) -> Vec<EventOption> {
    match event.current_screen {
        0 | 1 => vec![EventOption::new(
            EventChoiceMeta::new(if event.current_screen == 0 {
                "[Play Match and Keep!]"
            } else {
                "[Start matching]"
            }),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        )],
        2 => available_positions(event)
            .into_iter()
            .map(|pos| {
                EventOption::new(
                    EventChoiceMeta::new(format!("[Flip card {}]", pos)).with_public_detail(
                        crate::state::events::EventOptionPublicDetail::MatchPosition {
                            position: pos,
                        },
                    ),
                    EventOptionSemantics {
                        action: EventActionKind::Special,
                        effects: vec![EventEffect::RandomOutcome {
                            kind: EventRandomOutcomeKind::MatchAndKeepFlip,
                            chance_percent: None,
                        }],
                        ..Default::default()
                    },
                )
            })
            .collect(),
        3 => vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..Default::default()
            },
        )],
        _ => Vec::new(),
    }
}

pub fn get_choices(run_state: &RunState, event: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event = run_state.event_state.take().unwrap();
    match event.current_screen {
        0 | 1 if choice_idx == 0 => event.current_screen += 1,
        2 => {
            if let Some(&pos) = available_positions(&event).get(choice_idx) {
                event.extra_data[REVEALED_MASK_OFFSET] |= 1 << pos;
                let first = first_flipped(&event.extra_data);
                if first < 0 {
                    event.extra_data[14] = pos as i32;
                } else {
                    let first = first as usize;
                    event.extra_data[LAST_FLIPPED_1_OFFSET] = first as i32;
                    event.extra_data[LAST_FLIPPED_2_OFFSET] = pos as i32;
                    if let (Some((card, _)), Some((second, _))) = (
                        card_entry_at(&event.extra_data, &event.card_payload, first),
                        card_entry_at(&event.extra_data, &event.card_payload, pos),
                    ) {
                        if card == second {
                            // Original obtains chosenCard.makeCopy(), then applies
                            // the normal obtain hooks (including the eggs) again.
                            run_state.add_card_to_deck_with_upgrades_from(
                                card,
                                0,
                                DomainEventSource::Event(EventId::MatchAndKeep),
                            );
                            set_matched(&mut event.extra_data, first);
                            set_matched(&mut event.extra_data, pos);
                        }
                    }
                    event.extra_data[13] -= 1;
                    event.extra_data[14] = -1;
                    // Pair display, animation and cleanup settle automatically;
                    // the original has no extra Continue after a pair.
                    if attempts_remaining(&event.extra_data) == 0 {
                        event.current_screen = 3;
                    }
                }
            }
        }
        3 if choice_idx == 0 => event.completed = true,
        _ => {}
    }
    run_state.event_state = Some(event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardType;
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::events::{
        EventActionKind, EventEffect, EventOptionTransition, EventRandomOutcomeKind,
    };
    use crate::state::selection::DomainEvent;

    #[test]
    fn initialization_preserves_java_rng_overloads_and_order() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};

        for ascension in [0, 20] {
            let mut run = RunState::new(11, ascension, false, "Ironclad");
            let mut event = EventState::new(EventId::MatchAndKeep);
            let trace = begin_sts_rng_trace().unwrap();
            init_match_game_board(&mut run, &mut event);
            let calls = trace.finish().unwrap();

            // GremlinMatchGame.initializeCards -> getCard(rarity) ->
            // CardGroup.getRandomCard(true): three single-bound cardRng calls.
            assert_eq!(calls.len(), 6);
            for (call, expected_range) in calls.iter().zip([15, 35, 19]) {
                assert_eq!(call.owner, Some("run.card"));
                assert!(matches!(call.call, StsRngCall::RandomInt { range, .. }
                    if range == expected_range));
            }
            if ascension >= 15 {
                assert_eq!(calls[3].owner, Some("run.card"));
                assert!(matches!(
                    calls[3].call,
                    StsRngCall::RandomIntRange {
                        start: 0,
                        end: 9,
                        ..
                    }
                ));
            } else {
                assert_eq!(calls[3].owner, Some("floor.shuffle"));
                assert!(matches!(calls[3].call, StsRngCall::RandomLongBits { .. }));
            }
            // CardLibrary.getCurse uses the two-bound overload; it must remain
            // distinct even though both overloads can produce the same index.
            assert_eq!(calls[4].owner, Some("run.card"));
            assert!(matches!(
                calls[4].call,
                StsRngCall::RandomIntRange {
                    start: 0,
                    end: 9,
                    ..
                }
            ));
            assert_eq!(calls[5].owner, Some("floor.misc"));
            assert!(matches!(calls[5].call, StsRngCall::RandomLongBits { .. }));
        }
    }

    #[test]
    fn structured_options_expose_hidden_flip_boundaries_without_revealing_cards() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let mut event_state = EventState::new(EventId::MatchAndKeep);
        event_state.current_screen = 2;
        install_board(
            &mut event_state,
            &[
                (CardId::Bash, 1),
                (CardId::Strike, 0),
                (CardId::Defend, 0),
                (CardId::Clumsy, 0),
                (CardId::IronWave, 0),
                (CardId::Cleave, 0),
            ],
        );

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            &event_state,
        )
        .expect("Match and Keep should expose structured event options");

        assert_eq!(options.len(), 12);
        assert_eq!(options[0].ui.text, "[Flip card 0]");
        assert_eq!(options[0].semantics.action, EventActionKind::Special);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::RandomOutcome {
                kind: EventRandomOutcomeKind::MatchAndKeepFlip,
                chance_percent: None,
            }]
        );
        assert_eq!(options[0].semantics.transition, EventOptionTransition::None);

        let mut second_flip = event_state;
        second_flip.current_screen = 2;
        second_flip.extra_data[14] = 0;
        let second_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &second_flip,
            )
            .expect("Match and Keep second flip should expose structured event options");

        assert_eq!(second_options.len(), 11);
        assert!(second_options
            .iter()
            .all(|option| option.ui.text != "[Flip card 0]"));
        assert!(second_options.iter().all(|option| {
            option.semantics.effects
                == vec![EventEffect::RandomOutcome {
                    kind: EventRandomOutcomeKind::MatchAndKeepFlip,
                    chance_percent: None,
                }]
        }));
    }

    #[test]
    fn structured_intro_rules_and_complete_boundaries() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let intro = EventState::new(EventId::MatchAndKeep);
        let intro_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state, &intro,
            )
            .expect("Match and Keep intro should expose structured event options");

        assert_eq!(intro_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            intro_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut result = EventState::new(EventId::MatchAndKeep);
        result.current_screen = 1;
        install_board(
            &mut result,
            &[
                (CardId::Bash, 1),
                (CardId::Strike, 0),
                (CardId::Defend, 0),
                (CardId::Clumsy, 0),
                (CardId::IronWave, 0),
                (CardId::Cleave, 0),
            ],
        );
        let result_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state, &result,
            )
            .expect("Match and Keep result screen should expose structured event options");

        assert_eq!(
            result_options[0].semantics.action,
            EventActionKind::Continue
        );
        assert_eq!(
            result_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut complete = result;
        complete.current_screen = 3;
        let complete_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state, &complete,
            )
            .expect("Match and Keep complete screen should expose structured event options");

        assert_eq!(complete_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            complete_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        assert!(complete_options[0].semantics.terminal);
    }

    #[test]
    fn match_and_keep_start_card_matches_java_player_get_start_card_for_event() {
        let cases = [
            ("Ironclad", CardId::Bash),
            ("Silent", CardId::Neutralize),
            ("Defect", CardId::Zap),
            ("Watcher", CardId::Eruption),
        ];

        for (player_class, expected_start_card) in cases {
            let mut run_state = RunState::new(12345, 0, false, player_class);
            let mut event_state = EventState::new(EventId::MatchAndKeep);
            init_match_game_board(&mut run_state, &mut event_state);

            assert_eq!(
                event_state.card_payload[5].card_id, expected_start_card,
                "Java {}.getStartCardForEvent() must feed GremlinMatchGame.initializeCards",
                player_class
            );
        }
    }

    #[test]
    fn a15_board_uses_the_shared_ordered_curse_pool_without_changing_rng_consumption() {
        let mut expected = RunState::new(12345, 15, false, "Ironclad");
        let expected_cards = [
            expected.random_card_by_rarity(crate::content::cards::CardRarity::Rare),
            expected.random_card_by_rarity(crate::content::cards::CardRarity::Uncommon),
            expected.random_card_by_rarity(crate::content::cards::CardRarity::Common),
            expected.random_obtainable_curse(),
            expected.random_obtainable_curse(),
            CardId::Bash,
        ];
        let _shuffle_seed = expected.rng_pool.misc_rng.random_long();

        let mut actual = RunState::new(12345, 15, false, "Ironclad");
        let mut event_state = EventState::new(EventId::MatchAndKeep);
        init_match_game_board(&mut actual, &mut event_state);

        assert_eq!(
            event_state
                .card_payload
                .iter()
                .map(|card| card.card_id)
                .collect::<Vec<_>>(),
            expected_cards
        );
        assert_eq!(actual.rng_pool.card_rng, expected.rng_pool.card_rng);
        assert_eq!(actual.rng_pool.misc_rng, expected.rng_pool.misc_rng);
    }

    #[test]
    fn generated_board_stores_preview_obtain_upgrades_like_java() {
        let mut run_state = RunState::new(12345, 0, false, "Ironclad");
        run_state.relics.push(RelicState::new(RelicId::MoltenEgg));
        run_state.relics.push(RelicState::new(RelicId::ToxicEgg));
        run_state.relics.push(RelicState::new(RelicId::FrozenEgg));
        let mut event_state = EventState::new(EventId::MatchAndKeep);
        init_match_game_board(&mut run_state, &mut event_state);

        for card in &event_state.card_payload {
            let card_id = card.card_id;
            let upgrades = card.upgrades;
            let def = crate::content::cards::get_card_definition(card_id);
            if def.card_type == CardType::Curse {
                assert_eq!(upgrades, 0);
            } else {
                assert_eq!(
                    upgrades, 1,
                    "GremlinMatchGame calls onPreviewObtainCard before duplicating board cards"
                );
            }
        }
    }

    #[test]
    fn matching_obtains_a_fresh_copy_and_reapplies_obtain_hooks() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut event_state = EventState::new(EventId::MatchAndKeep);
        event_state.current_screen = 2;
        install_board(
            &mut event_state,
            &[
                (CardId::Bash, 1),
                (CardId::Strike, 0),
                (CardId::Defend, 0),
                (CardId::Clumsy, 0),
                (CardId::IronWave, 0),
                (CardId::Cleave, 0),
            ],
        );
        run_state.event_state = Some(event_state);

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        let obtained = run_state.master_deck.last().unwrap();
        assert_eq!(obtained.id, CardId::Bash);
        assert_eq!(obtained.upgrades, 0);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::MatchAndKeep),
            } if card.id == CardId::Bash && card.upgrades == 0
        )));
    }

    #[test]
    fn matching_cards_run_obtain_hooks_before_card_obtained_event() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut event_state = EventState::new(EventId::MatchAndKeep);
        event_state.current_screen = 2;
        install_board(
            &mut event_state,
            &[
                (CardId::Bash, 0),
                (CardId::Strike, 0),
                (CardId::Defend, 0),
                (CardId::Clumsy, 0),
                (CardId::IronWave, 0),
                (CardId::Cleave, 0),
            ],
        );
        run_state.event_state = Some(event_state);

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        let events = run_state.take_emitted_events();
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::MatchAndKeep),
                        ..
                    }
                )
            })
            .expect(
                "Ceramic Fish should run from the matched card ShowCardAndObtainEffect obtain hook",
            );
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::MatchAndKeep),
                    } if card.id == CardId::Bash
                )
            })
            .expect("Match and Keep should obtain a copy of the matched card");

        assert!(
            fish_gold_pos < obtained_pos,
            "Java GremlinMatchGame queues ShowCardAndObtainEffect for the matched card; that effect runs onObtainCard before Soul.obtain adds the card"
        );
    }

    #[test]
    fn matching_uses_card_id_not_board_type_index_like_java() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut event_state = EventState::new(EventId::MatchAndKeep);
        event_state.current_screen = 2;
        install_board(
            &mut event_state,
            &[
                (CardId::Bash, 0),
                (CardId::Bash, 0),
                (CardId::Defend, 0),
                (CardId::Clumsy, 0),
                (CardId::IronWave, 0),
                (CardId::Cleave, 0),
            ],
        );
        run_state.event_state = Some(event_state);

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        let state = run_state.event_state.as_ref().unwrap();
        assert!(is_matched(&state.extra_data, 0));
        assert!(is_matched(&state.extra_data, 1));
        assert_eq!(run_state.master_deck.last().unwrap().id, CardId::Bash);
    }

    #[test]
    fn second_flip_choices_do_not_include_synthetic_disabled_info_row() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let mut event_state = EventState::new(EventId::MatchAndKeep);
        event_state.current_screen = 2;
        install_board(
            &mut event_state,
            &[
                (CardId::Bash, 0),
                (CardId::Bash, 0),
                (CardId::Defend, 0),
                (CardId::Clumsy, 0),
                (CardId::IronWave, 0),
                (CardId::Cleave, 0),
            ],
        );
        event_state.extra_data[14] = 0;

        let choices = get_choices(&run_state, &event_state);

        assert_eq!(choices[0].text, "[Flip card 1]");
        assert!(
            choices.iter().all(|choice| !choice.disabled),
            "Java GremlinMatchGame exposes board card hitboxes, not a disabled dialog choice for the first flipped card"
        );
    }

    #[test]
    fn card_entry_at_rejects_malformed_positions_without_panic() {
        let malformed = vec![48; 40];

        assert!(card_entry_at(&malformed, &[], 48).is_none());
        assert!(card_entry_at(&malformed, &[], 0).is_none());
    }

    #[test]
    fn five_misses_settle_automatically_and_flips_do_not_consume_rng() {
        use crate::runtime::rng::begin_sts_rng_trace;
        let mut run = RunState::new(11, 20, false, "Ironclad");
        let mut event = EventState::new(EventId::MatchAndKeep);
        init_match_game_board(&mut run, &mut event);
        let mut state = EngineState::EventRoom;
        run.event_state = Some(event);
        handle_choice(&mut state, &mut run, 0);
        assert_eq!(run.event_state.as_ref().unwrap().current_screen, 1);
        assert_eq!(
            get_choices(&run, run.event_state.as_ref().unwrap()).len(),
            1
        );
        handle_choice(&mut state, &mut run, 0);
        let event = run.event_state.as_ref().unwrap();
        let other = (1..12)
            .find(|&p| {
                card_entry_at(&event.extra_data, &event.card_payload, p)
                    .unwrap()
                    .0
                    != card_entry_at(&event.extra_data, &event.card_payload, 0)
                        .unwrap()
                        .0
            })
            .unwrap();
        let deck = run.master_deck.clone();
        let trace = begin_sts_rng_trace().unwrap();
        for attempt in 0..5 {
            handle_choice(&mut state, &mut run, 0);
            assert_eq!(run.event_state.as_ref().unwrap().current_screen, 2);
            assert_eq!(
                run.event_state.as_ref().unwrap().extra_data[13],
                5 - attempt
            );
            handle_choice(&mut state, &mut run, other - 1);
            let event = run.event_state.as_ref().unwrap();
            assert_eq!(event.current_screen, if attempt == 4 { 3 } else { 2 });
            assert_eq!(event.extra_data[13], 4 - attempt);
            assert_eq!(event.extra_data[14], -1);
        }
        assert!(trace.finish().unwrap().is_empty());
        assert_eq!(run.master_deck, deck);
        assert_eq!(
            get_choices(&run, run.event_state.as_ref().unwrap()).len(),
            1
        );
    }

    fn install_board(event_state: &mut EventState, entries: &[(CardId, u8); CARD_TYPE_COUNT]) {
        event_state.extra_data = vec![0, 0, 1, 2, 3, 4, 5, 1, 2, 3, 4, 5, 0, 5, -1, -1, -1, 0];
        event_state.card_payload = entries
            .iter()
            .map(|&(card_id, upgrades)| EventCardState { card_id, upgrades })
            .collect();
    }
}
