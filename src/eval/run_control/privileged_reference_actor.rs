use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::agent::information::run::PublicCombatRunContextV1;
use crate::agent::information::state::CombatLearningEnemyIdentityV1;
use crate::agent::task::{
    ReloadPermissionV1, TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorIntentV1, TaskActorV1,
    TaskActorViewV1, TaskCombatResearchPlanRunCommitmentV1, TaskCombatResearchPlanSelectionBasisV1,
    TaskCombatResearchPlanSelectionOutcomeV1, TaskCombatResearchPlanSelectionUnavailableV1,
    TaskDecisionSiteV1, TaskPrivilegedCombatResearchSupportV1,
    TaskPrivilegedStrategicSupportOutcomeV1, TaskPrivilegedStrategicSupportV1,
    TaskPrivilegedSupportActorV1, TaskPrivilegedSupportProvenanceV1, TaskPrivilegedSupportV1,
    TaskPublicCandidateV1, TaskPublicChoiceV1, TaskPublicObservationV1,
    TaskResearchPlanAttemptExitV1, TaskResearchPlanLaneAttemptV1, TaskResearchPlanOutcomeV1,
    TaskResearchPlanPendingRewardV1, TaskResearchPlanV1, TaskStrategicActionV1,
};
use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicState;

pub const PRIVILEGED_REFERENCE_ACTOR_NAME_V1: &str = "privileged_complete_run_reference_actor";
pub const PRIVILEGED_REFERENCE_ACTOR_REVISION_V1: u32 = 8;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchDeckCardV1 {
    pub card_id: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchPlanAttemptReceiptV1 {
    pub public_snapshot_id: String,
    /// Complete public run context seen at the combat root. `None` is reserved
    /// for receipts written before this audit field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_context: Option<PublicCombatRunContextV1>,
    pub act: Option<u8>,
    pub floor: Option<i32>,
    pub encounter_id: Option<EncounterId>,
    pub hp: i32,
    pub max_hp: i32,
    pub turn: u32,
    pub deck: Vec<CombatResearchDeckCardV1>,
    pub relics: Vec<RelicState>,
    pub potions: Vec<Option<PotionId>>,
    pub monsters: Vec<CombatLearningEnemyIdentityV1>,
    pub lanes: Vec<TaskResearchPlanLaneAttemptV1>,
    /// Aggregate attempt summary. This is `Witness` whenever any retained
    /// complete candidate exists; producer-specific stop reasons remain in
    /// `lanes`.
    pub exit: TaskResearchPlanAttemptExitV1,
    pub candidates: Vec<CombatResearchPlanCandidateReceiptV1>,
    /// Candidate ordinals surviving strict outcome dominance. This records a
    /// fact frontier, not a run-level resource preference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_dominated_candidate_ordinals: Vec<usize>,
    /// Run-level owner that made the selection or preserved ambiguity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_provenance: Option<TaskPrivilegedSupportProvenanceV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_commitment: Option<TaskCombatResearchPlanRunCommitmentV1>,
    pub selection: CombatResearchPlanSelectionV1,
    /// Exact ordinal chosen by the run-level owner. Receipts written before
    /// this field existed can still be read through provenance/length
    /// projection, but new evidence must not discard an identity the owner
    /// already possessed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_candidate_ordinal: Option<usize>,
    pub selected_supporting_provenances: Vec<TaskPrivilegedSupportProvenanceV1>,
    pub plan_steps: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchPlanCandidateReceiptV1 {
    pub supporting_provenances: Vec<TaskPrivilegedSupportProvenanceV1>,
    pub plan_steps: usize,
    pub satisfies_requested_quality: bool,
    pub outcome: TaskResearchPlanOutcomeV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatResearchPlanSelectionV1 {
    NoCandidate,
    SingleCandidate,
    UniqueNonDominatedCandidate,
    UniqueShortestExactOutcome,
    EqualOutcomePlanOrderPreference,
    CarryoverHpBandResourceParetoPreference,
    RewardedExitResourceParetoPreference,
    GuaranteedFullHealResourceParetoPreference,
    ImmediateCampfireRecoveryPotionRetention,
    ForcedCampfireMarginalHpPotionRetention,
    FullHpOverUnprimedCyclicProgress,
    NonCombatRestCorridorResourceParetoPreference,
    // Retained for reading the short-lived first revision of this policy. The
    // current selector does not use lexicographic HP-then-potion ordering.
    CarryoverHpBandThenPotionRetention,
    AmbiguousNonDominatedFrontier,
    // Retained for reading older receipts. The current selector never uses
    // producer construction order as a resource preference.
    OrderedNonDominatedReferencePreference,
    // Retained for reading older receipts. The current selector never emits
    // either resident-specific disposition.
    BoundedRelicProgressResidentRepair,
    UnresolvedResidentFallback,
}

struct ActiveResearchPlanV1 {
    plan: TaskResearchPlanV1,
    next_step: usize,
    root_turn: u32,
}

struct ActiveRunCommitmentV1 {
    commitment: TaskCombatResearchPlanRunCommitmentV1,
    next_route_index: usize,
    remaining_required_pending_rewards: Vec<TaskResearchPlanPendingRewardV1>,
}

impl ActiveRunCommitmentV1 {
    fn new(commitment: TaskCombatResearchPlanRunCommitmentV1) -> Self {
        let remaining_required_pending_rewards = match &commitment {
            TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest { .. } => Vec::new(),
            TaskCombatResearchPlanRunCommitmentV1::NonCombatRestCorridor {
                required_pending_rewards,
                ..
            } => required_pending_rewards.clone(),
        };
        Self {
            commitment,
            next_route_index: 0,
            remaining_required_pending_rewards,
        }
    }
}

/// Combines exact strategic-owner choices with replay-verified combat plans.
/// Missing support is reported explicitly; a legality bootstrap is never
/// substituted as a combat continuation or strategic reference decision.
pub struct PrivilegedReferenceTaskActorV1 {
    active: Option<ActiveResearchPlanV1>,
    attempted_public_roots: BTreeSet<String>,
    installed_plans: usize,
    unavailable_plans: usize,
    planned_actions: usize,
    attempts: Vec<CombatResearchPlanAttemptReceiptV1>,
    strategic_choice: Option<TaskPublicChoiceV1>,
    strategic_support: Vec<TaskPrivilegedStrategicSupportV1>,
    run_commitment: Option<ActiveRunCommitmentV1>,
    yield_at_combat_entry: Option<EncounterId>,
}

impl PrivilegedReferenceTaskActorV1 {
    pub fn new() -> Self {
        Self {
            active: None,
            attempted_public_roots: BTreeSet::new(),
            installed_plans: 0,
            unavailable_plans: 0,
            planned_actions: 0,
            attempts: Vec::new(),
            strategic_choice: None,
            strategic_support: Vec::new(),
            run_commitment: None,
            yield_at_combat_entry: None,
        }
    }

    /// Configures an evaluator-only control yield before privileged support is
    /// requested at the matching public combat entry. This lets evidence
    /// capture publish a failing root without changing or probing the actor's
    /// combat policy.
    pub fn yielding_at_combat_entry_v1(mut self, encounter_id: EncounterId) -> Self {
        self.yield_at_combat_entry = Some(encounter_id);
        self
    }

    pub fn installed_plans(&self) -> usize {
        self.installed_plans
    }

    pub fn planned_actions(&self) -> usize {
        self.planned_actions
    }

    pub fn unavailable_plans(&self) -> usize {
        self.unavailable_plans
    }

    pub fn attempts(&self) -> &[CombatResearchPlanAttemptReceiptV1] {
        &self.attempts
    }

    pub fn strategic_support(&self) -> &[TaskPrivilegedStrategicSupportV1] {
        &self.strategic_support
    }

    fn should_yield_at_combat_entry_v1(&self, view: &TaskActorViewV1) -> bool {
        let Some(requested_encounter) = self.yield_at_combat_entry else {
            return false;
        };
        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return false;
        };
        let TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } = &decision.observation
        else {
            return false;
        };
        observation.turn.turn_count == 0
            && matches!(
                run_context,
                PublicCombatRunContextV1::Available {
                    encounter_id: Some(encounter_id),
                    ..
                } if *encounter_id == requested_encounter
            )
    }
}

impl Default for PrivilegedReferenceTaskActorV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskActorV1 for PrivilegedReferenceTaskActorV1 {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::privileged_support_assisted(
            PRIVILEGED_REFERENCE_ACTOR_NAME_V1,
            PRIVILEGED_REFERENCE_ACTOR_REVISION_V1,
        )
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        if self.should_yield_at_combat_entry_v1(view) {
            return TaskActorIntentV1::YieldControl;
        }
        if let Some(choice) = self.strategic_choice.take() {
            return TaskActorIntentV1::Act { choice };
        }
        let Some(active) = self.active.as_mut() else {
            return match &view.boundary {
                TaskActorBoundaryV1::Decision { decision }
                    if decision.site == TaskDecisionSiteV1::Combat =>
                {
                    TaskActorIntentV1::UnsupportedDecisionSurface {
                        reason: self.combat_unavailable_reason(&decision.information.snapshot_id),
                    }
                }
                TaskActorBoundaryV1::Decision { decision } => {
                    TaskActorIntentV1::UnsupportedDecisionSurface {
                        reason: self
                            .strategic_support
                            .iter()
                            .rev()
                            .find(|support| {
                                support.root_public_snapshot_id == decision.information.snapshot_id
                            })
                            .and_then(|support| match support.outcome {
                                TaskPrivilegedStrategicSupportOutcomeV1::Unavailable { reason } => {
                                    Some(format!("strategic choice unavailable: {reason:?}"))
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| {
                                "strategic support has not supplied a choice for this root"
                                    .to_string()
                            }),
                    }
                }
                TaskActorBoundaryV1::CommitReady
                | TaskActorBoundaryV1::ExploredScopeExit { .. } => TaskActorIntentV1::CommitScope,
                TaskActorBoundaryV1::Terminal { .. } => {
                    TaskActorIntentV1::UnsupportedDecisionSurface {
                        reason: "task runner invoked privileged reference actor after terminal"
                            .to_string(),
                    }
                }
            };
        };
        if active.next_step == active.plan.steps.len() {
            self.active = None;
            return TaskActorIntentV1::CommitScope;
        }
        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "combat research plan expected a public decision".to_string(),
            };
        };
        let step = &active.plan.steps[active.next_step];
        if step.source_public_snapshot_id != decision.information.snapshot_id {
            return TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "combat research plan diverged from exact replay".to_string(),
            };
        }
        active.next_step += 1;
        self.planned_actions = self.planned_actions.saturating_add(1);
        TaskActorIntentV1::Act {
            choice: step.choice.clone(),
        }
    }
}

impl TaskPrivilegedSupportActorV1 for PrivilegedReferenceTaskActorV1 {
    fn needs_privileged_support(&mut self, view: &TaskActorViewV1) -> bool {
        if self.should_yield_at_combat_entry_v1(view) {
            return false;
        }
        if !matches!(
            view.reload_permission,
            Some(ReloadPermissionV1::ReloadToExactScopeRoot)
        ) {
            return false;
        }
        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return false;
        };
        if decision.site != TaskDecisionSiteV1::Combat {
            return self.active.is_none();
        }
        if let Some(active) = self.active.as_ref() {
            let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
                return false;
            };
            if !combat_plan_turn_refresh_due_v1(active.root_turn, observation.turn.turn_count) {
                return false;
            }
        }
        !self
            .attempted_public_roots
            .contains(&decision.information.snapshot_id)
    }

    fn receive_privileged_support(
        &mut self,
        view: &TaskActorViewV1,
        support: TaskPrivilegedSupportV1,
    ) -> Result<(), String> {
        match support {
            TaskPrivilegedSupportV1::StrategicChoice { support } => {
                support.validate(view)?;
                self.validate_run_commitment_v1(view, &support)?;
                self.strategic_choice = match &support.outcome {
                    TaskPrivilegedStrategicSupportOutcomeV1::Choice { choice } => {
                        Some(choice.clone())
                    }
                    TaskPrivilegedStrategicSupportOutcomeV1::Unavailable { .. } => None,
                };
                self.strategic_support.push(support);
                Ok(())
            }
            TaskPrivilegedSupportV1::CombatResearchPlan { support } => {
                self.receive_combat_research_plan_attempt(view, support)
            }
        }
    }
}

impl PrivilegedReferenceTaskActorV1 {
    fn combat_unavailable_reason(&self, snapshot_id: &str) -> String {
        let Some(attempt) = self
            .attempts
            .iter()
            .rev()
            .find(|attempt| attempt.public_snapshot_id == snapshot_id)
        else {
            return "combat support has not supplied a plan for this root".to_string();
        };
        match attempt.selection {
            CombatResearchPlanSelectionV1::AmbiguousNonDominatedFrontier => format!(
                "combat choice unresolved: {} complete plans, {} non-dominated candidates",
                attempt.candidates.len(),
                attempt.non_dominated_candidate_ordinals.len()
            ),
            CombatResearchPlanSelectionV1::NoCandidate => format!(
                "combat research found no complete plan in this attempt ({:?})",
                attempt.exit
            ),
            _ => "selected combat plan is unavailable at this root".to_string(),
        }
    }

    fn validate_run_commitment_v1(
        &mut self,
        view: &TaskActorViewV1,
        support: &TaskPrivilegedStrategicSupportV1,
    ) -> Result<(), String> {
        let Some(mut active) = self.run_commitment.take() else {
            return Ok(());
        };
        let TaskPrivilegedStrategicSupportOutcomeV1::Choice { choice } = &support.outcome else {
            self.run_commitment = Some(active);
            return Ok(());
        };
        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return Err("run commitment requires a public decision".to_string());
        };
        let candidate = decision
            .candidates
            .get(choice.ordinal_path[0])
            .ok_or_else(|| "run commitment choice is outside the public surface".to_string())?;
        let TaskPublicCandidateV1::Strategic { action } = candidate else {
            return Err("run commitment received a non-strategic candidate".to_string());
        };
        validate_committed_potion_action_v1(&active.commitment, action)?;
        match &active.commitment {
            TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest {
                destination_x,
                destination_y,
                ..
            } if active.next_route_index == 0 && decision.site == TaskDecisionSiteV1::Map => {
                if !matches!(
                    action,
                    TaskStrategicActionV1::ChooseRouteNode { x, y, .. }
                        if x == destination_x && y == destination_y
                ) {
                    return Err(
                        "strategic owner violated the committed immediate campfire route"
                            .to_string(),
                    );
                }
                active.next_route_index = 1;
                self.run_commitment = Some(active);
            }
            TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest { .. }
                if active.next_route_index == 1
                    && decision.site == TaskDecisionSiteV1::Campfire =>
            {
                if !matches!(action, TaskStrategicActionV1::Rest) {
                    return Err(
                        "strategic owner violated the committed immediate campfire rest"
                            .to_string(),
                    );
                }
            }
            TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest { .. }
                if active.next_route_index == 1 =>
            {
                return Err(
                    "immediate campfire commitment reached an unexpected strategic boundary"
                        .to_string(),
                );
            }
            TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest { .. } => {
                self.run_commitment = Some(active)
            }
            TaskCombatResearchPlanRunCommitmentV1::NonCombatRestCorridor { route, .. } => {
                match decision.site {
                    TaskDecisionSiteV1::Reward => {
                        match action {
                            TaskStrategicActionV1::ClaimReward { reward, .. } => {
                                if let Some(position) =
                                    active.remaining_required_pending_rewards.iter().position(
                                        |required| required.matches_planner_descriptor_v1(reward),
                                    )
                                {
                                    active.remaining_required_pending_rewards.remove(position);
                                }
                            }
                            TaskStrategicActionV1::OpenPendingRewards
                                if !active.remaining_required_pending_rewards.is_empty() =>
                            {
                                return Err("strategic owner skipped a recovery-corridor reward"
                                    .to_string());
                            }
                            _ => {}
                        }
                        self.run_commitment = Some(active);
                    }
                    TaskDecisionSiteV1::CardReward => {
                        self.run_commitment = Some(active);
                    }
                    TaskDecisionSiteV1::Map if active.next_route_index < route.len() => {
                        let destination = route[active.next_route_index];
                        if !matches!(
                            action,
                            TaskStrategicActionV1::ChooseRouteNode { x, y, .. }
                                if *x == destination.x && *y == destination.y
                        ) {
                            return Err(
                                "strategic owner violated the committed recovery corridor route"
                                    .to_string(),
                            );
                        }
                        active.next_route_index += 1;
                        self.run_commitment = Some(active);
                    }
                    TaskDecisionSiteV1::Shop
                        if active.next_route_index > 0 && active.next_route_index < route.len() =>
                    {
                        self.run_commitment = Some(active);
                    }
                    TaskDecisionSiteV1::Campfire if active.next_route_index == route.len() => {
                        if !active.remaining_required_pending_rewards.is_empty() {
                            return Err("recovery corridor reached rest without required rewards"
                                .to_string());
                        }
                        if !matches!(action, TaskStrategicActionV1::Rest) {
                            return Err(
                                "strategic owner violated the committed recovery corridor rest"
                                    .to_string(),
                            );
                        }
                    }
                    _ => {
                        return Err("recovery corridor reached an unexpected strategic boundary"
                            .to_string());
                    }
                }
            }
        }
        Ok(())
    }

    fn receive_combat_research_plan_attempt(
        &mut self,
        view: &TaskActorViewV1,
        support: TaskPrivilegedCombatResearchSupportV1,
    ) -> Result<(), String> {
        support.validate(view)?;
        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return Err("research plan installation requires a public decision".to_string());
        };
        let TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } = &decision.observation
        else {
            return Err(
                "research combat plan attempt belongs to a non-combat decision".to_string(),
            );
        };
        let TaskPrivilegedCombatResearchSupportV1 {
            selection_provenance,
            attempt,
            non_dominated_candidate_ordinals,
            selection: owner_selection,
            ..
        } = support;
        let exit = attempt.summary_exit();
        let (act, floor, encounter_id) = match run_context {
            PublicCombatRunContextV1::Available {
                act,
                floor,
                encounter_id,
                ..
            } => (Some(*act), Some(*floor), *encounter_id),
            PublicCombatRunContextV1::Unavailable { .. } => (None, None, None),
        };
        let (selected_candidate_ordinal, selection, run_commitment) = match owner_selection {
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal,
                basis,
                run_commitment,
            } => (
                Some(candidate_ordinal),
                match basis {
                    TaskCombatResearchPlanSelectionBasisV1::SingleCandidate => {
                        CombatResearchPlanSelectionV1::SingleCandidate
                    }
                    TaskCombatResearchPlanSelectionBasisV1::UniqueNonDominatedCandidate => {
                        CombatResearchPlanSelectionV1::UniqueNonDominatedCandidate
                    }
                    TaskCombatResearchPlanSelectionBasisV1::UniqueShortestExactOutcome => {
                        CombatResearchPlanSelectionV1::UniqueShortestExactOutcome
                    }
                    TaskCombatResearchPlanSelectionBasisV1::EqualOutcomePlanOrderPreference => {
                        CombatResearchPlanSelectionV1::EqualOutcomePlanOrderPreference
                    }
                    TaskCombatResearchPlanSelectionBasisV1::CarryoverHpBandResourceParetoPreference => {
                        CombatResearchPlanSelectionV1::CarryoverHpBandResourceParetoPreference
                    }
                    TaskCombatResearchPlanSelectionBasisV1::RewardedExitResourceParetoPreference => {
                        CombatResearchPlanSelectionV1::RewardedExitResourceParetoPreference
                    }
                    TaskCombatResearchPlanSelectionBasisV1::GuaranteedFullHealResourceParetoPreference => {
                        CombatResearchPlanSelectionV1::GuaranteedFullHealResourceParetoPreference
                    }
                    TaskCombatResearchPlanSelectionBasisV1::ImmediateCampfireRecoveryPotionRetention => {
                        CombatResearchPlanSelectionV1::ImmediateCampfireRecoveryPotionRetention
                    }
                    TaskCombatResearchPlanSelectionBasisV1::ForcedCampfireMarginalHpPotionRetention => {
                        CombatResearchPlanSelectionV1::ForcedCampfireMarginalHpPotionRetention
                    }
                    TaskCombatResearchPlanSelectionBasisV1::FullHpOverUnprimedCyclicProgress => {
                        CombatResearchPlanSelectionV1::FullHpOverUnprimedCyclicProgress
                    }
                    TaskCombatResearchPlanSelectionBasisV1::NonCombatRestCorridorResourceParetoPreference => {
                        CombatResearchPlanSelectionV1::NonCombatRestCorridorResourceParetoPreference
                    }
                },
                run_commitment,
            ),
            TaskCombatResearchPlanSelectionOutcomeV1::Unavailable { reason } => (
                None,
                match reason {
                    TaskCombatResearchPlanSelectionUnavailableV1::NoCandidate => {
                        CombatResearchPlanSelectionV1::NoCandidate
                    }
                    TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier => {
                        CombatResearchPlanSelectionV1::AmbiguousNonDominatedFrontier
                    }
                },
                None,
            ),
        };
        let selected_candidate = selected_candidate_ordinal
            .and_then(|candidate_ordinal| attempt.candidates.get(candidate_ordinal));
        let candidate_receipts = attempt
            .candidates
            .iter()
            .map(|candidate| CombatResearchPlanCandidateReceiptV1 {
                supporting_provenances: candidate.supporting_provenances.clone(),
                plan_steps: candidate.plan.steps.len(),
                satisfies_requested_quality: candidate.satisfies_requested_quality,
                outcome: candidate.outcome.clone(),
            })
            .collect();
        let selected_supporting_provenances = selected_candidate
            .map(|candidate| candidate.supporting_provenances.clone())
            .unwrap_or_default();
        let plan_steps = selected_candidate.map_or(0, |candidate| candidate.plan.steps.len());
        self.attempts.push(CombatResearchPlanAttemptReceiptV1 {
            public_snapshot_id: decision.information.snapshot_id.clone(),
            run_context: Some(run_context.clone()),
            act,
            floor,
            encounter_id,
            hp: observation.player.hp,
            max_hp: observation.player.max_hp,
            turn: observation.turn.turn_count,
            deck: observation
                .cards
                .master_deck
                .cards
                .iter()
                .map(|card| CombatResearchDeckCardV1 {
                    card_id: card.card_id,
                    upgrades: card.upgrades,
                })
                .collect(),
            relics: observation.player.relics.clone(),
            potions: observation
                .potions
                .iter()
                .map(|potion| potion.as_ref().map(|potion| potion.potion_id))
                .collect(),
            monsters: observation
                .monsters
                .iter()
                .map(|monster| monster.enemy)
                .collect(),
            lanes: attempt.lanes.clone(),
            exit,
            candidates: candidate_receipts,
            non_dominated_candidate_ordinals,
            selection_provenance: Some(selection_provenance),
            run_commitment: run_commitment.clone(),
            selection,
            selected_candidate_ordinal,
            selected_supporting_provenances,
            plan_steps,
        });
        self.attempted_public_roots
            .insert(decision.information.snapshot_id.clone());
        let Some(plan) = selected_candidate.map(|candidate| candidate.plan.clone()) else {
            self.unavailable_plans = self.unavailable_plans.saturating_add(1);
            return Ok(());
        };
        plan.validate_for_root(decision)?;
        self.run_commitment = run_commitment.map(ActiveRunCommitmentV1::new);
        self.installed_plans = self.installed_plans.saturating_add(1);
        self.active = Some(ActiveResearchPlanV1 {
            plan,
            next_step: 0,
            root_turn: observation.turn.turn_count,
        });
        Ok(())
    }
}

fn combat_plan_turn_refresh_due_v1(root_turn: u32, current_turn: u32) -> bool {
    current_turn != root_turn
}

fn validate_committed_potion_action_v1(
    commitment: &TaskCombatResearchPlanRunCommitmentV1,
    action: &TaskStrategicActionV1,
) -> Result<(), String> {
    let (retained, recoverable): (&[_], &[_]) = match commitment {
        TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest {
            retained_entry_potions,
            ..
        } => (retained_entry_potions, &[]),
        TaskCombatResearchPlanRunCommitmentV1::NonCombatRestCorridor {
            retained_entry_potions,
            recoverable_entry_potions,
            ..
        } => (retained_entry_potions, recoverable_entry_potions),
    };
    let matches = |slot: &usize,
                   potion: &PotionId,
                   expected: &[crate::agent::task::TaskRetainedEntryPotionV1]| {
        expected
            .iter()
            .any(|retained| retained.slot == *slot && retained.potion == *potion)
    };
    match action {
        TaskStrategicActionV1::UseRunPotion { slot, potion } if matches(slot, potion, retained) => {
            Err("strategic owner consumed a committed retained potion".to_string())
        }
        TaskStrategicActionV1::DiscardRunPotion { slot, potion }
            if matches(slot, potion, retained) || matches(slot, potion, recoverable) =>
        {
            Err("strategic owner discarded committed recovery capacity".to_string())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_episode_keeps_policy_absence_separate_from_broken_execution() {
        use crate::agent::task::{
            run_task_privileged_support_actor_episode_v1, NodeReloadTaskEnvironmentV1,
            TaskBudgetV1, TaskEpisodeFailureV1, TaskEpisodeOutcomeV1,
        };
        use crate::eval::run_control::{
            ironclad_a0_exact_branching_task_spec_v1, ExactBranchingTaskWorldV1, RunControlConfig,
            RunControlSession,
        };
        use crate::state::core::EngineState;
        use crate::state::rewards::{RewardItem, RewardState};
        for invalid_exit in [false, true] {
            let mut session = RunControlSession::new(RunControlConfig::default());
            session.run_state.event_state = None;
            session.reward_automation.claim_gold = false;
            session.run_state.potions = vec![Some(crate::content::potions::Potion::new(
                PotionId::WeakenPotion,
                1,
            ))];
            let mut reward = RewardState::new();
            if invalid_exit {
                reward.skippable = false;
            } else {
                reward.items.push(RewardItem::Gold { amount: 12 });
            }
            session.engine_state = EngineState::RewardScreen(reward);
            let world =
                ExactBranchingTaskWorldV1::from_verified_production_session_v1(session).unwrap();
            let mut task = ironclad_a0_exact_branching_task_spec_v1();
            task.information_access = crate::agent::task::TaskInformationAccessV1::PublicHistoryLegalReloadMemoryAndPrivilegedSupport;
            let mut env = NodeReloadTaskEnvironmentV1::new(
                world,
                task,
                TaskBudgetV1 {
                    max_world_transitions: 10,
                    max_reloads: 0,
                },
            )
            .unwrap();
            let mut actor = PrivilegedReferenceTaskActorV1::new();
            let episode =
                run_task_privileged_support_actor_episode_v1(&mut env, &mut actor).unwrap();
            assert_eq!(episode.act_intents, 0);
            assert_eq!(episode.reload_intents, 0);
            let TaskEpisodeOutcomeV1::Failure {
                reason,
                game_outcome,
            } = episode.outcome
            else {
                panic!("unfinished episode");
            };
            assert!(game_outcome.is_none());
            if invalid_exit {
                assert!(matches!(
                    reason,
                    TaskEpisodeFailureV1::MechanicsOrProtocolError { .. }
                ));
            } else {
                assert_eq!(reason, TaskEpisodeFailureV1::UnsupportedDecisionSurface);
                assert!(matches!(
                    actor.strategic_support.last().unwrap().outcome,
                    TaskPrivilegedStrategicSupportOutcomeV1::Unavailable { .. }
                ));
            }
        }
    }

    #[test]
    fn combat_plan_refreshes_once_the_public_player_turn_changes() {
        assert!(!combat_plan_turn_refresh_due_v1(3, 3));
        assert!(combat_plan_turn_refresh_due_v1(2, 3));
    }
}

pub fn privileged_reference_task_actor_v1() -> PrivilegedReferenceTaskActorV1 {
    PrivilegedReferenceTaskActorV1::new()
}
