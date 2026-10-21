use std::collections::{BTreeMap, BTreeSet};

use crate::ai::combat_search_v2::turn_planner::types::{
    TurnPlanBucket, TurnPlanCandidateDropReasonV1, TurnPlanCandidateSelectionAuditV1,
    TurnPlanCandidateSelectionOutcomeV1, TurnPlanCoverageGroupAuditV1, TurnPlanCoverageGroupKeyV1,
    TurnPlanCoverageSignatureV1, TurnPlanFirstActionSummaryV1, TurnPlanSelectionAuditV1,
    TurnPlanV1, TurnPlannerConfigV1,
};

use super::ranking::compare_turn_plan_seed_candidate;

pub(super) fn first_action_summaries(
    candidates: &[TurnPlanV1],
) -> Vec<TurnPlanFirstActionSummaryV1> {
    let mut by_key = BTreeMap::<String, TurnPlanFirstActionSummaryV1>::new();
    for candidate in candidates {
        let Some(action) = candidate.actions.first() else {
            continue;
        };
        let entry = by_key.entry(action.action_key.clone()).or_insert_with(|| {
            TurnPlanFirstActionSummaryV1 {
                action: action.clone(),
                plan_count: 0,
                bucket_counts: BTreeMap::new(),
            }
        });
        entry.plan_count = entry.plan_count.saturating_add(1);
        *entry.bucket_counts.entry(candidate.bucket).or_default() += 1;
    }
    by_key.into_values().collect()
}

pub(super) fn bucket_counts(candidates: &[TurnPlanV1]) -> BTreeMap<TurnPlanBucket, usize> {
    let mut counts = BTreeMap::<TurnPlanBucket, usize>::new();
    for candidate in candidates {
        *counts.entry(candidate.bucket).or_default() += 1;
    }
    counts
}

pub(super) fn select_bucketed_plans(
    mut candidates: Vec<TurnPlanV1>,
    config: &TurnPlannerConfigV1,
    prior_state_hash: Option<&str>,
) -> (Vec<TurnPlanV1>, TurnPlanSelectionAuditV1) {
    if config.max_end_states == 0 || config.per_bucket_limit == 0 {
        let reason = if config.max_end_states == 0 {
            TurnPlanCandidateDropReasonV1::MaxEndStates
        } else {
            TurnPlanCandidateDropReasonV1::SelectionDisabled
        };
        return (
            Vec::new(),
            selection_audit(&candidates, &[], vec![Some(reason); candidates.len()]),
        );
    }

    candidates.sort_by(|left, right| {
        compare_turn_plan_seed_candidate(
            right,
            left,
            prior_state_hash,
            config.turn_plan_prior.as_ref(),
        )
    });
    let mut selected = Vec::new();
    let mut selected_indexes = vec![false; candidates.len()];
    let mut selected_plan_indexes = vec![None; candidates.len()];
    let mut drop_reasons = vec![None; candidates.len()];
    let mut bucket_counts = BTreeMap::<TurnPlanBucket, usize>::new();
    let mut selected_coverage = BTreeSet::new();

    for bucket in TURN_PLAN_BUCKET_DIVERSITY_ORDER {
        if selected.len() >= config.max_end_states {
            break;
        }
        if let Some((index, candidate)) = candidates
            .iter()
            .enumerate()
            .find(|(index, candidate)| !selected_indexes[*index] && candidate.bucket == bucket)
        {
            bucket_counts.insert(candidate.bucket, 1);
            selected_indexes[index] = true;
            selected_plan_indexes[index] = Some(selected.len());
            selected_coverage.insert(TurnPlanCoverageGroupKeyV1 {
                bucket: candidate.bucket,
                coverage: TurnPlanCoverageSignatureV1::from_plan(candidate).coverage_key(),
            });
            selected.push(candidate.clone());
        }
    }

    for (index, candidate) in candidates.iter().enumerate() {
        if selected.len() >= config.max_end_states {
            break;
        }
        if selected_indexes[index] {
            continue;
        }
        let count = bucket_counts.entry(candidate.bucket).or_default();
        if *count >= config.per_bucket_limit {
            continue;
        }
        let coverage = TurnPlanCoverageGroupKeyV1 {
            bucket: candidate.bucket,
            coverage: TurnPlanCoverageSignatureV1::from_plan(candidate).coverage_key(),
        };
        if !selected_coverage.insert(coverage) {
            continue;
        }
        *count = count.saturating_add(1);
        selected_indexes[index] = true;
        selected_plan_indexes[index] = Some(selected.len());
        selected.push(candidate.clone());
    }

    for (index, candidate) in candidates.iter().enumerate() {
        if selected.len() >= config.max_end_states {
            break;
        }
        if !selected_indexes[index] {
            let count = bucket_counts.entry(candidate.bucket).or_default();
            if *count >= config.per_bucket_limit {
                continue;
            }
            *count = count.saturating_add(1);
            selected_indexes[index] = true;
            selected_plan_indexes[index] = Some(selected.len());
            selected.push(candidate.clone());
        }
    }

    for (index, was_selected) in selected_indexes.iter().enumerate() {
        if *was_selected {
            continue;
        }
        let candidate = &candidates[index];
        drop_reasons[index] = Some(
            if bucket_counts
                .get(&candidate.bucket)
                .copied()
                .unwrap_or_default()
                >= config.per_bucket_limit
            {
                TurnPlanCandidateDropReasonV1::BucketCap
            } else {
                TurnPlanCandidateDropReasonV1::MaxEndStates
            },
        );
        if selected.len() < config.max_end_states {
            debug_assert_eq!(
                drop_reasons[index],
                Some(TurnPlanCandidateDropReasonV1::BucketCap)
            );
        }
    }

    let audit = selection_audit(&candidates, &selected_plan_indexes, drop_reasons);
    (selected, audit)
}

fn selection_audit(
    candidates: &[TurnPlanV1],
    selected_plan_indexes: &[Option<usize>],
    drop_reasons: Vec<Option<TurnPlanCandidateDropReasonV1>>,
) -> TurnPlanSelectionAuditV1 {
    let mut audits = Vec::with_capacity(candidates.len());
    let mut groups = BTreeMap::<TurnPlanCoverageGroupKeyV1, TurnPlanCoverageGroupAuditV1>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let coverage_signature = TurnPlanCoverageSignatureV1::from_plan(candidate);
        let coverage_key = coverage_signature.coverage_key();
        let group_key = TurnPlanCoverageGroupKeyV1 {
            bucket: candidate.bucket,
            coverage: coverage_key,
        };
        let selected_plan_index = selected_plan_indexes.get(index).copied().flatten();
        let drop_reason = drop_reasons.get(index).copied().flatten();
        let outcome = if selected_plan_index.is_some() {
            TurnPlanCandidateSelectionOutcomeV1::Selected
        } else {
            TurnPlanCandidateSelectionOutcomeV1::Dropped
        };
        let group =
            groups
                .entry(group_key.clone())
                .or_insert_with(|| TurnPlanCoverageGroupAuditV1 {
                    key: group_key,
                    preselection_count: 0,
                    selected_count: 0,
                    bucket_cap_dropped_count: 0,
                    max_end_states_dropped_count: 0,
                });
        group.preselection_count = group.preselection_count.saturating_add(1);
        match outcome {
            TurnPlanCandidateSelectionOutcomeV1::Selected => {
                group.selected_count = group.selected_count.saturating_add(1);
            }
            TurnPlanCandidateSelectionOutcomeV1::Dropped => match drop_reason {
                Some(TurnPlanCandidateDropReasonV1::BucketCap) => {
                    group.bucket_cap_dropped_count =
                        group.bucket_cap_dropped_count.saturating_add(1);
                }
                Some(TurnPlanCandidateDropReasonV1::MaxEndStates) => {
                    group.max_end_states_dropped_count =
                        group.max_end_states_dropped_count.saturating_add(1);
                }
                Some(TurnPlanCandidateDropReasonV1::SelectionDisabled) | None => {}
            },
        }
        audits.push(TurnPlanCandidateSelectionAuditV1 {
            preselection_rank: index,
            selected_plan_index,
            outcome,
            drop_reason,
            bucket: candidate.bucket,
            action_keys: candidate
                .actions
                .iter()
                .map(|action| action.action_key.clone())
                .collect(),
            coverage_key,
            coverage_signature,
        });
    }
    TurnPlanSelectionAuditV1 {
        candidates: audits,
        coverage_groups: groups.into_values().collect(),
    }
}

const TURN_PLAN_BUCKET_DIVERSITY_ORDER: [TurnPlanBucket; 7] = [
    TurnPlanBucket::TerminalWin,
    TurnPlanBucket::Progress,
    TurnPlanBucket::Survival,
    TurnPlanBucket::Setup,
    TurnPlanBucket::Balanced,
    TurnPlanBucket::Boundary,
    TurnPlanBucket::TerminalLoss,
];
