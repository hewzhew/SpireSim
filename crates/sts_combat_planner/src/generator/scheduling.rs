use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Instant;

use crate::policy::{CombatGuideLaneId, CombatStateGuide};

use super::guide_frontier::{
    GuidedGeneratorEntries, GuidedGeneratorFrontier, GuidedGeneratorQueueEntry,
};
use super::{elapsed_nanos_u64, GeneratorWork, GeneratorWorkHandle, TurnOptionGeneratorSession};

#[derive(Clone, Copy, Debug)]
pub(crate) enum TurnOptionGeneratorPreferredLane {
    Anchor,
    Guide(CombatGuideLaneId),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct GeneratorWorkPriority {
    pub(super) levin_log_priority: f64,
    pub(super) atomic_depth: usize,
    pub(super) negative_log_policy: f64,
}

impl GeneratorWorkPriority {
    pub(super) fn for_path(atomic_depth: usize, negative_log_policy: f64) -> Self {
        Self {
            levin_log_priority: (atomic_depth.max(1) as f64).ln() + negative_log_policy,
            atomic_depth,
            negative_log_policy,
        }
    }
}

impl Eq for GeneratorWorkPriority {}

impl PartialEq for GeneratorWorkPriority {
    fn eq(&self, other: &Self) -> bool {
        self.levin_log_priority.to_bits() == other.levin_log_priority.to_bits()
    }
}

impl Ord for GeneratorWorkPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap; reverse the finite Levin cost so the least
        // expensive retained path is selected first.
        other.levin_log_priority.total_cmp(&self.levin_log_priority)
    }
}

impl PartialOrd for GeneratorWorkPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) struct GeneratorQueueEntry {
    pub(super) priority: GeneratorWorkPriority,
    pub(super) sequence_id: u64,
    pub(super) work_id: usize,
}

impl Eq for GeneratorQueueEntry {}

impl PartialEq for GeneratorQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence_id == other.sequence_id
    }
}

impl Ord for GeneratorQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.sequence_id.cmp(&self.sequence_id))
    }
}

impl PartialOrd for GeneratorQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PushWorkTiming {
    pub(super) guide_elapsed_ns: u64,
    pub(super) retain_elapsed_ns: u64,
    pub(super) agenda_elapsed_ns: u64,
}

impl TurnOptionGeneratorSession {
    pub(crate) fn prefer_lane(&mut self, preferred: TurnOptionGeneratorPreferredLane) {
        self.next_scheduler_lane = match preferred {
            TurnOptionGeneratorPreferredLane::Anchor => 0,
            TurnOptionGeneratorPreferredLane::Guide(lane) => self
                .guide_frontier_index(lane)
                .map_or(0, |frontier_index| frontier_index.saturating_add(1)),
        };
    }

    pub(super) fn push_work(
        &mut self,
        work: GeneratorWork,
        priority: GeneratorWorkPriority,
    ) -> GeneratorWorkHandle {
        self.push_work_measured(work, priority, false).0
    }

    pub(super) fn push_work_measured(
        &mut self,
        mut work: GeneratorWork,
        priority: GeneratorWorkPriority,
        measure: bool,
    ) -> (GeneratorWorkHandle, PushWorkTiming) {
        debug_assert!(priority.levin_log_priority.is_finite());
        let guide_started = measure.then(Instant::now);
        let base_guides = match &mut work {
            GeneratorWork::AtomicActions(cursor) => cursor.guides.clone(),
            GeneratorWork::Expand(partial) => {
                if let Some(guides) = partial.generation_guides.as_ref() {
                    guides.clone()
                } else {
                    let guides: Arc<[CombatStateGuide]> =
                        self.policy.turn_generation_guides(&partial.position).into();
                    Arc::make_mut(partial).generation_guides = Some(guides.clone());
                    guides
                }
            }
            GeneratorWork::StructuredSelection(selection) => selection
                .parent
                .generation_guides
                .clone()
                .unwrap_or_else(|| {
                    self.policy
                        .turn_generation_guides(&selection.parent.position)
                        .into()
                }),
        };
        let guides = base_guides;
        let guide_elapsed_ns = guide_started.map(elapsed_nanos_u64).unwrap_or(0);

        let retain_started = measure.then(Instant::now);
        let ready_transition = matches!(
            work,
            GeneratorWork::AtomicActions(_) | GeneratorWork::StructuredSelection(_)
        );
        let follows_intent = self.work_follows_selection_intent(&work);
        let handle = self.publish_work_slot(work, guides.len());
        let entry = GeneratorQueueEntry {
            priority,
            sequence_id: handle.sequence_id,
            work_id: handle.work_id,
        };
        let retain_elapsed_ns = retain_started.map(elapsed_nanos_u64).unwrap_or(0);

        let agenda_started = measure.then(Instant::now);
        self.anchor_frontier.push(entry);
        if follows_intent {
            self.selection_intent_frontier.push(GeneratorQueueEntry {
                priority,
                sequence_id: handle.sequence_id,
                work_id: handle.work_id,
            });
        }
        for guide in guides.iter() {
            let frontier_index = self.ensure_guide_frontier(guide.lane);
            self.guided_frontiers[frontier_index]
                .entries
                .push(GuidedGeneratorQueueEntry {
                    guide_lane: guide.lane,
                    work_id: handle.work_id,
                    sequence_id: handle.sequence_id,
                    guide_rank: guide.rank.clone(),
                    ready_transition,
                    anchor_priority: priority,
                });
        }
        self.next_sequence_id = self.next_sequence_id.saturating_add(1);
        let agenda_elapsed_ns = agenda_started.map(elapsed_nanos_u64).unwrap_or(0);
        (
            handle,
            PushWorkTiming {
                guide_elapsed_ns,
                retain_elapsed_ns,
                agenda_elapsed_ns,
            },
        )
    }

    #[cfg(test)]
    pub(super) fn pop_scheduled_work(&mut self) -> Option<GeneratorWork> {
        let lane_count = self.guided_frontiers.len().saturating_add(1);
        for offset in 0..lane_count {
            let lane = (self.next_scheduler_lane + offset) % lane_count;
            let handle = if lane == 0 {
                self.pop_anchor_work_handle()
            } else {
                self.pop_guided_work_handle(lane - 1)
            };
            let Some(handle) = handle else {
                continue;
            };
            let work = self.take_live_work(handle);
            if lane == 0 {
                self.anchor_work_pops = self.anchor_work_pops.saturating_add(1);
            } else {
                self.guided_work_pops = self.guided_work_pops.saturating_add(1);
            }
            self.next_scheduler_lane = (lane + 1) % lane_count;
            return Some(work);
        }
        None
    }

    pub(super) fn peek_anchor_work_handle(&mut self) -> Option<GeneratorWorkHandle> {
        while let Some(entry) = self.anchor_frontier.peek() {
            let handle = GeneratorWorkHandle {
                work_id: entry.work_id,
                sequence_id: entry.sequence_id,
            };
            if self.is_live_work_handle(handle) {
                return Some(handle);
            }
            self.anchor_frontier.pop();
        }
        None
    }

    #[cfg(test)]
    pub(super) fn pop_anchor_work_handle(&mut self) -> Option<GeneratorWorkHandle> {
        self.peek_anchor_work_handle()?;
        self.anchor_frontier.pop().map(|entry| GeneratorWorkHandle {
            work_id: entry.work_id,
            sequence_id: entry.sequence_id,
        })
    }

    pub(super) fn peek_guided_work_handle(
        &mut self,
        guide_index: usize,
    ) -> Option<GeneratorWorkHandle> {
        loop {
            let entry = self.guided_frontiers.get(guide_index)?.entries.peek()?;
            let handle = GeneratorWorkHandle {
                work_id: entry.work_id,
                sequence_id: entry.sequence_id,
            };
            if self.is_live_work_handle(handle) {
                return Some(handle);
            }
            self.guided_frontiers[guide_index].entries.pop();
        }
    }

    #[cfg(test)]
    pub(super) fn pop_guided_work_handle(
        &mut self,
        guide_index: usize,
    ) -> Option<GeneratorWorkHandle> {
        self.peek_guided_work_handle(guide_index)?;
        self.guided_frontiers[guide_index]
            .entries
            .pop()
            .map(|entry| GeneratorWorkHandle {
                work_id: entry.work_id,
                sequence_id: entry.sequence_id,
            })
    }

    pub(super) fn guide_frontier_index(&self, lane: CombatGuideLaneId) -> Option<usize> {
        self.guided_frontiers
            .iter()
            .position(|frontier| frontier.lane == lane)
    }

    pub(super) fn ensure_guide_frontier(&mut self, lane: CombatGuideLaneId) -> usize {
        if let Some(index) = self.guide_frontier_index(lane) {
            return index;
        }
        self.guided_frontiers.push(GuidedGeneratorFrontier {
            lane,
            entries: GuidedGeneratorEntries::new(),
        });
        self.guided_frontiers.len() - 1
    }
}
