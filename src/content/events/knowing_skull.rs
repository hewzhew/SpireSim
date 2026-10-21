use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

// Java KnowingSkull has 4 independent cost counters:
//   potionCost, goldCost, cardCost — each starts at 6, incremented independently per purchase
//   leaveCost — fixed at 6
// We pack 3 counters into internal_state: bits [0..7]=potionN, [8..15]=goldN, [16..23]=cardN

const BASE_COST: i32 = 6;
const GOLD_REWARD: i32 = 90;

fn potion_n(state: i32) -> i32 {
    state & 0xFF
}
fn gold_n(state: i32) -> i32 {
    (state >> 8) & 0xFF
}
fn card_n(state: i32) -> i32 {
    (state >> 16) & 0xFF
}

fn inc_potion(state: &mut i32) {
    *state += 1;
}
fn inc_gold(state: &mut i32) {
    *state += 1 << 8;
}
fn inc_card(state: &mut i32) {
    *state += 1 << 16;
}

fn potion_cost(state: i32) -> i32 {
    BASE_COST + potion_n(state)
}
/// Returns the current HP cost of the Knowing Skull gold option from exact
/// event state.
pub fn gold_cost(state: i32) -> i32 {
    BASE_COST + gold_n(state)
}
fn card_cost(state: i32) -> i32 {
    BASE_COST + card_n(state)
}

pub fn get_choices(_run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(_run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            // Intro screen
            vec![EventOption::new(
                EventChoiceMeta::new("[Proceed]"),
                EventOptionSemantics {
                    action: EventActionKind::Continue,
                    transition: EventOptionTransition::AdvanceScreen,
                    ..Default::default()
                },
            )]
        }
        1 => {
            // ASK screen: repeatable options with independent escalating costs
            let s = event_state.internal_state;
            let mut potion_effects = vec![EventEffect::LoseHp(potion_cost(s))];
            if !run_state
                .relics
                .iter()
                .any(|relic| relic.id == crate::content::relics::RelicId::Sozu)
            {
                potion_effects.push(EventEffect::ObtainPotion { count: 1 });
            }
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Potion] Lose {} HP. Obtain a random Potion.",
                        potion_cost(s)
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: potion_effects,
                        transition: EventOptionTransition::None,
                        repeatable: true,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Gold] Gain {} Gold. Lose {} HP.",
                        GOLD_REWARD,
                        gold_cost(s)
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseHp(gold_cost(s)),
                            EventEffect::GainGold(GOLD_REWARD),
                        ],
                        transition: EventOptionTransition::None,
                        repeatable: true,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Card] Lose {} HP. Obtain a colorless card.",
                        card_cost(s)
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::LoseHp(card_cost(s)),
                            EventEffect::ObtainColorlessCard {
                                count: 1,
                                kind: EventCardKind::RandomColorlessUncommon,
                            },
                        ],
                        transition: EventOptionTransition::None,
                        repeatable: true,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!("[Leave] Lose {} HP.", BASE_COST)),
                    EventOptionSemantics {
                        action: EventActionKind::Leave,
                        effects: vec![EventEffect::LoseHp(BASE_COST)],
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

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();
    let source = DomainEventSource::Event(EventId::KnowingSkull);

    match event_state.current_screen {
        0 => {
            // Intro → ASK
            event_state.current_screen = 1;
        }
        1 => {
            match choice_idx {
                0 => {
                    // Potion: take potionCost damage, get potion, ++potionCost
                    let cost = potion_cost(event_state.internal_state);
                    super::apply_player_hp_loss_damage(run_state, cost, source);
                    inc_potion(&mut event_state.internal_state);
                    if !run_state
                        .relics
                        .iter()
                        .any(|relic| relic.id == crate::content::relics::RelicId::Sozu)
                    {
                        let pid = run_state.random_potion_flat();
                        let potion = crate::content::potions::Potion::new(
                            pid,
                            20000 + potion_n(event_state.internal_state) as u32,
                        );
                        let _ = run_state.obtain_potion_with_source(potion, source);
                    }
                    // Stay on ASK screen (repeatable)
                }
                1 => {
                    // Gold: take goldCost damage, gain 90g, ++goldCost
                    let cost = gold_cost(event_state.internal_state);
                    super::apply_player_hp_loss_damage(run_state, cost, source);
                    inc_gold(&mut event_state.internal_state);
                    run_state.change_gold_with_source(GOLD_REWARD, source);
                    // Stay on ASK screen
                }
                2 => {
                    // Card: take cardCost damage, get colorless card, ++cardCost
                    let cost = card_cost(event_state.internal_state);
                    super::apply_player_hp_loss_damage(run_state, cost, source);
                    inc_card(&mut event_state.internal_state);
                    let card_id = run_state
                        .random_colorless_card(crate::content::cards::CardRarity::Uncommon);
                    super::obtain_event_card(run_state, EventId::KnowingSkull, card_id);
                    // Stay on ASK screen
                }
                _ => {
                    // Leave: take fixed 6 damage, transition to COMPLETE
                    super::apply_player_hp_loss_damage(run_state, BASE_COST, source);
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
    use super::*;
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventOptionTransition,
    };
    use crate::state::selection::DomainEvent;

    fn skull_run() -> RunState {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.current_hp = 30;
        run_state.max_hp = 80;
        run_state.gold = 0;
        run_state.event_state = Some(EventState {
            id: EventId::KnowingSkull,
            current_screen: 1,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn structured_options_expose_repeatable_escalating_costs_and_rewards() {
        let mut run_state = skull_run();
        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 4);
        assert_eq!(options[0].semantics.action, EventActionKind::Trade);
        assert!(options[0].semantics.repeatable);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(6)));
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainPotion { count: 1 }));

        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(6)));
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::GainGold(90)));

        assert!(options[2]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(6)));
        assert!(options[2]
            .semantics
            .effects
            .contains(&EventEffect::ObtainColorlessCard {
                count: 1,
                kind: EventCardKind::RandomColorlessUncommon,
            }));

        assert_eq!(options[3].semantics.action, EventActionKind::Leave);
        assert!(options[3]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(6)));
        assert_eq!(
            options[3].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        run_state.event_state.as_mut().unwrap().internal_state = (2 << 16) | (1 << 8) | 3;
        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(9)));
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(7)));
        assert!(options[2]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(8)));
    }

    #[test]
    fn structured_potion_option_respects_sozu_actual_effect_boundary() {
        let mut run_state = skull_run();
        run_state.relics.push(RelicState::new(RelicId::Sozu));

        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(6)));
        assert!(!options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainPotion { count: 1 }));
    }

    #[test]
    fn structured_intro_and_complete_screens_expose_continue_then_leave() {
        let run_state = skull_run();
        let mut intro = EventState::new(EventId::KnowingSkull);
        intro.current_screen = 0;
        let intro_options = get_options(&run_state, &intro);
        assert_eq!(intro_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            intro_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut complete = EventState::new(EventId::KnowingSkull);
        complete.current_screen = 2;
        let complete_options = get_options(&run_state, &complete);
        assert_eq!(complete_options[0].semantics.action, EventActionKind::Leave);
        assert!(complete_options[0].semantics.terminal);
    }

    #[test]
    fn potion_reward_hp_loss_respects_tungsten_and_increments_only_potion_cost() {
        let mut run_state = skull_run();
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        run_state.relics.push(RelicState::new(RelicId::Sozu));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 25);
        let event_state = run_state.event_state.as_ref().unwrap();
        assert_eq!(event_state.current_screen, 1);
        assert_eq!(potion_cost(event_state.internal_state), 7);
        assert_eq!(gold_cost(event_state.internal_state), 6);
        assert_eq!(card_cost(event_state.internal_state), 6);
    }

    #[test]
    fn potion_reward_without_sozu_uses_flat_potion_helper_rng() {
        let mut run_state = skull_run();
        let potion_rng_before = run_state.rng_pool.potion_rng.counter;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(
            run_state.rng_pool.potion_rng.counter,
            potion_rng_before + 1,
            "Java Knowing Skull uses PotionHelper.getRandomPotion(), not rarity-weighted returnRandomPotion"
        );
        assert!(run_state.potions.iter().any(|slot| slot.is_some()));
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
    }

    #[test]
    fn gold_reward_hp_loss_respects_tungsten_then_grants_gold() {
        let mut run_state = skull_run();
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 25);
        assert_eq!(run_state.gold, 90);
        let event_state = run_state.event_state.as_ref().unwrap();
        assert_eq!(gold_cost(event_state.internal_state), 7);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -5,
                source: DomainEventSource::Event(EventId::KnowingSkull),
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: 90,
                source: DomainEventSource::Event(EventId::KnowingSkull),
                ..
            }
        )));
    }

    #[test]
    fn card_reward_hp_loss_and_random_colorless_card_use_event_source() {
        let mut run_state = skull_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert_eq!(run_state.current_hp, 24);
        let event_state = run_state.event_state.as_ref().unwrap();
        assert_eq!(event_state.current_screen, 1);
        assert_eq!(potion_cost(event_state.internal_state), 6);
        assert_eq!(gold_cost(event_state.internal_state), 6);
        assert_eq!(card_cost(event_state.internal_state), 7);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                source: DomainEventSource::Event(EventId::KnowingSkull),
                ..
            }
        )));
    }

    #[test]
    fn card_reward_hp_loss_resolves_before_delayed_card_obtain_hooks() {
        let mut run_state = skull_run();
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert_eq!(run_state.current_hp, 24);
        assert_eq!(run_state.gold, 9);
        let labels = run_state
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::HpChanged {
                    delta: -6,
                    source: DomainEventSource::Event(EventId::KnowingSkull),
                    ..
                } => Some("hp_loss"),
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::KnowingSkull),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    source: DomainEventSource::Event(EventId::KnowingSkull),
                    ..
                } => Some("card_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec!["hp_loss", "ceramic_fish_gold", "card_obtained"],
            "Java KnowingSkull card reward pays HP first, then delayed ShowCardAndObtainEffect runs onObtainCard before Soul.obtain"
        );
    }

    #[test]
    fn leave_hp_loss_respects_tungsten_and_moves_to_complete_screen() {
        let mut run_state = skull_run();
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 3);

        assert_eq!(run_state.current_hp, 25);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 2);
    }
}
