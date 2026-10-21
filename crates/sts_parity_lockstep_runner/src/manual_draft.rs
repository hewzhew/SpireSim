//! Adopt an analyst-selected complete Rust draft through the shared lockstep
//! runner. No search or action-handle interpretation belongs to this executor.
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use sts_parity_lockstep::{
    compare_snapshots_v1, ComparisonStatusV1, LockstepAdapterV1, LockstepRunnerV1,
    ParitySnapshotV1, SemanticActionV1,
};
use sts_parity_lockstep_rust::RustLockstepAdapterV1;

use crate::{manual_replay::adopted_scopes, ManualSessionV1};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualCombatDraftV1 {
    pub schema: String,
    pub session_id: String,
    pub rust_version: String,
    /// New drafts retain their exact raw-trajectory anchor. Older proposals
    /// remain usable at their original root, but cannot authorize suffix reuse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::ManualDraftSourceV1>,
    pub root: ParitySnapshotV1,
    pub actions: Vec<SemanticActionV1>,
    pub terminal: ParitySnapshotV1,
}

impl ManualCombatDraftV1 {
    /// Rebuild the whole battle outcome with current semantics before the first Java
    /// action. Saved projections are assertions, never executable checkpoints.
    pub fn validate_for_v1(&self, rust: &RustLockstepAdapterV1) -> Result<(), String> {
        if self.schema != "sts-manual-combat-draft-v1" || self.actions.is_empty() {
            return Err("adoption requires a nonempty complete combat draft".to_owned());
        }
        let current = rust.clone().observe().map_err(|e| e.message)?;
        require_same("draft root", &self.root, &current)?;
        let terminal = rust.verify_semantic_combat_completion_v1(&self.actions)?;
        require_same("draft terminal", &self.terminal, &terminal)
    }
}

/// Persist a private proposal, never a parity result. The handle is a UUID
/// prefix within the selected session's typed ancestry, not a mutable ordinal
/// or an implicit "latest" selection. Adoption still replays the whole draft.
pub fn save_manual_combat_draft_v1(
    session: &ManualSessionV1,
    draft: &ManualCombatDraftV1,
) -> Result<String, String> {
    save_manual_combat_draft_reference_v1(session, draft).map(|(_, handle)| handle)
}

/// Durable references keep the full identity; only presentation abbreviates it.
pub(crate) fn save_manual_combat_draft_reference_v1(
    session: &ManualSessionV1,
    draft: &ManualCombatDraftV1,
) -> Result<(String, String), String> {
    if draft.session_id != session.session_id()
        || draft.schema != "sts-manual-combat-draft-v1"
        || draft
            .source
            .as_ref()
            .is_some_and(|source| source.session_id != draft.session_id)
    {
        return Err("draft source does not match its owning session".to_owned());
    }
    let directory = session.directory().join("drafts");
    fs::create_dir_all(&directory).map_err(|e| format!("failed to create draft store: {e}"))?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let bytes = serde_json::to_vec(draft).map_err(|e| e.to_string())?;
    crate::draft_workspace::write_atomic_new(&directory.join(format!("{id}.json")), &bytes)?;
    let files = manual_draft_files(session)?;
    Ok((format!("D{id}"), draft_handle(&id, &files)))
}

pub(crate) fn display_manual_draft_reference_v1(
    session: &ManualSessionV1,
    reference: &str,
) -> Result<String, String> {
    let files = manual_draft_files(session)?;
    let id = reference
        .strip_prefix('D')
        .filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("invalid durable draft reference")?;
    if files.iter().filter(|(_, found, _)| found == id).count() != 1 {
        return Err("missing or ambiguous durable draft reference".into());
    }
    Ok(draft_handle(id, &files))
}

pub fn load_manual_combat_draft_v1(
    session: &ManualSessionV1,
    reference: &Path,
) -> Result<ManualCombatDraftV1, String> {
    let handle = reference.to_str().and_then(|s| s.strip_prefix('D'));
    let (path, owner) = if let Some(prefix) =
        handle.filter(|s| (8..=32).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        let prefix = prefix.to_ascii_lowercase();
        let matches = manual_draft_files(session)?
            .into_iter()
            .filter(|(_, id, _)| id.starts_with(&prefix))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [(path, _, owner)] => (path.clone(), Some(owner.clone())),
            [] => return Err("unknown draft handle; use drafts to see saved proposals".to_owned()),
            _ => {
                return Err(
                    "ambiguous draft handle; use the longer handle shown by drafts".to_owned(),
                )
            }
        }
    } else {
        // Explicit paths remain available for existing analyst-produced drafts.
        (reference.to_path_buf(), None)
    };
    let draft: ManualCombatDraftV1 =
        serde_json::from_slice(&fs::read(path).map_err(|e| format!("failed to read draft: {e}"))?)
            .map_err(|e| format!("failed to decode draft: {e}"))?;
    if owner.is_some_and(|id| draft.session_id != id) {
        return Err("draft source disagrees with its owning session".to_owned());
    }
    Ok(draft)
}

/// Display proposals at this root or along its selected prefix. Listing neither
/// certifies nor adopts them; current Rust semantics are checked at adoption.
pub fn list_manual_combat_drafts_v1(
    session: &ManualSessionV1,
    root: &ParitySnapshotV1,
) -> Result<Vec<(String, usize)>, String> {
    let files = manual_draft_files(session)?;
    let scopes = adopted_scopes(session.repository(), session, None)?;
    let mut proposals = Vec::new();
    for (path, id, owner) in &files {
        let draft = load_manual_combat_draft_v1(session, path)?;
        if draft.session_id != *owner {
            return Err("draft source disagrees with its owning session".to_owned());
        }
        let eligible = if let Some(source) = &draft.source {
            if source.session_id != draft.session_id {
                return Err("draft origin disagrees with its owning session".into());
            }
            source.selected_scope_index(&scopes).is_ok()
                && draft
                    .actions
                    .starts_with(&source.committed_suffix(session)?)
        } else {
            compare_snapshots_v1(&draft.root, root)?.status == ComparisonStatusV1::Exact
        };
        if eligible {
            proposals.push((draft_handle(id, &files), draft.actions.len()));
        }
    }
    Ok(proposals)
}

fn draft_handle(id: &str, files: &[(PathBuf, String, String)]) -> String {
    let length = (8..=32)
        .find(|length| {
            files
                .iter()
                .filter(|(_, other, _)| other.starts_with(&id[..*length]))
                .count()
                == 1
        })
        .unwrap_or(32);
    format!("D{}", &id[..length])
}

fn manual_draft_files(session: &ManualSessionV1) -> Result<Vec<(PathBuf, String, String)>, String> {
    let mut files = Vec::new();
    for (owner, _) in adopted_scopes(session.repository(), session, None)? {
        let entries = match fs::read_dir(owner.directory().join("drafts")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("failed to list drafts: {error}")),
        };
        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to list drafts: {e}"))?;
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && id.len() == 32
                && id
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                files.push((path.clone(), id.to_owned(), owner.session_id().to_owned()));
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) fn require_same(
    label: &str,
    expected: &ParitySnapshotV1,
    current: &ParitySnapshotV1,
) -> Result<(), String> {
    if compare_snapshots_v1(expected, current)?.status != ComparisonStatusV1::Exact {
        return Err(format!(
            "{label} differs from current replay; create a new draft"
        ));
    }
    Ok(())
}

pub fn commit_manual_combat_draft_v1<J: LockstepAdapterV1>(
    runner: &mut LockstepRunnerV1<J, RustLockstepAdapterV1>,
    draft: &ManualCombatDraftV1,
    on_exact: impl FnMut(usize, &LockstepRunnerV1<J, RustLockstepAdapterV1>) -> Result<(), String>,
) -> Result<usize, String> {
    if runner.is_stopped() || runner.current_comparison().status != ComparisonStatusV1::Exact {
        return Err("draft adoption requires an Exact running boundary".to_owned());
    }
    draft.validate_for_v1(runner.rust_adapter())?;
    runner.commit_exact_actions(&draft.actions, on_exact)?;
    require_same(
        "committed draft terminal",
        &draft.terminal,
        runner.current_snapshots().1,
    )?;
    Ok(draft.actions.len())
}
