use sts_oracle_runtime::ai::combat_search_v2::{CombatSearchV2ActionPreview, CombatSearchV2Report};

use super::super::search_types::SearchDiagnosticProgressFacts;

pub(super) fn complete_progress_facts(
    report: &CombatSearchV2Report,
    action_preview_limit: usize,
) -> Option<SearchDiagnosticProgressFacts> {
    let trajectory = report.best_complete_trajectory.as_ref()?;
    Some(SearchDiagnosticProgressFacts {
        source: "best_complete",
        terminal: trajectory.terminal,
        estimated: trajectory.estimated,
        final_hp: trajectory.final_hp,
        hp_loss: trajectory.hp_loss,
        turns: trajectory.turns,
        potions_used: trajectory.potions_used,
        cards_played: trajectory.cards_played,
        living_enemy_count: trajectory.final_state.living_enemy_count,
        total_enemy_hp: trajectory.final_state.total_enemy_hp,
        half_dead_enemy_count: trajectory
            .final_state
            .enemy_slots
            .iter()
            .filter(|enemy| enemy.half_dead)
            .count(),
        awakened_one_phase: trajectory
            .final_state
            .enemy_slots
            .iter()
            .find(|enemy| enemy.enemy_id == "AwakenedOne")
            .and_then(|enemy| enemy.phase),
        awakened_one_phase_observation: "exact_final_state",
        visible_incoming_damage: Some(trajectory.final_state.visible_incoming_damage),
        action_count: Some(trajectory.actions.len()),
        exact_prefix_action_count: Some(trajectory.actions.len()),
        action_key_preview: trajectory
            .actions
            .iter()
            .take(action_preview_limit)
            .map(|action| action.action_key.clone())
            .collect(),
        input_preview: trajectory
            .actions
            .iter()
            .take(action_preview_limit)
            .map(|action| action.input.clone())
            .collect(),
        full_action_preview: trajectory
            .actions
            .iter()
            .map(|action| CombatSearchV2ActionPreview {
                action_key: action.action_key.clone(),
                input: action.input.clone(),
            })
            .collect(),
    })
}
