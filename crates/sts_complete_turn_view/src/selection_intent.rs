use std::fmt::Write;

use serde::{Deserialize, Serialize};
use sts_agent::agent::information::action::{
    project_public_combat_action_surface_v1, public_choice_for_exact_combat_input_v1,
    CombatActionProjectionV1, PublicCombatActionChoiceV1, PublicCombatSelectionDomainCandidateV1,
    PublicCombatSelectionFamilyV1,
};
use sts_combat_planner::{CombatDraftSession, SelectionSource};
use sts_core::content::{cards::CardId, identity::gameplay_display_label_v1};
use sts_core::sim::combat_action_surface::CombatSelectionReasonV2;
use sts_core::state::core::{GridSelectReason, HandSelectReason};

/// Ordered public occurrences, resolved only against the supplied source.
/// An empty `choices` list is a zero-card selection, not withdrawal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatDraftSelectionMemberV1 {
    pub family: usize,
    pub choices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftSelectionViewV1 {
    pub families: Vec<PublicCombatSelectionFamilyV1>,
    pub preferences: Vec<CombatDraftSelectionMemberV1>,
}

/// Executor-side occurrence bindings. The private table stays in-process;
/// display callers use `project_combat_draft_selection_v1` below.
pub fn project_combat_draft_selection_surface_at_v1(
    draft: &CombatDraftSession,
    source: &SelectionSource,
) -> Result<CombatActionProjectionV1, String> {
    let surface = draft
        .selection_surface(source, &sts_core::sim::combat::EngineCombatStepper)
        .map_err(|error| format!("selection surface unavailable: {error:?}"))?;
    project_public_combat_action_surface_v1(&surface, &source.position().combat)
        .map_err(|error| error.to_string())
}

pub fn project_combat_draft_selection_v1(
    draft: &CombatDraftSession,
) -> Result<CombatDraftSelectionViewV1, String> {
    let source = draft
        .root_selection_intent()
        .ok_or("combat draft is terminal")?
        .source;
    project_combat_draft_selection_at_v1(draft, &source)
}

pub fn project_combat_draft_selection_at_v1(
    draft: &CombatDraftSession,
    source: &SelectionSource,
) -> Result<CombatDraftSelectionViewV1, String> {
    let projection = project_combat_draft_selection_surface_at_v1(draft, source)?;
    let intent = draft.selection_intent().expect("active draft has a root");
    let preferences = intent
        .members
        .iter()
        .filter(|_| intent.source == *source)
        .map(
            |input| match public_choice_for_exact_combat_input_v1(&projection, input)? {
                PublicCombatActionChoiceV1::Selection {
                    family_ordinal,
                    selected_domain_ordinals,
                } => Ok(CombatDraftSelectionMemberV1 {
                    family: family_ordinal,
                    choices: selected_domain_ordinals,
                }),
                PublicCombatActionChoiceV1::Atomic { .. } => {
                    Err("selection intention contains an atomic action".to_owned())
                }
            },
        )
        .collect::<Result<Vec<_>, String>>()?;
    Ok(CombatDraftSelectionViewV1 {
        families: projection.public.selection_families,
        preferences,
    })
}

/// Cheap index summary: only the first action needs the shared root projection.
/// Full prefix replay and public state are expanded on demand below.
pub fn summarize_combat_draft_selection_source_v1(
    draft: &CombatDraftSession,
    source: &SelectionSource,
) -> Result<String, String> {
    let view = project_combat_draft_selection_at_v1(draft, source)?;
    let reasons = view
        .families
        .iter()
        .map(|f| reason_label(&f.reason))
        .collect::<Vec<_>>()
        .join(", ");
    let prefix = if source.action_count() == 0 {
        "Now".to_owned()
    } else {
        let root = draft.current_root().ok_or("combat draft is terminal")?;
        let observation =
            sts_agent::agent::information::state::public_combat_state_v1(&root.position().combat);
        let projection = sts_agent::agent::information::action::project_public_combat_actions_v1(
            &root.position().engine,
            &root.position().combat,
        )
        .map_err(|e| e.to_string())?;
        let choice = public_choice_for_exact_combat_input_v1(
            &projection,
            source.first_input().expect("non-empty source prefix"),
        )?;
        let action = crate::selected_action(&observation, &projection.public, &choice)?;
        format!(
            "After {} action{} | First: {}",
            source.action_count(),
            if source.action_count() == 1 { "" } else { "s" },
            crate::combat_draft::render_action_brief(&action)?
        )
    };
    Ok(format!(
        "{prefix} | {reasons}{}",
        if view.preferences.is_empty() {
            ""
        } else {
            " | Preferred"
        }
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftSelectionContextV1 {
    pub actions: Vec<crate::CompleteTurnSelectedActionV1>,
    pub state: sts_agent::agent::information::state::PublicCombatStateV1,
}

pub fn project_combat_draft_selection_context_v1(
    draft: &CombatDraftSession,
    source: &SelectionSource,
    max_steps: usize,
) -> Result<CombatDraftSelectionContextV1, String> {
    draft
        .validate_selection_source(source)
        .map_err(|e| format!("{e:?}"))?;
    let actions = if source.action_count() > 0 {
        let root = draft.current_root().ok_or("combat draft is terminal")?;
        let prefix = source.actions();
        let (actions, replayed) = crate::replay_and_project_actions(
            root.position(),
            prefix
                .iter()
                .map(|a| (&a.input, Some(a.expected_successor_hash.as_str()))),
            max_steps,
            "selection source",
        )?;
        if replayed != *source.position() {
            return Err("selection source replay drifted".to_owned());
        }
        actions
    } else {
        Vec::new()
    };
    Ok(CombatDraftSelectionContextV1 {
        actions,
        state: sts_agent::agent::information::state::public_combat_state_v1(
            &source.position().combat,
        ),
    })
}

pub fn render_combat_draft_selection_v1(view: &CombatDraftSelectionViewV1) -> String {
    let mut text = String::new();
    if view.families.is_empty() {
        return "Selection: none\n".to_owned();
    }
    for (family_index, family) in view.families.iter().enumerate() {
        writeln!(
            text,
            "Family {family_index} | {} | choose {}..{} in order",
            reason_label(&family.reason),
            family.declared_min,
            family.effective_max
        )
        .unwrap();
        for (ordinal, candidate) in family.domain.iter().enumerate() {
            let (card, upgrades, eligible) = match candidate {
                PublicCombatSelectionDomainCandidateV1::Card {
                    card_id,
                    upgrades,
                    eligible,
                    ..
                } => (*card_id, *upgrades, *eligible),
                PublicCombatSelectionDomainCandidateV1::Scry {
                    card_id,
                    currently_present,
                    ..
                } => (*card_id, None, *currently_present),
            };
            let name = card_name(card, upgrades);
            writeln!(
                text,
                "  {ordinal} {name}{}",
                if eligible { "" } else { " (unavailable)" }
            )
            .unwrap();
        }
    }
    if view.preferences.is_empty() {
        text.push_str("Preferred: none\n");
    } else {
        for preference in &view.preferences {
            writeln!(
                text,
                "Preferred: family {} choices {:?}",
                preference.family, preference.choices
            )
            .unwrap();
        }
    }
    text
}

pub fn reason_label(reason: &CombatSelectionReasonV2) -> String {
    match reason {
        CombatSelectionReasonV2::Hand(reason) => match reason {
            HandSelectReason::Exhaust | HandSelectReason::PotionExhaust { .. } => {
                "Exhaust from hand".to_owned()
            }
            HandSelectReason::Discard => "Discard from hand".to_owned(),
            HandSelectReason::Retain => "Retain".to_owned(),
            HandSelectReason::PutOnDrawPile => "Put on draw pile".to_owned(),
            HandSelectReason::PutToBottomOfDraw => "Put at bottom of draw pile".to_owned(),
            HandSelectReason::Setup => "Setup".to_owned(),
            HandSelectReason::Copy { amount } => {
                format!("Copy: add {} copies", amount.saturating_sub(1))
            }
            HandSelectReason::Nightmare { amount } => {
                format!("Nightmare: {amount} copies next turn")
            }
            HandSelectReason::Upgrade => "Upgrade".to_owned(),
            HandSelectReason::GamblingChip => "Replace hand cards".to_owned(),
            HandSelectReason::Recycle => "Recycle".to_owned(),
        },
        CombatSelectionReasonV2::Grid(reason) => match reason {
            GridSelectReason::MoveToDrawPile => "Put on draw pile".to_owned(),
            GridSelectReason::Exhume { .. } => "Exhume".to_owned(),
            GridSelectReason::DrawPileToHand => "Draw pile to hand".to_owned(),
            GridSelectReason::SkillFromDeckToHand => "Skill from draw pile".to_owned(),
            GridSelectReason::AttackFromDeckToHand => "Attack from draw pile".to_owned(),
            GridSelectReason::DiscardToHand => "Discard to hand, free this turn".to_owned(),
            GridSelectReason::DiscardToHandNoCostChange => "Discard to hand".to_owned(),
            GridSelectReason::DiscardToHandRetain => "Discard to hand and retain".to_owned(),
            GridSelectReason::Omniscience { play_amount } => {
                format!("Omniscience: play {play_amount} times")
            }
        },
        CombatSelectionReasonV2::ScryDiscard => "Scry: discard".to_owned(),
    }
}

pub(crate) fn card_name(card: Option<CardId>, upgrades: Option<u8>) -> String {
    let mut name = card
        .map(gameplay_display_label_v1)
        .unwrap_or("Unknown card")
        .to_owned();
    if let Some(upgrades) = upgrades.filter(|n| *n > 0) {
        name.push('+');
        if card == Some(CardId::SearingBlow) || upgrades > 1 {
            write!(name, "{upgrades}").unwrap();
        }
    }
    name
}
