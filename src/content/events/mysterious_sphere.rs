use crate::content::monsters::factory::{build_encounter, EncounterId};
use crate::state::core::{CombatStartRequest, EngineState, PostCombatReturn, PreparedEncounter};
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;

pub fn init_event_state(run: &mut RunState, event: &mut EventState) {
    // Java constructs the room's MonsterGroup in MysteriousSphere's constructor.
    // HP is fixed even if the player later leaves; AI and pre-battle hooks wait.
    event.prepared_encounter = Some(PreparedEncounter {
        encounter_id: EncounterId::TwoOrbWalkers,
        monsters: build_encounter(
            EncounterId::TwoOrbWalkers,
            &mut run.rng_pool.misc_rng,
            &mut run.rng_pool.monster_hp_rng,
            run.ascension_level,
        ),
    });
}

fn fight_gold_range(run_state: &RunState) -> (i32, i32) {
    if run_state.is_daily_run {
        (0, 49)
    } else {
        (45, 55)
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![
            EventOption::new(
                EventChoiceMeta::new("[Open] Fight the guardians for a rare Relic!"),
                EventOptionSemantics {
                    action: EventActionKind::Fight,
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
        ],
        1 => {
            // Confirm fight
            let (min, max) = fight_gold_range(run_state);
            vec![EventOption::new(
                EventChoiceMeta::new("[Fight]"),
                EventOptionSemantics {
                    action: EventActionKind::Fight,
                    effects: vec![
                        EventEffect::GainGoldRange { min, max },
                        EventEffect::ObtainRelic {
                            count: 1,
                            kind: EventRelicKind::RandomRareRelic,
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
                    // Open — advance to confirm screen
                    event_state.current_screen = 1;
                }
                _ => {
                    // Java first moves to END text, then opens the map on the
                    // next click.
                    event_state.current_screen = 2;
                }
            }
        }
        1 => {
            // Fight! Set up rewards and enter event combat.
            let prepared_encounter = event_state
                .prepared_encounter
                .take()
                .expect("Mysterious Sphere guardians must exist before confirming combat");
            // Java: daily uses miscRng.random(50), ordinary runs use miscRng.random(45, 55).
            let gold = if run_state.is_daily_run {
                run_state.rng_pool.misc_rng.random(50)
            } else {
                run_state.rng_pool.misc_rng.random_range(45, 55)
            };
            let mut rewards = crate::rewards::state::RewardState::new();
            rewards
                .items
                .push(crate::rewards::state::RewardItem::Gold { amount: gold });

            let relic_id =
                run_state.random_screenless_relic(crate::content::relics::RelicTier::Rare);
            rewards
                .items
                .push(crate::rewards::state::RewardItem::Relic { relic_id });

            event_state.completed = true;
            run_state.event_state = Some(event_state);

            // Transition to event combat
            let mut request = CombatStartRequest::event(
                EncounterId::TwoOrbWalkers,
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
        _ => {
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicId;
    use crate::state::core::CombatContext;
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventRelicKind,
    };

    fn initialized_run(ascension: u8) -> RunState {
        let mut run = RunState::new(1, ascension, false, "Ironclad");
        let mut event = EventState::new(EventId::MysteriousSphere);
        init_event_state(&mut run, &mut event);
        run.event_state = Some(event);
        run
    }

    #[test]
    fn constructor_fixes_guardian_hp_before_any_choice_without_rolling_ai() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
        for ascension in [0, 6, 7, 20] {
            let mut run = RunState::new(29, ascension, false, "Ironclad");
            let ai_before = run.rng_pool.ai_rng.clone();
            let misc_before = run.rng_pool.misc_rng.clone();
            let mut event = EventState::new(EventId::MysteriousSphere);
            let trace = begin_sts_rng_trace().unwrap();
            init_event_state(&mut run, &mut event);
            let calls = trace.finish().unwrap();
            assert_eq!(calls.len(), 4);
            let prepared = event.prepared_encounter.as_ref().unwrap();
            assert_eq!(prepared.encounter_id, EncounterId::TwoOrbWalkers);
            assert_eq!(prepared.monsters.len(), 2);
            for (index, monster) in prepared.monsters.iter().enumerate() {
                assert!(matches!(
                    calls[index * 2].call,
                    StsRngCall::RandomIntRange {
                        start: 90,
                        end: 96,
                        ..
                    }
                ));
                let StsRngCall::RandomIntRange { start, end, result } = calls[index * 2 + 1].call
                else {
                    panic!("setHp must consume the second bounded HP draw");
                };
                assert_eq!(
                    (start, end),
                    if ascension >= 7 { (92, 102) } else { (90, 96) }
                );
                assert_eq!((monster.current_hp, monster.max_hp), (result, result));
                assert_eq!(monster.slot as usize, index);
                assert!(monster.move_history().is_empty());
            }
            assert_eq!(run.rng_pool.monster_hp_rng.counter, 4);
            assert_eq!(run.rng_pool.ai_rng, ai_before);
            assert_eq!(run.rng_pool.misc_rng, misc_before);
            run.event_state = Some(event);
            let hp_after_init = run.rng_pool.monster_hp_rng.clone();
            let mut engine = EngineState::EventRoom;
            handle_choice(&mut engine, &mut run, 1);
            handle_choice(&mut engine, &mut run, 0);
            assert!(run.event_state.as_ref().unwrap().completed);
            assert_eq!(
                run.rng_pool.monster_hp_rng, hp_after_init,
                "leaving preserves constructor draws and never rerolls guardians"
            );
        }
    }

    #[test]
    fn fight_hands_saved_guardians_to_shared_combat_start_without_rerolling_hp() {
        let mut run = initialized_run(20);
        let prepared = run
            .event_state
            .as_ref()
            .unwrap()
            .prepared_encounter
            .clone()
            .unwrap();
        let hp_after_init = run.rng_pool.monster_hp_rng.clone();
        let event_bytes = serde_json::to_vec(run.event_state.as_ref().unwrap()).unwrap();
        run.event_state = Some(serde_json::from_slice(&event_bytes).unwrap());
        let mut engine = EngineState::EventRoom;
        handle_choice(&mut engine, &mut run, 0);
        assert_eq!(run.rng_pool.monster_hp_rng, hp_after_init);
        handle_choice(&mut engine, &mut run, 0);
        let EngineState::CombatStart(request) = engine else {
            panic!("confirmation must request combat with the preconstructed guardians");
        };
        assert_eq!(request.prepared_encounter.as_ref(), Some(&prepared));
        assert!(run
            .event_state
            .as_ref()
            .unwrap()
            .prepared_encounter
            .is_none());
        let bytes = serde_json::to_vec(&request).unwrap();
        let mut request = serde_json::from_slice(&bytes).unwrap();
        let (_, combat) =
            crate::sim::combat_start::build_requested_combat_start(&mut run, &mut request).unwrap();
        assert_eq!(combat.rng.monster_hp_rng, hp_after_init);
        assert_eq!(combat.rng.ai_rng.counter, 2);
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
    fn shared_combat_start_rejects_a_different_prepared_encounter_without_consuming_it() {
        let run = initialized_run(20);
        let mut request = CombatStartRequest::room(
            EncounterId::Cultist,
            crate::state::map::node::RoomType::MonsterRoom,
        );
        request.prepared_encounter = run.event_state.as_ref().unwrap().prepared_encounter.clone();
        let before = request.clone();
        let mut run = run;
        let rng_before = run.rng_pool.clone();
        assert!(
            crate::sim::combat_start::build_requested_combat_start(&mut run, &mut request).is_err()
        );
        assert_eq!(request, before);
        assert_eq!(run.rng_pool, rng_before);
    }

    #[test]
    fn structured_options_split_open_leave_and_confirm_fight() {
        let run_state = initialized_run(0);

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            run_state.event_state.as_ref().unwrap(),
        )
        .expect("Mysterious Sphere should expose structured event options");

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Fight);
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut confirm = EventState::new(EventId::MysteriousSphere);
        confirm.current_screen = 1;
        let confirm_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state, &confirm,
            )
            .expect("Mysterious Sphere confirm screen should expose structured event options");

        assert_eq!(confirm_options[0].semantics.action, EventActionKind::Fight);
        assert!(confirm_options[0]
            .semantics
            .effects
            .contains(&EventEffect::GainGoldRange { min: 45, max: 55 }));
        assert!(confirm_options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::RandomRareRelic,
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
    fn leave_path_preserves_java_end_screen_before_map() {
        let mut run_state = initialized_run(0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let event_state = run_state.event_state.as_ref().unwrap();
        assert_eq!(event_state.current_screen, 2);
        assert!(!event_state.completed);
        assert!(matches!(engine_state, EngineState::EventRoom));

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state.event_state.as_ref().unwrap().completed);
    }

    #[test]
    fn fight_path_generates_java_event_rewards_before_event_combat() {
        let mut run_state = initialized_run(0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);

        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("confirmed Mysterious Sphere fight should request CombatStart");
        };
        assert_eq!(request.encounter_id, EncounterId::TwoOrbWalkers);
        let CombatContext::Event(combat) = request.context else {
            panic!("confirmed Mysterious Sphere fight should carry event combat context");
        };
        assert!(combat.reward_allowed);
        assert!(combat
            .rewards
            .items
            .iter()
            .any(|item| matches!(item, crate::rewards::state::RewardItem::Gold { amount } if (45..=55).contains(amount))));
        assert!(combat
            .rewards
            .items
            .iter()
            .any(|item| matches!(item, crate::rewards::state::RewardItem::Relic { .. })));
    }

    #[test]
    fn daily_fight_reward_uses_java_daily_gold_roll() {
        let mut run_state = initialized_run(0);
        run_state.is_daily_run = true;
        let mut expected_rng = run_state.rng_pool.misc_rng.clone();
        let expected_gold = expected_rng.random(50);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("confirmed Mysterious Sphere fight should request CombatStart");
        };
        let CombatContext::Event(combat) = request.context else {
            panic!("confirmed Mysterious Sphere fight should carry event combat context");
        };
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Gold { amount } if *amount == expected_gold
        )));
    }

    #[test]
    fn fight_reward_uses_rare_screenless_relic_pool() {
        let mut run_state = initialized_run(0);
        run_state.rare_relic_pool = vec![RelicId::Mango];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("confirmed Mysterious Sphere fight should request CombatStart");
        };
        let CombatContext::Event(combat) = request.context else {
            panic!("confirmed Mysterious Sphere fight should carry event combat context");
        };
        assert!(combat.rewards.items.iter().any(|item| matches!(
            item,
            crate::rewards::state::RewardItem::Relic {
                relic_id: RelicId::Mango
            }
        )));
    }
}
