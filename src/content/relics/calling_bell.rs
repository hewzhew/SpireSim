use crate::content::cards::CardId;
use crate::content::relics::RelicTier;
use crate::state::core::EngineState;
use crate::state::run::RunState;

pub fn on_equip(run_state: &mut RunState, _return_state: EngineState) -> Option<EngineState> {
    // Add Curse of the Bell directly to deck
    run_state.add_card_to_deck_with_upgrades_from(
        CardId::CurseOfTheBell,
        0,
        crate::state::selection::DomainEventSource::Relic(
            crate::content::relics::RelicId::CallingBell,
        ),
    );

    let mut rs = crate::rewards::state::RewardState::new();
    rs.items.push(crate::rewards::state::RewardItem::Relic {
        relic_id: run_state.random_screenless_relic(RelicTier::Common),
    });
    rs.items.push(crate::rewards::state::RewardItem::Relic {
        relic_id: run_state.random_screenless_relic(RelicTier::Uncommon),
    });
    rs.items.push(crate::rewards::state::RewardItem::Relic {
        relic_id: run_state.random_screenless_relic(RelicTier::Rare),
    });

    Some(EngineState::RewardScreen(rs))
}
