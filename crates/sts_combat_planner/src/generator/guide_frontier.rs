use std::cmp::Ordering;
use std::collections::{binary_heap, BinaryHeap};

use crate::policy::{CombatGuideLaneId, CombatStateGuideRank};

use super::scheduling::GeneratorWorkPriority;

#[derive(Clone, Debug)]
pub(super) struct GuidedGeneratorQueueEntry {
    pub(super) guide_lane: CombatGuideLaneId,
    pub(super) work_id: usize,
    pub(super) sequence_id: u64,
    pub(super) guide_rank: CombatStateGuideRank,
    /// Finish a prepared edge before expanding an equally ranked sibling.
    /// Otherwise anchor-fed expansions can consume every guide service while
    /// their action cursors wait behind the shorter expansion work.
    pub(super) ready_transition: bool,
    pub(super) anchor_priority: GeneratorWorkPriority,
}

impl Eq for GuidedGeneratorQueueEntry {}

impl PartialEq for GuidedGeneratorQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.guide_lane == other.guide_lane
            && self.work_id == other.work_id
            && self.sequence_id == other.sequence_id
    }
}

impl Ord for GuidedGeneratorQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.guide_rank
            .cmp(&other.guide_rank)
            .then_with(|| self.ready_transition.cmp(&other.ready_transition))
            .then_with(|| self.anchor_priority.cmp(&other.anchor_priority))
            .then_with(|| other.sequence_id.cmp(&self.sequence_id))
    }
}

impl PartialOrd for GuidedGeneratorQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Allocation-free storage for the overwhelmingly common one-entry guide
/// frontier, with the standard-library heap retained for every larger case.
pub(super) enum GuidedGeneratorEntries {
    Empty,
    One(GuidedGeneratorQueueEntry),
    Heap(BinaryHeap<GuidedGeneratorQueueEntry>),
}

impl GuidedGeneratorEntries {
    pub(super) fn new() -> Self {
        Self::Empty
    }

    pub(super) fn push(&mut self, entry: GuidedGeneratorQueueEntry) {
        match self {
            Self::Empty => *self = Self::One(entry),
            Self::One(_) => {
                let Self::One(previous) = std::mem::replace(self, Self::Empty) else {
                    unreachable!("one-entry frontier changed during promotion");
                };
                let mut heap = BinaryHeap::with_capacity(2);
                heap.push(previous);
                heap.push(entry);
                *self = Self::Heap(heap);
            }
            Self::Heap(heap) => heap.push(entry),
        }
    }

    pub(super) fn peek(&self) -> Option<&GuidedGeneratorQueueEntry> {
        match self {
            Self::Empty => None,
            Self::One(entry) => Some(entry),
            Self::Heap(heap) => heap.peek(),
        }
    }

    pub(super) fn pop(&mut self) -> Option<GuidedGeneratorQueueEntry> {
        match self {
            Self::Empty => None,
            Self::One(_) => {
                let Self::One(entry) = std::mem::replace(self, Self::Empty) else {
                    unreachable!("one-entry frontier changed during removal");
                };
                Some(entry)
            }
            Self::Heap(heap) => heap.pop(),
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::Heap(heap) => heap.len(),
        }
    }

    pub(super) fn capacity(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::Heap(heap) => heap.capacity(),
        }
    }

    pub(super) fn iter(&self) -> GuidedGeneratorEntriesIter<'_> {
        match self {
            Self::Empty => GuidedGeneratorEntriesIter::Inline(None.into_iter()),
            Self::One(entry) => GuidedGeneratorEntriesIter::Inline(Some(entry).into_iter()),
            Self::Heap(heap) => GuidedGeneratorEntriesIter::Heap(heap.iter()),
        }
    }

    pub(super) fn retain(&mut self, mut predicate: impl FnMut(&GuidedGeneratorQueueEntry) -> bool) {
        match self {
            Self::Empty => {}
            Self::One(entry) => {
                if !predicate(entry) {
                    *self = Self::Empty;
                }
            }
            Self::Heap(heap) => heap.retain(predicate),
        }
    }

    /// Reclaims heap allocation only at the generator's existing rebuild
    /// boundary. A heap reduced to one live entry returns to inline storage.
    pub(super) fn shrink_to_fit(&mut self) {
        let Self::Heap(heap) = self else {
            return;
        };
        if heap.len() > 1 {
            heap.shrink_to_fit();
            return;
        }
        let entry = heap.pop();
        *self = entry.map_or(Self::Empty, Self::One);
    }
}

pub(super) enum GuidedGeneratorEntriesIter<'a> {
    Inline(std::option::IntoIter<&'a GuidedGeneratorQueueEntry>),
    Heap(binary_heap::Iter<'a, GuidedGeneratorQueueEntry>),
}

impl<'a> Iterator for GuidedGeneratorEntriesIter<'a> {
    type Item = &'a GuidedGeneratorQueueEntry;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Inline(iter) => iter.next(),
            Self::Heap(iter) => iter.next(),
        }
    }
}

pub(super) struct GuidedGeneratorFrontier {
    pub(super) lane: CombatGuideLaneId,
    pub(super) entries: GuidedGeneratorEntries,
}

pub(super) fn guide_frontier_length_census(
    frontiers: &[GuidedGeneratorFrontier],
) -> ([usize; 5], usize) {
    let mut buckets = [0usize; 5];
    let mut maximum = 0;
    for frontier in frontiers {
        let entries = frontier.entries.len();
        maximum = maximum.max(entries);
        let bucket = match entries {
            0 => 0,
            1 => 1,
            2..=4 => 2,
            5..=16 => 3,
            _ => 4,
        };
        buckets[bucket] = buckets[bucket].saturating_add(1);
    }
    (buckets, maximum)
}
