// Java: AccursedBlacksmith (shrines)
// Screen 0: [Forge] Upgrade a card | [Rummage] Gain Pain curse + WarpedTongs relic | [Leave]
// Screen 1: [Leave]
//
// Forge uses gridSelectScreen for player to choose which card to upgrade.

use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventRelicKind,
    EventSelectionKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

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

    let has_upgradable = run_state
        .master_deck
        .iter()
        .any(crate::state::core::master_deck_card_can_upgrade);

    let mut choices = vec![];
    if has_upgradable {
        choices.push(EventOption::new(
            EventChoiceMeta::new("[Forge] Upgrade a card."),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::UpgradeCard { count: 1 }],
                constraints: vec![EventOptionConstraint::RequiresUpgradeableCard],
                transition: EventOptionTransition::OpenSelection(EventSelectionKind::UpgradeCard),
                ..Default::default()
            },
        ));
    } else {
        choices.push(EventOption::new(
            EventChoiceMeta::disabled("[Forge] Upgrade a card.", "No upgradable cards"),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::UpgradeCard { count: 1 }],
                constraints: vec![EventOptionConstraint::RequiresUpgradeableCard],
                transition: EventOptionTransition::OpenSelection(EventSelectionKind::UpgradeCard),
                ..Default::default()
            },
        ));
    }
    choices.push(EventOption::new(
        EventChoiceMeta::new("[Rummage] Obtain Pain and Warped Tongs."),
        EventOptionSemantics {
            action: EventActionKind::Trade,
            effects: vec![
                EventEffect::ObtainRelic {
                    count: 1,
                    kind: EventRelicKind::Specific(crate::content::relics::RelicId::WarpedTongs),
                },
                EventEffect::ObtainCurse {
                    count: 1,
                    kind: EventCardKind::Specific(crate::content::cards::CardId::Pain),
                },
            ],
            transition: EventOptionTransition::AdvanceScreen,
            ..Default::default()
        },
    ));
    choices.push(EventOption::new(
        EventChoiceMeta::new("[Leave]"),
        EventOptionSemantics {
            action: EventActionKind::Leave,
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
                    // Forge: upgrade a card via grid-select (Java: gridSelectScreen.open(getUpgradableCards(), 1, ...))
                    if run_state
                        .master_deck
                        .iter()
                        .any(crate::state::core::master_deck_card_can_upgrade)
                    {
                        *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                            selected: Vec::new(),
                            min_choices: 1,
                            max_choices: 1,
                            reason: RunPendingChoiceReason::Upgrade,
                            source: DomainEventSource::Event(EventId::AccursedBlacksmith),
                            return_state: Box::new(EngineState::EventRoom),
                        });
                        event_state.current_screen = 1;
                    }
                }
                1 => {
                    // Rummage: Java constructs ShowCardAndObtainEffect(Pain)
                    // before spawnRelicAndObtain(WarpedTongs). Omamori checks
                    // happen at effect construction time, while card obtain
                    // resolves after the relic has been obtained.
                    let omamori_snapshot = run_state
                        .relics
                        .iter()
                        .find(|relic| relic.id == crate::content::relics::RelicId::Omamori)
                        .map(|relic| relic.counter);
                    let _ = run_state.obtain_relic_with_source(
                        crate::content::relics::RelicId::WarpedTongs,
                        EngineState::EventRoom,
                        DomainEventSource::Event(EventId::AccursedBlacksmith),
                    );
                    run_state.add_card_to_deck_with_omamori_snapshot_from(
                        crate::content::cards::CardId::Pain,
                        0,
                        DomainEventSource::Event(EventId::AccursedBlacksmith),
                        omamori_snapshot.is_some(),
                        omamori_snapshot.unwrap_or(0),
                    );
                    event_state.current_screen = 1;
                }
                _ => {
                    // Java shows LEAVE_RESULT on screenNum 2. Only the
                    // following Leave opens the map.
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

    fn blacksmith_run() -> RunState {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.event_state = Some(EventState {
            id: EventId::AccursedBlacksmith,
            current_screen: 0,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        run_state
    }

    fn deck_card(id: CardId, uuid: u32, upgrades: u8) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = upgrades;
        card
    }

    #[test]
    fn options_expose_structured_forge_rummage_and_leave_semantics() {
        let mut run_state = blacksmith_run();
        run_state.master_deck = vec![deck_card(CardId::Strike, 101, 0)];
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Accursed Blacksmith should expose structured event semantics");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].semantics.action, EventActionKind::DeckOperation);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::UpgradeCard { count: 1 }]
        );
        assert!(options[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresUpgradeableCard));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::UpgradeCard)
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Trade);
        assert_eq!(
            options[1].semantics.effects,
            vec![
                EventEffect::ObtainRelic {
                    count: 1,
                    kind: EventRelicKind::Specific(RelicId::WarpedTongs),
                },
                EventEffect::ObtainCurse {
                    count: 1,
                    kind: EventCardKind::Specific(CardId::Pain),
                },
            ]
        );
        assert_eq!(options[2].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[2].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert!(!options[2].semantics.terminal);

        let mut result_screen = EventState::new(EventId::AccursedBlacksmith);
        result_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &result_screen,
            )
            .expect("Accursed Blacksmith result screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn initial_leave_shows_result_before_completing_without_rewards() {
        let mut run_state = blacksmith_run();
        let mut engine_state = EngineState::EventRoom;
        let before = (
            run_state.current_hp,
            run_state.gold,
            run_state.master_deck.clone(),
            run_state.relics.clone(),
        );

        handle_choice(&mut engine_state, &mut run_state, 2);

        let event = run_state.event_state.as_ref().unwrap();
        assert_eq!(event.current_screen, 1);
        assert!(!event.completed);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(get_options(&run_state, event).len(), 1);
        assert_eq!(
            before,
            (
                run_state.current_hp,
                run_state.gold,
                run_state.master_deck.clone(),
                run_state.relics.clone()
            )
        );
        assert!(run_state.take_emitted_events().is_empty());

        handle_choice(&mut engine_state, &mut run_state, 0);
        assert!(run_state.event_state.as_ref().unwrap().completed);
    }

    #[test]
    fn forge_opens_upgrade_pending_choice_like_grid_select() {
        let mut run_state = blacksmith_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(matches!(
            engine_state,
            EngineState::RunPendingChoice(ref pending)
                if pending.reason == RunPendingChoiceReason::Upgrade
                    && pending.min_choices == 1
                    && pending.max_choices == 1
        ));
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
    }

    #[test]
    fn disabled_forge_does_not_open_empty_upgrade_selection() {
        let mut run_state = blacksmith_run();
        run_state.master_deck.clear();
        run_state
            .master_deck
            .push(crate::runtime::combat::CombatCard::for_scenario(
                CardId::Injury,
                100,
            ));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn forge_selection_uses_upgradable_cards_like_java() {
        let mut run_state = blacksmith_run();
        run_state.master_deck = vec![
            deck_card(CardId::Strike, 101, 0),
            deck_card(CardId::Defend, 102, 1),
            deck_card(CardId::Injury, 103, 0),
        ];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Forge should open deck upgrade selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::Upgrade);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Upgrade);
        assert_eq!(
            request.targets,
            vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(101)
            )],
            "Java opens masterDeck.getUpgradableCards()"
        );
    }

    #[test]
    fn forge_upgrades_selected_card_with_event_source() {
        let mut run_state = blacksmith_run();
        run_state.master_deck = vec![deck_card(CardId::Strike, 101, 0)];
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
        assert_eq!(run_state.master_deck[0].upgrades, 1);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                before,
                after,
                source: DomainEventSource::Event(EventId::AccursedBlacksmith),
            } if before.id == CardId::Strike
                && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
                && before.upgrades == 0
                && after.upgrades == 1
        )));
    }

    #[test]
    fn rummage_uses_event_sources_for_pain_and_warped_tongs() {
        let mut run_state = blacksmith_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Pain));
        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::WarpedTongs));
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::AccursedBlacksmith),
            } if card.id == CardId::Pain
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::WarpedTongs,
                source: DomainEventSource::Event(EventId::AccursedBlacksmith),
            }
        )));
        let relic_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::RelicObtained {
                        relic_id: RelicId::WarpedTongs,
                        source: DomainEventSource::Event(EventId::AccursedBlacksmith),
                    }
                )
            })
            .expect("Warped Tongs should be obtained");
        let pain_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::AccursedBlacksmith),
                    } if card.id == CardId::Pain
                )
            })
            .expect("Pain should be obtained");
        assert!(
            relic_pos < pain_pos,
            "Java spawnRelicAndObtain resolves before the queued ShowCardAndObtainEffect obtains Pain"
        );
    }

    #[test]
    fn rummage_pain_can_be_blocked_by_omamori_without_blocking_warped_tongs() {
        let mut run_state = blacksmith_run();
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Pain));
        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::WarpedTongs));
        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori should remain after blocking Pain");
        assert_eq!(omamori.counter, 1);
    }
}
