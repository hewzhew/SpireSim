use crate::content::cards::{CardId, CardRarity, CardType};
use crate::content::relics::{RelicId, RelicTier};
use crate::state::map::node::RoomType;
use crate::state::run::RunState;

#[derive(Clone)]
pub(crate) struct RelicSpawnContext {
    owned_relics: Vec<RelicId>,
    master_deck: Vec<CardId>,
    floor_num: i32,
    act_num: u8,
    current_room_type: Option<RoomType>,
}

impl RelicSpawnContext {
    pub(crate) fn from_run(run_state: &RunState) -> Self {
        Self {
            owned_relics: run_state.relics.iter().map(|relic| relic.id).collect(),
            master_deck: run_state.master_deck.iter().map(|card| card.id).collect(),
            floor_num: run_state.floor_num,
            act_num: run_state.act_num,
            current_room_type: run_state.map.get_current_room_type(),
        }
    }

    fn has_relic(&self, id: RelicId) -> bool {
        self.owned_relics.contains(&id)
    }

    fn campfire_relic_count(&self) -> usize {
        self.owned_relics
            .iter()
            .filter(|&&id| matches!(id, RelicId::Girya | RelicId::PeacePipe | RelicId::Shovel))
            .count()
    }

    fn is_current_shop_room(&self) -> bool {
        self.current_room_type == Some(RoomType::ShopRoom)
    }
}

pub(crate) struct RelicPoolsMut<'a> {
    pub(crate) common: &'a mut Vec<RelicId>,
    pub(crate) uncommon: &'a mut Vec<RelicId>,
    pub(crate) rare: &'a mut Vec<RelicId>,
    pub(crate) shop: &'a mut Vec<RelicId>,
    pub(crate) boss: &'a mut Vec<RelicId>,
}

pub(crate) fn relic_can_spawn_in_context(id: RelicId, context: &RelicSpawnContext) -> bool {
    match id {
        RelicId::BlackBlood => context.has_relic(RelicId::BurningBlood),
        RelicId::FrozenCore => context.has_relic(RelicId::CrackedCore),
        RelicId::HolyWater => context.has_relic(RelicId::PureWater),
        RelicId::RingOfTheSerpent => context.has_relic(RelicId::SnakeRing),
        RelicId::BottledFlame => context.master_deck.iter().any(|&card_id| {
            let def = crate::content::cards::get_card_definition(card_id);
            def.card_type == CardType::Attack && def.rarity != CardRarity::Basic
        }),
        RelicId::BottledLightning => context.master_deck.iter().any(|&card_id| {
            let def = crate::content::cards::get_card_definition(card_id);
            def.card_type == CardType::Skill && def.rarity != CardRarity::Basic
        }),
        RelicId::BottledTornado => context.master_deck.iter().any(|&card_id| {
            crate::content::cards::get_card_definition(card_id).card_type == CardType::Power
        }),
        RelicId::AncientTeaSet
        | RelicId::CeramicFish
        | RelicId::DarkstonePeriapt
        | RelicId::DreamCatcher
        | RelicId::FrozenEgg
        | RelicId::JuzuBracelet
        | RelicId::MealTicket
        | RelicId::MeatOnTheBone
        | RelicId::MoltenEgg
        | RelicId::Omamori
        | RelicId::PotionBelt
        | RelicId::PrayerWheel
        | RelicId::QuestionCard
        | RelicId::RegalPillow
        | RelicId::SingingBowl
        | RelicId::ToxicEgg => context.floor_num <= 48,
        RelicId::Courier | RelicId::MawBank | RelicId::OldCoin | RelicId::SmilingMask => {
            context.floor_num <= 48 && !context.is_current_shop_room()
        }
        RelicId::Ectoplasm => context.act_num <= 1,
        RelicId::Matryoshka | RelicId::WingBoots => context.floor_num <= 40,
        RelicId::PreservedInsect => context.floor_num <= 52,
        RelicId::TinyChest => context.floor_num <= 35,
        RelicId::Girya | RelicId::PeacePipe | RelicId::Shovel => {
            context.floor_num < 48 && context.campfire_relic_count() < 2
        }
        _ => true,
    }
}

pub(crate) fn random_relic_by_tier_from_pools(
    tier: RelicTier,
    pools: &mut RelicPoolsMut<'_>,
    spawn_context: &RelicSpawnContext,
) -> RelicId {
    let id = draw_front_relic_key_from_pools(tier, pools, spawn_context);
    if !is_pool_fallback_relic(id) && !relic_can_spawn_in_context(id, spawn_context) {
        random_relic_end_by_tier_from_pools(tier, pools, spawn_context)
    } else {
        id
    }
}

pub(crate) fn random_relic_end_by_tier_from_pools(
    tier: RelicTier,
    pools: &mut RelicPoolsMut<'_>,
    spawn_context: &RelicSpawnContext,
) -> RelicId {
    let id = match tier {
        RelicTier::Common => pools.common.pop().unwrap_or_else(|| {
            random_relic_by_tier_from_pools(RelicTier::Uncommon, pools, spawn_context)
        }),
        RelicTier::Uncommon => pools.uncommon.pop().unwrap_or_else(|| {
            random_relic_by_tier_from_pools(RelicTier::Rare, pools, spawn_context)
        }),
        RelicTier::Rare => pools.rare.pop().unwrap_or(RelicId::Circlet),
        RelicTier::Shop => pools.shop.pop().unwrap_or_else(|| {
            random_relic_by_tier_from_pools(RelicTier::Uncommon, pools, spawn_context)
        }),
        RelicTier::Boss => {
            if !pools.boss.is_empty() {
                pools.boss.remove(0)
            } else {
                RelicId::RedCirclet
            }
        }
        _ => RelicId::Circlet,
    };
    if !is_pool_fallback_relic(id) && !relic_can_spawn_in_context(id, spawn_context) {
        random_relic_end_by_tier_from_pools(tier, pools, spawn_context)
    } else {
        id
    }
}

fn draw_front_relic_key_from_pools(
    tier: RelicTier,
    pools: &mut RelicPoolsMut<'_>,
    spawn_context: &RelicSpawnContext,
) -> RelicId {
    match tier {
        RelicTier::Common => {
            if !pools.common.is_empty() {
                pools.common.remove(0)
            } else {
                random_relic_by_tier_from_pools(RelicTier::Uncommon, pools, spawn_context)
            }
        }
        RelicTier::Uncommon => {
            if !pools.uncommon.is_empty() {
                pools.uncommon.remove(0)
            } else {
                random_relic_by_tier_from_pools(RelicTier::Rare, pools, spawn_context)
            }
        }
        RelicTier::Rare => {
            if !pools.rare.is_empty() {
                pools.rare.remove(0)
            } else {
                RelicId::Circlet
            }
        }
        RelicTier::Shop => {
            if !pools.shop.is_empty() {
                pools.shop.remove(0)
            } else {
                random_relic_by_tier_from_pools(RelicTier::Uncommon, pools, spawn_context)
            }
        }
        RelicTier::Boss => {
            if !pools.boss.is_empty() {
                pools.boss.remove(0)
            } else {
                RelicId::RedCirclet
            }
        }
        _ => RelicId::Circlet,
    }
}

fn is_pool_fallback_relic(id: RelicId) -> bool {
    matches!(id, RelicId::Circlet | RelicId::RedCirclet)
}
