//! Small frozen two-level log-linear prior. Only feature facts are hand chosen;
//! all group/member preferences are weights supplied by an offline learner.
mod features;
pub(crate) use features::encode_entities;
pub use features::{encode, LinearInput, FEATURE_COUNT, SCHEMA};

use super::{log_softmax, prior, PathContext, PathPrior};
use crate::{CombatBranch, CombatSearchState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearWeights {
    pub schema: u32,
    pub group: Vec<f64>,
    pub member: Vec<f64>,
}

pub struct FrozenLinearPrior {
    weights: LinearWeights,
    revision: String,
}

impl FrozenLinearPrior {
    pub fn new(weights: LinearWeights) -> Result<Self, String> {
        if weights.schema != SCHEMA
            || weights.group.len() != FEATURE_COUNT
            || weights.member.len() != FEATURE_COUNT
            || weights
                .group
                .iter()
                .chain(&weights.member)
                .any(|v| !v.is_finite())
        {
            return Err("invalid linear prior schema, shape or weights".into());
        }
        let revision = format!(
            "path-linear-v{SCHEMA}-{:x}",
            Sha256::digest(serde_json::to_vec(&weights).map_err(|e| e.to_string())?)
        );
        Ok(Self { weights, revision })
    }

    pub fn score(&self, input: &LinearInput) -> Result<Vec<f64>, String> {
        input.validate()?;
        let count = input.groups.iter().max().unwrap() + 1;
        let mut members = vec![Vec::new(); count];
        let mut group_logits = vec![0.0; count];
        let mut member_logits = Vec::new();
        for (a, row) in input.rows.iter().enumerate() {
            let g = input.groups[a];
            members[g].push(a);
            group_logits[g] += row
                .iter()
                .map(|(i, v)| self.weights.group[*i] * v)
                .sum::<f64>();
            member_logits.push(
                row.iter()
                    .map(|(i, v)| self.weights.member[*i] * v)
                    .sum::<f64>(),
            );
        }
        for (logit, members) in group_logits.iter_mut().zip(&members) {
            *logit /= members.len() as f64;
        }
        let group_logs = log_softmax(&group_logits)?;
        let mut logs = vec![0.0; input.rows.len()];
        for (g, members) in members.iter().enumerate() {
            let within = log_softmax(
                &members
                    .iter()
                    .map(|i| member_logits[*i])
                    .collect::<Vec<_>>(),
            )?;
            for (i, log) in members.iter().zip(within) {
                logs[*i] = group_logs[g] + log;
            }
        }
        prior::validate(&logs, input.rows.len())?;
        Ok(logs)
    }
}

impl PathPrior for FrozenLinearPrior {
    fn revision(&self) -> &str {
        &self.revision
    }
    fn log_probabilities(
        &self,
        state: &CombatSearchState,
        context: PathContext<'_>,
        branches: &[CombatBranch],
    ) -> Result<Vec<f64>, String> {
        self.score(&encode(state, context, branches)?)
    }
}

#[cfg(test)]
mod tests;
