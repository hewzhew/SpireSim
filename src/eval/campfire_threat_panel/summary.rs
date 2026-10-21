use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::eval::campfire_survival_scenarios::{CampfireSurvivalLens, CampfireSurvivalSubject};
use crate::eval::combat_lab_v1::CombatLabOutcomeClassV1;

use super::{
    CampfireThreatEncounterV1, CampfireThreatPanelCellV1,
    CAMPFIRE_THREAT_PANEL_SUMMARY_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampfireThreatPanelSummaryV1 {
    pub schema_version: u32,
    pub contract_hash: String,
    pub requested_samples: u64,
    pub completed_cells: usize,
    pub strata: Vec<CampfireThreatStratumSummaryV1>,
    pub pairs: Vec<CampfireThreatPairSummaryV1>,
    pub reversals: Vec<CampfireThreatDirectionReversalV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampfireThreatStratumSummaryV1 {
    pub lens: CampfireSurvivalLens,
    pub encounter: CampfireThreatEncounterV1,
    pub subject: CampfireSurvivalSubject,
    pub requested_cells: u64,
    pub completed_cells: usize,
    pub resolved_cells: usize,
    pub wins: usize,
    pub losses: usize,
    pub coverage_limited: usize,
    pub errors: usize,
    pub replayed_complete_candidates: usize,
    pub replayed_win_candidates: usize,
    pub replayed_loss_candidates: usize,
    pub terminal_hp: CampfireThreatNumericSummaryV1,
    pub hp_loss: CampfireThreatNumericSummaryV1,
    pub turns: CampfireThreatNumericSummaryV1,
    pub potions_used: CampfireThreatNumericSummaryV1,
    pub expanded_nodes: CampfireThreatNumericSummaryV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampfireThreatPairSummaryV1 {
    pub lens: CampfireSurvivalLens,
    pub encounter: CampfireThreatEncounterV1,
    pub left: CampfireSurvivalSubject,
    pub right: CampfireSurvivalSubject,
    pub shared_samples: usize,
    pub replayed_pairs: usize,
    pub resolved_pairs: usize,
    pub coverage_limited_pairs: usize,
    pub incomplete_pairs: usize,
    pub final_hp_delta_left_minus_right: CampfireThreatNumericSummaryV1,
    pub hp_loss_delta_left_minus_right: CampfireThreatNumericSummaryV1,
    pub turns_delta_left_minus_right: CampfireThreatNumericSummaryV1,
    pub potions_delta_left_minus_right: CampfireThreatNumericSummaryV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampfireThreatDirectionReversalV1 {
    pub lens: CampfireSurvivalLens,
    pub left: CampfireSurvivalSubject,
    pub right: CampfireSurvivalSubject,
    pub left_better_encounters: Vec<CampfireThreatEncounterV1>,
    pub right_better_encounters: Vec<CampfireThreatEncounterV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampfireThreatNumericSummaryV1 {
    pub count: usize,
    pub mean: Option<f64>,
    pub median: Option<f64>,
    pub min: Option<i64>,
    pub max: Option<i64>,
}

pub fn summarize_campfire_threat_panel_v1(
    contract_hash: &str,
    cells: &[CampfireThreatPanelCellV1],
    requested_samples: u64,
) -> Result<CampfireThreatPanelSummaryV1, String> {
    if requested_samples == 0 {
        return Err("Campfire threat panel requested_samples must be nonzero".to_string());
    }
    if let Some(foreign) = cells
        .iter()
        .find(|cell| cell.contract_hash != contract_hash)
    {
        return Err(format!(
            "Campfire threat cell '{}' has foreign contract hash '{}'",
            foreign.cell_key, foreign.contract_hash
        ));
    }

    let mut groups = BTreeMap::<String, Vec<&CampfireThreatPanelCellV1>>::new();
    for cell in cells {
        groups.entry(stratum_key(cell)).or_default().push(cell);
    }
    let strata = groups
        .values()
        .map(|group| summarize_stratum(group, requested_samples))
        .collect::<Vec<_>>();

    let pairs = summarize_pairs(cells, requested_samples);
    let reversals = summarize_reversals(&pairs);
    Ok(CampfireThreatPanelSummaryV1 {
        schema_version: CAMPFIRE_THREAT_PANEL_SUMMARY_SCHEMA_VERSION,
        contract_hash: contract_hash.to_string(),
        requested_samples,
        completed_cells: cells.len(),
        strata,
        pairs,
        reversals,
    })
}

fn summarize_stratum(
    cells: &[&CampfireThreatPanelCellV1],
    requested_samples: u64,
) -> CampfireThreatStratumSummaryV1 {
    let first = cells[0];
    let replayed = cells
        .iter()
        .filter_map(|cell| cell.replayed_candidate.as_ref())
        .collect::<Vec<_>>();
    CampfireThreatStratumSummaryV1 {
        lens: first.lens,
        encounter: first.encounter.clone(),
        subject: first.subject,
        requested_cells: requested_samples,
        completed_cells: cells.len(),
        resolved_cells: cells
            .iter()
            .filter(|cell| {
                matches!(
                    cell.outcome_class,
                    CombatLabOutcomeClassV1::ResolvedWin | CombatLabOutcomeClassV1::ResolvedLoss
                )
            })
            .count(),
        wins: cells
            .iter()
            .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::ResolvedWin)
            .count(),
        losses: cells
            .iter()
            .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::ResolvedLoss)
            .count(),
        coverage_limited: cells
            .iter()
            .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::CoverageLimited)
            .count(),
        errors: cells
            .iter()
            .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::ExecutionError)
            .count(),
        replayed_complete_candidates: replayed.len(),
        replayed_win_candidates: replayed
            .iter()
            .filter(|candidate| {
                candidate.terminal == crate::ai::combat_search_v2::SearchTerminalLabel::Win
            })
            .count(),
        replayed_loss_candidates: replayed
            .iter()
            .filter(|candidate| {
                candidate.terminal == crate::ai::combat_search_v2::SearchTerminalLabel::Loss
            })
            .count(),
        terminal_hp: numeric(
            replayed
                .iter()
                .map(|candidate| i64::from(candidate.final_hp)),
        ),
        hp_loss: numeric(
            replayed
                .iter()
                .map(|candidate| i64::from(candidate.hp_loss)),
        ),
        turns: numeric(replayed.iter().map(|candidate| i64::from(candidate.turns))),
        potions_used: numeric(
            replayed
                .iter()
                .map(|candidate| i64::from(candidate.potions_used)),
        ),
        expanded_nodes: numeric(
            cells
                .iter()
                .map(|cell| i64::try_from(cell.expanded_nodes).unwrap_or(i64::MAX)),
        ),
    }
}

fn summarize_pairs(
    cells: &[CampfireThreatPanelCellV1],
    requested_samples: u64,
) -> Vec<CampfireThreatPairSummaryV1> {
    let mut contexts = BTreeMap::<String, Vec<&CampfireThreatPanelCellV1>>::new();
    for cell in cells {
        contexts
            .entry(pair_context_key(cell))
            .or_default()
            .push(cell);
    }
    let mut summaries = Vec::new();
    for group in contexts.values() {
        let mut subjects = group.iter().map(|cell| cell.subject).collect::<Vec<_>>();
        subjects.sort_by_key(subject_key);
        subjects.dedup();
        for left_index in 0..subjects.len() {
            for right_index in (left_index + 1)..subjects.len() {
                let left = subjects[left_index];
                let right = subjects[right_index];
                let mut left_by_sample = BTreeMap::new();
                let mut right_by_sample = BTreeMap::new();
                for cell in group {
                    if cell.subject == left {
                        left_by_sample.insert(cell.sample_index, *cell);
                    } else if cell.subject == right {
                        right_by_sample.insert(cell.sample_index, *cell);
                    }
                }
                let mut final_hp = Vec::new();
                let mut hp_loss = Vec::new();
                let mut turns = Vec::new();
                let mut potions = Vec::new();
                let mut resolved_pairs = 0;
                let mut coverage_limited_pairs = 0;
                let mut shared_samples = 0;
                for sample_index in 0..requested_samples {
                    let (Some(left_cell), Some(right_cell)) = (
                        left_by_sample.get(&sample_index),
                        right_by_sample.get(&sample_index),
                    ) else {
                        continue;
                    };
                    shared_samples += 1;
                    if matches!(
                        left_cell.outcome_class,
                        CombatLabOutcomeClassV1::ResolvedWin
                            | CombatLabOutcomeClassV1::ResolvedLoss
                    ) && matches!(
                        right_cell.outcome_class,
                        CombatLabOutcomeClassV1::ResolvedWin
                            | CombatLabOutcomeClassV1::ResolvedLoss
                    ) {
                        resolved_pairs += 1;
                    } else if left_cell.outcome_class == CombatLabOutcomeClassV1::CoverageLimited
                        || right_cell.outcome_class == CombatLabOutcomeClassV1::CoverageLimited
                    {
                        coverage_limited_pairs += 1;
                    }
                    let (Some(left_candidate), Some(right_candidate)) = (
                        left_cell.replayed_candidate.as_ref(),
                        right_cell.replayed_candidate.as_ref(),
                    ) else {
                        continue;
                    };
                    final_hp.push(
                        i64::from(left_candidate.final_hp) - i64::from(right_candidate.final_hp),
                    );
                    hp_loss.push(
                        i64::from(left_candidate.hp_loss) - i64::from(right_candidate.hp_loss),
                    );
                    turns.push(i64::from(left_candidate.turns) - i64::from(right_candidate.turns));
                    potions.push(
                        i64::from(left_candidate.potions_used)
                            - i64::from(right_candidate.potions_used),
                    );
                }
                let replayed_pairs = final_hp.len();
                summaries.push(CampfireThreatPairSummaryV1 {
                    lens: group[0].lens,
                    encounter: group[0].encounter.clone(),
                    left,
                    right,
                    shared_samples,
                    replayed_pairs,
                    resolved_pairs,
                    coverage_limited_pairs,
                    incomplete_pairs: requested_samples as usize - replayed_pairs,
                    final_hp_delta_left_minus_right: numeric(final_hp),
                    hp_loss_delta_left_minus_right: numeric(hp_loss),
                    turns_delta_left_minus_right: numeric(turns),
                    potions_delta_left_minus_right: numeric(potions),
                });
            }
        }
    }
    summaries
}

fn summarize_reversals(
    pairs: &[CampfireThreatPairSummaryV1],
) -> Vec<CampfireThreatDirectionReversalV1> {
    let mut grouped = BTreeMap::<String, Vec<&CampfireThreatPairSummaryV1>>::new();
    for pair in pairs {
        grouped.entry(reversal_key(pair)).or_default().push(pair);
    }
    grouped
        .values()
        .filter_map(|group| {
            let mut left_better = Vec::new();
            let mut right_better = Vec::new();
            for pair in group {
                match pair.final_hp_delta_left_minus_right.median {
                    Some(value) if value > 0.0 => left_better.push(pair.encounter.clone()),
                    Some(value) if value < 0.0 => right_better.push(pair.encounter.clone()),
                    _ => {}
                }
            }
            (!left_better.is_empty() && !right_better.is_empty()).then(|| {
                CampfireThreatDirectionReversalV1 {
                    lens: group[0].lens,
                    left: group[0].left,
                    right: group[0].right,
                    left_better_encounters: left_better,
                    right_better_encounters: right_better,
                }
            })
        })
        .collect()
}

fn numeric(values: impl IntoIterator<Item = i64>) -> CampfireThreatNumericSummaryV1 {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort_unstable();
    let count = values.len();
    let mean =
        (count != 0).then(|| values.iter().map(|value| *value as f64).sum::<f64>() / count as f64);
    let median = match count {
        0 => None,
        count if count % 2 == 1 => Some(values[count / 2] as f64),
        count => Some((values[count / 2 - 1] as f64 + values[count / 2] as f64) / 2.0),
    };
    CampfireThreatNumericSummaryV1 {
        count,
        mean,
        median,
        min: values.first().copied(),
        max: values.last().copied(),
    }
}

fn stratum_key(cell: &CampfireThreatPanelCellV1) -> String {
    format!(
        "{:?}|{:?}|{}",
        cell.lens,
        cell.encounter.encounter_id,
        subject_key(&cell.subject)
    )
}

fn pair_context_key(cell: &CampfireThreatPanelCellV1) -> String {
    format!(
        "{:?}|{}",
        cell.lens,
        cell.encounter.encounter_id.stable_id()
    )
}

fn reversal_key(pair: &CampfireThreatPairSummaryV1) -> String {
    format!(
        "{:?}|{}|{}",
        pair.lens,
        subject_key(&pair.left),
        subject_key(&pair.right)
    )
}

fn subject_key(subject: &CampfireSurvivalSubject) -> String {
    serde_json::to_string(subject).expect("Campfire survival subject should serialize")
}
