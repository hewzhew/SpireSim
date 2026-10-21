use crate::ai::noncombat_strategy_v1::{
    build_run_strategy_snapshot_from_run_state_v2, StrategyPlanSupportV1,
};
use crate::ai::random_upgrade_opportunity_v1::{
    evaluate_random_upgrade_opportunity_v1, RandomUpgradeOpportunityPlanV1, RandomUpgradeSourceV1,
    RandomUpgradeVerdictV1,
};
use crate::content::relics::RelicId;
use crate::state::events::{
    EventActionKind, EventEffect, EventId, EventOption, EventOptionTransition,
};
use crate::state::run::RunState;

use super::evaluator::{autopilot_picks, evaluate_event_candidate_v1};
use super::types::{
    EventCandidateEvidenceV1, EventDecisionContextV1, EventDecisionV1, EventPolicyActionV1,
    EventPolicyClassV1, EventPolicyConfigV1,
};

pub fn build_event_decision_context_v1(
    run_state: &RunState,
    event_id: EventId,
    options: Vec<EventOption>,
) -> EventDecisionContextV1 {
    let strategy = build_run_strategy_snapshot_from_run_state_v2(run_state);
    let candidates = options
        .into_iter()
        .enumerate()
        .map(|(index, option)| candidate_evidence(index, option, run_state, event_id))
        .collect();
    EventDecisionContextV1 {
        event_id,
        strategy,
        current_hp: run_state.current_hp,
        max_hp: run_state.max_hp,
        has_mark_of_the_bloom: run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::MarkOfTheBloom),
        candidates,
    }
}

pub fn plan_event_decision_v1(
    context: &EventDecisionContextV1,
    config: &EventPolicyConfigV1,
) -> EventDecisionV1 {
    if context.event_id == EventId::Neow {
        return stop(
            context,
            "Neow choices remain explicit human strategy boundaries",
        );
    }

    let autopilot_picks = autopilot_picks(context, config);

    let action = match autopilot_picks.as_slice() {
        [pick] => EventPolicyActionV1::Pick {
            index: pick.index,
            label: pick.label.clone(),
            confidence: pick.confidence,
            reason: pick.reason.clone(),
        },
        [] => EventPolicyActionV1::Stop {
            reason: stop_reason(context),
        },
        _ => EventPolicyActionV1::Stop {
            reason: "event policy stopped because multiple autopilot picks matched".to_string(),
        },
    };

    EventDecisionV1 {
        action,
        label_role: "behavior_policy_not_teacher",
        context: context.clone(),
    }
}

fn candidate_evidence(
    index: usize,
    option: EventOption,
    run_state: &RunState,
    event_id: EventId,
) -> EventCandidateEvidenceV1 {
    let hp_cost = hp_cost(&option);
    let max_hp_loss = max_hp_loss(&option);
    let heal_amount = heal_amount(&option);
    let max_hp_gain = max_hp_gain(&option);
    let curse_count = curse_count(&option);
    let obtained_card_count = obtained_card_count(&option);
    let obtains_mark_of_the_bloom = obtains_mark_of_the_bloom(&option);
    let random_upgrade_opportunity = random_upgrade_opportunity(&option, run_state, event_id);
    let class = classify_event_option(&option, random_upgrade_opportunity.as_ref());
    let mut evidence = vec![format!(
        "event action kind is {:?}",
        option.semantics.action
    )];
    evidence.push(format!("transition is {:?}", option.semantics.transition));
    let mut risks = Vec::new();

    match class {
        EventPolicyClassV1::FreeKnownBenefit => {
            evidence.push("known positive effect with no visible cost".to_string());
        }
        EventPolicyClassV1::SafeExit => {
            evidence.push("exit/decline option with no visible cost".to_string());
        }
        EventPolicyClassV1::MaxHpForHpCost => {
            evidence.push(format!("gain {max_hp_gain} max HP for {hp_cost} HP"));
            risks.push("visible current HP cost".to_string());
        }
        EventPolicyClassV1::RandomUpgradeOpportunity => {
            if let Some(plan) = &random_upgrade_opportunity {
                evidence.extend(plan.evidence.iter().cloned());
                risks.extend(plan.risks.iter().cloned());
                if matches!(plan.verdict, RandomUpgradeVerdictV1::Leave) {
                    risks.push(
                        "random upgrade opportunity evaluator recommends leaving".to_string(),
                    );
                }
            }
        }
        EventPolicyClassV1::ResourceCost => {
            risks.push("visible resource cost".to_string());
        }
        EventPolicyClassV1::CurseDebt => {
            risks.push("adds curse or similar deck debt".to_string());
        }
        EventPolicyClassV1::SelectionOrDeckMutation => {
            risks.push("opens selection or mutates deck identity".to_string());
        }
        EventPolicyClassV1::CombatStart => {
            risks.push("starts combat".to_string());
        }
        EventPolicyClassV1::UncertainReward => {
            risks.push("contains random or unresolved reward outcome".to_string());
        }
        EventPolicyClassV1::Unknown => {
            risks.push("event policy did not select this option for autopilot".to_string());
        }
    }
    if obtains_mark_of_the_bloom {
        evidence.push("obtains Mark of the Bloom".to_string());
        risks.push("Mark of the Bloom disables all future healing".to_string());
    }

    let mut candidate = EventCandidateEvidenceV1 {
        index,
        label: display_event_label(&option.ui.text),
        class,
        evaluation: super::types::EventCandidateEvaluationV1 {
            score: 0,
            tier: super::types::EventCandidateTierV1::Risky,
            reasons: Vec::new(),
        },
        support_gate: support_gate_for_class(class),
        evidence,
        risks,
        disabled: option.ui.disabled,
        hp_cost,
        max_hp_loss,
        heal_amount,
        max_hp_gain,
        curse_count,
        obtained_card_count,
        obtains_mark_of_the_bloom,
        random_upgrade_opportunity,
    };
    candidate.evaluation =
        evaluate_event_candidate_v1(run_state.current_hp, run_state.max_hp, &candidate);
    candidate
}

fn classify_event_option(
    option: &EventOption,
    random_upgrade_opportunity: Option<&RandomUpgradeOpportunityPlanV1>,
) -> EventPolicyClassV1 {
    if option.ui.disabled {
        return EventPolicyClassV1::Unknown;
    }
    if is_safe_exit(option) {
        return EventPolicyClassV1::SafeExit;
    }
    if starts_combat(option) {
        return EventPolicyClassV1::CombatStart;
    }
    if has_curse_debt(option) {
        return EventPolicyClassV1::CurseDebt;
    }
    if random_upgrade_opportunity.is_some() {
        return EventPolicyClassV1::RandomUpgradeOpportunity;
    }
    if has_selection_or_deck_mutation(option) {
        return EventPolicyClassV1::SelectionOrDeckMutation;
    }
    if is_max_hp_for_hp_cost(option) {
        return EventPolicyClassV1::MaxHpForHpCost;
    }
    if has_visible_cost(option) {
        return EventPolicyClassV1::ResourceCost;
    }
    if has_unresolved_reward(option) {
        return EventPolicyClassV1::UncertainReward;
    }
    if has_known_positive_effect(option) && !has_negative_or_agency_effect(option) {
        return EventPolicyClassV1::FreeKnownBenefit;
    }
    EventPolicyClassV1::Unknown
}

fn random_upgrade_opportunity(
    option: &EventOption,
    run_state: &RunState,
    event_id: EventId,
) -> Option<RandomUpgradeOpportunityPlanV1> {
    if event_id != EventId::ShiningLight {
        return None;
    }
    let upgrade_count = option
        .semantics
        .effects
        .iter()
        .find_map(|effect| match effect {
            EventEffect::UpgradeCard { count } => Some(*count),
            _ => None,
        })?;
    if hp_cost(option) <= 0 {
        return None;
    }
    Some(evaluate_random_upgrade_opportunity_v1(
        run_state,
        RandomUpgradeSourceV1::ShiningLight {
            hp_cost: hp_cost(option),
            upgrade_count,
        },
    ))
}

fn is_max_hp_for_hp_cost(option: &EventOption) -> bool {
    hp_cost(option) > 0
        && max_hp_gain(option) > 0
        && option
            .semantics
            .effects
            .iter()
            .all(|effect| matches!(effect, EventEffect::LoseHp(_) | EventEffect::GainMaxHp(_)))
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

fn max_hp_loss(option: &EventOption) -> i32 {
    option
        .semantics
        .effects
        .iter()
        .map(|effect| match effect {
            EventEffect::LoseMaxHp(value) => *value,
            _ => 0,
        })
        .sum()
}

fn heal_amount(option: &EventOption) -> i32 {
    option
        .semantics
        .effects
        .iter()
        .map(|effect| match effect {
            EventEffect::Heal(value) => *value,
            _ => 0,
        })
        .sum()
}

fn max_hp_gain(option: &EventOption) -> i32 {
    option
        .semantics
        .effects
        .iter()
        .map(|effect| match effect {
            EventEffect::GainMaxHp(value) => *value,
            _ => 0,
        })
        .sum()
}

fn curse_count(option: &EventOption) -> i32 {
    option
        .semantics
        .effects
        .iter()
        .map(|effect| match effect {
            EventEffect::ObtainCurse { count, .. } => *count as i32,
            _ => 0,
        })
        .sum()
}

fn obtained_card_count(option: &EventOption) -> i32 {
    option
        .semantics
        .effects
        .iter()
        .map(|effect| match effect {
            EventEffect::ObtainCard { count, .. }
            | EventEffect::ObtainColorlessCard { count, .. } => *count as i32,
            _ => 0,
        })
        .sum()
}

fn obtains_mark_of_the_bloom(option: &EventOption) -> bool {
    option.semantics.effects.iter().any(|effect| {
        matches!(
            effect,
            EventEffect::ObtainRelic {
                kind: crate::state::events::EventRelicKind::Specific(RelicId::MarkOfTheBloom),
                ..
            }
        )
    })
}

fn is_safe_exit(option: &EventOption) -> bool {
    option.semantics.effects.is_empty()
        && matches!(
            option.semantics.action,
            EventActionKind::Leave | EventActionKind::Decline
        )
        && matches!(
            option.semantics.transition,
            EventOptionTransition::Complete | EventOptionTransition::AdvanceScreen
        )
        && label_is_exit_like(&option.ui.text)
}

fn label_is_exit_like(label: &str) -> bool {
    let cleaned = event_label_action_token(label).to_ascii_lowercase();
    matches!(cleaned.as_str(), "leave" | "ignore" | "proceed" | "return")
}

fn starts_combat(option: &EventOption) -> bool {
    option
        .semantics
        .effects
        .iter()
        .any(|effect| matches!(effect, EventEffect::StartCombat))
        || matches!(
            option.semantics.transition,
            EventOptionTransition::StartCombat
        )
        || matches!(option.semantics.action, EventActionKind::Fight)
}

fn has_curse_debt(option: &EventOption) -> bool {
    option
        .semantics
        .effects
        .iter()
        .any(|effect| matches!(effect, EventEffect::ObtainCurse { .. }))
}

fn has_selection_or_deck_mutation(option: &EventOption) -> bool {
    matches!(
        option.semantics.transition,
        EventOptionTransition::OpenSelection(_)
    ) || option.semantics.effects.iter().any(|effect| {
        matches!(
            effect,
            EventEffect::RemoveCard { .. }
                | EventEffect::TransformCard { .. }
                | EventEffect::DuplicateCard { .. }
                | EventEffect::UpgradeCard { .. }
                | EventEffect::UpgradeAllCards
        )
    })
}

fn has_visible_cost(option: &EventOption) -> bool {
    option.semantics.effects.iter().any(|effect| {
        matches!(
            effect,
            EventEffect::LoseGold(_)
                | EventEffect::LoseHp(_)
                | EventEffect::LoseMaxHp(_)
                | EventEffect::LoseRelic { .. }
                | EventEffect::LoseStarterRelic { .. }
        )
    })
}

fn has_unresolved_reward(option: &EventOption) -> bool {
    option.semantics.effects.iter().any(|effect| {
        matches!(
            effect,
            EventEffect::ObtainRelic { .. }
                | EventEffect::ObtainPotion { .. }
                | EventEffect::ObtainCard { .. }
                | EventEffect::ObtainColorlessCard { .. }
                | EventEffect::OfferCards { .. }
        )
    }) || matches!(
        option.semantics.transition,
        EventOptionTransition::OpenReward
    )
}

fn has_known_positive_effect(option: &EventOption) -> bool {
    option.semantics.effects.iter().any(|effect| {
        matches!(
            effect,
            EventEffect::GainGold(_) | EventEffect::Heal(_) | EventEffect::GainMaxHp(_)
        )
    })
}

fn has_negative_or_agency_effect(option: &EventOption) -> bool {
    has_visible_cost(option)
        || has_curse_debt(option)
        || has_selection_or_deck_mutation(option)
        || starts_combat(option)
        || has_unresolved_reward(option)
}

fn support_gate_for_class(class: EventPolicyClassV1) -> StrategyPlanSupportV1 {
    match class {
        EventPolicyClassV1::FreeKnownBenefit | EventPolicyClassV1::SafeExit => {
            StrategyPlanSupportV1::Strong
        }
        EventPolicyClassV1::MaxHpForHpCost | EventPolicyClassV1::RandomUpgradeOpportunity => {
            StrategyPlanSupportV1::Plausible
        }
        EventPolicyClassV1::ResourceCost
        | EventPolicyClassV1::CurseDebt
        | EventPolicyClassV1::SelectionOrDeckMutation
        | EventPolicyClassV1::CombatStart
        | EventPolicyClassV1::UncertainReward
        | EventPolicyClassV1::Unknown => StrategyPlanSupportV1::Blocked,
    }
}

fn stop(context: &EventDecisionContextV1, reason: impl Into<String>) -> EventDecisionV1 {
    EventDecisionV1 {
        action: EventPolicyActionV1::Stop {
            reason: reason.into(),
        },
        label_role: "behavior_policy_not_teacher",
        context: context.clone(),
    }
}

fn stop_reason(context: &EventDecisionContextV1) -> String {
    if context.candidates.is_empty() {
        return "event policy stopped because there are no candidates".to_string();
    }
    let classes = context
        .candidates
        .iter()
        .map(|candidate| format!("{}:{:?}", candidate.label, candidate.class))
        .collect::<Vec<_>>()
        .join(", ");
    format!("event policy stopped because no autopilot pick matched ({classes})")
}

fn display_event_label(label: &str) -> String {
    label.trim().trim_end_matches('.').to_string()
}

fn event_label_action_token(label: &str) -> String {
    let cleaned = label.trim().trim_end_matches('.');
    if let Some(stripped) = cleaned.strip_prefix('[') {
        if let Some((token, _rest)) = stripped.split_once(']') {
            return token.trim().to_string();
        }
    }
    cleaned
        .split_whitespace()
        .next()
        .unwrap_or(cleaned)
        .trim_matches(|ch| ch == '[' || ch == ']')
        .to_string()
}
