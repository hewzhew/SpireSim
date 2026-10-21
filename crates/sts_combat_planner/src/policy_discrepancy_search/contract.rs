use std::time::Instant;

use sts_core::sim::combat::CombatTerminal;
use sts_core::state::core::ClientInput;

use crate::atomic_witness::ExactAtomicWitness;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyDiscrepancyTurnMacroConfig {
    pub max_applied_transitions: usize,
    pub partial_beam_width: usize,
    pub retained_per_view: usize,
    pub max_atomic_depth: usize,
    pub max_structured_members_per_family: usize,
    pub proposals_per_view: usize,
}

impl Default for PolicyDiscrepancyTurnMacroConfig {
    fn default() -> Self {
        Self {
            max_applied_transitions: 256,
            partial_beam_width: 32,
            retained_per_view: 6,
            max_atomic_depth: 32,
            max_structured_members_per_family: 256,
            proposals_per_view: 8,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyDiscrepancyConfig {
    pub max_engine_steps_per_transition: usize,
    pub uniform_exploration_ppm: u32,
    pub max_greedy_actions_per_dive: usize,
    pub turn_macro: Option<PolicyDiscrepancyTurnMacroConfig>,
    /// Maximum potion resources expended by a terminal witness. Use and
    /// discard both count; over-budget wins do not terminate search.
    pub max_potions_used: Option<u32>,
    /// Whether explicit potion-discard inputs belong to this search surface.
    /// Keep true for all-legal diagnostics and false for semantic victory
    /// search unless a concrete slot-generation or revive-order case opts in.
    pub allow_potion_discard: bool,
    /// Optional exact slot mask for explicit potion use and discard actions.
    pub allowed_potion_slots: Option<u64>,
}

impl Default for PolicyDiscrepancyConfig {
    fn default() -> Self {
        Self {
            max_engine_steps_per_transition: 250,
            uniform_exploration_ppm: 10_000,
            max_greedy_actions_per_dive: 128,
            turn_macro: None,
            max_potions_used: None,
            allow_potion_discard: true,
            allowed_potion_slots: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PolicyDiscrepancyQuantum {
    pub additional_applied_transitions: usize,
    pub additional_engine_steps: usize,
    pub deadline: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PolicyDiscrepancyCounters {
    pub policy_dives: usize,
    pub applied_action_transitions: usize,
    pub engine_steps: usize,
    pub exact_states: usize,
    pub queued_discrepancies: usize,
    pub structured_inputs_materialized: usize,
    pub duplicate_or_dominated_states: usize,
    pub unsupported_stable_boundaries: usize,
    pub transition_step_limit_gaps: usize,
    pub greedy_depth_limit_hits: usize,
    pub turn_macro_generations: usize,
    pub turn_macro_partial_generations: usize,
    /// Complete-turn proposals interrupted by an external wall deadline and
    /// returned to the frontier. Partial options remain usable immediately;
    /// the retry prevents caller quantum boundaries from permanently
    /// truncating the proposal surface.
    pub turn_macro_deadline_retries: usize,
    pub turn_macro_applied_transitions: usize,
    pub turn_macro_options_generated: usize,
    pub turn_macro_options_enqueued: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyDiscrepancyInterruption {
    AppliedTransitionBudget,
    EngineStepBudget,
    Deadline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyDiscrepancyStatus {
    WitnessFound,
    Partial(PolicyDiscrepancyInterruption),
    FrontierExhausted,
    ReplayMismatch,
}

#[derive(Clone, Debug)]
pub struct PolicyDiscrepancyReport {
    pub before: PolicyDiscrepancyCounters,
    pub after: PolicyDiscrepancyCounters,
    pub frontier_entries: usize,
    pub best_queued_priority: Option<f64>,
    pub best_queued_discrepancy: Option<f64>,
    pub status: PolicyDiscrepancyStatus,
    pub witness: Option<ExactAtomicWitness>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDiscrepancyStateDiagnostic {
    pub exact_state_hash: String,
    pub discovered: bool,
    pub best_discrepancy: Option<f64>,
    pub policy_dive_services: usize,
    pub selected_by_turn_macro: bool,
    pub turn_macro_scheduled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDiscrepancyTrajectoryDeviation {
    pub action_index: usize,
    pub player_turn: u32,
    pub demonstrated_input: ClientInput,
    pub greedy_input: ClientInput,
    pub demonstrated_probability: f64,
    pub greedy_probability: f64,
    pub discrepancy_increment: f64,
    pub cumulative_discrepancy: f64,
    pub demonstrated_was_lazy: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDiscrepancyTrajectoryAudit {
    pub source_action_count: usize,
    pub non_greedy_action_count: usize,
    pub total_weighted_discrepancy: f64,
    pub terminal: CombatTerminal,
    pub deviations: Vec<PolicyDiscrepancyTrajectoryDeviation>,
}
