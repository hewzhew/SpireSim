use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventSelectionKind,
    EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

/// NoteForYourself event.
/// Java: playerPref stores a card across runs. Default: Iron Wave.
///   [Take] Obtain the stored card → GridSelect 1 card to remove (store for next run)
///   [Ignore] Do nothing
///
/// Since cross-run persistence is not supported, the obtained card is always Iron Wave.
/// The removal step is still important: player removes 1 card from deck (affects current run).
///
/// Screen 0: [Proceed]
/// Screen 1: [Take Card] / [Ignore]
pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![EventOption::new(
            EventChoiceMeta::new("[Proceed]"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        )],
        1 => {
            let def = crate::content::cards::get_card_definition(run_state.note_for_yourself_card);
            let upgrade_suffix = if run_state.note_for_yourself_upgrades > 0 {
                "+"
            } else {
                ""
            };
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Take Card] Obtain {}{}. Remove a card.",
                        def.name, upgrade_suffix
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![
                            EventEffect::ObtainCard {
                                count: 1,
                                kind: EventCardKind::Specific(run_state.note_for_yourself_card),
                            },
                            EventEffect::RemoveCard {
                                count: 1,
                                target_lineage: None,
                                kind: EventCardKind::Unknown,
                            },
                        ],
                        constraints: vec![EventOptionConstraint::RequiresNonBottledPurgeableCard],
                        transition: EventOptionTransition::OpenSelection(
                            EventSelectionKind::RemoveCard,
                        ),
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Ignore]"),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
            ]
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

    match event_state.current_screen {
        0 => {
            event_state.current_screen = 1;
        }
        1 => {
            match choice_idx {
                0 => {
                    // Take: obtain the profile note card, then pick 1 card to save.
                    // Java manually calls relic onObtainCard, adds to masterDeck, then opens
                    // CardGroup.getGroupWithoutBottledCards(masterDeck.getPurgeableCards()).
                    run_state.add_card_to_deck_without_interception_from(
                        run_state.note_for_yourself_card,
                        run_state.note_for_yourself_upgrades,
                        DomainEventSource::Event(EventId::NoteForYourself),
                    );
                    event_state.current_screen = 2;
                    run_state.event_state = Some(event_state);
                    *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                        selected: Vec::new(),
                        reason: RunPendingChoiceReason::PurgeNonBottled,
                        source: DomainEventSource::Event(EventId::NoteForYourself),
                        min_choices: 1,
                        max_choices: 1,
                        return_state: Box::new(EngineState::EventRoom),
                    });
                    return;
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
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::ClientInput;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventOptionConstraint, EventOptionTransition,
        EventSelectionKind,
    };
    use crate::state::selection::{
        DomainEvent, SelectionReason, SelectionResolution, SelectionScope, SelectionTargetRef,
    };

    #[test]
    fn structured_take_exposes_specific_note_card_and_remove_selection_boundary() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.note_for_yourself_card = CardId::Bash;
        let mut event_state = EventState::new(EventId::NoteForYourself);
        event_state.current_screen = 1;
        rs.event_state = Some(event_state);

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &rs,
            rs.event_state.as_ref().unwrap(),
        )
        .expect("NoteForYourself should expose structured event options");

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::DeckOperation);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainCard {
                count: 1,
                kind: EventCardKind::Specific(CardId::Bash),
            }));
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::RemoveCard {
                count: 1,
                target_lineage: None,
                kind: EventCardKind::Unknown,
            }));
        assert!(options[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresNonBottledPurgeableCard));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Decline);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
    }

    #[test]
    fn structured_intro_and_complete_boundaries() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let intro = EventState::new(EventId::NoteForYourself);
        let intro_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(&rs, &intro)
                .expect("NoteForYourself intro should expose structured event options");

        assert_eq!(intro_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            intro_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut complete = EventState::new(EventId::NoteForYourself);
        complete.current_screen = 2;
        let complete_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &rs, &complete,
            )
            .expect("NoteForYourself complete screen should expose structured event options");

        assert_eq!(complete_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            complete_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        assert!(complete_options[0].semantics.terminal);
    }

    #[test]
    fn take_uses_profile_note_card_and_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.note_for_yourself_card = CardId::Bash;
        rs.note_for_yourself_upgrades = 1;
        rs.event_state = Some(EventState {
            id: EventId::NoteForYourself,
            current_screen: 1,
            completed: false,
            combat_pending: false,
            internal_state: 0,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        let obtained = rs.master_deck.last().unwrap();
        assert_eq!(obtained.id, CardId::Bash);
        assert_eq!(obtained.upgrades, 1);
        assert!(matches!(
            engine_state,
            EngineState::RunPendingChoice(RunPendingChoiceState {
                reason: RunPendingChoiceReason::PurgeNonBottled,
                source: DomainEventSource::Event(EventId::NoteForYourself),
                ..
            })
        ));
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::NoteForYourself),
            } if card.id == CardId::Bash && card.upgrades == 1
        )));
    }

    #[test]
    fn take_manual_obtain_is_not_blocked_by_omamori() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.note_for_yourself_card = CardId::Regret;
        rs.relics.push(RelicState::new(RelicId::Omamori));
        rs.event_state = Some(EventState {
            id: EventId::NoteForYourself,
            current_screen: 1,
            completed: false,
            combat_pending: false,
            internal_state: 0,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        assert!(rs.master_deck.iter().any(|card| card.id == CardId::Regret));
        let omamori = rs.relics.iter().find(|r| r.id == RelicId::Omamori).unwrap();
        assert_eq!(omamori.counter, 2);
    }

    #[test]
    fn take_manual_obtain_runs_relic_hooks_before_card_obtained_event() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.note_for_yourself_card = CardId::Bash;
        rs.relics.push(RelicState::new(RelicId::CeramicFish));
        rs.event_state = Some(EventState {
            id: EventId::NoteForYourself,
            current_screen: 1,
            completed: false,
            combat_pending: false,
            internal_state: 0,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        let events = rs.take_emitted_events();
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::NoteForYourself),
                        ..
                    }
                )
            })
            .expect("NoteForYourself should manually run relic onObtainCard");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::NoteForYourself),
                    } if card.id == CardId::Bash
                )
            })
            .expect("NoteForYourself should add the stored note card to the master deck");

        assert!(
            fish_gold_pos < obtained_pos,
            "Java NoteForYourself manually calls relic onObtainCard before masterDeck.addToTop"
        );
    }

    #[test]
    fn take_manual_obtain_applies_egg_upgrade_to_note_card() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.note_for_yourself_card = CardId::Strike;
        rs.relics.push(RelicState::new(RelicId::MoltenEgg));
        rs.event_state = Some(EventState {
            id: EventId::NoteForYourself,
            current_screen: 1,
            completed: false,
            combat_pending: false,
            internal_state: 0,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        let obtained = rs
            .master_deck
            .last()
            .expect("NoteForYourself should add the note card to the top of master deck");
        assert_eq!(obtained.id, CardId::Strike);
        assert_eq!(
            obtained.upgrades, 1,
            "Java NoteForYourself calls relic onObtainCard before addToTop, so Molten Egg upgrades the stored Attack"
        );
    }

    #[test]
    fn take_selection_excludes_bottled_and_unpurgeable_cards_after_obtaining_note_card() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 101),
            CombatCard::for_scenario(CardId::Defend, 102),
            CombatCard::for_scenario(CardId::AscendersBane, 103),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(101).to_runtime_i32_payload();
        rs.relics.push(bottle);
        rs.note_for_yourself_card = CardId::Bash;
        rs.event_state = Some(EventState {
            id: EventId::NoteForYourself,
            current_screen: 1,
            completed: false,
            combat_pending: false,
            internal_state: 0,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);

        let obtained_uuid = rs
            .master_deck
            .last()
            .expect("Note card should be added before selection opens")
            .lineage;
        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Taking the note card should open deck purge selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::PurgeNonBottled);
        let request = choice.selection_request(&rs);
        assert_eq!(request.reason, SelectionReason::Purge);
        assert_eq!(
            request.targets,
            vec![
                SelectionTargetRef::CardLineage(crate::runtime::combat::CardLineageId::constructed_scenario(102)),
                SelectionTargetRef::CardLineage(obtained_uuid),
            ],
            "Java adds the note card, then opens CardGroup.getGroupWithoutBottledCards(masterDeck.getPurgeableCards())"
        );
    }

    #[test]
    fn selected_saved_card_updates_note_profile_before_removal() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::ShrugItOff, 12),
        ];
        rs.master_deck[1].upgrades = 1;
        rs.event_state = Some(EventState {
            id: EventId::NoteForYourself,
            current_screen: 2,
            completed: false,
            combat_pending: false,
            internal_state: 0,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        let mut engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            reason: RunPendingChoiceReason::PurgeNonBottled,
            source: DomainEventSource::Event(EventId::NoteForYourself),
            min_choices: 1,
            max_choices: 1,
            return_state: Box::new(EngineState::EventRoom),
        });
        let mut combat_state = None;

        assert!(tick_run(
            &mut engine_state,
            &mut rs,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(12)
                )],
            })),
        ));

        assert_eq!(rs.note_for_yourself_card, CardId::ShrugItOff);
        assert_eq!(rs.note_for_yourself_upgrades, 1);
        assert!(!rs
            .master_deck
            .iter()
            .any(|card| card.lineage
                == crate::runtime::combat::CardLineageId::constructed_scenario(12)));
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::NoteForYourself),
            } if card.id == CardId::ShrugItOff && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12)
        )));
    }
}
