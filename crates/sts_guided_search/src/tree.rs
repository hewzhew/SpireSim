use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use sts_core::sim::combat::CombatStepLimits;

use crate::guidance::{
    GuidanceBudget, GuidanceCosts, GuidanceLedger, GuidanceOutcome, GuidanceReceipt,
    GuidanceTicket, TokenReservation, TokenUsage,
};

/// Complete, linear-size choices at this state. Virtual input-assembly states
/// may factor a much larger input language without executing a game action.
pub struct Choice<A> {
    pub action: A,
    pub exploration_weight: f64,
}

pub trait SearchSpace {
    type State: Clone + PartialEq;
    type Action: Clone;

    fn terminal(&self, state: &Self::State) -> Option<bool>;
    fn choices(&self, state: &Self::State) -> Result<Vec<Choice<Self::Action>>, String>;
    fn transition(
        &self,
        state: &Self::State,
        action: &Self::Action,
        limits: CombatStepLimits,
    ) -> Result<Transition<Self::State>, String>;
}

pub enum Transition<S> {
    Settled {
        state: S,
        engine_steps: usize,
    },
    Interrupted {
        engine_steps: usize,
        cause: TransitionInterruption,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionInterruption {
    Deadline,
    EngineStepLimit,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchConfig {
    pub exploration: f64,
    pub uniform_fraction: f64,
    pub neutral_value: f64,
    pub max_engine_steps_per_transition: usize,
    /// Pending nodes, including in-flight requests. Default one preserves
    /// single-source use; larger widths permit independent leaves to gather.
    pub max_pending_guidance: usize,
    /// Consecutive raw-policy expansions after a new leaf (and at the root).
    /// Zero retains single-edge PUCT. Requires a single pending leaf per tree.
    pub max_policy_rollout_steps: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            exploration: 1.5,
            uniform_fraction: 0.1,
            neutral_value: 0.5,
            max_engine_steps_per_transition: 256,
            max_pending_guidance: 1,
            max_policy_rollout_steps: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SearchBudget {
    pub max_services: usize,
    pub max_engine_steps: usize,
    pub max_nodes: usize,
    /// Includes virtual selection assembly; reaching it is a coverage gap.
    pub max_depth: usize,
    pub deadline: Option<Instant>,
    pub guidance: GuidanceBudget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    ServiceBudget,
    EngineBudget,
    NodeBudget,
    Deadline,
    NoOpenWork,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchError {
    InvalidConfiguration,
    InvalidSource,
    AlreadyGuided,
    InvalidGuidance,
    InvalidTransitionCost,
    DuplicateGuidanceSource,
    GuidanceInFlight,
    GuidanceCapacity,
    InvalidGuidanceTicket,
    GuidanceAlreadyFinished,
    GuidanceBudgetExceeded,
    GuidanceUsageOverrun,
    GuidanceExpired,
    SearchBudgetExhausted(StopReason),
    Domain(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeRef {
    session: u64,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepStatus {
    Progress,
    /// A successor already has guidance and can be addressed directly.
    Guided(NodeRef),
    NeedsGuidance(NodeRef),
    Terminal {
        node: NodeRef,
        won: bool,
    },
    Stopped(StopReason),
}

/// None means explicit neutral exploration, not an implicit fallback policy.
#[derive(Clone, Debug, Default)]
pub struct Guidance {
    pub weights: Option<Vec<f64>>,
    pub value: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SearchStats {
    pub services: usize,
    pub engine_steps: usize,
    pub guidance_replies: usize,
    pub terminal_observations: usize,
    pub coverage_gaps: usize,
    pub policy_rollout_services: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SearchTimings {
    pub elapsed: Duration,
    /// Active step calls, including transition time. Not additive with it.
    pub active_search: Duration,
    pub transitions: Duration,
    /// Wall intervals when pending guidance prevents further scheduling.
    pub blocked_on_guidance: Duration,
}

pub struct GuidanceWork {
    /// Unissued pending nodes; already in-flight requests are excluded.
    pub sources: Vec<NodeRef>,
    /// When set, sources remain inspectable but no new call is admitted.
    /// Empty sources without a stop means outstanding replies are needed.
    pub stopped: Option<StopReason>,
}

#[derive(Default)]
struct Estimate {
    visits: usize,
    samples: usize,
    sum: f64,
}

impl Estimate {
    fn mean(&self, neutral: f64) -> f64 {
        if self.samples == 0 {
            neutral
        } else {
            self.sum / self.samples as f64
        }
    }

    fn update(&mut self, service: bool, value: Option<f64>) {
        self.visits += usize::from(service);
        if let Some(value) = value {
            self.samples += 1;
            self.sum += value;
        }
    }
}

struct Edge<A> {
    action: A,
    base_prior: f64,
    prior: f64,
    estimate: Estimate,
    child: Option<usize>,
    blocked: bool,
}

struct Node<S, A> {
    state: S,
    parent: Option<(usize, usize)>,
    depth: usize,
    edges: Vec<Edge<A>>,
    guided: bool,
    /// Maximum raw guidance weight before exploration mixing; ties use the
    /// lowest choice ordinal. Direct-policy experiments follow only this edge.
    preferred_edge: Option<usize>,
    closed: bool,
    runnable: bool,
    terminal: Option<bool>,
    estimate: Estimate,
}

pub struct NodeView<'a, S, A> {
    pub state: &'a S,
    pub actions: Vec<&'a A>,
    pub visits: usize,
    pub evaluation_samples: usize,
    pub estimated_value: Option<f64>,
}

/// One tree, one immutable objective/domain, one fixed total allowance.
/// Pending leaves are unavailable for expansion until explicitly guided;
/// waiting never installs a neutral policy or a virtual losing sample.
pub struct SearchSession<D: SearchSpace> {
    id: u64,
    pub(crate) space: D,
    config: SearchConfig,
    budget: SearchBudget,
    nodes: Vec<Node<D::State, D::Action>>,
    pending: Vec<usize>,
    stats: SearchStats,
    guidance: GuidanceLedger,
    started: Instant,
    active_search: Duration,
    transitions: Duration,
    blocked_time: Duration,
    blocked_since: Option<Instant>,
    policy_rollout: Option<(usize, usize)>,
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

impl<D: SearchSpace> SearchSession<D> {
    pub fn new(
        space: D,
        root: D::State,
        config: SearchConfig,
        budget: SearchBudget,
    ) -> Result<Self, SearchError> {
        if !config.exploration.is_finite()
            || config.exploration <= 0.0
            || !config.uniform_fraction.is_finite()
            || !(0.0..=1.0).contains(&config.uniform_fraction)
            || config.uniform_fraction == 0.0
            || !config.neutral_value.is_finite()
            || !(0.0..=1.0).contains(&config.neutral_value)
            || config.max_engine_steps_per_transition == 0
            || config.max_pending_guidance == 0
            || (config.max_policy_rollout_steps > 0 && config.max_pending_guidance != 1)
            || budget.max_nodes == 0
        {
            return Err(SearchError::InvalidConfiguration);
        }
        let started = Instant::now();
        let root = Self::make_node(&space, root, None, 0)?;
        let pending = if root.closed { Vec::new() } else { vec![0] };
        let terminal_observations = usize::from(root.terminal.is_some());
        let id = NEXT_SESSION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| SearchError::InvalidConfiguration)?;
        let mut session = Self {
            id,
            space,
            config,
            budget,
            nodes: vec![root],
            pending,
            stats: SearchStats {
                terminal_observations,
                ..Default::default()
            },
            guidance: GuidanceLedger::new(budget.guidance),
            started,
            active_search: started.elapsed(),
            transitions: Duration::ZERO,
            blocked_time: Duration::ZERO,
            blocked_since: None,
            policy_rollout: (config.max_policy_rollout_steps > 0)
                .then_some((0, config.max_policy_rollout_steps)),
        };
        session.refresh_wait();
        Ok(session)
    }

    fn make_node(
        space: &D,
        state: D::State,
        parent: Option<(usize, usize)>,
        depth: usize,
    ) -> Result<Node<D::State, D::Action>, SearchError> {
        let terminal = space.terminal(&state);
        let choices = if terminal.is_some() {
            Vec::new()
        } else {
            space.choices(&state).map_err(SearchError::Domain)?
        };
        if terminal.is_none() && choices.is_empty() {
            return Err(SearchError::Domain(
                "unresolved state has no legal choices".into(),
            ));
        }
        let weights = choices
            .iter()
            .map(|c| c.exploration_weight)
            .collect::<Vec<_>>();
        let normalized = if choices.is_empty() {
            Vec::new()
        } else {
            normalize(&weights)
                .ok_or_else(|| SearchError::Domain("invalid exploration mass".into()))?
        };
        if normalized.iter().any(|p| *p <= 0.0) {
            return Err(SearchError::Domain(
                "legal choice has zero exploration mass".into(),
            ));
        }
        let edges = choices
            .into_iter()
            .zip(normalized)
            .map(|(c, p)| Edge {
                action: c.action,
                base_prior: p,
                prior: p,
                estimate: Estimate::default(),
                child: None,
                blocked: false,
            })
            .collect();
        Ok(Node {
            state,
            parent,
            depth,
            edges,
            guided: false,
            preferred_edge: None,
            closed: terminal.is_some(),
            runnable: false,
            terminal,
            estimate: Estimate::default(),
        })
    }

    fn reference(&self, index: usize) -> NodeRef {
        NodeRef {
            session: self.id,
            index,
        }
    }

    fn resolve(&self, source: NodeRef) -> Result<usize, SearchError> {
        if source.session != self.id || source.index >= self.nodes.len() {
            Err(SearchError::InvalidSource)
        } else {
            Ok(source.index)
        }
    }

    pub fn root(&self) -> NodeRef {
        self.reference(0)
    }
    pub fn stats(&self) -> SearchStats {
        self.stats
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// A trusted in-process view, not a permission-filtered model observation.
    pub fn view(&self, source: NodeRef) -> Result<NodeView<'_, D::State, D::Action>, SearchError> {
        let n = &self.nodes[self.resolve(source)?];
        Ok(NodeView {
            state: &n.state,
            actions: n.edges.iter().map(|e| &e.action).collect(),
            visits: n.estimate.visits,
            evaluation_samples: n.estimate.samples,
            estimated_value: (n.estimate.samples != 0)
                .then(|| n.estimate.mean(self.config.neutral_value)),
        })
    }

    /// Direct, local guidance (for an analyst or an in-process policy). Model
    /// transports must use begin/complete so failures and usage are accounted.
    pub fn submit_guidance(
        &mut self,
        source: NodeRef,
        guidance: Guidance,
    ) -> Result<(), SearchError> {
        if self.guidance.in_flight(source) {
            return Err(SearchError::GuidanceInFlight);
        }
        let prepared = self.prepare_guidance(source, guidance)?;
        self.install_guidance(prepared);
        self.refresh_wait();
        Ok(())
    }

    fn prepare_guidance(
        &self,
        source: NodeRef,
        guidance: Guidance,
    ) -> Result<(usize, Vec<f64>, Option<f64>, usize), SearchError> {
        let index = self.resolve(source)?;
        if self.nodes[index].guided {
            return Err(SearchError::AlreadyGuided);
        }
        if !self.pending.contains(&index) {
            return Err(SearchError::InvalidSource);
        }
        let (priors, preferred) = self.prepare_node_guidance(&self.nodes[index], &guidance)?;
        Ok((index, priors, guidance.value, preferred))
    }

    fn prepare_node_guidance(
        &self,
        node: &Node<D::State, D::Action>,
        guidance: &Guidance,
    ) -> Result<(Vec<f64>, usize), SearchError> {
        if guidance
            .value
            .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err(SearchError::InvalidGuidance);
        }
        let edges = &node.edges;
        let model = match &guidance.weights {
            Some(weights) if weights.len() == edges.len() => {
                normalize(weights).ok_or(SearchError::InvalidGuidance)?
            }
            Some(_) => return Err(SearchError::InvalidGuidance),
            None => edges.iter().map(|e| e.base_prior).collect(),
        };
        let preferred = model
            .iter()
            .enumerate()
            .max_by(|(ai, a), (bi, b)| a.total_cmp(b).then_with(|| bi.cmp(ai)))
            .map(|(i, _)| i)
            .ok_or(SearchError::InvalidGuidance)?;
        let priors = edges
            .iter()
            .zip(model)
            .map(|(edge, p)| {
                (1.0 - self.config.uniform_fraction) * p
                    + self.config.uniform_fraction * edge.base_prior
            })
            .collect::<Vec<_>>();
        if priors.iter().any(|p| !p.is_finite() || *p <= 0.0) {
            return Err(SearchError::InvalidGuidance);
        }
        Ok((priors, preferred))
    }

    fn install_guidance(
        &mut self,
        (index, priors, value, preferred): (usize, Vec<f64>, Option<f64>, usize),
    ) {
        self.stats.guidance_replies += 1;
        self.apply_guidance(index, priors, value, preferred);
    }

    fn apply_guidance(
        &mut self,
        index: usize,
        priors: Vec<f64>,
        value: Option<f64>,
        preferred: usize,
    ) {
        for (edge, p) in self.nodes[index].edges.iter_mut().zip(priors) {
            edge.prior = p;
        }
        self.nodes[index].guided = true;
        self.nodes[index].preferred_edge = Some(preferred);
        self.pending.retain(|i| *i != index);
        self.feedback(index, false, value);
        self.close_ancestors(index);
    }

    pub fn pending_guidance(&self) -> Vec<NodeRef> {
        self.pending
            .iter()
            .map(|i| self.reference(*i))
            .filter(|r| !self.guidance.in_flight(*r))
            .collect()
    }

    /// Advances only already-guided parts of the same tree until the frontier
    /// is full, all remaining work awaits guidance, or a fixed limit stops it.
    pub fn gather_guidance(&mut self) -> Result<GuidanceWork, SearchError> {
        loop {
            if let Some(reason) = self.stop_reason() {
                self.refresh_wait();
                return Ok(GuidanceWork {
                    sources: self.pending_guidance(),
                    stopped: Some(reason),
                });
            }
            if self.pending.len() >= self.config.max_pending_guidance || !self.nodes[0].runnable {
                self.refresh_wait();
                return Ok(GuidanceWork {
                    sources: self.pending_guidance(),
                    stopped: None,
                });
            }
            if let StepStatus::Stopped(reason) = self.step()? {
                return Ok(GuidanceWork {
                    sources: self.pending_guidance(),
                    stopped: Some(reason),
                });
            }
        }
    }

    /// Reserve one external attempt before dispatch. The ticket binds exactly
    /// these nodes. The caller owns transport and enforcement of token bounds.
    pub fn begin_guidance_batch(
        &mut self,
        sources: &[NodeRef],
        reservation: TokenReservation,
    ) -> Result<GuidanceTicket, SearchError> {
        if let Some(reason) = self.stop_reason() {
            return Err(SearchError::SearchBudgetExhausted(reason));
        }
        if sources.is_empty() {
            return Err(SearchError::InvalidGuidance);
        }
        for (offset, source) in sources.iter().enumerate() {
            let index = self.resolve(*source)?;
            if sources[..offset].contains(source) {
                return Err(SearchError::DuplicateGuidanceSource);
            }
            if !self.pending.contains(&index) {
                return Err(SearchError::InvalidSource);
            }
            if self.guidance.in_flight(*source) {
                return Err(SearchError::GuidanceInFlight);
            }
        }
        self.guidance
            .begin(self.id, sources.to_vec(), reservation, Instant::now())
    }

    /// Validate the entire reply set before installing anything. Rejection
    /// seals and charges the ticket; a retry needs a new admitted attempt.
    pub fn complete_guidance_batch(
        &mut self,
        ticket: GuidanceTicket,
        replies: Vec<(NodeRef, Guidance)>,
        usage: Option<TokenUsage>,
    ) -> Result<(), SearchError> {
        let expected = self.guidance.open_sources(self.id, ticket)?;
        let mut prepared = (|| {
            if self.budget.deadline.is_some_and(|d| Instant::now() >= d) {
                return Err(SearchError::GuidanceExpired);
            }
            if replies.len() != expected.len() {
                return Err(SearchError::InvalidGuidance);
            }
            for (offset, (source, _)) in replies.iter().enumerate() {
                if !expected.contains(source) {
                    return Err(SearchError::InvalidSource);
                }
                if replies[..offset].iter().any(|(old, _)| old == source) {
                    return Err(SearchError::DuplicateGuidanceSource);
                }
            }
            // Apply in request order, regardless of provider response order.
            expected
                .iter()
                .map(|source| {
                    let (_, g) = replies
                        .iter()
                        .find(|(r, _)| r == source)
                        .expect("validated reply set");
                    self.prepare_guidance(*source, g.clone())
                })
                .collect::<Result<Vec<_>, _>>()
        })();
        // Reply acceptance is decided after validation, which may itself take
        // time. An expired batch still closes its cost receipt.
        let finished_at = Instant::now();
        if self.budget.deadline.is_some_and(|d| finished_at >= d) {
            prepared = Err(SearchError::GuidanceExpired);
        }
        let outcome = match &prepared {
            Ok(_) => GuidanceOutcome::Applied,
            Err(SearchError::GuidanceExpired) => GuidanceOutcome::Expired,
            Err(_) => GuidanceOutcome::Rejected,
        };
        let settled = self
            .guidance
            .finish(self.id, ticket, usage, outcome, finished_at)?;
        if settled == GuidanceOutcome::UsageExceededReservation {
            self.refresh_wait();
            return Err(SearchError::GuidanceUsageOverrun);
        }
        let result = match prepared {
            Ok(prepared) => {
                for item in prepared {
                    self.install_guidance(item);
                }
                Ok(())
            }
            Err(error) => Err(error),
        };
        self.refresh_wait();
        result
    }

    /// A failed call releases its node reservations, not its spent budget.
    /// Retrying requires an explicitly admitted new ticket.
    pub fn fail_guidance_batch(
        &mut self,
        ticket: GuidanceTicket,
        usage: Option<TokenUsage>,
    ) -> Result<(), SearchError> {
        let outcome = self.guidance.finish(
            self.id,
            ticket,
            usage,
            GuidanceOutcome::Failed,
            Instant::now(),
        )?;
        self.refresh_wait();
        if outcome == GuidanceOutcome::UsageExceededReservation {
            Err(SearchError::GuidanceUsageOverrun)
        } else {
            Ok(())
        }
    }

    /// A transport adapter could not decode or bind the reply. Charge the
    /// attempt without submitting partial or fabricated neutral guidance.
    pub fn reject_guidance_batch(
        &mut self,
        ticket: GuidanceTicket,
        usage: Option<TokenUsage>,
    ) -> Result<(), SearchError> {
        let outcome = self.guidance.finish(
            self.id,
            ticket,
            usage,
            GuidanceOutcome::Rejected,
            Instant::now(),
        )?;
        self.refresh_wait();
        if outcome == GuidanceOutcome::UsageExceededReservation {
            Err(SearchError::GuidanceUsageOverrun)
        } else {
            Ok(())
        }
    }

    pub fn guidance_costs(&self) -> GuidanceCosts {
        self.guidance.costs()
    }

    pub fn guidance_receipt(
        &self,
        ticket: GuidanceTicket,
    ) -> Result<&GuidanceReceipt, SearchError> {
        self.guidance.receipt(self.id, ticket)
    }

    pub fn timings(&self) -> SearchTimings {
        let now = Instant::now();
        let wait_end = self.budget.deadline.map_or(now, |d| d.min(now));
        SearchTimings {
            elapsed: now.saturating_duration_since(self.started),
            active_search: self.active_search,
            transitions: self.transitions,
            blocked_on_guidance: self.blocked_time
                + self
                    .blocked_since
                    .map_or(Duration::ZERO, |s| wait_end.saturating_duration_since(s)),
        }
    }

    fn stop_reason(&self) -> Option<StopReason> {
        if self.budget.deadline.is_some_and(|d| Instant::now() >= d) {
            Some(StopReason::Deadline)
        } else if self.nodes[0].closed {
            Some(StopReason::NoOpenWork)
        } else if self.stats.services >= self.budget.max_services {
            Some(StopReason::ServiceBudget)
        } else if self.nodes.len() >= self.budget.max_nodes {
            Some(StopReason::NodeBudget)
        } else if self
            .budget
            .max_engine_steps
            .saturating_sub(self.stats.engine_steps)
            < self.config.max_engine_steps_per_transition
        {
            Some(StopReason::EngineBudget)
        } else {
            None
        }
    }

    fn refresh_wait(&mut self) {
        let now = Instant::now();
        let waiting = self.stop_reason().is_none()
            && !self.pending.is_empty()
            && (self.pending.len() >= self.config.max_pending_guidance || !self.nodes[0].runnable);
        if waiting && self.blocked_since.is_none() {
            self.blocked_since = Some(now);
        }
        if !waiting {
            if let Some(started) = self.blocked_since.take() {
                let end = self.budget.deadline.map_or(now, |d| d.min(now));
                self.blocked_time += end.saturating_duration_since(started);
            }
        }
    }

    fn feedback(&mut self, mut index: usize, service: bool, value: Option<f64>) {
        loop {
            self.nodes[index].estimate.update(service, value);
            let Some((parent, edge)) = self.nodes[index].parent else {
                break;
            };
            self.nodes[parent].edges[edge]
                .estimate
                .update(service, value);
            index = parent;
        }
    }

    fn close_ancestors(&mut self, mut index: usize) {
        loop {
            let closed = self.nodes[index].closed
                || self.nodes[index]
                    .edges
                    .iter()
                    .all(|e| e.blocked || e.child.is_some_and(|c| self.nodes[c].closed));
            self.nodes[index].closed = closed;
            self.nodes[index].runnable = !closed
                && self.nodes[index].guided
                && self.nodes[index]
                    .edges
                    .iter()
                    .any(|e| !e.blocked && e.child.is_none_or(|c| self.nodes[c].runnable));
            let Some((parent, _)) = self.nodes[index].parent else {
                break;
            };
            index = parent;
        }
    }

    fn select_edge(&self, index: usize) -> Option<usize> {
        let n = &self.nodes[index];
        n.edges
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.blocked && e.child.is_none_or(|c| self.nodes[c].runnable))
            .max_by(|(ai, a), (bi, b)| {
                // Positive exploration mass keeps unseen edges eligible. A
                // separate unseen-first ordering would override PUCT and force
                // breadth-first model calls before following a strong prior.
                let score = |e: &Edge<D::Action>| {
                    e.estimate.mean(self.config.neutral_value)
                        + self.config.exploration
                            * e.prior
                            * ((n.estimate.visits + 1) as f64).sqrt()
                            / (1 + e.estimate.visits) as f64
                };
                score(a).total_cmp(&score(b)).then_with(|| bi.cmp(ai))
            })
            .map(|(i, _)| i)
    }

    pub fn step(&mut self) -> Result<StepStatus, SearchError> {
        if let Some(reason) = self.stop_reason() {
            self.refresh_wait();
            return Ok(StepStatus::Stopped(reason));
        }
        if self.pending.len() >= self.config.max_pending_guidance || !self.nodes[0].runnable {
            self.refresh_wait();
            return self
                .pending
                .first()
                .map(|i| StepStatus::NeedsGuidance(self.reference(*i)))
                .ok_or_else(|| SearchError::Domain("no runnable work or pending guidance".into()));
        }
        let started = Instant::now();
        let result = self.step_active();
        self.active_search += started.elapsed();
        self.refresh_wait();
        result
    }

    /// Execute only this source's accepted maximum-weight choice. No PUCT,
    /// exploration mixture, sibling fallback or implicit neutral guidance.
    /// A materialized edge can return its terminal or still-pending successor
    /// without new work, including an already-guided virtual successor.
    pub fn follow_guidance(&mut self, source: NodeRef) -> Result<StepStatus, SearchError> {
        let index = self.resolve(source)?;
        if self.guidance.in_flight(source) {
            return Err(SearchError::GuidanceInFlight);
        }
        let edge = self.nodes[index]
            .preferred_edge
            .ok_or(SearchError::InvalidGuidance)?;
        if let Some(child) = self.nodes[index].edges[edge].child {
            if self.nodes[child].terminal.is_none() && self.nodes[child].guided {
                return Ok(StepStatus::Guided(self.reference(child)));
            }
            return Ok(match self.nodes[child].terminal {
                Some(won) => StepStatus::Terminal {
                    node: self.reference(child),
                    won,
                },
                None => StepStatus::NeedsGuidance(self.reference(child)),
            });
        }
        if let Some(reason) = self.stop_reason() {
            self.refresh_wait();
            return Ok(StepStatus::Stopped(reason));
        }
        if self.nodes[index].closed || self.nodes[index].edges[edge].blocked {
            return Err(SearchError::InvalidSource);
        }
        if self.pending.len() >= self.config.max_pending_guidance {
            return Err(SearchError::GuidanceCapacity);
        }
        let started = Instant::now();
        // Explicit direct execution supersedes the automatic rollout cursor.
        self.policy_rollout = None;
        let result = if self.nodes[index].depth >= self.budget.max_depth {
            self.cut_depth(index)
        } else {
            self.expand_edge(index, edge)
        };
        self.active_search += started.elapsed();
        self.refresh_wait();
        result
    }

    fn cut_depth(&mut self, index: usize) -> Result<StepStatus, SearchError> {
        self.nodes[index].closed = true;
        self.stats.coverage_gaps += 1;
        self.stats.services += 1;
        self.feedback(index, true, None);
        self.close_ancestors(index);
        Ok(StepStatus::Progress)
    }

    fn step_active(&mut self) -> Result<StepStatus, SearchError> {
        if let Some((index, remaining)) = self.policy_rollout.take() {
            if self.nodes[index].depth >= self.budget.max_depth {
                return self.cut_depth(index);
            }
            // Only a fresh leaf receives uninterrupted policy work. If another
            // explicit operation already materialized this edge, resume PUCT;
            // never spin over a cached terminal or replay it as new evidence.
            if !self.nodes[index].closed && self.nodes[index].guided {
                if let Some(edge) = self.nodes[index].preferred_edge {
                    if !self.nodes[index].edges[edge].blocked
                        && self.nodes[index].edges[edge].child.is_none()
                    {
                        let before = self.stats.services;
                        let status = self.expand_edge(index, edge)?;
                        self.stats.policy_rollout_services += self.stats.services - before;
                        self.set_policy_rollout(status, remaining - 1)?;
                        return Ok(status);
                    }
                }
            }
        }
        let mut index = 0;
        loop {
            if self.budget.deadline.is_some_and(|d| Instant::now() >= d) {
                return Ok(StepStatus::Stopped(StopReason::Deadline));
            }
            if self.nodes[index].depth >= self.budget.max_depth {
                return self.cut_depth(index);
            }
            let edge = self
                .select_edge(index)
                .ok_or_else(|| SearchError::Domain("open node has no open edge".into()))?;
            if let Some(child) = self.nodes[index].edges[edge].child {
                index = child;
                continue;
            }
            let status = self.expand_edge(index, edge)?;
            self.set_policy_rollout(status, self.config.max_policy_rollout_steps)?;
            return Ok(status);
        }
    }

    fn set_policy_rollout(
        &mut self,
        status: StepStatus,
        remaining: usize,
    ) -> Result<(), SearchError> {
        if remaining > 0 {
            if let StepStatus::NeedsGuidance(source) | StepStatus::Guided(source) = status {
                self.policy_rollout = Some((self.resolve(source)?, remaining));
            }
        }
        Ok(())
    }

    // PUCT and direct-policy execution share exactly this materialization path.
    fn expand_edge(&mut self, index: usize, edge: usize) -> Result<StepStatus, SearchError> {
        if self.nodes.len() >= self.budget.max_nodes {
            return Ok(StepStatus::Stopped(StopReason::NodeBudget));
        }
        let remaining = self
            .budget
            .max_engine_steps
            .saturating_sub(self.stats.engine_steps);
        if remaining < self.config.max_engine_steps_per_transition {
            return Ok(StepStatus::Stopped(StopReason::EngineBudget));
        }
        let transition_started = Instant::now();
        let outcome = self.space.transition(
            &self.nodes[index].state,
            &self.nodes[index].edges[edge].action,
            CombatStepLimits {
                max_engine_steps: self.config.max_engine_steps_per_transition,
                deadline: self.budget.deadline,
            },
        );
        self.transitions += transition_started.elapsed();
        let outcome = outcome.map_err(SearchError::Domain)?;
        let spent = match &outcome {
            Transition::Settled { engine_steps, .. }
            | Transition::Interrupted { engine_steps, .. } => *engine_steps,
        };
        if spent > self.config.max_engine_steps_per_transition {
            return Err(SearchError::InvalidTransitionCost);
        }
        self.stats.engine_steps += spent;
        self.stats.services += 1;
        match outcome {
            Transition::Interrupted { cause, .. } => {
                self.nodes[index].edges[edge].estimate.update(true, None);
                self.feedback(index, true, None);
                if cause == TransitionInterruption::EngineStepLimit {
                    self.nodes[index].edges[edge].blocked = true;
                    self.stats.coverage_gaps += 1;
                    self.close_ancestors(index);
                    return Ok(StepStatus::Progress);
                }
                Ok(StepStatus::Stopped(StopReason::Deadline))
            }
            Transition::Settled { state, .. } => {
                // Exact equality only; a cycle is an unexpanded continuation,
                // never a fabricated losing terminal sample.
                let mut ancestor = Some(index);
                while let Some(a) = ancestor {
                    if self.nodes[a].state == state {
                        self.nodes[index].edges[edge].blocked = true;
                        self.nodes[index].edges[edge].estimate.update(true, None);
                        self.feedback(index, true, None);
                        self.stats.coverage_gaps += 1;
                        self.close_ancestors(index);
                        return Ok(StepStatus::Progress);
                    }
                    ancestor = self.nodes[a].parent.map(|(p, _)| p);
                }
                let child = Self::make_node(
                    &self.space,
                    state,
                    Some((index, edge)),
                    self.nodes[index].depth + 1,
                )?;
                let terminal = child.terminal;
                let child_index = self.nodes.len();
                self.nodes.push(child);
                self.nodes[index].edges[edge].child = Some(child_index);
                self.feedback(
                    child_index,
                    true,
                    terminal.map(|won| if won { 1.0 } else { 0.0 }),
                );
                if let Some(won) = terminal {
                    self.stats.terminal_observations += 1;
                    self.close_ancestors(child_index);
                    return Ok(StepStatus::Terminal {
                        node: self.reference(child_index),
                        won,
                    });
                }
                self.pending.push(child_index);
                self.close_ancestors(child_index);
                Ok(StepStatus::NeedsGuidance(self.reference(child_index)))
            }
        }
    }

    pub fn observed_wins(&self) -> Vec<NodeRef> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.terminal == Some(true))
            .map(|(i, _)| self.reference(i))
            .collect()
    }

    pub(crate) fn path(
        &self,
        source: NodeRef,
    ) -> Result<Vec<(&D::State, &D::Action, &D::State)>, SearchError> {
        Ok(self
            .decision_path(source)?
            .into_iter()
            .map(|(before, edge, after)| {
                (
                    &self.nodes[before.index].state,
                    &self.nodes[before.index].edges[edge].action,
                    &self.nodes[after.index].state,
                )
            })
            .collect())
    }

    /// Exact source-local edges, including virtual selection assembly. Shares
    /// the parent chain used by combat replay; ordinals have no global identity.
    pub(crate) fn decision_path(
        &self,
        source: NodeRef,
    ) -> Result<Vec<(NodeRef, usize, NodeRef)>, SearchError> {
        let mut index = self.resolve(source)?;
        let mut steps = Vec::new();
        while let Some((parent, edge)) = self.nodes[index].parent {
            steps.push((self.reference(parent), edge, self.reference(index)));
            index = parent;
        }
        steps.reverse();
        Ok(steps)
    }
}

fn normalize(weights: &[f64]) -> Option<Vec<f64>> {
    if weights.is_empty() || weights.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    let scale = weights.iter().copied().fold(0.0, f64::max);
    if scale == 0.0 {
        return None;
    }
    let total: f64 = weights.iter().map(|v| v / scale).sum();
    Some(weights.iter().map(|v| (v / scale) / total).collect())
}

#[cfg(test)]
mod timing_tests {
    use super::*;

    #[test]
    fn late_reply_is_charged_without_guidance_and_waiting_stops_at_deadline() {
        let mut session = SearchSession::new(
            crate::tests::Graph::new(vec![vec![1], vec![]], vec![None, Some(true)]),
            0,
            SearchConfig::default(),
            SearchBudget {
                max_services: 10,
                max_engine_steps: 1_000,
                max_nodes: 10,
                max_depth: 10,
                deadline: None,
                guidance: GuidanceBudget {
                    max_calls: 1,
                    max_input_tokens: 10,
                    max_output_tokens: 10,
                },
            },
        )
        .unwrap();
        let source = session.root();
        let ticket = session
            .begin_guidance_batch(
                &[source],
                TokenReservation {
                    input: 10,
                    output: 10,
                },
            )
            .unwrap();
        let before = session.stats();
        // Set clock anchors directly: no sleep, timeout race or public clock API.
        let start = Instant::now() - Duration::from_secs(10);
        session.started = start;
        session.blocked_since = Some(start + Duration::from_secs(1));
        session.budget.deadline = Some(start + Duration::from_secs(3));
        assert_eq!(
            session.timings().blocked_on_guidance,
            Duration::from_secs(2)
        );
        assert_eq!(
            session.complete_guidance_batch(
                ticket,
                vec![(
                    source,
                    Guidance {
                        value: Some(1.0),
                        ..Default::default()
                    }
                )],
                Some(TokenUsage {
                    input: 8,
                    output: 4
                })
            ),
            Err(SearchError::GuidanceExpired)
        );
        assert_eq!(session.stats(), before);
        assert_eq!(session.pending_guidance(), [source]);
        assert_eq!(session.view(source).unwrap().estimated_value, None);
        assert_eq!(
            session.guidance_receipt(ticket).unwrap().outcome,
            GuidanceOutcome::Expired
        );
        assert_eq!(session.guidance_costs().known_input_tokens, 8);
        assert_eq!(session.guidance_costs().in_flight, 0);
        assert!(session.blocked_since.is_none());
        assert_eq!(
            session.timings().blocked_on_guidance,
            Duration::from_secs(2)
        );
        assert_eq!(
            session.step().unwrap(),
            StepStatus::Stopped(StopReason::Deadline)
        );
        assert!(session.observed_wins().is_empty());
    }
}
