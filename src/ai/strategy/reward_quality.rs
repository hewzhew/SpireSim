use crate::ai::analysis::card_semantics::{CardBurden, DuplicateBehavior, Mechanic, PlayEffect};
use crate::ai::deck_shape_v1::is_status_digest_card;
use crate::ai::strategy::package_transition::{PackageKind, PackageTransitionReport};
use crate::content::cards::CardId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RewardDuplicateConcern {
    LowMarginalFrontload,
    RedundantDebuff,
    RedundantCombatUpgrade,
    DiminishingAccessCopy,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RewardQualityReport {
    pub thin_payoff_support: Vec<Mechanic>,
    pub duplicate_burdens: Vec<CardBurden>,
    pub duplicate_concerns: Vec<RewardDuplicateConcern>,
}

pub fn assess_reward_quality(
    deck: &[CardId],
    candidate: CardId,
    transition: &PackageTransitionReport,
) -> RewardQualityReport {
    let mut report = RewardQualityReport::default();
    if uses_thin_block_payoff(candidate, transition) && support_units(deck, Mechanic::Block) < 3 {
        report.thin_payoff_support.push(Mechanic::Block);
    }
    if uses_thin_strength_payoff(candidate, transition)
        && support_units(deck, Mechanic::Strength) < 2
    {
        report.thin_payoff_support.push(Mechanic::Strength);
    }
    if has_clutter_burden(transition)
        && has_existing_clutter_burden(deck, candidate)
        && !has_status_digest(deck)
    {
        report
            .duplicate_burdens
            .push(CardBurden::AddsCombatDeckClutter);
    }
    if same_card_count(deck, candidate) > 0 {
        if is_low_marginal_frontload(candidate) {
            report
                .duplicate_concerns
                .push(RewardDuplicateConcern::LowMarginalFrontload);
        }
        if is_debuff_frontload(candidate) {
            report
                .duplicate_concerns
                .push(RewardDuplicateConcern::RedundantDebuff);
        }
        if has_combat_upgrade_effect(transition) {
            report
                .duplicate_concerns
                .push(RewardDuplicateConcern::RedundantCombatUpgrade);
        }
        if has_duplicate_behavior(transition, DuplicateBehavior::DiminishingReturn)
            && has_duplicate_behavior(transition, DuplicateBehavior::AccessCopyUseful)
        {
            report
                .duplicate_concerns
                .push(RewardDuplicateConcern::DiminishingAccessCopy);
        }
    }
    report
}

impl RewardQualityReport {
    pub fn suppresses_support(&self, package: PackageKind) -> bool {
        match package {
            PackageKind::Block => self.thin_payoff_support.contains(&Mechanic::Block),
            PackageKind::Strength => self.thin_payoff_support.contains(&Mechanic::Strength),
            PackageKind::Exhaust | PackageKind::SelfDamage => false,
        }
    }

    pub fn suppresses_payoff_effect(&self, effect: &PlayEffect) -> bool {
        matches!(effect, PlayEffect::DamageUses(mechanic) if self.thin_payoff_support.contains(mechanic))
    }

    pub fn has_duplicate_burden(&self) -> bool {
        !self.duplicate_burdens.is_empty()
    }

    pub fn has_duplicate_penalty(&self) -> bool {
        !self.duplicate_burdens.is_empty()
            || self
                .duplicate_concerns
                .iter()
                .any(|concern| concern.is_hard_penalty())
    }
}

impl RewardDuplicateConcern {
    pub fn is_hard_penalty(self) -> bool {
        !matches!(self, RewardDuplicateConcern::DiminishingAccessCopy)
    }
}

fn uses_thin_block_payoff(candidate: CardId, transition: &PackageTransitionReport) -> bool {
    candidate == CardId::Entrench || uses_mechanic(transition, Mechanic::Block)
}

fn uses_thin_strength_payoff(candidate: CardId, transition: &PackageTransitionReport) -> bool {
    matches!(candidate, CardId::LimitBreak) || uses_mechanic(transition, Mechanic::Strength)
}

fn uses_mechanic(transition: &PackageTransitionReport, mechanic: Mechanic) -> bool {
    transition
        .candidate_play_effects
        .contains(&PlayEffect::DamageUses(mechanic))
}

fn has_clutter_burden(transition: &PackageTransitionReport) -> bool {
    transition
        .candidate_burdens
        .contains(&CardBurden::AddsCombatDeckClutter)
}

fn has_existing_clutter_burden(deck: &[CardId], candidate: CardId) -> bool {
    card_adds_clutter(candidate) && deck.iter().copied().any(card_adds_clutter)
}

fn has_status_digest(deck: &[CardId]) -> bool {
    deck.iter().copied().any(is_status_digest_card)
}

fn support_units(deck: &[CardId], mechanic: Mechanic) -> u8 {
    deck.iter()
        .copied()
        .map(|card| support_units_for_card(card, mechanic))
        .sum()
}

fn support_units_for_card(card: CardId, mechanic: Mechanic) -> u8 {
    match (mechanic, card) {
        (Mechanic::Block, CardId::FlameBarrier | CardId::Impervious | CardId::PowerThrough) => 2,
        (
            Mechanic::Block,
            CardId::ShrugItOff | CardId::TrueGrit | CardId::SecondWind | CardId::IronWave,
        ) => 1,
        (Mechanic::Strength, CardId::Inflame | CardId::DemonForm) => 3,
        (Mechanic::Strength, CardId::SpotWeakness) => 2,
        (Mechanic::Strength, CardId::Rupture) => 1,
        _ => 0,
    }
}

fn card_adds_clutter(card: CardId) -> bool {
    matches!(
        card,
        CardId::WildStrike | CardId::RecklessCharge | CardId::PowerThrough
    )
}

fn same_card_count(deck: &[CardId], candidate: CardId) -> usize {
    deck.iter().filter(|card| **card == candidate).count()
}

fn is_low_marginal_frontload(card: CardId) -> bool {
    matches!(
        card,
        CardId::Carnage | CardId::Rampage | CardId::SwiftStrike | CardId::TwinStrike
    )
}

fn is_debuff_frontload(card: CardId) -> bool {
    matches!(
        card,
        CardId::Clothesline | CardId::Uppercut | CardId::ThunderClap
    )
}

fn has_combat_upgrade_effect(transition: &PackageTransitionReport) -> bool {
    transition.candidate_play_effects.iter().any(|effect| {
        matches!(
            effect,
            PlayEffect::CombatUpgradeSingle | PlayEffect::CombatUpgradeAll
        )
    })
}

fn has_duplicate_behavior(
    transition: &PackageTransitionReport,
    behavior: DuplicateBehavior,
) -> bool {
    transition.candidate_duplicate_behaviors.contains(&behavior)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::analysis::card_semantics::card_definition;
    use crate::ai::strategy::package_transition::assess_package_transition;

    fn quality(deck: &[CardId], candidate: CardId) -> RewardQualityReport {
        let definitions = deck
            .iter()
            .copied()
            .map(card_definition)
            .collect::<Vec<_>>();
        let transition = assess_package_transition(&definitions, card_definition(candidate));
        assess_reward_quality(deck, candidate, &transition)
    }

    #[test]
    fn repeated_status_generator_is_a_duplicate_burden_without_digest() {
        assert!(quality(&[CardId::WildStrike], CardId::WildStrike).has_duplicate_burden());
    }

    #[test]
    fn status_digest_prevents_a_global_duplicate_clutter_ban() {
        for digest in [
            CardId::Evolve,
            CardId::TrueGrit,
            CardId::SecondWind,
            CardId::FireBreathing,
        ] {
            assert!(
                !quality(&[CardId::WildStrike, digest], CardId::WildStrike)
                    .has_duplicate_burden(),
                "{digest:?} should make repeated status generation contextual rather than an unconditional duplicate rejection"
            );
        }
    }
}
