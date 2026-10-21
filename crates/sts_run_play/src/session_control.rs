use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sts_oracle_learning::agent::task::TaskDecisionSiteV1;
use sts_oracle_learning::eval::run_control::TaskRunBranchDecisionInventoryV1;

use crate::compact::render_status_v1;
use crate::model::RunPlayPositionV1;
use crate::storage::{read_bounded, write_new_synced, RunPlayPathsV1};

const FOCUS_SCHEMA_V1: &str = "sts-run-play-focus-v1";
const SESSION_CONTROL_SCHEMA_V1: &str = "sts-run-play-session-control-v1";
const HANDOFF_EVENT_SCHEMA_V2: &str = "sts-run-play-handoff-event-v2";
const OVERVIEW_SCHEMA_V1: &str = "sts-run-play-overview-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlaySessionLifecycleV1 {
    Open,
    Parked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPlaySessionPurposeV1 {
    Unknown,
    Play,
    Lab,
    Probe,
    Archive,
}

impl std::str::FromStr for RunPlaySessionPurposeV1 {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "play" => Ok(Self::Play),
            "lab" => Ok(Self::Lab),
            "probe" => Ok(Self::Probe),
            "archive" => Ok(Self::Archive),
            _ => Err(format!(
                "unknown session purpose {value}; expected unknown, play, lab, probe, or archive"
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RunPlaySessionControlKindV1 {
    Describe {
        purpose: RunPlaySessionPurposeV1,
        label: Option<String>,
    },
    Park {
        reason: String,
    },
    Unpark {
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RunPlaySessionControlEventV1 {
    schema: String,
    control_revision: u64,
    created_unix_ms: u64,
    session_id: String,
    head_revision: u64,
    state_revision: u64,
    archive_node_id: String,
    public_snapshot_id: Option<String>,
    action: RunPlaySessionControlKindV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunPlaySessionControlStateV1 {
    pub lifecycle: RunPlaySessionLifecycleV1,
    pub purpose: RunPlaySessionPurposeV1,
    pub label: Option<String>,
    pub parked_reason: Option<String>,
    pub latest_control_revision: Option<u64>,
}

impl Default for RunPlaySessionControlStateV1 {
    fn default() -> Self {
        Self {
            lifecycle: RunPlaySessionLifecycleV1::Open,
            purpose: RunPlaySessionPurposeV1::Unknown,
            label: None,
            parked_reason: None,
            latest_control_revision: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlayFocusEventV1 {
    pub schema: String,
    pub focus_revision: u64,
    pub created_unix_ms: u64,
    pub session_path: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RunPlayHandoffActionV2 {
    Open {
        pause_reason: String,
        do_not_act: String,
        open_issues: Vec<String>,
        completed_issues: Vec<String>,
    },
    Resolve {
        resolved_handoff_revision: u64,
        resolution: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RunPlayHandoffEventV2 {
    pub schema: String,
    pub handoff_revision: u64,
    pub created_unix_ms: u64,
    pub repository_commit: String,
    pub session_id: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub archive_node_id: String,
    pub public_snapshot_id: Option<String>,
    action: RunPlayHandoffActionV2,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunPlayHandoffSummaryV1 {
    pub historical_event_count: usize,
    pub latest_event_revision: Option<u64>,
    pub open: Option<RunPlayOpenHandoffV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunPlayOpenHandoffV1 {
    pub handoff_revision: u64,
    pub repository_commit: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub archive_node_id: String,
    pub public_snapshot_id: Option<String>,
    pub pause_reason: String,
    pub do_not_act: String,
    pub open_issues: Vec<String>,
    pub completed_issues: Vec<String>,
    pub matches_current_head: bool,
    pub matches_repository_commit: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunPlaySessionCatalogEntryV1 {
    pub path: String,
    pub session_id: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub archive_node_id: String,
    pub terminal: bool,
    pub decision_site: Option<TaskDecisionSiteV1>,
    pub target_depth: Option<usize>,
    pub public_snapshot_id: Option<String>,
    pub lifecycle: RunPlaySessionLifecycleV1,
    pub purpose: RunPlaySessionPurposeV1,
    pub label: Option<String>,
    pub parked_reason: Option<String>,
    pub last_activity_unix_ms: u64,
    pub focused: bool,
    pub current_handoff: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunPlayUnreadableSessionV1 {
    pub path: String,
    pub error: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunPlayFocusedSessionV1 {
    pub catalog: RunPlaySessionCatalogEntryV1,
    pub status: String,
    pub current_decision: Option<TaskRunBranchDecisionInventoryV1>,
    pub handoff: RunPlayHandoffSummaryV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunPlayOverviewV1 {
    pub schema: String,
    pub repository_commit: String,
    pub repository_dirty: bool,
    pub workbench_sha256: String,
    pub focus: RunPlayFocusEventV1,
    pub focused: Option<RunPlayFocusedSessionV1>,
    pub sessions: Vec<RunPlaySessionCatalogEntryV1>,
    pub unreadable_sessions: Vec<RunPlayUnreadableSessionV1>,
    pub alerts: Vec<String>,
}

pub fn repository_root_v1() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "run-play manifest is outside the repository".to_string())
}

pub fn load_focus_v1(focus_root: &Path) -> Result<RunPlayFocusEventV1, String> {
    let Some(revision) = latest_revision_v1(focus_root, "focus-", ".json")? else {
        return Ok(RunPlayFocusEventV1 {
            schema: FOCUS_SCHEMA_V1.to_string(),
            focus_revision: 0,
            created_unix_ms: 0,
            session_path: None,
            session_id: None,
        });
    };
    let path = focus_root.join(format!("focus-{revision:020}.json"));
    let payload = read_bounded(&path, 128 * 1024, "run-play focus event")?;
    let event: RunPlayFocusEventV1 = serde_json::from_slice(&payload).map_err(|error| {
        format!(
            "failed to decode run-play focus {}: {error}",
            path.display()
        )
    })?;
    validate_focus_event_v1(&event, revision)?;
    Ok(event)
}

pub fn set_focus_v1(focus_root: &Path, session: &Path) -> Result<RunPlayFocusEventV1, String> {
    let repository = repository_root_v1()?;
    let local_root =
        canonical_directory_v1(&repository.join(".oracle-lab"), "run-play local root")?;
    let session = canonical_directory_v1(session, "run-play focus session")?;
    if !session.starts_with(&local_root) {
        return Err(format!(
            "focused session must be below {}: {}",
            local_root.display(),
            session.display()
        ));
    }
    let head = RunPlayPathsV1::new(&session).load_head()?;
    let session_path = repository_relative_text_v1(&repository, &session)?;
    let current = load_focus_v1(focus_root)?;
    if current.session_path.as_deref() == Some(session_path.as_str())
        && current.session_id.as_deref() == Some(head.session_id.as_str())
    {
        return Ok(current);
    }
    append_focus_v1(focus_root, Some(session_path), Some(head.session_id))
}

pub fn clear_focus_v1(focus_root: &Path) -> Result<RunPlayFocusEventV1, String> {
    let current = load_focus_v1(focus_root)?;
    if current.session_path.is_none() {
        return Ok(current);
    }
    append_focus_v1(focus_root, None, None)
}

pub fn migrate_legacy_focus_v1(
    focus_root: &Path,
    legacy_pointer: &Path,
) -> Result<RunPlayFocusEventV1, String> {
    if latest_revision_v1(focus_root, "focus-", ".json")?.is_some() {
        return load_focus_v1(focus_root);
    }
    let payload = read_bounded(legacy_pointer, 16 * 1024, "legacy run-play focus pointer")?;
    let text = std::str::from_utf8(&payload)
        .map_err(|error| format!("legacy run-play focus is not UTF-8: {error}"))?
        .trim();
    if text.is_empty() {
        return Err("legacy run-play focus pointer is empty".to_string());
    }
    let repository = repository_root_v1()?;
    let session = if Path::new(text).is_absolute() {
        PathBuf::from(text)
    } else {
        repository.join(text)
    };
    set_focus_v1(focus_root, &session)
}

pub fn resolve_focus_session_path_v1(focus_root: &Path) -> Result<Option<PathBuf>, String> {
    let focus = load_focus_v1(focus_root)?;
    let Some(stored) = focus.session_path.as_deref() else {
        return Ok(None);
    };
    let repository = canonical_directory_v1(&repository_root_v1()?, "repository")?;
    let local_root =
        canonical_directory_v1(&repository.join(".oracle-lab"), "run-play local root")?;
    let stored = Path::new(stored);
    if stored.is_absolute() {
        return Err("typed run-play focus must store a repository-relative path".to_string());
    }
    let session = canonical_directory_v1(&repository.join(stored), "focused run-play session")?;
    if !session.starts_with(&local_root) {
        return Err(format!(
            "focused session escaped the local run-play root: {}",
            session.display()
        ));
    }
    Ok(Some(session))
}

fn append_focus_v1(
    focus_root: &Path,
    session_path: Option<String>,
    session_id: Option<String>,
) -> Result<RunPlayFocusEventV1, String> {
    if session_path.is_some() != session_id.is_some() {
        return Err("run-play focus path and session id must be present together".to_string());
    }
    fs::create_dir_all(focus_root).map_err(|error| {
        format!(
            "failed to create run-play focus root {}: {error}",
            focus_root.display()
        )
    })?;
    let revision = latest_revision_v1(focus_root, "focus-", ".json")?
        .map_or(1, |revision| revision.saturating_add(1));
    let event = RunPlayFocusEventV1 {
        schema: FOCUS_SCHEMA_V1.to_string(),
        focus_revision: revision,
        created_unix_ms: unix_ms_v1()?,
        session_path,
        session_id,
    };
    validate_focus_event_v1(&event, revision)?;
    write_json_new_v1(
        &focus_root.join(format!("focus-{revision:020}.json")),
        &event,
        "run-play focus event",
    )?;
    Ok(event)
}

pub fn load_session_control_v1(
    paths: &RunPlayPathsV1,
) -> Result<RunPlaySessionControlStateV1, String> {
    let head = paths.load_head()?;
    let control_root = paths.root().join("control");
    let mut state = RunPlaySessionControlStateV1::default();
    for (revision, path) in revision_paths_v1(&control_root, "control-", ".json")? {
        let payload = read_bounded(&path, 128 * 1024, "run-play session control event")?;
        let event: RunPlaySessionControlEventV1 =
            serde_json::from_slice(&payload).map_err(|error| {
                format!(
                    "failed to decode run-play control {}: {error}",
                    path.display()
                )
            })?;
        validate_control_event_v1(&event, revision, &head.session_id)?;
        match event.action {
            RunPlaySessionControlKindV1::Describe { purpose, label } => {
                state.purpose = purpose;
                state.label = label;
            }
            RunPlaySessionControlKindV1::Park { reason } => {
                state.lifecycle = RunPlaySessionLifecycleV1::Parked;
                state.parked_reason = Some(reason);
            }
            RunPlaySessionControlKindV1::Unpark { .. } => {
                state.lifecycle = RunPlaySessionLifecycleV1::Open;
                state.parked_reason = None;
            }
        }
        state.latest_control_revision = Some(revision);
    }
    Ok(state)
}

pub fn describe_session_v1(
    paths: &RunPlayPathsV1,
    purpose: RunPlaySessionPurposeV1,
    label: Option<String>,
) -> Result<RunPlaySessionControlStateV1, String> {
    if let Some(label) = label.as_deref() {
        validate_short_text_v1("session label", label)?;
    }
    let current = load_session_control_v1(paths)?;
    if current.purpose == purpose && current.label == label {
        return Ok(current);
    }
    append_control_event_v1(
        paths,
        RunPlaySessionControlKindV1::Describe { purpose, label },
    )?;
    load_session_control_v1(paths)
}

pub fn park_session_v1(
    paths: &RunPlayPathsV1,
    reason: String,
) -> Result<RunPlaySessionControlStateV1, String> {
    validate_short_text_v1("park reason", &reason)?;
    let current = load_session_control_v1(paths)?;
    if current.lifecycle == RunPlaySessionLifecycleV1::Parked
        && current.parked_reason.as_deref() == Some(reason.as_str())
    {
        return Ok(current);
    }
    append_control_event_v1(paths, RunPlaySessionControlKindV1::Park { reason })?;
    load_session_control_v1(paths)
}

pub fn unpark_session_v1(
    paths: &RunPlayPathsV1,
    reason: String,
) -> Result<RunPlaySessionControlStateV1, String> {
    validate_short_text_v1("unpark reason", &reason)?;
    let current = load_session_control_v1(paths)?;
    if current.lifecycle == RunPlaySessionLifecycleV1::Open {
        return Ok(current);
    }
    append_control_event_v1(paths, RunPlaySessionControlKindV1::Unpark { reason })?;
    load_session_control_v1(paths)
}

pub fn ensure_session_writable_v1(paths: &RunPlayPathsV1) -> Result<(), String> {
    let control = load_session_control_v1(paths)?;
    if control.lifecycle == RunPlaySessionLifecycleV1::Parked {
        return Err(format!(
            "run-play session is parked{}; unpark it explicitly before changing its head",
            control
                .parked_reason
                .as_deref()
                .map_or_else(String::new, |reason| format!(": {reason}"))
        ));
    }
    Ok(())
}

fn append_control_event_v1(
    paths: &RunPlayPathsV1,
    action: RunPlaySessionControlKindV1,
) -> Result<RunPlaySessionControlEventV1, String> {
    let head = paths.load_head()?;
    let control_root = paths.root().join("control");
    fs::create_dir_all(&control_root).map_err(|error| {
        format!(
            "failed to create run-play control root {}: {error}",
            control_root.display()
        )
    })?;
    let revision = latest_revision_v1(&control_root, "control-", ".json")?
        .map_or(0, |revision| revision.saturating_add(1));
    let event = RunPlaySessionControlEventV1 {
        schema: SESSION_CONTROL_SCHEMA_V1.to_string(),
        control_revision: revision,
        created_unix_ms: unix_ms_v1()?,
        session_id: head.session_id.clone(),
        head_revision: head.head_revision,
        state_revision: head.state_revision,
        archive_node_id: head.position.archive_node_id().to_string(),
        public_snapshot_id: head
            .current_decision
            .as_ref()
            .map(|decision| decision.public_snapshot_id.clone()),
        action,
    };
    validate_control_event_v1(&event, revision, &head.session_id)?;
    write_json_new_v1(
        &control_root.join(format!("control-{revision:020}.json")),
        &event,
        "run-play session control event",
    )?;
    Ok(event)
}

pub fn open_handoff_v2(
    paths: &RunPlayPathsV1,
    repository_commit: String,
    pause_reason: String,
    do_not_act: String,
    open_issues: Vec<String>,
    completed_issues: Vec<String>,
) -> Result<RunPlayHandoffSummaryV1, String> {
    validate_short_text_v1("repository commit", &repository_commit)?;
    validate_short_text_v1("pause reason", &pause_reason)?;
    validate_short_text_v1("do-not-act boundary", &do_not_act)?;
    validate_issue_lists_v1(&open_issues, &completed_issues)?;
    let current = load_handoff_summary_v1(paths, Some(&repository_commit))?;
    if current.open.as_ref().is_some_and(|open| {
        open.matches_current_head
            && open.matches_repository_commit
            && open.pause_reason == pause_reason
            && open.do_not_act == do_not_act
            && open.open_issues == open_issues
            && open.completed_issues == completed_issues
    }) {
        return Ok(current);
    }
    append_handoff_event_v2(
        paths,
        repository_commit.clone(),
        RunPlayHandoffActionV2::Open {
            pause_reason,
            do_not_act,
            open_issues,
            completed_issues,
        },
    )?;
    load_handoff_summary_v1(paths, Some(&repository_commit))
}

pub fn resolve_handoff_v2(
    paths: &RunPlayPathsV1,
    repository_commit: String,
    resolution: String,
) -> Result<RunPlayHandoffSummaryV1, String> {
    validate_short_text_v1("repository commit", &repository_commit)?;
    validate_short_text_v1("handoff resolution", &resolution)?;
    let summary = load_handoff_summary_v1(paths, Some(&repository_commit))?;
    let open = summary
        .open
        .ok_or_else(|| "run-play session has no open V2 handoff to resolve".to_string())?;
    append_handoff_event_v2(
        paths,
        repository_commit.clone(),
        RunPlayHandoffActionV2::Resolve {
            resolved_handoff_revision: open.handoff_revision,
            resolution,
        },
    )?;
    load_handoff_summary_v1(paths, Some(&repository_commit))
}

pub fn load_handoff_summary_v1(
    paths: &RunPlayPathsV1,
    repository_commit: Option<&str>,
) -> Result<RunPlayHandoffSummaryV1, String> {
    let head = paths.load_head()?;
    let handoff_root = paths.root().join("handoffs");
    let mut event_count = 0usize;
    let mut latest_event_revision = None;
    let mut open: Option<RunPlayHandoffEventV2> = None;
    for (revision, path) in revision_paths_v1(&handoff_root, "handoff-", ".json")? {
        event_count = event_count.saturating_add(1);
        let payload = read_bounded(&path, 128 * 1024, "run-play handoff event")?;
        let value: serde_json::Value = serde_json::from_slice(&payload)
            .map_err(|error| format!("failed to decode handoff {}: {error}", path.display()))?;
        match value.get("schema").and_then(serde_json::Value::as_str) {
            Some(HANDOFF_EVENT_SCHEMA_V2) => {
                let event: RunPlayHandoffEventV2 =
                    serde_json::from_value(value).map_err(|error| {
                        format!("failed to decode V2 handoff {}: {error}", path.display())
                    })?;
                validate_handoff_event_v2(&event, revision, &head.session_id)?;
                match &event.action {
                    RunPlayHandoffActionV2::Open { .. } => open = Some(event),
                    RunPlayHandoffActionV2::Resolve {
                        resolved_handoff_revision,
                        ..
                    } => {
                        if open.as_ref().map(|event| event.handoff_revision)
                            != Some(*resolved_handoff_revision)
                        {
                            return Err(format!(
                                "V2 handoff resolve r{revision} does not name the currently open handoff"
                            ));
                        }
                        open = None;
                    }
                }
            }
            Some("sts-run-play-handoff-receipt-v1") => {}
            Some(schema) => {
                return Err(format!(
                    "unsupported run-play handoff schema {schema} in {}",
                    path.display()
                ));
            }
            None => {
                return Err(format!(
                    "run-play handoff has no schema: {}",
                    path.display()
                ));
            }
        }
        latest_event_revision = Some(revision);
    }
    let open = open.and_then(|event| {
        let RunPlayHandoffActionV2::Open {
            pause_reason,
            do_not_act,
            open_issues,
            completed_issues,
        } = event.action
        else {
            return None;
        };
        Some(RunPlayOpenHandoffV1 {
            handoff_revision: event.handoff_revision,
            repository_commit: event.repository_commit.clone(),
            head_revision: event.head_revision,
            state_revision: event.state_revision,
            archive_node_id: event.archive_node_id.clone(),
            public_snapshot_id: event.public_snapshot_id.clone(),
            pause_reason,
            do_not_act,
            open_issues,
            completed_issues,
            matches_current_head: event.session_id == head.session_id
                && event.head_revision == head.head_revision
                && event.state_revision == head.state_revision
                && event.archive_node_id == head.position.archive_node_id()
                && event.public_snapshot_id
                    == head
                        .current_decision
                        .as_ref()
                        .map(|decision| decision.public_snapshot_id.clone()),
            matches_repository_commit: repository_commit
                .is_some_and(|commit| commit == event.repository_commit),
        })
    });
    let historical_event_count = event_count.saturating_sub(usize::from(open.is_some()));
    Ok(RunPlayHandoffSummaryV1 {
        historical_event_count,
        latest_event_revision,
        open,
    })
}

fn append_handoff_event_v2(
    paths: &RunPlayPathsV1,
    repository_commit: String,
    action: RunPlayHandoffActionV2,
) -> Result<RunPlayHandoffEventV2, String> {
    let head = paths.load_head()?;
    let handoff_root = paths.root().join("handoffs");
    fs::create_dir_all(&handoff_root).map_err(|error| {
        format!(
            "failed to create run-play handoff root {}: {error}",
            handoff_root.display()
        )
    })?;
    let revision = latest_revision_v1(&handoff_root, "handoff-", ".json")?
        .map_or(0, |revision| revision.saturating_add(1));
    let event = RunPlayHandoffEventV2 {
        schema: HANDOFF_EVENT_SCHEMA_V2.to_string(),
        handoff_revision: revision,
        created_unix_ms: unix_ms_v1()?,
        repository_commit,
        session_id: head.session_id.clone(),
        head_revision: head.head_revision,
        state_revision: head.state_revision,
        archive_node_id: head.position.archive_node_id().to_string(),
        public_snapshot_id: head
            .current_decision
            .as_ref()
            .map(|decision| decision.public_snapshot_id.clone()),
        action,
    };
    validate_handoff_event_v2(&event, revision, &head.session_id)?;
    write_json_new_v1(
        &handoff_root.join(format!("handoff-{revision:020}.json")),
        &event,
        "run-play handoff event",
    )?;
    Ok(event)
}

pub fn build_overview_v1(
    repository_commit: String,
    repository_dirty: bool,
    workbench_sha256: String,
    run_root: &Path,
    focus_root: &Path,
    max_sessions: usize,
) -> Result<RunPlayOverviewV1, String> {
    let repository = repository_root_v1()?;
    let focus = load_focus_v1(focus_root)?;
    let mut alerts = Vec::new();
    let focused_path = match resolve_focus_session_path_v1(focus_root) {
        Ok(path) => path,
        Err(error) => {
            alerts.push(error);
            None
        }
    };
    let mut sessions = Vec::new();
    let mut unreadable_sessions = Vec::new();
    if run_root.is_dir() {
        for entry in fs::read_dir(run_root).map_err(|error| {
            format!(
                "failed to read run-play root {}: {error}",
                run_root.display()
            )
        })? {
            let entry =
                entry.map_err(|error| format!("failed to inspect run-play root: {error}"))?;
            if !entry
                .file_type()
                .map_err(|error| format!("failed to inspect run-play entry: {error}"))?
                .is_dir()
            {
                continue;
            }
            let path = entry.path();
            let focused = focused_path
                .as_ref()
                .is_some_and(|focus| same_path_v1(focus, &path));
            match catalog_entry_v1(&repository, &path, focused, &repository_commit) {
                Ok(entry) => sessions.push(entry),
                Err(error) => unreadable_sessions.push(RunPlayUnreadableSessionV1 {
                    path: repository_relative_text_v1(&repository, &path)
                        .unwrap_or_else(|_| path.display().to_string()),
                    error,
                }),
            }
        }
    }
    sessions.sort_by(|left, right| {
        right
            .last_activity_unix_ms
            .cmp(&left.last_activity_unix_ms)
            .then_with(|| left.path.cmp(&right.path))
    });
    sessions.truncate(max_sessions.max(1));

    let focused = if let Some(path) = focused_path {
        let paths = RunPlayPathsV1::new(&path);
        let head = paths.load_head()?;
        if focus.session_id.as_deref() != Some(head.session_id.as_str()) {
            alerts.push("focus session identity no longer matches its path".to_string());
        }
        let catalog = catalog_entry_v1(&repository, &path, true, &repository_commit)?;
        let handoff = load_handoff_summary_v1(&paths, Some(&repository_commit))?;
        if let Some(open) = handoff.open.as_ref() {
            if !open.matches_current_head || !open.matches_repository_commit {
                alerts.push(format!(
                    "open handoff r{} is historical for the current head or repository commit",
                    open.handoff_revision
                ));
            }
        }
        Some(RunPlayFocusedSessionV1 {
            catalog,
            status: render_status_v1(&head)?,
            current_decision: head.current_decision,
            handoff,
        })
    } else {
        alerts.push("no run-play session is focused".to_string());
        None
    };
    Ok(RunPlayOverviewV1 {
        schema: OVERVIEW_SCHEMA_V1.to_string(),
        repository_commit,
        repository_dirty,
        workbench_sha256,
        focus,
        focused,
        sessions,
        unreadable_sessions,
        alerts,
    })
}

fn catalog_entry_v1(
    repository: &Path,
    path: &Path,
    focused: bool,
    repository_commit: &str,
) -> Result<RunPlaySessionCatalogEntryV1, String> {
    let paths = RunPlayPathsV1::new(path);
    let head = paths.load_head()?;
    let control = load_session_control_v1(&paths)?;
    let handoff = load_handoff_summary_v1(&paths, Some(repository_commit))?;
    let current_handoff = handoff
        .open
        .as_ref()
        .is_some_and(|open| open.matches_current_head && open.matches_repository_commit);
    let (decision_site, target_depth, public_snapshot_id) =
        head.current_decision
            .as_ref()
            .map_or((None, None, None), |decision| {
                (
                    Some(decision.site),
                    Some(decision.target_archive_depth),
                    Some(decision.public_snapshot_id.clone()),
                )
            });
    Ok(RunPlaySessionCatalogEntryV1 {
        path: repository_relative_text_v1(repository, path)?,
        session_id: head.session_id,
        head_revision: head.head_revision,
        state_revision: head.state_revision,
        archive_node_id: head.position.archive_node_id().to_string(),
        terminal: matches!(head.position, RunPlayPositionV1::Terminal { .. }),
        decision_site,
        target_depth,
        public_snapshot_id,
        lifecycle: control.lifecycle,
        purpose: control.purpose,
        label: control.label,
        parked_reason: control.parked_reason,
        last_activity_unix_ms: paths.latest_head_modified_unix_ms()?,
        focused,
        current_handoff,
    })
}

pub fn render_focus_v1(focus: &RunPlayFocusEventV1) -> String {
    format!(
        "focus revision={} session={} id={}\n",
        focus.focus_revision,
        focus.session_path.as_deref().unwrap_or("none"),
        focus.session_id.as_deref().unwrap_or("none")
    )
}

pub fn render_control_v1(control: &RunPlaySessionControlStateV1) -> String {
    format!(
        "session_control lifecycle={:?} purpose={:?} label={} reason={} revision={}\n",
        control.lifecycle,
        control.purpose,
        control.label.as_deref().unwrap_or("none"),
        control.parked_reason.as_deref().unwrap_or("none"),
        control
            .latest_control_revision
            .map_or_else(|| "none".to_string(), |revision| revision.to_string())
    )
}

pub fn render_handoff_summary_v1(summary: &RunPlayHandoffSummaryV1) -> String {
    let mut output = format!(
        "handoff historical={} latest={} open={}\n",
        summary.historical_event_count,
        summary
            .latest_event_revision
            .map_or_else(|| "none".to_string(), |revision| revision.to_string()),
        summary.open.as_ref().map_or_else(
            || "none".to_string(),
            |open| open.handoff_revision.to_string()
        )
    );
    if let Some(open) = summary.open.as_ref() {
        output.push_str(&format!(
            "handoff_scope head_match={} commit_match={} commit={} at=h{} state={} node={} snapshot={}\n",
            open.matches_current_head,
            open.matches_repository_commit,
            open.repository_commit,
            open.head_revision,
            open.state_revision,
            open.archive_node_id,
            open.public_snapshot_id.as_deref().unwrap_or("none")
        ));
        output.push_str(&format!("pause={}\n", open.pause_reason));
        output.push_str(&format!("do_not_act={}\n", open.do_not_act));
        output.push_str(&format!("open=[{}]\n", open.open_issues.join(" | ")));
        output.push_str(&format!(
            "completed=[{}]\n",
            open.completed_issues.join(" | ")
        ));
    }
    output
}

pub fn render_overview_v1(overview: &RunPlayOverviewV1) -> String {
    let mut output = format!(
        "overview commit={} dirty={} build=fresh sha256={}\n{}",
        overview.repository_commit,
        overview.repository_dirty,
        overview.workbench_sha256,
        render_focus_v1(&overview.focus)
    );
    if let Some(focused) = overview.focused.as_ref() {
        output.push_str(&format!(
            "focused lifecycle={:?} purpose={:?} path={}\n",
            focused.catalog.lifecycle, focused.catalog.purpose, focused.catalog.path
        ));
        output.push_str(&focused.status);
        if let Some(open) = focused.handoff.open.as_ref() {
            if open.matches_current_head && open.matches_repository_commit {
                output.push_str(&render_handoff_summary_v1(&focused.handoff));
            } else {
                output.push_str(&format!(
                    "handoff historical_open=r{} head_match={} commit_match={}\n",
                    open.handoff_revision,
                    open.matches_current_head,
                    open.matches_repository_commit
                ));
            }
        } else if focused.handoff.historical_event_count > 0 {
            output.push_str(&format!(
                "handoff historical={} open=none\n",
                focused.handoff.historical_event_count
            ));
        }
    }
    output.push_str(&format!(
        "sessions shown={} unreadable_ignored={}\n",
        overview.sessions.len(),
        overview.unreadable_sessions.len()
    ));
    for session in &overview.sessions {
        output.push_str(&format!(
            "  {}{:?}/{:?} h{} depth={} site={} {}\n",
            if session.focused { "* " } else { "  " },
            session.lifecycle,
            session.purpose,
            session.head_revision,
            session
                .target_depth
                .map_or_else(|| "terminal".to_string(), |depth| depth.to_string()),
            session
                .decision_site
                .map_or_else(|| "none".to_string(), |site| format!("{site:?}")),
            session.path
        ));
    }
    for alert in &overview.alerts {
        output.push_str(&format!("alert={alert}\n"));
    }
    output
}

fn validate_focus_event_v1(event: &RunPlayFocusEventV1, revision: u64) -> Result<(), String> {
    if event.schema != FOCUS_SCHEMA_V1 || event.focus_revision != revision {
        return Err("run-play focus event identity is unsupported".to_string());
    }
    if event.session_path.is_some() != event.session_id.is_some() {
        return Err("run-play focus path and session id disagree".to_string());
    }
    Ok(())
}

fn validate_control_event_v1(
    event: &RunPlaySessionControlEventV1,
    revision: u64,
    session_id: &str,
) -> Result<(), String> {
    if event.schema != SESSION_CONTROL_SCHEMA_V1
        || event.control_revision != revision
        || event.session_id != session_id
    {
        return Err("run-play session control identity is unsupported".to_string());
    }
    match &event.action {
        RunPlaySessionControlKindV1::Describe { label, .. } => {
            if let Some(label) = label.as_deref() {
                validate_short_text_v1("session label", label)?;
            }
        }
        RunPlaySessionControlKindV1::Park { reason }
        | RunPlaySessionControlKindV1::Unpark { reason } => {
            validate_short_text_v1("session lifecycle reason", reason)?;
        }
    }
    Ok(())
}

fn validate_handoff_event_v2(
    event: &RunPlayHandoffEventV2,
    revision: u64,
    session_id: &str,
) -> Result<(), String> {
    if event.schema != HANDOFF_EVENT_SCHEMA_V2
        || event.handoff_revision != revision
        || event.session_id != session_id
    {
        return Err("run-play V2 handoff identity is unsupported".to_string());
    }
    validate_short_text_v1("repository commit", &event.repository_commit)?;
    match &event.action {
        RunPlayHandoffActionV2::Open {
            pause_reason,
            do_not_act,
            open_issues,
            completed_issues,
        } => {
            validate_short_text_v1("pause reason", pause_reason)?;
            validate_short_text_v1("do-not-act boundary", do_not_act)?;
            validate_issue_lists_v1(open_issues, completed_issues)?;
        }
        RunPlayHandoffActionV2::Resolve { resolution, .. } => {
            validate_short_text_v1("handoff resolution", resolution)?;
        }
    }
    Ok(())
}

fn validate_issue_lists_v1(open: &[String], completed: &[String]) -> Result<(), String> {
    if open.len() > 16 || completed.len() > 16 {
        return Err("run-play handoff permits at most 16 open and completed issues".to_string());
    }
    for issue in open.iter().chain(completed) {
        validate_short_text_v1("handoff issue", issue)?;
    }
    Ok(())
}

fn validate_short_text_v1(description: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 512 {
        return Err(format!(
            "run-play {description} is empty or exceeds 512 bytes"
        ));
    }
    Ok(())
}

fn latest_revision_v1(root: &Path, prefix: &str, suffix: &str) -> Result<Option<u64>, String> {
    Ok(revision_paths_v1(root, prefix, suffix)?
        .last()
        .map(|(revision, _)| *revision))
}

fn revision_paths_v1(
    root: &Path,
    prefix: &str,
    suffix: &str,
) -> Result<Vec<(u64, PathBuf)>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut revisions = fs::read_dir(root)
        .map_err(|error| format!("failed to read revision root {}: {error}", root.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            parse_revision_v1(&entry.file_name().to_string_lossy(), prefix, suffix)
                .map(|revision| (revision, entry.path()))
        })
        .collect::<Vec<_>>();
    revisions.sort_by_key(|(revision, _)| *revision);
    Ok(revisions)
}

fn parse_revision_v1(name: &str, prefix: &str, suffix: &str) -> Option<u64> {
    name.strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse()
        .ok()
}

fn unix_ms_v1() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .map_err(|error| format!("system time precedes Unix epoch: {error}"))
}

fn system_time_ms_v1(time: SystemTime) -> Result<u64, String> {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .map_err(|error| format!("filesystem timestamp precedes Unix epoch: {error}"))
}

fn canonical_directory_v1(path: &Path, description: &str) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        format!(
            "failed to resolve {description} {}: {error}",
            path.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "{description} is not a directory: {}",
            path.display()
        ));
    }
    Ok(canonical)
}

fn repository_relative_text_v1(repository: &Path, path: &Path) -> Result<String, String> {
    let repository = fs::canonicalize(repository).map_err(|error| {
        format!(
            "failed to resolve repository {}: {error}",
            repository.display()
        )
    })?;
    let path = fs::canonicalize(path)
        .map_err(|error| format!("failed to resolve path {}: {error}", path.display()))?;
    let relative = path.strip_prefix(&repository).map_err(|_| {
        format!(
            "path {} is outside repository {}",
            path.display(),
            repository.display()
        )
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn same_path_v1(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn write_json_new_v1<T: Serialize>(
    path: &Path,
    value: &T,
    description: &str,
) -> Result<(), String> {
    let mut payload = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {description}: {error}"))?;
    payload.push(b'\n');
    write_new_synced(path, &payload, description)
}

impl RunPlayPathsV1 {
    pub fn latest_head_modified_unix_ms(&self) -> Result<u64, String> {
        let path = self.latest_head_path_for_control_v1()?;
        let modified = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                format!(
                    "failed to inspect run-play head {}: {error}",
                    path.display()
                )
            })?;
        system_time_ms_v1(modified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RunPlayHeadV1, RunPlayPositionV1, RUN_PLAY_HEAD_SCHEMA_V1};
    use sts_oracle_learning::agent::task::{GameOutcomeV1, TaskBudgetV1, TaskEpisodeOutcomeV1};
    use sts_oracle_learning::eval::run_control::CombatResearchPlanOptionsV1;

    #[test]
    fn focus_is_append_only_and_clearable() {
        let root = temporary_directory_v1("focus");
        let focus_root = root.join("focus");
        let first = append_focus_v1(
            &focus_root,
            Some(".oracle-lab/run-play/example".to_string()),
            Some("session-a".to_string()),
        )
        .expect("set focus");
        assert_eq!(first.focus_revision, 1);
        let cleared = clear_focus_v1(&focus_root).expect("clear focus");
        assert_eq!(cleared.focus_revision, 2);
        assert_eq!(load_focus_v1(&focus_root).expect("load focus"), cleared);
        assert_eq!(clear_focus_v1(&focus_root).expect("repeat clear"), cleared);
        fs::remove_dir_all(root).expect("cleanup focus fixture");
    }

    #[test]
    fn parked_session_rejects_publication_until_explicitly_unparked() {
        let root = temporary_directory_v1("park");
        let paths = terminal_session_fixture_v1(&root.join("session"));
        let head = paths.load_head().expect("load fixture head");
        let described = describe_session_v1(
            &paths,
            RunPlaySessionPurposeV1::Play,
            Some("fixture".to_string()),
        )
        .expect("describe session");
        assert_eq!(
            describe_session_v1(
                &paths,
                RunPlaySessionPurposeV1::Play,
                Some("fixture".to_string())
            )
            .expect("repeat describe"),
            described
        );
        park_session_v1(&paths, "retain prior run".to_string()).expect("park session");

        let mut next = head.clone();
        next.head_revision = 1;
        let error = paths
            .publish(head.head_revision, &next, None, None)
            .expect_err("parked publication must fail");
        assert!(error.contains("session is parked"));
        assert_eq!(paths.load_head().expect("unchanged head").head_revision, 0);

        unpark_session_v1(&paths, "continue deliberately".to_string()).expect("unpark session");
        paths
            .publish(head.head_revision, &next, None, None)
            .expect("publication after unpark");
        assert_eq!(paths.load_head().expect("advanced head").head_revision, 1);
        fs::remove_dir_all(root).expect("cleanup park fixture");
    }

    #[test]
    fn handoff_v2_round_trips_resolves_and_treats_v1_as_history() {
        let root = temporary_directory_v1("handoff");
        let paths = terminal_session_fixture_v1(&root.join("session"));
        let handoffs = paths.root().join("handoffs");
        fs::create_dir_all(&handoffs).expect("handoff root");
        write_json_new_v1(
            &handoffs.join("handoff-00000000000000000000.json"),
            &serde_json::json!({"schema": "sts-run-play-handoff-receipt-v1"}),
            "legacy handoff fixture",
        )
        .expect("legacy handoff");

        let opened = open_handoff_v2(
            &paths,
            "commit-a".to_string(),
            "pause".to_string(),
            "do not advance".to_string(),
            vec!["open issue".to_string()],
            vec!["done issue".to_string()],
        )
        .expect("open V2 handoff");
        assert_eq!(opened.historical_event_count, 1);
        let current = opened.open.expect("open handoff");
        assert!(current.matches_current_head);
        assert!(current.matches_repository_commit);
        let repeated = open_handoff_v2(
            &paths,
            "commit-a".to_string(),
            "pause".to_string(),
            "do not advance".to_string(),
            vec!["open issue".to_string()],
            vec!["done issue".to_string()],
        )
        .expect("repeat identical V2 handoff");
        assert_eq!(repeated.latest_event_revision, Some(1));

        let other_commit = load_handoff_summary_v1(&paths, Some("commit-b"))
            .expect("load at another commit")
            .open
            .expect("open at another commit");
        assert!(!other_commit.matches_repository_commit);

        let resolved = resolve_handoff_v2(&paths, "commit-a".to_string(), "finished".to_string())
            .expect("resolve V2 handoff");
        assert!(resolved.open.is_none());
        assert_eq!(resolved.historical_event_count, 3);
        fs::remove_dir_all(root).expect("cleanup handoff fixture");
    }

    fn terminal_session_fixture_v1(session: &Path) -> RunPlayPathsV1 {
        let paths = RunPlayPathsV1::new(session);
        for child in ["heads", "states", "deltas"] {
            fs::create_dir_all(session.join(child)).expect("session fixture shape");
        }
        let head = RunPlayHeadV1 {
            schema: RUN_PLAY_HEAD_SCHEMA_V1.to_string(),
            scratch: false,
            scratch_parent_revision: None,
            scratch_floor_ends: Vec::new(),
            scratch_marks: Vec::new(),
            scratch_return_revision: None,
            evidence_identity: None,
            session_id: "session-control-fixture".to_string(),
            head_revision: 0,
            state_revision: 0,
            delta_revisions: Vec::new(),
            position: RunPlayPositionV1::Terminal {
                archive_node_id: "terminal".to_string(),
                outcome: TaskEpisodeOutcomeV1::Success {
                    game_outcome: GameOutcomeV1::ActThreeVictory,
                },
            },
            default_budget: TaskBudgetV1 {
                max_world_transitions: 1,
                max_reloads: 0,
            },
            combat_plan_options: CombatResearchPlanOptionsV1::default(),
            current_decision: None,
            current_path: Vec::new(),
            events: Vec::new(),
        };
        write_json_new_v1(
            &session.join("heads/head-00000000000000000000.json"),
            &head,
            "head fixture",
        )
        .expect("head fixture");
        fs::write(
            session.join("states/state-00000000000000000000.msgpack"),
            b"fixture",
        )
        .expect("state fixture");
        paths
    }

    fn temporary_directory_v1(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "sts-run-play-session-control-{label}-{}-{}",
            std::process::id(),
            unix_ms_v1().unwrap_or_default()
        ));
        fs::create_dir_all(&directory).expect("create session-control fixture");
        directory
    }
}
