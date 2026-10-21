use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Move 3 randomly chosen Attack cards from your draw pile to your hand (4 if upgraded), or all eligible cards if fewer remain. This moves cards rather than performing ordinary draws and does not reshuffle an empty draw pile. Cards that exceed the 10-card hand limit go to discard. Costs 0 Energy. Exhaust.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Violence,
        name: "Violence",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 0,
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
        upgrade_magic: 1,
    }
}

pub fn violence_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::DrawPileToHandByType {
            amount: evaluated.base_magic_num_mut.max(0) as u8,
            card_type: CardType::Attack,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
