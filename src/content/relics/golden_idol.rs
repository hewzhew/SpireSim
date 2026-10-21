// No dependencies

/// Golden Idol: Enemies drop 25% more Gold.
/// Java stores base gold on the RewardItem and a separate rounded bonusGold;
/// the player receives goldAmt + bonusGold when the reward is claimed.
pub fn reward_gold_bonus(amount: i32) -> i32 {
    (amount as f32 * 0.25).round() as i32
}

pub fn apply_reward_gold_bonus(amount: i32) -> i32 {
    amount + reward_gold_bonus(amount)
}
