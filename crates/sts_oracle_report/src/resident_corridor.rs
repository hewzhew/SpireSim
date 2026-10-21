use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const RESIDENT_CORRIDOR_AUDIT_SCHEMA_V1: &str = "sts-resident-corridor-audit-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentCorridorAuditV1 {
    pub schema: String,
    pub root_exact_state_hash: String,
    pub from_turn_index: usize,
    pub configured_generation_work: usize,
    pub configured_wall_ms: u64,
    pub configured_satisfaction: String,
    /// This diagnostic deliberately enables the bounded public-plan proposal
    /// seed. Production resident configurations may differ, so persist the
    /// distinction instead of inferring it from non-zero attempt counters.
    #[serde(default)]
    pub public_plan_turn_proposal_enabled: bool,
    pub elapsed_ms: u64,
    pub search_status: String,
    pub witness_found: bool,
    /// Absent in legacy V1 reports written before witness quality was
    /// persisted. `witness_found` remains for backward-readable presence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub witness: Option<ResidentCorridorWitnessAuditV1>,
    pub counters: ResidentCorridorSearchCountersV1,
    pub turns: Vec<ResidentCorridorTurnAuditV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentCorridorWitnessAuditV1 {
    pub starting_player_hp: i32,
    pub final_player_hp: i32,
    pub max_player_hp: i32,
    pub action_count: usize,
    pub potion_expenditures: u32,
    pub final_exact_state_hash: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentCorridorSearchCountersV1 {
    pub selections: usize,
    pub generation_work: usize,
    pub engine_steps: usize,
    pub exact_nodes: usize,
    pub exact_edges: usize,
    pub completed_turn_options: usize,
    pub terminal_win_options: usize,
    pub public_plan_proposal_attempts: usize,
    pub public_plan_proposal_retained_options: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentCorridorTurnAuditV1 {
    pub teacher_turn_index: usize,
    pub from_turn: u32,
    pub teacher_action_count: usize,
    pub source_exact_state_hash: String,
    pub successor_exact_state_hash: String,
    pub source: Option<ResidentCorridorStateAuditV1>,
    pub edge: Option<ResidentCorridorEdgeAuditV1>,
    pub successor: Option<ResidentCorridorStateAuditV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentCorridorStateAuditV1 {
    pub relative_turn_depth: usize,
    pub visits: usize,
    pub first_service_selection: Option<usize>,
    pub first_guide_service_selection: Option<usize>,
    pub generation_work: usize,
    pub retained_generator_work_items: usize,
    pub generator_anchor_work_pops: usize,
    pub generator_guided_work_pops: usize,
    pub generated_options: usize,
    pub children: usize,
    pub exhausted: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentCorridorEdgeAuditV1 {
    pub negative_log_policy: f64,
    pub plan_prefix_proposed: bool,
    pub visits: usize,
    pub anchor_visits: usize,
    pub backed_visits: usize,
    pub successor_path_cost: f64,
    pub successor_anchor_ordinal_rank: Option<usize>,
    pub successor_anchor_candidate_count: usize,
    pub successor_anchor_service_cost: Option<f64>,
    pub best_anchor_service_cost: Option<f64>,
    pub guide_service: Vec<ResidentCorridorGuideServiceAuditV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentCorridorGuideServiceAuditV1 {
    pub lane: u32,
    pub edge_visits: usize,
    pub sibling_ordinal_rank: usize,
    pub sibling_candidate_count: usize,
    pub global_ordinal_rank: Option<usize>,
    pub global_candidate_count: usize,
}

pub fn load_resident_corridor_audit_v1(path: &Path) -> Result<ResidentCorridorAuditV1, String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "failed to read resident corridor audit {}: {error}",
            path.display()
        )
    })?;
    parse_resident_corridor_audit_v1(&bytes)
}

pub fn parse_resident_corridor_audit_v1(bytes: &[u8]) -> Result<ResidentCorridorAuditV1, String> {
    let report = serde_json::from_slice::<ResidentCorridorAuditV1>(bytes)
        .map_err(|error| format!("invalid resident corridor audit JSON: {error}"))?;
    validate_resident_corridor_audit_v1(&report)?;
    Ok(report)
}

pub fn validate_resident_corridor_audit_v1(report: &ResidentCorridorAuditV1) -> Result<(), String> {
    if report.schema != RESIDENT_CORRIDOR_AUDIT_SCHEMA_V1 {
        return Err(format!(
            "unsupported resident corridor schema {:?}; expected {:?}",
            report.schema, RESIDENT_CORRIDOR_AUDIT_SCHEMA_V1
        ));
    }
    if report.root_exact_state_hash.is_empty() {
        return Err("resident corridor root exact-state hash is empty".to_owned());
    }
    if report.configured_generation_work == 0 || report.configured_wall_ms == 0 {
        return Err("resident corridor audit requires positive search limits".to_owned());
    }
    if !matches!(
        report.configured_satisfaction.as_str(),
        "first_witness" | "zero_loss_or_budget" | "budget_or_exhaustion"
    ) {
        return Err(format!(
            "unsupported resident corridor satisfaction {:?}",
            report.configured_satisfaction
        ));
    }
    if let Some(witness) = &report.witness {
        if !report.witness_found {
            return Err(
                "resident corridor witness details are present while witness_found is false"
                    .to_owned(),
            );
        }
        if witness.final_exact_state_hash.is_empty() {
            return Err("resident corridor witness exact-state hash is empty".to_owned());
        }
        if witness.max_player_hp <= 0
            || witness.starting_player_hp > witness.max_player_hp
            || witness.final_player_hp > witness.max_player_hp
        {
            return Err("resident corridor witness contains invalid player HP".to_owned());
        }
    }
    for (offset, turn) in report.turns.iter().enumerate() {
        let expected = report.from_turn_index.saturating_add(offset);
        if turn.teacher_turn_index != expected {
            return Err(format!(
                "resident corridor row {offset} names teacher turn {}, expected {expected}",
                turn.teacher_turn_index
            ));
        }
        if turn.source_exact_state_hash.is_empty() || turn.successor_exact_state_hash.is_empty() {
            return Err(format!(
                "resident corridor teacher turn {} contains an empty exact-state hash",
                turn.teacher_turn_index
            ));
        }
        if turn.edge.is_some() && (turn.source.is_none() || turn.successor.is_none()) {
            return Err(format!(
                "resident corridor teacher turn {} stores an edge without both endpoint states",
                turn.teacher_turn_index
            ));
        }
        if offset > 0
            && report.turns[offset - 1].successor_exact_state_hash != turn.source_exact_state_hash
        {
            return Err(format!(
                "resident corridor teacher turn {} is not contiguous with its predecessor",
                turn.teacher_turn_index
            ));
        }
    }
    Ok(())
}

pub fn render_resident_corridor_audit_v1(report: &ResidentCorridorAuditV1) -> String {
    let witness = match (&report.witness, report.witness_found) {
        (Some(witness), _) => format!(
            "{}/{} HP from {} | {} actions | {} potions",
            witness.final_player_hp,
            witness.max_player_hp,
            witness.starting_player_hp,
            witness.action_count,
            witness.potion_expenditures
        ),
        (None, true) => "yes (legacy quality unavailable)".to_owned(),
        (None, false) => "no".to_owned(),
    };
    let mut output = format!(
        "Resident corridor audit | satisfaction {} | status {} | witness {} | bounded proposal seed {} | work {}/{} | elapsed {} ms | nodes {} | edges {}\n",
        report.configured_satisfaction,
        report.search_status,
        witness,
        if report.public_plan_turn_proposal_enabled {
            "enabled"
        } else {
            "disabled"
        },
        report.counters.generation_work,
        report.configured_generation_work,
        report.elapsed_ms,
        report.counters.exact_nodes,
        report.counters.exact_edges
    );
    let mut first_missing = None;
    let mut first_unserviced = None;
    for turn in &report.turns {
        let materialized = turn.successor.is_some();
        let edge = turn.edge.is_some();
        let service = turn
            .successor
            .as_ref()
            .and_then(|state| state.first_service_selection);
        let work = turn
            .successor
            .as_ref()
            .map(|state| state.generation_work)
            .unwrap_or_default();
        let rank = turn
            .edge
            .as_ref()
            .and_then(|edge| edge.successor_anchor_ordinal_rank)
            .map(|rank| rank.to_string())
            .unwrap_or_else(|| "-".to_owned());
        let candidates = turn
            .edge
            .as_ref()
            .map(|edge| edge.successor_anchor_candidate_count)
            .unwrap_or_default();
        let _ = writeln!(
            output,
            "  teacher T{} (game T{}) | state {} | edge {} | first service {} | work {} | anchor rank {}/{}",
            turn.teacher_turn_index,
            turn.from_turn,
            if materialized { "yes" } else { "no" },
            if edge { "yes" } else { "no" },
            service
                .map(|selection| selection.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            work,
            rank,
            candidates
        );
        if !materialized && first_missing.is_none() {
            first_missing = Some(turn.teacher_turn_index);
        }
        if materialized && service.is_none() && first_unserviced.is_none() {
            first_unserviced = Some(turn.teacher_turn_index);
        }
    }
    match first_missing {
        Some(turn) => {
            let _ = writeln!(output, "First missing teacher successor: T{turn}");
        }
        None => output.push_str("Every teacher successor was materialized.\n"),
    }
    if let Some(turn) = first_unserviced {
        let _ = writeln!(
            output,
            "First materialized but unserviced teacher successor: T{turn}"
        );
    }
    output
}
