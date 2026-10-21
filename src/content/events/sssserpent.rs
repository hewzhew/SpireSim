use crate::content::cards::CardId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn gold_reward(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        150
    } else {
        175
    }
}

fn serpent_reward_effects(run_state: &RunState) -> Vec<EventEffect> {
    vec![
        EventEffect::GainGold(gold_reward(run_state)),
        EventEffect::ObtainCurse {
            count: 1,
            kind: EventCardKind::Specific(CardId::Doubt),
        },
    ]
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let gold = gold_reward(run_state);
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Agree] Gain {} Gold. Become Cursed - Doubt.",
                        gold
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Accept,
                        effects: serpent_reward_effects(run_state),
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Disagree] Leave."),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
            ]
        }
        1 => {
            // AGREE screen: confirm
            vec![EventOption::new(
                EventChoiceMeta::new("[Confirm]"),
                EventOptionSemantics {
                    action: EventActionKind::Continue,
                    effects: serpent_reward_effects(run_state),
                    transition: EventOptionTransition::AdvanceScreen,
                    ..Default::default()
                },
            )]
        }
        _ => vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..Default::default()
            },
        )],
    }
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            match choice_idx {
                0 => {
                    // Agree: advance to confirm screen
                    event_state.current_screen = 1;
                }
                _ => {
                    // Native CUR_SCREEN::DISAGREE retains the final leave button.
                    event_state.current_screen = 2;
                }
            }
        }
        1 => {
            // Confirm: gain gold + receive curse
            let gold = gold_reward(run_state);
            run_state.change_gold_with_source(gold, DomainEventSource::Event(EventId::Ssssserpent));
            super::obtain_event_card(run_state, EventId::Ssssserpent, CardId::Doubt);
            event_state.current_screen = 3;
        }
        _ => {
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

#[cfg(test)]
mod tests {
    use super::handle_choice;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventId, EventOptionTransition, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn serpent_run(ascension: u8) -> RunState {
        let mut run_state = RunState::new(1, ascension, false, "Ironclad");
        run_state.gold = 0;
        run_state.event_state = Some(EventState::new(EventId::Ssssserpent));
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn options_expose_structured_agree_confirm_disagree_and_leave_semantics() {
        let run_state = serpent_run(0);
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Ssssserpent should expose structured event semantics");

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Accept);
        assert_eq!(
            options[0].semantics.effects,
            vec![
                EventEffect::GainGold(175),
                EventEffect::ObtainCurse {
                    count: 1,
                    kind: EventCardKind::Specific(CardId::Doubt),
                },
            ]
        );
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Decline);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut confirm_screen = EventState::new(EventId::Ssssserpent);
        confirm_screen.current_screen = 1;
        let confirm_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &confirm_screen,
            )
            .expect("Ssssserpent confirm screen should expose actual reward semantics");
        assert_eq!(confirm_options.len(), 1);
        assert_eq!(
            confirm_options[0].semantics.action,
            EventActionKind::Continue
        );
        assert_eq!(
            confirm_options[0].semantics.effects,
            vec![
                EventEffect::GainGold(175),
                EventEffect::ObtainCurse {
                    count: 1,
                    kind: EventCardKind::Specific(CardId::Doubt),
                },
            ]
        );

        let mut leave_screen = EventState::new(EventId::Ssssserpent);
        leave_screen.current_screen = 3;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &leave_screen,
            )
            .expect("Ssssserpent final screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        leave_screen.current_screen = 2;
        assert_eq!(
            super::get_options(&run_state, &leave_screen),
            leave_options,
            "native DISAGREE and COMPLETE both expose only the final leave button"
        );
    }

    #[test]
    fn agree_is_two_step_and_confirm_grants_java_gold_and_doubt() {
        let mut run_state = serpent_run(0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 0);
        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Doubt));
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 175);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 3);
        assert!(!run_state.event_state.as_ref().unwrap().completed);
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Doubt));
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: 175,
                source: DomainEventSource::Event(EventId::Ssssserpent),
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::Ssssserpent),
            } if card.id == CardId::Doubt
        )));
    }

    #[test]
    fn decline_keeps_native_disagree_page_until_leave_without_reward() {
        let mut run_state = serpent_run(20);
        let mut engine_state = EngineState::EventRoom;
        let deck = run_state.master_deck.clone();

        handle_choice(&mut engine_state, &mut run_state, 1);

        let event = run_state.event_state.as_ref().unwrap();
        assert_eq!(event.current_screen, 2);
        assert!(!event.completed);
        assert_eq!(run_state.gold, 0);
        assert_eq!(run_state.master_deck, deck);
        assert!(run_state.take_emitted_events().is_empty());

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state.event_state.as_ref().unwrap().completed);
        assert_eq!(run_state.gold, 0);
        assert_eq!(run_state.master_deck, deck);
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn ascension_15_uses_java_lower_gold_reward() {
        let mut run_state = serpent_run(15);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 150);
    }

    #[test]
    fn omamori_blocks_doubt_but_not_gold() {
        let mut run_state = serpent_run(0);
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 175);
        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Doubt));
        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori should remain after blocking the curse");
        assert_eq!(omamori.counter, 1);
    }

    #[test]
    fn confirm_gold_resolves_before_delayed_doubt_obtain_like_java_effect_list() {
        let mut run_state = serpent_run(0);
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 184);
        let labels = run_state
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::GoldChanged {
                    delta: 175,
                    source: DomainEventSource::Event(EventId::Ssssserpent),
                    ..
                } => Some("event_gold"),
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::Ssssserpent),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::Ssssserpent),
                } if card.id == CardId::Doubt => Some("doubt_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec!["event_gold", "ceramic_fish_gold", "doubt_obtained"],
            "Java queues ShowCardAndObtainEffect before RainingGoldEffect but gains gold immediately; actual card obtain resolves later"
        );
    }
}
