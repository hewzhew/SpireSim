use crate::state::core::{ClientInput, EngineState};

use super::session::RunControlSession;
use super::view_model::{build_run_control_view_model, DecisionCandidate, RunControlViewModel};

#[derive(Clone, Debug, PartialEq)]
pub struct DecisionSurface {
    pub view: RunControlViewModel,
    pub visible_executable_inputs: Vec<ClientInput>,
}

pub fn build_decision_surface(session: &RunControlSession) -> DecisionSurface {
    let view = build_run_control_view_model(session);
    let visible_executable_inputs = view
        .candidates
        .iter()
        .filter_map(|candidate| candidate.action.executable_input())
        .collect::<Vec<_>>();
    DecisionSurface {
        view,
        visible_executable_inputs,
    }
}

pub fn resolve_surface_candidate<'a>(
    surface: &'a DecisionSurface,
    engine_state: &EngineState,
    raw_id: &str,
) -> Option<&'a DecisionCandidate> {
    resolve_candidate_alias(&surface.view.candidates, engine_state, raw_id)
}

pub(super) fn resolve_candidate_alias<'a>(
    candidates: &'a [DecisionCandidate],
    engine_state: &EngineState,
    raw_id: &str,
) -> Option<&'a DecisionCandidate> {
    if let Some(candidate) = candidates.iter().find(|candidate| candidate.id == raw_id) {
        return Some(candidate);
    }

    let id = raw_id.trim().to_ascii_lowercase();
    if let Some(candidate) = candidates.iter().find(|candidate| candidate.id == id) {
        return Some(candidate);
    }
    if id.chars().all(|ch| ch.is_ascii_digit()) && !id.is_empty() {
        let structured = match engine_state {
            EngineState::Shop(_) => Some(format!("card-{id}")),
            EngineState::Campfire => Some(format!("smith-{id}")),
            _ => None,
        };
        if let Some(structured) = structured {
            if let Some(candidate) = candidates
                .iter()
                .find(|candidate| candidate.id == structured)
            {
                return Some(candidate);
            }
        }
    }

    match id.as_str() {
        "leave" | "skip" => candidates.iter().find(|candidate| {
            let label = candidate
                .label
                .trim_start()
                .to_ascii_lowercase()
                .trim_end_matches(['.', '!', '?'])
                .to_string();
            label.starts_with(&id)
        }),
        _ => None,
    }
}

pub fn surface_allows_visible_input(surface: &DecisionSurface, input: &ClientInput) -> bool {
    surface
        .visible_executable_inputs
        .iter()
        .any(|candidate_input| candidate_input == input)
}

#[cfg(test)]
pub(super) fn surface_legal_visibility_violations(session: &RunControlSession) -> Vec<String> {
    let surface = build_decision_surface(session);
    let mut violations = Vec::new();
    for candidate in &surface.view.candidates {
        if candidate.action.executable_input().is_none() {
            continue;
        }
        if candidate.id.trim().is_empty() {
            violations.push(format!(
                "visible candidate '{}' has empty id",
                candidate.label
            ));
        }
        if candidate.label.trim().is_empty() {
            violations.push(format!(
                "visible candidate '{}' has empty label",
                candidate.id
            ));
        }
    }

    if let Ok(position) = session.current_combat_position_for_actions() {
        if matches!(position.engine, EngineState::PendingChoice(_)) {
            if super::selection_surface::active_selection_surface(session).is_some() {
                if !surface
                    .view
                    .candidates
                    .iter()
                    .any(|candidate| candidate.id == "select")
                {
                    violations.push(
                        "compact selection surface is missing select command candidate".to_string(),
                    );
                }
                return violations;
            }
            let legal_surface = crate::sim::combat_action_surface::combat_legal_action_surface_v2(
                &position.engine,
                &position.combat,
            );
            if !legal_surface.selection_families.is_empty() {
                violations.push(
                    "pending choice symbolic action family has no compact visible surface"
                        .to_string(),
                );
            }
            for legal in legal_surface.atomic_actions {
                if !surface_allows_visible_input(&surface, &legal) {
                    violations.push(format!(
                        "pending choice legal input '{}' is not visible",
                        super::view_model::client_input_hint(&legal)
                    ));
                }
            }
        }
    }
    violations
}
