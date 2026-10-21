use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::canonical_game_id;
use sts_parity_lockstep::{NaturalRelicCombatStateV1, NaturalRelicV3};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelicCaptureV3 {
    index: u64,
    relic_id: String,
    counter: i64,
    used_up: bool,
    #[serde(default)]
    bound_card_uuid: Option<String>,
    #[serde(default)]
    art_of_war: Option<ArtOfWarCaptureV1>,
    #[serde(default)]
    lantern: Option<LanternCaptureV1>,
    #[serde(default)]
    orange_pellets: Option<OrangePelletsCaptureV1>,
    #[serde(default)]
    centennial_puzzle: Option<CentennialPuzzleCaptureV1>,
    #[serde(default)]
    red_skull: Option<RedSkullCaptureV1>,
    #[serde(default)]
    hovering_kite: Option<HoveringKiteCaptureV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HoveringKiteCaptureV1 {
    triggered_this_turn: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RedSkullCaptureV1 {
    active: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CentennialPuzzleCaptureV1 {
    used_this_combat: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrangePelletsCaptureV1 {
    attack_played: bool,
    skill_played: bool,
    power_played: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LanternCaptureV1 {
    first_turn: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtOfWarCaptureV1 {
    first_turn: bool,
    gain_energy_next: bool,
}

pub(super) fn project_relics_v3(
    frame: &Value,
    runtime_card_bindings: Option<&BTreeMap<String, String>>,
    combat_active: bool,
) -> Result<Option<Value>, String> {
    let Some(raw_relics) = frame["parity_internal"]["relics"].as_array() else {
        return Ok(None);
    };
    let mut projected = Vec::with_capacity(raw_relics.len());
    let mut bound_cards = BTreeSet::new();
    for (expected_ordinal, raw) in raw_relics.iter().enumerate() {
        let relic: RelicCaptureV3 = serde_json::from_value(raw.clone())
            .map_err(|error| format!("Java persistent relic state is malformed: {error}"))?;
        if relic.index != expected_ordinal as u64 {
            return Err("Java persistent relic ordinals are not contiguous".to_owned());
        }
        if relic.relic_id.is_empty() {
            return Err("Java persistent relic has an empty identity".to_owned());
        }
        let id = canonical_game_id(&relic.relic_id);
        if id != "redskull" && relic.red_skull.is_some() {
            return Err("Java non-Red-Skull relic claims Red Skull state".to_owned());
        }
        if id != "centennialpuzzle" && relic.centennial_puzzle.is_some() {
            return Err("Java non-Centennial-Puzzle relic claims its combat state".to_owned());
        }
        if id != "artofwar" && relic.art_of_war.is_some() {
            return Err("Java non-Art-of-War relic claims Art of War state".to_owned());
        }
        if id != "lantern" && relic.lantern.is_some() {
            return Err("Java non-Lantern relic claims Lantern state".to_owned());
        }
        if id != "hoveringkite" && relic.hovering_kite.is_some() {
            return Err("Java non-Hovering-Kite relic claims its turn trigger state".into());
        }
        if id != "orangepellets" && relic.orange_pellets.is_some() {
            return Err("Java non-Orange-Pellets relic claims Orange Pellets state".to_owned());
        }
        let combat_state = if combat_active && id == "hoveringkite" {
            let Some(state) = relic.hovering_kite else {
                return Ok(None);
            };
            Some(NaturalRelicCombatStateV1::HoveringKite {
                triggered_this_turn: state.triggered_this_turn,
            })
        } else if combat_active && id == "redskull" {
            let Some(state) = relic.red_skull else {
                return Ok(None);
            };
            Some(NaturalRelicCombatStateV1::RedSkull {
                active: state.active,
            })
        } else if combat_active && id == "centennialpuzzle" {
            let Some(state) = relic.centennial_puzzle else {
                return Ok(None);
            };
            Some(NaturalRelicCombatStateV1::CentennialPuzzle {
                used_this_combat: state.used_this_combat,
            })
        } else if combat_active && id == "artofwar" {
            let Some(state) = relic.art_of_war else {
                return Ok(None);
            };
            Some(NaturalRelicCombatStateV1::ArtOfWar {
                first_turn: state.first_turn,
                gain_energy_next: state.gain_energy_next,
            })
        } else if combat_active && id == "lantern" {
            let Some(state) = relic.lantern else {
                return Ok(None);
            };
            Some(NaturalRelicCombatStateV1::Lantern {
                first_turn: state.first_turn,
            })
        } else if combat_active && id == "orangepellets" {
            let Some(state) = relic.orange_pellets else {
                return Ok(None);
            };
            Some(NaturalRelicCombatStateV1::OrangePellets {
                attack_played: state.attack_played,
                skill_played: state.skill_played,
                power_played: state.power_played,
            })
        } else {
            None
        };
        let is_bottle = matches!(
            id.as_str(),
            "bottledflame" | "bottledlightning" | "bottledtornado"
        );
        let bound_card = match relic.bound_card_uuid {
            None => None,
            Some(uuid) if is_bottle && !uuid.is_empty() => {
                let Some(binding) = runtime_card_bindings.and_then(|bindings| bindings.get(&uuid))
                else {
                    return Ok(None);
                };
                if !bound_cards.insert(binding.clone()) {
                    return Err("Java persistent relics repeat one bound card".to_owned());
                }
                Some(binding.clone())
            }
            Some(_) if !is_bottle => {
                return Err("Java non-bottled relic claims a bound card".to_owned())
            }
            _ => return Err("Java persistent relic has an invalid bound-card identity".to_owned()),
        };
        projected.push(NaturalRelicV3 {
            ordinal: expected_ordinal as u64,
            id,
            counter: relic.counter,
            used_up: relic.used_up,
            bound_card,
            combat_state,
        });
    }
    serde_json::to_value(projected)
        .map(Some)
        .map_err(|error| format!("failed to encode Java relics: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame() -> Value {
        json!({"parity_internal":{"relics":[{
            "index":0,"relic_id":"Art of War","counter":-1,"used_up":false
        }]}})
    }

    #[test]
    fn red_skull_requires_private_activation_independent_of_used_up() {
        let mut frame = frame();
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("Red Skull");
        assert!(project_relics_v3(&frame, None, true).unwrap().is_none());
        assert!(
            project_relics_v3(&frame, None, false).unwrap().unwrap()[0]["combat_state"].is_null()
        );
        for active in [false, true] {
            for used_up in [false, true] {
                let raw = &mut frame["parity_internal"]["relics"][0];
                raw["used_up"] = json!(used_up);
                raw["red_skull"] = json!({"active":active});
                let projected = project_relics_v3(&frame, None, true).unwrap().unwrap();
                let relic: NaturalRelicV3 = serde_json::from_value(projected[0].clone()).unwrap();
                assert_eq!(relic.used_up, used_up);
                assert_eq!(
                    relic.combat_state,
                    Some(NaturalRelicCombatStateV1::RedSkull { active })
                );
                assert!(project_relics_v3(&frame, None, false).unwrap().unwrap()[0]
                    ["combat_state"]
                    .is_null());
            }
        }
        for invalid in [
            json!({}),
            json!({"active":0}),
            json!({"active":false,"other":true}),
        ] {
            frame["parity_internal"]["relics"][0]["red_skull"] = invalid;
            assert!(project_relics_v3(&frame, None, true).is_err());
        }
        frame["parity_internal"]["relics"][0]["red_skull"] = json!({"active":false});
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("Vajra");
        for combat_active in [false, true] {
            assert!(project_relics_v3(&frame, None, combat_active).is_err());
        }
    }

    #[test]
    fn centennial_puzzle_requires_private_combat_state_independent_of_used_up() {
        let mut frame = frame();
        let raw = &mut frame["parity_internal"]["relics"][0];
        raw["relic_id"] = json!("Centennial Puzzle");
        assert!(project_relics_v3(&frame, None, true).unwrap().is_none());
        assert!(
            project_relics_v3(&frame, None, false).unwrap().unwrap()[0]["combat_state"].is_null()
        );
        for used in [false, true] {
            for used_up in [false, true] {
                frame["parity_internal"]["relics"][0]["used_up"] = json!(used_up);
                frame["parity_internal"]["relics"][0]["centennial_puzzle"] =
                    json!({"used_this_combat":used});
                let projected = project_relics_v3(&frame, None, true).unwrap().unwrap();
                assert_eq!(projected[0]["used_up"], used_up);
                assert_eq!(projected[0]["combat_state"]["used_this_combat"], used);
            }
        }
        for invalid in [
            json!({}),
            json!({"used_this_combat":1}),
            json!({"used_this_combat":false,"other":0}),
        ] {
            frame["parity_internal"]["relics"][0]["centennial_puzzle"] = invalid;
            assert!(project_relics_v3(&frame, None, true).is_err());
        }
        frame["parity_internal"]["relics"][0]["centennial_puzzle"] =
            json!({"used_this_combat":false});
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("Runic Cube");
        assert!(project_relics_v3(&frame, None, true).is_err());
    }

    #[test]
    fn orange_pellets_requires_original_combo_flags_not_the_display_counter() {
        let mut frame = frame();
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("OrangePellets");
        assert!(project_relics_v3(&frame, None, true).unwrap().is_none());
        assert!(
            project_relics_v3(&frame, None, false).unwrap().unwrap()[0]["combat_state"].is_null()
        );
        for mask in 0..8 {
            let flags = json!({
                "attack_played": mask & 1 != 0,
                "skill_played": mask & 2 != 0,
                "power_played": mask & 4 != 0,
            });
            frame["parity_internal"]["relics"][0]["orange_pellets"] = flags.clone();
            let projected = project_relics_v3(&frame, None, true).unwrap().unwrap();
            assert_eq!(projected[0]["counter"], -1);
            for key in ["attack_played", "skill_played", "power_played"] {
                assert_eq!(projected[0]["combat_state"][key], flags[key]);
                let mut missing = frame.clone();
                missing["parity_internal"]["relics"][0]["orange_pellets"]
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                assert!(project_relics_v3(&missing, None, true).is_err());
            }
        }
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("Vajra");
        assert!(project_relics_v3(&frame, None, true).is_err());
    }

    #[test]
    fn hovering_kite_requires_observed_trigger_independent_of_used_up() {
        let mut frame = frame();
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("HoveringKite");
        assert!(project_relics_v3(&frame, None, true).unwrap().is_none());
        assert!(
            project_relics_v3(&frame, None, false).unwrap().unwrap()[0]["combat_state"].is_null()
        );
        for triggered in [false, true] {
            for used_up in [false, true] {
                let raw = &mut frame["parity_internal"]["relics"][0];
                raw["used_up"] = json!(used_up);
                raw["hovering_kite"] = json!({"triggered_this_turn":triggered});
                let projected = project_relics_v3(&frame, None, true).unwrap().unwrap();
                let relic: NaturalRelicV3 = serde_json::from_value(projected[0].clone()).unwrap();
                assert_eq!(relic.used_up, used_up);
                assert_eq!(
                    relic.combat_state,
                    Some(NaturalRelicCombatStateV1::HoveringKite {
                        triggered_this_turn: triggered,
                    })
                );
            }
        }
        for invalid in [
            json!({}),
            json!({"triggered_this_turn":1}),
            json!({"triggered_this_turn":false,"other":true}),
        ] {
            frame["parity_internal"]["relics"][0]["hovering_kite"] = invalid;
            assert!(project_relics_v3(&frame, None, true).is_err());
        }
        frame["parity_internal"]["relics"][0]["hovering_kite"] =
            json!({"triggered_this_turn":false});
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("Lantern");
        assert!(project_relics_v3(&frame, None, true).is_err());
    }

    #[test]
    fn lantern_requires_its_real_first_turn_flag_independent_of_used_up() {
        let mut frame = frame();
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("Lantern");
        assert!(project_relics_v3(&frame, None, true).unwrap().is_none());
        assert!(
            project_relics_v3(&frame, None, false).unwrap().unwrap()[0]["combat_state"].is_null()
        );
        for first_turn in [false, true] {
            for used_up in [false, true] {
                let raw = &mut frame["parity_internal"]["relics"][0];
                raw["used_up"] = json!(used_up);
                raw["lantern"] = json!({"first_turn":first_turn});
                let projected = project_relics_v3(&frame, None, true).unwrap().unwrap();
                let relic: NaturalRelicV3 = serde_json::from_value(projected[0].clone()).unwrap();
                assert_eq!(relic.used_up, used_up);
                assert_eq!(
                    relic.combat_state,
                    Some(NaturalRelicCombatStateV1::Lantern { first_turn })
                );
                assert!(project_relics_v3(&frame, None, false).unwrap().unwrap()[0]
                    ["combat_state"]
                    .is_null());
            }
        }
        for invalid in [
            json!({}),
            json!({"first_turn":0}),
            json!({"first_turn":false,"other":true}),
        ] {
            frame["parity_internal"]["relics"][0]["lantern"] = invalid;
            assert!(project_relics_v3(&frame, None, true).is_err());
        }
        frame["parity_internal"]["relics"][0]["lantern"] = json!({"first_turn":false});
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("Lizard Tail");
        assert!(project_relics_v3(&frame, None, true).is_err());
    }

    #[test]
    fn art_of_war_requires_both_private_flags_during_combat() {
        let mut frame = frame();
        assert!(project_relics_v3(&frame, None, true).unwrap().is_none());
        for field in ["first_turn", "gain_energy_next"] {
            frame["parity_internal"]["relics"][0]["art_of_war"] = json!({(field): false});
            assert!(project_relics_v3(&frame, None, true).is_err());
        }
        for first_turn in [false, true] {
            for gain_energy_next in [false, true] {
                frame["parity_internal"]["relics"][0]["art_of_war"] =
                    json!({"first_turn":first_turn,"gain_energy_next":gain_energy_next});
                let value = project_relics_v3(&frame, None, true).unwrap().unwrap();
                let relic: NaturalRelicV3 = serde_json::from_value(value[0].clone()).unwrap();
                assert_eq!(relic.counter, -1);
                assert_eq!(
                    relic.combat_state,
                    Some(NaturalRelicCombatStateV1::ArtOfWar {
                        first_turn,
                        gain_energy_next,
                    })
                );
            }
        }
        // A displayed counter cannot stand in for either private boolean.
        frame["parity_internal"]["relics"][0]["counter"] = json!(7);
        let value = project_relics_v3(&frame, None, true).unwrap().unwrap();
        assert_eq!(value[0]["counter"], 7);
        frame["parity_internal"]["relics"][0]["art_of_war"]["first_turn"] = json!(0);
        assert!(project_relics_v3(&frame, None, true).is_err());
    }

    #[test]
    fn art_of_war_state_is_combat_local_and_cannot_belong_to_another_relic() {
        let mut frame = frame();
        let inactive = project_relics_v3(&frame, None, false).unwrap().unwrap();
        assert!(inactive[0]["combat_state"].is_null());
        frame["parity_internal"]["relics"][0]["art_of_war"] =
            json!({"first_turn":false,"gain_energy_next":true});
        assert_eq!(
            project_relics_v3(&frame, None, false).unwrap().unwrap(),
            inactive
        );
        frame["parity_internal"]["relics"][0]["relic_id"] = json!("InkBottle");
        assert!(project_relics_v3(&frame, None, true).is_err());
    }
}
