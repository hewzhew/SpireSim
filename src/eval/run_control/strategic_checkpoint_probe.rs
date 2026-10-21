use serde::Serialize;

use crate::state::run::RunState;

use super::strategic_encounter_probe::{
    run_strategic_encounter_probes_v1, StrategicEncounterProbeBudgetV1,
    StrategicEncounterProbeHpBasisV1, StrategicEncounterProbePotionUseV1,
    StrategicEncounterProbeReportV1, StrategicEncounterProbeSpecV1,
};

pub const STRATEGIC_CHECKPOINT_PROBE_SCHEMA_NAME: &str = "StrategicCheckpointProbeDecomposition";
pub const STRATEGIC_CHECKPOINT_PROBE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicCheckpointReferenceRelationV1 {
    /// The reference journal is an exact prefix of the observed journal.
    ExactJournalAncestor,
    /// The states are deliberately compared without a lineage claim.
    StateOnlyCounterfactual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicCheckpointProbeVariantKindV1 {
    Observed,
    FullHpOnly,
    PotionsFromReference,
    FullHpAndPotionsFromReference,
    DeckFromReference,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicCheckpointProbeStateSummaryV1 {
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub deck_size: usize,
    pub relic_count: usize,
    pub potion_count: usize,
    pub potion_ids: Vec<crate::content::potions::PotionId>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicCheckpointProbeVariantV1 {
    pub kind: StrategicCheckpointProbeVariantKindV1,
    pub controlled_changes: Vec<&'static str>,
    pub state: StrategicCheckpointProbeStateSummaryV1,
    pub probe: StrategicEncounterProbeReportV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicCheckpointProbeOmissionV1 {
    pub kind: StrategicCheckpointProbeVariantKindV1,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct StrategicCheckpointProbeDecompositionV1 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub evidence_policy: &'static str,
    pub observed: StrategicCheckpointProbeStateSummaryV1,
    pub reference: Option<StrategicCheckpointProbeStateSummaryV1>,
    pub reference_relation: Option<StrategicCheckpointReferenceRelationV1>,
    pub variants: Vec<StrategicCheckpointProbeVariantV1>,
    pub omitted_variants: Vec<StrategicCheckpointProbeOmissionV1>,
}

/// Runs controlled counterfactuals under the exact same encounter specs and
/// diagnostic RNG. The decomposition does not claim causal identification:
/// each variant changes only the fields named in `controlled_changes`, while
/// interactions between HP, potions, deck, relics, and search remain visible.
pub fn run_strategic_checkpoint_probe_decomposition_v1(
    observed: &RunState,
    reference: Option<&RunState>,
    reference_relation: Option<StrategicCheckpointReferenceRelationV1>,
    probes: &[StrategicEncounterProbeSpecV1],
    mut budget: StrategicEncounterProbeBudgetV1,
) -> Result<StrategicCheckpointProbeDecompositionV1, String> {
    match (reference, reference_relation) {
        (Some(reference), Some(_)) => validate_reference(observed, reference)?,
        (Some(_), None) => {
            return Err("checkpoint probe reference requires an explicit lineage relation".into())
        }
        (None, Some(_)) => {
            return Err("checkpoint probe lineage relation requires a reference state".into())
        }
        (None, None) => {}
    }
    // Variant construction owns HP normalization. Nested encounter reports
    // therefore always observe the state they receive without hidden edits.
    budget.hp_basis = StrategicEncounterProbeHpBasisV1::Current;

    let mut states = vec![(
        StrategicCheckpointProbeVariantKindV1::Observed,
        Vec::new(),
        observed.clone(),
    )];

    let mut omitted_variants = Vec::new();
    let hp_changes = observed.current_hp != observed.max_hp;
    if hp_changes {
        let mut full_hp = observed.clone();
        full_hp.current_hp = full_hp.max_hp;
        states.push((
            StrategicCheckpointProbeVariantKindV1::FullHpOnly,
            vec!["current_hp=max_hp"],
            full_hp,
        ));
    } else {
        omitted_variants.push(StrategicCheckpointProbeOmissionV1 {
            kind: StrategicCheckpointProbeVariantKindV1::FullHpOnly,
            reason: "observed_hp_already_full",
        });
    }

    if let Some(reference) = reference {
        let potions_change = observed.potions != reference.potions;
        if potions_change {
            if budget.potion_use == StrategicEncounterProbePotionUseV1::Disabled {
                return Err(
                    "potion counterfactual requires an enabled paired potion-use policy"
                        .to_string(),
                );
            }
            let mut restored_potions = observed.clone();
            restored_potions.potions = reference.potions.clone();
            states.push((
                StrategicCheckpointProbeVariantKindV1::PotionsFromReference,
                vec!["potions=reference.potions"],
                restored_potions,
            ));
        } else {
            omitted_variants.push(StrategicCheckpointProbeOmissionV1 {
                kind: StrategicCheckpointProbeVariantKindV1::PotionsFromReference,
                reason: "reference_potions_equal_observed_potions",
            });
        }

        if hp_changes && potions_change {
            let mut restored_hp_and_potions = observed.clone();
            restored_hp_and_potions.current_hp = restored_hp_and_potions.max_hp;
            restored_hp_and_potions.potions = reference.potions.clone();
            states.push((
                StrategicCheckpointProbeVariantKindV1::FullHpAndPotionsFromReference,
                vec!["current_hp=max_hp", "potions=reference.potions"],
                restored_hp_and_potions,
            ));
        } else {
            omitted_variants.push(StrategicCheckpointProbeOmissionV1 {
                kind: StrategicCheckpointProbeVariantKindV1::FullHpAndPotionsFromReference,
                reason: if !hp_changes {
                    "duplicates_potion_variant_because_observed_hp_is_full"
                } else {
                    "duplicates_full_hp_variant_because_reference_potions_are_unchanged"
                },
            });
        }

        if observed.master_deck != reference.master_deck {
            let mut reference_deck = observed.clone();
            reference_deck.master_deck = reference.master_deck.clone();
            states.push((
                StrategicCheckpointProbeVariantKindV1::DeckFromReference,
                vec!["master_deck=reference.master_deck"],
                reference_deck,
            ));
        } else {
            omitted_variants.push(StrategicCheckpointProbeOmissionV1 {
                kind: StrategicCheckpointProbeVariantKindV1::DeckFromReference,
                reason: "reference_deck_equal_observed_deck",
            });
        }
    }

    let variants = states
        .into_iter()
        .map(
            |(kind, controlled_changes, state)| StrategicCheckpointProbeVariantV1 {
                kind,
                controlled_changes,
                state: summarize_state(&state),
                probe: run_strategic_encounter_probes_v1(&state, probes, budget),
            },
        )
        .collect();

    Ok(StrategicCheckpointProbeDecompositionV1 {
        schema_name: STRATEGIC_CHECKPOINT_PROBE_SCHEMA_NAME,
        schema_version: STRATEGIC_CHECKPOINT_PROBE_SCHEMA_VERSION,
        evidence_policy: "paired_fixed_rng_controlled_fields_no_causal_or_successor_authority",
        observed: summarize_state(observed),
        reference: reference.map(summarize_state),
        reference_relation,
        variants,
        omitted_variants,
    })
}

fn validate_reference(observed: &RunState, reference: &RunState) -> Result<(), String> {
    if observed.player_class != reference.player_class {
        return Err(format!(
            "checkpoint probe class mismatch: observed {} reference {}",
            observed.player_class, reference.player_class
        ));
    }
    if observed.ascension_level != reference.ascension_level {
        return Err(format!(
            "checkpoint probe ascension mismatch: observed A{} reference A{}",
            observed.ascension_level, reference.ascension_level
        ));
    }
    Ok(())
}

fn summarize_state(run_state: &RunState) -> StrategicCheckpointProbeStateSummaryV1 {
    StrategicCheckpointProbeStateSummaryV1 {
        act: run_state.act_num,
        floor: run_state.floor_num,
        current_hp: run_state.current_hp,
        max_hp: run_state.max_hp,
        deck_size: run_state.master_deck.len(),
        relic_count: run_state.relics.len(),
        potion_count: run_state
            .potions
            .iter()
            .filter(|potion| potion.is_some())
            .count(),
        potion_ids: run_state
            .potions
            .iter()
            .filter_map(|potion| potion.as_ref().map(|potion| potion.id))
            .collect(),
    }
}
