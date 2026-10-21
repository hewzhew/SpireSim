use crate::ai::analysis::card_semantics::{
    CardBurden, CardDefinition, CombatEvent, DeckMechanicContext, DuplicateBehavior, EventHandler,
    InstalledRule, Mechanic, PayoffRequirement, PlayEffect,
};
use crate::ai::strategy::package_state::{
    assess_package_state, PackageMaturity, PackageStateReport,
};
use crate::content::cards::CardId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageKind {
    Strength,
    Exhaust,
    SelfDamage,
    Block,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageStateChange {
    pub package: PackageKind,
    pub from: PackageMaturity,
    pub to: PackageMaturity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageTransitionReport {
    pub candidate: CardId,
    pub before: PackageStateReport,
    pub after: PackageStateReport,
    pub package_changes: Vec<PackageStateChange>,
    pub newly_closed_requirements: Vec<PayoffRequirement>,
    pub newly_open_requirements: Vec<PayoffRequirement>,
    pub new_mechanics: Vec<Mechanic>,
    pub new_event_streams: Vec<CombatEvent>,
    pub new_installed_rules: Vec<InstalledRule>,
    pub candidate_play_effects: Vec<PlayEffect>,
    pub candidate_installed_rules: Vec<InstalledRule>,
    pub candidate_event_handlers: Vec<EventHandler>,
    pub candidate_burdens: Vec<CardBurden>,
    pub candidate_duplicate_behaviors: Vec<DuplicateBehavior>,
}

pub fn assess_package_transition(
    deck: &[CardDefinition],
    candidate: CardDefinition,
) -> PackageTransitionReport {
    let before_context = DeckMechanicContext::from_definitions(deck);
    let before = assess_package_state(&before_context);

    let mut after_deck = deck.to_vec();
    after_deck.push(candidate.clone());
    let after_context = DeckMechanicContext::from_definitions(&after_deck);
    let after = assess_package_state(&after_context);

    PackageTransitionReport {
        candidate: candidate.card,
        package_changes: package_state_changes(&before, &after),
        newly_closed_requirements: removed_requirements(
            &before.open_requirements,
            &after.open_requirements,
        ),
        newly_open_requirements: added_requirements(
            &before.open_requirements,
            &after.open_requirements,
        ),
        new_mechanics: added_items(&before_context.mechanics, &after_context.mechanics),
        new_event_streams: added_items(&before_context.event_streams, &after_context.event_streams),
        new_installed_rules: added_items(
            &before_context.installed_rules,
            &after_context.installed_rules,
        ),
        candidate_play_effects: candidate.play_effects.clone(),
        candidate_installed_rules: candidate.installed_rules.clone(),
        candidate_event_handlers: candidate.event_handlers.clone(),
        candidate_burdens: candidate.burdens.clone(),
        candidate_duplicate_behaviors: candidate.duplicate_behaviors.clone(),
        before,
        after,
    }
}

fn package_state_changes(
    before: &PackageStateReport,
    after: &PackageStateReport,
) -> Vec<PackageStateChange> {
    let mut changes = Vec::new();
    push_change_if_different(
        &mut changes,
        PackageKind::Strength,
        before.strength,
        after.strength,
    );
    push_change_if_different(
        &mut changes,
        PackageKind::Exhaust,
        before.exhaust,
        after.exhaust,
    );
    push_change_if_different(
        &mut changes,
        PackageKind::SelfDamage,
        before.self_damage,
        after.self_damage,
    );
    push_change_if_different(&mut changes, PackageKind::Block, before.block, after.block);
    changes
}

fn push_change_if_different(
    changes: &mut Vec<PackageStateChange>,
    package: PackageKind,
    from: PackageMaturity,
    to: PackageMaturity,
) {
    if from != to {
        changes.push(PackageStateChange { package, from, to });
    }
}

fn removed_requirements(
    before: &[PayoffRequirement],
    after: &[PayoffRequirement],
) -> Vec<PayoffRequirement> {
    before
        .iter()
        .copied()
        .filter(|requirement| !after.contains(requirement))
        .collect()
}

fn added_requirements(
    before: &[PayoffRequirement],
    after: &[PayoffRequirement],
) -> Vec<PayoffRequirement> {
    added_items(before, after)
}

fn added_items<T: Copy + Eq>(before: &[T], after: &[T]) -> Vec<T> {
    after
        .iter()
        .copied()
        .filter(|item| !before.contains(item))
        .collect()
}
