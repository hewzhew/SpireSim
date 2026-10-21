use crate::runtime::combat::Intent;
use crate::state::core::EngineState;
use crate::state::events::EventOptionTransition;
use crate::state::rewards::RewardState;

use super::labels::{boss_label, deck_summary, monster_name};
use super::RunControlSession;

pub(super) fn decision_context(session: &RunControlSession) -> Vec<String> {
    match &session.engine_state {
        EngineState::EventRoom => event_context(session),
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => map_context(session),
        EngineState::RewardScreen(reward) => reward_context(reward, false),
        EngineState::RewardOverlay { reward_state, .. } => reward_context(reward_state, true),
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_) => combat_context(session),
        EngineState::RunPendingChoice(choice) => {
            let request = choice.selection_request(&session.run_state);
            vec![
                format!(
                    "Selection: {:?} | {} | targets={}",
                    request.reason,
                    request.constraint.describe(request.targets.len()),
                    request.targets.len()
                ),
                deck_summary(&session.run_state),
            ]
        }
        EngineState::Shop(shop) => vec![format!(
            "Shop: cards={} relics={} potions={} purge_cost={} purge_available={} pending_rewards={}",
            shop.cards.len(),
            shop.relics.len(),
            shop.potions.len(),
            shop.purge_cost,
            shop.purge_available,
            shop.pending_reward_overlay
                .as_ref()
                .map(|reward| reward.items.len())
                .unwrap_or(0)
        )],
        EngineState::Campfire => vec![deck_summary(&session.run_state)],
        EngineState::TreasureRoom(chest) => vec![format!("Chest: {chest:?}")],
        EngineState::BossRelicSelect(choice) => {
            vec![format!("Relics offered: {}", choice.relics.len())]
        }
        EngineState::CombatStart(request) => vec![format!(
            "CombatStartRequest: encounter={:?} room={:?} context={:?}",
            request.encounter_id, request.room_type, request.context
        )],
        EngineState::GameOver(_) => Vec::new(),
        EngineState::DoubleBossComplete => vec!["First boss defeated; second boss has not started.".to_owned()],
        EngineState::Act3VictoryComplete => vec!["Act 3 boss defeated; awaiting Proceed.".to_owned()],
        EngineState::TrueVictoryComplete => vec!["Heart defeated; awaiting Proceed.".to_owned()],
    }
}

pub(super) fn decision_warnings(session: &RunControlSession) -> Vec<String> {
    match &session.engine_state {
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => vec![
            "Map context is local next-node visibility, not a route policy evaluation.".to_string(),
        ],
        EngineState::EventRoom => {
            let options = crate::engine::event_handler::get_event_options(&session.run_state);
            if options.iter().any(|option| {
                matches!(
                    option.semantics.transition,
                    EventOptionTransition::OpenSelection(_) | EventOptionTransition::OpenReward
                )
            }) {
                vec![
                    "Some event options open follow-up choices; this panel does not rank them."
                        .to_string(),
                ]
            } else {
                Vec::new()
            }
        }
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_) => Vec::new(),
        _ => Vec::new(),
    }
}

fn event_context(session: &RunControlSession) -> Vec<String> {
    let Some(event) = session.run_state.event_state.as_ref() else {
        return vec!["Event state missing".to_string()];
    };
    let mut context = vec![format!(
        "Event: {:?} | screen={} | completed={}",
        event.id, event.current_screen, event.completed
    )];
    if event.id == crate::state::events::EventId::Neow && event.current_screen == 0 {
        context.push("This screen only advances to Neow reward choices.".to_string());
    }
    context.push(deck_summary(&session.run_state));
    context
}

fn map_context(session: &RunControlSession) -> Vec<String> {
    let map = &session.run_state.map;
    vec![
        format!(
            "Map: current=({}, {}) | next_y={} | boss_available={}",
            map.current_x,
            map.current_y,
            if map.current_y == -1 {
                0
            } else {
                map.current_y + 1
            },
            map.boss_node_available_now()
        ),
        format!("Boss: {}", boss_label(&session.run_state)),
    ]
}

fn reward_context(reward: &RewardState, overlay: bool) -> Vec<String> {
    if let Some(cards) = reward.pending_card_choice.as_ref() {
        let destination = if overlay {
            "overlay reward screen"
        } else {
            "reward screen"
        };
        return vec![format!(
            "Reward cards: {} options | back returns to {destination} without consuming the card reward",
            cards.len()
        )];
    }
    let mut context = vec![format!(
        "Rewards: items={} | skippable={} | context={:?}",
        reward.items.len(),
        reward.skippable,
        reward.screen_context
    )];
    if overlay {
        context.push("Overlay reward: claim rewards before closing the overlay.".to_string());
    }
    context
}

fn combat_context(session: &RunControlSession) -> Vec<String> {
    let Some(combat) = session
        .active_combat
        .as_ref()
        .map(|active| &active.combat_state)
    else {
        return vec!["Combat state missing".to_string()];
    };
    let mut context = vec![format!(
        "Player: HP {}/{} | Block {} | Energy {}",
        combat.entities.player.current_hp,
        combat.entities.player.max_hp,
        combat.entities.player.block,
        combat.turn.energy
    )];
    context.push(format!(
        "Piles: hand={} draw={} discard={} exhaust={} limbo={}",
        combat.zones.hand.len(),
        combat.zones.draw_pile.len(),
        combat.zones.discard_pile.len(),
        combat.zones.exhaust_pile.len(),
        combat.zones.limbo.len()
    ));
    for monster in &combat.entities.monsters {
        let observation = combat
            .runtime
            .monster_protocol
            .get(&monster.id)
            .map(|protocol| &protocol.observation);
        let turn_plan = monster.turn_plan();
        let intent = observation
            .filter(|obs| obs.visible_intent != Intent::Unknown)
            .map(|obs| format!("{:?}", obs.visible_intent))
            .unwrap_or_else(|| format!("{:?}", turn_plan.summary_spec()));
        let damage = observation
            .filter(|obs| obs.preview_damage_per_hit > 0)
            .map(|obs| obs.preview_damage_per_hit)
            .or_else(|| turn_plan.attack().map(|attack| attack.base_damage))
            .unwrap_or(0);
        context.push(format!(
            "Enemy slot {}: {} hp {}/{} block {} intent={} damage={} alive={}",
            monster.slot,
            monster_name(monster.enemy_id),
            monster.current_hp,
            monster.max_hp,
            monster.block,
            intent,
            damage,
            monster.is_alive_for_action()
        ));
    }
    let hand = combat
        .zones
        .hand
        .iter()
        .enumerate()
        .map(|(idx, card)| format!("{idx}:{}", super::labels::combat_card_label(card)))
        .collect::<Vec<_>>()
        .join(", ");
    context.push(format!("Hand: {hand}"));
    context
}
