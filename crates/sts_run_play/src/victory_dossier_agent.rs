use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::storage::RunPlayWorkspaceV1;

const AGENT_TRANSCRIPT_EVENT_SCHEMA_V1: &str = "sts-run-agent-transcript-event-v1";
const MAX_AGENT_TRANSCRIPT_BYTES_V1: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RunPlayExternalAgentEvidenceV1 {
    pub input_sources: Vec<String>,
    pub jsonl_file_count: usize,
    pub agent_transcript_count: usize,
    pub non_agent_jsonl_count: usize,
    pub other_session_transcript_count: usize,
    pub matched_model_call_count: usize,
    pub provider_receipt_count: usize,
    pub proposal_count: usize,
    pub proposal_accepted_count: usize,
    pub proposal_rejected_count: usize,
    pub proposal_unresolved_count: usize,
    pub distinct_final_active_decision_count_with_proposal: usize,
    pub distinct_final_active_decision_action_proposal_count: usize,
    pub model_action_commit_receipt_count: usize,
    pub combat_plan_commit_receipt_count: usize,
    pub combat_milestone_commit_receipt_count: usize,
    pub combat_turn_commit_receipt_count: usize,
    pub forced_action_commit_receipt_count: usize,
    pub routine_action_commit_receipt_count: usize,
    pub distinct_final_active_model_action_commit_count: usize,
    pub distinct_final_active_combat_plan_commit_count: usize,
    pub distinct_final_active_combat_milestone_commit_count: usize,
    pub distinct_final_active_combat_turn_commit_count: usize,
    pub distinct_final_active_forced_action_commit_count: usize,
    pub distinct_final_active_routine_action_commit_count: usize,
    pub token_usage_totals: RunPlayAgentTokenUsageTotalsV1,
    pub matched_transcripts: Vec<RunPlayAgentTranscriptEvidenceV1>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RunPlayAgentTokenUsageTotalsV1 {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: u64,
    pub reasoning_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayAgentTranscriptEvidenceV1 {
    pub path: String,
    pub session_id: String,
    pub initial_head_revision: u64,
    pub complete: bool,
    pub failed: bool,
    pub stop_reason: Option<String>,
    pub model_calls: Vec<RunPlayAgentModelCallV1>,
    pub proposals: Vec<RunPlayAgentProposalEvidenceV1>,
    pub commits: Vec<RunPlayAgentCommitEvidenceV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayAgentModelCallV1 {
    pub turn_id: String,
    pub decision_stage: Option<String>,
    pub response_kind: String,
    pub provider_receipt: Option<RunPlayAgentProviderReceiptV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunPlayAgentProviderReceiptV1 {
    pub transport: String,
    pub model: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<RunPlayAgentTokenUsageV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunPlayAgentTokenUsageV1 {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub cached_input_tokens: Option<u64>,
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayAgentProposalEvidenceV1 {
    pub turn_id: String,
    pub root_session_id: Option<String>,
    pub root_head_revision: Option<u64>,
    pub root_public_snapshot_id: Option<String>,
    pub action: String,
    pub summary: Option<String>,
    pub provider_receipt: Option<RunPlayAgentProviderReceiptV1>,
    pub final_active_path_disposition: RunPlayAgentProposalDispositionV1,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayAgentCommitKindV1 {
    ModelAction,
    CombatPlan,
    CombatMilestone,
    CombatTurn,
    ForcedAction,
    RoutineAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlayAgentActionAdvanceV1 {
    OnePublicAction,
    NextStrategicDecision,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayAgentCommitEvidenceV1 {
    pub kind: RunPlayAgentCommitKindV1,
    pub action: String,
    pub advance: Option<RunPlayAgentActionAdvanceV1>,
    pub root_session_id: String,
    pub root_public_snapshot_id: Option<String>,
    pub successor_session_id: String,
    pub successor_head_revision: u64,
    pub successor_public_snapshot_id: Option<String>,
    pub final_active_path_disposition: RunPlayAgentCommitDispositionV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RunPlayAgentCommitDispositionV1 {
    OnFinalActivePath {
        final_active_decision: RunPlayAgentFinalActiveDecisionV1,
    },
    RootSessionMismatch,
    SuccessorSessionMismatch,
    MissingRootPublicSnapshot,
    RootNotOnFinalActivePath,
    AmbiguousRootPublicSnapshot {
        matching_active_path_indices: Vec<usize>,
    },
    MissingSuccessorPublicSnapshot,
    SuccessorNotOnFinalActivePath,
    AmbiguousSuccessorPublicSnapshot {
        matching_active_path_indices: Vec<usize>,
    },
    CombatPlanDidNotAdvance {
        root_active_path_index: usize,
        successor_active_path_index: usize,
    },
    CombatTurnDidNotAdvance {
        root_active_path_index: usize,
        successor_active_path_index: usize,
    },
    TranscriptActionMismatch {
        final_active_decision: RunPlayAgentFinalActiveDecisionV1,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RunPlayAgentProposalDispositionV1 {
    Accepted {
        final_active_decision: RunPlayAgentFinalActiveDecisionV1,
    },
    Rejected {
        final_active_decision: RunPlayAgentFinalActiveDecisionV1,
    },
    MissingRootPublicSnapshot,
    RootSessionMismatch {
        root_session_id: String,
    },
    RootNotOnFinalActivePath,
    AmbiguousRootPublicSnapshot {
        matching_active_path_indices: Vec<usize>,
    },
    CombatPlanSelection {
        final_active_decision: RunPlayAgentFinalActiveDecisionV1,
    },
    CombatTurnSelection {
        final_active_decision: RunPlayAgentFinalActiveDecisionV1,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayAgentFinalActiveDecisionV1 {
    pub active_path_index: usize,
    pub site: sts_oracle_learning::agent::task::TaskDecisionSiteV1,
    pub root_public_snapshot_id: String,
    pub root_archive_node_id: String,
    pub successor_archive_node_id: String,
    pub chosen_action: String,
}

#[derive(Clone, Debug, Deserialize)]
struct AgentTranscriptEventV1 {
    schema: String,
    ordinal: u64,
    kind: String,
    payload: Value,
}

#[derive(Clone, Debug)]
struct ParsedAgentTranscriptV1 {
    path: PathBuf,
    session_id: String,
    initial_head_revision: u64,
    complete: bool,
    failed: bool,
    stop_reason: Option<String>,
    model_calls: Vec<RunPlayAgentModelCallV1>,
    proposals: Vec<ParsedAgentProposalV1>,
    commits: Vec<ParsedAgentCommitV1>,
}

#[derive(Clone, Debug)]
struct ParsedAgentProposalV1 {
    turn_id: String,
    root_session_id: Option<String>,
    root_head_revision: Option<u64>,
    root_public_snapshot_id: Option<String>,
    action: String,
    summary: Option<String>,
}

#[derive(Clone, Debug)]
struct ParsedAgentCommitV1 {
    kind: RunPlayAgentCommitKindV1,
    action: String,
    advance: Option<RunPlayAgentActionAdvanceV1>,
    root_session_id: String,
    root_public_snapshot_id: Option<String>,
    successor_session_id: String,
    successor_head_revision: u64,
    successor_public_snapshot_id: Option<String>,
}

#[derive(Clone, Debug)]
struct AgentObservationIdentityV1 {
    session_id: String,
    head_revision: u64,
    public_snapshot_id: Option<String>,
}

type FinalActiveDecisionsBySnapshotV1 = BTreeMap<String, Vec<RunPlayAgentFinalActiveDecisionV1>>;

fn final_active_decisions_by_snapshot_v1(
    workspace: &RunPlayWorkspaceV1,
) -> Result<FinalActiveDecisionsBySnapshotV1, String> {
    let mut decisions = FinalActiveDecisionsBySnapshotV1::new();
    for (index, step) in workspace.head.current_path.iter().enumerate() {
        let node = workspace
            .state
            .archive
            .node(&step.archive_node_id)
            .ok_or_else(|| format!("active path node {} is missing", step.archive_node_id))?;
        let incoming = node.incoming_step.as_ref().ok_or_else(|| {
            format!(
                "active path node {} has no incoming public step",
                step.archive_node_id
            )
        })?;
        let root_archive_node_id = node
            .parent_node_id
            .clone()
            .ok_or_else(|| format!("active path node {} has no parent", step.archive_node_id))?;
        let chosen_action = step.semantic_action.clone().ok_or_else(|| {
            format!(
                "active path step {} lacks a stable semantic action",
                step.archive_node_id
            )
        })?;
        let root_public_snapshot_id = incoming.step.source.information.snapshot_id.clone();
        decisions
            .entry(root_public_snapshot_id.clone())
            .or_default()
            .push(RunPlayAgentFinalActiveDecisionV1 {
                active_path_index: index + 1,
                site: step.site,
                root_public_snapshot_id,
                root_archive_node_id,
                successor_archive_node_id: step.archive_node_id.clone(),
                chosen_action,
            });
    }
    Ok(decisions)
}

pub(crate) fn external_agent_evidence_v1(
    workspace: &RunPlayWorkspaceV1,
    transcript_sources: &[PathBuf],
) -> Result<RunPlayExternalAgentEvidenceV1, String> {
    let head = &workspace.head;
    let final_active_decisions = final_active_decisions_by_snapshot_v1(workspace)?;
    let (input_sources, files) = expand_transcript_sources_v1(transcript_sources)?;
    let mut evidence = RunPlayExternalAgentEvidenceV1 {
        input_sources,
        jsonl_file_count: files.len(),
        ..RunPlayExternalAgentEvidenceV1::default()
    };
    let mut distinct_final_active_decisions = BTreeSet::new();
    let mut distinct_final_active_decision_actions = BTreeSet::new();
    let mut distinct_final_active_commits =
        BTreeMap::<RunPlayAgentCommitKindV1, BTreeSet<usize>>::new();
    for path in files {
        let Some(parsed) = parse_agent_transcript_v1(&path)? else {
            evidence.non_agent_jsonl_count += 1;
            continue;
        };
        evidence.agent_transcript_count += 1;
        if parsed.session_id != head.session_id {
            evidence.other_session_transcript_count += 1;
            continue;
        }
        let receipts = parsed
            .model_calls
            .iter()
            .map(|call| (call.turn_id.as_str(), call.provider_receipt.clone()))
            .collect::<BTreeMap<_, _>>();
        evidence.matched_model_call_count += parsed.model_calls.len();
        for receipt in parsed
            .model_calls
            .iter()
            .filter_map(|call| call.provider_receipt.as_ref())
        {
            evidence.provider_receipt_count += 1;
            if let Some(usage) = &receipt.usage {
                evidence.token_usage_totals.input_tokens = evidence
                    .token_usage_totals
                    .input_tokens
                    .saturating_add(usage.input_tokens.unwrap_or_default());
                evidence.token_usage_totals.output_tokens = evidence
                    .token_usage_totals
                    .output_tokens
                    .saturating_add(usage.output_tokens.unwrap_or_default());
                evidence.token_usage_totals.total_tokens = evidence
                    .token_usage_totals
                    .total_tokens
                    .saturating_add(usage.total_tokens.unwrap_or_default());
                evidence.token_usage_totals.cached_input_tokens = evidence
                    .token_usage_totals
                    .cached_input_tokens
                    .saturating_add(usage.cached_input_tokens.unwrap_or_default());
                evidence.token_usage_totals.reasoning_tokens = evidence
                    .token_usage_totals
                    .reasoning_tokens
                    .saturating_add(usage.reasoning_tokens.unwrap_or_default());
            }
        }
        let proposals = parsed
            .proposals
            .into_iter()
            .map(|proposal| {
                let disposition = proposal_disposition_v1(
                    head,
                    &final_active_decisions,
                    proposal.root_session_id.as_deref(),
                    proposal.root_public_snapshot_id.as_deref(),
                    &proposal.action,
                );
                RunPlayAgentProposalEvidenceV1 {
                    provider_receipt: receipts.get(proposal.turn_id.as_str()).cloned().flatten(),
                    turn_id: proposal.turn_id,
                    root_session_id: proposal.root_session_id,
                    root_head_revision: proposal.root_head_revision,
                    root_public_snapshot_id: proposal.root_public_snapshot_id,
                    action: proposal.action,
                    summary: proposal.summary,
                    final_active_path_disposition: disposition,
                }
            })
            .collect::<Vec<_>>();
        let commits = parsed
            .commits
            .into_iter()
            .map(|commit| {
                let disposition = commit_disposition_v1(head, &final_active_decisions, &commit);
                match commit.kind {
                    RunPlayAgentCommitKindV1::ModelAction => {
                        evidence.model_action_commit_receipt_count += 1;
                    }
                    RunPlayAgentCommitKindV1::CombatPlan => {
                        evidence.combat_plan_commit_receipt_count += 1;
                    }
                    RunPlayAgentCommitKindV1::CombatMilestone => {
                        evidence.combat_milestone_commit_receipt_count += 1;
                    }
                    RunPlayAgentCommitKindV1::CombatTurn => {
                        evidence.combat_turn_commit_receipt_count += 1;
                    }
                    RunPlayAgentCommitKindV1::ForcedAction => {
                        evidence.forced_action_commit_receipt_count += 1;
                    }
                    RunPlayAgentCommitKindV1::RoutineAction => {
                        evidence.routine_action_commit_receipt_count += 1;
                    }
                }
                if let RunPlayAgentCommitDispositionV1::OnFinalActivePath {
                    final_active_decision,
                } = &disposition
                {
                    distinct_final_active_commits
                        .entry(commit.kind)
                        .or_default()
                        .insert(final_active_decision.active_path_index);
                }
                RunPlayAgentCommitEvidenceV1 {
                    kind: commit.kind,
                    action: commit.action,
                    advance: commit.advance,
                    root_session_id: commit.root_session_id,
                    root_public_snapshot_id: commit.root_public_snapshot_id,
                    successor_session_id: commit.successor_session_id,
                    successor_head_revision: commit.successor_head_revision,
                    successor_public_snapshot_id: commit.successor_public_snapshot_id,
                    final_active_path_disposition: disposition,
                }
            })
            .collect::<Vec<_>>();
        evidence.proposal_count += proposals.len();
        for proposal in &proposals {
            match &proposal.final_active_path_disposition {
                RunPlayAgentProposalDispositionV1::Accepted {
                    final_active_decision,
                } => {
                    evidence.proposal_accepted_count += 1;
                    distinct_final_active_decisions.insert(final_active_decision.active_path_index);
                    distinct_final_active_decision_actions.insert((
                        final_active_decision.active_path_index,
                        proposal.action.clone(),
                    ));
                }
                RunPlayAgentProposalDispositionV1::Rejected {
                    final_active_decision,
                } => {
                    evidence.proposal_rejected_count += 1;
                    distinct_final_active_decisions.insert(final_active_decision.active_path_index);
                    distinct_final_active_decision_actions.insert((
                        final_active_decision.active_path_index,
                        proposal.action.clone(),
                    ));
                }
                _ => evidence.proposal_unresolved_count += 1,
            }
        }
        evidence
            .matched_transcripts
            .push(RunPlayAgentTranscriptEvidenceV1 {
                path: parsed.path.display().to_string(),
                session_id: parsed.session_id,
                initial_head_revision: parsed.initial_head_revision,
                complete: parsed.complete,
                failed: parsed.failed,
                stop_reason: parsed.stop_reason,
                model_calls: parsed.model_calls,
                proposals,
                commits,
            });
    }
    evidence.distinct_final_active_decision_count_with_proposal =
        distinct_final_active_decisions.len();
    evidence.distinct_final_active_decision_action_proposal_count =
        distinct_final_active_decision_actions.len();
    evidence.distinct_final_active_model_action_commit_count = distinct_final_active_commits
        .get(&RunPlayAgentCommitKindV1::ModelAction)
        .map_or(0, BTreeSet::len);
    evidence.distinct_final_active_combat_plan_commit_count = distinct_final_active_commits
        .get(&RunPlayAgentCommitKindV1::CombatPlan)
        .map_or(0, BTreeSet::len);
    evidence.distinct_final_active_combat_milestone_commit_count = distinct_final_active_commits
        .get(&RunPlayAgentCommitKindV1::CombatMilestone)
        .map_or(0, BTreeSet::len);
    evidence.distinct_final_active_combat_turn_commit_count = distinct_final_active_commits
        .get(&RunPlayAgentCommitKindV1::CombatTurn)
        .map_or(0, BTreeSet::len);
    evidence.distinct_final_active_forced_action_commit_count = distinct_final_active_commits
        .get(&RunPlayAgentCommitKindV1::ForcedAction)
        .map_or(0, BTreeSet::len);
    evidence.distinct_final_active_routine_action_commit_count = distinct_final_active_commits
        .get(&RunPlayAgentCommitKindV1::RoutineAction)
        .map_or(0, BTreeSet::len);
    Ok(evidence)
}

fn commit_disposition_v1(
    head: &crate::model::RunPlayHeadV1,
    final_active_decisions: &FinalActiveDecisionsBySnapshotV1,
    commit: &ParsedAgentCommitV1,
) -> RunPlayAgentCommitDispositionV1 {
    if commit.root_session_id != head.session_id {
        return RunPlayAgentCommitDispositionV1::RootSessionMismatch;
    }
    if commit.successor_session_id != head.session_id {
        return RunPlayAgentCommitDispositionV1::SuccessorSessionMismatch;
    }
    let Some(root_public_snapshot_id) = commit.root_public_snapshot_id.as_deref() else {
        return RunPlayAgentCommitDispositionV1::MissingRootPublicSnapshot;
    };
    let Some(matches) = final_active_decisions.get(root_public_snapshot_id) else {
        return RunPlayAgentCommitDispositionV1::RootNotOnFinalActivePath;
    };
    if matches.len() != 1 {
        return RunPlayAgentCommitDispositionV1::AmbiguousRootPublicSnapshot {
            matching_active_path_indices: matches
                .iter()
                .map(|decision| decision.active_path_index)
                .collect(),
        };
    }
    let final_active_decision = matches[0].clone();
    if matches!(
        commit.kind,
        RunPlayAgentCommitKindV1::CombatPlan | RunPlayAgentCommitKindV1::CombatTurn
    ) {
        let Some(successor_public_snapshot_id) = commit.successor_public_snapshot_id.as_deref()
        else {
            if commit.successor_head_revision == head.head_revision
                && matches!(
                    head.position,
                    crate::model::RunPlayPositionV1::Terminal { .. }
                )
            {
                return RunPlayAgentCommitDispositionV1::OnFinalActivePath {
                    final_active_decision,
                };
            }
            return RunPlayAgentCommitDispositionV1::MissingSuccessorPublicSnapshot;
        };
        let Some(successor_matches) = final_active_decisions.get(successor_public_snapshot_id)
        else {
            return RunPlayAgentCommitDispositionV1::SuccessorNotOnFinalActivePath;
        };
        if successor_matches.len() != 1 {
            return RunPlayAgentCommitDispositionV1::AmbiguousSuccessorPublicSnapshot {
                matching_active_path_indices: successor_matches
                    .iter()
                    .map(|decision| decision.active_path_index)
                    .collect(),
            };
        }
        let successor_active_path_index = successor_matches[0].active_path_index;
        if successor_active_path_index <= final_active_decision.active_path_index {
            return match commit.kind {
                RunPlayAgentCommitKindV1::CombatPlan => {
                    RunPlayAgentCommitDispositionV1::CombatPlanDidNotAdvance {
                        root_active_path_index: final_active_decision.active_path_index,
                        successor_active_path_index,
                    }
                }
                RunPlayAgentCommitKindV1::CombatTurn => {
                    RunPlayAgentCommitDispositionV1::CombatTurnDidNotAdvance {
                        root_active_path_index: final_active_decision.active_path_index,
                        successor_active_path_index,
                    }
                }
                _ => unreachable!("multi-step commit kinds matched above"),
            };
        }
        return RunPlayAgentCommitDispositionV1::OnFinalActivePath {
            final_active_decision,
        };
    }
    if final_active_decision.chosen_action != commit.action {
        return RunPlayAgentCommitDispositionV1::TranscriptActionMismatch {
            final_active_decision,
        };
    }
    RunPlayAgentCommitDispositionV1::OnFinalActivePath {
        final_active_decision,
    }
}

fn proposal_disposition_v1(
    head: &crate::model::RunPlayHeadV1,
    final_active_decisions: &FinalActiveDecisionsBySnapshotV1,
    root_session_id: Option<&str>,
    root_public_snapshot_id: Option<&str>,
    proposed_action: &str,
) -> RunPlayAgentProposalDispositionV1 {
    let Some(root_session_id) = root_session_id else {
        return RunPlayAgentProposalDispositionV1::MissingRootPublicSnapshot;
    };
    if root_session_id != head.session_id {
        return RunPlayAgentProposalDispositionV1::RootSessionMismatch {
            root_session_id: root_session_id.to_string(),
        };
    }
    let Some(root_public_snapshot_id) = root_public_snapshot_id else {
        return RunPlayAgentProposalDispositionV1::MissingRootPublicSnapshot;
    };
    let Some(matches) = final_active_decisions.get(root_public_snapshot_id) else {
        return RunPlayAgentProposalDispositionV1::RootNotOnFinalActivePath;
    };
    if matches.len() != 1 {
        return RunPlayAgentProposalDispositionV1::AmbiguousRootPublicSnapshot {
            matching_active_path_indices: matches
                .iter()
                .map(|decision| decision.active_path_index)
                .collect(),
        };
    }
    let final_active_decision = matches[0].clone();
    if proposed_action.starts_with("combat-plan:") {
        return RunPlayAgentProposalDispositionV1::CombatPlanSelection {
            final_active_decision,
        };
    }
    if proposed_action.starts_with("combat-turn:") {
        return RunPlayAgentProposalDispositionV1::CombatTurnSelection {
            final_active_decision,
        };
    }
    if final_active_decision.chosen_action == proposed_action {
        RunPlayAgentProposalDispositionV1::Accepted {
            final_active_decision,
        }
    } else {
        RunPlayAgentProposalDispositionV1::Rejected {
            final_active_decision,
        }
    }
}

fn expand_transcript_sources_v1(
    sources: &[PathBuf],
) -> Result<(Vec<String>, Vec<PathBuf>), String> {
    let mut input_sources = Vec::new();
    let mut files = BTreeSet::new();
    for source in sources {
        let resolved = fs::canonicalize(source).map_err(|error| {
            format!(
                "failed to resolve transcript source {}: {error}",
                source.display()
            )
        })?;
        input_sources.push(resolved.display().to_string());
        if resolved.is_file() {
            files.insert(resolved);
            continue;
        }
        if !resolved.is_dir() {
            return Err(format!(
                "transcript source is neither a file nor directory: {}",
                resolved.display()
            ));
        }
        for entry in fs::read_dir(&resolved).map_err(|error| {
            format!(
                "failed to list transcript directory {}: {error}",
                resolved.display()
            )
        })? {
            let path = entry
                .map_err(|error| {
                    format!(
                        "failed to read transcript directory entry in {}: {error}",
                        resolved.display()
                    )
                })?
                .path();
            if path.is_file() && path.extension().is_some_and(|value| value == "jsonl") {
                files.insert(path);
            }
        }
    }
    Ok((input_sources, files.into_iter().collect()))
}

fn parse_agent_transcript_v1(path: &Path) -> Result<Option<ParsedAgentTranscriptV1>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect transcript {}: {error}", path.display()))?;
    if metadata.len() > MAX_AGENT_TRANSCRIPT_BYTES_V1 {
        return Err(format!(
            "transcript {} exceeds {} bytes",
            path.display(),
            MAX_AGENT_TRANSCRIPT_BYTES_V1
        ));
    }
    let payload = fs::read_to_string(path)
        .map_err(|error| format!("failed to read transcript {}: {error}", path.display()))?;
    parse_agent_transcript_payload_v1(path, &payload)
}

fn parse_agent_transcript_payload_v1(
    path: &Path,
    payload: &str,
) -> Result<Option<ParsedAgentTranscriptV1>, String> {
    let Some(first_line) = payload.lines().find(|line| !line.trim().is_empty()) else {
        return Ok(None);
    };
    let first_value: Value = serde_json::from_str(first_line).map_err(|error| {
        format!(
            "failed to decode first JSONL event in {}: {error}",
            path.display()
        )
    })?;
    if first_value.get("schema").and_then(Value::as_str) != Some(AGENT_TRANSCRIPT_EVENT_SCHEMA_V1) {
        return Ok(None);
    }
    let mut events = Vec::new();
    for (line_index, line) in payload.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event: AgentTranscriptEventV1 = serde_json::from_str(line).map_err(|error| {
            format!(
                "failed to decode agent transcript {} line {}: {error}",
                path.display(),
                line_index + 1
            )
        })?;
        if event.schema != AGENT_TRANSCRIPT_EVENT_SCHEMA_V1 {
            return Err(format!(
                "agent transcript {} line {} has schema {}",
                path.display(),
                line_index + 1,
                event.schema
            ));
        }
        if event.ordinal != events.len() as u64 {
            return Err(format!(
                "agent transcript {} has non-contiguous ordinal {}",
                path.display(),
                event.ordinal
            ));
        }
        events.push(event);
    }
    let start = events
        .iter()
        .find(|event| event.kind == "start")
        .ok_or_else(|| format!("agent transcript {} has no start event", path.display()))?;
    let initial = value_object_field_v1(&start.payload, "initial_observation", path)?;
    let mut current_identity = observation_identity_v1(initial, path)?;
    let session_id = current_identity.session_id.clone();
    let initial_head_revision = current_identity.head_revision;
    let decision_stages = events
        .iter()
        .filter(|event| event.kind == "model_turn")
        .filter_map(|event| {
            Some((
                event.payload.get("turn_id")?.as_str()?.to_string(),
                event
                    .payload
                    .get("decision_stage")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut model_calls = Vec::new();
    for event in events.iter().filter(|event| event.kind == "model_response") {
        let turn_id = value_string_field_v1(&event.payload, "turn_id", path)?.to_string();
        let response_kind = value_string_field_v1(&event.payload, "kind", path)?.to_string();
        let provider_receipt = event
            .payload
            .get("provider_receipt")
            .filter(|value| !value.is_null())
            .map(|value| {
                serde_json::from_value(value.clone()).map_err(|error| {
                    format!(
                        "agent transcript {} turn {turn_id} has invalid provider receipt: {error}",
                        path.display()
                    )
                })
            })
            .transpose()?;
        model_calls.push(RunPlayAgentModelCallV1 {
            decision_stage: decision_stages.get(&turn_id).cloned().flatten(),
            turn_id,
            response_kind,
            provider_receipt,
        });
    }
    let mut proposals = Vec::new();
    let mut commits = Vec::new();
    for event in &events {
        if event.kind == "validated_proposal" {
            if let Some(claimed_session) =
                event.payload.get("root_session_id").and_then(Value::as_str)
            {
                if claimed_session != current_identity.session_id {
                    return Err(format!(
                        "agent transcript {} proposal root session does not match its current observation",
                        path.display()
                    ));
                }
            }
            if let Some(claimed_head) = event
                .payload
                .get("root_head_revision")
                .and_then(Value::as_u64)
            {
                if claimed_head != current_identity.head_revision {
                    return Err(format!(
                        "agent transcript {} proposal root head does not match its current observation",
                        path.display()
                    ));
                }
            }
            proposals.push(ParsedAgentProposalV1 {
                turn_id: value_string_field_v1(&event.payload, "turn_id", path)?.to_string(),
                root_session_id: Some(current_identity.session_id.clone()),
                root_head_revision: Some(current_identity.head_revision),
                root_public_snapshot_id: current_identity.public_snapshot_id.clone(),
                action: value_string_field_v1(&event.payload, "action", path)?.to_string(),
                summary: event
                    .payload
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
            continue;
        }
        let kind = match event.kind.as_str() {
            "committed_action" => RunPlayAgentCommitKindV1::ModelAction,
            "committed_combat_plan" => RunPlayAgentCommitKindV1::CombatPlan,
            "committed_combat_milestone" => RunPlayAgentCommitKindV1::CombatMilestone,
            "committed_combat_turn" => RunPlayAgentCommitKindV1::CombatTurn,
            "committed_forced_action" => RunPlayAgentCommitKindV1::ForcedAction,
            "committed_routine_action" => RunPlayAgentCommitKindV1::RoutineAction,
            _ => continue,
        };
        let successor = value_object_field_v1(&event.payload, "observed_successor", path)?;
        let successor_identity = observation_identity_v1(successor, path)?;
        commits.push(ParsedAgentCommitV1 {
            kind,
            action: value_string_field_v1(&event.payload, "action", path)?.to_string(),
            advance: agent_action_advance_v1(&event.payload, path)?,
            root_session_id: current_identity.session_id.clone(),
            root_public_snapshot_id: current_identity.public_snapshot_id.clone(),
            successor_session_id: successor_identity.session_id.clone(),
            successor_head_revision: successor_identity.head_revision,
            successor_public_snapshot_id: successor_identity.public_snapshot_id.clone(),
        });
        current_identity = successor_identity;
    }
    let stop_reason = events
        .iter()
        .rev()
        .find(|event| event.kind == "stop")
        .and_then(|event| event.payload.get("reason"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(Some(ParsedAgentTranscriptV1 {
        path: path.to_path_buf(),
        session_id,
        initial_head_revision,
        complete: stop_reason.is_some(),
        failed: events.iter().any(|event| event.kind == "error"),
        stop_reason,
        model_calls,
        proposals,
        commits,
    }))
}

fn agent_action_advance_v1(
    payload: &Value,
    path: &Path,
) -> Result<Option<RunPlayAgentActionAdvanceV1>, String> {
    let Some(value) = payload.get("advance") else {
        return Ok(None);
    };
    let value = value.as_str().ok_or_else(|| {
        format!(
            "agent transcript {} field advance must be a string",
            path.display()
        )
    })?;
    match value {
        "one_public_action" => Ok(Some(RunPlayAgentActionAdvanceV1::OnePublicAction)),
        "next_strategic_decision" => Ok(Some(RunPlayAgentActionAdvanceV1::NextStrategicDecision)),
        _ => Err(format!(
            "agent transcript {} field advance has unsupported value {value}",
            path.display()
        )),
    }
}

fn value_object_field_v1<'a>(
    value: &'a Value,
    field: &str,
    path: &Path,
) -> Result<&'a Value, String> {
    value
        .get(field)
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            format!(
                "agent transcript {} field {field} must be an object",
                path.display()
            )
        })
}

fn observation_identity_v1(
    observation: &Value,
    path: &Path,
) -> Result<AgentObservationIdentityV1, String> {
    let public_snapshot_id = observation
        .get("position")
        .and_then(Value::as_object)
        .and_then(|position| position.get("public_snapshot_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(AgentObservationIdentityV1 {
        session_id: value_string_field_v1(observation, "session_id", path)?.to_string(),
        head_revision: value_u64_field_v1(observation, "head_revision", path)?,
        public_snapshot_id,
    })
}

fn value_string_field_v1<'a>(
    value: &'a Value,
    field: &str,
    path: &Path,
) -> Result<&'a str, String> {
    value.get(field).and_then(Value::as_str).ok_or_else(|| {
        format!(
            "agent transcript {} field {field} must be a string",
            path.display()
        )
    })
}

fn value_u64_field_v1(value: &Value, field: &str, path: &Path) -> Result<u64, String> {
    value.get(field).and_then(Value::as_u64).ok_or_else(|| {
        format!(
            "agent transcript {} field {field} must be a non-negative integer",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_projection_keeps_root_identity_and_paid_token_receipt() {
        let lines = [
            serde_json::json!({
                "schema": AGENT_TRANSCRIPT_EVENT_SCHEMA_V1,
                "ordinal": 0,
                "kind": "start",
                "payload": {"initial_observation": {
                    "session_id": "seed-0-fixture",
                    "head_revision": 247,
                    "position": {
                        "kind": "decision",
                        "public_snapshot_id": "root-snapshot"
                    }
                }}
            }),
            serde_json::json!({
                "schema": AGENT_TRANSCRIPT_EVENT_SCHEMA_V1,
                "ordinal": 1,
                "kind": "model_turn",
                "payload": {"turn_id": "h247-a0-r0", "decision_stage": "final"}
            }),
            serde_json::json!({
                "schema": AGENT_TRANSCRIPT_EVENT_SCHEMA_V1,
                "ordinal": 2,
                "kind": "model_response",
                "payload": {
                    "turn_id": "h247-a0-r0",
                    "kind": "act",
                    "provider_receipt": {
                        "transport": "openai_compatible_chat_completions_v1",
                        "model": "deepseek-v4-flash",
                        "usage": {"input_tokens": 18143, "output_tokens": 1153}
                    }
                }
            }),
            serde_json::json!({
                "schema": AGENT_TRANSCRIPT_EVENT_SCHEMA_V1,
                "ordinal": 3,
                "kind": "validated_proposal",
                "payload": {
                    "turn_id": "h247-a0-r0",
                    "root_session_id": "seed-0-fixture",
                    "root_head_revision": 247,
                    "action": "take:ShrugItOff+#0",
                    "summary": "proposal"
                }
            }),
            serde_json::json!({
                "schema": AGENT_TRANSCRIPT_EVENT_SCHEMA_V1,
                "ordinal": 4,
                "kind": "committed_action",
                "payload": {
                    "action": "take:ShrugItOff+#0",
                    "advance": "next_strategic_decision",
                    "observed_successor": {
                        "session_id": "seed-0-fixture",
                        "head_revision": 248,
                        "position": {
                            "kind": "decision",
                            "public_snapshot_id": "successor-snapshot"
                        }
                    }
                }
            }),
            serde_json::json!({
                "schema": AGENT_TRANSCRIPT_EVENT_SCHEMA_V1,
                "ordinal": 5,
                "kind": "committed_combat_plan",
                "payload": {
                    "action": "combat-plan:fixture-candidate",
                    "candidate_id": "fixture-candidate",
                    "observed_successor": {
                        "session_id": "seed-0-fixture",
                        "head_revision": 249,
                        "position": {
                            "kind": "decision",
                            "public_snapshot_id": "post-combat-snapshot"
                        }
                    }
                }
            }),
            serde_json::json!({
                "schema": AGENT_TRANSCRIPT_EVENT_SCHEMA_V1,
                "ordinal": 6,
                "kind": "committed_combat_turn",
                "payload": {
                    "action": "combat-turn:fixture-turn",
                    "candidate_id": "fixture-turn",
                    "observed_successor": {
                        "session_id": "seed-0-fixture",
                        "head_revision": 250,
                        "position": {
                            "kind": "decision",
                            "public_snapshot_id": "next-turn-snapshot"
                        }
                    }
                }
            }),
            serde_json::json!({
                "schema": AGENT_TRANSCRIPT_EVENT_SCHEMA_V1,
                "ordinal": 7,
                "kind": "stop",
                "payload": {"reason": "proposal_only"}
            }),
        ];
        let payload = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        let parsed = parse_agent_transcript_payload_v1(Path::new("fixture.jsonl"), &payload)
            .expect("valid transcript")
            .expect("agent transcript");

        assert_eq!(parsed.session_id, "seed-0-fixture");
        assert_eq!(parsed.initial_head_revision, 247);
        assert_eq!(parsed.proposals[0].root_head_revision, Some(247));
        assert_eq!(
            parsed.proposals[0].root_public_snapshot_id.as_deref(),
            Some("root-snapshot")
        );
        assert_eq!(parsed.commits.len(), 3);
        assert_eq!(
            parsed.commits[0].kind,
            RunPlayAgentCommitKindV1::ModelAction
        );
        assert_eq!(parsed.commits[0].successor_head_revision, 248);
        assert_eq!(
            parsed.commits[0].advance,
            Some(RunPlayAgentActionAdvanceV1::NextStrategicDecision)
        );
        assert_eq!(parsed.commits[0].root_session_id, "seed-0-fixture");
        assert_eq!(
            parsed.commits[0].root_public_snapshot_id.as_deref(),
            Some("root-snapshot")
        );
        assert_eq!(
            parsed.commits[0].successor_public_snapshot_id.as_deref(),
            Some("successor-snapshot")
        );
        assert_eq!(parsed.commits[1].kind, RunPlayAgentCommitKindV1::CombatPlan);
        assert_eq!(parsed.commits[1].advance, None);
        assert_eq!(
            parsed.commits[1].root_public_snapshot_id.as_deref(),
            Some("successor-snapshot")
        );
        assert_eq!(
            parsed.commits[1].successor_public_snapshot_id.as_deref(),
            Some("post-combat-snapshot")
        );
        assert_eq!(parsed.commits[2].kind, RunPlayAgentCommitKindV1::CombatTurn);
        assert_eq!(parsed.commits[2].advance, None);
        assert_eq!(
            parsed.commits[2].root_public_snapshot_id.as_deref(),
            Some("post-combat-snapshot")
        );
        assert_eq!(
            parsed.commits[2].successor_public_snapshot_id.as_deref(),
            Some("next-turn-snapshot")
        );
        let receipt = parsed.model_calls[0]
            .provider_receipt
            .as_ref()
            .expect("provider receipt");
        assert_eq!(receipt.model, "deepseek-v4-flash");
        assert_eq!(
            receipt.usage.as_ref().and_then(|usage| usage.input_tokens),
            Some(18143)
        );

        let unsupported_advance = payload.replace("next_strategic_decision", "teleport");
        assert!(parse_agent_transcript_payload_v1(
            Path::new("unsupported-advance.jsonl"),
            &unsupported_advance
        )
        .is_err());
    }
}
