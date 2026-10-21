use super::*;

mod context;
mod decision;
mod proposals;
mod semantics;

pub(super) use proposals::semantic_potion_action_allowed;

pub(super) fn semantic_potion_tactical_priority(
    combat: &CombatState,
    input: &ClientInput,
) -> Option<i32> {
    proposals::semantic_potion_tactical_role(combat, input).map(|role| role.priority_rank())
}

#[cfg(test)]
mod tests;
