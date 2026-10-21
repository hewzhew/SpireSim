use std::path::Path;

use serde_json::{json, Value};
use sts_oracle_runtime::runtime::branch::{
    load_oracle_analysis_workspace_v1, save_oracle_analysis_workspace_v1,
};

use super::workspace_view;

pub(super) fn choose(
    workspace: &Path,
    owner_rank: u64,
    expected_node: Option<usize>,
) -> Result<Value, String> {
    let mut analysis = load_oracle_analysis_workspace_v1(workspace)?;
    if let Some(expected) = expected_node {
        let actual = analysis.session.cursor_node_id();
        if expected != actual {
            return Err(format!(
                "oracle choose expected cursor node {expected}, but current cursor is {actual}"
            ));
        }
    }
    let current = analysis.view()?;
    let current_owner_order = workspace_view::current_owner_order(&analysis, current.node_id)?;
    let owner_rank = usize::try_from(owner_rank)
        .map_err(|_| "oracle choose owner rank exceeds platform usize".to_string())?;
    let choice_ref = current_owner_order.get(owner_rank).ok_or_else(|| {
        format!(
            "oracle node {} current owner has no choice at rank {owner_rank}",
            current.node_id
        )
    })?;
    let matches = current
        .choices
        .iter()
        .filter(|choice| &choice.choice_ref == choice_ref)
        .collect::<Vec<_>>();
    let [choice] = matches.as_slice() else {
        return Err(format!(
            "oracle node {} has {} materialized choices for current-owner choice '{}'; expected exactly one",
            current.node_id,
            matches.len(),
            choice_ref,
        ));
    };
    let view = analysis.try_choice(&choice.choice_ref.clone())?;
    save_oracle_analysis_workspace_v1(workspace, &analysis)?;
    let current_owner_order = workspace_view::current_owner_order(&analysis, view.node_id)?;
    Ok(workspace_view::compact_node(&view, 8, &current_owner_order))
}

pub(super) fn owner(workspace: &Path, steps: u8) -> Result<Value, String> {
    let mut analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let mut applied = Vec::new();
    let mut stopped = "step_limit";
    for _ in 0..steps {
        let current = analysis.view()?;
        let current_owner_order = workspace_view::current_owner_order(&analysis, current.node_id)?;
        let Some(choice_ref) = current_owner_order.first() else {
            stopped = "no_owner_choice";
            break;
        };
        let choices = current
            .choices
            .iter()
            .filter(|choice| &choice.choice_ref == choice_ref)
            .collect::<Vec<_>>();
        let [choice] = choices.as_slice() else {
            return Err(format!(
                "oracle node {} has {} materialized choices for current-owner choice '{}'; expected exactly one",
                current.node_id,
                choices.len(),
                choice_ref,
            ));
        };
        let candidate_id = choice.candidate_id.clone();
        let label = choice.label.clone();
        let choice_ref = choice.choice_ref.clone();
        applied.push(json!({
            "node": current.node_id,
            "candidate_id": candidate_id,
            "label": label,
            "materialized_owner_rank": choice.owner_rank,
        }));
        analysis.try_choice(&choice_ref)?;
    }
    if !applied.is_empty() {
        save_oracle_analysis_workspace_v1(workspace, &analysis)?;
    }
    let final_view = analysis.view()?;
    let current_owner_order = workspace_view::current_owner_order(&analysis, final_view.node_id)?;
    Ok(json!({
        "requested_steps": steps,
        "applied_count": applied.len(),
        "applied": applied,
        "stopped": stopped,
        "status": workspace_view::compact_node(&final_view, 8, &current_owner_order),
    }))
}
