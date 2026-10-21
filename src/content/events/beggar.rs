use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventSelectionKind,
    EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

const GOLD_COST: i32 = 75;
// Original CurScreen enum order: INTRO, LEAVE, GAVE_MONEY.
const GAVE_MONEY: usize = 2;
const LEAVE: usize = 1;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            // Donate 75g → purge a card, OR leave
            if run_state.gold >= GOLD_COST {
                vec![
                    EventOption::new(
                        EventChoiceMeta::new(format!(
                            "[Donate] Lose {} Gold. Remove a card.",
                            GOLD_COST
                        )),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![EventEffect::LoseGold(GOLD_COST)],
                            constraints: vec![EventOptionConstraint::RequiresGold(GOLD_COST)],
                            transition: EventOptionTransition::AdvanceScreen,
                            ..Default::default()
                        },
                    ),
                    EventOption::new(
                        EventChoiceMeta::new("[Leave]"),
                        EventOptionSemantics {
                            action: EventActionKind::Leave,
                            transition: EventOptionTransition::AdvanceScreen,
                            ..Default::default()
                        },
                    ),
                ]
            } else {
                vec![
                    EventOption::new(
                        EventChoiceMeta::disabled(
                            format!("[Donate] {} Gold.", GOLD_COST),
                            "Not enough Gold",
                        ),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![EventEffect::LoseGold(GOLD_COST)],
                            constraints: vec![EventOptionConstraint::RequiresGold(GOLD_COST)],
                            transition: EventOptionTransition::AdvanceScreen,
                            ..Default::default()
                        },
                    ),
                    EventOption::new(
                        EventChoiceMeta::new("[Leave]"),
                        EventOptionSemantics {
                            action: EventActionKind::Leave,
                            transition: EventOptionTransition::AdvanceScreen,
                            ..Default::default()
                        },
                    ),
                ]
            }
        }
        GAVE_MONEY => vec![EventOption::new(
            EventChoiceMeta::new("[Proceed] Remove a card."),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: None,
                    kind: EventCardKind::Unknown,
                }],
                constraints: vec![EventOptionConstraint::RequiresNonBottledPurgeableCard],
                transition: EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard),
                ..Default::default()
            },
        )],
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

    match event_state.current_screen {
        0 => match choice_idx {
            0 => {
                if run_state.gold >= GOLD_COST {
                    run_state.change_gold_with_source(
                        -GOLD_COST,
                        DomainEventSource::Event(EventId::Beggar),
                    );
                    event_state.current_screen = GAVE_MONEY;
                }
            }
            _ => {
                event_state.current_screen = LEAVE;
            }
        },
        GAVE_MONEY => {
            event_state.current_screen = LEAVE;
            run_state.event_state = Some(event_state);
            *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                min_choices: 1,
                max_choices: 1,
                reason: RunPendingChoiceReason::PurgeNonBottled,
                source: DomainEventSource::Event(EventId::Beggar),
                return_state: Box::new(EngineState::EventRoom),
            });
            return;
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
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason};
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventId, EventOptionConstraint,
        EventOptionTransition, EventSelectionKind, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn beggar_run(screen: usize) -> RunState {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.gold = 100;
        let mut event_state = EventState::new(EventId::Beggar);
        event_state.current_screen = screen;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        run_state
    }

    fn deck_card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    #[test]
    fn options_expose_structured_donate_proceed_and_leave_semantics() {
        let run_state = beggar_run(0);
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Beggar should expose structured event semantics");

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Trade);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::LoseGold(75)]
        );
        assert!(options[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresGold(75)));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut paid_screen = EventState::new(EventId::Beggar);
        paid_screen.current_screen = 2;
        let paid_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &paid_screen,
            )
            .expect("Beggar paid screen should expose remove-card selection semantics");
        assert_eq!(paid_options.len(), 1);
        assert_eq!(
            paid_options[0].semantics.action,
            EventActionKind::DeckOperation
        );
        assert_eq!(
            paid_options[0].semantics.effects,
            vec![EventEffect::RemoveCard {
                count: 1,
                target_lineage: None,
                kind: EventCardKind::Unknown,
            }]
        );
        assert!(paid_options[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresNonBottledPurgeableCard));
        assert_eq!(
            paid_options[0].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
        );

        let mut leave_screen = EventState::new(EventId::Beggar);
        leave_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &leave_screen,
            )
            .expect("Beggar leave screen should expose complete semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn leave_advances_to_cancel_result_screen_like_java() {
        let mut run_state = beggar_run(0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(
            run_state.event_state.as_ref().unwrap().current_screen,
            1,
            "Java Beggar shows a cancel result screen before opening the map"
        );
        assert!(!run_state.event_state.as_ref().unwrap().completed);
        assert!(matches!(engine_state, EngineState::EventRoom));
    }

    #[test]
    fn donate_pays_gold_before_opening_purge_prompt_like_java() {
        let mut run_state = beggar_run(0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 25);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 2);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: -75,
                new_total: 25,
                source: DomainEventSource::Event(EventId::Beggar)
            }
        )));
    }

    #[test]
    fn paid_continue_opens_non_bottled_purge_selection() {
        let mut run_state = beggar_run(2);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
        assert!(matches!(
            engine_state,
            EngineState::RunPendingChoice(ref pending)
                if pending.reason == RunPendingChoiceReason::PurgeNonBottled
        ));
    }

    #[test]
    fn paid_continue_selection_excludes_bottled_and_unpurgeable_cards_like_java() {
        let mut run_state = beggar_run(2);
        run_state.master_deck = vec![
            deck_card(CardId::Strike, 101),
            deck_card(CardId::Defend, 102),
            deck_card(CardId::AscendersBane, 103),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(101).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Beggar should open deck purge selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::PurgeNonBottled);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Purge);
        assert_eq!(
            request.targets,
            vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(102)
            )],
            "Java opens CardGroup.getGroupWithoutBottledCards(masterDeck.getPurgeableCards())"
        );
    }

    #[test]
    fn paid_continue_removes_selected_card_with_event_source() {
        let mut run_state = beggar_run(2);
        run_state.master_deck = vec![deck_card(CardId::Strike, 101)];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.master_deck.is_empty());
        assert!(
            run_state.event_state.as_ref().unwrap().completed,
            "Beggar.update opens the map immediately after purging, without another Leave click"
        );
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::Beggar),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )));
    }

    #[test]
    fn disabled_donate_does_not_pay_or_advance() {
        let mut run_state = beggar_run(0);
        run_state.gold = 50;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 50);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().is_empty());
    }
}
