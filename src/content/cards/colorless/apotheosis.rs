pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Apotheosis,
    };

use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Apotheosis,
        name: "Apotheosis",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::None,
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

pub fn apotheosis_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: crate::content::cards::mechanics::combat_upgrade_scope_for_card_v1(
            card.id,
            card.upgrades
        )
        .expect("Apotheosis has a reviewed combat upgrade scope")
        .queued_action(),
        insertion_mode: AddTo::Bottom,
    }]
}
