use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Hologram,
        name: "Hologram",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 0,
        base_block: 3,
        base_magic: 0,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 2,
        upgrade_magic: 0,
    }
}

pub fn hologram_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: evaluated.base_block_mut,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::SuspendForGridSelect {
                source_pile: crate::state::PileType::Discard,
                min: 1,
                max: 1,
                can_cancel: false,
                filter: crate::state::GridSelectFilter::Any,
                reason: crate::state::GridSelectReason::DiscardToHandNoCostChange,
            },
            insertion_mode: AddTo::Bottom,
        },
    ]
}
