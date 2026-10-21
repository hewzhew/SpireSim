use std::time::{Duration, Instant};

use sts_combat_planner::{
    LocalTurnGraphMilestoneClosurePathSnapshot, LocalTurnGraphPlanTransitionPathSnapshot,
    LocalTurnGraphRootActionFamilySnapshot,
};

use super::oracle_combat_witness_contract::OracleCombatWitnessCheckpointV1;
use super::oracle_combat_witness_options::OracleCombatWitnessOptionsV1;
use super::oracle_combat_witness_work::OracleRunCombatWitnessWorkV1;
use super::oracle_resident_combat_witness_evidence::OracleResidentCombatWitnessEvidenceV1;
use super::progress_options::{OracleCombatWitnessAdvanceV1, OracleCombatWitnessQuantumV1};
use super::session::{RunControlSession, RunProgressOutcome};
use crate::eval::combat_guidance_bundle::CombatGuidanceBundleV1;
use crate::state::core::ClientInput;

/// Opaque capability over one resident exact-combat search.
///
/// Analysis and explorer orchestration may grant bounded work, inspect typed
/// evidence, checkpoint, or commit a verified result. They never receive the
/// live local-graph/discrepancy sessions or their private queues.
pub struct OracleResidentCombatWitnessJobV1 {
    work: OracleRunCombatWitnessWorkV1,
}

impl OracleResidentCombatWitnessJobV1 {
    pub fn new(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        OracleRunCombatWitnessWorkV1::new_with_guidance(session, options, guidance)
            .map(|work| Self { work })
    }

    pub fn new_with_plan_transition_paths(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        OracleRunCombatWitnessWorkV1::new_with_plan_transition_paths(session, options, guidance)
            .map(|work| Self { work })
    }

    pub fn restore(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        checkpoint: OracleCombatWitnessCheckpointV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        OracleRunCombatWitnessWorkV1::restart_from_checkpoint_with_guidance(
            session, options, checkpoint, guidance,
        )
        .map(|work| Self { work })
    }

    pub fn promote(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        checkpoint: OracleCombatWitnessCheckpointV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        OracleRunCombatWitnessWorkV1::restart_for_higher_fidelity_with_guidance(
            session, options, checkpoint, guidance,
        )
        .map(|work| Self { work })
    }

    pub fn restart(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        OracleRunCombatWitnessWorkV1::restart_from_exact_state_with_guidance(
            session, options, guidance,
        )
        .map(|work| Self { work })
    }

    pub fn for_exact_actions(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            session, options, guidance,
        )
        .map(|work| Self { work })
    }

    pub fn root_action_families(&self) -> Vec<LocalTurnGraphRootActionFamilySnapshot> {
        self.work.root_action_families()
    }

    pub fn plan_transition_path_snapshots(&self) -> Vec<LocalTurnGraphPlanTransitionPathSnapshot> {
        self.work.plan_transition_path_snapshots()
    }

    pub fn milestone_closure_path_snapshots(
        &self,
    ) -> Vec<LocalTurnGraphMilestoneClosurePathSnapshot> {
        self.work.milestone_closure_path_snapshots()
    }

    pub fn checkpoint(&self) -> OracleCombatWitnessCheckpointV1 {
        self.work.checkpoint()
    }

    pub fn advance(
        &mut self,
        quantum: &OracleCombatWitnessQuantumV1,
        deadline: Option<Instant>,
    ) -> OracleCombatWitnessAdvanceV1 {
        self.work.advance(quantum, deadline)
    }

    pub fn advance_improving_incumbent(
        &mut self,
        quantum: &OracleCombatWitnessQuantumV1,
        deadline: Option<Instant>,
    ) -> OracleCombatWitnessAdvanceV1 {
        self.work.advance_improving_incumbent(quantum, deadline)
    }

    pub fn advance_current_stage_probe(
        &mut self,
        quantum: &OracleCombatWitnessQuantumV1,
        deadline: Option<Instant>,
    ) -> OracleCombatWitnessAdvanceV1 {
        self.work.advance_current_stage_probe(quantum, deadline)
    }

    pub fn ensure_requested_allowance(
        &mut self,
        requested_generation_work: usize,
        requested_wall_time: Option<Duration>,
    ) {
        self.work
            .ensure_requested_allowance(requested_generation_work, requested_wall_time);
    }

    pub fn mark_search_resume_exact(&mut self) {
        self.work.mark_search_resume_exact();
    }

    pub fn search_resume_exact(&self) -> bool {
        self.work.search_resume_exact()
    }

    pub fn has_verified_witness(&self) -> bool {
        self.work.has_verified_witness()
    }

    pub fn validate_exact_victory_frontier(&self) -> Result<(), String> {
        self.work.validate_exact_victory_frontier()
    }

    pub fn exact_root_matches(&self, root: &crate::sim::combat::CombatPosition) -> bool {
        self.work.exact_root_matches(root)
    }

    pub fn verified_witness_inputs(&self) -> Option<Vec<ClientInput>> {
        self.work.verified_witness_inputs()
    }

    /// Exposes all distinct replay-verified terminal lines retained by the
    /// resident search instead of only its compatibility incumbent.
    pub fn verified_witness_frontier_inputs(&self) -> Vec<Vec<ClientInput>> {
        self.work.verified_witness_frontier_inputs()
    }

    pub fn incumbent_hp_loss(&self) -> Option<u32> {
        self.work.incumbent_hp_loss()
    }

    pub fn has_refinement_ending_witness(&self) -> bool {
        self.work.has_refinement_ending_witness()
    }

    pub fn verify_and_restore_action_witness(
        &mut self,
        inputs: &[ClientInput],
    ) -> Result<(), String> {
        self.work.verify_and_restore_action_witness(inputs)
    }

    pub fn quantum_count(&self) -> usize {
        self.work.quantum_count()
    }

    pub fn remaining_generation_work(&self) -> usize {
        self.work.remaining_generation_work()
    }

    pub fn current_search_generation_work(&self) -> u64 {
        self.work.current_search_generation_work()
    }

    pub fn remaining_wall_ms(&self) -> Option<u64> {
        self.work.remaining_wall_ms()
    }

    pub fn max_potions_used(&self) -> Option<u32> {
        self.work.max_potions_used()
    }

    pub fn allowed_potion_slots(&self) -> Option<u64> {
        self.work.allowed_potion_slots()
    }

    pub fn restart_count(&self) -> usize {
        self.work.restart_count()
    }

    pub fn evidence(&self) -> OracleResidentCombatWitnessEvidenceV1 {
        self.work.progress()
    }

    pub(super) fn finish_and_apply(
        &self,
        session: &mut RunControlSession,
    ) -> Result<RunProgressOutcome, String> {
        self.work.finish_and_apply(session)
    }
}
