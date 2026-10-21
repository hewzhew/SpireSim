use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use serde_json::json;
use sts_combat_strategy::{
    combat_plan_projection_v1, combat_plan_transition_annotation_v1, CombatPlanMilestoneV1,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::sim::combat::{
    combat_terminal, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::sim::combat_action::combat_action_key;

use super::combat_replay_tools::save_combat_inputs;
use super::combat_trace_view::{combat_action_label, combat_position_snapshot};
use super::exact_turn_corridor::load_action_segments as load_combat_action_segments;
use super::{oracle_lab_runtime_identity, print_json};

#[derive(Debug, Serialize)]
struct FirstCompletedMilestoneV1 {
    action_count: usize,
    completed_milestones: Vec<CombatPlanMilestoneV1>,
    exact_state_hash: Option<String>,
    player_hp: i32,
    terminal: String,
}

#[derive(Debug, Args)]
pub(super) struct CombatCasePlanAnnotationsArgs {
    #[arg(long)]
    case: PathBuf,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

pub(super) fn run_annotations(args: CombatCasePlanAnnotationsArgs) -> Result<(), String> {
    let CombatCasePlanAnnotationsArgs {
        case,
        max_engine_steps_per_transition,
    } = args;
    let case_path = case.clone();
    let loaded = load_combat_case(&case)?;
    let position = loaded.core.position;
    let stepper = EngineCombatStepper;
    let surface = stepper.legal_action_surface(&position);
    let root_plan = combat_plan_projection_v1(&position);
    let annotations = surface
        .atomic_actions
        .iter()
        .map(|input| {
            let step = stepper.apply_to_stable(
                &position,
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: max_engine_steps_per_transition,
                    deadline: None,
                },
            );
            let exact_successor_hash = (!step.truncated)
                .then(|| combat_exact_state_hash_v2(&step.position.engine, &step.position.combat));
            let transition = (!step.truncated)
                .then(|| combat_plan_transition_annotation_v1(&position, &step.position))
                .flatten();
            let successor_plan = (!step.truncated)
                .then(|| combat_plan_projection_v1(&step.position))
                .flatten();
            json!({
                "label": combat_action_label(&position, input),
                "action_key": combat_action_key(&position.combat, input),
                "input": input,
                "engine_steps": step.engine_steps,
                "truncated": step.truncated,
                "timed_out": step.timed_out,
                "terminal": format!("{:?}", step.terminal),
                "exact_successor_hash": exact_successor_hash,
                "plan_transition": transition,
                "successor_plan": successor_plan,
            })
        })
        .collect::<Vec<_>>();
    print_json(&json!({
        "schema_name": "OracleCombatCasePlanAnnotationsV2",
        "schema_version": 2,
        "case": case_path,
        "runtime": oracle_lab_runtime_identity(),
        "contract": {
            "search": false,
            "policy_mutation": false,
            "ranking": false,
            "pruning": false,
            "terminal_truth": "exact_simulator_only",
        },
        "root_exact_state_hash": combat_exact_state_hash_v2(
            &position.engine,
            &position.combat,
        ),
        "root_plan": root_plan,
        "surface": {
            "atomic_action_count": surface.atomic_actions.len(),
            "structured_family_count": surface.selection_families.len(),
            "complete": surface.selection_families.is_empty(),
            "structured_families_unannotated": !surface.selection_families.is_empty(),
        },
        "max_engine_steps_per_transition": max_engine_steps_per_transition,
        "annotations": annotations,
    }))
}

#[derive(Debug, Args)]
pub(super) struct CombatCasePlanTraceArgs {
    #[arg(long)]
    case: PathBuf,
    /// Repeat to compose several exact action segments in order.
    #[arg(long, required = true)]
    actions: Vec<PathBuf>,
    /// Replay the complete supplied trace, then save the shortest exact prefix
    /// whose transition completes any encounter-owned typed milestone.
    #[arg(long)]
    export_first_completed_milestone_actions: Option<PathBuf>,
    /// Omit the per-action trace while retaining replay validity, the first
    /// completed milestone, final exact state, and export receipt.
    #[arg(long)]
    summary_only: bool,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

pub(super) fn run_trace(args: CombatCasePlanTraceArgs) -> Result<(), String> {
    let CombatCasePlanTraceArgs {
        case,
        actions,
        export_first_completed_milestone_actions,
        summary_only,
        max_engine_steps_per_transition,
    } = args;
    if export_first_completed_milestone_actions
        .as_ref()
        .is_some_and(|path| path.exists())
    {
        return Err("first-completed-milestone action output already exists".to_string());
    }
    let case_path = case.clone();
    let action_paths = actions.clone();
    let loaded = load_combat_case(&case)?;
    let inputs = load_combat_action_segments(&actions)?;
    let input_count = inputs.len();
    let stepper = EngineCombatStepper;
    let mut position = loaded.core.position;
    let root_exact_state_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
    let root_plan = combat_plan_projection_v1(&position);
    let mut trace = Vec::new();
    let mut consumed_actions = 0_usize;
    let mut invalid_action = None;
    let mut first_completed_milestone = None;

    for (index, input) in inputs.iter().cloned().enumerate() {
        let before_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
        let before_state = combat_position_snapshot(&position);
        let label = combat_action_label(&position, &input);
        let action_key = combat_action_key(&position.combat, &input);
        if stepper.choice_for_legal_input(&position, &input).is_none() {
            invalid_action = Some(json!({
                "action_index": index,
                "label": label,
                "action_key": action_key,
                "input": input,
                "before_exact_state_hash": before_hash,
                "before_state": before_state,
            }));
            trace.push(json!({
                "action_index": index,
                "label": label,
                "action_key": action_key,
                "input": input,
                "input_legal": false,
                "before_exact_state_hash": before_hash,
                "before_state": before_state,
                "after_exact_state_hash": null,
                "after_state": null,
                "engine_steps": 0,
                "truncated": false,
                "timed_out": false,
                "terminal": "Unresolved",
                "plan_transition": null,
                "successor_plan": null,
            }));
            break;
        }
        let step = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        let transition = (!step.truncated)
            .then(|| combat_plan_transition_annotation_v1(&position, &step.position))
            .flatten();
        let successor_plan = (!step.truncated)
            .then(|| combat_plan_projection_v1(&step.position))
            .flatten();
        let after_hash = (!step.truncated)
            .then(|| combat_exact_state_hash_v2(&step.position.engine, &step.position.combat));
        let after_state = (!step.truncated).then(|| combat_position_snapshot(&step.position));
        if first_completed_milestone.is_none() {
            let completed = transition
                .as_ref()
                .map(|annotation| annotation.completed_milestones())
                .unwrap_or_default();
            if !completed.is_empty() {
                first_completed_milestone = Some(FirstCompletedMilestoneV1 {
                    action_count: index.saturating_add(1),
                    completed_milestones: completed.to_vec(),
                    exact_state_hash: after_hash.clone(),
                    player_hp: step.position.combat.entities.player.current_hp,
                    terminal: format!("{:?}", step.terminal),
                });
            }
        }
        trace.push(json!({
            "action_index": index,
            "label": label,
            "action_key": action_key,
            "input": input,
            "input_legal": true,
            "before_exact_state_hash": before_hash,
            "before_state": before_state,
            "after_exact_state_hash": after_hash,
            "after_state": after_state,
            "engine_steps": step.engine_steps,
            "truncated": step.truncated,
            "timed_out": step.timed_out,
            "terminal": format!("{:?}", step.terminal),
            "plan_transition": transition,
            "successor_plan": successor_plan,
        }));
        consumed_actions = consumed_actions.saturating_add(1);
        position = step.position;
        if step.truncated || step.terminal != CombatTerminal::Unresolved {
            break;
        }
    }

    let exported_first_completed_milestone_actions =
        if let Some(output) = export_first_completed_milestone_actions.as_ref() {
            if invalid_action.is_some() {
                return Err(
                    "cannot export a milestone prefix from a trace containing an invalid action"
                        .to_string(),
                );
            }
            let action_count = first_completed_milestone
                .as_ref()
                .map(|milestone| milestone.action_count)
                .ok_or_else(|| {
                    "the supplied exact trace completes no typed combat-plan milestone".to_string()
                })?;
            save_combat_inputs(output, inputs.iter().take(action_count).cloned())?;
            Some(output)
        } else {
            None
        };

    let final_terminal = combat_terminal(&position.engine, &position.combat);
    let trace = (!summary_only).then_some(trace);
    print_json(&json!({
        "schema_name": "OracleCombatCasePlanTraceV4",
        "schema_version": 4,
        "case": case_path,
        "actions": action_paths,
        "runtime": oracle_lab_runtime_identity(),
        "contract": {
            "search": false,
            "policy_mutation": false,
            "ranking": false,
            "pruning": false,
            "caller_supplied_actions": true,
            "exact_input_membership_required": true,
            "terminal_truth": "exact_simulator_only",
        },
        "root_exact_state_hash": root_exact_state_hash,
        "root_plan": root_plan,
        "input_action_count": input_count,
        "consumed_action_count": consumed_actions,
        "unconsumed_action_count": input_count.saturating_sub(consumed_actions),
        "valid_action_prefix": invalid_action.is_none(),
        "invalid_action": invalid_action,
        "first_completed_milestone": first_completed_milestone,
        "exported_first_completed_milestone_actions": exported_first_completed_milestone_actions,
        "final_exact_state_hash": combat_exact_state_hash_v2(
            &position.engine,
            &position.combat,
        ),
        "final_terminal": format!("{final_terminal:?}"),
        "final_player_hp": position.combat.entities.player.current_hp,
        "final_plan": combat_plan_projection_v1(&position),
        "max_engine_steps_per_transition": max_engine_steps_per_transition,
        "trace_included": !summary_only,
        "trace": trace,
    }))
}
