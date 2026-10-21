pub mod artifact;
#[cfg(any(feature = "control-full", feature = "combat-search-driver"))]
pub mod campfire_evaluation;
#[cfg(any(feature = "control-full", feature = "combat-search-driver"))]
pub mod campfire_projection;
#[cfg(any(feature = "control-full", feature = "combat-search-driver"))]
pub mod campfire_survival_scenarios;
#[cfg(any(feature = "control-full", feature = "combat-search-driver"))]
pub mod campfire_threat_panel;
#[cfg(any(
    not(feature = "combat-search-driver"),
    feature = "oracle-lab",
    feature = "control-full"
))]
pub mod combat_action_imitation;
pub mod combat_baseline_outcome;
pub mod combat_capture;
pub mod combat_case_core;
#[cfg(any(
    not(feature = "combat-search-driver"),
    feature = "oracle-lab",
    feature = "control-full"
))]
pub mod combat_guidance_bundle;
pub mod combat_lab_v1;
pub mod combat_search_v2;
#[cfg(any(
    not(feature = "combat-search-driver"),
    feature = "oracle-lab",
    feature = "control-full"
))]
pub mod combat_state_features;
pub use sts_combat_outcome::combat_witness_materialization;
pub use sts_combat_storyboard::combat_witness_storyboard;
#[cfg(any(
    not(feature = "combat-search-driver"),
    feature = "oracle-lab",
    feature = "control-full"
))]
pub mod event_boundary_classifier_v1;
pub mod fingerprint;
pub mod source_identity;

pub(crate) fn repository_root() -> &'static std::path::Path {
    std::path::Path::new(env!("STS_REPOSITORY_ROOT"))
}
