use std::path::Path;

use serde_json::{json, Value};
use sts_oracle_runtime::eval::combat_case::save_combat_case;
use sts_oracle_runtime::eval::run_control::{
    exact_census_run_progress_journal_combat_roots_v1,
    exact_replay_run_progress_journal_identity_v1, exact_replay_run_progress_journal_prefix_v1,
    exact_replay_run_progress_journal_v1, run_progress_journal_fingerprint_v1,
    splice_exact_combat_resolution_v1, RunControlSessionCheckpointV1, RunProgressJournalV1,
    RunProgressStepV1,
};
use sts_oracle_runtime::runtime::branch::{
    load_oracle_analysis_workspace_v1, recover_oracle_analysis_combat_case_v2,
    save_oracle_run_continuation_v1, OracleRunContinuationV1,
};

pub(super) fn export_continuation(
    workspace: &Path,
    node: Option<usize>,
    output: &Path,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    let continuation = analysis.continuation(node)?;
    let journal_entries = continuation.journal.entries().len();
    let expected_final = continuation.session.clone().into_session()?;
    let census = exact_census_run_progress_journal_combat_roots_v1(
        continuation.seed,
        continuation.ascension,
        &continuation.journal,
        &expected_final,
    );
    save_oracle_run_continuation_v1(output, &continuation)?;
    Ok(json!({
        "schema_name": "OracleAnalysisContinuationExportV2",
        "schema_version": 2,
        "workspace": workspace,
        "node_id": node,
        "node_identity_scope": "workspace_local_only",
        "line_identity": census.line_identity,
        "replay_error": census.replay_error,
        "output": output,
        "journal_entries": journal_entries,
    }))
}

pub(super) fn export_prefix(
    workspace: &Path,
    node: Option<usize>,
    journal_entry: usize,
    output: &Path,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    let continuation = analysis.continuation(node)?;
    let expected_final = continuation.session.clone().into_session()?;
    let historical = exact_replay_run_progress_journal_prefix_v1(
        continuation.seed,
        continuation.ascension,
        &continuation.journal,
        &expected_final,
        journal_entry,
    )?;
    let prefix = RunProgressJournalV1::from_committed_steps(
        continuation.journal.entries()[..journal_entry].to_vec(),
    )?;
    let prefix_identity = exact_replay_run_progress_journal_identity_v1(
        continuation.seed,
        continuation.ascension,
        &prefix,
        &historical,
    )?;
    let mut checkpoint = RunControlSessionCheckpointV1::from_session(&historical);
    checkpoint.clear_combat_diagnostics_for_external_checkpoint();
    let output_continuation = OracleRunContinuationV1 {
        schema_name: continuation.schema_name,
        schema_version: continuation.schema_version,
        decision_prior: continuation.decision_prior,
        seed: continuation.seed,
        ascension: continuation.ascension,
        journal: prefix,
        session: checkpoint,
        explorer_frontier: None,
    };
    save_oracle_run_continuation_v1(output, &output_continuation)?;

    Ok(json!({
        "schema_name": "ExactOracleRunWitnessPrefixExportV2",
        "schema_version": 2,
        "workspace": workspace,
        "node_id": node,
        "node_identity_scope": "workspace_local_only",
        "journal_entry": journal_entry,
        "source_journal_fingerprint": run_progress_journal_fingerprint_v1(&continuation.journal),
        "line_identity": prefix_identity.line_identity,
        "output": output,
        "journal_entries": output_continuation.journal.len(),
        "act": historical.run_state.act_num,
        "floor": historical.run_state.floor_num,
        "current_hp": historical.run_state.current_hp,
        "max_hp": historical.run_state.max_hp,
    }))
}

pub(super) fn recover_combat_case(
    workspace: &Path,
    branch: usize,
    output: &Path,
) -> Result<Value, String> {
    let case = recover_oracle_analysis_combat_case_v2(workspace, branch)?;
    save_combat_case(output, &case)?;
    Ok(json!({
        "schema_name": "OracleRecoveredCombatCaseV1",
        "workspace": workspace,
        "branch_id": branch,
        "output": output,
        "source": case.core.source,
        "run": case.core.run,
        "combat": case.core.combat,
        "path_steps": case.path.len(),
    }))
}

pub(super) fn verify(workspace: &Path, node: Option<usize>) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    let continuation = analysis.continuation(node)?;
    let expected_final = continuation.session.into_session()?;
    let report = exact_replay_run_progress_journal_v1(
        analysis.seed,
        analysis.ascension,
        &continuation.journal,
        &expected_final,
    )?;
    Ok(json!({
        "schema_name": "ExactOracleRunWitnessReplayV1",
        "schema_version": 1,
        "workspace": workspace,
        "node_id": node,
        "report": report,
    }))
}

pub(super) fn splice_combat(
    workspace: &Path,
    node: usize,
    journal_entry: usize,
    replacement_workspace: &Path,
    replacement_node: usize,
    output: &Path,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let continuation = analysis.continuation(node)?;
    let replacement_analysis = load_oracle_analysis_workspace_v1(replacement_workspace)?;
    let replacement_continuation = replacement_analysis.continuation(replacement_node)?;
    if continuation.seed != replacement_continuation.seed
        || continuation.ascension != replacement_continuation.ascension
    {
        return Err("combat splice requires matching seed and ascension".to_string());
    }
    let replacement = replacement_continuation
        .journal
        .entries()
        .iter()
        .rev()
        .find_map(RunProgressStepV1::as_combat_resolution)
        .ok_or_else(|| "replacement witness contains no committed combat resolution".to_string())?;
    let original_source = continuation
        .journal
        .entries()
        .get(journal_entry)
        .and_then(RunProgressStepV1::as_combat_resolution)
        .map(|record| record.trajectory.source.label())
        .ok_or_else(|| format!("journal entry {journal_entry} is not a combat resolution"))?;
    let expected_final = continuation.session.clone().into_session()?;
    let (journal, replay) = splice_exact_combat_resolution_v1(
        continuation.seed,
        continuation.ascension,
        &continuation.journal,
        &expected_final,
        journal_entry,
        replacement,
    )?;
    let replacement_source = replacement.trajectory.source.label();
    let output_continuation = OracleRunContinuationV1 {
        schema_name: continuation.schema_name,
        schema_version: continuation.schema_version,
        decision_prior: continuation.decision_prior,
        seed: continuation.seed,
        ascension: continuation.ascension,
        journal,
        session: continuation.session,
        explorer_frontier: None,
    };
    save_oracle_run_continuation_v1(output, &output_continuation)?;
    Ok(json!({
        "schema_name": "ExactOracleCombatWitnessSpliceV1",
        "schema_version": 1,
        "workspace": workspace,
        "node_id": node,
        "journal_entry": journal_entry,
        "original_source": original_source,
        "replacement_workspace": replacement_workspace,
        "replacement_node_id": replacement_node,
        "replacement_source": replacement_source,
        "output": output,
        "replay": replay,
    }))
}
