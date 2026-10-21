use crate::content::potions::PotionId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatPotionSlotKey {
    pub(crate) slot: usize,
    pub(crate) potion: Option<CombatPotionKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatPotionKey {
    pub(crate) id: PotionId,
    pub(crate) uuid: u32,
    pub(crate) can_use: bool,
    pub(crate) can_discard: bool,
    pub(crate) requires_target: bool,
}
