use std::collections::{BTreeMap, BTreeSet};

use crate::ai::card_semantics_v1::{card_reward_semantic_profile_v1, CardRewardSemanticRoleV1};
use crate::ai::event_resource_budget::build_event_resource_budget;
use crate::ai::opening_hand_target_plan_v1::{
    opening_hand_target_profile_for_card_v1, OpeningHandDebtTierV1,
};
use crate::ai::route_window_facts::{build_route_window_facts, RouteWindowFactsConfig};
use crate::ai::strategy::deck_role_inventory::DeckRoleInventory;
use crate::ai::upgrade_planner_v1::{
    plan_upgrades_v1, upgrade_candidate_score_hint_v1, UpgradeCandidateV1,
};
use crate::content::cards::{get_card_definition, CardId, CardRarity, CardTag, CardType};
use crate::content::relics::RelicId;
use crate::runtime::combat::{CardLineageId, CombatCard};
use crate::state::core::{RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::EventId;
use crate::state::rewards::RewardCard;
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

use super::types::{
    AllowedDeckMutationConsumersV1, CompiledDeckMutationDecisionV1, DeckMutationCardSnapshotV1,
    DeckMutationCommitmentModeV1, DeckMutationCompilerOutputV1, DeckMutationCompilerRequestV1,
    DeckMutationKindV1, DeckMutationOpeningHandDebtTierV1, DeckMutationOpeningHandProfileV1,
    DeckMutationPlanCandidateV1, DeckMutationPlanRoleV1, DeckMutationPlanStepV1,
    DeckMutationRemovalBenefitTierV1, DeckMutationRemovalBenefitV1, DeckMutationTargetClassV1,
    DeckMutationTargetLossTierV1, DeckMutationTargetLossV1, DeckMutationTransformProfileV1,
    DuplicateStackBehaviorV1, DuplicateTargetEvaluationV1, DuplicateTargetRoleV1,
    TransformRandomAdditionBandV1, TransformVarianceRiskV1,
};

const MAX_DUPLICATE_OPTIONS_PER_BRANCH: usize = 4;
const MAX_MULTI_OPTIONS_FOR_INSPECTION: usize = 64;
const PREMIUM_DUPLICATE_TARGET_PRIORITY: i32 = 760;
const POLICY_PREFERRED_UPGRADE_PRIORITY_THRESHOLD: i32 = 180;

#[derive(Clone, Debug)]
struct ExactTarget {
    card: DeckMutationCardSnapshotV1,
    identity_key: String,
    selectable: bool,
    upgrade_priority: Option<i32>,
    upgrade_candidate: Option<UpgradeCandidateV1>,
    duplicate_evaluation: DuplicateTargetEvaluationV1,
}

#[derive(Clone, Debug)]
struct TargetGroup {
    targets: Vec<ExactTarget>,
}

#[derive(Clone, Debug)]
struct GroupCountCombination {
    group_counts: Vec<usize>,
    represented_exact_count: usize,
}

#[derive(Clone, Copy, Debug)]
struct BonfireOfferContextV1 {
    current_hp: i32,
    max_hp: i32,
    reserve_floor: i32,
    healing_blocked: bool,
}

pub fn compile_deck_mutation_decision_v1(
    run_state: &RunState,
    choice: &RunPendingChoiceState,
    request: DeckMutationCompilerRequestV1,
) -> CompiledDeckMutationDecisionV1 {
    let targets = exact_targets(run_state, choice);
    let mut candidate_plans = plan_candidates(choice, &targets, request.output);
    let bonfire_offer_context = bonfire_offer_context_v1(run_state, choice);

    let low_value_available = targets
        .iter()
        .filter(|target| target.selectable)
        .any(|target| target.card.target_class.is_low_value_mutation_target());
    let clean_bottle_target_available = if is_bottle_choice(choice.reason) {
        targets
            .iter()
            .filter(|target| target.selectable)
            .any(|target| {
                target.card.opening_hand.debt_tier <= DeckMutationOpeningHandDebtTierV1::Mild
                    && target.card.opening_hand.score_hint > 0
            })
    } else {
        false
    };

    for candidate in &mut candidate_plans {
        evaluate_candidate(
            choice,
            candidate,
            low_value_available,
            clean_bottle_target_available,
            bonfire_offer_context,
        );
    }
    candidate_plans.sort_by(compare_deck_mutation_candidates_v1);

    let selected_plan = selected_plan_for_commitment(request.commitment, &candidate_plans);
    let branch_limit = match request.output {
        DeckMutationCompilerOutputV1::BranchTopK { max_active } => max_active,
        _ => usize::MAX,
    };
    let branch_active_plans = candidate_plans
        .iter()
        .filter(|candidate| candidate.allowed_consumers.branch_active)
        .take(branch_limit)
        .cloned()
        .collect();
    let inspect_only_plans = candidate_plans
        .iter()
        .filter(|candidate| candidate.role == DeckMutationPlanRoleV1::InspectOnly)
        .cloned()
        .collect();
    let blocked_plans = candidate_plans
        .iter()
        .filter(|candidate| candidate.role == DeckMutationPlanRoleV1::Blocked)
        .cloned()
        .collect();

    CompiledDeckMutationDecisionV1 {
        reason: choice.reason,
        min_choices: choice.min_choices,
        max_choices: choice.max_choices,
        output: request.output,
        commitment: request.commitment,
        selected_plan,
        branch_active_plans,
        inspect_only_plans,
        blocked_plans,
        candidate_plans,
        label_role: "behavior_policy_not_teacher",
    }
}

fn selected_plan_for_commitment(
    commitment: DeckMutationCommitmentModeV1,
    candidate_plans: &[DeckMutationPlanCandidateV1],
) -> Option<DeckMutationPlanCandidateV1> {
    match commitment {
        DeckMutationCommitmentModeV1::Optional => candidate_plans
            .iter()
            .find(|candidate| candidate.allowed_consumers.execute_autopilot)
            .cloned(),
        DeckMutationCommitmentModeV1::CommittedForced => candidate_plans
            .iter()
            .find(|candidate| candidate.role != DeckMutationPlanRoleV1::Blocked)
            .cloned()
            .map(|mut candidate| {
                candidate
                    .reasons
                    .push("commitment_mode=CommittedForced".to_string());
                if !candidate.allowed_consumers.execute_autopilot {
                    candidate
                        .risks
                        .push("committed_forced_selected_non_autopilot_target".to_string());
                }
                if matches!(
                    candidate.role,
                    DeckMutationPlanRoleV1::RiskyExploration | DeckMutationPlanRoleV1::InspectOnly
                ) {
                    candidate.risks.push(
                        "committed_forced_least_bad_protected_or_functional_target".to_string(),
                    );
                }
                candidate
            }),
    }
}

pub fn compile_direct_deck_mutation_plan_candidate_v1(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
    deck_index: usize,
    command: String,
    effect_kind: String,
    effect_key: String,
    effect_label: String,
    low_value_available: bool,
) -> Option<DeckMutationPlanCandidateV1> {
    let target = exact_target_for_deck_index(run_state, reason, deck_index, true, None)?;
    let mut candidate = candidate_from_targets_for_reason(reason, vec![target], 1);
    candidate.plan_id = format!("deck_mutation:{effect_key}");
    candidate.step.command = command;
    candidate.step.effect_kind = effect_kind;
    candidate.step.effect_key = effect_key;
    candidate.step.effect_label = effect_label;
    candidate
        .reasons
        .push("direct event deck mutation option".to_string());
    evaluate_candidate_for_reason(reason, &mut candidate, low_value_available, false);
    Some(candidate)
}

pub fn deck_mutation_target_class_for_card_v1(
    reason: RunPendingChoiceReason,
    card: &CombatCard,
) -> DeckMutationTargetClassV1 {
    target_class_for_card_mutation(reason, card)
}

pub fn deck_removal_target_snapshots_v1(run_state: &RunState) -> Vec<DeckMutationCardSnapshotV1> {
    run_state
        .master_deck
        .iter()
        .enumerate()
        .filter(|(_, card)| crate::state::core::master_deck_card_is_purgeable(card))
        .filter_map(|(deck_index, _)| {
            exact_target_for_deck_index(
                run_state,
                RunPendingChoiceReason::PurgeNonBottled,
                deck_index,
                true,
                None,
            )
            .map(|target| target.card)
        })
        .collect()
}

pub fn best_duplicate_target_for_shop_v1(
    run_state: &RunState,
) -> Option<DuplicateTargetEvaluationV1> {
    run_state
        .master_deck
        .iter()
        .map(|card| duplicate_target_evaluation_v1(card, run_state))
        .filter(|evaluation| evaluation.premium)
        .max_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| (left.card as i32).cmp(&(right.card as i32)))
                .then_with(|| left.upgrades.cmp(&right.upgrades))
        })
}

fn compare_deck_mutation_candidates_v1(
    left: &DeckMutationPlanCandidateV1,
    right: &DeckMutationPlanCandidateV1,
) -> std::cmp::Ordering {
    deck_mutation_role_rank(left.role)
        .cmp(&deck_mutation_role_rank(right.role))
        .then_with(|| right.score_hint.cmp(&left.score_hint))
        .then_with(|| left.step.deck_indices.cmp(&right.step.deck_indices))
}

fn deck_mutation_role_rank(role: DeckMutationPlanRoleV1) -> u8 {
    match role {
        DeckMutationPlanRoleV1::PolicyPreferred => 0,
        DeckMutationPlanRoleV1::SafeAlternative => 1,
        DeckMutationPlanRoleV1::RiskyExploration => 2,
        DeckMutationPlanRoleV1::InspectOnly => 3,
        DeckMutationPlanRoleV1::Blocked => 4,
    }
}

fn exact_targets(run_state: &RunState, choice: &RunPendingChoiceState) -> Vec<ExactTarget> {
    let request = choice.selection_request(run_state);
    let target_lineages = request
        .targets
        .iter()
        .map(|target| match target {
            crate::state::selection::SelectionTargetRef::CardLineage(lineage) => *lineage,
        })
        .collect::<BTreeSet<_>>();
    let upgrade_candidates_by_index = if matches!(choice.reason, RunPendingChoiceReason::Upgrade) {
        plan_upgrades_v1(run_state)
            .candidates
            .into_iter()
            .map(|candidate| (candidate.deck_index, candidate))
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };
    run_state
        .master_deck
        .iter()
        .enumerate()
        .filter(|(_, card)| target_lineages.contains(&card.lineage))
        .filter_map(|(deck_index, _card)| {
            exact_target_for_deck_index(
                run_state,
                choice.reason,
                deck_index,
                run_state.master_deck.get(deck_index).is_some_and(|card| {
                    crate::state::core::run_pending_choice_allows_card_for_run(
                        &choice.reason,
                        card,
                        run_state,
                    )
                }),
                upgrade_candidates_by_index.get(&deck_index).cloned(),
            )
        })
        .collect()
}

fn exact_target_for_deck_index(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
    deck_index: usize,
    selectable: bool,
    upgrade_candidate: Option<UpgradeCandidateV1>,
) -> Option<ExactTarget> {
    let card = run_state.master_deck.get(deck_index)?;
    let target_class = target_class_for_card_mutation(reason, card);
    let upgrade_priority = upgrade_candidate
        .as_ref()
        .map(upgrade_candidate_score_hint_v1);
    Some(ExactTarget {
        card: DeckMutationCardSnapshotV1 {
            deck_index,
            lineage: card.lineage,
            card: card.id,
            upgrades: card.upgrades,
            label: card_label(card.id, card.upgrades),
            target_class,
            removal_benefit: removal_benefit_for_card_mutation(
                run_state,
                reason,
                card,
                target_class,
            ),
            target_loss: target_loss_for_card_mutation(run_state, reason, card, target_class),
            opening_hand: opening_hand_profile_for_card_mutation(run_state, reason, card),
            transform: transform_profile_for_card_mutation(run_state, reason, card, target_class),
        },
        identity_key: card_stat_identity_key(card),
        selectable,
        upgrade_priority,
        upgrade_candidate,
        duplicate_evaluation: duplicate_target_evaluation_v1(card, run_state),
    })
}

fn plan_candidates(
    choice: &RunPendingChoiceState,
    targets: &[ExactTarget],
    output: DeckMutationCompilerOutputV1,
) -> Vec<DeckMutationPlanCandidateV1> {
    if choice.min_choices == 0 || choice.min_choices != choice.max_choices {
        return Vec::new();
    }

    if choice.min_choices == 1 {
        let mut candidates = compressed_single_candidates(choice, targets);
        if matches!(choice.reason, RunPendingChoiceReason::Duplicate) {
            candidates.sort_by(|left, right| {
                right
                    .score_hint
                    .cmp(&left.score_hint)
                    .then_with(|| left.step.deck_indices.cmp(&right.step.deck_indices))
            });
            let limit = match output {
                DeckMutationCompilerOutputV1::BranchTopK { max_active } => {
                    max_active.min(MAX_DUPLICATE_OPTIONS_PER_BRANCH)
                }
                _ => candidates.len(),
            };
            apply_portfolio_suppression(&mut candidates, limit);
            return candidates.into_iter().take(limit).collect();
        }
        return candidates;
    }

    if matches!(output, DeckMutationCompilerOutputV1::ExecuteOne) {
        return greedy_multi_candidate(choice, targets)
            .into_iter()
            .collect();
    }

    compressed_multi_candidates(
        choice,
        targets,
        match output {
            DeckMutationCompilerOutputV1::BranchTopK { max_active } => max_active,
            DeckMutationCompilerOutputV1::Inspect => MAX_MULTI_OPTIONS_FOR_INSPECTION,
            DeckMutationCompilerOutputV1::ExecuteOne => unreachable!("handled above"),
        },
    )
    .unwrap_or_else(|| {
        greedy_multi_candidate(choice, targets)
            .into_iter()
            .collect()
    })
}

fn greedy_multi_candidate(
    choice: &RunPendingChoiceState,
    targets: &[ExactTarget],
) -> Option<DeckMutationPlanCandidateV1> {
    let mut selectable = targets
        .iter()
        .filter(|target| target.selectable)
        .cloned()
        .collect::<Vec<_>>();
    if selectable.len() < choice.min_choices {
        return None;
    }
    selectable.sort_by(|left, right| {
        target_score_hint(choice.reason, right)
            .cmp(&target_score_hint(choice.reason, left))
            .then_with(|| left.card.deck_index.cmp(&right.card.deck_index))
    });
    let selected = selectable
        .into_iter()
        .take(choice.min_choices)
        .collect::<Vec<_>>();
    let mut candidate = candidate_from_targets(choice, selected, 1);
    candidate
        .reasons
        .push("bounded compiler fallback selected a greedy representative".to_string());
    Some(candidate)
}

fn compressed_single_candidates(
    choice: &RunPendingChoiceState,
    targets: &[ExactTarget],
) -> Vec<DeckMutationPlanCandidateV1> {
    let mut groups = Vec::<TargetGroup>::new();
    for target in targets.iter().filter(|target| target.selectable).cloned() {
        if let Some(group) = groups.iter_mut().find(|group| {
            group.targets[0].identity_key == target.identity_key
                && group.targets[0].card.target_class == target.card.target_class
        }) {
            group.targets.push(target);
        } else {
            groups.push(TargetGroup {
                targets: vec![target],
            });
        }
    }
    groups
        .into_iter()
        .filter_map(|group| {
            let target = group.targets.first()?.clone();
            Some(candidate_from_targets(
                choice,
                vec![target],
                group.targets.len(),
            ))
        })
        .collect()
}

fn compressed_multi_candidates(
    choice: &RunPendingChoiceState,
    targets: &[ExactTarget],
    limit: usize,
) -> Option<Vec<DeckMutationPlanCandidateV1>> {
    let groups = target_groups(targets);
    let combinations = bounded_group_count_combinations(&groups, choice.min_choices, limit)?;
    if combinations.is_empty() {
        return None;
    }
    Some(
        combinations
            .into_iter()
            .filter_map(|combo| {
                let selected_targets = combo
                    .group_counts
                    .iter()
                    .enumerate()
                    .flat_map(|(group_idx, count)| groups[group_idx].targets.iter().take(*count))
                    .cloned()
                    .collect::<Vec<_>>();
                if selected_targets.len() == choice.min_choices {
                    Some(candidate_from_targets(
                        choice,
                        selected_targets,
                        combo.represented_exact_count,
                    ))
                } else {
                    None
                }
            })
            .collect(),
    )
}

fn target_groups(targets: &[ExactTarget]) -> Vec<TargetGroup> {
    let mut groups = Vec::<TargetGroup>::new();
    for target in targets.iter().filter(|target| target.selectable).cloned() {
        if let Some(group) = groups.iter_mut().find(|group| {
            group.targets[0].identity_key == target.identity_key
                && group.targets[0].card.target_class == target.card.target_class
        }) {
            group.targets.push(target);
        } else {
            groups.push(TargetGroup {
                targets: vec![target],
            });
        }
    }
    groups
}

fn bounded_group_count_combinations(
    groups: &[TargetGroup],
    choose: usize,
    limit: usize,
) -> Option<Vec<GroupCountCombination>> {
    if choose == 0
        || groups
            .iter()
            .map(|group| group.targets.len())
            .sum::<usize>()
            < choose
    {
        return None;
    }

    let mut combinations = Vec::new();
    let mut group_counts = vec![0; groups.len()];
    if collect_group_count_combinations(
        groups,
        choose,
        limit,
        0,
        &mut group_counts,
        &mut combinations,
    ) {
        Some(combinations)
    } else {
        None
    }
}

fn collect_group_count_combinations(
    groups: &[TargetGroup],
    remaining: usize,
    limit: usize,
    group_index: usize,
    group_counts: &mut [usize],
    combinations: &mut Vec<GroupCountCombination>,
) -> bool {
    if group_index >= groups.len() {
        if remaining == 0 {
            let represented_exact_count = group_counts
                .iter()
                .enumerate()
                .map(|(idx, count)| binomial(groups[idx].targets.len(), *count))
                .product();
            combinations.push(GroupCountCombination {
                group_counts: group_counts.to_vec(),
                represented_exact_count,
            });
        }
        return combinations.len() <= limit;
    }

    let max_count = groups[group_index].targets.len().min(remaining);
    for count in (0..=max_count).rev() {
        group_counts[group_index] = count;
        if !collect_group_count_combinations(
            groups,
            remaining - count,
            limit,
            group_index + 1,
            group_counts,
            combinations,
        ) {
            return false;
        }
    }
    group_counts[group_index] = 0;
    true
}

fn binomial(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    (0..k).fold(1usize, |acc, i| acc * (n - i) / (i + 1))
}

fn candidate_from_targets(
    choice: &RunPendingChoiceState,
    selected_targets: Vec<ExactTarget>,
    representative_count: usize,
) -> DeckMutationPlanCandidateV1 {
    candidate_from_targets_for_reason(choice.reason, selected_targets, representative_count)
}

fn candidate_from_targets_for_reason(
    reason: RunPendingChoiceReason,
    selected_targets: Vec<ExactTarget>,
    representative_count: usize,
) -> DeckMutationPlanCandidateV1 {
    let kind = deck_mutation_kind(reason);
    let cards = selected_targets
        .iter()
        .map(|target| target.card.clone())
        .collect::<Vec<_>>();
    let deck_indices = cards.iter().map(|card| card.deck_index).collect::<Vec<_>>();
    let labels = cards
        .iter()
        .map(|card| card.label.as_str())
        .collect::<Vec<_>>();
    let effect_kind = effect_kind(reason).to_string();
    let effect_key = if selected_targets.len() == 1 {
        format!(
            "run_selection:{}:{}",
            effect_kind, selected_targets[0].identity_key
        )
    } else {
        format!(
            "run_selection:combo:{}",
            selected_targets
                .iter()
                .map(|target| format!("{}:{}", effect_kind, target.identity_key))
                .collect::<Vec<_>>()
                .join("+")
        )
    };
    let effect_label = format!(
        "{} {}",
        effect_verb(reason),
        render_repeated_labels(&labels)
    );
    let score_hint = selected_targets
        .iter()
        .map(|target| target_score_hint(reason, target))
        .sum();
    let duplicate_targets = if matches!(reason, RunPendingChoiceReason::Duplicate) {
        selected_targets
            .iter()
            .map(|target| target.duplicate_evaluation.clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let duplicate_reasons = duplicate_targets
        .iter()
        .flat_map(|target| target.reasons.iter().cloned())
        .collect::<Vec<_>>();
    let duplicate_risks = duplicate_targets
        .iter()
        .flat_map(|target| target.risks.iter().cloned())
        .collect::<Vec<_>>();
    let upgrade_reasons = if matches!(reason, RunPendingChoiceReason::Upgrade) {
        selected_targets
            .iter()
            .filter_map(|target| target.upgrade_candidate.as_ref())
            .flat_map(|candidate| {
                std::iter::once(format!("upgrade_plan: {}", candidate.summary_label())).chain(
                    candidate
                        .evidence
                        .iter()
                        .map(|item| format!("upgrade_evidence: {item}")),
                )
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let upgrade_risks = if matches!(reason, RunPendingChoiceReason::Upgrade) {
        selected_targets
            .iter()
            .filter_map(|target| target.upgrade_candidate.as_ref())
            .flat_map(|candidate| {
                candidate
                    .risks
                    .iter()
                    .map(|item| format!("upgrade_risk: {item}"))
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let transform_reasons = if is_transform_choice(reason) {
        selected_targets
            .iter()
            .flat_map(|target| transform_reasons(&target.card))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let transform_risks = if is_transform_choice(reason) {
        selected_targets
            .iter()
            .flat_map(|target| transform_risks(&target.card))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    DeckMutationPlanCandidateV1 {
        plan_id: format!("deck_mutation:{effect_key}"),
        step: DeckMutationPlanStepV1 {
            kind,
            deck_indices,
            cards,
            command: format_select_command(
                &selected_targets
                    .iter()
                    .map(|target| target.card.deck_index)
                    .collect::<Vec<_>>(),
            ),
            effect_kind,
            effect_key,
            effect_label: effect_label.clone(),
        },
        role: DeckMutationPlanRoleV1::InspectOnly,
        allowed_consumers: AllowedDeckMutationConsumersV1::default(),
        representative_count,
        suppressed_count: representative_count.saturating_sub(1),
        score_hint,
        confidence: 0.0,
        duplicate_targets,
        reasons: selected_targets
            .iter()
            .flat_map(|target| target_loss_reasons(&target.card))
            .chain(duplicate_reasons)
            .chain(upgrade_reasons)
            .chain(transform_reasons)
            .chain(std::iter::once(format!("candidate for {:?}", reason)))
            .collect(),
        risks: selected_targets
            .iter()
            .flat_map(|target| target_risks(&target.card))
            .chain(duplicate_risks)
            .chain(upgrade_risks)
            .chain(transform_risks)
            .collect(),
    }
}

fn evaluate_candidate(
    choice: &RunPendingChoiceState,
    candidate: &mut DeckMutationPlanCandidateV1,
    low_value_available: bool,
    clean_bottle_target_available: bool,
    bonfire_offer_context: Option<BonfireOfferContextV1>,
) {
    evaluate_candidate_for_reason(
        choice.reason,
        candidate,
        low_value_available,
        clean_bottle_target_available,
    );
    if let Some(context) = bonfire_offer_context {
        apply_bonfire_offer_recovery_v1(choice.reason, candidate, context);
    }
}

fn bonfire_offer_context_v1(
    run_state: &RunState,
    choice: &RunPendingChoiceState,
) -> Option<BonfireOfferContextV1> {
    if choice.reason != RunPendingChoiceReason::PurgeNonBottled
        || !matches!(
            choice.source,
            DomainEventSource::Event(EventId::BonfireElementals | EventId::BonfireSpirits)
        )
    {
        return None;
    }

    let route_facts = build_route_window_facts(run_state, RouteWindowFactsConfig::default());
    let budget = build_event_resource_budget(run_state, &route_facts);
    Some(BonfireOfferContextV1 {
        current_hp: run_state.current_hp,
        max_hp: run_state.max_hp,
        reserve_floor: budget.hp.reserve_floor,
        healing_blocked: run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::MarkOfTheBloom),
    })
}

fn apply_bonfire_offer_recovery_v1(
    reason: RunPendingChoiceReason,
    candidate: &mut DeckMutationPlanCandidateV1,
    context: BonfireOfferContextV1,
) {
    let [card] = candidate.step.cards.as_slice() else {
        return;
    };
    let rarity = get_card_definition(card.card).rarity;
    let (projected_hp, projected_max_hp) = match (rarity, context.healing_blocked) {
        (CardRarity::Basic | CardRarity::Curse, _) => (context.current_hp, context.max_hp),
        (CardRarity::Common | CardRarity::Special, false) => {
            ((context.current_hp + 5).min(context.max_hp), context.max_hp)
        }
        (CardRarity::Common | CardRarity::Special, true) => (context.current_hp, context.max_hp),
        (CardRarity::Uncommon, false) => (context.max_hp, context.max_hp),
        (CardRarity::Uncommon, true) => (context.current_hp, context.max_hp),
        (CardRarity::Rare, false) => (context.max_hp + 10, context.max_hp + 10),
        (CardRarity::Rare, true) => (context.current_hp + 10, context.max_hp + 10),
    };
    candidate.reasons.push(format!(
        "bonfire_offer_reward rarity={rarity:?} projected_hp={projected_hp}/{projected_max_hp} reserve_floor={}",
        context.reserve_floor
    ));

    let crosses_survival_reserve = context.current_hp < context.reserve_floor
        && projected_hp >= context.reserve_floor
        && projected_hp > context.current_hp;
    let recovery_hp = projected_hp.saturating_sub(context.current_hp);
    let material_full_recovery = matches!(rarity, CardRarity::Uncommon | CardRarity::Rare)
        && recovery_hp >= context.reserve_floor.max(1);
    let loses_core_function = matches!(
        card.target_loss.tier,
        DeckMutationTargetLossTierV1::CoreFunctional | DeckMutationTargetLossTierV1::Unsupported
    );
    if (!crosses_survival_reserve && !material_full_recovery)
        || candidate.role == DeckMutationPlanRoleV1::Blocked
        || loses_core_function
    {
        return;
    }

    candidate.role = DeckMutationPlanRoleV1::PolicyPreferred;
    candidate.confidence = 0.82;
    candidate.score_hint = candidate
        .score_hint
        .saturating_add(10_000)
        .saturating_add(recovery_hp.saturating_mul(100));
    candidate.allowed_consumers = allowed_consumers(reason, candidate.role, candidate);
    candidate.reasons.push(if crosses_survival_reserve {
        format!(
            "bonfire_offer_crosses_survival_reserve current_hp={} projected_hp={} reserve_floor={}",
            context.current_hp, projected_hp, context.reserve_floor
        )
    } else {
        format!(
            "bonfire_offer_material_full_recovery current_hp={} projected_hp={} recovery_hp={} reserve_floor={}",
            context.current_hp, projected_hp, recovery_hp, context.reserve_floor
        )
    });
}

fn evaluate_candidate_for_reason(
    reason: RunPendingChoiceReason,
    candidate: &mut DeckMutationPlanCandidateV1,
    low_value_available: bool,
    clean_bottle_target_available: bool,
) {
    let has_unsupported_target = candidate
        .step
        .cards
        .iter()
        .any(|card| card.target_class == DeckMutationTargetClassV1::Unsupported);
    let has_functional_target = candidate.step.cards.iter().any(|card| {
        card.target_class == DeckMutationTargetClassV1::Functional
            || card.target_class == DeckMutationTargetClassV1::BasicCore
            || card.target_class == DeckMutationTargetClassV1::Unsupported
    });
    let mutation_choice = is_remove_choice(reason) || is_transform_choice(reason);
    let bottle_choice = is_bottle_choice(reason);
    let max_opening_debt = candidate
        .step
        .cards
        .iter()
        .map(|card| card.opening_hand.debt_tier)
        .max()
        .unwrap_or_default();

    let has_bottle_debt =
        bottle_choice && max_opening_debt >= DeckMutationOpeningHandDebtTierV1::Situational;

    let role = if has_unsupported_target {
        DeckMutationPlanRoleV1::Blocked
    } else if is_transform_choice(reason) {
        transform_candidate_role(candidate, has_functional_target, low_value_available)
    } else if has_bottle_debt && clean_bottle_target_available {
        DeckMutationPlanRoleV1::InspectOnly
    } else if has_bottle_debt {
        DeckMutationPlanRoleV1::RiskyExploration
    } else if mutation_choice && has_functional_target && low_value_available {
        DeckMutationPlanRoleV1::InspectOnly
    } else if mutation_choice && has_functional_target {
        DeckMutationPlanRoleV1::RiskyExploration
    } else if matches!(reason, RunPendingChoiceReason::Upgrade)
        && candidate.score_hint >= POLICY_PREFERRED_UPGRADE_PRIORITY_THRESHOLD
    {
        DeckMutationPlanRoleV1::PolicyPreferred
    } else {
        DeckMutationPlanRoleV1::SafeAlternative
    };

    candidate.role = role;
    candidate.confidence = match role {
        DeckMutationPlanRoleV1::PolicyPreferred => 0.82,
        DeckMutationPlanRoleV1::SafeAlternative => 0.66,
        DeckMutationPlanRoleV1::RiskyExploration => 0.35,
        DeckMutationPlanRoleV1::InspectOnly | DeckMutationPlanRoleV1::Blocked => 0.0,
    };
    candidate.allowed_consumers = allowed_consumers(reason, role, candidate);
    candidate.reasons.push(format!("role={role:?}"));
    if mutation_choice && has_functional_target && low_value_available {
        candidate.reasons.push(
            "functional deck mutation target is inspect-only while low-value targets exist"
                .to_string(),
        );
    }
    if has_bottle_debt {
        candidate.reasons.push(format!(
            "bottle target has {:?} opening-hand debt",
            max_opening_debt
        ));
        if clean_bottle_target_available {
            candidate
                .reasons
                .push("cleaner bottle target exists in candidate pool".to_string());
        } else {
            candidate
                .reasons
                .push("no cleaner bottle target exists; keeping risky fallback branch".to_string());
        }
    }
}

fn allowed_consumers(
    reason: RunPendingChoiceReason,
    role: DeckMutationPlanRoleV1,
    candidate: &DeckMutationPlanCandidateV1,
) -> AllowedDeckMutationConsumersV1 {
    let all_low_value = candidate
        .step
        .cards
        .iter()
        .all(|card| card.target_class.is_low_value_mutation_target());
    let execute_autopilot = match role {
        DeckMutationPlanRoleV1::PolicyPreferred => true,
        DeckMutationPlanRoleV1::SafeAlternative => is_remove_choice(reason) && all_low_value,
        DeckMutationPlanRoleV1::RiskyExploration
        | DeckMutationPlanRoleV1::InspectOnly
        | DeckMutationPlanRoleV1::Blocked => false,
    };

    match role {
        DeckMutationPlanRoleV1::PolicyPreferred => AllowedDeckMutationConsumersV1 {
            execute_autopilot,
            branch_active: true,
            branch_frozen: false,
            inspect: true,
            replay: true,
            human_prompt: false,
        },
        DeckMutationPlanRoleV1::SafeAlternative => AllowedDeckMutationConsumersV1 {
            execute_autopilot,
            branch_active: true,
            branch_frozen: false,
            inspect: true,
            replay: true,
            human_prompt: false,
        },
        DeckMutationPlanRoleV1::RiskyExploration => AllowedDeckMutationConsumersV1 {
            execute_autopilot: false,
            branch_active: true,
            branch_frozen: true,
            inspect: true,
            replay: true,
            human_prompt: true,
        },
        DeckMutationPlanRoleV1::InspectOnly => AllowedDeckMutationConsumersV1 {
            execute_autopilot: false,
            branch_active: false,
            branch_frozen: true,
            inspect: true,
            replay: true,
            human_prompt: true,
        },
        DeckMutationPlanRoleV1::Blocked => AllowedDeckMutationConsumersV1 {
            execute_autopilot: false,
            branch_active: false,
            branch_frozen: false,
            inspect: true,
            replay: false,
            human_prompt: true,
        },
    }
}

fn transform_candidate_role(
    candidate: &DeckMutationPlanCandidateV1,
    has_functional_target: bool,
    low_value_available: bool,
) -> DeckMutationPlanRoleV1 {
    if has_functional_target && low_value_available {
        return DeckMutationPlanRoleV1::InspectOnly;
    }
    candidate
        .step
        .cards
        .iter()
        .map(|card| card.transform.branch_lane)
        .max()
        .unwrap_or(DeckMutationPlanRoleV1::InspectOnly)
}

fn apply_portfolio_suppression(candidates: &mut [DeckMutationPlanCandidateV1], limit: usize) {
    let suppressed = candidates.len().saturating_sub(limit);
    if suppressed == 0 {
        return;
    }
    if let Some(first) = candidates.first_mut() {
        first.suppressed_count += suppressed;
        first.step.effect_label = format!(
            "{} | compiler portfolio cap suppressed {suppressed} candidate(s)",
            first.step.effect_label
        );
    }
}

fn target_class_for_card_mutation(
    reason: RunPendingChoiceReason,
    card: &CombatCard,
) -> DeckMutationTargetClassV1 {
    match reason {
        RunPendingChoiceReason::Upgrade => DeckMutationTargetClassV1::UpgradeTarget,
        RunPendingChoiceReason::Duplicate
        | RunPendingChoiceReason::BottleFlame
        | RunPendingChoiceReason::BottleLightning
        | RunPendingChoiceReason::BottleTornado => DeckMutationTargetClassV1::Functional,
        RunPendingChoiceReason::Purge
        | RunPendingChoiceReason::PurgeNonBottled
        | RunPendingChoiceReason::Transform
        | RunPendingChoiceReason::TransformNonBottled
        | RunPendingChoiceReason::TransformUpgraded => {
            target_class_for_card_mutation_candidate(card)
        }
    }
}

fn target_class_for_card_mutation_candidate(card: &CombatCard) -> DeckMutationTargetClassV1 {
    let definition = get_card_definition(card.id);
    if definition.card_type == CardType::Curse {
        DeckMutationTargetClassV1::Curse
    } else if definition.tags.contains(&CardTag::StarterStrike) {
        DeckMutationTargetClassV1::StarterStrike
    } else if definition.tags.contains(&CardTag::StarterDefend) {
        DeckMutationTargetClassV1::StarterDefend
    } else if definition.rarity == CardRarity::Basic {
        DeckMutationTargetClassV1::BasicCore
    } else {
        DeckMutationTargetClassV1::Functional
    }
}

impl DeckMutationTargetClassV1 {
    fn is_low_value_mutation_target(self) -> bool {
        matches!(
            self,
            DeckMutationTargetClassV1::Curse
                | DeckMutationTargetClassV1::StarterStrike
                | DeckMutationTargetClassV1::StarterDefend
                | DeckMutationTargetClassV1::Basic
        )
    }
}

fn target_score_hint(reason: RunPendingChoiceReason, target: &ExactTarget) -> i32 {
    match reason {
        RunPendingChoiceReason::Duplicate => target.duplicate_evaluation.priority,
        RunPendingChoiceReason::Upgrade => target.upgrade_priority.unwrap_or_default(),
        RunPendingChoiceReason::Purge
        | RunPendingChoiceReason::PurgeNonBottled
        | RunPendingChoiceReason::Transform
        | RunPendingChoiceReason::TransformNonBottled
        | RunPendingChoiceReason::TransformUpgraded => {
            -target_class_rank(target.card.target_class)
                - target_loss_rank(target.card.target_loss.tier)
                + removal_benefit_rank(target.card.removal_benefit.tier)
                - i32::from(target.card.upgrades) * 5
        }
        RunPendingChoiceReason::BottleFlame
        | RunPendingChoiceReason::BottleLightning
        | RunPendingChoiceReason::BottleTornado => target.card.opening_hand.score_hint,
    }
}

fn duplicate_target_evaluation_v1(
    card: &CombatCard,
    run_state: &RunState,
) -> DuplicateTargetEvaluationV1 {
    let def = get_card_definition(card.id);
    let mut evaluation = DuplicateTargetEvaluationV1 {
        card: card.id,
        upgrades: card.upgrades,
        priority: 0,
        premium: false,
        role: DuplicateTargetRoleV1::OrdinaryFiller,
        stack_behavior: DuplicateStackBehaviorV1::Ordinary,
        reasons: Vec::new(),
        risks: Vec::new(),
    };

    if matches!(def.card_type, CardType::Curse | CardType::Status) {
        evaluation.priority = -10_000;
        evaluation.role = DuplicateTargetRoleV1::Reject;
        evaluation.reasons.push("duplicate_role=reject".to_string());
        evaluation
            .risks
            .push("duplicate_rejects_curse_or_status".to_string());
        return evaluation;
    }
    if def.tags.contains(&CardTag::StarterStrike) || def.tags.contains(&CardTag::StarterDefend) {
        evaluation.priority = -500;
        evaluation.reasons.push("duplicate_role=reject".to_string());
        evaluation
            .risks
            .push("duplicate_target_is_starter_card".to_string());
        return evaluation;
    }
    if def.rarity == CardRarity::Basic {
        evaluation.priority = -300;
        evaluation.reasons.push("duplicate_role=reject".to_string());
        evaluation
            .risks
            .push("duplicate_target_is_basic_card".to_string());
        return evaluation;
    }

    let same_card_count = run_state
        .master_deck
        .iter()
        .filter(|deck_card| deck_card.id == card.id)
        .count();
    let upgraded = card.upgrades > 0;
    let exhaust_access_count = exhaust_access_count_v1(run_state);
    let has_exhaust_fuel = exhaust_access_count > 0;
    let has_exhaust_payoff = deck_has_any(run_state, &[CardId::FeelNoPain, CardId::DarkEmbrace]);
    let has_strength_plan = {
        let startup = crate::ai::deck_startup_profile_v1::deck_startup_profile_v1(run_state);
        startup.persistent_strength_source_count > 0
            || startup.convertible_strength_source_count > 0
    };

    match card.id {
        CardId::Offering => {
            evaluation.role = DuplicateTargetRoleV1::SetupAccelerator;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 980;
            evaluation
                .reasons
                .push("duplicate_role=setup_accelerator".to_string());
        }
        CardId::BattleTrance => {
            evaluation.role = DuplicateTargetRoleV1::SetupAccelerator;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 830;
            evaluation
                .reasons
                .push("duplicate_role=setup_accelerator".to_string());
            evaluation
                .risks
                .push("duplicate_draw_lockout_requires_turn_planning".to_string());
        }
        CardId::BurningPact if upgraded || has_exhaust_payoff => {
            evaluation.role = DuplicateTargetRoleV1::SetupAccelerator;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = if upgraded { 860 } else { 760 };
            evaluation
                .reasons
                .push("duplicate_role=setup_accelerator".to_string());
            evaluation
                .reasons
                .push("duplicate_adds_draw_and_controlled_exhaust".to_string());
        }
        CardId::DarkEmbrace | CardId::FeelNoPain if has_exhaust_fuel => {
            evaluation.role = DuplicateTargetRoleV1::EnginePayoff;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = if same_card_count >= 2 { 660 } else { 820 };
            evaluation
                .reasons
                .push("duplicate_role=engine_payoff".to_string());
            evaluation.reasons.push(format!(
                "duplicate_exhaust_fuel_count={exhaust_access_count}"
            ));
            if same_card_count >= 2 {
                evaluation
                    .risks
                    .push("duplicate_engine_payoff_already_redundant".to_string());
            }
        }
        CardId::Corruption => {
            evaluation.role = DuplicateTargetRoleV1::EngineEnabler;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::ConsistencyOnly;
            evaluation.priority = if same_card_count >= 2 {
                360
            } else if has_exhaust_payoff {
                800
            } else {
                680
            };
            evaluation
                .reasons
                .push("duplicate_role=engine_enabler".to_string());
            evaluation
                .risks
                .push("duplicate_non_stacking_power_copy_is_consistency_only".to_string());
        }
        CardId::SecondWind | CardId::FiendFire => {
            evaluation.role = DuplicateTargetRoleV1::EngineEnabler;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = if has_exhaust_payoff { 810 } else { 700 };
            evaluation
                .reasons
                .push("duplicate_role=engine_enabler".to_string());
        }
        CardId::TrueGrit if upgraded && has_exhaust_payoff => {
            evaluation.role = DuplicateTargetRoleV1::EngineEnabler;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 780;
            evaluation
                .reasons
                .push("duplicate_role=engine_enabler".to_string());
            evaluation
                .reasons
                .push("duplicate_requires_upgraded_controlled_exhaust".to_string());
        }
        CardId::Shockwave | CardId::Disarm => {
            evaluation.role = DuplicateTargetRoleV1::CompactBossAnswer;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 880;
            evaluation
                .reasons
                .push("duplicate_role=compact_boss_answer".to_string());
        }
        CardId::Uppercut if upgraded => {
            evaluation.role = DuplicateTargetRoleV1::CompactBossAnswer;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 780;
            evaluation
                .reasons
                .push("duplicate_role=compact_boss_answer".to_string());
            evaluation
                .reasons
                .push("duplicate_requires_upgraded_debuff_density".to_string());
        }
        CardId::Feed | CardId::Reaper => {
            evaluation.role = DuplicateTargetRoleV1::WinCondition;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 830;
            evaluation
                .reasons
                .push("duplicate_role=win_condition".to_string());
        }
        CardId::LimitBreak if upgraded && has_strength_plan => {
            evaluation.role = DuplicateTargetRoleV1::WinCondition;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 820;
            evaluation
                .reasons
                .push("duplicate_role=win_condition".to_string());
            evaluation
                .reasons
                .push("duplicate_strength_plan_present".to_string());
        }
        CardId::RitualDagger if card.misc_value >= 20 => {
            evaluation.role = DuplicateTargetRoleV1::WinCondition;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 850 + card.misc_value.min(40);
            evaluation
                .reasons
                .push("duplicate_role=win_condition".to_string());
            evaluation.reasons.push(format!(
                "duplicate_scaled_ritual_dagger={}",
                card.misc_value
            ));
        }
        CardId::SearingBlow if card.upgrades >= 3 => {
            evaluation.role = DuplicateTargetRoleV1::WinCondition;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Stackable;
            evaluation.priority = 780 + i32::from(card.upgrades) * 20;
            evaluation
                .reasons
                .push("duplicate_role=win_condition".to_string());
            evaluation
                .reasons
                .push(format!("duplicate_scaled_searing_blow=+{}", card.upgrades));
        }
        _ => {
            evaluation.role = DuplicateTargetRoleV1::OrdinaryFiller;
            evaluation.stack_behavior = DuplicateStackBehaviorV1::Ordinary;
            evaluation.priority = ordinary_duplicate_priority_v1(card, run_state);
            evaluation
                .reasons
                .push("duplicate_role=ordinary_filler".to_string());
            if matches!(
                card.id,
                CardId::ShrugItOff | CardId::PommelStrike | CardId::FlameBarrier
            ) {
                evaluation
                    .risks
                    .push("duplicate_good_reward_card_but_not_premium_mirror_target".to_string());
            }
        }
    }

    if upgraded {
        evaluation
            .reasons
            .push("duplicate_target_upgraded".to_string());
        evaluation.priority += 30;
    }
    if same_card_count > 1 {
        evaluation
            .reasons
            .push(format!("duplicate_same_card_count={same_card_count}"));
        if evaluation.stack_behavior == DuplicateStackBehaviorV1::ConsistencyOnly {
            evaluation.priority -= 180;
        } else {
            evaluation.priority -= 60 * (same_card_count.saturating_sub(1) as i32);
        }
    }
    if run_state.master_deck.len() >= 30
        && matches!(
            evaluation.role,
            DuplicateTargetRoleV1::OrdinaryFiller | DuplicateTargetRoleV1::CompactBossAnswer
        )
    {
        evaluation.priority -= 40;
        evaluation
            .risks
            .push("duplicate_large_deck_requires_stronger_access_case".to_string());
    }

    evaluation.premium = evaluation.priority >= PREMIUM_DUPLICATE_TARGET_PRIORITY
        && !matches!(
            evaluation.role,
            DuplicateTargetRoleV1::OrdinaryFiller | DuplicateTargetRoleV1::Reject
        );
    evaluation
        .reasons
        .push(format!("duplicate_priority={}", evaluation.priority));
    evaluation
        .reasons
        .push(format!("duplicate_premium={}", evaluation.premium));
    evaluation
}

fn ordinary_duplicate_priority_v1(card: &CombatCard, run_state: &RunState) -> i32 {
    let mut priority = 120;
    let profile = card_reward_semantic_profile_v1(&RewardCard::new(card.id, card.upgrades));
    for role in profile.roles {
        priority += match role {
            CardRewardSemanticRoleV1::CardDraw => 100,
            CardRewardSemanticRoleV1::CycleAccess => 100,
            CardRewardSemanticRoleV1::EnergySource => 90,
            CardRewardSemanticRoleV1::EnemyStrengthDown
            | CardRewardSemanticRoleV1::Weak
            | CardRewardSemanticRoleV1::Vulnerable => 80,
            CardRewardSemanticRoleV1::Block
            | CardRewardSemanticRoleV1::BlockRetention
            | CardRewardSemanticRoleV1::BlockMultiplier => 50,
            CardRewardSemanticRoleV1::FrontloadDamage | CardRewardSemanticRoleV1::AoeDamage => 35,
            CardRewardSemanticRoleV1::RandomOutput => -50,
            CardRewardSemanticRoleV1::ConditionalPlayability => -80,
            _ => 0,
        };
    }
    if supports_existing_duplicate_package(card.id, run_state) {
        priority += 40;
    }
    priority
}

fn supports_existing_duplicate_package(card: CardId, run_state: &RunState) -> bool {
    match card {
        CardId::BodySlam | CardId::Entrench | CardId::Barricade => deck_has_any(
            run_state,
            &[CardId::BodySlam, CardId::Entrench, CardId::Barricade],
        ),
        CardId::HeavyBlade | CardId::LimitBreak => {
            let startup = crate::ai::deck_startup_profile_v1::deck_startup_profile_v1(run_state);
            startup.persistent_strength_source_count > 0
                || startup.convertible_strength_source_count > 0
        }
        CardId::FeelNoPain | CardId::DarkEmbrace | CardId::Corruption => deck_has_any(
            run_state,
            &[
                CardId::FeelNoPain,
                CardId::DarkEmbrace,
                CardId::Corruption,
                CardId::SecondWind,
                CardId::FiendFire,
                CardId::TrueGrit,
            ],
        ),
        CardId::Evolve | CardId::FireBreathing => deck_has_any(
            run_state,
            &[
                CardId::Evolve,
                CardId::FireBreathing,
                CardId::PowerThrough,
                CardId::WildStrike,
                CardId::RecklessCharge,
            ],
        ),
        _ => false,
    }
}

fn exhaust_access_count_v1(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| {
            matches!(
                card.id,
                CardId::BurningPact
                    | CardId::Corruption
                    | CardId::SecondWind
                    | CardId::FiendFire
                    | CardId::SeverSoul
                    | CardId::TrueGrit
                    | CardId::Havoc
                    | CardId::Exhume
            )
        })
        .count()
}

fn deck_has_any(run_state: &RunState, cards: &[CardId]) -> bool {
    run_state
        .master_deck
        .iter()
        .any(|card| cards.contains(&card.id))
}

fn target_loss_rank(tier: DeckMutationTargetLossTierV1) -> i32 {
    match tier {
        DeckMutationTargetLossTierV1::LowValue => 0,
        DeckMutationTargetLossTierV1::RedundantFunctional => 25,
        DeckMutationTargetLossTierV1::Functional => 60,
        DeckMutationTargetLossTierV1::CoreFunctional => 120,
        DeckMutationTargetLossTierV1::Unsupported => 10_000,
    }
}

fn removal_benefit_rank(tier: DeckMutationRemovalBenefitTierV1) -> i32 {
    match tier {
        DeckMutationRemovalBenefitTierV1::None => 0,
        DeckMutationRemovalBenefitTierV1::LowValueCard => 10,
        DeckMutationRemovalBenefitTierV1::DrawObstruction => 30,
        DeckMutationRemovalBenefitTierV1::PersistentDrawObstruction => 45,
        DeckMutationRemovalBenefitTierV1::ForcedOpeningObstruction => 60,
    }
}

fn target_class_rank(class: DeckMutationTargetClassV1) -> i32 {
    match class {
        DeckMutationTargetClassV1::Curse => 0,
        DeckMutationTargetClassV1::StarterStrike => 10,
        DeckMutationTargetClassV1::StarterDefend => 20,
        DeckMutationTargetClassV1::Basic => 35,
        DeckMutationTargetClassV1::BasicCore => 90,
        DeckMutationTargetClassV1::Functional => 100,
        DeckMutationTargetClassV1::UpgradeTarget => 10_000,
        DeckMutationTargetClassV1::Unsupported => 10_000,
    }
}

fn removal_benefit_for_card_mutation(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
    card: &CombatCard,
    target_class: DeckMutationTargetClassV1,
) -> DeckMutationRemovalBenefitV1 {
    if !(is_remove_choice(reason) || is_transform_choice(reason)) {
        return DeckMutationRemovalBenefitV1::default();
    }

    let definition = get_card_definition(card.id);
    let mut benefit = DeckMutationRemovalBenefitV1::default();
    if let Some(signal) = low_current_output_removal_signal_v1(run_state, card) {
        benefit.tier = DeckMutationRemovalBenefitTierV1::LowValueCard;
        benefit.signals.push(signal);
        return benefit;
    }
    match target_class {
        DeckMutationTargetClassV1::Curse => {
            benefit.tier = if definition.innate {
                DeckMutationRemovalBenefitTierV1::ForcedOpeningObstruction
            } else if definition.cost == -2 && !definition.ethereal {
                DeckMutationRemovalBenefitTierV1::PersistentDrawObstruction
            } else {
                DeckMutationRemovalBenefitTierV1::DrawObstruction
            };
            benefit.signals.push("curse_draw_obstruction".to_string());
            if definition.cost == -2 {
                benefit.signals.push("unplayable_card".to_string());
            }
            if definition.innate {
                benefit
                    .signals
                    .push("forced_opening_hand_obstruction".to_string());
            }
            if definition.ethereal {
                benefit.signals.push("self_clearing_after_draw".to_string());
            }

            if run_state
                .relics
                .iter()
                .any(|relic| relic.id == RelicId::DuVuDoll && !relic.used_up)
            {
                benefit
                    .compensating_signals
                    .push("du_vu_doll_strength".to_string());
            }
            if run_state
                .relics
                .iter()
                .any(|relic| relic.id == RelicId::BlueCandle && !relic.used_up)
            {
                benefit
                    .compensating_signals
                    .push("blue_candle_access".to_string());
            }
            let inventory = DeckRoleInventory::from_deck(&run_state.master_deck);
            if inventory.broad_exhaust_units > 0 && inventory.exhaust_payoff_units > 0 {
                benefit
                    .compensating_signals
                    .push("exhaust_engine_fuel".to_string());
            }
        }
        DeckMutationTargetClassV1::StarterStrike
        | DeckMutationTargetClassV1::StarterDefend
        | DeckMutationTargetClassV1::Basic => {
            benefit.tier = DeckMutationRemovalBenefitTierV1::LowValueCard;
            benefit
                .signals
                .push(format!("low_value_target={target_class:?}"));
        }
        DeckMutationTargetClassV1::BasicCore
        | DeckMutationTargetClassV1::Functional
        | DeckMutationTargetClassV1::UpgradeTarget
        | DeckMutationTargetClassV1::Unsupported => {}
    }
    benefit
}

fn target_loss_for_card_mutation(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
    card: &CombatCard,
    target_class: DeckMutationTargetClassV1,
) -> DeckMutationTargetLossV1 {
    let same_card_count = run_state
        .master_deck
        .iter()
        .filter(|deck_card| deck_card.id == card.id)
        .count();
    let mut loss = DeckMutationTargetLossV1 {
        same_card_count,
        ..DeckMutationTargetLossV1::default()
    };

    if !(is_remove_choice(reason) || is_transform_choice(reason)) {
        loss.tier = DeckMutationTargetLossTierV1::LowValue;
        return loss;
    }

    if target_class == DeckMutationTargetClassV1::Unsupported {
        loss.tier = DeckMutationTargetLossTierV1::Unsupported;
        loss.signals.push("unsupported_target".to_string());
        return loss;
    }

    if target_class.is_low_value_mutation_target() {
        loss.tier = DeckMutationTargetLossTierV1::LowValue;
        loss.signals.push(format!("target_class={target_class:?}"));
        return loss;
    }

    if let Some(signal) = low_current_output_removal_signal_v1(run_state, card) {
        loss.tier = DeckMutationTargetLossTierV1::LowValue;
        loss.signals.push(signal);
        return loss;
    }

    let semantic = card_reward_semantic_profile_v1(&RewardCard::new(card.id, card.upgrades));
    let mut has_core_signal = false;
    for role in &semantic.roles {
        if let Some(signal) = target_loss_signal_for_role(*role) {
            loss.signals.push(signal.to_string());
            if target_loss_role_is_core(*role) {
                has_core_signal = true;
            }
        }
    }
    if removal_breaks_live_strength_package(run_state, card.lineage) {
        loss.signals
            .push("breaks_live_strength_package".to_string());
        has_core_signal = true;
    }
    if removal_breaks_last_high_quality_block(run_state, card.lineage) {
        loss.signals
            .push("removes_last_high_quality_block".to_string());
        has_core_signal = true;
    }
    if card.upgrades > 0 {
        loss.signals.push("upgraded_target".to_string());
    }
    if same_card_count > 1 {
        loss.signals
            .push(format!("same_card_count={same_card_count}"));
    }

    loss.tier = if same_card_count > 1 && !has_core_signal {
        DeckMutationTargetLossTierV1::RedundantFunctional
    } else if same_card_count > 1 {
        DeckMutationTargetLossTierV1::Functional
    } else if has_core_signal {
        DeckMutationTargetLossTierV1::CoreFunctional
    } else {
        DeckMutationTargetLossTierV1::Functional
    };
    loss
}

fn low_current_output_removal_signal_v1(run_state: &RunState, card: &CombatCard) -> Option<String> {
    // Perfected Strike is a deck-shaped attack: its printed definition is not
    // its current output.  A singleton after deck mutation can otherwise look
    // like an irreplaceable functional card merely because it has no duplicate,
    // even when the remaining Strike density makes it an inefficient draw.
    if card.id != CardId::PerfectedStrike {
        return None;
    }

    let definition = get_card_definition(card.id);
    let strike_count = run_state
        .master_deck
        .iter()
        .filter(|deck_card| {
            get_card_definition(deck_card.id)
                .tags
                .contains(&CardTag::Strike)
        })
        .count() as i32;
    let magic = definition.base_magic
        + if card.upgrades > 0 {
            definition.upgrade_magic
        } else {
            0
        };
    let damage = definition.base_damage + magic * strike_count;
    let dense = damage >= 20 || (definition.cost <= 1 && damage >= 10);
    (!dense).then(|| {
        format!(
            "low_current_output=PerfectedStrike:{damage}_damage_at_{}_energy_with_{strike_count}_strikes",
            definition.cost
        )
    })
}

fn removal_breaks_live_strength_package(
    run_state: &RunState,
    removed_lineage: CardLineageId,
) -> bool {
    let before = DeckRoleInventory::from_deck(&run_state.master_deck);
    let before_has_source =
        before.strength_source_units > 0 || before.conditional_strength_source_units > 0;
    let before_live = before_has_source && before.strength_multiplier_units > 0;
    if !before_live {
        return false;
    }

    let remaining = run_state
        .master_deck
        .iter()
        .filter(|card| card.lineage != removed_lineage)
        .cloned()
        .collect::<Vec<_>>();
    let after = DeckRoleInventory::from_deck(&remaining);
    let after_has_source =
        after.strength_source_units > 0 || after.conditional_strength_source_units > 0;
    !(after_has_source && after.strength_multiplier_units > 0)
}

fn removal_breaks_last_high_quality_block(
    run_state: &RunState,
    removed_lineage: CardLineageId,
) -> bool {
    let before = DeckRoleInventory::from_deck(&run_state.master_deck);
    if before.high_quality_block_units == 0 {
        return false;
    }

    let remaining = run_state
        .master_deck
        .iter()
        .filter(|card| card.lineage != removed_lineage)
        .cloned()
        .collect::<Vec<_>>();
    DeckRoleInventory::from_deck(&remaining).high_quality_block_units == 0
}

fn opening_hand_profile_for_card_mutation(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
    card: &CombatCard,
) -> DeckMutationOpeningHandProfileV1 {
    let profile = opening_hand_target_profile_for_card_v1(run_state, reason, card);
    DeckMutationOpeningHandProfileV1 {
        debt_tier: map_opening_hand_debt_tier(profile.debt_tier),
        score_hint: profile.score_hint,
        signals: profile.signals,
    }
}

fn transform_profile_for_card_mutation(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
    _card: &CombatCard,
    target_class: DeckMutationTargetClassV1,
) -> DeckMutationTransformProfileV1 {
    if !is_transform_choice(reason) {
        return DeckMutationTransformProfileV1::default();
    }

    let mut profile = DeckMutationTransformProfileV1 {
        random_addition_band: TransformRandomAdditionBandV1::Mixed,
        variance_risk: TransformVarianceRiskV1::Medium,
        branch_lane: DeckMutationPlanRoleV1::RiskyExploration,
        signals: vec!["transform_adds_random_card".to_string()],
    };

    match target_class {
        DeckMutationTargetClassV1::Curse
        | DeckMutationTargetClassV1::StarterStrike
        | DeckMutationTargetClassV1::StarterDefend
        | DeckMutationTargetClassV1::Basic => {
            profile.random_addition_band = TransformRandomAdditionBandV1::LikelyBetterThanTarget;
            profile.branch_lane = DeckMutationPlanRoleV1::SafeAlternative;
            profile
                .signals
                .push(format!("target_class={target_class:?}"));
        }
        DeckMutationTargetClassV1::BasicCore | DeckMutationTargetClassV1::Functional => {
            profile.random_addition_band = TransformRandomAdditionBandV1::Mixed;
            profile.variance_risk = TransformVarianceRiskV1::High;
            profile.branch_lane = DeckMutationPlanRoleV1::RiskyExploration;
            profile
                .signals
                .push("functional_transform_requires_strategy_context".to_string());
        }
        DeckMutationTargetClassV1::Unsupported | DeckMutationTargetClassV1::UpgradeTarget => {
            profile.random_addition_band = TransformRandomAdditionBandV1::Mixed;
            profile.variance_risk = TransformVarianceRiskV1::High;
            profile.branch_lane = DeckMutationPlanRoleV1::Blocked;
            profile
                .signals
                .push("unsupported_transform_target".to_string());
        }
    }

    match target_class {
        DeckMutationTargetClassV1::StarterStrike => profile
            .signals
            .push("short_term_frontload_loss_risk".to_string()),
        DeckMutationTargetClassV1::StarterDefend => profile
            .signals
            .push("short_term_block_loss_risk".to_string()),
        _ => {}
    }
    if run_state.act_num == 1 && run_state.floor_num <= 7 {
        profile
            .signals
            .push("act1_early_transform_variance".to_string());
        if matches!(
            target_class,
            DeckMutationTargetClassV1::StarterStrike | DeckMutationTargetClassV1::StarterDefend
        ) {
            profile.variance_risk = TransformVarianceRiskV1::Medium;
        }
    }

    profile
}

fn map_opening_hand_debt_tier(tier: OpeningHandDebtTierV1) -> DeckMutationOpeningHandDebtTierV1 {
    match tier {
        OpeningHandDebtTierV1::None => DeckMutationOpeningHandDebtTierV1::None,
        OpeningHandDebtTierV1::Mild => DeckMutationOpeningHandDebtTierV1::Mild,
        OpeningHandDebtTierV1::Situational => DeckMutationOpeningHandDebtTierV1::Situational,
        OpeningHandDebtTierV1::High => DeckMutationOpeningHandDebtTierV1::High,
    }
}

fn target_loss_signal_for_role(role: CardRewardSemanticRoleV1) -> Option<&'static str> {
    match role {
        CardRewardSemanticRoleV1::FrontloadDamage => Some("frontload_damage"),
        CardRewardSemanticRoleV1::ContextDependentDamage => Some("context_dependent_damage"),
        CardRewardSemanticRoleV1::AoeDamage => Some("aoe_damage"),
        CardRewardSemanticRoleV1::Block => Some("block"),
        CardRewardSemanticRoleV1::BlockRetention => Some("block_retention"),
        CardRewardSemanticRoleV1::BlockMultiplier => Some("block_multiplier"),
        CardRewardSemanticRoleV1::CardDraw => Some("card_draw"),
        CardRewardSemanticRoleV1::CycleAccess => Some("cycle_access"),
        CardRewardSemanticRoleV1::DiscardPileTopdeckAccess => Some("discard_pile_topdeck_access"),
        CardRewardSemanticRoleV1::HandTopdeckSelection => Some("hand_topdeck_selection"),
        CardRewardSemanticRoleV1::CardInstanceDamageGrowth => Some("card_instance_damage_growth"),
        CardRewardSemanticRoleV1::HpLossCostReduction => Some("hp_loss_cost_reduction"),
        CardRewardSemanticRoleV1::ConditionalCardAccess => Some("conditional_card_access"),
        CardRewardSemanticRoleV1::HandCopy => Some("hand_copy"),
        CardRewardSemanticRoleV1::TopDeckPlay => Some("top_deck_play"),
        CardRewardSemanticRoleV1::EnergySource => Some("energy_source"),
        CardRewardSemanticRoleV1::EnergyOnExhaust => Some("energy_on_exhaust"),
        CardRewardSemanticRoleV1::Vulnerable => Some("vulnerable"),
        CardRewardSemanticRoleV1::Weak => Some("weak"),
        CardRewardSemanticRoleV1::EnemyStrengthDown => Some("enemy_strength_down"),
        CardRewardSemanticRoleV1::ScalingSource => Some("scaling_source"),
        CardRewardSemanticRoleV1::TemporaryStrengthBurst => Some("temporary_strength_burst"),
        CardRewardSemanticRoleV1::StrengthPayoff => Some("strength_payoff"),
        CardRewardSemanticRoleV1::BlockPayoff => Some("block_payoff"),
        CardRewardSemanticRoleV1::StrikePayoff => Some("strike_payoff"),
        CardRewardSemanticRoleV1::UpgradePayoff => Some("upgrade_payoff"),
        CardRewardSemanticRoleV1::ExhaustGenerator => Some("exhaust_generator"),
        CardRewardSemanticRoleV1::ExhaustReuse => Some("exhaust_reuse"),
        CardRewardSemanticRoleV1::ExhaustPayoff => Some("exhaust_payoff"),
        CardRewardSemanticRoleV1::StatusGenerator => Some("status_generator"),
        CardRewardSemanticRoleV1::StatusPayoff => Some("status_payoff"),
        CardRewardSemanticRoleV1::SelfDamagePayoff => Some("self_damage_payoff"),
        CardRewardSemanticRoleV1::CombatExternalPayoff => Some("combat_external_payoff"),
        CardRewardSemanticRoleV1::CombatSustain => Some("combat_sustain"),
        CardRewardSemanticRoleV1::ActionAmplifier => Some("action_amplifier"),
        CardRewardSemanticRoleV1::PackagePayoff => None,
        CardRewardSemanticRoleV1::RandomOutput => Some("random_output"),
        CardRewardSemanticRoleV1::ConditionalPlayability => Some("conditional_playability"),
    }
}

fn target_loss_role_is_core(role: CardRewardSemanticRoleV1) -> bool {
    matches!(
        role,
        CardRewardSemanticRoleV1::BlockRetention
            | CardRewardSemanticRoleV1::BlockMultiplier
            | CardRewardSemanticRoleV1::CardDraw
            | CardRewardSemanticRoleV1::CycleAccess
            | CardRewardSemanticRoleV1::EnergySource
            | CardRewardSemanticRoleV1::EnergyOnExhaust
            | CardRewardSemanticRoleV1::Vulnerable
            | CardRewardSemanticRoleV1::Weak
            | CardRewardSemanticRoleV1::EnemyStrengthDown
            | CardRewardSemanticRoleV1::ScalingSource
            | CardRewardSemanticRoleV1::BlockPayoff
            | CardRewardSemanticRoleV1::ExhaustGenerator
            | CardRewardSemanticRoleV1::ExhaustReuse
            | CardRewardSemanticRoleV1::ExhaustPayoff
            | CardRewardSemanticRoleV1::StatusPayoff
            | CardRewardSemanticRoleV1::SelfDamagePayoff
            | CardRewardSemanticRoleV1::CombatExternalPayoff
            | CardRewardSemanticRoleV1::CombatSustain
            | CardRewardSemanticRoleV1::CardInstanceDamageGrowth
            | CardRewardSemanticRoleV1::HpLossCostReduction
            | CardRewardSemanticRoleV1::ConditionalCardAccess
            | CardRewardSemanticRoleV1::HandCopy
            | CardRewardSemanticRoleV1::TopDeckPlay
    )
}

fn target_loss_reasons(card: &DeckMutationCardSnapshotV1) -> Vec<String> {
    let mut reasons = vec![format!(
        "target_loss={:?} same_card_count={}",
        card.target_loss.tier, card.target_loss.same_card_count
    )];
    if !card.target_loss.signals.is_empty() {
        reasons.push(format!(
            "target_loss_signals={}",
            card.target_loss.signals.join(",")
        ));
    }
    if card.opening_hand.debt_tier != DeckMutationOpeningHandDebtTierV1::None {
        reasons.push(format!(
            "opening_hand_debt={:?}",
            card.opening_hand.debt_tier
        ));
    }
    if !card.opening_hand.signals.is_empty() {
        reasons.push(format!(
            "opening_hand_signals={}",
            card.opening_hand.signals.join(",")
        ));
    }
    reasons
}

fn transform_reasons(card: &DeckMutationCardSnapshotV1) -> Vec<String> {
    if card.transform.random_addition_band == TransformRandomAdditionBandV1::NotTransform {
        return Vec::new();
    }
    let mut reasons = vec![
        format!(
            "transform_random_addition_band={:?}",
            card.transform.random_addition_band
        ),
        format!("transform_branch_lane={:?}", card.transform.branch_lane),
    ];
    if !card.transform.signals.is_empty() {
        reasons.push(format!(
            "transform_signals={}",
            card.transform.signals.join(",")
        ));
    }
    reasons
}

fn transform_risks(card: &DeckMutationCardSnapshotV1) -> Vec<String> {
    if card.transform.variance_risk == TransformVarianceRiskV1::NotTransform {
        return Vec::new();
    }
    let mut risks = vec![format!(
        "transform_variance_risk={:?}",
        card.transform.variance_risk
    )];
    if card
        .transform
        .signals
        .iter()
        .any(|signal| signal == "short_term_frontload_loss_risk")
    {
        risks.push("transforming this target can reduce immediate attack density".to_string());
    }
    if card
        .transform
        .signals
        .iter()
        .any(|signal| signal == "short_term_block_loss_risk")
    {
        risks.push("transforming this target can reduce immediate block density".to_string());
    }
    risks
}

fn target_risks(card: &DeckMutationCardSnapshotV1) -> Vec<String> {
    let mut risks = match card.target_class {
        DeckMutationTargetClassV1::StarterStrike => {
            vec!["mutating starter attacks can reduce short-term frontload".to_string()]
        }
        DeckMutationTargetClassV1::StarterDefend => {
            vec!["mutating starter blocks can reduce short-term defense".to_string()]
        }
        DeckMutationTargetClassV1::Functional => {
            vec!["functional card mutation requires explicit strategy context".to_string()]
        }
        DeckMutationTargetClassV1::BasicCore => {
            vec!["basic core card mutation requires explicit strategy context".to_string()]
        }
        DeckMutationTargetClassV1::Unsupported => {
            vec!["unsupported target is blocked from automatic consumers".to_string()]
        }
        _ => Vec::new(),
    };
    match card.target_loss.tier {
        DeckMutationTargetLossTierV1::CoreFunctional => {
            risks.push("target carries singleton core deck function".to_string());
        }
        DeckMutationTargetLossTierV1::Functional => {
            risks.push("target carries replaceable but real deck function".to_string());
        }
        DeckMutationTargetLossTierV1::Unsupported => {
            risks.push("target loss profile is unsupported".to_string());
        }
        DeckMutationTargetLossTierV1::LowValue
        | DeckMutationTargetLossTierV1::RedundantFunctional => {}
    }
    match card.opening_hand.debt_tier {
        DeckMutationOpeningHandDebtTierV1::High => {
            risks.push("bottle target creates high opening-hand debt".to_string());
        }
        DeckMutationOpeningHandDebtTierV1::Situational => {
            risks.push("bottle target is context-dependent in the opening hand".to_string());
        }
        DeckMutationOpeningHandDebtTierV1::Mild | DeckMutationOpeningHandDebtTierV1::None => {}
    }
    risks
}

fn is_remove_choice(reason: RunPendingChoiceReason) -> bool {
    matches!(
        reason,
        RunPendingChoiceReason::Purge | RunPendingChoiceReason::PurgeNonBottled
    )
}

fn is_transform_choice(reason: RunPendingChoiceReason) -> bool {
    matches!(
        reason,
        RunPendingChoiceReason::Transform
            | RunPendingChoiceReason::TransformNonBottled
            | RunPendingChoiceReason::TransformUpgraded
    )
}

fn is_bottle_choice(reason: RunPendingChoiceReason) -> bool {
    matches!(
        reason,
        RunPendingChoiceReason::BottleFlame
            | RunPendingChoiceReason::BottleLightning
            | RunPendingChoiceReason::BottleTornado
    )
}

fn deck_mutation_kind(reason: RunPendingChoiceReason) -> DeckMutationKindV1 {
    match reason {
        RunPendingChoiceReason::Purge | RunPendingChoiceReason::PurgeNonBottled => {
            DeckMutationKindV1::Remove
        }
        RunPendingChoiceReason::Upgrade => DeckMutationKindV1::Upgrade,
        RunPendingChoiceReason::Transform
        | RunPendingChoiceReason::TransformNonBottled
        | RunPendingChoiceReason::TransformUpgraded => DeckMutationKindV1::Transform,
        RunPendingChoiceReason::Duplicate => DeckMutationKindV1::Duplicate,
        RunPendingChoiceReason::BottleFlame
        | RunPendingChoiceReason::BottleLightning
        | RunPendingChoiceReason::BottleTornado => DeckMutationKindV1::Bottle,
    }
}

fn effect_kind(reason: RunPendingChoiceReason) -> &'static str {
    match reason {
        RunPendingChoiceReason::Purge | RunPendingChoiceReason::PurgeNonBottled => "remove_card",
        RunPendingChoiceReason::Upgrade => "upgrade_card",
        RunPendingChoiceReason::Transform
        | RunPendingChoiceReason::TransformNonBottled
        | RunPendingChoiceReason::TransformUpgraded => "transform_card",
        RunPendingChoiceReason::Duplicate => "duplicate_card",
        RunPendingChoiceReason::BottleFlame
        | RunPendingChoiceReason::BottleLightning
        | RunPendingChoiceReason::BottleTornado => "bottle_card",
    }
}

fn effect_verb(reason: RunPendingChoiceReason) -> &'static str {
    match reason {
        RunPendingChoiceReason::Purge | RunPendingChoiceReason::PurgeNonBottled => "remove",
        RunPendingChoiceReason::Upgrade => "upgrade",
        RunPendingChoiceReason::Transform
        | RunPendingChoiceReason::TransformNonBottled
        | RunPendingChoiceReason::TransformUpgraded => "transform",
        RunPendingChoiceReason::Duplicate => "duplicate",
        RunPendingChoiceReason::BottleFlame
        | RunPendingChoiceReason::BottleLightning
        | RunPendingChoiceReason::BottleTornado => "bottle",
    }
}

fn card_label(card: CardId, upgrades: u8) -> String {
    let name = get_card_definition(card).name;
    match upgrades {
        0 => name.to_string(),
        1 => format!("{name}+"),
        upgrades => format!("{name}+{upgrades}"),
    }
}

fn render_repeated_labels(labels: &[&str]) -> String {
    let mut runs = Vec::<(&str, usize)>::new();
    for label in labels {
        if let Some((_, count)) = runs
            .iter_mut()
            .find(|(existing_label, _)| existing_label == label)
        {
            *count += 1;
        } else {
            runs.push((label, 1));
        }
    }
    runs.into_iter()
        .map(|(label, count)| {
            if count > 1 {
                format!("{label} x{count}")
            } else {
                label.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_select_command(indices: &[usize]) -> String {
    format!(
        "select {}",
        indices
            .iter()
            .map(|idx| idx.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn card_stat_identity_key(card: &CombatCard) -> String {
    let mut key = format!("{}:{}", card.id.stable_id(), card.upgrades);
    let default = crate::runtime::combat::CardBlueprint::fresh(card.id);
    let mut extras = Vec::new();

    if card.misc_value != default.misc_value {
        extras.push(format!("misc={}", card.misc_value));
    }
    if let Some(value) = card.base_damage_override {
        extras.push(format!("base_damage={value}"));
    }
    if let Some(value) = card.base_block_override {
        extras.push(format!("base_block={value}"));
    }
    if card.cost_modifier != 0 {
        extras.push(format!("cost_modifier={}", card.cost_modifier));
    }

    if !extras.is_empty() {
        key.push(':');
        key.push_str(&extras.join(":"));
    }
    key
}
