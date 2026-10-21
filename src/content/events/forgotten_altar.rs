use crate::content::cards::CardId;
use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let has_idol = run_state.relics.iter().any(|r| r.id == RelicId::GoldenIdol);
            let hp_loss_pct = if run_state.ascension_level >= 15 {
                0.35
            } else {
                0.25
            };
            let hp_loss = (run_state.max_hp as f32 * hp_loss_pct).round() as i32;
            let mut choices = vec![];
            if has_idol {
                choices.push(EventOption::new(
                    EventChoiceMeta::new("[Offer] Trade Golden Idol for Bloody Idol."),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseRelic {
                                specific: Some(RelicId::GoldenIdol),
                                starter_only: false,
                            },
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::Specific(RelicId::BloodyIdol),
                            },
                        ],
                        constraints: vec![EventOptionConstraint::RequiresRelic(
                            RelicId::GoldenIdol,
                        )],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            } else {
                choices.push(EventOption::new(
                    EventChoiceMeta::disabled("[Offer] Requires Golden Idol.", "No Golden Idol"),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseRelic {
                                specific: Some(RelicId::GoldenIdol),
                                starter_only: false,
                            },
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::Specific(RelicId::BloodyIdol),
                            },
                        ],
                        constraints: vec![EventOptionConstraint::RequiresRelic(
                            RelicId::GoldenIdol,
                        )],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            }
            choices.push(EventOption::new(
                EventChoiceMeta::new(format!("[Pray] Gain 5 Max HP. Lose {} HP.", hp_loss)),
                EventOptionSemantics {
                    action: EventActionKind::Accept,
                    effects: vec![EventEffect::GainMaxHp(5), EventEffect::LoseHp(hp_loss)],
                    constraints: vec![],
                    transition: EventOptionTransition::AdvanceScreen,
                    repeatable: false,
                    terminal: false,
                },
            ));
            choices.push(EventOption::new(
                EventChoiceMeta::new("[Desecrate] Become Cursed - Decay."),
                EventOptionSemantics {
                    action: EventActionKind::Decline,
                    effects: vec![EventEffect::ObtainCurse {
                        count: 1,
                        kind: EventCardKind::Specific(CardId::Decay),
                    }],
                    constraints: vec![],
                    transition: EventOptionTransition::AdvanceScreen,
                    repeatable: false,
                    terminal: false,
                },
            ));
            choices
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
                    // Trade Golden Idol for Bloody Idol
                    let source = DomainEventSource::Event(EventId::ForgottenAltar);
                    if let Some(pos) = run_state
                        .relics
                        .iter()
                        .position(|r| r.id == RelicId::GoldenIdol)
                    {
                        if run_state.relics.iter().any(|r| r.id == RelicId::BloodyIdol) {
                            if let Some(next_state) = run_state.obtain_relic_with_source(
                                RelicId::Circlet,
                                EngineState::EventRoom,
                                source,
                            ) {
                                *engine_state = next_state;
                            }
                        } else {
                            let _ = run_state.remove_relic_at_with_source(pos, source);
                            if let Some(next_state) = run_state.obtain_relic_at_with_source(
                                RelicId::BloodyIdol,
                                pos,
                                EngineState::EventRoom,
                                source,
                            ) {
                                *engine_state = next_state;
                            }
                        }
                        event_state.current_screen = 1;
                    }
                }
                1 => {
                    // +5 Max HP, then Java DamageInfo(null, hpLoss).
                    let source = DomainEventSource::Event(EventId::ForgottenAltar);
                    let hp_loss_pct = if run_state.ascension_level >= 15 {
                        0.35
                    } else {
                        0.25
                    };
                    let hp_loss = (run_state.max_hp as f32 * hp_loss_pct).round() as i32;
                    run_state.gain_max_hp_with_source(5, 5, source);
                    super::apply_player_default_damage(
                        run_state,
                        hp_loss,
                        super::EventDamageOwner::None,
                        source,
                    );
                    event_state.current_screen = 1;
                }
                _ => {
                    // Desecrate: Decay curse
                    super::obtain_event_card(run_state, EventId::ForgottenAltar, CardId::Decay);
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
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::EngineState;
    use crate::state::events::{EventId, EventState};
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn forgotten_altar_run() -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 50;
        run_state.max_hp = 80;
        run_state.event_state = Some(EventState::new(EventId::ForgottenAltar));
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn disabled_offer_without_golden_idol_does_not_advance_or_grant_relic() {
        let mut run_state = forgotten_altar_run();
        let mut engine_state = EngineState::EventRoom;

        let choices = get_choices(&run_state, run_state.event_state.as_ref().unwrap());
        assert!(choices[0].disabled);

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(!run_state
            .relics
            .iter()
            .any(|relic| matches!(relic.id, RelicId::BloodyIdol | RelicId::Circlet)));
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn offering_golden_idol_replaces_same_relic_slot_with_bloody_idol() {
        let mut run_state = forgotten_altar_run();
        run_state.relics.push(RelicState::new(RelicId::GoldenIdol));
        run_state.relics.push(RelicState::new(RelicId::Anchor));
        let golden_slot = run_state
            .relics
            .iter()
            .position(|relic| relic.id == RelicId::GoldenIdol)
            .unwrap();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.relics[golden_slot].id, RelicId::BloodyIdol);
        assert!(run_state
            .relics
            .iter()
            .all(|relic| relic.id != RelicId::GoldenIdol));
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicLost {
                relic_id: RelicId::GoldenIdol,
                source: DomainEventSource::Event(EventId::ForgottenAltar),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::BloodyIdol,
                source: DomainEventSource::Event(EventId::ForgottenAltar),
            }
        )));
    }

    #[test]
    fn offering_golden_idol_with_existing_bloody_idol_grants_circlet_without_losing_idol() {
        let mut run_state = forgotten_altar_run();
        run_state.relics.push(RelicState::new(RelicId::GoldenIdol));
        run_state.relics.push(RelicState::new(RelicId::BloodyIdol));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::GoldenIdol));
        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::BloodyIdol));
        assert_eq!(run_state.relics.last().unwrap().id, RelicId::Circlet);
        let events = run_state.take_emitted_events();
        assert!(!events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicLost {
                relic_id: RelicId::GoldenIdol,
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::Circlet,
                source: DomainEventSource::Event(EventId::ForgottenAltar),
            }
        )));
    }

    #[test]
    fn shed_blood_increases_max_hp_then_heals_then_takes_java_damage() {
        let mut run_state = forgotten_altar_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.max_hp, 85);
        assert_eq!(run_state.current_hp, 35);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::MaxHpChanged {
                delta: 5,
                current_hp: 55,
                max_hp: 85,
                source: DomainEventSource::Event(EventId::ForgottenAltar),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 5,
                current_hp: 55,
                max_hp: 85,
                source: DomainEventSource::Event(EventId::ForgottenAltar),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -20,
                current_hp: 35,
                max_hp: 85,
                source: DomainEventSource::Event(EventId::ForgottenAltar),
            }
        )));
    }

    #[test]
    fn shed_blood_damage_respects_tungsten_after_max_hp_heal() {
        let mut run_state = forgotten_altar_run();
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.max_hp, 85);
        assert_eq!(run_state.current_hp, 36);
    }

    #[test]
    fn desecrate_decay_uses_event_obtain_pipeline_and_omamori_can_block_it() {
        let mut run_state = forgotten_altar_run();
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Decay));
        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori should remain after blocking Decay");
        assert_eq!(omamori.counter, 1);
    }

    #[test]
    fn desecrate_runs_obtain_hooks_before_decay_obtained_event() {
        let mut run_state = forgotten_altar_run();
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        let events = run_state.take_emitted_events();
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::ForgottenAltar),
                        ..
                    }
                )
            })
            .expect("Ceramic Fish should run when Forgotten Altar's Decay is obtained");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::ForgottenAltar),
                    } if card.id == CardId::Decay
                )
            })
            .expect("Forgotten Altar should obtain Decay through ShowCardAndObtainEffect");

        assert!(
            fish_gold_pos < obtained_pos,
            "Java ForgottenAltar queues ShowCardAndObtainEffect for Decay; that effect runs onObtainCard before Soul.obtain"
        );
    }
}
