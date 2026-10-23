use std::fmt::Write;

use sts_agent::agent::information::action::{
    public_choice_for_exact_combat_input_v1, PublicCombatActionChoiceV1,
};
use sts_combat_planner::{
    CombatDraftOptionHandle, CombatDraftSession, CompleteTurnOptionBoundary, SelectionSource,
};
use sts_core::sim::combat_action_equivalence::CombatSelectionStateEquivalenceV1;

use crate::{
    project_combat_draft_selection_at_v1, project_combat_draft_selection_surface_at_v1,
    CombatDraftSelectionMemberV1,
};

#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftSelectionOutcomeV1 {
    pub handle: CombatDraftOptionHandle,
    pub selection: CombatDraftSelectionMemberV1,
    pub equivalence: Option<CombatSelectionStateEquivalenceV1>,
    pub preferred: bool,
    pub boundary: CompleteTurnOptionBoundary,
    pub action_count: usize,
    pub successor: crate::CombatDraftSuccessorSummaryV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftSelectionOutcomesViewV1 {
    pub outcomes: Vec<CombatDraftSelectionOutcomeV1>,
    pub total: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

/// These are retained successors, not new simulations or ranked conclusions.
pub fn project_combat_draft_selection_outcomes_v1(
    draft: &CombatDraftSession,
    source: &SelectionSource,
    offset: usize,
) -> Result<CombatDraftSelectionOutcomesViewV1, String> {
    let page = draft
        .selection_outcomes(source, offset, 8)
        .map_err(|e| format!("selection outcomes unavailable: {e:?}"))?;
    let projection = project_combat_draft_selection_surface_at_v1(draft, source)?;
    let selection_view = project_combat_draft_selection_at_v1(draft, source)?;
    let outcomes = page
        .outcomes
        .into_iter()
        .map(|outcome| {
            let input = outcome
                .equivalent_selection
                .as_ref()
                .map_or(&outcome.selection, |equivalent| &equivalent.input);
            let selection = match public_choice_for_exact_combat_input_v1(&projection, input)? {
                PublicCombatActionChoiceV1::Selection {
                    family_ordinal,
                    selected_domain_ordinals,
                } => CombatDraftSelectionMemberV1 {
                    family: family_ordinal,
                    choices: selected_domain_ordinals,
                },
                _ => return Err("selection outcome contains a non-selection input".to_owned()),
            };
            let option = draft
                .option(outcome.handle)
                .map_err(|e| format!("selection outcome unavailable: {e:?}"))?;
            let state = &option.exact_successor().combat;
            Ok(CombatDraftSelectionOutcomeV1 {
                handle: outcome.handle,
                preferred: selection_view.preferences.contains(&selection),
                selection,
                equivalence: outcome
                    .equivalent_selection
                    .map(|equivalent| equivalent.basis),
                boundary: option.boundary(),
                action_count: option.actions().len(),
                successor: crate::draft_successor::project_draft_successor(state),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let end = offset.saturating_add(outcomes.len());
    Ok(CombatDraftSelectionOutcomesViewV1 {
        outcomes,
        total: page.total,
        offset,
        next_offset: (end < page.total).then_some(end),
    })
}

pub fn render_combat_draft_selection_outcomes_v1(
    view: &CombatDraftSelectionOutcomesViewV1,
) -> String {
    let mut text = format!("Outcomes: {}", view.total);
    if !view.outcomes.is_empty() {
        write!(
            text,
            " | Showing {}..{}",
            view.offset + 1,
            view.offset + view.outcomes.len()
        )
        .unwrap();
    }
    text.push('\n');
    for outcome in &view.outcomes {
        let boundary = match outcome.boundary {
            CompleteTurnOptionBoundary::NextPlayerTurn => "Next turn",
            CompleteTurnOptionBoundary::TerminalWin => "Victory",
            CompleteTurnOptionBoundary::TerminalLoss => "Defeat",
            CompleteTurnOptionBoundary::Escape => "Escape",
        };
        write!(
            text,
            "{} | family {} choices {:?}{} | {boundary} | {}",
            outcome.handle,
            outcome.selection.family,
            outcome.selection.choices,
            if outcome.preferred {
                " (preferred)"
            } else {
                ""
            },
            crate::render_combat_draft_successor_v1(
                &outcome.successor,
                sts_presentation::Locale::En
            )
        )
        .unwrap();
        match outcome.equivalence {
            Some(CombatSelectionStateEquivalenceV1::GamblingChipCancel) => {
                text.push_str(" | via Cancel")
            }
            None => {}
        }
        writeln!(text, " | {} actions", outcome.action_count).unwrap();
    }
    text
}
