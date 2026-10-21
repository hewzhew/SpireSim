use std::{path::Path, time::Instant};

use clap::ValueEnum;
use serde::Serialize;
use serde_json::{json, Value};
use sts_oracle_runtime::eval::combat_lab_v1::atomic_write_json;
use sts_oracle_runtime::eval::run_control::OracleRunBoundaryV1;
use sts_oracle_runtime::runtime::branch::{
    load_oracle_analysis_workspace_v1, save_oracle_analysis_workspace_v1,
    OracleAnalysisAdvanceRequestV1, OracleAnalysisAdvanceStatusV1, OracleAnalysisNodeViewV1,
};

use super::workspace_view;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum OracleDriveBoundaryArg {
    MapDecision,
    Combat,
    Reward,
    Event,
    Shop,
    Campfire,
    RunChoice,
    Treasure,
    BossRelic,
    TerminalVictory,
    TerminalDefeat,
}

impl From<OracleDriveBoundaryArg> for OracleRunBoundaryV1 {
    fn from(value: OracleDriveBoundaryArg) -> Self {
        match value {
            OracleDriveBoundaryArg::MapDecision => Self::MapDecision,
            OracleDriveBoundaryArg::Combat => Self::Combat,
            OracleDriveBoundaryArg::Reward => Self::Reward,
            OracleDriveBoundaryArg::Event => Self::Event,
            OracleDriveBoundaryArg::Shop => Self::Shop,
            OracleDriveBoundaryArg::Campfire => Self::Campfire,
            OracleDriveBoundaryArg::RunChoice => Self::RunChoice,
            OracleDriveBoundaryArg::Treasure => Self::Treasure,
            OracleDriveBoundaryArg::BossRelic => Self::BossRelic,
            OracleDriveBoundaryArg::TerminalVictory => Self::TerminalVictory,
            OracleDriveBoundaryArg::TerminalDefeat => Self::TerminalDefeat,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum OracleAnalysisDriveStopV1 {
    StepLimit,
    WallLimit,
    TargetBoundary,
    NoOwnerChoice,
    CombatUnresolved,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn drive(
    workspace: &Path,
    output: Option<&Path>,
    max_steps: u16,
    max_quanta: usize,
    quantum_generation_work: usize,
    quantum_ms: u64,
    wall_ms: u64,
    stop_at: Option<OracleRunBoundaryV1>,
) -> Result<Value, String> {
    if max_steps == 0
        || max_quanta == 0
        || quantum_generation_work == 0
        || quantum_ms == 0
        || wall_ms == 0
    {
        return Err("oracle drive requires positive step, quantum, and wall budgets".to_string());
    }
    if let Some(output) = output {
        if output.exists() {
            return Err(format!(
                "oracle drive output already exists: '{}'",
                output.display()
            ));
        }
    }
    let mut analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let initial = compact_drive_initial(&analysis.view()?);
    let started = Instant::now();
    let mut events = Vec::new();
    let mut owner_decision_count = 0usize;
    let mut combat_advance_count = 0usize;
    let mut last_combat = None;
    let mut stop = OracleAnalysisDriveStopV1::StepLimit;

    for step_index in 0..max_steps {
        let elapsed_ms = elapsed_millis(started);
        if elapsed_ms >= wall_ms {
            stop = OracleAnalysisDriveStopV1::WallLimit;
            break;
        }
        let current = analysis.view()?;
        if stop_at == Some(current.boundary) {
            stop = OracleAnalysisDriveStopV1::TargetBoundary;
            break;
        }
        let current_owner_order = workspace_view::current_owner_order(&analysis, current.node_id)?;
        if let Some(choice_ref) = current_owner_order.first() {
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
            let source_node_id = current.node_id;
            let source = compact_drive_source(&current);
            let candidate_id = choice.candidate_id.clone();
            let choice_ref = choice.choice_ref.clone();
            let decision_kind = choice.kind.clone();
            let action = choice.action.clone();
            let materialized_owner_rank = choice.owner_rank;
            let result = analysis.try_choice(&choice_ref)?;
            let state_delta = compact_drive_state_delta(&current, &result);
            save_oracle_analysis_workspace_v1(workspace, &analysis)?;
            events.push(json!({
                "kind": "owner_decision",
                "step_index": step_index,
                "source_node_id": source_node_id,
                "source": source,
                "candidate_id": candidate_id,
                "choice_ref": choice_ref,
                "decision_kind": decision_kind,
                "action": action,
                "materialized_owner_rank": materialized_owner_rank,
                "state_delta": state_delta,
                "result": compact_drive_boundary(&result),
            }));
            owner_decision_count += 1;
            continue;
        }

        if current.boundary != OracleRunBoundaryV1::Combat {
            stop = OracleAnalysisDriveStopV1::NoOwnerChoice;
            break;
        }
        let remaining_wall_ms = wall_ms.saturating_sub(elapsed_millis(started));
        if remaining_wall_ms == 0 {
            stop = OracleAnalysisDriveStopV1::WallLimit;
            break;
        }
        let source = compact_drive_source(&current);
        let (report, result) = analysis.advance(OracleAnalysisAdvanceRequestV1 {
            max_quanta,
            quantum_generation_work,
            quantum_ms: Some(quantum_ms),
            wall_ms: Some(remaining_wall_ms),
            improve_incumbent: false,
        })?;
        save_oracle_analysis_workspace_v1(workspace, &analysis)?;
        let boundary_reached = matches!(
            report.status,
            OracleAnalysisAdvanceStatusV1::BoundaryReached { .. }
        );
        last_combat = report.combat.as_ref().map(|combat| {
            json!({
                "root_exact_state_hash": combat.root_exact_state_hash,
                "stage_count": combat.stage_trace.len(),
                "generation_work": combat.generation_work,
                "exact_states": combat.exact_states,
                "max_player_turn": combat.max_player_turn,
                "incumbent_final_hp": combat.incumbent_final_hp,
                "incumbent_hp_loss": combat.incumbent_hp_loss,
                "incumbent_action_count": combat.incumbent_action_count,
                "incumbent_potions_used": combat.incumbent_potions_used,
                "incumbent_potion_slots": combat.incumbent_potion_slots,
                "incumbent_satisfies_satisfaction": combat.incumbent_satisfies_satisfaction,
                "incumbent_ends_quality_refinement": combat.incumbent_ends_quality_refinement,
            })
        });
        let combat = report.combat.as_ref().map(|combat| {
            json!({
                "root_exact_state_hash": combat.root_exact_state_hash,
                "stage_trace": combat.stage_trace,
                "generation_work": combat.generation_work,
                "exact_states": combat.exact_states,
                "max_player_turn": combat.max_player_turn,
                "incumbent_final_hp": combat.incumbent_final_hp,
                "incumbent_hp_loss": combat.incumbent_hp_loss,
                "incumbent_action_count": combat.incumbent_action_count,
                "incumbent_potions_used": combat.incumbent_potions_used,
                "incumbent_potion_slots": combat.incumbent_potion_slots,
                "incumbent_satisfies_satisfaction": combat.incumbent_satisfies_satisfaction,
                "incumbent_ends_quality_refinement": combat.incumbent_ends_quality_refinement,
            })
        });
        events.push(json!({
            "kind": "combat_advance",
            "step_index": step_index,
            "source_node_id": report.source_node_id,
            "source": source,
            "status": report.status,
            "quanta_served": report.quanta_served,
            "elapsed_ms": report.elapsed_ms,
            "combat": combat,
            "result": compact_drive_boundary(&result),
        }));
        combat_advance_count += 1;
        if !boundary_reached {
            stop = OracleAnalysisDriveStopV1::CombatUnresolved;
            break;
        }
    }

    let final_view = analysis.view()?;
    let current_owner_order = workspace_view::current_owner_order(&analysis, final_view.node_id)?;
    let completed_steps = events.len();
    let elapsed_ms = elapsed_millis(started);
    let final_boundary = compact_drive_boundary(&final_view);
    let report = json!({
        "schema_name": "OracleAnalysisDriveV1",
        "schema_version": 1,
        "workspace": workspace,
        "initial": initial,
        "requested_max_steps": max_steps,
        "requested_stop_boundary": stop_at,
        "completed_steps": completed_steps,
        "elapsed_ms": elapsed_ms,
        "stop": stop,
        "events": events,
        "status": workspace_view::compact_node(&final_view, 8, &current_owner_order),
    });
    if let Some(output) = output {
        atomic_write_json(output, &report)?;
    }

    Ok(json!({
        "schema_name": "OracleAnalysisDriveReceipt",
        "schema_version": 1,
        "workspace": workspace,
        "output": output,
        "completed_steps": completed_steps,
        "owner_decision_count": owner_decision_count,
        "combat_advance_count": combat_advance_count,
        "elapsed_ms": elapsed_ms,
        "stop": stop,
        "final": final_boundary,
        "last_combat": last_combat,
    }))
}

fn compact_drive_boundary(view: &OracleAnalysisNodeViewV1) -> Value {
    json!({
        "node": view.node_id,
        "state_fingerprint": view.state_fingerprint,
        "boundary": view.boundary,
        "act": view.act,
        "floor": view.floor,
        "hp": view.current_hp,
        "max_hp": view.max_hp,
        "gold": view.gold,
    })
}

fn compact_drive_source(view: &OracleAnalysisNodeViewV1) -> Value {
    let mut source = compact_drive_boundary(view);
    if let Some(fields) = source.as_object_mut() {
        fields.insert("event".to_string(), json!(view.event));
        fields.insert("encounter".to_string(), json!(view.encounter));
    }
    source
}

fn compact_drive_initial(view: &OracleAnalysisNodeViewV1) -> Value {
    let mut initial = compact_drive_source(view);
    if let Some(fields) = initial.as_object_mut() {
        fields.insert("deck".to_string(), json!(view.deck));
        fields.insert("relics".to_string(), json!(view.relics));
        fields.insert("potions".to_string(), json!(view.potions));
    }
    initial
}

fn compact_drive_state_delta(
    source: &OracleAnalysisNodeViewV1,
    result: &OracleAnalysisNodeViewV1,
) -> Value {
    let deck_added = result
        .deck
        .iter()
        .filter(|card| {
            !source
                .deck
                .iter()
                .any(|before| before.lineage == card.lineage)
        })
        .collect::<Vec<_>>();
    let deck_removed = source
        .deck
        .iter()
        .filter(|card| {
            !result
                .deck
                .iter()
                .any(|after| after.lineage == card.lineage)
        })
        .collect::<Vec<_>>();
    let deck_changed = result
        .deck
        .iter()
        .filter_map(|after| {
            source
                .deck
                .iter()
                .find(|before| before.lineage == after.lineage && *before != after)
                .map(|before| json!({ "before": before, "after": after }))
        })
        .collect::<Vec<_>>();
    let relics_added = result
        .relics
        .iter()
        .filter(|relic| !source.relics.iter().any(|before| before.id == relic.id))
        .collect::<Vec<_>>();
    let relics_removed = source
        .relics
        .iter()
        .filter(|relic| !result.relics.iter().any(|after| after.id == relic.id))
        .collect::<Vec<_>>();
    let potion_slots = (source.potions != result.potions).then(|| {
        json!({
            "before": source.potions,
            "after": result.potions,
        })
    });
    json!({
        "current_hp": result.current_hp - source.current_hp,
        "max_hp": result.max_hp - source.max_hp,
        "gold": result.gold - source.gold,
        "deck_added": deck_added,
        "deck_removed": deck_removed,
        "deck_changed": deck_changed,
        "relics_added": relics_added,
        "relics_removed": relics_removed,
        "potion_slots": potion_slots,
    })
}

fn elapsed_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}
