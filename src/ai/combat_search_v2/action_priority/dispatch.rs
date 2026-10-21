use super::super::{combat_search_phase_profile, pending_choice_ordering_hint, potions};
use super::constants::{
    ROLE_DISCARD_POTION, ROLE_END_TURN, ROLE_TACTICAL_POTION_BASE, ROLE_UTILITY_PLAY,
};
use super::pending_choice::pending_choice_role_rank;
use super::priority::ActionOrderingPriority;
use super::role::ActionOrderingRole;
use crate::runtime::combat::CombatState;
use crate::state::core::{ClientInput, EngineState};

pub(in crate::ai::combat_search_v2) fn priority_for_input_with_plugins(
    engine: &EngineState,
    combat: &CombatState,
    input: &ClientInput,
    plugins: super::super::CombatSearchActionOrderingPlugins<'_>,
) -> ActionOrderingPriority {
    if let Some(hint) = pending_choice_ordering_hint(engine, combat, input) {
        let (role, role_rank) = pending_choice_role_rank(hint.role);
        return ActionOrderingPriority {
            role,
            role_rank,
            pending_choice_primary: hint.primary,
            pending_choice_secondary: hint.secondary,
            pending_choice_selected_count: hint.selected_count_tiebreak,
            policy_log2_bias: hint.policy_log2_bias,
            ..ActionOrderingPriority::neutral(role)
        };
    }

    if !matches!(engine, EngineState::CombatPlayerTurn) {
        return ActionOrderingPriority::neutral(ActionOrderingRole::Neutral);
    }

    match input {
        ClientInput::PlayCard { card_index, target } => {
            let phase_profile = combat_search_phase_profile(engine, combat);
            super::play_card::priority_for_play_card(
                combat,
                *card_index,
                *target,
                phase_profile,
                plugins,
            )
        }
        ClientInput::UsePotion { .. } => {
            let potion_rank =
                potions::semantic_potion_tactical_priority(combat, input).unwrap_or_default();
            ActionOrderingPriority {
                role: ActionOrderingRole::TacticalPotion,
                role_rank: ROLE_TACTICAL_POTION_BASE + potion_rank,
                potion_tactical_rank: potion_rank,
                ..ActionOrderingPriority::neutral(ActionOrderingRole::TacticalPotion)
            }
        }
        ClientInput::DiscardPotion(_) => ActionOrderingPriority {
            role: ActionOrderingRole::DiscardPotion,
            role_rank: ROLE_DISCARD_POTION,
            ..ActionOrderingPriority::neutral(ActionOrderingRole::DiscardPotion)
        },
        ClientInput::EndTurn => ActionOrderingPriority {
            role: ActionOrderingRole::EndTurn,
            role_rank: ROLE_END_TURN,
            ..ActionOrderingPriority::neutral(ActionOrderingRole::EndTurn)
        },
        _ => ActionOrderingPriority {
            role: ActionOrderingRole::UtilityPlay,
            role_rank: ROLE_UTILITY_PLAY,
            ..ActionOrderingPriority::neutral(ActionOrderingRole::UtilityPlay)
        },
    }
}
