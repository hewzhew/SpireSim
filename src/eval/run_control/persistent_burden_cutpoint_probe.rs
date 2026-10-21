use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::{CombatSearchV2Config, CombatSearchV2Report};
use crate::content::cards::CardId;
use crate::eval::combat_case::CombatCase;
use crate::sim::combat::CombatTerminal;
use crate::sim::combat_action_surface::CombatSelectionInputEncodingV2;
use crate::state::core::ClientInput;

use super::combat_case_adjudication::{
    project_combat_case_session, COMBAT_CASE_PROJECTION_TRUST_V1,
};
use super::combat_case_candidate_census::CombatCaseCandidateReplayFailureV1;
use cutpoint::locate_and_group_cutpoints;
use outcomes::probe_grouped_cutpoint;

mod burden;
mod cutpoint;
mod outcomes;

#[cfg(test)]
mod tests;

pub const PERSISTENT_BURDEN_CUTPOINT_LIMIT_V1: usize = 16;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistentBurdenGainedCurseCountV1 {
    pub card: CardId,
    pub count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistentBurdenCutpointConclusionV1 {
    CleanTerminalWinAvailable,
    BurdenTriggerPlanChangeAvailable,
    NoOneActionEscapeObserved,
    NoDirtyCandidateCutpoint,
    IncompleteDueToProbeFailures,
    IncompleteDueToUnsupportedStructuredActionDomain,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PersistentBurdenCutpointActionDomainV1 {
    AtomicActionsComplete,
    UnsupportedStructuredActionDomain {
        selection_input_encodings: Vec<CombatSelectionInputEncodingV2>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistentBurdenCutpointInputOutcomeKindV1 {
    CleanCombatVictory,
    NewCurse,
    LivingEnemyPlanChanged,
    Neutral,
    ApplyFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistentBurdenEnemyPlanChangeV1 {
    pub entity_id: usize,
    pub enemy: String,
    pub before_plan_id: u8,
    pub after_plan_id: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PersistentBurdenCutpointInputOutcomeV1 {
    pub action_key: String,
    pub input: ClientInput,
    pub kind: PersistentBurdenCutpointInputOutcomeKindV1,
    pub terminal: CombatTerminal,
    pub gained_curse_counts: Vec<PersistentBurdenGainedCurseCountV1>,
    pub living_enemy_plan_changes: Vec<PersistentBurdenEnemyPlanChangeV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistentBurdenCutpointAggregateV1 {
    pub clean_terminal_win_count: usize,
    pub burden_trigger_count: usize,
    pub living_enemy_plan_change_count: usize,
    pub neutral_count: usize,
    pub input_failure_count: usize,
    pub unsupported_structured_action_domain_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PersistentBurdenCutpointSummaryV1 {
    pub cutpoint_state_hash: String,
    pub candidate_frequency: usize,
    pub retained_indices: Vec<usize>,
    pub trigger_step_index: usize,
    pub trigger_action_key: String,
    pub trigger_input: ClientInput,
    pub trigger_gained_curse_counts: Vec<PersistentBurdenGainedCurseCountV1>,
    pub player_hp: i32,
    pub player_block: i32,
    pub enemy_hp: Vec<i32>,
    pub action_domain: PersistentBurdenCutpointActionDomainV1,
    pub outcomes: Vec<PersistentBurdenCutpointInputOutcomeV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CombatCasePersistentBurdenCutpointProbeV1 {
    NoDirtyCandidateCutpoint {
        source_review: String,
        retained_candidate_count: usize,
        unique_candidate_count: usize,
        replay_failures: Vec<CombatCaseCandidateReplayFailureV1>,
        conclusion: PersistentBurdenCutpointConclusionV1,
    },
    ProjectionFailed {
        source_review: String,
        error: String,
    },
    Probed {
        source_review: String,
        projection_trust: String,
        retained_candidate_count: usize,
        unique_candidate_count: usize,
        dirty_candidate_count: usize,
        candidates_with_cutpoint: usize,
        unique_cutpoint_count: usize,
        examined_cutpoint_count: usize,
        cutpoint_limit: usize,
        cutpoint_limit_hit: bool,
        omitted_cutpoint_count: usize,
        replay_failures: Vec<CombatCaseCandidateReplayFailureV1>,
        aggregate: PersistentBurdenCutpointAggregateV1,
        cutpoints: Vec<PersistentBurdenCutpointSummaryV1>,
        conclusion: PersistentBurdenCutpointConclusionV1,
    },
}

impl CombatCasePersistentBurdenCutpointProbeV1 {
    pub fn source_review(&self) -> &str {
        match self {
            Self::NoDirtyCandidateCutpoint { source_review, .. }
            | Self::ProjectionFailed { source_review, .. }
            | Self::Probed { source_review, .. } => source_review,
        }
    }
}

fn conclusion_from_aggregate(
    aggregate: &PersistentBurdenCutpointAggregateV1,
    replay_failure_count: usize,
) -> PersistentBurdenCutpointConclusionV1 {
    if aggregate.clean_terminal_win_count > 0 {
        PersistentBurdenCutpointConclusionV1::CleanTerminalWinAvailable
    } else if aggregate.living_enemy_plan_change_count > 0 {
        PersistentBurdenCutpointConclusionV1::BurdenTriggerPlanChangeAvailable
    } else if aggregate.unsupported_structured_action_domain_count > 0 {
        PersistentBurdenCutpointConclusionV1::IncompleteDueToUnsupportedStructuredActionDomain
    } else if replay_failure_count > 0 || aggregate.input_failure_count > 0 {
        PersistentBurdenCutpointConclusionV1::IncompleteDueToProbeFailures
    } else {
        PersistentBurdenCutpointConclusionV1::NoOneActionEscapeObserved
    }
}

fn aggregate_cutpoints(
    cutpoints: &[PersistentBurdenCutpointSummaryV1],
) -> PersistentBurdenCutpointAggregateV1 {
    let mut aggregate = PersistentBurdenCutpointAggregateV1::default();
    for cutpoint in cutpoints {
        if matches!(
            cutpoint.action_domain,
            PersistentBurdenCutpointActionDomainV1::UnsupportedStructuredActionDomain { .. }
        ) {
            aggregate.unsupported_structured_action_domain_count += 1;
        }
        for outcome in &cutpoint.outcomes {
            match outcome.kind {
                PersistentBurdenCutpointInputOutcomeKindV1::CleanCombatVictory => {
                    aggregate.clean_terminal_win_count += 1;
                }
                PersistentBurdenCutpointInputOutcomeKindV1::NewCurse => {
                    aggregate.burden_trigger_count += 1;
                }
                PersistentBurdenCutpointInputOutcomeKindV1::LivingEnemyPlanChanged => {
                    aggregate.living_enemy_plan_change_count += 1;
                }
                PersistentBurdenCutpointInputOutcomeKindV1::Neutral => {
                    aggregate.neutral_count += 1;
                }
                PersistentBurdenCutpointInputOutcomeKindV1::ApplyFailed => {
                    aggregate.input_failure_count += 1;
                }
            }
        }
    }
    aggregate
}

pub fn probe_combat_case_persistent_burden_cutpoints_v1(
    source_review: impl Into<String>,
    case: &CombatCase,
    config: &CombatSearchV2Config,
    report: &CombatSearchV2Report,
) -> CombatCasePersistentBurdenCutpointProbeV1 {
    let source_review = source_review.into();
    let base_session = match project_combat_case_session(case) {
        Ok(session) => session,
        Err(error) => {
            return CombatCasePersistentBurdenCutpointProbeV1::ProjectionFailed {
                source_review,
                error,
            };
        }
    };
    let located = locate_and_group_cutpoints(&base_session, config, report);
    if located.grouped.is_empty() {
        let conclusion = if located.replay_failures.is_empty() {
            PersistentBurdenCutpointConclusionV1::NoDirtyCandidateCutpoint
        } else {
            PersistentBurdenCutpointConclusionV1::IncompleteDueToProbeFailures
        };
        return CombatCasePersistentBurdenCutpointProbeV1::NoDirtyCandidateCutpoint {
            source_review,
            retained_candidate_count: located.retained_candidate_count,
            unique_candidate_count: located.unique_candidate_count,
            replay_failures: located.replay_failures,
            conclusion,
        };
    }

    let unique_cutpoint_count = located.grouped.len();
    let retained_candidate_count = located.retained_candidate_count;
    let unique_candidate_count = located.unique_candidate_count;
    let dirty_candidate_count = located.dirty_candidate_count;
    let replay_failures = located.replay_failures;
    let cutpoints = located
        .grouped
        .into_iter()
        .take(PERSISTENT_BURDEN_CUTPOINT_LIMIT_V1)
        .map(|cutpoint| probe_grouped_cutpoint(cutpoint, config))
        .collect::<Vec<_>>();
    let aggregate = aggregate_cutpoints(&cutpoints);
    let conclusion = conclusion_from_aggregate(&aggregate, replay_failures.len());
    let examined_cutpoint_count = cutpoints.len();
    let cutpoint_limit_hit = unique_cutpoint_count > PERSISTENT_BURDEN_CUTPOINT_LIMIT_V1;
    let omitted_cutpoint_count =
        unique_cutpoint_count.saturating_sub(PERSISTENT_BURDEN_CUTPOINT_LIMIT_V1);

    CombatCasePersistentBurdenCutpointProbeV1::Probed {
        source_review,
        projection_trust: COMBAT_CASE_PROJECTION_TRUST_V1.to_string(),
        retained_candidate_count,
        unique_candidate_count,
        dirty_candidate_count,
        candidates_with_cutpoint: dirty_candidate_count,
        unique_cutpoint_count,
        examined_cutpoint_count,
        cutpoint_limit: PERSISTENT_BURDEN_CUTPOINT_LIMIT_V1,
        cutpoint_limit_hit,
        omitted_cutpoint_count,
        replay_failures,
        aggregate,
        cutpoints,
        conclusion,
    }
}
