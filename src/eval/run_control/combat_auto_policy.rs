use crate::ai::combat_auto_policy_v1::{
    plan_combat_auto_search_v1, CombatAutoHpLossGateV1, CombatAutoSearchContextV1,
    CombatAutoSearchPlanV1,
};

use super::progress_options::{AtomicCombatSearchOptionsV2, RunControlHpLossLimit};
use super::session::RunControlSession;

pub(super) fn combat_auto_search_plan(
    session: &RunControlSession,
    options: &AtomicCombatSearchOptionsV2,
) -> CombatAutoSearchPlanV1 {
    plan_combat_auto_search_v1(&CombatAutoSearchContextV1 {
        high_stakes_potion_budget: active_combat_high_stakes_potion_budget(session),
        has_usable_potion: active_combat_has_usable_potion(session),
        command_wall_ms_set: options.wall_ms.is_some(),
        session_wall_ms_set: session.search_wall_ms.is_some(),
        command_potion_policy_set: options.potion_policy.is_some(),
        session_potion_policy_set: session.search_potion_policy.is_some(),
        command_max_potions_used_set: options.max_potions_used.is_some(),
        session_max_potions_used_set: session.search_max_potions_used.is_some(),
        hp_loss_gate: combat_auto_hp_loss_gate(session, options),
    })
}

fn active_combat_high_stakes_potion_budget(session: &RunControlSession) -> Option<u32> {
    let combat = &session.active_combat.as_ref()?.combat_state;
    crate::ai::combat_witness_contract::high_stakes_semantic_witness_potion_budget_v1(combat)
}

fn active_combat_has_usable_potion(session: &RunControlSession) -> bool {
    session.active_combat.as_ref().is_some_and(|active| {
        active
            .combat_state
            .entities
            .potions
            .iter()
            .flatten()
            .any(|potion| potion.can_use)
    })
}

fn combat_auto_hp_loss_gate(
    session: &RunControlSession,
    options: &AtomicCombatSearchOptionsV2,
) -> CombatAutoHpLossGateV1 {
    match options.max_hp_loss {
        Some(RunControlHpLossLimit::Limit(_)) => CombatAutoHpLossGateV1::Limited,
        Some(RunControlHpLossLimit::Unlimited) => CombatAutoHpLossGateV1::Unlimited,
        None if session.search_max_hp_loss.is_some() => CombatAutoHpLossGateV1::Limited,
        None => CombatAutoHpLossGateV1::Absent,
    }
}
