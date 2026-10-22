pub mod core;
pub mod defect;
pub mod ironclad;
pub mod mechanics;
pub mod silent;
pub mod store;
pub mod watcher;

crate::stable_gameplay_id! {
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerId {
    Strength => ("Strength", 0),
    Vulnerable => ("Vulnerable", 1),
    Weak => ("Weak", 2),
    Frail => ("Frail", 3),
    Dexterity => ("Dexterity", 4),
    Ritual => ("Ritual", 5),
    Poison => ("Poison", 6),
    LoseStrength => ("LoseStrength", 7),
    DemonForm => ("DemonForm", 8),
    Corruption => ("Corruption", 9),
    DoubleTap => ("DoubleTap", 10),
    DuplicationPower => ("DuplicationPower", 11),
    FeelNoPain => ("FeelNoPain", 12),
    DarkEmbrace => ("DarkEmbrace", 13),
    Minion => ("Minion", 14),
    Rupture => ("Rupture", 15),
    Combust => ("Combust", 16),
    Brutality => ("Brutality", 17),
    Barricade => ("Barricade", 18),
    Juggernaut => ("Juggernaut", 19),
    FlameBarrier => ("FlameBarrier", 20),
    Metallicize => ("Metallicize", 21),
    CurlUp => ("CurlUp", 22),
    SporeCloud => ("SporeCloud", 23),
    Split => ("Split", 24),
    Entangle => ("Entangle", 25),
    Evolve => ("Evolve", 26),
    FireBreathing => ("FireBreathing", 27),
    NoDraw => ("NoDraw", 28),
    NoBlock => ("NoBlock", 29),
    Regen => ("Regen", 30),
    Regeneration => ("Regeneration", 31),
    Rage => ("Rage", 32),
    Berserk => ("Berserk", 33),
    Angry => ("Angry", 34),
    Anger => ("Anger", 35),
    ModeShift => ("ModeShift", 36),
    SharpHide => ("SharpHide", 37),
    Artifact => ("Artifact", 38),
    GuardianThreshold => ("GuardianThreshold", 39),
    Flight => ("Flight", 40),
    Hex => ("Hex", 41),
    Malleable => ("Malleable", 42),
    PlatedArmor => ("PlatedArmor", 43),
    Confusion => ("Confusion", 44),
    Thievery => ("Thievery", 45),
    PainfulStabs => ("PainfulStabs", 46),
    Stasis => ("Stasis", 47),
    GenericStrengthUp => ("GenericStrengthUp", 48),
    Explosive => ("Explosive", 49),
    Thorns => ("Thorns", 50),
    Regrow => ("Regrow", 51),
    Constricted => ("Constricted", 52),
    Fading => ("Fading", 53),
    Shifting => ("Shifting", 54),
    Reactive => ("Reactive", 55),
    Slow => ("Slow", 56),
    Intangible => ("Intangible", 57),
    IntangiblePlayer => ("IntangiblePlayer", 58),
    Curiosity => ("Curiosity", 59),
    Unawakened => ("Unawakened", 60),
    TimeWarp => ("TimeWarp", 61),
    Invincible => ("Invincible", 62),
    Vigor => ("Vigor", 63),
    BeatOfDeath => ("BeatOfDeath", 64),
    Mantra => ("Mantra", 65),
    CannotChangeStance => ("CannotChangeStance", 66),
    WrathNextTurn => ("WrathNextTurn", 67),
    FreeAttackPower => ("FreeAttackPower", 68),
    BlockReturnPower => ("BlockReturnPower", 69),
    BattleHymnPower => ("BattleHymnPower", 70),
    DevotionPower => ("DevotionPower", 71),
    LikeWaterPower => ("LikeWaterPower", 72),
    MentalFortressPower => ("MentalFortressPower", 73),
    RushdownPower => ("RushdownPower", 74),
    ForesightPower => ("ForesightPower", 75),
    NirvanaPower => ("NirvanaPower", 76),
    MarkPower => ("MarkPower", 77),
    EstablishmentPower => ("EstablishmentPower", 78),
    WaveOfTheHandPower => ("WaveOfTheHandPower", 79),
    EnergyDownPower => ("EnergyDownPower", 80),
    DevaForm => ("DevaForm", 81),
    EndTurnDeathPower => ("EndTurnDeathPower", 82),
    CollectPower => ("CollectPower", 83),
    OmegaPower => ("OmegaPower", 84),
    Focus => ("Focus", 85),
    DexterityDown => ("DexterityDown", 86),
    PenNibPower => ("PenNibPower", 87),
    NextTurnBlock => ("NextTurnBlock", 88),
    DrawCardNextTurn => ("DrawCardNextTurn", 89),
    Draw => ("Draw", 90),
    Energized => ("Energized", 91),
    Equilibrium => ("Equilibrium", 92),
    Repair => ("Repair", 93),
    LockOn => ("LockOn", 94),
    Bias => ("Bias", 95),
    Loop => ("Loop", 96),
    StaticDischarge => ("StaticDischarge", 97),
    Heatsink => ("Heatsink", 98),
    Storm => ("Storm", 99),
    Amplify => ("Amplify", 100),
    EchoForm => ("EchoForm", 101),
    CreativeAI => ("CreativeAI", 102),
    Hello => ("Hello", 103),
    // Colorless card powers
    MagnetismPower => ("MagnetismPower", 104),
    MayhemPower => ("MayhemPower", 105),
    PanachePower => ("PanachePower", 106),
    SadisticPower => ("SadisticPower", 107),
    TheBombPower => ("TheBombPower", 108),
    Buffer => ("Buffer", 109),
    Electro => ("Electro", 110),
    NoSkills => ("NoSkills", 111),
    ThousandCuts => ("ThousandCuts", 112),
    Shackled => ("Shackled", 113),
    DrawReduction => ("DrawReduction", 114),
    Surrounded => ("Surrounded", 115),
    BackAttack => ("BackAttack", 116),
    NoxiousFumes => ("NoxiousFumes", 117),
    AfterImage => ("AfterImage", 118),
    Burst => ("Burst", 119),
    MasterRealityPower => ("MasterRealityPower", 120),
    AccuracyPower => ("AccuracyPower", 121),
    InfiniteBladesPower => ("InfiniteBladesPower", 122),
    Blur => ("Blur", 123),
    Choked => ("Choked", 124),
    CorpseExplosion => ("CorpseExplosion", 125),
    WraithForm => ("WraithForm", 126),
    Phantasmal => ("Phantasmal", 127),
    DoubleDamage => ("DoubleDamage", 128),
    Envenom => ("Envenom", 129),
    ToolsOfTheTrade => ("ToolsOfTheTrade", 130),
    RetainCards => ("RetainCards", 131),
    Nightmare => ("Nightmare", 132),
    Rebound => ("Rebound", 133),
    Study => ("Study", 134),
    EnergizedBlue => ("EnergizedBlue", 135),
}
}

use crate::runtime::combat::{CombatCard, CombatState};

pub struct PowerDefinition {
    pub id: PowerId,
    pub name: &'static str,
}

pub fn power_priority(id: PowerId) -> i32 {
    match id {
        PowerId::Confusion => 0,
        PowerId::DoubleDamage | PowerId::PenNibPower => 6,
        PowerId::Frail => 10,
        PowerId::DrawCardNextTurn => 20,
        PowerId::EstablishmentPower | PowerId::ToolsOfTheTrade => 25,
        PowerId::Flight | PowerId::Reactive => 50,
        PowerId::Intangible | PowerId::IntangiblePlayer => 75,
        PowerId::Invincible | PowerId::Weak => 99,
        PowerId::Constricted => 105,
        _ => 5,
    }
}

fn class_pool_for_rarity(
    player_class: &str,
    rarity: crate::content::cards::CardRarity,
) -> &'static [crate::content::cards::CardId] {
    match player_class {
        "Ironclad" => crate::content::cards::ironclad_pool_for_rarity(rarity),
        "Silent" => crate::content::cards::silent_pool_for_rarity(rarity),
        "Defect" => crate::content::cards::defect_pool_for_rarity(rarity),
        "Watcher" => crate::content::cards::watcher_pool_for_rarity(rarity),
        _ => crate::content::cards::ironclad_pool_for_rarity(rarity),
    }
}

fn random_class_card_by_rarity(
    state: &mut CombatState,
    rarity: crate::content::cards::CardRarity,
) -> Option<crate::runtime::combat::CardBlueprint> {
    let pool = class_pool_for_rarity(state.meta.player_class.as_str(), rarity);
    if pool.is_empty() {
        return None;
    }

    let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
    Some(crate::content::cards::make_fresh_card_blueprint_for_combat(
        pool[idx], state,
    ))
}

/// Java powers whose runtime amount is a sentinel `-1` rather than a stack count.
pub fn uses_sentinel_amount(id: PowerId) -> bool {
    matches!(
        id,
        PowerId::NoDraw
            | PowerId::Minion
            | PowerId::Corruption
            | PowerId::Split
            | PowerId::Barricade
            | PowerId::Stasis
            | PowerId::BackAttack
            | PowerId::PainfulStabs
            | PowerId::Electro
            | PowerId::CannotChangeStance
            | PowerId::Regrow
            | PowerId::Surrounded
            | PowerId::Unawakened
            | PowerId::Reactive
            | PowerId::Confusion
            | PowerId::Shifting
            | PowerId::WrathNextTurn
            | PowerId::EndTurnDeathPower
            | PowerId::MasterRealityPower
    )
}

/// Java `ApplyPowerAction` only has a special duplicate-application early return
/// for `NoDrawPower`. Other powers with sentinel `amount == -1` still use the
/// default `stackPower(-1)` path when they are reapplied; AbstractPower keeps
/// that existing -1 amount unchanged without skipping the application hooks.
pub fn reapplying_existing_power_is_noop(id: PowerId) -> bool {
    matches!(id, PowerId::NoDraw)
}

pub fn uses_distinct_instances(id: PowerId) -> bool {
    matches!(id, PowerId::TheBombPower | PowerId::Nightmare)
}

pub fn canonicalize_applied_amount(id: PowerId, amount: i32) -> i32 {
    if uses_sentinel_amount(id) {
        -1
    } else {
        amount
    }
}

pub fn allows_negative_amount(id: PowerId) -> bool {
    matches!(
        id,
        PowerId::Strength
            | PowerId::Dexterity
            | PowerId::Focus
            | PowerId::WraithForm
            | PowerId::Draw
    ) || uses_sentinel_amount(id)
}

pub fn should_keep_power_instance(id: PowerId, amount: i32) -> bool {
    if id == PowerId::Slow {
        return amount >= 0;
    }
    if id == PowerId::TimeWarp {
        return true;
    }
    if amount > 0 {
        return true;
    }
    if amount < 0 {
        return allows_negative_amount(id);
    }
    false
}

/// Java: AbstractPower.PowerType — BUFF or DEBUFF.
/// Some powers are dynamic: Strength/Focus/Dexterity are DEBUFF when amount < 0.
/// Used by: SadisticPower.onApplyPower, RemoveDebuffsAction, Artifact interception.
pub fn is_debuff(id: PowerId, amount: i32) -> bool {
    match id {
        // Dynamic: DEBUFF when amount < 0
        PowerId::Strength | PowerId::Focus | PowerId::Dexterity | PowerId::Draw => amount < 0,
        // Fixed DEBUFFs (from Java constructor: this.type = PowerType.DEBUFF)
        PowerId::Vulnerable
        | PowerId::Weak
        | PowerId::Frail
        | PowerId::Poison
        | PowerId::Entangle
        | PowerId::NoDraw
        | PowerId::NoBlock
        | PowerId::LockOn
        | PowerId::Bias
        | PowerId::Constricted
        | PowerId::Confusion
        | PowerId::Hex
        | PowerId::Slow
        | PowerId::LoseStrength
        | PowerId::DexterityDown
        | PowerId::NoSkills
        | PowerId::CannotChangeStance
        | PowerId::Fading
        | PowerId::Shackled
        | PowerId::DrawReduction
        | PowerId::Choked
        | PowerId::CorpseExplosion
        | PowerId::BlockReturnPower
        | PowerId::MarkPower
        | PowerId::EnergyDownPower
        | PowerId::WraithForm => true,
        // Everything else is BUFF
        _ => false,
    }
}

/// Artifact explanation beside the authoritative debuff classification.
pub const ARTIFACT_MECHANICS: mechanics::PowerMechanics = mechanics::PowerMechanics {
    text_id: crate::content::powers::PowerId::Artifact,
    round_duration: false,
};

/// Returns true only when this ApplyPower is making the target meaningfully more debuffed.
///
/// This is narrower than `is_debuff(...)`:
/// - fixed debuffs like Weak/Vulnerable are only debuff applications when `amount > 0`
/// - dynamic debuffs like negative Strength are only debuff applications when `amount < 0`
///
/// End-of-round cleanup like `Weak -1` should not be blocked by Artifact or trigger
/// "on apply debuff" hooks.
pub fn is_debuff_application(id: PowerId, amount: i32) -> bool {
    match id {
        PowerId::Strength | PowerId::Focus | PowerId::Dexterity => amount < 0,
        // Some Java debuffs use sentinel amounts instead of stacks. Artifact still
        // blocks the application even when the constructor amount is -1.
        PowerId::NoDraw | PowerId::Confusion | PowerId::WraithForm => amount != 0,
        PowerId::CannotChangeStance => amount != 0,
        PowerId::EnergyDownPower => amount > 0,
        PowerId::NoBlock => amount > 0,
        PowerId::Vulnerable
        | PowerId::Weak
        | PowerId::Frail
        | PowerId::Poison
        | PowerId::Entangle
        | PowerId::Constricted
        | PowerId::Hex
        | PowerId::Slow
        | PowerId::LockOn
        | PowerId::Bias
        | PowerId::LoseStrength
        | PowerId::DexterityDown
        | PowerId::NoSkills
        | PowerId::Fading
        | PowerId::DrawReduction
        | PowerId::Choked
        | PowerId::CorpseExplosion
        | PowerId::BlockReturnPower
        | PowerId::MarkPower => amount > 0,
        _ => false,
    }
}

pub fn get_power_definition(id: PowerId) -> PowerDefinition {
    match id {
        PowerId::Strength => PowerDefinition {
            id,
            name: "Strength",
        },
        PowerId::Vulnerable => PowerDefinition {
            id,
            name: "Vulnerable",
        },
        PowerId::Weak => PowerDefinition { id, name: "Weak" },
        PowerId::Frail => PowerDefinition { id, name: "Frail" },
        PowerId::Dexterity => PowerDefinition {
            id,
            name: "Dexterity",
        },
        PowerId::Ritual => PowerDefinition { id, name: "Ritual" },
        PowerId::Poison => PowerDefinition { id, name: "Poison" },
        PowerId::LoseStrength => PowerDefinition {
            id,
            name: "Lose Strength",
        },
        PowerId::DemonForm => PowerDefinition {
            id,
            name: "Demon Form",
        },
        PowerId::Corruption => PowerDefinition {
            id,
            name: "Corruption",
        },
        PowerId::DoubleTap => PowerDefinition {
            id,
            name: "Double Tap",
        },
        PowerId::DuplicationPower => PowerDefinition {
            id,
            name: "Duplication",
        },
        PowerId::FeelNoPain => PowerDefinition {
            id,
            name: "Feel No Pain",
        },
        PowerId::DarkEmbrace => PowerDefinition {
            id,
            name: "Dark Embrace",
        },
        PowerId::Minion => PowerDefinition { id, name: "Minion" },
        PowerId::Rupture => PowerDefinition {
            id,
            name: "Rupture",
        },
        PowerId::Combust => PowerDefinition {
            id,
            name: "Combust",
        },
        PowerId::Brutality => PowerDefinition {
            id,
            name: "Brutality",
        },
        PowerId::Barricade => PowerDefinition {
            id,
            name: "Barricade",
        },
        PowerId::Juggernaut => PowerDefinition {
            id,
            name: "Juggernaut",
        },
        PowerId::FlameBarrier => PowerDefinition {
            id,
            name: "Flame Barrier",
        },
        PowerId::Metallicize => PowerDefinition {
            id,
            name: "Metallicize",
        },
        PowerId::CurlUp => PowerDefinition {
            id,
            name: "Curl Up",
        },
        PowerId::SporeCloud => PowerDefinition {
            id,
            name: "Spore Cloud",
        },
        PowerId::Split => PowerDefinition { id, name: "Split" },
        PowerId::GenericStrengthUp => PowerDefinition {
            id,
            name: "Generic Strength Up",
        },
        PowerId::Entangle => PowerDefinition {
            id,
            name: "Entangled",
        },
        PowerId::Evolve => PowerDefinition { id, name: "Evolve" },
        PowerId::FireBreathing => PowerDefinition {
            id,
            name: "Fire Breathing",
        },
        PowerId::NoDraw => PowerDefinition {
            id,
            name: "No Draw",
        },
        PowerId::Regen => PowerDefinition {
            id,
            name: "Regenerate",
        },
        PowerId::Regeneration => PowerDefinition {
            id,
            name: "Regeneration",
        },
        PowerId::Rage => PowerDefinition { id, name: "Rage" },
        PowerId::Berserk => PowerDefinition {
            id,
            name: "Berserk",
        },
        PowerId::Angry => PowerDefinition { id, name: "Angry" },
        PowerId::Anger => PowerDefinition { id, name: "Anger" },
        // Java: Mode Shift is a display/state power. Guardian-specific behavior lives on the monster.
        PowerId::ModeShift => PowerDefinition {
            id,
            name: "Mode Shift",
        },
        PowerId::SharpHide => PowerDefinition {
            id,
            name: "Sharp Hide",
        },
        PowerId::Artifact => PowerDefinition {
            id,
            name: "Artifact",
        },
        // Internal Rust-only tracker for The Guardian's next threshold; not exported by Java snapshots.
        PowerId::GuardianThreshold => PowerDefinition {
            id,
            name: "Guardian Threshold",
        },
        PowerId::Flight => PowerDefinition { id, name: "Flight" },
        PowerId::Hex => PowerDefinition { id, name: "Hex" },
        PowerId::Malleable => PowerDefinition {
            id,
            name: "Malleable",
        },
        PowerId::PlatedArmor => PowerDefinition {
            id,
            name: "Plated Armor",
        },
        PowerId::Confusion => PowerDefinition {
            id,
            name: "Confusion",
        },
        PowerId::Thievery => PowerDefinition {
            id,
            name: "Thievery",
        },
        PowerId::PainfulStabs => PowerDefinition {
            id,
            name: "Painful Stabs",
        },
        PowerId::Stasis => PowerDefinition { id, name: "Stasis" },
        PowerId::Explosive => PowerDefinition {
            id,
            name: "Explosive",
        },
        PowerId::Thorns => PowerDefinition { id, name: "Thorns" },
        PowerId::Regrow => PowerDefinition {
            id,
            name: "Life Link",
        },
        PowerId::Constricted => PowerDefinition {
            id,
            name: "Constricted",
        },
        PowerId::Fading => PowerDefinition { id, name: "Fading" },
        PowerId::Shifting => PowerDefinition {
            id,
            name: "Shifting",
        },
        PowerId::Reactive => PowerDefinition {
            id,
            name: "Reactive",
        },
        PowerId::Slow => PowerDefinition { id, name: "Slow" },
        PowerId::Intangible => PowerDefinition {
            id,
            name: "Intangible",
        },
        PowerId::IntangiblePlayer => PowerDefinition {
            id,
            name: "Intangible",
        },
        PowerId::Curiosity => PowerDefinition {
            id,
            name: "Curiosity",
        },
        PowerId::Unawakened => PowerDefinition {
            id,
            name: "Unawakened",
        },
        PowerId::NoBlock => PowerDefinition {
            id,
            name: "No Block",
        },
        PowerId::Shackled => PowerDefinition {
            id,
            name: "Shackled",
        },
        PowerId::DrawReduction => PowerDefinition {
            id: PowerId::DrawReduction,
            name: "Draw Reduction",
        },
        PowerId::Surrounded => PowerDefinition {
            id: PowerId::Surrounded,
            name: "Surrounded",
        },
        PowerId::BackAttack => PowerDefinition {
            id: PowerId::BackAttack,
            name: "Back Attack",
        },
        PowerId::TimeWarp => PowerDefinition {
            id,
            name: "Time Warp",
        },
        PowerId::Invincible => PowerDefinition {
            id,
            name: "Invincible",
        },
        PowerId::BeatOfDeath => PowerDefinition {
            id,
            name: "Beat of Death",
        },
        PowerId::Vigor => PowerDefinition { id, name: "Vigor" },
        PowerId::Mantra => PowerDefinition { id, name: "Mantra" },
        PowerId::CannotChangeStance => PowerDefinition {
            id,
            name: "Cannot Change Stance",
        },
        PowerId::WrathNextTurn => PowerDefinition {
            id,
            name: "Wrath Next Turn",
        },
        PowerId::FreeAttackPower => PowerDefinition {
            id,
            name: "Free Attack",
        },
        PowerId::BlockReturnPower => PowerDefinition {
            id,
            name: "Talk to the Hand",
        },
        PowerId::BattleHymnPower => PowerDefinition {
            id,
            name: "Battle Hymn",
        },
        PowerId::DevotionPower => PowerDefinition {
            id,
            name: "Devotion",
        },
        PowerId::LikeWaterPower => PowerDefinition {
            id,
            name: "Like Water",
        },
        PowerId::MentalFortressPower => PowerDefinition {
            id,
            name: "Mental Fortress",
        },
        PowerId::RushdownPower => PowerDefinition {
            id,
            name: "Rushdown",
        },
        PowerId::ForesightPower => PowerDefinition {
            id,
            name: "Foresight",
        },
        PowerId::NirvanaPower => PowerDefinition {
            id,
            name: "Nirvana",
        },
        PowerId::MarkPower => PowerDefinition { id, name: "Mark" },
        PowerId::EstablishmentPower => PowerDefinition {
            id,
            name: "Establishment",
        },
        PowerId::WaveOfTheHandPower => PowerDefinition {
            id,
            name: "Wave of the Hand",
        },
        PowerId::EnergyDownPower => PowerDefinition {
            id,
            name: "Energy Down",
        },
        PowerId::DevaForm => PowerDefinition {
            id,
            name: "Deva Form",
        },
        PowerId::EndTurnDeathPower => PowerDefinition {
            id,
            name: "Blasphemer",
        },
        PowerId::CollectPower => PowerDefinition {
            id,
            name: "Collect",
        },
        PowerId::OmegaPower => PowerDefinition { id, name: "Omega" },
        PowerId::Focus => PowerDefinition { id, name: "Focus" },
        PowerId::DexterityDown => PowerDefinition {
            id,
            name: "Dexterity Down",
        },
        PowerId::PenNibPower => PowerDefinition {
            id,
            name: "Pen Nib",
        },
        PowerId::NextTurnBlock => PowerDefinition {
            id,
            name: "Next Turn Block",
        },
        PowerId::DrawCardNextTurn => PowerDefinition {
            id,
            name: "Draw Card",
        },
        PowerId::Draw => PowerDefinition { id, name: "Draw" },
        PowerId::Energized | PowerId::EnergizedBlue => PowerDefinition {
            id,
            name: "Energized",
        },
        PowerId::Equilibrium => PowerDefinition {
            id,
            name: "Equilibrium",
        },
        PowerId::Repair => PowerDefinition { id, name: "Repair" },
        PowerId::LockOn => PowerDefinition {
            id,
            name: "Lock-On",
        },
        PowerId::Bias => PowerDefinition { id, name: "Bias" },
        PowerId::Loop => PowerDefinition { id, name: "Loop" },
        PowerId::StaticDischarge => PowerDefinition {
            id,
            name: "Static Discharge",
        },
        PowerId::Heatsink => PowerDefinition {
            id,
            name: "Heatsink",
        },
        PowerId::Storm => PowerDefinition { id, name: "Storm" },
        PowerId::Amplify => PowerDefinition {
            id,
            name: "Amplify",
        },
        PowerId::EchoForm => PowerDefinition {
            id,
            name: "Echo Form",
        },
        PowerId::CreativeAI => PowerDefinition {
            id,
            name: "Creative AI",
        },
        PowerId::Hello => PowerDefinition { id, name: "Hello" },
        PowerId::MagnetismPower => PowerDefinition {
            id,
            name: "Magnetism",
        },
        PowerId::MayhemPower => PowerDefinition { id, name: "Mayhem" },
        PowerId::PanachePower => PowerDefinition {
            id,
            name: "Panache",
        },
        PowerId::SadisticPower => PowerDefinition {
            id,
            name: "Sadistic Nature",
        },
        PowerId::TheBombPower => PowerDefinition {
            id,
            name: "The Bomb",
        },
        PowerId::Buffer => PowerDefinition { id, name: "Buffer" },
        PowerId::Electro => PowerDefinition {
            id,
            name: "Electrodynamics",
        },
        PowerId::NoSkills => PowerDefinition {
            id,
            name: "No Skills",
        },
        PowerId::ThousandCuts => PowerDefinition {
            id,
            name: "A Thousand Cuts",
        },
        PowerId::NoxiousFumes => PowerDefinition {
            id,
            name: "Noxious Fumes",
        },
        PowerId::AfterImage => PowerDefinition {
            id,
            name: "After Image",
        },
        PowerId::Burst => PowerDefinition { id, name: "Burst" },
        PowerId::MasterRealityPower => PowerDefinition {
            id,
            name: "Master Reality",
        },
        PowerId::AccuracyPower => PowerDefinition {
            id,
            name: "Accuracy",
        },
        PowerId::InfiniteBladesPower => PowerDefinition {
            id,
            name: "Infinite Blades",
        },
        PowerId::Blur => PowerDefinition { id, name: "Blur" },
        PowerId::Choked => PowerDefinition { id, name: "Choked" },
        PowerId::CorpseExplosion => PowerDefinition {
            id,
            name: "Corpse Explosion",
        },
        PowerId::WraithForm => PowerDefinition {
            id,
            name: "Wraith Form",
        },
        PowerId::Phantasmal => PowerDefinition {
            id,
            name: "Phantasmal",
        },
        PowerId::DoubleDamage => PowerDefinition {
            id,
            name: "Double Damage",
        },
        PowerId::Envenom => PowerDefinition {
            id,
            name: "Envenom",
        },
        PowerId::ToolsOfTheTrade => PowerDefinition {
            id,
            name: "Tools of the Trade",
        },
        PowerId::RetainCards => PowerDefinition {
            id,
            name: "Retain Cards",
        },
        PowerId::Nightmare => PowerDefinition {
            id,
            name: "Nightmare",
        },
        PowerId::Rebound => PowerDefinition {
            id,
            name: "Rebound",
        },
        PowerId::Study => PowerDefinition { id, name: "Study" },
    }
}

/// Reproduces the constructor-owned Java `justApplied` flag for powers whose
/// decay hooks observe it.
///
/// This is deliberately power-specific. Java does not derive the flag from a
/// generic `ApplyPowerAction`: Weak/Frail use their explicit
/// `isSourceMonster` constructor argument, Vulnerable additionally observes
/// whether Java's `GameActionManager.turnHasEnded` has become true, while
/// NoBlock's only live constructor passes `false`. Rust's `TurnTransition`
/// still contains the pre-`EndTurnAction` ethereal/discard window; only
/// `MonsterTurn`, including pre-turn powers, corresponds to the Java flag for
/// current natural combat actions. New applications must audit their Java
/// constructor instead of broadening a generic source heuristic.
pub fn initial_just_applied(
    id: PowerId,
    target: crate::EntityId,
    phase: crate::runtime::combat::CombatPhase,
) -> bool {
    use crate::runtime::combat::CombatPhase;

    match id {
        PowerId::Weak | PowerId::Frail => target == 0,
        PowerId::Vulnerable => target == 0 && phase == CombatPhase::MonsterTurn,
        PowerId::NoBlock => false,
        PowerId::DrawReduction | PowerId::Intangible => true,
        _ => false,
    }
}

impl crate::content::identity::GameplayIdentitySurface for PowerId {
    fn display_name_v1(self) -> Option<&'static str> {
        Some(get_power_definition(self).name)
    }
}

pub fn power_removes_at_zero(id: PowerId) -> bool {
    !matches!(id, PowerId::Strength | PowerId::Dexterity)
}

// Global Power Hook Routers
pub fn resolve_power_on_apply(id: PowerId, state: &mut CombatState, _target: crate::EntityId) {
    if id == PowerId::Corruption {
        ironclad::corruption::on_apply(state)
    }
}

pub fn resolve_power_on_card_draw(id: PowerId, state: &mut CombatState, card: &mut CombatCard) {
    match id {
        PowerId::Corruption => ironclad::corruption::on_card_draw(state, card),
        PowerId::Confusion => core::confusion::on_card_draw(state, card),
        _ => {}
    }
}

pub fn resolve_power_on_use_card(
    id: PowerId,
    state: &mut CombatState,
    card: &CombatCard,
    exhaust_override: &mut bool,
    purge: bool,
    target: Option<usize>,
) {
    match id {
        PowerId::Corruption => ironclad::corruption::on_use_card(state, card, exhaust_override),
        PowerId::DoubleTap => ironclad::double_tap::on_use_card(state, card, purge, target),
        PowerId::DuplicationPower => {
            core::duplication_power::on_use_card(state, card, purge, target)
        }
        PowerId::Amplify => defect::amplify::on_use_card(state, card, purge, target),
        PowerId::EchoForm => defect::echo_form::on_use_card(state, card, purge, target),
        PowerId::Heatsink => {
            let amount = store::power_amount(state, 0, PowerId::Heatsink);
            defect::heatsink::on_use_card(state, card, amount);
        }
        PowerId::Storm => {
            let amount = store::power_amount(state, 0, PowerId::Storm);
            defect::storm::on_use_card(state, card, amount);
        }
        PowerId::Burst => silent::burst::on_use_card(state, card, purge, target),
        PowerId::PenNibPower => {
            let actions = core::pen_nib::on_use_card(card);
            for action in actions {
                state.queue_action_back(action);
            }
        }
        PowerId::Vigor => {
            // Java: onUseCard — remove Vigor when an attack card is played
            let def = crate::content::cards::get_card_definition(card.id);
            if def.card_type == crate::content::cards::CardType::Attack {
                state.queue_action_back(crate::runtime::action::Action::RemovePower {
                    target: 0, // Player
                    power_id: PowerId::Vigor,
                });
            }
        }
        PowerId::FreeAttackPower => {
            let def = crate::content::cards::get_card_definition(card.id);
            if def.card_type == crate::content::cards::CardType::Attack && !purge {
                let remaining =
                    store::with_power_mut(state, 0, PowerId::FreeAttackPower, |power| {
                        power.amount -= 1;
                        power.amount
                    });
                if remaining.is_some_and(|amount| amount <= 0) {
                    state.queue_action_front(crate::runtime::action::Action::RemovePower {
                        target: 0,
                        power_id: PowerId::FreeAttackPower,
                    });
                }
            }
        }
        _ => {}
    }
}

pub fn resolve_power_on_player_card_played(
    id: PowerId,
    owner: crate::EntityId,
    amount: i32,
    card: &CombatCard,
    state: &CombatState,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::Curiosity => core::curiosity::on_player_card_played(owner, amount, card),
        PowerId::TimeWarp => core::time_warp::on_player_card_played(owner, amount, card, state),
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_exhaust(
    id: PowerId,
    _state: &CombatState,
    owner: crate::EntityId,
    amount: i32,
    _card_lineage: crate::runtime::combat::CardLineageId,
    _card_id: crate::content::cards::CardId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::FeelNoPain => ironclad::feel_no_pain::on_exhaust(owner, amount),
        PowerId::DarkEmbrace => ironclad::dark_embrace::on_exhaust(_state, amount),
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_hp_lost(
    id: PowerId,
    state: &CombatState,
    owner: crate::EntityId,
    hp_lost: i32,
    power_amount: i32,
    source: Option<crate::EntityId>,
    damage_type: crate::runtime::action::DamageType,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        // Java RupturePower.wasHPLost checks the DamageInfo owner after all
        // prevention, independent of damage type and amount lost per event.
        PowerId::Rupture if hp_lost > 0 && source == Some(owner) => {
            ironclad::rupture::on_hp_lost(owner, power_amount)
        }
        PowerId::Split => core::split::on_hp_lost(state, owner, hp_lost),
        PowerId::PlatedArmor => {
            core::plated_armor::on_hp_lost(state, owner, hp_lost, source, damage_type)
        }
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_card_played(
    id: PowerId,
    state: &CombatState,
    owner: crate::EntityId,
    card: &CombatCard,
    power_amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        // Java Gremlin Nob Enrage = AngerPower.onUseCard(SKILL).
        PowerId::Anger => core::anger::on_card_played(state, owner, card, power_amount),
        PowerId::Rage => core::rage::on_card_played(owner, card, power_amount),
        PowerId::SharpHide => core::sharp_hide::on_card_played(state, owner, card, power_amount),
        PowerId::Hex => core::hex::on_card_played(state, owner, card, power_amount),
        PowerId::ThousandCuts => {
            silent::thousand_cuts::on_after_card_played(state, owner, power_amount)
        }
        PowerId::AfterImage => silent::after_image::on_card_played(owner, power_amount, card),
        PowerId::Choked => silent::choked::on_card_played(state, owner, card, power_amount),
        PowerId::Curiosity => core::curiosity::on_player_card_played(owner, power_amount, card),
        PowerId::TimeWarp => {
            core::time_warp::on_player_card_played(owner, power_amount, card, state)
        }
        PowerId::Slow => core::slow::on_card_played(owner),
        PowerId::PanachePower => {
            let mut acts = smallvec::SmallVec::new();
            if power_amount == 1 {
                let damage = crate::content::powers::store::powers_for(state, owner)
                    .and_then(|ps| ps.iter().find(|p| p.power_type == PowerId::PanachePower))
                    .map(|p| p.extra_data)
                    .unwrap_or(10);
                acts.push(crate::runtime::action::Action::DamageAllEnemies {
                    source: owner,
                    damages: crate::runtime::action::repeated_damage_matrix(
                        state.entities.monsters.len(),
                        damage,
                    ),
                    damage_type: crate::runtime::action::DamageType::Thorns,
                    is_modified: false,
                });
                acts.push(crate::runtime::action::Action::ApplyPower {
                    source: owner,
                    target: owner,
                    power_id: PowerId::PanachePower,
                    amount: 4,
                });
            } else {
                acts.push(crate::runtime::action::Action::ApplyPower {
                    source: owner,
                    target: owner,
                    power_id: PowerId::PanachePower,
                    amount: -1,
                });
            }
            acts
        }
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_at_turn_start(
    id: PowerId,
    state: &mut CombatState,
    owner: crate::EntityId,
    amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    let power = crate::runtime::combat::Power {
        power_type: id,
        instance_id: None,
        amount,
        extra_data: 0,
        payload: crate::runtime::combat::PowerPayload::None,
        just_applied: false,
    };
    resolve_power_instance_at_turn_start(&power, state, owner)
}

pub fn resolve_power_instance_at_turn_start(
    power: &crate::runtime::combat::Power,
    state: &mut CombatState,
    owner: crate::EntityId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    let id = power.power_type;
    let amount = power.amount;
    match id {
        PowerId::FlameBarrier => {
            smallvec::smallvec![crate::runtime::action::Action::RemovePower {
                target: owner,
                power_id: id
            }]
        }
        PowerId::Berserk => {
            smallvec::smallvec![crate::runtime::action::Action::GainEnergy { amount }]
        }
        PowerId::NextTurnBlock => core::next_turn_block::at_turn_start(owner, amount),
        PowerId::WrathNextTurn => smallvec::smallvec![
            crate::runtime::action::Action::EnterStance("Wrath".to_string()),
            crate::runtime::action::Action::RemovePower {
                target: owner,
                power_id: PowerId::WrathNextTurn,
            },
        ],
        PowerId::InfiniteBladesPower => {
            if state.are_monsters_basically_dead_java() {
                smallvec::smallvec![]
            } else {
                smallvec::smallvec![
                    crate::content::cards::make_constructed_temp_card_in_hand_action(
                        crate::content::cards::CardId::Shiv,
                        amount.max(0) as u8,
                        false,
                        state,
                    )
                ]
            }
        }
        PowerId::BattleHymnPower => watcher::battle_hymn_at_turn_start(state, amount),
        PowerId::ForesightPower => watcher::foresight_at_turn_start(state, amount),
        PowerId::EnergyDownPower => {
            smallvec::smallvec![crate::runtime::action::Action::GainEnergy { amount: -amount }]
        }
        PowerId::EndTurnDeathPower => smallvec::smallvec![
            crate::runtime::action::Action::LoseHp {
                target: owner,
                amount: 99_999,
                triggers_rupture: true,
            },
            crate::runtime::action::Action::RemovePower {
                target: owner,
                power_id: PowerId::EndTurnDeathPower,
            },
        ],
        PowerId::MagnetismPower => {
            let mut acts = smallvec::SmallVec::new();
            if state.are_monsters_basically_dead_java() {
                return acts;
            }
            let pool = state.colorless_combat_pool();
            if pool.is_empty() {
                return acts;
            }
            for _ in 0..amount {
                let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
                let card =
                    crate::content::cards::prepare_make_temp_card_blueprint_in_hand_constructor(
                        crate::runtime::combat::CardBlueprint::fresh(pool[idx]),
                        state,
                    );
                acts.push(crate::runtime::action::Action::MakeConstructedCopyInHand {
                    original: Box::new(card),
                    amount: 1,
                });
            }
            acts
        }
        PowerId::MayhemPower => {
            let mut acts = smallvec::SmallVec::new();
            for _ in 0..amount {
                // Java Mayhem queues a wrapper before the turn draw; the wrapper
                // appends PlayTopCardAction when it runs, after DrawCardAction.
                acts.push(crate::runtime::action::Action::QueuePlayTopCardToBottom {
                    target: None,
                    exhaust: false,
                });
            }
            acts
        }
        PowerId::PanachePower => {
            if amount == 5 {
                smallvec::smallvec![]
            } else {
                smallvec::smallvec![crate::runtime::action::Action::ApplyPower {
                    source: owner,
                    target: owner,
                    power_id: PowerId::PanachePower,
                    amount: 5 - amount,
                }]
            }
        }
        PowerId::Bias => smallvec::smallvec![crate::runtime::action::Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::Focus,
            amount: -amount,
        }],
        PowerId::Loop => {
            if owner == 0 && amount > 0 {
                // Native LoopPower calls orb callbacks here, before the turn
                // draw is queued. A deferred wrapper would append Lightning
                // damage behind that draw and change cross-stream RNG order.
                crate::content::orbs::hooks::trigger_first_orb_start_and_end_now(state, amount);
            }
            smallvec::smallvec![]
        }
        PowerId::EchoForm => {
            if owner == 0 {
                let _ = store::with_power_mut(state, owner, PowerId::EchoForm, |power| {
                    power.extra_data = 0;
                });
            }
            smallvec::smallvec![]
        }
        PowerId::CreativeAI => {
            let mut acts = smallvec::SmallVec::new();
            for _ in 0..amount.max(0) {
                if let Some(card) = crate::content::cards::make_random_class_card_copy_for_combat(
                    state,
                    Some(crate::content::cards::CardType::Power),
                ) {
                    let card =
                        crate::content::cards::prepare_make_temp_card_blueprint_in_hand_constructor(
                            card, state,
                        );
                    acts.push(crate::runtime::action::Action::MakeConstructedCopyInHand {
                        original: Box::new(card),
                        amount: 1,
                    });
                }
            }
            acts
        }
        PowerId::Hello => {
            let mut acts = smallvec::SmallVec::new();
            if state.are_monsters_basically_dead_java() {
                return acts;
            }
            for _ in 0..amount.max(0) {
                if let Some(card) =
                    random_class_card_by_rarity(state, crate::content::cards::CardRarity::Common)
                {
                    let card =
                        crate::content::cards::prepare_make_temp_card_blueprint_in_hand_constructor(
                            card, state,
                        );
                    acts.push(crate::runtime::action::Action::MakeConstructedCopyInHand {
                        original: Box::new(card),
                        amount: 1,
                    });
                }
            }
            acts
        }
        PowerId::Poison => core::poison::at_turn_start(owner, power),
        PowerId::Choked => silent::choked::at_turn_start(owner),
        PowerId::Phantasmal => silent::phantasmal::at_start_of_turn(owner),
        PowerId::Nightmare => silent::nightmare::at_start_of_turn(owner, state, power),
        PowerId::Invincible => {
            let _ = store::with_power_mut(state, owner, PowerId::Invincible, |invincible| {
                invincible.amount = invincible.extra_data;
            });
            smallvec::smallvec![]
        }
        PowerId::Flight => core::flight::at_turn_start(state, owner, amount),
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_post_draw(
    id: PowerId,
    _state: &CombatState,
    owner: crate::EntityId,
    amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::Brutality => ironclad::brutality::on_post_draw(owner, amount),
        PowerId::DemonForm => ironclad::demon_form::on_post_draw(owner, amount),
        PowerId::NoxiousFumes => silent::noxious_fumes::on_post_draw(_state, owner, amount),
        PowerId::ToolsOfTheTrade => silent::tools_of_the_trade::on_post_draw(owner, amount),
        PowerId::DevotionPower => watcher::devotion_on_post_draw(owner, amount, _state),
        PowerId::DrawCardNextTurn => smallvec::smallvec![
            crate::runtime::action::Action::DrawCards(amount.max(0) as u32),
            crate::runtime::action::Action::RemovePower {
                target: owner,
                power_id: PowerId::DrawCardNextTurn,
            },
        ],
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_scry(
    id: PowerId,
    owner: crate::EntityId,
    amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::NirvanaPower => watcher::nirvana_on_scry(owner, amount),
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_change_stance(
    id: PowerId,
    owner: crate::EntityId,
    amount: i32,
    old_stance: crate::runtime::combat::StanceId,
    new_stance: crate::runtime::combat::StanceId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::MentalFortressPower => {
            watcher::mental_fortress_on_change_stance(owner, amount, old_stance, new_stance)
        }
        PowerId::RushdownPower => {
            watcher::rushdown_on_change_stance(owner, amount, old_stance, new_stance)
        }
        _ => smallvec::smallvec![],
    }
}

/// Java player powers with atEndOfTurnPreEndTurnCards rather than atEndOfTurn.
/// Monsters run both callbacks together; players have a card-queue boundary.
pub fn end_turn_power_precedes_cards(id: PowerId) -> bool {
    matches!(
        id,
        PowerId::Metallicize | PowerId::PlatedArmor | PowerId::LikeWaterPower
    )
}

/// Read-only requests from the player powers that run before end-turn cards.
/// Shared with the ordinary callback; this does not settle their actions.
pub fn resolve_power_pre_end_turn_cards(
    power: &crate::runtime::combat::Power,
    state: &CombatState,
    owner: crate::EntityId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match power.power_type {
        PowerId::Metallicize => ironclad::metallicize::at_end_of_turn(owner, power.amount),
        PowerId::PlatedArmor => {
            core::plated_armor::on_monster_turn_ended(state, owner, power.amount)
        }
        PowerId::LikeWaterPower => watcher::like_water_at_end_of_turn(state, owner, power.amount),
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_at_end_of_turn(
    power: &crate::runtime::combat::Power,
    _state: &mut CombatState,
    owner: crate::EntityId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    if end_turn_power_precedes_cards(power.power_type) {
        return resolve_power_pre_end_turn_cards(power, _state, owner);
    }
    let id = power.power_type;
    let amount = power.amount;
    match id {
        PowerId::DoubleTap => smallvec::smallvec![crate::runtime::action::Action::RemovePower {
            target: owner,
            power_id: PowerId::DoubleTap,
        }],
        PowerId::Burst => smallvec::smallvec![crate::runtime::action::Action::RemovePower {
            target: owner,
            power_id: PowerId::Burst,
        }],
        PowerId::Amplify => smallvec::smallvec![crate::runtime::action::Action::RemovePower {
            target: owner,
            power_id: PowerId::Amplify,
        }],
        PowerId::Combust => ironclad::combust::at_end_of_turn(_state, owner, amount),
        PowerId::EstablishmentPower => {
            smallvec::smallvec![crate::runtime::action::Action::ReduceRetainedHandCosts { amount }]
        }
        PowerId::Entangle => core::entangle::at_end_of_turn(owner),
        PowerId::Malleable => core::malleable::on_monster_turn_ended(_state, owner, amount),
        PowerId::Thievery => core::thievery::on_monster_turn_ended(_state, owner, amount),
        PowerId::Regrow => core::regrow::at_end_of_turn(owner, amount),
        PowerId::Constricted => core::constricted::at_end_of_turn(owner, power),
        PowerId::LoseStrength => core::lose_strength::at_end_of_turn(owner, amount),
        PowerId::Rage => smallvec::smallvec![crate::runtime::action::Action::RemovePower {
            target: owner,
            power_id: PowerId::Rage,
        }],
        PowerId::DexterityDown => core::dexterity_down::at_end_of_turn(owner, amount),
        PowerId::WraithForm => silent::wraith_form::at_end_of_turn(owner, amount),
        PowerId::Equilibrium => {
            // Java marks the current hand inside the callback, before any
            // queued end-turn effects (including Establishment) resolve.
            if owner == 0 {
                for card in &mut _state.zones.hand {
                    if !crate::content::cards::is_ethereal(card) {
                        card.retain_override = Some(true);
                    }
                }
            }
            smallvec::smallvec![]
        }
        PowerId::RetainCards => silent::retain_cards::at_end_of_turn(_state, owner, amount),
        PowerId::Study => {
            smallvec::smallvec![crate::runtime::action::Action::MakeTempCardInDrawPile {
                card_id: crate::content::cards::CardId::Insight,
                amount: amount.max(0).min(u8::MAX as i32) as u8,
                random_spot: true,
                to_bottom: false,
                upgraded: false,
            }]
        }
        PowerId::OmegaPower => {
            if owner == 0 {
                smallvec::smallvec![crate::runtime::action::Action::DamageAllEnemies {
                    source: crate::runtime::action::NO_SOURCE,
                    damages: crate::runtime::action::repeated_damage_matrix(
                        _state.entities.monsters.len(),
                        amount.max(0),
                    ),
                    damage_type: crate::runtime::action::DamageType::Thorns,
                    is_modified: false,
                }]
            } else {
                smallvec::smallvec![]
            }
        }
        PowerId::Rebound => smallvec::smallvec![crate::runtime::action::Action::RemovePower {
            target: owner,
            power_id: PowerId::Rebound,
        }],
        PowerId::NoDraw => core::no_draw::at_end_of_turn(owner),
        PowerId::CannotChangeStance => {
            smallvec::smallvec![crate::runtime::action::Action::RemovePower {
                target: owner,
                power_id: PowerId::CannotChangeStance,
            }]
        }
        PowerId::Ritual => core::ritual::at_end_of_turn(owner, amount, power.extra_data),
        PowerId::Shackled => core::shackled::at_end_of_turn(owner, amount),
        PowerId::Shifting => core::shifting::at_end_of_turn(owner),
        PowerId::Intangible => core::intangible::at_end_of_turn(owner, amount, power.just_applied),
        PowerId::TheBombPower => {
            let mut acts = smallvec::SmallVec::new();
            if amount == 1 {
                if let Some(instance_id) = power.instance_id {
                    acts.push(crate::runtime::action::Action::ReducePowerInstance {
                        target: owner,
                        power_id: PowerId::TheBombPower,
                        instance_id,
                        amount: 1,
                    });
                } else {
                    acts.push(crate::runtime::action::Action::ReducePower {
                        target: owner,
                        power_id: PowerId::TheBombPower,
                        amount: 1,
                    });
                }
                acts.push(crate::runtime::action::Action::DamageAllEnemies {
                    source: owner,
                    damages: crate::runtime::action::repeated_damage_matrix(
                        _state.entities.monsters.len(),
                        power.extra_data.max(0),
                    ),
                    damage_type: crate::runtime::action::DamageType::Thorns,
                    is_modified: false,
                });
            } else if let Some(instance_id) = power.instance_id {
                acts.push(crate::runtime::action::Action::ReducePowerInstance {
                    target: owner,
                    power_id: PowerId::TheBombPower,
                    instance_id,
                    amount: 1,
                });
            } else {
                acts.push(crate::runtime::action::Action::ReducePower {
                    target: owner,
                    power_id: PowerId::TheBombPower,
                    amount: 1,
                });
            }
            acts
        }
        PowerId::Regen => {
            // Java: RegenerateMonsterPower.atEndOfTurn() only heals; it does not tick down.
            let mut acts = smallvec::SmallVec::new();
            acts.push(crate::runtime::action::Action::Heal {
                target: owner,
                amount,
            });
            acts
        }
        PowerId::Regeneration => {
            // Java: RegenPower.atEndOfTurn() -> RegenAction(heal), then decrement/remove Regeneration.
            let mut acts = smallvec::SmallVec::new();
            acts.push(crate::runtime::action::Action::Heal {
                target: owner,
                amount,
            });
            acts.push(crate::runtime::action::Action::ApplyPower {
                source: owner,
                target: owner,
                power_id: PowerId::Regeneration,
                amount: -1,
            });
            acts
        }
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_during_turn(
    power: &crate::runtime::combat::Power,
    owner: crate::EntityId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match power.power_type {
        PowerId::Explosive => core::explosive::during_turn(owner, power.amount),
        PowerId::Fading => core::fading::during_turn(owner, power.amount),
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_at_end_of_round(
    id: PowerId,
    _state: &CombatState,
    owner: crate::EntityId,
    amount: i32,
    just_applied: bool,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::DuplicationPower => {
            if amount == 0 {
                smallvec::smallvec![crate::runtime::action::Action::RemovePower {
                    target: owner,
                    power_id: PowerId::DuplicationPower,
                }]
            } else {
                smallvec::smallvec![crate::runtime::action::Action::ReducePower {
                    target: owner,
                    power_id: PowerId::DuplicationPower,
                    amount: 1,
                }]
            }
        }
        PowerId::Vulnerable => core::vulnerable::at_end_of_round(owner, amount, just_applied),
        PowerId::Weak => core::weak::at_end_of_round(owner, amount, just_applied),
        PowerId::Frail => core::frail::at_end_of_round(owner, amount, just_applied),
        PowerId::NoBlock => core::no_block::at_end_of_round(owner, amount, just_applied),
        PowerId::DrawReduction => {
            core::draw_reduction::at_end_of_round(owner, amount, just_applied)
        }
        PowerId::Blur => silent::blur::at_end_of_round(owner, amount),
        PowerId::WaveOfTheHandPower => {
            smallvec::smallvec![crate::runtime::action::Action::RemovePower {
                target: owner,
                power_id: PowerId::WaveOfTheHandPower,
            }]
        }
        PowerId::Equilibrium => {
            if amount == 0 {
                smallvec::smallvec![crate::runtime::action::Action::RemovePower {
                    target: owner,
                    power_id: PowerId::Equilibrium,
                }]
            } else {
                smallvec::smallvec![crate::runtime::action::Action::ReducePower {
                    target: owner,
                    power_id: PowerId::Equilibrium,
                    amount: 1,
                }]
            }
        }
        PowerId::LockOn => {
            if amount == 0 {
                smallvec::smallvec![crate::runtime::action::Action::RemovePower {
                    target: owner,
                    power_id: PowerId::LockOn,
                }]
            } else {
                smallvec::smallvec![crate::runtime::action::Action::ReducePower {
                    target: owner,
                    power_id: PowerId::LockOn,
                    amount: 1,
                }]
            }
        }
        PowerId::Slow => core::slow::at_end_of_round(owner, amount),
        PowerId::IntangiblePlayer => core::intangible::at_end_of_round(owner, amount),
        PowerId::Ritual => core::ritual::at_end_of_round(_state, owner, amount),
        PowerId::GenericStrengthUp => core::generic_strength_up::at_end_of_round(owner, amount),
        PowerId::DoubleDamage => silent::phantasmal::double_damage_at_end_of_round(owner, amount),
        // Other Powers that decay at end of round can be added here
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_victory(
    id: PowerId,
    state: &CombatState,
    owner: crate::EntityId,
    amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::Repair if owner == 0 && state.entities.player.current_hp > 0 => {
            smallvec::smallvec![crate::runtime::action::Action::Heal {
                target: owner,
                amount,
            }]
        }
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_card_drawn(
    id: PowerId,
    state: &CombatState,
    owner: crate::EntityId,
    amount: i32,
    card_lineage: crate::runtime::combat::CardLineageId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    if let Some(card) = state.zones.hand.iter().find(|c| c.lineage == card_lineage) {
        card_draw_trigger_requests(id, state, owner, amount, card.id)
    } else {
        smallvec::smallvec![]
    }
}

/// Effect requests conditioned on drawing this card kind. This does not draw
/// the card, establish access to it, or settle the resulting queued actions.
/// Runtime dispatch and conditional capability analysis share these rules.
pub fn card_draw_trigger_requests(
    id: PowerId,
    state: &CombatState,
    owner: crate::EntityId,
    amount: i32,
    card_id: crate::content::cards::CardId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::Evolve => ironclad::evolve::on_card_drawn(state, owner, card_id, amount),
        PowerId::FireBreathing => {
            ironclad::fire_breathing::on_card_drawn(state, owner, card_id, amount)
        }
        _ => smallvec::smallvec![],
    }
}

// -------------------------------------------------------------
// On Attack Modifiers
// -------------------------------------------------------------

pub fn resolve_power_on_inflict_damage(
    id: PowerId,
    damage: i32,
    damage_type: crate::runtime::action::DamageType,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::PainfulStabs => core::painful_stabs::on_inflict_damage(damage, damage_type),
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_attack(
    id: PowerId,
    owner: crate::EntityId,
    target: crate::EntityId,
    damage: i32,
    damage_type: crate::runtime::action::DamageType,
    power_amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::Envenom => {
            silent::envenom::on_attack(owner, target, damage, damage_type, power_amount)
        }
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_block_gained(
    id: PowerId,
    _state: &CombatState,
    _owner: crate::EntityId,
    amount: i32,
    _block_amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::Juggernaut => ironclad::juggernaut::on_block_gained(amount),
        PowerId::WaveOfTheHandPower => {
            watcher::wave_of_the_hand_on_block_gained(_state, _owner, amount, _block_amount)
        }
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_attacked(
    id: PowerId,
    state: &CombatState,
    owner: crate::EntityId,
    damage: i32,
    source: crate::EntityId,
    damage_type: crate::runtime::action::DamageType,
    power_amount: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::Angry => {
            core::angry::on_attacked(state, owner, damage, source, damage_type, power_amount)
        }
        PowerId::FlameBarrier => {
            ironclad::flame_barrier::on_attacked(owner, source, damage_type, power_amount)
        }
        PowerId::CurlUp => core::curl_up::on_attacked(state, owner, damage, source, power_amount),
        // SharpHide: moved from on_attacked to on_card_played (Java uses onUseCard, not onAttacked)
        PowerId::Flight => {
            core::flight::on_attacked(state, owner, damage, source, damage_type, power_amount)
        }
        PowerId::Malleable => core::malleable::on_attacked(state, owner, damage, power_amount),
        PowerId::StaticDischarge => {
            defect::static_discharge::on_attacked(owner, source, damage, damage_type, power_amount)
        }
        PowerId::BlockReturnPower => {
            if damage_type != crate::runtime::action::DamageType::Thorns
                && damage_type != crate::runtime::action::DamageType::HpLoss
                && source != crate::runtime::action::NO_SOURCE
                && source != owner
            {
                smallvec::smallvec![crate::runtime::action::Action::GainBlock {
                    target: 0,
                    amount: power_amount,
                }]
            } else {
                smallvec::smallvec![]
            }
        }
        PowerId::Thorns => {
            core::thorns::on_attacked(state, owner, damage, source, damage_type, power_amount)
        }
        PowerId::Shifting => {
            core::shifting::on_attacked(state, owner, damage, source, power_amount)
        }
        PowerId::Reactive => {
            core::reactive::on_attacked(state, owner, damage, source, damage_type, power_amount)
        }
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_death(
    id: PowerId,
    state: &CombatState,
    owner: crate::EntityId,
    amount: i32,
    extra_data: i32,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::SporeCloud => core::spore_cloud::on_death(state, owner, amount),
        PowerId::CorpseExplosion => silent::corpse_explosion::on_death(state, owner, amount),
        PowerId::Stasis => core::stasis::on_death(state, owner, extra_data),
        PowerId::Unawakened => core::unawakened::on_death(owner, amount),
        PowerId::Shackled => smallvec::smallvec![],
        PowerId::DrawReduction => smallvec::smallvec![],
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_remove(
    id: PowerId,
    state: &CombatState,
    owner: crate::EntityId,
) -> smallvec::SmallVec<[crate::runtime::action::Action; 2]> {
    match id {
        PowerId::Flight => core::flight::on_remove(state, owner),
        PowerId::PlatedArmor => core::plated_armor::on_remove(state, owner),
        _ => smallvec::smallvec![],
    }
}

pub fn resolve_power_on_attack_to_change_damage(
    id: PowerId,
    _state: &CombatState,
    _info: &crate::runtime::action::DamageInfo,
    current_damage: i32,
    amount: i32,
) -> i32 {
    match id {
        PowerId::Strength => core::strength::on_attack_to_change_damage(current_damage, amount),
        _ => current_damage,
    }
}

pub fn resolve_power_on_attacked_to_change_damage(
    id: PowerId,
    state: &mut CombatState,
    info: &crate::runtime::action::DamageInfo,
    current_damage: i32,
    _amount: i32,
) -> i32 {
    match id {
        PowerId::Buffer => {
            if current_damage > 0 {
                let target = info.target;
                let Some(remaining) =
                    store::with_power_mut(state, target, PowerId::Buffer, |power| {
                        power.amount -= 1;
                        power.amount
                    })
                else {
                    return current_damage;
                };
                if !should_keep_power_instance(PowerId::Buffer, remaining) {
                    store::remove_power_type(state, target, PowerId::Buffer);
                }
                0
            } else {
                current_damage
            }
        }
        PowerId::Invincible => {
            let target = info.target;
            let capped_damage =
                store::with_power_mut(state, target, PowerId::Invincible, |power| {
                    let capped = current_damage.min(power.amount).max(0);
                    power.amount = (power.amount - capped).max(0);
                    capped
                });
            capped_damage.unwrap_or(current_damage)
        }
        _ => current_damage,
    }
}

pub fn resolve_power_on_calculate_damage_to_enemy(
    id: PowerId,
    _state: &CombatState,
    card: &CombatCard,
    damage: f32,
    amount: i32,
) -> f32 {
    let def = crate::content::cards::get_card_definition(card.id);
    if def.card_type == crate::content::cards::CardType::Attack {
        match id {
            PowerId::Strength => core::strength::on_calculate_damage_to_enemy(
                card.id,
                card.base_magic_num_mut,
                damage,
                amount,
            ),
            PowerId::PenNibPower => core::pen_nib::on_calculate_damage_to_enemy(damage),
            PowerId::DoubleDamage => damage * 2.0,
            PowerId::Weak => damage * 0.75,
            // Java: atDamageGive — Vigor adds its amount to NORMAL attack damage
            PowerId::Vigor => damage + amount as f32,
            PowerId::AccuracyPower if card.id == crate::content::cards::CardId::Shiv => {
                damage + amount as f32
            }
            _ => damage,
        }
    } else {
        damage
    }
}

pub fn resolve_power_on_calculate_block(
    id: PowerId,
    _state: &CombatState,
    card: &CombatCard,
    block: f32,
    amount: i32,
) -> f32 {
    let def = crate::content::cards::get_card_definition(card.id);
    if def.base_block > 0 || def.card_type == crate::content::cards::CardType::Skill {
        match id {
            PowerId::Dexterity => core::dexterity::on_calculate_block(block, amount),
            PowerId::Frail => core::frail::on_calculate_block(block, amount),
            PowerId::NoBlock => core::no_block::on_calculate_block(block, amount),
            _ => block,
        }
    } else {
        block
    }
}

pub fn resolve_power_on_calculate_damage_from_player(
    id: PowerId,
    _state: &CombatState,
    card: &CombatCard,
    _target_id: crate::EntityId,
    damage: f32,
    amount: i32,
) -> f32 {
    let mut updated_damage = damage;
    let def = crate::content::cards::get_card_definition(card.id);

    // Slow modifies damage dynamically regardless of attack type, but only on normal damage hits usually
    if def.card_type == crate::content::cards::CardType::Attack {
        updated_damage = match id {
            PowerId::Vulnerable => {
                let multiplier = crate::content::relics::hooks::on_calculate_vulnerable_multiplier(
                    _state, false,
                );
                core::vulnerable::on_calculate_damage_from_player(
                    updated_damage,
                    amount,
                    multiplier,
                )
            }
            PowerId::Slow => core::slow::on_calculate_damage_from_player(
                _state,
                card,
                _target_id,
                updated_damage,
                amount,
            ),
            _ => updated_damage,
        };
    }

    updated_damage
}

/// Java: AbstractMonster.calculateDamage() pipeline
/// Evaluates a monster's Normal attack from its base truth input.
///
/// Contract:
/// - `base` is the monster plan truth value supplied by `Action::MonsterAttack`.
/// - Monster content code must not precompute a modified final damage output.
/// - This function applies the shared monster attack pipeline: Strength, Weak,
///   Vulnerable, player stance, and final receive effects such as Intangible.
pub fn calculate_monster_damage(
    base: i32,
    source_id: usize,
    target_id: usize,
    state: &CombatState,
) -> i32 {
    let mut tmp = base as f32;

    // 1. Monster powers (Strength, Weak, etc.) atDamageGive
    if let Some(owner_powers) = crate::content::powers::store::powers_for(state, source_id) {
        for p in owner_powers {
            tmp = match p.power_type {
                PowerId::Strength => {
                    core::strength::on_attack_to_change_damage(tmp as i32, p.amount) as f32
                } // Strength is linear addition
                PowerId::Weak => {
                    if source_id != 0
                        && target_id == 0
                        && state
                            .entities
                            .player
                            .has_relic(crate::content::relics::RelicId::PaperCrane)
                    {
                        tmp * crate::content::relics::paper_crane::WEAK_MULTIPLIER
                    } else {
                        tmp * 0.75
                    }
                }
                _ => tmp,
            };
        }
    }

    // 2. Target powers (Vulnerable, Intangible, etc.) atDamageReceive
    if let Some(target_powers) = crate::content::powers::store::powers_for(state, target_id) {
        for p in target_powers {
            tmp = match p.power_type {
                PowerId::Vulnerable => {
                    let multiplier = if target_id == 0 {
                        crate::content::relics::hooks::on_calculate_vulnerable_multiplier(
                            state, true,
                        )
                    } else {
                        1.5
                    };
                    core::vulnerable::on_calculate_damage_from_player(tmp, p.amount, multiplier)
                    // Same logic for monster vs player vulnerability
                }
                _ => tmp,
            };
        }
    }

    // 3. Player stance atDamageReceive
    if target_id == 0 && state.entities.player.stance == crate::runtime::combat::StanceId::Wrath {
        tmp *= 2.0;
    }

    // 4. Java `AbstractMonster.calculateDamage()` calls `applyBackAttack()`,
    // which reads Surrounded + player facing + monster position directly.
    // The BackAttackPower icon is a separately refreshed, potentially stale
    // UI marker and must not drive this multiplier.
    if target_id == 0
        && crate::content::powers::core::surrounded::monster_has_back_attack_from_facing(
            state, source_id,
        )
    {
        tmp *= 1.5;
    }

    // 5. Target powers atDamageFinalReceive (Intangible)
    if let Some(target_powers) = crate::content::powers::store::powers_for(state, target_id) {
        for p in target_powers {
            tmp = match p.power_type {
                PowerId::Intangible | PowerId::IntangiblePlayer => {
                    core::intangible::at_damage_final_receive(
                        tmp as i32,
                        p.amount,
                        crate::runtime::action::DamageType::Normal,
                    ) as f32
                }
                PowerId::Flight => core::flight::at_damage_final_receive(
                    tmp as i32,
                    p.amount,
                    crate::runtime::action::DamageType::Normal,
                ) as f32,
                _ => tmp,
            };
        }
    }

    let dmg = tmp.floor() as i32;
    dmg.max(0)
}

/// Java: AbstractPower.canPlayCard(AbstractCard) — returns false to block card play.
/// Called in AbstractCard.hasEnoughEnergy() for each power on the player.
/// Currently only NoSkillsPower (Watcher) overrides this in Java.
pub fn resolve_power_can_play_card(id: PowerId, card: &CombatCard) -> bool {
    match id {
        PowerId::NoSkills => {
            let def = crate::content::cards::get_card_definition(card.id);
            def.card_type != crate::content::cards::CardType::Skill
        }
        _ => true,
    }
}

/// Java: AbstractPower.onApplyPower(power, target, source)
/// Called on each power of the SOURCE entity when a power is applied.
/// Call site: ApplyPowerAction.update() — after the power is applied to target.
/// Only override: SadisticPower — deal THORNS damage when owner applies a debuff.
pub fn resolve_power_on_apply_power(
    id: PowerId,
    owner_amount: i32,
    applied_power_id: PowerId,
    applied_amount: i32,
    target: usize,
    source: usize,
    state: &CombatState,
) -> smallvec::SmallVec<[crate::runtime::action::ActionInfo; 2]> {
    let mut actions = smallvec::SmallVec::new();
    if id == PowerId::SadisticPower {
        // Java SadisticPower.onApplyPower:
        //   if (power.type == DEBUFF && !power.ID.equals("Shackled")
        //       && source == this.owner && target != this.owner
        //       && !target.hasPower("Artifact"))
        //     → addToBot(DamageAction(target, amount, THORNS))
        if is_debuff_application(applied_power_id, applied_amount)
            && target != source
            && !crate::content::powers::store::has_power(state, target, PowerId::Artifact)
        {
            actions.push(crate::runtime::action::ActionInfo {
                action: crate::runtime::action::Action::Damage(
                    crate::runtime::action::DamageInfo {
                        source,
                        target,
                        base: owner_amount,
                        output: owner_amount,
                        damage_type: crate::runtime::action::DamageType::Thorns,
                        is_modified: true,
                    },
                ),
                insertion_mode: crate::runtime::action::AddTo::Bottom,
            });
        }
    }
    actions
}

/// Java: AbstractPower.atDamageFinalReceive(damage, damageType)
/// Used exclusively for Intangible and Flight.
pub fn resolve_power_at_damage_final_receive(
    id: PowerId,
    damage: i32,
    amount: i32,
    damage_type: crate::runtime::action::DamageType,
) -> i32 {
    match id {
        PowerId::Intangible | PowerId::IntangiblePlayer => {
            core::intangible::at_damage_final_receive(damage, amount, damage_type)
        }
        PowerId::Flight => core::flight::at_damage_final_receive(damage, amount, damage_type),
        _ => damage,
    }
}

#[cfg(test)]
mod java_decay_tests {
    use super::*;
    use crate::runtime::action::{Action, DamageInfo, DamageType};
    use crate::runtime::combat::{Power, PowerPayload};

    fn power(power_type: PowerId, amount: i32, just_applied: bool) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied,
        }
    }

    #[test]
    fn turn_based_round_cleanup_uses_java_reduce_power_actions() {
        let state = crate::test_support::blank_test_combat();

        for power_id in [
            PowerId::Vulnerable,
            PowerId::Weak,
            PowerId::Frail,
            PowerId::NoBlock,
            PowerId::DrawReduction,
            PowerId::DuplicationPower,
            PowerId::IntangiblePlayer,
        ] {
            let actions = resolve_power_at_end_of_round(power_id, &state, 0, 1, false);
            assert_eq!(
                actions.as_slice(),
                &[Action::ReducePower {
                    target: 0,
                    power_id,
                    amount: 1,
                }],
                "{power_id:?} should decay through Java ReducePowerAction"
            );
        }
    }

    #[test]
    fn zero_amount_round_cleanup_uses_java_remove_specific_power_actions() {
        let state = crate::test_support::blank_test_combat();

        for power_id in [
            PowerId::Vulnerable,
            PowerId::Weak,
            PowerId::Frail,
            PowerId::NoBlock,
            PowerId::DuplicationPower,
            PowerId::IntangiblePlayer,
        ] {
            let actions = resolve_power_at_end_of_round(power_id, &state, 0, 0, false);
            assert_eq!(
                actions.as_slice(),
                &[Action::RemovePower {
                    target: 0,
                    power_id,
                }],
                "{power_id:?} amount == 0 should map to Java RemoveSpecificPowerAction"
            );
        }
    }

    #[test]
    fn just_applied_round_cleanup_skips_java_turn_based_debuffs() {
        let state = crate::test_support::blank_test_combat();

        for power_id in [
            PowerId::Vulnerable,
            PowerId::Weak,
            PowerId::Frail,
            PowerId::NoBlock,
            PowerId::DrawReduction,
        ] {
            let actions = resolve_power_at_end_of_round(power_id, &state, 0, 1, true);
            assert!(
                actions.is_empty(),
                "{power_id:?} justApplied should skip this round"
            );
        }
    }

    #[test]
    fn monster_intangible_respects_java_just_applied_and_reduce_power() {
        let mut state = crate::test_support::blank_test_combat();

        let just_applied = power(PowerId::Intangible, 1, true);
        assert!(resolve_power_at_end_of_turn(&just_applied, &mut state, 1).is_empty());

        let decaying = power(PowerId::Intangible, 1, false);
        let actions = resolve_power_at_end_of_turn(&decaying, &mut state, 1);
        assert_eq!(
            actions.as_slice(),
            &[Action::ReducePower {
                target: 1,
                power_id: PowerId::Intangible,
                amount: 1,
            }]
        );
    }

    #[test]
    fn monster_back_attack_damage_uses_surrounded_facing_not_stale_visible_marker() {
        let mut shield =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::SpireShield);
        shield.id = 1;
        shield.logical_position = -1;
        let mut spear =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::SpireSpear);
        spear.id = 2;
        spear.logical_position = 1;
        let mut state = crate::test_support::combat_with_monsters(vec![shield, spear]);
        crate::content::powers::store::set_powers_for(
            &mut state,
            0,
            vec![power(PowerId::Surrounded, -1, false)],
        );
        crate::content::powers::store::set_powers_for(
            &mut state,
            1,
            vec![power(PowerId::BackAttack, -1, false)],
        );

        assert_eq!(calculate_monster_damage(10, 1, 0, &state), 15);
        assert_eq!(calculate_monster_damage(10, 2, 0, &state), 10);

        state.entities.player.facing_left = true;

        assert_eq!(calculate_monster_damage(10, 1, 0, &state), 10);
        assert_eq!(calculate_monster_damage(10, 2, 0, &state), 15);
    }

    #[test]
    fn explosive_and_fading_countdowns_match_java_during_turn_action_order() {
        let mut state = crate::test_support::blank_test_combat();

        let explosive_two = power(PowerId::Explosive, 2, false);
        assert!(resolve_power_at_end_of_turn(&explosive_two, &mut state, 1).is_empty());
        let actions = resolve_power_during_turn(&explosive_two, 1);
        assert_eq!(
            actions.as_slice(),
            &[Action::ReducePower {
                target: 1,
                power_id: PowerId::Explosive,
                amount: 1,
            }]
        );

        let explosive_one = power(PowerId::Explosive, 1, false);
        let actions = resolve_power_during_turn(&explosive_one, 1);
        assert_eq!(
            actions.as_slice(),
            &[
                Action::Suicide {
                    target: 1,
                    trigger_relics: true,
                },
                Action::Damage(DamageInfo {
                    source: 1,
                    target: 0,
                    base: 30,
                    output: 30,
                    damage_type: DamageType::Thorns,
                    is_modified: false,
                }),
            ],
            "Java ExplosivePower queues SuicideAction before DamageAction"
        );

        let fading_two = power(PowerId::Fading, 2, false);
        assert!(resolve_power_at_end_of_turn(&fading_two, &mut state, 1).is_empty());
        let actions = resolve_power_during_turn(&fading_two, 1);
        assert_eq!(
            actions.as_slice(),
            &[Action::ReducePower {
                target: 1,
                power_id: PowerId::Fading,
                amount: 1,
            }]
        );
    }

    #[test]
    fn poison_uses_atomic_java_poison_lose_hp_action() {
        let actions = resolve_power_at_turn_start(
            PowerId::Poison,
            &mut crate::test_support::blank_test_combat(),
            70,
            3,
        );

        assert_eq!(
            actions.as_slice(),
            &[Action::PoisonLoseHp {
                source: crate::runtime::action::NO_SOURCE,
                target: 70,
                amount: 3,
            }],
            "Java PoisonLoseHpAction combines damage and poison countdown before post-combat cleanup"
        );
    }
}
