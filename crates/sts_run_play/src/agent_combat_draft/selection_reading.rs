//! Source-bound selection readings. Generation and adoption stay in the draft owner.
use std::fmt::Write as _;
use sts_presentation::Locale;

use serde::Serialize;
use sts_complete_turn_view::{
    combat_selection_reason_label_v1, render_combat_draft_action_v1, CombatDraftSelectionContextV1,
    CombatDraftSelectionMemberV1, CombatDraftSelectionOutcomesViewV1, CombatDraftSelectionViewV1,
};

use crate::decision::{combat, DecisionSelectionMemberV1};
use sts_oracle_learning::sim::combat_action_equivalence::CombatSelectionStateEquivalenceV1;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SelectionSourceV1 {
    pub source_ref: String,
    pub label: String,
    pub action_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SelectionFamilyV1 {
    pub family: usize,
    pub label: String,
    pub min: u64,
    pub max: u64,
    pub members: Vec<DecisionSelectionMemberV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SelectionOutcomeV1 {
    #[serde(flatten)]
    pub option: super::reading::DraftOptionV1,
    pub selection: CombatDraftSelectionMemberV1,
    pub preferred: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equivalence: Option<CombatSelectionStateEquivalenceV1>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct SelectionReadingV1 {
    pub view: String,
    pub sources: Vec<SelectionSourceV1>,
    pub families: Vec<SelectionFamilyV1>,
    pub preferences: Vec<CombatDraftSelectionMemberV1>,
    pub outcomes: Vec<SelectionOutcomeV1>,
    pub total: usize,
    pub offset: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_work: Option<usize>,
}

pub(super) fn index(
    sources: Vec<SelectionSourceV1>,
    total: usize,
    offset: usize,
    locale: Locale,
) -> SelectionReadingV1 {
    let mut view = super::message(locale, "selections", &[("count", &total.to_string())]);
    for (index, source) in sources.iter().enumerate() {
        writeln!(view, "{index}: {}", source.label).unwrap();
    }
    if offset + sources.len() < total {
        view.push_str(&super::message(locale, "moreSources", &[]));
    }
    SelectionReadingV1 {
        view,
        sources,
        total,
        offset,
        ..Default::default()
    }
}

pub(super) fn focus(
    selection: CombatDraftSelectionViewV1,
    outcomes: CombatDraftSelectionOutcomesViewV1,
    context: Option<CombatDraftSelectionContextV1>,
    remaining_work: usize,
    locale: Locale,
) -> Result<SelectionReadingV1, String> {
    let mut view = super::message(locale, "selection", &[]);
    if let Some(context) = context {
        for (index, action) in context.actions.iter().enumerate() {
            writeln!(
                view,
                "  {index}: {}",
                render_combat_draft_action_v1(action, locale)?
            )
            .unwrap();
        }
        view.push_str(&combat::state_reading(&context.state, locale));
    }
    let families = selection
        .families
        .iter()
        .enumerate()
        .map(|(family, value)| SelectionFamilyV1 {
            family,
            label: combat_selection_reason_label_v1(&value.reason, locale),
            min: value.declared_min,
            max: value.effective_max,
            members: combat::selection_members(&value.domain, locale),
        })
        .collect::<Vec<_>>();
    for family in &families {
        let count = if family.min == family.max {
            family.min.to_string()
        } else {
            format!("{}..{}", family.min, family.max)
        };
        writeln!(
            view,
            "{}",
            super::message(
                locale,
                "family",
                &[("label", &family.label), ("count", &count)]
            )
        )
        .unwrap();
        for (index, member) in family.members.iter().enumerate() {
            writeln!(view, "  {index}: {}", member.label).unwrap();
        }
    }
    for preference in &selection.preferences {
        writeln!(
            view,
            "{}",
            super::message(
                locale,
                "preferred",
                &[(
                    "selection",
                    &selection_label(&families, preference, locale)?
                )]
            )
        )
        .unwrap();
    }
    write!(
        view,
        "{}",
        super::message(
            locale,
            "outcomes",
            &[("count", &outcomes.total.to_string())]
        )
    )
    .unwrap();
    if !outcomes.outcomes.is_empty() {
        write!(
            view,
            "{}",
            super::message(
                locale,
                "showing",
                &[
                    ("first", &(outcomes.offset + 1).to_string()),
                    (
                        "last",
                        &(outcomes.offset + outcomes.outcomes.len()).to_string()
                    )
                ]
            )
        )
        .unwrap();
    }
    if remaining_work == 0 {
        view.push_str(&super::message(locale, "completed", &[]));
    }
    view.push('\n');
    let options = outcomes
        .outcomes
        .into_iter()
        .map(|outcome| {
            let preferred = selection.preferences.contains(&outcome.selection);
            let mut label = format!(
                "{}: {}{}{} | {} | {}",
                outcome.handle,
                selection_label(&families, &outcome.selection, locale)?,
                match outcome.equivalence {
                    Some(CombatSelectionStateEquivalenceV1::GamblingChipCancel) =>
                        super::message(locale, "viaCancel", &[]),
                    None => String::new(),
                },
                if preferred {
                    super::message(locale, "preferredSuffix", &[])
                } else {
                    String::new()
                },
                super::reading::boundary_label(outcome.boundary, locale),
                sts_complete_turn_view::render_combat_draft_successor_v1(
                    &outcome.successor,
                    locale
                )
            );
            write!(
                label,
                "{}",
                super::message(
                    locale,
                    "outcomeActions",
                    &[("count", &outcome.action_count.to_string())]
                )
            )
            .unwrap();
            writeln!(view, "{label}").unwrap();
            Ok(SelectionOutcomeV1 {
                option: super::reading::DraftOptionV1 {
                    option_ref: outcome.handle.to_string(),
                    label,
                    boundary: super::boundary_token(outcome.boundary),
                    action_count: outcome.action_count,
                    successor: outcome.successor,
                },
                selection: outcome.selection,
                preferred,
                equivalence: outcome.equivalence,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SelectionReadingV1 {
        view,
        families,
        preferences: selection.preferences,
        outcomes: options,
        total: outcomes.total,
        offset: outcomes.offset,
        remaining_work: Some(remaining_work),
        sources: vec![],
    })
}

fn selection_label(
    families: &[SelectionFamilyV1],
    selection: &CombatDraftSelectionMemberV1,
    locale: Locale,
) -> Result<String, String> {
    let family = families
        .iter()
        .find(|f| f.family == selection.family)
        .ok_or("selection family missing")?;
    let cards = selection
        .choices
        .iter()
        .map(|ordinal| {
            let (index, member) = family
                .members
                .iter()
                .enumerate()
                .find(|(_, m)| m.ordinal == *ordinal)
                .ok_or("selection member missing")?;
            Ok(format!("{} ({index})", member.label))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(if cards.is_empty() {
        super::message(locale, "zeroCards", &[])
    } else {
        cards.join(", ")
    })
}
