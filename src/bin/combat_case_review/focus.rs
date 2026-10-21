#[path = "focus/prior_rerun.rs"]
mod prior_rerun;
#[path = "focus/ranking.rs"]
mod ranking;
#[path = "focus/types.rs"]
mod types;
#[path = "focus/witness.rs"]
mod witness;

pub(super) use prior_rerun::witness_prior_rerun;
pub(super) use ranking::review_focus;
pub(super) use types::{CombatReviewFocus, CombatReviewFocusPriorRerun};
pub(super) use witness::focus_witness_line;

#[cfg(test)]
mod tests {
    use sts_oracle_runtime::ai::combat_search_v2::{
        CombatSearchV2ActionPreview, SearchTerminalLabel,
    };
    use sts_oracle_runtime::state::core::ClientInput;

    use super::super::search_types::SearchDiagnosticProgressFacts;
    use super::ranking::{focus_reason, progress_is_better_focus};
    use super::{focus_witness_line, CombatReviewFocus};

    fn progress_with_hidden_full_actions() -> SearchDiagnosticProgressFacts {
        SearchDiagnosticProgressFacts {
            source: "best_complete",
            terminal: SearchTerminalLabel::Loss,
            estimated: false,
            final_hp: 0,
            hp_loss: 10,
            turns: 1,
            potions_used: 0,
            cards_played: 0,
            living_enemy_count: 1,
            total_enemy_hp: 20,
            half_dead_enemy_count: 0,
            awakened_one_phase: None,
            awakened_one_phase_observation: "fixture",
            visible_incoming_damage: None,
            action_count: Some(2),
            exact_prefix_action_count: Some(2),
            action_key_preview: vec!["preview_only".to_string()],
            input_preview: vec![ClientInput::EndTurn],
            full_action_preview: vec![
                CombatSearchV2ActionPreview {
                    action_key: "full_1".to_string(),
                    input: ClientInput::EndTurn,
                },
                CombatSearchV2ActionPreview {
                    action_key: "full_2".to_string(),
                    input: ClientInput::Cancel,
                },
            ],
        }
    }

    #[test]
    fn focus_witness_line_prefers_hidden_full_actions_over_json_preview() {
        let focus = CombatReviewFocus {
            selected_review: "test",
            reason: "test",
            progress: progress_with_hidden_full_actions(),
        };

        let witness = focus_witness_line(&focus);

        assert_eq!(
            witness
                .actions
                .iter()
                .map(|action| action.action_key.as_str())
                .collect::<Vec<_>>(),
            vec!["full_1", "full_2"]
        );
    }

    #[test]
    fn focus_reason_marks_phase_pending_enemy_death_separately() {
        let mut progress = progress_with_hidden_full_actions();
        progress.final_hp = 0;
        progress.total_enemy_hp = 0;
        progress.living_enemy_count = 0;
        progress.half_dead_enemy_count = 1;

        assert_eq!(focus_reason(&progress), "phase_pending_enemy_player_died");
    }

    #[test]
    fn focus_ranking_does_not_treat_half_dead_zero_hp_as_plain_enemy_clear() {
        let mut phase_pending = progress_with_hidden_full_actions();
        phase_pending.final_hp = 0;
        phase_pending.total_enemy_hp = 0;
        phase_pending.living_enemy_count = 0;
        phase_pending.half_dead_enemy_count = 1;

        let mut ordinary_failure = progress_with_hidden_full_actions();
        ordinary_failure.final_hp = 0;
        ordinary_failure.total_enemy_hp = 10;
        ordinary_failure.living_enemy_count = 1;
        ordinary_failure.half_dead_enemy_count = 0;

        assert!(progress_is_better_focus(&ordinary_failure, &phase_pending));
        assert!(!progress_is_better_focus(&phase_pending, &ordinary_failure));
    }
}
