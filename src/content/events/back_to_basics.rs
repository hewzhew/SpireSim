use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventSelectionKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn upgradeable_starter_basic_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| {
            crate::content::cards::is_starter_basic(card.id)
                && crate::state::core::master_deck_card_can_upgrade(card)
        })
        .count()
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            // Java INTRO offers only Elegance (purge) and Simplicity (upgrade).
            // Leave becomes available on COMPLETE, after either decision.
            let simplicity_transition =
                if crate::state::core::has_non_bottled_purgeable_master_deck_card(run_state) {
                    EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
                } else {
                    EventOptionTransition::AdvanceScreen
                };
            vec![
                EventOption::new(
                    EventChoiceMeta::new("[Elegance] Remove a card from your deck."),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![EventEffect::RemoveCard {
                            count: 1,
                            target_lineage: None,
                            kind: EventCardKind::Unknown,
                        }],
                        transition: simplicity_transition,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Simplicity] Upgrade all Strikes and Defends."),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![EventEffect::UpgradeCard {
                            count: upgradeable_starter_basic_count(run_state),
                        }],
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
            ]
        }
        // After purge returns
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
            match choice_idx {
                0 => {
                    // Purge a card: transition to RunPendingChoice::Purge
                    event_state.current_screen = 1;
                    if crate::state::core::has_non_bottled_purgeable_master_deck_card(run_state) {
                        run_state.event_state = Some(event_state);
                        *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                            selected: Vec::new(),
                            min_choices: 1,
                            max_choices: 1,
                            reason: RunPendingChoiceReason::PurgeNonBottled,
                            source: DomainEventSource::Event(EventId::BackTotheBasics),
                            return_state: Box::new(EngineState::EventRoom),
                        });
                        return;
                    }
                }
                1 => {
                    // Upgrade all Strikes and Defends
                    let source = DomainEventSource::Event(EventId::BackTotheBasics);
                    let upgrade_lineages: Vec<crate::runtime::combat::CardLineageId> = run_state
                        .master_deck
                        .iter()
                        .filter(|card| {
                            crate::content::cards::is_starter_basic(card.id)
                                && crate::state::core::master_deck_card_can_upgrade(card)
                        })
                        .map(|card| card.lineage)
                        .collect();
                    for lineage in upgrade_lineages {
                        run_state.upgrade_card_with_source(lineage, source);
                    }
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
    use super::{get_options, handle_choice};
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason};
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventSelectionKind,
        EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn card(id: CardId, uuid: u32, upgrades: u8) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = upgrades;
        card
    }

    #[test]
    fn structured_options_expose_dynamic_simplicity_and_basics_boundaries() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![
            card(CardId::Strike, 11, 0),
            card(CardId::Defend, 12, 1),
            card(CardId::Bash, 13, 0),
        ];
        run_state.event_state = Some(EventState::new(EventId::BackTotheBasics));

        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 2, "Java INTRO has no Leave option");
        assert_eq!(options[0].semantics.action, EventActionKind::DeckOperation);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::RemoveCard {
                count: 1,
                target_lineage: None,
                kind: crate::state::events::EventCardKind::Unknown,
            }));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
        );
        assert!(!options[0].ui.disabled);

        assert_eq!(options[1].semantics.action, EventActionKind::DeckOperation);
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::UpgradeCard { count: 1 }));
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        run_state.master_deck = vec![card(CardId::AscendersBane, 21, 0)];
        let no_purge_options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert_eq!(
            no_purge_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert!(!no_purge_options[0].ui.disabled);

        let mut complete = EventState::new(EventId::BackTotheBasics);
        complete.current_screen = 1;
        let complete_options = get_options(&run_state, &complete);
        assert_eq!(complete_options.len(), 1);
        assert_eq!(complete_options[0].semantics.action, EventActionKind::Leave);
        assert!(complete_options[0].semantics.terminal);
    }

    #[test]
    fn basics_upgrades_only_upgradeable_starter_strikes_and_defends() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![
            card(CardId::Strike, 11, 0),
            card(CardId::Defend, 12, 1),
            card(CardId::Bash, 13, 0),
            card(CardId::AscendersBane, 14, 0),
        ];
        run_state.event_state = Some(EventState::new(EventId::BackTotheBasics));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(
            run_state
                .master_deck
                .iter()
                .map(|card| (card.id, card.upgrades))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Strike, 1),
                (CardId::Defend, 1),
                (CardId::Bash, 0),
                (CardId::AscendersBane, 0),
            ]
        );
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                before,
                after,
                source: DomainEventSource::Event(EventId::BackTotheBasics),
            } if before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11) && before.upgrades == 0 && after.upgrades == 1
        )));
    }

    #[test]
    fn simplicity_without_purgeable_cards_advances_without_pending_like_java() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![card(CardId::AscendersBane, 11, 0)];
        run_state.event_state = Some(EventState::new(EventId::BackTotheBasics));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        let choices = super::get_choices(&run_state, run_state.event_state.as_ref().unwrap());
        assert!(
            !choices[0].disabled,
            "Java BackToBasics always exposes the Simplicity button"
        );

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn simplicity_selection_excludes_bottled_and_unpurgeable_cards_like_java() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![
            card(CardId::Strike, 101, 0),
            card(CardId::Defend, 102, 0),
            card(CardId::AscendersBane, 103, 0),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(101).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        run_state.event_state = Some(EventState::new(EventId::BackTotheBasics));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Back to Basics should open deck purge selection");
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
    fn simplicity_removes_selected_card_with_event_source() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![card(CardId::Strike, 101, 0)];
        run_state.event_state = Some(EventState::new(EventId::BackTotheBasics));
        run_state.emitted_events.clear();
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
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
        assert!(run_state.master_deck.is_empty());
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::BackTotheBasics),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )));
    }
}
