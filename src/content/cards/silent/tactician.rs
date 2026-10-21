use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Tactician,
        name: "Tactician",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: -2,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
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

pub fn tactician_play(_state: &CombatState, _card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![]
}

pub fn tactician_manual_discard(card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    // Manual discard triggers outside normal play evaluation, so derive magic
    // from the static definition plus the concrete card's upgrade count.
    let def = crate::content::cards::get_card_definition(card.id);
    let upgraded = if card.upgrades > 0 { 1 } else { 0 };
    let magic = def.base_magic + upgraded * def.upgrade_magic;
    smallvec::smallvec![ActionInfo {
        action: Action::GainEnergy {
            amount: magic.max(0)
        },
        insertion_mode: AddTo::Top,
    }]
}
