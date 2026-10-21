use crate::content::monsters::factory::{build_encounter, EncounterId};
use crate::content::relics::RelicId;
use crate::state::core::{CombatStartRequest, EngineState, PostCombatReturn, PreparedEncounter};
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn init_event_state(run: &mut RunState, event: &mut EventState) {
    // MaskedBandits constructs its MonsterGroup before offering Pay/Fight.
    // AI and pre-battle hooks still wait until the player chooses combat.
    event.prepared_encounter = Some(PreparedEncounter {
        encounter_id: EncounterId::MaskedBandits,
        monsters: build_encounter(
            EncounterId::MaskedBandits,
            &mut run.rng_pool.misc_rng,
            &mut run.rng_pool.monster_hp_rng,
            run.ascension_level,
        ),
    });
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let reward_relic = if run_state.relics.iter().any(|r| r.id == RelicId::RedMask) {
                RelicId::Circlet
            } else {
                RelicId::RedMask
            };
            let gold_effect = if run_state.is_daily_run {
                EventEffect::GainGoldRange { min: 0, max: 30 }
            } else {
                EventEffect::GainGoldRange { min: 25, max: 35 }
            };
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!("[Pay] Lose all ({}) Gold.", run_state.gold)),
                    EventOptionSemantics {
                        action: EventActionKind::Trade,
                        effects: vec![EventEffect::LoseGold(run_state.gold)],
                        transition: EventOptionTransition::AdvanceScreen,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Fight] Engage the bandits!"),
                    EventOptionSemantics {
                        action: EventActionKind::Fight,
                        effects: vec![
                            EventEffect::StartCombat,
                            gold_effect,
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::Specific(reward_relic),
                            },
                        ],
                        transition: EventOptionTransition::StartCombat,
                        ..Default::default()
                    },
                ),
            ]
        }
        1 | 2 => vec![EventOption::new(
            EventChoiceMeta::new("[Continue]"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        )],
        3 => vec![EventOption::new(
            EventChoiceMeta::new("[Continue]"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..Default::default()
            },
        )],
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
                    // Pay all gold
                    run_state
                        .set_gold_with_source(0, DomainEventSource::Event(EventId::MaskedBandits));
                    event_state.current_screen = 1;
                }
                _ => {
                    // Fight bandits
                    let prepared_encounter = event_state
                        .prepared_encounter
                        .take()
                        .expect("Masked Bandits must exist before choosing combat");
                    let gold = if run_state.is_daily_run {
                        run_state.rng_pool.misc_rng.random(30)
                    } else {
                        run_state.rng_pool.misc_rng.random_range(25, 35)
                    };
                    let mut rewards = crate::rewards::state::RewardState::new();
                    rewards
                        .items
                        .push(crate::rewards::state::RewardItem::Gold { amount: gold });

                    if run_state.relics.iter().any(|r| r.id == RelicId::RedMask) {
                        rewards
                            .items
                            .push(crate::rewards::state::RewardItem::Relic {
                                relic_id: RelicId::Circlet,
                            });
                    } else {
                        rewards
                            .items
                            .push(crate::rewards::state::RewardItem::Relic {
                                relic_id: RelicId::RedMask,
                            });
                    }

                    event_state.completed = true;
                    run_state.event_state = Some(event_state);

                    let mut request = CombatStartRequest::event(
                        EncounterId::MaskedBandits,
                        rewards,
                        true,
                        false,
                        false,
                        PostCombatReturn::MapNavigation,
                    );
                    request.prepared_encounter = Some(prepared_encounter);
                    *engine_state = EngineState::CombatStart(request);
                    return;
                }
            }
        }
        1 => {
            event_state.current_screen = 2;
        }
        2 => {
            event_state.current_screen = 3;
        }
        3 => {
            event_state.completed = true;
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
        EventActionKind, EventEffect, EventOptionTransition, EventRelicKind,
    };
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn initialized_event(run: &mut RunState) {
        let mut event = EventState::new(EventId::MaskedBandits);
        init_event_state(run, &mut event);
        run.event_state = Some(event);
    }

    #[test]
    fn constructor_rolls_three_bandit_hp_values_before_pay_or_fight() {
        use crate::content::monsters::EnemyId;
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
        for ascension in [0, 6, 7, 20] {
            let mut run = RunState::new(29, ascension, false, "Defect");
            let ai_before = run.rng_pool.ai_rng.clone();
            let misc_before = run.rng_pool.misc_rng.clone();
            let trace = begin_sts_rng_trace().unwrap();
            initialized_event(&mut run);
            let calls = trace.finish().unwrap();
            let prepared = run
                .event_state
                .as_ref()
                .unwrap()
                .prepared_encounter
                .as_ref()
                .unwrap();
            assert_eq!(calls.len(), 3);
            assert_eq!(prepared.encounter_id, EncounterId::MaskedBandits);
            let expected = if ascension >= 7 {
                [
                    (EnemyId::BanditPointy, 34, 34),
                    (EnemyId::BanditLeader, 37, 41),
                    (EnemyId::BanditBear, 40, 44),
                ]
            } else {
                [
                    (EnemyId::BanditPointy, 30, 30),
                    (EnemyId::BanditLeader, 35, 39),
                    (EnemyId::BanditBear, 38, 42),
                ]
            };
            for (index, (enemy, min, max)) in expected.into_iter().enumerate() {
                let StsRngCall::RandomIntRange { start, end, result } = calls[index].call else {
                    panic!("constructor must roll bounded monster HP");
                };
                assert_eq!((start, end), (min, max));
                let monster = &prepared.monsters[index];
                assert_eq!((monster.enemy_id, monster.slot), (enemy, index as u8));
                assert_eq!((monster.current_hp, monster.max_hp), (result, result));
                assert!(monster.move_history().is_empty());
            }
            assert_eq!(run.rng_pool.monster_hp_rng.counter, 3);
            assert_eq!(run.rng_pool.ai_rng, ai_before);
            assert_eq!(run.rng_pool.misc_rng, misc_before);
            let hp_after_init = run.rng_pool.monster_hp_rng.clone();
            let mut engine = EngineState::EventRoom;
            for _ in 0..4 {
                handle_choice(&mut engine, &mut run, 0);
            }
            assert!(run.event_state.as_ref().unwrap().completed);
            assert_eq!(run.rng_pool.monster_hp_rng, hp_after_init);
        }
    }

    #[test]
    fn combat_reuses_serialized_bandits_without_rerolling_their_hp() {
        let mut run = RunState::new(29, 20, false, "Defect");
        initialized_event(&mut run);
        let prepared = run
            .event_state
            .as_ref()
            .unwrap()
            .prepared_encounter
            .clone()
            .unwrap();
        let hp_after_init = run.rng_pool.monster_hp_rng.clone();
        let bytes = serde_json::to_vec(run.event_state.as_ref().unwrap()).unwrap();
        run.event_state = Some(serde_json::from_slice(&bytes).unwrap());
        let mut engine = EngineState::EventRoom;
        handle_choice(&mut engine, &mut run, 1);
        let EngineState::CombatStart(mut request) = engine else {
            panic!("fight must request combat");
        };
        assert_eq!(request.prepared_encounter.as_ref(), Some(&prepared));
        assert!(run
            .event_state
            .as_ref()
            .unwrap()
            .prepared_encounter
            .is_none());
        let (_, combat) =
            crate::sim::combat_start::build_requested_combat_start(&mut run, &mut request).unwrap();
        assert_eq!(combat.rng.monster_hp_rng, hp_after_init);
        assert_eq!(combat.rng.ai_rng.counter, 3);
        for (before, after) in prepared.monsters.iter().zip(&combat.entities.monsters) {
            assert_eq!(
                (after.enemy_id, after.current_hp, after.max_hp, after.slot),
                (
                    before.enemy_id,
                    before.current_hp,
                    before.max_hp,
                    before.slot
                )
            );
        }
    }

    #[test]
    fn structured_options_expose_pay_and_fight_semantics() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 123;
        let event_state = EventState::new(EventId::MaskedBandits);

        let options = get_options(&run_state, &event_state);

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Trade);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseGold(123)));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        assert_eq!(options[1].semantics.action, EventActionKind::Fight);
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::StartCombat));
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::GainGoldRange { min: 25, max: 35 }));
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::RedMask),
            }));
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::StartCombat
        );
    }

    #[test]
    fn structured_fight_option_exposes_circlet_when_red_mask_is_already_owned() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.push(RelicState::new(RelicId::RedMask));
        let event_state = EventState::new(EventId::MaskedBandits);

        let options = get_options(&run_state, &event_state);

        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::Circlet),
            }));
    }

    #[test]
    fn structured_paid_dialog_continues_until_java_map_open_boundary() {
        let run_state = RunState::new(1, 0, false, "Ironclad");

        let mut screen1 = EventState::new(EventId::MaskedBandits);
        screen1.current_screen = 1;
        let screen1_options = get_options(&run_state, &screen1);
        assert_eq!(
            screen1_options[0].semantics.action,
            EventActionKind::Continue
        );
        assert_eq!(
            screen1_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut screen3 = EventState::new(EventId::MaskedBandits);
        screen3.current_screen = 3;
        let screen3_options = get_options(&run_state, &screen3);
        assert_eq!(
            screen3_options[0].semantics.action,
            EventActionKind::Continue
        );
        assert_eq!(
            screen3_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        assert!(screen3_options[0].semantics.terminal);
    }

    #[test]
    fn pay_path_opens_map_after_java_dialog_sequence_without_extra_leave_click() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 123;
        run_state.event_state = Some(EventState::new(EventId::MaskedBandits));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        assert_eq!(run_state.gold, 0);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: -123,
                new_total: 0,
                source: DomainEventSource::Event(EventId::MaskedBandits),
            }
        )));
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);

        handle_choice(&mut engine_state, &mut run_state, 0);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 2);

        handle_choice(&mut engine_state, &mut run_state, 0);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 3);

        handle_choice(&mut engine_state, &mut run_state, 0);
        assert!(run_state.event_state.as_ref().unwrap().completed);
    }

    #[test]
    fn fight_uses_java_event_encounter_and_event_rewards() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        initialized_event(&mut run_state);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("fight should request CombatStart");
        };
        assert_eq!(request.encounter_id, EncounterId::MaskedBandits);
        let CombatContext::Event(combat) = request.context else {
            panic!("fight should carry event combat context");
        };
        assert!(combat.reward_allowed);
        assert!(combat
            .rewards
            .items
            .iter()
            .any(|item| matches!(item, crate::rewards::state::RewardItem::Gold { amount } if (25..=35).contains(amount))));
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Relic {
                relic_id: RelicId::RedMask
            }
        )));
    }

    #[test]
    fn daily_fight_reward_uses_java_daily_gold_roll() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.is_daily_run = true;
        let mut expected_rng = run_state.rng_pool.misc_rng.clone();
        let expected_gold = expected_rng.random(30);
        initialized_event(&mut run_state);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("fight should request CombatStart");
        };
        let CombatContext::Event(combat) = request.context else {
            panic!("fight should carry event combat context");
        };
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Gold { amount } if *amount == expected_gold
        )));
    }

    #[test]
    fn fight_reward_gives_circlet_when_red_mask_is_already_owned() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.push(RelicState::new(RelicId::RedMask));
        initialized_event(&mut run_state);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("fight should request CombatStart");
        };
        let CombatContext::Event(combat) = request.context else {
            panic!("fight should carry event combat context");
        };
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Relic {
                relic_id: RelicId::Circlet
            }
        )));
    }
}
