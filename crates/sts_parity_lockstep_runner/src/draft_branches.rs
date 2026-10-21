//! Branch structure is derived from the same append-only workspace edits.
//! Only process-local, currently replayed states are cached here.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) struct BranchGraph {
    nodes: BTreeMap<usize, (usize, SemanticActionV1)>,
    pub cursor: usize,
    marks: BTreeMap<String, usize>,
}

impl BranchGraph {
    pub fn path(&self, mut node: usize) -> Result<Vec<usize>, String> {
        let mut path = Vec::new();
        while node != 0 {
            let (parent, _) = self.nodes.get(&node).ok_or("unknown workspace node")?;
            path.push(node);
            node = *parent;
        }
        path.reverse();
        Ok(path)
    }

    pub fn actions(&self, node: usize) -> Result<Vec<SemanticActionV1>, String> {
        Ok(self
            .path(node)?
            .iter()
            .map(|id| self.nodes[id].1.clone())
            .collect())
    }

    pub fn apply(&mut self, edit: &Edit, revision: usize) -> Result<(), String> {
        match edit {
            Edit::Choose { action } => {
                let node = revision.checked_add(1).ok_or("node identity exhausted")?;
                if self.nodes.contains_key(&node) {
                    return Err("duplicate workspace node".into());
                }
                self.nodes.insert(node, (self.cursor, action.clone()));
                self.cursor = node;
            }
            Edit::Undo => {
                self.cursor = self
                    .nodes
                    .get(&self.cursor)
                    .ok_or("draft undo has no parent")?
                    .0
            }
            Edit::Jump { node } => {
                self.path(*node)?;
                self.cursor = *node;
            }
            Edit::Mark { name, node } => {
                validate_name(name)?;
                if *node != self.cursor {
                    return Err("bookmark must name the current node".into());
                }
                if self.marks.get(name).is_some_and(|old| old != node) {
                    return Err(
                        "bookmark already names another node; choose a different name".into(),
                    );
                }
                self.marks.insert(name.clone(), *node);
            }
            Edit::Certified { .. } => {}
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.trim() != name
        || name.is_empty()
        || name.chars().count() > 80
        || name.chars().any(char::is_control)
    {
        return Err("bookmark requires a nonempty name of at most 80 characters, without control characters or surrounding whitespace".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceNodeV1 {
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub actions: usize,
}

pub(super) struct BranchProjection {
    states: Vec<std::rc::Rc<CachedState>>,
    selected: Vec<SemanticActionV1>,
    replay_stop: Option<String>,
}

pub struct WorkspaceComparisonV1 {
    pub current: WorkspaceNodeV1,
    pub other: WorkspaceNodeV1,
    pub common_actions: usize,
    pub current_snapshot: ParitySnapshotV1,
    pub other_snapshot: ParitySnapshotV1,
    pub current_state: crate::DecisionStateV1,
    pub other_state: crate::DecisionStateV1,
}

impl ManualDraftWorkspaceV1 {
    fn node_info(&self, node: usize) -> Result<WorkspaceNodeV1, String> {
        Ok(WorkspaceNodeV1 {
            reference: format!("N{}:{node}", self.node_scope),
            name: self
                .graph
                .marks
                .iter()
                .find(|(_, id)| **id == node)
                .map(|(name, _)| name.clone()),
            actions: self.graph.path(node)?.len(),
        })
    }

    fn resolve_node(&self, reference: &str) -> Result<usize, String> {
        let node = reference
            .strip_prefix(&format!("N{}:", self.node_scope))
            .ok_or("node belongs to a different workspace source")?
            .parse::<usize>()
            .map_err(|_| "invalid workspace node")?;
        self.graph.path(node)?;
        Ok(node)
    }

    pub fn current_node(&self) -> Result<WorkspaceNodeV1, String> {
        self.node_info(self.graph.cursor)
    }

    pub fn bookmarks(&self) -> Result<Vec<WorkspaceNodeV1>, String> {
        self.graph
            .marks
            .iter()
            .map(|(name, node)| {
                let mut info = self.node_info(*node)?;
                info.name = Some(name.clone());
                Ok(info)
            })
            .collect()
    }

    pub fn mark(&mut self, name: &str) -> Result<(), String> {
        if self.replay_stop.is_some() {
            return Err("cannot mark an unverified suffix".into());
        }
        validate_name(name)?;
        if self.graph.marks.get(name) == Some(&self.graph.cursor) {
            return Ok(());
        }
        self.append(Edit::Mark {
            name: name.into(),
            node: self.graph.cursor,
        })
    }

    // Reuse each verified prefix node. A cold branch is replayed lazily; failure
    // leaves its saved intent intact and never turns the last good prefix into
    // a claim about the requested terminal.
    pub(super) fn project_branch(&mut self, node: usize) -> Result<BranchProjection, String> {
        let ids = self.graph.path(node)?;
        let selected = self.graph.actions(node)?;
        let mut states = vec![self.cache[&0].clone()];
        let mut replay_stop = None;
        for (index, id) in ids.iter().enumerate() {
            let cached = if let Some(state) = self.cache.get(id) {
                state.clone()
            } else {
                let mut next = states.last().unwrap().adapter.clone();
                let result = (|| {
                    validate_private_step(&states[0].adapter, &mut next, &selected[index])?;
                    let before = next.observe().map_err(|e| e.message)?;
                    let prepared = next
                        .prepare(&selected[index], &before)
                        .map_err(|e| e.message)?;
                    let snapshot = next.execute(prepared).map_err(|e| e.message)?;
                    Ok::<_, String>(std::rc::Rc::new(CachedState {
                        adapter: next,
                        snapshot,
                    }))
                })();
                match result {
                    Ok(state) => {
                        self.cache.insert(*id, state.clone());
                        state
                    }
                    Err(reason) => {
                        replay_stop = Some(format!(
                            "selected action {} cannot replay: {reason}; saved choices retained",
                            index + 1
                        ));
                        break;
                    }
                }
            };
            states.push(cached);
        }
        Ok(BranchProjection {
            states,
            selected,
            replay_stop,
        })
    }

    pub(super) fn install_branch(&mut self, projection: BranchProjection) {
        self.states = projection.states;
        self.selected = projection.selected;
        self.replay_stop = projection.replay_stop;
    }

    pub fn switch_to(&mut self, reference: &str) -> Result<(), String> {
        let node = self.resolve_node(reference)?;
        let projection = self.project_branch(node)?;
        // Always append, even for the same node: an explicit switch invalidates
        // old view objects. A failed publication leaves the current cursor alone.
        self.append(Edit::Jump { node })?;
        self.install_branch(projection);
        self.certified = None;
        Ok(())
    }

    pub fn compare_with(&mut self, reference: &str) -> Result<WorkspaceComparisonV1, String> {
        if self.replay_stop.is_some() {
            return Err("current branch has an unverified suffix".into());
        }
        let node = self.resolve_node(reference)?;
        let projection = self.project_branch(node)?;
        if let Some(reason) = projection.replay_stop {
            return Err(format!("comparison branch cannot replay: {reason}"));
        }
        let current_path = self.graph.path(self.graph.cursor)?;
        let other_path = self.graph.path(node)?;
        Ok(WorkspaceComparisonV1 {
            current: self.current_node()?,
            other: self.node_info(node)?,
            common_actions: current_path
                .iter()
                .zip(&other_path)
                .take_while(|(a, b)| a == b)
                .count(),
            current_snapshot: self.snapshot()?,
            other_snapshot: projection.states.last().unwrap().snapshot.clone(),
            current_state: self.decision_state()?,
            other_state: {
                let other = projection.states.last().unwrap();
                crate::DecisionStateV1::capture(other.adapter.session(), &other.snapshot)?
            },
        })
    }
}
