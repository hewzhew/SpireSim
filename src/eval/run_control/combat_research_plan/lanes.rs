use super::*;
use std::cell::RefCell;
use std::time::Instant;
use sts_combat_planner::{
    combat_plan_state_guide_policy_v1, CombatDecisionRoot, CombatPlanningQuantum,
    LocalTurnGraphPlanTransitionPathSnapshot, TurnOptionGeneratorConfig,
    TurnOptionGeneratorSession,
};

pub(super) struct ResearchLaneResultV1 {
    pub(super) attempt: TaskResearchPlanLaneAttemptV1,
    pub(super) candidates: Vec<TaskResearchPlanCandidateV1>,
    pub(super) witness_input_lines: Vec<Vec<ClientInput>>,
    pub(super) representative_resources: Option<CombatWitnessMaterializedResourcesV1>,
}

impl ResearchLaneResultV1 {
    pub(super) fn has_witness(&self) -> bool {
        self.representative_resources.is_some()
    }

    pub(super) fn has_requested_quality_witness(&self) -> bool {
        self.candidates
            .iter()
            .any(|candidate| candidate.satisfies_requested_quality)
    }

    pub(super) fn minimum_potion_expenditures(&self) -> Option<u32> {
        self.candidates
            .iter()
            .map(|candidate| candidate.outcome.total_potions_consumed)
            .min()
    }

    pub(super) fn has_requested_quality_potion_free_witness(&self) -> bool {
        self.candidates.iter().any(|candidate| {
            candidate.satisfies_requested_quality && candidate.outcome.total_potions_consumed == 0
        })
    }
}

pub(super) struct ResidentResearchLaneResultV1 {
    pub(super) lane: ResearchLaneResultV1,
    pub(super) has_refinement_ending_witness: bool,
    pub(super) milestone_candidates: Vec<CombatMilestoneResearchMaterializationV1>,
    pub(super) milestone_closure_paths: Vec<LocalTurnGraphMilestoneClosurePathSnapshot>,
    pub(super) bounded_progress: Option<CombatResearchBoundedProgressMaterializationV1>,
}

pub(super) struct MilestoneClosureResearchLaneResultV1 {
    pub(super) attempt: TaskResearchPlanLaneAttemptV1,
    pub(super) candidates: Vec<CombatMilestoneResearchMaterializationV1>,
}

pub(super) fn run_resident_research_lane_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    position: &CombatPosition,
    deadline: Option<Instant>,
) -> Result<ResidentResearchLaneResultV1, String> {
    let mut job = OracleResidentCombatWitnessJobV1::new_with_plan_transition_paths(
        env.session(),
        OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(options.max_generation_work),
            max_engine_steps_per_transition: Some(options.max_engine_steps_per_transition),
            wall_ms: Some(options.wall_ms),
            satisfaction: Some(options.satisfaction),
            potion_policy: Some(options.potion_policy),
            max_potions_used: options.max_potions_used,
            allow_potion_discard: Some(false),
            ..OracleCombatWitnessOptionsV1::default()
        },
        None,
    )?;
    if !job.exact_root_matches(position) {
        return Err(
            "resident combat research job root diverged from its materialization root".to_string(),
        );
    }
    let quantum = OracleCombatWitnessQuantumV1 {
        label: "exact-combat-research-plan",
        additional_generation_work: options.quantum_generation_work,
        soft_wall_ms: Some(options.quantum_wall_ms),
    };
    let last_advance = loop {
        let advance = match options.satisfaction {
            CombatWitnessSatisfactionV1::FirstCompleteWin => job.advance(&quantum, deadline),
            _ => job.advance_improving_incumbent(&quantum, deadline),
        };
        if !matches!(advance, OracleCombatWitnessAdvanceV1::Pending)
            || (matches!(
                options.satisfaction,
                CombatWitnessSatisfactionV1::FirstCompleteWin
            ) && job.has_verified_witness())
        {
            break advance;
        }
    };
    let evidence = job.evidence();
    let generation_work = evidence.generation_work;
    let provenance = research_plan_provenance_v1(RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1);
    job.validate_exact_victory_frontier()?;
    let milestone_candidates = resident_milestone_candidates_v1(
        env,
        position,
        &job.plan_transition_path_snapshots(),
        options.max_engine_steps_per_transition,
    )?;
    let milestone_closure_paths = job.milestone_closure_path_snapshots();
    let representative_inputs = job.verified_witness_inputs();
    let frontier_inputs = job.verified_witness_frontier_inputs();
    let representative_resources = representative_inputs
        .as_ref()
        .map(|inputs| {
            materialize_and_minimize_redundant_potion_actions_v1(
                position,
                inputs,
                options.max_engine_steps_per_transition,
            )
            .map(|candidate| candidate.resources)
            .map_err(|error| {
                let retained = job.checkpoint().incumbent.map(|witness| {
                    format!(
                        "source={:?} terminal={:?} combat_smoked={} engine_steps={:?}",
                        witness.discovery_source,
                        EngineCombatStepper.terminal(&witness.final_position),
                        witness.final_position.combat.runtime.combat_smoked,
                        witness
                            .actions
                            .iter()
                            .map(|action| action.engine_steps)
                            .collect::<Vec<_>>(),
                    )
                });
                format!(
                    "resident representative materialization failed: {error}; retained={retained:?}"
                )
            })
        })
        .transpose()?;
    let bounded_progress = representative_inputs
        .is_none()
        .then(|| resident_bounded_progress_v1(env, &provenance, &evidence))
        .transpose()?;
    let exit = match representative_inputs {
        Some(_) => TaskResearchPlanAttemptExitV1::Witness,
        None => match last_advance {
            OracleCombatWitnessAdvanceV1::AllowanceExhausted => {
                if job.remaining_generation_work() == 0 {
                    TaskResearchPlanAttemptExitV1::GenerationWorkExhausted
                } else if job.remaining_wall_ms() == Some(0) {
                    TaskResearchPlanAttemptExitV1::WallTimeExhausted
                } else {
                    TaskResearchPlanAttemptExitV1::AllowanceExhausted
                }
            }
            OracleCombatWitnessAdvanceV1::GlobalDeadlineReached => {
                TaskResearchPlanAttemptExitV1::DeadlineReached
            }
            OracleCombatWitnessAdvanceV1::ReadyToFinish => {
                TaskResearchPlanAttemptExitV1::PortfolioCompleteWithoutWitness
            }
            OracleCombatWitnessAdvanceV1::Pending => {
                TaskResearchPlanAttemptExitV1::AllowanceExhausted
            }
        },
    };
    let attempt = TaskResearchPlanLaneAttemptV1 {
        provenance: provenance.clone(),
        work_unit: TaskResearchPlanWorkUnitV1::TurnGraphGenerationWork,
        work_consumed: generation_work,
        exit,
    };
    let frontier_candidates = frontier_inputs
        .iter()
        .enumerate()
        .map(|(index, inputs)| {
            materialize_and_minimize_redundant_potion_actions_v1(
                position,
                inputs,
                options.max_engine_steps_per_transition,
            )
            .map_err(|error| {
                format!(
                    "resident frontier candidate {index}/{} materialization failed after resident replay validation: {error}",
                    frontier_inputs.len(),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let candidates = frontier_inputs
        .iter()
        .zip(&frontier_candidates)
        .map(|(_, candidate)| {
            materialize_public_research_candidate_v1(
                env,
                position,
                &candidate.inputs,
                provenance.clone(),
                &candidate.resources,
                options.satisfaction,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let witness_input_lines = frontier_candidates
        .iter()
        .map(|candidate| candidate.inputs.clone())
        .collect();
    Ok(ResidentResearchLaneResultV1 {
        lane: ResearchLaneResultV1 {
            attempt,
            candidates,
            witness_input_lines,
            representative_resources,
        },
        has_refinement_ending_witness: job.has_refinement_ending_witness(),
        milestone_candidates,
        milestone_closure_paths,
        bounded_progress,
    })
}

fn resident_bounded_progress_v1(
    env: &LearningEnvV3,
    provenance: &TaskPrivilegedSupportProvenanceV1,
    evidence: &OracleResidentCombatWitnessEvidenceV1,
) -> Result<CombatResearchBoundedProgressMaterializationV1, String> {
    let mut paths = Vec::<CombatResearchBoundedProgressPathMaterializationV1>::new();
    for (role, state, actions) in [
        (
            CombatResearchBoundedProgressRoleV1::DeepestSurvival,
            evidence.deepest_survival_state.as_ref(),
            evidence.deepest_survival_actions.as_slice(),
        ),
        (
            CombatResearchBoundedProgressRoleV1::DeepestEnemyProgress,
            evidence.deepest_progress_state.as_ref(),
            evidence.deepest_progress_actions.as_slice(),
        ),
    ] {
        let Some(state) = state else {
            continue;
        };
        if actions.is_empty() {
            continue;
        }
        let inputs = actions
            .iter()
            .map(|action| action.input.clone())
            .collect::<Vec<_>>();
        let (plan, source_decisions, successor_decision) =
            exact_inputs_to_public_turn_plan_materialization_v1(env, &inputs, provenance.clone())?;
        let endpoint = CombatResearchBoundedProgressStateV1::from(state);
        if let Some(existing) = paths
            .iter_mut()
            .find(|existing| existing.candidate.plan == plan)
        {
            if existing.candidate.endpoint != endpoint
                || existing.exact_inputs != inputs
                || existing.source_decisions != source_decisions
                || existing.successor_decision != successor_decision
            {
                return Err(
                    "one resident progress prefix produced conflicting endpoint summaries"
                        .to_string(),
                );
            }
            if !existing.candidate.roles.contains(&role) {
                existing.candidate.roles.push(role);
                existing.candidate.roles.sort();
            }
        } else {
            paths.push(CombatResearchBoundedProgressPathMaterializationV1 {
                candidate: CombatResearchBoundedProgressPathV1 {
                    roles: vec![role],
                    plan,
                    endpoint,
                },
                exact_inputs: inputs,
                source_decisions,
                successor_decision,
            });
        }
    }
    Ok(CombatResearchBoundedProgressMaterializationV1 {
        provenance: provenance.clone(),
        max_player_turn: evidence.max_player_turn,
        recent_turn_survival_envelope: evidence
            .recent_turn_survival_envelope
            .iter()
            .map(Into::into)
            .collect(),
        paths,
    })
}

pub(super) fn run_milestone_closure_lane_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    closure_options: &CombatMilestoneClosureResearchOptionsV1,
    combat_root: &CombatPosition,
    closure_paths: &[LocalTurnGraphMilestoneClosurePathSnapshot],
    shared_deadline: Option<Instant>,
) -> Result<MilestoneClosureResearchLaneResultV1, String> {
    let mut parents = closure_paths
        .iter()
        .filter(|path| path.generation_work == 0)
        .collect::<Vec<_>>();
    parents.sort_by(|left, right| {
        right
            .plan
            .envelope
            .visible_damage_margin
            .cmp(&left.plan.envelope.visible_damage_margin)
            .then_with(|| {
                right
                    .plan
                    .envelope
                    .player_hp
                    .cmp(&left.plan.envelope.player_hp)
            })
            .then_with(|| {
                right
                    .plan
                    .resources
                    .remaining_hand_block_sources
                    .cmp(&left.plan.resources.remaining_hand_block_sources)
            })
            .then_with(|| {
                right
                    .plan
                    .resources
                    .remaining_accessible_block_sources
                    .cmp(&left.plan.resources.remaining_accessible_block_sources)
            })
            .then_with(|| left.root_actions.len().cmp(&right.root_actions.len()))
            .then_with(|| {
                left.path_negative_log_policy
                    .total_cmp(&right.path_negative_log_policy)
            })
            .then_with(|| left.exact_state_hash.cmp(&right.exact_state_hash))
    });
    parents.truncate(closure_options.max_parent_states);

    let deadline = lane_deadline(closure_options.wall_ms, shared_deadline);
    let mut generation_work = 0usize;
    let mut seen_successors = std::collections::BTreeSet::new();
    let mut discovered = Vec::<(String, CombatMilestoneResearchMaterializationV1)>::new();
    let allow_potions = options.potion_policy != CombatWitnessPotionPolicyV1::Never
        && options.max_potions_used != Some(0);

    for path in parents {
        if generation_work >= closure_options.max_generation_work || Instant::now() >= deadline {
            break;
        }
        let parent = replay_milestone_closure_parent_v1(
            combat_root,
            path,
            options.max_engine_steps_per_transition,
        )?;
        let root = CombatDecisionRoot::new(parent.clone())
            .map_err(|error| format!("invalid milestone closure root: {error:?}"))?;
        let policy = combat_plan_state_guide_policy_v1(retained_combat_tactical_policy_v1());
        let mut generator = TurnOptionGeneratorSession::with_policy(
            root,
            TurnOptionGeneratorConfig {
                max_engine_steps_per_transition: options.max_engine_steps_per_transition,
                allow_potion_expenditure: allow_potions,
                allow_potion_discard: false,
                ..TurnOptionGeneratorConfig::default()
            },
            policy,
        );
        let work = closure_options
            .generation_work_per_parent
            .min(
                closure_options
                    .max_generation_work
                    .saturating_sub(generation_work),
            )
            .max(1);
        generator.advance(
            &EngineCombatStepper,
            CombatPlanningQuantum {
                additional_generation_work: work,
                additional_engine_steps: work
                    .saturating_mul(options.max_engine_steps_per_transition),
                deadline: Some(deadline),
            },
        );
        generation_work = generation_work.saturating_add(generator.counters().generation_work);

        for option in generator.completed_options() {
            if EngineCombatStepper.terminal(option.exact_successor()) != CombatTerminal::Unresolved
            {
                continue;
            }
            let Some(transition) =
                combat_plan_transition_annotation_v1(&parent, option.exact_successor())
            else {
                continue;
            };
            if !transition
                .completed_milestones()
                .contains(&path.target_milestone)
            {
                continue;
            }
            if !seen_successors.insert(option.exact_successor_hash().to_owned()) {
                continue;
            }
            let mut root_actions = path.root_actions.clone();
            let parent_action_count = root_actions.len();
            root_actions.extend(option.actions().iter().cloned());
            let potion_expenditures = root_actions
                .iter()
                .filter(|action| {
                    matches!(
                        action.input,
                        ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
                    )
                })
                .count() as u32;
            if options
                .max_potions_used
                .is_some_and(|limit| potion_expenditures > limit)
            {
                continue;
            }
            let candidate_path = LocalTurnGraphPlanTransitionPathSnapshot {
                successor_exact_state_hash: option.exact_successor_hash().to_owned(),
                root_actions,
                parent_action_count,
                path_negative_log_policy: path.path_negative_log_policy
                    + option.negative_log_policy(),
                plan_transition_annotation: transition,
            };
            if let Some(candidate) = materialize_milestone_candidate_v1(
                env,
                combat_root,
                &candidate_path,
                options.max_engine_steps_per_transition,
                MILESTONE_CLOSURE_RESEARCH_PLAN_PRODUCER_V1,
            )? {
                discovered.push((candidate_path.successor_exact_state_hash, candidate));
            }
        }
    }

    discovered.sort_by(|(left_hash, left), (right_hash, right)| {
        right
            .candidate
            .final_player_hp
            .cmp(&left.candidate.final_player_hp)
            .then_with(|| {
                right
                    .candidate
                    .successor_plan
                    .envelope
                    .visible_damage_margin
                    .cmp(&left.candidate.successor_plan.envelope.visible_damage_margin)
            })
            .then_with(|| {
                left.candidate
                    .plan
                    .steps
                    .len()
                    .cmp(&right.candidate.plan.steps.len())
            })
            .then_with(|| left_hash.cmp(right_hash))
    });
    discovered.truncate(closure_options.max_candidates);
    let candidates = discovered
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect::<Vec<_>>();
    let exit = if !candidates.is_empty() {
        TaskResearchPlanAttemptExitV1::MilestoneCandidates
    } else if Instant::now() >= deadline {
        TaskResearchPlanAttemptExitV1::WallTimeExhausted
    } else if generation_work >= closure_options.max_generation_work {
        TaskResearchPlanAttemptExitV1::GenerationWorkExhausted
    } else {
        TaskResearchPlanAttemptExitV1::SearchFrontierExhaustedWithoutWitness
    };
    Ok(MilestoneClosureResearchLaneResultV1 {
        attempt: TaskResearchPlanLaneAttemptV1 {
            provenance: research_plan_provenance_v1(MILESTONE_CLOSURE_RESEARCH_PLAN_PRODUCER_V1),
            work_unit: TaskResearchPlanWorkUnitV1::MilestoneClosureGenerationWork,
            work_consumed: generation_work as u64,
            exit,
        },
        candidates,
    })
}

fn replay_milestone_closure_parent_v1(
    root: &CombatPosition,
    path: &LocalTurnGraphMilestoneClosurePathSnapshot,
    max_engine_steps_per_transition: usize,
) -> Result<CombatPosition, String> {
    let mut position = root.clone();
    for (action_index, action) in path.root_actions.iter().enumerate() {
        if EngineCombatStepper
            .choice_for_legal_input(&position, &action.input)
            .is_none()
        {
            return Err(format!(
                "milestone closure parent path contains an illegal action at index {action_index}"
            ));
        }
        let step = EngineCombatStepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out || step.terminal != CombatTerminal::Unresolved {
            return Err(format!(
                "milestone closure parent path did not reach an unresolved stable boundary at index {action_index}"
            ));
        }
        position = step.position;
    }
    let exact_state_hash =
        crate::ai::combat_state_key::combat_exact_state_hash_v2(&position.engine, &position.combat);
    if exact_state_hash != path.exact_state_hash {
        return Err("milestone closure parent replay changed exact identity".to_string());
    }
    Ok(position)
}

pub(super) fn run_complete_turn_proposal_beam_lane_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    beam_options: &CompleteTurnProposalBeamResearchOptionsV1,
    position: &CombatPosition,
    conservation_potion_budget: Option<u32>,
    shared_deadline: Option<Instant>,
) -> Result<ResearchLaneResultV1, String> {
    let plan = public_combat_plan_v1(&public_combat_state_v1(&position.combat));
    let policy =
        sts_combat_planner::combat_plan_state_guide_policy_v1(retained_combat_tactical_policy_v1());
    let max_total_potion_expenditures = conservation_potion_budget.or(options.max_potions_used);
    let materialized_frontier = RefCell::new(Vec::<ReplayCheckedCombatWitnessV1>::new());
    let first_rejected_witness_error = RefCell::new(None::<String>);
    let beam = sts_combat_planner::search_complete_turn_proposal_beam_with_win_acceptance_v1(
        position.clone(),
        plan,
        sts_combat_planner::CompleteTurnProposalBeamConfigV1 {
            proposal: sts_combat_planner::ExactCompleteTurnProposalConfigV1 {
                max_engine_steps_per_action: options.max_engine_steps_per_transition,
                allow_potion_expenditure: options.potion_policy
                    != CombatWitnessPotionPolicyV1::Never
                    && max_total_potion_expenditures != Some(0),
                max_potion_expenditures: max_total_potion_expenditures,
                allow_potion_discard: false,
                ..sts_combat_planner::ExactCompleteTurnProposalConfigV1::default()
            },
            max_total_potion_expenditures,
            beam_width: beam_options.beam_width,
            max_turn_depth: 32,
            max_expanded_boundaries: beam_options.max_expanded_boundaries,
            deadline: Some(lane_deadline(beam_options.wall_ms, shared_deadline)),
        },
        policy,
        &|win| {
            let inputs = win
                .actions
                .iter()
                .map(|action| action.input.clone())
                .collect::<Vec<_>>();
            let resources = match materialize_replay_checked_combat_outcome_v1(
                position,
                &inputs,
                &win.terminal,
            ) {
                Ok(resources) => resources,
                Err(error) => {
                    let mut first_error = first_rejected_witness_error.borrow_mut();
                    if first_error.is_none() {
                        *first_error = Some(format!(
                            "proposal-beam witness materialization failed: {error}"
                        ));
                    }
                    return false;
                }
            };
            if !materialized_potion_expenditures_within_limit_v1(
                &resources,
                max_total_potion_expenditures,
            ) {
                let mut first_error = first_rejected_witness_error.borrow_mut();
                if first_error.is_none() {
                    *first_error = Some(format!(
                        "proposal-beam witness exceeded its total potion budget: used={} limit={max_total_potion_expenditures:?}",
                        resources.total_potion_expenditures,
                    ));
                }
                return false;
            }
            let satisfies_requested_quality = resources.satisfies(position, options.satisfaction);
            retain_materialized_frontier_candidate_v1(
                &mut materialized_frontier.borrow_mut(),
                ReplayCheckedCombatWitnessV1 { inputs, resources },
            );
            satisfies_requested_quality
        },
    );
    let provenance = research_plan_provenance_v1(COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1);
    let raw_frontier = materialized_frontier.into_inner();
    let mut materialized_frontier = Vec::new();
    for candidate in raw_frontier {
        let candidate = minimize_redundant_potion_actions_v1(
            position,
            candidate.inputs,
            candidate.resources,
            options.max_engine_steps_per_transition,
        );
        retain_materialized_frontier_candidate_v1(&mut materialized_frontier, candidate);
    }
    if materialized_frontier.is_empty() {
        if let Some(error) = first_rejected_witness_error.into_inner() {
            return Err(error);
        }
    }
    let exit = if !materialized_frontier.is_empty() {
        TaskResearchPlanAttemptExitV1::Witness
    } else {
        match beam.interruption {
            Some(sts_combat_planner::CompleteTurnProposalBeamInterruptionV1::Deadline) => {
                TaskResearchPlanAttemptExitV1::WallTimeExhausted
            }
            Some(
                sts_combat_planner::CompleteTurnProposalBeamInterruptionV1::ExpandedBoundaries,
            ) => TaskResearchPlanAttemptExitV1::ExpandedNodeBudgetExhausted,
            Some(sts_combat_planner::CompleteTurnProposalBeamInterruptionV1::FrontierExhausted) => {
                TaskResearchPlanAttemptExitV1::SearchFrontierExhaustedWithoutWitness
            }
            Some(sts_combat_planner::CompleteTurnProposalBeamInterruptionV1::TurnDepth) | None => {
                TaskResearchPlanAttemptExitV1::FrontierOpenWithoutWitness
            }
        }
    };
    let attempt = TaskResearchPlanLaneAttemptV1 {
        provenance: provenance.clone(),
        work_unit: TaskResearchPlanWorkUnitV1::CompleteTurnProposalExpandedBoundaries,
        work_consumed: beam.counters.expanded_boundaries as u64,
        exit,
    };
    let representative_resources = materialized_frontier
        .iter()
        .find(|candidate| {
            candidate
                .resources
                .satisfies(position, options.satisfaction)
        })
        .or_else(|| materialized_frontier.first())
        .map(|candidate| candidate.resources.clone());
    let witness_input_lines = materialized_frontier
        .iter()
        .map(|candidate| candidate.inputs.clone())
        .collect();
    let candidates = materialized_frontier
        .into_iter()
        .map(|candidate| {
            materialize_public_research_candidate_v1(
                env,
                position,
                &candidate.inputs,
                provenance.clone(),
                &candidate.resources,
                options.satisfaction,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ResearchLaneResultV1 {
        attempt,
        candidates,
        witness_input_lines,
        representative_resources,
    })
}

struct ReplayCheckedCombatWitnessV1 {
    inputs: Vec<ClientInput>,
    resources: CombatWitnessMaterializedResourcesV1,
}

fn retain_materialized_frontier_candidate_v1(
    frontier: &mut Vec<ReplayCheckedCombatWitnessV1>,
    candidate: ReplayCheckedCombatWitnessV1,
) {
    if frontier.iter().any(|existing| {
        existing
            .resources
            .has_same_materialized_resources_v1(&candidate.resources)
            || existing.resources.strictly_dominates(&candidate.resources)
    }) {
        return;
    }
    frontier.retain(|existing| !candidate.resources.strictly_dominates(&existing.resources));
    frontier.push(candidate);
}

fn materialized_potion_expenditures_within_limit_v1(
    resources: &CombatWitnessMaterializedResourcesV1,
    limit: Option<u32>,
) -> bool {
    limit.is_none_or(|limit| resources.total_potion_expenditures <= limit)
}

fn materialize_and_minimize_redundant_potion_actions_v1(
    root: &CombatPosition,
    inputs: &[ClientInput],
    max_engine_steps_per_transition: usize,
) -> Result<ReplayCheckedCombatWitnessV1, String> {
    let resources =
        combat_witness_materialized_resources_v1(root, inputs, max_engine_steps_per_transition)?;
    Ok(minimize_redundant_potion_actions_v1(
        root,
        inputs.to_vec(),
        resources,
        max_engine_steps_per_transition,
    ))
}

fn minimize_redundant_potion_actions_v1(
    root: &CombatPosition,
    mut inputs: Vec<ClientInput>,
    mut resources: CombatWitnessMaterializedResourcesV1,
    max_engine_steps_per_transition: usize,
) -> ReplayCheckedCombatWitnessV1 {
    loop {
        let mut removed = false;
        for index in (0..inputs.len()).rev() {
            if !matches!(
                inputs[index],
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            ) {
                continue;
            }
            let mut trial_inputs = inputs.clone();
            trial_inputs.remove(index);
            let Ok(trial_resources) = combat_witness_materialized_resources_v1(
                root,
                &trial_inputs,
                max_engine_steps_per_transition,
            ) else {
                continue;
            };
            if trial_resources.strictly_dominates(&resources) {
                inputs = trial_inputs;
                resources = trial_resources;
                removed = true;
                break;
            }
        }
        if !removed {
            break;
        }
    }
    ReplayCheckedCombatWitnessV1 { inputs, resources }
}

pub(super) fn run_atomic_research_lane_v1(
    env: &LearningEnvV3,
    options: &CombatResearchPlanOptionsV1,
    challenger: &AtomicCombatResearchPlanChallengerOptionsV1,
    position: &CombatPosition,
    conservation_potion_budget: Option<u32>,
    shared_deadline: Option<Instant>,
) -> Result<ResearchLaneResultV1, String> {
    // A missing resident witness is budget evidence, not permission to turn an
    // anytime quality search into a first-win search.  The atomic engine keeps
    // every observed non-dominated win, so preserving the requested
    // satisfaction still returns a rescue witness at the budget boundary while
    // allowing later lower-expenditure or higher-HP wins to replace it.
    let config = CombatSearchV2Config {
        max_nodes: challenger.max_nodes,
        max_engine_steps_per_action: options.max_engine_steps_per_transition,
        wall_time: Some(
            lane_deadline(challenger.wall_ms, shared_deadline)
                .saturating_duration_since(Instant::now()),
        ),
        satisfaction: options.satisfaction,
        potion_policy: if conservation_potion_budget == Some(0) {
            CombatWitnessPotionPolicyV1::Never
        } else {
            options.potion_policy
        },
        max_potions_used: conservation_potion_budget.or(options.max_potions_used),
        allow_potion_discard: Some(false),
        turn_plan_policy: CombatSearchV2TurnPlanPolicy::TurnBoundaryFrontierSeed,
        input_label: Some(ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1.to_string()),
        ..Default::default()
    };
    let report = run_combat_search_v2(&position.engine, &position.combat, config);
    let work_consumed = report.stats.nodes_expanded;
    let coverage_status = report.outcome.coverage_status;
    let provenance = research_plan_provenance_v1(ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1);
    let frontier = materialize_atomic_frontier_v1(
        position,
        options.max_engine_steps_per_transition,
        report.best_win_trajectory,
        report.win_candidate_trajectories,
    )?;
    // A representative supports lane scheduling only. Every observed
    // non-dominated choice reaches the analyst, including incomparable reserves.
    let representative_resources = frontier
        .first()
        .map(|candidate| candidate.resources.clone());
    let witness_input_lines = frontier
        .iter()
        .map(|candidate| candidate.inputs.clone())
        .collect();
    let candidates = frontier
        .into_iter()
        .map(|candidate| {
            materialize_public_research_candidate_v1(
                env,
                position,
                &candidate.inputs,
                provenance.clone(),
                &candidate.resources,
                options.satisfaction,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let attempt = TaskResearchPlanLaneAttemptV1 {
        provenance,
        work_unit: TaskResearchPlanWorkUnitV1::AtomicExpandedNodes,
        work_consumed,
        exit: if representative_resources.is_some() {
            TaskResearchPlanAttemptExitV1::Witness
        } else {
            atomic_attempt_exit_v1(coverage_status)
        },
    };
    Ok(ResearchLaneResultV1 {
        attempt,
        candidates,
        witness_input_lines,
        representative_resources,
    })
}

pub(super) fn materialize_public_research_candidate_v1(
    env: &LearningEnvV3,
    root: &CombatPosition,
    inputs: &[ClientInput],
    provenance: TaskPrivilegedSupportProvenanceV1,
    resources: &CombatWitnessMaterializedResourcesV1,
    satisfaction: CombatWitnessSatisfactionV1,
) -> Result<TaskResearchPlanCandidateV1, String> {
    materialize_public_research_candidate_and_replay_v1(
        env,
        root,
        inputs,
        provenance,
        resources,
        satisfaction,
    )
    .map(|(candidate, _)| candidate)
}

pub(super) fn materialize_public_research_candidate_and_replay_v1(
    env: &LearningEnvV3,
    root: &CombatPosition,
    inputs: &[ClientInput],
    provenance: TaskPrivilegedSupportProvenanceV1,
    resources: &CombatWitnessMaterializedResourcesV1,
    satisfaction: CombatWitnessSatisfactionV1,
) -> Result<
    (
        TaskResearchPlanCandidateV1,
        CombatResearchReplayMaterializationV1,
    ),
    String,
> {
    let (plan, successor) = exact_inputs_to_public_replay_plan_v1(env, inputs, provenance.clone())?;
    let outcome = public_research_outcome_at_successor_v1(root, resources, &successor)?;
    Ok((
        TaskResearchPlanCandidateV1 {
            plan,
            outcome,
            satisfies_requested_quality: resources.satisfies(root, satisfaction),
            supporting_provenances: vec![provenance],
        },
        successor.replay,
    ))
}

fn materialize_atomic_frontier_v1(
    root: &CombatPosition,
    max_engine_steps_per_transition: usize,
    best_win: Option<crate::ai::combat_search_v2::CombatSearchV2TrajectoryReport>,
    win_frontier: Vec<crate::ai::combat_search_v2::CombatSearchV2TrajectoryReport>,
) -> Result<Vec<ReplayCheckedCombatWitnessV1>, String> {
    let mut trajectories = best_win.into_iter().collect::<Vec<_>>();
    for candidate in win_frontier {
        let duplicate = trajectories.iter().any(|existing| {
            existing.actions.len() == candidate.actions.len()
                && existing
                    .actions
                    .iter()
                    .zip(&candidate.actions)
                    .all(|(left, right)| left.input == right.input)
        });
        if !duplicate {
            trajectories.push(candidate);
        }
    }
    let mut frontier = Vec::new();
    for trajectory in trajectories {
        let inputs = trajectory
            .actions
            .into_iter()
            .map(|action| action.input)
            .collect::<Vec<_>>();
        let minimized = materialize_and_minimize_redundant_potion_actions_v1(
            root,
            &inputs,
            max_engine_steps_per_transition,
        )?;
        retain_materialized_frontier_candidate_v1(&mut frontier, minimized);
    }
    Ok(frontier)
}

fn atomic_attempt_exit_v1(status: SearchCoverageStatus) -> TaskResearchPlanAttemptExitV1 {
    match status {
        SearchCoverageStatus::Exhaustive => {
            TaskResearchPlanAttemptExitV1::SearchFrontierExhaustedWithoutWitness
        }
        SearchCoverageStatus::AcceptedCompleteCandidate => {
            TaskResearchPlanAttemptExitV1::FrontierOpenWithoutWitness
        }
        SearchCoverageStatus::NodeBudgetLimited => {
            TaskResearchPlanAttemptExitV1::ExpandedNodeBudgetExhausted
        }
        SearchCoverageStatus::ActionPrefixBudgetLimited => {
            TaskResearchPlanAttemptExitV1::ActionPrefixBudgetExhausted
        }
        SearchCoverageStatus::ActionSurfaceIncomplete => {
            TaskResearchPlanAttemptExitV1::ActionSurfaceIncomplete
        }
        SearchCoverageStatus::TimeBudgetLimited => TaskResearchPlanAttemptExitV1::WallTimeExhausted,
        SearchCoverageStatus::FrontierOpen => {
            TaskResearchPlanAttemptExitV1::FrontierOpenWithoutWitness
        }
    }
}
