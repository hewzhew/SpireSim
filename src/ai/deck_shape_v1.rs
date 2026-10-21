use crate::ai::card_semantics_v1::{
    CardRewardStatusDestinationV1, CardRewardStatusInjectionV1, CardRewardStatusPersistenceV1,
};
use crate::content::cards::{get_card_definition, CardId, CardType};
use crate::content::relics::RelicId;
use crate::state::run::RunState;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeckShapeProfileV1 {
    pub exhaust_enabler_count: u8,
    pub exhaust_payoff_count: u8,
    pub status_generator_count: u8,
    pub status_digest_count: u8,
    pub corruption_count: u8,
    pub havoc_count: u8,
    pub wild_strike_count: u8,
    pub clash_count: u8,
    pub curse_count: u8,
    pub non_attack_count: u8,
    pub risks: Vec<DeckShapeRiskV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DeckShapeRiskV1 {
    NonstackingPowerDuplicateWithoutPayoff { card: CardId, copies: u8 },
    RandomExhaustSaturationWithoutPayoff { card: CardId, copies: u8 },
    StatusGeneratorSaturationWithoutDigest { copies: u8 },
    ClashPlayabilityDebt { copies: u8 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeckShapeCandidateDeltaV1 {
    pub candidate: CardId,
    pub risks: Vec<DeckShapeRiskV1>,
    pub labels: Vec<&'static str>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistentDrawPileStatusHandlingV1 {
    Unsupported,
    Conditional,
    Covered,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentDrawPileStatusAssessmentV1 {
    pub injections: Vec<CardRewardStatusInjectionV1>,
    pub handling: PersistentDrawPileStatusHandlingV1,
    pub draw_recovery_count: u8,
    pub unrestricted_handling_count: u8,
    pub conditional_hand_exhaust_count: u8,
    pub status_payoff_count: u8,
    pub exhaust_payoff_count: u8,
}

impl DeckShapeCandidateDeltaV1 {
    pub fn has_blocking_risk(&self) -> bool {
        !self.risks.is_empty()
    }
}

pub fn deck_shape_profile_v1(run_state: &RunState) -> DeckShapeProfileV1 {
    let mut profile = DeckShapeProfileV1::default();

    for relic in &run_state.relics {
        match relic.id {
            RelicId::DeadBranch | RelicId::CharonsAshes => {
                profile.exhaust_payoff_count = profile.exhaust_payoff_count.saturating_add(1);
            }
            RelicId::MedicalKit => {
                profile.exhaust_enabler_count = profile.exhaust_enabler_count.saturating_add(1);
                profile.status_digest_count = profile.status_digest_count.saturating_add(1);
            }
            _ => {}
        }
    }

    for card in &run_state.master_deck {
        let id = card.id;
        let definition = get_card_definition(id);
        if definition.card_type != CardType::Attack {
            profile.non_attack_count = profile.non_attack_count.saturating_add(1);
        }
        if definition.card_type == CardType::Curse {
            profile.curse_count = profile.curse_count.saturating_add(1);
        }

        if is_exhaust_enabler_card(id) {
            profile.exhaust_enabler_count = profile.exhaust_enabler_count.saturating_add(1);
        }
        if is_exhaust_payoff_card(id) {
            profile.exhaust_payoff_count = profile.exhaust_payoff_count.saturating_add(1);
        }
        if is_status_generator_card(id) {
            profile.status_generator_count = profile.status_generator_count.saturating_add(1);
        }
        if is_status_digest_card(id) {
            profile.status_digest_count = profile.status_digest_count.saturating_add(1);
        }

        match id {
            CardId::Corruption => {
                profile.corruption_count = profile.corruption_count.saturating_add(1)
            }
            CardId::Havoc => profile.havoc_count = profile.havoc_count.saturating_add(1),
            CardId::WildStrike => {
                profile.wild_strike_count = profile.wild_strike_count.saturating_add(1)
            }
            CardId::Clash => profile.clash_count = profile.clash_count.saturating_add(1),
            _ => {}
        }
    }

    if profile.corruption_count >= 2 && profile.exhaust_payoff_count == 0 {
        profile
            .risks
            .push(DeckShapeRiskV1::NonstackingPowerDuplicateWithoutPayoff {
                card: CardId::Corruption,
                copies: profile.corruption_count,
            });
    }
    if profile.havoc_count >= 2 && profile.exhaust_payoff_count == 0 {
        profile
            .risks
            .push(DeckShapeRiskV1::RandomExhaustSaturationWithoutPayoff {
                card: CardId::Havoc,
                copies: profile.havoc_count,
            });
    }
    if profile.status_generator_count >= 2 && profile.status_digest_count == 0 {
        profile
            .risks
            .push(DeckShapeRiskV1::StatusGeneratorSaturationWithoutDigest {
                copies: profile.status_generator_count,
            });
    }
    if profile.clash_count > 0 && clash_debt_from_profile(&profile) {
        profile.risks.push(DeckShapeRiskV1::ClashPlayabilityDebt {
            copies: profile.clash_count,
        });
    }

    profile
}

pub fn deck_shape_candidate_delta_v1(
    profile: &DeckShapeProfileV1,
    candidate: CardId,
) -> DeckShapeCandidateDeltaV1 {
    let mut delta = DeckShapeCandidateDeltaV1 {
        candidate,
        risks: Vec::new(),
        labels: Vec::new(),
    };

    match candidate {
        CardId::Corruption
            if profile.corruption_count >= 1 && profile.exhaust_payoff_count == 0 =>
        {
            delta
                .labels
                .push("deck_shape_nonstacking_power_duplicate_without_payoff");
            delta
                .risks
                .push(DeckShapeRiskV1::NonstackingPowerDuplicateWithoutPayoff {
                    card: CardId::Corruption,
                    copies: profile.corruption_count.saturating_add(1),
                });
        }
        CardId::Havoc if profile.havoc_count >= 1 && profile.exhaust_payoff_count == 0 => {
            delta
                .labels
                .push("deck_shape_random_exhaust_saturation_without_payoff");
            delta
                .risks
                .push(DeckShapeRiskV1::RandomExhaustSaturationWithoutPayoff {
                    card: CardId::Havoc,
                    copies: profile.havoc_count.saturating_add(1),
                });
        }
        CardId::WildStrike | CardId::RecklessCharge | CardId::PowerThrough | CardId::Immolate
            if profile.status_generator_count >= 1 && profile.status_digest_count == 0 =>
        {
            delta
                .labels
                .push("deck_shape_status_generator_duplicate_without_digest");
            delta
                .risks
                .push(DeckShapeRiskV1::StatusGeneratorSaturationWithoutDigest {
                    copies: profile.status_generator_count.saturating_add(1),
                });
        }
        CardId::Clash if clash_debt_after_candidate(profile) => {
            delta.labels.push("deck_shape_clash_playability_debt");
            delta.risks.push(DeckShapeRiskV1::ClashPlayabilityDebt {
                copies: profile.clash_count.saturating_add(1),
            });
        }
        _ => {}
    }

    delta
}

pub fn persistent_draw_pile_status_assessment_v1(
    run_state: &RunState,
    status_injections: &[CardRewardStatusInjectionV1],
) -> Option<PersistentDrawPileStatusAssessmentV1> {
    let injections = status_injections
        .iter()
        .copied()
        .filter(|injection| {
            injection.destination == CardRewardStatusDestinationV1::DrawPile
                && injection.persistence == CardRewardStatusPersistenceV1::Persistent
        })
        .collect::<Vec<_>>();
    if injections.is_empty() {
        return None;
    }

    let mut draw_recovery_count = 0_u8;
    let unrestricted_handling_count = run_state
        .relics
        .iter()
        .filter(|relic| relic.id == RelicId::MedicalKit)
        .count()
        .min(u8::MAX as usize) as u8;
    let mut conditional_hand_exhaust_count = 0_u8;
    let mut status_payoff_count = 0_u8;
    let mut exhaust_payoff_count = run_state
        .relics
        .iter()
        .filter(|relic| matches!(relic.id, RelicId::DeadBranch | RelicId::CharonsAshes))
        .count()
        .min(u8::MAX as usize) as u8;

    for card in &run_state.master_deck {
        match card.id {
            CardId::Evolve => draw_recovery_count = draw_recovery_count.saturating_add(1),
            CardId::FireBreathing => status_payoff_count = status_payoff_count.saturating_add(1),
            card if is_conditional_status_hand_exhaust_card(card) => {
                conditional_hand_exhaust_count = conditional_hand_exhaust_count.saturating_add(1);
            }
            card if is_exhaust_payoff_card(card) => {
                exhaust_payoff_count = exhaust_payoff_count.saturating_add(1);
            }
            _ => {}
        }
    }

    let handling = if unrestricted_handling_count > 0 || draw_recovery_count > 0 {
        PersistentDrawPileStatusHandlingV1::Covered
    } else if conditional_hand_exhaust_count > 0 || status_payoff_count > 0 {
        PersistentDrawPileStatusHandlingV1::Conditional
    } else {
        PersistentDrawPileStatusHandlingV1::Unsupported
    };

    Some(PersistentDrawPileStatusAssessmentV1 {
        injections,
        handling,
        draw_recovery_count,
        unrestricted_handling_count,
        conditional_hand_exhaust_count,
        status_payoff_count,
        exhaust_payoff_count,
    })
}

pub fn is_exhaust_enabler_card(card: CardId) -> bool {
    matches!(
        card,
        CardId::Corruption
            | CardId::BurningPact
            | CardId::TrueGrit
            | CardId::SecondWind
            | CardId::FiendFire
            | CardId::SeverSoul
            | CardId::Havoc
    )
}

pub fn is_exhaust_payoff_card(card: CardId) -> bool {
    matches!(card, CardId::FeelNoPain | CardId::DarkEmbrace)
}

fn is_conditional_status_hand_exhaust_card(card: CardId) -> bool {
    matches!(
        card,
        CardId::BurningPact
            | CardId::TrueGrit
            | CardId::SecondWind
            | CardId::FiendFire
            | CardId::SeverSoul
    )
}

pub fn is_status_generator_card(card: CardId) -> bool {
    matches!(
        card,
        CardId::WildStrike | CardId::RecklessCharge | CardId::PowerThrough | CardId::Immolate
    )
}

pub fn is_status_digest_card(card: CardId) -> bool {
    matches!(
        card,
        CardId::Evolve
            | CardId::FireBreathing
            | CardId::BurningPact
            | CardId::TrueGrit
            | CardId::SecondWind
            | CardId::FiendFire
            | CardId::SeverSoul
    )
}

fn clash_debt_from_profile(profile: &DeckShapeProfileV1) -> bool {
    profile.curse_count > 0 || profile.non_attack_count >= 8 || profile.status_generator_count > 0
}

fn clash_debt_after_candidate(profile: &DeckShapeProfileV1) -> bool {
    profile.curse_count > 0
        || profile.non_attack_count.saturating_add(1) >= 8
        || profile.status_generator_count > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::card_semantics_v1::card_reward_facts_v1;
    use crate::content::relics::RelicState;
    use crate::runtime::combat::CombatCard;
    use crate::state::rewards::RewardCard;

    fn assessment(
        run_state: &RunState,
        candidate: CardId,
    ) -> Option<PersistentDrawPileStatusAssessmentV1> {
        let facts = card_reward_facts_v1(&RewardCard::new(candidate, 0));
        persistent_draw_pile_status_assessment_v1(run_state, &facts.status_injections)
    }

    #[test]
    fn persistent_draw_pile_status_without_handling_is_unsupported() {
        let run_state = RunState::new(1, 0, false, "Ironclad");

        let assessment = assessment(&run_state, CardId::WildStrike)
            .expect("Wild Strike injects a persistent draw-pile status");

        assert_eq!(
            assessment.handling,
            PersistentDrawPileStatusHandlingV1::Unsupported
        );
        assert_eq!(assessment.injections[0].card, CardId::Wound);
    }

    #[test]
    fn hand_exhaust_is_conditional_but_exhaust_payoff_alone_is_not_handling() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::SecondWind, 1),
            CombatCard::for_scenario(CardId::DarkEmbrace, 2),
        ];

        let conditional = assessment(&run_state, CardId::WildStrike)
            .expect("Wild Strike injects a persistent draw-pile status");

        assert_eq!(
            conditional.handling,
            PersistentDrawPileStatusHandlingV1::Conditional
        );
        assert_eq!(conditional.conditional_hand_exhaust_count, 1);
        assert_eq!(conditional.exhaust_payoff_count, 1);

        run_state.master_deck = vec![CombatCard::for_scenario(CardId::DarkEmbrace, 2)];
        assert_eq!(
            assessment(&run_state, CardId::WildStrike)
                .expect("Wild Strike assessment")
                .handling,
            PersistentDrawPileStatusHandlingV1::Unsupported
        );
    }

    #[test]
    fn draw_recovery_or_unrestricted_handling_covers_persistent_status() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![CombatCard::for_scenario(CardId::Evolve, 1)];
        assert_eq!(
            assessment(&run_state, CardId::WildStrike)
                .expect("Wild Strike assessment")
                .handling,
            PersistentDrawPileStatusHandlingV1::Covered
        );

        run_state.master_deck.clear();
        run_state.relics.push(RelicState::new(RelicId::MedicalKit));
        let assessment =
            assessment(&run_state, CardId::WildStrike).expect("Wild Strike assessment");
        assert_eq!(
            assessment.handling,
            PersistentDrawPileStatusHandlingV1::Covered
        );
        assert_eq!(assessment.unrestricted_handling_count, 1);
    }

    #[test]
    fn ethereal_draw_pile_status_does_not_create_persistent_assessment() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        assert_eq!(assessment(&run_state, CardId::RecklessCharge), None);
    }
}
