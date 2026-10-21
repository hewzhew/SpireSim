use crate::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use crate::sim::combat_action::combat_action_key;
use crate::state::core::ClientInput;
use std::collections::{BTreeMap, BTreeSet};

use super::{OracleAnalysisSessionV1, OracleRunBoundaryV1};

#[path = "combat_scratch/delta.rs"]
mod delta;
#[path = "combat_scratch/search.rs"]
mod search;
#[cfg(test)]
#[path = "combat_scratch/tests.rs"]
mod tests;
#[path = "combat_scratch/types.rs"]
mod types;
#[path = "combat_scratch/view.rs"]
mod view;

pub use types::*;
use view::{
    action_surface_view, exact_hash, position_view, resolve_action_ref, resolve_action_selector,
    resolve_observation_local_selection,
};

#[derive(Clone)]
pub(super) struct OracleAnalysisCombatLineLabV1 {
    run_node_id: usize,
    context: OracleAnalysisCombatScratchContextV1,
    root_exact_state_hash: String,
    baseline_source: OracleAnalysisCombatLineLabBaselineSourceV1,
    baseline_scratch_node_ids: Vec<u64>,
    kept_scratch_node_id: Option<u64>,
    max_engine_steps_per_transition: usize,
    cursor_scratch_node_id: u64,
    next_scratch_node_id: u64,
    nodes: BTreeMap<u64, OracleAnalysisCombatScratchNodeCheckpointV1>,
    positions: BTreeMap<u64, CombatPosition>,
}

impl OracleAnalysisCombatLineLabV1 {
    pub(super) fn start(
        run_node_id: usize,
        context: OracleAnalysisCombatScratchContextV1,
        root: CombatPosition,
        max_engine_steps_per_transition: usize,
    ) -> Result<Self, String> {
        Self::start_with_baseline(
            run_node_id,
            context,
            root,
            OracleAnalysisCombatLineLabBaselineSourceV1::Root,
            &[],
            max_engine_steps_per_transition,
        )
    }

    pub(super) fn start_with_baseline(
        run_node_id: usize,
        context: OracleAnalysisCombatScratchContextV1,
        root: CombatPosition,
        baseline_source: OracleAnalysisCombatLineLabBaselineSourceV1,
        baseline_inputs: &[ClientInput],
        max_engine_steps_per_transition: usize,
    ) -> Result<Self, String> {
        if max_engine_steps_per_transition == 0 {
            return Err("combat scratch transition budget must be positive".to_string());
        }
        if EngineCombatStepper.terminal(&root) != CombatTerminal::Unresolved {
            return Err("combat scratch requires an unresolved combat root".to_string());
        }
        let root_exact_state_hash = exact_hash(&root);
        let root_node = OracleAnalysisCombatScratchNodeCheckpointV1 {
            scratch_node_id: 0,
            parent_scratch_node_id: None,
            input: None,
            exact_state_hash: root_exact_state_hash.clone(),
        };
        let mut scratch = Self {
            run_node_id,
            context,
            root_exact_state_hash,
            baseline_source,
            baseline_scratch_node_ids: vec![0],
            kept_scratch_node_id: None,
            max_engine_steps_per_transition,
            cursor_scratch_node_id: 0,
            next_scratch_node_id: 1,
            nodes: BTreeMap::from([(0, root_node)]),
            positions: BTreeMap::from([(0, root)]),
        };
        for input in baseline_inputs {
            let node_id = scratch.play_input(input.clone())?;
            scratch.baseline_scratch_node_ids.push(node_id);
        }
        if baseline_source == OracleAnalysisCombatLineLabBaselineSourceV1::ResidentIncumbent
            && EngineCombatStepper.terminal(scratch.current_position()?) != CombatTerminal::Win
        {
            return Err("resident incumbent baseline is not a terminal victory".to_string());
        }
        Ok(scratch)
    }

    pub(super) fn restore(
        checkpoint: OracleAnalysisCombatScratchCheckpointV1,
        context: OracleAnalysisCombatScratchContextV1,
        root: CombatPosition,
    ) -> Result<Self, String> {
        if checkpoint.schema_name != ORACLE_ANALYSIS_COMBAT_SCRATCH_SCHEMA_NAME
            || checkpoint.schema_version != ORACLE_ANALYSIS_COMBAT_SCRATCH_SCHEMA_VERSION
        {
            return Err("unsupported oracle analysis combat scratch schema".to_string());
        }
        if checkpoint.max_engine_steps_per_transition == 0 {
            return Err("combat scratch transition budget must be positive".to_string());
        }
        let actual_root_hash = exact_hash(&root);
        if checkpoint.root_exact_state_hash != actual_root_hash {
            return Err(format!(
                "combat scratch root changed: checkpoint {}, run node {}",
                checkpoint.root_exact_state_hash, actual_root_hash
            ));
        }
        let mut nodes = BTreeMap::new();
        for node in checkpoint.nodes {
            if nodes.insert(node.scratch_node_id, node).is_some() {
                return Err("combat scratch checkpoint contains duplicate node ids".to_string());
            }
        }
        let root_node = nodes
            .get(&0)
            .ok_or_else(|| "combat scratch checkpoint has no root node 0".to_string())?;
        if root_node.parent_scratch_node_id.is_some()
            || root_node.input.is_some()
            || root_node.exact_state_hash != actual_root_hash
        {
            return Err("combat scratch root node is not the bound exact root".to_string());
        }
        if nodes.values().any(|node| {
            node.scratch_node_id != 0
                && (node.parent_scratch_node_id.is_none() || node.input.is_none())
        }) {
            return Err("combat scratch non-root node is missing its delta".to_string());
        }
        if !nodes.contains_key(&checkpoint.cursor_scratch_node_id) {
            return Err(format!(
                "combat scratch cursor references missing node {}",
                checkpoint.cursor_scratch_node_id
            ));
        }
        if let Some(kept_node_id) = checkpoint.kept_scratch_node_id {
            if !nodes.contains_key(&kept_node_id) {
                return Err(format!(
                    "combat line lab kept line references missing node {kept_node_id}"
                ));
            }
        }
        let maximum_node_id = nodes.keys().next_back().copied().unwrap_or(0);
        if checkpoint.next_scratch_node_id <= maximum_node_id {
            return Err("combat scratch next node id is not above retained nodes".to_string());
        }
        if checkpoint.baseline_scratch_node_ids.first().copied() != Some(0) {
            return Err("combat line lab baseline does not start at root node 0".to_string());
        }
        for pair in checkpoint.baseline_scratch_node_ids.windows(2) {
            let [parent_id, child_id] = pair else {
                unreachable!("window length is fixed");
            };
            let child = nodes.get(child_id).ok_or_else(|| {
                format!("combat line lab baseline references missing node {child_id}")
            })?;
            if child.parent_scratch_node_id != Some(*parent_id) {
                return Err(format!(
                    "combat line lab baseline node {child_id} is not a child of {parent_id}"
                ));
            }
        }

        let mut positions = BTreeMap::from([(0, root)]);
        let mut pending = nodes
            .keys()
            .copied()
            .filter(|node_id| *node_id != 0)
            .collect::<BTreeSet<_>>();
        while !pending.is_empty() {
            let mut progressed = false;
            for node_id in pending.iter().copied().collect::<Vec<_>>() {
                let node = nodes
                    .get(&node_id)
                    .expect("pending scratch node comes from node map");
                let parent_id = node
                    .parent_scratch_node_id
                    .expect("non-root parent validated above");
                let Some(parent) = positions.get(&parent_id) else {
                    continue;
                };
                let input = node.input.clone().expect("non-root input validated above");
                let successor =
                    apply_exact(parent, input, checkpoint.max_engine_steps_per_transition)?;
                let actual = exact_hash(&successor);
                if node.exact_state_hash != actual {
                    return Err(format!(
                        "combat scratch node {node_id} exact hash changed: checkpoint {}, replay {actual}",
                        node.exact_state_hash
                    ));
                }
                positions.insert(node_id, successor);
                pending.remove(&node_id);
                progressed = true;
            }
            if !progressed {
                return Err(
                    "combat scratch checkpoint contains a cycle or missing parent".to_string(),
                );
            }
        }

        Ok(Self {
            run_node_id: checkpoint.run_node_id,
            context,
            root_exact_state_hash: checkpoint.root_exact_state_hash,
            baseline_source: checkpoint.baseline_source,
            baseline_scratch_node_ids: checkpoint.baseline_scratch_node_ids,
            kept_scratch_node_id: checkpoint.kept_scratch_node_id,
            max_engine_steps_per_transition: checkpoint.max_engine_steps_per_transition,
            cursor_scratch_node_id: checkpoint.cursor_scratch_node_id,
            next_scratch_node_id: checkpoint.next_scratch_node_id,
            nodes,
            positions,
        })
    }

    pub(super) fn checkpoint(&self) -> OracleAnalysisCombatScratchCheckpointV1 {
        OracleAnalysisCombatScratchCheckpointV1 {
            schema_name: ORACLE_ANALYSIS_COMBAT_SCRATCH_SCHEMA_NAME.to_string(),
            schema_version: ORACLE_ANALYSIS_COMBAT_SCRATCH_SCHEMA_VERSION,
            run_node_id: self.run_node_id,
            root_exact_state_hash: self.root_exact_state_hash.clone(),
            max_engine_steps_per_transition: self.max_engine_steps_per_transition,
            cursor_scratch_node_id: self.cursor_scratch_node_id,
            next_scratch_node_id: self.next_scratch_node_id,
            nodes: self.nodes.values().cloned().collect(),
            baseline_source: self.baseline_source,
            baseline_scratch_node_ids: self.baseline_scratch_node_ids.clone(),
            kept_scratch_node_id: self.kept_scratch_node_id,
        }
    }

    fn view(
        &self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        self.view_at(
            self.cursor_scratch_node_id,
            selection_offset,
            selection_limit,
        )
    }

    fn view_at(
        &self,
        scratch_node_id: u64,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        if selection_limit == 0 || selection_limit > 64 {
            return Err("combat scratch selection page limit must be in 1..=64".to_string());
        }
        let node = self
            .nodes
            .get(&scratch_node_id)
            .ok_or_else(|| format!("unknown combat scratch node {scratch_node_id}"))?;
        let position = self
            .positions
            .get(&scratch_node_id)
            .ok_or_else(|| format!("combat scratch node {scratch_node_id} has no position"))?;
        Ok(OracleAnalysisCombatScratchViewV1 {
            run_node_id: self.run_node_id,
            context: self.context.clone(),
            root_exact_state_hash: self.root_exact_state_hash.clone(),
            max_engine_steps_per_transition: self.max_engine_steps_per_transition,
            cursor_scratch_node_id: scratch_node_id,
            scratch_node_count: self.nodes.len(),
            parent_scratch_node_id: node.parent_scratch_node_id,
            input_from_parent: node.input.clone(),
            position: position_view(position),
            legal_actions: action_surface_view(
                position,
                scratch_node_id,
                selection_offset,
                selection_limit,
            )?,
        })
    }

    fn play(
        &mut self,
        action_ref: &str,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        let parent = self.current_position()?.clone();
        let input = resolve_action_ref(&parent, action_ref)?;
        self.play_input(input)?;
        self.view(selection_offset, selection_limit)
    }

    fn play_selector(
        &mut self,
        selector: OracleAnalysisCombatScratchActionSelectorV1,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        let mut candidate = self.clone();
        let source_node_id = selector.scratch_node_id();
        if !candidate.nodes.contains_key(&source_node_id) {
            return Err(format!("unknown combat scratch node {source_node_id}"));
        }
        candidate.cursor_scratch_node_id = source_node_id;
        let input = resolve_action_selector(candidate.current_position()?, selector)?;
        candidate.play_input(input)?;
        let view = candidate.view(selection_offset, selection_limit)?;
        *self = candidate;
        Ok(view)
    }

    fn play_current_input(
        &mut self,
        input: ClientInput,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        let mut candidate = self.clone();
        candidate.play_input(input)?;
        let view = candidate.view(selection_offset, selection_limit)?;
        *self = candidate;
        Ok(view)
    }

    fn play_input(&mut self, input: ClientInput) -> Result<u64, String> {
        let parent_id = self.cursor_scratch_node_id;
        let parent = self.current_position()?.clone();
        if EngineCombatStepper.terminal(&parent) != CombatTerminal::Unresolved {
            return Err(
                "combat scratch cursor is terminal and cannot accept another action".to_string(),
            );
        }
        let successor = apply_exact(&parent, input.clone(), self.max_engine_steps_per_transition)?;
        let successor_hash = exact_hash(&successor);

        if let Some(existing) = self.nodes.values().find(|node| {
            node.parent_scratch_node_id == Some(parent_id)
                && node.input.as_ref() == Some(&input)
                && node.exact_state_hash == successor_hash
        }) {
            self.cursor_scratch_node_id = existing.scratch_node_id;
            return Ok(existing.scratch_node_id);
        }

        let scratch_node_id = self.next_scratch_node_id;
        self.next_scratch_node_id = self.next_scratch_node_id.saturating_add(1);
        self.nodes.insert(
            scratch_node_id,
            OracleAnalysisCombatScratchNodeCheckpointV1 {
                scratch_node_id,
                parent_scratch_node_id: Some(parent_id),
                input: Some(input),
                exact_state_hash: successor_hash,
            },
        );
        self.positions.insert(scratch_node_id, successor);
        self.cursor_scratch_node_id = scratch_node_id;
        Ok(scratch_node_id)
    }

    fn append_inputs_atomically(
        &mut self,
        inputs: &[ClientInput],
    ) -> Result<(Option<u64>, Option<u64>), String> {
        let mut candidate = self.clone();
        let mut first = None;
        let mut last = None;
        for input in inputs {
            let node_id = candidate.play_input(input.clone())?;
            first.get_or_insert(node_id);
            last = Some(node_id);
        }
        *self = candidate;
        Ok((first, last))
    }

    fn focus(
        &mut self,
        scratch_node_id: u64,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        self.focus_cursor(scratch_node_id)?;
        self.view(selection_offset, selection_limit)
    }

    fn focus_receipt(
        &mut self,
        scratch_node_id: u64,
    ) -> Result<OracleAnalysisCombatScratchNavigationV1, String> {
        let source_scratch_node_id = self.cursor_scratch_node_id;
        self.focus_cursor(scratch_node_id)?;
        self.navigation_receipt(source_scratch_node_id)
    }

    fn focus_cursor(&mut self, scratch_node_id: u64) -> Result<(), String> {
        if !self.nodes.contains_key(&scratch_node_id) {
            return Err(format!("unknown combat scratch node {scratch_node_id}"));
        }
        self.cursor_scratch_node_id = scratch_node_id;
        Ok(())
    }

    fn back(
        &mut self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        self.back_cursor()?;
        self.view(selection_offset, selection_limit)
    }

    fn back_receipt(&mut self) -> Result<OracleAnalysisCombatScratchNavigationV1, String> {
        let source_scratch_node_id = self.cursor_scratch_node_id;
        self.back_cursor()?;
        self.navigation_receipt(source_scratch_node_id)
    }

    fn back_cursor(&mut self) -> Result<(), String> {
        let parent = self
            .current_node()?
            .parent_scratch_node_id
            .ok_or_else(|| "combat scratch cursor is already at its root".to_string())?;
        self.cursor_scratch_node_id = parent;
        Ok(())
    }

    fn navigation_receipt(
        &self,
        source_scratch_node_id: u64,
    ) -> Result<OracleAnalysisCombatScratchNavigationV1, String> {
        Ok(OracleAnalysisCombatScratchNavigationV1 {
            kind: ORACLE_ANALYSIS_COMBAT_SCRATCH_NAVIGATION_KIND.to_string(),
            run_node_id: self.run_node_id,
            source_scratch_node_id,
            cursor_scratch_node_id: self.cursor_scratch_node_id,
            scratch_node_count: self.nodes.len(),
            parent_scratch_node_id: self.current_node()?.parent_scratch_node_id,
        })
    }

    fn tree(&self) -> Result<OracleAnalysisCombatScratchTreeV1, String> {
        let mut views = Vec::with_capacity(self.nodes.len());
        for node in self.nodes.values() {
            let position = self.positions.get(&node.scratch_node_id).ok_or_else(|| {
                format!(
                    "combat scratch node {} has no replayed position",
                    node.scratch_node_id
                )
            })?;
            let action_key_from_parent = match (node.parent_scratch_node_id, node.input.as_ref()) {
                (Some(parent_id), Some(input)) => {
                    let parent = self.positions.get(&parent_id).ok_or_else(|| {
                        format!(
                            "combat scratch node {} has no parent position",
                            node.scratch_node_id
                        )
                    })?;
                    Some(combat_action_key(&parent.combat, input))
                }
                _ => None,
            };
            views.push(OracleAnalysisCombatScratchTreeNodeV1 {
                scratch_node_id: node.scratch_node_id,
                parent_scratch_node_id: node.parent_scratch_node_id,
                is_cursor: node.scratch_node_id == self.cursor_scratch_node_id,
                input_from_parent: node.input.clone(),
                action_key_from_parent,
                exact_state_hash: node.exact_state_hash.clone(),
                terminal: EngineCombatStepper.terminal(position),
                turn: position.combat.turn.turn_count,
                player_hp: position.combat.entities.player.current_hp,
                player_block: position.combat.entities.player.block,
                enemy_hp_total: position
                    .combat
                    .entities
                    .monsters
                    .iter()
                    .filter(|monster| monster.is_alive_for_action())
                    .map(|monster| monster.current_hp.max(0))
                    .sum(),
            });
        }
        Ok(OracleAnalysisCombatScratchTreeV1 {
            run_node_id: self.run_node_id,
            root_exact_state_hash: self.root_exact_state_hash.clone(),
            cursor_scratch_node_id: self.cursor_scratch_node_id,
            nodes: views,
        })
    }

    fn current_node(&self) -> Result<&OracleAnalysisCombatScratchNodeCheckpointV1, String> {
        self.nodes.get(&self.cursor_scratch_node_id).ok_or_else(|| {
            format!(
                "combat scratch cursor references missing node {}",
                self.cursor_scratch_node_id
            )
        })
    }

    fn current_position(&self) -> Result<&CombatPosition, String> {
        self.positions
            .get(&self.cursor_scratch_node_id)
            .ok_or_else(|| {
                format!(
                    "combat scratch cursor has no replayed position {}",
                    self.cursor_scratch_node_id
                )
            })
    }

    fn cursor_actions(&self) -> Result<Vec<ClientInput>, String> {
        let mut actions = Vec::new();
        let mut node_id = self.cursor_scratch_node_id;
        let mut visited = BTreeSet::new();
        while node_id != 0 {
            if !visited.insert(node_id) {
                return Err("combat scratch path contains a cycle".to_string());
            }
            let node = self
                .nodes
                .get(&node_id)
                .ok_or_else(|| format!("combat scratch path references missing node {node_id}"))?;
            actions.push(
                node.input
                    .clone()
                    .ok_or_else(|| format!("combat scratch node {node_id} has no input delta"))?,
            );
            node_id = node.parent_scratch_node_id.ok_or_else(|| {
                format!("combat scratch node {node_id} has no parent before root")
            })?;
        }
        actions.reverse();
        Ok(actions)
    }

    fn node_path(&self, terminal_node_id: u64) -> Result<Vec<u64>, String> {
        let mut nodes = Vec::new();
        let mut node_id = terminal_node_id;
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(node_id) {
                return Err("combat line lab path contains a cycle".to_string());
            }
            nodes.push(node_id);
            if node_id == 0 {
                break;
            }
            node_id = self
                .nodes
                .get(&node_id)
                .ok_or_else(|| format!("combat line lab path references missing node {node_id}"))?
                .parent_scratch_node_id
                .ok_or_else(|| {
                    format!("combat line lab node {node_id} has no parent before root")
                })?;
        }
        nodes.reverse();
        Ok(nodes)
    }

    fn cursor_node_path(&self) -> Result<Vec<u64>, String> {
        self.node_path(self.cursor_scratch_node_id)
    }

    fn location_at(
        &self,
        scratch_node_id: u64,
    ) -> Result<OracleAnalysisCombatLineLabLocationV1, String> {
        let path = self.node_path(scratch_node_id)?;
        let position = self
            .positions
            .get(&scratch_node_id)
            .ok_or_else(|| format!("combat line lab node {scratch_node_id} has no position"))?;
        let turn = position.combat.turn.turn_count;
        let action_in_turn = path
            .windows(2)
            .filter(|pair| {
                self.positions
                    .get(&pair[0])
                    .is_some_and(|source| source.combat.turn.turn_count == turn)
            })
            .count();
        let action_index = path.len().saturating_sub(1);
        Ok(OracleAnalysisCombatLineLabLocationV1 {
            action_index,
            turn,
            action_in_turn,
            on_baseline: self.baseline_scratch_node_ids.get(action_index).copied()
                == Some(scratch_node_id),
        })
    }

    fn frame(
        &self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabFrameV1, String> {
        let decision = OracleAnalysisCombatScratchDecisionViewV1::from(
            self.view(selection_offset, selection_limit)?,
        );
        let location = self.location_at(self.cursor_scratch_node_id)?;
        let kept_line = self
            .kept_scratch_node_id
            .map(|node_id| self.node_path(node_id))
            .transpose()?
            .map(|path| self.line_summary(&path))
            .transpose()?;
        Ok(OracleAnalysisCombatLineLabFrameV1 {
            run_node_id: decision.run_node_id,
            context: decision.context,
            baseline_source: self.baseline_source,
            baseline_action_count: self.baseline_scratch_node_ids.len().saturating_sub(1),
            kept_line,
            location,
            terminal: decision.terminal,
            turn: decision.turn,
            phase: decision.phase,
            counters: decision.counters,
            player: decision.player,
            hand: decision.hand,
            draw_pile_top_first: decision.draw_pile_top_first,
            discard_pile: decision.discard_pile,
            exhaust_pile: decision.exhaust_pile,
            potions: decision.potions,
            monsters: decision.monsters,
            atomic_actions: decision.atomic_actions,
            selection_families: decision.selection_families,
        })
    }

    fn goto_line(
        &mut self,
        line: OracleAnalysisCombatLineLabLineV1,
        turn: u32,
        before_action: usize,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabFrameV1, String> {
        let path = match line {
            OracleAnalysisCombatLineLabLineV1::Baseline => self.baseline_scratch_node_ids.clone(),
            OracleAnalysisCombatLineLabLineV1::Current => self.cursor_node_path()?,
        };
        let matching = path
            .windows(2)
            .filter_map(|pair| {
                self.positions
                    .get(&pair[0])
                    .filter(|position| position.combat.turn.turn_count == turn)
                    .map(|_| pair[0])
            })
            .collect::<Vec<_>>();
        let node_id = matching.get(before_action).copied().ok_or_else(|| {
            format!(
                "combat line lab {line:?} line turn {turn} has {} actions; cannot select before action {before_action}",
                matching.len()
            )
        })?;
        if node_id != self.cursor_scratch_node_id {
            self.keep_non_baseline_win_before_navigation()?;
        }
        self.cursor_scratch_node_id = node_id;
        self.frame(selection_offset, selection_limit)
    }

    fn back_frame(
        &mut self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabFrameV1, String> {
        let parent = self
            .current_node()?
            .parent_scratch_node_id
            .ok_or_else(|| "combat scratch cursor is already at its root".to_string())?;
        self.keep_non_baseline_win_before_navigation()?;
        self.cursor_scratch_node_id = parent;
        self.frame(selection_offset, selection_limit)
    }

    fn keep_non_baseline_win_before_navigation(&mut self) -> Result<(), String> {
        if self.kept_scratch_node_id.is_some()
            || EngineCombatStepper.terminal(self.current_position()?) != CombatTerminal::Win
        {
            return Ok(());
        }
        if self.cursor_node_path()? != self.baseline_scratch_node_ids {
            self.kept_scratch_node_id = Some(self.cursor_scratch_node_id);
        }
        Ok(())
    }

    fn keep_current_line(&mut self) -> Result<OracleAnalysisCombatLineLabLineSummaryV1, String> {
        let path = self.cursor_node_path()?;
        let summary = self.line_summary(&path)?;
        self.kept_scratch_node_id = Some(self.cursor_scratch_node_id);
        Ok(summary)
    }

    fn restore_kept_line(
        &mut self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabFrameV1, String> {
        self.cursor_scratch_node_id = self.kept_scratch_node_id.ok_or_else(|| {
            "combat line lab has no kept line; use lab keep before replacing the current line"
                .to_string()
        })?;
        self.frame(selection_offset, selection_limit)
    }

    fn line_inputs(&self, path: &[u64]) -> Result<Vec<ClientInput>, String> {
        path.iter()
            .skip(1)
            .map(|node_id| {
                self.nodes
                    .get(node_id)
                    .ok_or_else(|| format!("combat line lab references missing node {node_id}"))?
                    .input
                    .clone()
                    .ok_or_else(|| format!("combat line lab node {node_id} has no input"))
            })
            .collect()
    }

    fn line_summary(
        &self,
        path: &[u64],
    ) -> Result<OracleAnalysisCombatLineLabLineSummaryV1, String> {
        let root_node_id = path.first().copied().unwrap_or(0);
        let root = self
            .positions
            .get(&root_node_id)
            .ok_or_else(|| "combat line lab line has no root position".to_string())?;
        let terminal_node_id = path.last().copied().unwrap_or(0);
        let terminal = self.positions.get(&terminal_node_id).ok_or_else(|| {
            format!("combat line lab terminal node {terminal_node_id} has no position")
        })?;
        let inputs = self.line_inputs(path)?;
        let mut turns = Vec::<OracleAnalysisCombatLineLabTurnSummaryV1>::new();
        for pair in path.windows(2) {
            let source = self.positions.get(&pair[0]).ok_or_else(|| {
                format!("combat line lab source node {} has no position", pair[0])
            })?;
            let successor = self.positions.get(&pair[1]).ok_or_else(|| {
                format!("combat line lab successor node {} has no position", pair[1])
            })?;
            let turn = source.combat.turn.turn_count;
            let enemy_hp_total = successor
                .combat
                .entities
                .monsters
                .iter()
                .filter(|monster| monster.is_alive_for_action())
                .map(|monster| monster.current_hp.max(0))
                .sum();
            if let Some(summary) = turns.last_mut().filter(|summary| summary.turn == turn) {
                summary.action_count = summary.action_count.saturating_add(1);
                summary.end_hp = successor.combat.entities.player.current_hp;
                summary.end_block = successor.combat.entities.player.block;
                summary.enemy_hp_total = enemy_hp_total;
            } else {
                turns.push(OracleAnalysisCombatLineLabTurnSummaryV1 {
                    turn,
                    action_count: 1,
                    start_hp: source.combat.entities.player.current_hp,
                    end_hp: successor.combat.entities.player.current_hp,
                    end_block: successor.combat.entities.player.block,
                    enemy_hp_total,
                });
            }
        }
        Ok(OracleAnalysisCombatLineLabLineSummaryV1 {
            terminal: EngineCombatStepper.terminal(terminal),
            suffix_known: EngineCombatStepper.terminal(terminal) != CombatTerminal::Unresolved,
            action_count: inputs.len(),
            initial_hp: root.combat.entities.player.current_hp,
            final_hp: terminal.combat.entities.player.current_hp,
            potions_used: inputs
                .iter()
                .filter(|input| {
                    matches!(
                        input,
                        ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
                    )
                })
                .count(),
            turns,
        })
    }

    fn action_at(
        &self,
        path: &[u64],
        action_index: usize,
    ) -> Result<Option<OracleAnalysisCombatLineLabActionV1>, String> {
        let Some(pair) = path.get(action_index..=action_index.saturating_add(1)) else {
            return Ok(None);
        };
        if pair.len() != 2 {
            return Ok(None);
        }
        let position = self
            .positions
            .get(&pair[0])
            .ok_or_else(|| format!("combat line lab node {} has no position", pair[0]))?;
        let input = self
            .nodes
            .get(&pair[1])
            .and_then(|node| node.input.as_ref())
            .ok_or_else(|| format!("combat line lab node {} has no input", pair[1]))?;
        Ok(Some(combat_line_lab_action(position, input)))
    }

    fn action_summaries(
        &self,
        path: &[u64],
        start_action_index: usize,
    ) -> Result<Vec<OracleAnalysisCombatLineLabActionSummaryV1>, String> {
        let mut action_in_turn = BTreeMap::<u32, usize>::new();
        let mut summaries = Vec::new();
        for (action_index, pair) in path.windows(2).enumerate() {
            let source = self.positions.get(&pair[0]).ok_or_else(|| {
                format!("combat line lab source node {} has no position", pair[0])
            })?;
            let result = self.positions.get(&pair[1]).ok_or_else(|| {
                format!("combat line lab result node {} has no position", pair[1])
            })?;
            let input = self
                .nodes
                .get(&pair[1])
                .and_then(|node| node.input.as_ref())
                .ok_or_else(|| format!("combat line lab node {} has no input", pair[1]))?;
            let turn = source.combat.turn.turn_count;
            let ordinal = action_in_turn.entry(turn).or_default();
            let current_action_in_turn = *ordinal;
            *ordinal = ordinal.saturating_add(1);
            if action_index < start_action_index {
                continue;
            }
            summaries.push(OracleAnalysisCombatLineLabActionSummaryV1 {
                action_index,
                turn,
                action_in_turn: current_action_in_turn,
                action: combat_line_lab_action(source, input),
                result_hp: result.combat.entities.player.current_hp,
                result_block: result.combat.entities.player.block,
                result_enemy_hp_total: result
                    .combat
                    .entities
                    .monsters
                    .iter()
                    .filter(|monster| monster.is_alive_for_action())
                    .map(|monster| monster.current_hp.max(0))
                    .sum(),
            });
        }
        Ok(summaries)
    }

    fn compare(&self) -> Result<OracleAnalysisCombatLineLabCompareV1, String> {
        let current_path = self.cursor_node_path()?;
        let baseline_inputs = self.line_inputs(&self.baseline_scratch_node_ids)?;
        let current_inputs = self.line_inputs(&current_path)?;
        let common_prefix_actions = baseline_inputs
            .iter()
            .zip(&current_inputs)
            .take_while(|(baseline, current)| baseline == current)
            .count();
        let first_divergence = (common_prefix_actions < baseline_inputs.len()
            || common_prefix_actions < current_inputs.len())
        .then(|| {
            Ok::<_, String>(OracleAnalysisCombatLineLabDivergenceV1 {
                action_index: common_prefix_actions,
                baseline_action: self
                    .action_at(&self.baseline_scratch_node_ids, common_prefix_actions)?,
                current_action: self.action_at(&current_path, common_prefix_actions)?,
            })
        })
        .transpose()?;
        Ok(OracleAnalysisCombatLineLabCompareV1 {
            run_node_id: self.run_node_id,
            baseline_source: self.baseline_source,
            common_prefix_actions,
            first_divergence,
            baseline: self.line_summary(&self.baseline_scratch_node_ids)?,
            current: self.line_summary(&current_path)?,
            baseline_tail: self
                .action_summaries(&self.baseline_scratch_node_ids, common_prefix_actions)?,
            current_tail: self.action_summaries(&current_path, common_prefix_actions)?,
        })
    }
}

fn combat_line_lab_action(
    position: &CombatPosition,
    input: &ClientInput,
) -> OracleAnalysisCombatLineLabActionV1 {
    let target_index = |target: Option<usize>| {
        target.and_then(|entity_id| {
            position
                .combat
                .entities
                .monsters
                .iter()
                .position(|monster| monster.id == entity_id)
        })
    };
    match input {
        ClientInput::PlayCard { card_index, target } => position
            .combat
            .zones
            .hand
            .get(*card_index)
            .map(|card| OracleAnalysisCombatLineLabActionV1::PlayCard {
                card_id: card.id,
                upgrades: card.upgrades,
                hand_index: *card_index,
                target_index: target_index(*target),
            })
            .unwrap_or_else(|| OracleAnalysisCombatLineLabActionV1::Other {
                input: input.clone(),
            }),
        ClientInput::UsePotion {
            potion_index,
            target,
        } => position
            .combat
            .entities
            .potions
            .get(*potion_index)
            .and_then(Option::as_ref)
            .map(|potion| OracleAnalysisCombatLineLabActionV1::UsePotion {
                potion_id: potion.id,
                potion_slot: *potion_index,
                target_index: target_index(*target),
            })
            .unwrap_or_else(|| OracleAnalysisCombatLineLabActionV1::Other {
                input: input.clone(),
            }),
        ClientInput::DiscardPotion(potion_slot) => position
            .combat
            .entities
            .potions
            .get(*potion_slot)
            .and_then(Option::as_ref)
            .map(
                |potion| OracleAnalysisCombatLineLabActionV1::DiscardPotion {
                    potion_id: potion.id,
                    potion_slot: *potion_slot,
                },
            )
            .unwrap_or_else(|| OracleAnalysisCombatLineLabActionV1::Other {
                input: input.clone(),
            }),
        ClientInput::EndTurn => OracleAnalysisCombatLineLabActionV1::EndTurn,
        _ => OracleAnalysisCombatLineLabActionV1::Other {
            input: input.clone(),
        },
    }
}

impl OracleAnalysisSessionV1 {
    pub fn start_combat_scratch(
        &mut self,
        run_node_id: Option<usize>,
        max_engine_steps_per_transition: usize,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        if self.combat_scratch.is_some() {
            return Err(
                "oracle analysis workspace already has an active combat scratch".to_string(),
            );
        }
        let run_node_id = run_node_id.unwrap_or(self.cursor_node_id);
        let branch = self.require_branch(run_node_id)?;
        if branch.boundary != OracleRunBoundaryV1::Combat {
            return Err(format!(
                "oracle analysis node {run_node_id} is at {:?}, not combat",
                branch.boundary
            ));
        }
        let context = OracleAnalysisCombatScratchContextV1 {
            act: branch.session.run_state.act_num,
            floor: branch.session.run_state.floor_num,
            gold: branch.session.run_state.gold,
        };
        let root = branch.session.current_active_combat_position()?;
        let scratch = OracleAnalysisCombatLineLabV1::start(
            run_node_id,
            context,
            root,
            max_engine_steps_per_transition,
        )?;
        let view = scratch.view(selection_offset, selection_limit)?;
        self.combat_scratch = Some(scratch);
        Ok(view)
    }

    pub fn open_combat_line_lab(
        &mut self,
        run_node_id: Option<usize>,
        baseline_source: OracleAnalysisCombatLineLabBaselineSourceV1,
        max_engine_steps_per_transition: usize,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabOpenV1, String> {
        if self.combat_scratch.is_some() {
            return Err(
                "oracle analysis workspace already has an active combat line lab".to_string(),
            );
        }
        let run_node_id = run_node_id.unwrap_or(self.cursor_node_id);
        let (context, root) = {
            let branch = self.require_branch(run_node_id)?;
            if branch.boundary != OracleRunBoundaryV1::Combat {
                return Err(format!(
                    "oracle analysis node {run_node_id} is at {:?}, not combat",
                    branch.boundary
                ));
            }
            (
                OracleAnalysisCombatScratchContextV1 {
                    act: branch.session.run_state.act_num,
                    floor: branch.session.run_state.floor_num,
                    gold: branch.session.run_state.gold,
                },
                branch.session.current_active_combat_position()?,
            )
        };
        let baseline_inputs = match baseline_source {
            OracleAnalysisCombatLineLabBaselineSourceV1::Root => Vec::new(),
            OracleAnalysisCombatLineLabBaselineSourceV1::ResidentIncumbent => self
                .combat_jobs
                .get(&run_node_id)
                .and_then(|job| job.work.verified_witness_inputs())
                .ok_or_else(|| {
                    format!(
                        "oracle analysis node {run_node_id} has no verified resident combat incumbent"
                    )
                })?,
        };
        let scratch = OracleAnalysisCombatLineLabV1::start_with_baseline(
            run_node_id,
            context,
            root,
            baseline_source,
            &baseline_inputs,
            max_engine_steps_per_transition,
        )?;
        let baseline = scratch.line_summary(&scratch.baseline_scratch_node_ids)?;
        let frame = scratch.frame(selection_offset, selection_limit)?;
        self.combat_scratch = Some(scratch);
        Ok(OracleAnalysisCombatLineLabOpenV1 { baseline, frame })
    }

    pub fn combat_line_lab_frame(
        &self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabFrameV1, String> {
        self.combat_scratch
            .as_ref()
            .ok_or_else(|| "oracle analysis workspace has no active combat line lab".to_string())?
            .frame(selection_offset, selection_limit)
    }

    pub fn goto_combat_line_lab(
        &mut self,
        line: OracleAnalysisCombatLineLabLineV1,
        turn: u32,
        before_action: usize,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabFrameV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat line lab".to_string())?
            .goto_line(line, turn, before_action, selection_offset, selection_limit)
    }

    pub fn back_combat_line_lab(
        &mut self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabFrameV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat line lab".to_string())?
            .back_frame(selection_offset, selection_limit)
    }

    pub fn keep_combat_line_lab(
        &mut self,
    ) -> Result<OracleAnalysisCombatLineLabLineSummaryV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat line lab".to_string())?
            .keep_current_line()
    }

    pub fn restore_kept_combat_line_lab(
        &mut self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabFrameV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat line lab".to_string())?
            .restore_kept_line(selection_offset, selection_limit)
    }

    pub fn play_combat_line_lab_card(
        &mut self,
        card_id: crate::content::cards::CardId,
        occurrence: Option<usize>,
        target_index: Option<usize>,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabPlayCardResultV1, String> {
        let (source_node_id, from, candidates) = {
            let scratch = self.combat_scratch.as_ref().ok_or_else(|| {
                "oracle analysis workspace has no active combat line lab".to_string()
            })?;
            let decision = OracleAnalysisCombatScratchDecisionViewV1::from(
                scratch.view(selection_offset, selection_limit)?,
            );
            let candidates = decision
                .hand
                .iter()
                .filter(|card| card.card.id == card_id)
                .enumerate()
                .map(
                    |(occurrence, card)| OracleAnalysisCombatLineLabCardCandidateV1 {
                        occurrence,
                        hand_index: card.hand_index,
                        upgrades: card.card.upgrades,
                        effective_cost: card.card.effective_cost,
                        playable_without_target: card.playable_without_target,
                        playable_target_indices: card.playable_target_indices.clone(),
                    },
                )
                .collect::<Vec<_>>();
            (
                scratch.cursor_scratch_node_id,
                scratch.location_at(scratch.cursor_scratch_node_id)?,
                candidates,
            )
        };
        if candidates.is_empty() {
            return Err(format!(
                "combat line lab hand has no card with stable id {}",
                card_id.stable_id()
            ));
        }
        if occurrence.is_none() && candidates.len() > 1 {
            return Ok(OracleAnalysisCombatLineLabPlayCardResultV1::AmbiguousCard {
                card_id,
                candidates,
            });
        }
        let occurrence = occurrence.unwrap_or(0);
        let candidate = candidates.get(occurrence).ok_or_else(|| {
            format!(
                "combat line lab card {} has {} copies, not occurrence {occurrence}",
                card_id.stable_id(),
                candidates.len()
            )
        })?;
        let target_index = match target_index {
            Some(target_index) if candidate.playable_target_indices.contains(&target_index) => {
                Some(target_index)
            }
            Some(target_index) => {
                return Err(format!(
                    "combat line lab card {} cannot target local monster {target_index}",
                    card_id.stable_id()
                ))
            }
            None if candidate.playable_without_target => None,
            None if candidate.playable_target_indices.len() == 1 => {
                candidate.playable_target_indices.first().copied()
            }
            None if candidate.playable_target_indices.len() > 1 => {
                return Ok(
                    OracleAnalysisCombatLineLabPlayCardResultV1::AmbiguousTarget {
                        card_id,
                        occurrence,
                        playable_target_indices: candidate.playable_target_indices.clone(),
                    },
                )
            }
            None => {
                return Err(format!(
                    "combat line lab card {} has no legal play at the current frame",
                    card_id.stable_id()
                ))
            }
        };
        let selector = OracleAnalysisCombatScratchActionSelectorV1::HandCard {
            scratch_node_id: source_node_id,
            hand_index: candidate.hand_index,
            target_index,
        };
        let input = {
            let scratch = self
                .combat_scratch
                .as_ref()
                .expect("combat line lab remained active");
            resolve_action_selector(scratch.current_position()?, selector)?
        };
        let delta =
            self.play_combat_scratch_selector_delta(selector, selection_offset, selection_limit)?;
        let to = self
            .combat_scratch
            .as_ref()
            .expect("combat line lab remained active")
            .location_at(delta.cursor_scratch_node_id)?;
        Ok(OracleAnalysisCombatLineLabPlayCardResultV1::Played {
            input,
            delta: OracleAnalysisCombatLineLabDecisionDeltaV1::from_scratch(from, to, delta),
        })
    }

    pub fn use_combat_line_lab_potion(
        &mut self,
        potion_id: crate::content::potions::PotionId,
        occurrence: Option<usize>,
        target_index: Option<usize>,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabUsePotionResultV1, String> {
        let (source_node_id, from, candidates) = {
            let scratch = self.combat_scratch.as_ref().ok_or_else(|| {
                "oracle analysis workspace has no active combat line lab".to_string()
            })?;
            let decision = OracleAnalysisCombatScratchDecisionViewV1::from(
                scratch.view(selection_offset, selection_limit)?,
            );
            let candidates = decision
                .potions
                .iter()
                .filter(|potion| potion.id == potion_id)
                .enumerate()
                .map(
                    |(occurrence, potion)| OracleAnalysisCombatLineLabPotionCandidateV1 {
                        occurrence,
                        potion_slot: potion.potion_slot,
                        can_use: potion.can_use,
                        requires_target: potion.requires_target,
                        usable_without_target: potion.usable_without_target,
                        usable_target_indices: potion.usable_target_indices.clone(),
                    },
                )
                .collect::<Vec<_>>();
            (
                scratch.cursor_scratch_node_id,
                scratch.location_at(scratch.cursor_scratch_node_id)?,
                candidates,
            )
        };
        if candidates.is_empty() {
            return Err(format!(
                "combat line lab inventory has no potion with stable id {}",
                potion_id.stable_id()
            ));
        }
        if occurrence.is_none() && candidates.len() > 1 {
            return Ok(
                OracleAnalysisCombatLineLabUsePotionResultV1::AmbiguousPotion {
                    potion_id,
                    candidates,
                },
            );
        }
        let occurrence = occurrence.unwrap_or(0);
        let candidate = candidates.get(occurrence).ok_or_else(|| {
            format!(
                "combat line lab potion {} has {} copies, not occurrence {occurrence}",
                potion_id.stable_id(),
                candidates.len()
            )
        })?;
        let target_index = match target_index {
            Some(target_index) if candidate.usable_target_indices.contains(&target_index) => {
                Some(target_index)
            }
            Some(target_index) => {
                return Err(format!(
                    "combat line lab potion {} cannot target local monster {target_index}",
                    potion_id.stable_id()
                ))
            }
            None if candidate.usable_without_target => None,
            None if candidate.usable_target_indices.len() == 1 => {
                candidate.usable_target_indices.first().copied()
            }
            None if candidate.usable_target_indices.len() > 1 => {
                return Ok(
                    OracleAnalysisCombatLineLabUsePotionResultV1::AmbiguousTarget {
                        potion_id,
                        occurrence,
                        usable_target_indices: candidate.usable_target_indices.clone(),
                    },
                )
            }
            None => {
                return Err(format!(
                    "combat line lab potion {} has no legal use at the current frame",
                    potion_id.stable_id()
                ))
            }
        };
        let selector = OracleAnalysisCombatScratchActionSelectorV1::PotionSlot {
            scratch_node_id: source_node_id,
            potion_slot: candidate.potion_slot,
            target_index,
        };
        let input = {
            let scratch = self
                .combat_scratch
                .as_ref()
                .expect("combat line lab remained active");
            resolve_action_selector(scratch.current_position()?, selector)?
        };
        let delta =
            self.play_combat_scratch_selector_delta(selector, selection_offset, selection_limit)?;
        let to = self
            .combat_scratch
            .as_ref()
            .expect("combat line lab remained active")
            .location_at(delta.cursor_scratch_node_id)?;
        Ok(OracleAnalysisCombatLineLabUsePotionResultV1::Used {
            input,
            delta: OracleAnalysisCombatLineLabDecisionDeltaV1::from_scratch(from, to, delta),
        })
    }

    pub fn select_combat_line_lab(
        &mut self,
        selected_domain_indices: Vec<usize>,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabDecisionDeltaV1, String> {
        let (from, base, input) = {
            let scratch = self.combat_scratch.as_ref().ok_or_else(|| {
                "oracle analysis workspace has no active combat line lab".to_string()
            })?;
            let source_node_id = scratch.cursor_scratch_node_id;
            let position = scratch.current_position()?;
            (
                scratch.location_at(source_node_id)?,
                OracleAnalysisCombatScratchDecisionViewV1::from(scratch.view_at(
                    source_node_id,
                    selection_offset,
                    selection_limit,
                )?),
                resolve_observation_local_selection(position, &selected_domain_indices)?,
            )
        };
        let result = {
            let scratch = self
                .combat_scratch
                .as_mut()
                .expect("combat line lab remained active");
            scratch
                .play_current_input(input, selection_offset, selection_limit)
                .map(OracleAnalysisCombatScratchDecisionViewV1::from)?
        };
        let delta = OracleAnalysisCombatScratchDecisionDeltaV1::between(&base, &result)?;
        let to = self
            .combat_scratch
            .as_ref()
            .expect("combat line lab remained active")
            .location_at(delta.cursor_scratch_node_id)?;
        Ok(OracleAnalysisCombatLineLabDecisionDeltaV1::from_scratch(
            from, to, delta,
        ))
    }

    pub fn end_combat_line_lab_turn(
        &mut self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatLineLabDecisionDeltaV1, String> {
        let (source_node_id, from) = {
            let scratch = self.combat_scratch.as_ref().ok_or_else(|| {
                "oracle analysis workspace has no active combat line lab".to_string()
            })?;
            (
                scratch.cursor_scratch_node_id,
                scratch.location_at(scratch.cursor_scratch_node_id)?,
            )
        };
        let delta = self.play_combat_scratch_selector_delta(
            OracleAnalysisCombatScratchActionSelectorV1::EndTurn {
                scratch_node_id: source_node_id,
            },
            selection_offset,
            selection_limit,
        )?;
        let to = self
            .combat_scratch
            .as_ref()
            .expect("combat line lab remained active")
            .location_at(delta.cursor_scratch_node_id)?;
        Ok(OracleAnalysisCombatLineLabDecisionDeltaV1::from_scratch(
            from, to, delta,
        ))
    }

    pub fn compare_combat_line_lab(&self) -> Result<OracleAnalysisCombatLineLabCompareV1, String> {
        self.combat_scratch
            .as_ref()
            .ok_or_else(|| "oracle analysis workspace has no active combat line lab".to_string())?
            .compare()
    }

    pub fn combat_scratch_view(
        &self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        self.combat_scratch
            .as_ref()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .view(selection_offset, selection_limit)
    }

    pub fn combat_scratch_decision_view(
        &self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchDecisionViewV1, String> {
        self.combat_scratch_view(selection_offset, selection_limit)
            .map(Into::into)
    }

    pub fn combat_scratch_decision_view_at(
        &self,
        scratch_node_id: u64,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchDecisionViewV1, String> {
        self.combat_scratch
            .as_ref()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .view_at(scratch_node_id, selection_offset, selection_limit)
            .map(Into::into)
    }

    pub fn combat_scratch_cursor_node_id(&self) -> Result<u64, String> {
        self.combat_scratch
            .as_ref()
            .map(|scratch| scratch.cursor_scratch_node_id)
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())
    }

    pub fn play_combat_scratch_action(
        &mut self,
        action_ref: &str,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .play(action_ref, selection_offset, selection_limit)
    }

    pub fn play_combat_scratch_selector(
        &mut self,
        selector: OracleAnalysisCombatScratchActionSelectorV1,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .play_selector(selector, selection_offset, selection_limit)
    }

    pub fn play_combat_scratch_selector_delta(
        &mut self,
        selector: OracleAnalysisCombatScratchActionSelectorV1,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchDecisionDeltaV1, String> {
        let source_node_id = selector.scratch_node_id();
        let base = self.combat_scratch_decision_view_at(
            source_node_id,
            selection_offset,
            selection_limit,
        )?;
        let result = self
            .play_combat_scratch_selector(selector, selection_offset, selection_limit)
            .map(OracleAnalysisCombatScratchDecisionViewV1::from)?;
        OracleAnalysisCombatScratchDecisionDeltaV1::between(&base, &result)
    }

    pub fn focus_combat_scratch_node(
        &mut self,
        scratch_node_id: u64,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .focus(scratch_node_id, selection_offset, selection_limit)
    }

    pub fn focus_combat_scratch_node_receipt(
        &mut self,
        scratch_node_id: u64,
    ) -> Result<OracleAnalysisCombatScratchNavigationV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .focus_receipt(scratch_node_id)
    }

    pub fn back_combat_scratch(
        &mut self,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<OracleAnalysisCombatScratchViewV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .back(selection_offset, selection_limit)
    }

    pub fn back_combat_scratch_receipt(
        &mut self,
    ) -> Result<OracleAnalysisCombatScratchNavigationV1, String> {
        self.combat_scratch
            .as_mut()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .back_receipt()
    }

    pub fn combat_scratch_tree(&self) -> Result<OracleAnalysisCombatScratchTreeV1, String> {
        self.combat_scratch
            .as_ref()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?
            .tree()
    }

    pub fn clear_combat_scratch(&mut self) -> bool {
        self.combat_scratch.take().is_some()
    }

    pub fn commit_combat_scratch(&mut self) -> Result<usize, String> {
        let scratch = self
            .combat_scratch
            .as_ref()
            .ok_or_else(|| "oracle analysis workspace has no active combat scratch".to_string())?;
        if EngineCombatStepper.terminal(scratch.current_position()?) != CombatTerminal::Win {
            return Err("combat scratch cursor is not a terminal victory".to_string());
        }
        let source_node_id = scratch.run_node_id;
        let actions = scratch.cursor_actions()?;
        let child_node_id = self.accept_combat_actions_from_node(source_node_id, &actions)?;
        self.combat_scratch = None;
        Ok(child_node_id)
    }
}

fn apply_exact(
    position: &CombatPosition,
    input: ClientInput,
    max_engine_steps_per_transition: usize,
) -> Result<CombatPosition, String> {
    if !EngineCombatStepper.is_legal_action(position, &input) {
        return Err(format!(
            "combat scratch input is not legal at exact state {}",
            exact_hash(position)
        ));
    }
    let result = EngineCombatStepper.apply_to_stable(
        position,
        input,
        CombatStepLimits {
            max_engine_steps: max_engine_steps_per_transition,
            deadline: None,
        },
    );
    if result.timed_out || result.truncated {
        return Err(format!(
            "combat scratch transition did not reach a stable boundary: timed_out={} truncated={} engine_steps={} limit={max_engine_steps_per_transition}",
            result.timed_out, result.truncated, result.engine_steps
        ));
    }
    Ok(result.position)
}
