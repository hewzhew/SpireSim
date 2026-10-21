use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use sts_core::content::cards::CardId;
use sts_core::content::monsters::EnemyId;
use sts_core::content::potions::{Potion, PotionId};
use sts_core::content::powers::PowerId;
use sts_core::runtime::combat::{CombatCard, Power, PowerPayload};
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use sts_core::state::core::{ClientInput, EngineState, HandSelectReason, PendingChoice};

use super::*;

mod act1_inputs;
mod batching;
mod direct;
mod entities;
mod learning_scope;
mod local;
mod local_tactics;
mod rollout;

#[test]
fn strong_guidance_can_continue_before_every_sibling_is_evaluated() {
    let graph = Graph::new(
        vec![vec![1, 2, 3], vec![4], vec![], vec![], vec![5], vec![]],
        vec![None, None, Some(false), Some(true), None, Some(true)],
    );
    let mut session =
        SearchSession::new(graph.clone(), 0, SearchConfig::default(), budget()).unwrap();
    session
        .submit_guidance(
            session.root(),
            Guidance {
                weights: Some(vec![1.0, 0.0, 0.0]),
                value: None,
            },
        )
        .unwrap();
    for _ in 0..3 {
        match session.step().unwrap() {
            StepStatus::NeedsGuidance(source) => {
                session
                    .submit_guidance(source, Guidance::default())
                    .unwrap();
            }
            StepStatus::Progress => {
                let sources = session.pending_guidance();
                for source in sources {
                    session
                        .submit_guidance(source, Guidance::default())
                        .unwrap();
                }
            }
            StepStatus::Terminal { won: true, .. } => break,
            other => panic!("unexpected step: {other:?}"),
        }
    }
    assert_eq!(*graph.calls.borrow(), [(0, 1), (1, 4), (4, 5)]);
    assert_eq!(session.observed_wins().len(), 1);
    assert_eq!(finish_graph(&mut session, None), StopReason::NoOpenWork);
    assert_eq!(session.observed_wins().len(), 2);
    assert!(graph.calls.borrow().contains(&(0, 2)));
    assert!(graph.calls.borrow().contains(&(0, 3)));
}

fn budget() -> SearchBudget {
    SearchBudget {
        max_services: 2_000,
        max_engine_steps: 100_000,
        max_nodes: 2_000,
        max_depth: 64,
        deadline: None,
        guidance: GuidanceBudget::default(),
    }
}

#[derive(Clone)]
pub(crate) struct Graph {
    next: Vec<Vec<usize>>,
    terminal: Vec<Option<bool>>,
    calls: Rc<RefCell<Vec<(usize, usize)>>>,
    interrupted: Option<TransitionInterruption>,
}

impl Graph {
    pub(crate) fn new(next: Vec<Vec<usize>>, terminal: Vec<Option<bool>>) -> Self {
        Self {
            next,
            terminal,
            calls: Default::default(),
            interrupted: None,
        }
    }
}

impl SearchSpace for Graph {
    type State = usize;
    type Action = usize;
    fn terminal(&self, state: &usize) -> Option<bool> {
        self.terminal[*state]
    }
    fn choices(&self, state: &usize) -> Result<Vec<Choice<usize>>, String> {
        Ok(self.next[*state]
            .iter()
            .map(|target| Choice {
                action: *target,
                exploration_weight: 1.0,
            })
            .collect())
    }
    fn transition(
        &self,
        state: &usize,
        action: &usize,
        _limits: CombatStepLimits,
    ) -> Result<Transition<usize>, String> {
        self.calls.borrow_mut().push((*state, *action));
        if let Some(cause) = self.interrupted {
            return Ok(Transition::Interrupted {
                engine_steps: 2,
                cause,
            });
        }
        Ok(Transition::Settled {
            state: *action,
            engine_steps: 1,
        })
    }
}

fn finish_graph(session: &mut SearchSession<Graph>, value: Option<f64>) -> StopReason {
    loop {
        match session.step().unwrap() {
            StepStatus::NeedsGuidance(source) => session
                .submit_guidance(
                    source,
                    Guidance {
                        value,
                        ..Default::default()
                    },
                )
                .unwrap(),
            StepStatus::Stopped(reason) => return reason,
            _ => (),
        }
    }
}

#[test]
fn zero_prior_does_not_hide_a_delayed_win_after_a_losing_suffix() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![], vec![3], vec![4], vec![]],
        vec![None, Some(false), None, None, Some(true)],
    );
    let mut session = SearchSession::new(graph, 0, SearchConfig::default(), budget()).unwrap();
    session
        .submit_guidance(
            session.root(),
            Guidance {
                weights: Some(vec![1.0, 0.0]),
                value: None,
            },
        )
        .unwrap();
    assert!(matches!(
        session.step().unwrap(),
        StepStatus::Terminal { won: false, .. }
    ));
    assert!(session.observed_wins().is_empty());
    assert_eq!(finish_graph(&mut session, None), StopReason::NoOpenWork);
    let wins = session.observed_wins();
    assert_eq!(wins.len(), 1);
    assert_eq!(
        session
            .path(wins[0])
            .unwrap()
            .iter()
            .map(|(_, a, _)| **a)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(session.stats().terminal_observations, 2);
}

#[test]
fn optimistic_leaf_estimates_are_not_terminal_observations() {
    let graph = Graph::new(
        vec![vec![1], vec![2], vec![]],
        vec![None, None, Some(false)],
    );
    let mut session = SearchSession::new(graph, 0, SearchConfig::default(), budget()).unwrap();
    session
        .submit_guidance(
            session.root(),
            Guidance {
                value: Some(1.0),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(session.observed_wins().is_empty());
    assert_eq!(session.stats().terminal_observations, 0);
    finish_graph(&mut session, Some(1.0));
    assert!(session.observed_wins().is_empty());
    assert_eq!(session.stats().terminal_observations, 1);
    assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 3);
}

#[test]
fn unanswered_and_rejected_guidance_preserve_work_and_source() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![], vec![]],
        vec![None, Some(false), Some(true)],
    );
    let mut a = SearchSession::new(graph.clone(), 0, SearchConfig::default(), budget()).unwrap();
    let b = SearchSession::new(graph.clone(), 0, SearchConfig::default(), budget()).unwrap();
    let request = a.step().unwrap();
    assert_eq!(a.step().unwrap(), request);
    assert_eq!(
        a.submit_guidance(b.root(), Guidance::default()),
        Err(SearchError::InvalidSource)
    );
    for guidance in [
        Guidance {
            weights: Some(vec![1.0]),
            value: None,
        },
        Guidance {
            weights: Some(vec![1.0, -1.0]),
            value: None,
        },
        Guidance {
            weights: Some(vec![0.0, 0.0]),
            value: None,
        },
        Guidance {
            weights: None,
            value: Some(f64::NAN),
        },
    ] {
        assert_eq!(
            a.submit_guidance(a.root(), guidance),
            Err(SearchError::InvalidGuidance)
        );
        assert_eq!(a.step().unwrap(), request);
    }
    assert_eq!(a.stats(), SearchStats::default());
    assert!(graph.calls.borrow().is_empty());
    a.submit_guidance(
        a.root(),
        Guidance {
            weights: Some(vec![f64::MAX, f64::MAX]),
            value: None,
        },
    )
    .unwrap();
    assert_eq!(
        a.submit_guidance(a.root(), Guidance::default()),
        Err(SearchError::AlreadyGuided)
    );
    finish_graph(&mut a, None);
    assert_eq!(a.observed_wins().len(), 1);
}

#[test]
fn leaf_values_guide_work_but_actual_failure_does_not_erase_the_other_branch() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![3], vec![4], vec![], vec![]],
        vec![None, None, None, Some(true), Some(false)],
    );
    let mut session =
        SearchSession::new(graph.clone(), 0, SearchConfig::default(), budget()).unwrap();
    session
        .submit_guidance(session.root(), Guidance::default())
        .unwrap();
    for value in [0.0, 1.0] {
        let StepStatus::NeedsGuidance(source) = session.step().unwrap() else {
            panic!("expected a new leaf")
        };
        session
            .submit_guidance(
                source,
                Guidance {
                    weights: None,
                    value: Some(value),
                },
            )
            .unwrap();
    }
    // Both root actions have received a service. The optimistic leaf is next,
    // although its index is later. Its failed suffix is an observation, not a
    // reason to accept the model's pessimistic verdict on the other action.
    assert!(matches!(
        session.step().unwrap(),
        StepStatus::Terminal { won: false, .. }
    ));
    assert_eq!(graph.calls.borrow().last(), Some(&(2, 4)));
    assert!(matches!(
        session.step().unwrap(),
        StepStatus::Terminal { won: true, .. }
    ));
    assert_eq!(graph.calls.borrow().last(), Some(&(1, 3)));
    assert_eq!(session.observed_wins().len(), 1);
}

#[test]
fn expired_deadline_admits_no_engine_work() {
    let graph = Graph::new(vec![vec![1], vec![]], vec![None, Some(true)]);
    let mut session = SearchSession::new(
        graph.clone(),
        0,
        SearchConfig::default(),
        SearchBudget {
            deadline: Some(Instant::now() - Duration::from_secs(1)),
            ..budget()
        },
    )
    .unwrap();
    assert_eq!(
        session.step().unwrap(),
        StepStatus::Stopped(StopReason::Deadline)
    );
    assert!(graph.calls.borrow().is_empty());
}

#[test]
fn an_interrupted_transition_never_publishes_a_partial_node() {
    let mut graph = Graph::new(vec![vec![1], vec![]], vec![None, Some(true)]);
    graph.interrupted = Some(TransitionInterruption::Deadline);
    let mut session = SearchSession::new(graph, 0, SearchConfig::default(), budget()).unwrap();
    session
        .submit_guidance(session.root(), Guidance::default())
        .unwrap();
    assert_eq!(
        session.step().unwrap(),
        StepStatus::Stopped(StopReason::Deadline)
    );
    assert_eq!(session.node_count(), 1);
    assert_eq!(session.stats().engine_steps, 2);
    assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 0);
    assert!(session.observed_wins().is_empty());
}

#[test]
fn transition_step_limit_is_a_gap_without_a_losing_sample() {
    let mut graph = Graph::new(vec![vec![1], vec![]], vec![None, Some(true)]);
    graph.interrupted = Some(TransitionInterruption::EngineStepLimit);
    let mut session = SearchSession::new(graph, 0, SearchConfig::default(), budget()).unwrap();
    finish_graph(&mut session, None);
    assert_eq!(session.stats().coverage_gaps, 1);
    assert_eq!(session.stats().engine_steps, 2);
    assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 0);
    assert!(session.observed_wins().is_empty());
}

#[test]
fn exact_cycle_does_not_manufacture_a_loss() {
    let graph = Graph::new(vec![vec![0]], vec![None]);
    let mut session = SearchSession::new(graph, 0, SearchConfig::default(), budget()).unwrap();
    finish_graph(&mut session, None);
    assert_eq!(session.stats().coverage_gaps, 1);
    assert_eq!(session.stats().terminal_observations, 0);
    assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 0);
}

#[test]
fn retained_prefix_advances_without_reexecuting_ancestors() {
    let graph = Graph::new(
        vec![vec![1], vec![2], vec![3], vec![]],
        vec![None, None, None, Some(true)],
    );
    let mut session =
        SearchSession::new(graph.clone(), 0, SearchConfig::default(), budget()).unwrap();
    finish_graph(&mut session, None);
    assert_eq!(*graph.calls.borrow(), [(0, 1), (1, 2), (2, 3)]);
    let before = session.stats();
    for _ in 0..3 {
        assert_eq!(
            session.step().unwrap(),
            StepStatus::Stopped(StopReason::NoOpenWork)
        );
    }
    assert_eq!(session.stats(), before);
}

#[test]
fn service_node_depth_and_engine_limits_remain_distinct() {
    let graph = Graph::new(vec![vec![1], vec![2], vec![]], vec![None, None, Some(true)]);
    for (limits, stop) in [
        (
            SearchBudget {
                max_services: 0,
                ..budget()
            },
            StopReason::ServiceBudget,
        ),
        (
            SearchBudget {
                max_nodes: 1,
                ..budget()
            },
            StopReason::NodeBudget,
        ),
        (
            SearchBudget {
                max_engine_steps: 1,
                ..budget()
            },
            StopReason::EngineBudget,
        ),
        (
            SearchBudget {
                max_depth: 0,
                ..budget()
            },
            StopReason::NoOpenWork,
        ),
    ] {
        let mut session =
            SearchSession::new(graph.clone(), 0, SearchConfig::default(), limits).unwrap();
        assert_eq!(finish_graph(&mut session, None), stop);
        assert!(session.observed_wins().is_empty());
        assert_eq!(session.stats().terminal_observations, 0);
        assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 0);
    }
}

fn combat_position(hand: &[CardId], draw: &[CardId], hp: i32) -> CombatPosition {
    let mut combat = sts_core::test_support::blank_test_combat();
    let mut monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    monster.current_hp = hp;
    monster.max_hp = hp;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = hand
        .iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 1))
        .collect();
    combat.zones.draw_pile = draw
        .iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 101))
        .collect::<Vec<_>>()
        .into();
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

fn settled(
    space: &CombatSearchSpace,
    state: &CombatSearchState,
    action: &CombatBranch,
) -> CombatSearchState {
    let Transition::Settled { state, .. } = space
        .transition(
            state,
            action,
            CombatStepLimits {
                max_engine_steps: 256,
                deadline: None,
            },
        )
        .unwrap()
    else {
        panic!("transition interrupted")
    };
    state
}

#[test]
fn ordered_selection_factorization_preserves_uniform_completed_input_mass() {
    let mut position = combat_position(&[CardId::Strike, CardId::Strike, CardId::Defend], &[], 40);
    position.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3]),
        min_cards: 0,
        max_cards: 3,
        can_cancel: false,
        reason: HandSelectReason::Discard,
    });
    let source = position.clone();
    let space = CombatSearchSpace::default();
    let root = CombatSearchState::new(position).unwrap();
    let mut leaves = Vec::new();
    fn visit(
        space: &CombatSearchSpace,
        state: &CombatSearchState,
        mass: f64,
        source: &CombatPosition,
        leaves: &mut Vec<(ClientInput, f64)>,
    ) {
        assert_eq!(
            state.position(),
            source,
            "input assembly cannot change cards or RNG"
        );
        let choices = space.choices(state).unwrap();
        let total: f64 = choices.iter().map(|c| c.exploration_weight).sum();
        for c in choices {
            let mass = mass * c.exploration_weight / total;
            if let Some(input) = space.input(state, &c.action).unwrap() {
                assert!(EngineCombatStepper.is_legal_action(source, &input));
                assert!(!leaves.iter().any(|(old, _)| old == &input));
                leaves.push((input, mass));
            } else {
                visit(
                    space,
                    &settled(space, state, &c.action),
                    mass,
                    source,
                    leaves,
                );
            }
        }
    }
    visit(&space, &root, 1.0, &source, &mut leaves);
    assert_eq!(leaves.len(), 16); // 1 + 3 + 6 + 6 ordered inputs
    assert!(leaves.iter().all(|(_, p)| (*p - 1.0 / 16.0).abs() < 1e-12));
}

#[test]
fn sixty_four_card_scry_stays_linear_and_source_bound() {
    let mut position = combat_position(&[], &vec![CardId::Strike; 64], 40);
    position.engine = EngineState::PendingChoice(PendingChoice::ScrySelect {
        cards: vec![CardId::Strike; 64],
        card_lineages: sts_core::test_support::scenario_lineages(101..165),
    });
    let original = position.clone();
    let space = CombatSearchSpace::default();
    let root = CombatSearchState::new(position).unwrap();
    assert_eq!(space.choices(&root).unwrap().len(), 1);
    let prefix = settled(&space, &root, &CombatBranch::BeginSelection(0));
    assert_eq!(space.choices(&prefix).unwrap().len(), 65);
    let selected = settled(&space, &prefix, &CombatBranch::SelectMember(63));
    assert_eq!(selected.position(), &original);
    assert_eq!(
        space
            .input(&selected, &CombatBranch::FinishSelection)
            .unwrap(),
        Some(ClientInput::SubmitScryDiscard(vec![63]))
    );
    assert!(space
        .transition(
            &selected,
            &CombatBranch::SelectMember(63),
            CombatStepLimits {
                max_engine_steps: 256,
                deadline: None
            }
        )
        .is_err());
}

// A fixture-only policy supplies known actions to test the adapter. It is not
// a production heuristic or evidence that a model learned this strategy.
fn fixture_guidance(state: &CombatSearchState, actions: &[&CombatBranch]) -> Guidance {
    let weights = actions
        .iter()
        .map(|action| match action {
            CombatBranch::Input(ClientInput::PlayCard { card_index, .. }) => {
                match state.position().combat.zones.hand[*card_index].id {
                    CardId::BurningPact | CardId::Dropkick => 100.0,
                    CardId::Strike => 50.0,
                    _ => 0.0,
                }
            }
            CombatBranch::Input(ClientInput::EndTurn) => 0.0,
            _ => 1.0,
        })
        .collect::<Vec<_>>();
    Guidance {
        weights: weights.iter().any(|w| *w > 0.0).then_some(weights),
        value: None,
    }
}

fn search_real_combat(root: CombatPosition) -> (SearchSession<CombatSearchSpace>, NodeRef) {
    let mut session = SearchSession::new(
        CombatSearchSpace::default(),
        CombatSearchState::new(root).unwrap(),
        SearchConfig::default(),
        budget(),
    )
    .unwrap();
    loop {
        match session.step().unwrap() {
            StepStatus::NeedsGuidance(source) => {
                let view = session.view(source).unwrap();
                let guidance = fixture_guidance(view.state, &view.actions);
                session.submit_guidance(source, guidance).unwrap();
            }
            StepStatus::Terminal { node, won: true } => return (session, node),
            StepStatus::Stopped(reason) => panic!(
                "fixture remained unknown: {reason:?}; {:?}",
                session.stats()
            ),
            _ => (),
        }
    }
}

#[test]
fn burning_pact_selection_and_draw_replay_from_the_unchanged_root() {
    let mut root = combat_position(
        &[CardId::BurningPact, CardId::Defend, CardId::Wound],
        &[CardId::Strike],
        6,
    );
    root.combat.entities.player.current_hp = 1;
    let original = root.clone();
    let (session, win) = search_real_combat(root);
    let replay = session.replay_win(win, 10_000, 256, None).unwrap();
    assert_eq!(replay.root(), &original);
    assert_eq!(
        session.view(session.root()).unwrap().state.position(),
        &original
    );
    assert!(
        replay
            .inputs()
            .iter()
            .any(|i| matches!(i, ClientInput::SubmitSelection(_))),
        "inputs: {:?}",
        replay.inputs()
    );
    assert!(EngineCombatStepper.is_exact_victory(replay.successor()));
    assert!(session.replay_win(win, 0, 256, None).is_err());
    assert!(session
        .replay_win(
            win,
            10_000,
            256,
            Some(Instant::now() - Duration::from_secs(1))
        )
        .is_err());
}

#[test]
fn repeated_dropkicks_follow_real_draws_and_shuffle_without_cycle_arithmetic() {
    let mut root = combat_position(&[CardId::Dropkick, CardId::Dropkick], &[], 28);
    root.combat.turn.energy = 1;
    root.combat.entities.player.current_hp = 1;
    let target = root.combat.entities.monsters[0].id;
    sts_core::content::powers::store::set_powers_for(
        &mut root.combat,
        target,
        vec![Power {
            power_type: PowerId::Vulnerable,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let (session, win) = search_real_combat(root);
    let replay = session.replay_win(win, 10_000, 256, None).unwrap();
    assert_eq!(replay.inputs().len(), 4);
    assert!(replay
        .inputs()
        .iter()
        .all(|i| matches!(i, ClientInput::PlayCard { .. })));
    assert!(EngineCombatStepper.is_exact_victory(replay.successor()));
    assert_ne!(replay.root().combat.rng, replay.successor().combat.rng);
}

#[test]
fn potion_constraints_do_not_depend_on_search_budget_and_escape_is_not_a_win() {
    let mut root = combat_position(&[CardId::Strike], &[], 40);
    root.combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 1))];
    let state = CombatSearchState::new(root).unwrap();
    let unlimited = CombatSearchSpace::default();
    let limited = CombatSearchSpace::with_potion_limit(0);
    let smoke = unlimited
        .choices(&state)
        .unwrap()
        .into_iter()
        .find(|c| matches!(c.action, CombatBranch::Input(ClientInput::UsePotion { .. })))
        .unwrap()
        .action;
    assert!(!limited
        .choices(&state)
        .unwrap()
        .iter()
        .any(|c| c.action == smoke));
    assert!(limited
        .transition(
            &state,
            &smoke,
            CombatStepLimits {
                max_engine_steps: 256,
                deadline: None
            }
        )
        .is_err());
    let escaped = settled(&unlimited, &state, &smoke);
    assert_eq!(escaped.potion_uses(), 1);
    assert_eq!(
        unlimited.terminal(&escaped),
        Some(false),
        "engine={:?}, smoked={}",
        escaped.position().engine,
        escaped.position().combat.runtime.combat_smoked
    );
}

#[test]
fn dead_monsters_during_processing_are_not_a_settled_root() {
    let mut position = combat_position(&[], &[], 0);
    position.engine = EngineState::CombatProcessing;
    assert!(CombatSearchState::new(position).is_err());
}
