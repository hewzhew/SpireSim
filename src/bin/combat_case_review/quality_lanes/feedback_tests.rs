use sts_oracle_runtime::ai::combat_search_v2::{CombatSearchV2ActionPreview, SearchTerminalLabel};
use sts_oracle_runtime::state::core::ClientInput;

use super::super::search_types::SearchDiagnosticProgressFacts;
use super::feedback_progress::estimated_rollout_feedback_witness;

#[test]
fn estimated_rollout_win_progress_can_become_feedback_witness() {
    let progress = SearchDiagnosticProgressFacts {
        source: "rollout_frontier",
        terminal: SearchTerminalLabel::Win,
        estimated: true,
        final_hp: 12,
        hp_loss: 8,
        turns: 3,
        potions_used: 1,
        cards_played: 4,
        living_enemy_count: 0,
        total_enemy_hp: 0,
        half_dead_enemy_count: 0,
        awakened_one_phase: None,
        awakened_one_phase_observation: "fixture",
        visible_incoming_damage: Some(0),
        action_count: Some(6),
        exact_prefix_action_count: Some(2),
        action_key_preview: vec!["a".to_string(), "b".to_string()],
        input_preview: vec![ClientInput::EndTurn, ClientInput::EndTurn],
        full_action_preview: vec![
            CombatSearchV2ActionPreview {
                action_key: "a".to_string(),
                input: ClientInput::EndTurn,
            },
            CombatSearchV2ActionPreview {
                action_key: "b".to_string(),
                input: ClientInput::EndTurn,
            },
        ],
    };

    let witness = estimated_rollout_feedback_witness("lane", &progress)
        .expect("rollout win progress should be reusable as witness");

    assert_eq!(witness.source, "lane");
    assert_eq!(witness.terminal, SearchTerminalLabel::Win);
    assert_eq!(witness.action_count, Some(6));
    assert_eq!(witness.actions.len(), 2);
}

#[test]
fn non_winning_rollout_progress_is_not_feedback_witness() {
    let progress = SearchDiagnosticProgressFacts {
        source: "rollout_frontier",
        terminal: SearchTerminalLabel::Loss,
        estimated: true,
        final_hp: 0,
        hp_loss: 30,
        turns: 2,
        potions_used: 0,
        cards_played: 3,
        living_enemy_count: 2,
        total_enemy_hp: 100,
        half_dead_enemy_count: 0,
        awakened_one_phase: None,
        awakened_one_phase_observation: "fixture",
        visible_incoming_damage: Some(20),
        action_count: Some(4),
        exact_prefix_action_count: Some(2),
        action_key_preview: vec!["a".to_string()],
        input_preview: vec![ClientInput::EndTurn],
        full_action_preview: vec![CombatSearchV2ActionPreview {
            action_key: "a".to_string(),
            input: ClientInput::EndTurn,
        }],
    };

    assert!(estimated_rollout_feedback_witness("lane", &progress).is_none());
}
