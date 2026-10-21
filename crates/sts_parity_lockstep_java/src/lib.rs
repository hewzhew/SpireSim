//! Java original-game adapter for the transport-independent lockstep core.
//!
//! This crate translates an already-running CommunicationMod session into
//! shared semantic actions and parity snapshots. It does not launch Java,
//! select strategy, own a trajectory, or expose a CLI/MCP surface. Its owned
//! production transport retains the separately constructed process owner so
//! adapter calls cannot rediscover or outlive that Java session.

mod acquisition;
mod action;
mod adapter;
mod deck_transform;
mod execution_inputs;
mod match_game;
mod projection;
mod recording;
mod shop;
mod transport;

#[cfg(test)]
mod observed_regression_fixtures;

pub use action::grid_selection_card_kind_v1;
pub use adapter::{JavaLockstepAdapterV1, JavaLockstepResumeStateV1, PreparedJavaActionV1};
pub use execution_inputs::execution_inputs_from_transitions_v1;
pub use projection::{
    java_act_exit_v1, project_cards_relics_mechanism_v1, project_combat_rng_mechanism_v1,
    project_monster_ai_mechanism_v1, project_player_mechanism_v1,
};
pub use recording::{
    RecordedJavaActionPhaseV1, RecordedJavaActionV1, RecordedJavaBoundaryV1,
    RecordedJavaCommittedStepV1, RecordedJavaEvidenceManifestV1, RecordedJavaEvidenceStoreV1,
    RecordedJavaEvidenceV1, RecordedJavaStepRefV1, RecordedJavaTransitionV1,
    RECORDED_JAVA_ACTION_SCHEMA_V1, RECORDED_JAVA_BOUNDARY_SCHEMA_V1,
    RECORDED_JAVA_EVIDENCE_MANIFEST_SCHEMA_V1,
};
pub use transport::{
    producer_identity_from_live_v1, JavaLockstepReceiptV1, JavaLockstepTransportV1,
    JavaNativeReloadExecutionV1, JavaRuntimeActionV1, JavaRuntimeObservationV1,
    LeasedJavaLockstepTransportV1, LiveJavaLockstepTransportV1, OwnedJavaLockstepTransportV1,
};
