use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;
use sts_oracle_runtime::content::cards::{get_card_definition, CardId};
use sts_oracle_runtime::sim::combat::CombatTerminal;

use super::replay::{
    action_sequence_hash, build_fiend_fire_observation, previous_card_index, record_id,
};
use super::{
    display_path, ActionObservation, ContractTrace, EvidenceRecord, MonsterObservation,
    PreviousCardBypassObservation, PreviousCardBypassStatus, StateObservation,
    EVIDENCE_SCHEMA_NAME,
};

pub(super) fn contract_record(trace_path: &Path, value: Value) -> Result<EvidenceRecord, String> {
    let trace: ContractTrace = serde_json::from_value(value).map_err(|error| {
        format!(
            "cannot decode contract trace '{}': {error}",
            trace_path.display()
        )
    })?;
    let inputs = trace
        .actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let action_hash = action_sequence_hash(&inputs)?;
    let record_id = record_id(&trace.root_exact_state_hash, &action_hash);
    let mut actions = trace
        .actions
        .into_iter()
        .map(|action| {
            let card = action.subject.and_then(|subject| subject.card);
            let card_type = card
                .as_ref()
                .map(|card| get_card_definition(card.id).card_type);
            let terminal_after = infer_terminal(&action.after);
            ActionObservation {
                index: action.index,
                input: action.input,
                card,
                card_type,
                before: action.before,
                after: action.after,
                terminal_after,
                previous_card_bypass: None,
            }
        })
        .collect::<Vec<_>>();
    for index in 0..actions.len() {
        if actions[index].card.is_none() {
            continue;
        }
        let previous_action_index = previous_card_index(&actions, index);
        actions[index].previous_card_bypass = Some(PreviousCardBypassObservation {
            previous_action_index,
            status: if previous_action_index.is_some() {
                PreviousCardBypassStatus::TraceOnlyUnavailable
            } else {
                PreviousCardBypassStatus::NoPreviousCardBoundary
            },
            terminal_after: None,
            after: None,
        });
    }
    let final_terminal = actions
        .last()
        .map(|action| infer_terminal(&action.after))
        .unwrap_or(CombatTerminal::Unresolved);
    let fiend_fire_observations = actions
        .iter()
        .enumerate()
        .filter(|(_, action)| action.card.as_ref().map(|card| card.id) == Some(CardId::FiendFire))
        .map(|(index, _)| {
            build_fiend_fire_observation(
                &record_id,
                &trace.root_exact_state_hash,
                &actions,
                index,
                actions[index].previous_card_bypass.clone().unwrap_or(
                    PreviousCardBypassObservation {
                        previous_action_index: None,
                        status: PreviousCardBypassStatus::TraceOnlyUnavailable,
                        terminal_after: None,
                        after: None,
                    },
                ),
                final_terminal,
            )
        })
        .collect::<Vec<_>>();
    Ok(EvidenceRecord {
        schema_name: EVIDENCE_SCHEMA_NAME.to_string(),
        schema_version: 3,
        record_id,
        root_exact_state_hash: trace.root_exact_state_hash,
        case_identity: None,
        action_sequence_blake2b_512: action_hash,
        provenance: BTreeSet::from(["typed_contract_trace".to_string()]),
        source_paths: BTreeSet::from([display_path(trace_path)]),
        case_path: None,
        action_paths: Vec::new(),
        replay_exact: false,
        supplied_action_count: actions.len(),
        consumed_action_count: actions.len(),
        final_terminal,
        final_player_hp: actions
            .last()
            .map(|action| action.after.player.hp)
            .unwrap_or_default(),
        actions,
        fiend_fire_observations,
    })
}

fn infer_terminal(state: &StateObservation) -> CombatTerminal {
    if state.player.hp <= 0 {
        CombatTerminal::Loss
    } else if !state.monsters.is_empty()
        && state.monsters.iter().all(MonsterObservation::terminal_like)
    {
        CombatTerminal::Win
    } else {
        CombatTerminal::Unresolved
    }
}
