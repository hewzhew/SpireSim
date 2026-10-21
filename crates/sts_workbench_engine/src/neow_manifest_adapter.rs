use crate::domain_manifest::{ManifestAuthorityV1, RenderedManifestV1};
use crate::domain_manifest_adapter::project_map_context_v1;
use crate::neow_manifest::{
    render_parity_lab_neow_v1, NeowActionKindV1, NeowActionV1, NeowStageV1, ParityLabNeowInputV1,
};
use crate::observation::ActionObservationReportV6;
use crate::scene_manifest_adapter::{
    project_action_availability_v1, project_shared_scene_v1, require_projection_schema_v1,
    validate_root_scope_v1,
};
use serde::Deserialize;
use sts_paired_root::{DecisionProfileV1, PairedActionV1, ProjectionDomainV1, StateRootStoreV2};

const NEOW_ROOM_SCHEMA_V1: &str = "sts-paired-neow-room-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StoredNeowKindV1 {
    Neow,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StoredNeowStageV1 {
    ChooseReward,
    Leave,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredNeowRoomV1 {
    kind: StoredNeowKindV1,
    stage: StoredNeowStageV1,
}

pub(super) fn render_neow_observation_v1(
    store: &StateRootStoreV2,
    report: &ActionObservationReportV6,
    authority: ManifestAuthorityV1,
) -> Result<RenderedManifestV1, String> {
    if report.decision_context.profile != DecisionProfileV1::Neow {
        return Err("Neow manifest adapter accepts Neow only".to_owned());
    }
    let root = store.load_root(&report.root_id)?;
    validate_root_scope_v1(&root, report)?;
    let shared = project_shared_scene_v1(&root, report, authority)?;
    let map_context = project_map_context_v1(store, &root, report, shared.run.act)?;
    require_projection_schema_v1(&root, ProjectionDomainV1::Room, NEOW_ROOM_SCHEMA_V1)?;
    let room: StoredNeowRoomV1 = serde_json::from_value(
        report
            .decision_context
            .room
            .clone()
            .ok_or_else(|| "Neow observation omitted its Exact room".to_owned())?,
    )
    .map_err(|error| format!("Exact Neow room has an invalid schema: {error}"))?;
    let actions = project_actions_v1(report)?;
    render_parity_lab_neow_v1(&ParityLabNeowInputV1 {
        shared,
        stage: match room.stage {
            StoredNeowStageV1::ChooseReward => NeowStageV1::ChooseReward,
            StoredNeowStageV1::Leave => NeowStageV1::Leave,
        },
        map: map_context.map,
        trail: map_context.trail,
        actions,
    })
}

fn project_actions_v1(report: &ActionObservationReportV6) -> Result<Vec<NeowActionV1>, String> {
    if report.observed_actions.len() != report.authorizations.len() {
        return Err("Neow observation does not bind every action authorization".to_owned());
    }
    report
        .observed_actions
        .iter()
        .map(|observed| {
            let kind = match observed.handle.action {
                PairedActionV1::ChooseNeowOption { reward, drawback } => {
                    NeowActionKindV1::Choose { reward, drawback }
                }
                PairedActionV1::LeaveNeow => NeowActionKindV1::Leave,
                _ => return Err("Neow observation exposed an action from another scene".to_owned()),
            };
            Ok(NeowActionV1 {
                action_id: observed.handle.handle_id.clone(),
                kind,
                availability: project_action_availability_v1(report, &observed.handle.handle_id)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_room_schema_keeps_reward_and_leave_stages_distinct() {
        let choose: StoredNeowRoomV1 = serde_json::from_value(json!({
            "kind": "neow",
            "stage": "choose_reward"
        }))
        .unwrap();
        let leave: StoredNeowRoomV1 = serde_json::from_value(json!({
            "kind": "neow",
            "stage": "leave"
        }))
        .unwrap();
        assert_eq!(choose.stage, StoredNeowStageV1::ChooseReward);
        assert_eq!(leave.stage, StoredNeowStageV1::Leave);
    }
}
