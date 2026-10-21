use super::action_priority::priority_for_input_with_plugins;
use super::*;

mod compare;
mod diagnostics;
mod summary;
mod types;

pub(super) use diagnostics::ActionOrderingDiagnosticsCollector;
#[cfg(test)]
use diagnostics::ACTION_EFFECT_SAMPLE_LIMIT;
pub(super) use types::{ActionOrderingSummary, IndexedActionChoice};

use compare::compare_action_ordering_entries;
use summary::summarize_ordering;
use types::{ActionOrderingEntry, ActionOrderingResult};

#[cfg(test)]
fn order_action_choices(
    engine: &EngineState,
    combat: &CombatState,
    choices: Vec<CombatActionChoice>,
) -> ActionOrderingResult {
    order_indexed_action_choices(
        engine,
        combat,
        choices
            .into_iter()
            .enumerate()
            .map(|(original_action_id, choice)| IndexedActionChoice {
                original_action_id,
                choice,
            })
            .collect(),
    )
}

pub(super) fn order_indexed_action_choices(
    engine: &EngineState,
    combat: &CombatState,
    choices: Vec<IndexedActionChoice>,
) -> ActionOrderingResult {
    order_indexed_action_choices_with_plugins(
        engine,
        combat,
        choices,
        CombatSearchActionOrderingPlugins::default(),
    )
}

#[cfg(test)]
pub(super) fn order_indexed_action_choices_with_prior(
    engine: &EngineState,
    combat: &CombatState,
    choices: Vec<IndexedActionChoice>,
    root_action_prior: Option<&CombatSearchV2RootActionPrior>,
    phase_guard: CombatSearchPhaseGuardPluginId,
    action_prior: CombatSearchActionPriorPluginId,
) -> ActionOrderingResult {
    order_indexed_action_choices_with_plugins(
        engine,
        combat,
        choices,
        CombatSearchActionOrderingPlugins {
            root_action_prior,
            phase_guard,
            action_prior,
        },
    )
}

pub(super) fn order_indexed_action_choices_with_plugins(
    engine: &EngineState,
    combat: &CombatState,
    choices: Vec<IndexedActionChoice>,
    plugins: CombatSearchActionOrderingPlugins<'_>,
) -> ActionOrderingResult {
    let exact_state_hash = plugins
        .root_action_prior
        .filter(|prior| !prior.is_empty())
        .map(|_| combat_exact_state_hash_v2(engine, combat));
    let mut entries = choices
        .into_iter()
        .map(|indexed| ActionOrderingEntry {
            original_action_id: indexed.original_action_id,
            root_action_prior_score: exact_state_hash.as_ref().and_then(|state_hash| {
                plugins
                    .root_action_prior
                    .and_then(|prior| prior.score(state_hash, &indexed.choice.action_key))
            }),
            priority: priority_for_input_with_plugins(
                engine,
                combat,
                &indexed.choice.input,
                plugins,
            ),
            choice: indexed.choice,
        })
        .collect::<Vec<_>>();

    if action_ordering_enabled(engine) {
        entries.sort_by(|left, right| {
            compare_action_ordering_entries(left, right)
                .then_with(|| left.original_action_id.cmp(&right.original_action_id))
        });
    }

    let summary = summarize_ordering(&entries);
    let choices = entries
        .into_iter()
        .map(|entry| IndexedActionChoice {
            original_action_id: entry.original_action_id,
            choice: entry.choice,
        })
        .collect();

    ActionOrderingResult { choices, summary }
}

pub(in crate::ai::combat_search_v2) fn action_ordering_enabled(engine: &EngineState) -> bool {
    sts_combat_knowledge::combat_tactical_action_ranking_enabled_v1(engine)
}

#[cfg(test)]
mod tests;
