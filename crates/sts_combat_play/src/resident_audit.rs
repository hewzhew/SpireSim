use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use sts_combat_planner::{
    combat_plan_state_guide_policy_v1, exact_trajectory_potion_expenditures,
    root_initial_expansion_work_for_budget, CombatDecisionRoot, LocalTurnGraphEdgeSnapshot,
    LocalTurnGraphStateSnapshot, LocalTurnGraphWitnessConfig, LocalTurnGraphWitnessQuantum,
    LocalTurnGraphWitnessSession, PublicPlanTurnProposalConfig, TurnOptionGeneratorConfig,
    DEFAULT_BACKED_GENERATION_QUANTUM_WORK,
};
use sts_core::sim::combat::{combat_terminal, CombatTerminal, EngineCombatStepper};
use sts_oracle_report::{
    validate_resident_corridor_audit_v1, ResidentCorridorAuditV1, ResidentCorridorEdgeAuditV1,
    ResidentCorridorGuideServiceAuditV1, ResidentCorridorSearchCountersV1,
    ResidentCorridorStateAuditV1, ResidentCorridorTurnAuditV1, ResidentCorridorWitnessAuditV1,
    RESIDENT_CORRIDOR_AUDIT_SCHEMA_V1,
};

use crate::session::{
    exact_hash, replay_session, replay_session_prefix, PlaySessionV1, MAX_ENGINE_STEPS,
};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ResidentCorridorAuditSatisfactionV1 {
    FirstWitness,
    ZeroLossOrBudget,
    BudgetOrExhaustion,
}

impl ResidentCorridorAuditSatisfactionV1 {
    fn label(self) -> &'static str {
        match self {
            Self::FirstWitness => "first_witness",
            Self::ZeroLossOrBudget => "zero_loss_or_budget",
            Self::BudgetOrExhaustion => "budget_or_exhaustion",
        }
    }

    fn planner_satisfaction(self) -> sts_combat_planner::OracleCombatWitnessSatisfaction {
        match self {
            Self::FirstWitness => sts_combat_planner::OracleCombatWitnessSatisfaction::FirstWitness,
            Self::ZeroLossOrBudget => {
                sts_combat_planner::OracleCombatWitnessSatisfaction::HpLossAtMost(0)
            }
            Self::BudgetOrExhaustion => {
                sts_combat_planner::OracleCombatWitnessSatisfaction::BudgetOrExhaustion
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ResidentCorridorAuditOptionsV1 {
    pub from_turn_index: usize,
    pub generation_work: usize,
    pub wall_ms: u64,
    pub satisfaction: ResidentCorridorAuditSatisfactionV1,
}

pub fn run_resident_corridor_audit_v1(
    session: &PlaySessionV1,
    options: ResidentCorridorAuditOptionsV1,
) -> Result<ResidentCorridorAuditV1, String> {
    if options.generation_work == 0 || options.wall_ms == 0 {
        return Err("resident corridor audit requires positive search limits".to_owned());
    }
    if options.from_turn_index >= session.turns.len() {
        return Err(format!(
            "resident corridor audit start {} is outside the {}-turn teacher line",
            options.from_turn_index,
            session.turns.len()
        ));
    }
    let terminal = replay_session(session)?;
    if combat_terminal(&terminal.engine, &terminal.combat) != CombatTerminal::Win {
        return Err(
            "resident corridor audit requires a replay-verified terminal winning session"
                .to_owned(),
        );
    }
    let start = replay_session_prefix(session, options.from_turn_index)?;
    let root_exact_state_hash = exact_hash(&start);
    let decision_root = CombatDecisionRoot::new(start.clone())
        .map_err(|error| format!("invalid resident corridor root: {error:?}"))?;
    let guide_service_bias = sts_combat_knowledge::darkling_survival_guide_service_bias_v1(&start);
    let policy =
        combat_plan_state_guide_policy_v1(sts_combat_atomic::retained_combat_tactical_policy_v1());
    let mut resident = LocalTurnGraphWitnessSession::with_policy(
        decision_root,
        LocalTurnGraphWitnessConfig {
            generator: TurnOptionGeneratorConfig {
                max_engine_steps_per_transition: MAX_ENGINE_STEPS,
                allow_potion_expenditure: session.allow_potions,
                allow_potion_discard: false,
                ..TurnOptionGeneratorConfig::default()
            },
            generation_quantum_work: 4,
            backed_generation_quantum_work: DEFAULT_BACKED_GENERATION_QUANTUM_WORK,
            guide_service_bias,
            initial_expansion_work: 64,
            root_initial_expansion_work: root_initial_expansion_work_for_budget(
                options.generation_work,
            ),
            max_turn_depth: 32,
            satisfaction: options.satisfaction.planner_satisfaction(),
            require_no_unrecovered_stolen_gold: matches!(
                options.satisfaction,
                ResidentCorridorAuditSatisfactionV1::ZeroLossOrBudget
            ),
            max_potions_used: if session.allow_potions { None } else { Some(0) },
        },
        policy,
    );
    resident
        .enable_public_plan_turn_proposals(PublicPlanTurnProposalConfig::default())
        .map_err(|error| format!("cannot enable public-plan turn proposals: {error:?}"))?;

    let started = Instant::now();
    let search = resident.advance(
        LocalTurnGraphWitnessQuantum {
            additional_selections: options.generation_work,
            additional_generation_work: options.generation_work,
            additional_engine_steps: options.generation_work.saturating_mul(MAX_ENGINE_STEPS),
            deadline: Some(Instant::now() + Duration::from_millis(options.wall_ms)),
        },
        &EngineCombatStepper,
    );
    let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;

    let mut source_hash = root_exact_state_hash.clone();
    let mut turns = Vec::with_capacity(session.turns.len() - options.from_turn_index);
    for (teacher_turn_index, teacher) in session
        .turns
        .iter()
        .enumerate()
        .skip(options.from_turn_index)
    {
        let successor_hash = teacher.end_exact_state_hash.clone();
        turns.push(ResidentCorridorTurnAuditV1 {
            teacher_turn_index,
            from_turn: teacher.from_turn,
            teacher_action_count: teacher.actions.len(),
            source_exact_state_hash: source_hash.clone(),
            successor_exact_state_hash: successor_hash.clone(),
            source: resident
                .state_snapshot_by_exact_hash(&source_hash)
                .map(project_state),
            edge: resident
                .edge_snapshot_by_exact_hashes(&source_hash, &successor_hash)
                .map(project_edge),
            successor: resident
                .state_snapshot_by_exact_hash(&successor_hash)
                .map(project_state),
        });
        source_hash = successor_hash;
    }

    let counters = &search.counters;
    let witness = resident
        .witness()
        .map(|witness| ResidentCorridorWitnessAuditV1 {
            starting_player_hp: start.combat.entities.player.current_hp,
            final_player_hp: witness.final_position.combat.entities.player.current_hp,
            max_player_hp: witness.final_position.combat.entities.player.max_hp,
            action_count: witness.actions.len(),
            potion_expenditures: exact_trajectory_potion_expenditures(
                &start,
                &witness.actions,
                &witness.final_position,
            ),
            final_exact_state_hash: exact_hash(&witness.final_position),
        });
    let audit = ResidentCorridorAuditV1 {
        schema: RESIDENT_CORRIDOR_AUDIT_SCHEMA_V1.to_owned(),
        root_exact_state_hash,
        from_turn_index: options.from_turn_index,
        configured_generation_work: options.generation_work,
        configured_wall_ms: options.wall_ms,
        configured_satisfaction: options.satisfaction.label().to_owned(),
        public_plan_turn_proposal_enabled: resident.public_plan_turn_proposal_enabled(),
        elapsed_ms,
        search_status: format!("{:?}", search.status),
        witness_found: witness.is_some(),
        witness,
        counters: ResidentCorridorSearchCountersV1 {
            selections: counters.selections,
            generation_work: counters.generation_work,
            engine_steps: counters.engine_steps,
            exact_nodes: counters.exact_nodes,
            exact_edges: counters.exact_edges,
            completed_turn_options: counters.completed_turn_options,
            terminal_win_options: counters.terminal_win_options,
            public_plan_proposal_attempts: counters.public_plan_proposal_attempts,
            public_plan_proposal_retained_options: counters.public_plan_proposal_retained_options,
        },
        turns,
    };
    validate_resident_corridor_audit_v1(&audit)?;
    Ok(audit)
}

pub fn save_resident_corridor_audit_v1(
    path: &Path,
    report: &ResidentCorridorAuditV1,
) -> Result<(), String> {
    validate_resident_corridor_audit_v1(report)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create resident corridor audit directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode resident corridor audit: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "failed to create fresh resident corridor audit {}: {error}",
                path.display()
            )
        })?;
    file.write_all(&bytes).map_err(|error| {
        format!(
            "failed to write resident corridor audit {}: {error}",
            path.display()
        )
    })
}

fn project_state(state: LocalTurnGraphStateSnapshot) -> ResidentCorridorStateAuditV1 {
    ResidentCorridorStateAuditV1 {
        relative_turn_depth: state.relative_turn_depth,
        visits: state.visits,
        first_service_selection: state.first_service_selection,
        first_guide_service_selection: state.first_guide_service_selection,
        generation_work: state.generation_work,
        retained_generator_work_items: state.retained_generator_work_items,
        generator_anchor_work_pops: state.generator_anchor_work_pops,
        generator_guided_work_pops: state.generator_guided_work_pops,
        generated_options: state.generated_options,
        children: state.children,
        exhausted: state.exhausted,
    }
}

fn project_edge(edge: LocalTurnGraphEdgeSnapshot) -> ResidentCorridorEdgeAuditV1 {
    ResidentCorridorEdgeAuditV1 {
        negative_log_policy: edge.negative_log_policy,
        plan_prefix_proposed: edge.plan_prefix_proposed,
        visits: edge.visits,
        anchor_visits: edge.anchor_visits,
        backed_visits: edge.backed_visits,
        successor_path_cost: edge.successor_path_cost,
        successor_anchor_ordinal_rank: edge.successor_anchor_ordinal_rank,
        successor_anchor_candidate_count: edge.successor_anchor_candidate_count,
        successor_anchor_service_cost: edge.successor_anchor_service_cost,
        best_anchor_service_cost: edge.best_anchor_service_cost,
        guide_service: edge
            .guide_service
            .into_iter()
            .map(|guide| ResidentCorridorGuideServiceAuditV1 {
                lane: guide.lane,
                edge_visits: guide.edge_visits,
                sibling_ordinal_rank: guide.sibling_ordinal_rank,
                sibling_candidate_count: guide.sibling_candidate_count,
                global_ordinal_rank: guide.global_ordinal_rank,
                global_candidate_count: guide.global_candidate_count,
            })
            .collect(),
    }
}
