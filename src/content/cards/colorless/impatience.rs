use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Impatience,
        name: "Impatience",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 2,
        target: CardTarget::SelfTarget,
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

pub fn impatience_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let has_attack = state.zones.hand.iter().any(|card| {
        crate::content::cards::get_card_definition(card.id).card_type == CardType::Attack
    });
    if has_attack {
        return SmallVec::new();
    }
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::DrawCards(evaluated.base_magic_num_mut.max(0) as u32),
        insertion_mode: AddTo::Bottom,
    }]
}
