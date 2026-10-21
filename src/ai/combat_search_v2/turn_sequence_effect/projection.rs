use super::super::*;
use crate::runtime::combat::{CombatCard, Power, PowerPayload};
pub(super) fn public_state_projection(
    engine: &EngineState,
    combat: &CombatState,
) -> impl std::fmt::Debug {
    let monsters = combat
        .entities
        .monsters
        .iter()
        .map(|monster| {
            (
                monster.slot,
                monster.enemy_id,
                monster.current_hp,
                monster.max_hp,
                monster.block,
                monster.is_dying,
                monster.is_escaped,
                monster.half_dead,
                combat.monster_protocol_visible_intent(monster.id).clone(),
                combat.monster_protocol_preview_damage_per_hit(monster.id),
                power_public_key(combat.entities.power_db.get(&monster.id)),
            )
        })
        .collect::<Vec<_>>();
    let player_power_key =
        power_public_key(combat.entities.power_db.get(&combat.entities.player.id));
    (
        engine_label(engine),
        combat.turn.turn_count,
        combat.turn.current_phase,
        combat.turn.energy,
        combat.entities.player.current_hp,
        combat.entities.player.max_hp,
        combat.entities.player.block,
        combat.entities.player.stance,
        player_power_key,
        monsters,
    )
}

fn engine_label(engine: &EngineState) -> &'static str {
    match engine {
        EngineState::CombatStart(_) => "combat_start",
        EngineState::CombatPlayerTurn => "combat_player_turn",
        EngineState::CombatProcessing => "combat_processing",
        EngineState::PendingChoice(_) => "pending_choice",
        EngineState::RewardScreen(_) => "reward_screen",
        EngineState::RewardOverlay { .. } => "reward_overlay",
        EngineState::TreasureRoom(_) => "treasure_room",
        EngineState::Campfire => "campfire",
        EngineState::Shop(_) => "shop",
        EngineState::MapNavigation => "map_navigation",
        EngineState::MapOverlay { .. } => "map_overlay",
        EngineState::EventRoom => "event_room",
        EngineState::RunPendingChoice(_) => "run_pending_choice",
        EngineState::BossRelicSelect(_) => "boss_relic_select",
        EngineState::GameOver(_) => "game_over",
        EngineState::DoubleBossComplete => "double_boss_complete",
        EngineState::Act3VictoryComplete => "act3_victory_complete",
        EngineState::TrueVictoryComplete => "true_victory_complete",
    }
}

pub(super) fn card_public_order_key<'a>(cards: impl IntoIterator<Item = &'a CombatCard>) -> String {
    stable_debug_hash(
        &cards
            .into_iter()
            .map(card_public_signature)
            .collect::<Vec<_>>(),
    )
}

pub(super) fn card_identity_order_key<'a>(
    cards: impl IntoIterator<Item = &'a CombatCard>,
) -> String {
    stable_debug_hash(
        &cards
            .into_iter()
            .map(|card| (card.lineage, card_public_signature(card)))
            .collect::<Vec<_>>(),
    )
}

fn card_public_signature(card: &CombatCard) -> impl std::fmt::Debug {
    (
        card.id,
        card.upgrades,
        card.misc_value,
        card.base_damage_override,
        card.base_block_override,
        card.cost_modifier,
        card.cost_for_turn,
        card.free_to_play_once,
    )
}

fn power_public_key(powers: Option<&Vec<Power>>) -> String {
    stable_debug_hash(
        &powers
            .into_iter()
            .flatten()
            .map(|power| {
                (
                    power.power_type,
                    power.amount,
                    power.extra_data,
                    matches!(power.payload, PowerPayload::Card(_)),
                    match power.payload {
                        PowerPayload::SourceEntity(source) => Some(source),
                        _ => None,
                    },
                    power.just_applied,
                )
            })
            .collect::<Vec<_>>(),
    )
}

pub(super) fn stable_debug_hash<T: std::fmt::Debug>(value: &T) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in format!("{value:?}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
