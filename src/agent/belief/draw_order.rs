//! Uniform kind orders subject to partial order facts, not a history posterior.
//!
//! Greedy subsequence recognition counts each order once even when an order
//! contains several embeddings of the remembered sequence. Unconstrained
//! kinds share one count bucket; this never enumerates their permutations.

use std::collections::HashMap;

use crate::agent::information::draw_order::DrawOrderKnowledge;
use crate::runtime::combat::DrawCardKind;
use crate::runtime::rng::StsRng;

use super::combat::CombatBeliefSamplingErrorV1 as Error;

pub(super) struct DrawOrderDistribution {
    prefix: Vec<DrawCardKind>,
    suffix: Vec<DrawCardKind>,
    kinds: Vec<DrawCardKind>,
    other: Vec<DrawCardKind>,
    counts: Vec<usize>,
    required: Vec<usize>,
    remaining_required: Vec<Vec<usize>>,
    probabilities: HashMap<(Vec<usize>, usize), f64>,
    max_states: usize,
}

impl DrawOrderDistribution {
    pub(super) fn new(knowledge: &DrawOrderKnowledge, max_states: usize) -> Result<Self, Error> {
        // Bounds recursion and f64 conditioning probabilities (1 / 128! is
        // still representable). This is a sampling limit, not a game limit.
        if knowledge.composition().len() > 128 {
            return Err(Error::DrawPileTooLarge);
        }
        let prefix = knowledge.known_prefix().to_vec();
        let suffix = if prefix.len() == knowledge.composition().len() {
            Vec::new()
        } else {
            knowledge.known_suffix().to_vec()
        };
        let required = &knowledge.ordered_subsequence()
            [prefix.len()..knowledge.ordered_subsequence().len() - suffix.len()];
        let mut middle = knowledge.composition().to_vec();
        for card in prefix.iter().chain(&suffix) {
            let index = middle
                .iter()
                .position(|item| item == card)
                .ok_or(Error::HistoryBoundaryMismatch)?;
            middle.remove(index);
        }
        let mut kinds = required.to_vec();
        kinds.sort_by_key(|kind| (kind.card_id.stable_ordinal(), kind.upgrades));
        kinds.dedup();
        let mut counts = vec![0; kinds.len() + 1];
        let mut other = Vec::new();
        for card in middle {
            let group = kinds.iter().position(|kind| *kind == card);
            counts[group.unwrap_or(kinds.len())] += 1;
            if group.is_none() {
                other.push(card);
            }
        }
        let required = required
            .iter()
            .map(|card| kinds.iter().position(|kind| kind == card).unwrap())
            .collect::<Vec<_>>();
        let mut remaining_required = vec![vec![0; kinds.len()]; required.len() + 1];
        for matched in (0..required.len()).rev() {
            remaining_required[matched] = remaining_required[matched + 1].clone();
            remaining_required[matched][required[matched]] += 1;
        }
        let mut distribution = Self {
            prefix,
            suffix,
            kinds,
            other,
            counts,
            required,
            remaining_required,
            probabilities: HashMap::new(),
            max_states,
        };
        let probability = distribution.completion_probability(&distribution.counts.clone(), 0)?;
        if !probability.is_finite() || probability <= 0.0 {
            return Err(Error::DrawOrderConditioningFailed);
        }
        Ok(distribution)
    }

    /// Probability that a uniform random completion contains the remaining
    /// sequence. Each symbol advances the greedy recognizer at most once.
    fn completion_probability(&mut self, counts: &[usize], matched: usize) -> Result<f64, Error> {
        if matched == self.required.len() {
            return Ok(1.0);
        }
        if counts
            .iter()
            .zip(&self.remaining_required[matched])
            .any(|(available, needed)| available < needed)
        {
            return Ok(0.0);
        }
        let key = (counts.to_vec(), matched);
        if let Some(probability) = self.probabilities.get(&key) {
            return Ok(*probability);
        }
        if self.probabilities.len() >= self.max_states {
            return Err(Error::DrawOrderConditioningBudgetExceeded {
                max_states: self.max_states,
            });
        }
        // Reserve before recursion, so unfinished calls also consume budget.
        self.probabilities.insert(key.clone(), 0.0);
        let total = counts.iter().sum::<usize>() as f64;
        let mut probability = 0.0;
        let mut next = counts.to_vec();
        for (group, count) in counts.iter().copied().enumerate().filter(|(_, n)| *n > 0) {
            next[group] -= 1;
            let advanced = matched + usize::from(self.required[matched] == group);
            probability += count as f64 / total * self.completion_probability(&next, advanced)?;
            next[group] += 1;
        }
        self.probabilities.insert(key, probability);
        Ok(probability)
    }

    pub(super) fn sample(&mut self, rng: &mut StsRng) -> Result<Vec<DrawCardKind>, Error> {
        let mut order = self.prefix.clone();
        let mut counts = self.counts.clone();
        let mut other = self.other.clone();
        let mut matched = 0;
        while counts.iter().any(|count| *count > 0) {
            let mut weights = vec![0.0; counts.len()];
            for group in 0..counts.len() {
                if counts[group] == 0 {
                    continue;
                }
                let count = counts[group];
                counts[group] -= 1;
                let advanced = matched + usize::from(self.required.get(matched) == Some(&group));
                weights[group] = count as f64 * self.completion_probability(&counts, advanced)?;
                counts[group] += 1;
            }
            let total = weights.iter().sum::<f64>();
            if !total.is_finite() || total <= 0.0 {
                return Err(Error::DrawOrderConditioningFailed);
            }
            let unit = (rng.random_long() >> 11) as f64 / (1_u64 << 53) as f64;
            let mut threshold = unit * total;
            // Last positive weight is also a rounding-safe fallback.
            let mut chosen = weights.iter().rposition(|weight| *weight > 0.0).unwrap();
            for (group, weight) in weights.iter().enumerate() {
                if threshold < *weight {
                    chosen = group;
                    break;
                }
                threshold -= weight;
            }
            counts[chosen] -= 1;
            matched += usize::from(self.required.get(matched) == Some(&chosen));
            order.push(if let Some(kind) = self.kinds.get(chosen) {
                *kind
            } else {
                let index = rng.random(other.len() as i32 - 1) as usize;
                other.swap_remove(index)
            });
        }
        order.extend_from_slice(&self.suffix);
        Ok(order)
    }
}

#[cfg(test)]
mod tests;
