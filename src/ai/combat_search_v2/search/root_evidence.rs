use std::collections::{BTreeMap, HashMap};

use super::super::frontier::{RootLineage, RootLineageId, SearchNode};
use super::super::*;
use super::loop_state::SearchLoopState;
use super::node_action_ordering::OrderedNodeAction;
use super::root_round_scheduler::{RootActionScheduleState, ROOT_SCHEDULING_POLICY};

const ROOT_RANKING_POLICY: &str =
    "best_exact_complete_or_priority_queue_head_then_ranked_by_combat_outcome_score_v2";
const ROOT_WORK_ACCOUNTING_SCOPE: &str =
    "generated_expanded_nodes_and_open_frontier_work_items_by_typed_root_lineage_v2";

pub(super) struct RootEvidenceBook {
    next_id: u32,
    by_action_key: HashMap<String, RootLineageId>,
    entries: BTreeMap<RootLineageId, RootEvidenceEntry>,
    unattributed: CombatSearchV2RootWorkEvidence,
    materialization: CombatSearchV2RootMaterializationStatus,
    unmaterialized_pending_work_items: Option<usize>,
}

struct RootEvidenceEntry {
    identity: CombatSearchV2RootActionIdentity,
    work: CombatSearchV2RootWorkEvidence,
}

impl Default for RootEvidenceBook {
    fn default() -> Self {
        Self {
            next_id: 0,
            by_action_key: HashMap::new(),
            entries: BTreeMap::new(),
            unattributed: CombatSearchV2RootWorkEvidence::default(),
            materialization: CombatSearchV2RootMaterializationStatus::NotStarted,
            unmaterialized_pending_work_items: None,
        }
    }
}

impl RootEvidenceBook {
    fn best_exact_win_root(&self, states: &[RootActionScheduleState]) -> Option<RootLineageId> {
        states
            .iter()
            .filter(|state| state.has_work)
            .filter_map(|state| {
                self.entries
                    .get(&state.id)?
                    .work
                    .best_exact_win
                    .as_ref()
                    .map(|win| (state.id, win))
            })
            .max_by(|(left_id, left), (right_id, right)| {
                left.outcome_order_key
                    .cmp(&right.outcome_order_key)
                    .then_with(|| right_id.cmp(left_id))
            })
            .map(|(id, _)| id)
    }

    pub(super) fn materialize_node(&mut self, node: &mut SearchNode) {
        if !matches!(node.root_lineage, RootLineage::Unmaterialized) {
            return;
        }
        let Some(action) = node.actions.first() else {
            return;
        };
        let id = self.register_identity(identity_from_trace(action));
        node.root_lineage = RootLineage::Action(id);
    }

    pub(super) fn observe_enumerated_root_surface(
        &mut self,
        node: &SearchNode,
        choices: &[OrderedNodeAction],
    ) {
        if !matches!(node.root_lineage, RootLineage::Unmaterialized) || !node.actions.is_empty() {
            return;
        }
        for choice in choices {
            self.register_authoritative_identity(CombatSearchV2RootActionIdentity {
                action_id: choice.choice.original_action_id,
                action_key: choice.choice.choice.action_key.clone(),
                action_debug: choice.choice.choice.action_debug.clone(),
            });
        }
        self.materialization = CombatSearchV2RootMaterializationStatus::Complete;
    }

    pub(super) fn mark_unmaterialized_surface_complete(&mut self, node: &SearchNode) {
        if matches!(node.root_lineage, RootLineage::Unmaterialized) && node.actions.is_empty() {
            self.materialization = CombatSearchV2RootMaterializationStatus::Complete;
        }
    }

    pub(super) fn begin_unmaterialized_pending_work(
        &mut self,
        node: &SearchNode,
        work_items: usize,
    ) {
        if !matches!(node.root_lineage, RootLineage::Unmaterialized) || !node.actions.is_empty() {
            return;
        }
        self.unmaterialized_pending_work_items = Some(work_items);
        self.materialization = if work_items == 0 {
            CombatSearchV2RootMaterializationStatus::Complete
        } else {
            CombatSearchV2RootMaterializationStatus::Partial
        };
    }

    pub(super) fn finish_unmaterialized_pending_work(&mut self, node: &SearchNode) {
        if !matches!(node.root_lineage, RootLineage::Unmaterialized) || !node.actions.is_empty() {
            return;
        }
        let Some(remaining) = self.unmaterialized_pending_work_items.as_mut() else {
            return;
        };
        *remaining = remaining.saturating_sub(1);
        if *remaining == 0 {
            self.materialization = CombatSearchV2RootMaterializationStatus::Complete;
        }
    }

    pub(super) fn record_generated(&mut self, node: &SearchNode) {
        let work = self.work_for_node_mut(node);
        work.generated_concrete_nodes = work.generated_concrete_nodes.saturating_add(1);
    }

    pub(super) fn record_expanded(&mut self, node: &SearchNode) {
        let work = self.work_for_node_mut(node);
        work.expanded_concrete_nodes = work.expanded_concrete_nodes.saturating_add(1);
        match node.combat.turn.turn_count {
            0 => work.expanded_turn_zero_nodes = work.expanded_turn_zero_nodes.saturating_add(1),
            1 => work.expanded_turn_one_nodes = work.expanded_turn_one_nodes.saturating_add(1),
            _ => {
                work.expanded_turn_two_or_later_nodes =
                    work.expanded_turn_two_or_later_nodes.saturating_add(1)
            }
        }
        work.max_expanded_turn = work.max_expanded_turn.max(node.combat.turn.turn_count);
        work.max_expanded_action_count = work.max_expanded_action_count.max(node.actions.len());
    }

    pub(super) fn record_bulk_work(
        &mut self,
        source: &SearchNode,
        nodes_expanded: usize,
        nodes_generated: usize,
    ) {
        let work = self.work_for_node_mut(source);
        work.expanded_concrete_nodes = work
            .expanded_concrete_nodes
            .saturating_add(nodes_expanded as u64);
        work.bulk_expanded_nodes_without_depth = work
            .bulk_expanded_nodes_without_depth
            .saturating_add(nodes_expanded as u64);
        work.generated_concrete_nodes = work
            .generated_concrete_nodes
            .saturating_add(nodes_generated as u64);
    }

    pub(super) fn observe_exact_complete(&mut self, node: &SearchNode) {
        let value = observed_value(node);
        remember_best(&mut self.work_for_node_mut(node).best_exact_complete, value);
    }

    pub(super) fn observe_exact_win(&mut self, node: &SearchNode) {
        let value = observed_value(node);
        let work = self.work_for_node_mut(node);
        remember_best(&mut work.best_exact_complete, value.clone());
        remember_best(&mut work.best_exact_win, value);
    }

    fn register_identity(&mut self, identity: CombatSearchV2RootActionIdentity) -> RootLineageId {
        if let Some(id) = self.by_action_key.get(&identity.action_key).copied() {
            return id;
        }
        let id = RootLineageId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.by_action_key.insert(identity.action_key.clone(), id);
        self.entries.insert(
            id,
            RootEvidenceEntry {
                identity,
                work: CombatSearchV2RootWorkEvidence::default(),
            },
        );
        if self.materialization == CombatSearchV2RootMaterializationStatus::NotStarted {
            self.materialization = CombatSearchV2RootMaterializationStatus::Partial;
        }
        id
    }

    fn register_authoritative_identity(
        &mut self,
        identity: CombatSearchV2RootActionIdentity,
    ) -> RootLineageId {
        let id = self.register_identity(identity.clone());
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.identity = identity;
        }
        id
    }

    fn work_for_node_mut(&mut self, node: &SearchNode) -> &mut CombatSearchV2RootWorkEvidence {
        match node.root_lineage {
            RootLineage::Action(id) => self
                .entries
                .get_mut(&id)
                .map(|entry| &mut entry.work)
                .unwrap_or(&mut self.unattributed),
            RootLineage::Unmaterialized => &mut self.unattributed,
        }
    }
}

impl SearchLoopState {
    pub(in crate::ai::combat_search_v2::search) fn root_surface_fully_materialized(&self) -> bool {
        self.root_evidence.materialization == CombatSearchV2RootMaterializationStatus::Complete
    }

    pub(in crate::ai::combat_search_v2::search) fn root_action_schedule_states(
        &self,
    ) -> Vec<RootActionScheduleState> {
        self.root_evidence
            .entries
            .iter()
            .map(|(id, entry)| RootActionScheduleState {
                id: *id,
                expanded: entry.work.expanded_concrete_nodes,
                has_work: self.frontier.has_root_action_work(*id),
            })
            .collect()
    }

    pub(in crate::ai::combat_search_v2::search) fn best_exact_win_root_with_work(
        &self,
        states: &[RootActionScheduleState],
    ) -> Option<RootLineageId> {
        self.root_evidence.best_exact_win_root(states)
    }

    pub(in crate::ai::combat_search_v2::search) fn materialize_root_lineage(
        &mut self,
        node: &mut SearchNode,
    ) {
        self.root_evidence.materialize_node(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn observe_enumerated_root_surface(
        &mut self,
        node: &SearchNode,
        choices: &[OrderedNodeAction],
    ) {
        self.root_evidence
            .observe_enumerated_root_surface(node, choices);
    }

    pub(in crate::ai::combat_search_v2::search) fn mark_unmaterialized_root_surface_complete(
        &mut self,
        node: &SearchNode,
    ) {
        self.root_evidence
            .mark_unmaterialized_surface_complete(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn begin_unmaterialized_root_pending_work(
        &mut self,
        node: &SearchNode,
        work_items: usize,
    ) {
        self.root_evidence
            .begin_unmaterialized_pending_work(node, work_items);
    }

    pub(in crate::ai::combat_search_v2::search) fn finish_unmaterialized_root_pending_work(
        &mut self,
        node: &SearchNode,
    ) {
        self.root_evidence.finish_unmaterialized_pending_work(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn observe_exact_root_terminal(
        &mut self,
        node: &SearchNode,
    ) {
        match terminal_label(&node.engine, &node.combat) {
            SearchTerminalLabel::Win => self.root_evidence.observe_exact_win(node),
            SearchTerminalLabel::Escape | SearchTerminalLabel::Loss => {
                self.root_evidence.observe_exact_complete(node)
            }
            SearchTerminalLabel::Unresolved => {}
        }
    }
}

pub(super) fn root_evidence_snapshot(
    loop_state: &SearchLoopState,
) -> CombatSearchV2RootEvidenceSnapshot {
    frontier_evidence_scan(loop_state).root_evidence
}

pub(super) struct FrontierEvidenceScan {
    pub(super) root_evidence: CombatSearchV2RootEvidenceSnapshot,
    pub(super) work_item_count: usize,
    pub(super) pending_choice_work_items: usize,
    pub(super) sample_states: Vec<CombatSearchV2StateSummary>,
}

/// Report frontier work without reconstructing every exact combat-state key.
/// Exact duplicate pruning remains authoritative inside the search tables;
/// reporting only needs queue occupancy, root attribution, and a small sample.
pub(super) fn frontier_evidence_scan(loop_state: &SearchLoopState) -> FrontierEvidenceScan {
    let root_schedule_states = loop_state.root_action_schedule_states();
    let current_comparison_round_complete = loop_state
        .root_round_scheduler
        .current_comparison_complete(&root_schedule_states);
    let mut contenders = loop_state
        .root_evidence
        .entries
        .iter()
        .map(|(id, entry)| {
            (
                *id,
                CombatSearchV2RootActionEvidence {
                    rank: None,
                    root_action: entry.identity.clone(),
                    work: entry.work.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut unattributed = loop_state.root_evidence.unattributed.clone();
    let mut pending_choice_work_items = 0usize;
    for summary in loop_state.frontier.lineage_summaries() {
        let lineage = match summary.lineage {
            RootLineage::Action(id) if contenders.contains_key(&id) => RootLineage::Action(id),
            RootLineage::Action(_) | RootLineage::Unmaterialized => RootLineage::Unmaterialized,
        };
        pending_choice_work_items =
            pending_choice_work_items.saturating_add(summary.pending_choice_work_items);
        match lineage {
            RootLineage::Action(id) => {
                let contender = contenders.get_mut(&id).expect("checked root lineage");
                contender.work.open_work_items = contender
                    .work
                    .open_work_items
                    .saturating_add(summary.work_items);
                contender.work.open_pending_choice_work_items = contender
                    .work
                    .open_pending_choice_work_items
                    .saturating_add(summary.pending_choice_work_items);
                if let Some(entry) = summary.best_entry {
                    remember_best(
                        &mut contender.work.best_open_observed,
                        observed_value(&entry.node),
                    );
                }
            }
            RootLineage::Unmaterialized => {
                unattributed.open_work_items = unattributed
                    .open_work_items
                    .saturating_add(summary.work_items);
                unattributed.open_pending_choice_work_items = unattributed
                    .open_pending_choice_work_items
                    .saturating_add(summary.pending_choice_work_items);
                if let Some(entry) = summary.best_entry {
                    remember_best(
                        &mut unattributed.best_open_observed,
                        observed_value(&entry.node),
                    );
                }
            }
        }
    }
    let sample_states = loop_state
        .frontier
        .iter()
        .take(FRONTIER_SAMPLE_LIMIT)
        .map(|entry| summarize_state(&entry.node.engine, &entry.node.combat))
        .collect();
    let frontier_work_items = loop_state.frontier.len();

    let mut contenders = contenders.into_values().collect::<Vec<_>>();
    contenders.sort_by(|left, right| {
        leading_value(&right.work)
            .map(|value| &value.outcome_order_key)
            .cmp(&leading_value(&left.work).map(|value| &value.outcome_order_key))
            .then_with(|| {
                left.root_action
                    .action_key
                    .cmp(&right.root_action.action_key)
            })
    });
    let mut next_rank = 1usize;
    for contender in &mut contenders {
        if leading_value(&contender.work).is_some() {
            contender.rank = Some(next_rank);
            next_rank = next_rank.saturating_add(1);
        }
    }
    let leader = contenders
        .iter()
        .find(|contender| contender.rank == Some(1))
        .map(|contender| contender.root_action.clone());

    let mut closure_blockers = Vec::new();
    if loop_state.root_evidence.materialization != CombatSearchV2RootMaterializationStatus::Complete
    {
        closure_blockers
            .push(CombatSearchV2RootClosureBlocker::RootActionSurfaceNotFullyMaterialized);
    }
    if unattributed.open_work_items > 0
        || contenders
            .iter()
            .any(|contender| contender.work.open_work_items > 0)
    {
        closure_blockers.push(CombatSearchV2RootClosureBlocker::OpenFrontierWork);
    }
    if unattributed.open_pending_choice_work_items > 0
        || contenders
            .iter()
            .any(|contender| contender.work.open_pending_choice_work_items > 0)
    {
        closure_blockers.push(CombatSearchV2RootClosureBlocker::OpenPendingChoiceWork);
    }
    if loop_state.unresolved_leaf_count > 0 {
        closure_blockers.push(CombatSearchV2RootClosureBlocker::UnresolvedLeaf);
    }
    if loop_state.stats.action_surface_incomplete {
        closure_blockers
            .push(CombatSearchV2RootClosureBlocker::PendingChoiceOrderedVariantsOmitted);
    }
    if loop_state.max_actions_cut_count > 0 {
        closure_blockers.push(CombatSearchV2RootClosureBlocker::MaxActionsPerLine);
    }
    if loop_state.engine_step_limit_count > 0 {
        closure_blockers.push(CombatSearchV2RootClosureBlocker::EngineStepLimit);
    }
    if loop_state.potion_budget_cut_count > 0 {
        closure_blockers.push(CombatSearchV2RootClosureBlocker::PotionBudget);
    }
    if loop_state.performance.turn_boundary_macro_calls > 0 {
        closure_blockers
            .push(CombatSearchV2RootClosureBlocker::HierarchicalTurnBoundaryPortfolioSelection);
    }
    // Closure cannot become authoritative until exact-equivalence, dominance,
    // and terminal dispositions are attributed to their root lineage.  Keep
    // this evidence shadow-only instead of turning an observed empty queue
    // into a false proof.
    closure_blockers.push(CombatSearchV2RootClosureBlocker::ProofDispositionsUnavailable);

    let root_evidence = CombatSearchV2RootEvidenceSnapshot {
        ranking_policy: ROOT_RANKING_POLICY.to_string(),
        work_accounting_scope: ROOT_WORK_ACCOUNTING_SCOPE.to_string(),
        scheduling_policy: ROOT_SCHEDULING_POLICY.to_string(),
        scheduling_trigger: loop_state
            .root_round_scheduler
            .activation_reason()
            .to_string(),
        completed_comparison_rounds: loop_state
            .root_round_scheduler
            .completed_rounds()
            .saturating_add(u32::from(current_comparison_round_complete)),
        current_comparison_round: loop_state.root_round_scheduler.round_index(),
        current_scheduling_phase: loop_state.root_round_scheduler.phase_name().to_string(),
        current_comparison_round_complete,
        current_round_expansions_per_action: loop_state
            .root_round_scheduler
            .comparison_expansions_per_action(),
        materialization: loop_state.root_evidence.materialization,
        closure_status: CombatSearchV2RootClosureStatus::NotProven,
        closure_blockers,
        leader,
        contenders,
        unattributed,
    };
    FrontierEvidenceScan {
        root_evidence,
        work_item_count: frontier_work_items,
        pending_choice_work_items,
        sample_states,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::state::core::ClientInput;
    use crate::test_support::{blank_test_combat, test_monster};

    fn attributed_node(
        state: &mut SearchLoopState,
        combat: CombatState,
        root_key: &str,
    ) -> SearchNode {
        let mut node = SearchNode::root(EngineState::CombatPlayerTurn, combat);
        node.push_action(CombatSearchV2ActionTrace {
            step_index: 0,
            action_id: 0,
            action_key: root_key.to_string(),
            action_debug: root_key.to_string(),
            input: ClientInput::Proceed,
        });
        state.materialize_root_lineage(&mut node);
        node
    }

    #[test]
    fn frontier_evidence_accounts_for_work_by_root_without_an_exact_state_census() {
        let config = CombatSearchV2Config {
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            ..CombatSearchV2Config::default()
        };
        let mut state = SearchLoopState::new(
            &config,
            false,
            CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat()),
        );
        let mut state_zero = blank_test_combat();
        state_zero.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        let mut state_one = state_zero.clone();
        state_one.entities.player.block = 1;

        let root_a_zero = attributed_node(&mut state, state_zero.clone(), "root/a");
        let root_a_zero_duplicate = attributed_node(&mut state, state_zero.clone(), "root/a");
        let root_b_zero = attributed_node(&mut state, state_zero.clone(), "root/b");
        let root_a_one = attributed_node(&mut state, state_one, "root/a");
        state.push_frontier(root_a_zero);
        state.push_frontier(root_a_zero_duplicate);
        state.push_frontier(root_b_zero);
        state.push_frontier(root_a_one);
        state.push_frontier(SearchNode::root(EngineState::CombatPlayerTurn, state_zero));

        let scan = frontier_evidence_scan(&state);

        assert_eq!(scan.work_item_count, 5);
        assert_eq!(scan.sample_states.len(), 5);
        assert_eq!(scan.pending_choice_work_items, 0);
        let root_a = scan
            .root_evidence
            .contenders
            .iter()
            .find(|entry| entry.root_action.action_key == "root/a")
            .expect("root/a evidence");
        let root_b = scan
            .root_evidence
            .contenders
            .iter()
            .find(|entry| entry.root_action.action_key == "root/b")
            .expect("root/b evidence");
        assert_eq!(root_a.work.open_work_items, 3);
        assert_eq!(root_b.work.open_work_items, 1);
        assert_eq!(scan.root_evidence.unattributed.open_work_items, 1);
        assert_eq!(
            scan.root_evidence
                .contenders
                .iter()
                .map(|entry| entry.work.open_work_items)
                .sum::<usize>()
                .saturating_add(scan.root_evidence.unattributed.open_work_items),
            scan.work_item_count
        );
        assert_eq!(
            scan.root_evidence
                .contenders
                .iter()
                .map(|entry| entry.work.open_pending_choice_work_items)
                .sum::<usize>()
                .saturating_add(
                    scan.root_evidence
                        .unattributed
                        .open_pending_choice_work_items
                ),
            scan.pending_choice_work_items
        );
    }

    #[test]
    fn legacy_exact_frontier_evidence_remains_distinct_when_deserialized() {
        let mut value = serde_json::to_value(CombatSearchV2RootWorkEvidence::default())
            .expect("serialize root work evidence");
        let object = value.as_object_mut().expect("root work object");
        object.remove("open_work_items");
        object.insert("open_concrete_states".to_string(), serde_json::json!(7));

        let restored: CombatSearchV2RootWorkEvidence =
            serde_json::from_value(value).expect("legacy root evidence");
        let blocker: CombatSearchV2RootClosureBlocker =
            serde_json::from_str("\"open_concrete_work\"").expect("legacy blocker");

        assert_eq!(restored.open_work_items, 0);
        assert_eq!(restored.legacy_open_concrete_states, Some(7));
        assert_eq!(
            blocker,
            CombatSearchV2RootClosureBlocker::LegacyOpenConcreteWork
        );
    }
}

fn identity_from_trace(action: &CombatSearchV2ActionTrace) -> CombatSearchV2RootActionIdentity {
    CombatSearchV2RootActionIdentity {
        action_id: action.action_id,
        action_key: action.action_key.clone(),
        action_debug: action.action_debug.clone(),
    }
}

fn observed_value(node: &SearchNode) -> CombatSearchV2RootObservedValue {
    CombatSearchV2RootObservedValue {
        terminal: terminal_label(&node.engine, &node.combat),
        outcome_order_key: CombatOutcomeScore::from_node(node).to_report_key(),
        final_hp: node.combat.entities.player.current_hp,
        hp_loss: (node.initial_hp - node.combat.entities.player.current_hp).max(0),
        turns: node.combat.turn.turn_count,
        potions_used: node.potions_used,
        potions_discarded: node.potions_discarded,
        cards_played: node.cards_played,
        actions_taken: node.actions.len(),
    }
}

fn remember_best(
    best: &mut Option<CombatSearchV2RootObservedValue>,
    candidate: CombatSearchV2RootObservedValue,
) {
    if best
        .as_ref()
        .is_none_or(|current| candidate.outcome_order_key > current.outcome_order_key)
    {
        *best = Some(candidate);
    }
}

fn leading_value(
    work: &CombatSearchV2RootWorkEvidence,
) -> Option<&CombatSearchV2RootObservedValue> {
    [
        work.best_exact_complete.as_ref(),
        work.best_open_observed.as_ref(),
    ]
    .into_iter()
    .flatten()
    .max_by_key(|value| &value.outcome_order_key)
}
