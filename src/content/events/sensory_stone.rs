use crate::rewards::state::{RewardItem, RewardState};
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn focus_effects(focus_count: usize, hp_loss: Option<i32>) -> Vec<EventEffect> {
    let mut effects = Vec::new();
    if let Some(amount) = hp_loss {
        effects.push(EventEffect::LoseHp(amount));
    }
    effects.push(EventEffect::OfferCards {
        count: focus_count * 3,
        kind: EventCardKind::RandomColorless,
    });
    effects
}

pub fn get_options(_run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![EventOption::new(
            EventChoiceMeta::new("[Recall]"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        )],
        1 => vec![
            EventOption::new(
                EventChoiceMeta::new("[Focus 1] Obtain 1 colorless card."),
                EventOptionSemantics {
                    action: EventActionKind::Gain,
                    effects: focus_effects(1, None),
                    transition: EventOptionTransition::OpenReward,
                    ..Default::default()
                },
            ),
            EventOption::new(
                EventChoiceMeta::new("[Focus 2] Obtain 2 colorless cards. Take 5 damage."),
                EventOptionSemantics {
                    action: EventActionKind::Trade,
                    effects: focus_effects(2, Some(5)),
                    transition: EventOptionTransition::OpenReward,
                    ..Default::default()
                },
            ),
            EventOption::new(
                EventChoiceMeta::new("[Focus 3] Obtain 3 colorless cards. Take 10 damage."),
                EventOptionSemantics {
                    action: EventActionKind::Trade,
                    effects: focus_effects(3, Some(10)),
                    transition: EventOptionTransition::OpenReward,
                    ..Default::default()
                },
            ),
        ],
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
            // Java first click only advances INTRO -> INTRO_2 and reveals the
            // three focus choices. No RNG or reward is consumed on this click.
            event_state.current_screen = 1;
        }
        1 => {
            let count = (choice_idx + 1).min(3);

            // Java: Collections.shuffle(memories, new Random(miscRng.randomLong()))
            // Consume randomLong for seed parity
            let _seed = run_state.rng_pool.misc_rng.random_long();

            // HP_LOSS damage for Focus 2/3. Java bypasses block, but
            // AbstractPlayer.damage still applies relic onLoseHpLast.
            match choice_idx {
                1 => {
                    super::apply_player_hp_loss_damage(
                        run_state,
                        5,
                        DomainEventSource::Event(EventId::SensoryStone),
                    );
                }
                2 => {
                    super::apply_player_hp_loss_damage(
                        run_state,
                        10,
                        DomainEventSource::Event(EventId::SensoryStone),
                    );
                }
                _ => {}
            }

            // Java: addCardReward(RewardItem(COLORLESS)) × count
            // Each card reward row offers 3 colorless cards (pick 1, skippable)
            let mut rewards = RewardState::new();
            for _ in 0..count {
                let cards = generate_colorless_card_row(run_state);
                rewards.items.push(RewardItem::Card { cards });
            }

            // reward() advances Java's transient ACCEPT (2) to LEAVE (3)
            // before the reward screen becomes a stable decision boundary.
            event_state.current_screen = 3;
            run_state.event_state = Some(event_state);
            *engine_state = EngineState::RewardScreen(rewards);
            return;
        }
        _ => {
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

/// Generate a colorless reward row with the ordinary relic choice-count rules.
/// Mirrors Java: `RewardItem(CardColor.COLORLESS)` delegates to
/// `AbstractDungeon.getColorlessRewardCards()`, which rolls rare/uncommon with
/// `colorlessRareChance`, chooses from the sorted colorless rarity pool via
/// `cardRng`, avoids duplicates within the row, then runs relic preview hooks.
fn generate_colorless_card_row(run_state: &mut RunState) -> Vec<crate::rewards::state::RewardCard> {
    use crate::content::cards::*;
    let num_cards = crate::rewards::generator::adjusted_card_reward_choice_count(run_state, 3);
    let mut cards = Vec::with_capacity(num_cards);
    for _ in 0..num_cards {
        let rarity = if run_state.rng_pool.card_rng.random_boolean_chance(0.3) {
            CardRarity::Rare
        } else {
            CardRarity::Uncommon
        };
        if rarity == CardRarity::Rare {
            // Java resets cardBlizzRandomizer when a colorless rare is rolled.
            run_state.card_blizz_randomizer = 5;
        }

        let mut card_id = run_state.random_colorless_card_from_pool(rarity);
        while cards
            .iter()
            .any(|card: &crate::rewards::state::RewardCard| card.id == card_id)
        {
            card_id = run_state.random_colorless_card_from_pool(rarity);
        }
        cards.push(crate::rewards::state::RewardCard::new(
            card_id,
            run_state.preview_obtain_card_upgrades(card_id, 0),
        ));
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::{get_choices, get_options, handle_choice};
    use crate::content::relics::{RelicId, RelicState};
    use crate::rewards::state::RewardItem;
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventId, EventOptionTransition, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn sensory_run(current_hp: i32, max_hp: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = current_hp;
        run_state.max_hp = max_hp;
        run_state.event_state = Some(EventState::new(EventId::SensoryStone));
        run_state.emitted_events.clear();
        run_state
    }

    fn sensory_focus_run(current_hp: i32, max_hp: i32) -> RunState {
        let mut run_state = sensory_run(current_hp, max_hp);
        run_state.event_state.as_mut().unwrap().current_screen = 1;
        run_state
    }

    #[test]
    fn structured_options_expose_intro_and_focus_reward_boundaries() {
        let run_state = sensory_run(20, 80);
        let intro_options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert_eq!(intro_options.len(), 1);
        assert_eq!(intro_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            intro_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let focus_run = sensory_focus_run(20, 80);
        let focus_options = get_options(&focus_run, focus_run.event_state.as_ref().unwrap());
        assert_eq!(focus_options.len(), 3);
        assert_eq!(focus_options[0].semantics.action, EventActionKind::Gain);
        assert!(focus_options[0]
            .semantics
            .effects
            .contains(&EventEffect::OfferCards {
                count: 3,
                kind: EventCardKind::RandomColorless,
            }));
        assert_eq!(
            focus_options[0].semantics.transition,
            EventOptionTransition::OpenReward
        );

        assert!(focus_options[1]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(5)));
        assert!(focus_options[1]
            .semantics
            .effects
            .contains(&EventEffect::OfferCards {
                count: 6,
                kind: EventCardKind::RandomColorless,
            }));

        assert!(focus_options[2]
            .semantics
            .effects
            .contains(&EventEffect::LoseHp(10)));
        assert!(focus_options[2]
            .semantics
            .effects
            .contains(&EventEffect::OfferCards {
                count: 9,
                kind: EventCardKind::RandomColorless,
            }));

        let mut complete = EventState::new(EventId::SensoryStone);
        complete.current_screen = 3;
        let complete_options = get_options(&focus_run, &complete);
        assert_eq!(complete_options[0].semantics.action, EventActionKind::Leave);
        assert!(complete_options[0].semantics.terminal);
    }

    #[test]
    fn first_click_only_reveals_focus_choices_like_java_intro_screen() {
        let mut run_state = sensory_run(20, 80);
        let misc_before = run_state.rng_pool.misc_rng.counter;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(
            run_state.event_state.as_ref().unwrap().current_screen,
            1,
            "Java SensoryStone first click advances INTRO to INTRO_2 before any Focus choice"
        );
        assert_eq!(
            run_state.rng_pool.misc_rng.counter, misc_before,
            "Java does not shuffle the memory text until the second-screen Focus choice"
        );
    }

    #[test]
    fn focus_choices_are_available_on_second_screen_even_at_high_ascension() {
        let mut run_state = sensory_focus_run(20, 80);
        run_state.ascension_level = 20;
        let event_state = run_state.event_state.as_ref().unwrap();

        assert_eq!(
            get_choices(&run_state, event_state).len(),
            3,
            "Java SensoryStone always shows Focus 1/2/3 on INTRO_2; there is no Ascension gate"
        );
    }

    #[test]
    fn colorless_reward_row_uses_reward_card_count_relics() {
        let mut run_state = sensory_focus_run(20, 80);
        run_state
            .relics
            .push(RelicState::new(RelicId::QuestionCard));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        match engine_state {
            EngineState::RewardScreen(rewards) => {
                let RewardItem::Card { cards } = &rewards.items[0] else {
                    panic!("expected colorless card reward row");
                };
                assert_eq!(
                    cards.len(),
                    4,
                    "Java RewardItem(CardColor.COLORLESS) still runs changeNumberOfCardsInReward"
                );
            }
            other => panic!("expected reward screen, got {other:?}"),
        }
    }

    #[test]
    fn card_reward_relics_change_sensory_options_without_adding_event_rows() {
        let mut run = sensory_focus_run(20, 80);
        for relic in [
            RelicId::BustedCrown,
            RelicId::QuestionCard,
            RelicId::PrayerWheel,
        ] {
            run.relics.push(RelicState::new(relic));
        }
        let mut engine = EngineState::EventRoom;
        handle_choice(&mut engine, &mut run, 2);
        let EngineState::RewardScreen(rewards) = engine else {
            panic!("expected reward screen")
        };
        assert_eq!(
            rewards.items.len(),
            3,
            "event chooses the number of reward rows"
        );
        for reward in rewards.items {
            let RewardItem::Card { cards } = reward else {
                panic!("expected card row")
            };
            assert_eq!(cards.len(), 2);
        }
    }

    #[test]
    fn focus_two_hp_loss_uses_event_source_and_opens_two_rewards() {
        let mut run_state = sensory_focus_run(20, 80);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 15);
        match engine_state {
            EngineState::RewardScreen(rewards) => {
                assert_eq!(rewards.items.len(), 2);
                assert!(rewards
                    .items
                    .iter()
                    .all(|item| matches!(item, RewardItem::Card { .. })));
            }
            other => panic!("expected reward screen, got {other:?}"),
        }
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -5,
                current_hp: 15,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::SensoryStone),
            }
        )));
    }

    #[test]
    fn focus_three_hp_loss_applies_tungsten_rod_on_lose_hp_last() {
        let mut run_state = sensory_focus_run(20, 80);
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 2);

        assert_eq!(run_state.current_hp, 11);
        match engine_state {
            EngineState::RewardScreen(rewards) => {
                assert_eq!(rewards.items.len(), 3);
            }
            other => panic!("expected reward screen, got {other:?}"),
        }
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -9,
                current_hp: 11,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::SensoryStone),
            }
        )));
    }
}
