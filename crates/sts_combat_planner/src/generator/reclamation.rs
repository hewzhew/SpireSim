use super::TurnOptionGeneratorSession;

impl TurnOptionGeneratorSession {
    /// Rebuilds lazy scheduling heaps only between frozen service rounds.
    ///
    /// A strict stale majority both amortizes the linear retain and bounds
    /// retained garbage without changing any live entry's `Ord` key.
    pub(super) fn reclaim_stale_scheduling_entries(&mut self) {
        debug_assert!(self.scheduled_round.is_empty());
        self.reclaim_selection_intent_entries();

        let stale_anchor_entries = self
            .anchor_frontier
            .len()
            .saturating_sub(self.live_work_items);
        let guide_entries = self
            .guided_frontiers
            .iter()
            .map(|frontier| frontier.entries.len())
            .sum::<usize>();
        let stale_guide_entries = guide_entries.saturating_sub(self.live_guide_entries);
        let rebuild_anchor = stale_anchor_entries > self.live_work_items;
        let rebuild_guides = stale_guide_entries > self.live_guide_entries;
        if !rebuild_anchor && !rebuild_guides {
            return;
        }

        let work = &self.work;
        let work_sequence_ids = &self.work_sequence_ids;
        let mut reclaimed_anchor_entries = 0;
        if rebuild_anchor {
            let before = self.anchor_frontier.len();
            self.anchor_frontier.retain(|entry| {
                work_sequence_ids
                    .get(entry.work_id)
                    .is_some_and(|sequence_id| *sequence_id == entry.sequence_id)
                    && work.get(entry.work_id).is_some_and(Option::is_some)
            });
            reclaimed_anchor_entries = before.saturating_sub(self.anchor_frontier.len());
            self.anchor_frontier.shrink_to_fit();
        }

        let mut reclaimed_guide_entries = 0usize;
        if rebuild_guides {
            for frontier in &mut self.guided_frontiers {
                let before = frontier.entries.len();
                frontier.entries.retain(|entry| {
                    work_sequence_ids
                        .get(entry.work_id)
                        .is_some_and(|sequence_id| *sequence_id == entry.sequence_id)
                        && work.get(entry.work_id).is_some_and(Option::is_some)
                });
                let reclaimed = before.saturating_sub(frontier.entries.len());
                reclaimed_guide_entries = reclaimed_guide_entries.saturating_add(reclaimed);
                if reclaimed > 0 {
                    frontier.entries.shrink_to_fit();
                }
            }
        }

        self.scheduling_rebuilds = self.scheduling_rebuilds.saturating_add(1);
        self.reclaimed_anchor_entries = self
            .reclaimed_anchor_entries
            .saturating_add(reclaimed_anchor_entries);
        self.reclaimed_guide_entries = self
            .reclaimed_guide_entries
            .saturating_add(reclaimed_guide_entries);
    }
}
