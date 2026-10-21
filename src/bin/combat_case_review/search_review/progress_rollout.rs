use sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Report;

use super::super::search_types::SearchDiagnosticProgressFacts;

pub(super) fn rollout_progress_facts(
    report: &CombatSearchV2Report,
    action_preview_limit: usize,
) -> Option<SearchDiagnosticProgressFacts> {
    report
        .rollout
        .best_frontier_estimate
        .as_ref()
        .map(|rollout| {
            let frontier = report.best_frontier_trajectory.as_ref();
            let exact_prefix_actions = frontier
                .map(|trajectory| trajectory.actions.as_slice())
                .unwrap_or(&[]);
            let exact_prefix_action_count = Some(exact_prefix_actions.len());
            SearchDiagnosticProgressFacts {
                source: "rollout_frontier",
                terminal: rollout.terminal,
                estimated: rollout.estimated,
                final_hp: rollout.final_hp,
                hp_loss: rollout.hp_loss,
                turns: rollout.turns,
                potions_used: rollout.potions_used,
                cards_played: rollout.cards_played,
                living_enemy_count: rollout.living_enemy_count,
                total_enemy_hp: rollout.total_enemy_hp,
                half_dead_enemy_count: frontier
                    .map(|trajectory| {
                        trajectory
                            .final_state
                            .enemy_slots
                            .iter()
                            .filter(|enemy| enemy.half_dead)
                            .count()
                    })
                    .unwrap_or_default(),
                awakened_one_phase: None,
                awakened_one_phase_observation: "unavailable_for_estimated_rollout_endpoint",
                visible_incoming_damage: frontier
                    .map(|trajectory| trajectory.final_state.visible_incoming_damage),
                action_count: Some(
                    rollout
                        .actions_simulated
                        .saturating_add(exact_prefix_actions.len()),
                ),
                exact_prefix_action_count,
                action_key_preview: rollout
                    .action_preview
                    .iter()
                    .take(action_preview_limit)
                    .map(|action| action.action_key.clone())
                    .collect(),
                input_preview: rollout
                    .action_preview
                    .iter()
                    .take(action_preview_limit)
                    .map(|action| action.input.clone())
                    .collect(),
                full_action_preview: rollout.action_preview.clone(),
            }
        })
}
