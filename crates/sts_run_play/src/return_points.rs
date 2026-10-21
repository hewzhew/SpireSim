//! Typed scratch return points. Storage owns retention and exact state; these
//! requests bind a selected generation to the session and head that issued it.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sts_oracle_learning::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_learning::agent::task::{
    TaskDecisionSiteV1, TaskEpisodeOutcomeV1, TaskPublicObservationV1,
};
use sts_oracle_learning::content::potions::PotionId;

use crate::model::RunPlayHeadV1;
use crate::storage::RunPlayPathsV1;

const POINTS_SCHEMA: &str = "run-play-return-points-v2";
const REQUEST_SCHEMA: &str = "run-play-return-request-v1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnIdentityV1 {
    pub head_revision: u64,
    pub state_revision: u64,
    pub archive_node_id: String,
    pub public_snapshot_id: Option<String>,
}

impl ReturnIdentityV1 {
    fn from_head(head: &RunPlayHeadV1) -> Self {
        Self {
            head_revision: head.head_revision,
            state_revision: head.state_revision,
            archive_node_id: head.position.archive_node_id().to_string(),
            public_snapshot_id: head
                .current_decision
                .as_ref()
                .map(|decision| decision.public_snapshot_id.clone()),
        }
    }
    fn same_state(&self, other: &Self) -> bool {
        self.state_revision == other.state_revision
            && self.archive_node_id == other.archive_node_id
            && self.public_snapshot_id == other.public_snapshot_id
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnSourceV1 {
    pub session_path: PathBuf,
    pub session_id: String,
    pub identity: ReturnIdentityV1,
}

fn source(paths: &RunPlayPathsV1, head: &RunPlayHeadV1) -> Result<ReturnSourceV1, String> {
    Ok(ReturnSourceV1 {
        session_path: paths
            .root()
            .canonicalize()
            .map_err(|error| error.to_string())?,
        session_id: head.session_id.clone(),
        identity: ReturnIdentityV1::from_head(head),
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReturnOriginV1 {
    Recent { steps: usize },
    Floor { floor: i32 },
    Mark { name: String },
    PreviousEndpoint,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReturnLocationV2 {
    Strategic {
        act: u8,
        floor: i32,
        site: TaskDecisionSiteV1,
    },
    Combat {
        act: Option<u8>,
        floor: Option<i32>,
        encounter: Option<String>,
        turn: usize,
    },
    Terminal {
        outcome: TaskEpisodeOutcomeV1,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnResourcesV2 {
    pub hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub potions: Vec<Option<PotionId>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnPointV2 {
    pub identity: ReturnIdentityV1,
    pub origins: Vec<ReturnOriginV1>,
    pub location: ReturnLocationV2,
    pub resources: Option<ReturnResourcesV2>,
}

fn point(head: &RunPlayHeadV1, origins: Vec<ReturnOriginV1>) -> Result<ReturnPointV2, String> {
    let (location, resources) = match &head.position {
        crate::model::RunPlayPositionV1::Terminal { outcome, .. } => (
            ReturnLocationV2::Terminal {
                outcome: outcome.clone(),
            },
            None,
        ),
        crate::model::RunPlayPositionV1::Decision { .. } => {
            let decision = head
                .current_decision
                .as_ref()
                .ok_or("return point has no cached decision")?;
            match &decision.observation {
                TaskPublicObservationV1::Strategic { observation } => (
                    ReturnLocationV2::Strategic {
                        act: observation.run.act,
                        floor: observation.run.floor,
                        site: observation.decision_site.into(),
                    },
                    Some(ReturnResourcesV2 {
                        hp: observation.run.current_hp,
                        max_hp: observation.run.max_hp,
                        gold: observation.run.gold,
                        potions: observation
                            .potions
                            .iter()
                            .map(|slot| slot.potion.as_ref().map(|potion| potion.potion))
                            .collect(),
                    }),
                ),
                TaskPublicObservationV1::Combat {
                    run_context,
                    observation,
                } => {
                    let (act, floor, encounter) = match run_context {
                        PublicCombatRunContextV1::Available {
                            act,
                            floor,
                            encounter_id,
                            ..
                        } => (
                            Some(*act),
                            Some(*floor),
                            encounter_id.as_ref().map(|id| id.stable_id().to_string()),
                        ),
                        PublicCombatRunContextV1::Unavailable { .. } => (None, None, None),
                    };
                    (
                        ReturnLocationV2::Combat {
                            act,
                            floor,
                            encounter,
                            turn: observation.turn.turn_count.saturating_add(1) as usize,
                        },
                        Some(ReturnResourcesV2 {
                            hp: observation.player.hp,
                            max_hp: observation.player.max_hp,
                            gold: observation.player.gold,
                            potions: observation
                                .potions
                                .iter()
                                .map(|slot| slot.as_ref().map(|potion| potion.potion_id))
                                .collect(),
                        }),
                    )
                }
            }
        }
    };
    Ok(ReturnPointV2 {
        identity: ReturnIdentityV1::from_head(head),
        origins,
        location,
        resources,
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnPointsV2 {
    pub schema: String,
    pub source: ReturnSourceV1,
    pub current: ReturnPointV2,
    pub targets: Vec<ReturnPointV2>,
}

pub fn return_points_v2(paths: &RunPlayPathsV1) -> Result<ReturnPointsV2, String> {
    let current = paths.load_head()?;
    if !current.scratch {
        return Err("return points require a scratch session".into());
    }
    let mut targets: Vec<ReturnPointV2> = Vec::new();
    let mut current_origins = Vec::new();
    let mut add = |head: RunPlayHeadV1, origin: ReturnOriginV1| -> Result<(), String> {
        paths.scratch_head_revision_v1(&current, head.head_revision)?;
        let identity = ReturnIdentityV1::from_head(&head);
        if identity.same_state(&ReturnIdentityV1::from_head(&current)) {
            current_origins.push(origin);
            return Ok(());
        }
        if let Some(existing) = targets
            .iter_mut()
            .find(|target| target.identity.same_state(&identity))
        {
            existing.origins.push(origin);
        } else {
            targets.push(point(&head, vec![origin])?);
        }
        Ok(())
    };
    for (index, head) in paths
        .recent_scratch_heads_v1(&current, 12)?
        .into_iter()
        .enumerate()
    {
        add(head, ReturnOriginV1::Recent { steps: index + 1 })?;
    }
    for end in &current.scratch_floor_ends {
        add(
            paths.scratch_head_revision_v1(&current, end.head_revision)?,
            ReturnOriginV1::Floor { floor: end.floor },
        )?;
    }
    for mark in &current.scratch_marks {
        add(
            paths.load_scratch_mark_head_v1(&current, &mark.name)?,
            ReturnOriginV1::Mark {
                name: mark.name.clone(),
            },
        )?;
    }
    if let Some(revision) = current.scratch_return_revision {
        add(
            paths.scratch_head_revision_v1(&current, revision)?,
            ReturnOriginV1::PreviousEndpoint,
        )?;
    }
    let result = ReturnPointsV2 {
        schema: POINTS_SCHEMA.to_string(),
        source: source(paths, &current)?,
        current: point(&current, current_origins)?,
        targets,
    };
    if result.source != source(paths, &paths.load_head()?)? {
        return Err("game advanced while listing return points".into());
    }
    Ok(result)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnRequestV1 {
    pub schema: String,
    pub source: ReturnSourceV1,
    pub target: ReturnIdentityV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReturnReceiptV1 {
    pub schema: String,
    pub from: ReturnSourceV1,
    pub to: ReturnSourceV1,
    pub previous_endpoint: ReturnIdentityV1,
}

pub fn restore_return_point_v1(
    paths: &RunPlayPathsV1,
    request: &ReturnRequestV1,
) -> Result<ReturnReceiptV1, String> {
    if request.schema != REQUEST_SCHEMA {
        return Err("unsupported return request schema".into());
    }
    let points = return_points_v2(paths)?;
    if request.source != points.source {
        return Err("return request source expired or belongs to another session".into());
    }
    if !points
        .targets
        .iter()
        .any(|target| target.identity == request.target)
    {
        return Err("return target is not in the retained points for this source".into());
    }
    let current = paths.load()?;
    if source(paths, &current.head)? != request.source {
        return Err("return request source expired".into());
    }
    let target = paths.scratch_head_revision_v1(&current.head, request.target.head_revision)?;
    if ReturnIdentityV1::from_head(&target) != request.target {
        return Err("return target identity changed".into());
    }
    let previous = paths.load_scratch_decision_head_v1(target)?;
    let previous_endpoint = ReturnIdentityV1::from_head(&current.head);
    let restored = crate::driver::publish_scratch_rewind_from_v1(paths, current, previous, true)?;
    Ok(ReturnReceiptV1 {
        schema: "run-play-return-receipt-v1".into(),
        from: request.source.clone(),
        to: source(paths, &restored.head)?,
        previous_endpoint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision_tests::TestDirectory;
    use crate::driver::{
        create_scratch_natural_session_v1, save_scratch_mark_v1, RunPlayControllerV1,
    };
    use sts_oracle_learning::agent::information::state::CombatLearningPotionV1;
    use sts_oracle_learning::agent::task::{
        GameOutcomeV1, TaskBudgetV1, TaskEpisodeFailureV1, TaskEpisodePauseV1,
        TaskStrategicPotionSlotV1, TaskStrategicPotionV1,
    };
    use sts_oracle_learning::eval::run_control::CombatResearchPlanOptionsV1;

    fn prepare(root: &std::path::Path) -> RunPlayPathsV1 {
        let paths = RunPlayPathsV1::new(root);
        create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        save_scratch_mark_v1(&paths, "start").unwrap();
        paths
    }
    fn advance(paths: &RunPlayPathsV1) {
        RunPlayControllerV1::open(paths.clone())
            .unwrap()
            .choose_action_v1(
                &crate::selector::parse_action_selector_v1("event:Neow:0", "act").unwrap(),
                true,
            )
            .unwrap();
    }
    fn request(points: &ReturnPointsV2, target: &ReturnPointV2) -> ReturnRequestV1 {
        ReturnRequestV1 {
            schema: REQUEST_SCHEMA.into(),
            source: points.source.clone(),
            target: target.identity.clone(),
        }
    }

    #[test]
    fn return_resources_preserve_potion_identities_slots_and_point_identity() {
        let directory = TestDirectory::new();
        let paths = prepare(&directory.0.join("potions"));
        let mut strategic = paths.load_head().unwrap();
        let original = strategic.clone();
        let TaskPublicObservationV1::Strategic { observation } =
            &mut strategic.current_decision.as_mut().unwrap().observation
        else {
            panic!("expected strategic fixture");
        };
        observation.potions = vec![
            TaskStrategicPotionSlotV1 {
                slot: 0,
                potion: None,
            },
            TaskStrategicPotionSlotV1 {
                slot: 1,
                potion: Some(TaskStrategicPotionV1 {
                    potion: PotionId::SteroidPotion,
                    can_use: false,
                    can_discard: true,
                    requires_target: false,
                }),
            },
        ];
        let session = crate::decision_tests::fixture("combat_multiturn");
        let game = crate::decision_tests::controller_with_session(
            &directory.0,
            "combat_potions",
            &session,
        );
        let mut combat = game.head().clone();
        let TaskPublicObservationV1::Combat { observation, .. } =
            &mut combat.current_decision.as_mut().unwrap().observation
        else {
            panic!("expected combat fixture");
        };
        observation.potions = vec![
            Some(CombatLearningPotionV1 {
                potion_id: PotionId::Elixir,
                can_use: true,
                can_discard: true,
                requires_target: false,
            }),
            None,
            Some(CombatLearningPotionV1 {
                potion_id: PotionId::WeakenPotion,
                can_use: true,
                can_discard: true,
                requires_target: true,
            }),
        ];
        for (head, expected) in [
            (&strategic, serde_json::json!([null, "SteroidPotion"])),
            (&combat, serde_json::json!(["Elixir", null, "WeakenPotion"])),
        ] {
            let projected = point(head, vec![]).unwrap();
            assert_eq!(projected.identity, ReturnIdentityV1::from_head(head));
            let json = serde_json::to_value(&projected).unwrap();
            assert_eq!(json["resources"]["potions"], expected);
            assert_eq!(
                serde_json::from_value::<ReturnPointV2>(json).unwrap(),
                projected
            );
        }
        assert_eq!(return_points_v2(&paths).unwrap().schema, POINTS_SCHEMA);
        assert_eq!(paths.load_head().unwrap(), original);
    }

    #[test]
    fn terminal_points_carry_the_existing_structured_outcome_and_raw_diagnostic() {
        let directory = TestDirectory::new();
        let paths = prepare(&directory.0.join("terminal"));
        let mut head = paths.load_head().unwrap();
        let identity = ReturnIdentityV1::from_head(&head);
        let mut outcomes = vec![
            TaskEpisodeOutcomeV1::Success {
                game_outcome: GameOutcomeV1::HeartVictory,
            },
            TaskEpisodeOutcomeV1::Success {
                game_outcome: GameOutcomeV1::ActThreeVictory,
            },
            TaskEpisodeOutcomeV1::Success {
                game_outcome: GameOutcomeV1::Defeat,
            },
            TaskEpisodeOutcomeV1::Paused {
                reason: TaskEpisodePauseV1::PublicDecisionControlYield {
                    site: TaskDecisionSiteV1::Map,
                },
            },
        ];
        outcomes.extend(
            [
                (
                    TaskEpisodeFailureV1::CommittedDefeat,
                    Some(GameOutcomeV1::Defeat),
                ),
                (
                    TaskEpisodeFailureV1::CommittedTerminalWithoutObjective,
                    Some(GameOutcomeV1::ActThreeVictory),
                ),
                (TaskEpisodeFailureV1::RetryBudgetExhausted, None),
                (TaskEpisodeFailureV1::TransitionBudgetExhausted, None),
                (TaskEpisodeFailureV1::UnsupportedDecisionSurface, None),
                (
                    TaskEpisodeFailureV1::MechanicsOrProtocolError {
                        message: "Original diagnostic 原始诊断 {keep}".into(),
                    },
                    None,
                ),
            ]
            .into_iter()
            .map(|(reason, game_outcome)| TaskEpisodeOutcomeV1::Failure {
                reason,
                game_outcome,
            }),
        );
        for outcome in outcomes {
            head.position = crate::model::RunPlayPositionV1::Terminal {
                archive_node_id: identity.archive_node_id.clone(),
                outcome: outcome.clone(),
            };
            let projected = point(&head, vec![]).unwrap();
            let json = serde_json::to_value(&projected).unwrap();
            assert_eq!(
                json["location"],
                serde_json::json!({"kind": "terminal", "outcome": outcome})
            );
            assert!(projected.resources.is_none());
            assert_eq!(projected.identity, identity);
            assert_eq!(
                serde_json::from_value::<ReturnPointV2>(json).unwrap(),
                projected
            );
        }
    }

    #[test]
    fn return_points_are_read_only_and_merge_metadata_only_generations() {
        let directory = TestDirectory::new();
        let paths = prepare(&directory.0.join("read"));
        let before = paths.load().unwrap();
        let points = return_points_v2(&paths).unwrap();
        assert!(
            points.targets.is_empty(),
            "saving a mark does not create another game state"
        );
        assert!(points.current.origins.contains(&ReturnOriginV1::Mark {
            name: "start".into()
        }));
        advance(&paths);
        let current = paths.load().unwrap();
        let points = return_points_v2(&paths).unwrap();
        assert_eq!(points.targets.len(), 1);
        assert!(points.targets[0].origins.contains(&ReturnOriginV1::Mark {
            name: "start".into()
        }));
        assert!(points.targets[0]
            .origins
            .contains(&ReturnOriginV1::Recent { steps: 1 }));
        assert_eq!(
            points.targets[0].identity.state_revision,
            before.head.state_revision
        );
        let after = paths.load().unwrap();
        assert_eq!(after.head, current.head);
        assert_eq!(after.state, current.state);
    }

    #[test]
    fn checked_return_preserves_exact_checkpoint_and_one_previous_endpoint() {
        let directory = TestDirectory::new();
        let paths = prepare(&directory.0.join("restore"));
        let initial = paths.load().unwrap();
        advance(&paths);
        let branch = paths.load().unwrap();
        let points = return_points_v2(&paths).unwrap();
        let receipt =
            restore_return_point_v1(&paths, &request(&points, &points.targets[0])).unwrap();
        let restored = paths.load().unwrap();
        assert_eq!(
            restored.head.current_decision,
            initial.head.current_decision
        );
        assert_eq!(
            restored.state.checkpoint_payload,
            initial.state.checkpoint_payload
        );
        assert_eq!(restored.state.archive, initial.state.archive);
        assert_eq!(
            restored.head.scratch_return_revision,
            Some(branch.head.head_revision)
        );
        assert_eq!(
            receipt.previous_endpoint,
            ReturnIdentityV1::from_head(&branch.head)
        );
        let points = return_points_v2(&paths).unwrap();
        let last = points
            .targets
            .iter()
            .find(|point| point.origins.contains(&ReturnOriginV1::PreviousEndpoint))
            .unwrap();
        restore_return_point_v1(&paths, &request(&points, last)).unwrap();
        let selected = paths.load().unwrap();
        assert_eq!(
            selected.state.checkpoint_payload,
            branch.state.checkpoint_payload
        );
        assert_eq!(selected.state.archive, branch.state.archive);
        assert_eq!(selected.head.current_decision, branch.head.current_decision);
        assert_eq!(
            selected.head.scratch_return_revision,
            Some(restored.head.head_revision)
        );
        assert_eq!(
            return_points_v2(&paths)
                .unwrap()
                .targets
                .iter()
                .filter(|point| point.origins.contains(&ReturnOriginV1::PreviousEndpoint))
                .count(),
            1
        );
        assert!(RunPlayControllerV1::open(paths).is_ok());
    }

    #[test]
    fn stale_foreign_and_forged_returns_do_not_publish_or_save_an_endpoint() {
        let directory = TestDirectory::new();
        let paths = prepare(&directory.0.join("one"));
        advance(&paths);
        let points = return_points_v2(&paths).unwrap();
        let valid = request(&points, &points.targets[0]);
        let mut forged = valid.clone();
        forged.target.public_snapshot_id = Some("other snapshot".into());
        let before = paths.load().unwrap();
        assert!(restore_return_point_v1(&paths, &forged)
            .unwrap_err()
            .contains("retained points"));
        let foreign = prepare(&directory.0.join("two"));
        advance(&foreign);
        let foreign_before = foreign.load().unwrap();
        assert!(restore_return_point_v1(&foreign, &valid)
            .unwrap_err()
            .contains("another session"));
        assert_eq!(foreign.load().unwrap().head, foreign_before.head);
        assert_eq!(paths.load().unwrap().head, before.head);
        save_scratch_mark_v1(&paths, "later").unwrap();
        let changed = paths.load().unwrap();
        assert!(restore_return_point_v1(&paths, &valid)
            .unwrap_err()
            .contains("source expired"));
        let after = paths.load().unwrap();
        assert_eq!(after.head, changed.head);
        assert_eq!(after.state, changed.state);
        assert!(after.head.scratch_return_revision.is_none());
    }
}
