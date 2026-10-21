use crate::{
    adapter::{create_operation_directory, run_typed_adapter},
    artifact::publish_artifact,
    card_reward_manifest_adapter::render_card_reward_observation_v1,
    combat_manifest_adapter::render_combat_observation_v1,
    combat_reward_manifest_adapter::render_combat_reward_observation_v1,
    domain_manifest::{ManifestAuthorityV1, RenderedManifestV1},
    domain_manifest_adapter::render_map_observation_v1,
    event_manifest_adapter::render_event_observation_v1,
    neow_manifest_adapter::render_neow_observation_v1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Duration;
use sts_paired_root::{
    CapabilityKindV1, CapabilityProofV1, CapabilityStatusV1, CombatRewardIdentityV1,
    DecisionProfileV1, MapRoomIdentityV1, NeowDrawbackIdentityV1, NeowRewardIdentityV1,
    PairedActionHandleV1, PairedActionV1, StateRootStoreV2,
};

const ACTION_OBSERVATION_REPORT_SCHEMA_V6: &str = "sts-paired-root-action-observation-v6";
const ACTION_OBSERVATION_ARTIFACT_TYPE: &str = "paired_root_action_observation";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActionObservationReportV6 {
    pub(super) schema: String,
    pub(super) root_id: String,
    pub(super) action_space_id: String,
    pub(super) decision_context: DecisionContextV1,
    pub(super) observed_actions: Vec<ObservedActionV1>,
    pub(super) authorizations: Vec<ActionAuthorizationV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DecisionContextV1 {
    pub(super) profile: DecisionProfileV1,
    pub(super) run: Value,
    pub(super) master_deck: Value,
    pub(super) relics: Value,
    pub(super) potions: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) map: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) room: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObservedActionV1 {
    pub(super) handle: PairedActionHandleV1,
    #[serde(default)]
    card: Option<ObservedCardV1>,
    #[serde(default)]
    potion: Option<ObservedPotionV1>,
    #[serde(default)]
    combat_reward: Option<CombatRewardIdentityV1>,
    #[serde(default)]
    card_reward: Option<ObservedCardRewardV1>,
    #[serde(default)]
    pub(super) event_option: Option<ObservedEventOptionV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObservedEventOptionV1 {
    pub(super) action_kind: ObservedEventActionKindV1,
    #[serde(default)]
    pub(super) effect_summary: Option<String>,
    pub(super) followup: ObservedEventFollowupV1,
    pub(super) repeatable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ObservedEventActionKindV1 {
    Unknown,
    Leave,
    Continue,
    Accept,
    Decline,
    Fight,
    Trade,
    DeckOperation,
    Gain,
    Special,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ObservedEventFollowupV1 {
    None,
    EventScreen,
    EventComplete,
    RemoveCardSelection,
    UpgradeCardSelection,
    TransformCardSelection,
    DuplicateCardSelection,
    OfferCardSelection,
    UnknownSelection,
    Reward,
    Combat,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActionAuthorizationV1 {
    pub(super) handle_id: String,
    pub(super) capability: CapabilityProofV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ObservedCardV1 {
    card_id: String,
    upgrades: u8,
    cost_for_turn: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ObservedPotionV1 {
    potion_id: String,
    potency: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ObservedCardRewardV1 {
    Card { card_id: String, upgrades: u8 },
    Skip,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VerifiedActionScopeV1 {
    pub action_space_id: String,
}

pub(super) struct ObservedRootV1 {
    pub root_id: String,
    pub action_space_id: String,
    pub detail_handle: String,
    pub manifest: Option<RenderedManifestV1>,
}

pub(super) fn observe_explicit_root(
    repository: &Path,
    root_id: &str,
    request_count: u64,
    authority: ManifestAuthorityV1,
) -> Result<ObservedRootV1, String> {
    let report = load_observation_report(repository, root_id, request_count)?;
    let manifest = render_manifest_candidate(repository, &report, authority)?;
    let document = serde_json::to_value(&report)
        .map_err(|error| format!("failed to encode observation artifact: {error}"))?;
    let detail_handle = publish_artifact(repository, ACTION_OBSERVATION_ARTIFACT_TYPE, &document)?;
    let root_id = report.root_id.clone();
    let action_space_id = report.action_space_id.clone();
    Ok(ObservedRootV1 {
        root_id,
        action_space_id,
        detail_handle,
        manifest,
    })
}

fn render_manifest_candidate(
    repository: &Path,
    report: &ActionObservationReportV6,
    authority: ManifestAuthorityV1,
) -> Result<Option<RenderedManifestV1>, String> {
    let store = StateRootStoreV2::new(repository.join(".oracle-lab").join("paired-root-store"))
        .with_operation_verification_cache();
    match report.decision_context.profile {
        DecisionProfileV1::MapNavigation => {
            render_map_observation_v1(&store, report, authority).map(Some)
        }
        DecisionProfileV1::CardReward => {
            render_card_reward_observation_v1(&store, report, authority).map(Some)
        }
        DecisionProfileV1::CombatReward => {
            render_combat_reward_observation_v1(&store, report, authority).map(Some)
        }
        DecisionProfileV1::Neow => render_neow_observation_v1(&store, report, authority).map(Some),
        DecisionProfileV1::CombatPlayerDecision => {
            render_combat_observation_v1(&store, report, authority)
        }
        DecisionProfileV1::Event => {
            render_event_observation_v1(&store, report, authority).map(Some)
        }
    }
}

pub(super) fn require_verified_actions(
    repository: &Path,
    root_id: &str,
    action_ids: &[String],
    request_count: u64,
) -> Result<VerifiedActionScopeV1, String> {
    if action_ids.is_empty() || action_ids.len() > 64 {
        return Err("preview requires between one and sixty-four actions".to_owned());
    }
    let mut distinct = BTreeSet::new();
    if action_ids.iter().any(|action_id| {
        !content_id_has_shape(action_id, "paired-action-v1") || !distinct.insert(action_id)
    }) {
        return Err("preview requires distinct typed PairedRoot action identities".to_owned());
    }
    let report = load_observation_report(repository, root_id, request_count)?;
    for action_id in action_ids {
        let mut observed_matches = report
            .observed_actions
            .iter()
            .filter(|observed| observed.handle.handle_id == *action_id);
        observed_matches
            .next()
            .ok_or_else(|| "preview action is absent from the requested root".to_owned())?;
        if observed_matches.next().is_some() {
            return Err("preview action identity is duplicated in its action space".to_owned());
        }
        let mut authorization_matches = report
            .authorizations
            .iter()
            .filter(|authorization| authorization.handle_id == *action_id);
        let authorization = authorization_matches
            .next()
            .ok_or_else(|| "preview action has no authorization decision".to_owned())?;
        if authorization_matches.next().is_some() {
            return Err("preview action authorization is duplicated".to_owned());
        }
        match &authorization.capability.status {
            CapabilityStatusV1::Verified { .. } => {}
            CapabilityStatusV1::Unknown { gap_id } => {
                return Err(format!("preview action capability is Unknown: {gap_id}"));
            }
            CapabilityStatusV1::Unsupported { reason } => {
                return Err(format!(
                    "preview action capability is Unsupported: {reason}"
                ));
            }
        }
    }
    Ok(VerifiedActionScopeV1 {
        action_space_id: report.action_space_id,
    })
}

fn load_observation_report(
    repository: &Path,
    root_id: &str,
    request_count: u64,
) -> Result<ActionObservationReportV6, String> {
    if !content_id_has_shape(root_id, "state-root-v2") {
        return Err("explicit observation requires a typed PairedRoot identity".to_owned());
    }
    let attempt = create_operation_directory(repository, "observe", request_count)?;
    let report_path = attempt.join("actions.json");
    let arguments = vec![
        "paired-root-observe-actions".to_owned(),
        "--root".to_owned(),
        root_id.to_owned(),
        "--store".to_owned(),
        repository
            .join(".oracle-lab")
            .join("paired-root-store")
            .to_string_lossy()
            .into_owned(),
        "--report".to_owned(),
        report_path.to_string_lossy().into_owned(),
    ];
    run_typed_adapter(
        repository,
        &attempt,
        "paired-root-observe-actions",
        &arguments,
        Duration::from_secs(30),
    )?;
    let encoded = fs::read(&report_path).map_err(|error| {
        format!(
            "paired-root observation omitted {}: {error}",
            report_path.display()
        )
    })?;
    let report: ActionObservationReportV6 = serde_json::from_slice(&encoded)
        .map_err(|error| format!("paired-root observation is invalid JSON evidence: {error}"))?;
    validate_observation_scope(&report, root_id)?;
    Ok(report)
}

fn validate_observation_scope(
    report: &ActionObservationReportV6,
    requested_root_id: &str,
) -> Result<(), String> {
    if report.schema != ACTION_OBSERVATION_REPORT_SCHEMA_V6
        || report.root_id != requested_root_id
        || !content_id_has_shape(&report.action_space_id, "paired-action-space-v1")
    {
        return Err("paired-root observation returned evidence for the wrong scope".to_owned());
    }
    let mut seen = BTreeSet::new();
    for observed in &report.observed_actions {
        let handle = &observed.handle;
        if !content_id_has_shape(&handle.handle_id, "paired-action-v1")
            || handle.source_root_id != report.root_id
            || handle.action_space_id != report.action_space_id
            || !seen.insert(handle.handle_id.as_str())
        {
            return Err(
                "paired-root observation contains an unbound or duplicate handle".to_owned(),
            );
        }
        validate_action_observation(observed)?;
    }
    let observed_ids = seen.clone();
    let mut authorization_ids = BTreeSet::new();
    for authorization in &report.authorizations {
        if !authorization_ids.insert(authorization.handle_id.as_str()) {
            return Err("paired-root observation repeats an action authorization".to_owned());
        }
        let observed = report
            .observed_actions
            .iter()
            .find(|observed| observed.handle.handle_id == authorization.handle_id)
            .ok_or_else(|| "action authorization has no matching observed action".to_owned())?;
        if !capability_matches_action(authorization.capability.capability, &observed.handle.action)
        {
            return Err(format!(
                "action {} carries the wrong capability proof",
                observed.handle.handle_id
            ));
        }
        validate_capability_status(&authorization.capability.status)?;
    }
    if authorization_ids != observed_ids {
        return Err(
            "observed actions and authorization decisions do not have one-to-one coverage"
                .to_owned(),
        );
    }
    Ok(())
}

fn capability_matches_action(capability: CapabilityKindV1, action: &PairedActionV1) -> bool {
    capability == capability_for_action(action)
        || capability == CapabilityKindV1::ChooseMapNode
            && matches!(
                action,
                PairedActionV1::ChooseMapNode {
                    room_type: MapRoomIdentityV1::QuestionMark,
                    ..
                }
            )
        || capability == CapabilityKindV1::ResolveCardReward
            && matches!(
                action,
                PairedActionV1::ClaimCombatReward { .. } | PairedActionV1::CloseCombatReward
            )
}

fn capability_for_action(action: &PairedActionV1) -> CapabilityKindV1 {
    match action {
        PairedActionV1::ChooseNeowOption { reward, drawback } => {
            neow_capability(*reward, *drawback)
        }
        PairedActionV1::LeaveNeow => CapabilityKindV1::LeaveNeow,
        PairedActionV1::PlayCard { .. } => CapabilityKindV1::PlayCard,
        PairedActionV1::EndTurn => CapabilityKindV1::EndTurn,
        PairedActionV1::UsePotion { .. } => CapabilityKindV1::UsePotion,
        PairedActionV1::ClaimCombatReward { .. } | PairedActionV1::CloseCombatReward => {
            CapabilityKindV1::ResolveCombatReward
        }
        PairedActionV1::ChooseCardReward { .. } | PairedActionV1::SkipCardReward => {
            CapabilityKindV1::ResolveCardReward
        }
        PairedActionV1::ChooseEventOption { .. } => CapabilityKindV1::ResolveEvent,
        PairedActionV1::ChooseMapNode { room_type, .. } => match room_type {
            MapRoomIdentityV1::Monster => CapabilityKindV1::EnterCombat,
            MapRoomIdentityV1::QuestionMark => CapabilityKindV1::EnterQuestionMark,
            MapRoomIdentityV1::Elite
            | MapRoomIdentityV1::Boss
            | MapRoomIdentityV1::Rest
            | MapRoomIdentityV1::Shop
            | MapRoomIdentityV1::Treasure
            | MapRoomIdentityV1::Victory
            | MapRoomIdentityV1::Neow => CapabilityKindV1::ChooseMapNode,
        },
    }
}

fn neow_capability(
    reward: NeowRewardIdentityV1,
    drawback: NeowDrawbackIdentityV1,
) -> CapabilityKindV1 {
    match (reward, drawback) {
        (NeowRewardIdentityV1::HundredGold, NeowDrawbackIdentityV1::None) => {
            CapabilityKindV1::ChooseNeowHundredGoldNoDrawback
        }
        (NeowRewardIdentityV1::ThreeEnemyKill, NeowDrawbackIdentityV1::None) => {
            CapabilityKindV1::ChooseNeowThreeEnemyKillNoDrawback
        }
        (
            NeowRewardIdentityV1::RemoveCard
            | NeowRewardIdentityV1::UpgradeCard
            | NeowRewardIdentityV1::TransformCard
            | NeowRewardIdentityV1::RemoveTwo
            | NeowRewardIdentityV1::TransformTwoCards,
            _,
        ) => CapabilityKindV1::ChooseNeowDeckSelection,
        (
            NeowRewardIdentityV1::ThreeCards
            | NeowRewardIdentityV1::RandomColorless
            | NeowRewardIdentityV1::ThreeSmallPotions
            | NeowRewardIdentityV1::RandomColorless2
            | NeowRewardIdentityV1::ThreeRareCards,
            _,
        ) => CapabilityKindV1::ChooseNeowReward,
        (
            NeowRewardIdentityV1::ThreeEnemyKill
            | NeowRewardIdentityV1::TenPercentHpBonus
            | NeowRewardIdentityV1::OneRandomRareCard
            | NeowRewardIdentityV1::RandomCommonRelic
            | NeowRewardIdentityV1::HundredGold
            | NeowRewardIdentityV1::OneRareRelic
            | NeowRewardIdentityV1::TwoFiftyGold
            | NeowRewardIdentityV1::TwentyPercentHpBonus
            | NeowRewardIdentityV1::BossRelic,
            _,
        ) => CapabilityKindV1::ChooseNeowClosed,
    }
}

fn validate_capability_status(status: &CapabilityStatusV1) -> Result<(), String> {
    match status {
        CapabilityStatusV1::Verified { proof }
            if is_sha256(&proof.sha256)
                && proof.bytes > 0
                && !proof.media_type.trim().is_empty() =>
        {
            Ok(())
        }
        CapabilityStatusV1::Unknown { gap_id } if !gap_id.trim().is_empty() => Ok(()),
        CapabilityStatusV1::Unsupported { reason } if !reason.trim().is_empty() => Ok(()),
        _ => Err("action capability proof has invalid typed evidence".to_owned()),
    }
}

fn validate_action_observation(observed: &ObservedActionV1) -> Result<(), String> {
    let valid = match (
        &observed.handle.action,
        &observed.card,
        &observed.potion,
        &observed.combat_reward,
        &observed.card_reward,
        &observed.event_option,
    ) {
        (PairedActionV1::PlayCard { .. }, Some(_), None, None, None, None) => true,
        (PairedActionV1::ChooseNeowOption { .. }, None, None, None, None, None)
        | (PairedActionV1::LeaveNeow, None, None, None, None, None) => true,
        (
            PairedActionV1::UsePotion {
                potion_id, potency, ..
            },
            None,
            Some(observed),
            None,
            None,
            None,
        ) => observed.potion_id == *potion_id && observed.potency == *potency,
        (PairedActionV1::EndTurn, None, None, None, None, None) => true,
        (
            PairedActionV1::ClaimCombatReward { reward, .. },
            None,
            None,
            Some(observed),
            None,
            None,
        ) => observed == reward,
        (
            PairedActionV1::ChooseCardReward {
                card_id, upgrades, ..
            },
            None,
            None,
            None,
            Some(ObservedCardRewardV1::Card {
                card_id: observed_id,
                upgrades: observed_upgrades,
            }),
            None,
        ) => observed_id == card_id && observed_upgrades == upgrades,
        (
            PairedActionV1::SkipCardReward,
            None,
            None,
            None,
            Some(ObservedCardRewardV1::Skip),
            None,
        ) => true,
        (PairedActionV1::CloseCombatReward, None, None, None, None, None) => true,
        (PairedActionV1::ChooseEventOption { .. }, None, None, None, None, Some(_)) => true,
        (PairedActionV1::ChooseMapNode { .. }, None, None, None, None, None) => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "action {} has inconsistent typed observation fields",
            observed.handle.handle_id
        ))
    }
}

fn content_id_has_shape(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|tail| tail.strip_prefix('-'))
        .is_some_and(is_sha256)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sts_paired_root::{
        BlobRefV1, MapRoomIdentityV1, NeowDrawbackIdentityV1, NeowRewardIdentityV1,
    };

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn capability(status: CapabilityStatusV1, kind: CapabilityKindV1) -> CapabilityProofV1 {
        CapabilityProofV1 {
            capability: kind,
            required_domains: Vec::new(),
            status,
        }
    }

    #[test]
    fn validates_verified_and_unknown_actions_without_conflating_capabilities() {
        let root_id = format!("state-root-v2-{}", digest('a'));
        let action_space_id = format!("paired-action-space-v1-{}", digest('b'));
        let end_turn_id = format!("paired-action-v1-{}", digest('c'));
        let map_id = format!("paired-action-v1-{}", digest('e'));
        let neow_id = format!("paired-action-v1-{}", digest('f'));
        let event_id = format!("paired-action-v1-{}", digest('2'));
        let report = ActionObservationReportV6 {
            schema: ACTION_OBSERVATION_REPORT_SCHEMA_V6.to_owned(),
            root_id: root_id.clone(),
            action_space_id: action_space_id.clone(),
            decision_context: DecisionContextV1 {
                profile: DecisionProfileV1::Neow,
                run: json!({"act": 1, "floor": 0}),
                master_deck: json!([]),
                relics: json!([]),
                potions: json!([]),
                map: Some(json!({"current_x": -1, "current_y": -1, "nodes": []})),
                room: None,
            },
            observed_actions: vec![
                ObservedActionV1 {
                    handle: PairedActionHandleV1 {
                        handle_id: end_turn_id.clone(),
                        source_root_id: root_id.clone(),
                        action_space_id: action_space_id.clone(),
                        action: PairedActionV1::EndTurn,
                    },
                    card: None,
                    potion: None,
                    combat_reward: None,
                    card_reward: None,
                    event_option: None,
                },
                ObservedActionV1 {
                    handle: PairedActionHandleV1 {
                        handle_id: event_id.clone(),
                        source_root_id: root_id.clone(),
                        action_space_id: action_space_id.clone(),
                        action: PairedActionV1::ChooseEventOption {
                            event_id: "ShiningLight".to_owned(),
                            screen: 0,
                            option_index: 1,
                        },
                    },
                    card: None,
                    potion: None,
                    combat_reward: None,
                    card_reward: None,
                    event_option: Some(ObservedEventOptionV1 {
                        action_kind: ObservedEventActionKindV1::Gain,
                        effect_summary: Some("gain 100 gold".to_owned()),
                        followup: ObservedEventFollowupV1::EventScreen,
                        repeatable: false,
                    }),
                },
                ObservedActionV1 {
                    handle: PairedActionHandleV1 {
                        handle_id: map_id.clone(),
                        source_root_id: root_id.clone(),
                        action_space_id: action_space_id.clone(),
                        action: PairedActionV1::ChooseMapNode {
                            x: 2,
                            y: 3,
                            room_type: MapRoomIdentityV1::QuestionMark,
                        },
                    },
                    card: None,
                    potion: None,
                    combat_reward: None,
                    card_reward: None,
                    event_option: None,
                },
                ObservedActionV1 {
                    handle: PairedActionHandleV1 {
                        handle_id: neow_id.clone(),
                        source_root_id: root_id.clone(),
                        action_space_id: action_space_id.clone(),
                        action: PairedActionV1::ChooseNeowOption {
                            reward: NeowRewardIdentityV1::HundredGold,
                            drawback: NeowDrawbackIdentityV1::None,
                        },
                    },
                    card: None,
                    potion: None,
                    combat_reward: None,
                    card_reward: None,
                    event_option: None,
                },
            ],
            authorizations: vec![
                ActionAuthorizationV1 {
                    handle_id: end_turn_id,
                    capability: capability(
                        CapabilityStatusV1::Verified {
                            proof: BlobRefV1 {
                                sha256: digest('d'),
                                bytes: 12,
                                media_type: "application/json".to_owned(),
                            },
                        },
                        CapabilityKindV1::EndTurn,
                    ),
                },
                ActionAuthorizationV1 {
                    handle_id: map_id,
                    capability: capability(
                        CapabilityStatusV1::Unknown {
                            gap_id: "map-transition-gap".to_owned(),
                        },
                        CapabilityKindV1::ChooseMapNode,
                    ),
                },
                ActionAuthorizationV1 {
                    handle_id: neow_id,
                    capability: capability(
                        CapabilityStatusV1::Verified {
                            proof: BlobRefV1 {
                                sha256: digest('1'),
                                bytes: 12,
                                media_type: "application/json".to_owned(),
                            },
                        },
                        CapabilityKindV1::ChooseNeowHundredGoldNoDrawback,
                    ),
                },
                ActionAuthorizationV1 {
                    handle_id: event_id,
                    capability: capability(
                        CapabilityStatusV1::Verified {
                            proof: BlobRefV1 {
                                sha256: digest('3'),
                                bytes: 12,
                                media_type: "application/json".to_owned(),
                            },
                        },
                        CapabilityKindV1::ResolveEvent,
                    ),
                },
            ],
        };
        validate_observation_scope(&report, &root_id).unwrap();
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseMapNode {
                x: 0,
                y: 0,
                room_type: MapRoomIdentityV1::Monster,
            }),
            CapabilityKindV1::EnterCombat
        );
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseMapNode {
                x: 1,
                y: 1,
                room_type: MapRoomIdentityV1::Elite,
            }),
            CapabilityKindV1::ChooseMapNode
        );
    }
}
