use super::super::CombatSearchV2PriorityAblation;
use super::super::PendingChoiceActionWork;
use super::node::{RootLineage, RootLineageId, SearchNode};
use super::priority::{priority_for_node_with_ablation, QueueEntry};
use crate::ai::combat_state_key::combat_exact_state_key;
use std::collections::{BTreeMap, BinaryHeap};

pub(in crate::ai::combat_search_v2) struct FrontierQueue {
    unattributed: BinaryHeap<QueueEntry>,
    by_root_action: BTreeMap<RootLineageId, BinaryHeap<QueueEntry>>,
    next_sequence_id: u64,
    priority_ablation: CombatSearchV2PriorityAblation,
}

pub(in crate::ai::combat_search_v2) struct FrontierLineageSummary<'a> {
    pub(in crate::ai::combat_search_v2) lineage: RootLineage,
    pub(in crate::ai::combat_search_v2) work_items: usize,
    pub(in crate::ai::combat_search_v2) pending_choice_work_items: usize,
    pub(in crate::ai::combat_search_v2) best_entry: Option<&'a QueueEntry>,
}

impl FrontierQueue {
    #[cfg(test)]
    pub(in crate::ai::combat_search_v2) fn new() -> Self {
        Self::new_with_priority_ablation(CombatSearchV2PriorityAblation::Baseline)
    }

    pub(in crate::ai::combat_search_v2) fn new_with_priority_ablation(
        priority_ablation: CombatSearchV2PriorityAblation,
    ) -> Self {
        Self {
            unattributed: BinaryHeap::new(),
            by_root_action: BTreeMap::new(),
            next_sequence_id: 0,
            priority_ablation,
        }
    }

    pub(in crate::ai::combat_search_v2) fn push_node(&mut self, node: SearchNode) {
        self.push_work_item(node, None);
    }

    pub(in crate::ai::combat_search_v2) fn push_pending_choice_work(
        &mut self,
        node: SearchNode,
        work: PendingChoiceActionWork,
    ) {
        self.push_work_item(node, Some(work));
    }

    fn push_work_item(
        &mut self,
        node: SearchNode,
        pending_choice_work: Option<PendingChoiceActionWork>,
    ) {
        let entry = QueueEntry {
            priority: priority_for_node_with_ablation(&node, self.priority_ablation),
            sequence_id: self.next_sequence_id,
            node,
            pending_choice_work,
        };
        self.next_sequence_id = self.next_sequence_id.saturating_add(1);
        self.push_entry(entry);
    }

    fn push_entry(&mut self, entry: QueueEntry) {
        match entry.node.root_lineage {
            RootLineage::Unmaterialized => self.unattributed.push(entry),
            RootLineage::Action(id) => self.by_root_action.entry(id).or_default().push(entry),
        }
    }

    pub(in crate::ai::combat_search_v2) fn pop(&mut self) -> Option<QueueEntry> {
        let mut selected_root = None;
        let mut best = self.unattributed.peek();
        for (id, heap) in &self.by_root_action {
            let Some(candidate) = heap.peek() else {
                continue;
            };
            if best.is_none_or(|current| candidate > current) {
                selected_root = Some(*id);
                best = Some(candidate);
            }
        }
        match selected_root {
            Some(id) => self.pop_root_action(id),
            None => self.unattributed.pop(),
        }
    }

    pub(in crate::ai::combat_search_v2) fn pop_unattributed(&mut self) -> Option<QueueEntry> {
        self.unattributed.pop()
    }

    pub(in crate::ai::combat_search_v2) fn pop_root_action(
        &mut self,
        id: RootLineageId,
    ) -> Option<QueueEntry> {
        let (entry, empty) = {
            let heap = self.by_root_action.get_mut(&id)?;
            let entry = heap.pop();
            (entry, heap.is_empty())
        };
        if empty {
            self.by_root_action.remove(&id);
        }
        entry
    }

    pub(in crate::ai::combat_search_v2) fn has_root_action_work(&self, id: RootLineageId) -> bool {
        self.by_root_action
            .get(&id)
            .is_some_and(|heap| !heap.is_empty())
    }

    pub(in crate::ai::combat_search_v2) fn len(&self) -> usize {
        self.unattributed.len()
            + self
                .by_root_action
                .values()
                .map(BinaryHeap::len)
                .sum::<usize>()
    }

    pub(in crate::ai::combat_search_v2) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(in crate::ai::combat_search_v2) fn iter(&self) -> impl Iterator<Item = &QueueEntry> {
        self.unattributed
            .iter()
            .chain(self.by_root_action.values().flat_map(|heap| heap.iter()))
    }

    pub(in crate::ai::combat_search_v2) fn lineage_summaries(
        &self,
    ) -> Vec<FrontierLineageSummary<'_>> {
        let mut summaries = Vec::with_capacity(self.by_root_action.len().saturating_add(1));
        if !self.unattributed.is_empty() {
            summaries.push(FrontierLineageSummary {
                lineage: RootLineage::Unmaterialized,
                work_items: self.unattributed.len(),
                pending_choice_work_items: self
                    .unattributed
                    .iter()
                    .filter(|entry| entry.pending_choice_work.is_some())
                    .count(),
                best_entry: self.unattributed.peek(),
            });
        }
        summaries.extend(self.by_root_action.iter().map(|(id, heap)| {
            FrontierLineageSummary {
                lineage: RootLineage::Action(*id),
                work_items: heap.len(),
                pending_choice_work_items: heap
                    .iter()
                    .filter(|entry| entry.pending_choice_work.is_some())
                    .count(),
                best_entry: heap.peek(),
            }
        }));
        summaries
    }

    /// Search nodes own deep engine snapshots and action paths. Dropping a
    /// large finished frontier serially can take seconds, so dispose of the
    /// independent root heaps in a small bounded worker set.
    pub(in crate::ai::combat_search_v2) fn drop_parallel(self) {
        const MAX_DROP_WORKERS: usize = 4;

        let Self {
            unattributed,
            by_root_action,
            next_sequence_id: _,
            priority_ablation: _,
        } = self;
        let mut heaps = Vec::with_capacity(by_root_action.len().saturating_add(1));
        if !unattributed.is_empty() {
            heaps.push(unattributed);
        }
        heaps.extend(by_root_action.into_values().filter(|heap| !heap.is_empty()));
        let workers = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .min(MAX_DROP_WORKERS)
            .min(heaps.len());
        if workers <= 1 {
            drop(heaps);
            return;
        }

        heaps.sort_unstable_by_key(|heap| std::cmp::Reverse(heap.len()));
        let mut groups = (0..workers)
            .map(|_| Vec::<BinaryHeap<QueueEntry>>::new())
            .collect::<Vec<_>>();
        let mut group_loads = vec![0usize; workers];
        for heap in heaps {
            let group_index = group_loads
                .iter()
                .enumerate()
                .min_by_key(|(_, load)| **load)
                .map(|(index, _)| index)
                .expect("at least one frontier drop worker");
            group_loads[group_index] = group_loads[group_index].saturating_add(heap.len());
            groups[group_index].push(heap);
        }
        std::thread::scope(|scope| {
            for (index, group) in groups.into_iter().enumerate() {
                // If the OS refuses another worker, dropping the failed
                // closure releases that group synchronously on this thread.
                // The search report must never panic merely because cleanup
                // could not acquire an optional worker.
                let _ = std::thread::Builder::new()
                    .name(format!("combat-frontier-drop-{index}"))
                    .spawn_scoped(scope, move || drop(group));
            }
        });
    }

    pub(in crate::ai::combat_search_v2) fn replace_exact_state_rollout_estimate(
        &mut self,
        target: &SearchNode,
        estimate: &super::super::RolloutNodeEstimate,
    ) {
        let target_key = combat_exact_state_key(&target.engine, &target.combat);
        let mut entries = std::mem::take(&mut self.unattributed).into_vec();
        for (_, heap) in std::mem::take(&mut self.by_root_action) {
            entries.extend(heap.into_vec());
        }
        for entry in &mut entries {
            if combat_exact_state_key(&entry.node.engine, &entry.node.combat) == target_key {
                entry.node.rollout_estimate = estimate.clone();
                entry.priority =
                    priority_for_node_with_ablation(&entry.node, self.priority_ablation);
            }
        }
        for entry in entries {
            self.push_entry(entry);
        }
    }
}
