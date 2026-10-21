use super::super::*;
use super::best_trajectories::SearchTrajectoryBook;

pub(super) struct SearchTrajectoryReports {
    pub(super) best_complete_trajectory: Option<CombatSearchV2TrajectoryReport>,
    pub(super) best_win_trajectory: Option<CombatSearchV2TrajectoryReport>,
    pub(super) win_candidate_trajectories: Vec<CombatSearchV2TrajectoryReport>,
    pub(super) best_frontier_trajectory: Option<CombatSearchV2TrajectoryReport>,
    pub(super) best_frontier_value: Option<CombatSearchV2FrontierValueReport>,
}

pub(super) fn trajectory_reports(trajectories: SearchTrajectoryBook) -> SearchTrajectoryReports {
    let SearchTrajectoryBook {
        best_complete,
        best_win,
        win_candidates,
        win_frontier_revision: _,
        best_frontier,
        ..
    } = trajectories;

    SearchTrajectoryReports {
        best_complete_trajectory: best_complete
            .as_ref()
            .map(|node| trajectory_report(node, false)),
        best_win_trajectory: best_win.as_ref().map(|node| trajectory_report(node, false)),
        win_candidate_trajectories: win_candidates
            .iter()
            .map(|candidate| trajectory_report(&candidate.node, false))
            .collect(),
        best_frontier_trajectory: best_frontier.as_ref().map(|node| {
            trajectory_report(
                node,
                terminal_label(&node.engine, &node.combat) == SearchTerminalLabel::Unresolved,
            )
        }),
        best_frontier_value: best_frontier
            .as_ref()
            .map(combat_search_frontier_value_report),
    }
}
