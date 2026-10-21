use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ChildPreflightOutcome {
    Continue {
        potion_tactical_priority: Option<i32>,
    },
    Advanced,
    DeadlineReached,
}

pub(super) fn prepare_child_for_expansion(
    loop_state: &mut SearchLoopState,
    parent: &SearchNode,
    ordered_choice: &IndexedActionChoice,
    deadline: Option<Instant>,
) -> ChildPreflightOutcome {
    let potion_tactical_priority =
        potions::semantic_potion_tactical_priority(&parent.combat, &ordered_choice.choice.input);
    if loop_state
        .plugins
        .potion
        .max_potions_used
        .is_some_and(|max| {
            parent.potions_used >= max && is_use_potion_input(&ordered_choice.choice.input)
        })
    {
        loop_state.record_potion_budget_cut();
        return ChildPreflightOutcome::Advanced;
    }
    if deadline.is_some_and(|limit| Instant::now() >= limit) {
        loop_state.mark_deadline_hit();
        return ChildPreflightOutcome::DeadlineReached;
    }
    ChildPreflightOutcome::Continue {
        potion_tactical_priority,
    }
}
