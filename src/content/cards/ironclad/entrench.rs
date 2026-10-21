use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Double your current Block. Costs 2 Energy (1 if upgraded).";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Entrench,
        name: "Entrench",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn entrench_play(_state: &CombatState, _card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::DoubleBlock { target: 0 },
        insertion_mode: AddTo::Bottom,
    }]
}
