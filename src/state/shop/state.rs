use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::state::rewards::RewardState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ShopConfig {
    pub ascension_level: i32,
    pub player_class: &'static str,
    pub has_courier: bool,
    pub has_membership_card: bool,
    pub has_smiling_mask: bool,
    pub has_molten_egg: bool,
    pub has_toxic_egg: bool,
    pub has_frozen_egg: bool,
    pub previous_purge_count: i32,
    pub potion_class: crate::content::potions::PotionClass,
    pub card_blizz_randomizer: i32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ShopCard {
    pub card_id: CardId,
    pub upgrades: u8,
    pub price: i32,
    pub can_buy: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ShopRelic {
    pub relic_id: RelicId,
    pub price: i32,
    pub can_buy: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ShopPotion {
    pub potion_id: PotionId,
    pub price: i32,
    pub can_buy: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ShopState {
    pub cards: Vec<ShopCard>,
    pub relics: Vec<ShopRelic>,
    pub potions: Vec<ShopPotion>,
    pub purge_cost: i32,
    pub purge_available: bool,
    /// Java's CombatRewardScreen can be opened over the shop by shop relics
    /// such as Orrery. Closing that screen returns to the shop without
    /// abandoning unclaimed reward items; the player can reopen the reward
    /// overlay before leaving the shop room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_reward_overlay: Option<RewardState>,
}

impl ShopState {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for ShopState {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            relics: Vec::new(),
            potions: Vec::new(),
            purge_cost: 75,
            purge_available: true,
            pending_reward_overlay: None,
        }
    }
}

pub fn membership_card_discounted_price(price: i32) -> i32 {
    (price as f32 * 0.5).round() as i32
}
