use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::AutoShields,
        name: "Auto-Shields",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 11,
        base_magic: 0,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 4,
        upgrade_magic: 0,
    }
}

pub fn auto_shields_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    if state.entities.player.block != 0 {
        return SmallVec::new();
    }
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::GainBlock {
            target: 0,
            amount: evaluated.base_block_mut,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
