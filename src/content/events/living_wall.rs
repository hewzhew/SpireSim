use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventSelectionKind,
    EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

/// Returns the choices for the Living Wall event: [Forget, Change, Grow]
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

    let has_non_bottled_purgeable =
        crate::state::core::has_non_bottled_purgeable_master_deck_card(run_state);
    let has_upgradable = run_state
        .master_deck
        .iter()
        .any(crate::state::core::master_deck_card_can_upgrade);
    let deck_selection_transition = |selection_kind| {
        if has_non_bottled_purgeable {
            EventOptionTransition::OpenSelection(selection_kind)
        } else {
            EventOptionTransition::AdvanceScreen
        }
    };

    let mut choices = vec![
        EventOption::new(
            EventChoiceMeta::new("[Forget] Remove a card from your deck."),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: None,
                    kind: EventCardKind::Unknown,
                }],
                constraints: vec![EventOptionConstraint::RequiresNonBottledPurgeableCard],
                transition: deck_selection_transition(EventSelectionKind::RemoveCard),
                ..Default::default()
            },
        ),
        EventOption::new(
            EventChoiceMeta::new("[Change] Transform a card in your deck."),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::TransformCard { count: 1 }],
                constraints: vec![EventOptionConstraint::RequiresNonBottledPurgeableCard],
                transition: deck_selection_transition(EventSelectionKind::TransformCard),
                ..Default::default()
            },
        ),
    ];

    if has_upgradable {
        choices.push(EventOption::new(
            EventChoiceMeta::new("[Grow] Upgrade a card in your deck."),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::UpgradeCard { count: 1 }],
                constraints: vec![
                    EventOptionConstraint::RequiresUpgradeableCard,
                    EventOptionConstraint::RequiresNonBottledPurgeableCard,
                ],
                transition: deck_selection_transition(EventSelectionKind::UpgradeCard),
                ..Default::default()
            },
        ));
    } else {
        choices.push(EventOption::new(
            EventChoiceMeta::disabled(
                "[Grow] Upgrade a card in your deck.",
                "Requires an upgradable card in your deck.",
            ),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::UpgradeCard { count: 1 }],
                constraints: vec![
                    EventOptionConstraint::RequiresUpgradeableCard,
                    EventOptionConstraint::RequiresNonBottledPurgeableCard,
                ],
                transition: deck_selection_transition(EventSelectionKind::UpgradeCard),
                ..Default::default()
            },
        ));
    }

    choices
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    if let EngineState::EventRoom = engine_state {
        let has_non_bottled_purgeable =
            crate::state::core::has_non_bottled_purgeable_master_deck_card(run_state);
        let has_upgradable = run_state
            .master_deck
            .iter()
            .any(crate::state::core::master_deck_card_can_upgrade);
        let event_state = if let Some(es) = &mut run_state.event_state {
            es
        } else {
            return;
        };

        if event_state.completed {
            return;
        }

        // This event only has 1 interactive screen (screen 0) where you pick one path, then screen 1 is just 'Leave'
        if event_state.current_screen == 0 {
            if choice_idx >= 2 && !has_upgradable {
                return;
            }

            if !has_non_bottled_purgeable {
                event_state.current_screen = 1;
                return;
            }

            let reason = match choice_idx {
                0 => RunPendingChoiceReason::PurgeNonBottled, // [Forget]
                1 => RunPendingChoiceReason::TransformNonBottled, // [Change]
                _ => RunPendingChoiceReason::Upgrade,         // [Grow], it's button index 2
            };

            event_state.current_screen = 1; // Advance to post-choice 'Leave' screen
            *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                reason,
                min_choices: 1,
                max_choices: 1,
                source: DomainEventSource::Event(EventId::LivingWall),
                return_state: Box::new(EngineState::EventRoom),
            });
        } else {
            // "Leave" button pressed on post-choice screen
            event_state.completed = true;
        }
    }
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
        EventActionKind, EventEffect, EventOptionConstraint, EventOptionTransition,
        EventSelectionKind,
    };
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn living_wall_run() -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.event_state = Some(EventState::new(crate::state::events::EventId::LivingWall));
        run_state.emitted_events.clear();
        run_state
    }

    fn deck_card(id: CardId, uuid: u32, upgrades: u8) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = upgrades;
        card
    }

    fn bottle_uuid(run_state: &mut RunState, uuid: u32) {
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(uuid).to_runtime_i32_payload();
        run_state.relics.push(bottle);
    }

    #[test]
    fn options_expose_structured_forget_change_grow_and_leave_semantics() {
        let mut run_state = living_wall_run();
        run_state.master_deck = vec![deck_card(CardId::Strike, 101, 0)];
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Living Wall should expose structured event semantics");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].semantics.action, EventActionKind::DeckOperation);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::RemoveCard {
                count: 1,
                target_lineage: None,
                kind: crate::state::events::EventCardKind::Unknown,
            }]
        );
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
        );
        assert!(options[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresNonBottledPurgeableCard));

        assert_eq!(
            options[1].semantics.effects,
            vec![EventEffect::TransformCard { count: 1 }]
        );
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::TransformCard)
        );
        assert!(options[1]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresNonBottledPurgeableCard));

        assert_eq!(
            options[2].semantics.effects,
            vec![EventEffect::UpgradeCard { count: 1 }]
        );
        assert!(options[2]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresUpgradeableCard));
        assert!(options[2]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresNonBottledPurgeableCard));
        assert_eq!(
            options[2].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::UpgradeCard)
        );

        let mut result_screen =
            crate::state::events::EventState::new(crate::state::events::EventId::LivingWall);
        result_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &result_screen,
            )
            .expect("Living Wall result screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn options_preserve_java_enabled_but_no_selection_living_wall_edge_case() {
        let mut run_state = living_wall_run();
        run_state.master_deck = vec![deck_card(CardId::Strike, 101, 0)];
        bottle_uuid(&mut run_state, 101);

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            run_state.event_state.as_ref().unwrap(),
        )
        .expect(
            "Living Wall should expose semantics even when no non-bottled card can be selected",
        );

        assert!(!options[0].ui.disabled);
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen,
            "Java keeps Forget clickable but opens no grid when all purgeable cards are bottled"
        );
        assert!(!options[1].ui.disabled);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen,
            "Java keeps Change clickable but opens no grid when all purgeable cards are bottled"
        );
        assert!(!options[2].ui.disabled);
        assert_eq!(
            options[2].semantics.transition,
            EventOptionTransition::AdvanceScreen,
            "Grow is enabled by upgradable cards, but Java's non-bottled purgeable guard still prevents the grid"
        );
    }

    #[test]
    fn disabled_grow_does_not_open_empty_upgrade_selection() {
        let mut run_state = living_wall_run();
        run_state.master_deck.clear();
        run_state
            .master_deck
            .push(crate::runtime::combat::CombatCard::for_scenario(
                CardId::Injury,
                100,
            ));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
    }

    #[test]
    fn grow_keeps_java_non_bottled_purgeable_guard_before_upgrade_prompt() {
        let mut run_state = living_wall_run();
        run_state.master_deck.clear();
        let strike = crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 100);
        let mut bottle =
            crate::content::relics::RelicState::new(crate::content::relics::RelicId::BottledFlame);
        bottle.amount = strike.lineage.to_runtime_i32_payload();
        run_state.relics.push(bottle);
        run_state.master_deck.push(strike);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
        assert!(
            matches!(engine_state, EngineState::EventRoom),
            "Java checks getGroupWithoutBottledCards(getPurgeableCards()) before opening the Grow upgrade grid"
        );
    }

    #[test]
    fn forget_and_change_selection_exclude_bottled_and_unpurgeable_cards_like_java() {
        for (choice_idx, expected_reason, expected_selection_reason) in [
            (
                0,
                RunPendingChoiceReason::PurgeNonBottled,
                SelectionReason::Purge,
            ),
            (
                1,
                RunPendingChoiceReason::TransformNonBottled,
                SelectionReason::Transform,
            ),
        ] {
            let mut run_state = living_wall_run();
            run_state.master_deck = vec![
                deck_card(CardId::Strike, 101, 0),
                deck_card(CardId::Defend, 102, 0),
                deck_card(CardId::AscendersBane, 103, 0),
            ];
            bottle_uuid(&mut run_state, 101);
            let mut engine_state = EngineState::EventRoom;

            handle_choice(&mut engine_state, &mut run_state, choice_idx);

            let EngineState::RunPendingChoice(choice) = engine_state else {
                panic!("Living Wall choice should open a deck selection");
            };
            assert_eq!(choice.reason, expected_reason);
            let request = choice.selection_request(&run_state);
            assert_eq!(request.reason, expected_selection_reason);
            assert_eq!(
                request.targets,
                vec![SelectionTargetRef::CardLineage(crate::runtime::combat::CardLineageId::constructed_scenario(102))],
                "Java uses CardGroup.getGroupWithoutBottledCards(masterDeck.getPurgeableCards()) for Forget and Change"
            );
        }
    }

    #[test]
    fn forget_removes_selected_card_with_event_source() {
        let mut run_state = living_wall_run();
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
        assert!(run_state.master_deck.is_empty());
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(crate::state::events::EventId::LivingWall),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )));
    }

    #[test]
    fn change_transforms_selected_card_with_event_source() {
        let mut run_state = living_wall_run();
        run_state.master_deck = vec![deck_card(CardId::Strike, 101, 0)];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

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
        assert_eq!(run_state.master_deck.len(), 1);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardTransformed {
                before,
                source: DomainEventSource::Event(crate::state::events::EventId::LivingWall),
                ..
            } if before.id == CardId::Strike && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )));
    }

    #[test]
    fn change_runs_obtain_hooks_before_transformed_card_is_recorded() {
        let mut run_state = living_wall_run();
        run_state.master_deck = vec![deck_card(CardId::Strike, 101, 0)];
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

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

        let events = run_state.take_emitted_events();
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(crate::state::events::EventId::LivingWall),
                        ..
                    }
                )
            })
            .expect("Ceramic Fish should run from the delayed ShowCardAndObtainEffect obtain hook");
        let transformed_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardTransformed {
                        before,
                        source: DomainEventSource::Event(crate::state::events::EventId::LivingWall),
                        ..
                    } if before.id == CardId::Strike && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )
            })
            .expect("Living Wall Change should record the transformed replacement");

        assert!(
            fish_gold_pos < transformed_pos,
            "Java LivingWall removes/transforms the selected card, then queued ShowCardAndObtainEffect runs onObtainCard before Soul.obtain completes the replacement"
        );
    }
}
