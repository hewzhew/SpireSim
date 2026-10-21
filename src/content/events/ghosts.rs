use crate::content::cards::CardId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let hp_loss = ((run_state.max_hp as f32) * 0.5).ceil() as i32;
            let count = if run_state.ascension_level >= 15 {
                3
            } else {
                5
            };
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Accept] Lose {} Max HP. Obtain {} Apparitions.",
                        hp_loss, count
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Accept,
                        effects: vec![
                            EventEffect::LoseMaxHp(hp_loss),
                            EventEffect::ObtainCard {
                                count: count as usize,
                                kind: EventCardKind::Specific(CardId::Apparition),
                            },
                        ],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Refuse]"),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        effects: vec![],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ),
            ]
        }
        _ => vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                effects: vec![],
                constraints: vec![],
                transition: EventOptionTransition::Complete,
                repeatable: false,
                terminal: true,
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
                    // Accept: lose 50% max HP, gain Apparitions
                    let mut hp_loss = ((run_state.max_hp as f32) * 0.5).ceil() as i32;
                    if hp_loss >= run_state.max_hp {
                        hp_loss = run_state.max_hp - 1;
                    }
                    run_state.lose_max_hp_with_source(
                        hp_loss,
                        DomainEventSource::Event(EventId::Ghosts),
                    );
                    let count = if run_state.ascension_level >= 15 {
                        3
                    } else {
                        5
                    };
                    for _ in 0..count {
                        super::obtain_event_card(run_state, EventId::Ghosts, CardId::Apparition);
                    }
                    event_state.current_screen = 1;
                }
                _ => {
                    event_state.current_screen = 2;
                }
            }
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
    use crate::state::events::{EventId, EventState};
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    #[test]
    fn accept_and_refuse_keep_distinct_original_result_pages() {
        for (choice, result_screen) in [(0, 1), (1, 2)] {
            let mut run_state = RunState::new(1, 20, false, "Defect");
            run_state.event_state = Some(EventState::new(EventId::Ghosts));
            let mut engine_state = EngineState::EventRoom;
            handle_choice(&mut engine_state, &mut run_state, choice);
            let event = run_state.event_state.as_ref().unwrap();
            assert_eq!(event.current_screen, result_screen);
            assert!(!event.completed);
            let options = super::get_options(&run_state, event);
            assert_eq!(options.len(), 1);
            assert!(options[0].semantics.terminal);
            handle_choice(&mut engine_state, &mut run_state, 0);
            assert!(run_state.event_state.as_ref().unwrap().completed);
        }
    }

    #[test]
    fn accept_loses_max_hp_and_obtains_apparitions_with_event_source() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 70;
        run_state.max_hp = 80;
        run_state.event_state = Some(EventState::new(EventId::Ghosts));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.max_hp, 40);
        assert_eq!(run_state.current_hp, 40);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::MaxHpChanged {
                delta: -40,
                current_hp: 40,
                max_hp: 40,
                source: DomainEventSource::Event(EventId::Ghosts),
            }
        )));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::Ghosts),
                    } if card.id == CardId::Apparition
                ))
                .count(),
            5
        );
    }

    #[test]
    fn accept_on_ascension_fifteen_obtains_three_apparitions() {
        let mut run_state = RunState::new(1, 15, false, "Ironclad");
        run_state.current_hp = 70;
        run_state.max_hp = 80;
        run_state.event_state = Some(EventState::new(EventId::Ghosts));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(
            run_state
                .take_emitted_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::Ghosts),
                    } if card.id == CardId::Apparition
                ))
                .count(),
            3
        );
    }

    #[test]
    fn accept_max_hp_loss_resolves_before_delayed_apparition_obtains() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 70;
        run_state.max_hp = 80;
        run_state.gold = 0;
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        run_state.event_state = Some(EventState::new(EventId::Ghosts));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.max_hp, 40);
        assert_eq!(run_state.current_hp, 40);
        assert_eq!(run_state.gold, 45);
        let labels = run_state
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::MaxHpChanged {
                    delta: -40,
                    source: DomainEventSource::Event(EventId::Ghosts),
                    ..
                } => Some("max_hp_loss"),
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::Ghosts),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::Ghosts),
                } if card.id == CardId::Apparition => Some("apparition_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "max_hp_loss",
                "ceramic_fish_gold",
                "apparition_obtained",
                "ceramic_fish_gold",
                "apparition_obtained",
                "ceramic_fish_gold",
                "apparition_obtained",
                "ceramic_fish_gold",
                "apparition_obtained",
                "ceramic_fish_gold",
                "apparition_obtained",
            ],
            "Java decreases max HP immediately, then each queued Apparition ShowCardAndObtainEffect later runs onObtainCard before Soul.obtain"
        );
    }
}
