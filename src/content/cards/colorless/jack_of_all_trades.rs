use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::JackOfAllTrades,
        name: "Jack of All Trades",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn jack_of_all_trades_play(
    state: &CombatState,
    card: &CombatCard,
) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let mut actions = SmallVec::new();
    for _ in 0..evaluated.base_magic_num_mut.max(1) {
        actions.push(ActionInfo {
            action: Action::MakeRandomColorlessCardInHand {
                cost_for_turn: None,
                upgraded: false,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
