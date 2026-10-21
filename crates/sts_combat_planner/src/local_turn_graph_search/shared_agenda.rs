use super::*;
use crate::CombatGuideServiceBias;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

#[derive(Clone, Debug)]
struct AnchorEntry {
    node_id: usize,
    service_cost: f64,
}

impl PartialEq for AnchorEntry {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id && self.service_cost.total_cmp(&other.service_cost).is_eq()
    }
}

impl Eq for AnchorEntry {}

impl PartialOrd for AnchorEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AnchorEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max heap. Smaller search cost and then smaller node
        // id are therefore ordered as the greater entry.
        other
            .service_cost
            .total_cmp(&self.service_cost)
            .then_with(|| other.node_id.cmp(&self.node_id))
    }
}

#[derive(Clone, Debug)]
struct GuideEntry {
    node_id: usize,
    rank: CombatStateGuideRank,
    path_cost: f64,
}

impl PartialEq for GuideEntry {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id
            && self.rank == other.rank
            && self.path_cost.total_cmp(&other.path_cost).is_eq()
    }
}

impl Eq for GuideEntry {}

impl PartialOrd for GuideEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GuideEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // BTreeSet iterates in ascending order. Put the strongest semantic
        // rank first, then the cheapest retained exact path.
        other
            .rank
            .cmp(&self.rank)
            .then_with(|| self.path_cost.total_cmp(&other.path_cost))
            .then_with(|| self.node_id.cmp(&other.node_id))
    }
}

/// Independent scheduling views over one shared exact-state graph.
///
/// A view chooses a boundary node. It never owns expansion below that node:
/// the selected node's turn generator chooses its own local generation lane.
pub(super) struct SharedBoundaryAgenda {
    anchor: BinaryHeap<AnchorEntry>,
    proposal_roots: VecDeque<usize>,
    proposal_root_members: BTreeSet<usize>,
    proposal_continuations: VecDeque<usize>,
    proposal_continuation_members: BTreeSet<usize>,
    proposal_service_claims: BTreeSet<usize>,
    guides: BTreeMap<CombatGuideLaneId, BTreeSet<GuideEntry>>,
    charged_generation_work: BTreeMap<LocalServiceView, usize>,
    next_tie_break: usize,
    guide_service_bias: Option<CombatGuideServiceBias>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SharedAgendaPosition {
    pub(super) ordinal_rank: Option<usize>,
    pub(super) candidate_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SharedAnchorPosition {
    pub(super) agenda: SharedAgendaPosition,
    pub(super) service_cost: Option<f64>,
    pub(super) best_service_cost: Option<f64>,
}

impl SharedBoundaryAgenda {
    pub(super) fn new(guide_service_bias: Option<CombatGuideServiceBias>) -> Self {
        Self {
            anchor: BinaryHeap::new(),
            proposal_roots: VecDeque::new(),
            proposal_root_members: BTreeSet::new(),
            proposal_continuations: VecDeque::new(),
            proposal_continuation_members: BTreeSet::new(),
            proposal_service_claims: BTreeSet::new(),
            guides: BTreeMap::new(),
            charged_generation_work: BTreeMap::new(),
            next_tie_break: 0,
            guide_service_bias,
        }
    }

    pub(super) fn clear(&mut self) {
        self.anchor.clear();
        self.proposal_roots.clear();
        self.proposal_root_members.clear();
        self.proposal_continuations.clear();
        self.proposal_continuation_members.clear();
        self.proposal_service_claims.clear();
        self.guides.clear();
        self.charged_generation_work.clear();
    }

    pub(super) fn publish_node(&mut self, node_id: usize, node: &GraphNode) {
        if node.generator.is_finished() || node.exhausted {
            return;
        }
        self.anchor.push(AnchorEntry {
            node_id,
            service_cost: anchor_service_cost(node),
        });
        self.publish_guide_entries(node_id, node);
    }

    pub(super) fn publish_guide_entries(&mut self, node_id: usize, node: &GraphNode) {
        if node.generator.is_finished() || node.exhausted {
            return;
        }
        for guide in &node.guides {
            self.publish_guide_entry(node_id, node, guide.lane);
        }
    }

    pub(super) fn publish_guide_entry(
        &mut self,
        node_id: usize,
        node: &GraphNode,
        lane: CombatGuideLaneId,
    ) {
        if node.generator.is_finished() || node.exhausted {
            return;
        }
        let Some(rank) = guide_rank(node, lane) else {
            return;
        };
        self.guides.entry(lane).or_default().insert(GuideEntry {
            node_id,
            rank: rank.clone(),
            path_cost: node.path_cost(),
        });
    }

    pub(super) fn remove_guide_entries(&mut self, node_id: usize, node: &GraphNode) {
        for guide in &node.guides {
            if let Some(entries) = self.guides.get_mut(&guide.lane) {
                entries.remove(&GuideEntry {
                    node_id,
                    rank: guide.rank.clone(),
                    path_cost: node.path_cost(),
                });
            }
        }
    }

    pub(super) fn republish_anchor(&mut self, node_id: usize, node: &GraphNode) {
        if node.generator.is_finished() || node.exhausted {
            return;
        }
        self.anchor.push(AnchorEntry {
            node_id,
            service_cost: anchor_service_cost(node),
        });
    }

    /// Gives one exact boundary with an applicable typed proposal one
    /// ordinary, auditable service opportunity.
    ///
    /// The node remains in the anchor agenda for completeness. This queue is
    /// FIFO, deduplicated while pending, and one-shot: selecting an entry
    /// never recursively services its descendants.
    pub(super) fn publish_proposal_root(&mut self, node_id: usize, node: &GraphNode) -> bool {
        if node.generator.is_finished()
            || node.exhausted
            || !self.proposal_service_claims.insert(node_id)
        {
            return false;
        }
        self.proposal_root_members.insert(node_id);
        self.proposal_roots.push_back(node_id);
        true
    }

    /// Gives the exact successor of a materialized proposal one opportunity
    /// in a queue independent from newly applicable proposal roots.
    ///
    /// If the successor was already waiting as an applicable root, its one
    /// lifetime proposal claim moves to the continuation queue. Once either
    /// proposal view services a node, later duplicate exact edges cannot
    /// restore proposal privilege.
    pub(super) fn publish_proposal_continuation(
        &mut self,
        node_id: usize,
        node: &GraphNode,
    ) -> bool {
        if node.generator.is_finished() || node.exhausted {
            return false;
        }
        self.enqueue_proposal_continuation(node_id)
    }

    fn enqueue_proposal_continuation(&mut self, node_id: usize) -> bool {
        if self.proposal_continuation_members.contains(&node_id) {
            return false;
        }
        if self.proposal_root_members.remove(&node_id) {
            self.proposal_roots
                .retain(|candidate_id| *candidate_id != node_id);
        } else if !self.proposal_service_claims.insert(node_id) {
            return false;
        }
        self.proposal_continuation_members.insert(node_id);
        self.proposal_continuations.push_back(node_id);
        true
    }

    pub(super) fn next_service_view(&mut self) -> LocalServiceView {
        let mut views = Vec::with_capacity(self.guides.len().saturating_add(3));
        if !self.anchor.is_empty() {
            views.push(LocalServiceView::Anchor);
        }
        if !self.proposal_continuations.is_empty() {
            views.push(LocalServiceView::ProposalContinuation);
        }
        if !self.proposal_roots.is_empty() {
            views.push(LocalServiceView::ProposalRoot);
        }
        views.extend(
            self.guides
                .iter()
                .filter(|(_, entries)| !entries.is_empty())
                .map(|(lane, _)| LocalServiceView::Guide(*lane)),
        );
        if views.is_empty() {
            // `select_work` will ask the anchor once more before declaring the
            // shared graph exhausted. Returning the anchor here keeps that
            // empty-agenda path total without inventing an active guide.
            return LocalServiceView::Anchor;
        }

        // A first visit can cost 128 work while an ordinary resumed visit
        // costs 4. Scheduling by visit count therefore lets one-shot guides
        // consume almost the whole bounded allowance while the complete
        // anchor stays near the front of its queue but never runs. Compare
        // virtual generation work instead. The completeness anchor owns half
        // of bounded work; active proposal and guide views share the other
        // half. Rotation is only the deterministic tie breaker. A configured
        // guide bias is represented as service weight, matching its former
        // extra-turn semantics within the auxiliary half.
        let rotation = self.next_tie_break % views.len();
        views.rotate_left(rotation);
        let mut selected = views[0];
        for candidate in views.iter().copied().skip(1) {
            if self
                .compare_virtual_work(candidate, selected, &views)
                .is_lt()
            {
                selected = candidate;
            }
        }
        self.next_tie_break = self.next_tie_break.saturating_add(1);
        selected
    }

    pub(super) fn view_count(&self) -> usize {
        self.guides
            .values()
            .filter(|entries| !entries.is_empty())
            .count()
            .saturating_add(usize::from(!self.anchor.is_empty()))
            .saturating_add(usize::from(!self.proposal_continuations.is_empty()))
            .saturating_add(usize::from(!self.proposal_roots.is_empty()))
    }

    pub(super) fn charge_generation_work(&mut self, view: LocalServiceView, work: usize) {
        let charged = self.charged_generation_work.entry(view).or_default();
        *charged = charged.saturating_add(work);
    }

    fn compare_virtual_work(
        &self,
        left: LocalServiceView,
        right: LocalServiceView,
        active_views: &[LocalServiceView],
    ) -> Ordering {
        let left_work = self
            .charged_generation_work
            .get(&left)
            .copied()
            .unwrap_or_default() as u128;
        let right_work = self
            .charged_generation_work
            .get(&right)
            .copied()
            .unwrap_or_default() as u128;
        let left_weight = self.service_weight(left, active_views) as u128;
        let right_weight = self.service_weight(right, active_views) as u128;
        left_work
            .saturating_mul(right_weight)
            .cmp(&right_work.saturating_mul(left_weight))
    }

    fn service_weight(&self, view: LocalServiceView, active_views: &[LocalServiceView]) -> usize {
        if view == LocalServiceView::Anchor {
            // The completeness owner receives half of bounded generation work
            // regardless of how many optional guide lanes happen to exist.
            // The other half is divided among proposal and guide views.
            return active_views
                .iter()
                .copied()
                .filter(|candidate| *candidate != LocalServiceView::Anchor)
                .map(|candidate| self.auxiliary_service_weight(candidate))
                .sum::<usize>()
                .max(1);
        }
        self.auxiliary_service_weight(view)
    }

    fn auxiliary_service_weight(&self, view: LocalServiceView) -> usize {
        match (view, self.guide_service_bias) {
            (LocalServiceView::Guide(lane), Some(bias)) if lane == bias.lane => {
                bias.extra_services_per_cycle.saturating_add(1)
            }
            _ => 1,
        }
    }

    pub(super) fn select_anchor(&mut self, nodes: &[GraphNode]) -> Option<usize> {
        while let Some(entry) = self.anchor.pop() {
            let node = &nodes[entry.node_id];
            if !node.exhausted
                && !node.generator.is_finished()
                && entry
                    .service_cost
                    .total_cmp(&anchor_service_cost(node))
                    .is_eq()
            {
                return Some(entry.node_id);
            }
        }
        None
    }

    pub(super) fn select_guide(
        &mut self,
        lane: CombatGuideLaneId,
        nodes: &[GraphNode],
    ) -> Option<usize> {
        let entries = self.guides.get_mut(&lane)?;
        while let Some(selected) = entries.first().cloned() {
            let valid = {
                let node = &nodes[selected.node_id];
                !node.exhausted
                    && !node.generator.is_finished()
                    && guide_rank(node, lane) == Some(&selected.rank)
                    && node.path_cost().total_cmp(&selected.path_cost).is_eq()
            };
            // One inadmissible guide grants one coherent expansion to a
            // shared exact state, then moves on. Stale entries are removed as
            // they reach the front so they cannot masquerade as an active
            // zero-cost view in the work-fair scheduler.
            entries.remove(&selected);
            if valid {
                return Some(selected.node_id);
            }
        }
        None
    }

    pub(super) fn select_proposal_root(&mut self, nodes: &[GraphNode]) -> Option<usize> {
        while let Some(node_id) = self.proposal_roots.pop_front() {
            self.proposal_root_members.remove(&node_id);
            self.remove_pending_proposal_continuation(node_id);
            let node = &nodes[node_id];
            if !node.exhausted && !node.generator.is_finished() {
                return Some(node_id);
            }
        }
        None
    }

    pub(super) fn select_proposal_continuation(&mut self, nodes: &[GraphNode]) -> Option<usize> {
        while let Some(node_id) = self.proposal_continuations.pop_front() {
            self.proposal_continuation_members.remove(&node_id);
            self.remove_pending_proposal_root(node_id);
            let node = &nodes[node_id];
            if !node.exhausted && !node.generator.is_finished() {
                return Some(node_id);
            }
        }
        None
    }

    fn remove_pending_proposal_root(&mut self, node_id: usize) {
        if self.proposal_root_members.remove(&node_id) {
            self.proposal_roots
                .retain(|candidate_id| *candidate_id != node_id);
        }
    }

    fn remove_pending_proposal_continuation(&mut self, node_id: usize) {
        if self.proposal_continuation_members.remove(&node_id) {
            self.proposal_continuations
                .retain(|candidate_id| *candidate_id != node_id);
        }
    }

    pub(super) fn anchor_position(
        &self,
        node_id: usize,
        nodes: &[GraphNode],
    ) -> SharedAnchorPosition {
        let mut candidates = self
            .anchor
            .iter()
            .filter_map(|entry| {
                let node = &nodes[entry.node_id];
                (!node.exhausted
                    && !node.generator.is_finished()
                    && entry
                        .service_cost
                        .total_cmp(&anchor_service_cost(node))
                        .is_eq())
                .then_some((entry.node_id, entry.service_cost))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(left_id, left_cost), (right_id, right_cost)| {
            left_cost
                .total_cmp(right_cost)
                .then_with(|| left_id.cmp(right_id))
        });
        candidates.dedup_by_key(|(candidate_id, _)| *candidate_id);
        SharedAnchorPosition {
            agenda: SharedAgendaPosition {
                ordinal_rank: candidates
                    .iter()
                    .position(|(candidate_id, _)| *candidate_id == node_id)
                    .map(|index| index.saturating_add(1)),
                candidate_count: candidates.len(),
            },
            service_cost: candidates
                .iter()
                .find(|(candidate_id, _)| *candidate_id == node_id)
                .map(|(_, cost)| *cost),
            best_service_cost: candidates.first().map(|(_, cost)| *cost),
        }
    }

    pub(super) fn proposal_root_position(
        &self,
        node_id: usize,
        nodes: &[GraphNode],
    ) -> SharedAgendaPosition {
        let candidates = self
            .proposal_roots
            .iter()
            .copied()
            .filter(|candidate_id| {
                self.proposal_root_members.contains(candidate_id)
                    && !nodes[*candidate_id].exhausted
                    && !nodes[*candidate_id].generator.is_finished()
            })
            .collect::<Vec<_>>();
        SharedAgendaPosition {
            ordinal_rank: candidates
                .iter()
                .position(|candidate_id| *candidate_id == node_id)
                .map(|index| index.saturating_add(1)),
            candidate_count: candidates.len(),
        }
    }

    pub(super) fn proposal_continuation_position(
        &self,
        node_id: usize,
        nodes: &[GraphNode],
    ) -> SharedAgendaPosition {
        let candidates = self
            .proposal_continuations
            .iter()
            .copied()
            .filter(|candidate_id| {
                self.proposal_continuation_members.contains(candidate_id)
                    && !nodes[*candidate_id].exhausted
                    && !nodes[*candidate_id].generator.is_finished()
            })
            .collect::<Vec<_>>();
        SharedAgendaPosition {
            ordinal_rank: candidates
                .iter()
                .position(|candidate_id| *candidate_id == node_id)
                .map(|index| index.saturating_add(1)),
            candidate_count: candidates.len(),
        }
    }

    pub(super) fn guide_position(
        &self,
        node_id: usize,
        lane: CombatGuideLaneId,
        nodes: &[GraphNode],
    ) -> (SharedAgendaPosition, Option<&CombatStateGuideRank>) {
        let Some(entries) = self.guides.get(&lane) else {
            return (
                SharedAgendaPosition {
                    ordinal_rank: None,
                    candidate_count: 0,
                },
                None,
            );
        };
        let candidates = entries
            .iter()
            .filter(|entry| {
                let node = &nodes[entry.node_id];
                !node.exhausted
                    && !node.generator.is_finished()
                    && guide_rank(node, lane) == Some(&entry.rank)
                    && node.path_cost().total_cmp(&entry.path_cost).is_eq()
            })
            .collect::<Vec<_>>();
        (
            SharedAgendaPosition {
                ordinal_rank: candidates
                    .iter()
                    .position(|entry| entry.node_id == node_id)
                    .map(|index| index.saturating_add(1)),
                candidate_count: candidates.len(),
            },
            candidates.first().map(|entry| &entry.rank),
        )
    }
}

fn anchor_service_cost(node: &GraphNode) -> f64 {
    node.path_cost() + (node.widen_anchor_visits.saturating_add(1) as f64).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_entries_order_stronger_rank_before_cheaper_weaker_path() {
        let strong = GuideEntry {
            node_id: 7,
            rank: CombatStateGuideRank::new(vec![2, 0]),
            path_cost: 100.0,
        };
        let weak = GuideEntry {
            node_id: 3,
            rank: CombatStateGuideRank::new(vec![1, 999]),
            path_cost: 0.0,
        };
        let entries = BTreeSet::from([weak, strong]);
        assert_eq!(entries.first().map(|entry| entry.node_id), Some(7));
    }

    #[test]
    fn anchor_heap_prefers_lower_service_cost() {
        let mut entries = BinaryHeap::from([
            AnchorEntry {
                node_id: 1,
                service_cost: 5.0,
            },
            AnchorEntry {
                node_id: 2,
                service_cost: 1.0,
            },
        ]);
        assert_eq!(entries.pop().map(|entry| entry.node_id), Some(2));
    }

    #[test]
    fn anchor_keeps_half_of_generation_work_across_expensive_guide_services() {
        let lanes = (1..=4).map(CombatGuideLaneId::new).collect::<Vec<_>>();
        let mut agenda = SharedBoundaryAgenda::new(None);
        agenda.anchor.push(AnchorEntry {
            node_id: 0,
            service_cost: 0.0,
        });
        for (node_id, lane) in lanes.iter().copied().enumerate() {
            agenda.guides.insert(
                lane,
                BTreeSet::from([GuideEntry {
                    node_id: node_id.saturating_add(1),
                    rank: CombatStateGuideRank::new(vec![1]),
                    path_cost: 0.0,
                }]),
            );
        }

        let mut anchor_work = 0usize;
        let mut guide_work = 0usize;
        while anchor_work.saturating_add(guide_work) < 4_096 {
            let view = agenda.next_service_view();
            let work = match view {
                LocalServiceView::Anchor => 4,
                LocalServiceView::Guide(_) => 128,
                LocalServiceView::ProposalRoot | LocalServiceView::ProposalContinuation => {
                    unreachable!("test agenda has no proposal queues")
                }
            };
            agenda.charge_generation_work(view, work);
            if view == LocalServiceView::Anchor {
                anchor_work = anchor_work.saturating_add(work);
            } else {
                guide_work = guide_work.saturating_add(work);
            }
        }

        assert!(anchor_work.abs_diff(guide_work) <= 128);
    }

    #[test]
    fn guide_bias_reweights_only_the_auxiliary_half() {
        let boosted_lane = CombatGuideLaneId::new(2);
        let ordinary_lane = CombatGuideLaneId::new(3);
        let mut agenda = SharedBoundaryAgenda::new(Some(CombatGuideServiceBias {
            lane: boosted_lane,
            extra_services_per_cycle: 2,
        }));
        agenda.anchor.push(AnchorEntry {
            node_id: 0,
            service_cost: 0.0,
        });
        for (node_id, lane) in [boosted_lane, ordinary_lane].into_iter().enumerate() {
            agenda.guides.insert(
                lane,
                BTreeSet::from([GuideEntry {
                    node_id: node_id.saturating_add(1),
                    rank: CombatStateGuideRank::new(vec![1]),
                    path_cost: 0.0,
                }]),
            );
        }

        let mut counts = BTreeMap::<LocalServiceView, usize>::new();
        for _ in 0..800 {
            let view = agenda.next_service_view();
            agenda.charge_generation_work(view, 1);
            *counts.entry(view).or_default() += 1;
        }

        let anchor = counts[&LocalServiceView::Anchor];
        let boosted = counts[&LocalServiceView::Guide(boosted_lane)];
        let ordinary = counts[&LocalServiceView::Guide(ordinary_lane)];
        assert!(anchor.abs_diff(boosted.saturating_add(ordinary)) <= 1);
        assert!(boosted.abs_diff(ordinary.saturating_mul(3)) <= 2);
    }

    #[test]
    fn proposal_queues_get_independent_fair_turns_without_replacing_anchor() {
        let lane = CombatGuideLaneId::new(3);
        let mut agenda = SharedBoundaryAgenda::new(None);
        agenda.proposal_continuations.push_back(4);
        agenda.proposal_continuation_members.insert(4);
        agenda.proposal_roots.push_back(9);
        agenda.proposal_root_members.insert(9);
        agenda.guides.insert(lane, BTreeSet::new());
        agenda.anchor.push(AnchorEntry {
            node_id: 0,
            service_cost: 0.0,
        });

        assert_eq!(agenda.view_count(), 3);
        assert_eq!(agenda.next_service_view(), LocalServiceView::Anchor);
        agenda.charge_generation_work(LocalServiceView::Anchor, 1);
        assert_eq!(
            agenda.next_service_view(),
            LocalServiceView::ProposalContinuation
        );
        agenda.charge_generation_work(LocalServiceView::ProposalContinuation, 1);
        assert_eq!(agenda.next_service_view(), LocalServiceView::ProposalRoot);
        agenda.charge_generation_work(LocalServiceView::ProposalRoot, 1);
        assert_eq!(agenda.next_service_view(), LocalServiceView::Anchor);
    }

    #[test]
    fn proposal_continuation_reclassifies_a_pending_root_without_duplicate_privilege() {
        let mut agenda = SharedBoundaryAgenda::new(None);
        agenda.proposal_roots.extend([4, 7]);
        agenda.proposal_root_members.extend([4, 7]);
        agenda.proposal_service_claims.extend([4, 7]);

        assert!(agenda.enqueue_proposal_continuation(7));
        assert_eq!(
            agenda.proposal_roots.iter().copied().collect::<Vec<_>>(),
            [4]
        );
        assert_eq!(
            agenda
                .proposal_continuations
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [7]
        );
        assert!(!agenda.enqueue_proposal_continuation(7));
        agenda.remove_pending_proposal_root(4);
        assert!(!agenda.enqueue_proposal_continuation(4));
        assert!(agenda.enqueue_proposal_continuation(9));
        assert_eq!(
            agenda
                .proposal_continuations
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [7, 9]
        );
    }
}
