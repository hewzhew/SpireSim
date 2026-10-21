use super::{RunControlSession, RunDecisionAction, RunDecisionTransactionV1, RunProgressStepV1};

/// One exact, public run-decision transition.
///
/// This is the model boundary consumed by run search.  It deliberately carries
/// no policy score or owner verdict: legality and state mutation come only from
/// the public decision surface and `RunControlSession` transaction machinery.
#[derive(Clone, Debug)]
pub struct ExactRunDecisionSuccessorV1 {
    pub session: RunControlSession,
    pub transaction: RunDecisionTransactionV1,
}

impl ExactRunDecisionSuccessorV1 {
    pub fn into_progress_step(self) -> (RunControlSession, RunProgressStepV1) {
        (self.session, RunProgressStepV1::Decision(self.transaction))
    }
}

/// Applies one already-bound public candidate to a cloned session.
///
/// The parent is never mutated.  Candidate binding, atomicity, decision-step
/// advancement, and the before/after transaction record remain owned by
/// `RunControlSession`.
pub fn exact_run_decision_successor_v1(
    parent: &RunControlSession,
    candidate_id: &str,
    action: RunDecisionAction,
) -> Result<ExactRunDecisionSuccessorV1, String> {
    let mut session = parent.clone();
    let transaction = session.execute_owner_candidate_transaction(candidate_id, action)?;
    Ok(ExactRunDecisionSuccessorV1 {
        session,
        transaction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::run_control::{
        build_decision_surface, DecisionCandidateKey, RunControlConfig,
    };
    use crate::state::core::EngineState;

    #[test]
    fn exact_successor_applies_one_public_action_without_mutating_parent() {
        let mut parent = RunControlSession::new(RunControlConfig::default());
        parent.engine_state = EngineState::Shop(crate::state::shop::ShopState::new());
        let surface = build_decision_surface(&parent);
        let leave = surface
            .view
            .candidates
            .iter()
            .find(|candidate| candidate.key == Some(DecisionCandidateKey::ShopLeave))
            .expect("shop leave must be a public candidate");
        let action = leave
            .action
            .executable_action()
            .expect("shop leave must be executable");

        let successor =
            exact_run_decision_successor_v1(&parent, &leave.id, action).expect("exact successor");

        assert_eq!(parent.decision_step, 0);
        assert!(matches!(parent.engine_state, EngineState::Shop(_)));
        assert_eq!(successor.transaction.before.decision_step, 0);
        assert_eq!(successor.transaction.after.decision_step, 1);
        assert_eq!(successor.session.decision_step, 1);
        assert!(!matches!(
            successor.session.engine_state,
            EngineState::Shop(_)
        ));
    }
}
