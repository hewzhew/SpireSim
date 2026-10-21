use super::*;
use std::collections::BTreeSet;

impl LocalTurnGraphWitnessSession {
    pub(super) fn diagnostic_actions_to_node(&self, mut node_id: usize) -> Vec<TurnOptionAction> {
        let mut path = Vec::new();
        while let Some(parent) = self.nodes[node_id].diagnostic_parent {
            path.push(parent);
            node_id = parent.0;
        }
        path.reverse();
        self.path_actions(&path).0
    }

    pub fn counters(&self) -> LocalTurnGraphWitnessCounters {
        self.used.clone()
    }

    /// Whether this session was configured to seed fresh graph nodes from the
    /// bounded public-plan complete-turn proposer before lazy generation.
    ///
    /// This is observation only. In particular, zero proposal attempts does
    /// not distinguish a disabled seed from an enabled seed that has not yet
    /// received work.
    pub fn public_plan_turn_proposal_enabled(&self) -> bool {
        self.public_plan_turn_proposal.is_some()
    }

    pub fn retained_state_work(&self) -> usize {
        self.nodes
            .iter()
            .map(|node| node.generator.retained_work_items())
            .sum::<usize>()
            .saturating_add(self.nodes.iter().filter(|node| !node.exhausted).count())
    }

    pub fn progress_snapshot(&self) -> OracleCombatWitnessProgressSnapshot {
        let root = &self.nodes[0];
        let root_counters = root.generator.counters();
        let mut survival_by_turn =
            BTreeMap::<u32, (OracleCombatDeepStateSnapshot, Vec<TurnOptionAction>)>::new();
        let mut deepest_survival = None::<(OracleCombatDeepStateSnapshot, Vec<TurnOptionAction>)>;
        let mut deepest_progress = None::<(OracleCombatDeepStateSnapshot, Vec<TurnOptionAction>)>;
        let mut max_path_atomic_depth = 0usize;
        for node_id in 0..self.nodes.len() {
            let actions = self.diagnostic_actions_to_node(node_id);
            max_path_atomic_depth = max_path_atomic_depth.max(actions.len());
            let state = local_deep_state_snapshot(&self.nodes[node_id], actions.len());
            let replace_turn =
                survival_by_turn
                    .get(&state.player_turn)
                    .is_none_or(|(current, _)| {
                        (state.player_hp, -state.enemy_total_hp, state.player_block)
                            > (
                                current.player_hp,
                                -current.enemy_total_hp,
                                current.player_block,
                            )
                    });
            if replace_turn {
                survival_by_turn.insert(state.player_turn, (state.clone(), actions.clone()));
            }
            let replace_survival = deepest_survival.as_ref().is_none_or(|(current, _)| {
                (
                    state.player_turn,
                    state.player_hp,
                    -state.enemy_total_hp,
                    state.player_block,
                ) > (
                    current.player_turn,
                    current.player_hp,
                    -current.enemy_total_hp,
                    current.player_block,
                )
            });
            if replace_survival {
                deepest_survival = Some((state.clone(), actions.clone()));
            }
            let replace_progress = deepest_progress.as_ref().is_none_or(|(current, _)| {
                (
                    state.player_turn,
                    -state.enemy_total_hp,
                    state.player_hp,
                    state.player_block,
                ) > (
                    current.player_turn,
                    -current.enemy_total_hp,
                    current.player_hp,
                    current.player_block,
                )
            });
            if replace_progress {
                deepest_progress = Some((state, actions));
            }
        }
        let recent_turn_survival_envelope = survival_by_turn
            .into_values()
            .rev()
            .take(32)
            .map(|(state, _)| state)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        OracleCombatWitnessProgressSnapshot {
            retained_states: self.nodes.iter().filter(|node| !node.exhausted).count(),
            queued_anchor_entries: self.nodes.iter().filter(|node| !node.exhausted).count(),
            queued_guided_entries: Vec::new(),
            guide_queues: Vec::new(),
            generation_gap_count: self.generation_gaps.len(),
            pending_witness_replay: false,
            root_state: Some(OracleCombatWitnessStateProgressSnapshot {
                exact_state_hash: exact_hash(root.generator.root().position()),
                path_atomic_depth: 0,
                path_negative_log_policy: 0.0,
                generator_work: root_counters.generation_work,
                generator_engine_steps: root_counters.engine_steps,
                completed_turn_options: root.generator.total_completed_options(),
                retained_generator_work_items: root.generator.retained_work_items(),
                synced_options: root.generated_options,
                anchor_states_ahead: None,
                guided_states_ahead: None,
                guided_lane_ranks: None,
            }),
            max_player_turn: self
                .nodes
                .iter()
                .map(|node| node.generator.root().position().combat.turn.turn_count)
                .max()
                .unwrap_or_default(),
            deepest_survival_state: deepest_survival.as_ref().map(|(state, _)| state.clone()),
            deepest_progress_state: deepest_progress.as_ref().map(|(state, _)| state.clone()),
            deepest_survival_actions: deepest_survival
                .map(|(_, actions)| actions)
                .unwrap_or_default(),
            deepest_progress_actions: deepest_progress
                .map(|(_, actions)| actions)
                .unwrap_or_default(),
            recent_turn_survival_envelope,
            max_path_atomic_depth,
            max_completed_turn_options_at_state: self
                .nodes
                .iter()
                .map(|node| node.generator.total_completed_options())
                .max()
                .unwrap_or_default(),
        }
    }

    fn node_id_by_exact_hash(&self, exact_state_hash: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|node| node.generator.root().exact_state_hash() == exact_state_hash)
    }

    pub fn state_snapshot_by_exact_hash(
        &self,
        exact_state_hash: &str,
    ) -> Option<LocalTurnGraphStateSnapshot> {
        let node_id = self.node_id_by_exact_hash(exact_state_hash)?;
        let node = &self.nodes[node_id];
        let counters = node.generator.counters();
        let retained_guide_promises = node
            .generation_service_views
            .iter()
            .filter_map(|view| {
                let LocalServiceView::Guide(lane) = view else {
                    return None;
                };
                node.generator
                    .best_retained_guide_promise_snapshot(*lane)
                    .map(|promise| LocalTurnGraphRetainedGuidePromiseSnapshot {
                        lane: lane.value(),
                        rank: promise.rank.components().to_vec(),
                        atomic_depth: promise.atomic_depth,
                    })
            })
            .collect();
        Some(LocalTurnGraphStateSnapshot {
            exact_state_hash: exact_state_hash.to_owned(),
            relative_turn_depth: node.relative_turn_depth,
            visits: node.visits,
            first_service_selection: node.first_service_selection,
            first_guide_service_selection: node.first_guide_service_selection,
            generation_work: counters.generation_work,
            generator_engine_steps: counters.engine_steps,
            retained_generator_work_items: node.generator.retained_work_items(),
            generator_anchor_work_pops: node.generator.anchor_work_pops(),
            generator_guided_work_pops: node.generator.guided_work_pops(),
            best_retained_anchor_atomic_depth: node
                .generator
                .best_retained_path_bound_snapshot()
                .map(|(atomic_depth, _)| atomic_depth),
            retained_guide_promises,
            generated_options: node.generated_options,
            children: node.children.len(),
            exhausted: node.exhausted,
        })
    }

    pub fn root_action_families(&self) -> Vec<LocalTurnGraphRootActionFamilySnapshot> {
        let mut snapshots = self
            .root_action_families
            .iter()
            .map(|family| self.root_action_family_snapshot(family))
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.best_root_negative_log_policy
                .unwrap_or(f64::INFINITY)
                .total_cmp(&right.best_root_negative_log_policy.unwrap_or(f64::INFINITY))
        });
        snapshots
    }

    pub fn edge_snapshot_by_exact_hashes(
        &self,
        parent_exact_state_hash: &str,
        successor_exact_state_hash: &str,
    ) -> Option<LocalTurnGraphEdgeSnapshot> {
        let parent_id = self.node_id_by_exact_hash(parent_exact_state_hash)?;
        let successor_id = self.node_id_by_exact_hash(successor_exact_state_hash)?;
        let parent = &self.nodes[parent_id];
        let edge = parent
            .children
            .iter()
            .find(|edge| edge.successor == successor_id)?;
        let successor = &self.nodes[successor_id];
        let successor_anchor_position = self
            .shared_agenda
            .anchor_position(successor_id, &self.nodes);
        let guide_service = successor
            .guides
            .iter()
            .map(|guide| {
                let mut candidates = parent
                    .children
                    .iter()
                    .filter(|candidate| !self.nodes[candidate.successor].exhausted)
                    .filter_map(|candidate| {
                        backed_guide_rank(candidate, &self.nodes[candidate.successor], guide.lane)
                            .map(|rank| (candidate, rank))
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by(|(left_edge, left_rank), (right_edge, right_rank)| {
                    guide_choice_order(
                        left_rank,
                        local_path_base(left_edge.actions.len(), left_edge.negative_log_policy),
                        left_edge.visits,
                        left_edge.successor,
                        right_rank,
                        local_path_base(right_edge.actions.len(), right_edge.negative_log_policy),
                        right_edge.visits,
                        right_edge.successor,
                    )
                });
                let ordinal_rank = candidates
                    .iter()
                    .position(|(candidate, _)| candidate.successor == successor_id)
                    .map(|index| index.saturating_add(1))
                    .unwrap_or(0);
                let (global_position, global_best_rank) =
                    self.shared_agenda
                        .guide_position(successor_id, guide.lane, &self.nodes);
                LocalTurnGraphGuideServiceSnapshot {
                    lane: guide.lane.value(),
                    edge_visits: edge.guide_visits.get(&guide.lane).copied().unwrap_or(0),
                    sibling_ordinal_rank: ordinal_rank,
                    sibling_candidate_count: candidates.len(),
                    successor_rank: backed_guide_rank(edge, successor, guide.lane)
                        .unwrap_or(&guide.rank)
                        .components()
                        .to_vec(),
                    sibling_best_rank: candidates
                        .first()
                        .map(|(_, rank)| rank.components().to_vec())
                        .unwrap_or_default(),
                    global_ordinal_rank: global_position.ordinal_rank,
                    global_candidate_count: global_position.candidate_count,
                    global_best_rank: global_best_rank
                        .map(|rank| rank.components().to_vec())
                        .unwrap_or_default(),
                }
            })
            .collect();
        Some(LocalTurnGraphEdgeSnapshot {
            parent_visits: parent.visits,
            parent_generated_options: parent.generated_options,
            parent_children: parent.children.len(),
            parent_widen_anchor_visits: parent.widen_anchor_visits,
            actions: edge.actions.clone(),
            negative_log_policy: edge.negative_log_policy,
            plan_prefix_proposed: edge.plan_prefix_proposed,
            plan_transition_annotation: edge.plan_transition_annotation.clone(),
            visits: edge.visits,
            anchor_visits: edge.anchor_visits,
            backed_visits: edge.backed_visits,
            successor_path_cost: successor.path_cost(),
            successor_anchor_ordinal_rank: successor_anchor_position.agenda.ordinal_rank,
            successor_anchor_candidate_count: successor_anchor_position.agenda.candidate_count,
            successor_anchor_service_cost: successor_anchor_position.service_cost,
            best_anchor_service_cost: successor_anchor_position.best_service_cost,
            guide_service,
            successor_visits: successor.visits,
            successor_generated_options: successor.generated_options,
            successor_children: successor.children.len(),
            successor_exhausted: successor.exhausted,
        })
    }

    pub fn plan_transition_edge_snapshots(&self) -> Vec<LocalTurnGraphPlanTransitionEdgeSnapshot> {
        let exact_hashes = self
            .nodes
            .iter()
            .map(|node| node.generator.root().exact_state_hash())
            .collect::<Vec<_>>();
        let mut snapshots = self
            .nodes
            .iter()
            .enumerate()
            .flat_map(|(parent_id, parent)| {
                let exact_hashes = &exact_hashes;
                parent.children.iter().filter_map(move |edge| {
                    let plan_transition_annotation =
                        edge.plan_transition_annotation.as_ref()?.clone();
                    Some(LocalTurnGraphPlanTransitionEdgeSnapshot {
                        parent_exact_state_hash: exact_hashes[parent_id].to_owned(),
                        successor_exact_state_hash: exact_hashes[edge.successor].to_owned(),
                        parent_relative_turn_depth: parent.relative_turn_depth,
                        action_count: edge.actions.len(),
                        negative_log_policy: edge.negative_log_policy,
                        plan_transition_annotation,
                        edge_visits: edge.visits,
                        anchor_visits: edge.anchor_visits,
                        guide_visits: edge.guide_visits.values().copied().sum(),
                        backed_visits: edge.backed_visits,
                        successor_visits: self.nodes[edge.successor].visits,
                    })
                })
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.parent_relative_turn_depth
                .cmp(&right.parent_relative_turn_depth)
                .then_with(|| {
                    left.parent_exact_state_hash
                        .cmp(&right.parent_exact_state_hash)
                })
                .then_with(|| {
                    left.successor_exact_state_hash
                        .cmp(&right.successor_exact_state_hash)
                })
        });
        snapshots
    }

    pub fn plan_transition_path_snapshots(&self) -> Vec<LocalTurnGraphPlanTransitionPathSnapshot> {
        let mut snapshots = self
            .nodes
            .iter()
            .enumerate()
            .flat_map(|(parent_id, parent)| {
                parent.children.iter().filter_map(move |edge| {
                    let plan_transition_annotation =
                        edge.plan_transition_annotation.as_ref()?.clone();
                    let mut root_actions = self.diagnostic_actions_to_node(parent_id);
                    let parent_action_count = root_actions.len();
                    root_actions.extend(edge.actions.iter().cloned());
                    Some(LocalTurnGraphPlanTransitionPathSnapshot {
                        successor_exact_state_hash: self.nodes[edge.successor]
                            .generator
                            .root()
                            .exact_state_hash()
                            .to_owned(),
                        root_actions,
                        parent_action_count,
                        path_negative_log_policy: parent.path_negative_log_policy
                            + edge.negative_log_policy,
                        plan_transition_annotation,
                    })
                })
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.root_actions
                .len()
                .cmp(&right.root_actions.len())
                .then_with(|| {
                    left.path_negative_log_policy
                        .total_cmp(&right.path_negative_log_policy)
                })
                .then_with(|| {
                    left.successor_exact_state_hash
                        .cmp(&right.successor_exact_state_hash)
                })
        });
        snapshots
    }

    pub fn milestone_closure_path_snapshots(
        &self,
    ) -> Vec<LocalTurnGraphMilestoneClosurePathSnapshot> {
        let mut snapshots = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(node_id, node)| {
                let position = node.generator.root().position();
                let target_milestone =
                    sts_combat_strategy::combat_plan_turn_closure_target_v1(position)?;
                let plan = sts_combat_strategy::combat_plan_projection_v1(position)?;
                Some(LocalTurnGraphMilestoneClosurePathSnapshot {
                    exact_state_hash: node.generator.root().exact_state_hash().to_owned(),
                    root_actions: self.diagnostic_actions_to_node(node_id),
                    path_negative_log_policy: node.path_negative_log_policy,
                    target_milestone,
                    plan,
                    visits: node.visits,
                    generation_work: node.generator.counters().generation_work,
                })
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.root_actions
                .len()
                .cmp(&right.root_actions.len())
                .then_with(|| {
                    left.path_negative_log_policy
                        .total_cmp(&right.path_negative_log_policy)
                })
                .then_with(|| left.exact_state_hash.cmp(&right.exact_state_hash))
        });
        snapshots
    }

    fn root_action_family_snapshot(
        &self,
        family: &LocalRootActionFamilyAccumulator,
    ) -> LocalTurnGraphRootActionFamilySnapshot {
        let root_successors = self.nodes[0]
            .children
            .iter()
            .filter(|edge| {
                edge.actions
                    .first()
                    .is_some_and(|action| action.input == family.first_action)
            })
            .map(|edge| edge.successor)
            .collect::<BTreeSet<_>>();
        let retained_next_turn_successors = root_successors
            .iter()
            .filter(|node_id| !self.nodes[**node_id].exhausted)
            .count();
        let mut pending = root_successors.iter().copied().collect::<VecDeque<_>>();
        let mut reachable = BTreeSet::new();
        while let Some(node_id) = pending.pop_front() {
            if !reachable.insert(node_id) {
                continue;
            }
            pending.extend(
                self.nodes[node_id]
                    .children
                    .iter()
                    .map(|edge| edge.successor),
            );
        }

        let mut max_player_turn = 0;
        let mut best_hp_at_max_turn = None;
        let mut lowest_enemy_hp_at_max_turn = None;
        let mut reachable_generation_work = 0usize;
        let mut reachable_completed_turn_options = 0usize;
        let mut reachable_retained_states = 0usize;
        for node_id in &reachable {
            let node = &self.nodes[*node_id];
            let position = node.generator.root().position();
            let turn = position.combat.turn.turn_count;
            let hp = position.combat.entities.player.current_hp;
            let enemy_hp = position
                .combat
                .entities
                .monsters
                .iter()
                .filter(|monster| monster.is_alive_for_action())
                .map(|monster| monster.current_hp.max(0))
                .sum::<i32>();
            if turn > max_player_turn {
                max_player_turn = turn;
                best_hp_at_max_turn = Some(hp);
                lowest_enemy_hp_at_max_turn = Some(enemy_hp);
            } else if turn == max_player_turn {
                best_hp_at_max_turn =
                    Some(best_hp_at_max_turn.map_or(hp, |current| current.max(hp)));
                lowest_enemy_hp_at_max_turn = Some(
                    lowest_enemy_hp_at_max_turn.map_or(enemy_hp, |current| current.min(enemy_hp)),
                );
            }
            let counters = node.generator.counters();
            reachable_generation_work =
                reachable_generation_work.saturating_add(counters.generation_work);
            reachable_completed_turn_options = reachable_completed_turn_options
                .saturating_add(node.generator.total_completed_options());
            if !node.exhausted {
                reachable_retained_states = reachable_retained_states.saturating_add(1);
            }
        }

        LocalTurnGraphRootActionFamilySnapshot {
            first_action: family.first_action.clone(),
            best_root_negative_log_policy: family.best_root_negative_log_policy,
            completed_root_turn_options: family.completed_root_turn_options,
            terminal_wins: family.terminal_wins,
            terminal_losses: family.terminal_losses,
            escapes: family.escapes,
            unique_next_turn_successors: root_successors.len(),
            retained_next_turn_successors,
            reachable_exact_states: reachable.len(),
            reachable_retained_states,
            reachable_generation_work,
            reachable_completed_turn_options,
            max_player_turn,
            best_hp_at_max_turn,
            lowest_enemy_hp_at_max_turn,
        }
    }

    pub(super) fn snapshot(
        &self,
        status: LocalTurnGraphWitnessStatus,
    ) -> LocalTurnGraphWitnessReport {
        LocalTurnGraphWitnessReport {
            status,
            counters: self.used.clone(),
            performance_timing: self.performance_timing,
            root_visits: self.nodes[0].visits,
            root_generated_options: self.nodes[0].generated_options,
            root_children: self.nodes[0].children.len(),
            generation_gaps: self.generation_gaps.clone(),
            witness: self.witness.clone(),
            witness_frontier: self
                .witness_frontier
                .iter()
                .map(|witness| {
                    terminal_outcome_snapshot(
                        &self.original_root,
                        witness,
                        self.witness
                            .as_ref()
                            .is_some_and(|selected| selected.actions == witness.actions),
                    )
                })
                .collect(),
        }
    }
}
