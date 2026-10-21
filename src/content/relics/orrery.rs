use crate::state::core::EngineState;
use crate::state::run::RunState;

fn push_card_reward(
    run_state: &mut RunState,
    reward_state: &mut crate::rewards::state::RewardState,
) {
    let num_cards = crate::rewards::generator::adjusted_card_reward_choice_count(run_state, 3);
    let cards = crate::rewards::generator::generate_card_reward(run_state, num_cards, false, false);
    if !cards.is_empty() {
        reward_state
            .items
            .push(crate::rewards::state::RewardItem::Card { cards });
    }
}

pub fn on_equip(run_state: &mut RunState, return_state: EngineState) -> Option<EngineState> {
    let (mut reward_state, overlay_return) = match return_state {
        EngineState::RewardScreen(reward_state) => (reward_state, None),
        EngineState::RewardOverlay {
            reward_state,
            return_state,
        } => (reward_state, Some(*return_state)),
        other => (crate::rewards::state::RewardState::new(), Some(other)),
    };

    for _ in 0..4 {
        push_card_reward(run_state, &mut reward_state);
    }

    if !matches!(
        reward_state.screen_context,
        crate::rewards::state::RewardScreenContext::TreasureRoom
    ) {
        push_card_reward(run_state, &mut reward_state);
    }

    Some(match overlay_return {
        Some(return_state) => EngineState::reward_overlay(reward_state, return_state),
        None => EngineState::RewardScreen(reward_state),
    })
}

#[cfg(test)]
mod tests {
    use super::on_equip;
    use crate::content::relics::{RelicId, RelicState};
    use crate::rewards::state::{RewardItem, RewardScreenContext, RewardState};
    use crate::state::core::EngineState;
    use crate::state::run::RunState;

    #[test]
    fn orrery_card_rewards_respect_question_card() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.clear();
        run_state
            .relics
            .push(RelicState::new(RelicId::QuestionCard));

        let next_state = on_equip(&mut run_state, EngineState::MapNavigation)
            .expect("orrery should open reward screen");

        match next_state {
            EngineState::RewardOverlay { reward_state, .. } => {
                assert_eq!(reward_state.items.len(), 5);
                for item in reward_state.items {
                    match item {
                        crate::rewards::state::RewardItem::Card { cards } => {
                            assert_eq!(cards.len(), 4);
                        }
                        other => panic!("expected card reward, got {other:?}"),
                    }
                }
            }
            other => panic!("expected reward overlay, got {other:?}"),
        }
    }

    #[test]
    fn orrery_preserves_existing_reward_items_before_generated_card_rewards() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut existing = RewardState::new();
        existing.items.push(RewardItem::Gold { amount: 25 });
        existing.items.push(RewardItem::Relic {
            relic_id: RelicId::Akabeko,
        });

        let next_state = on_equip(&mut run_state, EngineState::RewardScreen(existing))
            .expect("orrery should open reward screen");

        let EngineState::RewardScreen(reward_state) = next_state else {
            panic!("expected reward screen");
        };
        assert!(matches!(
            reward_state.items[0],
            RewardItem::Gold { amount: 25 }
        ));
        assert!(matches!(
            reward_state.items[1],
            RewardItem::Relic {
                relic_id: RelicId::Akabeko
            }
        ));
        assert_eq!(
            reward_state
                .items
                .iter()
                .filter(|item| matches!(item, RewardItem::Card { .. }))
                .count(),
            5
        );
    }

    #[test]
    fn orrery_from_shop_opens_reward_overlay_that_returns_to_shop() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");

        let next_state = on_equip(
            &mut run_state,
            EngineState::Shop(crate::state::shop::ShopState::new()),
        )
        .expect("orrery should open overlay rewards from shop");

        let EngineState::RewardOverlay {
            reward_state,
            return_state,
        } = next_state
        else {
            panic!("expected reward overlay");
        };
        assert!(matches!(*return_state, EngineState::Shop(_)));
        assert_eq!(
            reward_state
                .items
                .iter()
                .filter(|item| matches!(item, RewardItem::Card { .. }))
                .count(),
            5
        );
    }

    #[test]
    fn orrery_treasure_context_gets_only_four_direct_card_rewards() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let existing = RewardState::with_context(RewardScreenContext::TreasureRoom);

        let next_state = on_equip(&mut run_state, EngineState::RewardScreen(existing))
            .expect("orrery should open reward screen");

        let EngineState::RewardScreen(reward_state) = next_state else {
            panic!("expected reward screen");
        };
        assert_eq!(
            reward_state
                .items
                .iter()
                .filter(|item| matches!(item, RewardItem::Card { .. }))
                .count(),
            4
        );
    }
}
