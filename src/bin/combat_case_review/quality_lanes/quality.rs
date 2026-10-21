use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchV2ActionPreview, CombatSearchV2Report, CombatSearchV2TrajectoryReport,
    CombatSearchV2WitnessLine,
};

use super::types::CombatLineQuality;

pub(crate) fn witness_line_from_trajectory(
    source: &'static str,
    trajectory: &CombatSearchV2TrajectoryReport,
) -> CombatSearchV2WitnessLine {
    CombatSearchV2WitnessLine {
        source,
        terminal: trajectory.terminal,
        final_hp: trajectory.final_hp,
        total_enemy_hp: trajectory
            .enemy_final_state
            .iter()
            .filter(|enemy| enemy.alive)
            .map(|enemy| enemy.hp.max(0) + enemy.block.max(0))
            .sum(),
        action_count: Some(trajectory.actions.len()),
        actions: trajectory
            .actions
            .iter()
            .map(|action| CombatSearchV2ActionPreview {
                action_key: action.action_key.clone(),
                input: action.input.clone(),
            })
            .collect(),
    }
}

pub(crate) fn combat_line_quality(report: &CombatSearchV2Report) -> Option<CombatLineQuality> {
    let trajectory = report.best_win_trajectory.as_ref()?;
    Some(CombatLineQuality {
        terminal: trajectory.terminal,
        hp_loss: trajectory.hp_loss,
        final_hp: trajectory.final_hp,
        persistent_run_value: trajectory.persistent_run_value,
        persistent_adjusted_hp: trajectory
            .final_hp
            .saturating_add(trajectory.persistent_run_value),
        potions_used: trajectory.potions_used,
        turns: trajectory.turns,
        cards_played: trajectory.cards_played,
        action_count: trajectory.actions.len(),
    })
}

pub(crate) fn compare_quality(
    left: &CombatLineQuality,
    right: &CombatLineQuality,
) -> std::cmp::Ordering {
    (
        left.persistent_adjusted_hp,
        left.final_hp,
        left.persistent_run_value,
        -(left.potions_used as i32),
        -(left.turns as i32),
        -(left.cards_played as i32),
        -(left.action_count as i32),
    )
        .cmp(&(
            right.persistent_adjusted_hp,
            right.final_hp,
            right.persistent_run_value,
            -(right.potions_used as i32),
            -(right.turns as i32),
            -(right.cards_played as i32),
            -(right.action_count as i32),
        ))
}
