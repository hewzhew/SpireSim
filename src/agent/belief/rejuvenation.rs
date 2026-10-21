//! Bounded, correlated proposals checked against the entire recorded prefix.
//! This expands a scenario family; it does not estimate a run-seed posterior.

use super::environment::{current_run_context_v1, public_boundary_for_position_v1};
use super::{
    CombatBeliefEnvironmentV1, CombatBeliefParticleOriginV1, CombatBeliefParticleV1,
    CombatPublicHistoryEntryV1, CombatPublicHistoryV1,
};
use crate::runtime::combat::DrawCardKind;
use crate::runtime::rng::StsRng;
use crate::sim::combat::{CombatPosition, CombatStepper};
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryRejuvenationConfig {
    /// Each donor gets the same number of offspring, up to this population cap.
    pub target_particles: usize,
    pub moves_per_particle: usize,
    pub seed: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HistoryRejuvenationReport {
    pub input_particles: usize,
    pub output_particles: usize,
    pub proposals: usize,
    pub accepted: usize,
    pub contradicted_history: usize,
    pub unchanged_proposals: usize,
    pub changed_offspring: usize,
    /// Different exact endpoints may differ only in currently irrelevant RNG.
    /// These are NOT independent observations or an effective sample size.
    pub distinct_endpoints: usize,
    /// Kind and upgrade orders only; swapping two identical Strikes adds zero.
    pub distinct_draw_kind_orders: usize,
    pub replay_transitions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservedSuccessorRescueConfig {
    /// A fixed proposal schedule is evaluated for every donor.
    pub attempts_per_donor: usize,
    pub moves_per_attempt: usize,
    pub max_retained: usize,
    pub seed: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ObservedSuccessorRescueReport {
    pub donors: usize,
    pub proposals: usize,
    pub prefix_compatible_moves: usize,
    pub prefix_contradicted_moves: usize,
    pub compatible_proposals: usize,
    pub first_compatible_proposal: Option<usize>,
    pub retained_proposals: usize,
    pub contradicted_history: usize,
    /// Different exact endpoints among the retained proposals only.
    pub distinct_endpoints: usize,
    pub replay_transitions: usize,
    pub timed_out: bool,
}

impl ObservedSuccessorRescueConfig {
    pub fn work_bound(&self, environment: &CombatBeliefEnvironmentV1) -> Result<usize, String> {
        if self.attempts_per_donor == 0 || self.moves_per_attempt == 0 || self.max_retained == 0 {
            return Err(
                "observed-successor rescue needs positive attempt, move and retention limits"
                    .into(),
            );
        }
        let donors = environment.particles().len();
        let prefix_steps = environment.public_history().entries().len() - 1;
        let successor_steps = prefix_steps
            .checked_add(1)
            .ok_or("history length overflow")?;
        let work_per_attempt = self
            .moves_per_attempt
            .checked_mul(prefix_steps)
            .and_then(|n| n.checked_add(successor_steps))
            .ok_or("observed-successor rescue work overflow")?;
        donors
            .checked_mul(prefix_steps)
            .and_then(|prior| {
                donors
                    .checked_mul(self.attempts_per_donor)?
                    .checked_mul(work_per_attempt)?
                    .checked_add(prior)
            })
            .ok_or("observed-successor rescue work overflow".into())
    }
}

impl HistoryRejuvenationConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.target_particles == 0 || self.moves_per_particle == 0 {
            return Err("history rejuvenation requires positive population and move limits".into());
        }
        Ok(())
    }

    pub fn output_count(&self, donors: usize) -> Result<usize, String> {
        self.validate()?;
        if donors == 0 {
            return Err("history rejuvenation needs donors".into());
        }
        donors
            .checked_mul((self.target_particles / donors).max(1))
            .ok_or("population overflow".into())
    }

    /// Conservative reservation includes validating each donor's provenance.
    /// A complete move schedule is required; budget cannot favor early donors.
    pub fn work_bound(&self, environment: &CombatBeliefEnvironmentV1) -> Result<usize, String> {
        self.output_count(environment.particles().len())?
            .checked_mul(self.moves_per_particle)
            .and_then(|n| n.checked_add(environment.particles().len()))
            .and_then(|n| n.checked_mul(environment.public_history().entries().len() - 1))
            .ok_or("rejuvenation work overflow".into())
    }
}

impl CombatBeliefEnvironmentV1 {
    /// Propose root mutations conditioned on one newly observed PUBLIC successor.
    /// No exact post-action position is accepted. Matched proposals form a
    /// correlated scenario family, not a calibrated posterior or a proof of
    /// complete observation coverage. An empty match is an explicit Unknown.
    pub fn rescue_observed_successor(
        &self,
        observed: &CombatPublicHistoryEntryV1,
        config: &ObservedSuccessorRescueConfig,
        work: usize,
        duration: Duration,
        stepper: &impl CombatStepper,
        max_engine_steps: usize,
    ) -> Result<(Option<Self>, ObservedSuccessorRescueReport), String> {
        if max_engine_steps == 0 {
            return Err("engine step limit must be positive".into());
        }
        if duration.is_zero() {
            return Err("observed-successor rescue needs positive wall time".into());
        }
        if work < config.work_bound(self)? {
            return Err("Unknown: insufficient work for observed-successor rescue".into());
        }
        let deadline = Instant::now()
            .checked_add(duration)
            .ok_or("observed-successor rescue deadline overflow")?;
        let action = observed
            .chosen_action
            .clone()
            .ok_or("observed successor has no chosen public action")?;
        let history = self.public_history().successor(
            action,
            observed.boundary.clone(),
            observed.draw_pile_changes.clone(),
            observed.draw_order_trace_complete,
        );
        let mut report = ObservedSuccessorRescueReport {
            donors: self.particles().len(),
            ..Default::default()
        };
        for donor in self.particles() {
            if Instant::now() >= deadline {
                report.timed_out = true;
                return Ok((None, report));
            }
            let root = donor
                .reconstruction_root
                .as_ref()
                .ok_or("observed-successor rescue requires a sampled reconstruction root")?;
            let endpoint = replay_root(
                root,
                self.public_history(),
                stepper,
                max_engine_steps,
                &mut report.replay_transitions,
            )?;
            if endpoint.as_ref() != Some(donor.private_position()) {
                return Err("donor reconstruction does not reproduce its recorded endpoint".into());
            }
        }
        let mut rng = StsRng::new(config.seed);
        let mut retention_rng = StsRng::new(config.seed ^ 0x8e65_9ca9_5dbb_4f0b);
        let mut matches = Vec::new();
        for (donor_index, donor) in self.particles().iter().enumerate() {
            let root = donor.reconstruction_root.as_ref().unwrap();
            for attempt_index in 0..config.attempts_per_donor {
                if Instant::now() >= deadline {
                    report.timed_out = true;
                    return Ok((None, report));
                }
                let mut candidate = (**root).clone();
                for _ in 0..config.moves_per_attempt {
                    if Instant::now() >= deadline {
                        report.timed_out = true;
                        return Ok((None, report));
                    }
                    let proposal_seed = rng.random_long() as u64;
                    let proposed = propose(&candidate, proposal_seed);
                    if replay_root(
                        &proposed,
                        self.public_history(),
                        stepper,
                        max_engine_steps,
                        &mut report.replay_transitions,
                    )?
                    .is_some()
                    {
                        report.prefix_compatible_moves += 1;
                        candidate = proposed;
                    } else {
                        report.prefix_contradicted_moves += 1;
                    }
                }
                report.proposals += 1;
                match replay_root(
                    &candidate,
                    &history,
                    stepper,
                    max_engine_steps,
                    &mut report.replay_transitions,
                )? {
                    None => report.contradicted_history += 1,
                    Some(endpoint) => {
                        if report.first_compatible_proposal.is_none() {
                            report.first_compatible_proposal = Some(report.proposals);
                        }
                        report.compatible_proposals += 1;
                        let mut child = CombatBeliefParticleV1::from_private_position(
                            CombatBeliefParticleOriginV1::ObservedSuccessorRescue {
                                schedule_seed: config.seed,
                                donor_index,
                                attempt_index,
                            },
                            donor.probability_mass() / config.attempts_per_donor as f64,
                            endpoint,
                        );
                        child.reconstruction_root = Some(Arc::new(candidate));
                        if matches.len() < config.max_retained {
                            matches.push(child);
                        } else {
                            let slot = (retention_rng.random_long() as u64)
                                % u64::try_from(report.compatible_proposals)
                                    .map_err(|_| "compatible proposal count exceeds u64")?;
                            if slot < config.max_retained as u64 {
                                matches[slot as usize] = child;
                            }
                        }
                    }
                }
            }
        }
        // The last replay can cross the deadline after its pre-call check.
        // Never publish a partially time-bounded schedule as complete.
        if Instant::now() >= deadline {
            report.timed_out = true;
            return Ok((None, report));
        }
        let mut endpoints = Vec::new();
        for child in &matches {
            if !endpoints.contains(&child.private_position()) {
                endpoints.push(child.private_position());
            }
        }
        report.distinct_endpoints = endpoints.len();
        report.retained_proposals = matches.len();
        if matches.is_empty() {
            return Ok((None, report));
        }
        let environment = Self::from_particles(history, matches).map_err(|e| e.to_string())?;
        Ok((Some(environment), report))
    }

    /// Symmetric draw swaps / replacement of one of eight initial RNG streams,
    /// followed by full prefix replay. Only compatible proposals are retained.
    /// Equal offspring counts preserve each donor family's total mass. Finite
    /// mixing, donor bias and the independent-stream assumption remain explicit.
    pub fn rejuvenate_history(
        &self,
        config: &HistoryRejuvenationConfig,
        work: usize,
        stepper: &impl CombatStepper,
        max_engine_steps: usize,
    ) -> Result<(Self, HistoryRejuvenationReport), String> {
        if max_engine_steps == 0 {
            return Err("engine step limit must be positive".into());
        }
        if work < config.work_bound(self)? {
            return Err("Unknown: insufficient work for complete history rejuvenation".into());
        }
        let copies = config.output_count(self.particles().len())? / self.particles().len();
        let mut report = HistoryRejuvenationReport {
            input_particles: self.particles().len(),
            ..Default::default()
        };
        // Missing roots and instrumentation failures are errors, not rejected
        // statistical samples. Validate every donor before changing any weights.
        for donor in self.particles() {
            let root = donor.reconstruction_root.as_ref().ok_or(
                "history rejuvenation requires a sampled reconstruction root; later-boundary samples cannot supply it"
            )?;
            let endpoint = replay_root(
                root,
                self.public_history(),
                stepper,
                max_engine_steps,
                &mut report.replay_transitions,
            )?;
            if endpoint.as_ref() != Some(donor.private_position()) {
                return Err(
                    "donor reconstruction does not reproduce its recorded history and endpoint"
                        .into(),
                );
            }
        }
        let mut rng = StsRng::new(config.seed);
        let mut output = Vec::new();
        for donor in self.particles() {
            for _ in 0..copies {
                let mut child = donor.clone();
                for _ in 0..config.moves_per_particle {
                    report.proposals += 1;
                    let seed = rng.random_long() as u64;
                    let root = child.reconstruction_root.as_ref().unwrap();
                    let candidate = propose(root, seed);
                    if candidate == **root {
                        report.unchanged_proposals += 1;
                        continue;
                    }
                    match replay_root(
                        &candidate,
                        self.public_history(),
                        stepper,
                        max_engine_steps,
                        &mut report.replay_transitions,
                    )? {
                        None => report.contradicted_history += 1,
                        Some(endpoint) => {
                            report.accepted += 1;
                            child = CombatBeliefParticleV1::from_private_position(
                                CombatBeliefParticleOriginV1::HistoryReplayedMutation {
                                    proposal_seed: seed,
                                },
                                donor.probability_mass(),
                                endpoint,
                            );
                            child.reconstruction_root = Some(Arc::new(candidate));
                        }
                    }
                }
                report.changed_offspring +=
                    usize::from(child.private_position() != donor.private_position());
                child.set_probability_mass(donor.probability_mass() / copies as f64);
                output.push(child);
            }
        }
        report.output_particles = output.len();
        let mut endpoints = Vec::new();
        let mut orders = Vec::new();
        for child in &output {
            let position = child.private_position();
            if !endpoints.contains(&position) {
                endpoints.push(position);
            }
            let order = position
                .combat
                .zones
                .draw_pile
                .iter()
                .map(DrawCardKind::from)
                .collect::<Vec<_>>();
            if !orders.contains(&order) {
                orders.push(order);
            }
        }
        report.distinct_endpoints = endpoints.len();
        report.distinct_draw_kind_orders = orders.len();
        let result = Self::from_particles(self.public_history().clone(), output)
            .map_err(|e| e.to_string())?;
        Ok((result, report))
    }
}

fn propose(root: &CombatPosition, seed: u64) -> CombatPosition {
    let mut candidate = root.clone();
    let mut rng = StsRng::new(seed);
    if rng.random(1) == 0 {
        let draw = &mut candidate.combat.zones.draw_pile;
        if draw.len() >= 2 && draw.len() <= i32::MAX as usize {
            let a = rng.random(draw.len() as i32 - 1) as usize;
            let mut b = rng.random(draw.len() as i32 - 2) as usize;
            if b >= a {
                b += 1;
            }
            draw.swap(a, b);
        }
    } else {
        let pool = &mut candidate.combat.rng;
        let stream = match rng.random(7) {
            0 => &mut pool.ai_rng,
            1 => &mut pool.shuffle_rng,
            2 => &mut pool.card_random_rng,
            3 => &mut pool.misc_rng,
            4 => &mut pool.math_rng,
            5 => &mut pool.monster_hp_rng,
            6 => &mut pool.card_rng,
            _ => &mut pool.potion_rng,
        };
        *stream = StsRng::new_with_counter(rng.random_long() as u64, stream.counter);
    }
    candidate
}

fn replay_root(
    root: &CombatPosition,
    history: &CombatPublicHistoryV1,
    stepper: &impl CombatStepper,
    max_engine_steps: usize,
    work: &mut usize,
) -> Result<Option<CombatPosition>, String> {
    let first = &history.entries()[0].boundary;
    let actual = public_boundary_for_position_v1(current_run_context_v1(first).clone(), root)
        .map_err(|e| e.to_string())?;
    if actual != *first {
        return Ok(None);
    }
    let mut prefix = CombatPublicHistoryV1::root(actual);
    let mut position = root.clone();
    for expected in &history.entries()[1..] {
        let action = expected
            .chosen_action
            .clone()
            .ok_or("missing historical action")?;
        *work += 1;
        let (next_history, next) = prefix
            .advance_exact(&position, action, stepper, max_engine_steps)
            .map_err(|e| e.to_string())?;
        let entry = next_history.entries().last().unwrap();
        if expected.draw_order_trace_complete && !entry.draw_order_trace_complete {
            return Err(
                "cannot check recorded observations with incomplete replay coverage".into(),
            );
        }
        if entry.boundary != expected.boundary
            || (expected.draw_order_trace_complete
                && entry.draw_pile_changes != expected.draw_pile_changes)
        {
            return Ok(None);
        }
        prefix = next_history;
        position = next;
    }
    Ok(Some(position))
}

#[cfg(test)]
mod tests;
