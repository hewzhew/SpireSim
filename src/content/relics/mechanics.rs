//! Reviewed relic rules shared by runtime hooks and acquisition facts.
use super::RelicId;
use crate::content::powers::PowerId;
use serde::{Deserialize, Serialize};

/// Complete reviewed rules bind text at the execution owner. Presentation
/// language never changes effect values, counters or acquisition identity.
#[derive(Clone, Copy, Debug)]
pub struct RelicMechanics {
    pub text_id: RelicId,
}

impl RelicMechanics {
    pub fn explanation(self, locale: sts_presentation::Locale) -> String {
        locale
            .definition_text("game", &format!("relicRule.{}", self.text_id.stable_id()))
            .expect("complete relic mechanism text")
    }
}

pub fn get(relic: RelicId) -> Option<RelicMechanics> {
    Some(match relic {
        RelicId::Astrolabe => RelicMechanics {
            text_id: RelicId::Astrolabe,
        },
        RelicId::Pantograph => RelicMechanics {
            text_id: RelicId::Pantograph,
        },
        RelicId::IncenseBurner => super::incense_burner::MECHANICS,
        RelicId::ChampionBelt => super::champion_belt::MECHANICS,
        RelicId::EternalFeather => super::eternal_feather::MECHANICS,
        RelicId::AncientTeaSet => RelicMechanics {
            text_id: RelicId::AncientTeaSet,
        },
        RelicId::Lantern => super::lantern::MECHANICS,
        RelicId::Anchor => RelicMechanics {
            text_id: RelicId::Anchor,
        },
        RelicId::ArtOfWar => super::art_of_war::MECHANICS,
        RelicId::Brimstone => super::brimstone::MECHANICS,
        RelicId::Pocketwatch => super::pocketwatch::MECHANICS,
        RelicId::LizardTail => RelicMechanics {
            text_id: RelicId::LizardTail,
        },
        RelicId::GremlinHorn => RelicMechanics {
            text_id: RelicId::GremlinHorn,
        },
        RelicId::HandDrill => RelicMechanics {
            text_id: RelicId::HandDrill,
        },
        RelicId::StrikeDummy => super::strike_dummy::MECHANICS,
        RelicId::Necronomicon => RelicMechanics {
            text_id: RelicId::Necronomicon,
        },
        RelicId::Enchiridion => RelicMechanics {
            text_id: RelicId::Enchiridion,
        },
        RelicId::NilrysCodex => RelicMechanics {
            text_id: RelicId::NilrysCodex,
        },
        RelicId::Torii => RelicMechanics {
            text_id: RelicId::Torii,
        },
        RelicId::TungstenRod => super::tungsten_rod::MECHANICS,
        RelicId::UnceasingTop => RelicMechanics {
            text_id: RelicId::UnceasingTop,
        },
        RelicId::RegalPillow => RelicMechanics {
            text_id: RelicId::RegalPillow,
        },
        RelicId::ChemicalX => RelicMechanics {
            text_id: RelicId::ChemicalX,
        },
        RelicId::Sling => RelicMechanics {
            text_id: RelicId::Sling,
        },
        RelicId::BloodyIdol => RelicMechanics {
            text_id: RelicId::BloodyIdol,
        },
        RelicId::MummifiedHand => RelicMechanics {
            text_id: RelicId::MummifiedHand,
        },
        RelicId::Waffle => RelicMechanics {
            text_id: RelicId::Waffle,
        },
        RelicId::SingingBowl => super::singing_bowl::MECHANICS,
        RelicId::FrozenEgg => RelicMechanics {
            text_id: RelicId::FrozenEgg,
        },
        RelicId::MoltenEgg => RelicMechanics {
            text_id: RelicId::MoltenEgg,
        },
        RelicId::ToxicEgg => RelicMechanics {
            text_id: RelicId::ToxicEgg,
        },
        RelicId::Courier => super::courier::MECHANICS,
        // Prices are owned by state::shop::shop_screen and engine::shop_handler.
        RelicId::MembershipCard => RelicMechanics {
            text_id: RelicId::MembershipCard,
        },
        RelicId::MagicFlower => super::magic_flower::MECHANICS,
        // Played-card destination is resolved in cards::play_queue::handle_use_card_done.
        RelicId::StrangeSpoon => RelicMechanics {
            text_id: RelicId::StrangeSpoon,
        },
        RelicId::PreservedInsect => RelicMechanics {
            text_id: RelicId::PreservedInsect,
        },
        RelicId::BottledFlame => RelicMechanics {
            text_id: RelicId::BottledFlame,
        },
        RelicId::BottledLightning => RelicMechanics {
            text_id: RelicId::BottledLightning,
        },
        RelicId::BottledTornado => RelicMechanics {
            text_id: RelicId::BottledTornado,
        },
        RelicId::WarpedTongs => RelicMechanics {
            text_id: RelicId::WarpedTongs,
        },
        RelicId::OrangePellets => RelicMechanics {
            text_id: RelicId::OrangePellets,
        },
        RelicId::Ectoplasm => RelicMechanics {
            text_id: RelicId::Ectoplasm,
        },
        RelicId::TinyHouse => RelicMechanics {
            text_id: RelicId::TinyHouse,
        },
        RelicId::RunicCube => RelicMechanics {
            text_id: RelicId::RunicCube,
        },
        RelicId::RunicDome => RelicMechanics {
            text_id: RelicId::RunicDome,
        },
        RelicId::VelvetChoker => RelicMechanics {
            text_id: RelicId::VelvetChoker,
        },
        RelicId::CursedKey => RelicMechanics {
            text_id: RelicId::CursedKey,
        },
        RelicId::BlackStar => RelicMechanics {
            text_id: RelicId::BlackStar,
        },
        RelicId::BustedCrown => RelicMechanics {
            text_id: RelicId::BustedCrown,
        },
        RelicId::QuestionCard => RelicMechanics {
            text_id: RelicId::QuestionCard,
        },
        RelicId::CallingBell => RelicMechanics {
            text_id: RelicId::CallingBell,
        },
        RelicId::CoffeeDripper => RelicMechanics {
            text_id: RelicId::CoffeeDripper,
        },
        RelicId::EmptyCage => RelicMechanics {
            text_id: RelicId::EmptyCage,
        },
        RelicId::FusionHammer => RelicMechanics {
            text_id: RelicId::FusionHammer,
        },
        RelicId::PandorasBox => RelicMechanics {
            text_id: RelicId::PandorasBox,
        },
        RelicId::PhilosopherStone => RelicMechanics {
            text_id: RelicId::PhilosopherStone,
        },
        RelicId::RunicPyramid => RelicMechanics {
            text_id: RelicId::RunicPyramid,
        },
        RelicId::SacredBark => RelicMechanics {
            text_id: RelicId::SacredBark,
        },
        RelicId::SlaversCollar => RelicMechanics {
            text_id: RelicId::SlaversCollar,
        },
        RelicId::SneckoEye => RelicMechanics {
            text_id: RelicId::SneckoEye,
        },
        RelicId::Sozu => RelicMechanics {
            text_id: RelicId::Sozu,
        },
        RelicId::BlackBlood => RelicMechanics {
            text_id: RelicId::BlackBlood,
        },
        RelicId::MarkOfPain => RelicMechanics {
            text_id: RelicId::MarkOfPain,
        },
        RelicId::BurningBlood => RelicMechanics {
            text_id: RelicId::BurningBlood,
        },
        RelicId::Pear => RelicMechanics {
            text_id: RelicId::Pear,
        },
        RelicId::HappyFlower => RelicMechanics {
            text_id: RelicId::HappyFlower,
        },
        RelicId::Sundial => RelicMechanics {
            text_id: RelicId::Sundial,
        },
        RelicId::Abacus => super::abacus::MECHANICS,
        RelicId::Calipers => super::calipers::MECHANICS,
        RelicId::CaptainsWheel => RelicMechanics {
            text_id: RelicId::CaptainsWheel,
        },
        RelicId::DollysMirror => RelicMechanics {
            text_id: RelicId::DollysMirror,
        },
        RelicId::Shuriken => RelicMechanics {
            text_id: RelicId::Shuriken,
        },
        RelicId::Kunai => super::kunai::MECHANICS,
        RelicId::OddMushroom => super::odd_mushroom::MECHANICS,
        RelicId::MutagenicStrength => RelicMechanics {
            text_id: RelicId::MutagenicStrength,
        },
        RelicId::Orichalcum => super::orichalcum::MECHANICS,
        RelicId::Vajra => RelicMechanics {
            text_id: RelicId::Vajra,
        },
        RelicId::JuzuBracelet => RelicMechanics {
            text_id: RelicId::JuzuBracelet,
        },
        RelicId::BronzeScales => RelicMechanics {
            text_id: RelicId::BronzeScales,
        },
        RelicId::PenNib => RelicMechanics {
            text_id: RelicId::PenNib,
        },
        RelicId::Nunchaku => RelicMechanics {
            text_id: RelicId::Nunchaku,
        },
        RelicId::CeramicFish => RelicMechanics {
            text_id: RelicId::CeramicFish,
        },
        RelicId::PaperFrog => RelicMechanics {
            text_id: RelicId::PaperFrog,
        },
        RelicId::Toolbox => RelicMechanics {
            text_id: RelicId::Toolbox,
        },
        RelicId::RedMask => RelicMechanics {
            text_id: RelicId::RedMask,
        },
        RelicId::ToyOrnithopter => RelicMechanics {
            text_id: RelicId::ToyOrnithopter,
        },
        RelicId::PeacePipe => RelicMechanics {
            text_id: RelicId::PeacePipe,
        },
        RelicId::PotionBelt => RelicMechanics {
            text_id: RelicId::PotionBelt,
        },
        RelicId::Boot => RelicMechanics {
            text_id: RelicId::Boot,
        },
        RelicId::CentennialPuzzle => RelicMechanics {
            text_id: RelicId::CentennialPuzzle,
        },
        RelicId::MeatOnTheBone => RelicMechanics {
            text_id: RelicId::MeatOnTheBone,
        },
        RelicId::Akabeko => RelicMechanics {
            text_id: RelicId::Akabeko,
        },
        RelicId::OddlySmoothStone => RelicMechanics {
            text_id: RelicId::OddlySmoothStone,
        },
        RelicId::Turnip => super::turnip::MECHANICS,
        RelicId::Ginger => super::ginger::MECHANICS,
        RelicId::LetterOpener => super::letter_opener::MECHANICS,
        RelicId::SelfFormingClay => super::self_forming_clay::MECHANICS,
        RelicId::Matryoshka => super::matryoshka::MECHANICS,
        RelicId::WarPaint => super::war_paint::MECHANICS,
        RelicId::SmilingMask => RelicMechanics {
            text_id: RelicId::SmilingMask,
        },
        RelicId::DreamCatcher => RelicMechanics {
            text_id: RelicId::DreamCatcher,
        },
        RelicId::BloodVial => RelicMechanics {
            text_id: RelicId::BloodVial,
        },
        RelicId::MawBank => RelicMechanics {
            text_id: RelicId::MawBank,
        },
        RelicId::BagOfMarbles => RelicMechanics {
            text_id: RelicId::BagOfMarbles,
        },
        RelicId::PrismaticShard => RelicMechanics {
            text_id: RelicId::PrismaticShard,
        },
        RelicId::FossilizedHelix => RelicMechanics {
            text_id: RelicId::FossilizedHelix,
        },
        RelicId::NeowsLament => super::neows_lament::MECHANICS,
        RelicId::Omamori => super::omamori::MECHANICS,
        RelicId::Shovel => super::shovel::MECHANICS,
        RelicId::BagOfPreparation => RelicMechanics {
            text_id: RelicId::BagOfPreparation,
        },
        RelicId::BirdFacedUrn => RelicMechanics {
            text_id: RelicId::BirdFacedUrn,
        },
        RelicId::BlueCandle => RelicMechanics {
            text_id: RelicId::BlueCandle,
        },
        RelicId::Cauldron => RelicMechanics {
            text_id: RelicId::Cauldron,
        },
        RelicId::CharonsAshes => RelicMechanics {
            text_id: RelicId::CharonsAshes,
        },
        RelicId::ClockworkSouvenir => RelicMechanics {
            text_id: RelicId::ClockworkSouvenir,
        },
        RelicId::CultistMask => RelicMechanics {
            text_id: RelicId::CultistMask,
        },
        RelicId::DarkstonePeriapt => RelicMechanics {
            text_id: RelicId::DarkstonePeriapt,
        },
        RelicId::DeadBranch => RelicMechanics {
            text_id: RelicId::DeadBranch,
        },
        RelicId::DuVuDoll => RelicMechanics {
            text_id: RelicId::DuVuDoll,
        },
        RelicId::FaceOfCleric => RelicMechanics {
            text_id: RelicId::FaceOfCleric,
        },
        RelicId::FrozenEye => RelicMechanics {
            text_id: RelicId::FrozenEye,
        },
        RelicId::GamblingChip => RelicMechanics {
            text_id: RelicId::GamblingChip,
        },
        RelicId::Girya => RelicMechanics {
            text_id: RelicId::Girya,
        },
        RelicId::GoldenIdol => RelicMechanics {
            text_id: RelicId::GoldenIdol,
        },
        RelicId::GremlinMask => RelicMechanics {
            text_id: RelicId::GremlinMask,
        },
        RelicId::HornCleat => RelicMechanics {
            text_id: RelicId::HornCleat,
        },
        RelicId::IceCream => RelicMechanics {
            text_id: RelicId::IceCream,
        },
        RelicId::InkBottle => RelicMechanics {
            text_id: RelicId::InkBottle,
        },
        RelicId::MarkOfTheBloom => RelicMechanics {
            text_id: RelicId::MarkOfTheBloom,
        },
        RelicId::MealTicket => RelicMechanics {
            text_id: RelicId::MealTicket,
        },
        RelicId::MedicalKit => RelicMechanics {
            text_id: RelicId::MedicalKit,
        },
        RelicId::MercuryHourglass => RelicMechanics {
            text_id: RelicId::MercuryHourglass,
        },
        RelicId::NlothsGift => RelicMechanics {
            text_id: RelicId::NlothsGift,
        },
        RelicId::NlothsMask => RelicMechanics {
            text_id: RelicId::NlothsMask,
        },
        RelicId::OldCoin => RelicMechanics {
            text_id: RelicId::OldCoin,
        },
        RelicId::OrnamentalFan => RelicMechanics {
            text_id: RelicId::OrnamentalFan,
        },
        RelicId::Orrery => RelicMechanics {
            text_id: RelicId::Orrery,
        },
        RelicId::PrayerWheel => RelicMechanics {
            text_id: RelicId::PrayerWheel,
        },
        RelicId::RedSkull => super::red_skull::MECHANICS,
        RelicId::SpiritPoop => RelicMechanics {
            text_id: RelicId::SpiritPoop,
        },
        RelicId::SsserpentHead => RelicMechanics {
            text_id: RelicId::SsserpentHead,
        },
        RelicId::StoneCalendar => RelicMechanics {
            text_id: RelicId::StoneCalendar,
        },
        RelicId::Strawberry => RelicMechanics {
            text_id: RelicId::Strawberry,
        },
        RelicId::Mango => RelicMechanics {
            text_id: RelicId::Mango,
        },
        RelicId::ThreadAndNeedle => RelicMechanics {
            text_id: RelicId::ThreadAndNeedle,
        },
        RelicId::TinyChest => RelicMechanics {
            text_id: RelicId::TinyChest,
        },
        RelicId::Whetstone => RelicMechanics {
            text_id: RelicId::Whetstone,
        },
        RelicId::WhiteBeastStatue => RelicMechanics {
            text_id: RelicId::WhiteBeastStatue,
        },
        RelicId::WingBoots => RelicMechanics {
            text_id: RelicId::WingBoots,
        },
        RelicId::Circlet | RelicId::RedCirclet => RelicMechanics {
            text_id: RelicId::Circlet,
        },
        _ => return None,
    })
}

/// The compact flag also stores several temporary trigger states. Keep its
/// player-facing meaning with the relic rules rather than calling every flag
/// permanent consumption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelicTriggerStateV1 {
    LowHpStrengthActive,
    LowHpStrengthInactive,
    OpeningEnergySpent,
    OpeningEnergyChecked,
    DrawTriggerSpent,
    OpeningRedrawCompleted,
    AttackReplaySpent,
    DiscardEnergySpent,
    DisabledUntilTurnEnd,
    UsedUp,
}

impl RelicTriggerStateV1 {
    pub fn description(self, locale: sts_presentation::Locale) -> &'static str {
        use RelicTriggerStateV1::*;
        let key = match self {
            LowHpStrengthActive => "lowHpStrengthActive",
            LowHpStrengthInactive => "lowHpStrengthInactive",
            OpeningEnergySpent => "openingEnergySpent",
            OpeningEnergyChecked => "openingEnergyChecked",
            DrawTriggerSpent => "drawTriggerSpent",
            OpeningRedrawCompleted => "openingRedrawCompleted",
            AttackReplaySpent => "attackReplaySpent",
            DiscardEnergySpent => "discardEnergySpent",
            DisabledUntilTurnEnd => "disabledUntilTurnEnd",
            UsedUp => "usedUp",
        };
        locale
            .entry("game", &format!("relicState.{key}"))
            .expect("relic state message")
    }
}

pub fn trigger_state_v1(relic: RelicId, flag: bool) -> Option<RelicTriggerStateV1> {
    use RelicTriggerStateV1::*;
    if relic == RelicId::RedSkull {
        return Some(if flag {
            LowHpStrengthActive
        } else {
            LowHpStrengthInactive
        });
    }
    if !flag {
        return None;
    }
    Some(match relic {
        RelicId::Lantern => OpeningEnergySpent,
        RelicId::AncientTeaSet => OpeningEnergyChecked,
        RelicId::CentennialPuzzle => DrawTriggerSpent,
        RelicId::GamblingChip => OpeningRedrawCompleted,
        RelicId::Necronomicon => AttackReplaySpent,
        RelicId::HoveringKite => DiscardEnergySpent,
        RelicId::UnceasingTop => DisabledUntilTurnEnd,
        _ => UsedUp,
    })
}

pub fn trigger_state_description(relic: RelicId, flag: bool) -> Option<&'static str> {
    trigger_state_v1(relic, flag).map(|state| state.description(sts_presentation::Locale::En))
}

/// Applies to getRewardCards/getColorlessRewardCards, including callers such
/// as Dream Catcher and Orrery. It does not modify shop or Discovery choices.
pub fn relic_card_reward_choice_delta_v1(relic: RelicId) -> Option<i32> {
    match relic {
        RelicId::BustedCrown => Some(-2),
        RelicId::QuestionCard => Some(1),
        _ => None,
    }
}

/// The player receive-power hook prevents this new application before Artifact.
/// Existing powers are unchanged; this is not a cleanse or a current Power.
pub fn relic_prevent_player_debuff_before_artifact_v1(relic: RelicId) -> Option<PowerId> {
    match relic {
        RelicId::Ginger => Some(super::ginger::PREVENTED_POWER_ID),
        RelicId::Turnip => Some(super::turnip::PREVENTED_POWER_ID),
        _ => None,
    }
}

/// A request on surviving combat victory. Actual healing keeps its restrictions,
/// modifiers and HP cap; this is not an immediate or guaranteed HP gain.
pub fn relic_victory_healing_request_v1(relic: RelicId) -> Option<i32> {
    match relic {
        RelicId::BurningBlood => Some(super::burning_blood::VICTORY_HEAL_AMOUNT),
        RelicId::BlackBlood => Some(super::black_blood::VICTORY_HEAL_AMOUNT),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicCombatPotionDropOverrideV1 {
    pub chance_percent: i32,
    pub blocked_at_reward_entry_count: usize,
}

/// A reward-generation rule, not an occupied potion slot or a pickup guarantee.
/// The size gate takes precedence; collection still uses its ordinary rules.
pub fn relic_combat_potion_drop_override_v1(
    relic: RelicId,
) -> Option<RelicCombatPotionDropOverrideV1> {
    (relic == RelicId::WhiteBeastStatue).then_some(RelicCombatPotionDropOverrideV1 {
        chance_percent: super::white_beast_statue::POTION_DROP_CHANCE_OVERRIDE_PERCENT,
        blocked_at_reward_entry_count:
            crate::state::rewards::generator::POTION_DROP_BLOCKED_AT_REWARD_ENTRY_COUNT,
    })
}

/// Extra independently generated rows, only when a normal MonsterRoom creates
/// card rewards. Elite, boss and event rooms do not qualify.
pub fn relic_additional_normal_monster_card_reward_rows_v1(relic: RelicId) -> Option<u8> {
    (relic == RelicId::PrayerWheel).then_some(1)
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicGoldGainRuleV1 {
    PreventGoldGain,
}

pub fn relic_gold_gain_rule_v1(relic: RelicId) -> Option<RelicGoldGainRuleV1> {
    (relic == RelicId::Ectoplasm).then_some(RelicGoldGainRuleV1::PreventGoldGain)
}

pub fn relic_prevents_gold_gain(relic: RelicId) -> bool {
    relic_gold_gain_rule_v1(relic) == Some(RelicGoldGainRuleV1::PreventGoldGain)
}

/// One immediate healing request per successful positive gold gain, independent
/// of the gold amount. The current healing owner applies modifiers and caps.
pub fn relic_gold_gain_healing_v1(relic: RelicId) -> Option<i32> {
    (relic == RelicId::BloodyIdol).then_some(super::bloody_idol::HEAL_AMOUNT)
}

/// This only controls ordinary end-turn discard. It does not grant Retain,
/// prevent Ethereal exhaust, or block discards requested by another effect.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicEndTurnDiscardRuleV1 {
    SkipOrdinaryDiscard,
}

pub fn relic_end_turn_discard_rule_v1(relic: RelicId) -> Option<RelicEndTurnDiscardRuleV1> {
    (relic == RelicId::RunicPyramid).then_some(RelicEndTurnDiscardRuleV1::SkipOrdinaryDiscard)
}

/// Only the named campfire option is blocked; other healing and upgrades keep
/// their own rules. Missing here is not a claim of complete relic coverage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicCampfireRestrictionV1 {
    Rest,
    Smith,
}

pub fn relic_campfire_restriction_v1(relic: RelicId) -> Option<RelicCampfireRestrictionV1> {
    match relic {
        RelicId::CoffeeDripper => Some(RelicCampfireRestrictionV1::Rest),
        RelicId::FusionHammer => Some(RelicCampfireRestrictionV1::Smith),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicBattleStartScopeV1 {
    EveryCombat,
    EliteCombat,
}

impl RelicBattleStartScopeV1 {
    pub fn applies(self, is_elite_fight: bool) -> bool {
        self == Self::EveryCombat || is_elite_fight
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicBattleStartPowerKindV1 {
    Strength,
    Thorns,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicBattleStartPowerV1 {
    pub scope: RelicBattleStartScopeV1,
    pub power: RelicBattleStartPowerKindV1,
    pub amount: i32,
}

pub fn relic_battle_start_power_v1(relic: RelicId) -> Option<RelicBattleStartPowerV1> {
    let (scope, power, amount) = match relic {
        RelicId::BronzeScales => (
            RelicBattleStartScopeV1::EveryCombat,
            RelicBattleStartPowerKindV1::Thorns,
            super::bronze_scales::THORNS_AMOUNT,
        ),
        RelicId::Sling => (
            RelicBattleStartScopeV1::EliteCombat,
            RelicBattleStartPowerKindV1::Strength,
            super::sling::ELITE_STRENGTH_AMOUNT,
        ),
        _ => return None,
    };
    Some(RelicBattleStartPowerV1 {
        scope,
        power,
        amount,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicShuffleEnergyV1 {
    pub shuffle_trigger_at: i32,
    pub energy: i32,
    pub initial_counter: i32,
    pub counter_persists_between_combats: bool,
}

pub fn relic_shuffle_energy_v1(relic: RelicId) -> Option<RelicShuffleEnergyV1> {
    (relic == RelicId::Sundial).then(|| RelicShuffleEnergyV1 {
        shuffle_trigger_at: super::sundial::SHUFFLES_PER_TRIGGER,
        energy: super::sundial::ENERGY_AMOUNT,
        initial_counter: super::RelicState::new(relic).counter,
        counter_persists_between_combats: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;

    #[test]
    fn every_ironclad_pool_and_shared_special_relic_has_a_rule() {
        use super::super::{build_relic_pool, RelicTier};
        for tier in [
            RelicTier::Starter,
            RelicTier::Common,
            RelicTier::Uncommon,
            RelicTier::Rare,
            RelicTier::Boss,
            RelicTier::Shop,
            RelicTier::Special,
        ] {
            for relic in build_relic_pool(tier, "Ironclad") {
                assert!(
                    get(relic).is_some_and(|rule| !rule
                        .explanation(sts_presentation::Locale::En)
                        .is_empty()),
                    "{relic:?}"
                );
            }
        }
        for relic in [RelicId::Circlet, RelicId::RedCirclet] {
            assert!(get(relic).is_some(), "{relic:?}");
        }
    }

    #[test]
    fn reviewed_relic_rules_render_both_languages_with_explicit_shared_circlet_identity() {
        use crate::content::identity::StableGameplayId;
        use sts_presentation::Locale;
        let mut count = 0;
        let mut rules = std::collections::HashSet::new();
        for &id in RelicId::STABLE_IDS {
            let relic = RelicId::from_stable_id(id).unwrap();
            let Some(rule) = get(relic) else { continue };
            count += 1;
            rules.insert(rule.text_id);
            assert_eq!(
                rule.text_id,
                if relic == RelicId::RedCirclet {
                    RelicId::Circlet
                } else {
                    relic
                }
            );
            for locale in [Locale::En, Locale::ZhCn] {
                let text = rule.explanation(locale);
                assert!(!text.is_empty() && !text.contains("{{"), "{id}: {text}");
                if locale == Locale::ZhCn {
                    assert!(
                        text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                        "{id}"
                    );
                }
            }
        }
        assert_eq!(count, 151);
        assert_eq!(rules.len(), 150);
    }

    #[test]
    fn passive_request_readings_share_runtime_amounts_and_leave_other_effects_unknown() {
        for (relic, power, modify) in [
            (
                RelicId::Ginger,
                PowerId::Weak,
                super::super::ginger::on_receive_power_modify as fn(PowerId, i32) -> i32,
            ),
            (
                RelicId::Turnip,
                PowerId::Frail,
                super::super::turnip::on_receive_power_modify as fn(PowerId, i32) -> i32,
            ),
        ] {
            assert_eq!(
                relic_prevent_player_debuff_before_artifact_v1(relic),
                Some(power)
            );
            assert_eq!(modify(power, 2), 0);
            assert_eq!(modify(PowerId::Vulnerable, 2), 2);
        }
        let mut state = crate::test_support::blank_test_combat();
        for (relic, actions) in [
            (
                RelicId::BurningBlood,
                super::super::burning_blood::BurningBlood::on_victory(&state),
            ),
            (
                RelicId::BlackBlood,
                super::super::black_blood::BlackBlood::on_victory(&state),
            ),
        ] {
            assert_eq!(
                actions[0].action,
                Action::Heal {
                    target: 0,
                    amount: relic_victory_healing_request_v1(relic).unwrap(),
                }
            );
        }
        state.entities.player.current_hp = 0;
        assert!(super::super::burning_blood::BurningBlood::on_victory(&state).is_empty());
        assert!(super::super::black_blood::BlackBlood::on_victory(&state).is_empty());
        let potion = relic_combat_potion_drop_override_v1(RelicId::WhiteBeastStatue).unwrap();
        assert_eq!(
            (potion.chance_percent, potion.blocked_at_reward_entry_count),
            (100, 4)
        );
        assert!(relic_prevent_player_debuff_before_artifact_v1(RelicId::OddMushroom).is_none());
        assert!(relic_combat_potion_drop_override_v1(RelicId::Sozu).is_none());
        assert!(relic_victory_healing_request_v1(RelicId::MeatOnTheBone).is_none());
    }

    #[test]
    fn battle_start_readings_preserve_elite_only_scope_and_reactive_power() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.is_boss_fight = true;
        let sling = relic_battle_start_power_v1(RelicId::Sling).unwrap();
        assert!(!sling.scope.applies(state.meta.is_elite_fight));
        assert!(super::super::sling::at_battle_start(&state).is_empty());
        state.meta.is_elite_fight = true;
        assert!(sling.scope.applies(state.meta.is_elite_fight));
        assert!(matches!(
            super::super::sling::at_battle_start(&state)[0].action,
            Action::ApplyPower {
                power_id: PowerId::Strength,
                amount: 2,
                ..
            }
        ));
        let bronze = relic_battle_start_power_v1(RelicId::BronzeScales).unwrap();
        assert_eq!(bronze.power, RelicBattleStartPowerKindV1::Thorns);
        assert!(bronze.scope.applies(false));
        assert!(matches!(
            super::super::bronze_scales::BronzeScales::at_battle_start(0)[0].action,
            Action::ApplyPower {
                power_id: PowerId::Thorns,
                amount: 3,
                ..
            }
        ));
    }

    #[test]
    fn sundial_reading_keeps_cross_combat_progress_and_exact_trigger() {
        let rule = relic_shuffle_energy_v1(RelicId::Sundial).unwrap();
        assert_eq!(rule.initial_counter, 0);
        assert_eq!(rule.shuffle_trigger_at, 3);
        let mut relic = super::super::RelicState::new(RelicId::Sundial);
        relic.counter = 2;
        let mut carried =
            super::super::persistent_state::post_combat_persistent_relic_state_v1(&relic);
        assert!(rule.counter_persists_between_combats);
        assert_eq!(carried.counter, 2);
        let actions = super::super::sundial::on_shuffle(&mut carried);
        assert_eq!(carried.counter, 0);
        assert_eq!(actions[0].action, Action::GainEnergy { amount: 2 });
        carried.counter = 3;
        assert!(super::super::sundial::on_shuffle(&mut carried).is_empty());
        assert_eq!(
            carried.counter, 4,
            "preserve exact ++counter == 3, not >= or modulo"
        );
        assert!(relic_shuffle_energy_v1(RelicId::HappyFlower).is_none());
    }
}
