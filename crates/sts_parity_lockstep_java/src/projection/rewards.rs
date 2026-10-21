use crate::transport::JavaRuntimeObservationV1;
use serde_json::Value;
use sts_java_parity_protocol::canonical_game_id;
use sts_parity_lockstep::{NaturalRewardCardV1, NaturalRunRewardItemV1, NaturalRunRewardsV2};

pub(crate) fn project_rewards_v1(
    observation: &JavaRuntimeObservationV1,
    boundary_kind: &str,
) -> Result<NaturalRunRewardsV2, String> {
    let map_preview = boundary_kind == "natural_map"
        && observation
            .actions
            .iter()
            .any(|action| action.kind == "close_map");
    let pending_shop = matches!(boundary_kind, "natural_shop" | "natural_shop_entry")
        && !observation.frame["parity_internal"]["combat_rewards"].is_null();
    if !matches!(
        boundary_kind,
        "natural_combat_reward" | "natural_card_reward" | "natural_treasure_reward"
    ) && !map_preview
        && !pending_shop
    {
        if !observation.frame["parity_internal"]["combat_rewards"].is_null() {
            return Err("Java non-reward boundary carries combat reward state".to_owned());
        }
        return Ok(NaturalRunRewardsV2::Inactive);
    }
    let rewards = observation.frame["parity_internal"]["combat_rewards"]
        .as_array()
        .ok_or_else(|| "Java combat reward boundary omitted its typed rewards".to_owned())?;
    if pending_shop && (super::current_map_room(observation)? != "shop" || rewards.is_empty()) {
        return Err("Java pending shop rewards lack unclaimed shop content".to_owned());
    }
    let items = rewards
        .iter()
        .enumerate()
        .map(|(expected_index, reward)| {
            let index = usize::try_from(required_u64(reward, "index")?)
                .map_err(|_| "Java combat reward index exceeds usize".to_owned())?;
            if index != expected_index {
                return Err("Java combat reward indices are not contiguous".to_owned());
            }
            project_item(reward)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let close_actions = observation
        .actions
        .iter()
        .filter(|action| {
            matches!(
                action.kind.as_str(),
                "close_combat_reward" | "close_shop_rewards"
            )
        })
        .count();
    if close_actions > 1 {
        return Err("Java combat reward exposes repeated close actions".to_owned());
    }
    let treasure = boundary_kind == "natural_treasure_reward"
        || map_preview && super::current_map_room(observation)? == "treasure";
    if treasure {
        Ok(NaturalRunRewardsV2::Treasure {
            items,
            skippable: map_preview || close_actions == 1,
        })
    } else {
        Ok(NaturalRunRewardsV2::Combat {
            items,
            skippable: map_preview || close_actions == 1,
        })
    }
}

fn project_item(value: &Value) -> Result<NaturalRunRewardItemV1, String> {
    let item = match required_str(value, "type")? {
        "GOLD" => NaturalRunRewardItemV1::Gold {
            amount: required_i32(value, "gold")?,
        },
        "STOLEN_GOLD" => NaturalRunRewardItemV1::StolenGold {
            amount: required_i32(value, "gold")?,
        },
        "CARD" => NaturalRunRewardItemV1::Card {
            cards: value["cards"]
                .as_array()
                .ok_or_else(|| "Java card reward omitted its cards".to_owned())?
                .iter()
                .map(|card| {
                    Ok(NaturalRewardCardV1 {
                        card_id: canonical_game_id(required_str(card, "card_id")?),
                        upgrades: u8::try_from(required_u64(card, "upgrades")?)
                            .map_err(|_| "Java reward card upgrades exceed u8".to_owned())?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        },
        "RELIC" => NaturalRunRewardItemV1::Relic {
            relic_id: canonical_game_id(required_str(value, "relic_id")?),
        },
        "POTION" => NaturalRunRewardItemV1::Potion {
            potion_id: canonical_game_id(required_str(value, "potion_id")?),
        },
        "EMERALD_KEY" => NaturalRunRewardItemV1::EmeraldKey,
        "SAPPHIRE_KEY" => NaturalRunRewardItemV1::SapphireKey,
        other => return Err(format!("Java combat reward type {other:?} is unsupported")),
    };
    validate_item_shape(value, &item)?;
    Ok(item)
}

fn validate_item_shape(value: &Value, item: &NaturalRunRewardItemV1) -> Result<(), String> {
    let expected_gold = matches!(
        item,
        NaturalRunRewardItemV1::Gold { .. } | NaturalRunRewardItemV1::StolenGold { .. }
    );
    let expected_cards = matches!(item, NaturalRunRewardItemV1::Card { .. });
    let expected_relic = matches!(item, NaturalRunRewardItemV1::Relic { .. });
    let expected_potion = matches!(item, NaturalRunRewardItemV1::Potion { .. });
    if value["gold"].is_null() == expected_gold
        || value["cards"].is_null() == expected_cards
        || value["relic_id"].is_null() == expected_relic
        || value["potion_id"].is_null() == expected_potion
    {
        return Err("Java combat reward carries fields outside its typed variant".to_owned());
    }
    Ok(())
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("Java combat reward omitted string {field}"))
}

fn required_u64(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .ok_or_else(|| format!("Java combat reward omitted unsigned integer {field}"))
}

fn required_i32(value: &Value, field: &str) -> Result<i32, String> {
    i32::try_from(
        value[field]
            .as_i64()
            .ok_or_else(|| format!("Java combat reward omitted integer {field}"))?,
    )
    .map_err(|_| format!("Java combat reward integer {field} exceeds i32"))
}
