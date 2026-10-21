use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;
use sts_oracle_learning::agent::task::{
    TaskDecisionSiteV1, TaskEpisodeOutcomeV1, TaskPrivilegedSupportProvenanceV1,
};

use crate::model::{
    RunPlayBoundaryKindV1, RunPlayCombatMilestoneCandidateV1, RunPlayCombatResearchCandidateV1,
    RunPlayEventV1, RunPlayHeadV1, RunPlayPositionV1, RunPlayPublicScalarsV1,
    RunPlayStrategicOwnerAdviceV1,
};
use crate::storage::{RunPlayPathsV1, RunPlayWorkspaceV1};
use crate::victory_dossier_agent::{external_agent_evidence_v1, RunPlayExternalAgentEvidenceV1};

pub const RUN_PLAY_VICTORY_DOSSIER_SCHEMA_V1: &str = "sts-run-play-victory-dossier-v1";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictoryDossierV1 {
    pub schema: String,
    pub session: RunPlayVictorySessionV1,
    pub active_path: RunPlayVictoryActivePathV1,
    pub strategic_decisions: Vec<RunPlayVictoryStrategicDecisionV1>,
    pub controller_usage: RunPlayVictoryControllerUsageV1,
    pub external_agent_evidence: RunPlayExternalAgentEvidenceV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictorySessionV1 {
    pub session_id: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub final_archive_node_id: String,
    pub outcome: TaskEpisodeOutcomeV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictoryActivePathV1 {
    pub public_decision_count: usize,
    pub strategic_decision_count: usize,
    pub combat_decision_count: usize,
    pub site_counts: Vec<RunPlayVictorySiteCountV1>,
    pub source_counts: RunPlayVictoryActivePathSourceCountsV1,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RunPlayVictoryActivePathSourceCountsV1 {
    pub direct_choice: RunPlayVictoryDecisionDomainCountV1,
    pub manual_anchor_line: RunPlayVictoryDecisionDomainCountV1,
    pub combat_research_anchor_line: RunPlayVictoryDecisionDomainCountV1,
    pub combat_milestone_anchor_line: RunPlayVictoryDecisionDomainCountV1,
    pub retained_archive_path: RunPlayVictoryDecisionDomainCountV1,
    pub combat_research_producers: Vec<RunPlayVictoryCombatResearchProducerPathCountV1>,
    pub combat_milestone_producers: Vec<RunPlayVictoryCombatResearchProducerPathCountV1>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RunPlayVictoryDecisionDomainCountV1 {
    pub public_decisions: usize,
    pub strategic_decisions: usize,
    pub combat_decisions: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictoryCombatResearchProducerPathCountV1 {
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub decisions: RunPlayVictoryDecisionDomainCountV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictorySiteCountV1 {
    pub site: TaskDecisionSiteV1,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictoryStrategicDecisionV1 {
    pub active_path_index: usize,
    pub archive_node_id: String,
    pub depth: usize,
    pub site: TaskDecisionSiteV1,
    pub before: RunPlayPublicScalarsV1,
    pub chosen_action: String,
    pub observed_boundary: RunPlayBoundaryKindV1,
    pub source: RunPlayVictoryDecisionSourceV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunPlayVictoryDecisionSourceV1 {
    DirectChoice {
        head_revision: u64,
        executed_new_branch: bool,
        attempt_id: Option<String>,
        owner: Option<RunPlayVictoryOwnerComparisonV1>,
    },
    AnchorLine {
        head_revision: u64,
        line_step_offset: usize,
        line_step_count: usize,
        combat_research_supporting_provenances: Vec<TaskPrivilegedSupportProvenanceV1>,
        combat_milestone_provenance: Option<TaskPrivilegedSupportProvenanceV1>,
    },
    RetainedArchivePath,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictoryOwnerComparisonV1 {
    pub advice: RunPlayStrategicOwnerAdviceV1,
    pub recommendation_matches_chosen_action: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RunPlayVictoryControllerUsageV1 {
    pub choice_event_count: usize,
    pub active_choice_event_count: usize,
    pub strategic_owner_advice_on_active_choice_count: usize,
    pub strategic_owner_recommendation_accepted_count: usize,
    pub strategic_owner_recommendation_overridden_count: usize,
    pub strategic_owner_recommendation_unavailable_count: usize,
    pub manual_anchor_line_count: usize,
    pub manual_anchor_line_step_count: usize,
    pub active_manual_anchor_line_count: usize,
    pub active_manual_anchor_line_step_count: usize,
    pub combat_research_anchor_count: usize,
    pub active_combat_research_anchor_count: usize,
    pub combat_research_anchors: Vec<RunPlayVictoryCombatResearchAnchorV1>,
    pub combat_milestone_anchor_count: usize,
    pub active_combat_milestone_anchor_count: usize,
    pub combat_milestone_anchors: Vec<RunPlayVictoryCombatMilestoneAnchorV1>,
    pub navigation_events: Vec<RunPlayVictoryNavigationEventV1>,
    pub checkpoint_event_count: usize,
    pub compact_active_event_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictoryCombatResearchAnchorV1 {
    pub head_revision: u64,
    pub from_archive_node_id: String,
    pub to_archive_node_id: String,
    pub line_step_count: usize,
    pub lies_on_final_active_path: bool,
    pub candidate: RunPlayCombatResearchCandidateV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunPlayVictoryCombatMilestoneAnchorV1 {
    pub head_revision: u64,
    pub from_archive_node_id: String,
    pub to_archive_node_id: String,
    pub line_step_count: usize,
    pub lies_on_final_active_path: bool,
    pub candidate: RunPlayCombatMilestoneCandidateV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunPlayVictoryNavigationEventV1 {
    Back {
        head_revision: u64,
        from_archive_node_id: String,
        to_archive_node_id: String,
        steps: usize,
        target_lies_on_final_active_path: bool,
    },
    Jump {
        head_revision: u64,
        from_archive_node_id: String,
        to_archive_node_id: String,
        selector: String,
        target_lies_on_final_active_path: bool,
    },
}

pub fn victory_dossier_v1(
    paths: &RunPlayPathsV1,
    transcript_sources: &[PathBuf],
) -> Result<RunPlayVictoryDossierV1, String> {
    let workspace = paths.load()?;
    let outcome = match &workspace.head.position {
        RunPlayPositionV1::Terminal { outcome, .. }
            if matches!(outcome, TaskEpisodeOutcomeV1::Success { .. }) =>
        {
            outcome.clone()
        }
        RunPlayPositionV1::Terminal { outcome, .. } => {
            return Err(format!(
                "victory dossier requires a successful terminal head, got {outcome:?}"
            ));
        }
        RunPlayPositionV1::Decision { .. } => {
            return Err("victory dossier requires a terminal run-play head".to_string());
        }
    };
    let active_nodes = active_node_ids_v1(&workspace)?;
    let active_positions = active_nodes
        .iter()
        .enumerate()
        .map(|(index, node_id)| (node_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let sources = decision_sources_v1(&workspace.head, &active_positions)?;
    let source_counts = active_path_source_counts_v1(&workspace.head, &sources);
    let mut site_counts = BTreeMap::<TaskDecisionSiteV1, usize>::new();
    for step in &workspace.head.current_path {
        *site_counts.entry(step.site).or_default() += 1;
    }
    let strategic_decisions = workspace
        .head
        .current_path
        .iter()
        .zip(sources)
        .enumerate()
        .filter(|(_, (step, _))| step.site != TaskDecisionSiteV1::Combat)
        .map(|(index, (step, source))| {
            Ok(RunPlayVictoryStrategicDecisionV1 {
                active_path_index: index + 1,
                archive_node_id: step.archive_node_id.clone(),
                depth: step.depth,
                site: step.site,
                before: step.before,
                chosen_action: step.semantic_action.clone().ok_or_else(|| {
                    format!(
                        "active path step {} lacks a stable semantic action",
                        step.archive_node_id
                    )
                })?,
                observed_boundary: step.observed_boundary,
                source,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let controller_usage = controller_usage_v1(&workspace.head, &active_positions)?;
    let external_agent_evidence = external_agent_evidence_v1(&workspace, transcript_sources)?;
    Ok(RunPlayVictoryDossierV1 {
        schema: RUN_PLAY_VICTORY_DOSSIER_SCHEMA_V1.to_string(),
        session: RunPlayVictorySessionV1 {
            session_id: workspace.head.session_id.clone(),
            head_revision: workspace.head.head_revision,
            state_revision: workspace.head.state_revision,
            final_archive_node_id: workspace.head.position.archive_node_id().to_string(),
            outcome,
        },
        active_path: RunPlayVictoryActivePathV1 {
            public_decision_count: workspace.head.current_path.len(),
            strategic_decision_count: strategic_decisions.len(),
            combat_decision_count: workspace
                .head
                .current_path
                .iter()
                .filter(|step| step.site == TaskDecisionSiteV1::Combat)
                .count(),
            site_counts: site_counts
                .into_iter()
                .map(|(site, count)| RunPlayVictorySiteCountV1 { site, count })
                .collect(),
            source_counts,
        },
        strategic_decisions,
        controller_usage,
        external_agent_evidence,
    })
}

fn record_domain_count_v1(
    count: &mut RunPlayVictoryDecisionDomainCountV1,
    site: TaskDecisionSiteV1,
) {
    count.public_decisions += 1;
    if site == TaskDecisionSiteV1::Combat {
        count.combat_decisions += 1;
    } else {
        count.strategic_decisions += 1;
    }
}

fn active_path_source_counts_v1(
    head: &RunPlayHeadV1,
    sources: &[RunPlayVictoryDecisionSourceV1],
) -> RunPlayVictoryActivePathSourceCountsV1 {
    let mut counts = RunPlayVictoryActivePathSourceCountsV1::default();
    for (step, source) in head.current_path.iter().zip(sources) {
        match source {
            RunPlayVictoryDecisionSourceV1::DirectChoice { .. } => {
                record_domain_count_v1(&mut counts.direct_choice, step.site);
            }
            RunPlayVictoryDecisionSourceV1::AnchorLine {
                combat_research_supporting_provenances,
                combat_milestone_provenance: None,
                ..
            } if combat_research_supporting_provenances.is_empty() => {
                record_domain_count_v1(&mut counts.manual_anchor_line, step.site);
            }
            RunPlayVictoryDecisionSourceV1::AnchorLine {
                combat_research_supporting_provenances,
                combat_milestone_provenance: None,
                ..
            } if !combat_research_supporting_provenances.is_empty() => {
                record_domain_count_v1(&mut counts.combat_research_anchor_line, step.site);
                for provenance in combat_research_supporting_provenances {
                    let producer = counts
                        .combat_research_producers
                        .iter_mut()
                        .find(|entry| entry.provenance == *provenance);
                    if let Some(producer) = producer {
                        record_domain_count_v1(&mut producer.decisions, step.site);
                    } else {
                        let mut decisions = RunPlayVictoryDecisionDomainCountV1::default();
                        record_domain_count_v1(&mut decisions, step.site);
                        counts.combat_research_producers.push(
                            RunPlayVictoryCombatResearchProducerPathCountV1 {
                                provenance: provenance.clone(),
                                decisions,
                            },
                        );
                    }
                }
            }
            RunPlayVictoryDecisionSourceV1::AnchorLine {
                combat_research_supporting_provenances,
                combat_milestone_provenance: Some(provenance),
                ..
            } if combat_research_supporting_provenances.is_empty() => {
                record_domain_count_v1(&mut counts.combat_milestone_anchor_line, step.site);
                let producer = counts
                    .combat_milestone_producers
                    .iter_mut()
                    .find(|entry| entry.provenance == *provenance);
                if let Some(producer) = producer {
                    record_domain_count_v1(&mut producer.decisions, step.site);
                } else {
                    let mut decisions = RunPlayVictoryDecisionDomainCountV1::default();
                    record_domain_count_v1(&mut decisions, step.site);
                    counts.combat_milestone_producers.push(
                        RunPlayVictoryCombatResearchProducerPathCountV1 {
                            provenance: provenance.clone(),
                            decisions,
                        },
                    );
                }
            }
            RunPlayVictoryDecisionSourceV1::AnchorLine {
                combat_research_supporting_provenances,
                combat_milestone_provenance: Some(_),
                ..
            } if !combat_research_supporting_provenances.is_empty() => {
                unreachable!("one anchor line cannot be both terminal and milestone evidence")
            }
            RunPlayVictoryDecisionSourceV1::AnchorLine { .. } => {
                unreachable!("anchor-line evidence shape was not classified")
            }
            RunPlayVictoryDecisionSourceV1::RetainedArchivePath => {
                record_domain_count_v1(&mut counts.retained_archive_path, step.site);
            }
        }
    }
    counts
}

fn active_node_ids_v1(workspace: &RunPlayWorkspaceV1) -> Result<Vec<String>, String> {
    let first = workspace
        .head
        .current_path
        .first()
        .ok_or_else(|| "successful run-play head has an empty active path".to_string())?;
    let first_node = workspace
        .state
        .archive
        .node(&first.archive_node_id)
        .ok_or_else(|| format!("active path node {} is missing", first.archive_node_id))?;
    let root = first_node.parent_node_id.clone().ok_or_else(|| {
        format!(
            "first active path node {} has no parent root",
            first.archive_node_id
        )
    })?;
    Ok(std::iter::once(root)
        .chain(
            workspace
                .head
                .current_path
                .iter()
                .map(|step| step.archive_node_id.clone()),
        )
        .collect())
}

fn decision_sources_v1(
    head: &RunPlayHeadV1,
    active_positions: &BTreeMap<&str, usize>,
) -> Result<Vec<RunPlayVictoryDecisionSourceV1>, String> {
    let mut sources =
        vec![RunPlayVictoryDecisionSourceV1::RetainedArchivePath; head.current_path.len()];
    for event in &head.events {
        match event {
            RunPlayEventV1::Choice {
                head_revision,
                from_archive_node_id,
                to_archive_node_id: Some(to_archive_node_id),
                strategic_owner_advice,
                executed_new_branch,
                attempt_id,
                ..
            } => {
                let (Some(&from), Some(&to)) = (
                    active_positions.get(from_archive_node_id.as_str()),
                    active_positions.get(to_archive_node_id.as_str()),
                ) else {
                    continue;
                };
                if to != from + 1 {
                    continue;
                }
                let chosen = head.current_path[to - 1].semantic_action.as_deref();
                let owner =
                    strategic_owner_advice
                        .clone()
                        .map(|advice| RunPlayVictoryOwnerComparisonV1 {
                            recommendation_matches_chosen_action: advice
                                .semantic_action
                                .as_deref()
                                .zip(chosen)
                                .map(|(recommended, chosen)| recommended == chosen),
                            advice,
                        });
                sources[to - 1] = RunPlayVictoryDecisionSourceV1::DirectChoice {
                    head_revision: *head_revision,
                    executed_new_branch: *executed_new_branch,
                    attempt_id: attempt_id.clone(),
                    owner,
                };
            }
            RunPlayEventV1::AnchorLine {
                head_revision,
                from_archive_node_id,
                to_archive_node_id,
                choices,
                combat_research_candidate,
                combat_milestone_candidate,
                ..
            } => {
                let Some((from, to)) = active_segment_v1(
                    active_positions,
                    from_archive_node_id,
                    to_archive_node_id,
                    choices.len(),
                )?
                else {
                    continue;
                };
                for position in (from + 1)..=to {
                    sources[position - 1] = RunPlayVictoryDecisionSourceV1::AnchorLine {
                        head_revision: *head_revision,
                        line_step_offset: position - from,
                        line_step_count: choices.len(),
                        combat_research_supporting_provenances: combat_research_candidate
                            .as_ref()
                            .map(|candidate| candidate.supporting_provenances.clone())
                            .unwrap_or_default(),
                        combat_milestone_provenance: combat_milestone_candidate
                            .as_ref()
                            .map(|candidate| candidate.provenance.clone()),
                    };
                }
            }
            _ => {}
        }
    }
    Ok(sources)
}

fn active_segment_v1(
    active_positions: &BTreeMap<&str, usize>,
    from_archive_node_id: &str,
    to_archive_node_id: &str,
    expected_steps: usize,
) -> Result<Option<(usize, usize)>, String> {
    let (Some(&from), Some(&to)) = (
        active_positions.get(from_archive_node_id),
        active_positions.get(to_archive_node_id),
    ) else {
        return Ok(None);
    };
    if to <= from {
        return Ok(None);
    }
    if to - from != expected_steps {
        return Err(format!(
            "active anchor segment {from_archive_node_id}->{to_archive_node_id} has {} path steps but records {expected_steps} choices",
            to - from
        ));
    }
    Ok(Some((from, to)))
}

fn controller_usage_v1(
    head: &RunPlayHeadV1,
    active_positions: &BTreeMap<&str, usize>,
) -> Result<RunPlayVictoryControllerUsageV1, String> {
    let mut usage = RunPlayVictoryControllerUsageV1::default();
    for event in &head.events {
        match event {
            RunPlayEventV1::Choice {
                to_archive_node_id,
                strategic_owner_advice,
                ..
            } => {
                usage.choice_event_count += 1;
                let active = to_archive_node_id
                    .as_deref()
                    .is_some_and(|node| active_positions.contains_key(node));
                if !active {
                    continue;
                }
                usage.active_choice_event_count += 1;
                if let Some(advice) = strategic_owner_advice {
                    usage.strategic_owner_advice_on_active_choice_count += 1;
                    let chosen = to_archive_node_id.as_deref().and_then(|node| {
                        active_positions
                            .get(node)
                            .and_then(|position| position.checked_sub(1))
                            .and_then(|index| head.current_path.get(index))
                            .and_then(|step| step.semantic_action.as_deref())
                    });
                    match advice.semantic_action.as_deref().zip(chosen) {
                        Some((recommended, chosen)) if recommended == chosen => {
                            usage.strategic_owner_recommendation_accepted_count += 1;
                        }
                        Some(_) => {
                            usage.strategic_owner_recommendation_overridden_count += 1;
                        }
                        None => {
                            usage.strategic_owner_recommendation_unavailable_count += 1;
                        }
                    }
                }
            }
            RunPlayEventV1::AnchorLine {
                head_revision,
                from_archive_node_id,
                to_archive_node_id,
                choices,
                combat_research_candidate,
                combat_milestone_candidate,
                ..
            } => {
                let active = active_segment_v1(
                    active_positions,
                    from_archive_node_id,
                    to_archive_node_id,
                    choices.len(),
                )?
                .is_some();
                if let Some(candidate) = combat_research_candidate {
                    usage.combat_research_anchor_count += 1;
                    if active {
                        usage.active_combat_research_anchor_count += 1;
                    }
                    usage
                        .combat_research_anchors
                        .push(RunPlayVictoryCombatResearchAnchorV1 {
                            head_revision: *head_revision,
                            from_archive_node_id: from_archive_node_id.clone(),
                            to_archive_node_id: to_archive_node_id.clone(),
                            line_step_count: choices.len(),
                            lies_on_final_active_path: active,
                            candidate: candidate.clone(),
                        });
                }
                if let Some(candidate) = combat_milestone_candidate {
                    usage.combat_milestone_anchor_count += 1;
                    if active {
                        usage.active_combat_milestone_anchor_count += 1;
                    }
                    usage
                        .combat_milestone_anchors
                        .push(RunPlayVictoryCombatMilestoneAnchorV1 {
                            head_revision: *head_revision,
                            from_archive_node_id: from_archive_node_id.clone(),
                            to_archive_node_id: to_archive_node_id.clone(),
                            line_step_count: choices.len(),
                            lies_on_final_active_path: active,
                            candidate: candidate.clone(),
                        });
                }
                if combat_research_candidate.is_none() && combat_milestone_candidate.is_none() {
                    usage.manual_anchor_line_count += 1;
                    usage.manual_anchor_line_step_count += choices.len();
                    if active {
                        usage.active_manual_anchor_line_count += 1;
                        usage.active_manual_anchor_line_step_count += choices.len();
                    }
                }
            }
            RunPlayEventV1::Back {
                head_revision,
                from_archive_node_id,
                to_archive_node_id,
                steps,
            } => usage
                .navigation_events
                .push(RunPlayVictoryNavigationEventV1::Back {
                    head_revision: *head_revision,
                    from_archive_node_id: from_archive_node_id.clone(),
                    to_archive_node_id: to_archive_node_id.clone(),
                    steps: *steps,
                    target_lies_on_final_active_path: active_positions
                        .contains_key(to_archive_node_id.as_str()),
                }),
            RunPlayEventV1::Jump {
                head_revision,
                from_archive_node_id,
                to_archive_node_id,
                selector,
            } => usage
                .navigation_events
                .push(RunPlayVictoryNavigationEventV1::Jump {
                    head_revision: *head_revision,
                    from_archive_node_id: from_archive_node_id.clone(),
                    to_archive_node_id: to_archive_node_id.clone(),
                    selector: selector.clone(),
                    target_lies_on_final_active_path: active_positions
                        .contains_key(to_archive_node_id.as_str()),
                }),
            RunPlayEventV1::Checkpoint { .. } => usage.checkpoint_event_count += 1,
            RunPlayEventV1::CompactActive { .. } => usage.compact_active_event_count += 1,
        }
    }
    Ok(usage)
}
