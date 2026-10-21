use super::context::PotionPlanningContext;
use super::decision::{PotionGateDecision, PotionGateReason, PotionTacticalRole};
use super::semantics::{potion_semantics, PotionSemanticKind, PotionUncertainty};
use super::*;

mod gates;
use gates::{
    block_gate, direct_damage_gate, enemy_power_gate, max_hp_gate, pressure_gate,
    resource_conversion_gate, wounded_resource_gate,
};

pub(in crate::ai::combat_search_v2) fn semantic_potion_action_allowed(
    combat: &CombatState,
    input: &ClientInput,
) -> bool {
    semantic_potion_gate_decision(combat, input).allowed
}

pub(super) fn semantic_potion_tactical_role(
    combat: &CombatState,
    input: &ClientInput,
) -> Option<PotionTacticalRole> {
    semantic_potion_gate_decision(combat, input).role
}

pub(super) fn semantic_potion_gate_decision(
    combat: &CombatState,
    input: &ClientInput,
) -> PotionGateDecision {
    let ClientInput::UsePotion {
        potion_index,
        target,
    } = input
    else {
        return PotionGateDecision::reject(PotionGateReason::InvalidPotionAction);
    };
    let Some(Some(potion)) = combat.entities.potions.get(*potion_index) else {
        return PotionGateDecision::reject(PotionGateReason::PotionSlotMissing);
    };
    match potion.id {
        crate::content::potions::PotionId::LiquidMemories
            if combat.zones.discard_pile.is_empty() =>
        {
            return PotionGateDecision::reject(PotionGateReason::NoSelectableCards);
        }
        crate::content::potions::PotionId::GamblersBrew
        | crate::content::potions::PotionId::Elixir
            if combat.zones.hand.is_empty() =>
        {
            return PotionGateDecision::reject(PotionGateReason::NoSelectableCards);
        }
        _ => {}
    }

    let semantics = potion_semantics(combat, potion.id);
    if matches!(semantics.uncertainty, PotionUncertainty::PassiveOnly) {
        return PotionGateDecision::reject(PotionGateReason::PassiveOnly);
    }

    let context = PotionPlanningContext::from_combat(combat);
    match semantics.kind {
        PotionSemanticKind::DirectDamage { amount, area } => {
            direct_damage_gate(combat, context, *target, amount, area)
        }
        PotionSemanticKind::EnemyPower => enemy_power_gate(combat, context, *target),
        PotionSemanticKind::PlayerBlock => block_gate(context),
        PotionSemanticKind::PlayerHeal => wounded_resource_gate(context),
        PotionSemanticKind::TemporaryPlayerPower => pressure_gate(context),
        PotionSemanticKind::PlayerMaxHp => max_hp_gate(context),
        PotionSemanticKind::PlayerEnergy
        | PotionSemanticKind::PlayerDraw
        | PotionSemanticKind::CardDiscovery
        | PotionSemanticKind::CardGeneration
        | PotionSemanticKind::HandOrPileSelection
        | PotionSemanticKind::PlayTopCards
        | PotionSemanticKind::UpgradeHand
        | PotionSemanticKind::DuplicateNextCard
        | PotionSemanticKind::Stance => resource_conversion_gate(context),
        PotionSemanticKind::PlayerPower | PotionSemanticKind::Orb => pressure_gate(context),
        PotionSemanticKind::Escape => PotionGateDecision::reject(PotionGateReason::EscapeNotWin),
        PotionSemanticKind::RandomPotionGeneration => {
            PotionGateDecision::reject(PotionGateReason::RandomPotionGenerationUnsupported)
        }
        PotionSemanticKind::PassiveDeathPrevention => {
            PotionGateDecision::reject(PotionGateReason::PassiveOnly)
        }
    }
}
