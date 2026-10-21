use crate::rewards::state::{RewardItem, RewardState};
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventOption, EventOptionSemantics,
    EventOptionTransition, EventState,
};
use crate::state::run::RunState;

fn potion_reward_count(run_state: &RunState) -> usize {
    if run_state.ascension_level >= 15 {
        2
    } else {
        3
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    if event_state.current_screen == 1 {
        return vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..Default::default()
            },
        )];
    }

    let potion_count = potion_reward_count(run_state);
    vec![EventOption::new(
        EventChoiceMeta::new(format!("[Take] Obtain {} random Potions.", potion_count)),
        EventOptionSemantics {
            action: EventActionKind::Gain,
            effects: vec![EventEffect::ObtainPotion {
                count: potion_count,
            }],
            transition: EventOptionTransition::OpenReward,
            ..Default::default()
        },
    )]
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, _choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            // Take potions
            let potion_count = potion_reward_count(run_state);
            // Java adds potion RewardItems and opens the combat reward screen.
            let mut rewards = RewardState::new();
            for _ in 0..potion_count {
                let pid = run_state.random_potion_flat();
                rewards.items.push(RewardItem::Potion { potion_id: pid });
            }
            event_state.current_screen = 1;
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

#[cfg(test)]
mod tests {
    use super::{get_options, handle_choice};
    use crate::rewards::state::RewardItem;
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventState,
    };
    use crate::state::run::RunState;

    fn lab_run(ascension_level: u8) -> RunState {
        let mut run_state = RunState::new(1, ascension_level, false, "Ironclad");
        run_state.event_state = Some(EventState::new(EventId::Lab));
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn structured_options_expose_potion_reward_count_and_reward_screen_boundary() {
        let run_state = lab_run(0);
        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].semantics.action, EventActionKind::Gain);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainPotion { count: 3 }));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::OpenReward
        );

        let run_state_a15 = lab_run(15);
        let options_a15 = get_options(&run_state_a15, run_state_a15.event_state.as_ref().unwrap());
        assert!(options_a15[0]
            .semantics
            .effects
            .contains(&EventEffect::ObtainPotion { count: 2 }));

        let mut complete = EventState::new(EventId::Lab);
        complete.current_screen = 1;
        let complete_options = get_options(&run_state, &complete);
        assert_eq!(complete_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            complete_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        assert!(complete_options[0].semantics.terminal);
    }

    #[test]
    fn lab_opens_three_potion_rewards_without_directly_filling_inventory() {
        let mut run_state = lab_run(0);
        let starting_potions = run_state.potions.clone();
        let potion_rng_before = run_state.rng_pool.potion_rng.counter;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.potions, starting_potions);
        assert_eq!(
            run_state.rng_pool.potion_rng.counter,
            potion_rng_before + 3,
            "Java Lab uses PotionHelper.getRandomPotion(), one flat potionRng index per reward"
        );
        assert!(run_state.take_emitted_events().is_empty());
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
    fn lab_ascension_fifteen_opens_two_potion_rewards() {
        let mut run_state = lab_run(15);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        match engine_state {
            EngineState::RewardScreen(rewards) => {
                assert_eq!(rewards.items.len(), 2);
                assert!(rewards
                    .items
                    .iter()
                    .all(|item| matches!(item, RewardItem::Potion { .. })));
            }
            other => panic!("expected reward screen, got {other:?}"),
        }
    }
}
