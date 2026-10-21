use super::*;

fn configured(graph: Graph, steps: usize, work: SearchBudget) -> SearchSession<Graph> {
    SearchSession::new(
        graph,
        0,
        SearchConfig {
            max_policy_rollout_steps: steps,
            ..Default::default()
        },
        work,
    )
    .unwrap()
}

fn guide_next(session: &mut SearchSession<Graph>) {
    for source in session.pending_guidance() {
        // Equal guidance also establishes deterministic raw-policy tie order.
        session
            .submit_guidance(source, Guidance::default())
            .unwrap();
    }
}

#[test]
fn rollout_finishes_a_policy_continuation_before_exploring_siblings() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![3, 4], vec![], vec![5], vec![], vec![]],
        vec![None, None, Some(false), None, Some(false), Some(true)],
    );
    let mut session = configured(graph.clone(), 128, budget());
    for _ in 0..3 {
        guide_next(&mut session);
        session.step().unwrap();
    }
    assert_eq!(*graph.calls.borrow(), [(0, 1), (1, 3), (3, 5)]);
    assert_eq!(session.observed_wins().len(), 1);
    assert_eq!(session.stats().policy_rollout_services, 3);
    assert_eq!(session.stats().engine_steps, 3);
}

#[test]
fn rollout_limit_returns_work_to_tree_without_fabricating_a_terminal() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![3], vec![], vec![4], vec![]],
        vec![None, None, Some(false), None, Some(true)],
    );
    let mut session = configured(graph.clone(), 2, budget());
    for _ in 0..3 {
        guide_next(&mut session);
        session.step().unwrap();
    }
    assert_eq!(*graph.calls.borrow(), [(0, 1), (1, 3), (0, 2)]);
    assert_eq!(session.stats().policy_rollout_services, 2);
    assert_eq!(session.stats().terminal_observations, 1);
    assert!(session.observed_wins().is_empty());
    assert_eq!(session.stats().coverage_gaps, 0);
}

#[test]
fn failed_rollout_does_not_hide_an_alternative_win_or_repeat_a_closed_path() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![3], vec![4], vec![], vec![]],
        vec![None, None, None, Some(false), Some(true)],
    );
    let mut session = configured(graph.clone(), 128, budget());
    for _ in 0..4 {
        guide_next(&mut session);
        session.step().unwrap();
    }
    assert_eq!(*graph.calls.borrow(), [(0, 1), (1, 3), (0, 2), (2, 4)]);
    assert_eq!(session.stats().terminal_observations, 2);
    assert_eq!(session.observed_wins().len(), 1);
    assert_eq!(session.stats().services, 4);
}

#[test]
fn rollout_keeps_shared_budget_and_cycle_coverage_semantics() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![0], vec![]],
        vec![None, None, Some(true)],
    );
    let mut session = configured(
        graph.clone(),
        128,
        SearchBudget {
            max_services: 2,
            ..budget()
        },
    );
    for _ in 0..2 {
        guide_next(&mut session);
        session.step().unwrap();
    }
    assert_eq!(
        session.step().unwrap(),
        StepStatus::Stopped(StopReason::ServiceBudget)
    );
    assert_eq!(*graph.calls.borrow(), [(0, 1), (1, 0)]);
    assert_eq!(session.stats().coverage_gaps, 1);
    assert_eq!(session.stats().terminal_observations, 0);
    assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 0);
    assert!(session.observed_wins().is_empty());
}
