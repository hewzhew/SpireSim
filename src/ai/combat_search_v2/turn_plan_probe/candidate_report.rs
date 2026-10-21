use crate::sim::combat::CombatPosition;

use super::super::summarize_state;
use super::super::turn_plan_probe_report::{
    CombatSearchV2TurnPlanProbeCandidateReport, CombatSearchV2TurnPlanProbeStepReport,
};
use super::super::turn_planner::TurnPlanV1;
use super::types::CombatSearchV2TurnPlanProbeCandidate;

pub(super) fn candidate_report(
    (index, plan): (usize, &TurnPlanV1),
) -> CombatSearchV2TurnPlanProbeCandidate {
    CombatSearchV2TurnPlanProbeCandidate {
        report: CombatSearchV2TurnPlanProbeCandidateReport {
            plan_index: index,
            bucket: plan.bucket.label(),
            stop_reason: plan.stop_reason.label(),
            outcome_class: plan.eval.outcome_class().label(),
            survival_bucket: plan.eval.survival_bucket().label(),
            progress_bucket: plan.eval.progress_bucket().label(),
            action_count: plan.actions.len(),
            first_action_key: plan.actions.first().map(|action| action.action_key.clone()),
            action_keys: plan
                .actions
                .iter()
                .map(|action| action.action_key.clone())
                .collect(),
            actions: plan.actions.clone(),
            action_facts: plan.action_facts.clone(),
            steps: turn_plan_step_reports(plan),
            eval_final_hp: plan.eval.final_hp(),
            eval_risk_margin: plan.eval.risk_margin(),
            eval_enemy_progress: plan.eval.enemy_progress(),
            end_state: summarize_state(&plan.end_node.engine, &plan.end_node.combat),
        },
        position: CombatPosition::new(plan.end_node.engine.clone(), plan.end_node.combat.clone()),
    }
}

fn turn_plan_step_reports(plan: &TurnPlanV1) -> Vec<CombatSearchV2TurnPlanProbeStepReport> {
    plan.actions
        .iter()
        .zip(plan.action_facts.iter())
        .zip(plan.step_states.iter())
        .enumerate()
        .map(|(step_index, ((action, action_facts), state))| {
            CombatSearchV2TurnPlanProbeStepReport {
                step_index,
                action: action.clone(),
                action_facts: action_facts.clone(),
                exact_state_hash_kind: "combat_exact_state_hash_v2",
                state_before_exact_state_hash: state.before_exact_state_hash.clone(),
                state_after_exact_state_hash: state.after_exact_state_hash.clone(),
                state_before: state.before.clone(),
                state_after: state.after.clone(),
                engine_steps: state.engine_steps,
            }
        })
        .collect()
}
