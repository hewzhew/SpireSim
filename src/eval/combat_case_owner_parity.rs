use serde::Serialize;

use crate::eval::combat_case::CombatCase;
use crate::eval::combat_case_context::restore_combat_case_oracle_analysis_owner_v2;
use crate::eval::run_control::{
    seed_oracle_run_explorer_from_session_v1, CombatAutomationTrajectorySource,
    RunProgressJournalV1,
};
use crate::state::core::ClientInput;

use super::oracle_analysis_session::{
    OracleAnalysisAdvanceReportV1, OracleAnalysisAdvanceRequestV1, OracleAnalysisAdvanceStatusV1,
    OracleAnalysisCombatStageTraceV1, OracleAnalysisSessionCheckpointV1, OracleAnalysisSessionV1,
};

pub const COMBAT_CASE_OWNER_PARITY_REPORT_SCHEMA_NAME: &str = "CombatCaseOwnerParityReportV1";
pub const COMBAT_CASE_OWNER_PARITY_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct CombatCaseOwnerParityRequestV1 {
    pub advance: OracleAnalysisAdvanceRequestV1,
    pub keep_debug_checkpoint: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseOwnerParityReportV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub root_exact_state_hash: String,
    pub run_session_fingerprint: String,
    pub owner_policy_fingerprint: String,
    pub status: OracleAnalysisAdvanceStatusV1,
    pub quanta_served: usize,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatCaseOwnerParityCombatV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub witness: Option<CombatCaseOwnerParityWitnessV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseOwnerParityCombatV1 {
    pub stage_trace: Vec<OracleAnalysisCombatStageTraceV1>,
    pub search_stage: u8,
    pub generation_work: u64,
    pub exact_states: usize,
    pub completed_turn_options: usize,
    pub max_player_turn: u32,
    pub incumbent_final_hp: Option<i32>,
    pub incumbent_hp_loss: Option<i32>,
    pub incumbent_action_count: Option<usize>,
    pub incumbent_potions_used: Option<u32>,
    pub incumbent_potion_slots: Option<u64>,
    pub incumbent_satisfies_satisfaction: Option<bool>,
    pub incumbent_ends_quality_refinement: Option<bool>,
    pub last_status: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseOwnerParityWitnessV1 {
    pub source: CombatAutomationTrajectorySource,
    pub action_count: usize,
    pub inputs: Vec<ClientInput>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseOwnerParityDebugV1 {
    pub advance_report: OracleAnalysisAdvanceReportV1,
    pub analysis_checkpoint: OracleAnalysisSessionCheckpointV1,
}

pub struct CombatCaseOwnerParityRunV1 {
    pub report: CombatCaseOwnerParityReportV1,
    pub debug: Option<CombatCaseOwnerParityDebugV1>,
}

pub fn run_combat_case_owner_parity_v1(
    case: &CombatCase,
    request: CombatCaseOwnerParityRequestV1,
) -> Result<CombatCaseOwnerParityRunV1, String> {
    let identity = case.replay_identity_v2()?;
    let run_session_fingerprint = identity
        .run_session_fingerprint
        .ok_or_else(|| "combat-case owner parity requires exact production context".to_string())?;
    let owner_policy_fingerprint = identity.owner_policy_fingerprint.ok_or_else(|| {
        "combat case has exact production state but no production owner".to_string()
    })?;
    let root_exact_state_hash = identity.root_exact_state_hash;
    let (session, budgets) =
        restore_combat_case_oracle_analysis_owner_v2(&case.core, case.production_context.as_ref())?;
    let explorer = seed_oracle_run_explorer_from_session_v1(
        session,
        RunProgressJournalV1::default(),
        &budgets,
        None,
    )?;
    let mut analysis = OracleAnalysisSessionV1::from_explorer(explorer, None, budgets, None, None)?;
    let advance_report = analysis.advance_cursor(request.advance)?;
    let witness = match advance_report.status {
        OracleAnalysisAdvanceStatusV1::BoundaryReached { child_node_id } => analysis
            .combat_trajectory(child_node_id)?
            .map(|trajectory| CombatCaseOwnerParityWitnessV1 {
                source: trajectory.source,
                action_count: trajectory.action_count,
                inputs: trajectory
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect(),
            }),
        _ => None,
    };
    let combat = advance_report
        .combat
        .as_ref()
        .map(|combat| CombatCaseOwnerParityCombatV1 {
            stage_trace: combat.stage_trace.clone(),
            search_stage: combat.search_stage,
            generation_work: combat.generation_work,
            exact_states: combat.exact_states,
            completed_turn_options: combat.completed_turn_options,
            max_player_turn: combat.max_player_turn,
            incumbent_final_hp: combat.incumbent_final_hp,
            incumbent_hp_loss: combat.incumbent_hp_loss,
            incumbent_action_count: combat.incumbent_action_count,
            incumbent_potions_used: combat.incumbent_potions_used,
            incumbent_potion_slots: combat.incumbent_potion_slots,
            incumbent_satisfies_satisfaction: combat.incumbent_satisfies_satisfaction,
            incumbent_ends_quality_refinement: combat.incumbent_ends_quality_refinement,
            last_status: combat.last_status.clone(),
        });
    let report = CombatCaseOwnerParityReportV1 {
        schema_name: COMBAT_CASE_OWNER_PARITY_REPORT_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_CASE_OWNER_PARITY_REPORT_SCHEMA_VERSION,
        root_exact_state_hash,
        run_session_fingerprint,
        owner_policy_fingerprint,
        status: advance_report.status,
        quanta_served: advance_report.quanta_served,
        elapsed_ms: advance_report.elapsed_ms,
        combat,
        witness,
    };
    let debug = if request.keep_debug_checkpoint {
        Some(CombatCaseOwnerParityDebugV1 {
            advance_report,
            analysis_checkpoint: analysis.checkpoint()?,
        })
    } else {
        None
    };
    Ok(CombatCaseOwnerParityRunV1 { report, debug })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::eval::combat_case::{
        CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary, CombatCaseSource,
        CombatCaseWitnessBudgetV1,
    };
    use crate::eval::combat_case_context::{
        capture_combat_case_production_context_v2,
        capture_oracle_analysis_combat_case_production_context_v2,
    };
    use crate::eval::run_control::{
        OracleCombatWitnessOptionsV1, OracleRunCombatWitnessBudgetsV1, RunControlConfig,
        RunControlSession,
    };
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;

    fn fixture() -> (
        CombatCase,
        RunControlSession,
        OracleRunCombatWitnessBudgetsV1,
    ) {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
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
        monster.current_hp = 1;
        monster.max_hp = 1;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        combat.update_hand_cards();
        let mut session = RunControlSession::new(RunControlConfig {
            seed: 73,
            ascension_level: 0,
            player_class: "Ironclad",
            ..RunControlConfig::default()
        });
        session.run_state.act_num = 1;
        session.run_state.floor_num = 1;
        session.run_state.current_hp = combat.entities.player.current_hp;
        session.run_state.max_hp = combat.entities.player.max_hp;
        session.run_state.gold = combat.entities.player.gold;
        session.run_state.master_deck = combat.meta.master_deck_snapshot.to_vec();
        session.run_state.rng_pool = combat.rng.pool.clone();
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let position = session.current_active_combat_position().unwrap();
        let case = CombatCase::new(
            CombatCaseSource {
                seed: 73,
                ascension: 0,
                generation: 1,
                branch_id: 0,
                parent_id: None,
            },
            CombatCaseGap {
                boundary: "test".to_string(),
                reason: "owner_parity_contract".to_string(),
                witness_budget: CombatCaseWitnessBudgetV1::AtomicExactV2 {
                    primary_nodes: 128,
                    primary_wall_ms: 1_000,
                    rescue_nodes: 0,
                    rescue_wall_ms: 0,
                },
            },
            CombatCaseRunSummary {
                act: 1,
                floor: 1,
                hp: session.run_state.current_hp,
                max_hp: session.run_state.max_hp,
                gold: session.run_state.gold,
                deck_size: session.run_state.master_deck.len(),
                relic_count: session.run_state.relics.len(),
                potion_slots: session.run_state.potions.len(),
            },
            Vec::new(),
            None,
            Vec::new(),
            CombatCaseRngSummary::from_pool(&session.run_state.rng_pool),
            position,
        );
        let budgets = OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(128),
            wall_ms: Some(1_000),
            satisfaction: Some(
                crate::ai::combat_witness_contract::CombatWitnessSatisfactionV1::FirstCompleteWin,
            ),
            ..OracleCombatWitnessOptionsV1::default()
        });
        (case, session, budgets)
    }

    #[test]
    fn owner_parity_rejects_exact_state_without_owner_policy() {
        let (mut case, session, _) = fixture();
        case.production_context =
            Some(capture_combat_case_production_context_v2(&case.core, &session).unwrap());

        let error =
            run_combat_case_owner_parity_v1(&case, CombatCaseOwnerParityRequestV1::default())
                .err()
                .unwrap();

        assert!(error.contains("no production owner"), "{error}");
    }

    #[test]
    fn owner_parity_serves_one_in_memory_attempt_and_returns_exact_inputs() {
        let (mut case, session, budgets) = fixture();
        case.production_context = Some(
            capture_oracle_analysis_combat_case_production_context_v2(
                &case.core, &session, &budgets,
            )
            .unwrap(),
        );

        let result = run_combat_case_owner_parity_v1(
            &case,
            CombatCaseOwnerParityRequestV1 {
                advance: OracleAnalysisAdvanceRequestV1 {
                    max_quanta: 2,
                    quantum_generation_work: 128,
                    quantum_ms: None,
                    wall_ms: None,
                    improve_incumbent: false,
                },
                keep_debug_checkpoint: false,
            },
        )
        .unwrap();

        assert!(matches!(
            result.report.status,
            OracleAnalysisAdvanceStatusV1::BoundaryReached { .. }
        ));
        assert_eq!(
            result.report.owner_policy_fingerprint,
            case.replay_identity_v2()
                .unwrap()
                .owner_policy_fingerprint
                .unwrap()
        );
        assert!(result.report.witness.is_some());
        assert!(result.debug.is_none());
    }
}
