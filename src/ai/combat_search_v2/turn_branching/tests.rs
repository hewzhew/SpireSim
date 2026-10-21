use super::*;
use crate::content::monsters::EnemyId;
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn classifies_same_turn_card_child() {
    let mut parent = blank_test_combat();
    parent.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let mut child = parent.clone();
    child.turn.energy = 2;

    let transition = classify_turn_branch_transition(
        &EngineState::CombatPlayerTurn,
        &parent,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        &EngineState::CombatPlayerTurn,
        &child,
    );

    assert_eq!(transition.kind, TurnBranchTransitionKind::SameTurn);
    assert_eq!(transition.frontier_priority_hint(), 12);
}

#[test]
fn classifies_end_turn_next_turn_child() {
    let mut parent = blank_test_combat();
    parent.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let mut child = parent.clone();
    child.turn.turn_count = parent.turn.turn_count + 1;

    let transition = classify_turn_branch_transition(
        &EngineState::CombatPlayerTurn,
        &parent,
        &ClientInput::EndTurn,
        &EngineState::CombatPlayerTurn,
        &child,
    );

    assert_eq!(transition.kind, TurnBranchTransitionKind::NextTurn);
    assert_eq!(transition.frontier_priority_hint(), 0);
}

#[test]
fn collector_reports_turn_child_mix_without_action_tree() {
    let combat = blank_test_combat();
    let mut observation = TurnBranchingStateObservation::new(&combat, 2);
    observation.observe_child(TurnBranchTransition {
        action_kind: TurnBranchActionKind::PlayCard,
        kind: TurnBranchTransitionKind::SameTurn,
    });
    observation.observe_child(TurnBranchTransition {
        action_kind: TurnBranchActionKind::EndTurn,
        kind: TurnBranchTransitionKind::NextTurn,
    });
    let mut collector = TurnBranchingDiagnosticsCollector::default();

    collector.observe(&observation);
    let report = collector.finish();

    assert_eq!(
        report.behavioral_effect,
        "diagnostic_summary_plus_priority_hint_no_prune_no_merge"
    );
    assert_eq!(report.states_observed, 1);
    assert_eq!(report.same_turn_children, 1);
    assert_eq!(report.next_turn_children, 1);
    assert_eq!(report.end_turn_children, 1);
    assert_eq!(report.largest_turn_fanouts.len(), 1);
}
