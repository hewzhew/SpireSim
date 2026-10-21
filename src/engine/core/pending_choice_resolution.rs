use crate::engine::pending_choices;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::state::core::{ClientInput, EngineState, PendingChoice};

use super::discovery;

pub(super) fn resolve_pending_choice(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
    input: ClientInput,
) -> Result<(), &'static str> {
    let choice = if let EngineState::PendingChoice(c) = engine_state {
        c.clone()
    } else {
        return Err("Not in a pending choice state");
    };

    match choice {
        PendingChoice::ScrySelect {
            cards,
            card_lineages,
        } => pending_choices::handle_scry(
            engine_state,
            combat_state,
            cards.len(),
            &card_lineages,
            input,
        ),
        PendingChoice::HandSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel: cancellable,
            reason,
        } => pending_choices::handle_hand_select(
            engine_state,
            combat_state,
            &candidate_lineages,
            max_cards as usize,
            min_cards == max_cards,
            cancellable,
            reason,
            input,
        ),
        PendingChoice::GridSelect {
            source_pile,
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        } => pending_choices::handle_grid_select(
            engine_state,
            combat_state,
            &candidate_lineages,
            source_pile,
            min_cards,
            max_cards,
            can_cancel,
            reason,
            input,
        ),
        PendingChoice::DiscoverySelect(ref choice) => {
            // Player picks one card from the discovery options
            let choice = choice.clone();
            match input {
                ClientInput::SubmitDiscoverChoice(idx) if idx < choice.cards.len() => {
                    // Java DiscoveryAction.update() calls generateCardChoices()
                    // before checking whether the screen returned a selected
                    // discoveryCard, so resuming the action burns one more
                    // unused set of random choices.
                    let _ = discovery::generate_discovery_choices(
                        combat_state,
                        choice.colorless,
                        choice.card_type,
                    );
                    let card_id = choice.cards[idx];
                    let amount = choice.amount.max(1);
                    let cost_for_turn = combat_state.turn.take_discovery_cost_for_turn();
                    let initial_hand_len = combat_state.zones.hand.len();
                    let hand_copies =
                        (10usize.saturating_sub(initial_hand_len)).min(amount as usize);

                    for copy_idx in 0..amount as usize {
                        let lineage = combat_state.allocate_combat_generated_lineage();
                        let mut card = crate::content::cards::make_fresh_card_copy_for_combat(
                            card_id,
                            lineage,
                            combat_state,
                        );
                        let enters_hand = copy_idx < hand_copies;
                        let master_reality_call_sites = if enters_hand { 2 } else { 1 };
                        crate::content::cards::apply_master_reality_to_generated_card(
                            &mut card,
                            combat_state,
                            master_reality_call_sites,
                        );
                        // Java DiscoveryAction applies setCostForTurn(0) after
                        // Master Reality upgrades the generated copies.
                        if let Some(cost) = cost_for_turn {
                            card.set_cost_for_turn_java(cost as i32);
                        }
                        if enters_hand {
                            if crate::content::powers::store::has_power(
                                combat_state,
                                0,
                                crate::content::powers::PowerId::Corruption,
                            ) {
                                crate::content::cards::ironclad::corruption::corruption_on_card_draw(
                                    combat_state,
                                    &mut card,
                                );
                            }
                            crate::content::cards::evaluate_card(&mut card, combat_state, None);
                            combat_state.add_card_to_hand(card);
                        } else {
                            combat_state.add_card_to_discard_pile_top(card);
                        }
                    }
                    *engine_state = EngineState::CombatProcessing;
                    Ok(())
                }
                ClientInput::Cancel if choice.can_skip => {
                    let _ = discovery::generate_discovery_choices(
                        combat_state,
                        choice.colorless,
                        choice.card_type,
                    );
                    let _ = combat_state.turn.take_discovery_cost_for_turn();
                    *engine_state = EngineState::CombatProcessing;
                    Ok(())
                }
                _ => Err("Invalid discovery choice"),
            }
        }
        PendingChoice::CardRewardSelect {
            ref cards,
            destination,
            can_skip,
        } => {
            // Player picks one card from the reward options, or Cancel if can_skip
            match input {
                ClientInput::SubmitDiscoverChoice(idx) => {
                    if idx < cards.len() {
                        let card_id = cards[idx];
                        let lineage = combat_state.allocate_combat_generated_lineage();
                        let mut card = crate::content::cards::make_fresh_card_copy_for_combat(
                            card_id,
                            lineage,
                            combat_state,
                        );
                        match destination {
                            crate::runtime::action::CardDestination::Hand => {
                                // Java ChooseOneColorless: hand (or discard if full)
                                if combat_state.zones.hand.len() < 10 {
                                    crate::content::cards::apply_master_reality_to_generated_card(
                                        &mut card,
                                        combat_state,
                                        2,
                                    );
                                    if crate::content::powers::store::has_power(
                                        combat_state,
                                        0,
                                        crate::content::powers::PowerId::Corruption,
                                    ) {
                                        crate::content::cards::ironclad::corruption::corruption_on_card_draw(
                                            combat_state,
                                            &mut card,
                                        );
                                    }
                                    crate::content::cards::evaluate_card(
                                        &mut card,
                                        combat_state,
                                        None,
                                    );
                                    combat_state.add_card_to_hand(card);
                                } else {
                                    crate::content::cards::apply_master_reality_to_generated_card(
                                        &mut card,
                                        combat_state,
                                        1,
                                    );
                                    combat_state.add_card_to_discard_pile_top(card);
                                }
                            }
                            crate::runtime::action::CardDestination::DrawPileRandom => {
                                // Java CodexAction: add to draw pile at random position
                                crate::content::cards::apply_master_reality_to_generated_card(
                                    &mut card,
                                    combat_state,
                                    1,
                                );
                                combat_state.add_card_to_draw_pile_random_spot(card);
                            }
                        }
                        *engine_state = EngineState::CombatProcessing;
                        Ok(())
                    } else {
                        Err("Invalid card reward choice index")
                    }
                }
                ClientInput::Cancel if can_skip => {
                    // Java CodexAction: canSkip=true -> player can skip picking.
                    *engine_state = EngineState::CombatProcessing;
                    Ok(())
                }
                _ => Err("Invalid input for card reward selection"),
            }
        }
        PendingChoice::ForeignInfluenceSelect {
            ref cards,
            upgraded,
        } => {
            if let ClientInput::SubmitDiscoverChoice(idx) = input {
                let Some(&card_id) = cards.get(idx) else {
                    return Err("Invalid foreign influence choice index");
                };
                discovery::add_foreign_influence_choice_to_zone(combat_state, card_id, upgraded);
                *engine_state = EngineState::CombatProcessing;
                Ok(())
            } else {
                Err("Expected SubmitDiscoverChoice for foreign influence selection")
            }
        }
        PendingChoice::ChooseOneSelect { ref choices } => {
            if let ClientInput::SubmitDiscoverChoice(idx) = input {
                let Some(choice) = choices.get(idx).copied() else {
                    return Err("Invalid choose-one choice index");
                };
                let actions = crate::content::cards::resolve_choose_one_option(
                    choice.card_id,
                    choice.upgrades,
                    combat_state,
                );
                combat_state.queue_actions(actions);
                *engine_state = EngineState::CombatProcessing;
                Ok(())
            } else {
                Err("Expected SubmitDiscoverChoice for choose-one selection")
            }
        }
        PendingChoice::StanceChoice => {
            // Player picks 0=Wrath, 1=Calm
            if let ClientInput::SubmitDiscoverChoice(idx) = input {
                let stance = match idx {
                    0 => "Wrath",
                    1 => "Calm",
                    _ => return Err("Invalid stance choice (expected 0=Wrath or 1=Calm)"),
                };
                combat_state.queue_action_back(Action::EnterStance(stance.to_string()));
                *engine_state = EngineState::CombatProcessing;
                Ok(())
            } else {
                Err("Expected SubmitDiscoverChoice for stance selection")
            }
        }
    }
}

pub(super) fn hand_select_candidates(
    combat_state: &CombatState,
    filter: crate::state::HandSelectFilter,
) -> Vec<crate::runtime::combat::CardLineageId> {
    combat_state
        .zones
        .hand
        .iter()
        .filter(|card| hand_candidate_matches(card, filter))
        .map(|card| card.lineage)
        .collect()
}

fn hand_candidate_matches(
    card: &crate::runtime::combat::CombatCard,
    filter: crate::state::HandSelectFilter,
) -> bool {
    let def = crate::content::cards::get_card_definition(card.id);
    match filter {
        crate::state::HandSelectFilter::Any => true,
        crate::state::HandSelectFilter::Upgradeable => {
            (card.id == crate::content::cards::CardId::SearingBlow || card.upgrades == 0)
                && def.card_type != crate::content::cards::CardType::Status
                && def.card_type != crate::content::cards::CardType::Curse
        }
        crate::state::HandSelectFilter::AttackOrPower => {
            matches!(
                def.card_type,
                crate::content::cards::CardType::Attack | crate::content::cards::CardType::Power
            )
        }
    }
}

pub(super) fn hand_select_can_fizzle_when_empty(reason: crate::state::HandSelectReason) -> bool {
    matches!(
        reason,
        crate::state::HandSelectReason::Discard
            | crate::state::HandSelectReason::Exhaust
            | crate::state::HandSelectReason::PotionExhaust { .. }
            | crate::state::HandSelectReason::PutOnDrawPile
            | crate::state::HandSelectReason::Setup
            | crate::state::HandSelectReason::PutToBottomOfDraw
            | crate::state::HandSelectReason::Nightmare { .. }
            | crate::state::HandSelectReason::Recycle
    )
}

pub(super) fn grid_select_candidates(
    combat_state: &mut CombatState,
    source_pile: crate::state::PileType,
    filter: crate::state::GridSelectFilter,
    reason: crate::state::GridSelectReason,
) -> Vec<crate::runtime::combat::CardLineageId> {
    match reason {
        crate::state::GridSelectReason::Exhume { .. }
            if source_pile == crate::state::PileType::Exhaust
                && filter == crate::state::GridSelectFilter::NonExhume =>
        {
            // Java holds excluded Exhumes aside, then appends them on return.
            // Keep that projected order during the choice too: the Java
            // adapter explicitly rejoins its observed held cards at the tail.
            // Stable sorting preserves each group's original order.
            combat_state
                .zones
                .exhaust_pile
                .sort_by_key(|card| card.id == crate::content::cards::CardId::Exhume);
        }
        crate::state::GridSelectReason::DrawPileToHand
            if source_pile == crate::state::PileType::Draw
                && filter == crate::state::GridSelectFilter::Any =>
        {
            return java_better_draw_pile_to_hand_candidates(combat_state);
        }
        crate::state::GridSelectReason::Omniscience { .. }
            if source_pile == crate::state::PileType::Draw
                && filter == crate::state::GridSelectFilter::Any =>
        {
            return java_better_draw_pile_to_hand_candidates(combat_state);
        }
        crate::state::GridSelectReason::SkillFromDeckToHand
            if source_pile == crate::state::PileType::Draw
                && filter == crate::state::GridSelectFilter::Skill =>
        {
            return java_deck_to_hand_type_candidates(
                combat_state,
                crate::content::cards::CardType::Skill,
            );
        }
        crate::state::GridSelectReason::AttackFromDeckToHand
            if source_pile == crate::state::PileType::Draw
                && filter == crate::state::GridSelectFilter::Attack =>
        {
            return java_deck_to_hand_type_candidates(
                combat_state,
                crate::content::cards::CardType::Attack,
            );
        }
        _ => {}
    }

    let pile = match source_pile {
        crate::state::PileType::Draw => {
            crate::runtime::combat::CardPileView::Contiguous(combat_state.zones.draw_pile.as_ref())
        }
        crate::state::PileType::Discard => {
            crate::runtime::combat::CardPileView::Discard(&combat_state.zones.discard_pile)
        }
        crate::state::PileType::Exhaust => crate::runtime::combat::CardPileView::Contiguous(
            combat_state.zones.exhaust_pile.as_slice(),
        ),
        crate::state::PileType::Hand => {
            crate::runtime::combat::CardPileView::Contiguous(&combat_state.zones.hand)
        }
        crate::state::PileType::Limbo => {
            crate::runtime::combat::CardPileView::Contiguous(&combat_state.zones.limbo)
        }
        crate::state::PileType::MasterDeck => crate::runtime::combat::CardPileView::Contiguous(&[]),
    };

    pile.iter()
        .filter(|card| grid_candidate_matches(card, filter))
        .map(|card| card.lineage)
        .collect()
}

fn java_better_draw_pile_to_hand_candidates(
    combat_state: &CombatState,
) -> Vec<crate::runtime::combat::CardLineageId> {
    let mut cards: Vec<&crate::runtime::combat::CombatCard> =
        combat_state.zones.draw_pile.iter().rev().collect();

    cards.sort_by(|a, b| {
        let a_name = crate::content::cards::get_card_definition(a.id).name;
        let b_name = crate::content::cards::get_card_definition(b.id).name;
        a_name.cmp(b_name)
    });
    cards.sort_by(|a, b| {
        let a_rarity =
            java_card_rarity_ordinal(crate::content::cards::get_card_definition(a.id).rarity);
        let b_rarity =
            java_card_rarity_ordinal(crate::content::cards::get_card_definition(b.id).rarity);
        b_rarity.cmp(&a_rarity)
    });
    cards.sort_by(|a, b| {
        let a_status = crate::content::cards::get_card_definition(a.id).card_type
            == crate::content::cards::CardType::Status;
        let b_status = crate::content::cards::get_card_definition(b.id).card_type
            == crate::content::cards::CardType::Status;
        a_status.cmp(&b_status)
    });

    cards.into_iter().map(|card| card.lineage).collect()
}

fn java_deck_to_hand_type_candidates(
    combat_state: &mut CombatState,
    card_type: crate::content::cards::CardType,
) -> Vec<crate::runtime::combat::CardLineageId> {
    let matching_lineages: Vec<crate::runtime::combat::CardLineageId> = combat_state
        .zones
        .draw_pile
        .iter()
        .rev()
        .filter(|card| crate::content::cards::get_card_definition(card.id).card_type == card_type)
        .map(|card| card.lineage)
        .collect();

    let mut candidates = Vec::new();
    for lineage in matching_lineages {
        if candidates.is_empty() {
            candidates.push(lineage);
        } else {
            let index = combat_state
                .rng
                .card_random_rng
                .random(candidates.len() as i32 - 1) as usize;
            candidates.insert(index, lineage);
        }
    }
    candidates
}

fn java_card_rarity_ordinal(rarity: crate::content::cards::CardRarity) -> u8 {
    match rarity {
        crate::content::cards::CardRarity::Basic => 0,
        crate::content::cards::CardRarity::Special => 1,
        crate::content::cards::CardRarity::Common => 2,
        crate::content::cards::CardRarity::Uncommon => 3,
        crate::content::cards::CardRarity::Rare => 4,
        crate::content::cards::CardRarity::Curse => 5,
    }
}

fn grid_candidate_matches(
    card: &crate::runtime::combat::CombatCard,
    filter: crate::state::GridSelectFilter,
) -> bool {
    let def = crate::content::cards::get_card_definition(card.id);
    match filter {
        crate::state::GridSelectFilter::Any => true,
        crate::state::GridSelectFilter::NonExhume => {
            card.id != crate::content::cards::CardId::Exhume
        }
        crate::state::GridSelectFilter::Skill => {
            def.card_type == crate::content::cards::CardType::Skill
        }
        crate::state::GridSelectFilter::Attack => {
            def.card_type == crate::content::cards::CardType::Attack
        }
    }
}
