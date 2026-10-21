use sts_combat_planner::{
    materialize_exact_action_line, replay_oracle_combat_witness, ExactAtomicWitness,
    OracleCombatWitness,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::sim::combat::{CombatPosition, EngineCombatStepper};
use sts_oracle_runtime::state::core::EngineState;

use super::artifact_trace;
use super::CombatContractRequestV2;

pub(super) fn materialize(
    request: &CombatContractRequestV2,
    root: &CombatPosition,
) -> Result<(Option<ExactAtomicWitness>, f64), String> {
    let Some(prefix) = request.diagnostic_prefix.as_ref() else {
        return Ok((None, 0.0));
    };
    if prefix.inputs.is_empty() {
        return Err("diagnostic prefix must contain at least one input".to_owned());
    }
    if prefix.inputs.len() > 512 {
        return Err(format!(
            "diagnostic prefix has {} inputs; the V2 limit is 512",
            prefix.inputs.len()
        ));
    }
    let line = materialize_exact_action_line(&EngineCombatStepper, root, &prefix.inputs, 250)
        .map_err(|error| format!("diagnostic prefix is no longer replay-exact: {error:?}"))?;
    let actual_hash =
        combat_exact_state_hash_v2(&line.final_position.engine, &line.final_position.combat);
    if actual_hash != prefix.expected_search_root_exact_state_hash {
        return Err(format!(
            "diagnostic prefix exact successor drifted: request expects {}, replay produced {actual_hash}",
            prefix.expected_search_root_exact_state_hash
        ));
    }
    if !matches!(line.final_position.engine, EngineState::CombatPlayerTurn) {
        return Err(format!(
            "diagnostic prefix did not reach a player-turn search root: {:?}",
            line.final_position.engine
        ));
    }
    let policy_trace = artifact_trace::replay_actions(root, &prefix.inputs)?;
    if policy_trace.final_exact_state_hash != actual_hash {
        return Err(format!(
            "diagnostic prefix policy replay drifted: exact replay={actual_hash}, policy replay={}",
            policy_trace.final_exact_state_hash
        ));
    }
    let negative_log_policy = policy_trace
        .policy_trace
        .iter()
        .filter_map(|step| step.negative_log_probability)
        .sum();
    Ok((Some(line), negative_log_policy))
}

pub(super) fn compose_witnesses(
    root: &CombatPosition,
    prefix: Option<&ExactAtomicWitness>,
    prefix_negative_log_policy: f64,
    suffixes: &[OracleCombatWitness],
) -> Result<Vec<OracleCombatWitness>, String> {
    suffixes
        .iter()
        .map(|suffix| compose_witness(root, prefix, prefix_negative_log_policy, suffix))
        .collect()
}

pub(super) fn compose_witness(
    root: &CombatPosition,
    prefix: Option<&ExactAtomicWitness>,
    prefix_negative_log_policy: f64,
    suffix: &OracleCombatWitness,
) -> Result<OracleCombatWitness, String> {
    let Some(prefix) = prefix else {
        return Ok(suffix.clone());
    };
    let mut actions = Vec::with_capacity(prefix.actions.len() + suffix.actions.len());
    actions.extend_from_slice(&prefix.actions);
    actions.extend_from_slice(&suffix.actions);
    replay_oracle_combat_witness(
        root,
        &actions,
        prefix_negative_log_policy + suffix.negative_log_policy,
        suffix.discovery_source,
        &EngineCombatStepper,
    )
    .map_err(|error| {
        format!("diagnostic prefix and search suffix failed full-root replay: {error:?}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_planner::{materialize_exact_action_line, OracleCombatWitnessDiscoverySource};
    use sts_oracle_runtime::content::cards::CardId;
    use sts_oracle_runtime::content::monsters::EnemyId;
    use sts_oracle_runtime::runtime::combat::CombatCard;
    use sts_oracle_runtime::sim::combat::{combat_terminal, CombatTerminal};
    use sts_oracle_runtime::state::core::ClientInput;
    use sts_oracle_runtime::test_support::{blank_test_combat, test_monster};

    #[test]
    fn diagnostic_prefix_and_search_suffix_compose_from_the_original_root() {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.id = 10;
        monster.current_hp = 6;
        monster.max_hp = 6;
        combat.entities.monsters = vec![monster];
        combat.turn.energy = 3;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let prefix = materialize_exact_action_line(
            &EngineCombatStepper,
            &root,
            &[ClientInput::PlayCard {
                card_index: 0,
                target: None,
            }],
            250,
        )
        .expect("defend prefix should materialize");
        let suffix = materialize_exact_action_line(
            &EngineCombatStepper,
            &prefix.final_position,
            &[ClientInput::PlayCard {
                card_index: 0,
                target: Some(10),
            }],
            250,
        )
        .expect("lethal suffix should materialize");
        let suffix = OracleCombatWitness {
            actions: suffix.actions,
            final_position: suffix.final_position,
            negative_log_policy: 3.0,
            replay_engine_steps: suffix.replay_engine_steps,
            discovery_source: OracleCombatWitnessDiscoverySource::PlannerSearch,
        };

        let composed =
            compose_witness(&root, Some(&prefix), 2.0, &suffix).expect("full-root replay");

        assert_eq!(composed.actions.len(), 2);
        assert_eq!(composed.negative_log_policy, 5.0);
        assert_eq!(
            combat_terminal(
                &composed.final_position.engine,
                &composed.final_position.combat
            ),
            CombatTerminal::Win
        );
    }
}
