//! Reviewed use of damage definition fields. These are requests before combat
//! modifiers, not resolved HP loss, guaranteed hits, or a complete card model.

use serde::{Deserialize, Serialize};

use crate::content::cards::{get_card_definition, CardId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CardBaseDamageDeliveryV1 {
    Attack {
        amount: CardAttackAmountV1,
        hits: CardAttackHitsV1,
        targets: CardAttackTargetsV1,
        play_condition: Option<CardAttackPlayConditionV1>,
    },
    /// Wish's damage field is a Strength choice parameter, never an attack.
    StrengthIfChosen,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CardAttackAmountV1 {
    Base,
    BasePlusStrikeCards { per_card: i32 },
    BasePlusMantraGainedThisCombat,
    PlayerBlock,
    DrawPileSize,
    FrostChanneledThisCombat { per_orb: i32 },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CardAttackHitsV1 {
    Fixed { count: i32 },
    OncePlusIfTargetPoisonedAtResolution,
    PerEffectiveX,
    PerHandCardAtResolutionBeforeExhaust,
    PerSkillInHandAtResolution,
    PerFilledOrbAtResolution,
    PerPriorAttackThisTurnAtResolution,
    PerLightningChanneledThisCombatAtUse,
    PerLivingEnemyAtUse,
    FromInstanceMisc,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAttackTargetsV1 {
    SelectedEnemy,
    EachEnemy,
    RandomEnemyPerHit,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAttackPlayConditionV1 {
    HandContainsOnlyAttacks,
    DrawPileEmpty,
    NoOtherAttackInHand,
}

impl CardBaseDamageDeliveryV1 {
    pub fn is_attack(self) -> bool {
        matches!(self, Self::Attack { .. })
    }

    /// Count for the unconditional *base contribution* of a fresh card. Bane's
    /// extra hit and additive card-specific damage are not included. Zero is
    /// not substituted for an unknown/variable count or an unmet play condition.
    pub fn unconditional_base_hits(self) -> Option<i32> {
        let Self::Attack {
            amount,
            hits,
            play_condition: None,
            ..
        } = self
        else {
            return None;
        };
        if !matches!(
            amount,
            CardAttackAmountV1::Base
                | CardAttackAmountV1::BasePlusStrikeCards { .. }
                | CardAttackAmountV1::BasePlusMantraGainedThisCombat
        ) {
            return None;
        }
        match hits {
            CardAttackHitsV1::Fixed { count } => Some(count),
            CardAttackHitsV1::OncePlusIfTargetPoisonedAtResolution => Some(1),
            _ => None,
        }
    }

    pub fn has_context_dependent_attack(self) -> bool {
        matches!(self, Self::Attack { .. })
            && !matches!(
                self,
                Self::Attack {
                    amount: CardAttackAmountV1::Base,
                    hits: CardAttackHitsV1::Fixed { .. },
                    play_condition: None,
                    ..
                }
            )
    }

    pub fn targets(self) -> Option<CardAttackTargetsV1> {
        match self {
            Self::Attack { targets, .. } => Some(targets),
            Self::StrengthIfChosen => None,
        }
    }
}

/// Explicitly reviewed card-owned damage delivery. Missing means Unknown;
/// neither a positive definition field nor CardType::Attack is a fallback.
pub fn card_base_damage_delivery_v1(
    card: CardId,
    upgrades: u8,
) -> Option<CardBaseDamageDeliveryV1> {
    use CardAttackAmountV1 as Amount;
    use CardAttackHitsV1 as Hits;
    use CardAttackPlayConditionV1 as Condition;
    use CardAttackTargetsV1 as Targets;
    use CardId::*;

    if card == Wish {
        return Some(CardBaseDamageDeliveryV1::StrengthIfChosen);
    }
    let definition = get_card_definition(card);
    let magic = definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic;
    let hits = match card {
        TwinStrike | GlassKnife | FlyingSleeves | DaggerSpray => Hits::Fixed { count: 2 },
        Eviscerate => Hits::Fixed { count: 3 },
        RiddleWithHoles => Hits::Fixed { count: 5 },
        Pummel | SwordBoomerang | Tantrum | Ragnarok | RipAndTear => Hits::Fixed { count: magic },
        Bane => Hits::OncePlusIfTargetPoisonedAtResolution,
        Whirlwind | Skewer => Hits::PerEffectiveX,
        FiendFire => Hits::PerHandCardAtResolutionBeforeExhaust,
        Flechettes => Hits::PerSkillInHandAtResolution,
        Barrage => Hits::PerFilledOrbAtResolution,
        Finisher => Hits::PerPriorAttackThisTurnAtResolution,
        ThunderStrike => Hits::PerLightningChanneledThisCombatAtUse,
        BowlingBash => Hits::PerLivingEnemyAtUse,
        Expunger => Hits::FromInstanceMisc,
        Bite | DramaticEntrance | FlashOfSteel | HandOfGreed | MindBlast | RitualDagger | Smite
        | SwiftStrike | ThroughViolence | Shiv | AllForOne | BallLightning | BeamCell
        | Blizzard | Claw | ColdSnap | CompileDriver | CoreSurge | DoomAndGloom | Ftl
        | GoForTheEyes | Hyperbeam | LockOn | Melter | MeteorStrike | Rebound | Scrape
        | Streamline | StrikeB | Sunder | SweepingBeam | Anger | Bash | BloodForBlood
        | Bludgeon | BodySlam | Carnage | Clash | Cleave | Clothesline | Dropkick | Feed
        | Headbutt | HeavyBlade | Hemokinesis | Immolate | IronWave | PerfectedStrike
        | PommelStrike | Rampage | Reaper | RecklessCharge | SearingBlow | SeverSoul | Strike
        | ThunderClap | Uppercut | WildStrike | AllOutAttack | Backstab | Choke | DaggerThrow
        | Dash | DieDieDie | EndlessAgony | FlyingKnee | GrandFinale | HeelHook | MasterfulStab
        | Neutralize | PoisonedStab | Predator | QuickSlash | Slice | SneakyStrike | StrikeG
        | SuckerPunch | Unload | Brilliance | CarveReality | Conclude | Consecrate
        | CrushJoints | CutThroughFate | EmptyFist | Eruption | FearNoEvil | FlurryOfBlows
        | FollowUp | JustLucky | LessonLearned | ReachHeaven | SandsOfTime | SashWhip
        | SignatureMove | StrikeP | TalkToTheHand | Wallop | Weave | WheelKick | WindmillStrike => {
            Hits::Fixed { count: 1 }
        }
        _ => return None,
    };
    let amount = match card {
        BodySlam => Amount::PlayerBlock,
        MindBlast => Amount::DrawPileSize,
        Blizzard => Amount::FrostChanneledThisCombat { per_orb: magic },
        PerfectedStrike => Amount::BasePlusStrikeCards { per_card: magic },
        Brilliance => Amount::BasePlusMantraGainedThisCombat,
        _ => Amount::Base,
    };
    let targets = match card {
        SwordBoomerang | RipAndTear | Ragnarok | ThunderStrike => Targets::RandomEnemyPerHit,
        DramaticEntrance | Blizzard | DoomAndGloom | Hyperbeam | SweepingBeam | Cleave
        | Immolate | Reaper | ThunderClap | Whirlwind | AllOutAttack | DaggerSpray | DieDieDie
        | GrandFinale | Conclude | Consecrate => Targets::EachEnemy,
        _ => Targets::SelectedEnemy,
    };
    let play_condition = match card {
        Clash => Some(Condition::HandContainsOnlyAttacks),
        GrandFinale => Some(Condition::DrawPileEmpty),
        SignatureMove => Some(Condition::NoOtherAttackInHand),
        _ => None,
    };
    Some(CardBaseDamageDeliveryV1::Attack {
        amount,
        hits,
        targets,
        play_condition,
    })
}

#[cfg(test)]
mod tests;
