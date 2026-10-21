use crate::content::monsters::factory::EncounterId;
use crate::content::relics::RelicId;
use crate::state::events::{
    EventActionKind, EventEffect, EventId, EventOptionTransition, EventState,
};
use crate::state::run::RunState;

use super::cost::EventCostProjectionV1;
use super::oracle::{peek_cursed_tome_book_v1, peek_scrap_ooze_v1, EventOracleEvidenceV1};
use super::shape::EventDecisionShapeV1;
use super::spec::{event_plan_step, materialize_event_plan_specs_v1, EventPlanSpecV1};
use super::types::EventPolicyConfigV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventInformationModeV1 {
    PublicOnly,
    CounterfactualOracle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPlanIdV1 {
    LeaveNow,
    CursedTomeReadThenStop,
    CursedTomeReadAndTakeBook,
    ScrapOozeReachInOnce,
    DeadAdventurerLeaveNow,
    DeadAdventurerSearchAsOptionalElite,
    KnowingSkullLeaveNow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventPlanCandidateV1 {
    pub plan_id: EventPlanIdV1,
    pub event_id: EventId,
    pub steps: Vec<EventPlanStepV1>,
    pub cost: EventCostProjectionV1,
    pub reward: EventPlanRewardV1,
    pub risk_model: EventPlanRiskModelV1,
    pub oracle_evidence: Option<EventOracleEvidenceV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventPlanStepV1 {
    pub screen: usize,
    pub choice_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventPlanRewardV1 {
    None,
    RandomBookRelic { observed: Option<RelicId> },
    RandomRelic,
    DeadAdventurerSearch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventPlanRiskModelV1 {
    None,
    HpPayment,
    RepeatedGamble {
        current_success_chance_percent: i32,
        current_effective_hp_loss: i32,
        next_effective_hp_loss: i32,
        treat_as_optional_elite: bool,
        worst_case_warning_hp_loss: i32,
    },
    OptionalEliteLike {
        fight_chance_percent: i32,
        encounter: Option<EventEncounterProjectionV1>,
        reward_already_obtained: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventEncounterProjectionV1 {
    pub encounter_id: EncounterId,
    pub publicly_revealed: bool,
    pub starts_awake: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventPlanCompileStatusV1 {
    Supported(Vec<EventPlanCandidateV1>),
    UnsupportedShape {
        event_id: EventId,
        shape: EventPlanUnsupportedShapeV1,
    },
    NotPlanManaged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPlanUnsupportedShapeV1 {
    RepeatablePaidMenu,
}

pub fn compile_event_plan_candidates_v1(
    run_state: &RunState,
    information_mode: EventInformationModeV1,
) -> Vec<EventPlanCandidateV1> {
    let Some(event_state) = &run_state.event_state else {
        return Vec::new();
    };
    let specs = match event_state.id {
        EventId::CursedTome => {
            compile_cursed_tome_plan_specs_v1(run_state, event_state, information_mode)
        }
        EventId::ScrapOoze => {
            compile_scrap_ooze_plan_specs_v1(run_state, event_state, information_mode)
        }
        EventId::DeadAdventurer => compile_dead_adventurer_plan_specs_v1(event_state),
        EventId::KnowingSkull => compile_knowing_skull_plan_specs_v1(run_state, event_state),
        _ => Vec::new(),
    };
    materialize_event_plan_specs_v1(run_state, specs)
}

pub fn compile_event_plan_status_v1(
    run_state: &RunState,
    event_shape: &EventDecisionShapeV1,
    information_mode: EventInformationModeV1,
) -> EventPlanCompileStatusV1 {
    let Some(event_state) = &run_state.event_state else {
        return EventPlanCompileStatusV1::NotPlanManaged;
    };
    let plans = compile_event_plan_candidates_v1(run_state, information_mode);
    if !plans.is_empty() {
        return EventPlanCompileStatusV1::Supported(plans);
    }
    match event_shape {
        EventDecisionShapeV1::RepeatablePaidMenu(_) => EventPlanCompileStatusV1::UnsupportedShape {
            event_id: event_state.id,
            shape: EventPlanUnsupportedShapeV1::RepeatablePaidMenu,
        },
        EventDecisionShapeV1::Standard => EventPlanCompileStatusV1::NotPlanManaged,
    }
}

pub fn select_event_plan_candidate_v1(
    run_state: &RunState,
    information_mode: EventInformationModeV1,
    config: &EventPolicyConfigV1,
) -> Option<EventPlanCandidateV1> {
    let plans = compile_event_plan_candidates_v1(run_state, information_mode);
    let Some(event_state) = &run_state.event_state else {
        return None;
    };
    match event_state.id {
        EventId::CursedTome => select_cursed_tome_plan_v1(run_state, config, plans),
        EventId::ScrapOoze => select_scrap_ooze_plan_v1(run_state, config, plans),
        EventId::DeadAdventurer => select_dead_adventurer_plan_v1(run_state, config, plans),
        EventId::KnowingSkull => select_plan_by_id_v1(plans, EventPlanIdV1::KnowingSkullLeaveNow),
        _ => None,
    }
}

fn compile_cursed_tome_plan_specs_v1(
    run_state: &RunState,
    event_state: &EventState,
    information_mode: EventInformationModeV1,
) -> Vec<EventPlanSpecV1> {
    let mut plans = Vec::new();
    if event_state.current_screen == 0 {
        plans.push(EventPlanSpecV1::leave(
            EventPlanIdV1::LeaveNow,
            EventId::CursedTome,
            0,
            1,
        ));
    }

    if event_state.current_screen <= 4 {
        let prefix_steps = cursed_tome_continue_steps_from(event_state.current_screen);
        let prefix_losses = cursed_tome_continue_losses_from(event_state.current_screen);

        let mut stop_steps = prefix_steps.clone();
        stop_steps.push(event_plan_step(4, 1));
        let mut stop_losses = prefix_losses.clone();
        stop_losses.push(3);
        plans.push(EventPlanSpecV1::hp_payment(
            EventPlanIdV1::CursedTomeReadThenStop,
            EventId::CursedTome,
            stop_steps,
            stop_losses,
            EventPlanRewardV1::None,
            None,
        ));

        let final_damage = cursed_tome_final_damage(run_state);
        let oracle_evidence = match information_mode {
            EventInformationModeV1::PublicOnly => None,
            EventInformationModeV1::CounterfactualOracle => peek_cursed_tome_book_v1(run_state),
        };
        let mut take_steps = prefix_steps;
        take_steps.push(event_plan_step(4, 0));
        let mut take_losses = prefix_losses;
        take_losses.push(final_damage);
        let observed = oracle_evidence
            .as_ref()
            .and_then(|evidence| evidence.observed_relic);
        plans.push(EventPlanSpecV1::hp_payment(
            EventPlanIdV1::CursedTomeReadAndTakeBook,
            EventId::CursedTome,
            take_steps,
            take_losses,
            EventPlanRewardV1::RandomBookRelic { observed },
            oracle_evidence,
        ));
    }

    plans
}

fn compile_scrap_ooze_plan_specs_v1(
    run_state: &RunState,
    event_state: &EventState,
    information_mode: EventInformationModeV1,
) -> Vec<EventPlanSpecV1> {
    if event_state.current_screen != 0 {
        return Vec::new();
    }

    let (chance, damage) = scrap_ooze_chance_and_damage(run_state, event_state);
    let oracle_evidence = match information_mode {
        EventInformationModeV1::PublicOnly => None,
        EventInformationModeV1::CounterfactualOracle => peek_scrap_ooze_v1(run_state),
    };

    vec![
        EventPlanSpecV1::leave(EventPlanIdV1::LeaveNow, EventId::ScrapOoze, 0, 1),
        EventPlanSpecV1::repeated_paid_gamble(
            EventPlanIdV1::ScrapOozeReachInOnce,
            EventId::ScrapOoze,
            event_plan_step(0, 0),
            chance,
            damage,
            damage + 1,
            EventPlanRewardV1::RandomRelic,
            oracle_evidence,
        ),
    ]
}

fn compile_dead_adventurer_plan_specs_v1(event_state: &EventState) -> Vec<EventPlanSpecV1> {
    if event_state.current_screen != 0 {
        return Vec::new();
    }

    let num_rewards = dead_adventurer_num_rewards(event_state.internal_state);
    let fight_chance = dead_adventurer_encounter_chance(event_state.internal_state);
    let reward_already_obtained = (0..num_rewards)
        .any(|idx| dead_adventurer_reward_type(event_state.internal_state, idx) == 2);
    let encounter = dead_adventurer_encounter_id(event_state.internal_state).map(|encounter_id| {
        EventEncounterProjectionV1 {
            encounter_id,
            publicly_revealed: true,
            starts_awake: encounter_id == EncounterId::LagavulinEvent,
        }
    });

    vec![
        EventPlanSpecV1::leave(
            EventPlanIdV1::DeadAdventurerLeaveNow,
            EventId::DeadAdventurer,
            0,
            1,
        ),
        EventPlanSpecV1::optional_elite_search(
            EventPlanIdV1::DeadAdventurerSearchAsOptionalElite,
            EventId::DeadAdventurer,
            event_plan_step(0, 0),
            fight_chance,
            encounter,
            reward_already_obtained,
        ),
    ]
}

fn compile_knowing_skull_plan_specs_v1(
    run_state: &RunState,
    event_state: &EventState,
) -> Vec<EventPlanSpecV1> {
    let Some((choice_index, hp_loss)) = knowing_skull_leave_choice_v1(run_state, event_state)
    else {
        return Vec::new();
    };
    vec![EventPlanSpecV1::hp_payment(
        EventPlanIdV1::KnowingSkullLeaveNow,
        EventId::KnowingSkull,
        vec![event_plan_step(event_state.current_screen, choice_index)],
        vec![hp_loss],
        EventPlanRewardV1::None,
        None,
    )]
}

fn select_cursed_tome_plan_v1(
    run_state: &RunState,
    config: &EventPolicyConfigV1,
    plans: Vec<EventPlanCandidateV1>,
) -> Option<EventPlanCandidateV1> {
    let hp_floor = hp_safety_floor(run_state, config);
    let take = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::CursedTomeReadAndTakeBook)
        .cloned();
    if let Some(take) = take {
        if run_state
            .current_hp
            .saturating_sub(take.cost.effective_hp_loss)
            >= hp_floor
        {
            return Some(take);
        }
    }

    plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::LeaveNow)
        .cloned()
        .or_else(|| {
            plans
                .into_iter()
                .find(|plan| plan.plan_id == EventPlanIdV1::CursedTomeReadThenStop)
        })
}

fn select_scrap_ooze_plan_v1(
    run_state: &RunState,
    config: &EventPolicyConfigV1,
    plans: Vec<EventPlanCandidateV1>,
) -> Option<EventPlanCandidateV1> {
    let hp_floor = hp_safety_floor(run_state, config);
    let reach = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::ScrapOozeReachInOnce)
        .cloned();
    if let Some(reach) = reach {
        let warning_hp_loss = match &reach.risk_model {
            EventPlanRiskModelV1::RepeatedGamble {
                worst_case_warning_hp_loss,
                ..
            } => *worst_case_warning_hp_loss,
            _ => reach.cost.effective_hp_loss,
        };
        if run_state.current_hp.saturating_sub(warning_hp_loss) >= hp_floor {
            return Some(reach);
        }
    }

    plans
        .into_iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::LeaveNow)
}

fn select_dead_adventurer_plan_v1(
    run_state: &RunState,
    config: &EventPolicyConfigV1,
    plans: Vec<EventPlanCandidateV1>,
) -> Option<EventPlanCandidateV1> {
    let leave = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::DeadAdventurerLeaveNow)
        .cloned();
    let search = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::DeadAdventurerSearchAsOptionalElite)
        .cloned();
    let Some(search) = search else {
        return leave;
    };

    if let EventPlanRiskModelV1::OptionalEliteLike {
        reward_already_obtained,
        ..
    } = &search.risk_model
    {
        if *reward_already_obtained {
            return leave.or(Some(search));
        }
    }

    let hp_floor = hp_safety_floor(run_state, config);
    if run_state.current_hp.saturating_sub(20) >= hp_floor {
        Some(search)
    } else {
        leave.or(Some(search))
    }
}

fn select_plan_by_id_v1(
    plans: Vec<EventPlanCandidateV1>,
    plan_id: EventPlanIdV1,
) -> Option<EventPlanCandidateV1> {
    plans.into_iter().find(|plan| plan.plan_id == plan_id)
}

fn hp_safety_floor(run_state: &RunState, config: &EventPolicyConfigV1) -> i32 {
    let ratio_floor =
        (run_state.max_hp.max(0) as f32 * config.min_hp_ratio_after_safe_hp_cost).ceil() as i32;
    config.min_hp_after_safe_hp_cost.max(ratio_floor)
}

fn cursed_tome_continue_steps_from(current_screen: usize) -> Vec<EventPlanStepV1> {
    (current_screen..=3)
        .map(|screen| event_plan_step(screen, 0))
        .collect()
}

fn cursed_tome_continue_losses_from(current_screen: usize) -> Vec<i32> {
    (current_screen.max(1)..=3)
        .map(|screen| screen as i32)
        .collect()
}

fn cursed_tome_final_damage(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        15
    } else {
        10
    }
}

fn scrap_ooze_chance_and_damage(run_state: &RunState, event_state: &EventState) -> (i32, i32) {
    if event_state.internal_state == 0 {
        let base_damage = if run_state.ascension_level >= 15 {
            5
        } else {
            3
        };
        (25, base_damage)
    } else {
        let chance = event_state.internal_state & 0xFFFF;
        let damage = (event_state.internal_state >> 16) & 0xFFFF;
        (chance, damage)
    }
}

fn dead_adventurer_num_rewards(state: i32) -> i32 {
    state & 0xF
}

fn dead_adventurer_encounter_chance(state: i32) -> i32 {
    (state >> 4) & 0xFF
}

fn dead_adventurer_reward_type(state: i32, idx: i32) -> i32 {
    (state >> (12 + idx * 2)) & 0x3
}

fn dead_adventurer_encounter_id(state: i32) -> Option<EncounterId> {
    match (state >> 18) & 0x3 {
        0 => Some(EncounterId::ThreeSentries),
        1 => Some(EncounterId::GremlinNob),
        2 => Some(EncounterId::LagavulinEvent),
        _ => None,
    }
}

fn knowing_skull_leave_choice_v1(
    run_state: &RunState,
    event_state: &EventState,
) -> Option<(usize, i32)> {
    if event_state.id != EventId::KnowingSkull || event_state.current_screen != 1 {
        return None;
    }
    crate::content::events::knowing_skull::get_options(run_state, event_state)
        .into_iter()
        .enumerate()
        .find_map(|(index, option)| {
            if option.semantics.action != EventActionKind::Leave
                || option.semantics.transition != EventOptionTransition::AdvanceScreen
            {
                return None;
            }
            let hp_loss = option
                .semantics
                .effects
                .iter()
                .filter_map(|effect| match effect {
                    EventEffect::LoseHp(value) => Some(*value),
                    _ => None,
                })
                .sum();
            Some((index, hp_loss))
        })
}
