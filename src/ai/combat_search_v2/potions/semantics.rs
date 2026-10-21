use super::*;
use crate::content::potions::PotionId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PotionSemantics {
    pub(super) kind: PotionSemanticKind,
    pub(super) uncertainty: PotionUncertainty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PotionSemanticKind {
    DirectDamage { amount: i32, area: PotionArea },
    EnemyPower,
    PlayerBlock,
    PlayerHeal,
    PlayerMaxHp,
    PlayerEnergy,
    PlayerDraw,
    PlayerPower,
    TemporaryPlayerPower,
    CardDiscovery,
    CardGeneration,
    HandOrPileSelection,
    PlayTopCards,
    UpgradeHand,
    DuplicateNextCard,
    Escape,
    RandomPotionGeneration,
    PassiveDeathPrevention,
    Orb,
    Stance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PotionArea {
    SingleEnemy,
    AllEnemies,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PotionUncertainty {
    Deterministic,
    RandomOutcome,
    PlayerChoice,
    PassiveOnly,
}

pub(super) fn potion_semantics(combat: &CombatState, id: PotionId) -> PotionSemantics {
    use PotionId::*;
    let amount = potion_potency(combat, id);
    match id {
        FirePotion => PotionSemantics {
            kind: PotionSemanticKind::DirectDamage {
                amount,
                area: PotionArea::SingleEnemy,
            },
            uncertainty: PotionUncertainty::Deterministic,
        },
        ExplosivePotion => PotionSemantics {
            kind: PotionSemanticKind::DirectDamage {
                amount,
                area: PotionArea::AllEnemies,
            },
            uncertainty: PotionUncertainty::Deterministic,
        },
        PoisonPotion | WeakenPotion | FearPotion => PotionSemantics {
            kind: PotionSemanticKind::EnemyPower,
            uncertainty: PotionUncertainty::Deterministic,
        },
        BlockPotion | GhostInAJar => PotionSemantics {
            kind: PotionSemanticKind::PlayerBlock,
            uncertainty: PotionUncertainty::Deterministic,
        },
        BloodPotion | RegenPotion => PotionSemantics {
            kind: PotionSemanticKind::PlayerHeal,
            uncertainty: PotionUncertainty::Deterministic,
        },
        FruitJuice => PotionSemantics {
            kind: PotionSemanticKind::PlayerMaxHp,
            uncertainty: PotionUncertainty::Deterministic,
        },
        EnergyPotion => PotionSemantics {
            kind: PotionSemanticKind::PlayerEnergy,
            uncertainty: PotionUncertainty::Deterministic,
        },
        SwiftPotion | SneckoOil => PotionSemantics {
            kind: PotionSemanticKind::PlayerDraw,
            uncertainty: if id == SneckoOil {
                PotionUncertainty::RandomOutcome
            } else {
                PotionUncertainty::Deterministic
            },
        },
        StrengthPotion | DexterityPotion | FocusPotion | AncientPotion | EssenceOfSteel
        | LiquidBronze | PotionOfCapacity | HeartOfIron | CultistPotion => PotionSemantics {
            kind: PotionSemanticKind::PlayerPower,
            uncertainty: PotionUncertainty::Deterministic,
        },
        SpeedPotion | SteroidPotion => PotionSemantics {
            kind: PotionSemanticKind::TemporaryPlayerPower,
            uncertainty: PotionUncertainty::Deterministic,
        },
        AttackPotion | SkillPotion | PowerPotion | ColorlessPotion => PotionSemantics {
            kind: PotionSemanticKind::CardDiscovery,
            uncertainty: PotionUncertainty::PlayerChoice,
        },
        BottledMiracle | CunningPotion => PotionSemantics {
            kind: PotionSemanticKind::CardGeneration,
            uncertainty: PotionUncertainty::Deterministic,
        },
        LiquidMemories | GamblersBrew | Elixir => PotionSemantics {
            kind: PotionSemanticKind::HandOrPileSelection,
            uncertainty: PotionUncertainty::PlayerChoice,
        },
        DistilledChaosPotion => PotionSemantics {
            kind: PotionSemanticKind::PlayTopCards,
            uncertainty: PotionUncertainty::RandomOutcome,
        },
        BlessingOfTheForge => PotionSemantics {
            kind: PotionSemanticKind::UpgradeHand,
            uncertainty: PotionUncertainty::Deterministic,
        },
        DuplicationPotion => PotionSemantics {
            kind: PotionSemanticKind::DuplicateNextCard,
            uncertainty: PotionUncertainty::Deterministic,
        },
        SmokeBomb => PotionSemantics {
            kind: PotionSemanticKind::Escape,
            uncertainty: PotionUncertainty::Deterministic,
        },
        EntropicBrew => PotionSemantics {
            kind: PotionSemanticKind::RandomPotionGeneration,
            uncertainty: PotionUncertainty::RandomOutcome,
        },
        FairyPotion => PotionSemantics {
            kind: PotionSemanticKind::PassiveDeathPrevention,
            uncertainty: PotionUncertainty::PassiveOnly,
        },
        EssenceOfDarkness => PotionSemantics {
            kind: PotionSemanticKind::Orb,
            uncertainty: PotionUncertainty::Deterministic,
        },
        StancePotion | Ambrosia => PotionSemantics {
            kind: PotionSemanticKind::Stance,
            uncertainty: if id == StancePotion {
                PotionUncertainty::PlayerChoice
            } else {
                PotionUncertainty::Deterministic
            },
        },
    }
}

fn potion_potency(combat: &CombatState, id: PotionId) -> i32 {
    let mut potency = crate::content::potions::get_potion_definition(id).base_potency;
    if combat
        .entities
        .player
        .has_relic(crate::content::relics::RelicId::SacredBark)
    {
        potency *= 2;
    }
    potency
}
