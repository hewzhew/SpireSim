use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};
use sts_oracle_runtime::eval::combat_case::CombatCase;
use sts_oracle_runtime::eval::run_control::{
    exact_replay_run_progress_journal_identity_v1, RunProgressStepV1, RunWitnessCombatRootOriginV1,
};
use sts_oracle_runtime::runtime::branch::{OracleAnalysisNodeViewV1, OracleAnalysisWorkspaceV1};

pub(super) fn current_owner_order(
    analysis: &OracleAnalysisWorkspaceV1,
    node: usize,
) -> Result<Vec<String>, String> {
    analysis.session.current_choice_order(node)
}

pub(super) fn selected(
    analysis: &OracleAnalysisWorkspaceV1,
    node: Option<usize>,
) -> Result<OracleAnalysisNodeViewV1, String> {
    if let Some(node) = node {
        analysis.session.view_node(node)
    } else {
        analysis.view()
    }
}

pub(super) fn compact_node(
    view: &OracleAnalysisNodeViewV1,
    limit: usize,
    current_owner_order: &[String],
) -> Value {
    let choices = view
        .choices
        .iter()
        .take(limit)
        .map(|choice| {
            let current_owner_rank = current_owner_order
                .iter()
                .position(|choice_ref| choice_ref == &choice.choice_ref);
            json!({
                "choice_ref": choice.choice_ref,
                "kind": choice.kind,
                "candidate_id": choice.candidate_id,
                "label": choice.label,
                "owner_rank": current_owner_rank,
                "materialized_owner_rank": choice.owner_rank,
                "owner_rank_changed": current_owner_rank != usize::try_from(choice.owner_rank).ok(),
                "path_discrepancy": choice.path_discrepancy,
            })
        })
        .collect::<Vec<_>>();
    let children = view
        .children
        .iter()
        .take(limit)
        .map(|child| {
            json!({
                "edge_id": child.edge_id,
                "child_node_id": child.child_node_id,
                "kind": child.kind,
                "label": child.label,
                "is_on_mainline": child.is_on_mainline,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "node": view.node_id,
        "parent": view.canonical_parent_node_id,
        "act": view.act,
        "floor": view.floor,
        "hp": view.current_hp,
        "max_hp": view.max_hp,
        "gold": view.gold,
        "boundary": view.boundary,
        "state_fingerprint": view.state_fingerprint,
        "event": view.event,
        "keys": view.keys,
        "deck": view.deck,
        "relics": view.relics,
        "potions": view.potions,
        "reward": view.reward,
        "choice_count": view.choices.len(),
        "choices_shown": choices.len(),
        "choices_truncated": view.choices.len() > choices.len(),
        "owner_rank_scope": "current_policy_or_materialized_exact_choice",
        "choices": choices,
        "child_count": view.children.len(),
        "children_shown": children.len(),
        "children_truncated": view.children.len() > children.len(),
        "children": children,
        "encounter": view.encounter,
        "combat": view.combat,
    })
}

pub(super) fn compact_timeline(
    analysis: &OracleAnalysisWorkspaceV1,
    workspace: &Path,
    node: usize,
    tail: usize,
) -> Result<Value, String> {
    let continuation = analysis.continuation(node)?;
    let expected_final = continuation.session.clone().into_session()?;
    let identity = exact_replay_run_progress_journal_identity_v1(
        continuation.seed,
        continuation.ascension,
        &continuation.journal,
        &expected_final,
    )?;
    let combat_roots = identity
        .combat_roots
        .iter()
        .filter_map(|root| root.journal_entry.map(|entry| (entry, root)))
        .collect::<BTreeMap<_, _>>();
    let current_combat_root = identity
        .combat_roots
        .iter()
        .find(|root| root.origin == RunWitnessCombatRootOriginV1::FinalActiveCombat);
    let entries = continuation.journal.entries();
    let start = entries.len().saturating_sub(tail);
    let compact = entries[start..]
        .iter()
        .enumerate()
        .map(|(offset, entry)| -> Result<Value, String> {
            let journal_index = start + offset;
            match entry {
                RunProgressStepV1::Decision(record) => Ok(json!({
                    "journal_index": journal_index,
                    "kind": "decision",
                    "location": record.before.location,
                    "title": record.before.title,
                    "chosen": record.result.chosen_label,
                    "candidates": record.before.candidates.iter().map(|candidate| &candidate.label).collect::<Vec<_>>(),
                })),
                RunProgressStepV1::ForcedTransition(record) => Ok(json!({
                    "journal_index": journal_index,
                    "kind": "forced_transition",
                    "location": record.before.location,
                    "title": record.before.title,
                })),
                RunProgressStepV1::CombatResolution(record) => {
                    let root_identity = combat_roots.get(&journal_index).ok_or_else(|| {
                        format!(
                            "journal entry {journal_index} has no captured combat root identity"
                        )
                    })?;
                    Ok(json!({
                        "journal_index": journal_index,
                        "kind": "combat_resolution",
                        "root_identity": root_identity,
                        "location": record.before.location,
                        "title": record.before.title,
                        "resolution": record.kind,
                        "actions": record.trajectory.action_count,
                        "changes": record.result.changes,
                    }))
                }
                RunProgressStepV1::Stop(record) => Ok(json!({
                    "journal_index": journal_index,
                    "kind": "stop",
                    "stop_kind": record.kind,
                    "reason": record.reason,
                })),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "schema_name": "OracleAnalysisTimelineV2",
        "schema_version": 2,
        "workspace": workspace,
        "node": node,
        "node_identity_scope": "workspace_local_only",
        "line_identity": identity.line_identity,
        "current_combat_root": current_combat_root,
        "total_entries": entries.len(),
        "returned_entries": compact.len(),
        "entries": compact,
    }))
}

pub(super) fn combat_case(
    analysis: &OracleAnalysisWorkspaceV1,
    node: usize,
) -> Result<CombatCase, String> {
    let view = analysis.session.view_node(node)?;
    let (generation_work, wall_ms) = if view.encounter.as_ref().is_some_and(|it| it.is_boss) {
        (
            analysis.budget.boss_generation_work,
            analysis.budget.boss_ms,
        )
    } else if view.encounter.as_ref().is_some_and(|it| it.is_elite) {
        (
            analysis.budget.elite_generation_work,
            analysis.budget.elite_ms,
        )
    } else {
        (
            analysis.budget.hallway_generation_work,
            analysis.budget.hallway_ms,
        )
    };
    analysis.session.combat_case(
        node,
        analysis.seed,
        analysis.ascension,
        generation_work,
        wall_ms,
    )
}
