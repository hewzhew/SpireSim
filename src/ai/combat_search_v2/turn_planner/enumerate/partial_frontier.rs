use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ai::combat_search_v2::rollout_pending_choice::RolloutPendingChoiceProgress;
use crate::ai::combat_search_v2::turn_plan_probe::{
    CombatSearchV2TurnPlanPartialCandidateAuditV1,
    CombatSearchV2TurnPlanPartialGuideConditionedAuditV1,
    CombatSearchV2TurnPlanPartialRetentionAuditV1, CombatSearchV2TurnPlanPartialRetentionLaneV1,
    CombatSearchV2TurnPlanProbeGuideRankV1,
};
use crate::ai::combat_search_v2::turn_planner::types::TurnPlannerConfigV1;
use crate::ai::combat_search_v2::value::{combat_eval_from_rollout_estimate, CombatEvalV2};
use crate::ai::combat_search_v2::{
    CombatSearchV2ActionPendingChoiceSelectionFacts,
    CombatSearchV2ActionPendingChoiceSelectionRole, RolloutNodeEstimate, RolloutStopReason,
};

use super::resource_transformation::{
    continuation_resource_projections, PartialContinuationProjection,
    PartialContinuationProjectionScore,
};
use super::work::TurnPlanWorkNode;

const PARTIAL_FRONTIER_MIN_WIDTH: usize = 8;
const PARTIAL_FRONTIER_MAX_WIDTH: usize = 32;
const PARTIAL_FRONTIER_END_STATE_MULTIPLIER: usize = 4;
const PARTIAL_FRONTIER_PENDING_CHOICE_TRANSACTION_DIVISOR: usize = 3;
const PARTIAL_RESOURCE_TRANSFORMATION_FOLLOW_UP_HORIZON: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct PartialActionScore {
    durable_player_strength: i32,
    recurring_turn_energy: i32,
    attack_replay_charges: i32,
    installed_power_count: i32,
    future_attack_damage_percent_gain: i32,
    retained_next_turn_block: i32,
    sustained_mitigation: i32,
    visible_mitigation: i32,
    debuff_setup: i32,
    progress_hint: i32,
    focused_target_progress: i32,
    access_gain: i32,
    resource_timing: i32,
    reactive_safety: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct PartialPendingChoiceSelectionScore {
    role_rank: i32,
    primary: i32,
    secondary: i32,
    selected_count_tiebreak: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct CompletedResourceTransformationCoverageKey {
    transformation_action_key: String,
    first_follow_up_action_key: String,
    current_follow_up_action_key: String,
}

struct ScoredWorkNode {
    work: Option<TurnPlanWorkNode>,
    first_action_key: String,
    continuation_projections: Vec<PartialContinuationProjection>,
    completed_resource_transformation_key: Option<CompletedResourceTransformationCoverageKey>,
    eval: CombatEvalV2,
    action: PartialActionScore,
    pending_choice_continuation_action_key: Option<String>,
    pending_choice_selection: Option<PartialPendingChoiceSelectionScore>,
    pending_choice_selection_parent_key: Option<Vec<String>>,
    pending_choice_selection_action_key: Option<String>,
    living_enemy_score: i32,
    guides: Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
}

pub(super) fn select_partial_frontier(
    candidates: Vec<TurnPlanWorkNode>,
    config: &TurnPlannerConfigV1,
    root_action_len: usize,
    partial_state_guides: Option<
        &dyn Fn(&crate::sim::combat::CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
    >,
    capture_audit: bool,
) -> (
    Vec<TurnPlanWorkNode>,
    Option<CombatSearchV2TurnPlanPartialRetentionAuditV1>,
) {
    let width = partial_frontier_width(config);
    if candidates.len() <= width && !capture_audit {
        return (candidates, None);
    }
    let mut scored = candidates
        .into_iter()
        .map(|work| score_work_node(work, root_action_len, partial_state_guides))
        .collect::<Vec<_>>();
    let balanced = ordered_indexes(&scored, compare_balanced);
    let survival = ordered_indexes(&scored, compare_survival);
    let progress = ordered_indexes(&scored, compare_progress);
    let setup = ordered_indexes(&scored, compare_setup);
    let power_installation = power_installation_coverage(&scored);
    let resource_transformation = resource_transformation_coverage(&scored);
    let pending_choice_continuation = pending_choice_continuation_coverage(&scored);
    let pending_choice_selection = pending_choice_selection_coverage(&scored);
    let pending_choice_transaction =
        interleave_indexes(&pending_choice_continuation, &pending_choice_selection);
    let survival_ranks = ranks_for_order(&survival, scored.len());
    let progress_ranks = ranks_for_order(&progress, scored.len());
    let setup_ranks = ranks_for_order(&setup, scored.len());
    let power_installation_ranks = optional_ranks_for_order(&power_installation, scored.len());
    let resource_transformation_ranks =
        optional_ranks_for_order(&resource_transformation, scored.len());
    let pending_choice_continuation_ranks =
        optional_ranks_for_order(&pending_choice_continuation, scored.len());
    let pending_choice_selection_ranks =
        optional_ranks_for_order(&pending_choice_selection, scored.len());
    let balanced_ranks = ranks_for_order(&balanced, scored.len());
    let first_action_balanced_ranks = first_action_ranks(&scored, &balanced);
    let guide_conditioned_ranks = if capture_audit {
        guide_conditioned_ranks(&scored)
    } else {
        vec![Vec::new(); scored.len()]
    };
    if scored.len() <= width {
        let audit = CombatSearchV2TurnPlanPartialRetentionAuditV1 {
            max_retained_states: width,
            candidates: (0..scored.len())
                .map(|index| CombatSearchV2TurnPlanPartialCandidateAuditV1 {
                    selected_by: Some(CombatSearchV2TurnPlanPartialRetentionLaneV1::Untruncated),
                    survival_rank: survival_ranks[index],
                    progress_rank: progress_ranks[index],
                    setup_rank: setup_ranks[index],
                    power_installation_rank: power_installation_ranks[index],
                    resource_transformation_rank: resource_transformation_ranks[index],
                    pending_choice_continuation_rank: pending_choice_continuation_ranks[index],
                    pending_choice_selection_rank: pending_choice_selection_ranks[index],
                    balanced_rank: balanced_ranks[index],
                    first_action_balanced_rank: first_action_balanced_ranks[index],
                    guide_conditioned_ranks: guide_conditioned_ranks[index].clone(),
                })
                .collect(),
        };
        let retained = scored
            .into_iter()
            .filter_map(|candidate| candidate.work)
            .collect();
        return (retained, Some(audit));
    }

    let mut selected = Vec::with_capacity(width);
    let mut selected_flags = vec![false; scored.len()];
    let mut selected_by = vec![None; scored.len()];
    let mut covered_first_actions = HashSet::new();
    let mut first_action_coverage = Vec::new();
    for &index in &balanced {
        if covered_first_actions.insert(scored[index].first_action_key.clone()) {
            first_action_coverage.push(index);
        }
    }

    let mut guide_lanes = BTreeMap::<u32, Vec<usize>>::new();
    for (index, candidate) in scored.iter().enumerate() {
        for guide in &candidate.guides {
            guide_lanes.entry(guide.lane).or_default().push(index);
        }
    }
    for (lane, indexes) in &mut guide_lanes {
        indexes.sort_by(|left, right| {
            guide_components(&scored[*right], *lane)
                .cmp(&guide_components(&scored[*left], *lane))
                .then_with(|| left.cmp(right))
        });
    }
    // A pending choice such as Headbutt's draw-pile target is a compact but
    // long-horizon branch surface. Its local ordering owner is useful, but is
    // not strong enough to collapse the alternatives to one representative.
    // Reserve a strict minority of the bounded frontier for distinct choices;
    // the remaining two thirds stay available to the ordinary combat lanes.
    let pending_choice_transaction_quota = width
        .checked_div(PARTIAL_FRONTIER_PENDING_CHOICE_TRANSACTION_DIVISOR)
        .unwrap_or_default()
        .max(1)
        .min(pending_choice_transaction.len());
    for index in pending_choice_transaction
        .iter()
        .copied()
        .take(pending_choice_transaction_quota)
    {
        let lane = if scored[index]
            .pending_choice_continuation_action_key
            .is_some()
        {
            CombatSearchV2TurnPlanPartialRetentionLaneV1::PendingChoiceContinuation
        } else {
            CombatSearchV2TurnPlanPartialRetentionLaneV1::PendingChoiceSelection
        };
        select_index(
            index,
            lane,
            &mut selected,
            &mut selected_flags,
            &mut selected_by,
            width,
        );
    }
    let mut lanes = vec![
        (
            &first_action_coverage,
            CombatSearchV2TurnPlanPartialRetentionLaneV1::FirstActionCoverage,
        ),
        (
            &survival,
            CombatSearchV2TurnPlanPartialRetentionLaneV1::Survival,
        ),
        (
            &progress,
            CombatSearchV2TurnPlanPartialRetentionLaneV1::Progress,
        ),
        (&setup, CombatSearchV2TurnPlanPartialRetentionLaneV1::Setup),
        (
            &power_installation,
            CombatSearchV2TurnPlanPartialRetentionLaneV1::PowerInstallation,
        ),
        (
            &resource_transformation,
            CombatSearchV2TurnPlanPartialRetentionLaneV1::ResourceTransformation,
        ),
        (
            &balanced,
            CombatSearchV2TurnPlanPartialRetentionLaneV1::Balanced,
        ),
    ];
    lanes.extend(guide_lanes.iter().map(|(lane, indexes)| {
        (
            indexes,
            CombatSearchV2TurnPlanPartialRetentionLaneV1::Guide(*lane),
        )
    }));
    let mut lane_offsets = vec![0usize; lanes.len()];
    while selected.len() < width {
        let mut added = false;
        for (lane_index, (lane, reason)) in lanes.iter().enumerate() {
            while lane_offsets[lane_index] < lane.len() {
                let index = lane[lane_offsets[lane_index]];
                lane_offsets[lane_index] = lane_offsets[lane_index].saturating_add(1);
                if selected_flags[index] {
                    continue;
                }
                select_index(
                    index,
                    *reason,
                    &mut selected,
                    &mut selected_flags,
                    &mut selected_by,
                    width,
                );
                added = true;
                break;
            }
            if selected.len() >= width {
                break;
            }
        }
        if !added {
            break;
        }
    }

    let audit = capture_audit.then(|| CombatSearchV2TurnPlanPartialRetentionAuditV1 {
        max_retained_states: width,
        candidates: (0..scored.len())
            .map(|index| CombatSearchV2TurnPlanPartialCandidateAuditV1 {
                selected_by: selected_by[index],
                survival_rank: survival_ranks[index],
                progress_rank: progress_ranks[index],
                setup_rank: setup_ranks[index],
                power_installation_rank: power_installation_ranks[index],
                resource_transformation_rank: resource_transformation_ranks[index],
                pending_choice_continuation_rank: pending_choice_continuation_ranks[index],
                pending_choice_selection_rank: pending_choice_selection_ranks[index],
                balanced_rank: balanced_ranks[index],
                first_action_balanced_rank: first_action_balanced_ranks[index],
                guide_conditioned_ranks: guide_conditioned_ranks[index].clone(),
            })
            .collect(),
    });
    let retained = selected
        .into_iter()
        .filter_map(|index| scored[index].work.take())
        .collect();
    (retained, audit)
}

pub(super) fn partial_frontier_width(config: &TurnPlannerConfigV1) -> usize {
    config
        .max_partial_frontier_states
        .unwrap_or_else(|| {
            config
                .max_end_states
                .max(1)
                .saturating_mul(PARTIAL_FRONTIER_END_STATE_MULTIPLIER)
                .clamp(PARTIAL_FRONTIER_MIN_WIDTH, PARTIAL_FRONTIER_MAX_WIDTH)
        })
        .max(1)
        .min(config.max_inner_nodes.max(1))
}

fn score_work_node(
    work: TurnPlanWorkNode,
    root_action_len: usize,
    partial_state_guides: Option<
        &dyn Fn(&crate::sim::combat::CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
    >,
) -> ScoredWorkNode {
    let actions_simulated = work.node.actions.len().saturating_sub(root_action_len);
    let estimate = RolloutNodeEstimate::from_node(
        &work.node,
        actions_simulated,
        RolloutStopReason::MaxActions,
        None,
        RolloutPendingChoiceProgress::default(),
    );
    let eval = combat_eval_from_rollout_estimate(&estimate);
    let mut action = partial_action_score(&work);
    action.retained_next_turn_block = projected_retained_next_turn_block(&work.node.combat);
    let pending_choice_continuation_action_key = work
        .action_facts
        .last()
        .filter(|facts| facts.immediate.creates_pending_choice_after_one_step)
        .and_then(|_| {
            work.node
                .actions
                .last()
                .map(|action| action.action_key.clone())
        });
    let pending_choice_selection = work
        .action_facts
        .last()
        .and_then(|facts| facts.pending_choice_selection.as_ref())
        .map(partial_pending_choice_selection_score);
    let pending_choice_selection_parent_key = pending_choice_selection.and_then(|_| {
        let selection_index = work.node.actions.len().checked_sub(1)?;
        Some(
            work.node.actions[root_action_len..selection_index]
                .iter()
                .map(|action| action.action_key.clone())
                .collect(),
        )
    });
    let pending_choice_selection_action_key = pending_choice_selection.and_then(|_| {
        work.node
            .actions
            .last()
            .map(|action| action.action_key.clone())
    });
    let first_action_key = work
        .node
        .actions
        .get(root_action_len)
        .map(|action| action.action_key.clone())
        .unwrap_or_default();
    let continuation_projections = continuation_resource_projections(&work);
    let completed_resource_transformation_key =
        completed_resource_transformation_coverage_key(&work, root_action_len);
    let living_enemy_score = -(work
        .node
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count() as i32);
    let guides = partial_state_guides.map_or_else(Vec::new, |guide| {
        guide(&crate::sim::combat::CombatPosition::new(
            work.node.engine.clone(),
            work.node.combat.clone(),
        ))
    });
    ScoredWorkNode {
        work: Some(work),
        first_action_key,
        continuation_projections,
        completed_resource_transformation_key,
        eval,
        action,
        pending_choice_continuation_action_key,
        pending_choice_selection,
        pending_choice_selection_parent_key,
        pending_choice_selection_action_key,
        living_enemy_score,
        guides,
    }
}

/// Preserve the strongest input state for each runtime-owned resource
/// transformation that remains legally playable. Unlike ordinary action
/// coverage, representatives are compared by the resource *after* the
/// transformation. Thus 24 block followed by `DoubleBlock` outranks 8 block
/// followed by the same operation without assigning a value to Entrench by
/// card name.
fn resource_transformation_coverage(scored: &[ScoredWorkNode]) -> Vec<usize> {
    let mut best_by_family =
        BTreeMap::<&'static str, (usize, PartialContinuationProjectionScore)>::new();
    for (index, candidate) in scored.iter().enumerate() {
        for projection in &candidate.continuation_projections {
            let replace =
                best_by_family
                    .get(projection.family)
                    .is_none_or(|(current, current_score)| {
                        projection
                            .score
                            .cmp(current_score)
                            .then_with(|| compare_balanced(candidate, &scored[*current]))
                            .is_gt()
                    });
            if replace {
                best_by_family.insert(projection.family, (index, projection.score));
            }
        }
    }
    let mut pending_transformations = best_by_family
        .into_values()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    pending_transformations.sort_unstable();
    pending_transformations.dedup();
    pending_transformations.sort_by(|left, right| {
        compare_balanced(&scored[*right], &scored[*left]).then_with(|| left.cmp(right))
    });

    let mut best_by_completed_follow_up =
        BTreeMap::<CompletedResourceTransformationCoverageKey, usize>::new();
    for (index, candidate) in scored.iter().enumerate() {
        let Some(key) = candidate.completed_resource_transformation_key.as_ref() else {
            continue;
        };
        let replace = best_by_completed_follow_up
            .get(key)
            .is_none_or(|current| compare_balanced(candidate, &scored[*current]).is_gt());
        if replace {
            best_by_completed_follow_up.insert(key.clone(), index);
        }
    }
    let mut completed_follow_ups = best_by_completed_follow_up
        .into_values()
        .collect::<Vec<_>>();
    completed_follow_ups.sort_by(|left, right| {
        compare_balanced(&scored[*right], &scored[*left]).then_with(|| left.cmp(right))
    });

    // A resource conversion is not complete planning by itself. Preserve a
    // bounded sample of the first two exact follow-up actions so the beam does
    // not immediately forget the newly created hand/block/damage state. The
    // ordinary frontier width remains the hard cap, and pending conversions
    // stay interleaved with their realized continuations rather than being
    // displaced by a wide post-conversion action surface.
    let mut coverage = interleave_indexes(&pending_transformations, &completed_follow_ups);
    let mut seen = HashSet::new();
    coverage.retain(|index| seen.insert(*index));
    coverage
}

fn completed_resource_transformation_coverage_key(
    work: &TurnPlanWorkNode,
    root_action_len: usize,
) -> Option<CompletedResourceTransformationCoverageKey> {
    let actions = work.node.actions.get(root_action_len..)?;
    debug_assert_eq!(
        actions.len(),
        work.action_facts.len(),
        "turn-plan action facts align with actions added after the root"
    );
    if actions.len() != work.action_facts.len() {
        return None;
    }

    let transformation_index = work
        .action_facts
        .iter()
        .rposition(|facts| facts.mechanics.resource_timing.hand_resource_conversion)?;
    let first_follow_up_index = transformation_index.saturating_add(1);
    let follow_up_count = actions.len().saturating_sub(first_follow_up_index);
    if follow_up_count == 0 || follow_up_count > PARTIAL_RESOURCE_TRANSFORMATION_FOLLOW_UP_HORIZON {
        return None;
    }

    Some(CompletedResourceTransformationCoverageKey {
        transformation_action_key: actions[transformation_index].action_key.clone(),
        first_follow_up_action_key: actions[first_follow_up_index].action_key.clone(),
        current_follow_up_action_key: actions.last()?.action_key.clone(),
    })
}

fn pending_choice_continuation_coverage(scored: &[ScoredWorkNode]) -> Vec<usize> {
    let mut best_by_action = HashMap::<&str, usize>::new();
    for (index, candidate) in scored.iter().enumerate() {
        let Some(action_key) = candidate.pending_choice_continuation_action_key.as_deref() else {
            continue;
        };
        let replace = best_by_action.get(action_key).is_none_or(|current| {
            compare_setup(candidate, &scored[*current])
                .then_with(|| compare_balanced(candidate, &scored[*current]))
                .is_gt()
        });
        if replace {
            best_by_action.insert(action_key, index);
        }
    }
    let mut coverage = best_by_action.into_values().collect::<Vec<_>>();
    coverage.sort_by(|left, right| {
        compare_setup(&scored[*right], &scored[*left])
            .then_with(|| compare_balanced(&scored[*right], &scored[*left]))
            .then_with(|| left.cmp(right))
    });
    coverage
}

fn power_installation_coverage(scored: &[ScoredWorkNode]) -> Vec<usize> {
    let mut best_by_power_action = HashMap::<&str, usize>::new();
    for (index, candidate) in scored.iter().enumerate() {
        let installs_power = candidate
            .work
            .as_ref()
            .and_then(|work| work.action_facts.last())
            .and_then(|facts| facts.card.as_ref())
            .is_some_and(|card| card.card_type == crate::content::cards::CardType::Power);
        if !installs_power {
            continue;
        }
        let Some(action_key) = candidate
            .work
            .as_ref()
            .and_then(|work| work.node.actions.last())
            .map(|action| action.action_key.as_str())
        else {
            continue;
        };
        let replace = best_by_power_action.get(action_key).is_none_or(|current| {
            compare_survival(candidate, &scored[*current])
                .then_with(|| compare_setup(candidate, &scored[*current]))
                .then_with(|| compare_balanced(candidate, &scored[*current]))
                .is_gt()
        });
        if replace {
            best_by_power_action.insert(action_key, index);
        }
    }
    let mut coverage = best_by_power_action.into_values().collect::<Vec<_>>();
    coverage.sort_by(|left, right| {
        compare_survival(&scored[*right], &scored[*left])
            .then_with(|| compare_setup(&scored[*right], &scored[*left]))
            .then_with(|| compare_balanced(&scored[*right], &scored[*left]))
            .then_with(|| left.cmp(right))
    });
    coverage
}

fn interleave_indexes(left: &[usize], right: &[usize]) -> Vec<usize> {
    let mut interleaved = Vec::with_capacity(left.len().saturating_add(right.len()));
    let maximum_depth = left.len().max(right.len());
    for rank in 0..maximum_depth {
        if let Some(index) = left.get(rank) {
            interleaved.push(*index);
        }
        if let Some(index) = right.get(rank) {
            interleaved.push(*index);
        }
    }
    interleaved
}

fn partial_pending_choice_selection_score(
    facts: &CombatSearchV2ActionPendingChoiceSelectionFacts,
) -> PartialPendingChoiceSelectionScore {
    let role_rank = match facts.role {
        CombatSearchV2ActionPendingChoiceSelectionRole::Value => 3,
        CombatSearchV2ActionPendingChoiceSelectionRole::Removal => 2,
        CombatSearchV2ActionPendingChoiceSelectionRole::Neutral => 1,
        CombatSearchV2ActionPendingChoiceSelectionRole::Cancel => 0,
    };
    PartialPendingChoiceSelectionScore {
        role_rank,
        primary: facts.primary,
        secondary: facts.secondary,
        selected_count_tiebreak: facts.selected_count_tiebreak,
    }
}

/// Keep the strongest retained pending-choice parent's alternatives adjacent
/// before moving to weaker parents. The caller's bounded quota may still stop
/// within a large surface. Flattening every child into one global value ranking can
/// spend the entire bounded lane on repeated high-ranked choices from many
/// parents, while never preserving the lower-ranked alternative of any one
/// parent. That makes retaining the parent surface illusory. Grouping by the
/// complete action prefix keeps each transaction local; the choice owner still
/// orders alternatives inside that transaction without turning its components
/// into cardinal combat value.
fn pending_choice_selection_coverage(scored: &[ScoredWorkNode]) -> Vec<usize> {
    let mut best_by_parent_and_action = BTreeMap::<Vec<String>, BTreeMap<String, usize>>::new();
    for (index, candidate) in scored.iter().enumerate() {
        let (Some(parent_key), Some(selection_key), Some(selection)) = (
            candidate.pending_choice_selection_parent_key.as_ref(),
            candidate.pending_choice_selection_action_key.as_ref(),
            candidate.pending_choice_selection,
        ) else {
            continue;
        };
        let choices = best_by_parent_and_action
            .entry(parent_key.clone())
            .or_default();
        let replace = choices.get(selection_key).is_none_or(|current| {
            let current = &scored[*current];
            selection
                .cmp(
                    &current
                        .pending_choice_selection
                        .expect("selection coverage entries have selection facts"),
                )
                .then_with(|| compare_setup(candidate, current))
                .then_with(|| compare_balanced(candidate, current))
                .is_gt()
        });
        if replace {
            choices.insert(selection_key.clone(), index);
        }
    }
    let mut parents = best_by_parent_and_action
        .into_iter()
        .map(|(parent_key, choices)| {
            let mut choices = choices.into_values().collect::<Vec<_>>();
            choices.sort_by(|left, right| {
                scored[*right]
                    .pending_choice_selection
                    .cmp(&scored[*left].pending_choice_selection)
                    .then_with(|| compare_setup(&scored[*right], &scored[*left]))
                    .then_with(|| compare_balanced(&scored[*right], &scored[*left]))
                    .then_with(|| left.cmp(right))
            });
            (parent_key, choices)
        })
        .collect::<Vec<_>>();
    parents.sort_by(|(left_key, left), (right_key, right)| {
        let left = left[0];
        let right = right[0];
        compare_setup(&scored[right], &scored[left])
            .then_with(|| compare_balanced(&scored[right], &scored[left]))
            .then_with(|| left_key.cmp(right_key))
    });
    parents
        .into_iter()
        .flat_map(|(_, choices)| choices)
        .collect()
}

fn guide_components(scored: &ScoredWorkNode, lane: u32) -> Option<&Vec<i32>> {
    scored
        .guides
        .iter()
        .find(|guide| guide.lane == lane)
        .map(|guide| &guide.components)
}

fn partial_action_score(work: &TurnPlanWorkNode) -> PartialActionScore {
    let mut score = PartialActionScore {
        durable_player_strength: 0,
        recurring_turn_energy: 0,
        attack_replay_charges: 0,
        installed_power_count: 0,
        future_attack_damage_percent_gain: 0,
        retained_next_turn_block: 0,
        sustained_mitigation: 0,
        visible_mitigation: 0,
        debuff_setup: 0,
        progress_hint: 0,
        focused_target_progress: 0,
        access_gain: 0,
        resource_timing: 0,
        reactive_safety: 0,
    };
    let mut target_progress = HashMap::<usize, i32>::new();
    for facts in &work.action_facts {
        let declared_durable_strength = facts
            .mechanics
            .direct
            .player_strength_gain
            .saturating_sub(facts.mechanics.direct.player_temporary_strength_gain);
        // EndTurn's one-step delta spans the enemy phase and next turn start;
        // keep reactive relic/power changes out of the action-setup lane.
        let exact_durable_strength = if facts.action_kind == "end_turn" {
            0
        } else {
            facts
                .exact_one_step_delta
                .player_strength_delta
                .saturating_sub(facts.mechanics.direct.player_temporary_strength_gain)
        };
        score.durable_player_strength = score
            .durable_player_strength
            .saturating_add(declared_durable_strength.max(exact_durable_strength).max(0));
        score.recurring_turn_energy = score
            .recurring_turn_energy
            .saturating_add(facts.mechanics.direct.player_recurring_turn_energy_delta);
        score.attack_replay_charges = score
            .attack_replay_charges
            .saturating_add(facts.mechanics.direct.player_attack_replay_charges);
        score.installed_power_count = score.installed_power_count.saturating_add(i32::from(
            facts
                .card
                .as_ref()
                .is_some_and(|card| card.card_type == crate::content::cards::CardType::Power),
        ));
        score.future_attack_damage_percent_gain =
            score.future_attack_damage_percent_gain.saturating_add(
                facts
                    .mechanics
                    .reactive
                    .future_attack_damage_percent_gain_hint,
            );
        score.sustained_mitigation = score
            .sustained_mitigation
            .saturating_add(facts.mechanics.direct.persistent_enemy_strength_down);
        score.visible_mitigation = score
            .visible_mitigation
            .saturating_add(facts.mechanics.direct.temporary_enemy_strength_down)
            .saturating_add(facts.mechanics.direct.visible_attack_mitigation_hint)
            .saturating_add(facts.mechanics.derived.enemy_weak);
        score.debuff_setup = score
            .debuff_setup
            .saturating_add(facts.mechanics.derived.enemy_vulnerable);
        score.progress_hint = score
            .progress_hint
            .saturating_add(
                facts
                    .immediate
                    .target_progress_hint
                    .max(facts.immediate.all_enemy_progress_hint),
            )
            .saturating_add(facts.mechanics.reactive.enemy_damage);
        if let Some(target) = facts.target.as_ref() {
            let progress = facts
                .immediate
                .target_progress_hint
                .saturating_add(facts.mechanics.reactive.enemy_damage);
            let accumulated = target_progress.entry(target.target_slot).or_default();
            *accumulated = accumulated.saturating_add(progress);
        }
        score.access_gain = score.access_gain.saturating_add(
            facts
                .exact_one_step_delta
                .hand_delta
                .saturating_add(1)
                .max(0)
                .saturating_add(facts.exact_one_step_delta.energy_delta.max(0)),
        );
        score.resource_timing = score
            .resource_timing
            .saturating_add(facts.mechanics.resource_timing.ordering_score);
        score.reactive_safety = score.reactive_safety.saturating_sub(
            facts
                .mechanics
                .derived
                .enemy_strength_gain
                .saturating_add(facts.mechanics.derived.visible_attack_pressure_hint)
                .saturating_add(facts.mechanics.reactive.player_hp_loss)
                .saturating_add(facts.mechanics.reactive.bad_draw_cards)
                .saturating_add(i32::from(facts.mechanics.reactive.forced_turn_end)),
        );
    }
    score.focused_target_progress = target_progress.values().copied().max().unwrap_or_default();
    score
}

fn projected_retained_next_turn_block(combat: &crate::runtime::combat::CombatState) -> i32 {
    let remaining_after_visible_attacks = combat
        .entities
        .player
        .block
        .saturating_sub(crate::ai::combat_search_v2::visible_incoming_damage(combat))
        .max(0);
    if crate::content::powers::store::has_power(
        combat,
        combat.entities.player.id,
        crate::content::powers::PowerId::Barricade,
    ) || crate::content::powers::store::has_power(
        combat,
        combat.entities.player.id,
        crate::content::powers::PowerId::Blur,
    ) {
        return remaining_after_visible_attacks;
    }
    if combat
        .entities
        .player
        .relic_buses
        .on_calculate_block_retained
        .is_empty()
    {
        return 0;
    }
    crate::content::relics::hooks::on_calculate_block_retained(
        combat,
        remaining_after_visible_attacks,
    )
}

fn ordered_indexes(
    scored: &[ScoredWorkNode],
    compare: fn(&ScoredWorkNode, &ScoredWorkNode) -> Ordering,
) -> Vec<usize> {
    let mut indexes = (0..scored.len()).collect::<Vec<_>>();
    indexes.sort_by(|left, right| {
        compare(&scored[*right], &scored[*left]).then_with(|| left.cmp(right))
    });
    indexes
}

fn ranks_for_order(order: &[usize], candidate_count: usize) -> Vec<usize> {
    let mut ranks = vec![usize::MAX; candidate_count];
    for (rank, index) in order.iter().copied().enumerate() {
        ranks[index] = rank;
    }
    debug_assert!(ranks.iter().all(|rank| *rank != usize::MAX));
    ranks
}

fn optional_ranks_for_order(order: &[usize], candidate_count: usize) -> Vec<Option<usize>> {
    let mut ranks = vec![None; candidate_count];
    for (rank, index) in order.iter().copied().enumerate() {
        ranks[index] = Some(rank);
    }
    ranks
}

fn first_action_ranks(scored: &[ScoredWorkNode], balanced_order: &[usize]) -> Vec<usize> {
    let mut next_rank = HashMap::<&str, usize>::new();
    let mut ranks = vec![usize::MAX; scored.len()];
    for index in balanced_order.iter().copied() {
        let key = scored[index].first_action_key.as_str();
        let rank = next_rank.entry(key).or_default();
        ranks[index] = *rank;
        *rank = rank.saturating_add(1);
    }
    debug_assert!(ranks.iter().all(|rank| *rank != usize::MAX));
    ranks
}

fn guide_conditioned_ranks(
    scored: &[ScoredWorkNode],
) -> Vec<Vec<CombatSearchV2TurnPlanPartialGuideConditionedAuditV1>> {
    let mut groups = BTreeMap::<(u32, Vec<i32>), Vec<usize>>::new();
    for (index, candidate) in scored.iter().enumerate() {
        for guide in &candidate.guides {
            groups
                .entry((guide.lane, guide.components.clone()))
                .or_default()
                .push(index);
        }
    }

    let mut audits = vec![Vec::new(); scored.len()];
    for ((lane, _components), mut peers) in groups {
        peers.sort_unstable();
        peers.dedup();
        let survival = ordered_subset(scored, &peers, compare_survival);
        let progress = ordered_subset(scored, &peers, compare_progress);
        let setup = ordered_subset(scored, &peers, compare_setup);
        let balanced = ordered_subset(scored, &peers, compare_balanced);
        let survival_ranks = ranks_for_subset(&survival, scored.len());
        let progress_ranks = ranks_for_subset(&progress, scored.len());
        let setup_ranks = ranks_for_subset(&setup, scored.len());
        let balanced_ranks = ranks_for_subset(&balanced, scored.len());
        for index in peers.iter().copied() {
            audits[index].push(CombatSearchV2TurnPlanPartialGuideConditionedAuditV1 {
                lane,
                peer_count: peers.len(),
                survival_rank: survival_ranks[index],
                progress_rank: progress_ranks[index],
                setup_rank: setup_ranks[index],
                balanced_rank: balanced_ranks[index],
            });
        }
    }
    audits
}

fn ordered_subset(
    scored: &[ScoredWorkNode],
    candidates: &[usize],
    compare: fn(&ScoredWorkNode, &ScoredWorkNode) -> Ordering,
) -> Vec<usize> {
    let mut indexes = candidates.to_vec();
    indexes.sort_by(|left, right| {
        compare(&scored[*right], &scored[*left]).then_with(|| left.cmp(right))
    });
    indexes
}

fn ranks_for_subset(order: &[usize], candidate_count: usize) -> Vec<usize> {
    let mut ranks = vec![usize::MAX; candidate_count];
    for (rank, index) in order.iter().copied().enumerate() {
        ranks[index] = rank;
    }
    ranks
}

fn compare_balanced(left: &ScoredWorkNode, right: &ScoredWorkNode) -> Ordering {
    left.eval
        .cmp(&right.eval)
        .then_with(|| left.action.cmp(&right.action))
}

fn compare_survival(left: &ScoredWorkNode, right: &ScoredWorkNode) -> Ordering {
    left.eval
        .outcome_class()
        .cmp(&right.eval.outcome_class())
        .then_with(|| {
            left.eval
                .survival_bucket()
                .cmp(&right.eval.survival_bucket())
        })
        .then_with(|| left.eval.risk_margin().cmp(&right.eval.risk_margin()))
        .then_with(|| left.living_enemy_score.cmp(&right.living_enemy_score))
        .then_with(|| {
            left.action
                .sustained_mitigation
                .cmp(&right.action.sustained_mitigation)
        })
        .then_with(|| {
            left.action
                .visible_mitigation
                .cmp(&right.action.visible_mitigation)
        })
        .then_with(|| left.eval.cmp(&right.eval))
}

fn compare_progress(left: &ScoredWorkNode, right: &ScoredWorkNode) -> Ordering {
    left.eval
        .outcome_class()
        .cmp(&right.eval.outcome_class())
        .then_with(|| {
            left.eval
                .progress_bucket()
                .cmp(&right.eval.progress_bucket())
        })
        .then_with(|| left.living_enemy_score.cmp(&right.living_enemy_score))
        .then_with(|| {
            left.action
                .focused_target_progress
                .cmp(&right.action.focused_target_progress)
        })
        .then_with(|| left.eval.enemy_progress().cmp(&right.eval.enemy_progress()))
        .then_with(|| left.action.progress_hint.cmp(&right.action.progress_hint))
        .then_with(|| {
            left.eval
                .survival_bucket()
                .cmp(&right.eval.survival_bucket())
        })
        .then_with(|| left.eval.risk_margin().cmp(&right.eval.risk_margin()))
        .then_with(|| left.eval.cmp(&right.eval))
}

fn compare_setup(left: &ScoredWorkNode, right: &ScoredWorkNode) -> Ordering {
    left.eval
        .outcome_class()
        .cmp(&right.eval.outcome_class())
        .then_with(|| {
            left.action
                .durable_player_strength
                .cmp(&right.action.durable_player_strength)
        })
        .then_with(|| {
            left.action
                .recurring_turn_energy
                .cmp(&right.action.recurring_turn_energy)
        })
        .then_with(|| {
            left.action
                .attack_replay_charges
                .cmp(&right.action.attack_replay_charges)
        })
        .then_with(|| {
            left.action
                .installed_power_count
                .cmp(&right.action.installed_power_count)
        })
        .then_with(|| {
            left.action
                .future_attack_damage_percent_gain
                .cmp(&right.action.future_attack_damage_percent_gain)
        })
        .then_with(|| {
            left.action
                .retained_next_turn_block
                .cmp(&right.action.retained_next_turn_block)
        })
        .then_with(|| left.action.access_gain.cmp(&right.action.access_gain))
        .then_with(|| {
            left.action
                .sustained_mitigation
                .cmp(&right.action.sustained_mitigation)
        })
        .then_with(|| {
            left.action
                .visible_mitigation
                .cmp(&right.action.visible_mitigation)
        })
        .then_with(|| left.action.debuff_setup.cmp(&right.action.debuff_setup))
        .then_with(|| {
            left.action
                .resource_timing
                .cmp(&right.action.resource_timing)
        })
        .then_with(|| {
            left.action
                .reactive_safety
                .cmp(&right.action.reactive_safety)
        })
        .then_with(|| left.eval.cmp(&right.eval))
}

fn select_index(
    index: usize,
    reason: CombatSearchV2TurnPlanPartialRetentionLaneV1,
    selected: &mut Vec<usize>,
    selected_flags: &mut [bool],
    selected_by: &mut [Option<CombatSearchV2TurnPlanPartialRetentionLaneV1>],
    width: usize,
) {
    if selected.len() >= width || selected_flags[index] {
        return;
    }
    selected_flags[index] = true;
    selected_by[index] = Some(reason);
    selected.push(index);
}
