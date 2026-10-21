use crate::runtime::combat::CombatState;
use crate::state::core::{
    ActiveCombat, ClientInput, CombatContext, CombatStartRequest, EngineState, EventCombatContext,
    PostCombatReturn,
};
use crate::state::map::node::RoomType;
use crate::state::rewards::{RewardScreenContext, TreasureChestSize, TreasureChestState};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn roll_treasure_chest_spec(run_state: &mut RunState) -> TreasureChestState {
    let roll = run_state.rng_pool.treasure_rng.random_range(0, 99);
    let size = if run_state.act_num >= 4 {
        TreasureChestSize::Medium
    } else if roll < 50 {
        TreasureChestSize::Small
    } else if roll < 83 {
        TreasureChestSize::Medium
    } else {
        TreasureChestSize::Large
    };

    let (common_chance, uncommon_chance, gold_chance, gold_amount) = match size {
        TreasureChestSize::Small => (75, 25, 50, 25),
        TreasureChestSize::Medium => (35, 50, 35, 50),
        TreasureChestSize::Large => (0, 75, 50, 75),
    };

    let reward_roll = run_state.rng_pool.treasure_rng.random_range(0, 99);
    let base_relic_tier = if reward_roll < common_chance {
        crate::content::relics::RelicTier::Common
    } else if reward_roll < common_chance + uncommon_chance {
        crate::content::relics::RelicTier::Uncommon
    } else {
        crate::content::relics::RelicTier::Rare
    };
    let gold_reward_base_amount = if reward_roll < gold_chance {
        Some(gold_amount)
    } else {
        None
    };

    TreasureChestState {
        size,
        base_relic_tier,
        gold_reward_base_amount,
    }
}

fn open_treasure_chest(
    run_state: &mut RunState,
    chest: TreasureChestState,
) -> crate::state::rewards::RewardState {
    let mut reward =
        crate::state::rewards::RewardState::with_context(RewardScreenContext::TreasureRoom);

    // --- onChestOpen() relic hooks (non-boss chest) ---
    // CursedKey: add a random curse to deck
    if run_state
        .relics
        .iter()
        .any(|r| r.id == crate::content::relics::RelicId::CursedKey)
    {
        let curse_pool = crate::content::cards::get_curse_pool();
        if !curse_pool.is_empty() {
            let idx = run_state
                .rng_pool
                .card_rng
                .random_range(0, (curse_pool.len() - 1) as i32) as usize;
            run_state.add_card_to_deck_with_upgrades_from(
                curse_pool[idx],
                0,
                crate::state::selection::DomainEventSource::Relic(
                    crate::content::relics::RelicId::CursedKey,
                ),
            );
        }
    }

    // Matryoshka: add an extra relic reward (75% Common, 25% Uncommon)
    if let Some(mat) = run_state
        .relics
        .iter_mut()
        .find(|r| r.id == crate::content::relics::RelicId::Matryoshka && r.counter > 0)
    {
        mat.counter -= 1;
        if mat.counter == 0 {
            mat.counter = -2;
            mat.used_up = true;
        }
        let extra_tier = if run_state.rng_pool.relic_rng.random_boolean_chance(0.75) {
            crate::content::relics::RelicTier::Common
        } else {
            crate::content::relics::RelicTier::Uncommon
        };
        let extra_relic = run_state.random_relic_by_tier(extra_tier);
        reward.items.push(crate::state::rewards::RewardItem::Relic {
            relic_id: extra_relic,
        });
    }

    if let Some(base_amount) = chest.gold_reward_base_amount {
        let amount = if run_state.is_daily_run {
            base_amount
        } else {
            run_state
                .rng_pool
                .treasure_rng
                .random_f32_min_max(base_amount as f32 * 0.9, base_amount as f32 * 1.1)
                .round() as i32
        };
        crate::state::rewards::generator::add_gold_reward_like_java(&mut reward.items, amount);
    }

    // Generate chest relic reward after onChestOpen hooks, matching Java
    // AbstractChest.open(): Matryoshka inserts before the base chest relic,
    // and SapphireKey links to the last relic.
    let relic_id = run_state.random_relic_by_tier(chest.base_relic_tier);
    reward
        .items
        .push(crate::state::rewards::RewardItem::Relic { relic_id });
    if run_state.is_final_act_available && !run_state.keys[1] {
        reward
            .items
            .push(crate::state::rewards::RewardItem::SapphireKey);
    }

    // NlothsMask: remove one relic from rewards (onChestOpenAfter)
    if let Some(mask) = run_state
        .relics
        .iter_mut()
        .find(|r| r.id == crate::content::relics::RelicId::NlothsMask && r.counter > 0)
    {
        mask.counter -= 1;
        if mask.counter == 0 {
            mask.counter = -2;
            mask.used_up = true;
        }
        remove_one_relic_from_rewards_after_chest_open(&mut reward.items);
    }

    reward
}

fn remove_one_relic_from_rewards_after_chest_open(
    items: &mut Vec<crate::state::rewards::RewardItem>,
) {
    if let Some(pos) = items
        .iter()
        .position(|item| matches!(item, crate::state::rewards::RewardItem::Relic { .. }))
    {
        items.remove(pos);
        if matches!(
            items.get(pos),
            Some(crate::state::rewards::RewardItem::SapphireKey)
        ) {
            items.remove(pos);
        }
    }
}

use super::campfire_handler;
use super::shop_handler;

fn resolve_out_of_combat_defeat(engine_state: &mut EngineState, run_state: &RunState) -> bool {
    if run_state.current_hp <= 0 && !matches!(engine_state, EngineState::GameOver(_)) {
        *engine_state = EngineState::GameOver(crate::state::core::RunResult::Defeat);
        return true;
    }
    false
}

fn is_run_level_potion_context(engine_state: &EngineState) -> bool {
    matches!(
        engine_state,
        EngineState::MapNavigation
            | EngineState::MapOverlay { .. }
            | EngineState::EventRoom
            | EngineState::RewardScreen(_)
            | EngineState::RewardOverlay { .. }
            | EngineState::TreasureRoom(_)
            | EngineState::Campfire
            | EngineState::Shop(_)
            | EngineState::RunPendingChoice(_)
            | EngineState::BossRelicSelect(_)
            | EngineState::DoubleBossComplete
            | EngineState::Act3VictoryComplete
            | EngineState::TrueVictoryComplete
    )
}

fn run_event_is_we_meet_again(run_state: &RunState) -> bool {
    run_state
        .event_state
        .as_ref()
        .is_some_and(|event| event.id == crate::state::events::EventId::WeMeetAgain)
}

fn run_has_relic(run_state: &RunState, relic_id: crate::content::relics::RelicId) -> bool {
    run_state.relics.iter().any(|relic| relic.id == relic_id)
}

fn run_potion_potency(run_state: &RunState, potion_id: crate::content::potions::PotionId) -> i32 {
    let mut potency = crate::content::potions::get_potion_definition(potion_id).base_potency;
    if run_has_relic(run_state, crate::content::relics::RelicId::SacredBark) {
        potency *= 2;
    }
    potency
}

fn apply_run_level_on_use_potion_relics(run_state: &mut RunState) {
    if run_has_relic(run_state, crate::content::relics::RelicId::ToyOrnithopter) {
        run_state.heal_with_source(
            5,
            DomainEventSource::Relic(crate::content::relics::RelicId::ToyOrnithopter),
        );
    }
}

fn handle_run_level_potion_input(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    input: &Option<ClientInput>,
) -> bool {
    if !is_run_level_potion_context(engine_state) {
        return false;
    }

    let is_we_meet_again_event = run_event_is_we_meet_again(run_state);
    match input {
        Some(ClientInput::DiscardPotion(slot)) => {
            if !crate::content::potions::potion_can_discard_in_event(is_we_meet_again_event) {
                return true;
            }
            let Some((potion_id, can_discard)) = run_state
                .potions
                .get(*slot)
                .and_then(|slot| slot.as_ref())
                .map(|potion| (potion.id, potion.can_discard))
            else {
                return true;
            };
            if !can_discard {
                return true;
            }
            run_state.remove_potion_at_with_source(*slot, DomainEventSource::Potion(potion_id));
            true
        }
        Some(ClientInput::UsePotion {
            potion_index,
            target,
        }) => {
            if target.is_some() {
                return true;
            }
            let Some((potion_id, can_use)) = run_state
                .potions
                .get(*potion_index)
                .and_then(|slot| slot.as_ref())
                .map(|potion| (potion.id, potion.can_use))
            else {
                return true;
            };
            if !can_use {
                return true;
            }
            if !crate::content::potions::potion_can_use_out_of_combat(
                potion_id,
                is_we_meet_again_event,
            ) {
                return true;
            }

            let source = DomainEventSource::Potion(potion_id);
            match potion_id {
                crate::content::potions::PotionId::BloodPotion => {
                    let potency = run_potion_potency(run_state, potion_id);
                    let heal_amount =
                        crate::content::potions::mechanics::blood_potion_heal_request_v1(
                            run_state.max_hp,
                            potency,
                        );
                    run_state.heal_with_source(heal_amount, source);
                    apply_run_level_on_use_potion_relics(run_state);
                    run_state.remove_potion_at_with_source(*potion_index, source);
                }
                crate::content::potions::PotionId::FruitJuice => {
                    let potency = run_potion_potency(run_state, potion_id);
                    run_state.gain_max_hp_with_source(potency, potency, source);
                    apply_run_level_on_use_potion_relics(run_state);
                    run_state.remove_potion_at_with_source(*potion_index, source);
                }
                crate::content::potions::PotionId::EntropicBrew => {
                    let generated =
                        if run_has_relic(run_state, crate::content::relics::RelicId::Sozu) {
                            Vec::new()
                        } else {
                            let potion_slots = run_state.potions.len();
                            let potion_class = run_state.potion_class();
                            (0..potion_slots)
                                .map(|_| {
                                    crate::content::potions::random_potion(
                                        &mut run_state.rng_pool.potion_rng,
                                        potion_class,
                                        false,
                                    )
                                })
                                .collect::<Vec<_>>()
                        };
                    apply_run_level_on_use_potion_relics(run_state);
                    run_state.remove_potion_at_with_source(*potion_index, source);
                    for potion_id in generated {
                        let slot_hint = run_state.find_empty_potion_slot().unwrap_or(0);
                        run_state.obtain_potion_with_source(
                            crate::content::potions::Potion::new(
                                potion_id,
                                generated_run_level_potion_uuid(run_state, slot_hint),
                            ),
                            source,
                        );
                    }
                }
                _ => {}
            }
            true
        }
        _ => false,
    }
}

fn generated_run_level_potion_uuid(run_state: &RunState, slot: usize) -> u32 {
    60_000u32
        .saturating_add(run_state.rng_pool.potion_rng.counter.saturating_mul(10))
        .saturating_add(slot as u32)
}

fn apply_combat_meta_change(run_state: &mut RunState, change: crate::runtime::combat::MetaChange) {
    match change {
        crate::runtime::combat::MetaChange::AddCardToMasterDeck(acquisition) => {
            let card = acquisition.card;
            assert!(
                !run_state
                    .master_deck
                    .iter()
                    .any(|existing| existing.lineage == card.lineage),
                "combat acquisition was persisted twice"
            );
            run_state.emit_event(crate::state::selection::DomainEvent::CardObtained {
                card: crate::state::selection::DomainCardSnapshot {
                    id: card.id,
                    lineage: card.lineage,
                    upgrades: card.upgrades,
                },
                source: DomainEventSource::CombatMonsterGrant {
                    monster_id: acquisition.monster_id,
                    monster_slot: acquisition.monster_slot,
                },
            });
            run_state.master_deck.push(card);
        }
        crate::runtime::combat::MetaChange::ModifyCardMisc {
            card_lineage,
            amount,
        } => {
            run_state.modify_card_misc_value(card_lineage, amount);
        }
        crate::runtime::combat::MetaChange::UpgradeMasterDeckCard { card_lineage } => {
            run_state.upgrade_card_with_source(
                card_lineage,
                crate::state::selection::DomainEventSource::DeckMutation,
            );
        }
    }
}

fn persist_combat_meta_changes(run_state: &mut RunState, combat: &mut CombatState) {
    if combat.meta.meta_changes.is_empty() {
        return;
    }
    if combat.meta.meta_changes.iter().any(|change| {
        matches!(
            change,
            crate::runtime::combat::MetaChange::AddCardToMasterDeck(_)
        )
    }) {
        // Obtain hooks have already run in the exact combat kernel. Persist
        // their resources without replaying those hooks at combat end.
        run_state.absorb_combat_player(combat.entities.player.clone());
        run_state.master_deck_lineage_allocator = combat.meta.master_deck_lineage_allocator.clone();
    }
    for change in std::mem::take(&mut combat.meta.meta_changes) {
        apply_combat_meta_change(run_state, change);
    }
}

fn combat_start_request_for_room(
    run_state: &mut RunState,
    room_type: RoomType,
) -> Result<CombatStartRequest, String> {
    let encounter = match room_type {
        RoomType::MonsterRoom => run_state
            .peek_next_encounter()
            .ok_or_else(|| "normal encounter queue is empty".to_string())?,
        RoomType::MonsterRoomElite => run_state
            .peek_next_elite()
            .ok_or_else(|| "elite encounter queue is empty".to_string())?,
        RoomType::MonsterRoomBoss => run_state
            .next_boss()
            .ok_or_else(|| "boss encounter queue is empty".to_string())?,
        other => return Err(format!("room type {other:?} is not combat")),
    };
    Ok(CombatStartRequest::room(encounter, room_type))
}

fn start_pending_combat_if_needed(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    active_combat: &mut Option<ActiveCombat>,
) -> bool {
    let request = match engine_state {
        EngineState::CombatStart(request) if active_combat.is_none() => request.clone(),
        EngineState::CombatStart(_) => {
            eprintln!("Error: CombatStart requested while ActiveCombat already exists.");
            return false;
        }
        _ => return true,
    };

    match start_active_combat(run_state, request) {
        Ok(active) => {
            *engine_state = active.engine_state.clone();
            *active_combat = Some(active);
            true
        }
        Err(err) => {
            eprintln!("Combat start error: {err}");
            false
        }
    }
}

fn start_active_combat(
    run_state: &mut RunState,
    mut request: CombatStartRequest,
) -> Result<ActiveCombat, String> {
    let (engine_state, combat_state) =
        crate::sim::combat_start::build_requested_combat_start(run_state, &mut request)?;
    Ok(ActiveCombat::new_for_encounter(
        engine_state,
        combat_state,
        request.encounter_id,
        request.context,
    ))
}

fn finish_active_combat(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    active_combat: &mut Option<ActiveCombat>,
) -> bool {
    let Some(mut active) = active_combat.take() else {
        eprintln!("Error: tried to finish combat without ActiveCombat.");
        return false;
    };

    run_state.absorb_combat_player(active.combat_state.entities.player.clone());
    run_state.potions = active.combat_state.entities.potions.clone();
    // Java owns one global RNG pool. Combat clones it only to isolate search
    // branches, so the accepted exact branch must hand every stream back
    // before run-level rewards or same-floor continuations consume randomness.
    run_state.rng_pool = active.combat_state.rng.pool.clone();
    run_state.room_mugged |= active.combat_state.runtime.combat_mugged;
    run_state.room_smoked |= active.combat_state.runtime.combat_smoked;
    run_state.master_deck_lineage_allocator = active
        .combat_state
        .meta
        .master_deck_lineage_allocator
        .clone();

    for change in active.combat_state.meta.meta_changes.drain(..) {
        apply_combat_meta_change(run_state, change);
    }

    if let EngineState::GameOver(_) = engine_state {
        return false;
    }

    match active.context {
        CombatContext::Room(_) => {
            finish_room_combat(engine_state, run_state, &mut active.combat_state);
        }
        CombatContext::Event(event_context) => {
            finish_event_combat(
                engine_state,
                run_state,
                &mut active.combat_state,
                event_context,
            );
        }
    }

    if matches!(engine_state, EngineState::GameOver(_)) {
        return false;
    }
    start_pending_combat_if_needed(engine_state, run_state, active_combat)
}

fn finish_room_combat(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    combat_state: &mut CombatState,
) {
    // Java: AbstractRoom.update() skips normal reward-screen opening for
    // TheBeyond/TheEnding boss rooms. On A20, ProceedButton sends the player
    // directly to the second Act 3 boss while `bossList.size() == 2`.
    if let EngineState::RewardScreen(rs) = engine_state {
        let is_boss = combat_state.meta.is_boss_fight;
        let is_elite = combat_state.meta.is_elite_fight;

        if is_boss && run_state.act_num >= 3 {
            // AbstractRoom.update creates room gold before the TheBeyond /
            // TheEnding gate. No reward screen opens, so this gold is never
            // claimed and no potion or card reward generation follows it.
            let _unclaimed_gold =
                crate::state::rewards::generator::roll_boss_gold_amount(run_state);
        }

        if is_boss && run_state.act_num >= 4 {
            // TheEnding skips rewards but retains the original Proceed choice.
            *engine_state = EngineState::TrueVictoryComplete;
        } else if is_boss && run_state.act_num == 3 {
            if run_state.should_start_act3_double_boss() {
                *engine_state = EngineState::DoubleBossComplete;
            } else if run_state.is_final_act_available
                && run_state.keys[0]
                && run_state.keys[1]
                && run_state.keys[2]
            {
                *engine_state = EngineState::Act3VictoryComplete;
            } else {
                *engine_state = EngineState::GameOver(crate::state::core::RunResult::Victory);
            }
        } else {
            let screen_context = if run_state.room_mugged {
                RewardScreenContext::MuggedCombat
            } else if run_state.room_smoked {
                RewardScreenContext::SmokedCombat
            } else {
                RewardScreenContext::Standard
            };
            let mut existing_items = Vec::new();
            existing_items.append(&mut combat_state.runtime.pending_rewards);
            let normal_monster_rewards_allowed = !combat_state.have_monsters_escaped_java();
            if matches!(screen_context, RewardScreenContext::SmokedCombat) {
                let _hidden_room_rewards =
                    crate::state::rewards::generator::generate_combat_rewards_from_existing_with_escape_gate(
                        run_state,
                        is_elite,
                        is_boss,
                        existing_items,
                        false,
                        normal_monster_rewards_allowed,
                    );
                *rs = crate::state::rewards::RewardState::with_context(
                    RewardScreenContext::SmokedCombat,
                );
            } else {
                *rs = if existing_items.is_empty() && normal_monster_rewards_allowed {
                    crate::state::rewards::generator::generate_combat_rewards(
                        run_state, is_elite, is_boss,
                    )
                } else {
                    crate::state::rewards::generator::generate_combat_rewards_from_existing_with_escape_gate(
                        run_state,
                        is_elite,
                        is_boss,
                        existing_items,
                        true,
                        normal_monster_rewards_allowed,
                    )
                };
                rs.screen_context = screen_context;
            }

            if is_boss && run_state.act_num <= 2 {
                run_state.pending_boss_reward = true;
            }
        }
    }
}

fn finish_event_combat(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    combat_state: &mut CombatState,
    event_context: EventCombatContext,
) {
    if event_context.reward_allowed {
        let mut rewards = event_context.rewards;
        rewards.screen_context = if run_state.room_mugged {
            RewardScreenContext::MuggedCombat
        } else if run_state.room_smoked {
            RewardScreenContext::SmokedCombat
        } else {
            RewardScreenContext::Standard
        };
        if matches!(rewards.screen_context, RewardScreenContext::SmokedCombat) {
            let mut hidden_items = std::mem::take(&mut rewards.items);
            hidden_items.append(&mut combat_state.runtime.pending_rewards);
            crate::state::rewards::generator::add_potion_reward_like_java(
                run_state,
                &mut hidden_items,
            );
        } else {
            rewards
                .items
                .append(&mut combat_state.runtime.pending_rewards);
            crate::state::rewards::generator::add_potion_reward_like_java(
                run_state,
                &mut rewards.items,
            );
            if !event_context.no_cards_in_rewards {
                rewards.items.extend(
                    crate::state::rewards::generator::generate_card_reward_items(
                        run_state, false, false, false,
                    ),
                );
            }
        }
        *engine_state = EngineState::RewardScreen(rewards);
    } else {
        match event_context.post_combat_return {
            PostCombatReturn::EventRoom => {
                *engine_state = EngineState::EventRoom;
            }
            PostCombatReturn::MapNavigation => {
                *engine_state = EngineState::MapNavigation;
            }
        }
    }
}

#[cfg(test)]
/// Test-only compatibility wrapper for older room-combat tests. Runtime code
/// should use `tick_run_active` so combat context is explicit.
pub fn tick_run(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    combat_state: &mut Option<CombatState>,
    input: Option<ClientInput>,
) -> bool {
    let context = CombatContext::Room(crate::state::core::RoomCombatContext {
        room_type: run_state
            .map
            .get_current_room_type()
            .unwrap_or(RoomType::MonsterRoom),
    });
    let mut active_combat = combat_state
        .take()
        .map(|combat| ActiveCombat::new(engine_state.clone(), combat, context));
    let keep_running =
        tick_run_active_with_observer(engine_state, run_state, &mut active_combat, input)
            .keep_running;
    *combat_state = active_combat.map(|active| active.combat_state);
    keep_running
}

#[derive(Clone, Debug)]
pub struct FinishedActiveCombat {
    pub engine_state: EngineState,
    pub combat_state: CombatState,
}

#[derive(Clone, Debug)]
pub struct RunTickOutcome {
    pub keep_running: bool,
    pub finished_combat: Option<FinishedActiveCombat>,
}

impl RunTickOutcome {
    fn without_finished(keep_running: bool) -> Self {
        Self {
            keep_running,
            finished_combat: None,
        }
    }
}

fn settle_combat_won_before_player_input(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    active_combat: &mut Option<ActiveCombat>,
) -> Option<RunTickOutcome> {
    if active_combat.is_none() || !matches!(engine_state, EngineState::RewardScreen(_)) {
        return None;
    }

    let finished_combat = active_combat.as_ref().map(|active| FinishedActiveCombat {
        engine_state: active.engine_state.clone(),
        combat_state: active.combat_state.clone(),
    });
    let keep_running = finish_active_combat(engine_state, run_state, active_combat);
    Some(RunTickOutcome {
        keep_running,
        finished_combat,
    })
}

pub fn tick_run_active(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    active_combat: &mut Option<ActiveCombat>,
    input: Option<ClientInput>,
) -> bool {
    tick_run_active_with_observer(engine_state, run_state, active_combat, input).keep_running
}

pub fn tick_run_active_with_observer(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    active_combat: &mut Option<ActiveCombat>,
    input: Option<ClientInput>,
) -> RunTickOutcome {
    if handle_run_level_potion_input(engine_state, run_state, &input) {
        if resolve_out_of_combat_defeat(engine_state, run_state) {
            return RunTickOutcome {
                keep_running: false,
                finished_combat: None,
            };
        }
        return RunTickOutcome {
            keep_running: true,
            finished_combat: None,
        };
    }

    if !start_pending_combat_if_needed(engine_state, run_state, active_combat) {
        return RunTickOutcome {
            keep_running: false,
            finished_combat: None,
        };
    }
    if let Some(outcome) =
        settle_combat_won_before_player_input(engine_state, run_state, active_combat)
    {
        return outcome;
    }

    // Top level controller redirecting inputs
    let keep_running = match engine_state {
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_) => {
            if let Some(active) = active_combat.as_mut() {
                active.engine_state = engine_state.clone();
                let keep_running = super::core::tick_engine(
                    &mut active.engine_state,
                    &mut active.combat_state,
                    input.clone(),
                );
                // SmokeBomb.use marks the original room before the delayed
                // escape finishes; run observations must see the same flag.
                run_state.room_smoked |= active.combat_state.runtime.combat_smoked;
                persist_combat_meta_changes(run_state, &mut active.combat_state);
                *engine_state = active.engine_state.clone();
                if !keep_running {
                    let finished_combat =
                        active_combat.as_ref().map(|active| FinishedActiveCombat {
                            engine_state: active.engine_state.clone(),
                            combat_state: active.combat_state.clone(),
                        });
                    let keep_running = finish_active_combat(engine_state, run_state, active_combat);
                    return RunTickOutcome {
                        keep_running,
                        finished_combat,
                    };
                }
                true
            } else {
                eprintln!("Error: EngineState designates Combat but no ActiveCombat was provided.");
                false
            }
        }
        EngineState::RunPendingChoice(rpc_state) => {
            let rpc_state = rpc_state.clone();
            match crate::engine::run_pending_choice::tick_run_pending_choice_v1(
                engine_state,
                run_state,
                &rpc_state,
                input,
            ) {
                Ok(keep_running) => keep_running,
                Err(e) => {
                    eprintln!("Run pending choice error: {}", e);
                    return RunTickOutcome::without_finished(false);
                }
            }
        }
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => {
            let overlay_return_state = match engine_state {
                EngineState::MapOverlay { return_state } => Some(return_state.clone()),
                _ => None,
            };
            if let (Some(ClientInput::Cancel), Some(return_state)) = (&input, overlay_return_state)
            {
                *engine_state = *return_state;
                if resolve_out_of_combat_defeat(engine_state, run_state) {
                    return RunTickOutcome::without_finished(false);
                }
                return RunTickOutcome {
                    keep_running: true,
                    finished_combat: None,
                };
            }

            // Extract travel target from input: normal adjacency or WingBoots flight
            let travel_target = match &input {
                Some(ClientInput::SelectMapNode(target_x)) => {
                    let target_y = if run_state.map.current_y == -1 {
                        0
                    } else {
                        run_state.map.current_y + 1
                    };
                    Some((*target_x as i32, target_y, false))
                }
                Some(ClientInput::FlyToNode(target_x, target_y)) => {
                    Some((*target_x as i32, *target_y as i32, true))
                }
                _ => None,
            };

            if let Some((target_x, target_y, is_flight)) = travel_target {
                let previous_room_type = run_state.map.get_current_room_type();
                // WingBoots: check if player has charges for flight
                let has_flight = if is_flight {
                    run_state.relics.iter().any(|r| {
                        r.id == crate::content::relics::RelicId::WingBoots && r.counter > 0
                    })
                } else {
                    false
                };

                if run_state
                    .map
                    .travel_to(target_x, target_y, has_flight)
                    .is_ok()
                {
                    run_state.complete_current_room_encounter(previous_room_type);
                    run_state.room_mugged = false;
                    run_state.room_smoked = false;
                    run_state.event_state = None;
                    run_state.advance_floor_and_generate_seeds();

                    // WingBoots: decrement counter on successful flight (non-adjacent travel)
                    if is_flight {
                        if let Some(wb) = run_state
                            .relics
                            .iter_mut()
                            .find(|r| r.id == crate::content::relics::RelicId::WingBoots)
                        {
                            wb.counter -= 1;
                            if wb.counter == 0 {
                                wb.counter = -2;
                                wb.used_up = true;
                            }
                        }
                    }

                    run_state.on_enter_room();

                    if let Some(room_type) = run_state.map.get_current_room_type() {
                        // --- Room-type-specific onEnterRoom hooks ---
                        // EternalFeather: heal (deck_size / 5 * 3) on entering RestRoom
                        if room_type == RoomType::RestRoom
                            && run_state
                                .relics
                                .iter()
                                .any(|r| r.id == crate::content::relics::RelicId::EternalFeather)
                        {
                            let heal =
                                crate::content::relics::eternal_feather::rest_site_entry_heal(
                                    run_state.master_deck.len(),
                                );
                            if heal > 0
                                && !run_state.relics.iter().any(|r| {
                                    r.id == crate::content::relics::RelicId::MarkOfTheBloom
                                })
                            {
                                run_state.change_hp_with_source(
                                    heal,
                                    DomainEventSource::Relic(
                                        crate::content::relics::RelicId::EternalFeather,
                                    ),
                                );
                            }
                        }
                        // SsserpentHead: +50 gold on entering EventRoom
                        if room_type == RoomType::EventRoom
                            && run_state
                                .relics
                                .iter()
                                .any(|r| r.id == crate::content::relics::RelicId::SsserpentHead)
                        {
                            run_state.change_gold_with_source(
                                50,
                                DomainEventSource::Relic(
                                    crate::content::relics::RelicId::SsserpentHead,
                                ),
                            );
                        }

                        let actual_room_type = if room_type == RoomType::EventRoom {
                            match run_state.roll_question_mark_room_type(previous_room_type) {
                                crate::state::events::generator::RoomRoll::Monster => {
                                    RoomType::MonsterRoom
                                }
                                crate::state::events::generator::RoomRoll::Shop => {
                                    RoomType::ShopRoom
                                }
                                crate::state::events::generator::RoomRoll::Treasure => {
                                    RoomType::TreasureRoom
                                }
                                crate::state::events::generator::RoomRoll::Event => {
                                    RoomType::EventRoom
                                }
                                crate::state::events::generator::RoomRoll::Elite => {
                                    RoomType::MonsterRoomElite
                                }
                            }
                        } else {
                            room_type
                        };

                        if actual_room_type != room_type {
                            let _ = run_state.map.set_current_room_type(actual_room_type);
                        }

                        match actual_room_type {
                            RoomType::MonsterRoom
                            | RoomType::MonsterRoomElite
                            | RoomType::MonsterRoomBoss => {
                                match combat_start_request_for_room(run_state, actual_room_type) {
                                    Ok(request) => {
                                        *engine_state = EngineState::CombatStart(request);
                                    }
                                    Err(err) => {
                                        eprintln!("Combat start request error: {err}");
                                        return RunTickOutcome::without_finished(false);
                                    }
                                }
                            }
                            RoomType::RestRoom => {
                                // Java: onEnterRestRoom() for all relics
                                run_state.on_enter_rest_room();
                                *engine_state = EngineState::Campfire;
                            }
                            RoomType::ShopRoom => {
                                // MealTicket: heal 15 HP on shop entry
                                if run_state
                                    .relics
                                    .iter()
                                    .any(|r| r.id == crate::content::relics::RelicId::MealTicket)
                                    && !run_state.relics.iter().any(|r| {
                                        r.id == crate::content::relics::RelicId::MarkOfTheBloom
                                    })
                                {
                                    run_state.change_hp_with_source(
                                        15,
                                        DomainEventSource::Relic(
                                            crate::content::relics::RelicId::MealTicket,
                                        ),
                                    );
                                }
                                *engine_state = EngineState::Shop(run_state.generate_shop());
                            }
                            RoomType::EventRoom => {
                                let event_id = run_state.generate_event_from_event_room_duplicate();
                                let mut event_state =
                                    crate::state::events::EventState::new(event_id);
                                // Wire init functions for constructor-time event state.
                                use crate::state::events::EventId;
                                event_state.internal_state = match event_id {
                                    EventId::BigFish => crate::content::events::big_fish::init_big_fish_state(run_state),
                                    EventId::Nloth => crate::content::events::nloth::init_nloth_state(run_state),
                                    EventId::DeadAdventurer => crate::content::events::dead_adventurer::init_dead_adventurer_state(run_state),
                                    EventId::Designer => crate::content::events::designer::init_designer_state(run_state),
                                    EventId::WorldOfGoop => crate::content::events::goop_puddle::init_goop_puddle_state(run_state),
                                    EventId::Falling => crate::content::events::falling::init_falling_state(run_state),
                                    _ => 0,
                                };
                                if event_id == EventId::WeMeetAgain {
                                    crate::content::events::we_meet_again::init_we_meet_again_event_state(run_state, &mut event_state);
                                }
                                if event_id == EventId::MysteriousSphere {
                                    crate::content::events::mysterious_sphere::init_event_state(
                                        run_state,
                                        &mut event_state,
                                    );
                                }
                                if event_id == EventId::MaskedBandits {
                                    crate::content::events::masked_bandits::init_event_state(
                                        run_state,
                                        &mut event_state,
                                    );
                                }
                                // Events with extra_data init (complex state)
                                if event_id == EventId::MatchAndKeep {
                                    crate::content::events::match_and_keep::init_match_game_board(
                                        run_state,
                                        &mut event_state,
                                    );
                                }
                                run_state.event_state = Some(event_state);
                                *engine_state = EngineState::EventRoom;
                            }
                            RoomType::TreasureRoom => {
                                *engine_state =
                                    EngineState::TreasureRoom(roll_treasure_chest_spec(run_state));
                            }
                            RoomType::TrueVictoryRoom => {
                                // Act 4 ending — true victory after defeating the Heart
                                *engine_state =
                                    EngineState::GameOver(crate::state::core::RunResult::Victory);
                            }
                        }
                    }
                }
            }
            if resolve_out_of_combat_defeat(engine_state, run_state) {
                return RunTickOutcome::without_finished(false);
            }
            if !start_pending_combat_if_needed(engine_state, run_state, active_combat) {
                return RunTickOutcome::without_finished(false);
            }
            if let Some(outcome) =
                settle_combat_won_before_player_input(engine_state, run_state, active_combat)
            {
                return outcome;
            }
            true
        }
        EngineState::TreasureRoom(chest) => {
            match input {
                Some(ClientInput::OpenChest) => {
                    let reward = open_treasure_chest(run_state, *chest);
                    *engine_state = EngineState::RewardScreen(reward);
                }
                Some(ClientInput::Proceed) | Some(ClientInput::Cancel) => {
                    *engine_state = EngineState::MapNavigation;
                }
                _ => {}
            }
            if resolve_out_of_combat_defeat(engine_state, run_state) {
                return RunTickOutcome::without_finished(false);
            }
            if !start_pending_combat_if_needed(engine_state, run_state, active_combat) {
                return RunTickOutcome::without_finished(false);
            }
            if let Some(outcome) =
                settle_combat_won_before_player_input(engine_state, run_state, active_combat)
            {
                return outcome;
            }
            true
        }
        EngineState::Campfire => {
            let keep_running = campfire_handler::handle(engine_state, run_state, input);
            if !keep_running {
                return RunTickOutcome::without_finished(false);
            }
            if resolve_out_of_combat_defeat(engine_state, run_state) {
                return RunTickOutcome::without_finished(false);
            }
            if !start_pending_combat_if_needed(engine_state, run_state, active_combat) {
                return RunTickOutcome::without_finished(false);
            }
            if let Some(outcome) =
                settle_combat_won_before_player_input(engine_state, run_state, active_combat)
            {
                return outcome;
            }
            true
        }
        EngineState::Shop(_) => {
            let mut transition = None;
            if let EngineState::Shop(shop) = engine_state {
                if let Some(new_state) = shop_handler::handle(run_state, shop, input.clone()) {
                    transition = Some(new_state);
                }
            }
            if let Some(new_state) = transition {
                *engine_state = new_state;
            }
            if resolve_out_of_combat_defeat(engine_state, run_state) {
                return RunTickOutcome::without_finished(false);
            }
            true
        }
        EngineState::EventRoom => {
            if let Some(ClientInput::EventChoice(choice_idx)) = input {
                if let Err(e) = crate::engine::event_handler::handle_event_choice(
                    engine_state,
                    run_state,
                    choice_idx,
                ) {
                    eprintln!("Event Error: {}", e);
                }
            }
            if resolve_out_of_combat_defeat(engine_state, run_state) {
                return RunTickOutcome::without_finished(false);
            }
            if !start_pending_combat_if_needed(engine_state, run_state, active_combat) {
                return RunTickOutcome::without_finished(false);
            }
            if let Some(outcome) =
                settle_combat_won_before_player_input(engine_state, run_state, active_combat)
            {
                return outcome;
            }
            true
        }
        EngineState::RewardScreen(_) => {
            let mut transition = None;
            if let EngineState::RewardScreen(rs) = engine_state {
                if let Some(new_state) =
                    crate::engine::reward_handler::handle(run_state, rs, input.clone())
                {
                    transition = Some(new_state);
                }
            }
            if let Some(new_state) = transition {
                *engine_state = new_state;
            }
            if resolve_out_of_combat_defeat(engine_state, run_state) {
                return RunTickOutcome::without_finished(false);
            }
            true
        }
        EngineState::RewardOverlay { .. } => {
            let mut transition = None;
            if let EngineState::RewardOverlay {
                reward_state,
                return_state,
            } = engine_state
            {
                if let Some(new_state) = crate::engine::reward_handler::handle_overlay(
                    run_state,
                    reward_state,
                    input.clone(),
                    (**return_state).clone(),
                ) {
                    transition = Some(new_state);
                }
            }
            if let Some(new_state) = transition {
                *engine_state = new_state;
            }
            if resolve_out_of_combat_defeat(engine_state, run_state) {
                return RunTickOutcome::without_finished(false);
            }
            true
        }
        EngineState::TrueVictoryComplete => {
            if matches!(input, Some(ClientInput::Proceed)) {
                // ProceedButton.goToTrueVictoryRoom enters (3, 4) through
                // AbstractDungeon.nextRoomTransition, including floor RNG and
                // room-entry relic effects, before the terminal cutscene.
                run_state.complete_current_room_encounter(Some(RoomType::MonsterRoomBoss));
                run_state.advance_floor_and_generate_seeds();
                run_state.map.current_x = 3;
                run_state.map.current_y = 4;
                run_state.room_mugged = false;
                run_state.room_smoked = false;
                run_state.on_enter_room();
                *engine_state = EngineState::GameOver(crate::state::core::RunResult::Victory);
            }
            true
        }
        EngineState::Act3VictoryComplete => {
            if matches!(input, Some(ClientInput::Proceed)) {
                crate::content::events::spire_heart::enter_victory_room(run_state);
                *engine_state = EngineState::EventRoom;
            }
            true
        }
        EngineState::DoubleBossComplete => {
            if matches!(input, Some(ClientInput::Proceed)) {
                // ProceedButton.goToDoubleBoss enters a new (-1, 15) boss
                // node, represented by the same synthetic (0, 15) control
                // point as the first boss. RNG changes only on this input.
                run_state.complete_current_room_encounter(Some(RoomType::MonsterRoomBoss));
                run_state.advance_floor_and_generate_seeds();
                run_state.map.current_x = 0;
                run_state.map.current_y = 15;
                run_state.on_enter_room();
                run_state.reveal_next_boss_from_list();
                match combat_start_request_for_room(run_state, RoomType::MonsterRoomBoss) {
                    Ok(request) => *engine_state = EngineState::CombatStart(request),
                    Err(err) => {
                        eprintln!("Act 3 double boss start error: {err}");
                        return RunTickOutcome::without_finished(false);
                    }
                }
                if !start_pending_combat_if_needed(engine_state, run_state, active_combat) {
                    return RunTickOutcome::without_finished(false);
                }
            }
            true
        }
        EngineState::BossRelicSelect(_) => {
            let mut transition = None;
            if let EngineState::BossRelicSelect(bs) = engine_state {
                if let Some(new_state) =
                    crate::engine::boss_reward_handler::handle(run_state, bs, input.clone())
                {
                    transition = Some(new_state);
                }
            }
            if let Some(new_state) = transition {
                *engine_state = new_state;
            }
            if resolve_out_of_combat_defeat(engine_state, run_state) {
                return RunTickOutcome::without_finished(false);
            }
            true
        }
        EngineState::CombatStart(_) => {
            if !start_pending_combat_if_needed(engine_state, run_state, active_combat) {
                false
            } else if let Some(outcome) =
                settle_combat_won_before_player_input(engine_state, run_state, active_combat)
            {
                return outcome;
            } else {
                true
            }
        }
        EngineState::GameOver(_) => false,
    };
    RunTickOutcome {
        keep_running,
        finished_combat: None,
    }
}

#[cfg(test)]
mod tests;
