use crate::domain_manifest::{ManifestAuthorityV1, RenderedManifestV1};
use crate::domain_manifest_adapter::project_map_context_v1;
use crate::event_manifest::{
    render_parity_lab_event_v1, EventActionV1, ParityLabEventInputV1, PublicEventV1,
};
use crate::observation::ActionObservationReportV6;
use crate::scene_manifest_adapter::{
    project_action_availability_v1, project_shared_scene_v1, require_projection_schema_v1,
    validate_root_scope_v1,
};
use serde::Deserialize;
use sts_paired_root::{DecisionProfileV1, PairedActionV1, ProjectionDomainV1, StateRootStoreV2};

const EVENT_ROOM_SCHEMA_V1: &str = "sts-paired-event-room-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StoredEventKindV1 {
    Event,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredEventRoomV1 {
    kind: StoredEventKindV1,
    event_id: String,
    screen: usize,
}

pub(super) fn render_event_observation_v1(
    store: &StateRootStoreV2,
    report: &ActionObservationReportV6,
    authority: ManifestAuthorityV1,
) -> Result<RenderedManifestV1, String> {
    if report.decision_context.profile != DecisionProfileV1::Event {
        return Err("Event manifest adapter accepts Event only".to_owned());
    }
    let root = store.load_root(&report.root_id)?;
    validate_root_scope_v1(&root, report)?;
    let shared = project_shared_scene_v1(&root, report, authority)?;
    let map_context = project_map_context_v1(store, &root, report, shared.run.act)?;
    require_projection_schema_v1(&root, ProjectionDomainV1::Room, EVENT_ROOM_SCHEMA_V1)?;
    let room: StoredEventRoomV1 = serde_json::from_value(
        report
            .decision_context
            .room
            .clone()
            .ok_or_else(|| "Event observation omitted its Exact room".to_owned())?,
    )
    .map_err(|error| format!("Exact Event room has an invalid schema: {error}"))?;
    let actions = project_actions_v1(report, &room)?;
    render_parity_lab_event_v1(&ParityLabEventInputV1 {
        shared,
        event: PublicEventV1 {
            event_id: room.event_id,
            screen: room.screen,
        },
        map: map_context.map,
        trail: map_context.trail,
        actions,
    })
}

fn project_actions_v1(
    report: &ActionObservationReportV6,
    room: &StoredEventRoomV1,
) -> Result<Vec<EventActionV1>, String> {
    if report.observed_actions.len() != report.authorizations.len() {
        return Err("Event observation does not bind every action authorization".to_owned());
    }
    report
        .observed_actions
        .iter()
        .map(|observed| {
            let option_index = project_option_index_v1(&observed.handle.action, room)?;
            Ok(EventActionV1 {
                action_id: observed.handle.handle_id.clone(),
                option_index,
                semantics: observed.event_option.clone().ok_or_else(|| {
                    "Event action omitted simulator-owned public semantics".to_owned()
                })?,
                availability: project_action_availability_v1(report, &observed.handle.handle_id)?,
            })
        })
        .collect()
}

fn project_option_index_v1(
    action: &PairedActionV1,
    room: &StoredEventRoomV1,
) -> Result<usize, String> {
    let PairedActionV1::ChooseEventOption {
        event_id,
        screen,
        option_index,
    } = action
    else {
        return Err("Event observation exposed an action from another scene".to_owned());
    };
    if event_id != &room.event_id || *screen != room.screen {
        return Err("Event action disagrees with its Exact event and screen".to_owned());
    }
    Ok(*option_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn room() -> StoredEventRoomV1 {
        serde_json::from_value(json!({
            "kind": "event",
            "event_id": "big_fish",
            "screen": 1
        }))
        .unwrap()
    }

    #[test]
    fn event_action_must_match_the_exact_room_identity() {
        let exact = PairedActionV1::ChooseEventOption {
            event_id: "big_fish".to_owned(),
            screen: 1,
            option_index: 2,
        };
        assert_eq!(project_option_index_v1(&exact, &room()).unwrap(), 2);

        let wrong_screen = PairedActionV1::ChooseEventOption {
            event_id: "big_fish".to_owned(),
            screen: 0,
            option_index: 2,
        };
        assert!(project_option_index_v1(&wrong_screen, &room())
            .unwrap_err()
            .contains("disagrees"));

        assert!(project_option_index_v1(&PairedActionV1::EndTurn, &room())
            .unwrap_err()
            .contains("another scene"));
    }
}
