use super::{
    plan_combat_auto_search_v1, CombatAutoHpLossGateV1, CombatAutoSearchContextV1,
    CombatAutoSearchPlanV1,
};
use crate::ai::combat_search_v2::CombatSearchV2PotionPolicy;

fn boss_context() -> CombatAutoSearchContextV1 {
    CombatAutoSearchContextV1 {
        high_stakes_potion_budget: Some(2),
        has_usable_potion: true,
        command_wall_ms_set: false,
        session_wall_ms_set: false,
        command_potion_policy_set: false,
        session_potion_policy_set: false,
        command_max_potions_used_set: false,
        session_max_potions_used_set: false,
        hp_loss_gate: CombatAutoHpLossGateV1::Limited,
    }
}

#[test]
fn high_stakes_limited_plan_tries_no_potion_then_semantic_then_rescue() {
    let plan = plan_combat_auto_search_v1(&boss_context());

    assert_eq!(
        plan,
        CombatAutoSearchPlanV1 {
            default_wall_ms: Some(5_000),
            requires_explicit_hp_loss_gate: false,
            primary_potion_policy: Some(CombatSearchV2PotionPolicy::SemanticBudgeted),
            primary_max_potions_used: Some(2),
            no_potion_first: true,
            potion_rescue_policy: Some(CombatSearchV2PotionPolicy::All),
            potion_rescue_max_potions_used: Some(2),
        }
    );
}

#[test]
fn high_stakes_without_hp_acceptance_requires_gate() {
    let mut context = boss_context();
    context.hp_loss_gate = CombatAutoHpLossGateV1::Absent;

    let plan = plan_combat_auto_search_v1(&context);

    assert!(plan.requires_explicit_hp_loss_gate);
    assert!(!plan.no_potion_first);
    assert_eq!(plan.potion_rescue_policy, None);
}

#[test]
fn ordinary_limited_plan_keeps_primary_no_potion_but_allows_rescue() {
    let context = CombatAutoSearchContextV1 {
        high_stakes_potion_budget: None,
        has_usable_potion: true,
        command_wall_ms_set: false,
        session_wall_ms_set: false,
        command_potion_policy_set: false,
        session_potion_policy_set: false,
        command_max_potions_used_set: false,
        session_max_potions_used_set: false,
        hp_loss_gate: CombatAutoHpLossGateV1::Limited,
    };

    let plan = plan_combat_auto_search_v1(&context);

    assert_eq!(plan.primary_potion_policy, None);
    assert_eq!(plan.primary_max_potions_used, None);
    assert!(!plan.no_potion_first);
    assert_eq!(
        plan.potion_rescue_policy,
        Some(CombatSearchV2PotionPolicy::All)
    );
    assert_eq!(plan.potion_rescue_max_potions_used, Some(1));
}
