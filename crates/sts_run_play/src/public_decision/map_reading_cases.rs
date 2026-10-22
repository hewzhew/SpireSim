//! Explicitly constructed topology contracts and detached read-only audits.
use super::*;
use sts_oracle_learning::agent::task::TaskDecisionSiteV1;
use sts_oracle_learning::ai::planner_core::*;

fn node(x: i32, y: i32, room: PlannerMapRoom, edges: &[(i32, i32)]) -> PlannerPublicMapNode {
    PlannerPublicMapNode {
        x,
        y,
        room,
        has_emerald_key: false,
        edges: edges
            .iter()
            .map(|(x, y)| PlannerPublicMapEdge {
                destination_x: *x,
                destination_y: *y,
            })
            .collect(),
    }
}

fn route(x: i32, y: i32, flight: bool) -> TaskPublicCandidateV1 {
    TaskPublicCandidateV1::Strategic {
        action: TaskStrategicActionV1::ChooseRouteNode {
            x,
            y,
            flight,
            question_mark_outcomes: vec![],
        },
    }
}

fn cases() -> Vec<(&'static str, TaskPublicDecisionV1)> {
    use PlannerMapRoom::*;
    let specs = vec![
        (
            "entrance",
            vec![
                node(1, 0, Monster, &[(0, 1), (2, 1)]),
                node(0, 1, Rest, &[(0, 2)]),
                node(0, 2, Elite, &[(1, 3)]),
                node(2, 1, Elite, &[(2, 2)]),
                node(2, 2, Rest, &[(1, 3)]),
                node(1, 3, Shop, &[]),
                node(3, 0, Monster, &[(3, 1)]),
                node(3, 1, Shop, &[]),
            ],
            vec![
                route(1, 0, false),
                route(3, 0, false),
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::Cancel {
                        site: PlannerDecisionSite::Map,
                    },
                },
            ],
        ),
        (
            "flight",
            vec![
                node(0, 0, Monster, &[(0, 1)]),
                node(0, 1, Rest, &[]),
                node(6, 1, Elite, &[(6, 2)]),
                node(6, 2, Shop, &[]),
            ],
            vec![route(6, 1, true), route(0, 1, false)],
        ),
        (
            "unknown_end",
            vec![node(2, 14, Rest, &[(3, 16)])],
            vec![route(2, 14, false)],
        ),
        (
            "cycle",
            vec![node(0, 0, Monster, &[(0, 1)]), node(0, 1, Rest, &[(0, 0)])],
            vec![route(0, 0, false)],
        ),
    ];
    specs
        .into_iter()
        .map(|(name, nodes, candidates)| {
            let TaskPublicObservationV1::Strategic { mut observation } =
                super::tests::fixture_decision().observation
            else {
                unreachable!()
            };
            observation.decision_site = PlannerDecisionSite::Map;
            observation.context = PlannerDecisionContext::Map { overlay: false };
            observation.public_map = PlannerPublicMap {
                current_x: -1,
                current_y: -1,
                boss: None,
                nodes,
            };
            if name == "flight" {
                observation.public_map.current_x = 0;
                observation.public_map.current_y = 0;
            }
            let decision = TaskPublicDecisionV1::new(
                TaskDecisionSiteV1::Map,
                TaskPublicObservationV1::Strategic { observation },
                candidates,
            )
            .unwrap();
            (name, decision)
        })
        .collect()
}

fn exports() -> Value {
    let rows = cases()
        .into_iter()
        .map(|(name, decision)| {
            (
                name.to_owned(),
                public_decision_export_v3(
                    &decision,
                    PublicDecisionSourceV1::ConstructedContractCase {
                        case_id: format!("route-reading-{name}"),
                    },
                    sts_presentation::Locale::En,
                )
                .unwrap(),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({"schema":"sts-route-reading-contract-cases-v1", "cases":rows})
}

#[test]
fn public_route_reading_preserves_candidate_order_and_uses_one_topology_owner() {
    for (name, decision) in cases() {
        let surface = strategic_reading::project(&decision, sts_presentation::Locale::En).unwrap();
        let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
            unreachable!()
        };
        let summaries =
            crate::render::render_route_options_v1(&observation.public_map, &decision.candidates);
        let mut ordinal = 0;
        for (i, candidate) in decision.candidates.iter().enumerate() {
            let row = &surface["candidates"][i];
            assert_eq!(
                row["candidate_id"],
                decision.information.candidate_surface.ordered_candidate_ids[i]
            );
            if let TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ChooseRouteNode { x, y, .. },
            } = candidate
            {
                match crate::public_map_reading::PublicRouteReading::new(
                    &observation.public_map,
                    (*x, *y),
                    sts_presentation::Locale::En,
                ) {
                    Ok(reading) => {
                        assert!(summaries[ordinal].contains(&reading.summary()));
                        assert!(row["brief"].as_str().unwrap().contains(&reading.summary()));
                        assert!(row["detail"].as_str().unwrap().contains(&reading.detail()));
                    }
                    Err(error) => {
                        assert_eq!(name, "cycle");
                        assert!(summaries[ordinal].contains("Unknown"));
                        assert!(row["detail"].as_str().unwrap().contains(&error));
                        assert!(row["detail"]
                            .as_str()
                            .unwrap()
                            .contains("Route topology unavailable"));
                    }
                }
                ordinal += 1;
            }
        }
        assert_eq!(summaries.len(), ordinal);
    }
}

#[test]
fn retained_route_fixtures_have_the_current_public_projection() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-route-readings.json");
    let retained: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let current = exports();
    for (name, current) in current["cases"].as_object().unwrap() {
        let old = &retained["cases"][name];
        for key in ["decision", "source", "card_mechanics"] {
            assert_eq!(current["payload"][key], old["payload"][key]);
        }
        // Words may change independently of the contract. Source bytes and the
        // candidate/observation join must still be present and bound correctly.
        let surface = &old["payload"]["strategic_reading"];
        assert_eq!(
            surface["candidates"].as_array().unwrap().len(),
            current["payload"]["decision"]["candidates"]
                .as_array()
                .unwrap()
                .len()
        );
        for (current, old) in current["payload"]["strategic_reading"]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .zip(surface["candidates"].as_array().unwrap())
        {
            for key in ["candidate_id", "candidate_sha256"] {
                assert_eq!(current[key], old[key]);
            }
        }
        assert_eq!(
            surface["observation_sha256"],
            current["payload"]["strategic_reading"]["observation_sha256"]
        );
        assert_eq!(
            old["payload_sha256"],
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&old["payload"]).unwrap())
            )
        );
    }
}
