#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatRngPoolKey {
    pub(crate) monster_rng: CombatStsRngKey,
    pub(crate) event_rng: CombatStsRngKey,
    pub(crate) merchant_rng: CombatStsRngKey,
    pub(crate) card_rng: CombatStsRngKey,
    pub(crate) treasure_rng: CombatStsRngKey,
    pub(crate) relic_rng: CombatStsRngKey,
    pub(crate) potion_rng: CombatStsRngKey,
    pub(crate) monster_hp_rng: CombatStsRngKey,
    pub(crate) ai_rng: CombatStsRngKey,
    pub(crate) shuffle_rng: CombatStsRngKey,
    pub(crate) card_random_rng: CombatStsRngKey,
    pub(crate) misc_rng: CombatStsRngKey,
    pub(crate) math_rng: CombatStsRngKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatStsRngKey {
    pub(crate) seed0: u64,
    pub(crate) seed1: u64,
    pub(crate) counter: u32,
}
