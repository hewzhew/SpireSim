use crate::ai::combat_search_v2::CombatSearchV2PotionPolicy;

use super::types::{
    CombatAutoSearchContextV1, CombatAutoSearchPlanV1, DEFAULT_COMBAT_AUTO_SEARCH_WALL_MS,
};

pub fn plan_combat_auto_search_v1(context: &CombatAutoSearchContextV1) -> CombatAutoSearchPlanV1 {
    let default_wall_ms = if context.command_wall_ms_set || context.session_wall_ms_set {
        None
    } else {
        Some(DEFAULT_COMBAT_AUTO_SEARCH_WALL_MS)
    };
    let primary_potion_policy =
        if context.high_stakes_potion_budget.is_some() && !context.has_potion_policy_override() {
            Some(CombatSearchV2PotionPolicy::SemanticBudgeted)
        } else {
            None
        };
    let primary_max_potions_used =
        if primary_potion_policy.is_some() && !context.has_max_potions_override() {
            context.high_stakes_potion_budget
        } else {
            None
        };
    let no_potion_first = context.high_stakes_potion_budget.is_some()
        && !context.has_potion_policy_override()
        && context.hp_loss_gate.is_limited();

    let allow_rescue = context.hp_loss_gate.is_limited()
        && !context.has_potion_policy_override()
        && !context.has_max_potions_override()
        && context.has_usable_potion;
    let potion_rescue_policy = allow_rescue.then_some(CombatSearchV2PotionPolicy::All);
    let potion_rescue_max_potions_used = if allow_rescue {
        Some(context.high_stakes_potion_budget.unwrap_or(1))
    } else {
        None
    };

    CombatAutoSearchPlanV1 {
        default_wall_ms,
        requires_explicit_hp_loss_gate: context.high_stakes_potion_budget.is_some()
            && !context.hp_loss_gate.is_explicit_acceptance(),
        primary_potion_policy,
        primary_max_potions_used,
        no_potion_first,
        potion_rescue_policy,
        potion_rescue_max_potions_used,
    }
}
