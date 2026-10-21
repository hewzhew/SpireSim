use super::*;

pub(super) enum PreparedScheduledOracleRunWorkV1 {
    Decision {
        root: String,
        index: usize,
        work: LazyOracleRunDecisionV1,
    },
    DeferredCombat {
        root: String,
        index: usize,
        branch_id: usize,
        stage: u8,
    },
}

impl OracleRunExplorerV1 {
    #[cfg(test)]
    pub(super) fn take_best_decision(&mut self) -> Option<LazyOracleRunDecisionV1> {
        let index = self.best_decision_index()?;
        self.pending_decisions.remove(index)
    }

    #[cfg(test)]
    fn best_decision_index(&self) -> Option<usize> {
        self.pending_decisions
            .iter()
            .enumerate()
            .min_by(|(left_index, left), (right_index, right)| {
                oracle_run_decision_priority_order(*left_index, left, *right_index, right)
            })
            .map(|(index, _)| index)
    }

    fn next_neow_root_for_service(&self) -> Option<String> {
        let mut roots = BTreeSet::new();
        roots.extend(
            self.pending_decisions
                .iter()
                .map(|decision| decision.neow_root_candidate_id.clone()),
        );
        for deferred in &self.deferred_combats {
            let branch = self
                .branches
                .iter()
                .find(|branch| branch.branch_id == deferred.branch_id)
                .expect("deferred combat branch must remain live");
            roots.insert(branch.neow_root_candidate_id.clone());
        }
        let after_cursor = self.last_served_neow_root.as_ref().and_then(|last| {
            roots
                .iter()
                .find(|candidate| candidate.as_str() > last.as_str())
                .cloned()
        });
        after_cursor.or_else(|| roots.first().cloned())
    }

    fn best_decision_index_for_root(&self, root: &str) -> Option<usize> {
        self.pending_decisions
            .iter()
            .enumerate()
            .filter(|(_, decision)| decision.neow_root_candidate_id == root)
            .min_by(|(left_index, left), (right_index, right)| {
                oracle_run_decision_priority_order(*left_index, left, *right_index, right)
            })
            .map(|(index, _)| index)
    }

    fn best_deferred_combat_index_for_root(&self, root: &str) -> Option<usize> {
        self.deferred_combats
            .iter()
            .enumerate()
            .filter(|(_, deferred)| {
                self.branches
                    .iter()
                    .find(|branch| branch.branch_id == deferred.branch_id)
                    .is_some_and(|branch| branch.neow_root_candidate_id == root)
            })
            .min_by(|(left_index, left), (right_index, right)| {
                let left_branch = self
                    .branches
                    .iter()
                    .find(|branch| branch.branch_id == left.branch_id)
                    .expect("deferred combat branch must remain live");
                let right_branch = self
                    .branches
                    .iter()
                    .find(|branch| branch.branch_id == right.branch_id)
                    .expect("deferred combat branch must remain live");
                left_branch
                    .path_discrepancy
                    .cmp(&right_branch.path_discrepancy)
                    .then_with(|| {
                        right_branch
                            .session
                            .run_state
                            .act_num
                            .cmp(&left_branch.session.run_state.act_num)
                    })
                    .then_with(|| {
                        right_branch
                            .session
                            .run_state
                            .floor_num
                            .cmp(&left_branch.session.run_state.floor_num)
                    })
                    .then_with(|| right_branch.path_depth.cmp(&left_branch.path_depth))
                    .then_with(|| {
                        left_branch
                            .path_negative_log_policy
                            .total_cmp(&right_branch.path_negative_log_policy)
                    })
                    .then_with(|| left_index.cmp(right_index))
            })
            .map(|(index, _)| index)
    }

    pub(super) fn prepare_next_scheduled_work(&self) -> Option<PreparedScheduledOracleRunWorkV1> {
        let root = self.next_neow_root_for_service()?;
        let decision_index = self.best_decision_index_for_root(&root);
        let deferred_index = self.best_deferred_combat_index_for_root(&root);
        let take_deferred = match (decision_index, deferred_index) {
            (None, Some(_)) => true,
            (Some(_), None) => false,
            (Some(decision_index), Some(deferred_index)) => {
                let decision = &self.pending_decisions[decision_index];
                let deferred = &self.deferred_combats[deferred_index];
                let branch = self
                    .branches
                    .iter()
                    .find(|branch| branch.branch_id == deferred.branch_id)
                    .expect("deferred combat branch must remain live");
                match branch.path_discrepancy.cmp(&decision.path_discrepancy) {
                    std::cmp::Ordering::Less => true,
                    std::cmp::Ordering::Greater => false,
                    std::cmp::Ordering::Equal => {
                        (
                            branch.session.run_state.act_num,
                            branch.session.run_state.floor_num,
                            branch.path_depth,
                        )
                            .cmp(&(
                                decision.parent_act,
                                decision.parent_floor,
                                decision.path_depth,
                            ))
                            .then_with(|| {
                                decision
                                    .path_negative_log_policy
                                    .total_cmp(&branch.path_negative_log_policy)
                            })
                            == std::cmp::Ordering::Greater
                    }
                }
            }
            (None, None) => return None,
        };
        if take_deferred {
            let index = deferred_index.expect("deferred index selected");
            Some(PreparedScheduledOracleRunWorkV1::DeferredCombat {
                root,
                index,
                branch_id: self.deferred_combats[index].branch_id,
                stage: self.deferred_combats[index].stage,
            })
        } else {
            let index = decision_index.expect("decision index selected");
            Some(PreparedScheduledOracleRunWorkV1::Decision {
                root,
                index,
                work: self.pending_decisions[index].clone(),
            })
        }
    }

    pub(super) fn commit_scheduled_decision(
        &mut self,
        root: String,
        index: usize,
        expected_work_key: &str,
    ) {
        let removed = self
            .pending_decisions
            .remove(index)
            .expect("prepared decision work must remain queued until commit");
        assert_eq!(
            removed.stable_work_key, expected_work_key,
            "prepared decision work identity must remain stable until commit"
        );
        self.last_served_neow_root = Some(root);
    }

    pub(super) fn commit_scheduled_deferred_combat(
        &mut self,
        root: String,
        index: usize,
        expected_branch_id: usize,
        expected_stage: u8,
    ) {
        let removed = self
            .deferred_combats
            .remove(index)
            .expect("prepared deferred combat must remain queued until commit");
        assert_eq!(
            (removed.branch_id, removed.stage),
            (expected_branch_id, expected_stage),
            "prepared deferred combat identity must remain stable until commit"
        );
        self.last_served_neow_root = Some(root);
    }

    pub(super) fn refresh_combat_edge_probes(
        &mut self,
        edge_order: Option<OracleRunCombatEdgeOrderFnV1>,
    ) -> Result<(usize, usize), String> {
        let Some(edge_order) = edge_order else {
            return Ok((0, 0));
        };
        let mut evaluations = 0usize;
        let mut immediate = 0usize;
        for index in 0..self.pending_decisions.len() {
            if self.pending_decisions[index].combat_edge_probe.is_some() {
                continue;
            }
            let work = &self.pending_decisions[index];
            let branch = self
                .branches
                .iter()
                .find(|branch| branch.branch_id == work.parent_branch_id)
                .ok_or_else(|| {
                    format!(
                        "oracle decision edge probe references missing parent branch {}",
                        work.parent_branch_id
                    )
                })?;
            let order_key = edge_order(&branch.session, &work.candidate_id, &work.action);
            evaluations = evaluations.saturating_add(1);
            let probe = if let Some(order_key) = order_key {
                immediate = immediate.saturating_add(1);
                OracleRunCombatEdgeProbeV1::HeuristicEstimate { order_key }
            } else {
                OracleRunCombatEdgeProbeV1::NotImmediateCombat
            };
            self.pending_decisions[index].combat_edge_probe = Some(probe);
        }
        Ok((evaluations, immediate))
    }
}

fn combat_edge_probe_order(
    left: &LazyOracleRunDecisionV1,
    right: &LazyOracleRunDecisionV1,
) -> std::cmp::Ordering {
    match (left.combat_edge_probe, right.combat_edge_probe) {
        (
            Some(OracleRunCombatEdgeProbeV1::HeuristicEstimate {
                order_key: left_key,
            }),
            Some(OracleRunCombatEdgeProbeV1::HeuristicEstimate {
                order_key: right_key,
            }),
        ) => right_key.cmp(&left_key),
        _ => std::cmp::Ordering::Equal,
    }
}

fn oracle_run_decision_priority_order(
    left_index: usize,
    left: &LazyOracleRunDecisionV1,
    right_index: usize,
    right: &LazyOracleRunDecisionV1,
) -> std::cmp::Ordering {
    combat_edge_probe_order(left, right)
        .then_with(|| left.path_discrepancy.cmp(&right.path_discrepancy))
        .then_with(|| right.parent_act.cmp(&left.parent_act))
        .then_with(|| right.parent_floor.cmp(&left.parent_floor))
        .then_with(|| right.path_depth.cmp(&left.path_depth))
        .then_with(|| {
            left.path_negative_log_policy
                .total_cmp(&right.path_negative_log_policy)
        })
        .then_with(|| left_index.cmp(&right_index))
}
