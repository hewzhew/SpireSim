use crate::state::core::EngineState;
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn on_equip(run_state: &mut RunState) -> Option<EngineState> {
    use crate::content::cards::can_upgrade_card_once;
    use crate::content::relics::RelicId;
    use crate::rewards::state::{RewardItem, RewardState};

    let mut upgradable: Vec<usize> = run_state
        .master_deck
        .iter()
        .enumerate()
        .filter(|(_, c)| can_upgrade_card_once(c))
        .map(|(i, _)| i)
        .collect();
    if !upgradable.is_empty() {
        crate::runtime::rng::shuffle_with_random_long(
            &mut upgradable,
            &mut run_state.rng_pool.misc_rng,
        );
        let lineage = run_state.master_deck[upgradable[0]].lineage;
        run_state.upgrade_card_with_source(lineage, DomainEventSource::Relic(RelicId::TinyHouse));
    }

    run_state.gain_max_hp_with_source(5, 5, DomainEventSource::Relic(RelicId::TinyHouse));

    let potion_class = run_state.potion_class();
    let potion_id =
        crate::content::potions::random_potion_any(&mut run_state.rng_pool.misc_rng, potion_class);
    let num_cards = crate::rewards::generator::adjusted_card_reward_choice_count(run_state, 3);
    let cards = crate::rewards::generator::generate_card_reward(run_state, num_cards, false, false);

    let mut reward_state = RewardState::new();
    reward_state.items.push(RewardItem::Gold { amount: 50 });
    reward_state.items.push(RewardItem::Potion { potion_id });
    reward_state.items.push(RewardItem::Card { cards });
    Some(EngineState::RewardScreen(reward_state))
}
