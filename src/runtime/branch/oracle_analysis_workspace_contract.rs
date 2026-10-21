use serde::{Deserialize, Serialize};

use super::oracle_analysis_session::OracleAnalysisSessionCheckpointV1;
use crate::eval::combat_guidance_bundle::CombatGuidanceBundleV1;
use crate::eval::run_control::RunPolicyPriorIdentityV1;

use super::oracle_run::OracleRunBudget;

pub const ORACLE_ANALYSIS_WORKSPACE_SCHEMA_NAME: &str = "OracleAnalysisWorkspace";
pub const ORACLE_ANALYSIS_WORKSPACE_SCHEMA_VERSION: u32 = 2;

/// Durable workspace envelope. Live analysis/search objects never enter this
/// contract; their run-control owners first project typed checkpoints.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisWorkspaceArtifactV1 {
    pub schema_name: String,
    pub schema_version: u32,
    /// `None` exists only so a V1 artifact can be decoded and rejected with a
    /// precise schema error. Every supported V2 artifact must name its prior.
    #[serde(default)]
    pub decision_prior: Option<RunPolicyPriorIdentityV1>,
    pub seed: u64,
    pub ascension: u8,
    pub budget: OracleRunBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_guidance_bundle: Option<CombatGuidanceBundleV1>,
    pub session: OracleAnalysisSessionCheckpointV1,
}

impl OracleAnalysisWorkspaceArtifactV1 {
    pub fn validate_supported_semantics(&self) -> Result<(), String> {
        if self.schema_name != ORACLE_ANALYSIS_WORKSPACE_SCHEMA_NAME
            || self.schema_version != ORACLE_ANALYSIS_WORKSPACE_SCHEMA_VERSION
        {
            return Err(format!(
                "unsupported oracle analysis workspace {} version {}",
                self.schema_name, self.schema_version
            ));
        }
        if self.decision_prior != Some(RunPolicyPriorIdentityV1::UniformExactSurfaceV1) {
            return Err("unsupported oracle analysis workspace decision prior".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OracleAnalysisWorkspaceSaveTimingV1 {
    pub checkpoint_elapsed_ms: u64,
    pub write_elapsed_ms: u64,
}
