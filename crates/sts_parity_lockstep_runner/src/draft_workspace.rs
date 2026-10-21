//! Durable private choices. Only typed input is persisted; adapter states are
//! reconstructed from the proven natural source and current simulator code.
use crate::{
    manual_draft::require_same, manual_replay::adopted_scopes, ManualCombatDraftV1,
    ManualDraftSourceV1, ManualSessionV1,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use sts_parity_lockstep::{LockstepAdapterV1, ParitySnapshotV1, SemanticActionV1};
use sts_parity_lockstep_rust::RustLockstepAdapterV1;

#[path = "draft_branches.rs"]
mod branches;
use branches::BranchGraph;
pub use branches::{WorkspaceComparisonV1, WorkspaceNodeV1};
use std::{collections::BTreeMap, rc::Rc};
#[path = "draft_research.rs"]
mod research;
pub use research::{
    TurnEnemyHpLimitV1, TurnOutcomeV1, TurnPlayerPowerFilterV1, TurnResearchDetailV1,
    TurnResearchPageV1, TurnResearchRequestV1, TurnResearchStartV1,
};

struct CachedState {
    adapter: RustLockstepAdapterV1,
    snapshot: ParitySnapshotV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftWorker {
    package_version: String,
    source_revision: String,
    executable_sha256: String,
}

impl DraftWorker {
    pub(crate) fn capture(repository: &Path) -> Result<Self, String> {
        let revision = std::process::Command::new("git")
            .args(["describe", "--always", "--dirty", "--abbrev=40"])
            .current_dir(repository)
            .output()
            .map_err(|e| e.to_string())?;
        if !revision.status.success() {
            return Err("cannot record draft worker source".into());
        }
        Ok(Self {
            package_version: env!("CARGO_PKG_VERSION").into(),
            source_revision: String::from_utf8(revision.stdout)
                .map_err(|e| e.to_string())?
                .trim()
                .into(),
            executable_sha256: digest(
                &fs::read(std::env::current_exe().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            ),
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    source: ManualDraftSourceV1,
    worker: DraftWorker,
    root: ParitySnapshotV1,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    Choose { action: SemanticActionV1 },
    Undo,
    Certified { handle: String },
    Mark { name: String, node: usize },
    Jump { node: usize },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    index: usize,
    previous_digest: String,
    worker: DraftWorker,
    edit: Edit,
}

pub struct ManualDraftWorkspaceV1 {
    view_owner: String,
    directory: PathBuf,
    handle: String,
    source: ManualDraftSourceV1,
    worker: DraftWorker,
    _lease: File,
    states: Vec<Rc<CachedState>>,
    cache: BTreeMap<usize, Rc<CachedState>>,
    graph: BranchGraph,
    node_scope: String,
    selected: Vec<SemanticActionV1>,
    next_revision: usize,
    previous_digest: String,
    certified: Option<String>,
    replay_stop: Option<String>,
    research: BTreeMap<research::TurnResearchKey, research::TurnResearch>,
}

impl ManualDraftWorkspaceV1 {
    /// Search from the selected private prefix, then verify the complete line
    /// from its original source. Neither the workspace nor Java is advanced.
    pub fn plan_combat_completion(
        &self,
        source: &ManualDraftSourceV1,
        work: usize,
        wall_ms: u64,
        max_hp_loss: Option<u32>,
    ) -> Result<ManualCombatDraftV1, String> {
        if &self.source != source {
            return Err("workspace root does not match the selected manual source prefix".into());
        }
        if self.replay_stop.is_some() {
            return Err("selected draft has an unreplayable suffix".into());
        }
        let root = &self.states[0].adapter;
        root.validate_combat_draft_root_v1()?;
        let trial = &self.states.last().unwrap().adapter;
        let mut actions = self.selected.clone();
        actions.extend(plan_manual_combat_v1(trial, work, wall_ms, max_hp_loss)?);
        let terminal = root.verify_semantic_combat_victory_v1(&actions)?;
        Ok(ManualCombatDraftV1 {
            schema: "sts-manual-combat-draft-v1".into(),
            session_id: self.source.session_id.clone(),
            rust_version: self.worker.package_version.clone(),
            source: Some(self.source.clone()),
            root: self.states[0].snapshot.clone(),
            actions,
            terminal,
        })
    }

    /// Validate the entire explicit flip sequence before any durable edit.
    pub fn flip_match_cells(&mut self, cells: &[u8]) -> Result<(), String> {
        if cells.is_empty() || cells.len() > 10 {
            return Err("choose one to ten explicit cells".into());
        }
        let mut trial = self.states.last().unwrap().adapter.clone();
        let mut actions = Vec::new();
        for &cell in cells {
            let action = sts_parity_lockstep::natural_match_flip_v1(cell)?;
            validate_private_step(&self.states[0].adapter, &mut trial, &action)?;
            let before = trial.observe().map_err(|e| e.message)?;
            let prepared = trial.prepare(&action, &before).map_err(|e| e.message)?;
            trial.execute(prepared).map_err(|e| e.message)?;
            actions.push(action);
        }
        for action in actions {
            self.choose(action)?;
        }
        Ok(())
    }
    /// Compile the entire finite operation before editing; each acknowledged
    /// private step is then durable in this same workspace, including a prefix
    /// left by a storage failure. This never contacts Java.
    pub fn preview_rewards(&mut self, steps: Vec<crate::RewardPlanStepV1>) -> Result<(), String> {
        if self.states[0].adapter.session().active_combat.is_some() || self.replay_stop.is_some() {
            return Err("reward preview requires a valid reward workspace".into());
        }
        let plan = crate::ManualRewardPlanV1::compile(
            self.source.clone(),
            &self.states.last().unwrap().adapter,
            steps,
        )?;
        for action in plan.actions() {
            self.choose(action.clone())?;
        }
        Ok(())
    }

    pub fn action_view(&mut self) -> Result<sts_parity_lockstep::ActionViewV1, String> {
        let snapshot = self.snapshot()?;
        sts_parity_lockstep::ActionViewV1::new(
            &self.view_owner,
            self.next_revision as u64,
            &snapshot,
        )
    }

    pub fn choose_handle(&mut self, handle: &str) -> Result<(), String> {
        let view = self.action_view()?;
        self.choose(view.resolve_for(&view, handle)?)
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }
    pub fn selected_actions(&self) -> &[SemanticActionV1] {
        &self.selected
    }
    pub fn replay_stop(&self) -> Option<&str> {
        self.replay_stop.as_deref()
    }
    pub fn verified_actions(&self) -> usize {
        self.states.len() - 1
    }
    pub fn snapshot(&mut self) -> Result<ParitySnapshotV1, String> {
        Ok(self
            .states
            .last()
            .expect("workspace retains its root")
            .snapshot
            .clone())
    }

    pub fn decision_state(&self) -> Result<crate::DecisionStateV1, String> {
        let state = self.states.last().expect("workspace retains its root");
        crate::DecisionStateV1::capture(state.adapter.session(), &state.snapshot)
    }

    pub fn strategic_preview_stop(&self) -> Option<&'static str> {
        let root = &self.states[0].adapter;
        let current = &self.states.last().unwrap().adapter;
        if !is_strategic_root(root) || is_strategic_continuation(current) {
            return None;
        }
        Some(if current.session().active_combat.is_some() {
            "Preview complete: combat entry"
        } else {
            "Preview complete: next event or terminal"
        })
    }

    pub fn choose(&mut self, action: SemanticActionV1) -> Result<(), String> {
        if self.replay_stop.is_some() {
            return Err("undo the unreplayable suffix before choosing another action".into());
        }
        let mut next = self.states.last().unwrap().adapter.clone();
        validate_private_step(&self.states[0].adapter, &mut next, &action)?;
        let before = next.observe().map_err(|e| e.message)?;
        let prepared = next.prepare(&action, &before).map_err(|e| e.message)?;
        let snapshot = next.execute(prepared).map_err(|e| e.message)?;
        // An acknowledged choice is durable before the in-memory cursor moves.
        self.append(Edit::Choose {
            action: action.clone(),
        })?;
        self.selected.push(action);
        let cached = Rc::new(CachedState {
            adapter: next,
            snapshot,
        });
        self.cache.insert(self.graph.cursor, cached.clone());
        self.states.push(cached);
        self.certified = None;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<(), String> {
        if self.selected.is_empty() {
            return Ok(());
        }
        self.append(Edit::Undo)?;
        self.selected.pop();
        self.states.truncate(self.selected.len() + 1);
        if self.verified_actions() == self.selected.len() {
            self.replay_stop = None;
        }
        self.certified = None;
        Ok(())
    }

    pub fn certify(&mut self, session: &ManualSessionV1) -> Result<String, String> {
        if self.replay_stop.is_some() {
            return Err("selected draft has an unreplayable suffix".into());
        }
        let root = self.states[0].adapter.clone();
        if is_strategic_root(&root) {
            return Err(
                "strategic previews retain branch results; they do not export adoption plans"
                    .into(),
            );
        }
        if is_match_game(&root) {
            return Err(
                "Match and Keep previews are committed through the live cell choices".into(),
            );
        }
        if is_astrolabe_selection(&root) {
            return Err("Astrolabe previews are committed through the live card choices".into());
        }
        if root.session().active_combat.is_none() {
            self.source.selected_scope_index(&adopted_scopes(
                session.repository(),
                session,
                None,
            )?)?;
            let owner =
                ManualSessionV1::load_existing(session.repository(), &self.source.session_id)?;
            let plan = crate::ManualRewardPlanV1::from_selected(
                self.source.clone(),
                &root,
                &self.selected,
            )?;
            require_same(
                "reward workspace terminal",
                plan.terminal().ok_or("empty reward plan")?,
                &self.snapshot()?,
            )?;
            let (reference, handle, _) =
                crate::reward_plan::save_reward_plan_reference(&owner, &plan, self.worker.clone())?;
            // The operation server deduplicates request retries. A new explicit
            // certification creates a new proposal, as recorded by this edit.
            self.append(Edit::Certified {
                handle: reference.clone(),
            })?;
            self.certified = Some(reference);
            return Ok(handle);
        }
        let terminal = root.verify_semantic_combat_completion_v1(&self.selected)?;
        require_same("draft replay terminal", &terminal, &self.snapshot()?)?;
        // Save with the source owner even when the selected Java session has
        // subsequently been replaced by a native recovery descendant.
        self.source
            .selected_scope_index(&adopted_scopes(session.repository(), session, None)?)?;
        let owner = ManualSessionV1::load_existing(session.repository(), &self.source.session_id)?;
        let witness = ManualCombatDraftV1 {
            schema: "sts-manual-combat-draft-v1".into(),
            session_id: self.source.session_id.clone(),
            rust_version: self.worker.package_version.clone(),
            source: Some(self.source.clone()),
            root: self.states[0].snapshot.clone(),
            actions: self.selected.clone(),
            terminal,
        };
        if let Some(handle) = &self.certified {
            let saved = crate::load_manual_combat_draft_v1(&owner, Path::new(handle))?;
            if saved.source == witness.source
                && saved.actions == witness.actions
                && saved.validate_for_v1(&root).is_ok()
            {
                return crate::manual_draft::display_manual_draft_reference_v1(&owner, handle);
            }
        }
        let (reference, handle) =
            crate::manual_draft::save_manual_combat_draft_reference_v1(&owner, &witness)?;
        self.append(Edit::Certified {
            handle: reference.clone(),
        })?;
        self.certified = Some(reference);
        Ok(handle)
    }

    fn append(&mut self, edit: Edit) -> Result<(), String> {
        let mut graph = self.graph.clone();
        graph.apply(&edit, self.next_revision)?;
        let revision = Revision {
            index: self.next_revision,
            previous_digest: self.previous_digest.clone(),
            worker: self.worker.clone(),
            edit,
        };
        let bytes = serde_json::to_vec(&revision).map_err(|e| e.to_string())?;
        write_atomic_new(
            &self
                .directory
                .join(format!("{:08}.json", self.next_revision)),
            &bytes,
        )?;
        self.previous_digest = digest(&bytes);
        self.next_revision += 1;
        self.graph = graph;
        Ok(())
    }

    fn create(
        session: &ManualSessionV1,
        source: ManualDraftSourceV1,
        mut root: RustLockstepAdapterV1,
        worker: DraftWorker,
    ) -> Result<Self, String> {
        validate_private_root(&mut root)?;
        let parent = session.directory().join("draft-workspaces");
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let id = uuid::Uuid::new_v4().simple().to_string();
        let directory = parent.join(&id);
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let lease = lock_workspace(&directory)?;
        let manifest = Manifest {
            schema: workspace_schema(&root).into(),
            source: source.clone(),
            worker: worker.clone(),
            root: root.observe().map_err(|e| e.message)?,
        };
        let bytes = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        write_atomic_new(&directory.join("manifest.json"), &bytes)?;
        let files = workspace_files(session)?;
        let cached = Rc::new(CachedState {
            adapter: root,
            snapshot: manifest.root.clone(),
        });
        let node_scope = workspace_node_scope(&directory, &bytes)?;
        Ok(Self {
            view_owner: uuid::Uuid::new_v4().to_string(),
            handle: workspace_handle(&id, &files),
            directory,
            source,
            worker,
            _lease: lease,
            states: vec![cached.clone()],
            cache: BTreeMap::from([(0, cached)]),
            graph: BranchGraph::default(),
            node_scope,
            selected: vec![],
            next_revision: 0,
            previous_digest: digest(&bytes),
            certified: None,
            replay_stop: None,
            research: BTreeMap::new(),
        })
    }

    fn open(
        directory: PathBuf,
        handle: String,
        mut root: RustLockstepAdapterV1,
        worker: DraftWorker,
    ) -> Result<Self, String> {
        let lease = lock_workspace(&directory)?;
        let (manifest, selected, certified, count, hash, graph) = read_workspace(&directory)?;
        validate_private_root(&mut root)?;
        if manifest.schema != workspace_schema(&root) {
            return Err("workspace kind disagrees with its replayed source".into());
        }
        let root_snapshot = root.observe().map_err(|e| e.message)?;
        require_same("private draft source", &manifest.root, &root_snapshot)?;
        let node_scope = workspace_node_scope(
            &directory,
            &fs::read(directory.join("manifest.json")).map_err(|e| e.to_string())?,
        )?;
        let cached = Rc::new(CachedState {
            adapter: root,
            snapshot: root_snapshot,
        });
        let mut workspace = Self {
            view_owner: uuid::Uuid::new_v4().to_string(),
            directory,
            handle,
            source: manifest.source,
            worker,
            _lease: lease,
            states: vec![cached.clone()],
            cache: BTreeMap::from([(0, cached)]),
            graph,
            node_scope,
            selected,
            next_revision: count,
            previous_digest: hash,
            certified,
            replay_stop: None,
            research: BTreeMap::new(),
        };
        let projection = workspace.project_branch(workspace.graph.cursor)?;
        workspace.install_branch(projection);
        Ok(workspace)
    }
}

/// Use the same whole-combat search and settled resource facts as run-play.
/// The chosen complete witness is translated by the existing Rust adapter;
/// this function has no execution or journal authority.
pub fn plan_manual_combat_v1(
    adapter: &RustLockstepAdapterV1,
    work: usize,
    wall_ms: u64,
    max_hp_loss: Option<u32>,
) -> Result<Vec<SemanticActionV1>, String> {
    use sts_oracle_learning::eval::run_control::{
        run_control_session_whole_combat_portfolio_v1, CombatResearchPlanOptionsV1,
    };
    use sts_oracle_run_control::ai::combat_witness_contract::CombatWitnessSatisfactionV1;
    let portfolio = run_control_session_whole_combat_portfolio_v1(
        adapter.session(),
        CombatResearchPlanOptionsV1 {
            max_generation_work: work,
            wall_ms,
            satisfaction: max_hp_loss.map_or(
                CombatWitnessSatisfactionV1::FirstCompleteWin,
                CombatWitnessSatisfactionV1::HpLossAtMost,
            ),
            atomic_challenger: None,
            ..Default::default()
        },
    )?;
    let selected = portfolio
        .attempt
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.satisfies_requested_quality)
        .max_by_key(|(_, candidate)| {
            (
                candidate.outcome.final_hp,
                std::cmp::Reverse(candidate.outcome.total_potions_consumed),
                std::cmp::Reverse(candidate.plan.steps.len()),
            )
        });
    let Some((ordinal, _)) = selected else {
        return Err(format!(
            "whole-combat probe has no complete win meeting the resource request; candidates={}; best_final_hp={:?}; lanes={:?}; deepest_turn={:?}; result=Unknown",
            portfolio.attempt.candidates.len(),
            portfolio.attempt.candidates.iter().map(|c| c.outcome.final_hp).max(),
            portfolio.attempt.lanes.iter().map(|lane| (&lane.work_unit, lane.work_consumed, &lane.exit)).collect::<Vec<_>>(),
            portfolio.bounded_progress.iter().map(|p| p.max_player_turn).max(),
        ));
    };
    adapter
        .semantic_winning_combat_trajectory_from_inputs_v1(&portfolio.witness_input_lines[ordinal])
}

/// Opens one explicitly selected workspace, or resumes the sole unfinished
/// workspace at the current source root. Multiple choices are never guessed.
pub fn open_manual_draft_workspace_v1(
    session: &ManualSessionV1,
    reference: Option<&str>,
    observer: &mut dyn FnMut(crate::LockstepProgressV1),
) -> Result<ManualDraftWorkspaceV1, String> {
    open_manual_decision_workspace_v1(session, reference, None, observer)
}

/// An optional coordinate selects a verified prefix of the CURRENT source for
/// private research. It never moves the active trajectory or recovers Java.
pub fn open_manual_decision_workspace_v1(
    session: &ManualSessionV1,
    reference: Option<&str>,
    at: Option<usize>,
    observer: &mut dyn FnMut(crate::LockstepProgressV1),
) -> Result<ManualDraftWorkspaceV1, String> {
    if at.is_some() && reference.is_some() {
        return Err("select a workspace or a source coordinate".into());
    }
    let worker = DraftWorker::capture(session.repository())?;
    let files = workspace_files(session)?;
    if let Some(reference) = reference.filter(|r| *r != "new") {
        let (directory, id) = resolve_workspace_handle(reference, &files)?;
        let (manifest, ..) = read_workspace(directory)?;
        let source =
            ManualSessionV1::load_existing(session.repository(), &manifest.source.session_id)?;
        let ((_, root, _), _) = crate::manual_replay::replay_manual_scope_observed(
            source.repository(),
            &source,
            Some(manifest.source.committed_actions),
            None,
            observer,
        )?;
        return ManualDraftWorkspaceV1::open(
            directory.clone(),
            workspace_handle(id, &files),
            root,
            worker,
        );
    }
    let ((report, mut root, _), _) = crate::manual_replay::replay_manual_scope_observed(
        session.repository(),
        session,
        at,
        None,
        observer,
    )?;
    validate_private_root(&mut root)?;
    if reference.is_none() && at.is_none() {
        let snapshot = root.observe().map_err(|e| e.message)?;
        if let Some((directory, id)) = unfinished_workspace_at(session, &files, &snapshot)? {
            return ManualDraftWorkspaceV1::open(
                directory.clone(),
                workspace_handle(id, &files),
                root,
                worker,
            );
        }
    }
    ManualDraftWorkspaceV1::create(
        session,
        ManualDraftSourceV1 {
            session_id: session.session_id().into(),
            committed_actions: report.committed_actions,
        },
        root,
        worker,
    )
}

fn resolve_workspace_handle<'a>(
    reference: &str,
    files: &'a [(PathBuf, String)],
) -> Result<&'a (PathBuf, String), String> {
    let prefix = reference
        .strip_prefix('W')
        .filter(|id| (8..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("use a workspace handle W… or new")?
        .to_ascii_lowercase();
    let mut matches = files.iter().filter(|(_, id)| id.starts_with(&prefix));
    match (matches.next(), matches.next()) {
        (Some(found), None) => Ok(found),
        _ => Err("unknown or ambiguous workspace handle; use manual-draft list".into()),
    }
}

fn unfinished_workspace_at<'a>(
    session: &ManualSessionV1,
    files: &'a [(PathBuf, String)],
    snapshot: &ParitySnapshotV1,
) -> Result<Option<&'a (PathBuf, String)>, String> {
    let mut found = None;
    for entry in files {
        let (manifest, _, certified, ..) = read_workspace(&entry.0)?;
        // Snapshot equality alone cannot bind a workspace to a trajectory.
        if certified.is_none() && manifest.source.committed_suffix(session)?.is_empty() {
            require_same("workspace root", &manifest.root, snapshot)?;
            if found.is_some() {
                return Err(
                    "multiple unfinished drafts; use manual-draft list and select W…".into(),
                );
            }
            found = Some(entry);
        }
    }
    Ok(found)
}

/// Listing is read-only, including for a finished run. It grants no replay or adoption authority.
pub fn list_manual_draft_workspaces_v1(
    session: &ManualSessionV1,
) -> Result<Vec<(String, usize, bool)>, String> {
    let files = workspace_files(session)?;
    files
        .iter()
        .map(|(path, id)| {
            let (_, selected, certified, ..) = read_workspace(path)?;
            Ok((
                workspace_handle(id, &files),
                selected.len(),
                certified.is_some(),
            ))
        })
        .collect()
}

fn workspace_files(session: &ManualSessionV1) -> Result<Vec<(PathBuf, String)>, String> {
    let scopes = adopted_scopes(session.repository(), session, None)?;
    let mut result = Vec::new();
    for (owner, _) in &scopes {
        let entries = match fs::read_dir(owner.directory().join("draft-workspaces")) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if id.len() != 32
                || !id.bytes().all(|b| b.is_ascii_hexdigit())
                || !entry.file_type().map_err(|e| e.to_string())?.is_dir()
            {
                continue;
            }
            let path = entry.path();
            // A creation interrupted before publishing its manifest is not a workspace.
            if !path.join("manifest.json").exists() {
                continue;
            }
            let (manifest, ..) = read_workspace(&path)?;
            if manifest.source.session_id != owner.session_id() {
                return Err("workspace origin disagrees with its owner".into());
            }
            if manifest.source.selected_scope_index(&scopes).is_ok() {
                result.push((path, id));
            }
        }
    }
    result.sort();
    Ok(result)
}

fn workspace_handle(id: &str, files: &[(PathBuf, String)]) -> String {
    let length = (8..=32)
        .find(|n| {
            files
                .iter()
                .filter(|(_, other)| other.starts_with(&id[..*n]))
                .count()
                == 1
        })
        .unwrap_or(32);
    format!("W{}", &id[..length])
}

type WorkspaceContents = (
    Manifest,
    Vec<SemanticActionV1>,
    Option<String>,
    usize,
    String,
    BranchGraph,
);

fn read_workspace(directory: &Path) -> Result<WorkspaceContents, String> {
    let bytes = fs::read(directory.join("manifest.json")).map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !matches!(
        manifest.schema.as_str(),
        "sts-private-combat-workspace-v1"
            | "sts-private-reward-workspace-v1"
            | "sts-private-astrolabe-workspace-v1"
            | "sts-private-match-game-workspace-v1"
            | "sts-private-strategic-workspace-v1"
    ) {
        return Err("unsupported private workspace schema".into());
    }
    let mut previous = digest(&bytes);
    let mut records = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|p| {
            p.file_name().and_then(|v| v.to_str()).is_some_and(|n| {
                n.len() == 13
                    && n.ends_with(".json")
                    && n.as_bytes()[..8].iter().all(u8::is_ascii_digit)
            })
        })
        .collect::<Vec<_>>();
    records.sort();
    let mut graph = BranchGraph::default();
    let mut certified = None;
    for (index, path) in records.iter().enumerate() {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let record: Revision =
            serde_json::from_slice(&bytes).map_err(|e| format!("invalid draft revision: {e}"))?;
        if record.index != index
            || path.file_name().unwrap().to_string_lossy() != format!("{index:08}.json")
            || record.previous_digest != previous
        {
            return Err("private draft revision sequence or digest is broken".into());
        }
        graph.apply(&record.edit, index)?;
        match record.edit {
            Edit::Choose { .. } | Edit::Undo | Edit::Jump { .. } => certified = None,
            Edit::Certified { handle } => certified = Some(handle),
            Edit::Mark { .. } => {}
        }
        previous = digest(&bytes);
    }
    let selected = graph.actions(graph.cursor)?;
    Ok((
        manifest,
        selected,
        certified,
        records.len(),
        previous,
        graph,
    ))
}

fn lock_workspace(directory: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("writer.lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock_exclusive()
        .map_err(|_| "private draft is already open in another worker".to_owned())?;
    Ok(file)
}

fn validate_private_root(root: &mut RustLockstepAdapterV1) -> Result<(), String> {
    if root.session().active_combat.is_some() {
        return root.validate_combat_draft_root_v1();
    }
    let snapshot = root.observe().map_err(|e| e.message)?;
    if snapshot.boundary_kind == "natural_combat_reward"
        || is_astrolabe_selection(root)
        || is_match_game(root)
        || is_strategic_root(root)
    {
        return Ok(());
    }
    Err("private workspace requires combat, rewards, a strategic decision, Astrolabe or Match and Keep".into())
}

fn workspace_schema(root: &RustLockstepAdapterV1) -> &'static str {
    if root.session().active_combat.is_some() {
        "sts-private-combat-workspace-v1"
    } else if is_astrolabe_selection(root) {
        "sts-private-astrolabe-workspace-v1"
    } else if is_match_game(root) {
        "sts-private-match-game-workspace-v1"
    } else if is_strategic_root(root) {
        "sts-private-strategic-workspace-v1"
    } else {
        "sts-private-reward-workspace-v1"
    }
}

fn is_strategic_root(rust: &RustLockstepAdapterV1) -> bool {
    use sts_oracle_run_control::state::{
        core::EngineState, rewards::RewardScreenContext, selection::DomainEventSource,
    };
    if rust.session().active_combat.is_some() {
        return false;
    }
    match &rust.session().engine_state {
        EngineState::MapNavigation
        | EngineState::MapOverlay { .. }
        | EngineState::Shop(_)
        | EngineState::Campfire
        | EngineState::BossRelicSelect(_) => true,
        EngineState::RewardScreen(rewards) => {
            rewards.screen_context == RewardScreenContext::Standard
                && rewards.pending_card_choice.is_some()
        }
        EngineState::RunPendingChoice(choice) => choice.source == DomainEventSource::CampfireSmith,
        _ => false,
    }
}

fn is_strategic_continuation(rust: &RustLockstepAdapterV1) -> bool {
    use sts_oracle_run_control::state::core::EngineState;
    rust.session().active_combat.is_none()
        && (is_strategic_root(rust)
            || matches!(
                rust.session().engine_state,
                EngineState::RewardScreen(_)
                    | EngineState::TreasureRoom(_)
                    | EngineState::RunPendingChoice(_)
            ))
}

fn is_match_game(rust: &RustLockstepAdapterV1) -> bool {
    matches!(
        rust.session().engine_state,
        sts_oracle_run_control::state::core::EngineState::EventRoom
    ) && rust
        .session()
        .run_state
        .event_state
        .as_ref()
        .is_some_and(|event| {
            event.id == sts_oracle_run_control::state::events::EventId::MatchAndKeep
        })
}

fn is_astrolabe_selection(rust: &RustLockstepAdapterV1) -> bool {
    use sts_oracle_run_control::{
        content::relics::RelicId,
        state::{
            core::{EngineState, RunPendingChoiceReason},
            selection::DomainEventSource,
        },
    };
    matches!(&rust.session().engine_state, EngineState::RunPendingChoice(choice)
        if choice.source == DomainEventSource::Relic(RelicId::Astrolabe)
            && choice.reason == RunPendingChoiceReason::TransformUpgraded)
}

fn validate_private_step(
    root: &RustLockstepAdapterV1,
    current: &mut RustLockstepAdapterV1,
    action: &SemanticActionV1,
) -> Result<(), String> {
    if root.session().active_combat.is_some() {
        return current.validate_combat_draft_root_v1();
    }
    if is_strategic_root(root) {
        // The ordinary adapter owns legality, RNG and nested selections. Stop
        // before playing combat or selecting an event; this grants no adoption.
        return if is_strategic_continuation(current) {
            Ok(())
        } else {
            Err("strategic preview has reached its next combat, event or terminal".into())
        };
    }
    if is_match_game(root) {
        if is_match_game(current)
            && current
                .session()
                .run_state
                .event_state
                .as_ref()
                .unwrap()
                .current_screen
                < 3
            && matches!(action, SemanticActionV1::Select { surface: sts_parity_lockstep::DecisionSurfaceV1::Event, choice }
                if choice.kind == "match_card" || choice.kind == "event_option" && choice.id == "matchandkeep")
        {
            return Ok(());
        }
        return Err("Match and Keep preview stops after five attempts".into());
    }
    if is_astrolabe_selection(root) {
        // The adapter owns eligibility, ordered prefixes, RNG and transition.
        // This workspace grants only the remaining original grid clicks.
        if is_astrolabe_selection(current)
            && matches!(action, SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::GridSelect, choice
            } if choice.kind == "deck_card")
        {
            return Ok(());
        }
        return Err("Astrolabe preview stops after the three transformations".into());
    }
    validate_private_root(current)?;
    if matches!(action, SemanticActionV1::Select { surface: sts_parity_lockstep::DecisionSurfaceV1::CombatReward, choice }
        if choice.kind == "run_reward" && matches!(choice.id.as_str(), "gold" | "potion" | "card"))
    {
        return Ok(());
    }
    Err("reward workspace only takes explicit gold/potion rewards or opens cards; stop before card choice".into())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn workspace_node_scope(directory: &Path, manifest: &[u8]) -> Result<String, String> {
    let id = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("workspace has no identity")?;
    Ok(digest(format!("{id}:{}", digest(manifest)).as_bytes()))
}

pub(crate) fn write_atomic_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_file_name(format!("{}.part", uuid::Uuid::new_v4().simple()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    // Publishing a hard link is atomic and cannot replace a prior revision.
    // A crash before this point leaves only an ignored .part file.
    fs::hard_link(&temporary, path).map_err(|e| format!("draft save failed: {e}"))?;
    drop(file);
    let _ = fs::remove_file(&temporary);
    Ok(())
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;
