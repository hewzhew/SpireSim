use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct StableShopKey {
    pub purge_cost: i32,
    pub purge_available: bool,
    pub cards: Vec<StableShopRowKey>,
    pub relics: Vec<StableShopRowKey>,
    pub potions: Vec<StableShopRowKey>,
    pub pending_reward_overlay: Option<StableRewardKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct StableShopRowKey {
    pub id: String,
    pub price: i32,
    pub can_buy: bool,
    pub blocked_reason: Option<String>,
}

impl StableShopKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "purge{}:{}:cards{}:relics{}:potions{}:overlay{}",
            self.purge_cost,
            self.purge_available,
            join_diagnostic_strings(&self.cards),
            join_diagnostic_strings(&self.relics),
            join_diagnostic_strings(&self.potions),
            self.pending_reward_overlay
                .as_ref()
                .map(StableRewardKey::diagnostic_string)
                .unwrap_or_else(|| "_".to_string()),
        )
    }
}

impl StableShopRowKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.id,
            self.price,
            self.can_buy,
            self.blocked_reason.as_deref().unwrap_or("_"),
        )
    }
}
