use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::content::relics::RelicTier;
use serde::{Deserialize, Serialize};

pub(crate) mod generator;

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct RewardCard {
    pub id: crate::content::cards::CardId,
    pub upgrades: u8,
}

impl RewardCard {
    pub fn new(id: crate::content::cards::CardId, upgrades: u8) -> Self {
        Self { id, upgrades }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub enum RewardItem {
    Gold { amount: i32 },
    StolenGold { amount: i32 },
    Card { cards: Vec<RewardCard> },
    Relic { relic_id: RelicId },
    Potion { potion_id: PotionId },
    EmeraldKey,
    SapphireKey,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub enum RewardScreenContext {
    Standard,
    TreasureRoom,
    MuggedCombat,
    SmokedCombat,
    /// Neow opens a standalone card choice without a combat reward parent.
    NeowCardChoice,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct RewardState {
    pub items: Vec<RewardItem>,
    pub skippable: bool,
    /// Mirrors the Java combat reward screen opening mode:
    /// `open()`, `openCombat(TEXT[0])`, or `openCombat(TEXT[1], true)`.
    pub screen_context: RewardScreenContext,
    /// When a Card reward is opened, the offered cards are stored here until
    /// the player picks one or backs out to the reward screen.
    pub pending_card_choice: Option<Vec<RewardCard>>,
    /// Index of the reward item that opened `pending_card_choice`. Java keeps
    /// the reward item on the combat reward screen while the card screen is
    /// open; this lets us remove the right item only after a card is selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_card_reward_index: Option<usize>,
}

impl Default for RewardState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct BossRelicChoiceState {
    pub relics: Vec<RelicId>,
}

impl BossRelicChoiceState {
    pub fn new(relics: Vec<RelicId>) -> Self {
        Self { relics }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub enum TreasureChestSize {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub struct TreasureChestState {
    pub size: TreasureChestSize,
    pub base_relic_tier: RelicTier,
    pub gold_reward_base_amount: Option<i32>,
}

impl RewardState {
    pub fn neow_card_choice(cards: Vec<RewardCard>) -> Self {
        Self {
            items: vec![RewardItem::Card {
                cards: cards.clone(),
            }],
            pending_card_choice: Some(cards),
            pending_card_reward_index: Some(0),
            screen_context: RewardScreenContext::NeowCardChoice,
            ..Self::new()
        }
    }

    pub fn new() -> Self {
        RewardState {
            items: Vec::new(),
            skippable: true,
            screen_context: RewardScreenContext::Standard,
            pending_card_choice: None,
            pending_card_reward_index: None,
        }
    }

    pub fn with_context(screen_context: RewardScreenContext) -> Self {
        RewardState {
            screen_context,
            ..Self::new()
        }
    }

    pub fn has_card_reward_item(&self) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item, RewardItem::Card { .. }))
    }
}
