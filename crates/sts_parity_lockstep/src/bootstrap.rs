use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::ParityDomainV1;

pub const NATURAL_RUN_BOOTSTRAP_SCHEMA_V1: &str = "sts-parity-natural-run-bootstrap-v1";
pub const NATURAL_RUN_SNAPSHOT_CONTRACT_SCHEMA_V3: &str =
    "sts-parity-natural-run-boundary-contract-v3";
pub const NATURAL_RUN_STATE_SCHEMA_V2: &str = "sts-parity-natural-run-state-v2";
pub const NATURAL_RUN_MAP_SCHEMA_V1: &str = "sts-parity-public-map-adjacency-v1";
pub const NATURAL_RUN_MAP_SCHEMA_V2: &str = "sts-parity-active-map-adjacency-v2";
pub const NATURAL_RUN_ROOM_SCHEMA_V3: &str = "sts-parity-natural-room-v3";
pub const NATURAL_RUN_MASTER_DECK_SCHEMA_V2: &str = "sts-parity-bound-master-deck-v2";
pub const NATURAL_RUN_RELICS_SCHEMA_V3: &str = "sts-parity-relic-state-v3";
pub const NATURAL_RUN_POTIONS_SCHEMA_V1: &str = "sts-parity-persistent-potions-v1";
pub const NATURAL_RUN_RNG_SCHEMA_V4: &str = "sts-parity-active-rng-streams-v4";
pub const NATURAL_RUN_CARD_POOLS_SCHEMA_V1: &str = "sts-parity-card-pools-v1";
pub const NATURAL_RUN_CARD_BINDINGS_SCHEMA_V3: &str = "sts-parity-card-binding-relation-v3";
pub const NATURAL_RUN_SETTLEMENT_SCHEMA_V1: &str = "sts-parity-runtime-settlement-v1";
pub const NATURAL_RUN_COMBAT_SCHEMA_V4: &str = "sts-parity-natural-combat-decision-v4";
pub const NATURAL_RUN_REWARDS_SCHEMA_V2: &str = "sts-parity-natural-run-rewards-v2";
pub const NATURAL_RUN_SNAPSHOT_DOMAINS_V3: [ParityDomainV1; 13] = [
    ParityDomainV1::Run,
    ParityDomainV1::Map,
    ParityDomainV1::Room,
    ParityDomainV1::ActionSpace,
    ParityDomainV1::MasterDeck,
    ParityDomainV1::Relics,
    ParityDomainV1::Potions,
    ParityDomainV1::Combat,
    ParityDomainV1::Rewards,
    ParityDomainV1::RngStreams,
    ParityDomainV1::CardPools,
    ParityDomainV1::CardBindings,
    ParityDomainV1::RuntimeSettlement,
];

pub const NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2: [&str; 8] = [
    "run.monster",
    "run.event",
    "run.merchant",
    "run.card",
    "run.treasure",
    "run.relic",
    "run.potion",
    "run.map",
];

pub const NATURAL_RUN_FLOOR_RNG_OWNERS_V2: [&str; 5] = [
    "floor.monster_hp",
    "floor.ai",
    "floor.shuffle",
    "floor.card_random",
    "floor.misc",
];

/// The actual current room, independently extracted by each adapter. Other
/// includes event fights and bosses: both may start another combat in place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NaturalRunRngRoomV3 {
    Monster,
    Elite,
    Other,
}

/// RNG streams whose current state can affect deterministic continuation at a
/// shared natural-run boundary.
///
/// Java creates the floor streams before Neow and may consume `miscRng` for
/// background music. Every floor stream is replaced when a map choice enters
/// the next room, so none of those pre-room values are canonical. Once inside
/// a room, floor streams remain live except at ordinary monster/elite rewards:
/// combat has ended and the next combat requires nextRoomTransition, which
/// replaces them. Keep miscRng: obtaining War Paint/Whetstone can still use it.
/// Event rewards (e.g. Colosseum) and bosses retain every floor stream. Rich
/// Neow's private stream is live only while its reward page is active.
/// After the Emerald Key is acquired, mapRng has no remaining consumer in a
/// natural act: the unique burning elite has been passed, and the next act
/// constructs a new mapRng. Native loading regenerates the map without the
/// key-placement and old elite-buff draws; that obsolete cursor is not saved.
/// This lifecycle never filters the per-transition RNG call ledger.
pub fn natural_run_active_rng_owners_v4(
    boundary_kind: &str,
    neow_rng_active: bool,
    room: NaturalRunRngRoomV3,
    emerald_key_acquired: bool,
) -> Result<Vec<&'static str>, String> {
    let floor_streams_are_active = match boundary_kind {
        "natural_neow"
        | "natural_neow_grid_select"
        | "natural_neow_card_reward"
        | "natural_map" => false,
        "natural_event"
        | "natural_grid_select"
        | "natural_campfire"
        | "natural_campfire_smith"
        | "natural_campfire_smith_confirm"
        | "natural_campfire_complete"
        | "natural_shop"
        | "natural_shop_entry"
        | "natural_shop_purge_card"
        | "natural_shop_purge_confirm"
        | "natural_treasure"
        | "natural_treasure_reward"
        | "natural_boss_relic"
        | "natural_boss_treasure"
        | "natural_boss_treasure_complete"
        | "natural_combat"
        | "natural_hand_select"
        | "natural_combat_discovery"
        | "natural_combat_codex"
        | "natural_combat_grid_select"
        | "natural_combat_reward"
        | "natural_double_boss_complete"
        | "natural_act3_victory_complete"
        | "natural_true_victory_complete"
        | "natural_heart_victory"
        | "natural_card_reward" => true,
        other => {
            return Err(format!(
                "natural-run RNG lifecycle does not recognize boundary {other:?}"
            ))
        }
    };
    let mut owners = NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2.to_vec();
    if emerald_key_acquired {
        owners.retain(|owner| *owner != "run.map");
    }
    if floor_streams_are_active {
        let ordinary_combat_finished = matches!(
            boundary_kind,
            "natural_combat_reward" | "natural_card_reward"
        ) && matches!(
            room,
            NaturalRunRngRoomV3::Monster | NaturalRunRngRoomV3::Elite
        );
        if ordinary_combat_finished {
            owners.push("floor.misc");
        } else {
            owners.extend(NATURAL_RUN_FLOOR_RNG_OWNERS_V2);
        }
    }
    if neow_rng_active {
        if !matches!(boundary_kind, "natural_neow" | "natural_neow_grid_select") {
            return Err("Neow RNG cannot be active outside the Neow boundary".to_owned());
        }
        owners.push("run.neow");
    }
    Ok(owners)
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NaturalRunContentProfileV1 {
    AllUnlocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NaturalRunNeowEligibilityV1 {
    Mini,
    Rich,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalRunCardSourceV1 {
    StarterDeck {
        player_class: String,
        source_slot: u32,
    },
    AscendersBane {
        rule: String,
    },
    NeowDrawback {
        reward_type: String,
        drawback: String,
        product_slot: u32,
    },
    NeowCardReward {
        product_slot: u32,
    },
    CardReward {
        room_id: String,
        product_slot: u32,
    },
    ShopPurchase {
        room_id: String,
        product_slot: u32,
    },
    EventCard {
        room_id: String,
        event_id: String,
        screen: i64,
        offer_index: u32,
    },
    EventGrant {
        room_id: String,
        event_id: String,
        product_slot: u32,
    },
    RelicGrant {
        room_id: String,
        relic_id: String,
        /// Original producer construction order, before confirmation/obtain order.
        product_slot: u32,
    },
    MonsterGrant {
        room_id: String,
        monster_id: String,
        monster_slot: u32,
        product_slot: u32,
    },
    CombatBirth {
        room_id: String,
        action_ordinal: u32,
        cause: NaturalRunCombatCardBirthCauseV1,
        product_slot: u32,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalRunCombatCardBirthCauseV1 {
    CombatEntryResolution,
    CardPlay { producer_binding: String },
    EndTurnResolution,
    DiscoveryResolution,
    CombatSelectionResolution,
    PotionUse { potion_id: String, slot: u8 },
}

pub fn natural_run_card_source_key_v1(source: &NaturalRunCardSourceV1) -> Result<String, String> {
    match source {
        NaturalRunCardSourceV1::RelicGrant {
            room_id,
            relic_id,
            product_slot,
        } if valid_natural_room_id(room_id) && relic_id == "pandorasbox" => {
            Ok(format!("relic_grant:{room_id}:{relic_id}:{product_slot}"))
        }
        NaturalRunCardSourceV1::MonsterGrant {
            room_id,
            monster_id,
            monster_slot,
            product_slot,
        } if valid_natural_room_id(room_id)
            && monster_id == "writhingmass"
            && *monster_slot < 5
            && *product_slot == 0 =>
        {
            Ok(format!(
                "monster_grant:{room_id}:{monster_id}:{monster_slot}:{product_slot}"
            ))
        }
        NaturalRunCardSourceV1::EventGrant {
            room_id,
            event_id,
            product_slot,
        } if valid_natural_room_id(room_id)
            && match event_id.as_str() {
                "ghosts" | "matchandkeep" => *product_slot < 5,
                "windinghalls" => *product_slot < 2,
                "goldenshrine" | "drugdealer" | "addict" | "nest" => *product_slot == 0,
                _ => false,
            } =>
        {
            Ok(format!("event_grant:{room_id}:{event_id}:{product_slot}"))
        }
        NaturalRunCardSourceV1::NeowCardReward { product_slot: 0 } => {
            Ok("neow_card_reward:0".to_owned())
        }
        NaturalRunCardSourceV1::StarterDeck {
            player_class,
            source_slot,
        } if !player_class.is_empty() => Ok(format!("starter:{player_class}:{source_slot}")),
        NaturalRunCardSourceV1::AscendersBane { rule } if rule == "ascension_10" => {
            Ok("ascenders_bane:ascension_10".to_owned())
        }
        NaturalRunCardSourceV1::NeowDrawback {
            reward_type,
            drawback,
            product_slot,
        } if !reward_type.is_empty() && drawback == "curse" && *product_slot == 0 => Ok(format!(
            "neow_drawback:{reward_type}:{drawback}:{product_slot}"
        )),
        NaturalRunCardSourceV1::CardReward {
            room_id,
            product_slot,
        } if valid_natural_room_id(room_id) => Ok(format!("card_reward:{room_id}:{product_slot}")),
        NaturalRunCardSourceV1::ShopPurchase {
            room_id,
            product_slot,
        } if valid_natural_room_id(room_id) => {
            Ok(format!("shop_purchase:{room_id}:{product_slot}"))
        }
        NaturalRunCardSourceV1::EventCard {
            room_id,
            event_id,
            screen,
            offer_index,
        } if valid_natural_room_id(room_id)
            && event_id == "thelibrary"
            && *screen == 1
            && *offer_index < 20 =>
        {
            Ok(format!(
                "event_card:{room_id}:{event_id}:{screen}:{offer_index}"
            ))
        }
        NaturalRunCardSourceV1::CombatBirth {
            room_id,
            action_ordinal,
            cause,
            product_slot,
        } if valid_combat_birth_source(room_id, cause) => Ok(format!(
            "combat_birth:{room_id}:{action_ordinal}:{product_slot}"
        )),
        _ => Err("natural run card source is invalid".to_owned()),
    }
}

pub fn natural_run_card_binding_v1(source: &NaturalRunCardSourceV1) -> Result<String, String> {
    match source {
        NaturalRunCardSourceV1::RelicGrant {
            room_id,
            relic_id,
            product_slot,
        } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!(
                "natural_run/relic_grant/{room_id}/{relic_id}/{product_slot}"
            ))
        }
        NaturalRunCardSourceV1::MonsterGrant {
            room_id,
            monster_id,
            monster_slot,
            product_slot,
        } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!(
                "natural_run/monster_grant/{room_id}/{monster_id}/{monster_slot}/{product_slot}"
            ))
        }
        NaturalRunCardSourceV1::EventGrant {
            room_id,
            event_id,
            product_slot,
        } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!(
                "natural_run/event_grant/{room_id}/{event_id}/{product_slot}"
            ))
        }
        NaturalRunCardSourceV1::EventCard {
            room_id,
            event_id,
            screen,
            offer_index,
        } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!(
                "natural_run/event_card/{room_id}/{event_id}/{screen}/{offer_index}"
            ))
        }
        NaturalRunCardSourceV1::NeowCardReward { product_slot } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!("natural_run/neow_card_reward/{product_slot}"))
        }
        NaturalRunCardSourceV1::StarterDeck { source_slot, .. } => {
            Ok(format!("natural_run/starter/{source_slot}"))
        }
        NaturalRunCardSourceV1::AscendersBane { .. } => {
            Ok("natural_run/ascension_10/ascenders_bane".to_owned())
        }
        NaturalRunCardSourceV1::NeowDrawback {
            reward_type,
            drawback,
            product_slot,
        } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!(
                "natural_run/neow_drawback/{reward_type}/{drawback}/{product_slot}"
            ))
        }
        NaturalRunCardSourceV1::CardReward {
            room_id,
            product_slot,
        } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!("natural_run/card_reward/{room_id}/{product_slot}"))
        }
        NaturalRunCardSourceV1::CombatBirth {
            room_id,
            action_ordinal,
            product_slot,
            ..
        } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!(
                "natural_run/combat_birth/{room_id}/{action_ordinal}/{product_slot}"
            ))
        }
        NaturalRunCardSourceV1::ShopPurchase {
            room_id,
            product_slot,
        } => {
            natural_run_card_source_key_v1(source)?;
            Ok(format!(
                "natural_run/shop_purchase/{room_id}/{product_slot}"
            ))
        }
    }
}

fn valid_combat_birth_source(room_id: &str, cause: &NaturalRunCombatCardBirthCauseV1) -> bool {
    if !valid_natural_room_id(room_id) {
        return false;
    }
    match cause {
        NaturalRunCombatCardBirthCauseV1::CardPlay { producer_binding } => {
            producer_binding.starts_with("natural_run/")
        }
        NaturalRunCombatCardBirthCauseV1::CombatEntryResolution => true,
        NaturalRunCombatCardBirthCauseV1::EndTurnResolution => true,
        NaturalRunCombatCardBirthCauseV1::DiscoveryResolution => true,
        NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution => true,
        NaturalRunCombatCardBirthCauseV1::PotionUse { potion_id, .. } => !potion_id.is_empty(),
    }
}

fn valid_natural_room_id(room_id: &str) -> bool {
    let Some((floor, x)) = room_id
        .strip_prefix('F')
        .and_then(|value| value.split_once('X'))
    else {
        return false;
    };
    let Ok(floor) = floor.parse::<i64>() else {
        return false;
    };
    let Ok(x) = x.parse::<i64>() else {
        return false;
    };
    if floor <= 0 || natural_map_node_id_v1(x, floor - 1).as_deref() != Ok(room_id) {
        return false;
    }
    true
}

pub fn natural_run_neow_drawback_card_source_v1(
    action: &crate::SemanticActionV1,
) -> Result<Option<NaturalRunCardSourceV1>, String> {
    let crate::SemanticActionV1::Select {
        surface: crate::DecisionSurfaceV1::Neow,
        choice,
    } = action
    else {
        return Ok(None);
    };
    if choice.kind != "neow_reward" || choice.occurrence != 0 {
        return Err("Neow reward action has an invalid typed identity".to_owned());
    }
    let Some(crate::SemanticScalarV1::Text(drawback)) = choice.attributes.get("drawback") else {
        return Err("Neow reward action omitted its typed drawback".to_owned());
    };
    if drawback != "curse" {
        return Ok(None);
    }
    let source = NaturalRunCardSourceV1::NeowDrawback {
        reward_type: choice.id.clone(),
        drawback: drawback.clone(),
        product_slot: 0,
    };
    natural_run_card_source_key_v1(&source)?;
    Ok(Some(source))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunBootstrapV1 {
    pub schema: String,
    pub seed: String,
    pub ascension: u8,
    pub final_act: bool,
    pub content_profile: NaturalRunContentProfileV1,
    pub neow_eligibility: NaturalRunNeowEligibilityV1,
    pub player_class: String,
    pub cards: Vec<NaturalRunBootstrapCardV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunBootstrapCardV1 {
    pub binding: String,
    pub source: NaturalRunCardSourceV1,
    pub card_id: String,
    pub upgrades: u8,
}

impl NaturalRunBootstrapV1 {
    pub fn new(
        seed: impl Into<String>,
        ascension: u8,
        final_act: bool,
        content_profile: NaturalRunContentProfileV1,
        neow_eligibility: NaturalRunNeowEligibilityV1,
        player_class: impl Into<String>,
        cards: Vec<NaturalRunBootstrapCardV1>,
    ) -> Result<Self, String> {
        let bootstrap = Self {
            schema: NATURAL_RUN_BOOTSTRAP_SCHEMA_V1.to_owned(),
            seed: seed.into(),
            ascension,
            final_act,
            content_profile,
            neow_eligibility,
            player_class: player_class.into(),
            cards,
        };
        bootstrap.validate()?;
        Ok(bootstrap)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != NATURAL_RUN_BOOTSTRAP_SCHEMA_V1
            || self.seed.is_empty()
            || self.player_class.is_empty()
            || self.cards.is_empty()
        {
            return Err("natural run bootstrap header is invalid".to_owned());
        }

        let bindings = self
            .cards
            .iter()
            .map(|card| card.binding.as_str())
            .collect::<BTreeSet<_>>();
        if bindings.len() != self.cards.len() {
            return Err("natural run bootstrap repeats a binding".to_owned());
        }

        let starter_slots = self
            .cards
            .iter()
            .filter_map(|card| match &card.source {
                NaturalRunCardSourceV1::StarterDeck { source_slot, .. } => Some(*source_slot),
                NaturalRunCardSourceV1::AscendersBane { .. }
                | NaturalRunCardSourceV1::NeowDrawback { .. }
                | NaturalRunCardSourceV1::NeowCardReward { .. }
                | NaturalRunCardSourceV1::CardReward { .. }
                | NaturalRunCardSourceV1::ShopPurchase { .. }
                | NaturalRunCardSourceV1::EventCard { .. }
                | NaturalRunCardSourceV1::EventGrant { .. }
                | NaturalRunCardSourceV1::RelicGrant { .. }
                | NaturalRunCardSourceV1::MonsterGrant { .. }
                | NaturalRunCardSourceV1::CombatBirth { .. } => None,
            })
            .collect::<BTreeSet<_>>();
        let starter_entries = self
            .cards
            .iter()
            .filter(|card| matches!(card.source, NaturalRunCardSourceV1::StarterDeck { .. }))
            .count();
        if starter_entries == 0 || starter_slots.len() != starter_entries {
            return Err("natural run bootstrap repeats or omits a starter source".to_owned());
        }
        let starter_count = u32::try_from(starter_slots.len())
            .map_err(|_| "natural run starter deck is too large".to_owned())?;
        if starter_slots.iter().copied().ne(0..starter_count) {
            return Err("natural run starter source slots are not contiguous".to_owned());
        }

        let ascenders_bane_count = self
            .cards
            .iter()
            .filter(|card| {
                matches!(
                    &card.source,
                    NaturalRunCardSourceV1::AscendersBane { rule }
                        if rule == "ascension_10"
                )
            })
            .count();
        if (self.ascension >= 10 && ascenders_bane_count != 1)
            || (self.ascension < 10 && ascenders_bane_count != 0)
        {
            return Err("natural run Ascender's Bane source disagrees with ascension".to_owned());
        }

        if self.cards.iter().any(|card| {
            card.binding.is_empty()
                || card.card_id.is_empty()
                || matches!(
                    &card.source,
                    NaturalRunCardSourceV1::StarterDeck { player_class, .. }
                        if player_class != &self.player_class
                )
                || matches!(
                    &card.source,
                    NaturalRunCardSourceV1::AscendersBane { rule }
                        if rule != "ascension_10"
                )
                || matches!(
                    card.source,
                    NaturalRunCardSourceV1::NeowDrawback { .. }
                        | NaturalRunCardSourceV1::NeowCardReward { .. }
                        | NaturalRunCardSourceV1::CardReward { .. }
                        | NaturalRunCardSourceV1::ShopPurchase { .. }
                        | NaturalRunCardSourceV1::EventCard { .. }
                        | NaturalRunCardSourceV1::EventGrant { .. }
                        | NaturalRunCardSourceV1::RelicGrant { .. }
                        | NaturalRunCardSourceV1::MonsterGrant { .. }
                        | NaturalRunCardSourceV1::CombatBirth { .. }
                )
        }) {
            return Err("natural run bootstrap contains an invalid card source".to_owned());
        }
        Ok(())
    }
}

pub fn natural_map_node_id_v1(x: i64, y: i64) -> Result<String, String> {
    if x < 0 || y < 0 {
        return Err("map node identity cannot encode a negative coordinate".to_owned());
    }
    Ok(format!("F{:02}X{:02}", y + 1, x))
}

#[cfg(test)]
mod event_source_tests {
    use super::*;

    #[test]
    fn pandora_grants_have_distinct_producer_slots_and_a_closed_relic_owner() {
        let source = |relic: &str, room: &str, slot| NaturalRunCardSourceV1::RelicGrant {
            room_id: room.into(),
            relic_id: relic.into(),
            product_slot: slot,
        };
        assert_eq!(
            natural_run_card_binding_v1(&source("pandorasbox", "F16X00", 2)).unwrap(),
            "natural_run/relic_grant/F16X00/pandorasbox/2"
        );
        assert_ne!(
            natural_run_card_binding_v1(&source("pandorasbox", "F16X00", 0)).unwrap(),
            natural_run_card_binding_v1(&source("pandorasbox", "F16X00", 1)).unwrap()
        );
        assert!(natural_run_card_binding_v1(&source("astrolabe", "F16X00", 0)).is_err());
        assert!(natural_run_card_binding_v1(&source("pandorasbox", "bad", 0)).is_err());
    }

    #[test]
    fn single_event_grants_accept_only_their_canonical_product() {
        let source = |event: &str, slot| NaturalRunCardSourceV1::EventGrant {
            room_id: "F05X02".into(),
            event_id: event.into(),
            product_slot: slot,
        };
        for event in ["addict", "nest"] {
            assert_eq!(
                natural_run_card_binding_v1(&source(event, 0)).unwrap(),
                format!("natural_run/event_grant/F05X02/{event}/0")
            );
            assert!(natural_run_card_binding_v1(&source(event, 1)).is_err());
        }
        assert!(natural_run_card_binding_v1(&source("Addict", 0)).is_err());
        assert!(natural_run_card_binding_v1(&source("Nest", 0)).is_err());
    }
}
