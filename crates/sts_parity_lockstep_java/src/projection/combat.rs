use crate::transport::JavaRuntimeObservationV1;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::canonical_game_id;
use sts_parity_lockstep::{
    canonicalize_exhaust_by_binding_v4, NaturalCombatCardV4, NaturalCombatDecisionV4,
    NaturalCombatIntentEvidenceV4, NaturalCombatIntentV4, NaturalCombatMonsterV4,
    NaturalCombatPlayerV4, NaturalCombatPowerStateV4, NaturalCombatPowerV4, NaturalCombatZonesV4,
    NaturalRunCombatV4,
};

#[cfg(test)]
mod observed_regressions;

pub(super) fn project_combat_v4(
    observation: &JavaRuntimeObservationV1,
    runtime_card_bindings: Option<&BTreeMap<String, String>>,
    boundary_kind: &str,
    hand_select_profile: Option<crate::action::JavaHandSelectProfileV1>,
) -> Result<NaturalRunCombatV4, String> {
    let exhaust_hand = boundary_kind == "natural_hand_select"
        && matches!(
            hand_select_profile,
            Some(
                crate::action::JavaHandSelectProfileV1::SingleExhaust
                    | crate::action::JavaHandSelectProfileV1::OptionalExhaust { .. }
            )
        );
    let put_on_draw_pile = boundary_kind == "natural_hand_select"
        && hand_select_profile == Some(crate::action::JavaHandSelectProfileV1::SinglePutOnDrawPile);
    let discard_hand = boundary_kind == "natural_hand_select"
        && matches!(
            hand_select_profile,
            Some(crate::action::JavaHandSelectProfileV1::CardDiscard { .. })
        );
    let tools_discard = boundary_kind == "natural_hand_select"
        && matches!(
            hand_select_profile,
            Some(crate::action::JavaHandSelectProfileV1::ToolsOfTheTrade { .. })
        );
    let upgrade_hand = boundary_kind == "natural_hand_select"
        && hand_select_profile == Some(crate::action::JavaHandSelectProfileV1::SingleUpgrade);
    let copy_hand = boundary_kind == "natural_hand_select"
        && hand_select_profile == Some(crate::action::JavaHandSelectProfileV1::SingleCopy);
    let potion_exhaust = boundary_kind == "natural_hand_select"
        && hand_select_profile == Some(crate::action::JavaHandSelectProfileV1::PotionExhaust);
    let retain_hand = boundary_kind == "natural_hand_select"
        && matches!(
            hand_select_profile,
            Some(crate::action::JavaHandSelectProfileV1::Retain { .. })
        );
    let exhume = boundary_kind == "natural_combat_grid_select"
        && crate::action::is_exhume_selection(observation);
    let combat_grid = boundary_kind == "natural_combat_grid_select"
        && crate::action::is_supported_combat_grid_select(observation);
    let discard_to_hand =
        combat_grid && crate::action::combat_grid_kind(observation) == Some("discard_to_hand");
    if combat_grid
        && crate::action::combat_grid_kind(observation) == Some("scry")
        && observation.frame["parity_internal"]["zones"]
            .get("card_in_use")
            .is_none()
    {
        return Err("Java Scry omitted its explicit card-in-use observation".into());
    }
    if !matches!(
        boundary_kind,
        "natural_combat" | "natural_combat_discovery" | "natural_combat_codex"
    ) && !exhaust_hand
        && !discard_hand
        && !tools_discard
        && !put_on_draw_pile
        && !upgrade_hand
        && !copy_hand
        && !potion_exhaust
        && !retain_hand
        && !combat_grid
    {
        return Ok(NaturalRunCombatV4::Inactive);
    }
    let bindings = runtime_card_bindings.ok_or_else(|| {
        "Java combat entry has no complete source-proven card bindings".to_owned()
    })?;
    let public = &observation.frame["public_base"]["combat"];
    let internal = &observation.frame["parity_internal"];
    let turn = u32::try_from(required_u64(public, "turn")?)
        .map_err(|_| "Java combat turn exceeds u32".to_owned())?;
    let creatures = project_monster_identities_v4(observation)?;
    let player_powers =
        project_powers_with_sources_v4(&internal["player_powers"], "player", &creatures)?;
    let surrounded = player_powers
        .iter()
        .any(|power| power.power_id == "surrounded");

    let pending_zones;
    let zones = if upgrade_hand {
        pending_zones = zones_with_held_hand_cards(&internal["zones"], "armaments_held")?;
        &pending_zones
    } else if copy_hand {
        pending_zones = zones_with_held_hand_cards(&internal["zones"], "dual_wield_held")?;
        &pending_zones
    } else if exhume {
        pending_zones = zones_with_exhume_cards(&internal["zones"])?;
        &pending_zones
    } else if exhaust_hand
        || discard_hand
        || put_on_draw_pile
        || (combat_grid
            && !discard_to_hand
            && (crate::action::combat_grid_kind(observation) != Some("scry")
                || !internal["zones"]["card_in_use"].is_null()))
    {
        pending_zones = zones_with_card_in_use(&internal["zones"])?;
        &pending_zones
    } else {
        // Liquid Memories and Elixir suspend potion actions, not card plays.
        &internal["zones"]
    };
    let mut projected_zones = project_zones_v4(zones, bindings)?;
    let mut occupied = projected_zones
        .draw_next_first
        .iter()
        .chain(&projected_zones.hand)
        .chain(&projected_zones.discard_bottom_first)
        .chain(&projected_zones.exhaust_by_binding)
        .chain(&projected_zones.limbo)
        .map(|card| card.binding.clone())
        .collect::<BTreeSet<_>>();
    for (owner, card) in stasis_cards(observation)? {
        let card = project_card_v4(card, bindings, &mut BTreeSet::new())?;
        if !occupied.insert(card.binding.clone()) {
            return Err("Java Stasis held card also occurs in another card location".into());
        }
        projected_zones
            .stasis
            .push(sts_parity_lockstep::NaturalCombatStasisCardV4 { owner, card });
    }

    let monsters = project_monsters_v4(observation, surrounded)?;

    Ok(NaturalRunCombatV4::PlayerDecision {
        state: NaturalCombatDecisionV4 {
            turn,
            player: project_player_v4(observation, surrounded)?,
            zones: projected_zones,
            player_powers,
            monsters,
        },
    })
}

pub(super) fn project_player_v4(
    observation: &JavaRuntimeObservationV1,
    surrounded: bool,
) -> Result<NaturalCombatPlayerV4, String> {
    let player = &observation.frame["public_base"]["combat"]["player"];
    Ok(NaturalCombatPlayerV4 {
        current_hp: required_i32(player, "current_hp")?,
        max_hp: required_i32(player, "max_hp")?,
        block: required_i32(player, "block")?,
        energy: u8::try_from(required_u64(player, "energy")?)
            .map_err(|_| "Java combat energy exceeds u8".to_owned())?,
        character_state: match observation.frame["public_base"]["run"]["character"].as_str() {
            Some("DEFECT" | "WATCHER") => Some(project_character_state_v4(player)?),
            _ => None,
        },
        cards_discarded_this_turn: match observation.frame["public_base"]["run"]["character"]
            .as_str()
        {
            Some("THE_SILENT" | "DEFECT" | "WATCHER") => Some(
                u32::try_from(required_u64(player, "cards_discarded_this_turn")?)
                    .map_err(|_| "Java turn discard counter exceeds u32".to_owned())?,
            ),
            _ => None,
        },
        mantra_gained_this_combat: match observation.frame["public_base"]["run"]["character"]
            .as_str()
        {
            Some("WATCHER") => Some(required_i32(player, "mantra_gained_this_combat")?),
            _ => None,
        },
        facing_left: surrounded
            .then(|| required_bool(player, "facing_left"))
            .transpose()?,
    })
}

fn project_character_state_v4(
    player: &Value,
) -> Result<sts_parity_lockstep::NaturalCombatCharacterStateV4, String> {
    let state = &player["character_state"];
    let stance = canonical_game_id(required_str(state, "stance")?);
    if !matches!(stance.as_str(), "neutral" | "wrath" | "calm" | "divinity") {
        return Err(format!("Java sampled unsupported stance {stance:?}"));
    }
    let max_orbs = u8::try_from(required_u64(state, "max_orbs")?)
        .map_err(|_| "Java max_orbs exceeds u8".to_owned())?;
    let raw_orbs = state["orbs"]
        .as_array()
        .ok_or("Java character state omitted orb slots")?;
    if raw_orbs.len() != usize::from(max_orbs) {
        return Err("Java orb slot count differs from max_orbs".to_owned());
    }
    let orbs = raw_orbs
        .iter()
        .map(|orb| {
            let orb_id = canonical_game_id(required_str(orb, "orb_id")?);
            if !matches!(
                orb_id.as_str(),
                "empty" | "lightning" | "frost" | "dark" | "plasma"
            ) {
                return Err(format!("Java sampled unsupported orb {orb_id:?}"));
            }
            Ok(sts_parity_lockstep::NaturalCombatOrbV4 {
                orb_id,
                base_passive_amount: required_i32(orb, "base_passive_amount")?,
                base_evoke_amount: required_i32(orb, "base_evoke_amount")?,
                passive_amount: required_i32(orb, "passive_amount")?,
                evoke_amount: required_i32(orb, "evoke_amount")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(sts_parity_lockstep::NaturalCombatCharacterStateV4 {
        stance,
        max_orbs,
        orbs,
    })
}

pub(super) fn project_monsters_v4(
    observation: &JavaRuntimeObservationV1,
    surrounded: bool,
) -> Result<Vec<NaturalCombatMonsterV4>, String> {
    let creatures = project_monster_identities_v4(observation)?;
    let public = &observation.frame["public_base"]["combat"];
    let internal = &observation.frame["parity_internal"];
    let public_monsters = public["monsters"]
        .as_array()
        .ok_or_else(|| "Java combat entry omitted public monsters".to_owned())?;
    let internal_monsters = internal["monsters"]
        .as_array()
        .ok_or_else(|| "Java combat entry omitted internal monsters".to_owned())?;
    if public_monsters.len() != internal_monsters.len() || public_monsters.is_empty() {
        return Err("Java combat monster projections disagree in length".to_owned());
    }
    let mut enemy_counts = BTreeMap::<String, u32>::new();
    public_monsters
        .iter()
        .zip(internal_monsters)
        .enumerate()
        .map(|(index, (public_monster, internal_monster))| {
            if public_monster["index"].as_u64() != Some(index as u64)
                || public_monster["instance_id"].as_u64()
                    != internal_monster["instance_id"].as_u64()
            {
                return Err("Java public and internal monster identities disagree".to_owned());
            }
            let dead_or_escaped = required_bool(public_monster, "dead_or_escaped")?;
            // AbstractMonster clears its power list from the wall-clock death
            // animation, not from the deterministic action queue.  A stable
            // player decision can therefore observe either the old list or an
            // empty list without any gameplay state having changed.  Powers on
            // terminal monsters are inert here and do not belong in parity.
            let powers = if dead_or_escaped {
                Vec::new()
            } else {
                for power in internal_monster["powers"]
                    .as_array()
                    .ok_or("Java monster omitted powers")?
                {
                    if power["id"].as_str() == Some("Curl Up") {
                        let settled = observation.frame["boundary"]["kind"].as_str()
                            == Some("combat_player_decision")
                            || (observation.frame["boundary"]["evidence"]["actions_pending"]
                                .as_u64()
                                == Some(0)
                                && observation.frame["boundary"]["evidence"]["card_queue_pending"]
                                    .as_u64()
                                    == Some(0));
                        // A fully drained decision cannot retain triggered Curl Up:
                        // onAttacked always queues its removal. A choice with
                        // pending actions must sample the actual flag, including false.
                        if !settled && power["curl_up_triggered"].as_bool().is_none() {
                            return Err(
                                "Java suspended Curl Up omitted its observed triggered flag".into(),
                            );
                        }
                        if settled && power["curl_up_triggered"].as_bool() == Some(true) {
                            return Err(
                                "Java settled Curl Up still carries a triggered flag".into()
                            );
                        }
                    }
                }
                project_powers_with_sources_v4(
                    &internal_monster["powers"],
                    &format!("monster {index}"),
                    &creatures,
                )?
            };
            let public_intent = if dead_or_escaped {
                NaturalCombatIntentV4::terminal_monster()
            } else {
                project_intent(&public_monster["public_intent"])?
            };
            let enemy_id = canonical_game_id(required_str(public_monster, "id")?);
            let occurrence = *enemy_counts.entry(enemy_id.clone()).or_default();
            *enemy_counts
                .get_mut(&enemy_id)
                .expect("enemy occurrence was inserted") += 1;
            Ok(NaturalCombatMonsterV4 {
                enemy_id,
                occurrence,
                current_hp: required_i32(public_monster, "current_hp")?,
                max_hp: required_i32(public_monster, "max_hp")?,
                block: required_i32(public_monster, "block")?,
                dead_or_escaped,
                left_of_player: surrounded
                    .then(|| required_bool(public_monster, "left_of_player"))
                    .transpose()?,
                next_move_id: java_byte(internal_monster, "next_move_id")?,
                move_history: internal_monster["move_history"]
                    .as_array()
                    .ok_or_else(|| "Java combat monster omitted move history".to_owned())?
                    .iter()
                    .map(java_byte_value)
                    .collect::<Result<Vec<_>, _>>()?,
                public_intent,
                powers,
            })
        })
        .collect::<Result<Vec<_>, String>>()
}

pub(super) fn zones_with_held_hand_cards(zones: &Value, held_zone: &str) -> Result<Value, String> {
    let mut normalized = zones_with_card_in_use(zones)?;
    let held = zones[held_zone]["cards"]
        .as_array()
        .ok_or_else(|| format!("Java pending selection omitted its {held_zone} cards"))?;
    let hand = normalized["hand"]["cards"]
        .as_array_mut()
        .ok_or("Java pending selection omitted its candidate hand")?;
    hand.extend(held.iter().cloned());
    Ok(normalized)
}

pub(super) fn zones_with_exhume_cards(zones: &Value) -> Result<Value, String> {
    let mut normalized = zones_with_card_in_use(zones)?;
    let held = zones["exhume_held"]["cards"]
        .as_array()
        .ok_or("Java Exhume omitted its temporarily held exhaust cards")?;
    let exhaust = normalized["exhaust"]["cards"]
        .as_array_mut()
        .ok_or("Java Exhume omitted its exhaust pile")?;
    for card in held {
        if required_str(card, "card_id")? != "Exhume" {
            return Err("Java Exhume held a non-Exhume card outside its source pile".to_owned());
        }
        exhaust.push(card.clone());
    }
    Ok(normalized)
}

fn zones_with_card_in_use(zones: &Value) -> Result<Value, String> {
    let card = zones
        .get("card_in_use")
        .ok_or_else(|| "Java pending card choice omitted the card-in-use observation".to_owned())?;
    if card.is_null() {
        return Err("Java pending card choice has no card in use".to_owned());
    }
    let uuid = required_str(card, "instance_uuid")?;
    let mut result = zones.clone();
    let limbo = result["limbo"]["cards"]
        .as_array_mut()
        .ok_or_else(|| "Java pending choice omitted limbo cards".to_owned())?;
    if let Some(existing) = limbo
        .iter()
        .find(|candidate| candidate["instance_uuid"].as_str() == Some(uuid))
    {
        if existing != card {
            return Err("Java card-in-use and limbo observations disagree for one UUID".to_owned());
        }
    } else {
        limbo.push(card.clone());
    }
    Ok(result)
}

fn project_zone(
    zones: &Value,
    name: &str,
    bindings: &BTreeMap<String, String>,
    seen_uuids: &mut BTreeSet<String>,
) -> Result<Vec<NaturalCombatCardV4>, String> {
    let zone = &zones[name];
    if zone["ordering"].as_str() != Some("java_group_index_ascending")
        || zone["top"].as_str() != Some("last")
    {
        return Err(format!("Java combat zone {name:?} has unknown ordering"));
    }
    zone["cards"]
        .as_array()
        .ok_or_else(|| format!("Java combat zone {name:?} omitted its cards"))?
        .iter()
        .map(|card| {
            let mut projected = project_card_v4(card, bindings, seen_uuids)?;
            if name == "limbo" {
                // UseCardAction has already paid and clears this flag before disposal.
                projected.free_to_play_once = false;
            }
            Ok(projected)
        })
        .collect()
}

fn project_card_v4(
    card: &Value,
    bindings: &BTreeMap<String, String>,
    seen_uuids: &mut BTreeSet<String>,
) -> Result<NaturalCombatCardV4, String> {
    let uuid = required_str(card, "instance_uuid")?;
    if !seen_uuids.insert(uuid.to_owned()) {
        return Err("Java combat zones repeat one runtime card UUID".to_owned());
    }
    let binding = bindings
        .get(uuid)
        .ok_or_else(|| "Java combat card UUID has no source-proven binding".to_owned())?;
    Ok(NaturalCombatCardV4 {
        binding: binding.clone(),
        card_id: canonical_game_id(required_str(card, "card_id")?),
        upgrades: u8::try_from(required_u64(card, "upgrades")?)
            .map_err(|_| "Java combat card upgrades exceed u8".to_owned())?,
        combat_cost: required_i32(card, "combat_cost")?,
        cost_for_turn: required_i32(card, "cost_for_turn")?,
        free_to_play_once: match card.get("free_to_play_once") {
            Some(value) => value
                .as_bool()
                .ok_or("Java card free-once flag is not a boolean")?,
            // Earlier capture scope excludes Setup; authorizing Setup requires
            // an explicit sampled flag on every card in its adapter authorization.
            None => false,
        },
        base_damage: persistent_card_damage(card)?,
        misc: persistent_card_misc(card)?,
    })
}

pub(super) fn persistent_card_misc(card: &Value) -> Result<Option<i32>, String> {
    let card_id = canonical_game_id(required_str(card, "card_id")?);
    if !matches!(card_id.as_str(), "geneticalgorithm" | "ritualdagger") {
        return Ok(None);
    }
    let misc = required_i32(card, "misc")?;
    if misc < 1 {
        return Err(format!(
            "Java {card_id} omitted its positive observed growth value"
        ));
    }
    Ok(Some(misc))
}

pub(super) fn stasis_cards(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<(sts_parity_lockstep::NaturalCombatCreatureV4, &Value)>, String> {
    let creatures = project_monster_identities_v4(observation)?;
    let internal = observation.frame["parity_internal"]["monsters"]
        .as_array()
        .ok_or("Java Stasis observation omitted internal monsters")?;
    let public = observation.frame["public_base"]["combat"]["monsters"]
        .as_array()
        .ok_or("Java Stasis observation omitted public monsters")?;
    if internal.len() != public.len() {
        return Err("Java Stasis monster lists disagree".into());
    }
    let mut held = Vec::new();
    for (monster, visible) in internal.iter().zip(public) {
        if required_bool(visible, "dead_or_escaped")? {
            continue;
        }
        let owner = creatures
            .get(&required_u64(monster, "instance_id")?)
            .ok_or("Java Stasis owner is absent from creature identities")?;
        for power in monster["powers"]
            .as_array()
            .ok_or("Java monster omitted powers")?
        {
            if power["id"].as_str() == Some("Stasis") {
                let card = &power["stasis_card"];
                required_str(card, "instance_uuid")?;
                held.push((owner.clone(), card));
            }
        }
    }
    Ok(held)
}

fn persistent_card_damage(card: &Value) -> Result<Option<i32>, String> {
    let card_id = canonical_game_id(required_str(card, "card_id")?);
    if !matches!(card_id.as_str(), "rampage" | "glassknife") {
        return Ok(None);
    }
    let amount = required_i32(card, "base_damage")?;
    if amount < 0 {
        return Err(format!(
            "Java {card_id} has negative persistent base damage"
        ));
    }
    Ok(Some(amount))
}

fn project_exhaust_zone(
    zones: &Value,
    bindings: &BTreeMap<String, String>,
    seen_uuids: &mut BTreeSet<String>,
) -> Result<Vec<NaturalCombatCardV4>, String> {
    let mut cards = project_zone(zones, "exhaust", bindings, seen_uuids)?;
    canonicalize_exhaust_by_binding_v4(&mut cards)?;
    Ok(cards)
}

fn project_intent(value: &Value) -> Result<NaturalCombatIntentV4, String> {
    match required_str(value, "evidence")? {
        "visible_exact" => {
            let raw_kind = required_str(value, "visible_intent")?;
            let kind = match raw_kind {
                "ATTACK" => "attack",
                "ATTACK_BUFF" => "attack_buff",
                "ATTACK_DEBUFF" => "attack_debuff",
                "ATTACK_DEFEND" => "attack_defend",
                "BUFF" => "buff",
                "DEBUFF" => "debuff",
                "STRONG_DEBUFF" => "strong_debuff",
                "DEBUG" => "debug",
                "DEFEND" => "defend",
                "DEFEND_DEBUFF" => "defend_debuff",
                "DEFEND_BUFF" => "defend_buff",
                "ESCAPE" => "escape",
                "MAGIC" => "magic",
                "NONE" => "none",
                "SLEEP" => "sleep",
                "STUN" => "stun",
                "UNKNOWN" => "unknown",
                other => return Err(format!("Java combat intent {other:?} is unsupported")),
            };
            let damage_per_hit = optional_i32(value, "damage_per_hit")?;
            let hits = optional_u8(value, "hits")?;
            if kind.starts_with("attack") != (damage_per_hit.is_some() && hits.is_some())
                || !value["hidden_reason"].is_null()
            {
                return Err("Java visible combat intent has inconsistent payload".to_owned());
            }
            Ok(NaturalCombatIntentV4 {
                evidence: NaturalCombatIntentEvidenceV4::VisibleExact,
                kind: Some(kind.to_owned()),
                damage_per_hit,
                hits,
                hidden_reason: None,
            })
        }
        "hidden" => {
            if !value["visible_intent"].is_null()
                || !value["damage_per_hit"].is_null()
                || !value["hits"].is_null()
            {
                return Err("Java hidden combat intent leaks a visible payload".to_owned());
            }
            Ok(NaturalCombatIntentV4 {
                evidence: NaturalCombatIntentEvidenceV4::Hidden,
                kind: None,
                damage_per_hit: None,
                hits: None,
                hidden_reason: Some(required_str(value, "hidden_reason")?.to_owned()),
            })
        }
        other => Err(format!(
            "Java combat intent evidence {other:?} is unsupported"
        )),
    }
}

pub(super) fn project_monster_identities_v4(
    observation: &JavaRuntimeObservationV1,
) -> Result<BTreeMap<u64, sts_parity_lockstep::NaturalCombatCreatureV4>, String> {
    use sts_parity_lockstep::NaturalCombatCreatureV4;
    let public = observation.frame["public_base"]["combat"]["monsters"]
        .as_array()
        .ok_or("Java omitted public monster identities")?;
    let internal = observation.frame["parity_internal"]["monsters"]
        .as_array()
        .ok_or("Java omitted internal monster identities")?;
    if public.len() != internal.len() {
        return Err("Java public and internal monster identities disagree".to_owned());
    }
    let mut creatures = BTreeMap::new();
    let mut counts = BTreeMap::<String, u32>::new();
    for (index, (monster, private)) in public.iter().zip(internal).enumerate() {
        let instance = required_u64(monster, "instance_id")?;
        if required_u64(monster, "index")? != index as u64
            || required_u64(private, "instance_id")? != instance
        {
            return Err("Java public and internal monster identities disagree".to_owned());
        }
        let enemy_id = canonical_game_id(required_str(monster, "id")?);
        let occurrence = counts.entry(enemy_id.clone()).or_default();
        if creatures
            .insert(
                instance,
                NaturalCombatCreatureV4::Monster {
                    enemy_id,
                    occurrence: *occurrence,
                },
            )
            .is_some()
        {
            return Err("Java repeats a monster instance identity".to_owned());
        }
        *occurrence += 1;
    }
    Ok(creatures)
}

#[cfg(test)]
pub(super) fn project_powers_v4(
    value: &Value,
    owner: &str,
) -> Result<Vec<NaturalCombatPowerV4>, String> {
    project_powers_with_sources_v4(value, owner, &BTreeMap::new())
}

pub(super) fn project_powers_with_sources_v4(
    value: &Value,
    owner: &str,
    creatures: &BTreeMap<u64, sts_parity_lockstep::NaturalCombatCreatureV4>,
) -> Result<Vec<NaturalCombatPowerV4>, String> {
    let powers = value
        .as_array()
        .ok_or_else(|| format!("Java combat {owner} omitted powers"))?;
    let mut occurrences = BTreeMap::<String, u32>::new();
    powers
        .iter()
        .enumerate()
        .map(|(expected_index, power)| {
            if required_u64(power, "index")? != expected_index as u64 {
                return Err(format!(
                    "Java combat {owner} power indices are not contiguous"
                ));
            }
            let amount = required_i32(power, "amount")?;
            let raw_id = required_str(power, "id")?;
            if raw_id != "Constricted" && !power["constricted_source"].is_null() {
                return Err("Java non-Constricted power carries a creature source".to_owned());
            }
            if raw_id != "Poison" && !power["poison_source"].is_null() {
                return Err("Java non-Poison power carries a Poison source".to_owned());
            }
            if raw_id != "Invincible" && !power["invincible_max_amount"].is_null() {
                return Err("Java non-Invincible power carries its private maximum".to_owned());
            }
            if raw_id != "Combust" && !power["combust_hp_loss"].is_null() {
                return Err("Java non-Combust power carries its private HP loss".to_owned());
            }
            if raw_id != "Stasis" && !power["stasis_card"].is_null() {
                return Err("Java non-Stasis power holds a Stasis card".into());
            }
            if raw_id != "Curl Up" && !power["curl_up_triggered"].is_null() {
                return Err("Java non-Curl-Up power carries its triggered flag".into());
            }
            if raw_id != "Echo Form" && !power["echo_cards_doubled_this_turn"].is_null() {
                return Err("Java non-Echo-Form power carries its private repeat count".into());
            }
            let (expected_class, expected_type, state) = match raw_id {
                "Echo Form" => (
                    "com.megacrit.cardcrawl.powers.EchoPower",
                    "BUFF",
                    echo_form_state(power, owner, amount)?,
                ),
                "EndTurnDeath" => {
                    if owner != "player" || amount != -1 {
                        return Err(
                            "Java Blasphemy death power has invalid owner or sentinel state".into(),
                        );
                    }
                    (
                        "com.megacrit.cardcrawl.powers.watcher.EndTurnDeathPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Poison" => (
                    "com.megacrit.cardcrawl.powers.PoisonPower",
                    "DEBUFF",
                    poison_state(power, amount, creatures)?,
                ),
                "Stasis" => {
                    if owner == "player" || amount != -1 {
                        return Err("Java Stasis has invalid owner or amount".into());
                    }
                    required_str(&power["stasis_card"], "instance_uuid")?;
                    let mut remaining = power.clone();
                    remaining.as_object_mut().unwrap().remove("stasis_card");
                    require_no_power_private_state(&remaining)?;
                    (
                        "com.megacrit.cardcrawl.powers.StasisPower",
                        "BUFF",
                        NaturalCombatPowerStateV4::Stasis,
                    )
                }
                "Constricted" => (
                    "com.megacrit.cardcrawl.powers.ConstrictedPower",
                    "DEBUFF",
                    constricted_state(power, amount, creatures)?,
                ),
                "Blur" => (
                    "com.megacrit.cardcrawl.powers.BlurPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "CorpseExplosionPower" => (
                    "com.megacrit.cardcrawl.powers.CorpseExplosionPower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Wraith Form v2" => {
                    if amount == 0 {
                        return Err("Java Wraith Form has a zero dexterity rate".into());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.WraithFormPower",
                        "DEBUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Strength" => (
                    "com.megacrit.cardcrawl.powers.StrengthPower",
                    if amount > 0 { "BUFF" } else { "DEBUFF" },
                    require_stateless_power(power)?,
                ),
                "Dexterity" => (
                    "com.megacrit.cardcrawl.powers.DexterityPower",
                    if amount > 0 { "BUFF" } else { "DEBUFF" },
                    require_stateless_power(power)?,
                ),
                "Focus" => (
                    "com.megacrit.cardcrawl.powers.FocusPower",
                    if amount >= 0 { "BUFF" } else { "DEBUFF" },
                    require_stateless_power(power)?,
                ),
                "Vulnerable" => (
                    "com.megacrit.cardcrawl.powers.VulnerablePower",
                    "DEBUFF",
                    round_duration_state(power)?,
                ),
                "Weakened" => (
                    "com.megacrit.cardcrawl.powers.WeakPower",
                    "DEBUFF",
                    round_duration_state(power)?,
                ),
                "Frail" => (
                    "com.megacrit.cardcrawl.powers.FrailPower",
                    "DEBUFF",
                    round_duration_state(power)?,
                ),
                "Draw Reduction" => {
                    if amount <= 0 {
                        return Err("Java Draw Reduction must have a positive duration".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.DrawReductionPower",
                        "DEBUFF",
                        round_duration_state(power)?,
                    )
                }
                "Curl Up" => (
                    "com.megacrit.cardcrawl.powers.CurlUpPower",
                    "BUFF",
                    curl_up_state(power, amount)?,
                ),
                "Flight" => (
                    "com.megacrit.cardcrawl.powers.FlightPower",
                    "BUFF",
                    flight_state(power, amount)?,
                ),
                "Combust" => (
                    "com.megacrit.cardcrawl.powers.CombustPower",
                    "BUFF",
                    combust_state(power, amount)?,
                ),
                "Intangible" => (
                    "com.megacrit.cardcrawl.powers.IntangiblePower",
                    "BUFF",
                    turn_duration_state(power, amount)?,
                ),
                "Evolve" => (
                    "com.megacrit.cardcrawl.powers.EvolvePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Curiosity" => (
                    "com.megacrit.cardcrawl.powers.CuriosityPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Unawakened" => {
                    if amount != -1 {
                        return Err("Java Unawakened is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.UnawakenedPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Surrounded" | "BackAttack" => {
                    if amount != -1 {
                        return Err(format!("Java {raw_id} is not the original-game sentinel"));
                    }
                    (
                        if raw_id == "Surrounded" {
                            "com.megacrit.cardcrawl.powers.SurroundedPower"
                        } else {
                            "com.megacrit.cardcrawl.powers.BackAttackPower"
                        },
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Slow" => {
                    if amount < 0 {
                        return Err("Java Slow carries a negative count".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.SlowPower",
                        "DEBUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Time Warp" => {
                    if !(0..12).contains(&amount) {
                        return Err("Java Time Warp count is outside its stable range".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.TimeWarpPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Malleable" => (
                    "com.megacrit.cardcrawl.powers.MalleablePower",
                    "BUFF",
                    malleable_state(power, amount)?,
                ),
                "Anger" => (
                    "com.megacrit.cardcrawl.powers.AngerPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Rage" => (
                    "com.megacrit.cardcrawl.powers.RagePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Angry" => (
                    "com.megacrit.cardcrawl.powers.AngryPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Metallicize" => (
                    "com.megacrit.cardcrawl.powers.MetallicizePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Buffer" => (
                    "com.megacrit.cardcrawl.powers.BufferPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Thorns" => (
                    "com.megacrit.cardcrawl.powers.ThornsPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Flame Barrier" => (
                    "com.megacrit.cardcrawl.powers.FlameBarrierPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Fire Breathing" => (
                    "com.megacrit.cardcrawl.powers.FireBreathingPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "IntangiblePlayer" => (
                    "com.megacrit.cardcrawl.powers.IntangiblePlayerPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Hex" => (
                    "com.megacrit.cardcrawl.powers.HexPower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Retain Cards" => (
                    "com.megacrit.cardcrawl.powers.RetainCardPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Generic Strength Up Power" => (
                    "com.megacrit.cardcrawl.powers.GenericStrengthUpPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "BeatOfDeath" => (
                    "com.megacrit.cardcrawl.powers.BeatOfDeathPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Invincible" => (
                    "com.megacrit.cardcrawl.powers.InvinciblePower",
                    "BUFF",
                    invincible_state(power, amount)?,
                ),
                "Explosive" => (
                    "com.megacrit.cardcrawl.powers.ExplosivePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Fading" => (
                    "com.megacrit.cardcrawl.powers.FadingPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Shifting" => {
                    if amount != -1 {
                        return Err("Java Shifting is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.ShiftingPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Shackled" => {
                    if amount == 0 || !(-999..=999).contains(&amount) {
                        return Err("Java Shackled carries an invalid amount".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.GainStrengthPower",
                        "DEBUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Plated Armor" => (
                    "com.megacrit.cardcrawl.powers.PlatedArmorPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Regenerate" => (
                    "com.megacrit.cardcrawl.powers.RegenerateMonsterPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Regeneration" => (
                    "com.megacrit.cardcrawl.powers.RegenPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Spore Cloud" => (
                    "com.megacrit.cardcrawl.powers.SporeCloudPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Split" => {
                    if amount != -1 {
                        return Err("Java Split lost its original sentinel amount".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.SplitPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Artifact" => (
                    "com.megacrit.cardcrawl.powers.ArtifactPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Mode Shift" => (
                    "com.megacrit.cardcrawl.powers.ModeShiftPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Sharp Hide" => (
                    "com.megacrit.cardcrawl.powers.SharpHidePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Thievery" => (
                    "com.megacrit.cardcrawl.powers.ThieveryPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Feel No Pain" => (
                    "com.megacrit.cardcrawl.powers.FeelNoPainPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Repair" => (
                    "com.megacrit.cardcrawl.powers.RepairPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "After Image" => (
                    "com.megacrit.cardcrawl.powers.AfterImagePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Energized" => (
                    "com.megacrit.cardcrawl.powers.EnergizedPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "EnergizedBlue" => (
                    "com.megacrit.cardcrawl.powers.EnergizedBluePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Draw Card" => (
                    "com.megacrit.cardcrawl.powers.DrawCardNextTurnPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Choked" => (
                    "com.megacrit.cardcrawl.powers.ChokePower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Dark Embrace" => (
                    "com.megacrit.cardcrawl.powers.DarkEmbracePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Rupture" => (
                    "com.megacrit.cardcrawl.powers.RupturePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Juggernaut" => (
                    "com.megacrit.cardcrawl.powers.JuggernautPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Painful Stabs" => {
                    if amount != -1 {
                        return Err(
                            "Java Painful Stabs is not the original-game sentinel".to_owned()
                        );
                    }
                    (
                        "com.megacrit.cardcrawl.powers.PainfulStabsPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Brutality" => (
                    "com.megacrit.cardcrawl.powers.BrutalityPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Berserk" => (
                    "com.megacrit.cardcrawl.powers.BerserkPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Demon Form" => (
                    "com.megacrit.cardcrawl.powers.DemonFormPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Loop" => (
                    "com.megacrit.cardcrawl.powers.LoopPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Equilibrium" => (
                    "com.megacrit.cardcrawl.powers.EquilibriumPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Heatsink" => (
                    "com.megacrit.cardcrawl.powers.HeatsinkPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Hello" => (
                    "com.megacrit.cardcrawl.powers.HelloPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Creative AI" => (
                    "com.megacrit.cardcrawl.powers.CreativeAIPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Storm" => (
                    "com.megacrit.cardcrawl.powers.StormPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "StaticDischarge" => (
                    "com.megacrit.cardcrawl.powers.StaticDischargePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Amplify" => (
                    "com.megacrit.cardcrawl.powers.AmplifyPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Draw" => (
                    "com.megacrit.cardcrawl.powers.DrawPower",
                    if amount < 0 { "DEBUFF" } else { "BUFF" },
                    require_stateless_power(power)?,
                ),
                "Electro" => {
                    if amount != -1 {
                        return Err("Java Electro is not the original-game sentinel".into());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.ElectroPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Pen Nib" => {
                    if owner != "player" || amount != 1 {
                        return Err("Java Pen Nib is not the original player marker".into());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.PenNibPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Vigor" => (
                    "com.megacrit.cardcrawl.powers.watcher.VigorPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Flex" => (
                    "com.megacrit.cardcrawl.powers.LoseStrengthPower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "DexLoss" => (
                    "com.megacrit.cardcrawl.powers.LoseDexterityPower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Lockon" => (
                    "com.megacrit.cardcrawl.powers.LockOnPower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Bias" => (
                    "com.megacrit.cardcrawl.powers.BiasPower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Double Tap" => (
                    "com.megacrit.cardcrawl.powers.DoubleTapPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "DuplicationPower" => (
                    "com.megacrit.cardcrawl.powers.DuplicationPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "WireheadingPower" => (
                    "com.megacrit.cardcrawl.powers.watcher.ForesightPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Controlled" => (
                    "com.megacrit.cardcrawl.powers.watcher.MentalFortressPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Mantra" => (
                    "com.megacrit.cardcrawl.powers.watcher.MantraPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "BattleHymn" => (
                    "com.megacrit.cardcrawl.powers.watcher.BattleHymnPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Adaptation" => (
                    "com.megacrit.cardcrawl.powers.watcher.RushdownPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Nirvana" => (
                    "com.megacrit.cardcrawl.powers.watcher.NirvanaPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "WaveOfTheHandPower" => (
                    "com.megacrit.cardcrawl.powers.watcher.WaveOfTheHandPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Noxious Fumes" => (
                    "com.megacrit.cardcrawl.powers.NoxiousFumesPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Envenom" => (
                    "com.megacrit.cardcrawl.powers.EnvenomPower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Tools Of The Trade" => (
                    "com.megacrit.cardcrawl.powers.ToolsOfTheTradePower",
                    "BUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "BlockReturnPower" => (
                    "com.megacrit.cardcrawl.powers.watcher.BlockReturnPower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "Corruption" => {
                    if amount != -1 {
                        return Err("Java Corruption is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.CorruptionPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Confusion" => {
                    if amount != -1 {
                        return Err("Java Confusion is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.ConfusionPower",
                        "DEBUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Compulsive" => {
                    if amount != -1 {
                        return Err("Java Reactive is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.ReactivePower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Life Link" => {
                    if amount != -1 {
                        return Err("Java Life Link is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.RegrowPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Minion" => {
                    if amount != -1 {
                        return Err("Java Minion is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.MinionPower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Barricade" => {
                    if amount != -1 {
                        return Err("Java Barricade is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.BarricadePower",
                        "BUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Entangled" => (
                    "com.megacrit.cardcrawl.powers.EntanglePower",
                    "DEBUFF",
                    require_positive_stateless_power(power, amount)?,
                ),
                "No Draw" => {
                    if amount != -1 {
                        return Err("Java No Draw is not the original-game sentinel".to_owned());
                    }
                    (
                        "com.megacrit.cardcrawl.powers.NoDrawPower",
                        "DEBUFF",
                        require_stateless_power(power)?,
                    )
                }
                "Ritual" => (
                    "com.megacrit.cardcrawl.powers.RitualPower",
                    "BUFF",
                    ritual_state(power)?,
                ),
                other => {
                    return Err(format!(
                        "natural combat schema v4 does not model Java power {other:?}"
                    ))
                }
            };
            if required_str(power, "class_name")? != expected_class
                || required_str(power, "type")? != expected_type
            {
                return Err(format!(
                    "Java combat power {:?} is not the expected original-game class and type",
                    required_str(power, "id")?
                ));
            }
            let power_id = canonical_game_id(match raw_id {
                "Weakened" => "Weak",
                "Entangled" => "Entangle",
                "Generic Strength Up Power" => "GenericStrengthUp",
                "Flex" => "LoseStrength",
                "DexLoss" => "DexterityDown",
                "Compulsive" => "Reactive",
                other => other,
            });
            let occurrence = *occurrences.entry(power_id.clone()).or_default();
            *occurrences
                .get_mut(&power_id)
                .expect("power occurrence was inserted") += 1;
            Ok(NaturalCombatPowerV4 {
                power_id,
                occurrence,
                amount,
                state,
            })
        })
        .collect()
}

fn constricted_state(
    value: &Value,
    amount: i32,
    creatures: &BTreeMap<u64, sts_parity_lockstep::NaturalCombatCreatureV4>,
) -> Result<NaturalCombatPowerStateV4, String> {
    Ok(NaturalCombatPowerStateV4::Constricted {
        source: creature_power_source(value, "constricted_source", amount, creatures)?,
    })
}

fn creature_power_source(
    value: &Value,
    field: &str,
    amount: i32,
    creatures: &BTreeMap<u64, sts_parity_lockstep::NaturalCombatCreatureV4>,
) -> Result<sts_parity_lockstep::NaturalCombatCreatureV4, String> {
    use sts_parity_lockstep::NaturalCombatCreatureV4;
    let mut unrelated = value.clone();
    unrelated
        .as_object_mut()
        .ok_or("Java power is not an object")?
        .remove(field);
    require_no_power_private_state(&unrelated)?;
    if amount <= 0 {
        return Err(format!("Java {field} must have positive damage"));
    }
    let source = &value[field];
    let source = match required_str(source, "kind")? {
        "player" if source["monster_instance_id"].is_null() => NaturalCombatCreatureV4::Player,
        "monster" => creatures
            .get(&required_u64(source, "monster_instance_id")?)
            .cloned()
            .ok_or_else(|| format!("Java {field} is absent from the current creature boundary"))?,
        _ => return Err(format!("Java {field} is unobserved or malformed")),
    };
    Ok(source)
}

fn require_stateless_power(value: &Value) -> Result<NaturalCombatPowerStateV4, String> {
    require_no_power_private_state(value)?;
    Ok(NaturalCombatPowerStateV4::Stateless)
}

fn poison_state(
    value: &Value,
    amount: i32,
    creatures: &BTreeMap<u64, sts_parity_lockstep::NaturalCombatCreatureV4>,
) -> Result<NaturalCombatPowerStateV4, String> {
    Ok(NaturalCombatPowerStateV4::Poison {
        source: creature_power_source(value, "poison_source", amount, creatures)?,
    })
}

fn require_positive_stateless_power(
    value: &Value,
    amount: i32,
) -> Result<NaturalCombatPowerStateV4, String> {
    if amount <= 0 {
        return Err("Java power is not positive at a stable boundary".to_owned());
    }
    require_stateless_power(value)
}

fn round_duration_state(value: &Value) -> Result<NaturalCombatPowerStateV4, String> {
    if !value["turn_duration_just_applied"].is_null() {
        return Err("Java round-duration power carries turn-duration state".to_owned());
    }
    if !value["malleable_base_power"].is_null() {
        return Err("Java round-duration power carries Malleable state".to_owned());
    }
    if !value["flight_stored_amount"].is_null() {
        return Err("Java round-duration power carries Flight state".to_owned());
    }
    if !value["ritual_skip_first"].is_null() || !value["ritual_player_controlled"].is_null() {
        return Err("Java round-duration power carries Ritual state".to_owned());
    }
    Ok(NaturalCombatPowerStateV4::RoundDuration {
        just_applied: required_bool(value, "round_duration_just_applied")?,
    })
}

fn turn_duration_state(value: &Value, amount: i32) -> Result<NaturalCombatPowerStateV4, String> {
    if amount <= 0
        || !value["round_duration_just_applied"].is_null()
        || !value["ritual_skip_first"].is_null()
        || !value["ritual_player_controlled"].is_null()
        || !value["flight_stored_amount"].is_null()
        || !value["malleable_base_power"].is_null()
    {
        return Err(
            "Java turn-duration power carries invalid amount or unrelated state".to_owned(),
        );
    }
    Ok(NaturalCombatPowerStateV4::TurnDuration {
        just_applied: required_bool(value, "turn_duration_just_applied")?,
    })
}

fn ritual_state(value: &Value) -> Result<NaturalCombatPowerStateV4, String> {
    if !value["turn_duration_just_applied"].is_null() {
        return Err("Java Ritual carries turn-duration state".to_owned());
    }
    if !value["malleable_base_power"].is_null() {
        return Err("Java Ritual carries Malleable state".to_owned());
    }
    if !value["round_duration_just_applied"].is_null() || !value["flight_stored_amount"].is_null() {
        return Err("Java Ritual carries round-duration state".to_owned());
    }
    Ok(NaturalCombatPowerStateV4::Ritual {
        skip_first: required_bool(value, "ritual_skip_first")?,
        player_controlled: required_bool(value, "ritual_player_controlled")?,
    })
}

fn curl_up_state(value: &Value, amount: i32) -> Result<NaturalCombatPowerStateV4, String> {
    let triggered = match &value["curl_up_triggered"] {
        Value::Null | Value::Bool(false) => false,
        Value::Bool(true) => true,
        _ => return Err("Java Curl Up triggered flag is not a boolean".into()),
    };
    let mut unrelated = value.clone();
    unrelated
        .as_object_mut()
        .ok_or("Java power is not an object")?
        .remove("curl_up_triggered");
    require_positive_stateless_power(&unrelated, amount)?;
    Ok(if triggered {
        NaturalCombatPowerStateV4::CurlUpTriggered
    } else {
        NaturalCombatPowerStateV4::Stateless
    })
}

fn flight_state(value: &Value, amount: i32) -> Result<NaturalCombatPowerStateV4, String> {
    if !value["turn_duration_just_applied"].is_null() {
        return Err("Java Flight carries turn-duration state".to_owned());
    }
    if !value["malleable_base_power"].is_null() {
        return Err("Java Flight carries Malleable state".to_owned());
    }
    if !value["round_duration_just_applied"].is_null()
        || !value["ritual_skip_first"].is_null()
        || !value["ritual_player_controlled"].is_null()
    {
        return Err("Java Flight carries unrelated private state".to_owned());
    }
    let stored_amount = required_i32(value, "flight_stored_amount")?;
    if amount <= 0 || stored_amount <= 0 {
        return Err("Java Flight carries invalid current or stored amount".to_owned());
    }
    Ok(NaturalCombatPowerStateV4::Flight { stored_amount })
}

fn malleable_state(value: &Value, amount: i32) -> Result<NaturalCombatPowerStateV4, String> {
    if !value["turn_duration_just_applied"].is_null() {
        return Err("Java Malleable carries turn-duration state".to_owned());
    }
    if !value["round_duration_just_applied"].is_null()
        || !value["ritual_skip_first"].is_null()
        || !value["ritual_player_controlled"].is_null()
        || !value["flight_stored_amount"].is_null()
    {
        return Err("Java Malleable carries unrelated private state".to_owned());
    }
    let base_power = required_i32(value, "malleable_base_power")?;
    if amount <= 0 || base_power <= 0 {
        return Err("Java Malleable carries invalid current or base amount".to_owned());
    }
    Ok(NaturalCombatPowerStateV4::Malleable { base_power })
}

fn invincible_state(value: &Value, amount: i32) -> Result<NaturalCombatPowerStateV4, String> {
    let max_amount = required_i32(value, "invincible_max_amount")?;
    if max_amount <= 0 || !(0..=max_amount).contains(&amount) {
        return Err("Java Invincible carries invalid remaining or maximum amount".to_owned());
    }
    let mut unrelated = value.clone();
    unrelated
        .as_object_mut()
        .ok_or("Java power is not an object")?
        .remove("invincible_max_amount");
    require_no_power_private_state(&unrelated)?;
    Ok(NaturalCombatPowerStateV4::Invincible { max_amount })
}

fn combust_state(value: &Value, amount: i32) -> Result<NaturalCombatPowerStateV4, String> {
    let hp_loss = required_i32(value, "combust_hp_loss")?;
    if amount <= 0 || hp_loss <= 0 {
        return Err("Java Combust carries invalid damage or HP loss".to_owned());
    }
    let mut unrelated = value.clone();
    unrelated
        .as_object_mut()
        .ok_or("Java power is not an object")?
        .remove("combust_hp_loss");
    require_no_power_private_state(&unrelated)?;
    Ok(NaturalCombatPowerStateV4::Combust { hp_loss })
}

fn require_no_power_private_state(value: &Value) -> Result<(), String> {
    if value["round_duration_just_applied"].is_null()
        && value["turn_duration_just_applied"].is_null()
        && value["ritual_skip_first"].is_null()
        && value["ritual_player_controlled"].is_null()
        && value["flight_stored_amount"].is_null()
        && value["malleable_base_power"].is_null()
        && value["invincible_max_amount"].is_null()
        && value["constricted_source"].is_null()
        && value["poison_source"].is_null()
        && value["combust_hp_loss"].is_null()
        && value["stasis_card"].is_null()
        && value["curl_up_triggered"].is_null()
        && value["echo_cards_doubled_this_turn"].is_null()
    {
        Ok(())
    } else {
        Err("Java stateless power carries unexpected private state".to_owned())
    }
}

fn echo_form_state(
    value: &Value,
    owner: &str,
    amount: i32,
) -> Result<NaturalCombatPowerStateV4, String> {
    let count = required_i32(value, "echo_cards_doubled_this_turn")?;
    if owner != "player" || amount <= 0 || !(0..=amount).contains(&count) {
        return Err("Java Echo Form carries invalid owner, stacks or repeat count".into());
    }
    let mut unrelated = value.clone();
    unrelated
        .as_object_mut()
        .ok_or("Java power is not an object")?
        .remove("echo_cards_doubled_this_turn");
    require_no_power_private_state(&unrelated)?;
    Ok(NaturalCombatPowerStateV4::EchoForm {
        cards_doubled_this_turn: count,
    })
}

fn java_byte(value: &Value, field: &str) -> Result<u8, String> {
    java_byte_value(&value[field])
}

fn java_byte_value(value: &Value) -> Result<u8, String> {
    let signed = value
        .as_i64()
        .ok_or_else(|| "Java monster move byte is not an integer".to_owned())?;
    let signed =
        i8::try_from(signed).map_err(|_| "Java monster move byte is outside i8".to_owned())?;
    Ok(signed as u8)
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("Java combat state omitted string {field}"))
}

fn required_i32(value: &Value, field: &str) -> Result<i32, String> {
    i32::try_from(
        value[field]
            .as_i64()
            .ok_or_else(|| format!("Java combat state omitted integer {field}"))?,
    )
    .map_err(|_| format!("Java combat integer {field} exceeds i32"))
}

fn required_u64(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .ok_or_else(|| format!("Java combat state omitted unsigned integer {field}"))
}

fn required_bool(value: &Value, field: &str) -> Result<bool, String> {
    value[field]
        .as_bool()
        .ok_or_else(|| format!("Java combat state omitted boolean {field}"))
}

fn optional_i32(value: &Value, field: &str) -> Result<Option<i32>, String> {
    if value[field].is_null() {
        Ok(None)
    } else {
        required_i32(value, field).map(Some)
    }
}

fn optional_u8(value: &Value, field: &str) -> Result<Option<u8>, String> {
    if value[field].is_null() {
        Ok(None)
    } else {
        u8::try_from(required_u64(value, field)?)
            .map(Some)
            .map_err(|_| format!("Java combat integer {field} exceeds u8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watcher_mantra_total_requires_an_actual_sample_after_current_power_is_spent() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "test".into(),
                version: "1".into(),
                session_epoch: "mantra".into(),
            },
            frame_id: 1,
            actions: vec![],
            frame: json!({"public_base": {
                "run":{"character":"WATCHER"},
                "combat":{"player":{"current_hp":52,"max_hp":68,"block":0,"energy":3,
                    "cards_discarded_this_turn":0,
                    "character_state":{"stance":"Divinity","max_orbs":0,"orbs":[]}}}
            }}),
        };
        assert!(project_player_v4(&observation, false).is_err());
        for amount in [0, 2, 10, 12] {
            observation.frame["public_base"]["combat"]["player"]["mantra_gained_this_combat"] =
                json!(amount);
            assert_eq!(
                project_player_v4(&observation, false)
                    .unwrap()
                    .mantra_gained_this_combat,
                Some(amount)
            );
        }
        observation.frame["public_base"]["combat"]["player"]["mantra_gained_this_combat"] =
            json!("12");
        assert!(project_player_v4(&observation, false).is_err());
    }

    #[test]
    fn silent_discard_counter_requires_the_actual_sample_including_zero() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "test".into(),
                version: "1".into(),
                session_epoch: "silent-counter".into(),
            },
            frame_id: 1,
            actions: vec![],
            frame: serde_json::json!({"public_base":{
                "run":{"character":"THE_SILENT"},
                "combat":{"player":{"current_hp":57,"max_hp":66,"block":8,"energy":2}}
            }}),
        };
        assert!(project_player_v4(&observation, false).is_err());
        for count in [0, 1, 4] {
            observation.frame["public_base"]["combat"]["player"]["cards_discarded_this_turn"] =
                serde_json::json!(count);
            assert_eq!(
                project_player_v4(&observation, false)
                    .unwrap()
                    .cards_discarded_this_turn,
                Some(count)
            );
        }
        observation.frame["public_base"]["combat"]["player"]["cards_discarded_this_turn"] =
            serde_json::json!(-1);
        assert!(project_player_v4(&observation, false).is_err());
    }
    #[test]
    fn stasis_projects_actual_card_custody_and_rejects_missing_or_duplicate_identity() {
        let card = serde_json::json!({"instance_uuid":"held-card", "card_id":"Defend_R",
            "upgrades":1, "combat_cost":2, "cost_for_turn":0});
        let power = serde_json::json!({"index":0,"id":"Stasis",
            "class_name":"com.megacrit.cardcrawl.powers.StasisPower", "type":"BUFF", "amount":-1,
            "stasis_card":card});
        let mut zones = serde_json::Map::new();
        for zone in ["hand", "draw", "discard", "exhaust", "limbo"] {
            zones.insert(zone.into(), serde_json::json!({"ordering":"java_group_index_ascending","top":"last","cards":[]}));
        }
        let observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "fixture".into(),
                version: "test".into(),
                session_epoch: "test".into(),
            },
            frame_id: 1,
            actions: vec![],
            frame: serde_json::json!({"public_base":{"combat":{"turn":3,
                "player":{"current_hp":30,"max_hp":37,"block":0,"energy":4},
                "monsters":[{"index":0,"instance_id":72,"id":"BronzeOrb","current_hp":20,"max_hp":58,
                    "block":0,"dead_or_escaped":false,
                    "public_intent":{"evidence":"visible_exact","visible_intent":"UNKNOWN"}}]}},
                "parity_internal":{"zones":zones,"player_powers":[],
                    "monsters":[{"instance_id":72,"next_move_id":1,"move_history":[3],"powers":[power]}]}}),
        };
        let bindings = BTreeMap::from([("held-card".into(), "existing-card-source".into())]);
        let project = |raw: &JavaRuntimeObservationV1, bindings: &BTreeMap<String, String>| {
            project_combat_v4(raw, Some(bindings), "natural_combat", None)
        };
        let NaturalRunCombatV4::PlayerDecision { state } =
            project(&observation, &bindings).unwrap()
        else {
            panic!()
        };
        assert!(state.zones.limbo.is_empty());
        assert_eq!(state.zones.stasis.len(), 1);
        let held = &state.zones.stasis[0];
        assert_eq!(held.card.binding, "existing-card-source");
        assert_eq!(
            (
                held.card.upgrades,
                held.card.combat_cost,
                held.card.cost_for_turn
            ),
            (1, 2, 0)
        );
        assert_eq!(
            held.owner,
            sts_parity_lockstep::NaturalCombatCreatureV4::Monster {
                enemy_id: "bronzeorb".into(),
                occurrence: 0
            }
        );
        assert_eq!(
            state.monsters[0].powers[0].state,
            NaturalCombatPowerStateV4::Stasis
        );
        assert!(project(&observation, &BTreeMap::new()).is_err());
        for fault in 0..5 {
            let mut invalid = observation.clone();
            match fault {
                0 => {
                    invalid.frame["parity_internal"]["monsters"][0]["powers"][0]["stasis_card"] =
                        Value::Null
                }
                1 => {
                    invalid.frame["parity_internal"]["zones"]["hand"]["cards"] =
                        serde_json::json!([card])
                }
                2 => {
                    invalid.frame["parity_internal"]["monsters"][0]["powers"][0]["amount"] =
                        serde_json::json!(1)
                }
                3 => {
                    invalid.frame["parity_internal"]["monsters"][0]["powers"][0]["class_name"] =
                        serde_json::json!("SomeOtherPower")
                }
                _ => {
                    let mut duplicate = power.clone();
                    duplicate["index"] = serde_json::json!(1);
                    invalid.frame["parity_internal"]["monsters"][0]["powers"]
                        .as_array_mut()
                        .unwrap()
                        .push(duplicate);
                }
            }
            assert!(project(&invalid, &bindings).is_err(), "fault {fault}");
        }
        let mut returned = observation;
        returned.frame["public_base"]["combat"]["monsters"][0]["dead_or_escaped"] =
            serde_json::json!(true);
        returned.frame["parity_internal"]["zones"]["hand"]["cards"] = serde_json::json!([card]);
        let NaturalRunCombatV4::PlayerDecision { state } = project(&returned, &bindings).unwrap()
        else {
            panic!()
        };
        assert!(state.zones.stasis.is_empty());
        assert_eq!(state.zones.hand[0].binding, "existing-card-source");
    }

    #[test]
    fn constricted_binds_the_retained_monster_instance_to_its_canonical_occurrence() {
        use sts_parity_lockstep::NaturalCombatCreatureV4;
        let mut observation =
            crate::observed_regression_fixtures::observation("reduced-flight.json");
        observation.frame["public_base"]["combat"]["monsters"] = json!([
            {"index":0,"instance_id":91,"id":"Serpent"},
            {"index":1,"instance_id":37,"id":"Serpent"}
        ]);
        observation.frame["parity_internal"]["monsters"] = json!([
            {"instance_id":91}, {"instance_id":37}
        ]);
        let creatures = project_monster_identities_v4(&observation).unwrap();
        let power = json!([{
            "index":0,"id":"Constricted","amount":14,"type":"DEBUFF",
            "class_name":"com.megacrit.cardcrawl.powers.ConstrictedPower",
            "constricted_source":{"kind":"monster","monster_instance_id":37}
        }]);
        let projected = project_powers_with_sources_v4(&power, "player", &creatures).unwrap();
        assert_eq!(projected[0].power_id, "constricted");
        assert_eq!(
            projected[0].state,
            NaturalCombatPowerStateV4::Constricted {
                source: NaturalCombatCreatureV4::Monster {
                    enemy_id: "spiregrowth".to_owned(),
                    occurrence: 1
                }
            }
        );
        for (field, invalid_value) in [
            ("constricted_source", Value::Null),
            (
                "constricted_source",
                json!({"kind":"monster","monster_instance_id":2}),
            ),
            (
                "constricted_source",
                json!({"kind":"monster","monster_instance_id":"37"}),
            ),
            ("constricted_source", json!({"kind":"unobserved"})),
            (
                "constricted_source",
                json!({"kind":"player","monster_instance_id":37}),
            ),
            ("amount", json!(0)),
            ("amount", json!(-1)),
            ("class_name", json!("mod.ConstrictedPower")),
            ("type", json!("BUFF")),
            ("flight_stored_amount", json!(14)),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = invalid_value;
            assert!(
                project_powers_with_sources_v4(&invalid, "player", &creatures).is_err(),
                "{field}"
            );
        }
        let mut player_source = power.clone();
        player_source[0]["constricted_source"] = json!({"kind":"player"});
        assert_eq!(
            project_powers_with_sources_v4(&player_source, "monster", &creatures).unwrap()[0].state,
            NaturalCombatPowerStateV4::Constricted {
                source: NaturalCombatCreatureV4::Player
            }
        );
        for private_id in [Value::Null, json!(91), json!(999)] {
            let mut invalid = observation.clone();
            invalid.frame["parity_internal"]["monsters"][1]["instance_id"] = private_id;
            assert!(project_monster_identities_v4(&invalid).is_err());
        }
        observation.frame["public_base"]["combat"]["monsters"][1]["instance_id"] = json!(91);
        observation.frame["parity_internal"]["monsters"][1]["instance_id"] = json!(91);
        assert!(project_monster_identities_v4(&observation).is_err());
    }

    #[test]
    fn constricted_source_cannot_be_smuggled_onto_another_power() {
        let power = json!([{"index":0,"id":"Strength","amount":1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.StrengthPower",
            "constricted_source":{"kind":"player"}}]);
        assert!(project_powers_v4(&power, "player").is_err());
    }

    #[test]
    fn watcher_stance_and_attack_block_powers_require_native_identity_and_positive_amount() {
        for (id, class, kind, owner, canonical) in [
            ("Mantra", "MantraPower", "BUFF", "player", "mantra"),
            ("Adaptation", "RushdownPower", "BUFF", "player", "rushdown"),
            ("Nirvana", "NirvanaPower", "BUFF", "player", "nirvana"),
            (
                "WaveOfTheHandPower",
                "WaveOfTheHandPower",
                "BUFF",
                "player",
                "waveofthehandpower",
            ),
            (
                "BattleHymn",
                "BattleHymnPower",
                "BUFF",
                "player",
                "battlehymn",
            ),
            (
                "Controlled",
                "MentalFortressPower",
                "BUFF",
                "player",
                "mentalfortresspower",
            ),
            (
                "BlockReturnPower",
                "BlockReturnPower",
                "DEBUFF",
                "monster",
                "blockreturnpower",
            ),
        ] {
            let power = json!([{
                "index":0,"id":id,"amount":4,"type":kind,
                "class_name":format!("com.megacrit.cardcrawl.powers.watcher.{class}")
            }]);
            let projected = project_powers_v4(&power, owner).unwrap();
            assert_eq!(projected[0].power_id, canonical);
            assert_eq!(projected[0].amount, 4);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(-1)),
                (
                    "type",
                    json!(if kind == "BUFF" { "DEBUFF" } else { "BUFF" }),
                ),
                ("class_name", json!(format!("mod.{class}"))),
                ("round_duration_just_applied", json!(false)),
                ("turn_duration_just_applied", json!(false)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, owner).is_err(), "{id}.{field}");
            }
        }
    }

    #[test]
    fn entangle_preserves_positive_native_stacks_instead_of_a_negative_sentinel() {
        for amount in [1, 2] {
            let power = json!([{"index":0,"id":"Entangled","amount":amount,"type":"DEBUFF",
                "class_name":"com.megacrit.cardcrawl.powers.EntanglePower"}]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].power_id, "entangle");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(-1)),
                ("amount", json!(0)),
                ("type", json!("BUFF")),
                ("class_name", json!("mod.EntanglePower")),
                ("round_duration_just_applied", json!(false)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err(), "{field}");
            }
        }
    }

    #[test]
    fn blasphemy_death_power_requires_native_player_buff_and_negative_sentinel() {
        let power = json!([{
            "index":0,"id":"EndTurnDeath","amount":-1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.watcher.EndTurnDeathPower"
        }]);
        let projected = project_powers_v4(&power, "player").unwrap();
        assert_eq!(projected[0].power_id, "endturndeathpower");
        assert_eq!(projected[0].amount, -1);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        assert!(project_powers_v4(&power, "monster").is_err());
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(1)),
            ("type", json!("DEBUFF")),
            ("class_name", json!("mod.EndTurnDeathPower")),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err(), "{field}");
        }
    }

    #[test]
    fn poison_requires_its_observed_damage_source_and_resolves_monster_identity() {
        use sts_parity_lockstep::NaturalCombatCreatureV4;
        let power = json!([{
            "index":0,"id":"Poison","amount":6,"type":"DEBUFF",
            "class_name":"com.megacrit.cardcrawl.powers.PoisonPower",
            "poison_source":{"kind":"player"}
        }]);
        assert_eq!(
            project_powers_v4(&power, "monster").unwrap()[0].state,
            NaturalCombatPowerStateV4::Poison {
                source: NaturalCombatCreatureV4::Player
            }
        );
        for (field, value) in [
            ("poison_source", Value::Null),
            ("poison_source", json!({"kind":"unobserved"})),
            (
                "poison_source",
                json!({"kind":"player","monster_instance_id":37}),
            ),
            (
                "poison_source",
                json!({"kind":"monster","monster_instance_id":37}),
            ),
            ("amount", json!(0)),
            ("amount", json!(-1)),
            ("type", json!("BUFF")),
            ("class_name", json!("mod.PoisonPower")),
            ("round_duration_just_applied", json!(false)),
            ("constricted_source", json!({"kind":"player"})),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "monster").is_err(), "{field}");
        }
        let mut monster_source = power.clone();
        monster_source[0]["poison_source"] = json!({"kind":"monster","monster_instance_id":37});
        let creature = NaturalCombatCreatureV4::Monster {
            enemy_id: "sentry".into(),
            occurrence: 1,
        };
        let bindings = BTreeMap::from([(37, creature.clone())]);
        assert_eq!(
            project_powers_with_sources_v4(&monster_source, "player", &bindings).unwrap()[0].state,
            NaturalCombatPowerStateV4::Poison { source: creature }
        );
        let mut smuggled = power;
        smuggled[0]["id"] = json!("Strength");
        smuggled[0]["type"] = json!("BUFF");
        smuggled[0]["class_name"] = json!("com.megacrit.cardcrawl.powers.StrengthPower");
        assert!(project_powers_v4(&smuggled, "player").is_err());
    }

    #[test]
    fn persistent_misc_cards_require_observed_growth_in_every_combat_zone() {
        let bindings = BTreeMap::from([("genetic-uuid".to_owned(), "bound-genetic".to_owned())]);
        for card_id in ["Genetic Algorithm", "RitualDagger"] {
            for zone in ["hand", "draw", "discard", "exhaust", "limbo"] {
                let raw = json!({
                    zone: {"ordering":"java_group_index_ascending", "top":"last", "cards":[{
                        "instance_uuid":"genetic-uuid", "card_id":card_id, "upgrades":0,
                        "combat_cost":1, "cost_for_turn":1, "misc":17, "block":999, "damage":999
                    }]}
                });
                let projected = project_zone(&raw, zone, &bindings, &mut BTreeSet::new()).unwrap();
                assert_eq!(serde_json::to_value(&projected[0]).unwrap()["misc"], 17);
                for invalid_value in [Value::Null, json!(0), json!(-1), json!("17")] {
                    let mut invalid = raw.clone();
                    invalid[zone]["cards"][0]["misc"] = invalid_value;
                    assert!(project_zone(&invalid, zone, &bindings, &mut BTreeSet::new()).is_err());
                }
                let mut later = raw.clone();
                later[zone]["cards"][0]["misc"] = json!(19);
                assert_ne!(
                    projected,
                    project_zone(&later, zone, &bindings, &mut BTreeSet::new()).unwrap()
                );
            }
        }
    }

    #[test]
    fn growing_and_decaying_cards_require_persistent_damage_in_every_combat_zone() {
        let bindings = BTreeMap::from([("mutation-uuid".to_owned(), "bound-mutation".to_owned())]);
        for (card_id, initial, later_damage) in [("Rampage", 13, 18), ("GlassKnife", 6, 4)] {
            for zone in ["hand", "draw", "discard", "exhaust", "limbo"] {
                let raw = json!({
                    zone: {"ordering":"java_group_index_ascending", "top":"last", "cards":[{
                        "instance_uuid":"mutation-uuid", "card_id":card_id, "upgrades":0,
                        "combat_cost":1, "cost_for_turn":0, "base_damage":initial, "damage":999
                    }]}
                });
                let projected = project_zone(&raw, zone, &bindings, &mut BTreeSet::new()).unwrap();
                assert_eq!(projected[0].base_damage, Some(initial));
                assert_eq!(projected[0].binding, "bound-mutation");
                for invalid_damage in [Value::Null, json!(-1), json!("13")] {
                    let mut invalid = raw.clone();
                    invalid[zone]["cards"][0]["base_damage"] = invalid_damage;
                    assert!(project_zone(&invalid, zone, &bindings, &mut BTreeSet::new()).is_err());
                }
                let mut later = raw.clone();
                later[zone]["cards"][0]["base_damage"] = json!(later_damage);
                assert_ne!(
                    projected,
                    project_zone(&later, zone, &bindings, &mut BTreeSet::new()).unwrap()
                );
                later[zone]["cards"][0]["base_damage"] = json!(0);
                assert_eq!(
                    project_zone(&later, zone, &bindings, &mut BTreeSet::new()).unwrap()[0]
                        .base_damage,
                    Some(0)
                );
            }
        }
    }

    #[test]
    fn amplify_requires_the_original_remaining_repeat_budget() {
        let raw = json!([{"index":0,"id":"Amplify","amount":1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.AmplifyPower"}]);
        let projected = project_powers_v4(&raw, "player").unwrap();
        assert_eq!(projected[0].power_id, "amplify");
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        let mut invalid = raw.clone();
        invalid[0]["amount"] = json!(0);
        assert!(project_powers_v4(&invalid, "player").is_err());
        invalid = raw;
        invalid[0]["echo_cards_doubled_this_turn"] = json!(0);
        assert!(project_powers_v4(&invalid, "player").is_err());
    }

    #[test]
    fn echo_form_requires_observed_repeat_count_and_rejects_unrelated_private_state() {
        let raw = json!([{"index":0,"id":"Echo Form","amount":2,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.EchoPower","echo_cards_doubled_this_turn":0}]);
        for count in 0..=2 {
            let mut current = raw.clone();
            current[0]["echo_cards_doubled_this_turn"] = json!(count);
            let projected = project_powers_v4(&current, "player").unwrap();
            assert_eq!(projected[0].power_id, "echoform");
            assert_eq!(projected[0].amount, 2);
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::EchoForm {
                    cards_doubled_this_turn: count,
                }
            );
        }
        assert!(project_powers_v4(&raw, "monster").is_err());
        for (field, value) in [
            ("amount", json!(0)),
            ("echo_cards_doubled_this_turn", Value::Null),
            ("echo_cards_doubled_this_turn", json!(-1)),
            ("echo_cards_doubled_this_turn", json!(3)),
            ("echo_cards_doubled_this_turn", json!("1")),
            ("class_name", json!("modded.EchoPower")),
            ("type", json!("DEBUFF")),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = raw.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
        let unrelated = json!([{"index":0,"id":"Draw","amount":1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.DrawPower","echo_cards_doubled_this_turn":0}]);
        assert!(project_powers_v4(&unrelated, "player").is_err());
    }

    #[test]
    fn defect_positive_trigger_counts_require_the_original_class_and_no_private_state() {
        for (id, canonical, class) in [
            ("Heatsink", "heatsink", "HeatsinkPower"),
            ("Hello", "hello", "HelloPower"),
            ("Creative AI", "creativeai", "CreativeAIPower"),
            ("Storm", "storm", "StormPower"),
            ("StaticDischarge", "staticdischarge", "StaticDischargePower"),
        ] {
            let raw = json!([{"index":0,"id":id,"amount":1,"type":"BUFF",
            "class_name":format!("com.megacrit.cardcrawl.powers.{class}")}]);
            let projected = project_powers_v4(&raw, "player").unwrap();
            assert_eq!(projected[0].power_id, canonical);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(-1)),
                ("type", json!("DEBUFF")),
                ("class_name", json!("modded.HeatsinkPower")),
                ("round_duration_just_applied", json!(false)),
            ] {
                let mut invalid = raw.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err());
            }
        }
    }

    #[test]
    fn draw_preserves_signed_hand_size_and_requires_the_corresponding_original_type() {
        for amount in [-2, 0, 1, 3] {
            let raw = json!([{"index":0,"id":"Draw","amount":amount,
                "type":if amount < 0 { "DEBUFF" } else { "BUFF" },
                "class_name":"com.megacrit.cardcrawl.powers.DrawPower"}]);
            let projected = project_powers_v4(&raw, "player").unwrap();
            assert_eq!(projected[0].power_id, "draw");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("type", json!(if amount < 0 { "BUFF" } else { "DEBUFF" })),
                ("class_name", json!("modded.DrawPower")),
                ("round_duration_just_applied", json!(false)),
            ] {
                let mut invalid = raw.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err());
            }
        }
    }

    #[test]
    fn electro_requires_the_original_stateless_sentinel() {
        let raw = json!([{"index":0,"id":"Electro","amount":-1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.ElectroPower"}]);
        let projected = project_powers_v4(&raw, "player").unwrap();
        assert_eq!(projected[0].power_id, "electro");
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(1)),
            ("type", json!("DEBUFF")),
            ("class_name", json!("modded.ElectroPower")),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = raw.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn retain_cards_requires_original_class_positive_stacks_and_no_private_state() {
        let raw = json!([{"index":0,"id":"Retain Cards","amount":1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.RetainCardPower"}]);
        for amount in [1, 2, 7] {
            let mut stacked = raw.clone();
            stacked[0]["amount"] = json!(amount);
            let projected = project_powers_v4(&stacked, "player").unwrap();
            assert_eq!(projected[0].power_id, "retaincards");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(-1)),
            ("class_name", json!("modded.RetainCardPower")),
            ("type", json!("DEBUFF")),
            ("round_duration_just_applied", json!(true)),
        ] {
            let mut invalid = raw.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn equilibrium_requires_the_original_class_positive_rounds_and_no_private_state() {
        let raw = json!([{"index":0,"id":"Equilibrium","amount":1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.EquilibriumPower"}]);
        let projected = project_powers_v4(&raw, "player").unwrap();
        assert_eq!(projected[0].power_id, "equilibrium");
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(-1)),
            ("class_name", json!("modded.EquilibriumPower")),
            ("type", json!("DEBUFF")),
            ("round_duration_just_applied", json!(true)),
        ] {
            let mut invalid = raw.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn loop_requires_the_original_class_positive_stack_and_no_private_state() {
        let raw = json!([{"index":0,"id":"Loop","amount":1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.LoopPower"}]);
        for amount in [1, 2, 7] {
            let mut stacked = raw.clone();
            stacked[0]["amount"] = json!(amount);
            let projected = project_powers_v4(&stacked, "player").unwrap();
            assert_eq!(projected[0].power_id, "loop");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(-1)),
            ("class_name", json!("modded.LoopPower")),
            ("type", json!("DEBUFF")),
            ("round_duration_just_applied", json!(true)),
        ] {
            let mut invalid = raw.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn lock_on_requires_a_positive_original_debuff_without_duration_private_state() {
        let raw = json!([{"index":0,"id":"Lockon","amount":2,"type":"DEBUFF",
            "class_name":"com.megacrit.cardcrawl.powers.LockOnPower"}]);
        let projected = project_powers_v4(&raw, "monster").unwrap();
        assert_eq!(projected[0].power_id, "lockon");
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(-1)),
            (
                "class_name",
                json!("com.megacrit.cardcrawl.powers.WeakPower"),
            ),
            ("type", json!("BUFF")),
            ("round_duration_just_applied", json!(true)),
        ] {
            let mut invalid = raw.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "monster").is_err());
        }
    }

    #[test]
    fn pen_nib_requires_the_original_player_class_amount_and_stateless_marker() {
        let raw = json!([{"index":0,"id":"Pen Nib","amount":1,"type":"BUFF",
            "class_name":"com.megacrit.cardcrawl.powers.PenNibPower"}]);
        let projected = project_powers_v4(&raw, "player").unwrap();
        assert_eq!(projected[0].power_id, "pennib");
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        assert!(project_powers_v4(&raw, "monster").is_err());
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(2)),
            (
                "class_name",
                json!("com.megacrit.cardcrawl.powers.StrengthPower"),
            ),
            ("type", json!("DEBUFF")),
            ("round_duration_just_applied", json!(true)),
        ] {
            let mut invalid = raw.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn invincible_requires_the_actual_reset_amount_even_when_current_budget_is_zero() {
        for (amount, max_amount) in [(0, 200), (17, 300), (200, 200)] {
            let power = json!([{
                "index": 0, "id": "Invincible", "class_name": "com.megacrit.cardcrawl.powers.InvinciblePower",
                "type": "BUFF", "amount": amount, "invincible_max_amount": max_amount,
            }]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].amount, amount);
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::Invincible { max_amount }
            );
            for (field, value) in [
                ("invincible_max_amount", Value::Null),
                ("invincible_max_amount", json!(0)),
                ("invincible_max_amount", json!("200")),
                ("amount", json!(-1)),
                ("amount", json!(max_amount + 1)),
                ("class_name", json!("modded.InvinciblePower")),
                ("round_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(200)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
        assert!(require_stateless_power(&json!({"invincible_max_amount": 200})).is_err());
    }

    #[test]
    fn malleable_requires_observed_base_power_independent_of_current_amount() {
        let power = json!([{"index":0,"id":"Malleable","class_name":"com.megacrit.cardcrawl.powers.MalleablePower","type":"BUFF","amount":7,"malleable_base_power":3}]);
        let projected = project_powers_v4(&power, "monster").unwrap();
        assert_eq!(projected[0].amount, 7);
        assert_eq!(
            projected[0].state,
            NaturalCombatPowerStateV4::Malleable { base_power: 3 }
        );
        for invalid in [Value::Null, json!(0), json!(-1), json!("3")] {
            let mut invalid_power = power.clone();
            invalid_power[0]["malleable_base_power"] = invalid;
            assert!(project_powers_v4(&invalid_power, "monster").is_err());
        }
        let mut invalid_power = power;
        invalid_power[0]["flight_stored_amount"] = json!(3);
        assert!(project_powers_v4(&invalid_power, "monster").is_err());
        assert!(require_stateless_power(&json!({"malleable_base_power":3})).is_err());
    }
    use serde_json::json;

    #[test]
    fn flight_requires_observed_stored_amount_and_rejects_cross_power_state() {
        let power = json!([{
            "index": 0, "id": "Flight", "class_name": "com.megacrit.cardcrawl.powers.FlightPower",
            "type": "BUFF", "amount": 2, "flight_stored_amount": 4,
        }]);
        let projected = project_powers_v4(&power, "monster").unwrap();
        assert_eq!(projected[0].amount, 2);
        assert_eq!(
            projected[0].state,
            NaturalCombatPowerStateV4::Flight { stored_amount: 4 }
        );
        for invalid in [Value::Null, json!(0), json!(-1), json!("4")] {
            let mut invalid_power = power.clone();
            invalid_power[0]["flight_stored_amount"] = invalid;
            assert!(project_powers_v4(&invalid_power, "monster").is_err());
        }
        let mut invalid_power = power;
        invalid_power[0]["ritual_skip_first"] = json!(false);
        assert!(project_powers_v4(&invalid_power, "monster").is_err());
        assert!(require_stateless_power(&json!({"flight_stored_amount":4})).is_err());
    }

    #[test]
    fn java_vigor_projection_requires_the_original_positive_stateless_power() {
        let vigor = json!([{
            "index": 0,
            "id": "Vigor",
            "class_name": "com.megacrit.cardcrawl.powers.watcher.VigorPower",
            "type": "BUFF",
            "amount": 8,
            "round_duration_just_applied": null,
            "ritual_skip_first": null,
            "ritual_player_controlled": null,
        }]);

        let projected = project_powers_v4(&vigor, "player").unwrap();
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].power_id, "vigor");
        assert_eq!(projected[0].amount, 8);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);

        let mut wrong_class = vigor;
        wrong_class[0]["class_name"] = json!("modded.VigorPower");
        assert!(project_powers_v4(&wrong_class, "player").is_err());
    }

    #[test]
    fn plated_armor_requires_positive_amount_and_original_class_without_private_state() {
        let power = json!([{
            "index": 0, "id": "Plated Armor", "amount": 14, "type": "BUFF",
            "class_name": "com.megacrit.cardcrawl.powers.PlatedArmorPower",
        }]);
        let projected = project_powers_v4(&power, "monster").unwrap();
        assert_eq!(projected[0].power_id, "platedarmor");
        assert_eq!(projected[0].amount, 14);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            (
                "class_name",
                json!("com.megacrit.cardcrawl.powers.MetallicizePower"),
            ),
            ("flight_stored_amount", json!(14)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "monster").is_err());
        }
    }

    #[test]
    fn reactive_requires_original_compulsive_id_sentinel_and_no_private_state() {
        let power = json!([{
            "index": 0, "id": "Compulsive", "amount": -1, "type": "BUFF",
            "class_name": "com.megacrit.cardcrawl.powers.ReactivePower",
        }]);
        let projected = project_powers_v4(&power, "monster").unwrap();
        assert_eq!(projected[0].power_id, "reactive");
        assert_eq!(projected[0].amount, -1);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("id", json!("Reactive")),
            ("amount", json!(0)),
            ("amount", json!(1)),
            ("type", json!("DEBUFF")),
            ("class_name", json!("modded.ReactivePower")),
            ("malleable_base_power", json!(3)),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "monster").is_err());
        }
    }

    #[test]
    fn juggernaut_preserves_positive_damage_and_rejects_private_state() {
        let power = json!([{
            "index": 0, "id": "Juggernaut", "amount": 5, "type": "BUFF",
            "class_name": "com.megacrit.cardcrawl.powers.JuggernautPower",
        }]);
        for amount in [5, 7, 12] {
            let mut stacked = power.clone();
            stacked[0]["amount"] = json!(amount);
            let projected = project_powers_v4(&stacked, "player").unwrap();
            assert_eq!(projected[0].power_id, "juggernaut");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(-1)),
            ("type", json!("DEBUFF")),
            ("class_name", json!("modded.JuggernautPower")),
            ("flight_stored_amount", json!(5)),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn berserk_preserves_energy_stacks_without_private_state() {
        let power = json!([{
            "index": 0, "id": "Berserk", "amount": 2, "type": "BUFF",
            "class_name": "com.megacrit.cardcrawl.powers.BerserkPower",
        }]);
        let projected = project_powers_v4(&power, "player").unwrap();
        assert_eq!(projected[0].power_id, "berserk");
        assert_eq!(projected[0].amount, 2);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(-1)),
            ("type", json!("DEBUFF")),
            ("class_name", json!("modded.BerserkPower")),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn focus_requires_original_class_signed_type_and_no_private_state() {
        for amount in [-3, -1, 0, 1, 2, 999] {
            let power = json!([{
                "index":0,"id":"Focus","amount":amount,"type":if amount>=0 {"BUFF"} else {"DEBUFF"},
                "class_name":"com.megacrit.cardcrawl.powers.FocusPower"
            }]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].power_id, "focus");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("class_name", json!("modded.FocusPower")),
                ("type", json!(if amount >= 0 { "DEBUFF" } else { "BUFF" })),
                ("round_duration_just_applied", json!(false)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err());
            }
        }
    }

    #[test]
    fn demon_form_preserves_post_draw_strength_stacks_and_original_class() {
        for amount in [2, 3, 5] {
            let power = json!([{
                "index": 0, "id": "Demon Form", "amount": amount, "type": "BUFF",
                "class_name": "com.megacrit.cardcrawl.powers.DemonFormPower",
            }]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].power_id, "demonform");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(-1)),
                ("type", json!("DEBUFF")),
                ("class_name", json!("modded.DemonFormPower")),
                ("round_duration_just_applied", json!(false)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err());
            }
        }
    }

    #[test]
    fn minion_requires_original_sentinel_class_and_no_private_state() {
        let power = json!([{
            "index": 0, "id": "Minion", "amount": -1, "type": "BUFF",
            "class_name": "com.megacrit.cardcrawl.powers.MinionPower",
        }]);
        let projected = project_powers_v4(&power, "monster").unwrap();
        assert_eq!(projected[0].power_id, "minion");
        assert_eq!(projected[0].amount, -1);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(1)),
            ("type", json!("DEBUFF")),
            ("class_name", json!("modded.MinionPower")),
            ("flight_stored_amount", json!(-1)),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "monster").is_err());
        }
    }

    #[test]
    fn regrow_requires_original_life_link_identity_and_sentinel() {
        let power = json!([{
            "index": 0, "id": "Life Link", "amount": -1, "type": "BUFF",
            "class_name": "com.megacrit.cardcrawl.powers.RegrowPower",
        }]);
        let projected = project_powers_v4(&power, "monster").unwrap();
        assert_eq!(projected[0].power_id, "regrow");
        assert_eq!(projected[0].amount, -1);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(1)),
            ("class_name", json!("modded.RegrowPower")),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "monster").is_err());
        }
    }

    #[test]
    fn curl_up_choice_uses_observed_queue_drain_or_requires_its_private_flag() {
        let mut observation =
            crate::observed_regression_fixtures::observation("reduced-flight.json");
        observation.frame["boundary"]["kind"] = json!("run_decision");
        observation.frame["boundary"]["evidence"]["actions_pending"] = json!(0);
        observation.frame["boundary"]["evidence"]["card_queue_pending"] = json!(0);
        observation.frame["parity_internal"]["monsters"][0]["powers"] = json!([{
            "index":0,"id":"Curl Up","class_name":"com.megacrit.cardcrawl.powers.CurlUpPower",
            "amount":11,"type":"BUFF",
        }]);
        assert_eq!(
            project_monsters_v4(&observation, false).unwrap()[0].powers[0].state,
            NaturalCombatPowerStateV4::Stateless
        );
        observation.frame["boundary"]["evidence"]["actions_pending"] = json!(3);
        assert!(project_monsters_v4(&observation, false).is_err());
        for triggered in [false, true] {
            observation.frame["parity_internal"]["monsters"][0]["powers"][0]["curl_up_triggered"] =
                json!(triggered);
            assert_eq!(
                project_monsters_v4(&observation, false).unwrap()[0].powers[0].state,
                if triggered {
                    NaturalCombatPowerStateV4::CurlUpTriggered
                } else {
                    NaturalCombatPowerStateV4::Stateless
                }
            );
        }
        observation.frame["boundary"]["evidence"]["actions_pending"] = json!(0);
        assert!(
            project_monsters_v4(&observation, false).is_err(),
            "a triggered Curl Up always retains queued removal"
        );
        observation.frame["parity_internal"]["monsters"][0]["powers"][0]
            .as_object_mut()
            .unwrap()
            .remove("curl_up_triggered");
        observation.frame["boundary"]["evidence"]["card_queue_pending"] = json!(1);
        assert!(project_monsters_v4(&observation, false).is_err());
    }

    #[test]
    fn curl_up_retains_observed_trigger_and_rejects_foreign_state() {
        for triggered in [false, true] {
            let power = serde_json::json!([{
                "index":0,"id":"Curl Up","class_name":"com.megacrit.cardcrawl.powers.CurlUpPower",
                "amount":11,"type":"BUFF","curl_up_triggered":triggered,
            }]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].amount, 11);
            assert_eq!(
                projected[0].state,
                if triggered {
                    NaturalCombatPowerStateV4::CurlUpTriggered
                } else {
                    NaturalCombatPowerStateV4::Stateless
                }
            );
            for (field, value) in [
                ("amount", serde_json::json!(0)),
                ("curl_up_triggered", serde_json::json!(1)),
                ("class_name", serde_json::json!("modded.CurlUpPower")),
                ("type", serde_json::json!("DEBUFF")),
                ("flight_stored_amount", serde_json::json!(11)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
        let foreign = serde_json::json!([{"index":0,"id":"Strength","class_name":"com.megacrit.cardcrawl.powers.StrengthPower","type":"BUFF","amount":1,"curl_up_triggered":false}]);
        assert!(project_powers_v4(&foreign, "monster").is_err());
    }

    #[test]
    fn stateless_powers_require_original_class_amount_and_no_private_state() {
        for (id, class, amount, invalid_amount) in [
            ("Buffer", "BufferPower", 1, -1),
            ("Buffer", "BufferPower", 2, 0),
            ("Barricade", "BarricadePower", -1, 1),
            ("Split", "SplitPower", -1, 35),
            ("Thorns", "ThornsPower", 7, -1),
            ("Flame Barrier", "FlameBarrierPower", 4, -1),
            ("Rage", "RagePower", 3, -1),
            ("Rage", "RagePower", 8, -1),
            ("Fire Breathing", "FireBreathingPower", 6, -1),
            ("IntangiblePlayer", "IntangiblePlayerPower", 3, -1),
            ("BeatOfDeath", "BeatOfDeathPower", 2, -1),
            ("Explosive", "ExplosivePower", 2, -1),
            ("Fading", "FadingPower", 6, -1),
            ("Shifting", "ShiftingPower", -1, 1),
            ("Evolve", "EvolvePower", 2, -1),
            ("Curiosity", "CuriosityPower", 2, -1),
            ("Unawakened", "UnawakenedPower", -1, 1),
            ("Surrounded", "SurroundedPower", -1, 1),
            ("BackAttack", "BackAttackPower", -1, 1),
            ("Dark Embrace", "DarkEmbracePower", 2, -1),
            ("Rupture", "RupturePower", 2, -1),
            ("Repair", "RepairPower", 7, -1),
            ("Repair", "RepairPower", 10, -1),
            ("Energized", "EnergizedPower", 2, -1),
            ("EnergizedBlue", "EnergizedBluePower", 1, -1),
            ("Draw Card", "DrawCardNextTurnPower", 3, -1),
            ("After Image", "AfterImagePower", 1, -1),
            ("Blur", "BlurPower", 1, -1),
            ("Blur", "BlurPower", 2, 0),
            ("Painful Stabs", "PainfulStabsPower", -1, 1),
            ("Noxious Fumes", "NoxiousFumesPower", 2, -1),
            ("Noxious Fumes", "NoxiousFumesPower", 3, -1),
            ("Envenom", "EnvenomPower", 1, -1),
            ("Envenom", "EnvenomPower", 2, -1),
            ("Tools Of The Trade", "ToolsOfTheTradePower", 1, -1),
            ("Tools Of The Trade", "ToolsOfTheTradePower", 2, -1),
        ] {
            let power = json!([{
                "index": 0, "id": id, "amount": amount, "type": "BUFF",
                "class_name": format!("com.megacrit.cardcrawl.powers.{class}"),
            }]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].power_id, canonical_game_id(id));
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(invalid_amount)),
                ("type", json!("DEBUFF")),
                ("class_name", json!(format!("modded.{class}"))),
                ("flight_stored_amount", json!(1)),
                ("round_duration_just_applied", json!(false)),
                ("turn_duration_just_applied", json!(false)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
    }

    #[test]
    fn free_once_is_independent_of_turn_cost_and_is_spent_on_limbo_cards() {
        let card = json!({"instance_uuid":"setup-target", "card_id":"Wraith Form v2",
            "upgrades":0, "combat_cost":3, "cost_for_turn":3, "free_to_play_once":true});
        let bindings = BTreeMap::from([("setup-target".into(), "bound-wraith".into())]);
        let projected = project_card_v4(&card, &bindings, &mut BTreeSet::new()).unwrap();
        assert!(projected.free_to_play_once);
        assert_eq!(projected.cost_for_turn, 3);
        let zones = json!({"limbo":{"ordering":"java_group_index_ascending", "top":"last", "cards":[card.clone()]}});
        assert!(
            !project_zone(&zones, "limbo", &bindings, &mut BTreeSet::new()).unwrap()[0]
                .free_to_play_once
        );
        let mut malformed = card;
        malformed["free_to_play_once"] = json!("true");
        assert!(project_card_v4(&malformed, &bindings, &mut BTreeSet::new()).is_err());
    }

    #[test]
    fn corpse_explosion_preserves_stacks_and_original_debuff_identity() {
        for amount in [1, 2, 4] {
            let power = json!([{"index":0, "id":"CorpseExplosionPower", "amount":amount,
                "type":"DEBUFF", "class_name":"com.megacrit.cardcrawl.powers.CorpseExplosionPower"}]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].power_id, "corpseexplosion");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("id", json!("CorpseExplosion")),
                ("amount", json!(0)),
                ("amount", json!(-1)),
                ("type", json!("BUFF")),
                ("class_name", json!("modded.CorpseExplosionPower")),
                ("round_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(1)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
    }

    #[test]
    fn wraith_form_preserves_signed_rate_and_original_v2_debuff_identity() {
        for rate in [-2, -1, 1] {
            let power = json!([{"index":0, "id":"Wraith Form v2", "amount":rate, "type":"DEBUFF",
                "class_name":"com.megacrit.cardcrawl.powers.WraithFormPower"}]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].power_id, "wraithform");
            assert_eq!(projected[0].amount, rate);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("id", json!("WraithForm")),
                ("amount", json!(0)),
                ("type", json!("BUFF")),
                ("class_name", json!("modded.WraithFormPower")),
                ("round_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(1)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err());
            }
        }
    }

    #[test]
    fn bias_requires_original_debuff_identity_and_positive_loss_rate() {
        let power = json!([{
            "index":0, "id":"Bias", "amount":1, "type":"DEBUFF",
            "class_name":"com.megacrit.cardcrawl.powers.BiasPower"
        }]);
        let projected = project_powers_v4(&power, "player").unwrap();
        assert_eq!(projected[0].power_id, "bias");
        assert_eq!(projected[0].amount, 1);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(-1)),
            ("type", json!("BUFF")),
            ("class_name", json!("modded.BiasPower")),
            ("round_duration_just_applied", json!(false)),
            ("flight_stored_amount", json!(1)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn dexterity_loss_requires_original_debuff_identity_and_pending_amount() {
        for amount in [1, 5, 10, 15] {
            let power = json!([{
                "index": 0, "id": "DexLoss", "amount": amount, "type": "DEBUFF",
                "class_name": "com.megacrit.cardcrawl.powers.LoseDexterityPower",
            }]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].power_id, "dexteritydown");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(-1)),
                ("id", json!("DexterityDown")),
                ("type", json!("BUFF")),
                ("class_name", json!("modded.LoseDexterityPower")),
                ("round_duration_just_applied", json!(false)),
                ("turn_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(5)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err(), "{field}");
            }
        }
    }

    #[test]
    fn lose_strength_requires_original_flex_debuff_identity_and_pending_amount() {
        for amount in [2, 3, 4, 5, 8] {
            let power = json!([{
                "index": 0, "id": "Flex", "amount": amount, "type": "DEBUFF",
                "class_name": "com.megacrit.cardcrawl.powers.LoseStrengthPower",
            }]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].power_id, "losestrength");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(-1)),
                ("id", json!("LoseStrength")),
                ("type", json!("BUFF")),
                ("class_name", json!("modded.LoseStrengthPower")),
                ("round_duration_just_applied", json!(false)),
                ("turn_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(4)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err(), "{field}");
            }
        }
    }

    #[test]
    fn hex_requires_positive_count_original_debuff_class_and_no_private_state() {
        for amount in [1, 2] {
            let power = json!([{
                "index": 0, "id": "Hex", "amount": amount, "type": "DEBUFF",
                "class_name": "com.megacrit.cardcrawl.powers.HexPower",
            }]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].power_id, "hex");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(-1)),
                ("type", json!("BUFF")),
                ("class_name", json!("modded.HexPower")),
                ("round_duration_just_applied", json!(false)),
                ("turn_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(1)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err());
            }
        }
    }

    #[test]
    fn orb_walker_strength_gain_preserves_original_id_class_and_positive_amount() {
        for amount in [3, 5] {
            let power = json!([{
                "index": 0, "id": "Generic Strength Up Power", "amount": amount, "type": "BUFF",
                "class_name": "com.megacrit.cardcrawl.powers.GenericStrengthUpPower",
            }]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].power_id, "genericstrengthup");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(-1)),
                ("type", json!("DEBUFF")),
                ("class_name", json!("modded.GenericStrengthUpPower")),
                ("round_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(5)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
    }

    #[test]
    fn monster_intangible_requires_observed_turn_expiration_flag() {
        for just_applied in [false, true] {
            let power = json!([{
                "index": 0, "id": "Intangible", "amount": 1, "type": "BUFF",
                "class_name": "com.megacrit.cardcrawl.powers.IntangiblePower",
                "turn_duration_just_applied": just_applied,
            }]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].power_id, "intangible");
            assert_eq!(projected[0].amount, 1);
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::TurnDuration { just_applied }
            );
            for (field, value) in [
                ("amount", json!(0)),
                ("type", json!("DEBUFF")),
                ("class_name", json!("modded.IntangiblePower")),
                ("turn_duration_just_applied", json!(null)),
                ("round_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(1)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
    }

    #[test]
    fn combust_requires_observed_hp_loss_independent_of_damage() {
        for (amount, hp_loss) in [(5, 1), (7, 1), (12, 2), (12, 3)] {
            let power = json!([{
                "index": 0, "id": "Combust", "amount": amount, "type": "BUFF",
                "class_name": "com.megacrit.cardcrawl.powers.CombustPower",
                "combust_hp_loss": hp_loss,
            }]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].amount, amount);
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::Combust { hp_loss }
            );
            for (field, value) in [
                ("combust_hp_loss", Value::Null),
                ("combust_hp_loss", json!(0)),
                ("combust_hp_loss", json!(-1)),
                ("amount", json!(0)),
                ("class_name", json!("modded.CombustPower")),
                ("type", json!("DEBUFF")),
                ("round_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(1)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err());
            }
        }
        let unrelated = json!([{
            "index": 0, "id": "Strength", "amount": 2, "type": "BUFF",
            "class_name": "com.megacrit.cardcrawl.powers.StrengthPower", "combust_hp_loss": 1,
        }]);
        assert!(project_powers_v4(&unrelated, "player").is_err());
    }

    #[test]
    fn slow_keeps_zero_count_and_requires_original_debuff_class() {
        for amount in [0, 1, 12] {
            let power = json!([{
                "index": 0, "id": "Slow", "amount": amount, "type": "DEBUFF",
                "class_name": "com.megacrit.cardcrawl.powers.SlowPower",
            }]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].power_id, "slow");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(-1)),
                ("type", json!("BUFF")),
                ("class_name", json!("modded.SlowPower")),
                ("round_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(1)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
    }

    #[test]
    fn time_warp_keeps_zero_and_requires_the_original_counter_contract() {
        for amount in [0, 1, 11] {
            let power = json!([{
                "index": 0, "id": "Time Warp", "amount": amount, "type": "BUFF",
                "class_name": "com.megacrit.cardcrawl.powers.TimeWarpPower",
            }]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].power_id, "timewarp");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(-1)),
                ("amount", json!(12)),
                ("type", json!("DEBUFF")),
                ("class_name", json!("modded.TimeWarpPower")),
                ("round_duration_just_applied", json!(false)),
                ("flight_stored_amount", json!(1)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
    }

    #[test]
    fn draw_reduction_requires_its_observed_round_expiration_flag() {
        for just_applied in [false, true] {
            let power = json!([{
                "index": 0, "id": "Draw Reduction", "amount": 1, "type": "DEBUFF",
                "class_name": "com.megacrit.cardcrawl.powers.DrawReductionPower",
                "round_duration_just_applied": just_applied,
            }]);
            let projected = project_powers_v4(&power, "player").unwrap();
            assert_eq!(projected[0].power_id, "drawreduction");
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::RoundDuration { just_applied }
            );
            for (field, value) in [
                ("amount", json!(0)),
                ("type", json!("BUFF")),
                ("class_name", json!("modded.DrawReductionPower")),
                ("round_duration_just_applied", Value::Null),
                ("turn_duration_just_applied", json!(false)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "player").is_err());
            }
        }
    }

    #[test]
    fn confusion_requires_the_original_debuff_and_no_private_state() {
        let power = json!([{
            "index": 0, "id": "Confusion", "amount": -1, "type": "DEBUFF",
            "class_name": "com.megacrit.cardcrawl.powers.ConfusionPower",
        }]);
        let projected = project_powers_v4(&power, "player").unwrap();
        assert_eq!(projected[0].power_id, "confusion");
        assert_eq!(projected[0].amount, -1);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(1)),
            ("type", json!("BUFF")),
            ("class_name", json!("modded.ConfusionPower")),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn no_draw_requires_original_debuff_sentinel_and_no_private_state() {
        let power = json!([{
            "index": 0, "id": "No Draw", "amount": -1, "type": "DEBUFF",
            "class_name": "com.megacrit.cardcrawl.powers.NoDrawPower",
        }]);
        let projected = project_powers_v4(&power, "player").unwrap();
        assert_eq!(projected[0].power_id, "nodraw");
        assert_eq!(projected[0].amount, -1);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (field, value) in [
            ("amount", json!(0)),
            ("amount", json!(1)),
            ("type", json!("BUFF")),
            ("class_name", json!("modded.NoDrawPower")),
            ("round_duration_just_applied", json!(false)),
        ] {
            let mut invalid = power.clone();
            invalid[0][field] = value;
            assert!(project_powers_v4(&invalid, "player").is_err());
        }
    }

    #[test]
    fn shackled_keeps_signed_restoration_amount_and_original_debuff_class() {
        for amount in [-999, -53, 53, 999] {
            let power = json!([{
                "index": 0, "id": "Shackled", "amount": amount, "type": "DEBUFF",
                "class_name": "com.megacrit.cardcrawl.powers.GainStrengthPower",
            }]);
            let projected = project_powers_v4(&power, "monster").unwrap();
            assert_eq!(projected[0].power_id, "shackled");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("amount", json!(1000)),
                ("amount", json!(-1000)),
                ("type", json!("BUFF")),
                ("class_name", json!("modded.GainStrengthPower")),
                ("round_duration_just_applied", json!(false)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "monster").is_err());
            }
        }
    }

    #[test]
    fn pending_exhume_preserves_the_in_use_card_and_temporarily_hidden_exhumes() {
        let in_use = json!({"instance_uuid":"using", "card_id":"Exhume"});
        let held = json!({"instance_uuid":"held", "card_id":"Exhume"});
        let zones = json!({"limbo":{"cards":[]},"exhaust":{"cards":[{"instance_uuid":"eligible","card_id":"Impervious"}]},
            "card_in_use":in_use,"exhume_held":{"cards":[held]}});
        let normalized = zones_with_exhume_cards(&zones).unwrap();
        assert_eq!(normalized["limbo"]["cards"], json!([in_use]));
        assert_eq!(normalized["exhaust"]["cards"][1], held);
        assert_eq!(zones["exhaust"]["cards"].as_array().unwrap().len(), 1);
        let mut invalid = zones.clone();
        invalid["exhume_held"] = Value::Null;
        assert!(zones_with_exhume_cards(&invalid).is_err());
        let mut invalid = zones;
        invalid["exhume_held"]["cards"][0]["card_id"] = json!("Impervious");
        assert!(zones_with_exhume_cards(&invalid).is_err());
    }

    #[test]
    fn pending_warcry_observes_the_real_card_in_use_without_inventing_a_new_binding() {
        let card = json!({"instance_uuid":"warcry-uuid", "card_id":"Warcry", "upgrades":0, "combat_cost":0, "cost_for_turn":0});
        let zones = json!({"limbo":{"ordering":"java_group_index_ascending", "top":"last", "cards":[]}, "card_in_use":card});
        let pending = zones_with_card_in_use(&zones).unwrap();
        let binding = BTreeMap::from([(
            "warcry-uuid".to_owned(),
            "natural_run/combat_birth/F06X00/14/0".to_owned(),
        )]);
        let projected = project_zone(&pending, "limbo", &binding, &mut BTreeSet::new()).unwrap();
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].binding, binding["warcry-uuid"]);
        assert_eq!(projected[0].card_id, "warcry");
        assert_eq!(zones_with_card_in_use(&pending).unwrap(), pending);
        assert!(project_zone(&pending, "limbo", &BTreeMap::new(), &mut BTreeSet::new()).is_err());
        let mut missing = zones.clone();
        missing.as_object_mut().unwrap().remove("card_in_use");
        assert!(zones_with_card_in_use(&missing).is_err());
        let mut conflict = pending;
        conflict["card_in_use"]["cost_for_turn"] = json!(1);
        assert!(zones_with_card_in_use(&conflict).is_err());
    }

    #[test]
    fn regeneration_preserves_distinct_monster_and_player_power_identities() {
        for (id, class, shared_id) in [
            ("Regenerate", "RegenerateMonsterPower", "regenerate"),
            ("Regeneration", "RegenPower", "regeneration"),
        ] {
            let power = json!([{
                "index": 0, "id": id, "amount": 5, "type": "BUFF",
                "class_name": format!("com.megacrit.cardcrawl.powers.{class}"),
            }]);
            let projected = project_powers_v4(&power, "owner").unwrap();
            assert_eq!(projected[0].power_id, shared_id);
            assert_eq!(projected[0].amount, 5);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            for (field, value) in [
                ("amount", json!(0)),
                ("class_name", json!("modded.RegenPower")),
                ("flight_stored_amount", json!(5)),
            ] {
                let mut invalid = power.clone();
                invalid[0][field] = value;
                assert!(project_powers_v4(&invalid, "owner").is_err());
            }
        }
    }

    #[test]
    fn java_exhaust_projection_uses_binding_order_not_runtime_group_order() {
        let zones = json!({
            "exhaust": {
                "ordering": "java_group_index_ascending",
                "top": "last",
                "cards": [
                    {
                        "instance_uuid": "java-dazed-later",
                        "card_id": "Dazed",
                        "upgrades": 0,
                        "combat_cost": -2,
                        "cost_for_turn": -2
                    },
                    {
                        "instance_uuid": "java-dazed-earlier",
                        "card_id": "Dazed",
                        "upgrades": 0,
                        "combat_cost": -2,
                        "cost_for_turn": -2
                    }
                ]
            }
        });
        let bindings = BTreeMap::from([
            (
                "java-dazed-earlier".to_owned(),
                "natural_run/combat_birth/F11X03/3/3".to_owned(),
            ),
            (
                "java-dazed-later".to_owned(),
                "natural_run/combat_birth/F11X03/7/2".to_owned(),
            ),
        ]);
        let mut seen = BTreeSet::new();

        let projected = project_exhaust_zone(&zones, &bindings, &mut seen).unwrap();

        assert_eq!(
            projected
                .iter()
                .map(|card| card.binding.as_str())
                .collect::<Vec<_>>(),
            [
                "natural_run/combat_birth/F11X03/3/3",
                "natural_run/combat_birth/F11X03/7/2",
            ]
        );
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn java_zone_projection_requires_each_live_card_binding_but_allows_consumed_bindings() {
        let zones = json!({
            "hand": {
                "ordering": "java_group_index_ascending",
                "top": "last",
                "cards": [{
                    "instance_uuid": "java-live-strike",
                    "card_id": "Strike_R",
                    "upgrades": 0,
                    "combat_cost": 1,
                    "cost_for_turn": 1
                }]
            }
        });
        let bindings = BTreeMap::from([
            (
                "java-live-strike".to_owned(),
                "natural_run/starter/0".to_owned(),
            ),
            (
                "java-consumed-power".to_owned(),
                "natural_run/card_reward/F08X04/0".to_owned(),
            ),
        ]);
        let mut seen = BTreeSet::new();

        let cards = project_zone(&zones, "hand", &bindings, &mut seen).unwrap();

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].binding, "natural_run/starter/0");
        assert_eq!(seen, BTreeSet::from(["java-live-strike".to_owned()]));

        let missing =
            project_zone(&zones, "hand", &BTreeMap::new(), &mut BTreeSet::new()).unwrap_err();
        assert!(missing.contains("has no source-proven binding"));
    }

    #[test]
    fn java_zone_projection_keeps_combat_and_turn_costs_distinct() {
        let zones = json!({
            "hand": {
                "ordering": "java_group_index_ascending",
                "top": "last",
                "cards": [{
                    "instance_uuid": "java-corrupted-defend",
                    "card_id": "Defend_R",
                    "upgrades": 0,
                    "combat_cost": 0,
                    "cost_for_turn": 2
                }]
            }
        });
        let bindings = BTreeMap::from([(
            "java-corrupted-defend".to_owned(),
            "natural_run/starter/5".to_owned(),
        )]);

        let cards = project_zone(&zones, "hand", &bindings, &mut BTreeSet::new()).unwrap();

        assert_eq!(cards[0].combat_cost, 0);
        assert_eq!(cards[0].cost_for_turn, 2);
    }

    #[test]
    fn java_zone_projection_rejects_one_runtime_card_in_two_zones() {
        let zones = json!({
            "hand": {
                "ordering": "java_group_index_ascending",
                "top": "last",
                "cards": [{
                    "instance_uuid": "java-duplicate",
                    "card_id": "Strike_R",
                    "upgrades": 0,
                    "combat_cost": 1,
                    "cost_for_turn": 1
                }]
            },
            "discard": {
                "ordering": "java_group_index_ascending",
                "top": "last",
                "cards": [{
                    "instance_uuid": "java-duplicate",
                    "card_id": "Strike_R",
                    "upgrades": 0,
                    "combat_cost": 1,
                    "cost_for_turn": 1
                }]
            }
        });
        let bindings = BTreeMap::from([(
            "java-duplicate".to_owned(),
            "natural_run/starter/0".to_owned(),
        )]);
        let mut seen = BTreeSet::new();

        project_zone(&zones, "hand", &bindings, &mut seen).unwrap();
        let error = project_zone(&zones, "discard", &bindings, &mut seen).unwrap_err();

        assert!(error.contains("repeat one runtime card UUID"));
    }
}

pub(super) fn project_zones_v4(
    zones: &Value,
    bindings: &BTreeMap<String, String>,
) -> Result<NaturalCombatZonesV4, String> {
    let mut seen_uuids = BTreeSet::new();
    let mut draw = project_zone(zones, "draw", bindings, &mut seen_uuids)?;
    draw.reverse();
    let hand = project_zone(zones, "hand", bindings, &mut seen_uuids)?;
    let discard = project_zone(zones, "discard", bindings, &mut seen_uuids)?;
    let exhaust = project_exhaust_zone(zones, bindings, &mut seen_uuids)?;
    let limbo = project_zone(zones, "limbo", bindings, &mut seen_uuids)?;

    Ok(NaturalCombatZonesV4 {
        draw_next_first: draw,
        hand,
        discard_bottom_first: discard,
        exhaust_by_binding: exhaust,
        limbo,
        stasis: Vec::new(),
    })
}
