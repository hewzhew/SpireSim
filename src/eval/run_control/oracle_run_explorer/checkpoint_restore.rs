use super::*;

pub fn seed_oracle_run_explorer_from_checkpoint_v1(
    checkpoint: OracleRunExplorerCheckpointV1,
    combat_budgets: &OracleRunCombatWitnessBudgetsV1,
) -> Result<OracleRunExplorerV1, String> {
    let OracleRunExplorerCheckpointV1 {
        state_fingerprint_algorithm,
        next_branch_id,
        branches,
        pending_decisions,
        pending_selection_families,
        active_combat,
        deferred_combats,
        journal_nodes,
        payloads,
        combat_search_restarts,
        last_served_neow_root,
        unresolved_combats,
    } = checkpoint;
    if state_fingerprint_algorithm.as_deref() != Some(ORACLE_RUN_STATE_FINGERPRINT_ALGORITHM) {
        return Err(format!(
            "unsupported oracle run state fingerprint algorithm {:?}; regenerate the checkpoint after the stable gameplay identity cutover",
            state_fingerprint_algorithm.as_deref()
        ));
    }
    let mut explorer = OracleRunExplorerV1::empty();
    explorer.next_branch_id = next_branch_id;
    explorer.combat_search_restarts = combat_search_restarts;
    explorer.last_served_neow_root = last_served_neow_root;
    explorer.unresolved_combats = unresolved_combats;
    for saved in branches {
        let journal = checkpoint::restore_frontier_journal(saved.journal_tip, &journal_nodes)?;
        let replay = payloads.restore_replay(saved.replay_tip)?;
        let mut session_checkpoint = saved.session;
        payloads.hydrate_session(&mut session_checkpoint, &saved.session_payload_refs)?;
        let session = session_checkpoint.into_session()?;
        let actual_fingerprint = run_session_fingerprint_v3(&session);
        if actual_fingerprint != saved.state_fingerprint {
            return Err(format!(
                "oracle frontier branch {} fingerprint changed while restoring",
                saved.branch_id
            ));
        }
        let branch = OracleRunBranchV1 {
            branch_id: saved.branch_id,
            parent_branch_id: saved.parent_branch_id,
            neow_root_candidate_id: saved.neow_root_candidate_id,
            neow_root_label: saved.neow_root_label,
            state_fingerprint: actual_fingerprint,
            boundary: saved.boundary,
            path_negative_log_policy: saved.path_negative_log_policy,
            path_discrepancy: saved.path_discrepancy,
            path_depth: saved.path_depth,
            replay,
            journal,
            session,
        };
        if explorer.accept_branch(branch).is_none() {
            return Err("oracle frontier checkpoint contained duplicate states".to_string());
        }
    }
    explorer.next_branch_id = explorer.next_branch_id.max(
        explorer
            .branches
            .iter()
            .map(|branch| branch.branch_id.saturating_add(1))
            .max()
            .unwrap_or(0),
    );
    for mut decision in pending_decisions {
        let parent = explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == decision.parent_branch_id)
            .ok_or_else(|| {
                format!(
                    "oracle frontier decision references missing branch {}",
                    decision.parent_branch_id
                )
            })?;
        if parent.state_fingerprint != decision.parent_state_fingerprint {
            return Err(format!(
                "oracle frontier decision parent fingerprint changed for branch {}",
                decision.parent_branch_id
            ));
        }
        decision.parent_act = parent.session.run_state.act_num;
        decision.parent_floor = parent.session.run_state.floor_num;
        if explorer
            .registered_work_keys
            .insert(decision.stable_work_key.clone())
        {
            explorer.pending_decisions.push_back(decision);
        }
    }
    for family in pending_selection_families {
        let parent = explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == family.parent_branch_id)
            .ok_or_else(|| {
                format!(
                    "oracle frontier selection family references missing branch {}",
                    family.parent_branch_id
                )
            })?;
        if parent.state_fingerprint != family.parent_state_fingerprint {
            return Err(format!(
                "oracle frontier selection family parent fingerprint changed for branch {}",
                family.parent_branch_id
            ));
        }
        if family.cursor.is_exhausted() {
            return Err(format!(
                "oracle frontier selection family '{}' persisted after exhaustion",
                family.family_key
            ));
        }
        let Some(outstanding_work_key) = family.outstanding_work_key.as_deref() else {
            return Err(format!(
                "oracle frontier selection family '{}' has no outstanding exact member",
                family.family_key
            ));
        };
        if !explorer
            .pending_decisions
            .iter()
            .any(|decision| decision.stable_work_key == outstanding_work_key)
        {
            return Err(format!(
                "oracle frontier selection family '{}' lost outstanding member '{}'",
                family.family_key, outstanding_work_key
            ));
        }
        if !explorer
            .registered_work_keys
            .insert(family.family_key.clone())
        {
            return Err(format!(
                "oracle frontier duplicated selection family '{}'",
                family.family_key
            ));
        }
        explorer.pending_selection_families.push_back(family);
    }
    if let Some(active) = active_combat {
        let branch_id = active.branch_id;
        let branch = explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == branch_id)
            .ok_or_else(|| {
                format!("oracle frontier combat references missing branch {branch_id}")
            })?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle frontier active branch {branch_id} is not at a combat boundary"
            ));
        }
        let key = format!("combat:{}", branch.state_fingerprint);
        if !explorer.registered_work_keys.insert(key) {
            return Err(format!(
                "oracle frontier active combat branch {branch_id} duplicates registered work"
            ));
        }
        if explorer.last_served_neow_root.is_none() {
            explorer.last_served_neow_root = Some(branch.neow_root_candidate_id.clone());
        }
        let options =
            combat_budgets.for_session_stage_restore(&branch.session, active.stage, &active.work);
        let work = OracleResidentCombatWitnessJobV1::restore(
            &branch.session,
            options,
            active.work,
            combat_budgets.guidance_bundle.as_deref(),
        )?;
        explorer.pending_combats.push_back(PendingOracleCombatV1 {
            branch_id,
            stage: active.stage,
            work,
        });
        explorer.combat_search_restarts = explorer.combat_search_restarts.saturating_add(1);
    }
    for deferred in deferred_combats {
        let branch = explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == deferred.branch_id)
            .ok_or_else(|| {
                format!(
                    "oracle frontier deferred combat references missing branch {}",
                    deferred.branch_id
                )
            })?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle frontier deferred branch {} is not at a combat boundary",
                deferred.branch_id
            ));
        }
        let key = format!("combat:{}", branch.state_fingerprint);
        if !explorer.registered_work_keys.insert(key) {
            return Err(format!(
                "oracle frontier deferred combat branch {} duplicates registered work",
                deferred.branch_id
            ));
        }
        explorer.deferred_combats.push_back(DeferredOracleCombatV1 {
            branch_id: deferred.branch_id,
            stage: deferred.stage,
            prior_work: deferred.prior_work,
        });
    }
    Ok(explorer)
}
