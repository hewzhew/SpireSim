use super::decision_supply::{decision_supply_for_branch, OracleRunDecisionSupplyV1};
use super::*;

pub(in super::super) struct PreparedOracleRunDecisionV1 {
    child: OracleRunBranchV1,
}

impl PreparedOracleRunDecisionV1 {
    pub(in super::super) fn prospective_branch(&self) -> &OracleRunBranchV1 {
        &self.child
    }
}

pub(in super::super) struct PreparedOracleRunDecisionRegistrationV1 {
    supply: Option<OracleRunDecisionSupplyV1>,
}

impl OracleRunExplorerV1 {
    fn prepare_decision(
        &self,
        work: LazyOracleRunDecisionV1,
        decision_annotation: Option<OracleRunDecisionAnnotationFnV1>,
    ) -> Result<PreparedOracleRunDecisionV1, String> {
        let parent = self
            .branches
            .iter()
            .find(|branch| branch.branch_id == work.parent_branch_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "oracle decision references missing parent branch {}",
                    work.parent_branch_id
                )
            })?;
        if parent.state_fingerprint != work.parent_state_fingerprint {
            return Err(format!(
                "oracle decision parent fingerprint changed for branch {}",
                work.parent_branch_id
            ));
        }

        let annotation =
            decision_annotation.and_then(|annotate| annotate(&parent.session, &work.candidate_id));
        let successor = super::super::exact_run_decision_successor_v1(
            &parent.session,
            &work.candidate_id,
            work.action.clone(),
        )?;
        let mut session = successor.session;
        let mut transaction = successor.transaction;
        if let Some(annotation) = annotation {
            transaction.trace_annotations.push(annotation);
        }
        let forced_steps = settle_oracle_forced_transitions(&mut session)?;
        let successor_fingerprint = run_session_fingerprint_v3(&session);
        if successor_fingerprint == parent.state_fingerprint {
            return Err(format!(
                "oracle decision '{}' ({}) produced no state change at branch {}; \
                 executable decision surfaces must not expose no-op actions",
                work.label, work.candidate_id, parent.branch_id
            ));
        }
        let mut journal = parent.journal;
        journal.append_committed_steps(vec![RunProgressStepV1::Decision(transaction)])?;
        journal.append_committed_steps(forced_steps)?;
        let mut replay = parent.replay;
        replay.push(OracleRunReplayStepV1 {
            candidate_id: work.candidate_id,
            label: work.label,
            action: work.action,
        });
        let child = OracleRunBranchV1 {
            branch_id: self.next_branch_id,
            parent_branch_id: Some(parent.branch_id),
            neow_root_candidate_id: parent.neow_root_candidate_id,
            neow_root_label: parent.neow_root_label,
            state_fingerprint: successor_fingerprint,
            boundary: classify_run_boundary(&session),
            path_negative_log_policy: work.path_negative_log_policy,
            path_discrepancy: work.path_discrepancy,
            path_depth: work.path_depth,
            replay,
            journal,
            session,
        };
        Ok(PreparedOracleRunDecisionV1 { child })
    }

    pub(in super::super) fn commit_prepared_decision(
        &mut self,
        prepared: PreparedOracleRunDecisionV1,
    ) -> Option<usize> {
        assert_eq!(
            prepared.child.branch_id, self.next_branch_id,
            "prepared decision branch id must remain current until commit"
        );
        self.next_branch_id = self.next_branch_id.saturating_add(1);
        self.accept_branch(prepared.child)
    }

    pub(in super::super) fn prepare_explicit_decision(
        &self,
        work: LazyOracleRunDecisionV1,
        decision_annotation: Option<OracleRunDecisionAnnotationFnV1>,
    ) -> Result<PreparedOracleRunDecisionV1, String> {
        self.prepare_decision(work, decision_annotation)
    }

    pub(in super::super) fn prepare_explicit_decision_registration(
        &self,
        prepared: &PreparedOracleRunDecisionV1,
        decision_prior: Option<RunPolicyPriorFnV1>,
    ) -> Result<PreparedOracleRunDecisionRegistrationV1, String> {
        self.prepare_explicit_branch_registration(&prepared.child, decision_prior)
    }

    pub(in super::super) fn prepare_explicit_branch_registration(
        &self,
        prospective_child: &OracleRunBranchV1,
        decision_prior: Option<RunPolicyPriorFnV1>,
    ) -> Result<PreparedOracleRunDecisionRegistrationV1, String> {
        let branch = self
            .state_index
            .get(&prospective_child.state_fingerprint)
            .and_then(|branch_id| {
                self.branches
                    .iter()
                    .find(|branch| branch.branch_id == *branch_id)
            })
            .unwrap_or(prospective_child);
        let supply = match branch.boundary {
            OracleRunBoundaryV1::Combat
            | OracleRunBoundaryV1::TerminalVictory
            | OracleRunBoundaryV1::TerminalDefeat => None,
            _ => Some(decision_supply_for_branch(branch, decision_prior)?),
        };
        Ok(PreparedOracleRunDecisionRegistrationV1 { supply })
    }

    pub(in super::super) fn commit_explicit_decision(
        &mut self,
        prepared: PreparedOracleRunDecisionV1,
    ) -> usize {
        let duplicate_count = self.retired_exact_duplicates.len();
        if let Some(branch_id) = self.commit_prepared_decision(prepared) {
            return branch_id;
        }
        self.retired_exact_duplicates
            .get(duplicate_count)
            .map(|duplicate| duplicate.survivor_branch_id)
            .expect("explicit decision duplicate must record its survivor")
    }

    pub(in super::super) fn apply_explicit_decision_registration(
        &mut self,
        prepared: PreparedOracleRunDecisionRegistrationV1,
    ) {
        let Some(supply) = prepared.supply else {
            return;
        };
        self.apply_decision_supply(supply);
    }
}

pub(super) fn settle_oracle_forced_transitions(
    session: &mut RunControlSession,
) -> Result<Vec<RunProgressStepV1>, String> {
    let mut steps = Vec::new();
    if matches!(session.engine_state, EngineState::Campfire)
        && crate::engine::campfire_handler::get_available_options(&session.run_state).is_empty()
    {
        let transition = session.execute_forced_transition(
            super::super::RunForcedTransitionKindV1::EmptyCampfireExit,
        )?;
        steps.push(RunProgressStepV1::ForcedTransition(transition));
    }
    Ok(steps)
}
