use crate::domain_manifest::{
    render_parity_lab_map_v1, ExactTrailV1, ManifestAuthorityV1, MapActionRouteV1, MapActionV1,
    MapCoordinateV1, MapRoomTagV1, ParityLabMapInputV1, PublicMapNodeV1, PublicMapV1,
    RenderedManifestV1, TrailPointV1,
};
use crate::observation::ActionObservationReportV6;
use crate::scene_manifest_adapter::{
    project_action_availability_v1, project_shared_scene_v1, require_projection_schema_v1,
    validate_root_scope_v1, RUN_SCALARS_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use sts_paired_root::{
    DecisionProfileV1, MapRoomIdentityV1, PairedActionV1, ProjectionDomainV1, StateRootArtifactV2,
    StateRootStoreV2,
};

pub(super) const PUBLIC_MAP_SCHEMA_V1: &str = "sts-paired-public-map-v1";
const TRAIL_GAP_ID: &str = "map-trail-history-gap";
const TRAIL_GAP_REASON: &str = "exact_history_not_projected";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
struct RunPositionV1 {
    act: u8,
    floor: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredPublicMapV1 {
    current_x: i32,
    current_y: i32,
    boss_id: Option<String>,
    boss_anchor: Option<StoredMapEdgeV1>,
    nodes: Vec<StoredMapNodeV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredMapNodeV1 {
    x: i32,
    y: i32,
    room_type: MapRoomIdentityV1,
    has_emerald_key: bool,
    edges: Vec<StoredMapEdgeV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredMapEdgeV1 {
    destination_x: i32,
    destination_y: i32,
}

pub(super) struct ProjectedMapContextV1 {
    pub map: PublicMapV1,
    pub trail: ExactTrailV1,
}

pub(super) fn render_map_observation_v1(
    store: &StateRootStoreV2,
    report: &ActionObservationReportV6,
    authority: ManifestAuthorityV1,
) -> Result<RenderedManifestV1, String> {
    if report.decision_context.profile != DecisionProfileV1::MapNavigation {
        return Err("Map manifest adapter accepts MapNavigation only".to_owned());
    }
    let root = store.load_root(&report.root_id)?;
    validate_root_scope_v1(&root, report)?;
    let shared = project_shared_scene_v1(&root, report, authority)?;
    let map_context = project_map_context_v1(store, &root, report, shared.run.act)?;
    let actions = project_actions(report, &map_context.map)?;
    render_parity_lab_map_v1(&ParityLabMapInputV1 {
        shared,
        map: map_context.map,
        trail: map_context.trail,
        actions,
    })
}

pub(super) fn project_map_context_v1(
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
    report: &ActionObservationReportV6,
    act: u8,
) -> Result<ProjectedMapContextV1, String> {
    require_projection_schema_v1(root, ProjectionDomainV1::Map, PUBLIC_MAP_SCHEMA_V1)?;
    let stored_map: StoredPublicMapV1 = serde_json::from_value(
        report
            .decision_context
            .map
            .clone()
            .ok_or_else(|| "scene observation omitted its Exact map".to_owned())?,
    )
    .map_err(|error| format!("Exact map projection has an invalid schema: {error}"))?;
    let map = project_topology(&stored_map)?;
    let trail = exact_trail(store, root, &stored_map, act)?;
    Ok(ProjectedMapContextV1 { map, trail })
}

fn project_topology(stored: &StoredPublicMapV1) -> Result<PublicMapV1, String> {
    let current = stored_current(stored)?;
    let boss_anchor = stored
        .boss_anchor
        .map(|edge| coordinate(edge.destination_x, edge.destination_y));
    let coordinates = stored
        .nodes
        .iter()
        .map(|node| coordinate(node.x, node.y))
        .collect::<BTreeSet<_>>();
    if coordinates.len() != stored.nodes.len() {
        return Err("Exact map projection repeats a node coordinate".to_owned());
    }
    let incoming = stored
        .nodes
        .iter()
        .flat_map(|node| &node.edges)
        .map(|edge| coordinate(edge.destination_x, edge.destination_y))
        .filter(|target| coordinates.contains(target))
        .collect::<BTreeSet<_>>();
    let nodes = stored
        .nodes
        .iter()
        .filter(|node| {
            let point = coordinate(node.x, node.y);
            !node.edges.is_empty() || incoming.contains(&point) || current == Some(point)
        })
        .map(|node| {
            Ok(PublicMapNodeV1 {
                coordinate: coordinate(node.x, node.y),
                room: room_tag(node.room_type)?,
                burning: node.has_emerald_key,
                edges: node
                    .edges
                    .iter()
                    .map(|edge| coordinate(edge.destination_x, edge.destination_y))
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PublicMapV1 {
        current,
        boss_id: stored.boss_id.clone(),
        boss_anchor,
        nodes,
    })
}

fn project_actions(
    report: &ActionObservationReportV6,
    map: &PublicMapV1,
) -> Result<Vec<MapActionV1>, String> {
    if report.observed_actions.len() != report.authorizations.len() {
        return Err("map observation does not bind every action authorization".to_owned());
    }
    report
        .observed_actions
        .iter()
        .map(|observed| {
            let PairedActionV1::ChooseMapNode { x, y, room_type } = &observed.handle.action else {
                return Err("MapNavigation observation exposed a non-map action".to_owned());
            };
            let target = coordinate(*x, *y);
            require_action_room(map, target, *room_type)?;
            Ok(MapActionV1 {
                action_id: observed.handle.handle_id.clone(),
                target,
                route: MapActionRouteV1::GraphEdge,
                availability: project_action_availability_v1(report, &observed.handle.handle_id)?,
            })
        })
        .collect()
}

fn require_action_room(
    map: &PublicMapV1,
    target: MapCoordinateV1,
    room: MapRoomIdentityV1,
) -> Result<(), String> {
    if map.boss_anchor.is_some_and(|anchor| anchor == target) {
        return (room == MapRoomIdentityV1::Boss)
            .then_some(())
            .ok_or_else(|| "map boss action carries the wrong room identity".to_owned());
    }
    let node = map
        .nodes
        .iter()
        .find(|node| node.coordinate == target)
        .ok_or_else(|| "map action targets an absent projected node".to_owned())?;
    if node.room != room_tag(room)? {
        return Err("map action room identity disagrees with the Exact topology".to_owned());
    }
    Ok(())
}

fn exact_trail(
    store: &StateRootStoreV2,
    target_root: &StateRootArtifactV2,
    target_map: &StoredPublicMapV1,
    target_act: u8,
) -> Result<ExactTrailV1, String> {
    if stored_current(target_map)?.is_none() {
        return Ok(ExactTrailV1::Available(vec![TrailPointV1::Start]));
    }
    let chain = match store.logical_ancestry(&target_root.root_id) {
        Ok(chain) => chain,
        Err(_) => return Ok(trail_gap()),
    };
    let mut acts = Vec::with_capacity(chain.len());
    for root_id in &chain {
        let root = store.load_root(root_id)?;
        require_projection_schema_v1(&root, ProjectionDomainV1::RunScalars, RUN_SCALARS_SCHEMA_V1)?;
        let run: RunPositionV1 = store
            .load_exact_projection(root_id, ProjectionDomainV1::RunScalars)
            .map_err(|error| format!("trail ancestor omitted Exact run position: {error}"))?;
        acts.push(run.act);
    }
    let Some(first_current) = acts.iter().position(|act| *act == target_act) else {
        return Ok(trail_gap());
    };
    if acts[first_current..].iter().any(|act| *act != target_act) {
        return Ok(trail_gap());
    }
    let first_root = store.load_root(&chain[first_current])?;
    if first_root
        .root
        .preflight
        .components
        .iter()
        .all(|component| component.domain != ProjectionDomainV1::Map)
    {
        return Ok(trail_gap());
    }
    let first_map: StoredPublicMapV1 =
        store.load_exact_projection(&chain[first_current], ProjectionDomainV1::Map)?;
    if stored_current(&first_map)?.is_some() {
        return Ok(trail_gap());
    }

    let boss_anchor = target_map
        .boss_anchor
        .map(|edge| coordinate(edge.destination_x, edge.destination_y));
    let mut points = vec![TrailPointV1::Start];
    for source_ordinal in first_current..chain.len().saturating_sub(1) {
        if acts[source_ordinal] != target_act || acts[source_ordinal + 1] != target_act {
            continue;
        }
        let child = store.load_root(&chain[source_ordinal + 1])?;
        let parent = child
            .root
            .parent
            .as_ref()
            .ok_or_else(|| "trail child unexpectedly omitted its parent".to_owned())?;
        let commitment = store.load_transition_commitment(&parent.transition_commitment_id)?;
        if let PairedActionV1::ChooseMapNode { x, y, .. } = &commitment.transition.action.action {
            let point = coordinate(*x, *y);
            points.push(if Some(point) == boss_anchor {
                TrailPointV1::Boss
            } else {
                TrailPointV1::Node(point)
            });
        }
    }
    let expected = match stored_current(target_map)? {
        None => TrailPointV1::Start,
        Some(point) if Some(point) == boss_anchor => TrailPointV1::Boss,
        Some(point) => TrailPointV1::Node(point),
    };
    if points.last().copied() != Some(expected) {
        return Ok(trail_gap());
    }
    Ok(ExactTrailV1::Available(points))
}

fn trail_gap() -> ExactTrailV1 {
    ExactTrailV1::CoverageGap {
        gap_id: TRAIL_GAP_ID.to_owned(),
        reason: TRAIL_GAP_REASON.to_owned(),
    }
}

fn stored_current(map: &StoredPublicMapV1) -> Result<Option<MapCoordinateV1>, String> {
    match (map.current_x, map.current_y) {
        (-1, -1) => Ok(None),
        (x, y) if x >= 0 && y >= 0 => Ok(Some(coordinate(x, y))),
        _ => Err("Exact map projection has a partial or negative current coordinate".to_owned()),
    }
}

fn coordinate(x: i32, y: i32) -> MapCoordinateV1 {
    MapCoordinateV1 { x, y }
}

fn room_tag(room: MapRoomIdentityV1) -> Result<MapRoomTagV1, String> {
    match room {
        MapRoomIdentityV1::QuestionMark => Ok(MapRoomTagV1::Question),
        MapRoomIdentityV1::Monster => Ok(MapRoomTagV1::Monster),
        MapRoomIdentityV1::Elite => Ok(MapRoomTagV1::Elite),
        MapRoomIdentityV1::Boss => Ok(MapRoomTagV1::Boss),
        MapRoomIdentityV1::Rest => Ok(MapRoomTagV1::Rest),
        MapRoomIdentityV1::Shop => Ok(MapRoomTagV1::Shop),
        MapRoomIdentityV1::Treasure => Ok(MapRoomTagV1::Treasure),
        MapRoomIdentityV1::Victory => Ok(MapRoomTagV1::Victory),
        MapRoomIdentityV1::Neow => {
            Err("Neow cannot appear as a public map topology node".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::paired_store_fixture;
    use serde_json::json;
    use sts_paired_root::{
        action_handle, CapabilityKindV1, CapabilityProofV1, CapabilityStatusV1, ComparisonStatusV1,
        ProjectionComparisonV1,
    };

    fn stored_map() -> StoredPublicMapV1 {
        StoredPublicMapV1 {
            current_x: 2,
            current_y: 1,
            boss_id: Some("hexaghost".to_owned()),
            boss_anchor: Some(StoredMapEdgeV1 {
                destination_x: 3,
                destination_y: 16,
            }),
            nodes: vec![
                StoredMapNodeV1 {
                    x: 0,
                    y: 0,
                    room_type: MapRoomIdentityV1::Monster,
                    has_emerald_key: false,
                    edges: Vec::new(),
                },
                StoredMapNodeV1 {
                    x: 1,
                    y: 0,
                    room_type: MapRoomIdentityV1::Monster,
                    has_emerald_key: false,
                    edges: vec![StoredMapEdgeV1 {
                        destination_x: 2,
                        destination_y: 1,
                    }],
                },
                StoredMapNodeV1 {
                    x: 2,
                    y: 1,
                    room_type: MapRoomIdentityV1::QuestionMark,
                    has_emerald_key: false,
                    edges: vec![StoredMapEdgeV1 {
                        destination_x: 2,
                        destination_y: 2,
                    }],
                },
                StoredMapNodeV1 {
                    x: 2,
                    y: 2,
                    room_type: MapRoomIdentityV1::Rest,
                    has_emerald_key: false,
                    edges: vec![StoredMapEdgeV1 {
                        destination_x: 3,
                        destination_y: 16,
                    }],
                },
            ],
        }
    }

    #[test]
    fn exact_projection_grid_placeholders_are_not_topology_members() {
        let map = project_topology(&stored_map()).unwrap();
        assert_eq!(map.nodes.len(), 3);
        assert!(!map
            .nodes
            .iter()
            .any(|node| node.coordinate == coordinate(0, 0)));
    }

    #[test]
    fn exact_map_observation_becomes_one_hash_free_manifest() {
        let fixture = paired_store_fixture();
        let mut map = stored_map();
        map.current_x = -1;
        map.current_y = -1;
        let run_value = json!({
            "seed": "1",
            "ascension": 20,
            "character": "IRONCLAD",
            "act": 1,
            "floor": 0,
            "current_hp": 75,
            "max_hp": 75,
            "gold": 99,
            "final_act_available": true,
            "keys": [false, false, false]
        });
        let map_value = serde_json::to_value(&map).unwrap();
        let run_blob = fixture
            .store
            .put_bytes("application/json", &serde_json::to_vec(&run_value).unwrap())
            .unwrap();
        let map_blob = fixture
            .store
            .put_bytes("application/json", &serde_json::to_vec(&map_value).unwrap())
            .unwrap();
        let empty_blob = fixture
            .store
            .put_bytes("application/json", &serde_json::to_vec(&json!([])).unwrap())
            .unwrap();
        let mut root_body = fixture.source.root.clone();
        root_body.decision_profile = DecisionProfileV1::MapNavigation;
        root_body.preflight.action_space_id = "action-space:map-manifest-fixture".to_owned();
        root_body.preflight.components = vec![
            ProjectionComparisonV1 {
                domain: ProjectionDomainV1::RunScalars,
                projection_schema: RUN_SCALARS_SCHEMA_V1.to_owned(),
                java: run_blob.clone(),
                rust: run_blob,
                status: ComparisonStatusV1::Exact,
                first_difference: None,
            },
            ProjectionComparisonV1 {
                domain: ProjectionDomainV1::Map,
                projection_schema: PUBLIC_MAP_SCHEMA_V1.to_owned(),
                java: map_blob.clone(),
                rust: map_blob.clone(),
                status: ComparisonStatusV1::Exact,
                first_difference: None,
            },
            ProjectionComparisonV1 {
                domain: ProjectionDomainV1::MasterDeck,
                projection_schema: crate::scene_manifest_adapter::MASTER_DECK_SCHEMA_V1.to_owned(),
                java: empty_blob.clone(),
                rust: empty_blob.clone(),
                status: ComparisonStatusV1::Exact,
                first_difference: None,
            },
            ProjectionComparisonV1 {
                domain: ProjectionDomainV1::Relics,
                projection_schema: crate::scene_manifest_adapter::RELICS_SCHEMA_V1.to_owned(),
                java: empty_blob.clone(),
                rust: empty_blob.clone(),
                status: ComparisonStatusV1::Exact,
                first_difference: None,
            },
            ProjectionComparisonV1 {
                domain: ProjectionDomainV1::Potions,
                projection_schema: crate::scene_manifest_adapter::PERSISTENT_POTIONS_SCHEMA_V1
                    .to_owned(),
                java: empty_blob.clone(),
                rust: empty_blob,
                status: ComparisonStatusV1::Exact,
                first_difference: None,
            },
        ];
        root_body.capabilities = vec![
            CapabilityProofV1 {
                capability: CapabilityKindV1::Observe,
                required_domains: vec![ProjectionDomainV1::RunScalars, ProjectionDomainV1::Map],
                status: CapabilityStatusV1::Verified {
                    proof: map_blob.clone(),
                },
            },
            CapabilityProofV1 {
                capability: CapabilityKindV1::EnterCombat,
                required_domains: vec![ProjectionDomainV1::RunScalars, ProjectionDomainV1::Map],
                status: CapabilityStatusV1::Verified {
                    proof: map_blob.clone(),
                },
            },
        ];
        let root = fixture.store.create_root(root_body).unwrap();
        let action = action_handle(
            &root.root_id,
            &root.root.preflight.action_space_id,
            PairedActionV1::ChooseMapNode {
                x: 1,
                y: 0,
                room_type: MapRoomIdentityV1::Monster,
            },
        )
        .unwrap();
        let report: ActionObservationReportV6 = serde_json::from_value(json!({
            "schema": "sts-paired-root-action-observation-v6",
            "root_id": root.root_id,
            "action_space_id": root.root.preflight.action_space_id,
            "decision_context": {
                "profile": "map_navigation",
                "run": run_value,
                "master_deck": [],
                "relics": [],
                "potions": [],
                "map": map_value
            },
            "observed_actions": [{"handle": action}],
            "authorizations": [{
                "handle_id": action.handle_id,
                "capability": {
                    "capability": "enter_combat",
                    "required_domains": ["run_scalars", "map"],
                    "status": {"status": "verified", "proof": map_blob}
                }
            }]
        }))
        .unwrap();

        let rendered =
            render_map_observation_v1(&fixture.store, &report, ManifestAuthorityV1::ObserveOnly)
                .unwrap();
        assert!(rendered.text.contains("world=natural"));
        assert!(rendered.text.contains("START -> F01X01:monster"));
        assert!(rendered.text.contains("A0 enter F01X01:monster"));
        assert!(!rendered.text.contains("paired-action-v1"));
        assert!(!rendered.text.contains("sha"));
        assert_eq!(rendered.actions[0].action_id, Some(action.handle_id));
    }
}
