use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::model::{
    RunPlayDeltaV1, RunPlayHeadV1, RunPlayStateV1, RUN_PLAY_DELTA_SCHEMA_V1,
    RUN_PLAY_HEAD_SCHEMA_V1, RUN_PLAY_STATE_SCHEMA_V1,
};

pub const DEFAULT_ARTIFACT_MAX_BYTES: usize = 256 * 1024 * 1024;

pub struct RunPlayWorkspaceV1 {
    pub head: RunPlayHeadV1,
    pub state: RunPlayStateV1,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RunPlayWorkspaceLoadProfileV1 {
    pub total: Duration,
    pub head_load: Duration,
    pub state_read: Duration,
    pub state_deserialize: Duration,
    pub delta_read: Duration,
    pub delta_deserialize: Duration,
    pub delta_apply: Duration,
    pub state_payload_bytes: usize,
    pub delta_payload_bytes: usize,
    pub delta_count: usize,
}

#[derive(Clone)]
pub struct RunPlayPathsV1 {
    root: PathBuf,
}

impl RunPlayPathsV1 {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn heads(&self) -> PathBuf {
        self.root.join("heads")
    }

    fn states(&self) -> PathBuf {
        self.root.join("states")
    }

    fn deltas(&self) -> PathBuf {
        self.root.join("deltas")
    }

    fn head_path(&self, revision: u64) -> PathBuf {
        self.heads().join(format!("head-{revision:020}.json"))
    }

    fn state_path(&self, revision: u64) -> PathBuf {
        self.states().join(format!("state-{revision:020}.msgpack"))
    }

    fn delta_path(&self, revision: u64) -> PathBuf {
        self.deltas().join(format!("delta-{revision:020}.msgpack"))
    }

    pub fn create(&self, head: &RunPlayHeadV1, state: &RunPlayStateV1) -> Result<(), String> {
        if self.root.exists() {
            return Err(format!(
                "run-play session already exists: {}",
                self.root.display()
            ));
        }
        fs::create_dir_all(self.heads()).map_err(|error| {
            format!(
                "failed to create run-play heads directory {}: {error}",
                self.heads().display()
            )
        })?;
        fs::create_dir_all(self.states()).map_err(|error| {
            format!(
                "failed to create run-play states directory {}: {error}",
                self.states().display()
            )
        })?;
        fs::create_dir_all(self.deltas()).map_err(|error| {
            format!(
                "failed to create run-play deltas directory {}: {error}",
                self.deltas().display()
            )
        })?;
        self.write_state_new(state)?;
        self.write_head_new(head)
    }

    pub fn load(&self) -> Result<RunPlayWorkspaceV1, String> {
        self.load_profiled_v1()
            .map(|(workspace, _profile)| workspace)
    }

    pub fn load_profiled_v1(
        &self,
    ) -> Result<(RunPlayWorkspaceV1, RunPlayWorkspaceLoadProfileV1), String> {
        let total = Instant::now();
        let phase = Instant::now();
        let head = self.load_head()?;
        let head_load = phase.elapsed();
        let state_path = self.state_path(head.state_revision);
        let phase = Instant::now();
        let state_payload =
            read_bounded(&state_path, DEFAULT_ARTIFACT_MAX_BYTES, "run-play state")?;
        let state_read = phase.elapsed();
        let state_payload_bytes = state_payload.len();
        let phase = Instant::now();
        let mut state: RunPlayStateV1 = rmp_serde::from_slice(&state_payload).map_err(|error| {
            format!(
                "failed to decode run-play state {}: {error}",
                state_path.display()
            )
        })?;
        let state_deserialize = phase.elapsed();
        if state.schema != RUN_PLAY_STATE_SCHEMA_V1 || state.state_revision != head.state_revision {
            return Err("run-play head and state identity disagree".to_string());
        }
        let mut delta_read = Duration::ZERO;
        let mut delta_deserialize = Duration::ZERO;
        let mut delta_apply = Duration::ZERO;
        let mut delta_payload_bytes = 0usize;
        for revision in &head.delta_revisions {
            let path = self.delta_path(*revision);
            let phase = Instant::now();
            let payload = read_bounded(&path, 32 * 1024 * 1024, "run-play delta")?;
            delta_read += phase.elapsed();
            delta_payload_bytes = delta_payload_bytes.saturating_add(payload.len());
            let phase = Instant::now();
            let delta: RunPlayDeltaV1 = rmp_serde::from_slice(&payload).map_err(|error| {
                format!(
                    "failed to decode run-play delta {}: {error}",
                    path.display()
                )
            })?;
            delta_deserialize += phase.elapsed();
            let phase = Instant::now();
            if delta.schema != RUN_PLAY_DELTA_SCHEMA_V1
                || delta.delta_revision != *revision
                || delta.base_state_revision != state.state_revision
                || delta.expected_archive_nodes != state.archive.nodes.len()
                || delta.expected_archive_attempts != state.archive.attempts.len()
            {
                return Err(format!(
                    "run-play delta {} does not extend its base",
                    path.display()
                ));
            }
            state.archive.nodes.extend(delta.appended_nodes);
            state.archive.attempts.extend(delta.appended_attempts);
            delta_apply += phase.elapsed();
        }
        if state
            .archive
            .node(head.position.archive_node_id())
            .is_none()
        {
            return Err(format!(
                "run-play head points to missing archive node {}",
                head.position.archive_node_id()
            ));
        }
        let profile = RunPlayWorkspaceLoadProfileV1 {
            total: total.elapsed(),
            head_load,
            state_read,
            state_deserialize,
            delta_read,
            delta_deserialize,
            delta_apply,
            state_payload_bytes,
            delta_payload_bytes,
            delta_count: head.delta_revisions.len(),
        };
        Ok((RunPlayWorkspaceV1 { head, state }, profile))
    }

    pub fn load_head(&self) -> Result<RunPlayHeadV1, String> {
        let head_path = self.latest_head_path()?;
        let head_payload = read_bounded(&head_path, 8 * 1024 * 1024, "run-play head")?;
        let head: RunPlayHeadV1 = serde_json::from_slice(&head_payload).map_err(|error| {
            format!(
                "failed to decode run-play head {}: {error}",
                head_path.display()
            )
        })?;
        if head.schema != RUN_PLAY_HEAD_SCHEMA_V1 {
            return Err(format!("unsupported run-play head schema {}", head.schema));
        }
        match (&head.position, &head.current_decision) {
            (crate::model::RunPlayPositionV1::Decision { archive_node_id }, Some(decision))
                if decision.target_archive_node_id == *archive_node_id => {}
            (crate::model::RunPlayPositionV1::Terminal { .. }, None) => {}
            _ => return Err("run-play head position and cached decision disagree".to_string()),
        }
        if !self.state_path(head.state_revision).is_file() {
            return Err(format!(
                "run-play head refers to missing state {}",
                head.state_revision
            ));
        }
        let mut previous_delta = None;
        for revision in &head.delta_revisions {
            if previous_delta.is_some_and(|previous| previous >= *revision)
                || *revision > head.head_revision
                || !self.delta_path(*revision).is_file()
            {
                return Err(format!(
                    "run-play head has an invalid or missing delta {revision}"
                ));
            }
            previous_delta = Some(*revision);
        }
        Ok(head)
    }

    /// Follow exact decisions in the current floor, then retained floor ends.
    /// Each link is a complete state generation; no action archive is replayed.
    pub fn load_recent_scratch_decision_v1(
        &self,
        steps: usize,
    ) -> Result<RunPlayWorkspaceV1, String> {
        if steps == 0 {
            return Err("scratch back steps must be positive".into());
        }
        let mut cursor = self.load_head()?;
        if !cursor.scratch {
            return Err("recent scratch decisions require a scratch session".into());
        }
        for _ in 0..steps {
            cursor = self
                .previous_scratch_head_v1(&cursor)?
                .ok_or("no earlier scratch decision is retained")?;
        }
        self.load_scratch_decision_head_v1(cursor)
    }

    /// Read the same bounded return chain as `back`, without loading checkpoints.
    pub(crate) fn recent_scratch_heads_v1(
        &self,
        head: &RunPlayHeadV1,
        limit: usize,
    ) -> Result<Vec<RunPlayHeadV1>, String> {
        if !head.scratch {
            return Err("recent scratch decisions require a scratch session".into());
        }
        let mut heads = Vec::new();
        let mut cursor = head.clone();
        for _ in 0..limit {
            let Some(previous) = self.previous_scratch_head_v1(&cursor)? else {
                break;
            };
            cursor = previous;
            heads.push(cursor.clone());
        }
        Ok(heads)
    }

    fn previous_scratch_head_v1(
        &self,
        current: &RunPlayHeadV1,
    ) -> Result<Option<RunPlayHeadV1>, String> {
        let Some(parent) = current.scratch_parent_revision else {
            return Ok(None);
        };
        let revision = if self.head_path(parent).is_file() {
            parent
        } else {
            let Some(end) = current.scratch_floor_ends.iter().rev().find(|end| {
                end.head_revision < current.head_revision
                    && self.head_path(end.head_revision).is_file()
            }) else {
                return Ok(None);
            };
            end.head_revision
        };
        let path = self.head_path(revision);
        let payload = read_bounded(&path, 8 * 1024 * 1024, "scratch head")?;
        let previous: RunPlayHeadV1 = serde_json::from_slice(&payload)
            .map_err(|error| format!("cannot decode scratch head {}: {error}", path.display()))?;
        if !previous.scratch
            || previous.head_revision != revision
            || previous.head_revision >= current.head_revision
            || previous.session_id != current.session_id
        {
            return Err("scratch undo chain is invalid".into());
        }
        Ok(Some(previous))
    }

    pub fn load_scratch_floor_end_v1(&self, floor: i32) -> Result<RunPlayWorkspaceV1, String> {
        let current = self.load_head()?;
        if !current.scratch {
            return Err("floor ends require a scratch session".into());
        }
        let revision = current
            .scratch_floor_ends
            .iter()
            .find(|end| end.floor == floor)
            .ok_or_else(|| format!("no retained floor {floor} end"))?
            .head_revision;
        let path = self.head_path(revision);
        let payload = read_bounded(&path, 8 * 1024 * 1024, "scratch floor head")?;
        let head: RunPlayHeadV1 = serde_json::from_slice(&payload)
            .map_err(|error| format!("cannot decode scratch head {}: {error}", path.display()))?;
        if !head.scratch || head.head_revision != revision {
            return Err("scratch floor end head is invalid".into());
        }
        self.load_scratch_decision_head_v1(head)
    }

    pub fn load_scratch_mark_v1(&self, name: &str) -> Result<RunPlayWorkspaceV1, String> {
        let current = self.load_head()?;
        self.load_scratch_decision_head_v1(self.load_scratch_mark_head_v1(&current, name)?)
    }

    /// Read the retained public decision without recovering its exact state or
    /// publishing a rewind. Mark summaries and restoration share this lookup.
    pub(crate) fn load_scratch_mark_head_v1(
        &self,
        current: &RunPlayHeadV1,
        name: &str,
    ) -> Result<RunPlayHeadV1, String> {
        if !current.scratch {
            return Err("marks require a scratch session".into());
        }
        let revision = current
            .scratch_marks
            .iter()
            .find(|mark| mark.name == name)
            .ok_or_else(|| format!("no retained mark {name:?}"))?
            .head_revision;
        let path = self.head_path(revision);
        let payload = read_bounded(&path, 8 * 1024 * 1024, "scratch mark head")?;
        let head: RunPlayHeadV1 = serde_json::from_slice(&payload)
            .map_err(|error| format!("cannot decode scratch head {}: {error}", path.display()))?;
        if !head.scratch
            || head.head_revision != revision
            || head.schema != current.schema
            || head.session_id != current.session_id
        {
            return Err("scratch mark head is invalid".into());
        }
        Ok(head)
    }

    /// Resolve an exact historical root through retained scratch heads only.
    /// Marks, floor ends and bounded undo already own retention; inspection
    /// neither revives pruned generations nor publishes a navigation step.
    pub(crate) fn load_retained_scratch_node_v1(
        &self,
        current: &RunPlayHeadV1,
        exact_node_id: &str,
    ) -> Result<Option<RunPlayWorkspaceV1>, String> {
        if !current.scratch {
            return Ok(None);
        }
        let mut selected: Option<RunPlayHeadV1> = None;
        for entry in fs::read_dir(self.heads()).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let Some(revision) =
                parse_revision(&entry.file_name().to_string_lossy(), "head-", ".json")
            else {
                continue;
            };
            if revision > current.head_revision {
                continue;
            }
            let path = entry.path();
            let payload = read_bounded(&path, 8 * 1024 * 1024, "retained scratch head")?;
            let head: RunPlayHeadV1 = serde_json::from_slice(&payload).map_err(|error| {
                format!("cannot decode scratch head {}: {error}", path.display())
            })?;
            if !matches!(&head.position,
                crate::model::RunPlayPositionV1::Decision { archive_node_id }
                    if archive_node_id == exact_node_id
            ) {
                continue;
            }
            if head.schema != RUN_PLAY_HEAD_SCHEMA_V1
                || !head.scratch
                || head.head_revision != revision
                || head.session_id != current.session_id
                || head
                    .current_decision
                    .as_ref()
                    .is_none_or(|d| d.target_archive_node_id != exact_node_id)
            {
                return Err("retained scratch root identity and head disagree".into());
            }
            if let Some(previous) = &selected {
                if previous.current_decision != head.current_decision {
                    return Err("retained scratch node has conflicting public snapshots".into());
                }
                if previous.head_revision >= head.head_revision {
                    continue;
                }
            }
            selected = Some(head);
        }
        selected
            .map(|head| self.load_scratch_decision_head_v1(head))
            .transpose()
    }

    pub(crate) fn load_scratch_decision_head_v1(
        &self,
        cursor: RunPlayHeadV1,
    ) -> Result<RunPlayWorkspaceV1, String> {
        if !cursor.delta_revisions.is_empty() {
            return Err("scratch undo state unexpectedly contains deltas".into());
        }
        let path = self.state_path(cursor.state_revision);
        let payload = read_bounded(&path, DEFAULT_ARTIFACT_MAX_BYTES, "scratch state")?;
        let state: RunPlayStateV1 = rmp_serde::from_slice(&payload)
            .map_err(|error| format!("cannot decode scratch state {}: {error}", path.display()))?;
        if state.schema != RUN_PLAY_STATE_SCHEMA_V1
            || state.state_revision != cursor.state_revision
            || state
                .archive
                .node(cursor.position.archive_node_id())
                .is_none()
        {
            return Err("scratch undo state and head disagree".into());
        }
        Ok(RunPlayWorkspaceV1 {
            head: cursor,
            state,
        })
    }

    /// Read an issued retained generation; numbers are never interpreted as steps.
    pub(crate) fn scratch_head_revision_v1(
        &self,
        current: &RunPlayHeadV1,
        revision: u64,
    ) -> Result<RunPlayHeadV1, String> {
        let payload = read_bounded(
            &self.head_path(revision),
            8 * 1024 * 1024,
            "retained scratch head",
        )?;
        let head: RunPlayHeadV1 =
            serde_json::from_slice(&payload).map_err(|error| error.to_string())?;
        if !head.scratch
            || head.schema != current.schema
            || head.session_id != current.session_id
            || head.head_revision != revision
            || revision > current.head_revision
            || !self.state_path(head.state_revision).is_file()
        {
            return Err("retained scratch head identity is invalid".into());
        }
        Ok(head)
    }

    pub fn publish(
        &self,
        loaded_head_revision: u64,
        head: &RunPlayHeadV1,
        new_state: Option<&RunPlayStateV1>,
        new_delta: Option<&RunPlayDeltaV1>,
    ) -> Result<(), String> {
        crate::session_control::ensure_session_writable_v1(self)?;
        let current = self.latest_head_revision()?;
        if current != loaded_head_revision {
            return Err(format!(
                "run-play session advanced concurrently from head {loaded_head_revision} to {current}"
            ));
        }
        if head.head_revision != loaded_head_revision.saturating_add(1) {
            return Err("run-play head revision is not the next revision".to_string());
        }
        if new_state.is_some() && new_delta.is_some() {
            return Err("run-play publication cannot contain both a state and delta".to_string());
        }
        if let Some(state) = new_state {
            if state.state_revision != head.state_revision {
                return Err("new run-play state revision disagrees with head".to_string());
            }
            if !head.delta_revisions.is_empty() {
                return Err("new run-play state must reset its delta chain".to_string());
            }
            self.write_state_new(state)?;
        } else if let Some(delta) = new_delta {
            if delta.delta_revision != head.head_revision
                || delta.base_state_revision != head.state_revision
                || head.delta_revisions.last() != Some(&delta.delta_revision)
            {
                return Err("run-play delta identity disagrees with the new head".to_string());
            }
            self.write_delta_new(delta)?;
        } else if !self.state_path(head.state_revision).is_file() {
            return Err("run-play head refers to an unavailable state".to_string());
        }
        // The small append-only head is the commit record. A crash before this
        // create leaves at most an unreferenced state; it never publishes a
        // half-written archive/checkpoint pair.
        self.write_head_new(head)?;
        if head.scratch {
            self.prune_scratch_generations(head)?;
        } else {
            self.prune_heads(head.head_revision, 2)?;
            self.prune_states(head.state_revision, 2)?;
        }
        self.prune_deltas(&head.delta_revisions)
    }

    fn prune_scratch_generations(&self, head: &RunPlayHeadV1) -> Result<(), String> {
        // The latest floor remains undoable until the following floor closes.
        // Older floors retain one exact map decision each.
        let stop_at = head
            .scratch_floor_ends
            .iter()
            .rev()
            .nth(1)
            .map_or(0, |end| end.head_revision);
        let mut keep_heads = BTreeSet::new();
        let mut cursor = head.clone();
        loop {
            keep_heads.insert(cursor.head_revision);
            if cursor.head_revision <= stop_at {
                break;
            }
            let Some(parent) = cursor.scratch_parent_revision else {
                break;
            };
            let path = self.head_path(parent);
            if !path.is_file() {
                break;
            }
            let payload = read_bounded(&path, 8 * 1024 * 1024, "scratch head")?;
            cursor = serde_json::from_slice(&payload).map_err(|error| {
                format!("cannot decode scratch head {}: {error}", path.display())
            })?;
        }
        keep_heads.extend(head.scratch_floor_ends.iter().map(|end| end.head_revision));
        for revision in head
            .scratch_marks
            .iter()
            .map(|mark| mark.head_revision)
            .chain(head.scratch_return_revision)
        {
            keep_heads.insert(revision);
            let path = self.head_path(revision);
            let payload = read_bounded(&path, 8 * 1024 * 1024, "scratch mark head")?;
            let marked: RunPlayHeadV1 = serde_json::from_slice(&payload).map_err(|error| {
                format!("cannot decode scratch head {}: {error}", path.display())
            })?;
            keep_heads.extend(
                marked
                    .scratch_floor_ends
                    .iter()
                    .map(|end| end.head_revision),
            );
        }
        let mut keep_states = BTreeSet::new();
        for revision in &keep_heads {
            let path = self.head_path(*revision);
            let payload = read_bounded(&path, 8 * 1024 * 1024, "scratch head")?;
            let retained: RunPlayHeadV1 = serde_json::from_slice(&payload).map_err(|error| {
                format!("cannot decode scratch head {}: {error}", path.display())
            })?;
            keep_states.insert(retained.state_revision);
        }
        for entry in fs::read_dir(self.heads()).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if let Some(revision) =
                parse_revision(&entry.file_name().to_string_lossy(), "head-", ".json")
            {
                if !keep_heads.contains(&revision) {
                    fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
                }
            }
        }
        for entry in fs::read_dir(self.states()).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if let Some(revision) =
                parse_revision(&entry.file_name().to_string_lossy(), "state-", ".msgpack")
            {
                if !keep_states.contains(&revision) {
                    fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(())
    }

    fn latest_head_path(&self) -> Result<PathBuf, String> {
        let revision = self.latest_head_revision()?;
        Ok(self.head_path(revision))
    }

    pub(crate) fn latest_head_path_for_control_v1(&self) -> Result<PathBuf, String> {
        self.latest_head_path()
    }

    fn latest_head_revision(&self) -> Result<u64, String> {
        let entries = fs::read_dir(self.heads()).map_err(|error| {
            format!(
                "failed to read run-play heads directory {}: {error}",
                self.heads().display()
            )
        })?;
        let mut revisions = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                parse_revision(&entry.file_name().to_string_lossy(), "head-", ".json")
            })
            .collect::<Vec<_>>();
        revisions.sort_unstable();
        revisions
            .pop()
            .ok_or_else(|| "run-play session has no committed head".to_string())
    }

    fn write_state_new(&self, state: &RunPlayStateV1) -> Result<(), String> {
        let payload = rmp_serde::to_vec_named(state)
            .map_err(|error| format!("failed to encode run-play state: {error}"))?;
        if payload.len() > DEFAULT_ARTIFACT_MAX_BYTES {
            return Err("run-play state exceeds the artifact size limit".to_string());
        }
        write_new_synced(
            &self.state_path(state.state_revision),
            &payload,
            "run-play state",
        )
    }

    fn write_head_new(&self, head: &RunPlayHeadV1) -> Result<(), String> {
        let payload = serde_json::to_vec_pretty(head)
            .map_err(|error| format!("failed to encode run-play head: {error}"))?;
        write_new_synced(
            &self.head_path(head.head_revision),
            &payload,
            "run-play head",
        )
    }

    fn write_delta_new(&self, delta: &RunPlayDeltaV1) -> Result<(), String> {
        let payload = rmp_serde::to_vec_named(delta)
            .map_err(|error| format!("failed to encode run-play delta: {error}"))?;
        write_new_synced(
            &self.delta_path(delta.delta_revision),
            &payload,
            "run-play delta",
        )
    }

    fn prune_heads(&self, current_revision: u64, retained: u64) -> Result<(), String> {
        let keep_from = current_revision.saturating_sub(retained - 1);
        for entry in fs::read_dir(self.heads()).map_err(|error| {
            format!(
                "failed to inspect run-play heads directory {}: {error}",
                self.heads().display()
            )
        })? {
            let entry =
                entry.map_err(|error| format!("failed to inspect run-play head: {error}"))?;
            let Some(revision) =
                parse_revision(&entry.file_name().to_string_lossy(), "head-", ".json")
            else {
                continue;
            };
            if revision < keep_from {
                fs::remove_file(entry.path()).map_err(|error| {
                    format!(
                        "failed to prune superseded run-play head {}: {error}",
                        entry.path().display()
                    )
                })?;
            }
        }
        Ok(())
    }

    fn prune_states(&self, current_revision: u64, retained: u64) -> Result<(), String> {
        let keep_from = current_revision.saturating_sub(retained - 1);
        for entry in fs::read_dir(self.states()).map_err(|error| {
            format!(
                "failed to inspect run-play states directory {}: {error}",
                self.states().display()
            )
        })? {
            let entry =
                entry.map_err(|error| format!("failed to inspect run-play state: {error}"))?;
            let Some(revision) =
                parse_revision(&entry.file_name().to_string_lossy(), "state-", ".msgpack")
            else {
                continue;
            };
            if revision < keep_from {
                fs::remove_file(entry.path()).map_err(|error| {
                    format!(
                        "failed to prune superseded run-play state {}: {error}",
                        entry.path().display()
                    )
                })?;
            }
        }
        Ok(())
    }

    fn prune_deltas(&self, retained: &[u64]) -> Result<(), String> {
        for entry in fs::read_dir(self.deltas()).map_err(|error| {
            format!(
                "failed to inspect run-play deltas directory {}: {error}",
                self.deltas().display()
            )
        })? {
            let entry =
                entry.map_err(|error| format!("failed to inspect run-play delta: {error}"))?;
            let Some(revision) =
                parse_revision(&entry.file_name().to_string_lossy(), "delta-", ".msgpack")
            else {
                continue;
            };
            if !retained.contains(&revision) {
                fs::remove_file(entry.path()).map_err(|error| {
                    format!(
                        "failed to prune superseded run-play delta {}: {error}",
                        entry.path().display()
                    )
                })?;
            }
        }
        Ok(())
    }
}

fn parse_revision(name: &str, prefix: &str, suffix: &str) -> Option<u64> {
    name.strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse()
        .ok()
}

pub fn read_bounded(path: &Path, max_bytes: usize, description: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "failed to inspect {description} {}: {error}",
            path.display()
        )
    })?;
    if metadata.len() > max_bytes as u64 {
        return Err(format!(
            "{description} {} exceeds size limit",
            path.display()
        ));
    }
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open {description} {}: {error}", path.display()))?;
    let mut payload = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut payload)
        .map_err(|error| format!("failed to read {description} {}: {error}", path.display()))?;
    Ok(payload)
}

pub fn write_new_synced(path: &Path, payload: &[u8], description: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create {description} {}: {error}", path.display()))?;
    file.write_all(payload)
        .map_err(|error| format!("failed to write {description} {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("failed to sync {description} {}: {error}", path.display()))
}
