//! Combat-plan policy owned by the strategic-owner crate.

use crate::agent::information::run::PublicCombatRunContextV1;
use crate::agent::task::{
    task_research_plan_non_dominated_candidate_ordinals_v1,
    task_research_plan_unique_shortest_exact_outcome_candidate_v1,
    TaskCombatResearchPlanRunCommitmentV1, TaskCombatResearchPlanSelectionBasisV1,
    TaskCombatResearchPlanSelectionOutcomeV1, TaskCombatResearchPlanSelectionUnavailableV1,
    TaskCommittedRouteNodeV1, TaskPrivilegedCombatResearchSupportV1, TaskPrivilegedSupportAccessV1,
    TaskPrivilegedSupportProvenanceV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskResearchPlanAttemptV1, TaskResearchPlanCandidateV1, TaskResearchPlanOutcomeV1,
    TaskResearchPlanPendingRewardV1, TaskRetainedEntryPotionV1,
};
use crate::ai::planner_core::{PlannerMapRoom, PlannerPublicMap};
use crate::content::potions::PotionId;
use crate::content::relics::persistent_state::{
    persistent_relic_counter_semantics_v1, PersistentRelicCounterSemanticsV1,
};
use crate::content::relics::{RelicId, RelicState};
use crate::engine::campfire_handler::campfire_rest_heal_capacity;
use crate::eval::run_control::campfire_rest_is_efficient_recovery_v1;

pub const PRIVILEGED_COMBAT_PLAN_OWNER_V1: &str = "privileged_combat_plan_run_resource_owner";
pub const PRIVILEGED_COMBAT_PLAN_OWNER_REVISION_V1: u32 = 7;

/// Applies the run-level reference policy to a replay-verified combat
/// frontier. Search producers never call this function and never receive its
/// selection as a score or generation preference.
pub fn privileged_combat_research_support_v1(
    public_root: &TaskPublicDecisionV1,
    attempt: TaskResearchPlanAttemptV1,
) -> Result<TaskPrivilegedCombatResearchSupportV1, String> {
    public_root.validate()?;
    let TaskPublicObservationV1::Combat {
        run_context,
        observation,
    } = &public_root.observation
    else {
        return Err("combat plan owner requires a public combat root".to_string());
    };
    attempt.validate()?;
    for candidate in &attempt.candidates {
        candidate.plan.validate_for_root(public_root)?;
    }
    let non_dominated_candidate_ordinals =
        task_research_plan_non_dominated_candidate_ordinals_v1(&attempt.candidates);
    let selection = select_combat_research_candidate_v1(
        &attempt.candidates,
        &non_dominated_candidate_ordinals,
        run_context,
        &observation
            .potions
            .iter()
            .map(|potion| potion.as_ref().map(|potion| potion.potion_id))
            .collect::<Vec<_>>(),
        &observation.player.relics,
    );
    Ok(TaskPrivilegedCombatResearchSupportV1 {
        root_public_snapshot_id: public_root.information.snapshot_id.clone(),
        selection_provenance: TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactStrategicOwner,
            producer_name: PRIVILEGED_COMBAT_PLAN_OWNER_V1.to_string(),
            producer_revision: PRIVILEGED_COMBAT_PLAN_OWNER_REVISION_V1,
        },
        attempt,
        non_dominated_candidate_ordinals,
        selection,
    })
}

fn select_combat_research_candidate_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    non_dominated_candidate_ordinals: &[usize],
    run_context: &PublicCombatRunContextV1,
    entry_potions: &[Option<PotionId>],
    entry_relics: &[RelicState],
) -> TaskCombatResearchPlanSelectionOutcomeV1 {
    match candidates {
        [] => {
            return TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
                reason: TaskCombatResearchPlanSelectionUnavailableV1::NoCandidate,
            }
        }
        [_] => {
            return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 0,
                basis: TaskCombatResearchPlanSelectionBasisV1::SingleCandidate,
                run_commitment: None,
            }
        }
        _ => {}
    }
    if let [candidate_ordinal] = non_dominated_candidate_ordinals {
        return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
            candidate_ordinal: *candidate_ordinal,
            basis: TaskCombatResearchPlanSelectionBasisV1::UniqueNonDominatedCandidate,
            run_commitment: None,
        };
    }
    if let Some(candidate_ordinal) = task_research_plan_unique_shortest_exact_outcome_candidate_v1(
        candidates,
        non_dominated_candidate_ordinals,
    ) {
        return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
            candidate_ordinal,
            basis: TaskCombatResearchPlanSelectionBasisV1::UniqueShortestExactOutcome,
            run_commitment: None,
        };
    }
    if let Some(candidate_ordinal) =
        select_equal_outcome_plan_order_preference_v1(candidates, non_dominated_candidate_ordinals)
    {
        return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
            candidate_ordinal,
            basis: TaskCombatResearchPlanSelectionBasisV1::EqualOutcomePlanOrderPreference,
            run_commitment: None,
        };
    }
    if let PublicCombatRunContextV1::Available {
        combat_victory_guarantees_full_heal,
        ..
    } = run_context
    {
        if *combat_victory_guarantees_full_heal {
            if let Some(candidate_ordinal) =
                select_guaranteed_full_heal_resource_policy_dominator_v1(
                    candidates,
                    non_dominated_candidate_ordinals,
                )
            {
                return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                    candidate_ordinal,
                    basis: TaskCombatResearchPlanSelectionBasisV1::GuaranteedFullHealResourceParetoPreference,
                    run_commitment: None,
                };
            }
        }
        if let Some(candidate_ordinal) = select_reference_resource_policy_dominator_v1(
            candidates,
            non_dominated_candidate_ordinals,
        ) {
            return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal,
                basis:
                    TaskCombatResearchPlanSelectionBasisV1::CarryoverHpBandResourceParetoPreference,
                run_commitment: None,
            };
        }
        if let Some(candidate_ordinal) = select_rewarded_exit_resource_policy_dominator_v1(
            candidates,
            non_dominated_candidate_ordinals,
        ) {
            return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal,
                basis: TaskCombatResearchPlanSelectionBasisV1::RewardedExitResourceParetoPreference,
                run_commitment: None,
            };
        }
        if let Some(candidate_ordinal) = select_full_hp_over_unprimed_cyclic_progress_v1(
            candidates,
            non_dominated_candidate_ordinals,
        ) {
            return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal,
                basis: TaskCombatResearchPlanSelectionBasisV1::FullHpOverUnprimedCyclicProgress,
                run_commitment: None,
            };
        }
        if let Some(candidate_ordinal) = select_forced_campfire_marginal_hp_potion_retention_v1(
            candidates,
            non_dominated_candidate_ordinals,
            run_context,
            entry_relics,
        ) {
            return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal,
                basis:
                    TaskCombatResearchPlanSelectionBasisV1::ForcedCampfireMarginalHpPotionRetention,
                run_commitment: None,
            };
        }
        if let Some((candidate_ordinal, run_commitment)) =
            select_immediate_campfire_recovery_potion_retention_v1(
                candidates,
                non_dominated_candidate_ordinals,
                run_context,
                entry_potions,
                entry_relics,
            )
        {
            return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal,
                basis:
                    TaskCombatResearchPlanSelectionBasisV1::ImmediateCampfireRecoveryPotionRetention,
                run_commitment: Some(run_commitment),
            };
        }
        if let Some((candidate_ordinal, run_commitment)) =
            select_noncombat_rest_corridor_resource_pareto_v1(
                candidates,
                non_dominated_candidate_ordinals,
                run_context,
                entry_potions,
            )
        {
            return TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal,
                basis: TaskCombatResearchPlanSelectionBasisV1::NonCombatRestCorridorResourceParetoPreference,
                run_commitment: Some(run_commitment),
            };
        }
    }
    TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
        reason: TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier,
    }
}

fn select_equal_outcome_plan_order_preference_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    frontier: &[usize],
) -> Option<usize> {
    let first = &candidates[*frontier.first()?];
    if frontier.len() < 2
        || frontier
            .iter()
            .any(|i| candidates[*i].outcome != first.outcome)
    {
        return None;
    }
    // A reference-policy tie, not an equivalence proof about hidden future
    // RNG or whole-run value. After exact outcome equality and shortest
    // length, order the retained, source-bound public choices themselves.
    // Producer name, discovery order, hashes and requested quality do not
    // participate. Never rebind the chosen plan's ordinals to another frame.
    let compare = |left: usize, right: usize| {
        let left = &candidates[left].plan.steps;
        let right = &candidates[right].plan.steps;
        left.len().cmp(&right.len()).then_with(|| {
            left.iter()
                .map(|step| &step.choice.ordinal_path)
                .cmp(right.iter().map(|step| &step.choice.ordinal_path))
        })
    };
    let selected = frontier
        .iter()
        .copied()
        .min_by(|left, right| compare(*left, *right))?;
    // Indistinguishable keys with different source frames remain unresolved.
    frontier
        .iter()
        .all(|other| *other == selected || compare(selected, *other).is_lt())
        .then_some(selected)
}

fn select_full_hp_over_unprimed_cyclic_progress_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    non_dominated_candidate_ordinals: &[usize],
) -> Option<usize> {
    let mut preferred =
        non_dominated_candidate_ordinals
            .iter()
            .copied()
            .filter(|candidate_ordinal| {
                non_dominated_candidate_ordinals
                    .iter()
                    .all(|other_ordinal| {
                        candidate_ordinal == other_ordinal
                            || full_hp_over_unprimed_cyclic_progress_v1(
                                &candidates[*candidate_ordinal].outcome,
                                &candidates[*other_ordinal].outcome,
                            )
                    })
            });
    let selected = preferred.next()?;
    preferred.next().is_none().then_some(selected)
}

fn full_hp_over_unprimed_cyclic_progress_v1(
    left: &TaskResearchPlanOutcomeV1,
    right: &TaskResearchPlanOutcomeV1,
) -> bool {
    left.final_hp >= left.persistent.max_hp
        && right.final_hp < right.persistent.max_hp
        && left.claimable_gold_and_pending_rewards_equivalent_v1(right)
        && left.persistent == right.persistent
        && non_entry_potions_consumed_v1(left) == non_entry_potions_consumed_v1(right)
        && left
            .consumed_entry_potion_slots
            .iter()
            .all(|slot| right.consumed_entry_potion_slots.contains(slot))
        && left.total_potions_consumed <= right.total_potions_consumed
        && left.generated_potions == right.generated_potions
        && left.meta_changes == right.meta_changes
        && left.combat_mugged == right.combat_mugged
        && relics_equal_or_unprimed_cyclic_disadvantage_v1(&left.final_relics, &right.final_relics)
}

fn relics_equal_or_unprimed_cyclic_disadvantage_v1(
    left: &[RelicState],
    right: &[RelicState],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            if left.id != right.id || left.used_up != right.used_up || left.amount != right.amount {
                return false;
            }
            match persistent_relic_counter_semantics_v1(left.id) {
                PersistentRelicCounterSemanticsV1::Exact => left.counter == right.counter,
                PersistentRelicCounterSemanticsV1::CyclicProgress { trigger_at } => {
                    (0..trigger_at).contains(&left.counter)
                        && (0..trigger_at).contains(&right.counter)
                        && (left.counter >= right.counter || right.counter < trigger_at - 1)
                }
            }
        })
}

fn select_immediate_campfire_recovery_potion_retention_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    non_dominated_candidate_ordinals: &[usize],
    run_context: &PublicCombatRunContextV1,
    entry_potions: &[Option<PotionId>],
    entry_relics: &[RelicState],
) -> Option<(usize, TaskCombatResearchPlanRunCommitmentV1)> {
    let PublicCombatRunContextV1::Available { public_map, .. } = run_context else {
        return None;
    };
    if entry_relics
        .iter()
        .any(|relic| matches!(relic.id, RelicId::CoffeeDripper | RelicId::MarkOfTheBloom))
    {
        return None;
    }
    let (destination_x, destination_y) = immediate_campfire_destination_v1(public_map)?;

    let mut preferred =
        non_dominated_candidate_ordinals
            .iter()
            .copied()
            .filter_map(|candidate_ordinal| {
                let candidate = &candidates[candidate_ordinal];
                let rest_heal =
                    campfire_rest_heal_capacity(candidate.outcome.persistent.max_hp, entry_relics);
                let hp_gain = candidate
                    .outcome
                    .persistent
                    .max_hp
                    .saturating_sub(candidate.outcome.final_hp)
                    .min(rest_heal);
                if !campfire_rest_is_efficient_recovery_v1(hp_gain, rest_heal) {
                    return None;
                }
                let mut retained_entry_potions = Vec::new();
                for other_ordinal in non_dominated_candidate_ordinals {
                    if *other_ordinal == candidate_ordinal {
                        continue;
                    }
                    retained_entry_potions.extend(recovery_backed_potion_retention_dominates_v1(
                        &candidate.outcome,
                        &candidates[*other_ordinal].outcome,
                        rest_heal,
                        entry_potions,
                    )?);
                }
                retained_entry_potions.sort_by_key(|potion| potion.slot);
                retained_entry_potions.dedup_by_key(|potion| potion.slot);
                (!retained_entry_potions.is_empty()).then_some((
                    candidate_ordinal,
                    TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest {
                        destination_x,
                        destination_y,
                        projected_hp_after_rest: candidate.outcome.persistent.max_hp,
                        retained_entry_potions,
                    },
                ))
            });
    let selected = preferred.next()?;
    preferred.next().is_none().then_some(selected)
}

fn select_forced_campfire_marginal_hp_potion_retention_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    non_dominated_candidate_ordinals: &[usize],
    run_context: &PublicCombatRunContextV1,
    entry_relics: &[RelicState],
) -> Option<usize> {
    let PublicCombatRunContextV1::Available { public_map, .. } = run_context else {
        return None;
    };
    forced_immediate_campfire_destination_v1(public_map)?;
    let mut preferred =
        non_dominated_candidate_ordinals
            .iter()
            .copied()
            .filter(|candidate_ordinal| {
                non_dominated_candidate_ordinals
                    .iter()
                    .all(|other_ordinal| {
                        candidate_ordinal == other_ordinal
                            || forced_campfire_marginal_hp_potion_retention_dominates_v1(
                                &candidates[*candidate_ordinal].outcome,
                                &candidates[*other_ordinal].outcome,
                                entry_relics,
                            )
                    })
            });
    let selected = preferred.next()?;
    preferred.next().is_none().then_some(selected)
}

fn forced_campfire_marginal_hp_potion_retention_dominates_v1(
    left: &TaskResearchPlanOutcomeV1,
    right: &TaskResearchPlanOutcomeV1,
    entry_relics: &[RelicState],
) -> bool {
    if !left.claimable_gold_and_pending_rewards_equivalent_v1(right)
        || left.persistent != right.persistent
        || non_entry_potions_consumed_v1(left) != non_entry_potions_consumed_v1(right)
        || left.generated_potions != right.generated_potions
        || left.final_relics != right.final_relics
        || left.meta_changes != right.meta_changes
        || left.combat_mugged != right.combat_mugged
        || left.final_hp >= right.final_hp
        || right.final_hp < right.persistent.max_hp
        || left.total_potions_consumed >= right.total_potions_consumed
        || !left
            .consumed_entry_potion_slots
            .iter()
            .all(|slot| right.consumed_entry_potion_slots.contains(slot))
    {
        return false;
    }
    let rest_heal = campfire_rest_heal_capacity(left.persistent.max_hp, entry_relics);
    let hp_gain = left
        .persistent
        .max_hp
        .saturating_sub(left.final_hp)
        .min(rest_heal);
    hp_gain > 0 && !campfire_rest_is_efficient_recovery_v1(hp_gain, rest_heal)
}

#[derive(Clone, Debug)]
struct NonCombatRestCorridorProjectionV1 {
    projected_hp_after_rest: i32,
    effective_hp: i32,
    effective_consumed_entry_potion_slots: Vec<usize>,
    retained_entry_potions: Vec<TaskRetainedEntryPotionV1>,
    recoverable_entry_potions: Vec<TaskRetainedEntryPotionV1>,
    required_pending_rewards: Vec<TaskResearchPlanPendingRewardV1>,
}

fn select_noncombat_rest_corridor_resource_pareto_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    non_dominated_candidate_ordinals: &[usize],
    run_context: &PublicCombatRunContextV1,
    entry_potions: &[Option<PotionId>],
) -> Option<(usize, TaskCombatResearchPlanRunCommitmentV1)> {
    let PublicCombatRunContextV1::Available { public_map, .. } = run_context else {
        return None;
    };
    let route = unique_shop_to_rest_corridor_v1(public_map)?;
    let shop_count = route.len().saturating_sub(1);
    let projections = non_dominated_candidate_ordinals
        .iter()
        .map(|candidate_ordinal| {
            project_noncombat_rest_corridor_v1(
                &candidates[*candidate_ordinal].outcome,
                shop_count,
                entry_potions,
            )
        })
        .collect::<Vec<_>>();
    let mut preferred = non_dominated_candidate_ordinals.iter().enumerate().filter(
        |(projection_index, candidate_ordinal)| {
            let Some(left_projection) = &projections[*projection_index] else {
                return false;
            };
            non_dominated_candidate_ordinals.iter().enumerate().all(
                |(other_projection_index, other_ordinal)| {
                    candidate_ordinal == &other_ordinal
                        || projections[other_projection_index].as_ref().is_some_and(
                            |right_projection| {
                                noncombat_rest_corridor_resource_dominates_v1(
                                    &candidates[**candidate_ordinal].outcome,
                                    left_projection,
                                    &candidates[*other_ordinal].outcome,
                                    right_projection,
                                )
                            },
                        )
                },
            )
        },
    );
    let (projection_index, candidate_ordinal) = preferred.next()?;
    if preferred.next().is_some() {
        return None;
    }
    let projection = projections[projection_index].as_ref()?;
    Some((
        *candidate_ordinal,
        TaskCombatResearchPlanRunCommitmentV1::NonCombatRestCorridor {
            route,
            projected_hp_after_rest: projection.projected_hp_after_rest,
            required_pending_rewards: projection.required_pending_rewards.clone(),
            retained_entry_potions: projection.retained_entry_potions.clone(),
            recoverable_entry_potions: projection.recoverable_entry_potions.clone(),
        },
    ))
}

fn unique_shop_to_rest_corridor_v1(
    public_map: &PlannerPublicMap,
) -> Option<Vec<TaskCommittedRouteNodeV1>> {
    let current = public_map
        .nodes
        .iter()
        .find(|node| node.x == public_map.current_x && node.y == public_map.current_y)?;
    let mut corridors = Vec::new();
    for edge in &current.edges {
        collect_shop_to_rest_corridors_v1(
            public_map,
            edge.destination_x,
            edge.destination_y,
            &mut Vec::new(),
            &mut corridors,
        );
    }
    let corridor = corridors.pop()?;
    corridors.is_empty().then_some(corridor)
}

fn collect_shop_to_rest_corridors_v1(
    public_map: &PlannerPublicMap,
    x: i32,
    y: i32,
    prefix: &mut Vec<TaskCommittedRouteNodeV1>,
    corridors: &mut Vec<Vec<TaskCommittedRouteNodeV1>>,
) {
    if prefix.len() >= public_map.nodes.len() {
        return;
    }
    let Some(node) = public_map
        .nodes
        .iter()
        .find(|node| node.x == x && node.y == y)
    else {
        return;
    };
    prefix.push(TaskCommittedRouteNodeV1 { x, y });
    match node.room {
        PlannerMapRoom::Rest if prefix.len() >= 2 => corridors.push(prefix.clone()),
        PlannerMapRoom::Shop => {
            for edge in &node.edges {
                collect_shop_to_rest_corridors_v1(
                    public_map,
                    edge.destination_x,
                    edge.destination_y,
                    prefix,
                    corridors,
                );
            }
        }
        _ => {}
    }
    prefix.pop();
}

fn project_noncombat_rest_corridor_v1(
    outcome: &TaskResearchPlanOutcomeV1,
    shop_count: usize,
    entry_potions: &[Option<PotionId>],
) -> Option<NonCombatRestCorridorProjectionV1> {
    if outcome
        .final_relics
        .iter()
        .any(|relic| matches!(relic.id, RelicId::CoffeeDripper | RelicId::MarkOfTheBloom))
    {
        return None;
    }
    let max_hp = outcome.persistent.max_hp.max(1);
    let mut hp = outcome.final_hp.clamp(0, max_hp);
    let mut required_pending_rewards = Vec::new();
    if outcome
        .final_relics
        .iter()
        .any(|relic| relic.id == RelicId::BloodyIdol)
    {
        for reward in &outcome.pending_rewards {
            if matches!(reward, TaskResearchPlanPendingRewardV1::Gold { amount } if *amount > 0)
                && hp < max_hp
            {
                hp = hp.saturating_add(5).min(max_hp);
                required_pending_rewards.push(reward.clone());
            }
        }
    }
    if outcome
        .final_relics
        .iter()
        .any(|relic| relic.id == RelicId::MealTicket)
    {
        hp = hp
            .saturating_add(i32::try_from(shop_count).ok()?.saturating_mul(15))
            .min(max_hp);
    }
    let pending_regal_pillow = outcome.pending_rewards.iter().find(|reward| {
        matches!(
            reward,
            TaskResearchPlanPendingRewardV1::Relic {
                relic_id: RelicId::RegalPillow
            }
        )
    });
    let rest_heal = campfire_rest_heal_capacity(max_hp, &outcome.final_relics).saturating_add(
        if pending_regal_pillow.is_some()
            && !outcome
                .final_relics
                .iter()
                .any(|relic| relic.id == RelicId::RegalPillow)
        {
            15
        } else {
            0
        },
    );
    if let Some(reward) = pending_regal_pillow {
        required_pending_rewards.push(reward.clone());
    }
    hp = hp.saturating_add(rest_heal).min(max_hp);
    let projected_hp_after_rest = hp;

    let mut effective_consumed_entry_potion_slots = outcome.consumed_entry_potion_slots.clone();
    let mut recoverable_entry_potions = Vec::new();
    for (slot, potion) in entry_potions.iter().enumerate() {
        if hp >= max_hp || effective_consumed_entry_potion_slots.contains(&slot) {
            continue;
        }
        if *potion == Some(PotionId::BloodPotion) {
            let heal = max_hp.saturating_mul(20) / 100;
            hp = hp.saturating_add(heal).min(max_hp);
            effective_consumed_entry_potion_slots.push(slot);
            recoverable_entry_potions.push(TaskRetainedEntryPotionV1 {
                slot,
                potion: PotionId::BloodPotion,
            });
        }
    }
    if hp < max_hp || required_pending_rewards.is_empty() {
        return None;
    }
    effective_consumed_entry_potion_slots.sort_unstable();
    let retained_entry_potions = entry_potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            (!effective_consumed_entry_potion_slots.contains(&slot))
                .then(|| potion.map(|potion| TaskRetainedEntryPotionV1 { slot, potion }))
                .flatten()
        })
        .collect();
    Some(NonCombatRestCorridorProjectionV1 {
        projected_hp_after_rest,
        effective_hp: hp,
        effective_consumed_entry_potion_slots,
        retained_entry_potions,
        recoverable_entry_potions,
        required_pending_rewards,
    })
}

fn noncombat_rest_corridor_resource_dominates_v1(
    left: &TaskResearchPlanOutcomeV1,
    left_projection: &NonCombatRestCorridorProjectionV1,
    right: &TaskResearchPlanOutcomeV1,
    right_projection: &NonCombatRestCorridorProjectionV1,
) -> bool {
    if !left.claimable_gold_and_pending_rewards_equivalent_v1(right)
        || left.persistent != right.persistent
        || non_entry_potions_consumed_v1(left) != non_entry_potions_consumed_v1(right)
        || left.generated_potions != right.generated_potions
        || left.meta_changes != right.meta_changes
        || left.combat_mugged != right.combat_mugged
        || left_projection.effective_hp < right_projection.effective_hp
        || !left_projection
            .effective_consumed_entry_potion_slots
            .iter()
            .all(|slot| {
                right_projection
                    .effective_consumed_entry_potion_slots
                    .contains(slot)
            })
    {
        return false;
    }
    let left_total = left
        .total_potions_consumed
        .saturating_add(left_projection.recoverable_entry_potions.len() as u32);
    let right_total = right
        .total_potions_consumed
        .saturating_add(right_projection.recoverable_entry_potions.len() as u32);
    if left_total > right_total {
        return false;
    }
    let Some(relic_progress_strictly_better) =
        reference_relic_progress_dominates_or_equals_v1(&left.final_relics, &right.final_relics)
    else {
        return false;
    };
    left_projection.effective_hp > right_projection.effective_hp
        || left_projection.effective_consumed_entry_potion_slots
            != right_projection.effective_consumed_entry_potion_slots
        || left_total < right_total
        || relic_progress_strictly_better
}

fn immediate_campfire_destination_v1(
    public_map: &crate::ai::planner_core::PlannerPublicMap,
) -> Option<(i32, i32)> {
    let current = public_map
        .nodes
        .iter()
        .find(|node| node.x == public_map.current_x && node.y == public_map.current_y)?;
    let mut destinations = current.edges.iter().filter_map(|edge| {
        public_map
            .nodes
            .iter()
            .find(|node| node.x == edge.destination_x && node.y == edge.destination_y)
            .filter(|node| node.room == PlannerMapRoom::Rest)
            .map(|node| (node.x, node.y))
    });
    let destination = destinations.next()?;
    destinations.next().is_none().then_some(destination)
}

fn forced_immediate_campfire_destination_v1(
    public_map: &crate::ai::planner_core::PlannerPublicMap,
) -> Option<(i32, i32)> {
    let current = public_map
        .nodes
        .iter()
        .find(|node| node.x == public_map.current_x && node.y == public_map.current_y)?;
    let [edge] = current.edges.as_slice() else {
        return None;
    };
    public_map
        .nodes
        .iter()
        .find(|node| node.x == edge.destination_x && node.y == edge.destination_y)
        .filter(|node| node.room == PlannerMapRoom::Rest)
        .map(|node| (node.x, node.y))
}

fn recovery_backed_potion_retention_dominates_v1(
    left: &TaskResearchPlanOutcomeV1,
    right: &TaskResearchPlanOutcomeV1,
    rest_heal: i32,
    entry_potions: &[Option<PotionId>],
) -> Option<Vec<TaskRetainedEntryPotionV1>> {
    if !left.claimable_gold_and_pending_rewards_equivalent_v1(right)
        || left.persistent != right.persistent
        || non_entry_potions_consumed_v1(left) != non_entry_potions_consumed_v1(right)
        || left.generated_potions != right.generated_potions
        || left.meta_changes != right.meta_changes
        || left.combat_mugged != right.combat_mugged
        || left.total_potions_consumed >= right.total_potions_consumed
        || !left
            .consumed_entry_potion_slots
            .iter()
            .all(|slot| right.consumed_entry_potion_slots.contains(slot))
        || left.final_hp.saturating_add(rest_heal) < left.persistent.max_hp
        || right.final_hp.saturating_add(rest_heal) < right.persistent.max_hp
        || !relics_equal_or_shallow_cyclic_disadvantage_v1(&left.final_relics, &right.final_relics)
    {
        return None;
    }
    right
        .consumed_entry_potion_slots
        .iter()
        .filter(|slot| !left.consumed_entry_potion_slots.contains(slot))
        .map(|slot| {
            entry_potions
                .get(*slot)
                .copied()
                .flatten()
                .map(|potion| TaskRetainedEntryPotionV1 {
                    slot: *slot,
                    potion,
                })
        })
        .collect()
}

fn relics_equal_or_shallow_cyclic_disadvantage_v1(
    left: &[RelicState],
    right: &[RelicState],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            if left.id != right.id || left.used_up != right.used_up || left.amount != right.amount {
                return false;
            }
            match persistent_relic_counter_semantics_v1(left.id) {
                PersistentRelicCounterSemanticsV1::Exact => left.counter == right.counter,
                PersistentRelicCounterSemanticsV1::CyclicProgress { trigger_at } => {
                    (0..trigger_at).contains(&left.counter)
                        && (0..trigger_at).contains(&right.counter)
                        && right.counter.saturating_sub(left.counter) <= 1
                }
            }
        })
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum CombatCarryoverHpBandV1 {
    Critical,
    Damaged,
    Stable,
    Healthy,
    Full,
}

fn select_reference_resource_policy_dominator_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    non_dominated_candidate_ordinals: &[usize],
) -> Option<usize> {
    let mut preferred =
        non_dominated_candidate_ordinals
            .iter()
            .copied()
            .filter(|candidate_ordinal| {
                non_dominated_candidate_ordinals
                    .iter()
                    .all(|other_ordinal| {
                        candidate_ordinal == other_ordinal
                            || reference_resource_policy_dominates_v1(
                                &candidates[*candidate_ordinal].outcome,
                                &candidates[*other_ordinal].outcome,
                            )
                    })
            });
    let selected = preferred.next()?;
    preferred.next().is_none().then_some(selected)
}

fn select_guaranteed_full_heal_resource_policy_dominator_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    non_dominated_candidate_ordinals: &[usize],
) -> Option<usize> {
    let mut preferred =
        non_dominated_candidate_ordinals
            .iter()
            .copied()
            .filter(|candidate_ordinal| {
                non_dominated_candidate_ordinals
                    .iter()
                    .all(|other_ordinal| {
                        candidate_ordinal == other_ordinal
                            || guaranteed_full_heal_resource_policy_dominates_v1(
                                &candidates[*candidate_ordinal].outcome,
                                &candidates[*other_ordinal].outcome,
                            )
                    })
            });
    let selected = preferred.next()?;
    preferred.next().is_none().then_some(selected)
}

fn select_rewarded_exit_resource_policy_dominator_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    frontier: &[usize],
) -> Option<usize> {
    let mut preferred = frontier.iter().copied().filter(|ordinal| {
        let left = &candidates[*ordinal].outcome;
        !left.combat_mugged
            && frontier.iter().all(|other| {
                ordinal == other
                    || reference_resource_policy_dominates_v1(left, &candidates[*other].outcome)
                    || left.rewarded_exit_strictly_dominates_v1(&candidates[*other].outcome)
            })
    });
    let selected = preferred.next()?;
    preferred.next().is_none().then_some(selected)
}

fn reference_resource_policy_dominates_v1(
    left: &TaskResearchPlanOutcomeV1,
    right: &TaskResearchPlanOutcomeV1,
) -> bool {
    let Some(non_hp_resources_strictly_better) =
        reference_non_hp_resources_dominate_or_equal_v1(left, right)
    else {
        return false;
    };
    let left_hp_band = combat_carryover_hp_band_v1(left);
    let right_hp_band = combat_carryover_hp_band_v1(right);
    if left_hp_band < right_hp_band {
        return false;
    }
    left_hp_band > right_hp_band || non_hp_resources_strictly_better
}

fn guaranteed_full_heal_resource_policy_dominates_v1(
    left: &TaskResearchPlanOutcomeV1,
    right: &TaskResearchPlanOutcomeV1,
) -> bool {
    reference_non_hp_resources_dominate_or_equal_v1(left, right) == Some(true)
}

/// Returns whether the left outcome's resources that survive a guaranteed
/// full-heal transition dominate or equal the right outcome. `None` means the
/// outcomes remain incomparable; `Some(true)` records a strict improvement.
fn reference_non_hp_resources_dominate_or_equal_v1(
    left: &TaskResearchPlanOutcomeV1,
    right: &TaskResearchPlanOutcomeV1,
) -> Option<bool> {
    if !left.claimable_gold_and_pending_rewards_equivalent_v1(right)
        || left.persistent != right.persistent
        || non_entry_potions_consumed_v1(left) != non_entry_potions_consumed_v1(right)
        || left.generated_potions != right.generated_potions
        || left.meta_changes != right.meta_changes
        || left.combat_mugged != right.combat_mugged
    {
        return None;
    }
    if !left
        .consumed_entry_potion_slots
        .iter()
        .all(|slot| right.consumed_entry_potion_slots.contains(slot))
        || left.total_potions_consumed > right.total_potions_consumed
    {
        return None;
    }
    let relic_progress_strictly_better =
        reference_relic_progress_dominates_or_equals_v1(&left.final_relics, &right.final_relics)?;
    Some(
        left.consumed_entry_potion_slots != right.consumed_entry_potion_slots
            || left.total_potions_consumed < right.total_potions_consumed
            || relic_progress_strictly_better,
    )
}

fn non_entry_potions_consumed_v1(outcome: &TaskResearchPlanOutcomeV1) -> u32 {
    outcome
        .total_potions_consumed
        .saturating_sub(outcome.consumed_entry_potion_slots.len() as u32)
}

fn combat_carryover_hp_band_v1(outcome: &TaskResearchPlanOutcomeV1) -> CombatCarryoverHpBandV1 {
    let hp = i64::from(outcome.final_hp.max(0));
    let max_hp = i64::from(outcome.persistent.max_hp.max(1));
    if hp >= max_hp {
        CombatCarryoverHpBandV1::Full
    } else if hp.saturating_mul(4) >= max_hp.saturating_mul(3) {
        CombatCarryoverHpBandV1::Healthy
    } else if hp.saturating_mul(2) >= max_hp {
        CombatCarryoverHpBandV1::Stable
    } else if hp.saturating_mul(4) >= max_hp {
        CombatCarryoverHpBandV1::Damaged
    } else {
        CombatCarryoverHpBandV1::Critical
    }
}

fn reference_relic_progress_dominates_or_equals_v1(
    left: &[RelicState],
    right: &[RelicState],
) -> Option<bool> {
    if left.len() != right.len() {
        return None;
    }
    let mut strictly_better = false;
    for (left, right) in left.iter().zip(right) {
        if left.id != right.id || left.used_up != right.used_up || left.amount != right.amount {
            return None;
        }
        match persistent_relic_counter_semantics_v1(left.id) {
            PersistentRelicCounterSemanticsV1::Exact => {
                if left.counter != right.counter {
                    return None;
                }
            }
            PersistentRelicCounterSemanticsV1::CyclicProgress { trigger_at } => {
                let left_valid = (0..trigger_at).contains(&left.counter);
                let right_valid = (0..trigger_at).contains(&right.counter);
                if !left_valid || !right_valid {
                    if left.counter != right.counter {
                        return None;
                    }
                } else if left.counter < right.counter {
                    return None;
                } else {
                    strictly_better |= left.counter > right.counter;
                }
            }
        }
    }
    Some(strictly_better)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::{
        TaskPublicChoiceV1, TaskResearchPlanPersistentOutcomeV1, TaskResearchPlanStepV1,
        TaskResearchPlanV1,
    };
    use crate::ai::planner_core::{
        PlannerPublicMap, PlannerPublicMapEdge, PlannerPublicMapNode, PlannerRunGoal,
    };

    fn run_context() -> PublicCombatRunContextV1 {
        PublicCombatRunContextV1::Available {
            run_goal: PlannerRunGoal::ActThreeVictory,
            ascension_level: 0,
            combat_victory_guarantees_full_heal: false,
            act: 2,
            floor: 25,
            keys: [false; 3],
            public_map: PlannerPublicMap {
                current_x: 0,
                current_y: 0,
                boss: None,
                nodes: vec![
                    PlannerPublicMapNode {
                        x: 0,
                        y: 0,
                        room: PlannerMapRoom::Monster,
                        has_emerald_key: false,
                        edges: vec![PlannerPublicMapEdge {
                            destination_x: 1,
                            destination_y: 1,
                        }],
                    },
                    PlannerPublicMapNode {
                        x: 1,
                        y: 1,
                        room: PlannerMapRoom::Rest,
                        has_emerald_key: false,
                        edges: Vec::new(),
                    },
                ],
            },
            encounter_id: None,
        }
    }

    fn candidate(
        producer_name: &str,
        final_hp: i32,
        consumed_entry_potion_slots: Vec<usize>,
    ) -> TaskResearchPlanCandidateV1 {
        let provenance = TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: producer_name.to_string(),
            producer_revision: 1,
        };
        TaskResearchPlanCandidateV1 {
            plan: TaskResearchPlanV1 {
                root_public_snapshot_id: "root".to_string(),
                steps: vec![TaskResearchPlanStepV1 {
                    source_public_snapshot_id: "root".to_string(),
                    choice: TaskPublicChoiceV1::one(0),
                }],
            },
            outcome: TaskResearchPlanOutcomeV1 {
                final_hp,
                final_gold: 0,
                persistent: TaskResearchPlanPersistentOutcomeV1 {
                    max_hp: 80,
                    recoverable_gold_delta: 0,
                    unrecovered_stolen_gold: 0,
                    ritual_dagger_value: 0,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                consumed_entry_potion_slots: consumed_entry_potion_slots.clone(),
                total_potions_consumed: consumed_entry_potion_slots.len() as u32,
                generated_potions: Vec::new(),
                final_relics: Vec::new(),
                meta_changes: Vec::new(),
                pending_rewards: Vec::new(),
                combat_mugged: false,
            },
            satisfies_requested_quality: false,
            supporting_provenances: vec![provenance],
        }
    }

    fn selection(
        candidates: &[TaskResearchPlanCandidateV1],
    ) -> TaskCombatResearchPlanSelectionOutcomeV1 {
        selection_with(
            candidates,
            &run_context(),
            &[Some(PotionId::BlessingOfTheForge)],
        )
    }

    fn selection_with(
        candidates: &[TaskResearchPlanCandidateV1],
        run_context: &PublicCombatRunContextV1,
        entry_potions: &[Option<PotionId>],
    ) -> TaskCombatResearchPlanSelectionOutcomeV1 {
        let frontier = task_research_plan_non_dominated_candidate_ordinals_v1(candidates);
        select_combat_research_candidate_v1(candidates, &frontier, run_context, entry_potions, &[])
    }

    fn with_plan_steps(
        mut candidate: TaskResearchPlanCandidateV1,
        step_count: usize,
    ) -> TaskResearchPlanCandidateV1 {
        let step = candidate.plan.steps[0].clone();
        candidate.plan.steps = vec![step; step_count];
        candidate
    }

    #[test]
    fn exact_equal_outcomes_choose_the_unique_shortest_public_plan() {
        let candidates = [
            with_plan_steps(candidate("long", 33, Vec::new()), 14),
            with_plan_steps(candidate("short", 33, Vec::new()), 13),
        ];

        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 1,
                basis: TaskCombatResearchPlanSelectionBasisV1::UniqueShortestExactOutcome,
                run_commitment: None,
            }
        );
    }

    #[test]
    fn exact_equal_outcomes_with_tied_plan_length_stay_ambiguous() {
        let candidates = [
            with_plan_steps(candidate("left", 33, Vec::new()), 13),
            with_plan_steps(candidate("right", 33, Vec::new()), 13),
        ];

        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
                reason: TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier,
            }
        );
    }

    #[test]
    fn equal_outcome_plan_order_is_independent_of_producer_and_discovery_order() {
        let mut preferred = with_plan_steps(candidate("z-producer", 33, Vec::new()), 2);
        preferred.plan.steps[1].choice.ordinal_path = vec![2, 9];
        preferred.plan.steps[1].source_public_snapshot_id = "preferred-source".to_string();
        let mut alternate = preferred.clone();
        alternate.plan.steps[1].choice.ordinal_path = vec![10, 0];
        alternate.plan.steps[1].source_public_snapshot_id = "alternate-source".to_string();
        alternate.supporting_provenances[0].producer_name = "a-producer".to_string();
        alternate.satisfies_requested_quality = true;
        // Lower ordinals cannot make a longer plan win the tie.
        let longer = with_plan_steps(candidate("first-producer", 33, Vec::new()), 3);
        let original = [preferred, alternate, longer];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let candidates = order.map(|i| original[i].clone());
            let before = candidates.clone();
            let selected = order.iter().position(|i| *i == 0).unwrap();
            assert_eq!(
                selection(&candidates),
                TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                    candidate_ordinal: selected,
                    basis: TaskCombatResearchPlanSelectionBasisV1::EqualOutcomePlanOrderPreference,
                    run_commitment: None,
                }
            );
            assert_eq!(candidates[selected].plan, original[0].plan);
            assert_eq!(candidates, before);
            assert_eq!(
                task_research_plan_non_dominated_candidate_ordinals_v1(&candidates),
                vec![0, 1, 2]
            );
        }
    }

    #[test]
    fn equal_choice_sequences_do_not_guess_between_source_frames() {
        let left = with_plan_steps(candidate("left", 33, Vec::new()), 2);
        let mut right = left.clone();
        right.plan.steps[1].source_public_snapshot_id = "different-source".to_string();
        assert_eq!(
            select_equal_outcome_plan_order_preference_v1(&[left, right], &[0, 1]),
            None
        );
    }

    #[test]
    fn plan_order_does_not_break_resource_tradeoffs() {
        let original = candidate("left", 33, Vec::new());
        for difference in 0..6 {
            let mut alternative = original.clone();
            alternative.plan.steps[0].choice = TaskPublicChoiceV1::one(1);
            match difference {
                0 => alternative.outcome.final_hp += 1,
                1 => {
                    alternative.outcome.consumed_entry_potion_slots = vec![0];
                    alternative.outcome.total_potions_consumed = 1;
                }
                2 => alternative
                    .outcome
                    .pending_rewards
                    .push(TaskResearchPlanPendingRewardV1::Gold { amount: 1 }),
                3 => alternative
                    .outcome
                    .final_relics
                    .push(RelicState::new(RelicId::Sundial)),
                4 => alternative.outcome.persistent.max_hp += 1,
                _ => alternative.outcome.combat_mugged = true,
            }
            assert_eq!(
                select_equal_outcome_plan_order_preference_v1(
                    &[original.clone(), alternative],
                    &[0, 1]
                ),
                None
            );
        }
    }

    #[test]
    fn equal_outcome_support_contract_rejects_false_ties_and_longer_choices() {
        use crate::agent::task::{
            task_public_combat_decision_v1, TaskActorBoundaryV1, TaskActorViewV1,
            TaskBudgetStateV1, TaskResearchPlanAttemptExitV1, TaskResearchPlanLaneAttemptV1,
            TaskResearchPlanWorkUnitV1, TaskSpecV1,
        };
        let combat = crate::test_support::blank_test_combat();
        let actions = crate::agent::information::action::project_public_combat_actions_v1(
            &crate::state::EngineState::CombatPlayerTurn,
            &combat,
        )
        .unwrap();
        let decision = task_public_combat_decision_v1(
            run_context(),
            crate::agent::information::state::public_combat_state_v1(&combat),
            &actions.public,
        )
        .unwrap();
        let mut left = with_plan_steps(candidate("producer", 33, Vec::new()), 2);
        left.plan.root_public_snapshot_id = decision.information.snapshot_id.clone();
        left.plan.steps[0].source_public_snapshot_id = decision.information.snapshot_id.clone();
        let mut right = left.clone();
        right.plan.steps[1].choice = TaskPublicChoiceV1::one(1);
        let attempt = TaskResearchPlanAttemptV1 {
            lanes: vec![TaskResearchPlanLaneAttemptV1 {
                provenance: left.supporting_provenances[0].clone(),
                work_unit: TaskResearchPlanWorkUnitV1::AtomicExpandedNodes,
                work_consumed: 1,
                exit: TaskResearchPlanAttemptExitV1::Witness,
            }],
            candidates: vec![left, right],
        };
        let support = privileged_combat_research_support_v1(&decision, attempt).unwrap();
        let view = TaskActorViewV1 {
            task: TaskSpecV1::ironclad_a0_act_three_fail_closed(),
            budget: TaskBudgetStateV1 {
                remaining_world_transitions: 100,
                remaining_reloads: 0,
            },
            committed_history: Default::default(),
            legal_knowledge: Default::default(),
            node_memory: None,
            reload_permission: None,
            boundary: TaskActorBoundaryV1::Decision { decision },
        };
        assert_eq!(
            support.selection,
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 0,
                basis: TaskCombatResearchPlanSelectionBasisV1::EqualOutcomePlanOrderPreference,
                run_commitment: None,
            }
        );
        support.validate(&view).unwrap();
        let mut longer = support.clone();
        let extra_step = longer.attempt.candidates[0].plan.steps[1].clone();
        longer.attempt.candidates[0].plan.steps.push(extra_step);
        assert!(longer
            .validate(&view)
            .unwrap_err()
            .contains("plan-order selection requires"));
        let mut different = support.clone();
        different.attempt.candidates[1].outcome.final_hp += 1;
        different.attempt.candidates[1]
            .outcome
            .consumed_entry_potion_slots = vec![0];
        different.attempt.candidates[1]
            .outcome
            .total_potions_consumed = 1;
        assert_eq!(
            task_research_plan_non_dominated_candidate_ordinals_v1(&different.attempt.candidates),
            vec![0, 1]
        );
        assert!(different
            .validate(&view)
            .unwrap_err()
            .contains("plan-order selection requires"));
        let mut single = support;
        single.attempt.candidates.truncate(1);
        single.non_dominated_candidate_ordinals = vec![0];
        assert!(single
            .validate(&view)
            .unwrap_err()
            .contains("plan-order selection requires"));
    }

    fn with_nunchaku_counter(
        mut candidate: TaskResearchPlanCandidateV1,
        counter: i32,
    ) -> TaskResearchPlanCandidateV1 {
        let mut relic = RelicState::new(RelicId::Nunchaku);
        relic.counter = counter;
        candidate.outcome.final_relics = vec![relic];
        candidate
    }

    #[test]
    fn cross_band_hp_and_potion_tradeoff_stays_ambiguous() {
        let candidates = [
            candidate("conserving", 31, Vec::new()),
            candidate("potion-line", 74, vec![0]),
        ];

        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
                reason: TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier,
            }
        );
    }

    #[test]
    fn same_hp_band_retains_the_entry_potion() {
        let mut potion_line = candidate("potion-line", 62, vec![0]);
        potion_line.outcome.persistent.max_hp = 77;
        let mut conserving = candidate("conserving", 59, Vec::new());
        conserving.outcome.persistent.max_hp = 77;
        let candidates = [potion_line, conserving];

        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 1,
                basis:
                    TaskCombatResearchPlanSelectionBasisV1::CarryoverHpBandResourceParetoPreference,
                run_commitment: None,
            }
        );
    }

    #[test]
    fn known_cyclic_relic_progress_participates_in_resource_pareto() {
        let ahead = with_nunchaku_counter(candidate("ahead", 80, Vec::new()), 7);
        let behind = with_nunchaku_counter(candidate("behind", 80, Vec::new()), 6);
        let candidates = [ahead, behind];

        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 0,
                basis:
                    TaskCombatResearchPlanSelectionBasisV1::CarryoverHpBandResourceParetoPreference,
                run_commitment: None,
            }
        );
    }

    #[test]
    fn guaranteed_full_heal_ignores_terminal_hp_and_keeps_cyclic_progress() {
        let candidates = [
            with_nunchaku_counter(candidate("primed", 19, Vec::new()), 9),
            with_nunchaku_counter(candidate("healthier", 23, Vec::new()), 2),
            with_nunchaku_counter(candidate("healthier-behind", 23, Vec::new()), 0),
        ];
        let mut context = run_context();
        let PublicCombatRunContextV1::Available {
            combat_victory_guarantees_full_heal,
            ..
        } = &mut context
        else {
            unreachable!("test context is available")
        };
        *combat_victory_guarantees_full_heal = true;

        assert_eq!(
            selection_with(&candidates, &context, &[]),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 0,
                basis: TaskCombatResearchPlanSelectionBasisV1::GuaranteedFullHealResourceParetoPreference,
                run_commitment: None,
            }
        );
    }

    #[test]
    fn terminal_hp_remains_a_resource_without_guaranteed_full_heal() {
        let candidates = [
            with_nunchaku_counter(candidate("primed", 19, Vec::new()), 9),
            with_nunchaku_counter(candidate("healthier", 23, Vec::new()), 2),
            with_nunchaku_counter(candidate("healthier-behind", 23, Vec::new()), 0),
        ];

        assert_eq!(
            selection_with(&candidates, &run_context(), &[]),
            TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
                reason: TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier,
            }
        );
    }

    #[test]
    fn claimable_stolen_gold_distribution_does_not_block_resource_pareto() {
        let mut healthier = with_nunchaku_counter(candidate("healthier", 70, Vec::new()), 7);
        healthier.outcome.final_gold = 39;
        healthier.outcome.pending_rewards = vec![
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 45 },
            TaskResearchPlanPendingRewardV1::Gold { amount: 15 },
        ];
        let mut lower_hp = with_nunchaku_counter(candidate("lower-hp", 60, Vec::new()), 3);
        lower_hp.outcome.final_gold = 54;
        lower_hp.outcome.pending_rewards = vec![
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 30 },
            TaskResearchPlanPendingRewardV1::Gold { amount: 15 },
        ];
        let candidates = [healthier, lower_hp];

        assert_eq!(
            task_research_plan_non_dominated_candidate_ordinals_v1(&candidates),
            vec![0, 1]
        );
        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 0,
                basis:
                    TaskCombatResearchPlanSelectionBasisV1::CarryoverHpBandResourceParetoPreference,
                run_commitment: None,
            }
        );
    }

    fn looter_resource_candidates() -> [TaskResearchPlanCandidateV1; 3] {
        let mut saved = candidate("saved", 55, Vec::new());
        saved.outcome.final_gold = 75;
        saved.outcome.pending_rewards = vec![
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 60 },
            TaskResearchPlanPendingRewardV1::Gold { amount: 13 },
        ];
        let mut spent = candidate("spent", 62, vec![0, 1]);
        spent.outcome.final_gold = 95;
        spent.outcome.pending_rewards = vec![
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 40 },
            TaskResearchPlanPendingRewardV1::Gold { amount: 13 },
        ];
        let mut escaped = candidate("escaped", 48, Vec::new());
        escaped.outcome.final_gold = 75;
        escaped.outcome.persistent.recoverable_gold_delta = -60;
        escaped.outcome.persistent.unrecovered_stolen_gold = 60;
        escaped.outcome.combat_mugged = true;
        let mut candidates = [saved, spent, escaped];
        for candidate in &mut candidates {
            candidate.outcome.persistent.max_hp = 72;
        }
        candidates
    }

    #[test]
    fn rewarded_exit_preference_composes_with_potion_retention_without_pruning_facts() {
        let original = looter_resource_candidates();
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let candidates = order.map(|i| original[i].clone());
            let before = candidates.clone();
            let frontier = task_research_plan_non_dominated_candidate_ordinals_v1(&candidates);
            assert_eq!(frontier, vec![0, 1, 2]);
            let selected = selection_with(
                &candidates,
                &run_context(),
                &[Some(PotionId::GamblersBrew), Some(PotionId::SteroidPotion)],
            );
            assert_eq!(
                selected,
                TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                    candidate_ordinal: order.iter().position(|i| *i == 0).unwrap(),
                    basis:
                        TaskCombatResearchPlanSelectionBasisV1::RewardedExitResourceParetoPreference,
                    run_commitment: None,
                }
            );
            assert_eq!(candidates, before);
            assert_eq!(
                task_research_plan_non_dominated_candidate_ordinals_v1(&candidates),
                frontier
            );
        }
    }

    #[test]
    fn rewarded_exit_does_not_erase_real_resource_tradeoffs() {
        let [saved, _, escaped] = looter_resource_candidates();
        for difference in 0..6 {
            let mut alternative = escaped.clone();
            match difference {
                // Exact HP is required even within the same carryover band.
                0 => alternative.outcome.final_hp = 56,
                1 => alternative.outcome.final_gold = 200,
                2 => alternative.outcome.pending_rewards.push(
                    TaskResearchPlanPendingRewardV1::Potion {
                        potion_id: PotionId::GhostInAJar,
                    },
                ),
                3 => alternative
                    .outcome
                    .final_relics
                    .push(RelicState::new(RelicId::Sundial)),
                4 => alternative.outcome.persistent.max_hp += 1,
                _ => alternative.outcome.persistent.ritual_dagger_value += 1,
            }
            assert!(!saved
                .outcome
                .rewarded_exit_strictly_dominates_v1(&alternative.outcome));
            assert!(matches!(
                selection(&[saved.clone(), alternative]),
                TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
                    reason:
                        TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier
                }
            ));
        }
        let mut paid = saved.clone();
        paid.outcome.consumed_entry_potion_slots = vec![0];
        paid.outcome.total_potions_consumed = 1;
        assert!(!paid
            .outcome
            .rewarded_exit_strictly_dominates_v1(&escaped.outcome));
    }

    #[test]
    fn exit_label_alone_is_not_a_strict_resource_improvement() {
        let [saved, _, _] = looter_resource_candidates();
        let mut escaped = saved.clone();
        escaped.outcome.combat_mugged = true;
        assert!(!saved
            .outcome
            .rewarded_exit_strictly_dominates_v1(&escaped.outcome));
        assert!(!escaped
            .outcome
            .rewarded_exit_strictly_dominates_v1(&saved.outcome));
    }

    #[test]
    fn full_hp_without_potion_beats_unprimed_cyclic_progress() {
        let mut potion_line = candidate("potion-line", 57, vec![0]);
        potion_line.outcome.persistent.max_hp = 82;
        let potion_line = with_nunchaku_counter(potion_line, 7);
        let mut full_hp = candidate("full-hp", 82, Vec::new());
        full_hp.outcome.persistent.max_hp = 82;
        let full_hp = with_nunchaku_counter(full_hp, 5);
        let mut damaged = candidate("damaged", 43, Vec::new());
        damaged.outcome.persistent.max_hp = 82;
        let damaged = with_nunchaku_counter(damaged, 7);
        let candidates = [potion_line, full_hp, damaged];

        let frontier = task_research_plan_non_dominated_candidate_ordinals_v1(&candidates);
        assert_eq!(frontier, vec![0, 1, 2]);
        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 1,
                basis: TaskCombatResearchPlanSelectionBasisV1::FullHpOverUnprimedCyclicProgress,
                run_commitment: None,
            }
        );
    }

    #[test]
    fn full_hp_does_not_override_cyclic_progress_primed_for_next_action() {
        let mut full_hp = candidate("full-hp", 82, Vec::new());
        full_hp.outcome.persistent.max_hp = 82;
        let full_hp = with_nunchaku_counter(full_hp, 5);
        let mut primed = candidate("primed", 57, Vec::new());
        primed.outcome.persistent.max_hp = 82;
        let primed = with_nunchaku_counter(primed, 9);
        let candidates = [full_hp, primed];

        let frontier = task_research_plan_non_dominated_candidate_ordinals_v1(&candidates);
        assert_eq!(frontier, vec![0, 1]);
        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
                reason: TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier,
            }
        );
    }

    fn giant_head_recovery_context() -> PublicCombatRunContextV1 {
        PublicCombatRunContextV1::Available {
            run_goal: PlannerRunGoal::ActThreeVictory,
            ascension_level: 0,
            combat_victory_guarantees_full_heal: false,
            act: 3,
            floor: 47,
            keys: [false; 3],
            public_map: PlannerPublicMap {
                current_x: 0,
                current_y: 0,
                boss: Some(crate::content::monsters::factory::EncounterId::TimeEater),
                nodes: vec![
                    PlannerPublicMapNode {
                        x: 0,
                        y: 0,
                        room: PlannerMapRoom::Elite,
                        has_emerald_key: false,
                        edges: vec![
                            PlannerPublicMapEdge {
                                destination_x: 1,
                                destination_y: 1,
                            },
                            PlannerPublicMapEdge {
                                destination_x: 2,
                                destination_y: 1,
                            },
                        ],
                    },
                    PlannerPublicMapNode {
                        x: 1,
                        y: 1,
                        room: PlannerMapRoom::Shop,
                        has_emerald_key: false,
                        edges: vec![PlannerPublicMapEdge {
                            destination_x: 1,
                            destination_y: 2,
                        }],
                    },
                    PlannerPublicMapNode {
                        x: 2,
                        y: 1,
                        room: PlannerMapRoom::Monster,
                        has_emerald_key: false,
                        edges: vec![PlannerPublicMapEdge {
                            destination_x: 2,
                            destination_y: 2,
                        }],
                    },
                    PlannerPublicMapNode {
                        x: 1,
                        y: 2,
                        room: PlannerMapRoom::Rest,
                        has_emerald_key: false,
                        edges: Vec::new(),
                    },
                    PlannerPublicMapNode {
                        x: 2,
                        y: 2,
                        room: PlannerMapRoom::Rest,
                        has_emerald_key: false,
                        edges: Vec::new(),
                    },
                ],
            },
            encounter_id: Some(crate::content::monsters::factory::EncounterId::GiantHead),
        }
    }

    fn with_giant_head_recovery_resources(
        mut candidate: TaskResearchPlanCandidateV1,
        nunchaku_counter: i32,
    ) -> TaskResearchPlanCandidateV1 {
        candidate.outcome.persistent.max_hp = 82;
        candidate.outcome.pending_rewards = vec![
            TaskResearchPlanPendingRewardV1::Gold { amount: 30 },
            TaskResearchPlanPendingRewardV1::Relic {
                relic_id: RelicId::RegalPillow,
            },
            TaskResearchPlanPendingRewardV1::Potion {
                potion_id: PotionId::HeartOfIron,
            },
        ];
        let mut nunchaku = RelicState::new(RelicId::Nunchaku);
        nunchaku.counter = nunchaku_counter;
        candidate.outcome.final_relics = vec![
            RelicState::new(RelicId::BloodyIdol),
            RelicState::new(RelicId::MealTicket),
            nunchaku,
        ];
        candidate
    }

    #[test]
    fn noncombat_recovery_corridor_selects_the_resource_closed_dominator() {
        let resident =
            with_giant_head_recovery_resources(candidate("resident", 45, vec![0, 1, 2]), 4);
        let proposal = with_giant_head_recovery_resources(candidate("proposal", 20, vec![1]), 5);
        let atomic = with_giant_head_recovery_resources(candidate("atomic", 30, vec![0, 1, 2]), 5);
        let candidates = [resident, proposal, atomic];
        let entry_potions = [
            Some(PotionId::BlessingOfTheForge),
            Some(PotionId::FairyPotion),
            Some(PotionId::BloodPotion),
        ];

        assert_eq!(
            selection_with(
                &candidates,
                &giant_head_recovery_context(),
                &entry_potions,
            ),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 1,
                basis: TaskCombatResearchPlanSelectionBasisV1::NonCombatRestCorridorResourceParetoPreference,
                run_commitment: Some(
                    TaskCombatResearchPlanRunCommitmentV1::NonCombatRestCorridor {
                        route: vec![
                            TaskCommittedRouteNodeV1 { x: 1, y: 1 },
                            TaskCommittedRouteNodeV1 { x: 1, y: 2 },
                        ],
                        projected_hp_after_rest: 79,
                        required_pending_rewards: vec![
                            TaskResearchPlanPendingRewardV1::Gold { amount: 30 },
                            TaskResearchPlanPendingRewardV1::Relic {
                                relic_id: RelicId::RegalPillow,
                            },
                        ],
                        retained_entry_potions: vec![TaskRetainedEntryPotionV1 {
                            slot: 0,
                            potion: PotionId::BlessingOfTheForge,
                        }],
                        recoverable_entry_potions: vec![TaskRetainedEntryPotionV1 {
                            slot: 2,
                            potion: PotionId::BloodPotion,
                        }],
                    },
                ),
            }
        );
    }

    #[test]
    fn noncombat_recovery_corridor_stays_unknown_without_healing_closure() {
        let resident =
            with_giant_head_recovery_resources(candidate("resident", 45, vec![0, 1, 2]), 4);
        let proposal = with_giant_head_recovery_resources(candidate("proposal", 20, vec![1]), 5);
        let candidates = [resident, proposal];
        let entry_potions = [
            Some(PotionId::BlessingOfTheForge),
            Some(PotionId::FairyPotion),
            Some(PotionId::StrengthPotion),
        ];

        assert_eq!(
            selection_with(&candidates, &giant_head_recovery_context(), &entry_potions,),
            TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
                reason: TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier,
            }
        );
    }

    #[test]
    fn immediate_rest_commitment_retains_a_potion_over_one_cyclic_tick() {
        let mut potion_line = candidate("potion-line", 67, vec![0]);
        potion_line.outcome.persistent.max_hp = 77;
        let mut potion_line_relic = RelicState::new(RelicId::Nunchaku);
        potion_line_relic.counter = 1;
        potion_line.outcome.final_relics = vec![potion_line_relic];
        let mut conserving = candidate("conserving", 57, Vec::new());
        conserving.outcome.persistent.max_hp = 77;
        let mut conserving_relic = RelicState::new(RelicId::Nunchaku);
        conserving_relic.counter = 0;
        conserving.outcome.final_relics = vec![conserving_relic];
        let candidates = [potion_line, conserving];

        assert_eq!(
            selection(&candidates),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 1,
                basis:
                    TaskCombatResearchPlanSelectionBasisV1::ImmediateCampfireRecoveryPotionRetention,
                run_commitment: Some(
                    TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest {
                        destination_x: 1,
                        destination_y: 1,
                        projected_hp_after_rest: 77,
                        retained_entry_potions: vec![TaskRetainedEntryPotionV1 {
                            slot: 0,
                            potion: PotionId::BlessingOfTheForge,
                        }],
                    },
                ),
            }
        );
    }

    #[test]
    fn immediate_rest_commitment_rejects_seed17_marginal_top_up() {
        let potion_line = candidate("potion-line", 80, vec![0]);
        let conserving = candidate("conserving", 75, Vec::new());
        let candidates = [potion_line, conserving];
        let frontier = task_research_plan_non_dominated_candidate_ordinals_v1(&candidates);

        assert_eq!(
            select_immediate_campfire_recovery_potion_retention_v1(
                &candidates,
                &frontier,
                &run_context(),
                &[Some(PotionId::ColorlessPotion)],
                &[],
            ),
            None,
        );
    }

    #[test]
    fn forced_campfire_preserves_a_potion_over_seed17_marginal_full_hp_top_up() {
        let potion_line = candidate("potion-line", 80, vec![0]);
        let conserving = candidate("conserving", 75, Vec::new());
        let candidates = [potion_line, conserving];

        assert_eq!(
            selection_with(
                &candidates,
                &run_context(),
                &[Some(PotionId::ColorlessPotion)],
            ),
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal: 1,
                basis:
                    TaskCombatResearchPlanSelectionBasisV1::ForcedCampfireMarginalHpPotionRetention,
                run_commitment: None,
            },
        );
    }

    #[test]
    fn detached_combat_does_not_invent_a_run_resource_preference() {
        let mut potion_line = candidate("potion-line", 62, vec![0]);
        potion_line.outcome.persistent.max_hp = 77;
        let mut conserving = candidate("conserving", 59, Vec::new());
        conserving.outcome.persistent.max_hp = 77;
        let candidates = [potion_line, conserving];
        let frontier = task_research_plan_non_dominated_candidate_ordinals_v1(&candidates);

        assert_eq!(
            select_combat_research_candidate_v1(
                &candidates,
                &frontier,
                &PublicCombatRunContextV1::Unavailable {
                    reason: crate::agent::information::run::PublicCombatRunContextGapV1::DetachedExactCombatPosition,
                },
                &[Some(PotionId::BlessingOfTheForge)],
                &[],
            ),
            TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
                reason: TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier,
            }
        );
    }
}
