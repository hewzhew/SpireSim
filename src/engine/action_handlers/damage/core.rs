// action_handlers/damage.rs — Combat damage domain
//
// Handles: Damage, DamageAllEnemies, random-target damage, DropkickDamageAndEffect,
//          FiendFire, Feed, VampireDamage, VampireDamageAllEnemies,
//          LoseHp, GainBlock, GainBlockRandomMonster, LoseBlock, GainEnergy,
//          Heal, GainMaxHp, LoseMaxHp,
//          LimitBreak, BlockPerNonAttack, ExhaustAllNonAttack, ExhaustRandomCard

use crate::content::powers::store;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MonsterDamageOutcome {
    pub(super) hp_lost: i32,
    pub(super) died: bool,
}

pub(super) fn queue_player_hp_loss_hooks(
    state: &mut CombatState,
    amount: i32,
    source: Option<crate::EntityId>,
    damage_type: DamageType,
) {
    if amount <= 0 {
        return;
    }

    for power in &store::powers_snapshot_for(state, 0) {
        let hook_actions = crate::content::powers::resolve_power_on_hp_lost(
            power.power_type,
            state,
            0,
            amount,
            power.amount,
            source,
            damage_type,
        );
        queue_hp_lost_power_actions(state, power.power_type, hook_actions);
    }

    let relic_actions = crate::content::relics::hooks::on_lose_hp(state, amount);
    state.queue_actions(relic_actions);
}

fn queue_hp_lost_power_actions(
    state: &mut CombatState,
    power_id: PowerId,
    actions: smallvec::SmallVec<[Action; 2]>,
) {
    match power_id {
        PowerId::PlatedArmor => {
            for action in actions {
                state.queue_action_back(action);
            }
        }
        _ => {
            for action in actions.into_iter().rev() {
                state.queue_action_front(action);
            }
        }
    }
}

pub(super) fn queue_red_skull_threshold_actions(
    state: &mut CombatState,
    previous_hp: i32,
    current_hp: i32,
) {
    let max_hp = state.entities.player.max_hp;
    let actions = crate::content::relics::red_skull::on_player_hp_changed(
        state,
        previous_hp,
        current_hp,
        max_hp,
    );
    state.queue_actions(actions);
}

pub(super) fn queue_on_block_gained_hooks(
    state: &mut CombatState,
    owner: crate::EntityId,
    gained_block: i32,
) {
    if gained_block <= 0 {
        return;
    }

    for power in &store::powers_snapshot_for(state, owner) {
        let hook_actions = crate::content::powers::resolve_power_on_block_gained(
            power.power_type,
            state,
            owner,
            power.amount,
            gained_block,
        );
        for action in hook_actions {
            state.queue_action_back(action);
        }
    }
}

pub(super) fn update_player_cards_on_damage(state: &mut CombatState) {
    let update_cost = |card: &mut crate::runtime::combat::CombatCard| {
        if let Some(delta) =
            crate::content::cards::mechanics::card_cost_change_on_hp_loss_v1(card.id)
        {
            card.update_cost_java(delta);
        }
    };
    for card in state
        .zones
        .hand
        .iter_mut()
        .chain(state.zones.discard_pile.iter_mut())
    {
        update_cost(card);
    }
    state
        .zones
        .draw_pile
        .mutate_cards_with_kind_tracking(update_cost);
}

pub(super) fn target_receives_java_unique_kill_reward(state: &CombatState, target: usize) -> bool {
    state
        .entities
        .monsters
        .iter()
        .find(|m| m.id == target)
        .is_some_and(|m| {
            (m.is_dying || m.current_hp <= 0)
                && !m.half_dead
                && !store::has_power(state, target, PowerId::Minion)
        })
}

fn monsters_are_basically_dead_for_post_combat(state: &CombatState) -> bool {
    state.are_monsters_basically_dead_java()
}

pub(super) fn clear_post_combat_actions_if_ready(state: &mut CombatState) {
    if monsters_are_basically_dead_for_post_combat(state) {
        state.clear_post_combat_actions();
    }
}

/// Shared block-deduction logic. Returns unblocked damage.
pub fn deduct_block(block: &mut i32, damage: i32) -> i32 {
    if *block > 0 {
        if damage >= *block {
            let unblocked = damage - *block;
            *block = 0;
            unblocked
        } else {
            *block -= damage;
            0
        }
    } else {
        damage
    }
}

/// Complete damage-to-monster pipeline: block → Boot → onAttackedToChangeDamage → HP loss → death.
/// Returns the final HP lost (after block).
/// This does NOT include on_hp_lost / on_attacked power hooks — those are only fired from
/// the full Action::Damage path (which has access to damage_type for hook guards).
pub fn apply_raw_damage_to_monster(
    state: &mut CombatState,
    target_id: usize,
    raw_damage: i32,
) -> i32 {
    let mut hp_lost = 0;
    if let Some(m) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == target_id)
    {
        let mut final_damage = raw_damage.max(0);
        final_damage = deduct_block(&mut m.block, final_damage);
        if final_damage > 0 {
            let actual_hp_lost = final_damage.min(m.current_hp.max(0));
            m.current_hp = (m.current_hp - final_damage).max(0);
            hp_lost = actual_hp_lost;
        }
    }
    super::super::check_and_trigger_monster_death(state, target_id);
    hp_lost
}

/// Final-receive power hooks shared by damage and direct HP-loss paths, before Block.
pub(super) fn damage_after_final_receive(
    state: &CombatState,
    target: usize,
    damage: i32,
    kind: DamageType,
) -> i32 {
    store::powers_snapshot_for(state, target)
        .iter()
        .fold(damage, |damage, power| {
            crate::content::powers::resolve_power_at_damage_final_receive(
                power.power_type,
                damage,
                power.amount,
                kind,
            )
        })
}

pub(super) fn apply_damage_to_monster_via_pipeline(
    state: &mut CombatState,
    info: &crate::runtime::action::DamageInfo,
    mut final_damage: i32,
) -> MonsterDamageOutcome {
    let target_id = info.target;
    let source_id = info.source;
    let mut outcome = MonsterDamageOutcome::default();

    if let Some(mut m) = state
        .entities
        .monsters
        .iter()
        .find(|m| m.id == target_id)
        .cloned()
    {
        if m.is_dying || m.is_escaped {
            return outcome;
        }

        let target_hp_before_damage = m.current_hp;
        let hp_loss_damage = info.damage_type == DamageType::HpLoss;
        let had_block = !hp_loss_damage && m.block > 0;
        if !hp_loss_damage {
            final_damage = deduct_block(&mut m.block, final_damage);
        }

        if source_id == 0
            && info.damage_type == DamageType::Normal
            && final_damage > 0
            && final_damage < 5
            && state
                .entities
                .player
                .has_relic(crate::content::relics::RelicId::Boot)
        {
            final_damage = 5;
        }

        for power in &store::powers_snapshot_for(state, target_id) {
            final_damage = crate::content::powers::resolve_power_on_attacked_to_change_damage(
                power.power_type,
                state,
                info,
                final_damage,
                power.amount,
            );
        }

        if source_id != NO_SOURCE {
            for power in &store::powers_snapshot_for(state, source_id) {
                let hook_actions = crate::content::powers::resolve_power_on_attack(
                    power.power_type,
                    source_id,
                    target_id,
                    final_damage,
                    info.damage_type,
                    power.amount,
                );
                for action in hook_actions.into_iter().rev() {
                    state.queue_action_front(action);
                }
            }
        }

        if let Some(real_m) = state
            .entities
            .monsters
            .iter_mut()
            .find(|monster| monster.id == target_id)
        {
            real_m.block = m.block;
            if final_damage > 0 {
                let actual_hp_lost = final_damage.min(real_m.current_hp.max(0));
                real_m.current_hp = (real_m.current_hp - final_damage).max(0);
                outcome.hp_lost = actual_hp_lost;
                outcome.died = real_m.current_hp <= 0;
            }
        }

        // AbstractMonster.damage invokes onAttacked before die(). Retain
        // this hit's power instances before Darkling half-death clears them,
        // so their deferred actions still receive the observed trigger.
        let attacked_powers = store::powers_snapshot_for(state, target_id);
        super::super::check_and_trigger_monster_death(state, target_id);

        if had_block
            && m.block == 0
            && state
                .entities
                .player
                .has_relic(crate::content::relics::RelicId::HandDrill)
        {
            let hand_drill_actions =
                crate::content::relics::hand_drill::on_break_block(state, target_id);
            state.queue_actions(hand_drill_actions);
        }

        if outcome.hp_lost > 0 {
            for power in &store::powers_snapshot_for(state, target_id) {
                let hook_actions = crate::content::powers::resolve_power_on_hp_lost(
                    power.power_type,
                    state,
                    target_id,
                    outcome.hp_lost,
                    power.amount,
                    Some(source_id),
                    info.damage_type,
                );
                if power.power_type == PowerId::Split {
                    // Java large slimes / Slime Boss call setMove(Split) immediately
                    // inside damage(), then add a SetMoveAction to the bottom. The
                    // immediate mutation prevents duplicate split interrupts during
                    // queued multi-hit attacks while preserving the existing queue.
                    // Large slimes also set their private splitTriggered flag
                    // immediately; that state is not itself a queued Java action.
                    for a in hook_actions {
                        super::super::execute_action(a.clone(), state);
                        if !matches!(
                            a,
                            Action::UpdateMonsterRuntime {
                                patch: crate::runtime::action::MonsterRuntimePatch::LargeSlime { .. },
                                ..
                            }
                        ) {
                            state.queue_action_back(a);
                        }
                    }
                } else {
                    queue_hp_lost_power_actions(state, power.power_type, hook_actions);
                }
            }

            if let Some(m) = state
                .entities
                .monsters
                .iter()
                .find(|m| m.id == target_id)
                .cloned()
            {
                let monster_actions = crate::content::monsters::dispatch_on_damaged(
                    m.enemy_id,
                    state,
                    &m,
                    outcome.hp_lost,
                );
                state.queue_actions(monster_actions);
            }
        }

        for power in &attacked_powers {
            let should_fire_this_power = match info.damage_type {
                DamageType::HpLoss => false,
                DamageType::Thorns => power.power_type == PowerId::Shifting,
                _ => true,
            };
            if !should_fire_this_power {
                continue;
            }

            if power.power_type == PowerId::Malleable {
                if outcome.hp_lost > 0 && outcome.hp_lost < target_hp_before_damage {
                    state.queue_action_back(Action::GainBlock {
                        target: target_id,
                        amount: power.amount,
                    });
                    let _ = store::with_power_mut(state, target_id, PowerId::Malleable, |mal| {
                        mal.amount += 1;
                    });
                }
                continue;
            }

            let hook_actions = crate::content::powers::resolve_power_on_attacked(
                power.power_type,
                state,
                target_id,
                outcome.hp_lost,
                source_id,
                info.damage_type,
                power.amount,
            );
            let curl_up_triggered = power.power_type == PowerId::CurlUp && !hook_actions.is_empty();
            if matches!(
                power.power_type,
                PowerId::Malleable | PowerId::CurlUp | PowerId::Flight | PowerId::Reactive
            ) {
                for a in hook_actions {
                    state.queue_action_back(a);
                }
                if curl_up_triggered {
                    let _ = store::with_power_mut(state, target_id, PowerId::CurlUp, |curl| {
                        curl.extra_data = 1;
                    });
                }
            } else {
                for a in hook_actions {
                    state.queue_action_front(a);
                }
            }
        }
    }

    clear_post_combat_actions_if_ready(state);
    outcome
}

fn damage_action_target_is_dead_or_escaped(state: &CombatState, target_id: usize) -> bool {
    if target_id == 0 {
        state.entities.player.current_hp <= 0
    } else {
        state
            .entities
            .monsters
            .iter()
            .find(|m| m.id == target_id)
            .is_none_or(|m| m.is_dead_or_escaped())
    }
}

fn damage_action_source_is_dying_or_half_dead(state: &CombatState, source_id: usize) -> bool {
    if source_id == NO_SOURCE {
        return false;
    }
    if source_id == 0 {
        return state.entities.player.current_hp <= 0;
    }

    state
        .entities
        .monsters
        .iter()
        .find(|m| m.id == source_id)
        .is_some_and(|m| m.is_dying || m.half_dead)
}

pub(super) fn should_cancel_java_damage_action(
    state: &CombatState,
    info: &crate::runtime::action::DamageInfo,
) -> bool {
    if info.damage_type == DamageType::Thorns {
        return false;
    }

    damage_action_target_is_dead_or_escaped(state, info.target)
        || damage_action_source_is_dying_or_half_dead(state, info.source)
}

pub(super) fn calculate_damage_action_output(
    state: &CombatState,
    info: &crate::runtime::action::DamageInfo,
) -> (i32, bool) {
    let target_id = info.target;
    let source_id = info.source;

    // Damage contract:
    // - Player-origin Normal damage arrives pre-evaluated in `output`.
    // - Monster-origin Normal damage is re-resolved here from `base`.
    // - Non-Normal damage kinds (`HpLoss`, `Thorns`, etc.) use the supplied numeric value.
    if !info.is_modified
        && source_id != 0
        && source_id != NO_SOURCE
        && info.damage_type == DamageType::Normal
    {
        (
            crate::content::powers::calculate_monster_damage(
                info.base, source_id, target_id, state,
            ),
            true,
        )
    } else if (source_id == 0 || source_id == NO_SOURCE) && info.damage_type == DamageType::Normal {
        (info.output.max(0), true)
    } else {
        (info.output.max(0), false)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayerDamageResolution {
    pub raw_damage: i32,
    pub block_consumed: i32,
    pub damage_before_hp_loss_hooks: i32,
    pub hp_loss: i32,
}

pub fn resolve_player_damage(
    info: &crate::runtime::action::DamageInfo,
    state: &mut CombatState,
) -> PlayerDamageResolution {
    let (raw_damage, damage_already_includes_final_receive) =
        calculate_damage_action_output(state, info);
    let mut damage = raw_damage;

    if !damage_already_includes_final_receive {
        damage = damage_after_final_receive(state, 0, damage, info.damage_type);
    }

    let block_before = state.entities.player.block.max(0);
    // AbstractCreature.decrementBlock never consumes block for HP_LOSS.
    if info.damage_type != DamageType::HpLoss {
        damage = deduct_block(&mut state.entities.player.block, damage);
    }
    let block_consumed = block_before.saturating_sub(state.entities.player.block.max(0));

    damage = crate::content::relics::hooks::on_attacked_to_change_damage(state, damage, info);
    for power in &store::powers_snapshot_for(state, 0) {
        damage = crate::content::powers::resolve_power_on_attacked_to_change_damage(
            power.power_type,
            state,
            info,
            damage,
            power.amount,
        );
    }

    let damage_before_hp_loss_hooks = damage.max(0);
    let hp_loss =
        crate::content::relics::hooks::on_lose_hp_last(state, damage_before_hp_loss_hooks).max(0);

    PlayerDamageResolution {
        raw_damage: raw_damage.max(0),
        block_consumed,
        damage_before_hp_loss_hooks,
        hp_loss,
    }
}

pub fn handle_damage(info: crate::runtime::action::DamageInfo, state: &mut CombatState) {
    if should_cancel_java_damage_action(state, &info) {
        return;
    }

    let target_id = info.target;
    let source_id = info.source;
    let target_is_player = target_id == 0;

    if target_is_player {
        let resolution = resolve_player_damage(&info, state);
        // 4. on_attacked (Target Powers + Relics)
        if source_id != 0 || info.damage_type == DamageType::Normal {
            for power in &store::powers_snapshot_for(state, 0) {
                let hook_actions = crate::content::powers::resolve_power_on_attacked(
                    power.power_type,
                    state,
                    0,
                    resolution.damage_before_hp_loss_hooks,
                    source_id,
                    info.damage_type,
                    power.amount,
                );
                for a in hook_actions.into_iter().rev() {
                    state.queue_action_front(a);
                }
            }
        }

        let final_damage = resolution.hp_loss;
        if final_damage > 0 {
            let previous_hp = state.entities.player.current_hp;
            state.entities.player.current_hp =
                (state.entities.player.current_hp - final_damage).max(0);
            update_player_cards_on_damage(state);
            state.turn.increment_times_damaged_this_combat();
            queue_red_skull_threshold_actions(state, previous_hp, state.entities.player.current_hp);
            queue_player_hp_loss_hooks(
                state,
                final_damage,
                (source_id != NO_SOURCE).then_some(source_id),
                info.damage_type,
            );
            // AbstractPlayer.damage calls the attacker's onInflictDamage after
            // HP-loss prevention. Painful Stabs uses addToBot, so its Wound must
            // follow the already queued hits and monster actions.
            if source_id != NO_SOURCE {
                for power in &store::powers_snapshot_for(state, source_id) {
                    for action in crate::content::powers::resolve_power_on_inflict_damage(
                        power.power_type,
                        final_damage,
                        info.damage_type,
                    ) {
                        state.queue_action_back(action);
                    }
                }
            }

            // 7. Death Check
            if state.entities.player.current_hp <= 0 {
                super::super::try_revive(state);
            }
        }
    } else {
        let (calculated_output, damage_already_includes_final_receive) =
            calculate_damage_action_output(state, &info);
        let mut final_damage = calculated_output;
        if !damage_already_includes_final_receive {
            final_damage =
                damage_after_final_receive(state, target_id, final_damage, info.damage_type);
        }
        let _ = apply_damage_to_monster_via_pipeline(state, &info, final_damage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::PowerId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::action_handlers::execute_action;
    use crate::runtime::action::{DamageInfo, DamageType};
    use crate::runtime::combat::{Power, PowerPayload};
    use crate::state::rewards::RewardItem;
    use crate::test_support::{blank_test_combat, test_monster};

    fn thorns_damage(amount: i32) -> DamageInfo {
        DamageInfo {
            source: 1,
            target: 0,
            base: amount,
            output: amount,
            damage_type: DamageType::Thorns,
            is_modified: false,
        }
    }

    #[test]
    fn rupture_requires_self_source_and_positive_final_hp_loss() {
        for source in [0, 1, NO_SOURCE] {
            for damage_type in [DamageType::Normal, DamageType::Thorns, DamageType::HpLoss] {
                for prevention in ["none", "block", "buffer", "tungsten", "buffer-tungsten"] {
                    let mut state = blank_test_combat();
                    state.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
                    let power = |power_type, amount| Power {
                        power_type,
                        amount,
                        instance_id: None,
                        extra_data: 0,
                        payload: PowerPayload::None,
                        just_applied: false,
                    };
                    let mut powers = vec![power(PowerId::Rupture, 2)];
                    if prevention.contains("buffer") {
                        powers.push(power(PowerId::Buffer, 1));
                    }
                    state.entities.power_db.insert(0, powers);
                    if prevention.contains("tungsten") {
                        state
                            .entities
                            .player
                            .add_relic(RelicState::new(RelicId::TungstenRod));
                    }
                    if prevention == "block" {
                        state.entities.player.block = 1;
                    }
                    handle_damage(
                        DamageInfo {
                            source,
                            damage_type,
                            ..thorns_damage(1)
                        },
                        &mut state,
                    );
                    while let Some(action) = state.pop_next_action() {
                        execute_action(action, &mut state);
                    }
                    let lost = prevention == "none"
                        || (prevention == "block" && damage_type == DamageType::HpLoss);
                    assert_eq!(state.entities.player.current_hp, 80 - i32::from(lost));
                    assert_eq!(state.get_power(0, PowerId::Rupture), 2);
                    assert_eq!(
                        state.get_power(0, PowerId::Strength),
                        if lost && source == 0 { 2 } else { 0 },
                        "{source} {damage_type:?} {prevention}"
                    );
                    assert_eq!(state.get_power(0, PowerId::Buffer), 0);
                }
            }
        }
    }

    fn painful_stabs_combat() -> CombatState {
        let mut state = blank_test_combat();
        state.entities.monsters = vec![test_monster(EnemyId::BookOfStabbing)];
        state.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::PainfulStabs,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        state
    }

    #[test]
    fn painful_stabs_queues_one_wound_per_unblocked_hit_after_existing_actions() {
        use crate::content::cards::CardId;
        let mut state = painful_stabs_combat();
        state.entities.player.block = 17;
        for _ in 0..5 {
            state.queue_action_back(Action::Damage(DamageInfo {
                damage_type: DamageType::Normal,
                ..thorns_damage(5)
            }));
        }
        for _ in 0..5 {
            let action = state.pop_next_action().unwrap();
            assert!(matches!(action, Action::Damage(_)));
            execute_action(action, &mut state);
            assert!(state.zones.discard_pile.is_empty());
        }
        for _ in 0..2 {
            let action = state.pop_next_action().unwrap();
            assert!(matches!(
                action,
                Action::MakeTempCardInDiscard {
                    card_id: CardId::Wound,
                    amount: 1,
                    upgraded: false,
                }
            ));
            execute_action(action, &mut state);
        }
        assert_eq!(state.zones.discard_pile.len(), 2);
        assert!(state
            .zones
            .discard_pile
            .iter()
            .all(|card| card.id == CardId::Wound));
        assert!(state.pop_next_action().is_none());
    }

    #[test]
    fn painful_stabs_respects_damage_kind_and_hp_loss_prevention() {
        use crate::content::cards::CardId;
        for damage_type in [DamageType::Normal, DamageType::HpLoss, DamageType::Thorns] {
            for prevention in ["none", "block", "buffer", "tungsten"] {
                let mut state = painful_stabs_combat();
                match prevention {
                    "block" => state.entities.player.block = 1,
                    "buffer" => {
                        state.entities.power_db.insert(
                            0,
                            vec![Power {
                                power_type: PowerId::Buffer,
                                instance_id: None,
                                amount: 1,
                                extra_data: 0,
                                payload: PowerPayload::None,
                                just_applied: false,
                            }],
                        );
                    }
                    "tungsten" => state
                        .entities
                        .player
                        .add_relic(RelicState::new(RelicId::TungstenRod)),
                    _ => {}
                }
                handle_damage(
                    DamageInfo {
                        damage_type,
                        ..thorns_damage(1)
                    },
                    &mut state,
                );
                while let Some(action) = state.pop_next_action() {
                    execute_action(action, &mut state);
                }
                let expected = usize::from(
                    damage_type != DamageType::Thorns
                        && (prevention == "none"
                            || (prevention == "block" && damage_type == DamageType::HpLoss)),
                );
                assert_eq!(
                    state
                        .zones
                        .discard_pile
                        .iter()
                        .filter(|card| card.id == CardId::Wound)
                        .count(),
                    expected,
                    "{damage_type:?}, {prevention}"
                );
            }
        }
    }

    #[test]
    fn player_hp_loss_preserves_block_while_attacks_and_thorns_consume_it() {
        for damage_type in [DamageType::Normal, DamageType::Thorns, DamageType::HpLoss] {
            for block in [0, 2, 7] {
                let mut state = blank_test_combat();
                state.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
                state.entities.player.block = block;
                handle_damage(
                    DamageInfo {
                        damage_type,
                        ..thorns_damage(5)
                    },
                    &mut state,
                );
                let bypass = damage_type == DamageType::HpLoss;
                assert_eq!(
                    state.entities.player.current_hp,
                    80 - if bypass { 5 } else { (5 - block).max(0) }
                );
                assert_eq!(
                    state.entities.player.block,
                    if bypass { block } else { (block - 5).max(0) }
                );
            }
        }
    }

    #[test]
    fn regret_hp_loss_is_not_absorbed_by_player_block() {
        use crate::{
            content::cards::{curses::regret, CardId},
            runtime::combat::CombatCard,
        };
        let mut state = blank_test_combat();
        state.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        state.entities.player.block = 7;
        state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Regret, 0),
            CombatCard::for_scenario(CardId::Strike, 1),
        ];
        for action in regret::on_end_turn_in_hand(&state) {
            execute_action(action.action, &mut state);
        }
        assert_eq!(state.entities.player.current_hp, 78);
        assert_eq!(state.entities.player.block, 7);
    }

    #[test]
    fn player_damage_resolution_reports_block_and_hp_loss_without_subtracting_hp() {
        let mut projected = blank_test_combat();
        projected.entities.player.current_hp = 20;
        projected.entities.player.block = 2;
        let info = thorns_damage(3);

        let resolution = resolve_player_damage(&info, &mut projected);

        assert_eq!(resolution.raw_damage, 3);
        assert_eq!(resolution.block_consumed, 2);
        assert_eq!(resolution.damage_before_hp_loss_hooks, 1);
        assert_eq!(resolution.hp_loss, 1);
        assert_eq!(projected.entities.player.block, 0);
        assert_eq!(projected.entities.player.current_hp, 20);
    }

    #[test]
    fn player_damage_resolution_matches_live_block_and_hp_deltas() {
        let mut projected = blank_test_combat();
        projected.entities.player.current_hp = 20;
        projected.entities.player.block = 2;
        let mut live = projected.clone();
        let info = thorns_damage(3);

        let resolution = resolve_player_damage(&info, &mut projected);
        handle_damage(info, &mut live);

        assert_eq!(live.entities.player.block, projected.entities.player.block);
        assert_eq!(20 - live.entities.player.current_hp, resolution.hp_loss);
    }

    #[test]
    fn player_damage_resolution_reuses_buffer_and_tungsten_hooks() {
        let mut buffered = blank_test_combat();
        buffered.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::Buffer,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        let buffered_resolution = resolve_player_damage(&thorns_damage(3), &mut buffered);

        assert_eq!(buffered_resolution.raw_damage, 3);
        assert_eq!(buffered_resolution.hp_loss, 0);
        assert_eq!(buffered.get_power(0, PowerId::Buffer), 0);

        let mut tungsten = blank_test_combat();
        tungsten
            .entities
            .player
            .add_relic(RelicState::new(RelicId::TungstenRod));

        let tungsten_resolution = resolve_player_damage(&thorns_damage(3), &mut tungsten);

        assert_eq!(tungsten_resolution.damage_before_hp_loss_hooks, 3);
        assert_eq!(tungsten_resolution.hp_loss, 2);
    }

    #[test]
    fn final_thief_death_synchronously_accumulates_both_stolen_gold_rewards() {
        let mut state = blank_test_combat();
        let mut looter = test_monster(EnemyId::Looter);
        looter.thief.stolen_gold = 30;
        for action in
            crate::content::monsters::resolve_on_death(EnemyId::Looter, &mut state, &looter)
        {
            execute_action(action, &mut state);
        }

        let mut mugger = test_monster(EnemyId::Mugger);
        mugger.current_hp = 1;
        mugger.max_hp = 1;
        mugger.thief.stolen_gold = 30;
        state.entities.monsters = vec![mugger];
        handle_damage(
            DamageInfo {
                source: 0,
                target: 1,
                base: 1,
                output: 1,
                damage_type: DamageType::Normal,
                is_modified: false,
            },
            &mut state,
        );

        assert_eq!(
            state.runtime.pending_rewards,
            vec![RewardItem::StolenGold { amount: 60 }]
        );
    }

    #[test]
    fn lethal_darkling_hit_triggers_block_return_before_half_death_clears_powers() {
        for (damage_type, expected_block) in [
            (DamageType::Normal, 2),
            (DamageType::Thorns, 0),
            (DamageType::HpLoss, 0),
        ] {
            let mut state = blank_test_combat();
            let mut marked = test_monster(EnemyId::Darkling);
            marked.current_hp = 1;
            let target = marked.id;
            let mut survivor = test_monster(EnemyId::Darkling);
            survivor.id = target + 1;
            state.entities.monsters = vec![marked, survivor];
            state.entities.power_db.insert(
                target,
                vec![Power {
                    power_type: PowerId::BlockReturnPower,
                    instance_id: None,
                    amount: 2,
                    extra_data: 0,
                    payload: PowerPayload::None,
                    just_applied: false,
                }],
            );
            handle_damage(
                DamageInfo {
                    source: 0,
                    target,
                    base: 1,
                    output: 1,
                    damage_type,
                    is_modified: true,
                },
                &mut state,
            );
            assert!(state.entities.monsters[0].half_dead);
            assert!(!store::has_power(&state, target, PowerId::BlockReturnPower));
            while let Some(action) = state.pop_next_action() {
                execute_action(action, &mut state);
            }
            assert_eq!(
                state.entities.player.block, expected_block,
                "{damage_type:?}"
            );
        }
    }

    #[test]
    fn lethal_damage_does_not_consume_untriggered_curl_up() {
        let mut state = blank_test_combat();
        let mut louse = test_monster(EnemyId::LouseNormal);
        louse.current_hp = 1;
        louse.max_hp = 1;
        let target = louse.id;
        state.entities.monsters = vec![louse];
        state.entities.power_db.insert(
            target,
            vec![Power {
                power_type: PowerId::CurlUp,
                instance_id: None,
                amount: 9,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        handle_damage(
            DamageInfo {
                source: 0,
                target,
                base: 6,
                output: 6,
                damage_type: DamageType::Normal,
                is_modified: false,
            },
            &mut state,
        );

        assert_eq!(state.get_power(target, PowerId::CurlUp), 9);
    }

    #[test]
    fn nonlethal_damage_marks_triggered_curl_up_without_destroying_its_amount() {
        let mut state = blank_test_combat();
        let mut louse = test_monster(EnemyId::LouseNormal);
        louse.current_hp = 10;
        louse.max_hp = 10;
        let target = louse.id;
        state.entities.monsters = vec![louse];
        state.entities.power_db.insert(
            target,
            vec![Power {
                power_type: PowerId::CurlUp,
                instance_id: None,
                amount: 9,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        handle_damage(
            DamageInfo {
                source: 0,
                target,
                base: 6,
                output: 6,
                damage_type: DamageType::Normal,
                is_modified: false,
            },
            &mut state,
        );

        let curl_up = store::powers_for(&state, target)
            .and_then(|powers| {
                powers
                    .iter()
                    .find(|power| power.power_type == PowerId::CurlUp)
            })
            .expect("Curl Up should remain until its queued removal settles");
        assert_eq!(curl_up.amount, 9);
        assert_eq!(curl_up.extra_data, 1);
    }

    #[test]
    fn queued_second_hit_can_kill_louse_before_curl_up_removal_like_java() {
        let mut state = blank_test_combat();
        let mut louse = test_monster(EnemyId::LouseNormal);
        louse.current_hp = 8;
        louse.max_hp = 8;
        let target = louse.id;
        state.entities.monsters = vec![louse];
        state.entities.power_db.insert(
            target,
            vec![Power {
                power_type: PowerId::CurlUp,
                instance_id: None,
                amount: 12,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        for _ in 0..2 {
            state.queue_action_back(Action::Damage(DamageInfo {
                source: 0,
                target,
                base: 5,
                output: 5,
                damage_type: DamageType::Normal,
                is_modified: false,
            }));
        }

        while let Some(action) = state.pop_next_action() {
            execute_action(action, &mut state);
        }

        assert!(state.entities.monsters[0].is_dying);
        assert_eq!(state.entities.monsters[0].current_hp, 0);
        assert_eq!(state.entities.monsters[0].block, 0);
        let curl_up = store::powers_for(&state, target)
            .and_then(|powers| {
                powers
                    .iter()
                    .find(|power| power.power_type == PowerId::CurlUp)
            })
            .expect("dead Java louse keeps the triggered power object");
        assert_eq!(curl_up.amount, 12);
        assert_eq!(curl_up.extra_data, 1);
    }
}
