//! Identity-independent card mechanics shared by runtime and learned-policy adapters.
//!
//! `CardDefinition` remains the source for the ordinary printed attributes. This
//! projection adds upgrade-sensitive effective attributes and a deliberately
//! bounded vocabulary for effects that the printed definition cannot express.
//! Role coverage is explicit: an absent role is only a negative fact when the
//! profile is `Complete`.

use serde::{Deserialize, Serialize};

use super::{get_card_definition, CardDefinition, CardId, CardTarget};
use crate::runtime::combat::{CardBlueprint, CombatCard};

/// Reviewed effect order beyond printed attributes; absent explanations stay explicit.
pub fn explanation(card: CardId) -> Option<&'static str> {
    match card {
        CardId::Pain => Some(super::curses::pain::MECHANICS),
        CardId::Parasite => Some(super::curses::parasite::MECHANICS),
        CardId::Writhe => Some(super::curses::writhe::MECHANICS),
        CardId::Decay => Some(super::curses::decay::MECHANICS),
        CardId::Normality => Some(super::curses::normality::MECHANICS),
        CardId::Injury => Some("Unplayable. Occupies a draw and Hand slot; has no additional combat effect."),
        CardId::Regret => Some("Unplayable. If still in Hand at the end of your turn, lose HP equal to the Hand size when its end-turn effect is queued, including Regret itself. Block does not prevent this HP loss."),
        CardId::Clumsy => Some("Unplayable. Ethereal: Exhaust if still in Hand at the end of your turn. Occupies a draw and Hand slot until then."),
        CardId::CurseOfTheBell => Some("Unplayable. Occupies a draw and Hand slot; has no additional combat effect. Cannot be selected for ordinary permanent removal or transformation."),
        CardId::Doubt => Some("Unplayable. If still in Hand at the end of your turn, apply 1 Weak to yourself. Artifact can block this application."),
        CardId::Shame => Some("Unplayable. If still in Hand at the end of your turn, apply 1 Frail to yourself. Artifact can block this application."),
        CardId::Pride => Some("Innate. If still in Hand at the end of your turn, put one temporary Pride on top of the draw pile. Costs 1 Energy to play and Exhaust; playing it has no additional effect. Copies generated during combat do not enter the permanent deck."),
        CardId::Miracle => Some("Gain 1 Energy (2 if upgraded). Costs 0 Energy. Retain. Exhaust."),
        CardId::Insight => Some("Draw 2 cards (3 if upgraded). Costs 0 Energy. Retain. Exhaust. Draw prevention and hand capacity still apply."),
        CardId::Shiv => Some("Deal 4 damage to one enemy (6 if upgraded). Costs 0 Energy. Exhaust."),
        CardId::Strike => Some("Deal 6 damage to one enemy (9 if upgraded)."),
        CardId::Defend => Some("Gain 5 Block (8 if upgraded)."),
        CardId::JAX => Some("Lose 3 HP, then gain 2 Strength (3 if upgraded) for the rest of combat. Costs 0 Energy and does not Exhaust; Block does not prevent the self HP loss."),
        CardId::Apparition => Some("Gain 1 Intangible, reducing incoming damage and HP loss to 1 per instance while active. Exhaust. Ethereal when unupgraded; upgrading removes Ethereal but does not remove Exhaust."),
        CardId::Apotheosis => Some(super::colorless::apotheosis::MECHANICS),
        CardId::Bite => Some(super::colorless::bite::MECHANICS),
        CardId::RitualDagger => Some(super::colorless::ritual_dagger::MECHANICS),
        CardId::BandageUp => Some(super::colorless::bandage_up::MECHANICS),
        CardId::Madness => Some("Set one randomly selected eligible card in Hand to cost 0 for this combat. Prefers cards with a positive current turn cost; if none exist, uses a card whose combat cost is still positive. Costs 1 Energy (0 if upgraded). Exhaust."),
        CardId::Blind => Some("Apply 2 Weak to one enemy (all enemies if upgraded). Costs 0 Energy and does not Exhaust. Artifact can block each enemy's debuff application."),
        CardId::DarkShackles => Some("Reduce one enemy's Strength by 9 (15 if upgraded) for its next turn, then restore that amount at the end of that enemy's turn. Artifact can block the Strength reduction. Costs 0 Energy. Exhaust."),
        CardId::DramaticEntrance => Some("Deal 8 damage to all enemies (12 if upgraded). Costs 0 Energy. Innate. Exhaust."),
        CardId::Enlightenment => Some("Reduce costs greater than 1 on eligible cards currently in Hand to 1 for this turn (for this combat if upgraded). X-cost and unplayable cards are unaffected. Costs 0 Energy and does not Exhaust. Only the current Hand is changed."),
        CardId::Impatience => Some("If no Attack cards remain in Hand when this effect resolves, draw 2 cards (3 if upgraded). Otherwise draw none. Costs 0 Energy and does not Exhaust."),
        CardId::JackOfAllTrades => Some("Add 1 random Colorless card to Hand (2 if upgraded). The generated cards keep their normal costs and are temporary for this combat; hand capacity applies. Costs 0 Energy. Exhaust."),
        CardId::MindBlast => Some("Deal base damage equal to the number of cards currently in the draw pile, then apply ordinary Attack modifiers. Innate. Costs 2 Energy (1 if upgraded) and does not Exhaust."),
        CardId::Trip => Some("Apply 2 Vulnerable to one enemy (all enemies if upgraded). Costs 0 Energy and does not Exhaust. Artifact can block each enemy's debuff application."),
        CardId::Chrysalis => Some("Insert 3 random Skills from your class at random positions in the draw pile (5 if upgraded). These temporary cards cost 0 for this combat and must still be drawn to reach Hand. Costs 2 Energy. Exhaust."),
        CardId::HandOfGreed => Some("Deal 20 damage to one enemy (25 if upgraded). If this hit kills a non-Minion enemy that is not entering a half-dead phase, gain 20 Gold (25 if upgraded), subject to gold-gain rules. Costs 2 Energy and does not Exhaust."),
        CardId::Magnetism => Some("For the rest of combat, at each turn start from the next turn onward, add 1 random temporary Colorless card to Hand per stack. Generated cards keep their normal costs; hand capacity applies. Costs 2 Energy (1 if upgraded)."),
        CardId::Metamorphosis => Some("Insert 3 random Attacks from your class at random positions in the draw pile (5 if upgraded). These temporary cards cost 0 for this combat and must still be drawn to reach Hand. Costs 2 Energy. Exhaust."),
        CardId::ThinkingAhead => Some("Draw 2 cards, then choose 1 card from Hand to put on top of the draw pile. Costs 0 Energy. Exhaust when unupgraded; upgrading removes Exhaust. The returned card must be drawn again to reach Hand."),
        CardId::DeepBreath => Some(super::colorless::deep_breath::MECHANICS),
        CardId::Forethought => Some(super::colorless::forethought::MECHANICS),
        CardId::SwiftStrike => Some(super::colorless::swift_strike::MECHANICS),
        CardId::TheBomb => Some(super::colorless::the_bomb::MECHANICS),
        CardId::PanicButton => Some(super::colorless::panic_button::MECHANICS),
        CardId::Transmutation => Some(super::colorless::transmutation::MECHANICS),
        CardId::Purity => Some(super::colorless::purity::MECHANICS),
        CardId::Panache => Some(super::colorless::panache::MECHANICS),
        CardId::Discovery => Some("Choose one of three distinct randomly offered cards from your class's combat-generation pool (healing-tagged cards are excluded). Add a temporary copy to your hand; it costs 0 this turn. Costs 1 Energy. Exhaust when unupgraded; upgrading removes Exhaust."),
        CardId::SecretWeapon => Some("Choose one Attack card in your draw pile and move it to your hand. Costs 0 Energy. Cannot play if the draw pile has no Attack. Exhaust when unupgraded; upgrading removes Exhaust."),
        CardId::SecretTechnique => Some(super::colorless::secret_technique::MECHANICS),
        CardId::Panacea => Some(super::colorless::panacea::MECHANICS),
        CardId::FlashOfSteel => Some("Deal 3 damage to one enemy (6 if upgraded), then draw 1 card. Costs 0 Energy and does not Exhaust."),
        CardId::Mayhem => Some("For the rest of combat, after your normal start-of-turn draw, automatically play the top card of your remaining draw pile for free (a random enemy target if needed). Each stack plays one card. Does not force Exhaust; the played card keeps its own Exhaust rule. Costs 2 Energy (1 if upgraded)."),
        CardId::Bash => Some("Deal 8 damage (10 if upgraded), then apply 2 Vulnerable (3 if upgraded). The new Vulnerable does not increase this hit's damage."),
        CardId::Anger => Some("Deal 6 damage (8 if upgraded), then add a copy of this card to your discard pile. This combat-only copy is not added to your permanent master deck and disappears after combat. The original card is also discarded normally after play."),
        CardId::Barricade => Some("For the rest of combat, your Block is not removed at the start of your turn. Costs 3 Energy (2 if upgraded)."),
        CardId::Bludgeon => Some("Deal 32 damage to one enemy (42 if upgraded)."),
        CardId::Brutality => Some("At the start of each turn, after the normal draw, draw 1 additional card and lose 1 HP. Upgraded: Innate, so it starts in your opening hand."),
        CardId::BurningPact => Some("Choose and Exhaust 1 other card from your hand, then draw 2 cards (3 if upgraded). Burning Pact itself does not Exhaust unless another effect, such as Corruption, makes it do so; it can be drawn and played again. If no other card is in hand, the draw still occurs. Pact grants no Energy; any Energy gain needs a separate trigger, such as the selected Sentinel's own on-Exhaust effect."),
        CardId::Carnage => Some("Deal 20 damage (28 if upgraded). Ethereal: Exhaust if still in hand at the end of your turn; playing it does not Exhaust it."),
        CardId::Clash => Some("Can only be played if every card in your hand is an Attack; Skills, Powers, Statuses and Curses prevent play. Deal 14 damage (18 if upgraded)."),
        CardId::Combust => Some("At the end of each turn, lose 1 HP, then deal 5 non-Attack damage to all enemies (7 if upgraded). Additional applications add both HP loss and damage."),
        CardId::DarkEmbrace => Some("For the rest of combat, whenever a card is Exhausted, draw 1 card per stack of Dark Embrace. Each play adds 1 stack, so two plays draw 2 per Exhaust; replaying with Duplication also adds a stack. Draw prevention and hand capacity still apply. Costs 2 Energy (1 if upgraded)."),
        CardId::DemonForm => Some("At the start of each of your turns from the next turn onward, gain 2 Strength (3 if upgraded). Does not grant Strength immediately when played."),
        CardId::Disarm => Some(super::ironclad::disarm::MECHANICS),
        CardId::DoubleTap => Some("This turn, replay each of the next 1 Attack cards you play (2 if upgraded) once against the same target, without paying its cost again. Unused replays expire at turn end."),
        CardId::Evolve => Some("For the rest of combat, whenever you draw a Status card, draw 1 additional card (2 if upgraded). Curses do not trigger this effect."),
        CardId::FireBreathing => Some("For the rest of combat, whenever you draw a Status or Curse, deal 6 non-Attack damage to all enemies (10 if upgraded)."),
        CardId::Flex => Some("Gain 2 Strength (4 if upgraded), then apply an equal Lose Strength effect that removes that Strength at the end of this turn. Artifact can block the Lose Strength debuff."),
        CardId::GhostlyArmor => Some("Gain 10 Block (13 if upgraded). Ethereal: Exhaust if still in hand at the end of your turn; playing it does not Exhaust it."),
        CardId::Headbutt => Some("Deal 9 damage (12 if upgraded), then put 1 card from your discard pile on top of your draw pile. Choose when multiple cards are eligible; no card is moved if discard is empty. This does not draw the card."),
        CardId::HeavyBlade => Some("Deal 14 base damage. Strength contributes 3 times its usual amount (5 times if upgraded); this is one hit, not multiple hits."),
        CardId::Hemokinesis => Some("Lose 2 HP, then deal 15 damage to one enemy (20 if upgraded). Block does not prevent the self HP loss."),
        CardId::Impervious => Some("Gain 30 Block (40 if upgraded). Exhaust."),
        CardId::GoodInstincts => Some("Gain 6 Block (9 if upgraded). Costs 0 Energy and does not Exhaust. Block follows current Dexterity and Frail modifiers."),
        CardId::MasterOfStrategy => Some("Draw 3 cards (4 if upgraded). Costs 0 Energy. Exhaust. Draw prevention and hand capacity still apply."),
        CardId::Necronomicurse => Some("Unplayable. When Exhausted during combat, add a new temporary Necronomicurse to your hand. Permanent removal normally regenerates it in the master deck while Necronomicon is held."),
        CardId::InfernalBlade => Some("Add 1 random Attack from your class pool to Hand; overflow goes to Discard. The generated card is temporary for this combat and costs 0 this turn. At turn end its ordinary cost returns; it follows its own play conditions and lifecycle. Exhaust. Costs 1 Energy (0 if upgraded)."),
        CardId::Inflame => Some("Gain 2 Strength (3 if upgraded) for the rest of combat."),
        CardId::Intimidate => Some("Apply 1 Weak to all enemies (2 if upgraded). Exhaust."),
        CardId::LimitBreak => Some("Double your current Strength, including negative Strength. Gives no Strength if you have none. Exhaust when unupgraded; no longer Exhausts when upgraded."),
        CardId::Offering => Some("Lose 6 HP, then gain 2 Energy, then draw 3 cards (5 if upgraded). Block does not prevent the self HP loss. Exhaust."),
        CardId::PerfectedStrike => Some("Deal 6 base damage plus 2 for each Strike-tagged card in hand, draw and discard, including this card (3 per card if upgraded). Exhausted cards do not count."),
        CardId::Pummel => Some("Deal 2 damage to one enemy 4 times (5 if upgraded). Each hit benefits from Strength. Exhaust."),
        CardId::Reaper => Some(super::ironclad::reaper::MECHANICS),
        CardId::RecklessCharge => Some("Deal 7 damage (10 if upgraded), then insert 1 Dazed at a random position in your draw pile. Upgrading does not remove the Dazed."),
        CardId::SearingBlow => Some("Deal 12 damage at upgrade count 0. Can be upgraded repeatedly: base damage is 12 + n*(n+7)/2 at upgrade count n (16, 21, 27 for the first three upgrades)."),
        CardId::SecondWind => Some("Exhaust all non-Attack cards remaining in your hand, then gain 5 Block separately for each card Exhausted (7 if upgraded). Skills, Powers, Statuses and Curses qualify; Attack cards stay in hand."),
        CardId::SeeingRed => Some("Gain 2 Energy. Exhaust. Costs 1 Energy (0 if upgraded), so the net Energy gain is 1 (2 if upgraded) at the printed cost."),
        CardId::Shockwave => Some("Apply 3 Weak, then 3 Vulnerable to each enemy (5 of each if upgraded). Each debuff can consume a separate Artifact charge. Exhaust."),
        CardId::SwordBoomerang => Some("Deal 3 damage 3 times (4 if upgraded). Each hit chooses a random living enemy independently and benefits from Strength; you cannot select the target."),
        CardId::ThunderClap => Some("Deal 4 damage to all enemies (7 if upgraded), then apply 1 Vulnerable to all enemies. The new Vulnerable does not increase these hits' damage."),
        CardId::TwinStrike => Some("Deal 5 damage to one enemy twice (7 per hit if upgraded). Each hit benefits from Strength."),
        CardId::WildStrike => Some("Deal 12 damage (17 if upgraded), then insert 1 Wound at a random position in your draw pile. Upgrading does not remove the Wound."),
        CardId::Wound => Some("Unplayable. No draw or end-of-turn effect; it is discarded normally if still in hand, and remains in combat until removed by another effect."),
        CardId::Burn => Some("Unplayable. If still in hand at the end of your turn, deal 2 damage to yourself (4 if upgraded). Block can absorb this damage; Burn is then discarded normally."),
        CardId::Slimed => Some("Costs 1 Energy to play and Exhaust; has no other effect. If left in hand, it is discarded normally."),
        CardId::Void => Some("Unplayable. When drawn, lose 1 Energy. Ethereal: Exhaust if still in hand at the end of your turn."),
        CardId::AscendersBane => Some("Unplayable. Ethereal: Exhaust if still in hand at the end of your turn. Cannot be removed from your master deck."),
        CardId::Dazed => Some(super::status::dazed::MECHANICS),
        CardId::Cleave => Some("Deal 8 damage to all enemies (11 if upgraded)."),
        CardId::Whirlwind => {
            Some("Deal 5 damage to all enemies X times (8 if upgraded); X is the Energy spent.")
        }
        CardId::Havoc => Some(super::ironclad::havoc::MECHANICS),
        CardId::Warcry => Some(super::ironclad::warcry::MECHANICS),
        CardId::Rampage => Some(super::ironclad::rampage::MECHANICS),
        CardId::BloodForBlood => Some(super::ironclad::blood_for_blood::MECHANICS),
        CardId::Dropkick => Some(super::ironclad::dropkick::MECHANICS),
        CardId::DualWield => Some(super::ironclad::dual_wield::MECHANICS),
        CardId::Exhume => Some(super::ironclad::exhume::MECHANICS),
        CardId::Rupture => {
            Some("Whenever you lose HP from your own card, gain 1 Strength (2 if upgraded).")
        }
        CardId::FeelNoPain => Some("Whenever a card is Exhausted, gain 3 Block (4 if upgraded)."),
        CardId::Finesse => Some("Gain 2 Block (4 if upgraded), then draw 1 card."),
        CardId::Violence => Some(super::colorless::violence::MECHANICS),
        CardId::SadisticNature => Some(
            "When you apply a debuff to an enemy without Artifact, deal 5 damage (7 if upgraded).",
        ),
        CardId::Uppercut => Some(super::ironclad::uppercut::MECHANICS),
        CardId::FlameBarrier => Some(super::ironclad::flame_barrier::MECHANICS),
        CardId::Metallicize => Some(super::ironclad::metallicize::MECHANICS),
        CardId::Bloodletting => Some(super::ironclad::bloodletting::MECHANICS),
        CardId::Armaments => Some(super::ironclad::armaments::MECHANICS),
        CardId::BattleTrance => Some(super::ironclad::battle_trance::MECHANICS),
        CardId::SeverSoul => Some(super::ironclad::sever_soul::MECHANICS),
        CardId::PowerThrough => Some(super::ironclad::power_through::MECHANICS),
        CardId::ShrugItOff => Some(super::ironclad::shrug_it_off::MECHANICS),
        CardId::TrueGrit => Some(super::ironclad::true_grit::MECHANICS),
        CardId::Feed => Some(super::ironclad::feed::MECHANICS),
        CardId::Rage => Some(super::ironclad::rage::MECHANICS),
        CardId::Sentinel => Some(super::ironclad::sentinel::MECHANICS),
        CardId::Berserk => Some(super::ironclad::berserk::MECHANICS),
        CardId::Immolate => Some(super::ironclad::immolate::MECHANICS),
        CardId::Juggernaut => Some(super::ironclad::juggernaut::MECHANICS),
        CardId::BodySlam => Some(super::ironclad::body_slam::MECHANICS),
        CardId::SpotWeakness => Some(super::ironclad::spot_weakness::MECHANICS),
        CardId::Clothesline => Some(super::ironclad::clothesline::MECHANICS),
        CardId::PommelStrike => Some(super::ironclad::pommel_strike::MECHANICS),
        CardId::IronWave => Some(super::ironclad::iron_wave::MECHANICS),
        CardId::FiendFire => Some(super::ironclad::fiend_fire::MECHANICS),
        CardId::Entrench => Some(super::ironclad::entrench::MECHANICS),
        CardId::Corruption => Some(super::ironclad::corruption::MECHANICS),
        _ => None,
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u8)]
pub enum CardMechanicCoverage {
    /// Only the printed `CardDefinition` and effective upgrade attributes are known.
    DefinitionOnly = 0,
    /// Every listed role is true, but other roles may still be missing.
    Partial = 1,
    /// The card was reviewed against its production play/trigger actions for this vocabulary.
    Complete = 2,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[repr(u8)]
pub enum CardMechanicRole {
    DrawCards = 0,
    GainEnergy = 1,
    ApplyWeak = 2,
    ApplyVulnerable = 3,
    ExhaustOtherCards = 4,
    SelectFromHand = 5,
    DiscardFromHand = 6,
    DrawPileControl = 7,
    MultiHit = 8,
    RandomTarget = 9,
    GenerateCard = 10,
    SelfHpLoss = 11,
    EndTurnHandTrigger = 12,
    DrawOnStatus = 13,
    HandSizeScaling = 14,
    RandomOutcome = 15,
    /// A persistent card creates value when other cards are exhausted.
    ExhaustPayoff = 16,
    /// Playing the card prevents further card draw during the current turn.
    PreventFurtherDrawThisTurn = 17,
    /// The card permanently reduces an enemy's attack scaling resource.
    PersistentEnemyAttackReduction = 18,
    /// Playing the card adds one or more Status cards to a combat pile.
    GenerateStatusCard = 19,
    /// A persistent card creates value when a Status card is drawn.
    StatusDrawPayoff = 20,
}

pub const CARD_MECHANIC_ROLE_COUNT: u64 = CardMechanicRole::StatusDrawPayoff as u64 + 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CardMechanicRoleSet(u32);

impl CardMechanicRoleSet {
    pub fn contains(self, role: CardMechanicRole) -> bool {
        self.0 & (1_u32 << role as u8) != 0
    }

    pub fn iter(self) -> impl Iterator<Item = CardMechanicRole> {
        const ROLES: [CardMechanicRole; CARD_MECHANIC_ROLE_COUNT as usize] = [
            CardMechanicRole::DrawCards,
            CardMechanicRole::GainEnergy,
            CardMechanicRole::ApplyWeak,
            CardMechanicRole::ApplyVulnerable,
            CardMechanicRole::ExhaustOtherCards,
            CardMechanicRole::SelectFromHand,
            CardMechanicRole::DiscardFromHand,
            CardMechanicRole::DrawPileControl,
            CardMechanicRole::MultiHit,
            CardMechanicRole::RandomTarget,
            CardMechanicRole::GenerateCard,
            CardMechanicRole::SelfHpLoss,
            CardMechanicRole::EndTurnHandTrigger,
            CardMechanicRole::DrawOnStatus,
            CardMechanicRole::HandSizeScaling,
            CardMechanicRole::RandomOutcome,
            CardMechanicRole::ExhaustPayoff,
            CardMechanicRole::PreventFurtherDrawThisTurn,
            CardMechanicRole::PersistentEnemyAttackReduction,
            CardMechanicRole::GenerateStatusCard,
            CardMechanicRole::StatusDrawPayoff,
        ];
        ROLES.into_iter().filter(move |role| self.contains(*role))
    }

    fn insert(&mut self, role: CardMechanicRole) {
        self.0 |= 1_u32 << role as u8;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardMechanicProfile {
    pub coverage: CardMechanicCoverage,
    pub roles: CardMechanicRoleSet,
    pub effective_target: CardTarget,
    pub exhausts_when_played: bool,
    pub ethereal: bool,
}

pub fn effective_card_target(card: CardId, upgrades: u8) -> CardTarget {
    match card {
        CardId::Blind | CardId::Trip if upgrades > 0 => CardTarget::AllEnemy,
        _ => get_card_definition(card).target,
    }
}

pub fn card_exhausts_when_played(card: CardId, upgrades: u8) -> bool {
    match card {
        CardId::CalculatedGamble
        | CardId::LimitBreak
        | CardId::Discovery
        | CardId::SecretTechnique
        | CardId::SecretWeapon
        | CardId::ThinkingAhead
        | CardId::Hologram
        | CardId::Rainbow
        | CardId::Impulse => upgrades == 0,
        _ => get_card_definition(card).exhaust,
    }
}

pub fn card_is_ethereal(card: CardId, upgrades: u8) -> bool {
    match card {
        CardId::Apparition | CardId::EchoForm | CardId::DevaForm => upgrades == 0,
        _ => get_card_definition(card).ethereal,
    }
}

/// Base damage from the card definition and its upgrade count, before
/// instance mutation, combat conditions, powers, or target modifiers.
pub fn card_base_damage_for_upgrades_v1(definition: &CardDefinition, upgrades: u8) -> i32 {
    if definition.id == CardId::SearingBlow {
        let n = i32::from(upgrades);
        12 + n * (n + 7) / 2
    } else {
        definition.base_damage + i32::from(upgrades > 0) * definition.upgrade_damage
    }
}

mod damage;
pub use damage::*;
mod energy;
pub use energy::*;
mod follow_up;
pub use follow_up::*;
mod secondary;
pub use secondary::*;
mod status;
pub use status::*;
mod upgrade;
pub use upgrade::*;

/// Persistent card-owned numbers before combat-dependent substitutions and
/// modifiers. These are definition/instance fields, not damage or Block grants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CardPersistentBaseNumbersV1 {
    pub damage: i32,
    pub block: i32,
}

pub fn card_persistent_base_numbers_v1(card: &CombatCard) -> CardPersistentBaseNumbersV1 {
    persistent_base_numbers(
        card.id,
        card.upgrades,
        card.misc_value,
        card.base_damage_override,
        card.base_block_override,
    )
}

/// The same persistent fields for a public, identity-free instance recipe.
/// Reading these values never allocates or binds a runtime card identity.
pub fn card_blueprint_persistent_base_numbers_v1(
    card: &CardBlueprint,
) -> CardPersistentBaseNumbersV1 {
    persistent_base_numbers(
        card.id,
        card.upgrades,
        card.misc_value,
        card.base_damage_override,
        card.base_block_override,
    )
}

fn persistent_base_numbers(
    id: CardId,
    upgrades: u8,
    misc: i32,
    damage_override: Option<i32>,
    block_override: Option<i32>,
) -> CardPersistentBaseNumbersV1 {
    let def = get_card_definition(id);
    let damage = match id {
        CardId::SearingBlow => card_base_damage_for_upgrades_v1(&def, upgrades),
        CardId::RitualDagger => {
            if misc > 0 {
                misc
            } else {
                def.base_damage
            }
        }
        _ => damage_override.unwrap_or(card_base_damage_for_upgrades_v1(&def, upgrades)),
    };
    let block = if id == CardId::GeneticAlgorithm {
        if misc > 0 {
            misc
        } else {
            def.base_block
        }
    } else {
        block_override.unwrap_or(def.base_block + i32::from(upgrades > 0) * def.upgrade_block)
    };
    CardPersistentBaseNumbersV1 { damage, block }
}

/// The restoration scheduled by a card-owned Strength reduction, before
/// target-specific prevention, power removal, or encounter phase transitions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyStrengthRestorationV1 {
    /// The card does not schedule a matching Strength gain. This does not
    /// prevent a later enemy cleanse or phase reset from changing Strength.
    NoScheduledRestoration,
    /// Java DarkShackles/PiercingWail queue GainStrengthPower (ID Shackled)
    /// only when the target has no Artifact at card-use time, before queued
    /// Strength loss resolves. That power gains Strength at its owner's turn
    /// end if still present; removing it earlier does not itself restore it.
    ShackledAtOwnerTurnEnd,
}

/// Source-audited pairing used by printed reward facts and strategic roles.
/// `None` makes no claim about unreviewed, variable, or indirect mechanisms.
pub fn card_enemy_strength_restoration_v1(card: CardId) -> Option<EnemyStrengthRestorationV1> {
    match card {
        CardId::Disarm => Some(EnemyStrengthRestorationV1::NoScheduledRestoration),
        CardId::DarkShackles | CardId::PiercingWail => {
            Some(EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd)
        }
        _ => None,
    }
}

/// How a reviewed card's Strength magnitude is delivered to the player.
/// This is a request/schedule, not guaranteed Strength or attack damage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardPlayerStrengthDeliveryV1 {
    /// Apply Strength on play without scheduling a matching loss.
    OnPlay,
    /// SpotWeaknessAction checks getIntentBaseDmg() >= 0 when it resolves,
    /// including zero-damage attacks. This is not a playability restriction.
    IfTargetIntentBaseDamageNonnegativeAtResolution,
    /// Apply Strength, then separately apply the LoseStrength debuff. If the
    /// latter survives prevention/removal, it loses its amount at turn end.
    OnPlayWithSeparateLoseStrength,
    /// Install DemonForm; it requests Strength at future player turn starts,
    /// after normal draw, rather than granting Strength at card-use time.
    EachPlayerTurnStartPostDraw,
}

/// Missing means unreviewed, not that the card has no Strength-related effect.
pub fn card_player_strength_delivery_v1(card: CardId) -> Option<CardPlayerStrengthDeliveryV1> {
    use CardPlayerStrengthDeliveryV1::*;
    match card {
        CardId::Inflame => Some(OnPlay),
        CardId::SpotWeakness => Some(IfTargetIntentBaseDamageNonnegativeAtResolution),
        CardId::Flex => Some(OnPlayWithSeparateLoseStrength),
        CardId::DemonForm => Some(EachPlayerTurnStartPostDraw),
        _ => None,
    }
}

/// Returns the card-owned bonus added to the energy-derived effect of an
/// X-cost card. `None` means the card is not X-cost; `Some(0)` distinguishes
/// an X-cost card whose upgrade changes another part of the effect.
///
/// Chemical X is deliberately absent here. Relics modify the shared X value,
/// while this query owns only the concrete card's upgrade-sensitive addition.
pub fn x_cost_card_effect_bonus_v1(card: CardId, upgrades: u8) -> Option<u8> {
    if get_card_definition(card).cost != -1 {
        return None;
    }

    Some(u8::from(
        upgrades > 0
            && matches!(
                card,
                CardId::Tempest
                    | CardId::MultiCast
                    | CardId::Doppelganger
                    | CardId::Malaise
                    | CardId::Collect
                    | CardId::ConjureBlade
            ),
    ))
}

/// Returns the number of cards unconditionally requested after a successful
/// play of this card. `effective_magic` is the already evaluated magic number
/// for the concrete card. Conditional draws (Dropkick, Heel Hook, Impatience,
/// Sanctity), hand-size draws, and trigger-only cards return `None`; callers
/// can still observe their exact drawn-card events without pretending the
/// request was unconditional.
pub fn unconditional_card_draw_request_v1(card: CardId, effective_magic: i32) -> Option<u32> {
    use CardId::*;
    match card {
        ShrugItOff | Finesse | FlashOfSteel | DaggerThrow | EscapePlan | QuickSlash
        | CutThroughFate => Some(1),
        ThinkingAhead => Some(2),
        PommelStrike | BurningPact | BattleTrance | Offering | Warcry | DeepBreath | Insight
        | MasterOfStrategy | Coolheaded | Overclock | Reboot | Scrape | Skim | SweepingBeam
        | Acrobatics | Adrenaline | Backflip | Prepared | EmptyMind | WheelKick => {
            Some(effective_magic.max(0) as u32)
        }
        _ => None,
    }
}

/// How the card's base Block field participates in play. This is a reviewed
/// delivery rule, before evaluated instance values, No Block or other powers.
/// It does not describe every Block effect the card may have.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardBaseBlockDeliveryV1 {
    OnceOnPlay,
    BaseWithWrathBonus,
    IfPlayerBlockZeroAtUse,
    IfThisDrawHistoryContainsSkill,
    PerNonAttackInHandAtResolution,
    PerEffectiveX,
    PlatedArmorIfChosen,
    ReplacedByDiscardPileSizePlusUpgrade,
}

impl CardBaseBlockDeliveryV1 {
    pub fn has_unconditional_base_request(self) -> bool {
        matches!(self, Self::OnceOnPlay | Self::BaseWithWrathBonus)
    }
}

/// An absent rule is Unknown, not evidence that a card cannot give Block.
pub fn card_base_block_delivery_v1(card: CardId) -> Option<CardBaseBlockDeliveryV1> {
    use CardBaseBlockDeliveryV1::*;
    use CardId::*;
    Some(match card {
        Finesse | GoodInstincts | PanicButton | Safety | BootSequence | ConserveBattery
        | DefendB | Equilibrium | ForceField | GeneticAlgorithm | Glacier | Hologram | Leap
        | SteamBarrier | Armaments | Defend | FlameBarrier | GhostlyArmor | Impervious
        | IronWave | PowerThrough | Sentinel | ShrugItOff | TrueGrit | Backflip | Blur
        | CloakAndDagger | Dash | DefendG | Deflect | DodgeAndRoll | LegSweep | Survivor
        | DeceiveReality | DefendP | EmptyBody | Evaluate | JustLucky | Perseverance
        | Prostrate | Protect | Sanctity | Swivel | ThirdEye | Vigilance => OnceOnPlay,
        Halt => BaseWithWrathBonus,
        AutoShields => IfPlayerBlockZeroAtUse,
        EscapePlan => IfThisDrawHistoryContainsSkill,
        SecondWind => PerNonAttackInHandAtResolution,
        ReinforcedBody => PerEffectiveX,
        Wish => PlatedArmorIfChosen,
        Stack => ReplacedByDiscardPileSizePlusUpgrade,
        _ => return None,
    })
}

/// Damage requested by a card that remains in hand when the player ends the
/// turn. `blockable` is resolved before enemy attacks and therefore consumes
/// the same block pool; `hp_loss` bypasses block. Cards whose end-turn trigger
/// does not damage the player return zeroes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EndTurnHandDamageV1 {
    pub blockable: i32,
    pub hp_loss: i32,
}

/// Direct HP loss paid when this card is played. This is not attack damage:
/// block does not prevent it, Intangible caps losses above one to one, and
/// Rupture observes the actual HP-loss event.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardSelfHpLossOnPlayV1 {
    pub base_hp_loss: i32,
    pub bypasses_block: bool,
    pub bypasses_intangible: bool,
    pub triggers_rupture: bool,
}

/// Unconditional Block granted at the end of every player turn after this
/// card's persistent combat effect is applied.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardEndOfTurnBlockEachTurnV1 {
    pub amount: i32,
    pub persists_for_combat: bool,
    pub stacks_additively: bool,
    pub affected_by_dexterity: bool,
    pub affected_by_frail: bool,
}

pub fn card_end_of_turn_block_each_turn_v1(
    card: CardId,
    upgrades: u8,
) -> Option<CardEndOfTurnBlockEachTurnV1> {
    let definition = get_card_definition(card);
    let amount = match card {
        CardId::Metallicize => {
            definition.base_magic + definition.upgrade_magic * i32::from(upgrades)
        }
        _ => return None,
    };
    Some(CardEndOfTurnBlockEachTurnV1 {
        amount,
        persists_for_combat: true,
        stacks_additively: true,
        affected_by_dexterity: false,
        affected_by_frail: false,
    })
}

pub fn card_self_hp_loss_on_play_v1(card: CardId, upgrades: u8) -> Option<CardSelfHpLossOnPlayV1> {
    let definition = get_card_definition(card);
    let base_hp_loss = match card {
        CardId::Bloodletting | CardId::JAX => 3,
        CardId::Hemokinesis => {
            definition.base_magic + definition.upgrade_magic * i32::from(upgrades)
        }
        CardId::Offering => 6,
        _ => return None,
    };
    Some(CardSelfHpLossOnPlayV1 {
        base_hp_loss,
        bypasses_block: true,
        bypasses_intangible: false,
        triggers_rupture: true,
    })
}

pub fn end_turn_hand_damage_v1(
    card: CardId,
    upgrades: u8,
    hand_size: usize,
) -> EndTurnHandDamageV1 {
    match card {
        CardId::Burn => EndTurnHandDamageV1 {
            blockable: if upgrades > 0 { 4 } else { 2 },
            hp_loss: 0,
        },
        CardId::Decay => EndTurnHandDamageV1 {
            blockable: 2,
            hp_loss: 0,
        },
        CardId::Regret => EndTurnHandDamageV1 {
            blockable: 0,
            hp_loss: hand_size as i32,
        },
        _ => EndTurnHandDamageV1::default(),
    }
}

/// Returns stable mechanical facts for one concrete upgrade state.
///
/// The broad positive-role registry is intentionally allowed to be partial.
/// The small `Complete` set is the reviewed Ironclad Sentries transfer surface;
/// expanding that set requires production-action tests in the same change.
pub fn card_mechanic_profile(card: CardId, upgrades: u8) -> CardMechanicProfile {
    use CardId::*;
    use CardMechanicCoverage::{Complete, DefinitionOnly, Partial};
    use CardMechanicRole as Role;

    let upgraded = upgrades > 0;
    let mut roles = CardMechanicRoleSet::default();

    if matches!(
        card,
        PommelStrike
            | ShrugItOff
            | BurningPact
            | BattleTrance
            | Offering
            | Warcry
            | Finesse
            | FlashOfSteel
            | DeepBreath
            | Impatience
            | Insight
            | MasterOfStrategy
            | ThinkingAhead
            | Coolheaded
            | Overclock
            | Reboot
            | Scrape
            | Skim
            | SweepingBeam
            | Acrobatics
            | Adrenaline
            | Backflip
            | DaggerThrow
            | EscapePlan
            | Expertise
            | Prepared
            | QuickSlash
            | Reflex
            | CutThroughFate
            | EmptyMind
            | Sanctity
            | Scrawl
            | WheelKick
    ) {
        roles.insert(Role::DrawCards);
    }
    if matches!(
        card,
        Offering
            | SeeingRed
            | Bloodletting
            | Adrenaline
            | Turbo
            | Tactician
            | HeelHook
            | Dropkick
            | Aggregate
            | DoubleEnergy
    ) {
        roles.insert(Role::GainEnergy);
    }
    if matches!(
        card,
        Clothesline
            | Uppercut
            | Shockwave
            | Blind
            | SuckerPunch
            | GoForTheEyes
            | SashWhip
            | WaveOfTheHand
    ) {
        roles.insert(Role::ApplyWeak);
    }
    if matches!(
        card,
        Bash | ThunderClap | Uppercut | Shockwave | Trip | Terror | BeamCell | CrushJoints
    ) {
        roles.insert(Role::ApplyVulnerable);
    }
    if matches!(
        card,
        BurningPact | TrueGrit | SecondWind | SeverSoul | FiendFire | Recycle | Purity
    ) {
        roles.insert(Role::ExhaustOtherCards);
    }
    if matches!(card, BurningPact | DualWield | Forethought | Setup)
        || (card == TrueGrit && upgraded)
        || (card == Armaments && !upgraded)
        || card == ThinkingAhead
    {
        roles.insert(Role::SelectFromHand);
    }
    if matches!(
        card,
        Acrobatics | Prepared | DaggerThrow | CalculatedGamble | Survivor
    ) {
        roles.insert(Role::DiscardFromHand);
    }
    if matches!(
        card,
        ThinkingAhead
            | Warcry
            | Headbutt
            | Forethought
            | Setup
            | DeepBreath
            | Reboot
            | CutThroughFate
            | ThirdEye
    ) {
        roles.insert(Role::DrawPileControl);
    }
    if matches!(
        card,
        TwinStrike
            | SwordBoomerang
            | Whirlwind
            | Skewer
            | Pummel
            | FiendFire
            | RiddleWithHoles
            | Flechettes
            | Finisher
            | DaggerSpray
            | GlassKnife
            | RipAndTear
            | ThunderStrike
            | Barrage
            | Ragnarok
            | Tantrum
            | FlyingSleeves
    ) {
        roles.insert(Role::MultiHit);
    }
    if matches!(card, SwordBoomerang | RipAndTear | ThunderStrike | Ragnarok) {
        roles.insert(Role::RandomTarget);
    }
    if matches!(
        card,
        WildStrike
            | RecklessCharge
            | PowerThrough
            | Immolate
            | Discovery
            | InfernalBlade
            | JackOfAllTrades
            | WhiteNoise
            | Chrysalis
            | Metamorphosis
            | Transmutation
            | Magnetism
            | BladeDance
            | EndlessAgony
            | HelloWorld
            | ForeignInfluence
            | DeceiveReality
            | BattleHymn
    ) {
        roles.insert(Role::GenerateCard);
    }
    if matches!(card, WildStrike | RecklessCharge | PowerThrough | Immolate) {
        roles.insert(Role::GenerateStatusCard);
    }
    if matches!(
        card,
        Bloodletting | Offering | Hemokinesis | Combust | Brutality | JAX | Regret | Pain
    ) {
        roles.insert(Role::SelfHpLoss);
    }
    if matches!(card, Burn | Regret | Decay | Doubt | Pride | Shame) {
        roles.insert(Role::EndTurnHandTrigger);
    }
    if card == Evolve {
        roles.insert(Role::DrawOnStatus);
    }
    if matches!(card, Evolve | FireBreathing) {
        roles.insert(Role::StatusDrawPayoff);
    }
    if matches!(card, Regret | FiendFire) {
        roles.insert(Role::HandSizeScaling);
    }
    if matches!(card, FeelNoPain | DarkEmbrace) {
        roles.insert(Role::ExhaustPayoff);
    }
    if card == BattleTrance {
        roles.insert(Role::PreventFurtherDrawThisTurn);
    }
    if card == Disarm {
        roles.insert(Role::PersistentEnemyAttackReduction);
    }
    if matches!(
        card,
        SwordBoomerang
            | Discovery
            | InfernalBlade
            | JackOfAllTrades
            | WhiteNoise
            | Chrysalis
            | Metamorphosis
            | Transmutation
            | Magnetism
            | Mayhem
    ) || (card == TrueGrit && !upgraded)
    {
        roles.insert(Role::RandomOutcome);
    }

    let coverage = if matches!(
        card,
        Strike
            | Defend
            | Bash
            | AscendersBane
            | Evolve
            | PommelStrike
            | ShrugItOff
            | SwordBoomerang
            | ThinkingAhead
            | TrueGrit
            | Regret
            | FireBreathing
            | RecklessCharge
    ) {
        Complete
    } else if roles.iter().next().is_some() {
        Partial
    } else {
        DefinitionOnly
    };

    CardMechanicProfile {
        coverage,
        roles,
        effective_target: effective_card_target(card, upgrades),
        exhausts_when_played: card_exhausts_when_played(card, upgrades),
        ethereal: card_is_ethereal(card, upgrades),
    }
}

#[cfg(test)]
mod tests {
    use crate::content::cards::{
        resolve_card_play, resolve_card_play_with_context, CardUseContext,
    };
    use crate::content::powers::{self, PowerId};
    use crate::runtime::action::{Action, DamageType};
    use crate::runtime::combat::CombatCard;

    use super::*;

    #[test]
    fn persistent_base_numbers_match_runtime_precedence_and_upgrade_rules() {
        let state = crate::test_support::blank_test_combat();
        for (id, upgrades, misc, damage_override, block_override, damage, block) in [
            (CardId::Strike, 2, 0, None, None, 9, 0),
            (CardId::Strike, 1, 0, Some(17), None, 17, 0),
            (CardId::SearingBlow, 5, 0, Some(99), None, 42, 0),
            (CardId::RitualDagger, 1, 37, Some(99), None, 37, 0),
            (CardId::RitualDagger, 1, 0, Some(99), None, 15, 0),
            (CardId::GeneticAlgorithm, 1, 28, None, Some(99), 0, 28),
            (CardId::GeneticAlgorithm, 1, 0, None, Some(99), 0, 1),
            (CardId::Defend, 2, 0, None, None, 0, 8),
            (CardId::Defend, 1, 0, None, Some(17), 0, 17),
        ] {
            let mut card = CombatCard::for_scenario(id, 1);
            card.upgrades = upgrades;
            card.misc_value = misc;
            card.base_damage_override = damage_override;
            card.base_block_override = block_override;
            card.base_damage_mut = 999;
            card.base_block_mut = 999;
            assert_eq!(
                card_persistent_base_numbers_v1(&card),
                CardPersistentBaseNumbersV1 { damage, block },
                "{id:?}"
            );
            let evaluated = crate::content::cards::evaluate_card_for_play(&card, &state, None);
            assert_eq!(
                (evaluated.base_damage_mut, evaluated.base_block_mut),
                (damage, block),
                "{id:?}"
            );
        }
    }

    #[test]
    fn reviewed_once_on_play_block_matches_production_requests() {
        let mut state = crate::test_support::blank_test_combat();
        let mut monster =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
        monster.id = 7;
        state.entities.monsters = vec![monster];
        state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Defend, 2)].into();
        let mut cases = 0;
        for id in CardId::STABLE_IDS
            .iter()
            .map(|id| CardId::from_stable_id(id).unwrap())
        {
            if card_base_block_delivery_v1(id) != Some(CardBaseBlockDeliveryV1::OnceOnPlay) {
                continue;
            }
            for upgrades in [0, 1] {
                let mut card = CombatCard::for_scenario(id, 3);
                card.upgrades = upgrades;
                let evaluated =
                    crate::content::cards::evaluate_card_for_play(&card, &state, Some(7));
                let requests: Vec<_> = resolve_card_play(id, &state, &card, Some(7))
                    .into_iter()
                    .filter_map(|info| match info.action {
                        Action::GainBlock { target: 0, amount } => Some(amount),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    requests,
                    vec![evaluated.base_block_mut],
                    "{id:?}+{upgrades}"
                );
                cases += 1;
            }
        }
        assert!(cases > 0);
        println!("validated {cases} upgraded/base once-on-play Block requests");
    }

    #[test]
    fn conditional_block_matches_production_branch_and_draw_history() {
        use crate::runtime::combat::DrawnCardRecord;
        let settle = |state: &mut crate::runtime::combat::CombatState, card: &CombatCard| {
            state.queue_actions(resolve_card_play(card.id, state, card, None));
            let mut steps = 0;
            while let Some(action) = state.pop_next_action() {
                steps += 1;
                assert!(steps < 50, "conditional block probe must settle");
                crate::engine::action_handlers::execute_action(action, state);
            }
        };
        for upgrades in [0, 1] {
            let mut auto = CombatCard::for_scenario(CardId::AutoShields, 1);
            auto.upgrades = upgrades;
            assert_eq!(
                card_base_block_delivery_v1(auto.id),
                Some(CardBaseBlockDeliveryV1::IfPlayerBlockZeroAtUse)
            );
            for before in [0, 5] {
                let mut state = crate::test_support::blank_test_combat();
                state.entities.player.block = before;
                settle(&mut state, &auto);
                assert_eq!(
                    state.entities.player.block,
                    if before == 0 {
                        11 + 4 * i32::from(upgrades)
                    } else {
                        before
                    }
                );
            }
            let mut escape = CombatCard::for_scenario(CardId::EscapePlan, 1);
            escape.upgrades = upgrades;
            assert_eq!(
                card_base_block_delivery_v1(escape.id),
                Some(CardBaseBlockDeliveryV1::IfThisDrawHistoryContainsSkill)
            );
            for drawn in [CardId::DefendG, CardId::StrikeG] {
                for no_draw in [false, true] {
                    let mut state = crate::test_support::blank_test_combat();
                    state.zones.draw_pile = vec![CombatCard::for_scenario(drawn, 2)].into();
                    state.runtime.last_drawn_cards = vec![DrawnCardRecord {
                        card_lineage: crate::test_support::scenario_lineage(3),
                        card_id: CardId::DefendG,
                    }];
                    if no_draw {
                        crate::engine::action_handlers::execute_action(
                            Action::ApplyPower {
                                source: 0,
                                target: 0,
                                power_id: PowerId::NoDraw,
                                amount: 1,
                            },
                            &mut state,
                        );
                    }
                    settle(&mut state, &escape);
                    let expected = if drawn == CardId::DefendG && !no_draw {
                        3 + 2 * i32::from(upgrades)
                    } else {
                        0
                    };
                    assert_eq!(
                        state.entities.player.block, expected,
                        "{drawn:?} NoDraw={no_draw}"
                    );
                }
            }
        }
    }

    #[test]
    fn searing_blow_base_projection_and_runtime_follow_repeated_java_upgrades() {
        let mut card = CombatCard::for_scenario(CardId::SearingBlow, 42);
        let state = crate::test_support::blank_test_combat();
        for expected in [12, 16, 21, 27, 34, 42, 51] {
            assert_eq!(
                card_base_damage_for_upgrades_v1(&get_card_definition(card.id), card.upgrades),
                expected
            );
            let evaluated = crate::content::cards::evaluate_card_for_play(&card, &state, None);
            assert_eq!(evaluated.base_damage_mut, expected);
            assert!(crate::content::cards::upgrade_card_once_java(&mut card));
        }
        // Ordinary cards only apply their one normal upgrade even if an
        // externally supplied count exceeds one, matching runtime evaluation.
        assert_eq!(
            card_base_damage_for_upgrades_v1(&get_card_definition(CardId::Strike), 2),
            9
        );
    }

    #[test]
    fn reviewed_unconditional_draw_requests_match_production_queues() {
        use crate::content::monsters::EnemyId;
        use CardId::*;
        let mut state = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        state.entities.monsters = vec![monster];
        state.zones.hand = vec![CombatCard::for_scenario(Strike, 1)];
        state.zones.discard_pile = vec![CombatCard::for_scenario(Defend, 2)].into();
        for id in [
            ShrugItOff,
            Finesse,
            FlashOfSteel,
            DaggerThrow,
            EscapePlan,
            QuickSlash,
            CutThroughFate,
            ThinkingAhead,
            PommelStrike,
            BurningPact,
            BattleTrance,
            Offering,
            Warcry,
            DeepBreath,
            Insight,
            MasterOfStrategy,
            Coolheaded,
            Overclock,
            Reboot,
            Scrape,
            Skim,
            SweepingBeam,
            Acrobatics,
            Adrenaline,
            Backflip,
            Prepared,
            EmptyMind,
            WheelKick,
        ] {
            for upgrades in [0, 1] {
                let mut card = CombatCard::for_scenario(id, 3);
                card.upgrades = upgrades;
                let evaluated =
                    crate::content::cards::evaluate_card_for_play(&card, &state, Some(7));
                let declared =
                    unconditional_card_draw_request_v1(id, evaluated.base_magic_num_mut).unwrap();
                let actions = resolve_card_play(id, &state, &card, Some(7));
                let requested: u32 = actions
                    .iter()
                    .filter_map(|info| match info.action {
                        Action::DrawCards(amount) | Action::DrawCardsWithHistory { amount, .. } => {
                            Some(amount)
                        }
                        _ => None,
                    })
                    .sum();
                assert_eq!(declared, requested, "{id:?}+{upgrades}");
            }
        }
        for id in [
            Dropkick,
            HeelHook,
            Impatience,
            Sanctity,
            Evolve,
            DarkEmbrace,
        ] {
            assert_eq!(unconditional_card_draw_request_v1(id, 3), None, "{id:?}");
        }
    }

    #[test]
    fn strength_restoration_profile_matches_production_actions_and_artifact_timing() {
        use crate::content::monsters::EnemyId;
        use crate::engine::action_handlers::execute_action;
        for (id, base, upgrade_gain, restoration) in [
            (
                CardId::Disarm,
                2,
                1,
                EnemyStrengthRestorationV1::NoScheduledRestoration,
            ),
            (
                CardId::DarkShackles,
                9,
                6,
                EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd,
            ),
            (
                CardId::PiercingWail,
                6,
                2,
                EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd,
            ),
        ] {
            assert_eq!(card_enemy_strength_restoration_v1(id), Some(restoration));
            for upgrades in [0, 1] {
                for artifact in [false, true] {
                    let mut state = crate::test_support::blank_test_combat();
                    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
                    monster.id = 7;
                    state.entities.monsters = vec![monster];
                    if artifact {
                        execute_action(
                            Action::ApplyPower {
                                source: 0,
                                target: 7,
                                power_id: PowerId::Artifact,
                                amount: 1,
                            },
                            &mut state,
                        );
                    }
                    let mut card = CombatCard::for_scenario(id, 1);
                    card.upgrades = upgrades;
                    let actions = resolve_card_play(id, &state, &card, Some(7));
                    let expected_amount = base + upgrade_gain * i32::from(upgrades);
                    let powers: Vec<_> = actions
                        .iter()
                        .filter_map(|info| match info.action {
                            Action::ApplyPower {
                                target,
                                power_id,
                                amount,
                                ..
                            } => Some((target, power_id, amount)),
                            _ => None,
                        })
                        .collect();
                    let mut expected = vec![(7, PowerId::Strength, -expected_amount)];
                    if restoration == EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd
                        && !artifact
                    {
                        expected.push((7, PowerId::Shackled, expected_amount));
                    }
                    assert_eq!(powers, expected, "{id:?}+{upgrades} artifact={artifact}");
                }
            }
        }
        assert_eq!(card_enemy_strength_restoration_v1(CardId::Strike), None);
    }

    #[test]
    fn player_strength_delivery_matches_production_actions_and_post_draw_hook() {
        use CardPlayerStrengthDeliveryV1::*;
        let state = crate::test_support::blank_test_combat();
        for id in CardId::STABLE_IDS
            .iter()
            .map(|id| CardId::from_stable_id(id).unwrap())
        {
            let Some(delivery) = card_player_strength_delivery_v1(id) else {
                continue;
            };
            for upgrades in [0, 1] {
                let def = get_card_definition(id);
                let amount = def.base_magic + i32::from(upgrades) * def.upgrade_magic;
                let mut card = CombatCard::for_scenario(id, 1);
                card.upgrades = upgrades;
                let power = |power_id| Action::ApplyPower {
                    source: 0,
                    target: 0,
                    power_id,
                    amount,
                };
                let expected = match delivery {
                    OnPlay => vec![power(PowerId::Strength)],
                    IfTargetIntentBaseDamageNonnegativeAtResolution => {
                        vec![Action::SpotWeakness { target: 7, amount }]
                    }
                    OnPlayWithSeparateLoseStrength => {
                        vec![power(PowerId::Strength), power(PowerId::LoseStrength)]
                    }
                    EachPlayerTurnStartPostDraw => {
                        let requests = powers::resolve_power_on_post_draw(
                            PowerId::DemonForm,
                            &state,
                            0,
                            amount,
                        );
                        assert_eq!(requests.as_slice(), &[power(PowerId::Strength)]);
                        vec![power(PowerId::DemonForm)]
                    }
                };
                let actual: Vec<_> = resolve_card_play(id, &state, &card, Some(7))
                    .into_iter()
                    .map(|info| info.action)
                    .collect();
                assert_eq!(actual, expected, "{id:?}+{upgrades}");
            }
        }
    }

    #[test]
    fn spot_weakness_checks_resolution_intent_including_zero_damage_attacks() {
        use crate::engine::action_handlers::execute_action;
        for (damage, move_id, expected) in [(0, 3, true), (11, 3, true), (11, 4, false)] {
            let mut state = crate::test_support::blank_test_combat();
            let mut monster =
                crate::test_support::test_monster(crate::content::monsters::EnemyId::LouseNormal);
            monster.id = 7;
            // Construct an attack magnitude, including zero, through the real
            // monster plan owner; display-cache edits do not change its move.
            monster.louse.bite_damage = Some(damage);
            monster.set_planned_move_id(3);
            state.entities.monsters = vec![monster];
            let card = CombatCard::for_scenario(CardId::SpotWeakness, 1);
            // Resolve the card-use action before replacing the target intent.
            let actions = resolve_card_play(card.id, &state, &card, Some(7));
            state.entities.monsters[0].set_planned_move_id(move_id);
            execute_action(actions[0].action.clone(), &mut state);
            assert_eq!(
                state.pop_next_action(),
                expected.then_some(Action::ApplyPower {
                    source: 0,
                    target: 0,
                    power_id: PowerId::Strength,
                    amount: 3,
                })
            );
        }
    }

    fn has(profile: CardMechanicProfile, role: CardMechanicRole) -> bool {
        profile.roles.contains(role)
    }

    #[test]
    fn exhaust_payoff_role_distinguishes_the_engine_from_an_arbitrary_power() {
        assert!(has(
            card_mechanic_profile(CardId::FeelNoPain, 1),
            CardMechanicRole::ExhaustPayoff
        ));
        assert!(has(
            card_mechanic_profile(CardId::DarkEmbrace, 0),
            CardMechanicRole::ExhaustPayoff
        ));
        assert!(!has(
            card_mechanic_profile(CardId::Inflame, 0),
            CardMechanicRole::ExhaustPayoff
        ));
        assert!(has(
            card_mechanic_profile(CardId::BattleTrance, 0),
            CardMechanicRole::PreventFurtherDrawThisTurn
        ));
        assert!(!has(
            card_mechanic_profile(CardId::PommelStrike, 0),
            CardMechanicRole::PreventFurtherDrawThisTurn
        ));
    }

    #[test]
    fn reviewed_status_engine_roles_match_production_actions() {
        use CardMechanicRole as Role;

        let state = crate::test_support::blank_test_combat();
        let fire_breathing = card_mechanic_profile(CardId::FireBreathing, 0);
        let fire_breathing_actions = resolve_card_play(
            CardId::FireBreathing,
            &state,
            &CombatCard::for_scenario(CardId::FireBreathing, 1),
            None,
        );
        assert_eq!(fire_breathing.coverage, CardMechanicCoverage::Complete);
        assert!(has(fire_breathing, Role::StatusDrawPayoff));
        assert!(fire_breathing_actions.iter().any(|action| matches!(
            action.action,
            Action::ApplyPower {
                power_id: PowerId::FireBreathing,
                ..
            }
        )));

        let reckless_charge = card_mechanic_profile(CardId::RecklessCharge, 0);
        let reckless_actions = resolve_card_play(
            CardId::RecklessCharge,
            &state,
            &CombatCard::for_scenario(CardId::RecklessCharge, 2),
            Some(7),
        );
        assert_eq!(reckless_charge.coverage, CardMechanicCoverage::Complete);
        assert!(has(reckless_charge, Role::GenerateStatusCard));
        assert!(reckless_actions.iter().any(|action| matches!(
            action.action,
            Action::MakeTempCardInDrawPile {
                card_id: CardId::Dazed,
                ..
            }
        )));

        assert!(!has(
            card_mechanic_profile(CardId::Discovery, 0),
            Role::GenerateStatusCard
        ));
    }

    #[test]
    fn reviewed_sentries_play_roles_match_production_actions() {
        use CardMechanicRole as Role;

        let mut state = crate::test_support::blank_test_combat();
        state.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 20)];

        let bash = card_mechanic_profile(CardId::Bash, 0);
        let bash_actions = resolve_card_play(
            CardId::Bash,
            &state,
            &CombatCard::for_scenario(CardId::Bash, 1),
            Some(7),
        );
        assert_eq!(bash.coverage, CardMechanicCoverage::Complete);
        assert!(has(bash, Role::ApplyVulnerable));
        assert!(bash_actions.iter().any(|action| matches!(
            action.action,
            Action::ApplyPower {
                power_id: PowerId::Vulnerable,
                ..
            }
        )));

        let evolve = card_mechanic_profile(CardId::Evolve, 1);
        let mut evolve_card = CombatCard::for_scenario(CardId::Evolve, 2);
        evolve_card.upgrades = 1;
        let evolve_actions = resolve_card_play(CardId::Evolve, &state, &evolve_card, None);
        assert!(has(evolve, Role::DrawOnStatus));
        assert!(evolve_actions.iter().any(|action| matches!(
            action.action,
            Action::ApplyPower {
                power_id: PowerId::Evolve,
                amount: 2,
                ..
            }
        )));

        let pommel = card_mechanic_profile(CardId::PommelStrike, 1);
        let mut pommel_card = CombatCard::for_scenario(CardId::PommelStrike, 3);
        pommel_card.upgrades = 1;
        let pommel_actions = resolve_card_play(CardId::PommelStrike, &state, &pommel_card, Some(7));
        assert!(has(pommel, Role::DrawCards));
        assert!(matches!(pommel_actions[1].action, Action::DrawCards(2)));
        assert_eq!(
            unconditional_card_draw_request_v1(CardId::PommelStrike, 2),
            Some(2)
        );

        let shrug = card_mechanic_profile(CardId::ShrugItOff, 0);
        let shrug_actions = resolve_card_play(
            CardId::ShrugItOff,
            &state,
            &CombatCard::for_scenario(CardId::ShrugItOff, 4),
            None,
        );
        assert!(has(shrug, Role::DrawCards));
        assert!(matches!(shrug_actions[1].action, Action::DrawCards(1)));
        assert_eq!(
            unconditional_card_draw_request_v1(CardId::ShrugItOff, 0),
            Some(1)
        );

        let sword = card_mechanic_profile(CardId::SwordBoomerang, 0);
        let sword_actions = resolve_card_play(
            CardId::SwordBoomerang,
            &state,
            &CombatCard::for_scenario(CardId::SwordBoomerang, 5),
            None,
        );
        assert!(has(sword, Role::MultiHit));
        assert!(has(sword, Role::RandomTarget));
        assert_eq!(sword_actions.len(), 3);
        assert!(sword_actions
            .iter()
            .all(|action| matches!(action.action, Action::AttackDamageRandomEnemyCard { .. })));

        let thinking = card_mechanic_profile(CardId::ThinkingAhead, 0);
        let thinking_actions = resolve_card_play_with_context(
            CardId::ThinkingAhead,
            &state,
            &CombatCard::for_scenario(CardId::ThinkingAhead, 6),
            None,
            CardUseContext {
                played_from_hand: true,
            },
        );
        assert!(has(thinking, Role::DrawCards));
        assert!(has(thinking, Role::SelectFromHand));
        assert!(has(thinking, Role::DrawPileControl));
        assert!(matches!(thinking_actions[0].action, Action::DrawCards(2)));
        assert!(matches!(
            thinking_actions[1].action,
            Action::SuspendForHandSelect {
                reason: crate::state::HandSelectReason::PutOnDrawPile,
                ..
            }
        ));
    }

    #[test]
    fn reviewed_upgrade_and_trigger_roles_match_production_actions() {
        use CardMechanicRole as Role;

        let mut state = crate::test_support::blank_test_combat();
        state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 30),
            CombatCard::for_scenario(CardId::Defend, 31),
            CombatCard::for_scenario(CardId::Regret, 32),
            CombatCard::for_scenario(CardId::Wound, 33),
        ];

        let true_grit_base = card_mechanic_profile(CardId::TrueGrit, 0);
        let base_actions = resolve_card_play(
            CardId::TrueGrit,
            &state,
            &CombatCard::for_scenario(CardId::TrueGrit, 7),
            None,
        );
        assert!(has(true_grit_base, Role::ExhaustOtherCards));
        assert!(has(true_grit_base, Role::RandomOutcome));
        assert!(!has(true_grit_base, Role::SelectFromHand));
        assert!(matches!(
            base_actions[1].action,
            Action::ExhaustFromHand { random: true, .. }
        ));

        let true_grit_plus = card_mechanic_profile(CardId::TrueGrit, 1);
        let mut upgraded_card = CombatCard::for_scenario(CardId::TrueGrit, 8);
        upgraded_card.upgrades = 1;
        let upgraded_actions = resolve_card_play(CardId::TrueGrit, &state, &upgraded_card, None);
        assert!(has(true_grit_plus, Role::SelectFromHand));
        assert!(!has(true_grit_plus, Role::RandomOutcome));
        assert!(matches!(
            upgraded_actions[1].action,
            Action::ExhaustFromHand { random: false, .. }
        ));

        let armaments_base = card_mechanic_profile(CardId::Armaments, 0);
        let armaments_plus = card_mechanic_profile(CardId::Armaments, 1);
        assert!(has(armaments_base, Role::SelectFromHand));
        assert!(!has(armaments_plus, Role::SelectFromHand));

        let thinking_base = card_mechanic_profile(CardId::ThinkingAhead, 0);
        let thinking_plus = card_mechanic_profile(CardId::ThinkingAhead, 1);
        assert!(thinking_base.exhausts_when_played);
        assert!(!thinking_plus.exhausts_when_played);

        let regret = card_mechanic_profile(CardId::Regret, 0);
        let regret_actions = crate::content::cards::curses::regret::on_end_turn_in_hand(&state);
        assert!(has(regret, Role::EndTurnHandTrigger));
        assert!(has(regret, Role::SelfHpLoss));
        assert!(has(regret, Role::HandSizeScaling));
        assert!(matches!(
            regret_actions[0].action,
            Action::Damage(ref info)
                if info.target == 0
                    && info.output == 4
                    && info.damage_type == DamageType::HpLoss
        ));

        let evolve_trigger = powers::resolve_power_on_card_drawn(
            PowerId::Evolve,
            &state,
            0,
            2,
            crate::runtime::combat::CardLineageId::constructed_scenario(33),
        );
        assert_eq!(evolve_trigger.as_slice(), &[Action::DrawCards(2)]);
    }

    #[test]
    fn effective_attributes_and_role_coverage_are_explicit() {
        assert_eq!(
            card_mechanic_profile(CardId::Blind, 0).effective_target,
            CardTarget::Enemy
        );
        assert_eq!(
            card_mechanic_profile(CardId::Blind, 1).effective_target,
            CardTarget::AllEnemy
        );
        assert!(card_mechanic_profile(CardId::Apparition, 0).ethereal);
        assert!(!card_mechanic_profile(CardId::Apparition, 1).ethereal);
        assert_eq!(
            card_mechanic_profile(CardId::Offering, 0).coverage,
            CardMechanicCoverage::Partial
        );
        assert_eq!(
            card_mechanic_profile(CardId::Bludgeon, 0).coverage,
            CardMechanicCoverage::DefinitionOnly
        );
    }

    #[test]
    fn x_cost_effect_bonus_distinguishes_energy_x_from_card_upgrade_additions() {
        assert_eq!(x_cost_card_effect_bonus_v1(CardId::Strike, 0), None);
        assert_eq!(x_cost_card_effect_bonus_v1(CardId::Whirlwind, 1), Some(0));
        assert_eq!(x_cost_card_effect_bonus_v1(CardId::Malaise, 0), Some(0));
        assert_eq!(x_cost_card_effect_bonus_v1(CardId::Malaise, 1), Some(1));
    }
}
