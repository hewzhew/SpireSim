use crate::content::cards::CardId;
use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

const MAX_HP_AMT: i32 = 5;

pub fn init_big_fish_state(run_state: &RunState) -> i32 {
    run_state.max_hp / 3
}

fn frozen_heal_amount(event_state: &EventState) -> i32 {
    event_state.internal_state.max(0)
}

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

    let heal_amt = frozen_heal_amount(event_state);
    vec![
        EventOption::new(
            EventChoiceMeta::new(format!("[Banana] Heal {} HP.", heal_amt)),
            EventOptionSemantics {
                action: EventActionKind::Gain,
                effects: vec![EventEffect::Heal(heal_amt)],
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        ),
        EventOption::new(
            EventChoiceMeta::new(format!("[Donut] Gain {} Max HP.", MAX_HP_AMT)),
            EventOptionSemantics {
                action: EventActionKind::Gain,
                effects: vec![EventEffect::GainMaxHp(MAX_HP_AMT)],
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        ),
        EventOption::new(
            EventChoiceMeta::new("[Box] Obtain a random Relic. Become Cursed - Regret."),
            EventOptionSemantics {
                action: EventActionKind::Trade,
                effects: vec![
                    EventEffect::ObtainRelic {
                        count: 1,
                        kind: EventRelicKind::RandomRelic,
                    },
                    EventEffect::ObtainCurse {
                        count: 1,
                        kind: EventCardKind::Specific(CardId::Regret),
                    },
                ],
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
                    let heal_amt = frozen_heal_amount(&event_state);
                    run_state
                        .heal_with_source(heal_amt, DomainEventSource::Event(EventId::BigFish));
                }
                1 => {
                    run_state.gain_max_hp_with_source(
                        MAX_HP_AMT,
                        MAX_HP_AMT,
                        DomainEventSource::Event(EventId::BigFish),
                    );
                }
                _ => {
                    // Box: Random relic + Regret curse
                    // Java constructs ShowCardAndObtainEffect(Regret) before
                    // spawnRelicAndObtain(relic), so Omamori interception uses
                    // the pre-relic state while later obtain-card hooks see the
                    // newly obtained relic.
                    let omamori_snapshot = run_state
                        .relics
                        .iter()
                        .find(|relic| relic.id == RelicId::Omamori)
                        .map(|relic| relic.counter);
                    let relic_id = run_state.random_screenless_relic_reward();
                    if let Some(next_state) = run_state.obtain_relic_with_source(
                        relic_id,
                        EngineState::EventRoom,
                        DomainEventSource::Event(EventId::BigFish),
                    ) {
                        *engine_state = next_state;
                    }
                    let source = DomainEventSource::Event(EventId::BigFish);
                    run_state.add_card_to_deck_with_omamori_snapshot_from(
                        CardId::Regret,
                        0,
                        source,
                        omamori_snapshot.is_some(),
                        omamori_snapshot.unwrap_or(0),
                    );
                }
            }
            event_state.current_screen = 1;
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
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventId, EventOptionTransition,
        EventRelicKind, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};
    use crate::test_support::natural_test_run;

    fn big_fish_run(current_hp: i32, max_hp: i32) -> RunState {
        let mut run_state = natural_test_run(1, 0, false, "Ironclad");
        run_state.current_hp = current_hp;
        run_state.max_hp = max_hp;
        let mut event_state = EventState::new(EventId::BigFish);
        event_state.internal_state = super::init_big_fish_state(&run_state);
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn banana_uses_java_player_heal_semantics_and_event_source() {
        let mut run_state = big_fish_run(20, 81);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 47);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 27,
                current_hp: 47,
                max_hp: 81,
                source: DomainEventSource::Event(EventId::BigFish),
            }
        )));
    }

    #[test]
    fn banana_heal_is_frozen_when_the_event_opens() {
        let mut run_state = big_fish_run(20, 81);
        run_state.gain_max_hp_with_source(
            5,
            5,
            DomainEventSource::Potion(crate::content::potions::PotionId::FruitJuice),
        );
        let options = super::get_options(
            &run_state,
            run_state.event_state.as_ref().expect("Big Fish state"),
        );
        let mut engine_state = EngineState::EventRoom;

        assert_eq!(options[0].semantics.effects, vec![EventEffect::Heal(27)]);
        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.max_hp, 86);
        assert_eq!(run_state.current_hp, 52);
    }

    #[test]
    fn banana_heal_is_blocked_by_mark_of_the_bloom() {
        let mut run_state = big_fish_run(20, 81);
        run_state
            .relics
            .push(RelicState::new(RelicId::MarkOfTheBloom));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 20);
        assert!(!run_state
            .take_emitted_events()
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
    }

    #[test]
    fn donut_increase_max_hp_uses_java_increase_then_heal_semantics() {
        let mut run_state = big_fish_run(20, 80);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.max_hp, 85);
        assert_eq!(run_state.current_hp, 25);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 5,
                current_hp: 25,
                max_hp: 85,
                source: DomainEventSource::Event(EventId::BigFish),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::MaxHpChanged {
                delta: 5,
                current_hp: 25,
                max_hp: 85,
                source: DomainEventSource::Event(EventId::BigFish),
            }
        )));
    }

    #[test]
    fn donut_max_hp_gain_survives_mark_but_attached_heal_is_blocked() {
        let mut run_state = big_fish_run(20, 80);
        run_state
            .relics
            .push(RelicState::new(RelicId::MarkOfTheBloom));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.max_hp, 85);
        assert_eq!(run_state.current_hp, 20);
        let events = run_state.take_emitted_events();
        assert!(!events
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::MaxHpChanged {
                delta: 5,
                current_hp: 20,
                max_hp: 85,
                source: DomainEventSource::Event(EventId::BigFish),
            }
        )));
    }

    fn force_box_relic(run_state: &mut RunState, relic_id: RelicId) {
        run_state.common_relic_pool = vec![relic_id];
        run_state.uncommon_relic_pool = vec![relic_id];
        run_state.rare_relic_pool = vec![relic_id];
    }

    #[test]
    fn non_box_choices_do_not_advance_the_relic_rng_or_relic_pool_cursors() {
        for choice_idx in [0, 1] {
            let mut run_state = big_fish_run(80, 80);
            let relic_rng_before = run_state.rng_pool.relic_rng.clone();
            let pools_before = (
                run_state.common_relic_pool.clone(),
                run_state.uncommon_relic_pool.clone(),
                run_state.rare_relic_pool.clone(),
            );
            let mut engine_state = EngineState::EventRoom;

            handle_choice(&mut engine_state, &mut run_state, choice_idx);

            assert_eq!(run_state.rng_pool.relic_rng, relic_rng_before);
            assert_eq!(
                (
                    &run_state.common_relic_pool,
                    &run_state.uncommon_relic_pool,
                    &run_state.rare_relic_pool,
                ),
                (&pools_before.0, &pools_before.1, &pools_before.2),
                "Banana and Donut must leave every tier-specific relic cursor untouched"
            );
        }
    }

    #[test]
    fn blue_candle_box_consumes_one_tier_cursor_and_still_adds_regret() {
        let mut run_state = big_fish_run(80, 80);
        force_box_relic(&mut run_state, RelicId::BlueCandle);
        let relic_rng_counter_before = run_state.rng_pool.relic_rng.counter;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert_eq!(
            run_state.rng_pool.relic_rng.counter,
            relic_rng_counter_before + 1,
            "Box alone rolls a random relic tier"
        );
        assert_eq!(
            run_state.common_relic_pool.len()
                + run_state.uncommon_relic_pool.len()
                + run_state.rare_relic_pool.len(),
            2,
            "the selected tier consumes its front relic while the other tier cursors remain"
        );
        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::BlueCandle));
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Regret));
    }

    #[test]
    fn options_expose_structured_banana_donut_box_and_leave_semantics() {
        let run_state = big_fish_run(20, 81);
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Big Fish should expose structured event semantics");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].semantics.action, EventActionKind::Gain);
        assert_eq!(options[0].semantics.effects, vec![EventEffect::Heal(27)]);
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Gain);
        assert_eq!(
            options[1].semantics.effects,
            vec![EventEffect::GainMaxHp(5)]
        );
        assert_eq!(
            options[2].semantics.effects,
            vec![
                EventEffect::ObtainRelic {
                    count: 1,
                    kind: EventRelicKind::RandomRelic,
                },
                EventEffect::ObtainCurse {
                    count: 1,
                    kind: EventCardKind::Specific(CardId::Regret),
                },
            ]
        );

        let mut result_screen = EventState::new(EventId::BigFish);
        result_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &result_screen,
            )
            .expect("Big Fish result screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn box_new_omamori_does_not_block_regret_from_same_choice() {
        let mut run_state = big_fish_run(80, 80);
        force_box_relic(&mut run_state, RelicId::Omamori);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("box should obtain Omamori from the forced relic pool");
        assert_eq!(omamori.counter, 2);
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Regret));
    }

    #[test]
    fn box_new_darkstone_still_triggers_on_regret_after_relic_obtain() {
        let mut run_state = big_fish_run(80, 80);
        force_box_relic(&mut run_state, RelicId::DarkstonePeriapt);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Regret));
        assert_eq!(run_state.max_hp, 86);
    }

    #[test]
    fn box_new_ceramic_fish_triggers_before_regret_obtained_event() {
        let mut run_state = big_fish_run(80, 80);
        force_box_relic(&mut run_state, RelicId::CeramicFish);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        let events = run_state.take_emitted_events();
        let relic_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::RelicObtained {
                        relic_id: RelicId::CeramicFish,
                        source: DomainEventSource::Event(EventId::BigFish),
                    }
                )
            })
            .expect("Box should obtain the forced relic before the delayed curse resolves");
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::BigFish),
                        ..
                    }
                )
            })
            .expect("New Ceramic Fish should see the delayed Regret obtain hook");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::BigFish),
                    } if card.id == CardId::Regret
                )
            })
            .expect("Box should obtain Regret through the delayed ShowCardAndObtainEffect");

        assert!(
            relic_pos < fish_gold_pos && fish_gold_pos < obtained_pos,
            "Java BigFish constructs the curse effect before spawnRelicAndObtain, but the effect resolves after the new relic is owned and runs onObtainCard before Soul.obtain"
        );
    }
}
