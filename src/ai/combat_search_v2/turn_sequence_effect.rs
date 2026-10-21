use super::*;

// Keep diagnostic decomposition here in the search layer while the opaque
// dominance-key representation remains owned by the simulator core.
#[derive(Clone, Debug)]
pub(super) struct TurnSequenceEffectFingerprint {
    terminal_key: String,
    legal_action_count_key: String,
    public_state_key: String,
    hand_public_order_key: String,
    hand_identity_order_key: String,
    draw_public_order_key: String,
    draw_identity_order_key: String,
    discard_public_order_key: String,
    discard_identity_order_key: String,
    exhaust_public_order_key: String,
    exhaust_identity_order_key: String,
    limbo_public_order_key: String,
    limbo_identity_order_key: String,
    pending_queue_key: String,
    rng_key: String,
    dominance_key: String,
    dominance_engine_key: String,
    dominance_turn_key: String,
    turn_draw_modifier_key: String,
    turn_action_counter_key: String,
    turn_played_card_history_key: String,
    turn_discard_counter_key: String,
    turn_orb_history_key: String,
    turn_combat_flag_key: String,
    dominance_meta_key: String,
    dominance_zones_key: String,
    dominance_monsters_key: String,
    dominance_powers_key: String,
    dominance_potions_key: String,
    dominance_queue_key: String,
    dominance_runtime_key: String,
    dominance_rng_key: String,
    dominance_player_key: String,
    resource_public_key: String,
    resource_cost_key: String,
}

mod aggregate;
mod classification;
mod divergence;
mod projection;

pub(super) use aggregate::TurnSequenceEffectAggregate;
pub(super) use divergence::TurnSequenceDivergence;
use projection::{
    card_identity_order_key, card_public_order_key, public_state_projection, stable_debug_hash,
};

pub(super) fn effect_fingerprint(
    node: &SearchNode,
    legal_actions: usize,
) -> TurnSequenceEffectFingerprint {
    let resource = node.resource_vector().diagnostic_parts();
    let dominance = combat_dominance_key(&node.engine, &node.combat);
    let dominance_parts = combat_dominance_diagnostic_parts_v1(&dominance);
    TurnSequenceEffectFingerprint {
        terminal_key: format!("{:?}", terminal_label(&node.engine, &node.combat)),
        legal_action_count_key: legal_actions.to_string(),
        public_state_key: stable_debug_hash(&public_state_projection(&node.engine, &node.combat)),
        hand_public_order_key: card_public_order_key(&node.combat.zones.hand),
        hand_identity_order_key: card_identity_order_key(&node.combat.zones.hand),
        draw_public_order_key: card_public_order_key(&node.combat.zones.draw_pile),
        draw_identity_order_key: card_identity_order_key(&node.combat.zones.draw_pile),
        discard_public_order_key: card_public_order_key(&node.combat.zones.discard_pile),
        discard_identity_order_key: card_identity_order_key(&node.combat.zones.discard_pile),
        exhaust_public_order_key: card_public_order_key(&node.combat.zones.exhaust_pile),
        exhaust_identity_order_key: card_identity_order_key(&node.combat.zones.exhaust_pile),
        limbo_public_order_key: card_public_order_key(&node.combat.zones.limbo),
        limbo_identity_order_key: card_identity_order_key(&node.combat.zones.limbo),
        pending_queue_key: stable_debug_hash(&(
            &node.combat.engine.action_queue,
            &node.combat.zones.queued_cards,
        )),
        rng_key: stable_debug_hash(&node.combat.rng),
        dominance_key: stable_debug_hash(&dominance),
        dominance_engine_key: dominance_parts.engine_key,
        dominance_turn_key: dominance_parts.turn_key,
        turn_draw_modifier_key: stable_debug_hash(&node.combat.turn.turn_start_draw_modifier),
        turn_action_counter_key: stable_debug_hash(&(
            node.combat.turn.counters.cards_played_this_turn,
            node.combat.turn.counters.attacks_played_this_turn,
        )),
        turn_played_card_history_key: stable_debug_hash(&(
            &node.combat.turn.counters.card_ids_played_this_turn,
            &node.combat.turn.counters.card_ids_played_this_combat,
        )),
        turn_discard_counter_key: stable_debug_hash(
            &node.combat.turn.counters.cards_discarded_this_turn,
        ),
        turn_orb_history_key: stable_debug_hash(&(
            &node.combat.turn.counters.orbs_channeled_this_turn,
            &node.combat.turn.counters.orbs_channeled_this_combat,
            node.combat.turn.counters.mantra_gained_this_combat,
        )),
        turn_combat_flag_key: stable_debug_hash(&(
            node.combat.turn.counters.times_damaged_this_combat,
            node.combat.turn.counters.victory_triggered,
            node.combat.turn.counters.discovery_cost_for_turn,
            node.combat.turn.counters.early_end_turn_pending,
            node.combat.turn.counters.skip_monster_turn_pending,
            node.combat.turn.counters.player_escaping,
            node.combat.turn.counters.escape_pending_reward,
        )),
        dominance_meta_key: dominance_parts.meta_key,
        dominance_zones_key: dominance_parts.zones_key,
        dominance_monsters_key: dominance_parts.monsters_key,
        dominance_powers_key: dominance_parts.powers_key,
        dominance_potions_key: dominance_parts.potions_key,
        dominance_queue_key: dominance_parts.queue_key,
        dominance_runtime_key: dominance_parts.runtime_key,
        dominance_rng_key: dominance_parts.rng_key,
        dominance_player_key: dominance_parts.player_key,
        resource_public_key: stable_debug_hash(&(resource.hp, resource.block)),
        resource_cost_key: stable_debug_hash(&(
            resource.potions_used,
            resource.potions_discarded,
            resource.cards_played,
            resource.action_count,
        )),
    }
}

pub(super) fn effect_key(fingerprint: &TurnSequenceEffectFingerprint) -> String {
    stable_debug_hash(&[
        fingerprint.terminal_key.as_str(),
        fingerprint.legal_action_count_key.as_str(),
        fingerprint.public_state_key.as_str(),
        fingerprint.hand_public_order_key.as_str(),
        fingerprint.hand_identity_order_key.as_str(),
        fingerprint.draw_public_order_key.as_str(),
        fingerprint.draw_identity_order_key.as_str(),
        fingerprint.discard_public_order_key.as_str(),
        fingerprint.discard_identity_order_key.as_str(),
        fingerprint.exhaust_public_order_key.as_str(),
        fingerprint.exhaust_identity_order_key.as_str(),
        fingerprint.limbo_public_order_key.as_str(),
        fingerprint.limbo_identity_order_key.as_str(),
        fingerprint.pending_queue_key.as_str(),
        fingerprint.rng_key.as_str(),
        fingerprint.dominance_key.as_str(),
        fingerprint.dominance_engine_key.as_str(),
        fingerprint.dominance_turn_key.as_str(),
        fingerprint.turn_draw_modifier_key.as_str(),
        fingerprint.turn_action_counter_key.as_str(),
        fingerprint.turn_played_card_history_key.as_str(),
        fingerprint.turn_discard_counter_key.as_str(),
        fingerprint.turn_orb_history_key.as_str(),
        fingerprint.turn_combat_flag_key.as_str(),
        fingerprint.dominance_meta_key.as_str(),
        fingerprint.dominance_zones_key.as_str(),
        fingerprint.dominance_monsters_key.as_str(),
        fingerprint.dominance_powers_key.as_str(),
        fingerprint.dominance_potions_key.as_str(),
        fingerprint.dominance_queue_key.as_str(),
        fingerprint.dominance_runtime_key.as_str(),
        fingerprint.dominance_rng_key.as_str(),
        fingerprint.dominance_player_key.as_str(),
        fingerprint.resource_public_key.as_str(),
        fingerprint.resource_cost_key.as_str(),
    ])
}

#[cfg(test)]
mod tests;
