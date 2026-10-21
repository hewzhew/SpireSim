use crate::content::cards::{get_card_definition, CardType};
use crate::sim::combat::{CombatPosition, EngineCombatStepper};
use crate::state::core::ClientInput;

use super::super::CombatSearchV2TrajectoryReport;
use super::replay::replay_one;
use super::types::CutPoint;

pub(super) fn per_cut_budget_ms(total_ms: u64, cut_count: usize) -> u64 {
    if cut_count == 0 {
        return total_ms;
    }
    (total_ms / cut_count as u64).clamp(500, 8_000)
}

pub(super) fn collect_cut_points(
    start: &CombatPosition,
    parent: &CombatSearchV2TrajectoryReport,
    max_cuts: usize,
) -> Vec<CutPoint> {
    let mut cuts = Vec::new();
    let mut position = start.clone();
    let stepper = EngineCombatStepper;
    for (index, action) in parent.actions.iter().enumerate() {
        match action.input {
            ClientInput::UsePotion { .. } => add_cut(&mut cuts, "before_potion", index),
            ClientInput::PlayCard { card_index, .. } if is_power_in_hand(&position, card_index) => {
                add_cut(&mut cuts, "before_power", index);
            }
            ClientInput::EndTurn => add_cut(&mut cuts, "turn_boundary", index + 1),
            _ => {}
        }
        let Some(next) = replay_one(&position, action, &stepper) else {
            break;
        };
        position = next;
    }

    if !parent.actions.is_empty() {
        add_cut(
            &mut cuts,
            "late_suffix_12",
            parent.actions.len().saturating_sub(12),
        );
        add_cut(
            &mut cuts,
            "late_suffix_6",
            parent.actions.len().saturating_sub(6),
        );
    }
    select_cut_points(cuts, max_cuts)
}

fn add_cut(cuts: &mut Vec<CutPoint>, kind: &'static str, action_index: usize) {
    if cuts.iter().any(|cut| cut.action_index == action_index) {
        return;
    }
    cuts.push(CutPoint { kind, action_index });
}

fn select_cut_points(cuts: Vec<CutPoint>, max_cuts: usize) -> Vec<CutPoint> {
    if cuts.len() <= max_cuts {
        return cuts;
    }
    let mut selected = Vec::new();
    for kind in [
        "before_potion",
        "before_power",
        "late_suffix_12",
        "late_suffix_6",
    ] {
        for cut in cuts.iter().filter(|cut| cut.kind == kind) {
            add_cut(&mut selected, cut.kind, cut.action_index);
            if selected.len() >= max_cuts {
                return selected;
            }
        }
    }
    for cut in cuts.iter().rev().filter(|cut| cut.kind == "turn_boundary") {
        add_cut(&mut selected, cut.kind, cut.action_index);
        if selected.len() >= max_cuts {
            return selected;
        }
    }
    for cut in cuts {
        add_cut(&mut selected, cut.kind, cut.action_index);
        if selected.len() >= max_cuts {
            break;
        }
    }
    selected
}

fn is_power_in_hand(position: &CombatPosition, card_index: usize) -> bool {
    position
        .combat
        .zones
        .hand
        .get(card_index)
        .is_some_and(|card| get_card_definition(card.id).card_type == CardType::Power)
}
