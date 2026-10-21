use std::path::Path;

use serde_json::{json, Value};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::state::core::ClientInput;

use super::super::print_json;
use super::artifact_trace::{load_root, replay_candidate, ReplayedActionTraceV2};
use super::CombatContractArtifactV2;

pub(super) fn run(artifact_path: &Path, artifact: &CombatContractArtifactV2) -> Result<(), String> {
    let contract_candidate = artifact
        .terminal_candidates
        .iter()
        .find(|candidate| candidate.selected_by_contract_view)
        .ok_or_else(|| {
            format!(
                "V2 artifact '{}' has no contract-aligned terminal candidate",
                artifact_path.display()
            )
        })?;
    let local_hp_candidate = artifact
        .terminal_candidates
        .iter()
        .find(|candidate| candidate.selected_by_local_hp_view)
        .ok_or_else(|| {
            format!(
                "V2 artifact '{}' has no local-HP terminal candidate",
                artifact_path.display()
            )
        })?;
    let root = load_root(artifact_path, artifact)?;
    let (contract_actions, contract_trace) = replay_candidate(&root, contract_candidate)?;
    let (local_hp_actions, local_hp_trace) = replay_candidate(&root, local_hp_candidate)?;
    let first_divergence_index = first_divergence_index(&contract_actions, &local_hp_actions);
    let first_divergence = first_divergence_index
        .map(|index| {
            let contract_before = position_hash_before(&root, &contract_trace, index)?;
            let local_hp_before = position_hash_before(&root, &local_hp_trace, index)?;
            if contract_before != local_hp_before {
                return Err(format!(
                    "candidate prefixes drifted before their first input divergence at action {index}: contract={contract_before}, local_hp={local_hp_before}"
                ));
            }
            Ok(json!({
                "action_index": index,
                "exact_state_hash_before": contract_before,
                "contract_aligned": compact_action(&contract_trace, index),
                "local_hp": compact_action(&local_hp_trace, index),
            }))
        })
        .transpose()?;

    print_json(&json!({
        "schema_name": "OracleCombatContractCandidateComparisonV2",
        "schema_version": 2,
        "artifact": artifact_path,
        "case_id": artifact.request.case_id,
        "same_candidate": contract_candidate.candidate_id == local_hp_candidate.candidate_id,
        "contract_aligned_candidate": contract_candidate,
        "local_hp_candidate": local_hp_candidate,
        "first_divergence": first_divergence,
        "contract_aligned_actions": compact_action_sequence(&contract_trace),
        "local_hp_actions": compact_action_sequence(&local_hp_trace),
        "contract_aligned_turn_checkpoints": contract_trace.turn_checkpoints,
        "local_hp_turn_checkpoints": local_hp_trace.turn_checkpoints,
    }))
}

fn first_divergence_index(left: &[ClientInput], right: &[ClientInput]) -> Option<usize> {
    left.iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .or_else(|| (left.len() != right.len()).then_some(left.len().min(right.len())))
}

fn position_hash_before(
    root: &sts_oracle_runtime::sim::combat::CombatPosition,
    trace: &ReplayedActionTraceV2,
    action_index: usize,
) -> Result<String, String> {
    let position = if action_index == 0 {
        root
    } else {
        trace
            .prefix_positions
            .get(action_index - 1)
            .ok_or_else(|| {
                format!(
                    "candidate trace has no position before action {action_index}; replayed {} actions",
                    trace.prefix_positions.len()
                )
            })?
    };
    Ok(combat_exact_state_hash_v2(
        &position.engine,
        &position.combat,
    ))
}

fn compact_action(trace: &ReplayedActionTraceV2, action_index: usize) -> Option<Value> {
    trace.policy_trace.get(action_index).map(|step| {
        json!({
            "turn": step.turn,
            "action": step.action,
            "action_key": step.action_key,
            "input": step.input,
            "ordinal_rank": step.ordinal_rank,
            "raw_weight": step.raw_weight,
        })
    })
}

fn compact_action_sequence(trace: &ReplayedActionTraceV2) -> Vec<Value> {
    trace
        .policy_trace
        .iter()
        .map(|step| {
            json!({
                "step": step.step,
                "turn": step.turn,
                "action": step.action,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divergence_finds_changed_input_or_shorter_complete_line() {
        let end = ClientInput::EndTurn;
        let discard = ClientInput::DiscardPotion(0);

        assert_eq!(
            first_divergence_index(
                &[end.clone(), discard.clone()],
                &[end.clone(), ClientInput::DiscardPotion(1)]
            ),
            Some(1)
        );
        assert_eq!(
            first_divergence_index(&[end.clone()], &[end.clone(), discard]),
            Some(1)
        );
        assert_eq!(first_divergence_index(&[end.clone()], &[end]), None);
    }
}
