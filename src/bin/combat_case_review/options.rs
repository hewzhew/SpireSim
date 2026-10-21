use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchChildRolloutPluginId, CombatSearchExpansionPluginId, CombatSearchRolloutPluginId,
    CombatSearchTurnPlanPluginId,
};

use super::args::Args;

pub(super) struct ReviewOptions {
    pub(super) ladder: bool,
    pub(super) adjudicate: bool,
    pub(super) fast_nodes: usize,
    pub(super) fast_ms: u64,
    pub(super) slow_nodes: usize,
    pub(super) slow_ms: u64,
    pub(super) diagnostic_potion_max: u32,
    pub(super) action_preview_limit: usize,
    pub(super) replay_focus: bool,
    pub(super) disable_rollout: bool,
    rollout_plugin: Option<CombatSearchRolloutPluginId>,
    turn_plan_plugin: Option<CombatSearchTurnPlanPluginId>,
    expansion_plugin: Option<CombatSearchExpansionPluginId>,
    pub(super) rollout_max_actions: Option<usize>,
    pub(super) rollout_max_evaluations: Option<usize>,
    pub(super) line_lab: bool,
    pub(super) line_lab_ms: u64,
    pub(super) line_lab_cuts: usize,
    pub(super) quality_lanes: bool,
    pub(super) frozen_panel_lanes: bool,
    pub(super) quality_lane_total_nodes: Option<usize>,
    pub(super) quality_lane_total_ms: Option<u64>,
    pub(super) counterfactual_hp_probe: bool,
    pub(super) counterfactual_hp_levels: String,
    pub(super) awakened_opening_probe: bool,
    pub(super) awakened_opening_probe_ms: u64,
    pub(super) awakened_opening_probe_turns: usize,
    pub(super) power_setup_counterfactual: bool,
    pub(super) power_setup_optimistic_only: bool,
    immediate_child_rollout: bool,
    lazy_child_rollout: bool,
}

impl ReviewOptions {
    pub(super) fn from_args(args: &Args) -> Self {
        Self {
            ladder: args.ladder || args.adjudicate,
            adjudicate: args.adjudicate,
            fast_nodes: args.fast_nodes,
            fast_ms: args.fast_ms,
            slow_nodes: args.slow_nodes,
            slow_ms: args.slow_ms,
            diagnostic_potion_max: args.diagnostic_potion_max,
            action_preview_limit: args.action_preview_limit,
            replay_focus: args.replay_focus,
            disable_rollout: args.disable_rollout,
            rollout_plugin: args.rollout_policy,
            turn_plan_plugin: args.turn_plan_policy,
            expansion_plugin: args.expansion_policy,
            rollout_max_actions: args.rollout_max_actions,
            rollout_max_evaluations: args.rollout_max_evaluations,
            line_lab: args.line_lab,
            line_lab_ms: args.line_lab_ms,
            line_lab_cuts: args.line_lab_cuts,
            quality_lanes: args.quality_lanes,
            frozen_panel_lanes: args.frozen_panel_lanes,
            quality_lane_total_nodes: args.quality_lane_total_nodes,
            quality_lane_total_ms: args.quality_lane_total_ms,
            counterfactual_hp_probe: args.counterfactual_hp_probe,
            counterfactual_hp_levels: args.counterfactual_hp_levels.clone(),
            awakened_opening_probe: args.awakened_opening_probe,
            awakened_opening_probe_ms: args.awakened_opening_probe_ms,
            awakened_opening_probe_turns: args.awakened_opening_probe_turns.max(1),
            power_setup_counterfactual: args.power_setup_counterfactual,
            power_setup_optimistic_only: args.power_setup_optimistic_only,
            immediate_child_rollout: args.immediate_child_rollout,
            lazy_child_rollout: args.lazy_child_rollout,
        }
    }

    pub(super) fn child_rollout_plugin(&self) -> CombatSearchChildRolloutPluginId {
        if self.immediate_child_rollout && !self.lazy_child_rollout {
            CombatSearchChildRolloutPluginId::Immediate
        } else {
            CombatSearchChildRolloutPluginId::LazyOnPop
        }
    }

    pub(super) fn rollout_plugin(&self) -> CombatSearchRolloutPluginId {
        if self.disable_rollout {
            CombatSearchRolloutPluginId::Disabled
        } else {
            self.rollout_plugin
                .unwrap_or(CombatSearchRolloutPluginId::EnemyMechanicsAdaptiveNoPotion)
        }
    }

    pub(super) fn turn_plan_plugin(&self) -> CombatSearchTurnPlanPluginId {
        self.turn_plan_plugin
            .unwrap_or(CombatSearchTurnPlanPluginId::Disabled)
    }

    pub(super) fn expansion_plugin(&self) -> CombatSearchExpansionPluginId {
        self.expansion_plugin
            .unwrap_or(CombatSearchExpansionPluginId::AtomicActions)
    }
}
