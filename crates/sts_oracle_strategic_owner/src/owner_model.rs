//! Shared strategic-owner decision model.

use sts_simulator::ai::strategy::decision_pipeline::{CandidateEvaluation, CleanupTarget};
use sts_simulator::ai::strategy::reward_admission::RewardAdmission;
use sts_simulator::eval::run_control::{
    DecisionCandidateKey, RunDecisionAction, RunForcedTransitionKindV1,
};

pub type DecisionKey = DecisionCandidateKey;

pub enum OwnerDecision {
    Candidates(Vec<OwnerChoice>),
    Routine(OwnerRoutine),
    Gap(String),
}

pub enum OwnerRoutine {
    Candidate {
        candidate_id: String,
        action: RunDecisionAction,
    },
    RewardPolicyStep,
    ForcedTransition(RunForcedTransitionKindV1),
}

#[derive(Clone)]
pub struct OwnerChoice {
    pub candidate_id: String,
    pub key: Option<DecisionKey>,
    pub action: RunDecisionAction,
    pub label: String,
    pub annotation: ChoiceAnnotation,
    pub expansion: OwnerChoiceExpansion,
}

#[derive(Clone)]
pub enum ChoiceAnnotation {
    None,
    Candidate(OwnerCandidateDecision),
}

#[derive(Clone)]
pub struct OwnerCandidateDecision {
    pub evaluation: CandidateEvaluation,
    pub admission: Option<RewardAdmission>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerChoiceExpansion {
    AutoAllowed,
    InspectOnly(&'static str),
}

impl OwnerChoice {
    pub fn auto_expand_allowed(&self) -> bool {
        matches!(self.expansion, OwnerChoiceExpansion::AutoAllowed)
    }

    pub fn inspect_only_reason(&self) -> Option<&'static str> {
        match self.expansion {
            OwnerChoiceExpansion::InspectOnly(reason) => Some(reason),
            OwnerChoiceExpansion::AutoAllowed => None,
        }
    }
}

impl ChoiceAnnotation {
    pub fn admission(&self) -> Option<&RewardAdmission> {
        match self {
            ChoiceAnnotation::Candidate(decision) => decision.admission.as_ref(),
            _ => None,
        }
    }

    pub fn evaluation(&self) -> Option<&CandidateEvaluation> {
        match self {
            ChoiceAnnotation::Candidate(decision) => Some(&decision.evaluation),
            _ => None,
        }
    }

    pub fn candidate(&self) -> Option<&OwnerCandidateDecision> {
        match self {
            ChoiceAnnotation::Candidate(decision) => Some(decision),
            _ => None,
        }
    }
}

pub fn cleanup_target_label(target: CleanupTarget) -> &'static str {
    match target {
        CleanupTarget::Curse => "curse",
        CleanupTarget::Status => "status",
        CleanupTarget::StarterStrike => "starter-attack",
        CleanupTarget::UpgradedStarterStrike => "upgraded-starter-attack",
        CleanupTarget::StarterDefend => "starter-skill",
        CleanupTarget::UpgradedStarterDefend => "upgraded-starter-skill",
        CleanupTarget::OtherStarter => "starter",
        CleanupTarget::Other => "other",
    }
}
