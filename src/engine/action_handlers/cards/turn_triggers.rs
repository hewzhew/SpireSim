use crate::content::cards::CardId;
use crate::content::powers::store;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState, QueuedCardPlay, QueuedCardSource};

fn is_end_turn_triggered_autoplay(card_id: CardId) -> bool {
    matches!(
        card_id,
        CardId::Burn | CardId::Regret | CardId::Decay | CardId::Doubt | CardId::Shame
    )
}

pub fn resolve_end_turn_triggered_autoplay(
    state: &CombatState,
    card: &CombatCard,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = match card.id {
        CardId::Burn => crate::content::cards::status::burn::on_end_turn_in_hand(state, card),
        CardId::Regret => crate::content::cards::curses::regret::on_end_turn_queued_card(card),
        CardId::Decay => crate::content::cards::curses::decay::on_end_turn_in_hand(state),
        CardId::Doubt => crate::content::cards::curses::doubt::on_end_turn_in_hand(state),
        CardId::Shame => crate::content::cards::curses::shame::on_end_turn_in_hand(state),
        _ => smallvec::smallvec![],
    };
    // Java drains actions before taking the next cardQueue item. Keep this
    // card's payload before its cleanup and Rust's next-card flush, including
    // Burn (whose original use schedules damage with addToBot).
    for action in &mut actions {
        action.insertion_mode = AddTo::Top;
    }
    actions
}

/// Capture the same payload used by the end-turn card queue, including Regret's
/// trigger-time hand size. A projection may inspect it without running a card.
pub fn end_turn_autoplay_card(state: &CombatState, card: &CombatCard) -> Option<CombatCard> {
    if !is_end_turn_triggered_autoplay(card.id) {
        return None;
    }
    let mut queued = card.clone();
    if queued.id == CardId::Regret {
        queued.base_magic_num_mut = state.zones.hand.len() as i32;
    }
    Some(queued)
}

pub fn handle_end_turn_trigger(state: &mut CombatState) {
    let mut actions = smallvec::SmallVec::new();

    // 1. Relics
    actions.extend(crate::content::relics::hooks::at_end_of_turn(state));

    // 2. Java applyEndOfTurnPreCardPowers. Ordinary atEndOfTurn hooks
    // belong to AbstractRoom.endTurn, after the card queue has drained.
    for power in store::powers_snapshot_for(state, 0)
        .into_iter()
        .filter(|p| crate::content::powers::end_turn_power_precedes_cards(p.power_type))
    {
        actions.extend(
            crate::content::powers::resolve_power_at_end_of_turn(&power, state, 0)
                .into_iter()
                .map(|a| ActionInfo {
                    action: a,
                    insertion_mode: AddTo::Bottom,
                }),
        );
    }

    // 3. Orbs
    actions.extend(crate::content::orbs::hooks::trigger_end_of_turn_orbs(state));

    // 4. Status/curse in-hand triggers. Java does not process Ethereal here:
    // triggerOnEndOfPlayerTurn() belongs to DiscardAtEndOfTurnAction, after
    // these queued card plays have completely resolved.
    for card in &state.zones.hand {
        if let Some(queued) = end_turn_autoplay_card(state, card) {
            actions.push(ActionInfo {
                action: Action::EnqueueCardPlay {
                    item: Box::new(QueuedCardPlay {
                        card: queued,
                        target: None,
                        energy_on_use: state.turn.energy as i32,
                        ignore_energy_total: false,
                        autoplay: false,
                        random_target: false,
                        is_end_turn_autoplay: true,
                        purge_on_use: false,
                        source: QueuedCardSource::EndTurnTriggered,
                    }),
                    in_front: false,
                },
                insertion_mode: AddTo::Bottom,
            });
        } else if card.id == CardId::Pride {
            actions.extend(crate::content::cards::curses::pride::on_end_turn_in_hand(
                state,
            ));
        }
    }

    // 5. Stances
    actions.extend(crate::content::stances::hooks::on_end_of_turn(state));

    state.queue_actions(actions);
}

/// AbstractRoom.endTurn: ordinary power hooks and attribute reset happen
/// after end-turn card autoplay, before ethereal exhaust and hand discard.
pub fn handle_end_turn_post_cards(state: &mut CombatState) {
    state.turn.counters.end_turn_powers_triggered = true;
    for power in store::powers_snapshot_for(state, 0)
        .into_iter()
        .filter(|p| !crate::content::powers::end_turn_power_precedes_cards(p.power_type))
    {
        for action in crate::content::powers::resolve_power_at_end_of_turn(&power, state, 0) {
            state.queue_action_back(action);
        }
    }

    // Java AbstractRoom.endTurn invokes these power hooks first, queues
    // ClearCardQueueAction and DiscardAtEndOfTurnAction, then immediately calls
    // AbstractCard.resetAttributes() across draw, discard, and hand before any
    // queued action resolves. In particular, temporary costForTurn changes
    // such as Mummified Hand must not survive into the next player turn.
    // Exhaust is intentionally absent: Java resets only these three active
    // piles. Rust's one-turn Retain flag remains for the later discard phase.
    state
        .zones
        .draw_pile
        .mutate_cards_with_kind_tracking(CombatCard::reset_attributes_java);
    for card in state
        .zones
        .discard_pile
        .iter_mut()
        .chain(state.zones.hand.iter_mut())
    {
        card.reset_attributes_java();
    }
}

pub fn handle_post_draw_trigger(state: &mut CombatState) {
    let mut actions = smallvec::SmallVec::new();

    actions.extend(crate::content::relics::hooks::at_turn_start_post_draw(
        state,
    ));

    for power in &store::powers_snapshot_for(state, 0) {
        for action in crate::content::powers::resolve_power_on_post_draw(
            power.power_type,
            state,
            0,
            power.amount,
        ) {
            actions.push(ActionInfo {
                action,
                insertion_mode: AddTo::Bottom,
            });
        }
    }

    state.queue_actions(actions);
}

pub fn handle_clear_card_queue(state: &mut CombatState) {
    state.zones.queued_cards.clear();
    state.engine.retain(|a| {
        !matches!(
            a,
            Action::EnqueueCardPlay { .. }
                | Action::PlayCardDirect { .. }
                | Action::FlushNextQueuedCard
        )
    });
}

pub fn handle_pre_battle_trigger(state: &mut CombatState) {
    // 1. Monster pre-battle actions (CurlUp, ModeShift, etc.)
    // Java: AbstractRoom.initializeBattle() calls usePreBattleAction() for each monster
    let monsters_snapshot: Vec<_> = state
        .entities
        .monsters
        .iter()
        .map(|m| (m.enemy_id, m.id))
        .collect();
    for (enemy_id, monster_id) in &monsters_snapshot {
        if let Some(entity) = state.entities.monsters.iter().find(|m| m.id == *monster_id) {
            let entity_clone = entity.clone();
            let pre_actions = crate::content::monsters::resolve_pre_battle_actions(
                state,
                *enemy_id,
                &entity_clone,
                crate::content::monsters::PreBattleLegacyRng::MonsterHp,
            );
            for action in pre_actions {
                state.queue_action_back(action);
            }
        }
    }

    // 2. Room pre-battle actions. Java queues the burning-elite buff after
    // every monster's usePreBattleAction and before player relic atPreBattle.
    for action in std::mem::take(&mut state.runtime.room_pre_battle_actions) {
        state.queue_action_back(action);
    }

    // 3. Relic pre-battle hooks (e.g. Snecko Eye applying Confusion)
    let pre_battle_actions = crate::content::relics::hooks::at_pre_battle(state);
    state.queue_actions(pre_battle_actions);

    // Auto-chain Phase 2
    state.queue_action_back(crate::runtime::action::Action::BattleStartPreDrawTrigger);
}

pub fn handle_battle_start_pre_draw_trigger(state: &mut CombatState) {
    let pre_draw_actions = crate::content::relics::hooks::at_battle_start_pre_draw(state);
    state.queue_actions(pre_draw_actions);

    // Java AbstractRoom.update() constructs the whole opening queue before
    // actionManager drains it:
    //   atBattleStartPreDraw hooks
    //   DrawCardAction
    //   atBattleStart hooks
    //   atTurnStart relics
    //   atTurnStartPostDraw relics
    //   card / power / orb atTurnStart hooks
    //
    // Therefore these hook methods must run synchronously here. Queuing a later
    // synthetic BattleStartTrigger would incorrectly let the initial draw execute
    // before atBattleStart / atTurnStart hooks have had a chance to enqueue.
    let draw_amount = crate::engine::core::compute_player_turn_start_draw_count(state);
    if draw_amount > 0 {
        state.queue_action_back(crate::runtime::action::Action::DrawCards(
            draw_amount as u32,
        ));
    }

    queue_initial_battle_start_hooks_after_draw_is_queued(state);
}

pub fn handle_battle_start_trigger(state: &mut CombatState) {
    // Relic battle-start hooks (e.g. Akabeko, Marbles)
    let battle_start_actions = crate::content::relics::hooks::at_battle_start(state);
    state.queue_actions(battle_start_actions);
}

fn queue_initial_battle_start_hooks_after_draw_is_queued(state: &mut CombatState) {
    let battle_start_actions = crate::content::relics::hooks::at_battle_start(state);
    state.queue_actions(battle_start_actions);

    // Java AbstractPlayer.applyStartOfTurnRelics() calls stance.atStartOfTurn()
    // before relic atTurnStart hooks. Divinity queues a return to Neutral here.
    if state.entities.player.stance == crate::runtime::combat::StanceId::Divinity {
        state.queue_action_back(crate::runtime::action::Action::EnterStance(
            "Neutral".to_string(),
        ));
    }

    let turn_start_actions = crate::content::relics::hooks::at_turn_start(state);
    state.queue_actions(turn_start_actions);

    // Initial combat is special: AbstractRoom.update() calls only relic
    // atTurnStartPostDraw here, not power atStartOfTurnPostDraw.
    let post_draw_relic_actions = crate::content::relics::hooks::at_turn_start_post_draw(state);
    state.queue_actions(post_draw_relic_actions);

    let card_actions = crate::content::cards::hooks::at_turn_start_in_hand(state);
    state.queue_actions(card_actions);

    for power in &crate::content::powers::store::powers_snapshot_for(state, 0) {
        let power_actions =
            crate::content::powers::resolve_power_instance_at_turn_start(power, state, 0);
        for action in power_actions {
            state.queue_action_back(action);
        }
    }

    let orb_actions = crate::content::orbs::hooks::at_turn_start(state);
    state.queue_actions(orb_actions);
}
