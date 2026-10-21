use serde::{Deserialize, Serialize};

use super::*;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleRunCombatEvidenceKindV1 {
    BudgetUnknown,
    ExhaustiveRefutation,
    SetupOrMechanicsError,
}

impl OracleRunCombatEvidenceKindV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BudgetUnknown => "budget_unknown",
            Self::ExhaustiveRefutation => "exhaustive_refutation",
            Self::SetupOrMechanicsError => "setup_or_mechanics_error",
        }
    }
}

pub(super) enum FinishedOracleCombatV1 {
    Resolved(usize),
    ExactDuplicate,
    Unresolved(OracleRunUnresolvedCombatV1),
}

pub(in super::super) enum PreparedOracleRunCombatV1 {
    Resolved(OracleRunBranchV1),
    Unresolved(OracleRunUnresolvedCombatV1),
}

impl PreparedOracleRunCombatV1 {
    pub(in super::super) fn prospective_branch(&self) -> Option<&OracleRunBranchV1> {
        match self {
            Self::Resolved(branch) => Some(branch),
            Self::Unresolved(_) => None,
        }
    }
}

pub(super) fn classify_unresolved_combat_evidence(
    last_status: Option<&str>,
    generation_gap_count: usize,
) -> OracleRunCombatEvidenceKindV1 {
    match last_status {
        Some("frontier_exhausted") if generation_gap_count == 0 => {
            OracleRunCombatEvidenceKindV1::ExhaustiveRefutation
        }
        Some("mechanics_gap") | Some("replay_mismatch") => {
            OracleRunCombatEvidenceKindV1::SetupOrMechanicsError
        }
        _ => OracleRunCombatEvidenceKindV1::BudgetUnknown,
    }
}

impl OracleRunExplorerV1 {
    fn prepare_combat(
        &self,
        branch_id: usize,
        work: &OracleResidentCombatWitnessJobV1,
    ) -> Result<PreparedOracleRunCombatV1, String> {
        let parent = self
            .branches
            .iter()
            .find(|branch| branch.branch_id == branch_id)
            .cloned()
            .ok_or_else(|| format!("missing oracle combat branch {branch_id}"))?;
        let progress = work.evidence();
        let mut session = parent.session.clone();
        // Deadlines bound search advancement. Once a verified witness is
        // ready, its exact replay is an atomic prepare step and is never
        // interrupted by a wall-clock deadline.
        let outcome = work.finish_and_apply(&mut session).map_err(|error| {
            format!(
                "oracle combat branch {} at Act {} Floor {} failed to commit its witness: {error}",
                parent.branch_id,
                parent.session.run_state.act_num,
                parent.session.run_state.floor_num
            )
        })?;
        if outcome.progress_steps.is_empty() {
            let failure = outcome.oracle_combat_witness_failure.ok_or_else(|| {
                format!(
                    "oracle combat branch {} made no progress without a typed witness failure",
                    parent.branch_id
                )
            })?;
            return Ok(PreparedOracleRunCombatV1::Unresolved(
                OracleRunUnresolvedCombatV1 {
                    branch_id: parent.branch_id,
                    failure,
                    evidence_kind: classify_unresolved_combat_evidence(
                        progress.last_status.as_deref(),
                        progress.generation_gap_count,
                    ),
                    last_status: progress.last_status.clone(),
                    generation_work: progress.generation_work,
                    exact_states: progress.exact_states,
                    applied_action_transitions: progress.applied_action_transitions,
                    unique_successor_states: progress.unique_successor_states,
                    duplicate_exact_successors: progress.duplicate_exact_successors,
                    completed_turn_options: progress.completed_turn_options,
                    retained_state_work: progress.retained_state_work,
                    max_player_turn: progress.max_player_turn,
                    max_path_atomic_depth: progress.max_path_atomic_depth,
                    generation_gap_count: progress.generation_gap_count,
                    incumbent_final_hp: progress.incumbent_final_hp,
                },
            ));
        }
        self.prepare_resolved_combat_branch(parent, session, outcome.progress_steps)
            .map(PreparedOracleRunCombatV1::Resolved)
    }

    fn prepare_resolved_combat_branch(
        &self,
        parent: OracleRunBranchV1,
        session: RunControlSession,
        progress_steps: Vec<RunProgressStepV1>,
    ) -> Result<OracleRunBranchV1, String> {
        if progress_steps.len() != 1 {
            return Err(format!(
                "oracle combat branch {} committed {} progress steps; expected one",
                parent.branch_id,
                progress_steps.len()
            ));
        }
        let mut journal = parent.journal;
        journal.append_committed_steps(progress_steps)?;
        Ok(OracleRunBranchV1 {
            branch_id: self.next_branch_id,
            parent_branch_id: Some(parent.branch_id),
            neow_root_candidate_id: parent.neow_root_candidate_id,
            neow_root_label: parent.neow_root_label,
            state_fingerprint: run_session_fingerprint_v3(&session),
            boundary: classify_run_boundary(&session),
            path_negative_log_policy: parent.path_negative_log_policy,
            path_discrepancy: parent.path_discrepancy,
            path_depth: parent.path_depth.saturating_add(1),
            replay: parent.replay,
            journal,
            session,
        })
    }

    pub(super) fn commit_prepared_combat(
        &mut self,
        prepared: PreparedOracleRunCombatV1,
    ) -> Result<FinishedOracleCombatV1, String> {
        let child = match prepared {
            PreparedOracleRunCombatV1::Resolved(child) => child,
            PreparedOracleRunCombatV1::Unresolved(unresolved) => {
                return Ok(FinishedOracleCombatV1::Unresolved(unresolved));
            }
        };
        if child.branch_id != self.next_branch_id {
            return Err(format!(
                "prepared combat branch id {} no longer matches next branch id {}",
                child.branch_id, self.next_branch_id
            ));
        }
        self.next_branch_id = self.next_branch_id.saturating_add(1);
        Ok(match self.accept_branch(child) {
            Some(branch_id) => FinishedOracleCombatV1::Resolved(branch_id),
            None => FinishedOracleCombatV1::ExactDuplicate,
        })
    }

    pub(in super::super) fn prepare_explicit_combat(
        &self,
        branch_id: usize,
        work: &OracleResidentCombatWitnessJobV1,
    ) -> Result<PreparedOracleRunCombatV1, String> {
        self.prepare_combat(branch_id, work)
    }

    pub(in super::super) fn prepare_explicit_smoke_bomb_escape(
        &self,
        branch_id: usize,
    ) -> Result<PreparedOracleRunCombatV1, String> {
        let parent = self
            .branches
            .iter()
            .find(|branch| branch.branch_id == branch_id)
            .cloned()
            .ok_or_else(|| format!("missing oracle combat branch {branch_id}"))?;
        let mut session = parent.session.clone();
        let outcome =
            super::super::combat_no_win_fallback::try_apply_smoke_bomb_survival_fallback_after_rejection(
                &mut session,
                "explicit oracle escape",
            )?
            .ok_or_else(|| {
                format!(
                    "oracle combat branch {branch_id} has no currently usable Smoke Bomb escape"
                )
            })?;
        self.prepare_resolved_combat_branch(parent, session, outcome.progress_steps)
            .map(PreparedOracleRunCombatV1::Resolved)
    }

    pub(in super::super) fn commit_explicit_combat(
        &mut self,
        prepared: PreparedOracleRunCombatV1,
    ) -> Result<Option<usize>, String> {
        let duplicate_count = self.retired_exact_duplicates.len();
        match self.commit_prepared_combat(prepared)? {
            FinishedOracleCombatV1::Resolved(branch_id) => Ok(Some(branch_id)),
            FinishedOracleCombatV1::ExactDuplicate => self
                .retired_exact_duplicates
                .get(duplicate_count)
                .map(|duplicate| Some(duplicate.survivor_branch_id))
                .ok_or_else(|| {
                    "explicit oracle combat duplicated without a survivor record".to_string()
                }),
            FinishedOracleCombatV1::Unresolved(unresolved) => {
                self.unresolved_combats.push(unresolved);
                Ok(None)
            }
        }
    }
}
