use super::pending_choice_action_prefix::canonical_pending_choice_inputs;
use super::phase_profile::{PendingChoicePhaseKind, PendingChoicePhaseProfileV1};
use crate::sim::combat::{CombatPosition, CombatStepper};
use crate::sim::combat_action::CombatActionChoice;
use crate::state::core::{EngineState, PendingChoice};

/// Materialize only selection families whose size grows linearly with the
/// frozen candidate domain.  Single-card exhaust/discard/tutor choices are
/// ordinary rollout decisions; subset choices such as Gambling Chip remain
/// owned by the main search instead of exploding into a powerset here.
pub(super) fn linear_pending_choice_actions(
    position: &CombatPosition,
    stepper: &impl CombatStepper,
) -> Option<Vec<CombatActionChoice>> {
    let EngineState::PendingChoice(choice) = &position.engine else {
        return None;
    };
    let is_linear = match choice {
        PendingChoice::HandSelect { max_cards, .. }
        | PendingChoice::GridSelect { max_cards, .. } => *max_cards <= 1,
        _ => false,
    };
    if !is_linear {
        return None;
    }

    let actions = canonical_pending_choice_inputs(choice)?
        .filter_map(|input| stepper.choice_for_legal_input(position, &input))
        .collect::<Vec<_>>();
    Some(actions)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RolloutPendingChoiceProgress {
    pub(super) pending_choices_seen: usize,
    pub(super) pending_choice_actions_simulated: usize,
    pub(super) max_pending_choice_candidate_count: usize,
    pub(super) max_pending_choice_estimated_action_fanout: usize,
    pub(super) last_pending_choice_kind: Option<PendingChoicePhaseKind>,
    pub(super) stopped_on_high_fanout_pending_choice: bool,
}

impl RolloutPendingChoiceProgress {
    pub(super) fn observe_boundary(&mut self, pending_choice: PendingChoicePhaseProfileV1) {
        if !pending_choice.present {
            return;
        }

        self.pending_choices_seen = self.pending_choices_seen.saturating_add(1);
        self.max_pending_choice_candidate_count = self
            .max_pending_choice_candidate_count
            .max(pending_choice.candidate_count);
        self.max_pending_choice_estimated_action_fanout = self
            .max_pending_choice_estimated_action_fanout
            .max(pending_choice.estimated_action_fanout);
        self.last_pending_choice_kind = pending_choice.kind;
        if pending_choice.high_fanout {
            self.stopped_on_high_fanout_pending_choice = true;
        }
    }

    pub(super) fn note_simulated_action(&mut self, pending_choice: PendingChoicePhaseProfileV1) {
        if pending_choice.present {
            self.pending_choice_actions_simulated =
                self.pending_choice_actions_simulated.saturating_add(1);
        }
    }

    pub(super) fn last_pending_choice_kind_label(self) -> Option<&'static str> {
        self.last_pending_choice_kind
            .map(PendingChoicePhaseKind::label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_tracks_high_fanout_boundary_without_action() {
        let mut progress = RolloutPendingChoiceProgress::default();

        progress.observe_boundary(PendingChoicePhaseProfileV1 {
            present: true,
            kind: Some(PendingChoicePhaseKind::ScrySelect),
            candidate_count: 7,
            estimated_action_fanout: 128,
            high_fanout: true,
        });

        assert_eq!(progress.pending_choices_seen, 1);
        assert_eq!(progress.pending_choice_actions_simulated, 0);
        assert_eq!(progress.max_pending_choice_candidate_count, 7);
        assert_eq!(progress.max_pending_choice_estimated_action_fanout, 128);
        assert_eq!(
            progress.last_pending_choice_kind_label(),
            Some("scry_select")
        );
        assert!(progress.stopped_on_high_fanout_pending_choice);
    }
}
