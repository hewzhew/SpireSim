use std::cmp::Ordering;

use crate::ai::combat_search_v2::turn_planner::types::TurnPlanV1;
use crate::ai::combat_search_v2::value::CombatEvalSurvivalBucket;
use crate::ai::combat_search_v2::*;

pub(super) fn compare_turn_plan_seed_candidate(
    left: &TurnPlanV1,
    right: &TurnPlanV1,
    prior_state_hash: Option<&str>,
    turn_plan_prior: Option<&CombatSearchV2TurnPlanPrior>,
) -> Ordering {
    left.eval
        .outcome_class()
        .cmp(&right.eval.outcome_class())
        .then_with(|| {
            left.eval
                .progress_bucket()
                .cmp(&right.eval.progress_bucket())
        })
        .then_with(|| living_enemy_score(left).cmp(&living_enemy_score(right)))
        .then_with(|| left.eval.enemy_progress().cmp(&right.eval.enemy_progress()))
        .then_with(|| {
            left.eval
                .survival_bucket()
                .cmp(&right.eval.survival_bucket())
        })
        .then_with(|| {
            if turn_plan_is_in_danger(left) || turn_plan_is_in_danger(right) {
                left.eval.risk_margin().cmp(&right.eval.risk_margin())
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .then_with(|| turn_plan_seed_conservation(left).cmp(&turn_plan_seed_conservation(right)))
        .then_with(|| {
            compare_prior_scores(
                turn_plan_prior_score(left, prior_state_hash, turn_plan_prior),
                turn_plan_prior_score(right, prior_state_hash, turn_plan_prior),
            )
        })
        .then_with(|| {
            if turn_plan_is_in_danger(left) || turn_plan_is_in_danger(right) {
                Ordering::Equal
            } else {
                left.eval.risk_margin().cmp(&right.eval.risk_margin())
            }
        })
        .then_with(|| left.eval.final_hp().cmp(&right.eval.final_hp()))
        .then_with(|| left.eval.cmp(&right.eval))
}

fn living_enemy_score(plan: &TurnPlanV1) -> i32 {
    -(plan
        .end_node
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count() as i32)
}

pub(super) fn count_turn_plan_prior_scored_plans(
    plans: &[TurnPlanV1],
    prior_state_hash: Option<&str>,
    turn_plan_prior: Option<&CombatSearchV2TurnPlanPrior>,
) -> usize {
    plans
        .iter()
        .filter(|plan| turn_plan_prior_score(plan, prior_state_hash, turn_plan_prior).is_some())
        .count()
}

fn turn_plan_prior_score(
    plan: &TurnPlanV1,
    prior_state_hash: Option<&str>,
    turn_plan_prior: Option<&CombatSearchV2TurnPlanPrior>,
) -> Option<f64> {
    let state_hash = prior_state_hash?;
    let prior = turn_plan_prior?;
    let action_keys = plan
        .actions
        .iter()
        .map(|action| action.action_key.clone())
        .collect::<Vec<_>>();
    prior.score_for_action_keys(state_hash, &action_keys)
}

fn compare_prior_scores(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.partial_cmp(&right).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

fn turn_plan_is_in_danger(plan: &TurnPlanV1) -> bool {
    matches!(
        plan.eval.survival_bucket(),
        CombatEvalSurvivalBucket::DeadOrForcedLoss
            | CombatEvalSurvivalBucket::LethalVisible
            | CombatEvalSurvivalBucket::Critical
    )
}

fn turn_plan_seed_conservation(plan: &TurnPlanV1) -> (i32, i32, i32) {
    (
        plan.action_facts
            .iter()
            .map(|facts| facts.mechanics.resource_timing.ordering_score)
            .sum::<i32>(),
        -(low_impact_exhaust_action_count(plan) as i32),
        -(plan.actions.len() as i32),
    )
}

fn low_impact_exhaust_action_count(plan: &TurnPlanV1) -> usize {
    plan.action_facts
        .iter()
        .filter(|facts| {
            facts.immediate.exhausts_card
                && facts.immediate.damage_hint <= 0
                && facts.immediate.block_hint <= 0
                && facts.immediate.target_progress_hint <= 0
                && facts.immediate.all_enemy_progress_hint <= 0
                && facts.mechanics.direct.visible_attack_mitigation_hint <= 0
                && facts.mechanics.direct.persistent_enemy_strength_down <= 0
                && facts.mechanics.direct.temporary_enemy_strength_down <= 0
                && facts.mechanics.derived.enemy_vulnerable <= 0
                && facts.mechanics.derived.enemy_weak <= 0
                && facts.mechanics.direct.player_strength_gain <= 0
                && facts.mechanics.direct.player_temporary_strength_gain <= 0
                && facts.exact_one_step_delta.energy_delta <= 0
                && facts.exact_one_step_delta.hand_delta <= 0
        })
        .count()
}
