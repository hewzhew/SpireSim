use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Chrysalis,
        name: "Chrysalis",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 2,
    }
}

pub fn chrysalis_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let mut actions = SmallVec::new();
    for _ in 0..evaluated.base_magic_num_mut.max(0) {
        actions.push(ActionInfo {
            action: Action::MakeRandomCardInDrawPile {
                card_type: Some(CardType::Skill),
                cost_for_turn: Some(0),
                random_spot: true,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
