use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo};
use crate::runtime::combat::{CombatCard, CombatState};

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Nightmare,
        name: "Nightmare",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 3,
        base_damage: 0,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::None,
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

pub fn nightmare_play(
    state: &CombatState,
    card: &CombatCard,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::Nightmare {
            amount: evaluated.base_magic_num_mut.max(0).min(u8::MAX as i32) as u8,
        },
        insertion_mode: crate::runtime::action::AddTo::Bottom,
    }]
}
