//! Ordered public master-deck selection domains shared by protocol and readings.

use serde::Serialize;
use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1, TaskPublicDecisionV1, TaskPublicObservationV1, TaskStrategicActionV1,
    TaskStrategicCardV1,
};
use sts_oracle_learning::ai::card_semantics_v1::{card_owned_reading_v1, CardOwnedReadingV1};
use sts_oracle_learning::runtime::combat::CardBlueprint;
use sts_oracle_learning::state::selection::{SelectionReason, SelectionScope};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunSelectionFamilyV1 {
    pub scope: SelectionScope,
    pub reason: SelectionReason,
    pub min_choices: usize,
    pub max_choices: usize,
    pub selectable_cards: Vec<TaskStrategicCardV1>,
    pub card_tokens: Vec<String>,
    pub card_readings: Vec<CardOwnedReadingV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunSelectionTargetV1 {
    pub card: TaskStrategicCardV1,
    pub reading: CardOwnedReadingV1,
}

pub(crate) fn project_run_selection_target_v1(
    decision: &TaskPublicDecisionV1,
    candidate: &TaskPublicCandidateV1,
) -> Result<Option<RunSelectionTargetV1>, String> {
    let TaskPublicCandidateV1::Strategic {
        action:
            TaskStrategicActionV1::SubmitRunSelection {
                scope: SelectionScope::Deck,
                selected_card_ordinals,
            },
    } = candidate
    else {
        return Ok(None);
    };
    if selected_card_ordinals.len() != 1 {
        return Ok(None);
    }
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err("run selection target requires a strategic observation".into());
    };
    let card = project_deck_cards_v1(&observation.cards, selected_card_ordinals)?[0];
    Ok(Some(RunSelectionTargetV1 {
        card,
        reading: card_reading(card),
    }))
}

fn card_reading(card: TaskStrategicCardV1) -> CardOwnedReadingV1 {
    card_owned_reading_v1(&CardBlueprint {
        upgrades: card.upgrades,
        misc_value: card.misc_value,
        base_damage_override: card.base_damage_override,
        base_block_override: card.base_block_override,
        cost_modifier: card.cost_modifier,
        ..CardBlueprint::fresh(card.card)
    })
}

pub(crate) fn project_run_selection_family_v1(
    decision: &TaskPublicDecisionV1,
    candidate: &TaskPublicCandidateV1,
) -> Result<Option<RunSelectionFamilyV1>, String> {
    let TaskPublicCandidateV1::Strategic { action } = candidate else {
        return Ok(None);
    };
    if !matches!(action, TaskStrategicActionV1::BeginRunCardSelection { .. }) {
        return Ok(None);
    }
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err("run selection family requires a strategic observation".to_string());
    };
    project_run_selection_family_from_parts_v1(&observation.cards, action)
}

pub(crate) fn project_run_selection_family_from_parts_v1(
    cards: &[TaskStrategicCardV1],
    action: &TaskStrategicActionV1,
) -> Result<Option<RunSelectionFamilyV1>, String> {
    let TaskStrategicActionV1::BeginRunCardSelection {
        scope,
        reason,
        min_choices,
        max_choices,
        selectable_card_ordinals,
    } = action
    else {
        return Ok(None);
    };
    let selectable_cards = project_deck_cards_v1(cards, selectable_card_ordinals)?;
    let card_readings = selectable_cards.iter().copied().map(card_reading).collect();
    Ok(Some(RunSelectionFamilyV1 {
        scope: *scope,
        reason: *reason,
        min_choices: *min_choices,
        max_choices: *max_choices,
        selectable_cards,
        card_tokens: crate::selector::run_card_selection_tokens_v1(
            cards,
            selectable_card_ordinals,
        )?,
        card_readings,
    }))
}

pub(crate) fn project_deck_cards_v1(
    cards: &[TaskStrategicCardV1],
    ordinals: &[usize],
) -> Result<Vec<TaskStrategicCardV1>, String> {
    let mut selected = Vec::with_capacity(ordinals.len());
    for ordinal in ordinals {
        let card = cards
            .iter()
            .find(|card| card.ordinal == *ordinal)
            .copied()
            .ok_or_else(|| format!("run selection references missing deck ordinal {ordinal}"))?;
        selected.push(card);
    }
    Ok(selected)
}
