//! Display projections of the already validated public decision. No scoring,
//! state reconstruction, action resolution, or hidden-state access lives here.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1, TaskPublicDecisionV1, TaskPublicObservationV1, TaskStrategicActionV1,
};
use sts_oracle_learning::ai::planner_core::PlannerDecisionContext;

pub(super) fn project(decision: &TaskPublicDecisionV1) -> Result<Value, String> {
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err("strategic reading requires a strategic observation".into());
    };
    let mut context_details = String::new();
    let summary = match &observation.context {
        PlannerDecisionContext::Map { overlay } => format!("Map · overlay={overlay}"),
        PlannerDecisionContext::CardReward { reward_item_index, pending_rewards } => {
            for (index, reward) in pending_rewards.iter().enumerate() {
                context_details.push_str(&format!("Reward item {index}: {}\n",
                    crate::render::render_reward_descriptor(reward)));
            }
            format!("Card reward · reward item {reward_item_index:?}")
        }
        PlannerDecisionContext::Event { event, screen, .. } => {
            format!("Event {event:?} · screen {screen}; expand candidates for effects")
        }
        PlannerDecisionContext::Shop {
            purge_cost,
            purge_available,
            inventory,
        } => {
            crate::render::render_shop_inventory_v1(&mut context_details, &observation.context);
            format!("Shop · removal {purge_cost}g / available={purge_available} · cards/relics/potions remaining={}/{}/{}; expand inventory for details",
                inventory.cards.len(), inventory.relics.len(), inventory.potions.len())
        }
        PlannerDecisionContext::Reward { pending_rewards } => {
            for (index, reward) in pending_rewards.iter().enumerate() {
                context_details.push_str(&format!(
                    "Reward item {index}: {}\n",
                    crate::render::render_reward_descriptor(reward)
                ));
            }
            format!(
                "Rewards · {} items remaining; visible rewards may not be claimable yet",
                pending_rewards.len()
            )
        }
        PlannerDecisionContext::Campfire => "Campfire · available rest, upgrade and other actions are listed in the current candidates".into(),
        PlannerDecisionContext::BossRelic => "Boss relic choice".into(),
        PlannerDecisionContext::RunChoice { reason, min_choices, max_choices, .. } =>
            format!("Card selection · {reason:?} · choose {min_choices}..{max_choices}; expand candidates for eligible cards"),
        PlannerDecisionContext::Treasure => "Treasure".into(),
        PlannerDecisionContext::RunTransition { stage } => {
            crate::decision::run_transition_label_v1(*stage, sts_presentation::Locale::En).into()
        }
    };
    let candidates = decision.candidates.iter()
        .zip(&decision.information.candidate_surface.ordered_candidate_ids)
        .map(|(candidate, id)| {
            let TaskPublicCandidateV1::Strategic { action } = candidate else {
                return Err("strategic reading contains a non-strategic candidate".to_owned());
            };
            let rendered = crate::render::render_candidate_with_observation(candidate, Some(&decision.observation));
            let mut detail = rendered.clone();
            let brief = match action {
                TaskStrategicActionV1::ChooseRouteNode { x, y, .. } => {
                    match crate::public_map_reading::PublicRouteReading::new(&observation.public_map, (*x, *y)) {
                        Ok(route) => {
                            detail.push('\n');
                            detail.push_str(&route.detail());
                            format!("{rendered} · {}", route.summary())
                        }
                        Err(error) => {
                            detail.push_str(&format!("\nRoute topology unknown: {error}; original candidate retained, no route conclusion."));
                            format!("{rendered} · route topology unknown: {error}")
                        }
                    }
                }
                TaskStrategicActionV1::ChooseEventOption { event, screen, option_index, public_semantics, action } => {
                    let label = public_semantics.public_label.clone().unwrap_or_else(|| format!("{action:?}; public label Unknown"));
                    format!("{event:?} / screen {screen} / option {option_index}: {label}; expand candidate for full effects")
                }
                TaskStrategicActionV1::BeginRunCardSelection { scope, reason, min_choices, max_choices, selectable_card_ordinals } => {
                    format!("{reason:?} · {scope:?} · select {min_choices}..={max_choices} cards / {} eligible; expand candidate for eligible cards", selectable_card_ordinals.len())
                }
                _ => detail.clone(),
            };
            // Hash the sorted JSON value, the same encoding used by the Python
            // reader. It binds presentation to typed content without parsing prose.
            let typed = serde_json::to_value(candidate).map_err(|e| e.to_string())?;
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
