//! Typed owner-command adapters.

use super::owner_model::{ChoiceAnnotation, OwnerChoice, OwnerChoiceExpansion};
use sts_simulator::eval::run_control::{DecisionCandidateKey, DecisionSurface, RunDecisionAction};
use sts_simulator::state::core::ClientInput;

pub fn executable_choices(surface: &DecisionSurface) -> Vec<OwnerChoice> {
    executable_choices_with_cancel(surface, false)
}

pub(super) fn executable_choices_including_cancel(surface: &DecisionSurface) -> Vec<OwnerChoice> {
    executable_choices_with_cancel(surface, true)
}

pub(super) fn owner_candidate_id_for_action(
    surface: &DecisionSurface,
    action: &RunDecisionAction,
) -> Result<String, String> {
    let exact = surface
        .view
        .candidates
        .iter()
        .filter(|candidate| candidate.action.executable_action().as_ref() == Some(action))
        .collect::<Vec<_>>();
    if let [candidate] = exact.as_slice() {
        return Ok(candidate.id.clone());
    }
    if exact.len() > 1 {
        return Err(format!(
            "owner action {action:?} matches {} public candidates",
            exact.len()
        ));
    }

    if matches!(
        action,
        RunDecisionAction::Input(ClientInput::SubmitSelection(_))
    ) {
        let parameterized = surface
            .view
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.key.as_ref(),
                    Some(DecisionCandidateKey::SelectionSubmit { .. })
                )
            })
            .collect::<Vec<_>>();
        if let [candidate] = parameterized.as_slice() {
            return Ok(candidate.id.clone());
        }
        return Err(format!(
            "owner selection binding found {} public SelectionSubmit candidates",
            parameterized.len()
        ));
    }

    Err(format!("owner action {action:?} is not a public candidate"))
}

fn executable_choices_with_cancel(
    surface: &DecisionSurface,
    include_cancel: bool,
) -> Vec<OwnerChoice> {
    surface
        .view
        .candidates
        .iter()
        .filter_map(|candidate| {
            let action = candidate.action.executable_action()?;
            if !include_owner_choice_action(&action, include_cancel) {
                return None;
            }
            Some(OwnerChoice {
                candidate_id: candidate.id.clone(),
                key: candidate.key.clone(),
                action,
                label: candidate.label.clone(),
                annotation: ChoiceAnnotation::None,
                expansion: OwnerChoiceExpansion::AutoAllowed,
            })
        })
        .collect()
}

fn include_owner_choice_action(action: &RunDecisionAction, include_cancel: bool) -> bool {
    include_cancel || !is_navigation_only_action(action)
}

fn is_navigation_only_action(action: &RunDecisionAction) -> bool {
    matches!(action, RunDecisionAction::Input(ClientInput::Cancel))
}
