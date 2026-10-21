use crate::rewards::state::{RewardItem, RewardState};
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

const COST_1: i32 = 20;
const COST_2: i32 = 30;
const COST_3: i32 = 40;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let mut choices = vec![
                EventOption::new(
                    EventChoiceMeta::new(format!("[1 Potion] Lose {} Gold.", COST_1)),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseGold(COST_1),
                            EventEffect::ObtainPotion { count: 1 },
                        ],
                        constraints: vec![],
                        transition: EventOptionTransition::OpenReward,
                        repeatable: false,
                        terminal: false,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!("[2 Potions] Lose {} Gold.", COST_2)),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseGold(COST_2),
                            EventEffect::ObtainPotion { count: 2 },
                        ],
                        constraints: vec![],
                        transition: EventOptionTransition::OpenReward,
                        repeatable: false,
                        terminal: false,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!("[3 Potions] Lose {} Gold.", COST_3)),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseGold(COST_3),
                            EventEffect::ObtainPotion { count: 3 },
                        ],
                        constraints: vec![],
                        transition: EventOptionTransition::OpenReward,
                        repeatable: false,
                        terminal: false,
                    },
                ),
            ];
            if run_state.ascension_level >= 15 {
                let dmg = ((run_state.max_hp as f32 * 0.05).ceil()) as i32;
                choices.push(EventOption::new(
                    EventChoiceMeta::new(format!("[Leave] Lose {} HP.", dmg)),
                    EventOptionSemantics {
                        action: EventActionKind::Leave,
                        effects: vec![EventEffect::LoseHp(dmg)],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            } else {
                choices.push(EventOption::new(
                    EventChoiceMeta::new("[Leave]"),
                    EventOptionSemantics {
                        action: EventActionKind::Leave,
                        effects: vec![],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            }
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
                    run_state.change_gold_with_source(
                        -COST_1,
                        DomainEventSource::Event(EventId::WomanInBlue),
                    );
                    open_potion_rewards(engine_state, run_state, &mut event_state, 1);
                    return;
                }
                1 => {
                    run_state.change_gold_with_source(
                        -COST_2,
                        DomainEventSource::Event(EventId::WomanInBlue),
                    );
                    open_potion_rewards(engine_state, run_state, &mut event_state, 2);
                    return;
                }
                2 => {
                    run_state.change_gold_with_source(
                        -COST_3,
                        DomainEventSource::Event(EventId::WomanInBlue),
                    );
                    open_potion_rewards(engine_state, run_state, &mut event_state, 3);
                    return;
                }
                _ => {
                    // Leave (A15: take HP loss)
                    if run_state.ascension_level >= 15 {
                        let dmg = ((run_state.max_hp as f32 * 0.05).ceil()) as i32;
                        super::apply_player_hp_loss_damage(
                            run_state,
                            dmg,
                            DomainEventSource::Event(EventId::WomanInBlue),
                        );
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

fn open_potion_rewards(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    event_state: &mut EventState,
    count: usize,
) {
    let mut rewards = RewardState::new();
    for _ in 0..count {
        rewards.items.push(RewardItem::Potion {
            potion_id: run_state.random_potion_flat(),
        });
    }
    event_state.current_screen = 1;
    run_state.event_state = Some(event_state.clone());
    *engine_state = EngineState::RewardScreen(rewards);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::{RelicId, RelicState};
    use crate::rewards::state::RewardItem;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    #[test]
    fn three_potion_option_exposes_trade_semantics() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 0;
        let state = EventState::new(crate::state::events::EventId::WomanInBlue);
        let options = get_options(&rs, &state);
        assert!(
            !options[2].ui.disabled,
            "Java WomanInBlue potion buttons are not disabled by gold"
        );
        assert!(matches!(
            options[2].semantics.effects.as_slice(),
            [
                EventEffect::LoseGold(40),
                EventEffect::ObtainPotion { count: 3 }
            ]
        ));
        assert!(options[2].semantics.constraints.is_empty());
        assert_eq!(
            options[2].semantics.transition,
            EventOptionTransition::OpenReward
        );
    }

    #[test]
    fn buying_potions_opens_reward_screen_without_filling_slots_directly() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 50;
        let starting_potions = rs.potions.clone();
        let potion_rng_before = rs.rng_pool.potion_rng.counter;
        rs.event_state = Some(EventState::new(EventId::WomanInBlue));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 2);

        assert_eq!(rs.gold, 10);
        assert_eq!(rs.potions, starting_potions);
        assert_eq!(
            rs.rng_pool.potion_rng.counter,
            potion_rng_before + 3,
            "Java WomanInBlue uses PotionHelper.getRandomPotion(), one flat potionRng index per potion reward"
        );
        assert!(rs.take_emitted_events().iter().any(|event| {
            matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: -40,
                    new_total: 10,
                    source: DomainEventSource::Event(EventId::WomanInBlue)
                }
            )
        }));
        match engine_state {
            EngineState::RewardScreen(rewards) => {
                assert_eq!(rewards.items.len(), 3);
                assert!(rewards
                    .items
                    .iter()
                    .all(|item| matches!(item, RewardItem::Potion { .. })));
            }
            other => panic!("expected reward screen, got {other:?}"),
        }
    }

    #[test]
    fn buying_potions_with_insufficient_gold_clamps_gold_like_java() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.gold = 7;
        rs.event_state = Some(EventState::new(EventId::WomanInBlue));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 2);

        assert_eq!(rs.gold, 0);
        assert!(rs.take_emitted_events().iter().any(|event| {
            matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: -7,
                    new_total: 0,
                    source: DomainEventSource::Event(EventId::WomanInBlue)
                }
            )
        }));
        match engine_state {
            EngineState::RewardScreen(rewards) => {
                assert_eq!(rewards.items.len(), 3);
                assert!(rewards
                    .items
                    .iter()
                    .all(|item| matches!(item, RewardItem::Potion { .. })));
            }
            other => panic!("expected reward screen, got {other:?}"),
        }
    }

    #[test]
    fn ascension_leave_hp_loss_uses_event_source_and_tungsten_rod() {
        let mut rs = RunState::new(1, 15, true, "Ironclad");
        rs.current_hp = 20;
        rs.max_hp = 80;
        rs.relics.push(RelicState::new(RelicId::TungstenRod));
        rs.event_state = Some(EventState::new(EventId::WomanInBlue));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 3);

        assert_eq!(rs.current_hp, 17);
        assert!(rs.take_emitted_events().iter().any(|event| {
            matches!(
                event,
                DomainEvent::HpChanged {
                    delta: -3,
                    current_hp: 17,
                    max_hp: 80,
                    source: DomainEventSource::Event(EventId::WomanInBlue)
                }
            )
        }));
    }
}
