use crate::content::powers::{store, PowerId};
use crate::state::core::CombatContext;
use crate::state::map::node::RoomType;

use super::{RunControlHpLossLimit, RunControlSession};

pub fn strategic_combat_persistent_payoff_matters_v1(session: &RunControlSession) -> bool {
    let Some(active) = session.active_combat.as_ref() else {
        return false;
    };
    room_boss_combat(session)
        && !room_boss_win_ends_requested_run(session)
        && crate::ai::combat_persistent_outcome_v1::has_persistent_or_reward_payoff_opportunity(
            &active.combat_state,
        )
}

/// Returns whether winning the active combat deterministically restores full
/// HP before the run can encounter another damage-bearing decision.
///
/// Local final HP is not a useful potion-spend tiebreaker at this boundary:
/// pre-A5 Act 1/2 room Boss victories apply the game's guaranteed transition
/// heal, while the potion inventory continues into the next Act.
pub fn strategic_combat_victory_reaches_full_heal_v1(session: &RunControlSession) -> bool {
    room_boss_win_reaches_full_heal(session)
}

/// Returns the largest HP loss that is already good enough for strategic run
/// progression to stop refining an exact combat witness.
///
/// This is deliberately distinct from the much looser execution reserve. A
/// line must both preserve the run's generic survival reserve and lose no more
/// than one fifth of max HP. Finite-duration survival encounters additionally
/// preserve half of entry HP. Bosses that immediately heal at the next act or
/// end the requested run do not need a combat-local quality target.
pub fn strategic_combat_quality_hp_loss_limit_v1(
    session: &RunControlSession,
) -> RunControlHpLossLimit {
    let RunControlHpLossLimit::Limit(reserve_limit) =
        strategic_combat_survival_hp_loss_limit_v1(session)
    else {
        return RunControlHpLossLimit::Unlimited;
    };
    if session.active_combat.as_ref().is_some_and(|active| {
        crate::ai::combat_persistent_outcome_v1::has_healing_payoff_opportunity(
            &active.combat_state,
        )
    }) {
        return RunControlHpLossLimit::Limit(0);
    }
    let (_, max_hp) = session.visible_player_hp();
    let max_hp = max_hp.max(1);
    let quality_limit = (max_hp / 5).max(1) as u32;
    RunControlHpLossLimit::Limit(reserve_limit.min(quality_limit))
}

/// Returns the broad survival reserve used to reject combat lines that would
/// leave the run without a meaningful floor-to-floor buffer.
pub fn strategic_combat_survival_hp_loss_limit_v1(
    session: &RunControlSession,
) -> RunControlHpLossLimit {
    if room_boss_win_ends_requested_run(session)
        || strategic_combat_victory_reaches_full_heal_v1(session)
    {
        return RunControlHpLossLimit::Unlimited;
    }
    let (current_hp, max_hp) = session.visible_player_hp();
    let max_hp = max_hp.max(1);
    let generic_reserve_hp = (max_hp / 4).max(1);
    let finite_survival_reserve_hp = if finite_survival_damage_mitigation_active(session) {
        current_hp.max(0).saturating_add(1) / 2
    } else {
        Default::default()
    };
    let reserve_hp = generic_reserve_hp.max(finite_survival_reserve_hp);
    let max_hp_loss = current_hp.saturating_sub(reserve_hp).max(0) as u32;
    RunControlHpLossLimit::Limit(max_hp_loss)
}

fn finite_survival_damage_mitigation_active(session: &RunControlSession) -> bool {
    session.active_combat.as_ref().is_some_and(|active| {
        active
            .combat_state
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .any(|monster| {
                store::power_amount(&active.combat_state, monster.id, PowerId::Fading) > 0
                    && store::has_power(&active.combat_state, monster.id, PowerId::Shifting)
            })
    })
}

fn room_boss_combat(session: &RunControlSession) -> bool {
    session.active_combat.as_ref().is_some_and(|active| {
        matches!(
            active.context,
            CombatContext::Room(ref room) if room.room_type == RoomType::MonsterRoomBoss
        )
    })
}

fn room_boss_win_ends_requested_run(session: &RunControlSession) -> bool {
    room_boss_combat(session)
        && session.run_state.act_num >= 3
        && !session.run_state.should_start_act3_double_boss()
}

fn room_boss_win_reaches_full_heal(session: &RunControlSession) -> bool {
    room_boss_combat(session)
        && session.run_state.act_num < 3
        && session.run_state.ascension_level < 5
}
