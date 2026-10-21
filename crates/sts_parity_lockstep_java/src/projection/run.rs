use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use sts_java_parity_protocol::{canonical_encounter_schedule_id_v1, canonical_game_id};
use sts_parity_lockstep::{
    NaturalActExitV1, NaturalRunDropStateV2, NaturalRunEncounterScheduleV2,
    NaturalRunEventChanceBitsV2, NaturalRunEventScheduleV2, NaturalRunKeysV2,
    NaturalRunRelicPoolsV2, NaturalRunRoomFlagsV2, NaturalRunStateV2,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunStateCaptureV1 {
    daily: bool,
    shop_purge_base_cost: i64,
    drop_state: DropStateCaptureV1,
    room_flags: RoomFlagsCaptureV1,
    relic_pools: RelicPoolsCaptureV1,
    encounter_schedule: EncounterScheduleCaptureV1,
    event_schedule: EventScheduleCaptureV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DropStateCaptureV1 {
    potion_chance_modifier: i64,
    card_rarity_offset: i64,
    card_upgrade_chance_bits: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomFlagsCaptureV1 {
    mugged: bool,
    smoked: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelicPoolsCaptureV1 {
    common: Vec<String>,
    uncommon: Vec<String>,
    rare: Vec<String>,
    shop: Vec<String>,
    boss: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EncounterScheduleCaptureV1 {
    normal: Vec<String>,
    elite: Vec<String>,
    boss_key: Value,
    boss_queue: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventScheduleCaptureV1 {
    events: Vec<String>,
    shrines: Vec<String>,
    one_time_events: Vec<String>,
    chance_bits: EventChanceBitsCaptureV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventChanceBitsCaptureV1 {
    elite: u32,
    monster: u32,
    shop: u32,
    treasure: u32,
    shrine: u32,
}

/// Extract the observed exit, without inferring it from the floor or map node.
pub fn java_act_exit_v1(frame: &Value) -> Option<NaturalActExitV1> {
    let decision = &frame["public_base"]["decision"];
    match decision["kind"].as_str() {
        Some("boss_relic") => Some(NaturalActExitV1::BossRelicChoice),
        Some("event") if decision["event_id"].as_str() == Some("SpireHeart") => {
            Some(NaturalActExitV1::SpireHeart)
        }
        _ => None,
    }
}

pub(super) fn project_run_v1(frame: &Value) -> Result<Option<Value>, String> {
    let raw = &frame["parity_internal"]["run_state"];
    if raw.is_null() {
        return Ok(None);
    }
    let captured: RunStateCaptureV1 = serde_json::from_value(raw.clone())
        .map_err(|error| format!("Java deterministic run state is malformed: {error}"))?;
    if captured.shop_purge_base_cost < 75 || (captured.shop_purge_base_cost - 75) % 25 != 0 {
        return Err("Java shop purge base cost is outside the original progression".to_owned());
    }

    let public = &frame["public_base"]["run"];
    let seed = super::canonical_engine_seed_from_java_long(required_str(
        &frame["parity_internal"],
        "run_seed",
    )?)?;
    let boss_key = match captured.encounter_schedule.boss_key {
        Value::String(id) if !id.is_empty() => Some(canonical_encounter_schedule_id_v1(&id)),
        Value::Null => None,
        _ => return Err("Java encounter schedule has an invalid boss key".to_owned()),
    };

    let mut state = NaturalRunStateV2 {
        seed,
        ascension: required_u64(public, "ascension")?,
        character: required_str(public, "character")?.to_owned(),
        act: required_u64(public, "act")?,
        floor: required_i64(public, "floor")?,
        current_hp: required_i64(public, "current_hp")?,
        max_hp: required_i64(public, "max_hp")?,
        gold: required_i64(public, "gold")?,
        keys: NaturalRunKeysV2 {
            ruby: required_bool(public, "has_ruby_key")?,
            sapphire: required_bool(public, "has_sapphire_key")?,
            emerald: required_bool(public, "has_emerald_key")?,
        },
        final_act_available: required_bool(public, "final_act_available")?,
        daily: captured.daily,
        shop_purge_base_cost: captured.shop_purge_base_cost,
        drop_state: NaturalRunDropStateV2 {
            potion_chance_modifier: captured.drop_state.potion_chance_modifier,
            card_rarity_offset: captured.drop_state.card_rarity_offset,
            card_upgrade_chance_bits: captured.drop_state.card_upgrade_chance_bits,
        },
        room_flags: NaturalRunRoomFlagsV2 {
            mugged: captured.room_flags.mugged,
            smoked: captured.room_flags.smoked,
        },
        relic_pools: NaturalRunRelicPoolsV2 {
            common: canonical_ids("common relic pool", captured.relic_pools.common, true)?,
            uncommon: canonical_ids("uncommon relic pool", captured.relic_pools.uncommon, true)?,
            rare: canonical_ids("rare relic pool", captured.relic_pools.rare, true)?,
            shop: canonical_ids("shop relic pool", captured.relic_pools.shop, true)?,
            boss: canonical_ids("boss relic pool", captured.relic_pools.boss, true)?,
        },
        encounter_schedule: NaturalRunEncounterScheduleV2 {
            normal: canonical_encounter_ids(
                "normal encounter schedule",
                captured.encounter_schedule.normal,
                false,
            )?,
            elite: canonical_encounter_ids(
                "elite encounter schedule",
                captured.encounter_schedule.elite,
                false,
            )?,
            boss_key,
            boss_queue: canonical_encounter_ids(
                "boss encounter schedule",
                captured.encounter_schedule.boss_queue,
                false,
            )?,
        },
        event_schedule: NaturalRunEventScheduleV2 {
            events: canonical_ids("event pool", captured.event_schedule.events, true)?,
            shrines: canonical_ids("shrine pool", captured.event_schedule.shrines, true)?,
            one_time_events: canonical_ids(
                "one-time event pool",
                captured.event_schedule.one_time_events,
                true,
            )?,
            chance_bits: NaturalRunEventChanceBitsV2 {
                elite: captured.event_schedule.chance_bits.elite,
                monster: captured.event_schedule.chance_bits.monster,
                shop: captured.event_schedule.chance_bits.shop,
                treasure: captured.event_schedule.chance_bits.treasure,
                shrine: captured.event_schedule.chance_bits.shrine,
            },
        },
    };
    state.validate()?;
    state.retire_finished_act_shrines(java_act_exit_v1(frame));
    serde_json::to_value(state)
        .map(Some)
        .map_err(|error| format!("failed to encode canonical Java run state: {error}"))
}

fn canonical_ids(scope: &str, values: Vec<String>, unique: bool) -> Result<Vec<String>, String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|raw| {
            if raw.is_empty() {
                return Err(format!("Java {scope} contains an empty identity"));
            }
            let id = canonical_game_id(&raw);
            if unique && !seen.insert(id.clone()) {
                return Err(format!("Java {scope} repeats identity {id:?}"));
            }
            Ok(id)
        })
        .collect()
}

fn canonical_encounter_ids(
    scope: &str,
    values: Vec<String>,
    unique: bool,
) -> Result<Vec<String>, String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|raw| {
            if raw.is_empty() {
                return Err(format!("Java {scope} contains an empty identity"));
            }
            let id = canonical_encounter_schedule_id_v1(&raw);
            if unique && !seen.insert(id.clone()) {
                return Err(format!("Java {scope} repeats identity {id:?}"));
            }
            Ok(id)
        })
        .collect()
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("Java run state omitted string {field}"))
}

fn required_i64(value: &Value, field: &str) -> Result<i64, String> {
    value[field]
        .as_i64()
        .ok_or_else(|| format!("Java run state omitted integer {field}"))
}

fn required_u64(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .ok_or_else(|| format!("Java run state omitted unsigned integer {field}"))
}

fn required_bool(value: &Value, field: &str) -> Result<bool, String> {
    value[field]
        .as_bool()
        .ok_or_else(|| format!("Java run state omitted boolean {field}"))
}
