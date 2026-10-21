mod candidate;
mod coverage;
mod formation;
mod pressure;
mod route_package;
mod run_snapshot;
mod snapshot;
mod snapshot_v2;
mod threat;
mod types;

pub use candidate::candidate_plan_delta_v2;
pub use coverage::{
    threat_coverage_after_card_v1, threat_coverage_from_deck_facts_v1,
    threat_coverage_from_run_state_v1, threat_relevant_capability_improvements_v1,
};
pub use run_snapshot::{
    build_run_strategy_snapshot_from_run_state_v2,
    build_run_strategy_snapshot_from_run_state_with_route_v2,
};
pub use snapshot_v2::build_run_strategy_snapshot_v2;
pub use types::{
    RunStrategySnapshotV2, StrategyCandidateFactsV1, StrategyCandidatePlanDeltaV1,
    StrategyCapabilityCoverageV1, StrategyCapabilityEvidenceV1, StrategyCapabilityInputKindV1,
    StrategyCapabilityInputV1, StrategyCapabilityKindV1, StrategyDeckFactsV1,
    StrategyDeckFormationNeedV1, StrategyDeckFormationStageV1, StrategyFormationSummaryV2,
    StrategyPackageDomainV2, StrategyPackageGapV2, StrategyPackageIdV2, StrategyPackageV2,
    StrategyPlanEffectV1, StrategyPlanSupportV1, StrategyResourceFactsV2, StrategyRouteFutureV1,
    StrategyThreatCoverageGapV1, StrategyThreatCoverageLedgerV1, StrategyThreatProfileV1,
    StrategyThreatSourceRecordV1, StrategyThreatSourceV1, StrategyThreatTagV1,
};

#[cfg(test)]
mod tests;
