use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::eval::combat_guidance_bundle::CombatGuidanceBundleV1;

use super::OracleCombatWitnessOptionsV1;

/// Immutable owner input for selecting bounded combat-search lanes.
///
/// Scheduling and resume behavior are implemented by the run explorer, but
/// artifacts may capture this small data contract without depending on the
/// explorer's private queues or combat-work state.
#[derive(Clone, Debug)]
pub struct OracleRunCombatWitnessBudgetsV1 {
    pub hallway: OracleCombatWitnessOptionsV1,
    pub elite: OracleCombatWitnessOptionsV1,
    pub boss: OracleCombatWitnessOptionsV1,
    /// Determines whether each configured search satisfaction is used
    /// literally or whether non-boss combat derives the shared strategic
    /// quality target from the exact run state.
    pub quality_policy: OracleRunCombatWitnessQualityPolicyV1,
    /// A value greater than one enables a two-fidelity schedule. The first
    /// exact attempt receives `1 / initial_divisor` of the configured
    /// allowance. A budget-unknown result remains a live exact edge and may
    /// later earn one full-budget restart.
    pub initial_divisor: u32,
    /// Optional immutable learned guidance. Exact simulation, legality,
    /// terminal checks, and replay remain authoritative.
    pub guidance_bundle: Option<Arc<CombatGuidanceBundleV1>>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleRunCombatWitnessQualityPolicyV1 {
    /// Preserve the satisfaction carried by each configured search option.
    #[default]
    Configured,
    /// Stop refinement once an exact witness satisfies the run's shared
    /// survival-and-quality reserve. A boss that reaches a full act heal or
    /// the requested run end keeps first-witness semantics after one complete
    /// independent local-search challenge.
    StrategicRun,
}

impl OracleRunCombatWitnessBudgetsV1 {
    pub fn uniform(options: OracleCombatWitnessOptionsV1) -> Self {
        Self {
            hallway: options.clone(),
            elite: options.clone(),
            boss: options,
            quality_policy: OracleRunCombatWitnessQualityPolicyV1::Configured,
            initial_divisor: 1,
            guidance_bundle: None,
        }
    }

    pub fn with_guidance_bundle(mut self, bundle: Option<CombatGuidanceBundleV1>) -> Self {
        self.guidance_bundle = bundle.map(Arc::new);
        self
    }
}
