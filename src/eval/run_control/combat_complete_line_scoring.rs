use crate::content::cards::{get_card_definition, CardType};
use crate::sim::combat::{CombatPosition, CombatTerminal};
use crate::sim::combat_projection::monster_preview_total_damage_in_combat;
use crate::state::core::ClientInput;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum LineLane {
    Root,
    Setup,
    SetupPath,
    Progress,
    Survival,
    Other,
}

pub(super) fn classify_lane(
    before: &CombatPosition,
    after: &CombatPosition,
    input: &ClientInput,
) -> LineLane {
    if after.combat.are_monsters_basically_dead_java() {
        return LineLane::Progress;
    }
    if played_power(before, input) {
        return LineLane::Setup;
    }
    if enemy_effort(&after.combat) < enemy_effort(&before.combat) {
        return LineLane::Progress;
    }
    if net_visible_pressure(&after.combat) < net_visible_pressure(&before.combat)
        || after.combat.entities.player.block > before.combat.entities.player.block
    {
        return LineLane::Survival;
    }
    LineLane::Other
}

pub(super) fn played_power(position: &CombatPosition, input: &ClientInput) -> bool {
    let ClientInput::PlayCard { card_index, .. } = input else {
        return false;
    };
    position
        .combat
        .zones
        .hand
        .get(*card_index)
        .is_some_and(|card| get_card_definition(card.id).card_type == CardType::Power)
}

pub(super) fn score_position(
    position: &CombatPosition,
    terminal: CombatTerminal,
    initial_hp: i32,
    action_count: usize,
) -> i64 {
    let hp_loss = (initial_hp - position.combat.entities.player.current_hp).max(0) as i64;
    let enemy_effort = enemy_effort(&position.combat) as i64;
    let net_pressure = net_visible_pressure(&position.combat) as i64;
    match terminal {
        CombatTerminal::Win => 1_000_000 - hp_loss * 10_000 - action_count as i64,
        CombatTerminal::Loss => -1_000_000 - action_count as i64,
        CombatTerminal::Unresolved => {
            -hp_loss * 2_000 - enemy_effort * 450 - net_pressure * 700 - action_count as i64
        }
    }
}

fn net_visible_pressure(combat: &crate::runtime::combat::CombatState) -> i32 {
    (visible_incoming(combat) - combat.entities.player.block).max(0)
}

fn enemy_effort(combat: &crate::runtime::combat::CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| !monster.is_dead_or_escaped())
        .map(|monster| monster.current_hp.max(0) + monster.block.max(0))
        .sum()
}

fn visible_incoming(combat: &crate::runtime::combat::CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| !monster.is_dead_or_escaped())
        .map(|monster| monster_preview_total_damage_in_combat(combat, monster))
        .sum()
}
