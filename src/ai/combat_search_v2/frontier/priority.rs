use super::super::rollout_value::{rollout_priority_value, CombatSearchRolloutValueV1};
use super::super::transition::terminal_label;
use super::super::value::{combat_search_state_value, CombatTacticalStateValueV1};
use super::super::CombatSearchV2PriorityAblation;
use super::super::SearchTerminalLabel;
use super::node::SearchNode;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2::frontier) struct NodePriority {
    terminal_rank: i32,
    rollout_value: CombatSearchRolloutValueV1,
    action_prior_rank: i32,
    action_ordering_frontier_hint: i32,
    state_value: CombatTacticalStateValueV1,
    potion_tactical_priority: i32,
    potion_conservation: i32,
    turn_branch_priority: i32,
    shorter_line: i32,
}

impl Ord for NodePriority {
    fn cmp(&self, other: &Self) -> Ordering {
        self.terminal_rank
            .cmp(&other.terminal_rank)
            .then_with(|| self.rollout_value.cmp(&other.rollout_value))
            .then_with(|| self.action_prior_rank.cmp(&other.action_prior_rank))
            .then_with(|| {
                self.action_ordering_frontier_hint
                    .cmp(&other.action_ordering_frontier_hint)
            })
            .then_with(|| self.state_value.cmp(&other.state_value))
            .then_with(|| {
                self.potion_tactical_priority
                    .cmp(&other.potion_tactical_priority)
            })
            .then_with(|| self.potion_conservation.cmp(&other.potion_conservation))
            .then_with(|| self.turn_branch_priority.cmp(&other.turn_branch_priority))
            .then_with(|| self.shorter_line.cmp(&other.shorter_line))
    }
}

impl PartialOrd for NodePriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone)]
pub(in crate::ai::combat_search_v2) struct QueueEntry {
    pub(in crate::ai::combat_search_v2::frontier) priority: NodePriority,
    pub(in crate::ai::combat_search_v2::frontier) sequence_id: u64,
    pub(in crate::ai::combat_search_v2) node: SearchNode,
    pub(in crate::ai::combat_search_v2) pending_choice_work:
        Option<super::super::PendingChoiceActionWork>,
}

impl Eq for QueueEntry {}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.sequence_id == other.sequence_id && self.priority == other.priority
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.sequence_id.cmp(&self.sequence_id))
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
pub(in crate::ai::combat_search_v2::frontier) fn priority_for_node(
    node: &SearchNode,
) -> NodePriority {
    priority_for_node_with_ablation(node, CombatSearchV2PriorityAblation::Baseline)
}

pub(in crate::ai::combat_search_v2::frontier) fn priority_for_node_with_ablation(
    node: &SearchNode,
    ablation: CombatSearchV2PriorityAblation,
) -> NodePriority {
    let terminal_rank = match terminal_label(&node.engine, &node.combat) {
        SearchTerminalLabel::Win => 3,
        SearchTerminalLabel::Unresolved => 2,
        SearchTerminalLabel::Escape => 1,
        SearchTerminalLabel::Loss => 0,
    };
    NodePriority {
        terminal_rank,
        rollout_value: rollout_priority_value(&node.rollout_estimate),
        action_prior_rank: if ablation.uses_action_guidance() {
            action_prior_rank(node.action_prior_score)
        } else {
            Default::default()
        },
        action_ordering_frontier_hint: if ablation.uses_action_guidance() {
            node.action_ordering_frontier_hint
        } else {
            Default::default()
        },
        state_value: if ablation.uses_state_value() {
            combat_search_state_value(node)
        } else {
            Default::default()
        },
        potion_tactical_priority: node.potion_tactical_priority,
        potion_conservation: -((node.potions_used + node.potions_discarded) as i32),
        turn_branch_priority: node.last_turn_branch_priority,
        shorter_line: -(node.actions.len() as i32),
    }
}

fn action_prior_rank(score: Option<f64>) -> i32 {
    score
        .filter(|score| score.is_finite())
        .map(|score| (score.clamp(0.0, 1.0) * 1_000_000.0).round() as i32)
        .unwrap_or_default()
}
