use super::combat_completion::PreparedOracleRunCombatV1;
use super::*;

/// Facts returned after one explicit analyst decision commits atomically.
pub struct OracleRunExplorerDecisionCommitV1 {
    pub child_branch_id: usize,
    pub label: String,
}

/// Facts returned after one explicit combat transaction commits atomically.
pub struct OracleRunExplorerCombatCommitV1 {
    pub child_branch_id: Option<usize>,
    pub child_current_hp: Option<i32>,
}

/// Narrow mutation capability for analyst-requested explorer transactions.
///
/// Callers choose a typed decision identity or provide an already verified
/// combat job. Prepared branches, registration supplies, selection-family
/// release plans, and the explorer's private identity registries never cross
/// this boundary.
pub struct OracleRunExplorerExplicitTransactionsV1<'a> {
    explorer: &'a mut OracleRunExplorerV1,
}

impl OracleRunExplorerV1 {
    pub fn explicit_transactions(&mut self) -> OracleRunExplorerExplicitTransactionsV1<'_> {
        OracleRunExplorerExplicitTransactionsV1 { explorer: self }
    }
}

impl OracleRunExplorerExplicitTransactionsV1<'_> {
    pub fn commit_decision(
        &mut self,
        parent_branch_id: usize,
        stable_work_key: &str,
        decision_annotation: Option<OracleRunDecisionAnnotationFnV1>,
        decision_prior: Option<RunPolicyPriorFnV1>,
    ) -> Result<Option<OracleRunExplorerDecisionCommitV1>, String> {
        let Some(work) = self
            .explorer
            .pending_decisions
            .iter()
            .find(|work| {
                work.parent_branch_id == parent_branch_id && work.stable_work_key == stable_work_key
            })
            .cloned()
        else {
            return Ok(None);
        };
        let label = work.label.clone();
        let selection_release = self
            .explorer
            .prepare_selection_member_release(&work.stable_work_key)?;
        let decision = self
            .explorer
            .prepare_explicit_decision(work, decision_annotation)?;
        let child_registration = self
            .explorer
            .prepare_explicit_decision_registration(&decision, decision_prior)?;

        let child_branch_id = self.explorer.commit_explicit_decision(decision);
        self.explorer
            .apply_explicit_decision_registration(child_registration);
        self.explorer
            .apply_selection_member_release(selection_release);
        Ok(Some(OracleRunExplorerDecisionCommitV1 {
            child_branch_id,
            label,
        }))
    }

    pub fn commit_verified_combat(
        &mut self,
        source_branch_id: usize,
        work: &OracleResidentCombatWitnessJobV1,
        decision_prior: Option<RunPolicyPriorFnV1>,
    ) -> Result<OracleRunExplorerCombatCommitV1, String> {
        let prepared = self
            .explorer
            .prepare_explicit_combat(source_branch_id, work)?;
        self.commit_prepared_combat(prepared, decision_prior)
    }

    pub fn commit_smoke_bomb_escape(
        &mut self,
        source_branch_id: usize,
        decision_prior: Option<RunPolicyPriorFnV1>,
    ) -> Result<OracleRunExplorerCombatCommitV1, String> {
        let prepared = self
            .explorer
            .prepare_explicit_smoke_bomb_escape(source_branch_id)?;
        self.commit_prepared_combat(prepared, decision_prior)
    }

    fn commit_prepared_combat(
        &mut self,
        prepared: PreparedOracleRunCombatV1,
        decision_prior: Option<RunPolicyPriorFnV1>,
    ) -> Result<OracleRunExplorerCombatCommitV1, String> {
        let child_registration = prepared
            .prospective_branch()
            .map(|branch| {
                self.explorer
                    .prepare_explicit_branch_registration(branch, decision_prior)
            })
            .transpose()?;

        let child_branch_id = self.explorer.commit_explicit_combat(prepared)?;
        if let Some(child_registration) = child_registration {
            self.explorer
                .apply_explicit_decision_registration(child_registration);
        }
        let child_current_hp = child_branch_id.map(|branch_id| {
            self.explorer
                .branches
                .iter()
                .find(|branch| branch.branch_id == branch_id)
                .expect("committed combat child or exact survivor must remain addressable")
                .session
                .run_state
                .current_hp
        });
        Ok(OracleRunExplorerCombatCommitV1 {
            child_branch_id,
            child_current_hp,
        })
    }
}
