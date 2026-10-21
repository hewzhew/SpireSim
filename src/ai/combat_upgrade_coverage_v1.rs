use crate::content::cards::CardId;
use crate::state::run::RunState;

pub use crate::content::cards::mechanics::{
    combat_upgrade_persistence_for_card_v1, combat_upgrade_scope_for_card_v1,
    CombatUpgradePersistenceV1, CombatUpgradeScopeV1,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatUpgradeSourceV1 {
    pub deck_index: usize,
    pub card: CardId,
    pub upgrades: u8,
    pub scope: CombatUpgradeScopeV1,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CombatUpgradeCoverageProfileV1 {
    pub sources: Vec<CombatUpgradeSourceV1>,
}

impl CombatUpgradeCoverageProfileV1 {
    pub fn source_count(&self, scope: CombatUpgradeScopeV1) -> u8 {
        self.sources
            .iter()
            .filter(|source| source.scope == scope)
            .count()
            .min(u8::MAX as usize) as u8
    }

    pub fn has_scope(&self, scope: CombatUpgradeScopeV1) -> bool {
        self.sources.iter().any(|source| source.scope == scope)
    }

    pub fn strongest_scope(&self) -> Option<CombatUpgradeScopeV1> {
        self.sources.iter().map(|source| source.scope).max()
    }
}

pub fn combat_upgrade_coverage_profile_v1(run_state: &RunState) -> CombatUpgradeCoverageProfileV1 {
    let sources = run_state
        .master_deck
        .iter()
        .enumerate()
        .filter_map(|(deck_index, card)| {
            let scope = combat_upgrade_scope_for_card_v1(card.id, card.upgrades)?;
            Some(CombatUpgradeSourceV1 {
                deck_index,
                card: card.id,
                upgrades: card.upgrades,
                scope,
            })
        })
        .collect();

    CombatUpgradeCoverageProfileV1 { sources }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::CombatCard;

    #[test]
    fn distinguishes_selected_hand_whole_hand_and_all_combat_zones() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        let armaments = CombatCard::for_scenario(CardId::Armaments, 1001);
        let mut armaments_plus = CombatCard::for_scenario(CardId::Armaments, 1002);
        armaments_plus.upgrades = 1;
        let apotheosis = CombatCard::for_scenario(CardId::Apotheosis, 1003);
        run.master_deck = vec![armaments, armaments_plus, apotheosis];

        let profile = combat_upgrade_coverage_profile_v1(&run);

        assert_eq!(
            profile
                .sources
                .iter()
                .map(|source| source.scope)
                .collect::<Vec<_>>(),
            vec![
                CombatUpgradeScopeV1::SelectedCardInHand,
                CombatUpgradeScopeV1::WholeHand,
                CombatUpgradeScopeV1::AllCombatZones,
            ]
        );
        assert_eq!(
            profile.source_count(CombatUpgradeScopeV1::SelectedCardInHand),
            1
        );
        assert_eq!(
            profile.strongest_scope(),
            Some(CombatUpgradeScopeV1::AllCombatZones)
        );
        assert_eq!(
            combat_upgrade_persistence_for_card_v1(CardId::Armaments, 0),
            Some(CombatUpgradePersistenceV1::CombatOnly)
        );
        assert_eq!(
            combat_upgrade_persistence_for_card_v1(CardId::Apotheosis, 0),
            Some(CombatUpgradePersistenceV1::CombatOnly)
        );
    }
}
