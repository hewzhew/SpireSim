use serde::Serialize;

use crate::ai::combat_search_v2::{
    combat_search_action_ordering_role_label_for_state,
    combat_search_phase_profile_report_for_state, CombatSearchV2ActionTrace, CombatSearchV2Config,
    CombatSearchV2PhaseProfileReport,
};
use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use crate::sim::combat_projection::monster_preview_total_damage_in_combat;
use crate::state::core::{ClientInput, EngineState};

use super::benchmark::CombatSearchV2LoadedBenchmarkCase;
use super::CombatSearchV2RunOptions;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2RolloutPolicyFirstActionDiff {
    pub action_index: usize,
    pub left_action_id: Option<usize>,
    pub left_action_key: Option<String>,
    pub left_action_debug: Option<String>,
    pub left_action_role: Option<&'static str>,
    pub right_action_id: Option<usize>,
    pub right_action_key: Option<String>,
    pub right_action_debug: Option<String>,
    pub right_action_role: Option<&'static str>,
    pub context: Option<CombatSearchV2RolloutPolicyFirstDiffContext>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2RolloutPolicyFirstDiffContext {
    pub replay_status: &'static str,
    pub engine_state: &'static str,
    pub turn: u32,
    pub player_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub visible_incoming_damage: i32,
    pub living_enemy_count: usize,
    pub total_enemy_hp: i32,
    pub total_enemy_block: i32,
    pub phase_profile: CombatSearchV2PhaseProfileReport,
}

pub(super) fn first_action_diff(
    loaded: &CombatSearchV2LoadedBenchmarkCase,
    options: &CombatSearchV2RunOptions,
    left: Option<&[CombatSearchV2ActionTrace]>,
    right: Option<&[CombatSearchV2ActionTrace]>,
) -> Option<CombatSearchV2RolloutPolicyFirstActionDiff> {
    let left = left.unwrap_or(&[]);
    let right = right.unwrap_or(&[]);
    let action_index = first_action_diff_index(left, right)?;
    let left_action = left.get(action_index);
    let right_action = right.get(action_index);
    let position = position_before_first_diff(loaded, options, left, right, action_index);
    let context = position.as_ref().map(first_diff_context);

    Some(CombatSearchV2RolloutPolicyFirstActionDiff {
        action_index,
        left_action_id: left_action.map(|action| action.action_id),
        left_action_key: left_action.map(|action| action.action_key.clone()),
        left_action_debug: left_action.map(|action| action.action_debug.clone()),
        left_action_role: action_role(position.as_ref(), left_action),
        right_action_id: right_action.map(|action| action.action_id),
        right_action_key: right_action.map(|action| action.action_key.clone()),
        right_action_debug: right_action.map(|action| action.action_debug.clone()),
        right_action_role: action_role(position.as_ref(), right_action),
        context,
    })
}

fn first_action_diff_index(
    left: &[CombatSearchV2ActionTrace],
    right: &[CombatSearchV2ActionTrace],
) -> Option<usize> {
    let max_len = left.len().max(right.len());
    (0..max_len).find(|&action_index| {
        left.get(action_index).map(action_identity) != right.get(action_index).map(action_identity)
    })
}

fn action_identity(action: &CombatSearchV2ActionTrace) -> (&str, usize) {
    (action.action_key.as_str(), action.action_id)
}

fn position_before_first_diff(
    loaded: &CombatSearchV2LoadedBenchmarkCase,
    options: &CombatSearchV2RunOptions,
    left: &[CombatSearchV2ActionTrace],
    right: &[CombatSearchV2ActionTrace],
    action_index: usize,
) -> Option<CombatPosition> {
    let mut position = loaded.start.position.clone();
    let stepper = EngineCombatStepper;
    let max_engine_steps = options
        .max_engine_steps_per_action
        .unwrap_or_else(|| CombatSearchV2Config::default().max_engine_steps_per_action);

    for prefix_index in 0..action_index {
        let left_action = left.get(prefix_index)?;
        let right_action = right.get(prefix_index)?;
        if action_identity(left_action) != action_identity(right_action) {
            return None;
        }
        let result = stepper.apply_to_stable(
            &position,
            left_action.input.clone(),
            CombatStepLimits {
                max_engine_steps,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out || !result.alive {
            return None;
        }
        position = result.position;
    }

    Some(position)
}

fn first_diff_context(position: &CombatPosition) -> CombatSearchV2RolloutPolicyFirstDiffContext {
    let living_monsters = position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action());
    let mut living_enemy_count = 0;
    let mut total_enemy_hp = 0;
    let mut total_enemy_block = 0;
    let mut visible_incoming_damage = 0;

    for monster in living_monsters {
        living_enemy_count += 1;
        total_enemy_hp += monster.current_hp.max(0);
        total_enemy_block += monster.block.max(0);
        visible_incoming_damage +=
            monster_preview_total_damage_in_combat(&position.combat, monster);
    }

    CombatSearchV2RolloutPolicyFirstDiffContext {
        replay_status: "replayed_common_prefix",
        engine_state: engine_state_label(&position.engine),
        turn: position.combat.turn.turn_count,
        player_hp: position.combat.entities.player.current_hp,
        player_block: position.combat.entities.player.block,
        energy: position.combat.turn.energy,
        visible_incoming_damage,
        living_enemy_count,
        total_enemy_hp,
        total_enemy_block,
        phase_profile: combat_search_phase_profile_report_for_state(
            &position.engine,
            &position.combat,
        ),
    }
}

fn action_role(
    position: Option<&CombatPosition>,
    action: Option<&CombatSearchV2ActionTrace>,
) -> Option<&'static str> {
    let action = action?;
    Some(match position {
        Some(position) => combat_search_action_ordering_role_label_for_state(
            &position.engine,
            &position.combat,
            &action.input,
        ),
        None => fallback_action_role(&action.input),
    })
}

fn fallback_action_role(input: &ClientInput) -> &'static str {
    match input {
        ClientInput::PlayCard { .. } => "play_card",
        ClientInput::UsePotion { .. } => "tactical_potion",
        ClientInput::DiscardPotion(_) => "discard_potion",
        ClientInput::EndTurn => "end_turn",
        ClientInput::SubmitCardChoice(_)
        | ClientInput::SubmitDiscoverChoice(_)
        | ClientInput::SubmitScryDiscard(_)
        | ClientInput::SubmitSelection(_)
        | ClientInput::SubmitRelicChoice(_) => "pending_choice",
        _ => "other",
    }
}

fn engine_state_label(engine: &EngineState) -> &'static str {
    match engine {
        EngineState::CombatPlayerTurn => "combat_player_turn",
        EngineState::CombatProcessing => "combat_processing",
        EngineState::RewardScreen(_) => "reward_screen",
        EngineState::RewardOverlay { .. } => "reward_overlay",
        EngineState::TreasureRoom(_) => "treasure_room",
        EngineState::Campfire => "campfire",
        EngineState::Shop(_) => "shop",
        EngineState::MapNavigation => "map_navigation",
        EngineState::MapOverlay { .. } => "map_overlay",
        EngineState::EventRoom => "event_room",
        EngineState::CombatStart(_) => "combat_start",
        EngineState::PendingChoice(_) => "pending_choice",
        EngineState::RunPendingChoice(_) => "run_pending_choice",
        EngineState::BossRelicSelect(_) => "boss_relic_select",
        EngineState::GameOver(_) => "game_over",
        EngineState::DoubleBossComplete => "double_boss_complete",
        EngineState::Act3VictoryComplete => "act3_victory_complete",
        EngineState::TrueVictoryComplete => "true_victory_complete",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_action_diff_finds_first_key_change() {
        let left = vec![
            trace(0, "combat/end_turn"),
            trace(
                1,
                "combat/play_card/hand:0/card:Strike_R+0#1/target:monster_slot:0",
            ),
        ];
        let right = vec![
            trace(0, "combat/end_turn"),
            trace(
                1,
                "combat/play_card/hand:1/card:Bash+0#2/target:monster_slot:0",
            ),
        ];

        let diff_index = first_action_diff_index(&left, &right).expect("diff should exist");

        assert_eq!(diff_index, 1);
    }

    fn trace(action_id: usize, action_key: &str) -> CombatSearchV2ActionTrace {
        CombatSearchV2ActionTrace {
            step_index: action_id,
            action_id,
            action_key: action_key.to_string(),
            action_debug: action_key.to_string(),
            input: ClientInput::EndTurn,
        }
    }
}
