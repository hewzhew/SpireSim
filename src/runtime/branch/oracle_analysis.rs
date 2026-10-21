use crate::eval::combat_guidance_bundle::CombatGuidanceBundleV1;
use crate::eval::run_control::{
    expand_oracle_neow_candidates_v1, seed_oracle_run_explorer_from_checkpoint_v1,
    seed_oracle_run_explorer_from_session_v1, seed_oracle_run_explorer_v1,
    uniform_run_policy_prior_v1, RunControlConfig, RunControlSession, RunPolicyPriorIdentityV1,
};
use crate::state::core::ClientInput;

use super::oracle_analysis_session::{
    OracleAnalysisAdvanceReportV1, OracleAnalysisAdvanceRequestV1,
    OracleAnalysisCombatProbeReportV1, OracleAnalysisCombatProbeRequestV1,
    OracleAnalysisNodeViewV1, OracleAnalysisSessionV1,
};
use super::oracle_analysis_workspace_contract::{
    OracleAnalysisWorkspaceArtifactV1, ORACLE_ANALYSIS_WORKSPACE_SCHEMA_NAME,
    ORACLE_ANALYSIS_WORKSPACE_SCHEMA_VERSION,
};
use super::oracle_run::{
    oracle_run_combat_budgets_v1, OracleRunBudget, OracleRunConfig, OracleRunContinuationV1,
    ORACLE_RUN_CONTINUATION_SCHEMA_NAME, ORACLE_RUN_CONTINUATION_SCHEMA_VERSION,
};

pub struct OracleAnalysisWorkspaceV1 {
    pub seed: u64,
    pub ascension: u8,
    pub budget: OracleRunBudget,
    pub combat_guidance_bundle: Option<CombatGuidanceBundleV1>,
    pub session: OracleAnalysisSessionV1,
}

impl OracleAnalysisWorkspaceV1 {
    pub fn new(config: OracleRunConfig) -> Result<Self, String> {
        Self::new_with_combat_guidance(config, None)
    }

    pub fn new_with_combat_guidance(
        config: OracleRunConfig,
        combat_guidance_bundle: Option<CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        validate_analysis_config(&config)?;
        validate_combat_guidance(&combat_guidance_bundle)?;
        let session = RunControlSession::new(RunControlConfig {
            seed: config.seed,
            ascension_level: config.ascension,
            final_act: false,
            reward_automation: super::oracle_run::oracle_reward_automation_config(),
            ..RunControlConfig::default()
        });
        let expansion = expand_oracle_neow_candidates_v1(&session)
            .map_err(|error| format!("failed to materialize oracle Neow roots: {error}"))?;
        let explorer = seed_oracle_run_explorer_v1(expansion, Some(uniform_run_policy_prior_v1))?;
        let first_root = explorer.branches.first().map(|branch| branch.branch_id);
        let combat_budgets = oracle_run_combat_budgets_v1(&config)
            .with_guidance_bundle(combat_guidance_bundle.clone());
        let analysis = OracleAnalysisSessionV1::from_explorer(
            explorer,
            first_root,
            combat_budgets,
            Some(uniform_run_policy_prior_v1),
            None,
        )?;
        Ok(Self {
            seed: config.seed,
            ascension: config.ascension,
            budget: config.budget,
            combat_guidance_bundle,
            session: analysis,
        })
    }

    pub fn from_continuation(
        config: OracleRunConfig,
        continuation: OracleRunContinuationV1,
    ) -> Result<Self, String> {
        Self::from_continuation_with_combat_guidance(config, continuation, None)
    }

    pub fn from_continuation_with_combat_guidance(
        config: OracleRunConfig,
        continuation: OracleRunContinuationV1,
        combat_guidance_bundle: Option<CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        validate_analysis_config(&config)?;
        validate_combat_guidance(&combat_guidance_bundle)?;
        continuation.validate_supported_semantics()?;
        if continuation.seed != config.seed || continuation.ascension != config.ascension {
            return Err(format!(
                "oracle continuation is seed {} A{}, requested analysis is seed {} A{}",
                continuation.seed, continuation.ascension, config.seed, config.ascension
            ));
        }
        let combat_budgets = oracle_run_combat_budgets_v1(&config)
            .with_guidance_bundle(combat_guidance_bundle.clone());
        // Import the exact selected state and its committed journal. Historical
        // automatic frontier work is intentionally not treated as an editable
        // analysis tree; the workbench creates explicit variations from here.
        let explorer = seed_oracle_run_explorer_from_session_v1(
            continuation.session.into_session()?,
            continuation.journal,
            &combat_budgets,
            Some(uniform_run_policy_prior_v1),
        )?;
        let cursor = explorer.branches.first().map(|branch| branch.branch_id);
        let analysis = OracleAnalysisSessionV1::from_explorer(
            explorer,
            cursor,
            combat_budgets,
            Some(uniform_run_policy_prior_v1),
            None,
        )?;
        Ok(Self {
            seed: config.seed,
            ascension: config.ascension,
            budget: config.budget,
            combat_guidance_bundle,
            session: analysis,
        })
    }

    pub fn from_continuation_branch(
        config: OracleRunConfig,
        continuation: OracleRunContinuationV1,
        branch_id: usize,
    ) -> Result<Self, String> {
        Self::from_continuation_branch_with_combat_guidance(config, continuation, branch_id, None)
    }

    pub fn from_continuation_branch_with_combat_guidance(
        config: OracleRunConfig,
        continuation: OracleRunContinuationV1,
        branch_id: usize,
        combat_guidance_bundle: Option<CombatGuidanceBundleV1>,
    ) -> Result<Self, String> {
        validate_analysis_config(&config)?;
        validate_combat_guidance(&combat_guidance_bundle)?;
        continuation.validate_supported_semantics()?;
        if continuation.seed != config.seed || continuation.ascension != config.ascension {
            return Err(format!(
                "oracle continuation is seed {} A{}, requested analysis is seed {} A{}",
                continuation.seed, continuation.ascension, config.seed, config.ascension
            ));
        }
        let combat_budgets = oracle_run_combat_budgets_v1(&config)
            .with_guidance_bundle(combat_guidance_bundle.clone());
        let frontier = continuation.explorer_frontier.ok_or_else(|| {
            "oracle continuation has no retained frontier from which to import a branch".to_string()
        })?;
        let mut restored = seed_oracle_run_explorer_from_checkpoint_v1(frontier, &combat_budgets)?;
        let branch_index = restored
            .branches
            .iter()
            .position(|branch| branch.branch_id == branch_id)
            .ok_or_else(|| format!("oracle continuation does not retain branch {branch_id}"))?;
        let branch = restored.branches.swap_remove(branch_index);
        let explorer = seed_oracle_run_explorer_from_session_v1(
            branch.session,
            branch.journal,
            &combat_budgets,
            Some(uniform_run_policy_prior_v1),
        )?;
        let cursor = explorer.branches.first().map(|branch| branch.branch_id);
        let analysis = OracleAnalysisSessionV1::from_explorer(
            explorer,
            cursor,
            combat_budgets,
            Some(uniform_run_policy_prior_v1),
            None,
        )?;
        Ok(Self {
            seed: config.seed,
            ascension: config.ascension,
            budget: config.budget,
            combat_guidance_bundle,
            session: analysis,
        })
    }

    pub fn continuation(&self, node_id: usize) -> Result<OracleRunContinuationV1, String> {
        let (journal, session) = self.session.continuation_parts(node_id)?;
        Ok(OracleRunContinuationV1 {
            schema_name: ORACLE_RUN_CONTINUATION_SCHEMA_NAME.to_string(),
            schema_version: ORACLE_RUN_CONTINUATION_SCHEMA_VERSION,
            decision_prior: Some(RunPolicyPriorIdentityV1::UniformExactSurfaceV1),
            seed: self.seed,
            ascension: self.ascension,
            journal,
            session,
            explorer_frontier: None,
        })
    }

    /// Build a fresh one-node workbench from one exact committed node.
    ///
    /// The source remains unchanged. Historical variations are intentionally
    /// omitted, while the selected session and its committed journal remain
    /// exact. Resident combat search is rejected because its in-memory
    /// frontier is not represented by an ordinary run continuation.
    pub fn compact_from_node(&self, node_id: usize) -> Result<Self, String> {
        if self.session.has_resident_combat_search(node_id)? {
            return Err(format!(
                "oracle analysis node {node_id} has resident combat search; accept or restart that search before compacting"
            ));
        }
        let continuation = self.continuation(node_id)?;
        Self::from_continuation_with_combat_guidance(
            OracleRunConfig {
                seed: self.seed,
                ascension: self.ascension,
                budget: self.budget,
            },
            continuation,
            self.combat_guidance_bundle.clone(),
        )
    }

    pub fn restore(artifact: OracleAnalysisWorkspaceArtifactV1) -> Result<Self, String> {
        artifact.validate_supported_semantics()?;
        let config = OracleRunConfig {
            seed: artifact.seed,
            ascension: artifact.ascension,
            budget: artifact.budget,
        };
        validate_analysis_config(&config)?;
        validate_combat_guidance(&artifact.combat_guidance_bundle)?;
        let combat_budgets = oracle_run_combat_budgets_v1(&config)
            .with_guidance_bundle(artifact.combat_guidance_bundle.clone());
        let session = OracleAnalysisSessionV1::restore(
            artifact.session,
            combat_budgets,
            Some(uniform_run_policy_prior_v1),
            None,
        )?;
        Ok(Self {
            seed: artifact.seed,
            ascension: artifact.ascension,
            budget: artifact.budget,
            combat_guidance_bundle: artifact.combat_guidance_bundle,
            session,
        })
    }

    pub fn artifact(&self) -> Result<OracleAnalysisWorkspaceArtifactV1, String> {
        Ok(OracleAnalysisWorkspaceArtifactV1 {
            schema_name: ORACLE_ANALYSIS_WORKSPACE_SCHEMA_NAME.to_string(),
            schema_version: ORACLE_ANALYSIS_WORKSPACE_SCHEMA_VERSION,
            decision_prior: Some(RunPolicyPriorIdentityV1::UniformExactSurfaceV1),
            seed: self.seed,
            ascension: self.ascension,
            budget: self.budget,
            combat_guidance_bundle: self.combat_guidance_bundle.clone(),
            session: self.session.checkpoint()?,
        })
    }

    pub fn view(&self) -> Result<OracleAnalysisNodeViewV1, String> {
        self.session.view_cursor()
    }

    pub fn try_choice(&mut self, choice_ref: &str) -> Result<OracleAnalysisNodeViewV1, String> {
        self.session.try_choice(choice_ref)?;
        self.view()
    }

    pub fn advance(
        &mut self,
        request: OracleAnalysisAdvanceRequestV1,
    ) -> Result<(OracleAnalysisAdvanceReportV1, OracleAnalysisNodeViewV1), String> {
        let report = self.session.advance_cursor(request)?;
        let view = self.view()?;
        Ok((report, view))
    }

    pub fn probe_combat(
        &mut self,
        request: OracleAnalysisCombatProbeRequestV1,
    ) -> Result<(OracleAnalysisCombatProbeReportV1, OracleAnalysisNodeViewV1), String> {
        let report = self.session.probe_cursor_combat_stage(request)?;
        let view = self.view()?;
        Ok((report, view))
    }

    pub fn accept_combat_incumbent(&mut self) -> Result<OracleAnalysisNodeViewV1, String> {
        self.session.accept_cursor_combat_incumbent()?;
        self.view()
    }

    pub fn accept_combat_actions(
        &mut self,
        actions: &[ClientInput],
    ) -> Result<OracleAnalysisNodeViewV1, String> {
        self.session.accept_cursor_combat_actions(actions)?;
        self.view()
    }

    pub fn accept_smoke_bomb_escape(&mut self) -> Result<OracleAnalysisNodeViewV1, String> {
        self.session.accept_cursor_smoke_bomb_escape()?;
        self.view()
    }

    pub fn commit_combat_scratch(&mut self) -> Result<OracleAnalysisNodeViewV1, String> {
        self.session.commit_combat_scratch()?;
        self.view()
    }
}

fn validate_combat_guidance(guidance: &Option<CombatGuidanceBundleV1>) -> Result<(), String> {
    guidance
        .as_ref()
        .map(CombatGuidanceBundleV1::validate)
        .transpose()
        .map(|_| ())
}

fn validate_analysis_config(config: &OracleRunConfig) -> Result<(), String> {
    if config.ascension > 20 {
        return Err(format!(
            "oracle analysis ascension must be in 0..=20, got {}",
            config.ascension
        ));
    }
    if config.budget.combat_quantum_generation_work == 0 || config.budget.combat_quantum_ms == 0 {
        return Err("oracle analysis combat quantum must be positive".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        OracleAnalysisWorkspaceV1, OracleRunBudget, OracleRunConfig, RunPolicyPriorIdentityV1,
        ORACLE_ANALYSIS_WORKSPACE_SCHEMA_VERSION,
    };

    fn workspace() -> OracleAnalysisWorkspaceV1 {
        OracleAnalysisWorkspaceV1::new(OracleRunConfig {
            seed: 20260713007,
            ascension: 0,
            budget: OracleRunBudget::default(),
        })
        .expect("neutral oracle workspace")
    }

    #[test]
    fn same_seed_starts_from_the_same_neutral_neow_root() {
        let build = || workspace().view().expect("neutral initial view");

        let first = build();
        let second = build();
        assert_eq!(first.neow_root_label, second.neow_root_label);
        assert_eq!(first.recent_replay, second.recent_replay);
    }

    #[test]
    fn workspace_artifact_persists_the_uniform_prior_identity() {
        let artifact = workspace().artifact().expect("workspace artifact");

        assert_eq!(
            artifact.schema_version,
            ORACLE_ANALYSIS_WORKSPACE_SCHEMA_VERSION
        );
        assert_eq!(
            artifact.decision_prior,
            Some(RunPolicyPriorIdentityV1::UniformExactSurfaceV1)
        );
        let json = serde_json::to_value(&artifact).expect("serialized workspace artifact");
        assert_eq!(json["decision_prior"], "uniform_exact_surface_v1");
        OracleAnalysisWorkspaceV1::restore(artifact).expect("matching prior restores");
    }

    #[test]
    fn workspace_restore_rejects_legacy_or_missing_prior_semantics() {
        let artifact = workspace().artifact().expect("workspace artifact");

        let mut legacy = artifact.clone();
        legacy.schema_version = 1;
        legacy.decision_prior = None;
        let legacy_error = OracleAnalysisWorkspaceV1::restore(legacy)
            .err()
            .expect("legacy workspace must be rejected");
        assert!(legacy_error.contains("version 1"), "{legacy_error}");

        let mut missing_prior = artifact;
        missing_prior.decision_prior = None;
        let prior_error = OracleAnalysisWorkspaceV1::restore(missing_prior)
            .err()
            .expect("V2 workspace without a named prior must be rejected");
        assert!(prior_error.contains("decision prior"), "{prior_error}");
    }
}
