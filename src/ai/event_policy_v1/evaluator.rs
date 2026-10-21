use super::types::{
    EventCandidateEvaluationV1, EventCandidateEvidenceV1, EventCandidateTierV1,
    EventDecisionContextV1, EventPolicyClassV1, EventPolicyConfigV1,
};
use crate::ai::random_upgrade_opportunity_v1::RandomUpgradeVerdictV1;
use crate::state::events::EventId;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutopilotPick {
    pub(crate) index: usize,
    pub(crate) label: String,
    pub(crate) confidence: f32,
    pub(crate) reason: String,
}

pub(crate) fn autopilot_picks(
    context: &EventDecisionContextV1,
    config: &EventPolicyConfigV1,
) -> Vec<AutopilotPick> {
    context
        .candidates
        .iter()
        .filter_map(|candidate| autopilot_pick(candidate, context, config))
        .collect()
}

pub(crate) fn evaluate_event_candidate_v1(
    current_hp: i32,
    max_hp: i32,
    candidate: &EventCandidateEvidenceV1,
) -> EventCandidateEvaluationV1 {
    if candidate.disabled {
        return EventCandidateEvaluationV1 {
            score: -10_000,
            tier: EventCandidateTierV1::Blocked,
            reasons: vec!["disabled event option".to_string()],
        };
    }

    let hp_ratio = if max_hp > 0 {
        current_hp.max(0) as f32 / max_hp as f32
    } else {
        0.0
    };
    let mut score = 0;
    let mut reasons = Vec::new();

    match candidate.class {
        EventPolicyClassV1::FreeKnownBenefit => {
            score += 700;
            reasons.push("free known benefit".to_string());
        }
        EventPolicyClassV1::SafeExit => {
            score += 120;
            reasons.push("safe exit preserves current run state".to_string());
        }
        EventPolicyClassV1::MaxHpForHpCost => {
            score += 300;
            score -= candidate.hp_cost.saturating_mul(8);
            reasons.push("max hp for visible hp cost".to_string());
        }
        EventPolicyClassV1::CombatStart => {
            if hp_ratio >= 0.65 {
                score += 260;
                reasons.push("optional combat is plausible at healthy hp".to_string());
            } else if hp_ratio >= 0.45 {
                score += 40;
                reasons.push("optional combat is risky but still explorable".to_string());
            } else {
                score -= 240;
                reasons.push("optional combat is dangerous at low hp".to_string());
            }
        }
        EventPolicyClassV1::CurseDebt => {
            score -= 700 * candidate.curse_count.max(1);
            reasons.push("adds curse deck debt".to_string());
        }
        EventPolicyClassV1::SelectionOrDeckMutation => {
            score -= 80;
            reasons.push("mutates deck identity or opens selection".to_string());
        }
        EventPolicyClassV1::RandomUpgradeOpportunity => {
            if let Some(plan) = &candidate.random_upgrade_opportunity {
                score += plan.score_hint;
                reasons.push(format!(
                    "random upgrade opportunity verdict={:?}",
                    plan.verdict
                ));
                reasons.push(format!(
                    "random upgrade opportunity p_important={:?}",
                    plan.hit_distribution.p_hit_at_least_one_important_or_better
                ));
                if !plan.blockers.is_empty() {
                    reasons.push(format!("random upgrade blockers={:?}", plan.blockers));
                }
            } else {
                score -= 240;
                reasons.push("random upgrade opportunity missing evaluator plan".to_string());
            }
        }
        EventPolicyClassV1::ResourceCost => {
            score -= 120;
            reasons.push("visible resource cost".to_string());
        }
        EventPolicyClassV1::UncertainReward => {
            score -= 160;
            reasons.push("unresolved reward outcome".to_string());
        }
        EventPolicyClassV1::Unknown => {
            score -= 240;
            reasons.push("unknown event option semantics".to_string());
        }
    }

    if candidate.heal_amount > 0 {
        let missing_hp = max_hp.saturating_sub(current_hp).max(0);
        let useful_heal = candidate.heal_amount.min(missing_hp);
        let heal_need_multiplier = if hp_ratio < 0.35 {
            18
        } else if hp_ratio < 0.55 {
            10
        } else if hp_ratio < 0.75 {
            4
        } else {
            0
        };
        let heal_score = useful_heal.saturating_mul(heal_need_multiplier);
        if heal_score > 0 {
            score += heal_score;
            reasons.push(format!("heals useful hp: +{heal_score}"));
        } else {
            reasons.push("heal has little current value".to_string());
        }
    }

    if candidate.max_hp_loss > 0 {
        score -= candidate.max_hp_loss.saturating_mul(14);
        reasons.push("loses max hp".to_string());
    }
    if candidate.hp_cost > 0 && candidate.max_hp_gain == 0 {
        score -= candidate.hp_cost.saturating_mul(6);
        reasons.push("costs current hp".to_string());
    }
    if candidate.obtained_card_count > 0 && candidate.curse_count == 0 {
        score -= candidate.obtained_card_count.saturating_mul(30);
        reasons.push("adds permanent card count".to_string());
    }
    if candidate.obtains_mark_of_the_bloom {
        let bloom_penalty = if hp_ratio < 0.65 {
            620
        } else if hp_ratio < 0.85 {
            420
        } else {
            240
        };
        score -= bloom_penalty;
        reasons.push(format!(
            "Mark of the Bloom disables future healing: -{bloom_penalty}"
        ));
    }

    let tier = if score >= 450 {
        EventCandidateTierV1::Preferred
    } else if score >= 80 {
        EventCandidateTierV1::Viable
    } else if score >= -250 {
        EventCandidateTierV1::Risky
    } else {
        EventCandidateTierV1::Avoid
    };

    EventCandidateEvaluationV1 {
        score,
        tier,
        reasons,
    }
}

fn autopilot_pick(
    candidate: &EventCandidateEvidenceV1,
    context: &EventDecisionContextV1,
    config: &EventPolicyConfigV1,
) -> Option<AutopilotPick> {
    if candidate.disabled {
        return None;
    }
    if let Some(pick) = winding_halls_mark_of_bloom_autopilot_pick(candidate, context) {
        return Some(pick);
    }
    match candidate.class {
        EventPolicyClassV1::FreeKnownBenefit if config.allow_free_known_benefit => {
            Some(AutopilotPick {
                index: candidate.index,
                label: candidate.label.clone(),
                confidence: 0.84,
                reason: "free known public event benefit with no visible downside".to_string(),
            })
        }
        EventPolicyClassV1::SafeExit
            if config.allow_safe_exit_from_risky_event
                && all_other_enabled_candidates_are_risky(context, candidate.index) =>
        {
            Some(AutopilotPick {
                index: candidate.index,
                label: candidate.label.clone(),
                confidence: 0.72,
                reason: "declined event because every other visible option has cost, uncertainty, combat, or deck mutation".to_string(),
            })
        }
        EventPolicyClassV1::MaxHpForHpCost
            if config.allow_max_hp_for_safe_hp_cost
                && max_hp_for_hp_cost_is_safe(context, candidate, config) =>
        {
            Some(AutopilotPick {
                index: candidate.index,
                label: candidate.label.clone(),
                confidence: 0.74,
                reason: format!(
                    "gain {} max HP for {} HP while keeping a safe health buffer",
                    candidate.max_hp_gain, candidate.hp_cost
                ),
            })
        }
        EventPolicyClassV1::RandomUpgradeOpportunity
            if config.allow_random_upgrade_opportunity
                && random_upgrade_opportunity_is_safe(candidate) =>
        {
            Some(AutopilotPick {
                index: candidate.index,
                label: candidate.label.clone(),
                confidence: 0.70,
                reason: "random upgrade opportunity pays current upgrade debt with acceptable HP risk"
                    .to_string(),
            })
        }
        _ => None,
    }
}

fn random_upgrade_opportunity_is_safe(candidate: &EventCandidateEvidenceV1) -> bool {
    let Some(plan) = &candidate.random_upgrade_opportunity else {
        return false;
    };
    matches!(plan.verdict, RandomUpgradeVerdictV1::EnterClean)
        || (matches!(plan.verdict, RandomUpgradeVerdictV1::EnterRisky)
            && candidate.evaluation.tier <= EventCandidateTierV1::Viable)
}

fn winding_halls_mark_of_bloom_autopilot_pick(
    candidate: &EventCandidateEvidenceV1,
    context: &EventDecisionContextV1,
) -> Option<AutopilotPick> {
    if context.event_id != EventId::WindingHalls || !context.has_mark_of_the_bloom {
        return None;
    }
    if candidate.max_hp_loss <= 0
        || candidate.hp_cost > 0
        || candidate.heal_amount > 0
        || candidate.curse_count > 0
        || candidate.obtained_card_count > 0
    {
        return None;
    }
    Some(AutopilotPick {
        index: candidate.index,
        label: candidate.label.clone(),
        confidence: 0.82,
        reason: "Winding Halls: Mark of the Bloom blocks the heal option, so prefer the structured max-HP loss option over curse or deck growth".to_string(),
    })
}

fn max_hp_for_hp_cost_is_safe(
    context: &EventDecisionContextV1,
    candidate: &EventCandidateEvidenceV1,
    config: &EventPolicyConfigV1,
) -> bool {
    if candidate.hp_cost <= 0 || candidate.max_hp_gain <= 0 {
        return false;
    }
    let hp_after = context.current_hp.saturating_sub(candidate.hp_cost);
    if hp_after < config.min_hp_after_safe_hp_cost {
        return false;
    }
    if context.max_hp <= 0 {
        return false;
    }
    let ratio_after = hp_after as f32 / context.max_hp as f32;
    ratio_after >= config.min_hp_ratio_after_safe_hp_cost
}

fn all_other_enabled_candidates_are_risky(
    context: &EventDecisionContextV1,
    selected_index: usize,
) -> bool {
    context
        .candidates
        .iter()
        .filter(|candidate| candidate.index != selected_index && !candidate.disabled)
        .all(|candidate| {
            matches!(
                candidate.class,
                EventPolicyClassV1::ResourceCost
                    | EventPolicyClassV1::CurseDebt
                    | EventPolicyClassV1::SelectionOrDeckMutation
                    | EventPolicyClassV1::CombatStart
                    | EventPolicyClassV1::UncertainReward
                    | EventPolicyClassV1::Unknown
            )
        })
}
