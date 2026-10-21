use crate::content::cards::CardId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

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
            let hp_loss_pct = if run_state.ascension_level >= 15 {
                0.18
            } else {
                0.125
            };
            let hp_loss = (run_state.max_hp as f32 * hp_loss_pct).round() as i32;
            let heal_pct = if run_state.ascension_level >= 15 {
                0.20
            } else {
                0.25
            };
            let heal_amt = (run_state.max_hp as f32 * heal_pct).round() as i32;
            let max_hp_loss = (run_state.max_hp as f32 * 0.05).round() as i32;
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Embrace] Lose {} HP. Obtain 2 Madness.",
                        hp_loss
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Accept,
                        effects: vec![
                            EventEffect::LoseHp(hp_loss),
                            EventEffect::ObtainCard {
                                count: 2,
                                kind: EventCardKind::Specific(CardId::Madness),
                            },
                        ],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Retrace] Heal {} HP. Become Cursed - Writhe.",
                        heal_amt
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Accept,
                        effects: vec![
                            EventEffect::Heal(heal_amt),
                            EventEffect::ObtainCurse {
                                count: 1,
                                kind: EventCardKind::Specific(CardId::Writhe),
                            },
                        ],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!("[Accept] Lose {} Max HP.", max_hp_loss)),
                    EventOptionSemantics {
                        action: EventActionKind::Accept,
                        effects: vec![EventEffect::LoseMaxHp(max_hp_loss)],
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

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            event_state.current_screen = 1;
        }
        1 => {
            match choice_idx {
                0 => {
                    // Embrace Madness: Java DamageInfo(null, hpAmt) + 2 Madness.
                    let hp_loss_pct = if run_state.ascension_level >= 15 {
                        0.18
                    } else {
                        0.125
                    };
                    let hp_loss = (run_state.max_hp as f32 * hp_loss_pct).round() as i32;
                    super::apply_player_default_damage(
                        run_state,
                        hp_loss,
                        super::EventDamageOwner::None,
                        DomainEventSource::Event(EventId::WindingHalls),
                    );
                    super::obtain_event_card(run_state, EventId::WindingHalls, CardId::Madness);
                    super::obtain_event_card(run_state, EventId::WindingHalls, CardId::Madness);
                    event_state.current_screen = 2;
                }
                1 => {
                    // Retrace: heal + Writhe
                    let heal_pct = if run_state.ascension_level >= 15 {
                        0.20
                    } else {
                        0.25
                    };
                    let heal_amt = (run_state.max_hp as f32 * heal_pct).round() as i32;
                    run_state.heal_with_source(
                        heal_amt,
                        DomainEventSource::Event(EventId::WindingHalls),
                    );
                    super::obtain_event_card(run_state, EventId::WindingHalls, CardId::Writhe);
                    event_state.current_screen = 2;
                }
                _ => {
                    // Accept: lose Max HP
                    let max_hp_loss = (run_state.max_hp as f32 * 0.05).round() as i32;
                    run_state.lose_max_hp_with_source(
                        max_hp_loss,
                        DomainEventSource::Event(EventId::WindingHalls),
                    );
                    event_state.current_screen = 2;
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
    use crate::state::core::EngineState;
    use crate::state::events::{EventId, EventState};
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn winding_run(current_hp: i32, max_hp: i32, ascension_level: u8) -> RunState {
        let mut run_state = RunState::new(1, ascension_level, false, "Ironclad");
        run_state.current_hp = current_hp;
        run_state.max_hp = max_hp;
        let mut event_state = EventState::new(EventId::WindingHalls);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn embrace_madness_damage_uses_event_source_and_obtains_two_madness() {
        let mut run_state = winding_run(20, 80, 0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 10);
        assert_eq!(
            run_state
                .master_deck
                .iter()
                .filter(|card| card.id == CardId::Madness)
                .count(),
            2
        );
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -10,
                current_hp: 10,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::WindingHalls),
            }
        )));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::WindingHalls),
                    } if card.id == CardId::Madness
                ))
                .count(),
            2
        );
    }

    #[test]
    fn embrace_madness_damage_applies_tungsten_rod() {
        let mut run_state = winding_run(20, 80, 0);
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 11);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -9,
                current_hp: 11,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::WindingHalls),
            }
        )));
    }

    #[test]
    fn retrace_heal_uses_event_source_and_obtains_writhe() {
        let mut run_state = winding_run(10, 80, 0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 30);
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Writhe));
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 20,
                current_hp: 30,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::WindingHalls),
            }
        )));
    }

    #[test]
    fn retrace_heal_respects_mark_of_the_bloom_but_still_obtains_writhe() {
        let mut run_state = winding_run(10, 80, 0);
        run_state
            .relics
            .push(RelicState::new(RelicId::MarkOfTheBloom));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 10);
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Writhe));
        assert!(!run_state
            .take_emitted_events()
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
    }

    #[test]
    fn accept_loss_uses_max_hp_event_source_and_clamps_current_hp() {
        let mut run_state = winding_run(80, 80, 0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert_eq!(run_state.max_hp, 76);
        assert_eq!(run_state.current_hp, 76);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::MaxHpChanged {
                delta: -4,
                current_hp: 76,
                max_hp: 76,
                source: DomainEventSource::Event(EventId::WindingHalls),
            }
        )));
    }

    #[test]
    fn embrace_damage_resolves_before_delayed_madness_obtains_like_java_effect_list() {
        let mut run_state = winding_run(20, 80, 0);
        run_state.gold = 0;
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 10);
        assert_eq!(run_state.gold, 18);
        let labels = run_state
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::HpChanged {
                    delta: -10,
                    source: DomainEventSource::Event(EventId::WindingHalls),
                    ..
                } => Some("hp_loss"),
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::WindingHalls),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::WindingHalls),
                } if card.id == CardId::Madness => Some("madness_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "hp_loss",
                "ceramic_fish_gold",
                "madness_obtained",
                "ceramic_fish_gold",
                "madness_obtained",
            ],
            "Java damages immediately, then each queued ShowCardAndObtainEffect later runs onObtainCard before Soul.obtain"
        );
    }
}
