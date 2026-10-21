use crate::content::potions::PotionId;

use super::RunControlSession;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OraclePotionRescueTierV1 {
    /// A common, deterministic, combat-only tactical effect. This is retained
    /// as an audit classification, not a production admission rule.
    BoundedQuality,
    /// Flexible discovery, out-of-combat recovery, and all other uncommon or
    /// rare active resources need continuation-sensitive review. Quality-gated
    /// refinement may inspect them behind the exact no-potion incumbent, or
    /// use them when no victory has been found. This tier remains diagnostic.
    FindAnyWin,
    /// Passive death insurance and explicit escape are not active victory
    /// actions. Their separate run-control contracts remain authoritative.
    Excluded,
}

/// Returns usable active-victory slots without assigning a context-free value
/// tier. Passive death insurance and explicit escape keep separate contracts.
pub fn oracle_active_victory_potion_slot_mask_v1(session: &RunControlSession) -> u64 {
    let Some(active) = session.active_combat.as_ref() else {
        return 0;
    };
    active
        .combat_state
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref()?;
            if !potion.can_use || !active_victory_potion(potion.id) {
                return None;
            }
            u32::try_from(slot)
                .ok()
                .and_then(|slot| 1_u64.checked_shl(slot))
        })
        .fold(0, |mask, slot| mask | slot)
}

fn active_victory_potion(potion: PotionId) -> bool {
    !matches!(potion, PotionId::FairyPotion | PotionId::SmokeBomb)
}

pub fn oracle_potion_rescue_tier_v1(potion: PotionId) -> OraclePotionRescueTierV1 {
    match potion {
        // Common, deterministic effects whose value is fully realized inside
        // the current combat. This includes direct output and ordinary
        // energy/stat/hand conversion, but not healing or card discovery.
        PotionId::FirePotion
        | PotionId::ExplosivePotion
        | PotionId::PoisonPotion
        | PotionId::WeakenPotion
        | PotionId::FearPotion
        | PotionId::BlockPotion
        | PotionId::EnergyPotion
        | PotionId::StrengthPotion
        | PotionId::DexterityPotion
        | PotionId::SpeedPotion
        | PotionId::SteroidPotion
        | PotionId::SwiftPotion
        | PotionId::FocusPotion
        | PotionId::BottledMiracle
        | PotionId::BlessingOfTheForge => OraclePotionRescueTierV1::BoundedQuality,

        // Blood Potion can be spent outside combat; discovery effects carry
        // broader encounter-specific option value. Uncommon and rare active
        // potions are likewise kept for a genuine no-win emergency rather
        // than exchanged for a marginal improvement to a verified line.
        PotionId::BloodPotion
        | PotionId::AttackPotion
        | PotionId::SkillPotion
        | PotionId::PowerPotion
        | PotionId::ColorlessPotion
        | PotionId::AncientPotion
        | PotionId::RegenPotion
        | PotionId::EssenceOfSteel
        | PotionId::LiquidBronze
        | PotionId::DistilledChaosPotion
        | PotionId::DuplicationPotion
        | PotionId::CunningPotion
        | PotionId::PotionOfCapacity
        | PotionId::LiquidMemories
        | PotionId::GamblersBrew
        | PotionId::Elixir
        | PotionId::StancePotion
        | PotionId::FruitJuice
        | PotionId::EntropicBrew
        | PotionId::SneckoOil
        | PotionId::GhostInAJar
        | PotionId::HeartOfIron
        | PotionId::CultistPotion
        | PotionId::Ambrosia
        | PotionId::EssenceOfDarkness => OraclePotionRescueTierV1::FindAnyWin,

        PotionId::FairyPotion | PotionId::SmokeBomb => OraclePotionRescueTierV1::Excluded,
    }
}
