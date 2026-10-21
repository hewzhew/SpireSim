mod audit;
mod candidate;
mod compiler;
mod delta;
mod ledger;
mod retention;
mod run_debt;
mod snapshot;

pub use audit::{audit_delta_coverage, StrategicAuditReport};
pub use candidate::{CandidateAction, StrategicDecisionSite};
pub use compiler::{
    compile_decision, AcquisitionVerdict, CompiledDecision, StrategicDecisionTrace,
};
pub use delta::{
    add_snecko_cost_conversion_delta_v1, CandidateDelta, CandidateRole, LedgerDelta,
    OpportunityCost, StrategicContraindication, VerdictHint,
};
pub use ledger::{
    add_boss_matchup_shadow_pressure_to_ledger, add_run_debt_candidate_deltas_v1,
    add_run_debt_pressure_to_ledger, add_startup_profile_pressure_to_ledger, ledger_from_snapshot,
    PressureHorizon, PressureItem, PressureKind, PressureLedger, RunDebtCandidateSignalsV1,
    StrategicBossTax, StrategicDebt, StrategicJob,
};
pub use retention::{
    compact_branch_signature, compact_branch_signature_data, format_compact_branch_signature,
    BranchSignature, BranchSignatureCompact, RetentionBucket,
};
pub use run_debt::{
    has_curse_mitigation_or_synergy_v1, run_debt_ledger_for_relics_v1, run_debt_ledger_v1,
    run_debt_projection_for_relic_v1, run_debt_tag_rank_adjustment_v1, RunDebtContractKindV1,
    RunDebtContractV1, RunDebtLedgerV1, RunDebtProjectionV1,
};
pub use snapshot::{StrategicDeckFacts, StrategicRouteFacts, StrategicSnapshot};
