use crate::state::events::EventId;
use crate::state::run::RunState;

use super::cost::project_hp_loss_cost_v1;
use super::oracle::EventOracleEvidenceV1;
use super::plan::{
    EventEncounterProjectionV1, EventPlanCandidateV1, EventPlanIdV1, EventPlanRewardV1,
    EventPlanRiskModelV1, EventPlanStepV1,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EventPlanSpecV1 {
    plan_id: EventPlanIdV1,
    event_id: EventId,
    reward: EventPlanRewardV1,
    oracle_evidence: Option<EventOracleEvidenceV1>,
    kind: EventPlanSpecKindV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EventPlanSpecKindV1 {
    NoCost {
        steps: Vec<EventPlanStepV1>,
        risk_model: EventPlanRiskModelV1,
    },
    HpPayment {
        steps: Vec<EventPlanStepV1>,
        hp_losses: Vec<i32>,
    },
    RepeatedPaidGamble {
        step: EventPlanStepV1,
        current_success_chance_percent: i32,
        current_hp_loss: i32,
        next_hp_loss: i32,
        treat_as_optional_elite: bool,
        worst_case_warning_hp_loss: i32,
    },
    OptionalEliteSearch {
        step: EventPlanStepV1,
        fight_chance_percent: i32,
        encounter: Option<EventEncounterProjectionV1>,
        reward_already_obtained: bool,
    },
}

impl EventPlanSpecV1 {
    pub(super) fn leave(
        plan_id: EventPlanIdV1,
        event_id: EventId,
        screen: usize,
        choice_index: usize,
    ) -> Self {
        Self {
            plan_id,
            event_id,
            reward: EventPlanRewardV1::None,
            oracle_evidence: None,
            kind: EventPlanSpecKindV1::NoCost {
                steps: vec![event_plan_step(screen, choice_index)],
                risk_model: EventPlanRiskModelV1::None,
            },
        }
    }

    pub(super) fn hp_payment(
        plan_id: EventPlanIdV1,
        event_id: EventId,
        steps: Vec<EventPlanStepV1>,
        hp_losses: Vec<i32>,
        reward: EventPlanRewardV1,
        oracle_evidence: Option<EventOracleEvidenceV1>,
    ) -> Self {
        Self {
            plan_id,
            event_id,
            reward,
            oracle_evidence,
            kind: EventPlanSpecKindV1::HpPayment { steps, hp_losses },
        }
    }

    pub(super) fn repeated_paid_gamble(
        plan_id: EventPlanIdV1,
        event_id: EventId,
        step: EventPlanStepV1,
        current_success_chance_percent: i32,
        current_hp_loss: i32,
        next_hp_loss: i32,
        reward: EventPlanRewardV1,
        oracle_evidence: Option<EventOracleEvidenceV1>,
    ) -> Self {
        Self {
            plan_id,
            event_id,
            reward,
            oracle_evidence,
            kind: EventPlanSpecKindV1::RepeatedPaidGamble {
                step,
                current_success_chance_percent,
                current_hp_loss,
                next_hp_loss,
                treat_as_optional_elite: true,
                worst_case_warning_hp_loss: 10,
            },
        }
    }

    pub(super) fn optional_elite_search(
        plan_id: EventPlanIdV1,
        event_id: EventId,
        step: EventPlanStepV1,
        fight_chance_percent: i32,
        encounter: Option<EventEncounterProjectionV1>,
        reward_already_obtained: bool,
    ) -> Self {
        Self {
            plan_id,
            event_id,
            reward: EventPlanRewardV1::DeadAdventurerSearch,
            oracle_evidence: None,
            kind: EventPlanSpecKindV1::OptionalEliteSearch {
                step,
                fight_chance_percent,
                encounter,
                reward_already_obtained,
            },
        }
    }
}

pub(super) fn event_plan_step(screen: usize, choice_index: usize) -> EventPlanStepV1 {
    EventPlanStepV1 {
        screen,
        choice_index,
    }
}

pub(super) fn materialize_event_plan_specs_v1(
    run_state: &RunState,
    specs: Vec<EventPlanSpecV1>,
) -> Vec<EventPlanCandidateV1> {
    specs
        .into_iter()
        .map(|spec| materialize_event_plan_spec_v1(run_state, spec))
        .collect()
}

fn materialize_event_plan_spec_v1(
    run_state: &RunState,
    spec: EventPlanSpecV1,
) -> EventPlanCandidateV1 {
    let (steps, cost, risk_model) = match spec.kind {
        EventPlanSpecKindV1::NoCost { steps, risk_model } => {
            (steps, project_hp_loss_cost_v1(run_state, &[]), risk_model)
        }
        EventPlanSpecKindV1::HpPayment { steps, hp_losses } => (
            steps,
            project_hp_loss_cost_v1(run_state, &hp_losses),
            EventPlanRiskModelV1::HpPayment,
        ),
        EventPlanSpecKindV1::RepeatedPaidGamble {
            step,
            current_success_chance_percent,
            current_hp_loss,
            next_hp_loss,
            treat_as_optional_elite,
            worst_case_warning_hp_loss,
        } => {
            let current_cost = project_hp_loss_cost_v1(run_state, &[current_hp_loss]);
            let next_cost = project_hp_loss_cost_v1(run_state, &[next_hp_loss]);
            (
                vec![step],
                current_cost.clone(),
                EventPlanRiskModelV1::RepeatedGamble {
                    current_success_chance_percent,
                    current_effective_hp_loss: current_cost.effective_hp_loss,
                    next_effective_hp_loss: next_cost.effective_hp_loss,
                    treat_as_optional_elite,
                    worst_case_warning_hp_loss,
                },
            )
        }
        EventPlanSpecKindV1::OptionalEliteSearch {
            step,
            fight_chance_percent,
            encounter,
            reward_already_obtained,
        } => (
            vec![step],
            project_hp_loss_cost_v1(run_state, &[]),
            EventPlanRiskModelV1::OptionalEliteLike {
                fight_chance_percent,
                encounter,
                reward_already_obtained,
            },
        ),
    };

    EventPlanCandidateV1 {
        plan_id: spec.plan_id,
        event_id: spec.event_id,
        steps,
        cost,
        reward: spec.reward,
        risk_model,
        oracle_evidence: spec.oracle_evidence,
    }
}
