use super::*;
use serde::Serialize;

/// Read-only census of retained container ownership after one search slice.
///
/// Counts and capacities are observational only. Search scheduling, stopping,
/// and exact-state identity never read this structure.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphStorageSnapshot {
    pub exact_nodes: usize,
    pub finished_generators: usize,
    pub exhausted_nodes: usize,
    pub live_generator_work_items: usize,
    pub generator_work_slots: usize,
    pub generator_stale_work_slots: usize,
    pub generators_with_stale_work_majority: usize,
    pub generator_work_capacity: usize,
    pub generator_work_sequence_capacity: usize,
    pub generator_guide_entry_count_capacity: usize,
    pub generator_free_work_slots: usize,
    pub generator_free_work_capacity: usize,
    pub generator_seen_states: usize,
    pub generator_seen_capacity: usize,
    pub generator_anchor_entries: usize,
    pub generator_live_anchor_entries: usize,
    pub generator_stale_anchor_entries: usize,
    pub generators_with_stale_anchor_majority: usize,
    pub stale_anchor_entries_in_rebuild_candidates: usize,
    pub generator_anchor_capacity: usize,
    pub generator_guide_frontiers: usize,
    pub generator_empty_guide_frontiers: usize,
    pub generator_single_entry_guide_frontiers: usize,
    pub generator_two_to_four_entry_guide_frontiers: usize,
    pub generator_five_to_sixteen_entry_guide_frontiers: usize,
    pub generator_larger_guide_frontiers: usize,
    pub maximum_guide_frontier_entries: usize,
    pub generator_guide_entries: usize,
    pub generator_live_guide_entries: usize,
    pub generator_stale_guide_entries: usize,
    pub generators_with_stale_guide_majority: usize,
    pub live_guide_entries_in_rebuild_candidates: usize,
    pub stale_guide_entries_in_rebuild_candidates: usize,
    pub generator_guide_capacity: usize,
    pub generator_scheduled_round_entries: usize,
    pub generator_live_scheduled_round_entries: usize,
    pub generator_stale_scheduled_round_entries: usize,
    pub generator_scheduled_round_capacity: usize,
    pub generator_completed_options: usize,
    pub generator_completed_capacity: usize,
    pub generator_gaps: usize,
    pub generator_gaps_capacity: usize,
    pub finished_generator_work_capacity: usize,
    pub finished_generator_seen_capacity: usize,
    pub finished_generator_anchor_capacity: usize,
    pub finished_generator_guide_capacity: usize,
    pub finished_generator_scheduled_round_capacity: usize,
    pub finished_generator_completed_capacity: usize,
    pub finished_generator_gaps_capacity: usize,
    pub generator_scheduling_rebuilds: usize,
    pub generator_reused_work_slots: usize,
    pub generator_reclaimed_anchor_entries: usize,
    pub generator_reclaimed_guide_entries: usize,
    pub active_generators: usize,
    pub generators_with_one_live_work: usize,
    pub generators_with_two_to_four_live_work: usize,
    pub generators_with_five_to_sixteen_live_work: usize,
    pub generators_with_seventeen_to_sixty_four_live_work: usize,
    pub generators_with_more_than_sixty_four_live_work: usize,
    pub maximum_live_work_items_in_one_generator: usize,
    pub maximum_stale_guide_entries_in_one_generator: usize,
    pub graph_edges: usize,
    pub graph_edge_capacity: usize,
}

impl LocalTurnGraphWitnessSession {
    pub fn storage_snapshot(&self) -> LocalTurnGraphStorageSnapshot {
        let mut snapshot = LocalTurnGraphStorageSnapshot {
            exact_nodes: self.nodes.len(),
            exhausted_nodes: self.nodes.iter().filter(|node| node.exhausted).count(),
            ..LocalTurnGraphStorageSnapshot::default()
        };
        for node in &self.nodes {
            let generator = node.generator.storage_snapshot();
            snapshot.finished_generators = snapshot
                .finished_generators
                .saturating_add(usize::from(generator.finished));
            snapshot.live_generator_work_items = snapshot
                .live_generator_work_items
                .saturating_add(generator.live_work_items);
            snapshot.generator_work_slots = snapshot
                .generator_work_slots
                .saturating_add(generator.work_slots);
            let stale_work_slots = generator
                .work_slots
                .saturating_sub(generator.live_work_items);
            snapshot.generator_stale_work_slots = snapshot
                .generator_stale_work_slots
                .saturating_add(stale_work_slots);
            if stale_work_slots > generator.live_work_items {
                snapshot.generators_with_stale_work_majority = snapshot
                    .generators_with_stale_work_majority
                    .saturating_add(1);
            }
            snapshot.generator_work_capacity = snapshot
                .generator_work_capacity
                .saturating_add(generator.work_capacity);
            snapshot.generator_work_sequence_capacity = snapshot
                .generator_work_sequence_capacity
                .saturating_add(generator.work_sequence_capacity);
            snapshot.generator_guide_entry_count_capacity = snapshot
                .generator_guide_entry_count_capacity
                .saturating_add(generator.guide_entry_count_capacity);
            snapshot.generator_free_work_slots = snapshot
                .generator_free_work_slots
                .saturating_add(generator.free_work_slots);
            snapshot.generator_free_work_capacity = snapshot
                .generator_free_work_capacity
                .saturating_add(generator.free_work_capacity);
            snapshot.generator_seen_states = snapshot
                .generator_seen_states
                .saturating_add(generator.seen_states);
            snapshot.generator_seen_capacity = snapshot
                .generator_seen_capacity
                .saturating_add(generator.seen_capacity);
            snapshot.generator_anchor_entries = snapshot
                .generator_anchor_entries
                .saturating_add(generator.anchor_entries);
            snapshot.generator_live_anchor_entries = snapshot
                .generator_live_anchor_entries
                .saturating_add(generator.live_anchor_entries);
            let stale_anchor_entries = generator
                .anchor_entries
                .saturating_sub(generator.live_anchor_entries);
            snapshot.generator_stale_anchor_entries = snapshot
                .generator_stale_anchor_entries
                .saturating_add(stale_anchor_entries);
            if stale_anchor_entries > generator.live_anchor_entries {
                snapshot.generators_with_stale_anchor_majority = snapshot
                    .generators_with_stale_anchor_majority
                    .saturating_add(1);
                snapshot.stale_anchor_entries_in_rebuild_candidates = snapshot
                    .stale_anchor_entries_in_rebuild_candidates
                    .saturating_add(stale_anchor_entries);
            }
            snapshot.generator_anchor_capacity = snapshot
                .generator_anchor_capacity
                .saturating_add(generator.anchor_capacity);
            snapshot.generator_guide_frontiers = snapshot
                .generator_guide_frontiers
                .saturating_add(generator.guide_frontiers);
            snapshot.generator_empty_guide_frontiers = snapshot
                .generator_empty_guide_frontiers
                .saturating_add(generator.empty_guide_frontiers);
            snapshot.generator_single_entry_guide_frontiers = snapshot
                .generator_single_entry_guide_frontiers
                .saturating_add(generator.single_entry_guide_frontiers);
            snapshot.generator_two_to_four_entry_guide_frontiers = snapshot
                .generator_two_to_four_entry_guide_frontiers
                .saturating_add(generator.two_to_four_entry_guide_frontiers);
            snapshot.generator_five_to_sixteen_entry_guide_frontiers = snapshot
                .generator_five_to_sixteen_entry_guide_frontiers
                .saturating_add(generator.five_to_sixteen_entry_guide_frontiers);
            snapshot.generator_larger_guide_frontiers = snapshot
                .generator_larger_guide_frontiers
                .saturating_add(generator.larger_guide_frontiers);
            snapshot.maximum_guide_frontier_entries = snapshot
                .maximum_guide_frontier_entries
                .max(generator.maximum_guide_frontier_entries);
            snapshot.generator_guide_entries = snapshot
                .generator_guide_entries
                .saturating_add(generator.guide_entries);
            snapshot.generator_live_guide_entries = snapshot
                .generator_live_guide_entries
                .saturating_add(generator.live_guide_entries);
            let stale_guide_entries = generator
                .guide_entries
                .saturating_sub(generator.live_guide_entries);
            snapshot.generator_stale_guide_entries = snapshot
                .generator_stale_guide_entries
                .saturating_add(stale_guide_entries);
            if stale_guide_entries > generator.live_guide_entries {
                snapshot.generators_with_stale_guide_majority = snapshot
                    .generators_with_stale_guide_majority
                    .saturating_add(1);
                snapshot.live_guide_entries_in_rebuild_candidates = snapshot
                    .live_guide_entries_in_rebuild_candidates
                    .saturating_add(generator.live_guide_entries);
                snapshot.stale_guide_entries_in_rebuild_candidates = snapshot
                    .stale_guide_entries_in_rebuild_candidates
                    .saturating_add(stale_guide_entries);
            }
            snapshot.generator_guide_capacity = snapshot
                .generator_guide_capacity
                .saturating_add(generator.guide_capacity);
            snapshot.generator_scheduled_round_entries = snapshot
                .generator_scheduled_round_entries
                .saturating_add(generator.scheduled_round_entries);
            snapshot.generator_live_scheduled_round_entries = snapshot
                .generator_live_scheduled_round_entries
                .saturating_add(generator.live_scheduled_round_entries);
            snapshot.generator_stale_scheduled_round_entries = snapshot
                .generator_stale_scheduled_round_entries
                .saturating_add(
                    generator
                        .scheduled_round_entries
                        .saturating_sub(generator.live_scheduled_round_entries),
                );
            snapshot.generator_scheduled_round_capacity = snapshot
                .generator_scheduled_round_capacity
                .saturating_add(generator.scheduled_round_capacity);
            snapshot.generator_completed_options = snapshot
                .generator_completed_options
                .saturating_add(generator.completed_options);
            snapshot.generator_completed_capacity = snapshot
                .generator_completed_capacity
                .saturating_add(generator.completed_capacity);
            snapshot.generator_gaps = snapshot.generator_gaps.saturating_add(generator.gaps);
            snapshot.generator_gaps_capacity = snapshot
                .generator_gaps_capacity
                .saturating_add(generator.gaps_capacity);
            snapshot.generator_scheduling_rebuilds = snapshot
                .generator_scheduling_rebuilds
                .saturating_add(generator.scheduling_rebuilds);
            snapshot.generator_reused_work_slots = snapshot
                .generator_reused_work_slots
                .saturating_add(generator.reused_work_slots);
            snapshot.generator_reclaimed_anchor_entries = snapshot
                .generator_reclaimed_anchor_entries
                .saturating_add(generator.reclaimed_anchor_entries);
            snapshot.generator_reclaimed_guide_entries = snapshot
                .generator_reclaimed_guide_entries
                .saturating_add(generator.reclaimed_guide_entries);
            if generator.finished {
                snapshot.finished_generator_work_capacity = snapshot
                    .finished_generator_work_capacity
                    .saturating_add(generator.work_capacity);
                snapshot.finished_generator_seen_capacity = snapshot
                    .finished_generator_seen_capacity
                    .saturating_add(generator.seen_capacity);
                snapshot.finished_generator_anchor_capacity = snapshot
                    .finished_generator_anchor_capacity
                    .saturating_add(generator.anchor_capacity);
                snapshot.finished_generator_guide_capacity = snapshot
                    .finished_generator_guide_capacity
                    .saturating_add(generator.guide_capacity);
                snapshot.finished_generator_scheduled_round_capacity = snapshot
                    .finished_generator_scheduled_round_capacity
                    .saturating_add(generator.scheduled_round_capacity);
                snapshot.finished_generator_completed_capacity = snapshot
                    .finished_generator_completed_capacity
                    .saturating_add(generator.completed_capacity);
                snapshot.finished_generator_gaps_capacity = snapshot
                    .finished_generator_gaps_capacity
                    .saturating_add(generator.gaps_capacity);
            } else {
                snapshot.active_generators = snapshot.active_generators.saturating_add(1);
                match generator.live_work_items {
                    0 => {}
                    1 => {
                        snapshot.generators_with_one_live_work =
                            snapshot.generators_with_one_live_work.saturating_add(1);
                    }
                    2..=4 => {
                        snapshot.generators_with_two_to_four_live_work = snapshot
                            .generators_with_two_to_four_live_work
                            .saturating_add(1);
                    }
                    5..=16 => {
                        snapshot.generators_with_five_to_sixteen_live_work = snapshot
                            .generators_with_five_to_sixteen_live_work
                            .saturating_add(1);
                    }
                    17..=64 => {
                        snapshot.generators_with_seventeen_to_sixty_four_live_work = snapshot
                            .generators_with_seventeen_to_sixty_four_live_work
                            .saturating_add(1);
                    }
                    _ => {
                        snapshot.generators_with_more_than_sixty_four_live_work = snapshot
                            .generators_with_more_than_sixty_four_live_work
                            .saturating_add(1);
                    }
                }
                snapshot.maximum_live_work_items_in_one_generator = snapshot
                    .maximum_live_work_items_in_one_generator
                    .max(generator.live_work_items);
                snapshot.maximum_stale_guide_entries_in_one_generator = snapshot
                    .maximum_stale_guide_entries_in_one_generator
                    .max(stale_guide_entries);
            }
            snapshot.graph_edges = snapshot.graph_edges.saturating_add(node.children.len());
            snapshot.graph_edge_capacity = snapshot
                .graph_edge_capacity
                .saturating_add(node.children.capacity());
        }
        snapshot
    }
}
