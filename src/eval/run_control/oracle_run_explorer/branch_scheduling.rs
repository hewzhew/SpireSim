use super::decision_supply::{decision_supply_for_branch, OracleRunDecisionSupplyV1};
use super::*;

pub(super) enum PreparedOracleRunBranchScheduleV1 {
    None,
    Combat {
        work_key: String,
        pending: PendingOracleCombatV1,
    },
    Decisions(OracleRunDecisionSupplyV1),
}

impl OracleRunExplorerV1 {
    pub(super) fn prepare_branch_schedule(
        &self,
        branch: &OracleRunBranchV1,
        combat_budgets: &OracleRunCombatWitnessBudgetsV1,
        decision_prior: Option<RunPolicyPriorFnV1>,
    ) -> Result<PreparedOracleRunBranchScheduleV1, String> {
        if self
            .state_index
            .get(&branch.state_fingerprint)
            .is_some_and(|survivor_branch_id| *survivor_branch_id != branch.branch_id)
        {
            return Ok(PreparedOracleRunBranchScheduleV1::None);
        }
        match branch.boundary {
            OracleRunBoundaryV1::Combat => {
                if !self.pending_combats.is_empty() {
                    return Err(format!(
                        "oracle attempted to start combat branch {} while another lazy combat edge was active",
                        branch.branch_id
                    ));
                }
                let work_key = format!("combat:{}", branch.state_fingerprint);
                if self.registered_work_keys.contains(&work_key) {
                    return Ok(PreparedOracleRunBranchScheduleV1::None);
                }
                let work = OracleResidentCombatWitnessJobV1::new(
                    &branch.session,
                    combat_budgets.for_session_stage(&branch.session, 0),
                    combat_budgets.guidance_bundle.as_deref(),
                )?;
                Ok(PreparedOracleRunBranchScheduleV1::Combat {
                    work_key,
                    pending: PendingOracleCombatV1 {
                        branch_id: branch.branch_id,
                        stage: 0,
                        work,
                    },
                })
            }
            OracleRunBoundaryV1::TerminalVictory | OracleRunBoundaryV1::TerminalDefeat => {
                Ok(PreparedOracleRunBranchScheduleV1::None)
            }
            _ => decision_supply_for_branch(branch, decision_prior)
                .map(PreparedOracleRunBranchScheduleV1::Decisions),
        }
    }

    pub(super) fn apply_branch_schedule(&mut self, prepared: PreparedOracleRunBranchScheduleV1) {
        match prepared {
            PreparedOracleRunBranchScheduleV1::None => {}
            PreparedOracleRunBranchScheduleV1::Combat { work_key, pending } => {
                assert!(
                    self.pending_combats.is_empty(),
                    "prepared combat schedule requires the active-combat slot to remain free"
                );
                assert!(
                    self.registered_work_keys.insert(work_key),
                    "prepared combat schedule must remain unregistered until commit"
                );
                self.pending_combats.push_back(pending);
            }
            PreparedOracleRunBranchScheduleV1::Decisions(supply) => {
                self.apply_decision_supply(supply);
            }
        }
    }

    pub(super) fn schedule_branch(
        &mut self,
        branch_id: usize,
        combat_budgets: &OracleRunCombatWitnessBudgetsV1,
        decision_prior: Option<RunPolicyPriorFnV1>,
    ) -> Result<(), String> {
        let branch = self
            .branches
            .iter()
            .find(|branch| branch.branch_id == branch_id)
            .ok_or_else(|| format!("missing oracle run branch {branch_id}"))?;
        let prepared = self.prepare_branch_schedule(branch, combat_budgets, decision_prior)?;
        self.apply_branch_schedule(prepared);
        Ok(())
    }

    pub(super) fn prepare_deferred_combat(
        &self,
        deferred: &DeferredOracleCombatV1,
        combat_budgets: &OracleRunCombatWitnessBudgetsV1,
    ) -> Result<PendingOracleCombatV1, String> {
        if !self.pending_combats.is_empty() {
            return Err(
                "oracle cannot resume a deferred combat while another edge is active".into(),
            );
        }
        let branch = self
            .branches
            .iter()
            .find(|branch| branch.branch_id == deferred.branch_id)
            .ok_or_else(|| {
                format!(
                    "missing deferred oracle combat branch {}",
                    deferred.branch_id
                )
            })?;
        let work = OracleResidentCombatWitnessJobV1::promote(
            &branch.session,
            combat_budgets.for_session_stage_with_prior(
                &branch.session,
                deferred.stage,
                &deferred.prior_work,
            ),
            deferred.prior_work.clone(),
            combat_budgets.guidance_bundle.as_deref(),
        )?;
        Ok(PendingOracleCombatV1 {
            branch_id: deferred.branch_id,
            stage: deferred.stage,
            work,
        })
    }

    pub(super) fn apply_prepared_deferred_combat(&mut self, pending: PendingOracleCombatV1) {
        assert!(
            self.pending_combats.is_empty(),
            "prepared deferred combat requires the active-combat slot to remain free"
        );
        self.pending_combats.push_back(pending);
        self.combat_search_restarts = self.combat_search_restarts.saturating_add(1);
    }
}
