use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::BattleTrance,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::BattleTrance,
        name: "Battle Trance",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::None,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn battle_trance_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::DrawCards(evaluated.base_magic_num_mut as u32),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::NoDraw,
                amount: -1, // Java NoDrawPower uses a sentinel amount of -1
            },
            insertion_mode: AddTo::Bottom,
        }
    ]
}
