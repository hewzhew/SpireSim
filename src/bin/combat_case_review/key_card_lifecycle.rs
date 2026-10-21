use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};

use super::focus::{focus_witness_line, CombatReviewFocus};

#[path = "key_card_lifecycle/report.rs"]
mod report;
#[path = "key_card_lifecycle/targets.rs"]
mod targets;
#[cfg(test)]
#[path = "key_card_lifecycle/tests.rs"]
mod tests;
#[path = "key_card_lifecycle/tracking.rs"]
mod tracking;
#[path = "key_card_lifecycle/types.rs"]
mod types;
#[path = "key_card_lifecycle/zones.rs"]
mod zones;

pub(super) use types::KeyCardLifecycleReport;

use report::{finish_lifecycles, report_without_focus};
use tracking::{note_played_key_card, note_seen_zones, tracked_key_cards};

pub(super) fn key_card_lifecycle(
    root: &CombatPosition,
    focus: Option<&CombatReviewFocus>,
) -> Option<KeyCardLifecycleReport> {
    let mut tracked_cards = tracked_key_cards(&root.combat);
    if tracked_cards.is_empty() {
        return None;
    }

    let Some(focus) = focus else {
        return Some(report_without_focus(tracked_cards));
    };
    let witness = focus_witness_line(focus);
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut replayed_actions = 0usize;
    let mut truncated = false;
    let mut timed_out = false;

    for (index, action) in witness.actions.iter().cloned().enumerate() {
        if stepper.terminal(&position) != CombatTerminal::Unresolved {
            break;
        }
        let step_index = index + 1;
        note_played_key_card(
            &mut tracked_cards,
            step_index,
            &action.action_key,
            &action.input,
        );
        let step = stepper.apply_to_stable(
            &position,
            action.input,
            CombatStepLimits {
                max_engine_steps: 250,
                deadline: None,
            },
        );
        replayed_actions = replayed_actions.saturating_add(1);
        truncated |= step.truncated;
        timed_out |= step.timed_out;
        position = step.position;
        note_seen_zones(&mut tracked_cards, step_index, &position.combat);
        if truncated || timed_out || step.terminal != CombatTerminal::Unresolved {
            break;
        }
    }

    let truncated_by_preview = witness
        .action_count
        .is_some_and(|count| count > witness.actions.len());
    Some(KeyCardLifecycleReport {
        schema: "key_card_lifecycle_v0",
        contract: "exact_replay_key_card_visibility_and_play_timing_no_strategy_verdict",
        basis_line: focus.selected_review,
        witness_action_count: witness.action_count,
        replayed_actions,
        truncated_by_preview,
        truncated,
        timed_out,
        tracked_cards: finish_lifecycles(tracked_cards, replayed_actions, &position.combat),
    })
}
