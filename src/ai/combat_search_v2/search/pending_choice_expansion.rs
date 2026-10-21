use std::time::Instant;

use super::super::*;
use super::child_expansion::{expand_ordered_child, ChildExpansionInput, ChildExpansionOutcome};
use super::loop_state::SearchLoopState;
use super::node_child_observers::initialize_node_child_observers;
use super::node_pruning::{apply_node_prune_gates, NodePruneOutcome};
use crate::ai::combat_search_v2::pending_choice_ordering::PendingChoiceOrderingHint;

pub(super) enum PendingChoiceTransactionOutcome {
    NotApplicable(SearchNode),
    Handled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PendingChoicePrefixOutcome {
    Handled,
    Stop,
}

pub(super) fn pending_choice_prefix_owned(
    loop_state: &SearchLoopState,
    engine: &EngineState,
) -> bool {
    if !loop_state.owns_engine_pending_choice_prefixes {
        return false;
    }
    let EngineState::PendingChoice(choice) = engine else {
        return false;
    };
    PendingChoiceActionFamily::from_choice(choice).is_some()
}

/// Claims a concrete pending-choice parent exactly once.  The parent still
/// passes concrete transposition/dominance pruning, while its virtual prefixes
/// are queued as distinct work and never enter those tables themselves.
pub(super) fn start_pending_choice_transaction_if_owned(
    loop_state: &mut SearchLoopState,
    node: SearchNode,
    config: &CombatSearchV2Config,
) -> PendingChoiceTransactionOutcome {
    if !pending_choice_prefix_owned(loop_state, &node.engine)
        || terminal_label(&node.engine, &node.combat) != SearchTerminalLabel::Unresolved
    {
        return PendingChoiceTransactionOutcome::NotApplicable(node);
    }
    let EngineState::PendingChoice(choice) = &node.engine else {
        unreachable!("ownership check requires a pending choice");
    };
    let Some(family) = PendingChoiceActionFamily::from_choice(choice) else {
        return PendingChoiceTransactionOutcome::NotApplicable(node);
    };
    if family.omits_ordered_variants() {
        loop_state.mark_action_surface_incomplete();
    }

    loop_state.remember_best_frontier(&node);
    if apply_node_prune_gates(loop_state, &node, config) == NodePruneOutcome::Pruned {
        return PendingChoiceTransactionOutcome::Handled;
    }

    loop_state.record_node_expanded(&node);
    loop_state.performance.pending_choice_transactions_started = loop_state
        .performance
        .pending_choice_transactions_started
        .saturating_add(1);
    let profile = summarize_pending_choice(&node.engine);
    loop_state
        .diagnostics
        .observe_pending_choice(profile.as_ref());

    let work_items = family.into_work_items();
    loop_state.begin_unmaterialized_root_pending_work(&node, work_items.len());
    if work_items.is_empty() {
        loop_state.record_unresolved_leaf(&node);
        return PendingChoiceTransactionOutcome::Handled;
    }
    loop_state.performance.pending_choice_prefixes_generated = loop_state
        .performance
        .pending_choice_prefixes_generated
        .saturating_add(
            work_items
                .iter()
                .map(PendingChoiceActionWork::len)
                .sum::<usize>() as u64,
        );
    for work in work_items {
        loop_state.push_pending_choice_work(node.clone(), work);
    }
    PendingChoiceTransactionOutcome::Handled
}

pub(super) fn expand_pending_choice_prefix(
    loop_state: &mut SearchLoopState,
    node: SearchNode,
    mut work: PendingChoiceActionWork,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    pending_choice_prefix_limit: usize,
    deadline: Option<Instant>,
) -> PendingChoicePrefixOutcome {
    while !work.is_empty() {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            loop_state.mark_deadline_hit();
            loop_state.push_pending_choice_work(node, work);
            return PendingChoicePrefixOutcome::Stop;
        }
        if loop_state.performance.pending_choice_prefixes_expanded as usize
            >= pending_choice_prefix_limit
        {
            loop_state.mark_action_prefix_budget_hit();
            loop_state.push_pending_choice_work(node, work);
            return PendingChoicePrefixOutcome::Stop;
        }
        let prefix = work.pop().expect("non-empty action-prefix work");
        loop_state.performance.pending_choice_prefixes_expanded = loop_state
            .performance
            .pending_choice_prefixes_expanded
            .saturating_add(1);

        if let Some(input) = prefix.complete_input() {
            let action_ordinal = work.current_action_ordinal();
            let Some(outcome) = submit_completed_prefix(
                loop_state,
                &node,
                action_ordinal,
                input,
                stepper,
                config,
                deadline,
            ) else {
                loop_state
                    .performance
                    .pending_choice_complete_prefixes_rejected = loop_state
                    .performance
                    .pending_choice_complete_prefixes_rejected
                    .saturating_add(1);
                if work.is_empty() {
                    finish_exhausted_work(loop_state, &node, &work);
                } else {
                    loop_state.push_pending_choice_work(node, work);
                }
                return PendingChoicePrefixOutcome::Handled;
            };
            work.note_legal_input();
            if outcome == ChildExpansionOutcome::DeadlineReached {
                work.push_next(prefix);
                loop_state.push_pending_choice_work(node, work);
                return PendingChoicePrefixOutcome::Stop;
            }
            if outcome == ChildExpansionOutcome::Applied {
                work.note_action_applied();
                loop_state
                    .performance
                    .pending_choice_complete_actions_submitted = loop_state
                    .performance
                    .pending_choice_complete_actions_submitted
                    .saturating_add(1);
            }
            if !work.is_empty() {
                loop_state.push_pending_choice_work(node, work);
            } else {
                finish_exhausted_work(loop_state, &node, &work);
            }
            return if loop_state.exhausted {
                PendingChoicePrefixOutcome::Stop
            } else {
                PendingChoicePrefixOutcome::Handled
            };
        }

        let include_first = prefer_include(&node, &prefix);
        let branches = prefix.expand(include_first);
        loop_state.performance.pending_choice_prefixes_generated = loop_state
            .performance
            .pending_choice_prefixes_generated
            .saturating_add(branches.len() as u64);
        work.push_ordered(branches);
    }
    finish_exhausted_work(loop_state, &node, &work);
    PendingChoicePrefixOutcome::Handled
}

fn submit_completed_prefix(
    loop_state: &mut SearchLoopState,
    node: &SearchNode,
    action_ordinal: usize,
    input: ClientInput,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> Option<ChildExpansionOutcome> {
    let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
    let choice = stepper.choice_for_legal_input(&position, &input)?;
    let pending_choice = summarize_pending_choice(&node.engine);
    let action_prior_state_hash = config
        .root_action_prior
        .as_ref()
        .filter(|prior| !prior.is_empty())
        .map(|_| combat_exact_state_hash_v2(&node.engine, &node.combat));
    let (mut turn_branching, mut turn_local_dominance) = initialize_node_child_observers(node, 1);
    let outcome = expand_ordered_child(
        loop_state,
        &mut turn_branching,
        &mut turn_local_dominance,
        ChildExpansionInput {
            parent: node,
            position: &position,
            ordered_choice: IndexedActionChoice {
                original_action_id: action_ordinal,
                choice,
            },
            action_ordering_frontier_hint: 0,
            action_prior_state_hash: action_prior_state_hash.as_deref(),
            pending_choice: pending_choice.as_ref(),
            stepper,
            config,
            deadline,
        },
    );
    loop_state
        .diagnostics
        .observe_turn_branching(&turn_branching);
    loop_state
        .diagnostics
        .observe_turn_local_dominance(&turn_local_dominance);

    Some(outcome)
}

fn finish_exhausted_work(
    loop_state: &mut SearchLoopState,
    node: &SearchNode,
    work: &PendingChoiceActionWork,
) {
    debug_assert!(work.is_empty());
    if !work.legal_input_seen() {
        // The prefix tree is virtual work, not a set of concrete leaf states.
        // If every complete prefix was rejected, retain the concrete parent
        // once instead of inflating unresolved-leaf counts exponentially.
        loop_state.record_unresolved_leaf(node);
    }
    loop_state.finish_unmaterialized_root_pending_work(node);
}

fn prefer_include(node: &SearchNode, prefix: &PendingChoiceActionPrefix) -> bool {
    let Some((included, excluded)) = prefix.probe_inputs() else {
        return true;
    };
    let included = pending_choice_ordering_hint(&node.engine, &node.combat, &included)
        .map(hint_rank)
        .unwrap_or_default();
    let excluded = pending_choice_ordering_hint(&node.engine, &node.combat, &excluded)
        .map(hint_rank)
        .unwrap_or_default();
    included >= excluded
}

fn hint_rank(hint: PendingChoiceOrderingHint) -> (i32, i32, i32) {
    (hint.primary, hint.secondary, hint.selected_count_tiebreak)
}
