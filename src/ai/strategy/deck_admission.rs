use crate::ai::analysis::card_semantics::Mechanic;
use crate::ai::strategy::deck_role_inventory::DeckRoleInventory;
use crate::ai::strategy::reward_admission::{
    RewardAdmission, RewardAdmissionClass, RewardAdmissionReason,
};
use crate::runtime::combat::CombatCard;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeckAdmission {
    Welcome,
    Conditional,
    Discouraged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeckAdmissionContext {
    pub act: u8,
    pub current_hp: i32,
    pub max_hp: i32,
}

impl DeckAdmissionContext {
    pub fn survival_pressure(self) -> bool {
        let max_hp = self.max_hp.max(1);
        self.current_hp.saturating_mul(2) <= max_hp
            || (self.act >= 2 && self.current_hp.saturating_mul(3) <= max_hp.saturating_mul(2))
    }
}

pub fn assess_deck_admission(
    deck: &[CombatCard],
    context: DeckAdmissionContext,
    admission: &RewardAdmission,
) -> DeckAdmission {
    let inventory = DeckRoleInventory::from_deck(deck);
    assess_deck_admission_from_inventory(deck.len(), context, &inventory, admission)
}

pub fn assess_deck_admission_from_inventory(
    deck_size: usize,
    context: DeckAdmissionContext,
    inventory: &DeckRoleInventory,
    admission: &RewardAdmission,
) -> DeckAdmission {
    if admission.card.is_none() || always_structural(admission) {
        return DeckAdmission::Welcome;
    }
    if access_still_needed(inventory, admission) {
        return DeckAdmission::Welcome;
    }
    if admission.class == RewardAdmissionClass::BuildsSupportedPackage {
        return DeckAdmission::Welcome;
    }
    if let Some(admission) = repeated_role_admission(deck_size, context, inventory, admission) {
        return admission;
    }
    if context.survival_pressure() && survival_relevant(admission) {
        return DeckAdmission::Welcome;
    }
    let (soft_limit, hard_limit) = deck_size_limits(context);
    if deck_size >= hard_limit && ordinary_addition(admission) {
        return DeckAdmission::Discouraged;
    }
    if deck_size >= soft_limit && ordinary_addition(admission) {
        return DeckAdmission::Conditional;
    }
    DeckAdmission::Welcome
}

fn deck_size_limits(context: DeckAdmissionContext) -> (usize, usize) {
    if context.act >= 2 {
        (24, 30)
    } else {
        (28, 34)
    }
}

fn always_structural(admission: &RewardAdmission) -> bool {
    admission.class == RewardAdmissionClass::ClosesRequirement
        || admission
            .reasons
            .iter()
            .any(|reason| matches!(reason, RewardAdmissionReason::Installs(_)))
}

fn survival_relevant(admission: &RewardAdmission) -> bool {
    admission.reasons.iter().any(|reason| {
        matches!(
            reason,
            RewardAdmissionReason::Provides(
                Mechanic::Block
                    | Mechanic::Weak
                    | Mechanic::EnemyStrengthDown
                    | Mechanic::TemporaryEnemyStrengthDown
            )
        )
    })
}

fn ordinary_addition(admission: &RewardAdmission) -> bool {
    matches!(
        admission.class,
        RewardAdmissionClass::ImmediateWork | RewardAdmissionClass::BurdenedImmediateWork
    )
}

fn access_still_needed(inventory: &DeckRoleInventory, admission: &RewardAdmission) -> bool {
    admission_provides(admission, Mechanic::CardDraw) && inventory.draw_units < 2
        || admission_provides(admission, Mechanic::Energy) && inventory.energy_units < 1
}

fn repeated_role_admission(
    deck_size: usize,
    context: DeckAdmissionContext,
    inventory: &DeckRoleInventory,
    admission: &RewardAdmission,
) -> Option<DeckAdmission> {
    if context.act < 2 {
        return None;
    }
    let level = if deck_size >= 30 {
        DeckAdmission::Discouraged
    } else {
        DeckAdmission::Conditional
    };
    if admission_has_combat_upgrade(admission) && inventory.upgrade_access_units >= 1 {
        return Some(level);
    }
    if admission_damage_uses(admission, Mechanic::Block) && inventory.block_payoff_units >= 1 {
        return Some(level);
    }
    if admission_damage_uses(admission, Mechanic::Strength)
        && !admission_recovers_current_hp(admission)
        && inventory.strength_payoff_units >= 2
    {
        return Some(level);
    }
    if admission_provides(admission, Mechanic::Block)
        && !admission_provides(admission, Mechanic::CardDraw)
        && inventory.block_units >= 5
    {
        return Some(level);
    }
    if admission_frontloads(admission) && inventory.frontload_units >= 5 {
        return Some(level);
    }
    if admission_aoe(admission) && inventory.aoe_units >= 2 {
        return Some(level);
    }
    if admission_debuffs(admission)
        && (inventory.mitigation_units >= 2 || inventory.debuff_units >= 3)
    {
        return Some(level);
    }
    if admission_emits_exhaust(admission)
        && !admission_provides(admission, Mechanic::CardDraw)
        && inventory.exhaust_stream_units >= 3
    {
        return Some(level);
    }
    None
}

fn admission_provides(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::Provides(mechanic))
}

fn admission_frontloads(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::FrontloadDamage)
}

fn admission_aoe(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::AreaDamage)
}

fn admission_debuffs(admission: &RewardAdmission) -> bool {
    admission_provides(admission, Mechanic::Weak)
        || admission_provides(admission, Mechanic::Vulnerable)
        || admission_provides(admission, Mechanic::EnemyStrengthDown)
        || admission_provides(admission, Mechanic::TemporaryEnemyStrengthDown)
}

fn admission_emits_exhaust(admission: &RewardAdmission) -> bool {
    admission.reasons.iter().any(|reason| {
        matches!(
            reason,
            RewardAdmissionReason::Emits(
                crate::ai::analysis::card_semantics::CombatEvent::CardExhausted
            )
        )
    })
}

fn admission_damage_uses(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::DamageUses(mechanic))
}

fn admission_has_combat_upgrade(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::CombatUpgrade)
}

fn admission_recovers_current_hp(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::RecoverCurrentHp)
}
