use super::super::*;
use super::{
    CombatSearchV2DecisionMicroscopeConfigReport, CombatSearchV2DecisionSelectedAction,
    CombatSearchV2DecisionTrajectorySummary,
};

pub(super) fn selected_first_action(
    engine: &EngineState,
    combat: &CombatState,
    plugins: CombatSearchActionOrderingPlugins<'_>,
    search_report: &CombatSearchV2Report,
) -> Option<CombatSearchV2DecisionSelectedAction> {
    let (selection_source, action) = if let Some(action) = search_report
        .best_win_trajectory
        .as_ref()
        .and_then(|trajectory| trajectory.actions.first())
    {
        ("best_win_trajectory_first_action", action)
    } else {
        (
            "best_complete_trajectory_first_action",
            search_report
                .best_complete_trajectory
                .as_ref()?
                .actions
                .first()?,
        )
    };
    Some(CombatSearchV2DecisionSelectedAction {
        action_id: action.action_id,
        action_key: action.action_key.clone(),
        action_debug: action.action_debug.clone(),
        action_role: combat_search_action_ordering_role_label_for_state_with_plugins(
            engine,
            combat,
            &action.input,
            plugins,
        ),
        selection_source,
    })
}

pub(super) fn trajectory_summary(
    trajectory: &CombatSearchV2TrajectoryReport,
) -> CombatSearchV2DecisionTrajectorySummary {
    CombatSearchV2DecisionTrajectorySummary {
        terminal: trajectory.terminal,
        estimated: trajectory.estimated,
        final_hp: trajectory.final_hp,
        hp_loss: trajectory.hp_loss,
        turns: trajectory.turns,
        potions_used: trajectory.potions_used,
        potions_discarded: trajectory.potions_discarded,
        cards_played: trajectory.cards_played,
        action_count: trajectory.actions.len(),
    }
}

pub(super) fn config_report(
    config: &CombatSearchV2Config,
) -> CombatSearchV2DecisionMicroscopeConfigReport {
    let plugins = CombatSearchPluginStack::from_config(config);
    CombatSearchV2DecisionMicroscopeConfigReport {
        max_nodes: config.max_nodes,
        max_actions_per_line: config.max_actions_per_line,
        max_engine_steps_per_action: config.max_engine_steps_per_action,
        wall_time_ms: config.wall_time.map(|duration| duration.as_millis()),
        potion_policy: plugins.potion.policy.label(),
        max_potions_used: plugins.potion.max_potions_used,
        rollout_policy: CombatSearchV2RolloutPolicy::from(plugins.rollout).label(),
        rollout_max_evaluations: config.rollout_max_evaluations,
        rollout_max_actions: config.rollout_max_actions,
        rollout_beam_width: config.rollout_beam_width,
        phase_guard_policy: plugins.phase_guard.label(),
        setup_bias_policy: plugins.action_prior.label(),
    }
}
