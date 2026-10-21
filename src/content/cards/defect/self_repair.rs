use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::SelfRepair,
        name: "Self Repair",
        card_type: CardType::Power,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 7,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Healing],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 3,
    }
}

pub fn self_repair_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Repair,
            amount: evaluated.base_magic_num_mut,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
