use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::EscapePlan,
        name: "Escape Plan",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 3,
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

pub fn escape_plan_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::DrawCardsWithHistory {
                amount: 1,
                clear_history: true,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::EscapePlanBlockIfSkill {
                block: evaluated.base_block_mut,
            },
            insertion_mode: AddTo::Bottom,
        },
    ]
}
