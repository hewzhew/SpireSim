use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Gain 7 Block (9 if upgraded), then Exhaust 1 other card from hand. The card is random when unupgraded; choose it when upgraded. Block follows current Dexterity and Frail modifiers.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::TrueGrit,
        name: "True Grit",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 0,
        base_block: 7,
        base_magic: 0,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 2,
        upgrade_magic: 0,
    }
}

pub fn true_grit_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let mut actions = smallvec::smallvec![ActionInfo {
        action: Action::GainBlock {
            target: 0,
            amount: evaluated.base_block_mut,
        },
        insertion_mode: AddTo::Bottom,
    }];

    if card.upgrades > 0 {
        actions.push(ActionInfo {
            action: Action::ExhaustFromHand {
                amount: 1,
                random: false,
                any_number: false,
                can_pick_zero: false,
            },
            insertion_mode: AddTo::Bottom,
        });
    } else {
        actions.push(ActionInfo {
            action: Action::ExhaustFromHand {
                amount: 1,
                random: true,
                any_number: false,
                can_pick_zero: false,
            },
            insertion_mode: AddTo::Bottom,
        });
    }

    actions
}
