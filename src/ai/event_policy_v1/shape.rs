use crate::state::events::{EventActionKind, EventEffect, EventOption, EventOptionTransition};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventDecisionShapeV1 {
    Standard,
    RepeatablePaidMenu(RepeatablePaidMenuShapeV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepeatablePaidMenuShapeV1 {
    pub exit_index: usize,
    pub exit_cost_hp: i32,
    pub paid_option_indices: Vec<usize>,
}

pub fn classify_event_decision_shape_v1(options: &[EventOption]) -> EventDecisionShapeV1 {
    let paid_option_indices = options
        .iter()
        .enumerate()
        .filter_map(|(index, option)| repeatable_paid_option(option).then_some(index))
        .collect::<Vec<_>>();
    if paid_option_indices.is_empty() {
        return EventDecisionShapeV1::Standard;
    }

    let Some((exit_index, exit)) = options
        .iter()
        .enumerate()
        .find(|(_, option)| exit_option_for_paid_menu(option))
    else {
        return EventDecisionShapeV1::Standard;
    };

    EventDecisionShapeV1::RepeatablePaidMenu(RepeatablePaidMenuShapeV1 {
        exit_index,
        exit_cost_hp: hp_cost(exit),
        paid_option_indices,
    })
}

fn repeatable_paid_option(option: &EventOption) -> bool {
    option.semantics.repeatable
        && !exit_option_for_paid_menu(option)
        && visible_cost(option)
        && matches!(
            option.semantics.transition,
            EventOptionTransition::None | EventOptionTransition::AdvanceScreen
        )
}

fn exit_option_for_paid_menu(option: &EventOption) -> bool {
    !option.semantics.repeatable
        && matches!(
            option.semantics.action,
            EventActionKind::Leave | EventActionKind::Decline
        )
        && matches!(
            option.semantics.transition,
            EventOptionTransition::AdvanceScreen | EventOptionTransition::Complete
        )
}

fn visible_cost(option: &EventOption) -> bool {
    option.semantics.effects.iter().any(|effect| {
        matches!(
            effect,
            EventEffect::LoseHp(_)
                | EventEffect::LoseGold(_)
                | EventEffect::LoseMaxHp(_)
                | EventEffect::LoseRelic { .. }
                | EventEffect::LoseStarterRelic { .. }
        )
    })
}

fn hp_cost(option: &EventOption) -> i32 {
    option
        .semantics
        .effects
        .iter()
        .map(|effect| match effect {
            EventEffect::LoseHp(value) => *value,
            _ => 0,
        })
        .sum()
}
