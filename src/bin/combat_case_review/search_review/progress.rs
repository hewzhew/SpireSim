use sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Report;

use super::super::search_types::SearchDiagnosticProgressFacts;
use super::progress_complete::complete_progress_facts;
use super::progress_rollout::rollout_progress_facts;

pub(super) fn diagnostic_progress_facts(
    report: &CombatSearchV2Report,
    action_preview_limit: usize,
) -> Option<SearchDiagnosticProgressFacts> {
    complete_progress_facts(report, action_preview_limit)
        .or_else(|| rollout_progress_facts(report, action_preview_limit))
}
