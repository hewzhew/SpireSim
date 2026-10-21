use std::path::Path;

use serde_json::{json, Value};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_case::{
    save_combat_case, CombatCase, CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary,
    CombatCaseSource, CombatCaseWitnessBudgetV1,
};
use sts_oracle_runtime::eval::combat_case_context::capture_oracle_analysis_combat_case_production_context_v2;
use sts_oracle_runtime::eval::run_control::{
    exact_census_run_progress_journal_combat_roots_v1, exact_replay_run_progress_journal_prefix_v1,
    RunControlSessionCheckpointV1, RunProgressJournalV1, RunProgressStepV1,
};
use sts_oracle_runtime::runtime::branch::{
    load_oracle_analysis_workspace_v1, oracle_run_combat_budgets_v1,
    save_oracle_run_continuation_v1, OracleRunConfig, OracleRunContinuationV1,
};
use sts_oracle_runtime::sim::combat::{combat_terminal, CombatPosition};

use super::combat_evidence_manifest::{
    combat_evidence_manifest_path_for_actions, write_combat_evidence_manifest,
    CombatEvidenceManifestEntryV1, CombatEvidenceProducerV1,
};
use super::combat_replay_tools::{replay_combat_inputs, save_combat_inputs};

const EVIDENCE_REPLAY_MAX_ENGINE_STEPS_PER_TRANSITION: usize = 250;

pub(super) fn export_historical_combat(
    workspace: &Path,
    node: Option<usize>,
    journal_entry: usize,
    case_output: &Path,
    actions_output: &Path,
    continuation_output: Option<&Path>,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    let continuation = analysis.continuation(node)?;
    let resolution = continuation
        .journal
        .entries()
        .get(journal_entry)
        .and_then(RunProgressStepV1::as_combat_resolution)
        .cloned()
        .ok_or_else(|| format!("journal entry {journal_entry} is not a combat resolution"))?;
    let expected_final = continuation.session.clone().into_session()?;
    let source_census = exact_census_run_progress_journal_combat_roots_v1(
        continuation.seed,
        continuation.ascension,
        &continuation.journal,
        &expected_final,
    );
    let root_identity = source_census
        .combat_roots
        .iter()
        .find(|root| root.journal_entry == Some(journal_entry))
        .cloned()
        .ok_or_else(|| {
            format!("journal entry {journal_entry} has no captured combat root identity")
        })?;
    let historical = exact_replay_run_progress_journal_prefix_v1(
        continuation.seed,
        continuation.ascension,
        &continuation.journal,
        &expected_final,
        journal_entry,
    )?;
    let active = historical.active_combat.as_ref().ok_or_else(|| {
        format!("journal entry {journal_entry} does not begin at an active combat")
    })?;
    let position = CombatPosition::new(active.engine_state.clone(), active.combat_state.clone());
    let generation = continuation
        .journal
        .entries()
        .iter()
        .take(journal_entry)
        .filter_map(RunProgressStepV1::as_decision)
        .count();
    let mut case = CombatCase::new(
        CombatCaseSource {
            seed: continuation.seed,
            ascension: continuation.ascension,
            generation,
            branch_id: node,
            parent_id: None,
        },
        CombatCaseGap {
            boundary: format!(
                "Act {} Floor {} historical combat",
                historical.run_state.act_num, historical.run_state.floor_num
            ),
            reason: "verified_run_witness_extraction".to_string(),
            witness_budget: CombatCaseWitnessBudgetV1::NotRun,
        },
        CombatCaseRunSummary {
            act: historical.run_state.act_num,
            floor: historical.run_state.floor_num,
            hp: historical.run_state.current_hp,
            max_hp: historical.run_state.max_hp,
            gold: historical.run_state.gold,
            deck_size: historical.run_state.master_deck.len(),
            relic_count: historical.run_state.relics.len(),
            potion_slots: historical.run_state.potions.len(),
        },
        Vec::new(),
        None,
        Vec::new(),
        CombatCaseRngSummary::from_pool(&historical.run_state.rng_pool),
        position,
    );
    let owner_budgets = oracle_run_combat_budgets_v1(&OracleRunConfig {
        seed: analysis.seed,
        ascension: analysis.ascension,
        budget: analysis.budget,
    })
    .with_guidance_bundle(analysis.combat_guidance_bundle.clone());
    case.production_context = Some(capture_oracle_analysis_combat_case_production_context_v2(
        &case.core,
        &historical,
        &owner_budgets,
    )?);
    let actions = resolution
        .trajectory
        .actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let final_position = replay_combat_inputs(
        case.core.position.clone(),
        &actions,
        EVIDENCE_REPLAY_MAX_ENGINE_STEPS_PER_TRANSITION,
    )?;
    let root_exact_state_hash =
        combat_exact_state_hash_v2(&case.core.position.engine, &case.core.position.combat);
    let final_terminal = combat_terminal(&final_position.engine, &final_position.combat);
    let final_player_hp = final_position.combat.entities.player.current_hp;
    save_combat_case(case_output, &case)?;
    save_combat_inputs(actions_output, actions.iter().cloned())?;
    let manifest_output = combat_evidence_manifest_path_for_actions(actions_output);
    if let Some(output) = continuation_output {
        let prefix_journal = RunProgressJournalV1::from_committed_steps(
            continuation.journal.entries()[..journal_entry].to_vec(),
        )?;
        let prefix = OracleRunContinuationV1 {
            schema_name: continuation.schema_name,
            schema_version: continuation.schema_version,
            decision_prior: continuation.decision_prior,
            seed: continuation.seed,
            ascension: continuation.ascension,
            journal: prefix_journal,
            session: RunControlSessionCheckpointV1::from_session(&historical),
            explorer_frontier: None,
        };
        save_oracle_run_continuation_v1(output, &prefix)?;
    }
    write_combat_evidence_manifest(
        &manifest_output,
        CombatEvidenceProducerV1::HistoricalCombatWitnessExport,
        root_exact_state_hash,
        case_output.to_path_buf(),
        vec![CombatEvidenceManifestEntryV1::from_actions(
            format!("journal_entry_{journal_entry}"),
            vec![actions_output.to_path_buf()],
            &actions,
            final_terminal,
            Some(final_player_hp),
        )?],
    )?;
    Ok(json!({
        "schema_name": "HistoricalCombatWitnessExportV3",
        "schema_version": 3,
        "workspace": workspace,
        "node_id": node,
        "node_identity_scope": "workspace_local_only",
        "journal_entry": journal_entry,
        "source_line_identity": source_census.line_identity,
        "source_replay_error": source_census.replay_error,
        "combat_root_identity": root_identity,
        "source": resolution.trajectory.source.label(),
        "case_output": case_output,
        "actions_output": actions_output,
        "manifest_output": manifest_output,
        "continuation_output": continuation_output,
        "action_count": resolution.trajectory.actions.len(),
        "combat": case.core.combat,
    }))
}
