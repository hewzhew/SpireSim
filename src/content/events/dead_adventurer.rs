use crate::content::monsters::factory::EncounterId;
use crate::state::core::{CombatStartRequest, EngineState, PostCombatReturn};
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRandomOutcomeKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

// internal_state packing:
// bits[0..3]  = numRewards (how many times player has looted, 0-3)
// bits[4..11] = encounterChance (0-100)
// bits[12..13] = reward0 type (0=Gold, 1=Nothing, 2=Relic)
// bits[14..15] = reward1 type
// bits[16..17] = reward2 type
// bits[18..19] = enemy (0=3 Sentries, 1=Gremlin Nob, 2=Lagavulin Event)
// bits[20..25] = combat gold reward rolled when the fight is revealed

const GOLD_REWARD: i32 = 30;
const CHANCE_RAMP: i32 = 25;

/// Decodes the number of rewards already granted from the event's exact
/// compact internal state.
pub fn num_rewards(s: i32) -> i32 {
    s & 0xF
}
fn encounter_chance(s: i32) -> i32 {
    (s >> 4) & 0xFF
}
fn reward_type(s: i32, idx: i32) -> i32 {
    (s >> (12 + idx * 2)) & 0x3
}
fn enemy_type(s: i32) -> i32 {
    (s >> 18) & 0x3
}
fn combat_gold_reward(s: i32) -> i32 {
    (s >> 20) & 0x3F
}

fn set_num_rewards(s: &mut i32, n: i32) {
    *s = (*s & !0xF) | (n & 0xF);
}
fn set_encounter_chance(s: &mut i32, c: i32) {
    *s = (*s & !(0xFF << 4)) | ((c & 0xFF) << 4);
}
fn set_reward_types(s: &mut i32, r0: i32, r1: i32, r2: i32) {
    *s = (*s & !(0x3F << 12)) | ((r0 & 0x3) << 12) | ((r1 & 0x3) << 14) | ((r2 & 0x3) << 16);
}
fn set_enemy_type(s: &mut i32, enemy: i32) {
    *s = (*s & !(0x3 << 18)) | ((enemy & 0x3) << 18);
}
fn set_combat_gold_reward(s: &mut i32, gold: i32) {
    *s = (*s & !(0x3F << 20)) | ((gold & 0x3F) << 20);
}

fn encounter_id_for_enemy(enemy: i32) -> EncounterId {
    match enemy {
        0 => EncounterId::ThreeSentries,
        1 => EncounterId::GremlinNob,
        _ => EncounterId::LagavulinEvent,
    }
}

pub fn get_options(_run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            let chance = encounter_chance(event_state.internal_state);
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!("[Search] {}% chance of a fight.", chance)),
                    EventOptionSemantics {
                        action: EventActionKind::Special,
                        effects: vec![EventEffect::RandomOutcome {
                            kind: EventRandomOutcomeKind::DeadAdventurerSearch,
                            chance_percent: Some(chance.clamp(0, 100) as u8),
                        }],
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
        // Combat triggered
        1 => vec![EventOption::new(
            EventChoiceMeta::new("[Fight!]"),
            EventOptionSemantics {
                action: EventActionKind::Fight,
                effects: vec![EventEffect::StartCombat],
                transition: EventOptionTransition::StartCombat,
                terminal: true,
                ..Default::default()
            },
        )],
        // Post-loot or post-combat
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
                    // Search: check encounter chance
                    let chance = encounter_chance(event_state.internal_state);
                    let roll = run_state.rng_pool.misc_rng.random_range(0, 99);
                    if roll < chance {
                        // Combat!
                        // Java first reveals the fight and adds the 25-35 gold
                        // reward. Remaining corpse rewards are added on the next
                        // click immediately before enterCombat().
                        let combat_gold = if run_state.is_daily_run {
                            run_state.rng_pool.misc_rng.random(30)
                        } else {
                            run_state.rng_pool.misc_rng.random_range(25, 35)
                        };
                        set_combat_gold_reward(&mut event_state.internal_state, combat_gold);
                        event_state.current_screen = 1;
                        run_state.event_state = Some(event_state);
                        return;
                    }
                    // Safe loot
                    let n = num_rewards(event_state.internal_state);
                    let rt = reward_type(event_state.internal_state, n);
                    apply_reward(run_state, rt);

                    let new_n = n + 1;
                    set_num_rewards(&mut event_state.internal_state, new_n);
                    let new_chance = encounter_chance(event_state.internal_state) + CHANCE_RAMP;
                    set_encounter_chance(&mut event_state.internal_state, new_chance);

                    if new_n >= 3 {
                        // All 3 rewards claimed, done
                        event_state.current_screen = 2;
                    }
                    // else stay on screen 0
                }
                _ => {
                    // Leave
                    event_state.current_screen = 2;
                }
            }
        }
        1 => {
            let rewards = build_combat_rewards(run_state, event_state.internal_state);
            let encounter_id = encounter_id_for_enemy(enemy_type(event_state.internal_state));
            event_state.current_screen = 2;
            event_state.completed = true;
            run_state.event_state = Some(event_state);
            *engine_state = EngineState::CombatStart(CombatStartRequest::event(
                encounter_id,
                rewards,
                true,
                false,
                true,
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

fn build_combat_rewards(
    run_state: &mut RunState,
    internal_state: i32,
) -> crate::rewards::state::RewardState {
    let mut rewards = crate::rewards::state::RewardState::new();
    rewards.items.push(crate::rewards::state::RewardItem::Gold {
        amount: combat_gold_reward(internal_state),
    });
    let n = num_rewards(internal_state) as usize;
    for i in n..3 {
        let rt = reward_type(internal_state, i as i32);
        match rt {
            0 => rewards.items.push(crate::rewards::state::RewardItem::Gold {
                amount: GOLD_REWARD,
            }),
            2 => {
                let relic_id = run_state.random_relic();
                rewards
                    .items
                    .push(crate::rewards::state::RewardItem::Relic { relic_id });
            }
            _ => {}
        }
    }
    rewards
}

fn apply_reward(run_state: &mut RunState, reward_type: i32) {
    match reward_type {
        0 => {
            run_state.change_gold_with_source(
                GOLD_REWARD,
                DomainEventSource::Event(EventId::DeadAdventurer),
            );
        } // Gold
        2 => {
            // Relic
            let relic_id = run_state.random_screenless_relic_reward();
            let _ = run_state.obtain_relic_with_source(
                relic_id,
                EngineState::EventRoom,
                DomainEventSource::Event(EventId::DeadAdventurer),
            );
        }
        _ => {} // Nothing
    }
}

/// Initialize DeadAdventurer internal_state with shuffled rewards + starting encounter chance
pub fn init_dead_adventurer_state(run_state: &mut RunState) -> i32 {
    let base_chance = if run_state.ascension_level >= 15 {
        35
    } else {
        25
    };
    // Shuffle reward types: [Gold(0), Nothing(1), Relic(2)]
    // Java: Collections.shuffle(rewards, new Random(miscRng.randomLong()))
    let mut rewards = [0i32, 1, 2];
    crate::runtime::rng::shuffle_with_random_long(&mut rewards, &mut run_state.rng_pool.misc_rng);
    // Java constructor also chooses which elite encounter is in the corpse.
    let enemy = run_state.rng_pool.misc_rng.random_range(0, 2);
    let mut s = 0i32;
    set_num_rewards(&mut s, 0);
    set_encounter_chance(&mut s, base_chance);
    set_reward_types(&mut s, rewards[0], rewards[1], rewards[2]);
    set_enemy_type(&mut s, enemy);
    s
}

#[cfg(test)]
mod tests {
    use super::{
        combat_gold_reward, encounter_id_for_enemy, enemy_type, init_dead_adventurer_state,
        set_encounter_chance, set_enemy_type, set_num_rewards, set_reward_types,
    };
    use crate::content::monsters::factory::EncounterId;
    use crate::state::core::{CombatContext, EngineState, PostCombatReturn};
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventRandomOutcomeKind,
        EventState,
    };
    use crate::state::run::RunState;

    #[test]
    fn structured_options_expose_random_search_without_flattening_hidden_rewards() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut internal_state = 0;
        set_num_rewards(&mut internal_state, 0);
        set_encounter_chance(&mut internal_state, 25);
        set_reward_types(&mut internal_state, 0, 1, 2);
        set_enemy_type(&mut internal_state, 0);
        run_state.event_state = Some(EventState {
            id: EventId::DeadAdventurer,
            current_screen: 0,
            internal_state,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            run_state.event_state.as_ref().unwrap(),
        )
        .expect("Dead Adventurer should expose structured event options");

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Special);
        assert_eq!(
            options[0].semantics.effects,
            vec![EventEffect::RandomOutcome {
                kind: EventRandomOutcomeKind::DeadAdventurerSearch,
                chance_percent: Some(25),
            }]
        );
        assert!(options[0].semantics.repeatable);
        assert_eq!(options[0].semantics.transition, EventOptionTransition::None);
        assert_eq!(options[1].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
    }

    #[test]
    fn public_search_chance_tracks_the_screen_without_revealing_hidden_rewards() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        for chance in [25, 35, 50, 60, 75, 85, 100] {
            let mut event = EventState::new(EventId::DeadAdventurer);
            set_encounter_chance(&mut event.internal_state, chance);
            let before = run_state.rng_pool.misc_rng.counter;
            let options = super::get_options(&run_state, &event);
            assert_eq!(
                options[0].semantics.effects,
                vec![EventEffect::RandomOutcome {
                    kind: EventRandomOutcomeKind::DeadAdventurerSearch,
                    chance_percent: Some(chance as u8),
                }]
            );
            let visible = options;
            for enemy in 0..3 {
                set_enemy_type(&mut event.internal_state, enemy);
                set_reward_types(&mut event.internal_state, 2, 0, 1);
                assert_eq!(super::get_options(&run_state, &event), visible);
            }
            assert_eq!(run_state.rng_pool.misc_rng.counter, before);
        }
    }

    #[test]
    fn structured_fight_prompt_exposes_start_combat_without_hidden_reward_details() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut internal_state = 0;
        set_num_rewards(&mut internal_state, 1);
        set_encounter_chance(&mut internal_state, 50);
        set_reward_types(&mut internal_state, 0, 1, 2);
        set_enemy_type(&mut internal_state, 2);
        run_state.event_state = Some(EventState {
            id: EventId::DeadAdventurer,
            current_screen: 1,
            internal_state,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            run_state.event_state.as_ref().unwrap(),
        )
        .expect("Dead Adventurer fight prompt should expose structured event options");

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].semantics.action, EventActionKind::Fight);
        assert_eq!(options[0].semantics.effects, vec![EventEffect::StartCombat]);
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::StartCombat
        );
        assert!(options[0].semantics.terminal);
    }

    #[test]
    fn init_consumes_java_enemy_roll_and_stores_enemy_in_state() {
        let mut expected = RunState::new(1, 0, false, "Ironclad");
        let mut rewards = [0i32, 1, 2];
        crate::runtime::rng::shuffle_with_random_long(
            &mut rewards,
            &mut expected.rng_pool.misc_rng,
        );
        let expected_enemy = expected.rng_pool.misc_rng.random_range(0, 2);

        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let state = init_dead_adventurer_state(&mut run_state);

        assert_eq!(enemy_type(state), expected_enemy);
    }

    #[test]
    fn combat_trigger_first_stops_on_java_fight_prompt() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut internal_state = 0;
        set_num_rewards(&mut internal_state, 0);
        set_encounter_chance(&mut internal_state, 100);
        set_reward_types(&mut internal_state, 1, 1, 1);
        set_enemy_type(&mut internal_state, 0);
        run_state.event_state = Some(EventState {
            id: EventId::DeadAdventurer,
            current_screen: 0,
            internal_state,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        let mut engine_state = EngineState::EventRoom;

        super::handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(matches!(engine_state, EngineState::EventRoom));
        let event_state = run_state.event_state.as_ref().unwrap();
        assert_eq!(event_state.current_screen, 1);
        assert!((25..=35).contains(&combat_gold_reward(event_state.internal_state)));
    }

    #[test]
    fn daily_combat_trigger_uses_java_daily_gold_roll() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.is_daily_run = true;
        let mut expected_rng = run_state.rng_pool.misc_rng.clone();
        let _fight_roll = expected_rng.random_range(0, 99);
        let expected_gold = expected_rng.random(30);

        let mut internal_state = 0;
        set_num_rewards(&mut internal_state, 0);
        set_encounter_chance(&mut internal_state, 100);
        set_reward_types(&mut internal_state, 1, 1, 1);
        set_enemy_type(&mut internal_state, 0);
        run_state.event_state = Some(EventState {
            id: EventId::DeadAdventurer,
            current_screen: 0,
            internal_state,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        let mut engine_state = EngineState::EventRoom;

        super::handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(
            combat_gold_reward(run_state.event_state.as_ref().unwrap().internal_state),
            expected_gold,
            "Java daily Dead Adventurer uses miscRng.random(30), not random(25, 35)"
        );
    }

    #[test]
    fn fight_prompt_enters_combat_with_stored_java_enemy_key() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut internal_state = 0;
        set_num_rewards(&mut internal_state, 0);
        set_encounter_chance(&mut internal_state, 100);
        set_reward_types(&mut internal_state, 1, 1, 1);
        set_enemy_type(&mut internal_state, 0);
        run_state.event_state = Some(EventState {
            id: EventId::DeadAdventurer,
            current_screen: 0,
            internal_state,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        let mut engine_state = EngineState::EventRoom;

        super::handle_choice(&mut engine_state, &mut run_state, 0);
        super::handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("fight prompt should request CombatStart");
        };
        assert_eq!(request.encounter_id, EncounterId::ThreeSentries);
        let CombatContext::Event(combat) = request.context else {
            panic!("fight prompt should carry event combat context");
        };
        assert!(matches!(
            combat.post_combat_return,
            PostCombatReturn::MapNavigation
        ));
        assert!(combat.elite_trigger);
    }

    #[test]
    fn enemy_mapping_matches_java_get_monster_cases() {
        assert_eq!(encounter_id_for_enemy(0), EncounterId::ThreeSentries);
        assert_eq!(encounter_id_for_enemy(1), EncounterId::GremlinNob);
        assert_eq!(encounter_id_for_enemy(2), EncounterId::LagavulinEvent);
    }
}
