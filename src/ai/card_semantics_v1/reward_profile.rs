use crate::content::cards::CardId;
use crate::state::rewards::RewardCard;

use super::{
    card_mechanics_profile_v1, card_reward_facts_v1, CardRewardPickDependencyV1,
    CardRewardSemanticProfileV1, CardRewardSemanticRoleV1, CombatExternalPayoffV1,
};
use crate::ai::combat_upgrade_coverage_v1::combat_upgrade_scope_for_card_v1;

pub fn card_reward_semantic_profile_v1(card: &RewardCard) -> CardRewardSemanticProfileV1 {
    let facts = card_reward_facts_v1(card);
    let mechanics = card_mechanics_profile_v1(facts.card);
    let mut roles = Vec::new();

    if facts
        .damage
        .unconditional_base_total()
        .is_some_and(|amount| amount > 0)
    {
        push_role(&mut roles, CardRewardSemanticRoleV1::FrontloadDamage);
    }
    if facts
        .damage
        .delivery
        .is_some_and(|rule| rule.has_context_dependent_attack())
    {
        push_role(&mut roles, CardRewardSemanticRoleV1::ContextDependentDamage);
    }
    if facts.is_aoe {
        push_role(&mut roles, CardRewardSemanticRoleV1::AoeDamage);
    }
    if facts.block > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::Block);
    }
    match facts.card {
        CardId::Barricade => push_role(&mut roles, CardRewardSemanticRoleV1::BlockRetention),
        CardId::Entrench => push_role(&mut roles, CardRewardSemanticRoleV1::BlockMultiplier),
        _ => {}
    }
    // Warcry also returns one card from hand to the draw pile.  Count only
    // net-positive hand access as draw; the base card remains topdeck control.
    if facts.draw_cards > i32::from(mechanics.hand_topdeck_selection)
        || mechanics.recurring_card_draw
    {
        push_role(&mut roles, CardRewardSemanticRoleV1::CardDraw);
    }
    if mechanics.reshuffle_discard_into_draw {
        push_role(&mut roles, CardRewardSemanticRoleV1::CycleAccess);
    }
    if mechanics.discard_pile_topdeck_access {
        push_role(
            &mut roles,
            CardRewardSemanticRoleV1::DiscardPileTopdeckAccess,
        );
    }
    if mechanics.hand_topdeck_selection {
        push_role(&mut roles, CardRewardSemanticRoleV1::HandTopdeckSelection);
    }
    if facts.damage_growth_on_play.is_some_and(|amount| amount > 0) {
        push_role(
            &mut roles,
            CardRewardSemanticRoleV1::CardInstanceDamageGrowth,
        );
    }
    if facts
        .cost_change_on_hp_loss
        .is_some_and(|amount| amount < 0)
    {
        push_role(&mut roles, CardRewardSemanticRoleV1::HpLossCostReduction);
    }
    if facts.target_vulnerable_follow_up.is_some() {
        push_role(&mut roles, CardRewardSemanticRoleV1::ConditionalCardAccess);
    }
    if facts.hand_copy.is_some() {
        push_role(&mut roles, CardRewardSemanticRoleV1::HandCopy);
    }
    if facts.top_deck_play.is_some() {
        push_role(&mut roles, CardRewardSemanticRoleV1::TopDeckPlay);
    }
    if facts.energy_gain > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::EnergySource);
    }
    if facts.energy_gain_on_exhaust > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::EnergyOnExhaust);
    }
    if facts.vulnerable > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::Vulnerable);
    }
    if facts.weak > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::Weak);
    }
    if facts.enemy_strength_down > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::EnemyStrengthDown);
    }
    if mechanics.temporary_strength_burst && facts.strength_gain > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::TemporaryStrengthBurst);
    } else if facts.strength_gain > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::ScalingSource);
    }
    if matches!(
        mechanics.combat_external_payoff,
        Some(CombatExternalPayoffV1::PersistentOrReward)
    ) {
        push_role(&mut roles, CardRewardSemanticRoleV1::CombatExternalPayoff);
    }
    if matches!(
        mechanics.combat_external_payoff,
        Some(CombatExternalPayoffV1::HealingIfDamaged)
    ) {
        push_role(&mut roles, CardRewardSemanticRoleV1::CombatSustain);
    }
    if mechanics.action_supply.additional_play_scope.is_some() {
        push_role(&mut roles, CardRewardSemanticRoleV1::ActionAmplifier);
        push_role(&mut roles, CardRewardSemanticRoleV1::PackagePayoff);
    }
    if facts.exhausts_other_cards || facts.card == CardId::Corruption {
        push_role(&mut roles, CardRewardSemanticRoleV1::ExhaustGenerator);
    }
    if facts.card == CardId::Exhume {
        push_role(&mut roles, CardRewardSemanticRoleV1::ExhaustReuse);
    }
    if facts.adds_status_cards > 0 {
        push_role(&mut roles, CardRewardSemanticRoleV1::StatusGenerator);
    }
    if facts.is_random_output {
        push_role(&mut roles, CardRewardSemanticRoleV1::RandomOutput);
    }
    if facts.has_conditional_playability {
        push_role(&mut roles, CardRewardSemanticRoleV1::ConditionalPlayability);
    }

    for dependency in &facts.pick_dependencies {
        match dependency {
            CardRewardPickDependencyV1::StrengthScaling => {
                push_role(&mut roles, CardRewardSemanticRoleV1::StrengthPayoff);
                push_role(&mut roles, CardRewardSemanticRoleV1::PackagePayoff);
            }
            CardRewardPickDependencyV1::BlockDensity => {
                push_role(&mut roles, CardRewardSemanticRoleV1::BlockPayoff);
                push_role(&mut roles, CardRewardSemanticRoleV1::PackagePayoff);
            }
            CardRewardPickDependencyV1::StrikeDensity => {
                push_role(&mut roles, CardRewardSemanticRoleV1::StrikePayoff);
                push_role(&mut roles, CardRewardSemanticRoleV1::PackagePayoff);
            }
            CardRewardPickDependencyV1::RouteUpgradeDensity => {
                push_role(&mut roles, CardRewardSemanticRoleV1::UpgradePayoff);
                push_role(&mut roles, CardRewardSemanticRoleV1::PackagePayoff);
            }
            CardRewardPickDependencyV1::ExhaustPackage => {
                push_role(&mut roles, CardRewardSemanticRoleV1::ExhaustPayoff);
                push_role(&mut roles, CardRewardSemanticRoleV1::PackagePayoff);
            }
            CardRewardPickDependencyV1::StatusPackage => {
                push_role(&mut roles, CardRewardSemanticRoleV1::StatusPayoff);
                push_role(&mut roles, CardRewardSemanticRoleV1::PackagePayoff);
            }
            CardRewardPickDependencyV1::SelfDamagePackage => {
                push_role(&mut roles, CardRewardSemanticRoleV1::SelfDamagePayoff);
                push_role(&mut roles, CardRewardSemanticRoleV1::PackagePayoff);
            }
            CardRewardPickDependencyV1::RandomOutputPolicy
            | CardRewardPickDependencyV1::ConditionalPlayabilityPolicy => {}
        }
    }

    roles.sort();
    roles.dedup();

    CardRewardSemanticProfileV1 {
        card: facts.card,
        name: facts.name,
        roles,
        combat_upgrade_scope: combat_upgrade_scope_for_card_v1(facts.card, facts.upgrades),
        dependencies: facts.pick_dependencies,
    }
}

fn push_role(roles: &mut Vec<CardRewardSemanticRoleV1>, role: CardRewardSemanticRoleV1) {
    if !roles.contains(&role) {
        roles.push(role);
    }
}
