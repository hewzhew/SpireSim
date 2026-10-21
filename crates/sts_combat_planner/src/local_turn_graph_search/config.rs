use crate::types::TurnOptionGeneratorConfig;
use crate::witness::OracleCombatWitnessSatisfaction;
use crate::CombatGuideServiceBias;

/// One coherent, plan-aware complete-turn proposal batch at a fresh exact
/// player-turn boundary.  This is candidate generation, not a second graph
/// search: the local graph remains the sole owner of cross-turn identity,
/// terminal proof, and search budgets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicPlanTurnProposalConfig {
    pub max_inner_nodes: usize,
    pub max_end_states: usize,
    pub per_bucket_limit: usize,
}

impl Default for PublicPlanTurnProposalConfig {
    fn default() -> Self {
        Self {
            max_inner_nodes: 256,
            max_end_states: 24,
            per_bucket_limit: 24,
        }
    }
}

/// Coherent work granted by one guide-selected boundary service.
///
/// Runtime and laboratory hosts share this value so a report's scheduler
/// identity matches production.  It is large enough to make one selected
/// boundary productive without letting a few one-shot guide entries consume
/// the entire bounded allowance before later high-ranked states are visited.
pub const DEFAULT_BACKED_GENERATION_QUANTUM_WORK: usize = 128;
pub const DEFAULT_ROOT_INITIAL_EXPANSION_WORK: usize = 2_048;

/// Keeps the root discovery batch proportional to the caller's complete
/// allowance. A fixed 2,048-work root batch consumed half of a routine
/// 4,096-work contract even after a viable opening was already materialized,
/// leaving too little service for deeper exact boundaries.
pub fn root_initial_expansion_work_for_budget(total_generation_work: usize) -> usize {
    if total_generation_work == 0 {
        return 0;
    }
    total_generation_work
        .div_ceil(8)
        .clamp(64, DEFAULT_ROOT_INITIAL_EXPANSION_WORK)
        .min(total_generation_work)
}

/// Resumable search over a shared graph of exact player-turn boundaries.
///
/// Complete-turn generation remains lazy. Independent global views select one
/// shared boundary node, while the selected node owns its local generation
/// lane. No guide recursively owns the subtree below the state it selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalTurnGraphWitnessConfig {
    pub generator: TurnOptionGeneratorConfig,
    /// One deterministic service unit for a selected node's resumable turn
    /// generator. This controls preemption granularity, not search quality.
    pub generation_quantum_work: usize,
    /// Coherent generator service granted when one non-anchor guide selects an
    /// exact boundary. A guide services a shared state once; repeated and
    /// exhaustive coverage remains owned by the anchor queue.
    pub backed_generation_quantum_work: usize,
    /// Optional typed scheduler control that gives one existing boundary guide
    /// additional relative weight inside the auxiliary half of generation
    /// work. The lane still owns one-shot entries and every selected boundary
    /// receives the ordinary guide quantum. The default is disabled; an
    /// explicit typed caller may opt one encounter into a measured service
    /// concentration.
    pub guide_service_bias: Option<CombatGuideServiceBias>,
    /// Deterministic work reserved for the first expansion of a selected exact
    /// turn-boundary node. Later resumptions return to the small quantum.
    pub initial_expansion_work: usize,
    /// Root-only discovery batch. Root proposals gate every deeper path, so
    /// they receive a wider but still bounded first expansion.
    pub root_initial_expansion_work: usize,
    pub max_turn_depth: usize,
    pub satisfaction: OracleCombatWitnessSatisfaction,
    /// Require a satisfying terminal witness to leave no stolen gold on a
    /// living Looter or Mugger. Partial-recovery victories remain exact
    /// frontier evidence, but cannot terminate this search contract.
    pub require_no_unrecovered_stolen_gold: bool,
    /// Maximum number of potion resources expended by an accepted witness.
    ///
    /// This is a run-resource contract, not an action prior. Legal prefixes up
    /// to the limit remain searchable, while uses or discards beyond the
    /// remaining allowance are never generated. The spent allowance is also
    /// part of constrained exact-state identity.
    pub max_potions_used: Option<u32>,
}

impl Default for LocalTurnGraphWitnessConfig {
    fn default() -> Self {
        Self {
            generator: TurnOptionGeneratorConfig::default(),
            generation_quantum_work: 4,
            backed_generation_quantum_work: DEFAULT_BACKED_GENERATION_QUANTUM_WORK,
            guide_service_bias: None,
            initial_expansion_work: 64,
            root_initial_expansion_work: DEFAULT_ROOT_INITIAL_EXPANSION_WORK,
            max_turn_depth: 32,
            satisfaction: OracleCombatWitnessSatisfaction::FirstWitness,
            require_no_unrecovered_stolen_gold: false,
            max_potions_used: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_grounding_is_proportional_for_small_contracts_and_capped_for_large_ones() {
        assert_eq!(root_initial_expansion_work_for_budget(0), 0);
        assert_eq!(root_initial_expansion_work_for_budget(16), 16);
        assert_eq!(root_initial_expansion_work_for_budget(512), 64);
        assert_eq!(root_initial_expansion_work_for_budget(4_096), 512);
        assert_eq!(root_initial_expansion_work_for_budget(16_384), 2_048);
        assert_eq!(root_initial_expansion_work_for_budget(1_000_000), 2_048);
    }
}
