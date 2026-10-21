//! Read-only observations over a completed local combat search graph.

use serde_json::{json, Value};
use sts_combat_planner::LocalTurnGraphWitnessSession;
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::sim::combat::CombatPosition;

use super::combat_trace_view::combat_action_label;
use super::exact_turn_corridor::ExactTurnCorridor;

pub(super) struct LocalGraphObservation {
    pub(super) root_action_families: Vec<Value>,
    pub(super) watched_states: Vec<Value>,
    pub(super) watched_corridor: Option<Value>,
}

pub(super) fn capture_local_graph_observation(
    session: &LocalTurnGraphWitnessSession,
    search_root_position: &CombatPosition,
    watched_exact_state_hashes: &[String],
    watched_corridor: Option<&ExactTurnCorridor>,
) -> LocalGraphObservation {
    let root_action_families = session
        .root_action_families()
        .into_iter()
        .map(|family| {
            json!({
                "action": combat_action_label(search_root_position, &family.first_action),
                "best_root_negative_log_policy": family.best_root_negative_log_policy,
                "completed_root_turn_options": family.completed_root_turn_options,
                "terminal_wins": family.terminal_wins,
                "terminal_losses": family.terminal_losses,
                "escapes": family.escapes,
                "unique_next_turn_successors": family.unique_next_turn_successors,
                "retained_next_turn_successors": family.retained_next_turn_successors,
                "reachable_exact_states": family.reachable_exact_states,
                "reachable_retained_states": family.reachable_retained_states,
                "reachable_generation_work": family.reachable_generation_work,
                "reachable_completed_turn_options": family.reachable_completed_turn_options,
                "max_player_turn": family.max_player_turn,
                "best_hp_at_max_turn": family.best_hp_at_max_turn,
                "lowest_enemy_hp_at_max_turn": family.lowest_enemy_hp_at_max_turn,
            })
        })
        .collect();

    let root_exact_state_hash =
        combat_exact_state_hash_v2(&search_root_position.engine, &search_root_position.combat);
    let watched_states = watched_exact_state_hashes
        .iter()
        .map(|hash| {
            json!({
                "exact_state_hash": hash,
                "state": session.state_snapshot_by_exact_hash(hash),
                "incoming_from_root": session.edge_snapshot_by_exact_hashes(
                    &root_exact_state_hash,
                    hash,
                ),
            })
        })
        .collect();

    let watched_corridor = watched_corridor.map(|corridor| {
        let mut ranked_hashes = corridor
            .rank_by_exact_hash
            .iter()
            .map(|(hash, rank)| (*rank, hash))
            .collect::<Vec<_>>();
        ranked_hashes.sort_by_key(|(rank, _)| *rank);
        let states = ranked_hashes
            .iter()
            .enumerate()
            .map(|(index, (rank, hash))| {
                let outgoing_to_next = ranked_hashes.get(index + 1).and_then(|(_, next_hash)| {
                    session.edge_snapshot_by_exact_hashes(hash, next_hash)
                });
                json!({
                    "corridor_rank": rank,
                    "exact_state_hash": hash,
                    "state": session.state_snapshot_by_exact_hash(hash),
                    "outgoing_to_next": outgoing_to_next,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "authority": "diagnostic_only",
            "changes_search_order": false,
            "action_count": corridor.action_count,
            "exact_turn_states": states.len(),
            "terminal_final_hp": corridor.terminal_final_hp,
            "states": states,
        })
    });

    LocalGraphObservation {
        root_action_families,
        watched_states,
        watched_corridor,
    }
}
