use crate::{
    projection::{current_node, JavaStableCardBindingV1},
    transport::{JavaLockstepReceiptV1, JavaRuntimeObservationV1},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::canonical_game_id;
use sts_parity_lockstep::{
    natural_run_card_binding_v1, DecisionSurfaceV1, DomainEvidenceV1, NaturalRunCardSourceV1,
    ParityDomainV1, ParitySnapshotV1, SemanticActionV1,
};

/// Reconstructed from Java provenance, including acquisitions whose cards have
/// since left the deck. This is not Rust identity or serialized shadow state.
pub(crate) fn remember_master_deck_acquisitions(
    snapshot: &ParitySnapshotV1,
    next_slots: &mut BTreeMap<String, u32>,
) -> Result<(), String> {
    let Some(domain) = snapshot.domains.get(&ParityDomainV1::CardBindings) else {
        return Ok(());
    };
    let DomainEvidenceV1::Observed { value, .. } = &domain.evidence else {
        return Ok(());
    };
    for relation in value
        .as_array()
        .ok_or("Java card bindings are not an array")?
    {
        let source: NaturalRunCardSourceV1 = serde_json::from_value(relation["source"].clone())
            .map_err(|error| format!("Java bound card source is malformed: {error}"))?;
        let key_and_slot = match source {
            NaturalRunCardSourceV1::CardReward {
                room_id,
                product_slot,
            } => Some((room_id, product_slot)),
            NaturalRunCardSourceV1::ShopPurchase {
                room_id,
                product_slot,
            } => Some((format!("shop:{room_id}"), product_slot)),
            _ => None,
        };
        if let Some((key, product_slot)) = key_and_slot {
            let next = product_slot
                .checked_add(1)
                .ok_or("Java card reward slot overflow")?;
            let current = next_slots.entry(key).or_default();
            *current = (*current).max(next);
        }
    }
    Ok(())
}

pub(crate) struct JavaMasterDeckAcquisitionV1 {
    frame_id: u64,
    action_id: String,
    selected_uuid: String,
    card_id: String,
    upgrades: u64,
    prior_uuids: BTreeSet<String>,
    source: NaturalRunCardSourceV1,
    raw_source: Option<NaturalRunCardSourceV1>,
    copy_offer_uuids: Option<BTreeSet<String>>,
}

impl JavaMasterDeckAcquisitionV1 {
    pub(crate) fn prepare(
        observation: Option<&JavaRuntimeObservationV1>,
        semantic: &SemanticActionV1,
        runtime_handle: &str,
        next_slots: &BTreeMap<String, u32>,
    ) -> Result<Option<Self>, String> {
        if let SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices,
        } = semantic
        {
            if let [choice] = choices.as_slice() {
                if choice.kind == "event_card_offer" {
                    return Self::prepare_event_offer(
                        observation.ok_or("Java event acquisition has no predecessor")?,
                        choice,
                        runtime_handle,
                    )
                    .map(Some);
                }
            }
        }
        if matches!(semantic,SemanticActionV1::Select {surface:DecisionSurfaceV1::Shop,choice} if choice.kind=="shop_card")
        {
            return Self::prepare_shop(
                observation.ok_or("Java shop acquisition has no predecessor")?,
                runtime_handle,
                next_slots,
            )
            .map(Some);
        }
        if !matches!(
            semantic,
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::CardReward,
                choice,
            } if choice.kind == "reward_card"
        ) {
            return Ok(None);
        }
        let observation = observation.ok_or("Java card reward has no predecessor")?;
        let action = observation
            .actions
            .iter()
            .find(|action| action.runtime_handle == runtime_handle)
            .ok_or("Java card reward has no frame-bound action")?;
        let reward = &observation.frame["public_base"]["decision"]["card_reward"];
        let neow = reward["source_kind"] == "neow_reward";
        if action.kind != "choose_card_reward"
            || (!neow && reward["source_kind"] != "ordinary_reward")
        {
            return Err("Java card acquisition is not an ordinary typed reward".to_owned());
        }
        let index = action
            .reward_index
            .ok_or("Java card reward omitted its selected index")?;
        let card = reward["cards"]
            .as_array()
            .and_then(|cards| cards.get(index))
            .ok_or("Java card reward selection is absent from its offered cards")?;
        let selected_uuid = string(card, "card_uuid")?.to_owned();
        let raw_card_id = string(card, "card_id")?;
        let upgrades = card["upgrades"]
            .as_u64()
            .ok_or("Java card reward omitted upgrades")?;
        if action.action_id
            != format!("card_reward:{index}:{selected_uuid}:{raw_card_id}:{upgrades}")
        {
            return Err(
                "Java card reward receipt identity disagrees with its selected UUID".to_owned(),
            );
        }
        let deck = master_deck(observation)?;
        let prior_uuids = deck_uuids(deck)?;
        if prior_uuids.contains(&selected_uuid) {
            return Err("Java reward selected an already acquired card UUID".to_owned());
        }
        if neow {
            if crate::projection::boundary_kind(observation)? != "natural_neow_card_reward" {
                return Err("Java Neow acquisition lacks its typed entrance".to_owned());
            }
            // The frame-bound offered UUID must be the single new deck UUID
            // in the adjacent receipt. No deck order or card-name matching.
            return Ok(Some(Self {
                frame_id: observation.frame_id,
                action_id: action.action_id.clone(),
                selected_uuid,
                card_id: canonical_game_id(raw_card_id),
                upgrades,
                prior_uuids,
                source: NaturalRunCardSourceV1::NeowCardReward { product_slot: 0 },
                raw_source: None,
                copy_offer_uuids: None,
            }));
        }
        let room_id = current_node(observation)?.ok_or("Java card reward has no natural room")?;
        // CommunicationMod allocates its SpireField tag from the currently
        // visible Java tags. Reload clears those tags; validate that local
        // observation independently of the durable acquisition counter.
        let mut local_slots = BTreeSet::new();
        for card in deck {
            if let Some(raw) = card.get("source").filter(|raw| !raw.is_null()) {
                let source: NaturalRunCardSourceV1 = serde_json::from_value(raw.clone())
                    .map_err(|error| format!("Java raw card source is malformed: {error}"))?;
                if let NaturalRunCardSourceV1::CardReward {
                    room_id: room,
                    product_slot,
                } = source
                {
                    if room == room_id && !local_slots.insert(product_slot) {
                        return Err("Java raw card reward tags repeat a local slot".to_owned());
                    }
                }
            }
        }
        let local_next =
            u32::try_from(local_slots.len()).map_err(|_| "Java local reward slot overflow")?;
        if local_slots.iter().copied().ne(0..local_next) {
            return Err("Java raw card reward tags are not contiguous".to_owned());
        }
        let product_slot = next_slots.get(&room_id).copied().unwrap_or(0);
        Ok(Some(Self {
            frame_id: observation.frame_id,
            action_id: action.action_id.clone(),
            selected_uuid,
            card_id: canonical_game_id(raw_card_id),
            upgrades,
            prior_uuids,
            source: NaturalRunCardSourceV1::CardReward {
                room_id: room_id.clone(),
                product_slot,
            },
            raw_source: Some(NaturalRunCardSourceV1::CardReward {
                room_id,
                product_slot: local_next,
            }),
            copy_offer_uuids: None,
        }))
    }

    fn prepare_shop(
        observation: &JavaRuntimeObservationV1,
        runtime_handle: &str,
        next_slots: &BTreeMap<String, u32>,
    ) -> Result<Self, String> {
        let action = observation
            .actions
            .iter()
            .find(|action| {
                action.runtime_handle == runtime_handle && action.kind == "buy_shop_card"
            })
            .ok_or("Java shop acquisition has no frame-bound purchase")?;
        let wire = observation.frame["action_space"]["actions"]
            .as_array()
            .and_then(|actions| {
                actions
                    .iter()
                    .find(|wire| wire["action_id"].as_str() == Some(&action.action_id))
            })
            .ok_or("Java shop purchase has no raw contract")?;
        let slot = wire["shop_slot"]
            .as_u64()
            .and_then(|slot| usize::try_from(slot).ok())
            .ok_or("Java purchase omitted slot")?;
        let card = observation.frame["parity_internal"]["shop"]["cards"]
            .as_array()
            .and_then(|cards| cards.get(slot))
            .ok_or("Java purchased offer is absent from stock")?;
        let selected_uuid = string(card, "card_uuid")?.to_owned();
        let raw_id = string(card, "card_id")?;
        let upgrades = card["upgrades"]
            .as_u64()
            .ok_or("Java purchased offer omitted upgrades")?;
        let price = card["price"]
            .as_i64()
            .ok_or("Java purchased offer omitted price")?;
        if action.action_id
            != format!("shop_card:{slot}:{selected_uuid}:{raw_id}:{upgrades}:{price}")
        {
            return Err(
                "Java purchase receipt identity conflicts with its selected offer".to_owned(),
            );
        }
        let room_id = current_node(observation)?.ok_or("Java purchase has no room")?;
        let prior_uuids = deck_uuids(master_deck(observation)?)?;
        if prior_uuids.contains(&selected_uuid) {
            return Err("Java shop offers an existing deck UUID".to_owned());
        }
        let product_slot = next_slots
            .get(&format!("shop:{room_id}"))
            .copied()
            .unwrap_or(0);
        Ok(Self {
            frame_id: observation.frame_id,
            action_id: action.action_id.clone(),
            selected_uuid,
            card_id: canonical_game_id(raw_id),
            upgrades,
            prior_uuids,
            source: NaturalRunCardSourceV1::ShopPurchase {
                room_id,
                product_slot,
            },
            raw_source: None,
            copy_offer_uuids: None,
        })
    }

    fn prepare_event_offer(
        observation: &JavaRuntimeObservationV1,
        choice: &sts_parity_lockstep::SemanticChoiceV1,
        runtime_handle: &str,
    ) -> Result<Self, String> {
        let identity = crate::action::java_grid_select_boundary_identity_v1(observation)?;
        if identity.selection_kind != "event_card_offer" {
            return Err("Java event acquisition is not a supported offer page".to_owned());
        }
        let grid = &observation.frame["public_base"]["decision"]["grid_select"];
        let action = observation
            .actions
            .iter()
            .find(|action| {
                action.runtime_handle == runtime_handle && action.kind == "submit_grid_select"
            })
            .ok_or("Java event acquisition has no frame-bound selection")?;
        if action.action_id != format!("submit_grid_select:{}", string(grid, "grid_select_id")?) {
            return Err("Java event acquisition disagrees with its grid identity".to_owned());
        }
        let offers = grid["candidate_cards"]
            .as_array()
            .ok_or("Java event acquisition omitted offers")?;
        let card = offers
            .get(choice.occurrence as usize)
            .ok_or("Java event selected slot is outside its offers")?;
        let upgrades = u8::try_from(
            card["upgrades"]
                .as_u64()
                .ok_or("Java event offer omitted upgrades")?,
        )
        .map_err(|_| "Java event upgrades exceed u8")?;
        let card_id = canonical_game_id(string(card, "card_id")?);
        if *choice
            != sts_parity_lockstep::natural_event_card_offer_v1(
                &identity.event.canonical_id,
                identity.event.screen,
                choice.occurrence,
                &card_id,
                upgrades,
            )?
            || card["index"].as_u64() != Some(u64::from(choice.occurrence))
        {
            return Err("Java event acquisition differs from the selected typed offer".to_owned());
        }
        let prior_uuids = deck_uuids(master_deck(observation)?)?;
        let offer_uuids = deck_uuids(offers)?;
        if offers.len() != 20 || !prior_uuids.is_disjoint(&offer_uuids) {
            return Err("Java event acquisition has invalid fresh offers".to_owned());
        }
        Ok(Self {
            frame_id: observation.frame_id,
            action_id: action.action_id.clone(),
            selected_uuid: string(card, "card_uuid")?.to_owned(),
            card_id,
            upgrades: u64::from(upgrades),
            prior_uuids,
            source: NaturalRunCardSourceV1::EventCard {
                room_id: current_node(observation)?.ok_or("Java event acquisition has no room")?,
                event_id: identity.event.canonical_id,
                screen: identity.event.screen,
                offer_index: choice.occurrence,
            },
            raw_source: None,
            copy_offer_uuids: Some(offer_uuids),
        })
    }

    pub(crate) fn bind(
        self,
        receipt: &JavaLockstepReceiptV1,
        successor: &JavaRuntimeObservationV1,
    ) -> Result<(String, JavaStableCardBindingV1), String> {
        if receipt.status != "completed"
            || receipt.based_on_frame_id != self.frame_id
            || receipt.successor_frame_id != Some(successor.frame_id)
            || receipt.action_id != self.action_id
        {
            return Err(
                "Java card acquisition receipt does not bind adjacent frames and selection"
                    .to_owned(),
            );
        }
        let deck = master_deck(successor)?;
        let uuids = deck_uuids(deck)?;
        let products = uuids.difference(&self.prior_uuids).collect::<Vec<_>>();
        let [product_uuid] = products.as_slice() else {
            return Err("Java acquisition did not produce exactly one fresh deck card".to_owned());
        };
        // TheLibrary.update explicitly calls selectedCards[0].makeCopy(),
        // then obtains that sole new object. The adjacent typed selection
        // proves this copy; neither deck position nor a card-name search binds it.
        let valid_identity = if let Some(offers) = &self.copy_offer_uuids {
            !offers.contains(*product_uuid)
        } else {
            *product_uuid == &self.selected_uuid
        };
        if !self.prior_uuids.is_subset(&uuids) || !valid_identity {
            return Err(
                "Java card reward did not acquire exactly the selected fresh UUID".to_owned(),
            );
        }
        let card = deck
            .iter()
            .find(|card| card["card_uuid"].as_str() == Some(product_uuid.as_str()))
            .ok_or("Java card reward product is absent")?;
        if canonical_game_id(string(card, "card_id")?) != self.card_id
            || card["upgrades"].as_u64() != Some(self.upgrades)
        {
            return Err("Java acquired card differs from its selected offer".to_owned());
        }
        let raw_source: Option<NaturalRunCardSourceV1> = card
            .get("source")
            .filter(|raw| !raw.is_null())
            .map(|raw| serde_json::from_value(raw.clone()))
            .transpose()
            .map_err(|error| format!("Java acquired card has an invalid raw source: {error}"))?;
        if raw_source != self.raw_source {
            return Err(
                "Java acquired card source disagrees with its local source history".to_owned(),
            );
        }
        Ok((
            (*product_uuid).clone(),
            JavaStableCardBindingV1 {
                binding: natural_run_card_binding_v1(&self.source)?,
                source: self.source,
                raw_source,
            },
        ))
    }
}

fn master_deck(observation: &JavaRuntimeObservationV1) -> Result<&Vec<Value>, String> {
    observation.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java card acquisition omitted its master deck".to_owned())
}

fn deck_uuids(deck: &[Value]) -> Result<BTreeSet<String>, String> {
    let mut uuids = BTreeSet::new();
    for card in deck {
        if !uuids.insert(string(card, "card_uuid")?.to_owned()) {
            return Err("Java card acquisition deck repeats a UUID".to_owned());
        }
    }
    Ok(uuids)
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Java card acquisition omitted {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sts_parity_lockstep::{natural_card_reward_choice_v1, NaturalRewardCardV1};

    #[test]
    fn neow_acquisition_binds_the_selected_offer_uuid_without_a_map_room() {
        let (mut before, mut after, receipt, semantic) = fixture();
        before.frame["boundary"] = json!({"kind":"run_decision"});
        before.frame["public_base"]["run"] = json!({"act":1});
        before.frame["public_base"]["map"] = json!({"current_x":-1,"current_y":-1});
        before.frame["public_base"]["decision"]["kind"] = json!("card_reward");
        before.frame["public_base"]["decision"]["card_reward"]["source_kind"] =
            json!("neow_reward");
        after.frame["parity_internal"]["master_deck"][1]
            .as_object_mut()
            .unwrap()
            .remove("source");
        let prepared = JavaMasterDeckAcquisitionV1::prepare(
            Some(&before),
            &semantic,
            "pick",
            &BTreeMap::new(),
        )
        .unwrap()
        .unwrap();
        let (uuid, bound) = prepared.bind(&receipt, &after).unwrap();
        assert_eq!(uuid, "selected");
        assert_eq!(
            bound.source,
            NaturalRunCardSourceV1::NeowCardReward { product_slot: 0 }
        );

        after.frame["parity_internal"]["master_deck"][1]["card_uuid"] = json!("unselected");
        let prepared = JavaMasterDeckAcquisitionV1::prepare(
            Some(&before),
            &semantic,
            "pick",
            &BTreeMap::new(),
        )
        .unwrap()
        .unwrap();
        assert!(prepared
            .bind(&receipt, &after)
            .unwrap_err()
            .contains("selected fresh UUID"));
        before.frame["public_base"]["map"]["current_x"] = json!(0);
        assert!(JavaMasterDeckAcquisitionV1::prepare(
            Some(&before),
            &semantic,
            "pick",
            &BTreeMap::new()
        )
        .is_err());
    }

    fn fixture() -> (
        JavaRuntimeObservationV1,
        JavaRuntimeObservationV1,
        JavaLockstepReceiptV1,
        SemanticActionV1,
    ) {
        let action_id = "card_reward:1:selected:Rupture:0";
        let before: JavaRuntimeObservationV1 = serde_json::from_value(json!({
            "producer": {"implementation":"fixture", "version":"1", "session_epoch":"reload"},
            "frame_id": 50,
            "frame": {
                "public_base": {"map":{"current_x":1,"current_y":3}, "decision":{
                    "card_reward":{"source_kind":"ordinary_reward", "cards":[
                        {"card_uuid":"unselected", "card_id":"Rupture", "upgrades":0},
                        {"card_uuid":"selected", "card_id":"Rupture", "upgrades":0}
                    ]}
                }},
                "parity_internal":{"master_deck":[{"card_uuid":"reloaded-old", "card_id":"Impervious", "upgrades":0}]}
            },
            "actions":[{"runtime_handle":"pick", "action_id":action_id, "kind":"choose_card_reward", "reward_index":1, "parameters_required":false}]
        })).unwrap();
        let mut after = before.clone();
        after.frame_id = 51;
        after.frame["parity_internal"]["master_deck"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "card_uuid":"selected", "card_id":"Rupture", "upgrades":0,
                "source":{"kind":"card_reward", "room_id":"F04X01", "product_slot":0}
            }));
        let receipt = JavaLockstepReceiptV1 {
            status: "completed".to_owned(),
            request_id: "request".to_owned(),
            based_on_frame_id: 50,
            successor_frame_id: Some(51),
            action_id: action_id.to_owned(),
        };
        let semantic = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CardReward,
            choice: natural_card_reward_choice_v1(
                &NaturalRewardCardV1 {
                    card_id: "rupture".to_owned(),
                    upgrades: 0,
                },
                1,
            ),
        };
        (before, after, receipt, semantic)
    }

    #[test]
    fn library_acquisition_binds_one_fresh_copy_of_the_selected_offer() {
        let (mut before, mut after, mut receipt, _) = fixture();
        let cards = [
            "Rupture",
            "Thunderclap",
            "Entrench",
            "Carnage",
            "Armaments",
            "Cleave",
            "Battle Trance",
            "Clothesline",
            "Perfected Strike",
            "Heavy Blade",
            "Anger",
            "Inflame",
            "Fire Breathing",
            "Wild Strike",
            "Twin Strike",
            "Flex",
            "Combust",
            "Havoc",
            "Sword Boomerang",
            "True Grit",
        ];
        let uuids = (0..20)
            .map(|index| format!("offer-{index}"))
            .collect::<Vec<_>>();
        let grid_id = format!(
            "grid_select:TheLibrary:EventOffer:TheLibrary:1:false:false:{}:selected:",
            uuids.join(",")
        );
        let action_id = format!("submit_grid_select:{grid_id}");
        before.frame["public_base"]["decision"] = json!({"kind":"grid_select","event_id":"TheLibrary","event_screen_index":1,
            "event_identity":{"status":"supported","event_id":"TheLibrary","runtime_class":"com.megacrit.cardcrawl.events.city.TheLibrary"},
            "grid_select":{"grid_select_id":grid_id,"source_pile":"EventOffer","selection_reason":"TheLibrary","candidate_cards":cards.iter().enumerate().map(|(index,id)| json!({"index":index,"card_id":id,"card_uuid":uuids[index],"upgrades":0})).collect::<Vec<_>>()}});
        before.actions[0].kind = "submit_grid_select".into();
        before.actions[0].action_id = action_id.clone();
        after.frame["parity_internal"]["master_deck"][1] =
            json!({"card_uuid":"fresh-copy","card_id":"Rupture","upgrades":0});
        receipt.action_id = action_id;
        let semantic = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices: vec![sts_parity_lockstep::natural_event_card_offer_v1(
                "thelibrary",
                1,
                0,
                "rupture",
                0,
            )
            .unwrap()],
        };
        let acquisition = JavaMasterDeckAcquisitionV1::prepare(
            Some(&before),
            &semantic,
            "pick",
            &BTreeMap::new(),
        )
        .unwrap()
        .unwrap();
        let (uuid, binding) = acquisition.bind(&receipt, &after).unwrap();
        assert_eq!(uuid, "fresh-copy");
        assert_eq!(
            binding.binding,
            "natural_run/event_card/F04X01/thelibrary/1/0"
        );
        assert!(binding.raw_source.is_none());
        for fault in 0..8 {
            let mut wrong = after.clone();
            let mut wrong_receipt = receipt.clone();
            match fault {
                0 => {
                    wrong.frame["parity_internal"]["master_deck"][1]["card_uuid"] = json!(uuids[0])
                }
                1 => {
                    wrong.frame["parity_internal"]["master_deck"][1]["card_uuid"] = json!(uuids[1])
                }
                2 => {
                    wrong.frame["parity_internal"]["master_deck"][1]["card_id"] =
                        json!("Thunderclap")
                }
                3 => wrong.frame["parity_internal"]["master_deck"][1]["upgrades"] = json!(1),
                4 => {
                    wrong.frame["parity_internal"]["master_deck"]
                        .as_array_mut()
                        .unwrap()
                        .push(
                            json!({"card_uuid":"second-product","card_id":"Rupture","upgrades":0}),
                        );
                }
                5 => {
                    wrong.frame["parity_internal"]["master_deck"]
                        .as_array_mut()
                        .unwrap()
                        .remove(0);
                }
                6 => wrong_receipt.based_on_frame_id = 49,
                _ => wrong_receipt.action_id = "unrelated".into(),
            }
            let acquisition = JavaMasterDeckAcquisitionV1::prepare(
                Some(&before),
                &semantic,
                "pick",
                &BTreeMap::new(),
            )
            .unwrap()
            .unwrap();
            assert!(
                acquisition.bind(&wrong_receipt, &wrong).is_err(),
                "fault {fault}"
            );
        }
    }

    #[test]
    fn shop_acquisition_binds_the_purchased_object_and_its_own_source_family() {
        let (mut before, mut after, mut receipt, _) = fixture();
        let action_id = "shop_card:0:selected:Rupture:0:81";
        before.actions[0].kind = "buy_shop_card".to_owned();
        before.actions[0].action_id = action_id.to_owned();
        before.frame["action_space"] = json!({"actions":[{"action_id":action_id,"shop_slot":0}]});
        before.frame["parity_internal"]["shop"] = json!({"cards":[{
            "card_uuid":"selected","card_id":"Rupture","upgrades":0,"price":81
        }]});
        after.frame["parity_internal"]["master_deck"][1]
            .as_object_mut()
            .unwrap()
            .remove("source");
        receipt.action_id = action_id.to_owned();
        let semantic = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Shop,
            choice: sts_parity_lockstep::SemanticChoiceV1::new("shop_card", "rupture", 0),
        };
        let history = BTreeMap::from([("shop:F04X01".to_owned(), 2)]);
        let acquisition =
            JavaMasterDeckAcquisitionV1::prepare(Some(&before), &semantic, "pick", &history)
                .unwrap()
                .unwrap();
        let (uuid, bound) = acquisition.bind(&receipt, &after).unwrap();
        assert_eq!(uuid, "selected");
        assert_eq!(bound.binding, "natural_run/shop_purchase/F04X01/2");
        assert!(bound.raw_source.is_none());
    }

    #[test]
    fn card_reward_after_reload_uses_durable_history_and_keeps_the_raw_tag() {
        let (before, after, receipt, semantic) = fixture();
        let history = BTreeMap::from([("F04X01".to_owned(), 1)]);
        let acquisition =
            JavaMasterDeckAcquisitionV1::prepare(Some(&before), &semantic, "pick", &history)
                .unwrap()
                .unwrap();
        let (uuid, bound) = acquisition.bind(&receipt, &after).unwrap();
        assert_eq!(uuid, "selected");
        assert_eq!(bound.binding, "natural_run/card_reward/F04X01/1");
        assert_eq!(
            bound.raw_source,
            Some(NaturalRunCardSourceV1::CardReward {
                room_id: "F04X01".to_owned(),
                product_slot: 0
            })
        );
        assert_eq!(
            after.frame["parity_internal"]["master_deck"][1]["source"]["product_slot"],
            0
        );
    }

    #[test]
    fn card_reward_requires_selected_uuid_local_provenance_and_adjacent_receipt() {
        let (before, after, receipt, semantic) = fixture();
        for fault in 0..7 {
            let mut successor = after.clone();
            let mut receipt = receipt.clone();
            match fault {
                0 => {
                    successor.frame["parity_internal"]["master_deck"][1]["card_uuid"] =
                        json!("unselected")
                }
                1 => {
                    successor.frame["parity_internal"]["master_deck"][1]["source"]["product_slot"] =
                        json!(1)
                }
                2 => {
                    successor.frame["parity_internal"]["master_deck"][1]["source"]["room_id"] =
                        json!("F05X01")
                }
                3 => {
                    successor.frame["parity_internal"]["master_deck"]
                        .as_array_mut()
                        .unwrap()
                        .remove(0);
                }
                4 => {
                    receipt.based_on_frame_id = 49;
                }
                5 => {
                    receipt.action_id = "unrelated".to_owned();
                }
                _ => successor.frame["parity_internal"]["master_deck"][1]["upgrades"] = json!(1),
            };
            let acquisition = JavaMasterDeckAcquisitionV1::prepare(
                Some(&before),
                &semantic,
                "pick",
                &BTreeMap::new(),
            )
            .unwrap()
            .unwrap();
            assert!(
                acquisition.bind(&receipt, &successor).is_err(),
                "fault {fault}"
            );
        }
    }
}
