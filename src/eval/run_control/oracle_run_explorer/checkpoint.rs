use super::*;

mod payloads;

pub use payloads::{OracleRunCheckpointPayloadsV1, OracleRunSessionPayloadRefsV1};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunBranchCheckpointV1 {
    pub branch_id: usize,
    pub parent_branch_id: Option<usize>,
    pub neow_root_candidate_id: String,
    pub neow_root_label: String,
    pub state_fingerprint: String,
    pub boundary: OracleRunBoundaryV1,
    pub path_negative_log_policy: f64,
    pub path_discrepancy: u64,
    pub path_depth: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_tip: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_tip: Option<usize>,
    pub session_payload_refs: OracleRunSessionPayloadRefsV1,
    pub session: RunControlSessionCheckpointV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunJournalNodeCheckpointV1 {
    pub parent: Option<usize>,
    pub entry: RunProgressStepV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunActiveCombatCheckpointV1 {
    pub branch_id: usize,
    #[serde(default)]
    pub stage: u8,
    pub work: OracleCombatWitnessCheckpointV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunDeferredCombatCheckpointV1 {
    pub branch_id: usize,
    pub stage: u8,
    pub prior_work: OracleCombatWitnessCheckpointV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunExplorerCheckpointV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_fingerprint_algorithm: Option<String>,
    pub next_branch_id: usize,
    pub branches: Vec<OracleRunBranchCheckpointV1>,
    pub pending_decisions: Vec<LazyOracleRunDecisionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_selection_families: Vec<LazyOracleRunSelectionFamilyV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_combat: Option<OracleRunActiveCombatCheckpointV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deferred_combats: Vec<OracleRunDeferredCombatCheckpointV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub journal_nodes: Vec<OracleRunJournalNodeCheckpointV1>,
    #[serde(
        default,
        skip_serializing_if = "OracleRunCheckpointPayloadsV1::is_empty"
    )]
    pub payloads: OracleRunCheckpointPayloadsV1,
    #[serde(default)]
    pub combat_search_restarts: usize,
    /// The last top-level Neow option that received strategic service.
    /// Persisting this cursor prevents a wide option from regaining the first
    /// slot every time a continuation is resumed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_served_neow_root: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_combats: Vec<OracleRunUnresolvedCombatV1>,
}

impl OracleRunExplorerV1 {
    pub fn frontier_checkpoint(&self) -> Result<Option<OracleRunExplorerCheckpointV1>, String> {
        if self.pending_combats.len() > 1 {
            return Err(format!(
                "oracle frontier cannot checkpoint {} simultaneous combat edges",
                self.pending_combats.len()
            ));
        }
        let active_combat =
            self.pending_combats
                .front()
                .map(|pending| OracleRunActiveCombatCheckpointV1 {
                    branch_id: pending.branch_id,
                    stage: pending.stage,
                    work: pending.work.checkpoint(),
                });
        let mut live_branch_ids = self
            .pending_decisions
            .iter()
            .map(|decision| decision.parent_branch_id)
            .collect::<BTreeSet<_>>();
        live_branch_ids.extend(
            self.pending_selection_families
                .iter()
                .map(|family| family.parent_branch_id),
        );
        if let Some(branch_id) = active_combat.as_ref().map(|active| active.branch_id) {
            live_branch_ids.insert(branch_id);
        }
        live_branch_ids.extend(self.deferred_combats.iter().map(|combat| combat.branch_id));
        if live_branch_ids.is_empty() {
            return Ok(None);
        }
        live_branch_ids.extend(
            self.unresolved_combats
                .iter()
                .map(|combat| combat.branch_id),
        );
        self.checkpoint_for_branches(live_branch_ids, active_combat)
            .map(Some)
    }

    pub fn analysis_checkpoint(&self) -> Result<OracleRunExplorerCheckpointV1, String> {
        if !self.pending_combats.is_empty() {
            return Err(
                "analysis checkpoint requires combat work to be owned by the analysis session"
                    .to_string(),
            );
        }
        let branch_ids = self
            .branches
            .iter()
            .map(|branch| branch.branch_id)
            .collect::<BTreeSet<_>>();
        self.checkpoint_for_branches(branch_ids, None)
    }

    fn checkpoint_for_branches(
        &self,
        branch_ids: BTreeSet<usize>,
        active_combat: Option<OracleRunActiveCombatCheckpointV1>,
    ) -> Result<OracleRunExplorerCheckpointV1, String> {
        let mut journal_nodes = Vec::<OracleRunJournalNodeCheckpointV1>::new();
        let mut journal_index = BTreeMap::<(Option<usize>, String), usize>::new();
        let branch_by_id = self
            .branches
            .iter()
            .map(|branch| (branch.branch_id, branch))
            .collect::<BTreeMap<_, _>>();
        let mut checkpointed_journals = BTreeMap::<usize, (Option<usize>, usize)>::new();
        let mut branches = Vec::with_capacity(branch_ids.len());
        let mut payloads = OracleRunCheckpointPayloadsV1::default();
        for branch_id in branch_ids {
            let branch = branch_by_id
                .get(&branch_id)
                .copied()
                .ok_or_else(|| format!("missing live oracle branch {branch_id}"))?;
            let entries = branch.journal.entries();
            let (mut journal_tip, inherited_entries) = branch
                .parent_branch_id
                .and_then(|parent_id| {
                    let (parent_tip, parent_len) =
                        checkpointed_journals.get(&parent_id).copied()?;
                    let parent = branch_by_id.get(&parent_id).copied()?;
                    let parent_entries = parent.journal.entries();
                    (parent_entries.len() == parent_len
                        && entries.len() >= parent_len
                        && entries[..parent_len] == *parent_entries)
                        .then_some((parent_tip, parent_len))
                })
                .unwrap_or((None, 0));
            for entry in entries.iter().skip(inherited_entries) {
                let hash = crate::eval::fingerprint::hash_serializable(entry);
                let key = (journal_tip, hash);
                let node_id = if let Some(node_id) = journal_index.get(&key).copied() {
                    if journal_nodes[node_id].entry != *entry {
                        return Err("oracle journal fingerprint collision".to_string());
                    }
                    node_id
                } else {
                    let node_id = journal_nodes.len();
                    journal_nodes.push(OracleRunJournalNodeCheckpointV1 {
                        parent: journal_tip,
                        entry: entry.clone(),
                    });
                    journal_index.insert(key, node_id);
                    node_id
                };
                journal_tip = Some(node_id);
            }
            checkpointed_journals.insert(branch_id, (journal_tip, entries.len()));
            let replay_tip = payloads.intern_replay(&branch.replay);
            let mut session = RunControlSessionCheckpointV1::from_session(&branch.session);
            session.clear_combat_diagnostics_for_external_checkpoint();
            let session_payload_refs = payloads.externalize_session(&mut session)?;
            branches.push(OracleRunBranchCheckpointV1 {
                branch_id: branch.branch_id,
                parent_branch_id: branch.parent_branch_id,
                neow_root_candidate_id: branch.neow_root_candidate_id.clone(),
                neow_root_label: branch.neow_root_label.clone(),
                state_fingerprint: branch.state_fingerprint.clone(),
                boundary: branch.boundary,
                path_negative_log_policy: branch.path_negative_log_policy,
                path_discrepancy: branch.path_discrepancy,
                path_depth: branch.path_depth,
                replay_tip,
                journal_tip,
                session_payload_refs,
                session,
            });
        }
        Ok(OracleRunExplorerCheckpointV1 {
            state_fingerprint_algorithm: Some(ORACLE_RUN_STATE_FINGERPRINT_ALGORITHM.to_string()),
            next_branch_id: self.next_branch_id,
            branches,
            pending_decisions: self.pending_decisions.iter().cloned().collect(),
            pending_selection_families: self.pending_selection_families.iter().cloned().collect(),
            active_combat,
            deferred_combats: self
                .deferred_combats
                .iter()
                .map(|combat| OracleRunDeferredCombatCheckpointV1 {
                    branch_id: combat.branch_id,
                    stage: combat.stage,
                    prior_work: combat.prior_work.clone(),
                })
                .collect(),
            journal_nodes,
            payloads,
            combat_search_restarts: self.combat_search_restarts,
            last_served_neow_root: self.last_served_neow_root.clone(),
            unresolved_combats: self.unresolved_combats.clone(),
        })
    }
}

impl OracleRunExplorerCheckpointV1 {
    pub fn hydrated_branch_session(
        &self,
        branch: &OracleRunBranchCheckpointV1,
    ) -> Result<RunControlSessionCheckpointV1, String> {
        let mut session = branch.session.clone();
        self.payloads
            .hydrate_session(&mut session, &branch.session_payload_refs)?;
        Ok(session)
    }
}

pub(super) fn restore_frontier_journal(
    mut tip: Option<usize>,
    nodes: &[OracleRunJournalNodeCheckpointV1],
) -> Result<RunProgressJournalV1, String> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    while let Some(node_id) = tip {
        if !seen.insert(node_id) {
            return Err("oracle frontier journal contains a cycle".to_string());
        }
        let node = nodes
            .get(node_id)
            .ok_or_else(|| format!("oracle frontier journal node {node_id} is missing"))?;
        entries.push(node.entry.clone());
        tip = node.parent;
    }
    entries.reverse();
    RunProgressJournalV1::from_committed_steps(entries)
}
