use serde::{Deserialize, Serialize};

use super::{
    CampfireEvaluationContext, CampfireEvidenceLimitation, CampfireEvidenceProvenance,
    CampfireEvidenceStatus, CampfireFieldEvidence, CampfireProspectField, CampfireRunGoal,
};
use crate::ai::route_window_facts::{
    RouteWindowModality, RouteWindowPredicate, RouteWindowSubject,
};
use crate::engine::campfire_candidates::CampfireCandidate;
use crate::eval::campfire_projection::CampfireProjection;
use crate::state::run::RunState;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireRubyKeyObligation {
    NotRequired,
    AlreadySatisfied,
    SatisfiedByCandidate,
    DeferrableOnEveryVisiblePath,
    DeferrableOnSomeVisiblePath,
    ViolatedAtVisibleAct3BossDeadline,
    UnresolvedBeyondVisibleWindow,
    UnresolvedByChanceOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireRunFeasibility {
    pub declared_goal: CampfireRunGoal,
    pub ruby_key: CampfireRubyKeyObligation,
    pub sapphire_key_held: bool,
    pub emerald_key_held: bool,
}

pub(super) struct CampfireRunFeasibilityAssessment {
    pub value: CampfireRunFeasibility,
    pub evidence: CampfireFieldEvidence,
}

pub(super) fn assess_run_feasibility(
    root: &RunState,
    context: &CampfireEvaluationContext,
    candidate: CampfireCandidate,
    projection: &CampfireProjection,
) -> CampfireRunFeasibilityAssessment {
    let goal = context.spec.run_goal;
    let sapphire_key_held = root.keys[1];
    let emerald_key_held = root.keys[2];
    let (ruby_key, evidence) = match goal {
        CampfireRunGoal::Act3Victory => (
            CampfireRubyKeyObligation::NotRequired,
            exact_evidence(CampfireEvidenceProvenance::PublicRootObservation),
        ),
        CampfireRunGoal::HeartEligibility | CampfireRunGoal::HeartVictory => {
            if root.keys[0] {
                (
                    CampfireRubyKeyObligation::AlreadySatisfied,
                    satisfied_heart_evidence(
                        sapphire_key_held,
                        emerald_key_held,
                        CampfireEvidenceProvenance::PublicRootObservation,
                    ),
                )
            } else if candidate == CampfireCandidate::Recall && projected_ruby_key(projection) {
                (
                    CampfireRubyKeyObligation::SatisfiedByCandidate,
                    satisfied_heart_evidence(
                        sapphire_key_held,
                        emerald_key_held,
                        CampfireEvidenceProvenance::EngineTransition,
                    ),
                )
            } else {
                classify_missing_ruby_key(
                    root,
                    context,
                    candidate,
                    sapphire_key_held,
                    emerald_key_held,
                )
            }
        }
    };

    CampfireRunFeasibilityAssessment {
        value: CampfireRunFeasibility {
            declared_goal: goal,
            ruby_key,
            sapphire_key_held,
            emerald_key_held,
        },
        evidence,
    }
}

fn classify_missing_ruby_key(
    root: &RunState,
    context: &CampfireEvaluationContext,
    candidate: CampfireCandidate,
    sapphire_key_held: bool,
    emerald_key_held: bool,
) -> (CampfireRubyKeyObligation, CampfireFieldEvidence) {
    let boss_before_campfire = route_modality(
        context,
        RouteWindowPredicate::OccursBefore {
            subject: RouteWindowSubject::Boss,
            before: RouteWindowSubject::Campfire,
        },
    );
    if root.act_num == 3 && boss_before_campfire == Some(RouteWindowModality::Must) {
        if candidate == CampfireCandidate::Dig {
            return (
                CampfireRubyKeyObligation::UnresolvedByChanceOutcome,
                partial_evidence(
                    CampfireEvidenceProvenance::PublicRootAndRouteWindowFacts,
                    heart_limitations(
                        sapphire_key_held,
                        emerald_key_held,
                        [CampfireEvidenceLimitation::ChanceOutcomeCouldChangeRouteAccess],
                    ),
                ),
            );
        }
        return (
            CampfireRubyKeyObligation::ViolatedAtVisibleAct3BossDeadline,
            exact_evidence(CampfireEvidenceProvenance::PublicRootAndRouteWindowFacts),
        );
    }

    let future_campfire = route_modality(
        context,
        RouteWindowPredicate::PresentInWindow {
            subject: RouteWindowSubject::Campfire,
        },
    );
    match future_campfire {
        Some(RouteWindowModality::Must) => (
            CampfireRubyKeyObligation::DeferrableOnEveryVisiblePath,
            partial_evidence(
                CampfireEvidenceProvenance::PublicRootAndRouteWindowFacts,
                heart_limitations(
                    sapphire_key_held,
                    emerald_key_held,
                    [CampfireEvidenceLimitation::FutureRecallDecisionNotEvaluated],
                ),
            ),
        ),
        Some(RouteWindowModality::Can) => (
            CampfireRubyKeyObligation::DeferrableOnSomeVisiblePath,
            partial_evidence(
                CampfireEvidenceProvenance::PublicRootAndRouteWindowFacts,
                heart_limitations(
                    sapphire_key_held,
                    emerald_key_held,
                    [CampfireEvidenceLimitation::FutureRecallDecisionNotEvaluated],
                ),
            ),
        ),
        Some(RouteWindowModality::Cannot) | Some(RouteWindowModality::Unknown) | None => (
            CampfireRubyKeyObligation::UnresolvedBeyondVisibleWindow,
            partial_evidence(
                CampfireEvidenceProvenance::PublicRootAndRouteWindowFacts,
                heart_limitations(
                    sapphire_key_held,
                    emerald_key_held,
                    [CampfireEvidenceLimitation::VisibleDeadlineNotProven],
                ),
            ),
        ),
    }
}

fn route_modality(
    context: &CampfireEvaluationContext,
    predicate: RouteWindowPredicate,
) -> Option<RouteWindowModality> {
    context
        .route_window_facts
        .facts_for(&predicate)
        .first()
        .map(|fact| fact.modality)
}

fn heart_limitations<const N: usize>(
    sapphire_key_held: bool,
    emerald_key_held: bool,
    primary: [CampfireEvidenceLimitation; N],
) -> Vec<CampfireEvidenceLimitation> {
    let mut limitations = primary.to_vec();
    if !sapphire_key_held || !emerald_key_held {
        limitations.push(CampfireEvidenceLimitation::OtherHeartKeysNotEvaluated);
    }
    limitations
}

fn projected_ruby_key(projection: &CampfireProjection) -> bool {
    matches!(
        projection,
        CampfireProjection::Exact(exact) if exact.run_state.keys[0]
    )
}

fn satisfied_heart_evidence(
    sapphire_key_held: bool,
    emerald_key_held: bool,
    provenance: CampfireEvidenceProvenance,
) -> CampfireFieldEvidence {
    if sapphire_key_held && emerald_key_held {
        exact_evidence(provenance)
    } else {
        partial_evidence(
            provenance,
            vec![CampfireEvidenceLimitation::OtherHeartKeysNotEvaluated],
        )
    }
}

fn exact_evidence(provenance: CampfireEvidenceProvenance) -> CampfireFieldEvidence {
    CampfireFieldEvidence {
        field: CampfireProspectField::RunFeasibility,
        status: CampfireEvidenceStatus::Exact,
        provenance,
        limitations: Vec::new(),
    }
}

fn partial_evidence(
    provenance: CampfireEvidenceProvenance,
    limitations: Vec<CampfireEvidenceLimitation>,
) -> CampfireFieldEvidence {
    CampfireFieldEvidence {
        field: CampfireProspectField::RunFeasibility,
        status: CampfireEvidenceStatus::Partial,
        provenance,
        limitations,
    }
}
