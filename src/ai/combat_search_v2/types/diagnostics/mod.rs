use serde::Serialize;

mod action;
mod core;
mod identity;
mod pending;
mod turn;

pub use action::*;
pub use core::*;
pub use identity::*;
pub use pending::*;
pub use turn::*;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsReport {
    pub schema_version: u32,
    pub mode: &'static str,
    pub tables: CombatSearchV2DiagnosticsTables,
    pub branching: CombatSearchV2DiagnosticsBranching,
    pub expansion: CombatSearchV2DiagnosticsExpansion,
    pub target_fanout: CombatSearchV2DiagnosticsTargetFanout,
    pub equivalence: CombatSearchV2DiagnosticsEquivalence,
    pub ordering: CombatSearchV2DiagnosticsOrdering,
    pub turn_branching: CombatSearchV2DiagnosticsTurnBranching,
    pub pending_choice: CombatSearchV2DiagnosticsPendingChoice,
    pub turn_prefix: CombatSearchV2DiagnosticsTurnPrefix,
    pub turn_sequence: CombatSearchV2DiagnosticsTurnSequence,
    pub turn_plan: CombatSearchV2DiagnosticsTurnPlan,
    pub card_identity: CombatSearchV2DiagnosticsCardIdentity,
    pub turn_local_dominance: CombatSearchV2DiagnosticsTurnLocalDominance,
    pub pruning: CombatSearchV2DiagnosticsPruning,
    pub frontier: CombatSearchV2DiagnosticsFrontier,
    pub diagnosis: Vec<&'static str>,
}
