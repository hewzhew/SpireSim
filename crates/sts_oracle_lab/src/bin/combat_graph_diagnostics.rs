//! Exact traces and state snapshots materialized from selected graph paths.

use serde_json::Value;
use sts_combat_planner::{
    LocalTurnGraphStateSnapshot, LocalTurnGraphWitnessSession, TurnOptionAction,
};
use sts_oracle_runtime::sim::combat::CombatPosition;

use super::combat_replay_tools::{local_graph_state_snapshot_for_path, replay_combat_path};

pub(super) struct LocalGraphDiagnosticPaths<'a> {
    pub(super) deepest_survival: &'a [TurnOptionAction],
    pub(super) deepest_progress: &'a [TurnOptionAction],
    pub(super) witness: Option<&'a [TurnOptionAction]>,
}

pub(super) struct LocalGraphDiagnostics {
    pub(super) deepest_survival_trace: Option<Value>,
    pub(super) deepest_progress_trace: Option<Value>,
    pub(super) deepest_survival_node: Option<LocalTurnGraphStateSnapshot>,
    pub(super) deepest_progress_node: Option<LocalTurnGraphStateSnapshot>,
    pub(super) witness_trace: Option<Value>,
}

pub(super) fn materialize_local_graph_diagnostics(
    session: &LocalTurnGraphWitnessSession,
    root: &CombatPosition,
    paths: LocalGraphDiagnosticPaths<'_>,
    include_traces: bool,
    max_engine_steps_per_transition: usize,
) -> Result<LocalGraphDiagnostics, String> {
    let deepest_survival_trace = include_traces
        .then(|| {
            replay_combat_path(
                root.clone(),
                paths.deepest_survival,
                max_engine_steps_per_transition,
            )
        })
        .transpose()?;
    let deepest_progress_trace = include_traces
        .then(|| {
            replay_combat_path(
                root.clone(),
                paths.deepest_progress,
                max_engine_steps_per_transition,
            )
        })
        .transpose()?;
    let deepest_survival_node = local_graph_state_snapshot_for_path(
        session,
        root.clone(),
        paths.deepest_survival,
        max_engine_steps_per_transition,
    )?;
    let deepest_progress_node = local_graph_state_snapshot_for_path(
        session,
        root.clone(),
        paths.deepest_progress,
        max_engine_steps_per_transition,
    )?;
    let witness_trace = if include_traces {
        paths
            .witness
            .map(|actions| {
                replay_combat_path(root.clone(), actions, max_engine_steps_per_transition)
            })
            .transpose()?
    } else {
        None
    };

    Ok(LocalGraphDiagnostics {
        deepest_survival_trace,
        deepest_progress_trace,
        deepest_survival_node,
        deepest_progress_node,
        witness_trace,
    })
}
