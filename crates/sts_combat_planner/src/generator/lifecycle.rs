use super::*;

impl TurnOptionGeneratorSession {
    /// Release search-only ownership after the caller has externalized every
    /// completed option and generation gap from a finished session.
    ///
    /// Counters, timing, the exact root, and finished status remain available.
    /// Historical atomic-state membership and queue placement deliberately do
    /// not: local-turn graph diagnostics operate on admitted boundary nodes,
    /// while standalone membership audits never call this retirement hook.
    pub(crate) fn retire_finished_search_storage(&mut self) {
        debug_assert!(self.is_finished());
        debug_assert!(self.completed.is_empty());

        self.work = Vec::new();
        self.work_sequence_ids = Vec::new();
        self.free_work_ids = Vec::new();
        self.guide_entries_per_work = Vec::new();
        self.anchor_frontier = BinaryHeap::new();
        self.guided_frontiers = Vec::new();
        self.selection_intent_frontier = BinaryHeap::new();
        self.scheduled_round = VecDeque::new();
        self.live_guide_entries = 0;
        self.seen = HashSet::with_hasher(FxBuildHasher);
        self.completed = Vec::new();
        self.gaps = Vec::new();
    }
}
