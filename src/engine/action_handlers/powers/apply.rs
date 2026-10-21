use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, PowerPayload};
pub fn handle_apply_power(
    source: usize,
    target: usize,
    power_id: PowerId,
    amount: i32,
    state: &mut CombatState,
) {
    handle_apply_power_detailed(source, target, power_id, amount, None, None, state);
}

pub fn handle_apply_power_with_payload(
    source: usize,
    target: usize,
    power_id: PowerId,
    amount: i32,
    instance_id: Option<u32>,
    extra_data: Option<i32>,
    payload: PowerPayload,
    state: &mut CombatState,
) {
    handle_apply_power_detailed_internal(
        source,
        target,
        power_id,
        amount,
        instance_id,
        extra_data,
        payload,
        state,
    );
}

pub fn handle_apply_power_detailed(
    source: usize,
    target: usize,
    power_id: PowerId,
    amount: i32,
    instance_id: Option<u32>,
    extra_data: Option<i32>,
    state: &mut CombatState,
) {
    handle_apply_power_detailed_internal(
        source,
        target,
        power_id,
        amount,
        instance_id,
        extra_data,
        PowerPayload::None,
        state,
    );
}

fn handle_apply_power_detailed_internal(
    source: usize,
    target: usize,
    power_id: PowerId,
    mut amount: i32,
    instance_id: Option<u32>,
    extra_data: Option<i32>,
    payload: PowerPayload,
    state: &mut CombatState,
) {
    amount = crate::content::powers::canonicalize_applied_amount(power_id, amount);
    let initial_just_applied =
        crate::content::powers::initial_just_applied(power_id, target, state.turn.current_phase);

    // C1: Snake Skull → +1 Poison
    if amount > 0
        && power_id == PowerId::Poison
        && state
            .entities
            .player
            .has_relic(crate::content::relics::RelicId::SneckoSkull)
        && source == 0
        && target != 0
    {
        amount += 1;
    }

    // U1: Java ApplyPowerAction.update(): target.isDeadOrEscaped() returns true
    // for dying, escaped, and half-dead monsters. It does not check currentHealth.
    if target == 0 {
        // Player target — always valid
    } else if let Some(m) = state.entities.monsters.iter().find(|m| m.id == target) {
        if m.is_dead_or_escaped() {
            return;
        }
    }

    // U3: onApplyPower hooks (Sadistic Nature)
    if source != 0 || target != 0 {
        if let Some(source_powers) = store::powers_for(state, source).map(|powers| powers.to_vec())
        {
            for power in &source_powers {
                let hook_actions = crate::content::powers::resolve_power_on_apply_power(
                    power.power_type,
                    power.amount,
                    power_id,
                    amount,
                    target,
                    source,
                    state,
                );
                let hook_actions_4: smallvec::SmallVec<[crate::runtime::action::ActionInfo; 4]> =
                    hook_actions.into_iter().collect();
                state.queue_actions(hook_actions_4);
            }
        }
    }

    // U4: Champion Belt
    let champion_belt_actions =
        crate::content::relics::hooks::on_apply_power(state, source, power_id, target);
    state.queue_actions(champion_belt_actions);

    // U5: Monster re-check after hooks
    if target != 0 {
        if let Some(m) = state.entities.monsters.iter().find(|m| m.id == target) {
            if m.is_dead_or_escaped() {
                return;
            }
        }
    }

    // U6+U7: Ginger/Turnip
    if target == 0 {
        amount = crate::content::relics::hooks::on_receive_power_modify(state, power_id, amount);
        if amount == 0 && crate::content::powers::is_debuff(power_id, amount) {
            return;
        }
    }

    // U8: Artifact blocks actual debuff applications, not debuff cleanup like Weak -1.
    if crate::content::powers::is_debuff_application(power_id, amount) {
        let has_artifact = store::powers_for(state, target)
            .is_some_and(|powers| powers.iter().any(|p| p.power_type == PowerId::Artifact));
        if has_artifact {
            if let Some(amount) = store::with_power_mut(state, target, PowerId::Artifact, |art| {
                art.amount -= 1;
                art.amount
            }) {
                if amount <= 0 {
                    store::remove_power_type(state, target, PowerId::Artifact);
                }
            }
            return;
        }
    }

    // Java ApplyPowerAction: NoDrawPower is the duplicate-application special
    // case. Other sentinel amount powers still flow through stackPower(-1).
    if crate::content::powers::reapplying_existing_power_is_noop(power_id)
        && store::powers_for(state, target)
            .is_some_and(|powers| powers.iter().any(|p| p.power_type == power_id))
    {
        return;
    }

    let had_existing_power = store::has_power(state, target, power_id);
    if power_id == PowerId::Mantra && target == 0 {
        state.turn.counters.mantra_gained_this_combat += amount;
    }

    // Core power application
    // These powers capture their constructor source. Stacking changes only
    // the amount on the existing object, so it must retain that first source.
    let payload = if matches!(power_id, PowerId::Constricted | PowerId::Poison) {
        PowerPayload::SourceEntity(source)
    } else {
        payload
    };
    let powers = store::ensure_powers_for_mut(state, target);
    let mut should_remove_existing = false;
    if crate::content::powers::uses_distinct_instances(power_id) {
        if let Some(instance_id) = instance_id {
            if let Some(existing) = powers
                .iter_mut()
                .find(|p| p.power_type == power_id && p.instance_id == Some(instance_id))
            {
                existing.amount += amount;
                if let Some(extra_data) = extra_data {
                    existing.extra_data = extra_data;
                }
                if !matches!(payload, PowerPayload::None) {
                    existing.payload = payload;
                }
                if !crate::content::powers::should_keep_power_instance(power_id, existing.amount) {
                    should_remove_existing = true;
                }
            } else if crate::content::powers::should_keep_power_instance(power_id, amount) {
                powers.push(crate::runtime::combat::Power {
                    power_type: power_id,
                    instance_id: Some(instance_id),
                    amount,
                    extra_data: extra_data.unwrap_or(0),
                    payload,
                    just_applied: initial_just_applied,
                });
            }
        } else if let Some(existing) = powers.iter_mut().find(|p| p.power_type == power_id) {
            existing.amount += amount;
            if let Some(extra_data) = extra_data {
                existing.extra_data = extra_data;
            }
            if !matches!(payload, PowerPayload::None) {
                existing.payload = payload;
            }
            if !crate::content::powers::should_keep_power_instance(power_id, existing.amount) {
                should_remove_existing = true;
            }
        } else if crate::content::powers::should_keep_power_instance(power_id, amount) {
            powers.push(crate::runtime::combat::Power {
                power_type: power_id,
                instance_id: None,
                amount,
                extra_data: extra_data.unwrap_or(0),
                payload,
                just_applied: initial_just_applied,
            });
        }
    } else if let Some(existing) = powers.iter_mut().find(|p| p.power_type == power_id) {
        if matches!(
            power_id,
            PowerId::Strength | PowerId::Dexterity | PowerId::Focus
        ) {
            // These Java stackPower overrides cap the result, not the incoming
            // action amount. Keep Java integer arithmetic before applying caps.
            existing.amount = existing.amount.wrapping_add(amount).clamp(-999, 999);
        } else if power_id == PowerId::Combust {
            existing.amount += amount;
            existing.extra_data +=
                crate::content::cards::mechanics::COMBUST_HP_LOSS_PER_APPLICATION_V1;
        } else if power_id == PowerId::PanachePower && amount > 0 {
            // Panache has two distinct positive-amount paths:
            //   1. card application / stacking: amount is damage (10/14), damage stacks
            //   2. countdown reset: internal ApplyPower(+1..+4), amount should reset countdown
            if amount <= 4 && existing.amount < 5 {
                existing.amount = (existing.amount + amount).min(5);
            } else {
                existing.extra_data += amount;
            }
        } else if power_id == PowerId::Malleable && amount > 0 {
            existing.amount += amount;
            existing.extra_data += amount;
        } else if power_id == PowerId::Flight && amount > 0 {
            existing.amount += amount;
            existing.extra_data = existing.amount.max(existing.extra_data);
        } else if power_id == PowerId::LikeWaterPower && amount > 0 {
            existing.amount = (existing.amount + amount).min(999);
        } else if power_id == PowerId::DevaForm && amount > 0 {
            existing.amount += amount;
            existing.extra_data += 1;
        } else if matches!(power_id, PowerId::Energized | PowerId::EnergizedBlue) {
            existing.amount = existing.amount.wrapping_add(amount).min(999);
        } else if power_id == PowerId::CollectPower && amount > 0 {
            existing.amount = (existing.amount + amount).min(999);
        } else if power_id == PowerId::Poison {
            // PoisonPower delegates stacking to AbstractPower without a cap.
            existing.amount = existing.amount.wrapping_add(amount);
        } else if !crate::content::powers::uses_sentinel_amount(power_id) || existing.amount != -1 {
            // AbstractPower.stackPower returns immediately for its -1 marker.
            // Keep the application hooks; only its amount update is a no-op.
            existing.amount += amount;
        }

        // Strength/Dexterity/Focus can go negative, but Java still removes them at exactly 0.
        if !crate::content::powers::should_keep_power_instance(power_id, existing.amount) {
            should_remove_existing = true;
        }
    } else {
        // If they don't have it, we only add it if amount > 0, OR if it's a negative amount of a power that CAN go negative
        if crate::content::powers::should_keep_power_instance(power_id, amount) {
            let (stored_amount, extra_data) = match power_id {
                // Strength/Dexterity constructors cap too. Focus's constructor
                // does not, even though its stackPower implementation does.
                PowerId::Strength | PowerId::Dexterity => (amount.clamp(-999, 999), 0),
                PowerId::PanachePower => (5, amount),
                PowerId::Combust => (
                    amount,
                    crate::content::cards::mechanics::COMBUST_HP_LOSS_PER_APPLICATION_V1,
                ),
                PowerId::Malleable => (amount, amount),
                PowerId::Flight => (amount, amount),
                PowerId::DevaForm => (amount, 1),
                PowerId::CollectPower => (amount.min(999), 0),
                PowerId::Poison => (amount.min(9999), 0),
                PowerId::Invincible => (amount, amount),
                PowerId::Rebound => (amount, extra_data.unwrap_or(1)),
                _ if extra_data.is_some() => (amount, extra_data.unwrap_or(0)),
                _ => (amount, 0),
            };
            powers.push(crate::runtime::combat::Power {
                power_type: power_id,
                instance_id: None,
                amount: stored_amount,
                extra_data,
                payload,
                just_applied: initial_just_applied,
            });
        }
    }
    if should_remove_existing {
        if let Some(instance_id) = instance_id {
            store::remove_power_instance(state, target, power_id, instance_id);
        } else {
            store::remove_power_type(state, target, power_id);
        }
    } else {
        store::sort_powers_for_java(state, target);
    }

    if power_id == PowerId::Surrounded && target == 0 {
        crate::content::powers::core::surrounded::sync_back_attack_markers(state);
    }

    // Java MantraPower.stackPower(): only stacking an existing Mantra power
    // checks for the 10-point threshold, then subtracts 10 and enters Divinity.
    if power_id == PowerId::Mantra && target == 0 && had_existing_power {
        let current = store::power_amount(state, target, PowerId::Mantra);
        if current >= 10 {
            let remainder = current - 10;
            if remainder > 0 {
                let _ = store::with_power_mut(state, target, PowerId::Mantra, |power| {
                    power.amount = remainder;
                });
            } else {
                store::remove_power_type(state, target, PowerId::Mantra);
            }
            state.queue_action_front(Action::EnterStance("Divinity".to_string()));
        }
    }

    if target == 0 {
        state.recompute_turn_start_draw_modifier();
    }
    super::refresh_player_focus(target, power_id, state);
}

#[cfg(test)]
mod tests {
    use super::handle_apply_power;
    use crate::content::powers::{resolve_power_at_end_of_round, store, PowerId};
    use crate::runtime::action::Action;
    use crate::runtime::combat::CombatPhase;

    #[test]
    fn nonstacking_sentinels_remain_minus_one_without_suppressing_signed_stat_stacks() {
        for id in [PowerId::Electro, PowerId::Barricade, PowerId::Corruption] {
            let mut state = crate::test_support::blank_test_combat();
            for _ in 0..3 {
                handle_apply_power(0, 0, id, -1, &mut state);
                assert_eq!(store::power_amount(&state, 0, id), -1);
                assert_eq!(store::powers_for(&state, 0).unwrap().len(), 1);
            }
        }
        for id in [
            PowerId::Strength,
            PowerId::Dexterity,
            PowerId::Focus,
            PowerId::Draw,
            PowerId::WraithForm,
        ] {
            let mut state = crate::test_support::blank_test_combat();
            handle_apply_power(0, 0, id, -1, &mut state);
            handle_apply_power(0, 0, id, 2, &mut state);
            assert_eq!(store::power_amount(&state, 0, id), 1);
        }
    }

    #[test]
    fn focus_mutations_refresh_existing_orbs_immediately_without_resetting_dark_charge() {
        use crate::engine::action_handlers::powers::{handle_reduce_power, handle_remove_power};
        use crate::runtime::combat::{OrbEntity, OrbId};
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.max_orbs = 5;
        state.entities.player.orbs = [
            OrbId::Lightning,
            OrbId::Frost,
            OrbId::Dark,
            OrbId::Plasma,
            OrbId::Empty,
        ]
        .into_iter()
        .map(OrbEntity::new)
        .collect();
        state.entities.player.orbs[2].evoke_amount = 42;
        let values = |state: &crate::runtime::combat::CombatState| {
            state
                .entities
                .player
                .orbs
                .iter()
                .map(|orb| (orb.passive_amount, orb.evoke_amount))
                .collect::<Vec<_>>()
        };
        handle_apply_power(0, 0, PowerId::Focus, 2, &mut state);
        assert_eq!(values(&state), [(5, 10), (4, 7), (8, 42), (1, 2), (0, 0)]);
        handle_apply_power(0, 0, PowerId::Focus, -5, &mut state);
        assert_eq!(values(&state), [(0, 5), (0, 2), (3, 42), (1, 2), (0, 0)]);
        handle_reduce_power(0, PowerId::Focus, 1, &mut state);
        assert_eq!(values(&state), [(0, 4), (0, 1), (2, 42), (1, 2), (0, 0)]);
        let base = [(3, 8), (2, 5), (6, 42), (1, 2), (0, 0)];
        handle_apply_power(0, 0, PowerId::Focus, 4, &mut state);
        assert!(!store::has_power(&state, 0, PowerId::Focus));
        assert_eq!(values(&state), base);
        handle_apply_power(0, 0, PowerId::Focus, 2, &mut state);
        handle_remove_power(0, PowerId::Focus, &mut state);
        assert_eq!(values(&state), base);
        handle_apply_power(0, 0, PowerId::Focus, 2, &mut state);
        handle_reduce_power(0, PowerId::Focus, 2, &mut state);
        assert_eq!(values(&state), base);
    }

    #[test]
    fn signed_stat_application_matches_java_constructor_and_stack_caps() {
        for id in [PowerId::Strength, PowerId::Dexterity, PowerId::Focus] {
            for initial in [2000, -2000] {
                let mut state = crate::test_support::blank_test_combat();
                handle_apply_power(0, 0, id, initial, &mut state);
                let expected = if id == PowerId::Focus {
                    initial
                } else {
                    initial.clamp(-999, 999)
                };
                assert_eq!(store::power_amount(&state, 0, id), expected, "{id:?}");
                handle_apply_power(0, 0, id, initial.signum(), &mut state);
                assert_eq!(store::power_amount(&state, 0, id), initial.signum() * 999);
            }

            let mut state = crate::test_support::blank_test_combat();
            handle_apply_power(0, 0, id, -500, &mut state);
            handle_apply_power(0, 0, id, 2000, &mut state);
            assert_eq!(
                store::power_amount(&state, 0, id),
                999,
                "cap the sum, not the incoming amount"
            );
            handle_apply_power(0, 0, id, -3000, &mut state);
            assert_eq!(store::power_amount(&state, 0, id), -999);
            handle_apply_power(0, 0, id, 999, &mut state);
            assert!(!store::has_power(&state, 0, id), "zero removes {id:?}");
        }
    }

    #[test]
    fn repeated_strength_doubling_stays_capped_and_artifact_still_blocks_loss() {
        let mut state = crate::test_support::blank_test_combat();
        handle_apply_power(0, 0, PowerId::Strength, 1, &mut state);
        // The old application path overflows after 31 Limit Break doublings.
        for n in 1..=40 {
            let amount = store::power_amount(&state, 0, PowerId::Strength);
            handle_apply_power(0, 0, PowerId::Strength, amount, &mut state);
            assert_eq!(
                store::power_amount(&state, 0, PowerId::Strength),
                if n < 10 { 1 << n } else { 999 }
            );
        }
        handle_apply_power(0, 0, PowerId::Artifact, 1, &mut state);
        handle_apply_power(1, 0, PowerId::Strength, -1500, &mut state);
        assert_eq!(store::power_amount(&state, 0, PowerId::Strength), 999);
        assert!(!store::has_power(&state, 0, PowerId::Artifact));
        handle_apply_power(1, 0, PowerId::Strength, -1500, &mut state);
        assert_eq!(store::power_amount(&state, 0, PowerId::Strength), -501);
    }

    #[test]
    fn live_power_application_preserves_java_decay_start() {
        let mut state = crate::test_support::blank_test_combat();
        state.turn.current_phase = CombatPhase::PlayerTurn;

        handle_apply_power(0, 0, PowerId::NoBlock, 2, &mut state);
        let no_block = store::powers_snapshot_for(&state, 0)
            .into_iter()
            .find(|power| power.power_type == PowerId::NoBlock)
            .expect("Panic Button should apply NoBlock");
        assert!(!no_block.just_applied);
        assert_eq!(
            resolve_power_at_end_of_round(
                PowerId::NoBlock,
                &state,
                0,
                no_block.amount,
                no_block.just_applied,
            )
            .as_slice(),
            &[Action::ReducePower {
                target: 0,
                power_id: PowerId::NoBlock,
                amount: 1,
            }]
        );

        handle_apply_power(0, 1, PowerId::Weak, 2, &mut state);
        let weak = store::powers_snapshot_for(&state, 1)
            .into_iter()
            .find(|power| power.power_type == PowerId::Weak)
            .expect("player Weak should apply to the monster");
        assert!(!weak.just_applied);

        state.turn.current_phase = CombatPhase::MonsterTurn;
        handle_apply_power(1, 0, PowerId::Frail, 2, &mut state);
        let frail = store::powers_snapshot_for(&state, 0)
            .into_iter()
            .find(|power| power.power_type == PowerId::Frail)
            .expect("monster Frail should apply to the player");
        assert!(frail.just_applied);
    }

    #[test]
    fn player_vulnerable_distinguishes_end_turn_card_cleanup_from_monster_turn() {
        let mut cleanup = crate::test_support::blank_test_combat();
        cleanup.turn.current_phase = CombatPhase::TurnTransition;
        handle_apply_power(1, 0, PowerId::Vulnerable, 2, &mut cleanup);
        let cleanup_vulnerable = store::powers_snapshot_for(&cleanup, 0)
            .into_iter()
            .find(|power| power.power_type == PowerId::Vulnerable)
            .expect("Spore Cloud should apply Vulnerable during card cleanup");
        assert!(!cleanup_vulnerable.just_applied);
        assert_eq!(
            resolve_power_at_end_of_round(
                PowerId::Vulnerable,
                &cleanup,
                0,
                cleanup_vulnerable.amount,
                cleanup_vulnerable.just_applied,
            )
            .as_slice(),
            &[Action::ReducePower {
                target: 0,
                power_id: PowerId::Vulnerable,
                amount: 1,
            }]
        );

        let mut monster_turn = crate::test_support::blank_test_combat();
        monster_turn.turn.current_phase = CombatPhase::MonsterTurn;
        handle_apply_power(1, 0, PowerId::Vulnerable, 2, &mut monster_turn);
        let monster_vulnerable = store::powers_snapshot_for(&monster_turn, 0)
            .into_iter()
            .find(|power| power.power_type == PowerId::Vulnerable)
            .expect("a monster move should apply Vulnerable to the player");
        assert!(monster_vulnerable.just_applied);
        assert!(resolve_power_at_end_of_round(
            PowerId::Vulnerable,
            &monster_turn,
            0,
            monster_vulnerable.amount,
            monster_vulnerable.just_applied,
        )
        .is_empty());
    }
}
