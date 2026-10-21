use std::sync::Arc;
use std::time::{Duration, Instant};

const MIN_USABLE_WALL_ALLOWANCE: Duration = Duration::from_millis(1);

use super::combat_line_executor::apply_oracle_combat_witness;
use super::oracle_combat_policy::RetainedCombatTacticalPolicyV1;
use super::oracle_combat_witness_contract::{
    OracleCombatWitnessCandidateDispositionV1, OracleCombatWitnessCheckpointV1,
    OracleCombatWitnessFailureV1,
};
use super::oracle_combat_witness_options::{
    prepare_oracle_combat_witness_v1, OracleCombatWitnessOptionsV1,
};
use super::oracle_resident_combat_witness_evidence::OracleResidentCombatWitnessEvidenceV1;
use super::progress_options::{OracleCombatWitnessAdvanceV1, OracleCombatWitnessQuantumV1};
use super::session::{RunControlSession, RunProgressOutcome};
use super::trace_annotation::CombatAutomationTrajectorySource;
use crate::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1;
use crate::ai::combat_witness_contract::{
    combat_witness_satisfies_materialized_quality_v1, CombatWitnessEngineV1,
    CombatWitnessSatisfactionV1,
};
use crate::eval::combat_guidance_bundle::CombatGuidanceBundleV1;
use crate::state::core::ClientInput;
use sts_combat_planner::{
    combat_plan_state_guide_policy_v1, root_initial_expansion_work_for_budget, CombatDecisionRoot,
    CombatGuideServiceBias, LocalTurnGraphMilestoneClosurePathSnapshot,
    LocalTurnGraphPlanTransitionPathSnapshot, LocalTurnGraphRootActionFamilySnapshot,
    LocalTurnGraphWitnessConfig, LocalTurnGraphWitnessQuantum, LocalTurnGraphWitnessSession,
    LocalTurnGraphWitnessStatus, OracleCombatWitness, OracleCombatWitnessDiscoverySource,
    OracleCombatWitnessSatisfaction, PolicyDiscrepancyConfig, PolicyDiscrepancyQuantum,
    PolicyDiscrepancySession, PolicyDiscrepancyStatus, PolicyDiscrepancyTurnMacroConfig,
    TurnOptionAction, TurnOptionGeneratorConfig, DEFAULT_BACKED_GENERATION_QUANTUM_WORK,
};

pub(super) struct OracleRunCombatWitnessWorkV1 {
    start: crate::sim::combat::CombatPosition,
    guide_service_bias: Option<CombatGuideServiceBias>,
    members: PortfolioMemberSessionsV1,
    budget: WitnessBudgetStateV1,
    potion_contract: WitnessPotionContractV1,
    incumbents: WitnessIncumbentStateV1,
    evidence: WitnessEvidenceStateV1,
    satisfaction: PortfolioWitnessSatisfactionV1,
}

struct PortfolioMemberSessionsV1 {
    local_search: LocalTurnGraphWitnessSession,
    discrepancy_search: PolicyDiscrepancySession,
    next_portfolio_member: PortfolioMemberV1,
    local_complete: bool,
    discrepancy_complete: bool,
    local_status: Option<LocalTurnGraphWitnessStatus>,
    discrepancy_status: Option<PolicyDiscrepancyStatus>,
}

impl PortfolioMemberSessionsV1 {
    fn select_next(
        &mut self,
        productive_member: Option<PortfolioMemberV1>,
    ) -> Option<PortfolioMemberV1> {
        let selected = select_productive_portfolio_member(
            productive_member,
            self.next_portfolio_member,
            self.local_complete,
            self.discrepancy_complete,
        )?;
        self.next_portfolio_member = selected.other();
        Some(selected)
    }

    fn record_result(&mut self, status: &PortfolioStatusV1, complete: bool) {
        match status {
            PortfolioStatusV1::Local(status) => {
                self.local_complete = complete;
                self.local_status = Some(status.clone());
            }
            PortfolioStatusV1::PolicyDiscrepancy(status) => {
                self.discrepancy_complete = complete;
                self.discrepancy_status = Some(status.clone());
            }
        }
    }

    fn all_complete(&self) -> bool {
        self.local_complete && self.discrepancy_complete
    }
}

struct WitnessBudgetStateV1 {
    remaining_generation_work: usize,
    remaining_engine_steps: usize,
    max_transition_steps: usize,
    remaining_wall_time: Option<Duration>,
}

impl WitnessBudgetStateV1 {
    fn new(
        generation_work: usize,
        max_transition_steps: usize,
        wall_time: Option<Duration>,
    ) -> Self {
        Self {
            remaining_generation_work: generation_work,
            remaining_engine_steps: generation_work.saturating_mul(max_transition_steps),
            max_transition_steps,
            remaining_wall_time: wall_time,
        }
    }

    fn restore_remaining(
        &mut self,
        generation_work: usize,
        engine_steps: usize,
        wall_ms: Option<u64>,
    ) {
        self.remaining_generation_work = self.remaining_generation_work.min(generation_work);
        self.remaining_engine_steps = self.remaining_engine_steps.min(engine_steps);
        self.remaining_wall_time = match (self.remaining_wall_time, wall_ms) {
            (Some(configured), Some(saved_ms)) => {
                Some(configured.min(Duration::from_millis(saved_ms)))
            }
            (None, Some(saved_ms)) => Some(Duration::from_millis(saved_ms)),
            (configured, None) => configured,
        };
    }

    fn generation_grant(&self, requested: usize) -> usize {
        requested.min(self.remaining_generation_work)
    }

    fn engine_grant(&self, generation_work: usize) -> usize {
        self.remaining_engine_steps
            .min(generation_work.saturating_mul(self.max_transition_steps))
    }

    fn charge(&mut self, generation_work: usize, engine_steps: usize, elapsed: Duration) {
        self.charge_work(generation_work, engine_steps);
        self.charge_wall(elapsed);
    }

    fn charge_work(&mut self, generation_work: usize, engine_steps: usize) {
        self.remaining_generation_work = self
            .remaining_generation_work
            .saturating_sub(generation_work);
        self.remaining_engine_steps = self.remaining_engine_steps.saturating_sub(engine_steps);
    }

    fn charge_wall(&mut self, elapsed: Duration) {
        if let Some(remaining) = &mut self.remaining_wall_time {
            *remaining = remaining.saturating_sub(elapsed);
        }
    }

    fn ensure_at_least(&mut self, generation_work: usize, wall_time: Option<Duration>) {
        self.remaining_generation_work = self.remaining_generation_work.max(generation_work);
        self.remaining_engine_steps = self
            .remaining_engine_steps
            .max(generation_work.saturating_mul(self.max_transition_steps));
        if let (Some(remaining), Some(requested)) = (&mut self.remaining_wall_time, wall_time) {
            *remaining = (*remaining).max(requested);
        }
    }

    fn exhausted(&self) -> bool {
        self.remaining_generation_work == 0
            || self.remaining_engine_steps == 0
            || wall_allowance_exhausted(self.remaining_wall_time)
    }

    fn remaining_wall_ms(&self) -> Option<u64> {
        self.remaining_wall_time
            .map(|remaining| remaining.as_millis().min(u64::MAX as u128) as u64)
    }
}

struct WitnessPotionContractV1 {
    max_potions_used: Option<u32>,
    allowed_potion_slots: Option<u64>,
    allow_potion_discard: bool,
    potion_spend_requires_satisfaction: bool,
}

impl WitnessPotionContractV1 {
    fn within_resource_limits(
        &self,
        start: &crate::sim::combat::CombatPosition,
        witness: &OracleCombatWitness,
    ) -> bool {
        combat_witness_within_potion_contract(
            start,
            witness,
            self.max_potions_used,
            self.allowed_potion_slots,
        )
    }

    fn accepts_witness(
        &self,
        start: &crate::sim::combat::CombatPosition,
        witness: &OracleCombatWitness,
    ) -> bool {
        self.within_resource_limits(start, witness)
            && (self.allow_potion_discard || !combat_witness_uses_potion_discard(witness))
    }

    fn accepts_inputs(&self, inputs: &[ClientInput]) -> bool {
        self.allow_potion_discard
            || !inputs
                .iter()
                .any(|input| matches!(input, ClientInput::DiscardPotion(_)))
    }

    fn restore_protection_requirement(&mut self, required: bool) {
        self.potion_spend_requires_satisfaction = required;
    }

    fn protect_on_expansion(
        &mut self,
        prior_max_potions_used: Option<u32>,
        prior_required: bool,
        has_protected_incumbent: bool,
    ) {
        let expands_from_potion_free = prior_max_potions_used == Some(0)
            && self.max_potions_used.is_some_and(|limit| limit > 0);
        self.potion_spend_requires_satisfaction =
            prior_required || (expands_from_potion_free && has_protected_incumbent);
    }
}

#[derive(Default)]
struct WitnessIncumbentStateV1 {
    protected_potion_free_incumbent: Option<OracleCombatWitness>,
    prior_stage_incumbent: Option<OracleCombatWitness>,
    stage_entry_incumbent: Option<OracleCombatWitness>,
    discrepancy_witness: Option<OracleCombatWitness>,
}

struct WitnessEvidenceStateV1 {
    quantum_count: usize,
    prior_generation_work: u64,
    plan_prefix_proposals: usize,
    plan_prefix_proposed_turns: usize,
    plan_prefix_proposed_actions: usize,
    plan_prefix_proposal_rejections: usize,
    restart_count: usize,
    last_status: Option<PortfolioStatusV1>,
    incumbent_revision: u64,
    quanta_since_incumbent_improvement: usize,
    last_quantum_generation_work: usize,
    last_quantum_engine_steps: usize,
    search_resume_exact: bool,
    witness_source: CombatAutomationTrajectorySource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortfolioMemberV1 {
    LocalTurnGraph,
    PolicyDiscrepancy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortfolioAdvanceModeV1 {
    StopOnSatisfaction,
    RefineToSatisfaction,
    ProbeCurrentStage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortfolioWitnessSatisfactionV1 {
    FirstWitness,
    HpLossAtMost(u32),
    MaterializedPersistentPayoffGain,
    FirstWitnessWithoutNewExternalBurden,
    HpLossAtMostWithoutNewExternalBurden(u32),
    PotionFreeHpLossAtMostWithoutNewExternalBurden(u32),
    BudgetOrExhaustion,
}

impl PortfolioMemberV1 {
    fn other(self) -> Self {
        match self {
            Self::LocalTurnGraph => Self::PolicyDiscrepancy,
            Self::PolicyDiscrepancy => Self::LocalTurnGraph,
        }
    }
}

#[derive(Clone, Debug)]
enum PortfolioStatusV1 {
    Local(LocalTurnGraphWitnessStatus),
    PolicyDiscrepancy(PolicyDiscrepancyStatus),
}

impl OracleRunCombatWitnessWorkV1 {
    pub(super) fn root_action_families(&self) -> Vec<LocalTurnGraphRootActionFamilySnapshot> {
        self.members.local_search.root_action_families()
    }

    pub(super) fn plan_transition_path_snapshots(
        &self,
    ) -> Vec<LocalTurnGraphPlanTransitionPathSnapshot> {
        self.members.local_search.plan_transition_path_snapshots()
    }

    pub(super) fn milestone_closure_path_snapshots(
        &self,
    ) -> Vec<LocalTurnGraphMilestoneClosurePathSnapshot> {
        self.members.local_search.milestone_closure_path_snapshots()
    }

    pub(super) fn new_with_guidance(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        Self::new_exact_planner(session, options, guidance)
    }

    pub(super) fn new_with_plan_transition_paths(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        Self::new_exact_planner_internal(session, options, guidance, true)
    }

    fn new_exact_planner(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        Self::new_exact_planner_internal(session, options, guidance, false)
    }

    fn new_exact_planner_internal(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
        collect_plan_transition_paths: bool,
    ) -> Result<Self, String> {
        let prepared = prepare_oracle_combat_witness_v1(session, options)?;
        let max_transition_steps = prepared.max_engine_steps_per_transition;
        let max_generation_work = prepared.max_generation_work;
        let allow_potion_discard = prepared.allow_potion_discard;
        let (satisfaction, planner_satisfaction) = match prepared.satisfaction {
            CombatWitnessSatisfactionV1::BudgetOrExhaustion => (
                PortfolioWitnessSatisfactionV1::BudgetOrExhaustion,
                OracleCombatWitnessSatisfaction::BudgetOrExhaustion,
            ),
            CombatWitnessSatisfactionV1::ZeroLossOrBudget => (
                PortfolioWitnessSatisfactionV1::HpLossAtMost(0),
                OracleCombatWitnessSatisfaction::HpLossAtMost(0),
            ),
            CombatWitnessSatisfactionV1::FirstCompleteWin => (
                PortfolioWitnessSatisfactionV1::FirstWitness,
                OracleCombatWitnessSatisfaction::FirstWitness,
            ),
            CombatWitnessSatisfactionV1::HpLossAtMost(limit) => (
                PortfolioWitnessSatisfactionV1::HpLossAtMost(limit),
                OracleCombatWitnessSatisfaction::HpLossAtMost(limit),
            ),
            CombatWitnessSatisfactionV1::MaterializedPersistentPayoffGain => (
                PortfolioWitnessSatisfactionV1::MaterializedPersistentPayoffGain,
                OracleCombatWitnessSatisfaction::BudgetOrExhaustion,
            ),
            CombatWitnessSatisfactionV1::FirstCompleteWinWithoutNewExternalBurden => (
                PortfolioWitnessSatisfactionV1::FirstWitnessWithoutNewExternalBurden,
                OracleCombatWitnessSatisfaction::FirstWitnessWithoutNewExternalBurden,
            ),
            CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(limit) => (
                PortfolioWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(limit),
                OracleCombatWitnessSatisfaction::HpLossAtMostWithoutNewExternalBurden(limit),
            ),
            CombatWitnessSatisfactionV1::PotionFreeHpLossAtMostWithoutNewExternalBurden(limit) => (
                PortfolioWitnessSatisfactionV1::PotionFreeHpLossAtMostWithoutNewExternalBurden(
                    limit,
                ),
                OracleCombatWitnessSatisfaction::PotionFreeHpLossAtMostWithoutNewExternalBurden(
                    limit,
                ),
            ),
        };
        let root = CombatDecisionRoot::new(prepared.start.clone())
            .map_err(|error| format!("invalid oracle combat root: {error:?}"))?;
        let guide_service_bias =
            sts_combat_knowledge::darkling_survival_guide_service_bias_v1(&prepared.start);
        let policy = Arc::new(RetainedCombatTacticalPolicyV1);
        let policy = if let Some(guidance) = guidance {
            guidance.policy(policy)?
        } else {
            policy
        };
        let policy = combat_plan_state_guide_policy_v1(policy);
        let mut local_search = LocalTurnGraphWitnessSession::with_policy(
            root.clone(),
            LocalTurnGraphWitnessConfig {
                generator: TurnOptionGeneratorConfig {
                    max_engine_steps_per_transition: max_transition_steps,
                    allow_potion_expenditure: prepared.max_potions_used != Some(0),
                    allow_potion_discard,
                    allowed_potion_slots: prepared.allowed_potion_slots,
                    ..TurnOptionGeneratorConfig::default()
                },
                generation_quantum_work: 4,
                backed_generation_quantum_work: DEFAULT_BACKED_GENERATION_QUANTUM_WORK,
                guide_service_bias,
                initial_expansion_work: 64,
                root_initial_expansion_work: root_initial_expansion_work_for_budget(
                    max_generation_work,
                ),
                max_turn_depth: 32,
                satisfaction: planner_satisfaction,
                require_no_unrecovered_stolen_gold: matches!(
                    satisfaction,
                    PortfolioWitnessSatisfactionV1::HpLossAtMost(_)
                        | PortfolioWitnessSatisfactionV1::FirstWitnessWithoutNewExternalBurden
                        | PortfolioWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(_)
                        | PortfolioWitnessSatisfactionV1::PotionFreeHpLossAtMostWithoutNewExternalBurden(_)
                ),
                max_potions_used: prepared.max_potions_used,
            },
            policy.clone(),
        );
        if collect_plan_transition_paths {
            local_search
                .enable_plan_transition_annotations()
                .map_err(|error| {
                    format!(
                        "cannot enable resident plan-transition paths before graph work: {error:?}"
                    )
                })?;
        }
        let discrepancy_search = PolicyDiscrepancySession::with_policy(
            root,
            PolicyDiscrepancyConfig {
                max_engine_steps_per_transition: max_transition_steps,
                turn_macro: Some(PolicyDiscrepancyTurnMacroConfig {
                    max_applied_transitions: 4_096,
                    proposals_per_view: 8,
                    ..PolicyDiscrepancyTurnMacroConfig::default()
                }),
                max_potions_used: prepared.max_potions_used,
                allow_potion_discard,
                allowed_potion_slots: prepared.allowed_potion_slots,
                ..PolicyDiscrepancyConfig::default()
            },
            policy,
        );
        let mut work = Self {
            start: prepared.start,
            guide_service_bias,
            members: PortfolioMemberSessionsV1 {
                local_search,
                discrepancy_search,
                next_portfolio_member: PortfolioMemberV1::LocalTurnGraph,
                local_complete: false,
                discrepancy_complete: false,
                local_status: None,
                discrepancy_status: None,
            },
            budget: WitnessBudgetStateV1::new(
                max_generation_work,
                max_transition_steps,
                prepared.wall_time,
            ),
            potion_contract: WitnessPotionContractV1 {
                max_potions_used: prepared.max_potions_used,
                allowed_potion_slots: prepared.allowed_potion_slots,
                allow_potion_discard,
                potion_spend_requires_satisfaction: false,
            },
            incumbents: WitnessIncumbentStateV1::default(),
            satisfaction,
            evidence: WitnessEvidenceStateV1 {
                quantum_count: 0,
                prior_generation_work: 0,
                plan_prefix_proposals: 0,
                plan_prefix_proposed_turns: 0,
                plan_prefix_proposed_actions: 0,
                plan_prefix_proposal_rejections: 0,
                restart_count: 0,
                last_status: None,
                incumbent_revision: 0,
                quanta_since_incumbent_improvement: 0,
                last_quantum_generation_work: 0,
                last_quantum_engine_steps: 0,
                search_resume_exact: false,
                witness_source: CombatAutomationTrajectorySource::TurnGraphPortfolioV1,
            },
        };
        work.offer_initial_plan_prefix();
        Ok(work)
    }

    pub(super) fn restart_from_checkpoint_with_guidance(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        checkpoint: OracleCombatWitnessCheckpointV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        // A process restart restores the already charged allowance and exact
        // incumbent. The tactical frontier itself is intentionally rebuilt.
        let mut work = Self::new_exact_planner(session, options, guidance)?;
        work.budget.restore_remaining(
            checkpoint.remaining_generation_work,
            checkpoint.remaining_engine_steps,
            checkpoint.remaining_wall_ms,
        );
        work.evidence.quantum_count = checkpoint.quantum_count;
        work.evidence.prior_generation_work = checkpoint.consumed_generation_work;
        work.evidence.restart_count = checkpoint.restart_count.saturating_add(1);
        work.evidence.incumbent_revision = checkpoint.incumbent_revision;
        work.evidence.quanta_since_incumbent_improvement =
            checkpoint.quanta_since_incumbent_improvement;
        work.potion_contract
            .restore_protection_requirement(checkpoint.potion_spend_requires_satisfaction);
        if work.potion_contract.potion_spend_requires_satisfaction {
            work.incumbents.protected_potion_free_incumbent = checkpoint
                .incumbent
                .as_ref()
                .filter(|incumbent| combat_witness_potion_expenditures(&work.start, incumbent) == 0)
                .cloned();
        }
        work.incumbents.stage_entry_incumbent = checkpoint.incumbent.clone();
        if let Some(incumbent) = checkpoint.incumbent {
            work.restore_checkpoint_incumbent(incumbent)?;
        }
        Ok(work)
    }

    /// Restarts an exact combat at a higher configured fidelity while
    /// preserving all charged work and any already verified incumbent. The
    /// tactical frontier itself is intentionally not serialized, so the
    /// restart is explicit in both accounting and diagnostics.
    pub(super) fn restart_for_higher_fidelity_with_guidance(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        prior: OracleCombatWitnessCheckpointV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        let mut work = Self::new_exact_planner(session, options, guidance)?;
        work.evidence.quantum_count = prior.quantum_count;
        work.evidence.prior_generation_work = prior.consumed_generation_work;
        work.evidence.restart_count = prior.restart_count.saturating_add(1);
        work.evidence.incumbent_revision = prior.incumbent_revision;
        work.evidence.quanta_since_incumbent_improvement = prior.quanta_since_incumbent_improvement;
        let protected_incumbent = prior
            .incumbent
            .as_ref()
            .filter(|incumbent| combat_witness_potion_expenditures(&work.start, incumbent) == 0)
            .cloned();
        work.potion_contract.protect_on_expansion(
            prior.max_potions_used,
            prior.potion_spend_requires_satisfaction,
            protected_incumbent.is_some(),
        );
        if work.potion_contract.potion_spend_requires_satisfaction {
            work.incumbents.protected_potion_free_incumbent = protected_incumbent;
        }
        work.incumbents.stage_entry_incumbent = prior.incumbent.clone();
        if let Some(incumbent) = prior.incumbent {
            work.restore_checkpoint_incumbent(incumbent)?;
        }
        Ok(work)
    }

    fn restore_checkpoint_incumbent(
        &mut self,
        incumbent: OracleCombatWitness,
    ) -> Result<(), String> {
        self.verify_checkpoint_incumbent(&incumbent)?;
        if !self
            .potion_contract
            .within_resource_limits(&self.start, &incumbent)
        {
            self.incumbents.prior_stage_incumbent = Some(incumbent);
            return Ok(());
        }
        if incumbent.discovery_source == OracleCombatWitnessDiscoverySource::PolicyDiscrepancySearch
        {
            self.incumbents.discrepancy_witness = Some(incumbent);
        } else {
            self.members
                .local_search
                .restore_verified_witness(incumbent)?;
        }
        self.incumbents.prior_stage_incumbent = None;
        Ok(())
    }

    fn verify_checkpoint_incumbent(&self, incumbent: &OracleCombatWitness) -> Result<(), String> {
        use crate::sim::combat::CombatStepper;

        if incumbent.actions.is_empty() {
            return Err("checkpoint incumbent contains no combat actions".to_string());
        }
        let stepper = crate::sim::combat::EngineCombatStepper;
        let mut position = self.start.clone();
        for (index, action) in incumbent.actions.iter().enumerate() {
            if stepper
                .choice_for_legal_input(&position, &action.input)
                .is_none()
            {
                return Err(format!(
                    "checkpoint incumbent action {index} is not legal at its exact state"
                ));
            }
            let result = stepper.apply_to_stable(
                &position,
                action.input.clone(),
                crate::sim::combat::CombatStepLimits {
                    max_engine_steps: self.budget.max_transition_steps,
                    deadline: None,
                },
            );
            if result.truncated {
                return Err(format!(
                    "checkpoint incumbent action {index} exceeded the transition limit"
                ));
            }
            position = result.position;
        }
        if position != incumbent.final_position {
            return Err(
                "checkpoint incumbent final position does not match its exact replay".to_string(),
            );
        }
        if !crate::sim::combat::is_exact_combat_victory(&position) {
            return Err("checkpoint incumbent exact replay is not terminal victory".to_string());
        }
        Ok(())
    }

    fn offer_initial_plan_prefix(&mut self) {
        const MAX_PLAN_PREFIX_TURNS: usize = 6;
        const MAX_PLAN_PREFIX_ACTIONS: usize = 64;

        let max_actions = MAX_PLAN_PREFIX_ACTIONS.min(self.budget.remaining_generation_work);
        if max_actions == 0
            || wall_allowance_exhausted(self.budget.remaining_wall_time)
            || !self
                .members
                .local_search
                .has_supported_initial_plan_prefix()
        {
            return;
        }
        let started = Instant::now();
        let report = self.members.local_search.offer_plan_compatible_policy_line(
            MAX_PLAN_PREFIX_TURNS,
            max_actions,
            &crate::sim::combat::EngineCombatStepper,
        );
        self.budget.charge_wall(started.elapsed());
        let report = match report {
            Ok(report) => report,
            Err(_) => {
                self.evidence.plan_prefix_proposal_rejections = self
                    .evidence
                    .plan_prefix_proposal_rejections
                    .saturating_add(1);
                return;
            }
        };
        if report.proposed_turns == 0 {
            return;
        }

        self.evidence.plan_prefix_proposals = self.evidence.plan_prefix_proposals.saturating_add(1);
        self.evidence.plan_prefix_proposed_turns = self
            .evidence
            .plan_prefix_proposed_turns
            .saturating_add(report.proposed_turns);
        self.evidence.plan_prefix_proposed_actions = self
            .evidence
            .plan_prefix_proposed_actions
            .saturating_add(report.proposed_actions.len());
        self.budget
            .charge_work(report.proposed_actions.len(), report.engine_steps);
    }

    /// Restores a legacy exact combat state whose checkpoint did not preserve
    /// tactical allowance or incumbent information.  It must be reported as a
    /// search restart even though its allowance necessarily starts fresh.
    pub(super) fn restart_from_exact_state_with_guidance(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        let mut work = Self::new_with_guidance(session, options, guidance)?;
        work.evidence.restart_count = 1;
        Ok(work)
    }

    pub(super) fn for_exact_action_witness_with_guidance(
        session: &RunControlSession,
        options: OracleCombatWitnessOptionsV1,
        guidance: Option<&CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        // Exact analyst actions need simulator verification before they can
        // enter the same terminal frontier as planner-discovered witnesses.
        Self::new_exact_planner(session, options, guidance)
    }

    pub(super) fn checkpoint(&self) -> OracleCombatWitnessCheckpointV1 {
        OracleCombatWitnessCheckpointV1 {
            consumed_generation_work: self.generation_work(),
            remaining_generation_work: self.budget.remaining_generation_work,
            remaining_engine_steps: self.budget.remaining_engine_steps,
            remaining_wall_ms: self.remaining_wall_ms(),
            quantum_count: self.evidence.quantum_count,
            restart_count: self.evidence.restart_count,
            incumbent_revision: self.evidence.incumbent_revision,
            quanta_since_incumbent_improvement: self.evidence.quanta_since_incumbent_improvement,
            max_potions_used: self.potion_contract.max_potions_used,
            allowed_potion_slots: self.potion_contract.allowed_potion_slots,
            potion_spend_requires_satisfaction: self
                .potion_contract
                .potion_spend_requires_satisfaction,
            incumbent: self.best_witness().cloned(),
        }
    }

    pub(super) fn advance(
        &mut self,
        quantum: &OracleCombatWitnessQuantumV1,
        global_deadline: Option<Instant>,
    ) -> OracleCombatWitnessAdvanceV1 {
        // "First" is relative to the configured owner satisfaction. Survival
        // mode still accepts the first exact win; strategic quality modes keep
        // an insufficient win safe while using only this stage's allowance to
        // seek a quality-reaching replacement.
        self.advance_with_witness_policy(
            quantum,
            global_deadline,
            PortfolioAdvanceModeV1::StopOnSatisfaction,
        )
    }

    /// Continues serving the portfolio past an insufficient first witness, but
    /// honors the configured quality satisfaction instead of mechanically
    /// consuming the whole allowance. `BudgetOrExhaustion` remains available
    /// for an analyst who explicitly requests exhaustive bounded improvement.
    pub(super) fn advance_improving_incumbent(
        &mut self,
        quantum: &OracleCombatWitnessQuantumV1,
        global_deadline: Option<Instant>,
    ) -> OracleCombatWitnessAdvanceV1 {
        self.advance_with_witness_policy(
            quantum,
            global_deadline,
            PortfolioAdvanceModeV1::RefineToSatisfaction,
        )
    }

    /// Serves only the currently configured potion/stage identity for the
    /// caller's bounded grant. Strategic satisfaction does not end this probe;
    /// the analysis session keeps both the incumbent and frontier resident so
    /// the caller can inspect, continue, or explicitly accept the result.
    pub(super) fn advance_current_stage_probe(
        &mut self,
        quantum: &OracleCombatWitnessQuantumV1,
        global_deadline: Option<Instant>,
    ) -> OracleCombatWitnessAdvanceV1 {
        // The portfolio layer is not the only satisfaction owner: the local
        // graph also closes itself when its configured witness threshold is
        // reached. Temporarily switch that resumable graph to budget service
        // so this explicit probe cannot become another threshold-limited
        // refinement call in disguise.
        let configured_satisfaction = planner_satisfaction(self.satisfaction);
        self.members
            .local_search
            .set_satisfaction(OracleCombatWitnessSatisfaction::BudgetOrExhaustion);
        let advance = self.advance_with_witness_policy(
            quantum,
            global_deadline,
            PortfolioAdvanceModeV1::ProbeCurrentStage,
        );
        self.members
            .local_search
            .set_satisfaction(configured_satisfaction);
        advance
    }

    fn advance_with_witness_policy(
        &mut self,
        quantum: &OracleCombatWitnessQuantumV1,
        global_deadline: Option<Instant>,
        mode: PortfolioAdvanceModeV1,
    ) -> OracleCombatWitnessAdvanceV1 {
        let now = Instant::now();
        let global_remaining =
            global_deadline.map(|deadline| deadline.saturating_duration_since(now));
        if global_remaining == Some(Duration::ZERO) {
            return OracleCombatWitnessAdvanceV1::GlobalDeadlineReached;
        }
        let work = self
            .budget
            .generation_grant(quantum.additional_generation_work);
        if work == 0 || wall_allowance_exhausted(self.budget.remaining_wall_time) {
            return if self.best_witness().is_some() {
                OracleCombatWitnessAdvanceV1::ReadyToFinish
            } else {
                OracleCombatWitnessAdvanceV1::AllowanceExhausted
            };
        }
        let requested_wall = quantum.soft_wall_ms.map(Duration::from_millis);
        let soft_wall = [
            requested_wall,
            self.budget.remaining_wall_time,
            global_remaining,
        ]
        .into_iter()
        .flatten()
        .min();
        if soft_wall == Some(Duration::ZERO) {
            return if global_remaining == Some(Duration::ZERO) {
                OracleCombatWitnessAdvanceV1::GlobalDeadlineReached
            } else if self.best_witness().is_some() {
                OracleCombatWitnessAdvanceV1::ReadyToFinish
            } else {
                OracleCombatWitnessAdvanceV1::AllowanceExhausted
            };
        }
        let deadline = soft_wall.and_then(|duration| now.checked_add(duration));
        self.evidence.last_quantum_generation_work = 0;
        self.evidence.last_quantum_engine_steps = 0;
        let before_incumbent_quality = self
            .best_witness()
            .map(|witness| combat_witness_quality(&self.start, witness));
        let engine_grant = self.budget.engine_grant(work);
        let productive_member = if mode == PortfolioAdvanceModeV1::StopOnSatisfaction
            && self.satisfaction != PortfolioWitnessSatisfactionV1::BudgetOrExhaustion
        {
            self.best_witness().and_then(|witness| {
                if combat_witness_satisfies(self.satisfaction, &self.start, witness) {
                    return None;
                }
                match witness.discovery_source {
                    OracleCombatWitnessDiscoverySource::PlannerSearch => {
                        Some(PortfolioMemberV1::LocalTurnGraph)
                    }
                    OracleCombatWitnessDiscoverySource::PolicyDiscrepancySearch => {
                        Some(PortfolioMemberV1::PolicyDiscrepancy)
                    }
                    _ => None,
                }
            })
        } else {
            None
        };
        let Some(member) = self.members.select_next(productive_member) else {
            return OracleCombatWitnessAdvanceV1::ReadyToFinish;
        };
        let (consumed_work, consumed_engine, member_complete, status) = match member {
            PortfolioMemberV1::LocalTurnGraph => {
                let before = self.members.local_search.counters();
                let report = self.members.local_search.advance(
                    LocalTurnGraphWitnessQuantum {
                        additional_selections: work,
                        additional_generation_work: work,
                        additional_engine_steps: engine_grant,
                        deadline,
                    },
                    &crate::sim::combat::EngineCombatStepper,
                );
                let after = report.counters;
                let member_complete =
                    !matches!(&report.status, LocalTurnGraphWitnessStatus::Partial(_));
                (
                    after.generation_work.saturating_sub(before.generation_work),
                    after.engine_steps.saturating_sub(before.engine_steps),
                    member_complete,
                    PortfolioStatusV1::Local(report.status),
                )
            }
            PortfolioMemberV1::PolicyDiscrepancy => {
                let before = self.members.discrepancy_search.counters();
                let report = self.members.discrepancy_search.advance(
                    &crate::sim::combat::EngineCombatStepper,
                    PolicyDiscrepancyQuantum {
                        additional_applied_transitions: work,
                        additional_engine_steps: engine_grant,
                        deadline,
                    },
                );
                let after = report.after;
                let status = report.status;
                if let Some(witness) = report.witness {
                    let witness = OracleCombatWitness {
                        actions: witness.actions,
                        final_position: witness.final_position,
                        negative_log_policy: witness.negative_log_policy,
                        replay_engine_steps: witness.replay_engine_steps,
                        discovery_source:
                            OracleCombatWitnessDiscoverySource::PolicyDiscrepancySearch,
                    };
                    if self
                        .incumbents
                        .discrepancy_witness
                        .as_ref()
                        .is_none_or(|current| combat_witness_better(&self.start, &witness, current))
                    {
                        self.incumbents.discrepancy_witness = Some(witness);
                    }
                }
                let member_complete = !matches!(status, PolicyDiscrepancyStatus::Partial(_));
                (
                    after
                        .applied_action_transitions
                        .saturating_sub(before.applied_action_transitions),
                    after.engine_steps.saturating_sub(before.engine_steps),
                    member_complete,
                    PortfolioStatusV1::PolicyDiscrepancy(status),
                )
            }
        };
        self.members.record_result(&status, member_complete);
        self.evidence.last_quantum_generation_work = consumed_work;
        self.evidence.last_quantum_engine_steps = consumed_engine;
        let after_incumbent_quality = self
            .best_witness()
            .map(|witness| combat_witness_quality(&self.start, witness));
        let incumbent_improved = match (before_incumbent_quality, after_incumbent_quality) {
            (None, Some(_)) => true,
            (Some(before), Some(after)) => combat_witness_quality_better(after, before),
            _ => false,
        };
        let acceptance_improved =
            combat_witness_acceptance_improved(before_incumbent_quality, after_incumbent_quality);
        if incumbent_improved {
            self.evidence.incumbent_revision = self.evidence.incumbent_revision.saturating_add(1);
            self.evidence.quanta_since_incumbent_improvement = 0;
        } else {
            self.evidence.quanta_since_incumbent_improvement = self
                .evidence
                .quanta_since_incumbent_improvement
                .saturating_add(1);
        }
        self.budget
            .charge(consumed_work, consumed_engine, now.elapsed());
        self.evidence.quantum_count = self.evidence.quantum_count.saturating_add(1);
        self.evidence.last_status = Some(status);
        let inherited_satisfying_challenge_complete = inherited_satisfying_incumbent_challenged(
            mode == PortfolioAdvanceModeV1::StopOnSatisfaction,
            self.satisfaction,
            &self.start,
            self.incumbents.stage_entry_incumbent.as_ref(),
            self.current_local_search_work(),
            quantum.additional_generation_work,
        );
        let standard_satisfaction_reached = standard_witness_ends_stage(
            mode == PortfolioAdvanceModeV1::StopOnSatisfaction,
            acceptance_improved,
            self.satisfaction,
            &self.start,
            self.best_witness(),
        );
        let quality_satisfied = mode == PortfolioAdvanceModeV1::RefineToSatisfaction
            && self.has_refinement_ending_witness();
        if standard_satisfaction_reached
            || inherited_satisfying_challenge_complete
            || quality_satisfied
            || self.members.all_complete()
        {
            OracleCombatWitnessAdvanceV1::ReadyToFinish
        } else if self.budget.exhausted() {
            if self.best_witness().is_some() {
                OracleCombatWitnessAdvanceV1::ReadyToFinish
            } else {
                OracleCombatWitnessAdvanceV1::AllowanceExhausted
            }
        } else {
            OracleCombatWitnessAdvanceV1::Pending
        }
    }

    /// Extends only an exhausted allowance dimension. The tactical frontier,
    /// transposition table, generators, and incumbent remain resident.
    /// Ensures an explicit analysis request receives the allowance it asked
    /// for without discarding an existing tactical frontier. In particular,
    /// a two-second tail from the previous request must not consume a whole
    /// autosave cycle before a requested thirty-second continuation begins.
    pub(super) fn ensure_requested_allowance(
        &mut self,
        requested_generation_work: usize,
        requested_wall_time: Option<Duration>,
    ) {
        self.budget
            .ensure_at_least(requested_generation_work, requested_wall_time);
    }

    pub(super) fn mark_search_resume_exact(&mut self) {
        if self.evidence.quantum_count > 0 {
            self.evidence.search_resume_exact = true;
        }
    }

    pub(super) fn search_resume_exact(&self) -> bool {
        self.evidence.search_resume_exact
    }

    pub(super) fn has_verified_witness(&self) -> bool {
        self.best_witness().is_some()
    }

    pub(super) fn validate_exact_victory_frontier(&self) -> Result<(), String> {
        let retained = self.members.local_search.witness_frontier().iter().chain(
            [
                self.incumbents.discrepancy_witness.as_ref(),
                self.incumbents.protected_potion_free_incumbent.as_ref(),
                self.incumbents.prior_stage_incumbent.as_ref(),
            ]
            .into_iter()
            .flatten(),
        );
        for witness in retained {
            self.verify_checkpoint_incumbent(witness).map_err(|error| {
                format!(
                    "resident retained a non-replayable combat witness: source={:?} actions={}: {error}",
                    witness.discovery_source,
                    witness.actions.len(),
                )
            })?;
            if !crate::sim::combat::is_exact_combat_victory(&witness.final_position) {
                return Err(format!(
                    "resident retained a non-victory combat witness: source={:?} terminal={:?} combat_smoked={} actions={}",
                    witness.discovery_source,
                    crate::sim::combat::combat_terminal(
                        &witness.final_position.engine,
                        &witness.final_position.combat,
                    ),
                    witness.final_position.combat.runtime.combat_smoked,
                    witness.actions.len(),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn exact_root_matches(&self, root: &crate::sim::combat::CombatPosition) -> bool {
        self.start == *root
    }

    pub(super) fn verified_witness_inputs(&self) -> Option<Vec<ClientInput>> {
        self.best_witness().map(|witness| {
            witness
                .actions
                .iter()
                .map(|action| action.input.clone())
                .collect()
        })
    }

    /// Returns every distinct replay-verified terminal line retained by the
    /// resident portfolio. Unlike `verified_witness_inputs`, this boundary
    /// does not collapse incomparable HP, potion, or persistent outcomes to
    /// the portfolio's compatibility incumbent.
    pub(super) fn verified_witness_frontier_inputs(&self) -> Vec<Vec<ClientInput>> {
        let witnesses = self
            .members
            .local_search
            .witness_frontier()
            .iter()
            .chain(
                [
                    self.incumbents.discrepancy_witness.as_ref(),
                    self.incumbents.protected_potion_free_incumbent.as_ref(),
                    self.incumbents.prior_stage_incumbent.as_ref(),
                ]
                .into_iter()
                .flatten(),
            )
            .filter(|witness| self.potion_contract.accepts_witness(&self.start, witness));
        let mut inputs = Vec::new();
        for witness in witnesses {
            let candidate = witness
                .actions
                .iter()
                .map(|action| action.input.clone())
                .collect::<Vec<_>>();
            if !inputs.contains(&candidate) {
                inputs.push(candidate);
            }
        }
        inputs
    }

    pub(super) fn incumbent_hp_loss(&self) -> Option<u32> {
        let initial_hp = self.start.combat.entities.player.current_hp;
        self.best_witness().map(|witness| {
            initial_hp
                .saturating_sub(witness.final_position.combat.entities.player.current_hp)
                .max(0) as u32
        })
    }

    pub(super) fn has_refinement_ending_witness(&self) -> bool {
        self.members
            .local_search
            .witness_frontier()
            .iter()
            .chain(
                [
                    self.incumbents.discrepancy_witness.as_ref(),
                    self.incumbents.protected_potion_free_incumbent.as_ref(),
                    self.incumbents.prior_stage_incumbent.as_ref(),
                ]
                .into_iter()
                .flatten(),
            )
            .filter(|witness| self.potion_contract.accepts_witness(&self.start, witness))
            .any(|witness| {
                combat_witness_ends_quality_refinement(
                    &self.start,
                    self.satisfaction,
                    self.potion_contract.potion_spend_requires_satisfaction,
                    witness,
                )
            })
    }

    /// Replays an analyst-supplied exact action sequence from this job's
    /// unchanged combat root and installs it only when every action is legal
    /// and the simulator reaches a terminal victory. This is an explicit
    /// oracle-analysis operation, not a search claim or heuristic shortcut.
    pub(super) fn verify_and_restore_action_witness(
        &mut self,
        inputs: &[ClientInput],
    ) -> Result<(), String> {
        if !self.potion_contract.accepts_inputs(inputs) {
            return Err(
                "oracle combat witness uses potion discard outside an all-legal search policy"
                    .to_string(),
            );
        }
        let stepper = crate::sim::combat::EngineCombatStepper;
        let mut position = self.start.clone();
        let mut actions = Vec::with_capacity(inputs.len());
        let mut replay_engine_steps = 0usize;
        for (index, input) in inputs.iter().enumerate() {
            use crate::sim::combat::CombatStepper;

            if stepper.choice_for_legal_input(&position, input).is_none() {
                return Err(format!(
                    "oracle combat witness action {index} is not legal at its exact state: {input:?}"
                ));
            }
            let result = stepper.apply_to_stable(
                &position,
                input.clone(),
                crate::sim::combat::CombatStepLimits {
                    max_engine_steps: self.budget.max_transition_steps,
                    deadline: None,
                },
            );
            if result.truncated {
                return Err(format!(
                    "oracle combat witness action {index} exceeded the transition limit"
                ));
            }
            replay_engine_steps = replay_engine_steps.saturating_add(result.engine_steps);
            actions.push(TurnOptionAction {
                input: input.clone(),
                expected_successor_hash: crate::ai::combat_state_key::combat_exact_state_hash_v2(
                    &result.position.engine,
                    &result.position.combat,
                )
                .into(),
                engine_steps: result.engine_steps,
            });
            position = result.position;
        }
        if !crate::sim::combat::is_exact_combat_victory(&position) {
            return Err("oracle combat witness actions did not reach terminal victory".to_string());
        }
        let witness = OracleCombatWitness {
            actions,
            final_position: position,
            // The sequence is accepted for its exact replay proof. Search
            // may still replace it with an equal-HP, shorter witness later.
            negative_log_policy: inputs.len() as f64,
            replay_engine_steps,
            discovery_source: OracleCombatWitnessDiscoverySource::RestoredExactActions,
        };
        if !self
            .potion_contract
            .within_resource_limits(&self.start, &witness)
        {
            return Err(format!(
                "oracle combat witness violates potion contract: uses {} potion(s), limit {:?}, allowed slots {:?}",
                combat_witness_potion_expenditures(&self.start, &witness),
                self.potion_contract.max_potions_used,
                self.potion_contract.allowed_potion_slots,
            ));
        }
        self.members
            .local_search
            .restore_verified_witness(witness)?;
        self.evidence.witness_source = CombatAutomationTrajectorySource::OracleExactActions;
        Ok(())
    }

    pub(super) fn generation_work(&self) -> u64 {
        self.evidence
            .prior_generation_work
            .saturating_add(self.current_generation_work())
    }

    fn current_generation_work(&self) -> u64 {
        (self.current_local_search_work() as u64).saturating_add(
            self.members
                .discrepancy_search
                .counters()
                .applied_action_transitions as u64,
        )
    }

    fn current_local_search_work(&self) -> usize {
        let local = self.members.local_search.counters();
        local
            .generation_work
            .saturating_add(self.evidence.plan_prefix_proposed_actions)
    }

    fn best_witness(&self) -> Option<&OracleCombatWitness> {
        self.members
            .local_search
            .witness_frontier()
            .iter()
            .chain(
                [
                    self.incumbents.discrepancy_witness.as_ref(),
                    self.incumbents.protected_potion_free_incumbent.as_ref(),
                ]
                .into_iter()
                .flatten(),
            )
            .filter(|witness| {
                self.potion_contract
                    .within_resource_limits(&self.start, witness)
            })
            .chain(self.incumbents.prior_stage_incumbent.as_ref())
            .filter(|witness| {
                self.potion_contract.allow_potion_discard
                    || !combat_witness_uses_potion_discard(witness)
            })
            .reduce(|best, candidate| {
                if combat_witness_better_with_potion_quality_gate(
                    &self.start,
                    self.satisfaction,
                    self.potion_contract.potion_spend_requires_satisfaction,
                    candidate,
                    best,
                ) {
                    candidate
                } else {
                    best
                }
            })
    }

    fn best_local_witness(&self) -> Option<&OracleCombatWitness> {
        self.members
            .local_search
            .witness_frontier()
            .iter()
            .reduce(|best, candidate| {
                if combat_witness_better_with_potion_quality_gate(
                    &self.start,
                    self.satisfaction,
                    self.potion_contract.potion_spend_requires_satisfaction,
                    candidate,
                    best,
                ) {
                    candidate
                } else {
                    best
                }
            })
    }

    pub(super) fn quantum_count(&self) -> usize {
        self.evidence.quantum_count
    }

    pub(super) fn remaining_generation_work(&self) -> usize {
        self.budget.remaining_generation_work
    }

    pub(super) fn current_search_generation_work(&self) -> u64 {
        (self.current_local_search_work() as u64).saturating_add(
            self.members
                .discrepancy_search
                .counters()
                .applied_action_transitions as u64,
        )
    }

    pub(super) fn remaining_wall_ms(&self) -> Option<u64> {
        self.budget.remaining_wall_ms()
    }

    pub(super) fn max_potions_used(&self) -> Option<u32> {
        self.potion_contract.max_potions_used
    }

    pub(super) fn allowed_potion_slots(&self) -> Option<u64> {
        self.potion_contract.allowed_potion_slots
    }

    pub(super) fn restart_count(&self) -> usize {
        self.evidence.restart_count
    }

    pub(super) fn progress(&self) -> OracleResidentCombatWitnessEvidenceV1 {
        let local_counters = self.members.local_search.counters();
        let discrepancy_counters = self.members.discrepancy_search.counters();
        let local_progress = self.members.local_search.progress_snapshot();
        let search_progress = &local_progress;
        let initial_hp = self.start.combat.entities.player.current_hp;
        let local_candidate = self.best_local_witness();
        let incumbent = self.best_witness();
        let incumbent_final_hp =
            incumbent.map(|witness| witness.final_position.combat.entities.player.current_hp);
        let local_generation_work = self.current_local_search_work() as u64;
        let discrepancy_generation_work = discrepancy_counters.applied_action_transitions as u64;
        let current_generation_work = self.current_search_generation_work();
        let local_retained_state_work = self.members.local_search.retained_state_work();
        let discrepancy_retained_state_work = self.members.discrepancy_search.retained_state_work();
        OracleResidentCombatWitnessEvidenceV1 {
            witness_engine: CombatWitnessEngineV1::TurnGraphPortfolioV1,
            root_exact_state_hash: crate::ai::combat_state_key::combat_exact_state_hash_v2(
                &self.start.engine,
                &self.start.combat,
            ),
            guide_service_bias: self.guide_service_bias,
            historical_generation_work: self.evidence.prior_generation_work,
            current_search_generation_work: current_generation_work,
            generation_work: self
                .evidence
                .prior_generation_work
                .saturating_add(current_generation_work),
            local_generation_work,
            discrepancy_generation_work,
            engine_steps: local_counters
                .engine_steps
                .saturating_add(discrepancy_counters.engine_steps),
            exact_states: local_counters
                .exact_nodes
                .saturating_add(discrepancy_counters.exact_states),
            local_exact_states: local_counters.exact_nodes,
            discrepancy_exact_states: discrepancy_counters.exact_states,
            applied_action_transitions: local_counters
                .applied_action_transitions
                .saturating_add(discrepancy_counters.applied_action_transitions),
            unique_successor_states: local_counters.unique_successor_states,
            duplicate_exact_successors: local_counters
                .duplicate_exact_successors
                .saturating_add(discrepancy_counters.duplicate_or_dominated_states),
            completed_turn_options: local_counters
                .completed_turn_options
                .saturating_add(discrepancy_counters.turn_macro_options_generated),
            public_plan_turn_proposal_enabled: self
                .members
                .local_search
                .public_plan_turn_proposal_enabled(),
            public_plan_turn_proposal_attempts: local_counters.public_plan_proposal_attempts,
            public_plan_turn_proposal_applied_transitions: local_counters
                .public_plan_proposal_applied_transitions,
            public_plan_turn_proposal_completed_options: local_counters
                .public_plan_proposal_completed_options,
            public_plan_turn_proposal_retained_options: local_counters
                .public_plan_proposal_retained_options,
            public_plan_turn_proposal_grounding_failures: local_counters
                .public_plan_proposal_grounding_failures,
            retained_state_work: local_retained_state_work
                .saturating_add(discrepancy_retained_state_work),
            local_retained_state_work,
            discrepancy_retained_state_work,
            queued_anchor_entries: local_progress
                .queued_anchor_entries
                .saturating_add(self.members.discrepancy_search.frontier_entries()),
            queued_guided_entries: local_progress.queued_guided_entries.clone(),
            root_state: search_progress.root_state.clone(),
            max_player_turn: local_progress.max_player_turn,
            deepest_survival_state: search_progress.deepest_survival_state.clone(),
            deepest_progress_state: search_progress.deepest_progress_state.clone(),
            deepest_survival_actions: search_progress.deepest_survival_actions.clone(),
            deepest_progress_actions: search_progress.deepest_progress_actions.clone(),
            recent_turn_survival_envelope: search_progress.recent_turn_survival_envelope.clone(),
            max_path_atomic_depth: local_progress.max_path_atomic_depth,
            max_completed_turn_options_at_state: local_progress.max_completed_turn_options_at_state,
            generation_gap_count: local_progress
                .generation_gap_count
                .saturating_add(discrepancy_counters.transition_step_limit_gaps),
            pending_witness_replay: local_progress.pending_witness_replay,
            plan_prefix_proposals: self.evidence.plan_prefix_proposals,
            plan_prefix_proposed_turns: self.evidence.plan_prefix_proposed_turns,
            plan_prefix_proposed_actions: self.evidence.plan_prefix_proposed_actions,
            plan_prefix_proposal_rejections: self.evidence.plan_prefix_proposal_rejections,
            local_candidate_final_hp: local_candidate
                .map(|witness| witness.final_position.combat.entities.player.current_hp),
            local_candidate_action_count: local_candidate.map(|witness| witness.actions.len()),
            local_candidate_potions_used: local_candidate
                .map(|witness| combat_witness_potion_expenditures(&self.start, witness)),
            local_candidate_potion_slots: local_candidate
                .map(|witness| combat_witness_potion_expenditure_slots(&self.start, witness)),
            local_candidate_satisfies_satisfaction: local_candidate
                .map(|witness| combat_witness_satisfies(self.satisfaction, &self.start, witness)),
            local_candidate_disposition: combat_local_candidate_disposition(
                &self.start,
                self.satisfaction,
                &self.potion_contract,
                local_candidate,
                incumbent,
            ),
            incumbent_discovery_source: incumbent.map(|witness| witness.discovery_source),
            incumbent_final_hp,
            incumbent_hp_loss: incumbent_final_hp
                .map(|final_hp| initial_hp.saturating_sub(final_hp).max(0)),
            incumbent_action_count: incumbent.map(|witness| witness.actions.len()),
            incumbent_potions_used: incumbent
                .map(|witness| combat_witness_potion_expenditures(&self.start, witness)),
            incumbent_potion_slots: incumbent
                .map(|witness| combat_witness_potion_expenditure_slots(&self.start, witness)),
            incumbent_satisfies_satisfaction: incumbent
                .map(|witness| combat_witness_satisfies(self.satisfaction, &self.start, witness)),
            incumbent_ends_quality_refinement: incumbent.map(|witness| {
                combat_witness_ends_quality_refinement(
                    &self.start,
                    self.satisfaction,
                    self.potion_contract.potion_spend_requires_satisfaction,
                    witness,
                )
            }),
            potion_spend_requires_satisfaction: self
                .potion_contract
                .potion_spend_requires_satisfaction,
            incumbent_revision: self.evidence.incumbent_revision,
            quanta_since_incumbent_improvement: self.evidence.quanta_since_incumbent_improvement,
            last_quantum_generation_work: self.evidence.last_quantum_generation_work,
            last_quantum_engine_steps: self.evidence.last_quantum_engine_steps,
            last_status: overall_status_label(
                self.best_witness().is_some(),
                self.members.local_status.as_ref(),
                self.members.discrepancy_status.as_ref(),
                self.evidence.last_status.as_ref(),
            )
            .map(str::to_owned),
        }
    }

    pub(super) fn finish_and_apply(
        &self,
        session: &mut RunControlSession,
    ) -> Result<RunProgressOutcome, String> {
        if session.current_active_combat_position()? != self.start {
            return Err("oracle combat parent changed before search commit".to_string());
        }
        if let Some(witness) = self.best_witness() {
            let source = match witness.discovery_source {
                OracleCombatWitnessDiscoverySource::PlannerSearch => {
                    CombatAutomationTrajectorySource::TurnGraphPortfolioV1
                }
                OracleCombatWitnessDiscoverySource::PolicyDiscrepancySearch => {
                    CombatAutomationTrajectorySource::TurnGraphPortfolioV1
                }
                OracleCombatWitnessDiscoverySource::RestoredExactActions => {
                    CombatAutomationTrajectorySource::OracleExactActions
                }
                OracleCombatWitnessDiscoverySource::LegacyUnattributed => {
                    self.evidence.witness_source
                }
            };
            return apply_oracle_combat_witness(session, &self.start, witness, source);
        }
        let default_status = PortfolioStatusV1::Local(LocalTurnGraphWitnessStatus::Partial(
            sts_combat_planner::LocalTurnGraphWitnessInterruption::SelectionBudget,
        ));
        let status = self
            .evidence
            .last_status
            .as_ref()
            .unwrap_or(&default_status);
        Ok(RunProgressOutcome::message(format!(
            "Combat-search portfolio did not modify state. status={status:?} generation_work={} local_work={} discrepancy_work={} exact_states={} retained_work={}",
            self.evidence.prior_generation_work
                .saturating_add(self.current_generation_work()),
            self.members.local_search.counters().generation_work,
            self.members.discrepancy_search
                .counters()
                .applied_action_transitions,
            self.members.local_search
                .counters()
                .exact_nodes
                .saturating_add(self.members.discrepancy_search.counters().exact_states),
            self.members.local_search
                .retained_state_work()
                .saturating_add(self.members.discrepancy_search.retained_state_work()),
        ))
        .with_oracle_combat_witness_failure(
            OracleCombatWitnessFailureV1::NoVerifiedCompleteWin,
        ))
    }
}

#[derive(Clone, Copy)]
struct CombatWitnessQualityV1 {
    final_hp: i32,
    persistent: CombatPersistentOutcomeV1,
    potions_used: u32,
    action_count: usize,
    negative_log_policy: f64,
}

fn combat_witness_quality(
    start: &crate::sim::combat::CombatPosition,
    witness: &OracleCombatWitness,
) -> CombatWitnessQualityV1 {
    let final_hp = witness.final_position.combat.entities.player.current_hp;
    CombatWitnessQualityV1 {
        final_hp,
        persistent: CombatPersistentOutcomeV1::from_combat(&witness.final_position.combat),
        potions_used: combat_witness_potion_expenditures(start, witness),
        action_count: witness.actions.len(),
        negative_log_policy: witness.negative_log_policy,
    }
}

fn combat_witness_resources_dominate_or_equal(
    left: CombatWitnessQualityV1,
    right: CombatWitnessQualityV1,
) -> bool {
    left.final_hp >= right.final_hp
        && left.persistent.dominates_or_equals(right.persistent)
        && left.potions_used <= right.potions_used
}

fn combat_witness_resources_strictly_dominate(
    left: CombatWitnessQualityV1,
    right: CombatWitnessQualityV1,
) -> bool {
    combat_witness_resources_dominate_or_equal(left, right)
        && (left.final_hp != right.final_hp
            || left.persistent != right.persistent
            || left.potions_used != right.potions_used)
}

fn combat_witness_quality_better(
    left: CombatWitnessQualityV1,
    right: CombatWitnessQualityV1,
) -> bool {
    let left_dominates = combat_witness_resources_strictly_dominate(left, right);
    let right_dominates = combat_witness_resources_strictly_dominate(right, left);
    if left_dominates != right_dominates {
        return left_dominates;
    }

    // An HP/persistent-resource trade is deliberately unresolved here. The
    // exact frontier retains both witnesses for a future run-level owner; the
    // production fallback preserves survival instead of converting card
    // growth or gold into hidden HP units.
    let survival_order = left
        .final_hp
        .cmp(&right.final_hp)
        .then_with(|| right.potions_used.cmp(&left.potions_used));
    if survival_order != std::cmp::Ordering::Equal {
        return survival_order == std::cmp::Ordering::Greater;
    }
    if left.persistent != right.persistent {
        return false;
    }

    right.action_count.cmp(&left.action_count).then_with(|| {
        right
            .negative_log_policy
            .total_cmp(&left.negative_log_policy)
    }) == std::cmp::Ordering::Greater
}

fn combat_witness_acceptance_improved(
    before: Option<CombatWitnessQualityV1>,
    after: Option<CombatWitnessQualityV1>,
) -> bool {
    match (before, after) {
        (None, Some(_)) => true,
        (Some(before), Some(after)) => combat_witness_resources_strictly_dominate(after, before),
        _ => false,
    }
}

fn planner_satisfaction(
    satisfaction: PortfolioWitnessSatisfactionV1,
) -> OracleCombatWitnessSatisfaction {
    match satisfaction {
        PortfolioWitnessSatisfactionV1::FirstWitness => {
            OracleCombatWitnessSatisfaction::FirstWitness
        }
        PortfolioWitnessSatisfactionV1::HpLossAtMost(limit) => {
            OracleCombatWitnessSatisfaction::HpLossAtMost(limit)
        }
        PortfolioWitnessSatisfactionV1::FirstWitnessWithoutNewExternalBurden => {
            OracleCombatWitnessSatisfaction::FirstWitnessWithoutNewExternalBurden
        }
        PortfolioWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(limit) => {
            OracleCombatWitnessSatisfaction::HpLossAtMostWithoutNewExternalBurden(limit)
        }
        PortfolioWitnessSatisfactionV1::PotionFreeHpLossAtMostWithoutNewExternalBurden(limit) => {
            OracleCombatWitnessSatisfaction::PotionFreeHpLossAtMostWithoutNewExternalBurden(limit)
        }
        PortfolioWitnessSatisfactionV1::MaterializedPersistentPayoffGain
        | PortfolioWitnessSatisfactionV1::BudgetOrExhaustion => {
            OracleCombatWitnessSatisfaction::BudgetOrExhaustion
        }
    }
}

fn combat_witness_better(
    start: &crate::sim::combat::CombatPosition,
    left: &OracleCombatWitness,
    right: &OracleCombatWitness,
) -> bool {
    combat_witness_quality_better(
        combat_witness_quality(start, left),
        combat_witness_quality(start, right),
    )
}

fn standard_witness_ends_stage(
    stop_on_first_satisfying_witness: bool,
    acceptance_improved: bool,
    satisfaction: PortfolioWitnessSatisfactionV1,
    start: &crate::sim::combat::CombatPosition,
    witness: Option<&OracleCombatWitness>,
) -> bool {
    stop_on_first_satisfying_witness
        && acceptance_improved
        && witness.is_some_and(|witness| combat_witness_satisfies(satisfaction, start, witness))
}

fn inherited_satisfying_incumbent_challenged(
    stop_on_first_satisfying_witness: bool,
    satisfaction: PortfolioWitnessSatisfactionV1,
    start: &crate::sim::combat::CombatPosition,
    inherited: Option<&OracleCombatWitness>,
    local_generation_work: usize,
    challenge_work: usize,
) -> bool {
    stop_on_first_satisfying_witness
        && local_generation_work >= challenge_work
        && inherited.is_some_and(|witness| combat_witness_satisfies(satisfaction, start, witness))
}

fn combat_witness_better_with_potion_quality_gate(
    start: &crate::sim::combat::CombatPosition,
    satisfaction: PortfolioWitnessSatisfactionV1,
    potion_spend_requires_satisfaction: bool,
    left: &OracleCombatWitness,
    right: &OracleCombatWitness,
) -> bool {
    if potion_spend_requires_satisfaction {
        let left_potions = combat_witness_potion_expenditures(start, left);
        let right_potions = combat_witness_potion_expenditures(start, right);
        let left_satisfies = combat_witness_satisfies_strategic_quality(satisfaction, start, left);
        let right_satisfies =
            combat_witness_satisfies_strategic_quality(satisfaction, start, right);
        if left_satisfies != right_satisfies {
            return left_satisfies;
        }
        if !left_satisfies && left_potions != right_potions {
            return left_potions < right_potions;
        }
    }
    combat_witness_better(start, left, right)
}

fn combat_local_candidate_disposition(
    start: &crate::sim::combat::CombatPosition,
    satisfaction: PortfolioWitnessSatisfactionV1,
    potion_contract: &WitnessPotionContractV1,
    local_candidate: Option<&OracleCombatWitness>,
    incumbent: Option<&OracleCombatWitness>,
) -> Option<OracleCombatWitnessCandidateDispositionV1> {
    let local_candidate = local_candidate?;
    if incumbent.is_some_and(|incumbent| std::ptr::eq(local_candidate, incumbent)) {
        return Some(OracleCombatWitnessCandidateDispositionV1::SelectedIncumbent);
    }
    if !potion_contract.accepts_witness(start, local_candidate) {
        return Some(OracleCombatWitnessCandidateDispositionV1::RejectedOutsidePotionContract);
    }
    if potion_contract.potion_spend_requires_satisfaction
        && combat_witness_potion_expenditures(start, local_candidate) > 0
        && !combat_witness_satisfies(satisfaction, start, local_candidate)
    {
        return Some(
            OracleCombatWitnessCandidateDispositionV1::RejectedPotionSpendMissesSatisfaction,
        );
    }
    if potion_contract.potion_spend_requires_satisfaction
        && combat_witness_potion_expenditures(start, local_candidate) > 0
        && crate::ai::combat_persistent_outcome_v1::unrecovered_stolen_gold(
            &local_candidate.final_position.combat,
        ) > 0
    {
        return Some(
            OracleCombatWitnessCandidateDispositionV1::RejectedPotionSpendLeavesUnrecoveredTheft,
        );
    }
    Some(OracleCombatWitnessCandidateDispositionV1::RejectedByPortfolioComparison)
}

fn combat_witness_satisfies_strategic_quality(
    satisfaction: PortfolioWitnessSatisfactionV1,
    start: &crate::sim::combat::CombatPosition,
    witness: &OracleCombatWitness,
) -> bool {
    let satisfaction = match satisfaction {
        PortfolioWitnessSatisfactionV1::FirstWitness => {
            CombatWitnessSatisfactionV1::FirstCompleteWin
        }
        PortfolioWitnessSatisfactionV1::HpLossAtMost(limit) => {
            CombatWitnessSatisfactionV1::HpLossAtMost(limit)
        }
        PortfolioWitnessSatisfactionV1::MaterializedPersistentPayoffGain => {
            CombatWitnessSatisfactionV1::MaterializedPersistentPayoffGain
        }
        PortfolioWitnessSatisfactionV1::FirstWitnessWithoutNewExternalBurden => {
            CombatWitnessSatisfactionV1::FirstCompleteWinWithoutNewExternalBurden
        }
        PortfolioWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(limit) => {
            CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(limit)
        }
        PortfolioWitnessSatisfactionV1::PotionFreeHpLossAtMostWithoutNewExternalBurden(limit) => {
            CombatWitnessSatisfactionV1::PotionFreeHpLossAtMostWithoutNewExternalBurden(limit)
        }
        PortfolioWitnessSatisfactionV1::BudgetOrExhaustion => {
            CombatWitnessSatisfactionV1::BudgetOrExhaustion
        }
    };
    combat_witness_satisfies_materialized_quality_v1(
        satisfaction,
        start.combat.entities.player.current_hp,
        crate::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1::from_combat(
            &start.combat,
        ),
        witness.final_position.combat.entities.player.current_hp,
        crate::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1::from_combat(
            &witness.final_position.combat,
        ),
        combat_witness_potion_expenditures(start, witness),
    )
}

fn combat_witness_ends_quality_refinement(
    start: &crate::sim::combat::CombatPosition,
    satisfaction: PortfolioWitnessSatisfactionV1,
    potion_spend_requires_satisfaction: bool,
    witness: &OracleCombatWitness,
) -> bool {
    combat_witness_satisfies_strategic_quality(satisfaction, start, witness)
        && (!potion_spend_requires_satisfaction
            || combat_witness_potion_expenditures(start, witness) == 0)
}

fn combat_witness_potion_expenditures(
    start: &crate::sim::combat::CombatPosition,
    witness: &OracleCombatWitness,
) -> u32 {
    let explicit_expenditures = witness
        .actions
        .iter()
        .filter(|action| {
            matches!(
                action.input,
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            )
        })
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let remaining_uuids = witness
        .final_position
        .combat
        .entities
        .potions
        .iter()
        .flatten()
        .map(|potion| potion.uuid)
        .collect::<std::collections::BTreeSet<_>>();
    let missing_starting_resources = start
        .combat
        .entities
        .potions
        .iter()
        .flatten()
        .filter(|potion| !remaining_uuids.contains(&potion.uuid))
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    explicit_expenditures.max(missing_starting_resources)
}

fn combat_witness_uses_potion_discard(witness: &OracleCombatWitness) -> bool {
    witness
        .actions
        .iter()
        .any(|action| matches!(action.input, ClientInput::DiscardPotion(_)))
}

fn combat_witness_within_potion_contract(
    start: &crate::sim::combat::CombatPosition,
    witness: &OracleCombatWitness,
    max_potions_used: Option<u32>,
    allowed_potion_slots: Option<u64>,
) -> bool {
    max_potions_used.is_none_or(|limit| combat_witness_potion_expenditures(start, witness) <= limit)
        && witness.actions.iter().all(|action| {
            let slot = match action.input {
                ClientInput::UsePotion { potion_index, .. } => Some(potion_index),
                ClientInput::DiscardPotion(slot) => Some(slot),
                _ => None,
            };
            slot.is_none_or(|slot| {
                allowed_potion_slots.is_none_or(|allowed_slots| {
                    u32::try_from(slot)
                        .ok()
                        .and_then(|slot| 1_u64.checked_shl(slot))
                        .is_some_and(|slot_mask| allowed_slots & slot_mask != 0)
                })
            })
        })
        && allowed_potion_slots.is_none_or(|allowed_slots| {
            starting_potion_expenditure_slots(start, witness) & !allowed_slots == 0
        })
}

fn starting_potion_expenditure_slots(
    start: &crate::sim::combat::CombatPosition,
    witness: &OracleCombatWitness,
) -> u64 {
    let remaining_uuids = witness
        .final_position
        .combat
        .entities
        .potions
        .iter()
        .flatten()
        .map(|potion| potion.uuid)
        .collect::<std::collections::BTreeSet<_>>();
    start
        .combat
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref()?;
            if remaining_uuids.contains(&potion.uuid) {
                return None;
            }
            u32::try_from(slot)
                .ok()
                .and_then(|slot| 1_u64.checked_shl(slot))
        })
        .fold(0_u64, |mask, slot| mask | slot)
}

fn combat_witness_potion_expenditure_slots(
    start: &crate::sim::combat::CombatPosition,
    witness: &OracleCombatWitness,
) -> u64 {
    witness
        .actions
        .iter()
        .filter_map(|action| match action.input {
            ClientInput::UsePotion { potion_index, .. } => Some(potion_index),
            ClientInput::DiscardPotion(slot) => Some(slot),
            _ => None,
        })
        .filter_map(|slot| {
            u32::try_from(slot)
                .ok()
                .and_then(|slot| 1_u64.checked_shl(slot))
        })
        .fold(
            starting_potion_expenditure_slots(start, witness),
            |mask, slot| mask | slot,
        )
}

fn combat_witness_satisfies(
    satisfaction: PortfolioWitnessSatisfactionV1,
    start: &crate::sim::combat::CombatPosition,
    witness: &OracleCombatWitness,
) -> bool {
    match satisfaction {
        PortfolioWitnessSatisfactionV1::FirstWitness => true,
        PortfolioWitnessSatisfactionV1::HpLossAtMost(limit) => {
            let initial_hp = start.combat.entities.player.current_hp;
            let final_hp = witness.final_position.combat.entities.player.current_hp;
            initial_hp.saturating_sub(final_hp).max(0) as u32 <= limit
        }
        PortfolioWitnessSatisfactionV1::MaterializedPersistentPayoffGain => {
            CombatPersistentOutcomeV1::from_combat(&witness.final_position.combat)
                .strictly_dominates(CombatPersistentOutcomeV1::from_combat(&start.combat))
        }
        PortfolioWitnessSatisfactionV1::FirstWitnessWithoutNewExternalBurden
        | PortfolioWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(_)
        | PortfolioWitnessSatisfactionV1::PotionFreeHpLossAtMostWithoutNewExternalBurden(_) => {
            combat_witness_satisfies_strategic_quality(satisfaction, start, witness)
        }
        PortfolioWitnessSatisfactionV1::BudgetOrExhaustion => false,
    }
}

fn wall_allowance_exhausted(remaining: Option<Duration>) -> bool {
    remaining.is_some_and(|duration| duration < MIN_USABLE_WALL_ALLOWANCE)
}

fn select_productive_portfolio_member(
    productive_member: Option<PortfolioMemberV1>,
    next: PortfolioMemberV1,
    local_complete: bool,
    discrepancy_complete: bool,
) -> Option<PortfolioMemberV1> {
    match productive_member {
        Some(PortfolioMemberV1::LocalTurnGraph) if !local_complete => {
            Some(PortfolioMemberV1::LocalTurnGraph)
        }
        Some(PortfolioMemberV1::PolicyDiscrepancy) if !discrepancy_complete => {
            Some(PortfolioMemberV1::PolicyDiscrepancy)
        }
        _ => select_portfolio_member(next, local_complete, discrepancy_complete),
    }
}

fn select_portfolio_member(
    next: PortfolioMemberV1,
    local_complete: bool,
    discrepancy_complete: bool,
) -> Option<PortfolioMemberV1> {
    match (next, local_complete, discrepancy_complete) {
        (_, true, true) => None,
        (PortfolioMemberV1::LocalTurnGraph, false, _)
        | (PortfolioMemberV1::PolicyDiscrepancy, false, true) => {
            Some(PortfolioMemberV1::LocalTurnGraph)
        }
        (PortfolioMemberV1::PolicyDiscrepancy, _, false)
        | (PortfolioMemberV1::LocalTurnGraph, true, false) => {
            Some(PortfolioMemberV1::PolicyDiscrepancy)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::combat_witness_contract::CombatWitnessPotionPolicyV1;

    fn protected_single_slot_potion_contract() -> WitnessPotionContractV1 {
        WitnessPotionContractV1 {
            max_potions_used: Some(1),
            allowed_potion_slots: Some(1),
            allow_potion_discard: false,
            potion_spend_requires_satisfaction: true,
        }
    }

    fn hallway_combat_session() -> RunControlSession {
        let mut combat = crate::test_support::blank_test_combat();
        let mut jaw_worm =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
        let plan = crate::content::monsters::roll_monster_turn_plan(
            &mut combat.rng.ai_rng,
            &jaw_worm,
            combat.meta.ascension_level,
            99,
            std::slice::from_ref(&jaw_worm),
            &[],
        );
        jaw_worm.set_planned_move_id(plan.move_id);
        jaw_worm.set_planned_steps(plan.steps);
        jaw_worm.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters = vec![jaw_worm];
        let mut session =
            RunControlSession::new(crate::eval::run_control::RunControlConfig::default());
        session.active_combat = Some(crate::state::core::ActiveCombat::new(
            crate::state::core::EngineState::CombatPlayerTurn,
            combat,
            crate::state::core::CombatContext::Room(crate::state::core::RoomCombatContext {
                room_type: crate::state::map::node::RoomType::MonsterRoom,
            }),
        ));
        session
    }

    fn darkling_combat_session() -> RunControlSession {
        let mut session = hallway_combat_session();
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        combat.entities.monsters = (0..3)
            .map(|index| {
                let mut monster =
                    crate::test_support::test_monster(crate::content::monsters::EnemyId::Darkling);
                monster.id = index + 1;
                monster
            })
            .collect();
        session
    }

    fn boss_combat_session() -> RunControlSession {
        let mut session = hallway_combat_session();
        let active = session.active_combat.as_mut().expect("active combat");
        active.combat_state.meta.is_boss_fight = true;
        active.context =
            crate::state::core::CombatContext::Room(crate::state::core::RoomCombatContext {
                room_type: crate::state::map::node::RoomType::MonsterRoomBoss,
            });
        session
    }

    fn bronze_automaton_combat_session() -> RunControlSession {
        let mut session = boss_combat_session();
        let active = session.active_combat.as_mut().expect("active combat");
        let combat = &mut active.combat_state;
        let mut automaton =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::BronzeAutomaton);
        automaton.id = 10;
        let plan = crate::content::monsters::roll_monster_turn_plan(
            &mut combat.rng.ai_rng,
            &automaton,
            combat.meta.ascension_level,
            99,
            std::slice::from_ref(&automaton),
            &[],
        );
        automaton.set_planned_move_id(plan.move_id);
        automaton.set_planned_steps(plan.steps);
        automaton.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters = vec![automaton];
        crate::content::powers::store::set_powers_for(
            combat,
            10,
            vec![crate::runtime::combat::Power {
                power_type: crate::content::powers::PowerId::Artifact,
                instance_id: None,
                amount: 3,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat.turn.energy = 3;
        combat.zones.hand = vec![
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Disarm,
                1,
            ),
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::ThunderClap,
                2,
            ),
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Strike,
                3,
            ),
        ];
        session
    }

    fn one_strike_win_session() -> RunControlSession {
        let mut session = hallway_combat_session();
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        combat.turn.energy = 1;
        combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            1,
        )];
        combat.entities.monsters[0].current_hp = 1;
        combat.entities.monsters[0].max_hp = 1;
        session
    }

    fn single_potion_slot_options(slot_mask: u64) -> OracleCombatWitnessOptionsV1 {
        OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(16),
            max_potions_used: Some(1),
            allowed_potion_slots: Some(slot_mask),
            satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
            ..OracleCombatWitnessOptionsV1::default()
        }
    }

    fn explosive_stage_one_work() -> (RunControlSession, OracleRunCombatWitnessWorkV1) {
        let mut session = one_strike_win_session();
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        combat.entities.monsters[0].current_hp = 10;
        combat.entities.monsters[0].max_hp = 10;
        combat.entities.potions = vec![
            Some(crate::content::potions::Potion::new(
                crate::content::potions::PotionId::ExplosivePotion,
                70,
            )),
            Some(crate::content::potions::Potion::new(
                crate::content::potions::PotionId::BlockPotion,
                71,
            )),
        ];
        let mut work = OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            &session,
            single_potion_slot_options(0b01),
            None,
        )
        .expect("slot-zero exact witness work");
        work.verify_and_restore_action_witness(&[ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        }])
        .expect("Explosive Potion should provide an exact slot-zero victory");
        (session, work)
    }

    fn synthetic_witness(
        start: &crate::sim::combat::CombatPosition,
        final_hp: i32,
        spend_potion: bool,
    ) -> OracleCombatWitness {
        let mut final_position = start.clone();
        // These quality/frontier fixtures represent settled outcomes, not a
        // live turn with dead or escaped monsters awaiting aftermath.
        final_position.engine =
            crate::state::core::EngineState::GameOver(crate::state::core::RunResult::Victory);
        final_position.combat.entities.player.current_hp = final_hp;
        let actions = if spend_potion {
            final_position.combat.entities.potions[0] = None;
            vec![TurnOptionAction {
                input: ClientInput::UsePotion {
                    potion_index: 0,
                    target: None,
                },
                expected_successor_hash: "synthetic".into(),
                engine_steps: 0,
            }]
        } else {
            Vec::new()
        };
        OracleCombatWitness {
            actions,
            final_position,
            negative_log_policy: 0.0,
            replay_engine_steps: 0,
            discovery_source: OracleCombatWitnessDiscoverySource::RestoredExactActions,
        }
    }

    #[test]
    fn quality_gate_protects_no_potion_incumbent_from_marginal_spend() {
        let mut session = hallway_combat_session();
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .potions = vec![Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::ColorlessPotion,
            7,
        ))];
        let start = session
            .current_active_combat_position()
            .expect("exact potion rescue root");
        let baseline = synthetic_witness(&start, 50, false);
        let marginal_spend = synthetic_witness(&start, 55, true);
        let quality_spend = synthetic_witness(&start, 65, true);
        let quality_clean = synthetic_witness(&start, 65, false);
        let satisfaction = PortfolioWitnessSatisfactionV1::HpLossAtMost(20);

        assert!(
            combat_witness_better(&start, &marginal_spend, &baseline),
            "raw final-HP comparison demonstrates the regression guard's purpose"
        );
        assert!(!combat_witness_better_with_potion_quality_gate(
            &start,
            satisfaction,
            true,
            &marginal_spend,
            &baseline,
        ));
        assert_eq!(
            combat_local_candidate_disposition(
                &start,
                satisfaction,
                &protected_single_slot_potion_contract(),
                Some(&marginal_spend),
                Some(&baseline),
            ),
            Some(OracleCombatWitnessCandidateDispositionV1::RejectedPotionSpendMissesSatisfaction)
        );
        assert_eq!(
            combat_local_candidate_disposition(
                &start,
                satisfaction,
                &protected_single_slot_potion_contract(),
                Some(&baseline),
                Some(&baseline),
            ),
            Some(OracleCombatWitnessCandidateDispositionV1::SelectedIncumbent)
        );
        assert!(combat_witness_better_with_potion_quality_gate(
            &start,
            satisfaction,
            true,
            &baseline,
            &marginal_spend,
        ));
        assert!(combat_witness_better_with_potion_quality_gate(
            &start,
            satisfaction,
            true,
            &quality_spend,
            &baseline,
        ));
        assert!(!combat_witness_better_with_potion_quality_gate(
            &start,
            satisfaction,
            true,
            &baseline,
            &quality_spend,
        ));
        assert!(!combat_witness_ends_quality_refinement(
            &start,
            satisfaction,
            true,
            &quality_spend,
        ));
        assert!(combat_witness_ends_quality_refinement(
            &start,
            satisfaction,
            true,
            &quality_clean,
        ));
        assert!(combat_witness_ends_quality_refinement(
            &start,
            satisfaction,
            false,
            &quality_spend,
        ));
    }

    #[test]
    fn unrecovered_thief_gold_keeps_quality_refinement_open() {
        let session = hallway_combat_session();
        let start = session
            .current_active_combat_position()
            .expect("exact hallway combat root");
        let clean = synthetic_witness(&start, start.combat.entities.player.current_hp, false);
        let mut escaped_with_gold = clean.clone();
        let mut thief =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::Looter);
        thief.is_escaped = true;
        thief.thief.stolen_gold = 30;
        escaped_with_gold.final_position.combat.entities.monsters = vec![thief];

        assert!(combat_witness_ends_quality_refinement(
            &start,
            PortfolioWitnessSatisfactionV1::HpLossAtMost(0),
            false,
            &clean,
        ));
        assert!(!combat_witness_ends_quality_refinement(
            &start,
            PortfolioWitnessSatisfactionV1::HpLossAtMost(0),
            false,
            &escaped_with_gold,
        ));
    }

    #[test]
    fn strategic_potion_quality_prefers_threshold_compliant_full_theft_recovery() {
        let mut session = hallway_combat_session();
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .potions = vec![Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::FirePotion,
            7,
        ))];
        let start = session
            .current_active_combat_position()
            .expect("exact potion combat root");
        let initial_hp = start.combat.entities.player.current_hp;
        let mut partial_recovery = synthetic_witness(&start, initial_hp - 3, true);
        partial_recovery
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        partial_recovery
            .final_position
            .combat
            .runtime
            .pending_rewards
            .push(crate::state::rewards::RewardItem::StolenGold { amount: 45 });
        let mut escaped =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::Looter);
        escaped.is_escaped = true;
        escaped.thief.stolen_gold = 30;
        partial_recovery.final_position.combat.entities.monsters = vec![escaped];

        let mut full_recovery = synthetic_witness(&start, initial_hp - 14, true);
        full_recovery
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        full_recovery
            .final_position
            .combat
            .runtime
            .pending_rewards
            .push(crate::state::rewards::RewardItem::StolenGold { amount: 75 });
        full_recovery
            .final_position
            .combat
            .entities
            .monsters
            .clear();
        let satisfaction = PortfolioWitnessSatisfactionV1::HpLossAtMost(17);

        assert!(combat_witness_better(
            &start,
            &partial_recovery,
            &full_recovery,
        ));
        assert!(combat_witness_better_with_potion_quality_gate(
            &start,
            satisfaction,
            true,
            &full_recovery,
            &partial_recovery,
        ));
        assert_eq!(
            combat_local_candidate_disposition(
                &start,
                satisfaction,
                &protected_single_slot_potion_contract(),
                Some(&partial_recovery),
                Some(&full_recovery),
            ),
            Some(
                OracleCombatWitnessCandidateDispositionV1::RejectedPotionSpendLeavesUnrecoveredTheft
            )
        );
    }

    #[test]
    fn production_hp_quality_keeps_local_search_open_for_unrecovered_theft() {
        let session = hallway_combat_session();
        let start = session
            .current_active_combat_position()
            .expect("exact hallway combat root");
        let initial_hp = start.combat.entities.player.current_hp;
        let mut work = OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(1),
                max_potions_used: Some(0),
                satisfaction: Some(CombatWitnessSatisfactionV1::HpLossAtMost(17)),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("production combat work");

        let mut escaped = synthetic_witness(&start, initial_hp, false);
        escaped
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -30;
        let mut thief =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::Looter);
        thief.is_escaped = true;
        thief.thief.stolen_gold = 30;
        escaped.final_position.combat.entities.monsters = vec![thief];
        work.members
            .local_search
            .restore_verified_witness(escaped)
            .expect("escaped thief is a mechanically terminal victory");

        let zero_work = LocalTurnGraphWitnessQuantum {
            additional_selections: 0,
            additional_generation_work: 0,
            additional_engine_steps: 0,
            deadline: None,
        };
        let partial = work
            .members
            .local_search
            .advance(zero_work, &crate::sim::combat::EngineCombatStepper);
        assert!(matches!(
            partial.status,
            LocalTurnGraphWitnessStatus::Partial(_)
        ));

        let mut recovered = synthetic_witness(&start, initial_hp - 14, false);
        recovered
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -30;
        recovered.final_position.combat.entities.monsters.clear();
        recovered
            .final_position
            .combat
            .runtime
            .pending_rewards
            .push(crate::state::rewards::RewardItem::StolenGold { amount: 30 });
        work.members
            .local_search
            .restore_verified_witness(recovered)
            .expect("fully recovered line is a mechanically terminal victory");

        assert_eq!(work.members.local_search.witness_frontier().len(), 2);
        assert_eq!(
            work.members
                .local_search
                .witness()
                .expect("local HP compatibility witness")
                .final_position
                .combat
                .entities
                .player
                .current_hp,
            initial_hp,
            "the lower-HP qualifying frontier witness must not need to replace the HP view"
        );
        let satisfied = work
            .members
            .local_search
            .advance(zero_work, &crate::sim::combat::EngineCombatStepper);
        assert_eq!(satisfied.status, LocalTurnGraphWitnessStatus::WitnessFound);
    }

    #[test]
    fn resident_frontier_boundary_retains_incomparable_action_lines() {
        let mut session = hallway_combat_session();
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .potions = vec![Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::BlockPotion,
            91,
        ))];
        let start = session
            .current_active_combat_position()
            .expect("exact potion frontier root");
        let mut work = OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(1),
                max_potions_used: Some(1),
                satisfaction: Some(CombatWitnessSatisfactionV1::BudgetOrExhaustion),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("resident frontier work");
        let mut conserving = synthetic_witness(&start, 50, false);
        let mut healthier = synthetic_witness(&start, 55, true);
        for witness in [&mut conserving, &mut healthier] {
            let monster = witness
                .final_position
                .combat
                .entities
                .monsters
                .first_mut()
                .expect("terminal monster");
            monster.current_hp = 0;
            monster.is_dying = true;
        }
        work.members
            .local_search
            .restore_verified_witness(conserving)
            .expect("conserving terminal line");
        work.members
            .local_search
            .restore_verified_witness(healthier)
            .expect("healthier potion terminal line");

        assert_eq!(work.members.local_search.witness_frontier().len(), 2);
        assert_eq!(
            work.verified_witness_frontier_inputs().len(),
            2,
            "the resident capability boundary must not collapse an HP/potion tradeoff"
        );
    }

    #[test]
    fn exact_restored_witness_cannot_bypass_zero_potion_contract() {
        let mut session = one_strike_win_session();
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        let target = combat.entities.monsters[0].id;
        combat.entities.potions = vec![Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::FirePotion,
            7,
        ))];
        let mut work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                max_potions_used: Some(0),
                satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("zero-potion portfolio should accept an active combat");

        let error = work
            .verify_and_restore_action_witness(&[ClientInput::UsePotion {
                potion_index: 0,
                target: Some(target),
            }])
            .expect_err("an exact potion win must not bypass the configured resource contract");
        assert!(error.contains("violates potion contract"));
        assert!(work.best_witness().is_none_or(|witness| {
            combat_witness_potion_expenditures(&work.start, witness) == 0
        }));
    }

    #[test]
    fn exact_restored_discard_requires_an_all_legal_potion_policy() {
        // Exercise the public exact-witness restoration boundary under both
        // policy modes; filtering only generated actions would miss restores.
        let mut session = one_strike_win_session();
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        let target = combat.entities.monsters[0].id;
        combat.entities.potions = vec![Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::EnergyPotion,
            7,
        ))];
        let actions = [
            ClientInput::DiscardPotion(0),
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
        ];
        let options = |potion_policy| OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(16),
            potion_policy: Some(potion_policy),
            max_potions_used: Some(1),
            allowed_potion_slots: Some(1),
            satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
            ..OracleCombatWitnessOptionsV1::default()
        };
        let mut semantic = OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            &session,
            options(CombatWitnessPotionPolicyV1::SemanticBudgeted),
            None,
        )
        .expect("semantic exact-witness work");

        let error = semantic
            .verify_and_restore_action_witness(&actions)
            .expect_err("semantic victory search must omit explicit discard");
        assert!(error.contains("outside an all-legal search policy"));

        let mut all_legal = OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            &session,
            options(CombatWitnessPotionPolicyV1::All),
            None,
        )
        .expect("all-legal exact-witness work");
        all_legal
            .verify_and_restore_action_witness(&actions)
            .expect("all-legal discard witness");
        assert!(all_legal.has_verified_witness());

        let mut semantic_all_options = options(CombatWitnessPotionPolicyV1::All);
        semantic_all_options.allow_potion_discard = Some(false);
        let mut semantic_all =
            OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
                &session,
                semantic_all_options,
                None,
            )
            .expect("all-use semantic exact-witness work");
        let error = semantic_all
            .verify_and_restore_action_witness(&actions)
            .expect_err("explicit production admission must reject discard under All use policy");
        assert!(error.contains("outside an all-legal search policy"));
    }

    #[test]
    fn exact_restored_witness_cannot_bypass_potion_slot_contract() {
        let mut session = one_strike_win_session();
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        let target = combat.entities.monsters[0].id;
        combat.entities.potions = vec![Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::FirePotion,
            8,
        ))];
        let mut work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                max_potions_used: Some(1),
                allowed_potion_slots: Some(1_u64 << 1),
                satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("slot-constrained portfolio should accept an active combat");

        let error = work
            .verify_and_restore_action_witness(&[ClientInput::UsePotion {
                potion_index: 0,
                target: Some(target),
            }])
            .expect_err("an exact potion win must not bypass the exact slot contract");

        assert!(error.contains("allowed slots Some(2)"), "{error}");
        assert!(work.best_witness().is_none_or(|witness| {
            witness.actions.iter().all(|action| {
                !matches!(
                    action.input,
                    ClientInput::UsePotion {
                        potion_index: 0,
                        ..
                    }
                )
            })
        }));
    }

    #[test]
    fn identity_stage_preserves_verified_prior_incumbent_across_checkpoint_restore() {
        let (session, stage_one) = explosive_stage_one_work();
        let stage_one_checkpoint = stage_one.checkpoint();
        let stage_one_incumbent = stage_one_checkpoint
            .incumbent
            .as_ref()
            .expect("slot-zero stage should checkpoint its exact victory");
        assert_eq!(
            combat_witness_potion_expenditure_slots(&stage_one.start, stage_one_incumbent),
            0b01
        );

        let stage_two_options = single_potion_slot_options(0b10);
        let stage_two = OracleRunCombatWitnessWorkV1::restart_for_higher_fidelity_with_guidance(
            &session,
            stage_two_options.clone(),
            stage_one_checkpoint,
            None,
        )
        .expect("slot-one stage should retain the verified slot-zero incumbent");

        assert_eq!(stage_two.allowed_potion_slots(), Some(0b10));
        assert!(stage_two.has_verified_witness());
        let retained = stage_two
            .best_witness()
            .expect("prior-stage incumbent remains the portfolio fallback");
        assert_eq!(
            combat_witness_potion_expenditure_slots(&stage_two.start, retained),
            0b01
        );
        assert!(
            !combat_witness_within_potion_contract(
                &stage_two.start,
                retained,
                stage_two.max_potions_used(),
                stage_two.allowed_potion_slots(),
            ),
            "the prior incumbent is retained without widening the new stage's generation contract"
        );
        let progress = stage_two.progress();
        assert_eq!(progress.incumbent_potion_slots, Some(0b01));

        let stage_two_checkpoint = stage_two.checkpoint();
        assert_eq!(stage_two_checkpoint.allowed_potion_slots, Some(0b10));
        assert_eq!(
            combat_witness_potion_expenditure_slots(
                &stage_two.start,
                stage_two_checkpoint
                    .incumbent
                    .as_ref()
                    .expect("prior-stage fallback remains checkpointed"),
            ),
            0b01
        );
        let restored = OracleRunCombatWitnessWorkV1::restart_from_checkpoint_with_guidance(
            &session,
            stage_two_options,
            stage_two_checkpoint,
            None,
        )
        .expect("same-stage process restore should retain the exact prior-stage fallback");

        assert_eq!(restored.allowed_potion_slots(), Some(0b10));
        assert!(restored.has_verified_witness());
        assert_eq!(
            restored.progress().incumbent_potion_slots,
            Some(0b01),
            "checkpoint restore must not lose the earlier identity's verified victory"
        );
    }

    #[test]
    fn wider_potion_stage_keeps_a_separate_entry_incumbent_challenge() {
        let (session, stage_one) = explosive_stage_one_work();
        let stage_two = OracleRunCombatWitnessWorkV1::restart_for_higher_fidelity_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                max_potions_used: Some(2),
                allowed_potion_slots: Some(0b11),
                satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
                ..OracleCombatWitnessOptionsV1::default()
            },
            stage_one.checkpoint(),
            None,
        )
        .expect("wider stage should retain the exact slot-zero incumbent");

        assert!(stage_two.incumbents.prior_stage_incumbent.is_none());
        let inherited = stage_two
            .incumbents
            .stage_entry_incumbent
            .as_ref()
            .expect("stage entry should remain explicit even inside the wider mask");
        assert!(combat_witness_within_potion_contract(
            &stage_two.start,
            inherited,
            stage_two.max_potions_used(),
            stage_two.allowed_potion_slots(),
        ));
        assert!(inherited_satisfying_incumbent_challenged(
            true,
            stage_two.satisfaction,
            &stage_two.start,
            Some(inherited),
            16,
            16,
        ));
    }

    #[test]
    fn prior_stage_incumbent_must_replay_exactly_before_restore() {
        let (session, stage_one) = explosive_stage_one_work();
        let mut checkpoint = stage_one.checkpoint();
        checkpoint
            .incumbent
            .as_mut()
            .expect("slot-zero exact incumbent")
            .final_position
            .combat
            .entities
            .player
            .current_hp -= 1;

        let error = match OracleRunCombatWitnessWorkV1::restart_for_higher_fidelity_with_guidance(
            &session,
            single_potion_slot_options(0b10),
            checkpoint,
            None,
        ) {
            Ok(_) => {
                panic!("an unreplayable prior-stage incumbent must not enter the fallback channel")
            }
            Err(error) => error,
        };

        assert!(error.contains("final position"), "{error}");
    }

    #[test]
    fn passive_fairy_consumption_is_not_mislabeled_as_zero_potion() {
        let mut session = hallway_combat_session();
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .potions = vec![Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::FairyPotion,
            9,
        ))];
        let start = session
            .current_active_combat_position()
            .expect("exact Fairy root");
        let mut final_position = start.clone();
        final_position.combat.entities.potions[0] = None;
        let witness = OracleCombatWitness {
            actions: Vec::new(),
            final_position,
            negative_log_policy: 0.0,
            replay_engine_steps: 0,
            discovery_source: OracleCombatWitnessDiscoverySource::RestoredExactActions,
        };

        assert_eq!(combat_witness_potion_expenditures(&start, &witness), 1);
        assert_eq!(combat_witness_potion_expenditure_slots(&start, &witness), 1);
        assert!(!combat_witness_within_potion_contract(
            &start,
            &witness,
            Some(0),
            None,
        ));
        assert!(!combat_witness_within_potion_contract(
            &start,
            &witness,
            Some(1),
            Some(0),
        ));
        assert!(combat_witness_within_potion_contract(
            &start,
            &witness,
            Some(1),
            Some(1),
        ));
    }

    #[test]
    fn sub_millisecond_wall_tail_is_not_treated_as_usable_allowance() {
        assert!(wall_allowance_exhausted(Some(Duration::from_micros(999))));
        assert!(!wall_allowance_exhausted(Some(Duration::from_millis(1))));
        assert!(!wall_allowance_exhausted(None));
    }

    #[test]
    fn portfolio_alternates_live_members_and_skips_completed_members() {
        assert_eq!(
            select_portfolio_member(PortfolioMemberV1::LocalTurnGraph, false, false,),
            Some(PortfolioMemberV1::LocalTurnGraph)
        );
        assert_eq!(
            select_portfolio_member(PortfolioMemberV1::PolicyDiscrepancy, false, false,),
            Some(PortfolioMemberV1::PolicyDiscrepancy)
        );
        assert_eq!(
            select_portfolio_member(PortfolioMemberV1::LocalTurnGraph, true, false,),
            Some(PortfolioMemberV1::PolicyDiscrepancy)
        );
        assert_eq!(
            select_portfolio_member(PortfolioMemberV1::PolicyDiscrepancy, false, true,),
            Some(PortfolioMemberV1::LocalTurnGraph)
        );
        assert_eq!(
            select_portfolio_member(PortfolioMemberV1::LocalTurnGraph, true, true,),
            None
        );
    }

    #[test]
    fn insufficient_incumbent_keeps_service_with_its_productive_member() {
        assert_eq!(
            select_productive_portfolio_member(
                Some(PortfolioMemberV1::LocalTurnGraph),
                PortfolioMemberV1::PolicyDiscrepancy,
                false,
                false,
            ),
            Some(PortfolioMemberV1::LocalTurnGraph)
        );
        assert_eq!(
            select_productive_portfolio_member(
                Some(PortfolioMemberV1::PolicyDiscrepancy),
                PortfolioMemberV1::LocalTurnGraph,
                false,
                false,
            ),
            Some(PortfolioMemberV1::PolicyDiscrepancy)
        );
        assert_eq!(
            select_productive_portfolio_member(
                Some(PortfolioMemberV1::LocalTurnGraph),
                PortfolioMemberV1::LocalTurnGraph,
                true,
                false,
            ),
            Some(PortfolioMemberV1::PolicyDiscrepancy)
        );
    }

    #[test]
    fn portfolio_charges_each_live_search_from_one_shared_allowance() {
        let session = hallway_combat_session();
        let mut work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("portfolio should accept an active combat");
        let quantum = OracleCombatWitnessQuantumV1 {
            label: "portfolio_contract",
            additional_generation_work: 1,
            soft_wall_ms: None,
        };

        assert_eq!(
            work.advance(&quantum, None),
            OracleCombatWitnessAdvanceV1::Pending
        );
        assert_eq!(work.members.local_search.counters().generation_work, 1);
        assert_eq!(
            work.members
                .discrepancy_search
                .counters()
                .applied_action_transitions,
            0
        );
        assert_eq!(work.budget.remaining_generation_work, 15);

        assert_eq!(
            work.advance(&quantum, None),
            OracleCombatWitnessAdvanceV1::Pending
        );
        assert_eq!(work.members.local_search.counters().generation_work, 1);
        assert_eq!(
            work.members
                .discrepancy_search
                .counters()
                .applied_action_transitions,
            1
        );
        assert_eq!(work.budget.remaining_generation_work, 14);
    }

    #[test]
    fn boss_portfolio_serves_both_live_complete_trajectory_engines() {
        let session = boss_combat_session();
        let mut work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("boss portfolio should accept an active combat");
        let quantum = OracleCombatWitnessQuantumV1 {
            label: "boss_round_robin_contract",
            additional_generation_work: 1,
            soft_wall_ms: None,
        };

        assert_eq!(
            work.advance(&quantum, None),
            OracleCombatWitnessAdvanceV1::Pending
        );
        assert_eq!(
            work.advance(&quantum, None),
            OracleCombatWitnessAdvanceV1::Pending
        );
        assert_eq!(work.members.local_search.counters().generation_work, 1);
        assert_eq!(
            work.members
                .discrepancy_search
                .counters()
                .applied_action_transitions,
            1
        );
        assert_eq!(work.budget.remaining_generation_work, 14);
    }

    #[test]
    fn timed_bronze_plan_materializes_one_bounded_charged_prefix() {
        let session = bronze_automaton_combat_session();
        let max_generation_work = 128;
        let work = OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(max_generation_work),
                satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("Bronze Automaton should create a production portfolio");
        let progress = work.progress();

        assert_eq!(progress.plan_prefix_proposals, 1);
        assert!((1..=6).contains(&progress.plan_prefix_proposed_turns));
        assert!((1..=64).contains(&progress.plan_prefix_proposed_actions));
        assert_eq!(progress.plan_prefix_proposal_rejections, 0);
        assert_eq!(
            max_generation_work.saturating_sub(work.budget.remaining_generation_work),
            progress.plan_prefix_proposed_actions,
            "materialized prefix actions must consume the shared generation allowance"
        );
        assert_eq!(
            progress.current_search_generation_work as usize,
            progress.plan_prefix_proposed_actions
        );
        assert_eq!(
            max_generation_work
                .saturating_mul(work.budget.max_transition_steps)
                .saturating_sub(work.budget.remaining_engine_steps),
            progress.engine_steps,
            "exact transition previews must consume the shared engine allowance"
        );
    }

    #[test]
    fn encounter_without_timed_plan_does_not_offer_a_prefix() {
        let session = hallway_combat_session();
        let max_generation_work = 16;
        let work = OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(max_generation_work),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("ordinary hallway combat should create a production portfolio");
        let progress = work.progress();

        assert_eq!(progress.plan_prefix_proposals, 0);
        assert_eq!(progress.plan_prefix_proposed_turns, 0);
        assert_eq!(progress.plan_prefix_proposed_actions, 0);
        assert_eq!(progress.plan_prefix_proposal_rejections, 0);
        assert_eq!(work.budget.remaining_generation_work, max_generation_work);
    }

    #[test]
    fn current_stage_probe_ignores_local_and_portfolio_satisfaction() {
        let session = one_strike_win_session();
        let mut work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                satisfaction: Some(CombatWitnessSatisfactionV1::HpLossAtMost(0)),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("one-strike combat should create a portfolio");

        let result = work.advance_current_stage_probe(
            &OracleCombatWitnessQuantumV1 {
                label: "current_stage_probe_contract",
                additional_generation_work: 1,
                soft_wall_ms: None,
            },
            None,
        );

        assert!(
            work.members.local_search.counters().generation_work > 0,
            "the probe must serve the local graph regardless of local satisfaction"
        );
        assert_eq!(
            result,
            OracleCombatWitnessAdvanceV1::Pending,
            "quality satisfaction must not terminate the caller-bounded probe"
        );
    }

    #[test]
    fn explicit_budget_quality_mode_does_not_invent_an_early_threshold() {
        let session = one_strike_win_session();
        let mut work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                satisfaction: Some(CombatWitnessSatisfactionV1::BudgetOrExhaustion),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("one-strike combat should create a portfolio");

        let result = work.advance_improving_incumbent(
            &OracleCombatWitnessQuantumV1 {
                label: "explicit_budget_contract",
                additional_generation_work: 1,
                soft_wall_ms: None,
            },
            None,
        );

        assert_eq!(result, OracleCombatWitnessAdvanceV1::Pending);
    }

    #[test]
    fn persistent_payoff_satisfaction_requires_materialized_run_value() {
        let session = one_strike_win_session();
        let work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                satisfaction: Some(CombatWitnessSatisfactionV1::MaterializedPersistentPayoffGain),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("one-strike combat should create a portfolio");
        let plain = synthetic_witness(
            &work.start,
            work.start.combat.entities.player.current_hp,
            false,
        );
        let mut profitable = plain.clone();
        profitable
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = 20;

        assert!(!combat_witness_satisfies(
            PortfolioWitnessSatisfactionV1::MaterializedPersistentPayoffGain,
            &work.start,
            &plain
        ));
        assert!(combat_witness_satisfies(
            PortfolioWitnessSatisfactionV1::MaterializedPersistentPayoffGain,
            &work.start,
            &profitable
        ));
    }

    #[test]
    fn production_darkling_work_reports_its_typed_guide_service_bias() {
        let session = darkling_combat_session();
        let work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("Darkling combat should create a portfolio");

        let bias = work
            .progress()
            .guide_service_bias
            .expect("Darkling production work should report its service bias");
        assert_eq!(bias.lane, sts_combat_planner::CombatGuideLaneId::new(2));
        assert_eq!(bias.extra_services_per_cycle, 2);
    }

    #[test]
    fn standard_advance_ends_a_stage_only_on_a_satisfying_new_win() {
        // Keep one witness shape fixed and vary only its final HP so this
        // contract tests stage termination rather than search reachability.
        let session = one_strike_win_session();
        let work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                satisfaction: Some(CombatWitnessSatisfactionV1::HpLossAtMost(8)),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("one-strike combat should create a portfolio");
        let satisfying = synthetic_witness(
            &work.start,
            work.start.combat.entities.player.current_hp,
            false,
        );
        let mut insufficient = satisfying.clone();
        insufficient
            .final_position
            .combat
            .entities
            .player
            .current_hp = work
            .start
            .combat
            .entities
            .player
            .current_hp
            .saturating_sub(20);

        assert!(!standard_witness_ends_stage(
            true,
            true,
            PortfolioWitnessSatisfactionV1::HpLossAtMost(8),
            &work.start,
            Some(&insufficient)
        ));
        assert!(standard_witness_ends_stage(
            true,
            true,
            PortfolioWitnessSatisfactionV1::HpLossAtMost(8),
            &work.start,
            Some(&satisfying)
        ));
        assert!(standard_witness_ends_stage(
            true,
            true,
            PortfolioWitnessSatisfactionV1::FirstWitness,
            &work.start,
            Some(&insufficient)
        ));
        assert!(!standard_witness_ends_stage(
            true,
            false,
            PortfolioWitnessSatisfactionV1::HpLossAtMost(8),
            &work.start,
            Some(&satisfying)
        ));
        assert!(inherited_satisfying_incumbent_challenged(
            true,
            PortfolioWitnessSatisfactionV1::HpLossAtMost(8),
            &work.start,
            Some(&satisfying),
            64,
            64
        ));
        assert!(!inherited_satisfying_incumbent_challenged(
            true,
            PortfolioWitnessSatisfactionV1::HpLossAtMost(8),
            &work.start,
            Some(&insufficient),
            64,
            64
        ));
    }

    #[test]
    fn quality_satisfaction_rejects_a_verified_but_over_budget_win() {
        let session = one_strike_win_session();
        let work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(16),
                satisfaction: Some(CombatWitnessSatisfactionV1::HpLossAtMost(8)),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("one-strike combat should create a portfolio");
        let mut poor = synthetic_witness(
            &work.start,
            work.start.combat.entities.player.current_hp,
            false,
        );
        poor.final_position.combat.entities.player.current_hp = work
            .start
            .combat
            .entities
            .player
            .current_hp
            .saturating_sub(20);

        assert!(!combat_witness_satisfies(
            PortfolioWitnessSatisfactionV1::HpLossAtMost(8),
            &work.start,
            &poor
        ));
    }

    fn test_witness_quality(
        final_hp: i32,
        ritual_dagger_value: i32,
        action_count: usize,
        negative_log_policy: f64,
    ) -> CombatWitnessQualityV1 {
        CombatWitnessQualityV1 {
            final_hp,
            persistent: CombatPersistentOutcomeV1 {
                max_hp: 72,
                recoverable_gold_delta: 0,
                unrecovered_stolen_gold: 0,
                ritual_dagger_value,
                genetic_algorithm_value: 0,
                external_burden_count: 0,
            },
            potions_used: 0,
            action_count,
            negative_log_policy,
        }
    }

    #[test]
    fn equal_hp_search_result_does_not_end_fallback_improvement() {
        let fallback = test_witness_quality(56, 15, 33, 33.0);
        let equal_hp_search_result = test_witness_quality(56, 15, 30, 5.0);

        assert!(combat_witness_quality_better(
            equal_hp_search_result,
            fallback
        ));
        assert!(!combat_witness_acceptance_improved(
            Some(fallback),
            Some(equal_hp_search_result)
        ));
    }

    #[test]
    fn portfolio_accepts_free_growth_without_pricing_growth_as_hp() {
        let ordinary_lethal = test_witness_quality(13, 15, 30, 3.0);
        let free_growth_lethal = test_witness_quality(13, 18, 31, 4.0);
        let costly_growth_lethal = test_witness_quality(11, 18, 29, 2.0);

        assert!(combat_witness_quality_better(
            free_growth_lethal,
            ordinary_lethal
        ));
        assert!(combat_witness_acceptance_improved(
            Some(ordinary_lethal),
            Some(free_growth_lethal)
        ));
        assert!(
            !combat_witness_quality_better(costly_growth_lethal, ordinary_lethal),
            "an unresolved growth/HP trade must not be collapsed into hidden exchange units"
        );
        assert!(!combat_witness_acceptance_improved(
            Some(ordinary_lethal),
            Some(costly_growth_lethal)
        ));
    }

    #[test]
    fn portfolio_does_not_price_recoverable_stolen_gold_as_hidden_hp() {
        let session = hallway_combat_session();
        let start = session
            .current_active_combat_position()
            .expect("hallway combat root");
        let mut escaped = synthetic_witness(&start, 82, false);
        escaped
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        let mut killed = synthetic_witness(&start, 71, false);
        killed
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        killed
            .final_position
            .combat
            .runtime
            .pending_rewards
            .push(crate::state::rewards::RewardItem::StolenGold { amount: 75 });

        assert!(combat_witness_better(&start, &escaped, &killed));
        assert!(!combat_witness_better(&start, &killed, &escaped));
    }

    #[test]
    fn production_retains_gold_hp_tradeoff_but_uses_survival_fallback() {
        let session = hallway_combat_session();
        let start = session
            .current_active_combat_position()
            .expect("hallway combat root");
        let mut work = OracleRunCombatWitnessWorkV1::for_exact_action_witness_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(1),
                max_potions_used: Some(0),
                satisfaction: Some(CombatWitnessSatisfactionV1::BudgetOrExhaustion),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("production combat work");

        let mut escaped = synthetic_witness(&start, 82, false);
        escaped
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        let mut killed = synthetic_witness(&start, 71, false);
        killed
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        killed
            .final_position
            .combat
            .runtime
            .pending_rewards
            .push(crate::state::rewards::RewardItem::StolenGold { amount: 75 });
        for witness in [&mut escaped, &mut killed] {
            let monster = witness
                .final_position
                .combat
                .entities
                .monsters
                .first_mut()
                .expect("terminal monster");
            monster.current_hp = 0;
            monster.is_dying = true;
        }

        work.members
            .local_search
            .restore_verified_witness(escaped)
            .expect("escape outcome remains an exact terminal victory");
        work.members
            .local_search
            .restore_verified_witness(killed)
            .expect("recovered-gold outcome remains an exact terminal victory");

        assert_eq!(work.members.local_search.witness_frontier().len(), 2);
        let selected = work.best_witness().expect("production incumbent");
        assert_eq!(
            selected.final_position.combat.entities.player.current_hp,
            82
        );
        assert_eq!(
            crate::ai::combat_persistent_outcome_v1::recoverable_stolen_gold(
                &selected.final_position.combat,
            ),
            0
        );
    }

    #[test]
    fn production_exact_graph_charges_only_materialized_generation_work() {
        let session = hallway_combat_session();
        let mut work = OracleRunCombatWitnessWorkV1::new_with_guidance(
            &session,
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(4_096),
                satisfaction: Some(CombatWitnessSatisfactionV1::BudgetOrExhaustion),
                ..OracleCombatWitnessOptionsV1::default()
            },
            None,
        )
        .expect("portfolio should accept an active combat");
        let quantum = OracleCombatWitnessQuantumV1 {
            label: "exact_generation_accounting_contract",
            additional_generation_work: 4_096,
            soft_wall_ms: None,
        };

        let before_remaining = work.budget.remaining_generation_work;
        let _ = work.advance(&quantum, None);
        let counters = work.members.local_search.counters();
        assert_eq!(
            work.members
                .discrepancy_search
                .counters()
                .applied_action_transitions,
            0
        );
        assert_eq!(
            before_remaining.saturating_sub(work.budget.remaining_generation_work),
            counters.generation_work
        );
    }

    #[test]
    fn portfolio_preserves_stable_overall_status_semantics() {
        assert_eq!(
            overall_status_label(
                false,
                Some(&LocalTurnGraphWitnessStatus::FrontierExhausted),
                Some(&PolicyDiscrepancyStatus::FrontierExhausted),
                None,
            ),
            Some("frontier_exhausted")
        );
        assert_eq!(
            overall_status_label(
                false,
                Some(&LocalTurnGraphWitnessStatus::MechanicsGap),
                Some(&PolicyDiscrepancyStatus::FrontierExhausted),
                None,
            ),
            Some("mechanics_gap")
        );
        assert_eq!(
            overall_status_label(
                true,
                Some(&LocalTurnGraphWitnessStatus::Partial(
                    sts_combat_planner::LocalTurnGraphWitnessInterruption::SelectionBudget,
                )),
                None,
                None,
            ),
            Some("witness_found")
        );
    }
}

fn local_witness_status_label(status: &LocalTurnGraphWitnessStatus) -> &'static str {
    match status {
        LocalTurnGraphWitnessStatus::WitnessFound => "witness_found",
        LocalTurnGraphWitnessStatus::Partial(_) => "partial",
        LocalTurnGraphWitnessStatus::FrontierExhausted => "frontier_exhausted",
        LocalTurnGraphWitnessStatus::MechanicsGap => "mechanics_gap",
        LocalTurnGraphWitnessStatus::ReplayMismatch(_) => "replay_mismatch",
    }
}

fn discrepancy_status_label(status: &PolicyDiscrepancyStatus) -> &'static str {
    match status {
        PolicyDiscrepancyStatus::WitnessFound => "witness_found",
        PolicyDiscrepancyStatus::Partial(_) => "partial",
        PolicyDiscrepancyStatus::FrontierExhausted => "frontier_exhausted",
        PolicyDiscrepancyStatus::ReplayMismatch => "replay_mismatch",
    }
}

fn portfolio_status_label(status: &PortfolioStatusV1) -> &'static str {
    match status {
        PortfolioStatusV1::Local(status) => local_witness_status_label(status),
        PortfolioStatusV1::PolicyDiscrepancy(status) => discrepancy_status_label(status),
    }
}

fn overall_status_label(
    has_witness: bool,
    local: Option<&LocalTurnGraphWitnessStatus>,
    discrepancy: Option<&PolicyDiscrepancyStatus>,
    last: Option<&PortfolioStatusV1>,
) -> Option<&'static str> {
    if has_witness {
        return Some("witness_found");
    }
    if matches!(local, Some(LocalTurnGraphWitnessStatus::FrontierExhausted))
        && matches!(
            discrepancy,
            Some(PolicyDiscrepancyStatus::FrontierExhausted)
        )
    {
        return Some("frontier_exhausted");
    }
    if matches!(local, Some(LocalTurnGraphWitnessStatus::ReplayMismatch(_)))
        || matches!(discrepancy, Some(PolicyDiscrepancyStatus::ReplayMismatch))
    {
        return Some("replay_mismatch");
    }
    if matches!(local, Some(LocalTurnGraphWitnessStatus::MechanicsGap)) {
        return Some("mechanics_gap");
    }
    last.map(portfolio_status_label)
}
