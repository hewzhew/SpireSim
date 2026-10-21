use crate::content::cards::{get_card_definition, CardType};
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn is_fountain_removable_curse(
    card: &crate::runtime::combat::CombatCard,
    run_state: &RunState,
) -> bool {
    let def = get_card_definition(card.id);
    def.card_type == CardType::Curse
        && crate::state::core::master_deck_card_is_purgeable(card)
        && !crate::state::core::master_deck_card_is_bottled(card, &run_state.relics)
}

/// Counts curses the Fountain event can legally remove from the master deck.
pub fn removable_curse_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| is_fountain_removable_curse(card, run_state))
        .count()
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![
            EventOption::new(
                EventChoiceMeta::new("[Drink] Remove all removable Curses."),
                EventOptionSemantics {
                    action: EventActionKind::DeckOperation,
                    effects: vec![EventEffect::RemoveCard {
                        count: removable_curse_count(run_state),
                        target_lineage: None,
                        kind: EventCardKind::Unknown,
                    }],
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
        ],
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
                    // Remove all removable curses
                    let curses_to_remove: Vec<crate::runtime::combat::CardLineageId> = run_state
                        .master_deck
                        .iter()
                        .rev()
                        .filter(|card| is_fountain_removable_curse(card, run_state))
                        .map(|card| card.lineage)
                        .collect();

                    let source = DomainEventSource::Event(EventId::FountainOfCurseCleansing);
                    for lineage in curses_to_remove {
                        run_state.remove_card_from_deck_with_source(lineage, source);
                    }
                    event_state.current_screen = 1;
                }
                _ => {
                    event_state.current_screen = 1;
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
    use super::{get_choices, get_options, handle_choice};
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventId, EventOptionTransition, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    #[test]
    fn structured_options_expose_clickable_curse_removal_without_disabling_empty_decks() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Injury, 11),
            CombatCard::for_scenario(CardId::Parasite, 12),
            CombatCard::for_scenario(CardId::AscendersBane, 13),
            CombatCard::for_scenario(CardId::Strike, 14),
        ];
        run_state.event_state = Some(EventState::new(EventId::FountainOfCurseCleansing));

        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::DeckOperation);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::RemoveCard {
                count: 2,
                target_lineage: None,
                kind: EventCardKind::Unknown,
            }));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert!(!options[0].ui.disabled);
        assert_eq!(options[1].semantics.action, EventActionKind::Leave);

        run_state.master_deck = vec![CombatCard::for_scenario(CardId::Strike, 21)];
        let empty_options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert!(empty_options[0]
            .semantics
            .effects
            .contains(&EventEffect::RemoveCard {
                count: 0,
                target_lineage: None,
                kind: EventCardKind::Unknown,
            }));
        assert!(!empty_options[0].ui.disabled);
    }

    #[test]
    fn fountain_removes_only_non_bottled_removable_curses_with_event_source() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Injury, 11),
            CombatCard::for_scenario(CardId::Parasite, 12),
            CombatCard::for_scenario(CardId::Doubt, 13),
            CombatCard::for_scenario(CardId::Pain, 14),
            CombatCard::for_scenario(CardId::AscendersBane, 15),
            CombatCard::for_scenario(CardId::CurseOfTheBell, 16),
            CombatCard::for_scenario(CardId::Necronomicurse, 17),
            CombatCard::for_scenario(CardId::Strike, 18),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(14).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        run_state.current_hp = 80;
        run_state.max_hp = 80;
        run_state.event_state = Some(EventState::new(EventId::FountainOfCurseCleansing));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(
            run_state
                .master_deck
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Pain, crate::test_support::scenario_lineage(14)),
                (
                    CardId::AscendersBane,
                    crate::test_support::scenario_lineage(15)
                ),
                (
                    CardId::CurseOfTheBell,
                    crate::test_support::scenario_lineage(16)
                ),
                (
                    CardId::Necronomicurse,
                    crate::test_support::scenario_lineage(17)
                ),
                (CardId::Strike, crate::test_support::scenario_lineage(18)),
            ]
        );
        assert_eq!(
            run_state.max_hp, 77,
            "Java CardGroup.removeCard runs Parasite.onRemoveFromMasterDeck"
        );
        assert_eq!(run_state.current_hp, 77);

        let events = run_state.take_emitted_events();
        let removed: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                DomainEvent::CardRemoved {
                    card,
                    source: DomainEventSource::Event(EventId::FountainOfCurseCleansing),
                } => Some((card.id, card.lineage)),
                _ => None,
            })
            .collect();
        assert_eq!(
            removed,
            vec![
                (CardId::Doubt, crate::test_support::scenario_lineage(13)),
                (CardId::Parasite, crate::test_support::scenario_lineage(12)),
                (CardId::Injury, crate::test_support::scenario_lineage(11)),
            ],
            "Java Fountain loops masterDeck from the end toward the front"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::MaxHpChanged {
                delta: -3,
                current_hp: 77,
                max_hp: 77,
                source: DomainEventSource::Event(EventId::FountainOfCurseCleansing),
            }
        )));
    }

    #[test]
    fn fountain_drink_without_removable_curses_is_still_clickable_like_java() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Pain, 21),
            CombatCard::for_scenario(CardId::AscendersBane, 22),
        ];
        let mut bottle = RelicState::new(RelicId::BottledLightning);
        bottle.amount = crate::test_support::scenario_lineage(21).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        run_state.event_state = Some(EventState::new(EventId::FountainOfCurseCleansing));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        let choices = get_choices(&run_state, run_state.event_state.as_ref().unwrap());

        assert!(!choices[0].disabled);

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(
            run_state
                .master_deck
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Pain, crate::test_support::scenario_lineage(21)),
                (
                    CardId::AscendersBane,
                    crate::test_support::scenario_lineage(22)
                ),
            ]
        );
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().is_empty());
    }
}
