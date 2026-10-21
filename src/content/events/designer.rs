use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventSelectionKind,
    EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

// Java Designer: randomizes upgrade-one vs upgrade-two-random, and remove-one vs transform-two
// internal_state encodes: bit0 = adjustmentUpgradesOne, bit1 = cleanUpRemovesCards
// Costs: A15: 50/75/110/5hp, else: 40/60/90/3hp

fn adjust_cost(asc: u8) -> i32 {
    if asc >= 15 {
        50
    } else {
        40
    }
}
fn cleanup_cost(asc: u8) -> i32 {
    if asc >= 15 {
        75
    } else {
        60
    }
}
fn full_service_cost(asc: u8) -> i32 {
    if asc >= 15 {
        110
    } else {
        90
    }
}
fn hp_loss(asc: u8) -> i32 {
    if asc >= 15 {
        5
    } else {
        3
    }
}

fn upgrades_one(state: i32) -> bool {
    state & 1 != 0
}
fn removes_cards(state: i32) -> bool {
    state & 2 != 0
}

fn has_upgradable_card(run_state: &RunState) -> bool {
    run_state
        .master_deck
        .iter()
        .any(crate::state::core::master_deck_card_can_upgrade)
}

fn non_bottled_master_deck_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| !crate::state::core::master_deck_card_is_bottled(card, &run_state.relics))
        .count()
}

fn adjust_disabled(run_state: &RunState, _event_state: &EventState) -> bool {
    run_state.gold < adjust_cost(run_state.ascension_level) || !has_upgradable_card(run_state)
}

fn cleanup_disabled(run_state: &RunState, event_state: &EventState) -> bool {
    let non_bottled_count = non_bottled_master_deck_count(run_state);
    run_state.gold < cleanup_cost(run_state.ascension_level)
        || if removes_cards(event_state.internal_state) {
            non_bottled_count == 0
        } else {
            non_bottled_count < 2
        }
}

fn full_service_disabled(run_state: &RunState) -> bool {
    run_state.gold < full_service_cost(run_state.ascension_level)
        || non_bottled_master_deck_count(run_state) == 0
}

fn designer_random_upgrade(run_state: &mut RunState, count: usize) {
    let mut upgradable: Vec<usize> = run_state
        .master_deck
        .iter()
        .enumerate()
        .filter(|(_, card)| crate::state::core::master_deck_card_can_upgrade(card))
        .map(|(i, _)| i)
        .collect();

    if upgradable.is_empty() {
        return;
    }

    crate::runtime::rng::shuffle_with_random_long(
        &mut upgradable,
        &mut run_state.rng_pool.misc_rng,
    );
    let selected_lineages: Vec<crate::runtime::combat::CardLineageId> = upgradable
        .into_iter()
        .take(count)
        .filter_map(|idx| run_state.master_deck.get(idx).map(|card| card.lineage))
        .collect();

    for lineage in selected_lineages {
        run_state.upgrade_card_with_source(lineage, DomainEventSource::Event(EventId::Designer));
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![EventOption::new(
            EventChoiceMeta::new("[Proceed]"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                effects: vec![],
                constraints: vec![],
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        )],
        1 => {
            let asc = run_state.ascension_level;

            let adj_label = if upgrades_one(event_state.internal_state) {
                format!("[Adjust] {} Gold. Upgrade 1 card.", adjust_cost(asc))
            } else {
                format!(
                    "[Adjust] {} Gold. Upgrade 2 random cards.",
                    adjust_cost(asc)
                )
            };
            let adj_disabled = adjust_disabled(run_state, event_state);

            let clean_label = if removes_cards(event_state.internal_state) {
                format!("[Clean Up] {} Gold. Remove 1 card.", cleanup_cost(asc))
            } else {
                format!("[Clean Up] {} Gold. Transform 2 cards.", cleanup_cost(asc))
            };
            let clean_disabled = cleanup_disabled(run_state, event_state);

            let full_label = format!(
                "[Full Service] {} Gold. Remove 1 card + upgrade 1 random.",
                full_service_cost(asc)
            );
            let full_disabled = full_service_disabled(run_state);

            let punch_label = format!("[Punch] Lose {} HP.", hp_loss(asc));

            vec![
                if adj_disabled {
                    EventOption::new(
                        EventChoiceMeta::disabled(adj_label, "Not enough Gold/cards"),
                        EventOptionSemantics {
                            action: EventActionKind::DeckOperation,
                            effects: vec![EventEffect::LoseGold(adjust_cost(asc))],
                            constraints: vec![
                                EventOptionConstraint::RequiresGold(adjust_cost(asc)),
                                EventOptionConstraint::RequiresUpgradeableCard,
                            ],
                            transition: if upgrades_one(event_state.internal_state) {
                                EventOptionTransition::OpenSelection(
                                    EventSelectionKind::UpgradeCard,
                                )
                            } else {
                                EventOptionTransition::AdvanceScreen
                            },
                            repeatable: false,
                            terminal: false,
                        },
                    )
                } else {
                    EventOption::new(
                        EventChoiceMeta::new(adj_label),
                        EventOptionSemantics {
                            action: EventActionKind::DeckOperation,
                            effects: vec![
                                EventEffect::LoseGold(adjust_cost(asc)),
                                EventEffect::UpgradeCard {
                                    count: if upgrades_one(event_state.internal_state) {
                                        1
                                    } else {
                                        2
                                    },
                                },
                            ],
                            constraints: vec![
                                EventOptionConstraint::RequiresGold(adjust_cost(asc)),
                                EventOptionConstraint::RequiresUpgradeableCard,
                            ],
                            transition: if upgrades_one(event_state.internal_state) {
                                EventOptionTransition::OpenSelection(
                                    EventSelectionKind::UpgradeCard,
                                )
                            } else {
                                EventOptionTransition::AdvanceScreen
                            },
                            repeatable: false,
                            terminal: false,
                        },
                    )
                },
                if clean_disabled {
                    EventOption::new(
                        EventChoiceMeta::disabled(clean_label, "Not enough Gold"),
                        EventOptionSemantics {
                            action: EventActionKind::DeckOperation,
                            effects: vec![
                                EventEffect::LoseGold(cleanup_cost(asc)),
                                if removes_cards(event_state.internal_state) {
                                    EventEffect::RemoveCard {
                                        count: 1,
                                        target_lineage: None,
                                        kind: EventCardKind::Unknown,
                                    }
                                } else {
                                    EventEffect::TransformCard { count: 2 }
                                },
                            ],
                            constraints: vec![
                                EventOptionConstraint::RequiresGold(cleanup_cost(asc)),
                                if removes_cards(event_state.internal_state) {
                                    EventOptionConstraint::RequiresRemovableCard
                                } else {
                                    EventOptionConstraint::RequiresTransformableCard
                                },
                            ],
                            transition: if removes_cards(event_state.internal_state) {
                                EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
                            } else {
                                EventOptionTransition::OpenSelection(
                                    EventSelectionKind::TransformCard,
                                )
                            },
                            repeatable: false,
                            terminal: false,
                        },
                    )
                } else {
                    EventOption::new(
                        EventChoiceMeta::new(clean_label),
                        EventOptionSemantics {
                            action: EventActionKind::DeckOperation,
                            effects: vec![
                                EventEffect::LoseGold(cleanup_cost(asc)),
                                if removes_cards(event_state.internal_state) {
                                    EventEffect::RemoveCard {
                                        count: 1,
                                        target_lineage: None,
                                        kind: EventCardKind::Unknown,
                                    }
                                } else {
                                    EventEffect::TransformCard { count: 2 }
                                },
                            ],
                            constraints: vec![
                                EventOptionConstraint::RequiresGold(cleanup_cost(asc)),
                                if removes_cards(event_state.internal_state) {
                                    EventOptionConstraint::RequiresRemovableCard
                                } else {
                                    EventOptionConstraint::RequiresTransformableCard
                                },
                            ],
                            transition: if removes_cards(event_state.internal_state) {
                                EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
                            } else {
                                EventOptionTransition::OpenSelection(
                                    EventSelectionKind::TransformCard,
                                )
                            },
                            repeatable: false,
                            terminal: false,
                        },
                    )
                },
                if full_disabled {
                    EventOption::new(
                        EventChoiceMeta::disabled(full_label, "Not enough Gold"),
                        EventOptionSemantics {
                            action: EventActionKind::DeckOperation,
                            effects: vec![
                                EventEffect::LoseGold(full_service_cost(asc)),
                                EventEffect::RemoveCard {
                                    count: 1,
                                    target_lineage: None,
                                    kind: EventCardKind::Unknown,
                                },
                                EventEffect::UpgradeCard { count: 1 },
                            ],
                            constraints: vec![
                                EventOptionConstraint::RequiresGold(full_service_cost(asc)),
                                EventOptionConstraint::RequiresRemovableCard,
                            ],
                            transition: EventOptionTransition::OpenSelection(
                                EventSelectionKind::RemoveCard,
                            ),
                            repeatable: false,
                            terminal: false,
                        },
                    )
                } else {
                    EventOption::new(
                        EventChoiceMeta::new(full_label),
                        EventOptionSemantics {
                            action: EventActionKind::DeckOperation,
                            effects: vec![
                                EventEffect::LoseGold(full_service_cost(asc)),
                                EventEffect::RemoveCard {
                                    count: 1,
                                    target_lineage: None,
                                    kind: EventCardKind::Unknown,
                                },
                                EventEffect::UpgradeCard { count: 1 },
                            ],
                            constraints: vec![
                                EventOptionConstraint::RequiresGold(full_service_cost(asc)),
                                EventOptionConstraint::RequiresRemovableCard,
                            ],
                            transition: EventOptionTransition::OpenSelection(
                                EventSelectionKind::RemoveCard,
                            ),
                            repeatable: false,
                            terminal: false,
                        },
                    )
                },
                EventOption::new(
                    EventChoiceMeta::new(punch_label),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        effects: vec![EventEffect::LoseHp(hp_loss(asc))],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ),
            ]
        }
        _ => vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                effects: vec![],
                constraints: vec![],
                transition: EventOptionTransition::Complete,
                repeatable: false,
                terminal: true,
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
            run_state.event_state = Some(event_state);
        }
        1 => {
            let asc = run_state.ascension_level;
            match choice_idx {
                0 => {
                    // Adjust
                    if adjust_disabled(run_state, &event_state) {
                        run_state.event_state = Some(event_state);
                        return;
                    }
                    run_state.change_gold_with_source(
                        -adjust_cost(asc),
                        DomainEventSource::Event(EventId::Designer),
                    );
                    if upgrades_one(event_state.internal_state) {
                        // Upgrade 1: go to RunPendingChoice::Upgrade
                        event_state.current_screen = 2;
                        run_state.event_state = Some(event_state);
                        *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                            selected: Vec::new(),
                            min_choices: 1,
                            max_choices: 1,
                            reason: RunPendingChoiceReason::Upgrade,
                            source: DomainEventSource::Event(EventId::Designer),
                            return_state: Box::new(EngineState::EventRoom),
                        });
                        return;
                    } else {
                        // Java: Collections.shuffle(upgradableCards, new Random(miscRng.randomLong()))
                        designer_random_upgrade(run_state, 2);
                        event_state.current_screen = 2;
                    }
                }
                1 => {
                    // Clean Up
                    if cleanup_disabled(run_state, &event_state) {
                        run_state.event_state = Some(event_state);
                        return;
                    }
                    run_state.change_gold_with_source(
                        -cleanup_cost(asc),
                        DomainEventSource::Event(EventId::Designer),
                    );
                    if removes_cards(event_state.internal_state) {
                        // Remove 1 card
                        event_state.current_screen = 2;
                        run_state.event_state = Some(event_state);
                        *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                            selected: Vec::new(),
                            min_choices: 1,
                            max_choices: 1,
                            reason: RunPendingChoiceReason::PurgeNonBottled,
                            source: DomainEventSource::Event(EventId::Designer),
                            return_state: Box::new(EngineState::EventRoom),
                        });
                        return;
                    } else {
                        // Transform 2 cards
                        event_state.current_screen = 2;
                        run_state.event_state = Some(event_state);
                        *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                            selected: Vec::new(),
                            min_choices: 2,
                            max_choices: 2,
                            reason: RunPendingChoiceReason::TransformNonBottled,
                            source: DomainEventSource::Event(EventId::Designer),
                            return_state: Box::new(EngineState::EventRoom),
                        });
                        return;
                    }
                }
                2 => {
                    // Full Service: remove 1 card + upgrade 1 random (Java: REMOVE_AND_UPGRADE)
                    if full_service_disabled(run_state) {
                        run_state.event_state = Some(event_state);
                        return;
                    }
                    run_state.change_gold_with_source(
                        -full_service_cost(asc),
                        DomainEventSource::Event(EventId::Designer),
                    );
                    event_state.extra_data = vec![1]; // Mark as Full Service for post-purge upgrade
                    event_state.current_screen = 2;
                    run_state.event_state = Some(event_state);
                    *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                        selected: Vec::new(),
                        min_choices: 1,
                        max_choices: 1,
                        reason: RunPendingChoiceReason::PurgeNonBottled,
                        source: DomainEventSource::Event(EventId::Designer),
                        return_state: Box::new(EngineState::EventRoom),
                    });
                    return;
                }
                _ => {
                    // Punch: HP loss
                    super::apply_player_hp_loss_damage(
                        run_state,
                        hp_loss(asc),
                        DomainEventSource::Event(EventId::Designer),
                    );
                    event_state.current_screen = 2;
                }
            }
            run_state.event_state = Some(event_state);
        }
        _ => {
            // Returned from purge/upgrade/transform. For Full Service, upgrade 1 random card.
            // Java: REMOVE_AND_UPGRADE callback shuffles upgradable cards and upgrades [0].
            // We use extra_data[0] = 1 to mark Full Service so we can do the upgrade on return.
            if !event_state.extra_data.is_empty() && event_state.extra_data[0] == 1 {
                event_state.extra_data.clear();
                designer_random_upgrade(run_state, 1);
            }
            event_state.completed = true;
            run_state.event_state = Some(event_state);
        }
    }
}

/// Initialize Designer state.
/// Java constructor: miscRng.randomBoolean() × 2 for adjustmentUpgradesOne and cleanUpRemovesCards.
/// internal_state: bit0 = upgrades_one, bit1 = removes_cards
pub fn init_designer_state(run_state: &mut RunState) -> i32 {
    let upgrades_one = run_state.rng_pool.misc_rng.random_boolean();
    let removes_cards = run_state.rng_pool.misc_rng.random_boolean();
    (upgrades_one as i32) | ((removes_cards as i32) << 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::ClientInput;
    use crate::state::events::{EventOptionConstraint, EventOptionTransition, EventSelectionKind};
    use crate::state::selection::{
        DomainEvent, SelectionReason, SelectionResolution, SelectionScope, SelectionTargetRef,
    };

    fn designer_state(current_screen: usize, internal_state: i32) -> EventState {
        EventState {
            id: EventId::Designer,
            current_screen,
            internal_state,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        }
    }

    fn deck_card(id: CardId, uuid: u32, upgrades: u8) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = upgrades;
        card
    }

    #[test]
    fn designer_cleanup_remove_exposes_selection_semantics() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        let state = designer_state(1, 0b11);
        let options = get_options(&rs, &state);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
        );
        assert!(options[1]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresRemovableCard));
    }

    #[test]
    fn designer_adjust_upgrade_one_selection_uses_java_can_upgrade() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 1),
            deck_card(CardId::Defend, 12, 0),
            deck_card(CardId::AscendersBane, 13, 0),
        ];
        rs.event_state = Some(designer_state(1, 0b01));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Adjust upgrade-one should open deck upgrade selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::Upgrade);
        let request = choice.selection_request(&rs);
        assert_eq!(
            request.targets,
            vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(12)
            )],
            "already-upgraded normal cards and unupgradable curses must not be selectable"
        );
    }

    #[test]
    fn designer_disabled_adjust_without_gold_does_not_pay_or_open_selection() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 0;
        rs.master_deck = vec![deck_card(CardId::Defend, 12, 0)];
        rs.event_state = Some(designer_state(1, 0b01));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.gold, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 1);
        assert!(!rs.event_state.as_ref().unwrap().completed);
    }

    #[test]
    fn designer_disabled_adjust_without_upgradable_card_does_not_pay_or_advance() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![deck_card(CardId::Strike, 11, 1)];
        rs.event_state = Some(designer_state(1, 0b00));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.gold, 99);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 1);
        assert_eq!(rs.master_deck[0].upgrades, 1);
    }

    #[test]
    fn designer_cleanup_remove_selection_excludes_bottled_and_unpurgeable_cards() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 0),
            deck_card(CardId::Defend, 12, 0),
            deck_card(CardId::AscendersBane, 13, 0),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(11).to_runtime_i32_payload();
        rs.relics.push(bottle);
        rs.event_state = Some(designer_state(1, 0b11));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 1);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Clean Up remove should open deck purge selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::PurgeNonBottled);
        let request = choice.selection_request(&rs);
        assert_eq!(
            request.targets,
            vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(12)
            )],
            "Designer opens CardGroup.getGroupWithoutBottledCards(getPurgeableCards())"
        );
    }

    #[test]
    fn designer_cleanup_remove_selected_card_uses_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![deck_card(CardId::Strike, 11, 0)];
        rs.event_state = Some(designer_state(1, 0b11));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 1);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut rs,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(11)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(rs.gold, 99 - cleanup_cost(0));
        assert!(rs.master_deck.is_empty());
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::Designer),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11)
        )));
    }

    #[test]
    fn designer_cleanup_transform_selection_excludes_bottled_and_unpurgeable_cards() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 0),
            deck_card(CardId::Defend, 12, 0),
            deck_card(CardId::Bash, 13, 0),
            deck_card(CardId::AscendersBane, 14, 0),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(13).to_runtime_i32_payload();
        rs.relics.push(bottle);
        rs.event_state = Some(designer_state(1, 0b00));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 1);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Clean Up transform should open deck transform selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::TransformNonBottled);
        let request = choice.selection_request(&rs);
        assert_eq!(request.reason, SelectionReason::Transform);
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
            "Designer transform opens CardGroup.getGroupWithoutBottledCards(getPurgeableCards())"
        );
    }

    #[test]
    fn designer_cleanup_transform_selected_cards_use_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 0),
            deck_card(CardId::Defend, 12, 0),
        ];
        rs.event_state = Some(designer_state(1, 0b00));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 1);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut rs,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(11)
                    ),
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(12)
                    ),
                ],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(rs.gold, 99 - cleanup_cost(0));
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardTransformed {
                before,
                source: DomainEventSource::Event(EventId::Designer),
                ..
            } if before.id == CardId::Strike && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11)
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardTransformed {
                before,
                source: DomainEventSource::Event(EventId::Designer),
                ..
            } if before.id == CardId::Defend && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12)
        )));
    }

    #[test]
    fn designer_disabled_cleanup_without_gold_does_not_pay_or_open_selection() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 0;
        rs.master_deck = vec![deck_card(CardId::Strike, 11, 0)];
        rs.event_state = Some(designer_state(1, 0b11));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 1);

        assert_eq!(rs.gold, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 1);
    }

    #[test]
    fn designer_disabled_cleanup_transform_requires_two_non_bottled_cards() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![deck_card(CardId::Strike, 11, 0)];
        rs.event_state = Some(designer_state(1, 0b00));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 1);

        assert_eq!(rs.gold, 99);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 1);
    }

    #[test]
    fn designer_disabled_full_service_does_not_pay_or_open_selection() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 0;
        rs.master_deck = vec![deck_card(CardId::Strike, 11, 0)];
        rs.event_state = Some(designer_state(1, 0b00));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 2);

        assert_eq!(rs.gold, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 1);
    }

    #[test]
    fn designer_random_upgrade_uses_can_upgrade_and_domain_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 1),
            deck_card(CardId::Defend, 12, 0),
            deck_card(CardId::AscendersBane, 13, 0),
        ];
        rs.event_state = Some(designer_state(1, 0b00));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.master_deck[0].upgrades, 1);
        assert_eq!(rs.master_deck[1].upgrades, 1);
        assert_eq!(rs.master_deck[2].upgrades, 0);
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                before,
                after,
                source: DomainEventSource::Event(EventId::Designer),
            } if before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12) && before.upgrades == 0 && after.upgrades == 1
        )));
    }

    #[test]
    fn designer_punch_emits_hp_loss_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.current_hp = 10;
        rs.event_state = Some(designer_state(1, 0b00));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 3);

        assert_eq!(rs.current_hp, 7);
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -3,
                current_hp: 7,
                source: DomainEventSource::Event(EventId::Designer),
                ..
            }
        )));
    }

    #[test]
    fn designer_punch_hp_loss_applies_tungsten_rod() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.current_hp = 10;
        rs.relics.push(RelicState::new(RelicId::TungstenRod));
        rs.event_state = Some(designer_state(1, 0b00));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 3);

        assert_eq!(rs.current_hp, 8);
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -2,
                current_hp: 8,
                source: DomainEventSource::Event(EventId::Designer),
                ..
            }
        )));
    }

    #[test]
    fn designer_full_service_followup_upgrade_uses_domain_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 1),
            deck_card(CardId::Defend, 12, 0),
        ];
        let mut state = designer_state(2, 0b00);
        state.extra_data = vec![1];
        rs.event_state = Some(state);

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.master_deck[1].upgrades, 1);
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                before,
                after,
                source: DomainEventSource::Event(EventId::Designer),
            } if before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12) && before.upgrades == 0 && after.upgrades == 1
        )));
    }

    #[test]
    fn designer_full_service_selection_auto_runs_followup_upgrade_like_java_update() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 99;
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 0),
            deck_card(CardId::Defend, 12, 0),
        ];
        rs.event_state = Some(designer_state(1, 0b00));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 2);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut rs,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(11)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(rs.event_state.as_ref().unwrap().completed);
        assert_eq!(rs.gold, 99 - full_service_cost(0));
        assert_eq!(
            rs.master_deck
                .iter()
                .map(|card| (card.id, card.lineage, card.upgrades))
                .collect::<Vec<_>>(),
            vec![(CardId::Defend, crate::test_support::scenario_lineage(12), 1,)]
        );
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::Designer),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11)
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                before,
                after,
                source: DomainEventSource::Event(EventId::Designer),
            } if before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12) && before.upgrades == 0 && after.upgrades == 1
        )));
    }

    #[test]
    fn designer_run_pending_choice_rejects_invalid_direct_deck_input() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 0),
            deck_card(CardId::AscendersBane, 12, 0),
        ];
        let mut engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: RunPendingChoiceReason::Transform,
            source: DomainEventSource::Event(EventId::Designer),
            return_state: Box::new(EngineState::EventRoom),
        });

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut rs,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(
                SelectionResolution::card_lineages(
                    SelectionScope::Deck,
                    [crate::test_support::scenario_lineage(12)],
                )
            )),
        ));
        assert!(matches!(engine_state, EngineState::RunPendingChoice(_)));
        assert_eq!(
            rs.master_deck
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            vec![CardId::Strike, CardId::AscendersBane]
        );
    }
}
