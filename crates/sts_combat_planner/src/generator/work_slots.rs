use super::{GeneratorWork, TurnOptionGeneratorSession};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct GeneratorWorkHandle {
    pub(super) work_id: usize,
    pub(super) sequence_id: u64,
}

impl TurnOptionGeneratorSession {
    pub(super) fn publish_work_slot(
        &mut self,
        work: GeneratorWork,
        guide_entries: usize,
    ) -> GeneratorWorkHandle {
        let sequence_id = self.next_sequence_id;
        let work_id = if let Some(work_id) = self.free_work_ids.pop() {
            debug_assert!(self.work[work_id].is_none());
            self.work[work_id] = Some(work);
            self.work_sequence_ids[work_id] = sequence_id;
            self.guide_entries_per_work[work_id] = guide_entries;
            self.reused_work_slots = self.reused_work_slots.saturating_add(1);
            work_id
        } else {
            let work_id = self.work.len();
            self.work.push(Some(work));
            self.work_sequence_ids.push(sequence_id);
            self.guide_entries_per_work.push(guide_entries);
            work_id
        };
        self.live_work_items = self.live_work_items.saturating_add(1);
        self.live_guide_entries = self.live_guide_entries.saturating_add(guide_entries);
        GeneratorWorkHandle {
            work_id,
            sequence_id,
        }
    }

    pub(super) fn is_live_work_handle(&self, handle: GeneratorWorkHandle) -> bool {
        self.work_sequence_ids
            .get(handle.work_id)
            .is_some_and(|sequence_id| *sequence_id == handle.sequence_id)
            && self.work.get(handle.work_id).is_some_and(Option::is_some)
    }

    pub(super) fn live_work(&self, handle: GeneratorWorkHandle) -> Option<&GeneratorWork> {
        self.is_live_work_handle(handle)
            .then(|| self.work[handle.work_id].as_ref())
            .flatten()
    }

    pub(super) fn live_work_handle(&self, work_id: usize) -> Option<GeneratorWorkHandle> {
        self.work
            .get(work_id)
            .is_some_and(Option::is_some)
            .then(|| GeneratorWorkHandle {
                work_id,
                sequence_id: self.work_sequence_ids[work_id],
            })
    }

    pub(super) fn take_live_work(&mut self, handle: GeneratorWorkHandle) -> GeneratorWork {
        debug_assert_eq!(self.work_sequence_ids.len(), self.work.len());
        debug_assert_eq!(self.guide_entries_per_work.len(), self.work.len());
        assert!(
            self.is_live_work_handle(handle),
            "scheduled generator work handle must still be live"
        );
        let work = self.work[handle.work_id]
            .take()
            .expect("live generator work slot contains work");
        self.free_work_ids.push(handle.work_id);
        self.live_work_items = self.live_work_items.saturating_sub(1);
        let guide_entries = self.guide_entries_per_work[handle.work_id];
        debug_assert!(self.live_guide_entries >= guide_entries);
        self.live_guide_entries = self.live_guide_entries.saturating_sub(guide_entries);
        work
    }
}
