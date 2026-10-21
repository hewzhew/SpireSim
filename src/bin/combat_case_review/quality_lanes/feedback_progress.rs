use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchV2ActionPreview, CombatSearchV2WitnessLine, SearchTerminalLabel,
};

use super::super::search_types::SearchDiagnosticProgressFacts;

pub(super) fn estimated_rollout_feedback_witness(
    source: &'static str,
    progress: &SearchDiagnosticProgressFacts,
) -> Option<CombatSearchV2WitnessLine> {
    if progress.source != "rollout_frontier"
        || progress.terminal != SearchTerminalLabel::Win
        || !progress.estimated
    {
        return None;
    }
    let exact_prefix_action_count = progress.exact_prefix_action_count?;
    if exact_prefix_action_count == 0 {
        return None;
    }
    let actions = progress
        .action_key_preview
        .iter()
        .cloned()
        .zip(progress.input_preview.iter().cloned())
        .take(exact_prefix_action_count)
        .map(|(action_key, input)| CombatSearchV2ActionPreview { action_key, input })
        .collect::<Vec<_>>();
    if actions.is_empty() {
        return None;
    }
    Some(CombatSearchV2WitnessLine {
        source,
        terminal: progress.terminal,
        final_hp: progress.final_hp,
        total_enemy_hp: progress.total_enemy_hp,
        action_count: progress.action_count,
        actions,
    })
}

pub(super) fn estimated_rollout_feedback_rank(
    progress: &SearchDiagnosticProgressFacts,
) -> (i32, i32, i32, i32) {
    (
        progress.final_hp,
        -(progress.potions_used as i32),
        -(progress.total_enemy_hp),
        -(progress.action_count.unwrap_or(usize::MAX) as i32),
    )
}
