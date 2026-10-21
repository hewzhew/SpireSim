use crate::content::cards::CardId;
use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

const GOLD_COST: i32 = 85;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let mut choices = vec![];
            if run_state.gold >= GOLD_COST {
                choices.push(EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Pay] Lose {} Gold. Obtain a random Relic.",
                        GOLD_COST
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseGold(GOLD_COST),
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomRelic,
                            },
                        ],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                        ..Default::default()
                    },
                ));
            } else {
                choices.push(EventOption::new(
                    EventChoiceMeta::disabled(
                        format!("[Pay] Lose {} Gold. Obtain a random Relic.", GOLD_COST),
                        "Not enough Gold",
                    ),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseGold(GOLD_COST),
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomRelic,
                            },
                        ],
                        constraints: vec![
                            crate::state::events::EventOptionConstraint::RequiresGold(GOLD_COST),
                        ],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            }
            choices.push(EventOption::new(
                EventChoiceMeta::new("[Rob] Obtain a random Relic. Become Cursed - Shame."),
                EventOptionSemantics {
                    action: EventActionKind::Trade,
                    effects: vec![
                        EventEffect::ObtainRelic {
                            count: 1,
                            kind: EventRelicKind::RandomRelic,
                        },
                        EventEffect::ObtainCurse {
                            count: 1,
                            kind: EventCardKind::Specific(CardId::Shame),
                        },
                    ],
                    transition: EventOptionTransition::AdvanceScreen,
                    repeatable: false,
                    terminal: false,
                    ..Default::default()
                },
            ));
            choices.push(EventOption::new(
                EventChoiceMeta::new("[Leave]"),
                EventOptionSemantics {
                    action: EventActionKind::Leave,
                    transition: EventOptionTransition::Complete,
                    repeatable: false,
                    terminal: true,
                    ..Default::default()
                },
            ));
            choices
        }
        _ => vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                repeatable: false,
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
                    // Pay gold for relic
                    if run_state.gold >= GOLD_COST {
                        run_state.change_gold_with_source(
                            -GOLD_COST,
                            DomainEventSource::Event(EventId::Addict),
                        );
                        let relic_id = run_state.random_screenless_relic_reward();
                        if let Some(next_state) = run_state.obtain_relic_with_source(
                            relic_id,
                            EngineState::EventRoom,
                            DomainEventSource::Event(EventId::Addict),
                        ) {
                            *_engine_state = next_state;
                        }
                        event_state.current_screen = 1;
                    }
                }
                1 => {
                    // Rob: relic + Shame curse
                    // Java constructs ShowCardAndObtainEffect(Shame) before
                    // spawnRelicAndObtain(relic). Omamori interception happens
                    // in that constructor, so a newly obtained Omamori must not
                    // block the Shame. Other onObtainCard hooks still see the
                    // newly obtained relic when the effect resolves later.
                    let omamori_snapshot = run_state
                        .relics
                        .iter()
                        .find(|relic| relic.id == RelicId::Omamori)
                        .map(|relic| relic.counter);
                    let relic_id = run_state.random_screenless_relic_reward();
                    if let Some(next_state) = run_state.obtain_relic_with_source(
                        relic_id,
                        EngineState::EventRoom,
                        DomainEventSource::Event(EventId::Addict),
                    ) {
                        *_engine_state = next_state;
                    }
                    let source = DomainEventSource::Event(EventId::Addict);
                    run_state.add_card_to_deck_with_omamori_snapshot_from(
                        CardId::Shame,
                        0,
                        source,
                        omamori_snapshot.is_some(),
                        omamori_snapshot.unwrap_or(0),
                    );
                    event_state.current_screen = 1;
                }
                _ => {
                    event_state.completed = true;
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
    use super::*;
    use crate::content::relics::RelicState;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventOptionTransition, EventRelicKind,
    };
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn addict_run_for_rob(relic_id: RelicId) -> RunState {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.current_hp = 80;
        run_state.max_hp = 80;
        run_state.common_relic_pool = vec![relic_id];
        run_state.uncommon_relic_pool = vec![relic_id];
        run_state.rare_relic_pool = vec![relic_id];
        run_state.event_state = Some(EventState {
            id: EventId::Addict,
            current_screen: 0,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn disabled_pay_does_not_advance_or_obtain_relic() {
        let mut run_state = addict_run_for_rob(RelicId::Anchor);
        run_state.gold = 84;
        let mut engine_state = EngineState::EventRoom;

        let choices = get_choices(&run_state, run_state.event_state.as_ref().unwrap());
        assert!(choices[0].disabled);

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 84);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(!run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::Anchor));
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn options_expose_structured_pay_rob_and_leave_semantics() {
        let mut run_state = addict_run_for_rob(RelicId::Anchor);
        run_state.gold = GOLD_COST;
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Addict should expose structured event semantics");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].semantics.action, EventActionKind::Trade);
        assert_eq!(
            options[0].semantics.effects,
            vec![
                EventEffect::LoseGold(GOLD_COST),
                EventEffect::ObtainRelic {
                    count: 1,
                    kind: EventRelicKind::RandomRelic,
                },
            ]
        );
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Trade);
        assert_eq!(
            options[1].semantics.effects,
            vec![
                EventEffect::ObtainRelic {
                    count: 1,
                    kind: EventRelicKind::RandomRelic,
                },
                EventEffect::ObtainCurse {
                    count: 1,
                    kind: EventCardKind::Specific(CardId::Shame),
                },
            ]
        );
        assert_eq!(
            options[2].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn rob_new_omamori_does_not_block_shame_from_same_choice() {
        let mut run_state = addict_run_for_rob(RelicId::Omamori);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("rob should obtain Omamori from the forced relic pool");
        assert_eq!(
            omamori.counter, 2,
            "Java checks Omamori before the stolen relic is obtained"
        );
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Shame));
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::Omamori,
                source: DomainEventSource::Event(EventId::Addict),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::Addict),
            } if card.id == CardId::Shame
        )));
    }

    #[test]
    fn rob_existing_omamori_still_blocks_shame_before_stolen_relic_resolves() {
        let mut run_state = addict_run_for_rob(RelicId::DarkstonePeriapt);
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("existing Omamori should remain");
        assert_eq!(omamori.counter, 1);
        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Shame));
        assert_eq!(
            run_state.max_hp, 80,
            "blocked curse should not trigger newly obtained Darkstone Periapt"
        );
    }

    #[test]
    fn rob_new_darkstone_still_triggers_on_shame_after_relic_obtain() {
        let mut run_state = addict_run_for_rob(RelicId::DarkstonePeriapt);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Shame));
        assert_eq!(
            run_state.max_hp, 86,
            "ShowCardAndObtainEffect obtains the card after spawnRelicAndObtain"
        );
    }

    #[test]
    fn rob_new_ceramic_fish_triggers_before_shame_obtained_event() {
        let mut run_state = addict_run_for_rob(RelicId::CeramicFish);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let events = run_state.take_emitted_events();
        let relic_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::RelicObtained {
                        relic_id: RelicId::CeramicFish,
                        source: DomainEventSource::Event(EventId::Addict),
                    }
                )
            })
            .expect("Rob should obtain the forced relic before the delayed curse resolves");
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::Addict),
                        ..
                    }
                )
            })
            .expect("New Ceramic Fish should see the delayed Shame obtain hook");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::Addict),
                    } if card.id == CardId::Shame
                )
            })
            .expect("Rob should obtain Shame through the delayed ShowCardAndObtainEffect");

        assert!(
            relic_pos < fish_gold_pos && fish_gold_pos < obtained_pos,
            "Java Addict constructs the curse effect before spawnRelicAndObtain, but the effect resolves after the new relic is owned and runs onObtainCard before Soul.obtain"
        );
    }
}
