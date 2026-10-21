use crate::ai::analysis::card_semantics::{
    CombatEvent, DeckMechanicContext, InstalledRule, Mechanic, PayoffRequirement,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageMaturity {
    None,
    SourceOnly,
    PayoffOnly,
    Seeded,
    Supported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageStateReport {
    pub strength: PackageMaturity,
    pub exhaust: PackageMaturity,
    pub self_damage: PackageMaturity,
    pub block: PackageMaturity,
    pub open_requirements: Vec<PayoffRequirement>,
}

pub fn assess_package_state(ctx: &DeckMechanicContext) -> PackageStateReport {
    PackageStateReport {
        strength: assess_strength_package(ctx),
        exhaust: assess_exhaust_package(ctx),
        self_damage: assess_self_damage_package(ctx),
        block: assess_block_package(ctx),
        open_requirements: ctx.open_payoff_requirements.clone(),
    }
}

fn assess_strength_package(ctx: &DeckMechanicContext) -> PackageMaturity {
    let has_source = ctx.mechanics.contains(&Mechanic::Strength);
    let has_payoff = ctx
        .payoff_requirements
        .contains(&PayoffRequirement::WantsMechanic(Mechanic::Strength));
    maturity_from_source_and_payoff(has_source, has_payoff, false)
}

fn assess_exhaust_package(ctx: &DeckMechanicContext) -> PackageMaturity {
    let has_direct_stream = ctx.event_streams.contains(&CombatEvent::CardExhausted);
    let has_installed_seed = ctx
        .installed_rules
        .contains(&InstalledRule::SkillCardsCostZeroAndExhaust);
    let has_payoff = ctx
        .payoff_requirements
        .contains(&PayoffRequirement::WantsEventStream(
            CombatEvent::CardExhausted,
        ));
    maturity_from_source_and_payoff(has_direct_stream, has_payoff, has_installed_seed)
}

fn assess_self_damage_package(ctx: &DeckMechanicContext) -> PackageMaturity {
    let has_source = ctx.event_streams.contains(&CombatEvent::CardSelfDamage);
    let has_repeatable_source = ctx
        .repeatable_event_streams
        .contains(&CombatEvent::CardSelfDamage);
    let has_payoff = ctx
        .payoff_requirements
        .contains(&PayoffRequirement::WantsEventStream(
            CombatEvent::CardSelfDamage,
        ));

    match (has_source, has_repeatable_source, has_payoff) {
        (_, true, true) => PackageMaturity::Supported,
        (true, false, true) => PackageMaturity::Seeded,
        (true, _, false) => PackageMaturity::SourceOnly,
        (false, _, true) => PackageMaturity::PayoffOnly,
        (false, _, false) => PackageMaturity::None,
    }
}

fn assess_block_package(ctx: &DeckMechanicContext) -> PackageMaturity {
    let has_source = ctx.mechanics.contains(&Mechanic::Block);
    let has_payoff = ctx
        .payoff_requirements
        .contains(&PayoffRequirement::WantsMechanic(Mechanic::Block));
    maturity_from_source_and_payoff(has_source, has_payoff, false)
}

fn maturity_from_source_and_payoff(
    has_source: bool,
    has_payoff: bool,
    has_seed: bool,
) -> PackageMaturity {
    match (has_source, has_payoff, has_seed) {
        (true, true, _) => PackageMaturity::Supported,
        (false, true, true) => PackageMaturity::Seeded,
        (true, false, _) => PackageMaturity::SourceOnly,
        (false, true, false) => PackageMaturity::PayoffOnly,
        (false, false, true) => PackageMaturity::SourceOnly,
        (false, false, false) => PackageMaturity::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::analysis::card_semantics::card_definition;
    use crate::content::cards::CardId;

    fn state(cards: &[CardId]) -> PackageStateReport {
        let definitions = cards
            .iter()
            .copied()
            .map(card_definition)
            .collect::<Vec<_>>();
        assess_package_state(&DeckMechanicContext::from_definitions(&definitions))
    }

    #[test]
    fn limited_self_damage_with_rupture_is_seeded() {
        assert_eq!(
            state(&[CardId::Offering, CardId::Rupture]).self_damage,
            PackageMaturity::Seeded
        );
    }

    #[test]
    fn repeatable_self_damage_with_rupture_is_supported() {
        assert_eq!(
            state(&[CardId::Bloodletting, CardId::Rupture]).self_damage,
            PackageMaturity::Supported
        );
    }
}
