mod adapters;
mod hash;
mod outcome;
mod replay;
mod types;
mod validation;

pub use outcome::{
    attach_noncombat_outcome_v1, attach_noncombat_outcome_with_card_reward_observation_v1,
    noncombat_decision_record_hash_v1, CardRewardOutcomeAttachmentV1,
    CardRewardOutcomeObservationV1, NonCombatOutcomeAttachmentV1, NonCombatOutcomeMetricsV1,
    NonCombatOutcomeSnapshotV1, NonCombatOutcomeWindowV1, NonCombatRunTerminalV1,
    NONCOMBAT_OUTCOME_ATTACHMENT_SCHEMA_NAME, NONCOMBAT_OUTCOME_ATTACHMENT_SCHEMA_VERSION,
};
pub use replay::{
    compare_noncombat_decision_records_v1, NonCombatDecisionReplayReportV1,
    NonCombatReplayCandidateSetStatusV1, NonCombatReplayCandidateSetV1,
    NonCombatReplayValueDeltaV1, NONCOMBAT_DECISION_REPLAY_SCHEMA_NAME,
    NONCOMBAT_DECISION_REPLAY_SCHEMA_VERSION,
};
pub use types::{
    CandidateDescriptorV1, DataRoleV1, DecisionSiteKindV1, EvidenceBundleV1, EvidenceItemV1,
    EvidenceKindV1, InformationBoundaryV1, InformationClassV1, NonCombatDecisionRecordV1,
    PolicyProvenanceV1, PolicySelectionStatusV1, PolicySelectionV1, PublicActionPlanV1,
    ValueComponentV1, ValueEstimateV1, NONCOMBAT_DECISION_RECORD_SCHEMA_NAME,
    NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
};
pub use validation::{
    render_noncombat_decision_record_validation_errors, validate_noncombat_decision_record_v1,
    NonCombatDecisionRecordValidationErrorV1,
};
