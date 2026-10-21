use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct TurnBranchingStateObservation {
    pub(in crate::ai::combat_search_v2::turn_branching) parent_turn_count: u32,
    pub(in crate::ai::combat_search_v2::turn_branching) parent_energy: u8,
    pub(in crate::ai::combat_search_v2::turn_branching) legal_actions: usize,
    pub(in crate::ai::combat_search_v2::turn_branching) generated_children: usize,
    pub(in crate::ai::combat_search_v2::turn_branching) same_turn_children: usize,
    pub(in crate::ai::combat_search_v2::turn_branching) next_turn_children: usize,
    pub(in crate::ai::combat_search_v2::turn_branching) pending_choice_children: usize,
    pub(in crate::ai::combat_search_v2::turn_branching) terminal_children: usize,
    pub(in crate::ai::combat_search_v2::turn_branching) other_children: usize,
    pub(in crate::ai::combat_search_v2::turn_branching) end_turn_children: usize,
    pub(in crate::ai::combat_search_v2::turn_branching) transition_counts:
        BTreeMap<TurnBranchTransitionCountKey, usize>,
}

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct TurnBranchingDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2::turn_branching) states_observed: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) total_legal_actions: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) total_generated_children: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) same_turn_children: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) next_turn_children: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) pending_choice_children: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) terminal_children: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) other_children: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) end_turn_children: u64,
    pub(in crate::ai::combat_search_v2::turn_branching) transition_counts:
        BTreeMap<TurnBranchTransitionCountKey, u64>,
    pub(in crate::ai::combat_search_v2::turn_branching) largest_turn_fanouts:
        Vec<TurnBranchingStateObservation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct TurnBranchTransition {
    pub(in crate::ai::combat_search_v2::turn_branching) action_kind: TurnBranchActionKind,
    pub(in crate::ai::combat_search_v2::turn_branching) kind: TurnBranchTransitionKind,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2::turn_branching) enum TurnBranchTransitionKind {
    SameTurn,
    NextTurn,
    PendingChoice,
    Terminal,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2::turn_branching) enum TurnBranchActionKind {
    PlayCard,
    EndTurn,
    UsePotion,
    DiscardPotion,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2::turn_branching) struct TurnBranchTransitionCountKey {
    pub(in crate::ai::combat_search_v2::turn_branching) action_kind: TurnBranchActionKind,
    pub(in crate::ai::combat_search_v2::turn_branching) transition_kind: TurnBranchTransitionKind,
}

impl TurnBranchTransition {
    pub(in crate::ai::combat_search_v2) fn is_same_turn(self) -> bool {
        matches!(self.kind, TurnBranchTransitionKind::SameTurn)
    }

    pub(in crate::ai::combat_search_v2) fn is_next_turn(self) -> bool {
        matches!(self.kind, TurnBranchTransitionKind::NextTurn)
    }

    pub(in crate::ai::combat_search_v2) fn is_pending_choice(self) -> bool {
        matches!(self.kind, TurnBranchTransitionKind::PendingChoice)
    }

    pub(in crate::ai::combat_search_v2) fn is_terminal(self) -> bool {
        matches!(self.kind, TurnBranchTransitionKind::Terminal)
    }

    pub(in crate::ai::combat_search_v2) fn frontier_priority_hint(self) -> i32 {
        match (self.action_kind, self.kind) {
            (_, TurnBranchTransitionKind::Terminal) => 40,
            (_, TurnBranchTransitionKind::PendingChoice) => 15,
            (TurnBranchActionKind::PlayCard, TurnBranchTransitionKind::SameTurn) => 12,
            (TurnBranchActionKind::UsePotion, TurnBranchTransitionKind::SameTurn) => 8,
            (TurnBranchActionKind::EndTurn, TurnBranchTransitionKind::NextTurn) => 0,
            (TurnBranchActionKind::DiscardPotion, _) => -20,
            (_, TurnBranchTransitionKind::SameTurn) => 4,
            (_, TurnBranchTransitionKind::NextTurn) => 0,
            (_, TurnBranchTransitionKind::Other) => 0,
        }
    }

    pub(in crate::ai::combat_search_v2) fn resets_turn_prefix(self) -> bool {
        matches!(self.kind, TurnBranchTransitionKind::NextTurn)
    }

    #[cfg(test)]
    pub(in crate::ai::combat_search_v2) fn test_same_turn_play_card() -> Self {
        Self {
            action_kind: TurnBranchActionKind::PlayCard,
            kind: TurnBranchTransitionKind::SameTurn,
        }
    }

    #[cfg(test)]
    pub(in crate::ai::combat_search_v2) fn test_next_turn_end_turn() -> Self {
        Self {
            action_kind: TurnBranchActionKind::EndTurn,
            kind: TurnBranchTransitionKind::NextTurn,
        }
    }
}

impl TurnBranchActionKind {
    pub(in crate::ai::combat_search_v2::turn_branching) fn from_input(
        input: &crate::state::core::ClientInput,
    ) -> Self {
        match input {
            crate::state::core::ClientInput::PlayCard { .. } => Self::PlayCard,
            crate::state::core::ClientInput::EndTurn => Self::EndTurn,
            crate::state::core::ClientInput::UsePotion { .. } => Self::UsePotion,
            crate::state::core::ClientInput::DiscardPotion(_) => Self::DiscardPotion,
            _ => Self::Other,
        }
    }

    pub(in crate::ai::combat_search_v2::turn_branching) fn label(self) -> &'static str {
        match self {
            Self::PlayCard => "play_card",
            Self::EndTurn => "end_turn",
            Self::UsePotion => "use_potion",
            Self::DiscardPotion => "discard_potion",
            Self::Other => "other",
        }
    }
}

impl TurnBranchTransitionKind {
    pub(in crate::ai::combat_search_v2::turn_branching) fn label(self) -> &'static str {
        match self {
            Self::SameTurn => "same_turn",
            Self::NextTurn => "next_turn",
            Self::PendingChoice => "pending_choice",
            Self::Terminal => "terminal",
            Self::Other => "other",
        }
    }
}
