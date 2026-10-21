// Java: DrugDealer (city) — "Drug Dealer"
// Screen 0:
//   [0] Obtain J.A.X. card
//   [1] Transform 2 cards (requires ≥2 purgeable) — grid-select
//   [2] Obtain MutagenicStrength relic (Circlet if already owned)
// Screen 1: [Leave]

use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventRelicKind,
    EventSelectionKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn purgeable_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| crate::state::core::master_deck_card_is_purgeable(card))
        .count()
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    if event_state.current_screen == 1 {
        return vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..Default::default()
            },
        )];
    }

    let mut choices = vec![EventOption::new(
        EventChoiceMeta::new("[Ingest Mutagens] Obtain J.A.X."),
        EventOptionSemantics {
            action: EventActionKind::Gain,
            effects: vec![EventEffect::ObtainCard {
                count: 1,
                kind: EventCardKind::Specific(crate::content::cards::CardId::JAX),
            }],
            transition: EventOptionTransition::AdvanceScreen,
            ..Default::default()
        },
    )];

    if purgeable_count(run_state) >= 2 {
        choices.push(EventOption::new(
            EventChoiceMeta::new("[Become a Test Subject] Transform 2 cards."),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::TransformCard { count: 2 }],
                constraints: vec![EventOptionConstraint::RequiresTransformableCards(2)],
                transition: EventOptionTransition::OpenSelection(EventSelectionKind::TransformCard),
                ..Default::default()
            },
        ));
    } else {
        choices.push(EventOption::new(
            EventChoiceMeta::disabled(
                "[Become a Test Subject] Transform 2 cards.",
                "Not enough purgeable cards",
            ),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::TransformCard { count: 2 }],
                constraints: vec![EventOptionConstraint::RequiresTransformableCards(2)],
                transition: EventOptionTransition::OpenSelection(EventSelectionKind::TransformCard),
                ..Default::default()
            },
        ));
    }

    let relic_id = if run_state
        .relics
        .iter()
        .any(|r| r.id == crate::content::relics::RelicId::MutagenicStrength)
    {
        crate::content::relics::RelicId::Circlet
    } else {
        crate::content::relics::RelicId::MutagenicStrength
    };
    choices.push(EventOption::new(
        EventChoiceMeta::new("[Inject Mutagens] Obtain Mutagenic Strength relic."),
        EventOptionSemantics {
            action: EventActionKind::Gain,
            effects: vec![EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(relic_id),
            }],
            transition: EventOptionTransition::AdvanceScreen,
            ..Default::default()
        },
    ));
    choices
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    if event_state.completed {
        run_state.event_state = Some(event_state);
        return;
    }

    match event_state.current_screen {
        0 => {
            match choice_idx {
                0 => {
                    // Obtain J.A.X.
                    super::obtain_event_card(
                        run_state,
                        EventId::DrugDealer,
                        crate::content::cards::CardId::JAX,
                    );
                    event_state.current_screen = 1;
                }
                1 => {
                    // Transform 2 cards (Java: gridSelectScreen.open(getPurgeableCards(), 2, ...))
                    if purgeable_count(run_state) >= 2 {
                        *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                            selected: Vec::new(),
                            min_choices: 2,
                            max_choices: 2,
                            reason: RunPendingChoiceReason::Transform,
                            source: DomainEventSource::Event(EventId::DrugDealer),
                            return_state: Box::new(EngineState::EventRoom),
                        });
                        event_state.current_screen = 1;
                    }
                }
                2 => {
                    // Obtain MutagenicStrength relic
                    let relic_id = if run_state
                        .relics
                        .iter()
                        .any(|r| r.id == crate::content::relics::RelicId::MutagenicStrength)
                    {
                        crate::content::relics::RelicId::Circlet
                    } else {
                        crate::content::relics::RelicId::MutagenicStrength
                    };
                    let _ = run_state.obtain_relic_with_source(
                        relic_id,
                        EngineState::EventRoom,
                        DomainEventSource::Event(EventId::DrugDealer),
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
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::ClientInput;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventOptionConstraint, EventOptionTransition,
        EventRelicKind, EventSelectionKind,
    };
    use crate::state::selection::{
        DomainEvent, SelectionReason, SelectionResolution, SelectionScope, SelectionTargetRef,
    };

    fn drug_dealer_run() -> RunState {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.event_state = Some(EventState {
            id: EventId::DrugDealer,
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

    fn deck_card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    #[test]
    fn options_expose_structured_jax_transform_relic_and_leave_semantics() {
        let mut run_state = drug_dealer_run();
        run_state.master_deck = vec![
            deck_card(CardId::Strike, 101),
            deck_card(CardId::Defend, 102),
        ];
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Drug Dealer should expose structured event semantics");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].semantics.action, EventActionKind::Gain);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::ObtainCard {
                count: 1,
                kind: EventCardKind::Specific(CardId::JAX),
            }]
        );
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::TransformCard)
        );
        assert_eq!(
            options[1].semantics.effects,
            vec![EventEffect::TransformCard { count: 2 }]
        );
        assert!(options[1]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresTransformableCards(2)));
        assert_eq!(
            options[2].semantics.effects,
            vec![EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::MutagenicStrength),
            }]
        );

        let mut result_screen = EventState::new(EventId::DrugDealer);
        result_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &result_screen,
            )
            .expect("Drug Dealer result screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn ingest_mutagens_obtains_jax_with_event_source() {
        let mut run_state = drug_dealer_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::JAX));
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::DrugDealer),
            } if card.id == CardId::JAX
        )));
    }

    #[test]
    fn inject_mutagens_obtains_relic_with_event_source() {
        let mut run_state = drug_dealer_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::MutagenicStrength));
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::MutagenicStrength,
                source: DomainEventSource::Event(EventId::DrugDealer),
            }
        )));
    }

    #[test]
    fn inject_mutagens_grants_circlet_through_obtain_pipeline_when_already_owned() {
        let mut run_state = drug_dealer_run();
        run_state
            .relics
            .push(RelicState::new(RelicId::MutagenicStrength));
        run_state.relics.push(RelicState::new(RelicId::Circlet));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        let circlet = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Circlet)
            .expect("existing Circlet should remain");
        assert_eq!(circlet.counter, 2);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::Circlet,
                source: DomainEventSource::Event(EventId::DrugDealer),
            }
        )));
    }

    #[test]
    fn disabled_test_subject_does_not_open_transform_selection_with_too_few_purgeable_cards() {
        let mut run_state = drug_dealer_run();
        run_state.master_deck = vec![crate::runtime::combat::CombatCard::for_scenario(
            CardId::AscendersBane,
            11,
        )];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(
            run_state.event_state.as_ref().unwrap().current_screen,
            0,
            "disabled Java option should not advance the event state"
        );
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn test_subject_transform_selection_uses_purgeable_cards_including_bottled_like_java() {
        let mut run_state = drug_dealer_run();
        run_state.master_deck = vec![
            deck_card(CardId::Strike, 101),
            deck_card(CardId::Defend, 102),
            deck_card(CardId::AscendersBane, 103),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(101).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Drug Dealer test subject should open transform selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::Transform);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Transform);
        assert_eq!(
            request.targets,
            vec![
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(101)
                ),
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(102)
                ),
            ],
            "Java opens masterDeck.getPurgeableCards(), not getGroupWithoutBottledCards"
        );
    }

    #[test]
    fn test_subject_transforms_two_cards_with_event_source() {
        let mut run_state = drug_dealer_run();
        run_state.master_deck = vec![
            deck_card(CardId::Strike, 101),
            deck_card(CardId::Defend, 102),
        ];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(101)
                    ),
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(102)
                    ),
                ],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(run_state.master_deck.len(), 2);
        let events = run_state.take_emitted_events();
        let transformed_before_ids = events
            .iter()
            .filter_map(|event| match event {
                DomainEvent::CardTransformed { before, .. } => Some(before.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            transformed_before_ids,
            vec![CardId::Strike, CardId::Defend],
            "Java DrugDealer iterates gridSelectScreen.selectedCards in selected order"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardTransformed {
                before,
                source: DomainEventSource::Event(EventId::DrugDealer),
                ..
            } if before.id == CardId::Strike && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardTransformed {
                before,
                source: DomainEventSource::Event(EventId::DrugDealer),
                ..
            } if before.id == CardId::Defend && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(102)
        )));
    }

    #[test]
    fn test_subject_transforms_each_selected_card_sequentially_like_java() {
        let mut run_state = drug_dealer_run();
        run_state.master_deck = vec![
            deck_card(CardId::Parasite, 101),
            deck_card(CardId::Parasite, 102),
            deck_card(CardId::Strike, 103),
        ];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(101)
                    ),
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(102)
                    ),
                ],
            })),
        ));

        let events = run_state.take_emitted_events();
        let relevant = events
            .iter()
            .filter_map(|event| match event {
                DomainEvent::MaxHpChanged {
                    delta: -3,
                    source: DomainEventSource::Event(EventId::DrugDealer),
                    ..
                } => Some("remove_parasite"),
                DomainEvent::CardTransformed {
                    before,
                    source: DomainEventSource::Event(EventId::DrugDealer),
                    ..
                } if before.id == CardId::Parasite => Some("obtain_replacement"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            relevant,
            vec![
                "remove_parasite",
                "remove_parasite",
                "obtain_replacement",
                "obtain_replacement"
            ],
            "Java DrugDealer queues ShowCardAndObtainEffect for each transformed card; actual obtains resolve after both selected cards have been removed"
        );
    }
}
