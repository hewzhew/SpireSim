use crate::rewards::state::{RewardItem, RewardScreenContext, RewardState};
use crate::state::core::EngineState;
use crate::state::run::RunState;

pub fn on_equip(run_state: &mut RunState, return_state: EngineState) -> Option<EngineState> {
    let (mut reward_state, overlay_return) = match return_state {
        EngineState::RewardScreen(reward_state) => (reward_state, None),
        EngineState::RewardOverlay {
            reward_state,
            return_state,
        } => (reward_state, Some(*return_state)),
        other => (RewardState::new(), Some(other)),
    };

    let potion_class = run_state.potion_class();
    for _ in 0..5 {
        let potion_id = crate::content::potions::random_potion_any(
            &mut run_state.rng_pool.potion_rng,
            potion_class,
        );
        reward_state.items.push(RewardItem::Potion { potion_id });
    }

    if !matches!(
        reward_state.screen_context,
        RewardScreenContext::TreasureRoom
    ) {
        let num_cards = crate::rewards::generator::adjusted_card_reward_choice_count(run_state, 3);
        let cards =
            crate::rewards::generator::generate_card_reward(run_state, num_cards, false, false);
        if !cards.is_empty() {
            reward_state.items.push(RewardItem::Card { cards });
        }
    }

    if let Some(index) = reward_state
        .items
        .iter()
        .position(|item| matches!(item, RewardItem::Card { .. }))
    {
        reward_state.items.remove(index);
    }

    Some(match overlay_return {
        Some(return_state) => EngineState::reward_overlay(reward_state, return_state),
        None => EngineState::RewardScreen(reward_state),
    })
}
