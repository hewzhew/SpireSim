use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::runtime::combat::CombatCard;
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn prunes_same_parent_same_turn_dominance_duplicate_child() {
    let parent = test_combat();
    let mut child_combat = parent.clone();
    child_combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let first = test_node(child_combat.clone(), 1);
    let second = test_node(child_combat, 1);
    let mut observation =
        TurnLocalDominanceStateObservation::new(&EngineState::CombatPlayerTurn, &parent, 2);

    assert!(!observation.observe_child(&first));
    assert!(observation.observe_child(&second));

    assert_eq!(observation.eligible_child_states, 2);
    assert_eq!(observation.accepted_child_states, 1);
    assert_eq!(observation.pruned_child_states, 1);
}

#[test]
fn keeps_same_dominance_child_when_resource_vector_is_not_covered() {
    let parent = test_combat();
    let child_combat = parent.clone();
    let first = test_node(child_combat.clone(), 1);
    let second = test_node(child_combat, 2);
    let mut observation =
        TurnLocalDominanceStateObservation::new(&EngineState::CombatPlayerTurn, &parent, 2);

    assert!(!observation.observe_child(&second));
    assert!(!observation.observe_child(&first));

    assert_eq!(observation.accepted_child_states, 2);
    assert_eq!(observation.pruned_child_states, 0);
}

#[test]
fn prunes_hp_block_variant_when_resource_vector_is_covered() {
    let parent = test_combat();
    let mut better_combat = parent.clone();
    better_combat.entities.player.current_hp = 70;
    better_combat.entities.player.block = 5;
    let mut worse_combat = parent.clone();
    worse_combat.entities.player.current_hp = 60;
    worse_combat.entities.player.block = 0;
    let better = test_node(better_combat, 1);
    let worse = test_node(worse_combat, 1);
    let mut observation =
        TurnLocalDominanceStateObservation::new(&EngineState::CombatPlayerTurn, &parent, 2);

    assert!(!observation.observe_child(&better));
    assert!(observation.observe_child(&worse));

    assert_eq!(observation.accepted_child_states, 1);
    assert_eq!(observation.pruned_child_states, 1);
}

#[test]
fn ignores_next_turn_children() {
    let parent = test_combat();
    let mut child_combat = parent.clone();
    child_combat.turn.turn_count = parent.turn.turn_count + 1;
    let child = test_node(child_combat, 1);
    let mut observation =
        TurnLocalDominanceStateObservation::new(&EngineState::CombatPlayerTurn, &parent, 1);

    assert!(!observation.observe_child(&child));

    assert_eq!(observation.eligible_child_states, 0);
    assert_eq!(observation.pruned_child_states, 0);
}

#[test]
fn collector_reports_parent_prunes_without_action_tree() {
    let parent = test_combat();
    let child = test_node(parent.clone(), 1);
    let mut observation =
        TurnLocalDominanceStateObservation::new(&EngineState::CombatPlayerTurn, &parent, 2);
    assert!(!observation.observe_child(&child));
    assert!(observation.observe_child(&child));
    let mut collector = TurnLocalDominanceDiagnosticsCollector::default();

    collector.observe(&observation);
    let report = collector.finish();

    assert_eq!(
        report.behavioral_effect,
        "safe_sibling_child_prune_only_no_cross_parent_no_next_turn_no_terminal_prune"
    );
    assert_eq!(report.parent_states_observed, 1);
    assert_eq!(report.enabled_parent_states, 1);
    assert_eq!(report.eligible_child_states, 2);
    assert_eq!(report.pruned_child_states, 1);
    assert_eq!(report.largest_parent_samples.len(), 1);
}

fn test_node(combat: CombatState, action_count: usize) -> SearchNode {
    SearchNode {
        engine: EngineState::CombatPlayerTurn,
        combat,
        actions: vec![
            CombatSearchV2ActionTrace {
                step_index: 0,
                action_id: 0,
                action_key: "test".to_string(),
                action_debug: "test".to_string(),
                input: ClientInput::EndTurn,
            };
            action_count
        ],
        turn_prefix: TurnPrefixState::default(),
        initial_hp: 80,
        potions_used: 0,
        potions_discarded: 0,
        cards_played: 0,
        potion_tactical_priority: 0,
        last_turn_branch_priority: 0,
        action_prior_score: None,
        action_ordering_frontier_hint: 0,
        rollout_estimate: RolloutNodeEstimate::unevaluated(),
        root_lineage: Default::default(),
    }
}

fn test_combat() -> CombatState {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat
}
