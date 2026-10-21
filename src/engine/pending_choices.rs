use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CardLineageId, CombatState};
use crate::state::core::{ClientInput, EngineState, GridSelectReason, HandSelectReason, PileType};
use crate::state::selection::{
    DomainCardSnapshot, DomainEvent, DomainEventSource, SelectionResolution, SelectionScope,
    SelectionTargetRef,
};

fn selection_to_lineages(
    input: ClientInput,
    expected_scope: SelectionScope,
) -> Result<Vec<CardLineageId>, &'static str> {
    match input {
        ClientInput::SubmitSelection(SelectionResolution { scope, selected }) => {
            if scope != expected_scope {
                return Err("Selection scope does not match pending choice");
            }
            Ok(selected
                .into_iter()
                .map(SelectionTargetRef::card_lineage)
                .collect())
        }
        _ => Err("Invalid input for selection"),
    }
}

fn reject_duplicate_lineages(
    lineages: &[CardLineageId],
    message: &'static str,
) -> Result<(), &'static str> {
    for (idx, lineage) in lineages.iter().enumerate() {
        if lineages[..idx].contains(lineage) {
            return Err(message);
        }
    }
    Ok(())
}

fn pile_contains_all<'a>(
    pile: impl IntoIterator<Item = &'a crate::runtime::combat::CombatCard>,
    lineages: &[CardLineageId],
) -> bool {
    let present = pile
        .into_iter()
        .map(|card| card.lineage)
        .collect::<std::collections::HashSet<_>>();
    lineages.iter().all(|lineage| present.contains(lineage))
}

fn snapshot_cards_from_hand(
    combat_state: &CombatState,
    lineages: &[CardLineageId],
) -> Vec<DomainCardSnapshot> {
    lineages
        .iter()
        .filter_map(|lineage| {
            combat_state
                .zones
                .hand
                .iter()
                .find(|card| card.lineage == *lineage)
                .map(|card| DomainCardSnapshot {
                    id: card.id,
                    upgrades: card.upgrades,
                    lineage: card.lineage,
                })
        })
        .collect()
}

pub fn handle_scry(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
    amount: usize,
    card_lineages: &[CardLineageId],
    input: ClientInput,
) -> Result<(), &'static str> {
    match input {
        ClientInput::SubmitScryDiscard(indices) => {
            if card_lineages.len() != amount {
                return Err("Scry candidate count mismatch");
            }
            let mut selected = Vec::new();
            for index in indices {
                if index >= card_lineages.len() {
                    return Err("Invalid discard indices");
                }
                let lineage = card_lineages[index];
                if selected.contains(&lineage) {
                    return Err("Duplicate scry discard index");
                }
                selected.push(lineage);
            }

            if !pile_contains_all(&combat_state.zones.draw_pile, &selected) {
                return Err("Scry candidate no longer in draw pile");
            }

            for lineage in selected {
                if let Some(pos) = combat_state
                    .zones
                    .draw_pile
                    .iter()
                    .position(|card| card.lineage == lineage)
                {
                    let card = combat_state.zones.draw_pile.remove(pos);
                    crate::engine::action_handlers::cards::move_card_to_discard_pile(
                        card,
                        combat_state,
                    );
                } else {
                    return Err("Scry candidate no longer in draw pile");
                }
            }
            for action in crate::content::cards::hooks::on_scry(combat_state) {
                combat_state.queue_action_back(action.action);
            }

            *engine_state = EngineState::CombatProcessing;
            Ok(())
        }
        _ => Err("Invalid input for Scry"),
    }
}

pub fn handle_hand_select(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
    candidate_lineages: &[CardLineageId],
    count: usize,
    requires_exact: bool,
    cancellable: bool,
    reason: HandSelectReason,
    input: ClientInput,
) -> Result<(), &'static str> {
    match input {
        ClientInput::Cancel => {
            if cancellable {
                *engine_state = EngineState::CombatProcessing;
                Ok(())
            } else {
                Err("Cannot cancel this selection")
            }
        }
        input => {
            let lineages = selection_to_lineages(input, SelectionScope::Hand)?;
            if lineages
                .iter()
                .any(|lineage| !candidate_lineages.contains(lineage))
            {
                return Err("Selected card is not in the frozen hand-select candidate set");
            }
            reject_duplicate_lineages(&lineages, "Duplicate hand selection")?;
            if !pile_contains_all(&combat_state.zones.hand, &lineages) {
                return Err("Selected hand card no longer in hand");
            }
            if requires_exact && lineages.len() != count {
                return Err("Must select exact number of cards");
            }
            if lineages.len() > count {
                return Err("Too many cards selected");
            }

            match reason {
                HandSelectReason::GamblingChip => {
                    let num_selected = lineages.len();
                    // Java GamblingChipAction queues the draw action to top
                    // before processing selected discards. Tactician's later
                    // addToTop must therefore run before this draw.
                    if num_selected > 0 {
                        let action = ActionInfo {
                            action: Action::DrawCards(num_selected as u32),
                            insertion_mode: AddTo::Top,
                        };
                        combat_state.queue_actions(smallvec::smallvec![action]);
                    }
                    crate::engine::action_handlers::cards::handle_discard_cards_with_order(
                        &lineages,
                        crate::engine::action_handlers::cards::DiscardHookOrder::RelicsThenCard,
                        combat_state,
                    );
                }
                HandSelectReason::Exhaust | HandSelectReason::PotionExhaust { .. } => {
                    let exhausted_cards = snapshot_cards_from_hand(combat_state, &lineages);
                    // Java ExhaustAction: exhaust selected cards from hand
                    for lineage in &lineages {
                        crate::engine::action_handlers::cards::handle_exhaust_card(
                            *lineage,
                            PileType::Hand,
                            combat_state,
                        );
                    }
                    if !exhausted_cards.is_empty() {
                        combat_state.emit_event(DomainEvent::CardsExhausted {
                            cards: exhausted_cards,
                            source: DomainEventSource::Selection(reason.into()),
                        });
                    }
                }
                HandSelectReason::Recycle => {
                    for lineage in &lineages {
                        crate::engine::action_handlers::cards::handle_recycle_selected_card(
                            *lineage,
                            combat_state,
                        );
                    }
                }
                HandSelectReason::Discard => {
                    // Java DiscardAction selected-card path triggers card
                    // manual-discard hooks before incrementDiscard relic hooks.
                    crate::engine::action_handlers::cards::handle_discard_cards_with_order(
                        &lineages,
                        crate::engine::action_handlers::cards::DiscardHookOrder::CardThenRelics,
                        combat_state,
                    );
                }
                HandSelectReason::PutOnDrawPile => {
                    // Move selected cards from hand to top of draw pile
                    for lineage in &lineages {
                        if let Some(pos) = combat_state
                            .zones
                            .hand
                            .iter()
                            .position(|c| c.lineage == *lineage)
                        {
                            let card = combat_state.zones.hand.remove(pos);
                            combat_state.add_card_to_draw_pile_top(card);
                        }
                    }
                }
                HandSelectReason::Setup => {
                    // Java SetupAction checks AbstractCard.cost, not costForTurn.
                    // Temporary turn-cost reductions do not stop the selected
                    // card from becoming free next time it is played.
                    for lineage in &lineages {
                        if let Some(pos) = combat_state
                            .zones
                            .hand
                            .iter()
                            .position(|c| c.lineage == *lineage)
                        {
                            let mut card = combat_state.zones.hand.remove(pos);
                            if card.combat_cost_without_turn_override_java() > 0 {
                                card.free_to_play_once = true;
                            }
                            combat_state.add_card_to_draw_pile_top(card);
                        }
                    }
                }
                HandSelectReason::PutToBottomOfDraw => {
                    // Java ForethoughtAction checks AbstractCard.cost, not
                    // costForTurn. Temporary turn-cost reductions do not stop
                    // the selected card from becoming free next time it is
                    // drawn.
                    for lineage in &lineages {
                        if let Some(pos) = combat_state
                            .zones
                            .hand
                            .iter()
                            .position(|c| c.lineage == *lineage)
                        {
                            let mut card = combat_state.zones.hand.remove(pos);
                            if card.combat_cost_without_turn_override_java() > 0 {
                                card.free_to_play_once = true;
                            }
                            combat_state.add_card_to_draw_pile_bottom(card);
                        }
                    }
                }
                HandSelectReason::Retain => {
                    // Java RetainCardsAction/MeditateAction sets AbstractCard.retain
                    // for one end-of-turn discard pass only for non-ethereal
                    // cards. RestoreRetainedCardsAction clears that flag after
                    // the card survives the turn.
                    for lineage in &lineages {
                        if let Some(card) = combat_state
                            .zones
                            .hand
                            .iter_mut()
                            .find(|c| c.lineage == *lineage)
                        {
                            if !crate::content::cards::is_ethereal(card) {
                                card.retain_override = Some(true);
                            }
                        }
                    }
                }
                HandSelectReason::Copy { amount } => {
                    // Java DualWieldAction reads selectedCards after the hand
                    // select screen has removed the selected original from
                    // hand. Its selected branch queues one extra copy to
                    // replace that original, so Rust must remove the selected
                    // card before applying `amount`.
                    if candidate_lineages.len() > 1 {
                        let mut remaining_candidates = Vec::new();
                        let mut selected_cards = Vec::new();
                        let mut non_candidates = Vec::new();

                        for card in combat_state.zones.hand.drain(..) {
                            if lineages.contains(&card.lineage) {
                                selected_cards.push(card);
                            } else if candidate_lineages.contains(&card.lineage) {
                                remaining_candidates.push(card);
                            } else {
                                non_candidates.push(card);
                            }
                        }

                        combat_state.zones.hand = remaining_candidates;
                        for card in non_candidates {
                            combat_state.add_card_to_hand(card);
                        }
                        for card in selected_cards {
                            let constructed =
                                crate::content::cards::prepare_make_temp_card_in_hand_constructor(
                                    card,
                                    combat_state,
                                );
                            crate::engine::action_handlers::cards::handle_make_constructed_copy_in_hand(
                                constructed,
                                amount,
                                combat_state,
                            );
                        }
                    } else {
                        for lineage in &lineages {
                            if let Some(pos) = combat_state
                                .zones
                                .hand
                                .iter()
                                .position(|c| c.lineage == *lineage)
                            {
                                let card = combat_state.zones.hand.remove(pos);
                                let constructed =
                                    crate::content::cards::prepare_make_temp_card_in_hand_constructor(
                                        card,
                                        combat_state,
                                    );
                                crate::engine::action_handlers::cards::handle_make_constructed_copy_in_hand(
                                    constructed,
                                    amount,
                                    combat_state,
                                );
                            }
                        }
                    }
                }
                HandSelectReason::Nightmare { amount } => {
                    // Java NightmareAction's selected branch removes the
                    // selected card through the hand-select screen, applies a
                    // NightmarePower carrying a stat-equivalent copy, then
                    // returns the original selected card to hand with addToHand.
                    for lineage in &lineages {
                        if let Some(pos) = combat_state
                            .zones
                            .hand
                            .iter()
                            .position(|c| c.lineage == *lineage)
                        {
                            let card = combat_state.zones.hand.remove(pos);
                            crate::engine::action_handlers::cards::queue_nightmare_power_front(
                                &card,
                                amount,
                                combat_state,
                            );
                            combat_state.add_card_to_hand(card);
                        }
                    }
                }
                HandSelectReason::Upgrade => {
                    if candidate_lineages.len() > 1 {
                        let mut remaining_candidates = Vec::new();
                        let mut selected_cards = Vec::new();
                        let mut non_candidates = Vec::new();

                        for card in combat_state.zones.hand.drain(..) {
                            if lineages.contains(&card.lineage) {
                                selected_cards.push(card);
                            } else if candidate_lineages.contains(&card.lineage) {
                                remaining_candidates.push(card);
                            } else {
                                non_candidates.push(card);
                            }
                        }

                        combat_state.zones.hand = remaining_candidates;
                        for mut card in selected_cards {
                            let before = DomainCardSnapshot {
                                id: card.id,
                                upgrades: card.upgrades,
                                lineage: card.lineage,
                            };
                            crate::content::cards::upgrade_card_once_java(&mut card);
                            combat_state.add_card_to_hand(card);
                            combat_state.emit_event(DomainEvent::CardUpgraded {
                                before,
                                after: before.upgraded(),
                                source: DomainEventSource::Selection(reason.into()),
                            });
                        }
                        for card in non_candidates {
                            combat_state.add_card_to_hand(card);
                        }
                    } else {
                        // Java ArmamentsAction's single-upgradeable branch upgrades in place.
                        for lineage in &lineages {
                            if let Some(card) = combat_state
                                .zones
                                .hand
                                .iter_mut()
                                .find(|c| c.lineage == *lineage)
                            {
                                let before = DomainCardSnapshot {
                                    id: card.id,
                                    upgrades: card.upgrades,
                                    lineage: card.lineage,
                                };
                                crate::content::cards::upgrade_card_once_java(card);
                                combat_state.emit_event(DomainEvent::CardUpgraded {
                                    before,
                                    after: before.upgraded(),
                                    source: DomainEventSource::Selection(reason.into()),
                                });
                            }
                        }
                    }
                }
            }

            combat_state.emit_event(DomainEvent::SelectionResolved {
                scope: SelectionScope::Hand,
                reason: reason.into(),
                selected: lineages
                    .iter()
                    .copied()
                    .map(SelectionTargetRef::CardLineage)
                    .collect(),
                source: DomainEventSource::Selection(reason.into()),
            });
            *engine_state = EngineState::CombatProcessing;
            Ok(())
        }
    }
}

pub fn handle_grid_select(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
    candidate_lineages: &[CardLineageId],
    source_pile: PileType,
    min_cards: u8,
    max_cards: u8,
    can_cancel: bool,
    reason: GridSelectReason,
    input: ClientInput,
) -> Result<(), &'static str> {
    match input {
        ClientInput::Cancel => {
            if can_cancel {
                *engine_state = EngineState::CombatProcessing;
                Ok(())
            } else {
                Err("Cannot cancel this selection")
            }
        }
        input => {
            let lineages = selection_to_lineages(input, SelectionScope::Grid)?;
            if lineages
                .iter()
                .any(|lineage| !candidate_lineages.contains(lineage))
            {
                return Err("Selected card is not in the frozen grid-select candidate set");
            }
            reject_duplicate_lineages(&lineages, "Duplicate grid selection")?;
            if lineages.len() < min_cards as usize {
                return Err("Must select at least the minimum number of cards");
            }
            if lineages.len() > max_cards as usize {
                return Err("Too many cards selected");
            }
            match reason {
                GridSelectReason::DiscardToHand
                | GridSelectReason::DiscardToHandNoCostChange
                | GridSelectReason::DiscardToHandRetain => {
                    // Java BetterDiscardPileToHandAction has both cost-preserving
                    // and setCostForTurn(newCost) constructors. Liquid Memories
                    // uses the cost-setting path; Hologram does not.
                    if !matches!(source_pile, PileType::Discard) {
                        return Err("Discard-to-hand selection must source from discard pile");
                    }
                    if !pile_contains_all(&combat_state.zones.discard_pile, &lineages) {
                        return Err("Grid candidate no longer in discard pile");
                    }
                    for lineage in &lineages {
                        if combat_state.zones.hand.len() < 10 {
                            let pos = combat_state
                                .zones
                                .discard_pile
                                .iter()
                                .position(|c| c.lineage == *lineage)
                                .expect("validated discard-to-hand selection must still exist");
                            let mut card = combat_state.zones.discard_pile.remove(pos);
                            if reason == GridSelectReason::DiscardToHand {
                                card.set_cost_for_turn_java(0);
                            } else if reason == GridSelectReason::DiscardToHandRetain {
                                card.retain_override = Some(true);
                            }
                            combat_state.add_card_to_hand(card);
                        }
                    }
                }
                GridSelectReason::MoveToDrawPile => {
                    // Headbutt-style movement: selected card returns to the top of draw pile.
                    let candidates_still_present =
                        match source_pile {
                            PileType::Discard => {
                                pile_contains_all(&combat_state.zones.discard_pile, &lineages)
                            }
                            PileType::Exhaust => {
                                pile_contains_all(&combat_state.zones.exhaust_pile, &lineages)
                            }
                            _ => return Err(
                                "Move-to-draw selection must source from discard or exhaust pile",
                            ),
                        };
                    if !candidates_still_present {
                        return Err("Grid candidate no longer in source pile");
                    }
                    for lineage in &lineages {
                        let card = match source_pile {
                            PileType::Discard => combat_state
                                .zones
                                .discard_pile
                                .remove_by_lineage(*lineage)
                                .expect("validated discard selection must still exist"),
                            PileType::Exhaust => {
                                let pos = combat_state
                                    .zones
                                    .exhaust_pile
                                    .iter()
                                    .position(|card| card.lineage == *lineage)
                                    .expect("validated exhaust selection must still exist");
                                combat_state.zones.exhaust_pile.remove(pos)
                            }
                            _ => unreachable!("source pile was validated above"),
                        };
                        combat_state.add_card_to_draw_pile_top(card);
                    }
                }
                GridSelectReason::Exhume { upgrade } => {
                    if !matches!(source_pile, PileType::Exhaust) {
                        return Err("Exhume selection must source from exhaust pile");
                    }
                    if !pile_contains_all(&combat_state.zones.exhaust_pile, &lineages) {
                        return Err("Grid candidate no longer in exhaust pile");
                    }
                    for lineage in &lineages {
                        crate::engine::action_handlers::cards::handle_exhume_card(
                            *lineage,
                            upgrade,
                            combat_state,
                        );
                    }
                }
                GridSelectReason::DrawPileToHand
                | GridSelectReason::SkillFromDeckToHand
                | GridSelectReason::AttackFromDeckToHand => {
                    // Seek/SecretTechnique/SecretWeapon: move from draw pile to hand.
                    if !matches!(source_pile, PileType::Draw) {
                        return Err("Deck-to-hand selection must source from draw pile");
                    }
                    if !pile_contains_all(&combat_state.zones.draw_pile, &lineages) {
                        return Err("Grid candidate no longer in draw pile");
                    }
                    for lineage in &lineages {
                        let pos = combat_state
                            .zones
                            .draw_pile
                            .iter()
                            .position(|c| c.lineage == *lineage)
                            .expect("validated deck-to-hand selection must still exist");
                        let card = combat_state.zones.draw_pile.remove(pos);
                        if combat_state.zones.hand.len() < 10 {
                            combat_state.add_card_to_hand(card);
                        } else {
                            combat_state.add_card_to_discard_pile_top(card);
                        }
                    }
                }
                GridSelectReason::Omniscience { play_amount } => {
                    if !matches!(source_pile, PileType::Draw) {
                        return Err("Omniscience selection must source from draw pile");
                    }
                    if !pile_contains_all(&combat_state.zones.draw_pile, &lineages) {
                        return Err("Grid candidate no longer in draw pile");
                    }
                    for lineage in &lineages {
                        let pos = combat_state
                            .zones
                            .draw_pile
                            .iter()
                            .position(|c| c.lineage == *lineage)
                            .expect("validated Omniscience selection must still exist");
                        let mut card = combat_state.zones.draw_pile.remove(pos);
                        card.exhaust_override = Some(true);
                        combat_state.queue_action_back(Action::EnqueueCardPlay {
                            item: Box::new(crate::runtime::combat::QueuedCardPlay {
                                card: card.clone(),
                                target: None,
                                energy_on_use: combat_state.turn.energy as i32,
                                ignore_energy_total: false,
                                autoplay: true,
                                random_target: true,
                                is_end_turn_autoplay: false,
                                purge_on_use: false,
                                source: crate::runtime::combat::QueuedCardSource::Normal,
                            }),
                            in_front: false,
                        });
                        for _ in 1..play_amount {
                            let copy = card.make_stat_equivalent_copy_with_lineage(
                                combat_state.allocate_combat_generated_lineage(),
                            );
                            combat_state.observe_omniscience_copy(&card, &copy);
                            combat_state.queue_action_back(Action::EnqueueCardPlay {
                                item: Box::new(crate::runtime::combat::QueuedCardPlay {
                                    card: copy,
                                    target: None,
                                    energy_on_use: combat_state.turn.energy as i32,
                                    ignore_energy_total: false,
                                    autoplay: true,
                                    random_target: true,
                                    is_end_turn_autoplay: false,
                                    purge_on_use: true,
                                    source: crate::runtime::combat::QueuedCardSource::Normal,
                                }),
                                in_front: false,
                            });
                        }
                    }
                }
            }

            combat_state.emit_event(DomainEvent::SelectionResolved {
                scope: SelectionScope::Grid,
                reason: reason.into(),
                selected: lineages
                    .iter()
                    .copied()
                    .map(SelectionTargetRef::CardLineage)
                    .collect(),
                source: DomainEventSource::Selection(reason.into()),
            });
            *engine_state = EngineState::CombatProcessing;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_grid_select, handle_hand_select, handle_scry};
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{
        ClientInput, EngineState, GridSelectReason, HandSelectReason, PileType,
    };
    use crate::test_support::blank_test_combat;

    #[test]
    fn scry_discards_selected_top_candidates_by_index() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
                cards: vec![CardId::Strike, CardId::Defend],
                card_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(1),
                    crate::runtime::combat::CardLineageId::constructed_scenario(2),
                ],
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::Bash, 3),
        ])
        .into();

        handle_scry(
            &mut engine_state,
            &mut combat_state,
            2,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(1),
                crate::runtime::combat::CardLineageId::constructed_scenario(2),
            ],
            ClientInput::SubmitScryDiscard(vec![1]),
        )
        .expect("scry selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(
            combat_state
                .zones
                .draw_pile
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            vec![CardId::Strike, CardId::Bash]
        );
        assert_eq!(combat_state.zones.discard_pile.len(), 1);
        assert_eq!(combat_state.zones.discard_pile[0].id, CardId::Defend);
    }

    #[test]
    fn scry_rejects_duplicate_indices_without_mutating_piles() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
                cards: vec![CardId::Strike, CardId::Defend],
                card_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(1),
                    crate::runtime::combat::CardLineageId::constructed_scenario(2),
                ],
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ])
        .into();

        let result = handle_scry(
            &mut engine_state,
            &mut combat_state,
            2,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(1),
                crate::runtime::combat::CardLineageId::constructed_scenario(2),
            ],
            ClientInput::SubmitScryDiscard(vec![0, 0]),
        );

        assert!(result.is_err());
        assert_eq!(combat_state.zones.draw_pile.len(), 2);
        assert!(combat_state.zones.discard_pile.is_empty());
    }

    #[test]
    fn scry_rejects_stale_candidate_without_partial_mutation() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
                cards: vec![CardId::Strike, CardId::Defend],
                card_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(10),
                    crate::runtime::combat::CardLineageId::constructed_scenario(20),
                ],
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 10)]).into();

        let result = handle_scry(
            &mut engine_state,
            &mut combat_state,
            2,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(10),
                crate::runtime::combat::CardLineageId::constructed_scenario(20),
            ],
            ClientInput::SubmitScryDiscard(vec![0, 1]),
        );

        assert_eq!(result, Err("Scry candidate no longer in draw pile"));
        assert_eq!(
            combat_state
                .zones
                .draw_pile
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            vec![crate::test_support::scenario_lineage(10)]
        );
        assert!(combat_state.zones.discard_pile.is_empty());
    }

    #[test]
    fn hand_select_copy_matches_dual_wield_selected_card_replacement() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
                reason: HandSelectReason::Copy { amount: 3 },
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(10),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
            });
        let mut combat_state = blank_test_combat();
        let mut selected = CombatCard::for_scenario(CardId::Strike, 10);
        selected.upgrades = 1;
        selected.misc_value = 5;
        selected.base_damage_override = Some(16);
        selected.base_damage_mut = 99;
        selected.free_to_play_once = true;
        combat_state.zones.hand = vec![selected];
        for lineage in 11..20 {
            combat_state
                .zones
                .hand
                .push(CombatCard::for_scenario(CardId::Defend, lineage));
        }

        handle_hand_select(
            &mut engine_state,
            &mut combat_state,
            &[crate::runtime::combat::CardLineageId::constructed_scenario(
                10,
            )],
            1,
            true,
            false,
            HandSelectReason::Copy { amount: 3 },
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Hand,
                    [10].map(crate::test_support::scenario_lineage),
                ),
            ),
        )
        .expect("copy selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(
            combat_state
                .zones
                .combat_lineage_allocator
                .next_creation_ordinal(),
            4
        );
        assert_eq!(combat_state.zones.hand.len(), 10);
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .filter(|c| c.lineage
                    == crate::runtime::combat::CardLineageId::constructed_scenario(10))
                .count(),
            0,
            "Java hand select removes the selected original before DualWieldAction creates copies"
        );
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .filter(|card| card.id == CardId::Strike)
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            vec![crate::runtime::combat::CardLineageId::combat_generated(1)],
            "one replacement copy fits in hand after the selected original is removed"
        );
        assert_eq!(combat_state.zones.discard_pile.len(), 2);
        assert_eq!(
            combat_state
                .zones
                .discard_pile
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            vec![
                (
                    CardId::Strike,
                    crate::runtime::combat::CardLineageId::combat_generated(2),
                ),
                (
                    CardId::Strike,
                    crate::runtime::combat::CardLineageId::combat_generated(3),
                ),
            ]
        );
        let hand_copy = combat_state
            .zones
            .hand
            .iter()
            .find(|card| card.id == CardId::Strike)
            .expect("one Dual Wield replacement copy should fit in hand");
        assert_eq!(hand_copy.upgrades, 1);
        assert_eq!(hand_copy.misc_value, 5);
        assert_eq!(hand_copy.base_damage_override, Some(16));
        assert_eq!(
            hand_copy.base_damage_mut, 16,
            "Java hand insertion applies powers after makeStatEquivalentCopy resets rendered damage"
        );
        assert!(hand_copy.free_to_play_once);

        for copy in combat_state.zones.discard_pile.iter() {
            assert_eq!(copy.upgrades, 1);
            assert_eq!(copy.misc_value, 5);
            assert_eq!(copy.base_damage_override, Some(16));
            assert_eq!(
                copy.base_damage_mut, 0,
                "Java DualWieldAction overflow discard keeps the reset stat-equivalent source card, not the hand-applied rendered damage"
            );
            assert!(copy.free_to_play_once);
        }
    }

    #[test]
    fn hand_select_copy_returns_non_candidates_before_dual_wield_copies() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
                reason: HandSelectReason::Copy { amount: 2 },
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(20),
                    crate::runtime::combat::CardLineageId::constructed_scenario(30),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 10),
            CombatCard::for_scenario(CardId::Strike, 20),
            CombatCard::for_scenario(CardId::Inflame, 30),
            CombatCard::for_scenario(CardId::Defend, 40),
        ];

        handle_hand_select(
            &mut engine_state,
            &mut combat_state,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(20),
                crate::runtime::combat::CardLineageId::constructed_scenario(30),
            ],
            1,
            true,
            false,
            HandSelectReason::Copy { amount: 2 },
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Hand,
                    vec![crate::test_support::scenario_lineage(20)],
                ),
            ),
        )
        .expect("copy selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Inflame, crate::test_support::scenario_lineage(30)),
                (CardId::Defend, crate::test_support::scenario_lineage(10)),
                (CardId::Defend, crate::test_support::scenario_lineage(40)),
                (
                    CardId::Strike,
                    crate::runtime::combat::CardLineageId::combat_generated(1),
                ),
                (
                    CardId::Strike,
                    crate::runtime::combat::CardLineageId::combat_generated(2),
                ),
            ],
            "Java DualWieldAction removes non-candidates before selection, returns them, then queued MakeTempCardInHandAction copies resolve"
        );
    }

    #[test]
    fn hand_select_upgrade_matches_armaments_screen_order() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
                reason: HandSelectReason::Upgrade,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(20),
                    crate::runtime::combat::CardLineageId::constructed_scenario(30),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
            });
        let mut combat_state = blank_test_combat();
        let mut already_upgraded = CombatCard::for_scenario(CardId::Strike, 10);
        already_upgraded.upgrades = 1;
        combat_state.zones.hand = vec![
            already_upgraded,
            CombatCard::for_scenario(CardId::Defend, 20),
            CombatCard::for_scenario(CardId::Bash, 30),
            CombatCard::for_scenario(CardId::Wound, 40),
        ];

        handle_hand_select(
            &mut engine_state,
            &mut combat_state,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(20),
                crate::runtime::combat::CardLineageId::constructed_scenario(30),
            ],
            1,
            true,
            false,
            HandSelectReason::Upgrade,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Hand,
                    vec![crate::test_support::scenario_lineage(20)],
                ),
            ),
        )
        .expect("upgrade selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| (card.id, card.lineage, card.upgrades))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Bash, crate::test_support::scenario_lineage(30), 0),
                (CardId::Defend, crate::test_support::scenario_lineage(20), 1),
                (CardId::Strike, crate::test_support::scenario_lineage(10), 1),
                (CardId::Wound, crate::test_support::scenario_lineage(40), 0),
            ],
            "Java ArmamentsAction removes non-upgradeable cards before selection, then addToTop returns the selected card and non-upgradeables"
        );
    }

    #[test]
    fn hand_select_retain_marks_selected_cards_for_one_turn_retain() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
                reason: HandSelectReason::Retain,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(10),
                    crate::runtime::combat::CardLineageId::constructed_scenario(20),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 20),
        ];

        handle_hand_select(
            &mut engine_state,
            &mut combat_state,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(10),
                crate::runtime::combat::CardLineageId::constructed_scenario(20),
            ],
            1,
            true,
            false,
            HandSelectReason::Retain,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Hand,
                    vec![crate::test_support::scenario_lineage(20)],
                ),
            ),
        )
        .expect("retain selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| (card.id, card.lineage, card.retain_override))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Strike, crate::test_support::scenario_lineage(10), None),
                (CardId::Defend, crate::test_support::scenario_lineage(20), Some(true)),
            ],
            "Java RetainCardsAction/MeditateAction marks only the selected cards with one-turn retain"
        );
    }

    #[test]
    fn hand_select_forethought_uses_combat_cost_not_turn_cost_for_free_once() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
                reason: HandSelectReason::PutToBottomOfDraw,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(10),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
            });
        let mut combat_state = blank_test_combat();
        let mut temporarily_free_defend = CombatCard::for_scenario(CardId::Defend, 10);
        temporarily_free_defend.set_cost_for_turn_java(0);
        combat_state.zones.hand = vec![temporarily_free_defend];

        handle_hand_select(
            &mut engine_state,
            &mut combat_state,
            &[crate::runtime::combat::CardLineageId::constructed_scenario(
                10,
            )],
            1,
            true,
            false,
            HandSelectReason::PutToBottomOfDraw,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Hand,
                    vec![crate::test_support::scenario_lineage(10)],
                ),
            ),
        )
        .expect("Forethought-style selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert!(combat_state.zones.hand.is_empty());
        assert_eq!(combat_state.zones.draw_pile.len(), 1);
        let moved = &combat_state.zones.draw_pile[0];
        assert_eq!(
            moved.lineage,
            crate::runtime::combat::CardLineageId::constructed_scenario(10)
        );
        assert!(
            moved.free_to_play_once,
            "Java ForethoughtAction checks AbstractCard.cost > 0, so a card made free only for this turn still becomes free once after redraw"
        );
    }

    #[test]
    fn grid_select_rejects_duplicate_candidate_without_mutation() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
                source_pile: PileType::Discard,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(10),
                ],
                min_cards: 1,
                max_cards: 2,
                can_cancel: false,
                reason: GridSelectReason::MoveToDrawPile,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Strike, 10)].into();

        let result = handle_grid_select(
            &mut engine_state,
            &mut combat_state,
            &[crate::runtime::combat::CardLineageId::constructed_scenario(
                10,
            )],
            PileType::Discard,
            1,
            2,
            false,
            GridSelectReason::MoveToDrawPile,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    [10, 10].map(crate::test_support::scenario_lineage).to_vec(),
                ),
            ),
        );

        assert_eq!(result, Err("Duplicate grid selection"));
        assert_eq!(
            combat_state
                .zones
                .discard_pile
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            vec![crate::test_support::scenario_lineage(10)]
        );
        assert!(combat_state.zones.draw_pile.is_empty());
    }

    #[test]
    fn discard_to_hand_selection_leaves_card_when_hand_is_full() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
                source_pile: PileType::Discard,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(20),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: GridSelectReason::DiscardToHand,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.hand = (0..10)
            .map(|idx| CombatCard::for_scenario(CardId::Defend, 100 + idx))
            .collect();
        combat_state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Strike, 20)].into();

        handle_grid_select(
            &mut engine_state,
            &mut combat_state,
            &[crate::runtime::combat::CardLineageId::constructed_scenario(
                20,
            )],
            PileType::Discard,
            1,
            1,
            false,
            GridSelectReason::DiscardToHand,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    vec![crate::test_support::scenario_lineage(20)],
                ),
            ),
        )
        .expect("full hand still resolves like Java BetterDiscardPileToHandAction");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(combat_state.zones.hand.len(), 10);
        assert_eq!(
            combat_state
                .zones
                .discard_pile
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            vec![crate::test_support::scenario_lineage(20)],
            "Java leaves selected discard cards in discard when the hand is full"
        );
    }

    #[test]
    fn discard_to_hand_no_cost_change_preserves_cost_for_hologram_style_selection() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
                source_pile: PileType::Discard,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(21),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: GridSelectReason::DiscardToHandNoCostChange,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Bash, 21)].into();

        handle_grid_select(
            &mut engine_state,
            &mut combat_state,
            &[crate::runtime::combat::CardLineageId::constructed_scenario(
                21,
            )],
            PileType::Discard,
            1,
            1,
            false,
            GridSelectReason::DiscardToHandNoCostChange,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    vec![crate::test_support::scenario_lineage(21)],
                ),
            ),
        )
        .expect("Hologram-style discard-to-hand selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert!(combat_state.zones.discard_pile.is_empty());
        assert_eq!(combat_state.zones.hand.len(), 1);
        assert_eq!(combat_state.zones.hand[0].id, CardId::Bash);
        assert_eq!(
            combat_state.zones.hand[0].cost_for_turn_java(),
            2,
            "Hologram's BetterDiscardPileToHandAction constructor does not set a new cost"
        );
        assert_eq!(combat_state.zones.hand[0].cost_for_turn, None);
    }

    #[test]
    fn discard_to_hand_retain_marks_selected_card_for_meditate() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
                source_pile: PileType::Discard,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(22),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: GridSelectReason::DiscardToHandRetain,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Bash, 22)].into();

        handle_grid_select(
            &mut engine_state,
            &mut combat_state,
            &[crate::runtime::combat::CardLineageId::constructed_scenario(
                22,
            )],
            PileType::Discard,
            1,
            1,
            false,
            GridSelectReason::DiscardToHandRetain,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    vec![crate::test_support::scenario_lineage(22)],
                ),
            ),
        )
        .expect("Meditate-style discard-to-hand selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert!(combat_state.zones.discard_pile.is_empty());
        assert_eq!(combat_state.zones.hand.len(), 1);
        assert_eq!(combat_state.zones.hand[0].id, CardId::Bash);
        assert_eq!(combat_state.zones.hand[0].cost_for_turn_java(), 2);
        assert_eq!(combat_state.zones.hand[0].retain_override, Some(true));
    }

    #[test]
    fn deck_to_hand_selection_discards_selected_card_when_hand_is_full() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
                source_pile: PileType::Draw,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(30),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: GridSelectReason::AttackFromDeckToHand,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.hand = (0..10)
            .map(|idx| CombatCard::for_scenario(CardId::Defend, 200 + idx))
            .collect();
        combat_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 30)]).into();

        handle_grid_select(
            &mut engine_state,
            &mut combat_state,
            &[crate::runtime::combat::CardLineageId::constructed_scenario(
                30,
            )],
            PileType::Draw,
            1,
            1,
            false,
            GridSelectReason::AttackFromDeckToHand,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    vec![crate::test_support::scenario_lineage(30)],
                ),
            ),
        )
        .expect("full hand should discard selected deck-to-hand card");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert!(combat_state.zones.draw_pile.is_empty());
        assert_eq!(
            combat_state
                .zones
                .discard_pile
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            vec![crate::test_support::scenario_lineage(30)],
            "Java SecretWeapon/SecretTechnique move selected draw-pile card to discard when hand is full"
        );
    }

    #[test]
    fn seek_draw_pile_to_hand_selection_moves_any_selected_cards() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
                source_pile: PileType::Draw,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(30),
                    crate::runtime::combat::CardLineageId::constructed_scenario(40),
                ],
                min_cards: 2,
                max_cards: 2,
                can_cancel: false,
                reason: GridSelectReason::DrawPileToHand,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Strike, 30),
            CombatCard::for_scenario(CardId::Defend, 40),
        ])
        .into();

        handle_grid_select(
            &mut engine_state,
            &mut combat_state,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(30),
                crate::runtime::combat::CardLineageId::constructed_scenario(40),
            ],
            PileType::Draw,
            2,
            2,
            false,
            GridSelectReason::DrawPileToHand,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    [40, 30].map(crate::test_support::scenario_lineage).to_vec(),
                ),
            ),
        )
        .expect("Seek should move arbitrary selected draw-pile cards to hand");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert!(combat_state.zones.draw_pile.is_empty());
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            [40, 30].map(crate::test_support::scenario_lineage).to_vec(),
            "Seek keeps the submitted selection order when resolving selected cards"
        );
    }

    #[test]
    fn recycle_hand_selection_gains_energy_and_exhausts_selected_card() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(20),
                    crate::runtime::combat::CardLineageId::constructed_scenario(21),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: HandSelectReason::Recycle,
            });
        let mut combat_state = blank_test_combat();
        let mut selected = CombatCard::for_scenario(CardId::Strike, 20);
        selected.update_cost_java(2);
        selected.set_cost_for_turn_java(0);
        combat_state.zones.hand = vec![selected, CombatCard::for_scenario(CardId::Defend, 21)];

        handle_hand_select(
            &mut engine_state,
            &mut combat_state,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(20),
                crate::runtime::combat::CardLineageId::constructed_scenario(21),
            ],
            1,
            true,
            false,
            HandSelectReason::Recycle,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Hand,
                    vec![crate::test_support::scenario_lineage(20)],
                ),
            ),
        )
        .expect("Recycle should resolve selected hand card");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            vec![crate::test_support::scenario_lineage(21)]
        );
        assert_eq!(
            combat_state.zones.exhaust_pile[0].lineage,
            crate::runtime::combat::CardLineageId::constructed_scenario(20)
        );
        assert_eq!(
            combat_state.pop_next_action(),
            None,
            "Recycle reads costForTurn, so a temporarily free card gives no energy even if combat cost is positive"
        );
    }

    #[test]
    fn deck_to_hand_selection_rejects_stale_candidate_without_partial_mutation() {
        let mut engine_state =
            EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
                source_pile: PileType::Draw,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(40),
                    crate::runtime::combat::CardLineageId::constructed_scenario(50),
                ],
                min_cards: 2,
                max_cards: 2,
                can_cancel: false,
                reason: GridSelectReason::SkillFromDeckToHand,
            });
        let mut combat_state = blank_test_combat();
        combat_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Defend, 40)]).into();

        let result = handle_grid_select(
            &mut engine_state,
            &mut combat_state,
            &[
                crate::runtime::combat::CardLineageId::constructed_scenario(40),
                crate::runtime::combat::CardLineageId::constructed_scenario(50),
            ],
            PileType::Draw,
            2,
            2,
            false,
            GridSelectReason::SkillFromDeckToHand,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    [40, 50].map(crate::test_support::scenario_lineage).to_vec(),
                ),
            ),
        );

        assert_eq!(result, Err("Grid candidate no longer in draw pile"));
        assert_eq!(
            combat_state
                .zones
                .draw_pile
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            vec![crate::test_support::scenario_lineage(40)]
        );
        assert!(combat_state.zones.hand.is_empty());
        assert!(combat_state.zones.discard_pile.is_empty());
    }
}
