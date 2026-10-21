use crate::{CombatBranch, CombatSearchState, ResourceRequirement};
use sts_core::content::{cards::CardId, potions::PotionId};
use sts_core::sim::combat::CombatPosition;
use sts_core::state::core::ClientInput;

/// Frozen task plus prefix facts. The root supplies original resource identity;
/// encoders must not expose raw RNG, UUIDs or source identifiers as features.
#[derive(Clone, Copy)]
pub struct PathContext<'a> {
    pub root: &'a CombatPosition,
    pub goal: &'a ResourceRequirement,
    pub spent_originals: u64,
}

/// Frozen, deterministic guidance for one search. It may inspect the supplied
/// goal/ledger, but must not update parameters or invoke search while scoring.
/// Output is normalized log probability over every supplied concrete branch.
pub trait PathPrior {
    fn revision(&self) -> &str;
    fn log_probabilities(
        &self,
        state: &CombatSearchState,
        context: PathContext<'_>,
        branches: &[CombatBranch],
    ) -> Result<Vec<f64>, String>;
}

impl<P: PathPrior + ?Sized> PathPrior for Box<P> {
    fn revision(&self) -> &str {
        (**self).revision()
    }
    fn log_probabilities(
        &self,
        state: &CombatSearchState,
        context: PathContext<'_>,
        branches: &[CombatBranch],
    ) -> Result<Vec<f64>, String> {
        (**self).log_probabilities(state, context, branches)
    }
}

/// Stable log normalization retains low-probability legal choices even when
/// their probability would underflow as an ordinary floating point number.
pub fn log_softmax(logits: &[f64]) -> Result<Vec<f64>, String> {
    if logits.is_empty() || logits.iter().any(|x| !x.is_finite()) {
        return Err("path prior needs nonempty finite logits".into());
    }
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let shifted: Vec<_> = logits.iter().map(|x| x - max).collect();
    if shifted.iter().any(|x| !x.is_finite()) {
        return Err("path prior logit range exceeds numeric limits".into());
    }
    let normalizer = shifted.iter().map(|x| x.exp()).sum::<f64>().ln();
    Ok(shifted.iter().map(|x| x - normalizer).collect())
}

pub(super) fn validate(logs: &[f64], count: usize) -> Result<(), String> {
    if logs.len() != count || logs.is_empty() || logs.iter().any(|x| !x.is_finite() || *x > 0.0) {
        return Err("path prior must assign finite log probability to every legal choice".into());
    }
    let max = logs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let log_sum = max + logs.iter().map(|x| (x - max).exp()).sum::<f64>().ln();
    if log_sum.abs() > 1e-9 {
        return Err("path prior log probabilities are not normalized".into());
    }
    Ok(())
}

/// A representation baseline, not trained guidance. Card plays share group
/// mass by CardId; potion operations by PotionId. Other choices are individual
/// groups. Concrete instances and targets remain separate within each group.
pub struct GroupedNeutralPrior;

#[derive(PartialEq)]
enum Group {
    Play(CardId),
    Use(PotionId),
    Discard(PotionId),
    Other(usize),
}

impl PathPrior for GroupedNeutralPrior {
    fn revision(&self) -> &str {
        "grouped-neutral-v1"
    }

    fn log_probabilities(
        &self,
        state: &CombatSearchState,
        _context: PathContext<'_>,
        branches: &[CombatBranch],
    ) -> Result<Vec<f64>, String> {
        let indices = choice_groups(state, branches)?;
        let mut counts = vec![0; indices.iter().max().map_or(0, |g| g + 1)];
        for &group in &indices {
            counts[group] += 1;
        }
        let group_log = -(counts.len() as f64).ln();
        Ok(indices
            .iter()
            .map(|g| group_log - (counts[*g] as f64).ln())
            .collect())
    }
}

/// Dense group IDs in choice order, shared by the baseline prior and dataset.
/// Grouping affects probability mass only; it never merges concrete branches.
pub fn choice_groups(
    state: &CombatSearchState,
    branches: &[CombatBranch],
) -> Result<Vec<usize>, String> {
    let c = &state.position().combat;
    let mut groups = Vec::new();
    let mut indices = Vec::new();
    for (index, branch) in branches.iter().enumerate() {
        let group = match branch {
            CombatBranch::Input(ClientInput::PlayCard { card_index, .. }) => Group::Play(
                c.zones
                    .hand
                    .get(*card_index)
                    .ok_or("missing prior card")?
                    .id,
            ),
            CombatBranch::Input(ClientInput::UsePotion { potion_index, .. }) => Group::Use(
                c.entities
                    .potions
                    .get(*potion_index)
                    .and_then(Option::as_ref)
                    .ok_or("missing prior potion")?
                    .id,
            ),
            CombatBranch::Input(ClientInput::DiscardPotion(slot)) => Group::Discard(
                c.entities
                    .potions
                    .get(*slot)
                    .and_then(Option::as_ref)
                    .ok_or("missing prior potion")?
                    .id,
            ),
            _ => Group::Other(index),
        };
        let g = groups.iter().position(|x| *x == group).unwrap_or_else(|| {
            groups.push(group);
            groups.len() - 1
        });
        indices.push(g);
    }
    Ok(indices)
}
