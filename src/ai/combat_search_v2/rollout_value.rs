use super::value::{combat_eval_from_rollout_estimate, CombatEvalV2};
use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CombatSearchRolloutValueV1 {
    pub(super) eval: CombatEvalV2,
}

impl Ord for CombatSearchRolloutValueV1 {
    fn cmp(&self, other: &Self) -> Ordering {
        // A rollout is guidance, not an exact terminal proof. In particular, a
        // policy-simulated loss must not outrank a still-unvisited live state
        // merely because the loss happened to be evaluated first. CombatEvalV2
        // orders outcomes as loss < unresolved < win and evidence within an
        // outcome, which gives unevaluated states first-play urgency between a
        // simulated loss and an evaluated unresolved estimate.
        self.eval.cmp(&other.eval)
    }
}

impl PartialOrd for CombatSearchRolloutValueV1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) fn rollout_priority_value(estimate: &RolloutNodeEstimate) -> CombatSearchRolloutValueV1 {
    CombatSearchRolloutValueV1 {
        eval: combat_eval_from_rollout_estimate(estimate),
    }
}

#[cfg(test)]
mod tests;
