//! Shared exact evidence classification for bounded combat-successor audits.
//!
//! Callers may establish an immediate terminal result or a verified witness
//! directly. Non-terminal positions are searched with the same bounded exact
//! graph search. A bounded miss remains `BudgetUnknown`; only gap-free,
//! depth-complete frontier exhaustion is an exact refutation.

use serde::{Deserialize, Serialize};
use sts_combat_planner::{
    CombatDecisionRoot, LocalTurnGraphWitnessConfig, LocalTurnGraphWitnessQuantum,
    LocalTurnGraphWitnessReport, LocalTurnGraphWitnessSession, LocalTurnGraphWitnessStatus,
    OracleCombatWitnessSatisfaction, TurnOptionGeneratorConfig,
};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::{ClientInput, EngineState};

pub(crate) struct ExactCombatEvaluation {
    pub(crate) evidence: ExactCombatEvidence,
    pub(crate) witness_actions: Option<Vec<ClientInput>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ExactCombatEvidence {
    ExactWin {
        source: String,
        final_hp: i32,
        suffix_action_count: usize,
        search_cost: Option<ExactCombatSearchCost>,
    },
    ExactRefutation {
        source: String,
        search_cost: ExactCombatSearchCost,
    },
    ExactTerminalNonWin {
        boundary: String,
    },
    BudgetUnknown {
        status: String,
        search_cost: ExactCombatSearchCost,
        deepest_player_turn: u32,
        gap_count: usize,
        depth_limited_successors: usize,
    },
}

impl ExactCombatEvidence {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::ExactWin { .. } => "exact_win",
            Self::ExactRefutation { .. } => "exact_refutation",
            Self::ExactTerminalNonWin { .. } => "exact_terminal_non_win",
            Self::BudgetUnknown { .. } => "budget_unknown",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct ExactCombatSearchCost {
    generation_work: usize,
    applied_action_transitions: usize,
    engine_steps: usize,
    exact_nodes: usize,
    exact_edges: usize,
}

impl ExactCombatSearchCost {
    fn add_assign(&mut self, other: &Self) {
        self.generation_work = self.generation_work.saturating_add(other.generation_work);
        self.applied_action_transitions = self
            .applied_action_transitions
            .saturating_add(other.applied_action_transitions);
        self.engine_steps = self.engine_steps.saturating_add(other.engine_steps);
        self.exact_nodes = self.exact_nodes.saturating_add(other.exact_nodes);
        self.exact_edges = self.exact_edges.saturating_add(other.exact_edges);
    }
}

pub(crate) fn known_exact_win(
    source: &'static str,
    final_hp: i32,
    suffix_action_count: usize,
) -> ExactCombatEvidence {
    ExactCombatEvidence::ExactWin {
        source: source.to_string(),
        final_hp,
        suffix_action_count,
        search_cost: None,
    }
}

pub(crate) fn exact_terminal_non_win(boundary: impl Into<String>) -> ExactCombatEvidence {
    ExactCombatEvidence::ExactTerminalNonWin {
        boundary: boundary.into(),
    }
}

/// Preserve a replay-verified win after still charging the ordinary equal-work
/// search used for every first-action successor. A better searched win remains
/// authoritative; otherwise the verified line is the minimum known quality.
pub(crate) fn retain_verified_win_floor(
    evaluation: ExactCombatEvaluation,
    source: &'static str,
    final_hp: i32,
    witness_actions: Vec<ClientInput>,
) -> ExactCombatEvaluation {
    if matches!(
        &evaluation.evidence,
        ExactCombatEvidence::ExactWin {
            final_hp: searched_hp,
            ..
        } if *searched_hp >= final_hp
    ) {
        return evaluation;
    }
    let search_cost = match &evaluation.evidence {
        ExactCombatEvidence::ExactWin { search_cost, .. } => search_cost.clone(),
        ExactCombatEvidence::ExactRefutation { search_cost, .. }
        | ExactCombatEvidence::BudgetUnknown { search_cost, .. } => Some(search_cost.clone()),
        ExactCombatEvidence::ExactTerminalNonWin { .. } => None,
    };
    ExactCombatEvaluation {
        evidence: ExactCombatEvidence::ExactWin {
            source: source.to_string(),
            final_hp,
            suffix_action_count: witness_actions.len(),
            search_cost,
        },
        witness_actions: Some(witness_actions),
    }
}

pub(crate) fn evaluate_nonterminal_position(
    position: &CombatPosition,
    solve_work: usize,
    max_engine_steps_per_transition: usize,
    max_potions_used: Option<u32>,
) -> Result<ExactCombatEvaluation, String> {
    let root = CombatDecisionRoot::new(position.clone())
        .map_err(|error| format!("invalid successor root: {error:?}"))?;
    let search_config = LocalTurnGraphWitnessConfig {
        generator: TurnOptionGeneratorConfig {
            max_engine_steps_per_transition,
            ..TurnOptionGeneratorConfig::default()
        },
        generation_quantum_work: 4,
        max_turn_depth: 32,
        satisfaction: OracleCombatWitnessSatisfaction::BudgetOrExhaustion,
        max_potions_used,
        ..LocalTurnGraphWitnessConfig::default()
    };
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        root,
        search_config,
        retained_combat_tactical_policy_v1(),
    );
    let report = session.advance(
        LocalTurnGraphWitnessQuantum {
            additional_selections: solve_work.saturating_mul(8),
            additional_generation_work: solve_work,
            additional_engine_steps: solve_work.saturating_mul(max_engine_steps_per_transition),
            deadline: None,
        },
        &EngineCombatStepper,
    );
    if let Some(witness) = report.witness.as_ref() {
        return Ok(ExactCombatEvaluation {
            evidence: ExactCombatEvidence::ExactWin {
                source: "bounded_equal_work_search".to_string(),
                final_hp: witness.final_position.combat.entities.player.current_hp,
                suffix_action_count: witness.actions.len(),
                search_cost: Some(search_cost(&report)),
            },
            witness_actions: Some(
                witness
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect(),
            ),
        });
    }
    if matches!(
        report.status,
        LocalTurnGraphWitnessStatus::FrontierExhausted
    ) && report.generation_gaps.is_empty()
        && report.counters.depth_limited_successors == 0
    {
        return Ok(ExactCombatEvaluation {
            evidence: ExactCombatEvidence::ExactRefutation {
                source: "gap_free_frontier_exhaustion".to_string(),
                search_cost: search_cost(&report),
            },
            witness_actions: None,
        });
    }
    let progress = session.progress_snapshot();
    Ok(ExactCombatEvaluation {
        evidence: ExactCombatEvidence::BudgetUnknown {
            status: format!("{:?}", report.status),
            search_cost: search_cost(&report),
            deepest_player_turn: progress.max_player_turn,
            gap_count: report.generation_gaps.len(),
            depth_limited_successors: report.counters.depth_limited_successors,
        },
        witness_actions: None,
    })
}

/// Evaluates any unresolved player-input state. Ordinary action states enter
/// exact combat search directly. Structured pending choices are first
/// materialized into canonical complete inputs; each resulting stable state
/// receives an equal share of the parent action's search budget. A discovered
/// win is replayable through the selected structured input. A bounded miss or
/// truncated structured surface remains `BudgetUnknown`.
pub(crate) fn evaluate_unresolved_position(
    position: &CombatPosition,
    solve_work: usize,
    max_structured_alternatives: usize,
    max_engine_steps_per_transition: usize,
    max_potions_used: Option<u32>,
) -> Result<ExactCombatEvaluation, String> {
    let EngineState::PendingChoice(choice) = &position.engine else {
        return evaluate_nonterminal_position(
            position,
            solve_work,
            max_engine_steps_per_transition,
            max_potions_used,
        );
    };
    let Some(inputs) =
        sts_oracle_runtime::ai::combat_search_v2::pending_choice_action_prefix::canonical_pending_choice_inputs(
            choice,
        )
    else {
        return Ok(ExactCombatEvaluation {
            evidence: ExactCombatEvidence::BudgetUnknown {
                status: "UnsupportedStructuredChoice".to_string(),
                search_cost: ExactCombatSearchCost::default(),
                deepest_player_turn: position.combat.turn.turn_count,
                gap_count: 1,
                depth_limited_successors: 0,
            },
            witness_actions: None,
        });
    };
    let mut inputs = inputs
        .take(max_structured_alternatives.saturating_add(1))
        .collect::<Vec<_>>();
    let surface_truncated = inputs.len() > max_structured_alternatives;
    inputs.truncate(max_structured_alternatives);
    if inputs.is_empty() {
        return Err("structured successor has no canonical legal input".to_string());
    }

    let per_choice_work = solve_work.div_ceil(inputs.len()).max(1);
    let mut aggregate_cost = ExactCombatSearchCost::default();
    let mut deepest_player_turn = position.combat.turn.turn_count;
    let mut gap_count = usize::from(surface_truncated);
    let mut depth_limited_successors = 0usize;
    let mut unknown_seen = surface_truncated;
    for input in inputs {
        let step = EngineCombatStepper.apply_to_stable(
            position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            unknown_seen = true;
            gap_count = gap_count.saturating_add(1);
            continue;
        }
        match step.terminal {
            CombatTerminal::Win if !step.position.combat.runtime.combat_smoked => {
                return Ok(ExactCombatEvaluation {
                    evidence: known_exact_win(
                        "structured_choice_immediate_terminal_replay",
                        step.position.combat.entities.player.current_hp,
                        1,
                    ),
                    witness_actions: Some(vec![input]),
                });
            }
            CombatTerminal::Win | CombatTerminal::Loss => continue,
            CombatTerminal::Unresolved => {}
        }
        let evaluation = evaluate_unresolved_position(
            &step.position,
            per_choice_work,
            max_structured_alternatives,
            max_engine_steps_per_transition,
            max_potions_used,
        )?;
        match &evaluation.evidence {
            ExactCombatEvidence::ExactWin { .. } => {
                let mut actions = vec![input];
                actions.extend(evaluation.witness_actions.unwrap_or_default());
                let mut evidence = evaluation.evidence;
                if let ExactCombatEvidence::ExactWin {
                    suffix_action_count,
                    ..
                } = &mut evidence
                {
                    *suffix_action_count = suffix_action_count.saturating_add(1);
                }
                return Ok(ExactCombatEvaluation {
                    evidence,
                    witness_actions: Some(actions),
                });
            }
            ExactCombatEvidence::ExactRefutation { search_cost, .. } => {
                aggregate_cost.add_assign(search_cost);
            }
            ExactCombatEvidence::ExactTerminalNonWin { .. } => {}
            ExactCombatEvidence::BudgetUnknown {
                search_cost,
                deepest_player_turn: child_deepest,
                gap_count: child_gaps,
                depth_limited_successors: child_depth_limited,
                ..
            } => {
                unknown_seen = true;
                aggregate_cost.add_assign(search_cost);
                deepest_player_turn = deepest_player_turn.max(*child_deepest);
                gap_count = gap_count.saturating_add(*child_gaps);
                depth_limited_successors =
                    depth_limited_successors.saturating_add(*child_depth_limited);
            }
        }
    }
    if unknown_seen {
        return Ok(ExactCombatEvaluation {
            evidence: ExactCombatEvidence::BudgetUnknown {
                status: if surface_truncated {
                    "StructuredChoiceSurfaceTruncated"
                } else {
                    "StructuredChoiceContainsBudgetUnknown"
                }
                .to_string(),
                search_cost: aggregate_cost,
                deepest_player_turn,
                gap_count,
                depth_limited_successors,
            },
            witness_actions: None,
        });
    }
    Ok(ExactCombatEvaluation {
        evidence: ExactCombatEvidence::ExactRefutation {
            source: "structured_choices_all_exact_non_win".to_string(),
            search_cost: aggregate_cost,
        },
        witness_actions: None,
    })
}

fn search_cost(report: &LocalTurnGraphWitnessReport) -> ExactCombatSearchCost {
    ExactCombatSearchCost {
        generation_work: report.counters.generation_work,
        applied_action_transitions: report.counters.applied_action_transitions,
        engine_steps: report.counters.engine_steps,
        exact_nodes: report.counters.exact_nodes,
        exact_edges: report.counters.exact_edges,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        exact_terminal_non_win, known_exact_win, ExactCombatEvidence, ExactCombatSearchCost,
    };

    #[test]
    fn exact_evidence_kinds_do_not_collapse_unknown_with_terminal_results() {
        let cost = ExactCombatSearchCost {
            generation_work: 1,
            applied_action_transitions: 2,
            engine_steps: 3,
            exact_nodes: 4,
            exact_edges: 5,
        };
        assert_eq!(known_exact_win("verified", 17, 3).kind(), "exact_win");
        assert_eq!(
            ExactCombatEvidence::ExactRefutation {
                source: "exhausted".to_string(),
                search_cost: cost.clone(),
            }
            .kind(),
            "exact_refutation"
        );
        assert_eq!(
            exact_terminal_non_win("Loss").kind(),
            "exact_terminal_non_win"
        );
        assert_eq!(
            ExactCombatEvidence::BudgetUnknown {
                status: "Partial".to_string(),
                search_cost: cost,
                deepest_player_turn: 2,
                gap_count: 0,
                depth_limited_successors: 0,
            }
            .kind(),
            "budget_unknown"
        );
    }
}
