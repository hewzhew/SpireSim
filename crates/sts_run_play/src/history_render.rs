use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use sts_presentation::Locale;

use sts_oracle_learning::agent::information::action::PublicCombatAtomicActionV1;
use sts_oracle_learning::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_learning::agent::task::{
    TaskObservedBoundaryV1, TaskPublicCandidateV1, TaskPublicChoiceV1, TaskPublicDecisionV1,
    TaskPublicObservationV1, TaskStrategicObservationV1,
};
use sts_oracle_learning::ai::planner_core::PlannerDecisionContext;

use crate::history::{
    RunPlayJourneyEpisodeFamilyV1, RunPlayJourneyEpisodeV1, RunPlayJourneyHistoryV1,
    RunPlayJourneyTerminalV1, RunPlayJourneyWindowV1,
};
use crate::history_state::{
    combat_boundary_deltas_v1, message, push_scalar_delta_v1, render_deck_v1, render_potions_v1,
    render_relics_v1,
};
use crate::render::short_id;
use crate::selector::semantic_action_for_choice_v1;

pub fn render_journey_history_v1(
    history: &RunPlayJourneyHistoryV1,
    detail: bool,
    locale: Locale,
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        message(
            locale,
            "heading",
            &[
                ("episodes", &(history.total_episodes.to_string())),
                ("boundaries", &(history.total_path_boundaries.to_string())),
                ("showing", &(history.episodes.len().to_string())),
                ("window", &(render_window_v1(history.window, locale))),
                (
                    "current",
                    &(short_id(&history.current_archive_node_id).to_string())
                )
            ]
        )
    )
    .expect("string write");
    if let Some(depth) = history.next_after_depth {
        writeln!(
            output,
            "{}",
            message(locale, "next", &[("depth", &(depth.to_string()))])
        )
        .expect("string write");
    }

    for (offset, episode) in history.episodes.iter().enumerate() {
        let title = episode_title_v1(episode, locale);
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
            "{}",
            message(
                locale,
                "episode",
                &[
                    (
                        "episode",
                        &((history.first_episode_ordinal + offset).to_string())
                    ),
                    ("act", &(act)),
                    ("floor", &(floor)),
                    ("title", &(title)),
                    ("first", &(first.depth.to_string())),
                    ("last", &(last.depth.to_string())),
                    ("count", &(episode.steps.len().to_string())),
                    ("from", &(short_id(&first.from_archive_node_id).to_string())),
                    ("to", &(short_id(&last.to_archive_node_id).to_string()))
                ]
            )
        )
        .expect("string write");

        match episode.family {
            RunPlayJourneyEpisodeFamilyV1::Combat => {
                let terminal = (last.to_archive_node_id == history.current_archive_node_id)
                    .then_some(history.terminal.as_ref())
                    .flatten();
                render_combat_episode_v1(&mut output, episode, detail, terminal, locale)
            }
            _ => render_strategic_episode_v1(&mut output, episode, detail, locale),
        }
    }
    output
}

fn render_strategic_episode_v1(
    output: &mut String,
    episode: &RunPlayJourneyEpisodeV1,
    detail: bool,
    locale: Locale,
) {
    let Some(TaskPublicObservationV1::Strategic {
        observation: before,
    }) = episode.steps.first().map(|step| &step.source.observation)
    else {
        writeln!(output, "{}", message(locale, "noStrategicSource", &[])).expect("string write");
        return;
    };
    writeln!(
        output,
        "{}",
        message(
            locale,
            "before",
            &[
                ("hp", &(before.run.current_hp.to_string())),
                ("maxHp", &(before.run.max_hp.to_string())),
                ("gold", &(before.run.gold.to_string())),
                ("deck", &(before.cards.len().to_string())),
                ("relics", &(render_relics_v1(before, locale))),
                ("potions", &(render_potions_v1(before, locale))),
                ("keys", &(format!("{:?}", before.run.keys)))
            ]
        )
    )
    .expect("string write");
    if detail {
        writeln!(
            output,
            "{}",
            message(
                locale,
                "deck",
                &[("cards", &(render_deck_v1(before, locale)))]
            )
        )
        .expect("string write");
    }

    for step in &episode.steps {
        let selected_ordinal = step.choice.ordinal_path.first().copied();
        let selected_action = semantic_action_for_choice_v1(&step.source, &step.choice)
            .unwrap_or_else(|error| format!("<semantic-error:{error}>"));
        let selected_human = selected_ordinal
            .and_then(|ordinal| step.source.candidates.get(ordinal))
            .map(|candidate| candidate_label(candidate, &step.source.observation, locale))
            .unwrap_or_else(|| message(locale, "noCandidate", &[]));
        writeln!(
            output,
            "{}",
            message(
                locale,
                "step",
                &[
                    ("depth", &(step.depth.to_string())),
                    ("site", &(site_label(step.source.site, locale))),
                    ("count", &(step.source.candidates.len().to_string())),
                    (
                        "forced",
                        &(if step.source.candidates.len() == 1 {
                            message(locale, "forced", &[])
                        } else {
                            String::new()
                        })
                    ),
                    ("choice", &(format!("{:?}", step.choice.ordinal_path))),
                    ("label", &(selected_human)),
                    ("selector", &(selected_action.clone())),
                    (
                        "boundary",
                        &(observed_boundary_label_v1(&step.observed_boundary, locale))
                    )
                ]
            )
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
                    candidate_label(candidate, &step.source.observation, locale),
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
                        .then(|| candidate_label(candidate, &step.source.observation, locale))
                    })
                    .unwrap_or_else(|| message(locale, "noCandidate", &[]));
                writeln!(
                    output,
                    "{}",
                    message(
                        locale,
                        "suggestion",
                        &[
                            ("producer", &(provenance.producer_name.clone())),
                            ("revision", &provenance.producer_revision.to_string()),
                            ("access", &(format!("{:?}", provenance.access))),
                            (
                                "relation",
                                &(message(
                                    locale,
                                    if selector == &selected_action {
                                        "accepted"
                                    } else {
                                        "overridden"
                                    },
                                    &[]
                                ))
                            ),
                            ("label", &(human)),
                            ("selector", &(selector.clone()))
                        ]
                    )
                )
                .expect("string write");
            } else {
                writeln!(
                    output,
                    "{}",
                    message(
                        locale,
                        "noSuggestion",
                        &[
                            ("producer", &(provenance.producer_name.clone())),
                            ("revision", &provenance.producer_revision.to_string()),
                            ("access", &(format!("{:?}", provenance.access)))
                        ]
                    )
                )
                .expect("string write");
            }
        }
    }

    render_episode_result_v1(output, before, episode, locale);
}

fn render_combat_episode_v1(
    output: &mut String,
    episode: &RunPlayJourneyEpisodeV1,
    detail: bool,
    terminal: Option<&RunPlayJourneyTerminalV1>,
    locale: Locale,
) {
    let terminal_resources = terminal.and_then(|terminal| terminal.research_outcome.as_ref());
    let Some(health) = combat_health_summary_v1(
        episode,
        terminal_resources.map(|outcome| (outcome.final_hp, outcome.persistent.max_hp)),
    ) else {
        writeln!(output, "{}", message(locale, "noCombatSource", &[])).expect("string write");
        return;
    };
    writeln!(
        output,
        "{}",
        message(
            locale,
            "combatHealth",
            &[
                ("entry", &(health.entry_hp.to_string())),
                ("max", &(health.entry_max_hp.to_string())),
                ("min", &(health.min_observed_hp.to_string())),
                ("last", &(health.last_combat_observation_hp.to_string())),
                (
                    "lastMax",
                    &(health.last_combat_observation_max_hp.to_string())
                ),
                (
                    "post",
                    &(health
                        .post_combat_hp
                        .zip(health.post_combat_max_hp)
                        .map_or_else(
                            || message(locale, "unavailable", &[]),
                            |(hp, max)| format!("{hp}/{max}")
                        ))
                ),
                (
                    "net",
                    &(render_optional_signed_v1(health.net_hp_change, locale))
                ),
                ("decrease", &(health.observed_hp_decrease.to_string())),
                ("increase", &(health.observed_hp_increase.to_string())),
                (
                    "completion",
                    &(render_optional_signed_v1(health.completion_boundary_delta, locale))
                )
            ]
        )
    )
    .expect("string write");
    writeln!(
        output,
        "{}",
        message(
            locale,
            "combatProgress",
            &[
                ("actions", &(episode.steps.len().to_string())),
                ("turns", &(health.observed_player_turns.to_string())),
                ("last", &(health.last_internal_turn_index.to_string()))
            ]
        )
    )
    .expect("string write");
    let card_counts = combat_episode_card_choices_v1(episode, locale);
    writeln!(
        output,
        "{}",
        message(
            locale,
            "cardPlays",
            &[(
                "cards",
                &(card_counts
                    .iter()
                    .map(|(card, count)| format!("{card} x{count}"))
                    .collect::<Vec<_>>()
                    .join(", "))
            )]
        )
    )
    .expect("string write");
    if detail {
        render_combat_boundary_changes_v1(output, episode, locale);
    }
    let last = episode.steps.last().expect("combat episode is non-empty");
    let potions_used = combat_episode_potions_used_v1(episode);
    if let Some(terminal) = terminal {
        if let Some(resources) = terminal_resources {
            writeln!(
                output,
                "{}",
                message(
                    locale,
                    "terminalResult",
                    &[
                        (
                            "outcome",
                            &(crate::compact::render_outcome_compact(&terminal.outcome, locale))
                        ),
                        ("hp", &(resources.final_hp.to_string())),
                        ("maxHp", &(resources.persistent.max_hp.to_string())),
                        ("gold", &(resources.final_gold.to_string())),
                        ("potions", &(potions_used.to_string()))
                    ]
                )
            )
            .expect("string write");
        } else {
            writeln!(
                output,
                "{}",
                message(
                    locale,
                    "terminalNoResources",
                    &[
                        (
                            "outcome",
                            &(crate::compact::render_outcome_compact(&terminal.outcome, locale))
                        ),
                        ("potions", &(potions_used.to_string()))
                    ]
                )
            )
            .expect("string write");
        }
    } else if let Some(after) = observed_decision_v1(&last.observed_boundary) {
        match &after.observation {
            TaskPublicObservationV1::Strategic { observation } => {
                writeln!(
                    output,
                    "{}",
                    message(
                        locale,
                        "combatResult",
                        &[
                            ("site", &(site_label(after.site, locale))),
                            ("hp", &(observation.run.current_hp.to_string())),
                            ("maxHp", &(observation.run.max_hp.to_string())),
                            ("gold", &(observation.run.gold.to_string())),
                            ("potions", &(potions_used.to_string()))
                        ]
                    )
                )
                .expect("string write");
            }
            TaskPublicObservationV1::Combat { observation, .. } => {
                writeln!(
                    output,
                    "{}",
                    message(
                        locale,
                        "continuingCombat",
                        &[
                            ("hp", &(observation.player.hp.to_string())),
                            ("maxHp", &(observation.player.max_hp.to_string()))
                        ]
                    )
                )
                .expect("string write");
            }
        }
    } else {
        writeln!(
            output,
            "{}",
            message(
                locale,
                "boundaryResult",
                &[(
                    "boundary",
                    &(observed_boundary_label_v1(&last.observed_boundary, locale))
                )]
            )
        )
        .expect("string write");
    }
}

fn combat_episode_card_choices_v1(
    episode: &RunPlayJourneyEpisodeV1,
    locale: Locale,
) -> Vec<(String, usize)> {
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
        let identity = observation
            .cards
            .hand
            .cards
            .get(*hand_index)
            .map(|card| (card.card_id.stable_id(), card.upgrades));
        *counts.entry(identity).or_insert(0) += 1;
    }
    let mut identities = BTreeMap::<_, BTreeSet<_>>::new();
    for (id, upgrades) in counts.keys().flatten() {
        identities
            .entry(locale.card_name(id, *upgrades))
            .or_default()
            .insert(*id);
    }
    counts
        .into_iter()
        .map(|(identity, count)| {
            let label = identity.map_or_else(
                || message(locale, "noCard", &[]),
                |(id, upgrades)| {
                    // Keep same-name cards distinct in both languages.
                    let mut name = locale.card_name(id, upgrades);
                    if identities[&name].len() > 1 {
                        write!(name, " [{id}]").expect("string write");
                    }
                    name
                },
            );
            (label, count)
        })
        .collect()
}

fn render_combat_boundary_changes_v1(
    output: &mut String,
    episode: &RunPlayJourneyEpisodeV1,
    locale: Locale,
) {
    writeln!(output, "{}", message(locale, "boundaryChanges", &[])).expect("string write");
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
            }) => combat_boundary_deltas_v1(before, after, locale),
            Some(TaskPublicObservationV1::Strategic { observation: after }) => vec![message(
                locale,
                "completionHp",
                &[
                    ("before", &before.player.hp.to_string()),
                    ("after", &after.run.current_hp.to_string()),
                ],
            )],
            None => vec![message(
                locale,
                "noSuccessor",
                &[(
                    "boundary",
                    &observed_boundary_label_v1(&step.observed_boundary, locale),
                )],
            )],
        };
        writeln!(
            output,
            "    d{} T{} {} | {}",
            step.depth,
            before.turn.turn_count.saturating_add(1),
            action,
            if changes.is_empty() {
                message(locale, "noChanges", &[])
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
    locale: Locale,
) {
    let last = episode.steps.last().expect("episode is non-empty");
    let Some(after) = observed_decision_v1(&last.observed_boundary) else {
        writeln!(
            output,
            "{}",
            message(
                locale,
                "boundaryResult",
                &[(
                    "boundary",
                    &(observed_boundary_label_v1(&last.observed_boundary, locale))
                )]
            )
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
            for (label, delta) in
                crate::compact::permanent_card_changes_v1(&before.cards, &after.cards, locale)
            {
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
            let before_relics = render_relics_v1(before, locale);
            let after_relics = render_relics_v1(after, locale);
            if before.relics != after.relics {
                deltas.push(format!("relics=[{}]->[{}]", before_relics, after_relics));
            }
            let before_potions = render_potions_v1(before, locale);
            let after_potions = render_potions_v1(after, locale);
            if before
                .potions
                .iter()
                .map(|slot| (slot.slot, slot.potion.as_ref().map(|p| p.potion)))
                .collect::<Vec<_>>()
                != after
                    .potions
                    .iter()
                    .map(|slot| (slot.slot, slot.potion.as_ref().map(|p| p.potion)))
                    .collect::<Vec<_>>()
            {
                deltas.push(format!("potions=[{}]->[{}]", before_potions, after_potions));
            }
            if before.run.keys != after.run.keys {
                deltas.push(format!("keys={:?}->{:?}", before.run.keys, after.run.keys));
            }
            writeln!(
                output,
                "{}",
                message(
                    locale,
                    "strategicResult",
                    &[
                        ("site", &(site_label(after.decision_site.into(), locale))),
                        ("hp", &(after.run.current_hp.to_string())),
                        ("maxHp", &(after.run.max_hp.to_string())),
                        ("gold", &(after.run.gold.to_string())),
                        (
                            "changes",
                            &(if deltas.is_empty() {
                                message(locale, "none", &[])
                            } else {
                                deltas.join(" | ")
                            })
                        )
                    ]
                )
            )
            .expect("string write");
        }
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => {
            let encounter = combat_encounter_label_v1(run_context, locale)
                .unwrap_or_else(|| message(locale, "unavailable", &[]));
            writeln!(
                output,
                "{}",
                message(
                    locale,
                    "encounterResult",
                    &[
                        ("encounter", &(encounter)),
                        ("hp", &(observation.player.hp.to_string())),
                        ("maxHp", &(observation.player.max_hp.to_string())),
                        ("gold", &(observation.player.gold.to_string()))
                    ]
                )
            )
            .expect("string write");
        }
    }
}

fn episode_title_v1(episode: &RunPlayJourneyEpisodeV1, locale: Locale) -> String {
    let first = episode.steps.first().expect("episode is non-empty");
    match episode.family {
        RunPlayJourneyEpisodeFamilyV1::Combat => match &first.source.observation {
            TaskPublicObservationV1::Combat { run_context, .. } => {
                combat_encounter_label_v1(run_context, locale)
                    .map(|encounter| message(locale, "namedCombat", &[("encounter", &encounter)]))
                    .unwrap_or_else(|| message(locale, "site.Combat", &[]))
            }
            TaskPublicObservationV1::Strategic { .. } => message(locale, "site.Combat", &[]),
        },
        RunPlayJourneyEpisodeFamilyV1::Event => match &first.source.observation {
            TaskPublicObservationV1::Strategic { observation } => match &observation.context {
                PlannerDecisionContext::Event { event, .. } => message(
                    locale,
                    "namedEvent",
                    &[("event", locale.entity("event", event.stable_id()))],
                ),
                _ => message(locale, "site.Event", &[]),
            },
            TaskPublicObservationV1::Combat { .. } => message(locale, "site.Event", &[]),
        },
        family => message(locale, &format!("site.{family:?}"), &[]),
    }
}

fn combat_encounter_label_v1(
    run_context: &PublicCombatRunContextV1,
    locale: Locale,
) -> Option<String> {
    match run_context {
        PublicCombatRunContextV1::Available {
            encounter_id: Some(encounter),
            ..
        } => Some(locale.entity("encounter", encounter.stable_id()).to_owned()),
        _ => None,
    }
}

fn candidate_label(
    candidate: &TaskPublicCandidateV1,
    public: &TaskPublicObservationV1,
    locale: Locale,
) -> String {
    if let TaskPublicObservationV1::Strategic { observation } = public {
        return crate::decision::option_label(candidate, observation, public, false, locale)
            .unwrap_or_else(|error| format!("<display-error:{error}>"));
    }
    crate::render::render_candidate_with_observation(candidate, Some(public), locale)
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

fn observed_boundary_label_v1(boundary: &TaskObservedBoundaryV1, locale: Locale) -> String {
    let key = match boundary {
        TaskObservedBoundaryV1::Decision { .. } => "decision",
        TaskObservedBoundaryV1::CommitReady => "commitReady",
        TaskObservedBoundaryV1::ExploredScopeExit { .. } => "scopeExit",
        TaskObservedBoundaryV1::CommittedNextScope { .. } => "nextScope",
        TaskObservedBoundaryV1::CommittedGameTerminal { .. } => "terminal",
    };
    message(locale, &format!("boundary.{key}"), &[])
}

fn site_label(
    site: sts_oracle_learning::agent::task::TaskDecisionSiteV1,
    locale: Locale,
) -> String {
    message(locale, &format!("site.{site:?}"), &[])
}

fn render_window_v1(window: RunPlayJourneyWindowV1, locale: Locale) -> String {
    match window {
        RunPlayJourneyWindowV1::First { count } => {
            message(locale, "windowFirst", &[("count", &count.to_string())])
        }
        RunPlayJourneyWindowV1::Last { count } => {
            message(locale, "windowLast", &[("count", &count.to_string())])
        }
        RunPlayJourneyWindowV1::AfterDepth { depth, count } => message(
            locale,
            "windowAfter",
            &[("depth", &depth.to_string()), ("count", &count.to_string())],
        ),
    }
}

fn render_optional_signed_v1(value: Option<i32>, locale: Locale) -> String {
    value.map_or_else(
        || message(locale, "unavailable", &[]),
        |value| format!("{value:+}"),
    )
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
        let text = render_journey_history_v1(&history, true, Locale::En);
        assert!(text.contains("cards+=[Shrug It Off]"), "{text}");
        assert!(!text.contains("cards-=["), "{text}");
        let original = history.clone();
        let chinese = render_journey_history_v1(&history, true, Locale::ZhCn);
        assert!(chinese.starts_with("行程："), "{chinese}");
        assert!(chinese.contains("cards+=[耸肩无视]"), "{chinese}");
        for step in history.episodes.iter().flat_map(|episode| &episode.steps) {
            let selector = semantic_action_for_choice_v1(&step.source, &step.choice).unwrap();
            assert!(text.contains(&format!("selector={selector}")), "{text}");
            assert!(
                chinese.contains(&format!("selector={selector}")),
                "{chinese}"
            );
        }
        assert_eq!(render_journey_history_v1(&history, true, Locale::En), text);
        assert_eq!(history, original);
        let after = paths.load().unwrap();
        assert_eq!(after.head, before.head);
        assert_eq!(after.state, before.state);
        assert_eq!(game.current_state_fingerprints_v1().unwrap(), fingerprint);
    }

    #[test]
    fn retained_combat_history_localizes_observed_results_without_replaying_actions() {
        let directory = crate::decision_tests::TestDirectory::new();
        let mut game = crate::decision_tests::controller_with_session(
            &directory.0,
            "history-combat",
            &crate::decision_tests::fixture("combat_finish"),
        );
        game.choose_action_v1(
            &crate::selector::parse_action_selector_v1("play:Strike@0", "history test").unwrap(),
            true,
        )
        .unwrap();
        let paths = crate::storage::RunPlayPathsV1::new(directory.0.join("history-combat"));
        let before = paths.load().unwrap();
        let fingerprint = game.current_state_fingerprints_v1().unwrap();
        let history =
            crate::history::journey_history_v1(&paths, RunPlayJourneyWindowV1::Last { count: 2 })
                .unwrap();
        let original = history.clone();
        let english = render_journey_history_v1(&history, true, Locale::En);
        let chinese = render_journey_history_v1(&history, true, Locale::ZhCn);
        assert!(
            english.contains("Chosen card plays [Strike x1]"),
            "{english}"
        );
        assert!(chinese.contains("主动出牌 [打击 x1]"), "{chinese}");
        assert!(
            english.contains("Combat(Jaw Worm)") && chinese.contains("战斗（大颚虫）"),
            "{english}\n{chinese}"
        );
        assert!(chinese.contains("play:Strike") && english.contains("play:Strike"));
        assert!(chinese.contains("战斗结算生命") && english.contains("at combat completion"));
        assert_eq!(history, original);
        let after = paths.load().unwrap();
        assert_eq!(after.head, before.head);
        assert_eq!(after.state, before.state);
        assert_eq!(game.current_state_fingerprints_v1().unwrap(), fingerprint);

        // A constructed same-name pair checks counting identity independently of translation.
        let mut episode = history
            .episodes
            .iter()
            .find(|episode| episode.family == RunPlayJourneyEpisodeFamilyV1::Combat)
            .unwrap()
            .clone();
        let mut green = episode.steps[0].clone();
        let TaskPublicObservationV1::Combat { observation, .. } = &mut green.source.observation
        else {
            panic!()
        };
        observation.cards.hand.cards[0].card_id =
            sts_oracle_learning::content::cards::CardId::StrikeG;
        episode.steps.push(green);
        for locale in [Locale::En, Locale::ZhCn] {
            let counts = combat_episode_card_choices_v1(&episode, locale);
            assert_eq!(counts.len(), 2);
            assert!(counts.iter().all(|(_, count)| *count == 1));
            assert!(counts.iter().any(|(label, _)| label.contains("[Strike]")));
            assert!(counts.iter().any(|(label, _)| label.contains("[StrikeG]")));
        }
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
