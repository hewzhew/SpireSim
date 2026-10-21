use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventSelectionKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn get_options(_run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
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

    vec![
        EventOption::new(
            EventChoiceMeta::new("[Pray] Duplicate a card in your deck."),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::DuplicateCard { count: 1 }],
                transition: EventOptionTransition::OpenSelection(EventSelectionKind::DuplicateCard),
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
                    // Duplicate a card
                    event_state.current_screen = 1;
                    run_state.event_state = Some(event_state);
                    *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                        selected: Vec::new(),
                        reason: RunPendingChoiceReason::Duplicate,
                        source: DomainEventSource::Event(EventId::Duplicator),
                        min_choices: 1,
                        max_choices: 1,
                        return_state: Box::new(EngineState::EventRoom),
                    });
                    return;
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
    use super::handle_choice;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason, RunResult};
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventSelectionKind,
        EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn deck_card(id: CardId, uuid: u32, upgrades: u8) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = upgrades;
        card
    }

    #[test]
    fn options_expose_structured_duplicate_ignore_and_leave_semantics() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![deck_card(CardId::AscendersBane, 11, 0)];
        rs.event_state = Some(EventState::new(EventId::Duplicator));

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &rs,
            rs.event_state.as_ref().unwrap(),
        )
        .expect("Duplicator should expose structured event semantics");

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::DeckOperation);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::DuplicateCard { count: 1 }]
        );
        assert!(options[0].semantics.constraints.is_empty());
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::DuplicateCard)
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut result_screen = EventState::new(EventId::Duplicator);
        result_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &rs,
                &result_screen,
            )
            .expect("Duplicator result screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn duplicate_selection_uses_full_master_deck_like_java() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![
            deck_card(CardId::AscendersBane, 11, 0),
            deck_card(CardId::Strike, 12, 0),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(12).to_runtime_i32_payload();
        rs.relics.push(bottle);
        rs.event_state = Some(EventState::new(EventId::Duplicator));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Duplicator should open deck duplicate selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::Duplicate);
        let request = choice.selection_request(&rs);
        assert_eq!(request.reason, SelectionReason::Duplicate);
        assert_eq!(
            request.targets,
            vec![
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(11)
                ),
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(12)
                ),
            ],
            "Java Duplicator opens the full masterDeck, not getPurgeableCards or non-bottled cards"
        );
    }

    #[test]
    fn duplicate_selection_obtains_stat_equivalent_copy_with_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        let mut original = deck_card(CardId::RitualDagger, 101, 2);
        original.misc_value = 17;
        original.base_damage_override = Some(23);
        original.cost_modifier = -1;
        original.cost_for_turn = Some(0);
        original.free_to_play_once = true;
        original.base_damage_mut = 99;
        original.base_block_mut = 88;
        original.base_magic_num_mut = 77;
        rs.master_deck = vec![original];
        rs.event_state = Some(EventState::new(EventId::Duplicator));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut rs,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(!matches!(
            engine_state,
            EngineState::GameOver(RunResult::Defeat)
        ));
        assert_eq!(rs.master_deck.len(), 2);
        let copied = rs
            .master_deck
            .iter()
            .find(|card| {
                card.lineage != crate::runtime::combat::CardLineageId::constructed_scenario(101)
            })
            .expect("Duplicator should add a copied card");
        assert_eq!(copied.id, CardId::RitualDagger);
        assert_eq!(copied.upgrades, 2);
        assert_eq!(copied.misc_value, 17);
        assert_eq!(copied.base_damage_override, Some(23));
        assert_eq!(copied.cost_modifier, -1);
        assert_eq!(copied.cost_for_turn, Some(0));
        assert!(copied.free_to_play_once);
        assert_eq!(copied.base_damage_mut, 0);
        assert_eq!(copied.base_block_mut, 0);
        assert_eq!(copied.base_magic_num_mut, 0);
        assert_ne!(
            copied.lineage,
            crate::runtime::combat::CardLineageId::constructed_scenario(101)
        );
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::Duplicator),
            } if card.id == CardId::RitualDagger && card.upgrades == 2
        )));
    }

    #[test]
    fn duplicate_copy_runs_obtain_hooks_before_card_obtained_event() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![deck_card(CardId::Strike, 101, 0)];
        rs.relics.push(RelicState::new(RelicId::MoltenEgg));
        rs.relics.push(RelicState::new(RelicId::CeramicFish));
        rs.event_state = Some(EventState::new(EventId::Duplicator));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut rs,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )],
            })),
        ));

        let copied = rs
            .master_deck
            .iter()
            .find(|card| {
                card.lineage != crate::runtime::combat::CardLineageId::constructed_scenario(101)
            })
            .expect("Duplicator should add a copied Strike");
        assert_eq!(
            copied.upgrades, 1,
            "Java ShowCardAndObtainEffect calls Molten Egg onObtainCard on the copied unupgraded Attack"
        );
        assert_eq!(
            rs.master_deck
                .iter()
                .find(|card| card.lineage
                    == crate::runtime::combat::CardLineageId::constructed_scenario(101))
                .unwrap()
                .upgrades,
            0,
            "Duplicator must not upgrade the original selected master-deck card"
        );

        let events = rs.take_emitted_events();
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::Duplicator),
                        ..
                    }
                )
            })
            .expect("Ceramic Fish should run while the copied card is being obtained");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::Duplicator),
                    } if card.id == CardId::Strike && card.upgrades == 1
                )
            })
            .expect("Duplicator should record the egg-upgraded copied Strike");

        assert!(
            fish_gold_pos < obtained_pos,
            "Java Duplicator queues ShowCardAndObtainEffect for makeStatEquivalentCopy; that effect runs relic onObtainCard before Soul.obtain"
        );
    }
}
