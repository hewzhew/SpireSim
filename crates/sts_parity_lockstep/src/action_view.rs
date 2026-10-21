//! Input references belong to one owner revision, never to a moving menu.
use crate::{ParitySnapshotV1, SemanticActionSpaceV1, SemanticActionV1};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq)]
pub struct ActionViewV1 {
    reference_prefix: String,
    owner: String,
    revision: u64,
    snapshot: ParitySnapshotV1,
    actions: SemanticActionSpaceV1,
}

impl ActionViewV1 {
    /// Owner must identify this execution incarnation (including reopen/reload).
    /// Revision is monotonic even when the owner undoes a private edit.
    pub fn new(owner: &str, revision: u64, snapshot: &ParitySnapshotV1) -> Result<Self, String> {
        if owner.is_empty() {
            return Err("action view requires an owner".into());
        }
        let mut actions = SemanticActionSpaceV1::from_snapshot(snapshot)?;
        let bytes = serde_json::to_vec(&(owner, revision, snapshot)).map_err(|e| e.to_string())?;
        let digest = Sha256::digest(bytes);
        let token = URL_SAFE_NO_PAD.encode(&digest[..16]);
        for (index, action) in actions.actions.iter_mut().enumerate() {
            action.handle = format!("V{token}:{index}");
        }
        Ok(Self {
            reference_prefix: format!("V{token}"),
            owner: owner.into(),
            revision,
            snapshot: snapshot.clone(),
            actions,
        })
    }

    pub fn actions(&self) -> &SemanticActionSpaceV1 {
        &self.actions
    }

    pub fn reference_prefix(&self) -> &str {
        &self.reference_prefix
    }

    /// The full source and snapshot are checked as well as the display token.
    pub fn resolve_for(&self, current: &Self, handle: &str) -> Result<SemanticActionV1, String> {
        if self != current {
            return Err("stale action view; observe the current boundary".into());
        }
        self.actions.resolve(handle).cloned().ok_or_else(|| {
            "stale or unknown action reference; copy a reference from the current view".into()
        })
    }
}
