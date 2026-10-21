use crate::content::powers::store;
use crate::engine::pending_choices;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatPhase, CombatState};
use crate::state::core::{
    ClientInput, DiscoveryChoiceState, EngineState, PendingChoice, RunResult,
};
use crate::state::selection::{EngineDiagnostic, EngineDiagnosticClass, EngineDiagnosticSeverity};
use crate::state::selection::{SelectionResolution, SelectionScope};

use super::diagnostics::record_engine_diagnostic;
use super::pending_choice_resolution::{
    grid_select_candidates, hand_select_can_fizzle_when_empty, hand_select_candidates,
};
use super::{
    compute_player_turn_start_draw_count, discard_hand_for_turn_transition, discovery,
    queue_end_of_turn_ethereal_exhausts, update_monster_intents, victory,
    CombatEnginePhaseProfiler, CombatEngineProfilePhaseV1,
};

pub(super) fn process_combat_processing<P: CombatEnginePhaseProfiler>(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
    profiler: &mut P,
) -> bool {
    combat_state.ensure_flush_next_queued_card();
    if combat_state.has_pending_actions() {
        let next_action = combat_state.pop_next_action().unwrap();

        // Intercept SuspendFor* actions and transition to PendingChoice.
        match next_action {
            Action::SuspendForHandSelect {
                min,
                max,
                can_cancel,
                filter,
                reason,
            } => {
                let candidate_lineages = hand_select_candidates(combat_state, filter);
                let available = candidate_lineages.len() as u8;
                if available == 0 {
                    let legal_empty_fizzle = min == 0
                        || hand_select_can_fizzle_when_empty(reason)
                        || (filter == crate::state::HandSelectFilter::Upgradeable
                            && reason == crate::state::HandSelectReason::Upgrade);
                    record_engine_diagnostic(
                            combat_state,
                            EngineDiagnostic {
                                severity: if legal_empty_fizzle {
                                    EngineDiagnosticSeverity::Info
                                } else {
                                    EngineDiagnosticSeverity::Error
                                },
                                class: if legal_empty_fizzle {
                                    EngineDiagnosticClass::Normalization
                                } else {
                                    EngineDiagnosticClass::Broken
                                },
                                message: format!(
                                    "auto-skipped empty hand select for {:?} with filter {:?} (requested min={}, max={})",
                                    reason, filter, min, max
                                ),
                            },
                        );
                    return true;
                }

                if available == 1 && min == 1 && max == 1 && !can_cancel {
                    let _ = pending_choices::handle_hand_select(
                        engine_state,
                        combat_state,
                        &candidate_lineages,
                        1,
                        true,
                        false,
                        reason,
                        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                            SelectionScope::Hand,
                            [candidate_lineages[0]],
                        )),
                    );
                    return true;
                }

                let min_cards = min.min(available);
                let max_cards = max.min(available);
                if min_cards != min || max_cards != max {
                    record_engine_diagnostic(
                            combat_state,
                            EngineDiagnostic {
                                severity: if min > available {
                                    EngineDiagnosticSeverity::Warning
                                } else {
                                    EngineDiagnosticSeverity::Info
                                },
                                class: if min > available {
                                    EngineDiagnosticClass::Suspicious
                                } else {
                                    EngineDiagnosticClass::Normalization
                                },
                                message: format!(
                                    "normalized hand select for {:?} with filter {:?} from min/max {}/{} to {}/{} because only {} candidates remain",
                                    reason, filter, min, max, min_cards, max_cards, available
                                ),
                            },
                        );
                }

                update_monster_intents(combat_state);
                *engine_state = EngineState::PendingChoice(PendingChoice::HandSelect {
                    candidate_lineages,
                    min_cards,
                    max_cards,
                    can_cancel,
                    reason,
                });
                return true;
            }
            Action::SuspendForGridSelect {
                source_pile,
                min,
                max,
                can_cancel,
                filter,
                reason,
            } => {
                let exhume_selection = source_pile == crate::state::PileType::Exhaust
                    && filter == crate::state::GridSelectFilter::NonExhume
                    && matches!(reason, crate::state::GridSelectReason::Exhume { .. });
                if exhume_selection && combat_state.zones.hand.len() >= 10 {
                    return true;
                }
                let auto_select_whole_draw_pile = source_pile == crate::state::PileType::Draw
                    && filter == crate::state::GridSelectFilter::Any
                    && reason == crate::state::GridSelectReason::DrawPileToHand
                    && !can_cancel
                    && min == max
                    && combat_state.zones.draw_pile.len() <= usize::from(max);
                let candidate_lineages = if auto_select_whole_draw_pile {
                    // Java's auto-all branch iterates drawPile.group directly.
                    // Only the interactive branch builds and sorts a grid.
                    combat_state
                        .zones
                        .draw_pile
                        .iter()
                        .rev()
                        .map(|card| card.lineage)
                        .collect()
                } else {
                    grid_select_candidates(combat_state, source_pile, filter, reason)
                };
                let available = candidate_lineages.len() as u8;
                if available == 0 {
                    let legal_empty_fizzle = min == 0 || exhume_selection
                        || matches!(reason, crate::state::GridSelectReason::Omniscience { .. })
                        // These Java actions inspect their pile only when they
                        // resolve. Earlier hooks may have drawn every match.
                        || matches!((source_pile, filter, reason),
                            (crate::state::PileType::Draw, crate::state::GridSelectFilter::Skill,
                                crate::state::GridSelectReason::SkillFromDeckToHand)
                            | (crate::state::PileType::Draw, crate::state::GridSelectFilter::Attack,
                                crate::state::GridSelectReason::AttackFromDeckToHand)
                            | (crate::state::PileType::Draw, crate::state::GridSelectFilter::Any,
                                crate::state::GridSelectReason::DrawPileToHand))
                        || (source_pile == crate::state::PileType::Discard
                            && matches!(
                                reason,
                                crate::state::GridSelectReason::DiscardToHand
                                    | crate::state::GridSelectReason::DiscardToHandNoCostChange
                                    | crate::state::GridSelectReason::DiscardToHandRetain
                            ));
                    record_engine_diagnostic(
                            combat_state,
                            EngineDiagnostic {
                                severity: if legal_empty_fizzle {
                                    EngineDiagnosticSeverity::Info
                                } else {
                                    EngineDiagnosticSeverity::Error
                                },
                                class: if legal_empty_fizzle {
                                    EngineDiagnosticClass::Normalization
                                } else {
                                    EngineDiagnosticClass::Broken
                                },
                                message: format!(
                                    "auto-skipped empty grid select for {:?} on {:?} with filter {:?} (requested min={}, max={})",
                                    reason, source_pile, filter, min, max
                                ),
                            },
                        );
                    return true;
                }

                let java_auto_selects_single_candidate = match reason {
                    crate::state::GridSelectReason::Omniscience { .. } => false,
                    crate::state::GridSelectReason::Exhume { .. } if exhume_selection => {
                        // The singleton branch precedes removal of Exhumes.
                        // A filtered singleton still opens the grid.
                        combat_state.zones.exhaust_pile.len() == 1
                    }
                    _ => true,
                };
                if auto_select_whole_draw_pile
                    || (java_auto_selects_single_candidate
                        && available == 1
                        && min == 1
                        && max == 1
                        && !can_cancel)
                {
                    let _ = pending_choices::handle_grid_select(
                        engine_state,
                        combat_state,
                        &candidate_lineages,
                        source_pile,
                        available,
                        available,
                        false,
                        reason,
                        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                            SelectionScope::Grid,
                            candidate_lineages.iter().copied(),
                        )),
                    );
                    return true;
                }

                let min_cards = min.min(available);
                let max_cards = max.min(available);
                if min_cards != min || max_cards != max {
                    record_engine_diagnostic(
                            combat_state,
                            EngineDiagnostic {
                                severity: if min > available {
                                    EngineDiagnosticSeverity::Warning
                                } else {
                                    EngineDiagnosticSeverity::Info
                                },
                                class: if min > available {
                                    EngineDiagnosticClass::Suspicious
                                } else {
                                    EngineDiagnosticClass::Normalization
                                },
                                message: format!(
                                    "normalized grid select for {:?} on {:?} with filter {:?} from min/max {}/{} to {}/{} because only {} candidates remain",
                                    reason, source_pile, filter, min, max, min_cards, max_cards, available
                                ),
                            },
                        );
                }

                update_monster_intents(combat_state);
                *engine_state = EngineState::PendingChoice(PendingChoice::GridSelect {
                    source_pile,
                    candidate_lineages,
                    min_cards,
                    max_cards,
                    can_cancel,
                    reason,
                });
                return true;
            }
            Action::SuspendForDiscovery {
                colorless,
                card_type,
                amount,
                cost_for_turn,
                can_skip,
            } => {
                // Java DiscoveryAction.generateCardChoices(type) /
                // generateColorlessCardChoices() samples three unique card
                // IDs when the screen opens.
                let cards =
                    discovery::generate_discovery_choices(combat_state, colorless, card_type);
                combat_state.turn.set_discovery_cost_for_turn(cost_for_turn);
                update_monster_intents(combat_state);
                *engine_state = EngineState::PendingChoice(PendingChoice::DiscoverySelect(
                    DiscoveryChoiceState {
                        cards,
                        colorless,
                        card_type,
                        amount,
                        can_skip,
                    },
                ));
                return true;
            }
            Action::SuspendForForeignInfluence { upgraded } => {
                let cards = discovery::generate_foreign_influence_choices(combat_state);
                update_monster_intents(combat_state);
                *engine_state = EngineState::PendingChoice(PendingChoice::ForeignInfluenceSelect {
                    cards,
                    upgraded,
                });
                return true;
            }
            Action::SuspendForCardReward {
                pool,
                destination,
                can_skip,
                skip_if_monsters_basically_dead,
            } => {
                if skip_if_monsters_basically_dead
                    && combat_state.are_monsters_basically_dead_java()
                {
                    return true;
                }
                // Generate 3 unique random cards from pool
                use crate::runtime::action::CardRewardPool;
                let mut card_pool: Vec<crate::content::cards::CardId> = Vec::new();
                match pool {
                    CardRewardPool::ClassAll => {
                        // Java: returnTrulyRandomCardInCombat() -> all cards for
                        // the current player's class, not always Ironclad.
                        card_pool.extend(crate::content::cards::class_combat_card_pool_for_type(
                            combat_state.meta.player_class.as_str(),
                            None,
                        ));
                    }
                    CardRewardPool::Colorless => {
                        // Java: returnTrulyRandomColorlessCardInCombat()
                        card_pool.extend(crate::content::cards::random_colorless_in_combat_pool());
                    }
                }
                let mut cards = Vec::new();
                while cards.len() < 3 && !card_pool.is_empty() {
                    let idx = combat_state
                        .rng
                        .card_random_rng
                        .random(card_pool.len() as i32 - 1) as usize;
                    let id = card_pool[idx];
                    if !cards.contains(&id) {
                        cards.push(id);
                    }
                }
                update_monster_intents(combat_state);
                *engine_state = EngineState::PendingChoice(PendingChoice::CardRewardSelect {
                    cards,
                    destination,
                    can_skip,
                });
                return true;
            }
            Action::SuspendForStanceChoice => {
                update_monster_intents(combat_state);
                *engine_state = EngineState::PendingChoice(PendingChoice::StanceChoice);
                return true;
            }
            Action::SuspendForChooseOne { choices } => {
                update_monster_intents(combat_state);
                *engine_state =
                    EngineState::PendingChoice(PendingChoice::ChooseOneSelect { choices });
                return true;
            }

            Action::Scry(amount) => {
                let amount = crate::content::relics::hooks::on_scry(combat_state, amount);
                if combat_state.are_monsters_basically_dead_java() {
                    return true;
                }
                for power in &crate::content::powers::store::powers_snapshot_for(combat_state, 0) {
                    for action in crate::content::powers::resolve_power_on_scry(
                        power.power_type,
                        0,
                        power.amount,
                    ) {
                        combat_state.queue_action_back(action);
                    }
                }
                if amount == 0 || combat_state.zones.draw_pile.is_empty() {
                    return true;
                }
                let limit = amount.min(combat_state.zones.draw_pile.len());
                let cards = combat_state
                    .zones
                    .draw_pile
                    .iter()
                    .take(limit)
                    .map(|card| card.id)
                    .collect();
                let card_lineages = combat_state
                    .zones
                    .draw_pile
                    .iter()
                    .take(limit)
                    .map(|card| card.lineage)
                    .collect();
                for action in crate::content::cards::hooks::on_scry(combat_state) {
                    combat_state.queue_action_back(action.action);
                }

                update_monster_intents(combat_state);
                *engine_state = EngineState::PendingChoice(PendingChoice::ScrySelect {
                    cards,
                    card_lineages,
                });
                return true;
            }

            Action::FleeCombat => {
                // SmokeBomb starts an automatic escape animation. Its timer calls
                // AbstractRoom.endBattle without an end-turn input or hooks.
                combat_state.runtime.combat_smoked = true;
                combat_state.turn.mark_player_escaping();
                combat_state.turn.clear_escape_pending_reward();
                return true;
            }
            _ => {
                crate::engine::action_handlers::execute_action(next_action, combat_state);
            }
        }
        if combat_state.entities.player.current_hp <= 0 {
            combat_state.clear_pending_actions();
            *engine_state = EngineState::GameOver(RunResult::Defeat);
            return false;
        }
        if matches!(engine_state, EngineState::PendingChoice(_)) {
            return true;
        }
    } else if combat_state.turn.counters.player_escaping {
        // AbstractPlayer.updateEscapeAnimation calls endBattle after the timer,
        // independently of player/end-turn input. Preserve the hand and skip all
        // end-turn powers, Ethereal exhausts and enemy turns.
        if !combat_state.turn.counters.victory_triggered {
            combat_state.turn.mark_victory_triggered();
            if !victory::resolve_victory_hooks_immediately(combat_state) {
                *engine_state = EngineState::GameOver(RunResult::Defeat);
                return false;
            }
        }
        combat_state.entities.player.facing_left = false;
        *engine_state =
            EngineState::RewardScreen(crate::state::rewards::RewardState::with_context(
                crate::state::rewards::RewardScreenContext::SmokedCombat,
            ));
        return false;
    } else {
        // Queue is empty; decide next state based on combat phase.
        match combat_state.turn.current_phase {
            CombatPhase::PlayerTurn => {
                // GameActionManager refreshes the hand when its queue drains
                // back to WAITING_ON_USER. A card's intermediate selection
                // screen (e.g. Headbutt) still belongs to its running action:
                // facing has changed there, but BackAttack icons may not have.
                crate::content::powers::core::surrounded::sync_back_attack_markers(combat_state);
                if crate::content::relics::unceasing_top::maybe_on_refresh_hand(combat_state) {
                    *engine_state = EngineState::CombatProcessing;
                    return true;
                }
                update_monster_intents(combat_state);
                *engine_state = EngineState::CombatPlayerTurn;
            }
            CombatPhase::TurnTransition => {
                // === TURN TRANSITION: end of player turn -> enemy turn -> new player turn ===

                // Both actions and cardQueue have drained (ensure_flush above).
                // Keep this phase across power-generated pending choices so
                // resuming them cannot run the end-turn powers twice.
                if !combat_state.turn.counters.end_turn_powers_triggered {
                    crate::engine::action_handlers::cards::handle_end_turn_post_cards(combat_state);
                    return true;
                }

                if combat_state.turn.counters.end_turn_hand_discarded {
                    // Bottom-queued Exhaust hooks have now drained. Their
                    // generated cards were created after Java's discard pass
                    // and must remain in hand for the next turn.
                    combat_state.turn.clear_end_turn_hand_discarded();
                } else {
                    // 1. Java DiscardAtEndOfTurnAction first runs Ethereal
                    // triggerOnEndOfPlayerTurn hooks. End-turn queued card plays
                    // have already drained before this empty-queue boundary.
                    if queue_end_of_turn_ethereal_exhausts(combat_state) {
                        *engine_state = EngineState::CombatProcessing;
                        return true;
                    }

                    // 2. Discard hand (respecting Retain and RunicPyramid)
                    let marker = profiler.begin(CombatEngineProfilePhaseV1::DiscardHand);
                    discard_hand_for_turn_transition(combat_state);
                    profiler.end(CombatEngineProfilePhaseV1::DiscardHand, marker);
                }

                let skip_monster_turn = combat_state.turn.counters.skip_monster_turn_pending;
                let player_had_blur_for_block_retention = crate::content::powers::store::has_power(
                    combat_state,
                    0,
                    crate::content::powers::PowerId::Blur,
                );

                if !skip_monster_turn {
                    // Java's EndTurnAction has set turnHasEnded before
                    // MonsterStartTurnAction runs pre-turn powers. A Poison
                    // death can apply Spore Cloud here and must receive the
                    // same fresh Vulnerable duration as a monster move.
                    combat_state.begin_monster_turn();
                    // 1.5 === MONSTER PRE-TURN LOGIC ===
                    // Java: MonsterStartTurnAction calls MonsterGroup.applyPreTurnLogic() -> clears block, etc.
                    let pre_turn_marker =
                        profiler.begin(CombatEngineProfilePhaseV1::MonsterPreTurn);
                    let alive_for_pre: Vec<_> = combat_state
                        .entities
                        .monsters
                        .iter()
                        .filter(|m| !m.is_dying && !m.is_escaped)
                        .map(|m| m.id)
                        .collect();

                    for mid in &alive_for_pre {
                        // 1. Clear block
                        let has_barricade = crate::content::powers::store::has_power(
                            combat_state,
                            *mid,
                            crate::content::powers::PowerId::Barricade,
                        );
                        if let Some(monster) = combat_state
                            .entities
                            .monsters
                            .iter_mut()
                            .find(|m| m.id == *mid)
                        {
                            if !has_barricade {
                                monster.block = 0;
                            }
                        }
                        // 2. Fire Start of Turn Powers (e.g. Poison tick, Flight regain)
                        for power in
                            &crate::content::powers::store::powers_snapshot_for(combat_state, *mid)
                        {
                            let hook_actions =
                                crate::content::powers::resolve_power_instance_at_turn_start(
                                    power,
                                    combat_state,
                                    *mid,
                                );
                            for a in hook_actions {
                                combat_state.queue_action_back(a);
                            }
                        }
                    }
                    // 3. Drain pre-turn actions instantly
                    while let Some(action) = combat_state.pop_next_action() {
                        crate::engine::action_handlers::execute_action(action, combat_state);
                        if combat_state.entities.player.current_hp <= 0 {
                            combat_state.clear_pending_actions();
                            *engine_state = EngineState::GameOver(RunResult::Defeat);
                            profiler
                                .end(CombatEngineProfilePhaseV1::MonsterPreTurn, pre_turn_marker);
                            return false;
                        }
                    }
                    profiler.end(CombatEngineProfilePhaseV1::MonsterPreTurn, pre_turn_marker);

                    let monster_turn_setup_marker =
                        profiler.begin(CombatEngineProfilePhaseV1::MonsterTurnSetup);
                    // 2. Execute each alive monster's turn (player block absorbs damage)
                    let mut monster_snapshots = Vec::new();
                    let mut dead_ids = Vec::new();
                    for m in &combat_state.entities.monsters {
                        if m.is_dying || m.is_escaped {
                            dead_ids.push(m.id);
                        } else {
                            monster_snapshots.push(m.clone());
                        }
                    }
                    for id in dead_ids {
                        store::remove_entity_powers(combat_state, id);
                    }
                    profiler.end(
                        CombatEngineProfilePhaseV1::MonsterTurnSetup,
                        monster_turn_setup_marker,
                    );
                    for monster in &monster_snapshots {
                        let monster_move_resolution_marker =
                            profiler.begin(CombatEngineProfilePhaseV1::MonsterMoveResolution);
                        combat_state.record_monster_protocol_executed_move(
                            monster.id,
                            monster.planned_move_id(),
                        );
                        let actions =
                            crate::content::monsters::resolve_monster_turn(combat_state, monster);
                        for action in actions {
                            combat_state.queue_action_back(action);
                        }
                        profiler.end(
                            CombatEngineProfilePhaseV1::MonsterMoveResolution,
                            monster_move_resolution_marker,
                        );
                        let monster_during_turn_powers_marker =
                            profiler.begin(CombatEngineProfilePhaseV1::MonsterDuringTurnPowers);
                        for power in &crate::content::powers::store::powers_snapshot_for(
                            combat_state,
                            monster.id,
                        ) {
                            let during_turn_actions =
                                crate::content::powers::resolve_power_during_turn(
                                    power, monster.id,
                                );
                            for action in during_turn_actions {
                                combat_state.queue_action_back(action);
                            }
                        }
                        profiler.end(
                            CombatEngineProfilePhaseV1::MonsterDuringTurnPowers,
                            monster_during_turn_powers_marker,
                        );
                        // Drain this monster's turn actions
                        while let Some(action) = combat_state.pop_next_action() {
                            crate::engine::action_handlers::execute_action_with_profiler(
                                action,
                                combat_state,
                                profiler,
                            );
                            if combat_state.entities.player.current_hp <= 0 {
                                combat_state.clear_pending_actions();
                                *engine_state = EngineState::GameOver(RunResult::Defeat);
                                return false;
                            }
                        }
                    }

                    // Java gates the next round on areMonstersBasicallyDead
                    // after the monster queue and its actions have drained.
                    // Fading, Explosive, poison, or retaliation can end combat
                    // here. Do not enqueue another player's draw/relic/power
                    // hooks, but keep committed death aftermath ahead of victory.
                    if let Some(keep_running) =
                        victory::settle_victory_if_ready(engine_state, combat_state)
                    {
                        return keep_running;
                    }

                    let monster_end_round_marker =
                        profiler.begin(CombatEngineProfilePhaseV1::MonsterEndRound);
                    // (Monster actions now drained per-monster inside the for-loop above)

                    // 2.3 === COLLECTIVE END OF TURN ===
                    // Java: MonsterGroup.applyEndOfTurnPowers() calls p.atEndOfTurn(false) across all alive monsters.
                    let alive_monsters_for_end_turn: Vec<_> = combat_state
                        .entities
                        .monsters
                        .iter()
                        .filter(|m| !m.is_dying && !m.is_escaped)
                        .map(|m| m.id)
                        .collect();
                    for mid in &alive_monsters_for_end_turn {
                        for power in
                            &crate::content::powers::store::powers_snapshot_for(combat_state, *mid)
                        {
                            let hook_actions = crate::content::powers::resolve_power_at_end_of_turn(
                                power,
                                combat_state,
                                *mid,
                            );
                            for a in hook_actions {
                                combat_state.queue_action_back(a);
                            }
                        }
                    }
                    // 2.5 === FULL ROUND END ===
                    // Java: applyEndOfTurnPowers() calls p.atEndOfRound() on player and all monsters.
                    // These hooks enqueue actions but Java does not drain the action queue until
                    // after the following player start-of-turn hooks and DrawCardAction are queued.
                    // Vault sets room.skipMonsterTurn, and GameActionManager skips this whole call.
                    for power in
                        &crate::content::powers::store::powers_snapshot_for(combat_state, 0)
                    {
                        let hook_actions = crate::content::powers::resolve_power_at_end_of_round(
                            power.power_type,
                            combat_state,
                            0,
                            power.amount,
                            power.just_applied,
                        );
                        for a in hook_actions {
                            combat_state.queue_action_back(a);
                        }
                    }
                    // Monster powers:
                    let alive_monsters: Vec<_> = combat_state
                        .entities
                        .monsters
                        .iter()
                        .filter(|m| !m.is_dying && !m.is_escaped)
                        .map(|m| m.id)
                        .collect();
                    for mid in alive_monsters {
                        for power in
                            &crate::content::powers::store::powers_snapshot_for(combat_state, mid)
                        {
                            let hook_actions =
                                crate::content::powers::resolve_power_at_end_of_round(
                                    power.power_type,
                                    combat_state,
                                    mid,
                                    power.amount,
                                    power.just_applied,
                                );
                            for a in hook_actions {
                                combat_state.queue_action_back(a);
                            }
                        }
                    }
                    // Clear all just_applied flags globally at the end of the round!
                    store::clear_just_applied_flags(combat_state);
                    profiler.end(
                        CombatEngineProfilePhaseV1::MonsterEndRound,
                        monster_end_round_marker,
                    );
                }

                // If player died during monster turn, immediate game over
                if combat_state.entities.player.current_hp <= 0 {
                    combat_state.clear_pending_actions();
                    *engine_state = EngineState::GameOver(RunResult::Defeat);
                    return false;
                }

                let player_turn_start_marker =
                    profiler.begin(CombatEngineProfilePhaseV1::PlayerTurnStart);
                // 3. Intent rolling is usually handled by Action::RollMonsterMove in the queue,
                //    but freshly spawned monsters may roll immediately during spawn to match
                //    Java SpawnMonsterAction.init() timing.

                // === NEW PLAYER TURN START ===
                // 4. Clear player block (Barricade: keep all, Calipers: retain up to 15)
                let has_barricade = crate::content::powers::store::has_power(
                    combat_state,
                    0,
                    crate::content::powers::PowerId::Barricade,
                );
                if !has_barricade && !player_had_blur_for_block_retention {
                    let has_calipers = !combat_state
                        .entities
                        .player
                        .relic_buses
                        .on_calculate_block_retained
                        .is_empty();
                    if has_calipers {
                        let retained = crate::content::relics::hooks::on_calculate_block_retained(
                            combat_state,
                            combat_state.entities.player.block,
                        );
                        combat_state.entities.player.block = retained;
                    } else {
                        combat_state.entities.player.block = 0;
                    }
                }

                // (Monster blocks are cleared per-monster at the start of each monster's turn above)

                combat_state.begin_next_player_turn();
                crate::engine::action_handlers::powers::apply_player_turn_energy_recharge_hooks(
                    combat_state,
                );
                // Reset player Invincible limit
                let _ = store::with_power_mut(
                    combat_state,
                    0,
                    crate::content::powers::PowerId::Invincible,
                    |inv| {
                        inv.amount = inv.extra_data;
                    },
                );

                // 7.9. current stance atStartOfTurn.
                // Java: AbstractPlayer.applyStartOfTurnRelics() calls stance.atStartOfTurn()
                // before relic atTurnStart hooks. Divinity queues a return to Neutral here.
                if combat_state.entities.player.stance == crate::runtime::combat::StanceId::Divinity
                {
                    combat_state.queue_action_back(Action::EnterStance("Neutral".to_string()));
                }

                // 8. at_turn_start relic hooks (AncientTeaSet, HappyFlower, etc.)
                // Java: stance and relics fire atTurnStart BEFORE draw cards
                let turn_start_actions = crate::content::relics::hooks::at_turn_start(combat_state);
                combat_state.queue_actions(turn_start_actions);

                // 8.1. applyStartOfTurnCards (draw pile, hand, discard pile)
                // Java runs card atTurnStart hooks before player powers and orbs.
                let card_actions =
                    crate::content::cards::hooks::at_turn_start_in_hand(combat_state);
                combat_state.queue_actions(card_actions);

                // 8.2. at_turn_start power hooks (Player)
                // Java: player.applyStartOfTurnPowers()
                for power in &crate::content::powers::store::powers_snapshot_for(combat_state, 0) {
                    let pa = crate::content::powers::resolve_power_instance_at_turn_start(
                        power,
                        combat_state,
                        0,
                    );
                    for a in pa {
                        combat_state.queue_action_back(a);
                    }
                }

                // 8.3. applyStartOfTurnOrbs
                let orb_actions = crate::content::orbs::hooks::at_turn_start(combat_state);
                combat_state.queue_actions(orb_actions);

                // 9. Draw cards (default 5, reduced by DrawReduction power)
                // Java consumes AbstractDungeon.player.gameHandSize here.
                // Rust still derives the same result locally until a broader
                // draw-target state is justified.
                let draw_count = compute_player_turn_start_draw_count(combat_state);
                // Java calls post-draw hook methods before DrawCardAction
                // executes; their addToBot actions therefore land behind
                // DrawCardAction but ahead of actions produced while drawing.
                combat_state.queue_action_back(Action::PostDrawTrigger);
                if draw_count > 0 {
                    combat_state.queue_action_back(Action::DrawCards(draw_count as u32));
                }

                *engine_state = EngineState::CombatProcessing;
                profiler.end(
                    CombatEngineProfilePhaseV1::PlayerTurnStart,
                    player_turn_start_marker,
                );
            }
            CombatPhase::MonsterTurn => {
                // Monster actions drained, transition to player turn start
                combat_state.turn.begin_player_phase();
                *engine_state = EngineState::CombatProcessing;
            }
        }
        if combat_state.entities.player.current_hp <= 0 {
            *engine_state = EngineState::GameOver(RunResult::Defeat);
            return false;
        }
        if let Some(keep_running) = victory::settle_victory_if_ready(engine_state, combat_state) {
            return keep_running;
        }
        return true;
    }

    if let Some(keep_running) = victory::settle_victory_if_ready(engine_state, combat_state) {
        return keep_running;
    }

    true
}
