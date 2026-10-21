use crate::ai::combat_search_v2::turn_planner::types::core::TurnPlanBucket;
use crate::ai::combat_search_v2::turn_planner::types::coverage::{
    TurnPlanCoverageKeyV1, TurnPlanCoverageSignatureV1,
};

#[derive(Clone, Debug, Default)]
pub(in crate::ai::combat_search_v2) struct TurnPlanSelectionAuditV1 {
    pub(in crate::ai::combat_search_v2) candidates: Vec<TurnPlanCandidateSelectionAuditV1>,
    pub(in crate::ai::combat_search_v2) coverage_groups: Vec<TurnPlanCoverageGroupAuditV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct TurnPlanCandidateSelectionAuditV1 {
    pub(in crate::ai::combat_search_v2) preselection_rank: usize,
    pub(in crate::ai::combat_search_v2) selected_plan_index: Option<usize>,
    pub(in crate::ai::combat_search_v2) outcome: TurnPlanCandidateSelectionOutcomeV1,
    pub(in crate::ai::combat_search_v2) drop_reason: Option<TurnPlanCandidateDropReasonV1>,
    pub(in crate::ai::combat_search_v2) bucket: TurnPlanBucket,
    pub(in crate::ai::combat_search_v2) action_keys: Vec<String>,
    pub(in crate::ai::combat_search_v2) coverage_key: TurnPlanCoverageKeyV1,
    pub(in crate::ai::combat_search_v2) coverage_signature: TurnPlanCoverageSignatureV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) enum TurnPlanCandidateSelectionOutcomeV1 {
    Selected,
    Dropped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) enum TurnPlanCandidateDropReasonV1 {
    BucketCap,
    MaxEndStates,
    SelectionDisabled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct TurnPlanCoverageGroupAuditV1 {
    pub(in crate::ai::combat_search_v2) key: TurnPlanCoverageGroupKeyV1,
    pub(in crate::ai::combat_search_v2) preselection_count: usize,
    pub(in crate::ai::combat_search_v2) selected_count: usize,
    pub(in crate::ai::combat_search_v2) bucket_cap_dropped_count: usize,
    pub(in crate::ai::combat_search_v2) max_end_states_dropped_count: usize,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) struct TurnPlanCoverageGroupKeyV1 {
    pub(in crate::ai::combat_search_v2) bucket: TurnPlanBucket,
    pub(in crate::ai::combat_search_v2) coverage: TurnPlanCoverageKeyV1,
}

impl TurnPlanCandidateSelectionOutcomeV1 {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::Dropped => "dropped",
        }
    }
}

impl TurnPlanCandidateDropReasonV1 {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::BucketCap => "bucket_cap",
            Self::MaxEndStates => "max_end_states",
            Self::SelectionDisabled => "selection_disabled",
        }
    }
}
