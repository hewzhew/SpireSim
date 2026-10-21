use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;
use crate::content::relics::RelicId;
use crate::state::core::{CombatStartRequest, EngineState, PostCombatReturn};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn act1_boss_encounter_id(index: u8) -> EncounterId {
    match index {
        0 => EncounterId::TheGuardian,
        1 => EncounterId::Hexaghost,
        _ => EncounterId::SlimeBoss,
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            // Java: 3rd option depends on floorNum % 50
            let desire_text = if run_state.floor_num % 50 <= 40 {
                "[Desire] Gain 999 Gold. Obtain 2 Normality."
            } else {
                "[Desire] Heal to full HP. Obtain Doubt."
            };
            vec![
                EventOption::new(
                    EventChoiceMeta::new("[Fight] Battle a boss for rewards."),
                    EventOptionSemantics {
                        action: EventActionKind::Fight,
                        effects: vec![],
                        constraints: vec![],
                        transition: EventOptionTransition::StartCombat,
                        repeatable: false,
                        terminal: false,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Remember] Upgrade all cards. Obtain Mark of the Bloom."),
                    EventOptionSemantics {
                        action: EventActionKind::Accept,
                        effects: vec![
                            EventEffect::UpgradeAllCards,
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::Specific(RelicId::MarkOfTheBloom),
                            },
                        ],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ),
                if run_state.floor_num % 50 <= 40 {
                    EventOption::new(
                        EventChoiceMeta::new(desire_text),
                        EventOptionSemantics {
                            action: EventActionKind::Accept,
                            effects: vec![
                                EventEffect::GainGold(999),
                                EventEffect::ObtainCurse {
                                    count: 2,
                                    kind: EventCardKind::Specific(CardId::Normality),
                                },
                            ],
                            constraints: vec![],
                            transition: EventOptionTransition::AdvanceScreen,
                            repeatable: false,
                            terminal: false,
                        },
                    )
                } else {
                    EventOption::new(
                        EventChoiceMeta::new(desire_text),
                        EventOptionSemantics {
                            action: EventActionKind::Accept,
                            effects: vec![
                                EventEffect::Heal((run_state.max_hp - run_state.current_hp).max(0)),
                                EventEffect::ObtainCurse {
                                    count: 1,
                                    kind: EventCardKind::Specific(CardId::Doubt),
                                },
                            ],
                            constraints: vec![],
                            transition: EventOptionTransition::AdvanceScreen,
                            repeatable: false,
                            terminal: false,
                        },
                    )
                },
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
            match choice_idx {
                0 => {
                    // Fight: battle Act 1 boss
                    // Java: shuffle boss list with miscRng.randomLong()
                    let mut boss_indices = [0u8, 1, 2]; // Guardian, Hexaghost, SlimeBoss
                    crate::runtime::rng::shuffle_with_random_long(
                        &mut boss_indices,
                        &mut run_state.rng_pool.misc_rng,
                    );
                    let encounter_id = act1_boss_encounter_id(boss_indices[0]);

                    // Java: addGoldToRewards(A13>=13 ? 25 : 50) + addRelicToRewards(RARE)
                    let mut rewards = crate::rewards::state::RewardState::new();
                    let gold = if run_state.ascension_level >= 13 {
                        25
                    } else {
                        50
                    };
                    rewards
                        .items
                        .push(crate::rewards::state::RewardItem::Gold { amount: gold });
                    let rare_relic =
                        run_state.random_relic_by_tier(crate::content::relics::RelicTier::Rare);
                    rewards
                        .items
                        .push(crate::rewards::state::RewardItem::Relic {
                            relic_id: rare_relic,
                        });

                    event_state.current_screen = 1;
                    event_state.completed = true;
                    run_state.event_state = Some(event_state);
                    *engine_state = EngineState::CombatStart(CombatStartRequest::event(
                        encounter_id,
                        rewards,
                        true,
                        false,
                        false,
                        PostCombatReturn::MapNavigation,
                    ));
                    return;
                }
                1 => {
                    // Remember: upgrade all upgradable cards + MarkOfTheBloom
                    // Java checks canUpgrade() and calls c.upgrade() for each card.
                    let source = DomainEventSource::Event(EventId::MindBloom);
                    let upgrade_lineages: Vec<crate::runtime::combat::CardLineageId> = run_state
                        .master_deck
                        .iter()
                        .filter(|card| crate::state::core::master_deck_card_can_upgrade(card))
                        .map(|card| card.lineage)
                        .collect();
                    for lineage in upgrade_lineages {
                        run_state.upgrade_card_with_source(lineage, source);
                    }
                    if let Some(next_state) = run_state.obtain_relic_with_source(
                        RelicId::MarkOfTheBloom,
                        EngineState::EventRoom,
                        source,
                    ) {
                        *engine_state = next_state;
                    }
                    event_state.current_screen = 1;
                }
                _ => {
                    // Desire: depends on floorNum % 50
                    if run_state.floor_num % 50 <= 40 {
                        // Normal path: 999 gold + 2 Normality
                        run_state.change_gold_with_source(
                            999,
                            DomainEventSource::Event(EventId::MindBloom),
                        );
                        super::obtain_event_card(run_state, EventId::MindBloom, CardId::Normality);
                        super::obtain_event_card(run_state, EventId::MindBloom, CardId::Normality);
                    } else {
                        // High floor path: heal to full + Doubt curse
                        run_state.heal_with_source(
                            run_state.max_hp,
                            DomainEventSource::Event(EventId::MindBloom),
                        );
                        super::obtain_event_card(run_state, EventId::MindBloom, CardId::Doubt);
                    }
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
    use crate::content::relics::RelicState;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::CombatContext;
    use crate::state::events::{EventOptionTransition, EventRelicKind};
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn deck_card(id: CardId, uuid: u32, upgrades: u8) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = upgrades;
        card
    }

    #[test]
    fn remember_option_exposes_mark_of_the_bloom_semantics() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.floor_num = 20;
        let state = EventState::new(crate::state::events::EventId::MindBloom);
        let options = get_options(&rs, &state);
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::MarkOfTheBloom),
            }));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::StartCombat
        );
    }

    #[test]
    fn remember_obtains_mark_of_the_bloom_with_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.floor_num = 20;
        rs.event_state = Some(EventState::new(EventId::MindBloom));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 1);

        assert!(rs
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::MarkOfTheBloom));
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::MarkOfTheBloom,
                source: DomainEventSource::Event(EventId::MindBloom),
            }
        )));
    }

    #[test]
    fn remember_upgrades_all_java_can_upgrade_cards_with_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.floor_num = 20;
        rs.master_deck = vec![
            deck_card(CardId::Strike, 11, 0),
            deck_card(CardId::Defend, 12, 1),
            deck_card(CardId::SearingBlow, 13, 3),
            deck_card(CardId::Injury, 14, 0),
        ];
        rs.event_state = Some(EventState::new(EventId::MindBloom));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 1);

        assert_eq!(
            rs.master_deck
                .iter()
                .map(|card| (card.id, card.upgrades))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Strike, 1),
                (CardId::Defend, 1),
                (CardId::SearingBlow, 4),
                (CardId::Injury, 0),
            ]
        );
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                before,
                after,
                source: DomainEventSource::Event(EventId::MindBloom),
            } if before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11) && before.upgrades == 0 && after.upgrades == 1
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                before,
                after,
                source: DomainEventSource::Event(EventId::MindBloom),
            } if before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(13) && before.upgrades == 3 && after.upgrades == 4
        )));
        assert!(!events.iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                before,
                source: DomainEventSource::Event(EventId::MindBloom),
                ..
            } if before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12) || before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(14)
        )));
    }

    #[test]
    fn remember_option_uses_explicit_all_card_upgrade_effect() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let state = EventState::new(EventId::MindBloom);

        let options = get_options(&rs, &state);

        assert_eq!(
            options[1].semantics.effects[0],
            EventEffect::UpgradeAllCards,
            "Mind Bloom Remember should expose an explicit all-card upgrade effect"
        );
    }

    #[test]
    fn fight_uses_java_shuffled_act1_boss_key_and_rare_relic_reward() {
        let mut rs = RunState::new(123, 0, true, "Ironclad");
        rs.floor_num = 20;
        rs.rare_relic_pool = vec![RelicId::OldCoin];
        rs.event_state = Some(EventState::new(EventId::MindBloom));
        let mut expected_indices = [0u8, 1, 2];
        let mut expected_misc_rng = rs.rng_pool.misc_rng.clone();
        crate::runtime::rng::shuffle_with_random_long(
            &mut expected_indices,
            &mut expected_misc_rng,
        );
        let expected_encounter_id = act1_boss_encounter_id(expected_indices[0]);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("Mind Bloom fight should request CombatStart");
        };
        assert_eq!(request.encounter_id, expected_encounter_id);
        let CombatContext::Event(combat) = request.context else {
            panic!("Mind Bloom fight should carry event combat context");
        };
        assert!(combat
            .rewards
            .items
            .iter()
            .any(|item| matches!(item, crate::rewards::state::RewardItem::Gold { amount: 50 })));
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Relic {
                relic_id: RelicId::OldCoin
            }
        )));
        assert_eq!(rs.rng_pool.misc_rng.counter, expected_misc_rng.counter);
    }

    #[test]
    fn high_floor_desire_heals_with_event_source_and_obtains_doubt() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.floor_num = 41;
        rs.current_hp = 10;
        rs.max_hp = 80;
        rs.event_state = Some(EventState::new(EventId::MindBloom));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 2);

        assert_eq!(rs.current_hp, 80);
        assert!(rs.master_deck.iter().any(|card| card.id == CardId::Doubt));
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 70,
                current_hp: 80,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::MindBloom),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                source: DomainEventSource::Event(EventId::MindBloom),
                card,
            } if card.id == CardId::Doubt
        )));
    }

    #[test]
    fn high_floor_desire_heal_respects_mark_of_the_bloom() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.floor_num = 41;
        rs.current_hp = 10;
        rs.max_hp = 80;
        rs.relics.push(RelicState::new(RelicId::MarkOfTheBloom));
        rs.event_state = Some(EventState::new(EventId::MindBloom));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 2);

        assert_eq!(rs.current_hp, 10);
        assert!(rs.master_deck.iter().any(|card| card.id == CardId::Doubt));
        let events = rs.take_emitted_events();
        assert!(!events
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                source: DomainEventSource::Event(EventId::MindBloom),
                card,
            } if card.id == CardId::Doubt
        )));
    }

    #[test]
    fn low_floor_desire_gold_resolves_before_normality_obtain_hooks() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.floor_num = 40;
        rs.relics.push(RelicState::new(RelicId::CeramicFish));
        rs.event_state = Some(EventState::new(EventId::MindBloom));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 2);

        let events = rs.take_emitted_events();
        let desire_gold_pos = events
            .iter()
            .position(|event| matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: 999,
                    source: DomainEventSource::Event(EventId::MindBloom),
                    ..
                }
            ))
            .expect("Mind Bloom low-floor Desire should gain 999 gold before queued Normalities resolve");
        let fish_gold_positions = events
            .iter()
            .enumerate()
            .filter_map(|(idx, event)| match event {
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::MindBloom),
                    ..
                } => Some(idx),
                _ => None,
            })
            .collect::<Vec<_>>();
        let normality_positions = events
            .iter()
            .enumerate()
            .filter_map(|(idx, event)| match event {
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::MindBloom),
                } if card.id == CardId::Normality => Some(idx),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(fish_gold_positions.len(), 2);
        assert_eq!(normality_positions.len(), 2);
        assert!(
            desire_gold_pos < fish_gold_positions[0]
                && fish_gold_positions[0] < normality_positions[0]
                && normality_positions[0] < fish_gold_positions[1]
                && fish_gold_positions[1] < normality_positions[1],
            "Java MindBloom gains 999 gold immediately, then each queued Normality ShowCardAndObtainEffect runs onObtainCard before Soul.obtain"
        );
    }
}
