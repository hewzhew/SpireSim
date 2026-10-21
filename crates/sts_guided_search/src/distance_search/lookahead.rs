//! Optional short checks within the resident graph; no separate search budget.
use super::*;
use std::cmp::Reverse;

pub(super) struct ShortLookahead {
    pub root: usize,
    pub stop_g: usize,
    pub queue: BinaryHeap<Reverse<(usize, usize)>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShortCheck {
    pub node: usize,
    pub inputs: usize,
    pub prediction: f64,
    /// Truncated factual bound, independent of the prediction.
    pub lower: u32,
    pub witnessed_remaining_inputs: Option<usize>,
}

impl DistanceSearch {
    /// Configure only before work starts. Zero preserves the ordinary scheduler.
    pub fn with_short_lookahead(mut self, inputs: usize) -> Result<Self, String> {
        if self.stats.work() != 0 {
            return Err("configure short checks before search starts".into());
        }
        if inputs > 0 && self.probe_immediate {
            return Err("immediate probes and predicted short checks are separate modes".into());
        }
        self.lookahead_inputs = inputs;
        Ok(self)
    }

    pub fn short_checks(&self) -> &[ShortCheck] {
        &self.short_checks
    }

    pub fn short_check_pending(&self) -> bool {
        self.lookahead.is_some()
    }

    fn complete(&self, node: usize) -> bool {
        self.nodes[node]
            .choices
            .as_ref()
            .is_some_and(|c| self.nodes[node].cursor == c.len())
    }

    pub(super) fn required_lower(&self, mut node: usize) -> u32 {
        let g = self.nodes[node].g;
        let mut lower = self.nodes[node].lower;
        while let Some(parent) = self.nodes[node].parent {
            let distance = u32::try_from(g - self.nodes[parent].g).unwrap_or(u32::MAX);
            lower = lower.max(self.nodes[parent].lower.saturating_sub(distance));
            node = parent;
        }
        lower
    }

    pub(super) fn priority(&self, node: usize, estimate: f64) -> f64 {
        let lower = if self.lookahead_inputs == 0 && !self.probe_immediate && !self.factual_bounds {
            0
        } else {
            self.required_lower(node)
        };
        let remaining = estimate.max(f64::from(lower));
        match self.ordering {
            DistanceOrdering::TotalInputs => self.nodes[node].g as f64 + remaining,
            DistanceOrdering::RemainingInputs => remaining,
        }
    }

    pub(super) fn propagate_lower(&mut self, mut node: usize) {
        if self.lookahead_inputs == 0 && !self.probe_immediate && !self.factual_bounds {
            return;
        }
        loop {
            if !self.complete(node) {
                break;
            }
            let lower = self.nodes[node]
                .children
                .iter()
                .map(|j| {
                    self.nodes[*j]
                        .lower
                        .saturating_add(u32::from(self.nodes[*j].input.is_some()))
                })
                .min()
                .unwrap_or(0)
                .min(self.horizon + 1);
            if lower <= self.nodes[node].lower {
                break;
            }
            self.nodes[node].lower = lower;
            self.stats.lower_bound_updates += 1;
            match self.nodes[node].parent {
                Some(parent) => node = parent,
                None => break,
            }
        }
    }

    pub(super) fn start_short_check(&mut self, node: usize) -> Result<(), String> {
        let estimate = self.nodes[node].estimate.unwrap();
        if self.lookahead_inputs == 0
            || estimate > self.lookahead_inputs as f64
            || u64::from(self.required_lower(node)) > self.lookahead_inputs as u64
        {
            return Ok(());
        }
        let stop_g = self.nodes[node]
            .g
            .checked_add(self.lookahead_inputs)
            .ok_or("short check input limit overflow")?;
        // An unresolved stable decision needs a real input to end combat;
        // virtual selection only assembles that input.
        self.nodes[node].lower = self.nodes[node].lower.max(1);
        self.lookahead = Some(ShortLookahead {
            root: node,
            stop_g,
            queue: BinaryHeap::new(),
        });
        self.stats.short_checks_started += 1;
        Ok(())
    }

    pub(super) fn retain_short_child(&mut self, child: usize) {
        if let Some(check) = &mut self.lookahead {
            if self.nodes[child].terminal.is_none() && self.nodes[child].g < check.stop_g {
                check.queue.push(Reverse((self.nodes[child].g, child)));
            }
        }
    }

    /// Called only after the active source and its pending evaluations finish.
    pub(super) fn next_short_source(&mut self) -> Option<usize> {
        loop {
            let Reverse((_, node)) = match self.lookahead.as_mut()?.queue.pop() {
                Some(next) => next,
                None => {
                    let check = self.lookahead.take().unwrap();
                    self.short_checks.push(ShortCheck {
                        node: check.root,
                        inputs: check.stop_g - self.nodes[check.root].g,
                        prediction: self.nodes[check.root].estimate.unwrap(),
                        lower: self.nodes[check.root].lower,
                        witnessed_remaining_inputs: self.nodes[check.root].witness_upper,
                    });
                    self.stats.short_checks_completed += 1;
                    return None;
                }
            };
            if !self.complete(node) {
                return Some(node);
            }
        }
    }

    pub(super) fn frontier_entry(&mut self) -> Option<Entry> {
        while let Some(mut entry) = self.frontier.pop() {
            if self.nodes[entry.node].released {
                continue;
            }
            let current = self.priority(entry.node, self.nodes[entry.node].estimate.unwrap());
            if current > entry.f {
                entry.f = current;
                self.frontier.push(entry);
                continue;
            }
            return Some(entry);
        }
        None
    }
}
