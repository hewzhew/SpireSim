use crate::content::cards::{CardId, CardRarity};
use crate::state::rewards::{RewardCard, RewardItem, RewardState};
use crate::state::run::RunState;

mod card;
mod room;

/// Java addPotionToRewards blocks generation at this many existing entries,
/// after applying the White Beast Statue chance override.
pub(crate) const POTION_DROP_BLOCKED_AT_REWARD_ENTRY_COUNT: usize = 4;

/// Base amount rolled by AbstractRoom before deciding whether to open rewards.
/// TheBeyond and TheEnding bosses also roll it, but never offer it to the player.
pub(crate) fn roll_boss_gold_amount(run_state: &mut RunState) -> i32 {
    if run_state.is_daily_run {
        return 100;
    }
    let amount = 100 + run_state.rng_pool.misc_rng.random_range(-5, 5);
    if run_state.ascension_level >= 13 {
        (amount as f32 * 0.75).round() as i32
    } else {
        amount
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
use card::{reward_card_candidate_pool_for_run, select_reward_card_candidate};

pub fn adjusted_card_reward_choice_count(run_state: &RunState, base_count: usize) -> usize {
    card::adjusted_card_reward_choice_count(run_state, base_count)
}

pub fn generate_combat_rewards(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
) -> RewardState {
    room::generate_combat_rewards(run_state, is_elite, is_boss)
}

#[cfg(test)]
fn generate_combat_rewards_from_existing(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
    existing_items: Vec<RewardItem>,
    include_card_rewards: bool,
) -> RewardState {
    room::generate_combat_rewards_from_existing(
        run_state,
        is_elite,
        is_boss,
        existing_items,
        include_card_rewards,
    )
}

pub fn generate_combat_rewards_from_existing_with_escape_gate(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
    existing_items: Vec<RewardItem>,
    include_card_rewards: bool,
    normal_monster_rewards_allowed: bool,
) -> RewardState {
    room::generate_combat_rewards_from_existing_with_escape_gate(
        run_state,
        is_elite,
        is_boss,
        existing_items,
        include_card_rewards,
        normal_monster_rewards_allowed,
    )
}

pub fn add_gold_reward_like_java(items: &mut Vec<RewardItem>, amount: i32) {
    room::add_gold_reward_like_java(items, amount);
}

pub fn add_potion_reward_like_java(run_state: &mut RunState, items: &mut Vec<RewardItem>) {
    room::add_potion_reward_like_java(run_state, items);
}

pub fn generate_card_reward_items(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
    prayer_wheel_allowed: bool,
) -> Vec<RewardItem> {
    card::generate_card_reward_items(run_state, is_elite, is_boss, prayer_wheel_allowed)
}

pub fn generate_card_reward(
    run_state: &mut RunState,
    num_cards: usize,
    is_elite: bool,
    is_boss: bool,
) -> Vec<RewardCard> {
    card::generate_card_reward(run_state, num_cards, is_elite, is_boss)
}
