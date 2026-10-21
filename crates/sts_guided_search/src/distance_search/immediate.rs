//! Check one real input before source estimation; retain successors for later.
use super::*;
use std::collections::VecDeque;

pub(super) struct ImmediateProbe {
    root: usize,
    pub(super) stop_g: usize,
    current: Option<usize>,
    pending: VecDeque<usize>,
    boundary: Vec<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImmediateCheck {
    pub node: usize,
    pub lower: u32,
    pub witnessed_remaining_inputs: Option<usize>,
    pub boundary_nodes: usize,
}

impl DistanceSearch {
    pub fn with_immediate_probe(mut self, enabled: bool) -> Result<Self, String> {
        if self.stats.work() != 0 {
            return Err("configure immediate probes before search starts".into());
        }
        if enabled && self.lookahead_inputs > 0 {
            return Err("immediate probes and predicted short checks are separate modes".into());
        }
        if !enabled && self.probe_forced_boundaries {
            return Err("forced probe boundaries require immediate probes".into());
        }
        self.probe_immediate = enabled;
        Ok(self)
    }

    /// Continue unique legal choices beyond a probe's first real input. Multiple
    /// choices still wait at a boundary; this is not another depth of branching.
    pub fn with_probe_forced_boundaries(mut self, enabled: bool) -> Result<Self, String> {
        if self.stats.work() != 0 {
            return Err("configure forced probe boundaries before search starts".into());
        }
        if enabled && (!self.probe_immediate || !self.advance_forced) {
            return Err(
                "forced probe boundaries require immediate probes and forced advancement".into(),
            );
        }
        if !enabled && self.estimate_probe_boundaries {
            return Err("boundary estimates require forced probe boundaries".into());
        }
        self.probe_forced_boundaries = enabled;
        Ok(self)
    }

    /// Queue prepared multi-choice boundaries by their own estimates. Only a
    /// selected boundary starts its next probe; admission does not expand it.
    pub fn with_probe_boundary_estimates(mut self, enabled: bool) -> Result<Self, String> {
        if self.stats.work() != 0 {
            return Err("configure boundary estimates before search starts".into());
        }
        if enabled && !self.probe_forced_boundaries {
            return Err("boundary estimates require forced probe boundaries".into());
        }
        self.estimate_probe_boundaries = enabled;
        Ok(self)
    }

    pub fn immediate_pending(&self) -> bool {
        self.immediate.is_some()
    }

    pub fn immediate_checks(&self) -> &[ImmediateCheck] {
        &self.immediate_checks
    }

    pub(super) fn start_immediate(&mut self, node: usize) -> Result<(), String> {
        let stop_g = self.nodes[node]
            .g
            .checked_add(1)
            .ok_or("immediate input limit overflow")?;
        self.nodes[node].lower = self.nodes[node].lower.max(1);
        self.immediate = Some(ImmediateProbe {
            root: node,
            stop_g,
            current: Some(node),
            pending: VecDeque::new(),
            boundary: vec![],
        });
        self.stats.immediate_checks_started += 1;
        Ok(())
    }

    pub(super) fn retain_immediate_child(&mut self, node: usize) {
        let probe = self.immediate.as_mut().unwrap();
        if self.nodes[node].g < probe.stop_g || self.probe_forced_boundaries {
            // Optional tails first classify each boundary against real choices.
            probe.pending.push_back(node);
        } else {
            // A real input is settled. Keep this exact boundary without probing
            // recursively, preparing it or issuing its estimate yet.
            probe.boundary.push(node);
        }
    }

    pub(super) fn immediate_step(
        &mut self,
        b: &DistanceBudget,
    ) -> Result<Option<DistanceStop>, String> {
        if let Some(index) = self.immediate.as_ref().unwrap().current {
            if self.nodes[index].choices.is_none() {
                self.preparing = Some(index);
            } else if self.probe_forced_boundaries
                && self.nodes[index].g >= self.immediate.as_ref().unwrap().stop_g
                && self.nodes[index].choices.as_ref().unwrap().len() > 1
            {
                // Prepared once, but not probed recursively. Queue ownership
                // is chosen when this probe completes.
                let probe = self.immediate.as_mut().unwrap();
                probe.boundary.push(index);
                probe.current = None;
            } else if self.nodes[index].cursor == self.nodes[index].choices.as_ref().unwrap().len()
            {
                self.propagate_lower(index);
                if index != self.immediate.as_ref().unwrap().root {
                    self.nodes[index].released = true;
                }
                self.immediate.as_mut().unwrap().current = None;
            } else if self.nodes[index].depth >= b.max_depth {
                return Ok(Some(DistanceStop::DepthLimit));
            } else if self.nodes.len() >= b.max_nodes {
                return Ok(Some(DistanceStop::NodeLimit));
            } else {
                return self.transition(index, b.step);
            }
            return Ok(None);
        }
        if let Some(index) = self.immediate.as_mut().unwrap().pending.pop_front() {
            self.immediate.as_mut().unwrap().current = Some(index);
            return Ok(None);
        }
        let probe = self.immediate.take().unwrap();
        let root = probe.root;
        self.propagate_lower(root);
        self.immediate_checks.push(ImmediateCheck {
            node: root,
            lower: self.nodes[root].lower,
            witnessed_remaining_inputs: self.nodes[root].witness_upper,
            boundary_nodes: probe.boundary.len(),
        });
        self.stats.immediate_checks_completed += 1;
        if self.estimate_probe_boundaries {
            // Every boundary already owns its actual choices and features.
            // Never route admission through `preparing`: that starts a probe.
            self.nodes[root].released = true;
            self.pending.extend(probe.boundary);
        } else if probe.boundary.is_empty() {
            self.nodes[root].released = true;
        } else {
            self.nodes[root].probe_boundary = Some(probe.boundary);
            // Estimation controls only further expansion, never this completed
            // one-input check. Partial probes cannot enter this queue.
            self.pending.push(root);
        }
        Ok(None)
    }
}
