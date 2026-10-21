//! Bounded lookahead over public decisions and observation-contingent policies.
//! The caller supplies a PUBLIC leaf estimate. Particles are propagated with
//! their existing weights, with optional history-checked correlated proposals.

use serde::Serialize;
use std::time::{Duration, Instant};
use sts_combat_outcome::agent::{
    belief::{
        CombatBeliefChanceBranchV1, CombatBeliefEnvironmentV1, CombatPublicBoundaryV1,
        CombatPublicDecisionV1, HistoryRejuvenationConfig, HistoryRejuvenationReport,
    },
    information::action::{
        PublicCombatActionChoiceV1 as Choice, PublicCombatAtomicActionV1 as Atomic,
        PublicCombatSelectionDomainCandidateV1 as Member,
    },
};
use sts_core::{
    runtime::combat::DrawPileChange,
    sim::combat::{CombatStepper, CombatTerminal},
};

#[derive(Clone, Debug, Serialize)]
pub struct LookaheadConfig {
    pub depth: usize,
    /// Below the root, only this many one-step candidates receive deeper work.
    pub width: usize,
    pub work: usize,
    pub max_engine_steps: usize,
    pub selection_limit: usize,
    /// Slot permissions, not an HP price for spending a potion.
    pub allowed_potion_slots: Vec<usize>,
    /// Optional proposal-family expansion when observations thin the population.
    /// These correlated candidates do not form a calibrated history posterior.
    pub rejuvenation: Option<HistoryRejuvenationConfig>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LeafStop {
    Terminal,
    Depth,
    Work,
    Time,
    NoCandidates,
    PopulationWork,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LeafSummary {
    pub win_mass: f64,
    pub loss_mass: f64,
    pub unfinished_mass: f64,
    /// Includes live frontier states. Not an estimate of final combat HP.
    pub mean_boundary_hp: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContingentPolicy {
    pub boundary: CombatPublicBoundaryV1,
    pub value_estimate: f64,
    pub leaves: LeafSummary,
    pub action: Option<Choice>,
    pub stop: Option<LeafStop>,
    pub branches: Vec<PolicyBranch>,
    pub rejuvenation: Option<RejuvenationReceipt>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RejuvenationReceipt {
    pub config: HistoryRejuvenationConfig,
    pub report: HistoryRejuvenationReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct PolicyBranch {
    pub probability: f64,
    pub draw_pile_changes: Vec<DrawPileChange>,
    pub trace_complete: bool,
    pub continuation: ContingentPolicy,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LookaheadStats {
    /// Charged per particle/input, including action batches rejected by errors.
    pub transitions: usize,
    pub expanded_decisions: usize,
    pub action_representatives_omitted: usize,
    pub selection_capped_nodes: usize,
    pub width_pruned: usize,
    pub rejuvenated_nodes: usize,
    pub reconstruction_transitions: usize,
    pub proposals_accepted: usize,
    pub proposals_contradicted_history: usize,
    pub population_work_stops: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct LookaheadReport {
    pub policy: ContingentPolicy,
    /// Every retained root action, including shallow or unfinished alternatives.
    pub alternatives: Vec<ContingentPolicy>,
    pub stats: LookaheadStats,
}

/// Larger estimates are preferred. Estimates are heuristics, never bounds or
/// win probabilities. Only a public boundary is supplied; callers must not
/// capture private runtime state in the evaluator's closure.
pub fn lookahead(
    environment: &CombatBeliefEnvironmentV1,
    config: &LookaheadConfig,
    duration: Duration,
    evaluate: &impl Fn(&CombatPublicBoundaryV1) -> f64,
    stepper: &impl CombatStepper,
) -> Result<LookaheadReport, String> {
    if config.depth == 0
        || config.depth > 64
        || config.width == 0
        || config.max_engine_steps == 0
        || config.selection_limit == 0
    {
        return Err("lookahead needs depth 1..64 and positive width/step/selection limits".into());
    }
    if let Some(rejuvenation) = &config.rejuvenation {
        rejuvenation.validate()?;
    }
    let deadline = Instant::now()
        .checked_add(duration)
        .ok_or("lookahead duration overflow")?;
    let mut search = Search {
        config,
        evaluate,
        stepper,
        deadline,
        stats: LookaheadStats::default(),
    };
    let mut budget = config.work;
    let (policy, alternatives) = search.solve(environment, config.depth, &mut budget, true)?;
    Ok(LookaheadReport {
        policy,
        alternatives,
        stats: search.stats,
    })
}

struct Search<'a, E, S> {
    config: &'a LookaheadConfig,
    evaluate: &'a E,
    stepper: &'a S,
    deadline: Instant,
    stats: LookaheadStats,
}

impl<E: Fn(&CombatPublicBoundaryV1) -> f64, S: CombatStepper> Search<'_, E, S> {
    fn leaf(
        &self,
        environment: &CombatBeliefEnvironmentV1,
        reason: LeafStop,
    ) -> Result<ContingentPolicy, String> {
        let boundary = environment.public_history().current().clone();
        let (observation, terminal) = match &boundary {
            CombatPublicBoundaryV1::Decision { decision } => {
                (&decision.observation, CombatTerminal::Unresolved)
            }
            CombatPublicBoundaryV1::Terminal {
                observation,
                outcome,
                ..
            } => (observation, *outcome),
        };
        if environment
            .particles()
            .iter()
            .any(|p| p.private_position().combat.runtime.combat_smoked)
        {
            return Err(
                "escaped combat needs a distinct public outcome before lookahead can evaluate it"
                    .into(),
            );
        }
        let value_estimate = (self.evaluate)(&boundary);
        if !value_estimate.is_finite() {
            return Err("public leaf estimate must be finite".into());
        }
        Ok(ContingentPolicy {
            leaves: LeafSummary {
                win_mass: f64::from(terminal == CombatTerminal::Win),
                loss_mass: f64::from(terminal == CombatTerminal::Loss),
                unfinished_mass: f64::from(terminal == CombatTerminal::Unresolved),
                mean_boundary_hp: observation.player.hp as f64,
            },
            boundary,
            value_estimate,
            action: None,
            stop: Some(if terminal == CombatTerminal::Unresolved {
                reason
            } else {
                LeafStop::Terminal
            }),
            branches: vec![],
            rejuvenation: None,
        })
    }

    fn solve(
        &mut self,
        environment: &CombatBeliefEnvironmentV1,
        depth: usize,
        budget: &mut usize,
        root: bool,
    ) -> Result<(ContingentPolicy, Vec<ContingentPolicy>), String> {
        let mut leaf = self.leaf(environment, LeafStop::Depth)?;
        if depth == 0 || leaf.stop == Some(LeafStop::Terminal) {
            return Ok((leaf, vec![]));
        }
        if Instant::now() >= self.deadline {
            leaf.stop = Some(LeafStop::Time);
            return Ok((leaf, vec![]));
        }
        let CombatPublicBoundaryV1::Decision { decision } = &leaf.boundary else {
            unreachable!()
        };
        let (actions, omitted, capped) = candidates(decision, self.config);
        self.stats.action_representatives_omitted += omitted;
        self.stats.selection_capped_nodes += usize::from(capped);
        if actions.is_empty() {
            leaf.stop = Some(LeafStop::NoCandidates);
            return Ok((leaf, vec![]));
        }
        let refreshed;
        let environment = if let Some(settings) = self
            .config
            .rejuvenation
            .as_ref()
            .filter(|s| environment.particles().len() <= s.target_particles / 2)
        {
            let mut settings = settings.clone();
            // Public traversal and configured planner randomness only. Execution
            // identity, actual seed and private source state never select this seed.
            settings.seed = settings
                .seed
                .wrapping_add(
                    (environment.public_history().entries().len() as u64)
                        .wrapping_mul(0x9e37_79b9_7f4a_7c15),
                )
                .wrapping_add(self.stats.rejuvenated_nodes as u64);
            let work = settings.work_bound(environment)?;
            let comparison = settings
                .output_count(environment.particles().len())?
                .checked_mul(actions.len())
                .ok_or("work overflow")?;
            if *budget < work.checked_add(comparison).ok_or("work overflow")? {
                leaf.stop = Some(LeafStop::PopulationWork);
                self.stats.population_work_stops += 1;
                return Ok((leaf, vec![]));
            }
            let (next, report) = environment.rejuvenate_history(
                &settings,
                work,
                self.stepper,
                self.config.max_engine_steps,
            )?;
            *budget -= report.replay_transitions;
            self.stats.transitions += report.replay_transitions;
            self.stats.reconstruction_transitions += report.replay_transitions;
            self.stats.rejuvenated_nodes += 1;
            self.stats.proposals_accepted += report.accepted;
            self.stats.proposals_contradicted_history += report.contradicted_history;
            leaf.rejuvenation = Some(RejuvenationReceipt {
                config: settings,
                report,
            });
            refreshed = next;
            &refreshed
        } else {
            environment
        };
        // Reserve a complete one-step comparison. A too-small budget cannot
        // favor an action merely because it happened to be listed first.
        let cost = actions
            .len()
            .checked_mul(environment.particles().len())
            .ok_or("work overflow")?;
        if *budget < cost {
            leaf.stop = Some(LeafStop::Work);
            return Ok((leaf, vec![]));
        }
        let mut alternatives = Vec::new();
        let mut successors = Vec::new();
        self.stats.expanded_decisions += 1;
        for action in actions {
            if Instant::now() >= self.deadline {
                leaf.stop = Some(LeafStop::Time);
                return Ok((leaf, vec![])); // discard the incomplete comparison
            }
            let charged = environment.particles().len();
            *budget -= charged;
            self.stats.transitions += charged;
            let branches = environment
                .step(action.clone(), self.stepper, self.config.max_engine_steps)
                .map_err(|error| error.to_string())?;
            let mut plan = leaf.clone();
            plan.action = Some(action);
            plan.stop = None;
            let mut environments = Vec::new();
            for branch in branches {
                let entry = branch
                    .environment()
                    .public_history()
                    .entries()
                    .last()
                    .unwrap();
                plan.branches.push(PolicyBranch {
                    probability: branch.probability(),
                    draw_pile_changes: entry.draw_pile_changes.clone(),
                    trace_complete: entry.draw_order_trace_complete,
                    continuation: self.leaf(branch.environment(), LeafStop::Depth)?,
                });
                environments.push(branch.into_environment());
            }
            backup(&mut plan);
            alternatives.push(plan);
            successors.push(environments);
        }
        if depth > 1 {
            let mut order = (0..alternatives.len()).collect::<Vec<_>>();
            // All root actions receive a share. Width only limits subsequent
            // candidate refinement, based on the declared public estimate.
            if !root {
                order.sort_by(|&a, &b| {
                    alternatives[b]
                        .value_estimate
                        .total_cmp(&alternatives[a].value_estimate)
                });
                self.stats.width_pruned += order.len().saturating_sub(self.config.width);
                order.truncate(self.config.width);
            }
            for (slot, &index) in order.iter().enumerate() {
                let allowance = *budget / (order.len() - slot);
                let mut local = allowance;
                let count = successors[index].len();
                for (branch_index, successor) in successors[index].iter().enumerate() {
                    let share = local / (count - branch_index);
                    let mut remaining = share;
                    let (continuation, _) =
                        self.solve(successor, depth - 1, &mut remaining, false)?;
                    local -= share - remaining;
                    alternatives[index].branches[branch_index].continuation = continuation;
                }
                *budget -= allowance - local;
                backup(&mut alternatives[index]);
            }
        }
        let chosen = alternatives
            .iter()
            .enumerate()
            .max_by(|(a, left), (b, right)| {
                left.value_estimate
                    .total_cmp(&right.value_estimate)
                    .then_with(|| b.cmp(a))
            })
            .unwrap()
            .0;
        Ok((
            alternatives[chosen].clone(),
            if root { alternatives } else { vec![] },
        ))
    }
}

fn backup(plan: &mut ContingentPolicy) {
    plan.value_estimate = 0.0;
    plan.leaves = LeafSummary::default();
    for branch in &plan.branches {
        let p = branch.probability;
        let child = &branch.continuation;
        plan.value_estimate += p * child.value_estimate;
        plan.leaves.win_mass += p * child.leaves.win_mass;
        plan.leaves.loss_mass += p * child.leaves.loss_mass;
        plan.leaves.unfinished_mass += p * child.leaves.unfinished_mass;
        plan.leaves.mean_boundary_hp += p * child.leaves.mean_boundary_hp;
    }
}

fn candidates(
    decision: &CombatPublicDecisionV1,
    config: &LookaheadConfig,
) -> (Vec<Choice>, usize, bool) {
    let mut choices = Vec::new();
    let mut plays = Vec::new();
    let mut omitted = 0;
    for (ordinal, action) in decision.actions.atomic_actions.iter().enumerate() {
        match action {
            Atomic::DiscardPotion { .. } => continue,
            Atomic::UsePotion { potion_index, .. }
                if !config.allowed_potion_slots.contains(potion_index) =>
            {
                continue
            }
            Atomic::PlayCard {
                hand_index,
                target_monster_index,
            } => {
                let card = &decision.observation.cards.hand.cards[*hand_index];
                if plays.contains(&(card, target_monster_index)) {
                    omitted += 1;
                    continue;
                }
                // Approximate representative selection, not exact equivalence:
                // discarded duplicate instances can affect a later shuffle.
                plays.push((card, target_monster_index));
            }
            _ => {}
        }
        choices.push(Choice::Atomic {
            action_ordinal: ordinal,
        });
    }
    let mut capped = false;
    for (family_ordinal, family) in decision.actions.selection_families.iter().enumerate() {
        let members = family
            .domain
            .iter()
            .enumerate()
            .filter_map(|(index, member)| match member {
                Member::Card { eligible: true, .. }
                | Member::Scry {
                    currently_present: true,
                    ..
                } => Some(index),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut selections = Vec::new();
        selection_prefixes(
            &members,
            family.declared_min as usize,
            family.effective_max as usize,
            config.selection_limit,
            &mut vec![],
            &mut selections,
        );
        capped |= selections.len() == config.selection_limit;
        choices.extend(
            selections
                .into_iter()
                .map(|selected_domain_ordinals| Choice::Selection {
                    family_ordinal,
                    selected_domain_ordinals,
                }),
        );
    }
    (choices, omitted, capped)
}

fn selection_prefixes(
    members: &[usize],
    min: usize,
    max: usize,
    limit: usize,
    prefix: &mut Vec<usize>,
    output: &mut Vec<Vec<usize>>,
) {
    if output.len() >= limit {
        return;
    }
    if prefix.len() >= min {
        output.push(prefix.clone());
    }
    if prefix.len() >= max {
        return;
    }
    for member in members {
        if !prefix.contains(member) {
            prefix.push(*member);
            selection_prefixes(members, min, max, limit, prefix, output);
            prefix.pop();
            if output.len() >= limit {
                break;
            }
        }
    }
}

/// Replay every branch on the supplied population. Only public observations
/// and transition receipts select a continuation. Work is reported separately
/// from search. A partial policy remains partial after a successful replay.
pub fn replay_policy(
    environment: &CombatBeliefEnvironmentV1,
    policy: &ContingentPolicy,
    stepper: &impl CombatStepper,
    max_engine_steps: usize,
) -> Result<usize, String> {
    let (branches, mut work) = policy_successors(environment, policy, stepper, max_engine_steps)?;
    if branches.len() != policy.branches.len() {
        return Err("policy observation branches differ".into());
    }
    for branch in branches {
        let entry = branch
            .environment()
            .public_history()
            .entries()
            .last()
            .unwrap();
        let matches = policy
            .branches
            .iter()
            .filter(|expected| {
                expected.continuation.boundary == *branch.environment().public_history().current()
                    && expected.draw_pile_changes == entry.draw_pile_changes
                    && expected.trace_complete == entry.draw_order_trace_complete
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 || (matches[0].probability - branch.probability()).abs() > 1e-9 {
            return Err("policy receipt or probability mismatch".into());
        }
        work += replay_policy(
            branch.environment(),
            &matches[0].continuation,
            stepper,
            max_engine_steps,
        )?;
    }
    Ok(work)
}

/// Reconstruct the exact proposal schedule recorded at this policy node, then
/// apply its shared action. Used by policy replay and independently owned execution.
pub fn policy_successors(
    environment: &CombatBeliefEnvironmentV1,
    policy: &ContingentPolicy,
    stepper: &impl CombatStepper,
    max_engine_steps: usize,
) -> Result<(Vec<CombatBeliefChanceBranchV1>, usize), String> {
    if environment.public_history().current() != &policy.boundary {
        return Err("policy observation mismatch".into());
    }
    let refreshed;
    let mut work = 0;
    let environment = if let Some(receipt) = &policy.rejuvenation {
        let bound = receipt.config.work_bound(environment)?;
        let (next, report) =
            environment.rejuvenate_history(&receipt.config, bound, stepper, max_engine_steps)?;
        if report != receipt.report {
            return Err("policy rejuvenation receipt mismatch".into());
        }
        work += report.replay_transitions;
        refreshed = next;
        &refreshed
    } else {
        environment
    };
    let branches = if let Some(action) = &policy.action {
        work += environment.particles().len();
        environment
            .step(action.clone(), stepper, max_engine_steps)
            .map_err(|e| e.to_string())?
    } else {
        vec![]
    };
    Ok((branches, work))
}

#[cfg(test)]
mod tests;
