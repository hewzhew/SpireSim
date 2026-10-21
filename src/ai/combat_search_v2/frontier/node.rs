use super::super::{
    advance_turn_prefix, CombatSearchV2ActionTrace, RolloutNodeEstimate, TurnBranchTransition,
    TurnPrefixState,
};
use super::resources::ResourceVector;
use crate::runtime::combat::CombatState;
use crate::state::core::{ClientInput, EngineState};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) struct RootLineageId(pub(in crate::ai::combat_search_v2) u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub(in crate::ai::combat_search_v2) enum RootLineage {
    #[default]
    Unmaterialized,
    Action(RootLineageId),
}

#[derive(Clone)]
pub(in crate::ai::combat_search_v2) struct SearchNode {
    pub(in crate::ai::combat_search_v2) engine: EngineState,
    pub(in crate::ai::combat_search_v2) combat: CombatState,
    pub(in crate::ai::combat_search_v2) actions: Vec<CombatSearchV2ActionTrace>,
    pub(in crate::ai::combat_search_v2) turn_prefix: TurnPrefixState,
    pub(in crate::ai::combat_search_v2) initial_hp: i32,
    pub(in crate::ai::combat_search_v2) potions_used: u32,
    pub(in crate::ai::combat_search_v2) potions_discarded: u32,
    pub(in crate::ai::combat_search_v2) cards_played: u32,
    pub(in crate::ai::combat_search_v2) potion_tactical_priority: i32,
    pub(in crate::ai::combat_search_v2) last_turn_branch_priority: i32,
    pub(in crate::ai::combat_search_v2) action_prior_score: Option<f64>,
    pub(in crate::ai::combat_search_v2) action_ordering_frontier_hint: i32,
    pub(in crate::ai::combat_search_v2) rollout_estimate: RolloutNodeEstimate,
    pub(in crate::ai::combat_search_v2) root_lineage: RootLineage,
}

impl SearchNode {
    pub(in crate::ai::combat_search_v2) fn root(engine: EngineState, combat: CombatState) -> Self {
        let initial_hp = combat.entities.player.current_hp;
        Self {
            engine,
            combat,
            actions: Vec::new(),
            turn_prefix: TurnPrefixState::default(),
            initial_hp,
            potions_used: 0,
            potions_discarded: 0,
            cards_played: 0,
            potion_tactical_priority: 0,
            last_turn_branch_priority: 0,
            action_prior_score: None,
            action_ordering_frontier_hint: 0,
            rollout_estimate: RolloutNodeEstimate::unevaluated(),
            root_lineage: RootLineage::Unmaterialized,
        }
    }

    pub(in crate::ai::combat_search_v2) fn clone_for_child(
        &self,
        engine: EngineState,
        combat: CombatState,
    ) -> Self {
        Self {
            engine,
            combat,
            actions: self.actions.clone(),
            turn_prefix: self.turn_prefix.clone(),
            initial_hp: self.initial_hp,
            potions_used: self.potions_used,
            potions_discarded: self.potions_discarded,
            cards_played: self.cards_played,
            potion_tactical_priority: self.potion_tactical_priority,
            last_turn_branch_priority: self.last_turn_branch_priority,
            action_prior_score: None,
            action_ordering_frontier_hint: 0,
            rollout_estimate: RolloutNodeEstimate::unevaluated(),
            root_lineage: self.root_lineage,
        }
    }

    pub(in crate::ai::combat_search_v2) fn note_input(&mut self, input: &ClientInput) {
        match input {
            ClientInput::UsePotion { .. } => {
                self.potions_used = self.potions_used.saturating_add(1);
            }
            ClientInput::DiscardPotion(_) => {
                self.potions_discarded = self.potions_discarded.saturating_add(1);
            }
            ClientInput::PlayCard { .. } => {
                self.cards_played = self.cards_played.saturating_add(1);
            }
            _ => {}
        }
    }

    pub(in crate::ai::combat_search_v2) fn push_action(
        &mut self,
        action: CombatSearchV2ActionTrace,
    ) {
        self.actions.push(action);
    }

    pub(in crate::ai::combat_search_v2) fn note_potion_tactical_priority(
        &mut self,
        priority: Option<i32>,
    ) {
        if let Some(priority) = priority {
            self.potion_tactical_priority = self.potion_tactical_priority.max(priority);
        }
    }

    pub(in crate::ai::combat_search_v2) fn note_turn_branch_priority(&mut self, priority: i32) {
        self.last_turn_branch_priority = priority;
    }

    pub(in crate::ai::combat_search_v2) fn note_action_prior_score(&mut self, score: Option<f64>) {
        self.action_prior_score = score.filter(|score| score.is_finite());
    }

    pub(in crate::ai::combat_search_v2) fn note_action_ordering_frontier_hint(
        &mut self,
        hint: i32,
    ) {
        self.action_ordering_frontier_hint = hint.max(0);
    }

    pub(in crate::ai::combat_search_v2) fn note_turn_prefix(
        &mut self,
        parent_combat: &CombatState,
        input: &ClientInput,
        transition: TurnBranchTransition,
    ) {
        self.turn_prefix = advance_turn_prefix(&self.turn_prefix, parent_combat, input, transition);
    }

    pub(in crate::ai::combat_search_v2) fn resource_vector(&self) -> ResourceVector {
        ResourceVector {
            hp: self.combat.entities.player.current_hp,
            block: self.combat.entities.player.block,
            potions_used: self.potions_used,
            potions_discarded: self.potions_discarded,
            cards_played: self.cards_played,
            action_count: self.actions.len(),
        }
    }
}
