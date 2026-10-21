use super::session::RunControlSession;
use crate::state::core::EngineState;

pub(super) fn run_control_next_hint(session: &RunControlSession) -> &'static str {
    match &session.engine_state {
        EngineState::EventRoom => {
            let is_neow_bonus = session.run_state.event_state.as_ref().is_some_and(|event| {
                event.id == crate::state::events::EventId::Neow && event.current_screen > 0
            });
            if is_neow_bonus {
                "Next: choose a Neow bonus id, or inspect deck/map/relics first."
            } else {
                "Next: choose a visible event option id; use inspect/details/raw if the semantics look wrong."
            }
        }
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => {
            "Next: use rs to inspect route evidence, rg to accept the exact route policy, or type a visible path id."
        }
        EngineState::RewardScreen(reward) if reward.pending_card_choice.is_some() => {
            "Next: choose a card id or skip; use deck/map/relics before choosing if needed."
        }
        EngineState::RewardOverlay { reward_state, .. }
            if reward_state.pending_card_choice.is_some() =>
        {
            "Next: choose a card id or skip; use deck/map/relics before choosing if needed."
        }
        EngineState::RewardScreen(reward) | EngineState::RewardOverlay { reward_state: reward, .. }
            if reward.has_card_reward_item() =>
        {
            "Next: open the card reward id, then choose a card or skip; use deck/map/relics first if needed."
        }
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. } => {
            "Next: choose a visible reward id, or skip to preview the map while unclaimed rewards remain."
        }
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_) => {
            "Next: play manually, cap the combat if useful, or try sc max_nodes=N wall_ms=N."
        }
        EngineState::BossRelicSelect(_) => {
            "Next: choose a visible boss relic id; inspect deck/relics first if needed."
        }
        EngineState::Shop(shop) if shop.pending_reward_overlay.is_some() => {
            "Next: type rewards to reopen pending shop rewards, or finish shopping before leaving."
        }
        EngineState::Shop(_) => "Next: buy card/relic/potion, purge a card, or leave the shop.",
        EngineState::Campfire => {
            "Next: rest, smith a deck index, or use another visible campfire option."
        }
        EngineState::TreasureRoom(_) => "Next: open the chest or leave it unopened.",
        EngineState::RunPendingChoice(_) => "Next: choose a visible run-choice id.",
        EngineState::CombatStart(_) => {
            "Next: advance once; combat setup should settle into a player turn."
        }
        EngineState::GameOver(_) => "Next: q to exit, or start a new run from the shell.",
        EngineState::DoubleBossComplete => "Next: proceed to the second boss.",
        EngineState::Act3VictoryComplete => "Next: proceed to the Spire Heart.",
        EngineState::TrueVictoryComplete => "Next: proceed to the victory screen.",
    }
}
