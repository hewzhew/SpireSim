use crate::combat_reward_manifest::{
    render_parity_lab_combat_reward_v1, CombatRewardActionKindV1, CombatRewardActionV1,
    ParityLabCombatRewardInputV1, PublicCombatRewardItemV1, PublicCombatRewardV1,
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
use sts_paired_root::{
    CombatRewardIdentityV1, DecisionProfileV1, PairedActionV1, ProjectionDomainV1, StateRootStoreV2,
};

const COMBAT_REWARD_ROOM_SCHEMA_V1: &str = "sts-paired-combat-reward-room-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StoredCombatRewardKindV1 {
    CombatReward,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StoredCombatRewardItemV1 {
    Gold { amount: i32 },
    StolenGold { amount: i32 },
    Card { choice_count: usize },
    Relic { id: String },
    Potion { id: String },
    EmeraldKey,
    SapphireKey,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCombatRewardRoomV1 {
    kind: StoredCombatRewardKindV1,
    items: Vec<StoredCombatRewardItemV1>,
    close_available: bool,
}

pub(super) fn render_combat_reward_observation_v1(
    store: &StateRootStoreV2,
    report: &ActionObservationReportV6,
    authority: ManifestAuthorityV1,
) -> Result<RenderedManifestV1, String> {
    if report.decision_context.profile != DecisionProfileV1::CombatReward {
        return Err("CombatReward manifest adapter accepts CombatReward only".to_owned());
    }
    let root = store.load_root(&report.root_id)?;
    validate_root_scope_v1(&root, report)?;
    let shared = project_shared_scene_v1(&root, report, authority)?;
    let map_context = project_map_context_v1(store, &root, report, shared.run.act)?;
    require_projection_schema_v1(
        &root,
        ProjectionDomainV1::Room,
        COMBAT_REWARD_ROOM_SCHEMA_V1,
    )?;
    let room: StoredCombatRewardRoomV1 = serde_json::from_value(
        report
            .decision_context
            .room
            .clone()
            .ok_or_else(|| "CombatReward observation omitted its Exact room".to_owned())?,
    )
    .map_err(|error| format!("Exact CombatReward room has an invalid schema: {error}"))?;
    let actions = project_actions_v1(report, &room.items)?;
    render_parity_lab_combat_reward_v1(&ParityLabCombatRewardInputV1 {
        shared,
        reward: PublicCombatRewardV1 {
            items: room.items.iter().map(project_item_v1).collect(),
            close_available: room.close_available,
        },
        map: map_context.map,
        trail: map_context.trail,
        actions,
    })
}

fn project_item_v1(item: &StoredCombatRewardItemV1) -> PublicCombatRewardItemV1 {
    match item {
        StoredCombatRewardItemV1::Gold { amount } => {
            PublicCombatRewardItemV1::Gold { amount: *amount }
        }
        StoredCombatRewardItemV1::StolenGold { amount } => {
            PublicCombatRewardItemV1::StolenGold { amount: *amount }
        }
        StoredCombatRewardItemV1::Card { choice_count } => PublicCombatRewardItemV1::Card {
            choice_count: *choice_count,
        },
        StoredCombatRewardItemV1::Relic { id } => {
            PublicCombatRewardItemV1::Relic { id: id.clone() }
        }
        StoredCombatRewardItemV1::Potion { id } => {
            PublicCombatRewardItemV1::Potion { id: id.clone() }
        }
        StoredCombatRewardItemV1::EmeraldKey => PublicCombatRewardItemV1::EmeraldKey,
        StoredCombatRewardItemV1::SapphireKey => PublicCombatRewardItemV1::SapphireKey,
    }
}

fn project_actions_v1(
    report: &ActionObservationReportV6,
    items: &[StoredCombatRewardItemV1],
) -> Result<Vec<CombatRewardActionV1>, String> {
    if report.observed_actions.len() != report.authorizations.len() {
        return Err("CombatReward observation does not bind every action authorization".to_owned());
    }
    let occurrence_to_index = reward_occurrence_indices_v1(items);
    report
        .observed_actions
        .iter()
        .map(|observed| {
            let kind = match &observed.handle.action {
                PairedActionV1::ClaimCombatReward {
                    reward,
                    same_reward_ordinal,
                } => CombatRewardActionKindV1::Claim {
                    item_index: *occurrence_to_index
                        .get(&(reward.clone(), *same_reward_ordinal))
                        .ok_or_else(|| {
                            "CombatReward action does not resolve to one Exact room item".to_owned()
                        })?,
                },
                PairedActionV1::CloseCombatReward => CombatRewardActionKindV1::Close,
                _ => {
                    return Err(
                        "CombatReward observation exposed an action from another scene".to_owned(),
                    )
                }
            };
            Ok(CombatRewardActionV1 {
                action_id: Some(observed.handle.handle_id.clone()),
                kind,
                availability: project_action_availability_v1(report, &observed.handle.handle_id)?,
            })
        })
        .collect()
}

fn reward_occurrence_indices_v1(
    items: &[StoredCombatRewardItemV1],
) -> BTreeMap<(CombatRewardIdentityV1, usize), usize> {
    let mut indices = BTreeMap::new();
    let mut occurrences = BTreeMap::<CombatRewardIdentityV1, usize>::new();
    for (item_index, item) in items.iter().enumerate() {
        let Some(identity) = supported_identity_v1(item) else {
            continue;
        };
        let same_reward_ordinal = *occurrences.get(&identity).unwrap_or(&0);
        occurrences.insert(identity.clone(), same_reward_ordinal + 1);
        indices.insert((identity, same_reward_ordinal), item_index);
    }
    indices
}

fn supported_identity_v1(item: &StoredCombatRewardItemV1) -> Option<CombatRewardIdentityV1> {
    match item {
        StoredCombatRewardItemV1::Gold { amount } => {
            Some(CombatRewardIdentityV1::Gold { amount: *amount })
        }
        StoredCombatRewardItemV1::StolenGold { amount } => {
            Some(CombatRewardIdentityV1::StolenGold { amount: *amount })
        }
        StoredCombatRewardItemV1::Card { choice_count } => Some(CombatRewardIdentityV1::Card {
            choice_count: *choice_count,
        }),
        StoredCombatRewardItemV1::Potion { id } => Some(CombatRewardIdentityV1::Potion {
            potion_id: id.clone(),
        }),
        StoredCombatRewardItemV1::Relic { .. }
        | StoredCombatRewardItemV1::EmeraldKey
        | StoredCombatRewardItemV1::SapphireKey => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_room_schema_preserves_duplicate_reward_occurrences() {
        let room: StoredCombatRewardRoomV1 = serde_json::from_value(json!({
            "kind": "combat_reward",
            "items": [
                {"kind": "gold", "amount": 10},
                {"kind": "gold", "amount": 10},
                {"kind": "card", "choice_count": 3},
                {"kind": "relic", "id": "vajra"}
            ],
            "close_available": false
        }))
        .unwrap();
        let indices = reward_occurrence_indices_v1(&room.items);
        assert_eq!(
            indices.get(&(CombatRewardIdentityV1::Gold { amount: 10 }, 0)),
            Some(&0)
        );
        assert_eq!(
            indices.get(&(CombatRewardIdentityV1::Gold { amount: 10 }, 1)),
            Some(&1)
        );
        assert_eq!(
            indices.get(&(CombatRewardIdentityV1::Card { choice_count: 3 }, 0)),
            Some(&2)
        );
        assert_eq!(indices.len(), 3);
    }
}
