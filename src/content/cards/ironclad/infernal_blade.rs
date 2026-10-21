use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const RANDOM_ATTACK_TO_HAND: super::super::mechanics::CardRandomAttackToHandV1 =
    super::super::mechanics::CardRandomAttackToHandV1 {
        count: 1,
        cost_for_turn: 0,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::InfernalBlade,
        name: "Infernal Blade",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
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

pub fn infernal_blade_play(_state: &CombatState, _card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();

    actions.push(ActionInfo {
        action: Action::MakeRandomCardInHand {
            card_type: Some(crate::content::cards::CardType::Attack),
            cost_for_turn: Some(RANDOM_ATTACK_TO_HAND.cost_for_turn),
        },
        insertion_mode: AddTo::Bottom,
    });

    actions
}
