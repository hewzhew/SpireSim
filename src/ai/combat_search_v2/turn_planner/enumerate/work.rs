use crate::ai::combat_search_v2::frontier::SearchNode;
use crate::ai::combat_search_v2::turn_planner::types::TurnPlanStepStateV1;
use crate::ai::combat_search_v2::CombatSearchV2ActionFacts;
use crate::sim::combat_action::CombatActionChoice;

#[derive(Clone)]
pub(super) struct TurnPlanWorkNode {
    pub(super) node: SearchNode,
    pub(super) action_facts: Vec<CombatSearchV2ActionFacts>,
    pub(super) step_states: Vec<TurnPlanStepStateV1>,
    /// The already filtered exact action surface for this same-turn state.
    ///
    /// Partial-frontier selection uses this only for typed continuation
    /// projections. If the state survives, the next expansion consumes the
    /// same cache instead of asking the stepper for it again.
    pub(super) cached_eligible_actions: Option<Vec<CombatActionChoice>>,
}
