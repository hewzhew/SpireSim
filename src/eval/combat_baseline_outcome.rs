use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::sim::combat::CombatTerminal;

pub const COMBAT_BASELINE_OUTCOME_SCHEMA_NAME: &str = "CombatBaselineOutcomeV1";
pub const COMBAT_BASELINE_OUTCOME_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatBaselineOutcomeV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub case_id: String,
    pub terminal: CombatTerminal,
    pub start_hp: i32,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
}

impl CombatBaselineOutcomeV1 {
    pub fn terminal(&self) -> CombatTerminal {
        self.terminal
    }
}

pub fn load_combat_baseline_outcome_v1(path: &Path) -> Result<CombatBaselineOutcomeV1, String> {
    let payload = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let baseline: CombatBaselineOutcomeV1 =
        serde_json::from_str(&payload).map_err(|err| err.to_string())?;
    validate_combat_baseline_outcome_v1(&baseline)?;
    Ok(baseline)
}

pub fn save_combat_baseline_outcome_v1(
    path: &Path,
    baseline: &CombatBaselineOutcomeV1,
) -> Result<(), String> {
    validate_combat_baseline_outcome_v1(baseline)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let payload = serde_json::to_string_pretty(baseline).map_err(|err| err.to_string())?;
    fs::write(path, payload).map_err(|err| err.to_string())
}

pub fn validate_combat_baseline_outcome_v1(
    baseline: &CombatBaselineOutcomeV1,
) -> Result<(), String> {
    if baseline.schema_name != COMBAT_BASELINE_OUTCOME_SCHEMA_NAME {
        return Err(format!(
            "unsupported combat baseline schema '{}'",
            baseline.schema_name
        ));
    }
    if baseline.schema_version != COMBAT_BASELINE_OUTCOME_SCHEMA_VERSION {
        return Err(format!(
            "unsupported combat baseline schema_version {}",
            baseline.schema_version
        ));
    }
    if baseline.case_id.trim().is_empty() {
        return Err("combat baseline case_id cannot be empty".to_string());
    }
    if baseline.hp_loss != (baseline.start_hp - baseline.final_hp).max(0) {
        return Err("combat baseline hp_loss does not match start/final hp".to_string());
    }
    Ok(())
}
