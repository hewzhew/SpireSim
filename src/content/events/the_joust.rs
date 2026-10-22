// Java: TheJoust (city) — "The Joust"
// Screen 0: [Approach] → explanation
// Screen 1: [Bet Against (Murderer)] pay 50g, win 100g if murderer wins (70%)
//           [Bet For (Owner)] pay 50g, win 250g if owner wins (30%)
// Screen 2: [Continue] roll fight result
// Screen 3: [Continue] reveal result and apply gold payout
// Screen 4: Result screen
// Screen 5: [Leave]
//
// Java uses miscRng.randomBoolean(0.3f) — owner wins 30%, murderer wins 70%

use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRandomOutcomeKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

const BET_AMT: i32 = 50;
const WIN_MURDERER: i32 = 100;
const WIN_OWNER: i32 = 250;
const OWNER_WIN_CHANCE_PERCENT: u8 = 30;

fn outcome_odds() -> EventEffect {
    EventEffect::RandomOutcome {
        kind: EventRandomOutcomeKind::JoustOwnerWin,
        chance_percent: Some(OWNER_WIN_CHANCE_PERCENT),
    }
}

fn bet_for_owner(event_state: &EventState) -> bool {
    (event_state.internal_state & 1) != 0
}

pub fn get_options(_run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![EventOption::new(
            EventChoiceMeta::new("[Approach] Investigate the commotion."),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        )],
        1 => vec![
            EventOption::new(
                EventChoiceMeta::new(
                    "[Bet Against Owner] Pay 50 Gold. Win 100 Gold if Murderer wins.",
                ),
                EventOptionSemantics {
                    action: EventActionKind::Trade,
                    effects: vec![
                        EventEffect::LoseGold(BET_AMT),
                        EventEffect::GainGoldRange {
                            min: 0,
                            max: WIN_MURDERER,
                        },
                        outcome_odds(),
                    ],
                    transition: EventOptionTransition::AdvanceScreen,
                    ..Default::default()
                },
            ),
            EventOption::new(
                EventChoiceMeta::new("[Bet For Owner] Pay 50 Gold. Win 250 Gold if Owner wins."),
                EventOptionSemantics {
                    action: EventActionKind::Trade,
                    effects: vec![
                        EventEffect::LoseGold(BET_AMT),
                        EventEffect::GainGoldRange {
                            min: 0,
                            max: WIN_OWNER,
                        },
                        outcome_odds(),
                    ],
                    transition: EventOptionTransition::AdvanceScreen,
                    ..Default::default()
                },
            ),
        ],
        2 => vec![EventOption::new(
            EventChoiceMeta::new("[Continue] Watch the joust!"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                effects: vec![outcome_odds()],
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        )],
        3 => {
            let max_payout = if bet_for_owner(event_state) {
                WIN_OWNER
            } else {
                WIN_MURDERER
            };
            vec![EventOption::new(
                EventChoiceMeta::new("[Continue] Resolve the joust."),
                EventOptionSemantics {
                    action: EventActionKind::Continue,
                    effects: vec![EventEffect::GainGoldRange {
                        min: 0,
                        max: max_payout,
                    }],
                    transition: EventOptionTransition::AdvanceScreen,
                    ..Default::default()
                },
            )]
        }
        4 => {
            // internal_state encodes: bit0 = betFor, bit1 = ownerWins
            let bet_for = (event_state.internal_state & 1) != 0;
            let owner_wins = (event_state.internal_state & 2) != 0;
            let msg = if owner_wins {
                if bet_for {
                    "Owner wins! You won 250 Gold!"
                } else {
                    "Owner wins! You lost your bet."
                }
            } else if bet_for {
                "Murderer wins! You lost your bet."
            } else {
                "Murderer wins! You won 100 Gold!"
            };
            vec![EventOption::new(
                EventChoiceMeta::new(format!("{} [Leave]", msg)).with_public_detail(
                    crate::state::events::EventOptionPublicDetail::JoustResult {
                        owner_wins,
                        gold_received: if owner_wins && bet_for {
                            WIN_OWNER
                        } else if !owner_wins && !bet_for {
                            WIN_MURDERER
                        } else {
                            0
                        },
                    },
                ),
                EventOptionSemantics {
                    action: EventActionKind::Leave,
                    transition: EventOptionTransition::Complete,
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

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            // Approach → explanation
            event_state.current_screen = 1;
        }
        1 => {
            // Place bet
            let bet_for = choice_idx == 1;
            run_state
                .change_gold_with_source(-BET_AMT, DomainEventSource::Event(EventId::TheJoust));
            event_state.internal_state = if bet_for { 1 } else { 0 };
            event_state.current_screen = 2;
        }
        2 => {
            // Java PRE_JOUST click: roll result and enter animation/result-pending screen.
            // Java: miscRng.randomBoolean(0.3f) — owner wins 30%
            let owner_wins = run_state
                .rng_pool
                .misc_rng
                .random_boolean_chance(f32::from(OWNER_WIN_CHANCE_PERCENT) / 100.0);
            let bet_for = (event_state.internal_state & 1) != 0;
            event_state.internal_state =
                (if bet_for { 1 } else { 0 }) | (if owner_wins { 2 } else { 0 });
            event_state.current_screen = 3;
        }
        3 => {
            // Java JOUST click: reveal result and apply payout.
            let owner_wins = (event_state.internal_state & 2) != 0;
            let bet_for = (event_state.internal_state & 1) != 0;

            // Calculate gold payout
            if owner_wins && bet_for {
                run_state.change_gold_with_source(
                    WIN_OWNER,
                    DomainEventSource::Event(EventId::TheJoust),
                );
            } else if !owner_wins && !bet_for {
                run_state.change_gold_with_source(
                    WIN_MURDERER,
                    DomainEventSource::Event(EventId::TheJoust),
                );
            }
            // else: bet lost, gold already deducted

            event_state.current_screen = 4;
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
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn joust_run(screen: usize, internal_state: i32, gold: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = gold;
        let mut event_state = EventState::new(EventId::TheJoust);
        event_state.current_screen = screen;
        event_state.internal_state = internal_state;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn structured_options_expose_bet_cost_and_possible_payout_ranges() {
        let run_state = joust_run(1, 0, 50);
        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Trade);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::LoseGold(50)));
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::GainGoldRange { min: 0, max: 100 }));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        assert_eq!(options[1].semantics.action, EventActionKind::Trade);
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::LoseGold(50)));
        assert!(options[1]
            .semantics
            .effects
            .contains(&EventEffect::GainGoldRange { min: 0, max: 250 }));
        for option in options {
            assert!(option.semantics.effects.contains(&super::outcome_odds()));
        }
    }

    #[test]
    fn structured_roll_and_result_screens_preserve_java_reveal_boundary() {
        let run_state = joust_run(2, 0, 50);
        let roll_options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert_eq!(roll_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            roll_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let run_state = joust_run(3, 1, 50);
        let reveal_options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert_eq!(
            reveal_options[0].semantics.action,
            EventActionKind::Continue
        );
        assert!(reveal_options[0]
            .semantics
            .effects
            .contains(&EventEffect::GainGoldRange { min: 0, max: 250 }));

        let run_state = joust_run(4, 0, 50);
        let leave_options = get_options(&run_state, run_state.event_state.as_ref().unwrap());
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        assert!(leave_options[0].semantics.terminal);
    }

    #[test]
    fn pre_joust_continue_rolls_result_without_payout_like_java() {
        let mut run_state = joust_run(2, 0, 50);
        let start_counter = run_state.rng_pool.misc_rng.counter;
        let mut engine_state = EngineState::EventRoom;
        let mut expected_rng = run_state.rng_pool.misc_rng.clone();
        let expected_owner_wins = expected_rng.random_boolean_chance(0.3);

        handle_choice(&mut engine_state, &mut run_state, 0);

        let event_state = run_state.event_state.as_ref().unwrap();
        assert_eq!(event_state.current_screen, 3);
        assert_eq!((event_state.internal_state & 2) != 0, expected_owner_wins);
        assert_eq!(run_state.rng_pool.misc_rng.counter, start_counter + 1);
        assert_eq!(run_state.gold, 50);
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn result_continue_pays_murderer_bet_after_roll_screen() {
        let mut run_state = joust_run(3, 0, 50);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 4);
        assert_eq!(run_state.gold, 150);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: 100,
                new_total: 150,
                source: DomainEventSource::Event(EventId::TheJoust)
            }
        )));
    }

    #[test]
    fn result_continue_pays_owner_bet_after_roll_screen() {
        let mut run_state = joust_run(3, 0b11, 50);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 4);
        assert_eq!(run_state.gold, 300);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::GoldChanged {
                delta: 250,
                new_total: 300,
                source: DomainEventSource::Event(EventId::TheJoust)
            }
        )));
    }
}
