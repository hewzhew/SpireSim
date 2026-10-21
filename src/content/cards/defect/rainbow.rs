use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState, OrbId};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Rainbow,
        name: "Rainbow",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::SelfTarget,
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

pub fn rainbow_play(_state: &CombatState, _card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![
        ActionInfo {
            action: Action::ChannelOrb(OrbId::Lightning),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::ChannelOrb(OrbId::Frost),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::ChannelOrb(OrbId::Dark),
            insertion_mode: AddTo::Bottom,
        },
    ]
}
