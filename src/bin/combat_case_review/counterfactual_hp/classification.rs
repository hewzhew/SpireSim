use sts_oracle_runtime::sim::combat::CombatTerminal;

use super::types::{CounterfactualHpClassification, CounterfactualHpLevel};

pub(super) fn classify_counterfactual_hp_probe(
    levels: &[CounterfactualHpLevel],
    original_hp: i32,
) -> CounterfactualHpClassification {
    if levels
        .iter()
        .any(|level| level.hp == original_hp && level.complete_win)
    {
        return CounterfactualHpClassification::OriginalHpWin;
    }
    if levels.iter().any(|level| {
        level.hp != original_hp
            && level
                .replay_on_original_hp
                .as_ref()
                .is_some_and(|replay| matches!(replay.terminal, CombatTerminal::Win))
    }) {
        return CounterfactualHpClassification::CounterfactualLineStillWinsOriginalHp;
    }
    if levels
        .iter()
        .any(|level| level.hp != original_hp && level.complete_win)
    {
        return CounterfactualHpClassification::CounterfactualOnlyWin;
    }
    CounterfactualHpClassification::NoWinFound
}
