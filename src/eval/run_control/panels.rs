use crate::content::cards::get_card_definition;
use crate::runtime::combat::CombatCard;
use crate::state::core::EngineState;

use super::decision_surface::{build_decision_surface, DecisionSurface};
use super::session::RunControlSession;
use super::view_model::{combat_card_label, reward_card_label};

mod combat;

pub fn render_run_control_main(session: &RunControlSession) -> String {
    let surface = build_decision_surface(session);
    let mut out = String::new();
    push_header(&mut out, &surface);
    push_line(&mut out, surface.view.decision.label.clone());
    push_visible_screen(session, &mut out);
    push_line(&mut out, "");
    push_candidates(&surface, &mut out);
    push_line(&mut out, "");
    out
}

fn push_header(out: &mut String, surface: &DecisionSurface) {
    push_line(
        out,
        "================================================================================",
    );
    push_line(
        out,
        format!(
            "{} | {}",
            surface.view.header.location, surface.view.header.title
        ),
    );
    push_line(out, surface.view.header.config.clone());
    push_line(
        out,
        "--------------------------------------------------------------------------------",
    );
}

fn push_visible_screen(session: &RunControlSession, out: &mut String) {
    match &session.engine_state {
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_) => combat::push_combat_screen(session, out),
        EngineState::RewardScreen(reward) if reward.pending_card_choice.is_some() => {
            push_reward_card_choice_screen(reward, "reward screen", session, out);
        }
        EngineState::RewardOverlay { reward_state, .. }
            if reward_state.pending_card_choice.is_some() =>
        {
            push_reward_card_choice_screen(reward_state, "overlay reward screen", session, out);
        }
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => {
            push_line(out, "");
            if matches!(session.engine_state, EngineState::MapOverlay { .. }) {
                push_line(
                    out,
                    "Map preview: selecting a path commits travel; `back` returns to rewards.",
                );
            } else {
                push_line(out, "Map navigation boundary.");
            }
        }
        _ => {}
    }
}

fn push_candidates(surface: &DecisionSurface, out: &mut String) {
    push_line(out, "Candidates:");
    if surface.view.candidates.is_empty() {
        push_line(out, "  none");
        return;
    }
    for candidate in &surface.view.candidates {
        push_line(out, format!("  {} | {}", candidate.id, candidate.label));
        if let Some(note) = candidate
            .note
            .as_ref()
            .filter(|note| note.as_str() != "routine")
        {
            push_line(out, format!("      {note}"));
        }
    }
}

pub(super) fn debug_words(raw: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in raw.chars().enumerate() {
        if idx > 0 && ch.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

pub(super) fn card_line(card: &CombatCard, runtime_cost: bool) -> String {
    let def = get_card_definition(card.id);
    let name = combat_card_label(card);
    let cost = if runtime_cost {
        card.cost_for_turn_java()
    } else {
        def.cost as i32
    };
    let mut parts = vec![
        name,
        format!("{:?}", def.card_type),
        format!("{:?}", def.rarity),
        format!("cost {}", format_cost(cost)),
    ];
    if def.base_damage > 0 {
        parts.push(format!(
            "damage {}",
            def.base_damage + def.upgrade_damage * card.upgrades as i32
        ));
    }
    if def.base_block > 0 {
        parts.push(format!(
            "block {}",
            def.base_block + def.upgrade_block * card.upgrades as i32
        ));
    }
    if def.base_magic > 0 {
        parts.push(format!(
            "magic {}",
            def.base_magic + def.upgrade_magic * card.upgrades as i32
        ));
    }
    if def.exhaust {
        parts.push("exhaust".to_string());
    }
    parts.join(" | ")
}

fn reward_card_brief(id: crate::content::cards::CardId, upgrades: u8) -> String {
    let def = get_card_definition(id);
    format!(
        "{} | {:?} | {:?} | cost {}",
        reward_card_label(id, upgrades),
        def.card_type,
        def.rarity,
        format_cost(def.cost as i32)
    )
}

fn push_reward_card_choice_screen(
    reward: &crate::state::rewards::RewardState,
    back_destination: &str,
    session: &RunControlSession,
    out: &mut String,
) {
    let Some(cards) = reward.pending_card_choice.as_ref() else {
        return;
    };
    push_line(out, "");
    push_line(out, "Cards:");
    for (idx, card) in cards.iter().enumerate() {
        push_line(
            out,
            format!("  {idx} {}", reward_card_brief(card.id, card.upgrades)),
        );
    }
    if session
        .run_state
        .relics
        .iter()
        .any(|relic| relic.id == crate::content::relics::RelicId::SingingBowl)
    {
        push_line(out, "  bowl Singing Bowl | gain 2 max HP");
    }
    push_line(out, format!("  back Return to {back_destination}"));
}

fn format_cost(cost: i32) -> String {
    match cost {
        -1 => "X".to_string(),
        -2 => "unplayable".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn push_line(out: &mut String, line: impl AsRef<str>) {
    out.push_str(line.as_ref());
    out.push('\n');
}
