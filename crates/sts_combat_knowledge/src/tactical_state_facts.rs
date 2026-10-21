//! Mechanics-aware state facts shared by tactical guide lanes.
//!
//! These facts inspect one exact simulator state. They do not order frontier
//! work, choose an action, run a rollout, or claim a terminal result.

use sts_card_semantics::ai::analysis::card_semantics::{
    card_definition_with_upgrades as strategic_card_definition, CombatEvent, PlayEffect,
};
use sts_core::content::cards::{exhausts_when_played, get_card_definition, CardId, CardType};
use sts_core::content::monsters::EnemyId;
use sts_core::content::powers::{store, PowerId};
use sts_core::content::relics::RelicId;
use sts_core::runtime::combat::{CombatCard, CombatState, Power};
use sts_core::sim::combat::CombatPosition;
use sts_core::state::core::EngineState;
use sts_core::EntityId;

/// Progress against encounter members whose defeat owns a persistent
/// strategic payoff.
///
/// Looter and Mugger damage matters because concentrating damage on either
/// thief is the exact corridor that can prevent or recover stolen gold. Donu
/// is the other current owner: its death removes the Donu/Deca encounter's
/// team-wide Strength clock. This remains a guide fact, never a forced target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatPriorityOwnerProgressV1 {
    pub completed_targets: i32,
    pub focused_damage: i32,
    pub total_damage: i32,
}

pub fn combat_priority_owner_progress_v1(combat: &CombatState) -> CombatPriorityOwnerProgressV1 {
    let mut progress = CombatPriorityOwnerProgressV1::default();
    for thief in combat
        .entities
        .monsters
        .iter()
        .filter(|monster| matches!(monster.enemy_id, EnemyId::Looter | EnemyId::Mugger))
    {
        let damage = thief.max_hp.saturating_sub(thief.current_hp.max(0)).max(0);
        progress.completed_targets = progress
            .completed_targets
            .saturating_add(i32::from(thief.current_hp <= 0));
        progress.focused_damage = progress.focused_damage.max(damage);
        progress.total_damage = progress.total_damage.saturating_add(damage);
    }
    let has_deca = combat
        .entities
        .monsters
        .iter()
        .any(|monster| monster.enemy_id == EnemyId::Deca);
    if !has_deca {
        return progress;
    }
    if let Some(donu) = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.enemy_id == EnemyId::Donu)
    {
        let damage = donu.max_hp.saturating_sub(donu.current_hp.max(0)).max(0);
        progress.completed_targets = progress
            .completed_targets
            .saturating_add(i32::from(donu.current_hp <= 0));
        progress.focused_damage = progress.focused_damage.max(damage);
        progress.total_damage = progress.total_damage.saturating_add(damage);
    }
    progress
}

/// Realized player setup and its remaining exact fuel.
///
/// This describes the current state rather than rewarding the action that
/// created it. Conditional icons alone do not count as deterministic output.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatPlayerSetupSummaryV1 {
    pub exhaust_engine_connected: i32,
    pub status_access_engine_connected: i32,
    pub exhaust_engine_fuel: i32,
    pub exhaust_payoff_engine_count: i32,
    pub exhaust_payoff_engine_fuel: i32,
    pub recurring_output_count: i32,
    pub recurring_output_mass: i32,
    pub active_power_count: i32,
    pub active_power_mass: i32,
}

pub fn combat_player_setup_summary_v1(combat: &CombatState) -> CombatPlayerSetupSummaryV1 {
    let player = combat.entities.player.id;
    let (active_power_count, active_power_mass, recurring_output_count, recurring_output_mass) =
        store::powers_for(combat, player)
            .into_iter()
            .flatten()
            .filter_map(|power| {
                realized_player_setup_power_amount(combat, player, power)
                    .map(|amount| (power.power_type, amount))
            })
            .fold(
                (0_i32, 0_i32, 0_i32, 0_i32),
                |(active_count, active_mass, recurring_count, recurring_mass), (power, amount)| {
                    let recurring =
                        i32::from(player_power_has_deterministic_recurring_output(power));
                    (
                        active_count.saturating_add(1),
                        active_mass.saturating_add(amount),
                        recurring_count.saturating_add(recurring),
                        recurring_mass.saturating_add(amount.saturating_mul(recurring)),
                    )
                },
            );
    let unexhausted_cards = combat
        .zones
        .hand
        .iter()
        .chain(&combat.zones.draw_pile)
        .chain(&combat.zones.discard_pile)
        .collect::<Vec<_>>();
    let remaining_statuses = unexhausted_cards
        .iter()
        .filter(|card| get_card_definition(card.id).card_type == CardType::Status)
        .count()
        .try_into()
        .unwrap_or(i32::MAX);
    let status_access_engine_connected =
        i32::from(remaining_statuses > 0 && store::has_power(combat, player, PowerId::Evolve));
    let corruption_active = store::has_power(combat, player, PowerId::Corruption);
    let remaining_exhaust_event_sources = unexhausted_cards
        .iter()
        .filter(|card| {
            corruption_active && get_card_definition(card.id).card_type == CardType::Skill
                || card_can_emit_exhaust_event(card)
        })
        .count()
        .try_into()
        .unwrap_or(i32::MAX);
    let exhaust_engine_connected = i32::from(
        remaining_exhaust_event_sources > 0
            && store::has_power(combat, player, PowerId::DarkEmbrace),
    );
    let exhaust_payoff_engine_count = if remaining_exhaust_event_sources > 0 {
        i32::from(store::has_power(combat, player, PowerId::FeelNoPain)).saturating_add(i32::from(
            store::has_power(combat, player, PowerId::DarkEmbrace),
        ))
    } else {
        0
    };
    CombatPlayerSetupSummaryV1 {
        exhaust_engine_connected,
        status_access_engine_connected,
        exhaust_engine_fuel: remaining_exhaust_event_sources
            .saturating_mul(exhaust_engine_connected),
        exhaust_payoff_engine_count,
        exhaust_payoff_engine_fuel: remaining_exhaust_event_sources
            .saturating_mul(exhaust_payoff_engine_count),
        recurring_output_count,
        recurring_output_mass,
        active_power_count,
        active_power_mass,
    }
}

/// Categorical current-state opportunities used by the independent setup
/// guide. They are service hints, not permission to spend HP and not claims
/// that either line wins.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatPlayerTacticalOpportunitySummaryV1 {
    pub runic_cube_emergency_draw_conversion: i32,
    pub dark_embrace_wound_access_conversion: i32,
}

pub fn combat_player_tactical_opportunity_summary_v1(
    position: &CombatPosition,
    survival_margin: i32,
) -> CombatPlayerTacticalOpportunitySummaryV1 {
    CombatPlayerTacticalOpportunitySummaryV1 {
        runic_cube_emergency_draw_conversion: i32::from(
            runic_cube_emergency_draw_conversion_ready(position, survival_margin),
        ),
        dark_embrace_wound_access_conversion: i32::from(
            dark_embrace_wound_access_conversion_ready(position, survival_margin),
        ),
    }
}

pub fn connected_second_wind_wound_engine_v1(combat: &CombatState) -> bool {
    let player = combat.entities.player.id;
    store::has_power(combat, player, PowerId::DarkEmbrace)
        && active_unexhausted_cards(combat).any(|card| card.id == CardId::SecondWind)
        && active_unexhausted_cards(combat).any(|card| card.id == CardId::PowerThrough)
}

fn active_unexhausted_cards(combat: &CombatState) -> impl Iterator<Item = &CombatCard> {
    combat
        .zones
        .hand
        .iter()
        .chain(&combat.zones.draw_pile)
        .chain(&combat.zones.discard_pile)
}

fn dark_embrace_wound_access_conversion_ready(
    position: &CombatPosition,
    survival_margin: i32,
) -> bool {
    let combat = &position.combat;
    if !matches!(&position.engine, EngineState::CombatPlayerTurn)
        || survival_margin >= 0
        || !connected_second_wind_wound_engine_v1(combat)
    {
        return false;
    }
    let burning_pact_affordable = combat.zones.hand.iter().any(|card| {
        card.id == CardId::BurningPact
            && nonnegative_card_cost(card).is_some_and(|cost| cost <= i32::from(combat.turn.energy))
    });
    let has_wound = combat
        .zones
        .hand
        .iter()
        .any(|card| card.id == CardId::Wound);
    let has_non_wound_exhaust_target = combat
        .zones
        .hand
        .iter()
        .any(|card| !matches!(card.id, CardId::BurningPact | CardId::Wound));
    burning_pact_affordable && has_wound && has_non_wound_exhaust_target
}

fn runic_cube_emergency_draw_conversion_ready(
    position: &CombatPosition,
    survival_margin: i32,
) -> bool {
    let combat = &position.combat;
    if !matches!(&position.engine, EngineState::CombatPlayerTurn)
        || survival_margin >= 0
        || !combat.entities.player.has_relic(RelicId::RunicCube)
    {
        return false;
    }
    let Some(power_through) = combat
        .zones
        .draw_pile
        .first()
        .filter(|card| card.id == CardId::PowerThrough)
    else {
        return false;
    };
    let Some(second_wind_cost) = combat
        .zones
        .hand
        .iter()
        .filter(|card| card.id == CardId::SecondWind)
        .filter_map(nonnegative_card_cost)
        .min()
    else {
        return false;
    };
    let Some(power_through_cost) = nonnegative_card_cost(power_through) else {
        return false;
    };
    let energy_after_conversion = i32::from(combat.turn.energy)
        .saturating_sub(second_wind_cost)
        .saturating_sub(power_through_cost);
    if energy_after_conversion < 0 {
        return false;
    }
    let has_affordable_attack = combat.zones.hand.iter().any(|card| {
        get_card_definition(card.id).card_type == CardType::Attack
            && nonnegative_card_cost(card).is_some_and(|cost| cost <= energy_after_conversion)
    });
    if !has_affordable_attack {
        return false;
    }
    let retaliation = combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| {
            store::power_amount(combat, monster.id, PowerId::Thorns)
                .max(0)
                .saturating_add(store::power_amount(combat, monster.id, PowerId::SharpHide).max(0))
        })
        .max()
        .unwrap_or_default();
    let hp_loss = retaliation.saturating_sub(combat.entities.player.block);
    hp_loss > 0 && hp_loss < combat.entities.player.current_hp
}

fn nonnegative_card_cost(card: &CombatCard) -> Option<i32> {
    let cost = card.cost_for_turn_java();
    (cost >= 0).then_some(cost)
}

fn card_can_emit_exhaust_event(card: &CombatCard) -> bool {
    if exhausts_when_played(card) {
        return true;
    }
    strategic_card_definition(card.id, card.upgrades)
        .play_effects
        .iter()
        .any(|effect| {
            matches!(
                effect,
                PlayEffect::EmitEvent(CombatEvent::CardExhausted)
                    | PlayEffect::PlayTopCardAndExhaust
            )
        })
}

/// Powers in this set have already paid their setup cost and deterministically
/// produce a useful resource or combat effect on later turns. Conditional
/// engines, one-shot effects, and recurring effects with an HP cost stay out.
fn player_power_has_deterministic_recurring_output(power: PowerId) -> bool {
    matches!(
        power,
        PowerId::Ritual
            | PowerId::DemonForm
            | PowerId::Metallicize
            | PowerId::BattleHymnPower
            | PowerId::DevotionPower
            | PowerId::DevaForm
            | PowerId::OmegaPower
            | PowerId::NoxiousFumes
            | PowerId::InfiniteBladesPower
            | PowerId::ToolsOfTheTrade
    )
}

fn realized_player_setup_power_amount(
    combat: &CombatState,
    player: EntityId,
    power: &Power,
) -> Option<i32> {
    if !player_power_is_positive_setup(power.power_type, power.amount) {
        return None;
    }
    let amount = if sts_core::content::powers::uses_sentinel_amount(power.power_type) {
        1
    } else {
        power.amount.clamp(1, 12)
    };
    let scheduled_rollback = match power.power_type {
        PowerId::Strength => store::power_amount(combat, player, PowerId::LoseStrength),
        PowerId::Dexterity => store::power_amount(combat, player, PowerId::DexterityDown),
        _ => 0,
    }
    .max(0);
    let realized = amount.saturating_sub(scheduled_rollback);
    (realized > 0).then_some(realized)
}

fn player_power_is_positive_setup(power: PowerId, amount: i32) -> bool {
    if amount <= 0 && !sts_core::content::powers::uses_sentinel_amount(power) {
        return false;
    }
    !matches!(
        power,
        PowerId::Vulnerable
            | PowerId::Weak
            | PowerId::Frail
            | PowerId::LoseStrength
            | PowerId::Entangle
            | PowerId::Hex
            | PowerId::NoDraw
            | PowerId::NoBlock
            | PowerId::Confusion
            | PowerId::Constricted
            | PowerId::Shackled
            | PowerId::DrawReduction
            | PowerId::Surrounded
            | PowerId::BackAttack
            | PowerId::EnergyDownPower
            | PowerId::DexterityDown
            | PowerId::CannotChangeStance
    )
}
