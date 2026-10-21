use super::super::*;

const TURN_PLAN_SEED_CRITICAL_SURVIVAL_MARGIN: i32 = 6;

pub(super) fn should_seed_turn_plan_at_node(
    node: &SearchNode,
    config: &CombatSearchV2Config,
    plugins: &CombatSearchPluginStack,
) -> bool {
    if plugins.expansion.owns_turn_boundaries()
        || !plugins.turn_plan.seeds_turn_boundary_frontier()
        || !matches!(node.engine, EngineState::CombatPlayerTurn)
        || node.turn_prefix.prefix_length() != 0
        || terminal_label(&node.engine, &node.combat) != SearchTerminalLabel::Unresolved
    {
        return false;
    }

    if turn_plan_prior_has_current_state(node, config) {
        return true;
    }

    if plugins.turn_plan.requires_tactical_enemy_gate() {
        return tactical_enemy_turn_plan_seed_gate(node);
    }

    true
}

fn turn_plan_prior_has_current_state(node: &SearchNode, config: &CombatSearchV2Config) -> bool {
    let Some(prior) = config
        .turn_plan_prior
        .as_ref()
        .filter(|prior| !prior.is_empty())
    else {
        return false;
    };
    let state_hash = combat_exact_state_hash_v2(&node.engine, &node.combat);
    prior.has_hints_for_state(&state_hash)
}

pub(super) fn tactical_enemy_turn_plan_seed_gate(node: &SearchNode) -> bool {
    if node.combat.meta.is_boss_fight || node.combat.meta.is_elite_fight {
        return true;
    }

    if visible_high_pressure_turn_plan_seed_gate(&node.combat) {
        return true;
    }

    let profile = combat_search_phase_profile(&node.engine, &node.combat);
    (profile.enemy_mechanics.healer_support_count > 0 && living_enemy_count(&node.combat) >= 2)
        || profile.enemy_mechanics.fungi_beast_count >= 3
}

fn visible_high_pressure_turn_plan_seed_gate(combat: &CombatState) -> bool {
    let incoming = visible_incoming_damage(combat);
    if incoming <= 0 {
        return false;
    }
    let survival_margin = combat
        .entities
        .player
        .current_hp
        .saturating_add(combat.entities.player.block)
        .saturating_sub(incoming);
    survival_margin <= TURN_PLAN_SEED_CRITICAL_SURVIVAL_MARGIN
}
