use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Havoc,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Havoc,
        name: "Havoc",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::None,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn havoc_play(
    _state: &CombatState,
    card: &CombatCard,
    _target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::PlayTopCard {
            target: None,
            exhaust: crate::content::cards::mechanics::card_top_deck_play_v1(card.id)
                .expect("Havoc top-deck play rule")
                .exhaust_after_play
        },
        insertion_mode: AddTo::Bottom,
    }]
}
