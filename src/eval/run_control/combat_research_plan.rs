use crate::agent::information::action::{
    public_choice_for_exact_combat_input_v1, CombatActionProjectionV1, PublicCombatActionChoiceV1,
};
use crate::agent::information::state::public_combat_state_v1;
use crate::agent::task::public_combat_plan_v1;
use crate::agent::task::{
    TaskPrivilegedSupportAccessV1, TaskPrivilegedSupportProvenanceV1, TaskPublicChoiceV1,
    TaskPublicDecisionV1, TaskResearchPlanAttemptExitV1, TaskResearchPlanAttemptV1,
    TaskResearchPlanCandidateV1, TaskResearchPlanLaneAttemptV1, TaskResearchPlanOutcomeV1,
    TaskResearchPlanPendingRewardV1, TaskResearchPlanStepV1, TaskResearchPlanV1,
    TaskResearchPlanWorkUnitV1,
};
use crate::ai::combat_search_v2::{
    run_combat_search_v2, CombatSearchV2Config, CombatSearchV2TurnPlanPolicy, SearchCoverageStatus,
};
use crate::ai::combat_witness_contract::{
    CombatWitnessPotionPolicyV1, CombatWitnessSatisfactionV1,
};
use crate::ai::planner_core::PlannerDecisionContext;
use crate::sim::combat::CombatPosition;
use crate::sim::combat::{CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper};
use crate::state::core::ClientInput;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use sts_combat_planner::{
    LocalTurnGraphMilestoneClosurePathSnapshot, LocalTurnGraphPlanTransitionPathSnapshot,
    OracleCombatDeepStateSnapshot,
};
use sts_combat_strategy::{
    combat_plan_projection_v1, combat_plan_transition_annotation_v1, CombatPlanMilestoneV1,
    CombatPlanProjectionV1, CombatPlanTransitionAnnotationV1,
};

use crate::eval::combat_witness_materialization::{
    materialize_combat_witness_outcome_v1 as combat_witness_materialized_resources_v1,
    materialize_replay_checked_combat_outcome_v1,
    CombatWitnessMaterializedOutcomeV1 as CombatWitnessMaterializedResourcesV1,
};

use super::{
    retained_combat_tactical_policy_v1,
    task_world_support::{prepare_task_choice_v1, task_public_decision_v1},
    LearningBoundaryV1, LearningEnvV3, LearningPreparedActionV3, OracleCombatWitnessAdvanceV1,
    OracleCombatWitnessOptionsV1, OracleCombatWitnessQuantumV1,
    OracleResidentCombatWitnessEvidenceV1, OracleResidentCombatWitnessJobV1,
};

#[path = "combat_research_plan/lanes.rs"]
mod lanes;

use lanes::{
    run_atomic_research_lane_v1, run_complete_turn_proposal_beam_lane_v1,
    run_milestone_closure_lane_v1, run_resident_research_lane_v1, ResearchLaneResultV1,
};

pub const RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1: &str =
    "turn_graph_portfolio_exact_combat_research_plan";
pub const ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1: &str =
    "atomic_exact_v2_turn_boundary_research_plan_challenger";
pub const COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1: &str =
    "synchronous_complete_turn_proposal_beam_research_plan";
pub const RESIDENT_COMBAT_MILESTONE_PLAN_PRODUCER_V1: &str =
    "turn_graph_portfolio_exact_combat_milestone_plan";
pub const MILESTONE_CLOSURE_RESEARCH_PLAN_PRODUCER_V1: &str =
    "typed_exact_combat_milestone_closure";
pub const WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1: &str = "whole_combat_solver";
const COMBAT_RESEARCH_PLAN_PRODUCER_REVISION_V3: u32 = 3;

/// Whole-combat executor shared by run-play and strategic learning. Uses the same public
/// replay/settlement owner as the older portfolio; only the search changes.
pub fn run_control_session_whole_combat_portfolio_v1(
    session: &super::RunControlSession,
    options: CombatResearchPlanOptionsV1,
) -> Result<CombatResearchPortfolioV1, String> {
    let env = LearningEnvV3::from_session(session.clone());
    whole_combat_portfolio_v1(&env, &options, None)
}

fn whole_combat_portfolio_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    deadline: Option<Instant>,
) -> Result<CombatResearchPortfolioV1, String> {
    let mut research = WholeCombatResearchV1::new(env.clone(), options.clone())?;
    let wall = Duration::from_millis(options.wall_ms);
    let wall = deadline.map_or(wall, |limit| {
        wall.min(limit.saturating_duration_since(Instant::now()))
    });
    research.advance_to_with_wall(options.max_generation_work, wall)
}

/// One exact-root search retained across caller-owned resource studies. Work
/// targets are cumulative; pauses and publication do not reset search state or
/// its total wall allowance. Published witnesses never change on later advances.
pub struct WholeCombatResearchV1 {
    env: LearningEnvV3,
    options: CombatResearchPlanOptionsV1,
    solver: sts_guided_search::solver::CombatSolver,
    search_time: Duration,
    stop: sts_guided_search::solver::SolverStop,
}

impl WholeCombatResearchV1 {
    pub fn from_session(
        session: &super::RunControlSession,
        options: CombatResearchPlanOptionsV1,
    ) -> Result<Self, String> {
        Self::new(LearningEnvV3::from_session(session.clone()), options)
    }

    fn new(env: LearningEnvV3, options: CombatResearchPlanOptionsV1) -> Result<Self, String> {
        use sts_guided_search::solver::{CombatSolver, SolverConfig, SolverStop};
        options.validate()?;
        let solver = CombatSolver::new(
            env.session().current_active_combat_position()?,
            SolverConfig {
                max_engine_steps_per_input: options.max_engine_steps_per_transition,
                max_potion_uses: if options.potion_policy == CombatWitnessPotionPolicyV1::Never {
                    Some(0)
                } else {
                    options.max_potions_used.map(|n| n as usize)
                },
                ..SolverConfig::default()
            },
        )?;
        Ok(Self {
            env,
            options,
            solver,
            search_time: Duration::ZERO,
            stop: SolverStop::WorkBudget,
        })
    }

    pub fn work_consumed(&self) -> usize {
        self.solver.stats().work()
    }

    pub fn advance_to(&mut self, work: usize) -> Result<CombatResearchPortfolioV1, String> {
        self.advance_to_with_wall(work, Duration::from_millis(self.options.wall_ms))
    }

    fn advance_to_with_wall(
        &mut self,
        work: usize,
        wall: Duration,
    ) -> Result<CombatResearchPortfolioV1, String> {
        if work == 0 || work > self.options.max_generation_work || work < self.work_consumed() {
            return Err(
                "whole combat cumulative target is outside the remaining work contract".into(),
            );
        }
        let additional = work - self.work_consumed();
        if additional > 0 {
            let remaining = Duration::from_millis(self.options.wall_ms)
                .saturating_sub(self.search_time)
                .min(wall);
            let started = Instant::now();
            let advanced = self.solver.advance(additional, remaining);
            self.search_time += started.elapsed();
            self.stop = advanced?;
        }
        whole_combat_portfolio_from_solver_v1(&self.env, &self.options, &self.solver, self.stop)
    }
}

fn whole_combat_portfolio_from_solver_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    solver: &sts_guided_search::solver::CombatSolver,
    stop: sts_guided_search::solver::SolverStop,
) -> Result<CombatResearchPortfolioV1, String> {
    use sts_guided_search::solver::SolverStop;
    let position = solver.root();
    let provenance = research_plan_provenance_v1(WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1);
    let mut candidates: Vec<TaskResearchPlanCandidateV1> = Vec::new();
    let mut witness_input_lines = Vec::new();
    let mut witness_replays: Vec<Option<CombatResearchReplayMaterializationV1>> = Vec::new();
    for plan in solver.plans() {
        let (candidate, replay) = lanes::materialize_public_research_candidate_and_replay_v1(
            env,
            position,
            &plan.inputs,
            provenance.clone(),
            &plan.outcome,
            options.satisfaction,
        )?;
        // Keep all candidate facts and inputs. Retain expensive step payloads
        // only while their fully settled result belongs to the visible resource
        // frontier; a later adapter can reuse them without a third replay.
        let dominated = candidates
            .iter()
            .any(|other| other.resource_dominates_v1(&candidate));
        for (other, retained) in candidates.iter().zip(&mut witness_replays) {
            if candidate.resource_dominates_v1(other) {
                *retained = None;
            }
        }
        witness_replays.push((!dominated).then_some(replay));
        candidates.push(candidate);
        witness_input_lines.push(plan.inputs.clone());
    }
    let exit = match stop {
        SolverStop::WorkBudget if !candidates.is_empty() => {
            TaskResearchPlanAttemptExitV1::WitnessAtWorkLimit
        }
        SolverStop::TimeBudget if !candidates.is_empty() => {
            TaskResearchPlanAttemptExitV1::WitnessAtWallLimit
        }
        SolverStop::WorkBudget => TaskResearchPlanAttemptExitV1::GenerationWorkExhausted,
        SolverStop::TimeBudget => TaskResearchPlanAttemptExitV1::WallTimeExhausted,
        SolverStop::ScheduleComplete if !candidates.is_empty() => {
            TaskResearchPlanAttemptExitV1::Witness
        }
        SolverStop::ScheduleComplete => {
            TaskResearchPlanAttemptExitV1::PortfolioCompleteWithoutWitness
        }
    };
    // Report actual simulated depth on a miss, without inventing a surviving
    // envelope or an adoptable prefix. A terminal losing state can set this depth.
    let bounded_progress = if candidates.is_empty() {
        vec![CombatResearchBoundedProgressMaterializationV1 {
            provenance: provenance.clone(),
            max_player_turn: solver
                .stats()
                .deepest_turn
                .max(position.combat.turn.turn_count),
            recent_turn_survival_envelope: vec![],
            paths: vec![],
        }]
    } else {
        vec![]
    };
    Ok(CombatResearchPortfolioV1 {
        attempt: TaskResearchPlanAttemptV1 {
            candidates,
            lanes: vec![TaskResearchPlanLaneAttemptV1 {
                provenance,
                work_unit: TaskResearchPlanWorkUnitV1::WholeCombatTransitions,
                work_consumed: solver.stats().work() as u64,
                exit,
            }],
        },
        witness_input_lines,
        witness_replays,
        milestone_candidates: vec![],
        bounded_progress,
    })
}

/// One replay-exact finite prefix which completes an encounter-owned typed
/// milestone while the combat remains unresolved.
///
/// This is deliberately separate from `TaskResearchPlanCandidateV1`: it has
/// no terminal combat outcome and therefore cannot enter the complete-plan
/// resource frontier. The bound public plan is executable evidence; the
/// transition annotation is the milestone acceptance proof.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatMilestoneResearchCandidateV1 {
    pub plan: TaskResearchPlanV1,
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub transition: CombatPlanTransitionAnnotationV1,
    pub successor_plan: CombatPlanProjectionV1,
    pub final_player_hp: i32,
}

/// Process-local public replay materialization retained beside a finite
/// milestone candidate. It is deliberately outside the serialized candidate
/// identity: the candidate remains the durable typed evidence, while run-play
/// can project its semantic line and successor without replaying every prefix
/// from the full run root a second time.
#[derive(Clone, Debug)]
pub struct CombatMilestoneResearchMaterializationV1 {
    pub candidate: CombatMilestoneResearchCandidateV1,
    pub source_decisions: Vec<TaskPublicDecisionV1>,
    pub successor_decision: TaskPublicDecisionV1,
}

/// Neutral dimensions for one replayable prefix retained by a bounded combat
/// search. A role describes what the producer observed; it is not a score or a
/// recommendation to commit the prefix.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatResearchBoundedProgressRoleV1 {
    DeepestSurvival,
    DeepestEnemyProgress,
}

/// Public combat scalars at the end of one bounded-search prefix.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchBoundedProgressStateV1 {
    pub player_turn: u32,
    pub player_hp: i32,
    pub player_block: i32,
    pub alive_enemy_count: usize,
    pub enemy_total_hp: i32,
    pub hand_size: usize,
    pub draw_pile_size: usize,
    pub discard_pile_size: usize,
    pub exhaust_pile_size: usize,
    pub path_atomic_depth: usize,
}

impl From<&OracleCombatDeepStateSnapshot> for CombatResearchBoundedProgressStateV1 {
    fn from(value: &OracleCombatDeepStateSnapshot) -> Self {
        Self {
            player_turn: value.player_turn,
            player_hp: value.player_hp,
            player_block: value.player_block,
            alive_enemy_count: value.alive_enemy_count,
            enemy_total_hp: value.enemy_total_hp,
            hand_size: value.hand_size,
            draw_pile_size: value.draw_pile_size,
            discard_pile_size: value.discard_pile_size,
            exhaust_pile_size: value.exhaust_pile_size,
            path_atomic_depth: value.path_atomic_depth,
        }
    }
}

/// One exact, publicly replayable prefix selected for diagnostic progress.
/// Several roles may name the same prefix; keeping them together avoids
/// presenting duplicate lines as independent evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchBoundedProgressPathV1 {
    pub roles: Vec<CombatResearchBoundedProgressRoleV1>,
    pub plan: TaskResearchPlanV1,
    pub endpoint: CombatResearchBoundedProgressStateV1,
}

/// How far one producer actually progressed inside its finite whole-combat
/// search. This remains separate from terminal candidates and milestones:
/// absence of a witness is still Unknown, and none of these prefixes has
/// commit authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchBoundedProgressV1 {
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub max_player_turn: u32,
    pub recent_turn_survival_envelope: Vec<CombatResearchBoundedProgressStateV1>,
    pub paths: Vec<CombatResearchBoundedProgressPathV1>,
}

#[derive(Clone, Debug)]
pub struct CombatResearchBoundedProgressPathMaterializationV1 {
    pub candidate: CombatResearchBoundedProgressPathV1,
    /// Process-local exact inputs retained only long enough for a trusted
    /// adapter to translate the prefix into a runtime-owned typed intent.
    /// They are deliberately absent from the durable public candidate.
    pub exact_inputs: Vec<ClientInput>,
    pub source_decisions: Vec<TaskPublicDecisionV1>,
    pub successor_decision: TaskPublicDecisionV1,
}

#[derive(Clone, Debug)]
pub struct CombatResearchBoundedProgressMaterializationV1 {
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub max_player_turn: u32,
    pub recent_turn_survival_envelope: Vec<CombatResearchBoundedProgressStateV1>,
    pub paths: Vec<CombatResearchBoundedProgressPathMaterializationV1>,
}

#[derive(Clone, Debug)]
pub struct CombatResearchPortfolioV1 {
    pub attempt: TaskResearchPlanAttemptV1,
    /// Exact input lines aligned with `attempt.candidates`. This is a
    /// process-local materialization boundary, not public task evidence.
    pub witness_input_lines: Vec<Vec<ClientInput>>,
    /// Process-local public replays aligned with the candidate facts. Empty
    /// for producers without retained replays; dominated results may be None.
    pub witness_replays: Vec<Option<CombatResearchReplayMaterializationV1>>,
    pub milestone_candidates: Vec<CombatMilestoneResearchMaterializationV1>,
    pub bounded_progress: Vec<CombatResearchBoundedProgressMaterializationV1>,
}

#[derive(Clone, Debug)]
pub struct CombatResearchReplayStepV1 {
    pub source_decision: TaskPublicDecisionV1,
    pub monster_entities_before: Vec<(u8, crate::EntityId)>,
    pub monster_entities_after: Vec<(u8, crate::EntityId)>,
    pub action_result: Option<super::RunActionResultV1>,
    pub elapsed: Duration,
}

/// Public steps already verified against exact inputs and fully settled by
/// the run owner. This never becomes durable evidence or commit authority.
#[derive(Clone, Debug)]
pub struct CombatResearchReplayMaterializationV1 {
    pub steps: Vec<CombatResearchReplayStepV1>,
    pub successor_decision: Option<TaskPublicDecisionV1>,
    pub terminal_game_outcome: Option<crate::agent::task::GameOutcomeV1>,
    pub fork_elapsed: Duration,
    pub total_elapsed: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatResearchLaneV1 {
    WholeCombat,
    CompleteTurnProposalBeam,
    AtomicChallenger,
    ResidentTurnGraph,
    MilestoneClosure,
}

impl CombatResearchLaneV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::WholeCombat => "whole_combat",
            Self::CompleteTurnProposalBeam => "proposal",
            Self::AtomicChallenger => "atomic",
            Self::ResidentTurnGraph => "resident",
            Self::MilestoneClosure => "milestone_closure",
        }
    }
}

#[derive(Clone, Debug)]
pub enum CombatResearchProgressEventV1 {
    LaneStarted {
        lane: CombatResearchLaneV1,
    },
    LaneCompleted {
        lane: CombatResearchLaneV1,
        elapsed: Duration,
        attempt: TaskResearchPlanLaneAttemptV1,
    },
    LaneFailed {
        lane: CombatResearchLaneV1,
        elapsed: Duration,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnProposalBeamResearchOptionsV1 {
    pub beam_width: usize,
    pub max_expanded_boundaries: usize,
    pub wall_ms: u64,
}

impl Default for CompleteTurnProposalBeamResearchOptionsV1 {
    fn default() -> Self {
        Self {
            beam_width: 8,
            max_expanded_boundaries: 512,
            wall_ms: 15_000,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtomicCombatResearchPlanChallengerOptionsV1 {
    pub max_nodes: usize,
    pub wall_ms: u64,
}

impl Default for AtomicCombatResearchPlanChallengerOptionsV1 {
    fn default() -> Self {
        Self {
            max_nodes: 4_096,
            wall_ms: 1_000,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatMilestoneClosureResearchOptionsV1 {
    pub max_parent_states: usize,
    pub generation_work_per_parent: usize,
    pub max_generation_work: usize,
    pub max_candidates: usize,
    pub wall_ms: u64,
}

impl Default for CombatMilestoneClosureResearchOptionsV1 {
    fn default() -> Self {
        Self {
            max_parent_states: 256,
            generation_work_per_parent: 64,
            max_generation_work: 16_384,
            max_candidates: 32,
            wall_ms: 5_000,
        }
    }
}

/// Serial lane ownership for one read-only combat-research portfolio.
///
/// `ResidentFrontier` preserves the historical resource-refinement contract:
/// the turn-graph resident establishes the first resource incumbent before
/// proposal and atomic challengers are admitted. `FirstWitness` is a separate
/// latency contract for callers that need one replay-verified win before any
/// expensive frontier refinement. `WholeCombat` calls the maintained standalone
/// solver with the same terminal replay/resource contract and shared deadline.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatResearchLaneScheduleV1 {
    #[default]
    ResidentFrontier,
    FirstWitness,
    WholeCombat,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchPlanOptionsV1 {
    pub max_generation_work: usize,
    pub max_engine_steps_per_transition: usize,
    pub wall_ms: u64,
    pub quantum_generation_work: usize,
    pub quantum_wall_ms: u64,
    pub satisfaction: CombatWitnessSatisfactionV1,
    pub potion_policy: CombatWitnessPotionPolicyV1,
    pub max_potions_used: Option<u32>,
    #[serde(default)]
    pub lane_schedule: CombatResearchLaneScheduleV1,
    #[serde(default)]
    pub milestone_closure: Option<CombatMilestoneClosureResearchOptionsV1>,
    pub complete_turn_proposal_beam: Option<CompleteTurnProposalBeamResearchOptionsV1>,
    pub atomic_challenger: Option<AtomicCombatResearchPlanChallengerOptionsV1>,
}

impl Default for CombatResearchPlanOptionsV1 {
    fn default() -> Self {
        Self {
            max_generation_work: 4_096,
            max_engine_steps_per_transition: 250,
            wall_ms: 500,
            quantum_generation_work: 256,
            quantum_wall_ms: 50,
            satisfaction: CombatWitnessSatisfactionV1::ZeroLossOrBudget,
            potion_policy: CombatWitnessPotionPolicyV1::All,
            max_potions_used: None,
            lane_schedule: CombatResearchLaneScheduleV1::ResidentFrontier,
            milestone_closure: None,
            complete_turn_proposal_beam: None,
            atomic_challenger: Some(AtomicCombatResearchPlanChallengerOptionsV1::default()),
        }
    }
}

impl CombatResearchPlanOptionsV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_generation_work == 0
            || self.max_engine_steps_per_transition == 0
            || self.quantum_generation_work == 0
        {
            return Err("combat research plan requires positive resident work limits".to_string());
        }
        if self.wall_ms == 0 || self.quantum_wall_ms == 0 {
            return Err("combat research plan requires positive resident wall limits".to_string());
        }
        if self.milestone_closure.as_ref().is_some_and(|closure| {
            closure.max_parent_states == 0
                || closure.generation_work_per_parent == 0
                || closure.max_generation_work == 0
                || closure.max_candidates == 0
                || closure.wall_ms == 0
        }) {
            return Err("combat milestone closure requires positive limits".to_string());
        }
        if self
            .complete_turn_proposal_beam
            .as_ref()
            .is_some_and(|beam| {
                beam.beam_width == 0 || beam.max_expanded_boundaries == 0 || beam.wall_ms == 0
            })
        {
            return Err("complete-turn proposal beam requires positive limits".to_string());
        }
        if self
            .atomic_challenger
            .as_ref()
            .is_some_and(|challenger| challenger.max_nodes == 0 || challenger.wall_ms == 0)
        {
            return Err("atomic combat research challenger requires positive limits".to_string());
        }
        Ok(())
    }
}

pub(super) fn combat_research_plan_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
) -> Result<TaskResearchPlanAttemptV1, String> {
    combat_research_portfolio_v1(env, options).map(|portfolio| portfolio.attempt)
}

pub(super) fn combat_research_portfolio_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
) -> Result<CombatResearchPortfolioV1, String> {
    combat_research_portfolio_with_progress_v1(env, options, None, &mut |_| {})
}

/// Runs the maintained combat-research portfolio directly from one exact
/// in-memory session. The owner clones the session into an isolated learning
/// environment; no run archive, controller head, or durable checkpoint is
/// created or changed.
pub fn run_control_session_combat_research_portfolio_with_options_v1(
    session: &super::RunControlSession,
    options: Option<CombatResearchPlanOptionsV1>,
) -> Result<CombatResearchPortfolioV1, String> {
    let env = LearningEnvV3::from_session(session.clone());
    combat_research_portfolio_v1(&env, &options.unwrap_or_default())
}

pub(super) fn combat_research_portfolio_with_progress_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    deadline: Option<Instant>,
    progress: &mut dyn FnMut(CombatResearchProgressEventV1),
) -> Result<CombatResearchPortfolioV1, String> {
    options.validate()?;
    if !matches!(env.observe()?, LearningBoundaryV1::Combat { .. }) {
        return Err("combat research plan requested outside combat".to_string());
    }

    let position = env.session().current_active_combat_position()?;
    if deadline_reached(deadline) {
        // Record the first scheduled producer's zero-work deadline admission;
        // even search initialization may otherwise perform a rollout.
        let (producer, work_unit) = match options.lane_schedule {
            CombatResearchLaneScheduleV1::WholeCombat => (
                WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1,
                TaskResearchPlanWorkUnitV1::WholeCombatTransitions,
            ),
            CombatResearchLaneScheduleV1::FirstWitness
                if options.complete_turn_proposal_beam.is_some() =>
            {
                (
                    COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1,
                    TaskResearchPlanWorkUnitV1::CompleteTurnProposalExpandedBoundaries,
                )
            }
            CombatResearchLaneScheduleV1::FirstWitness if options.atomic_challenger.is_some() => (
                ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1,
                TaskResearchPlanWorkUnitV1::AtomicExpandedNodes,
            ),
            _ => (
                RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1,
                TaskResearchPlanWorkUnitV1::TurnGraphGenerationWork,
            ),
        };
        let mut portfolio = ResearchPlanPortfolioV1::default();
        portfolio.lanes.push(TaskResearchPlanLaneAttemptV1 {
            provenance: research_plan_provenance_v1(producer),
            work_unit,
            work_consumed: 0,
            exit: TaskResearchPlanAttemptExitV1::DeadlineReached,
        });
        return Ok(portfolio.finish());
    }
    match options.lane_schedule {
        CombatResearchLaneScheduleV1::WholeCombat => run_observed_research_lane_v1(
            CombatResearchLaneV1::WholeCombat,
            progress,
            || whole_combat_portfolio_v1(env, options, deadline),
            |result| &result.attempt.lanes[0],
        ),
        CombatResearchLaneScheduleV1::ResidentFrontier => {
            combat_research_resident_frontier_v1(env, options, &position, deadline, progress)
        }
        CombatResearchLaneScheduleV1::FirstWitness => {
            combat_research_first_witness_v1(env, options, &position, deadline, progress)
        }
    }
}

fn combat_research_resident_frontier_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    position: &CombatPosition,
    deadline: Option<Instant>,
    progress: &mut dyn FnMut(CombatResearchProgressEventV1),
) -> Result<CombatResearchPortfolioV1, String> {
    let resident = run_observed_research_lane_v1(
        CombatResearchLaneV1::ResidentTurnGraph,
        progress,
        || run_resident_research_lane_v1(env, options, position, deadline),
        |result| &result.lane.attempt,
    )
    .map_err(|error| format!("resident combat-research lane failed: {error}"))?;
    let resident_resources = resident.lane.representative_resources.clone();
    let mut conservation_potion_budget = resource_conservation_challenger_potion_budget_v1(
        resident_resources.as_ref(),
        options.complete_turn_proposal_beam.is_some() || options.atomic_challenger.is_some(),
    );
    let resident_has_witness = resident.lane.has_witness();
    let milestone_closure_paths = resident.milestone_closure_paths;
    let stop_after_resident = resident_witness_closes_research_portfolio_v1(
        resident_has_witness,
        resident.has_refinement_ending_witness,
        options.complete_turn_proposal_beam.is_some(),
        options.atomic_challenger.is_some(),
        conservation_potion_budget.is_some(),
    );
    let mut portfolio = ResearchPlanPortfolioV1::default();
    portfolio.bounded_progress.extend(resident.bounded_progress);
    portfolio.push(resident.lane)?;
    portfolio
        .milestone_candidates
        .extend(resident.milestone_candidates);
    if stop_after_resident || deadline_reached(deadline) {
        return Ok(portfolio.finish());
    }

    if let Some(closure_options) = &options.milestone_closure {
        let closure = run_observed_research_lane_v1(
            CombatResearchLaneV1::MilestoneClosure,
            progress,
            || {
                run_milestone_closure_lane_v1(
                    env,
                    options,
                    closure_options,
                    position,
                    &milestone_closure_paths,
                    deadline,
                )
            },
            |result| &result.attempt,
        )
        .map_err(|error| format!("milestone-closure combat-research lane failed: {error}"))?;
        portfolio.lanes.push(closure.attempt);
        portfolio.milestone_candidates.extend(closure.candidates);
    }

    if deadline_reached(deadline) {
        return Ok(portfolio.finish());
    }
    if let Some(beam_options) = &options.complete_turn_proposal_beam {
        let beam = run_observed_research_lane_v1(
            CombatResearchLaneV1::CompleteTurnProposalBeam,
            progress,
            || {
                run_complete_turn_proposal_beam_lane_v1(
                    env,
                    options,
                    beam_options,
                    position,
                    conservation_potion_budget,
                    deadline,
                )
            },
            |result| &result.attempt,
        )
        .map_err(|error| format!("proposal-beam combat-research lane failed: {error}"))?;
        let beam_has_requested_quality_witness = beam.has_requested_quality_witness();
        let beam_has_requested_quality_potion_free_witness =
            beam.has_requested_quality_potion_free_witness();
        conservation_potion_budget = next_challenger_potion_budget_v1(
            conservation_potion_budget,
            beam.minimum_potion_expenditures(),
        );
        portfolio.push(beam)?;
        // Requested quality is not a price for potion expenditure. While an
        // atomic challenger remains, a quality witness may close this lane
        // early only after the quality frontier itself reaches zero potions.
        if beam_has_requested_quality_witness
            && (options.atomic_challenger.is_none()
                || beam_has_requested_quality_potion_free_witness)
        {
            return Ok(portfolio.finish());
        }
    }

    if !portfolio.lanes.is_empty() && deadline_reached(deadline) {
        return Ok(portfolio.finish());
    }
    if let Some(challenger) = &options.atomic_challenger {
        let atomic = run_observed_research_lane_v1(
            CombatResearchLaneV1::AtomicChallenger,
            progress,
            || {
                run_atomic_research_lane_v1(
                    env,
                    options,
                    challenger,
                    position,
                    conservation_potion_budget,
                    deadline,
                )
            },
            |result| &result.attempt,
        )
        .map_err(|error| format!("atomic combat-research lane failed: {error}"))?;
        portfolio.push(atomic)?;
    }

    Ok(portfolio.finish())
}

fn combat_research_first_witness_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    position: &CombatPosition,
    deadline: Option<Instant>,
    progress: &mut dyn FnMut(CombatResearchProgressEventV1),
) -> Result<CombatResearchPortfolioV1, String> {
    let mut portfolio = ResearchPlanPortfolioV1::default();

    if let Some(beam_options) = &options.complete_turn_proposal_beam {
        let beam = run_observed_research_lane_v1(
            CombatResearchLaneV1::CompleteTurnProposalBeam,
            progress,
            || {
                run_complete_turn_proposal_beam_lane_v1(
                    env,
                    options,
                    beam_options,
                    position,
                    None,
                    deadline,
                )
            },
            |result| &result.attempt,
        )
        .map_err(|error| format!("proposal-beam combat-research lane failed: {error}"))?;
        let has_requested_quality_witness = beam.has_requested_quality_witness();
        portfolio.push(beam)?;
        if has_requested_quality_witness {
            return Ok(portfolio.finish());
        }
    }

    if !portfolio.lanes.is_empty() && deadline_reached(deadline) {
        return Ok(portfolio.finish());
    }
    if let Some(challenger) = &options.atomic_challenger {
        let atomic = run_observed_research_lane_v1(
            CombatResearchLaneV1::AtomicChallenger,
            progress,
            || run_atomic_research_lane_v1(env, options, challenger, position, None, deadline),
            |result| &result.attempt,
        )
        .map_err(|error| format!("atomic combat-research lane failed: {error}"))?;
        let has_requested_quality_witness = atomic.has_requested_quality_witness();
        portfolio.push(atomic)?;
        if has_requested_quality_witness {
            return Ok(portfolio.finish());
        }
    }

    if !portfolio.lanes.is_empty() && deadline_reached(deadline) {
        return Ok(portfolio.finish());
    }
    let resident = run_observed_research_lane_v1(
        CombatResearchLaneV1::ResidentTurnGraph,
        progress,
        || run_resident_research_lane_v1(env, options, position, deadline),
        |result| &result.lane.attempt,
    )
    .map_err(|error| format!("resident combat-research lane failed: {error}"))?;
    let resident_has_requested_quality_witness = resident.lane.has_requested_quality_witness();
    let milestone_closure_paths = resident.milestone_closure_paths;
    portfolio.bounded_progress.extend(resident.bounded_progress);
    portfolio.push(resident.lane)?;
    portfolio
        .milestone_candidates
        .extend(resident.milestone_candidates);
    if resident_has_requested_quality_witness || deadline_reached(deadline) {
        return Ok(portfolio.finish());
    }

    if let Some(closure_options) = &options.milestone_closure {
        let closure = run_observed_research_lane_v1(
            CombatResearchLaneV1::MilestoneClosure,
            progress,
            || {
                run_milestone_closure_lane_v1(
                    env,
                    options,
                    closure_options,
                    position,
                    &milestone_closure_paths,
                    deadline,
                )
            },
            |result| &result.attempt,
        )
        .map_err(|error| format!("milestone-closure combat-research lane failed: {error}"))?;
        portfolio.lanes.push(closure.attempt);
        portfolio.milestone_candidates.extend(closure.candidates);
    }

    Ok(portfolio.finish())
}

fn deadline_reached(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn lane_deadline(wall_ms: u64, shared: Option<Instant>) -> Instant {
    let local = Instant::now() + Duration::from_millis(wall_ms);
    shared.map_or(local, |shared| shared.min(local))
}

fn run_observed_research_lane_v1<T>(
    lane: CombatResearchLaneV1,
    progress: &mut dyn FnMut(CombatResearchProgressEventV1),
    run: impl FnOnce() -> Result<T, String>,
    attempt: impl for<'a> Fn(&'a T) -> &'a TaskResearchPlanLaneAttemptV1,
) -> Result<T, String> {
    progress(CombatResearchProgressEventV1::LaneStarted { lane });
    let started = Instant::now();
    let result = run();
    match &result {
        Ok(result) => progress(CombatResearchProgressEventV1::LaneCompleted {
            lane,
            elapsed: started.elapsed(),
            attempt: attempt(result).clone(),
        }),
        Err(_) => progress(CombatResearchProgressEventV1::LaneFailed {
            lane,
            elapsed: started.elapsed(),
        }),
    }
    result
}

#[derive(Default)]
struct ResearchPlanPortfolioV1 {
    candidates: Vec<TaskResearchPlanCandidateV1>,
    witness_input_lines: Vec<Vec<ClientInput>>,
    lanes: Vec<TaskResearchPlanLaneAttemptV1>,
    milestone_candidates: Vec<CombatMilestoneResearchMaterializationV1>,
    bounded_progress: Vec<CombatResearchBoundedProgressMaterializationV1>,
}

impl ResearchPlanPortfolioV1 {
    fn push(&mut self, lane: ResearchLaneResultV1) -> Result<(), String> {
        if lane.candidates.len() != lane.witness_input_lines.len() {
            return Err(
                "combat research lane candidate/input materializations disagree".to_string(),
            );
        }
        for (candidate, exact_inputs) in lane.candidates.into_iter().zip(lane.witness_input_lines) {
            if let Some(existing_ordinal) = self
                .candidates
                .iter()
                .position(|existing| existing.plan == candidate.plan)
            {
                let existing = &mut self.candidates[existing_ordinal];
                if existing.outcome != candidate.outcome
                    || existing.satisfies_requested_quality != candidate.satisfies_requested_quality
                    || self.witness_input_lines[existing_ordinal] != exact_inputs
                {
                    return Err(
                        "combat research producers replayed one execution to conflicting evidence"
                            .to_string(),
                    );
                }
                existing
                    .supporting_provenances
                    .extend(candidate.supporting_provenances);
                existing.supporting_provenances.sort();
                existing.supporting_provenances.dedup();
            } else {
                self.candidates.push(candidate);
                self.witness_input_lines.push(exact_inputs);
            }
        }
        self.lanes.push(lane.attempt);
        Ok(())
    }

    fn finish(self) -> CombatResearchPortfolioV1 {
        let Self {
            candidates,
            witness_input_lines,
            lanes,
            milestone_candidates,
            bounded_progress,
        } = self;
        // Bounded progress exists to explain a portfolio-level terminal miss.
        // Once any producer supplies a terminal witness, the verified terminal
        // frontier is the stronger contract and the earlier miss receipt must
        // not survive beside it as if the result were still Unknown.
        let bounded_progress = if candidates.is_empty() {
            bounded_progress
        } else {
            Vec::new()
        };
        CombatResearchPortfolioV1 {
            attempt: TaskResearchPlanAttemptV1 { candidates, lanes },
            witness_input_lines,
            witness_replays: vec![],
            milestone_candidates,
            bounded_progress,
        }
    }
}

fn materialize_milestone_candidate_v1(
    env: &LearningEnvV3,
    root: &CombatPosition,
    path: &LocalTurnGraphPlanTransitionPathSnapshot,
    max_engine_steps_per_transition: usize,
    producer_name: &'static str,
) -> Result<Option<CombatMilestoneResearchMaterializationV1>, String> {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut inputs = Vec::with_capacity(path.root_actions.len());
    if path.parent_action_count >= path.root_actions.len() {
        return Err("resident milestone path has an empty annotated edge".to_string());
    }
    let mut edge_parent = None;
    for (action_index, action) in path.root_actions.iter().enumerate() {
        if action_index == path.parent_action_count {
            edge_parent = Some(position.clone());
        }
        if stepper
            .choice_for_legal_input(&position, &action.input)
            .is_none()
        {
            return Err(format!(
                "resident milestone trace contains an illegal exact action at index {action_index}"
            ));
        }
        let before = position;
        let step = stepper.apply_to_stable(
            &before,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "resident milestone trace did not reach a stable successor at index {action_index}"
            ));
        }
        inputs.push(action.input.clone());
        if step.terminal != CombatTerminal::Unresolved {
            return Ok(None);
        }
        position = step.position;
    }
    let transition = combat_plan_transition_annotation_v1(
        &edge_parent.ok_or_else(|| {
            "resident milestone path did not reach its annotated parent".to_string()
        })?,
        &position,
    )
    .ok_or_else(|| "resident milestone path lost its typed graph annotation".to_string())?;
    if transition != path.plan_transition_annotation {
        return Err(
            "resident milestone path replay disagrees with its typed graph annotation".to_string(),
        );
    }
    let milestones = transition.completed_milestones();
    if milestones.is_empty() || milestones.contains(&CombatPlanMilestoneV1::EncounterDefeated) {
        return Ok(None);
    }
    let provenance = research_plan_provenance_v1(producer_name);
    let (plan, source_decisions, successor_decision) =
        exact_inputs_to_public_turn_plan_materialization_v1(env, &inputs, provenance.clone())?;
    Ok(Some(CombatMilestoneResearchMaterializationV1 {
        candidate: CombatMilestoneResearchCandidateV1 {
            plan,
            provenance,
            transition,
            successor_plan: combat_plan_projection_v1(&position).ok_or_else(|| {
                "resident milestone successor lost its encounter-owned combat plan".to_string()
            })?,
            final_player_hp: position.combat.entities.player.current_hp,
        },
        source_decisions,
        successor_decision,
    }))
}

fn resident_milestone_candidates_v1(
    env: &LearningEnvV3,
    root: &CombatPosition,
    paths: &[LocalTurnGraphPlanTransitionPathSnapshot],
    max_engine_steps_per_transition: usize,
) -> Result<Vec<CombatMilestoneResearchMaterializationV1>, String> {
    let mut candidates: Vec<CombatMilestoneResearchMaterializationV1> = Vec::new();
    let mut seen_successors = std::collections::BTreeSet::new();
    for path in paths {
        if !seen_successors.insert(path.successor_exact_state_hash.as_str()) {
            continue;
        }
        let Some(candidate) = materialize_milestone_candidate_v1(
            env,
            root,
            path,
            max_engine_steps_per_transition,
            RESIDENT_COMBAT_MILESTONE_PLAN_PRODUCER_V1,
        )?
        else {
            continue;
        };
        if !candidates.iter().any(|existing| {
            existing.candidate.plan.steps == candidate.candidate.plan.steps
                && existing.candidate.transition == candidate.candidate.transition
        }) {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

fn resource_conservation_challenger_potion_budget_v1(
    resident: Option<&CombatWitnessMaterializedResourcesV1>,
    conservation_challenger_configured: bool,
) -> Option<u32> {
    conservation_challenger_configured
        .then(|| resident?.total_potion_expenditures.checked_sub(1))
        .flatten()
}

fn next_challenger_potion_budget_v1(
    prior_budget: Option<u32>,
    observed_minimum_potion_expenditures: Option<u32>,
) -> Option<u32> {
    observed_minimum_potion_expenditures
        .map(|expenditures| expenditures.saturating_sub(1))
        .or(prior_budget)
}

fn resident_witness_closes_research_portfolio_v1(
    resident_has_witness: bool,
    resident_ends_quality_refinement: bool,
    complete_turn_proposal_beam_configured: bool,
    atomic_challenger_configured: bool,
    conservation_challenge_open: bool,
) -> bool {
    resident_has_witness
        && ((!complete_turn_proposal_beam_configured && !atomic_challenger_configured)
            || (resident_ends_quality_refinement && !conservation_challenge_open))
}

fn research_plan_provenance_v1(producer_name: &str) -> TaskPrivilegedSupportProvenanceV1 {
    TaskPrivilegedSupportProvenanceV1 {
        access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
        producer_name: producer_name.to_string(),
        producer_revision: if producer_name == WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1 {
            1
        } else {
            COMBAT_RESEARCH_PLAN_PRODUCER_REVISION_V3
        },
    }
}

fn exact_inputs_to_public_replay_plan_v1(
    root_env: &LearningEnvV3,
    inputs: &[ClientInput],
    provenance: TaskPrivilegedSupportProvenanceV1,
) -> Result<(TaskResearchPlanV1, CombatReplaySuccessorResourcesV1), String> {
    exact_inputs_to_public_replay_plan_inner_v1(root_env, inputs, provenance, true)
}

/// Replays one exact terminal combat line through the full run environment and
/// returns resources at its next public boundary. This is the shared owner for
/// mandatory handoffs such as the second A20 Act 3 boss and Pantograph; fixed-
/// root diagnostics must not infer those effects from combat-terminal HP.
pub(super) fn exact_inputs_to_public_replay_outcome_for_run_play_v1(
    root_env: &LearningEnvV3,
    inputs: &[ClientInput],
    provenance: TaskPrivilegedSupportProvenanceV1,
) -> Result<TaskResearchPlanOutcomeV1, String> {
    exact_inputs_to_public_replay_plan_and_outcome_for_run_play_v1(root_env, inputs, provenance)
        .map(|(_, outcome)| outcome)
}

pub(super) fn exact_inputs_to_public_replay_plan_and_outcome_for_run_play_v1(
    root_env: &LearningEnvV3,
    inputs: &[ClientInput],
    provenance: TaskPrivilegedSupportProvenanceV1,
) -> Result<(TaskResearchPlanV1, TaskResearchPlanOutcomeV1), String> {
    let root = root_env.session().current_active_combat_position()?;
    let resources = combat_witness_materialized_resources_v1(&root, inputs, 250)?;
    let (plan, successor) = exact_inputs_to_public_replay_plan_v1(root_env, inputs, provenance)?;
    let outcome = public_research_outcome_at_successor_v1(&root, &resources, &successor)?;
    Ok((plan, outcome))
}

struct CombatReplaySuccessorResourcesV1 {
    current_hp: i32,
    max_hp: i32,
    gold: i32,
    relics: Vec<crate::content::relics::RelicState>,
    pending_rewards: Option<Vec<TaskResearchPlanPendingRewardV1>>,
    replay: CombatResearchReplayMaterializationV1,
}

fn public_research_outcome_at_successor_v1(
    root: &CombatPosition,
    resources: &CombatWitnessMaterializedResourcesV1,
    successor: &CombatReplaySuccessorResourcesV1,
) -> Result<TaskResearchPlanOutcomeV1, String> {
    let mut outcome = resources.to_public_outcome_v1(root)?;
    outcome.final_hp = successor.current_hp;
    outcome.final_gold = successor.gold;
    outcome.persistent.max_hp = successor.max_hp;
    outcome.final_relics = successor.relics.clone();
    if let Some(pending_rewards) = &successor.pending_rewards {
        outcome.pending_rewards = pending_rewards.clone();
    }
    Ok(outcome)
}

pub(super) fn exact_inputs_to_public_turn_plan_v1(
    root_env: &LearningEnvV3,
    inputs: &[ClientInput],
    provenance: TaskPrivilegedSupportProvenanceV1,
) -> Result<TaskResearchPlanV1, String> {
    exact_inputs_to_public_replay_plan_inner_v1(root_env, inputs, provenance, false)
        .map(|(plan, _)| plan)
}

fn exact_inputs_to_public_turn_plan_materialization_v1(
    root_env: &LearningEnvV3,
    inputs: &[ClientInput],
    provenance: TaskPrivilegedSupportProvenanceV1,
) -> Result<
    (
        TaskResearchPlanV1,
        Vec<TaskPublicDecisionV1>,
        TaskPublicDecisionV1,
    ),
    String,
> {
    let (plan, successor) =
        exact_inputs_to_public_replay_plan_inner_v1(root_env, inputs, provenance, false)?;
    let successor_decision = successor.replay.successor_decision.ok_or_else(|| {
        "combat milestone public replay did not end at a public combat decision".to_string()
    })?;
    if successor_decision.site != crate::agent::task::TaskDecisionSiteV1::Combat {
        return Err("combat milestone public replay exited combat".to_string());
    }
    let source_decisions = successor
        .replay
        .steps
        .into_iter()
        .map(|step| step.source_decision)
        .collect();
    Ok((plan, source_decisions, successor_decision))
}

fn exact_inputs_to_public_replay_plan_inner_v1(
    root_env: &LearningEnvV3,
    inputs: &[ClientInput],
    provenance: TaskPrivilegedSupportProvenanceV1,
    require_combat_exit: bool,
) -> Result<(TaskResearchPlanV1, CombatReplaySuccessorResourcesV1), String> {
    if inputs.is_empty() {
        return Err("combat research witness contains no exact actions".to_string());
    }
    let started = Instant::now();
    let root = task_public_decision_v1(root_env)?;
    let mut witness_replay = root_env.clone();
    let mut public_replay = root_env.clone();
    let root_combat_sequence = witness_replay.register_active_combat_sequence_v1()?;
    let public_root_combat_sequence = public_replay.register_active_combat_sequence_v1()?;
    if public_root_combat_sequence != root_combat_sequence {
        return Err(format!(
            "combat research replay roots registered different combat sequences: witness={} public={}",
            root_combat_sequence, public_root_combat_sequence,
        ));
    }
    let mut steps = Vec::with_capacity(inputs.len());
    let fork_elapsed = started.elapsed();
    let mut replay_steps = Vec::with_capacity(inputs.len());
    for (step_index, input) in inputs.iter().enumerate() {
        for (replay, sequence) in [
            ("witness", witness_replay.session().combat_sequence()),
            ("public", public_replay.session().combat_sequence()),
        ] {
            if sequence != root_combat_sequence {
                return Err(format!(
                    "combat research {replay} replay continued into another combat before input {step_index}: producer={} root_combat_sequence={} current_combat_sequence={} exact_prefix={:?}",
                    provenance.producer_name,
                    root_combat_sequence,
                    sequence,
                    &inputs[..step_index],
                ));
            }
        }
        let boundary = public_replay.observe()?;
        let LearningBoundaryV1::Combat { boundary } = boundary else {
            return Err("combat research witness continued after combat exit".to_string());
        };
        let decision = task_public_decision_v1(&public_replay)?;
        let witness_decision = task_public_decision_v1(&witness_replay)?;
        if witness_decision.information.snapshot_id != decision.information.snapshot_id {
            return Err(format!(
                "combat witness and public replay diverged before step {step_index}: witness_snapshot={} public_snapshot={} exact_prefix={:?}",
                witness_decision.information.snapshot_id,
                decision.information.snapshot_id,
                &inputs[..step_index],
            ));
        }
        let projection = CombatActionProjectionV1 {
            public: boundary.public_actions,
            private_resolution: boundary.private_resolution,
        };
        let public_choice = public_choice_for_exact_combat_input_v1(&projection, input)?;
        let choice = task_choice_for_public_combat_choice_v1(&projection, public_choice);
        decision.validate_choice(&choice)?;
        let prepared = prepare_task_choice_v1(&public_replay, &choice)?;
        match &prepared {
            LearningPreparedActionV3::CombatInput { input: resolved } if resolved == input => {}
            LearningPreparedActionV3::CombatInput { .. } => {
                return Err("public ordinal plan resolved to another exact combat input".to_string())
            }
            _ => return Err("combat witness resolved outside the combat action domain".to_string()),
        }
        let LearningPreparedActionV3::CombatInput { input: resolved } = &prepared else {
            unreachable!("combat domain checked above")
        };
        debug_assert_eq!(resolved, input);
        steps.push(TaskResearchPlanStepV1 {
            source_public_snapshot_id: decision.information.snapshot_id.clone(),
            choice,
        });
        let witness_prepared = witness_replay.prepare_action(
            crate::eval::run_control::LearningActionV1::CombatInput {
                input: input.clone(),
            },
        )?;
        witness_replay.step_prepared(witness_prepared)?;
        let monster_entities_before = public_replay
            .session()
            .combat_monster_entities_for_run_play_v1();
        let phase = Instant::now();
        let (_, action_result) = public_replay.step_prepared_with_action_result_v1(prepared)?;
        let monster_entities_after = public_replay
            .session()
            .combat_monster_entities_for_run_play_v1();
        replay_steps.push(CombatResearchReplayStepV1 {
            source_decision: decision,
            monster_entities_before,
            monster_entities_after,
            action_result,
            elapsed: phase.elapsed(),
        });
    }
    let witness_successor = witness_replay.observe()?;
    if require_combat_exit {
        if let LearningBoundaryV1::Combat { boundary } = &witness_successor {
            if combat_boundary_still_belongs_to_research_root_v1(
                root_combat_sequence,
                witness_replay.session().combat_sequence(),
            ) {
                return Err(combat_replay_remained_in_combat_error_v1(
                    "witness",
                    &provenance,
                    inputs,
                    boundary,
                    root_combat_sequence,
                    witness_replay.session().combat_sequence(),
                ));
            }
        }
    }
    let successor = public_replay.observe()?;
    if require_combat_exit {
        if let LearningBoundaryV1::Combat { boundary } = &successor {
            if combat_boundary_still_belongs_to_research_root_v1(
                root_combat_sequence,
                public_replay.session().combat_sequence(),
            ) {
                return Err(combat_replay_remained_in_combat_error_v1(
                    "public",
                    &provenance,
                    inputs,
                    boundary,
                    root_combat_sequence,
                    public_replay.session().combat_sequence(),
                ));
            }
        }
    }
    let pending_rewards = match &successor {
        LearningBoundaryV1::Strategic { boundary } => match &boundary.observation.context {
            PlannerDecisionContext::Reward { pending_rewards } => Some(
                pending_rewards
                    .iter()
                    .map(TaskResearchPlanPendingRewardV1::from_planner_descriptor_v1)
                    .collect(),
            ),
            _ => None,
        },
        _ => None,
    };
    let successor_run = &public_replay.session().run_state;
    let successor_decision = matches!(
        successor,
        LearningBoundaryV1::Combat { .. } | LearningBoundaryV1::Strategic { .. }
    )
    .then(|| task_public_decision_v1(&public_replay))
    .transpose()?;
    let terminal_game_outcome = match &successor {
        LearningBoundaryV1::Terminal { outcome } => Some(
            super::task_world_support::game_outcome_for_learning_terminal_v1(
                &public_replay,
                outcome.result.clone(),
            ),
        ),
        _ => None,
    };
    let (current_hp, max_hp, gold, relics) = match &successor {
        LearningBoundaryV1::Combat { boundary } => (
            boundary.observation.player.hp,
            boundary.observation.player.max_hp,
            boundary.observation.player.gold,
            boundary.observation.player.relics.clone(),
        ),
        LearningBoundaryV1::Terminal { outcome } => (
            outcome.terminal_hp,
            outcome.terminal_max_hp,
            outcome.terminal_gold,
            successor_run.relics.clone(),
        ),
        LearningBoundaryV1::Strategic { .. } | LearningBoundaryV1::Unsupported => (
            successor_run.current_hp,
            successor_run.max_hp,
            successor_run.gold,
            successor_run.relics.clone(),
        ),
    };
    let successor_resources = CombatReplaySuccessorResourcesV1 {
        current_hp,
        max_hp,
        gold,
        relics: relics
            .iter()
            .map(crate::content::relics::persistent_state::post_combat_persistent_relic_state_v1)
            .collect(),
        pending_rewards,
        replay: CombatResearchReplayMaterializationV1 {
            steps: replay_steps,
            successor_decision,
            terminal_game_outcome,
            fork_elapsed,
            total_elapsed: started.elapsed(),
        },
    };
    Ok((
        TaskResearchPlanV1 {
            root_public_snapshot_id: root.information.snapshot_id,
            steps,
        },
        successor_resources,
    ))
}

fn combat_boundary_still_belongs_to_research_root_v1(
    root_combat_sequence: u64,
    current_combat_sequence: u64,
) -> bool {
    current_combat_sequence == root_combat_sequence
}

fn combat_replay_remained_in_combat_error_v1(
    replay: &str,
    provenance: &TaskPrivilegedSupportProvenanceV1,
    inputs: &[ClientInput],
    boundary: &super::LearningCombatBoundaryV1,
    root_combat_sequence: u64,
    current_combat_sequence: u64,
) -> String {
    let living_enemies = boundary
        .observation
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.escaped)
        .map(|monster| {
            format!(
                "slot{}:{:?}:hp{}:block{}:dying{}:half_dead{}",
                monster.slot,
                monster.enemy,
                monster.hp,
                monster.block,
                monster.dying,
                monster.half_dead,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "combat research {replay} replay remained in root combat: producer={} root_combat_sequence={} current_combat_sequence={} input_count={} turn={} phase={:?} player_hp={} player_block={} living_enemies=[{}] last_input={:?} exact_inputs={inputs:?}",
        provenance.producer_name,
        root_combat_sequence,
        current_combat_sequence,
        inputs.len(),
        boundary.observation.turn.turn_count,
        boundary.observation.turn.phase,
        boundary.observation.player.hp,
        boundary.observation.player.block,
        living_enemies,
        inputs.last(),
    )
}

fn task_choice_for_public_combat_choice_v1(
    projection: &CombatActionProjectionV1,
    choice: PublicCombatActionChoiceV1,
) -> TaskPublicChoiceV1 {
    match choice {
        PublicCombatActionChoiceV1::Atomic { action_ordinal } => {
            TaskPublicChoiceV1::one(action_ordinal)
        }
        PublicCombatActionChoiceV1::Selection {
            family_ordinal,
            selected_domain_ordinals,
        } => {
            let mut ordinal_path = vec![projection.public.atomic_actions.len() + family_ordinal];
            ordinal_path.extend(selected_domain_ordinals);
            TaskPublicChoiceV1 { ordinal_path }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::TaskPublicCandidateV1;
    use crate::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1;
    use crate::content::cards::CardId;
    use crate::content::monsters::{factory::EncounterId, EnemyId};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;

    #[test]
    fn retained_public_replay_carries_monster_identity_through_collector_summon() {
        let mut session = one_strike_learning_env().session().clone();
        let (engine, combat) = crate::sim::combat_start::build_natural_combat_start(
            &mut session.run_state,
            EncounterId::Collector,
            RoomType::MonsterRoomBoss,
        )
        .unwrap();
        session.engine_state = engine.clone();
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            engine,
            combat,
            EncounterId::Collector,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomBoss,
            }),
        ));
        let env = LearningEnvV3::from_session(session);
        let (_, successor) = exact_inputs_to_public_replay_plan_inner_v1(
            &env,
            &[ClientInput::EndTurn],
            research_plan_provenance_v1(WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1),
            false,
        )
        .unwrap();
        let step = &successor.replay.steps[0];
        assert_eq!(step.monster_entities_before.len(), 1);
        assert_eq!(step.monster_entities_after.len(), 3);
        let boss_id = step.monster_entities_before[0].1;
        assert_eq!(
            step.monster_entities_after
                .iter()
                .find(|(_, id)| *id == boss_id)
                .unwrap()
                .0,
            2
        );
        assert_eq!(
            env.session().combat_monster_entities_for_run_play_v1(),
            step.monster_entities_before
        );
    }

    fn one_strike_learning_env() -> LearningEnvV3 {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        let plan = crate::content::monsters::roll_monster_turn_plan(
            &mut combat.rng.ai_rng,
            &monster,
            combat.meta.ascension_level,
            99,
            std::slice::from_ref(&monster),
            &[],
        );
        monster.set_planned_move_id(plan.move_id);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        monster.current_hp = 6;
        monster.max_hp = 6;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];

        let mut session =
            super::super::RunControlSession::new(super::super::RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            EngineState::CombatPlayerTurn,
            combat,
            EncounterId::JawWorm,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        LearningEnvV3::from_session(session)
    }

    #[test]
    fn incremental_whole_combat_cannot_reset_exhausted_wall_allowance() {
        let options = CombatResearchPlanOptionsV1::default();
        let wall = Duration::from_millis(options.wall_ms);
        let mut research = WholeCombatResearchV1::new(one_strike_learning_env(), options).unwrap();
        research.search_time = wall;
        for target in [1, 100] {
            let result = research.advance_to(target).unwrap();
            assert_eq!(result.attempt.lanes[0].work_consumed, 0);
            assert_eq!(
                result.attempt.lanes[0].exit,
                TaskResearchPlanAttemptExitV1::WallTimeExhausted
            );
            assert!(result.attempt.candidates.is_empty());
            assert_eq!(result.bounded_progress.len(), 1);
            assert!(result.bounded_progress[0].paths.is_empty());
            assert!(result.bounded_progress[0]
                .recent_turn_survival_envelope
                .is_empty());
        }
        assert!(research.advance_to(0).is_err());
    }

    #[test]
    fn whole_combat_miss_reports_actual_depth_even_from_a_later_root() {
        use sts_guided_search::solver::{CombatSolver, SolverConfig, SolverStop};
        let env = one_strike_learning_env();
        let mut position = env.session().current_active_combat_position().unwrap();
        position.combat.turn.turn_count = 5;
        position.combat.zones.hand.clear();
        position.combat.entities.player.current_hp = 80;
        position.combat.entities.player.max_hp = 80;
        let mut solver = CombatSolver::new(position, SolverConfig::default()).unwrap();
        solver.advance(200, Duration::from_secs(2)).unwrap();
        assert!(solver.stats().deepest_turn > 5);
        assert!(solver.plans().is_empty());
        let report = whole_combat_portfolio_from_solver_v1(
            &env,
            &CombatResearchPlanOptionsV1::default(),
            &solver,
            SolverStop::WorkBudget,
        )
        .unwrap();
        let progress = &report.bounded_progress[0];
        assert_eq!(progress.max_player_turn, solver.stats().deepest_turn);
        assert_eq!(
            progress.provenance.producer_name,
            WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1
        );
        assert!(progress.paths.is_empty());
        assert!(progress.recent_turn_survival_envelope.is_empty());
    }

    #[test]
    fn shared_deadline_stops_all_lane_schedules_without_starting_challengers() {
        for (schedule, beam, atomic, expected) in [
            (
                CombatResearchLaneScheduleV1::WholeCombat,
                false,
                false,
                CombatResearchLaneV1::WholeCombat,
            ),
            (
                CombatResearchLaneScheduleV1::ResidentFrontier,
                true,
                true,
                CombatResearchLaneV1::ResidentTurnGraph,
            ),
            (
                CombatResearchLaneScheduleV1::FirstWitness,
                true,
                true,
                CombatResearchLaneV1::CompleteTurnProposalBeam,
            ),
            (
                CombatResearchLaneScheduleV1::FirstWitness,
                false,
                true,
                CombatResearchLaneV1::AtomicChallenger,
            ),
            (
                CombatResearchLaneScheduleV1::FirstWitness,
                false,
                false,
                CombatResearchLaneV1::ResidentTurnGraph,
            ),
        ] {
            let options = CombatResearchPlanOptionsV1 {
                lane_schedule: schedule,
                complete_turn_proposal_beam: beam
                    .then(CompleteTurnProposalBeamResearchOptionsV1::default),
                atomic_challenger: atomic
                    .then(AtomicCombatResearchPlanChallengerOptionsV1::default),
                milestone_closure: Some(CombatMilestoneClosureResearchOptionsV1::default()),
                ..CombatResearchPlanOptionsV1::default()
            };
            let mut started = Vec::new();
            let result = combat_research_portfolio_with_progress_v1(
                &one_strike_learning_env(),
                &options,
                Some(Instant::now() - Duration::from_millis(1)),
                &mut |event| {
                    if let CombatResearchProgressEventV1::LaneStarted { lane } = event {
                        started.push(lane);
                    }
                },
            )
            .unwrap();
            result.attempt.validate().unwrap();
            assert!(started.is_empty());
            assert!(result.attempt.candidates.is_empty());
            assert_eq!(result.attempt.lanes.len(), 1);
            assert_eq!(result.attempt.lanes[0].work_consumed, 0);
            assert_eq!(
                result.attempt.summary_exit(),
                TaskResearchPlanAttemptExitV1::DeadlineReached
            );
            let producer = match expected {
                CombatResearchLaneV1::WholeCombat => WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1,
                CombatResearchLaneV1::ResidentTurnGraph => {
                    RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1
                }
                CombatResearchLaneV1::CompleteTurnProposalBeam => {
                    COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1
                }
                CombatResearchLaneV1::AtomicChallenger => ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1,
                _ => unreachable!(),
            };
            assert_eq!(result.attempt.lanes[0].provenance.producer_name, producer);
            assert_eq!(
                result.attempt.lanes[0].provenance.producer_revision,
                if expected == CombatResearchLaneV1::WholeCombat {
                    1
                } else {
                    COMBAT_RESEARCH_PLAN_PRODUCER_REVISION_V3
                }
            );
            if expected == CombatResearchLaneV1::WholeCombat {
                assert_eq!(
                    result.attempt.lanes[0].work_unit,
                    TaskResearchPlanWorkUnitV1::WholeCombatTransitions
                );
            }
        }
    }

    #[test]
    fn each_lane_uses_the_earlier_of_its_own_limit_and_the_shared_deadline() {
        let shared = Instant::now() + Duration::from_millis(10);
        assert_eq!(lane_deadline(30_000, Some(shared)), shared);
        let later = Instant::now() + Duration::from_secs(30);
        assert!(lane_deadline(1, Some(later)) < later);
        let expired = Instant::now() - Duration::from_millis(1);
        assert_eq!(lane_deadline(30_000, Some(expired)), expired);
        assert!(deadline_reached(Some(expired)));
        assert!(!deadline_reached(None));
    }

    fn two_equivalent_strikes_learning_env() -> LearningEnvV3 {
        let mut env = one_strike_learning_env();
        let mut session = env.clone().into_session();
        let combat = &mut session
            .active_combat
            .as_mut()
            .expect("active combat")
            .combat_state;
        combat.entities.monsters[0].current_hp = 12;
        combat.entities.monsters[0].max_hp = 12;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Strike, 11),
        ];
        env = LearningEnvV3::from_session(session);
        env
    }

    fn durable_jaw_worm_learning_env() -> LearningEnvV3 {
        let env = one_strike_learning_env();
        let mut session = env.into_session();
        let monster = &mut session
            .active_combat
            .as_mut()
            .expect("active combat")
            .combat_state
            .entities
            .monsters[0];
        monster.current_hp = 60;
        monster.max_hp = 60;
        LearningEnvV3::from_session(session)
    }

    fn bronze_beam_closure_learning_env() -> LearningEnvV3 {
        let mut combat = crate::test_support::blank_test_combat();
        combat.meta.ascension_level = 20;
        combat.turn.energy = 3;
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 1)];
        let mut automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        automaton.id = 10;
        automaton.current_hp = 300;
        automaton.max_hp = 300;
        automaton.bronze_automaton.first_turn = false;
        automaton.bronze_automaton.num_turns = 4;
        let beam_plan = <crate::content::monsters::city::bronze_automaton::BronzeAutomaton as crate::content::monsters::MonsterBehavior>::roll_move_plan(
            &mut combat.rng.ai_rng,
            &automaton,
            combat.meta.ascension_level,
            0,
        );
        automaton.bronze_automaton.num_turns = 0;
        automaton.set_planned_move_id(beam_plan.move_id);
        automaton.set_planned_steps(beam_plan.steps);
        automaton.set_planned_visible_spec(beam_plan.visible_spec);
        automaton.move_history_mut().push_back(beam_plan.move_id);
        combat.entities.monsters = vec![automaton];

        let mut session =
            super::super::RunControlSession::new(super::super::RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            EngineState::CombatPlayerTurn,
            combat,
            EncounterId::Automaton,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomBoss,
            }),
        ));
        LearningEnvV3::from_session(session)
    }

    fn materialized_resources(
        final_hp: i32,
        starting_potion_expenditure_slots: u64,
    ) -> CombatWitnessMaterializedResourcesV1 {
        let combat = crate::test_support::blank_test_combat();
        CombatWitnessMaterializedResourcesV1 {
            final_hp,
            final_gold: 0,
            persistent: CombatPersistentOutcomeV1::from_combat(&combat),
            starting_potion_expenditure_slots,
            total_potion_expenditures: starting_potion_expenditure_slots.count_ones(),
            generated_potions: Vec::new(),
            final_relics: Vec::new(),
            meta_changes: Vec::new(),
            pending_rewards: Vec::new(),
            combat_mugged: false,
        }
    }

    #[test]
    fn resident_witness_becomes_a_replay_verified_public_ordinal_plan() {
        let env = one_strike_learning_env();
        let attempt = combat_research_plan_v1(
            &env,
            &CombatResearchPlanOptionsV1 {
                max_generation_work: 64,
                wall_ms: 1_000,
                quantum_generation_work: 32,
                quantum_wall_ms: 250,
                potion_policy: CombatWitnessPotionPolicyV1::Never,
                satisfaction: CombatWitnessSatisfactionV1::FirstCompleteWin,
                ..CombatResearchPlanOptionsV1::default()
            },
        )
        .expect("resident witness search");
        assert_eq!(
            attempt.summary_exit(),
            TaskResearchPlanAttemptExitV1::Witness
        );
        assert_eq!(attempt.lanes.len(), 1);
        assert_eq!(
            attempt.lanes[0].work_unit,
            TaskResearchPlanWorkUnitV1::TurnGraphGenerationWork
        );
        let candidate = attempt.candidates.first().expect("one-Strike candidate");
        let plan = &candidate.plan;

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(
            candidate.supporting_provenances[0].access,
            TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness
        );
        let root = task_public_decision_v1(&env).expect("public combat root");
        let selected = &root.candidates[plan.steps[0].choice.ordinal_path[0]];
        assert!(matches!(
            selected,
            TaskPublicCandidateV1::CombatAtomic {
                action: crate::agent::information::action::PublicCombatAtomicActionV1::PlayCard {
                    hand_index: 0,
                    target_monster_index: Some(0),
                    ..
                }
            }
        ));
    }

    #[test]
    fn bounded_resident_miss_retains_replayable_neutral_progress() {
        let env = durable_jaw_worm_learning_env();
        let portfolio = combat_research_portfolio_v1(
            &env,
            &CombatResearchPlanOptionsV1 {
                max_generation_work: 64,
                wall_ms: 1_000,
                quantum_generation_work: 32,
                quantum_wall_ms: 250,
                potion_policy: CombatWitnessPotionPolicyV1::Never,
                satisfaction: CombatWitnessSatisfactionV1::FirstCompleteWin,
                complete_turn_proposal_beam: None,
                atomic_challenger: None,
                ..CombatResearchPlanOptionsV1::default()
            },
        )
        .expect("bounded resident miss");

        assert!(portfolio.attempt.candidates.is_empty());
        let progress = portfolio
            .bounded_progress
            .first()
            .expect("resident progress receipt");
        assert_eq!(
            progress.provenance.producer_name,
            RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1
        );
        assert!(!progress.recent_turn_survival_envelope.is_empty());
        assert!(!progress.paths.is_empty());
        for path in &progress.paths {
            assert!(!path.candidate.roles.is_empty());
            assert_eq!(
                path.candidate.plan.steps.len(),
                path.candidate.endpoint.path_atomic_depth
            );
            assert!(path.candidate.endpoint.player_turn <= progress.max_player_turn);
        }
    }

    #[test]
    fn first_witness_schedule_services_proposal_before_resident() {
        let env = one_strike_learning_env();
        let attempt = combat_research_plan_v1(
            &env,
            &CombatResearchPlanOptionsV1 {
                lane_schedule: CombatResearchLaneScheduleV1::FirstWitness,
                satisfaction: CombatWitnessSatisfactionV1::FirstCompleteWin,
                potion_policy: CombatWitnessPotionPolicyV1::Never,
                complete_turn_proposal_beam: Some(
                    CompleteTurnProposalBeamResearchOptionsV1::default(),
                ),
                atomic_challenger: None,
                ..CombatResearchPlanOptionsV1::default()
            },
        )
        .expect("first-witness proposal search");

        assert_eq!(
            attempt.summary_exit(),
            TaskResearchPlanAttemptExitV1::Witness
        );
        assert_eq!(attempt.lanes.len(), 1);
        assert_eq!(
            attempt.lanes[0].provenance.producer_name,
            COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1
        );
        assert_eq!(
            attempt.lanes[0].work_unit,
            TaskResearchPlanWorkUnitV1::CompleteTurnProposalExpandedBoundaries
        );
    }

    #[test]
    fn first_witness_schedule_services_atomic_before_resident_without_proposal() {
        let env = one_strike_learning_env();
        let attempt = combat_research_plan_v1(
            &env,
            &CombatResearchPlanOptionsV1 {
                lane_schedule: CombatResearchLaneScheduleV1::FirstWitness,
                satisfaction: CombatWitnessSatisfactionV1::FirstCompleteWin,
                potion_policy: CombatWitnessPotionPolicyV1::Never,
                complete_turn_proposal_beam: None,
                atomic_challenger: Some(AtomicCombatResearchPlanChallengerOptionsV1::default()),
                ..CombatResearchPlanOptionsV1::default()
            },
        )
        .expect("first-witness atomic search");

        assert_eq!(
            attempt.summary_exit(),
            TaskResearchPlanAttemptExitV1::Witness
        );
        assert_eq!(attempt.lanes.len(), 1);
        assert_eq!(
            attempt.lanes[0].provenance.producer_name,
            ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1
        );
        assert_eq!(
            attempt.lanes[0].work_unit,
            TaskResearchPlanWorkUnitV1::AtomicExpandedNodes
        );
    }

    fn portfolio_lane_v1(candidate: TaskResearchPlanCandidateV1) -> ResearchLaneResultV1 {
        let provenance = candidate.supporting_provenances[0].clone();
        ResearchLaneResultV1 {
            attempt: TaskResearchPlanLaneAttemptV1 {
                provenance,
                work_unit: TaskResearchPlanWorkUnitV1::AtomicExpandedNodes,
                work_consumed: 1,
                exit: TaskResearchPlanAttemptExitV1::Witness,
            },
            candidates: vec![candidate],
            witness_input_lines: vec![vec![ClientInput::EndTurn]],
            representative_resources: None,
        }
    }

    fn one_strike_candidate_v1() -> TaskResearchPlanCandidateV1 {
        let env = one_strike_learning_env();
        combat_research_plan_v1(
            &env,
            &CombatResearchPlanOptionsV1 {
                max_generation_work: 64,
                wall_ms: 1_000,
                quantum_generation_work: 32,
                quantum_wall_ms: 250,
                potion_policy: CombatWitnessPotionPolicyV1::Never,
                satisfaction: CombatWitnessSatisfactionV1::FirstCompleteWin,
                complete_turn_proposal_beam: None,
                atomic_challenger: None,
                ..CombatResearchPlanOptionsV1::default()
            },
        )
        .expect("one-strike research")
        .candidates
        .into_iter()
        .next()
        .expect("one-strike candidate")
    }

    #[test]
    fn portfolio_merges_identical_execution_evidence_across_producers() {
        let resident = one_strike_candidate_v1();
        let mut proposal = resident.clone();
        proposal.supporting_provenances = vec![research_plan_provenance_v1(
            COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1,
        )];
        let mut portfolio = ResearchPlanPortfolioV1::default();

        portfolio
            .push(portfolio_lane_v1(resident))
            .expect("resident lane");
        portfolio
            .push(portfolio_lane_v1(proposal))
            .expect("proposal lane");
        let attempt = portfolio.finish().attempt;

        assert_eq!(attempt.candidates.len(), 1);
        assert_eq!(attempt.candidates[0].supporting_provenances.len(), 2);
        assert_eq!(attempt.lanes.len(), 2);
        attempt.validate().expect("merged typed attempt");
    }

    #[test]
    fn portfolio_drops_bounded_miss_progress_after_any_terminal_witness() {
        let mut portfolio = ResearchPlanPortfolioV1::default();
        portfolio
            .bounded_progress
            .push(CombatResearchBoundedProgressMaterializationV1 {
                provenance: research_plan_provenance_v1(RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1),
                max_player_turn: 3,
                recent_turn_survival_envelope: Vec::new(),
                paths: Vec::new(),
            });
        portfolio
            .push(portfolio_lane_v1(one_strike_candidate_v1()))
            .expect("terminal witness lane");

        let finished = portfolio.finish();
        assert_eq!(finished.attempt.candidates.len(), 1);
        assert!(finished.bounded_progress.is_empty());
    }

    #[test]
    fn portfolio_rejects_conflicting_evidence_for_one_execution() {
        let resident = one_strike_candidate_v1();
        let mut conflicting = resident.clone();
        conflicting.outcome.final_hp += 1;
        conflicting.supporting_provenances = vec![research_plan_provenance_v1(
            COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1,
        )];
        let mut portfolio = ResearchPlanPortfolioV1::default();

        portfolio
            .push(portfolio_lane_v1(resident))
            .expect("resident lane");
        assert_eq!(
            portfolio.push(portfolio_lane_v1(conflicting)),
            Err(
                "combat research producers replayed one execution to conflicting evidence"
                    .to_string()
            )
        );

        let resident = one_strike_candidate_v1();
        let mut conflicting_quality = resident.clone();
        conflicting_quality.satisfies_requested_quality =
            !conflicting_quality.satisfies_requested_quality;
        conflicting_quality.supporting_provenances = vec![research_plan_provenance_v1(
            COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1,
        )];
        let mut portfolio = ResearchPlanPortfolioV1::default();
        portfolio
            .push(portfolio_lane_v1(resident))
            .expect("resident lane");
        assert!(portfolio
            .push(portfolio_lane_v1(conflicting_quality))
            .is_err());
    }

    #[test]
    fn portfolio_keeps_different_executions_with_equal_outcomes() {
        let resident = one_strike_candidate_v1();
        let mut alternate = resident.clone();
        let resident_ordinal = resident.plan.steps[0].choice.ordinal_path[0];
        alternate.plan.steps[0].choice =
            TaskPublicChoiceV1::one(if resident_ordinal == 0 { 1 } else { 0 });
        assert_ne!(resident.plan, alternate.plan);
        alternate.supporting_provenances = vec![research_plan_provenance_v1(
            COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1,
        )];
        let mut portfolio = ResearchPlanPortfolioV1::default();

        portfolio
            .push(portfolio_lane_v1(resident))
            .expect("resident lane");
        portfolio
            .push(portfolio_lane_v1(alternate))
            .expect("proposal lane");

        assert_eq!(portfolio.finish().attempt.candidates.len(), 2);
    }

    #[test]
    fn milestone_closure_is_bounded_and_remains_an_unresolved_candidate() {
        let env = bronze_beam_closure_learning_env();
        let root = CombatPosition::new(
            env.session().engine_state.clone(),
            env.session()
                .active_combat
                .as_ref()
                .expect("active combat")
                .combat_state
                .clone(),
        );
        let plan = combat_plan_projection_v1(&root).expect("Bronze beam plan");
        assert_eq!(
            plan.stage,
            sts_combat_strategy::CombatPlanStageV1::SurviveForcedAttackWindow
        );
        let path = LocalTurnGraphMilestoneClosurePathSnapshot {
            exact_state_hash: crate::ai::combat_state_key::combat_exact_state_hash_v2(
                &root.engine,
                &root.combat,
            ),
            root_actions: Vec::new(),
            path_negative_log_policy: 0.0,
            target_milestone: CombatPlanMilestoneV1::ForcedAttackWindowSurvived,
            plan,
            visits: 0,
            generation_work: 0,
        };
        let options = CombatResearchPlanOptionsV1 {
            max_engine_steps_per_transition: 1_000,
            potion_policy: CombatWitnessPotionPolicyV1::Never,
            max_potions_used: Some(0),
            ..CombatResearchPlanOptionsV1::default()
        };
        let closure_options = CombatMilestoneClosureResearchOptionsV1 {
            max_parent_states: 1,
            generation_work_per_parent: 64,
            max_generation_work: 64,
            max_candidates: 1,
            wall_ms: 2_000,
        };

        let lane =
            run_milestone_closure_lane_v1(&env, &options, &closure_options, &root, &[path], None)
                .expect("bounded milestone closure");

        assert_eq!(
            lane.attempt.work_unit,
            TaskResearchPlanWorkUnitV1::MilestoneClosureGenerationWork
        );
        assert!(lane.attempt.work_consumed <= closure_options.max_generation_work as u64);
        assert_eq!(
            lane.attempt.exit,
            TaskResearchPlanAttemptExitV1::MilestoneCandidates
        );
        assert!(lane.candidates.len() <= closure_options.max_candidates);
        let candidate = lane.candidates.first().expect("one unresolved milestone");
        assert!(candidate
            .candidate
            .transition
            .completed_milestones()
            .contains(&CombatPlanMilestoneV1::ForcedAttackWindowSurvived));
        assert_eq!(
            candidate.candidate.successor_plan.stage,
            sts_combat_strategy::CombatPlanStageV1::ExploitForcedAttackRecoveryWindow
        );
        assert_eq!(
            candidate.candidate.provenance.producer_name,
            MILESTONE_CLOSURE_RESEARCH_PLAN_PRODUCER_V1
        );
    }

    #[test]
    fn atomic_challenger_rescues_a_resident_miss_and_keeps_public_replay() {
        let env = two_equivalent_strikes_learning_env();
        let attempt = combat_research_plan_v1(
            &env,
            &CombatResearchPlanOptionsV1 {
                max_generation_work: 1,
                wall_ms: 1_000,
                quantum_generation_work: 1,
                quantum_wall_ms: 250,
                satisfaction: CombatWitnessSatisfactionV1::FirstCompleteWin,
                potion_policy: CombatWitnessPotionPolicyV1::Never,
                max_potions_used: Some(0),
                complete_turn_proposal_beam: None,
                atomic_challenger: Some(AtomicCombatResearchPlanChallengerOptionsV1 {
                    max_nodes: 64,
                    wall_ms: 1_000,
                }),
                ..CombatResearchPlanOptionsV1::default()
            },
        )
        .expect("atomic challenger research plan");

        assert_eq!(
            attempt.summary_exit(),
            TaskResearchPlanAttemptExitV1::Witness
        );
        assert_eq!(attempt.lanes.len(), 2);
        assert_ne!(
            attempt.lanes[0].exit,
            TaskResearchPlanAttemptExitV1::Witness
        );
        assert_eq!(
            attempt.lanes[1].work_unit,
            TaskResearchPlanWorkUnitV1::AtomicExpandedNodes
        );
        let candidate = attempt
            .candidates
            .first()
            .expect("challenger witness candidate");
        let plan = &candidate.plan;
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(
            candidate.supporting_provenances[0].producer_name,
            ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1
        );
    }

    #[test]
    fn atomic_lane_publishes_both_replay_exact_potion_reserves() {
        use crate::content::potions::{Potion, PotionId};

        let mut session = one_strike_learning_env().into_session();
        let active = session.active_combat.as_mut().expect("active combat");
        active.combat_state.zones.hand.clear();
        active.combat_state.entities.potions = vec![
            Some(Potion::new(PotionId::FirePotion, 101)),
            Some(Potion::new(PotionId::ExplosivePotion, 102)),
        ];
        let root = CombatPosition::new(active.engine_state.clone(), active.combat_state.clone());
        let env = LearningEnvV3::from_session(session);
        let lane = run_atomic_research_lane_v1(
            &env,
            &CombatResearchPlanOptionsV1 {
                satisfaction: CombatWitnessSatisfactionV1::BudgetOrExhaustion,
                potion_policy: CombatWitnessPotionPolicyV1::All,
                max_potions_used: Some(1),
                ..CombatResearchPlanOptionsV1::default()
            },
            &AtomicCombatResearchPlanChallengerOptionsV1 {
                max_nodes: 32,
                wall_ms: 1_000,
            },
            &root,
            Some(1),
            None,
        )
        .expect("atomic lane must materialize all observed resource choices");
        let mut portfolio = ResearchPlanPortfolioV1::default();
        portfolio
            .push(lane)
            .expect("publish each exact input line with its public plan");
        let portfolio = portfolio.finish();
        let mut spent = portfolio
            .attempt
            .candidates
            .iter()
            .map(|candidate| {
                assert_eq!(candidate.plan.steps.len(), 1);
                assert_eq!(
                    candidate.outcome.final_hp,
                    root.combat.entities.player.current_hp
                );
                candidate.outcome.consumed_entry_potion_slots.clone()
            })
            .collect::<Vec<_>>();
        spent.sort();
        assert_eq!(spent, [vec![0], vec![1]]);
        assert_eq!(portfolio.witness_input_lines.len(), 2);
        assert_eq!(
            env.session().active_combat.as_ref().unwrap().combat_state,
            root.combat
        );
    }

    #[test]
    fn duplicate_basic_occurrences_stay_aligned_across_a_multi_step_public_plan() {
        let env = two_equivalent_strikes_learning_env();
        let target = env
            .session()
            .active_combat
            .as_ref()
            .expect("active combat")
            .combat_state
            .entities
            .monsters[0]
            .id;
        let (plan, successor) = exact_inputs_to_public_replay_plan_v1(
            &env,
            &[
                ClientInput::PlayCard {
                    card_index: 1,
                    target: Some(target),
                },
                ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(target),
                },
            ],
            research_plan_provenance_v1(RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1),
        )
        .expect("canonical public replay keeps duplicate occurrences aligned");

        assert_eq!(plan.steps.len(), 2);
        assert!(successor.pending_rewards.is_some());
        assert!(plan
            .steps
            .iter()
            .all(|step| step.choice.ordinal_path.len() == 1));
    }

    #[test]
    fn incomplete_witness_reports_producer_and_exact_resume_state() {
        let env = two_equivalent_strikes_learning_env();
        let target = env
            .session()
            .active_combat
            .as_ref()
            .expect("active combat")
            .combat_state
            .entities
            .monsters[0]
            .id;
        let error = match exact_inputs_to_public_replay_plan_v1(
            &env,
            &[ClientInput::PlayCard {
                card_index: 1,
                target: Some(target),
            }],
            research_plan_provenance_v1(RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1),
        ) {
            Ok(_) => panic!("one strike is not a complete combat witness"),
            Err(error) => error,
        };

        assert!(error.contains("witness replay remained in root combat"));
        assert!(error.contains(RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1));
        assert!(error.contains("root_combat_sequence="));
        assert!(error.contains("current_combat_sequence="));
        assert!(error.contains("input_count=1"));
        assert!(error.contains("living_enemies=[slot0:"));
        assert!(error.contains("last_input=Some(PlayCard"));
    }

    #[test]
    fn consecutive_boss_combat_sequence_is_a_valid_current_encounter_exit() {
        let time_eater_sequence = 50;

        assert!(combat_boundary_still_belongs_to_research_root_v1(
            time_eater_sequence,
            time_eater_sequence,
        ));
        assert!(!combat_boundary_still_belongs_to_research_root_v1(
            time_eater_sequence,
            time_eater_sequence + 1,
        ));
    }

    #[test]
    fn materialized_resource_dominance_does_not_price_hp_against_potions() {
        let incumbent = materialized_resources(10, 0b01);
        let healthier_and_conserving = materialized_resources(12, 0);
        let healthier_but_spending_more = materialized_resources(12, 0b11);

        assert!(healthier_and_conserving.strictly_dominates(&incumbent));
        assert!(!healthier_but_spending_more.strictly_dominates(&incumbent));
        assert!(!incumbent.strictly_dominates(&incumbent));
    }

    #[test]
    fn potion_spending_resident_keeps_a_lower_budget_challenger_open() {
        let one_potion = materialized_resources(74, 0b01);
        let two_potions = materialized_resources(74, 0b11);
        let conserving = materialized_resources(70, 0);

        assert_eq!(
            resource_conservation_challenger_potion_budget_v1(Some(&one_potion), true),
            Some(0)
        );
        assert_eq!(
            resource_conservation_challenger_potion_budget_v1(Some(&two_potions), true),
            Some(1)
        );
        assert_eq!(
            resource_conservation_challenger_potion_budget_v1(Some(&conserving), true),
            None
        );
        assert_eq!(
            resource_conservation_challenger_potion_budget_v1(Some(&one_potion), false),
            None
        );
    }

    #[test]
    fn each_successful_lane_advances_the_next_challenger_toward_potion_free() {
        assert_eq!(next_challenger_potion_budget_v1(Some(1), Some(1)), Some(0));
        assert_eq!(next_challenger_potion_budget_v1(None, Some(2)), Some(1));
        assert_eq!(next_challenger_potion_budget_v1(Some(1), None), Some(1));
        assert_eq!(next_challenger_potion_budget_v1(Some(1), Some(0)), Some(0));
    }

    #[test]
    fn low_quality_resident_witness_does_not_skip_an_enabled_proposal_beam() {
        assert!(!resident_witness_closes_research_portfolio_v1(
            true, false, true, false, false,
        ));
    }

    #[test]
    fn refinement_ending_resident_without_resource_challenge_closes_portfolio() {
        assert!(resident_witness_closes_research_portfolio_v1(
            true, true, true, false, false,
        ));
    }

    #[test]
    fn open_resource_challenge_keeps_portfolio_open_after_refinement_quality() {
        assert!(!resident_witness_closes_research_portfolio_v1(
            true, true, true, true, true,
        ));
    }

    #[test]
    fn sole_resident_lane_can_finish_with_any_witness() {
        assert!(resident_witness_closes_research_portfolio_v1(
            true, false, false, false, false,
        ));
        assert!(!resident_witness_closes_research_portfolio_v1(
            false, true, false, false, false,
        ));
    }

    #[test]
    fn requested_quality_is_only_a_fact_for_resource_incomparability() {
        let mut root = crate::sim::combat::CombatPosition::new(
            EngineState::CombatPlayerTurn,
            crate::test_support::blank_test_combat(),
        );
        root.combat.entities.player.current_hp = 74;
        root.combat.entities.player.max_hp = 74;
        let conserving_but_high_loss = materialized_resources(31, 0);
        let zero_loss_with_one_potion = materialized_resources(74, 0b01);

        assert!(!conserving_but_high_loss
            .satisfies(&root, CombatWitnessSatisfactionV1::ZeroLossOrBudget));
        assert!(zero_loss_with_one_potion
            .satisfies(&root, CombatWitnessSatisfactionV1::ZeroLossOrBudget));
        assert!(!zero_loss_with_one_potion.strictly_dominates(&conserving_but_high_loss));
    }
}
