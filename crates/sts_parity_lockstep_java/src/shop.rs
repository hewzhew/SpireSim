use crate::{
    action::{run_decision_actions, JavaActionExecutionV1, ResolvedJavaActionV1},
    transport::JavaRuntimeObservationV1,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::canonical_game_id;
use sts_parity_lockstep::{
    natural_deck_card_choice_v1, DecisionSurfaceV1, NaturalShopCardV1, NaturalShopItemV1,
    NaturalShopStockV1, SemanticActionV1, SemanticChoiceV1, SemanticScalarV1,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCard {
    slot: usize,
    card_uuid: String,
    card_id: String,
    upgrades: u8,
    price: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawItem {
    slot: usize,
    id: String,
    price: i32,
    purchased: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStock {
    cards: Vec<RawCard>,
    relics: Vec<RawItem>,
    potions: Vec<RawItem>,
    purge_cost: i32,
    purge_available: bool,
    purge_cards: Vec<RawCard>,
}

fn raw_stock(observation: &JavaRuntimeObservationV1) -> Result<RawStock, String> {
    let stock: RawStock =
        serde_json::from_value(observation.frame["parity_internal"]["shop"].clone())
            .map_err(|error| format!("Java shop omitted valid complete inventory: {error}"))?;
    for cards in [&stock.cards, &stock.purge_cards] {
        let mut uuids = BTreeSet::new();
        for (index, card) in cards.iter().enumerate() {
            if card.slot != index || card.card_uuid.is_empty() || !uuids.insert(&card.card_uuid) {
                return Err("Java shop cards have invalid slots or UUIDs".to_owned());
            }
        }
    }
    for items in [&stock.relics, &stock.potions] {
        if items
            .iter()
            .enumerate()
            .any(|(index, item)| item.slot != index || item.id.is_empty())
        {
            return Err("Java shop items have invalid slots or IDs".to_owned());
        }
    }
    if stock.relics.iter().any(|item| item.purchased) {
        return Err("Java shop retained a purchased relic".to_owned());
    }
    Ok(stock)
}

pub(crate) fn stock(observation: &JavaRuntimeObservationV1) -> Result<NaturalShopStockV1, String> {
    let raw = raw_stock(observation)?;
    let stock = NaturalShopStockV1 {
        cards: raw
            .cards
            .iter()
            .map(|item| NaturalShopCardV1 {
                card_id: canonical_game_id(&item.card_id),
                upgrades: item.upgrades,
                price: item.price,
            })
            .collect(),
        relics: raw
            .relics
            .iter()
            .map(|item| NaturalShopItemV1 {
                id: canonical_game_id(&item.id),
                price: item.price,
            })
            .collect(),
        potions: raw
            .potions
            .iter()
            .filter(|item| !item.purchased)
            .map(|item| NaturalShopItemV1 {
                id: canonical_game_id(&item.id),
                price: item.price,
            })
            .collect(),
        purge_cost: raw.purge_cost,
        purge_available: raw.purge_available,
    };
    stock.validate()?;
    Ok(stock)
}

fn purge_choice(
    card: &RawCard,
    bindings: &BTreeMap<String, String>,
    cost: i32,
) -> Result<SemanticChoiceV1, String> {
    let binding = bindings
        .get(&card.card_uuid)
        .ok_or("Java purge card has no source binding")?;
    let mut choice =
        natural_deck_card_choice_v1(binding, &canonical_game_id(&card.card_id), card.upgrades)?;
    choice.attributes.insert(
        "price".to_owned(),
        SemanticScalarV1::Integer(i64::from(cost)),
    );
    Ok(choice)
}

pub(crate) fn actions(
    observation: &JavaRuntimeObservationV1,
    bindings: &BTreeMap<String, String>,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let raw = raw_stock(observation)?;
    let stock = stock(observation)?;
    let kind = observation.frame["public_base"]["decision"]["kind"]
        .as_str()
        .ok_or("Java shop omitted its decision")?;
    let (primary, mut resolved) = run_decision_actions(observation)?;
    let cards = stock.card_choices();
    let relics = stock.relic_choices();
    let potions = stock.potion_choices();
    let raw_actions = observation.frame["action_space"]["actions"]
        .as_array()
        .ok_or("Java shop omitted raw actions")?;
    let mut seen = BTreeSet::new();
    for action in primary {
        if action.parameters_required || !seen.insert(&action.action_id) {
            return Err("Java shop repeats or parameterizes a direct action".to_owned());
        }
        let matches = raw_actions
            .iter()
            .filter(|raw| raw["action_id"].as_str() == Some(&action.action_id))
            .collect::<Vec<_>>();
        let [wire] = matches.as_slice() else {
            return Err("Java shop action has no unique raw identity".to_owned());
        };
        if wire["kind"].as_str() != Some(&action.kind) {
            return Err("Java shop action kind conflicts with raw evidence".to_owned());
        }
        let execution = JavaActionExecutionV1::Act;
        let semantic = match (kind, action.kind.as_str()) {
            ("shop" | "shop_entry", "open_shop_rewards")
                if action.action_id == "open_shop_rewards" =>
            {
                let rewards = crate::projection::rewards::project_rewards_v1(
                    observation,
                    crate::projection::boundary_kind(observation)?,
                )?;
                if !matches!(rewards, sts_parity_lockstep::NaturalRunRewardsV2::Combat { ref items, .. } if !items.is_empty())
                {
                    return Err("Java shop reward reopen action lacks pending rewards".to_owned());
                }
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::CombatReward,
                }
            }
            ("shop_entry", "enter_shop") if action.action_id == "enter_shop" => {
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::Shop,
                }
            }
            ("shop_entry", "proceed_shop_room") if action.action_id == "proceed_shop_room" => {
                SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::Shop,
                }
            }
            ("shop", "leave_shop") if action.action_id == "leave_shop" => {
                SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::Shop,
                }
            }
            ("shop", "begin_shop_purge") => {
                if !raw.purge_available
                    || wire["shop_price"].as_i64() != Some(i64::from(raw.purge_cost))
                    || action.action_id != format!("begin_shop_purge:{}", raw.purge_cost)
                {
                    return Err(
                        "Java purge action disagrees with shop price or availability".to_owned(),
                    );
                }
                let deck = observation.frame["parity_internal"]["master_deck"]
                    .as_array()
                    .ok_or("Java shop omitted master deck")?;
                for card in &raw.purge_cards {
                    if !deck.iter().any(|entry| {
                        entry["card_uuid"] == card.card_uuid
                            && entry["card_id"] == card.card_id
                            && entry["upgrades"] == card.upgrades
                    }) {
                        return Err("Java purge candidate disagrees with master deck".to_owned());
                    }
                    resolved.push(ResolvedJavaActionV1 {
                        semantic: SemanticActionV1::Select {
                            surface: DecisionSurfaceV1::Shop,
                            choice: purge_choice(card, bindings, raw.purge_cost)?,
                        },
                        runtime_handle: action.runtime_handle.clone(),
                        execution: JavaActionExecutionV1::PurgeShopCard {
                            selected_card_uuid: card.card_uuid.clone(),
                        },
                    });
                }
                continue;
            }
            ("shop", "buy_shop_card" | "buy_shop_relic" | "buy_shop_potion") => {
                let slot = wire["shop_slot"]
                    .as_u64()
                    .and_then(|slot| usize::try_from(slot).ok())
                    .ok_or("Java shop action omitted slot")?;
                let choice = match action.kind.as_str() {
                    "buy_shop_card" => {
                        let card = raw
                            .cards
                            .get(slot)
                            .ok_or("Java shop card slot exceeds inventory")?;
                        if action.action_id
                            != format!(
                                "shop_card:{slot}:{}:{}:{}:{}",
                                card.card_uuid, card.card_id, card.upgrades, card.price
                            )
                            || wire["card_uuid"] != card.card_uuid
                            || wire["card_id"] != card.card_id
                            || wire["card_upgrades"] != card.upgrades
                        {
                            return Err("Java shop card action disagrees with inventory".to_owned());
                        }
                        cards[slot].clone()
                    }
                    "buy_shop_relic" => {
                        let item = raw
                            .relics
                            .get(slot)
                            .ok_or("Java shop relic slot exceeds inventory")?;
                        if action.action_id
                            != format!("shop_relic:{slot}:{}:{}", item.id, item.price)
                            || wire["reward_id"] != item.id
                        {
                            return Err(
                                "Java shop relic action disagrees with inventory".to_owned()
                            );
                        }
                        relics[slot].clone()
                    }
                    _ => {
                        let item = raw
                            .potions
                            .get(slot)
                            .ok_or("Java shop potion slot exceeds inventory")?;
                        if item.purchased
                            || action.action_id
                                != format!("shop_potion:{slot}:{}:{}", item.id, item.price)
                            || wire["potion_id"] != item.id
                        {
                            return Err(
                                "Java shop potion action disagrees with inventory".to_owned()
                            );
                        }
                        let available_index = raw.potions[..slot]
                            .iter()
                            .filter(|item| !item.purchased)
                            .count();
                        potions[available_index].clone()
                    }
                };
                if wire["shop_price"].as_i64()
                    != choice
                        .attributes
                        .get("price")
                        .and_then(|value| match value {
                            SemanticScalarV1::Integer(value) => Some(*value),
                            _ => None,
                        })
                {
                    return Err("Java shop action price conflicts with inventory".to_owned());
                }
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Shop,
                    choice,
                }
            }
            ("shop_purge_card", "choose_shop_purge_card")
            | ("shop_purge_confirm", "confirm_shop_purge_card") => {
                let uuid = wire["card_uuid"]
                    .as_str()
                    .ok_or("Java purge omitted selected UUID")?;
                let card = raw
                    .purge_cards
                    .iter()
                    .find(|card| card.card_uuid == uuid)
                    .ok_or("Java purge selected UUID is absent")?;
                let ordinal = wire["card_ordinal"]
                    .as_u64()
                    .ok_or("Java purge omitted deck ordinal")?;
                let prefix = if kind == "shop_purge_card" {
                    "shop_purge_card"
                } else {
                    "confirm_shop_purge_card"
                };
                if action.action_id
                    != format!(
                        "{prefix}:{ordinal}:{uuid}:{}:{}",
                        card.card_id, card.upgrades
                    )
                {
                    return Err("Java purge action has conflicting identity".to_owned());
                }
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Shop,
                    choice: purge_choice(card, bindings, raw.purge_cost)?,
                }
            }
            _ => {
                return Err(format!(
                    "Java shop exposes unsupported action {} on {kind}",
                    action.kind
                ))
            }
        };
        // Every semantic target was taken from this raw frame, never from a
        // simulator shop index or a displayed card name.
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution,
        });
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn fixture() -> JavaRuntimeObservationV1 {
        serde_json::from_value(json!({
            "producer":{"implementation":"fixture","version":"1","session_epoch":"shop"},
            "frame_id":1,
            "frame":{
                "public_base":{"decision":{"kind":"shop"}},
                "parity_internal":{"shop":{
                    "cards":[{"slot":0,"card_uuid":"a","card_id":"Exhume","upgrades":0,"price":999}],
                    "relics":[{"slot":0,"id":"Letter Opener","price":270,"purchased":false}],
                    "potions":[{"slot":0,"id":"Regen Potion","price":85,"purchased":true},
                        {"slot":1,"id":"Skill Potion","price":54,"purchased":false}],
                    "purge_available":true,"purge_cost":75,"purge_cards":[]
                }},
                "action_space":{"actions":[{"kind":"buy_shop_potion","action_id":"shop_potion:1:Skill Potion:54","shop_slot":1,"shop_price":54,"potion_id":"Skill Potion"}]}
            },
            "actions":[{"runtime_handle":"pick","kind":"buy_shop_potion","action_id":"shop_potion:1:Skill Potion:54","parameters_required":false}]
        })).unwrap()
    }

    #[test]
    fn shop_keeps_unaffordable_stock_and_resolves_after_a_sold_potion_slot() {
        let observation = fixture();
        let stock = stock(&observation).unwrap();
        assert_eq!(stock.cards[0].price, 999);
        assert_eq!(stock.relics[0].price, 270);
        assert_eq!(stock.potions.len(), 1);
        let actions = actions(&observation, &BTreeMap::new()).unwrap();
        assert_eq!(
            actions[0].semantic,
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::Shop,
                choice: stock.potion_choices()[0].clone()
            }
        );
    }

    #[test]
    fn shop_rejects_missing_inventory_duplicate_uuid_and_incorrect_action_price() {
        for fault in 0..3 {
            let mut observation = fixture();
            match fault {
                0 => observation.frame["parity_internal"]["shop"]["relics"] = Value::Null,
                1 => {
                    let mut duplicate =
                        observation.frame["parity_internal"]["shop"]["cards"][0].clone();
                    duplicate["slot"] = json!(1);
                    observation.frame["parity_internal"]["shop"]["cards"]
                        .as_array_mut()
                        .unwrap()
                        .push(duplicate);
                }
                _ => observation.frame["action_space"]["actions"][0]["shop_price"] = json!(53),
            }
            assert!(actions(&observation, &BTreeMap::new()).is_err());
        }
    }
}
