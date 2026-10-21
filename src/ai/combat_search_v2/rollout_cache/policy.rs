use super::super::value::combat_eval_from_rollout_estimate;
use super::super::*;

pub(super) fn adaptive_no_potion_rollout_plugin(node: &SearchNode) -> CombatSearchRolloutPluginId {
    let profile = combat_search_phase_profile(&node.engine, &node.combat);
    if profile
        .enemy_mechanics
        .finite_survival_damage_mitigation_target_count
        > 0
        || profile.enemy_mechanics.guardian_open_count > 0
        || profile.enemy_mechanics.guardian_defensive_count > 0
        || profile.enemy_mechanics.bronze_automaton_count > 0
        || profile.enemy_mechanics.bronze_orb_count > 0
    {
        CombatSearchRolloutPluginId::PhaseAwareNoPotion
    } else {
        CombatSearchRolloutPluginId::ConservativeNoPotion
    }
}

pub(super) fn better_rollout_estimate(
    left: RolloutNodeEstimate,
    right: RolloutNodeEstimate,
) -> RolloutNodeEstimate {
    let left_eval = combat_eval_from_rollout_estimate(&left);
    let right_eval = combat_eval_from_rollout_estimate(&right);
    if right_eval > left_eval {
        right
    } else {
        left
    }
}
