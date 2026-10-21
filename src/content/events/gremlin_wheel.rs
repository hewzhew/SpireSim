// Java: GremlinWheelGame (shrines) — "Wheel of Change"
// Spin wheel → result 0-5:
//   0 = Gain gold (100/200/300 by act)
//   1 = Obtain random relic
//   2 = Heal to full HP
//   3 = Obtain Decay curse
//   4 = Remove a card (grid-select purge)
//   5 = Lose HP (10% max, 15% at A15+)
// Screen 0: [Play] rolls the result, then screen 3 exposes the wheel button.
// Screen 3: [Spin] settles animation and preApplyResult (gold only).
// Screen 1: [Confirm] applies the other result, then screen 2: [Leave].
// Java keeps INTRO for the wheel button; 3 names that distinct input boundary.

use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRandomOutcomeKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![EventOption::new(
            EventChoiceMeta::new("[Play] Approach the wheel"),
            EventOptionSemantics {
                action: EventActionKind::Special,
                effects: vec![EventEffect::RandomOutcome {
                    kind: EventRandomOutcomeKind::GremlinWheel,
                    chance_percent: None,
                }],
                transition: EventOptionTransition::None,
                ..Default::default()
            },
        )],
        1 => {
            // The result is visible, but only gold has been applied yet.
            let desc = match event_state.internal_state {
                0 => {
                    let gold = match run_state.act_num {
                        1 => 100,
                        2 => 200,
                        _ => 300,
                    };
                    format!("You gained {} Gold!", gold)
                }
                1 => "Obtain a random relic".to_string(),
                2 => "Heal to full HP".to_string(),
                3 => "Obtain Decay".to_string(),
                4 => "Remove a card".to_string(),
                _ => {
                    let pct = if run_state.ascension_level >= 15 {
                        15
                    } else {
                        10
                    };
                    format!("Lose {}% max HP", pct)
                }
            };
            vec![EventOption::new(
                EventChoiceMeta::new(format!("[Confirm] {}", desc)),
                EventOptionSemantics {
                    action: EventActionKind::Special,
                    transition: EventOptionTransition::None,
                    ..Default::default()
                },
            )]
        }
        3 => vec![EventOption::new(
            EventChoiceMeta::new("[Spin] Spin the wheel!"),
            EventOptionSemantics {
                action: EventActionKind::Special,
                transition: EventOptionTransition::None,
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

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, _choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    if event_state.completed {
        run_state.event_state = Some(event_state);
        return;
    }

    match event_state.current_screen {
        0 => {
            // Spin: roll 0-5 using miscRng (Java: miscRng.random(0, 5))
            let result = run_state.rng_pool.misc_rng.random_range(0, 5);
            event_state.internal_state = result;
            event_state.current_screen = 3;
        }
        3 => {
            // preApplyResult runs after animation, before confirmation.
            if event_state.internal_state == 0 {
                let gold = match run_state.act_num {
                    1 => 100,
                    2 => 200,
                    _ => 300,
                };
                run_state.change_gold_with_source(
                    gold,
                    DomainEventSource::Event(EventId::GremlinWheelGame),
                );
            }
            event_state.current_screen = 1;
        }
        1 => {
            let result = event_state.internal_state;
            // applyResult runs only when the revealed outcome is confirmed.
            match result {
                0 => {}
                1 => {
                    // Random relic — Java: addRelicToRewards(r) + combatRewardScreen.open()
                    let relic = run_state.random_screenless_relic_reward();
                    let mut rewards = crate::rewards::state::RewardState::new();
                    rewards
                        .items
                        .push(crate::rewards::state::RewardItem::Relic { relic_id: relic });
                    event_state.internal_state = result;
                    event_state.current_screen = 2;
                    run_state.event_state = Some(event_state);
                    *engine_state = EngineState::RewardScreen(rewards);
                    return;
                }
                2 => {
                    // Java: player.heal(player.maxHealth)
                    run_state.heal_with_source(
                        run_state.max_hp,
                        DomainEventSource::Event(EventId::GremlinWheelGame),
                    );
                }
                3 => {
                    // Obtain Decay curse
                    super::obtain_event_card(
                        run_state,
                        crate::state::events::EventId::GremlinWheelGame,
                        crate::content::cards::CardId::Decay,
                    );
                }
                4 => {
                    // Remove a card (Java: grid-select purge)
                    if crate::state::core::has_non_bottled_purgeable_master_deck_card(run_state) {
                        *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                            selected: Vec::new(),
                            min_choices: 1,
                            max_choices: 1,
                            reason: RunPendingChoiceReason::PurgeNonBottled,
                            source: DomainEventSource::Event(EventId::GremlinWheelGame),
                            return_state: Box::new(EngineState::EventRoom),
                        });
                    }
                }
                _ => {
                    // Java: player.damage(DamageInfo(null, damage, HP_LOSS)).
                    // HP_LOSS bypasses block, but AbstractPlayer.damage still applies
                    // onLoseHpLast, so Tungsten Rod reduces this by 1.
                    let pct = if run_state.ascension_level >= 15 {
                        0.15f32
                    } else {
                        0.10f32
                    };
                    let damage = (run_state.max_hp as f32 * pct) as i32;
                    super::apply_player_hp_loss_damage(
                        run_state,
                        damage,
                        DomainEventSource::Event(EventId::GremlinWheelGame),
                    );
                }
            }

            event_state.internal_state = result;
            event_state.current_screen = 2;
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
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::runtime::rng::StsRng;
    use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason};
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventRandomOutcomeKind,
        EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn resolve_wheel(engine: &mut EngineState, run: &mut RunState) {
        for _ in 0..3 {
            handle_choice(engine, run, 0);
        }
    }

    #[test]
    fn wheel_result_waits_for_confirmation_and_gold_is_applied_only_after_spin() {
        for result in 0..=5 {
            let mut run = wheel_run(30, 80, 20);
            run.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(result));
            let original_gold = run.gold;
            let original_deck = run.master_deck.clone();
            let mut engine = EngineState::EventRoom;
            handle_choice(&mut engine, &mut run, 0);
            assert_eq!(run.event_state.as_ref().unwrap().current_screen, 3);
            assert_eq!(run.event_state.as_ref().unwrap().internal_state, result);
            assert_eq!(run.rng_pool.misc_rng.counter, 1);
            assert_eq!(run.gold, original_gold);
            assert_eq!(run.current_hp, 30);
            assert_eq!(run.master_deck, original_deck);
            assert!(run.take_emitted_events().is_empty());
            handle_choice(&mut engine, &mut run, 0);
            assert_eq!(run.event_state.as_ref().unwrap().current_screen, 1);
            assert_eq!(run.rng_pool.misc_rng.counter, 1);
            assert_eq!(run.current_hp, 30);
            assert_eq!(run.master_deck, original_deck);
            assert_eq!(run.gold, original_gold + if result == 0 { 100 } else { 0 });
            assert_eq!(engine, EngineState::EventRoom);
            handle_choice(&mut engine, &mut run, 0);
            assert_eq!(run.event_state.as_ref().unwrap().current_screen, 2);
            assert_eq!(run.gold, original_gold + if result == 0 { 100 } else { 0 });
            if result == 2 {
                assert_eq!(run.current_hp, 80);
            }
            if result == 4 {
                assert!(matches!(engine, EngineState::RunPendingChoice(_)));
            }
        }
    }

    fn seed_for_wheel_result(result: i32) -> u64 {
        (1..10_000)
            .find(|seed| {
                let mut rng = StsRng::new(*seed);
                rng.random_range(0, 5) == result
            })
            .expect("test seed for Gremlin Wheel result")
    }

    fn wheel_run(current_hp: i32, max_hp: i32, ascension_level: u8) -> RunState {
        let mut run_state = RunState::new(1, ascension_level, false, "Ironclad");
        run_state.current_hp = current_hp;
        run_state.max_hp = max_hp;
        run_state.event_state = Some(EventState::new(EventId::GremlinWheelGame));
        run_state.emitted_events.clear();
        run_state
    }

    fn deck_card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    #[test]
    fn structured_spin_exposes_random_wheel_outcome_boundary() {
        let run_state = wheel_run(20, 80, 0);

        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].semantics.action, EventActionKind::Special);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::RandomOutcome {
                kind: EventRandomOutcomeKind::GremlinWheel,
                chance_percent: None,
            }]
        );
        assert_eq!(options[0].semantics.transition, EventOptionTransition::None);
    }

    #[test]
    fn structured_result_screen_is_terminal_leave_without_reapplying_result() {
        let run_state = wheel_run(20, 80, 0);
        let mut event_state = EventState::new(EventId::GremlinWheelGame);
        event_state.current_screen = 2;
        event_state.internal_state = 0;

        let options = get_options(&run_state, &event_state);

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].semantics.action, EventActionKind::Leave);
        assert!(options[0].semantics.effects.is_empty());
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        assert!(options[0].semantics.terminal);
    }

    #[test]
    fn full_heal_uses_java_heal_source_and_respects_mark_of_the_bloom() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(2));
        run_state
            .relics
            .push(RelicState::new(RelicId::MarkOfTheBloom));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert_eq!(run_state.current_hp, 20);
        assert!(!run_state
            .take_emitted_events()
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
    }

    #[test]
    fn full_heal_emits_event_source_without_mark() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(2));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert_eq!(run_state.current_hp, 80);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 60,
                current_hp: 80,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::GremlinWheelGame),
            }
        )));
    }

    #[test]
    fn hp_loss_result_uses_source_and_can_reduce_hp_to_zero() {
        let mut run_state = wheel_run(5, 80, 0);
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(5));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert_eq!(run_state.current_hp, 0);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -5,
                current_hp: 0,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::GremlinWheelGame),
            }
        )));
    }

    #[test]
    fn hp_loss_result_applies_tungsten_rod_on_lose_hp_last() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(5));
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert_eq!(run_state.current_hp, 13);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -7,
                current_hp: 13,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::GremlinWheelGame),
            }
        )));
    }

    #[test]
    fn gold_result_uses_act_scaled_gold_and_event_source() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.act_num = 2;
        run_state.gold = 10;
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(0));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert_eq!(run_state.gold, 210);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: 200,
                new_total: 210,
                source: DomainEventSource::Event(EventId::GremlinWheelGame),
            }
        )));
    }

    #[test]
    fn relic_result_opens_reward_screen_with_one_relic_reward() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(1));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        let EngineState::RewardScreen(rewards) = engine_state else {
            panic!("relic result should open reward screen");
        };
        assert_eq!(rewards.items.len(), 1);
        assert!(matches!(
            rewards.items[0],
            crate::rewards::state::RewardItem::Relic { .. }
        ));
    }

    #[test]
    fn curse_result_uses_obtain_pipeline_so_omamori_can_block_decay() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(3));
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Decay));
        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori should remain after blocking the curse");
        assert_eq!(omamori.counter, 1);
    }

    #[test]
    fn curse_result_runs_obtain_hooks_before_decay_add_like_show_card_effect() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.gold = 0;
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(3));
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert_eq!(run_state.gold, 9);
        assert!(run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Decay));
        let labels = run_state
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::GremlinWheelGame),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::GremlinWheelGame),
                } if card.id == CardId::Decay => Some("decay_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec!["ceramic_fish_gold", "decay_obtained"],
            "Java ShowCardAndObtainEffect later runs onObtainCard before Soul.obtain for the wheel curse result"
        );
    }

    #[test]
    fn purge_result_opens_non_bottled_purge_selection_when_possible() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(4));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert!(matches!(
            engine_state,
            EngineState::RunPendingChoice(ref pending)
                if pending.reason == RunPendingChoiceReason::PurgeNonBottled
                    && pending.min_choices == 1
                    && pending.max_choices == 1
        ));
    }

    #[test]
    fn purge_result_without_purgeable_card_advances_without_pending_like_java() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.master_deck = vec![deck_card(CardId::AscendersBane, 101)];
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(4));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 2);
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn purge_result_selection_excludes_bottled_and_unpurgeable_cards_like_java() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.master_deck = vec![
            deck_card(CardId::Strike, 101),
            deck_card(CardId::Defend, 102),
            deck_card(CardId::AscendersBane, 103),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(101).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(4));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Gremlin Wheel purge result should open deck selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::PurgeNonBottled);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Purge);
        assert_eq!(
            request.targets,
            vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(102)
            )],
            "Java opens CardGroup.getGroupWithoutBottledCards(masterDeck.getPurgeableCards())"
        );
    }

    #[test]
    fn purge_result_removes_selected_card_with_event_source() {
        let mut run_state = wheel_run(20, 80, 0);
        run_state.master_deck = vec![deck_card(CardId::Strike, 101)];
        run_state.rng_pool.misc_rng = StsRng::new(seed_for_wheel_result(4));
        let mut engine_state = EngineState::EventRoom;

        resolve_wheel(&mut engine_state, &mut run_state);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 2);
        assert!(run_state.master_deck.is_empty());
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::GremlinWheelGame),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )));
    }
}
