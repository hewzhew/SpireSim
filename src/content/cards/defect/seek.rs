use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Seek,
        name: "Seek",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::None,
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

pub fn seek_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let amount = evaluated.base_magic_num_mut.max(0) as usize;
    if amount == 0 {
        return SmallVec::new();
    }

    // BetterDrawPileToHandAction chooses between empty, auto-all and grid
    // when it resolves, after earlier hooks may have drawn or reshuffled.
    smallvec::smallvec![ActionInfo {
        action: Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Draw,
            min: amount.min(u8::MAX as usize) as u8,
            max: amount.min(u8::MAX as usize) as u8,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::Any,
            reason: crate::state::GridSelectReason::DrawPileToHand,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
