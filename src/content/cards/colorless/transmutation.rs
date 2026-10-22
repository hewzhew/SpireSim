use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Transmutation,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Transmutation,
        name: "Transmutation",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: -1,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::SelfTarget,
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

pub fn transmutation_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::Transmutation {
            upgraded: card.upgrades > 0,
            free_to_play_once: card.free_to_play_once,
            energy_on_use: card.energy_on_use.max(state.turn.energy as i32),
        },
        insertion_mode: AddTo::Bottom,
    }]
}
