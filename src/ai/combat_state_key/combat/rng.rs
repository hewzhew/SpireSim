use crate::runtime::rng::{RngPool, StsRng};

use super::super::types::{CombatRngPoolKey, CombatStsRngKey};

pub(crate) fn rng_pool_key(pool: &RngPool) -> CombatRngPoolKey {
    CombatRngPoolKey {
        monster_rng: sts_rng_key(&pool.monster_rng),
        event_rng: sts_rng_key(&pool.event_rng),
        merchant_rng: sts_rng_key(&pool.merchant_rng),
        card_rng: sts_rng_key(&pool.card_rng),
        treasure_rng: sts_rng_key(&pool.treasure_rng),
        relic_rng: sts_rng_key(&pool.relic_rng),
        potion_rng: sts_rng_key(&pool.potion_rng),
        monster_hp_rng: sts_rng_key(&pool.monster_hp_rng),
        ai_rng: sts_rng_key(&pool.ai_rng),
        shuffle_rng: sts_rng_key(&pool.shuffle_rng),
        card_random_rng: sts_rng_key(&pool.card_random_rng),
        misc_rng: sts_rng_key(&pool.misc_rng),
        math_rng: sts_rng_key(&pool.math_rng),
    }
}

fn sts_rng_key(rng: &StsRng) -> CombatStsRngKey {
    CombatStsRngKey {
        seed0: rng.seed0,
        seed1: rng.seed1,
        counter: rng.counter,
    }
}
