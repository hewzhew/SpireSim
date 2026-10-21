use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Whenever you gain Block, deal 5 damage (7 if upgraded) to a random enemy. Each separate Block gain triggers once, regardless of the amount gained.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Juggernaut,
        name: "Juggernaut",
        card_type: CardType::Power,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 5,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 2,
    }
}

pub fn juggernaut_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Juggernaut,
            amount: evaluated.base_magic_num_mut as i32,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
