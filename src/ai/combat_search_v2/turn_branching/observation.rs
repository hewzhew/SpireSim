use super::super::*;
use super::types::{
    TurnBranchActionKind, TurnBranchTransition, TurnBranchTransitionCountKey,
    TurnBranchTransitionKind, TurnBranchingStateObservation,
};

impl TurnBranchingStateObservation {
    pub(in crate::ai::combat_search_v2) fn new(
        parent_combat: &CombatState,
        legal_actions: usize,
    ) -> Self {
        Self {
            parent_turn_count: parent_combat.turn.turn_count,
            parent_energy: parent_combat.turn.energy,
            legal_actions,
            generated_children: 0,
            same_turn_children: 0,
            next_turn_children: 0,
            pending_choice_children: 0,
            terminal_children: 0,
            other_children: 0,
            end_turn_children: 0,
            transition_counts: std::collections::BTreeMap::new(),
        }
    }

    pub(in crate::ai::combat_search_v2) fn observe_child(
        &mut self,
        transition: TurnBranchTransition,
    ) {
        self.generated_children = self.generated_children.saturating_add(1);
        match transition.kind {
            TurnBranchTransitionKind::SameTurn => {
                self.same_turn_children = self.same_turn_children.saturating_add(1)
            }
            TurnBranchTransitionKind::NextTurn => {
                self.next_turn_children = self.next_turn_children.saturating_add(1)
            }
            TurnBranchTransitionKind::PendingChoice => {
                self.pending_choice_children = self.pending_choice_children.saturating_add(1)
            }
            TurnBranchTransitionKind::Terminal => {
                self.terminal_children = self.terminal_children.saturating_add(1)
            }
            TurnBranchTransitionKind::Other => {
                self.other_children = self.other_children.saturating_add(1)
            }
        }
        if transition.action_kind == TurnBranchActionKind::EndTurn {
            self.end_turn_children = self.end_turn_children.saturating_add(1);
        }

        let key = TurnBranchTransitionCountKey {
            action_kind: transition.action_kind,
            transition_kind: transition.kind,
        };
        *self.transition_counts.entry(key).or_insert(0) += 1;
    }
}
