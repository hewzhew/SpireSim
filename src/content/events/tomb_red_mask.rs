use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionConstraint,
    EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let has_mask = run_state.relics.iter().any(|r| r.id == RelicId::RedMask);
            if has_mask {
                vec![
                    EventOption::new(
                        EventChoiceMeta::new("[Don the Mask] Gain 222 Gold."),
                        EventOptionSemantics {
                            action: EventActionKind::Gain,
                            effects: vec![EventEffect::GainGold(222)],
                            transition: EventOptionTransition::AdvanceScreen,
                            ..Default::default()
                        },
                    ),
                    EventOption::new(
                        EventChoiceMeta::new("[Leave]"),
                        EventOptionSemantics {
                            action: EventActionKind::Leave,
                            transition: EventOptionTransition::Complete,
                            terminal: true,
                            ..Default::default()
                        },
                    ),
                ]
            } else {
                vec![
                    EventOption::new(
                        EventChoiceMeta::disabled(
                            "[Don the Mask] Requires Red Mask.",
                            "No Red Mask",
                        ),
                        EventOptionSemantics {
                            action: EventActionKind::Gain,
                            effects: vec![EventEffect::GainGold(222)],
                            constraints: vec![EventOptionConstraint::RequiresRelic(
                                RelicId::RedMask,
                            )],
                            transition: EventOptionTransition::AdvanceScreen,
                            ..Default::default()
                        },
                    ),
                    EventOption::new(
                        EventChoiceMeta::new(format!(
                            "[Pay] Lose all ({}) Gold. Obtain Red Mask.",
                            run_state.gold
                        )),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![
                                EventEffect::LoseGold(run_state.gold),
                                EventEffect::ObtainRelic {
                                    count: 1,
                                    kind: EventRelicKind::Specific(RelicId::RedMask),
                                },
                            ],
                            transition: EventOptionTransition::AdvanceScreen,
                            ..Default::default()
                        },
                    ),
                    EventOption::new(
                        EventChoiceMeta::new("[Leave]"),
                        EventOptionSemantics {
                            action: EventActionKind::Leave,
                            transition: EventOptionTransition::Complete,
                            terminal: true,
                            ..Default::default()
                        },
                    ),
                ]
            }
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

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();
    let has_mask = run_state.relics.iter().any(|r| r.id == RelicId::RedMask);

    match event_state.current_screen {
        0 => {
            if has_mask {
                match choice_idx {
                    0 => {
                        // Don the Mask: +222 gold
                        run_state.change_gold_with_source(
                            222,
                            DomainEventSource::Event(EventId::TombRedMask),
                        );
                        event_state.current_screen = 1;
                    }
                    _ => {
                        event_state.completed = true;
                    }
                }
            } else {
                match choice_idx {
                    0 => {}
                    1 => {
                        // Pay all gold, get Red Mask
                        run_state.set_gold_with_source(
                            0,
                            DomainEventSource::Event(EventId::TombRedMask),
                        );
                        if let Some(next_state) = run_state.obtain_relic_with_source(
                            RelicId::RedMask,
                            EngineState::EventRoom,
                            DomainEventSource::Event(EventId::TombRedMask),
                        ) {
                            *engine_state = next_state;
                        }
                        event_state.current_screen = 1;
                    }
                    _ => {
                        event_state.completed = true;
                    }
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
    use super::{get_choices, handle_choice};
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionConstraint, EventOptionTransition,
        EventRelicKind, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    #[test]
    fn options_expose_structured_mask_pay_and_leave_semantics() {
        let mut no_mask = RunState::new(1, 0, false, "Ironclad");
        no_mask.gold = 123;
        no_mask.event_state = Some(EventState::new(EventId::TombRedMask));

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &no_mask,
            no_mask.event_state.as_ref().unwrap(),
        )
        .expect("Tomb Red Mask should expose structured event semantics");

        assert_eq!(options.len(), 3);
        assert!(options[0].ui.disabled);
        assert_eq!(options[0].semantics.action, EventActionKind::Gain);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::GainGold(222)]
        );
        assert!(options[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresRelic(RelicId::RedMask)));
        assert_eq!(options[1].semantics.action, EventActionKind::Trade);
        assert_eq!(
            options[1].semantics.effects,
            vec![
                EventEffect::LoseGold(123),
                EventEffect::ObtainRelic {
                    count: 1,
                    kind: EventRelicKind::Specific(RelicId::RedMask),
                },
            ]
        );
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[2].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[2].semantics.transition,
            EventOptionTransition::Complete
        );

        let mut has_mask = RunState::new(1, 0, false, "Ironclad");
        has_mask.gold = 10;
        has_mask.relics.push(RelicState::new(RelicId::RedMask));
        has_mask.event_state = Some(EventState::new(EventId::TombRedMask));
        let mask_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &has_mask,
                has_mask.event_state.as_ref().unwrap(),
            )
            .expect("Tomb Red Mask with Red Mask should expose gain-gold semantics");

        assert_eq!(mask_options.len(), 2);
        assert!(!mask_options[0].ui.disabled);
        assert_eq!(mask_options[0].semantics.action, EventActionKind::Gain);
        assert_eq!(
            mask_options[0].semantics.effects,
            vec![EventEffect::GainGold(222)]
        );
        assert_eq!(
            mask_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut result_screen = EventState::new(EventId::TombRedMask);
        result_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &has_mask,
                &result_screen,
            )
            .expect("Tomb Red Mask result screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn paying_without_mask_loses_all_gold_and_obtains_red_mask_with_event_source() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 123;
        run_state.event_state = Some(EventState::new(EventId::TombRedMask));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.gold, 0);
        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::RedMask));
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: -123,
                    new_total: 0,
                    source: DomainEventSource::Event(EventId::TombRedMask)
                }
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::RelicObtained {
                    relic_id: RelicId::RedMask,
                    source: DomainEventSource::Event(EventId::TombRedMask)
                }
            )
        }));
    }

    #[test]
    fn wearing_existing_mask_gains_222_gold_with_event_source() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 10;
        run_state.relics.push(RelicState::new(RelicId::RedMask));
        run_state.event_state = Some(EventState::new(EventId::TombRedMask));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 232);
        assert!(run_state.take_emitted_events().iter().any(|event| {
            matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: 222,
                    new_total: 232,
                    source: DomainEventSource::Event(EventId::TombRedMask)
                }
            )
        }));
    }

    #[test]
    fn choices_preserve_java_button_indices_when_mask_is_missing() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 17;
        let event_state = EventState::new(EventId::TombRedMask);

        let choices = get_choices(&run_state, &event_state);

        assert_eq!(choices.len(), 3);
        assert!(choices[0].disabled);
        assert!(!choices[1].disabled);
        assert!(!choices[2].disabled);
    }

    #[test]
    fn disabled_don_mask_without_red_mask_does_not_advance_or_grant_gold() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 17;
        run_state.event_state = Some(EventState::new(EventId::TombRedMask));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 17);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().is_empty());
    }
}
