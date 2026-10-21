use crate::content::cards::CardId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    if event_state.current_screen == 1 {
        return vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..EventOptionSemantics::default()
            },
        )];
    }

    let gold_amt = if run_state.ascension_level >= 15 {
        50
    } else {
        100
    };
    vec![
        EventOption::new(
            EventChoiceMeta::new(format!("[Pray] Gain {} Gold.", gold_amt)),
            EventOptionSemantics {
                action: EventActionKind::Gain,
                effects: vec![EventEffect::GainGold(gold_amt)],
                transition: EventOptionTransition::AdvanceScreen,
                ..EventOptionSemantics::default()
            },
        ),
        EventOption::new(
            EventChoiceMeta::new("[Desecrate] Gain 275 Gold. Become Cursed - Regret."),
            EventOptionSemantics {
                action: EventActionKind::Gain,
                effects: vec![
                    EventEffect::GainGold(275),
                    EventEffect::ObtainCurse {
                        count: 1,
                        kind: EventCardKind::Specific(CardId::Regret),
                    },
                ],
                transition: EventOptionTransition::AdvanceScreen,
                ..EventOptionSemantics::default()
            },
        ),
        EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..EventOptionSemantics::default()
            },
        ),
    ]
}

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    if let EngineState::EventRoom = engine_state {
        let (completed, current_screen) = if let Some(es) = &run_state.event_state {
            (es.completed, es.current_screen)
        } else {
            return;
        };

        if completed {
            return;
        }

        if current_screen == 0 {
            match choice_idx {
                0 => {
                    // Pray: +gold (100 or 50 at A15)
                    let gold_amt = if run_state.ascension_level >= 15 {
                        50
                    } else {
                        100
                    };
                    run_state.change_gold_with_source(
                        gold_amt,
                        DomainEventSource::Event(EventId::GoldenShrine),
                    );
                    if let Some(es) = &mut run_state.event_state {
                        es.current_screen = 1; // Transition to leave screen
                    }
                }
                1 => {
                    // Desecrate: +275 Gold, +Regret (via add_card_to_deck for Omamori check)
                    run_state.change_gold_with_source(
                        275,
                        DomainEventSource::Event(EventId::GoldenShrine),
                    );
                    super::obtain_event_card(run_state, EventId::GoldenShrine, CardId::Regret);

                    if let Some(es) = &mut run_state.event_state {
                        es.current_screen = 1;
                    }
                }
                _ => {
                    // Leave
                    if let Some(es) = &mut run_state.event_state {
                        es.completed = true;
                    }
                }
            }
        } else if let Some(es) = &mut run_state.event_state {
            es.completed = true;
        }
    }
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

    fn shrine_run(ascension: u8) -> RunState {
        let mut run_state = RunState::new(1, ascension, false, "Ironclad");
        run_state.gold = 0;
        run_state.event_state = Some(EventState::new(EventId::GoldenShrine));
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn pray_gain_uses_java_ascension_gold_amount() {
        let mut normal = shrine_run(0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut normal, 0);

        assert_eq!(normal.gold, 100);

        let mut asc15 = shrine_run(15);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut asc15, 0);

        assert_eq!(asc15.gold, 50);
    }

    #[test]
    fn desecrate_gold_resolves_before_delayed_regret_obtain_like_java_effect_list() {
        let mut run_state = shrine_run(0);
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.gold, 284);
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Regret));

        let labels = run_state
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::GoldChanged {
                    delta: 275,
                    source: DomainEventSource::Event(EventId::GoldenShrine),
                    ..
                } => Some("event_gold"),
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::GoldenShrine),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::GoldenShrine),
                } if card.id == CardId::Regret => Some("regret_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec!["event_gold", "ceramic_fish_gold", "regret_obtained"],
            "Java gains 275 gold immediately, then ShowCardAndObtainEffect later runs onObtainCard before Soul.obtain"
        );
    }

    #[test]
    fn desecrate_omamori_blocks_regret_but_not_immediate_gold() {
        let mut run_state = shrine_run(0);
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.gold, 275);
        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Regret));
        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori should remain after blocking Regret");
        assert_eq!(omamori.counter, 1);
    }
}
