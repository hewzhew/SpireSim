//! Typed assertions and compact results for exact combat-case contracts.

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use sts_combat_planner::{LocalTurnGraphPolicyLineReport, LocalTurnGraphWitnessReport};

pub(super) struct LocalGraphContractRequest<'a> {
    pub(super) case: &'a Path,
    pub(super) elapsed: Duration,
    pub(super) report: &'a LocalTurnGraphWitnessReport,
    pub(super) policy_line: Option<&'a LocalTurnGraphPolicyLineReport>,
    pub(super) expect_witness: bool,
    pub(super) expect_min_final_hp: Option<i32>,
    pub(super) contract_only: bool,
}

pub(super) fn evaluate_local_graph_contract(
    request: LocalGraphContractRequest<'_>,
) -> Result<Option<Value>, String> {
    let LocalGraphContractRequest {
        case,
        elapsed,
        report,
        policy_line,
        expect_witness,
        expect_min_final_hp,
        contract_only,
    } = request;

    if expect_witness && report.witness.is_none() {
        return Err("combat-case contract failed: no replay-verified witness".to_owned());
    }
    if let Some(expected_minimum) = expect_min_final_hp {
        let actual = report
            .witness
            .as_ref()
            .map(|witness| witness.final_position.combat.entities.player.current_hp)
            .ok_or_else(|| {
                "combat-case contract failed: final HP requires a verified witness".to_owned()
            })?;
        if actual < expected_minimum {
            return Err(format!(
                "combat-case contract failed: final HP {actual} is below {expected_minimum}"
            ));
        }
    }
    if !contract_only {
        return Ok(None);
    }

    let witness = report.witness.as_ref().ok_or_else(|| {
        "combat-case contract failed: compact result requires a verified witness".to_owned()
    })?;
    Ok(Some(json!({
        "schema_name": "CombatCaseContractResultV1",
        "schema_version": 1,
        "status": "passed",
        "case": case,
        "elapsed_ms": elapsed.as_millis(),
        "final_hp": witness.final_position.combat.entities.player.current_hp,
        "witness_actions": witness.actions.len(),
        "plan_prefix": policy_line.map(|policy_line| json!({
            "proposed_turns": policy_line.proposed_turns,
            "chosen_action_transitions": policy_line.chosen_action_transitions,
            "engine_steps": policy_line.engine_steps,
            "reached_terminal_win": policy_line.reached_terminal_win,
        })),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_planner::{LocalTurnGraphWitnessCounters, LocalTurnGraphWitnessStatus};

    fn report_without_witness() -> LocalTurnGraphWitnessReport {
        LocalTurnGraphWitnessReport {
            status: LocalTurnGraphWitnessStatus::FrontierExhausted,
            counters: LocalTurnGraphWitnessCounters::default(),
            performance_timing: Default::default(),
            root_visits: 0,
            root_generated_options: 0,
            root_children: 0,
            generation_gaps: Vec::new(),
            witness: None,
            witness_frontier: Vec::new(),
        }
    }

    fn request<'a>(
        report: &'a LocalTurnGraphWitnessReport,
        policy_line: Option<&'a LocalTurnGraphPolicyLineReport>,
    ) -> LocalGraphContractRequest<'a> {
        LocalGraphContractRequest {
            case: Path::new("fixture.combat.json"),
            elapsed: Duration::ZERO,
            report,
            policy_line,
            expect_witness: false,
            expect_min_final_hp: None,
            contract_only: false,
        }
    }

    #[test]
    fn required_witness_rejects_budget_unknown() {
        let report = report_without_witness();
        let mut request = request(&report, None);
        request.expect_witness = true;

        assert_eq!(
            evaluate_local_graph_contract(request),
            Err("combat-case contract failed: no replay-verified witness".to_owned())
        );
    }

    #[test]
    fn minimum_final_hp_requires_a_verified_witness() {
        let report = report_without_witness();
        let mut request = request(&report, None);
        request.expect_min_final_hp = Some(1);

        assert_eq!(
            evaluate_local_graph_contract(request),
            Err("combat-case contract failed: final HP requires a verified witness".to_owned())
        );
    }

    #[test]
    fn inactive_contract_does_not_emit_a_compact_result() {
        let report = report_without_witness();

        assert_eq!(
            evaluate_local_graph_contract(request(&report, None)),
            Ok(None)
        );
    }
}
