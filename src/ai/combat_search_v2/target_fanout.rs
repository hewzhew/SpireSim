use super::*;
use std::collections::BTreeMap;

mod candidates;
mod diagnostics;
use candidates::{target_fanout_candidate, TargetFanoutCandidate};
pub(super) use diagnostics::TargetFanoutDiagnosticsCollector;

#[derive(Clone, Debug)]
pub(super) struct TargetFanoutSummary {
    targeted_actions: usize,
    groups: Vec<TargetFanoutGroupSummary>,
}

#[derive(Clone, Debug)]
struct TargetFanoutGroupSummary {
    kind: TargetFanoutKind,
    source_key: String,
    target_count: usize,
    lethal_targets: usize,
    min_target_hp_with_block: i32,
    max_target_hp_with_block: i32,
    min_damage_hint: i32,
    max_damage_hint: i32,
    first_action_key: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TargetFanoutKind {
    PlayCard,
    UsePotion,
}

pub(super) fn summarize_target_fanout(
    combat: &CombatState,
    choices: &[CombatActionChoice],
) -> TargetFanoutSummary {
    let mut grouped: BTreeMap<(TargetFanoutKind, String), Vec<TargetFanoutCandidate>> =
        BTreeMap::new();

    for choice in choices {
        if let Some(candidate) = target_fanout_candidate(combat, choice) {
            grouped
                .entry((candidate.kind, candidate.source_key.clone()))
                .or_default()
                .push(candidate);
        }
    }

    let targeted_actions = grouped.values().map(Vec::len).sum();
    let groups = grouped
        .into_values()
        .filter_map(summarize_group)
        .collect::<Vec<_>>();

    TargetFanoutSummary {
        targeted_actions,
        groups,
    }
}

fn summarize_group(candidates: Vec<TargetFanoutCandidate>) -> Option<TargetFanoutGroupSummary> {
    let first = candidates.first()?;
    let mut min_target_hp_with_block = i32::MAX;
    let mut max_target_hp_with_block = i32::MIN;
    let mut min_damage_hint = i32::MAX;
    let mut max_damage_hint = i32::MIN;
    let mut lethal_targets = 0usize;

    for candidate in &candidates {
        min_target_hp_with_block = min_target_hp_with_block.min(candidate.target_hp_with_block);
        max_target_hp_with_block = max_target_hp_with_block.max(candidate.target_hp_with_block);
        min_damage_hint = min_damage_hint.min(candidate.damage_hint);
        max_damage_hint = max_damage_hint.max(candidate.damage_hint);
        if candidate.lethal {
            lethal_targets = lethal_targets.saturating_add(1);
        }
    }

    Some(TargetFanoutGroupSummary {
        kind: first.kind,
        source_key: first.source_key.clone(),
        target_count: candidates.len(),
        lethal_targets,
        min_target_hp_with_block,
        max_target_hp_with_block,
        min_damage_hint,
        max_damage_hint,
        first_action_key: first.action_key.clone(),
    })
}

impl TargetFanoutGroupSummary {
    fn target_hp_span(&self) -> i32 {
        self.max_target_hp_with_block - self.min_target_hp_with_block
    }
}

#[cfg(test)]
mod tests;
