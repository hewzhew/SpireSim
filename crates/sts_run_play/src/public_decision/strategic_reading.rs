//! Display projections of the already validated public decision. No scoring,
//! state reconstruction, action resolution, or hidden-state access lives here.

use std::fmt::Write;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1, TaskPublicChoiceV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskStrategicActionV1,
};
use sts_oracle_learning::ai::planner_core::PlannerDecisionContext;
use sts_presentation::Locale;

fn text(locale: Locale, key: &str, values: &[(&str, &str)]) -> String {
    locale
        .text("publicDecision", key, values)
        .expect("public decision message")
}

pub(super) fn project(decision: &TaskPublicDecisionV1, locale: Locale) -> Result<Value, String> {
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err("strategic reading requires a strategic observation".into());
    };
    // Include every published candidate. A detached view does not inherit the
    // live reading's compact admission or grant execution authority.
    let options = decision
        .candidates
        .iter()
        .enumerate()
        .map(|(ordinal, candidate)| {
            if !matches!(candidate, TaskPublicCandidateV1::Strategic { .. }) {
                return Err("strategic reading contains a non-strategic candidate".to_owned());
            }
            Ok(crate::decision::DecisionOptionV1 {
                choice: TaskPublicChoiceV1::one(ordinal),
                label: crate::decision::option_label(
                    candidate,
                    observation,
                    &decision.observation,
                    false,
                    locale,
                )?,
                candidate: candidate.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let context = |key, values: &[(&str, &str)]| {
        locale
            .text("decision", key, values)
            .expect("decision context message")
    };
    let mut context_details = String::new();
    let summary = match &observation.context {
        PlannerDecisionContext::Map { overlay } => context(
            if *overlay {
                "context.mapOverlay"
            } else {
                "context.map"
            },
            &[],
        ),
        PlannerDecisionContext::CardReward {
            pending_rewards, ..
        }
        | PlannerDecisionContext::Reward { pending_rewards } => {
            for (index, reward) in pending_rewards.iter().enumerate() {
                writeln!(
                    context_details,
                    "{}",
                    text(
                        locale,
                        "rewardItem",
                        &[
                            ("index", &index.to_string()),
                            (
                                "reward",
                                &crate::render::render_reward_descriptor(reward, locale)
                            ),
                        ]
                    )
                )
                .expect("string write");
            }
            match &observation.context {
                PlannerDecisionContext::CardReward { .. } => context("context.cardReward", &[]),
                _ if pending_rewards.is_empty() => context("context.rewardsComplete", &[]),
                _ => context(
                    "context.rewards",
                    &[("count", &pending_rewards.len().to_string())],
                ),
            }
        }
        PlannerDecisionContext::Event { event, screen, .. } => context(
            "context.event",
            &[
                ("event", locale.entity("event", event.stable_id())),
                ("screen", &screen.to_string()),
            ],
        ),
        PlannerDecisionContext::Shop { inventory, .. } => {
            crate::decision::render_shop(
                &mut context_details,
                decision,
                inventory,
                &options,
                true,
                false,
                locale,
            )?;
            context("context.shop", &[])
        }
        PlannerDecisionContext::Campfire => context("context.campfire", &[]),
        PlannerDecisionContext::BossRelic => context("context.bossRelic", &[]),
        PlannerDecisionContext::RunChoice {
            reason,
            source,
            min_choices,
            max_choices,
            ..
        } => {
            writeln!(
                context_details,
                "{}",
                text(
                    locale,
                    "selectionBounds",
                    &[
                        ("min", &min_choices.to_string()),
                        ("max", &max_choices.to_string())
                    ]
                )
            )
            .expect("string write");
            context(
                "context.cardSelection",
                &[
                    (
                        "reason",
                        &crate::decision::run_selection_label(*reason, locale)
                            .map(str::to_owned)
                            .unwrap_or_else(|_| format!("{reason:?}")),
                    ),
                    (
                        "source",
                        &source.map_or_else(String::new, |source| {
                            context(
                                "context.selectionSource",
                                &[("source", &format!("{source:?}"))],
                            )
                        }),
                    ),
                ],
            )
        }
        PlannerDecisionContext::Treasure => context("context.treasure", &[]),
        PlannerDecisionContext::RunTransition { stage } => {
            crate::decision::run_transition_label_v1(*stage, locale).into()
        }
    };
    let candidates = options.iter()
        .zip(&decision.information.candidate_surface.ordered_candidate_ids)
        .map(|(option, id)| {
            let TaskPublicCandidateV1::Strategic { action } = &option.candidate else { unreachable!() };
            let mut brief = option.label.clone();
            let mut detail = format!("{}\n", option.label);
            crate::decision::write_choice_mechanics(&mut detail, action, false, locale);
            crate::decision::write_event_resource_modifiers(&mut detail, action, observation, locale);
            if let TaskStrategicActionV1::ChooseRouteNode { x, y, .. } = action {
                match crate::public_map_reading::PublicRouteReading::new(&observation.public_map, (*x, *y), locale) {
                    Ok(route) => {
                        detail.push_str(&route.detail());
                        write!(brief, " | {}", route.summary()).expect("string write");
                    }
                    Err(error) => {
                        let unavailable = text(locale, "routeUnavailable", &[("error", &error)]);
                        detail.push_str(&unavailable);
                        write!(brief, " | {unavailable}").expect("string write");
                    }
                }
            }
            if let TaskStrategicActionV1::BeginRunCardSelection { selectable_card_ordinals, .. } = action {
                let cards = crate::run_selection::project_deck_cards_v1(&observation.cards, selectable_card_ordinals)?;
                writeln!(detail, "{}", text(locale, "eligibleCards", &[("cards", &cards.iter().map(|card| format!(
                    "{}: {}", card.ordinal, crate::compact::permanent_card_label_v1(card, locale)
                )).collect::<Vec<_>>().join(", "))])).expect("string write");
            }
            // Hash typed content with the same sorted JSON encoding as Python.
            let typed = serde_json::to_value(&option.candidate).map_err(|e| e.to_string())?;
            let bytes = serde_json::to_vec(&typed).map_err(|e| e.to_string())?;
            Ok(json!({"candidate_id": id, "candidate_sha256": format!("{:x}", Sha256::digest(bytes)),
                "brief": brief, "detail": detail}))
        }).collect::<Result<Vec<_>, String>>()?;
    let observation_bytes = serde_json::to_vec(
        &serde_json::to_value(&decision.observation).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "schema": "sts-strategic-reading-surface-v1",
        "public_snapshot_id": decision.information.snapshot_id,
        "observation_sha256": format!("{:x}", Sha256::digest(observation_bytes)),
        "decision_site": decision.site,
        "context_summary": summary,
        "context_detail": context_details,
        "candidates": candidates,
    }))
}
