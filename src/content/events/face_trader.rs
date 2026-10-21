use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaceTraderResult {
    Touch,
    Trade,
    Leave,
}

impl FaceTraderResult {
    fn state_code(self) -> i32 {
        match self {
            Self::Touch => 1,
            Self::Trade => 2,
            Self::Leave => 3,
        }
    }

    pub fn from_event_state(event: &EventState) -> Option<Self> {
        if event.id != EventId::FaceTrader || event.current_screen != 2 {
            return None;
        }
        match event.internal_state {
            1 => Some(Self::Touch),
            2 => Some(Self::Trade),
            3 => Some(Self::Leave),
            _ => None,
        }
    }
}

const FACE_RELICS: [RelicId; 5] = [
    RelicId::CultistMask,
    RelicId::FaceOfCleric,
    RelicId::GremlinMask,
    RelicId::NlothsMask,
    RelicId::SsserpentHead,
];

fn gold_reward(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        50
    } else {
        75
    }
}

fn touch_damage(run_state: &RunState) -> i32 {
    (run_state.max_hp / 10).max(1)
}

fn owns_relic(run_state: &RunState, relic_id: RelicId) -> bool {
    run_state.relics.iter().any(|owned| owned.id == relic_id)
}

fn face_relic_reward_kind(run_state: &RunState) -> EventRelicKind {
    if FACE_RELICS
        .iter()
        .copied()
        .all(|relic_id| owns_relic(run_state, relic_id))
    {
        EventRelicKind::Specific(RelicId::Circlet)
    } else {
        EventRelicKind::RandomFace
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![EventOption::new(
            EventChoiceMeta::new("[Proceed]"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        )],
        1 => {
            let gold_reward = gold_reward(run_state);
            let damage = touch_damage(run_state);
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Touch] Lose {} HP. Gain {} Gold.",
                        damage, gold_reward
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseHp(damage),
                            EventEffect::GainGold(gold_reward),
                        ],
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Trade] Obtain a face Relic."),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![EventEffect::ObtainRelic {
                            count: 1,
                            kind: face_relic_reward_kind(run_state),
                        }],
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
            event_state.current_screen = 1;
        }
        1 => {
            match choice_idx {
                0 => {
                    // Touch: damage + gold
                    // Java: DamageInfo(null, damage) — DEFAULT damage type, ownerless.
                    let gold_reward = gold_reward(run_state);
                    let damage = touch_damage(run_state);
                    run_state.change_gold_with_source(
                        gold_reward,
                        DomainEventSource::Event(EventId::FaceTrader),
                    );
                    super::apply_player_default_damage(
                        run_state,
                        damage,
                        super::EventDamageOwner::None,
                        DomainEventSource::Event(EventId::FaceTrader),
                    );
                    event_state.internal_state = FaceTraderResult::Touch.state_code();
                    event_state.current_screen = 2;
                }
                1 => {
                    // Trade: get a face relic
                    // Java: Collections.shuffle(ids, new Random(miscRng.randomLong()))
                    let mut available: Vec<RelicId> = FACE_RELICS
                        .iter()
                        .copied()
                        .filter(|r| !run_state.relics.iter().any(|owned| owned.id == *r))
                        .collect();

                    let relic_id = if available.is_empty() {
                        // Consume randomLong for seed parity even with no available relics
                        let _seed = run_state.rng_pool.misc_rng.random_long();
                        RelicId::Circlet
                    } else {
                        crate::runtime::rng::shuffle_with_random_long(
                            &mut available,
                            &mut run_state.rng_pool.misc_rng,
                        );
                        available[0]
                    };
                    if let Some(next_state) = run_state.obtain_relic_with_source(
                        relic_id,
                        EngineState::EventRoom,
                        DomainEventSource::Event(EventId::FaceTrader),
                    ) {
                        *engine_state = next_state;
                    }
                    event_state.internal_state = FaceTraderResult::Trade.state_code();
                    event_state.current_screen = 2;
                }
                _ => {
                    event_state.internal_state = FaceTraderResult::Leave.state_code();
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
    use super::{get_options, handle_choice, FaceTraderResult};
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventRelicKind, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn face_trader_main_run() -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state.gold = 0;
        let mut event_state = EventState::new(EventId::FaceTrader);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn structured_intro_and_main_options_expose_java_semantics() {
        let run_state = face_trader_main_run();

        let intro_options = get_options(&run_state, &EventState::new(EventId::FaceTrader));
        assert_eq!(intro_options.len(), 1);
        assert_eq!(intro_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            intro_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let event_state = run_state.event_state.as_ref().unwrap();
        let main_options = get_options(&run_state, event_state);

        assert_eq!(main_options.len(), 3);
        assert_eq!(main_options[0].semantics.action, EventActionKind::Trade);
        assert!(main_options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(8)));
        assert!(main_options[0]
            .semantics
            .effects
            .contains(&EventEffect::GainGold(75)));
        assert_eq!(
            main_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        assert_eq!(main_options[1].semantics.action, EventActionKind::Trade);
        assert!(main_options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::RandomFace,
            }));

        assert_eq!(main_options[2].semantics.action, EventActionKind::Leave);
        assert_eq!(
            main_options[2].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
    }

    #[test]
    fn structured_trade_option_exposes_circlet_when_all_faces_are_owned() {
        let mut run_state = face_trader_main_run();
        run_state.relics.extend([
            RelicState::new(RelicId::CultistMask),
            RelicState::new(RelicId::FaceOfCleric),
            RelicState::new(RelicId::GremlinMask),
            RelicState::new(RelicId::NlothsMask),
            RelicState::new(RelicId::SsserpentHead),
        ]);

        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::Circlet),
            }));
    }

    #[test]
    fn leave_from_main_screen_goes_to_java_result_screen_before_map() {
        let mut run_state = face_trader_main_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        let event_state = run_state.event_state.as_ref().unwrap();
        assert!(!event_state.completed);
        assert_eq!(event_state.current_screen, 2);
        assert_eq!(
            FaceTraderResult::from_event_state(event_state),
            Some(FaceTraderResult::Leave)
        );

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state.event_state.as_ref().unwrap().completed);
    }

    #[test]
    fn touch_uses_event_hp_and_gold_sources() {
        let mut run_state = face_trader_main_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 12);
        assert_eq!(run_state.gold, 75);
        assert_eq!(
            FaceTraderResult::from_event_state(run_state.event_state.as_ref().unwrap()),
            Some(FaceTraderResult::Touch)
        );
        let events = run_state.take_emitted_events();
        assert!(matches!(
            events.as_slice(),
            [
                DomainEvent::GoldChanged {
                    delta: 75,
                    new_total: 75,
                    source: DomainEventSource::Event(EventId::FaceTrader),
                },
                DomainEvent::HpChanged {
                    delta: -8,
                    current_hp: 12,
                    max_hp: 80,
                    source: DomainEventSource::Event(EventId::FaceTrader),
                },
            ]
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -8,
                current_hp: 12,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::FaceTrader),
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: 75,
                new_total: 75,
                source: DomainEventSource::Event(EventId::FaceTrader),
            }
        )));
    }

    #[test]
    fn touch_damage_respects_tungsten_rod() {
        let mut run_state = face_trader_main_run();
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 13);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -7,
                source: DomainEventSource::Event(EventId::FaceTrader),
                ..
            }
        )));
    }

    #[test]
    fn trade_obtains_face_relic_through_event_source_pipeline() {
        let mut run_state = face_trader_main_run();
        run_state.relics.retain(|relic| {
            !matches!(
                relic.id,
                RelicId::CultistMask
                    | RelicId::FaceOfCleric
                    | RelicId::GremlinMask
                    | RelicId::NlothsMask
                    | RelicId::SsserpentHead
            )
        });
        run_state.rng_pool.misc_rng = crate::runtime::rng::StsRng::new(1);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let obtained = run_state.relics.last().unwrap().id;
        assert_eq!(
            FaceTraderResult::from_event_state(run_state.event_state.as_ref().unwrap()),
            Some(FaceTraderResult::Trade)
        );
        assert!(matches!(
            obtained,
            RelicId::CultistMask
                | RelicId::FaceOfCleric
                | RelicId::GremlinMask
                | RelicId::NlothsMask
                | RelicId::SsserpentHead
        ));
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id,
                source: DomainEventSource::Event(EventId::FaceTrader),
            } if *relic_id == obtained
        )));
    }

    #[test]
    fn trade_grants_circlet_when_all_face_relics_are_owned() {
        let mut run_state = face_trader_main_run();
        run_state.relics.extend([
            RelicState::new(RelicId::CultistMask),
            RelicState::new(RelicId::FaceOfCleric),
            RelicState::new(RelicId::GremlinMask),
            RelicState::new(RelicId::NlothsMask),
            RelicState::new(RelicId::SsserpentHead),
        ]);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.relics.last().unwrap().id, RelicId::Circlet);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::RelicObtained {
                relic_id: RelicId::Circlet,
                source: DomainEventSource::Event(EventId::FaceTrader),
            }
        )));
    }
}
