use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionConstraint,
    EventOptionSemantics, EventOptionTransition, EventSelectionKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn has_upgradable_cards(run_state: &RunState) -> bool {
    run_state
        .master_deck
        .iter()
        .any(crate::state::core::master_deck_card_can_upgrade)
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

    let mut choices = Vec::new();

    if has_upgradable_cards(run_state) {
        choices.push(EventOption::new(
            EventChoiceMeta::new("[Pray] Upgrade a card."),
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
            EventChoiceMeta::disabled("[Pray] Upgrade a card.", "No upgradable cards."),
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

    match event_state.current_screen {
        0 => {
            match choice_idx {
                0 => {
                    if has_upgradable_cards(run_state) {
                        event_state.current_screen = 1;
                        run_state.event_state = Some(event_state);
                        *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                            selected: Vec::new(),
                            reason: RunPendingChoiceReason::Upgrade,
                            source: DomainEventSource::Event(EventId::UpgradeShrine),
                            min_choices: 1,
                            max_choices: 1,
                            return_state: Box::new(EngineState::EventRoom),
                        });
                        return;
                    }
                }
                _ => {
                    // Leave
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
    use super::{get_choices, handle_choice};
    use crate::content::cards::CardId;
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason};
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionConstraint, EventOptionTransition,
        EventSelectionKind, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn shrine_run(deck: Vec<CombatCard>) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = deck;
        run_state.event_state = Some(EventState::new(EventId::UpgradeShrine));
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn options_expose_structured_pray_ignore_and_leave_semantics() {
        let run_state = shrine_run(vec![CombatCard::for_scenario(CardId::Defend, 102)]);
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Upgrade Shrine should expose structured event semantics");

        assert_eq!(options.len(), 2);
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
        assert_eq!(options[1].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let disabled_run = shrine_run(vec![CombatCard::for_scenario(CardId::Injury, 11)]);
        let disabled = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &disabled_run,
            disabled_run.event_state.as_ref().unwrap(),
        )
        .expect("Upgrade Shrine disabled Pray should still expose semantics");
        assert!(disabled[0].ui.disabled);
        assert!(disabled[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresUpgradeableCard));

        let mut result_screen = EventState::new(EventId::UpgradeShrine);
        result_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &result_screen,
            )
            .expect("Upgrade Shrine result screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn disabled_pray_does_not_open_empty_upgrade_selection() {
        let mut run_state = shrine_run(vec![CombatCard::for_scenario(CardId::Injury, 11)]);
        let mut engine_state = EngineState::EventRoom;

        let choices = get_choices(&run_state, run_state.event_state.as_ref().unwrap());
        assert!(choices[0].disabled);

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn searing_blow_remains_upgradeable_after_prior_upgrades() {
        let mut searing = CombatCard::for_scenario(CardId::SearingBlow, 12);
        searing.upgrades = 3;
        let mut run_state = shrine_run(vec![searing]);
        let mut engine_state = EngineState::EventRoom;

        let choices = get_choices(&run_state, run_state.event_state.as_ref().unwrap());
        assert!(!choices[0].disabled);

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(matches!(
            engine_state,
            EngineState::RunPendingChoice(ref pending)
                if pending.reason == RunPendingChoiceReason::Upgrade
        ));
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
    }

    #[test]
    fn upgrade_selection_uses_java_upgradable_cards() {
        let mut upgraded_strike = CombatCard::for_scenario(CardId::Strike, 101);
        upgraded_strike.upgrades = 1;
        let mut run_state = shrine_run(vec![
            upgraded_strike,
            CombatCard::for_scenario(CardId::Defend, 102),
            CombatCard::for_scenario(CardId::Injury, 103),
        ]);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Upgrade Shrine should open deck upgrade selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::Upgrade);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Upgrade);
        assert_eq!(
            request.targets,
            vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(102)
            )],
            "Java opens masterDeck.getUpgradableCards()"
        );
    }

    #[test]
    fn selected_card_is_upgraded_with_event_source() {
        let mut run_state = shrine_run(vec![CombatCard::for_scenario(CardId::Defend, 102)]);
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
                    crate::runtime::combat::CardLineageId::constructed_scenario(102)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(run_state.master_deck[0].upgrades, 1);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                after,
                source: DomainEventSource::Event(EventId::UpgradeShrine),
                ..
            } if after.id == CardId::Defend && after.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(102) && after.upgrades == 1
        )));
    }
}
