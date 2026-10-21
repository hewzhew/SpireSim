use std::collections::BTreeMap;

use super::super::*;
use super::keys::{equivalence_key_for_choice, ActionEquivalenceKey};
use super::types::{
    ActionEquivalenceGroupSummary, ActionEquivalenceResult, ActionEquivalenceSummary,
    PendingEquivalenceGroup,
};

pub(in crate::ai::combat_search_v2) fn compress_equivalent_actions(
    engine: &EngineState,
    combat: &CombatState,
    choices: Vec<CombatActionChoice>,
) -> ActionEquivalenceResult {
    let atomic_actions_in = choices.len();
    let mut representatives = Vec::with_capacity(choices.len());
    let mut seen: BTreeMap<ActionEquivalenceKey, usize> = BTreeMap::new();
    let mut groups: Vec<(ActionEquivalenceKey, PendingEquivalenceGroup)> = Vec::new();

    for (original_action_id, choice) in choices.into_iter().enumerate() {
        let Some(key) = equivalence_key_for_choice(engine, combat, &choice) else {
            representatives.push(IndexedActionChoice {
                original_action_id,
                choice,
            });
            continue;
        };

        if let Some(group_index) = seen.get(&key).copied() {
            groups[group_index]
                .1
                .removed_original_action_ids
                .push(original_action_id);
        } else {
            representatives.push(IndexedActionChoice {
                original_action_id,
                choice,
            });
            seen.insert(key.clone(), groups.len());
            groups.push((
                key,
                PendingEquivalenceGroup {
                    representative_original_action_id: original_action_id,
                    removed_original_action_ids: Vec::new(),
                },
            ));
        }
    }

    let groups = groups
        .into_iter()
        .filter_map(|(key, group)| {
            if group.removed_original_action_ids.is_empty() {
                None
            } else {
                Some(ActionEquivalenceGroupSummary {
                    key,
                    representative_original_action_id: group.representative_original_action_id,
                    removed_original_action_ids: group.removed_original_action_ids,
                })
            }
        })
        .collect::<Vec<_>>();

    ActionEquivalenceResult {
        summary: ActionEquivalenceSummary {
            atomic_actions_in,
            representative_actions_out: representatives.len(),
            groups,
        },
        choices: representatives,
    }
}
