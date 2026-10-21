use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Halt,
        name: "Halt",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
        cost: 0,
        base_damage: 0,
        base_block: 3,
        base_magic: 9,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 1,
        upgrade_magic: 5,
    }
}

pub fn halt_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::Halt {
            block: evaluated.base_block_mut,
            additional: evaluated.base_magic_num_mut,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
