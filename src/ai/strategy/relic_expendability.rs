use crate::content::relics::{RelicId, RelicState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelicExpendability {
    FreeToFeed,
    Situational,
    Keep,
}

pub fn relic_expendability(relic: &RelicState) -> RelicExpendability {
    if is_junk_relic(relic.id) || is_spent_relic(relic) || is_pickup_only_relic(relic.id) {
        RelicExpendability::FreeToFeed
    } else if is_situational_relic(relic.id) {
        RelicExpendability::Situational
    } else {
        RelicExpendability::Keep
    }
}

pub fn nloth_free_feed_priority(relic: &RelicState) -> Option<u8> {
    match relic_expendability(relic) {
        RelicExpendability::FreeToFeed if is_junk_relic(relic.id) => Some(30),
        RelicExpendability::FreeToFeed if is_spent_relic(relic) => Some(20),
        RelicExpendability::FreeToFeed => Some(10),
        RelicExpendability::Situational | RelicExpendability::Keep => None,
    }
}

fn is_junk_relic(id: RelicId) -> bool {
    matches!(
        id,
        RelicId::SpiritPoop | RelicId::GremlinMask | RelicId::NlothsMask
    )
}

fn is_spent_relic(relic: &RelicState) -> bool {
    match relic.id {
        RelicId::NeowsLament | RelicId::Omamori | RelicId::Matryoshka | RelicId::WingBoots => {
            relic.used_up || relic.counter <= 0
        }
        RelicId::LizardTail | RelicId::MawBank => relic.used_up,
        _ => false,
    }
}

fn is_pickup_only_relic(id: RelicId) -> bool {
    matches!(
        id,
        RelicId::Astrolabe
            | RelicId::CallingBell
            | RelicId::Cauldron
            | RelicId::DollysMirror
            | RelicId::EmptyCage
            | RelicId::Mango
            | RelicId::OldCoin
            | RelicId::Orrery
            | RelicId::PandorasBox
            | RelicId::Pear
            | RelicId::Strawberry
            | RelicId::TinyHouse
            | RelicId::Waffle
            | RelicId::WarPaint
            | RelicId::Whetstone
    )
}

fn is_situational_relic(id: RelicId) -> bool {
    matches!(
        id,
        RelicId::CeramicFish
            | RelicId::DarkstonePeriapt
            | RelicId::DreamCatcher
            | RelicId::Girya
            | RelicId::JuzuBracelet
            | RelicId::PeacePipe
            | RelicId::PotionBelt
            | RelicId::Shovel
            | RelicId::TinyChest
    )
}
