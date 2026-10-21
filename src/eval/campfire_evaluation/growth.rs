use std::collections::HashMap;

use crate::ai::deck_mutation_compiler_v1::{
    deck_removal_target_snapshots_v1, DeckMutationCardSnapshotV1, DeckMutationTargetClassV1,
    DeckMutationTargetLossV1,
};
use crate::ai::upgrade_planner_v1::{
    plan_upgrades_v1, UpgradeCandidateV1, UpgradeDebtKindV1, UpgradeDebtSeverityV1,
    UpgradeMechanicalDeltaV1, UpgradeRoleV1, UpgradeVerdictV1,
};
use crate::content::cards::CardId;
use crate::content::relics::{girya::Girya, RelicId};
use crate::engine::campfire_candidates::CampfireCandidate;
use crate::eval::campfire_projection::CampfireProjection;
use crate::runtime::combat::CardLineageId;
use crate::state::run::RunState;

use super::{
    unsupported_evidence, with_future_limitations, CampfireEvidenceLimitation,
    CampfireEvidenceProvenance, CampfireEvidenceStatus, CampfireFieldEvidence,
    CampfireProspectField,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CampfireGrowth {
    pub smith: Option<CampfireSmithGrowth>,
    pub toke: Option<CampfireTokeGrowth>,
    pub lift: Option<CampfireLiftGrowth>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfireSmithGrowth {
    pub card_lineage: CardLineageId,
    pub card: CardId,
    pub upgrades_before: u8,
    pub upgrades_after: u8,
    pub mechanical_delta: UpgradeMechanicalDeltaV1,
    pub roles: Vec<UpgradeRoleV1>,
    pub pays_debts: Vec<UpgradeDebtKindV1>,
    pub opportunity_costs: Vec<String>,
    pub urgency: UpgradeDebtSeverityV1,
    pub verdict: UpgradeVerdictV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfireTokeGrowth {
    pub card_lineage: CardLineageId,
    pub card: CardId,
    pub upgrades: u8,
    pub deck_size_before: usize,
    pub deck_size_after: usize,
    pub target_class: DeckMutationTargetClassV1,
    pub target_loss: DeckMutationTargetLossV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfireLiftGrowth {
    pub girya_counter_before: i32,
    pub girya_counter_after: i32,
    pub combat_start_strength_before: i32,
    pub combat_start_strength_after: i32,
    pub combat_start_strength_delta: i32,
}

pub(super) struct CampfireGrowthFacts {
    smith_by_lineage: HashMap<CardLineageId, UpgradeCandidateV1>,
    toke_by_lineage: HashMap<CardLineageId, DeckMutationCardSnapshotV1>,
}

pub(super) fn build_growth_facts(root: &RunState) -> CampfireGrowthFacts {
    let smith_by_lineage = plan_upgrades_v1(root)
        .candidates
        .into_iter()
        .map(|candidate| (candidate.card_lineage, candidate))
        .collect();
    let toke_by_lineage = deck_removal_target_snapshots_v1(root)
        .into_iter()
        .map(|candidate| (candidate.lineage, candidate))
        .collect();
    CampfireGrowthFacts {
        smith_by_lineage,
        toke_by_lineage,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampfireGrowthError {
    MissingUpgradeCandidate {
        card_lineage: CardLineageId,
    },
    ExpectedExactProjection {
        candidate: CampfireCandidate,
    },
    MissingProjectedCard {
        card_lineage: CardLineageId,
    },
    AmbiguousProjectedCard {
        card_lineage: CardLineageId,
    },
    UpgradeCountMismatch {
        card_lineage: CardLineageId,
        before: u8,
        after: u8,
    },
    MissingRemovalTarget {
        card_lineage: CardLineageId,
    },
    RemovalProjectionMismatch {
        card_lineage: CardLineageId,
    },
    MissingGirya,
    AmbiguousGirya,
    GiryaCounterMismatch {
        before: i32,
        after: i32,
    },
}

pub(super) struct CampfireGrowthAssessment {
    pub value: CampfireGrowth,
    pub evidence: CampfireFieldEvidence,
}

pub(super) fn assess_growth(
    root: &RunState,
    facts: &CampfireGrowthFacts,
    candidate: CampfireCandidate,
    projection: &CampfireProjection,
) -> Result<CampfireGrowthAssessment, CampfireGrowthError> {
    match candidate {
        CampfireCandidate::Smith { card_lineage } => {
            assess_smith(facts, candidate, card_lineage, projection)
        }
        CampfireCandidate::Toke { card_lineage } => {
            assess_toke(root, facts, candidate, card_lineage, projection)
        }
        CampfireCandidate::Lift => assess_lift(root, candidate, projection),
        _ => Ok(unsupported_growth(projection)),
    }
}

fn assess_smith(
    facts: &CampfireGrowthFacts,
    candidate: CampfireCandidate,
    card_lineage: CardLineageId,
    projection: &CampfireProjection,
) -> Result<CampfireGrowthAssessment, CampfireGrowthError> {
    let CampfireProjection::Exact(exact) = projection else {
        return Err(CampfireGrowthError::ExpectedExactProjection { candidate });
    };
    let planner = facts
        .smith_by_lineage
        .get(&card_lineage)
        .ok_or(CampfireGrowthError::MissingUpgradeCandidate { card_lineage })?;

    let mut projected_cards = exact
        .run_state
        .master_deck
        .iter()
        .filter(|card| card.lineage == card_lineage);
    let projected = projected_cards
        .next()
        .ok_or(CampfireGrowthError::MissingProjectedCard { card_lineage })?;
    if projected_cards.next().is_some() {
        return Err(CampfireGrowthError::AmbiguousProjectedCard { card_lineage });
    }
    let expected_after = planner.upgrades.saturating_add(1);
    if projected.upgrades != expected_after {
        return Err(CampfireGrowthError::UpgradeCountMismatch {
            card_lineage,
            before: planner.upgrades,
            after: projected.upgrades,
        });
    }

    Ok(CampfireGrowthAssessment {
        value: CampfireGrowth {
            smith: Some(CampfireSmithGrowth {
                card_lineage,
                card: planner.card,
                upgrades_before: planner.upgrades,
                upgrades_after: projected.upgrades,
                mechanical_delta: planner.mechanical_delta.clone(),
                roles: planner.roles.clone(),
                pays_debts: planner.pays_debts.clone(),
                opportunity_costs: planner.opportunity_costs.clone(),
                urgency: planner.urgency,
                verdict: planner.verdict,
            }),
            toke: None,
            lift: None,
        },
        evidence: partial_growth_evidence(
            CampfireEvidenceProvenance::EngineTransitionAndUpgradePlanner,
        ),
    })
}

fn assess_toke(
    root: &RunState,
    facts: &CampfireGrowthFacts,
    candidate: CampfireCandidate,
    card_lineage: CardLineageId,
    projection: &CampfireProjection,
) -> Result<CampfireGrowthAssessment, CampfireGrowthError> {
    let CampfireProjection::Exact(exact) = projection else {
        return Err(CampfireGrowthError::ExpectedExactProjection { candidate });
    };
    let removal = facts
        .toke_by_lineage
        .get(&card_lineage)
        .ok_or(CampfireGrowthError::MissingRemovalTarget { card_lineage })?;
    let expected_deck = root
        .master_deck
        .iter()
        .filter(|card| card.lineage != card_lineage)
        .cloned()
        .collect::<Vec<_>>();
    if exact.run_state.master_deck != expected_deck {
        return Err(CampfireGrowthError::RemovalProjectionMismatch { card_lineage });
    }

    Ok(CampfireGrowthAssessment {
        value: CampfireGrowth {
            smith: None,
            toke: Some(CampfireTokeGrowth {
                card_lineage,
                card: removal.card,
                upgrades: removal.upgrades,
                deck_size_before: root.master_deck.len(),
                deck_size_after: exact.run_state.master_deck.len(),
                target_class: removal.target_class,
                target_loss: removal.target_loss.clone(),
            }),
            lift: None,
        },
        evidence: partial_growth_evidence(
            CampfireEvidenceProvenance::EngineTransitionAndDeckMutationCompiler,
        ),
    })
}

fn assess_lift(
    root: &RunState,
    candidate: CampfireCandidate,
    projection: &CampfireProjection,
) -> Result<CampfireGrowthAssessment, CampfireGrowthError> {
    let CampfireProjection::Exact(exact) = projection else {
        return Err(CampfireGrowthError::ExpectedExactProjection { candidate });
    };
    let girya_counter_before = unique_girya_counter(root)?;
    let girya_counter_after = unique_girya_counter(&exact.run_state)?;
    if girya_counter_after != girya_counter_before + 1 {
        return Err(CampfireGrowthError::GiryaCounterMismatch {
            before: girya_counter_before,
            after: girya_counter_after,
        });
    }
    let combat_start_strength_before = Girya::battle_start_strength(girya_counter_before);
    let combat_start_strength_after = Girya::battle_start_strength(girya_counter_after);

    Ok(CampfireGrowthAssessment {
        value: CampfireGrowth {
            smith: None,
            toke: None,
            lift: Some(CampfireLiftGrowth {
                girya_counter_before,
                girya_counter_after,
                combat_start_strength_before,
                combat_start_strength_after,
                combat_start_strength_delta: combat_start_strength_after
                    - combat_start_strength_before,
            }),
        },
        evidence: partial_growth_evidence(
            CampfireEvidenceProvenance::EngineTransitionAndRelicMechanics,
        ),
    })
}

fn unique_girya_counter(run: &RunState) -> Result<i32, CampfireGrowthError> {
    let mut giryas = run.relics.iter().filter(|relic| relic.id == RelicId::Girya);
    let girya = giryas.next().ok_or(CampfireGrowthError::MissingGirya)?;
    if giryas.next().is_some() {
        return Err(CampfireGrowthError::AmbiguousGirya);
    }
    Ok(girya.counter)
}

fn partial_growth_evidence(provenance: CampfireEvidenceProvenance) -> CampfireFieldEvidence {
    CampfireFieldEvidence {
        field: CampfireProspectField::GrowthDistribution,
        status: CampfireEvidenceStatus::Partial,
        provenance,
        limitations: vec![CampfireEvidenceLimitation::DownstreamGrowthDistributionNotEvaluated],
    }
}

fn unsupported_growth(projection: &CampfireProjection) -> CampfireGrowthAssessment {
    let future_limitations = match projection {
        CampfireProjection::Exact(_) => Vec::new(),
        CampfireProjection::Chance(_) => {
            vec![CampfireEvidenceLimitation::ChanceOutcomeNotEstimated]
        }
        CampfireProjection::ChanceThenDecision(_) => vec![
            CampfireEvidenceLimitation::ChanceOutcomeNotEstimated,
            CampfireEvidenceLimitation::PostRevealRecourseNotEvaluated,
        ],
    };

    CampfireGrowthAssessment {
        value: CampfireGrowth::default(),
        evidence: unsupported_evidence(
            CampfireProspectField::GrowthDistribution,
            with_future_limitations(
                CampfireEvidenceLimitation::GrowthNotEvaluated,
                &future_limitations,
            ),
        ),
    }
}
