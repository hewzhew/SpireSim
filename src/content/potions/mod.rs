pub mod mechanics;
pub mod potion_effects;

use serde::{Deserialize, Serialize};

crate::stable_gameplay_id! {
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PotionId {
    // Common (20)
    FirePotion => ("FirePotion", 0),
    ExplosivePotion => ("ExplosivePotion", 1),
    PoisonPotion => ("PoisonPotion", 2),
    WeakenPotion => ("WeakenPotion", 3),
    FearPotion => ("FearPotion", 4),
    BlockPotion => ("BlockPotion", 5),
    BloodPotion => ("BloodPotion", 6),
    EnergyPotion => ("EnergyPotion", 7),
    StrengthPotion => ("StrengthPotion", 8),
    DexterityPotion => ("DexterityPotion", 9),
    SpeedPotion => ("SpeedPotion", 10),
    SteroidPotion => ("SteroidPotion", 11),
    SwiftPotion => ("SwiftPotion", 12),
    FocusPotion => ("FocusPotion", 13), // Defect
    AttackPotion => ("AttackPotion", 14),
    SkillPotion => ("SkillPotion", 15),
    PowerPotion => ("PowerPotion", 16),
    ColorlessPotion => ("ColorlessPotion", 17),
    BottledMiracle => ("BottledMiracle", 18), // Watcher
    BlessingOfTheForge => ("BlessingOfTheForge", 19),

    // Uncommon (12)
    AncientPotion => ("AncientPotion", 20),
    RegenPotion => ("RegenPotion", 21),
    EssenceOfSteel => ("EssenceOfSteel", 22),
    LiquidBronze => ("LiquidBronze", 23),
    DistilledChaosPotion => ("DistilledChaosPotion", 24),
    DuplicationPotion => ("DuplicationPotion", 25),
    CunningPotion => ("CunningPotion", 26),    // Silent
    PotionOfCapacity => ("PotionOfCapacity", 27), // Defect
    LiquidMemories => ("LiquidMemories", 28),
    GamblersBrew => ("GamblersBrew", 29),
    Elixir => ("Elixir", 30),
    StancePotion => ("StancePotion", 31), // Watcher

    // Rare (10)
    FairyPotion => ("FairyPotion", 32),
    SmokeBomb => ("SmokeBomb", 33),
    FruitJuice => ("FruitJuice", 34),
    EntropicBrew => ("EntropicBrew", 35),
    SneckoOil => ("SneckoOil", 36),
    GhostInAJar => ("GhostInAJar", 37),
    HeartOfIron => ("HeartOfIron", 38),
    CultistPotion => ("CultistPotion", 39),
    Ambrosia => ("Ambrosia", 40),          // Watcher
    EssenceOfDarkness => ("EssenceOfDarkness", 41), // Defect
}
}

impl PotionId {
    /// Human-authored start-spec spellings whose normalized form differs from
    /// the simulator-owned stable ID. Ordinary whitespace and punctuation
    /// variants of a stable ID are accepted without being duplicated here.
    const fn human_input_aliases_v1(self) -> &'static [&'static str] {
        match self {
            Self::DistilledChaosPotion => &["DistilledChaos"],
            Self::Elixir => &["ElixirPotion"],
            Self::WeakenPotion => &["Weak Potion"],
            _ => &[],
        }
    }

    /// Permissive parser for human-authored boundaries such as combat fixtures.
    /// Durable serde and exact protocol ingestion use separate exact owners.
    pub fn from_input_alias_v1(raw: &str) -> Option<Self> {
        crate::content::identity::gameplay_identity_from_input_alias_v1(raw)
    }
}

impl crate::content::identity::GameplayIdentitySurface for PotionId {
    fn display_name_v1(self) -> Option<&'static str> {
        Some(get_potion_definition(self).name)
    }

    fn input_aliases_v1(self) -> Vec<&'static str> {
        self.human_input_aliases_v1().to_vec()
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub enum PotionRarity {
    Common,
    Uncommon,
    Rare,
}

/// Which player class can obtain this potion (None = any class)
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub enum PotionClass {
    Any,
    Ironclad,
    Silent,
    Defect,
    Watcher,
}

/// Public, composable combat mechanics shared by potion identities.
///
/// A learned policy should not have to rediscover from an identity embedding
/// that Fire Potion and Explosive Potion both deal direct damage, or that
/// Speed Potion and Dexterity Potion both grant Dexterity.  These roles are a
/// compact projection of the typed actions in `potion_effects`; a potion may
/// expose more than one role when its effect is genuinely composite.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash, Serialize)]
#[repr(u8)]
pub enum PotionMechanicRole {
    DirectDamage = 0,
    MultiTarget = 1,
    ApplyPoison = 2,
    ApplyWeak = 3,
    ApplyVulnerable = 4,
    GainBlock = 5,
    Heal = 6,
    GainEnergy = 7,
    GainStrength = 8,
    GainDexterity = 9,
    Temporary = 10,
    DrawCards = 11,
    GainFocus = 12,
    DiscoverAttack = 13,
    DiscoverSkill = 14,
    DiscoverPower = 15,
    DiscoverColorless = 16,
    AddCardToHand = 17,
    UpgradeHand = 18,
    GainArtifact = 19,
    GainRegeneration = 20,
    GainPlatedArmor = 21,
    GainThorns = 22,
    PlayTopCards = 23,
    DuplicateCards = 24,
    GainOrbSlots = 25,
    RetrieveFromDiscard = 26,
    DiscardAndDraw = 27,
    ExhaustCards = 28,
    ChooseStance = 29,
    PreventDeath = 30,
    EscapeCombat = 31,
    GainMaxHp = 32,
    GeneratePotions = 33,
    RandomizeHandCosts = 34,
    GainIntangible = 35,
    GainMetallicize = 36,
    GainRitual = 37,
    EnterDivinity = 38,
    ChannelDark = 39,
}

pub const POTION_MECHANIC_ROLE_COUNT: u64 = PotionMechanicRole::ChannelDark as u64 + 1;

/// Returns the identity-independent mechanical roles of a potion.
pub fn potion_mechanic_roles(id: PotionId) -> &'static [PotionMechanicRole] {
    use PotionMechanicRole as Role;

    match id {
        PotionId::FirePotion => &[Role::DirectDamage],
        PotionId::ExplosivePotion => &[Role::DirectDamage, Role::MultiTarget],
        PotionId::PoisonPotion => &[Role::ApplyPoison],
        PotionId::WeakenPotion => &[Role::ApplyWeak],
        PotionId::FearPotion => &[Role::ApplyVulnerable],
        PotionId::BlockPotion => &[Role::GainBlock],
        PotionId::BloodPotion => &[Role::Heal],
        PotionId::EnergyPotion => &[Role::GainEnergy],
        PotionId::StrengthPotion => &[Role::GainStrength],
        PotionId::DexterityPotion => &[Role::GainDexterity],
        PotionId::SpeedPotion => &[Role::GainDexterity, Role::Temporary],
        PotionId::SteroidPotion => &[Role::GainStrength, Role::Temporary],
        PotionId::SwiftPotion => &[Role::DrawCards],
        PotionId::FocusPotion => &[Role::GainFocus],
        PotionId::AttackPotion => &[Role::DiscoverAttack],
        PotionId::SkillPotion => &[Role::DiscoverSkill],
        PotionId::PowerPotion => &[Role::DiscoverPower],
        PotionId::ColorlessPotion => &[Role::DiscoverColorless],
        PotionId::BottledMiracle => &[Role::AddCardToHand],
        PotionId::BlessingOfTheForge => &[Role::UpgradeHand],
        PotionId::AncientPotion => &[Role::GainArtifact],
        PotionId::RegenPotion => &[Role::GainRegeneration],
        PotionId::EssenceOfSteel => &[Role::GainPlatedArmor],
        PotionId::LiquidBronze => &[Role::GainThorns],
        PotionId::DistilledChaosPotion => &[Role::PlayTopCards],
        PotionId::DuplicationPotion => &[Role::DuplicateCards],
        PotionId::CunningPotion => &[Role::AddCardToHand],
        PotionId::PotionOfCapacity => &[Role::GainOrbSlots],
        PotionId::LiquidMemories => &[Role::RetrieveFromDiscard],
        PotionId::GamblersBrew => &[Role::DiscardAndDraw],
        PotionId::Elixir => &[Role::ExhaustCards],
        PotionId::StancePotion => &[Role::ChooseStance],
        PotionId::FairyPotion => &[Role::PreventDeath, Role::Heal],
        PotionId::SmokeBomb => &[Role::EscapeCombat],
        PotionId::FruitJuice => &[Role::GainMaxHp],
        PotionId::EntropicBrew => &[Role::GeneratePotions],
        PotionId::SneckoOil => &[Role::DrawCards, Role::RandomizeHandCosts],
        PotionId::GhostInAJar => &[Role::GainIntangible],
        PotionId::HeartOfIron => &[Role::GainMetallicize],
        PotionId::CultistPotion => &[Role::GainRitual],
        PotionId::Ambrosia => &[Role::EnterDivinity],
        PotionId::EssenceOfDarkness => &[Role::ChannelDark],
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PotionDefinition {
    pub id: PotionId,
    pub name: &'static str,
    pub rarity: PotionRarity,
    pub base_potency: i32,
    pub target_required: bool,
    pub is_thrown: bool,
    pub class: PotionClass,
}

/// Effective request potency; shared by combat execution and observations.
/// Target hooks, healing caps and other effect-specific settlement come later.
pub fn potion_potency_in_combat(id: PotionId, state: &crate::runtime::combat::CombatState) -> i32 {
    let multiplier = if state
        .entities
        .player
        .has_relic(crate::content::relics::RelicId::SacredBark)
    {
        2
    } else {
        1
    };
    get_potion_definition(id).base_potency * multiplier
}

pub fn get_potion_definition(id: PotionId) -> PotionDefinition {
    match id {
        // ---- Common ----
        PotionId::FirePotion => PotionDefinition {
            id,
            name: "Fire Potion",
            rarity: PotionRarity::Common,
            base_potency: 20,
            target_required: true,
            is_thrown: true,
            class: PotionClass::Any,
        },
        PotionId::ExplosivePotion => PotionDefinition {
            id,
            name: "Explosive Potion",
            rarity: PotionRarity::Common,
            base_potency: 10,
            target_required: false,
            is_thrown: true,
            class: PotionClass::Any,
        },
        PotionId::PoisonPotion => PotionDefinition {
            id,
            name: "Poison Potion",
            rarity: PotionRarity::Common,
            base_potency: 6,
            target_required: true,
            is_thrown: true,
            class: PotionClass::Silent,
        },
        PotionId::WeakenPotion => PotionDefinition {
            id,
            name: "Weak Potion",
            rarity: PotionRarity::Common,
            base_potency: 3,
            target_required: true,
            is_thrown: true,
            class: PotionClass::Any,
        },
        PotionId::FearPotion => PotionDefinition {
            id,
            name: "Fear Potion",
            rarity: PotionRarity::Common,
            base_potency: 3,
            target_required: true,
            is_thrown: true,
            class: PotionClass::Any,
        },
        PotionId::BlockPotion => PotionDefinition {
            id,
            name: "Block Potion",
            rarity: PotionRarity::Common,
            base_potency: 12,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::BloodPotion => PotionDefinition {
            id,
            name: "Blood Potion",
            rarity: PotionRarity::Common,
            base_potency: 20,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Ironclad,
        },
        PotionId::EnergyPotion => PotionDefinition {
            id,
            name: "Energy Potion",
            rarity: PotionRarity::Common,
            base_potency: 2,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::StrengthPotion => PotionDefinition {
            id,
            name: "Strength Potion",
            rarity: PotionRarity::Common,
            base_potency: 2,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::DexterityPotion => PotionDefinition {
            id,
            name: "Dexterity Potion",
            rarity: PotionRarity::Common,
            base_potency: 2,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::SpeedPotion => PotionDefinition {
            id,
            name: "Speed Potion",
            rarity: PotionRarity::Common,
            base_potency: 5,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::SteroidPotion => PotionDefinition {
            id,
            name: "Flex Potion",
            rarity: PotionRarity::Common,
            base_potency: 5,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::SwiftPotion => PotionDefinition {
            id,
            name: "Swift Potion",
            rarity: PotionRarity::Common,
            base_potency: 3,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::FocusPotion => PotionDefinition {
            id,
            name: "Focus Potion",
            rarity: PotionRarity::Common,
            base_potency: 2,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Defect,
        },
        PotionId::AttackPotion => PotionDefinition {
            id,
            name: "Attack Potion",
            rarity: PotionRarity::Common,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::SkillPotion => PotionDefinition {
            id,
            name: "Skill Potion",
            rarity: PotionRarity::Common,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::PowerPotion => PotionDefinition {
            id,
            name: "Power Potion",
            rarity: PotionRarity::Common,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::ColorlessPotion => PotionDefinition {
            id,
            name: "Colorless Potion",
            rarity: PotionRarity::Common,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::BottledMiracle => PotionDefinition {
            id,
            name: "Bottled Miracle",
            rarity: PotionRarity::Common,
            base_potency: 2,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Watcher,
        },
        PotionId::BlessingOfTheForge => PotionDefinition {
            id,
            name: "Blessing of the Forge",
            rarity: PotionRarity::Common,
            base_potency: 0,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },

        // ---- Uncommon ----
        PotionId::AncientPotion => PotionDefinition {
            id,
            name: "Ancient Potion",
            rarity: PotionRarity::Uncommon,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::RegenPotion => PotionDefinition {
            id,
            name: "Regen Potion",
            rarity: PotionRarity::Uncommon,
            base_potency: 5,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::EssenceOfSteel => PotionDefinition {
            id,
            name: "Essence of Steel",
            rarity: PotionRarity::Uncommon,
            base_potency: 4,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::LiquidBronze => PotionDefinition {
            id,
            name: "Liquid Bronze",
            rarity: PotionRarity::Uncommon,
            base_potency: 3,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::DistilledChaosPotion => PotionDefinition {
            id,
            name: "Distilled Chaos",
            rarity: PotionRarity::Uncommon,
            base_potency: 3,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::DuplicationPotion => PotionDefinition {
            id,
            name: "Duplication Potion",
            rarity: PotionRarity::Uncommon,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::CunningPotion => PotionDefinition {
            id,
            name: "Cunning Potion",
            rarity: PotionRarity::Uncommon,
            base_potency: 3,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Silent,
        },
        PotionId::PotionOfCapacity => PotionDefinition {
            id,
            name: "Potion of Capacity",
            rarity: PotionRarity::Uncommon,
            base_potency: 2,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Defect,
        },
        PotionId::LiquidMemories => PotionDefinition {
            id,
            name: "Liquid Memories",
            rarity: PotionRarity::Uncommon,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::GamblersBrew => PotionDefinition {
            id,
            name: "Gambler's Brew",
            rarity: PotionRarity::Uncommon,
            base_potency: 0,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::Elixir => PotionDefinition {
            id,
            name: "Elixir",
            rarity: PotionRarity::Uncommon,
            base_potency: 0,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Ironclad,
        },
        PotionId::StancePotion => PotionDefinition {
            id,
            name: "Stance Potion",
            rarity: PotionRarity::Uncommon,
            base_potency: 0,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Watcher,
        },

        // ---- Rare ----
        PotionId::FairyPotion => PotionDefinition {
            id,
            name: "Fairy in a Bottle",
            rarity: PotionRarity::Rare,
            base_potency: 30,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::SmokeBomb => PotionDefinition {
            id,
            name: "Smoke Bomb",
            rarity: PotionRarity::Rare,
            base_potency: 0,
            target_required: false,
            is_thrown: true,
            class: PotionClass::Any,
        },
        PotionId::FruitJuice => PotionDefinition {
            id,
            name: "Fruit Juice",
            rarity: PotionRarity::Rare,
            base_potency: 5,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::EntropicBrew => PotionDefinition {
            id,
            name: "Entropic Brew",
            rarity: PotionRarity::Rare,
            base_potency: 0,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::SneckoOil => PotionDefinition {
            id,
            name: "Snecko Oil",
            rarity: PotionRarity::Rare,
            base_potency: 5,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::GhostInAJar => PotionDefinition {
            id,
            name: "Ghost In A Jar",
            rarity: PotionRarity::Rare,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Silent,
        },
        PotionId::HeartOfIron => PotionDefinition {
            id,
            name: "Heart of Iron",
            rarity: PotionRarity::Rare,
            base_potency: 6,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Ironclad,
        },
        PotionId::CultistPotion => PotionDefinition {
            id,
            name: "Cultist Potion",
            rarity: PotionRarity::Rare,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Any,
        },
        PotionId::Ambrosia => PotionDefinition {
            id,
            name: "Ambrosia",
            rarity: PotionRarity::Rare,
            base_potency: 2,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Watcher,
        },
        PotionId::EssenceOfDarkness => PotionDefinition {
            id,
            name: "Essence of Darkness",
            rarity: PotionRarity::Rare,
            base_potency: 1,
            target_required: false,
            is_thrown: false,
            class: PotionClass::Defect,
        },
    }
}

/// Price at the shop
pub fn get_potion_price(id: PotionId) -> i32 {
    match get_potion_definition(id).rarity {
        PotionRarity::Common => 50,
        PotionRarity::Uncommon => 75,
        PotionRarity::Rare => 100,
    }
}

/// All potions available for a given class filter, in **Java PotionHelper.getPotions()** order.
/// Class-specific potions come first, then shared potions in the exact Java order.
/// This order is critical for RNG seed parity since random_potion() indexes into this pool.
pub fn potions_for_class(class: PotionClass) -> Vec<PotionId> {
    let mut pool = Vec::with_capacity(42);

    // 1. Class-specific potions (Java lines 82-107: getPotions(class, false))
    match class {
        PotionClass::Ironclad => {
            pool.push(PotionId::BloodPotion);
            pool.push(PotionId::Elixir);
            pool.push(PotionId::HeartOfIron);
        }
        PotionClass::Silent => {
            pool.push(PotionId::PoisonPotion);
            pool.push(PotionId::CunningPotion);
            pool.push(PotionId::GhostInAJar);
        }
        PotionClass::Defect => {
            pool.push(PotionId::FocusPotion);
            pool.push(PotionId::PotionOfCapacity);
            pool.push(PotionId::EssenceOfDarkness);
        }
        PotionClass::Watcher => {
            pool.push(PotionId::BottledMiracle);
            pool.push(PotionId::StancePotion);
            pool.push(PotionId::Ambrosia);
        }
        PotionClass::Any => {
            // "getAll" mode: add all 12 class-specific first
            pool.push(PotionId::BloodPotion);
            pool.push(PotionId::Elixir);
            pool.push(PotionId::HeartOfIron);
            pool.push(PotionId::PoisonPotion);
            pool.push(PotionId::CunningPotion);
            pool.push(PotionId::GhostInAJar);
            pool.push(PotionId::FocusPotion);
            pool.push(PotionId::PotionOfCapacity);
            pool.push(PotionId::EssenceOfDarkness);
            pool.push(PotionId::BottledMiracle);
            pool.push(PotionId::StancePotion);
            pool.push(PotionId::Ambrosia);
        }
    }

    // 2. Shared potions (Java lines 122-151, exact order)
    pool.push(PotionId::BlockPotion);
    pool.push(PotionId::DexterityPotion);
    pool.push(PotionId::EnergyPotion);
    pool.push(PotionId::ExplosivePotion);
    pool.push(PotionId::FirePotion);
    pool.push(PotionId::StrengthPotion);
    pool.push(PotionId::SwiftPotion);
    pool.push(PotionId::WeakenPotion);
    pool.push(PotionId::FearPotion);
    pool.push(PotionId::AttackPotion);
    pool.push(PotionId::SkillPotion);
    pool.push(PotionId::PowerPotion);
    pool.push(PotionId::ColorlessPotion);
    pool.push(PotionId::SteroidPotion);
    pool.push(PotionId::SpeedPotion);
    pool.push(PotionId::BlessingOfTheForge);
    pool.push(PotionId::RegenPotion);
    pool.push(PotionId::AncientPotion);
    pool.push(PotionId::LiquidBronze);
    pool.push(PotionId::GamblersBrew);
    pool.push(PotionId::EssenceOfSteel);
    pool.push(PotionId::DuplicationPotion);
    pool.push(PotionId::DistilledChaosPotion);
    pool.push(PotionId::LiquidMemories);
    pool.push(PotionId::CultistPotion);
    pool.push(PotionId::FruitJuice);
    pool.push(PotionId::SneckoOil);
    pool.push(PotionId::FairyPotion);
    pool.push(PotionId::SmokeBomb);
    pool.push(PotionId::EntropicBrew);

    pool
}

/// Complete list of all potion IDs (for iteration, not for RNG — use potions_for_class for RNG)
pub const ALL_POTIONS: &[PotionId] = &[
    // Class-specific
    PotionId::BloodPotion,
    PotionId::Elixir,
    PotionId::HeartOfIron,
    PotionId::PoisonPotion,
    PotionId::CunningPotion,
    PotionId::GhostInAJar,
    PotionId::FocusPotion,
    PotionId::PotionOfCapacity,
    PotionId::EssenceOfDarkness,
    PotionId::BottledMiracle,
    PotionId::StancePotion,
    PotionId::Ambrosia,
    // Shared (Java order)
    PotionId::BlockPotion,
    PotionId::DexterityPotion,
    PotionId::EnergyPotion,
    PotionId::ExplosivePotion,
    PotionId::FirePotion,
    PotionId::StrengthPotion,
    PotionId::SwiftPotion,
    PotionId::WeakenPotion,
    PotionId::FearPotion,
    PotionId::AttackPotion,
    PotionId::SkillPotion,
    PotionId::PowerPotion,
    PotionId::ColorlessPotion,
    PotionId::SteroidPotion,
    PotionId::SpeedPotion,
    PotionId::BlessingOfTheForge,
    PotionId::RegenPotion,
    PotionId::AncientPotion,
    PotionId::LiquidBronze,
    PotionId::GamblersBrew,
    PotionId::EssenceOfSteel,
    PotionId::DuplicationPotion,
    PotionId::DistilledChaosPotion,
    PotionId::LiquidMemories,
    PotionId::CultistPotion,
    PotionId::FruitJuice,
    PotionId::SneckoOil,
    PotionId::FairyPotion,
    PotionId::SmokeBomb,
    PotionId::EntropicBrew,
];

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct Potion {
    pub id: PotionId,
    pub uuid: u32,
    #[serde(default = "serde_true", skip_serializing_if = "is_true")]
    pub can_use: bool,
    #[serde(default = "serde_true", skip_serializing_if = "is_true")]
    pub can_discard: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub requires_target: bool,
}

fn serde_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Potion {
    pub fn new(id: PotionId, uuid: u32) -> Self {
        let definition = get_potion_definition(id);
        Self {
            id,
            uuid,
            can_use: id != PotionId::FairyPotion,
            can_discard: true,
            requires_target: definition.target_required,
        }
    }

    pub fn with_affordance_truth(
        id: PotionId,
        uuid: u32,
        can_use: bool,
        can_discard: bool,
        requires_target: bool,
    ) -> Self {
        Self {
            id,
            uuid,
            can_use,
            can_discard,
            requires_target,
        }
    }
}

pub fn potion_can_discard_in_event(is_we_meet_again_event: bool) -> bool {
    !is_we_meet_again_event
}

pub fn potion_can_use_out_of_combat(id: PotionId, is_we_meet_again_event: bool) -> bool {
    if is_we_meet_again_event {
        return false;
    }
    matches!(
        id,
        PotionId::BloodPotion | PotionId::FruitJuice | PotionId::EntropicBrew
    )
}

/// Java `AbstractPotion.canUse()` for an already-owned potion in combat, plus
/// potion-specific overrides that are mechanical rather than UI-only.
///
/// This deliberately belongs below the generic potion metadata because it is a
/// runtime legality rule: adapters and action execution must agree on it.
pub fn potion_can_use_in_combat_like_java(
    potion: &Potion,
    combat: &crate::runtime::combat::CombatState,
) -> bool {
    if !potion.can_use || potion.id == PotionId::FairyPotion {
        return false;
    }
    if !matches!(
        combat.turn.current_phase,
        crate::runtime::combat::CombatPhase::PlayerTurn
    ) {
        return false;
    }
    if matches!(
        potion.id,
        PotionId::BloodPotion | PotionId::FruitJuice | PotionId::EntropicBrew
    ) {
        return true;
    }
    !combat.are_monsters_basically_dead_java()
        && !potion_blocked_in_combat_by_java_override(potion.id, combat)
}

pub fn potion_blocked_in_combat_by_java_override(
    id: PotionId,
    combat: &crate::runtime::combat::CombatState,
) -> bool {
    id == PotionId::SmokeBomb
        && (combat.meta.is_boss_fight
            || combat.entities.monsters.iter().any(|monster| {
                monster.enemy_id.is_boss()
                    || crate::content::powers::store::has_power(
                        combat,
                        monster.id,
                        crate::content::powers::PowerId::BackAttack,
                    )
            }))
}

/// Returns a random potion with rarity weighting, matching Java's `AbstractDungeon.returnRandomPotion()`.
/// Rolls 0-99: <65 = Common, 65-89 = Uncommon, ≥90 = Rare.
/// When `limited=true`, excludes FruitJuice (Java behavior for EntropicBrew).
pub fn random_potion(
    rng: &mut crate::runtime::rng::StsRng,
    class: PotionClass,
    limited: bool,
) -> PotionId {
    let roll = rng.random_range(0, 99);
    let rarity = if roll < 65 {
        PotionRarity::Common
    } else if roll < 90 {
        PotionRarity::Uncommon
    } else {
        PotionRarity::Rare
    };
    random_potion_by_rarity(rng, class, rarity, limited)
}

/// Returns a random potion of the given rarity from the class pool.
/// Java: `AbstractDungeon.returnRandomPotion(rarity, limited)` — rejection-samples from flat pool.
///
/// When `limited=true`, Java initializes `spamCheck` to true after taking an
/// initial flat random potion, so the first flat roll is always discarded before
/// the acceptable non-Fruit-Juice rejection-sampling loop begins.
pub fn random_potion_by_rarity(
    rng: &mut crate::runtime::rng::StsRng,
    class: PotionClass,
    rarity: PotionRarity,
    limited: bool,
) -> PotionId {
    let pool = potions_for_class(class);
    if limited {
        let _ = pool[rng.random(pool.len() as i32 - 1) as usize];
    }
    loop {
        let idx = rng.random(pool.len() as i32 - 1) as usize;
        let id = pool[idx];
        if get_potion_definition(id).rarity != rarity {
            continue;
        }
        if limited && id == PotionId::FruitJuice {
            continue;
        }
        return id;
    }
}

/// Returns a totally random potion (no rarity weighting). Java: `AbstractDungeon.returnTotallyRandomPotion()`.
pub fn random_potion_any(rng: &mut crate::runtime::rng::StsRng, class: PotionClass) -> PotionId {
    let pool = potions_for_class(class);
    let idx = rng.random(pool.len() as i32 - 1) as usize;
    pool[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::runtime::action::{Action, AddTo};
    use crate::runtime::combat::{Power, PowerPayload};

    #[test]
    fn human_input_aliases_are_owned_by_potion_identity() {
        assert_eq!(
            PotionId::from_input_alias_v1("Fire Potion"),
            Some(PotionId::FirePotion)
        );
        assert_eq!(
            PotionId::from_input_alias_v1("DistilledChaos"),
            Some(PotionId::DistilledChaosPotion)
        );
        assert_eq!(
            PotionId::from_input_alias_v1("ElixirPotion"),
            Some(PotionId::Elixir)
        );
        assert_eq!(
            PotionId::from_input_alias_v1("Weak Potion"),
            Some(PotionId::WeakenPotion)
        );
        assert_eq!(PotionId::from_input_alias_v1("Potion Slot"), None);

        for stable_id in PotionId::STABLE_IDS {
            let potion = PotionId::from_stable_id(stable_id).unwrap();
            assert_eq!(PotionId::from_input_alias_v1(stable_id), Some(potion));
            for alias in potion.human_input_aliases_v1() {
                assert_eq!(PotionId::from_input_alias_v1(alias), Some(potion));
            }
        }
    }

    #[test]
    fn every_potion_exposes_bounded_shared_mechanics() {
        for potion in ALL_POTIONS {
            let roles = potion_mechanic_roles(*potion);
            assert!(!roles.is_empty(), "{potion:?} has no mechanical fallback");
            assert!(roles
                .iter()
                .all(|role| (*role as u64) < POTION_MECHANIC_ROLE_COUNT));
        }
        assert_eq!(
            potion_mechanic_roles(PotionId::SneckoOil),
            &[
                PotionMechanicRole::DrawCards,
                PotionMechanicRole::RandomizeHandCosts
            ]
        );
    }

    #[test]
    fn potion_helper_pools_match_java_order_for_all_classes() {
        assert_eq!(
            potions_for_class(PotionClass::Ironclad),
            vec![
                PotionId::BloodPotion,
                PotionId::Elixir,
                PotionId::HeartOfIron,
                PotionId::BlockPotion,
                PotionId::DexterityPotion,
                PotionId::EnergyPotion,
                PotionId::ExplosivePotion,
                PotionId::FirePotion,
                PotionId::StrengthPotion,
                PotionId::SwiftPotion,
                PotionId::WeakenPotion,
                PotionId::FearPotion,
                PotionId::AttackPotion,
                PotionId::SkillPotion,
                PotionId::PowerPotion,
                PotionId::ColorlessPotion,
                PotionId::SteroidPotion,
                PotionId::SpeedPotion,
                PotionId::BlessingOfTheForge,
                PotionId::RegenPotion,
                PotionId::AncientPotion,
                PotionId::LiquidBronze,
                PotionId::GamblersBrew,
                PotionId::EssenceOfSteel,
                PotionId::DuplicationPotion,
                PotionId::DistilledChaosPotion,
                PotionId::LiquidMemories,
                PotionId::CultistPotion,
                PotionId::FruitJuice,
                PotionId::SneckoOil,
                PotionId::FairyPotion,
                PotionId::SmokeBomb,
                PotionId::EntropicBrew,
            ]
        );
        assert_eq!(
            &potions_for_class(PotionClass::Silent)[..3],
            &[
                PotionId::PoisonPotion,
                PotionId::CunningPotion,
                PotionId::GhostInAJar
            ]
        );
        assert_eq!(
            &potions_for_class(PotionClass::Defect)[..3],
            &[
                PotionId::FocusPotion,
                PotionId::PotionOfCapacity,
                PotionId::EssenceOfDarkness
            ]
        );
        assert_eq!(
            &potions_for_class(PotionClass::Watcher)[..3],
            &[
                PotionId::BottledMiracle,
                PotionId::StancePotion,
                PotionId::Ambrosia
            ]
        );
        assert_eq!(
            potions_for_class(PotionClass::Any),
            ALL_POTIONS,
            "Java PotionHelper.getPotions(null, true) is the all-potion upload/order list"
        );
        assert_eq!(ALL_POTIONS.len(), 42);
    }

    #[test]
    fn potion_can_use_overrides_match_java_sources() {
        assert!(potion_can_use_out_of_combat(PotionId::BloodPotion, false));
        assert!(potion_can_use_out_of_combat(PotionId::FruitJuice, false));
        assert!(potion_can_use_out_of_combat(PotionId::EntropicBrew, false));
        assert!(!potion_can_use_out_of_combat(PotionId::BlockPotion, false));
        assert!(!potion_can_use_out_of_combat(PotionId::FirePotion, false));
        assert!(!potion_can_use_out_of_combat(PotionId::FairyPotion, false));
        assert!(!potion_can_use_out_of_combat(PotionId::BloodPotion, true));
        assert!(!potion_can_discard_in_event(true));

        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        )];
        combat.turn.current_phase = crate::runtime::combat::CombatPhase::PlayerTurn;
        combat.meta.is_boss_fight = false;
        assert!(potion_can_use_in_combat_like_java(
            &Potion::new(PotionId::FirePotion, 1),
            &combat
        ));
        assert!(potion_can_use_in_combat_like_java(
            &Potion::new(PotionId::BloodPotion, 2),
            &combat
        ));
        assert!(!potion_can_use_in_combat_like_java(
            &Potion::new(PotionId::FairyPotion, 3),
            &combat
        ));

        combat.entities.monsters[0].current_hp = 0;
        combat.entities.monsters[0].is_dying = true;
        assert!(!potion_can_use_in_combat_like_java(
            &Potion::new(PotionId::FirePotion, 5),
            &combat
        ));
        assert!(
            potion_can_use_in_combat_like_java(&Potion::new(PotionId::FruitJuice, 6), &combat),
            "Java FruitJuice overrides AbstractPotion.canUse and does not inherit the alive-monster gate"
        );
        combat.entities.monsters[0].current_hp = 40;
        combat.entities.monsters[0].is_dying = false;

        combat.meta.is_boss_fight = true;
        assert!(!potion_can_use_in_combat_like_java(
            &Potion::new(PotionId::SmokeBomb, 4),
            &combat
        ));
    }

    #[test]
    fn limited_random_potion_discards_initial_flat_roll_like_java() {
        let mut rng = crate::runtime::rng::StsRng::new(17);
        let before = rng.counter;

        let potion =
            random_potion_by_rarity(&mut rng, PotionClass::Silent, PotionRarity::Rare, true);

        assert_ne!(potion, PotionId::FruitJuice);
        assert!(
            rng.counter >= before + 2,
            "Java returnRandomPotion(rarity, true) consumes and discards one initial PotionHelper.getRandomPotion roll before accepting a limited result"
        );
    }

    #[test]
    fn watcher_potion_metadata_matches_java_sources() {
        let bottled = get_potion_definition(PotionId::BottledMiracle);
        assert_eq!(bottled.rarity, PotionRarity::Common);
        assert_eq!(bottled.base_potency, 2);
        assert!(!bottled.target_required);
        assert!(!bottled.is_thrown);
        assert_eq!(bottled.class, PotionClass::Watcher);

        let stance = get_potion_definition(PotionId::StancePotion);
        assert_eq!(stance.rarity, PotionRarity::Uncommon);
        assert_eq!(stance.base_potency, 0);
        assert!(!stance.target_required);
        assert!(!stance.is_thrown);
        assert_eq!(stance.class, PotionClass::Watcher);

        let ambrosia = get_potion_definition(PotionId::Ambrosia);
        assert_eq!(ambrosia.rarity, PotionRarity::Rare);
        assert_eq!(ambrosia.base_potency, 2);
        assert!(!ambrosia.target_required);
        assert!(!ambrosia.is_thrown);
        assert_eq!(ambrosia.class, PotionClass::Watcher);

        let watcher_pool = potions_for_class(PotionClass::Watcher);
        assert_eq!(
            &watcher_pool[..3],
            &[
                PotionId::BottledMiracle,
                PotionId::StancePotion,
                PotionId::Ambrosia
            ]
        );
    }

    #[test]
    fn watcher_potion_actions_match_java_sources() {
        let state = crate::test_support::blank_test_combat();

        let bottled = potion_effects::get_potion_actions(&state, PotionId::BottledMiracle, None, 2);
        assert_eq!(bottled.len(), 1);
        assert_eq!(bottled[0].insertion_mode, AddTo::Bottom);
        match &bottled[0].action {
            Action::MakeConstructedCopyInHand { original, amount } => {
                assert_eq!(original.id, CardId::Miracle);
                assert_eq!(original.upgrades, 0);
                assert_eq!(*amount, 2);
            }
            other => panic!("Bottled Miracle should queue constructed Miracles, got {other:?}"),
        }

        let stance = potion_effects::get_potion_actions(&state, PotionId::StancePotion, None, 0);
        assert_eq!(stance.len(), 1);
        assert_eq!(stance[0].insertion_mode, AddTo::Bottom);
        assert!(matches!(stance[0].action, Action::SuspendForStanceChoice));

        let ambrosia = potion_effects::get_potion_actions(&state, PotionId::Ambrosia, None, 2);
        assert_eq!(ambrosia.len(), 1);
        assert_eq!(ambrosia[0].insertion_mode, AddTo::Bottom);
        assert!(matches!(&ambrosia[0].action, Action::EnterStance(stance) if stance == "Divinity"));
    }

    #[test]
    fn elixir_selection_records_only_a_material_pending_on_use_effect() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.current_hp = state.entities.player.max_hp;

        let ordinary = potion_effects::get_potion_actions(&state, PotionId::Elixir, None, 0);
        assert!(matches!(
            ordinary[0].action,
            Action::SuspendForHandSelect {
                reason: crate::state::HandSelectReason::PotionExhaust {
                    has_pending_use_effect: false
                },
                ..
            }
        ));

        state
            .entities
            .player
            .add_relic(crate::content::relics::RelicState::new(
                crate::content::relics::RelicId::ToyOrnithopter,
            ));
        state.entities.player.current_hp -= 5;
        let ornithopter = potion_effects::get_potion_actions(&state, PotionId::Elixir, None, 0);
        assert!(matches!(
            ornithopter[0].action,
            Action::SuspendForHandSelect {
                reason: crate::state::HandSelectReason::PotionExhaust {
                    has_pending_use_effect: true
                },
                ..
            }
        ));

        state
            .entities
            .player
            .add_relic(crate::content::relics::RelicState::new(
                crate::content::relics::RelicId::MarkOfTheBloom,
            ));
        let blocked_heal = potion_effects::get_potion_actions(&state, PotionId::Elixir, None, 0);
        assert!(matches!(
            blocked_heal[0].action,
            Action::SuspendForHandSelect {
                reason: crate::state::HandSelectReason::PotionExhaust {
                    has_pending_use_effect: false
                },
                ..
            }
        ));
    }

    #[test]
    fn generated_card_potions_use_make_temp_constructor_master_reality() {
        let mut state = crate::test_support::blank_test_combat();
        crate::content::powers::store::set_powers_for(
            &mut state,
            0,
            vec![Power {
                power_type: crate::content::powers::PowerId::MasterRealityPower,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        let bottled = potion_effects::get_potion_actions(&state, PotionId::BottledMiracle, None, 2);
        match &bottled[0].action {
            Action::MakeConstructedCopyInHand { original, amount } => {
                assert_eq!(original.id, CardId::Miracle);
                assert_eq!(original.upgrades, 1);
                assert_eq!(*amount, 2);
            }
            other => {
                panic!("Bottled Miracle should construct Miracle before queueing, got {other:?}")
            }
        }

        let cunning = potion_effects::get_potion_actions(&state, PotionId::CunningPotion, None, 3);
        match &cunning[0].action {
            Action::MakeConstructedCopyInHand { original, amount } => {
                assert_eq!(original.id, CardId::Shiv);
                assert_eq!(original.upgrades, 1);
                assert_eq!(*amount, 3);
            }
            other => panic!(
                "Cunning Potion should construct upgraded Shiv before queueing, got {other:?}"
            ),
        }
    }
}
