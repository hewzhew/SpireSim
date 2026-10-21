use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;
use crate::content::relics::RelicId;
use crate::state::core::{CombatStartRequest, EngineState, PostCombatReturn};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn fight_gold_range(run_state: &RunState) -> (i32, i32) {
    if run_state.is_daily_run {
        (0, 24)
    } else {
        (20, 30)
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let heal_amt = (run_state.max_hp as f32 * 0.25) as i32;
            vec![
                EventOption::new(
                    EventChoiceMeta::new("[Stomp] Fight the mushrooms!"),
                    EventOptionSemantics {
                        action: EventActionKind::Fight,
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Eat] Heal {} HP. Become Cursed - Parasite.",
                        heal_amt
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![
                            EventEffect::Heal(heal_amt),
                            EventEffect::ObtainCurse {
                                count: 1,
                                kind: EventCardKind::Specific(CardId::Parasite),
                            },
                        ],
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
            ]
        }
        1 => {
            // Post-heal leave screen.
            vec![EventOption::new(
                EventChoiceMeta::new("[Leave]"),
                EventOptionSemantics {
                    action: EventActionKind::Leave,
                    transition: EventOptionTransition::Complete,
                    terminal: true,
                    ..Default::default()
                },
            )]
        }
        2 => {
            let (min, max) = fight_gold_range(run_state);
            let relic = if run_state
                .relics
                .iter()
                .any(|r| r.id == RelicId::OddMushroom)
            {
                RelicId::Circlet
            } else {
                RelicId::OddMushroom
            };
            vec![EventOption::new(
                EventChoiceMeta::new("[Fight]"),
                EventOptionSemantics {
                    action: EventActionKind::Fight,
                    effects: vec![
                        EventEffect::GainGoldRange { min, max },
                        EventEffect::ObtainRelic {
                            count: 1,
                            kind: EventRelicKind::Specific(relic),
                        },
                        EventEffect::StartCombat,
                    ],
                    transition: EventOptionTransition::StartCombat,
                    terminal: true,
                    ..Default::default()
                },
            )]
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
                    // Java first click only reveals the fight text and confirms the
                    // encounter. Rewards are generated on the next click.
                    event_state.current_screen = 2;
                }
                _ => {
                    // Eat: Heal 25% maxHP + Parasite curse.
                    // Java uses AbstractPlayer.heal and ShowCardAndObtainEffect.
                    let heal_amt = (run_state.max_hp as f32 * 0.25) as i32;
                    run_state
                        .heal_with_source(heal_amt, DomainEventSource::Event(EventId::Mushrooms));
                    super::obtain_event_card(run_state, EventId::Mushrooms, CardId::Parasite);
                    event_state.current_screen = 1;
                }
            }
        }
        2 => {
            // Fight the mushrooms. Java generates rewards immediately before
            // enterCombat().
            let gold = if run_state.is_daily_run {
                run_state.rng_pool.misc_rng.random(25)
            } else {
                run_state.rng_pool.misc_rng.random_range(20, 30)
            };
            let mut rewards = crate::rewards::state::RewardState::new();
            rewards
                .items
                .push(crate::rewards::state::RewardItem::Gold { amount: gold });

            if run_state
                .relics
                .iter()
                .any(|r| r.id == RelicId::OddMushroom)
            {
                rewards
                    .items
                    .push(crate::rewards::state::RewardItem::Relic {
                        relic_id: RelicId::Circlet,
                    });
            } else {
                rewards
                    .items
                    .push(crate::rewards::state::RewardItem::Relic {
                        relic_id: RelicId::OddMushroom,
                    });
            }

            event_state.completed = true;
            run_state.event_state = Some(event_state);

            *engine_state = EngineState::CombatStart(CombatStartRequest::event(
                EncounterId::TheMushroomLair,
                rewards,
                true,
                false,
                false,
                PostCombatReturn::MapNavigation,
            ));
            return;
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
    use crate::content::relics::RelicState;
    use crate::state::core::CombatContext;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventOptionTransition, EventRelicKind,
    };
    use crate::state::selection::DomainEvent;

    #[test]
    fn structured_options_split_prepare_fight_confirm_fight_and_eat() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state.event_state = Some(EventState::new(EventId::Mushrooms));

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            run_state.event_state.as_ref().unwrap(),
        )
        .expect("Mushrooms should expose structured event options");

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Fight);
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Trade);
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::Heal(20)));
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainCurse {
                count: 1,
                kind: EventCardKind::Specific(CardId::Parasite),
            }));

        let mut confirm = EventState::new(EventId::Mushrooms);
        confirm.current_screen = 2;
        let confirm_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state, &confirm,
            )
            .expect("Mushrooms confirm screen should expose structured event options");

        assert_eq!(confirm_options[0].semantics.action, EventActionKind::Fight);
        assert!(confirm_options[0]
            .semantics
            .effects
            .contains(&EventEffect::GainGoldRange { min: 20, max: 30 }));
        assert!(confirm_options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::OddMushroom),
            }));
        assert!(confirm_options[0]
            .semantics
            .effects
            .contains(&EventEffect::StartCombat));
        assert_eq!(
            confirm_options[0].semantics.transition,
            EventOptionTransition::StartCombat
        );
    }

    #[test]
    fn structured_fight_reward_exposes_circlet_when_odd_mushroom_is_already_owned() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.push(RelicState::new(RelicId::OddMushroom));
        let mut event_state = EventState::new(EventId::Mushrooms);
        event_state.current_screen = 2;

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            &event_state,
        )
        .expect("Mushrooms confirm screen should expose structured event options");

        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::Circlet),
            }));
    }

    #[test]
    fn fight_path_requires_java_confirm_screen_before_combat() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(matches!(engine_state, EngineState::EventRoom));
        let event_state = run_state.event_state.as_ref().unwrap();
        assert_eq!(event_state.current_screen, 2);
        assert!(!event_state.completed);

        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("confirmed Mushrooms fight should request CombatStart");
        };
        assert_eq!(request.encounter_id, EncounterId::TheMushroomLair);
        let CombatContext::Event(combat) = request.context else {
            panic!("confirmed Mushrooms fight should carry event combat context");
        };
        assert!(combat.reward_allowed);
        assert!(combat
            .rewards
            .items
            .iter()
            .any(|item| matches!(item, crate::rewards::state::RewardItem::Gold { amount } if (20..=30).contains(amount))));
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Relic {
                relic_id: RelicId::OddMushroom
            }
        )));
    }

    #[test]
    fn daily_fight_reward_uses_java_daily_gold_roll() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.is_daily_run = true;
        let mut expected_rng = run_state.rng_pool.misc_rng.clone();
        let expected_gold = expected_rng.random(25);
        run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("confirmed Mushrooms fight should request CombatStart");
        };
        let CombatContext::Event(combat) = request.context else {
            panic!("confirmed Mushrooms fight should carry event combat context");
        };
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Gold { amount } if *amount == expected_gold
        )));
    }

    #[test]
    fn eat_uses_player_heal_and_show_card_obtain_semantics() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 40);
        assert_eq!(run_state.master_deck.last().unwrap().id, CardId::Parasite);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 20,
                source: DomainEventSource::Event(EventId::Mushrooms),
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Event(EventId::Mushrooms),
            } if card.id == CardId::Parasite
        )));
    }

    #[test]
    fn eat_heal_is_blocked_by_mark_of_the_bloom_but_curse_obtain_still_runs() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state
            .relics
            .push(RelicState::new(RelicId::MarkOfTheBloom));
        run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 20);
        assert_eq!(run_state.master_deck.last().unwrap().id, CardId::Parasite);
    }

    #[test]
    fn fight_reward_gives_circlet_when_odd_mushroom_is_already_owned() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.push(RelicState::new(RelicId::OddMushroom));
        run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("confirmed Mushrooms fight should request CombatStart");
        };
        let CombatContext::Event(combat) = request.context else {
            panic!("confirmed Mushrooms fight should carry event combat context");
        };
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Relic {
                relic_id: RelicId::Circlet
            }
        )));
    }

    #[test]
    fn eat_parasite_can_be_blocked_by_omamori_like_show_card_and_obtain_effect() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 40);
        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Parasite));
        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori should remain after blocking the curse");
        assert_eq!(omamori.counter, 1);
    }

    #[test]
    fn eat_heal_resolves_before_delayed_parasite_obtain_like_java_effect_list() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state.gold = 0;
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 40);
        assert_eq!(run_state.gold, 9);
        let labels = run_state
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::HpChanged {
                    delta: 20,
                    source: DomainEventSource::Event(EventId::Mushrooms),
                    ..
                } => Some("heal"),
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::Mushrooms),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::Mushrooms),
                } if card.id == CardId::Parasite => Some("parasite_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec!["heal", "ceramic_fish_gold", "parasite_obtained"],
            "Java heals immediately, then delayed ShowCardAndObtainEffect runs onObtainCard before Soul.obtain"
        );
    }
}
