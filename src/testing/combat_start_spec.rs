use serde::{Deserialize, Serialize};

use crate::content::cards::{get_card_definition, upgraded_base_cost_override_for, CardId};
use crate::content::potions::Potion;
use crate::content::relics::RelicState;
use crate::runtime::combat::{CombatCard, CombatState};
use crate::runtime::rng::StsRng;
use crate::sim::combat_start::{
    build_natural_combat_start, encounter_id_from_input, room_type_from_input,
};
use crate::state::core::EngineState;
use crate::state::run::RunState;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatStartSpec {
    pub name: String,
    pub player_class: String,
    pub ascension_level: i32,
    pub encounter_id: String,
    pub room_type: String,
    pub seed: u64,
    pub player_current_hp: i32,
    pub player_max_hp: i32,
    #[serde(default)]
    pub relics: Vec<StartSpecRelicSpec>,
    #[serde(default)]
    pub potions: Vec<String>,
    pub master_deck: Vec<StartSpecCardSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StartSpecCardSpec {
    Simple(String),
    Detailed(StartSpecCardEntry),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartSpecCardEntry {
    pub id: String,
    #[serde(default)]
    pub upgrades: u8,
    #[serde(default)]
    pub cost: Option<i32>,
    #[serde(default)]
    pub misc: Option<i32>,
    #[serde(default = "default_count")]
    pub count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StartSpecRelicSpec {
    Simple(String),
    Detailed(StartSpecRelicEntry),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartSpecRelicEntry {
    pub id: String,
    #[serde(default = "default_relic_counter")]
    pub counter: i32,
}

pub fn compile_combat_start_spec(
    spec: &CombatStartSpec,
) -> Result<(EngineState, CombatState), String> {
    compile_combat_start_spec_with_rng_overrides(spec, spec.seed, None)
}

pub fn compile_combat_start_spec_with_seed(
    spec: &CombatStartSpec,
    seed: u64,
) -> Result<(EngineState, CombatState), String> {
    compile_combat_start_spec_with_rng_overrides(spec, seed, None)
}

pub fn compile_combat_start_spec_with_rng_overrides(
    spec: &CombatStartSpec,
    seed: u64,
    shuffle_seed: Option<u64>,
) -> Result<(EngineState, CombatState), String> {
    compile_combat_start_spec_inner(spec, seed, shuffle_seed)
}

/// Compiles the run-level portion of a start spec without realizing an
/// encounter. Offline run-decision experiments use this to share the same
/// card/relic/potion schema as exact combat fixtures.
pub fn compile_run_state_from_combat_start_spec(
    spec: &CombatStartSpec,
    seed: u64,
) -> Result<RunState, String> {
    compile_run_state_from_combat_start_spec_with_shuffle(spec, seed, None)
}

fn compile_combat_start_spec_inner(
    spec: &CombatStartSpec,
    seed: u64,
    shuffle_seed: Option<u64>,
) -> Result<(EngineState, CombatState), String> {
    let encounter_id = encounter_id_from_spec(&spec.encounter_id)?;
    let room_type = room_type_from_spec(&spec.room_type)?;
    let mut run_state =
        compile_run_state_from_combat_start_spec_with_shuffle(spec, seed, shuffle_seed)?;

    build_natural_combat_start(&mut run_state, encounter_id, room_type)
}

fn compile_run_state_from_combat_start_spec_with_shuffle(
    spec: &CombatStartSpec,
    seed: u64,
    shuffle_seed: Option<u64>,
) -> Result<RunState, String> {
    let player_class = canonical_player_class(&spec.player_class)?;
    let ascension_level = u8::try_from(spec.ascension_level)
        .map_err(|_| format!("ascension_level {} out of u8 range", spec.ascension_level))?;
    let mut run_state = RunState::new(seed, ascension_level, false, player_class);
    if let Some(shuffle_seed) = shuffle_seed {
        run_state.rng_pool.shuffle_rng = StsRng::new(shuffle_seed);
    }
    run_state.current_hp = spec.player_current_hp;
    run_state.max_hp = spec.player_max_hp;
    run_state.master_deck = compile_master_deck(&spec.master_deck)?;
    run_state.relics = compile_relics(&spec.relics)?;
    run_state.potions = compile_potions(&spec.potions, ascension_level)?;
    Ok(run_state)
}

pub fn encounter_id_from_spec(
    raw: &str,
) -> Result<crate::content::monsters::factory::EncounterId, String> {
    encounter_id_from_input(raw)
}

pub fn room_type_from_spec(raw: &str) -> Result<crate::state::map::node::RoomType, String> {
    room_type_from_input(raw)
}

fn canonical_player_class(raw: &str) -> Result<&'static str, String> {
    match normalize_identifier(raw).as_str() {
        "ironclad" => Ok("Ironclad"),
        "silent" => Ok("Silent"),
        "defect" => Ok("Defect"),
        "watcher" => Ok("Watcher"),
        _ => Err(format!("unsupported player_class '{raw}'")),
    }
}

fn compile_master_deck(specs: &[StartSpecCardSpec]) -> Result<Vec<CombatCard>, String> {
    let mut deck = Vec::new();
    let mut next_uuid = 10_000u32;
    for spec in specs {
        let entry = match spec {
            StartSpecCardSpec::Simple(id) => StartSpecCardEntry {
                id: id.clone(),
                upgrades: 0,
                cost: None,
                misc: None,
                count: 1,
            },
            StartSpecCardSpec::Detailed(entry) => entry.clone(),
        };
        let card_id = input_ids::card_id_from_start_spec(&entry.id)
            .ok_or_else(|| format!("unknown card id '{}'", entry.id))?;
        for _ in 0..entry.count.max(1) {
            let mut card = CombatCard::for_scenario(card_id, next_uuid);
            card.upgrades = entry.upgrades;
            card.misc_value = entry.misc.unwrap_or(0);
            if let Some(explicit_cost) = entry.cost {
                let upgraded_base = upgraded_or_base_cost(card.id, card.upgrades);
                if explicit_cost != upgraded_base {
                    return Err(format!(
                        "CombatStartSpec does not support per-card cost overrides yet: {} cost {} != base {}",
                        entry.id, explicit_cost, upgraded_base
                    ));
                }
            }
            deck.push(card);
            next_uuid += 1;
        }
    }
    if deck.is_empty() {
        return Err("CombatStartSpec requires a non-empty master_deck".to_string());
    }
    Ok(deck)
}

fn compile_relics(specs: &[StartSpecRelicSpec]) -> Result<Vec<RelicState>, String> {
    specs
        .iter()
        .map(|spec| match spec {
            StartSpecRelicSpec::Simple(id) => {
                let relic_id = input_ids::relic_id_from_start_spec(id)
                    .ok_or_else(|| format!("unknown relic id '{id}'"))?;
                Ok(RelicState::new(relic_id))
            }
            StartSpecRelicSpec::Detailed(entry) => {
                let relic_id = input_ids::relic_id_from_start_spec(&entry.id)
                    .ok_or_else(|| format!("unknown relic id '{}'", entry.id))?;
                let mut relic = RelicState::new(relic_id);
                relic.counter = entry.counter;
                Ok(relic)
            }
        })
        .collect()
}

fn compile_potions(specs: &[String], ascension_level: u8) -> Result<Vec<Option<Potion>>, String> {
    let slot_count = if ascension_level >= 11 {
        2usize
    } else {
        3usize
    };
    if specs.len() > slot_count {
        return Err(format!(
            "CombatStartSpec requested {} potions but only {slot_count} slots are available",
            specs.len()
        ));
    }
    let mut potions = vec![None; slot_count];
    for (index, id) in specs.iter().enumerate() {
        let potion_id = input_ids::potion_id_from_start_spec(id)
            .ok_or_else(|| format!("unknown potion id '{id}'"))?;
        potions[index] = Some(Potion::new(potion_id, 20_000 + index as u32));
    }
    Ok(potions)
}

fn upgraded_or_base_cost(card_id: CardId, upgrades: u8) -> i32 {
    upgraded_base_cost_override_for(card_id, upgrades)
        .unwrap_or_else(|| get_card_definition(card_id).cost) as i32
}

fn normalize_identifier(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn default_count() -> usize {
    1
}

fn default_relic_counter() -> i32 {
    -1
}

mod input_ids {
    #![allow(dead_code)]

    use crate::content::cards::{self, CardId};
    use crate::content::potions::PotionId;
    use crate::content::relics::RelicId;

    pub(super) fn card_id_from_start_spec(raw: &str) -> Option<CardId> {
        let map = cards::build_java_id_map();
        if let Some(card) = map.get(raw).copied() {
            return Some(card);
        }
        let normalized = normalize_input_alias(raw);
        if normalized.is_empty() {
            return None;
        }
        map.into_iter()
            .find_map(|(java, card)| (normalize_input_alias(java) == normalized).then_some(card))
    }

    pub(super) fn relic_id_from_start_spec(raw: &str) -> Option<RelicId> {
        RelicId::from_input_alias_v1(raw)
    }

    pub(super) fn potion_id_from_start_spec(raw: &str) -> Option<PotionId> {
        PotionId::from_input_alias_v1(raw)
    }

    fn normalize_input_alias(raw: &str) -> String {
        raw.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compile_combat_start_spec, compile_combat_start_spec_with_rng_overrides, CombatStartSpec,
        StartSpecCardEntry, StartSpecCardSpec, StartSpecRelicEntry,
    };
    use serde_json::json;

    #[test]
    fn detailed_card_rejects_unknown_fields() {
        let error = serde_json::from_value::<StartSpecCardEntry>(json!({
            "id": "Bash",
            "unsupported": true
        }))
        .expect_err("unknown detailed card fields must fail preflight");

        assert!(error.to_string().contains("unknown field `unsupported`"));
    }

    #[test]
    fn detailed_relic_rejects_unknown_fields() {
        let error = serde_json::from_value::<StartSpecRelicEntry>(json!({
            "id": "Burning Blood",
            "unsupported": true
        }))
        .expect_err("unknown detailed relic fields must fail preflight");

        assert!(error.to_string().contains("unknown field `unsupported`"));
    }

    #[test]
    fn shuffle_override_changes_only_shuffle_rng_before_natural_start() {
        let spec = deterministic_start_spec();
        let (default_engine, default_combat) =
            compile_combat_start_spec(&spec).expect("default combat start should compile");
        let (overridden_engine, overridden_combat) =
            compile_combat_start_spec_with_rng_overrides(&spec, spec.seed, Some(5_678))
                .expect("overridden combat start should compile");

        assert_eq!(default_engine, overridden_engine, "engine boundary");
        assert_eq!(
            default_combat.entities.monsters.len(),
            overridden_combat.entities.monsters.len()
        );
        for (default_monster, overridden_monster) in default_combat
            .entities
            .monsters
            .iter()
            .zip(&overridden_combat.entities.monsters)
        {
            assert_eq!(
                (default_monster.id, default_monster.enemy_id),
                (overridden_monster.id, overridden_monster.enemy_id),
                "monster identity"
            );
            assert_eq!(
                (default_monster.current_hp, default_monster.max_hp),
                (overridden_monster.current_hp, overridden_monster.max_hp),
                "monster HP"
            );
            assert_eq!(
                default_monster.turn_plan(),
                overridden_monster.turn_plan(),
                "initial monster intention"
            );
        }
        assert_eq!(
            default_combat.entities.player, overridden_combat.entities.player,
            "player resources"
        );
        assert_eq!(
            default_combat.entities.potions, overridden_combat.entities.potions,
            "player potions"
        );
        assert_eq!(
            default_combat.turn.energy, overridden_combat.turn.energy,
            "player energy"
        );

        let default_rngs = default_combat.rng.pool.clone();
        let mut overridden_rngs = overridden_combat.rng.pool.clone();
        assert_ne!(default_rngs.shuffle_rng, overridden_rngs.shuffle_rng);
        overridden_rngs.shuffle_rng = default_rngs.shuffle_rng.clone();
        assert_eq!(
            default_rngs, overridden_rngs,
            "every RNG stream except shuffle_rng"
        );
        assert!(
            default_combat.zones.hand != overridden_combat.zones.hand
                || default_combat.zones.draw_pile != overridden_combat.zones.draw_pile,
            "shuffle override should change the opening hand or draw-pile order"
        );
    }

    #[test]
    fn same_shuffle_override_reproduces_identical_start() {
        let spec = deterministic_start_spec();
        let first = compile_combat_start_spec_with_rng_overrides(&spec, spec.seed, Some(5_678))
            .expect("first overridden combat start should compile");
        let second = compile_combat_start_spec_with_rng_overrides(&spec, spec.seed, Some(5_678))
            .expect("second overridden combat start should compile");

        assert_eq!(first, second);
    }

    fn deterministic_start_spec() -> CombatStartSpec {
        CombatStartSpec {
            name: "shuffle_override_isolation".to_string(),
            player_class: "Ironclad".to_string(),
            ascension_level: 0,
            encounter_id: "JawWorm".to_string(),
            room_type: "monster".to_string(),
            seed: 1_234,
            player_current_hp: 72,
            player_max_hp: 80,
            relics: Vec::new(),
            potions: Vec::new(),
            master_deck: vec![
                counted_card("Strike_R", 5),
                counted_card("Defend_R", 4),
                StartSpecCardSpec::Simple("Bash".to_string()),
            ],
        }
    }

    fn counted_card(id: &str, count: usize) -> StartSpecCardSpec {
        StartSpecCardSpec::Detailed(StartSpecCardEntry {
            id: id.to_string(),
            upgrades: 0,
            cost: None,
            misc: None,
            count,
        })
    }
}
