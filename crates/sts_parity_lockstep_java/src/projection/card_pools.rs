use serde_json::{json, Value};
use std::collections::BTreeSet;
use sts_java_parity_protocol::canonical_game_id;

const REQUIRED_POOLS: [&str; 6] = [
    "common",
    "uncommon",
    "rare",
    "colorless_uncommon",
    "colorless_rare",
    "colorless_combat",
];

pub(super) fn project_card_pools_v1(frame: &Value) -> Result<Option<Value>, String> {
    let Some(pools) = frame["parity_internal"]["card_pools"].as_object() else {
        return Ok(None);
    };
    if REQUIRED_POOLS.iter().any(|key| !pools.contains_key(*key)) {
        return Ok(None);
    }
    if pools.len() != REQUIRED_POOLS.len() {
        return Err("Java card-pool observation contains an unexpected pool".to_owned());
    }

    let mut projected = serde_json::Map::new();
    for key in REQUIRED_POOLS {
        let values = pools[key]
            .as_array()
            .ok_or_else(|| format!("Java card pool {key:?} is not an array"))?;
        let mut seen = BTreeSet::new();
        let mut ids = Vec::with_capacity(values.len());
        for value in values {
            let raw = value
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| format!("Java card pool {key:?} contains a non-card value"))?;
            let id = canonical_game_id(raw);
            if !seen.insert(id.clone()) {
                return Err(format!("Java card pool {key:?} repeats card {id:?}"));
            }
            ids.push(Value::String(id));
        }
        if ids.is_empty() {
            return Err(format!("Java card pool {key:?} is empty"));
        }
        projected.insert(key.to_owned(), Value::Array(ids));
    }
    Ok(Some(json!(projected)))
}
