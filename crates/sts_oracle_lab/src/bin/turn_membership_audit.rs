use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Args;
use sts_combat_planner::{
    combat_plan_state_guide_policy_v1, CombatDecisionRoot, CombatPlanningQuantum,
    TurnOptionGenerationStatus, TurnOptionGeneratorConfig, TurnOptionGeneratorSession,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::bounded_continuation_audit::{
    probe_bounded_continuations_v1, BoundedContinuationProbeOptions,
};
use super::combat_policy_controls::anchor_only_policy;
use super::combat_trace_view::target_atomic_policy_trace;
use super::exact_turn_corridor::{
    load as load_exact_turn_corridor, load_action_segments as load_combat_action_segments,
};
use super::print_json;
use super::turn_audits::single_potion_slot_mask;

#[derive(Debug, Args)]
pub(super) struct TurnMembershipArgs {
    #[arg(long)]
    case: PathBuf,
    #[arg(
        long,
        required_unless_present_any = ["corridor_actions", "trace_actions"],
        conflicts_with_all = ["corridor_actions", "trace_actions"]
    )]
    actions: Option<PathBuf>,
    /// One or more consecutive exact action segments forming a complete
    /// verified witness. Repeat the flag instead of hand-splicing JSON.
    #[arg(
        long,
        required_unless_present_any = ["actions", "trace_actions"],
        conflicts_with_all = ["actions", "trace_actions"],
        requires = "corridor_rank"
    )]
    corridor_actions: Vec<PathBuf>,
    /// Zero-based player-turn boundary in --corridor-actions. The last
    /// boundary checks the terminal winning segment.
    #[arg(long, requires = "corridor_actions")]
    corridor_rank: Option<usize>,
    /// One or more action files forming a legal finite trace. Unlike a
    /// corridor, the trace may stop at an unresolved combat boundary.
    #[arg(
        long,
        required_unless_present_any = ["actions", "corridor_actions"],
        conflicts_with_all = ["actions", "corridor_actions"],
        requires = "trace_rank"
    )]
    trace_actions: Vec<PathBuf>,
    /// Zero-based player-turn segment in --trace-actions.
    #[arg(long, requires = "trace_actions")]
    trace_rank: Option<usize>,
    #[arg(long, default_value_t = 100_000)]
    max_work: usize,
    #[arg(long, default_value_t = 5_000)]
    wall_ms: u64,
    #[arg(long, default_value_t = 8)]
    quantum_work: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    /// Lab-only control: keep action weights but disable all state guides.
    #[arg(long, conflicts_with = "typed_plan_guide")]
    anchor_only: bool,
    /// Add the resident actor's typed combat-plan state guide while retaining
    /// the same legal actions and exact simulator.
    #[arg(long, conflicts_with = "anchor_only")]
    typed_plan_guide: bool,
    /// Restrict the generator to one exact zero-based potion identity slot and
    /// omit explicit discard, matching a production single-potion stage.
    #[arg(long)]
    potion_slot: Option<usize>,
    /// After the parent membership surface, grant every observed unique exact
    /// successor this much independent next-turn work. Diagnostic only.
    #[arg(long, requires = "corridor_rank")]
    continuation_work: Option<usize>,
    /// Bound continuation work to policy, independent parent-guide, and
    /// semantic live/exhaust setup representatives per first-action family.
    #[arg(long, requires = "continuation_work")]
    representative_continuations: bool,
    /// Include every target-prefix queue snapshot. By default the report
    /// stays compact and includes only the last reached and first missing
    /// prefixes.
    #[arg(long)]
    full: bool,
}

pub(super) fn run(args: TurnMembershipArgs) -> Result<(), String> {
    let TurnMembershipArgs {
        case,
        actions,
        corridor_actions,
        corridor_rank,
        trace_actions,
        trace_rank,
        max_work,
        wall_ms,
        quantum_work,
        max_engine_steps_per_transition,
        anchor_only,
        typed_plan_guide,
        potion_slot,
        continuation_work,
        representative_continuations,
        full,
    } = args;
    if continuation_work == Some(0) {
        return Err("--continuation-work must be positive".to_string());
    }
    let (
        root_position,
        target,
        selected_corridor_rank,
        selected_trace_rank,
        expected_next_corridor_state_hash,
    ) = match (
        actions.as_ref(),
        corridor_actions.as_slice(),
        corridor_rank,
        trace_actions.as_slice(),
        trace_rank,
    ) {
        (Some(actions), [], None, [], None) => {
            let case = load_combat_case(&case)?;
            let target = serde_json::from_slice::<Vec<ClientInput>>(
                &std::fs::read(actions).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("invalid target action list: {error}"))?;
            (case.core.position, target, None, None, None)
        }
        (None, corridor_actions, Some(rank), [], None) if !corridor_actions.is_empty() => {
            let corridor =
                load_exact_turn_corridor(&case, corridor_actions, max_engine_steps_per_transition)?;
            let root_position = corridor
                .positions_by_rank
                .get(rank)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "corridor rank {rank} is out of range 0..{}",
                        corridor.positions_by_rank.len()
                    )
                })?;
            let target = corridor
                .transition_actions
                .get(rank)
                .cloned()
                .expect("verified corridor has one transition per boundary");
            let expected_next_corridor_state_hash = corridor
                .positions_by_rank
                .get(rank.saturating_add(2))
                .map(|position| combat_exact_state_hash_v2(&position.engine, &position.combat));
            (
                root_position,
                target,
                Some(rank),
                None,
                expected_next_corridor_state_hash,
            )
        }
        (None, [], None, trace_actions, Some(rank)) if !trace_actions.is_empty() => {
            let case = load_combat_case(&case)?;
            let actions = load_combat_action_segments(trace_actions)?;
            let (root_position, target) = finite_trace_turn_segment_v1(
                case.core.position,
                &actions,
                rank,
                max_engine_steps_per_transition,
            )?;
            (root_position, target, None, Some(rank), None)
        }
        _ => unreachable!("clap selects either actions or corridor rank"),
    };
    let (target_policy_trace, target_successor_exact_state_hash, target_prefix_positions) =
        target_atomic_policy_trace(&root_position, &target, max_engine_steps_per_transition)?;
    let root = CombatDecisionRoot::new(root_position.clone())
        .map_err(|error| format!("invalid combat case root: {error:?}"))?;
    let policy = retained_combat_tactical_policy_v1();
    let policy = if typed_plan_guide {
        combat_plan_state_guide_policy_v1(policy)
    } else {
        policy
    };
    let policy = if anchor_only {
        anchor_only_policy(policy)
    } else {
        policy
    };
    let allowed_potion_slots = potion_slot.map(single_potion_slot_mask).transpose()?;
    let mut generator_config = TurnOptionGeneratorConfig {
        max_engine_steps_per_transition,
        ..TurnOptionGeneratorConfig::default()
    };
    if let Some(mask) = allowed_potion_slots {
        generator_config.allow_potion_expenditure = true;
        generator_config.allow_potion_discard = false;
        generator_config.allowed_potion_slots = Some(mask);
    }
    let mut generator =
        TurnOptionGeneratorSession::with_policy(root, generator_config, policy.clone());
    let started = Instant::now();
    let deadline = started + Duration::from_millis(wall_ms);
    let mut scanned_options = 0usize;
    let mut matched = None;
    let mut prefix_insertions = vec![None; target_prefix_positions.len()];
    let mut transition_insertions = vec![None; target_prefix_positions.len()];
    let mut last_status = TurnOptionGenerationStatus::Partial(
        sts_combat_planner::GenerationInterruption::GenerationWorkBudget,
    );
    while generator.counters().generation_work < max_work
        && !generator.is_finished()
        && Instant::now() < deadline
    {
        let remaining = max_work.saturating_sub(generator.counters().generation_work);
        let work = quantum_work.max(1).min(remaining);
        let report = generator.advance(
            &EngineCombatStepper,
            CombatPlanningQuantum {
                additional_generation_work: work,
                additional_engine_steps: work.saturating_mul(max_engine_steps_per_transition),
                deadline: Some(deadline),
            },
        );
        last_status = report.status;
        for (index, position) in target_prefix_positions.iter().enumerate() {
            if prefix_insertions[index].is_none() && generator.has_seen_exact_position(position) {
                let anchor_rank = generator
                    .live_expand_queue_ranks_at_exact_position(position)
                    .map(|(anchor, _)| anchor);
                prefix_insertions[index] = Some((
                    report.after.generation_work,
                    generator.anchor_work_pops(),
                    anchor_rank,
                ));
            }
            if transition_insertions[index].is_none() {
                transition_insertions[index] = target
                    .get(index + 1)
                    .and_then(|next| generator.live_action_transition_snapshot(position, next))
                    .map(|snapshot| {
                        serde_json::json!({
                            "generation_work": report.after.generation_work,
                            "candidate_ordinal": snapshot.candidate_ordinal,
                            "remaining_candidate_count": snapshot.remaining_candidate_count,
                            "conditional_probability": snapshot.conditional_probability,
                            "candidate_negative_log_policy": snapshot.candidate_negative_log_policy,
                            "cursor_negative_log_policy": snapshot.cursor_negative_log_policy,
                            "anchor_queue_rank": snapshot.anchor_queue_rank,
                            "guide_queue_ranks": snapshot.guide_queue_ranks,
                        })
                    });
            }
        }
        for option in &generator.completed_options()[scanned_options..] {
            let exact_action_match = option.actions().len() == target.len()
                && option
                    .actions()
                    .iter()
                    .zip(&target)
                    .all(|(actual, expected)| actual.input == *expected);
            let equivalent_successor_match =
                option.exact_successor_hash() == target_successor_exact_state_hash;
            if exact_action_match || equivalent_successor_match {
                matched = Some(serde_json::json!({
                    "match_kind": if exact_action_match { "exact_actions" } else { "equivalent_exact_successor" },
                    "exact_action_match": exact_action_match,
                    "equivalent_successor_match": equivalent_successor_match,
                    "generation_work": report.after.generation_work,
                    "engine_steps": report.after.engine_steps,
                    "elapsed_ms": started.elapsed().as_millis(),
                    "boundary": format!("{:?}", option.boundary()),
                    "successor_exact_state_hash": option.exact_successor_hash(),
                    "negative_log_policy": option.negative_log_policy(),
                }));
                break;
            }
        }
        scanned_options = generator.completed_options().len();
        if matched.is_some() && continuation_work.is_none() {
            break;
        }
    }
    let counters = generator.counters();
    let continuation_probe = continuation_work
        .map(|continuation_work_per_candidate| {
            probe_bounded_continuations_v1(
                &root_position,
                generator.completed_options(),
                policy.clone(),
                generator_config,
                BoundedContinuationProbeOptions {
                    parent_status: last_status,
                    parent_generation_work: counters.generation_work,
                    parent_gap_count: generator.gaps().len(),
                    continuation_work_per_candidate,
                    max_engine_steps_per_transition,
                    target_actions: &target,
                    target_successor_exact_state_hash: &target_successor_exact_state_hash,
                    expected_next_corridor_state_hash: expected_next_corridor_state_hash.as_deref(),
                    include_all_candidates: full,
                    representative_cohort: representative_continuations,
                },
            )
        })
        .transpose()?;
    let target_prefix_membership = target_prefix_positions
        .iter()
        .enumerate()
        .map(|(index, position)| {
            let insertion =
                prefix_insertions[index].map(|(generation_work, anchor_pops, anchor_rank)| {
                    serde_json::json!({
                        "generation_work": generation_work,
                        "anchor_pops": anchor_pops,
                        "anchor_rank": anchor_rank,
                        "anchor_pops_since": generator
                            .anchor_work_pops()
                            .saturating_sub(anchor_pops),
                    })
                });
            let (live_expand, live_apply_action, live_structured_selection) =
                generator.live_work_counts_at_exact_position(position);
            let queue_ranks = generator
                .live_expand_queue_ranks_at_exact_position(position)
                .map(|(anchor, guides)| {
                    serde_json::json!({
                        "anchor": anchor,
                        "guides": guides,
                    })
                });
            let next_target_transition = target
                .get(index + 1)
                .and_then(|next| generator.live_action_transition_snapshot(position, next));
            serde_json::json!({
                "through_action": index + 1,
                "exact_state_hash": sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
                    &position.engine,
                    &position.combat,
                ),
                "seen": generator.has_seen_exact_position(position),
                "first_observed": insertion,
                "live_work": {
                    "expand": live_expand,
                    "apply_action": live_apply_action,
                    "structured_selection": live_structured_selection,
                },
                "live_expand_queue_ranks": queue_ranks,
                "next_target_transition_live": next_target_transition.is_some(),
                "next_target_transition_first_observed": transition_insertions[index],
                "next_target_transition": next_target_transition.map(|snapshot| serde_json::json!({
                    "candidate_ordinal": snapshot.candidate_ordinal,
                    "remaining_candidate_count": snapshot.remaining_candidate_count,
                    "conditional_probability": snapshot.conditional_probability,
                    "candidate_negative_log_policy": snapshot.candidate_negative_log_policy,
                    "cursor_negative_log_policy": snapshot.cursor_negative_log_policy,
                    "anchor_queue_rank": snapshot.anchor_queue_rank,
                    "guide_queue_ranks": snapshot.guide_queue_ranks,
                })),
            })
        })
        .collect::<Vec<_>>();
    let last_reached_prefix = target_prefix_membership
        .iter()
        .rev()
        .find(|prefix| prefix.get("seen").and_then(serde_json::Value::as_bool) == Some(true))
        .cloned();
    let first_missing_prefix = target_prefix_membership
        .iter()
        .find(|prefix| prefix.get("seen").and_then(serde_json::Value::as_bool) == Some(false))
        .cloned();
    let mut output = serde_json::json!({
        "schema_name": "OracleTurnMembershipProbeV1",
        "schema_version": 1,
        "scheduler": if anchor_only { "anchor_only" } else { "anchor_and_guides" },
        "typed_plan_guide": typed_plan_guide,
        "potion_contract": {
            "potion_slot": potion_slot,
            "allowed_potion_slots": allowed_potion_slots,
            "allow_potion_discard": generator_config.allow_potion_discard,
        },
        "matched": matched.is_some(),
        "match": matched,
        "target_action_count": target.len(),
        "corridor_rank": selected_corridor_rank,
        "trace_rank": selected_trace_rank,
        "target_successor_exact_state_hash": target_successor_exact_state_hash,
        "target_policy_trace": target_policy_trace,
        "last_reached_prefix": last_reached_prefix,
        "first_missing_prefix": first_missing_prefix,
        "status": format!("{:?}", last_status),
        "elapsed_ms": started.elapsed().as_millis(),
        "generation_work": counters.generation_work,
        "engine_steps": counters.engine_steps,
        "scheduler_counters": {
            "atomic_state_expansions": generator.atomic_state_expansions(),
            "anchor_work_pops": generator.anchor_work_pops(),
            "guided_work_pops": generator.guided_work_pops(),
            "applied_action_transitions": generator.diagnostics().applied_action_transitions,
        },
        "completed_turn_options": generator.completed_options().len(),
        "retained_work_items": generator.retained_work_items(),
        "finished": generator.is_finished(),
        "bounded_continuation_probe": continuation_probe,
    });
    if full {
        output
            .as_object_mut()
            .expect("membership report must be an object")
            .insert(
                "target_prefix_membership".to_owned(),
                serde_json::Value::Array(target_prefix_membership),
            );
    }
    print_json(&output)
}

fn finite_trace_turn_segment_v1(
    mut position: CombatPosition,
    actions: &[ClientInput],
    requested_rank: usize,
    max_engine_steps_per_transition: usize,
) -> Result<(CombatPosition, Vec<ClientInput>), String> {
    let stepper = EngineCombatStepper;
    let mut turn_roots = vec![position.clone()];
    let mut turn_actions = Vec::<Vec<ClientInput>>::new();
    let mut current = Vec::new();
    for (action_index, input) in actions.iter().enumerate() {
        if stepper.choice_for_legal_input(&position, input).is_none() {
            return Err(format!(
                "finite trace action {action_index} is illegal at turn {}: {input:?}",
                position.combat.turn.turn_count
            ));
        }
        let previous_turn = position.combat.turn.turn_count;
        current.push(input.clone());
        let step = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "finite trace action {action_index} did not reach a stable boundary"
            ));
        }
        position = step.position;
        if step.terminal == CombatTerminal::Unresolved
            && position.combat.turn.turn_count != previous_turn
        {
            turn_actions.push(std::mem::take(&mut current));
            turn_roots.push(position.clone());
        } else if step.terminal != CombatTerminal::Unresolved {
            turn_actions.push(std::mem::take(&mut current));
            break;
        }
    }
    if !current.is_empty() {
        turn_actions.push(current);
    }
    let available = turn_actions.len();
    let root = turn_roots.get(requested_rank).cloned().ok_or_else(|| {
        format!("finite trace rank {requested_rank} is out of range 0..{available}")
    })?;
    let target = turn_actions.get(requested_rank).cloned().ok_or_else(|| {
        format!("finite trace rank {requested_rank} has no outgoing action segment")
    })?;
    Ok((root, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_runtime::content::monsters::EnemyId;
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::test_support::{blank_test_combat, planned_monster};

    #[test]
    fn finite_unresolved_trace_selects_each_outgoing_player_turn() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let actions = vec![ClientInput::EndTurn, ClientInput::EndTurn];

        let (second_turn, target) =
            finite_trace_turn_segment_v1(root, &actions, 1, 1_000).expect("second trace turn");

        assert_eq!(target, vec![ClientInput::EndTurn]);
        assert_eq!(second_turn.combat.turn.turn_count, 2);
        let error = finite_trace_turn_segment_v1(second_turn, &target, 1, 1_000)
            .expect_err("the unresolved trace has no third outgoing segment");
        assert!(error.contains("no outgoing action segment"));
    }
}
