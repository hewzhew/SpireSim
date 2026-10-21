use crate::content::relics::RelicId;
use crate::rewards::state::{RewardItem, RewardState};
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![
            EventOption::new(
                EventChoiceMeta::new("[Read] Begin reading the tome."),
                EventOptionSemantics {
                    action: EventActionKind::Continue,
                    effects: vec![],
                    constraints: vec![],
                    transition: EventOptionTransition::AdvanceScreen,
                    repeatable: false,
                    terminal: false,
                },
            ),
            EventOption::new(
                EventChoiceMeta::new("[Leave]"),
                EventOptionSemantics {
                    action: EventActionKind::Leave,
                    effects: vec![],
                    constraints: vec![],
                    transition: EventOptionTransition::Complete,
                    repeatable: false,
                    terminal: true,
                },
            ),
        ],
        1 => vec![EventOption::new(
            EventChoiceMeta::new("[Continue] Take 1 damage."),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                effects: vec![EventEffect::LoseHp(1)],
                constraints: vec![],
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        )],
        2 => vec![EventOption::new(
            EventChoiceMeta::new("[Continue] Take 2 damage."),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                effects: vec![EventEffect::LoseHp(2)],
                constraints: vec![],
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        )],
        3 => vec![EventOption::new(
            EventChoiceMeta::new("[Continue] Take 3 damage."),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                effects: vec![EventEffect::LoseHp(3)],
                constraints: vec![],
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        )],
        4 => {
            let final_dmg = if run_state.ascension_level >= 15 {
                15
            } else {
                10
            };
            vec![
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Take the Book] Take {} damage. Obtain a Book relic.",
                        final_dmg
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Accept,
                        effects: vec![
                            EventEffect::LoseHp(final_dmg),
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomBook,
                            },
                        ],
                        constraints: vec![],
                        transition: EventOptionTransition::OpenReward,
                        repeatable: false,
                        terminal: false,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new("[Stop Reading] Take 3 damage."),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        effects: vec![EventEffect::LoseHp(3)],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ),
            ]
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
        0 => match choice_idx {
            0 => {
                event_state.current_screen = 1;
            }
            _ => {
                event_state.completed = true;
            }
        },
        1 => {
            super::apply_player_hp_loss_damage(
                run_state,
                1,
                crate::state::selection::DomainEventSource::Event(EventId::CursedTome),
            );
            event_state.current_screen = 2;
        }
        2 => {
            super::apply_player_hp_loss_damage(
                run_state,
                2,
                crate::state::selection::DomainEventSource::Event(EventId::CursedTome),
            );
            event_state.current_screen = 3;
        }
        3 => {
            super::apply_player_hp_loss_damage(
                run_state,
                3,
                crate::state::selection::DomainEventSource::Event(EventId::CursedTome),
            );
            event_state.current_screen = 4;
        }
        4 => {
            match choice_idx {
                0 => {
                    // Take the book
                    let final_dmg = if run_state.ascension_level >= 15 {
                        15
                    } else {
                        10
                    };
                    super::apply_player_hp_loss_damage(
                        run_state,
                        final_dmg,
                        crate::state::selection::DomainEventSource::Event(EventId::CursedTome),
                    );
                    // Random book relic (Java randomBook)
                    let book_relics = [
                        RelicId::Necronomicon,
                        RelicId::Enchiridion,
                        RelicId::NilrysCodex,
                    ];
                    let mut possible_books: Vec<RelicId> = book_relics
                        .iter()
                        .copied()
                        .filter(|r| !run_state.relics.iter().any(|owned| owned.id == *r))
                        .collect();
                    if possible_books.is_empty() {
                        possible_books.push(RelicId::Circlet);
                    }
                    let idx = run_state
                        .rng_pool
                        .misc_rng
                        .random(possible_books.len() as i32 - 1)
                        as usize;
                    let relic_id = possible_books[idx];
                    // Java: addRelicToRewards(r) + combatRewardScreen.open()
                    let mut rewards = RewardState::new();
                    rewards.items.push(RewardItem::Relic { relic_id });
                    event_state.current_screen = 5;
                    run_state.event_state = Some(event_state);
                    *engine_state = EngineState::RewardScreen(rewards);
                    return;
                }
                _ => {
                    // Stop reading: 3 damage
                    super::apply_player_hp_loss_damage(
                        run_state,
                        3,
                        crate::state::selection::DomainEventSource::Event(EventId::CursedTome),
                    );
                    event_state.current_screen = 5;
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
    use crate::content::relics::RelicState;
    use crate::state::events::{
        EventActionKind, EventEffect, EventOptionTransition, EventRelicKind,
    };

    fn tome_run(screen: usize, current_hp: i32, max_hp: i32) -> RunState {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.current_hp = current_hp;
        rs.max_hp = max_hp;
        rs.event_state = Some(EventState {
            id: EventId::CursedTome,
            current_screen: screen,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        rs
    }

    #[test]
    fn take_book_option_exposes_reward_transition() {
        let rs = tome_run(4, 80, 80);
        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        let take_book = &options[0];

        assert_eq!(take_book.semantics.action, EventActionKind::Accept);
        assert!(take_book
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::RandomBook,
            }));
        assert_eq!(
            take_book.semantics.transition,
            EventOptionTransition::OpenReward
        );
    }

    #[test]
    fn page_damage_uses_java_hp_loss_so_tungsten_rod_can_reduce_to_zero() {
        let mut rs = tome_run(1, 20, 80);
        rs.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.current_hp, 20);
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 2);
    }

    #[test]
    fn take_book_final_damage_uses_hp_loss_and_opens_book_reward() {
        let mut rs = tome_run(4, 30, 80);
        rs.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.current_hp, 21);
        let EngineState::RewardScreen(rewards) = engine_state else {
            panic!("taking the book should open the reward screen");
        };
        assert_eq!(rewards.items.len(), 1);
        assert!(matches!(rewards.items[0], RewardItem::Relic { .. }));
    }

    #[test]
    fn book_rewards_return_to_the_end_dialog_after_taking_or_skipping_the_relic() {
        use crate::state::core::ClientInput;
        for take in [false, true] {
            let mut run = tome_run(4, 53, 71);
            run.ascension_level = 20;
            run.relics.extend(
                [RelicId::Necronomicon, RelicId::Enchiridion]
                    .into_iter()
                    .map(RelicState::new),
            );
            let mut engine = EngineState::EventRoom;
            handle_choice(&mut engine, &mut run, 0);
            assert_eq!(run.current_hp, 38);
            let EngineState::RewardScreen(mut rewards) = engine else {
                panic!("taking the book first opens the actual reward screen");
            };
            if take {
                assert!(crate::engine::reward_handler::handle(
                    &mut run,
                    &mut rewards,
                    Some(ClientInput::ClaimReward(0))
                )
                .is_none());
            }
            let rng = run.rng_pool.clone();
            let next = crate::engine::reward_handler::handle(
                &mut run,
                &mut rewards,
                Some(ClientInput::Proceed),
            );
            assert!(matches!(next, Some(EngineState::EventRoom)));
            let event = run.event_state.as_ref().unwrap();
            assert_eq!(event.current_screen, 5);
            assert!(!event.completed);
            assert_eq!(
                get_options(&run, event)[0].semantics.action,
                EventActionKind::Leave
            );
            let mut engine = next.unwrap();
            handle_choice(&mut engine, &mut run, 0);
            assert!(run.event_state.as_ref().unwrap().completed);
            assert_eq!(run.current_hp, 38);
            assert_eq!(run.rng_pool, rng);
            assert_eq!(
                run.relics
                    .iter()
                    .any(|relic| relic.id == RelicId::NilrysCodex),
                take
            );
        }
    }

    #[test]
    fn random_book_uses_native_single_bound_call_for_each_remaining_pool_size() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
        let books = [
            RelicId::Necronomicon,
            RelicId::Enchiridion,
            RelicId::NilrysCodex,
        ];
        for owned in 0..=3 {
            let mut rs = tome_run(4, 80, 80);
            rs.relics
                .extend(books[..owned].iter().copied().map(RelicState::new));
            let before_counter = rs.rng_pool.misc_rng.counter;
            let mut engine_state = EngineState::EventRoom;
            let trace = begin_sts_rng_trace().unwrap();
            handle_choice(&mut engine_state, &mut rs, 0);
            let calls = trace.finish().unwrap();
            assert_eq!(rs.rng_pool.misc_rng.counter, before_counter + 1);
            assert_eq!(calls.len(), 1);
            let StsRngCall::RandomInt { range, result } = calls[0].call else {
                panic!("Java randomBook calls random(size - 1), including a one-item pool");
            };
            assert_eq!(range, (3 - owned).max(1) as i32 - 1);
            let EngineState::RewardScreen(rewards) = engine_state else {
                panic!("taking the book should open the reward screen");
            };
            let expected = if owned == 3 {
                RelicId::Circlet
            } else {
                books[owned + result as usize]
            };
            assert_eq!(
                rewards.items,
                vec![RewardItem::Relic { relic_id: expected }]
            );
        }
    }
}
