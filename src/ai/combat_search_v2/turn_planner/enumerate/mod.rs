mod partial_frontier;
mod plan;
mod ranking;
mod resource_transformation;
mod selection;
mod session;
mod work;

use std::time::Instant;

use crate::sim::combat::{CombatPosition, CombatStepper};

use super::super::*;
use super::types::{TurnPlanEnumeration, TurnPlannerConfigV1};
use crate::ai::combat_search_v2::turn_plan_probe::CombatSearchV2TurnPlanProbeGuideRankV1;
pub(in crate::ai::combat_search_v2) use session::{TurnPlanLayerSessionV1, TurnPlanPartialLayerV1};

pub(in crate::ai::combat_search_v2) fn enumerate_turn_plans(
    root: &SearchNode,
    stepper: &impl CombatStepper,
    config: &TurnPlannerConfigV1,
    deadline: Option<Instant>,
) -> TurnPlanEnumeration {
    enumerate_turn_plans_impl(root, stepper, config, deadline, false, None)
}

pub(in crate::ai::combat_search_v2) fn enumerate_turn_plans_across_pending_choices(
    root: &SearchNode,
    stepper: &impl CombatStepper,
    config: &TurnPlannerConfigV1,
    deadline: Option<Instant>,
) -> TurnPlanEnumeration {
    enumerate_turn_plans_impl(root, stepper, config, deadline, true, None)
}

pub(in crate::ai::combat_search_v2) fn enumerate_turn_plans_across_pending_choices_guided(
    root: &SearchNode,
    stepper: &impl CombatStepper,
    config: &TurnPlannerConfigV1,
    deadline: Option<Instant>,
    partial_state_guides: &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
) -> TurnPlanEnumeration {
    enumerate_turn_plans_impl(
        root,
        stepper,
        config,
        deadline,
        true,
        Some(partial_state_guides),
    )
}

fn enumerate_turn_plans_impl(
    root: &SearchNode,
    stepper: &impl CombatStepper,
    config: &TurnPlannerConfigV1,
    deadline: Option<Instant>,
    continue_pending_choices: bool,
    partial_state_guides: Option<
        &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
    >,
) -> TurnPlanEnumeration {
    let mut session = TurnPlanLayerSessionV1::new(root, config, continue_pending_choices);
    while session.expand_next_layer(stepper, deadline) {
        session.retain_pending_with_guides(partial_state_guides);
    }
    session.finish()
}

fn legal_action_choices(
    stepper: &impl CombatStepper,
    position: &CombatPosition,
    continue_pending_choices: bool,
    structured_choice_cap: usize,
) -> Vec<crate::sim::combat_action::CombatActionChoice> {
    let mut choices = stepper.atomic_action_choices(position);
    if !continue_pending_choices || !stepper.supports_canonical_pending_choice_actions() {
        return choices;
    }
    let EngineState::PendingChoice(choice) = &position.engine else {
        return choices;
    };
    let mut automatic_surface = stepper.legal_action_surface(position);
    automatic_surface.apply_automatic_action_admission(&position.combat);
    choices.retain(|choice| automatic_surface.atomic_actions.contains(&choice.input));
    let Some(inputs) =
        crate::ai::combat_search_v2::pending_choice_action_prefix::canonical_pending_choice_inputs(
            choice,
        )
    else {
        return choices;
    };

    for input in inputs.take(structured_choice_cap.max(1)) {
        if choices.iter().any(|choice| choice.input == input) {
            continue;
        }
        if let Some(choice) = stepper.choice_for_legal_input(position, &input) {
            choices.push(choice);
        }
    }
    choices
}
