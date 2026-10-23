use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use sts_oracle_learning::agent::information::action::PublicCombatAtomicActionV1;
use sts_oracle_learning::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_learning::agent::task::{
    TaskEpisodeOutcomeV1, TaskObservedBoundaryV1, TaskPublicCandidateV1, TaskPublicChoiceV1,
    TaskPublicDecisionV1, TaskPublicObservationV1, TaskStrategicObservationV1,
};
use sts_oracle_learning::ai::planner_core::PlannerDecisionContext;
use sts_oracle_learning::content::identity::gameplay_display_label_v1;

use crate::history::{
    RunPlayJourneyEpisodeFamilyV1, RunPlayJourneyEpisodeV1, RunPlayJourneyHistoryV1,
    RunPlayJourneyTerminalV1, RunPlayJourneyWindowV1,
};
use crate::history_state::{
    combat_boundary_deltas_v1, push_scalar_delta_v1, render_deck_v1, render_potions_v1,
    render_relics_v1,
};
use crate::render::{render_candidate_with_observation, short_id};
use crate::selector::semantic_action_for_choice_v1;

pub fn render_journey_history_v1(history: &RunPlayJourneyHistoryV1, detail: bool) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "journey episodes={} boundaries={} showing={} window={} current=n{}",
        history.total_episodes,
        history.total_path_boundaries,
        history.episodes.len(),
        render_window_v1(history.window),
        short_id(&history.current_archive_node_id),
    )
    .expect("string write");
    if let Some(depth) = history.next_after_depth {
        writeln!(output, "next=--after-depth {depth} --count <n>").expect("string write");
    }

    for (offset, episode) in history.episodes.iter().enumerate() {
        let title = episode_title_v1(episode);
        let act = episode
            .act
            .map_or_else(|| "?".to_string(), |act| act.to_string());
        let floor = episode
            .floor
            .map_or_else(|| "?".to_string(), |floor| floor.to_string());
        let first = episode.steps.first().expect("episode is non-empty");
        let last = episode.steps.last().expect("episode is non-empty");
        writeln!(
            output,
            "E{} A{}F{} {} depths=d{}..d{} boundaries={} edge=n{}->n{}",
            history.first_episode_ordinal + offset,
            act,
            floor,
            title,
            first.depth,
            last.depth,
            episode.steps.len(),
            short_id(&first.from_archive_node_id),
            short_id(&last.to_archive_node_id),
        )
        .expect("string write");

        match episode.family {
            RunPlayJourneyEpisodeFamilyV1::Combat => {
                let terminal = (last.to_archive_node_id == history.current_archive_node_id)
                    .then_some(history.terminal.as_ref())
                    .flatten();
                render_combat_episode_v1(&mut output, episode, detail, terminal)
            }
            _ => render_strategic_episode_v1(&mut output, episode, detail),
        }
    }
    output
}

fn render_strategic_episode_v1(
    output: &mut String,
    episode: &RunPlayJourneyEpisodeV1,
    detail: bool,
) {
    let Some(TaskPublicObservationV1::Strategic {
        observation: before,
    }) = episode.steps.first().map(|step| &step.source.observation)
    else {
        writeln!(output, "  <strategic source unavailable>").expect("string write");
        return;
    };
    writeln!(
        output,
        "  before hp={}/{} gold={} deck={} relics=[{}] potions=[{}] keys={:?}",
        before.run.current_hp,
        before.run.max_hp,
        before.run.gold,
        before.cards.len(),
        render_relics_v1(before),
        render_potions_v1(before),
        before.run.keys,
    )
    .expect("string write");
    if detail {
        writeln!(output, "  deck [{}]", render_deck_v1(before)).expect("string write");
    }

    for step in &episode.steps {
        let selected_ordinal = step.choice.ordinal_path.first().copied();
        let selected_action = semantic_action_for_choice_v1(&step.source, &step.choice)
            .unwrap_or_else(|error| format!("<semantic-error:{error}>"));
        let selected_human = selected_ordinal
            .and_then(|ordinal| step.source.candidates.get(ordinal))
            .map(|candidate| {
                render_candidate_with_observation(candidate, Some(&step.source.observation))
            })
            .unwrap_or_else(|| "<selected candidate unavailable>".to_string());
        writeln!(
            output,
            "  d{} {:?} candidates={}{} choose={:?} {} selector={} -> {}",
            step.depth,
            step.source.site,
            step.source.candidates.len(),
            if step.source.candidates.len() == 1 {
                " forced"
            } else {
                ""
            },
            step.choice.ordinal_path,
            selected_human,
            selected_action,
            observed_boundary_label_v1(&step.observed_boundary),
        )
        .expect("string write");
        if detail {
            for (ordinal, candidate) in step.source.candidates.iter().enumerate() {
                let choice = TaskPublicChoiceV1::one(ordinal);
                let selector = semantic_action_for_choice_v1(&step.source, &choice)
                    .unwrap_or_else(|error| format!("<semantic-error:{error}>"));
                writeln!(
                    output,
                    "    {}[{}] {} selector={}",
                    if selected_ordinal == Some(ordinal) {
                        "*"
                    } else {
                        " "
                    },
                    ordinal,
                    render_candidate_with_observation(candidate, Some(&step.source.observation)),
                    selector,
                )
                .expect("string write");
            }
        }
        if let Some(advice) = &step.recorded_owner_advice {
            let provenance = &advice.support.provenance;
            if let Some(selector) = &advice.semantic_action {
                let human = step
                    .source
                    .candidates
                    .iter()
                    .enumerate()
                    .find_map(|(ordinal, candidate)| {
                        let choice = TaskPublicChoiceV1::one(ordinal);
                        (semantic_action_for_choice_v1(&step.source, &choice)
                            .ok()
                            .as_deref()
                            == Some(selector.as_str()))
                        .then(|| {
                            render_candidate_with_observation(
                                candidate,
                                Some(&step.source.observation),
                            )
                        })
                    })
                    .unwrap_or_else(|| "<candidate unavailable>".to_string());
                writeln!(
                    output,
                    "    recorded-owner-suggestion producer={} revision={} access={:?} relation={} {} selector={}",
                    provenance.producer_name,
                    provenance.producer_revision,
                    provenance.access,
                    if selector == &selected_action {
                        "accepted"
                    } else {
                        "overridden"
                    },
                    human,
                    selector,
                )
                .expect("string write");
            } else {
                writeln!(
                    output,
                    "    recorded-owner-suggestion producer={} revision={} access={:?} relation=unavailable selector=<unavailable>",
                    provenance.producer_name,
                    provenance.producer_revision,
                    provenance.access,
                )
                .expect("string write");
            }
        }
    }

    render_episode_result_v1(output, before, episode);
}

fn render_combat_episode_v1(
    output: &mut String,
    episode: &RunPlayJourneyEpisodeV1,
    detail: bool,
    terminal: Option<&RunPlayJourneyTerminalV1>,
) {
    let terminal_resources = terminal.and_then(|terminal| terminal.research_outcome.as_ref());
    let Some(health) = combat_health_summary_v1(
        episode,
        terminal_resources.map(|outcome| (outcome.final_hp, outcome.persistent.max_hp)),
    ) else {
        writeln!(output, "  <combat source unavailable>").expect("string write");
        return;
    };
    writeln!(
        output,
        "  combat-health entry={}/{} min-observed={}/{} last-combat-observation={}/{} post-combat={} net={} observed-decrease={} observed-increase={} completion-boundary-delta={}",
        health.entry_hp,
        health.entry_max_hp,
        health.min_observed_hp,
        health.entry_max_hp,
        health.last_combat_observation_hp,
        health.last_combat_observation_max_hp,
        health
            .post_combat_hp
            .zip(health.post_combat_max_hp)
            .map_or_else(|| "unavailable".to_string(), |(hp, max_hp)| format!("{hp}/{max_hp}")),
        render_optional_signed_v1(health.net_hp_change),
        health.observed_hp_decrease,
        health.observed_hp_increase,
        render_optional_signed_v1(health.completion_boundary_delta),
    )
    .expect("string write");
    writeln!(
        output,
        "  combat-progress actions={} observed-player-turns={} internal-last-turn-index={}",
        episode.steps.len(),
        health.observed_player_turns,
        health.last_internal_turn_index,
    )
    .expect("string write");
    let card_counts = combat_episode_card_choices_v1(episode);
    writeln!(
        output,
        "  chosen-card-plays [{}] (automatic repeats not counted)",
        card_counts
            .iter()
            .map(|(card, count)| format!("{card} x{count}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .expect("string write");
    if detail {
        render_combat_boundary_changes_v1(output, episode);
    }
    let last = episode.steps.last().expect("combat episode is non-empty");
    let potions_used = combat_episode_potions_used_v1(episode);
    if let Some(terminal) = terminal {
        if let Some(resources) = terminal_resources {
            writeln!(
                output,
                "  result terminal={} hp={}/{} gold={} potions-used={}",
                render_terminal_outcome_v1(&terminal.outcome),
                resources.final_hp,
                resources.persistent.max_hp,
                resources.final_gold,
                potions_used,
            )
            .expect("string write");
        } else {
            writeln!(
                output,
                "  result terminal={} hp=unavailable gold=unavailable potions-used={}",
                render_terminal_outcome_v1(&terminal.outcome),
                potions_used,
            )
            .expect("string write");
        }
    } else if let Some(after) = observed_decision_v1(&last.observed_boundary) {
        match &after.observation {
            TaskPublicObservationV1::Strategic { observation } => {
                writeln!(
                    output,
                    "  result next={:?} hp={}/{} gold={} potions-used={}",
                    after.site,
                    observation.run.current_hp,
                    observation.run.max_hp,
                    observation.run.gold,
                    potions_used,
                )
                .expect("string write");
            }
            TaskPublicObservationV1::Combat { observation, .. } => {
                writeln!(
                    output,
                    "  result next=Combat hp={}/{}",
                    observation.player.hp, observation.player.max_hp
                )
                .expect("string write");
            }
        }
    } else {
        writeln!(
            output,
            "  result {}",
            observed_boundary_label_v1(&last.observed_boundary)
        )
        .expect("string write");
    }
}

fn combat_episode_card_choices_v1(episode: &RunPlayJourneyEpisodeV1) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for step in &episode.steps {
        let Some(ordinal) = step.choice.ordinal_path.first() else {
            continue;
        };
        let Some(TaskPublicCandidateV1::CombatAtomic {
            action: PublicCombatAtomicActionV1::PlayCard { hand_index, .. },
        }) = step.source.candidates.get(*ordinal)
        else {
            continue;
        };
        let TaskPublicObservationV1::Combat { observation, .. } = &step.source.observation else {
            continue;
        };
        let name = observation.cards.hand.cards.get(*hand_index).map_or_else(
            || "<unavailable card>".to_string(),
            |card| crate::render::format_card(card.card_id, card.upgrades),
        );
        *counts.entry(name).or_default() += 1;
    }
    counts
}

fn render_combat_boundary_changes_v1(output: &mut String, episode: &RunPlayJourneyEpisodeV1) {
    writeln!(
        output,
        "  Recorded boundary changes (net changes, not an effect event log):"
    )
    .expect("string write");
    for step in &episode.steps {
        let TaskPublicObservationV1::Combat {
            observation: before,
            ..
        } = &step.source.observation
        else {
            continue;
        };
        let action = semantic_action_for_choice_v1(&step.source, &step.choice)
            .unwrap_or_else(|_| "<unavailable action>".into());
        let changes = match observed_decision_v1(&step.observed_boundary).map(|d| &d.observation) {
            Some(TaskPublicObservationV1::Combat {
                observation: after, ..
            }) => combat_boundary_deltas_v1(before, after),
            Some(TaskPublicObservationV1::Strategic { observation: after }) => vec![format!(
                "Player HP {} -> {} at combat completion; post-combat powers unavailable",
                before.player.hp, after.run.current_hp
            )],
            None => vec![format!(
                "{}; successor public state unavailable",
                observed_boundary_label_v1(&step.observed_boundary)
            )],
        };
        writeln!(
            output,
            "    d{} T{} {} | {}",
            step.depth,
            before.turn.turn_count.saturating_add(1),
            action,
            if changes.is_empty() {
                "no tracked HP, Block or power-amount changes".into()
            } else {
                changes.join("; ")
            }
        )
        .expect("string write");
    }
}

fn combat_episode_potions_used_v1(episode: &RunPlayJourneyEpisodeV1) -> usize {
    episode
        .steps
        .iter()
        .filter(|step| {
            let Some(root_ordinal) = step.choice.ordinal_path.first() else {
                return false;
            };
            matches!(
                step.source.candidates.get(*root_ordinal),
                Some(TaskPublicCandidateV1::CombatAtomic {
                    action: PublicCombatAtomicActionV1::UsePotion { .. }
                })
            )
        })
        .count()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CombatHealthSummaryV1 {
    entry_hp: i32,
    entry_max_hp: i32,
    min_observed_hp: i32,
    last_combat_observation_hp: i32,
    last_combat_observation_max_hp: i32,
    post_combat_hp: Option<i32>,
    post_combat_max_hp: Option<i32>,
    net_hp_change: Option<i32>,
    observed_hp_decrease: i32,
    observed_hp_increase: i32,
    completion_boundary_delta: Option<i32>,
    observed_player_turns: usize,
    last_internal_turn_index: u32,
}

fn combat_health_summary_v1(
    episode: &RunPlayJourneyEpisodeV1,
    terminal_post_combat: Option<(i32, i32)>,
) -> Option<CombatHealthSummaryV1> {
    let mut samples = Vec::<(i32, i32)>::new();
    let mut turns = BTreeSet::<u32>::new();
    let mut post_combat = None;
    for step in &episode.steps {
        if let TaskPublicObservationV1::Combat { observation, .. } = &step.source.observation {
            samples.push((observation.player.hp, observation.player.max_hp));
            turns.insert(observation.turn.turn_count);
        }
        if let Some(after) = observed_decision_v1(&step.observed_boundary) {
            match &after.observation {
                TaskPublicObservationV1::Combat { observation, .. } => {
                    samples.push((observation.player.hp, observation.player.max_hp));
                    turns.insert(observation.turn.turn_count);
                }
                TaskPublicObservationV1::Strategic { observation } => {
                    post_combat = Some((observation.run.current_hp, observation.run.max_hp));
                }
            }
        }
    }
    summarize_combat_health_samples_v1(&samples, &turns, post_combat.or(terminal_post_combat))
}

fn summarize_combat_health_samples_v1(
    samples: &[(i32, i32)],
    turns: &BTreeSet<u32>,
    post_combat: Option<(i32, i32)>,
) -> Option<CombatHealthSummaryV1> {
    let &(entry_hp, entry_max_hp) = samples.first()?;
    let &(last_combat_observation_hp, last_combat_observation_max_hp) = samples.last()?;
    let mut min_observed_hp = entry_hp;
    let mut observed_hp_decrease = 0_i32;
    let mut observed_hp_increase = 0_i32;
    for pair in samples.windows(2) {
        let previous = pair[0].0;
        let current = pair[1].0;
        min_observed_hp = min_observed_hp.min(current);
        if current < previous {
            observed_hp_decrease = observed_hp_decrease.saturating_add(previous - current);
        } else if current > previous {
            observed_hp_increase = observed_hp_increase.saturating_add(current - previous);
        }
    }
    let (post_combat_hp, post_combat_max_hp) = post_combat.unzip();
    Some(CombatHealthSummaryV1 {
        entry_hp,
        entry_max_hp,
        min_observed_hp,
        last_combat_observation_hp,
        last_combat_observation_max_hp,
        post_combat_hp,
        post_combat_max_hp,
        net_hp_change: post_combat_hp.map(|hp| hp.saturating_sub(entry_hp)),
        observed_hp_decrease,
        observed_hp_increase,
        completion_boundary_delta: post_combat_hp
            .map(|hp| hp.saturating_sub(last_combat_observation_hp)),
        observed_player_turns: turns.len(),
        last_internal_turn_index: turns.last().copied().unwrap_or(0),
    })
}

fn render_episode_result_v1(
    output: &mut String,
    before: &TaskStrategicObservationV1,
    episode: &RunPlayJourneyEpisodeV1,
) {
    let last = episode.steps.last().expect("episode is non-empty");
    let Some(after) = observed_decision_v1(&last.observed_boundary) else {
        writeln!(
            output,
            "  result {}",
            observed_boundary_label_v1(&last.observed_boundary)
        )
        .expect("string write");
        return;
    };
    match &after.observation {
        TaskPublicObservationV1::Strategic { observation: after } => {
            let mut deltas = Vec::new();
            push_scalar_delta_v1(
                &mut deltas,
                "hp",
                before.run.current_hp,
                after.run.current_hp,
            );
            push_scalar_delta_v1(&mut deltas, "max_hp", before.run.max_hp, after.run.max_hp);
            push_scalar_delta_v1(&mut deltas, "gold", before.run.gold, after.run.gold);
            let (mut added, mut removed) = (Vec::new(), Vec::new());
            for (label, delta) in crate::compact::permanent_card_changes_v1(
                &before.cards,
                &after.cards,
                sts_presentation::Locale::En,
            ) {
                let count = delta.unsigned_abs();
                let label = if count == 1 {
                    label
                } else {
                    format!("{label}x{count}")
                };
                if delta > 0 {
                    added.push(label);
                } else {
                    removed.push(label);
                }
            }
            if !added.is_empty() {
                deltas.push(format!("cards+=[{}]", added.join(", ")));
            }
            if !removed.is_empty() {
                deltas.push(format!("cards-=[{}]", removed.join(", ")));
            }
            let before_relics = render_relics_v1(before);
            let after_relics = render_relics_v1(after);
            if before_relics != after_relics {
                deltas.push(format!("relics=[{}]->[{}]", before_relics, after_relics));
            }
            let before_potions = render_potions_v1(before);
            let after_potions = render_potions_v1(after);
            if before_potions != after_potions {
                deltas.push(format!("potions=[{}]->[{}]", before_potions, after_potions));
            }
            if before.run.keys != after.run.keys {
                deltas.push(format!("keys={:?}->{:?}", before.run.keys, after.run.keys));
            }
            writeln!(
                output,
                "  result next={:?} hp={}/{} gold={} changes=[{}]",
                after.decision_site,
                after.run.current_hp,
                after.run.max_hp,
                after.run.gold,
                if deltas.is_empty() {
                    "none".to_string()
                } else {
                    deltas.join(" | ")
                },
            )
            .expect("string write");
        }
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => {
            let encounter =
                combat_encounter_label_v1(run_context).unwrap_or_else(|| "unavailable".to_string());
            writeln!(
                output,
                "  result next=Combat encounter={} hp={}/{} gold={}",
                encounter,
                observation.player.hp,
                observation.player.max_hp,
                observation.player.gold
            )
            .expect("string write");
        }
    }
}

fn episode_title_v1(episode: &RunPlayJourneyEpisodeV1) -> String {
    let first = episode.steps.first().expect("episode is non-empty");
    match episode.family {
        RunPlayJourneyEpisodeFamilyV1::Combat => match &first.source.observation {
            TaskPublicObservationV1::Combat { run_context, .. } => {
                combat_encounter_label_v1(run_context)
                    .map(|encounter| format!("Combat({encounter})"))
                    .unwrap_or_else(|| "Combat".to_string())
            }
            TaskPublicObservationV1::Strategic { .. } => "Combat".to_string(),
        },
        RunPlayJourneyEpisodeFamilyV1::Event => match &first.source.observation {
            TaskPublicObservationV1::Strategic { observation } => match &observation.context {
                PlannerDecisionContext::Event { event, .. } => {
                    format!("Event({})", gameplay_display_label_v1(*event))
                }
                _ => "Event".to_string(),
            },
            TaskPublicObservationV1::Combat { .. } => "Event".to_string(),
        },
        family => format!("{family:?}").trim_end_matches("V1").to_string(),
    }
}

fn combat_encounter_label_v1(run_context: &PublicCombatRunContextV1) -> Option<String> {
    match run_context {
        PublicCombatRunContextV1::Available {
            encounter_id: Some(encounter),
            ..
        } => Some(gameplay_display_label_v1(*encounter).to_string()),
        _ => None,
    }
}

fn observed_decision_v1(boundary: &TaskObservedBoundaryV1) -> Option<&TaskPublicDecisionV1> {
    match boundary {
        TaskObservedBoundaryV1::Decision { decision }
        | TaskObservedBoundaryV1::CommittedNextScope { decision } => Some(decision),
        TaskObservedBoundaryV1::CommitReady
        | TaskObservedBoundaryV1::ExploredScopeExit { .. }
        | TaskObservedBoundaryV1::CommittedGameTerminal { .. } => None,
    }
}

fn observed_boundary_label_v1(boundary: &TaskObservedBoundaryV1) -> &'static str {
    match boundary {
        TaskObservedBoundaryV1::Decision { .. } => "decision",
        TaskObservedBoundaryV1::CommitReady => "commit-ready",
        TaskObservedBoundaryV1::ExploredScopeExit { .. } => "scope-exit",
        TaskObservedBoundaryV1::CommittedNextScope { .. } => "next-scope",
        TaskObservedBoundaryV1::CommittedGameTerminal { .. } => "terminal",
    }
}

fn render_window_v1(window: RunPlayJourneyWindowV1) -> String {
    match window {
        RunPlayJourneyWindowV1::First { count } => format!("first:{count}"),
        RunPlayJourneyWindowV1::Last { count } => format!("last:{count}"),
        RunPlayJourneyWindowV1::AfterDepth { depth, count } => {
            format!("after-depth:{depth},count:{count}")
        }
    }
}

fn render_optional_signed_v1(value: Option<i32>) -> String {
    value.map_or_else(|| "unavailable".to_string(), |value| format!("{value:+}"))
}

fn render_terminal_outcome_v1(outcome: &TaskEpisodeOutcomeV1) -> String {
    match outcome {
        TaskEpisodeOutcomeV1::Success { game_outcome } => format!("success:{game_outcome:?}"),
        TaskEpisodeOutcomeV1::Paused { reason } => format!("paused:{reason:?}"),
        TaskEpisodeOutcomeV1::Failure {
            reason,
            game_outcome,
        } => game_outcome.as_ref().map_or_else(
            || format!("failure:{reason:?}"),
            |game_outcome| format!("failure:{reason:?}:{game_outcome:?}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_reward_history_uses_real_adopted_card_change_without_mutating_the_source() {
        let directory = crate::decision_tests::TestDirectory::new();
        let mut game = crate::decision_tests::controller_with_session(
            &directory.0,
            "history-reward",
            &crate::decision_tests::fixture("reward"),
        );
        let selector =
            crate::selector::parse_action_selector_v1("take:ShrugItOff", "history regression")
                .unwrap();
        game.choose_action_v1(&selector, true).unwrap();
        let paths = crate::storage::RunPlayPathsV1::new(directory.0.join("history-reward"));
        let before = paths.load().unwrap();
        let fingerprint = game.current_state_fingerprints_v1().unwrap();
        let history =
            crate::history::journey_history_v1(&paths, RunPlayJourneyWindowV1::Last { count: 2 })
                .unwrap();
        assert!(history.total_episodes > 0);
        let text = render_journey_history_v1(&history, true);
        assert!(text.contains("cards+=[Shrug It Off]"), "{text}");
        assert!(!text.contains("cards-=["), "{text}");
        let after = paths.load().unwrap();
        assert_eq!(after.head, before.head);
        assert_eq!(after.state, before.state);
        assert_eq!(game.current_state_fingerprints_v1().unwrap(), fingerprint);
    }

    #[test]
    fn combat_health_keeps_observed_damage_separate_from_post_combat_recovery() {
        let samples = [(68, 75), (68, 75), (62, 75), (62, 75)];
        let turns = BTreeSet::from([0, 1, 2]);

        let summary = summarize_combat_health_samples_v1(&samples, &turns, Some((68, 75)))
            .expect("combat health summary");

        assert_eq!(summary.entry_hp, 68);
        assert_eq!(summary.min_observed_hp, 62);
        assert_eq!(summary.last_combat_observation_hp, 62);
        assert_eq!(summary.post_combat_hp, Some(68));
        assert_eq!(summary.observed_hp_decrease, 6);
        assert_eq!(summary.observed_hp_increase, 0);
        assert_eq!(summary.completion_boundary_delta, Some(6));
        assert_eq!(summary.net_hp_change, Some(0));
        assert_eq!(summary.observed_player_turns, 3);
        assert_eq!(summary.last_internal_turn_index, 2);
    }
}
