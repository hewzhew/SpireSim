//! Reviewed combat-only upgrade scopes shared by card execution and reading.
//! Targets are resolved from the current piles when the queued effect runs.
use super::CardId;
use crate::runtime::action::Action;
use crate::state::{HandSelectFilter, HandSelectReason};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatUpgradeScopeV1 {
    SelectedCardInHand,
    WholeHand,
    /// Current hand, draw, discard and exhaust; not limbo or the master deck.
    AllCombatZones,
}

impl CombatUpgradeScopeV1 {
    /// Does not capture card identities: eligibility belongs to resolution.
    /// The caller still owns the order relative to its other effects.
    pub fn queued_action(self) -> Action {
        match self {
            Self::SelectedCardInHand => Action::SuspendForHandSelect {
                min: 1,
                max: 1,
                can_cancel: false,
                filter: HandSelectFilter::Upgradeable,
                reason: HandSelectReason::Upgrade,
            },
            Self::WholeHand => Action::UpgradeAllInHand,
            Self::AllCombatZones => Action::UpgradeAllCardsInCombat,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatUpgradePersistenceV1 {
    CombatOnly,
}

pub fn combat_upgrade_scope_for_card_v1(
    card: CardId,
    upgrades: u8,
) -> Option<CombatUpgradeScopeV1> {
    match card {
        CardId::Armaments if upgrades > 0 => Some(CombatUpgradeScopeV1::WholeHand),
        CardId::Armaments => Some(CombatUpgradeScopeV1::SelectedCardInHand),
        CardId::Apotheosis => Some(CombatUpgradeScopeV1::AllCombatZones),
        _ => None,
    }
}

pub fn combat_upgrade_persistence_for_card_v1(
    card: CardId,
    upgrades: u8,
) -> Option<CombatUpgradePersistenceV1> {
    combat_upgrade_scope_for_card_v1(card, upgrades).map(|_| CombatUpgradePersistenceV1::CombatOnly)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::core::tick_engine;
    use crate::runtime::combat::CombatCard;
    use crate::state::{ClientInput, EngineState};

    #[test]
    fn apotheosis_updates_current_four_piles_but_not_master_deck_or_limbo() {
        let mut state = crate::test_support::blank_test_combat();
        state
            .entities
            .monsters
            .push(crate::test_support::test_monster(
                crate::content::monsters::EnemyId::JawWorm,
            ));
        state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Apotheosis, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Wound, 3),
        ];
        state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Defend, 4)].into();
        state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Bash, 5)].into();
        let mut blow = CombatCard::for_scenario(CardId::SearingBlow, 6);
        blow.upgrades = 5;
        state.zones.exhaust_pile = vec![blow].into();
        state.zones.limbo = vec![CombatCard::for_scenario(CardId::Corruption, 7)];
        state.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Strike, 2)].into();
        let mut engine = EngineState::CombatPlayerTurn;
        assert!(tick_engine(
            &mut engine,
            &mut state,
            Some(ClientInput::PlayCard {
                card_index: 0,
                target: None,
            })
        ));
        for _ in 0..100 {
            if engine != EngineState::CombatProcessing {
                break;
            }
            assert!(tick_engine(&mut engine, &mut state, None));
        }
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert_eq!(
            state
                .zones
                .hand
                .iter()
                .map(|card| card.upgrades)
                .collect::<Vec<_>>(),
            [1, 0]
        );
        assert_eq!(state.zones.draw_pile[0].upgrades, 1);
        assert_eq!(state.zones.discard_pile[0].upgrades, 1);
        assert_eq!(state.zones.exhaust_pile[0].upgrades, 6);
        assert_eq!(state.zones.exhaust_pile[1].id, CardId::Apotheosis);
        assert_eq!(state.zones.exhaust_pile[1].upgrades, 0);
        assert_eq!(state.zones.limbo[0].upgrades, 0);
        assert_eq!(state.meta.master_deck_snapshot[0].upgrades, 0);
        state.add_card_to_hand(CombatCard::for_scenario(CardId::Defend, 8));
        assert_eq!(
            state.zones.hand.last().unwrap().upgrades,
            0,
            "the effect is not a persistent upgrade aura"
        );
    }
}
