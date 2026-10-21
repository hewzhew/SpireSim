//! Heavy offline and exact-search command frontend for the dedicated oracle runtime.

mod action_boundary_evidence;
mod action_boundary_followup;
mod action_boundary_policy;
mod action_boundary_root_race;
mod action_reanalysis_policy;
mod action_reanalysis_queue;
mod action_successor_reanalysis;
mod boundary_successor_corpus;
mod bounded_continuation_audit;
mod canonical_launch;
mod combat_case_contract;
mod combat_case_loadout_counterfactual;
mod combat_case_local_graph;
mod combat_case_owner_parity;
mod combat_case_performance;
mod combat_evidence_audit;
mod combat_evidence_export;
mod combat_evidence_manifest;
mod combat_graph_diagnostics;
mod combat_graph_execution;
mod combat_graph_exports;
mod combat_graph_observation;
mod combat_graph_report;
mod combat_graph_search_spec;
mod combat_plan_diagnostics;
mod combat_planning_view;
mod combat_policy_controls;
mod combat_replay_tools;
mod combat_route_compare;
mod combat_scratch_cli;
mod combat_trace_view;
mod depth_beam_audits;
mod exact_combat_evidence;
mod exact_turn_corridor;
mod guidance_artifact_commands;
mod guidance_combination_audit;
mod historical_combat_export;
mod learning_root_commands;
mod oracle_budget_cli;
mod oracle_case_catalog_v2;
mod oracle_cli;
mod oracle_contract_v2;
mod oracle_seed_panel;
mod policy_discrepancy_search;
mod potion_expenditure_audit;
mod run_witness_commands;
mod run_witness_suite;
mod turn_audits;
mod turn_membership_audit;
mod turn_quality_corridor;
mod workspace_combat_scratch;
mod workspace_commands;
mod workspace_drive;
mod workspace_owner_commands;
mod workspace_policy_audits;
mod workspace_storage_commands;
mod workspace_view;

use canonical_launch::{
    runtime_identity as oracle_lab_runtime_identity, source_content_fingerprint,
};

use oracle_cli::Command;
use serde::Serialize;
use sts_oracle_runtime::eval::combat_guidance_bundle::CombatGuidanceBundleV1;
use sts_oracle_runtime::runtime::branch::{
    load_oracle_run_continuation_v1, save_oracle_analysis_workspace_v1, OracleAnalysisWorkspaceV1,
    OracleRunConfig,
};

fn main() -> Result<(), String> {
    let (canonical_oracle, command) = oracle_cli::parse();
    canonical_launch::validate(canonical_oracle)?;
    match command {
        Command::Contract(args) => oracle_contract_v2::run_contract_command(args),
        Command::Artifact(args) => oracle_contract_v2::run_artifact_command(args),
        Command::Case(args) => oracle_case_catalog_v2::run(args),
        Command::SeedPanel(args) => print_json(&oracle_seed_panel::run(args)?),
        Command::New {
            seed,
            ascension,
            workspace,
            combat_guidance_bundle,
            budget,
        } => {
            let guidance = combat_guidance_bundle
                .as_deref()
                .map(CombatGuidanceBundleV1::load)
                .transpose()?;
            let analysis = OracleAnalysisWorkspaceV1::new_with_combat_guidance(
                OracleRunConfig {
                    seed,
                    ascension,
                    budget: budget.into_budget(),
                },
                guidance,
            )?;
            let view = analysis.view()?;
            save_oracle_analysis_workspace_v1(&workspace, &analysis)?;
            print_json(&view)
        }
        Command::Import {
            continuation,
            workspace,
            branch_id,
            combat_guidance_bundle,
            budget,
        } => {
            let continuation = load_oracle_run_continuation_v1(&continuation)?;
            let guidance = combat_guidance_bundle
                .as_deref()
                .map(CombatGuidanceBundleV1::load)
                .transpose()?;
            let config = OracleRunConfig {
                seed: continuation.seed,
                ascension: continuation.ascension,
                budget: budget.into_budget(),
            };
            let analysis = match branch_id {
                Some(branch_id) => {
                    OracleAnalysisWorkspaceV1::from_continuation_branch_with_combat_guidance(
                        config,
                        continuation,
                        branch_id,
                        guidance,
                    )?
                }
                None => OracleAnalysisWorkspaceV1::from_continuation_with_combat_guidance(
                    config,
                    continuation,
                    guidance,
                )?,
            };
            let view = analysis.view()?;
            save_oracle_analysis_workspace_v1(&workspace, &analysis)?;
            print_json(&view)
        }
        Command::ExportContinuation {
            workspace,
            node,
            output,
        } => print_json(&run_witness_commands::export_continuation(
            &workspace, node, &output,
        )?),
        Command::LearningRoot(command) => learning_root_commands::run(command),
        Command::CompactWorkspace {
            workspace,
            node,
            output,
        } => print_json(&workspace_storage_commands::compact_workspace(
            &workspace, node, &output,
        )?),
        Command::RepackWorkspace { workspace, output } => print_json(
            &workspace_storage_commands::repack_workspace(&workspace, &output)?,
        ),
        Command::RecoverCombatCase {
            workspace,
            branch,
            output,
        } => print_json(&run_witness_commands::recover_combat_case(
            &workspace, branch, &output,
        )?),
        Command::VerifyRunWitness { workspace, node } => {
            print_json(&run_witness_commands::verify(&workspace, node)?)
        }
        Command::ExportRunWitnessPrefix {
            workspace,
            node,
            journal_entry,
            output,
        } => print_json(&run_witness_commands::export_prefix(
            &workspace,
            node,
            journal_entry,
            &output,
        )?),
        Command::VerifyRunWitnessSuite { args } => {
            print_json(&run_witness_suite::verify_run_witness_suite(args)?)
        }
        Command::SpliceCombatWitness {
            workspace,
            node,
            journal_entry,
            replacement_workspace,
            replacement_node,
            output,
        } => print_json(&run_witness_commands::splice_combat(
            &workspace,
            node,
            journal_entry,
            &replacement_workspace,
            replacement_node,
            &output,
        )?),
        Command::ExportHistoricalCombatWitness {
            workspace,
            node,
            journal_entry,
            case_output,
            actions_output,
            continuation_output,
        } => print_json(&historical_combat_export::export_historical_combat(
            &workspace,
            node,
            journal_entry,
            &case_output,
            &actions_output,
            continuation_output.as_deref(),
        )?),
        Command::BuildValuePrototype {
            case,
            actions,
            output,
            max_engine_steps_per_transition,
        } => print_json(&guidance_artifact_commands::build_value_prototype(
            &case,
            &actions,
            &output,
            max_engine_steps_per_transition,
        )?),
        Command::BuildValuePrototypeCorpus {
            manifest,
            output,
            max_engine_steps_per_transition,
        } => print_json(&guidance_artifact_commands::build_value_prototype_corpus(
            &manifest,
            &output,
            max_engine_steps_per_transition,
        )?),
        Command::BuildCombatGuidanceBundle {
            action_imitation_artifact,
            value_prototype_artifact,
            output,
        } => print_json(&guidance_artifact_commands::build_guidance_bundle(
            &action_imitation_artifact,
            &value_prototype_artifact,
            &output,
        )?),
        Command::BuildActionImitation {
            case,
            actions,
            output,
            max_engine_steps_per_transition,
        } => print_json(&guidance_artifact_commands::build_action_imitation(
            &case,
            &actions,
            &output,
            max_engine_steps_per_transition,
        )?),
        Command::BuildActionImitationCorpus {
            manifest,
            output,
            residual_over_existing_policy,
            max_engine_steps_per_transition,
        } => print_json(&guidance_artifact_commands::build_action_imitation_corpus(
            &manifest,
            &output,
            residual_over_existing_policy,
            max_engine_steps_per_transition,
        )?),
        Command::BuildActionSuccessorCorpus { args } => {
            let report = action_successor_reanalysis::build(args)?;
            print_json(&report)
        }
        Command::BuildActionBoundaryEvidence { args } => {
            print_json(&action_boundary_evidence::build(args)?)
        }
        Command::BuildActionBoundaryEvidenceBatch { args } => {
            print_json(&action_boundary_evidence::build_batch(args)?)
        }
        Command::BuildActionBoundaryPolicy { args } => {
            print_json(&action_boundary_policy::build(args)?)
        }
        Command::AuditActionBoundaryRootRace { args } => {
            print_json(&action_boundary_root_race::run(args)?)
        }
        Command::BuildActionReanalysisPolicy { args } => {
            let report = action_reanalysis_policy::build(args)?;
            print_json(&report)
        }
        Command::BuildActionReanalysisQueue { args } => {
            let report = action_reanalysis_queue::build(args)?;
            print_json(&report)
        }
        Command::BuildActionReanalysisBatch { args } => {
            let report = action_reanalysis_queue::build_batch(args)?;
            print_json(&report)
        }
        Command::AuditActionImitation {
            case,
            actions,
            artifact,
            max_engine_steps_per_transition,
        } => print_json(&guidance_artifact_commands::audit_action_imitation(
            &case,
            &actions,
            &artifact,
            max_engine_steps_per_transition,
        )?),
        Command::BuildBoundarySuccessorCorpus { args } => {
            let summary = boundary_successor_corpus::build(args)?;
            print_json(&summary)
        }
        Command::AuditGuidanceCombination(args) => {
            print_json(&guidance_combination_audit::run(args)?)
        }
        Command::CombatCaseLocalGraph(args) => combat_case_local_graph::run(args),
        Command::CombatCaseOwnerParity(args) => print_json(&combat_case_owner_parity::run(args)?),
        Command::CombatCasePlanAnnotations(args) => combat_plan_diagnostics::run_annotations(args),
        Command::CombatCasePlanTrace(args) => combat_plan_diagnostics::run_trace(args),
        Command::CombatCaseRouteCompare(args) => print_json(&combat_route_compare::run(args)?),
        Command::CombatCaseLoadoutCounterfactual(args) => {
            print_json(&combat_case_loadout_counterfactual::run(args)?)
        }
        Command::CombatEvidenceAudit(args) => print_json(&combat_evidence_audit::run(args)?),
        Command::CombatCasePolicyDiscrepancy(args) => policy_discrepancy_search::run(args),
        Command::CombatCasePotionExpenditureAudit(args) => {
            print_json(&potion_expenditure_audit::run(args)?)
        }
        Command::TurnActionAudit(args) => turn_audits::run_action(args),
        Command::TurnPlanAudit(args) => turn_audits::run_plan(args),
        Command::TurnQualityCorridor(args) => print_json(&turn_quality_corridor::run(args)?),
        Command::TurnQualityFrontier(args) => {
            print_json(&turn_quality_corridor::inspect_frontier(args)?)
        }
        Command::DepthBeamTurnAudit(args) => depth_beam_audits::run_turn(args),
        Command::TurnMembership(args) => turn_membership_audit::run(args),
        Command::View { workspace, node } => {
            print_json(&workspace_commands::view(&workspace, node)?)
        }
        Command::RoutePolicyAudit(args) => {
            print_json(&workspace_policy_audits::route(&args.workspace, args.node)?)
        }
        Command::ShopPolicyAudit(args) => {
            print_json(&workspace_policy_audits::shop(&args.workspace, args.node)?)
        }
        Command::CardRewardPath(args) => print_json(&workspace_policy_audits::card_reward_path(
            &args.workspace,
            args.node,
            args.output.as_deref(),
        )?),
        Command::Status {
            workspace,
            node,
            limit,
        } => print_json(&workspace_commands::status(&workspace, node, limit)?),
        Command::Choose {
            workspace,
            owner_rank,
            node,
        } => print_json(&workspace_owner_commands::choose(
            &workspace, owner_rank, node,
        )?),
        Command::Owner { workspace, steps } => {
            print_json(&workspace_owner_commands::owner(&workspace, steps)?)
        }
        Command::Drive {
            workspace,
            output,
            max_steps,
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            stop_at,
        } => print_json(&workspace_drive::drive(
            &workspace,
            output.as_deref(),
            max_steps,
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            stop_at.map(Into::into),
        )?),
        Command::Timeline {
            workspace,
            node,
            tail,
        } => print_json(&workspace_commands::timeline(&workspace, node, tail)?),
        Command::ExportCombatCase {
            workspace,
            node,
            output,
        } => print_json(&workspace_commands::export_combat_case(
            &workspace, node, &output,
        )?),
        Command::Combat {
            workspace,
            node,
            max_engine_steps_per_transition,
        } => print_json(&workspace_commands::combat(
            &workspace,
            node,
            max_engine_steps_per_transition,
        )?),
        Command::CombatScratch { workspace, command } => {
            print_json(&workspace_combat_scratch::execute(&workspace, command)?)
        }
        Command::Tree { workspace } => print_json(&workspace_commands::tree(&workspace)?),
        Command::Try {
            workspace,
            choice_ref,
        } => print_json(&workspace_commands::try_choice(&workspace, &choice_ref)?),
        Command::Focus { workspace, node } => {
            print_json(&workspace_commands::focus(&workspace, node)?)
        }
        Command::Follow { workspace, edge } => {
            print_json(&workspace_commands::follow(&workspace, edge)?)
        }
        Command::Back { workspace } => print_json(&workspace_commands::back(&workspace)?),
        Command::Promote { workspace } => print_json(&workspace_commands::promote(&workspace)?),
        Command::Advance {
            workspace,
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            improve_incumbent,
            detailed,
        } => print_json(&workspace_commands::advance(
            &workspace,
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            improve_incumbent,
            detailed,
        )?),
        Command::ProbeCombat {
            workspace,
            generation_work,
            quantum_generation_work,
            wall_ms,
            detailed,
        } => print_json(&workspace_commands::probe_combat(
            &workspace,
            generation_work,
            quantum_generation_work,
            wall_ms,
            detailed,
        )?),
        Command::AcceptCombat { workspace } => {
            print_json(&workspace_commands::accept_combat(&workspace)?)
        }
        Command::AcceptCombatActions { workspace, actions } => print_json(
            &workspace_commands::accept_combat_actions(&workspace, &actions)?,
        ),
        Command::RestartCombat { workspace } => {
            print_json(&workspace_commands::restart_combat(&workspace)?)
        }
        Command::History {
            workspace,
            node,
            journal,
        } => print_json(&workspace_commands::history(&workspace, node, journal)?),
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string(value)
            .map_err(|error| format!("failed to serialize oracle_lab output: {error}"))?
    );
    Ok(())
}
