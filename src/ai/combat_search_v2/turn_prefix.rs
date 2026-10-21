use super::turn_branching::TurnBranchTransition;
use super::*;

mod diagnostics;
mod state;

pub(super) use diagnostics::TurnPrefixDiagnosticsCollector;
pub(super) use state::{TurnPrefixKind, TurnPrefixObservation, TurnPrefixState, TurnPrefixSummary};

pub(super) fn summarize_turn_prefix(
    prefix: &TurnPrefixState,
    legal_actions: usize,
) -> TurnPrefixSummary {
    TurnPrefixSummary {
        prefix: prefix.clone(),
        legal_actions,
    }
}

pub(super) fn advance_turn_prefix(
    current: &TurnPrefixState,
    parent_combat: &CombatState,
    input: &ClientInput,
    transition: TurnBranchTransition,
) -> TurnPrefixState {
    if transition.resets_turn_prefix() {
        return TurnPrefixState::default();
    }

    let Some(token) = prefix_token(parent_combat, input) else {
        return current.clone();
    };

    let mut next = current.clone();
    if next.prefix_length == 0 {
        next.origin_key = Some(turn_origin_key(parent_combat));
    }
    next.prefix_length = next.prefix_length.saturating_add(1);
    match input {
        ClientInput::PlayCard { .. } => next.cards_played = next.cards_played.saturating_add(1),
        ClientInput::UsePotion { .. } => next.potions_used = next.potions_used.saturating_add(1),
        ClientInput::DiscardPotion(_) => {
            next.potions_discarded = next.potions_discarded.saturating_add(1)
        }
        _ => next.other_actions = next.other_actions.saturating_add(1),
    }
    next.push_token(&token);
    next
}

fn prefix_token(parent_combat: &CombatState, input: &ClientInput) -> Option<String> {
    match input {
        ClientInput::PlayCard { card_index, .. } => parent_combat
            .zones
            .hand
            .get(*card_index)
            .map(|card| format!("card:{}#{}", card.id.stable_id(), card.lineage)),
        ClientInput::UsePotion { potion_index, .. } => Some(format!("potion:{potion_index}")),
        ClientInput::DiscardPotion(slot) => Some(format!("discard_potion:{slot}")),
        ClientInput::EndTurn => Some("end_turn".to_string()),
        _ => Some("other".to_string()),
    }
}

fn turn_origin_key(parent_combat: &CombatState) -> String {
    stable_debug_hash(&combat_dominance_key(
        &EngineState::CombatPlayerTurn,
        parent_combat,
    ))
}

fn stable_debug_hash<T: std::fmt::Debug>(value: &T) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in format!("{value:?}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests;
