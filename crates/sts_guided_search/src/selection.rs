use std::collections::HashSet;

use sts_core::runtime::combat::CardLineageId;
use sts_core::sim::combat_action_surface::{
    CombatSelectionActionFamilyV2, CombatSelectionDomainCandidateV2,
    CombatSelectionInputEncodingV2, CombatSelectionStatusV2,
};
use sts_core::state::core::ClientInput;
use sts_core::state::selection::{SelectionResolution, SelectionScope};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum SelectionMember {
    Card(CardLineageId),
    Scry {
        index: usize,
        lineage: CardLineageId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectionDomain {
    pub members: Vec<SelectionMember>,
    pub min: usize,
    pub max: usize,
    encoding: CombatSelectionInputEncodingV2,
}

impl SelectionDomain {
    pub fn new(family: &CombatSelectionActionFamilyV2) -> Result<Self, String> {
        if family.selection_status != CombatSelectionStatusV2::Enabled {
            return Err("disabled selection family requires diagnosis".into());
        }
        let mut members = Vec::new();
        let mut identities = HashSet::new();
        let mut scry_indices = HashSet::new();
        for candidate in &family.raw_domain {
            let (member, lineage) = match (family.input_encoding, candidate) {
                (
                    CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages
                    | CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages,
                    CombatSelectionDomainCandidateV2::CardLineage {
                        lineage,
                        eligible: true,
                        ..
                    },
                ) => (SelectionMember::Card(*lineage), *lineage),
                (
                    _,
                    CombatSelectionDomainCandidateV2::CardLineage {
                        eligible: false, ..
                    },
                ) => continue,
                (
                    CombatSelectionInputEncodingV2::SubmitScryDiscardIndices,
                    CombatSelectionDomainCandidateV2::ScryIndex {
                        index,
                        card_lineage: Some(lineage),
                        currently_present: true,
                        ..
                    },
                ) => {
                    let index = usize::try_from(*index).map_err(|_| "scry index overflow")?;
                    if !scry_indices.insert(index) {
                        return Err("duplicate scry source".into());
                    }
                    (
                        SelectionMember::Scry {
                            index,
                            lineage: *lineage,
                        },
                        *lineage,
                    )
                }
                (
                    _,
                    CombatSelectionDomainCandidateV2::ScryIndex {
                        currently_present: false,
                        ..
                    },
                ) => continue,
                _ => return Err("selection encoding and source disagree".into()),
            };
            if !identities.insert(lineage) {
                return Err("duplicate selection lineage".into());
            }
            members.push(member);
        }
        let min = usize::try_from(family.declared_min).map_err(|_| "selection minimum overflow")?;
        let max =
            usize::try_from(family.effective_max).map_err(|_| "selection maximum overflow")?;
        if min > max || max > members.len() {
            return Err("inconsistent selection bounds".into());
        }
        Ok(Self {
            members,
            min,
            max,
            encoding: family.input_encoding,
        })
    }

    pub fn remaining_mass(&self, chosen: usize) -> Result<f64, String> {
        if chosen > self.max || chosen > self.members.len() {
            return Err("invalid selection prefix length".into());
        }
        completions(
            self.members.len() - chosen,
            self.min.saturating_sub(chosen),
            self.max - chosen,
        )
    }

    pub fn compile(&self, selected: &[usize]) -> Result<ClientInput, String> {
        if selected.len() < self.min
            || selected.len() > self.max
            || selected.iter().any(|i| *i >= self.members.len())
            || selected.iter().copied().collect::<HashSet<_>>().len() != selected.len()
        {
            return Err("invalid completed selection".into());
        }
        match self.encoding {
            CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages
            | CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages => {
                let scope = if self.encoding
                    == CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages
                {
                    SelectionScope::Hand
                } else {
                    SelectionScope::Grid
                };
                let lineages = selected
                    .iter()
                    .map(|i| match self.members[*i] {
                        SelectionMember::Card(lineage) => Ok(lineage),
                        _ => Err("invalid lineage payload".to_string()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ClientInput::SubmitSelection(
                    SelectionResolution::card_lineages(scope, lineages),
                ))
            }
            CombatSelectionInputEncodingV2::SubmitScryDiscardIndices => {
                let indices = selected
                    .iter()
                    .map(|i| match self.members[*i] {
                        SelectionMember::Scry { index, .. } => Ok(index),
                        _ => Err("invalid scry payload".to_string()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ClientInput::SubmitScryDiscard(indices))
            }
        }
    }

    /// Inverse of compile, bound to this exact domain. Preserve submitted order
    /// and instance identity; never enumerate permutations or match card names.
    pub fn resolve(&self, input: &ClientInput) -> Option<Vec<usize>> {
        let selected = match (self.encoding, input) {
            (
                CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages
                | CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages,
                ClientInput::SubmitSelection(resolution),
            ) => resolution
                .selected_card_lineages()
                .iter()
                .map(|lineage| {
                    self.members
                        .iter()
                        .position(|m| *m == SelectionMember::Card(*lineage))
                })
                .collect::<Option<Vec<_>>>()?,
            (
                CombatSelectionInputEncodingV2::SubmitScryDiscardIndices,
                ClientInput::SubmitScryDiscard(indices),
            ) => indices
                .iter()
                .map(|index| {
                    self.members.iter().position(|m| {
                        matches!(m, SelectionMember::Scry { index: source, .. } if source == index)
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        // Also validates scope, cardinality, duplicates and exact payload.
        (self.compile(&selected).ok().as_ref() == Some(input)).then_some(selected)
    }
}

/// Counts ordered completions, including the empty sequence where legal.
/// Floating overflow is a visible domain limit, never saturated probability.
fn completions(n: usize, min: usize, max: usize) -> Result<f64, String> {
    let mut mass = 0.0;
    let mut permutations = 1.0;
    for length in 0..=max {
        if length >= min {
            mass += permutations;
        }
        if length < max {
            permutations *= (n - length) as f64;
        }
        if !mass.is_finite() || !permutations.is_finite() {
            return Err("selection mass exceeds numeric range".into());
        }
    }
    Ok(mass)
}
