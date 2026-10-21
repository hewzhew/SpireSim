use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{EventId, EventState};
use crate::state::rewards::RewardItem;
use crate::state::run::RunState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventOracleEvidenceV1 {
    pub event_id: EventId,
    pub observed_relic: Option<RelicId>,
    pub outcome: EventOracleOutcomeV1,
    pub committed: bool,
    pub misc_rng_delta_if_committed: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventOracleOutcomeV1 {
    CursedTomeBook {
        observed_relic: Option<RelicId>,
    },
    ScrapOoze {
        attempts_until_success: Option<usize>,
        failed_attempts_before_stop: usize,
        effective_hp_loss_if_committed: i32,
        observed_relic: Option<RelicId>,
    },
}

pub(super) fn peek_cursed_tome_book_v1(run_state: &RunState) -> Option<EventOracleEvidenceV1> {
    let Some(event_state) = &run_state.event_state else {
        return None;
    };
    if event_state.id != EventId::CursedTome || event_state.current_screen > 4 {
        return None;
    }

    let mut clone = run_state.clone();
    let misc_before = clone.rng_pool.misc_rng.counter;
    let mut engine_state = EngineState::EventRoom;

    while clone.event_state.as_ref()?.current_screen < 4 {
        crate::content::events::cursed_tome::handle_choice(&mut engine_state, &mut clone, 0);
    }
    crate::content::events::cursed_tome::handle_choice(&mut engine_state, &mut clone, 0);

    let observed_relic = match engine_state {
        EngineState::RewardScreen(rewards) => {
            rewards.items.into_iter().find_map(|item| match item {
                RewardItem::Relic { relic_id } => Some(relic_id),
                _ => None,
            })
        }
        _ => None,
    };
    let misc_after = clone.rng_pool.misc_rng.counter;

    Some(EventOracleEvidenceV1 {
        event_id: EventId::CursedTome,
        observed_relic,
        outcome: EventOracleOutcomeV1::CursedTomeBook { observed_relic },
        committed: false,
        misc_rng_delta_if_committed: misc_after.saturating_sub(misc_before),
    })
}

pub(super) fn peek_scrap_ooze_v1(run_state: &RunState) -> Option<EventOracleEvidenceV1> {
    let Some(event_state) = &run_state.event_state else {
        return None;
    };
    if event_state.id != EventId::ScrapOoze || event_state.current_screen != 0 {
        return None;
    }

    let mut clone = run_state.clone();
    let misc_before = clone.rng_pool.misc_rng.counter;
    let hp_before = clone.current_hp;
    let relic_count_before = clone.relics.len();
    let mut engine_state = EngineState::EventRoom;
    let mut attempts = 0usize;

    while clone
        .event_state
        .as_ref()
        .is_some_and(scrap_ooze_waits_for_reach)
        && clone.current_hp > 0
        && attempts < 32
    {
        crate::content::events::scrap_ooze::handle_choice(&mut engine_state, &mut clone, 0);
        attempts += 1;
    }

    let success = clone
        .event_state
        .as_ref()
        .is_some_and(scrap_ooze_relic_obtained_screen)
        && clone.relics.len() > relic_count_before;
    let observed_relic = if success {
        clone.relics.get(relic_count_before).map(|relic| relic.id)
    } else {
        None
    };
    let misc_after = clone.rng_pool.misc_rng.counter;
    let effective_hp_loss_if_committed = hp_before.saturating_sub(clone.current_hp);

    Some(EventOracleEvidenceV1 {
        event_id: EventId::ScrapOoze,
        observed_relic,
        outcome: EventOracleOutcomeV1::ScrapOoze {
            attempts_until_success: success.then_some(attempts),
            failed_attempts_before_stop: if success {
                attempts.saturating_sub(1)
            } else {
                attempts
            },
            effective_hp_loss_if_committed,
            observed_relic,
        },
        committed: false,
        misc_rng_delta_if_committed: misc_after.saturating_sub(misc_before),
    })
}

fn scrap_ooze_waits_for_reach(state: &EventState) -> bool {
    state.id == EventId::ScrapOoze && state.current_screen == 0
}

fn scrap_ooze_relic_obtained_screen(state: &EventState) -> bool {
    state.id == EventId::ScrapOoze && state.current_screen == 1
}
