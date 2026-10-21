use serde::{Deserialize, Serialize};

use crate::ai::planner_core::PlannerMapRoom;
use crate::content::potions::PotionId;

use super::{
    task_research_plan_non_dominated_candidate_ordinals_v1,
    task_research_plan_unique_shortest_exact_outcome_candidate_v1, TaskActorV1, TaskActorViewV1,
    TaskPublicChoiceV1, TaskResearchPlanAttemptV1, TaskResearchPlanPendingRewardV1, TaskWorldV1,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPrivilegedSupportAccessV1 {
    PrivilegedExactCombatWitness,
    PrivilegedExactStrategicOwner,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPrivilegedSupportProvenanceV1 {
    pub access: TaskPrivilegedSupportAccessV1,
    pub producer_name: String,
    pub producer_revision: u32,
}

impl TaskPrivilegedSupportProvenanceV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.producer_name.trim().is_empty() || self.producer_revision == 0 {
            return Err("privileged support has invalid producer provenance".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPrivilegedStrategicSupportUnavailableV1 {
    ExactOwnerUnavailable,
    OwnerCandidateNotRepresented,
    PlannerCandidateMissing,
    PlannerCandidateAmbiguous,
    SelectionPayloadUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskPrivilegedStrategicSupportOutcomeV1 {
    Choice {
        choice: TaskPublicChoiceV1,
    },
    Unavailable {
        reason: TaskPrivilegedStrategicSupportUnavailableV1,
    },
}

/// One exact strategic owner's decision after it has been projected onto the
/// unchanged public task surface. It contains no checkpoint, RNG state,
/// private candidate id, UUID, or executable simulator action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPrivilegedStrategicSupportV1 {
    pub root_public_snapshot_id: String,
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub outcome: TaskPrivilegedStrategicSupportOutcomeV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCombatResearchPlanSelectionBasisV1 {
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
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRetainedEntryPotionV1 {
    pub slot: usize,
    pub potion: PotionId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCommittedRouteNodeV1 {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskCombatResearchPlanRunCommitmentV1 {
    ImmediateCampfireRest {
        destination_x: i32,
        destination_y: i32,
        projected_hp_after_rest: i32,
        retained_entry_potions: Vec<TaskRetainedEntryPotionV1>,
    },
    NonCombatRestCorridor {
        route: Vec<TaskCommittedRouteNodeV1>,
        projected_hp_after_rest: i32,
        required_pending_rewards: Vec<TaskResearchPlanPendingRewardV1>,
        retained_entry_potions: Vec<TaskRetainedEntryPotionV1>,
        recoverable_entry_potions: Vec<TaskRetainedEntryPotionV1>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCombatResearchPlanSelectionUnavailableV1 {
    NoCandidate,
    AmbiguousNonDominatedFrontier,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskCombatResearchPlanSelectionOutcomeV1 {
    Choice {
        candidate_ordinal: usize,
        basis: TaskCombatResearchPlanSelectionBasisV1,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_commitment: Option<TaskCombatResearchPlanRunCommitmentV1>,
    },
    Unavailable {
        reason: TaskCombatResearchPlanSelectionUnavailableV1,
    },
}

/// A run-level owner's decision over replay-verified combat outcomes.
///
/// The search attempt remains evidence. The selection is a separate,
/// provenance-carrying commitment over its strict non-dominated frontier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPrivilegedCombatResearchSupportV1 {
    pub root_public_snapshot_id: String,
    pub selection_provenance: TaskPrivilegedSupportProvenanceV1,
    pub attempt: TaskResearchPlanAttemptV1,
    pub non_dominated_candidate_ordinals: Vec<usize>,
    pub selection: TaskCombatResearchPlanSelectionOutcomeV1,
}

impl TaskPrivilegedCombatResearchSupportV1 {
    pub fn validate(&self, view: &TaskActorViewV1) -> Result<(), String> {
        self.selection_provenance.validate()?;
        let super::TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return Err("combat research support requires a public decision".to_string());
        };
        if decision.site != super::TaskDecisionSiteV1::Combat {
            return Err("combat research support was delivered outside combat".to_string());
        }
        if self.root_public_snapshot_id != decision.information.snapshot_id {
            return Err("combat research support belongs to another public root".to_string());
        }
        self.attempt.validate()?;
        for candidate in &self.attempt.candidates {
            candidate.plan.validate_for_root(decision)?;
        }
        let expected_frontier =
            task_research_plan_non_dominated_candidate_ordinals_v1(&self.attempt.candidates);
        if self.non_dominated_candidate_ordinals != expected_frontier {
            return Err("combat research support frontier disagrees with outcomes".to_string());
        }
        match &self.selection {
            TaskCombatResearchPlanSelectionOutcomeV1::Choice {
                candidate_ordinal,
                basis,
                run_commitment,
            } => {
                if !self
                    .non_dominated_candidate_ordinals
                    .contains(candidate_ordinal)
                {
                    return Err(
                        "combat research support selected outside the fact frontier".to_string()
                    );
                }
                match basis {
                    TaskCombatResearchPlanSelectionBasisV1::SingleCandidate
                        if self.attempt.candidates.len() != 1 =>
                    {
                        return Err(
                            "single-candidate selection requires exactly one candidate".to_string(),
                        );
                    }
                    TaskCombatResearchPlanSelectionBasisV1::UniqueNonDominatedCandidate
                        if self.non_dominated_candidate_ordinals.len() != 1 =>
                    {
                        return Err(
                            "unique-frontier selection requires exactly one survivor".to_string(),
                        );
                    }
                    TaskCombatResearchPlanSelectionBasisV1::UniqueShortestExactOutcome
                        if task_research_plan_unique_shortest_exact_outcome_candidate_v1(
                            &self.attempt.candidates,
                            &self.non_dominated_candidate_ordinals,
                        ) != Some(*candidate_ordinal) =>
                    {
                        return Err(
                            "shortest-plan selection requires one unique shortest plan over exact-equal frontier outcomes"
                                .to_string(),
                        );
                    }
                    TaskCombatResearchPlanSelectionBasisV1::EqualOutcomePlanOrderPreference => {
                        let selected = &self.attempt.candidates[*candidate_ordinal];
                        if self.non_dominated_candidate_ordinals.len() < 2
                            || self.non_dominated_candidate_ordinals.iter().any(|other| {
                                let other = &self.attempt.candidates[*other];
                                other.outcome != selected.outcome
                                    || other.plan.steps.len() < selected.plan.steps.len()
                            })
                        {
                            return Err(
                                "plan-order selection requires a shortest plan over exact-equal frontier outcomes"
                                    .to_string(),
                            );
                        }
                    }
                    TaskCombatResearchPlanSelectionBasisV1::CarryoverHpBandResourceParetoPreference
                    | TaskCombatResearchPlanSelectionBasisV1::RewardedExitResourceParetoPreference
                    | TaskCombatResearchPlanSelectionBasisV1::GuaranteedFullHealResourceParetoPreference
                        if self.non_dominated_candidate_ordinals.len() < 2 =>
                    {
                        return Err(
                            "run-resource selection requires an incomparable frontier".to_string(),
                        );
                    }
                    TaskCombatResearchPlanSelectionBasisV1::ImmediateCampfireRecoveryPotionRetention
                        if self.non_dominated_candidate_ordinals.len() < 2
                            || run_commitment.is_none() =>
                    {
                        return Err(
                            "recovery-backed selection requires an incomparable frontier and commitment"
                                .to_string(),
                        );
                    }
                    TaskCombatResearchPlanSelectionBasisV1::FullHpOverUnprimedCyclicProgress
                        if self.non_dominated_candidate_ordinals.len() < 2 =>
                    {
                        return Err(
                            "full-HP run-resource selection requires an incomparable frontier"
                            .to_string(),
                        );
                    }
                    TaskCombatResearchPlanSelectionBasisV1::ForcedCampfireMarginalHpPotionRetention
                        if self.non_dominated_candidate_ordinals.len() < 2 =>
                    {
                        return Err(
                            "forced-campfire marginal-HP selection requires an incomparable frontier"
                                .to_string(),
                        );
                    }
                    TaskCombatResearchPlanSelectionBasisV1::NonCombatRestCorridorResourceParetoPreference
                        if self.non_dominated_candidate_ordinals.len() < 2
                            || !matches!(
                                run_commitment,
                                Some(TaskCombatResearchPlanRunCommitmentV1::NonCombatRestCorridor { .. })
                            ) =>
                    {
                        return Err(
                            "non-combat recovery selection requires an incomparable frontier and corridor commitment"
                                .to_string(),
                        );
                    }
                    _ => {}
                }
                let commitment_matches_basis = matches!(
                    (basis, run_commitment),
                    (
                        TaskCombatResearchPlanSelectionBasisV1::ImmediateCampfireRecoveryPotionRetention,
                        Some(TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest { .. })
                    ) | (
                        TaskCombatResearchPlanSelectionBasisV1::NonCombatRestCorridorResourceParetoPreference,
                        Some(TaskCombatResearchPlanRunCommitmentV1::NonCombatRestCorridor { .. })
                    )
                );
                if run_commitment.is_some() && !commitment_matches_basis {
                    return Err(
                        "combat run commitment does not match its recovery-backed selection basis"
                            .to_string(),
                    );
                }
                if let Some(run_commitment) = run_commitment {
                    validate_combat_run_commitment_v1(
                        decision,
                        &self.attempt,
                        &self.non_dominated_candidate_ordinals,
                        *candidate_ordinal,
                        run_commitment,
                    )?;
                }
            }
            TaskCombatResearchPlanSelectionOutcomeV1::Unavailable { reason } => match reason {
                TaskCombatResearchPlanSelectionUnavailableV1::NoCandidate
                    if !self.attempt.candidates.is_empty() =>
                {
                    return Err("no-candidate selection disagrees with attempt".to_string());
                }
                TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier
                    if self.non_dominated_candidate_ordinals.len() < 2 =>
                {
                    return Err(
                        "ambiguous selection requires multiple frontier candidates".to_string()
                    );
                }
                _ => {}
            },
        }
        Ok(())
    }
}

fn validate_combat_run_commitment_v1(
    decision: &super::TaskPublicDecisionV1,
    attempt: &TaskResearchPlanAttemptV1,
    frontier: &[usize],
    selected_candidate_ordinal: usize,
    commitment: &TaskCombatResearchPlanRunCommitmentV1,
) -> Result<(), String> {
    let super::TaskPublicObservationV1::Combat {
        run_context,
        observation,
    } = &decision.observation
    else {
        return Err("combat run commitment requires a combat observation".to_string());
    };
    let crate::agent::information::run::PublicCombatRunContextV1::Available { public_map, .. } =
        run_context
    else {
        return Err("combat run commitment requires public run context".to_string());
    };
    let selected = &attempt.candidates[selected_candidate_ordinal];
    match commitment {
        TaskCombatResearchPlanRunCommitmentV1::ImmediateCampfireRest {
            destination_x,
            destination_y,
            projected_hp_after_rest,
            retained_entry_potions,
        } => {
            let current = public_map
                .nodes
                .iter()
                .find(|node| node.x == public_map.current_x && node.y == public_map.current_y)
                .ok_or_else(|| "combat run commitment map root is unavailable".to_string())?;
            if !current.edges.iter().any(|edge| {
                edge.destination_x == *destination_x && edge.destination_y == *destination_y
            }) || !public_map.nodes.iter().any(|node| {
                node.x == *destination_x
                    && node.y == *destination_y
                    && node.room == PlannerMapRoom::Rest
            }) {
                return Err(
                    "combat run commitment destination is not an immediate campfire".to_string(),
                );
            }
            if *projected_hp_after_rest != selected.outcome.persistent.max_hp {
                return Err(
                    "combat run commitment projected HP disagrees with selected outcome"
                        .to_string(),
                );
            }
            validate_committed_entry_potions_v1(
                observation,
                attempt,
                frontier,
                selected_candidate_ordinal,
                retained_entry_potions,
                true,
            )?;
        }
        TaskCombatResearchPlanRunCommitmentV1::NonCombatRestCorridor {
            route,
            projected_hp_after_rest,
            required_pending_rewards,
            retained_entry_potions,
            recoverable_entry_potions,
        } => {
            if route.len() < 2 {
                return Err(
                    "non-combat recovery corridor requires a shop-to-rest route".to_string()
                );
            }
            let mut source_x = public_map.current_x;
            let mut source_y = public_map.current_y;
            for (index, destination) in route.iter().enumerate() {
                let source = public_map
                    .nodes
                    .iter()
                    .find(|node| node.x == source_x && node.y == source_y)
                    .ok_or_else(|| "combat recovery corridor source is unavailable".to_string())?;
                if !source.edges.iter().any(|edge| {
                    edge.destination_x == destination.x && edge.destination_y == destination.y
                }) {
                    return Err("combat recovery corridor contains a non-edge".to_string());
                }
                let node = public_map
                    .nodes
                    .iter()
                    .find(|node| node.x == destination.x && node.y == destination.y)
                    .ok_or_else(|| {
                        "combat recovery corridor destination is unavailable".to_string()
                    })?;
                let expected_room = if index + 1 == route.len() {
                    PlannerMapRoom::Rest
                } else {
                    PlannerMapRoom::Shop
                };
                if node.room != expected_room {
                    return Err(
                        "combat recovery corridor may contain only shops followed by a rest"
                            .to_string(),
                    );
                }
                source_x = destination.x;
                source_y = destination.y;
            }
            if *projected_hp_after_rest < selected.outcome.final_hp
                || *projected_hp_after_rest > selected.outcome.persistent.max_hp
            {
                return Err("combat recovery corridor projected HP is out of range".to_string());
            }
            if required_pending_rewards.is_empty() {
                return Err(
                    "combat recovery corridor requires explicit recovery reward evidence"
                        .to_string(),
                );
            }
            let mut pending = selected.outcome.pending_rewards.clone();
            for required in required_pending_rewards {
                let Some(position) = pending.iter().position(|reward| reward == required) else {
                    return Err(
                        "combat recovery corridor requires a reward absent from the outcome"
                            .to_string(),
                    );
                };
                pending.remove(position);
            }
            validate_committed_entry_potions_v1(
                observation,
                attempt,
                frontier,
                selected_candidate_ordinal,
                retained_entry_potions,
                true,
            )?;
            validate_committed_entry_potions_v1(
                observation,
                attempt,
                frontier,
                selected_candidate_ordinal,
                recoverable_entry_potions,
                false,
            )?;
            if recoverable_entry_potions.is_empty()
                || recoverable_entry_potions
                    .iter()
                    .any(|potion| potion.potion != PotionId::BloodPotion)
                || retained_entry_potions.iter().any(|retained| {
                    recoverable_entry_potions
                        .iter()
                        .any(|recoverable| retained.slot == recoverable.slot)
                })
            {
                return Err(
                    "combat recovery corridor requires disjoint Blood Potion recovery capacity"
                        .to_string(),
                );
            }
        }
    }
    Ok(())
}

fn validate_committed_entry_potions_v1(
    observation: &crate::agent::information::state::PublicCombatStateV1,
    attempt: &TaskResearchPlanAttemptV1,
    frontier: &[usize],
    selected_candidate_ordinal: usize,
    potions: &[TaskRetainedEntryPotionV1],
    require_frontier_advantage: bool,
) -> Result<(), String> {
    if potions.is_empty()
        || potions
            .windows(2)
            .any(|potions| potions[0].slot >= potions[1].slot)
    {
        return Err("committed potion list must be sorted and non-empty".to_string());
    }
    let selected = &attempt.candidates[selected_candidate_ordinal];
    for retained in potions {
        let observed = observation
            .potions
            .get(retained.slot)
            .and_then(|potion| potion.as_ref())
            .map(|potion| potion.potion_id);
        if observed != Some(retained.potion)
            || selected
                .outcome
                .consumed_entry_potion_slots
                .contains(&retained.slot)
            || (require_frontier_advantage
                && !frontier.iter().any(|candidate_ordinal| {
                    attempt.candidates[*candidate_ordinal]
                        .outcome
                        .consumed_entry_potion_slots
                        .contains(&retained.slot)
                }))
        {
            return Err(
                "combat run commitment potion is not supported by the frontier".to_string(),
            );
        }
    }
    Ok(())
}

impl TaskPrivilegedStrategicSupportV1 {
    pub fn validate(&self, view: &TaskActorViewV1) -> Result<(), String> {
        self.provenance.validate()?;
        let super::TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return Err("strategic support requires a public decision".to_string());
        };
        if decision.site == super::TaskDecisionSiteV1::Combat {
            return Err("strategic support was delivered at a combat decision".to_string());
        }
        if self.root_public_snapshot_id != decision.information.snapshot_id {
            return Err("strategic support belongs to another public root".to_string());
        }
        if let TaskPrivilegedStrategicSupportOutcomeV1::Choice { choice } = &self.outcome {
            decision.validate_choice(choice)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum TaskPrivilegedSupportV1 {
    CombatResearchPlan {
        support: TaskPrivilegedCombatResearchSupportV1,
    },
    StrategicChoice {
        support: TaskPrivilegedStrategicSupportV1,
    },
}

/// Private execution-side capability. Possessing an exact world does not make
/// this method available to an ordinary public actor or runner.
pub trait TaskPrivilegedSupportWorldV1: TaskWorldV1 {
    fn privileged_support(&mut self) -> Result<TaskPrivilegedSupportV1, Self::Error>;
}

/// Actor opt-in for separately delivered, provenance-carrying privileged
/// support. The ordinary actor view remains public-only.
pub trait TaskPrivilegedSupportActorV1: TaskActorV1 {
    fn needs_privileged_support(&mut self, view: &TaskActorViewV1) -> bool;

    fn receive_privileged_support(
        &mut self,
        view: &TaskActorViewV1,
        support: TaskPrivilegedSupportV1,
    ) -> Result<(), String>;
}
