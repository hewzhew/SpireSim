use crate::state::events::{EventActionKind, EventEffect, EventOption, EventOptionTransition};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventBoundaryClassV1 {
    Disabled,
    FlavorAdvance,
    TerminalNoopLeave,
    ForcedStateResolution,
    FollowupSelection,
    FollowupReward,
    CombatStart,
    RepeatableChoice,
    StrategicChoice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventBoundaryClassificationV1 {
    pub class: EventBoundaryClassV1,
}

impl EventBoundaryClassificationV1 {
    pub fn single_auto_advance_reason(self) -> Option<&'static str> {
        match self.class {
            EventBoundaryClassV1::FlavorAdvance | EventBoundaryClassV1::TerminalNoopLeave => {
                Some("routine event transition")
            }
            EventBoundaryClassV1::ForcedStateResolution => Some("forced event resolution"),
            _ => None,
        }
    }

    pub fn single_candidate_note(self) -> Option<&'static str> {
        match self.class {
            EventBoundaryClassV1::FlavorAdvance | EventBoundaryClassV1::TerminalNoopLeave => {
                Some("routine")
            }
            EventBoundaryClassV1::ForcedStateResolution => Some("forced event resolution"),
            _ => None,
        }
    }
}

pub fn classify_event_option_boundary_v1(option: &EventOption) -> EventBoundaryClassificationV1 {
    let class = if option.ui.disabled {
        EventBoundaryClassV1::Disabled
    } else if starts_combat(option) {
        EventBoundaryClassV1::CombatStart
    } else {
        match option.semantics.transition {
            EventOptionTransition::OpenSelection(_) => EventBoundaryClassV1::FollowupSelection,
            EventOptionTransition::Complete if terminal_no_effect_leave(option) => {
                EventBoundaryClassV1::TerminalNoopLeave
            }
            EventOptionTransition::AdvanceScreen | EventOptionTransition::Complete
                if has_no_state_surface(option) && !option.semantics.repeatable =>
            {
                EventBoundaryClassV1::FlavorAdvance
            }
            EventOptionTransition::AdvanceScreen | EventOptionTransition::Complete
                if matches!(option.semantics.action, EventActionKind::Continue)
                    && !option.semantics.repeatable =>
            {
                EventBoundaryClassV1::ForcedStateResolution
            }
            _ if forced_single_resolution_surface(option) => {
                EventBoundaryClassV1::ForcedStateResolution
            }
            _ if option.semantics.repeatable => EventBoundaryClassV1::RepeatableChoice,
            EventOptionTransition::OpenReward => EventBoundaryClassV1::FollowupReward,
            _ => EventBoundaryClassV1::StrategicChoice,
        }
    };

    EventBoundaryClassificationV1 { class }
}

fn starts_combat(option: &EventOption) -> bool {
    matches!(
        option.semantics.transition,
        EventOptionTransition::StartCombat
    ) || matches!(option.semantics.action, EventActionKind::Fight)
        || option
            .semantics
            .effects
            .iter()
            .any(|effect| matches!(effect, EventEffect::StartCombat))
}

fn terminal_no_effect_leave(option: &EventOption) -> bool {
    matches!(option.semantics.action, EventActionKind::Leave)
        && has_no_state_surface(option)
        && option.semantics.terminal
}

fn has_no_state_surface(option: &EventOption) -> bool {
    option.semantics.effects.is_empty() && option.semantics.constraints.is_empty()
}

fn forced_single_resolution_surface(option: &EventOption) -> bool {
    !option.semantics.repeatable
        && option.semantics.constraints.is_empty()
        && !has_card_acquisition_surface(option)
}

fn has_card_acquisition_surface(option: &EventOption) -> bool {
    option.semantics.effects.iter().any(|effect| {
        matches!(
            effect,
            EventEffect::OfferCards { .. }
                | EventEffect::ObtainCard { .. }
                | EventEffect::ObtainColorlessCard { .. }
        )
    })
}
