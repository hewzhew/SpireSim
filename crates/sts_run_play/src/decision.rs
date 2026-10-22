//! Source-bound strategic interaction. The controller still owns all game
//! state, settlement and durable history; labels are never executable input.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use sts_presentation::Locale;

use serde::{Deserialize, Serialize};
use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1, TaskPublicChoiceV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskStrategicActionV1, TaskStrategicCardV1, TaskStrategicObservationV1,
};
use sts_oracle_learning::ai::planner_core::{
    stable_planner_id, PlannerDecisionContext, PlannerMapRoom, PlannerRewardDescriptor,
    PlannerShopInventoryV1, PlannerShopPurchaseBlockReasonV1, PlannerShopPurchaseStatusV1,
};
use sts_oracle_learning::content::identity::gameplay_display_label_v1;
use sts_oracle_learning::eval::run_control::ExactRunDecisionSourceV1;

use crate::model::{RunPlayEventV1, RunPlayHeadV1, RunPlayPositionV1};
use crate::render::{render_candidate_with_observation, render_reward_descriptor};
use crate::run_selection::{project_deck_cards_v1, project_run_selection_family_v1};

pub(crate) mod combat;
pub(crate) mod preparation;
mod smith;

fn text(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("decision", key, parameters)
        .expect("decision presentation message")
}

#[cfg(test)]
#[path = "decision/presentation_tests.rs"]
mod presentation_tests;

pub(crate) fn run_transition_label_v1(
    stage: sts_oracle_learning::ai::planner_core::PlannerRunTransitionV1,
    locale: Locale,
) -> &'static str {
    use sts_oracle_learning::ai::planner_core::PlannerRunTransitionV1;
    let key = match stage {
        PlannerRunTransitionV1::SecondBoss => "transition.secondBoss",
        PlannerRunTransitionV1::FinalAct => "transition.finalAct",
        PlannerRunTransitionV1::VictoryScreen => "transition.victory",
    };
    locale.entry("decision", key).expect("transition message")
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DecisionSelectionMemberV1 {
    pub ordinal: usize,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DecisionSelectionV1 {
    pub label: String,
    pub choice: TaskPublicChoiceV1,
    pub min: u64,
    pub max: u64,
    pub members: Vec<DecisionSelectionMemberV1>,
}

/// This envelope is assembled from an observed frame, not from model-authored
/// identities. Replaying it after any published change fails closed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunPlayBoundChoiceV1 {
    pub source: ExactRunDecisionSourceV1,
    pub choice: TaskPublicChoiceV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionDetailV1 {
    Brief,
    Map,
    Options,
    State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionControlV1 {
    Strategic,
    Combat,
    Terminal,
}

/// A caller's acknowledgement of public context it still retains. These
/// content references only control presentation; they never authorize actions.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DecisionKnownContextV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relics: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potions: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DecisionOptionV1 {
    pub choice: TaskPublicChoiceV1,
    pub label: String,
    pub candidate: TaskPublicCandidateV1,
}

/// A projection of the existing durable event, not a second execution journal.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DecisionChoiceReceiptV1 {
    pub head_revision: u64,
    pub from_archive_node_id: String,
    pub to_archive_node_id: Option<String>,
    pub choices: Vec<TaskPublicChoiceV1>,
}

fn last_choice(head: &RunPlayHeadV1) -> Option<DecisionChoiceReceiptV1> {
    match head.events.last()? {
        RunPlayEventV1::Choice {
            head_revision,
            from_archive_node_id,
            to_archive_node_id,
            choice,
            ..
        } => Some(DecisionChoiceReceiptV1 {
            head_revision: *head_revision,
            from_archive_node_id: from_archive_node_id.clone(),
            to_archive_node_id: to_archive_node_id.clone(),
            choices: vec![choice.clone()],
        }),
        RunPlayEventV1::AnchorLine {
            head_revision,
            from_archive_node_id,
            to_archive_node_id,
            choices,
            combat_research_candidate: None,
            combat_milestone_candidate: None,
            ..
        } => Some(DecisionChoiceReceiptV1 {
            head_revision: *head_revision,
            from_archive_node_id: from_archive_node_id.clone(),
            to_archive_node_id: Some(to_archive_node_id.clone()),
            choices: choices.clone(),
        }),
        _ => None,
    }
}

/// `view` is the compact model-facing reading. Source and typed options belong
/// to the client binding; full public state is an explicit inspection request.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RunPlayDecisionReadingV1 {
    pub control: DecisionControlV1,
    pub view: String,
    /// Current position rendered from the same public facts as `view`, without
    /// options or expanded rules. Consumers must not extract it from headings.
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<DecisionKnownContextV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ExactRunDecisionSourceV1>,
    pub options: Vec<DecisionOptionV1>,
    pub selections: Vec<DecisionSelectionV1>,
    pub can_collect_rewards: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<TaskPublicDecisionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_choice: Option<DecisionChoiceReceiptV1>,
}

pub(crate) fn decision_source(head: &RunPlayHeadV1) -> Result<ExactRunDecisionSourceV1, String> {
    let decision = head
        .current_decision
        .as_ref()
        .ok_or("No current decision")?;
    Ok(ExactRunDecisionSourceV1 {
        session_id: head.session_id.clone(),
        head_revision: head.head_revision,
        archive_node_id: head.position.archive_node_id().to_owned(),
        public_snapshot_id: decision.public_snapshot_id.clone(),
    })
}

pub(crate) fn read_decision(
    head: &RunPlayHeadV1,
    details: &[DecisionDetailV1],
    known: Option<&DecisionKnownContextV1>,
    locale: Locale,
) -> Result<RunPlayDecisionReadingV1, String> {
    if let RunPlayPositionV1::Terminal { outcome, .. } = &head.position {
        let summary = format!(
            "{}\n",
            text(locale, "terminal", &[("outcome", &format!("{outcome:?}"))])
        );
        return Ok(RunPlayDecisionReadingV1 {
            control: DecisionControlV1::Terminal,
            view: summary.clone(),
            summary,
            context: None,
            source: None,
            options: vec![],
            selections: vec![],
            can_collect_rewards: false,
            state: None,
            last_choice: last_choice(head),
        });
    }
    let decision = head
        .current_decision
        .as_ref()
        .ok_or("No current decision")?
        .public_decision_v1()?;
    read_public_decision_v1(
        &decision,
        details,
        known,
        Some(decision_source(head)?),
        last_choice(head),
        locale,
    )
}

/// Render a privately probed public successor with the same labels as a live
/// decision, but without manufacturing a durable source or choice receipt.
pub(crate) fn read_preview_decision_v1(
    decision: &TaskPublicDecisionV1,
    known: Option<&DecisionKnownContextV1>,
    locale: Locale,
) -> Result<RunPlayDecisionReadingV1, String> {
    read_public_decision_v1(
        decision,
        &[DecisionDetailV1::Brief],
        known,
        None,
        None,
        locale,
    )
}

fn read_public_decision_v1(
    decision: &TaskPublicDecisionV1,
    details: &[DecisionDetailV1],
    known: Option<&DecisionKnownContextV1>,
    source: Option<ExactRunDecisionSourceV1>,
    last_choice: Option<DecisionChoiceReceiptV1>,
    locale: Locale,
) -> Result<RunPlayDecisionReadingV1, String> {
    let mut result = RunPlayDecisionReadingV1 {
        control: DecisionControlV1::Strategic,
        view: String::new(),
        summary: String::new(),
        context: None,
        source,
        options: vec![],
        selections: vec![],
        can_collect_rewards: false,
        state: details
            .contains(&DecisionDetailV1::State)
            .then(|| (*decision).clone()),
        last_choice,
    };
    let observation = match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => observation,
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => {
            result.control = DecisionControlV1::Combat;
            result.summary = combat::summary(run_context, observation, locale);
            result.view = combat::reading(
                run_context,
                observation,
                details.contains(&DecisionDetailV1::Brief),
                locale,
            );
            combat::pending_choices(
                &decision,
                &mut result,
                details.contains(&DecisionDetailV1::Brief),
                locale,
            )?;
            if result.options.is_empty() && result.selections.is_empty() {
                result.source = None;
            }
            return Ok(result);
        }
    };
    let mut options = Vec::new();
    result.can_collect_rewards =
        matches!(observation.context, PlannerDecisionContext::Reward { .. })
            && decision.candidates.iter().any(|c| {
                matches!(
                    c,
                    TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::OpenCardReward { .. }
                    }
                )
            });
    for (ordinal, candidate) in decision.candidates.iter().enumerate() {
        if let Some(family) = project_run_selection_family_v1(&decision, candidate)? {
            result.selections.push(DecisionSelectionV1 {
                label: run_selection_label(family.reason, locale)?.into(),
                choice: TaskPublicChoiceV1::one(ordinal),
                min: family.min_choices as u64,
                max: family.max_choices as u64,
                members: family
                    .selectable_cards
                    .iter()
                    .enumerate()
                    .map(|(ordinal, card)| {
                        // Execution consumes the ordered domain index, not the
                        // master-deck ordinal carried by the public card.
                        Ok(DecisionSelectionMemberV1 {
                            ordinal,
                            label: if family.reason
                                == sts_oracle_learning::state::selection::SelectionReason::Upgrade
                            {
                                format!(
                                    "{} | {}",
                                    run_selection_card_label(card, observation, locale),
                                    smith::changes_for(*card, locale)?
                                )
                            } else {
                                run_selection_card_label(card, observation, locale)
                            },
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            });
            continue;
        }
        options.push(DecisionOptionV1 {
            choice: TaskPublicChoiceV1::one(ordinal),
            label: option_label(candidate, observation, &decision.observation, true, locale)?,
            candidate: candidate.clone(),
        });
    }
    let (primary, secondary): (Vec<_>, Vec<_>) = options.into_iter().partition(|option| {
        !matches!(
            &option.candidate,
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::DiscardRunPotion { .. }
                    | TaskStrategicActionV1::UseRunPotion { .. }
                    | TaskStrategicActionV1::RemoveCard { .. }
            }
        )
    });
    // Display positions belong to this reading. The original choice path is
    // carried intact, even when hidden candidates precede it in the engine.
    let (potion_rewards, free_potion_slots) = match &observation.context {
        PlannerDecisionContext::Reward { pending_rewards } => (
            pending_rewards
                .iter()
                .filter(|reward| matches!(reward, PlannerRewardDescriptor::Potion { .. }))
                .count(),
            observation
                .potions
                .iter()
                .filter(|slot| slot.potion.is_none())
                .count(),
        ),
        _ => (0, 0),
    };
    let potion_choice = potion_rewards > free_potion_slots
        && !observation
            .relics
            .iter()
            .any(|relic| relic.relic == sts_oracle_learning::content::relics::RelicId::Sozu);
    let expand_secondary = details.contains(&DecisionDetailV1::Options) || potion_choice;
    let tucked = if expand_secondary { 0 } else { secondary.len() };
    let use_potion_choices = secondary
        .iter()
        .filter(|option| {
            matches!(
                &option.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::UseRunPotion { .. }
                }
            )
        })
        .count();
    let discard_potion_choices = secondary
        .iter()
        .filter(|option| {
            matches!(
                &option.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::DiscardRunPotion { .. }
                }
            )
        })
        .count();
    let removal_choices = secondary
        .iter()
        .filter(|option| {
            matches!(
                &option.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::RemoveCard { .. }
                }
            )
        })
        .count();
    result.options = primary;
    if expand_secondary {
        result.options.extend(secondary);
    }
    let out = &mut result.view;
    let context = DecisionKnownContextV1 {
        deck: Some(stable_planner_id(
            "run-play-reading-deck-v1",
            &(locale, &observation.cards),
        )?),
        relics: Some(stable_planner_id(
            "run-play-reading-relics-v1",
            &(locale, &observation.relics),
        )?),
        potions: Some(stable_planner_id(
            "run-play-reading-potions-v1",
            &(locale, &observation.potions),
        )?),
    };
    let repeat_deck = known.is_none_or(|old| old.deck != context.deck);
    let repeat_relics = known.is_none_or(|old| old.relics != context.relics);
    let repeat_potions = known.is_none_or(|old| old.potions != context.potions);
    result.context = Some(context);
    let brief = details.contains(&DecisionDetailV1::Brief);
    let show_build_resources = brief
        && matches!(
            &observation.context,
            PlannerDecisionContext::CardReward { .. }
                | PlannerDecisionContext::Shop { .. }
                | PlannerDecisionContext::Campfire
                | PlannerDecisionContext::BossRelic
                | PlannerDecisionContext::RunChoice { .. }
                | PlannerDecisionContext::Event { .. }
        );
    let run = &observation.run;
    let heart_goal =
        observation.run_goal == sts_oracle_learning::ai::planner_core::PlannerRunGoal::HeartVictory;
    let context = match &observation.context {
        PlannerDecisionContext::Map { overlay } => {
            let mut label = if *overlay {
                text(locale, "context.mapOverlay", &[])
            } else {
                text(locale, "context.map", &[])
            };
            let map = &observation.public_map;
            if let Some(node) = map
                .nodes
                .iter()
                .find(|node| node.x == map.current_x && node.y == map.current_y)
            {
                label.push_str(&text(
                    locale,
                    if *overlay {
                        "context.currentNode"
                    } else {
                        "context.completedNode"
                    },
                    &[
                        ("x", &node.x.to_string()),
                        ("y", &node.y.to_string()),
                        ("room", room_label(node.room, locale)),
                    ],
                ));
            }
            label
        }
        PlannerDecisionContext::CardReward { .. } => text(locale, "context.cardReward", &[]),
        PlannerDecisionContext::Shop { .. } => text(locale, "context.shop", &[]),
        PlannerDecisionContext::Reward { pending_rewards } if pending_rewards.is_empty() => {
            text(locale, "context.rewardsComplete", &[])
        }
        PlannerDecisionContext::Reward { pending_rewards } => text(
            locale,
            "context.rewards",
            &[("count", &pending_rewards.len().to_string())],
        ),
        PlannerDecisionContext::Campfire => text(locale, "context.campfire", &[]),
        PlannerDecisionContext::BossRelic => text(locale, "context.bossRelic", &[]),
        PlannerDecisionContext::RunChoice { reason, source, .. } => text(
            locale,
            "context.cardSelection",
            &[
                ("reason", run_selection_label(*reason, locale)?),
                (
                    "source",
                    &source.map_or_else(String::new, |source| {
                        text(
                            locale,
                            "context.selectionSource",
                            &[("source", &format!("{source:?}"))],
                        )
                    }),
                ),
            ],
        ),
        PlannerDecisionContext::Treasure => text(locale, "context.treasure", &[]),
        PlannerDecisionContext::RunTransition { stage } => {
            run_transition_label_v1(*stage, locale).into()
        }
        PlannerDecisionContext::Event { event, screen, .. } => text(
            locale,
            "context.event",
            &[
                (
                    "event",
                    locale
                        .entry("game", &format!("event.{}", event.stable_id()))
                        .unwrap_or_else(|| gameplay_display_label_v1(*event)),
                ),
                ("screen", &screen.to_string()),
            ],
        ),
    };
    let header = |key| {
        text(
            locale,
            key,
            &[
                ("act", &run.act.to_string()),
                ("floor", &run.floor.to_string()),
                ("hp", &run.current_hp.to_string()),
                ("maxHp", &run.max_hp.to_string()),
                ("gold", &run.gold.to_string()),
                ("context", &context),
                ("ascension", &run.ascension_level.to_string()),
                (
                    "energy",
                    &crate::agent_protocol::public_base_energy_per_turn_v1(&observation.relics)
                        .to_string(),
                ),
            ],
        )
    };
    result.summary = format!("{}\n", header("summary"));
    if brief {
        out.push_str(&result.summary);
        if !show_build_resources {
            writeln!(
                out,
                "{}",
                text(
                    locale,
                    "deckCount",
                    &[("count", &observation.cards.len().to_string())]
                )
            )
            .unwrap();
        }
    } else {
        writeln!(out, "{}", header("fullSummary")).unwrap();
    }
    if brief && heart_goal && run.act == 3 {
        let key_choice = matches!(
            observation.context,
            PlannerDecisionContext::Map { .. }
                | PlannerDecisionContext::Campfire
                | PlannerDecisionContext::Treasure
        ) || matches!(&observation.context, PlannerDecisionContext::Reward { pending_rewards }
                if pending_rewards.iter().any(|reward| matches!(reward,
                    PlannerRewardDescriptor::SapphireKey | PlannerRewardDescriptor::EmeraldKey)));
        let missing = ["ruby", "sapphire", "emerald"]
            .into_iter()
            .zip(run.keys)
            .filter_map(|(name, held)| (!held).then(|| text(locale, &format!("keys.{name}"), &[])))
            .collect::<Vec<_>>();
        if key_choice && !missing.is_empty() {
            writeln!(
                out,
                "{}",
                text(locale, "keys.stillNeeded", &[("keys", &missing.join(", "))])
            )
            .unwrap();
            let routes = result
                .options
                .iter()
                .filter_map(|option| match &option.candidate {
                    TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::ChooseRouteNode { x, y, .. },
                    } => observation
                        .public_map
                        .nodes
                        .iter()
                        .find(|node| node.x == *x && node.y == *y),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if !routes.is_empty()
                && routes.iter().all(|node| {
                    node.room == sts_oracle_learning::ai::planner_core::PlannerMapRoom::Boss
                })
            {
                writeln!(out, "{}", text(locale, "keys.act3Exit", &[])).unwrap();
            }
        }
    }
    if let Some(relic) = observation.relics.iter().filter(|_| !brief).find(|relic| {
        sts_oracle_learning::content::relics::mechanics::relic_prevents_gold_gain(relic.relic)
    }) {
        writeln!(
            out,
            "{}",
            text(
                locale,
                "goldBlocked",
                &[("relic", locale.entity("relic", relic.relic.stable_id()))]
            )
        )
        .unwrap();
    }
    if let PlannerDecisionContext::Event {
        event,
        screen,
        face_trader_result,
    } = &observation.context
    {
        if let Some(scene) = crate::event_scene::current_scene(*event, *screen, *face_trader_result)
        {
            writeln!(
                out,
                "{}",
                text(locale, "sceneEnglish", &[("scene", &scene)])
            )
            .unwrap();
        }
    }
    if (repeat_deck && !brief) || show_build_resources {
        let mut names = BTreeMap::<_, BTreeSet<_>>::new();
        for card in &observation.cards {
            names
                .entry(locale.entity("card", card.card.stable_id()))
                .or_default()
                .insert(card.card.stable_id());
        }
        let deck = crate::compact::inventory_card_groups_v1(&observation.cards);
        writeln!(
            out,
            "{}",
            text(
                locale,
                "deck",
                &[
                    ("count", &observation.cards.len().to_string()),
                    (
                        "cards",
                        &deck
                            .into_iter()
                            .map(|(count, card)| {
                                let mut label = deck_card_label(&card, locale);
                                if names[locale.entity("card", card.card.stable_id())].len() > 1 {
                                    write!(label, " [{}]", card.card.stable_id()).unwrap();
                                }
                                if count == 1 {
                                    label
                                } else {
                                    format!("{count}x {label}")
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                ]
            )
        )
        .unwrap();
    }
    if (repeat_relics && !brief) || show_build_resources {
        writeln!(
            out,
            "{}",
            text(
                locale,
                "relics",
                &[(
                    "relics",
                    &observation
                        .relics
                        .iter()
                        .map(|r| crate::compact::relic_label_v1(
                            r.relic, r.counter, r.used_up, locale
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                )]
            )
        )
        .unwrap();
    }
    if repeat_potions || brief {
        writeln!(
            out,
            "{}",
            text(
                locale,
                "potions",
                &[(
                    "potions",
                    &observation
                        .potions
                        .iter()
                        .map(|slot| format!(
                            "P{} {}",
                            slot.slot,
                            slot.potion.map_or_else(
                                || text(locale, "empty", &[]),
                                |p| locale.entity("potion", p.potion.stable_id()).to_string()
                            )
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                )]
            )
        )
        .unwrap();
    }
    if potion_choice {
        writeln!(
            out,
            "{}",
            text(
                locale,
                "potionChoice",
                &[
                    ("rewards", &potion_rewards.to_string()),
                    ("slots", &free_potion_slots.to_string())
                ]
            )
        )
        .unwrap();
    }
    if potion_rewards > 0
        && observation
            .relics
            .iter()
            .any(|relic| relic.relic == sts_oracle_learning::content::relics::RelicId::Sozu)
    {
        writeln!(out, "{}", text(locale, "sozuReward", &[])).unwrap();
    }
    let reused: Vec<_> = [
        ("deck", repeat_deck),
        ("relics", repeat_relics),
        ("potions", repeat_potions),
    ]
    .into_iter()
    .filter_map(|(name, repeated)| {
        (!repeated).then(|| text(locale, &format!("resource.{name}"), &[]))
    })
    .collect();
    if !reused.is_empty() && !brief {
        writeln!(
            out,
            "{}",
            text(locale, "unchanged", &[("resources", &reused.join(", "))])
        )
        .unwrap();
    }
    if !brief {
        crate::mechanics_info::write_key_status(out, run.keys, heart_goal, locale);
    }
    let bound = result.source.is_some();
    let shop_choices = if let PlannerDecisionContext::Shop { inventory, .. } = &observation.context
    {
        render_shop(
            out,
            decision,
            inventory,
            &result.options,
            brief,
            bound,
            locale,
        )?
    } else {
        BTreeSet::new()
    };
    // A legal-action list is not an inventory: full potion slots, for example,
    // remove the claim action while the reward remains publicly visible.
    if let PlannerDecisionContext::Reward { pending_rewards } = &observation.context {
        for (index, reward) in pending_rewards.iter().enumerate() {
            let has_choice = decision.candidates.iter().any(|candidate| {
                matches!(candidate, TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::ClaimReward { reward_item_index, .. }
                        | TaskStrategicActionV1::OpenCardReward { reward_item_index }
                } if *reward_item_index == index)
            });
            if !has_choice {
                writeln!(
                    out,
                    "{}",
                    text(
                        locale,
                        "reward.unavailable",
                        &[("reward", &render_reward_descriptor(reward, locale))]
                    )
                )
                .unwrap();
                if matches!(
                    reward,
                    sts_oracle_learning::ai::planner_core::PlannerRewardDescriptor::Potion { .. }
                ) {
                    if observation
                        .relics
                        .iter()
                        .any(|r| r.relic == sts_oracle_learning::content::relics::RelicId::Sozu)
                    {
                        writeln!(out, "{}", text(locale, "reward.sozu", &[])).unwrap();
                    } else if observation.potions.iter().all(|slot| slot.potion.is_some()) {
                        writeln!(out, "{}", text(locale, "reward.slotsFull", &[])).unwrap();
                    }
                }
                write_choice_mechanics(
                    out,
                    &TaskStrategicActionV1::ClaimReward {
                        reward_item_index: index,
                        reward: reward.clone(),
                    },
                    brief,
                    locale,
                );
            }
        }
    }
    render_selections(out, decision, &result.selections, bound, locale)?;
    let upgrades = smith::comparisons(observation, &result.options, locale)?;
    let route_choices: Vec<_> = result
        .options
        .iter()
        .filter_map(|option| match &option.candidate {
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ChooseRouteNode { x, y, .. },
            } => Some((*x, *y)),
            _ => None,
        })
        .collect();
    if bound && !route_choices.is_empty() {
        writeln!(
            out,
            "{}",
            text(
                locale,
                "nextRooms",
                &[("count", &route_choices.len().to_string())]
            )
        )
        .unwrap();
    }
    for (ordinal, option) in result.options.iter().enumerate() {
        if shop_choices.contains(&ordinal) {
            continue;
        }
        let label = if brief {
            if let Some(upgrade) = upgrades
                .iter()
                .find(|upgrade| upgrade.ordinals.contains(&ordinal))
            {
                if !details.contains(&DecisionDetailV1::Options) && upgrade.ordinals[0] != ordinal {
                    continue;
                }
                let count = if !details.contains(&DecisionDetailV1::Options)
                    && upgrade.ordinals.len() > 1
                {
                    text(
                        locale,
                        "copies",
                        &[("count", &upgrade.ordinals.len().to_string())],
                    )
                } else {
                    String::new()
                };
                format!("{}{} | {}", option.label, count, upgrade.changes)
            } else if let TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ChooseRouteNode { x, y, flight, .. },
            } = &option.candidate
            {
                route_option_label(observation, *x, *y, *flight, "", locale)
            } else {
                option_label(
                    &option.candidate,
                    observation,
                    &decision.observation,
                    false,
                    locale,
                )?
            }
        } else {
            option.label.clone()
        };
        render_option_row(out, decision, option, ordinal, &label, bound, locale)?;
        if let TaskPublicCandidateV1::Strategic { action } = &option.candidate {
            write_choice_mechanics(out, action, brief, locale);
            write_event_resource_modifiers(out, action, observation, locale);
        }
    }
    if !brief
        && details.contains(&DecisionDetailV1::State)
        && matches!(
            observation.context,
            PlannerDecisionContext::CardReward { .. } | PlannerDecisionContext::Shop { .. }
        )
    {
        let mut seen = std::collections::HashSet::new();
        for card in &observation.cards {
            if seen.insert((card.card, card.upgrades)) {
                if seen.len() == 1 {
                    writeln!(
                        out,
                        "Owned deck rules (deduplicated by card and upgrade; Power effects apply after deployment):"
                    )
                    .unwrap();
                    writeln!(out, "{}", crate::mechanics_info::EXHAUST_SCOPE).unwrap();
                }
                crate::mechanics_info::write_card_rule(out, card.card, card.upgrades, locale);
            }
        }
    }
    if tucked > 0 && !details.contains(&DecisionDetailV1::Options) {
        let kinds = [
            ("use held potion", use_potion_choices),
            ("discard held potion", discard_potion_choices),
            ("card removals", removal_choices),
        ]
        .into_iter()
        .filter_map(|(name, count)| (count > 0).then(|| format!("{name} x{count}")))
        .collect::<Vec<_>>()
        .join(", ");
        writeln!(out, "Other commands in options: {kinds}").unwrap();
    }
    // Current commands and their effects precede broader preparation context.
    // Whole-inventory mechanics have one owner: inventory --effects, also
    // injected by the player when it establishes a fresh decision context.
    if !brief {
        smith::write_full(out, &upgrades, locale);
    }
    if brief && !route_choices.is_empty() {
        out.push_str(&crate::public_map_reading::route_examples(
            &observation.public_map,
            &route_choices,
        )?);
    } else if brief && matches!(observation.context, PlannerDecisionContext::Map { .. }) {
        if let Some(boss) = observation.public_map.boss {
            let scope = if run.pending_act_entry.is_some() {
                "Completed"
            } else {
                "Published"
            };
            writeln!(out, "{scope} Boss: {}", gameplay_display_label_v1(boss)).unwrap();
        }
    }
    if !brief {
        crate::mechanics_info::write_preparation_window(out, observation, locale);
    }
    if !brief && heart_goal && run.keys.contains(&false) {
        writeln!(out, "{}", crate::mechanics_info::KEY_COLLECTION_TIMING).unwrap();
        if !run.keys[2] {
            writeln!(out, "{}", crate::mechanics_info::EMERALD_KEY_OPPORTUNITIES).unwrap();
        }
    }
    if !brief
        && run.pending_act_entry.is_none()
        && matches!(
            observation.context,
            PlannerDecisionContext::Shop { .. }
                | PlannerDecisionContext::Campfire
                | PlannerDecisionContext::Reward { .. }
                | PlannerDecisionContext::CardReward { .. }
        )
    {
        if let Some(routes) =
            crate::public_map_reading::preparation_routes(&observation.public_map)?
        {
            writeln!(out, "{routes}").unwrap();
        }
    }
    if details.contains(&DecisionDetailV1::Map) {
        writeln!(
            out,
            "Map (x,y); current ({},{})",
            observation.public_map.current_x, observation.public_map.current_y
        )
        .unwrap();
        for node in &observation.public_map.nodes {
            writeln!(
                out,
                "({},{}) {}{} -> {}",
                node.x,
                node.y,
                room_label(node.room, locale),
                if node.has_emerald_key {
                    " + Emerald Key"
                } else {
                    ""
                },
                node.edges
                    .iter()
                    .map(|e| format!("({},{})", e.destination_x, e.destination_y))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
        }
    }
    Ok(result)
}

pub(crate) fn deck_card_label(card: &TaskStrategicCardV1, locale: Locale) -> String {
    let mut label = locale.card_name(card.card.stable_id(), card.upgrades);
    if card.misc_value != 0 {
        write!(
            label,
            " ({})",
            locale
                .text(
                    "inventory",
                    "misc",
                    &[("amount", &card.misc_value.to_string())]
                )
                .unwrap()
        )
        .unwrap();
    }
    if card.cost_modifier != 0 {
        write!(
            label,
            " ({})",
            locale
                .text(
                    "inventory",
                    "cost",
                    &[("amount", &format!("{:+}", card.cost_modifier))]
                )
                .unwrap()
        )
        .unwrap();
    }
    if let Some(damage) = card.base_damage_override {
        write!(
            label,
            " ({})",
            locale
                .text("inventory", "damage", &[("amount", &damage.to_string())])
                .unwrap()
        )
        .unwrap();
    }
    if let Some(block) = card.base_block_override {
        write!(
            label,
            " ({})",
            locale
                .text("inventory", "block", &[("amount", &block.to_string())])
                .unwrap()
        )
        .unwrap();
    }
    if let Some(relic) = card.bottled_by {
        write!(label, " [{}]", locale.entity("relic", relic.stable_id())).unwrap();
    }
    label
}

fn run_selection_card_label(
    card: &TaskStrategicCardV1,
    observation: &TaskStrategicObservationV1,
    locale: Locale,
) -> String {
    let reward = crate::render::run_selection_reward_label_v1(observation, card.card)
        .map_or_else(String::new, |label| format!(" | {label}"));
    format!("{}{reward}", deck_card_label(card, locale))
}

fn run_selection_label(
    reason: sts_oracle_learning::state::selection::SelectionReason,
    locale: Locale,
) -> Result<&'static str, String> {
    use sts_oracle_learning::state::selection::SelectionReason::*;
    let key = match reason {
        Upgrade => "upgrade",
        Purge => "purge",
        Transform => "transform",
        TransformUpgraded => "transformUpgraded",
        Duplicate => "duplicate",
        BottleFlame => "bottleAttack",
        BottleLightning => "bottleSkill",
        BottleTornado => "bottlePower",
        _ => {
            return Err(format!(
                "Unexpected master-deck selection reason: {reason:?}"
            ))
        }
    };
    Ok(locale
        .entry("decision", &format!("deckSelection.{key}"))
        .expect("deck selection reason"))
}

/// Commands are projections of typed choices. Presentation never resolves a
/// choice by reading its label or an already formatted line.
fn choice_command(
    decision: &TaskPublicDecisionV1,
    choice: &TaskPublicChoiceV1,
) -> Result<String, String> {
    let semantic = crate::selector::semantic_action_for_choice_v1(decision, choice)?;
    match &decision.candidates[choice.ordinal_path[0]] {
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::ChooseRouteNode { x, y, .. },
        } => Ok(format!("go {x},{y}")),
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::SubmitRunSelection { .. },
        }
        | TaskPublicCandidateV1::CombatSelectionFamily { .. } => Ok(format!("act {semantic}")),
        _ => Ok(semantic),
    }
}

fn render_option_row(
    out: &mut String,
    decision: &TaskPublicDecisionV1,
    option: &DecisionOptionV1,
    ordinal: usize,
    label: &str,
    bound: bool,
    locale: Locale,
) -> Result<(), String> {
    if bound {
        writeln!(
            out,
            "./p {}  |  {}",
            choice_command(decision, &option.choice)?,
            text(locale, "effect", &[("label", label)])
        )
        .unwrap();
    } else {
        writeln!(out, "{ordinal}: {label}").unwrap();
    }
    Ok(())
}

fn render_selections(
    out: &mut String,
    decision: &TaskPublicDecisionV1,
    selections: &[DecisionSelectionV1],
    bound: bool,
    locale: Locale,
) -> Result<(), String> {
    let text = |key, parameters: &[(&str, &str)]| {
        locale
            .text("decision", key, parameters)
            .expect("selection message")
    };
    for (index, selection) in selections.iter().enumerate() {
        let count = if selection.min == selection.max {
            selection.min.to_string()
        } else {
            format!("{}..{}", selection.min, selection.max)
        };
        let heading = if selections.len() == 1 {
            text("selection.heading", &[])
        } else {
            text("selection.numbered", &[("index", &(index + 1).to_string())])
        };
        writeln!(
            out,
            "{}",
            text(
                "selection.prompt",
                &[
                    ("heading", &heading),
                    ("reason", &selection.label),
                    ("count", &count)
                ]
            )
        )
        .unwrap();
        if selection.min != 1 || selection.max != 1 {
            writeln!(
                out,
                "{}",
                text(
                    "selection.input",
                    &[("prefix", if bound { "./p act " } else { "" })]
                )
            )
            .unwrap();
            if bound && selection.max > 1 {
                writeln!(out, "{}", text("selection.distinct", &[])).unwrap();
            }
            if selection.min == 0 {
                writeln!(
                    out,
                    "  {}select:none  |  {}",
                    if bound { "./p act " } else { "" },
                    text("selection.none", &[])
                )
                .unwrap();
            }
        }
        for member in &selection.members {
            let root = selection.choice.ordinal_path[0];
            let token = crate::selector::selection_member_token_v1(decision, root, member.ordinal)?;
            if selection.min == 1 && selection.max == 1 {
                let mut choice = selection.choice.clone();
                choice.ordinal_path.push(member.ordinal);
                let exact = crate::selector::semantic_action_for_choice_v1(decision, &choice)?;
                writeln!(
                    out,
                    "  {}{exact}  |  {}",
                    if bound { "./p act " } else { "" },
                    member.label
                )
                .unwrap();
            } else {
                writeln!(out, "  {token}  |  {}", member.label).unwrap();
            }
        }
    }
    Ok(())
}

fn option_label(
    candidate: &TaskPublicCandidateV1,
    observation: &TaskStrategicObservationV1,
    public: &TaskPublicObservationV1,
    include_preparation_routes: bool,
    locale: Locale,
) -> Result<String, String> {
    Ok(match candidate {
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Proceed { .. },
        } if matches!(observation.context, PlannerDecisionContext::Reward { .. }) => {
            text(locale, "action.leaveRewards", &[])
        }
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ClaimReward {
                    reward: reward @ PlannerRewardDescriptor::Potion { .. },
                    ..
                },
        } if observation
            .relics
            .iter()
            .any(|relic| relic.relic == sts_oracle_learning::content::relics::RelicId::Sozu) =>
        {
            text(
                locale,
                "action.sozuReward",
                &[("reward", &render_reward_descriptor(reward, locale))],
            )
        }
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ClaimReward {
                    reward:
                        reward @ (PlannerRewardDescriptor::Gold { .. }
                        | PlannerRewardDescriptor::StolenGold { .. }),
                    ..
                },
        } if observation.relics.iter().any(|relic| {
            sts_oracle_learning::content::relics::mechanics::relic_prevents_gold_gain(relic.relic)
        }) =>
        {
            text(
                locale,
                "action.ectoplasmReward",
                &[("reward", &render_reward_descriptor(reward, locale))],
            )
        }
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::ClaimReward { reward, .. },
        } => render_reward_descriptor(reward, locale),
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::OpenCardReward { .. },
        } => text(locale, "action.cardReward", &[]),
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Rest,
        } => {
            let relics: Vec<_> = observation
                .relics
                .iter()
                .map(|relic| sts_oracle_learning::content::relics::RelicState::new(relic.relic))
                .collect();
            let run = &observation.run;
            let capacity =
                sts_oracle_learning::engine::campfire_handler::campfire_rest_heal_capacity(
                    run.max_hp, &relics,
                );
            let healed_hp = run.current_hp.saturating_add(capacity).min(run.max_hp);
            text(
                locale,
                "action.rest",
                &[
                    ("before", &run.current_hp.to_string()),
                    ("after", &healed_hp.to_string()),
                    ("healed", &(healed_hp - run.current_hp).to_string()),
                ],
            )
        }
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Smith { card_ordinal, .. },
        } => {
            let card = project_deck_cards_v1(&observation.cards, &[*card_ordinal])?[0];
            text(
                locale,
                "action.upgrade",
                &[("card", &deck_card_label(&card, locale))],
            )
        }
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::SubmitRunSelection {
                    selected_card_ordinals,
                    ..
                },
        } => {
            let cards = project_deck_cards_v1(&observation.cards, selected_card_ordinals)?;
            let upgrading = matches!(
                observation.context,
                PlannerDecisionContext::RunChoice {
                    reason: sts_oracle_learning::state::selection::SelectionReason::Upgrade,
                    ..
                }
            );
            text(
                locale,
                "action.select",
                &[(
                    "cards",
                    &cards
                        .iter()
                        .map(|card| {
                            let label = run_selection_card_label(card, observation, locale);
                            if upgrading {
                                Ok(format!("{label} | {}", smith::changes_for(*card, locale)?))
                            } else {
                                Ok(label)
                            }
                        })
                        .collect::<Result<Vec<_>, String>>()?
                        .join(", "),
                )],
            )
        }
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseEventOption {
                    public_semantics, ..
                },
        } if public_semantics
            .public_label
            .as_ref()
            .is_some_and(|label| !label.trim().is_empty()) =>
        {
            public_semantics
                .public_label
                .clone()
                .expect("nonempty public label")
        }
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::ChooseRouteNode { x, y, flight, .. },
        } => {
            let outlook = if include_preparation_routes {
                crate::public_map_reading::PublicRouteReading::new(
                    &observation.public_map,
                    (*x, *y),
                )
                .map(|route| route.summary())
                .unwrap_or_else(|error| format!("Route analysis error: {error}"))
            } else {
                String::new()
            };
            route_option_label(observation, *x, *y, *flight, &outlook, locale)
        }
        TaskPublicCandidateV1::Strategic { action } => {
            crate::render::render_strategic_action(action, Some(public), locale)
        }
        _ => render_candidate_with_observation(candidate, Some(public)),
    })
}

fn route_option_label(
    observation: &TaskStrategicObservationV1,
    x: i32,
    y: i32,
    flight: bool,
    outlook: &str,
    locale: Locale,
) -> String {
    let room = observation
        .public_map
        .nodes
        .iter()
        .find(|n| n.x == x && n.y == y)
        .map_or_else(
            || text(locale, "action.unknownRoom", &[]),
            |n| room_label(n.room, locale).to_string(),
        );
    text(
        locale,
        "action.go",
        &[
            ("x", &x.to_string()),
            ("y", &y.to_string()),
            ("room", &room),
            (
                "flight",
                &if flight {
                    text(locale, "action.flight", &[])
                } else {
                    String::new()
                },
            ),
            (
                "outlook",
                &if outlook.is_empty() {
                    String::new()
                } else {
                    format!(" | {outlook}")
                },
            ),
        ],
    )
}

fn write_event_resource_modifiers(
    out: &mut String,
    action: &TaskStrategicActionV1,
    observation: &TaskStrategicObservationV1,
    locale: Locale,
) {
    use sts_oracle_learning::ai::planner_core::PlannerEventPublicEffectV1 as Effect;
    let TaskStrategicActionV1::ChooseEventOption {
        public_semantics, ..
    } = action
    else {
        return;
    };
    let Some(semantics) = &public_semantics.structured_semantics else {
        return;
    };
    // These are public effect summaries, including conditional and later
    // effects. Apply a blocker only while its retention is certain.
    let retains = |id| {
        !semantics.effects.iter().any(|effect| {
            matches!(effect, Effect::LoseRelic { specific, starter_only }
                if *specific == Some(id) || (specific.is_none() && !starter_only))
        })
    };
    if observation.relics.iter().any(|relic| {
        sts_oracle_learning::content::relics::mechanics::relic_prevents_gold_gain(relic.relic)
            && retains(relic.relic)
    }) && semantics.effects.iter().any(|effect| {
        matches!(
            effect,
            Effect::GainGold { .. } | Effect::GainGoldRange { .. }
        )
    }) {
        let paid: i32 = semantics
            .effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::LoseGold { amount } => Some(*amount),
                _ => None,
            })
            .sum::<i32>()
            .min(observation.run.gold)
            .max(0);
        writeln!(
            out,
            "  {}",
            text(
                locale,
                "mechanics.ectoplasm",
                &[("paid", &paid.to_string())]
            )
        )
        .unwrap();
    }
    if observation.relics.iter().any(|relic| {
        relic.relic == sts_oracle_learning::content::relics::RelicId::Sozu && retains(relic.relic)
    }) && semantics
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::ObtainRandomPotions { count } if *count > 0))
    {
        writeln!(out, "  {}", text(locale, "mechanics.sozu", &[])).unwrap();
    }
}

/// The choice's typed identity supplies its category and rules. Do not make
/// players infer a potion/relic distinction from a shared `buy` label.
fn write_choice_mechanics(
    out: &mut String,
    action: &TaskStrategicActionV1,
    brief: bool,
    locale: Locale,
) {
    use sts_oracle_learning::content::relics;
    if let TaskStrategicActionV1::TakeCard { card, upgrades, .. }
    | TaskStrategicActionV1::BuyCard { card, upgrades, .. } = action
    {
        if brief {
            crate::mechanics_info::write_card_offer(out, *card, *upgrades, locale);
        } else {
            crate::mechanics_info::write_card_rule(out, *card, *upgrades, locale);
        }
        return;
    }
    let key_query = match action {
        TaskStrategicActionV1::Recall => Some("RubyKey"),
        TaskStrategicActionV1::ClaimReward {
            reward: PlannerRewardDescriptor::SapphireKey,
            ..
        } => Some("SapphireKey"),
        TaskStrategicActionV1::ClaimReward {
            reward: PlannerRewardDescriptor::EmeraldKey,
            ..
        } => Some("EmeraldKey"),
        _ => None,
    };
    if let Some(query) = key_query {
        if brief {
            if query == "RubyKey" {
                writeln!(out, "  {}", text(locale, "keys.recall", &[])).unwrap();
            } else if query == "EmeraldKey" {
                writeln!(out, "  {}", text(locale, "keys.eliteRewards", &[])).unwrap();
            }
            return;
        }
        let mechanics = crate::mechanics_info::render(None, query, locale)
            .expect("key choices have shared mechanics definitions");
        for line in mechanics.lines() {
            writeln!(out, "  {line}").unwrap();
        }
        return;
    }
    if let TaskStrategicActionV1::ChooseEventOption {
        public_semantics, ..
    } = action
    {
        if let Some(semantics) = &public_semantics.structured_semantics {
            writeln!(
                out,
                "  {}",
                text(
                    locale,
                    "mechanics.eventEffectsEnglish",
                    &[(
                        "effects",
                        &crate::render::render_event_structured_semantics_v1(Some(semantics))
                    )]
                )
            )
            .unwrap();
        }
        let mut resources = Vec::new();
        for (label, delta) in [
            (text(locale, "mechanics.hp", &[]), public_semantics.hp_delta),
            (
                text(locale, "mechanics.maxHp", &[]),
                public_semantics.max_hp_delta,
            ),
        ] {
            if delta != 0 {
                resources.push(format!("{label} {delta:+}"));
            }
        }
        let (min, max) = (
            public_semantics.gold_delta_min,
            public_semantics.gold_delta_max,
        );
        if min != 0 || max != 0 {
            resources.push(if min == max {
                text(
                    locale,
                    "mechanics.goldDelta",
                    &[("amount", &format!("{min:+}"))],
                )
            } else {
                text(
                    locale,
                    "mechanics.goldDelta",
                    &[("amount", &format!("{min:+}..{max:+}"))],
                )
            });
        }
        if !resources.is_empty() {
            writeln!(
                out,
                "  {}",
                text(
                    locale,
                    "mechanics.resources",
                    &[("resources", &resources.join("; "))]
                )
            )
            .unwrap();
        }
        // The event owner already identifies these items. Expand their shared
        // rules at the choice instead of requiring a name-based follow-up guess.
        use sts_oracle_learning::ai::planner_core::{
            PlannerEventCardPoolV1 as CardPool, PlannerEventPublicEffectV1 as Effect,
            PlannerEventRelicPoolV1 as RelicPool,
        };
        let mut cards = public_semantics
            .specific_cards_gained
            .iter()
            .map(|card| (card.card, card.upgrades))
            .collect::<Vec<_>>();
        let mut relics = Vec::new();
        if let Some(semantics) = &public_semantics.structured_semantics {
            for effect in &semantics.effects {
                match effect {
                    Effect::ObtainCard { count, pool }
                    | Effect::ObtainColorlessCard { count, pool }
                    | Effect::OfferCards { count, pool }
                    | Effect::ObtainCurse { count, pool }
                    | Effect::RemoveCard { count, pool, .. }
                        if *count > 0 =>
                    {
                        if let CardPool::Specific { card, upgrades } = pool {
                            cards.push((*card, *upgrades));
                        }
                    }
                    Effect::ObtainRelic {
                        count,
                        pool: RelicPool::Specific { relic },
                    } if *count > 0 => relics.push(*relic),
                    Effect::LoseRelic {
                        specific: Some(relic),
                        ..
                    }
                    | Effect::LoseStarterRelic {
                        specific: Some(relic),
                    } => relics.push(*relic),
                    _ => {}
                }
            }
        }
        let mut seen_cards = std::collections::HashSet::new();
        for (card, upgrades) in cards {
            if seen_cards.insert((card, upgrades)) {
                if brief {
                    crate::mechanics_info::write_card_offer(out, card, upgrades, locale);
                } else {
                    crate::mechanics_info::write_card_rule(out, card, upgrades, locale);
                }
            }
        }
        let mut seen_relics = std::collections::HashSet::new();
        for relic in relics {
            if seen_relics.insert(relic) {
                writeln!(
                    out,
                    "  {}", text(locale, "mechanics.eventRelicEnglish", &[
                        ("name", locale.entity("relic", relic.stable_id())),
                        ("effect", relics::mechanics::explanation(relic).unwrap_or("Effect explanation unavailable; query info relic or report the gap.")),
                    ])
                )
                .unwrap();
            }
        }
        return;
    }
    let (category, explanation) = match action {
        TaskStrategicActionV1::TakeBossRelic { relic, .. }
        | TaskStrategicActionV1::BuyRelic { relic, .. }
        | TaskStrategicActionV1::ClaimReward {
            reward: PlannerRewardDescriptor::Relic { relic },
            ..
        } => (
            "mechanics.relicEnglish",
            relics::mechanics::explanation(*relic),
        ),
        TaskStrategicActionV1::BuyPotion { potion, .. }
        | TaskStrategicActionV1::ClaimReward {
            reward: PlannerRewardDescriptor::Potion { potion },
            ..
        } => (
            "mechanics.potion",
            locale.entry("game", &format!("potionRule.{}", potion.stable_id())),
        ),
        _ => return,
    };
    writeln!(
        out,
        "  {}",
        text(
            locale,
            category,
            &[(
                "effect",
                explanation.unwrap_or_else(|| locale
                    .entry("inventory", "unknown")
                    .expect("missing rule message"))
            )]
        )
    )
    .unwrap();
}

fn shop_offers(
    inventory: &PlannerShopInventoryV1,
    locale: Locale,
) -> Vec<(
    TaskStrategicActionV1,
    String,
    i32,
    &PlannerShopPurchaseStatusV1,
)> {
    inventory
        .cards
        .iter()
        .map(|x| {
            (
                TaskStrategicActionV1::BuyCard {
                    shop_slot: x.slot,
                    card: x.card,
                    upgrades: x.upgrades,
                    price: x.price,
                },
                locale.card_name(x.card.stable_id(), x.upgrades),
                x.price,
                &x.purchase_status,
            )
        })
        .chain(inventory.relics.iter().map(|x| {
            (
                TaskStrategicActionV1::BuyRelic {
                    shop_slot: x.slot,
                    relic: x.relic,
                    price: x.price,
                },
                locale.entity("relic", x.relic.stable_id()).to_string(),
                x.price,
                &x.purchase_status,
            )
        }))
        .chain(inventory.potions.iter().map(|x| {
            (
                TaskStrategicActionV1::BuyPotion {
                    shop_slot: x.slot,
                    potion: x.potion,
                    price: x.price,
                },
                locale.entity("potion", x.potion.stable_id()).to_string(),
                x.price,
                &x.purchase_status,
            )
        }))
        .collect()
}

fn render_shop(
    out: &mut String,
    decision: &TaskPublicDecisionV1,
    inventory: &PlannerShopInventoryV1,
    options: &[DecisionOptionV1],
    brief: bool,
    bound: bool,
    locale: Locale,
) -> Result<BTreeSet<usize>, String> {
    let offers = shop_offers(inventory, locale);
    let purchasable = offers
        .iter()
        .filter(|(_, _, _, status)| matches!(status, PlannerShopPurchaseStatusV1::Purchasable))
        .map(|(_, name, price, _)| format!("{name} ({price}g)"))
        .collect::<Vec<_>>();
    if !brief {
        writeln!(
            out,
            "{}",
            text(
                locale,
                "shop.purchasable",
                &[(
                    "offers",
                    &if purchasable.is_empty() {
                        text(locale, "none", &[])
                    } else {
                        purchasable.join(", ")
                    }
                )]
            )
        )
        .unwrap();
    }
    if let Some(purge) = &inventory.purge {
        writeln!(
            out,
            "{}",
            text(
                locale,
                "shop.removal",
                &[
                    ("price", &purge.price.to_string()),
                    ("status", &purchase_suffix(&purge.purchase_status, locale))
                ]
            ),
        )
        .unwrap();
    }
    let mut shown = BTreeSet::new();
    for (offer, name, price, status) in offers {
        // Match the complete typed offer, including its category and current slot.
        // Only the original supplied option can be returned as an executable choice.
        let matches: Vec<_> = options.iter().enumerate().filter(|(_, option)| {
            matches!(&option.candidate, TaskPublicCandidateV1::Strategic { action } if *action == offer)
        }).collect();
        match (status, matches.as_slice()) {
            (PlannerShopPurchaseStatusV1::Purchasable, [(ordinal, option)]) => {
                if !shown.insert(*ordinal) {
                    return Err("Duplicate shop offer in public inventory".into());
                }
                render_option_row(
                    out,
                    decision,
                    option,
                    *ordinal,
                    &option.label,
                    bound,
                    locale,
                )?;
            }
            (PlannerShopPurchaseStatusV1::Blocked { .. }, []) => {
                writeln!(
                    out,
                    "  {}",
                    text(
                        locale,
                        "shop.price",
                        &[
                            ("name", &name),
                            ("price", &price.to_string()),
                            ("status", &purchase_suffix(status, locale))
                        ]
                    )
                )
                .unwrap();
            }
            _ => return Err("Shop inventory and supplied purchase choices disagree".into()),
        }
        if !brief
            || matches!(status, PlannerShopPurchaseStatusV1::Purchasable)
            || matches!(
                status,
                PlannerShopPurchaseStatusV1::Blocked {
                    reason: PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot
                }
            )
        {
            write_choice_mechanics(out, &offer, brief, locale);
        }
    }
    if inventory.potions.iter().any(|potion| {
        matches!(
            potion.purchase_status,
            PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot
            }
        )
    }) {
        writeln!(out, "{}", text(locale, "shop.replacement", &[])).unwrap();
    }
    Ok(shown)
}

pub(crate) fn purchase_suffix(status: &PlannerShopPurchaseStatusV1, locale: Locale) -> String {
    let PlannerShopPurchaseStatusV1::Blocked { reason } = status else {
        return String::new();
    };
    let reason = match reason {
        PlannerShopPurchaseBlockReasonV1::MerchantUnavailable { .. } => "unavailable",
        PlannerShopPurchaseBlockReasonV1::NotEnoughGold { .. } => "gold",
        PlannerShopPurchaseBlockReasonV1::BlockedBySozu => "sozu",
        PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot => "slots",
        PlannerShopPurchaseBlockReasonV1::PurgeAlreadyUsed => "usedRemoval",
        PlannerShopPurchaseBlockReasonV1::NoEligiblePurgeTarget => "noRemoval",
    };
    text(
        locale,
        "shop.blocked",
        &[("reason", &text(locale, &format!("shop.{reason}"), &[]))],
    )
}

fn room_label(room: PlannerMapRoom, locale: Locale) -> &'static str {
    let key = match room {
        PlannerMapRoom::Monster => "monster",
        PlannerMapRoom::QuestionMark => "questionMark",
        PlannerMapRoom::Elite => "elite",
        PlannerMapRoom::Rest => "rest",
        PlannerMapRoom::Shop => "shop",
        PlannerMapRoom::Treasure => "treasure",
        PlannerMapRoom::Boss => "boss",
        PlannerMapRoom::Victory => "victory",
    };
    locale
        .entry("decision", &format!("room.{key}"))
        .expect("room label")
}
