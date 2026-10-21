//! Offline local-learning and search experiments over the shared simulator.
//! State transitions, numerical observations and replay are shared by the callers.
//! See tools/guided_search/README.md for the retained experimental scope.

mod combat;
pub mod continuation;
pub mod distance_search;
pub mod entities;
mod guidance;
pub mod information_search;
pub mod local;
pub mod local_tactics;
pub mod natural;
mod objective;
pub mod observation;
pub mod path_search;
pub mod policy_search;
pub mod resource_value;
pub mod segment_learning;
mod selection;
pub mod shuffle_order;
pub mod solver;
pub mod task_information_search;
mod tree;

pub use combat::{
    CombatBranch, CombatSearchSpace, CombatSearchState, ObservedCombatInput, ReplayedCombatWin,
};
pub use guidance::{
    GuidanceBudget, GuidanceCosts, GuidanceOutcome, GuidanceReceipt, GuidanceTicket,
    TokenReservation, TokenUsage,
};
pub use objective::ResourceRequirement;
pub use selection::SelectionMember;
pub use tree::{
    Choice, Guidance, GuidanceWork, NodeRef, NodeView, SearchBudget, SearchConfig, SearchError,
    SearchSession, SearchSpace, SearchStats, SearchTimings, StepStatus, StopReason, Transition,
    TransitionInterruption,
};

#[cfg(test)]
mod tests;
