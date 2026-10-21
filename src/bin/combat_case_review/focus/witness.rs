use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchV2ActionPreview, CombatSearchV2WitnessLine,
};

use super::types::CombatReviewFocus;

pub(crate) fn focus_witness_line(focus: &CombatReviewFocus) -> CombatSearchV2WitnessLine {
    let actions = if focus.progress.full_action_preview.is_empty() {
        focus
            .progress
            .action_key_preview
            .iter()
            .cloned()
            .zip(focus.progress.input_preview.iter().cloned())
            .map(|(action_key, input)| CombatSearchV2ActionPreview { action_key, input })
            .collect()
    } else {
        focus.progress.full_action_preview.clone()
    };
    CombatSearchV2WitnessLine {
        source: focus.progress.source,
        terminal: focus.progress.terminal,
        final_hp: focus.progress.final_hp,
        total_enemy_hp: focus.progress.total_enemy_hp,
        action_count: focus.progress.action_count,
        actions,
    }
}
