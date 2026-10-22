use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Violence,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Violence,
        name: "Violence",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn violence_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::DrawPileToHandByType {
            amount: evaluated.base_magic_num_mut.max(0) as u8,
            card_type: CardType::Attack,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
