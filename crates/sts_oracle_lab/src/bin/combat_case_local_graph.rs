use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use clap::Args;
use serde_json::Value;
use sts_combat_planner::{
    CombatDecisionRoot, OracleCombatWitnessSatisfaction, PublicPlanTurnProposalConfig,
};
use sts_complete_turn_proposal::{
    exact_complete_turn_proposals_v1, ExactCompleteTurnProposalBoundaryV1,
    ExactCompleteTurnProposalConfigV1,
};
use sts_complete_turn_view::{complete_turn_candidate_id_v1, project_complete_turn_candidate_v1};
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::combat_guidance_bundle::{
    combat_value_prototype_policy_v1, CombatGuidanceBundleV1,
};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{CombatPosition, EngineCombatStepper};

use super::combat_case_contract::{evaluate_local_graph_contract, LocalGraphContractRequest};
use super::combat_case_performance;
use super::combat_graph_diagnostics::{
    materialize_local_graph_diagnostics, LocalGraphDiagnosticPaths,
};
use super::combat_graph_execution::LocalGraphExecutionProfile;
use super::combat_graph_exports::{
    export_local_graph_paths, LocalGraphExportActions, LocalGraphExportPaths,
};
use super::combat_graph_observation::capture_local_graph_observation;
use super::combat_graph_report::{
    local_graph_full_report, local_graph_trace_report, LocalGraphCounterfactual,
    LocalGraphFullReportOptions, LocalGraphReportData, LocalGraphRootTurnContinuationV1,
    LocalGraphRunIdentity,
};
use super::combat_graph_search_spec::{LocalGraphGuideServiceBiasSpec, LocalGraphSearchSpec};
use super::combat_planning_view::combat_plan_transition_portfolio_v1;
use super::combat_policy_controls::load_action_imitation_policy;
use super::exact_turn_corridor::load as load_exact_turn_corridor;
use super::guidance_artifact_commands::load_value_prototype;
use super::print_json;
use super::turn_audits::potion_slot_mask;

#[derive(Debug, Args)]
pub(super) struct CombatCaseLocalGraphArgs {
    #[arg(long)]
    case: PathBuf,
    /// Replay one content-addressed complete-turn candidate from the case
    /// root, then search only from that exact successor. This conditions the
    /// continuation without claiming that budget-missing alternatives lose.
    #[arg(
        long,
        conflicts_with = "full_health",
        conflicts_with = "watch_corridor_actions",
        conflicts_with = "contract_only",
        conflicts_with = "performance_only",
        conflicts_with = "export_witness_actions",
        conflicts_with = "export_deepest_survival_case",
        conflicts_with = "export_deepest_progress_case"
    )]
    root_turn_candidate_id: Option<String>,
    /// Diagnostic control: preserve action-policy weights while removing
    /// every boundary and mid-turn state guide.
    #[arg(long, conflicts_with = "root_turn_anchor_only")]
    anchor_only: bool,
    /// Diagnostic control: use only action-policy anchor service during
    /// the root player turn, then restore all guides at later turns.
    #[arg(long, conflicts_with = "anchor_only")]
    root_turn_anchor_only: bool,
    /// Optional typed action-order policy distilled from exact witnesses.
    /// It changes guidance only; legality and terminal truth stay exact.
    #[arg(long)]
    action_imitation_artifact: Option<PathBuf>,
    /// Optional lab-only turn-boundary value prototypes distilled from an
    /// exact witness. This is a teacher upper-bound control, not production.
    #[arg(long)]
    value_prototype_artifact: Option<PathBuf>,
    /// One immutable, compatibility-checked package containing both the
    /// typed action residual and cross-turn value prototypes.
    #[arg(
        long,
        conflicts_with = "action_imitation_artifact",
        conflicts_with = "value_prototype_artifact"
    )]
    guidance_bundle: Option<PathBuf>,
    /// Replay one verified witness and observe each exact player-turn
    /// boundary without changing policy, guides, or search order.
    #[arg(long)]
    watch_corridor_actions: Vec<PathBuf>,
    /// Attach encounter-owned, typed plan facts to newly materialized
    /// exact turn-boundary edges. Diagnostic only: annotations are not
    /// read by policy, scheduling, pruning, or witness authority.
    #[arg(long)]
    plan_transition_annotations: bool,
    /// Opt-in lab control: add the encounter-owned typed combat-plan
    /// state view as one independent guide lane. Action weights,
    /// legality, exact-state identity and terminal truth remain unchanged.
    #[arg(long, conflicts_with = "anchor_only")]
    typed_plan_guide: bool,
    /// Lab-only ablation of one positive typed boundary-guide lane id. This
    /// changes no action weights, legal actions, exact identity, or terminal
    /// authority.
    #[arg(long, conflicts_with = "anchor_only")]
    omit_guide_lane: Option<u32>,
    /// Opt-in lab control: order concrete members of structured selections
    /// using encounter-owned plan timing. This does not add a state guide.
    #[arg(long)]
    typed_plan_selection_timing: bool,
    /// Lab-only control: materialize one exact base-policy mainline at
    /// player-turn boundaries. A typed encounter plan may defer a
    /// prematurely resource-consuming action or prefer a precisely timed
    /// action; all rejected alternatives remain searchable.
    #[arg(long)]
    plan_compatible_policy_line: bool,
    /// Hidden parity control for the resident actor's typed complete-turn
    /// proposal owner. Proposals remain ordinary exact graph options.
    #[arg(long, hide = true)]
    public_plan_turn_proposals: bool,
    /// Contract assertion: return a non-zero exit status unless an exact,
    /// replay-verified combat witness is found.
    #[arg(long)]
    expect_witness: bool,
    /// Contract assertion: require the verified witness to finish with at
    /// least this much HP.
    #[arg(long, requires = "expect_witness")]
    expect_min_final_hp: Option<i32>,
    /// Print only the compact contract result after all requested
    /// assertions pass. This keeps repeat regression checks readable.
    #[arg(long, requires = "expect_witness")]
    contract_only: bool,
    /// Print only a hierarchical performance profile. Parent timings remain
    /// separate from nested generator and transition timings, and rates are
    /// normalized by exact work rather than inferred from wall time alone.
    #[arg(
        long,
        conflicts_with = "contract_only",
        conflicts_with = "readable",
        conflicts_with = "trace"
    )]
    performance_only: bool,
    #[arg(long, default_value_t = 250_000)]
    max_nodes: usize,
    #[arg(long, default_value_t = 1_000_000)]
    max_selections: usize,
    #[arg(long, default_value_t = 5_000)]
    wall_ms: u64,
    /// Diagnostic-only quality mode: retain the first verified witness
    /// and keep searching until the explicit work/deadline allowance.
    #[arg(long)]
    improve_incumbent: bool,
    /// Stop at the first replay-verified witness whose HP loss is at most
    /// this non-negative bound. This exposes the planner's existing
    /// satisfaction contract without collapsing every combat to either
    /// first-win or best-HP search.
    #[arg(long, conflicts_with = "improve_incumbent")]
    max_hp_loss: Option<u32>,
    /// Stop at the first replay-verified witness whose post-victory HP reaches
    /// this absolute threshold. Unlike `--max-hp-loss`, this remains meaningful
    /// when victory relics or combat healing raise HP above the search root.
    #[arg(
        long,
        conflicts_with = "improve_incumbent",
        conflicts_with = "max_hp_loss"
    )]
    satisfy_min_final_hp: Option<i32>,
    /// Maximum potion resources the exact search may expend. The laboratory
    /// starts potion-free; pass a positive value to open an explicit potion
    /// lane. Every finite limit is enforced during generation, not only when
    /// a terminal witness is accepted.
    #[arg(long, default_value = "0")]
    max_potions_used: Option<u32>,
    /// Restrict newly generated potion actions to exact zero-based slots.
    /// Repeat the flag to admit a specific combination. This is a search
    /// contract, not a trace-only filter.
    #[arg(long)]
    potion_slot: Vec<usize>,
    /// All-legal diagnostic control: admit explicit potion discard actions.
    /// Semantic victory search omits them by default because discarding is not
    /// a generic way to diversify a sparse search.
    #[arg(long)]
    include_discard_actions: bool,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    /// Uniform exploration mixed into action-policy weights, in parts per
    /// million. This is part of search identity and must be explicit when
    /// comparing this graph with another exact-search host.
    #[arg(long, default_value_t = 50_000)]
    uniform_exploration_ppm: u32,
    #[arg(long, default_value_t = 4)]
    generation_quantum_work: usize,
    /// Hidden lab A/B control for the coherent first batch granted to a fresh
    /// exact node selected by a guide or typed proposal. Omit to retain the
    /// production planner default.
    #[arg(long, hide = true)]
    backed_generation_quantum_work: Option<usize>,
    /// Lab-only scheduler A/B: give this positive typed boundary-guide lane
    /// additional one-shot services per round-robin cycle.
    #[arg(long, conflicts_with = "anchor_only")]
    boost_guide_lane: Option<u32>,
    /// Additional service turns granted to `--boost-guide-lane` in each
    /// cycle. Zero preserves the ordinary planner scheduler.
    #[arg(long, default_value_t = 0, requires = "boost_guide_lane")]
    boost_guide_extra_services: usize,
    /// Lab A/B control for the deterministic first service granted to a newly
    /// selected exact turn-boundary node. Omit to retain the planner default.
    #[arg(long)]
    initial_expansion_work: Option<usize>,
    #[arg(long, default_value_t = 32)]
    max_turn_depth: usize,
    /// Diagnostic counterfactual: keep the exact combat state, RNG,
    /// deck, relics and potions, but restore current HP to max HP before
    /// search. This classifies arrival debt; it is never a legal witness
    /// for the original run.
    #[arg(long)]
    full_health: bool,
    /// Include readable, exact replay traces for the deepest survival,
    /// deepest progress, and terminal witness paths.
    #[arg(long)]
    readable: bool,
    /// Print only compact per-turn traces for the deepest states and
    /// witness. Omits raw action hashes and full frontier diagnostics.
    #[arg(long, conflicts_with = "readable")]
    trace: bool,
    /// Report exact graph membership and local service for selected states.
    #[arg(long)]
    watch_exact_state_hash: Vec<String>,
    /// If a replay-verified win is found, save its exact ClientInput list.
    #[arg(long)]
    export_witness_actions: Option<PathBuf>,
    /// Save the exact deepest-survival state as a standalone diagnostic
    /// combat case. Inspect `deepest.survival_node.exhausted` before using
    /// it as a segmented-search continuation.
    #[arg(
        long,
        visible_alias = "export-deepest-case",
        conflicts_with = "export_deepest_progress_case"
    )]
    export_deepest_survival_case: Option<PathBuf>,
    /// Save the exact deepest-progress state as a new standalone combat
    /// case instead of the survival envelope.
    #[arg(long, conflicts_with = "export_deepest_survival_case")]
    export_deepest_progress_case: Option<PathBuf>,
}

fn select_root_turn_continuation_v1(
    root: &CombatPosition,
    requested_candidate_id: &str,
    max_engine_steps_per_action: usize,
) -> Result<(CombatPosition, LocalGraphRootTurnContinuationV1), String> {
    if requested_candidate_id.trim().is_empty() {
        return Err("--root-turn-candidate-id must not be empty".to_string());
    }
    let batch = exact_complete_turn_proposals_v1(
        root,
        ExactCompleteTurnProposalConfigV1 {
            max_engine_steps_per_action,
            ..ExactCompleteTurnProposalConfigV1::default()
        },
    );
    let mut public_ids = BTreeSet::new();
    for candidate in batch.candidates {
        let view =
            project_complete_turn_candidate_v1(root, &candidate, &[], max_engine_steps_per_action)?;
        let candidate_id = complete_turn_candidate_id_v1(&view)?;
        if !public_ids.insert(candidate_id.clone()) {
            continue;
        }
        if candidate_id != requested_candidate_id {
            continue;
        }
        if view.boundary != ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn {
            return Err(format!(
                "root-turn candidate {candidate_id} ends at {:?}, so it has no unresolved continuation to audit",
                view.boundary
            ));
        }
        return Ok((
            candidate.successor,
            LocalGraphRootTurnContinuationV1 {
                candidate_id,
                forced_action_count: view.actions.len(),
                candidate: view,
            },
        ));
    }
    Err(format!(
        "root-turn candidate {requested_candidate_id} is absent from the {} public complete-turn candidates: {}",
        public_ids.len(),
        public_ids.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

pub(super) fn run(args: CombatCaseLocalGraphArgs) -> Result<(), String> {
    let CombatCaseLocalGraphArgs {
        case,
        root_turn_candidate_id,
        anchor_only,
        root_turn_anchor_only,
        action_imitation_artifact,
        value_prototype_artifact,
        guidance_bundle,
        watch_corridor_actions,
        plan_transition_annotations,
        typed_plan_guide,
        omit_guide_lane,
        typed_plan_selection_timing,
        plan_compatible_policy_line,
        public_plan_turn_proposals,
        expect_witness,
        expect_min_final_hp,
        contract_only,
        performance_only,
        max_nodes,
        max_selections,
        wall_ms,
        improve_incumbent,
        max_hp_loss,
        satisfy_min_final_hp,
        mut max_potions_used,
        potion_slot,
        include_discard_actions,
        max_engine_steps_per_transition,
        uniform_exploration_ppm,
        generation_quantum_work,
        backed_generation_quantum_work,
        boost_guide_lane,
        boost_guide_extra_services,
        initial_expansion_work,
        max_turn_depth,
        full_health,
        readable,
        trace,
        watch_exact_state_hash,
        export_witness_actions,
        export_deepest_survival_case,
        export_deepest_progress_case,
    } = args;
    let command_started = Instant::now();
    let mut loaded = load_combat_case(&case)?;
    let original_hp = loaded.core.position.combat.entities.player.current_hp;
    if full_health {
        loaded.core.position.combat.entities.player.current_hp =
            loaded.core.position.combat.entities.player.max_hp;
        loaded.refresh_derived_summaries_and_clear_production_context();
    }
    if !potion_slot.is_empty() && max_potions_used == Some(0) {
        return Err("--potion-slot requires a positive --max-potions-used".to_string());
    }
    if initial_expansion_work == Some(0) {
        return Err("--initial-expansion-work must be positive".to_string());
    }
    if backed_generation_quantum_work == Some(0) {
        return Err("--backed-generation-quantum-work must be positive".to_string());
    }
    if boost_guide_lane == Some(0) {
        return Err("--boost-guide-lane must be positive".to_string());
    }
    if boost_guide_lane.is_some() && boost_guide_extra_services == 0 {
        return Err("--boost-guide-extra-services must be positive".to_string());
    }
    if boost_guide_lane.is_some() && boost_guide_lane == omit_guide_lane {
        return Err("--boost-guide-lane cannot name the omitted guide lane".to_string());
    }
    let allowed_potion_slots = (!potion_slot.is_empty())
        .then(|| potion_slot_mask(&potion_slot))
        .transpose()?;
    let root_turn_continuation = root_turn_candidate_id
        .as_deref()
        .map(|candidate_id| {
            select_root_turn_continuation_v1(
                &loaded.core.position,
                candidate_id,
                max_engine_steps_per_transition,
            )
        })
        .transpose()?;
    if let Some((successor, continuation)) = root_turn_continuation.as_ref() {
        let forced_potion_mask = continuation.candidate.potion_slot_mask;
        if allowed_potion_slots.is_some_and(|allowed| forced_potion_mask & !allowed != 0) {
            return Err(format!(
                "root-turn candidate {} spends a potion outside --potion-slot",
                continuation.candidate_id
            ));
        }
        let forced_potions_used = continuation.candidate.potion_expenditures;
        if let Some(limit) = max_potions_used {
            if forced_potions_used > limit {
                return Err(format!(
                    "root-turn candidate {} spends {forced_potions_used} potion(s), exceeding --max-potions-used {limit}",
                    continuation.candidate_id
                ));
            }
            max_potions_used = Some(limit - forced_potions_used);
        }
        loaded.core.position = successor.clone();
        loaded.refresh_derived_summaries_and_clear_production_context();
    }
    let root_turn_continuation = root_turn_continuation
        .as_ref()
        .map(|(_, continuation)| continuation);
    let initial_hp = loaded.core.position.combat.entities.player.current_hp;
    let root_player_turn = loaded.core.position.combat.turn.turn_count;
    let execution_profile = LocalGraphExecutionProfile::from_controls(
        anchor_only,
        root_turn_anchor_only,
        typed_plan_guide,
        typed_plan_selection_timing,
        omit_guide_lane,
    )?;
    let search_spec = LocalGraphSearchSpec::from_controls(
        max_nodes,
        max_selections,
        wall_ms,
        max_engine_steps_per_transition,
        uniform_exploration_ppm,
        generation_quantum_work,
        max_turn_depth,
        max_potions_used,
        include_discard_actions,
        allowed_potion_slots,
        initial_expansion_work,
        boost_guide_lane
            .map(|lane| LocalGraphGuideServiceBiasSpec::new(lane, boost_guide_extra_services)),
    );
    let search_spec = backed_generation_quantum_work.map_or(search_spec, |work| {
        search_spec.with_backed_generation_quantum_work(work)
    });
    let search_root_position = loaded.core.position.clone();
    let watched_corridor = if watch_corridor_actions.is_empty() {
        None
    } else {
        Some(load_exact_turn_corridor(
            &case,
            &watch_corridor_actions,
            max_engine_steps_per_transition,
        )?)
    };
    let root = CombatDecisionRoot::new(loaded.core.position.clone())
        .map_err(|error| format!("invalid combat case root: {error:?}"))?;
    let satisfaction = if improve_incumbent {
        OracleCombatWitnessSatisfaction::BudgetOrExhaustion
    } else if let Some(minimum) = satisfy_min_final_hp {
        OracleCombatWitnessSatisfaction::FinalHpAtLeast(minimum)
    } else if let Some(limit) = max_hp_loss {
        OracleCombatWitnessSatisfaction::HpLossAtMost(limit)
    } else {
        OracleCombatWitnessSatisfaction::FirstWitness
    };
    let config = search_spec.planner_config(satisfaction);
    let policy = if let Some(path) = guidance_bundle.as_deref() {
        CombatGuidanceBundleV1::load(path)?.policy(retained_combat_tactical_policy_v1())?
    } else {
        let policy = action_imitation_artifact
            .as_deref()
            .map(|path| load_action_imitation_policy(path, retained_combat_tactical_policy_v1()))
            .transpose()?
            .unwrap_or_else(retained_combat_tactical_policy_v1);
        if let Some(path) = value_prototype_artifact.as_deref() {
            let artifact = load_value_prototype(path)?;
            combat_value_prototype_policy_v1(policy, &artifact)
        } else {
            policy
        }
    };
    let mut session = execution_profile.prepare_session(root, root_player_turn, config, policy);
    if public_plan_turn_proposals {
        session
            .enable_public_plan_turn_proposals(PublicPlanTurnProposalConfig::default())
            .map_err(|error| {
                format!(
                    "cannot enable public-plan turn proposals after graph construction: \
                     {error:?}"
                )
            })?;
    }
    if plan_transition_annotations {
        session
            .enable_plan_transition_annotations()
            .map_err(|error| {
                format!(
                    "cannot enable plan transition annotations after graph construction: \
                             {error:?}"
                )
            })?;
    }
    let policy_line_report = plan_compatible_policy_line
        .then(|| {
            session.offer_plan_compatible_policy_line(max_turn_depth, 256, &EngineCombatStepper)
        })
        .transpose()?;
    let search_started = Instant::now();
    let report = session.advance(search_spec.quantum(), &EngineCombatStepper);
    let search_elapsed = search_started.elapsed();
    let progress = session.progress_snapshot();
    let witness_inputs = report.witness.as_ref().map(|witness| {
        witness
            .actions
            .iter()
            .map(|action| action.input.clone())
            .collect::<Vec<_>>()
    });
    // Exports are explicit side effects of the command, not presentation.
    // Complete them before compact-contract or performance-only reporting can
    // return early.
    let exports = export_local_graph_paths(
        &loaded,
        (!full_health && root_turn_continuation.is_none()).then_some(case.as_path()),
        LocalGraphExportPaths {
            witness_actions: export_witness_actions.as_deref(),
            deepest_survival_case: export_deepest_survival_case.as_deref(),
            deepest_progress_case: export_deepest_progress_case.as_deref(),
        },
        LocalGraphExportActions {
            witness: witness_inputs.as_deref(),
            witness_final_position: report
                .witness
                .as_ref()
                .map(|witness| &witness.final_position),
            deepest_survival: &progress.deepest_survival_actions,
            deepest_progress: &progress.deepest_progress_actions,
        },
        max_engine_steps_per_transition,
    )?;
    if let Some(contract_result) = evaluate_local_graph_contract(LocalGraphContractRequest {
        case: &case,
        elapsed: command_started.elapsed(),
        report: &report,
        policy_line: policy_line_report.as_ref(),
        expect_witness,
        expect_min_final_hp,
        contract_only,
    })? {
        return print_json(&contract_result);
    }
    let performance_profile =
        combat_case_performance::local_graph_performance_report(search_elapsed, &case, &report);
    if performance_only {
        return print_json(&performance_profile);
    }
    let performance_timing = combat_case_performance::local_graph_performance_timing(&report);
    let include_trace = readable || trace;
    let diagnostics = materialize_local_graph_diagnostics(
        &session,
        &search_root_position,
        LocalGraphDiagnosticPaths {
            deepest_survival: &progress.deepest_survival_actions,
            deepest_progress: &progress.deepest_progress_actions,
            witness: report
                .witness
                .as_ref()
                .map(|witness| witness.actions.as_slice()),
        },
        include_trace,
        max_engine_steps_per_transition,
    )?;
    let observation = capture_local_graph_observation(
        &session,
        &search_root_position,
        &watch_exact_state_hash,
        watched_corridor.as_ref(),
    );
    let plan_transition_portfolio = plan_transition_annotations
        .then(|| combat_plan_transition_portfolio_v1(&session))
        .unwrap_or(Value::Null);
    let report_data = LocalGraphReportData {
        run: LocalGraphRunIdentity {
            case: &case,
            elapsed: command_started.elapsed(),
            satisfaction,
            execution_profile,
            search_spec,
            counterfactual: LocalGraphCounterfactual {
                full_health,
                original_hp,
                search_hp: initial_hp,
            },
            root_turn_continuation,
        },
        report: &report,
        progress: &progress,
        retained_state_work: session.retained_state_work(),
        storage: session.storage_snapshot(),
        policy_line: policy_line_report.as_ref(),
        plan_transition_annotations,
        plan_transition_portfolio: &plan_transition_portfolio,
        diagnostics: &diagnostics,
        observation: &observation,
        exports: &exports,
    };
    if trace {
        return print_json(&local_graph_trace_report(&report_data));
    }
    let output = local_graph_full_report(
        &report_data,
        LocalGraphFullReportOptions {
            action_imitation_artifact: action_imitation_artifact.as_deref(),
            value_prototype_artifact: value_prototype_artifact.as_deref(),
            guidance_bundle: guidance_bundle.as_deref(),
            watch_corridor_actions: &watch_corridor_actions,
            readable,
            search_elapsed,
            performance_timing: &performance_timing,
            performance_profile: &performance_profile,
        },
    );
    print_json(&output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
    use sts_oracle_runtime::content::{cards::CardId, monsters::EnemyId};
    use sts_oracle_runtime::runtime::combat::CombatCard;
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::test_support::{blank_test_combat, planned_monster};

    #[test]
    fn root_turn_continuation_reuses_the_public_candidate_identity_and_exact_successor() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let config = ExactCompleteTurnProposalConfigV1::default();
        let batch = exact_complete_turn_proposals_v1(&root, config);
        let expected = batch
            .candidates
            .iter()
            .find(|candidate| {
                candidate.boundary == ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn
            })
            .expect("next-turn candidate");
        let expected_view = project_complete_turn_candidate_v1(
            &root,
            expected,
            &[],
            config.max_engine_steps_per_action,
        )
        .expect("public complete-turn view");
        let candidate_id =
            complete_turn_candidate_id_v1(&expected_view).expect("complete-turn identity");

        let (selected, continuation) = select_root_turn_continuation_v1(
            &root,
            &candidate_id,
            config.max_engine_steps_per_action,
        )
        .expect("typed root-turn continuation");

        assert_eq!(continuation.candidate_id, candidate_id);
        assert_eq!(continuation.candidate, expected_view);
        assert_eq!(
            combat_exact_state_hash_v2(&selected.engine, &selected.combat),
            combat_exact_state_hash_v2(&expected.successor.engine, &expected.successor.combat)
        );
    }
}
