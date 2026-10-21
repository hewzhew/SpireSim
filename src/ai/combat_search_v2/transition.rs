use super::*;

pub(super) fn filtered_legal_actions(
    legal: Vec<CombatActionChoice>,
    potion_policy: CombatSearchV2PotionPolicy,
    allowed_potion_slots: Option<u64>,
    allow_potion_discard: Option<bool>,
    combat: &CombatState,
) -> Vec<CombatActionChoice> {
    let legal = legal
        .into_iter()
        .filter(|choice| {
            crate::sim::combat_action_surface::automatic_client_input_is_admitted(
                combat,
                &choice.input,
            )
        })
        .filter(|choice| potion_slot_is_allowed(&choice.input, allowed_potion_slots))
        .filter(|choice| {
            allow_potion_discard.unwrap_or(matches!(potion_policy, CombatSearchV2PotionPolicy::All))
                || !matches!(choice.input, ClientInput::DiscardPotion(_))
        })
        .collect::<Vec<_>>();
    match potion_policy {
        CombatSearchV2PotionPolicy::All => legal,
        CombatSearchV2PotionPolicy::Never => legal
            .into_iter()
            .filter(|choice| !is_potion_input(&choice.input))
            .collect(),
        CombatSearchV2PotionPolicy::SemanticBudgeted => legal
            .into_iter()
            .filter(|choice| {
                !is_potion_input(&choice.input)
                    || potions::semantic_potion_action_allowed(combat, &choice.input)
            })
            .collect(),
    }
}

fn potion_slot_is_allowed(input: &ClientInput, allowed_potion_slots: Option<u64>) -> bool {
    let Some(allowed_potion_slots) = allowed_potion_slots else {
        return true;
    };
    let slot = match input {
        ClientInput::UsePotion { potion_index, .. } => Some(*potion_index),
        ClientInput::DiscardPotion(slot) => Some(*slot),
        _ => None,
    };
    slot.is_none_or(|slot| {
        u32::try_from(slot)
            .ok()
            .and_then(|slot| 1_u64.checked_shl(slot))
            .is_some_and(|slot| allowed_potion_slots & slot != 0)
    })
}

pub(super) fn is_potion_input(input: &ClientInput) -> bool {
    matches!(
        input,
        ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
    )
}

pub(super) fn is_use_potion_input(input: &ClientInput) -> bool {
    matches!(input, ClientInput::UsePotion { .. })
}

pub(super) fn terminal_label(engine: &EngineState, combat: &CombatState) -> SearchTerminalLabel {
    if combat.runtime.combat_smoked {
        return SearchTerminalLabel::Escape;
    }
    match combat_terminal(engine, combat) {
        CombatTerminal::Win => SearchTerminalLabel::Win,
        CombatTerminal::Loss => SearchTerminalLabel::Loss,
        CombatTerminal::Unresolved => SearchTerminalLabel::Unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::core::RunResult;

    #[test]
    fn smoke_bomb_exit_has_a_distinct_non_win_search_terminal() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.runtime.combat_smoked = true;

        assert_eq!(
            terminal_label(&EngineState::GameOver(RunResult::Victory), &combat),
            SearchTerminalLabel::Escape,
        );
    }
}
