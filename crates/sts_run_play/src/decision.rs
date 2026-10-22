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
use crate::render::{format_card, render_candidate_with_observation, render_reward_descriptor};
use crate::run_selection::{project_deck_cards_v1, project_run_selection_family_v1};

pub(crate) mod combat;
pub(crate) mod preparation;
mod smith;

#[cfg(test)]
#[path = "decision/presentation_tests.rs"]
mod presentation_tests;

pub(crate) fn run_transition_label_v1(
    stage: sts_oracle_learning::ai::planner_core::PlannerRunTransitionV1,
) -> &'static str {
    use sts_oracle_learning::ai::planner_core::PlannerRunTransitionV1;
    match stage {
        PlannerRunTransitionV1::SecondBoss => "Continue to second boss",
        PlannerRunTransitionV1::FinalAct => "Continue to Act 4",
        PlannerRunTransitionV1::VictoryScreen => "Continue to victory screen",
    }
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
        let summary = format!("Terminal | {outcome:?}\n");
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
            result.summary = combat::summary(run_context, observation);
            result.view = combat::reading(
                run_context,
                observation,
                details.contains(&DecisionDetailV1::Brief),
            );
            combat::pending_choices(
                &decision,
                &mut result,
                details.contains(&DecisionDetailV1::Brief),
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
                label: run_selection_label(family.reason)?.into(),
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
                                    run_selection_card_label(card, observation),
                                    smith::changes_for(*card)?
                                )
                            } else {
                                run_selection_card_label(card, observation)
                            },
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            });
            continue;
        }
        options.push(DecisionOptionV1 {
            choice: TaskPublicChoiceV1::one(ordinal),
            label: option_label(candidate, observation, &decision.observation, true)?,
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
                "Map overlay · choose next room, or cancel to return to the room screen".to_string()
            } else {
                "Map · choose next room".to_string()
            };
            let map = &observation.public_map;
            if let Some(node) = map
                .nodes
                .iter()
                .find(|node| node.x == map.current_x && node.y == map.current_y)
            {
                write!(
                    label,
                    " · {} node ({},{}) {}",
                    if *overlay { "current" } else { "completed" },
                    node.x,
                    node.y,
                    room_label(node.room)
                )
                .unwrap();
            }
            label
        }
        PlannerDecisionContext::CardReward { .. } => "Card reward".into(),
        PlannerDecisionContext::Shop { .. } => "Shop".into(),
        PlannerDecisionContext::Reward { pending_rewards } if pending_rewards.is_empty() => {
            "Rewards complete".into()
        }
        PlannerDecisionContext::Reward { pending_rewards } => {
            format!("Rewards ({} remaining)", pending_rewards.len())
        }
        PlannerDecisionContext::Campfire => "Campfire".into(),
        PlannerDecisionContext::BossRelic => "Boss relic".into(),
        PlannerDecisionContext::RunChoice { reason, source, .. } => format!(
            "Card selection · {reason:?}{}",
            source.map_or_else(String::new, |source| format!(" · source: {source:?}"))
        ),
        PlannerDecisionContext::Treasure => "Treasure".into(),
        PlannerDecisionContext::RunTransition { stage } => run_transition_label_v1(*stage).into(),
        PlannerDecisionContext::Event { event, screen, .. } => format!(
            "{} | Event screen {screen}",
            gameplay_display_label_v1(*event)
        ),
    };
    result.summary = format!(
        "Act {} F{} | HP {}/{} | Gold {} | {context}\n",
        run.act, run.floor, run.current_hp, run.max_hp, run.gold
    );
    if brief {
        out.push_str(&result.summary);
        if !show_build_resources {
            writeln!(out, "Deck: {} cards", observation.cards.len()).unwrap();
        }
    } else {
        writeln!(
            out,
            "Act {} F{} | A{} | HP {}/{} | Gold {} | Base energy/turn {} | {context}",
            run.act,
            run.floor,
            run.ascension_level,
            run.current_hp,
            run.max_hp,
            run.gold,
            crate::agent_protocol::public_base_energy_per_turn_v1(&observation.relics)
        )
        .unwrap();
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
        let missing = ["Ruby", "Sapphire", "Emerald"]
            .into_iter()
            .zip(run.keys)
            .filter_map(|(name, held)| (!held).then_some(name))
            .collect::<Vec<_>>();
        if key_choice && !missing.is_empty() {
            writeln!(out, "Heart keys still needed: {}", missing.join(", ")).unwrap();
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
                writeln!(
                    out,
                    "Without all three keys, these Act 3 bosses end the run before Act 4."
                )
                .unwrap();
            }
        }
    }
    if let Some(relic) = observation.relics.iter().filter(|_| !brief).find(|relic| {
        sts_oracle_learning::content::relics::mechanics::relic_prevents_gold_gain(relic.relic)
    }) {
        writeln!(
            out,
            "Gold gains: prevented by {}. Held Gold remains spendable; Gold costs still apply.",
            gameplay_display_label_v1(relic.relic)
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
            writeln!(out, "Scene: {scene}").unwrap();
        }
    }
    if (repeat_deck && !brief) || show_build_resources {
        let mut deck = BTreeMap::<String, usize>::new();
        for card in &observation.cards {
            *deck.entry(deck_card_label(card)).or_default() += 1;
        }
        writeln!(
            out,
            "Deck: {} cards | {}",
            observation.cards.len(),
            deck.into_iter()
                .map(|(card, count)| {
                    if count == 1 {
                        card
                    } else {
                        format!("{count}x {card}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    }
    if (repeat_relics && !brief) || show_build_resources {
        writeln!(
            out,
            "Relics: {}",
            observation
                .relics
                .iter()
                .map(|r| crate::compact::relic_label_v1(r.relic, r.counter, r.used_up, Locale::En))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    }
    if repeat_potions || brief {
        writeln!(
            out,
            "Potions: {}",
            observation
                .potions
                .iter()
                .map(|slot| format!(
                    "P{} {}",
                    slot.slot,
                    slot.potion.map_or_else(
                        || "empty".to_owned(),
                        |p| gameplay_display_label_v1(p.potion).to_string()
                    )
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    }
    if potion_choice {
        writeln!(out, "Potion choice: {potion_rewards} rewards, {free_potion_slots} empty slots. Choose rewards to claim or use a held potion/discard command below to free a slot.").unwrap();
    }
    if potion_rewards > 0
        && observation
            .relics
            .iter()
            .any(|relic| relic.relic == sts_oracle_learning::content::relics::RelicId::Sozu)
    {
        writeln!(out, "Sozu prevents obtaining potions. Claiming a potion reward clears it and keeps the held inventory unchanged.").unwrap();
    }
    let reused: Vec<_> = [
        ("Deck", repeat_deck),
        ("Relics", repeat_relics),
        ("Potions", repeat_potions),
    ]
    .into_iter()
    .filter_map(|(name, repeated)| (!repeated).then_some(name))
    .collect();
    if !reused.is_empty() && !brief {
        writeln!(out, "Unchanged: {}", reused.join(", ")).unwrap();
    }
    if !brief {
        crate::mechanics_info::write_key_status(out, run.keys, heart_goal);
    }
    let bound = result.source.is_some();
    let shop_choices = if let PlannerDecisionContext::Shop { inventory, .. } = &observation.context
    {
        render_shop(out, decision, inventory, &result.options, brief, bound)?
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
                    "Unavailable reward: {}",
                    render_reward_descriptor(reward)
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
                        writeln!(out, "  Sozu prevents obtaining potions.").unwrap();
                    } else if observation.potions.iter().all(|slot| slot.potion.is_some()) {
                        writeln!(out, "  Potion slots full. Choose a use/discard command below, then claim this reward from the updated reading.").unwrap();
                    }
                }
                write_choice_mechanics(
                    out,
                    &TaskStrategicActionV1::ClaimReward {
                        reward_item_index: index,
                        reward: reward.clone(),
                    },
                    brief,
                );
            }
        }
    }
    render_selections(out, decision, &result.selections, bound)?;
    let upgrades = smith::comparisons(observation, &result.options)?;
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
        writeln!(out, "Next rooms: {}", route_choices.len()).unwrap();
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
                    format!(" (choose 1 of {} copies)", upgrade.ordinals.len())
                } else {
                    String::new()
                };
                format!("{}{} | {}", option.label, count, upgrade.changes)
            } else if let TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ChooseRouteNode { x, y, flight, .. },
            } = &option.candidate
            {
                route_option_label(observation, *x, *y, *flight, "")
            } else {
                option_label(&option.candidate, observation, &decision.observation, false)?
            }
        } else {
            option.label.clone()
        };
        render_option_row(out, decision, option, ordinal, &label, bound)?;
        if let TaskPublicCandidateV1::Strategic { action } = &option.candidate {
            write_choice_mechanics(out, action, brief);
            write_event_resource_modifiers(out, action, observation);
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
                crate::mechanics_info::write_card_rule(out, card.card, card.upgrades);
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
        smith::write_full(out, &upgrades);
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
        crate::mechanics_info::write_preparation_window(out, observation);
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
                room_label(node.room),
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

pub(crate) fn deck_card_label(card: &TaskStrategicCardV1) -> String {
    let mut label = format_card(card.card, card.upgrades);
    if card.misc_value != 0 {
        write!(label, " (misc {})", card.misc_value).unwrap();
    }
    if card.cost_modifier != 0 {
        write!(label, " (cost {:+})", card.cost_modifier).unwrap();
    }
    if let Some(damage) = card.base_damage_override {
        write!(label, " (damage {damage})").unwrap();
    }
    if let Some(block) = card.base_block_override {
        write!(label, " (block {block})").unwrap();
    }
    if let Some(relic) = card.bottled_by {
        write!(label, " [{}]", gameplay_display_label_v1(relic)).unwrap();
    }
    label
}

fn run_selection_card_label(
    card: &TaskStrategicCardV1,
    observation: &TaskStrategicObservationV1,
) -> String {
    let reward = crate::render::run_selection_reward_label_v1(observation, card.card)
        .map_or_else(String::new, |label| format!(" | {label}"));
    format!("{}{reward}", deck_card_label(card))
}

fn run_selection_label(
    reason: sts_oracle_learning::state::selection::SelectionReason,
) -> Result<&'static str, String> {
    use sts_oracle_learning::state::selection::SelectionReason::*;
    match reason {
        Upgrade => Ok("Upgrade from deck"),
        Purge => Ok("Remove from deck"),
        Transform => Ok("Transform from deck"),
        TransformUpgraded => Ok("Transform and upgrade"),
        Duplicate => Ok("Duplicate from deck"),
        BottleFlame => Ok("Bottle an attack"),
        BottleLightning => Ok("Bottle a skill"),
        BottleTornado => Ok("Bottle a power"),
        _ => Err(format!(
            "Unexpected master-deck selection reason: {reason:?}"
        )),
    }
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
) -> Result<(), String> {
    if bound {
        writeln!(
            out,
            "./p {}  |  Effect: {label}",
            choice_command(decision, &option.choice)?
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
) -> Result<(), String> {
    for (index, selection) in selections.iter().enumerate() {
        let count = if selection.min == selection.max {
            selection.min.to_string()
        } else {
            format!("{}..{}", selection.min, selection.max)
        };
        let heading = if selections.len() == 1 {
            "Selection".to_string()
        } else {
            format!("Selection {}", index + 1)
        };
        writeln!(out, "{heading} | {} | Choose {count}", selection.label).unwrap();
        if selection.min != 1 || selection.max != 1 {
            writeln!(
                out,
                "  Input: {}select:<card tokens joined by commas without spaces>",
                if bound { "./p act " } else { "" }
            )
            .unwrap();
            if bound && selection.max > 1 {
                writeln!(out, "  Select distinct listed card tokens in this one selection; #0 and #1 identify different copies, not the same #0 twice.").unwrap();
            }
            if selection.min == 0 {
                writeln!(
                    out,
                    "  {}select:none  |  choose none",
                    if bound { "./p act " } else { "" }
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
) -> Result<String, String> {
    Ok(match candidate {
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Proceed { .. },
        } if matches!(observation.context, PlannerDecisionContext::Reward { .. }) => {
            "leave rewards".into()
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
            format!(
                "{} (Sozu: clear reward; potion gain prevented)",
                render_reward_descriptor(reward)
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
            format!(
                "{} (Gold gain prevented by Ectoplasm)",
                render_reward_descriptor(reward)
            )
        }
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::ClaimReward { reward, .. },
        } => render_reward_descriptor(reward),
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::OpenCardReward { .. },
        } => "card reward".into(),
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
            format!(
                "Rest | HP {} -> {} (+{})",
                run.current_hp,
                healed_hp,
                healed_hp - run.current_hp
            )
        }
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Smith { card_ordinal, .. },
        } => {
            let card = project_deck_cards_v1(&observation.cards, &[*card_ordinal])?[0];
            format!("Upgrade one {}", deck_card_label(&card))
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
            format!(
                "Select {}",
                cards
                    .iter()
                    .map(|card| {
                        let label = run_selection_card_label(card, observation);
                        if upgrading {
                            Ok(format!("{label} | {}", smith::changes_for(*card)?))
                        } else {
                            Ok(label)
                        }
                    })
                    .collect::<Result<Vec<_>, String>>()?
                    .join(", ")
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
            route_option_label(observation, *x, *y, *flight, &outlook)
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
) -> String {
    let room = observation
        .public_map
        .nodes
        .iter()
        .find(|n| n.x == x && n.y == y)
        .map_or("Unknown", |n| room_label(n.room));
    format!(
        "Go ({x},{y}) {room}{}{}",
        if flight { " using Wing Boots" } else { "" },
        if outlook.is_empty() {
            String::new()
        } else {
            format!(" · {outlook}")
        }
    )
}

fn write_event_resource_modifiers(
    out: &mut String,
    action: &TaskStrategicActionV1,
    observation: &TaskStrategicObservationV1,
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
        writeln!(out, "  With Ectoplasm: Gold received 0; Gold paid {paid}.").unwrap();
    }
    if observation.relics.iter().any(|relic| {
        relic.relic == sts_oracle_learning::content::relics::RelicId::Sozu && retains(relic.relic)
    }) && semantics
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::ObtainRandomPotions { count } if *count > 0))
    {
        writeln!(out, "  With Sozu: potions obtained 0.").unwrap();
    }
}

/// The choice's typed identity supplies its category and rules. Do not make
/// players infer a potion/relic distinction from a shared `buy` label.
fn write_choice_mechanics(out: &mut String, action: &TaskStrategicActionV1, brief: bool) {
    use sts_oracle_learning::content::{cards, potions, relics};
    if brief {
        if let TaskStrategicActionV1::TakeCard { card, upgrades, .. }
        | TaskStrategicActionV1::BuyCard { card, upgrades, .. } = action
        {
            crate::mechanics_info::write_card_offer(out, *card, *upgrades);
            return;
        }
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
                writeln!(
                    out,
                    "  Gain the Ruby Key instead of another campfire action."
                )
                .unwrap();
            } else if query == "EmeraldKey" {
                writeln!(
                    out,
                    "  {}",
                    crate::mechanics_info::EMERALD_KEY_REWARD_RELATIONSHIP
                )
                .unwrap();
            }
            return;
        }
        let mechanics = crate::mechanics_info::render(None, query, Locale::En)
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
                "  Effects: {}",
                crate::render::render_event_structured_semantics_v1(Some(semantics))
            )
            .unwrap();
        }
        let mut resources = Vec::new();
        for (label, delta) in [
            ("HP", public_semantics.hp_delta),
            ("Max HP", public_semantics.max_hp_delta),
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
                format!("Gold {min:+}")
            } else {
                format!("Gold {min:+}..{max:+}")
            });
        }
        if !resources.is_empty() {
            writeln!(
                out,
                "  Resources (base effect summary): {}",
                resources.join("; ")
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
                    crate::mechanics_info::write_card_offer(out, card, upgrades);
                } else {
                    crate::mechanics_info::write_card_rule(out, card, upgrades);
                }
            }
        }
        let mut seen_relics = std::collections::HashSet::new();
        for relic in relics {
            if seen_relics.insert(relic) {
                writeln!(
                    out,
                    "  Relic {}: {}",
                    gameplay_display_label_v1(relic),
                    relics::mechanics::explanation(relic).unwrap_or(
                        "Effect explanation unavailable; query info relic or report the gap."
                    )
                )
                .unwrap();
            }
        }
        return;
    }
    let (category, explanation) = match action {
        TaskStrategicActionV1::TakeCard { card, upgrades, .. }
        | TaskStrategicActionV1::BuyCard { card, upgrades, .. } => {
            writeln!(
                out,
                "  Card: {}",
                crate::mechanics_info::card_attributes(*card, *upgrades, 0, Locale::En)
            )
            .unwrap();
            let mut blueprint = sts_oracle_learning::runtime::combat::CardBlueprint::fresh(*card);
            blueprint.upgrades = *upgrades;
            let numbers = cards::mechanics::card_blueprint_persistent_base_numbers_v1(&blueprint);
            let mut stats = Vec::new();
            if numbers.damage > 0 {
                stats.push(format!("damage {}", numbers.damage));
            }
            if numbers.block > 0 {
                stats.push(format!("Block {}", numbers.block));
            }
            if !stats.is_empty() {
                writeln!(
                    out,
                    "  Printed base stats at this upgrade: {}",
                    stats.join("; ")
                )
                .unwrap();
            }
            (
                "Printed rule (base/upgraded)",
                cards::mechanics::explanation(*card),
            )
        }
        TaskStrategicActionV1::TakeBossRelic { relic, .. }
        | TaskStrategicActionV1::BuyRelic { relic, .. }
        | TaskStrategicActionV1::ClaimReward {
            reward: PlannerRewardDescriptor::Relic { relic },
            ..
        } => ("Relic", relics::mechanics::explanation(*relic)),
        TaskStrategicActionV1::BuyPotion { potion, .. }
        | TaskStrategicActionV1::ClaimReward {
            reward: PlannerRewardDescriptor::Potion { potion },
            ..
        } => (
            "Potion (base rule)",
            potions::mechanics::explanation(*potion),
        ),
        _ => return,
    };
    writeln!(out, "  {category}: {}", explanation.unwrap_or("Effect explanation unavailable; query info for this category or report the gap. This does not mean unsupported gameplay.")).unwrap();
    if let TaskStrategicActionV1::TakeCard { card, upgrades, .. }
    | TaskStrategicActionV1::BuyCard { card, upgrades, .. } = action
    {
        crate::mechanics_info::write_card_fixed_requests(out, *card, *upgrades, Locale::En);
    }
}

fn shop_offers(
    inventory: &PlannerShopInventoryV1,
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
                format_card(x.card, x.upgrades),
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
                gameplay_display_label_v1(x.relic).to_string(),
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
                gameplay_display_label_v1(x.potion).to_string(),
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
) -> Result<BTreeSet<usize>, String> {
    let offers = shop_offers(inventory);
    let purchasable = offers
        .iter()
        .filter(|(_, _, _, status)| matches!(status, PlannerShopPurchaseStatusV1::Purchasable))
        .map(|(_, name, price, _)| format!("{name} ({price}g)"))
        .collect::<Vec<_>>();
    if !brief {
        writeln!(
            out,
            "Purchasable now: {}",
            if purchasable.is_empty() {
                "none".into()
            } else {
                purchasable.join(", ")
            }
        )
        .unwrap();
    }
    if let Some(purge) = &inventory.purge {
        writeln!(
            out,
            "Removal: {}g{}",
            purge.price,
            purchase_suffix(&purge.purchase_status),
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
                render_option_row(out, decision, option, *ordinal, &option.label, bound)?;
            }
            (PlannerShopPurchaseStatusV1::Blocked { .. }, []) => {
                writeln!(out, "  {name} {price}g{}", purchase_suffix(status)).unwrap();
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
            write_choice_mechanics(out, &offer, brief);
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
        writeln!(out, "Potion replacement: use options to inspect held-potion discard commands. Discard the bottle you choose to replace, then read the updated offers and buy the replacement at its current price.").unwrap();
    }
    Ok(shown)
}

pub(crate) fn purchase_suffix(status: &PlannerShopPurchaseStatusV1) -> String {
    let PlannerShopPurchaseStatusV1::Blocked { reason } = status else {
        return String::new();
    };
    let reason = match reason {
        PlannerShopPurchaseBlockReasonV1::MerchantUnavailable { .. } => "unavailable",
        PlannerShopPurchaseBlockReasonV1::NotEnoughGold { .. } => "insufficient gold",
        PlannerShopPurchaseBlockReasonV1::BlockedBySozu => "Sozu",
        PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot => "slots full",
        PlannerShopPurchaseBlockReasonV1::PurgeAlreadyUsed => "already removed a card",
        PlannerShopPurchaseBlockReasonV1::NoEligiblePurgeTarget => "no removable card",
    };
    format!(" ({reason})")
}

fn room_label(room: PlannerMapRoom) -> &'static str {
    match room {
        PlannerMapRoom::Monster => "Monster",
        PlannerMapRoom::QuestionMark => "?",
        PlannerMapRoom::Elite => "Elite",
        PlannerMapRoom::Rest => "Campfire",
        PlannerMapRoom::Shop => "Shop",
        PlannerMapRoom::Treasure => "Treasure",
        PlannerMapRoom::Boss => "Boss",
        PlannerMapRoom::Victory => "Victory",
    }
}
