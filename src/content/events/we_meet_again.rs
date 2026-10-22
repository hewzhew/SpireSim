use crate::runtime::combat::CardLineageId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

const NO_POTION_SLOT: i32 = 0xFF;
const NO_CARD_LINEAGE: i32 = -1;

fn gold_amount(event_state: &EventState) -> i32 {
    event_state.internal_state & 0xFF
}

fn potion_slot(event_state: &EventState) -> Option<usize> {
    let slot = (event_state.internal_state >> 8) & 0xFF;
    if slot == NO_POTION_SLOT {
        None
    } else {
        Some(slot as usize)
    }
}

fn card_lineage(event_state: &EventState) -> Option<CardLineageId> {
    event_state
        .extra_data
        .first()
        .copied()
        .filter(|&lineage| lineage != NO_CARD_LINEAGE)
        .and_then(CardLineageId::from_runtime_i32_payload)
}

fn card_by_lineage(
    run_state: &RunState,
    lineage: CardLineageId,
) -> Option<&crate::runtime::combat::CombatCard> {
    run_state
        .master_deck
        .iter()
        .find(|card| card.lineage == lineage)
}

fn potion_by_slot(
    run_state: &RunState,
    slot: Option<usize>,
) -> Option<&crate::content::potions::Potion> {
    slot.and_then(|slot| run_state.potions.get(slot))
        .and_then(|potion| potion.as_ref())
}

fn potion_label(potion: &crate::content::potions::Potion) -> &'static str {
    crate::content::potions::get_potion_definition(potion.id).name
}

fn card_label(card: &crate::runtime::combat::CombatCard) -> String {
    let name = crate::content::cards::get_card_definition(card.id).name;
    if card.upgrades == 0 {
        name.to_string()
    } else {
        format!("{name}+{}", card.upgrades)
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let selected_potion = potion_by_slot(run_state, potion_slot(event_state));
            let gold_amt = gold_amount(event_state);
            let has_gold = gold_amt > 0;
            let card_lineage = card_lineage(event_state);
            let selected_card =
                card_lineage.and_then(|lineage| card_by_lineage(run_state, lineage));

            vec![
                if let Some(potion) = selected_potion {
                    EventOption::new(
                        EventChoiceMeta::new(format!(
                            "[Give Potion] Give {}. Obtain a Relic.",
                            potion_label(potion)
                        ))
                        .with_public_detail(
                            crate::state::events::EventOptionPublicDetail::RequestedPotion {
                                potion: potion.id,
                            },
                        ),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomRelic,
                            }],
                            constraints: vec![EventOptionConstraint::RequiresPotion],
                            transition: EventOptionTransition::AdvanceScreen,
                            repeatable: false,
                            terminal: false,
                        },
                    )
                } else {
                    EventOption::new(
                        EventChoiceMeta::disabled("[Give Potion]", "No Potions"),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomRelic,
                            }],
                            constraints: vec![EventOptionConstraint::RequiresPotion],
                            transition: EventOptionTransition::AdvanceScreen,
                            repeatable: false,
                            terminal: false,
                        },
                    )
                },
                if has_gold {
                    EventOption::new(
                        EventChoiceMeta::new(format!(
                            "[Give Gold] Lose {} Gold. Obtain a Relic.",
                            gold_amt
                        )),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![
                                EventEffect::LoseGold(gold_amt),
                                EventEffect::ObtainRelic {
                                    count: 1,
                                    kind: EventRelicKind::RandomRelic,
                                },
                            ],
                            constraints: vec![EventOptionConstraint::RequiresGold(gold_amt)],
                            transition: EventOptionTransition::AdvanceScreen,
                            repeatable: false,
                            terminal: false,
                        },
                    )
                } else {
                    EventOption::new(
                        EventChoiceMeta::disabled("[Give Gold]", "Not enough Gold"),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomRelic,
                            }],
                            constraints: vec![EventOptionConstraint::RequiresGold(50)],
                            transition: EventOptionTransition::AdvanceScreen,
                            repeatable: false,
                            terminal: false,
                        },
                    )
                },
                if let Some(card) = selected_card {
                    EventOption::new(
                        EventChoiceMeta::new(format!(
                            "[Give Card] Give {}. Obtain a Relic.",
                            card_label(card)
                        )),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![
                                card_remove_effect(run_state, card_lineage),
                                EventEffect::ObtainRelic {
                                    count: 1,
                                    kind: EventRelicKind::RandomRelic,
                                },
                            ],
                            constraints: vec![EventOptionConstraint::RequiresRemovableCard],
                            transition: EventOptionTransition::AdvanceScreen,
                            repeatable: false,
                            terminal: false,
                        },
                    )
                } else {
                    EventOption::new(
                        EventChoiceMeta::disabled("[Give Card]", "No eligible cards"),
                        EventOptionSemantics {
                            action: EventActionKind::Trade,
                            effects: vec![EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomRelic,
                            }],
                            constraints: vec![EventOptionConstraint::RequiresRemovableCard],
                            transition: EventOptionTransition::AdvanceScreen,
                            repeatable: false,
                            terminal: false,
                        },
                    )
                },
                EventOption::new(
                    EventChoiceMeta::new("[Attack]"),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        effects: vec![],
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

fn card_remove_effect(run_state: &RunState, lineage: Option<CardLineageId>) -> EventEffect {
    match lineage.and_then(|lineage| card_by_lineage(run_state, lineage)) {
        Some(card) => EventEffect::RemoveCard {
            count: 1,
            target_lineage: Some(card.lineage),
            kind: EventCardKind::Specific(card.id),
        },
        None => EventEffect::RemoveCard {
            count: 1,
            target_lineage: None,
            kind: EventCardKind::Unknown,
        },
    }
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

fn obtain_event_relic(engine_state: &mut EngineState, run_state: &mut RunState) {
    let relic_id = run_state.random_screenless_relic_reward();
    if let Some(next_state) = run_state.obtain_relic_with_source(
        relic_id,
        EngineState::EventRoom,
        DomainEventSource::Event(EventId::WeMeetAgain),
    ) {
        *engine_state = next_state;
    }
}

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            match choice_idx {
                0 => {
                    // Give potion → relic
                    if let Some(potion_slot) = potion_slot(&event_state) {
                        if run_state
                            .potions
                            .get(potion_slot)
                            .and_then(|potion| potion.as_ref())
                            .is_some()
                        {
                            run_state.remove_potion_at_with_source(
                                potion_slot,
                                DomainEventSource::Event(EventId::WeMeetAgain),
                            );
                            event_state.current_screen = 1;
                            obtain_event_relic(_engine_state, run_state);
                        }
                    }
                }
                1 => {
                    // Give gold → relic
                    let amt = gold_amount(&event_state);
                    if amt > 0 && run_state.gold >= amt {
                        run_state.change_gold_with_source(
                            -amt,
                            DomainEventSource::Event(EventId::WeMeetAgain),
                        );
                        event_state.current_screen = 1;
                        obtain_event_relic(_engine_state, run_state);
                    }
                }
                2 => {
                    // Give card → relic
                    if let Some(lineage) = card_lineage(&event_state) {
                        if card_by_lineage(run_state, lineage).is_some() {
                            run_state.remove_card_from_deck_with_source(
                                lineage,
                                DomainEventSource::Event(EventId::WeMeetAgain),
                            );
                            event_state.current_screen = 1;
                            obtain_event_relic(_engine_state, run_state);
                        }
                    }
                }
                _ => {
                    // Attack (leave)
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

/// Initialize WeMeetAgain state.
/// Java constructor RNG call order:
///   1. getRandomPotion() → Collections.shuffle(list, new Random(miscRng.randomLong()))
///   2. getGoldAmount() → miscRng.random(50, min(gold, 150)) if gold >= 50
///   3. getRandomNonBasicCard() → Collections.shuffle(list, new Random(miscRng.randomLong()))
///
/// internal_state packing:
///   byte 0 (bits 0-7):   goldAmount (0-150, or 0 = none)
///   byte 1 (bits 8-15):  potion slot index (or 0xFF = none)
/// extra_data:
///   [0] = card lineage selected by Java-like `cardOption` object reference, or -1.
pub fn init_we_meet_again_event_state(run_state: &mut RunState, event_state: &mut EventState) {
    // 1. Random potion: Java getRandomPotion() shuffles via miscRng.randomLong()
    let potion_slot: i32 = {
        let potion_indices: Vec<usize> = run_state
            .potions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_some())
            .map(|(i, _)| i)
            .collect();
        if potion_indices.is_empty() {
            NO_POTION_SLOT // no potion — Java also skips randomLong when no potions
        } else {
            // Consume miscRng.randomLong() for shuffle seed, pick first after shuffle
            let mut shuffled = potion_indices;
            crate::runtime::rng::shuffle_with_random_long(
                &mut shuffled,
                &mut run_state.rng_pool.misc_rng,
            );
            shuffled[0] as i32
        }
    };

    // 2. Gold amount: Java miscRng.random(50, min(gold, 150))
    let gold_amount: u8 = if run_state.gold < 50 {
        0
    } else {
        let cap = if run_state.gold > 150 {
            150
        } else {
            run_state.gold
        };
        run_state.rng_pool.misc_rng.random_range(50, cap) as u8
    };

    // 3. Random non-basic card: shuffle with randomLong then pick [0]
    let mut eligible_indices: Vec<usize> = run_state
        .master_deck
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            let def = crate::content::cards::get_card_definition(c.id);
            def.rarity != crate::content::cards::CardRarity::Basic
                && def.card_type != crate::content::cards::CardType::Curse
        })
        .map(|(i, _)| i)
        .collect();

    let card_lineage = if eligible_indices.is_empty() {
        // Still consume randomLong? No — Java returns null if list is empty, no shuffle
        NO_CARD_LINEAGE
    } else {
        crate::runtime::rng::shuffle_with_random_long(
            &mut eligible_indices,
            &mut run_state.rng_pool.misc_rng,
        );
        run_state.master_deck[eligible_indices[0]]
            .lineage
            .to_runtime_i32_payload()
    };

    event_state.internal_state = (gold_amount as i32) | (potion_slot << 8);
    event_state.extra_data = vec![card_lineage];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::potions::{Potion, PotionId};
    use crate::runtime::combat::CombatCard;
    use crate::state::events::EventOptionConstraint;
    use crate::state::selection::DomainEvent;

    #[test]
    fn gold_trade_option_exposes_required_gold_constraint() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: 75 | (NO_POTION_SLOT << 8),
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![NO_CARD_LINEAGE],
        };
        let options = get_options(&rs, &event_state);
        assert_eq!(
            options[1].semantics.constraints,
            vec![EventOptionConstraint::RequiresGold(75)]
        );
    }

    #[test]
    fn card_trade_option_exposes_specific_remove_effect() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![CombatCard::for_scenario(CardId::ShrugItOff, 11)];
        let event_state = EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: NO_POTION_SLOT << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![crate::test_support::scenario_lineage(11).to_runtime_i32_payload()],
        };

        let options = get_options(&rs, &event_state);

        assert!(matches!(
            options[2].semantics.effects.as_slice(),
            [
                EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: Some(target_lineage),
                    kind: EventCardKind::Specific(CardId::ShrugItOff),
                },
                EventEffect::ObtainRelic { .. }
            ] if *target_lineage == crate::test_support::scenario_lineage(11)
        ));
    }

    #[test]
    fn card_trade_option_names_the_requested_card() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![CombatCard::for_scenario(CardId::ShrugItOff, 11)];
        let event_state = EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: NO_POTION_SLOT << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![crate::test_support::scenario_lineage(11).to_runtime_i32_payload()],
        };

        let options = get_options(&rs, &event_state);

        assert!(
            options[2].ui.text.contains("Shrug It Off"),
            "Java shows the exact requested card in the button text: {:?}",
            options[2].ui.text
        );
    }

    #[test]
    fn potion_trade_option_names_the_requested_potion() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.potions[1] = Some(Potion::new(PotionId::FirePotion, 91));
        let event_state = EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: 1 << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![NO_CARD_LINEAGE],
        };

        let options = get_options(&rs, &event_state);

        assert!(
            options[0].ui.text.contains("Fire Potion"),
            "Java shows the exact requested potion in the button text: {:?}",
            options[0].ui.text
        );
    }

    #[test]
    fn stale_potion_slot_disables_potion_trade() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: 1 << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![NO_CARD_LINEAGE],
        };

        let options = get_options(&rs, &event_state);

        assert!(
            options[0].ui.disabled,
            "We Meet Again should not expose a potion trade when the stored slot no longer contains a potion"
        );
    }

    #[test]
    fn card_trade_removes_card_and_obtains_relic_with_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![CombatCard::for_scenario(CardId::ShrugItOff, 11)];
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: NO_POTION_SLOT << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![crate::test_support::scenario_lineage(11).to_runtime_i32_payload()],
        });

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 2);

        assert!(rs.master_deck.is_empty());
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::WeMeetAgain),
            } if card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11)
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                source: DomainEventSource::Event(EventId::WeMeetAgain),
                ..
            }
        )));
    }

    #[test]
    fn card_trade_uses_stored_card_lineage_instead_of_packed_deck_index() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = (0..300)
            .map(|idx| CombatCard::for_scenario(CardId::Strike, 10_000 + idx))
            .collect();
        rs.master_deck
            .push(CombatCard::for_scenario(CardId::ShrugItOff, 90_101));
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: NO_POTION_SLOT << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![crate::test_support::scenario_lineage(90_101).to_runtime_i32_payload()],
        });

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 2);

        assert!(!rs.master_deck.iter().any(|card| card.lineage
            == crate::runtime::combat::CardLineageId::constructed_scenario(90_101)));
        assert_eq!(
            rs.master_deck.len(),
            300,
            "Java stores a card object reference; Rust must not truncate that to an 8-bit deck index"
        );
    }

    #[test]
    fn potion_trade_removes_selected_potion_and_obtains_relic_with_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.potions[1] = Some(Potion::new(PotionId::FirePotion, 91));
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: 1 << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![NO_CARD_LINEAGE],
        });

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        assert!(rs.potions[1].is_none());
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::PotionLost {
                potion_id: PotionId::FirePotion,
                slot: 1,
                source: DomainEventSource::Event(EventId::WeMeetAgain),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                source: DomainEventSource::Event(EventId::WeMeetAgain),
                ..
            }
        )));
    }

    #[test]
    fn disabled_potion_trade_does_not_grant_free_relic() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: NO_POTION_SLOT << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![NO_CARD_LINEAGE],
        });
        rs.emitted_events.clear();
        let relic_count = rs.relics.len();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.relics.len(), relic_count);
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(rs.take_emitted_events().is_empty());
    }

    #[test]
    fn disabled_gold_trade_does_not_grant_free_relic() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 40;
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: NO_POTION_SLOT << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![NO_CARD_LINEAGE],
        });
        rs.emitted_events.clear();
        let relic_count = rs.relics.len();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 1);

        assert_eq!(rs.gold, 40);
        assert_eq!(rs.relics.len(), relic_count);
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(rs.take_emitted_events().is_empty());
    }

    #[test]
    fn disabled_card_trade_does_not_grant_free_relic() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: NO_POTION_SLOT << 8,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![NO_CARD_LINEAGE],
        });
        rs.emitted_events.clear();
        let relic_count = rs.relics.len();
        let deck_len = rs.master_deck.len();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 2);

        assert_eq!(rs.master_deck.len(), deck_len);
        assert_eq!(rs.relics.len(), relic_count);
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(rs.take_emitted_events().is_empty());
    }
}
