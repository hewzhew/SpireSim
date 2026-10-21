use crate::sim::combat::CombatPosition;

use super::super::turn_plan_probe_report::{
    CombatSearchV2TurnPlanProbeCandidateReport, CombatSearchV2TurnPlanProbeRootReport,
};

/// One caller-owned, lexicographically ordered view over an exact partial-turn
/// state. The legacy enumerator treats the lane and components as opaque and
/// may only use them to retain a bounded representative.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatSearchV2TurnPlanProbeGuideRankV1 {
    pub lane: u32,
    pub components: Vec<i32>,
}

/// The maintained partial-frontier view which first retained one candidate.
/// Guide lanes carry their caller-owned opaque lane identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatSearchV2TurnPlanPartialRetentionLaneV1 {
    Untruncated,
    FirstActionCoverage,
    Survival,
    Progress,
    Setup,
    PowerInstallation,
    ResourceTransformation,
    PendingChoiceContinuation,
    PendingChoiceSelection,
    Balanced,
    Guide(u32),
}

/// Selector-native audit for one candidate before the partial frontier is
/// truncated. Ranks are zero-based over the complete pending layer; the
/// first-action rank is zero-based only among candidates sharing that first
/// exact action key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatSearchV2TurnPlanPartialCandidateAuditV1 {
    pub selected_by: Option<CombatSearchV2TurnPlanPartialRetentionLaneV1>,
    pub survival_rank: usize,
    pub progress_rank: usize,
    pub setup_rank: usize,
    pub power_installation_rank: Option<usize>,
    pub resource_transformation_rank: Option<usize>,
    pub pending_choice_continuation_rank: Option<usize>,
    pub pending_choice_selection_rank: Option<usize>,
    pub balanced_rank: usize,
    pub first_action_balanced_rank: usize,
    pub guide_conditioned_ranks: Vec<CombatSearchV2TurnPlanPartialGuideConditionedAuditV1>,
}

/// Ranks within candidates that expose the same opaque guide lane and exactly
/// equal guide components. This distinguishes a missing conditioned frontier
/// from a value function that rejects the target even among semantic peers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatSearchV2TurnPlanPartialGuideConditionedAuditV1 {
    pub lane: u32,
    pub peer_count: usize,
    pub survival_rank: usize,
    pub progress_rank: usize,
    pub setup_rank: usize,
    pub balanced_rank: usize,
}

/// Diagnostic-only record returned by the exact selector while it performs
/// the same mutation production enumeration would have performed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatSearchV2TurnPlanPartialRetentionAuditV1 {
    pub max_retained_states: usize,
    pub candidates: Vec<CombatSearchV2TurnPlanPartialCandidateAuditV1>,
}

#[derive(Clone)]
pub struct CombatSearchV2TurnPlanProbeCandidate {
    pub report: CombatSearchV2TurnPlanProbeCandidateReport,
    pub position: CombatPosition,
}

#[derive(Clone)]
pub struct CombatSearchV2TurnPlanProbeEnumeration {
    pub report: CombatSearchV2TurnPlanProbeRootReport,
    pub candidates: Vec<CombatSearchV2TurnPlanProbeCandidate>,
}
