use super::super::super::{RolloutNodeEstimate, SearchTerminalLabel};
use super::types::{
    CombatEvalEvidenceKind, CombatEvalOutcomeClass, CombatEvalProgressBucket,
    CombatEvalSurvivalBucket, CombatEvalV2,
};

// CombatEval is an estimate-only ordering surface. It must not become an
// authoritative terminal-outcome boundary; exact terminal trajectories and exact
// state keys remain authoritative.
const CRITICAL_SURVIVAL_MARGIN_MAX: i32 = 6;
const STABILIZING_SURVIVAL_MARGIN_MAX: i32 = 15;
const LIKELY_NEXT_TURN_LETHAL_EFFORT_MAX: i32 = 20;
const RACE_FAVORED_EFFORT_MAX: i32 = 60;
const ATTRITION_FAVORED_EFFORT_MAX: i32 = 120;

pub(in crate::ai::combat_search_v2) fn combat_eval_from_rollout_estimate(
    estimate: &RolloutNodeEstimate,
) -> CombatEvalV2 {
    CombatEvalV2 {
        evidence: rollout_evidence(estimate),
        outcome: outcome_class(estimate.terminal),
        survival: survival_bucket(estimate),
        progress: progress_bucket(estimate),
        risk_margin: estimate.survival_margin,
        persistent_adjusted_hp: estimate
            .final_hp
            .saturating_add(estimate.persistent_run_value),
        final_hp: estimate.final_hp,
        persistent_run_value: estimate.persistent_run_value,
        enemy_progress: -estimate.phase_adjusted_enemy_effort,
        phase_stability: phase_stability(estimate),
        resource_conservation: -((estimate.potions_used + estimate.potions_discarded) as i32),
        faster_turns: -(estimate.turns as i32),
        fewer_cards_played: -(estimate.cards_played as i32),
    }
}

fn rollout_evidence(estimate: &RolloutNodeEstimate) -> CombatEvalEvidenceKind {
    if !estimate.evaluated {
        CombatEvalEvidenceKind::None
    } else if estimate.terminal == SearchTerminalLabel::Unresolved {
        CombatEvalEvidenceKind::UnresolvedEstimate
    } else {
        CombatEvalEvidenceKind::SimulatedTerminal
    }
}

fn outcome_class(terminal: SearchTerminalLabel) -> CombatEvalOutcomeClass {
    match terminal {
        SearchTerminalLabel::Win => CombatEvalOutcomeClass::Win,
        SearchTerminalLabel::Escape => CombatEvalOutcomeClass::Loss,
        SearchTerminalLabel::Unresolved => CombatEvalOutcomeClass::Unresolved,
        SearchTerminalLabel::Loss => CombatEvalOutcomeClass::Loss,
    }
}

fn survival_bucket(estimate: &RolloutNodeEstimate) -> CombatEvalSurvivalBucket {
    if estimate.terminal == SearchTerminalLabel::Loss || estimate.final_hp <= 0 {
        CombatEvalSurvivalBucket::DeadOrForcedLoss
    } else if estimate.survival_margin < 0 {
        CombatEvalSurvivalBucket::LethalVisible
    } else if estimate.survival_margin <= CRITICAL_SURVIVAL_MARGIN_MAX {
        CombatEvalSurvivalBucket::Critical
    } else if estimate.survival_margin <= STABILIZING_SURVIVAL_MARGIN_MAX {
        CombatEvalSurvivalBucket::Stabilizing
    } else {
        CombatEvalSurvivalBucket::Stable
    }
}

fn progress_bucket(estimate: &RolloutNodeEstimate) -> CombatEvalProgressBucket {
    if estimate.terminal == SearchTerminalLabel::Win {
        return CombatEvalProgressBucket::LethalNow;
    }
    if estimate.terminal == SearchTerminalLabel::Loss {
        return CombatEvalProgressBucket::Regression;
    }
    if estimate.terminal == SearchTerminalLabel::Escape {
        return CombatEvalProgressBucket::Regression;
    }

    let effort = estimate.phase_adjusted_enemy_effort.max(0);
    if effort <= LIKELY_NEXT_TURN_LETHAL_EFFORT_MAX {
        CombatEvalProgressBucket::LethalNextTurnLikely
    } else if effort <= RACE_FAVORED_EFFORT_MAX {
        CombatEvalProgressBucket::RaceFavored
    } else if effort <= ATTRITION_FAVORED_EFFORT_MAX {
        CombatEvalProgressBucket::AttritionFavored
    } else {
        CombatEvalProgressBucket::Stalled
    }
}

fn phase_stability(estimate: &RolloutNodeEstimate) -> i32 {
    let pressure = ((estimate.special_enemy_phase_count
        + estimate.guardian_mode_shift_pending_count
        + estimate.lagavulin_waking_count
        + estimate.sentry_dazed_pressure_count
        + estimate.hexaghost_opening_pressure_count
        + usize::from(estimate.high_fanout_pending_choice)) as i32)
        .saturating_add(estimate.gremlin_nob_anger_amount_total.max(0))
        .saturating_add(estimate.pending_choice_estimated_action_fanout as i32);
    -pressure
}
