use super::frontier::SearchNode;
use super::phase_profile::combat_search_phase_profile;
use super::transition::terminal_label;
use super::types::{CombatSearchV2PerformanceReport, CombatSearchV2Stats};
use super::value_facts::total_living_enemy_hp;
use super::SearchTerminalLabel;
use super::{
    CombatSearchChildRolloutPluginId, CombatSearchPluginStack, CombatSearchRolloutPluginId,
};

const TURN_BEAM_EVALUATIONS_PER_EXTENSION: usize = 32;
const TURN_BEAM_MAX_EXTENSION_MULTIPLIER: usize = 4;
const DEFERRED_ROLLOUT_MAX_CALL_SHARE_BPS: u64 = 1_500;
const DEFERRED_ROLLOUT_PERIODIC_NODE_INTERVAL: u64 = 256;
const DEFERRED_ROLLOUT_VISIBLE_LETHAL_MARGIN: i32 = 0;
const DEFERRED_ROLLOUT_HARD_NEAR_LETHAL_ENEMY_HP: i32 = 25;
const DEFERRED_ROLLOUT_SOFT_NEAR_LETHAL_ENEMY_HP: i32 = 45;

pub(super) fn turn_beam_extension_budget(max_evaluations: usize, beam_width: usize) -> usize {
    if max_evaluations == 0 {
        return 0;
    }
    let evaluation_budget = max_evaluations.div_ceil(TURN_BEAM_EVALUATIONS_PER_EXTENSION);
    let beam_budget = beam_width
        .max(1)
        .saturating_mul(TURN_BEAM_MAX_EXTENSION_MULTIPLIER);
    evaluation_budget.min(beam_budget).min(max_evaluations)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeferredRolloutAdmission {
    AdmitSignal,
    AdmitPeriodic,
    SkipLowSignal,
    SkipBudgetShare,
}

impl DeferredRolloutAdmission {
    pub(super) fn admitted(self) -> bool {
        matches!(self, Self::AdmitSignal | Self::AdmitPeriodic)
    }
}

pub(super) fn deferred_child_rollout_admission(
    node: &SearchNode,
    plugins: &CombatSearchPluginStack,
    stats: &CombatSearchV2Stats,
    performance: &CombatSearchV2PerformanceReport,
) -> DeferredRolloutAdmission {
    if !deferred_rollout_candidate(node, plugins) {
        return DeferredRolloutAdmission::SkipLowSignal;
    }
    let over_budget = deferred_rollout_call_share_over_budget(performance, stats);
    if deferred_rollout_hard_signal(node) {
        return DeferredRolloutAdmission::AdmitSignal;
    }
    if over_budget {
        return DeferredRolloutAdmission::SkipBudgetShare;
    }
    if deferred_rollout_soft_signal(node) {
        return DeferredRolloutAdmission::AdmitSignal;
    }
    if stats.nodes_expanded > 0
        && stats
            .nodes_expanded
            .is_multiple_of(DEFERRED_ROLLOUT_PERIODIC_NODE_INTERVAL)
    {
        return DeferredRolloutAdmission::AdmitPeriodic;
    }
    DeferredRolloutAdmission::SkipLowSignal
}

fn deferred_rollout_candidate(node: &SearchNode, plugins: &CombatSearchPluginStack) -> bool {
    plugins.child_rollout == CombatSearchChildRolloutPluginId::LazyOnPop
        && plugins.rollout != CombatSearchRolloutPluginId::Disabled
        && !node.rollout_estimate.is_evaluated()
        && terminal_label(&node.engine, &node.combat) == SearchTerminalLabel::Unresolved
}

fn deferred_rollout_hard_signal(node: &SearchNode) -> bool {
    let player = &node.combat.entities.player;
    if player.current_hp * 3 <= player.max_hp {
        return true;
    }
    total_living_enemy_hp(&node.combat) <= DEFERRED_ROLLOUT_HARD_NEAR_LETHAL_ENEMY_HP
}

fn deferred_rollout_soft_signal(node: &SearchNode) -> bool {
    if total_living_enemy_hp(&node.combat) <= DEFERRED_ROLLOUT_SOFT_NEAR_LETHAL_ENEMY_HP {
        return true;
    }
    combat_search_phase_profile(&node.engine, &node.combat)
        .pressure
        .survival_margin
        <= DEFERRED_ROLLOUT_VISIBLE_LETHAL_MARGIN
}

fn deferred_rollout_call_share_over_budget(
    performance: &CombatSearchV2PerformanceReport,
    stats: &CombatSearchV2Stats,
) -> bool {
    // A clock-derived share changes the search frontier even when no deadline
    // expires. Count optional deferred estimates against completed main nodes;
    // the rollout cache's own evaluation cap remains the absolute work bound.
    performance
        .deferred_child_rollout_estimate_calls
        .saturating_mul(10_000)
        > stats
            .nodes_expanded
            .saturating_mul(DEFERRED_ROLLOUT_MAX_CALL_SHARE_BPS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_beam_extension_budget_scales_with_rollout_budget_and_beam_width() {
        assert_eq!(turn_beam_extension_budget(0, 3), 0);
        assert_eq!(turn_beam_extension_budget(1, 3), 1);
        assert_eq!(turn_beam_extension_budget(16, 3), 1);
        assert_eq!(turn_beam_extension_budget(384, 3), 12);
        assert_eq!(turn_beam_extension_budget(384, 8), 12);
        assert_eq!(turn_beam_extension_budget(2048, 3), 12);
        assert_eq!(turn_beam_extension_budget(2048, 8), 32);
    }

    #[test]
    fn deferred_rollout_share_uses_completed_work_not_clock_time() {
        let mut stats = CombatSearchV2Stats {
            nodes_expanded: 7,
            ..CombatSearchV2Stats::default()
        };
        let mut performance = CombatSearchV2PerformanceReport {
            deferred_child_rollout_estimate_calls: 1,
            ..CombatSearchV2PerformanceReport::default()
        };
        assert!(!deferred_rollout_call_share_over_budget(
            &performance,
            &stats
        ));
        performance.deferred_child_rollout_estimate_calls = 2;
        assert!(deferred_rollout_call_share_over_budget(
            &performance,
            &stats
        ));
        stats.nodes_expanded = 14;
        assert!(!deferred_rollout_call_share_over_budget(
            &performance,
            &stats
        ));
    }
}
