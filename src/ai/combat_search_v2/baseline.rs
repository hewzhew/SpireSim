use super::*;

pub const WHOLE_COMBAT_OUTCOME_CRITERIA: [&str; 5] = [
    "win_over_loss",
    "higher_final_hp",
    "fewer_potions_used",
    "fewer_turns",
    "fewer_cards_played",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatSearchV2OutcomeMetrics {
    pub terminal: SearchTerminalLabel,
    pub final_hp: i32,
    pub potions_used: u32,
    pub turns: u32,
    pub cards_played: u32,
}

impl CombatSearchV2OutcomeMetrics {
    pub fn from_trajectory(trajectory: &CombatSearchV2TrajectoryReport) -> Self {
        Self {
            terminal: trajectory.terminal,
            final_hp: trajectory.final_hp,
            potions_used: trajectory.potions_used,
            turns: trajectory.turns,
            cards_played: trajectory.cards_played,
        }
    }
}

pub fn compare_outcome_metrics(
    left: CombatSearchV2OutcomeMetrics,
    right: CombatSearchV2OutcomeMetrics,
) -> Ordering {
    terminal_rank(left.terminal)
        .cmp(&terminal_rank(right.terminal))
        .then_with(|| left.final_hp.cmp(&right.final_hp))
        .then_with(|| right.potions_used.cmp(&left.potions_used))
        .then_with(|| right.turns.cmp(&left.turns))
        .then_with(|| right.cards_played.cmp(&left.cards_played))
}
