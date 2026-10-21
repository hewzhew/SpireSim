//! Opt-in observation of a supplied, replay-checked line. Never seeds search.
use super::retention::SCORE_LANES;
use super::*;
use std::collections::BTreeMap;

struct Reference {
    nodes: Vec<Node>,
    index: HashMap<u64, Vec<usize>>,
    steps: Vec<ReferenceStep>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReferenceStep {
    /// Number of fully materialized inputs, including this boundary.
    pub inputs_completed: usize,
    pub branch: Option<CombatBranch>,
    pub turn: u32,
    pub hp: i32,
    pub energy: u8,
    pub hand: Vec<String>,
    pub selection: Option<SelectionAudit>,
    pub scores: [i64; SCORE_LANES],
    /// Request relations at this exact boundary. Counterfactual preparation
    /// is not an executed successor, forecast, or survival certificate.
    pub recovery: super::RecoveryProfile,
    /// Latent source-bound requests, separate from realized hand/resources.
    /// Collected for diagnosis only; they do not guide retention or expansion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draw_triggers: Option<super::DrawTriggerProfile>,
}

/// Virtual assembly has not changed the combat position or real input trace.
/// Record the source-bound members so equal-looking choices remain inspectable.
#[derive(Clone, Debug, Serialize)]
pub struct SelectionAudit {
    pub family: usize,
    pub selected: Vec<crate::selection::SelectionMember>,
}

fn selection(state: &CombatSearchState) -> Option<SelectionAudit> {
    Some(SelectionAudit {
        family: state.selection_family()?,
        selected: state
            .selected_members()?
            .iter()
            .map(|&index| state.selection_members().unwrap()[index].clone())
            .collect(),
    })
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AuditCount {
    pub count: usize,
    pub first_work: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RetentionAudit {
    pub pool: String,
    pub candidates: usize,
    pub capacity: usize,
    /// One-based ranks before exact deduplication; ties follow production order.
    pub ranks: [usize; SCORE_LANES],
    pub equal_scores: usize,
    pub kept: bool,
    /// Diagnostic selection from this same pool without backed guidance.
    /// These counterfactual nodes are never adopted or expanded.
    pub kept_without_backed_guidance: bool,
    /// Same pool and existing graph guidance, with only the direct model lane
    /// replaced by its hand-written score. Earlier model effects are not undone.
    /// Diagnostic only; these candidates are never adopted or expanded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept_without_resource_model: Option<bool>,
    /// Real input chains of up to four retained competitors, for replay.
    pub competitors: Vec<Vec<ClientInput>>,
    /// First retained peer sharing the actual diversity context, if any.
    /// This is not a state equivalence or proof that the peer has a better future.
    pub same_context_kept: Option<RetentionPeer>,
    /// Actual root-bound outcomes available at this exact omission boundary.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub backed_returns: Vec<CombatWitnessMaterializedOutcomeV1>,
    /// Up to four retained candidates with replay-backed returns at omission.
    /// Availability does not establish whether guidance affected selection.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub backed_kept: Vec<RetentionPeer>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RetentionPeer {
    pub inputs: Vec<ClientInput>,
    pub selection: Option<SelectionAudit>,
    pub scores: [i64; SCORE_LANES],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub backed_returns: Vec<CombatWitnessMaterializedOutcomeV1>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StepAudit {
    pub events: BTreeMap<String, AuditCount>,
    pub first_omission: Option<RetentionAudit>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct WorkAudit {
    pub transitions: usize,
    pub transitions_by_turn: BTreeMap<u32, usize>,
    /// Already violated the monotone explicit-expenditure lower bound at source.
    pub from_over_explicit_limit: usize,
    pub from_discard_prefix: usize,
    pub end_turn: usize,
    pub play_card: usize,
    pub use_potion: usize,
    pub discard_potion: usize,
    pub selection_or_other: usize,
    pub pending_choice_work: BTreeMap<String, usize>,
}

#[derive(Serialize)]
pub struct SolverAudit {
    pub reference: Vec<ReferenceStep>,
    pub passes: Vec<PassAuditReport>,
}

#[derive(Serialize)]
pub struct PassAuditReport {
    pub width: usize,
    pub card_only: bool,
    pub work: WorkAudit,
    pub steps: Vec<StepAudit>,
    pub quotas: Vec<QuotaAudit>,
}

/// Bounded observations at actual turn-quota cuts; counts describe queued
/// work, not distinct positions or the reachability of a continuation.
#[derive(Clone, Debug, Default, Serialize)]
pub struct QuotaTurnAudit {
    pub active: usize,
    pub next: usize,
    pub ended: usize,
    pub attack: usize,
    pub effect: usize,
    pub response: usize,
    pub pending_sources: usize,
    pub pending_branches: usize,
    pub ordinary_branches: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct QuotaAudit {
    pub at_work: usize,
    pub turn_work: usize,
    pub reserve_size: usize,
    pub reserve_cursors: usize,
    pub by_turn: BTreeMap<u32, QuotaTurnAudit>,
}

pub(super) struct PassAudit {
    reference: Arc<Reference>,
    work: WorkAudit,
    steps: Vec<StepAudit>,
    quotas: Vec<QuotaAudit>,
    capture: Option<(usize, usize, bool)>,
    pub(super) cohort: Option<super::inspection::RetentionCohort>,
}

impl PassAudit {
    pub(super) fn quota(pass: &mut Pass, work: usize) {
        if pass.audit.as_ref().is_none_or(|a| a.quotas.len() >= 32) {
            return;
        }
        let mut by_turn = BTreeMap::<u32, QuotaTurnAudit>::new();
        let turn = |node: &Node| node.state.position().combat.turn.turn_count;
        for node in &pass.active {
            by_turn.entry(turn(node)).or_default().active += 1;
        }
        for node in &pass.next {
            by_turn.entry(turn(node)).or_default().next += 1;
        }
        for node in &pass.ended {
            by_turn.entry(turn(node)).or_default().ended += 1;
        }
        for job in &pass.tactical {
            let counts = by_turn.entry(turn(&job.node)).or_default();
            if job.plan.is_attack() {
                counts.attack += 1;
            } else if job.plan.is_response() {
                counts.response += 1;
            } else {
                counts.effect += 1;
            }
        }
        for task in pass.pending.iter().chain(pass.resolving.iter()) {
            for node in task.nodes() {
                by_turn.entry(turn(node)).or_default().pending_sources += 1;
            }
            for (node, remaining) in task.branches_by_source() {
                by_turn.entry(turn(node)).or_default().pending_branches += remaining;
            }
        }
        if let Some(e) = &pass.ordinary {
            by_turn.entry(turn(&e.node)).or_default().ordinary_branches +=
                e.branches.len() - e.index;
        }
        pass.audit.as_mut().unwrap().quotas.push(QuotaAudit {
            at_work: work,
            turn_work: pass.turn_work,
            reserve_size: pass.deferred.len(),
            reserve_cursors: pass.deferred_cursors.len(),
            by_turn,
        });
    }

    fn new(reference: Arc<Reference>) -> Self {
        Self {
            steps: vec![StepAudit::default(); reference.nodes.len()],
            reference,
            work: WorkAudit::default(),
            quotas: vec![],
            capture: None,
            cohort: None,
        }
    }

    pub(super) fn fork(&self) -> Self {
        let mut fork = Self::new(self.reference.clone());
        fork.capture = self.capture;
        fork
    }

    fn matches(&self, node: &Node) -> Vec<usize> {
        self.reference
            .index
            .get(&node.index_key())
            .into_iter()
            .flatten()
            .copied()
            .filter(|&i| self.reference.nodes[i].state == node.state)
            .collect()
    }

    pub(super) fn note(&mut self, node: &Node, event: &str, work: usize) {
        for i in self.matches(node) {
            let count = self.steps[i].events.entry(event.into()).or_default();
            count.count += 1;
            count.first_work.get_or_insert(work);
        }
    }

    pub(super) fn transition(
        &mut self,
        source: &Node,
        input: Option<&ClientInput>,
        limit: Option<usize>,
    ) {
        self.work.transitions += 1;
        *self
            .work
            .transitions_by_turn
            .entry(source.state.position().combat.turn.turn_count)
            .or_default() += 1;
        self.work.from_over_explicit_limit += usize::from(
            limit.is_some_and(|max| source.expenditures().explicit_count() as usize > max),
        );
        let mut trace = source.trace.as_deref();
        while let Some(t) = trace {
            if matches!(t.input, ClientInput::DiscardPotion(_)) {
                self.work.from_discard_prefix += 1;
                break;
            }
            trace = t.previous.as_deref();
        }
        match input {
            Some(ClientInput::EndTurn) => self.work.end_turn += 1,
            Some(ClientInput::PlayCard { .. }) => self.work.play_card += 1,
            Some(ClientInput::UsePotion { .. }) => self.work.use_potion += 1,
            Some(ClientInput::DiscardPotion(_)) => self.work.discard_potion += 1,
            _ => self.work.selection_or_other += 1,
        }
        use sts_core::state::core::{EngineState, PendingChoice};
        let reason = match &source.state.position().engine {
            EngineState::PendingChoice(PendingChoice::HandSelect { reason, .. }) => {
                Some(format!("hand/{}", reason.vocabulary_token_v1()))
            }
            EngineState::PendingChoice(PendingChoice::GridSelect { reason, .. }) => {
                Some(format!("grid/{reason:?}"))
            }
            EngineState::PendingChoice(_) => Some("other".into()),
            _ => None,
        };
        if let Some(reason) = reason {
            *self.work.pending_choice_work.entry(reason).or_default() += 1;
        }
    }

    pub(super) fn partition(
        &mut self,
        pool: &[Node],
        kept: &[Node],
        capacity: usize,
        label: &str,
        work: usize,
        model: Option<&crate::resource_value::ValueModel>,
        effects: bool,
        card_compositions: bool,
        guide: Option<super::graph::RetentionGuide<'_>>,
    ) {
        let mut without_backed_guidance = None;
        let mut without_resource_model = None;
        for (ordinal, node) in pool.iter().enumerate() {
            let targets = self.matches(node);
            if targets.is_empty() {
                continue;
            }
            let selected = kept.iter().any(|n| n.state == node.state);
            self.note(node, if selected { "retained" } else { "omitted" }, work);
            if selected
                || targets
                    .iter()
                    .all(|&i| self.steps[i].first_omission.is_some())
            {
                continue;
            }
            let values = retention::evaluate(node, model);
            let mut ranks = [1; SCORE_LANES];
            let mut equal_scores = 0;
            for (j, candidate) in pool.iter().enumerate() {
                let other = retention::evaluate(candidate, model);
                equal_scores += usize::from(other.lanes == values.lanes);
                for lane in 0..SCORE_LANES {
                    ranks[lane] += usize::from(
                        retention::lane_rank_key(
                            &other.lanes,
                            lane,
                            other.durable,
                            other.enemy_progress,
                            candidate.input_count,
                            j,
                        ) < retention::lane_rank_key(
                            &values.lanes,
                            lane,
                            values.durable,
                            values.enemy_progress,
                            node.input_count,
                            ordinal,
                        ),
                    );
                }
            }
            let observation = RetentionAudit {
                pool: label.into(),
                candidates: pool.len(),
                capacity,
                ranks,
                equal_scores,
                kept: selected,
                kept_without_backed_guidance: without_backed_guidance
                    .get_or_insert_with(|| {
                        retention::partition(
                            pool.to_vec(),
                            capacity,
                            &mut SolverStats::default(),
                            effects,
                            card_compositions,
                            model,
                            None,
                        )
                        .0
                    })
                    .iter()
                    .any(|other| other.state == node.state),
                kept_without_resource_model: model.map(|_| {
                    without_resource_model
                        .get_or_insert_with(|| {
                            retention::partition(
                                pool.to_vec(),
                                capacity,
                                &mut SolverStats::default(),
                                effects,
                                card_compositions,
                                None,
                                guide,
                            )
                            .0
                        })
                        .iter()
                        .any(|other| other.state == node.state)
                }),
                backed_returns: guide.map_or_else(Vec::new, |guide| guide.outcomes(node)),
                backed_kept: guide.map_or_else(Vec::new, |guide| {
                    kept.iter()
                        .filter_map(|other| {
                            let backed_returns = guide.outcomes(other);
                            (!backed_returns.is_empty()).then(|| RetentionPeer {
                                inputs: other.inputs(),
                                selection: selection(&other.state),
                                scores: retention::evaluated_scores(other, model),
                                backed_returns,
                            })
                        })
                        .take(4)
                        .collect()
                }),
                competitors: kept.iter().take(4).map(Node::inputs).collect(),
                same_context_kept: kept
                    .iter()
                    .find(|other| {
                        retention::same_retention_context(node, other, effects, card_compositions)
                    })
                    .map(|other| RetentionPeer {
                        inputs: other.inputs(),
                        selection: selection(&other.state),
                        scores: retention::evaluated_scores(other, model),
                        backed_returns: guide.map_or_else(Vec::new, |guide| guide.outcomes(other)),
                    }),
            };
            for i in targets {
                self.steps[i]
                    .first_omission
                    .get_or_insert_with(|| observation.clone());
            }
        }
    }
}

impl Pass {
    pub(super) fn partition(
        &mut self,
        pool: Vec<Node>,
        width: usize,
        stats: &mut SolverStats,
        effects: bool,
        card_compositions: bool,
        model: Option<&crate::resource_value::ValueModel>,
        label: &str,
        guide: Option<super::graph::RetentionGuide<'_>>,
    ) -> (Vec<Node>, Vec<Node>) {
        // Only the explicitly enabled audit retains the before-pool for inspection.
        let before = self.audit.as_ref().map(|_| pool.clone());
        let result =
            retention::partition(pool, width, stats, effects, card_compositions, model, guide);
        if let Some(samples) = &self.samples {
            samples.lock().unwrap().observe(
                &result.0,
                &result.1,
                self.width,
                self.card_only,
                width,
                label,
                stats.work(),
            );
        }
        self.offer_attack_plans(&result.1, &result.0, stats);
        if label == "next_turn" {
            self.offer_response_plans(&result.0, stats);
            self.offer_response_plans(&result.1, stats);
        }
        self.offer_order_families(&result.0, &result.1, stats);
        if let (Some(audit), Some(before)) = (&mut self.audit, before) {
            if let Some((step, pass_width, card_only)) = audit.capture {
                let target = &audit.reference.nodes[step];
                if audit.cohort.is_none()
                    && self.width == pass_width
                    && self.card_only == card_only
                    && before.iter().any(|n| {
                        n.state == target.state && n.expenditures() == target.expenditures()
                    })
                    && !result.0.iter().any(|n| {
                        n.state == target.state && n.expenditures() == target.expenditures()
                    })
                {
                    audit.cohort = Some(super::inspection::RetentionCohort::new(
                        step,
                        &before,
                        &result.0,
                        target,
                        label,
                        stats.work(),
                        effects,
                        card_compositions,
                    ));
                }
            }
            audit.partition(
                &before,
                &result.0,
                width,
                label,
                stats.work(),
                model,
                effects,
                card_compositions,
                guide,
            );
        }
        result
    }
}

impl CombatSolver {
    /// Capture one actual omission batch for later, separately budgeted inspection.
    /// Requires a replay-checked reference and must be requested before search.
    pub fn capture_retention(
        &mut self,
        reference_step: usize,
        width: usize,
        card_only: bool,
    ) -> Result<(), String> {
        if self.config.resource_model.is_some() {
            return Err(
                "retention response inspection currently requires the hand-written score lanes"
                    .into(),
            );
        }
        if self.stats.work() != 0 {
            return Err("capture must be requested before searching".into());
        }
        let audit = self.beams[0].passes[0]
            .audit
            .as_ref()
            .ok_or("enable reference audit first")?;
        let target = audit
            .reference
            .nodes
            .get(reference_step)
            .ok_or("reference step out of range")?;
        if target.state.selected_members().is_some() {
            return Err("inspection requires a materialized input boundary".into());
        }
        let mut scheduled_width = self.config.initial_width;
        while scheduled_width < width && scheduled_width < self.config.max_width {
            scheduled_width = (scheduled_width * 2).min(self.config.max_width);
        }
        if scheduled_width != width || !self.beams.iter().any(|b| b.card_only == card_only) {
            return Err("requested width or potion family is not scheduled".into());
        }
        for pass in self.beams.iter_mut().flat_map(|b| &mut b.passes) {
            pass.audit.as_mut().unwrap().capture = Some((reference_step, width, card_only));
        }
        Ok(())
    }

    /// Validate a complete reference under this solver's resource constraint,
    /// then watch exact states (including selection assembly) without inserting
    /// candidates or changing ranking. Must be enabled before any search work.
    /// Reference replay is diagnostic setup and is not charged as search work.
    pub fn audit_reference(
        &mut self,
        inputs: &[ClientInput],
        expected: &CombatPosition,
    ) -> Result<(), String> {
        if self.stats.work() != 0 {
            return Err("enable audit before searching".into());
        }
        let mut node = Node::root(CombatSearchState::new(self.root.clone())?)
            .with_potion_limit(self.config.max_potion_uses);
        let mut reference = Reference {
            nodes: vec![],
            index: HashMap::new(),
            steps: vec![],
        };
        let mut push = |node: &Node, branch: Option<CombatBranch>| {
            let i = reference.nodes.len();
            reference.index.entry(node.index_key()).or_default().push(i);
            let c = &node.state.position().combat;
            let values = retention::evaluate(node, self.config.resource_model.as_ref());
            reference.steps.push(ReferenceStep {
                inputs_completed: node.input_count,
                branch,
                turn: c.turn.turn_count + 1,
                hp: c.entities.player.current_hp,
                energy: c.turn.energy,
                hand: c
                    .zones
                    .hand
                    .iter()
                    .map(|c| format!("{:?}+{}", c.id, c.upgrades))
                    .collect(),
                selection: selection(&node.state),
                scores: values.lanes,
                recovery: retention::recovery::profile(node, values.recovery),
                draw_triggers: retention::draw_triggers::profile(c),
            });
            reference.nodes.push(node.clone());
        };
        push(&node, None);
        for input in inputs {
            let path = self.space.prepare_choices(&node.state)?.input_path(input)?;
            for branch in path {
                let prepared = self.space.prepare_choices(&node.state)?;
                let index = prepared
                    .branches()
                    .iter()
                    .position(|b| *b == branch)
                    .ok_or("reference branch not permitted")?;
                let materialized = prepared.input(index)?;
                let Transition::Settled { state, .. } = prepared.transition(
                    index,
                    CombatStepLimits {
                        max_engine_steps: self.config.max_engine_steps_per_input,
                        deadline: None,
                    },
                )?
                else {
                    return Err("reference replay truncated".into());
                };
                node.state = state;
                if let Some(input) = materialized {
                    let mut expenditures = node.expenditures();
                    expenditures.record(&input);
                    node.trace = Some(Arc::new(Trace {
                        previous: node.trace,
                        input,
                        expenditures,
                    }));
                    node.input_count += 1;
                }
                push(&node, Some(branch));
            }
        }
        if node.state.position() != expected {
            return Err("reference successor mismatch".into());
        }
        let outcome = materialize_replay_checked_combat_outcome_v1(&self.root, inputs, expected)?;
        if self
            .config
            .max_potion_uses
            .is_some_and(|max| outcome.total_potion_expenditures as usize > max)
        {
            return Err("reference exceeds solver potion expenditure constraint".into());
        }
        let reference = Arc::new(reference);
        for beam in &mut self.beams {
            for pass in &mut beam.passes {
                pass.audit = Some(PassAudit::new(reference.clone()));
            }
        }
        Ok(())
    }

    pub fn audit(&self) -> Option<SolverAudit> {
        let reference = &self
            .beams
            .first()?
            .passes
            .first()?
            .audit
            .as_ref()?
            .reference;
        Some(SolverAudit {
            reference: reference.steps.clone(),
            passes: self
                .beams
                .iter()
                .flat_map(|b| &b.passes)
                .filter_map(|pass| {
                    let audit = pass.audit.as_ref()?;
                    Some(PassAuditReport {
                        width: pass.width,
                        card_only: pass.card_only,
                        work: audit.work.clone(),
                        steps: audit.steps.clone(),
                        quotas: audit.quotas.clone(),
                    })
                })
                .collect(),
        })
    }
}
