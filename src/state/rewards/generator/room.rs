use super::*;

/// Generates post-combat loot transitioning into the RewardState
pub(super) fn generate_combat_rewards(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
) -> RewardState {
    generate_combat_rewards_from_existing(run_state, is_elite, is_boss, Vec::new(), true)
}

pub(super) fn generate_combat_rewards_from_existing(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
    existing_items: Vec<RewardItem>,
    include_card_rewards: bool,
) -> RewardState {
    generate_combat_rewards_from_existing_with_escape_gate(
        run_state,
        is_elite,
        is_boss,
        existing_items,
        include_card_rewards,
        true,
    )
}

pub(super) fn generate_combat_rewards_from_existing_with_escape_gate(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
    existing_items: Vec<RewardItem>,
    include_card_rewards: bool,
    normal_monster_rewards_allowed: bool,
) -> RewardState {
    let mut items = generate_room_rewards_before_screen(
        run_state,
        is_elite,
        is_boss,
        existing_items,
        normal_monster_rewards_allowed,
    );

    if include_card_rewards {
        items.extend(generate_card_reward_items(
            run_state, is_elite, is_boss, true,
        ));
    }

    RewardState {
        items,
        // Combat rewards are individually optional, including after a boss.
        // The player may leave an unclaimed potion behind when every slot is
        // full and continue to the boss relic screen.
        skippable: true,
        screen_context: crate::state::rewards::RewardScreenContext::Standard,
        pending_card_choice: None,
        pending_card_reward_index: None,
    }
}

/// Java room rewards before `CombatRewardScreen.setupItemReward()`.
///
/// Existing rewards are those already inserted into `currRoom.rewards` during
/// combat, such as thief stolen gold. Java then appends/merges normal room
/// gold, elite relic/key rewards, and the potion roll before the reward screen
/// copies the list and optionally appends card rewards.
fn generate_room_rewards_before_screen(
    run_state: &mut RunState,
    is_elite: bool,
    is_boss: bool,
    mut items: Vec<RewardItem>,
    normal_monster_rewards_allowed: bool,
) -> Vec<RewardItem> {
    let prevents_gold_gain = run_state
        .relics
        .iter()
        .any(|relic| crate::content::relics::mechanics::relic_prevents_gold_gain(relic.id));

    // 1. Generate Gold
    if !prevents_gold_gain {
        if is_boss {
            let amount = roll_boss_gold_amount(run_state);
            add_gold_reward_like_java(&mut items, amount);
        } else if !is_elite && !normal_monster_rewards_allowed {
            // Java skips ordinary MonsterRoom gold when every monster escaped.
        } else {
            let amount = if run_state.is_daily_run {
                if is_elite {
                    30
                } else {
                    15
                }
            } else if is_elite {
                run_state.rng_pool.treasure_rng.random_range(25, 35)
            } else {
                run_state.rng_pool.treasure_rng.random_range(10, 20)
            };
            add_gold_reward_like_java(&mut items, amount);
        }
    }

    if is_elite {
        // Java: MonsterRoomElite.dropReward() runs before addPotionToRewards()
        // and before CombatRewardScreen.setupItemReward() appends card rewards.
        let relic_id = run_state.random_relic();
        items.push(RewardItem::Relic { relic_id });

        // Black Star: second relic reward from elites.
        if run_state
            .relics
            .iter()
            .any(|r| r.id == crate::content::relics::RelicId::BlackStar)
        {
            let relic_id2 = run_state.random_noncampfire_relic_reward();
            items.push(RewardItem::Relic {
                relic_id: relic_id2,
            });
        }

        // Java MonsterRoomElite.addEmeraldKey() appends the key immediately
        // after elite relic rewards, before potion and card rewards are added.
        if run_state.is_final_act_available && !run_state.keys[2] {
            if let Some(node) = run_state.map.get_current_node() {
                if node.has_emerald_key {
                    items.push(RewardItem::EmeraldKey);
                }
            }
        }
    }

    // 2. Generate Potions
    add_potion_reward_like_java_with_room_gate(
        run_state,
        &mut items,
        is_elite || is_boss || normal_monster_rewards_allowed,
    );

    items
}

pub(super) fn add_gold_reward_like_java(items: &mut Vec<RewardItem>, amount: i32) {
    for item in items.iter_mut() {
        if let RewardItem::Gold { amount: existing } = item {
            *existing += amount;
            return;
        }
    }
    items.push(RewardItem::Gold { amount });
}

/// Java `AbstractRoom.addPotionToRewards()` for room types whose base potion
/// chance is eligible before relic and reward-size overrides.
pub(super) fn add_potion_reward_like_java(run_state: &mut RunState, items: &mut Vec<RewardItem>) {
    add_potion_reward_like_java_with_room_gate(run_state, items, true);
}

fn add_potion_reward_like_java_with_room_gate(
    run_state: &mut RunState,
    items: &mut Vec<RewardItem>,
    base_chance_allowed: bool,
) {
    let mut chance = if base_chance_allowed {
        40 + run_state.potion_drop_chance_mod
    } else {
        0
    };
    if run_state
        .relics
        .iter()
        .any(|r| r.id == crate::content::relics::RelicId::WhiteBeastStatue)
    {
        chance = crate::content::relics::white_beast_statue::POTION_DROP_CHANCE_OVERRIDE_PERCENT;
    }
    if items.len() >= super::POTION_DROP_BLOCKED_AT_REWARD_ENTRY_COUNT {
        chance = 0;
    }

    let roll = run_state.rng_pool.potion_rng.random_range(0, 99);
    if roll < chance {
        run_state.potion_drop_chance_mod -= 10;
        let potion_class = run_state.potion_class();
        let potion_id = crate::content::potions::random_potion(
            &mut run_state.rng_pool.potion_rng,
            potion_class,
            false,
        );
        items.push(RewardItem::Potion { potion_id });
    } else {
        run_state.potion_drop_chance_mod += 10;
    }
}
