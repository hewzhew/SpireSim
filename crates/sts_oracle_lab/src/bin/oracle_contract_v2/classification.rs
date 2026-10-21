use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sts_combat_planner::{
    LocalTurnGraphTerminalOutcomeSnapshotV1, LocalTurnGraphWitnessInterruption,
    LocalTurnGraphWitnessReport, LocalTurnGraphWitnessStatus, OracleCombatWitness,
    OracleCombatWitnessDiscoverySource,
};

use super::CombatContractRequestV2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CombatContractClassificationV2 {
    Passed,
    WitnessBelowFinalHp,
    WitnessSpentTooManyPotions,
    WitnessLeftStolenGold,
    FrontierExhausted,
    MechanicsGap,
    ReplayMismatch,
    BudgetUnknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CombatContractSearchStatusV2 {
    WitnessFound,
    SelectionBudget,
    GenerationWorkBudget,
    EngineStepBudget,
    Deadline,
    FrontierExhausted,
    MechanicsGap,
    ReplayMismatch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct CombatContractCandidateSummaryV2 {
    pub(super) final_hp: i32,
    pub(super) potions_used: u32,
    pub(super) unrecovered_stolen_gold: i32,
    pub(super) action_count: usize,
    pub(super) discovery_source: OracleCombatWitnessDiscoverySource,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct CombatContractResultV2 {
    pub(super) schema_name: String,
    pub(super) schema_version: u32,
    pub(super) artifact: PathBuf,
    pub(super) classification: CombatContractClassificationV2,
    pub(super) contract_passed: bool,
    pub(super) search_status: CombatContractSearchStatusV2,
    pub(super) candidate_witness: bool,
    pub(super) contract_witness: bool,
    pub(super) discovery_source: Option<OracleCombatWitnessDiscoverySource>,
    pub(super) final_hp: Option<i32>,
    pub(super) potions_used: Option<u32>,
    pub(super) unrecovered_stolen_gold: Option<i32>,
    pub(super) action_count: Option<usize>,
    pub(super) witness_actions: Option<PathBuf>,
    pub(super) terminal_outcome_count: usize,
    pub(super) resource_contract_candidate_count: usize,
    pub(super) local_hp_candidate: Option<CombatContractCandidateSummaryV2>,
    pub(super) search_root_exact_state_hash: Option<String>,
    pub(super) diagnostic_prefix_action_count: usize,
    pub(super) diagnostic_prefix_potions_used: u32,
    pub(super) generation_work: usize,
    pub(super) elapsed_ms: u128,
}

pub(super) struct CombatContractAssessmentV2 {
    pub(super) result: CombatContractResultV2,
    pub(super) selected_witness_index: Option<usize>,
    pub(super) local_hp_witness_index: Option<usize>,
}

pub(super) fn classify_contract(
    request: &CombatContractRequestV2,
    report: &LocalTurnGraphWitnessReport,
    witness_frontier: &[OracleCombatWitness],
    artifact: &Path,
    elapsed: Duration,
) -> CombatContractAssessmentV2 {
    let indexes = evidence_indexes(request, report, witness_frontier);
    let contract_index = indexes.contract;
    let candidate_index = indexes.selected;
    let outcome = candidate_index.and_then(|index| report.witness_frontier.get(index));
    let witness = candidate_index.and_then(|index| witness_frontier.get(index));
    let final_hp = outcome.map(|outcome| outcome.final_hp);
    let potions_used = outcome.map(|outcome| outcome.potion_expenditures);
    let unrecovered_stolen_gold = outcome.map(|outcome| outcome.unrecovered_stolen_gold);
    let classification = if contract_index.is_some() {
        CombatContractClassificationV2::Passed
    } else if let Some(outcome) = outcome {
        if outcome.potion_expenditures > request.max_potions_used {
            CombatContractClassificationV2::WitnessSpentTooManyPotions
        } else if request.require_recovered_stolen_gold && outcome.unrecovered_stolen_gold > 0 {
            CombatContractClassificationV2::WitnessLeftStolenGold
        } else if request
            .min_final_hp
            .is_some_and(|minimum| outcome.final_hp < minimum)
        {
            CombatContractClassificationV2::WitnessBelowFinalHp
        } else {
            CombatContractClassificationV2::BudgetUnknown
        }
    } else {
        match report.status {
            LocalTurnGraphWitnessStatus::MechanicsGap => {
                CombatContractClassificationV2::MechanicsGap
            }
            LocalTurnGraphWitnessStatus::ReplayMismatch(_) => {
                CombatContractClassificationV2::ReplayMismatch
            }
            LocalTurnGraphWitnessStatus::FrontierExhausted => {
                CombatContractClassificationV2::FrontierExhausted
            }
            _ => CombatContractClassificationV2::BudgetUnknown,
        }
    };
    CombatContractAssessmentV2 {
        result: CombatContractResultV2 {
            schema_name: "OracleCombatContractResultV2".to_owned(),
            schema_version: 3,
            artifact: artifact.to_path_buf(),
            classification,
            contract_passed: classification == CombatContractClassificationV2::Passed,
            search_status: search_status(&report.status),
            candidate_witness: candidate_index.is_some(),
            contract_witness: contract_index.is_some(),
            discovery_source: witness.map(|witness| witness.discovery_source),
            final_hp,
            potions_used,
            unrecovered_stolen_gold,
            action_count: outcome.map(|outcome| outcome.action_count),
            witness_actions: None,
            terminal_outcome_count: report.witness_frontier.len(),
            resource_contract_candidate_count: report
                .witness_frontier
                .iter()
                .filter(|outcome| outcome_satisfies_resource_contract(request, outcome))
                .count(),
            local_hp_candidate: indexes
                .local_hp
                .and_then(|index| candidate_summary(report, witness_frontier, index)),
            search_root_exact_state_hash: None,
            diagnostic_prefix_action_count: 0,
            diagnostic_prefix_potions_used: 0,
            generation_work: report.counters.generation_work,
            elapsed_ms: elapsed.as_millis(),
        },
        selected_witness_index: candidate_index,
        local_hp_witness_index: indexes.local_hp,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CombatContractEvidenceIndexesV2 {
    contract: Option<usize>,
    selected: Option<usize>,
    local_hp: Option<usize>,
}

fn evidence_indexes(
    request: &CombatContractRequestV2,
    report: &LocalTurnGraphWitnessReport,
    witness_frontier: &[OracleCombatWitness],
) -> CombatContractEvidenceIndexesV2 {
    let paired_len = report.witness_frontier.len().min(witness_frontier.len());
    let outcomes = report
        .witness_frontier
        .iter()
        .take(paired_len)
        .enumerate()
        .collect::<Vec<_>>();
    let contract = outcomes
        .iter()
        .copied()
        .filter(|(_, outcome)| outcome_satisfies_contract(request, outcome))
        .max_by(|(_, left), (_, right)| contract_alignment_order(request, left, right))
        .map(|(index, _)| index);
    let selected = contract.or_else(|| {
        outcomes
            .iter()
            .copied()
            .max_by(|(_, left), (_, right)| contract_alignment_order(request, left, right))
            .map(|(index, _)| index)
    });
    let local_hp = outcomes
        .iter()
        .copied()
        .find(|(_, outcome)| outcome.selected_by_local_hp_view)
        .map(|(index, _)| index)
        .or((paired_len > 0).then_some(0));
    CombatContractEvidenceIndexesV2 {
        contract,
        selected,
        local_hp,
    }
}

pub(super) fn outcome_satisfies_contract(
    request: &CombatContractRequestV2,
    outcome: &LocalTurnGraphTerminalOutcomeSnapshotV1,
) -> bool {
    request
        .min_final_hp
        .is_none_or(|minimum| outcome.final_hp >= minimum)
        && outcome_satisfies_resource_contract(request, outcome)
}

pub(super) fn outcome_satisfies_resource_contract(
    request: &CombatContractRequestV2,
    outcome: &LocalTurnGraphTerminalOutcomeSnapshotV1,
) -> bool {
    outcome.potion_expenditures <= request.max_potions_used
        && (!request.require_recovered_stolen_gold || outcome.unrecovered_stolen_gold == 0)
}

fn contract_alignment_order(
    request: &CombatContractRequestV2,
    left: &LocalTurnGraphTerminalOutcomeSnapshotV1,
    right: &LocalTurnGraphTerminalOutcomeSnapshotV1,
) -> std::cmp::Ordering {
    let left_recovered =
        !request.require_recovered_stolen_gold || left.unrecovered_stolen_gold == 0;
    let right_recovered =
        !request.require_recovered_stolen_gold || right.unrecovered_stolen_gold == 0;
    let left_meets_hp = request
        .min_final_hp
        .is_none_or(|minimum| left.final_hp >= minimum);
    let right_meets_hp = request
        .min_final_hp
        .is_none_or(|minimum| right.final_hp >= minimum);

    (left.potion_expenditures <= request.max_potions_used)
        .cmp(&(right.potion_expenditures <= request.max_potions_used))
        .then_with(|| left_recovered.cmp(&right_recovered))
        .then_with(|| left_meets_hp.cmp(&right_meets_hp))
        .then_with(|| left.final_hp.cmp(&right.final_hp))
        .then_with(|| right.potion_expenditures.cmp(&left.potion_expenditures))
        .then_with(|| {
            right
                .unrecovered_stolen_gold
                .cmp(&left.unrecovered_stolen_gold)
        })
        .then_with(|| right.action_count.cmp(&left.action_count))
        .then_with(|| {
            right
                .negative_log_policy
                .total_cmp(&left.negative_log_policy)
        })
}

fn candidate_summary(
    report: &LocalTurnGraphWitnessReport,
    witness_frontier: &[OracleCombatWitness],
    index: usize,
) -> Option<CombatContractCandidateSummaryV2> {
    let outcome = report.witness_frontier.get(index)?;
    let witness = witness_frontier.get(index)?;
    Some(CombatContractCandidateSummaryV2 {
        final_hp: outcome.final_hp,
        potions_used: outcome.potion_expenditures,
        unrecovered_stolen_gold: outcome.unrecovered_stolen_gold,
        action_count: outcome.action_count,
        discovery_source: witness.discovery_source,
    })
}

fn search_status(status: &LocalTurnGraphWitnessStatus) -> CombatContractSearchStatusV2 {
    match status {
        LocalTurnGraphWitnessStatus::WitnessFound => CombatContractSearchStatusV2::WitnessFound,
        LocalTurnGraphWitnessStatus::Partial(interruption) => match interruption {
            LocalTurnGraphWitnessInterruption::SelectionBudget => {
                CombatContractSearchStatusV2::SelectionBudget
            }
            LocalTurnGraphWitnessInterruption::GenerationWorkBudget => {
                CombatContractSearchStatusV2::GenerationWorkBudget
            }
            LocalTurnGraphWitnessInterruption::EngineStepBudget => {
                CombatContractSearchStatusV2::EngineStepBudget
            }
            LocalTurnGraphWitnessInterruption::Deadline => CombatContractSearchStatusV2::Deadline,
        },
        LocalTurnGraphWitnessStatus::FrontierExhausted => {
            CombatContractSearchStatusV2::FrontierExhausted
        }
        LocalTurnGraphWitnessStatus::MechanicsGap => CombatContractSearchStatusV2::MechanicsGap,
        LocalTurnGraphWitnessStatus::ReplayMismatch(_) => {
            CombatContractSearchStatusV2::ReplayMismatch
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_planner::LocalTurnGraphWitnessCounters;
    use sts_oracle_runtime::sim::combat::CombatPosition;
    use sts_oracle_runtime::state::core::{EngineState, RunResult};
    use sts_oracle_runtime::test_support::blank_test_combat;

    fn request() -> CombatContractRequestV2 {
        CombatContractRequestV2 {
            case_id: "root".to_owned(),
            case: PathBuf::from("case.json"),
            min_final_hp: Some(66),
            max_potions_used: 0,
            require_recovered_stolen_gold: true,
            generation_work: 4_096,
            wall_ms: 2_000,
            diagnostic_prefix: None,
        }
    }

    fn report(counters: LocalTurnGraphWitnessCounters) -> LocalTurnGraphWitnessReport {
        LocalTurnGraphWitnessReport {
            status: LocalTurnGraphWitnessStatus::Partial(
                LocalTurnGraphWitnessInterruption::GenerationWorkBudget,
            ),
            counters,
            performance_timing: Default::default(),
            root_visits: 0,
            root_generated_options: 0,
            root_children: 0,
            generation_gaps: Vec::new(),
            witness: None,
            witness_frontier: Vec::new(),
        }
    }

    fn terminal_outcome(
        final_hp: i32,
        unrecovered_stolen_gold: i32,
        selected: bool,
    ) -> LocalTurnGraphTerminalOutcomeSnapshotV1 {
        LocalTurnGraphTerminalOutcomeSnapshotV1::from_materialized_outcome_v1(
            sts_oracle_learning::eval::combat_witness_materialization::CombatWitnessMaterializedOutcomeV1 {
                final_hp,
                final_gold: 0,
                persistent: sts_oracle_learning::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1 {
                    max_hp: 80,
                    recoverable_gold_delta: 0,
                    unrecovered_stolen_gold,
                    ritual_dagger_value: 0,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                starting_potion_expenditure_slots: 0,
                total_potion_expenditures: 0,
                generated_potions: Vec::new(),
                final_relics: Vec::new(),
                meta_changes: Vec::new(),
                pending_rewards: Vec::new(),
                combat_mugged: false,
            },
            1,
            1.0,
            selected,
        )
    }

    fn witness(final_hp: i32) -> OracleCombatWitness {
        let mut combat = blank_test_combat();
        combat.entities.player.current_hp = final_hp;
        OracleCombatWitness {
            actions: Vec::new(),
            final_position: CombatPosition::new(EngineState::GameOver(RunResult::Victory), combat),
            negative_log_policy: 1.0,
            replay_engine_steps: 1,
            discovery_source: OracleCombatWitnessDiscoverySource::PlannerSearch,
        }
    }

    #[test]
    fn bounded_missing_witness_remains_unknown_without_rollout_diagnostics() {
        let result = classify_contract(
            &request(),
            &report(LocalTurnGraphWitnessCounters::default()),
            &[],
            Path::new("manifest.json"),
            Duration::ZERO,
        )
        .result;
        assert_eq!(
            result.classification,
            CombatContractClassificationV2::BudgetUnknown
        );
        assert!(!result.contract_passed);
    }

    #[test]
    fn compact_result_stays_below_one_kibibyte() {
        let result = classify_contract(
            &request(),
            &report(LocalTurnGraphWitnessCounters::default()),
            &[],
            Path::new("manifest.json"),
            Duration::ZERO,
        )
        .result;
        assert!(serde_json::to_vec(&result).unwrap().len() < 1_024);
    }

    #[test]
    fn contract_selects_a_satisfying_frontier_witness_not_the_hp_first_candidate() {
        let hp_first = witness(71);
        let contract = witness(66);
        let mut report = report(LocalTurnGraphWitnessCounters::default());
        report.status = LocalTurnGraphWitnessStatus::WitnessFound;
        report.witness = Some(hp_first.clone());
        report.witness_frontier = vec![
            terminal_outcome(71, 30, true),
            terminal_outcome(66, 0, false),
        ];

        let assessment = classify_contract(
            &request(),
            &report,
            &[hp_first, contract],
            Path::new("manifest.json"),
            Duration::ZERO,
        );

        assert_eq!(assessment.selected_witness_index, Some(1));
        assert_eq!(assessment.local_hp_witness_index, Some(0));
        let result = assessment.result;
        assert_eq!(
            result.classification,
            CombatContractClassificationV2::Passed
        );
        assert!(result.contract_witness);
        assert_eq!(result.final_hp, Some(66));
        assert_eq!(result.unrecovered_stolen_gold, Some(0));
    }

    #[test]
    fn failed_contract_reports_the_resource_aligned_candidate_not_local_hp_escape() {
        let escaped = witness(71);
        let recovered = witness(61);
        let mut report = report(LocalTurnGraphWitnessCounters::default());
        report.witness = Some(escaped.clone());
        report.witness_frontier = vec![
            terminal_outcome(71, 30, true),
            terminal_outcome(61, 0, false),
        ];

        let assessment = classify_contract(
            &request(),
            &report,
            &[escaped, recovered],
            Path::new("manifest.json"),
            Duration::ZERO,
        );

        assert_eq!(assessment.selected_witness_index, Some(1));
        assert_eq!(assessment.local_hp_witness_index, Some(0));
        let result = assessment.result;
        assert_eq!(
            result.classification,
            CombatContractClassificationV2::WitnessBelowFinalHp
        );
        assert_eq!(result.final_hp, Some(61));
        assert_eq!(result.unrecovered_stolen_gold, Some(0));
        assert_eq!(result.terminal_outcome_count, 2);
        assert_eq!(result.resource_contract_candidate_count, 1);
        assert_eq!(
            result
                .local_hp_candidate
                .as_ref()
                .map(|candidate| candidate.final_hp),
            Some(71)
        );
    }
}
