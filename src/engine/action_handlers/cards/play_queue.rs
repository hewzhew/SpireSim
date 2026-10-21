use super::exhaust::move_card_to_exhaust_pile;
use super::generated::{
    materialize_random_class_card_in_draw_pile_action,
    materialize_random_class_card_in_hand_action, materialize_random_colorless_card_in_hand_action,
};
use crate::content::cards::CardId;
use crate::content::powers::{store, PowerId};
use crate::engine::targeting;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState, QueuedCardPlay, QueuedCardSource};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct UseCardPlacementOverrides {
    rebound: bool,
}

/// Requests from the current after-use damage hooks, before mitigation and
/// the played card's final exhaust. This reads no future card or successor.
pub fn after_use_damage_requests(state: &CombatState) -> smallvec::SmallVec<[Action; 2]> {
    std::iter::once(0)
        .chain(state.entities.monsters.iter().map(|monster| monster.id))
        .flat_map(|owner| {
            store::powers_for(state, owner)
                .unwrap_or_default()
                .iter()
                .filter(|power| power.power_type == PowerId::BeatOfDeath)
                .flat_map(move |power| {
                    crate::content::powers::core::beat_of_death::on_after_use_card(
                        owner,
                        power.amount,
                    )
                })
        })
        .collect()
}

fn apply_use_card_after_use_hooks(
    card: &CombatCard,
    state: &mut CombatState,
) -> UseCardPlacementOverrides {
    let mut overrides = UseCardPlacementOverrides::default();
    let player_powers = crate::content::powers::store::powers_snapshot_for(state, 0);
    for power in player_powers {
        if power.power_type == PowerId::Rebound {
            overrides.rebound |=
                crate::content::powers::defect::rebound::on_after_use_card(state, card);
        }
    }
    // UseCardAction.update visits after-use hooks only after the card's own
    // effects have run. Exhaust effects such as Burning Pact may already have
    // appended Feel No Pain block. Beat of Death follows those effects, but
    // precedes the block from exhausting the played card in UseCardDone below.
    for action in after_use_damage_requests(state) {
        state.queue_action_back(action);
    }
    overrides
}

pub fn handle_use_card_after_use_hooks(mut card: CombatCard, state: &mut CombatState) {
    card.free_to_play_once = false;
    apply_use_card_after_use_hooks(&card, state);
    state.observe_autoplay_copy_purged(&card);
    resolve_early_end_turn_pending_after_card_use(state);
}

pub fn handle_use_card_done(
    should_exhaust: bool,
    trigger_after_use_hooks: bool,
    state: &mut CombatState,
) {
    if let Some(mut card) = state.zones.limbo.pop() {
        let placement_overrides = if trigger_after_use_hooks {
            apply_use_card_after_use_hooks(&card, state)
        } else {
            UseCardPlacementOverrides::default()
        };

        // Java UseCardAction clears this before moving the card to discard or
        // exhaust. Keeping it on a saved/discarded card makes later draws free.
        card.free_to_play_once = false;
        // `energyOnUse` belongs to the queued play, not to the card after that
        // play has settled. Java overwrites it from the next CardQueueItem;
        // retaining the old X value here creates a false stable-state field and
        // splits otherwise equivalent search states.
        card.energy_on_use = 0;
        // Java also clears exhaustOnUseOnce after choosing the destination.
        // `should_exhaust` already captured its effect for this play.
        card.exhaust_override = None;

        let def = crate::content::cards::get_card_definition(card.id);
        let spoon_saves_exhaust = should_exhaust
            && def.card_type != crate::content::cards::CardType::Power
            && state
                .entities
                .player
                .has_relic(crate::content::relics::RelicId::StrangeSpoon)
            && state.rng.card_random_rng.random_boolean();

        if should_exhaust && !spoon_saves_exhaust {
            move_card_to_exhaust_pile(card, state);
        } else {
            // CardGroup starts a Soul for these destinations. Soul.update()
            // calls clearPowers/resetAttributes before the next settled player
            // decision, including cards saved by Strange Spoon. After-use hooks
            // above still observe the played card's temporary attributes.
            card.reset_attributes_java();
            if placement_overrides.rebound {
                state.add_card_to_draw_pile_top(card);
            } else if crate::content::cards::shuffle_back_into_draw_pile_when_played(&card) {
                state.add_card_to_draw_pile_random_spot(card);
            } else {
                super::discard::move_card_to_discard_pile(card, state);
            }
        }
    }

    resolve_early_end_turn_pending_after_card_use(state);
}

fn resolve_early_end_turn_pending_after_card_use(state: &mut CombatState) {
    if state.turn.counters.early_end_turn_pending {
        state.turn.clear_early_end_turn_pending();
        state.begin_turn_transition();
        state.queue_action_back(Action::EndTurnTrigger);
    }
}

pub fn handle_queue_early_end_turn(state: &mut CombatState) {
    let queued_cards: Vec<_> = state.zones.queued_cards.drain(..).collect();
    for queued in queued_cards.into_iter().rev() {
        if queued.autoplay && !queued.purge_on_use {
            let should_exhaust = queued
                .card
                .exhaust_override
                .unwrap_or(crate::content::cards::exhausts_when_played(&queued.card));
            state.add_card_to_limbo(queued.card);
            state.queue_action_back(Action::UseCardDone {
                should_exhaust,
                trigger_after_use_hooks: false,
            });
        }
    }
    state.turn.mark_early_end_turn_pending();
}

pub fn handle_skip_enemies_turn(state: &mut CombatState) {
    state.turn.mark_skip_monster_turn_pending();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardPlaySource {
    Hand,
    Direct,
}

fn execute_played_card(
    mut played_card: CombatCard,
    target: Option<usize>,
    purge: bool,
    source: CardPlaySource,
    state: &mut CombatState,
) {
    let card_id = played_card.id;
    let def = crate::content::cards::get_card_definition(card_id);

    crate::content::cards::evaluate_card(&mut played_card, state, target);

    let mut card_actions = crate::content::cards::resolve_card_play_with_context(
        card_id,
        state,
        &played_card,
        target,
        crate::content::cards::CardUseContext {
            played_from_hand: source == CardPlaySource::Hand,
        },
    );
    if card_id == CardId::Havoc || card_id == CardId::BouncingFlask {
        for action in &mut card_actions {
            match &mut action.action {
                Action::PlayTopCard { target, .. } | Action::BouncingFlask { target, .. }
                    if target.is_none() =>
                {
                    *target = targeting::pick_random_target(
                        state,
                        crate::state::TargetValidation::AnyEnemy,
                    );
                }
                _ => {}
            }
        }
    }
    for action in &mut card_actions {
        materialize_random_class_card_in_hand_action(&mut action.action, state);
        materialize_random_class_card_in_draw_pile_action(&mut action.action, state);
        materialize_random_colorless_card_in_hand_action(&mut action.action, state);
    }
    state.queue_actions(card_actions);

    let passive_card_actions = crate::content::cards::on_play_card(&played_card, state);
    state.queue_actions(passive_card_actions);
    // UseCardAction's constructor visits player powers before relics, then
    // cards and monster powers. Preserve the player's power order across both
    // dispatch forms: queued effects such as Hex must precede Ink Bottle draw.
    {
        let player_powers = crate::content::powers::store::powers_snapshot_for(state, 0);
        let mut exhaust_override = false;
        for power in &player_powers {
            let hook_actions = crate::content::powers::resolve_power_on_card_played(
                power.power_type,
                state,
                0,
                &played_card,
                power.amount,
            );
            for action in hook_actions {
                if power.power_type == PowerId::Anger {
                    state.queue_action_front(action);
                } else {
                    state.queue_action_back(action);
                }
            }
            use crate::content::powers::PowerId;
            match power.power_type {
                PowerId::DoubleTap
                | PowerId::DuplicationPower
                | PowerId::Amplify
                | PowerId::EchoForm
                | PowerId::Burst
                | PowerId::Corruption
                | PowerId::Heatsink
                | PowerId::PenNibPower
                | PowerId::Storm
                | PowerId::Vigor
                | PowerId::FreeAttackPower => {
                    crate::content::powers::resolve_power_on_use_card(
                        power.power_type,
                        state,
                        &played_card,
                        &mut exhaust_override,
                        purge,
                        target,
                    );
                }
                _ => {}
            }
        }
        if exhaust_override {
            played_card.exhaust_override = Some(true);
        }
    }

    let relic_actions = crate::content::relics::hooks::on_use_card(state, &played_card, target);
    state.queue_actions(relic_actions);
    crate::content::cards::trigger_cards_on_card_played(&played_card, state);

    let monster_ids: Vec<_> = state.entities.monsters.iter().map(|m| m.id).collect();
    for entity_id in monster_ids {
        for power in &store::powers_snapshot_for(state, entity_id) {
            let hook_actions = crate::content::powers::resolve_power_on_card_played(
                power.power_type,
                state,
                entity_id,
                &played_card,
                power.amount,
            );
            for action in hook_actions {
                // Native AngerPower.onUseCard uses addToTop. In particular,
                // its Strength must resolve before Survivor can ask to discard.
                if power.power_type == PowerId::Anger {
                    state.queue_action_front(action);
                } else {
                    state.queue_action_back(action);
                }
            }
        }
    }

    state.turn.record_card_played(card_id);
    if def.card_type == crate::content::cards::CardType::Attack {
        state.turn.increment_attacks_played();
    }

    let mut should_exhaust = played_card
        .exhaust_override
        .unwrap_or(crate::content::cards::exhausts_when_played(&played_card))
        || (def.card_type == crate::content::cards::CardType::Status
            && state
                .entities
                .player
                .has_relic(crate::content::relics::RelicId::MedicalKit))
        || (def.card_type == crate::content::cards::CardType::Curse
            && state
                .entities
                .player
                .has_relic(crate::content::relics::RelicId::BlueCandle));
    crate::content::cards::ironclad::corruption::corruption_on_use_card(
        state,
        &played_card,
        &mut should_exhaust,
    );

    if def.card_type != crate::content::cards::CardType::Power && !purge {
        state.add_card_to_limbo(played_card);
        state.queue_action_back(Action::UseCardDone {
            should_exhaust,
            trigger_after_use_hooks: true,
        });
    } else {
        state.queue_action_back(Action::UseCardAfterUseHooks {
            card: Box::new(played_card),
        });
    }
}

pub fn handle_play_card_from_hand(
    card_index: usize,
    target: Option<usize>,
    state: &mut CombatState,
) -> Result<(), &'static str> {
    if card_index >= state.zones.hand.len() {
        return Err("Card index out of range");
    }

    let card = &state.zones.hand[card_index];
    let card_id = card.id;
    let def = crate::content::cards::get_card_definition(card_id);

    crate::content::cards::can_play_card(card, state)?;

    let target = targeting::resolve_target_request(
        state,
        targeting::validation_for_card_target(crate::content::cards::effective_target(card)),
        target,
    )?;

    let base_cost = crate::content::cards::upgraded_base_cost_override(card).unwrap_or(def.cost);
    let free_attack_power_applies = def.card_type == crate::content::cards::CardType::Attack
        && crate::content::powers::store::power_amount(
            state,
            0,
            crate::content::powers::PowerId::FreeAttackPower,
        ) > 0;
    let effective_cost = if card.free_to_play_once || free_attack_power_applies {
        0
    } else if let Some(cft) = card.cost_for_turn {
        cft as i32
    } else {
        card.get_cost() as i32
    };

    let is_x_cost = base_cost == -1;
    let energy_on_use = if is_x_cost {
        state.turn.energy as i32
    } else {
        effective_cost.max(0)
    };

    if !is_x_cost && energy_on_use > state.turn.energy as i32 {
        return Err("Not enough energy");
    }

    crate::content::powers::core::surrounded::face_target_for_surrounded_if_needed(state, target);

    // AbstractCard.hasEnoughEnergy still checks the displayed cost above.
    // AbstractPlayer.useCard separately waives payment for Corruption skills,
    // even if Snecko Oil has overwritten their costForTurn since the last draw.
    // X-cost cards retain the energy handling owned by their own actions.
    let corruption_waives_payment = def.card_type == crate::content::cards::CardType::Skill
        && store::has_power(state, 0, PowerId::Corruption);
    if !is_x_cost && !corruption_waives_payment {
        state.turn.spend_energy(energy_on_use);
    }

    let card_mut = &mut state.zones.hand[card_index];
    if is_x_cost {
        card_mut.energy_on_use = energy_on_use;
    }

    {
        let mut card_copy = state.zones.hand[card_index].clone();
        crate::content::cards::evaluate_card(&mut card_copy, state, target);
        state.zones.hand[card_index] = card_copy;
    }

    let played_card = state.zones.hand.remove(card_index);
    execute_played_card(played_card, target, false, CardPlaySource::Hand, state);

    Ok(())
}

pub fn handle_enqueue_card_play(item: QueuedCardPlay, in_front: bool, state: &mut CombatState) {
    state.enqueue_card_play(item, in_front);
}

fn queued_card_target_fails_java_can_use(
    card: &CombatCard,
    target: Option<usize>,
    state: &CombatState,
) -> bool {
    if targeting::validation_for_card_target(crate::content::cards::effective_target(card))
        .is_none()
    {
        return false;
    }

    if state.are_monsters_basically_dead_java() {
        return true;
    }

    target.is_some_and(|target_id| {
        state
            .entities
            .monsters
            .iter()
            .find(|m| m.id == target_id)
            .is_some_and(|m| m.is_dying)
    })
}

fn queued_card_target_allows_java_use_card(
    card: &CombatCard,
    target: Option<usize>,
    state: &CombatState,
) -> bool {
    if targeting::validation_for_card_target(crate::content::cards::effective_target(card))
        .is_none()
    {
        return true;
    }

    let Some(target_id) = target else {
        return false;
    };

    state
        .entities
        .monsters
        .iter()
        .find(|m| m.id == target_id)
        .is_some_and(|m| !m.is_dead_or_escaped())
}

pub fn handle_flush_next_queued_card(state: &mut CombatState) {
    // Java GameActionManager.getNextAction drains actions before cardQueue,
    // including addToBottom consequences appended after this wake-up marker
    // (for example Dark Embrace's draw from Havoc's UseCardAction). Coalesce
    // stale markers so multiple pending plays cannot keep deferring each other.
    if state.has_pending_actions() {
        state
            .engine
            .retain(|action| !matches!(action, Action::FlushNextQueuedCard));
        state.queue_action_back(Action::FlushNextQueuedCard);
        return;
    }

    if state.zones.queued_cards.len() == 1
        && state
            .zones
            .queued_cards
            .front()
            .is_some_and(|queued| queued.is_end_turn_autoplay)
    {
        for relic in &mut state.entities.player.relics {
            if relic.id == crate::content::relics::RelicId::UnceasingTop {
                crate::content::relics::unceasing_top::disable_until_turn_ends(relic);
            }
        }
    }

    let Some(mut queued) = state.zones.queued_cards.pop_front() else {
        return;
    };

    let has_more_queued_cards = !state.zones.queued_cards.is_empty();
    if queued.source == QueuedCardSource::EndTurnTriggered {
        // Java stores a reference to the live card in hand. AbstractPlayer.useCard
        // removes that same instance before its payload resolves, and the later
        // UseCardAction places it in discard before the ordinary end-turn hand
        // discard. Preserve Regret's trigger-time hand-size snapshot from the
        // queued copy while retaining any other live card mutations.
        if let Some(position) = state
            .zones
            .hand
            .iter()
            .position(|card| card.lineage == queued.card.lineage)
        {
            let mut card = state.zones.hand.remove(position);
            if card.id == CardId::Regret {
                card.base_magic_num_mut = queued.card.base_magic_num_mut;
            }
            let payload = super::turn_triggers::resolve_end_turn_triggered_autoplay(state, &card);
            state.add_card_to_limbo(card);
            state.queue_action_back(Action::UseCardDone {
                should_exhaust: false,
                trigger_after_use_hooks: false,
            });
            if has_more_queued_cards {
                state.queue_action_back(Action::FlushNextQueuedCard);
            }
            state.queue_actions(payload);
        } else if has_more_queued_cards {
            state.queue_action_back(Action::FlushNextQueuedCard);
        }
        return;
    }

    queued.card.energy_on_use = queued.energy_on_use;
    let target = if queued.random_target {
        targeting::validation_for_card_target(crate::content::cards::effective_target(&queued.card))
            .and_then(|validation| targeting::pick_random_target(state, validation))
    } else {
        queued.target
    };

    if crate::content::cards::can_play_card_ignoring_energy(&queued.card, state).is_err()
        || queued_card_target_fails_java_can_use(&queued.card, target, state)
    {
        if queued.autoplay && !queued.purge_on_use {
            let should_exhaust = queued
                .card
                .exhaust_override
                .unwrap_or(crate::content::cards::exhausts_when_played(&queued.card));
            state.add_card_to_limbo(queued.card);
            state.queue_action_front(Action::UseCardDone {
                should_exhaust,
                trigger_after_use_hooks: false,
            });
        }
        if has_more_queued_cards {
            state.queue_action_back(Action::FlushNextQueuedCard);
        }
        return;
    }

    if has_more_queued_cards {
        state.queue_action_back(Action::FlushNextQueuedCard);
    }
    state.queue_action_front(Action::PlayCardDirect {
        card: Box::new(queued.card),
        target,
        purge: queued.purge_on_use,
    });
}

pub fn handle_play_card_direct(
    played_card: CombatCard,
    target: Option<usize>,
    purge: bool,
    state: &mut CombatState,
) {
    if !queued_card_target_allows_java_use_card(&played_card, target, state) {
        return;
    }
    execute_played_card(played_card, target, purge, CardPlaySource::Direct, state);
}

pub fn handle_play_top_card(target: Option<usize>, exhaust: bool, state: &mut CombatState) {
    let queued_random_target = target
        .or_else(|| targeting::pick_random_target(state, crate::state::TargetValidation::AnyEnemy));

    if state.zones.draw_pile.is_empty() {
        if state.zones.discard_pile.is_empty() {
            return;
        }
        state.queue_action_front(Action::PlayTopCard {
            target: queued_random_target,
            exhaust,
        });
        state.queue_action_front(Action::EmptyDeckShuffle);
        return;
    }

    let mut card = Box::new(
        state
            .draw_top_card()
            .expect("draw pile was checked non-empty before PlayTopCard"),
    );
    if crate::content::cards::get_card_definition(card.id).cost == -1 {
        card.energy_on_use = state.turn.energy as i32;
    }
    let resolved_target = if let Some(validation) =
        targeting::validation_for_card_target(crate::content::cards::effective_target(&card))
    {
        match queued_random_target {
            Some(explicit) => {
                targeting::resolve_target_request(state, Some(validation), Some(explicit))
                    .ok()
                    .flatten()
                    .or_else(|| targeting::pick_random_target(state, validation))
            }
            None => targeting::pick_random_target(state, validation),
        }
    } else {
        queued_random_target
    };

    if exhaust {
        card.exhaust_override = Some(true);
    }
    state.queue_action_front(Action::EnqueueCardPlay {
        item: Box::new(QueuedCardPlay {
            card: *card,
            target: resolved_target,
            energy_on_use: state.turn.energy as i32,
            ignore_energy_total: true,
            autoplay: true,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: false,
            source: QueuedCardSource::Normal,
        }),
        in_front: false,
    });
}

pub fn handle_queue_play_top_card_to_bottom(
    target: Option<usize>,
    exhaust: bool,
    state: &mut CombatState,
) {
    let resolved_target = target
        .or_else(|| targeting::pick_random_target(state, crate::state::TargetValidation::AnyEnemy));
    state.queue_action_back(Action::PlayTopCard {
        target: resolved_target,
        exhaust,
    });
}

#[cfg(test)]
mod tests {
    use crate::content::relics::{RelicId, RelicState};

    use super::*;

    fn settle_queued_plays(state: &mut CombatState) {
        let mut engine = crate::state::core::EngineState::CombatProcessing;
        for _ in 0..100 {
            if !matches!(engine, crate::state::core::EngineState::CombatProcessing) {
                assert!(state.zones.queued_cards.is_empty());
                assert!(state.zones.limbo.is_empty());
                return;
            }
            crate::engine::core::tick_engine(&mut engine, state, None);
        }
        panic!("queued cards and their consequences must settle");
    }

    #[test]
    fn nob_anger_resolves_before_survivor_opens_its_discard_choice() {
        use crate::state::{ClientInput, EngineState, SelectionResolution, SelectionScope};
        let mut state = crate::test_support::blank_test_combat();
        state.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::GremlinNob,
        )];
        for (power_id, amount) in [(PowerId::Anger, 3), (PowerId::Strength, 6)] {
            crate::engine::action_handlers::execute_action(
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id,
                    amount,
                },
                &mut state,
            );
        }
        let defend = CombatCard::for_scenario(CardId::DefendG, 732);
        let discarded = defend.lineage;
        state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Survivor, 731),
            defend,
            CombatCard::for_scenario(CardId::StrikeG, 733),
        ];
        let mut engine = EngineState::CombatPlayerTurn;
        assert!(crate::engine::core::tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::PlayCard {
                card_index: 0,
                target: None
            }
        ));
        assert!(matches!(engine, EngineState::PendingChoice(_)));
        assert_eq!(state.entities.player.block, 8);
        assert_eq!(store::power_amount(&state, 1, PowerId::Strength), 9);
        assert!(crate::engine::core::tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                [discarded]
            ))
        ));
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert_eq!(store::power_amount(&state, 1, PowerId::Strength), 9);
    }

    #[test]
    fn dual_wield_has_equal_net_copy_gain_with_and_without_selection() {
        use crate::state::{ClientInput, EngineState, SelectionResolution, SelectionScope};
        for upgrades in [0, 1] {
            for selection in [false, true] {
                let mut state = crate::test_support::blank_test_combat();
                state.entities.monsters = vec![crate::test_support::test_monster(
                    crate::content::monsters::EnemyId::JawWorm,
                )];
                state.turn.energy = 3;
                let mut dual_wield = CombatCard::for_scenario(CardId::DualWield, 711);
                dual_wield.upgrades = upgrades;
                let strike = CombatCard::for_scenario(CardId::Strike, 712);
                let original = strike.lineage;
                state.zones.hand = vec![dual_wield, strike];
                if selection {
                    state
                        .zones
                        .hand
                        .push(CombatCard::for_scenario(CardId::Inflame, 713));
                }
                let mut engine = EngineState::CombatPlayerTurn;
                assert!(crate::engine::core::tick_until_stable_turn(
                    &mut engine,
                    &mut state,
                    ClientInput::PlayCard {
                        card_index: 0,
                        target: None
                    }
                ));
                if selection {
                    assert!(matches!(engine, EngineState::PendingChoice(_)));
                    assert!(crate::engine::core::tick_until_stable_turn(
                        &mut engine,
                        &mut state,
                        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                            SelectionScope::Hand,
                            [original]
                        ))
                    ));
                }
                assert_eq!(engine, EngineState::CombatPlayerTurn);
                assert_eq!(
                    state
                        .zones
                        .hand
                        .iter()
                        .filter(|card| card.id == CardId::Strike)
                        .count(),
                    2 + usize::from(upgrades)
                );
                assert_eq!(
                    state.zones.hand.iter().any(|card| card.lineage == original),
                    !selection
                );
            }
        }
    }

    #[test]
    fn havoc_bypasses_fixed_costs_but_preserves_card_owned_x_cost_payment() {
        for upgrades in [0, 1] {
            for (card_id, free) in [
                (CardId::Bash, false),
                (CardId::Whirlwind, false),
                (CardId::Whirlwind, true),
            ] {
                let mut state = crate::test_support::blank_test_combat();
                let mut monster =
                    crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
                monster.id = 1;
                monster.current_hp = 100;
                monster.max_hp = 100;
                state.entities.monsters = vec![monster];
                state.turn.energy = 3;
                let mut havoc = CombatCard::for_scenario(CardId::Havoc, 701);
                havoc.upgrades = upgrades;
                state.zones.hand = vec![havoc];
                let mut top = CombatCard::for_scenario(card_id, 702);
                top.free_to_play_once = free;
                state.zones.draw_pile = vec![top].into();

                handle_play_card_from_hand(0, None, &mut state).unwrap();
                let remaining_energy = 2 + i32::from(upgrades);
                assert_eq!(state.turn.energy as i32, remaining_energy);
                settle_queued_plays(&mut state);

                let expected_energy = if card_id == CardId::Whirlwind && !free {
                    0
                } else {
                    remaining_energy
                };
                assert_eq!(state.turn.energy as i32, expected_energy, "{card_id:?}");
                let damage = if card_id == CardId::Whirlwind {
                    5 * remaining_energy
                } else {
                    8
                };
                assert_eq!(state.entities.monsters[0].current_hp, 100 - damage);
                assert_eq!(state.zones.exhaust_pile.len(), 1);
                assert_eq!(state.zones.exhaust_pile[0].id, card_id);
                assert!(!state.zones.exhaust_pile[0].free_to_play_once);
            }
        }
    }

    #[test]
    fn lethal_feed_finishes_summoner_suicides_before_exhaust_draw() {
        use crate::content::monsters::EnemyId;

        for (summoner, minion) in [
            (EnemyId::Reptomancer, EnemyId::SnakeDagger),
            (EnemyId::BronzeAutomaton, EnemyId::BronzeOrb),
            (EnemyId::TheCollector, EnemyId::TorchHead),
        ] {
            for lethal in [false, true] {
                let mut boss = crate::test_support::test_monster(summoner);
                boss.id = 1;
                boss.current_hp = if lethal { 1 } else { 100 };
                let mut summon = crate::test_support::test_monster(minion);
                summon.id = 2;
                let mut state = crate::test_support::combat_with_monsters(vec![boss, summon]);
                state.turn.energy = 3;
                state.entities.player.current_hp = 40;
                let max_hp_before = state.entities.player.max_hp;
                state.zones.hand = vec![CombatCard::for_scenario(CardId::Feed, 90001)];
                state.zones.draw_pile =
                    vec![CombatCard::for_scenario(CardId::Strike, 90002)].into();
                for power in [PowerId::DarkEmbrace, PowerId::Confusion] {
                    crate::engine::action_handlers::powers::handle_apply_power(
                        0, 0, power, 1, &mut state,
                    );
                }
                let card_rng_before = state.rng.card_random_rng.counter;

                handle_play_card_from_hand(0, Some(1), &mut state).unwrap();
                settle_queued_plays(&mut state);

                assert_eq!(state.entities.monsters[0].is_dying, lethal, "{summoner:?}");
                assert_eq!(state.entities.monsters[1].is_dying, lethal, "{summoner:?}");
                assert_eq!(
                    state.entities.player.max_hp,
                    max_hp_before + if lethal { 3 } else { 0 }
                );
                assert_eq!(
                    state.entities.player.current_hp,
                    40 + if lethal { 3 } else { 0 }
                );
                assert_eq!(state.zones.exhaust_pile[0].id, CardId::Feed);
                assert_eq!(
                    state.rng.card_random_rng.counter - card_rng_before,
                    if lethal { 0 } else { 1 },
                    "{summoner:?}: Java addToTop suicides precede Feed's onExhaust; a surviving summoner still permits Dark Embrace drawing through Confusion"
                );
                assert_eq!(state.zones.hand.len(), usize::from(!lethal));
                assert_eq!(state.zones.draw_pile.len(), usize::from(lethal));
            }
        }
    }

    fn beat_of_death_state(feel_no_pain: i32) -> CombatState {
        let mut state = crate::test_support::blank_test_combat();
        let mut heart =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::CorruptHeart);
        heart.id = 41;
        state.entities.monsters = vec![heart];
        state.entities.player.current_hp = 40;
        crate::engine::action_handlers::powers::handle_apply_power(
            41,
            41,
            PowerId::BeatOfDeath,
            2,
            &mut state,
        );
        if feel_no_pain > 0 {
            crate::engine::action_handlers::powers::handle_apply_power(
                0,
                0,
                PowerId::FeelNoPain,
                feel_no_pain,
                &mut state,
            );
        }
        state
    }

    #[test]
    fn corruption_waives_skill_payment_without_changing_the_energy_admission_check() {
        for active in [false, true] {
            for energy in [2, 3, 4] {
                for id in [CardId::Defend, CardId::Strike, CardId::Inflame] {
                    let mut state = crate::test_support::blank_test_combat();
                    let mut monster = crate::test_support::test_monster(
                        crate::content::monsters::EnemyId::JawWorm,
                    );
                    monster.id = 41;
                    state.entities.monsters = vec![monster];
                    state.turn.energy = energy;
                    if active {
                        crate::engine::action_handlers::powers::handle_apply_power(
                            0,
                            0,
                            PowerId::Corruption,
                            -1,
                            &mut state,
                        );
                    }
                    let mut card = CombatCard::for_scenario(id, 800);
                    if active && id == CardId::Defend {
                        card.modify_cost_for_combat_java(-9);
                    }
                    // Snecko Oil's RandomizeHandCost overwrites the turn cost
                    // after drawing, without setting freeToPlayOnce.
                    card.set_cost_for_turn_java(3);
                    state.zones.hand = vec![card];
                    let target = (id == CardId::Strike).then_some(41);
                    assert_eq!(
                        crate::content::cards::can_play_card(&state.zones.hand[0], &state).is_ok(),
                        energy >= 3
                    );
                    let result = handle_play_card_from_hand(0, target, &mut state);
                    if energy < 3 {
                        assert!(result.is_err());
                        assert_eq!(state.zones.hand[0].cost_for_turn_java(), 3);
                        assert_eq!(state.turn.energy, energy);
                        assert!(!state.has_pending_actions());
                        continue;
                    }
                    result.unwrap();
                    let waived = active && id == CardId::Defend;
                    assert_eq!(state.turn.energy, if waived { energy } else { energy - 3 });
                    settle_queued_plays(&mut state);
                    assert_eq!(state.turn.energy, if waived { energy } else { energy - 3 });
                    if waived {
                        assert_eq!(state.zones.exhaust_pile[0].id, CardId::Defend);
                        assert_eq!(state.entities.player.block, 5);
                    }
                }
            }
        }
    }

    #[test]
    fn beat_of_death_follows_exhausted_hand_card_hooks_before_played_card_exhaust() {
        use crate::state::{ClientInput, EngineState, SelectionResolution, SelectionScope};
        for selection in [false, true] {
            for corruption in [false, true] {
                for block in [0, 1, 3] {
                    let mut state = beat_of_death_state(block);
                    if corruption {
                        crate::engine::action_handlers::powers::handle_apply_power(
                            0,
                            0,
                            PowerId::Corruption,
                            -1,
                            &mut state,
                        );
                    }
                    let dazed = CombatCard::for_scenario(CardId::Dazed, 901);
                    let selected = dazed.lineage;
                    state.zones.hand =
                        vec![CombatCard::for_scenario(CardId::BurningPact, 900), dazed];
                    if selection {
                        state
                            .zones
                            .hand
                            .push(CombatCard::for_scenario(CardId::Wound, 902));
                    }
                    state.zones.draw_pile = vec![
                        CombatCard::for_scenario(CardId::Strike, 903),
                        CombatCard::for_scenario(CardId::Strike, 904),
                    ]
                    .into();
                    let mut engine = EngineState::CombatPlayerTurn;
                    assert!(crate::engine::core::tick_until_stable_turn(
                        &mut engine,
                        &mut state,
                        ClientInput::PlayCard {
                            card_index: 0,
                            target: None
                        }
                    ));
                    if selection {
                        assert!(matches!(engine, EngineState::PendingChoice(_)));
                        assert_eq!(state.entities.player.current_hp, 40);
                        assert_eq!(state.entities.player.block, 0);
                        assert!(crate::engine::core::tick_until_stable_turn(
                            &mut engine,
                            &mut state,
                            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                                SelectionScope::Hand,
                                [selected]
                            ))
                        ));
                    }
                    assert_eq!(engine, EngineState::CombatPlayerTurn);
                    assert_eq!(state.entities.player.current_hp, 40 - (2 - block).max(0));
                    assert_eq!(
                        state.entities.player.block,
                        (block - 2).max(0) + if corruption { block } else { 0 }
                    );
                    assert_eq!(
                        state.zones.exhaust_pile.len(),
                        if corruption { 2 } else { 1 }
                    );
                }
            }
        }
    }

    #[test]
    fn beat_of_death_precedes_feel_no_pain_from_the_played_card_itself() {
        let mut state = beat_of_death_state(3);
        state.zones.hand = vec![CombatCard::for_scenario(CardId::SeeingRed, 900)];
        handle_play_card_from_hand(0, None, &mut state).unwrap();
        settle_queued_plays(&mut state);
        assert_eq!(state.entities.player.current_hp, 38);
        assert_eq!(state.entities.player.block, 3);
        assert_eq!(state.zones.exhaust_pile[0].id, CardId::SeeingRed);
    }

    #[test]
    fn hex_inserts_before_ink_bottle_draws_on_the_tenth_card() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRng, StsRngCall};

        let mut state = crate::test_support::blank_test_combat();
        state.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::Chosen,
        )];
        let mut bottle = RelicState::new(RelicId::InkBottle);
        bottle.counter = 9;
        state.entities.player.add_relic(bottle);
        crate::engine::action_handlers::powers::handle_apply_power(
            0,
            0,
            PowerId::Hex,
            1,
            &mut state,
        );
        state.zones.hand = vec![CombatCard::for_scenario(CardId::SeeingRed, 900)];
        state.zones.draw_pile = [
            CardId::Strike,
            CardId::Defend,
            CardId::Bash,
            CardId::Anger,
            CardId::BodySlam,
            CardId::IronWave,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, 901 + i as u32))
        .collect();
        // Adjacent raw Java boundary from the Chosen counterexample. Its next
        // call inserts into six cards (inclusive range 5), yielding index 3.
        state.rng.card_random_rng = StsRng::from_state(0x67E5C8116F2EDE4C, 0xC7ACEF61D1706CC3, 13);
        let trace = begin_sts_rng_trace().unwrap();

        handle_play_card_from_hand(0, None, &mut state).unwrap();
        settle_queued_plays(&mut state);

        let calls: Vec<_> = trace
            .finish()
            .unwrap()
            .into_iter()
            .map(|e| e.call)
            .collect();
        assert_eq!(
            calls,
            [StsRngCall::RandomInt {
                range: 5,
                result: 3
            }]
        );
        assert_eq!(state.entities.player.relics[0].counter, 0);
        assert_eq!(state.zones.hand.len(), 1);
        assert_eq!(state.zones.hand[0].id, CardId::Strike);
        assert_eq!(
            state
                .zones
                .draw_pile
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            [
                CardId::Defend,
                CardId::Bash,
                CardId::Dazed,
                CardId::Anger,
                CardId::BodySlam,
                CardId::IronWave,
            ]
        );
        assert_eq!(state.zones.exhaust_pile[0].id, CardId::SeeingRed);
        assert_eq!(state.turn.energy, 4);
    }

    #[test]
    fn velvet_choker_havoc_checks_the_limit_again_when_autoplay_settles() {
        for (already_played, expected_block) in [(4, 5), (5, 0)] {
            let mut state = crate::test_support::blank_test_combat();
            state.entities.monsters = vec![crate::test_support::test_monster(
                crate::content::monsters::EnemyId::JawWorm,
            )];
            let mut choker = RelicState::new(RelicId::VelvetChoker);
            choker.counter = already_played;
            state.entities.player.add_relic(choker);
            state.turn.counters.cards_played_this_turn = already_played as u8;
            state.turn.energy = 1;
            state.zones.hand = vec![CombatCard::for_scenario(CardId::Havoc, 9001)];
            state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Defend, 9002)].into();

            handle_play_card_from_hand(0, None, &mut state).unwrap();
            settle_queued_plays(&mut state);

            assert_eq!(
                state.entities.player.block, expected_block,
                "autoplay ignores energy but cannot bypass the relic's canPlay veto"
            );
            assert_eq!(state.turn.counters.cards_played_this_turn, 6);
            assert_eq!(state.entities.player.relics[0].counter, 6);
            assert_eq!(state.turn.energy, 0);
            assert_eq!(state.zones.discard_pile[0].id, CardId::Havoc);
            assert_eq!(
                state.zones.exhaust_pile[0].id,
                CardId::Defend,
                "Havoc's blocked autoplay still follows its exhaust cleanup"
            );
            assert!(state.zones.draw_pile.is_empty());
        }
    }

    #[test]
    fn velvet_choker_uses_its_own_counter_for_manual_and_queued_playability() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        )];
        let mut choker = RelicState::new(RelicId::VelvetChoker);
        choker.counter = 6;
        state.entities.player.add_relic(choker);
        state.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 9003)];
        assert!(crate::content::cards::can_play_card(&state.zones.hand[0], &state).is_err());
        assert!(
            crate::content::cards::can_play_card_ignoring_energy(&state.zones.hand[0], &state)
                .is_err()
        );
        assert!(handle_play_card_from_hand(0, None, &mut state).is_err());
        assert_eq!(state.zones.hand.len(), 1);
        // Constructed divergent counters distinguish the relic's authority
        // from GameActionManager's independent play history.
        state.entities.player.relics[0].counter = 0;
        state.turn.counters.cards_played_this_turn = 6;
        assert!(crate::content::cards::can_play_card(&state.zones.hand[0], &state).is_ok());
        assert!(
            crate::content::cards::can_play_card_ignoring_energy(&state.zones.hand[0], &state)
                .is_ok()
        );
        assert!(handle_play_card_from_hand(0, None, &mut state).is_ok());
    }

    #[test]
    fn havoc_exhaust_draw_settles_before_autoplay_power_mummified_hand() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        )];
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::MummifiedHand));
        state.zones.hand = vec![CombatCard::for_scenario(CardId::Havoc, 801)];
        state.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Rupture, 803),
            CombatCard::for_scenario(CardId::Strike, 802),
        ]
        .into();
        for (power, amount) in [
            (PowerId::Corruption, 1),
            (PowerId::DarkEmbrace, 1),
            (PowerId::FeelNoPain, 3),
        ] {
            state.queue_action_back(Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: power,
                amount,
            });
        }
        settle_queued_plays(&mut state);
        state.turn.energy = 0;

        handle_play_card_from_hand(0, None, &mut state).unwrap();
        settle_queued_plays(&mut state);

        assert_eq!(state.zones.hand.len(), 1);
        assert_eq!(state.zones.hand[0].id, CardId::Strike);
        assert_eq!(state.zones.hand[0].cost_for_turn_java(), 0);
        assert_eq!(
            state.rng.card_random_rng.counter, 2,
            "Havoc target, then Mummified Hand's single eligible card"
        );
        assert_eq!(state.zones.exhaust_pile.len(), 1);
        assert_eq!(state.zones.exhaust_pile[0].id, CardId::Havoc);
        assert_eq!(state.get_power(0, PowerId::Rupture), 1);
        assert_eq!(state.entities.player.block, 3);
        assert_eq!(state.turn.energy, 0);
    }

    #[test]
    fn consecutive_queued_powers_settle_heatsinks_before_next_mummified_hand() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        )];
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::MummifiedHand));
        crate::engine::action_handlers::powers::handle_apply_power(
            0,
            0,
            PowerId::Heatsink,
            1,
            &mut state,
        );
        state.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Strike, 811),
            CombatCard::for_scenario(CardId::Strike, 812),
        ]
        .into();
        for serial in [813, 814] {
            state.enqueue_card_play(
                QueuedCardPlay {
                    card: CombatCard::for_scenario(CardId::Inflame, serial),
                    target: None,
                    energy_on_use: 0,
                    ignore_energy_total: true,
                    autoplay: true,
                    random_target: false,
                    is_end_turn_autoplay: false,
                    purge_on_use: false,
                    source: QueuedCardSource::Normal,
                },
                false,
            );
        }
        settle_queued_plays(&mut state);

        assert_eq!(state.get_power(0, PowerId::Strength), 4);
        assert_eq!(state.zones.hand.len(), 2);
        assert_eq!(state.zones.hand[0].cost_for_turn_java(), 0);
        assert_eq!(state.zones.hand[1].cost_for_turn_java(), 1);
        assert_eq!(state.rng.card_random_rng.counter, 1);
    }

    #[test]
    fn settled_play_placement_clears_temporary_cost_but_preserves_combat_mutations() {
        // Ordinary discard, Rebound, and an innate shuffle-back card all use
        // Java's Soul movement. A discounted card must not remain free when
        // retrieved later in the same turn.
        for (card_id, rebound) in [
            (CardId::Bash, false),
            (CardId::Bash, true),
            (CardId::Tantrum, false),
        ] {
            let mut state = crate::test_support::blank_test_combat();
            let mut card = CombatCard::for_scenario(card_id, 31);
            card.cost_modifier = 1;
            card.misc_value = 13;
            card.base_damage_mut = 17;
            card.set_cost_for_turn_java(0);
            let expected_cost = card.combat_cost_without_turn_override_java();
            let lineage = card.lineage;
            state.zones.limbo.push(card);
            if rebound {
                crate::engine::action_handlers::powers::handle_apply_power(
                    0,
                    0,
                    PowerId::Rebound,
                    1,
                    &mut state,
                );
                store::with_power_mut(&mut state, 0, PowerId::Rebound, |power| {
                    power.extra_data = 0;
                });
            }

            handle_use_card_done(false, true, &mut state);

            let settled = if rebound || card_id == CardId::Tantrum {
                &state.zones.draw_pile[0]
            } else {
                &state.zones.discard_pile[0]
            };
            assert_eq!(settled.lineage, lineage);
            assert_eq!(settled.cost_for_turn_java(), expected_cost);
            assert_eq!(settled.cost_modifier, 1);
            assert_eq!(settled.misc_value, 13);
            assert_eq!(settled.base_damage_mut, 0);
            assert!(state.zones.limbo.is_empty());
        }
    }
}
