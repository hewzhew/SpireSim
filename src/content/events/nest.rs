use crate::content::cards::CardId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn nest_gold_gain(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        50
    } else {
        99
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![EventOption::new(
            EventChoiceMeta::new("[Explore]"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        )],
        1 => {
            let gold_gain = nest_gold_gain(run_state);
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!("[Steal] Gain {} Gold.", gold_gain)),
                    EventOptionSemantics {
                        action: EventActionKind::Gain,
                        effects: vec![EventEffect::GainGold(gold_gain)],
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Join] Take 6 damage. Obtain Ritual Dagger."),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseHp(6),
                            EventEffect::ObtainColorlessCard {
                                count: 1,
                                kind: EventCardKind::Specific(CardId::RitualDagger),
                            },
                        ],
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

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            event_state.current_screen = 1;
        }
        1 => {
            match choice_idx {
                0 => {
                    // Steal gold
                    let gold_gain = nest_gold_gain(run_state);
                    run_state.change_gold_with_source(
                        gold_gain,
                        DomainEventSource::Event(EventId::Nest),
                    );
                    event_state.current_screen = 2;
                }
                _ => {
                    // Join cult: Java DamageInfo(null, 6), then Ritual Dagger.
                    super::apply_player_default_damage(
                        run_state,
                        6,
                        super::EventDamageOwner::None,
                        DomainEventSource::Event(EventId::Nest),
                    );
                    super::obtain_event_card(run_state, EventId::Nest, CardId::RitualDagger);
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
    use super::{get_options, handle_choice};
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventId, EventOptionTransition, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn nest_run(current_hp: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = current_hp;
        let mut event_state = EventState::new(EventId::Nest);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn structured_options_expose_reveal_steal_join_and_result_boundaries() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.event_state = Some(EventState::new(EventId::Nest));
        let intro_options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert_eq!(intro_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            intro_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let run_state = nest_run(50);
        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Gain);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::GainGold(99)));

        assert_eq!(options[1].semantics.action, EventActionKind::Trade);
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(6)));
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainColorlessCard {
                count: 1,
                kind: EventCardKind::Specific(CardId::RitualDagger),
            }));

        let mut a15_run = nest_run(50);
        a15_run.ascension_level = 15;
        let a15_options = get_options(&a15_run, a15_run.event_state.as_ref().unwrap());
        assert!(a15_options[0]
            .semantics
            .effects
            .contains(&EventEffect::GainGold(50)));

        let mut result = EventState::new(EventId::Nest);
        result.current_screen = 2;
        let result_options = get_options(&run_state, &result);
        assert_eq!(result_options[0].semantics.action, EventActionKind::Leave);
        assert!(result_options[0].semantics.terminal);
    }

    #[test]
    fn steal_gold_uses_event_source() {
        let mut run_state = nest_run(50);
        run_state.gold = 0;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, 99);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: 99,
                new_total: 99,
                source: DomainEventSource::Event(EventId::Nest),
            }
        )));
    }

    #[test]
    fn join_cult_damage_and_ritual_dagger_use_event_source() {
        let mut run_state = nest_run(20);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 14);
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::RitualDagger));
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -6,
                current_hp: 14,
                source: DomainEventSource::Event(EventId::Nest),
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::Nest),
            } if card.id == CardId::RitualDagger
        )));
    }

    #[test]
    fn join_cult_damage_applies_tungsten_rod() {
        let mut run_state = nest_run(20);
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 15);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -5,
                current_hp: 15,
                source: DomainEventSource::Event(EventId::Nest),
                ..
            }
        )));
    }

    #[test]
    fn join_cult_damage_resolves_before_delayed_ritual_dagger_obtain() {
        let mut run_state = nest_run(20);
        run_state.gold = 0;
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 14);
        assert_eq!(run_state.gold, 9);
        let labels = run_state
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::HpChanged {
                    delta: -6,
                    source: DomainEventSource::Event(EventId::Nest),
                    ..
                } => Some("hp_loss"),
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::Nest),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::Nest),
                } if card.id == CardId::RitualDagger => Some("ritual_dagger_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec!["hp_loss", "ceramic_fish_gold", "ritual_dagger_obtained"],
            "Java Nest applies DamageInfo(null, 6) before the delayed Ritual Dagger ShowCardAndObtainEffect resolves"
        );
    }
}
