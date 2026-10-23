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

pub(super) fn status(env: &CombatDraftEnvV1, locale: Locale) -> Result<DraftReadingV1, String> {
    let text = |key: &str, params: &[(&str, &str)]| super::message(locale, key, params);
    if let CombatDraftEnvStatusV1::Terminal {
        selected_turn_count,
        boundary,
    } = env.status()
    {
        return Ok(DraftReadingV1 {
            view: text(
                if selected_turn_count == 1 {
                    "terminalOne"
                } else {
                    "terminalOther"
                },
                &[
                    ("boundary", &boundary_label(boundary, locale)),
                    ("count", &selected_turn_count.to_string()),
                ],
            ),
            options: vec![],
        });
    }
    let state = env.observe().map_err(|error| error.to_string())?;
    let mut view = text("prefix", &[]);
    view.push_str(&crate::decision::combat::state_reading(&state.root, locale));
    let coverage = match state.coverage {
        CombatDraftOptionCoverage::Complete => text("complete", &[]),
        CombatDraftOptionCoverage::Open {
            mechanics_gap_count: 0,
        } => text("open", &[]),
        CombatDraftOptionCoverage::Open {
            mechanics_gap_count,
        } => text("openGaps", &[("count", &mechanics_gap_count.to_string())]),
        CombatDraftOptionCoverage::MechanicsGap { gap_count } => {
            text("gaps", &[("count", &gap_count.to_string())])
        }
    };
    writeln!(
        view,
        "{}",
        text(
            "options",
            &[
                ("count", &state.options.len().to_string()),
                ("coverage", &coverage)
            ]
        )
    )
    .unwrap();
    let options = state
        .options
        .iter()
        .map(|option| {
            let first = option
                .first_action
                .as_ref()
                .map(|action| render_combat_draft_action_v1(action, locale))
                .transpose()?
                .unwrap_or_else(|| text("settle", &[]));
            let count = text(
                if option.action_count == 1 {
                    "actionOne"
                } else {
                    "actionOther"
                },
                &[("count", &option.action_count.to_string())],
            );
            let explored = if option.continuation.is_some() {
                text("explored", &[])
            } else {
                String::new()
            };
            let label = text(
                "option",
                &[
                    ("handle", &option.handle.to_string()),
                    ("first", &first),
                    ("actions", &count),
                    ("boundary", &boundary_label(option.boundary, locale)),
                    (
                        "successor",
                        &render_combat_draft_successor_v1(&option.successor, locale),
                    ),
                    ("explored", &explored),
                ],
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
        writeln!(
            view,
            "{}",
            text(
                "moreOptions",
                &[("count", &(options.len() - 12).to_string())]
            )
        )
        .unwrap();
    }
    Ok(DraftReadingV1 { view, options })
}

pub(super) fn inspect(
    env: &CombatDraftEnvV1,
    option_ref: &str,
    locale: Locale,
) -> Result<String, String> {
    let detail = env.inspect(option_ref).map_err(|error| error.to_string())?;
    let mut view = format!(
        "{option_ref} | {}\n",
        boundary_label(detail.boundary, locale)
    );
    for (index, action) in detail.actions.iter().enumerate() {
        writeln!(
            view,
            "  {index}: {}",
            render_combat_draft_action_v1(action, locale)?
        )
        .unwrap();
    }
    view.push_str(&super::message(locale, "successor", &[]));
    view.push_str(&crate::decision::combat::state_reading(
        &detail.successor,
        locale,
    ));
    Ok(view)
}

pub(super) fn boundary_label(boundary: CompleteTurnOptionBoundary, locale: Locale) -> String {
    super::message(
        locale,
        match boundary {
            CompleteTurnOptionBoundary::NextPlayerTurn => "nextTurn",
            CompleteTurnOptionBoundary::TerminalWin => "victory",
            CompleteTurnOptionBoundary::TerminalLoss => "defeat",
            CompleteTurnOptionBoundary::Escape => "escape",
        },
        &[],
    )
}
