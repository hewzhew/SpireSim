use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::session::RunControlSession;
use super::view_model::{build_run_control_view_model, CandidateResolution};

pub const RUN_DECISION_CASE_SCHEMA_NAME: &str = "sts_simulator.run_decision_case";
pub const RUN_DECISION_CASE_SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunDecisionCaseV3 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub saved_at_unix_ms: u128,
    pub label_role: &'static str,
    pub trainable_as_action_label: bool,
    pub policy_quality_claim: bool,
    pub run: RunDecisionRunContextV3,
    pub screen_title: String,
    pub screen_text: String,
    pub candidates: Vec<RunDecisionCandidateV3>,
    pub panels_available: Vec<&'static str>,
    pub debug_boundary: RunDecisionDebugBoundaryV3,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunDecisionRunContextV3 {
    pub seed: u64,
    pub ascension_level: u8,
    pub player_class: &'static str,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub boss: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunDecisionCandidateV3 {
    pub id: String,
    pub label: String,
    pub command: String,
    pub note: Option<String>,
    pub resolution: Option<CandidateResolution>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunDecisionDebugBoundaryV3 {
    pub engine_state: String,
    pub active_combat_engine_state: Option<String>,
    pub decision_step: u64,
}

impl RunDecisionCaseV3 {
    pub fn from_session(session: &RunControlSession) -> Self {
        let view = build_run_control_view_model(session);
        let (current_hp, max_hp) = session.visible_player_hp();

        Self {
            schema_name: RUN_DECISION_CASE_SCHEMA_NAME,
            schema_version: RUN_DECISION_CASE_SCHEMA_VERSION,
            saved_at_unix_ms: unix_ms_now(),
            label_role: "diagnostic_not_teacher_label",
            trainable_as_action_label: false,
            policy_quality_claim: false,
            run: RunDecisionRunContextV3 {
                seed: session.run_state.seed,
                ascension_level: session.run_state.ascension_level,
                player_class: session.run_state.player_class,
                act: session.run_state.act_num,
                floor: session.run_state.floor_num,
                current_hp,
                max_hp,
                gold: session.run_state.gold,
                boss: super::view_model::boss_label(&session.run_state),
            },
            screen_title: view.header.title,
            screen_text: view.decision.label,
            candidates: view
                .candidates
                .into_iter()
                .map(|candidate| RunDecisionCandidateV3 {
                    id: candidate.id,
                    label: candidate.label,
                    command: candidate.action.summary(),
                    note: candidate.note,
                    resolution: candidate.resolution,
                })
                .collect(),
            panels_available: vec![
                "main", "deck", "map", "relics", "potions", "draw", "discard", "exhaust",
                "inspect", "details", "raw",
            ],
            debug_boundary: RunDecisionDebugBoundaryV3 {
                engine_state: session.engine_state.kind_v1().to_string(),
                active_combat_engine_state: session
                    .active_combat
                    .as_ref()
                    .map(|active| active.engine_state.kind_v1().to_string()),
                decision_step: session.decision_step,
            },
        }
    }
}

pub fn save_run_decision_case_v3(path: &Path, case: &RunDecisionCaseV3) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let payload = serde_json::to_string_pretty(case).map_err(|err| err.to_string())?;
    fs::write(path, payload).map_err(|err| err.to_string())
}

pub fn default_run_decision_case_path(session: &RunControlSession) -> std::path::PathBuf {
    let screen = build_run_control_view_model(session)
        .header
        .title
        .to_ascii_lowercase()
        .replace(' ', "_");
    std::path::PathBuf::from("tools")
        .join("artifacts")
        .join("run_control_cases")
        .join(format!(
            "seed{}_step{:04}_{}_{}.decision.json",
            session.run_state.seed,
            session.decision_step,
            screen,
            unix_ms_now()
        ))
}

fn unix_ms_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
