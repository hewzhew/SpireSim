use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct StableRewardKey {
    pub screen_context: String,
    pub skippable: bool,
    pub items: Vec<StableRewardItemKey>,
    pub pending_card_choice: Vec<StableRewardCardKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) enum StableRewardItemKey {
    Gold(i32),
    StolenGold(i32),
    Card(Vec<StableRewardCardKey>),
    Relic(String),
    Potion(String),
    EmeraldKey,
    SapphireKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct StableRewardCardKey {
    pub id: String,
    pub upgrades: u8,
}

impl StableRewardKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "ctx{}:skip{}:items{}:pending{}",
            self.screen_context,
            self.skippable,
            join_diagnostic_strings(&self.items),
            if self.pending_card_choice.is_empty() {
                "_".to_string()
            } else {
                join_diagnostic_strings(&self.pending_card_choice)
            },
        )
    }
}

impl StableRewardItemKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        match self {
            StableRewardItemKey::Gold(amount) => format!("gold:{amount}"),
            StableRewardItemKey::StolenGold(amount) => format!("stolen_gold:{amount}"),
            StableRewardItemKey::Card(cards) => {
                format!("card:{}", join_diagnostic_strings(cards))
            }
            StableRewardItemKey::Relic(id) => format!("relic:{id}"),
            StableRewardItemKey::Potion(id) => format!("potion:{id}"),
            StableRewardItemKey::EmeraldKey => "emerald_key".to_string(),
            StableRewardItemKey::SapphireKey => "sapphire_key".to_string(),
        }
    }
}

impl StableRewardCardKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!("{}:u{}", self.id, self.upgrades)
    }
}
