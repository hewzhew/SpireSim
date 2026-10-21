use sts_oracle_learning::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_learning::agent::task::{
    TaskDecisionSiteV1, TaskEpisodeOutcomeV1, TaskObservedBoundaryV1, TaskPublicChoiceV1,
    TaskPublicDecisionV1, TaskPublicObservationV1, TaskResearchPlanOutcomeV1,
};
use sts_oracle_learning::content::monsters::factory::EncounterId;

use crate::driver::run_play_path_v1;
use crate::model::{
    RunPlayEventV1, RunPlayHeadV1, RunPlayPositionV1, RunPlayStrategicOwnerAdviceV1,
};
use crate::storage::RunPlayPathsV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunPlayJourneyWindowV1 {
    First { count: usize },
    Last { count: usize },
    AfterDepth { depth: usize, count: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunPlayJourneyEpisodeFamilyV1 {
    Combat,
    Neow,
    Map,
    Reward,
    Event,
    Shop,
    Campfire,
    BossRelic,
    Treasure,
    RunChoice,
    RunTransition,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunPlayJourneyStepV1 {
    pub(crate) from_archive_node_id: String,
    pub(crate) to_archive_node_id: String,
    pub(crate) depth: usize,
    pub(crate) source: TaskPublicDecisionV1,
    pub(crate) choice: TaskPublicChoiceV1,
    pub(crate) observed_boundary: TaskObservedBoundaryV1,
    pub(crate) recorded_owner_advice: Option<RunPlayStrategicOwnerAdviceV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunPlayJourneyEpisodeV1 {
    pub(crate) family: RunPlayJourneyEpisodeFamilyV1,
    pub(crate) act: Option<u8>,
    pub(crate) floor: Option<i32>,
    pub(crate) steps: Vec<RunPlayJourneyStepV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunPlayJourneyTerminalV1 {
    pub(crate) outcome: TaskEpisodeOutcomeV1,
    pub(crate) research_outcome: Option<TaskResearchPlanOutcomeV1>,
}

impl RunPlayJourneyEpisodeV1 {
    fn first_depth(&self) -> usize {
        self.steps
            .first()
            .expect("journey episodes are non-empty")
            .depth
    }

    fn last_depth(&self) -> usize {
        self.steps
            .last()
            .expect("journey episodes are non-empty")
            .depth
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunPlayJourneyHistoryV1 {
    pub(crate) current_archive_node_id: String,
    pub(crate) total_path_boundaries: usize,
    pub(crate) total_episodes: usize,
    pub(crate) window: RunPlayJourneyWindowV1,
    pub(crate) first_episode_ordinal: usize,
    pub(crate) episodes: Vec<RunPlayJourneyEpisodeV1>,
    pub(crate) next_after_depth: Option<usize>,
    pub(crate) terminal: Option<RunPlayJourneyTerminalV1>,
}

pub fn journey_history_v1(
    paths: &RunPlayPathsV1,
    window: RunPlayJourneyWindowV1,
) -> Result<RunPlayJourneyHistoryV1, String> {
    let workspace = paths.load()?;
    workspace.state.archive.validate()?;
    let current_archive_node_id = workspace.head.position.archive_node_id().to_string();
    let terminal = journey_terminal_v1(&workspace.head, &current_archive_node_id);
    let current_path = run_play_path_v1(&workspace.state.archive, &current_archive_node_id)?;
    let total_path_boundaries = current_path.len();
    let mut episodes = Vec::<RunPlayJourneyEpisodeV1>::new();

    for path_step in current_path {
        let node = workspace
            .state
            .archive
            .node(&path_step.archive_node_id)
            .ok_or_else(|| format!("journey path node {} is missing", path_step.archive_node_id))?;
        let incoming = node.incoming_step.as_ref().ok_or_else(|| {
            format!(
                "journey path node {} has no incoming public step",
                path_step.archive_node_id
            )
        })?;
        let from_archive_node_id = node.parent_node_id.clone().ok_or_else(|| {
            format!(
                "journey path node {} has no parent",
                path_step.archive_node_id
            )
        })?;
        let family = episode_family_v1(&incoming.step.source);
        let (act, floor) = decision_location_v1(&incoming.step.source);
        let recorded_owner_advice = workspace.head.events.iter().rev().find_map(|event| {
            let RunPlayEventV1::Choice {
                from_archive_node_id: event_from,
                to_archive_node_id: Some(event_to),
                choice,
                strategic_owner_advice,
                ..
            } = event
            else {
                return None;
            };
            (event_from == &from_archive_node_id
                && event_to == &path_step.archive_node_id
                && choice == &incoming.step.choice)
                .then(|| strategic_owner_advice.clone())
                .flatten()
        });
        let step = RunPlayJourneyStepV1 {
            from_archive_node_id,
            to_archive_node_id: path_step.archive_node_id,
            depth: path_step.depth,
            source: incoming.step.source.clone(),
            choice: incoming.step.choice.clone(),
            observed_boundary: incoming.step.observed_boundary.clone(),
            recorded_owner_advice,
        };

        let merge = episodes.last().is_some_and(|episode| {
            episode.act == act
                && episode.floor == floor
                && episode_families_merge_v1(episode.family, family)
                && (family != RunPlayJourneyEpisodeFamilyV1::Combat
                    || episode_combat_encounter_v1(episode)
                        == decision_combat_encounter_v1(&incoming.step.source))
        });
        if merge {
            episodes
                .last_mut()
                .expect("merge requires a current episode")
                .steps
                .push(step);
        } else {
            episodes.push(RunPlayJourneyEpisodeV1 {
                family,
                act,
                floor,
                steps: vec![step],
            });
        }
    }

    let total_episodes = episodes.len();
    let (start, end) = journey_window_range_v1(&episodes, window);
    let next_after_depth =
        (end < total_episodes && start < end).then(|| episodes[end - 1].last_depth());
    let episodes = episodes.into_iter().skip(start).take(end - start).collect();
    Ok(RunPlayJourneyHistoryV1 {
        current_archive_node_id,
        total_path_boundaries,
        total_episodes,
        window,
        first_episode_ordinal: start + 1,
        episodes,
        next_after_depth,
        terminal,
    })
}

fn journey_terminal_v1(
    head: &RunPlayHeadV1,
    current_archive_node_id: &str,
) -> Option<RunPlayJourneyTerminalV1> {
    let RunPlayPositionV1::Terminal {
        archive_node_id,
        outcome,
    } = &head.position
    else {
        return None;
    };
    if archive_node_id != current_archive_node_id {
        return None;
    }
    let research_outcome = head.events.iter().rev().find_map(|event| match event {
        RunPlayEventV1::AnchorLine {
            to_archive_node_id,
            combat_research_candidate: Some(candidate),
            ..
        } if to_archive_node_id == current_archive_node_id => Some(candidate.outcome.clone()),
        _ => None,
    });
    Some(RunPlayJourneyTerminalV1 {
        outcome: outcome.clone(),
        research_outcome,
    })
}

fn episode_family_v1(decision: &TaskPublicDecisionV1) -> RunPlayJourneyEpisodeFamilyV1 {
    match decision.site {
        TaskDecisionSiteV1::Combat => RunPlayJourneyEpisodeFamilyV1::Combat,
        TaskDecisionSiteV1::Neow => RunPlayJourneyEpisodeFamilyV1::Neow,
        TaskDecisionSiteV1::Map => RunPlayJourneyEpisodeFamilyV1::Map,
        TaskDecisionSiteV1::Reward | TaskDecisionSiteV1::CardReward => {
            RunPlayJourneyEpisodeFamilyV1::Reward
        }
        TaskDecisionSiteV1::Event => RunPlayJourneyEpisodeFamilyV1::Event,
        TaskDecisionSiteV1::Shop => RunPlayJourneyEpisodeFamilyV1::Shop,
        TaskDecisionSiteV1::Campfire => RunPlayJourneyEpisodeFamilyV1::Campfire,
        TaskDecisionSiteV1::BossRelic => RunPlayJourneyEpisodeFamilyV1::BossRelic,
        TaskDecisionSiteV1::Treasure => RunPlayJourneyEpisodeFamilyV1::Treasure,
        TaskDecisionSiteV1::RunChoice => RunPlayJourneyEpisodeFamilyV1::RunChoice,
        TaskDecisionSiteV1::RunTransition => RunPlayJourneyEpisodeFamilyV1::RunTransition,
    }
}

fn episode_families_merge_v1(
    existing: RunPlayJourneyEpisodeFamilyV1,
    incoming: RunPlayJourneyEpisodeFamilyV1,
) -> bool {
    if existing == incoming {
        return existing != RunPlayJourneyEpisodeFamilyV1::Map;
    }
    matches!(
        (existing, incoming),
        (
            RunPlayJourneyEpisodeFamilyV1::Event
                | RunPlayJourneyEpisodeFamilyV1::Shop
                | RunPlayJourneyEpisodeFamilyV1::Campfire
                | RunPlayJourneyEpisodeFamilyV1::Treasure,
            RunPlayJourneyEpisodeFamilyV1::RunChoice
        ) | (
            RunPlayJourneyEpisodeFamilyV1::Treasure,
            RunPlayJourneyEpisodeFamilyV1::Reward
        )
    )
}

fn decision_location_v1(decision: &TaskPublicDecisionV1) -> (Option<u8>, Option<i32>) {
    match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => {
            (Some(observation.run.act), Some(observation.run.floor))
        }
        TaskPublicObservationV1::Combat { run_context, .. } => match run_context {
            PublicCombatRunContextV1::Available { act, floor, .. } => (Some(*act), Some(*floor)),
            PublicCombatRunContextV1::Unavailable { .. } => (None, None),
        },
    }
}

fn episode_combat_encounter_v1(episode: &RunPlayJourneyEpisodeV1) -> Option<EncounterId> {
    episode
        .steps
        .first()
        .and_then(|step| decision_combat_encounter_v1(&step.source))
}

fn decision_combat_encounter_v1(decision: &TaskPublicDecisionV1) -> Option<EncounterId> {
    match &decision.observation {
        TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Available { encounter_id, .. },
            ..
        } => *encounter_id,
        TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Unavailable { .. },
            ..
        }
        | TaskPublicObservationV1::Strategic { .. } => None,
    }
}

fn journey_window_range_v1(
    episodes: &[RunPlayJourneyEpisodeV1],
    window: RunPlayJourneyWindowV1,
) -> (usize, usize) {
    match window {
        RunPlayJourneyWindowV1::First { count } => (0, count.min(episodes.len())),
        RunPlayJourneyWindowV1::Last { count } => {
            (episodes.len().saturating_sub(count), episodes.len())
        }
        RunPlayJourneyWindowV1::AfterDepth { depth, count } => {
            let start = episodes
                .iter()
                .position(|episode| episode.first_depth() > depth)
                .unwrap_or(episodes.len());
            (start, start.saturating_add(count).min(episodes.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_merging_keeps_navigation_and_combat_as_distinct_history_units() {
        assert!(!episode_families_merge_v1(
            RunPlayJourneyEpisodeFamilyV1::Map,
            RunPlayJourneyEpisodeFamilyV1::Map
        ));
        assert!(episode_families_merge_v1(
            RunPlayJourneyEpisodeFamilyV1::Reward,
            RunPlayJourneyEpisodeFamilyV1::Reward
        ));
        assert!(episode_families_merge_v1(
            RunPlayJourneyEpisodeFamilyV1::Event,
            RunPlayJourneyEpisodeFamilyV1::RunChoice
        ));
        assert!(!episode_families_merge_v1(
            RunPlayJourneyEpisodeFamilyV1::Combat,
            RunPlayJourneyEpisodeFamilyV1::Reward
        ));
    }
}
