use crate::ai::card_semantics_v1::{potion_acquisition_traits_v1, PotionAcquisitionTraitV1};
use crate::ai::combat_search_v2::{
    has_external_payoff_opportunity, CombatSearchV2Report, SearchTerminalLabel,
};
use crate::ai::strategy::candidate_pressure_response::assess_candidate_pressure_response;
use crate::ai::strategy::pressure_assessment::PressureAxis;
use crate::ai::strategy::reward_admission::assess_reward_admission_from_master_deck;
use crate::content::powers::{store, PowerId};
use crate::runtime::combat::{CardLineageId, CombatCard};
use crate::sim::combat::{CombatPosition, CombatTerminal};
use crate::sim::combat_legal_actions::engine_atomic_actions;
use crate::state::core::{ClientInput, EngineState, RunResult};

use super::combat_candidate_line::CombatCandidateLine;
use super::combat_line_adjudication::CombatLineAdjudicationV1;
use super::session::RunControlSession;
use super::trace_annotation::{
    CombatAutomationActionV1, CombatAutomationAnswerClaimV1, CombatAutomationAnswerSourceV1,
    CombatAutomationCardOriginV1, CombatAutomationMonsterStateV1,
    CombatAutomationOpportunityStateV1, CombatAutomationPotionStateV1, CombatAutomationStepStateV1,
    CombatSearchPerformanceSnapshotV1, CombatSearchTerminalLineSummary,
    RunControlTraceAnnotationV1,
};
use super::transition_report::{CardSnapshot, RunApplyStatus};

#[derive(Clone, Copy)]
pub(super) struct CombatCandidateLinePerformance {
    pub(super) nodes_expanded: u64,
    pub(super) nodes_generated: u64,
    pub(super) total_us: u64,
}

pub(super) fn combat_automation_opportunity_state_v1(
    session: &RunControlSession,
) -> Option<CombatAutomationOpportunityStateV1> {
    let active = session.active_combat.as_ref()?;
    let combat = &active.combat_state;
    // This trace field describes actions that can be taken directly from the
    // player-turn boundary.  A PendingChoice temporarily owns input, so cards
    // and potions are not playable there.  The trace therefore reads only the
    // finite player-turn actions and never asks a selection family to become a
    // candidate vector.
    let legal_moves = if matches!(active.engine_state, EngineState::CombatPlayerTurn) {
        engine_atomic_actions(&active.engine_state, combat)
    } else {
        Vec::new()
    };
    let mut playable_card_lineages = legal_moves
        .iter()
        .filter_map(|input| match input {
            ClientInput::PlayCard { card_index, .. } => {
                combat.zones.hand.get(*card_index).map(|card| card.lineage)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    playable_card_lineages.sort_unstable();
    playable_card_lineages.dedup();
    let mut usable_potion_uuids = legal_moves
        .iter()
        .filter_map(|input| match input {
            ClientInput::UsePotion { potion_index, .. } => combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .map(|potion| potion.uuid),
            _ => None,
        })
        .collect::<Vec<_>>();
    usable_potion_uuids.sort_unstable();
    usable_potion_uuids.dedup();

    Some(CombatAutomationOpportunityStateV1 {
        turn: combat.turn.turn_count,
        energy: combat.turn.energy,
        hand: combat
            .zones
            .hand
            .iter()
            .map(|card| CardSnapshot {
                id: card.id,
                lineage: card.lineage,
                upgrades: card.upgrades,
            })
            .collect(),
        potions: combat
            .entities
            .potions
            .iter()
            .map(|slot| {
                slot.as_ref().map(|potion| CombatAutomationPotionStateV1 {
                    id: potion.id,
                    uuid: potion.uuid,
                })
            })
            .collect(),
        playable_card_lineages,
        usable_potion_uuids,
    })
}

pub(super) fn combat_automation_answer_claims_v1(
    master_deck: &[CombatCard],
    actions: &[CombatAutomationActionV1],
) -> Vec<CombatAutomationAnswerClaimV1> {
    let mut claims = Vec::new();
    for card in master_deck {
        let peers = master_deck
            .iter()
            .filter(|peer| peer.lineage != card.lineage)
            .cloned()
            .collect::<Vec<_>>();
        push_card_claim(
            &mut claims,
            &peers,
            card.id,
            card.lineage,
            card.upgrades,
            CombatAutomationCardOriginV1::MasterDeck,
        );
    }

    for opportunity in actions
        .iter()
        .filter_map(|action| action.opportunity_before.as_ref())
    {
        for card in &opportunity.hand {
            if !claims.iter().any(|claim| {
                matches!(claim.source, CombatAutomationAnswerSourceV1::Card { lineage, .. } if lineage == card.lineage)
            }) {
                push_card_claim(
                    &mut claims,
                    master_deck,
                    card.id,
                    card.lineage,
                    card.upgrades,
                    CombatAutomationCardOriginV1::CombatGenerated,
                );
            }
        }
        for potion in opportunity.potions.iter().flatten() {
            if claims.iter().any(|claim| {
                matches!(claim.source, CombatAutomationAnswerSourceV1::Potion { uuid, .. } if uuid == potion.uuid)
            }) {
                continue;
            }
            let axes = potion_answer_axes(potion.id);
            if !axes.is_empty() {
                claims.push(CombatAutomationAnswerClaimV1 {
                    source: CombatAutomationAnswerSourceV1::Potion {
                        id: potion.id,
                        uuid: potion.uuid,
                    },
                    axes,
                });
            }
        }
    }
    claims
}

fn push_card_claim(
    claims: &mut Vec<CombatAutomationAnswerClaimV1>,
    deck_context: &[CombatCard],
    id: crate::content::cards::CardId,
    lineage: CardLineageId,
    upgrades: u8,
    origin: CombatAutomationCardOriginV1,
) {
    let admission = assess_reward_admission_from_master_deck(deck_context, id, upgrades);
    let axes = assess_candidate_pressure_response(Some((id, upgrades)), &admission).axes;
    if axes.is_empty() {
        return;
    }
    claims.push(CombatAutomationAnswerClaimV1 {
        source: CombatAutomationAnswerSourceV1::Card {
            id,
            lineage,
            upgrades,
            origin,
        },
        axes,
    });
}

fn potion_answer_axes(potion: crate::content::potions::PotionId) -> Vec<PressureAxis> {
    let mut axes = Vec::new();
    for trait_ in potion_acquisition_traits_v1(potion) {
        let axis = match trait_ {
            PotionAcquisitionTraitV1::CombatDamage | PotionAcquisitionTraitV1::VulnerableSetup => {
                Some(PressureAxis::ResolutionTempo)
            }
            PotionAcquisitionTraitV1::AoeDamage => Some(PressureAxis::MultiTargetControl),
            PotionAcquisitionTraitV1::CombatBlock
            | PotionAcquisitionTraitV1::WeakControl
            | PotionAcquisitionTraitV1::DeathInsurance => Some(PressureAxis::DelayCapacity),
            PotionAcquisitionTraitV1::EnergyBurst
            | PotionAcquisitionTraitV1::CardAccess
            | PotionAcquisitionTraitV1::CardDiscovery
            | PotionAcquisitionTraitV1::ExhaustControl
            | PotionAcquisitionTraitV1::ActionAmplifier
            | PotionAcquisitionTraitV1::DebuffControl => Some(PressureAxis::Deployability),
            PotionAcquisitionTraitV1::StrengthGain => Some(PressureAxis::GrowthHorizon),
            // Whole-hand upgrades can improve damage, defense, draw, or cost
            // depending on the actual hand. Do not claim one pressure axis
            // from acquisition metadata alone; exact potion effects remain
            // visible to combat evaluation.
            PotionAcquisitionTraitV1::CombatUpgrade
            | PotionAcquisitionTraitV1::EscapeTool
            | PotionAcquisitionTraitV1::PermanentMaxHp => None,
        };
        if let Some(axis) = axis {
            if !axes.contains(&axis) {
                axes.push(axis);
            }
        }
    }
    axes.sort();
    axes
}

pub(super) fn combat_automation_step_state_v1(
    session: &RunControlSession,
) -> Option<CombatAutomationStepStateV1> {
    let combat = &session.active_combat.as_ref()?.combat_state;
    Some(CombatAutomationStepStateV1 {
        player_hp: combat.entities.player.current_hp,
        player_max_hp: combat.entities.player.max_hp,
        player_block: combat.entities.player.block,
        energy: combat.turn.energy,
        cards_played_this_turn: combat.turn.counters.cards_played_this_turn,
        early_end_turn_pending: combat.turn.counters.early_end_turn_pending,
        monsters: combat
            .entities
            .monsters
            .iter()
            .map(|monster| CombatAutomationMonsterStateV1 {
                id: monster.id,
                label: monster.enemy_id.get_name().to_string(),
                hp: monster.current_hp,
                max_hp: monster.max_hp,
                block: monster.block,
                alive: monster.is_alive_for_action(),
                time_warp: store::power_amount(combat, monster.id, PowerId::TimeWarp),
                strength: store::power_amount(combat, monster.id, PowerId::Strength),
            })
            .collect(),
    })
}

pub(super) fn combat_search_performance_trace_annotation(
    source: impl Into<String>,
    session: &RunControlSession,
    start: &CombatPosition,
    report: &CombatSearchV2Report,
) -> RunControlTraceAnnotationV1 {
    RunControlTraceAnnotationV1::CombatSearchPerformance {
        snapshot: combat_search_performance_snapshot(source.into(), session, start, report),
    }
}

pub(super) fn attach_execution_adjudication(
    annotations: &mut [RunControlTraceAnnotationV1],
    adjudication: &CombatLineAdjudicationV1,
) {
    if let Some(snapshot) = annotations
        .iter_mut()
        .rev()
        .find_map(|annotation| match annotation {
            RunControlTraceAnnotationV1::CombatSearchPerformance { snapshot } => Some(snapshot),
            _ => None,
        })
    {
        snapshot.execution_adjudication = Some(adjudication.clone());
    }
}

pub(super) fn combat_line_performance_trace_annotation(
    source: impl Into<String>,
    session: &RunControlSession,
    start: &CombatPosition,
    report: &CombatSearchV2Report,
    selected_line: &CombatCandidateLine,
    line_performance: Option<CombatCandidateLinePerformance>,
) -> RunControlTraceAnnotationV1 {
    let source = source.into();
    let mut snapshot = combat_search_performance_snapshot(source.clone(), session, start, report);
    let line_summary = combat_candidate_line_summary(selected_line);
    snapshot.complete_trajectory_found = true;
    snapshot.complete_win_found = selected_line.terminal == CombatTerminal::Win;
    snapshot.best_complete = Some(line_summary.clone());
    snapshot.best_win = (selected_line.terminal == CombatTerminal::Win).then_some(line_summary);
    snapshot.best_hp_loss =
        (selected_line.terminal == CombatTerminal::Win).then_some(selected_line.hp_loss);
    if let Some(performance) = line_performance {
        snapshot.coverage_status = format!("{source}Applied");
        snapshot.nodes_expanded = performance.nodes_expanded;
        snapshot.nodes_generated = performance.nodes_generated;
        snapshot.terminal_wins = u64::from(selected_line.terminal == CombatTerminal::Win);
        snapshot.total_us = performance.total_us;
        snapshot.unattributed_us = performance.total_us;
        snapshot.rollout_us = 0;
        snapshot.expansion_us = 0;
        snapshot.child_bookkeeping_us = 0;
        snapshot.engine_step_us = 0;
        snapshot.pre_expand_us = 0;
        snapshot.frontier_pop_us = 0;
        snapshot.turn_plan_seed_us = 0;
        snapshot.shadow_audit_us = 0;
        snapshot.root_turn_plan_diag_us = 0;
    }
    RunControlTraceAnnotationV1::CombatSearchPerformance { snapshot }
}

fn combat_search_performance_snapshot(
    source: String,
    session: &RunControlSession,
    start: &CombatPosition,
    report: &CombatSearchV2Report,
) -> CombatSearchPerformanceSnapshotV1 {
    let combat = &start.combat;
    CombatSearchPerformanceSnapshotV1 {
        source,
        act: session.run_state.act_num,
        floor: session.run_state.floor_num,
        turn: combat.turn.turn_count,
        combat_kind: combat_kind_label(combat),
        enemies: combat_enemy_names(combat),
        boss: session
            .run_state
            .boss_key
            .map(|boss| boss.stable_id().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        external_payoff_opportunity: has_external_payoff_opportunity(combat),
        coverage_status: format!("{:?}", report.outcome.coverage_status),
        complete_trajectory_found: report.outcome.complete_trajectory_found,
        complete_win_found: report.outcome.complete_win_found,
        best_complete: report
            .best_complete_trajectory
            .as_ref()
            .map(combat_search_line_summary),
        best_win: report
            .best_win_trajectory
            .as_ref()
            .map(combat_search_line_summary),
        best_hp_loss: report
            .best_win_trajectory
            .as_ref()
            .map(|trajectory| trajectory.hp_loss),
        execution_adjudication: None,
        nodes_to_first_win: report.stats.nodes_to_first_win,
        deadline_hit: report.stats.deadline_hit,
        node_budget_hit: report.stats.node_budget_hit,
        quantum_history: report.quantum_history.clone(),
        final_root_evidence: Some(report.final_root_evidence.clone()),
        nodes_expanded: report.stats.nodes_expanded,
        nodes_generated: report.stats.nodes_generated,
        terminal_wins: report.stats.terminal_wins,
        total_us: micros_to_u64(report.performance.total_elapsed_us),
        unattributed_us: micros_to_u64(report.performance.unattributed_elapsed_us),
        report_finalization_us: micros_to_u64(report.performance.report_finalization_elapsed_us),
        report_frontier_scan_us: micros_to_u64(report.performance.report_frontier_scan_elapsed_us),
        report_search_storage_drop_us: micros_to_u64(
            report.performance.report_search_storage_drop_elapsed_us,
        ),
        rollout_calls: report.performance.rollout_estimate_calls,
        root_rollout_calls: report.performance.root_rollout_estimate_calls,
        child_rollout_calls: report.performance.child_rollout_estimate_calls,
        deferred_child_rollout_calls: report.performance.deferred_child_rollout_estimate_calls,
        turn_plan_seed_rollout_calls: report.performance.turn_plan_seed_rollout_estimate_calls,
        deferred_child_rollout_nodes: report.performance.deferred_child_rollout_nodes,
        deferred_child_rollout_requeues: report.performance.deferred_child_rollout_requeues,
        rollout_cache_hits: report.rollout.cache_hits,
        rollout_cache_queries: report.rollout.cache_queries,
        rollout_cache_misses: report.rollout.cache_misses,
        rollout_cache_inserts: report.rollout.cache_inserts,
        rollout_budget_skips: report.rollout.budget_skips,
        rollout_max_evaluation_budget_skips: report.rollout.max_evaluation_budget_skips,
        rollout_deadline_budget_skips: report.rollout.deadline_budget_skips,
        rollout_truncated: report.rollout.truncated_rollouts,
        rollout_terminal_wins: report.rollout.terminal_wins,
        rollout_cache_lookup_us: micros_to_u64(report.rollout.performance.cache_lookup_us),
        rollout_policy_dispatch_us: micros_to_u64(report.rollout.performance.policy_dispatch_us),
        rollout_no_potion_iterations: report.rollout.performance.no_potion_iterations,
        rollout_no_potion_phase_profile_us: micros_to_u64(
            report.rollout.performance.no_potion_phase_profile_us,
        ),
        rollout_no_potion_legal_actions_us: micros_to_u64(
            report.rollout.performance.no_potion_legal_actions_us,
        ),
        rollout_no_potion_choose_action_us: micros_to_u64(
            report.rollout.performance.no_potion_choose_action_us,
        ),
        rollout_no_potion_choose_ordering_us: micros_to_u64(
            report.rollout.performance.no_potion_choose_ordering_us,
        ),
        rollout_no_potion_probe_us: micros_to_u64(report.rollout.performance.no_potion_probe_us),
        rollout_no_potion_probe_score_calls: report.rollout.performance.no_potion_probe_score_calls,
        rollout_no_potion_probe_actions_evaluated: report
            .rollout
            .performance
            .no_potion_probe_actions_evaluated,
        rollout_no_potion_probe_step_reuses: report.rollout.performance.no_potion_probe_step_reuses,
        rollout_no_potion_probe_engine_step_us: micros_to_u64(
            report.rollout.performance.no_potion_probe_engine_step_us,
        ),
        rollout_no_potion_probe_phase_profile_us: micros_to_u64(
            report.rollout.performance.no_potion_probe_phase_profile_us,
        ),
        rollout_no_potion_probe_action_facts_us: micros_to_u64(
            report.rollout.performance.no_potion_probe_action_facts_us,
        ),
        rollout_no_potion_engine_step_us: micros_to_u64(
            report.rollout.performance.no_potion_engine_step_us,
        ),
        rollout_no_potion_child_build_us: micros_to_u64(
            report.rollout.performance.no_potion_child_build_us,
        ),
        terminal_child_rollout_skips: report.performance.terminal_child_rollout_skips,
        terminal_turn_plan_seed_rollout_skips: report
            .performance
            .terminal_turn_plan_seed_rollout_skips,
        turn_local_dominance_rollout_skips: report.performance.turn_local_dominance_rollout_skips,
        rollout_us: micros_to_u64(report.performance.rollout_estimate_elapsed_us),
        expansion_us: micros_to_u64(report.performance.expansion_elapsed_us),
        child_bookkeeping_us: micros_to_u64(report.performance.child_bookkeeping_elapsed_us),
        engine_step_us: micros_to_u64(report.performance.engine_step_elapsed_us),
        pre_expand_us: micros_to_u64(report.performance.pre_expand_elapsed_us),
        frontier_pop_us: micros_to_u64(report.performance.frontier_pop_elapsed_us),
        turn_plan_seed_us: micros_to_u64(report.performance.turn_plan_frontier_seed_elapsed_us),
        shadow_audit_us: micros_to_u64(report.performance.shadow_audit_elapsed_us),
        root_turn_plan_diag_us: micros_to_u64(
            report.performance.root_turn_plan_diagnostics_elapsed_us,
        ),
    }
}

fn combat_kind_label(combat: &crate::runtime::combat::CombatState) -> String {
    if combat.meta.is_boss_fight {
        "boss".to_string()
    } else if combat.meta.is_elite_fight {
        "elite".to_string()
    } else {
        "hallway".to_string()
    }
}

fn combat_enemy_names(combat: &crate::runtime::combat::CombatState) -> Vec<String> {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.current_hp > 0 && !monster.is_escaped)
        .map(|monster| monster.enemy_id.get_name().to_string())
        .collect()
}

pub(super) fn combat_search_line_summary(
    trajectory: &crate::ai::combat_search_v2::CombatSearchV2TrajectoryReport,
) -> CombatSearchTerminalLineSummary {
    CombatSearchTerminalLineSummary {
        terminal: trajectory.terminal,
        final_hp: trajectory.final_hp,
        hp_loss: trajectory.hp_loss,
        turns: trajectory.turns,
        cards_played: trajectory.cards_played,
        potions_used: trajectory.potions_used,
        potions_discarded: trajectory.potions_discarded,
        action_count: trajectory.actions.len(),
    }
}

pub(super) fn combat_candidate_line_summary(
    line: &CombatCandidateLine,
) -> CombatSearchTerminalLineSummary {
    CombatSearchTerminalLineSummary {
        terminal: match line.terminal {
            CombatTerminal::Win => SearchTerminalLabel::Win,
            CombatTerminal::Loss => SearchTerminalLabel::Loss,
            CombatTerminal::Unresolved => SearchTerminalLabel::Unresolved,
        },
        final_hp: line.final_hp,
        hp_loss: line.hp_loss,
        turns: line.turns,
        cards_played: line.cards_played,
        potions_used: line.potions_used,
        potions_discarded: line.potions_discarded,
        action_count: line.actions.len(),
    }
}

pub(super) fn current_run_apply_status(session: &RunControlSession) -> RunApplyStatus {
    match session.engine_state {
        EngineState::GameOver(RunResult::Victory) => RunApplyStatus::Victory,
        EngineState::GameOver(RunResult::Defeat) => RunApplyStatus::Defeat,
        _ => RunApplyStatus::Running,
    }
}

pub(super) fn millis_to_micros_u64(value: u128) -> u64 {
    micros_to_u64(value.saturating_mul(1_000))
}

fn micros_to_u64(value: u128) -> u64 {
    value.min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::combat_search_v2::{run_combat_search_v2, CombatSearchV2Config};
    use crate::eval::run_control::combat_candidate_line::CombatCandidateLineSource;
    use crate::eval::run_control::{
        atomic_combat_search_trace_summaries, AtomicCombatSearchTraceSummaryV2,
    };
    use crate::state::core::EngineState;

    #[test]
    fn combat_search_trace_preserves_node_budget_hit() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters.clear();
        let start = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let mut report = run_combat_search_v2(
            &start.engine,
            &start.combat,
            CombatSearchV2Config {
                max_nodes: 1,
                ..CombatSearchV2Config::default()
            },
        );
        report.stats.node_budget_hit = true;
        let session = RunControlSession::new(Default::default());
        let annotations = vec![combat_search_performance_trace_annotation(
            "search_combat",
            &session,
            &start,
            &report,
        )];

        let RunControlTraceAnnotationV1::CombatSearchPerformance { snapshot } = &annotations[0]
        else {
            panic!("expected combat search performance annotation")
        };
        let summary = atomic_combat_search_trace_summaries(&annotations)
            .next()
            .expect("combat search summary");

        assert!(snapshot.node_budget_hit);
        assert!(summary.node_budget_hit);
    }

    #[test]
    fn legacy_combat_search_trace_defaults_node_budget_hit_to_false() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters.clear();
        let start = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let mut report = run_combat_search_v2(
            &start.engine,
            &start.combat,
            CombatSearchV2Config {
                max_nodes: 1,
                ..CombatSearchV2Config::default()
            },
        );
        report.stats.node_budget_hit = true;
        let session = RunControlSession::new(Default::default());
        let annotation =
            combat_search_performance_trace_annotation("legacy", &session, &start, &report);
        let RunControlTraceAnnotationV1::CombatSearchPerformance { snapshot } = annotation else {
            panic!("expected combat search performance annotation")
        };
        let mut snapshot_value = serde_json::to_value(snapshot).expect("serialize snapshot");
        snapshot_value
            .as_object_mut()
            .expect("snapshot object")
            .remove("node_budget_hit");
        for field in [
            "report_finalization_us",
            "report_frontier_scan_us",
            "report_search_storage_drop_us",
        ] {
            snapshot_value
                .as_object_mut()
                .expect("snapshot object")
                .remove(field);
        }
        let restored_snapshot: CombatSearchPerformanceSnapshotV1 =
            serde_json::from_value(snapshot_value).expect("legacy snapshot");

        let mut summary_value = serde_json::to_value(AtomicCombatSearchTraceSummaryV2 {
            coverage_status: "NodeBudgetLimited".to_string(),
            node_budget_hit: true,
            ..AtomicCombatSearchTraceSummaryV2::default()
        })
        .expect("serialize summary");
        summary_value
            .as_object_mut()
            .expect("summary object")
            .remove("node_budget_hit");
        let restored_summary: AtomicCombatSearchTraceSummaryV2 =
            serde_json::from_value(summary_value).expect("legacy summary");

        assert!(!restored_snapshot.node_budget_hit);
        assert_eq!(restored_snapshot.report_finalization_us, 0);
        assert_eq!(restored_snapshot.report_frontier_scan_us, 0);
        assert_eq!(restored_snapshot.report_search_storage_drop_us, 0);
        assert!(!restored_summary.node_budget_hit);
    }

    #[test]
    fn selected_line_snapshot_keeps_report_performance() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters.clear();
        let start = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let report = run_combat_search_v2(
            &start.engine,
            &start.combat,
            CombatSearchV2Config {
                max_nodes: 1,
                ..CombatSearchV2Config::default()
            },
        );
        let mut selected_position = start.clone();
        selected_position.engine = EngineState::GameOver(crate::state::core::RunResult::Victory);
        selected_position.combat.entities.player.current_hp = 65;
        let selected = CombatCandidateLine::from_position(
            CombatCandidateLineSource::SearchComplete,
            Vec::new(),
            80,
            &selected_position,
        );
        let session = RunControlSession::new(Default::default());

        let annotation = combat_line_performance_trace_annotation(
            "search_combat",
            &session,
            &start,
            &report,
            &selected,
            None,
        );
        let RunControlTraceAnnotationV1::CombatSearchPerformance { snapshot } = annotation else {
            panic!("expected combat search performance annotation")
        };

        assert_eq!(snapshot.best_hp_loss, Some(15));
        assert_eq!(snapshot.nodes_expanded, report.stats.nodes_expanded);
        assert_eq!(
            snapshot.report_finalization_us,
            micros_to_u64(report.performance.report_finalization_elapsed_us)
        );
        assert_eq!(
            snapshot.report_frontier_scan_us,
            micros_to_u64(report.performance.report_frontier_scan_elapsed_us)
        );
        assert_eq!(
            snapshot.report_search_storage_drop_us,
            micros_to_u64(report.performance.report_search_storage_drop_elapsed_us)
        );
        assert_eq!(snapshot.quantum_history, report.quantum_history);
        assert_eq!(
            snapshot.final_root_evidence.as_ref(),
            Some(&report.final_root_evidence)
        );
    }

    #[test]
    fn combat_search_trace_round_trips_dirty_adjudication() {
        use crate::ai::combat_search_v2::CombatSearchAcceptancePluginId;
        use crate::content::cards::CardId;
        use crate::eval::run_control::{
            CombatLineAdjudicationV1, CombatLineCleanlinessV1, CombatLineObservedOutcomeV1,
            RunActionCardSnapshotV1,
        };

        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters.clear();
        let start = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let report = run_combat_search_v2(
            &start.engine,
            &start.combat,
            CombatSearchV2Config {
                max_nodes: 1,
                ..CombatSearchV2Config::default()
            },
        );
        let session = RunControlSession::new(Default::default());
        let adjudication = CombatLineAdjudicationV1::Accepted {
            policy: CombatSearchAcceptancePluginId::AcceptedLineOnly,
            cleanliness: CombatLineCleanlinessV1::Dirty,
            observed_outcome: CombatLineObservedOutcomeV1 {
                terminal: CombatTerminal::Win,
                final_hp: 44,
                hp_loss: 0,
                potions_used: 0,
                action_count: 32,
                gold_delta: 0,
                ritual_dagger_growth: 0,
                gained_curses: vec![RunActionCardSnapshotV1 {
                    id: CardId::Parasite,
                    lineage: crate::test_support::scenario_lineage(9001),
                    upgrades: 0,
                }],
            },
        };
        let mut annotations = vec![combat_search_performance_trace_annotation(
            "search_combat_rejected_dirty_win",
            &session,
            &start,
            &report,
        )];

        attach_execution_adjudication(&mut annotations, &adjudication);

        let json = serde_json::to_string(&annotations[0]).expect("serialize trace annotation");
        let restored: RunControlTraceAnnotationV1 =
            serde_json::from_str(&json).expect("deserialize trace annotation");
        assert_eq!(restored, annotations[0]);
        assert!(json.contains("Parasite"));
        assert!(json.contains("accepted_line_only"));
    }
}
