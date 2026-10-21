use crate::content::cards::{get_card_definition, CardId, CardRarity};
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventCardState, EventChoiceMeta, EventEffect, EventId,
    EventOption, EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

/// TheLibrary event.
/// Java: 2 options:
///   [Read] Generate 20 unique class cards (rollRarity + getCard, deduped), player picks 1
///   [Sleep] Heal 33% HP (A15: 20% HP)
///
/// Screen 0: initial choice (Read / Sleep)
/// Screen 1: 20 cards to choose from (only when Read was picked)
/// Screen 2: Leave
fn library_heal_amount(run_state: &RunState) -> i32 {
    let heal_pct = if run_state.ascension_level >= 15 {
        0.20
    } else {
        0.33
    };
    (run_state.max_hp as f32 * heal_pct).round() as i32
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let heal_amt = library_heal_amount(run_state);
            vec![
                EventOption::new(
                    EventChoiceMeta::new("[Read] Choose a card from 20 offerings."),
                    EventOptionSemantics {
                        action: EventActionKind::Gain,
                        effects: vec![EventEffect::OfferCards {
                            count: 20,
                            kind: EventCardKind::RandomClassCard,
                        }],
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!("[Sleep] Heal {} HP.", heal_amt)),
                    EventOptionSemantics {
                        action: EventActionKind::Gain,
                        effects: vec![EventEffect::Heal(heal_amt)],
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
            ]
        }
        1 => {
            // Show the 20 typed card offerings retained by this event.
            let mut options = Vec::with_capacity(20);
            for idx in 0..event_state.card_payload.len() {
                let Some((card_id, upgrades)) =
                    library_card_entry_at(&event_state.card_payload, idx)
                else {
                    continue;
                };
                let def = get_card_definition(card_id);
                let upgrade_suffix = if upgrades > 0 { "+" } else { "" };
                options.push(EventOption::new(
                    EventChoiceMeta::new(format!(
                        "{}{} ({:?} {:?})",
                        def.name, upgrade_suffix, def.rarity, def.card_type
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Gain,
                        effects: vec![EventEffect::ObtainCard {
                            count: 1,
                            kind: EventCardKind::Specific(card_id),
                        }],
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ));
            }
            options
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

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            match choice_idx {
                0 => {
                    // Read: generate 20 cards and show them
                    generate_library_cards(run_state, &mut event_state.card_payload);
                    event_state.current_screen = 1;
                }
                _ => {
                    // Sleep: heal
                    let heal_amt = library_heal_amount(run_state);
                    run_state
                        .heal_with_source(heal_amt, DomainEventSource::Event(EventId::TheLibrary));
                    event_state.current_screen = 2;
                }
            }
        }
        1 => {
            // Pick one of the 20 cards
            if let Some((card_id, upgrades)) =
                library_card_entry_at(&event_state.card_payload, choice_idx)
            {
                run_state.add_card_to_deck_with_upgrades_from(
                    card_id,
                    upgrades,
                    DomainEventSource::Event(EventId::TheLibrary),
                );
            }
            event_state.current_screen = 2;
        }
        _ => {
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

/// Generate 20 unique class cards via Java's rollRarity + getCard with dedup.
///
/// Java logic (TheLibrary.buttonEffect case 0):
///   for (int i = 0; i < 20; ++i) {
///       card = getCard(rollRarity());
///       while (group contains card) {
///           card = getCard(rollRarity());  // re-roll both rarity and card
///       }
///       group.addToBottom(card);
///   }
///
/// rollRarity() uses cardRng.random(99) + cardBlizzRandomizer
/// getCard(rarity) uses cardRng.random(pool.size()-1) via pool.getRandomCard(true)
fn generate_library_cards(run_state: &mut RunState, cards: &mut Vec<EventCardState>) {
    cards.clear();

    let mut selected: Vec<CardId> = Vec::with_capacity(20);

    for _ in 0..20 {
        let mut card_id = roll_and_get_card(run_state);

        // Dedup: re-roll if we already have this card
        // Java does while(containsDupe) { re-roll both rarity and card }
        // Safety limit to prevent infinite loop (shouldn't happen with 70+ card pool)
        let mut attempts = 0;
        while selected.contains(&card_id) && attempts < 100 {
            card_id = roll_and_get_card(run_state);
            attempts += 1;
        }

        selected.push(card_id);
        let upgrades = run_state.preview_obtain_card_upgrades(card_id, 0);
        // CardGroup.addToBottom inserts at group index zero. The grid exposes
        // that group directly, so the visible offers reverse generation order.
        cards.insert(0, EventCardState { card_id, upgrades });
    }
}

/// Resolves one visible Library entry to its exact card identity and upgrade.
pub fn library_card_entry_at(cards: &[EventCardState], idx: usize) -> Option<(CardId, u8)> {
    cards.get(idx).map(|card| (card.card_id, card.upgrades))
}

/// Roll a rarity and get a random card from that rarity pool.
/// Mirrors Java: rollRarity() + getCard(rarity)
///
/// rollRarity: cardRng.random(0,99) + cardBlizzRandomizer
///   - roll < 3 → RARE (fallback rates, since we're in event room)
///   - roll < 40 → UNCOMMON
///   - else → COMMON
///
/// getCard(rarity): pool.getRandomCard(true) → cardRng.random(pool.size()-1)
fn roll_and_get_card(run_state: &mut RunState) -> CardId {
    // Step 1: rollRarity — uses cardRng
    let roll = run_state.rng_pool.card_rng.random(99) + run_state.card_blizz_randomizer;

    // Event room uses fallback rarity rates (no combat room rarity adjustments)
    let rarity = if roll < 3 {
        CardRarity::Rare
    } else if roll < 40 {
        CardRarity::Uncommon
    } else {
        CardRarity::Common
    };

    // Step 2: getCard(rarity) — uses cardRng via pool.getRandomCard(true)
    let pool = crate::engine::campfire_handler::nonempty_card_pool_for_class(
        run_state.player_class,
        rarity,
    );
    if pool.is_empty() {
        return match run_state.player_class {
            "Silent" => CardId::StrikeG,
            _ => CardId::Strike,
        };
    }
    let idx = run_state.rng_pool.card_rng.random(pool.len() as i32 - 1) as usize;
    pool[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardType;
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventOptionTransition,
    };
    use crate::state::selection::DomainEvent;

    #[test]
    fn recorded_library_rng_produces_the_original_visible_offer_order() {
        // PC 2.3.4, CommunicationMod e8c1dfb, adjacent natural Library frames
        // 278 -> 279. Only the owning card RNG, rarity offset and expected
        // offer page are retained; this is not a saved run or replay chain.
        let mut rs = RunState::new(1, 20, true, "Ironclad");
        rs.rng_pool.card_rng = crate::runtime::rng::StsRng::from_state(
            0x9680_C1EC_C1A2_25CF,
            0x2D40_A687_497A_F495,
            271,
        );
        rs.card_blizz_randomizer = 2;
        let mut cards = Vec::new();
        generate_library_cards(&mut rs, &mut cards);
        assert_eq!(
            cards.iter().map(|card| card.card_id).collect::<Vec<_>>(),
            [
                CardId::Shockwave,
                CardId::ThunderClap,
                CardId::Entrench,
                CardId::Carnage,
                CardId::Armaments,
                CardId::Cleave,
                CardId::BattleTrance,
                CardId::Clothesline,
                CardId::PerfectedStrike,
                CardId::HeavyBlade,
                CardId::Anger,
                CardId::Inflame,
                CardId::FireBreathing,
                CardId::WildStrike,
                CardId::TwinStrike,
                CardId::Flex,
                CardId::Combust,
                CardId::Havoc,
                CardId::SwordBoomerang,
                CardId::TrueGrit,
            ]
        );
        assert!(cards.iter().all(|card| card.upgrades == 0));
        assert_eq!(
            rs.rng_pool.card_rng,
            crate::runtime::rng::StsRng::from_state(
                0x6E7F_F0AF_14F0_3D41,
                0x8D22_1321_2D7A_2551,
                325
            )
        );
    }

    #[test]
    fn structured_options_expose_read_sleep_and_specific_book_card_choices() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.current_hp = 10;
        rs.max_hp = 80;
        rs.event_state = Some(EventState::new(EventId::TheLibrary));

        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Gain);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::OfferCards {
                count: 20,
                kind: EventCardKind::RandomClassCard,
            }));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Gain);
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::Heal(26)));

        let mut event_state = EventState::new(EventId::TheLibrary);
        event_state.current_screen = 1;
        event_state.card_payload.push(EventCardState {
            card_id: CardId::ShrugItOff,
            upgrades: 1,
        });
        let card_options = get_options(&rs, &event_state);
        assert_eq!(card_options.len(), 1);
        assert_eq!(card_options[0].semantics.action, EventActionKind::Gain);
        assert!(card_options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainCard {
                count: 1,
                kind: EventCardKind::Specific(CardId::ShrugItOff),
            }));
        assert_eq!(
            card_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut complete = EventState::new(EventId::TheLibrary);
        complete.current_screen = 2;
        let complete_options = get_options(&rs, &complete);
        assert_eq!(complete_options[0].semantics.action, EventActionKind::Leave);
        assert!(complete_options[0].semantics.terminal);
    }

    #[test]
    fn structured_sleep_uses_a15_heal_amount() {
        let mut rs = RunState::new(1, 15, true, "Ironclad");
        rs.max_hp = 80;
        rs.event_state = Some(EventState::new(EventId::TheLibrary));

        let options = get_options(&rs, rs.event_state.as_ref().unwrap());

        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::Heal(16)));
    }

    #[test]
    fn read_preserves_preview_obtain_upgrades_and_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.relics.push(RelicState::new(RelicId::MoltenEgg));
        rs.relics.push(RelicState::new(RelicId::ToxicEgg));
        rs.relics.push(RelicState::new(RelicId::FrozenEgg));
        rs.event_state = Some(EventState::new(EventId::TheLibrary));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 0);

        let event_state = rs.event_state.as_ref().unwrap();
        assert_eq!(event_state.current_screen, 1);
        assert_eq!(event_state.card_payload.len(), 20);
        assert_eq!(get_choices(&rs, event_state).len(), 20);

        for idx in 0..event_state.card_payload.len() {
            let (card_id, upgrades) =
                library_card_entry_at(&event_state.card_payload, idx).unwrap();
            let def = get_card_definition(card_id);
            assert!(matches!(
                def.card_type,
                CardType::Attack | CardType::Skill | CardType::Power
            ));
            assert_eq!(
                upgrades, 1,
                "Library must store each candidate after Java onPreviewObtainCard hooks"
            );
        }

        let (selected_id, selected_upgrades) =
            library_card_entry_at(&event_state.card_payload, 0).unwrap();
        handle_choice(&mut engine_state, &mut rs, 0);

        let obtained = rs.master_deck.last().unwrap();
        assert_eq!(obtained.id, selected_id);
        assert_eq!(
            obtained.upgrades, selected_upgrades,
            "selected preview-upgraded copy must not be upgraded again on obtain"
        );
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::TheLibrary),
            } if card.id == selected_id && card.upgrades == selected_upgrades
        )));
    }

    #[test]
    fn sleep_heals_through_player_heal_semantics_and_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.current_hp = 10;
        rs.max_hp = 80;
        rs.event_state = Some(EventState::new(EventId::TheLibrary));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 1);

        assert_eq!(rs.current_hp, 36);
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 26,
                current_hp: 36,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::TheLibrary),
            }
        )));
    }

    #[test]
    fn sleep_is_blocked_by_mark_of_the_bloom_like_java_player_heal() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.current_hp = 10;
        rs.max_hp = 80;
        rs.relics.push(RelicState::new(RelicId::MarkOfTheBloom));
        rs.event_state = Some(EventState::new(EventId::TheLibrary));

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 1);

        assert_eq!(rs.current_hp, 10);
        assert!(!rs
            .take_emitted_events()
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
    }

    #[test]
    fn selected_book_card_runs_obtain_hooks_before_card_obtained_event() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.relics.push(RelicState::new(RelicId::CeramicFish));
        rs.event_state = Some(EventState::new(EventId::TheLibrary));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);
        let selected_id = {
            let event_state = rs.event_state.as_ref().unwrap();
            library_card_entry_at(&event_state.card_payload, 0)
                .unwrap()
                .0
        };
        handle_choice(&mut engine_state, &mut rs, 0);

        let events = rs.take_emitted_events();
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::TheLibrary),
                        ..
                    }
                )
            })
            .expect("Ceramic Fish should run when the selected Library card is obtained");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::TheLibrary),
                    } if card.id == selected_id
                )
            })
            .expect("Library should obtain the selected card through ShowCardAndObtainEffect");

        assert!(
            fish_gold_pos < obtained_pos,
            "Java TheLibrary copies the selected preview card into ShowCardAndObtainEffect; that effect runs onObtainCard before Soul.obtain"
        );
    }
}
