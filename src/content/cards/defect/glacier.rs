use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState, OrbId};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Glacier,
        name: "Glacier",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 0,
        base_block: 7,
        base_magic: 2,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 3,
        upgrade_magic: 0,
    }
}

pub fn glacier_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let mut actions = smallvec::smallvec![ActionInfo {
        action: Action::GainBlock {
            target: 0,
            amount: evaluated.base_block_mut,
        },
        insertion_mode: AddTo::Bottom,
    }];
    for _ in 0..evaluated.base_magic_num_mut.max(0) {
        actions.push(ActionInfo {
            action: Action::ChannelOrb(OrbId::Frost),
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
