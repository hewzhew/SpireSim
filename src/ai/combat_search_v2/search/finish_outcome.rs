use super::super::*;

pub(super) fn outcome_report(
    coverage_status: SearchCoverageStatus,
    coverage_reason: String,
    complete_trajectory_found: bool,
    complete_win_found: bool,
    exhaustive: bool,
) -> CombatSearchV2OutcomeReport {
    CombatSearchV2OutcomeReport {
        coverage_status,
        coverage_reason,
        complete_trajectory_found,
        complete_win_found,
        exhaustive,
    }
}
