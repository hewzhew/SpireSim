use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Prepared,
        name: "Prepared",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::None,
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

pub fn prepared_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::DrawCards(evaluated.base_magic_num_mut.max(0) as u32),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::DiscardFromHand {
                amount: evaluated.base_magic_num_mut.max(0),
                random: false,
                end_turn: false,
            },
            insertion_mode: AddTo::Bottom,
        },
    ]
}
