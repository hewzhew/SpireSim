use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::agent::task::{
    CommittedPublicHistoryV1, GameOutcomeV1, NodeReloadTaskEnvironmentV1, ReloadAuthorizationV1,
    ReloadPermissionV1, ReloadScopeV1, TaskBudgetV1, TaskDecisionSiteV1,
    TaskExecutionEvidenceWorldV1, TaskExploredScopeExitV1, TaskInitialConditionV1, TaskObjectiveV1,
    TaskPrivilegedSupportV1, TaskPrivilegedSupportWorldV1, TaskPublicChoiceV1,
    TaskPublicDecisionV1, TaskSpecV1, TaskWorldCommitV1, TaskWorldExplorationV1, TaskWorldV1,
};
use crate::ai::route_window_facts::{summarize_route_entry_paths_v1, RouteEntryPathSummaryV1};
use crate::content::events::neow::NeowEligibilityV1;
use crate::content::monsters::factory::EncounterId;
use crate::eval::combat_case::{
    capture_production_combat_case_v2, CombatCase, CombatCaseGap, CombatCaseSource,
    CombatCaseWitnessBudgetV1,
};
use crate::eval::combat_case_context::restore_combat_case_production_session_v2;
use crate::sim::combat::{CombatPosition, CombatTerminal};
use crate::state::core::ClientInput;

use super::combat_research_plan::{
    combat_research_plan_v1, combat_research_portfolio_v1,
    combat_research_portfolio_with_progress_v1, CombatResearchPlanOptionsV1,
    CombatResearchPortfolioV1, CombatResearchProgressEventV1,
};
use super::privileged_strategic_support::privileged_strategic_support_v1;
use super::task_world_support::{
    game_outcome_for_learning_terminal_v1, natural_ironclad_learning_env_v1,
    natural_ironclad_learning_env_with_neow_and_objective_v1, prepare_task_choice_v1,
    task_public_decision_v1,
};
use super::{
    build_decision_surface, exact_boss_relic_policy_audit_v1, exact_campfire_policy_audit_v1,
    exact_card_reward_policy_audit_v1, exact_direct_card_acquisition_policy_audit_v1,
    exact_route_policy_audit_v1, exact_run_choice_policy_audit_v1, exact_shop_policy_audit_v1,
    potion_reward_policy_audit_v1, ExactBossRelicPolicyAuditV1, ExactCampfirePolicyAuditV1,
    ExactCardRewardPolicyAuditV1, ExactRoutePolicyAuditV1, ExactRunChoicePolicyAuditV1,
    ExactShopPolicyAuditV1, LearningBoundaryV1, LearningEnvCheckpointV3, LearningEnvV3,
    LearningPreparedActionV3, PotionRewardPolicyAuditV1, RunPolicyCandidateV1,
};

/// Private exact simulator root for one research branching scope. It is never
/// serialized into or exposed through the public actor view.
#[derive(Clone, Debug)]
pub struct SimulatorExactScopeRootV1 {
    checkpoint: LearningEnvCheckpointV3,
    public_root: TaskPublicDecisionV1,
    execution_evidence: ExactBranchingTaskExecutionEvidenceV1,
    captured_combat_sequences: BTreeSet<u64>,
}

/// Durable behavior state for one exact scope root. Evaluator-only combat
/// capture evidence is intentionally excluded so retaining many run branch
/// points grows linearly instead of repeatedly embedding an accumulated
/// diagnostic history.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatorExactScopeCheckpointV1 {
    pub(super) checkpoint: LearningEnvCheckpointV3,
    pub(super) public_root: TaskPublicDecisionV1,
}

/// Validated identity facts exposed to the durable checkpoint owner without
/// exposing the simulator checkpoint representation itself.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct SimulatorExactScopeCheckpointIdentityV1 {
    pub seed: u64,
    pub exact_state_id: String,
    pub lineage_id: String,
    pub public_root: TaskPublicDecisionV1,
}

impl SimulatorExactScopeCheckpointV1 {
    /// Extracts only the simulator state for an explicit reroot. The stored
    /// public decision is not reprojected here: a later build may correct its
    /// display without changing this exact state. The new root is separately
    /// projected and checked against the extracted state fingerprint.
    #[doc(hidden)]
    pub fn exact_state_for_reroot_v1(
        &self,
    ) -> Result<(super::RunControlSessionCheckpointV1, String, String), String> {
        let env = LearningEnvV3::from_checkpoint(self.checkpoint.clone())?;
        Ok((
            env.session_checkpoint(),
            env.exact_state_id(),
            env.lineage_id().to_string(),
        ))
    }

    #[doc(hidden)]
    pub fn normalized_for_durable_branch_v1(mut self) -> Self {
        self.checkpoint = self.checkpoint.trim_combat_diagnostics_for_durable_branch();
        self
    }

    #[doc(hidden)]
    pub fn public_root_v1(&self) -> &TaskPublicDecisionV1 {
        &self.public_root
    }

    #[doc(hidden)]
    pub fn validated_identity_v1(&self) -> Result<SimulatorExactScopeCheckpointIdentityV1, String> {
        let env = LearningEnvV3::from_checkpoint(self.checkpoint.clone())?;
        let public_root = task_public_decision_v1(&env)?;
        if let Some(path) = self.public_root.replay_incompatibility_v2(&public_root) {
            return Err(format!(
                "durable exact scope checkpoint changed its public root at {path}"
            ));
        }
        Ok(SimulatorExactScopeCheckpointIdentityV1 {
            seed: env.session().run_state.seed,
            exact_state_id: env.exact_state_id(),
            lineage_id: env.lineage_id().to_string(),
            // Preserve the stored public root while validating an older
            // observation schema so the checkpoint's content-addressed
            // identity remains stable. Runtime restoration independently
            // reprojects the current public decision from the exact state.
            public_root: self.public_root.clone(),
        })
    }

    #[doc(hidden)]
    pub fn into_runtime_root_v1(self) -> SimulatorExactScopeRootV1 {
        SimulatorExactScopeRootV1::from_durable_checkpoint_v1(self)
    }
}

impl SimulatorExactScopeRootV1 {
    pub fn durable_checkpoint_v1(&self) -> SimulatorExactScopeCheckpointV1 {
        SimulatorExactScopeCheckpointV1 {
            checkpoint: self.checkpoint.clone(),
            public_root: self.public_root.clone(),
        }
        .normalized_for_durable_branch_v1()
    }

    pub(super) fn from_durable_checkpoint_v1(checkpoint: SimulatorExactScopeCheckpointV1) -> Self {
        Self {
            checkpoint: checkpoint.checkpoint,
            public_root: checkpoint.public_root,
            execution_evidence: ExactBranchingTaskExecutionEvidenceV1::default(),
            captured_combat_sequences: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct LiveExactScopeV1 {
    root: SimulatorExactScopeRootV1,
}

#[derive(Clone, Debug)]
enum PendingExactScopeExitV1 {
    GameTerminal {
        env: LearningEnvV3,
        game_outcome: GameOutcomeV1,
    },
}

/// Research-only exact branching over the same public decisions as the task
/// actor. Original-game autosave replacement is an ordinary transition here,
/// not a search boundary. Formal game terminal also remains provisional until
/// the task kernel explicitly selects it. A planner may commit a non-empty
/// prefix at any represented decision, rebasing the next exact scope there.
#[derive(Clone, Debug)]
pub struct ExactBranchingTaskWorldV1 {
    env: LearningEnvV3,
    live: LiveExactScopeV1,
    pending: Option<PendingExactScopeExitV1>,
    combat_plan_options: Option<CombatResearchPlanOptionsV1>,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
    execution_evidence: ExactBranchingTaskExecutionEvidenceV1,
    captured_combat_sequences: BTreeSet<u64>,
    last_action_result: Option<super::RunActionResultV1>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExactBranchingTaskEvidenceRequestV1 {
    pub combat_entry_encounter: Option<EncounterId>,
    pub capture_all_combat_entries: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ExactBranchingTaskExecutionEvidenceV1 {
    pub captured_combat_case: Option<CombatCase>,
    pub captured_combat_entries: Vec<ExactBranchingCombatEntryEvidenceV1>,
    pub committed_combats: Vec<ExactBranchingCommittedCombatEvidenceV1>,
}

#[derive(Clone, Debug)]
pub struct ExactBranchingCombatEntryEvidenceV1 {
    pub root_public_snapshot_id: String,
    pub case: CombatCase,
}

#[derive(Clone, Debug)]
pub struct ExactBranchingCommittedCombatEvidenceV1 {
    pub root_public_snapshot_id: String,
    pub case: CombatCase,
    pub actions: Vec<ClientInput>,
    pub outcome: super::CombatBaselineOutcomeV1,
}

impl ExactBranchingTaskWorldV1 {
    pub fn new_natural(config: super::RunControlConfig) -> Result<Self, String> {
        Self::from_initial_env(super::task_world_support::natural_learning_env_v1(config)?)
    }

    pub fn new_ironclad(seed: u64, ascension_level: u8) -> Result<Self, String> {
        Self::from_initial_env(natural_ironclad_learning_env_v1(seed, ascension_level)?)
    }

    pub fn new_ironclad_with_neow(
        seed: u64,
        ascension_level: u8,
        neow_eligibility: NeowEligibilityV1,
    ) -> Result<Self, String> {
        Self::new_ironclad_with_neow_and_objective(
            seed,
            ascension_level,
            neow_eligibility,
            TaskObjectiveV1::ActThreeVictory,
        )
    }

    pub fn new_ironclad_with_neow_and_objective(
        seed: u64,
        ascension_level: u8,
        neow_eligibility: NeowEligibilityV1,
        objective: TaskObjectiveV1,
    ) -> Result<Self, String> {
        Self::from_initial_env(natural_ironclad_learning_env_with_neow_and_objective_v1(
            seed,
            ascension_level,
            neow_eligibility,
            objective,
        )?)
    }

    pub fn new_ironclad_a0(seed: u64) -> Result<Self, String> {
        Self::new_ironclad(seed, 0)
    }

    fn from_initial_env(env: LearningEnvV3) -> Result<Self, String> {
        let public_root = task_public_decision_v1(&env)?;
        let checkpoint = env.checkpoint();
        Ok(Self {
            env,
            live: LiveExactScopeV1 {
                root: SimulatorExactScopeRootV1 {
                    checkpoint,
                    public_root,
                    execution_evidence: ExactBranchingTaskExecutionEvidenceV1::default(),
                    captured_combat_sequences: BTreeSet::new(),
                },
            },
            pending: None,
            combat_plan_options: None,
            evidence_request: ExactBranchingTaskEvidenceRequestV1::default(),
            execution_evidence: ExactBranchingTaskExecutionEvidenceV1::default(),
            captured_combat_sequences: BTreeSet::new(),
            last_action_result: None,
        })
    }

    /// Starts an exact research scope from a session whose production-combat
    /// identity has already been checked by the combat-case restoration
    /// boundary. The public factory below owns that validation.
    pub fn from_verified_production_session_v1(
        session: super::RunControlSession,
    ) -> Result<Self, String> {
        Self::from_initial_env(LearningEnvV3::from_session(session))
    }

    /// Starts a new exact-run lineage from a schema-independent run-control
    /// checkpoint. The caller owns the new task/archive provenance; no prior
    /// public decision or task-kernel history enters this world.
    #[doc(hidden)]
    pub fn from_run_decision_checkpoint_v1(
        checkpoint: super::RunControlSessionCheckpointV1,
    ) -> Result<Self, String> {
        Self::from_verified_production_session_v1(checkpoint.into_session()?)
    }

    /// Rebuilds the execution shell around one already validated durable
    /// scope checkpoint. Unlike a reroot, this preserves the checkpoint's
    /// lineage for ordinary same-schema workspace reopen.
    #[doc(hidden)]
    pub fn from_durable_scope_checkpoint_v1(
        checkpoint: SimulatorExactScopeCheckpointV1,
    ) -> Result<Self, String> {
        let root = SimulatorExactScopeRootV1::from_durable_checkpoint_v1(checkpoint);
        let env = LearningEnvV3::from_checkpoint(root.checkpoint.clone())?;
        let restored = task_public_decision_v1(&env)?;
        if !root.public_root.replay_compatible_v2(&restored) {
            return Err("durable exact scope checkpoint changed its public root".to_string());
        }
        Ok(Self {
            env,
            live: LiveExactScopeV1 { root },
            pending: None,
            combat_plan_options: None,
            evidence_request: ExactBranchingTaskEvidenceRequestV1::default(),
            execution_evidence: ExactBranchingTaskExecutionEvidenceV1::default(),
            captured_combat_sequences: BTreeSet::new(),
            last_action_result: None,
        })
    }

    /// Captures only the current exact simulator session for an owning
    /// run-play reroot adapter. Public task history and the stored public root
    /// are deliberately excluded.
    #[doc(hidden)]
    pub fn current_run_decision_checkpoint_v1(
        &self,
    ) -> Result<super::RunControlSessionCheckpointV1, String> {
        if self.pending.is_some() {
            return Err("run decision checkpoint requires a public decision".to_string());
        }
        task_public_decision_v1(&self.env)?;
        Ok(self.env.session_checkpoint())
    }

    pub fn with_combat_research_plans(
        mut self,
        options: CombatResearchPlanOptionsV1,
    ) -> Result<Self, String> {
        options.validate()?;
        self.combat_plan_options = Some(options);
        Ok(self)
    }

    pub fn with_execution_evidence_request(
        mut self,
        request: ExactBranchingTaskEvidenceRequestV1,
    ) -> Result<Self, String> {
        self.evidence_request = request;
        self.capture_requested_execution_evidence()?;
        self.refresh_live_root_evidence_snapshot();
        Ok(self)
    }

    /// Reconstructs the live simulator through its normalized checkpoint
    /// instead of recursively cloning the full run state. This process-local
    /// fork is used only by the exact run-play controller; it does not create
    /// a durable branch point or expose private state to an actor.
    #[doc(hidden)]
    pub fn fork_for_hot_run_play_v1(&self) -> Result<Self, String> {
        if self.pending.is_some() {
            return Err("hot run-play fork requires a non-terminal exact decision".to_string());
        }
        let current = task_public_decision_v1(&self.env)?;
        let env = LearningEnvV3::from_checkpoint(self.env.checkpoint())?;
        let restored = task_public_decision_v1(&env)?;
        if !current.replay_compatible_v2(&restored) {
            return Err("hot run-play fork changed its current public decision".to_string());
        }
        Ok(Self {
            env,
            live: self.live.clone(),
            pending: None,
            combat_plan_options: self.combat_plan_options.clone(),
            evidence_request: self.evidence_request,
            execution_evidence: self.execution_evidence.clone(),
            captured_combat_sequences: self.captured_combat_sequences.clone(),
            last_action_result: None,
        })
    }

    /// Returns the transition result produced by the most recent exact action
    /// on this private fork. It is diagnostic evidence and never enters the
    /// model-facing task decision.
    #[doc(hidden)]
    pub fn last_action_result_for_run_play_v1(&self) -> Option<super::RunActionResultV1> {
        self.last_action_result.clone()
    }

    #[doc(hidden)]
    pub fn combat_monster_entities_for_run_play_v1(&self) -> Vec<(u8, crate::EntityId)> {
        self.env.session().combat_monster_entities_for_run_play_v1()
    }

    /// Returns one process-local exact combat position to the trusted
    /// run-play diagnostic owner. It is never serialized or exposed through
    /// the public actor surface.
    #[doc(hidden)]
    pub fn current_combat_position_for_run_play_v1(&self) -> Result<CombatPosition, String> {
        if self.pending.is_some() {
            return Err("run-play combat position requires a non-terminal decision".to_string());
        }
        self.env.session().current_active_combat_position()
    }

    /// Projects one exact same-turn input line back through the public task
    /// action surface. This trusted run-play adapter does not publish the
    /// private inputs; it returns replay-stable public choices only.
    #[doc(hidden)]
    pub fn public_combat_turn_plan_for_run_play_v1(
        &self,
        inputs: &[crate::state::core::ClientInput],
        provenance: crate::agent::task::TaskPrivilegedSupportProvenanceV1,
    ) -> Result<crate::agent::task::TaskResearchPlanV1, String> {
        super::combat_research_plan::exact_inputs_to_public_turn_plan_v1(
            &self.env, inputs, provenance,
        )
    }

    /// Replays one exact winning combat line to its real next public run
    /// boundary. The caller receives only the public resource outcome, never
    /// the forked environment or hidden RNG state.
    #[doc(hidden)]
    pub fn public_combat_replay_outcome_for_run_play_v1(
        &self,
        inputs: &[crate::state::core::ClientInput],
        provenance: crate::agent::task::TaskPrivilegedSupportProvenanceV1,
    ) -> Result<crate::agent::task::TaskResearchPlanOutcomeV1, String> {
        super::combat_research_plan::exact_inputs_to_public_replay_outcome_for_run_play_v1(
            &self.env, inputs, provenance,
        )
    }

    #[doc(hidden)]
    pub fn public_combat_replay_plan_and_outcome_for_run_play_v1(
        &self,
        inputs: &[crate::state::core::ClientInput],
        provenance: crate::agent::task::TaskPrivilegedSupportProvenanceV1,
    ) -> Result<
        (
            crate::agent::task::TaskResearchPlanV1,
            crate::agent::task::TaskResearchPlanOutcomeV1,
        ),
        String,
    > {
        super::combat_research_plan::exact_inputs_to_public_replay_plan_and_outcome_for_run_play_v1(
            &self.env, inputs, provenance,
        )
    }

    /// Runs the terminal and finite-milestone combat research projections in
    /// one resident portfolio. The finite prefixes remain separate from
    /// terminal candidates and are never installed by this read-only owner.
    #[doc(hidden)]
    pub fn combat_research_portfolio_for_run_play_v1(
        &self,
        options: Option<&CombatResearchPlanOptionsV1>,
    ) -> Result<CombatResearchPortfolioV1, String> {
        let options = options
            .or(self.combat_plan_options.as_ref())
            .ok_or_else(|| "exact run-play has no combat research options".to_string())?;
        combat_research_portfolio_v1(&self.env, options)
    }

    #[doc(hidden)]
    pub fn combat_research_portfolio_for_run_play_with_progress_v1(
        &self,
        options: Option<&CombatResearchPlanOptionsV1>,
        deadline: Option<std::time::Instant>,
        progress: &mut dyn FnMut(CombatResearchProgressEventV1),
    ) -> Result<CombatResearchPortfolioV1, String> {
        let options = options
            .or(self.combat_plan_options.as_ref())
            .ok_or_else(|| "exact run-play has no combat research options".to_string())?;
        combat_research_portfolio_with_progress_v1(&self.env, options, deadline, progress)
    }

    /// Projects value-free public path-family facts for every currently legal
    /// map entry without exposing the private run session or selecting a
    /// route. This is valid before an actual Map decision, including Neow.
    pub fn public_route_entry_paths_for_run_play_v1(
        &self,
        path_budget: usize,
    ) -> Result<Vec<RouteEntryPathSummaryV1>, String> {
        if self.pending.is_some() {
            return Err("run-play route-entry paths require a non-terminal decision".to_string());
        }
        Ok(summarize_route_entry_paths_v1(
            &self.env.session().run_state,
            path_budget,
        ))
    }

    /// Captures the current exact run-play combat root in the canonical
    /// production case format. The caller receives no live session or private
    /// world handle, and capture does not advance the environment.
    #[doc(hidden)]
    pub fn current_combat_case_for_run_play_v1(&self) -> Result<CombatCase, String> {
        if self.pending.is_some() {
            return Err("run-play combat case requires a non-terminal decision".to_string());
        }
        let session = self.env.session();
        let active = session
            .active_combat
            .as_ref()
            .ok_or_else(|| "run-play combat case requires an active exact combat".to_string())?;
        let generation = usize::try_from(session.decision_step)
            .map_err(|_| "run-play combat case generation exceeds usize".to_string())?;
        let encounter_label = active
            .encounter_id
            .map(|encounter_id| encounter_id.stable_id().to_string())
            .unwrap_or_else(|| "unknown encounter".to_string());
        capture_production_combat_case_v2(
            session,
            CombatCaseSource {
                seed: session.run_state.seed,
                ascension: session.run_state.ascension_level,
                generation,
                branch_id: 0,
                parent_id: None,
            },
            CombatCaseGap {
                boundary: format!(
                    "Act {} Floor {} {} exact run-play combat root",
                    session.run_state.act_num, session.run_state.floor_num, encounter_label
                ),
                reason: "explicit run-play combat case export".to_string(),
                witness_budget: CombatCaseWitnessBudgetV1::NotRun,
            },
        )
    }

    /// Projects the existing typed route-policy audit for the current exact
    /// map boundary. This deliberately returns no session, checkpoint, RNG
    /// state, or executable simulator action.
    #[doc(hidden)]
    pub fn exact_route_policy_audit_for_run_play_v1(
        &self,
    ) -> Result<ExactRoutePolicyAuditV1, String> {
        self.exact_policy_audit_for_run_play_v1(
            TaskDecisionSiteV1::Map,
            "route",
            exact_route_policy_audit_v1,
        )
    }

    /// Projects the existing typed card-reward audit for the current exact
    /// public boundary without exposing executable or hidden state.
    #[doc(hidden)]
    pub fn exact_card_reward_policy_audit_for_run_play_v1(
        &self,
    ) -> Result<ExactCardRewardPolicyAuditV1, String> {
        self.exact_policy_audit_for_run_play_v1(
            TaskDecisionSiteV1::CardReward,
            "card-reward",
            exact_card_reward_policy_audit_v1,
        )
    }

    /// Projects the shared direct-card acquisition audit for event options
    /// such as The Library while excluding ambient potion actions.
    #[doc(hidden)]
    pub fn exact_direct_card_acquisition_policy_audit_for_run_play_v1(
        &self,
    ) -> Result<ExactCardRewardPolicyAuditV1, String> {
        self.exact_policy_audit_for_run_play_v1(
            TaskDecisionSiteV1::Event,
            "direct-card-acquisition",
            |session, legal| {
                let direct = legal
                    .iter()
                    .copied()
                    .filter(|candidate| {
                        matches!(
                            candidate.action,
                            super::RunDecisionAction::Input(
                                ClientInput::SelectEventOption(_) | ClientInput::EventChoice(_)
                            )
                        )
                    })
                    .collect::<Vec<_>>();
                exact_direct_card_acquisition_policy_audit_v1(session, &direct)
            },
        )
    }

    /// Projects the existing typed campfire audit for the current exact
    /// public boundary without exposing executable or hidden state.
    #[doc(hidden)]
    pub fn exact_campfire_policy_audit_for_run_play_v1(
        &self,
    ) -> Result<ExactCampfirePolicyAuditV1, String> {
        self.exact_policy_audit_for_run_play_v1(
            TaskDecisionSiteV1::Campfire,
            "campfire",
            exact_campfire_policy_audit_v1,
        )
    }

    /// Projects the existing typed boss-relic audit for the current exact
    /// public boundary without exposing executable or hidden state.
    #[doc(hidden)]
    pub fn exact_boss_relic_policy_audit_for_run_play_v1(
        &self,
    ) -> Result<ExactBossRelicPolicyAuditV1, String> {
        self.exact_policy_audit_for_run_play_v1(
            TaskDecisionSiteV1::BossRelic,
            "boss-relic",
            exact_boss_relic_policy_audit_v1,
        )
    }

    /// Projects the existing typed shop audit for the current exact public
    /// boundary without exposing executable or hidden state.
    #[doc(hidden)]
    pub fn exact_shop_policy_audit_for_run_play_v1(
        &self,
    ) -> Result<ExactShopPolicyAuditV1, String> {
        self.exact_policy_audit_for_run_play_v1(
            TaskDecisionSiteV1::Shop,
            "shop",
            exact_shop_policy_audit_v1,
        )
    }

    /// Projects the typed deck-mutation owner audit for the current exact
    /// run-choice boundary without exposing card occurrence identities.
    #[doc(hidden)]
    pub fn exact_run_choice_policy_audit_for_run_play_v1(
        &self,
    ) -> Result<ExactRunChoicePolicyAuditV1, String> {
        self.exact_policy_audit_for_run_play_v1(
            TaskDecisionSiteV1::RunChoice,
            "run-choice",
            |session, _legal| exact_run_choice_policy_audit_v1(session),
        )
    }

    /// Projects the typed full-belt potion exchange audit at a reward screen.
    #[doc(hidden)]
    pub fn potion_reward_policy_audit_for_run_play_v1(
        &self,
    ) -> Result<Option<PotionRewardPolicyAuditV1>, String> {
        self.exact_policy_audit_for_run_play_v1(
            TaskDecisionSiteV1::Reward,
            "potion-reward",
            |session, _legal| Ok(potion_reward_policy_audit_v1(session)),
        )
    }

    fn exact_policy_audit_for_run_play_v1<T>(
        &self,
        expected_site: TaskDecisionSiteV1,
        owner: &str,
        project: impl FnOnce(
            &super::RunControlSession,
            &[RunPolicyCandidateV1<'_>],
        ) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.pending.is_some() {
            return Err(format!(
                "run-play {owner} audit requires a non-terminal decision"
            ));
        }
        let actual_site = task_public_decision_v1(&self.env)?.site;
        if actual_site != expected_site {
            return Err(format!(
                "run-play {owner} audit requires {expected_site:?}, got {actual_site:?}"
            ));
        }
        let session = self.env.session();
        let surface = build_decision_surface(session);
        let legal = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect::<Vec<_>>();
        project(session, &legal)
    }

    #[cfg(test)]
    fn from_test_session(session: super::RunControlSession) -> Result<Self, String> {
        Self::from_initial_env(LearningEnvV3::from_session(session))
    }

    fn reload_scope(&self) -> ReloadScopeV1<SimulatorExactScopeRootV1> {
        ReloadScopeV1 {
            public_root: self.live.root.public_root.clone(),
            restore_anchor: self.live.root.clone(),
            reload_permission: ReloadPermissionV1::ReloadToExactScopeRoot,
        }
    }

    fn rebase_current_position(
        &mut self,
    ) -> Result<ReloadScopeV1<SimulatorExactScopeRootV1>, String> {
        let public_root = task_public_decision_v1(&self.env)?;
        let checkpoint = self.env.checkpoint();
        self.live = LiveExactScopeV1 {
            root: SimulatorExactScopeRootV1 {
                checkpoint,
                public_root,
                execution_evidence: self.execution_evidence.clone(),
                captured_combat_sequences: self.captured_combat_sequences.clone(),
            },
        };
        self.pending = None;
        Ok(self.reload_scope())
    }

    fn capture_requested_execution_evidence(&mut self) -> Result<(), String> {
        if self.evidence_request.combat_entry_encounter.is_none()
            && !self.evidence_request.capture_all_combat_entries
        {
            return Ok(());
        }
        let Some(active) = self.env.session().active_combat.as_ref() else {
            return Ok(());
        };
        if active.combat_state.turn.turn_count != 0 {
            return Ok(());
        }
        let requested_encounter = self.evidence_request.combat_entry_encounter;
        let capture_requested_encounter = requested_encounter.is_some_and(|encounter_id| {
            self.execution_evidence.captured_combat_case.is_none()
                && active.encounter_id == Some(encounter_id)
        });
        let combat_sequence = self.env.session().combat_sequence();
        let capture_run_entry = self.evidence_request.capture_all_combat_entries
            && !self.captured_combat_sequences.contains(&combat_sequence);
        if !capture_requested_encounter && !capture_run_entry {
            return Ok(());
        }
        let session = self.env.session();
        let generation = usize::try_from(session.decision_step)
            .map_err(|_| "combat case capture decision step exceeds usize".to_owned())?;
        let encounter_label = active
            .encounter_id
            .map(|encounter_id| encounter_id.stable_id().to_string())
            .unwrap_or_else(|| "unknown encounter".to_owned());
        let case = capture_production_combat_case_v2(
            session,
            CombatCaseSource {
                seed: session.run_state.seed,
                ascension: session.run_state.ascension_level,
                generation,
                branch_id: 0,
                parent_id: None,
            },
            CombatCaseGap {
                boundary: format!(
                    "Act {} Floor {} {} public actor combat entry",
                    session.run_state.act_num, session.run_state.floor_num, encounter_label
                ),
                reason: "episode execution evidence capture".to_owned(),
                witness_budget: CombatCaseWitnessBudgetV1::NotRun,
            },
        )?;
        if capture_requested_encounter {
            self.execution_evidence.captured_combat_case = Some(case.clone());
        }
        if capture_run_entry {
            let decision = task_public_decision_v1(&self.env)?;
            if decision.site != TaskDecisionSiteV1::Combat {
                return Err("combat entry capture did not project a combat decision".to_owned());
            }
            self.execution_evidence.captured_combat_entries.push(
                ExactBranchingCombatEntryEvidenceV1 {
                    root_public_snapshot_id: decision.information.snapshot_id,
                    case,
                },
            );
            self.captured_combat_sequences.insert(combat_sequence);
        }
        Ok(())
    }

    fn refresh_live_root_evidence_snapshot(&mut self) {
        self.live.root.execution_evidence = self.execution_evidence.clone();
        self.live.root.captured_combat_sequences = self.captured_combat_sequences.clone();
    }
}

impl ExactBranchingTaskExecutionEvidenceV1 {
    /// Resolves only the task kernel's final committed public history back to
    /// exact combat inputs. Reloaded or otherwise abandoned attempts never
    /// enter this evidence.
    pub fn materialize_committed_combats(
        mut self,
        committed_history: &CommittedPublicHistoryV1,
    ) -> Result<Self, String> {
        self.committed_combats = self
            .captured_combat_entries
            .iter()
            .map(|entry| committed_combat_evidence_v1(entry, committed_history))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(self)
    }
}

fn committed_combat_evidence_v1(
    entry: &ExactBranchingCombatEntryEvidenceV1,
    committed_history: &CommittedPublicHistoryV1,
) -> Result<Option<ExactBranchingCommittedCombatEvidenceV1>, String> {
    let matching_steps = committed_history
        .steps
        .iter()
        .enumerate()
        .filter(|step| {
            step.1.site == TaskDecisionSiteV1::Combat
                && step.1.step.source.information.snapshot_id == entry.root_public_snapshot_id
        })
        .collect::<Vec<_>>();
    if matching_steps.is_empty() {
        return Ok(None);
    }
    let [(root_step_index, root_step)] = matching_steps.as_slice() else {
        return Err(format!(
            "captured combat root {} appears {} times in committed history",
            entry.root_public_snapshot_id,
            matching_steps.len()
        ));
    };
    let scope = root_step.scope;
    let steps = committed_history
        .steps
        .iter()
        .skip(*root_step_index)
        .take_while(|step| step.scope == scope && step.site == TaskDecisionSiteV1::Combat)
        .collect::<Vec<_>>();
    if steps.is_empty() {
        return Err(format!(
            "captured combat root {} has no committed combat suffix",
            entry.root_public_snapshot_id
        ));
    }

    let session = restore_combat_case_production_session_v2(
        &entry.case.core,
        entry.case.production_context.as_ref(),
    )?;
    let mut replay = LearningEnvV3::from_session(session);
    let mut actions = Vec::with_capacity(steps.len());
    for (step_index, committed) in steps.into_iter().enumerate() {
        let current = task_public_decision_v1(&replay)?;
        if current != committed.step.source {
            return Err(format!(
                "captured combat root {} diverged at committed step {step_index}",
                entry.root_public_snapshot_id
            ));
        }
        let prepared = prepare_task_choice_v1(&replay, &committed.step.choice)?;
        let LearningPreparedActionV3::CombatInput { input } = &prepared else {
            return Err(format!(
                "captured combat root {} resolved a non-combat committed action",
                entry.root_public_snapshot_id
            ));
        };
        actions.push(input.clone());
        replay.step_prepared(prepared)?;
    }
    let outcome = replay
        .session()
        .last_combat_baseline()
        .cloned()
        .ok_or_else(|| {
            format!(
                "captured combat root {} did not complete during its committed scope",
                entry.root_public_snapshot_id
            )
        })?;
    if outcome.terminal != CombatTerminal::Win {
        return Err(format!(
            "captured combat root {} committed a non-winning combat outcome {:?}",
            entry.root_public_snapshot_id, outcome.terminal
        ));
    }
    Ok(Some(ExactBranchingCommittedCombatEvidenceV1 {
        root_public_snapshot_id: entry.root_public_snapshot_id.clone(),
        case: entry.case.clone(),
        actions,
        outcome,
    }))
}

impl TaskPrivilegedSupportWorldV1 for ExactBranchingTaskWorldV1 {
    fn privileged_support(&mut self) -> Result<TaskPrivilegedSupportV1, Self::Error> {
        if self.pending.is_some() {
            return Err("privileged support requested with a pending exact scope exit".to_string());
        }
        let public_root = task_public_decision_v1(&self.env)?;
        match public_root.site {
            crate::agent::task::TaskDecisionSiteV1::Combat => {
                let Some(options) = self.combat_plan_options.as_ref() else {
                    return Err("exact branching world has no combat support producer".to_string());
                };
                let attempt = combat_research_plan_v1(&self.env, options)?;
                let support = sts_oracle_strategic_owner::privileged_combat_research_support_v1(
                    &public_root,
                    attempt,
                )?;
                Ok(TaskPrivilegedSupportV1::CombatResearchPlan { support })
            }
            _ => Ok(TaskPrivilegedSupportV1::StrategicChoice {
                support: privileged_strategic_support_v1(&self.env, &public_root)?,
            }),
        }
    }
}

impl TaskExecutionEvidenceWorldV1 for ExactBranchingTaskWorldV1 {
    type Evidence = ExactBranchingTaskExecutionEvidenceV1;

    fn take_execution_evidence(&mut self) -> Self::Evidence {
        std::mem::take(&mut self.execution_evidence)
    }
}

impl TaskWorldV1 for ExactBranchingTaskWorldV1 {
    type ScopeAnchor = SimulatorExactScopeRootV1;
    type Error = String;

    fn initial_scope(&self) -> Result<ReloadScopeV1<Self::ScopeAnchor>, Self::Error> {
        Ok(self.reload_scope())
    }

    fn restore_scope_anchor(
        &mut self,
        anchor: &Self::ScopeAnchor,
    ) -> Result<TaskPublicDecisionV1, Self::Error> {
        self.pending = None;
        let restored_env = LearningEnvV3::from_checkpoint(anchor.checkpoint.clone())?;
        let restored = task_public_decision_v1(&restored_env)?;
        if !anchor.public_root.replay_compatible_v2(&restored) {
            return Err("exact branching restore changed its public scope root".to_string());
        }
        self.env = restored_env;
        self.execution_evidence = anchor.execution_evidence.clone();
        self.captured_combat_sequences = anchor.captured_combat_sequences.clone();
        self.last_action_result = None;
        self.live = LiveExactScopeV1 {
            root: anchor.clone(),
        };
        self.capture_requested_execution_evidence()?;
        Ok(restored)
    }

    fn explore_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldExplorationV1, Self::Error> {
        if self.pending.is_some() {
            return Err("exact branching task already has a pending scope exit".to_string());
        }
        let prepared = prepare_task_choice_v1(&self.env, choice)?;
        let mut trial = self.env.clone();
        let (step, action_result) = trial.step_prepared_with_action_result_v1(prepared)?;
        self.last_action_result = action_result;
        match step.boundary {
            LearningBoundaryV1::Terminal { outcome } => {
                let game_outcome = game_outcome_for_learning_terminal_v1(&trial, outcome.result);
                self.pending = Some(PendingExactScopeExitV1::GameTerminal {
                    env: trial,
                    game_outcome,
                });
                Ok(TaskWorldExplorationV1::ExploredScopeExit(
                    TaskExploredScopeExitV1::GameTerminal { game_outcome },
                ))
            }
            LearningBoundaryV1::Unsupported => {
                Err("exact branching task reached an unsupported boundary".to_string())
            }
            LearningBoundaryV1::Strategic { .. } | LearningBoundaryV1::Combat { .. } => {
                let decision = task_public_decision_v1(&trial)?;
                self.env = trial;
                self.capture_requested_execution_evidence()?;
                Ok(TaskWorldExplorationV1::Decision(decision))
            }
        }
    }

    fn commit_explored_scope(
        &mut self,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        match self.pending.take() {
            Some(PendingExactScopeExitV1::GameTerminal { env, game_outcome }) => {
                self.env = env;
                Ok(TaskWorldCommitV1::Terminal(game_outcome))
            }
            None => Ok(TaskWorldCommitV1::NextScope(
                self.rebase_current_position()?,
            )),
        }
    }

    fn commit_choice(
        &mut self,
        _choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        Err("exact branching world requires provisional exploration before commit".to_string())
    }
}

pub fn ironclad_a0_exact_branching_task_spec_v1() -> TaskSpecV1 {
    ironclad_exact_branching_task_spec_v1(0)
        .expect("Ironclad A0 exact task specification must be valid")
}

pub fn ironclad_exact_branching_task_spec_v1(ascension_level: u8) -> Result<TaskSpecV1, String> {
    ironclad_exact_branching_task_spec_with_objective_v1(
        ascension_level,
        TaskObjectiveV1::ActThreeVictory,
    )
}

pub fn ironclad_exact_branching_task_spec_with_objective_v1(
    ascension_level: u8,
    objective: TaskObjectiveV1,
) -> Result<TaskSpecV1, String> {
    let mut task = TaskSpecV1::ironclad_a0_act_three_fail_closed();
    task.ascension_level = ascension_level;
    task.objective = objective;
    task.reload_protocol.protocol_name = "simulator_exact_branching_scopes_v1".to_string();
    task.reload_protocol.authorization = ReloadAuthorizationV1::ExactBranchingScopes {
        simulator_contract_reference:
            "LearningEnvV3 exact checkpoint/restore and typed task choice execution".to_string(),
    };
    task.validate()?;
    Ok(task)
}

pub fn ironclad_a0_exact_branching_environment_v1(
    seed: u64,
    budget: TaskBudgetV1,
) -> Result<NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>, String> {
    NodeReloadTaskEnvironmentV1::new(
        ExactBranchingTaskWorldV1::new_ironclad_a0(seed)?,
        ironclad_a0_exact_branching_task_spec_v1(),
        budget,
    )
}

pub fn ironclad_a0_privileged_reference_environment_v1(
    seed: u64,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>, String> {
    ironclad_privileged_reference_environment_v1(seed, 0, budget, plan_options, evidence_request)
}

pub fn ironclad_privileged_reference_environment_v1(
    seed: u64,
    ascension_level: u8,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>, String> {
    ironclad_privileged_reference_environment_with_neow_v1(
        seed,
        ascension_level,
        NeowEligibilityV1::Rich,
        budget,
        plan_options,
        evidence_request,
    )
}

pub fn ironclad_privileged_reference_environment_with_neow_v1(
    seed: u64,
    ascension_level: u8,
    neow_eligibility: NeowEligibilityV1,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>, String> {
    ironclad_privileged_reference_environment_with_neow_and_objective_v1(
        seed,
        ascension_level,
        neow_eligibility,
        TaskObjectiveV1::ActThreeVictory,
        budget,
        plan_options,
        evidence_request,
    )
}

pub fn ironclad_privileged_reference_environment_with_neow_and_objective_v1(
    seed: u64,
    ascension_level: u8,
    neow_eligibility: NeowEligibilityV1,
    objective: TaskObjectiveV1,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>, String> {
    let mut task =
        ironclad_exact_branching_task_spec_with_objective_v1(ascension_level, objective)?;
    task.information_access = crate::agent::task::TaskInformationAccessV1::
        PublicHistoryLegalReloadMemoryAndPrivilegedSupport;
    NodeReloadTaskEnvironmentV1::new(
        ExactBranchingTaskWorldV1::new_ironclad_with_neow_and_objective(
            seed,
            ascension_level,
            neow_eligibility,
            objective,
        )?
        .with_combat_research_plans(plan_options)?
        .with_execution_evidence_request(evidence_request)?,
        task,
        budget,
    )
}

pub fn ironclad_a0_production_combat_case_privileged_reference_environment_v1(
    case: &CombatCase,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>, String> {
    let (mut task, world) = ironclad_a0_production_combat_case_task_world_v1(case)?;
    task.information_access = crate::agent::task::TaskInformationAccessV1::
        PublicHistoryLegalReloadMemoryAndPrivilegedSupport;
    task.validate()?;
    NodeReloadTaskEnvironmentV1::new(
        world
            .with_combat_research_plans(plan_options)?
            .with_execution_evidence_request(evidence_request)?,
        task,
        budget,
    )
}

pub fn ironclad_a0_production_combat_case_exact_branching_environment_v1(
    case: &CombatCase,
    budget: TaskBudgetV1,
) -> Result<NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>, String> {
    let (task, world) = ironclad_a0_production_combat_case_task_world_v1(case)?;
    NodeReloadTaskEnvironmentV1::new(world, task, budget)
}

fn ironclad_a0_production_combat_case_task_world_v1(
    case: &CombatCase,
) -> Result<(TaskSpecV1, ExactBranchingTaskWorldV1), String> {
    let identity = case.replay_identity_v2()?;
    let run_session_fingerprint = identity
        .run_session_fingerprint
        .clone()
        .ok_or_else(|| "combat research resume requires exact production state".to_string())?;
    let session =
        restore_combat_case_production_session_v2(&case.core, case.production_context.as_ref())?;
    if session.run_state.player_class != "Ironclad" || session.run_state.ascension_level != 0 {
        return Err(format!(
            "combat research case resume currently requires Ironclad A0, got {} A{}",
            session.run_state.player_class, session.run_state.ascension_level
        ));
    }
    let mut task = ironclad_a0_exact_branching_task_spec_v1();
    task.initial_condition = TaskInitialConditionV1::ExactProductionCombatCase {
        source_seed: case.core.source.seed,
        act: case.core.run.act,
        floor: case.core.run.floor,
        root_exact_state_hash: identity.root_exact_state_hash,
        run_session_fingerprint,
    };
    task.validate()?;
    Ok((
        task,
        ExactBranchingTaskWorldV1::from_verified_production_session_v1(session)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::information::action::PublicCombatAtomicActionV1;
    use crate::agent::task::{
        NodeAttemptStatusV1, TaskActorBoundaryV1, TaskDecisionCommandV1, TaskDecisionSiteV1,
        TaskEpisodeFailureV1, TaskEpisodeOutcomeV1, TaskPublicCandidateV1,
    };
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::eval::combat_case::{CombatCaseGap, CombatCaseSource, CombatCaseWitnessBudgetV1};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;

    fn budget() -> TaskBudgetV1 {
        TaskBudgetV1 {
            max_world_transitions: 128,
            max_reloads: 16,
        }
    }

    fn command(
        env: &NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>,
        choice: TaskPublicChoiceV1,
    ) -> TaskDecisionCommandV1 {
        env.decision_handle()
            .expect("decision handle")
            .into_command(choice)
    }

    fn exact_a0_production_combat_case() -> CombatCase {
        let mut session = super::super::RunControlSession::new(super::super::RunControlConfig {
            seed: 71,
            ascension_level: 0,
            player_class: "Ironclad",
            ..super::super::RunControlConfig::default()
        });
        let mut combat = crate::test_support::blank_test_combat();
        combat.turn.turn_count = 0;
        combat.zones.hand = vec![CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            51,
        )];
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.set_planned_move_id(1);
        let plan =
            crate::content::monsters::exordium::jaw_worm::JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 7;
        session.run_state.current_hp = combat.entities.player.current_hp;
        session.run_state.max_hp = combat.entities.player.max_hp;
        session.run_state.gold = combat.entities.player.gold;
        session.run_state.master_deck = combat.meta.master_deck_snapshot.to_vec();
        session.run_state.relics = combat.entities.player.relics.clone();
        session.run_state.potions = combat.entities.potions.clone();
        session.run_state.rng_pool = combat.rng.pool.clone();
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            EngineState::CombatPlayerTurn,
            combat,
            crate::content::monsters::factory::EncounterId::JawWorm,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        capture_production_combat_case_v2(
            &session,
            CombatCaseSource {
                seed: 71,
                ascension: 0,
                generation: 1,
                branch_id: 1,
                parent_id: None,
            },
            CombatCaseGap {
                boundary: "fixture".to_string(),
                reason: "exact production resume contract".to_string(),
                witness_budget: CombatCaseWitnessBudgetV1::NotRun,
            },
        )
        .expect("capture exact production case")
    }

    #[test]
    fn durable_checkpoint_identity_reports_the_mismatched_public_component() {
        let case = exact_a0_production_combat_case();
        let (_task, world) = ironclad_a0_production_combat_case_task_world_v1(&case)
            .expect("production combat case world");
        let mut checkpoint = world.live.root.durable_checkpoint_v1();
        assert!(!checkpoint.public_root.candidates.is_empty());
        checkpoint.public_root.candidates.clear();

        let error = checkpoint
            .validated_identity_v1()
            .expect_err("changed public candidates must be rejected");
        assert_eq!(
            error,
            "durable exact scope checkpoint changed its public root at candidates.length"
        );
    }

    #[test]
    fn encounter_entry_capture_is_identity_preserving_and_search_independent() {
        let case = exact_a0_production_combat_case();
        let options = CombatResearchPlanOptionsV1 {
            max_generation_work: 1,
            wall_ms: 1,
            quantum_generation_work: 1,
            quantum_wall_ms: 1,
            atomic_challenger: None,
            ..CombatResearchPlanOptionsV1::default()
        };
        let mut environment =
            ironclad_a0_production_combat_case_privileged_reference_environment_v1(
                &case,
                budget(),
                options,
                ExactBranchingTaskEvidenceRequestV1 {
                    combat_entry_encounter: Some(
                        crate::content::monsters::factory::EncounterId::JawWorm,
                    ),
                    ..ExactBranchingTaskEvidenceRequestV1::default()
                },
            )
            .expect("exact production environment");

        let captured = environment
            .take_execution_evidence()
            .captured_combat_case
            .expect("captured exact case");
        assert_eq!(
            captured
                .replay_identity_v2()
                .expect("captured replay identity")
                .root_exact_state_hash,
            case.replay_identity_v2()
                .expect("source replay identity")
                .root_exact_state_hash
        );
    }

    #[test]
    fn run_capture_keeps_uncommitted_combat_entries_out_of_execution_evidence() {
        let case = exact_a0_production_combat_case();
        let mut environment =
            ironclad_a0_production_combat_case_privileged_reference_environment_v1(
                &case,
                budget(),
                CombatResearchPlanOptionsV1::default(),
                ExactBranchingTaskEvidenceRequestV1 {
                    capture_all_combat_entries: true,
                    ..ExactBranchingTaskEvidenceRequestV1::default()
                },
            )
            .expect("exact production environment");

        let evidence = environment
            .take_execution_evidence()
            .materialize_committed_combats(&CommittedPublicHistoryV1::default())
            .expect("uncommitted capture remains diagnostic evidence");

        assert_eq!(evidence.captured_combat_entries.len(), 1);
        assert!(evidence.committed_combats.is_empty());
    }

    fn forward_choice(decision: &TaskPublicDecisionV1) -> TaskPublicChoiceV1 {
        for (ordinal, candidate) in decision.candidates.iter().enumerate() {
            match candidate {
                TaskPublicCandidateV1::Strategic {
                    action:
                        crate::agent::task::TaskStrategicActionV1::BeginRunCardSelection {
                            min_choices,
                            selectable_card_ordinals,
                            ..
                        },
                } if selectable_card_ordinals.len() >= *min_choices => {
                    let mut path = vec![ordinal];
                    path.extend(0..*min_choices);
                    return TaskPublicChoiceV1 { ordinal_path: path };
                }
                TaskPublicCandidateV1::Strategic {
                    action:
                        crate::agent::task::TaskStrategicActionV1::Proceed { .. }
                        | crate::agent::task::TaskStrategicActionV1::SkipCardReward { .. },
                } => return TaskPublicChoiceV1::one(ordinal),
                _ => {}
            }
        }
        TaskPublicChoiceV1::one(0)
    }

    fn explore_across_native_generation(
        env: &mut NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>,
    ) -> TaskPublicDecisionV1 {
        for _ in 0..16 {
            let TaskActorBoundaryV1::Decision { decision } = &env.actor_view().boundary else {
                panic!("Neow closure expected a decision")
            };
            let view = env
                .explore_choice(command(env, forward_choice(decision)))
                .expect("advance exact Neow branch");
            match view.boundary {
                TaskActorBoundaryV1::Decision { decision }
                    if decision.site == TaskDecisionSiteV1::Map =>
                {
                    return decision
                }
                TaskActorBoundaryV1::Decision { .. } => {}
                other => panic!("unexpected exact Neow boundary: {other:?}"),
            }
        }
        panic!("Neow closure did not reach its next exact scope")
    }

    #[test]
    fn native_generation_can_be_crossed_then_reloaded_before_one_prefix_is_committed() {
        let mut env = ironclad_a0_exact_branching_environment_v1(1, budget())
            .expect("natural exact branching environment");
        let root = env.actor_view().node_memory.unwrap().attempts[0]
            .root
            .clone();
        let observed_next = explore_across_native_generation(&mut env);
        assert_eq!(observed_next.site, TaskDecisionSiteV1::Map);
        assert!(env.actor_view().committed_history.steps.is_empty());

        let restored = env.reload().expect("restore exact Neow root");
        let memory = restored.node_memory.expect("exact node memory");
        assert_eq!(memory.attempts.last().expect("current attempt").root, root);
        assert_eq!(
            memory.attempts[0].status,
            NodeAttemptStatusV1::AbandonedByReload
        );
        assert!(restored.committed_history.steps.is_empty());

        let selected_next = explore_across_native_generation(&mut env);
        assert_eq!(selected_next, observed_next);
        let committed = env
            .commit_current_attempt()
            .expect("commit selected branch");
        assert!(matches!(
            committed.boundary,
            TaskActorBoundaryV1::Decision { ref decision } if decision == &observed_next
        ));
        assert_eq!(committed.committed_history.completed_scopes.len(), 1);
        assert!(!committed.committed_history.steps.is_empty());
    }

    #[test]
    fn production_combat_case_resume_keeps_exact_identity_and_public_root() {
        let case = exact_a0_production_combat_case();
        let identity = case.replay_identity_v2().expect("case identity");
        let restored =
            restore_combat_case_production_session_v2(&case.core, case.production_context.as_ref())
                .expect("restore case session");
        let expected_public_root = task_public_decision_v1(&LearningEnvV3::from_session(restored))
            .expect("expected public root");
        let environment = ironclad_a0_production_combat_case_privileged_reference_environment_v1(
            &case,
            budget(),
            CombatResearchPlanOptionsV1::default(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("case research environment");

        assert_eq!(
            environment.actor_view().task.initial_condition,
            TaskInitialConditionV1::ExactProductionCombatCase {
                source_seed: 71,
                act: 1,
                floor: 7,
                root_exact_state_hash: identity.root_exact_state_hash,
                run_session_fingerprint: identity
                    .run_session_fingerprint
                    .expect("production fingerprint"),
            }
        );
        assert!(matches!(
            &environment.actor_view().boundary,
            TaskActorBoundaryV1::Decision { decision } if decision == &expected_public_root
        ));
    }

    #[test]
    fn run_play_combat_case_export_preserves_production_replay_identity() {
        let case = exact_a0_production_combat_case();
        let expected = case.replay_identity_v2().expect("source case identity");
        let (_task, world) = ironclad_a0_production_combat_case_task_world_v1(&case)
            .expect("production combat case world");

        let exported = world
            .current_combat_case_for_run_play_v1()
            .expect("run-play combat case export");

        assert_eq!(
            exported.replay_identity_v2().expect("export identity"),
            expected
        );
        assert_eq!(exported.core.source.seed, case.core.source.seed);
        assert_eq!(exported.core.source.ascension, case.core.source.ascension);
    }

    #[test]
    fn formal_defeat_is_an_observed_exact_branch_and_remains_reloadable_until_commit() {
        let mut session =
            super::super::RunControlSession::new(super::super::RunControlConfig::default());
        session.run_state.event_state = None;
        session.run_state.current_hp = 1;
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.player.current_hp = 1;
        combat.entities.player.max_hp = 80;
        combat.zones.hand = vec![CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            51,
        )];
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.set_planned_move_id(1);
        let plan =
            crate::content::monsters::exordium::jaw_worm::JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            EngineState::CombatPlayerTurn,
            combat,
            crate::content::monsters::factory::EncounterId::JawWorm,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let world = ExactBranchingTaskWorldV1::from_test_session(session)
            .expect("lethal exact combat scope");
        let mut env = NodeReloadTaskEnvironmentV1::new(
            world,
            ironclad_a0_exact_branching_task_spec_v1(),
            budget(),
        )
        .expect("exact task environment");
        let TaskActorBoundaryV1::Decision { decision } = &env.actor_view().boundary else {
            panic!("expected combat decision")
        };
        let end_turn = decision
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatAtomic {
                        action: PublicCombatAtomicActionV1::EndTurn
                    }
                )
            })
            .expect("end turn candidate");

        let explored = env
            .explore_choice(command(&env, TaskPublicChoiceV1::one(end_turn)))
            .expect("observe lethal branch");
        assert!(matches!(
            explored.boundary,
            TaskActorBoundaryV1::ExploredScopeExit {
                exit: TaskExploredScopeExitV1::GameTerminal {
                    game_outcome: GameOutcomeV1::Defeat
                }
            }
        ));
        assert!(explored.committed_history.steps.is_empty());
        let restored = env.reload().expect("reload after observed defeat");
        assert!(matches!(
            restored.boundary,
            TaskActorBoundaryV1::Decision { .. }
        ));

        let selected = env
            .explore_choice(command(&env, TaskPublicChoiceV1::one(end_turn)))
            .expect("select lethal branch");
        assert!(matches!(
            selected.boundary,
            TaskActorBoundaryV1::ExploredScopeExit { .. }
        ));
        let committed = env
            .commit_current_attempt()
            .expect("commit selected defeat");
        assert!(matches!(
            committed.boundary,
            TaskActorBoundaryV1::Terminal {
                outcome: TaskEpisodeOutcomeV1::Failure {
                    reason: TaskEpisodeFailureV1::CommittedDefeat,
                    game_outcome: Some(GameOutcomeV1::Defeat),
                }
            }
        ));
        assert_eq!(committed.committed_history.steps.len(), 1);
        assert!(matches!(
            env.reload(),
            Err(crate::agent::task::TaskCommandErrorV1::EpisodeTerminal)
        ));
    }
}
