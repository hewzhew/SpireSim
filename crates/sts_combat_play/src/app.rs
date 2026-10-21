use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use serde::Serialize;
use sts_agent::agent::information::action::{
    project_public_combat_actions_v1, public_choice_for_exact_combat_input_v1,
    PublicCombatActionChoiceV1, PublicCombatSelectionDomainCandidateV1,
};
use sts_agent::agent::information::state::public_combat_state_v1;
use sts_agent::agent::task::{
    advance_public_combat_plan_v1, public_combat_agenda_for_plan_v1, public_combat_plan_v1,
    PublicCombatPlanStateV1,
};
use sts_combat_atomic::ai::combat_search_v2::{
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices,
    run_combat_search_v2, CombatSearchV2Config, CombatSearchV2PotionPolicy,
    CombatSearchV2Satisfaction, CombatSearchV2TurnPlanPolicy,
    CombatSearchV2TurnPlanProbeEnumeration,
};
use sts_combat_option_grounder::{ground_public_combat_agenda_v1, GroundableCompleteTurnV1};
use sts_combat_planner::{
    combat_plan_state_guide_policy_v1, root_initial_expansion_work_for_budget, CombatDecisionRoot,
    LocalTurnGraphWitnessConfig, LocalTurnGraphWitnessQuantum, LocalTurnGraphWitnessSession,
    LocalTurnGraphWitnessStatus, OracleCombatWitnessSatisfaction, PublicPlanTurnProposalConfig,
    TurnOptionGeneratorConfig, DEFAULT_BACKED_GENERATION_QUANTUM_WORK,
};
use sts_combat_storyboard::combat_witness_storyboard::{
    build_combat_witness_storyboard_v1, CombatWitnessStoryboardBoundaryV1,
};
use sts_combat_strategy::combat_plan_projection_v1;
use sts_complete_turn_env::{
    complete_turn_surface_with_config_v1, project_complete_turn_surface_v1,
    replay_verified_winning_complete_turn_line_v1, CompleteTurnExactEnvV1,
};
use sts_complete_turn_proposal::{
    exact_complete_turn_proposals_guided_v1, exact_complete_turn_proposals_v1,
    ExactCompleteTurnProposalBatchV1, ExactCompleteTurnProposalConfigV1,
    ExactCompleteTurnProposalGuideRankV1, ExactCompleteTurnProposalV1,
};
use sts_complete_turn_view::{
    CompleteTurnCandidateEvidenceV1, CompleteTurnDemonstrationDecisionV1,
    CompleteTurnPositivePrefixEvidenceV1, CompleteTurnWinningDemonstrationV1,
    COMPLETE_TURN_WINNING_DEMONSTRATION_SCHEMA,
};
use sts_core::content::identity::gameplay_display_label_v1;
use sts_core::content::powers::store;
use sts_core::runtime::combat::{CombatCard, CombatState, MonsterEntity};
use sts_core::sim::combat::{combat_terminal, CombatPosition, CombatTerminal, EngineCombatStepper};
use sts_core::sim::combat_projection::project_monster_move_preview_in_combat;
use sts_core::state::core::{ClientInput, EngineState};

use crate::resident_audit::{
    run_resident_corridor_audit_v1, save_resident_corridor_audit_v1,
    ResidentCorridorAuditOptionsV1, ResidentCorridorAuditSatisfactionV1,
};
use crate::session::*;

#[derive(Clone)]
struct PlannedPosition {
    position: CombatPosition,
    plan: PublicCombatPlanStateV1,
}

#[derive(Debug, Parser)]
#[command(
    name = "combat_play",
    about = "Play one exact combat case turn by turn without running whole-combat search"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start a fresh play session directly from an exact combat case.
    New {
        #[arg(long)]
        case: PathBuf,
        #[arg(long)]
        session: PathBuf,
        /// Admit legal potion use while enumerating complete-turn plans.
        #[arg(long)]
        allow_potions: bool,
        #[arg(long, default_value_t = INTERACTIVE_MAX_INNER_NODES)]
        max_inner_nodes: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_PARTIAL_FRONTIER_STATES)]
        max_partial_frontier_states: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_END_STATES)]
        max_end_states: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_END_STATES)]
        per_bucket_limit: usize,
        #[arg(long, default_value_t = 16)]
        limit: usize,
    },
    /// Replay a complete exact winning action line, verify every complete-turn
    /// successor against the configured bounded surface, and save a session.
    ImportWinningActions {
        #[arg(long)]
        case: PathBuf,
        #[arg(long)]
        actions: PathBuf,
        #[arg(long)]
        session: PathBuf,
        /// Admit legal potion use while enumerating complete-turn plans.
        #[arg(long)]
        allow_potions: bool,
        #[arg(long, default_value_t = INTERACTIVE_MAX_INNER_NODES)]
        max_inner_nodes: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_PARTIAL_FRONTIER_STATES)]
        max_partial_frontier_states: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_END_STATES)]
        max_end_states: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_END_STATES)]
        per_bucket_limit: usize,
    },
    /// Show the current exact state and a compact complete-turn surface.
    Show {
        session: PathBuf,
        #[arg(long, default_value_t = 16)]
        limit: usize,
    },
    /// Save the existing complete-turn preselection and coverage audit for the
    /// current replay-exact boundary without changing proposal behavior.
    ProposalAudit {
        session: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Test whether public positive-prefix evidence can carry one already
    /// replay-verified turn through a smaller bounded proposal sampler.
    PrefixRecallAudit {
        session: PathBuf,
        #[arg(long, default_value_t = 0)]
        turn_index: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_INNER_NODES)]
        max_inner_nodes: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_PARTIAL_FRONTIER_STATES)]
        max_partial_frontier_states: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_END_STATES)]
        max_end_states: usize,
        #[arg(long, default_value_t = INTERACTIVE_MAX_END_STATES)]
        per_bucket_limit: usize,
        #[arg(long)]
        output: PathBuf,
    },
    /// Choose one displayed plan index and advance exactly one complete turn.
    Choose {
        session: PathBuf,
        plan: usize,
        #[arg(long, default_value_t = 16)]
        limit: usize,
    },
    /// Undo the most recent chosen complete turn.
    Back { session: PathBuf },
    /// Return to the exact case root while keeping the session.
    Reset { session: PathBuf },
    /// Print the compact replay-verified route chosen so far.
    History { session: PathBuf },
    /// Explain the replay-verified route as exact per-turn state and named actions.
    Storyboard {
        session: PathBuf,
        /// Also save the complete typed storyboard as fresh JSON.
        #[arg(long)]
        json_output: Option<PathBuf>,
    },
    /// Expand public-agenda non-dominated complete-turn frontiers without
    /// using a retained route to guide generation.
    AgendaCorridor {
        session: PathBuf,
        #[arg(long, default_value_t = 3)]
        depth: usize,
        /// Abort rather than truncate if one exact layer exceeds this count.
        #[arg(long, default_value_t = 4096)]
        max_states: usize,
    },
    /// Run only the resident turn graph from one replay-verified teacher
    /// boundary and save exact corridor membership and service facts.
    ResidentCorridorAudit {
        session: PathBuf,
        #[arg(long, default_value_t = 0)]
        from_turn_index: usize,
        #[arg(long, default_value_t = 4_096)]
        generation_work: usize,
        #[arg(long, default_value_t = 500)]
        wall_ms: u64,
        #[arg(long, value_enum, default_value_t = ResidentCorridorAuditSatisfactionV1::ZeroLossOrBudget)]
        satisfaction: ResidentCorridorAuditSatisfactionV1,
        #[arg(long)]
        output: PathBuf,
    },
    /// From the session's current replay-exact boundary, follow the public
    /// plan to lethal conversion and validate a complete winning suffix.
    AgendaValidate {
        session: PathBuf,
        #[arg(long, default_value_t = 6)]
        max_agenda_turns: usize,
        #[arg(long, default_value_t = 4_096)]
        resident_generation_work: usize,
        #[arg(long, default_value_t = 500)]
        resident_wall_ms: u64,
        /// Enable synchronous complete-turn proposal beam search with this width.
        #[arg(long, default_value_t = 0)]
        proposal_beam_width: usize,
        /// Maximum exact player-turn boundaries expanded by the proposal beam.
        #[arg(long, default_value_t = 0)]
        proposal_beam_boundaries: usize,
        /// Wall-clock allowance for the proposal beam.
        #[arg(long, default_value_t = 0)]
        proposal_beam_wall_ms: u64,
        #[arg(long, default_value_t = 4_096)]
        atomic_nodes: usize,
        #[arg(long, default_value_t = 1_000)]
        atomic_wall_ms: u64,
        /// Save the replay-verified root-to-terminal action line when found.
        #[arg(long)]
        winning_actions: Option<PathBuf>,
        /// Save each distinct final proposal-beam guide route as a replayable
        /// combat_play session below one fresh directory.
        #[arg(long)]
        proposal_beam_sessions: Option<PathBuf>,
    },
    /// Export one fully replay-verified terminal winning route as exact inputs.
    ExportWinningActions {
        session: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Export one terminally verified route as ragged complete-turn decision
    /// rows. The chosen candidate is positive demonstration evidence; every
    /// unchosen candidate remains explicitly unknown.
    ExportWinningDemonstration {
        session: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

pub fn main_entry() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    match Cli::parse().command {
        Command::New {
            case,
            session,
            allow_potions,
            max_inner_nodes,
            max_partial_frontier_states,
            max_end_states,
            per_bucket_limit,
            limit,
        } => {
            require_positive_limit(limit)?;
            let proposal_limits = PlayProposalLimitsV1 {
                max_inner_nodes,
                max_partial_frontier_states,
                max_end_states,
                per_bucket_limit,
            };
            validate_proposal_limits(proposal_limits)?;
            if session.exists() {
                return Err(format!(
                    "play session '{}' already exists; use reset or choose a fresh path",
                    session.display()
                ));
            }
            let case = case.canonicalize().map_err(|error| {
                format!(
                    "failed to resolve combat case '{}': {error}",
                    case.display()
                )
            })?;
            let root = load_case_root(&case)?;
            let play = PlaySessionV1 {
                schema: SESSION_SCHEMA.to_owned(),
                case,
                root_exact_state_hash: exact_hash(&root),
                allow_potions,
                proposal_limits,
                turns: Vec::new(),
            };
            save_session(&session, &play)?;
            show(&session, &play, &root, limit)
        }
        Command::ImportWinningActions {
            case,
            actions,
            session,
            allow_potions,
            max_inner_nodes,
            max_partial_frontier_states,
            max_end_states,
            per_bucket_limit,
        } => {
            let proposal_limits = PlayProposalLimitsV1 {
                max_inner_nodes,
                max_partial_frontier_states,
                max_end_states,
                per_bucket_limit,
            };
            validate_proposal_limits(proposal_limits)?;
            import_winning_actions(&case, &actions, &session, allow_potions, proposal_limits)
        }
        Command::Show { session, limit } => {
            require_positive_limit(limit)?;
            let play = load_session(&session)?;
            let position = replay_session(&play)?;
            show(&session, &play, &position, limit)
        }
        Command::ProposalAudit { session, output } => {
            let play = load_session(&session)?;
            let position = replay_session(&play)?;
            ensure_player_turn(&position)?;
            let surface = enumerate_legacy_turn(&position, &play);
            let policy = combat_plan_state_guide_policy_v1(
                sts_combat_atomic::retained_combat_tactical_policy_v1(),
            );
            let guided = exact_complete_turn_proposals_guided_v1(
                &position,
                session_proposal_config(&play),
                &|successor| {
                    policy
                        .state_guides(successor)
                        .into_iter()
                        .map(|guide| ExactCompleteTurnProposalGuideRankV1 {
                            lane: guide.lane.value(),
                            components: guide.rank.components().to_vec(),
                        })
                        .collect()
                },
            );
            let guided_candidates = guided
                .candidates
                .iter()
                .enumerate()
                .map(|(plan_index, candidate)| {
                    let guide_ranks = policy
                        .state_guides(&candidate.successor)
                        .into_iter()
                        .map(|guide| serde_json::json!({
                            "lane": guide.lane.value(),
                            "components": guide.rank.components(),
                        }))
                        .collect::<Vec<_>>();
                    Ok(serde_json::json!({
                        "production_plan_index": plan_index,
                        "actions": candidate.actions.iter().map(|action| &action.input).collect::<Vec<_>>(),
                        "action_line": named_action_line(
                            &position,
                            &candidate.actions.iter().map(|action| action.input.clone()).collect::<Vec<_>>(),
                        )?,
                        "support_claims": candidate.support_claims.iter().map(|claim| format!("{claim:?}")).collect::<Vec<_>>(),
                        "potion_slot_mask": candidate.potion_slot_mask,
                        "potion_expenditures": candidate.potion_expenditures,
                        "guide_ranks": guide_ranks,
                        "successor_exact_state_hash": exact_hash(&candidate.successor),
                        "successor_player_hp": candidate.successor.combat.entities.player.current_hp,
                        "successor_total_enemy_hp": living_enemy_hp(&candidate.successor),
                    }))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let legacy_selected_candidate_guides = surface
                .candidates
                .iter()
                .map(|candidate| {
                    Ok(serde_json::json!({
                        "legacy_plan_index": candidate.report.plan_index,
                        "action_line": named_action_line(
                            &position,
                            &candidate.report.actions.iter().map(|action| action.input.clone()).collect::<Vec<_>>(),
                        )?,
                        "successor_player_hp": candidate.position.combat.entities.player.current_hp,
                        "successor_total_enemy_hp": living_enemy_hp(&candidate.position),
                        "guide_ranks": policy
                            .state_guides(&candidate.position)
                            .into_iter()
                            .map(|guide| serde_json::json!({
                                "lane": guide.lane.value(),
                                "components": guide.rank.components(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let legacy_preselection_count = surface.report.enumeration.preselection_plans;
            let legacy_selected_count = surface.report.enumeration.plans;
            let guided_selected_count = guided.candidates.len();
            let resource_profile_retained_count = guided.resource_profile_retained_candidate_count;
            let support_retained_count = guided.support_retained_candidate_count;
            let report = serde_json::json!({
                "schema": "exact_combat_play_proposal_audit_v1",
                "current_turn": position.combat.turn.turn_count,
                "current_exact_state_hash": exact_hash(&position),
                "legacy_selection": surface.report,
                "legacy_selected_candidate_guides": legacy_selected_candidate_guides,
                "guided_selection": {
                    "selected_candidates": guided_candidates,
                    "selected_candidate_count": guided_selected_count,
                    "preselection_candidate_count": guided.preselection_candidate_count,
                    "resource_profile_retained_candidate_count": resource_profile_retained_count,
                    "support_retained_candidate_count": support_retained_count,
                },
            });
            write_fresh_json(&output, &report)?;
            println!(
                "Saved turn {} proposal audit: {} preselection candidates -> {} legacy / {} guided plans ({} resource / {} support representatives) at {}.",
                position.combat.turn.turn_count,
                legacy_preselection_count,
                legacy_selected_count,
                guided_selected_count,
                resource_profile_retained_count,
                support_retained_count,
                output.display()
            );
            Ok(())
        }
        Command::PrefixRecallAudit {
            session,
            turn_index,
            max_inner_nodes,
            max_partial_frontier_states,
            max_end_states,
            per_bucket_limit,
            output,
        } => {
            let limits = PlayProposalLimitsV1 {
                max_inner_nodes,
                max_partial_frontier_states,
                max_end_states,
                per_bucket_limit,
            };
            validate_proposal_limits(limits)?;
            let play = load_session(&session)?;
            let position = replay_session_prefix(&play, turn_index)?;
            ensure_player_turn(&position)?;
            let source_turn = play.turns.get(turn_index).ok_or_else(|| {
                format!(
                    "play session contains {} turns, so turn index {turn_index} has no verified teacher line",
                    play.turns.len()
                )
            })?;
            if position.combat.turn.turn_count != source_turn.from_turn {
                return Err(format!(
                    "teacher turn index {turn_index} expects turn {}, reached {}",
                    source_turn.from_turn, position.combat.turn.turn_count
                ));
            }
            let evidence = CompleteTurnPositivePrefixEvidenceV1::from_replayed_complete_turn(
                &position,
                &source_turn.actions,
                MAX_ENGINE_STEPS,
            )?;
            let positive_prefix_count = evidence.positive_prefix_count();
            let config = ExactCompleteTurnProposalConfigV1 {
                max_inner_nodes,
                max_partial_frontier_states,
                max_end_states,
                per_bucket_limit,
                max_engine_steps_per_action: MAX_ENGINE_STEPS,
                allow_potion_expenditure: play.allow_potions,
                max_potion_expenditures: if play.allow_potions { None } else { Some(0) },
                allowed_potion_slots: None,
                allow_potion_discard: false,
            };
            let mut env = CompleteTurnExactEnvV1::from_position_with_config(position, config)
                .with_partial_prefix_scorer(Arc::new(evidence))?;
            let matched_ordinal = env.matching_successor_ordinal(&source_turn.actions)?;
            let diagnostics = env.surface_diagnostics()?;
            let report = serde_json::json!({
                "schema": "exact_combat_play_prefix_recall_audit_v1",
                "source_session": session,
                "source_turn_index": turn_index,
                "source_turn": source_turn.from_turn,
                "teacher_evidence": "replay_verified_public_positive_prefixes",
                "positive_prefix_count": positive_prefix_count,
                "source_action_count": source_turn.actions.len(),
                "source_end_exact_state_hash": source_turn.end_exact_state_hash,
                "matched_successor": matched_ordinal.is_some(),
                "matched_ordinal": matched_ordinal,
                "surface": {
                    "max_inner_nodes": diagnostics.max_inner_nodes,
                    "max_partial_frontier_states": diagnostics.max_partial_frontier_states,
                    "max_end_states": diagnostics.max_end_states,
                    "per_bucket_limit": diagnostics.per_bucket_limit,
                    "candidate_count": diagnostics.candidate_count,
                    "nodes_expanded": diagnostics.nodes_expanded,
                    "nodes_generated": diagnostics.nodes_generated,
                    "engine_steps": diagnostics.engine_steps,
                    "preselection_candidate_count": diagnostics.preselection_candidate_count,
                    "partial_prefix_scorer_active": diagnostics.partial_prefix_scorer_active,
                },
            });
            write_fresh_json(&output, &report)?;
            println!(
                "Prefix recall audit turn {turn_index}: matched={} ordinal={:?}, {} candidates / {} expanded nodes at {}.",
                matched_ordinal.is_some(),
                matched_ordinal,
                diagnostics.candidate_count,
                diagnostics.nodes_expanded,
                output.display()
            );
            Ok(())
        }
        Command::Choose {
            session,
            plan,
            limit,
        } => {
            require_positive_limit(limit)?;
            let mut play = load_session(&session)?;
            let position = replay_session(&play)?;
            ensure_player_turn(&position)?;
            let surface = session_turn_surface(&position, &play);
            let chosen = surface
                .candidates
                .iter()
                .enumerate()
                .find(|(plan_index, _)| *plan_index == plan)
                .map(|(_, candidate)| candidate)
                .ok_or_else(|| {
                    let visible = compact_proposals(&surface, limit)
                        .iter()
                        .map(|(plan_index, _)| plan_index.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "plan {plan} is not on this exact turn surface; displayed plans: [{visible}]"
                    )
                })?;
            let end_hash = exact_hash(&chosen.successor);
            verify_proposal_successor(chosen, &end_hash)?;
            play.turns.push(ChosenTurnV1 {
                from_turn: position.combat.turn.turn_count,
                plan_index: plan,
                source: None,
                actions: chosen
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect(),
                end_exact_state_hash: end_hash,
            });
            save_session(&session, &play)?;
            show(&session, &play, &chosen.successor, limit)
        }
        Command::Back { session } => {
            let mut play = load_session(&session)?;
            let Some(turn) = play.turns.pop() else {
                return Err("play session is already at its exact root".to_owned());
            };
            save_session(&session, &play)?;
            let position = replay_session(&play)?;
            println!(
                "Undid turn {} plan {}. Now at turn {} ({}).",
                turn.from_turn,
                turn.plan_index,
                position.combat.turn.turn_count,
                short_hash(&exact_hash(&position))
            );
            Ok(())
        }
        Command::Reset { session } => {
            let mut play = load_session(&session)?;
            let removed = play.turns.len();
            play.turns.clear();
            save_session(&session, &play)?;
            println!("Reset to the exact case root; removed {removed} chosen turns.");
            Ok(())
        }
        Command::History { session } => {
            let play = load_session(&session)?;
            print_history(&play)
        }
        Command::Storyboard {
            session,
            json_output,
        } => {
            let play = load_session(&session)?;
            print_storyboard(&play, json_output.as_deref())
        }
        Command::AgendaCorridor {
            session,
            depth,
            max_states,
        } => {
            let play = load_session(&session)?;
            inspect_agenda_corridor(&play, depth, max_states)
        }
        Command::ResidentCorridorAudit {
            session,
            from_turn_index,
            generation_work,
            wall_ms,
            satisfaction,
            output,
        } => {
            let play = load_session(&session)?;
            let audit = run_resident_corridor_audit_v1(
                &play,
                ResidentCorridorAuditOptionsV1 {
                    from_turn_index,
                    generation_work,
                    wall_ms,
                    satisfaction,
                },
            )?;
            save_resident_corridor_audit_v1(&output, &audit)?;
            print!(
                "{}",
                sts_oracle_report::render_resident_corridor_audit_v1(&audit)
            );
            println!(
                "Saved typed resident corridor audit to {}.",
                output.display()
            );
            Ok(())
        }
        Command::AgendaValidate {
            session,
            max_agenda_turns,
            resident_generation_work,
            resident_wall_ms,
            proposal_beam_width,
            proposal_beam_boundaries,
            proposal_beam_wall_ms,
            atomic_nodes,
            atomic_wall_ms,
            winning_actions,
            proposal_beam_sessions,
        } => {
            let play = load_session(&session)?;
            validate_agenda_with_exact_search(
                &play,
                max_agenda_turns,
                resident_generation_work,
                resident_wall_ms,
                proposal_beam_width,
                proposal_beam_boundaries,
                proposal_beam_wall_ms,
                atomic_nodes,
                atomic_wall_ms,
                winning_actions.as_deref(),
                proposal_beam_sessions.as_deref(),
            )
        }
        Command::ExportWinningActions { session, output } => {
            let play = load_session(&session)?;
            export_winning_actions(&play, &output)
        }
        Command::ExportWinningDemonstration { session, output } => {
            let play = load_session(&session)?;
            export_winning_demonstration(&play, &output)
        }
    }
}

fn show(
    session_path: &Path,
    session: &PlaySessionV1,
    position: &CombatPosition,
    limit: usize,
) -> Result<(), String> {
    println!(
        "Exact combat play | {} | chosen turns {} | potions {}",
        session_path.display(),
        session.turns.len(),
        if session.allow_potions {
            "allowed"
        } else {
            "disabled"
        }
    );
    println!(
        "Proposal limits | inner {} | partial {} | end {} | per-bucket {}",
        session.proposal_limits.max_inner_nodes,
        session.proposal_limits.max_partial_frontier_states,
        session.proposal_limits.max_end_states,
        session.proposal_limits.per_bucket_limit,
    );
    print_position(position);
    print_mature_combat_plan(position);
    match combat_terminal(&position.engine, &position.combat) {
        CombatTerminal::Win => {
            println!(
                "\nWIN — final HP {}.",
                position.combat.entities.player.current_hp
            );
            return Ok(());
        }
        CombatTerminal::Loss => {
            println!("\nLOSS.");
            return Ok(());
        }
        CombatTerminal::Unresolved => ensure_player_turn(position)?,
    }

    let surface = session_turn_surface(position, session);
    let candidates = compact_proposals(&surface, limit);
    let non_loss_count = surface
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.boundary
                != sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1::TerminalLoss
        })
        .count();
    let unique_count = unique_non_loss_proposal_count(&surface);
    println!(
        "\nPlans — showing {} of {} unique exact successors ({} selected non-loss plans):",
        candidates.len(),
        unique_count,
        non_loss_count
    );
    for (plan_index, candidate) in candidates {
        print_proposal(position, plan_index, candidate)?;
    }
    if unique_count > limit {
        println!("  … use --limit {unique_count} to inspect the complete selected surface");
    }
    Ok(())
}

fn print_mature_combat_plan(position: &CombatPosition) {
    let Some(plan) = combat_plan_projection_v1(position) else {
        return;
    };
    let beam = match plan.envelope.forced_attack_turns_remaining {
        Some(0) => "visible now".to_owned(),
        Some(turns) => format!("in {turns} end-turn transitions"),
        None if plan.envelope.forced_attack_recovery_window => "recovery window".to_owned(),
        None => "not projected".to_owned(),
    };
    let focus = plan
        .envelope
        .priority_target_hp_with_block
        .map(|hp| hp.to_string())
        .unwrap_or_else(|| "none".to_owned());
    println!(
        "Mature combat plan: {:?} / {:?} | forced attack {} | block sources hand {} / accessible {} / withheld {} | focus HP+block {}",
        plan.plan,
        plan.stage,
        beam,
        plan.resources.remaining_hand_block_sources,
        plan.resources.remaining_accessible_block_sources,
        plan.resources.recoverable_withheld_block_sources,
        focus,
    );
}

fn print_position(position: &CombatPosition) {
    let combat = &position.combat;
    let player = &combat.entities.player;
    println!(
        "Turn {} | HP {}/{} | block {} | energy {} | state {}",
        combat.turn.turn_count,
        player.current_hp,
        player.max_hp,
        player.block,
        combat.turn.energy,
        short_hash(&exact_hash(position))
    );
    let powers = power_labels(combat, player.id);
    if !powers.is_empty() {
        println!("Player powers: {}", powers.join(", "));
    }
    let potions = combat
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            potion
                .as_ref()
                .map(|potion| format!("[{slot}] {}", gameplay_display_label_v1(potion.id)))
        })
        .collect::<Vec<_>>();
    println!(
        "Potions: {}",
        if potions.is_empty() {
            "none".to_owned()
        } else {
            potions.join(" | ")
        }
    );
    println!("Enemies:");
    for monster in &combat.entities.monsters {
        println!("  {}", monster_label(combat, monster));
    }
    let hand = combat
        .zones
        .hand
        .iter()
        .enumerate()
        .map(|(index, card)| format!("[{index}] {}", card_label(card)))
        .collect::<Vec<_>>()
        .join(" | ");
    println!("Hand: {hand}");
    println!(
        "Piles: draw {} | discard {} | exhaust {}",
        combat.zones.draw_pile.len(),
        combat.zones.discard_pile.len(),
        combat.zones.exhaust_pile.len()
    );
}

fn print_proposal(
    root: &CombatPosition,
    plan_index: usize,
    candidate: &ExactCompleteTurnProposalV1,
) -> Result<(), String> {
    let inputs = candidate
        .actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let labels = named_action_line(root, &inputs)?;
    let terminal = combat_terminal(&candidate.successor.engine, &candidate.successor.combat);
    let player = &candidate.successor.combat.entities.player;
    let visible_incoming = visible_incoming_damage(&candidate.successor.combat);
    let visible_loss = visible_incoming.saturating_sub(player.block).max(0);
    println!("  [{plan_index:>2}] {labels}");
    if terminal == CombatTerminal::Win {
        println!(
            "       WIN | HP {} | {}",
            player.current_hp,
            short_hash(&exact_hash(&candidate.successor))
        );
    } else {
        println!(
            "       next HP {} | enemies {} / total HP {} | incoming {} (visible loss {}) | {}",
            player.current_hp,
            living_enemy_count(&candidate.successor),
            living_enemy_hp(&candidate.successor),
            visible_incoming,
            visible_loss,
            short_hash(&exact_hash(&candidate.successor))
        );
    }
    Ok(())
}

fn compact_proposals(
    surface: &ExactCompleteTurnProposalBatchV1,
    limit: usize,
) -> Vec<(usize, &ExactCompleteTurnProposalV1)> {
    let mut hashes = HashSet::new();
    surface
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.boundary
                != sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1::TerminalLoss
        })
        .filter(|(_, candidate)| hashes.insert(exact_hash(&candidate.successor)))
        .take(limit)
        .collect()
}

fn unique_non_loss_proposal_count(surface: &ExactCompleteTurnProposalBatchV1) -> usize {
    surface
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.boundary
                != sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1::TerminalLoss
        })
        .map(|candidate| exact_hash(&candidate.successor))
        .collect::<HashSet<_>>()
        .len()
}

fn action_label(position: &CombatPosition, input: &ClientInput) -> String {
    match input {
        ClientInput::PlayCard { card_index, target } => position
            .combat
            .zones
            .hand
            .get(*card_index)
            .map(|card| match target_label(&position.combat, *target) {
                Some(target) => format!("{}({target})", card_label(card)),
                None => card_label(card),
            })
            .unwrap_or_else(|| format!("{input:?}")),
        ClientInput::UsePotion {
            potion_index,
            target,
        } => {
            let potion = position
                .combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .map(|potion| gameplay_display_label_v1(potion.id).to_string())
                .unwrap_or_else(|| format!("potion[{potion_index}]"));
            match target_label(&position.combat, *target) {
                Some(target) => format!("{potion}({target})"),
                None => potion,
            }
        }
        ClientInput::EndTurn => "End Turn".to_owned(),
        ClientInput::SubmitSelection(_) => public_selection_label(position, input)
            .unwrap_or_else(|error| format!("Select <unavailable: {error}>")),
        other => format!("{other:?}"),
    }
}

fn public_selection_label(
    position: &CombatPosition,
    input: &ClientInput,
) -> Result<String, String> {
    let ClientInput::SubmitSelection(selection) = input else {
        return Err("input is not a card selection".to_owned());
    };
    let projection = project_public_combat_actions_v1(&position.engine, &position.combat)
        .map_err(|error| format!("public projection failed: {error}"))?;
    let PublicCombatActionChoiceV1::Selection {
        family_ordinal,
        selected_domain_ordinals,
    } = public_choice_for_exact_combat_input_v1(&projection, input)?
    else {
        return Err("exact selection resolved as an atomic public action".to_owned());
    };
    let family = projection
        .public
        .selection_families
        .get(family_ordinal)
        .ok_or_else(|| "public selection family is out of range".to_owned())?;
    if selected_domain_ordinals.is_empty() {
        return Ok("Select none".to_owned());
    }
    let public_source = family
        .source_pile
        .map(|pile| format!("{pile:?}").to_ascii_lowercase())
        .unwrap_or_else(|| format!("{:?}", selection.scope).to_ascii_lowercase());
    let selected = selected_domain_ordinals
        .into_iter()
        .map(|domain_ordinal| {
            family
                .domain
                .get(domain_ordinal)
                .map(|candidate| selection_candidate_label(candidate, &public_source))
                .ok_or_else(|| "public selection occurrence is out of range".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("Select {}", selected.join(", ")))
}

fn selection_candidate_label(
    candidate: &PublicCombatSelectionDomainCandidateV1,
    public_source: &str,
) -> String {
    match candidate {
        PublicCombatSelectionDomainCandidateV1::Card {
            ordinal,
            card_id,
            upgrades,
            ..
        } => {
            let card = match (card_id, upgrades) {
                (Some(card_id), Some(0) | None) => gameplay_display_label_v1(*card_id).to_owned(),
                (Some(card_id), Some(upgrades)) => {
                    format!("{}+{upgrades}", gameplay_display_label_v1(*card_id))
                }
                (None, _) => "UnknownCard".to_owned(),
            };
            format!("{card}@{public_source}[{ordinal}]")
        }
        PublicCombatSelectionDomainCandidateV1::Scry { index, card_id, .. } => {
            let card = card_id
                .map(|card_id| gameplay_display_label_v1(card_id).to_owned())
                .unwrap_or_else(|| "UnknownCard".to_owned());
            format!("{card}@scry[{index}]")
        }
    }
}

fn print_history(session: &PlaySessionV1) -> Result<(), String> {
    let root = load_case_root(&session.case)?;
    if exact_hash(&root) != session.root_exact_state_hash {
        return Err(
            "combat case root identity changed since this play session was created".to_owned(),
        );
    }
    println!("Case: {}", session.case.display());
    println!("Root: {}", session.root_exact_state_hash);
    if session.turns.is_empty() {
        println!("No turns chosen.");
        return Ok(());
    }
    let stepper = EngineCombatStepper;
    let mut position = root;
    for turn in &session.turns {
        let mut labels = Vec::with_capacity(turn.actions.len());
        for input in &turn.actions {
            labels.push(action_label(&position, input));
            position = apply_exact(&stepper, &position, input.clone())?;
        }
        println!(
            "T{} [{}] {} -> {}",
            turn.from_turn,
            turn.plan_index,
            labels.join(" → "),
            short_hash(&turn.end_exact_state_hash)
        );
    }
    println!(
        "Reached: turn {} | {:?} | HP {}",
        position.combat.turn.turn_count,
        combat_terminal(&position.engine, &position.combat),
        position.combat.entities.player.current_hp
    );
    Ok(())
}

fn export_proposal_beam_sessions(
    source_session: &PlaySessionV1,
    root: &CombatPosition,
    directory: &Path,
    turn_depth: usize,
    routes: &[(
        Vec<u32>,
        &sts_combat_planner::CompleteTurnProposalBeamGuideRepresentativeV1,
    )],
) -> Result<(), String> {
    if directory.exists() {
        return Err(format!(
            "proposal-beam session directory '{}' already exists",
            directory.display()
        ));
    }
    if let Some(parent) = directory
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create proposal-beam session parent '{}': {error}",
                parent.display()
            )
        })?;
    }
    fs::create_dir(directory).map_err(|error| {
        format!(
            "failed to create proposal-beam session directory '{}': {error}",
            directory.display()
        )
    })?;

    for (route_index, (lanes, representative)) in routes.iter().enumerate() {
        let mut position = root.clone();
        let mut from_turn = position.combat.turn.turn_count;
        let mut turn_actions = Vec::new();
        let mut turns = Vec::new();
        for input in &representative.actions {
            turn_actions.push(input.clone());
            position = apply_exact(&EngineCombatStepper, &position, input.clone())?;
            let terminal = combat_terminal(&position.engine, &position.combat);
            let reached_boundary = terminal != CombatTerminal::Unresolved
                || (matches!(position.engine, EngineState::CombatPlayerTurn)
                    && position.combat.turn.turn_count > from_turn);
            if reached_boundary {
                turns.push(ChosenTurnV1 {
                    from_turn,
                    plan_index: route_index,
                    source: Some(format!(
                        "proposal_beam_depth_{turn_depth}_guides_{}",
                        lanes
                            .iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join("-")
                    )),
                    actions: std::mem::take(&mut turn_actions),
                    end_exact_state_hash: exact_hash(&position),
                });
                from_turn = position.combat.turn.turn_count;
            }
        }
        if !turn_actions.is_empty() {
            return Err(format!(
                "proposal-beam route {route_index} ended between replayable turn boundaries"
            ));
        }
        if exact_hash(&position) != representative.exact_state_hash {
            return Err(format!(
                "proposal-beam route {route_index} replay drifted: expected={}, reached={}",
                representative.exact_state_hash,
                exact_hash(&position)
            ));
        }
        let lane_label = lanes
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("-");
        let path = directory.join(format!(
            "route-{route_index:02}-guides-{lane_label}-hp-{}.session.json",
            representative.player_hp
        ));
        let session = PlaySessionV1 {
            schema: SESSION_SCHEMA.to_owned(),
            case: source_session.case.clone(),
            root_exact_state_hash: source_session.root_exact_state_hash.clone(),
            allow_potions: source_session.allow_potions,
            proposal_limits: source_session.proposal_limits,
            turns,
        };
        save_session(&path, &session)?;
    }
    println!(
        "  saved {} replayable proposal-beam guide sessions at {}",
        routes.len(),
        directory.display()
    );
    Ok(())
}

fn print_storyboard(session: &PlaySessionV1, json_output: Option<&Path>) -> Result<(), String> {
    let root = load_case_root(&session.case)?;
    if exact_hash(&root) != session.root_exact_state_hash {
        return Err("combat case root identity changed since this play session was created".into());
    }
    let inputs = session
        .turns
        .iter()
        .flat_map(|turn| turn.actions.iter().cloned())
        .collect::<Vec<_>>();
    let board = build_combat_witness_storyboard_v1(&root, &inputs, MAX_ENGINE_STEPS)?;

    println!(
        "Exact storyboard | {:?} | {} turns | {} actions | HP {} -> {}",
        board.terminal,
        board.turns.len(),
        board.total_actions,
        board.initial.player_hp,
        board.final_boundary.player_hp
    );
    for turn in &board.turns {
        println!(
            "\nT{} | HP {} | block {} | energy {} | incoming {} | draw/discard/exhaust {}/{}/{}",
            turn.turn,
            turn.start.player_hp,
            turn.start.player_block,
            turn.start.energy,
            turn.start.visible_incoming_damage,
            turn.start.draw_count,
            turn.start.discard_count,
            turn.start.exhaust_count
        );
        println!(
            "  enemies: {}",
            boundary_enemy_labels(&turn.start).join(" | ")
        );
        println!(
            "  hand: {}",
            turn.start
                .hand
                .iter()
                .map(|card| format!(
                    "{}{}",
                    card.card_name,
                    if card.upgrades > 0 { "+" } else { "" }
                ))
                .collect::<Vec<_>>()
                .join(" | ")
        );
        println!(
            "  actions: {}",
            turn.actions
                .iter()
                .map(|action| action.label.as_str())
                .collect::<Vec<_>>()
                .join(" -> ")
        );
        println!(
            "  result: HP {} | enemy HP delta {} | player HP delta {} | draw/discard/exhaust {}/{}/{}",
            turn.end.player_hp,
            turn.observed_enemy_hp_delta,
            turn.observed_player_hp_delta,
            turn.end.draw_count,
            turn.end.discard_count,
            turn.end.exhaust_count
        );
    }

    if let Some(output) = json_output {
        write_fresh_json(output, &board)?;
        println!("\nSaved typed storyboard to {}.", output.display());
    }
    Ok(())
}

fn inspect_agenda_corridor(
    session: &PlaySessionV1,
    depth: usize,
    max_states: usize,
) -> Result<(), String> {
    if depth == 0 || max_states == 0 {
        return Err("agenda corridor requires positive depth and max-states".into());
    }
    let root = load_case_root(&session.case)?;
    if exact_hash(&root) != session.root_exact_state_hash {
        return Err("combat case root identity changed since this play session was created".into());
    }
    let watched_hashes = watched_successor_hashes(session, depth);
    let root_plan = public_combat_plan_v1(&public_combat_state_v1(&root.combat));
    let mut current = vec![PlannedPosition {
        position: root,
        plan: root_plan,
    }];
    let mut watched_corridor_alive = true;
    println!("Public agenda corridor | depth {depth} | guard {max_states} exact states");
    for layer in 0..depth {
        let parent_count = current.len();
        let mut expanded_turn_plans = 0_usize;
        let mut carried_turn_plans = 0_usize;
        let mut agenda_frontier_turn_plans = 0_usize;
        let mut carried_option_satisfaction = std::collections::BTreeMap::<String, usize>::new();
        let mut retained_plan_labels = Vec::<String>::new();
        let mut next_by_identity =
            std::collections::BTreeMap::<(String, PublicCombatPlanStateV1), PlannedPosition>::new();
        let mut parent_stages = std::collections::BTreeMap::<String, usize>::new();
        for planned in &current {
            let position = &planned.position;
            *parent_stages
                .entry(format!("{:?}", planned.plan.stage))
                .or_default() += 1;
            if combat_terminal(&position.engine, &position.combat) != CombatTerminal::Unresolved {
                next_by_identity
                    .entry((exact_hash(position), planned.plan.clone()))
                    .or_insert_with(|| planned.clone());
                continue;
            }
            ensure_player_turn(position)?;
            let surface = enumerate_legacy_turn(position, session);
            expanded_turn_plans = expanded_turn_plans.saturating_add(surface.candidates.len());
            let root_public = public_combat_state_v1(&position.combat);
            let agenda = public_combat_agenda_for_plan_v1(&root_public, &planned.plan);
            let groundable = surface
                .candidates
                .into_iter()
                .map(|candidate| GroundableCompleteTurnV1 {
                    plan_index: candidate.report.plan_index,
                    actions: candidate
                        .report
                        .actions
                        .iter()
                        .map(|action| action.input.clone())
                        .collect(),
                    successor: candidate.position,
                })
                .collect();
            let grounded = ground_public_combat_agenda_v1(
                position,
                &planned.plan,
                &agenda,
                groundable,
                MAX_ENGINE_STEPS,
            )?;
            carried_turn_plans = carried_turn_plans.saturating_add(grounded.all_candidates().len());
            agenda_frontier_turn_plans = agenda_frontier_turn_plans
                .saturating_add(grounded.report().frontier_plan_indices.len());
            // Agenda dominance is diagnostic. Carry every distinct exact
            // successor until a real decision owner supplies continuation
            // semantics; do not silently turn this report into pruning.
            for grounded_turn in grounded.into_all_candidates() {
                for progress in &grounded_turn.report.option_progress {
                    if progress.satisfied {
                        *carried_option_satisfaction
                            .entry(format!("{:?}", progress.option))
                            .or_default() += 1;
                    }
                }
                if retained_plan_labels.len() < 4 {
                    retained_plan_labels.push(named_action_line(
                        position,
                        &grounded_turn.candidate.actions,
                    )?);
                }
                let successor_public =
                    public_combat_state_v1(&grounded_turn.candidate.successor.combat);
                let next_plan = advance_public_combat_plan_v1(&planned.plan, &successor_public);
                let key = (
                    exact_hash(&grounded_turn.candidate.successor),
                    next_plan.clone(),
                );
                next_by_identity.entry(key).or_insert(PlannedPosition {
                    position: grounded_turn.candidate.successor,
                    plan: next_plan,
                });
            }
        }
        if next_by_identity.len() > max_states {
            return Err(format!(
                "agenda layer {} produced {} exact frontier states, above guard {max_states}; refusing to truncate",
                layer + 1,
                next_by_identity.len()
            ));
        }
        let watched = watched_corridor_alive
            .then(|| watched_hashes.get(layer))
            .flatten();
        let retained = watched.is_some_and(|hash| {
            next_by_identity
                .keys()
                .any(|(exact_state_hash, _)| exact_state_hash == hash)
        });
        println!(
            "  layer {}: parents {} | complete-turn candidates {} | carried exact plans {} | agenda non-dominated {} | unique exact states {} | watched corridor {}",
            layer + 1,
            parent_count,
            expanded_turn_plans,
            carried_turn_plans,
            agenda_frontier_turn_plans,
            next_by_identity.len(),
            match watched {
                Some(_) if retained => "retained",
                Some(_) => "missing",
                None => "not supplied",
            }
        );
        println!("           parent stages: {parent_stages:?}");
        for (option, count) in carried_option_satisfaction {
            println!("           carried option satisfied {count}/{carried_turn_plans}: {option}");
        }
        if watched.is_some() && !retained {
            println!("           retained:");
            for line in &retained_plan_labels {
                println!("             {line}");
            }
            if let Some(turn) = session.turns.get(layer) {
                println!(
                    "           watched:  {}",
                    named_action_line(&current[0].position, &turn.actions)?
                );
            }
            watched_corridor_alive = false;
        }
        current = next_by_identity.into_values().collect();
        if current.is_empty() {
            break;
        }
        let wins = current
            .iter()
            .filter(|planned| {
                combat_terminal(&planned.position.engine, &planned.position.combat)
                    == CombatTerminal::Win
            })
            .collect::<Vec<_>>();
        let losses = current
            .iter()
            .filter(|planned| {
                combat_terminal(&planned.position.engine, &planned.position.combat)
                    == CombatTerminal::Loss
            })
            .count();
        if wins.len().saturating_add(losses) == current.len() {
            let mut win_hp = wins
                .iter()
                .map(|planned| planned.position.combat.entities.player.current_hp)
                .collect::<Vec<_>>();
            win_hp.sort_unstable_by(|left, right| right.cmp(left));
            println!(
                "  terminal frontier: wins {} at HP {:?} | losses {}",
                wins.len(),
                win_hp,
                losses
            );
            let mut loss_summaries = current
                .iter()
                .filter(|planned| {
                    combat_terminal(&planned.position.engine, &planned.position.combat)
                        == CombatTerminal::Loss
                })
                .map(|planned| {
                    let position = &planned.position;
                    let enemy_hp = position
                        .combat
                        .entities
                        .monsters
                        .iter()
                        .filter(|monster| monster.is_alive_for_action())
                        .map(|monster| monster.current_hp.max(0))
                        .sum::<i32>();
                    (
                        position.combat.turn.turn_count,
                        position.combat.entities.player.current_hp,
                        enemy_hp,
                    )
                })
                .collect::<Vec<_>>();
            loss_summaries.sort_unstable_by_key(|summary| summary.2);
            for (turn, hp, enemy_hp) in loss_summaries {
                println!("           loss at turn {turn}: HP {hp}, living enemy HP {enemy_hp}");
            }
            break;
        }
    }
    println!("The watched session hashes were used only for post-expansion membership auditing.");
    Ok(())
}

fn validate_agenda_with_exact_search(
    session: &PlaySessionV1,
    max_agenda_turns: usize,
    resident_generation_work: usize,
    resident_wall_ms: u64,
    proposal_beam_width: usize,
    proposal_beam_boundaries: usize,
    proposal_beam_wall_ms: u64,
    atomic_nodes: usize,
    atomic_wall_ms: u64,
    winning_actions: Option<&Path>,
    proposal_beam_sessions: Option<&Path>,
) -> Result<(), String> {
    if max_agenda_turns == 0
        || resident_generation_work == 0
        || resident_wall_ms == 0
        || atomic_nodes == 0
        || atomic_wall_ms == 0
    {
        return Err("agenda validation requires positive turn and search limits".into());
    }
    let root = load_case_root(&session.case)?;
    if exact_hash(&root) != session.root_exact_state_hash {
        return Err("combat case root identity changed since this play session was created".into());
    }
    let current = replay_session(session)?;
    let current_public = public_combat_state_v1(&current.combat);
    let mut planned = PlannedPosition {
        position: current,
        plan: public_combat_plan_v1(&current_public),
    };
    let mut prefix = session
        .turns
        .iter()
        .flat_map(|turn| turn.actions.iter().cloned())
        .collect::<Vec<ClientInput>>();
    println!(
        "Agenda validation | agenda turns {max_agenda_turns} | resident {resident_generation_work} generation work / {resident_wall_ms} ms | atomic {atomic_nodes} nodes / {atomic_wall_ms} ms"
    );

    let proposal_beam_enabled =
        proposal_beam_width > 0 && proposal_beam_boundaries > 0 && proposal_beam_wall_ms > 0;
    if proposal_beam_sessions.is_some() && !proposal_beam_enabled {
        return Err("--proposal-beam-sessions requires an enabled proposal beam".to_owned());
    }
    if proposal_beam_enabled {
        println!(
            "  boundary: turn {} | stage {:?} | HP {} | enemy HP {} | prefix actions {} | proposal beam owns the non-dominated agenda frontier",
            planned.position.combat.turn.turn_count,
            planned.plan.stage,
            planned.position.combat.entities.player.current_hp,
            living_enemy_hp(&planned.position),
            prefix.len()
        );
    }

    for agenda_turn in 0..=if proposal_beam_enabled {
        0
    } else {
        max_agenda_turns
    } {
        if proposal_beam_enabled {
            break;
        }
        let terminal = combat_terminal(&planned.position.engine, &planned.position.combat);
        if terminal != CombatTerminal::Unresolved {
            return finish_validated_line(&root, prefix, terminal, winning_actions);
        }
        if planned.plan.stage == sts_agent::agent::task::PublicCombatPlanStageV1::ConvertToLethal {
            println!(
                "  boundary: turn {} | stage {:?} | HP {} | enemy HP {} | prefix actions {}",
                planned.position.combat.turn.turn_count,
                planned.plan.stage,
                planned.position.combat.entities.player.current_hp,
                living_enemy_hp(&planned.position),
                prefix.len()
            );
            break;
        }
        if agenda_turn == max_agenda_turns {
            return Err(format!(
                "public plan did not reach lethal conversion within {max_agenda_turns} agenda turns"
            ));
        }
        ensure_player_turn(&planned.position)?;
        let surface = enumerate_legacy_turn(&planned.position, session);
        let agenda = public_combat_agenda_for_plan_v1(
            &public_combat_state_v1(&planned.position.combat),
            &planned.plan,
        );
        let groundable = surface
            .candidates
            .into_iter()
            .map(|candidate| GroundableCompleteTurnV1 {
                plan_index: candidate.report.plan_index,
                actions: candidate
                    .report
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect(),
                successor: candidate.position,
            })
            .collect();
        let grounded = ground_public_combat_agenda_v1(
            &planned.position,
            &planned.plan,
            &agenda,
            groundable,
            MAX_ENGINE_STEPS,
        )?;
        let candidate_count = grounded.all_candidates().len();
        let frontier_count = grounded.report().frontier_plan_indices.len();
        if candidate_count != 1 {
            return Err(format!(
                "public agenda carried {} distinct exact successors at turn {} ({} marked non-dominated); agenda grounding is diagnostic and no decision owner selected one",
                candidate_count,
                planned.position.combat.turn.turn_count,
                frontier_count
            ));
        }
        let mut grounded = grounded.into_all_candidates();
        let grounded = grounded.pop().expect("grounded length checked");
        println!(
            "  agenda: turn {} | stage {:?} | {}",
            planned.position.combat.turn.turn_count,
            planned.plan.stage,
            named_action_line(&planned.position, &grounded.candidate.actions)?
        );
        prefix.extend(grounded.candidate.actions);
        let successor_public = public_combat_state_v1(&grounded.candidate.successor.combat);
        let next_plan = advance_public_combat_plan_v1(&planned.plan, &successor_public);
        planned = PlannedPosition {
            position: grounded.candidate.successor,
            plan: next_plan,
        };
    }

    let decision_root = CombatDecisionRoot::new(planned.position.clone())
        .map_err(|error| format!("invalid agenda validation boundary: {error:?}"))?;
    probe_watched_complete_turn_proposals(session, &root, &planned, &prefix)?;
    let watched_start = prefix_turn_count(&root, &prefix);
    let guide_service_bias =
        sts_combat_knowledge::darkling_survival_guide_service_bias_v1(&planned.position);
    let policy =
        combat_plan_state_guide_policy_v1(sts_combat_atomic::retained_combat_tactical_policy_v1());
    if proposal_beam_enabled {
        let beam = sts_combat_planner::search_complete_turn_proposal_beam_v1(
            planned.position.clone(),
            planned.plan.clone(),
            sts_combat_planner::CompleteTurnProposalBeamConfigV1 {
                proposal: session_proposal_config(session),
                max_total_potion_expenditures: if session.allow_potions { None } else { Some(0) },
                beam_width: proposal_beam_width,
                max_turn_depth: 32,
                max_expanded_boundaries: proposal_beam_boundaries,
                deadline: Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(proposal_beam_wall_ms),
                ),
            },
            policy.clone(),
        );
        for layer in &beam.layers {
            let watched = session
                .turns
                .get(watched_start + layer.turn_depth - 1)
                .is_some_and(|turn| {
                    layer
                        .retained_exact_state_hashes
                        .contains(&turn.end_exact_state_hash)
                });
            println!(
                "  proposal beam depth {}: parents {} | candidates {} | grounded {} | agenda frontier {} | unique {} | duplicates {} | losses {} | retained {} from {} root lines (max {}) | watched {} | guides {}",
                layer.turn_depth,
                layer.parents,
                layer.proposal_candidates,
                layer.grounded_candidates,
                layer.agenda_frontier_candidates,
                layer.unique_successors,
                layer.duplicate_successors,
                layer.terminal_losses,
                layer.retained_successors,
                layer.retained_root_lineages,
                layer.largest_root_lineage,
                if watched { "retained" } else { "absent" },
                compact_guide_envelopes(&layer.retained_guide_envelopes),
            );
        }
        if let Some(layer) = beam
            .layers
            .iter()
            .rfind(|layer| !layer.retained_guide_representatives.is_empty())
        {
            println!(
                "  final non-empty guide routes at depth {}:",
                layer.turn_depth
            );
            let mut unique_routes = Vec::<(
                Vec<u32>,
                &sts_combat_planner::CompleteTurnProposalBeamGuideRepresentativeV1,
            )>::new();
            for representative in &layer.retained_guide_representatives {
                if let Some((lanes, _)) = unique_routes
                    .iter_mut()
                    .find(|(_, existing)| existing.actions == representative.actions)
                {
                    lanes.push(representative.lane.value());
                    continue;
                }
                unique_routes.push((vec![representative.lane.value()], representative));
            }
            for (lanes, representative) in &unique_routes {
                println!(
                    "    guides {:?} | HP {} block {} | enemies {} / HP {} | stage {:?} | rank [{}] | actions {} | {}",
                    lanes,
                    representative.player_hp,
                    representative.player_block,
                    representative.living_enemies,
                    representative.total_living_enemy_hp,
                    representative.plan.stage,
                    representative
                        .rank_components
                        .iter()
                        .take(6)
                        .map(i32::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                    representative.actions.len(),
                    compact_named_action_route(
                        &planned.position,
                        &representative.actions,
                        representative.actions.len(),
                    )?,
                );
            }
            if let Some(directory) = proposal_beam_sessions {
                export_proposal_beam_sessions(
                    session,
                    &root,
                    directory,
                    layer.turn_depth,
                    &unique_routes,
                )?;
            }
        }
        println!(
            "  proposal beam: boundaries {} | proposal nodes {} | candidates {} | interruption {:?} | complete win {}",
            beam.counters.expanded_boundaries,
            beam.counters.proposal_nodes_expanded,
            beam.counters.proposal_candidates,
            beam.interruption,
            beam.win.is_some()
        );
        if let Some(win) = beam.win {
            prefix.extend(win.actions.into_iter().map(|action| action.input));
            return finish_validated_line(&root, prefix, CombatTerminal::Win, winning_actions);
        }
    }
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
                resident_generation_work,
            ),
            max_turn_depth: 32,
            satisfaction: OracleCombatWitnessSatisfaction::FirstWitness,
            require_no_unrecovered_stolen_gold: false,
            max_potions_used: if session.allow_potions { None } else { Some(0) },
        },
        policy,
    );
    resident
        .enable_public_plan_turn_proposals(PublicPlanTurnProposalConfig::default())
        .map_err(|error| format!("cannot enable public-plan turn proposals: {error:?}"))?;
    let resident_report = resident.advance(
        LocalTurnGraphWitnessQuantum {
            additional_selections: resident_generation_work,
            additional_generation_work: resident_generation_work,
            additional_engine_steps: resident_generation_work.saturating_mul(MAX_ENGINE_STEPS),
            deadline: Some(
                std::time::Instant::now() + std::time::Duration::from_millis(resident_wall_ms),
            ),
        },
        &EngineCombatStepper,
    );
    println!(
        "  turn graph: generation work {} | plan proposal transitions {} across {} roots | retained {}/{} complete turns | exact states {} | status {:?} | complete win {}",
        resident_report.counters.generation_work,
        resident_report.counters.public_plan_proposal_applied_transitions,
        resident_report.counters.public_plan_proposal_attempts,
        resident_report.counters.public_plan_proposal_retained_options,
        resident_report.counters.public_plan_proposal_completed_options,
        resident_report.counters.exact_nodes,
        resident_report.status,
        resident.witness().is_some()
    );
    for (offset, watched_successor) in session.turns.iter().skip(watched_start).take(2).enumerate()
    {
        match resident.state_snapshot_by_exact_hash(&watched_successor.end_exact_state_hash) {
            Some(snapshot) => println!(
                "  watched T{} successor: generated | depth {} | work {} | children {} | visits {}",
                watched_start + offset,
                snapshot.relative_turn_depth,
                snapshot.generation_work,
                snapshot.children,
                snapshot.visits
            ),
            None => println!(
                "  watched T{} successor: not generated by resident search",
                watched_start + offset
            ),
        }
    }
    if let Some(witness) = resident.witness() {
        prefix.extend(witness.actions.iter().map(|action| action.input.clone()));
        return finish_validated_line(&root, prefix, CombatTerminal::Win, winning_actions);
    }
    if matches!(
        resident_report.status,
        LocalTurnGraphWitnessStatus::ReplayMismatch(_)
    ) {
        return Err("resident exact witness search encountered a replay mismatch".into());
    }

    let config = CombatSearchV2Config {
        max_nodes: atomic_nodes,
        max_engine_steps_per_action: MAX_ENGINE_STEPS,
        wall_time: Some(std::time::Duration::from_millis(atomic_wall_ms)),
        satisfaction: CombatSearchV2Satisfaction::FirstCompleteWin,
        potion_policy: if session.allow_potions {
            CombatSearchV2PotionPolicy::All
        } else {
            CombatSearchV2PotionPolicy::Never
        },
        max_potions_used: if session.allow_potions { None } else { Some(0) },
        allow_potion_discard: Some(false),
        turn_plan_policy: CombatSearchV2TurnPlanPolicy::TurnBoundaryFrontierSeed,
        input_label: Some("public_agenda_lethal_boundary_validation".to_owned()),
        ..Default::default()
    };
    let report = run_combat_search_v2(&planned.position.engine, &planned.position.combat, config);
    println!(
        "  exact search: nodes {} | elapsed {} ms | complete win {} | coverage {:?}",
        report.stats.nodes_expanded,
        report.stats.elapsed_ms,
        report.outcome.complete_win_found,
        report.outcome.coverage_status
    );
    let Some(win) = report.best_win_trajectory else {
        println!("UNKNOWN — the bounded exact search found no complete winning suffix.");
        return Ok(());
    };
    prefix.extend(win.actions.into_iter().map(|action| action.input));
    finish_validated_line(&root, prefix, CombatTerminal::Win, winning_actions)
}

fn compact_guide_envelopes(
    envelopes: &[sts_combat_planner::CompleteTurnProposalBeamGuideEnvelopeV1],
) -> String {
    envelopes
        .iter()
        .map(|envelope| {
            let best = envelope
                .best_components
                .iter()
                .take(6)
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let worst = envelope
                .worst_components
                .iter()
                .take(6)
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}:[{}]/[{}]x{}",
                envelope.lane.value(),
                best,
                worst,
                envelope.members
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn probe_watched_complete_turn_proposals(
    session: &PlaySessionV1,
    root: &CombatPosition,
    planned: &PlannedPosition,
    prefix: &[ClientInput],
) -> Result<(), String> {
    let watched_start = prefix_turn_count(root, prefix);
    let mut source = planned.clone();
    let stepper = EngineCombatStepper;
    for watched in session.turns.iter().skip(watched_start).take(2) {
        if source.position.combat.turn.turn_count != watched.from_turn {
            return Err(format!(
                "watched proposal source drifted: expected turn {}, reached {}",
                watched.from_turn, source.position.combat.turn.turn_count
            ));
        }
        let batch = exact_complete_turn_proposals_v1(
            &source.position,
            ExactCompleteTurnProposalConfigV1 {
                max_engine_steps_per_action: MAX_ENGINE_STEPS,
                allow_potion_expenditure: session.allow_potions,
                max_potion_expenditures: if session.allow_potions { None } else { Some(0) },
                allow_potion_discard: false,
                ..ExactCompleteTurnProposalConfigV1::default()
            },
        );
        let target_hash = &watched.end_exact_state_hash;
        let raw_target_index = batch
            .candidates
            .iter()
            .position(|candidate| exact_hash(&candidate.successor) == *target_hash);
        let raw_count = batch.candidates.len();
        let groundable = batch
            .candidates
            .into_iter()
            .enumerate()
            .map(|(plan_index, candidate)| GroundableCompleteTurnV1 {
                plan_index,
                actions: candidate
                    .actions
                    .into_iter()
                    .map(|action| action.input)
                    .collect(),
                successor: candidate.successor,
            })
            .collect();
        let agenda = public_combat_agenda_for_plan_v1(
            &public_combat_state_v1(&source.position.combat),
            &source.plan,
        );
        let grounded = ground_public_combat_agenda_v1(
            &source.position,
            &source.plan,
            &agenda,
            groundable,
            MAX_ENGINE_STEPS,
        )?;
        let carried_target = grounded
            .all_candidates()
            .iter()
            .any(|candidate| exact_hash(&candidate.candidate.successor) == *target_hash);
        println!(
            "  direct T{} proposal: target raw {} | carried {} | carried exact {}/{} | agenda non-dominated {} | expanded {}",
            watched.from_turn,
            if raw_target_index.is_some() {
                "yes"
            } else {
                "no"
            },
            if carried_target { "yes" } else { "no" },
            grounded.all_candidates().len(),
            raw_count,
            grounded.report().frontier_plan_indices.len(),
            batch.nodes_expanded
        );
        if let Some(target_index) = raw_target_index {
            if let Some(report) = grounded
                .report()
                .candidates
                .iter()
                .find(|candidate| candidate.plan_index == target_index)
            {
                println!("    target: {}", compact_grounding_report(report));
            }
            for candidate in grounded
                .report()
                .candidates
                .iter()
                .filter(|candidate| candidate.on_non_dominated_frontier)
                .take(4)
            {
                println!("    frontier: {}", compact_grounding_report(candidate));
            }
        }

        let mut successor = source.position;
        for input in &watched.actions {
            successor = apply_exact(&stepper, &successor, input.clone())?;
        }
        let actual_hash = exact_hash(&successor);
        if actual_hash != *target_hash {
            return Err(format!(
                "watched T{} successor drifted: expected={}, reached={actual_hash}",
                watched.from_turn, target_hash
            ));
        }
        let successor_public = public_combat_state_v1(&successor.combat);
        source = PlannedPosition {
            position: successor,
            plan: advance_public_combat_plan_v1(&source.plan, &successor_public),
        };
    }
    Ok(())
}

fn compact_grounding_report(
    report: &sts_combat_option_grounder::PublicCombatAgendaGroundedCandidateV1,
) -> String {
    let progress = report
        .option_progress
        .iter()
        .map(|item| {
            format!(
                "{:?}:{:?}={}/{}",
                item.priority, item.option, item.achieved, item.required
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "plan {} | HP {} | enemies {} / HP {} | {}",
        report.plan_index, report.player_hp, report.living_enemies, report.total_enemy_hp, progress
    )
}

fn finish_validated_line(
    root: &CombatPosition,
    actions: Vec<ClientInput>,
    expected_terminal: CombatTerminal,
    winning_actions: Option<&Path>,
) -> Result<(), String> {
    let mut position = root.clone();
    for input in &actions {
        position = apply_exact(&EngineCombatStepper, &position, input.clone())?;
    }
    let terminal = combat_terminal(&position.engine, &position.combat);
    if terminal != expected_terminal {
        return Err(format!(
            "combined agenda/search line replayed to {terminal:?}, expected {expected_terminal:?}"
        ));
    }
    println!(
        "  replay: terminal {terminal:?} | final HP {} | actions {} | exact {}",
        position.combat.entities.player.current_hp,
        actions.len(),
        short_hash(&exact_hash(&position))
    );
    if terminal == CombatTerminal::Win {
        if let Some(output) = winning_actions {
            write_fresh_json(output, &actions)?;
            println!(
                "  saved replay-verified winning actions to {}",
                output.display()
            );
        }
        println!("WIN — public plan plus exact continuation reached a complete victory.");
    }
    Ok(())
}

fn living_enemy_hp(position: &CombatPosition) -> i32 {
    position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp.max(0))
        .sum()
}

fn prefix_turn_count(root: &CombatPosition, actions: &[ClientInput]) -> usize {
    let mut position = root.clone();
    let start_turn = position.combat.turn.turn_count;
    for input in actions {
        let Ok(next) = apply_exact(&EngineCombatStepper, &position, input.clone()) else {
            break;
        };
        position = next;
    }
    position.combat.turn.turn_count.saturating_sub(start_turn) as usize
}

fn named_action_line(root: &CombatPosition, actions: &[ClientInput]) -> Result<String, String> {
    let mut position = root.clone();
    let mut labels = Vec::with_capacity(actions.len());
    for input in actions {
        labels.push(action_label(&position, input));
        position = apply_exact(&EngineCombatStepper, &position, input.clone())?;
    }
    Ok(labels.join(" -> "))
}

fn compact_named_action_route(
    root: &CombatPosition,
    actions: &[ClientInput],
    edge_actions: usize,
) -> Result<String, String> {
    let mut position = root.clone();
    let mut labels = Vec::with_capacity(actions.len());
    for input in actions {
        labels.push(action_label(&position, input));
        position = apply_exact(&EngineCombatStepper, &position, input.clone())?;
    }
    if labels.len() <= edge_actions.saturating_mul(2) {
        return Ok(labels.join(" -> "));
    }
    Ok(format!(
        "{} -> … {} actions … -> {}",
        labels[..edge_actions].join(" -> "),
        labels.len().saturating_sub(edge_actions.saturating_mul(2)),
        labels[labels.len() - edge_actions..].join(" -> ")
    ))
}

fn watched_successor_hashes(session: &PlaySessionV1, depth: usize) -> Vec<String> {
    session
        .turns
        .iter()
        .take(depth)
        .map(|turn| turn.end_exact_state_hash.clone())
        .collect()
}

fn boundary_enemy_labels(boundary: &CombatWitnessStoryboardBoundaryV1) -> Vec<String> {
    boundary
        .enemies
        .iter()
        .map(|enemy| {
            let intent = match enemy.visible_total_damage {
                Some(total) => format!("{:?} {total}", enemy.visible_intent),
                None => format!("{:?}", enemy.visible_intent),
            };
            format!(
                "{} {}/{} ({intent})",
                enemy.enemy_name, enemy.hp, enemy.max_hp
            )
        })
        .collect()
}

fn write_fresh_json<T: Serialize>(output: &Path, value: &T) -> Result<(), String> {
    if output.exists() {
        return Err(format!("JSON output '{}' already exists", output.display()));
    }
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create JSON output directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let mut payload = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode storyboard JSON: {error}"))?;
    payload.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| {
            format!(
                "failed to create JSON output '{}': {error}",
                output.display()
            )
        })?;
    file.write_all(&payload)
        .and_then(|_| file.flush())
        .map_err(|error| {
            format!(
                "failed to finish JSON output '{}': {error}",
                output.display()
            )
        })
}

fn export_winning_actions(session: &PlaySessionV1, output: &Path) -> Result<(), String> {
    if output.exists() {
        return Err(format!(
            "winning-action output '{}' already exists",
            output.display()
        ));
    }
    let terminal = replay_session(session)?;
    if combat_terminal(&terminal.engine, &terminal.combat) != CombatTerminal::Win {
        return Err("play session is not an exact terminal win".to_owned());
    }
    let actions = session
        .turns
        .iter()
        .flat_map(|turn| turn.actions.iter().cloned())
        .collect::<Vec<_>>();
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create winning-action directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let mut payload = serde_json::to_vec_pretty(&actions)
        .map_err(|error| format!("failed to encode winning actions: {error}"))?;
    payload.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| {
            format!(
                "failed to create winning-action output '{}': {error}",
                output.display()
            )
        })?;
    file.write_all(&payload)
        .and_then(|_| file.flush())
        .map_err(|error| {
            format!(
                "failed to finish winning-action output '{}': {error}",
                output.display()
            )
        })?;
    println!(
        "Exported {} exact winning actions across {} turns to {}.",
        actions.len(),
        session.turns.len(),
        output.display()
    );
    Ok(())
}

fn export_winning_demonstration(session: &PlaySessionV1, output: &Path) -> Result<(), String> {
    let terminal = replay_session(session)?;
    if combat_terminal(&terminal.engine, &terminal.combat) != CombatTerminal::Win {
        return Err("play session is not an exact terminal win".to_owned());
    }

    let mut position = load_case_root(&session.case)?;
    if exact_hash(&position) != session.root_exact_state_hash {
        return Err("combat case root identity changed since this play session was created".into());
    }
    let mut decisions = Vec::with_capacity(session.turns.len());

    for (turn_index, recorded) in session.turns.iter().enumerate() {
        if position.combat.turn.turn_count != recorded.from_turn {
            return Err(format!(
                "demonstration turn {turn_index} boundary drifted before projection"
            ));
        }
        let surface = session_turn_surface(&position, session);
        let chosen_candidate_ordinal = surface
            .candidates
            .iter()
            .position(|candidate| exact_hash(&candidate.successor) == recorded.end_exact_state_hash)
            .ok_or_else(|| {
                format!(
                    "demonstration turn {turn_index} chosen successor is absent from the current complete-turn surface"
                )
            })?;

        let decision = project_complete_turn_surface_v1(&position, &surface)?;
        let candidate_evidence = (0..decision.candidates.len())
            .map(|ordinal| {
                if ordinal == chosen_candidate_ordinal {
                    CompleteTurnCandidateEvidenceV1::VerifiedPositiveDemonstration
                } else {
                    CompleteTurnCandidateEvidenceV1::Unknown
                }
            })
            .collect();
        decisions.push(CompleteTurnDemonstrationDecisionV1 {
            decision,
            candidate_evidence,
            chosen_candidate_ordinal,
        });
        position = surface.candidates[chosen_candidate_ordinal]
            .successor
            .clone();
    }

    let demonstration = CompleteTurnWinningDemonstrationV1 {
        schema: COMPLETE_TURN_WINNING_DEMONSTRATION_SCHEMA.to_owned(),
        source_case: session.case.clone(),
        source_root_exact_state_hash: session.root_exact_state_hash.clone(),
        terminal_evidence: "exact_replay_win".to_owned(),
        decisions,
    };
    demonstration.validate()?;
    write_fresh_json(output, &demonstration)?;
    println!(
        "Exported {} replay-verified complete-turn decisions to {}.",
        demonstration.decisions.len(),
        output.display()
    );
    Ok(())
}

fn import_winning_actions(
    case: &Path,
    actions_path: &Path,
    session_path: &Path,
    allow_potions: bool,
    proposal_limits: PlayProposalLimitsV1,
) -> Result<(), String> {
    if session_path.exists() {
        return Err(format!(
            "play session '{}' already exists; choose a fresh path",
            session_path.display()
        ));
    }
    let case = case.canonicalize().map_err(|error| {
        format!(
            "failed to resolve combat case '{}': {error}",
            case.display()
        )
    })?;
    let actions_path = actions_path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve winning actions '{}': {error}",
            actions_path.display()
        )
    })?;
    let actions_payload = fs::read(&actions_path).map_err(|error| {
        format!(
            "failed to read winning actions '{}': {error}",
            actions_path.display()
        )
    })?;
    let case_payload = fs::read(&case)
        .map_err(|error| format!("failed to read combat case '{}': {error}", case.display()))?;
    let winning_line =
        replay_verified_winning_complete_turn_line_v1(&case_payload, &actions_payload)?;

    let root = load_case_root(&case)?;
    ensure_player_turn(&root)?;
    let mut session = PlaySessionV1 {
        schema: SESSION_SCHEMA.to_owned(),
        case,
        root_exact_state_hash: exact_hash(&root),
        allow_potions,
        proposal_limits,
        turns: Vec::new(),
    };
    let source = format!("imported exact win from {}", actions_path.display());
    let mut turn_start = root;
    for (turn_index, verified_turn) in winning_line.turns.iter().enumerate() {
        let mut position = turn_start.clone();
        for action in &verified_turn.inputs {
            position = apply_exact(&EngineCombatStepper, &position, action.clone())?;
        }
        let end_exact_state_hash = exact_hash(&position);
        let surface = session_turn_surface(&turn_start, &session);
        let plan_index = surface
            .candidates
            .iter()
            .position(|candidate| exact_hash(&candidate.successor) == end_exact_state_hash)
            .ok_or_else(|| {
                format!(
                    "imported turn {} exact successor is absent from the configured complete-turn surface",
                    turn_index
                )
            })?;
        session.turns.push(ChosenTurnV1 {
            from_turn: verified_turn.from_turn,
            plan_index,
            source: Some(source.clone()),
            actions: verified_turn.inputs.clone(),
            end_exact_state_hash,
        });
        turn_start = position;
    }

    save_session(session_path, &session)?;
    println!(
        "Imported {} exact winning actions across {} verified complete turns into {}.",
        winning_line.action_count,
        session.turns.len(),
        session_path.display()
    );
    Ok(())
}

fn enumerate_legacy_turn(
    position: &CombatPosition,
    session: &PlaySessionV1,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    let config = CombatSearchV2Config {
        max_engine_steps_per_action: MAX_ENGINE_STEPS,
        turn_plan_probe_max_inner_nodes: Some(session.proposal_limits.max_inner_nodes),
        turn_plan_probe_max_partial_frontier_states: Some(
            session.proposal_limits.max_partial_frontier_states,
        ),
        turn_plan_probe_max_end_states: Some(session.proposal_limits.max_end_states),
        turn_plan_probe_per_bucket_limit: Some(session.proposal_limits.per_bucket_limit),
        potion_policy: if session.allow_potions {
            CombatSearchV2PotionPolicy::All
        } else {
            CombatSearchV2PotionPolicy::Never
        },
        max_potions_used: if session.allow_potions { None } else { Some(0) },
        allow_potion_discard: Some(false),
        input_label: Some("exact_combat_play".to_owned()),
        ..Default::default()
    };
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices(
        &position.engine,
        &position.combat,
        &config,
    )
}

fn session_turn_surface(
    position: &CombatPosition,
    session: &PlaySessionV1,
) -> ExactCompleteTurnProposalBatchV1 {
    complete_turn_surface_with_config_v1(position, session_proposal_config(session))
}

fn session_proposal_config(session: &PlaySessionV1) -> ExactCompleteTurnProposalConfigV1 {
    ExactCompleteTurnProposalConfigV1 {
        max_inner_nodes: session.proposal_limits.max_inner_nodes,
        max_partial_frontier_states: session.proposal_limits.max_partial_frontier_states,
        max_end_states: session.proposal_limits.max_end_states,
        per_bucket_limit: session.proposal_limits.per_bucket_limit,
        max_engine_steps_per_action: MAX_ENGINE_STEPS,
        allow_potion_expenditure: session.allow_potions,
        max_potion_expenditures: if session.allow_potions { None } else { Some(0) },
        allowed_potion_slots: None,
        allow_potion_discard: false,
    }
}

fn verify_proposal_successor(
    candidate: &ExactCompleteTurnProposalV1,
    actual_hash: &str,
) -> Result<(), String> {
    let expected = candidate
        .actions
        .last()
        .map(|action| action.expected_successor_hash.as_str());
    if expected != Some(actual_hash) {
        return Err(format!(
            "proposal exact successor drifted: report={expected:?}, actual={actual_hash}",
        ));
    }
    Ok(())
}

fn living_enemy_count(position: &CombatPosition) -> usize {
    position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count()
}

fn visible_incoming_damage(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .filter_map(|monster| project_monster_move_preview_in_combat(combat, monster).total_damage)
        .fold(0_i32, i32::saturating_add)
}

fn require_positive_limit(limit: usize) -> Result<(), String> {
    if limit == 0 {
        Err("display limit must be positive".to_owned())
    } else {
        Ok(())
    }
}

fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

fn card_label(card: &CombatCard) -> String {
    if card.upgrades == 0 {
        gameplay_display_label_v1(card.id).to_owned()
    } else {
        format!("{}+{}", gameplay_display_label_v1(card.id), card.upgrades)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::cards::CardId;
    use sts_core::state::core::{HandSelectReason, PendingChoice};
    use sts_core::state::selection::{SelectionResolution, SelectionScope};
    use sts_core::test_support::blank_test_combat;

    #[test]
    fn legacy_v1_session_uses_the_maintained_proposal_limits() {
        let session: PlaySessionV1 = serde_json::from_str(
            r#"{"schema":"exact_combat_play_session_v1","case":"case.json","root_exact_state_hash":"root","allow_potions":false,"turns":[]}"#,
        )
        .unwrap();

        assert_eq!(session.proposal_limits.max_inner_nodes, 256);
        assert_eq!(session.proposal_limits.max_partial_frontier_states, 32);
        assert_eq!(session.proposal_limits.max_end_states, 24);
        assert_eq!(session.proposal_limits.per_bucket_limit, 24);
    }

    #[test]
    fn selection_label_uses_public_card_identity_and_visible_occurrence() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 101),
            CombatCard::for_scenario(CardId::Defend, 202),
        ];
        let position = CombatPosition {
            engine: EngineState::PendingChoice(PendingChoice::HandSelect {
                candidate_lineages: sts_core::test_support::scenario_lineages([101, 202]),
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: HandSelectReason::Exhaust,
            }),
            combat,
        };
        let input = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Hand,
            [sts_core::test_support::scenario_lineage(202)],
        ));

        assert_eq!(action_label(&position, &input), "Select Defend@hand[1]");
    }
}

fn target_label(combat: &CombatState, target: Option<usize>) -> Option<String> {
    let target = target?;
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target)
        .map(|monster| {
            let name = monster.enemy_id.get_name();
            format!("{name}[{}]", monster.slot)
        })
        .or_else(|| Some(format!("entity#{target}")))
}

fn monster_label(combat: &CombatState, monster: &MonsterEntity) -> String {
    let name = monster.enemy_id.get_name();
    if monster.is_escaped {
        return format!("{name}[{}] escaped", monster.slot);
    }
    if !monster.is_alive_for_action() {
        return format!("{name}[{}] dead", monster.slot);
    }
    let intent = monster
        .move_state
        .planned_visible_spec
        .as_ref()
        .map(|intent| format!("{intent:?}"))
        .unwrap_or_else(|| format!("move:{}", monster.planned_move_id()));
    let powers = power_labels(combat, monster.id);
    let powers = if powers.is_empty() {
        String::new()
    } else {
        format!(" | {}", powers.join(", "))
    };
    format!(
        "{name}[{}] HP {}/{} | block {} | {intent}{powers}",
        monster.slot, monster.current_hp, monster.max_hp, monster.block
    )
}

fn power_labels(combat: &CombatState, entity: usize) -> Vec<String> {
    store::powers_for(combat, entity)
        .unwrap_or_default()
        .iter()
        .map(|power| match &power.payload {
            sts_core::runtime::combat::PowerPayload::SourceEntity(source) => format!(
                "{}:{} [source {source}]",
                gameplay_display_label_v1(power.power_type),
                power.amount
            ),
            sts_core::runtime::combat::PowerPayload::None => {
                let stasis_card = (power.power_type == sts_core::content::powers::PowerId::Stasis)
                    .then(|| {
                        let held_lineage =
                            sts_core::content::powers::core::stasis::held_card_lineage(
                                power.extra_data,
                            )?;
                        combat
                            .zones
                            .limbo
                            .iter()
                            .find(|card| card.lineage == held_lineage)
                    })
                    .flatten();
                match stasis_card {
                    Some(card) => format!(
                        "{:?}:{} [{}]",
                        power.power_type,
                        power.amount,
                        card_label(card)
                    ),
                    None => format!(
                        "{}:{}",
                        gameplay_display_label_v1(power.power_type),
                        power.amount
                    ),
                }
            }
            sts_core::runtime::combat::PowerPayload::Card(card) => format!(
                "{:?}:{} [{}]",
                power.power_type,
                power.amount,
                card_label(card)
            ),
        })
        .collect()
}
