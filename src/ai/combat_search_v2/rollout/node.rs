use super::super::*;

impl SearchNode {
    pub(in crate::ai::combat_search_v2::rollout) fn clone_for_rollout(&self) -> Self {
        let mut clone = self.clone();
        clone.rollout_estimate = RolloutNodeEstimate::unevaluated();
        clone
    }
}
