//! Rust simulator adapter for the transport-independent parity lockstep core.
//!
//! This crate owns only the translation between [`RunControlSession`] and the
//! shared semantic action/snapshot contracts. It does not launch Java, select
//! strategy, persist a run head, or expose a CLI/MCP surface.

mod action;
mod adapter;
mod bootstrap;
mod match_game;
mod projection;
mod rng_trace;
mod shop;

pub use adapter::{PreparedRustActionV1, RustLockstepAdapterV1};
pub use bootstrap::{natural_run_bootstrap_v1, run_control_config_from_bootstrap_v1};
pub use sts_parity_lockstep::{
    NaturalRunBootstrapCardV1, NaturalRunBootstrapV1, NaturalRunCardSourceV1,
    NaturalRunContentProfileV1, NaturalRunNeowEligibilityV1,
    NATURAL_RUN_SNAPSHOT_CONTRACT_SCHEMA_V3, NATURAL_RUN_SNAPSHOT_DOMAINS_V3,
};

pub use projection::{
    project_ai_shuffle_mechanism_v1, project_cards_relics_mechanism_v1,
    project_creatures_mechanism_v1,
};
