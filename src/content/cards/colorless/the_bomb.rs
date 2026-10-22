use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::TheBomb,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::TheBomb,
        name: "The Bomb",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 40,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 10,
    }
}

pub fn the_bomb_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::ApplyPowerDetailed {
            source: 0,
            target: 0,
            power_id: PowerId::TheBombPower,
            amount: 3,
            instance_id: Some(card.lineage.compact_runtime_value()),
            extra_data: Some(evaluated.base_magic_num_mut),
        },
        insertion_mode: AddTo::Bottom,
    }]
}
