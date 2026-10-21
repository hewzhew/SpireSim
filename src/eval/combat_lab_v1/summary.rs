use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{CombatLabCellRecordV1, CombatLabManifestV1, CombatLabOutcomeClassV1};

pub const COMBAT_LAB_SUMMARY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabSummaryV1 {
    pub schema_version: u32,
    pub experiment_hash: String,
    pub requested_samples: u64,
    pub completed_cells: usize,
    pub profiles: Vec<CombatLabProfileSummaryV1>,
    pub pairs: Vec<CombatLabPairSummaryV1>,
    pub interaction: Option<CombatLabInteractionSummaryV1>,
    pub interaction_omitted_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabProfileSummaryV1 {
    pub profile_id: String,
    pub requested_cells: u64,
    pub completed_cells: usize,
    pub resolved_cells: usize,
    pub wins: usize,
    pub losses: usize,
    pub coverage_limited: usize,
    pub errors: usize,
    pub candidates: CombatLabCandidateSummaryV1,
    pub win_rate_all_non_error: Option<f64>,
    pub win_rate_all_non_error_denominator: usize,
    pub win_rate_resolved: Option<f64>,
    pub win_rate_resolved_denominator: usize,
    pub hp_loss_mean: Option<f64>,
    pub hp_loss_stddev_population: Option<f64>,
    pub hp_loss_median: Option<f64>,
    pub hp_loss_p90_nearest_rank: Option<i32>,
    pub terminal_hp_mean: Option<f64>,
    pub terminal_hp_stddev_population: Option<f64>,
    pub terminal_hp_median: Option<f64>,
    pub terminal_hp_p10_nearest_rank: Option<i32>,
    pub turns: CombatLabNumericSummaryV1,
    pub potions_used: CombatLabNumericSummaryV1,
    pub expanded_nodes: CombatLabNumericSummaryV1,
    pub deadline_exhaustion_rate: Option<f64>,
    pub node_budget_exhaustion_rate: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabCandidateSummaryV1 {
    pub replayed_complete_candidates: usize,
    pub replayed_win_candidates: usize,
    pub replayed_loss_candidates: usize,
    pub replayed_win_rate_all_non_error: Option<f64>,
    pub replayed_win_rate_all_non_error_denominator: usize,
    pub win_hp_loss: CombatLabNumericSummaryV1,
    pub terminal_hp: CombatLabNumericSummaryV1,
    pub turns: CombatLabNumericSummaryV1,
    pub potions_used: CombatLabNumericSummaryV1,
    pub nodes_to_first_win: CombatLabNumericSummaryV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabNumericSummaryV1 {
    pub count: usize,
    pub mean: Option<f64>,
    pub stddev_population: Option<f64>,
    pub median: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabPairSummaryV1 {
    pub left_profile_id: String,
    pub right_profile_id: String,
    pub shared_samples: usize,
    pub incomplete_pair_samples: usize,
    pub both_win: usize,
    pub left_only_win: usize,
    pub right_only_win: usize,
    pub both_loss: usize,
    pub unresolved_or_error: usize,
    pub comparable_resolved_samples: usize,
    pub final_hp_delta_left_minus_right: CombatLabNumericSummaryV1,
    pub hp_loss_delta_left_minus_right: CombatLabNumericSummaryV1,
    pub left_strictly_better: usize,
    pub right_strictly_better: usize,
    pub tied: usize,
    pub divergences: Vec<CombatLabPairDivergenceV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabPairDivergenceV1 {
    pub sample_index: u64,
    pub first_action_divergence: Option<usize>,
    pub first_draw_divergence: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabInteractionSummaryV1 {
    pub eligible_samples: usize,
    pub profile_count: usize,
    pub total_sum_squares: f64,
    pub shuffle_sum_squares: f64,
    pub profile_sum_squares: f64,
    pub interaction_sum_squares: f64,
    pub shuffle_share: Option<f64>,
    pub profile_share: Option<f64>,
    pub interaction_share: Option<f64>,
}

pub fn summarize_combat_lab_v1(
    manifest: &CombatLabManifestV1,
    cells: &[CombatLabCellRecordV1],
    requested_samples: u64,
) -> Result<CombatLabSummaryV1, String> {
    let mut sorted_cells = cells.iter().collect::<Vec<_>>();
    sorted_cells.sort_by(|left, right| {
        (left.sample_index, left.profile_id.as_str())
            .cmp(&(right.sample_index, right.profile_id.as_str()))
    });
    if let Some(foreign) = sorted_cells
        .iter()
        .find(|cell| cell.experiment_hash != manifest.experiment_hash)
    {
        return Err(format!(
            "combat laboratory cell '{}' has foreign experiment hash '{}'; expected '{}'",
            foreign.cell_key, foreign.experiment_hash, manifest.experiment_hash
        ));
    }

    let profiles = manifest
        .resolved_spec
        .profiles
        .iter()
        .map(|profile| summarize_profile(&profile.spec.id, &sorted_cells, requested_samples))
        .collect();

    let mut pairs = Vec::new();
    for left_index in 0..manifest.resolved_spec.profiles.len() {
        for right_index in (left_index + 1)..manifest.resolved_spec.profiles.len() {
            pairs.push(summarize_pair(
                &manifest.resolved_spec.profiles[left_index].spec.id,
                &manifest.resolved_spec.profiles[right_index].spec.id,
                &sorted_cells,
                requested_samples,
            )?);
        }
    }
    let (interaction, eligible_interaction_samples) =
        summarize_interaction(manifest, &sorted_cells);
    let interaction_omitted_reason = if manifest.resolved_spec.profiles.len() < 2 {
        Some("interaction requires at least two profiles".to_string())
    } else if eligible_interaction_samples == 0 {
        Some(
            "interaction requires a balanced resolved matrix; found 0 fully resolved shared samples"
                .to_string(),
        )
    } else if eligible_interaction_samples == 1 {
        Some("interaction requires at least two balanced resolved samples; found 1".to_string())
    } else {
        None
    };

    Ok(CombatLabSummaryV1 {
        schema_version: COMBAT_LAB_SUMMARY_SCHEMA_VERSION,
        experiment_hash: manifest.experiment_hash.clone(),
        requested_samples,
        completed_cells: cells.len(),
        profiles,
        pairs,
        interaction,
        interaction_omitted_reason,
    })
}

fn summarize_interaction(
    manifest: &CombatLabManifestV1,
    cells: &[&CombatLabCellRecordV1],
) -> (Option<CombatLabInteractionSummaryV1>, usize) {
    let profiles = &manifest.resolved_spec.profiles;
    if profiles.len() < 2 {
        return (None, 0);
    }
    let mut resolved_hp_by_sample = BTreeMap::<u64, BTreeMap<&str, f64>>::new();
    for cell in cells {
        if resolved(cell) {
            if let Some(final_hp) = cell.final_hp {
                resolved_hp_by_sample
                    .entry(cell.sample_index)
                    .or_default()
                    .insert(cell.profile_id.as_str(), f64::from(final_hp));
            }
        }
    }
    let rows = resolved_hp_by_sample
        .values()
        .filter_map(|by_profile| {
            profiles
                .iter()
                .map(|profile| by_profile.get(profile.spec.id.as_str()).copied())
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Vec<_>>();
    if rows.len() < 2 {
        let eligible_samples = rows.len();
        return (None, eligible_samples);
    }

    let eligible_samples = rows.len();
    let profile_count = profiles.len();
    let grand_mean = rows.iter().flatten().sum::<f64>() / (eligible_samples * profile_count) as f64;
    let sample_means = rows
        .iter()
        .map(|row| row.iter().sum::<f64>() / profile_count as f64)
        .collect::<Vec<_>>();
    let profile_means = (0..profile_count)
        .map(|profile_index| {
            rows.iter().map(|row| row[profile_index]).sum::<f64>() / eligible_samples as f64
        })
        .collect::<Vec<_>>();
    let total_sum_squares = rows
        .iter()
        .flatten()
        .map(|value| (*value - grand_mean).powi(2))
        .sum::<f64>();
    let shuffle_sum_squares = profile_count as f64
        * sample_means
            .iter()
            .map(|mean| (*mean - grand_mean).powi(2))
            .sum::<f64>();
    let profile_sum_squares = eligible_samples as f64
        * profile_means
            .iter()
            .map(|mean| (*mean - grand_mean).powi(2))
            .sum::<f64>();
    let interaction_sum_squares = rows
        .iter()
        .enumerate()
        .map(|(sample_index, row)| {
            row.iter()
                .enumerate()
                .map(|(profile_index, value)| {
                    (*value - sample_means[sample_index] - profile_means[profile_index]
                        + grand_mean)
                        .powi(2)
                })
                .sum::<f64>()
        })
        .sum::<f64>();
    let share = |sum_squares| (total_sum_squares != 0.0).then_some(sum_squares / total_sum_squares);

    (
        Some(CombatLabInteractionSummaryV1 {
            eligible_samples,
            profile_count,
            total_sum_squares,
            shuffle_sum_squares,
            profile_sum_squares,
            interaction_sum_squares,
            shuffle_share: share(shuffle_sum_squares),
            profile_share: share(profile_sum_squares),
            interaction_share: share(interaction_sum_squares),
        }),
        eligible_samples,
    )
}

fn summarize_pair(
    left_profile_id: &str,
    right_profile_id: &str,
    cells: &[&CombatLabCellRecordV1],
    requested_samples: u64,
) -> Result<CombatLabPairSummaryV1, String> {
    let left = cells
        .iter()
        .copied()
        .filter(|cell| cell.profile_id == left_profile_id)
        .map(|cell| (cell.sample_index, cell))
        .collect::<BTreeMap<_, _>>();
    let right = cells
        .iter()
        .copied()
        .filter(|cell| cell.profile_id == right_profile_id)
        .map(|cell| (cell.sample_index, cell))
        .collect::<BTreeMap<_, _>>();
    let shared = left
        .iter()
        .filter_map(|(sample_index, left)| {
            right
                .get(sample_index)
                .map(|right| (*sample_index, *left, *right))
        })
        .collect::<Vec<_>>();
    let requested_sample_count = usize::try_from(requested_samples).map_err(|_| {
        format!(
            "requested sample target {requested_samples} cannot be represented as a pair count on this platform"
        )
    })?;
    let shared_requested_samples = shared
        .iter()
        .filter(|(sample_index, _, _)| *sample_index < requested_samples)
        .count();
    let incomplete_pair_samples = requested_sample_count
        .checked_sub(shared_requested_samples)
        .ok_or_else(|| {
            format!(
                "shared requested sample count {shared_requested_samples} exceeds requested target {requested_sample_count}"
            )
        })?;

    let mut both_win = 0;
    let mut left_only_win = 0;
    let mut right_only_win = 0;
    let mut both_loss = 0;
    let mut unresolved_or_error = 0;
    let mut final_hp_deltas = Vec::new();
    let mut hp_loss_deltas = Vec::new();
    let mut comparable_resolved_samples = 0;
    let mut left_strictly_better = 0;
    let mut right_strictly_better = 0;
    let mut tied = 0;
    let mut divergences = Vec::new();

    for (sample_index, left, right) in &shared {
        match (left.outcome_class, right.outcome_class) {
            (CombatLabOutcomeClassV1::ResolvedWin, CombatLabOutcomeClassV1::ResolvedWin) => {
                both_win += 1;
            }
            (CombatLabOutcomeClassV1::ResolvedWin, CombatLabOutcomeClassV1::ResolvedLoss) => {
                left_only_win += 1;
            }
            (CombatLabOutcomeClassV1::ResolvedLoss, CombatLabOutcomeClassV1::ResolvedWin) => {
                right_only_win += 1;
            }
            (CombatLabOutcomeClassV1::ResolvedLoss, CombatLabOutcomeClassV1::ResolvedLoss) => {
                both_loss += 1;
            }
            _ => unresolved_or_error += 1,
        }

        if resolved(left) && resolved(right) {
            comparable_resolved_samples += 1;
            if let (Some(left_hp), Some(right_hp)) = (left.final_hp, right.final_hp) {
                final_hp_deltas.push(f64::from(left_hp) - f64::from(right_hp));
            }
            if let (Some(left_loss), Some(right_loss)) = (left.hp_loss, right.hp_loss) {
                hp_loss_deltas.push(f64::from(left_loss) - f64::from(right_loss));
            }
            if let (Some(left_key), Some(right_key)) =
                (left.outcome_order_key, right.outcome_order_key)
            {
                match left_key.cmp(&right_key) {
                    Ordering::Greater => left_strictly_better += 1,
                    Ordering::Less => right_strictly_better += 1,
                    Ordering::Equal => tied += 1,
                }
            }
            if left.replay_validated && right.replay_validated {
                let first_action_divergence =
                    first_divergence(&left.action_history, &right.action_history);
                let first_draw_divergence =
                    first_divergence(&left.draw_history, &right.draw_history);
                if first_action_divergence.is_some() || first_draw_divergence.is_some() {
                    divergences.push(CombatLabPairDivergenceV1 {
                        sample_index: *sample_index,
                        first_action_divergence,
                        first_draw_divergence,
                    });
                }
            }
        }
    }

    Ok(CombatLabPairSummaryV1 {
        left_profile_id: left_profile_id.to_string(),
        right_profile_id: right_profile_id.to_string(),
        shared_samples: shared.len(),
        incomplete_pair_samples,
        both_win,
        left_only_win,
        right_only_win,
        both_loss,
        unresolved_or_error,
        comparable_resolved_samples,
        final_hp_delta_left_minus_right: numeric_summary(final_hp_deltas),
        hp_loss_delta_left_minus_right: numeric_summary(hp_loss_deltas),
        left_strictly_better,
        right_strictly_better,
        tied,
        divergences,
    })
}

fn resolved(cell: &CombatLabCellRecordV1) -> bool {
    matches!(
        cell.outcome_class,
        CombatLabOutcomeClassV1::ResolvedWin | CombatLabOutcomeClassV1::ResolvedLoss
    )
}

fn first_divergence<T: PartialEq>(left: &[T], right: &[T]) -> Option<usize> {
    let shared_len = left.len().min(right.len());
    left.iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .or_else(|| (left.len() != right.len()).then_some(shared_len))
}

fn summarize_profile(
    profile_id: &str,
    cells: &[&CombatLabCellRecordV1],
    requested_samples: u64,
) -> CombatLabProfileSummaryV1 {
    let cells = cells
        .iter()
        .copied()
        .filter(|cell| cell.profile_id == profile_id)
        .collect::<Vec<_>>();
    let wins = cells
        .iter()
        .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::ResolvedWin)
        .count();
    let losses = cells
        .iter()
        .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::ResolvedLoss)
        .count();
    let coverage_limited = cells
        .iter()
        .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::CoverageLimited)
        .count();
    let errors = cells
        .iter()
        .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::ExecutionError)
        .count();
    let resolved_cells = wins + losses;
    let all_non_error_denominator = cells.len() - errors;
    let mut win_hp_losses = cells
        .iter()
        .filter(|cell| cell.outcome_class == CombatLabOutcomeClassV1::ResolvedWin)
        .filter_map(|cell| cell.hp_loss)
        .collect::<Vec<_>>();
    win_hp_losses.sort_unstable();
    let mut resolved_terminal_hp = cells
        .iter()
        .filter(|cell| {
            matches!(
                cell.outcome_class,
                CombatLabOutcomeClassV1::ResolvedWin | CombatLabOutcomeClassV1::ResolvedLoss
            )
        })
        .filter_map(|cell| cell.final_hp)
        .collect::<Vec<_>>();
    resolved_terminal_hp.sort_unstable();
    let deadline_exhaustions = cells.iter().filter(|cell| cell.deadline_exhausted).count();
    let node_budget_exhaustions = cells
        .iter()
        .filter(|cell| cell.node_budget_exhausted)
        .count();
    let replayed_candidates = cells
        .iter()
        .filter_map(|cell| cell.replayed_candidate.as_ref())
        .collect::<Vec<_>>();
    let replayed_win_candidates = replayed_candidates
        .iter()
        .filter(|candidate| {
            candidate.terminal == crate::ai::combat_search_v2::SearchTerminalLabel::Win
        })
        .count();
    let replayed_loss_candidates = replayed_candidates
        .iter()
        .filter(|candidate| {
            candidate.terminal == crate::ai::combat_search_v2::SearchTerminalLabel::Loss
        })
        .count();
    let candidates = CombatLabCandidateSummaryV1 {
        replayed_complete_candidates: replayed_candidates.len(),
        replayed_win_candidates,
        replayed_loss_candidates,
        replayed_win_rate_all_non_error: ratio(replayed_win_candidates, all_non_error_denominator),
        replayed_win_rate_all_non_error_denominator: all_non_error_denominator,
        win_hp_loss: numeric_summary(
            replayed_candidates
                .iter()
                .filter(|candidate| {
                    candidate.terminal == crate::ai::combat_search_v2::SearchTerminalLabel::Win
                })
                .map(|candidate| f64::from(candidate.hp_loss)),
        ),
        terminal_hp: numeric_summary(
            replayed_candidates
                .iter()
                .map(|candidate| f64::from(candidate.final_hp)),
        ),
        turns: numeric_summary(
            replayed_candidates
                .iter()
                .map(|candidate| f64::from(candidate.turns)),
        ),
        potions_used: numeric_summary(
            replayed_candidates
                .iter()
                .map(|candidate| f64::from(candidate.potions_used)),
        ),
        nodes_to_first_win: numeric_summary(cells.iter().filter_map(|cell| {
            let candidate = cell.replayed_candidate.as_ref()?;
            (candidate.terminal == crate::ai::combat_search_v2::SearchTerminalLabel::Win)
                .then_some(cell.nodes_to_first_win)
                .flatten()
                .map(|nodes| nodes as f64)
        })),
    };

    CombatLabProfileSummaryV1 {
        profile_id: profile_id.to_string(),
        requested_cells: requested_samples,
        completed_cells: cells.len(),
        resolved_cells,
        wins,
        losses,
        coverage_limited,
        errors,
        candidates,
        win_rate_all_non_error: ratio(wins, all_non_error_denominator),
        win_rate_all_non_error_denominator: all_non_error_denominator,
        win_rate_resolved: ratio(wins, resolved_cells),
        win_rate_resolved_denominator: resolved_cells,
        hp_loss_mean: mean_i32(&win_hp_losses),
        hp_loss_stddev_population: stddev_population_i32(&win_hp_losses),
        hp_loss_median: median_i32(&win_hp_losses),
        hp_loss_p90_nearest_rank: nearest_rank(&win_hp_losses, 9, 10),
        terminal_hp_mean: mean_i32(&resolved_terminal_hp),
        terminal_hp_stddev_population: stddev_population_i32(&resolved_terminal_hp),
        terminal_hp_median: median_i32(&resolved_terminal_hp),
        terminal_hp_p10_nearest_rank: nearest_rank(&resolved_terminal_hp, 1, 10),
        turns: numeric_summary(cells.iter().filter_map(|cell| cell.turns.map(f64::from))),
        potions_used: numeric_summary(
            cells
                .iter()
                .filter_map(|cell| cell.potions_used.map(f64::from)),
        ),
        expanded_nodes: numeric_summary(cells.iter().map(|cell| cell.expanded_nodes as f64)),
        deadline_exhaustion_rate: ratio(deadline_exhaustions, cells.len()),
        node_budget_exhaustion_rate: ratio(node_budget_exhaustions, cells.len()),
    }
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator != 0).then(|| numerator as f64 / denominator as f64)
}

fn mean_i32(sorted: &[i32]) -> Option<f64> {
    (!sorted.is_empty())
        .then(|| sorted.iter().map(|value| f64::from(*value)).sum::<f64>() / sorted.len() as f64)
}

fn stddev_population_i32(sorted: &[i32]) -> Option<f64> {
    let mean = mean_i32(sorted)?;
    Some(
        (sorted
            .iter()
            .map(|value| (f64::from(*value) - mean).powi(2))
            .sum::<f64>()
            / sorted.len() as f64)
            .sqrt(),
    )
}

fn median_i32(sorted: &[i32]) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        Some((f64::from(sorted[middle - 1]) + f64::from(sorted[middle])) / 2.0)
    } else {
        Some(f64::from(sorted[middle]))
    }
}

fn nearest_rank(sorted: &[i32], numerator: usize, denominator: usize) -> Option<i32> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (numerator * sorted.len())
        .div_ceil(denominator)
        .clamp(1, sorted.len());
    Some(sorted[rank - 1])
}

fn numeric_summary(values: impl IntoIterator<Item = f64>) -> CombatLabNumericSummaryV1 {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let count = values.len();
    if count == 0 {
        return CombatLabNumericSummaryV1 {
            count,
            mean: None,
            stddev_population: None,
            median: None,
        };
    }
    let mean = values.iter().sum::<f64>() / count as f64;
    let stddev_population = (values
        .iter()
        .map(|value| (*value - mean).powi(2))
        .sum::<f64>()
        / count as f64)
        .sqrt();
    let middle = count / 2;
    let median = if count % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    };
    CombatLabNumericSummaryV1 {
        count,
        mean: Some(mean),
        stddev_population: Some(stddev_population),
        median: Some(median),
    }
}
