use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

use crate::content::cards::mechanics::card_end_of_turn_block_each_turn_v1;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Metallicize,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Metallicize,
        name: "Metallicize",
        card_type: CardType::Power,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn metallicize_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let recurring_block = card_end_of_turn_block_each_turn_v1(card.id, card.upgrades)
        .expect("Metallicize has recurring end-of-turn Block");
    smallvec::smallvec![ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Metallicize,
            amount: recurring_block.amount,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
