use crate::transport::JavaRuntimeObservationV1;
use serde_json::Value;
use std::collections::BTreeSet;
use sts_parity_lockstep::{
    natural_run_active_rng_owners_v4, NaturalRunNeowEligibilityV1, NaturalRunRngRoomV3,
    NaturalRunRngStreamV2, NaturalRunRngStreamsV2,
};

const JAVA_RNG_ALGORITHM: &str = "com.badlogic.gdx.math.RandomXS128";

pub(super) fn project_rng_streams_v1(
    observation: &JavaRuntimeObservationV1,
    boundary_kind: &str,
    neow_eligibility: NaturalRunNeowEligibilityV1,
) -> Result<Option<Value>, String> {
    let internal = &observation.frame["rng_internal"];
    let (Some(floor_and_potion), Some(run)) = (
        internal["streams"].as_array(),
        internal["run_streams"].as_array(),
    ) else {
        return Ok(None);
    };

    let mut seen = BTreeSet::new();
    let mut projected = Vec::with_capacity(floor_and_potion.len() + run.len());
    for stream in floor_and_potion.iter().chain(run) {
        let java_owner = required_str(stream, "owner")?;
        let (owner, expected_scope) = canonical_rng_owner(java_owner)
            .ok_or_else(|| format!("Java exposed an unexpected RNG owner {java_owner:?}"))?;
        if !seen.insert(owner) {
            return Err(format!("Java repeated RNG owner {java_owner:?}"));
        }
        if required_str(stream, "algorithm")? != JAVA_RNG_ALGORITHM
            || required_str(stream, "scope")? != expected_scope
        {
            return Err(format!("Java RNG metadata is invalid for {java_owner:?}"));
        }
        let raw_counter = required_i64(stream, "counter")?;
        let counter = u32::try_from(raw_counter)
            .map_err(|_| format!("Java RNG counter is invalid for {java_owner:?}"))?;
        let raw_state = stream["state_hex"]
            .as_array()
            .filter(|state| state.len() == 2)
            .ok_or_else(|| format!("Java RNG state is invalid for {java_owner:?}"))?;
        let state_hex = raw_state
            .iter()
            .map(|word| {
                let word = word
                    .as_str()
                    .filter(|word| {
                        word.len() == 16 && word.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
                    .ok_or_else(|| format!("Java RNG state is invalid for {java_owner:?}"))?;
                Ok(word.to_ascii_uppercase())
            })
            .collect::<Result<Vec<_>, String>>()?
            .try_into()
            .map_err(|_| format!("Java RNG state is invalid for {java_owner:?}"))?;
        projected.push(NaturalRunRngStreamV2::new(
            owner,
            expected_scope,
            state_hex,
            if owner == "run.map" {
                None
            } else {
                Some(counter)
            },
        )?);
    }

    let neow_screen = observation.frame["public_base"]["decision"]["event_screen_index"].as_i64();
    let neow_rng_is_active = neow_eligibility == NaturalRunNeowEligibilityV1::Rich
        && ((boundary_kind == "natural_neow" && neow_screen == Some(3))
            || boundary_kind == "natural_neow_grid_select");
    let room = if matches!(
        boundary_kind,
        "natural_combat_reward" | "natural_card_reward"
    ) {
        match super::current_map_room(observation)? {
            "monster" => NaturalRunRngRoomV3::Monster,
            "elite" => NaturalRunRngRoomV3::Elite,
            _ => NaturalRunRngRoomV3::Other,
        }
    } else {
        NaturalRunRngRoomV3::Other
    };
    let emerald_key_acquired =
        super::required_bool(&observation.frame["public_base"]["run"], "has_emerald_key")?;
    let active_owners = natural_run_active_rng_owners_v4(
        boundary_kind,
        neow_rng_is_active,
        room,
        emerald_key_acquired,
    )?;
    if active_owners.iter().any(|owner| !seen.contains(owner)) {
        return Ok(None);
    }

    let streams = NaturalRunRngStreamsV2::for_boundary(
        projected,
        boundary_kind,
        neow_rng_is_active,
        room,
        emerald_key_acquired,
    )?;
    serde_json::to_value(streams)
        .map(Some)
        .map_err(|error| format!("failed to encode canonical Java RNG streams: {error}"))
}

fn canonical_rng_owner(java_owner: &str) -> Option<(&'static str, &'static str)> {
    let owner = match java_owner {
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterRng" => "run.monster",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.eventRng" => "run.event",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.merchantRng" => "run.merchant",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRng" => "run.card",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.treasureRng" => "run.treasure",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.relicRng" => "run.relic",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.potionRng" => "run.potion",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng" => "run.map",
        "com.megacrit.cardcrawl.neow.NeowEvent.rng" => "run.neow",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterHpRng" => "floor.monster_hp",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.aiRng" => "floor.ai",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.shuffleRng" => "floor.shuffle",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRandomRng" => "floor.card_random",
        "com.megacrit.cardcrawl.dungeons.AbstractDungeon.miscRng" => "floor.misc",
        _ => return None,
    };
    let scope = if owner.starts_with("floor.") {
        "floor"
    } else {
        "run"
    };
    Some((owner, scope))
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("Java RNG omitted string {field}"))
}

fn required_i64(value: &Value, field: &str) -> Result<i64, String> {
    value[field]
        .as_i64()
        .ok_or_else(|| format!("Java RNG omitted integer {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sts_parity_lockstep::ProducerIdentityV1;

    #[test]
    fn reward_rng_projection_uses_actual_room_and_requires_its_evidence() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".to_owned(),
                version: "test".to_owned(),
                session_epoch: "test".to_owned(),
            },
            frame_id: 1,
            actions: vec![],
            frame: json!({
                "public_base": {"run":{"has_emerald_key":false},"map": {"current_x": 0, "current_y": 0,
                    "nodes": [{"x": 0, "y": 0, "room_type": "MonsterRoom"}]}},
                "rng_internal": {"streams": [], "run_streams": []},
            }),
        };
        for (scope, field, names) in [
            (
                "floor",
                "streams",
                vec![
                    "monsterHpRng",
                    "aiRng",
                    "shuffleRng",
                    "cardRandomRng",
                    "miscRng",
                ],
            ),
            (
                "run",
                "run_streams",
                vec![
                    "monsterRng",
                    "eventRng",
                    "merchantRng",
                    "cardRng",
                    "treasureRng",
                    "relicRng",
                    "potionRng",
                    "mapRng",
                ],
            ),
        ] {
            observation.frame["rng_internal"][field] = json!(names
                .into_iter()
                .map(|name| json!({
                    "owner": format!("com.megacrit.cardcrawl.dungeons.AbstractDungeon.{name}"),
                    "algorithm": JAVA_RNG_ALGORITHM, "scope": scope, "counter": 0,
                    "state_hex": ["0000000000000001", "0000000000000002"],
                }))
                .collect::<Vec<_>>());
        }
        for boundary in ["natural_combat_reward", "natural_card_reward"] {
            for (room, count) in [
                ("MonsterRoom", 9),
                ("MonsterRoomElite", 9),
                ("EventRoom", 13),
                ("MonsterRoomBoss", 13),
            ] {
                observation.frame["public_base"]["map"]["nodes"][0]["room_type"] = json!(room);
                let projected = project_rng_streams_v1(
                    &observation,
                    boundary,
                    NaturalRunNeowEligibilityV1::Rich,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    projected.as_array().unwrap().len(),
                    count,
                    "{room}/{boundary}"
                );
                let mut with_key = observation.clone();
                with_key.frame["public_base"]["run"]["has_emerald_key"] = json!(true);
                let projected =
                    project_rng_streams_v1(&with_key, boundary, NaturalRunNeowEligibilityV1::Rich)
                        .unwrap()
                        .unwrap();
                assert_eq!(projected.as_array().unwrap().len(), count - 1);
                assert!(projected
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| s["owner"] != "run.map"));
            }
        }
        observation.frame["public_base"]["map"]["nodes"] = json!([]);
        assert!(project_rng_streams_v1(
            &observation,
            "natural_combat_reward",
            NaturalRunNeowEligibilityV1::Rich
        )
        .is_err());
    }
}
