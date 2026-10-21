//! Source-qualified continuation of an explicitly selected complete battle outcome.
use crate::{
    manual_draft::require_same,
    manual_replay::{adopted_scopes, replay_manual_scope_observed},
    ManualCombatDraftV1, ManualSessionV1,
};
use serde::{Deserialize, Serialize};
use sts_parity_lockstep::{LockstepAdapterV1, LockstepRunnerV1, SemanticActionV1};
use sts_parity_lockstep_java::RecordedJavaEvidenceV1;
use sts_parity_lockstep_rust::RustLockstepAdapterV1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualDraftSourceV1 {
    pub session_id: String,
    /// Semantic action coordinate local to this session's evidence, not the
    /// number of raw Java settlement actions or an ancestry-wide offset.
    pub committed_actions: usize,
}

impl ManualDraftSourceV1 {
    pub(crate) fn selected_scope_index(
        &self,
        scopes: &[(ManualSessionV1, Option<usize>)],
    ) -> Result<usize, String> {
        let index = scopes
            .iter()
            .position(|(owner, _)| owner.session_id() == self.session_id)
            .ok_or("draft origin is outside the selected trajectory")?;
        if scopes[index]
            .1
            .is_some_and(|limit| self.committed_actions > limit)
        {
            return Err("draft origin is beyond the selected source prefix".into());
        }
        Ok(index)
    }

    pub(crate) fn committed_suffix(
        &self,
        session: &ManualSessionV1,
    ) -> Result<Vec<SemanticActionV1>, String> {
        let scopes = adopted_scopes(session.repository(), session, None)?;
        let index = self.selected_scope_index(&scopes)?;
        let mut actions = Vec::new();
        for (offset, (owner, limit)) in scopes[index..].iter().enumerate() {
            let evidence = match limit {
                Some(count) => {
                    RecordedJavaEvidenceV1::load_prefix(&owner.evidence_directory(), *count)?
                }
                None => RecordedJavaEvidenceV1::load_complete(&owner.evidence_directory())?,
            };
            let start = if offset == 0 {
                self.committed_actions
            } else {
                0
            };
            if start > evidence.steps.len() {
                return Err("draft origin is beyond the recorded source".into());
            }
            actions.extend(
                evidence
                    .steps
                    .into_iter()
                    .skip(start)
                    .map(|step| step.action),
            );
        }
        Ok(actions)
    }
}

/// Constructible only after current-code source and complete-outcome verification.
/// Contains actual snapshots from adapters, never journal digest receipts.
pub struct PreparedManualDraftAdoptionV1 {
    remaining: ManualCombatDraftV1,
    completed: usize,
    total: usize,
}

impl PreparedManualDraftAdoptionV1 {
    pub fn completed_actions(&self) -> usize {
        self.completed
    }
    pub fn remaining_actions(&self) -> usize {
        self.remaining.actions.len()
    }
    pub fn total_actions(&self) -> usize {
        self.total
    }

    pub fn commit<J: LockstepAdapterV1>(
        &self,
        runner: &mut LockstepRunnerV1<J, RustLockstepAdapterV1>,
        on_exact: impl FnMut(usize, &LockstepRunnerV1<J, RustLockstepAdapterV1>) -> Result<(), String>,
    ) -> Result<usize, String> {
        if runner.is_stopped()
            || runner.current_comparison().status != sts_parity_lockstep::ComparisonStatusV1::Exact
        {
            return Err("draft adoption requires an Exact running boundary".into());
        }
        require_same(
            "prepared adoption root",
            &self.remaining.root,
            runner.current_snapshots().1,
        )?;
        if self.remaining.actions.is_empty() {
            require_same(
                "completed draft terminal",
                &self.remaining.terminal,
                runner.current_snapshots().1,
            )?;
            return Ok(0);
        }
        crate::commit_manual_combat_draft_v1(runner, &self.remaining, on_exact)
    }
}

pub fn prepare_manual_draft_adoption_v1(
    session: &ManualSessionV1,
    draft: &ManualCombatDraftV1,
    current: &RustLockstepAdapterV1,
    observer: &mut dyn FnMut(crate::LockstepProgressV1),
) -> Result<PreparedManualDraftAdoptionV1, String> {
    let _timing = sts_parity_lockstep::timing_span_v1("draft.verify");
    let Some(source) = &draft.source else {
        draft.validate_for_v1(current)?;
        return Ok(PreparedManualDraftAdoptionV1 {
            remaining: draft.clone(),
            completed: 0,
            total: draft.actions.len(),
        });
    };
    if source.session_id != draft.session_id {
        return Err("draft origin disagrees with its owning session".into());
    }
    let scopes = adopted_scopes(session.repository(), session, None)?;
    if source.selected_scope_index(&scopes).is_err() {
        // An explicitly selected private proposal can be re-proven at the
        // identical input after native recovery created a different journal.
        // Its owner may still be an ancestor while the proposal's coordinate
        // lies beyond the retained prefix; that is not suffix continuation.
        // This does not import the old trajectory or authorize suffix reuse.
        require_same(
            "draft root",
            &draft.root,
            &current.clone().observe().map_err(|e| e.message)?,
        )?;
        let ((_, mut replayed, _), _) =
            replay_manual_scope_observed(session.repository(), session, None, None, observer)?;
        require_same(
            "adoption current boundary",
            &replayed.observe().map_err(|e| e.message)?,
            &current.clone().observe().map_err(|e| e.message)?,
        )?;
        let origin_session =
            ManualSessionV1::load_existing(session.repository(), &source.session_id)?;
        let ((_, origin, _), _) = replay_manual_scope_observed(
            session.repository(),
            &origin_session,
            Some(source.committed_actions),
            None,
            observer,
        )?;
        return prepare_verified_suffix(draft, &origin, &[], current);
    }
    let committed = source.committed_suffix(session)?;
    if !draft.actions.starts_with(&committed) {
        return Err(
            "selected trajectory diverges from the chosen draft; no actions submitted".into(),
        );
    }
    let ((_, mut replayed, _), anchor) = replay_manual_scope_observed(
        session.repository(),
        session,
        None,
        Some((&source.session_id, source.committed_actions)),
        observer,
    )?;
    let (root, _) = *anchor.ok_or("draft origin was not observed during replay")?;
    require_same(
        "adoption current boundary",
        &replayed.observe().map_err(|e| e.message)?,
        &current.clone().observe().map_err(|e| e.message)?,
    )?;
    prepare_verified_suffix(draft, &root, &committed, current)
}

// Kept below the source replay gate; tests exercise interruption and changed
// current semantics with actual Rust adapters and an instrumented oracle.
pub(crate) fn prepare_verified_suffix(
    draft: &ManualCombatDraftV1,
    origin: &RustLockstepAdapterV1,
    committed: &[SemanticActionV1],
    current: &RustLockstepAdapterV1,
) -> Result<PreparedManualDraftAdoptionV1, String> {
    if !draft.actions.starts_with(committed) {
        return Err("committed actions differ from the selected draft prefix".into());
    }
    draft.validate_for_v1(origin)?;
    let mut expected = origin.clone();
    let mut snapshot = expected.observe().map_err(|e| e.message)?;
    for action in committed {
        let prepared = expected.prepare(action, &snapshot).map_err(|e| e.message)?;
        snapshot = expected.execute(prepared).map_err(|e| e.message)?;
    }
    let current_snapshot = current.clone().observe().map_err(|e| e.message)?;
    require_same("committed draft prefix", &snapshot, &current_snapshot)?;
    let remaining = ManualCombatDraftV1 {
        root: current_snapshot,
        actions: draft.actions[committed.len()..].to_vec(),
        source: None,
        ..draft.clone()
    };
    if remaining.actions.is_empty() {
        require_same("completed draft", &remaining.terminal, &remaining.root)?;
    } else {
        remaining.validate_for_v1(current)?;
    }
    Ok(PreparedManualDraftAdoptionV1 {
        remaining,
        completed: committed.len(),
        total: draft.actions.len(),
    })
}
