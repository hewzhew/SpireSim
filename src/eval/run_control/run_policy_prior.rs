use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    DecisionCandidateKey, ExactRunPolicyActionSuccessorV1, ExactRunPolicyDecisionV1,
    RunControlSession, RunDecisionAction,
};

const RANKED_POLICY_UNIFORM_MIX_V1: f64 = 0.05;
const RANKED_POLICY_DECAY_V1: f64 = 0.25;
const NORMALIZED_PROBABILITY_TOLERANCE_V1: f64 = 1.0e-9;

/// Stable identity for a run-policy prior whose behavior can affect durable
/// frontier scheduling. Function pointers themselves are deliberately not
/// serialized.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPolicyPriorIdentityV1 {
    UniformExactSurfaceV1,
}

/// One exact legal action presented to a run policy.
///
/// Policies may rank these actions, but they do not create, remove, or mutate
/// them. The exact model remains the sole source of action legality.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunPolicyCandidateV1<'a> {
    pub candidate_id: &'a str,
    pub label: &'a str,
    pub action: &'a RunDecisionAction,
}

/// Positive policy support for one exact legal action.
#[derive(Clone, Debug, PartialEq)]
pub struct RunActionPriorV1 {
    pub candidate_id: String,
    pub probability: f64,
}

/// A complete, normalized policy distribution over one exact decision surface.
///
/// Entry order is the policy rank used by discrepancy search. Every legal
/// candidate must appear exactly once with finite, strictly positive support.
#[derive(Clone, Debug, PartialEq)]
pub struct RunPolicyPriorV1 {
    pub entries: Vec<RunActionPriorV1>,
}

pub type RunPolicyPriorFnV1 =
    for<'a> fn(&RunControlSession, &[RunPolicyCandidateV1<'a>]) -> Result<RunPolicyPriorV1, String>;

/// Validates a complete or owner-only legal surface and returns the exact
/// actions owned by one typed policy. Auxiliary actions remain legal and later
/// receive positive support, but never enter an unrelated owner's parser.
pub fn owned_run_policy_actions_v1<'a>(
    exact: &'a ExactRunPolicyDecisionV1,
    legal: &[RunPolicyCandidateV1<'_>],
    owner: &str,
    owns: fn(&DecisionCandidateKey) -> bool,
) -> Result<Vec<(usize, &'a ExactRunPolicyActionSuccessorV1)>, String> {
    let owned = exact
        .actions
        .iter()
        .enumerate()
        .filter(|(_, action)| action.candidate_key.as_ref().is_some_and(owns))
        .collect::<Vec<_>>();
    let owned_ids = owned
        .iter()
        .map(|(_, action)| action.candidate_id.as_str())
        .collect::<BTreeSet<_>>();
    let legal_ids = legal
        .iter()
        .map(|candidate| candidate.candidate_id)
        .collect::<BTreeSet<_>>();

    if owned_ids.is_empty() || legal_ids.len() != legal.len() || !owned_ids.is_subset(&legal_ids) {
        return Err(format!(
            "{owner} policy surface disagrees with exact ownership: owned={} policy={}",
            owned_ids.len(),
            legal_ids.len()
        ));
    }
    Ok(owned)
}

impl RunPolicyPriorV1 {
    pub fn validate_for(&self, legal: &[RunPolicyCandidateV1<'_>]) -> Result<(), String> {
        if legal.is_empty() {
            return Err("run policy prior requires at least one legal candidate".to_string());
        }
        if self.entries.len() != legal.len() {
            return Err(format!(
                "run policy prior returned {} entries for {} legal candidates",
                self.entries.len(),
                legal.len()
            ));
        }

        let legal_ids = legal
            .iter()
            .map(|candidate| candidate.candidate_id)
            .collect::<BTreeSet<_>>();
        if legal_ids.len() != legal.len() {
            return Err("exact decision surface contains duplicate candidate ids".to_string());
        }

        let mut prior_ids = BTreeSet::new();
        let mut probability_sum = 0.0;
        for entry in &self.entries {
            if !legal_ids.contains(entry.candidate_id.as_str()) {
                return Err(format!(
                    "run policy prior returned non-legal candidate '{}'",
                    entry.candidate_id
                ));
            }
            if !prior_ids.insert(entry.candidate_id.as_str()) {
                return Err(format!(
                    "run policy prior duplicated candidate '{}'",
                    entry.candidate_id
                ));
            }
            if !entry.probability.is_finite() || entry.probability <= 0.0 {
                return Err(format!(
                    "run policy prior assigned non-positive or non-finite support to '{}'",
                    entry.candidate_id
                ));
            }
            probability_sum += entry.probability;
        }
        if (probability_sum - 1.0).abs() > NORMALIZED_PROBABILITY_TOLERANCE_V1 {
            return Err(format!(
                "run policy prior probabilities sum to {probability_sum}, expected 1"
            ));
        }
        Ok(())
    }
}

/// Gives every exact legal candidate equal positive support while preserving
/// the decision surface's stable enumeration order.
///
/// This is a scheduling fallback, not strategic advice: it neither inspects
/// private successors nor claims that an earlier candidate is better.
pub fn uniform_run_policy_prior_v1(
    _session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    if legal.is_empty() {
        return Err("cannot build a policy prior for an empty decision surface".to_string());
    }
    let probability = 1.0 / legal.len() as f64;
    let prior = RunPolicyPriorV1 {
        entries: legal
            .iter()
            .map(|candidate| RunActionPriorV1 {
                candidate_id: candidate.candidate_id.to_string(),
                probability,
            })
            .collect(),
    };
    prior.validate_for(legal)?;
    Ok(prior)
}

/// Converts a preferred candidate order into the current positive-support
/// ranked prior. Unknown and duplicate preferred ids are ignored; every legal
/// candidate is then appended in exact-surface order.
///
/// This helper preserves the existing oracle behavior while legacy owners are
/// migrated. New policies may return their own complete normalized priors.
pub fn positive_ranked_run_policy_prior_v1(
    legal: &[RunPolicyCandidateV1<'_>],
    preferred_candidate_ids: impl IntoIterator<Item = String>,
) -> Result<RunPolicyPriorV1, String> {
    if legal.is_empty() {
        return Err("cannot build a policy prior for an empty decision surface".to_string());
    }

    let mut legal_indices = BTreeMap::new();
    for (index, candidate) in legal.iter().enumerate() {
        if legal_indices
            .insert(candidate.candidate_id, index)
            .is_some()
        {
            return Err(format!(
                "exact decision surface duplicated candidate '{}'",
                candidate.candidate_id
            ));
        }
    }

    let mut ordered_indices = Vec::with_capacity(legal.len());
    let mut selected_indices = BTreeSet::new();
    for candidate_id in preferred_candidate_ids {
        if let Some(index) = legal_indices.get(candidate_id.as_str()).copied() {
            if selected_indices.insert(index) {
                ordered_indices.push(index);
            }
        }
    }
    for index in 0..legal.len() {
        if selected_indices.insert(index) {
            ordered_indices.push(index);
        }
    }

    let raw = (0..legal.len())
        .map(|rank| RANKED_POLICY_DECAY_V1.powi(rank as i32))
        .collect::<Vec<_>>();
    let raw_sum = raw.iter().sum::<f64>();
    let uniform = 1.0 / legal.len() as f64;
    let entries = ordered_indices
        .into_iter()
        .enumerate()
        .map(|(rank, index)| RunActionPriorV1 {
            candidate_id: legal[index].candidate_id.to_string(),
            probability: (1.0 - RANKED_POLICY_UNIFORM_MIX_V1) * raw[rank] / raw_sum
                + RANKED_POLICY_UNIFORM_MIX_V1 * uniform,
        })
        .collect();
    let prior = RunPolicyPriorV1 { entries };
    prior.validate_for(legal)?;
    Ok(prior)
}

/// Converts ordered preference groups into a complete positive-support prior.
/// Candidates inside one group receive identical support; group order alone
/// carries policy preference. Unknown legal candidates are retained together
/// in one final group in exact-surface order.
pub fn positive_grouped_run_policy_prior_v1(
    legal: &[RunPolicyCandidateV1<'_>],
    preferred_candidate_groups: impl IntoIterator<Item = Vec<String>>,
) -> Result<RunPolicyPriorV1, String> {
    if legal.is_empty() {
        return Err("cannot build a policy prior for an empty decision surface".to_string());
    }

    let mut legal_indices = BTreeMap::new();
    for (index, candidate) in legal.iter().enumerate() {
        if legal_indices
            .insert(candidate.candidate_id, index)
            .is_some()
        {
            return Err(format!(
                "exact decision surface duplicated candidate '{}'",
                candidate.candidate_id
            ));
        }
    }

    let mut groups = Vec::new();
    let mut selected_indices = BTreeSet::new();
    for candidate_group in preferred_candidate_groups {
        let group = candidate_group
            .into_iter()
            .filter_map(|candidate_id| legal_indices.get(candidate_id.as_str()).copied())
            .filter(|index| selected_indices.insert(*index))
            .collect::<Vec<_>>();
        if !group.is_empty() {
            groups.push(group);
        }
    }
    let unranked = (0..legal.len())
        .filter(|index| selected_indices.insert(*index))
        .collect::<Vec<_>>();
    if !unranked.is_empty() {
        groups.push(unranked);
    }

    let raw_sum = groups
        .iter()
        .enumerate()
        .map(|(rank, group)| RANKED_POLICY_DECAY_V1.powi(rank as i32) * group.len() as f64)
        .sum::<f64>();
    let uniform = 1.0 / legal.len() as f64;
    let entries = groups
        .into_iter()
        .enumerate()
        .flat_map(|(rank, group)| {
            let probability = (1.0 - RANKED_POLICY_UNIFORM_MIX_V1)
                * RANKED_POLICY_DECAY_V1.powi(rank as i32)
                / raw_sum
                + RANKED_POLICY_UNIFORM_MIX_V1 * uniform;
            group.into_iter().map(move |index| RunActionPriorV1 {
                candidate_id: legal[index].candidate_id.to_string(),
                probability,
            })
        })
        .collect();
    let prior = RunPolicyPriorV1 { entries };
    prior.validate_for(legal)?;
    Ok(prior)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::core::ClientInput;

    fn candidate<'a>(
        candidate_id: &'a str,
        action: &'a RunDecisionAction,
    ) -> RunPolicyCandidateV1<'a> {
        RunPolicyCandidateV1 {
            candidate_id,
            label: candidate_id,
            action,
        }
    }

    #[test]
    fn ranked_prior_keeps_every_legal_candidate_positive() {
        let action = RunDecisionAction::Input(ClientInput::Proceed);
        let legal = vec![
            candidate("first", &action),
            candidate("second", &action),
            candidate("third", &action),
        ];
        let prior = positive_ranked_run_policy_prior_v1(
            &legal,
            ["second".to_string(), "unknown".to_string()],
        )
        .expect("ranked prior");

        assert_eq!(prior.entries[0].candidate_id, "second");
        assert_eq!(prior.entries.len(), legal.len());
        assert!(prior.entries.iter().all(|entry| entry.probability > 0.0));
        assert!(
            (prior
                .entries
                .iter()
                .map(|entry| entry.probability)
                .sum::<f64>()
                - 1.0)
                .abs()
                < NORMALIZED_PROBABILITY_TOLERANCE_V1
        );
    }

    #[test]
    fn uniform_prior_preserves_surface_order_without_strategic_preference() {
        let session = RunControlSession::new(Default::default());
        let action = RunDecisionAction::Input(ClientInput::Proceed);
        let legal = vec![candidate("first", &action), candidate("second", &action)];

        let prior = uniform_run_policy_prior_v1(&session, &legal).expect("uniform prior");

        assert_eq!(prior.entries[0].candidate_id, "first");
        assert_eq!(prior.entries[1].candidate_id, "second");
        assert_eq!(prior.entries[0].probability, prior.entries[1].probability);
    }

    #[test]
    fn grouped_prior_preserves_semantic_ties_without_dropping_unknown_actions() {
        let action = RunDecisionAction::Input(ClientInput::Proceed);
        let legal = vec![
            candidate("first", &action),
            candidate("second", &action),
            candidate("third", &action),
            candidate("fourth", &action),
        ];
        let prior = positive_grouped_run_policy_prior_v1(
            &legal,
            [
                vec!["second".to_string(), "first".to_string()],
                vec!["third".to_string()],
            ],
        )
        .expect("grouped prior");

        assert_eq!(prior.entries[0].candidate_id, "second");
        assert_eq!(prior.entries[1].candidate_id, "first");
        assert_eq!(prior.entries[2].candidate_id, "third");
        assert_eq!(prior.entries[3].candidate_id, "fourth");
        assert_eq!(prior.entries[0].probability, prior.entries[1].probability);
        assert!(prior.entries[1].probability > prior.entries[2].probability);
        assert!(prior.entries[2].probability > prior.entries[3].probability);
        prior.validate_for(&legal).expect("valid grouped prior");
    }

    #[test]
    fn prior_validation_rejects_missing_or_zero_support() {
        let action = RunDecisionAction::Input(ClientInput::Proceed);
        let legal = vec![candidate("first", &action), candidate("second", &action)];
        let prior = RunPolicyPriorV1 {
            entries: vec![
                RunActionPriorV1 {
                    candidate_id: "first".to_string(),
                    probability: 1.0,
                },
                RunActionPriorV1 {
                    candidate_id: "second".to_string(),
                    probability: 0.0,
                },
            ],
        };

        assert!(prior.validate_for(&legal).is_err());
    }
}
