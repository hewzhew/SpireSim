use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn offered_relic(
    run_state: &RunState,
    event_state: &EventState,
    choice_idx: usize,
) -> Option<RelicId> {
    let relic_idx = if choice_idx == 0 {
        (event_state.internal_state & 0xFF) as usize
    } else {
        ((event_state.internal_state >> 8) & 0xFF) as usize
    };
    run_state.relics.get(relic_idx).map(|relic| relic.id)
}

fn nloth_reward_relic(run_state: &RunState) -> RelicId {
    if run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::NlothsGift)
    {
        RelicId::Circlet
    } else {
        RelicId::NlothsGift
    }
}

fn trade_effects(
    run_state: &RunState,
    event_state: &EventState,
    choice_idx: usize,
) -> Vec<EventEffect> {
    let mut effects = Vec::new();
    let reward_relic = nloth_reward_relic(run_state);
    if reward_relic == RelicId::NlothsGift {
        if let Some(relic) = offered_relic(run_state, event_state, choice_idx) {
            effects.push(EventEffect::LoseRelic {
                specific: Some(relic),
                starter_only: false,
            });
        }
    }
    effects.push(EventEffect::ObtainRelic {
        count: 1,
        kind: EventRelicKind::Specific(reward_relic),
    });
    effects
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            // N'loth offers to trade 2 random relics for N'loth's Gift
            // internal_state encodes: bits[0..7]=choice1_idx, bits[8..15]=choice2_idx
            let c1 = (event_state.internal_state & 0xFF) as usize;
            let c2 = ((event_state.internal_state >> 8) & 0xFF) as usize;
            let r1_name = run_state
                .relics
                .get(c1)
                .map(|r| r.id.stable_id().to_string())
                .unwrap_or("???".into());
            let r2_name = run_state
                .relics
                .get(c2)
                .map(|r| r.id.stable_id().to_string())
                .unwrap_or("???".into());
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!("[Trade {}] Obtain N'loth's Gift.", r1_name)),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: trade_effects(run_state, event_state, 0),
                        transition: EventOptionTransition::AdvanceScreen,
                        ..EventOptionSemantics::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!("[Trade {}] Obtain N'loth's Gift.", r2_name)),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: trade_effects(run_state, event_state, 1),
                        transition: EventOptionTransition::AdvanceScreen,
                        ..EventOptionSemantics::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Leave]"),
                    EventOptionSemantics {
                        action: EventActionKind::Leave,
                        transition: EventOptionTransition::AdvanceScreen,
                        ..EventOptionSemantics::default()
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
                ..EventOptionSemantics::default()
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
                0 | 1 => {
                    let relic_idx = if choice_idx == 0 {
                        (event_state.internal_state & 0xFF) as usize
                    } else {
                        ((event_state.internal_state >> 8) & 0xFF) as usize
                    };

                    let source = DomainEventSource::Event(EventId::Nloth);
                    let already_has_gift =
                        run_state.relics.iter().any(|r| r.id == RelicId::NlothsGift);

                    // Java only removes the offered relic when the reward is N'loth's Gift.
                    // If the player already has N'loth's Gift, it grants Circlet without
                    // calling loseRelic on the offered relic.
                    if !already_has_gift && relic_idx < run_state.relics.len() {
                        run_state.remove_relic_at_with_source(relic_idx, source);
                    }

                    let gift_id = if already_has_gift {
                        RelicId::Circlet
                    } else {
                        RelicId::NlothsGift
                    };
                    let _ =
                        run_state.obtain_relic_with_source(gift_id, EngineState::EventRoom, source);
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

/// Initialize N'loth event state: pick 2 random relics to offer.
/// Java: Collections.shuffle(relics, new Random(miscRng.randomLong()))
/// then choice1 = relics[0], choice2 = relics[1]
pub fn init_nloth_state(run_state: &mut RunState) -> i32 {
    if run_state.relics.len() < 2 {
        return 0;
    }
    // Build index list and shuffle with randomLong seed (matching Java exactly)
    let mut indices: Vec<usize> = (0..run_state.relics.len()).collect();
    crate::runtime::rng::shuffle_with_random_long(&mut indices, &mut run_state.rng_pool.misc_rng);
    let idx1 = indices[0];
    let idx2 = indices[1];
    (idx1 as i32) | ((idx2 as i32) << 8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;
    use crate::state::events::{EventEffect, EventRelicKind};
    use crate::state::selection::DomainEvent;

    #[test]
    fn trade_option_exposes_specific_relic_trade_effects() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.relics = vec![
            RelicState::new(RelicId::BurningBlood),
            RelicState::new(RelicId::Anchor),
        ];
        let event_state = EventState {
            id: EventId::Nloth,
            current_screen: 0,
            internal_state: 1,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let options = get_options(&rs, &event_state);

        assert_eq!(options[0].ui.text, "[Trade Anchor] Obtain N'loth's Gift.");
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseRelic {
                specific: Some(RelicId::Anchor),
                starter_only: false,
            }));
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::NlothsGift),
            }));
    }

    #[test]
    fn event_handler_uses_structured_nloth_options() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.relics = vec![
            RelicState::new(RelicId::BurningBlood),
            RelicState::new(RelicId::Anchor),
        ];
        rs.event_state = Some(EventState {
            id: EventId::Nloth,
            current_screen: 0,
            internal_state: 1,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        let options = crate::engine::event_handler::get_event_options(&rs);

        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseRelic {
                specific: Some(RelicId::Anchor),
                starter_only: false,
            }));
    }

    #[test]
    fn trade_removes_offered_relic_and_obtains_gift_with_event_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.relics = vec![
            RelicState::new(RelicId::BurningBlood),
            RelicState::new(RelicId::Anchor),
        ];
        rs.event_state = Some(EventState {
            id: EventId::Nloth,
            current_screen: 0,
            internal_state: 1,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        handle_choice(&mut EngineState::EventRoom, &mut rs, 0);

        assert!(!rs.relics.iter().any(|r| r.id == RelicId::Anchor));
        assert!(rs.relics.iter().any(|r| r.id == RelicId::NlothsGift));
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicLost {
                relic_id: RelicId::Anchor,
                source: DomainEventSource::Event(EventId::Nloth),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::NlothsGift,
                source: DomainEventSource::Event(EventId::Nloth),
            }
        )));
    }

    #[test]
    fn trade_with_existing_gift_grants_circlet_without_losing_offered_relic() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.relics = vec![
            RelicState::new(RelicId::BurningBlood),
            RelicState::new(RelicId::Anchor),
            RelicState::new(RelicId::NlothsGift),
        ];
        rs.event_state = Some(EventState {
            id: EventId::Nloth,
            current_screen: 0,
            internal_state: 1,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        handle_choice(&mut EngineState::EventRoom, &mut rs, 0);

        assert!(rs.relics.iter().any(|r| r.id == RelicId::Anchor));
        assert!(rs.relics.iter().any(|r| r.id == RelicId::Circlet));
        let events = rs.take_emitted_events();
        assert!(!events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicLost {
                relic_id: RelicId::Anchor,
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::Circlet,
                source: DomainEventSource::Event(EventId::Nloth),
            }
        )));
    }
}
