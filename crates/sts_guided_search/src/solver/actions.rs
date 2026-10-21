//! Incremental action completion and optional preparation followups. Cursors
//! suspend after every charged branch without eagerly forming choice products.
use super::*;

pub(super) struct PendingAction {
    next: Option<Node>,
    stack: Vec<Expansion>,
}

/// Resume the unvisited work, not just its source position. A pending action
/// can own several suspended choice ancestors; their branch indices matter.
pub(super) enum DeferredCursor {
    Ordinary(Expansion),
    Pending(PendingAction),
    Tactical(tactics::TacticalJob),
}

impl DeferredCursor {
    pub(super) fn nodes(&self) -> Box<dyn Iterator<Item = &Node> + '_> {
        match self {
            Self::Ordinary(e) => Box::new(std::iter::once(&e.node)),
            Self::Pending(task) => Box::new(task.nodes()),
            Self::Tactical(job) => Box::new(std::iter::once(&job.node)),
        }
    }

    pub(super) fn remaining_branches(&self) -> usize {
        match self {
            Self::Ordinary(e) => e.branches.len() - e.index,
            Self::Pending(task) => task.remaining_branches(),
            Self::Tactical(_) => 1,
        }
    }
}

impl PendingAction {
    pub(super) fn new(node: Node) -> Self {
        Self {
            next: Some(node),
            stack: vec![],
        }
    }

    pub(super) fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.next.iter().chain(self.stack.iter().map(|e| &e.node))
    }

    fn has_work(&self) -> bool {
        self.next.is_some() || !self.stack.is_empty()
    }

    pub(super) fn remaining_branches(&self) -> usize {
        self.stack.iter().map(|e| e.branches.len() - e.index).sum()
    }

    pub(super) fn branches_by_source(&self) -> impl Iterator<Item = (&Node, usize)> {
        self.stack
            .iter()
            .map(|e| (&e.node, e.branches.len() - e.index))
    }
}

pub(super) fn unfinished(node: &Node) -> bool {
    node.state.selected_members().is_some()
        || matches!(
            node.state.position().engine,
            sts_core::state::core::EngineState::PendingChoice(_)
        )
}

/// The same family/expenditure admission used for ordinary expansion. Potions
/// remain genuine choices unless this search explicitly excludes their use.
pub(super) fn permits_branch(
    node: &Node,
    branch: &CombatBranch,
    card_only: bool,
    max_potion_uses: Option<usize>,
) -> bool {
    !matches!(
        branch,
        CombatBranch::Input(ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_))
    ) || (!card_only
        && max_potion_uses.is_none_or(|max| (node.expenditures().explicit_count() as usize) < max)
        && node.remaining_potion_budget().is_none_or(|left| left > 0))
}

/// An unresolved fight with no active enemy is a transient phase boundary.
/// If EndTurn is its only admitted input, reach the real next phase before
/// scoring. Ordinary live-enemy turn ends retain their existing search path.
/// The transition still executes in the charged, resumable engine path.
pub(super) fn forced_end_turn(
    node: &Node,
    space: &CombatSearchSpace,
    card_only: bool,
    max_potion_uses: Option<usize>,
) -> Result<bool, String> {
    if unfinished(node)
        || !matches!(
            node.state.position().engine,
            sts_core::state::core::EngineState::CombatPlayerTurn
        )
        || node
            .state
            .position()
            .combat
            .entities
            .monsters
            .iter()
            .any(|m| m.current_hp > 0 && !m.half_dead && !m.is_escaped)
    {
        return Ok(false);
    }
    let choices = space.prepare_choices(&node.state)?;
    let mut admitted = choices
        .branches()
        .iter()
        .filter(|branch| permits_branch(node, branch, card_only, max_potion_uses));
    Ok(matches!(
        admitted.next(),
        Some(CombatBranch::Input(ClientInput::EndTurn))
    ) && admitted.next().is_none())
}

/// Materialize a single-card hand's small real decision before another score
/// cut: ending the turn or playing that card on one target. Neither is forced.
/// Refill effects return to ordinary retention as soon as they expose a wider
/// choice. No pile lookahead, card-name preference or free transition is used.
pub(super) fn narrow_card_choice(
    node: &Node,
    space: &CombatSearchSpace,
    card_only: bool,
    max_potion_uses: Option<usize>,
) -> Result<bool, String> {
    if unfinished(node)
        || node.state.position().combat.zones.hand.len() != 1
        || !matches!(
            node.state.position().engine,
            sts_core::state::core::EngineState::CombatPlayerTurn
        )
    {
        return Ok(false);
    }
    let choices = space.prepare_choices(&node.state)?;
    let mut end = false;
    let mut play = false;
    for branch in choices
        .branches()
        .iter()
        .filter(|branch| permits_branch(node, branch, card_only, max_potion_uses))
    {
        match branch {
            CombatBranch::Input(ClientInput::EndTurn) if !end => end = true,
            CombatBranch::Input(ClientInput::PlayCard { .. }) if !play => play = true,
            _ => return Ok(false),
        }
    }
    Ok(end && play)
}

/// A new hand option or changed player effect may enable a later play. This
/// broad scheduling eligibility does not assert usefulness, price the effect,
/// recognize every synergy, or establish state equivalence.
pub(super) fn offers_followup(before: &CombatSearchState, after: &CombatSearchState) -> bool {
    let a = &before.position().combat;
    let b = &after.position().combat;
    if b.zones.hand.iter().any(|card| !a.zones.hand.contains(card)) {
        return true;
    }
    let before =
        sts_core::content::powers::store::powers_for(a, a.entities.player.id).unwrap_or(&[]);
    sts_core::content::powers::store::powers_for(b, b.entities.player.id)
        .unwrap_or(&[])
        .iter()
        .any(|power| {
            !before.iter().any(|old| {
                old.power_type == power.power_type
                    && old.amount >= power.amount
                    && old.payload == power.payload
            })
        })
}

impl Pass {
    /// Called only when another unit of work is available. Until then all
    /// cursors remain in place, including a newly generated child.
    pub(super) fn park_expansion(&mut self, first_result_priority: bool) {
        let expansion = self.expansion.take().filter(|e| e.index < e.branches.len());
        if let Some(mut task) = self.resolving.take() {
            if let Some(expansion) = expansion {
                task.stack.push(expansion);
            }
            if task.has_work() {
                if first_result_priority && task.next.is_some() {
                    // Finish one open opportunity chain before admitting more
                    // work. Its unvisited siblings still rotate after a result;
                    // neither the action product nor the turn is drained here.
                    self.pending.push_front(task);
                    self.service_pending = true;
                } else {
                    self.pending.push_back(task);
                }
            }
        } else if let Some(expansion) = expansion {
            // A completed tactical step has no remaining branches. It must not
            // erase the ordinary expansion parked before that step was served.
            debug_assert!(self.ordinary.is_none());
            self.ordinary = Some(expansion);
        }
    }

    pub(super) fn pending_nodes(&self) -> impl Iterator<Item = &Node> {
        self.pending
            .iter()
            .chain(self.resolving.iter())
            .flat_map(PendingAction::nodes)
    }

    pub(super) fn tactical_jobs(&self) -> impl Iterator<Item = &tactics::TacticalJob> {
        self.tactical.iter().chain(
            self.deferred_cursors
                .iter()
                .filter_map(|cursor| match cursor {
                    DeferredCursor::Tactical(job) => Some(job),
                    _ => None,
                }),
        )
    }

    pub(super) fn defer_turn(&mut self, stats: &mut SolverStats) {
        audit::PassAudit::quota(self, stats.work());
        if let Some(e) = self.ordinary.take() {
            self.defer_cursor(DeferredCursor::Ordinary(e), stats);
        }
        for task in std::mem::take(&mut self.pending) {
            self.defer_cursor(DeferredCursor::Pending(task), stats);
        }
        let tactical = std::mem::take(&mut self.tactical);
        for job in tactical {
            let attack = job.plan.is_attack();
            let response = job.plan.is_response();
            if self.defer_cursor(DeferredCursor::Tactical(job), stats) {
                continue;
            }
            if attack {
                stats.attack_plans_quota_stopped += 1;
            } else if response {
                stats.response_plans_quota_stopped += 1;
            } else {
                stats.effect_plans_quota_stopped += 1;
            }
        }
        let nodes: Vec<Node> = std::mem::take(&mut self.active)
            .into_iter()
            .chain(std::mem::take(&mut self.next))
            .collect();
        stats.turn_budget_dropped += nodes.len();
        if let Some(audit) = &mut self.audit {
            for node in &nodes {
                audit.note(node, "turn_quota", stats.work());
            }
        }
        self.defer(nodes, stats);
    }

    fn defer_cursor(&mut self, cursor: DeferredCursor, stats: &mut SolverStats) -> bool {
        stats.turn_budget_dropped += cursor.nodes().count();
        if let Some(audit) = &mut self.audit {
            for node in cursor.nodes() {
                audit.note(node, "turn_quota", stats.work());
            }
        }
        // Keep the existing slot bound, giving already prepared work priority
        // over unexpanded score omissions. Cursors cannot be deduplicated by
        // source state: equal sources may own different unvisited branches.
        if self.deferred_cursors.len() == self.width * 8 {
            stats.quota_cursors_dropped += 1;
            if let DeferredCursor::Tactical(job) = &cursor {
                self.record_effect_proposal(&job.plan, &job.node);
            }
            if let Some(audit) = &mut self.audit {
                for node in cursor.nodes() {
                    audit.note(node, "cursor_full", stats.work());
                }
            }
            return false;
        }
        if self.deferred.len() + self.deferred_cursors.len() == self.width * 8 {
            let node = self.deferred.pop_back().unwrap();
            self.remove_deferred_index(&node);
            stats.deferred_limit_dropped += 1;
            if let Some(audit) = &mut self.audit {
                audit.note(&node, "reserve_evicted_for_cursor", stats.work());
            }
        }
        if let Some(audit) = &mut self.audit {
            for node in cursor.nodes() {
                audit.note(node, "cursor_saved", stats.work());
            }
        }
        self.deferred_cursors.push_back(cursor);
        stats.quota_cursors_saved += 1;
        true
    }

    pub(super) fn enqueue_resolution(
        &mut self,
        node: Node,
        stats: &mut SolverStats,
        first_result_priority: bool,
    ) {
        if let Some(audit) = &mut self.audit {
            audit.note(&node, "resolution_queued", stats.work());
        }
        if let Some(task) = &mut self.resolving {
            debug_assert!(task.next.is_none());
            task.next = Some(node);
        } else if self.pending.len() < self.width * 8 {
            stats.pending_actions_started += 1;
            if first_result_priority {
                // Backpressure: a newly admitted task can produce a result
                // before the ordinary producer creates another task.
                self.pending.push_front(PendingAction::new(node));
                self.service_pending = true;
            } else {
                self.pending.push_back(PendingAction::new(node));
            }
        } else {
            stats.pending_action_limit_dropped += 1;
            if let Some(audit) = &mut self.audit {
                audit.note(&node, "resolution_limit", stats.work());
            }
            self.defer([node], stats);
        }
    }

    pub(super) fn prepare_resolution(
        &mut self,
        space: &CombatSearchSpace,
        config: &SolverConfig,
        stats: &mut SolverStats,
    ) -> Result<bool, String> {
        let ordinary_ready = self.ordinary.is_some() || !self.active.is_empty();
        if !self.pending.is_empty() && (self.service_pending || !ordinary_ready) {
            let mut task = self.pending.pop_front().unwrap();
            self.expansion = Some(if let Some(node) = task.next.take() {
                self.prepare_node(node, space, config, stats)?
            } else {
                task.stack.pop().unwrap()
            });
            self.resolving = Some(task);
            self.service_pending = false;
            return Ok(true);
        }
        self.service_pending = true;
        if let Some(expansion) = self.ordinary.take() {
            self.expansion = Some(expansion);
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::cards::CardId;
    use sts_core::state::core::{HandSelectReason, PendingChoice};

    #[test]
    fn quota_cursor_displaces_only_unexpanded_nodes_and_resumes_unvisited_branch() {
        let root = super::super::tests::root(&[CardId::Strike, CardId::Defend], 20);
        let config = SolverConfig {
            initial_width: 1,
            max_width: 1,
            tactical_attacks: false,
            ..Default::default()
        };
        let mut solver = CombatSolver::new(root.clone(), config.clone()).unwrap();
        let pass = &mut solver.beams[0].passes[0];
        let source = pass.active.pop_front().unwrap();
        let mut e = pass
            .prepare_node(source.clone(), &solver.space, &config, &mut solver.stats)
            .unwrap();
        assert!(e.branches.len() > 1);
        e.index = 1; // EndTurn was already visited; resume the next real branch.
        let expected_input = e.choices.input(e.branches[1]).unwrap().unwrap();
        pass.ordinary = Some(e);
        for hp in 10..18 {
            let mut position = root.clone();
            position.combat.entities.monsters[0].current_hp = hp;
            pass.defer(
                [Node::root(CombatSearchState::new(position).unwrap())],
                &mut solver.stats,
            );
        }
        pass.defer_turn(&mut solver.stats);
        assert_eq!(pass.deferred.len(), 7);
        assert_eq!(pass.deferred_cursors.len(), 1);
        assert_eq!(pass.deferred_index.values().map(Vec::len).sum::<usize>(), 7);
        assert_eq!(solver.stats.deferred_limit_dropped, 1);
        assert_eq!(solver.stats.work(), 0, "saving a cursor does no simulation");
        solver.advance(1, Duration::from_secs(5)).unwrap();
        assert_eq!(solver.stats.quota_cursors_resumed, 1);
        assert_eq!(solver.stats.search_transitions, 1);
        assert_eq!(solver.stats.work(), 1);
        let child = solver
            .sample_prefixes(16)
            .into_iter()
            .find(|p| p.inputs == [expected_input.clone()])
            .unwrap();
        assert_eq!(
            crate::shuffle_order::replay(&root, &[expected_input])
                .unwrap()
                .last(),
            Some(&child.successor)
        );
        assert_eq!(solver.root(), &root);
    }

    #[test]
    fn quota_pending_cursor_keeps_selection_members_and_unvisited_ancestors() {
        let mut position =
            super::super::tests::root(&[CardId::Strike, CardId::Defend, CardId::Bash], 20);
        position.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3]),
            min_cards: 1,
            max_cards: 2,
            can_cancel: false,
            reason: HandSelectReason::Exhaust,
        });
        let space = CombatSearchSpace::default();
        let state = CombatSearchState::new(position).unwrap();
        let selecting = space
            .virtual_successor(&state, &CombatBranch::BeginSelection(0))
            .unwrap();
        let selected = space
            .virtual_successor(&selecting, &CombatBranch::SelectMember(1))
            .unwrap();
        let source = Node::root(selecting);
        let mut pass = Pass::new(source.clone(), 1, false);
        pass.active.clear();
        let config = SolverConfig {
            complete_actions: true,
            tactical_attacks: false,
            ..Default::default()
        };
        let mut stats = SolverStats::default();
        let mut ancestor = pass
            .prepare_node(source.clone(), &space, &config, &mut stats)
            .unwrap();
        ancestor.index = 2;
        let branches = ancestor.branches.clone();
        pass.pending.push_back(PendingAction {
            next: Some(Node::root(selected.clone())),
            stack: vec![ancestor],
        });
        // Same exact source but different unvisited work must not merge.
        let mut other = pass
            .prepare_node(source, &space, &config, &mut stats)
            .unwrap();
        other.index = 1;
        pass.ordinary = Some(other);
        pass.defer_turn(&mut stats);
        assert_eq!(pass.deferred_cursors.len(), 2);
        // FIFO resumes the ordinary cursor first, with its existing index.
        assert!(pass.prepare(&space, &config, &mut stats, None).unwrap());
        assert_eq!(pass.expansion.take().unwrap().index, 1);
        assert!(pass.prepare(&space, &config, &mut stats, None).unwrap());
        assert_eq!(pass.expansion.as_ref().unwrap().node.state, selected);
        let task = pass.resolving.as_ref().unwrap();
        assert_eq!(task.stack[0].index, 2);
        assert_eq!(task.stack[0].branches, branches);
        assert_eq!(stats.work(), 0);
        assert_eq!(stats.quota_cursors_resumed, 2);
    }

    #[test]
    fn quota_cursors_share_the_reserve_cap_without_merging_work_at_equal_sources() {
        let source = Node::root(
            CombatSearchState::new(super::super::tests::root(&[CardId::Strike], 20)).unwrap(),
        );
        let mut pass = Pass::new(source.clone(), 1, false);
        let mut stats = SolverStats::default();
        let space = CombatSearchSpace::default();
        for i in 0..9 {
            let e = pass
                .prepare_node(source.clone(), &space, &SolverConfig::default(), &mut stats)
                .unwrap();
            assert_eq!(
                pass.defer_cursor(DeferredCursor::Ordinary(e), &mut stats),
                i < 8
            );
        }
        pass.defer([source], &mut stats);
        assert_eq!(pass.deferred_cursors.len(), 8);
        assert!(pass.deferred.is_empty());
        assert_eq!(stats.quota_cursors_saved, 8);
        assert_eq!(stats.quota_cursors_dropped, 1);
        assert_eq!(stats.deferred_limit_dropped, 1);
    }
}
