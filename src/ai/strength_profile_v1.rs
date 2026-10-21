use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CombatEvent, DeckMechanicContext,
};
use crate::ai::card_semantics_v1::{
    card_mechanics_profile_v1, potion_mechanics_profile_v1, relic_mechanics_profile_v1,
    StrengthConversionMechanicV1,
};
use crate::content::cards::CardId;
use crate::runtime::combat::CombatCard;
use crate::state::run::RunState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrengthConversionRequirementV1 {
    CurrentStrength,
    SameTurnAccess,
    CorrectOrder,
    DebuffPreventionBeforeStrengthDown,
    UncontestedDebuffPreventionCapacity,
    AttackSkillPowerSameTurn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrengthConvertiblePotentialV1 {
    pub mechanic: StrengthConversionMechanicV1,
    pub count: u8,
    pub requirements: Vec<StrengthConversionRequirementV1>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StrengthProfileV1 {
    pub stable_sources: u8,
    pub temporary_bursts: u8,
    pub converters: u8,
    /// Finite or per-cycle Artifact sources that can prevent Strength Down.
    pub debuff_prevention_capacity: u8,
    /// Other modeled self-debuffs that can consume the same Artifact before
    /// a temporary-strength conversion becomes available.
    pub competing_debuff_prevention_demand: u8,
    pub convertible_potential_count: u8,
    pub payoffs: u8,
    pub potentials: Vec<StrengthConvertiblePotentialV1>,
    pub diagnosis: Vec<&'static str>,
}

pub fn strength_profile_v1(run_state: &RunState) -> StrengthProfileV1 {
    let mut profile = StrengthProfileV1::default();
    let mut rupture_count = 0u8;
    let mut has_amplifier = false;
    let mut debuff_prevention_capacity = 0u8;
    let mut has_debuff_cleanse = false;
    let definitions = run_state
        .master_deck
        .iter()
        .map(|card| card_definition_with_upgrades(card.id, card.upgrades))
        .collect::<Vec<_>>();
    let mechanic_context = DeckMechanicContext::from_definitions(&definitions);
    let has_repeatable_self_damage = mechanic_context
        .repeatable_event_streams
        .contains(&CombatEvent::CardSelfDamage);

    for relic in &run_state.relics {
        let mechanics = relic_mechanics_profile_v1(relic.id);
        if mechanics.persistent_strength_source {
            profile.stable_sources = profile.stable_sources.saturating_add(1);
        }
        if mechanics.temporary_strength_burst {
            profile.temporary_bursts = profile.temporary_bursts.saturating_add(1);
        }
        register_converter(
            &mut profile,
            mechanics.strength_converter,
            &mut has_amplifier,
            &mut debuff_prevention_capacity,
            &mut has_debuff_cleanse,
        );
    }

    for card in &run_state.master_deck {
        let mechanics = card_mechanics_profile_v1(card.id);
        if mechanics.persistent_strength_source {
            profile.stable_sources = profile.stable_sources.saturating_add(1);
        }
        if mechanics.temporary_strength_burst {
            profile.temporary_bursts = profile.temporary_bursts.saturating_add(1);
        }
        register_converter(
            &mut profile,
            mechanics.strength_converter,
            &mut has_amplifier,
            &mut debuff_prevention_capacity,
            &mut has_debuff_cleanse,
        );
        if mechanics.applies_no_draw_debuff {
            profile.competing_debuff_prevention_demand =
                profile.competing_debuff_prevention_demand.saturating_add(1);
        }
        if card.id == CardId::Rupture {
            rupture_count = rupture_count.saturating_add(1);
        }
        if mechanics.strength_payoff {
            profile.payoffs = profile.payoffs.saturating_add(1);
        }
    }

    for potion in run_state.potions.iter().flatten() {
        let mechanics = potion_mechanics_profile_v1(potion.id);
        if mechanics.temporary_strength_burst {
            profile.temporary_bursts = profile.temporary_bursts.saturating_add(1);
        }
        register_converter(
            &mut profile,
            mechanics.strength_converter,
            &mut has_amplifier,
            &mut debuff_prevention_capacity,
            &mut has_debuff_cleanse,
        );
    }
    profile.debuff_prevention_capacity = debuff_prevention_capacity;

    if rupture_count > 0 && has_repeatable_self_damage {
        profile.stable_sources = profile.stable_sources.saturating_add(rupture_count);
    }
    if profile.temporary_bursts > 0 {
        if has_amplifier {
            profile.potentials.push(StrengthConvertiblePotentialV1 {
                mechanic: StrengthConversionMechanicV1::AmplifyCurrentStrength,
                count: profile.temporary_bursts,
                requirements: vec![
                    StrengthConversionRequirementV1::CurrentStrength,
                    StrengthConversionRequirementV1::SameTurnAccess,
                    StrengthConversionRequirementV1::CorrectOrder,
                ],
            });
        }
        let unclaimed_debuff_prevention =
            debuff_prevention_capacity.saturating_sub(profile.competing_debuff_prevention_demand);
        if unclaimed_debuff_prevention > 0 {
            profile.potentials.push(StrengthConvertiblePotentialV1 {
                mechanic: StrengthConversionMechanicV1::PreventStrengthDownDebuff,
                count: profile.temporary_bursts.min(unclaimed_debuff_prevention),
                requirements: vec![
                    StrengthConversionRequirementV1::DebuffPreventionBeforeStrengthDown,
                    StrengthConversionRequirementV1::UncontestedDebuffPreventionCapacity,
                ],
            });
        } else if debuff_prevention_capacity > 0 {
            profile
                .diagnosis
                .push("debuff_prevention_capacity_claimed_by_other_self_debuffs");
        }
        if has_debuff_cleanse {
            profile.potentials.push(StrengthConvertiblePotentialV1 {
                mechanic: StrengthConversionMechanicV1::ClearStrengthDownDebuff,
                count: profile.temporary_bursts,
                requirements: vec![StrengthConversionRequirementV1::AttackSkillPowerSameTurn],
            });
        }
    }
    profile.convertible_potential_count = profile
        .potentials
        .iter()
        .map(|potential| potential.count)
        .max()
        .unwrap_or(0);

    if profile.payoffs > 0 && profile.stable_sources == 0 {
        if profile.convertible_potential_count > 0 {
            profile
                .diagnosis
                .push("payoff_has_convertible_strength_potential");
        } else if profile.temporary_bursts > 0 {
            profile
                .diagnosis
                .push("payoff_has_temporary_burst_not_stable_scaling");
        } else {
            profile.diagnosis.push("payoff_without_strength_source");
        }
    }
    if profile.temporary_bursts > 0 && profile.converters == 0 {
        profile
            .diagnosis
            .push("temporary_strength_has_no_converter");
    }

    profile
}

/// Whether adding this exact card closes a previously absent convertible
/// strength package that the current deck already has payoffs for.
///
/// This evaluates the same shared strength model before and after the
/// candidate. It therefore covers either missing half of the package
/// (temporary strength or a converter) without teaching reward owners card
/// names or treating temporary strength as stable scaling.
pub fn card_unlocks_convertible_strength_payoff_v1(
    run_state: &RunState,
    card: CardId,
    upgrades: u8,
) -> bool {
    let before = strength_profile_v1(run_state);
    if before.payoffs == 0 {
        return false;
    }

    let mut trial = run_state.clone();
    let lineage = trial.allocate_master_deck_lineage();
    let mut candidate = CombatCard::with_lineage(card, lineage);
    candidate.upgrades = upgrades;
    trial.master_deck.push(candidate);
    let after = strength_profile_v1(&trial);

    after.convertible_potential_count > before.convertible_potential_count
}

fn register_converter(
    profile: &mut StrengthProfileV1,
    converter: Option<StrengthConversionMechanicV1>,
    has_amplifier: &mut bool,
    debuff_prevention_capacity: &mut u8,
    has_debuff_cleanse: &mut bool,
) {
    let Some(converter) = converter else {
        return;
    };
    profile.converters = profile.converters.saturating_add(1);
    match converter {
        StrengthConversionMechanicV1::AmplifyCurrentStrength => *has_amplifier = true,
        StrengthConversionMechanicV1::PreventStrengthDownDebuff => {
            *debuff_prevention_capacity = debuff_prevention_capacity.saturating_add(1)
        }
        StrengthConversionMechanicV1::ClearStrengthDownDebuff => *has_debuff_cleanse = true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::{RelicId, RelicState};

    #[test]
    fn flex_alone_is_temporary_not_convertible() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::Flex);

        let profile = strength_profile_v1(&run_state);

        assert_eq!(profile.stable_sources, 0);
        assert_eq!(profile.temporary_bursts, 1);
        assert_eq!(profile.converters, 0);
        assert_eq!(profile.convertible_potential_count, 0);
        assert!(profile
            .diagnosis
            .contains(&"temporary_strength_has_no_converter"));
    }

    #[test]
    fn flex_with_limit_break_is_convertible_potential() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::Flex);
        run_state.add_card_to_deck(CardId::LimitBreak);

        let profile = strength_profile_v1(&run_state);

        assert_eq!(profile.stable_sources, 0);
        assert_eq!(profile.temporary_bursts, 1);
        assert_eq!(profile.converters, 1);
        assert_eq!(profile.convertible_potential_count, 1);
        assert!(profile.potentials.iter().any(|potential| {
            potential.mechanic == StrengthConversionMechanicV1::AmplifyCurrentStrength
                && potential
                    .requirements
                    .contains(&StrengthConversionRequirementV1::SameTurnAccess)
        }));
    }

    #[test]
    fn flex_with_debuff_prevention_is_convertible_potential() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::Flex);
        run_state.potions[0] = Some(Potion::new(PotionId::AncientPotion, 1));

        let profile = strength_profile_v1(&run_state);

        assert_eq!(profile.convertible_potential_count, 1);
        assert!(profile.potentials.iter().any(|potential| {
            potential.mechanic == StrengthConversionMechanicV1::PreventStrengthDownDebuff
                && potential
                    .requirements
                    .contains(&StrengthConversionRequirementV1::DebuffPreventionBeforeStrengthDown)
        }));
    }

    #[test]
    fn candidate_flex_closes_existing_artifact_and_payoff_package() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::SwordBoomerang);
        run_state
            .relics
            .push(RelicState::new(RelicId::ClockworkSouvenir));

        assert!(card_unlocks_convertible_strength_payoff_v1(
            &run_state,
            CardId::Flex,
            1
        ));

        let mut no_payoff = RunState::new(1, 0, false, "Ironclad");
        no_payoff
            .relics
            .push(RelicState::new(RelicId::ClockworkSouvenir));
        assert!(!card_unlocks_convertible_strength_payoff_v1(
            &no_payoff,
            CardId::Flex,
            1
        ));
        assert!(!card_unlocks_convertible_strength_payoff_v1(
            &run_state,
            CardId::Strike,
            0
        ));
    }

    #[test]
    fn competing_self_debuff_claims_single_artifact_before_candidate_flex() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::SwordBoomerang);
        run_state.add_card_to_deck(CardId::BattleTrance);
        run_state
            .relics
            .push(RelicState::new(RelicId::ClockworkSouvenir));

        assert!(!card_unlocks_convertible_strength_payoff_v1(
            &run_state,
            CardId::Flex,
            1
        ));

        run_state.potions[0] = Some(Potion::new(PotionId::AncientPotion, 2));
        assert!(card_unlocks_convertible_strength_payoff_v1(
            &run_state,
            CardId::Flex,
            1
        ));
        let mut with_flex = run_state;
        with_flex.add_card_to_deck(CardId::Flex);
        let profile = strength_profile_v1(&with_flex);
        assert_eq!(profile.debuff_prevention_capacity, 2);
        assert_eq!(profile.competing_debuff_prevention_demand, 1);
        assert!(profile.potentials.iter().any(|potential| {
            potential.mechanic == StrengthConversionMechanicV1::PreventStrengthDownDebuff
                && potential
                    .requirements
                    .contains(&StrengthConversionRequirementV1::UncontestedDebuffPreventionCapacity)
        }));
    }

    #[test]
    fn flex_with_orange_pellets_tracks_cleanse_requirement() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::Flex);
        run_state
            .relics
            .push(RelicState::new(RelicId::OrangePellets));

        let profile = strength_profile_v1(&run_state);

        assert_eq!(profile.convertible_potential_count, 1);
        assert!(profile.potentials.iter().any(|potential| {
            potential.mechanic == StrengthConversionMechanicV1::ClearStrengthDownDebuff
                && potential
                    .requirements
                    .contains(&StrengthConversionRequirementV1::AttackSkillPowerSameTurn)
        }));
    }

    #[test]
    fn jax_counts_as_persistent_strength_source() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::JAX);

        let profile = strength_profile_v1(&run_state);

        assert_eq!(profile.stable_sources, 1);
        assert_eq!(profile.temporary_bursts, 0);
    }

    #[test]
    fn offering_does_not_make_rupture_a_stable_strength_source() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::Offering);
        run_state.add_card_to_deck(CardId::Rupture);

        assert_eq!(strength_profile_v1(&run_state).stable_sources, 0);
    }

    #[test]
    fn repeatable_self_damage_makes_rupture_a_stable_strength_source() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::Bloodletting);
        run_state.add_card_to_deck(CardId::Rupture);

        assert_eq!(strength_profile_v1(&run_state).stable_sources, 1);
    }
}
