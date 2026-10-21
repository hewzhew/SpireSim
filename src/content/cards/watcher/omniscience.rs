use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Omniscience,
        name: "Omniscience",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 4,
        base_damage: 0,
        base_block: 0,
        base_magic: 2,
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

pub fn omniscience_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, _state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Draw,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::Any,
            reason: crate::state::GridSelectReason::Omniscience {
                play_amount: evaluated.base_magic_num_mut.max(0) as u8,
            },
        },
        insertion_mode: AddTo::Bottom,
    }]
}
