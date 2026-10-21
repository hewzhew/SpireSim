use serde::Serialize;
use serde_json::{json, Value};
use sts_oracle_learning::agent::task::{
    TaskDecisionSiteV1, TaskPublicCandidateV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskStrategicActionV1, TaskStrategicObservationV1,
};
use sts_oracle_learning::ai::planner_core::{
    PlannerDecisionContext, PlannerEventCardPoolV1, PlannerEventPublicEffectV1,
    PlannerRewardDescriptor,
};

const REFERENCE_REQUEST_SCHEMA_V3: &str = "sts-reference-decision-request-v3";

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceRequestV3<'a> {
    pub schema: &'static str,
    pub decision_site: TaskDecisionSiteV1,
    pub public_context: &'a TaskPublicObservationV1,
    pub candidates: Vec<ReferenceCandidateV2<'a>>,
    pub contextual_entities: Vec<ReferenceEntityV1>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceCandidateV2<'a> {
    pub candidate_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub typed_candidate: &'a TaskPublicCandidateV1,
    pub entities: Vec<ReferenceEntityV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entityless_reason: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceEntityV1 {
    pub kind: &'static str,
    pub stable_id: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<&'static str>,
    pub simulator_facts: Value,
}

pub(crate) fn reference_request_v3(
    decision: &TaskPublicDecisionV1,
) -> Result<ReferenceRequestV3<'_>, String> {
    decision.validate()?;
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err(
            "run-play reference-request currently accepts strategic decisions only".to_string(),
        );
    };
    if decision.candidates.len()
        != decision
            .information
            .candidate_surface
            .ordered_candidate_ids
            .len()
    {
        return Err(
            "reference candidates disagree with the public candidate identity surface".into(),
        );
    }

    let candidates = decision
        .candidates
        .iter()
        .zip(
            decision
                .information
                .candidate_surface
                .ordered_candidate_ids
                .iter(),
        )
        .map(|(candidate, candidate_id)| {
            reference_candidate_v2(candidate, candidate_id, observation)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ReferenceRequestV3 {
        schema: REFERENCE_REQUEST_SCHEMA_V3,
        decision_site: decision.site,
        public_context: &decision.observation,
        candidates,
        contextual_entities: contextual_entities_v1(observation),
    })
}

fn reference_candidate_v2<'a>(
    candidate: &'a TaskPublicCandidateV1,
    candidate_id: &'a str,
    observation: &TaskStrategicObservationV1,
) -> Result<ReferenceCandidateV2<'a>, String> {
    let TaskPublicCandidateV1::Strategic { action } = candidate else {
        return Err("reference request received a non-strategic candidate".to_string());
    };
    let mut entities = Vec::new();
    candidate_entities_v1(action, observation, &mut entities)?;
    let entityless_reason = entities
        .is_empty()
        .then_some("the typed action does not directly identify a gameplay entity");
    Ok(ReferenceCandidateV2 {
        candidate_id,
        label: candidate_label_v1(action),
        typed_candidate: candidate,
        entities,
        entityless_reason,
    })
}

fn push_entity_v1(
    entities: &mut Vec<ReferenceEntityV1>,
    kind: &'static str,
    stable_id: &'static str,
    simulator_facts: Value,
) {
    if entities
        .iter()
        .any(|entity| entity.kind == kind && entity.stable_id == stable_id)
    {
        return;
    }
    entities.push(ReferenceEntityV1 {
        kind,
        stable_id,
        display_name: None,
        simulator_facts,
    });
}

fn candidate_entities_v1(
    action: &TaskStrategicActionV1,
    observation: &TaskStrategicObservationV1,
    entities: &mut Vec<ReferenceEntityV1>,
) -> Result<(), String> {
    match action {
        TaskStrategicActionV1::ChooseEventOption {
            event,
            public_semantics,
            ..
        } => {
            push_entity_v1(
                entities,
                "event",
                event.stable_id(),
                serde_json::to_value(public_semantics).map_err(|error| {
                    format!("failed to serialize event candidate facts: {error}")
                })?,
            );
            for effect in public_semantics
                .structured_semantics
                .iter()
                .flat_map(|semantics| semantics.effects.iter())
            {
                event_effect_entities_v1(effect, entities);
            }
        }
        TaskStrategicActionV1::TakeCard { card, upgrades, .. }
        | TaskStrategicActionV1::Smith { card, upgrades, .. }
        | TaskStrategicActionV1::Toke { card, upgrades, .. }
        | TaskStrategicActionV1::BuyCard { card, upgrades, .. }
        | TaskStrategicActionV1::RemoveCard { card, upgrades, .. } => push_entity_v1(
            entities,
            "card",
            card.stable_id(),
            json!({ "upgrades": upgrades }),
        ),
        TaskStrategicActionV1::BuyRelic { relic, .. }
        | TaskStrategicActionV1::TakeBossRelic { relic, .. } => {
            push_entity_v1(entities, "relic", relic.stable_id(), json!({}));
        }
        TaskStrategicActionV1::BuyPotion { potion, .. }
        | TaskStrategicActionV1::UseRunPotion { potion, .. }
        | TaskStrategicActionV1::DiscardRunPotion { potion, .. } => {
            push_entity_v1(entities, "potion", potion.stable_id(), json!({}));
        }
        TaskStrategicActionV1::ClaimReward { reward, .. } => {
            reward_entities_v1(reward, entities);
        }
        TaskStrategicActionV1::BeginRunCardSelection {
            selectable_card_ordinals,
            ..
        } => {
            for ordinal in selectable_card_ordinals {
                let card = observation.cards.get(*ordinal).ok_or_else(|| {
                    format!("reference selection card ordinal is out of bounds: {ordinal}")
                })?;
                push_entity_v1(
                    entities,
                    "card",
                    card.card.stable_id(),
                    json!({ "upgrades": card.upgrades, "ordinal": ordinal }),
                );
            }
        }
        TaskStrategicActionV1::SubmitRunSelection {
            selected_card_ordinals,
            ..
        } => {
            for ordinal in selected_card_ordinals {
                let card = observation.cards.get(*ordinal).ok_or_else(|| {
                    format!("reference selected card ordinal is out of bounds: {ordinal}")
                })?;
                push_entity_v1(
                    entities,
                    "card",
                    card.card.stable_id(),
                    json!({ "upgrades": card.upgrades, "ordinal": ordinal }),
                );
            }
        }
        TaskStrategicActionV1::ChooseRouteNode { .. }
        | TaskStrategicActionV1::OpenCardReward { .. }
        | TaskStrategicActionV1::SingingBowl { .. }
        | TaskStrategicActionV1::SkipCardReward { .. }
        | TaskStrategicActionV1::Rest
        | TaskStrategicActionV1::Dig
        | TaskStrategicActionV1::Lift
        | TaskStrategicActionV1::Recall
        | TaskStrategicActionV1::OpenPendingRewards
        | TaskStrategicActionV1::LeaveShop
        | TaskStrategicActionV1::SkipBossRelic
        | TaskStrategicActionV1::OpenChest
        | TaskStrategicActionV1::Proceed { .. }
        | TaskStrategicActionV1::Cancel { .. } => {}
    }
    Ok(())
}

fn event_effect_entities_v1(
    effect: &PlannerEventPublicEffectV1,
    entities: &mut Vec<ReferenceEntityV1>,
) {
    match effect {
        PlannerEventPublicEffectV1::ObtainRelic {
            pool: sts_oracle_learning::ai::planner_core::PlannerEventRelicPoolV1::Specific { relic },
            ..
        } => push_entity_v1(entities, "relic", relic.stable_id(), json!({})),
        PlannerEventPublicEffectV1::ObtainCard {
            pool: PlannerEventCardPoolV1::Specific { card, upgrades },
            ..
        }
        | PlannerEventPublicEffectV1::ObtainColorlessCard {
            pool: PlannerEventCardPoolV1::Specific { card, upgrades },
            ..
        }
        | PlannerEventPublicEffectV1::OfferCards {
            pool: PlannerEventCardPoolV1::Specific { card, upgrades },
            ..
        }
        | PlannerEventPublicEffectV1::ObtainCurse {
            pool: PlannerEventCardPoolV1::Specific { card, upgrades },
            ..
        } => push_entity_v1(
            entities,
            "card",
            card.stable_id(),
            json!({ "upgrades": upgrades }),
        ),
        _ => {}
    }
}

fn reward_entities_v1(reward: &PlannerRewardDescriptor, entities: &mut Vec<ReferenceEntityV1>) {
    match reward {
        PlannerRewardDescriptor::CardReward { cards } => {
            for card in cards {
                push_entity_v1(
                    entities,
                    "card",
                    card.card.stable_id(),
                    json!({ "upgrades": card.upgrades }),
                );
            }
        }
        PlannerRewardDescriptor::Relic { relic } => {
            push_entity_v1(entities, "relic", relic.stable_id(), json!({}));
        }
        PlannerRewardDescriptor::Potion { potion } => {
            push_entity_v1(entities, "potion", potion.stable_id(), json!({}));
        }
        PlannerRewardDescriptor::Gold { .. }
        | PlannerRewardDescriptor::StolenGold { .. }
        | PlannerRewardDescriptor::EmeraldKey
        | PlannerRewardDescriptor::SapphireKey => {}
    }
}

fn contextual_entities_v1(observation: &TaskStrategicObservationV1) -> Vec<ReferenceEntityV1> {
    let mut entities = Vec::new();
    if let Some(boss) = observation.public_map.boss {
        push_entity_v1(
            &mut entities,
            "enemy",
            boss.stable_id(),
            json!({ "role": "act_boss" }),
        );
    }
    if let PlannerDecisionContext::Event { event, screen, .. } = observation.context {
        push_entity_v1(
            &mut entities,
            "event",
            event.stable_id(),
            json!({ "screen": screen }),
        );
    }
    entities
}

fn candidate_label_v1(action: &TaskStrategicActionV1) -> Option<String> {
    match action {
        TaskStrategicActionV1::ChooseEventOption {
            public_semantics, ..
        } => public_semantics.public_label.clone(),
        TaskStrategicActionV1::TakeCard { card, upgrades, .. } => {
            Some(format!("{}+{}", card.stable_id(), upgrades))
        }
        TaskStrategicActionV1::BuyCard {
            card,
            upgrades,
            price,
            ..
        } => Some(format!("{}+{} ({price} gold)", card.stable_id(), upgrades)),
        TaskStrategicActionV1::BuyRelic { relic, price, .. } => {
            Some(format!("{} ({price} gold)", relic.stable_id()))
        }
        TaskStrategicActionV1::BuyPotion { potion, price, .. } => {
            Some(format!("{} ({price} gold)", potion.stable_id()))
        }
        TaskStrategicActionV1::UseRunPotion { potion, slot } => {
            Some(format!("use {} from slot {slot}", potion.stable_id()))
        }
        TaskStrategicActionV1::DiscardRunPotion { potion, slot } => {
            Some(format!("discard {} from slot {slot}", potion.stable_id()))
        }
        TaskStrategicActionV1::TakeBossRelic { relic, .. } => Some(relic.stable_id().to_string()),
        TaskStrategicActionV1::SkipCardReward { .. } => Some("skip card reward".to_string()),
        TaskStrategicActionV1::SkipBossRelic => Some("skip boss relic".to_string()),
        TaskStrategicActionV1::Rest => Some("rest".to_string()),
        TaskStrategicActionV1::Dig => Some("dig".to_string()),
        TaskStrategicActionV1::Lift => Some("lift".to_string()),
        TaskStrategicActionV1::Recall => Some("recall".to_string()),
        TaskStrategicActionV1::LeaveShop => Some("leave shop".to_string()),
        TaskStrategicActionV1::OpenCardReward { .. } => Some("open card reward".to_string()),
        TaskStrategicActionV1::SingingBowl { .. } => Some("singing bowl".to_string()),
        TaskStrategicActionV1::OpenPendingRewards => Some("open pending rewards".to_string()),
        TaskStrategicActionV1::OpenChest => Some("open chest".to_string()),
        TaskStrategicActionV1::Proceed { site } => Some(format!("proceed from {site:?}")),
        TaskStrategicActionV1::Cancel { site } => Some(format!("cancel {site:?}")),
        _ => None,
    }
}
