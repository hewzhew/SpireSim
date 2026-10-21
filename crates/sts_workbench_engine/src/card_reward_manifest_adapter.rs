use crate::card_reward_manifest::{
    render_parity_lab_card_reward_v1, CardRewardActionKindV1, CardRewardActionV1,
    ParityLabCardRewardInputV1, PublicCardRewardChoiceV1, PublicCardRewardV1,
};
use crate::domain_manifest::{ManifestAuthorityV1, RenderedManifestV1};
use crate::domain_manifest_adapter::project_map_context_v1;
use crate::observation::ActionObservationReportV6;
use crate::scene_manifest_adapter::{
    project_action_availability_v1, project_shared_scene_v1, require_projection_schema_v1,
    validate_root_scope_v1,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use sts_paired_root::{DecisionProfileV1, PairedActionV1, ProjectionDomainV1, StateRootStoreV2};

const CARD_REWARD_ROOM_SCHEMA_V1: &str = "sts-paired-card-reward-room-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StoredCardRewardKindV1 {
    CardReward,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StoredCardRewardSourceV1 {
    OrdinaryReward,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCardRewardChoiceV1 {
    card_id: String,
    upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCardRewardRoomV1 {
    kind: StoredCardRewardKindV1,
    source_kind: StoredCardRewardSourceV1,
    cards: Vec<StoredCardRewardChoiceV1>,
    skip_available: bool,
    singing_bowl_available: bool,
}

pub(super) fn render_card_reward_observation_v1(
    store: &StateRootStoreV2,
    report: &ActionObservationReportV6,
    authority: ManifestAuthorityV1,
) -> Result<RenderedManifestV1, String> {
    if report.decision_context.profile != DecisionProfileV1::CardReward {
        return Err("CardReward manifest adapter accepts CardReward only".to_owned());
    }
    let root = store.load_root(&report.root_id)?;
    validate_root_scope_v1(&root, report)?;
    let shared = project_shared_scene_v1(&root, report, authority)?;
    let map_context = project_map_context_v1(store, &root, report, shared.run.act)?;
    require_projection_schema_v1(&root, ProjectionDomainV1::Room, CARD_REWARD_ROOM_SCHEMA_V1)?;
    let room: StoredCardRewardRoomV1 = serde_json::from_value(
        report
            .decision_context
            .room
            .clone()
            .ok_or_else(|| "CardReward observation omitted its Exact room".to_owned())?,
    )
    .map_err(|error| format!("Exact CardReward room has an invalid schema: {error}"))?;
    let choices = room
        .cards
        .iter()
        .map(|choice| PublicCardRewardChoiceV1 {
            card_id: choice.card_id.clone(),
            upgrades: choice.upgrades,
        })
        .collect::<Vec<_>>();
    let actions = project_actions_v1(report, &room.cards)?;
    render_parity_lab_card_reward_v1(&ParityLabCardRewardInputV1 {
        shared,
        reward: PublicCardRewardV1 {
            choices,
            skip_available: room.skip_available,
            singing_bowl_available: room.singing_bowl_available,
        },
        map: map_context.map,
        trail: map_context.trail,
        actions,
    })
}

fn project_actions_v1(
    report: &ActionObservationReportV6,
    choices: &[StoredCardRewardChoiceV1],
) -> Result<Vec<CardRewardActionV1>, String> {
    if report.observed_actions.len() != report.authorizations.len() {
        return Err("CardReward observation does not bind every action authorization".to_owned());
    }
    let occurrence_to_index = choice_occurrence_indices_v1(choices);
    report
        .observed_actions
        .iter()
        .map(|observed| {
            let kind = match &observed.handle.action {
                PairedActionV1::ChooseCardReward {
                    card_id,
                    upgrades,
                    same_card_ordinal,
                } => CardRewardActionKindV1::Take {
                    choice_index: *occurrence_to_index
                        .get(&(card_id.as_str(), *upgrades, *same_card_ordinal))
                        .ok_or_else(|| {
                            "CardReward action does not resolve to one Exact room choice".to_owned()
                        })?,
                },
                PairedActionV1::SkipCardReward => CardRewardActionKindV1::Skip,
                _ => {
                    return Err(
                        "CardReward observation exposed an action from another scene".to_owned(),
                    )
                }
            };
            Ok(CardRewardActionV1 {
                action_id: observed.handle.handle_id.clone(),
                kind,
                availability: project_action_availability_v1(report, &observed.handle.handle_id)?,
            })
        })
        .collect()
}

fn choice_occurrence_indices_v1(
    choices: &[StoredCardRewardChoiceV1],
) -> BTreeMap<(&str, u8, usize), usize> {
    let mut occurrence_to_index = BTreeMap::new();
    let mut occurrences = BTreeMap::<(&str, u8), usize>::new();
    for (choice_index, choice) in choices.iter().enumerate() {
        let key = (choice.card_id.as_str(), choice.upgrades);
        let same_card_ordinal = *occurrences.get(&key).unwrap_or(&0);
        occurrences.insert(key, same_card_ordinal + 1);
        occurrence_to_index.insert(
            (choice.card_id.as_str(), choice.upgrades, same_card_ordinal),
            choice_index,
        );
    }
    occurrence_to_index
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_room_schema_preserves_duplicate_choice_occurrences() {
        let room: StoredCardRewardRoomV1 = serde_json::from_value(json!({
            "kind": "card_reward",
            "source_kind": "ordinary_reward",
            "cards": [
                {"card_id": "Perfected Strike", "upgrades": 0},
                {"card_id": "Perfected Strike", "upgrades": 0},
                {"card_id": "Sword Boomerang", "upgrades": 0}
            ],
            "skip_available": true,
            "singing_bowl_available": false
        }))
        .unwrap();
        let indices = choice_occurrence_indices_v1(&room.cards);
        assert_eq!(indices.get(&("Perfected Strike", 0, 0)), Some(&0));
        assert_eq!(indices.get(&("Perfected Strike", 0, 1)), Some(&1));
        assert_eq!(indices.get(&("Sword Boomerang", 0, 0)), Some(&2));
    }
}
