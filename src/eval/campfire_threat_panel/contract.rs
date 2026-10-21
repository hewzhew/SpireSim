use serde::{Deserialize, Serialize};

use crate::content::monsters::encounter_pool::{
    public_encounter_pool, EncounterPoolTier, PUBLIC_ENCOUNTER_POOL_SCHEMA_VERSION,
};
use crate::content::monsters::factory::EncounterId;
use crate::eval::campfire_survival_scenarios::CampfireSurvivalLens;
use crate::eval::combat_lab_v1::{
    CombatLabCommonBudgetV1, CombatLabProfileSpecV1, CombatLabShuffleScheduleV1,
};
use crate::eval::fingerprint::hash_serializable;
use crate::state::map::node::RoomType;

pub const CAMPFIRE_THREAT_PANEL_SCHEMA_VERSION: u32 = 1;
pub const CAMPFIRE_THREAT_PANEL_CELL_SCHEMA_VERSION: u32 = 2;
pub const CAMPFIRE_THREAT_PANEL_SUMMARY_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireThreatPanelSpecV1 {
    pub schema_version: u32,
    pub experiment_id: String,
    pub analysis_seed: u64,
    pub encounter_sources: Vec<CampfireThreatEncounterSourceV1>,
    pub schedule: CombatLabShuffleScheduleV1,
    pub lenses: Vec<CampfireSurvivalLens>,
    pub include_unchanged_root: bool,
    pub profile: CombatLabProfileSpecV1,
    pub common_budget: CombatLabCommonBudgetV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CampfireThreatEncounterSourceV1 {
    PublicPool {
        act: u8,
        tier: EncounterPoolTier,
    },
    Explicit {
        encounter_id: EncounterId,
        room_type: RoomType,
        label: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CampfireThreatEncounterProvenanceV1 {
    PublicPool {
        act: u8,
        tier: EncounterPoolTier,
        pool_schema_version: u32,
        raw_weight: f32,
        normalized_weight: f64,
    },
    Explicit {
        label: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireThreatEncounterV1 {
    pub encounter_id: EncounterId,
    pub room_type: RoomType,
    pub provenance: CampfireThreatEncounterProvenanceV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResolvedCampfireThreatPanelSpecV1 {
    pub spec: CampfireThreatPanelSpecV1,
    pub encounters: Vec<CampfireThreatEncounterV1>,
    pub contract_hash: String,
}

pub fn resolve_campfire_threat_panel_spec_v1(
    spec: CampfireThreatPanelSpecV1,
) -> Result<ResolvedCampfireThreatPanelSpecV1, String> {
    if spec.schema_version != CAMPFIRE_THREAT_PANEL_SCHEMA_VERSION {
        return Err(format!(
            "unsupported Campfire threat panel schema_version {}; expected {}",
            spec.schema_version, CAMPFIRE_THREAT_PANEL_SCHEMA_VERSION
        ));
    }
    if spec.experiment_id.trim().is_empty() {
        return Err("Campfire threat panel experiment_id must not be empty".to_string());
    }
    if spec.encounter_sources.is_empty() {
        return Err("Campfire threat panel requires at least one encounter source".to_string());
    }
    if spec.lenses.is_empty() {
        return Err("Campfire threat panel requires at least one lens".to_string());
    }
    let mut unique_lenses = Vec::new();
    for lens in &spec.lenses {
        if unique_lenses.contains(lens) {
            return Err(format!("duplicate Campfire threat panel lens {lens:?}"));
        }
        unique_lenses.push(*lens);
    }
    if spec.profile.id.trim().is_empty() {
        return Err("Campfire threat panel profile id must not be empty".to_string());
    }
    if spec.common_budget.max_nodes == 0
        || spec.common_budget.max_actions_per_line == 0
        || spec.common_budget.max_engine_steps_per_action == 0
    {
        return Err("Campfire threat panel common budget limits must be nonzero".to_string());
    }
    if spec.common_budget.wall_ms.is_some() {
        return Err(
            "Campfire threat panel forbids wall_ms: paired evidence requires a deterministic node budget"
                .to_string(),
        );
    }

    let mut encounters = Vec::new();
    for source in &spec.encounter_sources {
        match source {
            CampfireThreatEncounterSourceV1::PublicPool { act, tier } => {
                let entries = public_encounter_pool(*act, *tier);
                if entries.is_empty() {
                    return Err(format!(
                        "public encounter pool act={act} tier={tier:?} is empty"
                    ));
                }
                let total_weight = entries.iter().map(|entry| entry.weight as f64).sum::<f64>();
                let room_type = match tier {
                    EncounterPoolTier::Weak | EncounterPoolTier::Strong => RoomType::MonsterRoom,
                    EncounterPoolTier::Elite => RoomType::MonsterRoomElite,
                };
                encounters.extend(entries.iter().map(|entry| CampfireThreatEncounterV1 {
                    encounter_id: entry.encounter,
                    room_type,
                    provenance: CampfireThreatEncounterProvenanceV1::PublicPool {
                        act: *act,
                        tier: *tier,
                        pool_schema_version: PUBLIC_ENCOUNTER_POOL_SCHEMA_VERSION,
                        raw_weight: entry.weight,
                        normalized_weight: f64::from(entry.weight) / total_weight,
                    },
                }));
            }
            CampfireThreatEncounterSourceV1::Explicit {
                encounter_id,
                room_type,
                label,
            } => {
                if label.trim().is_empty() {
                    return Err("explicit encounter label must not be empty".to_string());
                }
                if !matches!(
                    room_type,
                    RoomType::MonsterRoom | RoomType::MonsterRoomElite | RoomType::MonsterRoomBoss
                ) {
                    return Err(format!(
                        "explicit encounter {} requires a combat room type",
                        encounter_id.stable_id()
                    ));
                }
                encounters.push(CampfireThreatEncounterV1 {
                    encounter_id: *encounter_id,
                    room_type: *room_type,
                    provenance: CampfireThreatEncounterProvenanceV1::Explicit {
                        label: label.clone(),
                    },
                });
            }
        }
    }
    let mut unique = Vec::<(EncounterId, RoomType)>::new();
    for encounter in &encounters {
        let key = (encounter.encounter_id, encounter.room_type);
        if unique.contains(&key) {
            return Err(format!(
                "duplicate Campfire threat encounter {:?} in {:?}",
                encounter.encounter_id, encounter.room_type
            ));
        }
        unique.push(key);
    }

    #[derive(Serialize)]
    struct ContractHashInput<'a> {
        spec: &'a CampfireThreatPanelSpecV1,
        encounters: &'a [CampfireThreatEncounterV1],
    }
    let contract_hash = hash_serializable(&ContractHashInput {
        spec: &spec,
        encounters: &encounters,
    });

    Ok(ResolvedCampfireThreatPanelSpecV1 {
        spec,
        encounters,
        contract_hash,
    })
}
