//! Complete-run task semantics for a deployable learned agent.
//!
//! This module deliberately owns no simulator checkpoint type. Exact worlds
//! implement [`TaskWorldV1`], while deployed actors receive only
//! [`TaskActorViewV1`].

mod actor;
mod combat_agenda;
mod combat_plan_actor;
mod combat_proposal;
mod environment;
mod exact_branch_actor;
mod improvement;
mod privileged_support;
mod protocol;
mod public;
mod research_plan;
mod run_archive;
mod spec;
mod strategic_projection;
mod trajectory;

pub use actor::*;
pub use combat_agenda::*;
pub use combat_plan_actor::*;
pub use combat_proposal::*;
pub use environment::*;
pub use exact_branch_actor::*;
pub use improvement::*;
pub use privileged_support::*;
pub use protocol::*;
pub use public::*;
pub use research_plan::*;
pub use run_archive::*;
pub use spec::*;
pub use trajectory::*;

#[cfg(test)]
mod tests;
