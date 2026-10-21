use crate::content::powers::PowerId;
use crate::runtime::combat::CombatState;
use crate::sim::combat::CombatStepResult;
use crate::state::core::ClientInput;

use super::super::phase_profile::combat_search_phase_profile;
use super::super::transition::terminal_label;
use super::types::CombatSearchV2ActionExactDeltaFacts;

pub(super) fn exact_delta_facts_from_step(
    combat: &CombatState,
    step: &CombatStepResult,
) -> CombatSearchV2ActionExactDeltaFacts {
    let before_enemy_hp = total_monster_hp(combat);
    let before_enemy_block = total_monster_block(combat);
    let after = &step.position.combat;
    let phase = combat_search_phase_profile(&step.position.engine, after);
    CombatSearchV2ActionExactDeltaFacts {
        status: step_status(step),
        terminal: terminal_label(&step.position.engine, after),
        engine_steps: step.engine_steps,
        player_hp_delta: after.entities.player.current_hp - combat.entities.player.current_hp,
        player_block_delta: after.entities.player.block - combat.entities.player.block,
        player_strength_delta: after.get_power(after.entities.player.id, PowerId::Strength)
            - combat.get_power(combat.entities.player.id, PowerId::Strength),
        energy_delta: i32::from(after.turn.energy) - i32::from(combat.turn.energy),
        hand_delta: len_delta(after.zones.hand.len(), combat.zones.hand.len()),
        draw_delta: len_delta(after.zones.draw_pile.len(), combat.zones.draw_pile.len()),
        discard_delta: len_delta(
            after.zones.discard_pile.len(),
            combat.zones.discard_pile.len(),
        ),
        exhaust_delta: len_delta(
            after.zones.exhaust_pile.len(),
            combat.zones.exhaust_pile.len(),
        ),
        limbo_delta: len_delta(after.zones.limbo.len(), combat.zones.limbo.len()),
        queued_cards_delta: len_delta(
            after.zones.queued_cards.len(),
            combat.zones.queued_cards.len(),
        ),
        total_enemy_hp_delta: total_monster_hp(after) - before_enemy_hp,
        total_enemy_block_delta: total_monster_block(after) - before_enemy_block,
        pending_choice_present: phase.pending_choice.present,
        pending_choice_estimated_action_fanout: phase.pending_choice.estimated_action_fanout,
    }
}

pub(super) fn action_kind(input: &ClientInput) -> &'static str {
    match input {
        ClientInput::PlayCard { .. } => "play_card",
        ClientInput::UsePotion { .. } => "use_potion",
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

fn total_monster_hp(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp.max(0))
        .sum()
}

fn total_monster_block(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.block.max(0))
        .sum()
}

fn len_delta(after: usize, before: usize) -> i32 {
    after as i32 - before as i32
}

fn step_status(step: &CombatStepResult) -> &'static str {
    if step.timed_out {
        "timed_out"
    } else if step.truncated {
        "engine_step_limit"
    } else if !step.alive {
        "player_dead"
    } else {
        "stable"
    }
}
