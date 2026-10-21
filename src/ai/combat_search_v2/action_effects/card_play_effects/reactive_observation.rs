use crate::content::powers::store::powers_snapshot_for;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};

use super::monster_signals::{is_living_monster_id, visible_strength_gain_pressure_hint};
use super::observation::CardPlayEffectAccumulator;

pub(super) fn observe_card_play_reactive_actions(
    combat: &CombatState,
    card: &CombatCard,
    accumulator: &mut CardPlayEffectAccumulator,
) {
    let trigger_owners = std::iter::once(0usize)
        .chain(combat.entities.monsters.iter().map(|monster| monster.id))
        .collect::<Vec<_>>();
    for owner in trigger_owners {
        for power in powers_snapshot_for(combat, owner) {
            let actions = crate::content::powers::resolve_power_on_card_played(
                power.power_type,
                combat,
                owner,
                card,
                power.amount,
            );
            for action in actions {
                observe_reactive_action(combat, accumulator, action);
            }
        }
    }
}

fn observe_reactive_action(
    combat: &CombatState,
    accumulator: &mut CardPlayEffectAccumulator,
    action: Action,
) {
    match action {
        Action::ApplyPower {
            target,
            power_id,
            amount,
            ..
        }
        | Action::ApplyPowerDetailed {
            target,
            power_id,
            amount,
            ..
        }
        | Action::ApplyPowerWithPayload {
            target,
            power_id,
            amount,
            ..
        } => observe_reactive_apply_power(combat, accumulator, target, power_id, amount),
        Action::Damage(info)
        | Action::PummelDamage(info)
        | Action::BaneDamage(info)
        | Action::WallopDamage(info)
        | Action::DamagePerAttackPlayed(info)
        | Action::DropkickDamageAndEffect {
            damage_info: info, ..
        }
        | Action::Ftl {
            damage_info: info, ..
        }
        | Action::Skewer {
            damage_info: info, ..
        }
        | Action::VampireDamage(info)
        | Action::Barrage { damage: info }
        | Action::Sunder {
            damage_info: info, ..
        }
        | Action::FearNoEvil {
            damage_info: info, ..
        }
        | Action::FiendFire {
            damage_info: info, ..
        }
        | Action::Feed {
            damage_info: info, ..
        }
        | Action::LessonLearned {
            damage_info: info, ..
        }
        | Action::HandOfGreed {
            damage_info: info, ..
        }
        | Action::RitualDagger {
            damage_info: info, ..
        } => observe_reactive_damage_info(
            combat,
            accumulator,
            info.target,
            info.output.max(info.base),
        ),
        Action::LoseHp { target, amount, .. } | Action::PoisonLoseHp { target, amount, .. } => {
            observe_reactive_hp_loss(combat, accumulator, target, amount)
        }
        Action::DamageAllEnemies { damages, .. }
        | Action::VampireDamageAllEnemies { damages, .. } => {
            for (slot, damage) in damages.into_iter().enumerate() {
                if let Some(monster) = combat.entities.monsters.get(slot) {
                    observe_reactive_hp_loss(combat, accumulator, monster.id, damage);
                }
            }
        }
        Action::GainBlock { target: 0, amount } => {
            accumulator.reactive.player_block = accumulator
                .reactive
                .player_block
                .saturating_add(amount.max(0));
        }
        Action::MakeTempCardInDrawPile {
            card_id, amount, ..
        } => {
            if generated_card_is_bad_draw(card_id) {
                accumulator.reactive.bad_draw_cards = accumulator
                    .reactive
                    .bad_draw_cards
                    .saturating_add(i32::from(amount));
            }
        }
        Action::TriggerTimeWarpEndTurn { .. } => {
            accumulator.reactive.forced_turn_end = true;
        }
        _ => {}
    }
}

fn observe_reactive_apply_power(
    combat: &CombatState,
    accumulator: &mut CardPlayEffectAccumulator,
    target: usize,
    power_id: PowerId,
    amount: i32,
) {
    match power_id {
        PowerId::Strength if target != 0 && amount > 0 && is_living_monster_id(combat, target) => {
            accumulator.reactive.enemy_strength_gain = accumulator
                .reactive
                .enemy_strength_gain
                .saturating_add(amount);
            accumulator.reactive.visible_attack_pressure_hint = accumulator
                .reactive
                .visible_attack_pressure_hint
                .saturating_add(visible_strength_gain_pressure_hint(combat, target, amount));
        }
        PowerId::Weak if target != 0 && amount > 0 && is_living_monster_id(combat, target) => {
            accumulator.reactive.enemy_weak =
                accumulator.reactive.enemy_weak.saturating_add(amount);
        }
        PowerId::Vulnerable
            if target != 0 && amount > 0 && is_living_monster_id(combat, target) =>
        {
            accumulator.reactive.enemy_vulnerable =
                accumulator.reactive.enemy_vulnerable.saturating_add(amount);
        }
        PowerId::Slow if target != 0 && amount > 0 && is_living_monster_id(combat, target) => {
            accumulator.reactive.future_attack_damage_percent_gain_hint = accumulator
                .reactive
                .future_attack_damage_percent_gain_hint
                .max(amount.saturating_mul(10));
        }
        _ => {}
    }
}

fn observe_reactive_damage_info(
    combat: &CombatState,
    accumulator: &mut CardPlayEffectAccumulator,
    target: usize,
    amount: i32,
) {
    observe_reactive_hp_loss(combat, accumulator, target, amount);
}

fn observe_reactive_hp_loss(
    combat: &CombatState,
    accumulator: &mut CardPlayEffectAccumulator,
    target: usize,
    amount: i32,
) {
    let amount = amount.max(0);
    if amount == 0 {
        return;
    }
    if target == 0 {
        accumulator.reactive.player_hp_loss =
            accumulator.reactive.player_hp_loss.saturating_add(amount);
    } else if is_living_monster_id(combat, target) {
        accumulator.reactive.enemy_damage =
            accumulator.reactive.enemy_damage.saturating_add(amount);
    }
}

fn generated_card_is_bad_draw(card_id: crate::content::cards::CardId) -> bool {
    let def = crate::content::cards::get_card_definition(card_id);
    matches!(
        def.card_type,
        crate::content::cards::CardType::Status | crate::content::cards::CardType::Curse
    )
}
