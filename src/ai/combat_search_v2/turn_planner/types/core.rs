use std::collections::BTreeMap;

use crate::ai::combat_search_v2::frontier::SearchNode;
use crate::ai::combat_search_v2::turn_planner::types::selection::TurnPlanSelectionAuditV1;
use crate::ai::combat_search_v2::value::{
    CombatEvalOutcomeClass, CombatEvalProgressBucket, CombatEvalSurvivalBucket, CombatEvalV2,
};
use crate::ai::combat_search_v2::*;

pub(in crate::ai::combat_search_v2) const DEFAULT_TURN_PLAN_MAX_INNER_NODES: usize = 128;
pub(in crate::ai::combat_search_v2) const DEFAULT_TURN_PLAN_MAX_END_STATES: usize = 16;
pub(in crate::ai::combat_search_v2) const DEFAULT_TURN_PLAN_PER_BUCKET_LIMIT: usize = 3;

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct TurnPlannerConfigV1 {
    pub(in crate::ai::combat_search_v2) max_inner_nodes: usize,
    pub(in crate::ai::combat_search_v2) max_partial_frontier_states: Option<usize>,
    pub(in crate::ai::combat_search_v2) max_end_states: usize,
    pub(in crate::ai::combat_search_v2) per_bucket_limit: usize,
    pub(in crate::ai::combat_search_v2) potion_policy: CombatSearchV2PotionPolicy,
    /// Whole-line potion-use allowance inherited from the owning search.
    /// The root node may already have spent part of it before this turn.
    pub(in crate::ai::combat_search_v2) max_potions_used: Option<u32>,
    pub(in crate::ai::combat_search_v2) allowed_potion_slots: Option<u64>,
    pub(in crate::ai::combat_search_v2) allow_potion_discard: Option<bool>,
    pub(in crate::ai::combat_search_v2) max_engine_steps_per_action: usize,
    pub(in crate::ai::combat_search_v2) turn_plan_prior: Option<CombatSearchV2TurnPlanPrior>,
    pub(in crate::ai::combat_search_v2) capture_step_trace: bool,
}

impl Default for TurnPlannerConfigV1 {
    fn default() -> Self {
        Self {
            max_inner_nodes: DEFAULT_TURN_PLAN_MAX_INNER_NODES,
            max_partial_frontier_states: None,
            max_end_states: DEFAULT_TURN_PLAN_MAX_END_STATES,
            per_bucket_limit: DEFAULT_TURN_PLAN_PER_BUCKET_LIMIT,
            potion_policy: CombatSearchV2PotionPolicy::Never,
            max_potions_used: None,
            allowed_potion_slots: None,
            allow_potion_discard: None,
            max_engine_steps_per_action: CombatSearchV2Config::default()
                .max_engine_steps_per_action,
            turn_plan_prior: None,
            capture_step_trace: true,
        }
    }
}

#[derive(Clone)]
pub(in crate::ai::combat_search_v2) struct TurnPlanV1 {
    pub(in crate::ai::combat_search_v2) actions: Vec<CombatSearchV2ActionTrace>,
    pub(in crate::ai::combat_search_v2) action_facts: Vec<CombatSearchV2ActionFacts>,
    pub(in crate::ai::combat_search_v2) step_states: Vec<TurnPlanStepStateV1>,
    pub(in crate::ai::combat_search_v2) end_node: SearchNode,
    pub(in crate::ai::combat_search_v2) stop_reason: TurnPlanStopReason,
    pub(in crate::ai::combat_search_v2) bucket: TurnPlanBucket,
    pub(in crate::ai::combat_search_v2) eval: CombatEvalV2,
}

#[derive(Clone)]
pub(in crate::ai::combat_search_v2) struct TurnPlanStepStateV1 {
    pub(in crate::ai::combat_search_v2) before_exact_state_hash: String,
    pub(in crate::ai::combat_search_v2) before: CombatSearchV2StateSummary,
    pub(in crate::ai::combat_search_v2) after_exact_state_hash: String,
    pub(in crate::ai::combat_search_v2) after: CombatSearchV2StateSummary,
    pub(in crate::ai::combat_search_v2) engine_steps: usize,
}

#[derive(Clone, Default)]
pub(in crate::ai::combat_search_v2) struct TurnPlanEnumeration {
    pub(in crate::ai::combat_search_v2) plans: Vec<TurnPlanV1>,
    pub(in crate::ai::combat_search_v2) selection_audit: TurnPlanSelectionAuditV1,
    pub(in crate::ai::combat_search_v2) preselection_plan_count: usize,
    pub(in crate::ai::combat_search_v2) preselection_first_actions: Vec<CombatSearchV2ActionTrace>,
    pub(in crate::ai::combat_search_v2) preselection_first_action_summaries:
        Vec<TurnPlanFirstActionSummaryV1>,
    pub(in crate::ai::combat_search_v2) preselection_bucket_counts: BTreeMap<TurnPlanBucket, usize>,
    pub(in crate::ai::combat_search_v2) nodes_expanded: usize,
    pub(in crate::ai::combat_search_v2) nodes_generated: usize,
    pub(in crate::ai::combat_search_v2) engine_steps: usize,
    pub(in crate::ai::combat_search_v2) exact_state_skips: usize,
    pub(in crate::ai::combat_search_v2) truncated_children: usize,
    pub(in crate::ai::combat_search_v2) turn_plan_prior_scored_plans: usize,
}

#[derive(Clone)]
pub(in crate::ai::combat_search_v2) struct TurnPlanFirstActionSummaryV1 {
    pub(in crate::ai::combat_search_v2) action: CombatSearchV2ActionTrace,
    pub(in crate::ai::combat_search_v2) plan_count: usize,
    pub(in crate::ai::combat_search_v2) bucket_counts: BTreeMap<TurnPlanBucket, usize>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPlanStopReason {
    Terminal,
    NextTurn,
    PendingChoice,
    OtherBoundary,
    NoLegalActions,
    EngineStepLimit,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPlanBucket {
    TerminalWin,
    TerminalLoss,
    Survival,
    Progress,
    Setup,
    Balanced,
    Boundary,
}

impl TurnPlanBucket {
    pub(in crate::ai::combat_search_v2) fn from_root_and_eval(
        root_eval: CombatEvalV2,
        eval: CombatEvalV2,
        stop_reason: TurnPlanStopReason,
    ) -> Self {
        let bucket = Self::from_eval_and_stop(eval, stop_reason);
        if matches!(
            bucket,
            Self::TerminalWin | Self::TerminalLoss | Self::Boundary
        ) {
            return bucket;
        }
        if survival_bucket_is_danger(root_eval.survival_bucket())
            && !survival_bucket_is_danger(eval.survival_bucket())
        {
            return Self::Survival;
        }
        bucket
    }

    pub(in crate::ai::combat_search_v2) fn from_eval_and_stop(
        eval: CombatEvalV2,
        stop_reason: TurnPlanStopReason,
    ) -> Self {
        match eval.outcome_class() {
            CombatEvalOutcomeClass::Win => Self::TerminalWin,
            CombatEvalOutcomeClass::Loss => Self::TerminalLoss,
            CombatEvalOutcomeClass::Unresolved => match stop_reason {
                TurnPlanStopReason::PendingChoice | TurnPlanStopReason::OtherBoundary => {
                    Self::Boundary
                }
                _ if matches!(
                    eval.survival_bucket(),
                    CombatEvalSurvivalBucket::DeadOrForcedLoss
                        | CombatEvalSurvivalBucket::LethalVisible
                        | CombatEvalSurvivalBucket::Critical
                ) =>
                {
                    Self::Survival
                }
                _ if matches!(
                    eval.progress_bucket(),
                    CombatEvalProgressBucket::LethalNow
                        | CombatEvalProgressBucket::LethalNextTurnLikely
                        | CombatEvalProgressBucket::RaceFavored
                ) =>
                {
                    Self::Progress
                }
                _ if matches!(
                    eval.progress_bucket(),
                    CombatEvalProgressBucket::AttritionFavored
                ) =>
                {
                    Self::Setup
                }
                _ => Self::Balanced,
            },
        }
    }

    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::TerminalWin => "terminal_win",
            Self::TerminalLoss => "terminal_loss",
            Self::Survival => "survival",
            Self::Progress => "progress",
            Self::Setup => "setup",
            Self::Balanced => "balanced",
            Self::Boundary => "boundary",
        }
    }
}

fn survival_bucket_is_danger(bucket: CombatEvalSurvivalBucket) -> bool {
    matches!(
        bucket,
        CombatEvalSurvivalBucket::DeadOrForcedLoss
            | CombatEvalSurvivalBucket::LethalVisible
            | CombatEvalSurvivalBucket::Critical
    )
}

impl TurnPlanStopReason {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::NextTurn => "next_turn",
            Self::PendingChoice => "pending_choice",
            Self::OtherBoundary => "other_boundary",
            Self::NoLegalActions => "no_legal_actions",
            Self::EngineStepLimit => "engine_step_limit",
        }
    }
}
