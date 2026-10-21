use std::time::Instant;

use crate::sim::combat::{CombatPosition, EngineCombatStepper};
use crate::state::core::ClientInput;

use super::super::frontier::SearchNode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleCombatRolloutGuideV1 {
    pub components: Vec<i32>,
    pub winning_suffix: Option<OracleCombatRolloutWinningSuffixV1>,
    pub actions_simulated: usize,
    pub action_preview: Vec<ClientInput>,
    pub terminal: super::super::SearchTerminalLabel,
    pub final_hp: i32,
    pub unrecovered_stolen_gold: i32,
    pub contract_satisfied: bool,
    pub stop_reason: &'static str,
    pub last_action_reason: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleCombatRolloutWinningSuffixV1 {
    pub actions: Vec<ClientInput>,
    pub final_hp_hint: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OracleCombatRolloutContractV1 {
    pub minimum_final_hp: Option<i32>,
    pub require_no_unrecovered_stolen_gold: bool,
}

impl OracleCombatRolloutContractV1 {
    pub(in crate::ai::combat_search_v2) fn accepts_terminal_position(
        self,
        position: &CombatPosition,
    ) -> bool {
        self.minimum_final_hp
            .is_none_or(|minimum| position.combat.entities.player.current_hp >= minimum)
            && (!self.require_no_unrecovered_stolen_gold
                || super::super::external_payoff::unrecovered_stolen_gold(&position.combat) == 0)
    }

    fn accepts_terminal_estimate(
        self,
        estimate: &super::super::rollout_estimate::RolloutNodeEstimate,
    ) -> bool {
        self.minimum_final_hp
            .is_none_or(|minimum| estimate.final_hp >= minimum)
            && (!self.require_no_unrecovered_stolen_gold || estimate.unrecovered_stolen_gold == 0)
    }
}

/// Runs one bounded, non-authoritative tactical rollout. Its typed rank remains
/// ordering evidence. A complete terminal-win action suffix may also be
/// exposed as an untrusted proposal; callers must validate it at the exact
/// evaluated state and replay the joined line from their unchanged root.
pub fn oracle_combat_rollout_guide_v1(
    position: &CombatPosition,
    max_actions: usize,
    deadline: Option<Instant>,
    contract: OracleCombatRolloutContractV1,
) -> OracleCombatRolloutGuideV1 {
    let node = SearchNode::root(position.engine.clone(), position.combat.clone());
    let config = super::super::CombatSearchV2Config::default();
    let mut performance = super::super::rollout_profile::RolloutPerformanceCounters::default();
    let estimate = super::super::rollout::phase_aware_no_potion_rollout_for_contract(
        &node,
        &EngineCombatStepper,
        &config,
        max_actions.max(1),
        deadline,
        &mut performance,
        contract,
    );
    let contract_win = estimate.terminal == super::super::SearchTerminalLabel::Win
        && contract.accepts_terminal_estimate(&estimate);
    let (outcome, evidence) = if contract_win {
        // A simulated win is positive existence evidence. The caller still
        // has to generate and replay its own exact witness.
        (2, 2)
    } else if estimate.evaluated {
        // One bounded policy rollout that loses or runs out of horizon is not
        // a refutation of the exact state. Keep it live and let the remaining
        // typed coordinates order this heuristic sample.
        (1, 1)
    } else {
        (1, 0)
    };
    let eval = super::super::value::combat_eval_from_rollout_estimate(&estimate);
    let survival = match eval.survival_bucket() {
        super::super::value::CombatEvalSurvivalBucket::DeadOrForcedLoss => 0,
        super::super::value::CombatEvalSurvivalBucket::LethalVisible => 1,
        super::super::value::CombatEvalSurvivalBucket::Critical => 2,
        super::super::value::CombatEvalSurvivalBucket::Stabilizing => 3,
        super::super::value::CombatEvalSurvivalBucket::Stable => 4,
    };
    let progress = match eval.progress_bucket() {
        super::super::value::CombatEvalProgressBucket::Regression => 0,
        super::super::value::CombatEvalProgressBucket::Stalled => 1,
        super::super::value::CombatEvalProgressBucket::AttritionFavored => 2,
        super::super::value::CombatEvalProgressBucket::RaceFavored => 3,
        super::super::value::CombatEvalProgressBucket::LethalNextTurnLikely => 4,
        super::super::value::CombatEvalProgressBucket::LethalNow => 5,
    };
    let phase_pressure = (estimate.special_enemy_phase_count
        + estimate.guardian_mode_shift_pending_count
        + estimate.lagavulin_waking_count
        + estimate.sentry_dazed_pressure_count
        + estimate.hexaghost_opening_pressure_count
        + usize::from(estimate.high_fanout_pending_choice)) as i32
        + estimate.gremlin_nob_anger_amount_total.max(0)
        + estimate.pending_choice_estimated_action_fanout as i32;
    let action_preview = estimate
        .action_preview
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let winning_suffix = (estimate.is_replayable_terminal_win() && contract_win).then(|| {
        OracleCombatRolloutWinningSuffixV1 {
            actions: action_preview.clone(),
            final_hp_hint: estimate.final_hp,
        }
    });
    OracleCombatRolloutGuideV1 {
        // Positive existence evidence leads. Non-winning bounded rollouts
        // remain live heuristic estimates rather than false refutations.
        // Remaining coordinates are intentionally typed and lexicographic
        // rather than collapsed into one score.
        components: vec![
            outcome,
            evidence,
            survival,
            progress,
            estimate.survival_margin,
            estimate
                .final_hp
                .saturating_add(estimate.persistent_run_value),
            estimate.final_hp,
            estimate.persistent_run_value,
            -estimate.phase_adjusted_enemy_effort,
            -phase_pressure,
            -((estimate.potions_used + estimate.potions_discarded) as i32),
            -(estimate.turns as i32),
            -(estimate.cards_played as i32),
        ],
        winning_suffix,
        actions_simulated: estimate.actions_simulated,
        action_preview,
        terminal: estimate.terminal,
        final_hp: estimate.final_hp,
        unrecovered_stolen_gold: estimate.unrecovered_stolen_gold,
        contract_satisfied: contract_win,
        stop_reason: estimate.stop_reason.label(),
        last_action_reason: estimate.last_action_reason,
    }
}
