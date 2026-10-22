//! Read-only projections of the resident turn draft. Handles come from the
//! environment; presentation never chooses or replays a candidate index.
use std::fmt::Write as _;
use sts_presentation::Locale;

use serde::Serialize;
use sts_combat_planner::{CombatDraftOptionCoverage, CompleteTurnOptionBoundary};
use sts_complete_turn_env::{CombatDraftEnvStatusV1, CombatDraftEnvV1};
use sts_complete_turn_view::{
    render_combat_draft_action_v1, render_combat_draft_successor_v1, CombatDraftSuccessorSummaryV1,
};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DraftOptionV1 {
    pub option_ref: String,
    pub label: String,
    pub boundary: &'static str,
    pub action_count: usize,
    pub successor: CombatDraftSuccessorSummaryV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DraftReadingV1 {
    pub view: String,
    pub options: Vec<DraftOptionV1>,
}

pub(super) fn status(env: &CombatDraftEnvV1) -> Result<DraftReadingV1, String> {
    if let CombatDraftEnvStatusV1::Terminal {
        selected_turn_count,
        boundary,
    } = env.status()
    {
        return Ok(DraftReadingV1 {
            view: format!(
                "Draft | {} | {selected_turn_count} turn{}\n",
                boundary_label(boundary),
                if selected_turn_count == 1 { "" } else { "s" },
            ),
            options: vec![],
        });
    }
    let state = env.observe().map_err(|e| e.to_string())?;
    let mut view = String::from("Draft | ");
    view.push_str(&crate::decision::combat::state_reading(
        &state.root,
        Locale::En,
    ));
    let coverage = match state.coverage {
        CombatDraftOptionCoverage::Complete => "complete".to_owned(),
        CombatDraftOptionCoverage::Open {
            mechanics_gap_count: 0,
        } => "open".to_owned(),
        CombatDraftOptionCoverage::Open {
            mechanics_gap_count,
        } => format!("open; {mechanics_gap_count} mechanics gaps"),
        CombatDraftOptionCoverage::MechanicsGap { gap_count } => {
            format!("{gap_count} mechanics gaps")
        }
    };
    writeln!(
        view,
        "Turn options | {} generated | {coverage}",
        state.options.len()
    )
    .unwrap();
    let options = state
        .options
        .iter()
        .map(|option| {
            let first = option
                .first_action
                .as_ref()
                .map(render_combat_draft_action_v1)
                .transpose()?
                .unwrap_or_else(|| "Settle".to_owned());
            let label = format!(
                "{}: {first} | {} action{} | {} | {}{}",
                option.handle,
                option.action_count,
                if option.action_count == 1 { "" } else { "s" },
                boundary_label(option.boundary),
                render_combat_draft_successor_v1(&option.successor),
                if option.continuation.is_some() {
                    " | explored"
                } else {
                    ""
                }
            );
            Ok(DraftOptionV1 {
                option_ref: option.handle.to_string(),
                label,
                boundary: super::boundary_token(option.boundary),
                action_count: option.action_count,
                successor: option.successor.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    for option in options.iter().take(12) {
        writeln!(view, "{}", option.label).unwrap();
    }
    if options.len() > 12 {
        writeln!(view, "{} more in options", options.len() - 12).unwrap();
    }
    Ok(DraftReadingV1 { view, options })
}

pub(super) fn inspect(env: &CombatDraftEnvV1, option_ref: &str) -> Result<String, String> {
    let detail = env.inspect(option_ref).map_err(|e| e.to_string())?;
    let mut view = format!("{option_ref} | {}\n", boundary_label(detail.boundary));
    for (index, action) in detail.actions.iter().enumerate() {
        writeln!(
            view,
            "  {index}: {}",
            render_combat_draft_action_v1(action)?
        )
        .unwrap();
    }
    view.push_str("Successor | ");
    view.push_str(&crate::decision::combat::state_reading(
        &detail.successor,
        Locale::En,
    ));
    Ok(view)
}

pub(super) fn boundary_label(boundary: CompleteTurnOptionBoundary) -> &'static str {
    match boundary {
        CompleteTurnOptionBoundary::NextPlayerTurn => "Next turn",
        CompleteTurnOptionBoundary::TerminalWin => "Victory",
        CompleteTurnOptionBoundary::TerminalLoss => "Defeat",
        CompleteTurnOptionBoundary::Escape => "Escape",
    }
}
