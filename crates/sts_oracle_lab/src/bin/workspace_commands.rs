use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};
use sts_oracle_runtime::runtime::branch::{
    load_oracle_analysis_workspace_v1, oracle_live_combat_diagnostic_v1,
    save_oracle_analysis_workspace_v1, OracleAnalysisAdvanceRequestV1,
    OracleAnalysisCombatProbeRequestV1, OracleAnalysisWorkspaceV1,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::workspace_view;

pub(super) fn view(workspace: &Path, node: Option<usize>) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    encode(workspace_view::selected(&analysis, node)?)
}

pub(super) fn status(workspace: &Path, node: Option<usize>, limit: usize) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let view = workspace_view::selected(&analysis, node)?;
    let current_owner_order = workspace_view::current_owner_order(&analysis, view.node_id)?;
    Ok(workspace_view::compact_node(
        &view,
        limit,
        &current_owner_order,
    ))
}

pub(super) fn timeline(
    workspace: &Path,
    node: Option<usize>,
    tail: usize,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    if tail == 0 || tail > 500 {
        return Err("timeline tail must be in 1..=500".to_string());
    }
    workspace_view::compact_timeline(&analysis, workspace, node, tail)
}

pub(super) fn export_combat_case(
    workspace: &Path,
    node: Option<usize>,
    output: &Path,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    let case = workspace_view::combat_case(&analysis, node)?;
    sts_oracle_runtime::eval::combat_case::save_combat_case(output, &case)?;
    Ok(json!({
        "node": node,
        "output": output,
        "combat": case.core.combat,
    }))
}

pub(super) fn combat(
    workspace: &Path,
    node: Option<usize>,
    max_engine_steps_per_transition: usize,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    oracle_live_combat_diagnostic_v1(&analysis, node, max_engine_steps_per_transition)
}

pub(super) fn tree(workspace: &Path) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    encode(analysis.session.tree())
}

pub(super) fn try_choice(workspace: &Path, choice_ref: &str) -> Result<Value, String> {
    mutate(workspace, |analysis| analysis.try_choice(choice_ref))
}

pub(super) fn focus(workspace: &Path, node: usize) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        analysis.session.focus_node(node)?;
        analysis.view()
    })
}

pub(super) fn follow(workspace: &Path, edge: u64) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        analysis.session.follow_edge(edge)?;
        analysis.view()
    })
}

pub(super) fn back(workspace: &Path) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        analysis.session.back()?;
        analysis.view()
    })
}

pub(super) fn promote(workspace: &Path) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        analysis.session.promote_cursor();
        analysis.view()
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn advance(
    workspace: &Path,
    max_quanta: usize,
    quantum_generation_work: usize,
    quantum_ms: u64,
    wall_ms: Option<u64>,
    improve_incumbent: bool,
    detailed: bool,
) -> Result<Value, String> {
    let mut analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let (report, view) = analysis.advance(OracleAnalysisAdvanceRequestV1 {
        max_quanta,
        quantum_generation_work,
        quantum_ms: Some(quantum_ms),
        wall_ms,
        improve_incumbent,
    })?;
    save_oracle_analysis_workspace_v1(workspace, &analysis)?;
    if detailed {
        return Ok(json!({ "report": report, "view": view }));
    }
    let combat = report.combat.as_ref().map(|combat| {
        json!({
            "root_exact_state_hash": combat.root_exact_state_hash,
            "stage_trace": combat.stage_trace,
            "generation_work": combat.generation_work,
            "current_search_generation_work": combat.current_search_generation_work,
            "exact_states": combat.exact_states,
            "completed_turn_options": combat.completed_turn_options,
            "retained_state_work": combat.retained_state_work,
            "max_player_turn": combat.max_player_turn,
            "incumbent_discovery_source": combat.incumbent_discovery_source,
            "incumbent_final_hp": combat.incumbent_final_hp,
            "incumbent_hp_loss": combat.incumbent_hp_loss,
            "incumbent_action_count": combat.incumbent_action_count,
            "incumbent_satisfies_satisfaction": combat.incumbent_satisfies_satisfaction,
            "incumbent_ends_quality_refinement": combat.incumbent_ends_quality_refinement,
            "last_status": combat.last_status,
        })
    });
    Ok(json!({
        "schema_name": "OracleAnalysisAdvanceSummaryV1",
        "schema_version": 1,
        "source_node_id": report.source_node_id,
        "status": report.status,
        "quanta_served": report.quanta_served,
        "elapsed_ms": report.elapsed_ms,
        "combat": combat,
        "result": {
            "node": view.node_id,
            "boundary": view.boundary,
            "act": view.act,
            "floor": view.floor,
            "hp": view.current_hp,
            "max_hp": view.max_hp,
            "gold": view.gold,
            "choice_count": view.choices.len(),
            "child_count": view.children.len(),
        },
    }))
}

pub(super) fn probe_combat(
    workspace: &Path,
    generation_work: usize,
    quantum_generation_work: usize,
    wall_ms: u64,
    detailed: bool,
) -> Result<Value, String> {
    let mut analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let (report, view) = analysis.probe_combat(OracleAnalysisCombatProbeRequestV1 {
        generation_work,
        quantum_generation_work,
        wall_ms,
    })?;
    save_oracle_analysis_workspace_v1(workspace, &analysis)?;
    if detailed {
        return Ok(json!({ "report": report, "view": view }));
    }
    Ok(json!({
        "schema_name": "OracleAnalysisCombatProbeSummaryV1",
        "schema_version": 1,
        "source_node_id": report.source_node_id,
        "stop": report.stop,
        "generation_work_requested": report.generation_work_requested,
        "generation_work_consumed": report.generation_work_consumed,
        "quanta_served": report.quanta_served,
        "elapsed_ms": report.elapsed_ms,
        "combat": {
            "root_exact_state_hash": report.combat.root_exact_state_hash,
            "stage_trace": report.combat.stage_trace,
            "search_stage": report.combat.search_stage,
            "max_potions_used": report.combat.max_potions_used,
            "allowed_potion_slots": report.combat.allowed_potion_slots,
            "generation_work": report.combat.generation_work,
            "current_search_generation_work": report.combat.current_search_generation_work,
            "local_generation_work": report.combat.local_generation_work,
            "discrepancy_generation_work": report.combat.discrepancy_generation_work,
            "incumbent_final_hp": report.combat.incumbent_final_hp,
            "incumbent_hp_loss": report.combat.incumbent_hp_loss,
            "incumbent_action_count": report.combat.incumbent_action_count,
            "incumbent_satisfies_satisfaction": report.combat.incumbent_satisfies_satisfaction,
            "last_status": report.combat.last_status,
        },
        "result": {
            "node": view.node_id,
            "boundary": view.boundary,
            "act": view.act,
            "floor": view.floor,
            "hp": view.current_hp,
            "max_hp": view.max_hp,
            "gold": view.gold,
            "child_count": view.children.len(),
        },
    }))
}

pub(super) fn accept_combat(workspace: &Path) -> Result<Value, String> {
    mutate(
        workspace,
        OracleAnalysisWorkspaceV1::accept_combat_incumbent,
    )
}

pub(super) fn accept_combat_actions(
    workspace: &Path,
    action_paths: &[PathBuf],
) -> Result<Value, String> {
    let action_lists = action_paths
        .iter()
        .map(|path| {
            serde_json::from_slice::<Vec<ClientInput>>(
                &std::fs::read(path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| {
                format!(
                    "invalid combat witness action list '{}': {error}",
                    path.display()
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let actions = action_lists.into_iter().flatten().collect::<Vec<_>>();
    mutate(workspace, |analysis| {
        analysis.accept_combat_actions(&actions)
    })
}

pub(super) fn restart_combat(workspace: &Path) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        analysis.session.restart_cursor_combat_search()?;
        analysis.view()
    })
}

pub(super) fn history(
    workspace: &Path,
    node: Option<usize>,
    journal: bool,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    if journal {
        encode(analysis.session.journal_entries(node)?)
    } else {
        encode(analysis.session.replay(node)?)
    }
}

pub(super) fn mutate<T: Serialize>(
    workspace: &Path,
    operation: impl FnOnce(&mut OracleAnalysisWorkspaceV1) -> Result<T, String>,
) -> Result<Value, String> {
    let mut analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let output = operation(&mut analysis)?;
    save_oracle_analysis_workspace_v1(workspace, &analysis)?;
    encode(output)
}

pub(super) fn encode(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("failed to encode workspace result: {error}"))
}
