use crate::ai::strategy::decision_pipeline::{CleanupTarget, DecisionCandidateKind};
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShopBossPreviewClass {
    BaselineLeave,
    DeterministicBossRepair,
    DeterministicSupport,
    DeterministicCleanup,
    RandomOrDeferred,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShopBossPreviewCandidate {
    pub kind: DecisionCandidateKind,
    pub class: ShopBossPreviewClass,
    pub include_in_v0: bool,
    pub reason: &'static str,
}

pub fn shop_boss_preview_candidates(
    candidates: impl IntoIterator<Item = DecisionCandidateKind>,
) -> Vec<ShopBossPreviewCandidate> {
    let mut preview_candidates = Vec::new();
    for candidate in candidates
        .into_iter()
        .map(classify_shop_boss_preview_candidate)
        .filter(|candidate| candidate.include_in_v0)
    {
        if preview_candidates
            .iter()
            .any(|existing: &ShopBossPreviewCandidate| existing.kind == candidate.kind)
        {
            continue;
        }
        preview_candidates.push(candidate);
    }
    preview_candidates
}

pub fn classify_shop_boss_preview_candidate(
    kind: DecisionCandidateKind,
) -> ShopBossPreviewCandidate {
    let (class, include_in_v0, reason) = match kind {
        DecisionCandidateKind::ShopLeave => {
            (ShopBossPreviewClass::BaselineLeave, true, "BaselineLeave")
        }
        DecisionCandidateKind::ShopPurge { target } => match target {
            CleanupTarget::Curse | CleanupTarget::StarterStrike | CleanupTarget::StarterDefend => (
                ShopBossPreviewClass::DeterministicCleanup,
                true,
                "DeterministicCleanup",
            ),
            _ => (
                ShopBossPreviewClass::Unsupported,
                false,
                "UnsupportedCleanupTarget",
            ),
        },
        DecisionCandidateKind::ShopBuyCard { card, .. } => match card {
            CardId::DemonForm => (
                ShopBossPreviewClass::DeterministicBossRepair,
                true,
                "DeterministicBossScalingRepair",
            ),
            CardId::FiendFire | CardId::Bludgeon | CardId::Immolate | CardId::Reaper => (
                ShopBossPreviewClass::DeterministicBossRepair,
                true,
                "DeterministicBossDamageRepair",
            ),
            CardId::TrueGrit
            | CardId::ShrugItOff
            | CardId::Disarm
            | CardId::Metallicize
            | CardId::FeelNoPain
            | CardId::DarkEmbrace
            | CardId::SecondWind => (
                ShopBossPreviewClass::DeterministicSupport,
                true,
                "DeterministicSupportCard",
            ),
            _ => (ShopBossPreviewClass::Unsupported, false, "NotPreviewV0Card"),
        },
        DecisionCandidateKind::ShopBuyRelic { relic, .. } => match relic {
            RelicId::Vajra | RelicId::Lantern | RelicId::BagOfPreparation | RelicId::Anchor => (
                ShopBossPreviewClass::DeterministicSupport,
                true,
                "DeterministicSupportRelic",
            ),
            RelicId::Orrery => (
                ShopBossPreviewClass::RandomOrDeferred,
                false,
                "RandomRewardRelicExcluded",
            ),
            _ => (
                ShopBossPreviewClass::Unsupported,
                false,
                "NotPreviewV0Relic",
            ),
        },
        DecisionCandidateKind::ShopBuyPotion { potion, .. } => match potion {
            PotionId::FirePotion | PotionId::ExplosivePotion | PotionId::FearPotion => (
                ShopBossPreviewClass::DeterministicBossRepair,
                true,
                "DeterministicBossPotion",
            ),
            PotionId::BlockPotion | PotionId::DexterityPotion | PotionId::SpeedPotion => (
                ShopBossPreviewClass::DeterministicSupport,
                true,
                "DeterministicSupportPotion",
            ),
            PotionId::PowerPotion => (
                ShopBossPreviewClass::RandomOrDeferred,
                true,
                "HighCeilingPowerDiscoveryPotion",
            ),
            PotionId::AttackPotion | PotionId::EntropicBrew | PotionId::GamblersBrew => (
                ShopBossPreviewClass::RandomOrDeferred,
                false,
                "RandomPotionExcluded",
            ),
            _ => (
                ShopBossPreviewClass::Unsupported,
                false,
                "NotPreviewV0Potion",
            ),
        },
        _ => (
            ShopBossPreviewClass::Unsupported,
            false,
            "UnsupportedCandidateKind",
        ),
    };

    ShopBossPreviewCandidate {
        kind,
        class,
        include_in_v0,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use crate::ai::strategy::decision_pipeline::{CleanupTarget, DecisionCandidateKind};
    use crate::content::cards::CardId;
    use crate::content::potions::PotionId;
    use crate::content::relics::RelicId;

    use super::{
        classify_shop_boss_preview_candidate, shop_boss_preview_candidates, ShopBossPreviewClass,
    };

    #[test]
    fn keeps_deterministic_boss_repair_candidates() {
        let candidates = shop_boss_preview_candidates([
            DecisionCandidateKind::ShopLeave,
            DecisionCandidateKind::ShopBuyCard {
                card: CardId::FiendFire,
                upgrades: 0,
                price: 170,
            },
            DecisionCandidateKind::ShopBuyCard {
                card: CardId::Bludgeon,
                upgrades: 0,
                price: 155,
            },
            DecisionCandidateKind::ShopBuyPotion {
                potion: PotionId::FirePotion,
                price: 50,
            },
            DecisionCandidateKind::ShopPurge {
                target: CleanupTarget::StarterStrike,
            },
        ]);

        assert_eq!(candidates.len(), 5);
        assert!(candidates
            .iter()
            .any(|candidate| matches!(candidate.class, ShopBossPreviewClass::BaselineLeave)));
        assert!(candidates.iter().any(|candidate| matches!(
            candidate.kind,
            DecisionCandidateKind::ShopBuyCard {
                card: CardId::FiendFire,
                ..
            }
        )));
        assert!(candidates.iter().any(|candidate| matches!(
            candidate.kind,
            DecisionCandidateKind::ShopBuyPotion {
                potion: PotionId::FirePotion,
                ..
            }
        )));
        assert!(candidates.iter().any(|candidate| matches!(
            candidate.kind,
            DecisionCandidateKind::ShopPurge {
                target: CleanupTarget::StarterStrike
            }
        )));
    }

    #[test]
    fn excludes_random_or_deferred_shop_items_from_v0_preview() {
        let candidates = shop_boss_preview_candidates([
            DecisionCandidateKind::ShopBuyPotion {
                potion: PotionId::AttackPotion,
                price: 50,
            },
            DecisionCandidateKind::ShopBuyPotion {
                potion: PotionId::EntropicBrew,
                price: 70,
            },
            DecisionCandidateKind::ShopBuyRelic {
                relic: RelicId::Orrery,
                price: 164,
            },
            DecisionCandidateKind::ShopBuyRelic {
                relic: RelicId::MawBank,
                price: 150,
            },
        ]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn classifies_support_relics_but_does_not_call_them_hard_answers() {
        let vajra = classify_shop_boss_preview_candidate(DecisionCandidateKind::ShopBuyRelic {
            relic: RelicId::Vajra,
            price: 143,
        });
        let lantern = classify_shop_boss_preview_candidate(DecisionCandidateKind::ShopBuyRelic {
            relic: RelicId::Lantern,
            price: 150,
        });

        assert_eq!(vajra.class, ShopBossPreviewClass::DeterministicSupport);
        assert_eq!(lantern.class, ShopBossPreviewClass::DeterministicSupport);
        assert!(vajra.include_in_v0);
        assert!(lantern.include_in_v0);
    }

    #[test]
    fn deduplicates_repeated_cleanup_targets_for_preview() {
        let candidates = shop_boss_preview_candidates([
            DecisionCandidateKind::ShopLeave,
            DecisionCandidateKind::ShopPurge {
                target: CleanupTarget::StarterStrike,
            },
            DecisionCandidateKind::ShopPurge {
                target: CleanupTarget::StarterStrike,
            },
            DecisionCandidateKind::ShopPurge {
                target: CleanupTarget::StarterDefend,
            },
            DecisionCandidateKind::ShopPurge {
                target: CleanupTarget::StarterDefend,
            },
        ]);

        assert_eq!(
            candidates
                .iter()
                .filter(|candidate| matches!(
                    candidate.kind,
                    DecisionCandidateKind::ShopPurge { .. }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn includes_power_potion_as_high_ceiling_boss_preview_candidate() {
        let power = classify_shop_boss_preview_candidate(DecisionCandidateKind::ShopBuyPotion {
            potion: PotionId::PowerPotion,
            price: 78,
        });
        assert_eq!(power.class, ShopBossPreviewClass::RandomOrDeferred);
        assert!(power.include_in_v0);
        assert_eq!(power.reason, "HighCeilingPowerDiscoveryPotion");
    }

    #[test]
    fn classifies_demon_form_as_deterministic_boss_scaling_repair() {
        let demon_form = classify_shop_boss_preview_candidate(DecisionCandidateKind::ShopBuyCard {
            card: CardId::DemonForm,
            upgrades: 0,
            price: 139,
        });

        assert_eq!(
            demon_form.class,
            ShopBossPreviewClass::DeterministicBossRepair
        );
        assert!(demon_form.include_in_v0);
        assert_eq!(demon_form.reason, "DeterministicBossScalingRepair");
    }
}
