use crate::content::cards::CardId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let curse_chance = if run_state.ascension_level >= 15 {
                100
            } else {
                50
            };
            let mut open_effects = vec![EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::RandomRelic,
            }];
            if run_state.ascension_level >= 15 {
                open_effects.push(EventEffect::ObtainCurse {
                    count: 1,
                    kind: EventCardKind::Specific(CardId::Writhe),
                });
            }
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Open] {}% chance of Writhe. Obtain a random Relic.",
                        curse_chance
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: open_effects,
                        transition: EventOptionTransition::AdvanceScreen,
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

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            match choice_idx {
                0 => {
                    // Open: relic + possible Writhe curse
                    // Java: always calls miscRng.randomBoolean(), then overrides at A15
                    let mut gets_curse = run_state.rng_pool.misc_rng.random_boolean();
                    if run_state.ascension_level >= 15 {
                        gets_curse = true;
                    }
                    let omamori_snapshot = run_state
                        .relics
                        .iter()
                        .find(|relic| relic.id == crate::content::relics::RelicId::Omamori)
                        .map(|relic| relic.counter);
                    let relic_id = run_state.random_screenless_relic_reward();
                    if let Some(next_state) = run_state.obtain_relic_with_source(
                        relic_id,
                        EngineState::EventRoom,
                        DomainEventSource::Event(EventId::Mausoleum),
                    ) {
                        *engine_state = next_state;
                    }
                    if gets_curse {
                        let source = DomainEventSource::Event(EventId::Mausoleum);
                        run_state.add_card_to_deck_with_omamori_snapshot_from(
                            CardId::Writhe,
                            0,
                            source,
                            omamori_snapshot.is_some(),
                            omamori_snapshot.unwrap_or(0),
                        );
                    }
                    event_state.current_screen = 1;
                }
                _ => {
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
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventOptionTransition, EventRelicKind,
    };
    use crate::state::selection::DomainEvent;

    fn mausoleum_run() -> RunState {
        let mut run_state = RunState::new(1, 15, true, "Ironclad");
        run_state.current_hp = 50;
        run_state.max_hp = 80;
        run_state.common_relic_pool = vec![RelicId::DarkstonePeriapt];
        run_state.uncommon_relic_pool = vec![RelicId::DarkstonePeriapt];
        run_state.rare_relic_pool = vec![RelicId::DarkstonePeriapt];
        run_state.event_state = Some(EventState {
            id: EventId::Mausoleum,
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

    #[test]
    fn structured_options_expose_random_relic_and_only_certain_writhe_at_a15() {
        let run_state = mausoleum_run();
        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Trade);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::RandomRelic,
            }));
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainCurse {
                count: 1,
                kind: EventCardKind::Specific(CardId::Writhe),
            }));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut a0_run = mausoleum_run();
        a0_run.ascension_level = 0;
        let a0_options = get_options(&a0_run, a0_run.event_state.as_ref().unwrap());
        assert!(!a0_options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainCurse {
                count: 1,
                kind: EventCardKind::Specific(CardId::Writhe),
            }));

        let mut result = EventState::new(EventId::Mausoleum);
        result.current_screen = 1;
        let result_options = get_options(&run_state, &result);
        assert_eq!(result_options[0].semantics.action, EventActionKind::Leave);
        assert!(result_options[0].semantics.terminal);
    }

    #[test]
    fn cursed_open_obtains_relic_before_writhe_effect_resolves_like_java() {
        let mut run_state = mausoleum_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::DarkstonePeriapt));
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Writhe));
        assert_eq!(run_state.max_hp, 86);
        assert_eq!(run_state.current_hp, 56);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::MaxHpChanged {
                delta: 6,
                source: DomainEventSource::Event(EventId::Mausoleum),
                ..
            }
        )));
    }

    #[test]
    fn cursed_open_still_rolls_misc_rng_before_a15_forces_curse() {
        let mut run_state = mausoleum_run();
        let before_counter = run_state.rng_pool.misc_rng.counter;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.rng_pool.misc_rng.counter, before_counter + 1);
    }

    #[test]
    fn omamori_blocks_writhe_after_relic_obtain_so_darkstone_does_not_trigger() {
        let mut run_state = mausoleum_run();
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::DarkstonePeriapt));
        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Writhe));
        assert_eq!(run_state.max_hp, 80);
        assert_eq!(run_state.current_hp, 50);
        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori should remain after blocking Writhe");
        assert_eq!(omamori.counter, 1);
    }

    #[test]
    fn newly_obtained_omamori_does_not_block_writhe_from_same_open() {
        let mut run_state = mausoleum_run();
        run_state.common_relic_pool = vec![RelicId::Omamori];
        run_state.uncommon_relic_pool = vec![RelicId::Omamori];
        run_state.rare_relic_pool = vec![RelicId::Omamori];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Mausoleum should obtain Omamori from the forced relic pool");
        assert_eq!(
            omamori.counter, 2,
            "Java checks Omamori when Writhe's ShowCardAndObtainEffect is constructed"
        );
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Writhe));
    }

    #[test]
    fn newly_obtained_ceramic_fish_triggers_before_writhe_obtained_event() {
        let mut run_state = mausoleum_run();
        run_state.common_relic_pool = vec![RelicId::CeramicFish];
        run_state.uncommon_relic_pool = vec![RelicId::CeramicFish];
        run_state.rare_relic_pool = vec![RelicId::CeramicFish];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let events = run_state.take_emitted_events();
        let relic_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::RelicObtained {
                        relic_id: RelicId::CeramicFish,
                        source: DomainEventSource::Event(EventId::Mausoleum),
                    }
                )
            })
            .expect("Mausoleum should obtain the forced relic before the delayed curse resolves");
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::Mausoleum),
                        ..
                    }
                )
            })
            .expect("New Ceramic Fish should see the delayed Writhe obtain hook");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::Mausoleum),
                    } if card.id == CardId::Writhe
                )
            })
            .expect("Mausoleum should obtain Writhe through the delayed ShowCardAndObtainEffect");

        assert!(
            relic_pos < fish_gold_pos && fish_gold_pos < obtained_pos,
            "Java Mausoleum constructs the Writhe effect before spawnRelicAndObtain, but the effect resolves after the new relic is owned and runs onObtainCard before Soul.obtain"
        );
    }

    #[test]
    fn leave_from_intro_goes_to_java_result_screen_before_map() {
        let mut run_state = mausoleum_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let event_state = run_state.event_state.as_ref().unwrap();
        assert!(!event_state.completed);
        assert_eq!(event_state.current_screen, 1);

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state.event_state.as_ref().unwrap().completed);
    }
}
