use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::PowerThrough,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::PowerThrough,
        name: "Power Through",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 15,
        base_magic: 2,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 5,
        upgrade_magic: 0,
    }
}

pub fn power_through_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: crate::content::cards::mechanics::card_status_creation_on_play_v1(card.id)
                .expect("Power Through status creation rule")
                .action(state),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: evaluated.base_block_mut as i32
            },
            insertion_mode: AddTo::Bottom,
        }
    ]
}
