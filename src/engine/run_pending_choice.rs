use crate::content::cards::{get_card_definition, CardRarity, CardType};
use crate::content::relics::RelicId;
use crate::runtime::combat::{CardLineageId, CombatCard};
use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::EventId;
use crate::state::run::RunState;
use crate::state::selection::{
    DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
    SelectionTargetRef,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunPendingChoicePostResolutionV1 {
    None,
    ResumeEvent(EventId),
}

#[derive(Clone, Debug)]
struct ResolvedRunPendingChoiceV1 {
    source: DomainEventSource,
    selected_lineages_in_order: Vec<CardLineageId>,
    sorted_indices_desc: Vec<usize>,
    post_resolution: RunPendingChoicePostResolutionV1,
}

pub(crate) fn tick_run_pending_choice_v1(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    rpc_state: &RunPendingChoiceState,
    input: Option<ClientInput>,
) -> Result<bool, &'static str> {
    let mut prefix_indices = Vec::new();
    for lineage in &rpc_state.selected {
        let Some(index) = run_state
            .master_deck
            .iter()
            .position(|card| card.lineage == *lineage)
        else {
            return Ok(true);
        };
        if prefix_indices.contains(&index)
            || !crate::state::core::run_pending_choice_allows_card_for_run(
                &rpc_state.reason,
                &run_state.master_deck[index],
                run_state,
            )
        {
            return Ok(true);
        }
        prefix_indices.push(index);
    }
    let indices = if let Some(ClientInput::SelectCard(index)) = input.as_ref() {
        let Some(card) = run_state.master_deck.get(*index) else {
            return Ok(true);
        };
        if rpc_state.min_choices != rpc_state.max_choices
            || rpc_state.selected.len() >= rpc_state.max_choices
            || !rpc_state
                .selection_request(run_state)
                .targets
                .contains(&SelectionTargetRef::CardLineage(card.lineage))
        {
            return Ok(true);
        }
        let mut pending = rpc_state.clone();
        pending.selected.push(card.lineage);
        if pending.selected.len() < pending.max_choices {
            *engine_state = EngineState::RunPendingChoice(pending);
            return Ok(true);
        }
        prefix_indices.push(*index);
        Some(prefix_indices)
    } else {
        input
            .clone()
            .and_then(|value| resolve_run_pending_selection(value, run_state))
            .map(|mut indices| {
                prefix_indices.append(&mut indices);
                prefix_indices
            })
    };
    if let Some(indices) = indices {
        let Some(resolved) = resolve_run_pending_choice_v1(rpc_state, run_state, indices) else {
            return Ok(true);
        };
        apply_run_pending_choice_mutation_v1(rpc_state.reason, run_state, &resolved);
        restore_run_pending_return_state_v1(engine_state, run_state, rpc_state);
        apply_run_pending_post_resolution_v1(engine_state, run_state, resolved.post_resolution)?;
        return Ok(true);
    }

    if let Some(ClientInput::Cancel) = input {
        if !rpc_state.selection_request(run_state).can_cancel {
            return Ok(true);
        }
        restore_run_pending_return_state_v1(engine_state, run_state, rpc_state);
    }

    Ok(true)
}

fn resolve_run_pending_choice_v1(
    rpc_state: &RunPendingChoiceState,
    run_state: &mut RunState,
    indices: Vec<usize>,
) -> Option<ResolvedRunPendingChoiceV1> {
    if indices.len() < rpc_state.min_choices || indices.len() > rpc_state.max_choices {
        return None;
    }

    let mut seen_indices = Vec::new();
    for &idx in &indices {
        let card = run_state.master_deck.get(idx)?;
        if seen_indices.contains(&idx)
            || !crate::state::core::run_pending_choice_allows_card_for_run(
                &rpc_state.reason,
                card,
                run_state,
            )
        {
            return None;
        }
        seen_indices.push(idx);
    }

    let source = rpc_state.source;
    let selection_reason: SelectionReason = rpc_state.reason.into();
    let selected_refs = indices
        .iter()
        .filter_map(|&idx| run_state.master_deck.get(idx))
        .map(|card| SelectionTargetRef::CardLineage(card.lineage))
        .collect::<Vec<_>>();
    let selected_lineages_in_order = selected_refs
        .iter()
        .map(|target| match target {
            SelectionTargetRef::CardLineage(lineage) => *lineage,
        })
        .collect::<Vec<_>>();

    run_state.emit_event(DomainEvent::SelectionResolved {
        scope: SelectionScope::Deck,
        reason: selection_reason,
        selected: selected_refs,
        source,
    });

    let mut sorted_indices_desc = indices;
    sorted_indices_desc.sort_unstable();
    sorted_indices_desc.reverse();

    Some(ResolvedRunPendingChoiceV1 {
        source,
        selected_lineages_in_order,
        sorted_indices_desc,
        post_resolution: post_resolution_for_source_v1(source),
    })
}

fn apply_run_pending_choice_mutation_v1(
    reason: RunPendingChoiceReason,
    run_state: &mut RunState,
    resolved: &ResolvedRunPendingChoiceV1,
) {
    match reason {
        RunPendingChoiceReason::Purge | RunPendingChoiceReason::PurgeNonBottled => {
            apply_run_pending_purge_v1(run_state, resolved);
        }
        RunPendingChoiceReason::Upgrade => {
            for idx in &resolved.sorted_indices_desc {
                if *idx < run_state.master_deck.len() {
                    let lineage = run_state.master_deck[*idx].lineage;
                    run_state.upgrade_card_with_source(lineage, resolved.source);
                }
            }
        }
        RunPendingChoiceReason::Transform | RunPendingChoiceReason::TransformNonBottled => {
            if resolved.source == DomainEventSource::Event(EventId::Neow)
                && resolved.selected_lineages_in_order.len() > 1
            {
                run_state.transform_card_lineages_after_removing_all_with_source(
                    &resolved.selected_lineages_in_order,
                    false,
                    resolved.source,
                );
            } else if resolved.selected_lineages_in_order.len() > 1 {
                run_state.transform_card_lineages_deferred_obtain_with_source(
                    &resolved.selected_lineages_in_order,
                    false,
                    resolved.source,
                );
            } else {
                run_state.transform_card_lineages_with_source(
                    &resolved.selected_lineages_in_order,
                    false,
                    resolved.source,
                );
            }
        }
        RunPendingChoiceReason::TransformUpgraded => {
            run_state.transform_card_lineages_deferred_obtain_with_source(
                &resolved.selected_lineages_in_order,
                true,
                resolved.source,
            );
        }
        RunPendingChoiceReason::Duplicate => {
            let cards_to_copy = resolved
                .sorted_indices_desc
                .iter()
                .filter_map(|&idx| run_state.master_deck.get(idx).cloned())
                .collect::<Vec<_>>();
            for card in cards_to_copy {
                run_state.add_card_instance_copy_to_deck_from(&card, resolved.source);
            }
        }
        reason @ (RunPendingChoiceReason::BottleFlame
        | RunPendingChoiceReason::BottleLightning
        | RunPendingChoiceReason::BottleTornado) => {
            if let Some((relic_id, card_type)) = bottled_choice_target(&reason) {
                assign_bottled_card(
                    run_state,
                    relic_id,
                    card_type,
                    &resolved.sorted_indices_desc,
                );
            }
        }
    }
}

fn apply_run_pending_purge_v1(run_state: &mut RunState, resolved: &ResolvedRunPendingChoiceV1) {
    let source_event_id = match resolved.source {
        DomainEventSource::Event(event_id) => Some(event_id),
        _ => None,
    };

    for lineage in &resolved.selected_lineages_in_order {
        let Some(idx) = run_state
            .master_deck
            .iter()
            .position(|card| card.lineage == *lineage)
        else {
            continue;
        };

        let rarity_state = event_offer_rarity_state_v1(&run_state.master_deck[idx]);
        if let Some(event_id) = source_event_id {
            apply_event_purge_side_effects_v1(run_state, event_id, idx, rarity_state);
        }
        let lineage = run_state.master_deck[idx].lineage;
        run_state.remove_card_from_deck_with_source(lineage, resolved.source);
    }
}

fn event_offer_rarity_state_v1(card: &CombatCard) -> i32 {
    match get_card_definition(card.id).rarity {
        CardRarity::Curse => 0,
        CardRarity::Basic => 1,
        CardRarity::Common => 2,
        CardRarity::Special => 3,
        CardRarity::Uncommon => 4,
        CardRarity::Rare => 5,
    }
}

fn apply_event_purge_side_effects_v1(
    run_state: &mut RunState,
    event_id: EventId,
    deck_index: usize,
    rarity_state: i32,
) {
    if run_state.event_state.as_ref().map(|event| event.id) != Some(event_id) {
        return;
    }

    match event_id {
        EventId::BonfireElementals => {
            if let Some(ref mut es) = run_state.event_state {
                es.internal_state = rarity_state;
            }
            let mut reward_engine_state = EngineState::EventRoom;
            crate::content::events::bonfire_elementals::apply_offer_reward(
                &mut reward_engine_state,
                run_state,
                rarity_state,
            );
            if let Some(ref mut es) = run_state.event_state {
                es.current_screen = 3;
            }
        }
        EventId::BonfireSpirits => {
            if let Some(ref mut es) = run_state.event_state {
                es.internal_state = rarity_state;
            }
            let mut reward_engine_state = EngineState::EventRoom;
            crate::content::events::bonfire_elementals::apply_offer_reward(
                &mut reward_engine_state,
                run_state,
                rarity_state,
            );
            if let Some(ref mut es) = run_state.event_state {
                es.current_screen = 3;
            }
        }
        EventId::NoteForYourself => {
            let saved_card = &run_state.master_deck[deck_index];
            run_state.note_for_yourself_card = saved_card.id;
            run_state.note_for_yourself_upgrades = saved_card.upgrades;
        }
        _ => {}
    }
}

fn restore_run_pending_return_state_v1(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    rpc_state: &RunPendingChoiceState,
) {
    *engine_state = *rpc_state.return_state.clone();
    if run_state.complete_pending_boss_act_transition() {
        *engine_state = EngineState::MapNavigation;
    }
}

fn apply_run_pending_post_resolution_v1(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    post_resolution: RunPendingChoicePostResolutionV1,
) -> Result<(), &'static str> {
    let RunPendingChoicePostResolutionV1::ResumeEvent(event_id) = post_resolution else {
        return Ok(());
    };
    if !matches!(engine_state, EngineState::EventRoom) {
        return Ok(());
    }
    if run_state.event_state.as_ref().map(|event| event.id) != Some(event_id) {
        return Ok(());
    }
    crate::engine::event_handler::handle_event_post_run_pending_choice(engine_state, run_state)?;
    Ok(())
}

fn post_resolution_for_source_v1(source: DomainEventSource) -> RunPendingChoicePostResolutionV1 {
    match source {
        DomainEventSource::Event(event_id) => {
            RunPendingChoicePostResolutionV1::ResumeEvent(event_id)
        }
        _ => RunPendingChoicePostResolutionV1::None,
    }
}

fn bottled_choice_target(reason: &RunPendingChoiceReason) -> Option<(RelicId, CardType)> {
    match reason {
        RunPendingChoiceReason::BottleFlame => Some((RelicId::BottledFlame, CardType::Attack)),
        RunPendingChoiceReason::BottleLightning => {
            Some((RelicId::BottledLightning, CardType::Skill))
        }
        RunPendingChoiceReason::BottleTornado => Some((RelicId::BottledTornado, CardType::Power)),
        _ => None,
    }
}

fn assign_bottled_card(
    run_state: &mut RunState,
    relic_id: RelicId,
    card_type: CardType,
    selected_indices: &[usize],
) {
    let Some(&idx) = selected_indices.first() else {
        return;
    };
    let Some(card) = run_state.master_deck.get(idx) else {
        return;
    };
    let def = get_card_definition(card.id);
    if def.card_type != card_type {
        return;
    }

    let selected_lineage_payload = card.lineage.to_runtime_i32_payload();
    if let Some(relic) = run_state
        .relics
        .iter_mut()
        .rev()
        .find(|relic| relic.id == relic_id && relic.amount == 0)
    {
        relic.amount = selected_lineage_payload;
    } else if let Some(relic) = run_state
        .relics
        .iter_mut()
        .rev()
        .find(|relic| relic.id == relic_id)
    {
        relic.amount = selected_lineage_payload;
    }
}

fn resolve_run_pending_selection(input: ClientInput, run_state: &RunState) -> Option<Vec<usize>> {
    match input {
        ClientInput::SubmitSelection(SelectionResolution {
            scope: SelectionScope::Deck,
            selected,
        }) => Some(
            selected
                .into_iter()
                .filter_map(|target| {
                    let lineage = target.card_lineage();
                    run_state
                        .master_deck
                        .iter()
                        .position(|card| card.lineage == lineage)
                })
                .collect(),
        ),
        _ => None,
    }
}
