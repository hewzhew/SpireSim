use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::state::ChooseOneCardChoice;
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Wish,
        name: "Wish",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 3,
        base_damage: 3,
        base_block: 6,
        base_magic: 25,
        target: CardTarget::None,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Healing],
        upgrade_damage: 1,
        upgrade_block: 2,
        upgrade_magic: 5,
    }
}

pub fn wish_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let upgrades = if card.upgrades > 0 { 1 } else { 0 };
    smallvec::smallvec![ActionInfo {
        action: Action::SuspendForChooseOne {
            choices: vec![
                ChooseOneCardChoice {
                    card_id: CardId::BecomeAlmighty,
                    upgrades,
                },
                ChooseOneCardChoice {
                    card_id: CardId::FameAndFortune,
                    upgrades,
                },
                ChooseOneCardChoice {
                    card_id: CardId::LiveForever,
                    upgrades,
                },
            ],
        },
        insertion_mode: AddTo::Bottom,
    }]
}
