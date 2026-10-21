use std::collections::HashSet;

use crate::ai::combat_search_v2::{CombatSearchV2Report, CombatSearchV2TrajectoryReport};

pub(super) struct RetainedWinTrajectory<'a> {
    pub(super) retained_index: usize,
    pub(super) trajectory: &'a CombatSearchV2TrajectoryReport,
}

pub(super) struct RetainedWinTrajectories<'a> {
    pub(super) retained_candidate_count: usize,
    pub(super) trajectories: Vec<RetainedWinTrajectory<'a>>,
}

pub(super) fn unique_retained_win_trajectories(
    report: &CombatSearchV2Report,
) -> RetainedWinTrajectories<'_> {
    let retained = report
        .best_win_trajectory
        .iter()
        .chain(&report.win_candidate_trajectories)
        .collect::<Vec<_>>();
    let unique_indices = unique_action_trace_indices(&retained);
    let trajectories = unique_indices
        .into_iter()
        .map(|retained_index| RetainedWinTrajectory {
            retained_index,
            trajectory: retained[retained_index],
        })
        .collect();
    RetainedWinTrajectories {
        retained_candidate_count: retained.len(),
        trajectories,
    }
}

fn unique_action_trace_indices(retained: &[&CombatSearchV2TrajectoryReport]) -> Vec<usize> {
    let mut seen = HashSet::<Vec<&str>>::new();
    retained
        .iter()
        .enumerate()
        .filter_map(|(index, trajectory)| {
            let fingerprint = trajectory
                .actions
                .iter()
                .map(|action| action.action_key.as_str())
                .collect::<Vec<_>>();
            seen.insert(fingerprint).then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::ai::combat_search_v2::{
        CombatSearchV2ActionTrace, CombatSearchV2OutcomeOrderKeyReport, CombatSearchV2StateSummary,
        CombatSearchV2TrajectoryReport, SearchTerminalLabel,
    };
    use crate::state::core::ClientInput;

    use super::unique_action_trace_indices;

    fn trajectory(keys: &[&str]) -> CombatSearchV2TrajectoryReport {
        CombatSearchV2TrajectoryReport {
            terminal: SearchTerminalLabel::Win,
            estimated: false,
            outcome_order_key: CombatSearchV2OutcomeOrderKeyReport {
                terminal_rank: 2,
                run_hygiene: 0,
                persistent_adjusted_hp: 30,
                final_hp: 30,
                persistent_run_value: 0,
                potion_conservation: 0,
                faster_turns: -2,
                fewer_cards_played: -1,
                enemy_progress: 0,
                shorter_line: -(keys.len() as i32),
            },
            actions: keys
                .iter()
                .enumerate()
                .map(|(step_index, action_key)| CombatSearchV2ActionTrace {
                    step_index,
                    action_id: step_index,
                    action_key: (*action_key).to_string(),
                    action_debug: (*action_key).to_string(),
                    input: ClientInput::EndTurn,
                })
                .collect(),
            final_hp: 30,
            final_max_hp: 40,
            persistent_run_value: 0,
            final_block: 0,
            hp_loss: 10,
            turns: 2,
            potions_used: 0,
            potions_discarded: 0,
            cards_played: 1,
            enemy_final_state: Vec::new(),
            final_state: CombatSearchV2StateSummary {
                engine_state: "RewardScreen".to_string(),
                terminal: SearchTerminalLabel::Win,
                player_hp: 30,
                player_block: 0,
                energy: 0,
                turn_count: 2,
                living_enemy_count: 0,
                total_enemy_hp: 0,
                visible_incoming_damage: 0,
                enemy_slots: Vec::new(),
                hand_count: 0,
                draw_count: 0,
                discard_count: 0,
                exhaust_count: 0,
                limbo_count: 0,
                queued_cards_count: 0,
            },
        }
    }

    #[test]
    fn retained_action_trace_dedup_preserves_first_report_index() {
        let first = trajectory(&["a", "b"]);
        let duplicate = trajectory(&["a", "b"]);
        let distinct = trajectory(&["a", "c"]);
        let retained = [&first, &duplicate, &distinct];

        assert_eq!(unique_action_trace_indices(&retained), vec![0, 2]);
    }
}
