use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRandomOutcomeKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

// internal_state encodes: lower 16 bits = relic_chance, upper 16 bits = current_dmg

fn decode_state(state: i32) -> (i32, i32) {
    let chance = state & 0xFFFF;
    let dmg = (state >> 16) & 0xFFFF;
    (chance, dmg)
}

fn encode_state(chance: i32, dmg: i32) -> i32 {
    (dmg << 16) | (chance & 0xFFFF)
}

fn current_relic_chance_and_damage(run_state: &RunState, event_state: &EventState) -> (i32, i32) {
    if event_state.internal_state == 0 {
        let base_dmg = if run_state.ascension_level >= 15 {
            5
        } else {
            3
        };
        (25, base_dmg)
    } else {
        decode_state(event_state.internal_state)
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
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

    let (chance, dmg) = current_relic_chance_and_damage(run_state, event_state);

    vec![
        EventOption::new(
            EventChoiceMeta::new(format!(
                "[Reach In] Take {} damage. {}% chance to obtain a Relic.",
                dmg, chance
            )),
            EventOptionSemantics {
                action: EventActionKind::Special,
                effects: vec![
                    EventEffect::LoseHp(dmg),
                    EventEffect::RandomOutcome {
                        kind: EventRandomOutcomeKind::ScrapOozeRelicRoll,
                        chance_percent: Some(chance.clamp(0, 100) as u8),
                    },
                ],
                transition: EventOptionTransition::None,
                repeatable: true,
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
                    // Reach In
                    let (mut chance, mut dmg) =
                        current_relic_chance_and_damage(run_state, &event_state);

                    // Java uses DamageInfo(null, dmg): no block, no Torii owner hook,
                    // but Tungsten Rod still applies on HP loss.
                    super::apply_player_default_damage(
                        run_state,
                        dmg,
                        super::EventDamageOwner::None,
                        DomainEventSource::Event(EventId::ScrapOoze),
                    );

                    // Roll for relic
                    let roll = run_state.rng_pool.misc_rng.random_range(0, 99);
                    if roll >= 99 - chance {
                        // Success! Get a relic
                        let relic_id = run_state.random_screenless_relic_reward();
                        if let Some(next_state) = run_state.obtain_relic_with_source(
                            relic_id,
                            EngineState::EventRoom,
                            DomainEventSource::Event(EventId::ScrapOoze),
                        ) {
                            *_engine_state = next_state;
                        }
                        event_state.current_screen = 1;
                    } else {
                        // Fail: escalate
                        chance += 10;
                        dmg += 1;
                        event_state.internal_state = encode_state(chance, dmg);
                        // Stay on screen 0
                    }
                }
                _ => {
                    // Flee
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
    use super::{get_options, handle_choice};
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventRandomOutcomeKind,
        EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    #[test]
    fn structured_options_expose_current_damage_and_relic_chance_boundary() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        let mut event_state = EventState::new(EventId::ScrapOoze);
        event_state.internal_state = super::encode_state(35, 4);
        run_state.event_state = Some(event_state);

        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Special);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(4)));
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::RandomOutcome {
                kind: EventRandomOutcomeKind::ScrapOozeRelicRoll,
                chance_percent: Some(35),
            }));
        assert!(options[0].semantics.repeatable);
        assert_eq!(options[0].semantics.transition, EventOptionTransition::None);
        assert_eq!(options[1].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
    }

    #[test]
    fn structured_options_use_a15_starting_damage() {
        let mut run_state = RunState::new(1, 15, true, "Ironclad");
        run_state.event_state = Some(EventState::new(EventId::ScrapOoze));

        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(5)));
    }

    #[test]
    fn structured_complete_screen_is_terminal_leave() {
        let run_state = RunState::new(1, 0, true, "Ironclad");
        let mut event_state = EventState::new(EventId::ScrapOoze);
        event_state.current_screen = 1;

        let options = get_options(&run_state, &event_state);

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        assert!(options[0].semantics.terminal);
    }

    #[test]
    fn reach_in_default_null_damage_ignores_torii_but_applies_tungsten() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.current_hp = 30;
        run_state.max_hp = 80;
        run_state.relics.push(RelicState::new(RelicId::Torii));
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        run_state.event_state = Some(EventState::new(EventId::ScrapOoze));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(
            run_state.current_hp, 28,
            "Java DamageInfo(null, 3) skips Torii but Tungsten reduces HP loss by 1"
        );
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -2,
                current_hp: 28,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::ScrapOoze),
            }
        )));
    }
}
