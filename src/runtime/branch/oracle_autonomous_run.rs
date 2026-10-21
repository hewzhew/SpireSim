use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};

use crate::eval::run_control::OracleRunBoundaryV1;

use super::{
    oracle_analysis_session::{OracleAnalysisAdvanceRequestV1, OracleAnalysisNodeViewV1},
    OracleAnalysisWorkspaceV1,
};

pub(super) struct OwnerBatchResult {
    pub(super) applied_count: usize,
    pub(super) applied: Vec<Value>,
    pub(super) stopped: String,
    pub(super) current: OracleAnalysisNodeViewV1,
}

pub(super) fn apply_owner_steps(
    workspace: &mut OracleAnalysisWorkspaceV1,
    steps: u8,
) -> Result<OwnerBatchResult, String> {
    if steps == 0 {
        return Err("oracle owner steps must be positive".to_string());
    }
    let current = workspace.view()?;
    let mut visited_nodes = BTreeSet::new();
    apply_owner_steps_from(workspace, current, steps, &mut visited_nodes)
}

fn apply_owner_steps_from(
    workspace: &mut OracleAnalysisWorkspaceV1,
    mut current: OracleAnalysisNodeViewV1,
    steps: u8,
    visited_nodes: &mut BTreeSet<usize>,
) -> Result<OwnerBatchResult, String> {
    debug_assert!(steps > 0);

    let mut applied = Vec::new();
    let mut stopped = "step_limit".to_string();
    visited_nodes.insert(current.node_id);
    for _ in 0..steps {
        let current_owner_order = workspace.session.current_choice_order(current.node_id)?;
        let Some(current_choice_ref) = current_owner_order.first() else {
            stopped = if current.choices.is_empty() {
                "no_choices".to_string()
            } else {
                "current_owner_order_empty".to_string()
            };
            break;
        };
        let matching = current
            .choices
            .iter()
            .filter(|choice| choice.choice_ref == *current_choice_ref)
            .collect::<Vec<_>>();
        let [choice] = matching.as_slice() else {
            stopped = format!(
                "current_owner_choice_join_count={} choice_ref={current_choice_ref}",
                matching.len()
            );
            break;
        };
        let parent_node_id = current.node_id;
        let candidate_id = choice.candidate_id.clone();
        let label = choice.label.clone();
        let choice_ref = choice.choice_ref.clone();
        let materialized_owner_rank = choice.owner_rank;
        let next = match workspace.try_choice(&choice_ref) {
            Ok(next) => next,
            Err(error) => {
                stopped = format!("choice_execution_error={error}");
                break;
            }
        };
        if next.node_id == parent_node_id {
            stopped = format!("choice_made_no_progress={label}");
            break;
        }
        current = next;
        applied.push(json!({
            "node": parent_node_id,
            "candidate_id": candidate_id,
            "label": label,
            "owner_rank": 0,
            "materialized_owner_rank": materialized_owner_rank,
        }));
        if !visited_nodes.insert(current.node_id) {
            stopped = format!("owner_node_cycle={}", current.node_id);
            break;
        }
    }
    let applied_count = applied.len();
    Ok(OwnerBatchResult {
        applied_count,
        applied,
        stopped,
        current,
    })
}

#[derive(Clone, Debug)]
pub struct OracleAutonomousRunConfigV1 {
    pub hallway_wall_ms: u64,
    pub elite_wall_ms: u64,
    pub boss_wall_ms: u64,
    pub max_quanta: usize,
    pub quantum_generation_work: usize,
    pub quantum_ms: u64,
    pub max_boundaries: usize,
    pub run_wall_ms: Option<u64>,
    pub export_continuation: Option<PathBuf>,
}

#[derive(Default)]
struct AutonomousRunTiming {
    initial_view_ms: u64,
    owner_ms: u64,
    combat_advance_ms: u64,
    combat_accept_ms: u64,
    reported_combat_ms: u64,
}

impl AutonomousRunTiming {
    fn accounted_run_ms(&self) -> u64 {
        self.initial_view_ms
            .saturating_add(self.owner_ms)
            .saturating_add(self.combat_advance_ms)
            .saturating_add(self.combat_accept_ms)
    }

    fn report(&self, run_elapsed_ms: u64, run_wall_ms: Option<u64>) -> Value {
        let accounted_run_ms = self.accounted_run_ms();
        json!({
            "run_ms": run_elapsed_ms,
            "run_budget_ms": run_wall_ms,
            "budget_overshoot_ms": run_wall_ms
                .map(|budget| run_elapsed_ms.saturating_sub(budget)),
            "initial_view_ms": self.initial_view_ms,
            "owner_ms": self.owner_ms,
            "combat_advance_wall_ms": self.combat_advance_ms,
            "combat_accept_wall_ms": self.combat_accept_ms,
            "combat_reported_ms": self.reported_combat_ms,
            "accounted_run_ms": accounted_run_ms,
            "run_residual_ms": run_elapsed_ms.saturating_sub(accounted_run_ms),
        })
    }
}

pub fn run_oracle_analysis_to_stop_v1(
    workspace: &mut OracleAnalysisWorkspaceV1,
    config: &OracleAutonomousRunConfigV1,
) -> Result<Value, String> {
    if config.max_boundaries == 0
        || config.max_quanta == 0
        || config.quantum_generation_work == 0
        || config.quantum_ms == 0
        || config.hallway_wall_ms == 0
        || config.elite_wall_ms == 0
        || config.boss_wall_ms == 0
    {
        return Err("oracle run budgets and max-boundaries must be positive".to_string());
    }

    let run_started = Instant::now();
    let mut timing = AutonomousRunTiming::default();
    let initial_view_started = Instant::now();
    let mut node = workspace.view()?;
    timing.initial_view_ms = elapsed_millis(initial_view_started);
    let start_node = node.node_id;
    let mut owner_decisions = 0_u64;
    let mut combats = Vec::new();
    let mut owner_visited_nodes = BTreeSet::new();

    for _ in 0..config.max_boundaries {
        if matches!(
            node.boundary,
            OracleRunBoundaryV1::TerminalVictory | OracleRunBoundaryV1::TerminalDefeat
        ) {
            return terminal_autonomous_run_report(
                workspace,
                &node,
                start_node,
                owner_decisions,
                &combats,
                &timing,
                elapsed_millis(run_started),
                config.run_wall_ms,
                config.export_continuation.as_deref(),
            );
        }
        if run_wall_budget_reached(run_started, config.run_wall_ms) {
            return Ok(stopped_autonomous_run_report(
                start_node,
                &node,
                owner_decisions,
                &combats,
                &timing,
                elapsed_millis(run_started),
                config.run_wall_ms,
                "run_wall_budget",
            ));
        }

        if !node.choices.is_empty() {
            let owner_started = Instant::now();
            let owner = apply_owner_steps_from(workspace, node, 64, &mut owner_visited_nodes)?;
            timing.owner_ms = timing
                .owner_ms
                .saturating_add(elapsed_millis(owner_started));
            node = owner.current;
            owner_decisions = owner_decisions.saturating_add(owner.applied_count as u64);
            if owner.stopped.starts_with("owner_node_cycle=") {
                return Ok(stopped_autonomous_run_report(
                    start_node,
                    &node,
                    owner_decisions,
                    &combats,
                    &timing,
                    elapsed_millis(run_started),
                    config.run_wall_ms,
                    "owner_node_cycle",
                ));
            }
            if owner.applied_count == 0 {
                return Ok(stopped_autonomous_run_report(
                    start_node,
                    &node,
                    owner_decisions,
                    &combats,
                    &timing,
                    elapsed_millis(run_started),
                    config.run_wall_ms,
                    "owner_choice_missing_or_ambiguous",
                ));
            }
            continue;
        }

        if node.boundary != OracleRunBoundaryV1::Combat {
            return Ok(stopped_autonomous_run_report(
                start_node,
                &node,
                owner_decisions,
                &combats,
                &timing,
                elapsed_millis(run_started),
                config.run_wall_ms,
                "noncombat_boundary_without_owner_choice",
            ));
        }

        let encounter = node
            .encounter
            .as_ref()
            .ok_or_else(|| "combat boundary omitted encounter metadata".to_string())?;
        let combat_wall_ms = if encounter.is_boss {
            config.boss_wall_ms
        } else if encounter.is_elite {
            config.elite_wall_ms
        } else {
            config.hallway_wall_ms
        };
        let wall_ms = config.run_wall_ms.map_or(combat_wall_ms, |limit| {
            combat_wall_ms.min(limit.saturating_sub(elapsed_millis(run_started)).max(1))
        });
        let combat_node = node.node_id;

        let advance_started = Instant::now();
        let (report, mut after) = workspace.advance(OracleAnalysisAdvanceRequestV1 {
            max_quanta: config.max_quanta,
            quantum_generation_work: config.quantum_generation_work,
            quantum_ms: Some(config.quantum_ms),
            wall_ms: Some(wall_ms),
            // Standard advance checks strategic HP quality and promotes to an
            // exact potion rescue only when needed. Autonomous run must not
            // bypass that contract or force every combat to spend its full
            // wall polishing an acceptable incumbent.
            improve_incumbent: false,
        })?;
        timing.combat_advance_ms = timing
            .combat_advance_ms
            .saturating_add(elapsed_millis(advance_started));
        timing.reported_combat_ms = timing.reported_combat_ms.saturating_add(report.elapsed_ms);

        let incumbent = report
            .combat
            .as_ref()
            .and_then(|combat| combat.incumbent_final_hp);
        let materialized = after.boundary != OracleRunBoundaryV1::Combat;
        let incumbent_survival_floor_satisfied = if materialized {
            incumbent.map(|_| true)
        } else if incumbent.is_some() {
            Some(
                workspace
                    .session
                    .cursor_combat_incumbent_preserves_survival_floor()?,
            )
        } else {
            None
        };
        let incumbent_needs_acceptance = combat_incumbent_needs_acceptance(
            materialized,
            incumbent,
            incumbent_survival_floor_satisfied,
        );
        let accepted_after_run_wall =
            incumbent_needs_acceptance && run_wall_budget_reached(run_started, config.run_wall_ms);
        let accepted = if incumbent_needs_acceptance {
            let accept_started = Instant::now();
            after = workspace.accept_combat_incumbent()?;
            timing.combat_accept_ms = timing
                .combat_accept_ms
                .saturating_add(elapsed_millis(accept_started));
            true
        } else {
            false
        };
        combats.push(json!({
            "node": combat_node,
            "act": node.act,
            "floor": node.floor,
            "start_hp": node.current_hp,
            "kind": encounter_kind(encounter.is_elite, encounter.is_boss),
            "monsters": encounter.monsters.iter().map(|monster| monster.label.clone()).collect::<Vec<_>>(),
            "budget_ms": wall_ms,
            "elapsed_ms": report.elapsed_ms,
            "accepted_incumbent": accepted,
            "incumbent_survival_floor_satisfied": incumbent_survival_floor_satisfied,
            "incumbent_accepted_after_run_wall": accepted_after_run_wall,
            "search": compact_run_combat_progress(report.combat.as_ref()),
            "after": compact_run_node(&after),
        }));

        if !materialized && !accepted {
            let stop_reason =
                if incumbent.is_some() && incumbent_survival_floor_satisfied == Some(false) {
                    "combat_budget_unknown_without_reserve_compliant_witness"
                } else {
                    "combat_budget_unknown_without_witness"
                };
            return Ok(stopped_autonomous_run_report(
                start_node,
                &after,
                owner_decisions,
                &combats,
                &timing,
                elapsed_millis(run_started),
                config.run_wall_ms,
                stop_reason,
            ));
        }
        node = after;
    }

    if matches!(
        node.boundary,
        OracleRunBoundaryV1::TerminalVictory | OracleRunBoundaryV1::TerminalDefeat
    ) {
        return terminal_autonomous_run_report(
            workspace,
            &node,
            start_node,
            owner_decisions,
            &combats,
            &timing,
            elapsed_millis(run_started),
            config.run_wall_ms,
            config.export_continuation.as_deref(),
        );
    }
    Ok(stopped_autonomous_run_report(
        start_node,
        &node,
        owner_decisions,
        &combats,
        &timing,
        elapsed_millis(run_started),
        config.run_wall_ms,
        "boundary_limit",
    ))
}

fn terminal_autonomous_run_report(
    workspace: &OracleAnalysisWorkspaceV1,
    node: &OracleAnalysisNodeViewV1,
    start_node: usize,
    owner_decisions: u64,
    combats: &[Value],
    timing: &AutonomousRunTiming,
    run_elapsed_ms: u64,
    run_wall_ms: Option<u64>,
    export_continuation: Option<&Path>,
) -> Result<Value, String> {
    let victory = node.boundary == OracleRunBoundaryV1::TerminalVictory;
    let final_node = node.node_id;
    let mut verification_ms = 0;
    let mut export_ms = 0;
    let mut verification = None;
    let mut export = None;
    if victory {
        let continuation = workspace.continuation(final_node)?;
        if let Some(path) = export_continuation {
            let export_started = Instant::now();
            super::oracle_run::save_oracle_run_continuation_v1(path, &continuation)?;
            export_ms = elapsed_millis(export_started);
            export = Some(json!({
                "node_id": final_node,
                "path": path,
                "journal_entries": continuation.journal.entries().len(),
            }));
        }
        let verify_started = Instant::now();
        let expected_final = continuation.session.into_session()?;
        let report = crate::eval::run_control::exact_replay_run_progress_journal_v1(
            workspace.seed,
            workspace.ascension,
            &continuation.journal,
            &expected_final,
        )?;
        verification_ms = elapsed_millis(verify_started);
        verification = Some(json!({
            "schema_name": "ExactOracleRunWitnessReplayV1",
            "schema_version": 1,
            "node_id": final_node,
            "report": report,
        }));
    }
    let mut timing_report = timing.report(run_elapsed_ms, run_wall_ms);
    timing_report["run_ms_before_verification_and_export"] = json!(run_elapsed_ms);
    timing_report["verification_ms"] = json!(verification_ms);
    timing_report["export_ms"] = json!(export_ms);
    Ok(json!({
        "schema_name": "OracleAutonomousRunReportV2",
        "schema_version": 2,
        "status": if victory { "victory_verified" } else { "terminal_defeat" },
        "start_node": start_node,
        "final": compact_run_node(node),
        "owner_decisions": owner_decisions,
        "combat_count": combats.len(),
        "total_combat_elapsed_ms": timing.reported_combat_ms,
        "timing": timing_report,
        "combats": combats,
        "verification": verification,
        "continuation_export": export,
    }))
}

fn stopped_autonomous_run_report(
    start_node: usize,
    node: &OracleAnalysisNodeViewV1,
    owner_decisions: u64,
    combats: &[Value],
    timing: &AutonomousRunTiming,
    run_elapsed_ms: u64,
    run_wall_ms: Option<u64>,
    reason: &str,
) -> Value {
    json!({
        "schema_name": "OracleAutonomousRunReportV2",
        "schema_version": 2,
        "status": "stopped",
        "reason": reason,
        "start_node": start_node,
        "final": compact_run_node(node),
        "owner_decisions": owner_decisions,
        "combat_count": combats.len(),
        "total_combat_elapsed_ms": timing.reported_combat_ms,
        "timing": timing.report(run_elapsed_ms, run_wall_ms),
        "combats": combats,
    })
}

fn compact_run_node(node: &OracleAnalysisNodeViewV1) -> Value {
    json!({
        "node": node.node_id,
        "parent": node.canonical_parent_node_id,
        "boundary": node.boundary,
        "act": node.act,
        "floor": node.floor,
        "hp": node.current_hp,
        "max_hp": node.max_hp,
        "gold": node.gold,
        "choice_count": node.choices.len(),
        "choices": node.choices.iter().take(8).map(|choice| json!({
            "candidate_id": choice.candidate_id,
            "label": choice.label,
            "owner_rank": choice.owner_rank,
        })).collect::<Vec<_>>(),
        "child_count": node.children.len(),
    })
}

fn compact_run_combat_progress(
    combat: Option<&super::oracle_analysis_session::OracleAnalysisCombatProgressV1>,
) -> Value {
    let Some(combat) = combat else {
        return Value::Null;
    };
    json!({
        "root_exact_state_hash": combat.root_exact_state_hash,
        "stage_trace": combat.stage_trace,
        "search_stage": combat.search_stage,
        "max_potions_used": combat.max_potions_used,
        "allowed_potion_slots": combat.allowed_potion_slots,
        "generation_work": combat.generation_work,
        "local_generation_work": combat.local_generation_work,
        "discrepancy_generation_work": combat.discrepancy_generation_work,
        "exact_states": combat.exact_states,
        "local_exact_states": combat.local_exact_states,
        "discrepancy_exact_states": combat.discrepancy_exact_states,
        "completed_turn_options": combat.completed_turn_options,
        "retained_state_work": combat.retained_state_work,
        "local_retained_state_work": combat.local_retained_state_work,
        "discrepancy_retained_state_work": combat.discrepancy_retained_state_work,
        "max_player_turn": combat.max_player_turn,
        "incumbent_final_hp": combat.incumbent_final_hp,
        "incumbent_hp_loss": combat.incumbent_hp_loss,
        "incumbent_actions": combat.incumbent_action_count,
        "incumbent_potions_used": combat.incumbent_potions_used,
        "incumbent_potion_slots": combat.incumbent_potion_slots,
        "incumbent_satisfies_satisfaction": combat.incumbent_satisfies_satisfaction,
        "incumbent_ends_quality_refinement": combat.incumbent_ends_quality_refinement,
        "last_status": combat.last_status,
        "resume_kind": combat.resume_kind,
        "restart_count": combat.restart_count,
    })
}

fn encounter_kind(is_elite: bool, is_boss: bool) -> &'static str {
    if is_boss {
        "boss"
    } else if is_elite {
        "elite"
    } else {
        "hallway_or_event"
    }
}

pub(super) fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn run_wall_budget_reached(started: Instant, run_wall_ms: Option<u64>) -> bool {
    run_wall_ms.is_some_and(|limit| elapsed_millis(started) >= limit)
}

fn combat_incumbent_needs_acceptance(
    materialized: bool,
    incumbent_final_hp: Option<i32>,
    survival_floor_satisfied: Option<bool>,
) -> bool {
    !materialized && incumbent_final_hp.is_some() && survival_floor_satisfied == Some(true)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        apply_owner_steps, combat_incumbent_needs_acceptance, run_oracle_analysis_to_stop_v1,
        run_wall_budget_reached, AutonomousRunTiming, OracleAutonomousRunConfigV1,
    };
    use crate::content::cards::CardId;
    use crate::content::potions::{Potion, PotionId};
    use crate::eval::run_control::{
        positive_ranked_run_policy_prior_v1, seed_oracle_run_explorer_from_session_v1,
        RunControlConfig, RunControlSession, RunPolicyCandidateV1, RunPolicyPriorV1,
        RunProgressJournalV1,
    };
    use crate::runtime::branch::oracle_run::oracle_run_combat_budgets_v1;
    use crate::runtime::branch::{
        OracleAnalysisSessionV1, OracleAnalysisWorkspaceV1, OracleRunBudget, OracleRunConfig,
    };
    use crate::state::core::EngineState;
    use crate::state::rewards::{RewardCard, RewardItem, RewardState};
    use crate::state::shop::ShopState;

    #[test]
    fn owner_batch_returns_the_current_workspace_view() {
        let mut workspace = OracleAnalysisWorkspaceV1::new(OracleRunConfig {
            seed: 20260713007,
            ascension: 0,
            budget: OracleRunBudget::default(),
        })
        .expect("seed007 oracle workspace");

        let batch = apply_owner_steps(&mut workspace, 3).expect("owner batch");
        let reloaded = workspace.view().expect("current workspace view");

        assert_eq!(batch.current.node_id, reloaded.node_id);
        assert_eq!(batch.current.boundary, reloaded.boundary);
        assert_eq!(batch.current.current_hp, reloaded.current_hp);
        assert_eq!(batch.applied_count, batch.applied.len());
    }

    #[test]
    fn owner_batch_joins_current_policy_to_stale_choices_by_exact_choice_ref() {
        fn materialized_first_prior(
            _session: &RunControlSession,
            legal: &[RunPolicyCandidateV1<'_>],
        ) -> Result<RunPolicyPriorV1, String> {
            positive_ranked_run_policy_prior_v1(
                legal,
                legal
                    .first()
                    .map(|candidate| candidate.candidate_id.to_string()),
            )
        }

        fn current_skip_prior(
            _session: &RunControlSession,
            legal: &[RunPolicyCandidateV1<'_>],
        ) -> Result<RunPolicyPriorV1, String> {
            positive_ranked_run_policy_prior_v1(legal, ["skip-card-reward".to_string()])
        }

        let cards = vec![
            RewardCard::new(CardId::Cleave, 0),
            RewardCard::new(CardId::IronWave, 0),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Card {
            cards: cards.clone(),
        }];
        reward.pending_card_choice = Some(cards);
        reward.pending_card_reward_index = Some(0);
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::RewardScreen(reward);

        let config = OracleRunConfig {
            seed: 20260803001,
            ascension: 0,
            budget: OracleRunBudget::default(),
        };
        let combat_budgets = oracle_run_combat_budgets_v1(&config);
        let explorer = seed_oracle_run_explorer_from_session_v1(
            session,
            RunProgressJournalV1::default(),
            &combat_budgets,
            Some(materialized_first_prior),
        )
        .expect("stale-rank test explorer");
        let analysis = OracleAnalysisSessionV1::from_explorer(
            explorer,
            None,
            combat_budgets,
            Some(current_skip_prior),
            None,
        )
        .expect("stale-rank test analysis session");
        let mut workspace = OracleAnalysisWorkspaceV1 {
            seed: config.seed,
            ascension: config.ascension,
            budget: config.budget,
            combat_guidance_bundle: None,
            session: analysis,
        };
        let materialized = workspace.view().expect("materialized reward view");
        assert_ne!(
            materialized
                .choices
                .iter()
                .find(|choice| choice.owner_rank == 0)
                .map(|choice| choice.candidate_id.as_str()),
            Some("skip-card-reward")
        );

        let batch = apply_owner_steps(&mut workspace, 1).expect("current-owner step");

        assert_eq!(batch.applied_count, 1);
        assert_eq!(batch.applied[0]["candidate_id"], "skip-card-reward");
        assert_eq!(batch.applied[0]["owner_rank"], 0);
        assert_ne!(batch.applied[0]["materialized_owner_rank"], 0);
    }

    #[test]
    fn owner_batch_stops_on_a_multi_node_policy_cycle() {
        fn cycling_prior(
            session: &RunControlSession,
            legal: &[RunPolicyCandidateV1<'_>],
        ) -> Result<RunPolicyPriorV1, String> {
            let preferred = match session.engine_state {
                EngineState::Shop(_) => vec!["rewards".to_string()],
                EngineState::RewardOverlay { .. } => vec!["back".to_string()],
                _ => Vec::new(),
            };
            positive_ranked_run_policy_prior_v1(legal, preferred)
        }

        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 16;
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::AttackPotion, 1)),
            Some(Potion::new(PotionId::SpeedPotion, 2)),
            Some(Potion::new(PotionId::EssenceOfSteel, 3)),
        ];
        let mut pending = RewardState::new();
        pending.items.push(RewardItem::Potion {
            potion_id: PotionId::LiquidBronze,
        });
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.pending_reward_overlay = Some(pending);
        session.engine_state = EngineState::Shop(shop);

        let config = OracleRunConfig {
            seed: 20260730006,
            ascension: 0,
            budget: OracleRunBudget::default(),
        };
        let combat_budgets = oracle_run_combat_budgets_v1(&config);
        let explorer = seed_oracle_run_explorer_from_session_v1(
            session,
            RunProgressJournalV1::default(),
            &combat_budgets,
            Some(cycling_prior),
        )
        .expect("cycle test explorer");
        let analysis = OracleAnalysisSessionV1::from_explorer(
            explorer,
            None,
            combat_budgets,
            Some(cycling_prior),
            None,
        )
        .expect("cycle test analysis session");
        let mut workspace = OracleAnalysisWorkspaceV1 {
            seed: config.seed,
            ascension: config.ascension,
            budget: config.budget,
            combat_guidance_bundle: None,
            session: analysis,
        };
        let start_node = workspace.view().expect("cycle start view").node_id;

        let batch = apply_owner_steps(&mut workspace, 8).expect("guarded owner batch");

        assert!(
            (2..8).contains(&batch.applied_count),
            "a multi-node cycle should stop before the requested step limit"
        );
        assert_ne!(batch.current.node_id, start_node);
        assert_eq!(
            batch.stopped,
            format!("owner_node_cycle={}", batch.current.node_id)
        );
    }

    #[test]
    fn timing_report_closes_the_run_ledger_and_reports_overshoot() {
        let timing = AutonomousRunTiming {
            initial_view_ms: 3,
            owner_ms: 5,
            combat_advance_ms: 11,
            combat_accept_ms: 7,
            reported_combat_ms: 9,
        };

        let report = timing.report(31, Some(29));

        assert_eq!(report["accounted_run_ms"], 26);
        assert_eq!(report["run_residual_ms"], 5);
        assert_eq!(report["budget_overshoot_ms"], 2);
        assert_eq!(
            report["accounted_run_ms"].as_u64().unwrap()
                + report["run_residual_ms"].as_u64().unwrap(),
            report["run_ms"].as_u64().unwrap()
        );
    }

    #[test]
    fn run_wall_budget_check_is_explicitly_disabled_by_none() {
        let started = Instant::now() - Duration::from_millis(10);

        assert!(!run_wall_budget_reached(started, None));
        assert!(run_wall_budget_reached(started, Some(5)));
    }

    #[test]
    fn verified_incumbent_remains_an_atomic_commit_after_search_wall() {
        let started = Instant::now() - Duration::from_millis(10);

        assert!(run_wall_budget_reached(started, Some(5)));
        assert!(combat_incumbent_needs_acceptance(
            false,
            Some(51),
            Some(true)
        ));
        assert!(!combat_incumbent_needs_acceptance(
            false,
            Some(9),
            Some(false)
        ));
        assert!(!combat_incumbent_needs_acceptance(
            true,
            Some(51),
            Some(true)
        ));
        assert!(!combat_incumbent_needs_acceptance(false, None, None));
    }

    #[test]
    fn autonomous_run_does_not_preseed_an_external_witness_before_exact_search() {
        // This must cross the public workspace runner: lower combat-work tests
        // cannot catch an autonomous pre-acceptance bypass around `advance`.
        let config = OracleRunConfig {
            seed: 20260730012,
            ascension: 0,
            budget: OracleRunBudget::default(),
        };
        let combat_budgets = oracle_run_combat_budgets_v1(&config);
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
        let plan = crate::content::monsters::roll_monster_turn_plan(
            &mut combat.rng.ai_rng,
            &monster,
            combat.meta.ascension_level,
            99,
            std::slice::from_ref(&monster),
            &[],
        );
        monster.set_planned_move_id(plan.move_id);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        monster.current_hp = 6;
        monster.max_hp = 6;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            1,
        )];
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(crate::state::core::ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            crate::state::core::CombatContext::Room(crate::state::core::RoomCombatContext {
                room_type: crate::state::map::node::RoomType::MonsterRoom,
            }),
        ));
        let explorer = seed_oracle_run_explorer_from_session_v1(
            session,
            RunProgressJournalV1::default(),
            &combat_budgets,
            None,
        )
        .expect("seed one-strike combat explorer");
        let analysis =
            OracleAnalysisSessionV1::from_explorer(explorer, Some(0), combat_budgets, None, None)
                .expect("one-strike analysis session");
        let mut workspace = OracleAnalysisWorkspaceV1 {
            seed: config.seed,
            ascension: config.ascension,
            budget: config.budget,
            combat_guidance_bundle: None,
            session: analysis,
        };
        let before = workspace.view().expect("combat view");
        assert_eq!(
            before.boundary,
            crate::eval::run_control::OracleRunBoundaryV1::Combat
        );
        assert_eq!(
            before
                .combat
                .as_ref()
                .and_then(|work| work.incumbent_final_hp),
            None,
            "production must not preseed a terminal witness outside exact frontier search"
        );

        let report = run_oracle_analysis_to_stop_v1(
            &mut workspace,
            &OracleAutonomousRunConfigV1 {
                hallway_wall_ms: 1_000,
                elite_wall_ms: 1_000,
                boss_wall_ms: 1_000,
                max_quanta: 8,
                quantum_generation_work: 64,
                quantum_ms: 10,
                max_boundaries: 1,
                run_wall_ms: None,
                export_continuation: None,
            },
        )
        .expect("autonomous one-strike run");
        let combat = &report["combats"][0];

        assert!(combat.get("accepted_existing_incumbent").is_none());
        assert!(combat["search"]["generation_work"].as_u64().unwrap_or(0) > 0);
        assert_eq!(report["final"]["boundary"], "reward");
    }
}
