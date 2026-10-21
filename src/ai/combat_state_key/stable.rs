use crate::runtime::combat::{
    CardZones, CombatCard, CombatState, PlayerEntity, Power, TurnRuntime,
};
use crate::state::EngineState;

use super::combat::{phase_key, rng_pool_key};
use super::monster::stable_monster_signature;
use super::pending_choice::pending_choice_key;
use super::postcombat::{
    stable_boss_relic_key, stable_meta_key, stable_postcombat_player_key,
    stable_postcombat_runtime_key, stable_reward_key, stable_run_pending_choice_key,
    stable_run_pending_return_key, stable_run_result_signature, stable_shop_key,
    stable_treasure_chest_key,
};
use super::types::{
    StableCombatPlayerKey, StableEngineKey, StableOutcomeKey, StableOutcomePayload, StableTurnKey,
    StableZonesKey,
};
use super::{stable_frontier_scope, StableFrontierScope};

pub(super) fn build_stable_outcome_key(
    engine: &EngineState,
    combat: &CombatState,
) -> StableOutcomeKey {
    let scope = stable_frontier_scope(engine, combat);
    match scope {
        StableFrontierScope::CombatReady
        | StableFrontierScope::PendingChoice
        | StableFrontierScope::Unstable => StableOutcomeKey::new(
            scope,
            stable_combat_engine_key(engine, combat),
            StableOutcomePayload::Combat {
                meta: stable_meta_key(&combat.meta),
                turn: stable_turn_key(&combat.turn),
                player: stable_player_key(&combat.entities.player),
                zones: stable_zones_key(&combat.zones),
                monsters: combat
                    .entities
                    .monsters
                    .iter()
                    .map(stable_monster_signature)
                    .collect(),
                powers: stable_powers_signature(combat),
                rng: rng_pool_key(&combat.rng.pool),
            },
        ),
        StableFrontierScope::PostCombat => StableOutcomeKey::new(
            scope,
            stable_postcombat_engine_key(engine, combat),
            StableOutcomePayload::PostCombat {
                player: stable_postcombat_player_key(&combat.entities.player),
                meta: stable_meta_key(&combat.meta),
                runtime: stable_postcombat_runtime_key(combat),
                rng: rng_pool_key(&combat.rng.pool),
            },
        ),
        StableFrontierScope::GameOver => StableOutcomeKey::new(
            scope,
            stable_postcombat_engine_key(engine, combat),
            StableOutcomePayload::GameOver,
        ),
    }
}

pub(super) fn stable_card_signature(card: &CombatCard) -> String {
    serde_json::to_string(&(
        card.id.stable_id(),
        card.upgrades,
        card.misc_value,
        card.base_damage_override,
        card.cost_modifier,
        card.cost_for_turn,
        card.base_damage_mut,
        card.base_block_mut,
        card.base_magic_num_mut,
        &card.multi_damage,
        card.exhaust_override,
        card.retain_override,
        card.free_to_play_once,
        card.energy_on_use,
    ))
    .expect("stable card signature should serialize")
}

fn stable_combat_engine_key(engine: &EngineState, combat: &CombatState) -> StableEngineKey {
    match engine {
        EngineState::CombatPlayerTurn => StableEngineKey::CombatReady,
        EngineState::CombatProcessing => StableEngineKey::CombatProcessing,
        EngineState::PendingChoice(choice) => {
            StableEngineKey::PendingChoice(pending_choice_key(choice, combat))
        }
        EngineState::CombatStart(request) => StableEngineKey::CombatStart(
            serde_json::to_string(request).expect("combat start request should serialize"),
        ),
        _ => stable_postcombat_engine_key(engine, combat),
    }
}

fn stable_postcombat_engine_key(engine: &EngineState, combat: &CombatState) -> StableEngineKey {
    match engine {
        EngineState::DoubleBossComplete => StableEngineKey::DoubleBossComplete,
        EngineState::Act3VictoryComplete => StableEngineKey::Act3VictoryComplete,
        EngineState::TrueVictoryComplete => StableEngineKey::TrueVictoryComplete,
        EngineState::RewardScreen(reward) => StableEngineKey::Reward(stable_reward_key(reward)),
        EngineState::RewardOverlay {
            reward_state,
            return_state,
        } => StableEngineKey::RewardOverlay {
            reward: stable_reward_key(reward_state),
            return_state: stable_run_pending_return_key(return_state),
        },
        EngineState::TreasureRoom(chest) => {
            StableEngineKey::TreasureRoom(stable_treasure_chest_key(chest))
        }
        EngineState::Campfire => StableEngineKey::Campfire,
        EngineState::Shop(shop) => StableEngineKey::Shop(stable_shop_key(shop)),
        EngineState::MapNavigation => StableEngineKey::MapNavigation,
        EngineState::MapOverlay { return_state } => {
            StableEngineKey::MapOverlay(stable_run_pending_return_key(return_state))
        }
        EngineState::EventRoom => StableEngineKey::EventRoom,
        EngineState::RunPendingChoice(state) => {
            StableEngineKey::RunPendingChoice(stable_run_pending_choice_key(state))
        }
        EngineState::CombatStart(request) => StableEngineKey::CombatStart(
            serde_json::to_string(request).expect("combat start request should serialize"),
        ),
        EngineState::BossRelicSelect(state) => {
            StableEngineKey::BossRelic(stable_boss_relic_key(state))
        }
        EngineState::GameOver(result) => {
            StableEngineKey::GameOver(stable_run_result_signature(result))
        }
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_) => stable_combat_engine_key(engine, combat),
    }
}

fn stable_turn_key(turn: &TurnRuntime) -> StableTurnKey {
    StableTurnKey {
        turn_count: turn.turn_count,
        current_phase: phase_key(turn.current_phase),
        energy: turn.energy,
        turn_start_draw_modifier: turn.turn_start_draw_modifier,
        cards_played_this_turn: turn.counters.cards_played_this_turn,
        attacks_played_this_turn: turn.counters.attacks_played_this_turn,
        times_damaged_this_combat: turn.counters.times_damaged_this_combat,
        victory_triggered: turn.counters.victory_triggered,
        discovery_cost_for_turn: turn.counters.discovery_cost_for_turn,
        early_end_turn_pending: turn.counters.early_end_turn_pending,
        player_escaping: turn.counters.player_escaping,
        escape_pending_reward: turn.counters.escape_pending_reward,
    }
}

fn stable_player_key(player: &PlayerEntity) -> StableCombatPlayerKey {
    StableCombatPlayerKey {
        max_hp: player.max_hp,
        facing_left: player.facing_left,
        orbs: serde_json::to_string(&player.orbs).expect("orbs should serialize"),
        max_orbs: player.max_orbs,
        stance: player.stance.stable_id().to_owned(),
        relics: serde_json::to_string(&player.relics).expect("relics should serialize"),
        energy_master: player.energy_master,
    }
}

fn stable_zones_key(zones: &CardZones) -> StableZonesKey {
    StableZonesKey {
        draw: stable_card_zone_key(&zones.draw_pile),
        hand: stable_card_zone_key(&zones.hand),
        discard: stable_card_zone_key(&zones.discard_pile),
        exhaust: stable_card_zone_key(&zones.exhaust_pile),
        limbo: stable_card_zone_key(&zones.limbo),
    }
}

fn stable_card_zone_key<'a>(cards: impl IntoIterator<Item = &'a CombatCard>) -> Vec<String> {
    cards.into_iter().map(stable_card_signature).collect()
}

fn stable_powers_signature(combat: &CombatState) -> Vec<String> {
    let mut powers = combat
        .entities
        .power_db
        .iter()
        .map(|(entity_id, powers)| {
            let mut entries = powers
                .iter()
                .map(stable_power_signature)
                .collect::<Vec<_>>();
            entries.sort();
            format!("{entity_id}:[{}]", entries.join(","))
        })
        .collect::<Vec<_>>();
    powers.sort();
    powers
}

fn stable_power_signature(power: &Power) -> String {
    let source = match power.payload {
        crate::runtime::combat::PowerPayload::SourceEntity(source) => Some(source),
        _ => None,
    };
    format!(
        "{}:{}:{}:{}:{source:?}",
        power.power_type.stable_id(),
        power.amount,
        power.extra_data,
        power.just_applied,
    )
}
