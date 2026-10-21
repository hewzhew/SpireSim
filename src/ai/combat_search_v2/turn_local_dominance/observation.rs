use super::*;
use std::collections::HashMap;

#[derive(Debug)]
pub(in crate::ai::combat_search_v2) struct TurnLocalDominanceStateObservation {
    pub(super) enabled: bool,
    pub(super) parent_turn_count: u32,
    pub(super) legal_actions: usize,
    pub(super) eligible_child_states: usize,
    pub(super) accepted_child_states: usize,
    pub(super) pruned_child_states: usize,
    pub(super) dominance_buckets: HashMap<CombatDominanceKey, Vec<ResourceVector>>,
    pub(super) max_bucket_width: usize,
}

impl TurnLocalDominanceStateObservation {
    pub(in crate::ai::combat_search_v2) fn new(
        parent_engine: &EngineState,
        parent_combat: &CombatState,
        legal_actions: usize,
    ) -> Self {
        Self {
            enabled: matches!(parent_engine, EngineState::CombatPlayerTurn),
            parent_turn_count: parent_combat.turn.turn_count,
            legal_actions,
            eligible_child_states: 0,
            accepted_child_states: 0,
            pruned_child_states: 0,
            dominance_buckets: HashMap::new(),
            max_bucket_width: 0,
        }
    }

    pub(in crate::ai::combat_search_v2) fn observe_child(&mut self, child: &SearchNode) -> bool {
        if !self.enabled || !self.is_same_turn_player_child(child) {
            return false;
        }

        self.eligible_child_states = self.eligible_child_states.saturating_add(1);
        let dominance_key = combat_dominance_key(&child.engine, &child.combat);
        if is_resource_covered(
            &mut self.dominance_buckets,
            dominance_key,
            child.resource_vector(),
        ) {
            self.pruned_child_states = self.pruned_child_states.saturating_add(1);
            true
        } else {
            self.accepted_child_states = self.accepted_child_states.saturating_add(1);
            self.max_bucket_width = self.max_bucket_width.max(
                self.dominance_buckets
                    .values()
                    .map(Vec::len)
                    .max()
                    .unwrap_or_default(),
            );
            false
        }
    }

    pub(super) fn resource_vector_count(&self) -> usize {
        self.dominance_buckets.values().map(Vec::len).sum()
    }

    fn is_same_turn_player_child(&self, child: &SearchNode) -> bool {
        matches!(child.engine, EngineState::CombatPlayerTurn)
            && child.combat.turn.turn_count == self.parent_turn_count
            && terminal_label(&child.engine, &child.combat) == SearchTerminalLabel::Unresolved
    }
}
