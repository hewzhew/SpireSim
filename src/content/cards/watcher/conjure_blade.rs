use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::ConjureBlade,
        name: "Conjure Blade",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: -1,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn conjure_blade_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let energy_on_use = if card.upgrades > 0 {
        card.energy_on_use + 1
    } else {
        card.energy_on_use
    };
    smallvec::smallvec![ActionInfo {
        action: Action::ConjureBlade {
            free_to_play_once: card.free_to_play_once,
            energy_on_use,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
